//! Compile-time integration test for the `hm_plugin!` proc macro.
//!
//! If this file compiles, the macro expansion is syntactically and
//! type-theoretically correct. We cannot call `hm_load_plugin` at
//! runtime without a real `HostRef`, but compilation itself proves the
//! generated code is well-formed.

#![allow(
    unsafe_code,
    clippy::pedantic,
    clippy::nursery,
    clippy::cargo,
    clippy::multiple_crate_versions,
    clippy::cargo_common_metadata,
    clippy::expect_used,
    clippy::manual_async_fn,
    dead_code
)]

use core::future::Future;
use hm_plugin_sdk::*;

// ---------- Executor --------------------------------------------------------

#[derive(Default)]
struct TestExec;

impl StepExecutor for TestExec {
    fn run<'a>(
        &'a self,
        _ctx: &'a PluginContext<'a>,
        _input: ExecutorInput,
    ) -> impl Future<Output = Result<StepResult, PluginError>> + Send + 'a {
        async {
            Ok(StepResult {
                exit_code: 0,
                committed_snapshot: None,
                artifacts: vec![],
            })
        }
    }
}

// ---------- Hook ------------------------------------------------------------

#[derive(Default)]
struct TestHook;

impl LifecycleHook for TestHook {
    fn on_event(
        &self,
        _ctx: &PluginContext<'_>,
        _event: HookEvent,
    ) -> impl Future<Output = Result<HookOutcome, PluginError>> + Send + '_ {
        async { Ok(HookOutcome::Continue) }
    }
}

// ---------- Subcommand ------------------------------------------------------

#[derive(Default)]
struct TestSub;

impl SubcommandPlugin for TestSub {
    fn run(
        &self,
        _ctx: &PluginContext<'_>,
        _input: SubcommandInput,
    ) -> impl Future<Output = Result<ExitInfo, PluginError>> + Send + '_ {
        async {
            Ok(ExitInfo {
                exit_code: 0,
                message: None,
            })
        }
    }
}

// ---------- Macro invocations -----------------------------------------------

// Full invocation with all capabilities
hm_plugin!(
    manifest = PluginManifest {
        api_version: HM_PLUGIN_API_VERSION,
        name: "test-all-caps".into(),
        version: semver::Version::new(0, 0, 1),
        description: "compile-test with all capabilities".into(),
        capabilities: vec![],
        config_schema: None,
    },
    executor = TestExec,
    hook = TestHook,
    subcommand = TestSub,
);

#[test]
fn macro_compiles() {
    // If we reach here, the macro expansion compiled successfully.
}
