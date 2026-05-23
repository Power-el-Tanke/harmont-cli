//! Calls every host fn the stabby API defines and reports back what
//! happened. Used by `tests/plugin_host_fns.rs` to assert each host
//! fn is wired up and produces the expected behaviour.

#![allow(
    unsafe_code,
    clippy::pedantic,
    clippy::nursery,
    clippy::cargo,
    clippy::multiple_crate_versions,
    clippy::cargo_common_metadata,
    clippy::missing_errors_doc
)]

use core::future::Future;
use hm_plugin_sdk::*;
use serde::Serialize;

#[derive(Default, Serialize)]
struct Report {
    log_ok: bool,
    kv_round_trip: bool,
    kv_isolated_per_scope: bool,
    fs_read_returns_none_for_missing: bool,
    should_cancel_default_false: bool,
}

#[derive(Default)]
struct Probe;

impl SubcommandPlugin for Probe {
    fn run<'a>(
        &'a self,
        ctx: &'a PluginContext<'a>,
        _input: SubcommandInput,
    ) -> impl Future<Output = Result<ExitInfo, PluginError>> + Send + 'a {
        async move {
            let mut r = Report::default();

            ctx.log(Level::Info, "probe: log");
            r.log_ok = true;

            ctx.kv_set(KvScope::Plugin, "k", b"v1");
            let v = ctx.kv_get(KvScope::Plugin, "k").unwrap_or_default();
            r.kv_round_trip = v == b"v1";

            ctx.kv_set(KvScope::Build, "k", b"v2");
            let p = ctx.kv_get(KvScope::Plugin, "k").unwrap_or_default();
            let b = ctx.kv_get(KvScope::Build, "k").unwrap_or_default();
            r.kv_isolated_per_scope = p == b"v1" && b == b"v2";

            r.fs_read_returns_none_for_missing =
                ctx.fs_read_config("does/not/exist").is_none();

            r.should_cancel_default_false = !ctx.should_cancel();

            let json = serde_json::to_string(&r)
                .map_err(|e| PluginError::new("serde", e.to_string()))?;
            Ok(ExitInfo {
                exit_code: 0,
                message: Some(json),
            })
        }
    }
}

hm_plugin!(
    manifest = PluginManifest {
        api_version: HM_PLUGIN_API_VERSION,
        name: "harmont-fixture-probe".into(),
        version: semver::Version::new(0, 1, 0),
        description: "Test fixture: exercises every host fn.".into(),
        capabilities: vec![Capability::Subcommand(SubcommandSpec {
            verb: "fixture-probe".into(),
            about: "Probe host-fn surface".into(),
            args_schema: serde_json::json!({"args": []}),
            subcommands: vec![],
        })],
        required_host_fns: vec![],
        config_schema: None,
        allowed_hosts: vec![],
    },
    subcommand = Probe,
);
