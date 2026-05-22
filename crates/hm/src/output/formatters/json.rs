//! Placeholder; populated in Task A2.

use hm_plugin_protocol::BuildEvent;

#[derive(Debug, Default)]
pub struct Json;

impl Json {
    pub fn on_event(&mut self, _ev: &BuildEvent) {}
    pub fn finalize(&mut self) {}
}
