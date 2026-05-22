//! Build-event broadcast → `TuiEvent` adapter for local `hm run`.
//!
//! The orchestrator emits wire [`BuildEvent`]s on its broadcast bus and
//! forwards them on a [`tokio::sync::mpsc`] sender when one is provided.
//! This adapter sits between that `mpsc` and the TUI's `TuiEvent` channel,
//! translating each [`BuildEvent`] 1:1 (the `Lagged` variant is handled
//! separately by the scheduler bridge).

use hm_plugin_protocol::BuildEvent;
use tokio::sync::mpsc;

use crate::tui::event::TuiEvent;

/// Spawn the translator task. Returns the bus-side sender for
/// `scheduler::run` and the consumer receiver for `tui::run`.
#[must_use]
pub fn spawn() -> (
    mpsc::Sender<BuildEvent>,
    mpsc::Receiver<TuiEvent>,
) {
    let (bus_tx, mut bus_rx) = mpsc::channel::<BuildEvent>(super::TUI_CHANNEL_CAPACITY);
    let (tui_tx, tui_rx) = super::channel();

    tokio::spawn(async move {
        while let Some(ev) = bus_rx.recv().await {
            let translated = translate(ev);
            if tui_tx.send(translated).await.is_err() {
                break;
            }
        }
    });

    (bus_tx, tui_rx)
}

pub(crate) fn translate(ev: BuildEvent) -> TuiEvent {
    match ev {
        BuildEvent::BuildStart { run_id, plan, started_at } => TuiEvent::BuildStart {
            run_id,
            plan,
            started_at,
        },
        BuildEvent::StepQueued { step_id: _, key, chain_idx } => TuiEvent::ChainQueued {
            chain_idx,
            label: key,
            parent: None,
        },
        BuildEvent::StepStart { step_id, runner, image } => TuiEvent::StepStart {
            step_id,
            // chain_idx and label are filled in by the reducer from the
            // preceding StepQueued event; this translator does not know
            // the chain index from a StepStart alone.
            chain_idx: 0,
            runner,
            image,
            label: String::new(),
        },
        BuildEvent::StepLog { step_id, stream, line, ts } => TuiEvent::StepLog {
            step_id,
            stream,
            line,
            ts,
        },
        BuildEvent::StepCacheHit { step_id, key, tag } => TuiEvent::StepCacheHit {
            step_id,
            key,
            tag,
        },
        BuildEvent::StepEnd { step_id, exit_code, duration_ms, snapshot: _ } => TuiEvent::StepEnd {
            step_id,
            exit_code,
            duration_ms,
        },
        BuildEvent::ChainFailed {
            chain_idx,
            failed_step_id: _,
            failed_step_key,
            exit_code,
            message,
            ts: _,
        } => TuiEvent::ChainFailed {
            chain_idx,
            failed_step_key,
            exit_code,
            message,
        },
        BuildEvent::BuildEnd { exit_code, duration_ms } => TuiEvent::BuildEnd {
            exit_code,
            duration_ms,
        },
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic, reason = "test-only: panic on unexpected event variant is intentional")]
mod tests {
    use super::*;
    use hm_plugin_protocol::PlanSummary;
    use uuid::Uuid;

    #[tokio::test]
    async fn forwards_build_start() {
        let (bus_tx, mut tui_rx) = spawn();
        bus_tx.send(BuildEvent::BuildStart {
            run_id: Uuid::nil(),
            plan: PlanSummary {
                step_count: 1,
                chain_count: 1,
                default_runner: "docker".into(),
            },
            started_at: chrono::Utc::now(),
        }).await.unwrap();
        let ev = tui_rx.recv().await.unwrap();
        match ev {
            TuiEvent::BuildStart { .. } => {}
            other => panic!("got {other:?}"),
        }
    }
}
