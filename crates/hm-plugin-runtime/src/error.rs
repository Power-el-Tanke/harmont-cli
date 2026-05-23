use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("plugin '{name}' failed to load from {path}: {reason}")]
    PluginLoad {
        name: String,
        path: PathBuf,
        reason: String,
        doc_url: &'static str,
    },

    #[error("plugin '{name}': API version mismatch (plugin={found_api}, host={expected_api})")]
    PluginManifest {
        name: String,
        expected_api: u32,
        found_api: u32,
    },

    #[error(
        "plugin '{name}': required host fn '{fn_name}' is unavailable (this hm build is too old; needs >= {min_hm_version})"
    )]
    PluginMissingHostFn {
        name: String,
        fn_name: String,
        min_hm_version: semver::Version,
    },

    #[error("plugin '{name}' panicked during '{capability}': {message}")]
    PluginPanic {
        name: String,
        capability: String,
        message: String,
    },

    #[error("plugin '{name}' timed out after {after_ms}ms during '{capability}'")]
    PluginTimeout {
        name: String,
        capability: String,
        after_ms: u32,
    },

    #[error("plugin conflict: both '{plugin_a}' and '{plugin_b}' claim '{verb}'")]
    PluginConflict {
        verb: String,
        plugin_a: String,
        plugin_b: String,
    },
}
