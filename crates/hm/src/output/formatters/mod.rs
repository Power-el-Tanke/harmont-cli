//! Built-in `BuildEvent` formatters.
//!
//! External plugins can still register their own formatter via the
//! `OutputFormatter` capability; these are the in-tree implementations
//! that ship with every build of `hm`.

use hm_plugin_protocol::BuildEvent;

pub mod human;
pub mod json;

/// A formatter that lives inside the `hm` binary.
///
/// Returned by [`builtin`] for names the orchestrator already knows.
/// The orchestrator's output subscriber falls through to the WASM
/// plugin registry only when this returns `None`.
#[derive(Debug)]
pub enum Builtin {
    Human(human::Human),
    Json(json::Json),
}

impl Builtin {
    pub fn on_event(&mut self, event: &BuildEvent) {
        match self {
            Self::Human(h) => h.on_event(event),
            Self::Json(j) => j.on_event(event),
        }
    }

    pub const fn finalize(&mut self) {
        match self {
            Self::Human(h) => h.finalize(),
            Self::Json(j) => j.finalize(),
        }
    }
}

#[must_use]
pub fn builtin(name: &str) -> Option<Builtin> {
    match name {
        "human" => Some(Builtin::Human(human::Human::default())),
        "json" => Some(Builtin::Json(json::Json)),
        _ => None,
    }
}
