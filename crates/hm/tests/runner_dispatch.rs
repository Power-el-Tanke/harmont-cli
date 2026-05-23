//! Regression test: a `CommandStep` declaring `runner: "freestyle"`
//! must dispatch to the freestyle plugin, not the docker default.
//!
//! Background — PR #22: an earlier conversion path between the wire
//! `Pipeline` and the scheduler's `Node`/`ExecutorInput` round-trip
//! silently dropped the `runner` field, so every step landed on the
//! docker executor regardless of what the IR declared. A3 made the
//! orchestrator graph consume wire types directly so `runner` survives
//! end-to-end. This test pins that behaviour.

#![allow(
    clippy::cargo_common_metadata,
    clippy::multiple_crate_versions,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

pub mod common;

use std::collections::BTreeMap;
use std::sync::Arc;

use common::fixtures;
use harmont_cli::orchestrator::graph::Graph;
use harmont_cli::plugin::host_api::HostApiImpl;
use harmont_cli::plugin::{PluginRegistry, RegistryConfig};
use hm_plugin_protocol::{ArchiveId, CacheDecision, ExecutorInput, Pipeline, StepResult};
use hm_plugin_sdk::ffi::RawHostApi;
use uuid::Uuid;

const PIPELINE_JSON: &[u8] = br#"{
    "version": "0",
    "steps": [
        {
            "type": "command",
            "key": "fs-step",
            "cmd": "irrelevant; fixture ignores cmd",
            "runner": "freestyle"
        }
    ]
}"#;

#[tokio::test(flavor = "multi_thread")]
async fn runner_field_dispatches_to_named_plugin() {
    let host_api = Arc::new(HostApiImpl::new_noop());

    let reg = PluginRegistry::load(RegistryConfig {
        auto_discover: false,
        extra_paths: vec![fixtures::fixture_path("hm-fixture-freestyle-runner")],
        host_api: Arc::clone(&host_api),
    })
    .expect("load registry");

    let pipeline: Pipeline = serde_json::from_slice(PIPELINE_JSON).expect("parse pipeline");
    let graph = Graph::build(&pipeline).expect("build graph");

    assert_eq!(
        graph.nodes[0].step.runner.as_deref(),
        Some("freestyle"),
        "graph dropped `runner` field"
    );

    let step_wire = graph.nodes[0].step.clone();
    let input = ExecutorInput {
        step: step_wire,
        workspace_archive_id: ArchiveId(Uuid::nil()),
        env: BTreeMap::new(),
        workdir: "/workspace".into(),
        run_id: Uuid::nil(),
        step_id: Uuid::nil(),
        cache_lookup: CacheDecision::MissNoCommit,
        parent_snapshot: None,
    };

    let runner = input.step.runner.clone().unwrap_or_else(|| "docker".into());
    assert_eq!(runner, "freestyle", "runner derivation lost the field");

    let idx = *reg
        .runner_index
        .get(&runner)
        .unwrap_or_else(|| panic!("runner '{runner}' not in registry"));
    let plugin = reg.get(idx).expect("plugin present at index");

    let result: StepResult = plugin
        .execute_step(&input)
        .await
        .expect("dispatch freestyle");
    assert_eq!(result.exit_code, 0);

    // The fixture writes `step.key` into KvScope::Plugin (scope 0)
    // under "freestyle_called_with". Read it back from the shared
    // HostApiImpl.
    let key = "freestyle_called_with";
    let ffi_result = host_api.kv_get(
        0, // KvScope::Plugin
        hm_plugin_sdk::ffi::FfiSlice::from(key.as_bytes()),
    );
    let opt: Option<hm_plugin_sdk::ffi::FfiBytes> = ffi_result.into();
    let recorded = opt.expect("freestyle plugin did not record `freestyle_called_with`");
    assert_eq!(
        recorded.as_slice(),
        b"fs-step",
        "freestyle plugin recorded the wrong step key"
    );
}
