use core::future::Future;

use crate::context::PluginContext;
use hm_plugin_protocol::{ExecutorInput, PluginError, StepResult};

/// Implemented by step-executor plugins. The host calls
/// [`StepExecutor::run`] exactly once per step; the plugin returns a
/// [`StepResult`] or a [`PluginError`].
///
/// During the call the plugin may stream logs via
/// [`PluginContext::emit_step_log_stdout`] /
/// [`PluginContext::emit_step_log_stderr`] and check cancellation via
/// [`PluginContext::should_cancel`].
pub trait StepExecutor: Send + Sync + Default {
    /// Execute a single step.
    ///
    /// # Errors
    /// Returns a [`PluginError`] describing the failure. The host
    /// converts errors into build events and a non-zero step exit.
    fn run<'a>(
        &'a self,
        ctx: &'a PluginContext<'a>,
        input: ExecutorInput,
    ) -> impl Future<Output = Result<StepResult, PluginError>> + Send + 'a;
}
