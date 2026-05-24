//! Verb implementations for `hm cloud <subcommand>`. Each module
//! exposes a `run(ctx, env, verb, args)` entry point that
//! `cli::dispatch` calls with JSON args extracted by the host.

pub(crate) mod billing;
pub(crate) mod build;
pub(crate) mod job;
pub(crate) mod org;
pub(crate) mod pipeline;
pub(crate) mod run;
