//! `hm cloud build list|show|cancel|watch`.

use std::collections::BTreeMap;

use hm_plugin_protocol::PluginError;
use hm_plugin_sdk::PluginContext;

use crate::api::types::{Build, BuildList};
use crate::config::Config;
use crate::creds;
use crate::http::Client;
use crate::state::CloudState;

pub(crate) async fn run(
    ctx: &PluginContext<'_>,
    env: &BTreeMap<String, String>,
    verb: &str,
    args: &serde_json::Value,
) -> Result<(), PluginError> {
    let cfg = Config::from_env(env);
    let token = creds::load_token(&cfg.api_base, env).ok_or_else(not_logged_in)?;
    let client = Client::new(&cfg, Some(token));
    let org = active_org(ctx)?;

    match verb {
        "list" => {
            let pipeline = require_str(args, "pipeline")?;
            list(ctx, &client, &org, &pipeline).await
        }
        "show" => {
            let pipeline = require_str(args, "pipeline")?;
            let number = require_i64(args, "number")?;
            show(ctx, &client, &org, &pipeline, number).await
        }
        "cancel" => {
            let pipeline = require_str(args, "pipeline")?;
            let number = require_i64(args, "number")?;
            cancel(ctx, &client, &org, &pipeline, number).await
        }
        "watch" => {
            let pipeline = require_str(args, "pipeline")?;
            let number = require_i64(args, "number")?;
            watch(ctx, &client, &org, &pipeline, number).await
        }
        _ => Err(PluginError::new(
            "cloud_unknown_verb",
            format!("unknown build verb: {verb}"),
        )),
    }
}

pub(crate) async fn watch_build(
    ctx: &PluginContext<'_>,
    env: &BTreeMap<String, String>,
    pipeline: &str,
    number: i64,
) -> Result<(), PluginError> {
    let cfg = Config::from_env(env);
    let token = creds::load_token(&cfg.api_base, env).ok_or_else(not_logged_in)?;
    let client = Client::new(&cfg, Some(token));
    let org = active_org(ctx)?;
    watch(ctx, &client, &org, pipeline, number).await
}

async fn list(
    ctx: &PluginContext<'_>,
    client: &Client,
    org: &str,
    pipe: &str,
) -> Result<(), PluginError> {
    let builds: BuildList = client
        .get(&format!("/organizations/{org}/pipelines/{pipe}/builds"))
        .await?;
    for b in &builds.data {
        let line = format!(
            "#{:<5} {:<10} {}\n",
            b.number,
            b.state,
            b.message.as_deref().unwrap_or("")
        );
        ctx.write_stdout(line.as_bytes());
    }
    Ok(())
}

async fn show(
    ctx: &PluginContext<'_>,
    client: &Client,
    org: &str,
    pipe: &str,
    number: i64,
) -> Result<(), PluginError> {
    let b: Build = client
        .get(&format!(
            "/organizations/{org}/pipelines/{pipe}/builds/{number}"
        ))
        .await?;
    let json = serde_json::to_string_pretty(&b).unwrap_or_default();
    ctx.write_stdout(json.as_bytes());
    ctx.write_stdout(b"\n");
    Ok(())
}

async fn cancel(
    ctx: &PluginContext<'_>,
    client: &Client,
    org: &str,
    pipe: &str,
    number: i64,
) -> Result<(), PluginError> {
    let _: serde_json::Value = client
        .post(
            &format!("/organizations/{org}/pipelines/{pipe}/builds/{number}/cancel"),
            &serde_json::json!({}),
        )
        .await?;
    ctx.write_stderr(format!("build #{number} cancelled\n").as_bytes());
    Ok(())
}

async fn watch(
    ctx: &PluginContext<'_>,
    client: &Client,
    org: &str,
    pipe: &str,
    number: i64,
) -> Result<(), PluginError> {
    let mut last_state = String::new();
    loop {
        if ctx.should_cancel() {
            return Err(PluginError::new(
                "cloud_cancelled",
                "watch cancelled by user",
            ));
        }
        let b: Build = client
            .get(&format!(
                "/organizations/{org}/pipelines/{pipe}/builds/{number}"
            ))
            .await?;
        if b.state != last_state {
            ctx.write_stderr(format!("state: {last_state} -> {}\n", b.state).as_bytes());
            last_state = b.state.clone();
        }
        match b.state.as_str() {
            "passed" => return Ok(()),
            "failed" | "canceled" => {
                return Err(PluginError::new(
                    "cloud_build_failed",
                    format!("build {} ({})", b.state, number),
                ));
            }
            _ => {}
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
}

fn require_str(args: &serde_json::Value, key: &str) -> Result<String, PluginError> {
    args[key]
        .as_str()
        .map(String::from)
        .ok_or_else(|| PluginError::new("cloud_cli_parse", format!("missing required argument: {key}")))
}

fn require_i64(args: &serde_json::Value, key: &str) -> Result<i64, PluginError> {
    args[key]
        .as_i64()
        .ok_or_else(|| PluginError::new("cloud_cli_parse", format!("missing required argument: {key}")))
}

fn not_logged_in() -> PluginError {
    PluginError::new("cloud_not_logged_in", "not logged in; run `hm cloud login`")
}

fn active_org(ctx: &PluginContext<'_>) -> Result<String, PluginError> {
    CloudState::load(ctx).active_org.ok_or_else(|| {
        PluginError::new(
            "cloud_no_active_org",
            "no active organization; run `hm cloud org switch <slug>`",
        )
    })
}
