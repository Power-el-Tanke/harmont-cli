//! Clap derive types used solely for generating the plugin manifest's
//! `SubcommandSpec` tree. The host parses args at runtime; these types
//! exist only so `spec_from_command` can introspect the CLI structure.

use clap::{CommandFactory, Parser, Subcommand};
use hm_plugin_protocol::SubcommandSpec;
use hm_plugin_sdk::spec_from_clap::spec_from_command;

pub(crate) fn cloud_spec() -> SubcommandSpec {
    spec_from_command(&CloudCli::command())
}

#[derive(Debug, Parser)]
#[command(
    name = "cloud",
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
    Run {
        /// Pipeline slug.
        pipeline: String,
        /// Branch to record on the build.
        #[arg(short, long)]
        branch: Option<String>,
        /// Build message.
        #[arg(short, long)]
        message: Option<String>,
        /// Path to a pre-rendered pipeline JSON file.
        #[arg(long)]
        plan_file: Option<String>,
        /// Don't watch; print the build URL and exit.
        #[arg(long)]
        no_watch: bool,
    },
}

#[derive(Debug, Subcommand)]
enum OrgCommand {
    /// Set the active organization.
    Switch {
        /// Organization slug.
        slug: String,
    },
}

#[derive(Debug, Subcommand)]
enum PipelineCommand {
    /// List pipelines for the active organization.
    List,
    /// Show pipeline details by slug.
    Show {
        /// Pipeline slug.
        slug: String,
    },
}

#[derive(Debug, Subcommand)]
enum BuildCommand {
    /// List builds for a pipeline.
    List {
        /// Pipeline slug.
        #[arg(short, long)]
        pipeline: String,
    },
    /// Show a build by number.
    Show {
        /// Pipeline slug.
        #[arg(short, long)]
        pipeline: String,
        /// Build number.
        number: i64,
    },
    /// Cancel a build.
    Cancel {
        /// Pipeline slug.
        #[arg(short, long)]
        pipeline: String,
        /// Build number.
        number: i64,
    },
    /// Watch a build until it reaches a terminal state.
    Watch {
        /// Pipeline slug.
        #[arg(short, long)]
        pipeline: String,
        /// Build number.
        number: i64,
    },
}

#[derive(Debug, Subcommand)]
enum JobCommand {
    /// List jobs in a build.
    List {
        /// Pipeline slug.
        #[arg(short, long)]
        pipeline: String,
        /// Build number.
        #[arg(short, long)]
        build: i64,
    },
    /// Show a job by id.
    Show {
        /// Pipeline slug.
        #[arg(short, long)]
        pipeline: String,
        /// Build number.
        #[arg(short, long)]
        build: i64,
        /// Job ID.
        job_id: String,
    },
    /// Print the job log.
    Log {
        /// Pipeline slug.
        #[arg(short, long)]
        pipeline: String,
        /// Build number.
        #[arg(short, long)]
        build: i64,
        /// Job ID.
        job_id: String,
    },
}

#[derive(Debug, Subcommand)]
enum BillingCommand {
    /// Print the current credit balance.
    Balance,
    /// List billing transactions.
    Transactions {
        /// Maximum number of transactions to show.
        #[arg(long, default_value = "100")]
        limit: u32,
    },
    /// Show usage over a time window.
    Usage {
        /// Start date (YYYY-MM-DD).
        #[arg(long)]
        from: Option<String>,
        /// End date (YYYY-MM-DD).
        #[arg(long)]
        to: Option<String>,
    },
    /// Top up credits via Stripe checkout.
    Topup {
        /// Amount in USD to top up.
        amount_usd: u32,
        /// Print the checkout URL instead of opening a browser.
        #[arg(long)]
        no_browser: bool,
    },
    /// Redeem a coupon code.
    Redeem {
        /// Coupon code.
        code: String,
    },
}
