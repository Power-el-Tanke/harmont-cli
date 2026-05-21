//! `hm dev logs` handler.

use anyhow::Result;

use crate::cli::DevLogsArgs;
use crate::context::RunContext;

pub async fn handle(_args: DevLogsArgs, _ctx: RunContext) -> Result<i32> {
    anyhow::bail!("hm dev logs: not yet implemented")
}
