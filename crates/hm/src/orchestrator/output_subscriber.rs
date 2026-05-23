//! Build-event subscriber that dispatches every `BuildEvent` into the
//! selected output-formatter plugin via typed `on_output_event` /
//! `finalize_output` methods on `LoadedPlugin`.
//!
//! The subscriber acquires an `Arc<LoadedPlugin>` from the registry per
//! event; the typed async call happens AFTER the registry lock is
//! dropped so concurrent step-executor invocations do not contend with
//! it.

// Pedantic-bucket nags accepted at module scope:
// - `needless_pass_by_value` on `bus`: the owned `Arc<EventBus>` makes
//   the bus->subscriber handoff explicit at the call site, mirrors the
//   plan-2 `stderr_sink::spawn_stderr_sink` shape.
// - `significant_drop_tightening`: the registry `MutexGuard` is held
//   only across the synchronous `get` lookup; the `else` arms return
//   from the spawn task and the happy path moves the `Arc` out and
//   drops the guard naturally at the end of the inner block. The lint
//   would have us sprinkle `drop(reg)` calls which add no clarity.
// - `print_stderr`: the Lagged arm intentionally bypasses the event
//   bus (which is the source of the lag) to surface a user-visible
//   drop signal, so an `eprintln!` direct to stderr is correct.
#![allow(
    clippy::needless_pass_by_value,
    clippy::significant_drop_tightening,
    clippy::print_stderr
)]

use std::sync::Arc;

use anyhow::Result;
use hm_plugin_protocol::BuildEvent;
use tokio::sync::Mutex;
use tokio::sync::broadcast::error::RecvError;

use super::events::EventBus;
use crate::plugin::PluginRegistry;

/// Spawn the subscriber task. Returns a join handle the orchestrator
/// awaits at shutdown so the `BuildEnd` event is fully drained.
///
/// `format_name` must already exist in `registry.output_formatter_index`
/// — `scheduler::run` validates this before emitting `BuildStart`, so
/// a missing entry here means we lost a race against a concurrent
/// registry mutation (impossible in single-run orchestration). We drop
/// events silently in that case and exit on `BuildEnd`.
#[must_use]
pub fn spawn(
    bus: Arc<EventBus>,
    registry: Arc<Mutex<PluginRegistry>>,
    format_name: String,
) -> tokio::task::JoinHandle<Result<()>> {
    let mut rx = bus.subscribe();
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    // Resolve the plugin under the registry lock, then
                    // drop the lock before awaiting the typed method
                    // so concurrent step-executor calls keep flowing.
                    let plugin = {
                        let reg = registry.lock().await;
                        let Some(&idx) = reg.output_formatter_index.get(&format_name) else {
                            // No plugin for this format; CLI parser
                            // should have caught this. Drain silently.
                            if matches!(event, BuildEvent::BuildEnd { .. }) {
                                return Ok(());
                            }
                            continue;
                        };
                        let Some(p) = reg.get(idx) else {
                            if matches!(event, BuildEvent::BuildEnd { .. }) {
                                return Ok(());
                            }
                            continue;
                        };
                        p
                    };
                    let is_end = matches!(event, BuildEvent::BuildEnd { .. });
                    // Log-and-continue on formatter failures: a broken
                    // output plugin shouldn't fail the build.
                    let _: Result<()> = plugin.on_output_event(&event).await;
                    if is_end {
                        // Finalise if the plugin exports it. Tolerate
                        // missing/erroring export — most streaming
                        // formatters don't implement it.
                        let _: Result<Vec<u8>> =
                            plugin.finalize_output().await;
                        return Ok(());
                    }
                }
                Err(RecvError::Closed) => return Ok(()),
                Err(RecvError::Lagged(n)) => {
                    tracing::warn!(
                        target: "orchestrator",
                        "output_subscriber: dropped {n} build events (subscriber fell behind)"
                    );
                    // Also surface to the user: send a synthetic stderr line via
                    // the host's write_stderr fn directly. This bypasses the
                    // event bus (which is the source of the lag), so it can't
                    // contribute to the lag we're reporting.
                    eprintln!("[output] dropped {n} build events (subscriber fell behind)");
                }
            }
        }
    })
}
