use thiserror::Error;

/// Exit codes for the CLI.
pub const EXIT_SUCCESS: i32 = 0;
pub const EXIT_BUILD_FAILED: i32 = 1;
pub const EXIT_USAGE: i32 = 2;
pub const EXIT_AUTH: i32 = 3;
pub const EXIT_NETWORK: i32 = 4;
pub const EXIT_API: i32 = 5;
/// Plugin load/validation failure (manifest, conflicts, missing host fns).
/// Shares the same numeric code as `EXIT_API`; named separately so plugin
/// call-sites read clearly.
pub const EXIT_PLUGIN_LOAD: i32 = 5;
/// Plugin runtime failure (panic in capability call, timeout).
pub const EXIT_PLUGIN_RUNTIME: i32 = 6;
/// Pipeline-level invalid configuration (unknown runner, no default executor).
pub const EXIT_PIPELINE_INVALID: i32 = 7;

#[derive(Debug, Error)]
pub enum HmError {
    #[error("not authenticated\n  → run `hm login`")]
    NotAuthenticated,

    #[error("no active organization\n  → run `hm org switch <slug>` or set HARMONT_ORG=<slug>")]
    NoOrganization,

    #[error("API error (HTTP {status}): {message}")]
    Api { status: u16, message: String },

    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error(
        "source archive exceeds the {max_mb} MB limit\n  → trim the source tree or add ignores in .harmontignore"
    )]
    ArchiveTooLarge { max_mb: u64 },

    #[error("pipeline not found: {slug}\n  → list available pipelines with `hm pipeline list`")]
    PipelineNotFound { slug: String },

    /// 403 with `code = pipeline_manual_disabled` from
    /// `POST /api/v0/organizations/{org}/pipelines/{slug}/builds`.
    ///
    /// The message is the literal § 5 shape; `print_error` will prepend
    /// its ✘ glyph and the body carries the `error:` prefix and trailing
    /// `hm run --help` footer the task spec asks for verbatim.
    #[error(
        "error: manual builds are disabled for this pipeline\n  \u{2192} ask the pipeline owner to set allow_manual=True\n\nhm run --help   for more"
    )]
    PipelineManualDisabled,

    #[error("configuration error: {0}")]
    Config(String),

    #[error("docker error: {0}\n  → check that the Docker daemon is running (`docker version`)")]
    Docker(String),

    #[error("pipeline render error: {0}")]
    PipelineRender(String),

    #[error("local scheduler error: {0}")]
    LocalScheduling(String),

    #[error(transparent)]
    PluginRuntime(#[from] hm_plugin_runtime::error::RuntimeError),

    #[error(
        "step '{step_key}' requested runner '{runner}', but no plugin provides it (available: {available:?})"
    )]
    UnknownRunner {
        step_key: String,
        runner: String,
        available: Vec<String>,
    },

    #[error("no default step-executor plugin is registered (need exactly one with default=true)")]
    NoDefaultExecutor,

    #[error(
        "unknown command '{verb}'\n  available: {available:?}\n  fix: `hm plugin install <ref>` to add a plugin that provides this command"
    )]
    UnknownVerb {
        verb: String,
        available: Vec<String>,
    },

    #[error("{0}")]
    Other(#[from] anyhow::Error),
}

/// Coarse error category.
///
/// Each variant maps 1:1 to a single CLI exit code. Categorising via
/// this intermediate enum lets `exit_code` stay a five-arm match
/// regardless of how many error variants share a code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCategory {
    /// Generic failure, exit 1.
    BuildFailed,
    /// User-facing usage / configuration error, exit 2.
    Usage,
    /// Authentication / authorization error, exit 3.
    Auth,
    /// Network or daemon-reachability error, exit 4.
    Network,
    /// Server- or evaluator-side API error, exit 5.
    Api,
    /// Plugin load/manifest/conflict failure, exit 5 (`EXIT_PLUGIN_LOAD`).
    PluginLoad,
    /// Plugin runtime failure, exit 6.
    PluginRuntime,
    /// Pipeline-level invalid config (unknown runner, no default executor), exit 7.
    PipelineInvalid,
}

