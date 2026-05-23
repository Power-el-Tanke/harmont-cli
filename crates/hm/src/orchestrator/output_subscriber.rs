//! Build-event subscriber that renders every `BuildEvent` directly via
//! `BuildEventRenderer` — no plugin dispatch, no FFI.
//!
//! Human output goes to stderr; JSON output goes to stdout. Both are
//! written with locked handles so concurrent flushes from other threads
//! do not interleave partial lines.

// Pedantic-bucket nags accepted at module scope:
// - `needless_pass_by_value` on `bus`: the owned `Arc<EventBus>` makes
//   the bus->subscriber handoff explicit at the call site, mirrors the
//   plan-2 `stderr_sink::spawn_stderr_sink` shape.
// - `print_stderr`: the Lagged arm intentionally bypasses the event
//   bus (which is the source of the lag) to surface a user-visible
//   drop signal, so an `eprintln!` direct to stderr is correct.
#![allow(clippy::needless_pass_by_value, clippy::print_stderr)]

use std::io::Write;
use std::sync::Arc;

use anyhow::Result;
use hm_plugin_protocol::BuildEvent;
use tokio::sync::broadcast::error::RecvError;

use super::events::EventBus;
use crate::output::OutputMode;
use crate::output::build_events::BuildEventRenderer;

/// Spawn the subscriber task. Returns a join handle the orchestrator
/// awaits at shutdown so the `BuildEnd` event is fully drained.
///
/// `format` controls where output is written:
/// - `OutputMode::Human { .. }` → stderr
/// - `OutputMode::Json` → stdout
#[must_use]
pub fn spawn(
    bus: Arc<EventBus>,
    format: OutputMode,
) -> tokio::task::JoinHandle<Result<()>> {
    let mut rx = bus.subscribe();
    tokio::spawn(async move {
        let mut renderer = BuildEventRenderer::new();
        loop {
            match rx.recv().await {
                Ok(event) => {
                    let is_end = matches!(event, BuildEvent::BuildEnd { .. });
                    let bytes = match &format {
                        OutputMode::Human { .. } => renderer.render_human(&event),
                        OutputMode::Json => renderer.render_json(&event),
                    };
                    if !bytes.is_empty() {
                        match &format {
                            OutputMode::Human { .. } => {
                                let stderr = std::io::stderr();
                                let mut handle = stderr.lock();
                                let _ = handle.write_all(&bytes);
                                let _ = handle.flush();
                            }
                            OutputMode::Json => {
                                let stdout = std::io::stdout();
                                let mut handle = stdout.lock();
                                let _ = handle.write_all(&bytes);
                                let _ = handle.flush();
                            }
                        }
                    }
                    if is_end {
                        return Ok(());
                    }
                }
                Err(RecvError::Closed) => return Ok(()),
                Err(RecvError::Lagged(n)) => {
                    tracing::warn!(
                        target: "orchestrator",
                        "output_subscriber: dropped {n} build events (subscriber fell behind)"
                    );
                    eprintln!("[output] dropped {n} build events (subscriber fell behind)");
                }
            }
        }
    })
}
