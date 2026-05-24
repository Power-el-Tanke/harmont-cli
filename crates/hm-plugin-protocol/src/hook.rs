//! Lifecycle hook wire types.

use borsh::{BorshDeserialize, BorshSerialize};
use schemars::JsonSchema as DeriveJsonSchema;
use serde::{Deserialize, Serialize};

use crate::events::BuildEvent;

/// Hook entry-point input. The host wraps a `BuildEvent` and tells
/// the plugin which phase this call is.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize, DeriveJsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HookEvent {
    pub event: BuildEvent,
    pub phase: HookPhase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize, DeriveJsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HookPhase {
    /// May return [`HookOutcome::Abort`] to fail the build.
    Before,
    /// Read-only; the return value is discarded.
    After,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize, DeriveJsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HookOutcome {
    /// Continue the build.
    Continue,
    /// Abort the build. Only honoured for `phase: Before`; ignored on
    /// `After` (with a host-side warning).
    Abort { reason: String },
}

/// Subset of [`crate::hook::HookEvent`] discriminants used at manifest time.
///
/// The manifest declares *what* events the plugin wants, not the per-event
/// payload. Kept in this file so plugin authors only import one module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize, Serialize, Deserialize, DeriveJsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HookEventKind {
    BuildStart,
    StepQueued,
    StepStart,
    StepLog,
    StepCacheHit,
    StepEnd,
    BuildEnd,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use uuid::Uuid;

    #[test]
    fn hook_event_borsh_round_trip() {
        let hook = HookEvent {
            event: crate::events::BuildEvent::BuildEnd {
                exit_code: 0,
                duration_ms: 500,
            },
            phase: HookPhase::After,
        };
        let bytes = borsh::to_vec(&hook).unwrap();
        let decoded = HookEvent::try_from_slice(&bytes).unwrap();
        assert_eq!(hook, decoded);
    }

    #[test]
    fn hook_event_with_datetime_borsh_round_trip() {
        let hook = HookEvent {
            event: crate::events::BuildEvent::BuildStart {
                run_id: Uuid::nil(),
                plan: crate::events::PlanSummary {
                    step_count: 1,
                    chain_count: 1,
                    default_runner: "docker".into(),
                },
                started_at: Utc.with_ymd_and_hms(2026, 5, 23, 12, 0, 0).unwrap(),
            },
            phase: HookPhase::Before,
        };
        let bytes = borsh::to_vec(&hook).unwrap();
        let decoded = HookEvent::try_from_slice(&bytes).unwrap();
        assert_eq!(hook, decoded);
    }

    #[test]
    fn hook_outcome_borsh_round_trip() {
        for outcome in [
            HookOutcome::Continue,
            HookOutcome::Abort { reason: "bad".into() },
        ] {
            let bytes = borsh::to_vec(&outcome).unwrap();
            let decoded = HookOutcome::try_from_slice(&bytes).unwrap();
            assert_eq!(outcome, decoded);
        }
    }
}
