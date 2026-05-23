//! Built-in cloud client plugin for the hm CLI.
//!
//! Implements `hm cloud {login,logout,whoami,org,pipeline,build,job,billing,run}`.
//! HTTP traffic goes through reqwest directly (native dylib, no WASM sandbox).

#![allow(unsafe_code)]
#![allow(
    clippy::pedantic,
    clippy::nursery,
    clippy::cargo,
    clippy::multiple_crate_versions,
    clippy::cargo_common_metadata,
    clippy::missing_errors_doc,
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

use core::future::Future;
use hm_plugin_sdk::*;

#[derive(Default)]
struct Cloud;

impl SubcommandPlugin for Cloud {
    fn run<'a>(
        &'a self,
        ctx: &'a PluginContext<'a>,
        input: SubcommandInput,
    ) -> impl Future<Output = Result<ExitInfo, PluginError>> + Send + 'a {
        async move {
            let argv = input.verb_path.clone();
            cli::dispatch(ctx, argv, input.env).await
        }
    }
}

hm_plugin!(
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
        required_host_fns: vec![],
        config_schema: None,
        allowed_hosts: vec![],
    },
    subcommand = Cloud,
);
