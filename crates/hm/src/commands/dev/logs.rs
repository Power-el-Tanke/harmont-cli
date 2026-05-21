//! `hm dev logs` handler.

use anyhow::Result;

use crate::cli::DevLogsArgs;
use crate::context::RunContext;

/// # Errors
///
/// Always errors — not yet implemented.
#[expect(clippy::unused_async, reason = "signature required by dispatcher; impl lands in a later task")]
pub async fn handle(_args: DevLogsArgs, _ctx: RunContext) -> Result<i32> {
    anyhow::bail!("hm dev logs: not yet implemented")
}
