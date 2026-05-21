//! `hm dev down` handler.

use anyhow::Result;

use crate::cli::DevDownArgs;
use crate::context::RunContext;

pub async fn handle(_args: DevDownArgs, _ctx: RunContext) -> Result<i32> {
    anyhow::bail!("hm dev down: not yet implemented")
}
