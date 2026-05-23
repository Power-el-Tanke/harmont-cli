//! `hm cloud build list|show|cancel|watch`.

use std::collections::BTreeMap;

use hm_plugin_protocol::PluginError;
use hm_plugin_sdk::PluginContext;

use crate::api::types::{Build, BuildList};
use crate::cli::BuildCommand;
use crate::config::Config;
use crate::creds;
use crate::http::Client;
use crate::state::CloudState;

pub(crate) async fn run(
    ctx: &PluginContext<'_>,
    env: &BTreeMap<String, String>,
    cmd: BuildCommand,
) -> Result<(), PluginError> {
    let cfg = Config::from_env(env);
    let token = creds::load_token(&cfg.api_base, env).ok_or_else(not_logged_in)?;
    let client = Client::new(&cfg, Some(token));
    let org = active_org(ctx)?;

    match cmd {
        BuildCommand::List { pipeline } => list(ctx, &client, &org, &pipeline).await,
        BuildCommand::Show { pipeline, number } => show(ctx, &client, &org, &pipeline, number).await,
        BuildCommand::Cancel { pipeline, number } => {
            cancel(ctx, &client, &org, &pipeline, number).await
        }
        BuildCommand::Watch { pipeline, number } => {
            watch(ctx, &client, &org, &pipeline, number).await
        }
    }
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
    // Poll the build's state every 2 seconds; print state transitions
    // to stderr. Exit when terminal (passed/failed/canceled).
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
