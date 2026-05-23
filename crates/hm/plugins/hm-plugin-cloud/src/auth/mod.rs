//! `hm cloud login | logout | whoami`.
//!
//! Each submodule exposes a single `run(env, ...)` entry point that
//! the plugin's `cli::dispatch` calls once it has parsed argv.

pub(crate) mod login;
pub(crate) mod logout;
pub(crate) mod whoami;
