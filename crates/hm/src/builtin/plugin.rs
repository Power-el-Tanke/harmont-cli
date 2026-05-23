//! Implementation of `hm plugin {list, info, install, remove}`.
//!
//! `list` and `info` read the real [`PluginRegistry`]; `install` and
//! `remove` operate on the on-disk install dir via
//! [`crate::plugin::install`] and [`crate::plugin::paths`].

use anyhow::{Context, Result};

use crate::cli::PluginCommand;
use crate::plugin::{PluginRegistry, RegistryConfig, paths};

/// Dispatch a parsed `hm plugin <subcommand>` to its handler.
///
/// # Errors
///
/// Surfaces registry-load failures from [`PluginRegistry::load`] for
/// `list`/`info`, "no such plugin" for `info <unknown>`, network/IO
/// failures and SHA-256 mismatches for `install`, and "no plugin file"
/// for `remove`.
pub async fn run(cmd: PluginCommand) -> Result<()> {
    match cmd {
        PluginCommand::List => list().await,
        PluginCommand::Info { name } => info(&name).await,
        PluginCommand::Install { source, pin } => install_cmd(&source, pin.as_deref()).await,
        PluginCommand::Remove { name } => remove(&name).await,
    }
}

// `println!` is the user-facing output for `hm plugin list`; this is
// the intended sink, not a debug-print left behind.
#[allow(clippy::print_stdout)]
// The dispatcher signature in `commands::dispatch` is `async`, but the
// body is currently synchronous — the registry load is CPU-bound.
#[allow(clippy::unused_async)]
async fn list() -> Result<()> {
    let reg = PluginRegistry::load(RegistryConfig {
        auto_discover: true,
        ..Default::default()
    })?;
    if reg.manifests().count() == 0 {
        println!("No plugins installed.");
        println!();
        println!("Plugins live in:");
        if let Some(p) = paths::user_plugins_dir() {
            println!("  {}", p.display());
        }
        if let Some(p) = paths::project_plugins_dir() {
            println!("  {}", p.display());
        }
        println!();
        println!("Install one with `hm plugin install <path-or-url>`.");
        return Ok(());
    }
    println!("{:<28} {:>10}  capabilities", "name", "version");
    for m in reg.manifests() {
        let caps: Vec<String> = m.capabilities.iter().map(capability_summary).collect();
        println!("{:<28} {:>10}  {}", m.name, m.version, caps.join(", "));
    }
    Ok(())
}

// `println!` is the user-facing output for `hm plugin info`; intended.
#[allow(clippy::print_stdout)]
#[allow(clippy::unused_async)]
async fn info(name: &str) -> Result<()> {
    let reg = PluginRegistry::load(RegistryConfig {
        auto_discover: true,
        ..Default::default()
    })?;
    let m = reg
        .manifests()
        .find(|m| m.name == name)
        .with_context(|| format!("no plugin named '{name}' is installed"))?;
    let json = serde_json::to_string_pretty(m)?;
    println!("{json}");
    Ok(())
}

// `println!` is the user-facing success line for `hm plugin install`.
#[allow(clippy::print_stdout)]
async fn install_cmd(source: &str, pin: Option<&str>) -> Result<()> {
    let path = crate::plugin::install::install(source, pin).await?;
    println!("Installed plugin to {}", path.display());
    Ok(())
}

// `println!` is the user-facing success line for `hm plugin remove`.
#[allow(clippy::print_stdout)]
#[allow(clippy::unused_async)]
async fn remove(name: &str) -> Result<()> {
    let dir = crate::plugin::paths::install_dir().context("no install dir")?;
    let dll_ext = std::env::consts::DLL_EXTENSION;
    let target = dir.join(format!("{name}.{dll_ext}"));
    if !target.is_file() {
        anyhow::bail!("no plugin file at {}", target.display());
    }
    std::fs::remove_file(&target).context("remove plugin")?;
    println!("Removed {}", target.display());
    Ok(())
}

fn capability_summary(cap: &hm_plugin_protocol::Capability) -> String {
    use hm_plugin_protocol::Capability::{
        LifecycleHook, OutputFormatter, StepExecutor, Subcommand,
    };
    match cap {
        Subcommand(s) => format!("subcmd:{}", s.verb),
        StepExecutor(s) => {
            if s.default {
                format!("runner:{}(*)", s.runner)
            } else {
                format!("runner:{}", s.runner)
            }
        }
        LifecycleHook(_) => "hook".into(),
        OutputFormatter(s) => format!("format:{}", s.name),
    }
}
