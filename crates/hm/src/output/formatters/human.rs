//! Human-readable BuildEvent formatter — writes prefixed step logs and
//! brief status lines to stderr. Moved from the standalone
//! `hm-plugin-output-human` WASM crate into the `hm` binary so the
//! built-in formatter does not pay a WASM round-trip per event.

use hm_plugin_protocol::BuildEvent;
use std::collections::HashMap;
use std::io::Write;
use uuid::Uuid;

#[derive(Debug, Default)]
pub struct Human {
    step_keys: HashMap<Uuid, String>,
}

impl Human {
    pub fn on_event(&mut self, ev: &BuildEvent) {
        let bytes = self.render(ev);
        if !bytes.is_empty() {
            let _ = std::io::stderr().write_all(&bytes);
        }
    }

    pub fn finalize(&mut self) {}

    fn render(&mut self, ev: &BuildEvent) -> Vec<u8> {
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
            BuildEvent::StepStart { step_id, runner, image } => {
                let key = self.key_for(*step_id);
                let line = match image {
                    Some(img) => format!("[{key}] start (runner={runner} image={img})\n"),
                    None => format!("[{key}] start (runner={runner})\n"),
                };
                line.into_bytes()
            }
            BuildEvent::StepLog { step_id, line, .. } => {
                let key = self.key_for(*step_id);
                format!("[{key}] {line}\n").into_bytes()
            }
            BuildEvent::StepCacheHit { step_id, tag, .. } => {
                let key = self.key_for(*step_id);
                format!("[{key}] cache hit ({tag})\n").into_bytes()
            }
            BuildEvent::StepEnd { step_id, exit_code, duration_ms, .. } => {
                let key = self.key_for(*step_id);
                format!("[{key}] end exit={exit_code} duration={duration_ms}ms\n").into_bytes()
            }
            BuildEvent::BuildEnd { exit_code, duration_ms } => format!(
                "build: end exit={exit_code} duration={duration_ms}ms\n"
            )
            .into_bytes(),
            BuildEvent::ChainFailed {
                chain_idx, failed_step_key, exit_code, message, ..
            } => format!(
                "chain {chain_idx}: FAILED at step '{failed_step_key}' (exit={exit_code}): {message}\n"
            )
            .into_bytes(),
        }
    }

    fn key_for(&self, id: Uuid) -> String {
        self.step_keys.get(&id).cloned().unwrap_or_else(|| "?".to_string())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use hm_plugin_protocol::{PlanSummary, StdStream};

    #[test]
    fn build_start_renders_step_and_chain_counts() {
        let mut h = Human::default();
        let s = String::from_utf8(h.render(&BuildEvent::BuildStart {
            run_id: Uuid::nil(),
            plan: PlanSummary {
                step_count: 3,
                chain_count: 2,
                default_runner: "docker".into(),
            },
            started_at: chrono::Utc::now(),
        }))
        .unwrap();
        assert!(s.contains("3 steps"));
        assert!(s.contains("2 chain"));
    }

    #[test]
    fn step_log_renders_with_prefix_after_step_queued_recorded_key() {
        let mut h = Human::default();
        let step_id = Uuid::new_v4();
        h.render(&BuildEvent::StepQueued {
            step_id,
            key: "build".into(),
            chain_idx: 0,
        });
        let s = String::from_utf8(h.render(&BuildEvent::StepLog {
            step_id,
            stream: StdStream::Stdout,
            line: "hello".into(),
            ts: chrono::Utc::now(),
        }))
        .unwrap();
        assert_eq!(s, "[build] hello\n");
    }

    #[test]
    fn step_log_with_unknown_key_renders_question_mark() {
        let mut h = Human::default();
        let s = String::from_utf8(h.render(&BuildEvent::StepLog {
            step_id: Uuid::new_v4(),
            stream: StdStream::Stdout,
            line: "x".into(),
            ts: chrono::Utc::now(),
        }))
        .unwrap();
        assert!(s.starts_with("[?] "));
    }
}
