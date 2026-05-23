//! Manifest validation: hosts must reject wrong API versions, missing
//! host fns, and duplicate runners.

#![allow(
    clippy::cargo_common_metadata,
    clippy::multiple_crate_versions,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

pub mod common;

use common::fixtures;
use harmont_cli::plugin::error::RuntimeError;
use harmont_cli::plugin::{PluginRegistry, RegistryConfig};

#[test]
fn rejects_wrong_api_version() {
    let path = fixtures::fixture_path("hm-fixture-bad-api-version");
    let err = PluginRegistry::load(RegistryConfig {
        auto_discover: false,
        extra_paths: vec![path],
        ..Default::default()
    })
    .expect_err("should fail to load");
    let rt_err: &RuntimeError = err.downcast_ref().expect("RuntimeError");
    match rt_err {
        RuntimeError::PluginManifest {
            found_api,
            expected_api,
            ..
        } => {
            assert_eq!(*found_api, 9999);
            assert_eq!(*expected_api, hm_plugin_protocol::HM_PLUGIN_API_VERSION);
        }
        other => panic!("expected PluginManifest variant, got {other:?}"),
    }
}

#[test]
fn rejects_duplicate_runner() {
    let path = fixtures::fixture_path("hm-fixture-noop-executor");
    let err = PluginRegistry::load(RegistryConfig {
        auto_discover: false,
        extra_paths: vec![path.clone(), path],

        ..Default::default()
    })
    .expect_err("should detect duplicate");
    let rt_err: &RuntimeError = err.downcast_ref().expect("RuntimeError");
    assert!(matches!(rt_err, RuntimeError::PluginConflict { verb, .. } if verb == "runner:noop"));
}
