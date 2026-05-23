//! Wire types used as host-function arguments and return values.
//! Plugins import these to talk to the hm host fns; the host imports
//! them to expose those fns.

use schemars::JsonSchema as DeriveJsonSchema;
use serde::{Deserialize, Serialize};

use crate::executor::ArchiveId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, DeriveJsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, DeriveJsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum KvScope {
    /// Per-plugin, persistent across builds. Stored in
    /// `~/.config/harmont/state/<plugin-name>.kv`.
    Plugin,
    /// Per-build, in memory. Lost when the build ends.
    Build,
    /// Per-step, in memory. Lost when the step ends.
    Step,
}

/// Host-fn argument struct for the corresponding `hm_archive_read` host function.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchiveReadArgs {
    pub id: ArchiveId,
    pub offset: u64,
    pub max: u64,
}
