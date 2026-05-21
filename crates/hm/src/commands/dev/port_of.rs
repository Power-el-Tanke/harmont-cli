//! `hm dev port-of` handler.

use anyhow::Result;

use crate::cli::DevPortOfArgs;
use crate::context::RunContext;

/// # Errors
///
/// Always errors — not yet implemented.
#[expect(clippy::unused_async, reason = "signature required by dispatcher; impl lands in a later task")]
pub async fn handle(_args: DevPortOfArgs, _ctx: RunContext) -> Result<i32> {
    anyhow::bail!("hm dev port-of: not yet implemented")
}
