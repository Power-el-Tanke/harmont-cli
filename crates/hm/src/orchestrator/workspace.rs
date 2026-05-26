//! Workspace provisioning for pipeline steps.
//!
//! Two modes are supported:
//! - **Archive**: builds a tar.gz once, each step extracts it in-container.
//! - **Bind-mount**: per-step COW workspace tree that propagates through the DAG.
//!
//! In bind-mount mode, each step gets its own workspace directory. A step's
//! workspace is a COW clone of its parent step's workspace, so artifacts
//! from cached parent steps (e.g. `.venv/`, `node_modules/`) are visible
//! to downstream steps.

#![allow(
    clippy::doc_markdown,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::needless_pass_by_value,
    clippy::option_if_let_else,
    clippy::redundant_clone,
    clippy::significant_drop_tightening,
    clippy::unwrap_used,
    clippy::missing_const_for_fn,
    clippy::used_underscore_binding
)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, bail};
use hm_plugin_protocol::ArchiveId;

use super::archive::ArchiveStore;
use super::docker_client::DockerClient;
use super::source::build_archive_bytes;

/// Manages workspace provisioning for pipeline steps.
#[derive(Debug)]
pub struct WorkspaceManager {
    mode: WorkspaceMode,
}

#[derive(Debug)]
enum WorkspaceMode {
    Archive {
        archive_id: ArchiveId,
    },
    BindMount {
        base_dir: PathBuf,
        /// Per-step workspace directories. Key is step_key.
        step_workspaces: Mutex<HashMap<String, PathBuf>>,
        _temp_dir: tempfile::TempDir,
    },
}

impl WorkspaceManager {
    /// Create in archive mode: builds tar.gz and registers in the store.
    pub fn archive(repo_root: &Path, store: Arc<ArchiveStore>) -> Result<Self> {
        let bytes = build_archive_bytes(repo_root).context("build source archive")?;
        let id = store.register(bytes);
        Ok(Self {
            mode: WorkspaceMode::Archive { archive_id: id },
        })
    }

    /// Create in bind-mount mode: the repo root IS the workspace source.
    pub fn bind_mount(repo_root: PathBuf) -> Result<Self> {
        let temp_dir = tempfile::Builder::new()
            .prefix("hm-workspace-")
            .tempdir()
            .context("create workspace temp dir")?;
        Ok(Self {
            mode: WorkspaceMode::BindMount {
                base_dir: repo_root,
                step_workspaces: Mutex::new(HashMap::new()),
                _temp_dir: temp_dir,
            },
        })
    }

    /// Get the archive ID (only valid in archive mode).
    #[must_use]
    pub fn archive_id(&self) -> Option<ArchiveId> {
        match &self.mode {
            WorkspaceMode::Archive { archive_id } => Some(*archive_id),
            WorkspaceMode::BindMount { .. } => None,
        }
    }

    /// Is this manager in bind-mount mode?
    #[must_use]
    pub fn is_bind_mount(&self) -> bool {
        matches!(self.mode, WorkspaceMode::BindMount { .. })
    }

    /// Get the workspace path for a step that already has one registered.
    pub fn get_step_workspace(&self, step_key: &str) -> Option<PathBuf> {
        match &self.mode {
            WorkspaceMode::BindMount {
                step_workspaces, ..
            } => step_workspaces.lock().unwrap().get(step_key).cloned(),
            WorkspaceMode::Archive { .. } => None,
        }
    }

    /// Create a workspace for a step by COW-cloning its parent's workspace.
    /// If `parent_step_key` is None, clones the base source directory.
    pub fn clone_for_step(&self, step_key: &str, parent_step_key: Option<&str>) -> Result<PathBuf> {
        match &self.mode {
            WorkspaceMode::BindMount {
                base_dir,
                step_workspaces,
                _temp_dir,
            } => {
                let mut map = step_workspaces.lock().unwrap();
                if let Some(existing) = map.get(step_key) {
                    return Ok(existing.clone());
                }

                let source = if let Some(parent_key) = parent_step_key {
                    map.get(parent_key)
                        .cloned()
                        .unwrap_or_else(|| base_dir.clone())
                } else {
                    base_dir.clone()
                };

                let clone_path = _temp_dir.path().join(format!("step-{step_key}"));
                hm_util::cow_clone::cow_clone_dir(&source, &clone_path)
                    .with_context(|| format!("COW clone for step '{step_key}'"))?;
                map.insert(step_key.to_owned(), clone_path.clone());
                Ok(clone_path)
            }
            WorkspaceMode::Archive { .. } => {
                bail!("clone_for_step called in archive mode")
            }
        }
    }

