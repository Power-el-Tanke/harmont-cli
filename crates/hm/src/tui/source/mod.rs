//! Event-source adapters for the Mission Control TUI.
//!
//! Each command surface (`hm run`, `hm dev up`, `hm cloud build watch`)
//! constructs a source that converts its command-specific event stream
//! into `TuiEvent`s sent on the mpsc channel `tui::run` consumes.

pub mod local;
pub mod dev;
pub mod cloud;

use tokio::sync::mpsc;

use super::event::TuiEvent;

/// Channel capacity from adapter → TUI. Adapters drop `StepLog`
/// events when full and emit a single `Lagged` synthetic event per
/// drop burst, matching the protocol-bus contract.
pub const TUI_CHANNEL_CAPACITY: usize = 1024;

/// Create the (sender, receiver) pair used by adapters and the TUI.
#[must_use]
pub fn channel() -> (mpsc::Sender<TuiEvent>, mpsc::Receiver<TuiEvent>) {
    mpsc::channel(TUI_CHANNEL_CAPACITY)
}
