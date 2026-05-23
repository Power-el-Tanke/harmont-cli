use clap::{Parser, Subcommand};
use std::path::PathBuf;

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
// Run (the killer feature)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Parser)]
pub struct RunArgs {
    /// Pipeline slug. Required when the repo declares more than one
    /// `@hm.pipeline`; the CLI lists available slugs when omitted.
    #[arg()]
    pub pipeline: Option<String>,

    /// Branch to record on the build.
    #[arg(short, long)]
    pub branch: Option<String>,

    /// Build message.
    #[arg(short, long)]
    pub message: Option<String>,

    /// Environment variables (KEY=VALUE).
    #[arg(short, long)]
    pub env: Vec<String>,

    /// Source root (defaults to cwd).
    #[arg(short, long)]
    pub dir: Option<PathBuf>,

    /// Skip watching the build after it's created.
    #[arg(long)]
    pub no_watch: bool,

    /// Maximum number of chains to run concurrently. Defaults to the
    /// host's available parallelism. `0` is treated as `1`.
    #[arg(long, value_name = "N")]
    pub parallelism: Option<usize>,

    /// Output formatter (matches an installed output-formatter plugin
    /// `name`). Built-ins: `human`, `json`. Default: `human`.
    #[arg(long, value_name = "NAME", default_value = "human", global = false)]
    pub format: String,
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

// ---------------------------------------------------------------------------
// Dev (local deployments)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Subcommand)]
pub enum DevCommand {
    /// Bring deployments up in the foreground. Blocks until Ctrl-C.
    Up(DevUpArgs),
    /// Tear down deployments owned by this worktree's sessions.
    Down(DevDownArgs),
    /// List registered + running deployments.
    Ls,
    /// Tail logs of a live deployment from another terminal.
    Logs(DevLogsArgs),
    /// Print the host port for a live deployment. Designed for $() use.
    PortOf(DevPortOfArgs),
    /// One-shot exec into a live deployment container.
    Exec(DevExecArgs),
}

#[derive(Debug, Clone, Parser)]
pub struct DevUpArgs {
    /// Deployment slugs to bring up. When empty, brings up everything
    /// registered in `.harmont/*.py`.
    #[arg()]
    pub slugs: Vec<String>,

    /// Skip transitive dependencies; bring up exactly the listed slugs.
    #[arg(long)]
    pub no_deps: bool,

    /// Force image rebuild on `from_=Step` deployments even if a cached
    /// build image exists.
    #[arg(long)]
    pub rebuild: bool,
}

#[derive(Debug, Clone, Parser)]
pub struct DevDownArgs {
    /// Slugs to sweep. When empty, sweeps all sessions of this worktree.
    #[arg()]
    pub slugs: Vec<String>,

    /// Sweep one specific session entirely (overrides `slugs`).
    #[arg(long, value_name = "ID")]
    pub session: Option<String>,

    /// Sweep system-wide instead of this worktree (every container
    /// labelled `harmont.driver=local`).
    #[arg(long)]
    pub all: bool,
}

#[derive(Debug, Clone, Parser)]
pub struct DevLogsArgs {
    pub slug: String,

    #[arg(short, long)]
    pub follow: bool,

    #[arg(long, value_name = "ID")]
    pub session: Option<String>,
}

#[derive(Debug, Clone, Parser)]
pub struct DevPortOfArgs {
    pub slug: String,

    /// Container-internal port whose host binding to print.
    pub container_port: u16,

    #[arg(long, value_name = "ID")]
    pub session: Option<String>,
}

#[derive(Debug, Clone, Parser)]
pub struct DevExecArgs {
    pub slug: String,

    /// Command to run inside the container. Default `sh -l`.
    #[arg(trailing_var_arg = true)]
    pub cmd: Vec<String>,

    #[arg(long, value_name = "ID")]
    pub session: Option<String>,
}
