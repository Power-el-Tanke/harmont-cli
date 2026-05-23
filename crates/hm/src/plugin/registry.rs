//! Discovers native shared-library plugins under the user and project
//! plugin dirs, validates each manifest, and builds a capability index
//! used by the dispatcher.

// Pedantic-bucket nags accepted at module scope:
// - `missing_errors_doc`: every fallible fn returns `anyhow::Result`
//   with rich `with_context` messages.
// - `needless_pass_by_value`: `RegistryConfig` is intentionally moved
//   into `load` so callers can't reuse a config they expected to
//   consume.
// - `collapsible_if`: the nested `if s.default { … }` reads more clearly
//   one rule per line.
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::collapsible_if)]

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use hm_plugin_protocol::{Capability, PluginManifest};

use super::host::LoadedPlugin;
use super::host_api::HostApiImpl;
use super::manifest::{ManifestError, validate_standalone};
use super::paths;
use crate::error::HmError;

#[derive(Debug)]
pub struct RegistryConfig {
    /// If `false`, skip discovery and only registers explicitly added
    /// plugins. Used by integration tests.
    pub auto_discover: bool,
    /// Extra plugin paths to load (in addition to discovery). Used by
    /// tests to load fixture plugins.
    pub extra_paths: Vec<PathBuf>,
    /// The host API implementation shared by all loaded plugins.
    pub host_api: Arc<HostApiImpl>,
}

impl Default for RegistryConfig {
    fn default() -> Self {
        Self {
            auto_discover: false,
            extra_paths: Vec::new(),
            host_api: Arc::new(HostApiImpl::new_noop()),
        }
    }
}

#[derive(Debug)]
pub struct PluginRegistry {
    plugins: Vec<Arc<LoadedPlugin>>,
    pub subcommand_index: BTreeMap<String, usize>,
    pub runner_index: BTreeMap<String, usize>,
    pub output_formatter_index: BTreeMap<String, usize>,
    pub default_runner: Option<usize>,
}

impl PluginRegistry {
    pub fn load(config: RegistryConfig) -> Result<Self> {
        let mut plugins: Vec<Arc<LoadedPlugin>> = Vec::new();
        let dll_ext = std::env::consts::DLL_EXTENSION;

        if config.auto_discover {
            for dir in [paths::user_plugins_dir(), paths::project_plugins_dir()]
                .into_iter()
                .flatten()
            {
                if !dir.is_dir() {
                    continue;
                }
                let entries =
                    std::fs::read_dir(&dir).with_context(|| format!("read {}", dir.display()))?;
                for ent in entries {
                    let Ok(ent) = ent else { continue };
                    let path = ent.path();
                    if path.extension().and_then(|s| s.to_str()) != Some(dll_ext) {
                        continue;
                    }
                    let p = LoadedPlugin::load(&path, config.host_api.clone())
                        .with_context(|| format!("load {}", path.display()))?;
                    validate(&p.manifest)?;
                    plugins.push(Arc::new(p));
                }
            }
        }

        for path in &config.extra_paths {
            let p = LoadedPlugin::load(path, config.host_api.clone())
                .with_context(|| format!("load {}", path.display()))?;
            validate(&p.manifest)?;
            plugins.push(Arc::new(p));
        }

        let mut me = Self {
            plugins,
            subcommand_index: BTreeMap::new(),
            runner_index: BTreeMap::new(),
            output_formatter_index: BTreeMap::new(),
            default_runner: None,
        };
        me.index_capabilities()?;
        Ok(me)
    }

    fn index_capabilities(&mut self) -> Result<()> {
        for (i, p) in self.plugins.iter().enumerate() {
            for cap in &p.manifest.capabilities {
                match cap {
                    Capability::Subcommand(s) => {
                        if let Some(other) = self.subcommand_index.insert(s.verb.clone(), i) {
                            return Err(HmError::PluginConflict {
                                verb: s.verb.clone(),
                                plugin_a: self.plugins[other].manifest.name.clone(),
                                plugin_b: p.manifest.name.clone(),
                            }
                            .into());
                        }
                    }
                    Capability::StepExecutor(s) => {
                        if let Some(other) = self.runner_index.insert(s.runner.clone(), i) {
                            return Err(HmError::PluginConflict {
                                verb: format!("runner:{}", s.runner),
                                plugin_a: self.plugins[other].manifest.name.clone(),
                                plugin_b: p.manifest.name.clone(),
                            }
                            .into());
                        }
                        if s.default {
                            if let Some(other) = self.default_runner.replace(i) {
                                return Err(HmError::PluginConflict {
                                    verb: "default-runner".into(),
                                    plugin_a: self.plugins[other].manifest.name.clone(),
                                    plugin_b: p.manifest.name.clone(),
                                }
                                .into());
                            }
                        }
                    }
                    Capability::OutputFormatter(s) => {
                        if let Some(other) = self.output_formatter_index.insert(s.name.clone(), i) {
                            return Err(HmError::PluginConflict {
                                verb: format!("format:{}", s.name),
                                plugin_a: self.plugins[other].manifest.name.clone(),
                                plugin_b: p.manifest.name.clone(),
                            }
                            .into());
                        }
                    }
                    Capability::LifecycleHook(_) => {
                        // Hooks can stack; no conflict possible.
                    }
                }
            }
        }
        Ok(())
    }

    pub fn manifests(&self) -> impl Iterator<Item = &PluginManifest> {
        self.plugins.iter().map(|p| &p.manifest)
    }

    /// Return a cheap clone of the plugin at `idx`. Callers should
    /// drop any registry-level lock they hold before awaiting on the
    /// returned plugin — the trait object is `Send + Sync`, so
    /// concurrent callers can invoke methods directly.
    #[must_use]
    pub fn get(&self, idx: usize) -> Option<Arc<LoadedPlugin>> {
        self.plugins.get(idx).cloned()
    }

    /// Returns the runner name of the plugin marked `default: true` at
    /// registration time, if any. Used by the scheduler to resolve
    /// steps that don't declare a `runner` field.
    #[must_use]
    pub fn default_runner_name(&self) -> Option<&str> {
        let idx = self.default_runner?;
        self.runner_index
            .iter()
            .find_map(|(name, &i)| (i == idx).then_some(name.as_str()))
    }
}

fn validate(m: &PluginManifest) -> Result<()> {
    validate_standalone(m).map_err(|e| match e {
        ManifestError::ApiVersion {
            name,
            found,
            expected,
        } => HmError::PluginManifest {
            name,
            expected_api: expected,
            found_api: found,
        }
        .into(),
        ManifestError::NoCapabilities { ref name }
        | ManifestError::BadRunnerName { ref name, .. }
        | ManifestError::DuplicateSubcommandVerb { ref name, .. } => HmError::PluginLoad {
            name: name.clone(),
            path: std::path::PathBuf::new(),
            reason: e.to_string(),
            doc_url: "https://harmont.dev/docs/plugins/manifest",
        }
        .into(),
    })
}
