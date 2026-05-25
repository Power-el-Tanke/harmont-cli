//! Workspace provisioning for pipeline steps.
//!
//! Two modes are supported:
//! - **Archive**: builds a tar.gz once, each step extracts it in-container.
//! - **Bind-mount**: creates COW clones per chain, mounted directly into containers.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, bail};
use hm_plugin_protocol::ArchiveId;

use super::archive::ArchiveStore;
use super::source::build_archive_bytes;

/// Manages workspace provisioning for pipeline steps.
///
/// Two modes:
/// - **Archive**: builds a tar.gz once, each step extracts it in-container.
/// - **Bind-mount**: creates COW clones per chain, mounted directly into containers.
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
        clones: Mutex<HashMap<usize, PathBuf>>,
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
                clones: Mutex::new(HashMap::new()),
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

    /// Get or create a COW clone for the given chain. Returns the host
    /// path to mount into the container. Only valid in bind-mount mode.
    pub fn clone_for_chain(&self, chain_id: usize) -> Result<PathBuf> {
        match &self.mode {
            WorkspaceMode::BindMount {
                base_dir,
                clones,
                _temp_dir,
            } => {
                let mut map = clones.lock().unwrap();
                if let Some(path) = map.get(&chain_id) {
                    return Ok(path.clone());
                }
                let clone_path = _temp_dir.path().join(format!("chain-{chain_id}"));
                hm_util::cow_clone::cow_clone_dir(base_dir, &clone_path)
                    .with_context(|| format!("COW clone for chain {chain_id}"))?;
                map.insert(chain_id, clone_path.clone());
                Ok(clone_path)
            }
            WorkspaceMode::Archive { .. } => {
                bail!("clone_for_chain called in archive mode")
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn bind_mount_mode_creates_cow_clone_per_chain() {
        let src = TempDir::new().unwrap();
        fs::write(src.path().join("main.rs"), "fn main() {}").unwrap();

        let mgr = WorkspaceManager::bind_mount(src.path().to_path_buf()).unwrap();

        let chain0 = mgr.clone_for_chain(0).unwrap();
        let chain1 = mgr.clone_for_chain(1).unwrap();

        // Both have the source file
        assert!(chain0.join("main.rs").exists());
        assert!(chain1.join("main.rs").exists());

        // Writes to one don't affect the other
        fs::write(chain0.join("new_file.txt"), "chain0").unwrap();
        assert!(!chain1.join("new_file.txt").exists());
    }

    #[test]
    fn bind_mount_mode_reuses_existing_clone() {
        let src = TempDir::new().unwrap();
        fs::write(src.path().join("a.txt"), "hello").unwrap();

        let mgr = WorkspaceManager::bind_mount(src.path().to_path_buf()).unwrap();

        let first = mgr.clone_for_chain(0).unwrap();
        let second = mgr.clone_for_chain(0).unwrap();
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
    fn clone_for_chain_errors_in_archive_mode() {
        let src = TempDir::new().unwrap();
        fs::write(src.path().join("main.rs"), "fn main() {}").unwrap();

        let archives = Arc::new(ArchiveStore::new());
        let mgr = WorkspaceManager::archive(src.path(), archives).unwrap();

        assert!(mgr.clone_for_chain(0).is_err());
    }
}
