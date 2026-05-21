//! `hm dev exec` handler.

use anyhow::Result;

use crate::cli::DevExecArgs;
use crate::context::RunContext;

/// # Errors
///
/// Always errors — not yet implemented.
#[expect(clippy::unused_async, reason = "signature required by dispatcher; impl lands in a later task")]
pub async fn handle(_args: DevExecArgs, _ctx: RunContext) -> Result<i32> {
    anyhow::bail!("hm dev exec: not yet implemented")
}
