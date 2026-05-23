//! Declares a manifest with the wrong api_version. Used to assert
//! the host rejects it at load time.

#![allow(
    unsafe_code,
    clippy::pedantic,
    clippy::nursery,
    clippy::cargo,
    clippy::multiple_crate_versions,
    clippy::cargo_common_metadata,
    clippy::missing_errors_doc
)]

use core::future::Future;
use hm_plugin_sdk::*;

/// Dummy executor required by `hm_plugin!` — the host should reject
/// this plugin before ever calling `run` because of the bad
/// `api_version`.
#[derive(Default)]
struct DummyExec;

impl StepExecutor for DummyExec {
    fn run<'a>(
        &'a self,
        _ctx: &'a PluginContext<'a>,
        _input: ExecutorInput,
    ) -> impl Future<Output = Result<StepResult, PluginError>> + Send + 'a {
        async move {
            Err(PluginError::new(
                "unreachable",
                "bad-api-version fixture should never be called",
            ))
        }
    }
}

hm_plugin!(
    manifest = PluginManifest {
        api_version: 9999,
        name: "harmont-fixture-bad-api".into(),
        version: semver::Version::new(0, 1, 0),
        description: "always fails to load".into(),
        capabilities: vec![Capability::StepExecutor(StepExecutorSpec {
            runner: "bad".into(),
            default: false,
            step_schema: None,
        })],
        config_schema: None,
    },
    executor = DummyExec,
);
