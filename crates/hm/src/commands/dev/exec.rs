//! `hm dev exec` handler.

use anyhow::Result;

use crate::cli::DevExecArgs;
use crate::context::RunContext;

pub async fn handle(_args: DevExecArgs, _ctx: RunContext) -> Result<i32> {
    anyhow::bail!("hm dev exec: not yet implemented")
}
