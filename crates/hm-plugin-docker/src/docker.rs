//! Bollard-based Docker client for the step-executor plugin.
//!
//! Ported from the host-side `docker_client.rs`. The key difference is
//! that exec output is streamed through [`PluginContext`] rather than
//! an [`AsyncWrite`] sink.

use std::collections::HashMap;
use std::sync::Arc;

use bollard::Docker;
use bollard::container::{
    Config, CreateContainerOptions, RemoveContainerOptions, StartContainerOptions,
    StopContainerOptions,
};
use bollard::exec::{CreateExecOptions, StartExecResults};
use bollard::image::{CommitContainerOptions, CreateImageOptions, ListImagesOptions};
use futures_util::StreamExt;
use hm_plugin_protocol::{ArchiveId, PluginError};
use hm_plugin_sdk::PluginContext;
use tokio::io::AsyncWriteExt;

#[derive(Debug, Clone)]
pub(crate) struct DockerClient {
    inner: Arc<Docker>,
}

impl DockerClient {
    /// Connect to the local Docker daemon using platform defaults.
    pub(crate) fn connect() -> Result<Self, PluginError> {
        let d = Docker::connect_with_local_defaults()
            .map_err(|e| PluginError::new("docker_connect", format!("connect: {e}")))?;
        Ok(Self { inner: Arc::new(d) })
    }

    /// True if `tag` resolves to a locally-cached image.
    pub(crate) async fn image_exists(&self, tag: &str) -> Result<bool, PluginError> {
        let mut filters = HashMap::new();
        filters.insert("reference".to_string(), vec![tag.to_string()]);
        let images = self
            .inner
            .list_images(Some(ListImagesOptions {
                filters,
                ..Default::default()
            }))
            .await
            .map_err(|e| PluginError::new("docker_image_exists", format!("list_images: {e}")))?;
        Ok(!images.is_empty())
    }

    /// Pull `tag` from its registry, draining the progress stream.
    pub(crate) async fn pull_image(&self, tag: &str) -> Result<(), PluginError> {
        let mut s = self.inner.create_image(
            Some(CreateImageOptions {
                from_image: tag,
                ..Default::default()
            }),
            None,
            None,
        );
        while let Some(item) = s.next().await {
            item.map_err(|e| PluginError::new("docker_pull", format!("pull {tag}: {e}")))?;
        }
        Ok(())
    }

    /// Start a long-lived container running `sleep infinity`.
    /// Returns the container ID.
    pub(crate) async fn start_long_lived(
        &self,
        image: &str,
        env: &[String],
        workdir: &str,
        name: &str,
    ) -> Result<String, PluginError> {
        let cfg = Config {
            image: Some(image.to_string()),
            cmd: Some(vec!["sh".into(), "-c".into(), "sleep infinity".into()]),
            env: Some(env.to_vec()),
            working_dir: Some(workdir.to_string()),
            ..Default::default()
        };
        let create = self
            .inner
            .create_container(
                Some(CreateContainerOptions {
                    name,
                    ..Default::default()
                }),
                cfg,
            )
            .await
            .map_err(|e| PluginError::new("docker_start", format!("create_container: {e}")))?;
        self.inner
            .start_container(&create.id, None::<StartContainerOptions<String>>)
            .await
            .map_err(|e| PluginError::new("docker_start", format!("start_container: {e}")))?;
        Ok(create.id)
    }

    /// Exec a command inside a running container, streaming
    /// stdout/stderr through the plugin context. Returns the exit code.
    pub(crate) async fn exec_streaming(
        &self,
        container_id: &str,
        cmd: &[String],
        env: &[String],
        workdir: &str,
        ctx: &PluginContext<'_>,
    ) -> Result<i32, PluginError> {
        use bollard::container::LogOutput;

        let exec = self
            .inner
            .create_exec(
                container_id,
                CreateExecOptions {
                    cmd: Some(cmd.iter().map(String::as_str).collect()),
                    env: Some(env.iter().map(String::as_str).collect()),
                    working_dir: Some(workdir),
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    ..Default::default()
                },
            )
            .await
            .map_err(|e| PluginError::new("docker_exec", format!("create_exec: {e}")))?;

        match self
            .inner
            .start_exec(&exec.id, None)
            .await
            .map_err(|e| PluginError::new("docker_exec", format!("start_exec: {e}")))?
        {
            StartExecResults::Attached { mut output, .. } => {
                while let Some(item) = output.next().await {
                    let chunk =
                        item.map_err(|e| PluginError::new("docker_exec", format!("exec stream: {e}")))?;
                    match chunk {
                        LogOutput::StdOut { message } => {
                            ctx.emit_step_log_stdout(&message);
                        }
                        LogOutput::StdErr { message } => {
                            ctx.emit_step_log_stderr(&message);
                        }
                        LogOutput::Console { message } => {
                            ctx.emit_step_log_stdout(&message);
                        }
                        LogOutput::StdIn { .. } => {
                            // StdIn frames echoed by some daemons; ignore.
                        }
                    }
                }
            }
            StartExecResults::Detached => {}
        }

        let inspect = self
            .inner
            .inspect_exec(&exec.id)
            .await
            .map_err(|e| PluginError::new("docker_exec", format!("inspect_exec: {e}")))?;
        let code = inspect.exit_code.unwrap_or(0);
        Ok(i32::try_from(code).unwrap_or(1))
    }

