//! Capability indexing.
//!
//! After loading `noop_executor` + `recording_hook` + `failing_subcommand`,
//! the registry has the expected indices and we can dispatch through them.

#![allow(
    clippy::cargo_common_metadata,
    clippy::multiple_crate_versions,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

pub mod common;

use common::fixtures;
use harmont_cli::plugin::{PluginRegistry, RegistryConfig};
use hm_plugin_protocol::{
    ArchiveId, CacheDecision, CommandStep, ExecutorInput, SubcommandInput, StepResult,
};
use uuid::Uuid;

#[test]
fn loads_three_fixtures_and_builds_indices() {
    let reg = PluginRegistry::load(RegistryConfig {
        auto_discover: false,
        extra_paths: vec![
            fixtures::fixture_path("hm-fixture-noop-executor"),
            fixtures::fixture_path("hm-fixture-recording-hook"),
            fixtures::fixture_path("hm-fixture-failing-subcommand"),
        ],
        ..Default::default()
    })
    .expect("load");
    assert!(reg.capabilities.resolve_runner("noop").is_some());
    assert!(reg.capabilities.resolve_subcommand("fixture-fail").is_some());
    assert_eq!(reg.manifests().count(), 3);
}

#[tokio::test(flavor = "multi_thread")]
async fn dispatches_subcommand_with_nonzero_exit_info() {
    let reg = PluginRegistry::load(RegistryConfig {
        auto_discover: false,
        extra_paths: vec![fixtures::fixture_path("hm-fixture-failing-subcommand")],
        ..Default::default()
    })
    .unwrap();
    let idx = reg.capabilities.resolve_subcommand("fixture-fail").unwrap();
    let plugin = reg.get(idx).unwrap();
    let input = SubcommandInput {
        verb_path: vec!["fixture-fail".into()],
        args: serde_json::json!({}),
        env: std::collections::BTreeMap::new(),
    };
    let info = plugin
        .run_subcommand(&input)
        .await
        .unwrap();
    assert_eq!(info.exit_code, 7);
    assert_eq!(
        info.message.as_deref(),
        Some("intentional failure for tests")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn dispatches_step_executor() {
    let reg = PluginRegistry::load(RegistryConfig {
        auto_discover: false,
        extra_paths: vec![fixtures::fixture_path("hm-fixture-noop-executor")],
        ..Default::default()
    })
    .unwrap();
    let idx = reg.capabilities.resolve_runner("noop").unwrap();
    let plugin = reg.get(idx).unwrap();
    let input = ExecutorInput {
        step: CommandStep {
            key: "build".into(),
            label: None,
            cmd: "true".into(),
            builds_in: None,
            image: None,
            env: None,
            timeout_seconds: None,
            cache: None,
            runner: Some("noop".into()),
            runner_args: None,
        },
        workspace_archive_id: ArchiveId(Uuid::nil()),
        env: std::collections::BTreeMap::new(),
        workdir: "/workspace".into(),
        run_id: Uuid::nil(),
        step_id: Uuid::nil(),
        cache_lookup: CacheDecision::MissNoCommit,
        parent_snapshot: None,
    };
    let result: StepResult = plugin
        .execute_step(&input)
        .await
        .unwrap();
    assert_eq!(result.exit_code, 0);
    assert!(result.committed_snapshot.is_none());
}
