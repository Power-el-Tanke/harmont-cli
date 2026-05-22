//! Dev daemon → `TuiEvent` adapter.
//!
//! Driven by `hm dev up`'s existing `LogLine` mpsc and a list of known
//! deploys at boot time. v1 synthesises `Healthy` per deploy at start
//! and `Stopped` when logmux closes; richer Docker-level health
//! polling is a follow-up. The adapter never panics: send-errors mean
//! the TUI consumer dropped, so we exit the task cleanly.

use chrono::Utc;
use tokio::sync::mpsc;

use crate::commands::dev::logmux::LogLine;
use crate::tui::event::{DeployState, TuiEvent};

/// Spawn the translator. Returns the `TuiEvent` receiver. The caller
/// passes the `LogLine` receiver (already created in `hm dev up`) and
/// a list of `(slug, deploy_id)` pairs known at boot time.
#[must_use]
pub fn spawn(
    mut log_rx: mpsc::UnboundedReceiver<LogLine>,
    deploys: Vec<(String, String)>,
) -> mpsc::Receiver<TuiEvent> {
    let (tx, rx) = super::channel();

    let tx_init = tx.clone();
    let deploys_init = deploys.clone();
    tokio::spawn(async move {
        // Synthetic BuildStart so AppState header renders.
        let _ = tx_init.send(TuiEvent::BuildStart {
            run_id: uuid::Uuid::new_v4(),
            plan: hm_plugin_protocol::PlanSummary {
                step_count: deploys_init.len(),
                chain_count: deploys_init.len(),
                default_runner: "docker".into(),
            },
            started_at: Utc::now(),
        }).await;

        for (idx, (slug, _deploy_id)) in deploys_init.iter().enumerate() {
            let _ = tx_init.send(TuiEvent::ChainQueued {
                chain_idx: idx,
                label: slug.clone(),
                parent: None,
            }).await;
            let _ = tx_init.send(TuiEvent::DeployStatus {
                deploy_id: slug.clone(),
                label: slug.clone(),
                state: DeployState::Healthy,
                restarts: 0,
                uptime_ms: 0,
            }).await;
        }

        while let Some(line) = log_rx.recv().await {
            let payload = String::from_utf8_lossy(&line.bytes).into_owned();
            if tx_init.send(TuiEvent::DeployLog {
                deploy_id: line.slug,
                stream: hm_plugin_protocol::StdStream::Stdout,
                line: payload,
                ts: Utc::now(),
            }).await.is_err() {
                return; // TUI consumer dropped
            }
        }

        // logmux closed — mark every deploy stopped and synthesise
        // BuildEnd so the summary card renders.
        for (slug, _) in &deploys {
            let _ = tx_init.send(TuiEvent::DeployStatus {
                deploy_id: slug.clone(),
                label: slug.clone(),
                state: DeployState::Stopped,
                restarts: 0,
                uptime_ms: 0,
            }).await;
        }
        let _ = tx_init.send(TuiEvent::BuildEnd {
            exit_code: 0,
            duration_ms: 0,
        }).await;
    });

    rx
}
