//! Validates plugin manifests as they're loaded.

// Pedantic nags suppressed scope-wide:
// - `missing_errors_doc`: the only fn returning Result is
//   `validate_standalone`, whose errors are typed as `ManifestError`
//   and each variant carries its own message.
// - `collapsible_if`: keeping the inner `if` separate from the outer
//   `match` makes the validation rules easier to read one-per-line.
// - `single_match_else` style: see same rationale.
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::collapsible_match)]
// The first doc paragraph explains both what `validate_standalone` does
// and what it deliberately leaves to the registry; splitting that
// across paragraphs would scatter the contract.
#![allow(clippy::too_long_first_doc_paragraph)]

use std::collections::HashSet;

use hm_plugin_protocol::{Capability, HM_PLUGIN_API_VERSION, PluginManifest};
use thiserror::Error;

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

/// Returns Ok(()) iff `manifest` passes every check we can do
/// statically (i.e. without consulting other plugins). Cross-plugin
/// conflicts (e.g. two plugins both claim `runner: "docker"`) are
/// caught by [`super::registry`].
///
/// Host-fn validation is no longer needed for stabby-based native
/// plugins: the host API is passed as a trait object, so all methods
/// are always available.
pub fn validate_standalone(manifest: &PluginManifest) -> Result<(), ManifestError> {
    if manifest.api_version != HM_PLUGIN_API_VERSION {
        return Err(ManifestError::ApiVersion {
            name: manifest.name.clone(),
            found: manifest.api_version,
            expected: HM_PLUGIN_API_VERSION,
        });
    }
    if manifest.capabilities.is_empty() {
        return Err(ManifestError::NoCapabilities {
            name: manifest.name.clone(),
        });
    }
    let mut seen_verbs: HashSet<&str> = HashSet::new();
    for cap in &manifest.capabilities {
        match cap {
            Capability::StepExecutor(s) => {
                if s.runner.trim().is_empty() || s.runner.chars().any(char::is_whitespace) {
                    return Err(ManifestError::BadRunnerName {
                        name: manifest.name.clone(),
                        runner: s.runner.clone(),
                    });
                }
            }
            Capability::Subcommand(s) => {
                if !seen_verbs.insert(s.verb.as_str()) {
                    return Err(ManifestError::DuplicateSubcommandVerb {
                        name: manifest.name.clone(),
                        verb: s.verb.clone(),
                    });
                }
            }
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use hm_plugin_protocol::{Capability, StepExecutorSpec};
    use semver::Version;

    #[test]
    fn rejects_wrong_api_version() {
        let m = PluginManifest {
            api_version: 999,
            name: "p".into(),
            version: Version::new(0, 1, 0),
            description: "x".into(),
            capabilities: vec![Capability::StepExecutor(StepExecutorSpec {
                runner: "a".into(),
                default: false,
                step_schema: None,
            })],
            config_schema: None,
        };
        assert!(matches!(
            validate_standalone(&m),
            Err(ManifestError::ApiVersion { .. })
        ));
    }

    #[test]
    fn accepts_minimal_valid_manifest() {
        let m = PluginManifest {
            api_version: HM_PLUGIN_API_VERSION,
            name: "p".into(),
            version: Version::new(0, 1, 0),
            description: "x".into(),
            capabilities: vec![Capability::StepExecutor(StepExecutorSpec {
                runner: "a".into(),
                default: false,
                step_schema: None,
            })],
            config_schema: None,
        };
        assert!(validate_standalone(&m).is_ok());
    }
}
