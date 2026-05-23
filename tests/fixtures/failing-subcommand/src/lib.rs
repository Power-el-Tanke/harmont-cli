//! A subcommand plugin that always exits non-zero. Lets the host
//! exercise `ExitInfo` plumbing.

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

#[derive(Default)]
struct Failing;

impl SubcommandPlugin for Failing {
    fn run<'a>(
        &'a self,
        _ctx: &'a PluginContext<'a>,
        _input: SubcommandInput,
    ) -> impl Future<Output = Result<ExitInfo, PluginError>> + Send + 'a {
        async move {
            Ok(ExitInfo {
                exit_code: 7,
                message: Some("intentional failure for tests".into()),
            })
        }
    }
}

hm_plugin!(
    manifest = PluginManifest {
        api_version: HM_PLUGIN_API_VERSION,
        name: "harmont-fixture-failing".into(),
        version: semver::Version::new(0, 1, 0),
        description: "Test fixture: always exits 7.".into(),
        capabilities: vec![Capability::Subcommand(SubcommandSpec {
            verb: "fixture-fail".into(),
            about: "Intentionally fails (test fixture)".into(),
            args: vec![],
            subcommands: vec![],
        })],
        config_schema: None,
    },
    subcommand = Failing,
);
