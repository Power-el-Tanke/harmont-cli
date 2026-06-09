#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Verify workspace freshness across a deep cache chain.
//!
//! Pipeline: A (forever) → B (forever) → C (uncached, reads marker).
//! Two runs with different marker contents: C must see the updated
//! marker even when both A and B are cache hits.
//!
//! Requires Docker. `cargo test --test deep_cache_chain_workspace -- --ignored`

use std::fs;
use std::path::Path;
use std::process::Command;

use tempfile::tempdir;

fn write_pipeline(dir: &Path, marker: &str) {
    let hm = dir.join(".hm");
    fs::create_dir_all(&hm).expect("mkdir .hm");
    fs::write(dir.join("marker.txt"), marker).expect("marker.txt");
    fs::write(
        hm.join("pipeline.py"),
        r#"
import harmont as hm

def build():
    a = hm.scratch().run("echo step-a", label="a", cache=hm.forever())
    b = a.run("echo step-b", label="b", cache=hm.forever())
    c = b.run("cat /workspace/marker.txt", label="c")
    return hm.pipeline(c, default_image="alpine:3.20")
"#,
    )
    .expect("pipeline.py");
}

fn run_hm(repo: &Path) -> String {
    let bin = env!("CARGO_BIN_EXE_hm");
    let out = Command::new(bin)
        .args(["run"])
        .current_dir(repo)
        .output()
        .expect("spawn hm");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "hm run failed.\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    stderr
}

#[test]
#[ignore = "requires Docker"]
fn deep_chain_child_sees_fresh_workspace() {
    let dir = tempdir().expect("tempdir");

    write_pipeline(dir.path(), "deep-v1");
    let out1 = run_hm(dir.path());
    assert!(out1.contains("deep-v1"), "run 1 missing marker:\n{out1}");

    fs::write(dir.path().join("marker.txt"), "deep-v2").expect("rewrite");

    let out2 = run_hm(dir.path());
    assert!(
        out2.contains("deep-v2"),
        "run 2 did not see fresh workspace through deep cache chain:\n{out2}"
    );
    assert!(
        !out2.contains("deep-v1"),
        "run 2 leaked stale workspace through deep cache chain:\n{out2}"
    );
}
