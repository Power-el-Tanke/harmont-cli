use std::io::IsTerminal;

use anyhow::Result;

use crate::cli::Cli;
use crate::config::Config;
use crate::output::OutputMode;

/// Runtime context that bundles resolved config and output preferences.
///
/// After the plan-4 cloud-plugin cutover this is intentionally thin:
/// API client, credential store, and active-org resolution moved into
/// `hm-plugin-cloud`. The host context retains the config file (for
/// future use) and the output mode.
#[derive(Debug)]
pub struct RunContext {
    pub config: Config,
    /// Output mode for the residual built-in verbs (the legacy global
    /// `--format` flag was retired in plan 3; per-subcommand `--format`
    /// is the only currently-wired source, so this defaults to human).
    pub output: OutputMode,
    /// Disable the interactive TUI even when stdout is a TTY.
    pub no_tui: bool,
    /// Disable TUI animation effects.
    pub no_fx: bool,
}

impl RunContext {
    /// Build a [`RunContext`] from parsed CLI args.
    ///
    /// # Errors
    ///
    /// Returns an error if the config file is unreadable or malformed.
    pub fn from_cli(cli: &Cli) -> Result<Self> {
        let config = Config::load()?;

        let output = OutputMode::Human {
            color: !cli.no_color,
            interactive: std::io::stdout().is_terminal(),
        };

        Ok(Self {
            config,
            output,
            no_tui: cli.no_tui,
            no_fx: cli.no_fx,
        })
    }
}
