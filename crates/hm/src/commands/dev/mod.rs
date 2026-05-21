//! `hm dev` — local Docker deployment subcommand tree.
//!
//! Reads `.harmont/*.py` for `@hm.deploy` registrations (via a Python
//! subprocess) and orchestrates long-lived containers on a per-session
//! bridge network. See
//! `docs/superpowers/specs/2026-05-21-hm-dev-deploy-design.md`.

use anyhow::Result;

use crate::cli::DevCommand;
use crate::context::RunContext;

pub mod down;
pub mod exec;
pub mod logmux;
pub mod logs;
pub mod ls;
pub mod naming;
pub mod network;
pub mod port_of;
pub mod registry;
pub mod service_spec;
pub mod topo;
pub mod up;

/// Top-level dispatcher for `hm dev`.
///
/// # Errors
///
/// Returns errors from the subcommand handler.
pub async fn dispatch(command: DevCommand, ctx: RunContext) -> Result<i32> {
    match command {
        DevCommand::Up(args) => up::handle(args, ctx).await,
        DevCommand::Down(args) => down::handle(args, ctx).await,
        DevCommand::Ls => ls::handle(ctx).await,
        DevCommand::Logs(args) => logs::handle(args, ctx).await,
        DevCommand::PortOf(args) => port_of::handle(args, ctx).await,
        DevCommand::Exec(args) => exec::handle(args, ctx).await,
    }
}
