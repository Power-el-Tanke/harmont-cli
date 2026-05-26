//! Copy-on-write directory cloning.
//!
//! The main entry point is [`cow_clone_dir`], which creates a copy-on-write clone
//! of a directory tree using the most efficient method available on the
//! current platform.

use std::path::Path;

use anyhow::{Context, Result};

/// Create a copy-on-write clone of `src` at `dst`.
///
/// On macOS (APFS), uses `cp -c` for instant O(1) clones.
/// On Linux, uses `cp --reflink=auto` (instant on btrfs/xfs, falls back
/// to regular copy on ext4).
/// Falls back to a recursive copy if platform-specific methods fail.
///
/// # Errors
///
/// Returns an error if the clone cannot be created (e.g. `src` does not
/// exist or `dst` already exists and the copy fails).
pub fn cow_clone_dir(src: &Path, dst: &Path) -> Result<()> {
    cow_clone_platform(src, dst)
}

#[cfg(target_os = "macos")]
fn cow_clone_platform(src: &Path, dst: &Path) -> Result<()> {
    let status = std::process::Command::new("cp")
        .args(["-cR", "--"])
        .arg(src)
        .arg(dst)
        .status()
        .context("failed to spawn cp -cR")?;
    if status.success() {
        return Ok(());
    }
    fallback_copy(src, dst)
}

#[cfg(target_os = "linux")]
fn cow_clone_platform(src: &Path, dst: &Path) -> Result<()> {
    let status = std::process::Command::new("cp")
        .args(["--reflink=auto", "-a", "--"])
        .arg(src)
        .arg(dst)
        .status()
        .context("failed to spawn cp --reflink=auto")?;
    if status.success() {
        return Ok(());
    }
    fallback_copy(src, dst)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn cow_clone_platform(src: &Path, dst: &Path) -> Result<()> {
    fallback_copy(src, dst)
}

/// Overlay source files from `src` onto an existing `dst` directory.
///
/// Copies all files from `src` into `dst`, overwriting any that already
/// exist. Used to layer fresh source on top of extracted cached artifacts
/// so local edits always win over stale cached source.
///
/// # Errors
///
/// Returns an error if the overlay operation fails.
pub fn overlay_source(src: &Path, dst: &Path) -> Result<()> {
    // rsync -a with trailing slash merges contents into dst.
    let status = std::process::Command::new("rsync")
        .args(["-a"])
        .arg(format!("{}/", src.display()))
        .arg(format!("{}/", dst.display()))
        .status();

    match status {
        Ok(s) if s.success() => return Ok(()),
        _ => {}
    }

    // Fallback: cp -R src/. dst (POSIX "copy directory contents")
    #[cfg(target_os = "macos")]
    {
        let src_dot = src.join(".");
        let status = std::process::Command::new("cp")
            .args(["-R", "--"])
            .arg(&src_dot)
            .arg(dst)
            .status()
            .context("overlay source via cp")?;
        anyhow::ensure!(status.success(), "cp overlay exited with {status}");
    }
    #[cfg(not(target_os = "macos"))]
    {
        let status = std::process::Command::new("cp")
            .args(["-R", "-T", "--"])
            .arg(src)
            .arg(dst)
            .status()
            .context("overlay source via cp")?;
        anyhow::ensure!(status.success(), "cp overlay exited with {status}");
    }
    Ok(())
}

fn fallback_copy(src: &Path, dst: &Path) -> Result<()> {
    let status = std::process::Command::new("cp")
        .args(["-R", "--"])
        .arg(src)
        .arg(dst)
        .status()
        .context("failed to spawn cp -R")?;
    anyhow::ensure!(status.success(), "cp -R exited with {status}");
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn clone_creates_independent_copy() {
        let src = TempDir::new().unwrap();
        let dst = TempDir::new().unwrap();
        let dst_path = dst.path().join("clone");

        fs::write(src.path().join("hello.txt"), "world").unwrap();
        fs::create_dir(src.path().join("sub")).unwrap();
        fs::write(src.path().join("sub/nested.txt"), "deep").unwrap();

        cow_clone_dir(src.path(), &dst_path).unwrap();

        assert_eq!(
            fs::read_to_string(dst_path.join("hello.txt")).unwrap(),
            "world"
        );
        assert_eq!(
            fs::read_to_string(dst_path.join("sub/nested.txt")).unwrap(),
            "deep"
        );

        // Writes to clone don't affect source
        fs::write(dst_path.join("hello.txt"), "modified").unwrap();
        assert_eq!(
            fs::read_to_string(src.path().join("hello.txt")).unwrap(),
            "world"
        );
    }
}
