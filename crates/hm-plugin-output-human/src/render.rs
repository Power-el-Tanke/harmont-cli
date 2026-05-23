//! Pure-function rendering of BuildEvents to stderr bytes.
//!
//! Step keys are tracked per-plugin instance because the wire
//! BuildEvents carry step_id (Uuid) only; the plugin builds a
//! step_id → key map from the StepQueued events it sees.

use hm_plugin_protocol::BuildEvent;
use std::collections::HashMap;
use std::sync::Mutex;
use uuid::Uuid;

static STEP_KEYS: Mutex<HmKeyMap> = Mutex::new(HmKeyMap { inner: None });

struct HmKeyMap {
    inner: Option<HashMap<Uuid, String>>,
}

impl HmKeyMap {
    fn ensure(&mut self) -> &mut HashMap<Uuid, String> {
        self.inner.get_or_insert_with(HashMap::new)
    }
}

fn record_step_key(id: Uuid, key: String) {
    let Ok(mut g) = STEP_KEYS.lock() else { return };
    g.ensure().insert(id, key);
}

fn step_key_for(id: Uuid) -> String {
    STEP_KEYS
        .lock()
        .ok()
        .and_then(|g| g.inner.as_ref().and_then(|m| m.get(&id).cloned()))
        .unwrap_or_else(|| "?".to_string())
}

pub(crate) fn render(ev: &BuildEvent) -> Vec<u8> {
    match ev {
        BuildEvent::BuildStart { plan, .. } => format!(
            "build: {} steps in {} chain(s)\n",
            plan.step_count, plan.chain_count
        )
        .into_bytes(),
        BuildEvent::StepQueued { step_id, key, .. } => {
            record_step_key(*step_id, key.clone());
            Vec::new() // queue itself doesn't produce visible output
        }
        BuildEvent::StepStart {
            step_id,
            runner,
            image,
        } => {
            let key = step_key_for(*step_id);
            let line = match image {
                Some(img) => format!("[{key}] start (runner={runner} image={img})\n"),
                None => format!("[{key}] start (runner={runner})\n"),
            };
            line.into_bytes()
        }
        BuildEvent::StepLog { step_id, line, .. } => {
            let key = step_key_for(*step_id);
            format!("[{key}] {line}\n").into_bytes()
        }
        BuildEvent::StepCacheHit { step_id, tag, .. } => {
            let key = step_key_for(*step_id);
            format!("[{key}] cache hit ({tag})\n").into_bytes()
        }
        BuildEvent::StepEnd {
            step_id,
            exit_code,
            duration_ms,
            ..
        } => {
            let key = step_key_for(*step_id);
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use hm_plugin_protocol::{PlanSummary, StdStream};

    #[test]
    fn build_start_renders_step_and_chain_counts() {
        let ev = BuildEvent::BuildStart {
            run_id: Uuid::nil(),
            plan: PlanSummary {
                step_count: 3,
                chain_count: 2,
                default_runner: "docker".into(),
            },
            started_at: chrono::Utc::now(),
        };
        let s = String::from_utf8(render(&ev)).unwrap();
        assert!(s.contains("3 steps"));
        assert!(s.contains("2 chain"));
    }

    #[test]
    fn step_log_renders_with_prefix_after_step_queued_recorded_key() {
        let step_id = Uuid::new_v4();
        render(&BuildEvent::StepQueued {
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
        let s = String::from_utf8(render(&ev)).unwrap();
        assert_eq!(s, "[build] hello\n");
    }

    #[test]
    fn step_log_with_unknown_key_renders_question_mark() {
        let s = String::from_utf8(render(&BuildEvent::StepLog {
            step_id: Uuid::new_v4(),
            stream: StdStream::Stdout,
            line: "x".into(),
            ts: chrono::Utc::now(),
        }))
        .unwrap();
        assert!(s.starts_with("[?] "));
    }
}
