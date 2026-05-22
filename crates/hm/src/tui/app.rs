//! Mission Control reducer. Pure: `AppState::apply(TuiEvent)` yields
//! a new state without touching the terminal. All ratatui widgets are
//! immediate-mode renders over this state.

use std::collections::BTreeMap;
use std::collections::VecDeque;
use std::time::Instant;

use chrono::{DateTime, Utc};
use hm_plugin_protocol::{PlanSummary, StdStream};
use uuid::Uuid;

use super::event::{DeployState, TuiEvent};

const LOG_RING_CAPACITY: usize = 2000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepStatus {
    Queued,
    Running,
    CachedHit,
    Passed,
    Failed,
}

#[derive(Debug, Clone)]
pub struct Step {
    pub id: Uuid,
    pub chain_idx: usize,
    pub label: String,
    pub status: StepStatus,
    pub started_at: Option<DateTime<Utc>>,
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct Chain {
    pub idx: usize,
    pub label: String,
    pub parent: Option<usize>,
    pub steps: Vec<Uuid>,
    pub deploy_state: Option<DeployState>,
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub ts: DateTime<Utc>,
    pub stream: StdStream,
    pub line: String,
}

#[derive(Debug)]
pub struct StepLogBuffer {
    pub entries: VecDeque<LogEntry>,
    pub dropped: u64,
}

impl Default for StepLogBuffer {
    fn default() -> Self {
        Self {
            entries: VecDeque::with_capacity(LOG_RING_CAPACITY),
            dropped: 0,
        }
    }
}

impl StepLogBuffer {
    pub fn push(&mut self, e: LogEntry) {
        if self.entries.len() == LOG_RING_CAPACITY {
            self.entries.pop_front();
        }
        self.entries.push_back(e);
    }
}

#[derive(Debug, Default)]
pub struct AppState {
    pub run_id: Option<Uuid>,
    pub plan: Option<PlanSummary>,
    pub started_at: Option<DateTime<Utc>>,
    pub ended_at: Option<DateTime<Utc>>,
    pub exit_code: Option<i32>,
    pub chains: Vec<Chain>,
    pub steps: BTreeMap<Uuid, Step>,
    pub logs: BTreeMap<Uuid, StepLogBuffer>,
    pub focused_chain: usize,
    pub fail_message: Option<String>,
}

impl AppState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn apply(&mut self, event: TuiEvent) {
        match event {
            TuiEvent::BuildStart { run_id, plan, started_at } => {
                self.run_id = Some(run_id);
                self.plan = Some(plan);
                self.started_at = Some(started_at);
            }
            TuiEvent::ChainQueued { chain_idx, label, parent } => {
                while self.chains.len() <= chain_idx {
                    self.chains.push(Chain {
                        idx: self.chains.len(),
                        label: String::new(),
                        parent: None,
                        steps: vec![],
                        deploy_state: None,
                    });
                }
                let c = &mut self.chains[chain_idx];
                c.label = label;
                c.parent = parent;
            }
            TuiEvent::StepStart { step_id, chain_idx, runner: _, image: _, label } => {
                self.steps.insert(step_id, Step {
                    id: step_id,
                    chain_idx,
                    label,
                    status: StepStatus::Running,
                    started_at: Some(Utc::now()),
                    duration_ms: None,
                });
                while self.chains.len() <= chain_idx {
                    self.chains.push(Chain {
                        idx: self.chains.len(),
                        label: String::new(),
                        parent: None,
                        steps: vec![],
                        deploy_state: None,
                    });
                }
                self.chains[chain_idx].steps.push(step_id);
            }
            TuiEvent::StepLog { step_id, stream, line, ts } => {
                let buf = self.logs.entry(step_id).or_default();
                buf.push(LogEntry { ts, stream, line });
            }
            TuiEvent::StepCacheHit { step_id, .. } => {
                if let Some(s) = self.steps.get_mut(&step_id) {
                    s.status = StepStatus::CachedHit;
                }
            }
            TuiEvent::StepEnd { step_id, exit_code, duration_ms } => {
                if let Some(s) = self.steps.get_mut(&step_id) {
                    if s.status != StepStatus::CachedHit {
                        s.status = if exit_code == 0 {
                            StepStatus::Passed
                        } else {
                            StepStatus::Failed
                        };
                    }
                    s.duration_ms = Some(duration_ms);
                }
            }
            TuiEvent::ChainFailed { chain_idx: _, failed_step_key, exit_code, message } => {
                self.fail_message = Some(format!(
                    "{failed_step_key} exited {exit_code}: {message}"
                ));
            }
            TuiEvent::BuildEnd { exit_code, duration_ms: _ } => {
                self.exit_code = Some(exit_code);
                self.ended_at = Some(Utc::now());
            }
            TuiEvent::DeployStatus { deploy_id, label, state, restarts: _, uptime_ms: _ } => {
                let chain_idx = self.find_or_create_deploy_chain(&deploy_id, &label);
                self.chains[chain_idx].deploy_state = Some(state);
            }
            TuiEvent::DeployLog { deploy_id, stream, line, ts } => {
                let chain_idx = self.find_or_create_deploy_chain(&deploy_id, &deploy_id);
                let step_id = uuid_from_deploy_id(&deploy_id);
                if !self.steps.contains_key(&step_id) {
                    self.steps.insert(step_id, Step {
                        id: step_id,
                        chain_idx,
                        label: deploy_id.clone(),
                        status: StepStatus::Running,
                        started_at: Some(ts),
                        duration_ms: None,
                    });
                    self.chains[chain_idx].steps.push(step_id);
                }
                let buf = self.logs.entry(step_id).or_default();
                buf.push(LogEntry { ts, stream, line });
            }
            TuiEvent::Lagged { dropped } => {
                if let Some(focused_step) = self.focused_step_id() {
                    let buf = self.logs.entry(focused_step).or_default();
                    buf.dropped += dropped;
                }
            }
        }
    }

