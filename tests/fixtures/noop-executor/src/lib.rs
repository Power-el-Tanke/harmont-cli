//! Minimal step-executor plugin. Records every `ExecutorInput` it
//! receives into a `Plugin`-scoped KV slot so tests can inspect it
//! after invocation.

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
struct NoopExec;

impl StepExecutor for NoopExec {
    fn run<'a>(
        &'a self,
        ctx: &'a PluginContext<'a>,
        input: ExecutorInput,
    ) -> impl Future<Output = Result<StepResult, PluginError>> + Send + 'a {
        async move {
            let key = format!("seen:{}", input.step.key);
            let val = serde_json::to_vec(&input)
                .map_err(|e| PluginError::new("serde", e.to_string()))?;
            ctx.kv_set(KvScope::Plugin, &key, &val);
            ctx.log(Level::Info, &format!("noop ran step '{}'", input.step.key));
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
        name: "harmont-fixture-noop".into(),
        version: semver::Version::new(0, 1, 0),
        description: "Test fixture: records ExecutorInput, returns 0.".into(),
        capabilities: vec![Capability::StepExecutor(StepExecutorSpec {
            runner: "noop".into(),
            default: false,
            step_schema: None,
        })],
        config_schema: None,
    },
    executor = NoopExec,
);
