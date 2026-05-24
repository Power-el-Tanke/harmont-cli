//! `hm cloud run` — submit the local pipeline plan to the cloud
//! and watch the resulting build.

use std::collections::BTreeMap;

use hm_plugin_protocol::PluginError;
use hm_plugin_sdk::PluginContext;

use crate::api::types::{Build, CreateBuildRequest};
use crate::config::Config;
use crate::creds;
use crate::http::Client;
use crate::state::CloudState;

pub(crate) async fn run(
    ctx: &PluginContext<'_>,
    env: &BTreeMap<String, String>,
    args: &serde_json::Value,
) -> Result<(), PluginError> {
    let pipeline = require_str(args, "pipeline")?;
    let branch = args.get("branch").and_then(serde_json::Value::as_str);
    let message = args.get("message").and_then(serde_json::Value::as_str);
    let plan_file = args.get("plan_file").and_then(serde_json::Value::as_str);
    let no_watch = args
        .get("no_watch")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);

    let cfg = Config::from_env(env);
    let token = creds::load_token(&cfg.api_base, env).ok_or_else(|| {
        PluginError::new("cloud_not_logged_in", "not logged in; run `hm cloud login`")
    })?;
    let client = Client::new(&cfg, Some(token));
    let org = CloudState::load(ctx).active_org.ok_or_else(|| {
        PluginError::new(
            "cloud_no_active_org",
            "no active organization; run `hm cloud org switch <slug>`",
        )
    })?;

    let plan_path = plan_file.unwrap_or("plan.json");
    let bytes = ctx.fs_read_config(plan_path).ok_or_else(|| {
        PluginError::new(
            "cloud_plan_missing",
            format!("could not read plan file '{plan_path}'; render the plan first"),
        )
    })?;
    let plan_json: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|e| PluginError::new("cloud_plan_invalid_json", e.to_string()))?;

    let req = CreateBuildRequest {
        pipeline_slug: pipeline.clone(),
        branch: branch.map(String::from),
        message: message.map(String::from),
        env: env
            .iter()
            .filter(|(k, _)| k.starts_with("HM_RUN_ENV_"))
            .map(|(k, v)| (k.trim_start_matches("HM_RUN_ENV_").to_string(), v.clone()))
            .collect(),
        plan_json,
    };
    let build: Build = client
        .post(
            &format!("/organizations/{org}/pipelines/{pipeline}/builds"),
            &req,
        )
        .await?;
    let url = format!(
        "{}/{}/{}/builds/{}",
        cfg.api_base.trim_end_matches("/api"),
        org,
        pipeline,
        build.number
    );
    ctx.write_stderr(format!("submitted build #{}: {url}\n", build.number).as_bytes());
    if no_watch {
        return Ok(());
    }
    super::build::watch_build(ctx, env, &pipeline, build.number).await
}

fn require_str(args: &serde_json::Value, key: &str) -> Result<String, PluginError> {
    args[key]
        .as_str()
        .map(String::from)
        .ok_or_else(|| PluginError::new("cloud_cli_parse", format!("missing required argument: {key}")))
}
