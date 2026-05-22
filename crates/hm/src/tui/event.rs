//! Host-only event vocabulary fed to `AppState::apply`. Translated
//! from wire `BuildEvent` (local + cloud sources) and dev-daemon
//! status diffs at the adapter boundary.

use chrono::{DateTime, Utc};
use hm_plugin_protocol::{PlanSummary, StdStream};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeployState {
    Starting,
    Healthy,
    Unhealthy,
    Restarting,
    Stopped,
}

#[derive(Debug, Clone)]
pub enum TuiEvent {
    BuildStart {
        run_id: Uuid,
        plan: PlanSummary,
        started_at: DateTime<Utc>,
    },
    ChainQueued {
        chain_idx: usize,
        label: String,
        parent: Option<usize>,
    },
    StepStart {
        step_id: Uuid,
        chain_idx: usize,
        runner: String,
        image: Option<String>,
        label: String,
    },
    StepLog {
        step_id: Uuid,
        stream: StdStream,
        line: String,
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
    },
    ChainFailed {
        chain_idx: usize,
        failed_step_key: String,
        exit_code: i32,
        message: String,
    },
    BuildEnd {
        exit_code: i32,
        duration_ms: u64,
    },

    DeployStatus {
        deploy_id: String,
        label: String,
        state: DeployState,
        restarts: u32,
        uptime_ms: u64,
    },
    DeployLog {
        deploy_id: String,
        stream: StdStream,
        line: String,
        ts: DateTime<Utc>,
    },

    /// Synthetic event the adapter inserts when it has dropped one or
    /// more `StepLog` events due to backpressure. The reducer renders
    /// a single dim "events dropped" line in the affected step.
    Lagged { dropped: u64 },
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn deploy_state_eq() {
        assert_eq!(DeployState::Healthy, DeployState::Healthy);
        assert_ne!(DeployState::Healthy, DeployState::Unhealthy);
    }

    #[test]
    fn step_log_is_clone() {
        let ev = TuiEvent::StepLog {
            step_id: Uuid::nil(),
            stream: StdStream::Stdout,
            line: "hi".into(),
            ts: chrono::Utc::now(),
        };
        let cloned = ev.clone();
        assert!(matches!(ev, TuiEvent::StepLog { .. }));
        assert!(matches!(cloned, TuiEvent::StepLog { .. }));
    }
}
