//! `hm dev up` handler.

use anyhow::Result;

use crate::cli::DevUpArgs;
use crate::context::RunContext;

pub async fn handle(_args: DevUpArgs, _ctx: RunContext) -> Result<i32> {
    anyhow::bail!("hm dev up: not yet implemented")
}
