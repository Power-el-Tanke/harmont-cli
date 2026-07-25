//! Filesystem helpers returning rich, typed errors.
//!
//! These wrap the corresponding [`std::fs`] operations, attaching the target
//! path and the failing syscall's [`io::Error`] as a structured [`FsError`]
//! rather than a stringly-typed context message.

use std::io;
use std::path::{Path, PathBuf};

/// An error from one of this module's filesystem helpers.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum FsError {
    /// A directory could not be created.
    #[error("failed to create directory `{path}`")]
    CreateDir {
        /// The directory whose creation failed.
        path: PathBuf,
        /// The underlying OS error.
        #[source]
        source: io::Error,
    },
}

/// Create `dir` and any missing ancestor directories.
///
/// A no-op (returns `Ok`) when the directory already exists. Wraps
/// [`std::fs::create_dir_all`], attaching `dir` to any failure.
///
/// # Errors
/// Returns [`FsError::CreateDir`] if the directory cannot be created — e.g. a
/// component of the path exists but is not a directory, or permissions deny it.
pub fn create_dir_all(dir: impl AsRef<Path>) -> Result<(), FsError> {
    let dir = dir.as_ref();
    std::fs::create_dir_all(dir).map_err(|source| FsError::CreateDir {
        path: dir.to_path_buf(),
        source,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "test setup and assertions")]
mod tests {
    use super::*;

    #[test]
    fn creates_nested_directories() {
        let tmp = tempfile::tempdir().unwrap();
        let nested = tmp.path().join("a/b/c");

        create_dir_all(&nested).unwrap();

        assert!(nested.is_dir(), "expected {} to be a directory", nested.display());
    }

    #[test]
    fn is_ok_when_directory_already_exists() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("here");

        create_dir_all(&dir).unwrap();
        // Second call over an existing directory must still succeed.
        create_dir_all(&dir).unwrap();

        assert!(dir.is_dir());
    }

    #[test]
    fn error_carries_the_offending_path() {
        let tmp = tempfile::tempdir().unwrap();
        // Occupy `blocker` with a *file*, then ask to create a directory
        // underneath it — the OS refuses because a path component is not a dir.
        let blocker = tmp.path().join("blocker");
        std::fs::write(&blocker, b"x").unwrap();
        let target = blocker.join("child");

        let err = create_dir_all(&target).unwrap_err();

        let FsError::CreateDir { path, .. } = err;
        assert_eq!(path, target);
    }
}
