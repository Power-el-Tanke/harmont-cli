//! Cloud watch (host-fn fed) → TuiEvent adapter.
//!
//! The cloud plugin runs `watch` inside WASM and emits wire
//! `BuildEvent`s via the `hm_build_event_emit` host fn. The host fn
//! pushes them into the mpsc owned by `OrchestratorState::tui_event_tx`.
//! This source spawns the same translator task as `local::spawn` —
//! the wire format is identical.

pub use super::local::spawn;
