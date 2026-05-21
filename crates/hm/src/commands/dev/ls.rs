//! `hm dev ls` handler.

use anyhow::Result;

use crate::context::RunContext;

/// # Errors
///
/// Always errors — not yet implemented.
#[expect(clippy::unused_async, reason = "signature required by dispatcher; impl lands in a later task")]
pub async fn handle(_ctx: RunContext) -> Result<i32> {
    anyhow::bail!("hm dev ls: not yet implemented")
}
