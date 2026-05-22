//! JSON-lines BuildEvent formatter — one event per line to stdout.
//! Moved from the standalone `hm-plugin-output-json` WASM crate.

use hm_plugin_protocol::BuildEvent;
use std::io::Write;

#[derive(Debug, Default)]
pub struct Json;

impl Json {
    pub fn on_event(&mut self, ev: &BuildEvent) {
        if let Some(bytes) = format_event(ev) {
            let _ = std::io::stdout().write_all(&bytes);
        }
    }

    pub fn finalize(&mut self) {}
}

/// Serialise `ev` to one JSON line (trailing `\n` included). Returns
/// `None` if `serde_json` fails to serialise — output formatters must
/// never panic the run, so the host swallows the loss silently.
fn format_event(ev: &BuildEvent) -> Option<Vec<u8>> {
    let mut bytes = serde_json::to_vec(ev).ok()?;
    bytes.push(b'\n');
    Some(bytes)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use hm_plugin_protocol::PlanSummary;
    use uuid::Uuid;

    #[test]
    fn build_start_serialises_to_json_line_with_kind_and_step_count() {
        let ev = BuildEvent::BuildStart {
            run_id: Uuid::nil(),
            plan: PlanSummary {
                step_count: 1,
                chain_count: 1,
                default_runner: "docker".into(),
            },
            started_at: chrono::Utc::now(),
        };
        let bytes = format_event(&ev).expect("serialise build_start");
        let s = std::str::from_utf8(&bytes).unwrap();
        assert!(
            s.contains(r#""kind":"build_start""#),
            "expected kind tag, got: {s}"
        );
        let parsed: serde_json::Value = serde_json::from_str(s.trim_end()).unwrap();
        assert_eq!(parsed["plan"]["step_count"], 1);
    }

    #[test]
    fn format_event_appends_trailing_newline() {
        let ev = BuildEvent::BuildEnd {
            exit_code: 0,
            duration_ms: 5,
        };
        let bytes = format_event(&ev).expect("serialise build_end");
        assert_eq!(bytes.last(), Some(&b'\n'));
    }
}
