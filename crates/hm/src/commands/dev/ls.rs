//! `hm dev ls` handler.

use anyhow::Result;

use crate::context::RunContext;

pub async fn handle(_ctx: RunContext) -> Result<i32> {
    anyhow::bail!("hm dev ls: not yet implemented")
}
