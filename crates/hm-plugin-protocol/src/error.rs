//! Error and exit-info types returned by plugin capability exports.

use borsh::{BorshDeserialize, BorshSerialize};
use schemars::JsonSchema as DeriveJsonSchema;
use serde::{Deserialize, Serialize};

/// Returned by a subcommand plugin from `hm_subcommand_run`. The host
/// translates `exit_code` into the process exit code.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize, DeriveJsonSchema)]
pub struct ExitInfo {
    pub exit_code: i32,
    /// Optional message written to stderr by the host before exit.
    /// Used for the rare case where the plugin wants to add context
    /// beyond the bytes it already streamed via `hm_log`.
    pub message: Option<String>,
}

/// Error returned from any capability export. The host renders these
/// with the `code` field; downstream tooling matches on it.
#[derive(
    Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize, DeriveJsonSchema, thiserror::Error,
)]
#[error("{message}")]
pub struct PluginError {
    /// Stable `snake_case` identifier scoped to the plugin, e.g.
    /// `cloud_auth_token_invalid`. Downstream tooling matches on this.
    pub code: String,
    pub message: String,
    /// Optional URL the host renders alongside the message.
    pub doc_url: Option<String>,
}

impl PluginError {
    #[must_use]
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            doc_url: None,
        }
    }

    #[must_use]
    pub fn with_doc(mut self, url: impl Into<String>) -> Self {
        self.doc_url = Some(url.into());
        self
    }
}
