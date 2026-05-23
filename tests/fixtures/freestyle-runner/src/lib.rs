//! Fixture: registers as `runner: "freestyle"` and records the step
//! key it was invoked with into `Plugin`-scoped KV under
//! `freestyle_called_with`. The host-side test asserts this KV value
//! to prove that a step declaring `runner: "freestyle"` actually
//! lands here (and not on the docker default) — the regression guard
//! for PR #22's runner-field-drop bug.

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

#[derive(Default)]
struct Freestyle;

impl StepExecutor for Freestyle {
    fn run<'a>(
        &'a self,
        ctx: &'a PluginContext<'a>,
        input: ExecutorInput,
    ) -> impl Future<Output = Result<StepResult, PluginError>> + Send + 'a {
        async move {
            ctx.kv_set(
                KvScope::Plugin,
                "freestyle_called_with",
                input.step.key.as_bytes(),
            );
            Ok(StepResult {
                exit_code: 0,
                committed_snapshot: None,
                artifacts: vec![],
            })
        }
    }
}

hm_plugin!(
    manifest = PluginManifest {
        api_version: HM_PLUGIN_API_VERSION,
        name: "harmont-fixture-freestyle".into(),
        version: semver::Version::new(0, 1, 0),
        description: "Test fixture: records step key under runner=freestyle.".into(),
        capabilities: vec![Capability::StepExecutor(StepExecutorSpec {
            runner: "freestyle".into(),
            default: false,
            step_schema: None,
        })],
        required_host_fns: vec![],
        config_schema: None,
        allowed_hosts: vec![],
    },
    executor = Freestyle,
);
