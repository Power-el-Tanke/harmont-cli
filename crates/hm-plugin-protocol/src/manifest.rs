//! Plugin manifest types. A plugin advertises what it provides by
//! returning a [`PluginManifest`] from its mandatory `hm_manifest`
//! export at load time.

use std::collections::HashSet;

use schemars::JsonSchema as DeriveJsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::hook::{HookEventKind, HookPhase};

/// JSON Schema fragment (serde-passthrough). Used to validate
/// plugin-specific config blobs and `runner_args`.
pub type JsonSchema = serde_json::Value;

/// Clap-derived JSON describing a subcommand's argument schema.
/// Produced by the SDK helper [`crate::manifest::clap_json_from`]
/// (added in [`hm-plugin-sdk`]).
pub type ClapJson = serde_json::Value;

/// Returned by a plugin's manifest export at load time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, DeriveJsonSchema)]
pub struct PluginManifest {
    /// Must equal [`crate::HM_PLUGIN_API_VERSION`] or the host rejects
    /// the plugin at load time.
    pub api_version: u32,
    /// Stable plugin identifier, e.g. `harmont-docker`. Used as the
    /// key in the registry and in error messages.
    pub name: String,
    pub version: semver::Version,
    pub description: String,
    pub capabilities: Vec<Capability>,
    /// Optional JSON Schema describing plugin-specific configuration
    /// that lives in the project's `.harmont/plugins.toml`.
    pub config_schema: Option<JsonSchema>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, DeriveJsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Capability {
    Subcommand(SubcommandSpec),
    StepExecutor(StepExecutorSpec),
    LifecycleHook(LifecycleHookSpec),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, DeriveJsonSchema)]
pub struct SubcommandSpec {
    /// Top-level verb under `hm`. Two plugins may not claim the
    /// same `verb`.
    pub verb: String,
    pub about: String,
    /// Clap-shaped JSON for argument parsing (the host re-parses on
    /// the plugin's behalf via `clap`).
    pub args_schema: ClapJson,
    pub subcommands: Vec<Self>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, DeriveJsonSchema)]
pub struct StepExecutorSpec {
    /// Matched against `CommandStep.runner` at dispatch time.
    pub runner: String,
    /// At most one plugin may set `default: true`. The host runs that
    /// executor when a step omits `runner`.
    pub default: bool,
    /// Optional JSON Schema for `CommandStep.runner_args`. The host
    /// validates `runner_args` against this schema before dispatch.
    pub step_schema: Option<JsonSchema>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, DeriveJsonSchema)]
pub struct LifecycleHookSpec {
    pub events: Vec<HookEventKind>,
    pub phase: HookPhase,
    pub timeout_ms: u32,
}

#[derive(Debug, Error)]
pub enum ManifestError {
    #[error("plugin '{name}': api_version mismatch (plugin: {found}, host: {expected})")]
    ApiVersion {
        name: String,
        found: u32,
        expected: u32,
    },
    #[error("plugin '{name}': declared no capabilities")]
    NoCapabilities { name: String },
    #[error("plugin '{name}': StepExecutorSpec.runner '{runner}' is empty or contains whitespace")]
    BadRunnerName { name: String, runner: String },
    #[error("plugin '{name}': declared the same subcommand verb twice ('{verb}')")]
    DuplicateSubcommandVerb { name: String, verb: String },
}

impl PluginManifest {
    /// Validate this manifest statically (without consulting other
    /// plugins). Cross-plugin conflicts (e.g. two plugins both claim
    /// `runner: "docker"`) are caught by the registry.
    pub fn validate(&self) -> Result<(), ManifestError> {
        if self.api_version != crate::HM_PLUGIN_API_VERSION {
            return Err(ManifestError::ApiVersion {
                name: self.name.clone(),
                found: self.api_version,
                expected: crate::HM_PLUGIN_API_VERSION,
            });
        }
        if self.capabilities.is_empty() {
            return Err(ManifestError::NoCapabilities {
                name: self.name.clone(),
            });
        }
        let mut seen_verbs: HashSet<&str> = HashSet::new();
        for cap in &self.capabilities {
            match cap {
                Capability::StepExecutor(s) => {
                    if s.runner.trim().is_empty() || s.runner.chars().any(char::is_whitespace) {
                        return Err(ManifestError::BadRunnerName {
                            name: self.name.clone(),
                            runner: s.runner.clone(),
                        });
                    }
                }
                Capability::Subcommand(s) => {
                    if !seen_verbs.insert(s.verb.as_str()) {
                        return Err(ManifestError::DuplicateSubcommandVerb {
                            name: self.name.clone(),
                            verb: s.verb.clone(),
                        });
                    }
                }
                Capability::LifecycleHook(_) => {}
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn valid_manifest() -> PluginManifest {
        PluginManifest {
            api_version: crate::HM_PLUGIN_API_VERSION,
            name: "p".into(),
            version: semver::Version::new(0, 1, 0),
            description: "x".into(),
            capabilities: vec![Capability::StepExecutor(StepExecutorSpec {
                runner: "a".into(),
                default: false,
                step_schema: None,
            })],
            config_schema: None,
        }
    }

    #[test]
    fn validate_accepts_valid_manifest() {
        assert!(valid_manifest().validate().is_ok());
    }

    #[test]
    fn validate_rejects_wrong_api_version() {
        let mut m = valid_manifest();
        m.api_version = 999;
        assert!(matches!(m.validate(), Err(ManifestError::ApiVersion { .. })));
    }

    #[test]
    fn validate_rejects_empty_capabilities() {
        let mut m = valid_manifest();
        m.capabilities.clear();
        assert!(matches!(
            m.validate(),
            Err(ManifestError::NoCapabilities { .. })
        ));
    }

    #[test]
    fn capability_tagged_serialization() {
        let cap = Capability::StepExecutor(StepExecutorSpec {
            runner: "docker".into(),
            default: true,
            step_schema: None,
        });
        let s = serde_json::to_string(&cap).unwrap();
        assert!(s.contains(r#""kind":"step_executor""#), "got: {s}");
        assert!(s.contains(r#""runner":"docker""#), "got: {s}");
    }
}
