//! Wire types passed to and returned by step-executor plugins.

use std::collections::BTreeMap;

use borsh::{BorshDeserialize, BorshSerialize};
use schemars::JsonSchema as DeriveJsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::ir::CommandStep;

/// Opaque archive handle. The plugin streams bytes via
/// `hm_archive_read(id, offset, max)`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize, Serialize, Deserialize, DeriveJsonSchema,
    derive_more::From, derive_more::Deref, derive_more::Display,
)]
#[serde(transparent)]
pub struct ArchiveId(pub Uuid);

/// Opaque snapshot reference. For the docker plugin this is an image
/// tag; other plugins are free to encode their own format. The host
/// never inspects the contents.
#[derive(
    Debug, Clone, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize, Serialize, Deserialize, DeriveJsonSchema,
    derive_more::From, derive_more::Deref, derive_more::Display,
)]
#[serde(transparent)]
pub struct SnapshotRef(pub String);

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize, DeriveJsonSchema)]
pub struct ArtifactRef {
    pub key: String,
    pub mime: String,
    pub size_bytes: u64,
}

/// Host-decided cache outcome. The executor honours this; it does
/// not re-decide.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize, DeriveJsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CacheDecision {
    /// Boot from `tag`; skip running `cmd`.
    Hit { tag: SnapshotRef },
    /// Run `cmd`; on success, commit to `tag` and report it back in
    /// `StepResult::committed_snapshot`.
    MissBuildAs { tag: SnapshotRef },
    /// Run `cmd`; do not commit.
    MissNoCommit,
}

#[derive(Debug, Clone, PartialEq, BorshSerialize, BorshDeserialize, Serialize, Deserialize, DeriveJsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExecutorInput {
    pub step: CommandStep,
    pub workspace_archive_id: ArchiveId,
    pub env: BTreeMap<String, String>,
    pub workdir: String,
    pub run_id: Uuid,
    pub step_id: Uuid,
    /// Host-decided; see [`CacheDecision`]. Every step has one.
    pub cache_lookup: CacheDecision,

    /// Snapshot tag of the upstream step in this chain (if any),
    /// or of the chain-fork parent. When `Some`, the executor must
    /// boot from this tag rather than `step.image` — that's how
    /// chain-stepwise filesystem inheritance works: the orchestrator
    /// commits a snapshot between steps and the next step boots from
    /// it.
    #[serde(default)]
    pub parent_snapshot: Option<SnapshotRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize, DeriveJsonSchema)]
pub struct StepResult {
    pub exit_code: i32,
    /// `Some(tag)` when the executor wrote a snapshot for this step
    /// (typically only on `CacheDecision::MissBuildAs`).
    pub committed_snapshot: Option<SnapshotRef>,
    pub artifacts: Vec<ArtifactRef>,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::ir::Cache;

    #[test]
    fn executor_input_borsh_round_trip() {
        let input = ExecutorInput {
            step: CommandStep {
                key: "build".into(),
                label: Some("Build app".into()),
                cmd: "cargo build".into(),
                builds_in: None,
                image: Some("rust:latest".into()),
                env: Some({
                    let mut m = BTreeMap::new();
                    m.insert("RUST_LOG".into(), "debug".into());
                    m
                }),
                timeout_seconds: Some(300),
                cache: Some(Cache {
                    policy: "content".into(),
                    key: Some("build-cache".into()),
                }),
                runner: Some("docker".into()),
                runner_args: Some(crate::Value::Object({
                    let mut m = BTreeMap::new();
                    m.insert("privileged".into(), crate::Value::Bool(false));
                    m
                })),
            },
            workspace_archive_id: ArchiveId(Uuid::nil()),
            env: BTreeMap::new(),
            workdir: "/app".into(),
            run_id: Uuid::nil(),
            step_id: Uuid::nil(),
            cache_lookup: CacheDecision::MissBuildAs {
                tag: SnapshotRef("snap:abc".into()),
            },
            parent_snapshot: Some(SnapshotRef("snap:parent".into())),
        };
        let bytes = borsh::to_vec(&input).unwrap();
        let decoded = ExecutorInput::try_from_slice(&bytes).unwrap();
        assert_eq!(input, decoded);
    }

    #[test]
    fn step_result_borsh_round_trip() {
        let result = StepResult {
            exit_code: 0,
            committed_snapshot: Some(SnapshotRef("snap:123".into())),
            artifacts: vec![ArtifactRef {
                key: "binary".into(),
                mime: "application/octet-stream".into(),
                size_bytes: 1024,
            }],
        };
        let bytes = borsh::to_vec(&result).unwrap();
        let decoded = StepResult::try_from_slice(&bytes).unwrap();
        assert_eq!(result, decoded);
    }
}
