pub mod dev;
pub mod run;

use anyhow::Result;

use crate::cli::Command;
use crate::context::RunContext;

/// Dispatch a parsed CLI command to the appropriate handler. Returns an exit code.
///
/// # Errors
///
/// Returns an error if the dispatched handler returns one. The exact set
/// of failures depends on the command (filesystem/Docker for `run`,
/// plugin-registry IO for `plugin`, plugin runtime errors for `external`).
pub async fn dispatch(command: Command, ctx: RunContext) -> Result<i32> {
    match command {
        Command::Run(args) => run::handle(args, ctx).await,
        Command::Dev(cmd) => dev::dispatch(cmd, ctx).await,
        Command::Version => crate::builtin::version::run().await.map(|()| 0),
        Command::Plugin(cmd) => crate::builtin::plugin::run(cmd).await.map(|()| 0),
        Command::External(argv) => crate::dispatcher::run(argv).await,
    }
}
