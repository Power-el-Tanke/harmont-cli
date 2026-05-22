//! Thin wrapper around `extism::Plugin` instances loaded into a
//! per-plugin pool. Concurrent invocations from chain tasks acquire
//! a pool slot rather than blocking on a single plugin instance.

// Pedantic-bucket nags that don't add safety on this module:
// - `missing_errors_doc`: every public fn here returns `anyhow::Result`
//   with a context message; an `# Errors` section would just restate it.
// - `significant_drop_tightening` on `call_capability`: the `PoolGuard`
//   intentionally lives until after `serde_json::from_slice` returns,
//   because the `&[u8]` we just borrowed from the plugin's memory
//   only stays valid while the plugin instance is in scope.
#![allow(clippy::missing_errors_doc, clippy::significant_drop_tightening)]

use std::path::PathBuf;

use anyhow::{Context, Result};
use hm_plugin_protocol::PluginManifest;

use super::pool::PluginPool;
use crate::error::HmError;

#[derive(Debug)]
pub struct LoadedPlugin {
    pub manifest: PluginManifest,
    /// Path the plugin was loaded from. `None` if loaded from embedded
    /// bytes (`include_bytes!`).
    pub source: Option<PathBuf>,
    pool: PluginPool,
}

impl LoadedPlugin {
    /// Build a plugin from an on-disk `.wasm` file. The Extism manifest
    /// disables WASI filesystem access entirely (host-mediated reads
    /// only).
    ///
    /// Two-phase load: instantiate with no allowed hosts, read the
    /// plugin's [`PluginManifest`], then rebuild the pool with the
    /// allowlist the plugin declared. The throwaway pool is dropped
    /// before the real one is built.
    pub fn from_file(path: PathBuf, max_instances: usize) -> Result<Self> {
        let probe = PluginPool::from_file(path.clone(), max_instances)
            .with_context(|| format!("load plugin from {}", path.display()))?;
        let manifest = read_manifest(&probe)?;
        drop(probe);
        let pool = PluginPool::from_file_with_hosts(
            path.clone(),
            max_instances,
            manifest.allowed_hosts.clone(),
        )
        .with_context(|| format!("reload plugin from {} with allowed_hosts", path.display()))?;
        Ok(Self {
            manifest,
            source: Some(path),
            pool,
        })
    }

    /// Build a plugin from embedded bytes (used for in-tree builtins).
    ///
    /// Two-phase load: see [`LoadedPlugin::from_file`].
    pub fn from_bytes(bytes: &'static [u8], max_instances: usize) -> Result<Self> {
        let probe = PluginPool::from_bytes(bytes, max_instances).context("load embedded plugin")?;
        let manifest = read_manifest(&probe)?;
        drop(probe);
        let pool =
            PluginPool::from_bytes_with_hosts(bytes, max_instances, manifest.allowed_hosts.clone())
                .context("reload embedded plugin with allowed_hosts")?;
        Ok(Self {
            manifest,
            source: None,
            pool,
        })
    }

    /// Call a capability export. Acquires a pool slot for the duration
    /// of the call, then returns it. Generic over the input/output
    /// types.
    ///
    /// The `Send + Sync` bound on `I` is required so the returned
    /// future is `Send` — chain tasks await this future across a
    /// `tokio::spawn` boundary.
    pub async fn call_capability<I, O>(&self, export: &str, input: &I) -> Result<O>
    where
        I: serde::Serialize + Sync,
        O: serde::de::DeserializeOwned,
    {
        let in_bytes = serde_json::to_vec(input).context("serialise capability input")?;
        let mut guard = self
            .pool
            .acquire()
            .await
            .context("acquire plugin instance")?;
        // Set the per-plugin thread-local so `hm_kv_*` host fns can
        // resolve `KvScope::Plugin` to the right on-disk file.
        crate::plugin::host_fns::set_current_plugin_name(self.manifest.name.clone());
        let call_result = guard.plugin().call::<Vec<u8>, &[u8]>(export, in_bytes);
        crate::plugin::host_fns::clear_current_plugin_name();
        let out_bytes = call_result.map_err(|e| HmError::PluginPanic {
            name: self.manifest.name.clone(),
            capability: export.to_string(),
            message: format!("{e:#}"),
        })?;
        serde_json::from_slice(out_bytes).context("decode capability output")
    }
}

/// Test helper: synthesises a `SubcommandInput` shaped JSON value for
/// the `host_fn_probe` fixture and any other integration test that
/// needs a minimal valid input to `hm_subcommand_run`.
///
/// `#[doc(hidden)]` because this is not part of the production public
/// API; it exists so `tests/*.rs` integration tests (which see only
/// the public surface) can call into it without a separate feature
/// flag.
#[doc(hidden)]
#[must_use]
pub fn dummy_subcommand_input() -> serde_json::Value {
    serde_json::json!({
        "verb_path": ["fixture-probe"],
        "args": {},
        "env": {}
    })
}

/// Read the manifest from a freshly-instantiated plugin. Runs the
/// `hm_manifest` export and decodes the JSON.
///
/// Loading happens synchronously from startup paths (`hm version`,
/// `hm plugin list`) as well as from inside an existing tokio runtime
/// (`orchestrator::scheduler::run`). Use the current handle if
/// present; otherwise spin up a small single-threaded runtime.
fn read_manifest(pool: &PluginPool) -> Result<PluginManifest> {
    let task = async {
        let mut guard = pool.acquire().await?;
        let bytes = guard
            .plugin()
            .call::<&str, &[u8]>("hm_manifest", "")
            .context("call hm_manifest")?
            .to_vec();
        let manifest: PluginManifest =
            serde_json::from_slice(&bytes).context("decode hm_manifest output")?;
        Ok::<PluginManifest, anyhow::Error>(manifest)
    };
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        tokio::task::block_in_place(|| handle.block_on(task))
    } else {
        // No runtime; spin up a tiny one. Happens only when
        // `LoadedPlugin::from_*` is called from a truly synchronous
        // entry point (none in production today — kept for robustness
        // and unit tests that drive `LoadedPlugin` directly).
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .context("build adhoc tokio runtime for manifest read")?;
        rt.block_on(task)
    }
}
