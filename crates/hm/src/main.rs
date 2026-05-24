#![allow(
    clippy::print_stderr,
    reason = "the panic banner in handle_error is the last-resort stderr writer"
)]
#![allow(
    clippy::multiple_crate_versions,
    reason = "transitive dependency version conflicts in rand/windows-sys/thiserror chains; not fixable without upstream updates"
)]

use std::sync::Arc;

use clap::{CommandFactory, FromArgMatches};
use owo_colors::OwoColorize;
use tracing_subscriber::EnvFilter;

use harmont_cli::cli::{self, Cli};
use harmont_cli::context::RunContext;
use harmont_cli::error::{self, HmError};
use harmont_cli::output::status;
use harmont_cli::plugin::host_api::HostApiImpl;
use harmont_cli::plugin::{PluginRegistry, RegistryConfig};
#[tokio::main]
async fn main() {
    let code = match run().await {
        Ok(code) => code,
        Err(e) => handle_error(&e),
    };

    std::process::exit(code);
}

async fn run() -> Result<i32, anyhow::Error> {
    // 1. Best-effort plugin discovery — if it fails we proceed with
    //    only the built-in subcommands and log a warning later.
    let registry = PluginRegistry::load(RegistryConfig {
        auto_discover: true,
        extra_paths: vec![],
        host_api: Arc::new(HostApiImpl::new_noop()),
    })
    .await
    .ok();

    let plugin_specs = registry
        .as_ref()
        .map(PluginRegistry::subcommand_specs)
        .unwrap_or_default();

    // 2. Build the augmented clap::Command (built-ins + plugin verbs)
    //    and parse argv once.
    let builtins: std::collections::HashSet<String> = Cli::command()
        .get_subcommands()
        .map(|c| c.get_name().to_owned())
        .collect();

    let cmd = Cli::command_with_plugins(&plugin_specs);
    let matches = cmd.get_matches();

    // 3. Extract global flags from the matches so we can configure
    //    tracing and color regardless of which subcommand was matched.
    let verbose = matches.get_flag("verbose");
    let no_color = matches.get_flag("no_color");

    if verbose {
        tracing_subscriber::fmt()
            .with_env_filter(
                EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("debug")),
            )
            .with_target(false)
            .init();
    }

    let color_enabled =
        !no_color && std::env::var_os("NO_COLOR").is_none() && console::Term::stderr().is_term();
    owo_colors::set_override(color_enabled);

    // 4. Route: built-in subcommand → derive reconstruction; plugin
    //    verb → extract args via clap_bridge and forward to the plugin.
    let (sub_name, sub_matches) = matches
        .subcommand()
        .ok_or_else(|| anyhow::anyhow!("no subcommand provided"))?;

    if builtins.contains(sub_name) {
        // Reconstruct the full Cli struct from the already-parsed
        // ArgMatches so built-in handlers keep their typed derive args.
        let cli_args = Cli::from_arg_matches(&matches)?;
        let command = cli_args.command.clone();
        let ctx = RunContext::from_cli(&cli_args)?;
        cli::dispatch(command, ctx).await
    } else {
        // Plugin subcommand — extract args via the clap bridge.
        let mut verb_path = vec![sub_name.to_string()];
        let (sub_path, args) = hm_plugin_runtime::clap_bridge::extract_args(sub_matches);
        verb_path.extend(sub_path);

        let reg = registry.ok_or_else(|| {
            anyhow::anyhow!(
                "plugin registry failed to load; cannot dispatch plugin verb '{sub_name}'"
            )
        })?;
        cli::external::run_parsed(sub_name, verb_path, args, &reg).await
    }
}

fn handle_error(err: &anyhow::Error) -> i32 {
    // Try to downcast to our typed error for a specific exit code.
    if let Some(hm_err) = err.downcast_ref::<HmError>() {
        status::print_error(&format!("{hm_err}"));
        return hm_err.exit_code();
    }

    // Generic error.
    let msg = format!("{err:#}");
    eprintln!("{} {msg}", "error:".red().bold());
    error::EXIT_BUILD_FAILED
}