    pub fn focused_step_id(&self) -> Option<Uuid> {
        self.chains
            .get(self.focused_chain)
            .and_then(|c| c.steps.last().copied())
    }

    pub fn cycle_focus(&mut self, delta: isize) {
        if self.chains.is_empty() {
            return;
        }
        let len = self.chains.len() as isize;
        let next = (self.focused_chain as isize + delta).rem_euclid(len);
        self.focused_chain = next as usize;
    }

    fn find_or_create_deploy_chain(&mut self, deploy_id: &str, label: &str) -> usize {
        if let Some(idx) = self.chains.iter().position(|c| c.label == deploy_id) {
            return idx;
        }
        let idx = self.chains.len();
        self.chains.push(Chain {
            idx,
            label: label.to_string(),
            parent: None,
            steps: vec![],
            deploy_state: None,
        });
        idx
    }
}

fn uuid_from_deploy_id(deploy_id: &str) -> Uuid {
    Uuid::new_v5(&Uuid::NAMESPACE_OID, deploy_id.as_bytes())
}

#[allow(dead_code)]
fn _instant_unused_marker() -> Instant { Instant::now() }

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use hm_plugin_protocol::PlanSummary;

    fn nil() -> Uuid { Uuid::nil() }

    fn plan(n: usize) -> PlanSummary {
        PlanSummary {
            step_count: n,
            chain_count: n,
            default_runner: "docker".into(),
        }
    }

    #[test]
    fn build_start_sets_metadata() {
        let mut s = AppState::new();
        s.apply(TuiEvent::BuildStart {
            run_id: nil(),
            plan: plan(3),
            started_at: Utc::now(),
        });
        assert!(s.run_id.is_some());
        assert!(s.plan.is_some());
    }

    #[test]
    fn chain_queued_grows_chains() {
        let mut s = AppState::new();
        s.apply(TuiEvent::ChainQueued {
            chain_idx: 2,
            label: "c2".into(),
            parent: None,
        });
        assert_eq!(s.chains.len(), 3);
        assert_eq!(s.chains[2].label, "c2");
    }

    #[test]
    fn step_lifecycle_transitions_status() {
        let mut s = AppState::new();
        let sid = Uuid::new_v4();
        s.apply(TuiEvent::StepStart {
            step_id: sid,
            chain_idx: 0,
            runner: "docker".into(),
            image: None,
            label: "test".into(),
        });
        assert_eq!(s.steps[&sid].status, StepStatus::Running);
        s.apply(TuiEvent::StepEnd {
            step_id: sid,
            exit_code: 0,
            duration_ms: 42,
        });
        assert_eq!(s.steps[&sid].status, StepStatus::Passed);
        assert_eq!(s.steps[&sid].duration_ms, Some(42));
    }

    #[test]
    fn cache_hit_sticks_through_step_end() {
        let mut s = AppState::new();
        let sid = Uuid::new_v4();
        s.apply(TuiEvent::StepStart {
            step_id: sid,
            chain_idx: 0,
            runner: "docker".into(),
            image: None,
            label: "build".into(),
        });
        s.apply(TuiEvent::StepCacheHit {
            step_id: sid,
            key: "k".into(),
            tag: "t".into(),
        });
        s.apply(TuiEvent::StepEnd {
            step_id: sid,
            exit_code: 0,
            duration_ms: 1,
        });
        assert_eq!(s.steps[&sid].status, StepStatus::CachedHit);
    }

    #[test]
    fn failed_step_records_status() {
        let mut s = AppState::new();
        let sid = Uuid::new_v4();
        s.apply(TuiEvent::StepStart {
            step_id: sid,
            chain_idx: 0,
            runner: "docker".into(),
            image: None,
            label: "test".into(),
        });
        s.apply(TuiEvent::StepEnd {
            step_id: sid,
            exit_code: 1,
            duration_ms: 9,
        });
        assert_eq!(s.steps[&sid].status, StepStatus::Failed);
    }

    #[test]
    fn log_buffer_caps_at_ring_capacity() {
        let mut s = AppState::new();
        let sid = Uuid::new_v4();
        for i in 0..(LOG_RING_CAPACITY + 50) {
            s.apply(TuiEvent::StepLog {
                step_id: sid,
                stream: StdStream::Stdout,
                line: format!("L{i}"),
                ts: Utc::now(),
            });
        }
        assert_eq!(s.logs[&sid].entries.len(), LOG_RING_CAPACITY);
        assert_eq!(s.logs[&sid].entries.front().unwrap().line, format!("L{}", 50));
    }

    #[test]
    fn focus_cycles_modulo_chains() {
        let mut s = AppState::new();
        for i in 0..3 {
            s.apply(TuiEvent::ChainQueued {
                chain_idx: i,
                label: format!("c{i}"),
                parent: None,
            });
        }
        s.cycle_focus(1);
        assert_eq!(s.focused_chain, 1);
        s.cycle_focus(-1);
        assert_eq!(s.focused_chain, 0);
        s.cycle_focus(-1);
        assert_eq!(s.focused_chain, 2);
    }

    #[test]
    fn deploy_status_creates_deploy_chain() {
        let mut s = AppState::new();
        s.apply(TuiEvent::DeployStatus {
            deploy_id: "db".into(),
            label: "db".into(),
            state: DeployState::Healthy,
            restarts: 0,
            uptime_ms: 1000,
        });
        assert_eq!(s.chains.len(), 1);
        assert_eq!(s.chains[0].deploy_state, Some(DeployState::Healthy));
    }
}