    /// Populate a step's workspace by extracting /workspace from a cached
    /// Docker image. Used for cache hits so downstream steps inherit artifacts.
    pub async fn populate_from_cached_image(
        &self,
        step_key: &str,
        image_tag: &str,
        workdir: &str,
        docker: &DockerClient,
    ) -> Result<PathBuf> {
        match &self.mode {
            WorkspaceMode::BindMount {
                base_dir,
                step_workspaces,
                _temp_dir,
            } => {
                let clone_path = _temp_dir.path().join(format!("step-{step_key}"));

                // Extract cached image workspace first (has artifacts like
                // node_modules/, .venv/, target/).
                docker
                    .extract_workspace_from_image(image_tag, workdir, &clone_path)
                    .await
                    .with_context(|| {
                        format!("extract workspace from cached image '{image_tag}'")
                    })?;

                // Overlay fresh source on top so working-tree edits always
                // win over stale source files baked into the cache.
                hm_util::cow_clone::overlay_source(base_dir, &clone_path)
                    .with_context(|| format!("overlay source for cached step '{step_key}'"))?;

                let mut map = step_workspaces.lock().unwrap();
                map.insert(step_key.to_owned(), clone_path.clone());
                Ok(clone_path)
            }
            WorkspaceMode::Archive { .. } => {
                bail!("populate_from_cached_image called in archive mode")
            }
        }
    }

    // Keep backward compat for container pool (uses chain_id)
    #[doc(hidden)]
    pub fn clone_for_chain(&self, chain_id: usize) -> Result<PathBuf> {
        self.clone_for_step(&format!("__chain_{chain_id}"), None)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn bind_mount_clone_for_step_creates_independent_copies() {
        let src = TempDir::new().unwrap();
        fs::write(src.path().join("main.rs"), "fn main() {}").unwrap();

        let mgr = WorkspaceManager::bind_mount(src.path().to_path_buf()).unwrap();

        let ws_a = mgr.clone_for_step("step-a", None).unwrap();
        let ws_b = mgr.clone_for_step("step-b", None).unwrap();

        assert!(ws_a.join("main.rs").exists());
        assert!(ws_b.join("main.rs").exists());

        // Writes to one don't affect the other
        fs::write(ws_a.join("artifact.txt"), "from-a").unwrap();
        assert!(!ws_b.join("artifact.txt").exists());
    }

    #[test]
    fn bind_mount_clone_from_parent_inherits_artifacts() {
        let src = TempDir::new().unwrap();
        fs::write(src.path().join("main.rs"), "fn main() {}").unwrap();

        let mgr = WorkspaceManager::bind_mount(src.path().to_path_buf()).unwrap();

        // Step A runs and produces an artifact
        let ws_a = mgr.clone_for_step("step-a", None).unwrap();
        fs::write(ws_a.join("node_modules.txt"), "installed").unwrap();

        // Step B clones from A — should see A's artifact
        let ws_b = mgr.clone_for_step("step-b", Some("step-a")).unwrap();
        assert_eq!(
            fs::read_to_string(ws_b.join("node_modules.txt")).unwrap(),
            "installed"
        );
        assert!(ws_b.join("main.rs").exists());

        // Writes to B don't affect A
        fs::write(ws_b.join("new.txt"), "b-only").unwrap();
        assert!(!ws_a.join("new.txt").exists());
    }

    #[test]
    fn bind_mount_reuses_existing_step_workspace() {
        let src = TempDir::new().unwrap();
        fs::write(src.path().join("a.txt"), "hello").unwrap();

        let mgr = WorkspaceManager::bind_mount(src.path().to_path_buf()).unwrap();

        let first = mgr.clone_for_step("x", None).unwrap();
        let second = mgr.clone_for_step("x", None).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn archive_mode_returns_archive_id() {
        let src = TempDir::new().unwrap();
        fs::write(src.path().join("main.rs"), "fn main() {}").unwrap();

        let archives = Arc::new(ArchiveStore::new());
        let mgr = WorkspaceManager::archive(src.path(), archives.clone()).unwrap();

        assert!(mgr.archive_id().is_some());
        assert!(!mgr.is_bind_mount());
    }

    #[test]
    fn clone_for_step_errors_in_archive_mode() {
        let src = TempDir::new().unwrap();
        fs::write(src.path().join("main.rs"), "fn main() {}").unwrap();

        let archives = Arc::new(ArchiveStore::new());
        let mgr = WorkspaceManager::archive(src.path(), archives).unwrap();

        assert!(mgr.clone_for_step("x", None).is_err());
    }
}
