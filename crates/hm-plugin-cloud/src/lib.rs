//! Built-in cloud client plugin for the hm CLI.
//!
//! Implements `hm cloud {login,logout,whoami,org,pipeline,build,job,billing,run}`.
//! All HTTP traffic goes through extism-pdk's host-mediated http_request
//! (enforced by the manifest's allowed_hosts list).

#![allow(unsafe_code, reason = "extism-pdk host_fn imports require unsafe")]
#![allow(
    clippy::pedantic,
    clippy::nursery,
    clippy::cargo,
    clippy::multiple_crate_versions,
    clippy::cargo_common_metadata,
    clippy::missing_errors_doc,
    reason = "matches the test-fixtures allow-list; plugin authoring crate"
)]

mod api;
mod auth;
mod cli;
mod config;
mod creds;
mod http;
mod output;
mod state;
mod verbs;

use hm_plugin_sdk::*;

#[derive(Default)]
struct Cloud;

impl SubcommandPlugin for Cloud {
    fn run(&self, input: SubcommandInput) -> Result<ExitInfo, PluginError> {
        // Parse argv inside the plugin. input.verb_path[0] is "cloud";
        // the rest is the nested verb + args.
        let argv = input.verb_path.clone();
        cli::dispatch(argv, input.env)
    }
}

register_plugin!(
    manifest = PluginManifest {
        api_version: HM_PLUGIN_API_VERSION,
        name: "harmont-cloud".into(),
        version: semver::Version::new(0, 1, 0),
        description: "Cloud client: login, whoami, org, pipeline, build, job, billing, run.".into(),
        capabilities: vec![Capability::Subcommand(SubcommandSpec {
            verb: "cloud".into(),
            about: "Talk to the Harmont cloud API".into(),
            args_schema: serde_json::json!({}),
            subcommands: vec![],
        })],
        required_host_fns: vec![
            "hm_log".into(),
            "hm_write_stdout".into(),
            "hm_write_stderr".into(),
            "hm_tty_prompt".into(),
            "hm_tty_confirm".into(),
            "hm_browser_open".into(),
            "hm_spawn_loopback".into(),
            "hm_loopback_recv".into(),
            "hm_keyring_get".into(),
            "hm_keyring_set".into(),
            "hm_keyring_delete".into(),
            "hm_kv_get".into(),
            "hm_kv_set".into(),
            "hm_should_cancel".into(),
            "hm_build_event_emit".into(),
        ],
        config_schema: None,
        allowed_hosts: vec![
            "api.harmont.dev".into(),
            "*.harmont.dev".into(),
            // Test-only: wiremock binds 127.0.0.1 on a random port.
            // extism's HTTP gate matches by host, not port, so adding
            // these patterns lets integration tests target a local
            // mock server via `HARMONT_API_URL=http://127.0.0.1:<port>`
            // without compromising the prod allowlist.
            "127.0.0.1".into(),
            "localhost".into(),
        ],
    },
    subcommand = Cloud,
);
