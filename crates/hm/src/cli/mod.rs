pub mod dev;
pub mod run;

pub use dev::{DevCommand, DevDownArgs, DevExecArgs, DevLogsArgs, DevPortOfArgs, DevUpArgs};
pub use run::RunArgs;

use clap::{Parser, Subcommand};

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

    /// Plugin-provided subcommand. Captured raw; the dispatcher
    /// looks it up in the registry and invokes the matching plugin.
    #[command(external_subcommand)]
    External(Vec<String>),
}

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Subcommand)]
pub enum PluginCommand {
    /// List installed plugins (embedded + user + project).
    List,

    /// Show one plugin's manifest in detail.
    Info {
        /// Plugin name (matches `name` field of the manifest).
        name: String,
    },

    /// Install a plugin from a file path or HTTPS URL.
    ///
    /// HTTPS URLs require `--pin <sha256>` for integrity.
    Install {
        /// Plugin source: local path (`./foo.wasm`) or HTTPS URL.
        source: String,

        /// SHA-256 hex digest to verify against. Required for HTTPS
        /// sources; optional for local paths.
        #[arg(long, value_name = "SHA256_HEX")]
        pin: Option<String>,
    },

    /// Remove an installed plugin by name.
    Remove {
        /// Plugin name.
        name: String,
    },
}

