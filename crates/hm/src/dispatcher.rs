//! Subcommand-plugin dispatcher.
//!
//! Routes `hm <unknown-verb> <args...>` to the registered plugin
//! whose manifest's `SubcommandSpec.verb` matches the first argv
//! token. The plugin parses its own argv internally; the host
//! forwards the raw args.

#![allow(
    clippy::print_stderr,
    reason = "this is a top-level dispatch site; ExitInfo.message is user-facing output to stderr"
)]

use std::collections::BTreeMap;

use anyhow::{Context, Result};
use hm_plugin_protocol::{ExitInfo, SubcommandInput};

use crate::error::HmError;
use crate::plugin::{PluginRegistry, RegistryConfig};

/// Entry point: invoke a plugin-provided subcommand. `argv` is the
/// captured `external_subcommand` args INCLUDING the verb itself (clap's
/// convention). `no_tui` suppresses the interactive TUI even when stdout
/// is a TTY. Returns the process exit code.
///
/// # Errors
/// Returns an error if no plugin claims the verb, the plugin fails to
/// load, or the plugin panics during dispatch. Non-zero `ExitInfo.exit_code`
/// is surfaced as `Ok(i32)`, not as `Err`.
pub async fn run(argv: Vec<String>, no_tui: bool) -> Result<i32> {
    if argv.is_empty() {
        anyhow::bail!("dispatcher called with empty argv (clap bug)");
    }

    use is_terminal::IsTerminal;

    // Detect `hm cloud build watch ...` to opt into the TUI session sink.
    let is_cloud_build_watch = argv.first().map(String::as_str) == Some("cloud")
        && argv.get(1).map(String::as_str) == Some("build")
        && argv.get(2).map(String::as_str) == Some("watch");
    let want_tui_for_cloud_watch = is_cloud_build_watch
        && !no_tui
        && std::env::var_os("NO_COLOR").is_none()
        && std::io::stdout().is_terminal();

    if want_tui_for_cloud_watch {
        let tui_rx = crate::tui::install_session_sink();
        let opts = crate::tui::TuiOptions {
            fx_enabled: std::env::var_os("NO_COLOR").is_none(),
            summary_card: true,
            title: "hm cloud build watch".into(),
        };
        let argv_clone = argv.clone();
        let plugin_handle = tokio::spawn(async move { run_plugin(argv_clone).await });
        let tui_exit = crate::tui::run(tui_rx, opts)
            .await
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        let plugin_exit = plugin_handle.await??;
        return Ok(if tui_exit != 0 { tui_exit } else { plugin_exit });
    }

    run_plugin(argv).await
}

/// Load the plugin registry, resolve the verb, and call the plugin.
async fn run_plugin(argv: Vec<String>) -> Result<i32> {
    let verb = argv
        .first()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("dispatcher called with empty argv (clap bug)"))?;

    let registry = PluginRegistry::load(RegistryConfig {
        auto_discover: true,
        extra_paths: vec![],
        embedded: vec![
            (
                "harmont-docker",
                crate::plugin::embedded::DOCKER_PLUGIN_WASM,
            ),
            ("harmont-cloud", crate::plugin::embedded::CLOUD_PLUGIN_WASM),
        ],
        pool_sizes: BTreeMap::new(),
    })
    .context("load plugin registry")?;

    let idx = registry
        .subcommand_index
        .get(&verb)
        .copied()
        .ok_or_else(|| HmError::UnknownVerb {
            verb: verb.clone(),
            available: registry.subcommand_index.keys().cloned().collect(),
        })?;

    let plugin = registry
        .get(idx)
        .context("plugin moved away during dispatch")?;

    let env: BTreeMap<String, String> = std::env::vars()
        .filter(|(k, _)| k.starts_with("HARMONT_"))
        .collect();

    let input = SubcommandInput {
        verb_path: argv.clone(),
        args: serde_json::Value::Null, // plugin parses raw argv itself
        env,
    };

    let info: ExitInfo = plugin
        .call_capability("hm_subcommand_run", &input)
        .await
        .with_context(|| format!("invoke plugin for verb '{verb}'"))?;

    if let Some(msg) = info.message {
        eprintln!("{msg}");
    }
    Ok(info.exit_code)
}
