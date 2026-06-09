//! Host-side workspace utilities for COW build directories.

use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, ensure};

/// Create a copy-on-write clone of `src` contents into `dst`.
///
/// macOS APFS: `cp -cR` with full-copy fallback for cross-volume.
/// Linux: `cp --reflink=auto -a` (COW on btrfs/XFS, full copy on ext4).
///
/// # Errors
///
/// Returns an error if `cp` cannot be spawned or exits with a non-zero status.
pub fn cow_copy(src: &Path, dst: &Path) -> Result<()> {
    let src_dot = format!("{}/.", src.display());

    let status = if cfg!(target_os = "macos") {
        let st = Command::new("cp")
            .args(["-cpR", &src_dot])
            .arg(dst)
            .status();
        match st {
            Ok(s) if s.success() => s,
            _ => {
                // APFS clonefile failed (cross-volume or non-APFS). Wipe any
                // partial state left by the failed attempt before falling back
                // to a regular copy.
                if dst.exists() {
                    for entry in std::fs::read_dir(dst)
                        .with_context(|| format!("cleaning partial COW dst {}", dst.display()))?
                    {
                        let p = entry?.path();
                        if p.is_dir() {
                            std::fs::remove_dir_all(&p).ok();
                        } else {
                            std::fs::remove_file(&p).ok();
                        }
                    }
                }
                Command::new("cp")
                    .args(["-pR", &src_dot])
                    .arg(dst)
                    .status()
                    .with_context(|| {
                        format!(
                            "spawning cp fallback: {} -> {}",
                            src.display(),
                            dst.display()
                        )
                    })?
            }
        }
    } else {
        Command::new("cp")
            .args(["--reflink=auto", "-a", &src_dot])
            .arg(dst)
            .status()
            .with_context(|| format!("spawning cp: {} -> {}", src.display(), dst.display()))?
    };

    ensure!(
        status.success(),
        "cp {} -> {} exited with {status}",
        src.display(),
        dst.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn cow_copy_produces_independent_clone() {
        let src = tempdir().unwrap();
        fs::write(src.path().join("file.txt"), "original").unwrap();
        fs::create_dir(src.path().join("sub")).unwrap();
        fs::write(src.path().join("sub/nested.txt"), "nested").unwrap();

        let dst = tempdir().unwrap();
        cow_copy(src.path(), dst.path()).unwrap();

        assert_eq!(
            fs::read_to_string(dst.path().join("file.txt")).unwrap(),
            "original"
        );
        assert_eq!(
            fs::read_to_string(dst.path().join("sub/nested.txt")).unwrap(),
            "nested"
        );

        fs::write(dst.path().join("file.txt"), "modified").unwrap();
        assert_eq!(
            fs::read_to_string(src.path().join("file.txt")).unwrap(),
            "original"
        );
    }

    #[test]
    fn cow_copy_empty_dir() {
        let src = tempdir().unwrap();
        let dst = tempdir().unwrap();
        cow_copy(src.path(), dst.path()).unwrap();
    }
}
