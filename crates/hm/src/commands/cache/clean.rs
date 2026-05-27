use anyhow::Result;

pub async fn handle_clean() -> Result<i32> {
    let mut cleaned = false;

    // 1. Remove workspace cache directory.
    if let Some(ws_cache) = hm_util::dirs::harmont_workspace_cache_dir() {
        if ws_cache.exists() {
            let size = dir_size(&ws_cache);
            std::fs::remove_dir_all(&ws_cache)?;
            tracing::info!(
                path = %ws_cache.display(),
                "removed workspace cache ({})",
                human_bytes(size),
            );
            cleaned = true;
        }
    }

    // 2. Connect to Docker for image cleanup.
    let docker = match crate::orchestrator::docker_client::DockerClient::connect() {
        Ok(d) => match d.ping().await {
            Ok(()) => Some(d),
            Err(e) => {
                tracing::warn!(%e, "Docker daemon unreachable — skipping image cleanup");
                None
            }
        },
        Err(e) => {
            tracing::warn!(%e, "cannot connect to Docker — skipping image cleanup");
            None
        }
    };

    if let Some(docker) = &docker {
        // 3. Remove harmont-cache/* Docker images.
        let cache_images = docker.list_images_by_prefix("harmont-cache/").await?;
        for tag in &cache_images {
            if let Err(e) = docker.remove_image(tag).await {
                tracing::warn!(image = %tag, %e, "failed to remove cached image");
            } else {
                tracing::info!(image = %tag, "removed cached Docker image");
                cleaned = true;
            }
        }

        // 4. Remove harmont-local-ephemeral/* Docker images.
        let ephemeral_images = docker
            .list_images_by_prefix("harmont-local-ephemeral/")
            .await?;
        for tag in &ephemeral_images {
            if let Err(e) = docker.remove_image(tag).await {
                tracing::warn!(image = %tag, %e, "failed to remove ephemeral image");
            } else {
                tracing::info!(image = %tag, "removed ephemeral Docker image");
                cleaned = true;
            }
        }
    }

    if !cleaned {
        tracing::info!("nothing to clean");
    }

    Ok(0)
}

fn dir_size(path: &std::path::Path) -> u64 {
    fn walk(p: &std::path::Path) -> u64 {
        std::fs::read_dir(p)
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .map(|e| {
                let path = e.path();
                if path.is_dir() {
                    walk(&path)
                } else {
                    e.metadata().map(|m| m.len()).unwrap_or(0)
                }
            })
            .sum()
    }
    walk(path)
}

fn human_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes}B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1}KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1}MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1}GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}
