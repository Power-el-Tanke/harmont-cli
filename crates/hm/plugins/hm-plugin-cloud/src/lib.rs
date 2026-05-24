//! Built-in cloud client plugin for the hm CLI.
//!
//! Implements `hm cloud {login,logout,whoami,org,pipeline,build,job,billing,run}`.
//! HTTP traffic goes through reqwest directly (native dylib, no WASM sandbox).

#![allow(unsafe_code)]
#![allow(
    clippy::pedantic,
    clippy::nursery,
    clippy::cargo,
    clippy::multiple_crate_versions,
    clippy::cargo_common_metadata,
    clippy::missing_errors_doc,
)]

mod api;
mod auth;
mod cli;
mod config;
mod creds;
mod http;
mod output;
mod state;
mod verbs;

use core::future::Future;
use hm_plugin_sdk::*;

#[derive(Default)]
struct Cloud;

impl SubcommandPlugin for Cloud {
    fn run<'a>(
        &'a self,
        ctx: &'a PluginContext<'a>,
        input: SubcommandInput,
    ) -> impl Future<Output = Result<ExitInfo, PluginError>> + Send + 'a {
        async move { cli::dispatch(ctx, input).await }
    }
}

hm_plugin!(
    manifest = PluginManifest {
        api_version: HM_PLUGIN_API_VERSION,
        name: "harmont-cloud".into(),
        version: semver::Version::new(0, 1, 0),
        description: "Cloud client: login, whoami, org, pipeline, build, job, billing, run.".into(),
        capabilities: vec![Capability::Subcommand(SubcommandSpec {
            verb: "cloud".into(),
            about: "Talk to the Harmont cloud API".into(),
            args: vec![],
            subcommands: vec![
                SubcommandSpec {
                    verb: "login".into(),
                    about: "Authenticate this CLI against the Harmont API".into(),
                    args: vec![ArgSpec::Flag {
                        long: "paste".into(),
                        short: None,
                        help: Some("Skip the loopback flow and prompt for a paste-in code".into()),
                    }],
                    subcommands: vec![],
                },
                SubcommandSpec {
                    verb: "logout".into(),
                    about: "Remove stored credentials".into(),
                    args: vec![],
                    subcommands: vec![],
                },
                SubcommandSpec {
                    verb: "whoami".into(),
                    about: "Show the authenticated user".into(),
                    args: vec![],
                    subcommands: vec![],
                },
                SubcommandSpec {
                    verb: "org".into(),
                    about: "Manage organizations".into(),
                    args: vec![],
                    subcommands: vec![SubcommandSpec {
                        verb: "switch".into(),
                        about: "Set the active organization".into(),
                        args: vec![ArgSpec::Positional {
                            name: "slug".into(),
                            help: Some("Organization slug".into()),
                            required: true,
                            value_type: ValueType::String,
                        }],
                        subcommands: vec![],
                    }],
                },
                SubcommandSpec {
                    verb: "pipeline".into(),
                    about: "Manage pipelines".into(),
                    args: vec![],
                    subcommands: vec![
                        SubcommandSpec {
                            verb: "list".into(),
                            about: "List pipelines for the active organization".into(),
                            args: vec![],
                            subcommands: vec![],
                        },
                        SubcommandSpec {
                            verb: "show".into(),
                            about: "Show pipeline details by slug".into(),
                            args: vec![ArgSpec::Positional {
                                name: "slug".into(),
                                help: Some("Pipeline slug".into()),
                                required: true,
                                value_type: ValueType::String,
                            }],
                            subcommands: vec![],
                        },
                    ],
                },
                SubcommandSpec {
                    verb: "build".into(),
                    about: "Manage builds".into(),
                    args: vec![],
                    subcommands: vec![
                        SubcommandSpec {
                            verb: "list".into(),
                            about: "List builds for a pipeline".into(),
                            args: vec![ArgSpec::Option {
                                long: "pipeline".into(),
                                short: Some('p'),
                                help: Some("Pipeline slug".into()),
                                required: true,
                                value_type: ValueType::String,
                                default: None,
                            }],
                            subcommands: vec![],
                        },
                        SubcommandSpec {
                            verb: "show".into(),
                            about: "Show a build by number".into(),
                            args: vec![
                                ArgSpec::Option {
                                    long: "pipeline".into(),
                                    short: Some('p'),
                                    help: Some("Pipeline slug".into()),
                                    required: true,
                                    value_type: ValueType::String,
                                    default: None,
                                },
                                ArgSpec::Positional {
                                    name: "number".into(),
                                    help: Some("Build number".into()),
                                    required: true,
                                    value_type: ValueType::Int,
                                },
                            ],
                            subcommands: vec![],
                        },
                        SubcommandSpec {
                            verb: "cancel".into(),
                            about: "Cancel a build".into(),
                            args: vec![
                                ArgSpec::Option {
                                    long: "pipeline".into(),
                                    short: Some('p'),
                                    help: Some("Pipeline slug".into()),
                                    required: true,
                                    value_type: ValueType::String,
                                    default: None,
                                },
                                ArgSpec::Positional {
                                    name: "number".into(),
                                    help: Some("Build number".into()),
                                    required: true,
                                    value_type: ValueType::Int,
                                },
                            ],
                            subcommands: vec![],
                        },
                        SubcommandSpec {
                            verb: "watch".into(),
                            about: "Watch a build until it reaches a terminal state".into(),
                            args: vec![
                                ArgSpec::Option {
                                    long: "pipeline".into(),
                                    short: Some('p'),
                                    help: Some("Pipeline slug".into()),
                                    required: true,
                                    value_type: ValueType::String,
                                    default: None,
                                },
                                ArgSpec::Positional {
                                    name: "number".into(),
                                    help: Some("Build number".into()),
                                    required: true,
                                    value_type: ValueType::Int,
                                },
                            ],
                            subcommands: vec![],
                        },
                    ],
                },
                SubcommandSpec {
                    verb: "job".into(),
                    about: "Manage jobs".into(),
                    args: vec![],
                    subcommands: vec![
                        SubcommandSpec {
                            verb: "list".into(),
                            about: "List jobs in a build".into(),
                            args: vec![
                                ArgSpec::Option {
                                    long: "pipeline".into(),
                                    short: Some('p'),
                                    help: Some("Pipeline slug".into()),
                                    required: true,
                                    value_type: ValueType::String,
                                    default: None,
                                },
                                ArgSpec::Option {
                                    long: "build".into(),
                                    short: Some('b'),
                                    help: Some("Build number".into()),
                                    required: true,
                                    value_type: ValueType::Int,
                                    default: None,
                                },
                            ],
                            subcommands: vec![],
                        },
                        SubcommandSpec {
                            verb: "show".into(),
                            about: "Show a job by id".into(),
                            args: vec![
                                ArgSpec::Option {
                                    long: "pipeline".into(),
                                    short: Some('p'),
                                    help: Some("Pipeline slug".into()),
                                    required: true,
                                    value_type: ValueType::String,
                                    default: None,
                                },
                                ArgSpec::Option {
                                    long: "build".into(),
                                    short: Some('b'),
                                    help: Some("Build number".into()),
                                    required: true,
                                    value_type: ValueType::Int,
                                    default: None,
                                },
                                ArgSpec::Positional {
                                    name: "job_id".into(),
                                    help: Some("Job ID".into()),
                                    required: true,
                                    value_type: ValueType::String,
                                },
                            ],
                            subcommands: vec![],
                        },
                        SubcommandSpec {
                            verb: "log".into(),
                            about: "Print the job log".into(),
                            args: vec![
                                ArgSpec::Option {
                                    long: "pipeline".into(),
                                    short: Some('p'),
                                    help: Some("Pipeline slug".into()),
                                    required: true,
                                    value_type: ValueType::String,
                                    default: None,
                                },
                                ArgSpec::Option {
                                    long: "build".into(),
                                    short: Some('b'),
                                    help: Some("Build number".into()),
                                    required: true,
                                    value_type: ValueType::Int,
                                    default: None,
                                },
                                ArgSpec::Positional {
                                    name: "job_id".into(),
                                    help: Some("Job ID".into()),
                                    required: true,
                                    value_type: ValueType::String,
                                },
                            ],
                            subcommands: vec![],
                        },
                    ],
                },
                SubcommandSpec {
                    verb: "billing".into(),
                    about: "Manage credits, top-ups, and usage".into(),
                    args: vec![],
                    subcommands: vec![
                        SubcommandSpec {
                            verb: "balance".into(),
                            about: "Print the current credit balance".into(),
                            args: vec![],
                            subcommands: vec![],
                        },
                        SubcommandSpec {
                            verb: "transactions".into(),
                            about: "List billing transactions".into(),
                            args: vec![ArgSpec::Option {
                                long: "limit".into(),
                                short: None,
                                help: Some("Maximum number of transactions to show".into()),
                                required: false,
                                value_type: ValueType::Int,
                                default: Some("100".into()),
                            }],
                            subcommands: vec![],
                        },
                        SubcommandSpec {
                            verb: "usage".into(),
                            about: "Show usage over a time window".into(),
                            args: vec![
                                ArgSpec::Option {
                                    long: "from".into(),
                                    short: None,
                                    help: Some("Start date (YYYY-MM-DD)".into()),
                                    required: false,
                                    value_type: ValueType::String,
                                    default: None,
                                },
                                ArgSpec::Option {
                                    long: "to".into(),
                                    short: None,
                                    help: Some("End date (YYYY-MM-DD)".into()),
                                    required: false,
                                    value_type: ValueType::String,
                                    default: None,
                                },
                            ],
                            subcommands: vec![],
                        },
                        SubcommandSpec {
                            verb: "topup".into(),
                            about: "Top up credits via Stripe checkout".into(),
                            args: vec![
                                ArgSpec::Positional {
                                    name: "amount_usd".into(),
                                    help: Some("Amount in USD to top up".into()),
                                    required: true,
                                    value_type: ValueType::Int,
                                },
                                ArgSpec::Flag {
                                    long: "no_browser".into(),
                                    short: None,
                                    help: Some("Print the checkout URL instead of opening a browser".into()),
                                },
                            ],
                            subcommands: vec![],
                        },
                        SubcommandSpec {
                            verb: "redeem".into(),
                            about: "Redeem a coupon code".into(),
                            args: vec![ArgSpec::Positional {
                                name: "code".into(),
                                help: Some("Coupon code".into()),
                                required: true,
                                value_type: ValueType::String,
                            }],
                            subcommands: vec![],
                        },
                    ],
                },
                SubcommandSpec {
                    verb: "run".into(),
                    about: "Submit the local pipeline to the cloud and watch its build".into(),
                    args: vec![
                        ArgSpec::Positional {
                            name: "pipeline".into(),
                            help: Some("Pipeline slug".into()),
                            required: true,
                            value_type: ValueType::String,
                        },
                        ArgSpec::Option {
                            long: "branch".into(),
                            short: Some('b'),
                            help: Some("Branch to record on the build".into()),
                            required: false,
                            value_type: ValueType::String,
                            default: None,
                        },
                        ArgSpec::Option {
                            long: "message".into(),
                            short: Some('m'),
                            help: Some("Build message".into()),
                            required: false,
                            value_type: ValueType::String,
                            default: None,
                        },
                        ArgSpec::Option {
                            long: "plan_file".into(),
                            short: None,
                            help: Some("Path to a pre-rendered pipeline JSON file".into()),
                            required: false,
                            value_type: ValueType::String,
                            default: None,
                        },
                        ArgSpec::Flag {
                            long: "no_watch".into(),
                            short: None,
                            help: Some("Don't watch; print the build URL and exit".into()),
                        },
                    ],
                    subcommands: vec![],
                },
            ],
        })],
        config_schema: None,
    },
    subcommand = Cloud,
);
