//! Plugin-internal CLI parsing. The plugin receives the raw argv from
//! the host (verb_path = ["cloud", ...]) and parses it with clap.

use std::collections::BTreeMap;

use clap::{Parser, Subcommand};
use hm_plugin_protocol::{ExitInfo, PluginError};
use hm_plugin_sdk::PluginContext;

use crate::{auth, verbs};

#[derive(Debug, Parser)]
#[command(
    name = "hm cloud",
    about = "Talk to the Harmont cloud API",
    disable_help_subcommand = true
)]
struct CloudCli {
    #[command(subcommand)]
    command: CloudCommand,
}

#[derive(Debug, Subcommand)]
enum CloudCommand {
    /// Authenticate this CLI against the Harmont API.
    Login {
        /// Skip the loopback flow and prompt for a paste-in code.
        #[arg(long)]
        paste: bool,
    },
    /// Remove stored credentials.
    Logout,
    /// Show the authenticated user.
    Whoami,
    /// Manage organizations.
    #[command(subcommand)]
    Org(OrgCommand),
    /// Manage pipelines.
    #[command(subcommand)]
    Pipeline(PipelineCommand),
    /// Manage builds.
    #[command(subcommand)]
    Build(BuildCommand),
    /// Manage jobs.
    #[command(subcommand)]
    Job(JobCommand),
    /// Manage credits, top-ups, and usage.
    #[command(subcommand)]
    Billing(BillingCommand),
    /// Submit the local pipeline to the cloud and watch its build.
    Run(verbs::run::RunArgs),
}

#[derive(Debug, Subcommand)]
pub(crate) enum OrgCommand {
    /// Set the active organization.
    Switch {
        /// Organization slug.
        slug: String,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum PipelineCommand {
    /// List pipelines for the active organization.
    List,
    /// Show pipeline details by slug.
    Show { slug: String },
}

#[derive(Debug, Subcommand)]
pub(crate) enum BuildCommand {
    /// List builds for a pipeline.
    List {
        #[arg(short, long)]
        pipeline: String,
    },
    /// Show a build by number.
    Show {
        #[arg(short, long)]
        pipeline: String,
        number: i64,
    },
    /// Cancel a build.
    Cancel {
        #[arg(short, long)]
        pipeline: String,
        number: i64,
    },
    /// Watch a build until it reaches a terminal state.
    Watch {
        #[arg(short, long)]
        pipeline: String,
        number: i64,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum JobCommand {
    /// List jobs in a build.
    List {
        #[arg(short, long)]
        pipeline: String,
        #[arg(short, long)]
        build: i64,
    },
    /// Show a job by id.
    Show {
        #[arg(short, long)]
        pipeline: String,
        #[arg(short, long)]
        build: i64,
        job_id: String,
    },
    /// Print the job log.
    Log {
        #[arg(short, long)]
        pipeline: String,
        #[arg(short, long)]
        build: i64,
        job_id: String,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum BillingCommand {
    /// Print the current credit balance.
    Balance,
    /// List billing transactions.
    Transactions {
        #[arg(long, default_value = "100")]
        limit: u32,
    },
    /// Show usage over a time window.
    Usage {
        #[arg(long)]
        from: Option<String>,
        #[arg(long)]
        to: Option<String>,
    },
    /// Top up credits via Stripe checkout.
    Topup {
        amount_usd: u32,
        #[arg(long)]
        no_browser: bool,
    },
    /// Redeem a coupon code.
    Redeem { code: String },
}

pub(crate) async fn dispatch(
    ctx: &PluginContext<'_>,
    argv: Vec<String>,
    env: BTreeMap<String, String>,
) -> Result<ExitInfo, PluginError> {
    // clap expects argv[0] to be the binary name; the host passes
    // the verb path which starts with "cloud". Replace argv[0] with
    // "hm cloud" so clap discards it as the program name and parses
    // the remaining tokens (the cloud subcommand + args) correctly.
    let mut full: Vec<String> = vec!["hm cloud".to_string()];
    full.extend(argv.into_iter().skip(1));
    let parsed = match CloudCli::try_parse_from(&full) {
        Ok(p) => p,
        Err(e) => {
            // clap surfaces `--help` / `--version` as errors with
            // specific kinds; render them as a successful exit so the
            // user sees the help text without an error code.
            use clap::error::ErrorKind;
            let msg = e.to_string();
            return match e.kind() {
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => {
                    ctx.write_stdout(msg.as_bytes());
                    Ok(ExitInfo {
                        exit_code: 0,
                        message: None,
                    })
                }
                _ => Ok(ExitInfo {
                    exit_code: 2,
                    message: Some(msg),
                }),
            };
        }
    };
    let result = match parsed.command {
        CloudCommand::Login { paste } => auth::login::run(ctx, &env, paste).await,
        CloudCommand::Logout => auth::logout::run(ctx, &env).await,
        CloudCommand::Whoami => auth::whoami::run(ctx, &env).await,
        CloudCommand::Org(cmd) => verbs::org::run(ctx, &env, cmd).await,
        CloudCommand::Pipeline(cmd) => verbs::pipeline::run(ctx, &env, cmd).await,
        CloudCommand::Build(cmd) => verbs::build::run(ctx, &env, cmd).await,
        CloudCommand::Job(cmd) => verbs::job::run(ctx, &env, cmd).await,
        CloudCommand::Billing(cmd) => verbs::billing::run(ctx, &env, cmd).await,
        CloudCommand::Run(args) => verbs::run::run(ctx, &env, args).await,
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
