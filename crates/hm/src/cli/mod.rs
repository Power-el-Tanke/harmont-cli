pub mod dev;
pub mod external;
pub mod plugin;
pub mod run;
pub mod version;

pub use dev::{DevCommand, DevDownArgs, DevExecArgs, DevLogsArgs, DevPortOfArgs, DevUpArgs};
pub use plugin::PluginCommand;
pub use run::RunArgs;

use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand};
use hm_plugin_protocol::SubcommandSpec;

use crate::context::RunContext;

#[derive(Debug, Parser)]
#[command(
    name = "hm",
    version,
    about = "hm — CLI for the Harmont CI platform",
    long_about = "hm is the command-line interface for Harmont.\n\n\
                   Run `hm run` to push local code through a pipeline without committing.",
    propagate_version = true,
    arg_required_else_help = true,
    disable_help_subcommand = true
)]
pub struct Cli {
    /// Override the API base URL. Hidden flag — set `HARMONT_API_URL` instead.
    #[arg(long, global = true, env = "HARMONT_API_URL", hide = true)]
    pub api_url: Option<String>,

    /// Enable verbose/debug logging.
    #[arg(long, short, global = true)]
    pub verbose: bool,

    /// Disable colored output.
    #[arg(long, global = true)]
    pub no_color: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Clone, Subcommand)]
pub enum Command {
    /// Run a pipeline locally via Docker.
    Run(RunArgs),

    /// Show hm version and plugin protocol API version.
    Version,

    /// Manage plugins.
    #[command(subcommand)]
    Plugin(PluginCommand),

    /// Manage local long-lived deployments (dev databases, dev API
    /// servers, dev webapps). Reads `.harmont/*.py` for
    /// `@hm.deploy`-decorated functions and brings them up via Docker.
    #[command(subcommand)]
    Dev(DevCommand),
}

/// Build a `clap::Command` that contains both the derive-defined
/// built-in subcommands and any plugin-provided subcommands.
///
/// The caller parses with the returned command, then routes based on
/// whether the matched subcommand is a built-in or plugin verb.
#[must_use]
pub fn build_augmented_command(plugin_specs: &[SubcommandSpec]) -> clap::Command {
    let mut cmd = Cli::command();
    for spec in plugin_specs {
        cmd = cmd.subcommand(hm_plugin_runtime::clap_bridge::build_command(spec));
    }
    cmd
}

/// Names of built-in subcommands defined in the [`Command`] derive enum.
/// Used by the two-phase parser to decide whether to reconstruct `Cli`
/// via `from_arg_matches` or route to the plugin dispatcher.
pub const BUILTIN_SUBCOMMANDS: &[&str] = &["run", "version", "plugin", "dev"];

/// Dispatch a parsed CLI command to the appropriate handler. Returns an exit code.
///
/// # Errors
///
/// Returns an error if the dispatched handler fails.
pub async fn dispatch(command: Command, ctx: RunContext) -> Result<i32> {
    match command {
        Command::Run(args) => crate::commands::run::handle(args, ctx).await,
        Command::Dev(cmd) => dev::dispatch(cmd, ctx).await,
        Command::Version => version::run().await.map(|()| 0),
        Command::Plugin(cmd) => plugin::run(cmd).await.map(|()| 0),
    }
}

