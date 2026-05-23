//! End-to-end: load the `host_fn_probe` fixture, run its subcommand,
//! and parse its `Report`. Every host fn in `HOST_FN_NAMES` must
//! either be exercised here or by a downstream plan's tests.

#![allow(
    clippy::cargo_common_metadata,
    clippy::multiple_crate_versions,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    unsafe_code,
    reason = "test pokes XDG_CONFIG_HOME via std::env::set_var, which is unsafe in Rust 2024"
)]

pub mod common;

use common::fixtures;
use harmont_cli::plugin::host::dummy_subcommand_input;
use harmont_cli::plugin::{PluginRegistry, RegistryConfig};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "this is a test report struct"
)]
struct Report {
    log_ok: bool,
    kv_round_trip: bool,
    kv_isolated_per_scope: bool,
    fs_read_returns_none_for_missing: bool,
    should_cancel_default_false: bool,
}

#[tokio::test(flavor = "multi_thread")]
async fn host_fn_probe_passes_all_checks() {
    // KvScope::Plugin is persisted under <config_dir>/harmont/state/ and
    // the credential store under <home>/.harmont/credentials.toml. Point
    // both at a tempdir so this test doesn't touch the developer's real
    // config tree.
    let temp = tempfile::tempdir().expect("tempdir");
    // SAFETY: process-wide env vars set during a test; the tempdir is
    // unique per run and the test doesn't unset it (other tests use
    // their own tempdirs).
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", temp.path());
        std::env::set_var("HOME", temp.path());
    }
    let path = fixtures::fixture_path("hm-fixture-host-fn-probe");
    let reg = PluginRegistry::load(RegistryConfig {
        auto_discover: false,
        extra_paths: vec![path],
        ..Default::default()
    })
    .expect("load registry");
    let idx = reg.subcommand_index["fixture-probe"];
    let plugin = reg.get(idx).expect("plugin present");
    let info = plugin
        .run_subcommand(&dummy_subcommand_input())
        .await
        .expect("invoke");
    let report: Report =
        serde_json::from_str(info.message.as_deref().expect("report message present"))
            .expect("parse Report json");
    assert!(report.log_ok);
    assert!(report.kv_round_trip);
    assert!(report.kv_isolated_per_scope);
    assert!(report.fs_read_returns_none_for_missing);
    assert!(report.should_cancel_default_false);
}
