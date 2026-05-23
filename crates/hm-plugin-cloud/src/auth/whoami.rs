//! `hm cloud whoami` — print the user the stored token belongs to.

use std::collections::BTreeMap;

use hm_plugin_protocol::PluginError;
use hm_plugin_sdk::PluginContext;

use crate::api::types::User;
use crate::config::Config;
use crate::creds;
use crate::http::Client;

#[allow(
    dead_code,
    reason = "wired by `cli::dispatch` in the next cluster (Task 15)"
)]
pub(crate) async fn run(
    ctx: &PluginContext<'_>,
    env: &BTreeMap<String, String>,
) -> Result<(), PluginError> {
    let cfg = Config::from_env(env);
    let token = creds::load_token(&cfg.api_base, env).ok_or_else(|| {
        PluginError::new(
            "cloud_not_logged_in",
            format!("not logged in to {}\n  fix: `hm cloud login`", cfg.api_base),
        )
    })?;
    let client = Client::new(&cfg, Some(token));
    let me: User = client.get("/auth/me").await?;
    ctx.write_stdout(
        format!(
            "{} <{}> (id {})\n",
            me.display_name.clone().unwrap_or_else(|| me.email.clone()),
            me.email,
            me.id,
        )
        .as_bytes(),
    );
    Ok(())
}
