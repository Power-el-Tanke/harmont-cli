//! `hm cloud logout` — clears the stored bearer token.

use std::collections::BTreeMap;

use hm_plugin_protocol::PluginError;
use hm_plugin_sdk::PluginContext;

use crate::config::Config;
use crate::creds;

#[allow(
    dead_code,
    reason = "wired by `cli::dispatch` in the next cluster (Task 15)"
)]
pub(crate) async fn run(
    ctx: &PluginContext<'_>,
    env: &BTreeMap<String, String>,
) -> Result<(), PluginError> {
    let cfg = Config::from_env(env);
    creds::clear_token(&cfg.api_base);
    ctx.write_stderr(format!("logged out of {}\n", cfg.api_base).as_bytes());
    Ok(())
}
