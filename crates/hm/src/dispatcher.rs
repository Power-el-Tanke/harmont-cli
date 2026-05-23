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
use std::sync::Arc;

use anyhow::{Context, Result};
use hm_plugin_protocol::{ExitInfo, SubcommandInput};

use crate::error::HmError;
use crate::plugin::host_api::HostApiImpl;
use crate::plugin::{PluginRegistry, RegistryConfig};

/// Entry point: invoke a plugin-provided subcommand. `argv` is the
/// captured `external_subcommand` args INCLUDING the verb itself (clap's
/// convention). Returns the process exit code.
///
/// # Errors
/// Returns an error if no plugin claims the verb, the plugin fails to
/// load, or the plugin panics during dispatch. Non-zero `ExitInfo.exit_code`
/// is surfaced as `Ok(i32)`, not as `Err`.
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
        .run_subcommand(&input)
        .await
        .with_context(|| format!("invoke plugin for verb '{verb}'"))?;

    if let Some(msg) = info.message {
        eprintln!("{msg}");
    }
    Ok(info.exit_code)
}
