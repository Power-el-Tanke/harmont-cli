//! Build-event rendering for human-readable and JSON output modes.
//!
//! The renderer lives in-process and owns its step-key map directly.

use std::collections::HashMap;

use hm_plugin_protocol::BuildEvent;
use uuid::Uuid;

/// Stateful renderer that maps step UUIDs to human-friendly keys and
/// formats [`BuildEvent`]s for either human or JSON output.
pub(crate) struct BuildEventRenderer {
    step_keys: HashMap<Uuid, String>,
}

impl BuildEventRenderer {
    pub(crate) fn new() -> Self {
        Self {
            step_keys: HashMap::new(),
        }
    }

    /// Look up the human-readable key for a step, falling back to `"?"`.
    fn step_key_for(&self, id: Uuid) -> &str {
        self.step_keys
            .get(&id)
            .map(String::as_str)
            .unwrap_or("?")
    }

    /// Render a [`BuildEvent`] as human-readable bytes (for stderr).
    pub(crate) fn render_human(&mut self, ev: &BuildEvent) -> Vec<u8> {
        match ev {
            BuildEvent::BuildStart { plan, .. } => format!(
                "build: {} steps in {} chain(s)\n",
                plan.step_count, plan.chain_count
            )
            .into_bytes(),
            BuildEvent::StepQueued { step_id, key, .. } => {
                self.step_keys.insert(*step_id, key.clone());
                Vec::new()
            }
            BuildEvent::StepStart {
                step_id,
                runner,
                image,
            } => {
                let key = self.step_key_for(*step_id);
                let line = match image {
                    Some(img) => format!("[{key}] start (runner={runner} image={img})\n"),
                    None => format!("[{key}] start (runner={runner})\n"),
                };
                line.into_bytes()
            }
            BuildEvent::StepLog { step_id, line, .. } => {
                let key = self.step_key_for(*step_id);
                format!("[{key}] {line}\n").into_bytes()
            }
            BuildEvent::StepCacheHit { step_id, tag, .. } => {
                let key = self.step_key_for(*step_id);
                format!("[{key}] cache hit ({tag})\n").into_bytes()
            }
            BuildEvent::StepEnd {
                step_id,
                exit_code,
                duration_ms,
                ..
            } => {
                let key = self.step_key_for(*step_id);
                format!("[{key}] end exit={exit_code} duration={duration_ms}ms\n").into_bytes()
            }
            BuildEvent::BuildEnd {
                exit_code,
                duration_ms,
            } => format!("build: end exit={exit_code} duration={duration_ms}ms\n").into_bytes(),
            BuildEvent::ChainFailed {
                chain_idx,
                failed_step_key,
                exit_code,
                message,
                ..
            } => format!(
                "chain {chain_idx}: FAILED at step '{failed_step_key}' (exit={exit_code}): {message}\n"
            )
            .into_bytes(),
        }
    }

    /// Render a [`BuildEvent`] as a JSON line (for stdout).
    pub(crate) fn render_json(&self, ev: &BuildEvent) -> Vec<u8> {
        let mut buf = serde_json::to_vec(ev).expect("BuildEvent serialization is infallible");
        buf.push(b'\n');
        buf
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use hm_plugin_protocol::{PlanSummary, StdStream};

    #[test]
    fn build_start_renders_step_and_chain_counts() {
        let mut r = BuildEventRenderer::new();
        let ev = BuildEvent::BuildStart {
            run_id: Uuid::nil(),
            plan: PlanSummary {
                step_count: 3,
                chain_count: 2,
                default_runner: "docker".into(),
            },
            started_at: chrono::Utc::now(),
        };
        let s = String::from_utf8(r.render_human(&ev)).unwrap();
        assert!(s.contains("3 steps"));
        assert!(s.contains("2 chain"));
    }

    #[test]
    fn step_log_renders_with_prefix_after_step_queued_recorded_key() {
        let mut r = BuildEventRenderer::new();
        let step_id = Uuid::new_v4();
        r.render_human(&BuildEvent::StepQueued {
            step_id,
            key: "build".into(),
            chain_idx: 0,
        });
        let ev = BuildEvent::StepLog {
            step_id,
            stream: StdStream::Stdout,
            line: "hello".into(),
            ts: chrono::Utc::now(),
        };
        let s = String::from_utf8(r.render_human(&ev)).unwrap();
        assert_eq!(s, "[build] hello\n");
    }

    #[test]
    fn step_log_with_unknown_key_renders_question_mark() {
        let mut r = BuildEventRenderer::new();
        let s = String::from_utf8(r.render_human(&BuildEvent::StepLog {
            step_id: Uuid::new_v4(),
            stream: StdStream::Stdout,
            line: "x".into(),
            ts: chrono::Utc::now(),
        }))
        .unwrap();
        assert!(s.starts_with("[?] "));
    }
}
