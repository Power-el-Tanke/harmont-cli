//! Built-in Docker step-executor plugin for the hm CLI.
//!
//! Uses bollard to drive the local Docker daemon directly. The plugin
//! streams exec output through the host's event bus via
//! `PluginContext::emit_step_log_stdout()` / `emit_step_log_stderr()`.

#![allow(unsafe_code)]
#![allow(
    clippy::pedantic,
    clippy::nursery,
    clippy::cargo,
    clippy::multiple_crate_versions,
    clippy::cargo_common_metadata,
    clippy::missing_errors_doc,
)]

use core::future::Future;
use hm_plugin_sdk::*;

mod decision;
mod docker;
mod image_name;

#[derive(Default)]
struct DockerExec;

impl StepExecutor for DockerExec {
    fn run<'a>(
        &'a self,
        ctx: &'a PluginContext<'a>,
        input: ExecutorInput,
    ) -> impl Future<Output = Result<StepResult, PluginError>> + Send + 'a {
        run_step(ctx, input)
    }
}

async fn run_step(
    ctx: &PluginContext<'_>,
    input: ExecutorInput,
) -> Result<StepResult, PluginError> {
    use crate::decision::plan;
    use crate::image_name::resolve_image;

    let plan = plan(&input.cache_lookup);

    // Cache hit shortcut: no container, no exec; hand back the hit tag
    // so downstream steps can boot from it.
    if !plan.run_command {
        return Ok(StepResult {
            exit_code: 0,
            committed_snapshot: plan.hit_tag.clone(),
            artifacts: vec![],
        });
    }

    let client = docker::DockerClient::connect()?;

    let image = resolve_image(
        &input.step,
        plan.hit_tag.as_ref(),
        input.parent_snapshot.as_ref(),
    );
    let container_name = sanitize_container_name(&input.run_id.to_string(), &input.step.key);

    // Ensure image is locally available.
    if !client.image_exists(&image).await? {
        ctx.log(Level::Info, &format!("pulling {image}"));
        client.pull_image(&image).await?;
    }

    // Convert BTreeMap env to Vec<String> for bollard ("KEY=VALUE" format).
    let env_vec: Vec<String> = input.env.iter().map(|(k, v)| format!("{k}={v}")).collect();

    let cid = client
        .start_long_lived(&image, &env_vec, &input.workdir, &container_name)
        .await?;

    // Run the step inside the container; always clean up afterward.
    let result = run_in_container(&client, ctx, &input, &cid, &env_vec, &plan).await;
    client.stop_remove(&cid).await;
    result
}

async fn run_in_container(
    client: &docker::DockerClient,
    ctx: &PluginContext<'_>,
    input: &ExecutorInput,
    cid: &str,
    env_vec: &[String],
    plan: &decision::DecisionPlan,
) -> Result<StepResult, PluginError> {
    // Extract workspace archive into container.
    client
        .extract_workspace(ctx, cid, &input.workspace_archive_id, &input.workdir)
        .await?;

    // Exec the step command.
    let cmd = vec!["sh".into(), "-c".into(), input.step.cmd.clone()];
    let exit_code = client
        .exec_streaming(cid, &cmd, env_vec, &input.workdir, ctx)
        .await?;

    // Commit on success.
    let committed = if exit_code == 0 {
        let target_tag = plan.commit_to.clone().unwrap_or_else(|| {
            let safe: String = input
                .step
                .key
                .chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                        c
                    } else {
                        '-'
                    }
                })
                .collect();
            hm_plugin_protocol::SnapshotRef(format!(
                "harmont-local-ephemeral/{safe}:run-{}",
                input.step_id.simple()
            ))
        });
        client.commit_container(cid, &target_tag.0).await?;
        Some(target_tag)
    } else {
        None
    };

    Ok(StepResult {
        exit_code,
        committed_snapshot: committed,
        artifacts: vec![],
    })
}

fn sanitize_container_name(run_id: &str, step_key: &str) -> String {
    let run_short: String = run_id.chars().take(8).collect();
    let key: String = step_key
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    format!("harmont-{run_short}-{key}")
}

hm_plugin!(
    manifest = PluginManifest {
        api_version: HM_PLUGIN_API_VERSION,
        name: "harmont-docker".into(),
        version: semver::Version::new(0, 1, 0),
        description: "Docker step executor (default runner).".into(),
        capabilities: vec![Capability::StepExecutor(StepExecutorSpec {
            runner: "docker".into(),
            default: true,
            step_schema: None,
        })],
        config_schema: None,
    },
    executor = DockerExec,
);
