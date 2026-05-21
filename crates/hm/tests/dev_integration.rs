//! Docker-gated integration tests.
//!
//! Run with: `cargo test -p harmont-cli --features docker-integration -- --ignored`
//! Requires:
//!   * A reachable Docker daemon
//!   * harmont-py installed in the env at HARMONT_PYTHON (defaults to python3)
//!     with the `feat/hm-dev-deploy` branch checked out (or merged to main)
//!
//! Each test creates its own .harmont/ in a tmpdir to avoid step-on
//! between concurrent runs.

#![cfg(feature = "docker-integration")]

use std::path::PathBuf;
use std::process::Command;

fn write_deploys_py(dir: &std::path::Path, body: &str) {
    let h = dir.join(".harmont");
    std::fs::create_dir_all(&h).unwrap();
    std::fs::write(h.join("deploys.py"), body).unwrap();
}

fn hm_bin() -> PathBuf {
    let mut p = std::env::current_exe().unwrap();
    p.pop(); // /target/debug/deps -> /target/debug
    p.pop();
    p.push("hm");
    p
}

#[test]
#[ignore]
fn up_and_port_of_postgres() {
    let tmp = tempfile::tempdir().unwrap();
    write_deploys_py(tmp.path(), r#"
import harmont as hm

@hm.deploy("db")
def db():
    return hm.dev.deploy(
        image="postgres:16",
        port_mapping={5432: hm.dev.port()},
        env={"POSTGRES_PASSWORD": "dev"},
    )
"#);

    // Spawn `hm dev up` in the background.
    let mut up = Command::new(hm_bin())
        .args(["dev", "up"])
        .current_dir(tmp.path())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn hm dev up");

    // Wait for "all up." marker on stderr.
    let stderr = up.stderr.as_mut().unwrap();
    use std::io::Read;
    let mut buf = String::new();
    let mut chunk = [0u8; 1024];
    let started = std::time::Instant::now();
    while started.elapsed().as_secs() < 60 {
        let n = stderr.read(&mut chunk).unwrap_or(0);
        if n == 0 { break; }
        buf.push_str(&String::from_utf8_lossy(&chunk[..n]));
        if buf.contains("all up.") { break; }
    }
    assert!(buf.contains("all up."), "up did not become ready; stderr:\n{buf}");

    // Query the host port from another invocation.
    let port_of = Command::new(hm_bin())
        .args(["dev", "port-of", "db", "5432"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(port_of.status.success(), "port-of failed: {}", String::from_utf8_lossy(&port_of.stderr));
    let host_port: u16 = String::from_utf8(port_of.stdout).unwrap().trim().parse().unwrap();
    assert!(host_port > 1024, "expected ephemeral host port, got {host_port}");

    // Tear down via SIGINT.
    let _ = nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(up.id() as i32),
        nix::sys::signal::Signal::SIGINT,
    );
    let _ = up.wait();

    // After teardown, port-of should report not-running.
    let port_of_after = Command::new(hm_bin())
        .args(["dev", "port-of", "db", "5432"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert_eq!(port_of_after.status.code(), Some(4),
        "stopped slug should exit 4: {}", String::from_utf8_lossy(&port_of_after.stderr));
}
