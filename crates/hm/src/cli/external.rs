use std::collections::BTreeMap;

use anyhow::{Context, Result};
use hm_plugin_protocol::{ExitInfo, SubcommandInput};

use crate::error::HmError;
use crate::plugin::PluginRegistry;

/// Run a plugin-provided subcommand with host-parsed arguments.
///
/// The caller (the two-phase parser in `main`) has already matched the
/// verb against the augmented `clap::Command` and extracted typed args
/// via [`hm_plugin_runtime::clap_bridge::extract_args`].
///
/// # Errors
///
/// Returns an error if plugin lookup or invocation fails.
pub async fn run_parsed(
    verb: &str,
    verb_path: Vec<String>,
    args: serde_json::Value,
    registry: &PluginRegistry,
) -> Result<i32> {
    let idx = registry
        .capabilities
        .resolve_subcommand(verb)
        .ok_or_else(|| HmError::UnknownVerb {
            verb: verb.to_owned(),
            available: registry
                .capabilities
                .available_subcommands()
                .map(Into::into)
                .collect(),
        })?;

    let plugin = registry
        .get(idx)
        .context("plugin moved away during dispatch")?;

    let env: BTreeMap<String, String> = std::env::vars()
        .filter(|(k, _)| k.starts_with("HARMONT_"))
        .collect();

    let input = SubcommandInput {
        verb_path,
        args,
        env,
    };

    let info: ExitInfo = plugin
        .run_subcommand(&input)
        .await
        .with_context(|| format!("invoke plugin for verb '{verb}'"))?;

    if let Some(msg) = info.message {
        eprintln!("{msg}");
    }
    Ok(info.exit_code)
}
