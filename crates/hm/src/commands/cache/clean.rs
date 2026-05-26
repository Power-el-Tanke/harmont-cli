use anyhow::Result;

use crate::cli::CacheCleanArgs;
use crate::orchestrator::docker_client::DockerClient;

/// # Errors
///
/// Returns an error if Docker is unreachable or image removal fails.
pub async fn handle_clean(args: CacheCleanArgs) -> Result<i32> {
    let docker = DockerClient::connect()?;
    docker.ping().await?;

    let removed = if let Some(days) = args.older_than_days {
        let max_age = std::time::Duration::from_secs(days * 24 * 3600);
        docker.evict_stale_cache_images(max_age).await?
    } else {
        docker.purge_all_cache_images().await?
    };

    if removed == 0 {
        tracing::info!("no cache images to remove");
    } else {
        tracing::info!(count = removed, "removed cache images");
    }
    Ok(0)
}