impl ErrorCategory {
    /// CLI exit code for this category.
    #[must_use]
    pub const fn exit_code(self) -> i32 {
        match self {
            Self::BuildFailed => EXIT_BUILD_FAILED,
            Self::Usage => EXIT_USAGE,
            Self::Auth => EXIT_AUTH,
            Self::Network => EXIT_NETWORK,
            Self::Api | Self::PluginLoad => EXIT_API,
            Self::PluginRuntime => EXIT_PLUGIN_RUNTIME,
            Self::PipelineInvalid => EXIT_PIPELINE_INVALID,
        }
    }
}

/// Map an error to its exit-code category. The four meaningful
/// categories are auth, network, API, and "the build failed" — anything
/// else maps to `EXIT_USAGE`. Keep this aligned with the `EXIT_*`
/// constants at the top of this file.
impl HmError {
    /// Map an error variant to its broad category.
    ///
    /// Patterns are merged by category to satisfy `clippy::match_same_arms`;
    /// the comment after each arm names the variants in source order so
    /// the mapping stays inspectable from this site.
    #[must_use]
    pub const fn category(&self) -> ErrorCategory {
        match self {
            // Auth: NotAuthenticated, Api{401|403}
            Self::NotAuthenticated => ErrorCategory::Auth,
            Self::Api { status, .. } if *status == 401 || *status == 403 => ErrorCategory::Auth,
            // Usage: NoOrganization, ArchiveTooLarge, Config, PipelineRender, UnknownVerb
            Self::NoOrganization
            | Self::ArchiveTooLarge { .. }
            | Self::Config(_)
            | Self::PipelineRender(_)
            | Self::UnknownVerb { .. } => ErrorCategory::Usage,
            // Api (server-side): Api{*}, PipelineNotFound,
            // PipelineManualDisabled, LocalScheduling
            Self::Api { .. }
            | Self::PipelineNotFound { .. }
            | Self::PipelineManualDisabled
            | Self::LocalScheduling(_) => ErrorCategory::Api,
            // Network: Network (reqwest), Docker (daemon unreachable)
            Self::Network(_) | Self::Docker(_) => ErrorCategory::Network,
            // Plugin failures — delegate categorisation to the inner enum.
            Self::PluginRuntime(e) => {
                use hm_plugin_runtime::error::RuntimeError;
                match e {
                    RuntimeError::PluginLoad { .. }
                    | RuntimeError::PluginManifest { .. }
                    | RuntimeError::PluginMissingHostFn { .. }
                    | RuntimeError::PluginConflict { .. } => ErrorCategory::PluginLoad,
                    RuntimeError::PluginPanic { .. }
                    | RuntimeError::PluginTimeout { .. } => ErrorCategory::PluginRuntime,
                }
            }
            // Pipeline-level invalid config (exit 7).
            Self::UnknownRunner { .. } | Self::NoDefaultExecutor => ErrorCategory::PipelineInvalid,
            // Generic build failure: anyhow-wrapped errors propagate here.
            Self::Other(_) => ErrorCategory::BuildFailed,
        }
    }

    /// Map an error to its CLI exit code.
    #[must_use]
    pub const fn exit_code(&self) -> i32 {
        self.category().exit_code()
    }
}

#[cfg(test)]
mod tests {
    use super::{ErrorCategory, HmError};

    /// The § 5 error shape is part of the user-facing contract: scripts
    /// and humans both rely on the exact wording, so the rendered string
    /// is checked byte-for-byte rather than with a substring match.
    #[test]
    fn pipeline_manual_disabled_renders_section5_shape() {
        let s = format!("{}", HmError::PipelineManualDisabled);
        assert_eq!(
            s,
            "error: manual builds are disabled for this pipeline\n  \u{2192} ask the pipeline owner to set allow_manual=True\n\nhm run --help   for more"
        );
    }

    #[test]
    fn pipeline_manual_disabled_is_api_category() {
        assert_eq!(
            HmError::PipelineManualDisabled.category(),
            ErrorCategory::Api
        );
        assert_eq!(HmError::PipelineManualDisabled.exit_code(), super::EXIT_API);
    }
}
