use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::{Context, Result};
use hm_plugin_protocol::{ExitInfo, SubcommandInput};

use crate::error::HmError;
use crate::plugin::host_api::HostApiImpl;
use crate::plugin::{PluginRegistry, RegistryConfig};

/// Run a plugin-provided external subcommand.
///
/// # Errors
///
/// Returns an error if plugin lookup or invocation fails.
pub async fn run(argv: Vec<String>) -> Result<i32> {
    let verb = argv
        .first()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("dispatcher called with empty argv (clap bug)"))?;

    let registry = PluginRegistry::load(RegistryConfig {
        auto_discover: true,
        extra_paths: vec![],
        host_api: Arc::new(HostApiImpl::new_noop()),
    })
    .context("load plugin registry")?;

    let idx = registry
        .capabilities
        .resolve_subcommand(&verb)
        .ok_or_else(|| HmError::UnknownVerb {
            verb: verb.clone(),
            available: registry.capabilities.available_subcommands().map(Into::into).collect(),
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
        .run_subcommand(&input)
        .await
        .with_context(|| format!("invoke plugin for verb '{verb}'"))?;

    if let Some(msg) = info.message {
        eprintln!("{msg}");
    }
    Ok(info.exit_code)
}
