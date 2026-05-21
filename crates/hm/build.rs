//! Build script: compiles the embedded WASM plugins shipped with `hm`
//! (`hm-plugin-docker`, `hm-plugin-output-human`, `hm-plugin-output-json`,
//! `hm-plugin-cloud`) and stages their artifacts under `$OUT_DIR` so the
//! host can `include_bytes!` them at runtime.
#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::print_stdout,
    reason = "build scripts terminate the build via panic/expect; stdout is cargo:rerun-if-changed directives"
)]

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    build_embedded_plugins();
}

fn build_wasm_plugin(crate_name: &str) {
    let underscore = crate_name.replace('-', "_");
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let dest = out_dir.join(format!("{underscore}.wasm"));

    // Bundled-wasm path: `cargo publish` extracts this crate into an
    // isolated `target/package/harmont-cli-<ver>/` and runs build.rs
    // there. The sibling plugin crates aren't reachable from that
    // sandbox (and aren't reachable for `cargo install harmont-cli`
    // end users either). Release CI pre-builds the wasms and stages
    // them under `crates/hm/embedded/` before invoking `cargo publish`,
    // and the `include = [...]` in Cargo.toml carries them into the
    // tarball. When build.rs sees a pre-built file there, just copy.
    let bundled = PathBuf::from(format!("embedded/{underscore}.wasm"));
    println!("cargo:rerun-if-changed={}", bundled.display());
    if bundled.is_file() {
        fs::copy(&bundled, &dest).unwrap_or_else(|e| {
            panic!("copy bundled {} -> {}: {e}", bundled.display(), dest.display())
        });
        return;
    }

    // Dev path: cross-compile from the sibling crate in the workspace.
    let src = format!("../{crate_name}/src");
    let cargo_toml = format!("../{crate_name}/Cargo.toml");
    println!("cargo:rerun-if-changed={src}");
    println!("cargo:rerun-if-changed={cargo_toml}");

    let status = Command::new(env::var("CARGO").as_deref().unwrap_or("cargo"))
        .args([
            "build",
            "--target",
            "wasm32-wasip1",
            "-p",
            crate_name,
            "--release",
        ])
        .current_dir("../..")
        .status()
        .unwrap_or_else(|e| panic!("invoke cargo build for {crate_name}: {e}"));
    assert!(status.success(), "{crate_name} wasm build failed");

    let src_wasm = PathBuf::from(format!(
        "../../target/wasm32-wasip1/release/{underscore}.wasm"
    ));
    fs::copy(&src_wasm, &dest)
        .unwrap_or_else(|e| panic!("copy {} -> {}: {e}", src_wasm.display(), dest.display()));
}

fn build_embedded_plugins() {
    build_wasm_plugin("hm-plugin-docker");
    build_wasm_plugin("hm-plugin-output-human");
    build_wasm_plugin("hm-plugin-output-json");
    build_wasm_plugin("hm-plugin-cloud");
}
