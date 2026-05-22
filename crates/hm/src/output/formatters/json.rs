//! JSON-lines BuildEvent formatter — one event per line to stdout.
//! Moved from the standalone `hm-plugin-output-json` WASM crate.

use hm_plugin_protocol::BuildEvent;
use std::io::Write;

#[derive(Debug, Default)]
pub struct Json;

impl Json {
    pub fn on_event(&mut self, ev: &BuildEvent) {
        let Ok(mut bytes) = serde_json::to_vec(ev) else {
            return;
        };
        bytes.push(b'\n');
        let _ = std::io::stdout().write_all(&bytes);
    }

    pub fn finalize(&mut self) {}
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use hm_plugin_protocol::PlanSummary;
    use uuid::Uuid;

    #[test]
    fn build_start_serialises_to_json_line() {
        let ev = BuildEvent::BuildStart {
            run_id: Uuid::nil(),
            plan: PlanSummary {
                step_count: 1,
                chain_count: 1,
                default_runner: "docker".into(),
            },
            started_at: chrono::Utc::now(),
        };
        let bytes = serde_json::to_vec(&ev).unwrap();
        let s = std::str::from_utf8(&bytes).unwrap();
        assert!(s.contains(r#""BuildStart""#) || s.contains(r#""build_start""#));
        assert!(s.contains(r#""step_count":1"#));
    }
}
