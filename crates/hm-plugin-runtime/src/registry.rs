//! Discovers native shared-library plugins under the user and project
//! plugin dirs, validates each manifest, and builds a capability index
//! used by the dispatcher.

#![allow(clippy::missing_errors_doc)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::collapsible_if)]

use std::collections::btree_map::Entry;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use hm_plugin_protocol::{Capability, PluginManifest};

use crate::error::RuntimeError;
use crate::host::LoadedPlugin;
use crate::host_api::HostApiImpl;
use crate::paths;

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
pub struct CapabilityIndex {
    subcommands: BTreeMap<String, usize>,
    runners: BTreeMap<String, usize>,
    default_runner: Option<usize>,
}

impl CapabilityIndex {
    /// Scan every plugin's declared capabilities and build the lookup
    /// indexes. Returns an error if two plugins claim the same verb,
    /// runner name, or default-runner slot.
    fn build(plugins: &[Arc<LoadedPlugin>]) -> Result<Self> {
        let conflict = |verb: String, a: usize, b: usize| -> anyhow::Error {
            RuntimeError::PluginConflict {
                verb,
                plugin_a: plugins[a].manifest.name.clone(),
                plugin_b: plugins[b].manifest.name.clone(),
            }
            .into()
        };

        plugins
            .iter()
            .enumerate()
            .flat_map(|(i, p)| p.manifest.capabilities.iter().map(move |cap| (i, cap)))
            .try_fold(
                (BTreeMap::new(), BTreeMap::new(), None::<usize>),
                |(mut subs, mut runners, mut default), (i, cap)| match cap {
                    Capability::Subcommand(s) => match subs.entry(s.verb.clone()) {
                        Entry::Vacant(e) => {
                            e.insert(i);
                            Ok((subs, runners, default))
                        }
                        Entry::Occupied(e) => Err(conflict(s.verb.clone(), *e.get(), i)),
                    },
                    Capability::StepExecutor(s) => {
                        match runners.entry(s.runner.clone()) {
                            Entry::Vacant(e) => {
                                e.insert(i);
                            }
                            Entry::Occupied(e) => {
                                return Err(conflict(
                                    format!("runner:{}", s.runner),
                                    *e.get(),
                                    i,
                                ))
                            }
                        }
                        if s.default {
                            if let Some(other) = default.replace(i) {
                                return Err(conflict("default-runner".into(), other, i));
                            }
                        }
                        Ok((subs, runners, default))
                    }
                    Capability::LifecycleHook(_) => Ok((subs, runners, default)),
                },
            )
            .map(|(subcommands, runners, default_runner)| Self {
                subcommands,
                runners,
                default_runner,
            })
    }

    /// Look up the plugin index that registered `verb` as a subcommand.
    #[must_use]
    pub fn resolve_subcommand(&self, verb: &str) -> Option<usize> {
        self.subcommands.get(verb).copied()
    }

    /// Look up the plugin index for `name`, falling back to the
    /// default runner if no exact match exists.
    #[must_use]
    pub fn resolve_runner(&self, name: &str) -> Option<usize> {
        self.runners.get(name).copied().or(self.default_runner)
    }

    /// The runner name of the plugin marked `default: true`, if any.
    #[must_use]
    pub fn default_runner_name(&self) -> Option<&str> {
        let idx = self.default_runner?;
        self.runners
            .iter()
            .find_map(|(name, &i)| (i == idx).then_some(name.as_str()))
    }

    /// All registered subcommand verbs, sorted alphabetically.
    pub fn available_subcommands(&self) -> impl Iterator<Item = &str> {
        self.subcommands.keys().map(String::as_str)
    }

    /// All registered runner names, sorted alphabetically.
    pub fn available_runners(&self) -> impl Iterator<Item = &str> {
        self.runners.keys().map(String::as_str)
    }
}

#[derive(Debug)]
pub struct PluginRegistry {
    plugins: Vec<Arc<LoadedPlugin>>,
    pub capabilities: CapabilityIndex,
}

impl PluginRegistry {
    /// Discover and load plugins from the filesystem, validate each
    /// manifest, and build the capability index.
    pub fn load(config: RegistryConfig) -> Result<Self> {
        let mut plugins: Vec<Arc<LoadedPlugin>> = Vec::new();
        let dll_ext = std::env::consts::DLL_EXTENSION;

        if config.auto_discover {
            for dir in paths::discovery_dirs() {
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
                    p.manifest.validate().map_err(RuntimeError::from)?;
                    plugins.push(Arc::new(p));
                }
            }
        }

        for path in &config.extra_paths {
            let p = LoadedPlugin::load(path, config.host_api.clone())
                .with_context(|| format!("load {}", path.display()))?;
            p.manifest.validate().map_err(RuntimeError::from)?;
            plugins.push(Arc::new(p));
        }

        let capabilities = CapabilityIndex::build(&plugins)?;

        Ok(Self {
            plugins,
            capabilities,
        })
    }

    /// Iterate over every loaded plugin's manifest.
    pub fn manifests(&self) -> impl Iterator<Item = &PluginManifest> {
        self.plugins.iter().map(|p| &p.manifest)
    }

    /// Clone the `Arc` for the plugin at `idx` (returned by the
    /// capability index's resolve methods).
    #[must_use]
    pub fn get(&self, idx: usize) -> Option<Arc<LoadedPlugin>> {
        self.plugins.get(idx).cloned()
    }

}

