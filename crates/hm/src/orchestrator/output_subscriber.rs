//! Build-event subscriber that dispatches every `BuildEvent` into the
//! selected output formatter. Built-in formatters (`human`, `json`) are
//! resolved first and bypass the WASM plugin registry entirely; the
//! registry lookup is only reached for externally-registered formatters.
//!
//! Replaces the plan-2 stop-gap `stderr_sink`. The subscriber acquires
//! an `Arc<LoadedPlugin>` from the registry per event; the actual
//! `call_capability` await happens AFTER the registry lock is dropped
//! so concurrent step-executor invocations do not contend with it.
//! Output plugins live in their own pool slot (default size 1) — only
//! this one subscriber task drains the bus, so a pool of 1 suffices.

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
/// `format_name` is resolved first against the built-in formatter set
/// (`human`, `json`). If no built-in matches, the name must exist in
/// `registry.output_formatter_index` — `scheduler::run` validates this
/// before emitting `BuildStart`. A missing registry entry here means a
/// race against a concurrent registry mutation (impossible in
/// single-run orchestration); events are drained silently until
/// `BuildEnd`.
#[must_use]
pub fn spawn(
    bus: Arc<EventBus>,
    registry: Arc<Mutex<PluginRegistry>>,
    format_name: String,
) -> tokio::task::JoinHandle<Result<()>> {
    let mut rx = bus.subscribe();
    let mut builtin = crate::output::formatters::builtin(&format_name);
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    let is_end = matches!(event, BuildEvent::BuildEnd { .. });
                    if let Some(b) = builtin.as_mut() {
                        b.on_event(&event);
                        if is_end {
                            b.finalize();
                            return Ok(());
                        }
                        continue;
                    }
                    // Fall through: format_name is not a built-in;
                    // resolve from the plugin registry.
                    let plugin = {
                        let reg = registry.lock().await;
                        let Some(&idx) = reg.output_formatter_index.get(&format_name) else {
                            if is_end { return Ok(()); }
                            continue;
                        };
                        let Some(p) = reg.get(idx) else {
                            if is_end { return Ok(()); }
                            continue;
                        };
                        p
                    };
                    let _: Result<()> =
                        plugin.call_capability("hm_output_on_event", &event).await;
                    if is_end {
                        let _: Result<Vec<u8>> =
                            plugin.call_capability("hm_output_finalize", &()).await;
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
