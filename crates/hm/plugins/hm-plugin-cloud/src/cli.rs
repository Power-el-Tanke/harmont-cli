//! Plugin-internal dispatch. The host has already parsed the CLI via
//! clap_bridge; the plugin receives structured `SubcommandInput` with
//! verb_path and JSON args.

use hm_plugin_protocol::{ExitInfo, PluginError, SubcommandInput};
use hm_plugin_sdk::PluginContext;

use crate::{auth, verbs};

pub(crate) async fn dispatch(
    ctx: &PluginContext<'_>,
    input: SubcommandInput,
) -> Result<ExitInfo, PluginError> {
    // Convert once: downstream verb functions still accept &serde_json::Value.
    let args_json: serde_json::Value = input.args.into();
    let tail: Vec<&str> = input.verb_path.iter().skip(1).map(String::as_str).collect();
    let result = match tail.as_slice() {
        ["login"] => {
            let paste = args_json.get("paste").and_then(serde_json::Value::as_bool).unwrap_or(false);
            auth::login::run(ctx, &input.env, paste).await
        }
        ["logout"] => auth::logout::run(ctx, &input.env).await,
        ["whoami"] => auth::whoami::run(ctx, &input.env).await,
        ["org", verb] => verbs::org::run(ctx, &input.env, verb, &args_json).await,
        ["pipeline", verb] => verbs::pipeline::run(ctx, &input.env, verb, &args_json).await,
        ["build", verb] => verbs::build::run(ctx, &input.env, verb, &args_json).await,
        ["job", verb] => verbs::job::run(ctx, &input.env, verb, &args_json).await,
        ["billing", verb] => verbs::billing::run(ctx, &input.env, verb, &args_json).await,
        ["run"] => verbs::run::run(ctx, &input.env, &args_json).await,
        other => {
            return Ok(ExitInfo {
                exit_code: 2,
                message: Some(format!("unknown cloud verb: {}", other.join(" "))),
            });
        }
    };
    match result {
        Ok(()) => Ok(ExitInfo {
            exit_code: 0,
            message: None,
        }),
        Err(e) => Ok(ExitInfo {
            exit_code: exit_code_for(&e),
            message: Some(e.message),
        }),
    }
}

fn exit_code_for(e: &PluginError) -> i32 {
    match e.code.as_str() {
        "cloud_auth" | "cloud_not_logged_in" => 3,
        "cloud_http" | "cloud_http_request" => 4,
        "cloud_cli_parse" => 2,
        _ => 1,
    }
}
