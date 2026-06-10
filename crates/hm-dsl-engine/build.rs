// Build scripts legitimately panic on errors — no runtime to propagate to.
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::print_stderr
)]

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let ts_src = manifest_dir.join("harmont-ts/src");

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", ts_src.display());

    let esbuild = find_esbuild(&manifest_dir);

    let Some(esbuild) = esbuild else {
        panic!(
            "esbuild not found.\
             Install it or run `npm ci` in crates/hm-dsl-engine/harmont-ts/ for generating js bundles before building the crate."
        );
    };

    bundle(
        &esbuild,
        &ts_src.join("index.ts"),
        &out_dir.join("harmont-index.mjs"),
    );
    bundle(
        &esbuild,
        &ts_src.join("toolchains/index.ts"),
        &out_dir.join("harmont-toolchains.mjs"),
    );
}

fn bundle(esbuild: &Path, entry: &Path, outfile: &Path) {
    let status = Command::new(esbuild)
        .arg(entry)
        .arg("--bundle")
        .arg("--format=esm")
        .arg("--platform=node")
        .arg(format!("--outfile={}", outfile.display()))
        .status()
        .expect("failed to run esbuild");

    assert!(status.success(), "esbuild failed for {}", entry.display());
}

fn find_esbuild(manifest_dir: &Path) -> Option<PathBuf> {
    let local = manifest_dir.join("harmont-ts/node_modules/.bin/esbuild");
    if local.exists() {
        return Some(local);
    }
    let which = Command::new("which").arg("esbuild").output().ok()?;
    if which.status.success() {
        let path = String::from_utf8_lossy(&which.stdout).trim().to_string();
        if !path.is_empty() {
            return Some(PathBuf::from(path));
        }
    }
    None
}
