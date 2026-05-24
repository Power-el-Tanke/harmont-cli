//! Build-time events. Produced by the orchestrator (host) and fanned
//! out to the output subscriber, lifecycle hooks, and (via the host
//! re-broadcast of `hm_emit_step_log`) any subscriber.

use borsh::{BorshDeserialize, BorshSerialize};
use chrono::{DateTime, Utc};
use schemars::JsonSchema as DeriveJsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::borsh_helpers;

use crate::executor::SnapshotRef;

#[derive(Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize, DeriveJsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StdStream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize, DeriveJsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BuildEvent {
    BuildStart {
        run_id: Uuid,
        plan: PlanSummary,
        #[borsh(
            serialize_with = "borsh_helpers::serialize_datetime",
            deserialize_with = "borsh_helpers::deserialize_datetime"
        )]
        started_at: DateTime<Utc>,
    },
    StepQueued {
        step_id: Uuid,
        key: String,
        chain_idx: usize,
    },
    StepStart {
        step_id: Uuid,
        runner: String,
        image: Option<String>,
    },
    StepLog {
        step_id: Uuid,
        stream: StdStream,
        line: String,
        #[borsh(
            serialize_with = "borsh_helpers::serialize_datetime",
            deserialize_with = "borsh_helpers::deserialize_datetime"
        )]
        ts: DateTime<Utc>,
    },
    StepCacheHit {
        step_id: Uuid,
        key: String,
        tag: String,
    },
    StepEnd {
        step_id: Uuid,
        exit_code: i32,
        duration_ms: u64,
        snapshot: Option<SnapshotRef>,
    },
    /// Emitted when any step in a chain returns non-zero. Carries the
    /// failing step's identity so output plugins can render a precise
    /// diagnostic. Distinct from `StepEnd` (per-step) and `BuildEnd`
    /// (per-run).
    ChainFailed {
        chain_idx: usize,
        failed_step_id: Uuid,
        failed_step_key: String,
        exit_code: i32,
        message: String,
        #[borsh(
            serialize_with = "borsh_helpers::serialize_datetime",
            deserialize_with = "borsh_helpers::deserialize_datetime"
        )]
        ts: DateTime<Utc>,
    },
    BuildEnd {
        exit_code: i32,
        duration_ms: u64,
    },
}

/// Compact summary of the resolved IR included in `BuildStart`. Lets
/// the renderer print a header without needing the full pipeline.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize, DeriveJsonSchema)]
pub struct PlanSummary {
    pub step_count: usize,
    pub chain_count: usize,
    pub default_runner: String,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn build_event_borsh_round_trip() {
        let events = vec![
            BuildEvent::BuildStart {
                run_id: Uuid::nil(),
                plan: PlanSummary {
                    step_count: 3,
                    chain_count: 1,
                    default_runner: "docker".into(),
                },
                started_at: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
            },
            BuildEvent::StepLog {
                step_id: Uuid::nil(),
                stream: StdStream::Stderr,
                line: "hello".into(),
                ts: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 1).unwrap(),
            },
            BuildEvent::ChainFailed {
                chain_idx: 0,
                failed_step_id: Uuid::nil(),
                failed_step_key: "build".into(),
                exit_code: 1,
                message: "fail".into(),
                ts: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 2).unwrap(),
            },
            BuildEvent::BuildEnd {
                exit_code: 0,
                duration_ms: 1234,
            },
        ];
        for event in &events {
            let bytes = borsh::to_vec(event).unwrap();
            let decoded = BuildEvent::try_from_slice(&bytes).unwrap();
            assert_eq!(*event, decoded);
        }
    }
}
