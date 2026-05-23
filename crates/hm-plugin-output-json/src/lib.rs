//! Built-in JSON-lines output formatter.
//!
//! Each `BuildEvent` is serialised to JSON on a single line and
//! written to stdout. Stderr is reserved for plugin/host diagnostics.

#![allow(unsafe_code)]
#![allow(
    clippy::pedantic,
    clippy::nursery,
    clippy::cargo,
    clippy::multiple_crate_versions,
    clippy::cargo_common_metadata,
    clippy::missing_errors_doc,
)]

use core::future::Future;
use hm_plugin_sdk::*;

#[derive(Default)]
struct Json;

impl OutputFormatter for Json {
    fn on_event(
        &self,
        ctx: &PluginContext<'_>,
        event: BuildEvent,
    ) -> impl Future<Output = Result<(), PluginError>> + Send + '_ {
        let result = (|| {
            let mut bytes = serde_json::to_vec(&event)
                .map_err(|e| PluginError::new("output_json_serde", e.to_string()))?;
            bytes.push(b'\n');
            ctx.write_stdout(&bytes);
            Ok(())
        })();
        async move { result }
    }
}

hm_plugin!(
    manifest = PluginManifest {
        api_version: HM_PLUGIN_API_VERSION,
        name: "harmont-output-json".into(),
        version: semver::Version::new(0, 1, 0),
        description: "JSON-lines build output formatter.".into(),
        capabilities: vec![Capability::OutputFormatter(OutputFormatterSpec {
            name: "json".into(),
            mime: "application/x-ndjson".into(),
        })],
        required_host_fns: vec![],
        config_schema: None,
        allowed_hosts: vec![],
    },
    output = Json,
);