    /// Extract the workspace archive into a container.
    ///
    /// Reads the archive from the host via `ctx.archive_read()` in
    /// chunks, then pipes the bytes into `tar -xzf -` via exec stdin.
    pub(crate) async fn extract_workspace(
        &self,
        ctx: &PluginContext<'_>,
        container_id: &str,
        archive_id: &ArchiveId,
        workdir: &str,
    ) -> Result<(), PluginError> {
        // Read the full archive from the host in chunks.
        let total = ctx.archive_total_size(archive_id);
        let chunk_size: u64 = 256 * 1024; // 256 KiB chunks
        let mut archive_bytes = Vec::with_capacity(total as usize);
        let mut offset: u64 = 0;
        while offset < total {
            let chunk = ctx.archive_read(archive_id, offset, chunk_size);
            if chunk.is_empty() {
                break;
            }
            offset += chunk.len() as u64;
            archive_bytes.extend_from_slice(&chunk);
        }

        // Pipe the archive into `tar -xzf -` inside the container.
        let cmd: Vec<String> = vec!["tar".into(), "-xzf".into(), "-".into()];
        let exec = self
            .inner
            .create_exec(
                container_id,
                CreateExecOptions {
                    cmd: Some(cmd.iter().map(String::as_str).collect()),
                    env: Some(Vec::new()),
                    working_dir: Some(workdir),
                    attach_stdin: Some(true),
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    ..Default::default()
                },
            )
            .await
            .map_err(|e| PluginError::new("docker_extract", format!("create_exec: {e}")))?;

        match self
            .inner
            .start_exec(&exec.id, None)
            .await
            .map_err(|e| PluginError::new("docker_extract", format!("start_exec: {e}")))?
        {
            StartExecResults::Attached {
                mut output,
                mut input,
            } => {
                input
                    .write_all(&archive_bytes)
                    .await
                    .map_err(|e| PluginError::new("docker_extract", format!("write stdin: {e}")))?;
                input
                    .shutdown()
                    .await
                    .map_err(|e| PluginError::new("docker_extract", format!("close stdin: {e}")))?;
                drop(input);
                // Drain output (tar may write warnings to stderr).
                while let Some(item) = output.next().await {
                    let _chunk = item.map_err(|e| {
                        PluginError::new("docker_extract", format!("exec stream: {e}"))
                    })?;
                }
            }
            StartExecResults::Detached => {}
        }

        let inspect = self
            .inner
            .inspect_exec(&exec.id)
            .await
            .map_err(|e| PluginError::new("docker_extract", format!("inspect_exec: {e}")))?;
        let code = inspect.exit_code.unwrap_or(0);
        if code != 0 {
            return Err(PluginError::new(
                "docker_extract",
                format!("tar exited with code {code}"),
            ));
        }
        Ok(())
    }

    /// Commit a running container to an image tag.
    pub(crate) async fn commit_container(
        &self,
        container_id: &str,
        tag: &str,
    ) -> Result<(), PluginError> {
        let parts: Vec<&str> = tag.splitn(2, ':').collect();
        let (repo, ver) = match parts.as_slice() {
            [r, v] => (*r, *v),
            [r] => (*r, "latest"),
            _ => unreachable!("splitn(2) yields one or two parts for non-empty input"),
        };
        let opts = CommitContainerOptions {
            container: container_id,
            repo,
            tag: ver,
            ..Default::default()
        };
        self.inner
            .commit_container(opts, Config::<String>::default())
            .await
            .map_err(|e| PluginError::new("docker_commit", format!("commit_container: {e}")))?;
        Ok(())
    }

    /// Stop and force-remove a container. Best-effort; errors are
    /// silently swallowed.
    pub(crate) async fn stop_remove(&self, container_id: &str) {
        let _ = self
            .inner
            .stop_container(container_id, Some(StopContainerOptions { t: 0 }))
            .await;
        let _ = self
            .inner
            .remove_container(
                container_id,
                Some(RemoveContainerOptions {
                    force: true,
                    v: true,
                    ..Default::default()
                }),
            )
            .await;
    }
}
