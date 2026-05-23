//! Built-in human-readable output formatter for the hm CLI.
//!
//! Subscribes to the orchestrator's BuildEvent stream via the
//! `on_output_event` capability; writes prefixed step logs and brief
//! status lines to stderr.

#![allow(unsafe_code)]
#![allow(
    clippy::pedantic,
    clippy::nursery,
    clippy::cargo,
    clippy::multiple_crate_versions,
    clippy::cargo_common_metadata,
    clippy::missing_errors_doc,
)]

mod render;

use core::future::Future;
use hm_plugin_sdk::*;

#[derive(Default)]
struct Human;

impl OutputFormatter for Human {
    fn on_event(
        &self,
        ctx: &PluginContext<'_>,
        event: BuildEvent,
    ) -> impl Future<Output = Result<(), PluginError>> + Send + '_ {
        let bytes = render::render(&event);
        if !bytes.is_empty() {
            ctx.write_stderr(&bytes);
        }
        async { Ok(()) }
    }
}

hm_plugin!(
    manifest = PluginManifest {
        api_version: HM_PLUGIN_API_VERSION,
        name: "harmont-output-human".into(),
        version: semver::Version::new(0, 1, 0),
        description: "Human-readable build output formatter.".into(),
        capabilities: vec![Capability::OutputFormatter(OutputFormatterSpec {
            name: "human".into(),
            mime: "text/plain".into(),
        })],
        required_host_fns: vec![],
        config_schema: None,
        allowed_hosts: vec![],
    },
    output = Human,
);
