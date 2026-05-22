//! `hm cloud build list|show|cancel|watch`.

use std::collections::BTreeMap;

use hm_plugin_protocol::PluginError;
use hm_plugin_sdk::host;

use crate::api::types::{Build, BuildList};
use crate::cli::BuildCommand;
use crate::config::Config;
use crate::creds;
use crate::http::Client;
use crate::state::CloudState;

pub(crate) fn run(env: &BTreeMap<String, String>, cmd: BuildCommand) -> Result<(), PluginError> {
    let cfg = Config::from_env(env);
    let token = creds::load_token(&cfg.api_base, env).ok_or_else(not_logged_in)?;
    let client = Client::new(&cfg, Some(token));
    let org = active_org()?;

    match cmd {
        BuildCommand::List { pipeline } => list(&client, &org, &pipeline),
        BuildCommand::Show { pipeline, number } => show(&client, &org, &pipeline, number),
        BuildCommand::Cancel { pipeline, number } => cancel(&client, &org, &pipeline, number),
        BuildCommand::Watch { pipeline, number } => watch(&client, &org, &pipeline, number),
    }
}

fn list(client: &Client, org: &str, pipe: &str) -> Result<(), PluginError> {
    let builds: BuildList = client.get(&format!("/organizations/{org}/pipelines/{pipe}/builds"))?;
    for b in &builds.data {
        let line = format!(
            "#{:<5} {:<10} {}\n",
            b.number,
            b.state,
            b.message.as_deref().unwrap_or("")
        );
        host::write_stdout(line.as_bytes());
    }
    Ok(())
}

fn show(client: &Client, org: &str, pipe: &str, number: i64) -> Result<(), PluginError> {
    let b: Build = client.get(&format!(
        "/organizations/{org}/pipelines/{pipe}/builds/{number}"
    ))?;
    let json = serde_json::to_string_pretty(&b).unwrap_or_default();
    host::write_stdout(json.as_bytes());
    host::write_stdout(b"\n");
    Ok(())
}

fn cancel(client: &Client, org: &str, pipe: &str, number: i64) -> Result<(), PluginError> {
    let _: serde_json::Value = client.post(
        &format!("/organizations/{org}/pipelines/{pipe}/builds/{number}/cancel"),
        &serde_json::json!({}),
    )?;
    host::write_stderr(format!("build #{number} cancelled\n").as_bytes());
    Ok(())
}

fn watch(client: &Client, org: &str, pipe: &str, number: i64) -> Result<(), PluginError> {
    use hm_plugin_protocol::{BuildEvent, PlanSummary, StdStream};
    use uuid::Uuid;

    let run_id = Uuid::new_v4();
    let step_id = Uuid::new_v4();

    host::build_event_emit(&BuildEvent::BuildStart {
        run_id,
        plan: PlanSummary {
            step_count: 1,
            chain_count: 1,
            default_runner: "cloud".into(),
        },
        started_at: chrono::Utc::now(),
    });
    host::build_event_emit(&BuildEvent::StepQueued {
        step_id,
        key: format!("cloud build #{number}"),
        chain_idx: 0,
    });
    host::build_event_emit(&BuildEvent::StepStart {
        step_id,
        runner: "cloud".into(),
        image: None,
    });

    let started = std::time::SystemTime::now();
    let mut last_state = String::new();

    loop {
        if host::should_cancel() {
            host::build_event_emit(&BuildEvent::ChainFailed {
                chain_idx: 0,
                failed_step_id: step_id,
                failed_step_key: format!("cloud build #{number}"),
                exit_code: 130,
                message: "watch cancelled by user".into(),
                ts: chrono::Utc::now(),
            });
            return Err(PluginError::new("cloud_cancelled", "watch cancelled by user"));
        }
        let b: Build = client.get(&format!(
            "/organizations/{org}/pipelines/{pipe}/builds/{number}"
        ))?;
        if b.state != last_state {
            host::build_event_emit(&BuildEvent::StepLog {
                step_id,
                stream: StdStream::Stderr,
                line: format!("state: {last_state} -> {}", b.state),
                ts: chrono::Utc::now(),
            });
            last_state = b.state.clone();
        }
        let terminal = match b.state.as_str() {
            "passed" => Some(0i32),
            "failed" | "canceled" => Some(1i32),
            _ => None,
        };
        if let Some(code) = terminal {
            let elapsed_ms = u64::try_from(
                started.elapsed().map(|d| d.as_millis()).unwrap_or(0)
            ).unwrap_or(u64::MAX);
            host::build_event_emit(&BuildEvent::StepEnd {
                step_id,
                exit_code: code,
                duration_ms: elapsed_ms,
                snapshot: None,
            });
            host::build_event_emit(&BuildEvent::BuildEnd {
                exit_code: code,
                duration_ms: elapsed_ms,
            });
            if code == 0 {
                return Ok(());
            }
            return Err(PluginError::new(
                "cloud_build_failed",
                format!("build {} ({})", b.state, number),
            ));
        }
        // Busy-wait ~2s, polling cancellation. Same shape as before;
        // hm_sleep_ms host fn arrives in a later plan.
        let spin_start = std::time::SystemTime::now();
        while spin_start.elapsed().map(|d| d.as_secs() < 2).unwrap_or(true) {
            if host::should_cancel() {
                break;
            }
        }
    }
}

fn not_logged_in() -> PluginError {
    PluginError::new("cloud_not_logged_in", "not logged in; run `hm cloud login`")
}

fn active_org() -> Result<String, PluginError> {
    CloudState::load().active_org.ok_or_else(|| {
        PluginError::new(
            "cloud_no_active_org",
            "no active organization; run `hm cloud org switch <slug>`",
        )
    })
}
