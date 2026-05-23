//! `hm cloud run [TASK]` — submit the local pipeline plan to the cloud
//! and watch the resulting build.
//!
//! For plan 4 the caller supplies a pre-rendered plan JSON via
//! `--plan-file` (or `.harmont/plan.json` by convention). Source-archive
//! upload — required by the live API — lands in plan 5.

use std::collections::BTreeMap;

use clap::Parser;
use hm_plugin_protocol::PluginError;
use hm_plugin_sdk::PluginContext;

use crate::api::types::{Build, CreateBuildRequest};
use crate::config::Config;
use crate::creds;
use crate::http::Client;
use crate::state::CloudState;

#[derive(Debug, Parser)]
pub(crate) struct RunArgs {
    /// Pipeline slug. Required.
    pub pipeline: String,
    /// Branch to record on the build.
    #[arg(short, long)]
    pub branch: Option<String>,
    /// Build message.
    #[arg(short, long)]
    pub message: Option<String>,
    /// Path to a pre-rendered pipeline JSON file.
    /// If unset, the plugin reads `.harmont/plan.json`.
    #[arg(long)]
    pub plan_file: Option<String>,
    /// Don't watch; print the build URL and exit.
    #[arg(long)]
    pub no_watch: bool,
}

pub(crate) async fn run(
    ctx: &PluginContext<'_>,
    env: &BTreeMap<String, String>,
    args: RunArgs,
) -> Result<(), PluginError> {
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

    // Read the pipeline plan. plan-4 has no in-plugin renderer; the
    // host's existing rendering pipeline (or the user) is responsible
    // for materialising the JSON.
    let plan_path = args.plan_file.as_deref().unwrap_or("plan.json");
    let bytes = ctx.fs_read_config(plan_path).ok_or_else(|| {
        PluginError::new(
            "cloud_plan_missing",
            format!("could not read plan file '{plan_path}'; render the plan first"),
        )
    })?;
    let plan_json: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|e| PluginError::new("cloud_plan_invalid_json", e.to_string()))?;

    let req = CreateBuildRequest {
        pipeline_slug: args.pipeline.clone(),
        branch: args.branch.clone(),
        message: args.message.clone(),
        env: env
            .iter()
            .filter(|(k, _)| k.starts_with("HM_RUN_ENV_"))
            .map(|(k, v)| (k.trim_start_matches("HM_RUN_ENV_").to_string(), v.clone()))
            .collect(),
        plan_json,
    };
    let build: Build = client
        .post(
            &format!("/organizations/{org}/pipelines/{}/builds", args.pipeline),
            &req,
        )
        .await?;
    let url = format!(
        "{}/{}/{}/builds/{}",
        cfg.api_base.trim_end_matches("/api"),
        org,
        args.pipeline,
        build.number
    );
    ctx.write_stderr(format!("submitted build #{}: {url}\n", build.number).as_bytes());
    if args.no_watch {
        return Ok(());
    }
    // Watch loop: same shape as verbs::build::watch.
    crate::verbs::build::run(
        ctx,
        env,
        crate::cli::BuildCommand::Watch {
            pipeline: args.pipeline.clone(),
            number: build.number,
        },
    )
    .await
}
