//! Honor a CacheDecision. Returns whether the plugin should run the
//! step's command (false ⇒ cache hit, just record).

use hm_plugin_protocol::{CacheDecision, SnapshotRef};

#[derive(Debug, Clone)]
pub(crate) struct DecisionPlan {
    pub(crate) run_command: bool,
    pub(crate) commit_to: Option<SnapshotRef>,
    pub(crate) hit_tag: Option<SnapshotRef>,
}

#[must_use]
pub(crate) fn plan(decision: &CacheDecision) -> DecisionPlan {
    match decision {
        CacheDecision::Hit { tag } => DecisionPlan {
            run_command: false,
            commit_to: None,
            hit_tag: Some(tag.clone()),
        },
        CacheDecision::MissBuildAs { tag } => DecisionPlan {
            run_command: true,
            commit_to: Some(tag.clone()),
            hit_tag: None,
        },
        CacheDecision::MissNoCommit => DecisionPlan {
            run_command: true,
            commit_to: None,
            hit_tag: None,
        },
    }
}
