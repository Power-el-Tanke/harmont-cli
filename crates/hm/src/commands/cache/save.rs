use std::path::Path;

use anyhow::{Context, Result};
use tracing::info;

use super::manifest::{self, Manifest};
use crate::orchestrator::docker_client::DockerClient;

/// Save all `harmont-local/*` images to a cache directory as tar files,
/// write a manifest, and prune stale tars that no longer correspond to
/// any known image.
///
/// Prints the manifest's content hash to stdout so CI runners (e.g.
/// GitHub Actions) can capture it for use as a cache key.
///
/// # Errors
///
/// Returns an error if the Docker daemon is unreachable, an image
/// export fails, or any filesystem operation on `dir` fails.
pub async fn handle_save(dir: &Path) -> Result<i32> {
    let docker = DockerClient::connect()?;
    docker.ping().await?;

    tokio::fs::create_dir_all(dir)
        .await
        .with_context(|| format!("create cache dir {}", dir.display()))?;

    // 1. List all harmont-local/* images (list_images_by_prefix already skips ephemeral)
    let tags = docker.list_images_by_prefix("harmont-local/").await?;

    let mut manifest = Manifest::new();

    // 2. For each image, save to tar if not already on disk
    for tag in &tags {
        let filename = manifest::tar_name_for_tag(tag);
        let tar_path = dir.join(&filename);

        if tar_path.exists() {
            info!("skip (exists): {filename}");
        } else {
            info!("save: {tag} → {filename}");
            docker.export_image(tag, &tar_path).await?;
        }

        manifest.images.insert(filename, tag.clone());
    }

    // 3. Write manifest.json
    let manifest_json = serde_json::to_string_pretty(&manifest)?;
    tokio::fs::write(dir.join("manifest.json"), &manifest_json)
        .await
        .context("write manifest.json")?;

    // 4. Prune stale tars
    let mut entries = tokio::fs::read_dir(dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.ends_with(".tar") && !manifest.images.contains_key(name_str.as_ref()) {
            info!("prune stale: {name_str}");
            tokio::fs::remove_file(entry.path()).await.ok();
        }
    }

    // 5. Print content hash to stdout (GHA captures this for the cache key)
    let hash = manifest.content_hash();
    #[allow(clippy::print_stdout, reason = "hash must go to stdout for CI capture")]
    {
        println!("{hash}");
    }

    Ok(0)
}
