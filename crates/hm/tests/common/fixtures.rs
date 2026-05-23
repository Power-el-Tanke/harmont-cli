//! Locates fixture dylib files for tests.

#![allow(dead_code)]

use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

static BUILT: OnceLock<()> = OnceLock::new();

/// Build the fixture `cdylib` crates if they haven't been built in this
/// test process yet. Idempotent across threads.
///
/// # Panics
///
/// Panics if `cargo build` cannot be invoked or returns a non-zero
/// exit. Tests can't proceed without the artifacts, so failing loudly
/// is the right behaviour.
pub fn ensure_built() {
    BUILT.get_or_init(|| {
        let packages = [
            "hm-fixture-noop-executor",
            "hm-fixture-recording-hook",
            "hm-fixture-failing-subcommand",
            "hm-fixture-host-fn-probe",
            "hm-fixture-bad-api-version",
            "hm-fixture-freestyle-runner",
        ];
        for pkg in packages {
            let status = Command::new("cargo")
                .args(["build", "-p", pkg])
                .current_dir(workspace_root())
                .status()
                .unwrap_or_else(|_| panic!("invoke cargo build for {pkg}"));
            assert!(status.success(), "{pkg} build failed");
        }
    });
}

/// Path to the compiled dylib for a fixture.
/// `name` is the crate name with hyphens, e.g. `"hm-fixture-noop-executor"`.
/// The dylib will be at `target/debug/lib<underscored>.{dylib,so,dll}`.
#[must_use]
pub fn fixture_path(name: &str) -> PathBuf {
    ensure_built();
    let underscored = name.replace('-', "_");
    let ext = std::env::consts::DLL_EXTENSION;
    let lib_name = if cfg!(target_os = "windows") {
        format!("{underscored}.{ext}")
    } else {
        format!("lib{underscored}.{ext}")
    };
    workspace_root().join("target").join("debug").join(lib_name)
}

fn workspace_root() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop(); // crates/hm -> crates
    p.pop(); // crates    -> workspace root
    p
}
