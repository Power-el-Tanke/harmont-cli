use core::future::Future;

use crate::context::PluginContext;
use hm_plugin_protocol::{ExitInfo, PluginError, SubcommandInput};

/// Implemented by subcommand plugins.
pub trait SubcommandPlugin: Send + Sync + Default {
    /// Run the subcommand.
    ///
    /// # Errors
    /// Returns a [`PluginError`] describing the failure. The host
    /// renders the error and exits the process with code 1.
    fn run<'a>(
        &'a self,
        ctx: &'a PluginContext<'a>,
        input: SubcommandInput,
    ) -> impl Future<Output = Result<ExitInfo, PluginError>> + Send + 'a;
}
