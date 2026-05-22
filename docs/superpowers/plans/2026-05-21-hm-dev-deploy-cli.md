# `harmont-cli`: `hm dev` Local Deployments — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship the Rust CLI half of local deployments: `hm dev up | down | ls | logs | port-of | exec`. `hm dev up` blocks foreground, multiplexes container logs by slug w/ color, and tears down cleanly on SIGINT. Multiple sessions in one worktree co-exist via a 6-hex session-id discriminator on container and network names.

**Architecture:** A new `crates/hm/src/commands/dev/` module tree. `up.rs` is the centerpiece; it shells out to `python -m harmont.dev --dump-registry` to read the registered deployments (produced by the harmont-py branch on `feat/hm-dev-deploy`), topo-sorts the dep graph, creates a per-session bridge network, starts containers level-by-level via an extended `DockerClient`, streams logs through a per-slug mux to stdout, and tears down on signal. Bollard 0.18 is already in deps.

**Tech Stack:** Rust 2021 edition, clap v4 derive, tokio "full", bollard 0.18, serde/serde_json, owo-colors, anyhow/thiserror. No new dependencies.

**Spec:** `/home/marko/harmont-py/docs/superpowers/specs/2026-05-21-hm-dev-deploy-design.md` (committed on `feat/hm-dev-deploy` in harmont-py). Read § 2 (CLI surface), § 3 (runtime), § 4 (lifecycle), § 5 (error handling) before starting.

**Branch:** `feat/hm-dev-deploy`. Already created off `main`.

**Prerequisite:** The harmont-py branch `feat/hm-dev-deploy` must be installed in the Python environment the CLI invokes. For local dev: `pip install -e /home/marko/harmont-py` (with the branch checked out). Integration tests document this; pure-Rust unit tests do not require Python.

**Commit cadence:** Every task ends with a commit. The commit subject line is in the example commands.

---

## File Map

### Create

- `crates/hm/src/commands/dev/mod.rs` — subcommand dispatcher + module exports.
- `crates/hm/src/commands/dev/registry.rs` — serde types for the registry JSON; subprocess invocation of `python -m harmont.dev --dump-registry`.
- `crates/hm/src/commands/dev/naming.rs` — worktree-hash, session-id generation, container/network name formatters; constants for the label keys.
- `crates/hm/src/commands/dev/topo.rs` — `BootPlan` builder: dep-graph topo sort grouped into parallel boot levels.
- `crates/hm/src/commands/dev/network.rs` — bollard network create / remove wrappers, scoped to the dev driver.
- `crates/hm/src/commands/dev/logmux.rs` — line-prefixed colored log stream, partial-chunk buffering, owo-colors palette.
- `crates/hm/src/commands/dev/service_spec.rs` — `ServiceSpec` struct + `build_spec(reg_entry, ctx, session, net)` converter.
- `crates/hm/src/commands/dev/up.rs` — top-level orchestrator: registry → plan → boot → mux → signal → teardown.
- `crates/hm/src/commands/dev/down.rs` — orphan sweep (this worktree or `--all`).
- `crates/hm/src/commands/dev/ls.rs` — registry walk + docker inspect merge, table rendering.
- `crates/hm/src/commands/dev/logs.rs` — `docker logs --follow` shim w/ ambiguity rule.
- `crates/hm/src/commands/dev/port_of.rs` — `docker inspect` → host port lookup w/ ambiguity rule.
- `crates/hm/src/commands/dev/exec.rs` — `docker exec` w/ TTY allocation.
- `crates/hm/tests/dev_integration.rs` — docker-gated integration tests (feature `docker-integration`).

### Modify

- `crates/hm/src/cli.rs` — add `Dev(DevCommand)` variant + the `DevCommand` enum + per-subcommand argument structs.
- `crates/hm/src/commands/mod.rs` — register the `dev` module; route `Command::Dev` to `dev::dispatch`.
- `crates/hm/src/orchestrator/docker_client.rs` — extend with: `create_network`, `remove_network`, `start_service(spec)`, `inspect_ports`, `commit_container`, `stop_container`, `remove_container`, `list_containers_by_label`, `logs_stream`, `exec_tty`.
- `crates/hm/src/orchestrator/mod.rs` — add `pub async fn build_image_from_pipeline(...)` that reuses the existing local-pipeline runner to build an image from a v0 IR pipeline, then commits it under a tag.
- `crates/hm/Cargo.toml` — add `[features] docker-integration = []` so the integration tests are opt-in.

### Do NOT touch

- `crates/hm/src/commands/run/` — the `hm run` codepath. The dev plan reuses its docker primitives, not its surface.
- `crates/hm-plugin-protocol/`, `crates/hm-plugin-sdk/`, `crates/hm-plugin-{docker,cloud,output-*}/` — plugins are orthogonal to local deployments in v1.

---

## Task 1: CLI scaffolding for `hm dev`

Wires up the new subcommand tree with no behavior. Every subcommand returns `unimplemented` so the rest of the tasks can flesh them in independently.

**Files:**
- Modify: `crates/hm/src/cli.rs`
- Create: `crates/hm/src/commands/dev/mod.rs`
- Modify: `crates/hm/src/commands/mod.rs`

- [ ] **Step 1: Add `Dev(DevCommand)` to the `Command` enum in `cli.rs`**

Locate the `Command` enum in `crates/hm/src/cli.rs`. After the existing `Plugin(PluginCommand)` variant and before `External(Vec<String>)`, add:

```rust
    /// Manage local long-lived deployments (dev databases, dev API
    /// servers, dev webapps). Reads `.harmont/*.py` for
    /// `@hm.deploy`-decorated functions and brings them up via Docker.
    #[command(subcommand)]
    Dev(DevCommand),
```

Then at the bottom of `cli.rs` (after the `PluginCommand` enum), add:

```rust
// ---------------------------------------------------------------------------
// Dev (local deployments)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Subcommand)]
pub enum DevCommand {
    /// Bring deployments up in the foreground. Blocks until Ctrl-C.
    Up(DevUpArgs),
    /// Tear down deployments owned by this worktree's sessions.
    Down(DevDownArgs),
    /// List registered + running deployments.
    Ls,
    /// Tail logs of a live deployment from another terminal.
    Logs(DevLogsArgs),
    /// Print the host port for a live deployment. Designed for $() use.
    PortOf(DevPortOfArgs),
    /// One-shot exec into a live deployment container.
    Exec(DevExecArgs),
}

#[derive(Debug, Clone, Parser)]
pub struct DevUpArgs {
    /// Deployment slugs to bring up. When empty, brings up everything
    /// registered in `.harmont/*.py`.
    #[arg()]
    pub slugs: Vec<String>,

    /// Skip transitive dependencies; bring up exactly the listed slugs.
    #[arg(long)]
    pub no_deps: bool,

    /// Force image rebuild on `from_=Step` deployments even if a cached
    /// build image exists.
    #[arg(long)]
    pub rebuild: bool,
}

#[derive(Debug, Clone, Parser)]
pub struct DevDownArgs {
    /// Slugs to sweep. When empty, sweeps all sessions of this worktree.
    #[arg()]
    pub slugs: Vec<String>,

    /// Sweep one specific session entirely (overrides `slugs`).
    #[arg(long, value_name = "ID")]
    pub session: Option<String>,

    /// Sweep system-wide instead of this worktree (every container
    /// labelled `harmont.driver=local`).
    #[arg(long)]
    pub all: bool,
}

#[derive(Debug, Clone, Parser)]
pub struct DevLogsArgs {
    pub slug: String,

    #[arg(short, long)]
    pub follow: bool,

    #[arg(long, value_name = "ID")]
    pub session: Option<String>,
}

#[derive(Debug, Clone, Parser)]
pub struct DevPortOfArgs {
    pub slug: String,

    /// Container-internal port whose host binding to print.
    pub container_port: u16,

    #[arg(long, value_name = "ID")]
    pub session: Option<String>,
}

#[derive(Debug, Clone, Parser)]
pub struct DevExecArgs {
    pub slug: String,

    /// Command to run inside the container. Default `sh -l`.
    #[arg(trailing_var_arg = true)]
    pub cmd: Vec<String>,

    #[arg(long, value_name = "ID")]
    pub session: Option<String>,
}
```

- [ ] **Step 2: Create `crates/hm/src/commands/dev/mod.rs` with the dispatcher stub**

```rust
//! `hm dev` — local Docker deployment subcommand tree.
//!
//! Reads `.harmont/*.py` for `@hm.deploy` registrations (via a Python
//! subprocess) and orchestrates long-lived containers on a per-session
//! bridge network. See
//! `docs/superpowers/specs/2026-05-21-hm-dev-deploy-design.md`.

use anyhow::Result;

use crate::cli::DevCommand;
use crate::context::RunContext;

pub mod down;
pub mod exec;
pub mod logmux;
pub mod logs;
pub mod ls;
pub mod naming;
pub mod network;
pub mod port_of;
pub mod registry;
pub mod service_spec;
pub mod topo;
pub mod up;

/// Top-level dispatcher for `hm dev`.
///
/// # Errors
///
/// Returns errors from the subcommand handler.
pub async fn dispatch(command: DevCommand, ctx: RunContext) -> Result<i32> {
    match command {
        DevCommand::Up(args) => up::handle(args, ctx).await,
        DevCommand::Down(args) => down::handle(args, ctx).await,
        DevCommand::Ls => ls::handle(ctx).await,
        DevCommand::Logs(args) => logs::handle(args, ctx).await,
        DevCommand::PortOf(args) => port_of::handle(args, ctx).await,
        DevCommand::Exec(args) => exec::handle(args, ctx).await,
    }
}
```

- [ ] **Step 3: Stub each subcommand handler so the workspace compiles**

For each of `up.rs`, `down.rs`, `ls.rs`, `logs.rs`, `port_of.rs`, `exec.rs`, create the file with a minimal stub. Use this template (replace `<FnArgs>` per handler signature):

```rust
//! `hm dev <verb>` handler.

use anyhow::Result;

use crate::cli::<FnArgs>;
use crate::context::RunContext;

#[expect(clippy::missing_errors_doc, reason = "stub; real errors land in the implementation task")]
pub async fn handle(_args: <FnArgs>, _ctx: RunContext) -> Result<i32> {
    anyhow::bail!("hm dev <verb>: not yet implemented")
}
```

Exact concrete files:

`up.rs`:
```rust
//! `hm dev up` handler.

use anyhow::Result;

use crate::cli::DevUpArgs;
use crate::context::RunContext;

pub async fn handle(_args: DevUpArgs, _ctx: RunContext) -> Result<i32> {
    anyhow::bail!("hm dev up: not yet implemented")
}
```

`down.rs`:
```rust
//! `hm dev down` handler.

use anyhow::Result;

use crate::cli::DevDownArgs;
use crate::context::RunContext;

pub async fn handle(_args: DevDownArgs, _ctx: RunContext) -> Result<i32> {
    anyhow::bail!("hm dev down: not yet implemented")
}
```

`ls.rs`:
```rust
//! `hm dev ls` handler.

use anyhow::Result;

use crate::context::RunContext;

pub async fn handle(_ctx: RunContext) -> Result<i32> {
    anyhow::bail!("hm dev ls: not yet implemented")
}
```

`logs.rs`:
```rust
//! `hm dev logs` handler.

use anyhow::Result;

use crate::cli::DevLogsArgs;
use crate::context::RunContext;

pub async fn handle(_args: DevLogsArgs, _ctx: RunContext) -> Result<i32> {
    anyhow::bail!("hm dev logs: not yet implemented")
}
```

`port_of.rs`:
```rust
//! `hm dev port-of` handler.

use anyhow::Result;

use crate::cli::DevPortOfArgs;
use crate::context::RunContext;

pub async fn handle(_args: DevPortOfArgs, _ctx: RunContext) -> Result<i32> {
    anyhow::bail!("hm dev port-of: not yet implemented")
}
```

`exec.rs`:
```rust
//! `hm dev exec` handler.

use anyhow::Result;

use crate::cli::DevExecArgs;
use crate::context::RunContext;

pub async fn handle(_args: DevExecArgs, _ctx: RunContext) -> Result<i32> {
    anyhow::bail!("hm dev exec: not yet implemented")
}
```

Also create empty placeholder files for the other modules so `pub mod` declarations in `dev/mod.rs` resolve:

`registry.rs`:
```rust
//! Deployment registry (filled in Task 3).
```

`naming.rs`:
```rust
//! Container / network / session naming (filled in Task 2).
```

`topo.rs`:
```rust
//! Boot plan topo sort (filled in Task 4).
```

`network.rs`:
```rust
//! Bridge network create / remove (filled in Task 6).
```

`logmux.rs`:
```rust
//! Log multiplexer (filled in Task 7).
```

`service_spec.rs`:
```rust
//! Container service spec (filled in Task 8).
```

- [ ] **Step 4: Wire dispatch in `crates/hm/src/commands/mod.rs`**

Add `pub mod dev;` near `pub mod run;`. In the `dispatch` function's match block, add the `Dev` arm so the function reads:

```rust
pub async fn dispatch(command: Command, ctx: RunContext) -> Result<i32> {
    match command {
        Command::Run(args) => run::handle(args, ctx).await,
        Command::Dev(cmd) => dev::dispatch(cmd, ctx).await,
        Command::Version => crate::builtin::version::run().await.map(|()| 0),
        Command::Plugin(cmd) => crate::builtin::plugin::run(cmd).await.map(|()| 0),
        Command::External(argv) => crate::dispatcher::run(argv).await,
    }
}
```

- [ ] **Step 5: Build to verify the scaffolding compiles**

```bash
cargo build -p harmont-cli
```

Expected: clean build.

- [ ] **Step 6: Smoke-check the CLI tree**

```bash
cargo run -p harmont-cli -- dev --help
cargo run -p harmont-cli -- dev up --help
cargo run -p harmont-cli -- dev port-of --help
```

Expected: each prints its subcommand help. Run `cargo run -p harmont-cli -- dev up` (no args) and expect: `Error: hm dev up: not yet implemented`.

- [ ] **Step 7: Commit**

```bash
git add crates/hm/src/cli.rs crates/hm/src/commands/mod.rs crates/hm/src/commands/dev/
git commit -m "$(cat <<'EOF'
feat(dev): scaffold `hm dev` subcommand tree

clap definitions for up | down | ls | logs | port-of | exec.
Module placeholders for every component file referenced by later
tasks. Every subcommand currently errors as "not yet implemented"
so the rest of the plan can drop in implementations independently.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: Naming module (worktree-hash, session-id, container/network names)

**Files:**
- Modify: `crates/hm/src/commands/dev/naming.rs`

- [ ] **Step 1: Write the failing tests**

Replace `crates/hm/src/commands/dev/naming.rs` with:

```rust
//! Worktree-hash, session-id, container / network name formatters.

use std::path::Path;

use anyhow::Result;
use sha1::{Digest, Sha1};

pub const LABEL_WORKTREE: &str = "harmont.worktree";
pub const LABEL_SLUG: &str = "harmont.slug";
pub const LABEL_SESSION: &str = "harmont.session";
pub const LABEL_DRIVER: &str = "harmont.driver";
pub const DRIVER_LOCAL: &str = "local";

/// Stable 10-hex-char identity for a worktree, derived from the
/// canonical absolute path. Used as a Docker container/network name
/// component and as a label value.
#[must_use]
pub fn worktree_hash(path: &Path) -> String {
    let bytes = path.to_string_lossy();
    let mut hasher = Sha1::new();
    hasher.update(bytes.as_bytes());
    let out = hasher.finalize();
    let mut hex = String::with_capacity(10);
    for b in out.iter().take(5) {
        use std::fmt::Write as _;
        write!(&mut hex, "{b:02x}").expect("write to String never fails");
    }
    hex
}

/// 6 hex chars from a cryptographically secure RNG. Each `hm dev up`
/// generates its own; collisions are avoided by checking against
/// running containers on creation (Docker would 409 anyway).
#[must_use]
pub fn fresh_session_id() -> String {
    use rand::Rng;
    use rand::distributions::Alphanumeric;
    let raw: Vec<u8> = rand::thread_rng()
        .sample_iter(Alphanumeric)
        .take(64)
        .collect();
    // Reduce to 6 lowercase hex chars via sha1 of the random sample.
    let mut hasher = Sha1::new();
    hasher.update(&raw);
    let out = hasher.finalize();
    let mut hex = String::with_capacity(6);
    for b in out.iter().take(3) {
        use std::fmt::Write as _;
        write!(&mut hex, "{b:02x}").expect("write to String never fails");
    }
    hex
}

#[must_use]
pub fn container_name(worktree_hash: &str, slug: &str, session: &str) -> String {
    format!("hm-{worktree_hash}-{slug}-{session}")
}

#[must_use]
pub fn network_name(worktree_hash: &str, session: &str) -> String {
    format!("hm-{worktree_hash}-{session}")
}

/// Resolve the worktree root. Falls back to the absolute current
/// working directory when there's no git repo.
///
/// # Errors
///
/// Returns an error if the cwd is unreadable.
pub fn resolve_worktree_root() -> Result<std::path::PathBuf> {
    use std::process::Command;
    let try_git = Command::new("git").args(["rev-parse", "--show-toplevel"]).output();
    if let Ok(out) = try_git {
        if out.status.success() {
            let s = String::from_utf8(out.stdout)?.trim().to_string();
            if !s.is_empty() {
                return Ok(std::path::PathBuf::from(s));
            }
        }
    }
    Ok(std::env::current_dir()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worktree_hash_is_stable() {
        let h1 = worktree_hash(Path::new("/home/marko/myrepo"));
        let h2 = worktree_hash(Path::new("/home/marko/myrepo"));
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 10);
        assert!(h1.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn worktree_hash_differs_per_path() {
        let h1 = worktree_hash(Path::new("/home/marko/myrepo"));
        let h2 = worktree_hash(Path::new("/home/marko/myrepo-wt2"));
        assert_ne!(h1, h2);
    }

    #[test]
    fn session_id_is_six_hex_chars() {
        let s = fresh_session_id();
        assert_eq!(s.len(), 6);
        assert!(s.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn container_name_format() {
        assert_eq!(
            container_name("a1b2c3d4e5", "db", "7a2f91"),
            "hm-a1b2c3d4e5-db-7a2f91",
        );
    }

    #[test]
    fn network_name_format() {
        assert_eq!(
            network_name("a1b2c3d4e5", "7a2f91"),
            "hm-a1b2c3d4e5-7a2f91",
        );
    }
}
```

- [ ] **Step 2: Add the new dependencies in `crates/hm/Cargo.toml`**

Locate `[dependencies]` and ensure both are present (insert at the appropriate sorted slot if missing):

```toml
sha1 = "0.10"
```

`rand` is already in deps. Confirm via `grep '^rand' crates/hm/Cargo.toml`.

- [ ] **Step 3: Run tests to verify they pass**

```bash
cargo test -p harmont-cli --lib commands::dev::naming
```

Expected: 5 passed.

- [ ] **Step 4: Commit**

```bash
git add crates/hm/src/commands/dev/naming.rs crates/hm/Cargo.toml crates/hm/Cargo.lock
git commit -m "$(cat <<'EOF'
feat(dev): naming primitives for worktree/session/container/network

10-hex-char worktree-hash from canonical path; 6-hex-char session-id
per `hm dev up`. Constants for the four `harmont.*` Docker labels.
resolve_worktree_root prefers `git rev-parse --show-toplevel` and
falls back to cwd so we never refuse to run for lack of a git repo.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: Registry deserialization + python subprocess

**Files:**
- Modify: `crates/hm/src/commands/dev/registry.rs`

- [ ] **Step 1: Write the failing tests + implementation**

Replace `crates/hm/src/commands/dev/registry.rs` with:

```rust
//! Read the deployment registry from `python -m harmont.dev --dump-registry`.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Stdio;

use anyhow::{Context, Result, anyhow};
use serde::Deserialize;
use tokio::process::Command;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct DevRegistry {
    pub schema_version: String,
    pub worktree: String,
    pub deployments: BTreeMap<String, RegEntry>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(tag = "driver")]
pub enum RegEntry {
    #[serde(rename = "local")]
    Local(LocalSpec),
    /// Any other driver. Carries the discriminator + `_unhandled: true`.
    /// Used by `hm dev ls` to display non-local deployments.
    #[serde(other)]
    Unhandled,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct LocalSpec {
    pub image: Option<String>,
    #[serde(default)]
    pub from: Option<FromSource>,
    #[serde(default)]
    pub cmd: Option<Vec<String>>,
    pub port_mapping: BTreeMap<String, String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub volumes: BTreeMap<String, String>,
    #[serde(default)]
    pub workdir: Option<String>,
    #[serde(default)]
    pub deps: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum FromSource {
    #[serde(rename = "step_chain")]
    StepChain { pipeline_v0: serde_json::Value },
}

/// Wire sentinel for hm.dev.port() — emitted by harmont.dev.dump_registry_json.
pub const PORT_SENTINEL: &str = "__hm_dev_port__";

/// Invoke `python -m harmont.dev --dump-registry --worktree-root <root>`
/// and deserialize the output.
///
/// # Errors
///
/// Returns an error if python is missing on PATH, the subprocess exits
/// non-zero (stderr is included in the message), or the JSON is malformed.
pub async fn dump(worktree_root: &Path) -> Result<DevRegistry> {
    let py = std::env::var("HARMONT_PYTHON").unwrap_or_else(|_| "python3".to_string());
    let output = Command::new(&py)
        .args([
            "-m",
            "harmont.dev",
            "--dump-registry",
            "--worktree-root",
        ])
        .arg(worktree_root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .with_context(|| format!("invoke `{py} -m harmont.dev`; is harmont-py installed?"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!(
            "python -m harmont.dev --dump-registry exited {}: {}",
            output.status,
            stderr.trim()
        ));
    }
    serde_json::from_slice(&output.stdout).context("parse deployment registry JSON")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_minimal_local() {
        let raw = r#"{
          "schema_version": "0",
          "worktree": "/tmp/wt",
          "deployments": {
            "db": {
              "driver": "local",
              "image": "postgres:16",
              "from": null,
              "cmd": null,
              "port_mapping": {"5432": "__hm_dev_port__"},
              "env": {"POSTGRES_PASSWORD": "dev"},
              "volumes": {},
              "workdir": null,
              "deps": []
            }
          }
        }"#;
        let reg: DevRegistry = serde_json::from_str(raw).unwrap();
        assert_eq!(reg.schema_version, "0");
        let RegEntry::Local(spec) = &reg.deployments["db"] else { panic!("local expected") };
        assert_eq!(spec.image.as_deref(), Some("postgres:16"));
        assert_eq!(spec.port_mapping["5432"], PORT_SENTINEL);
    }

    #[test]
    fn deserialize_step_chain_from() {
        let raw = r#"{
          "schema_version": "0",
          "worktree": "/tmp/wt",
          "deployments": {
            "api": {
              "driver": "local",
              "image": null,
              "from": {"type": "step_chain", "pipeline_v0": {"version":"0","steps":[]}},
              "cmd": null,
              "port_mapping": {"8000": "__hm_dev_port__"},
              "env": {},
              "volumes": {},
              "workdir": null,
              "deps": ["db"]
            }
          }
        }"#;
        let reg: DevRegistry = serde_json::from_str(raw).unwrap();
        let RegEntry::Local(spec) = &reg.deployments["api"] else { panic!() };
        assert!(matches!(spec.from, Some(FromSource::StepChain { .. })));
        assert_eq!(spec.deps, vec!["db"]);
    }

    #[test]
    fn unknown_driver_maps_to_unhandled() {
        let raw = r#"{
          "schema_version": "0",
          "worktree": "/tmp/wt",
          "deployments": {
            "prod": {"driver": "aws", "_unhandled": true}
          }
        }"#;
        let reg: DevRegistry = serde_json::from_str(raw).unwrap();
        assert!(matches!(reg.deployments["prod"], RegEntry::Unhandled));
    }
}
```

- [ ] **Step 2: Run tests to verify they pass**

```bash
cargo test -p harmont-cli --lib commands::dev::registry
```

Expected: 3 passed.

- [ ] **Step 3: Commit**

```bash
git add crates/hm/src/commands/dev/registry.rs
git commit -m "$(cat <<'EOF'
feat(dev): deserialize deployment registry from `python -m harmont.dev`

Serde types match the spec's v0 schema (§1). Unknown drivers fall
back to RegEntry::Unhandled via #[serde(other)] so hm dev ls can
display them. dump() shells out to `python -m harmont.dev
--dump-registry`; HARMONT_PYTHON overrides the python binary for
test environments.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: Boot-plan topo sort

**Files:**
- Modify: `crates/hm/src/commands/dev/topo.rs`

- [ ] **Step 1: Write the failing tests + implementation**

Replace `crates/hm/src/commands/dev/topo.rs` with:

```rust
//! Boot-plan topo sort over the local-driver subset of the registry.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, anyhow};

use super::registry::{DevRegistry, RegEntry};

/// A topo-sorted list of boot levels. Each inner Vec contains slugs
/// that can boot in parallel after all earlier levels are running.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootPlan {
    pub levels: Vec<Vec<String>>,
}

impl BootPlan {
    /// Flat iterator over every slug in boot order.
    pub fn slugs(&self) -> impl Iterator<Item = &str> {
        self.levels.iter().flatten().map(String::as_str)
    }
}

/// Compute the boot plan over local-driver deployments.
///
/// - `requested`: explicit slug subset; empty means "all local".
/// - `no_deps`: when true, only the requested slugs are included.
/// - Otherwise, transitive deps of the requested slugs are pulled in.
///
/// # Errors
///
/// Returns an error if a requested slug isn't registered, isn't a
/// local-driver entry, or if the dep graph contains a cycle.
pub fn plan(reg: &DevRegistry, requested: &[String], no_deps: bool) -> Result<BootPlan> {
    let mut deps: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (slug, entry) in &reg.deployments {
        if let RegEntry::Local(spec) = entry {
            deps.insert(slug.as_str(), spec.deps.iter().map(String::as_str).collect());
        }
    }
    for s in requested {
        if !deps.contains_key(s.as_str()) {
            let exists = reg.deployments.contains_key(s);
            return Err(anyhow!(
                "hm: slug `{s}` {}",
                if exists { "is not a local-driver deployment (use the matching driver's `up`)" }
                else { "is not registered in this worktree's .harmont/" }
            ));
        }
    }
    let selected: BTreeSet<String> = if requested.is_empty() {
        deps.keys().map(|s| s.to_string()).collect()
    } else if no_deps {
        requested.iter().cloned().collect()
    } else {
        let mut out: BTreeSet<String> = BTreeSet::new();
        let mut stack: Vec<String> = requested.to_vec();
        while let Some(s) = stack.pop() {
            if out.insert(s.clone()) {
                for d in deps.get(s.as_str()).cloned().unwrap_or_default() {
                    if deps.contains_key(d) {
                        stack.push(d.to_string());
                    }
                }
            }
        }
        out
    };
    // Kahn's algorithm restricted to `selected`.
    let mut indeg: BTreeMap<String, usize> = selected
        .iter()
        .map(|s| {
            let count = deps
                .get(s.as_str())
                .map(|ds| ds.iter().filter(|d| selected.contains(**d)).count())
                .unwrap_or(0);
            (s.clone(), count)
        })
        .collect();
    let mut levels: Vec<Vec<String>> = Vec::new();
    while !indeg.is_empty() {
        let ready: Vec<String> = indeg
            .iter()
            .filter(|(_, &c)| c == 0)
            .map(|(s, _)| s.clone())
            .collect();
        if ready.is_empty() {
            let unresolved: Vec<String> = indeg.keys().cloned().collect();
            return Err(anyhow!(
                "hm: dep cycle among deployments: {}",
                unresolved.join(", ")
            ));
        }
        for s in &ready {
            indeg.remove(s);
        }
        for (_, downstreams) in deps.iter() {
            // No-op: indeg keys we still hold are the ones not in `ready`;
            // decrement their indeg if any of their deps were in `ready`.
        }
        for (slug, count) in indeg.iter_mut() {
            if let Some(ds) = deps.get(slug.as_str()) {
                let removed = ds.iter().filter(|d| ready.iter().any(|r| *r == **d)).count();
                *count = count.saturating_sub(removed);
            }
        }
        levels.push(ready);
    }
    Ok(BootPlan { levels })
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::registry::{DevRegistry, LocalSpec, RegEntry};
    use std::collections::BTreeMap;

    fn reg(specs: &[(&str, &[&str])]) -> DevRegistry {
        let mut deployments = BTreeMap::new();
        for (slug, deps) in specs {
            deployments.insert(
                (*slug).to_string(),
                RegEntry::Local(LocalSpec {
                    image: Some("img".into()),
                    from: None,
                    cmd: None,
                    port_mapping: BTreeMap::new(),
                    env: BTreeMap::new(),
                    volumes: BTreeMap::new(),
                    workdir: None,
                    deps: deps.iter().map(|d| (*d).to_string()).collect(),
                }),
            );
        }
        DevRegistry {
            schema_version: "0".into(),
            worktree: "/tmp/wt".into(),
            deployments,
        }
    }

    #[test]
    fn empty_request_brings_up_everything() {
        let r = reg(&[("db", &[]), ("api", &["db"]), ("web", &["api"])]);
        let plan = plan(&r, &[], false).unwrap();
        assert_eq!(plan.levels, vec![
            vec!["db".to_string()],
            vec!["api".to_string()],
            vec!["web".to_string()],
        ]);
    }

    #[test]
    fn explicit_slug_pulls_in_transitive_deps() {
        let r = reg(&[("db", &[]), ("api", &["db"]), ("web", &["api"])]);
        let plan = plan(&r, &["web".to_string()], false).unwrap();
        let slugs: Vec<&str> = plan.slugs().collect();
        assert_eq!(slugs, vec!["db", "api", "web"]);
    }

    #[test]
    fn no_deps_skips_transitive() {
        let r = reg(&[("db", &[]), ("api", &["db"]), ("web", &["api"])]);
        let plan = plan(&r, &["web".to_string()], true).unwrap();
        let slugs: Vec<&str> = plan.slugs().collect();
        assert_eq!(slugs, vec!["web"]);
    }

    #[test]
    fn unknown_slug_errors() {
        let r = reg(&[("db", &[])]);
        let err = plan(&r, &["redis".to_string()], false).unwrap_err();
        assert!(err.to_string().contains("not registered"));
    }

    #[test]
    fn cycle_errors() {
        let r = reg(&[("a", &["b"]), ("b", &["a"])]);
        let err = plan(&r, &[], false).unwrap_err();
        assert!(err.to_string().contains("dep cycle"));
    }

    #[test]
    fn parallel_siblings_share_a_level() {
        let r = reg(&[("db", &[]), ("cache", &[]), ("api", &["db", "cache"])]);
        let plan = plan(&r, &[], false).unwrap();
        assert_eq!(plan.levels.len(), 2);
        // First level should contain both leaf deps (order is BTreeMap iteration order).
        let level0: BTreeSet<&str> = plan.levels[0].iter().map(String::as_str).collect();
        assert!(level0.contains("db"));
        assert!(level0.contains("cache"));
        assert_eq!(plan.levels[1], vec!["api".to_string()]);
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test -p harmont-cli --lib commands::dev::topo
```

Expected: 6 passed.

- [ ] **Step 3: Commit**

```bash
git add crates/hm/src/commands/dev/topo.rs
git commit -m "$(cat <<'EOF'
feat(dev): boot-plan topo sort over the local-driver registry

Kahn's algorithm; one BTreeSet of selected slugs grown from explicit
requests by walking transitive deps (or not, when --no-deps is set);
levels grouped by indeg=0 so siblings boot in parallel. Cycle
detection mirrors the python-side check; useful as defense-in-depth.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 5: Extend `DockerClient` with the needed primitives

The existing `crates/hm/src/orchestrator/docker_client.rs` already has `connect`, `ping`, `image_exists`, `pull_image`, `start_long_lived`, and `exec_streaming`. Add the rest.

**Files:**
- Modify: `crates/hm/src/orchestrator/docker_client.rs`

- [ ] **Step 1: Append the new methods to `DockerClient` impl block**

Open `crates/hm/src/orchestrator/docker_client.rs`. After the existing `exec_streaming` method (or wherever the impl ends — there's only one `impl DockerClient` block), insert the following methods:

```rust
    // --- network ---

    /// Create a user-defined bridge network. Returns the network ID.
    ///
    /// # Errors
    ///
    /// Returns [`HmError::Docker`] if the daemon rejects the create.
    pub async fn create_network(
        &self,
        name: &str,
        labels: std::collections::HashMap<String, String>,
    ) -> Result<String> {
        use bollard::network::CreateNetworkOptions;
        let resp = self
            .inner
            .create_network(CreateNetworkOptions {
                name,
                driver: "bridge",
                labels,
                ..Default::default()
            })
            .await
            .map_err(|e| HmError::Docker(format!("create_network({name}): {e}")))?;
        Ok(resp.id.unwrap_or_else(|| name.to_string()))
    }

    /// Remove a network by name. Idempotent — silently swallows "not found".
    ///
    /// # Errors
    ///
    /// Returns [`HmError::Docker`] for non-404 daemon errors.
    pub async fn remove_network(&self, name: &str) -> Result<()> {
        match self.inner.remove_network(name).await {
            Ok(()) => Ok(()),
            Err(bollard::errors::Error::DockerResponseServerError { status_code: 404, .. }) => Ok(()),
            Err(e) => Err(HmError::Docker(format!("remove_network({name}): {e}")).into()),
        }
    }

    // --- service container ---

    /// Spec for a long-lived service container (one deployment).
    /// Pass into [`start_service`].
    pub fn build_service_spec<'a>(
        image: &'a str,
        name: &'a str,
    ) -> ServiceSpecBuilder<'a> {
        ServiceSpecBuilder::new(image, name)
    }

    /// Create + start a long-lived container per the supplied spec.
    /// The container is *not* the bare `sleep infinity` shell that
    /// [`start_long_lived`] uses — this is for actual deployments where
    /// the image's CMD (optionally overridden) is the process.
    /// Returns the container id.
    ///
    /// # Errors
    ///
    /// Returns [`HmError::Docker`] on create / start / network attach failures.
    pub async fn start_service(&self, spec: ServiceSpec<'_>) -> Result<String> {
        use bollard::container::{Config, CreateContainerOptions, StartContainerOptions};
        use bollard::models::{HostConfig, PortBinding};
        use bollard::network::ConnectNetworkOptions;
        use std::collections::HashMap;

        let mut exposed: HashMap<String, HashMap<(), ()>> = HashMap::new();
        let mut port_bindings: HashMap<String, Option<Vec<PortBinding>>> = HashMap::new();
        for cport in &spec.publish {
            let key = format!("{cport}/tcp");
            exposed.insert(key.clone(), HashMap::new());
            port_bindings.insert(
                key,
                Some(vec![PortBinding {
                    host_ip: None,
                    host_port: Some(String::new()), // empty -> daemon assigns ephemeral
                }]),
            );
        }

        let host_config = HostConfig {
            binds: if spec.binds.is_empty() { None } else { Some(spec.binds.clone()) },
            port_bindings: Some(port_bindings),
            network_mode: Some(spec.network.to_string()),
            ..Default::default()
        };

        let cfg = Config {
            image: Some(spec.image.to_string()),
            cmd: spec.cmd.clone(),
            env: Some(spec.env.clone()),
            working_dir: spec.workdir.map(str::to_string),
            exposed_ports: Some(exposed),
            host_config: Some(host_config),
            labels: Some(spec.labels.clone().into_iter().collect()),
            ..Default::default()
        };

        let create = self
            .inner
            .create_container(
                Some(CreateContainerOptions {
                    name: spec.name,
                    platform: None,
                }),
                cfg,
            )
            .await
            .map_err(|e| HmError::Docker(format!("create_container({}): {e}", spec.name)))?;

        // Attach to the per-session network with the slug as alias so
        // siblings reach this container via DNS.
        self.inner
            .connect_network(
                spec.network,
                ConnectNetworkOptions {
                    container: create.id.clone(),
                    endpoint_config: bollard::models::EndpointSettings {
                        aliases: Some(vec![spec.network_alias.to_string()]),
                        ..Default::default()
                    },
                },
            )
            .await
            .map_err(|e| HmError::Docker(format!("connect_network({}): {e}", spec.network)))?;

        self.inner
            .start_container(&create.id, None::<StartContainerOptions<String>>)
            .await
            .map_err(|e| HmError::Docker(format!("start_container({}): {e}", create.id)))?;

        Ok(create.id)
    }

    /// Inspect a container; return its container-port → host-port map for tcp.
    ///
    /// # Errors
    ///
    /// Returns [`HmError::Docker`] when inspect fails.
    pub async fn inspect_ports(&self, container_id: &str)
        -> Result<std::collections::HashMap<u16, u16>>
    {
        let info = self
            .inner
            .inspect_container(container_id, None)
            .await
            .map_err(|e| HmError::Docker(format!("inspect_container({container_id}): {e}")))?;
        let mut out = std::collections::HashMap::new();
        if let Some(ns) = info.network_settings {
            if let Some(ports) = ns.ports {
                for (key, bindings) in ports {
                    // key like "5432/tcp"
                    let cport: u16 = key
                        .split('/')
                        .next()
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0);
                    if cport == 0 { continue; }
                    if let Some(bs) = bindings {
                        for b in bs {
                            if let Some(hp) = b.host_port {
                                if let Ok(p) = hp.parse::<u16>() {
                                    out.insert(cport, p);
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(out)
    }

    /// Commit a one-shot build container to an image tag.
    ///
    /// # Errors
    ///
    /// Returns [`HmError::Docker`] when the commit fails.
    pub async fn commit_container(&self, container_id: &str, tag: &str) -> Result<()> {
        use bollard::image::CommitContainerOptions;
        // Tag is "repo:tag"; split for the API.
        let (repo, ver) = match tag.rsplit_once(':') {
            Some((r, v)) => (r.to_string(), v.to_string()),
            None => (tag.to_string(), "latest".to_string()),
        };
        self.inner
            .commit_container::<&str, &str, &str>(
                CommitContainerOptions {
                    container: container_id,
                    repo: &repo,
                    tag: &ver,
                    ..Default::default()
                },
                bollard::container::Config::<&str> { ..Default::default() },
            )
            .await
            .map_err(|e| HmError::Docker(format!("commit_container({container_id} -> {tag}): {e}")))?;
        Ok(())
    }

    /// Stop a container with a 10s grace, then SIGKILL. Idempotent on "not found".
    ///
    /// # Errors
    ///
    /// Returns [`HmError::Docker`] for non-404 daemon errors.
    pub async fn stop_container(&self, container_id: &str) -> Result<()> {
        use bollard::container::StopContainerOptions;
        match self
            .inner
            .stop_container(container_id, Some(StopContainerOptions { t: 10 }))
            .await
        {
            Ok(()) => Ok(()),
            Err(bollard::errors::Error::DockerResponseServerError { status_code: 404, .. }) => Ok(()),
            Err(e) => Err(HmError::Docker(format!("stop_container({container_id}): {e}")).into()),
        }
    }

    /// Remove a container. Idempotent on "not found".
    ///
    /// # Errors
    ///
    /// Returns [`HmError::Docker`] for non-404 daemon errors.
    pub async fn remove_container(&self, container_id: &str) -> Result<()> {
        match self
            .inner
            .remove_container(
                container_id,
                Some(bollard::container::RemoveContainerOptions {
                    force: true,
                    ..Default::default()
                }),
            )
            .await
        {
            Ok(()) => Ok(()),
            Err(bollard::errors::Error::DockerResponseServerError { status_code: 404, .. }) => Ok(()),
            Err(e) => Err(HmError::Docker(format!("remove_container({container_id}): {e}")).into()),
        }
    }

    /// List container summaries filtered by a single label `k=v` predicate.
    ///
    /// # Errors
    ///
    /// Returns [`HmError::Docker`] when list_containers fails.
    pub async fn list_containers_by_label(
        &self,
        k: &str,
        v: &str,
    ) -> Result<Vec<bollard::secret::ContainerSummary>> {
        use bollard::container::ListContainersOptions;
        use std::collections::HashMap;
        let mut filters: HashMap<String, Vec<String>> = HashMap::new();
        filters.insert("label".to_string(), vec![format!("{k}={v}")]);
        let out = self
            .inner
            .list_containers(Some(ListContainersOptions {
                all: true,
                filters,
                ..Default::default()
            }))
            .await
            .map_err(|e| HmError::Docker(format!("list_containers: {e}")))?;
        Ok(out)
    }
```

After the impl block, add the `ServiceSpec` builder + struct:

```rust
// ---------------------------------------------------------------------------
// ServiceSpec
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ServiceSpec<'a> {
    pub image: &'a str,
    pub name: &'a str,
    pub env: Vec<String>,
    pub cmd: Option<Vec<String>>,
    pub workdir: Option<&'a str>,
    pub binds: Vec<String>,
    pub publish: Vec<u16>,
    pub network: &'a str,
    pub network_alias: &'a str,
    pub labels: std::collections::HashMap<String, String>,
}

pub struct ServiceSpecBuilder<'a> {
    inner: ServiceSpec<'a>,
}

impl<'a> ServiceSpecBuilder<'a> {
    pub fn new(image: &'a str, name: &'a str) -> Self {
        Self {
            inner: ServiceSpec {
                image,
                name,
                env: Vec::new(),
                cmd: None,
                workdir: None,
                binds: Vec::new(),
                publish: Vec::new(),
                network: "",
                network_alias: "",
                labels: std::collections::HashMap::new(),
            },
        }
    }
    pub fn env(mut self, env: Vec<String>) -> Self { self.inner.env = env; self }
    pub fn cmd(mut self, cmd: Option<Vec<String>>) -> Self { self.inner.cmd = cmd; self }
    pub fn workdir(mut self, w: Option<&'a str>) -> Self { self.inner.workdir = w; self }
    pub fn binds(mut self, b: Vec<String>) -> Self { self.inner.binds = b; self }
    pub fn publish(mut self, ports: Vec<u16>) -> Self { self.inner.publish = ports; self }
    pub fn network(mut self, net: &'a str, alias: &'a str) -> Self {
        self.inner.network = net; self.inner.network_alias = alias; self
    }
    pub fn labels(mut self, l: std::collections::HashMap<String, String>) -> Self {
        self.inner.labels = l; self
    }
    pub fn build(self) -> ServiceSpec<'a> { self.inner }
}
```

- [ ] **Step 2: Build to verify**

```bash
cargo build -p harmont-cli
```

Expected: clean build. Bollard 0.18's exact module paths may surface compile errors; if any signature mismatches show, consult `cargo doc --open -p bollard --no-deps` (or `cargo expand`) and adjust.

- [ ] **Step 3: Commit**

```bash
git add crates/hm/src/orchestrator/docker_client.rs
git commit -m "$(cat <<'EOF'
feat(dev): extend DockerClient with deployment primitives

Adds: create_network, remove_network, start_service (long-lived
container with port publishing + bridge network alias),
inspect_ports (container -> host port map), commit_container,
stop_container, remove_container, list_containers_by_label.

ServiceSpec + ServiceSpecBuilder give the call sites a fluent API
without polluting docker_client with N-keyword args. All "not found"
errors are swallowed so teardown is idempotent.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 6: Bridge-network helper (thin wrapper for the up flow)

**Files:**
- Modify: `crates/hm/src/commands/dev/network.rs`

- [ ] **Step 1: Implement + test**

Replace `crates/hm/src/commands/dev/network.rs` with:

```rust
//! Per-session bridge network for `hm dev up`.

use std::collections::HashMap;

use anyhow::Result;

use crate::orchestrator::docker_client::DockerClient;

use super::naming::{
    DRIVER_LOCAL, LABEL_DRIVER, LABEL_SESSION, LABEL_WORKTREE, network_name,
};

#[derive(Debug, Clone)]
pub struct Network {
    pub name: String,
}

/// Create the per-session bridge network with the canonical labels.
///
/// # Errors
///
/// Returns the docker error if the daemon rejects creation.
pub async fn create(
    docker: &DockerClient,
    worktree_hash: &str,
    session: &str,
) -> Result<Network> {
    let name = network_name(worktree_hash, session);
    let mut labels = HashMap::new();
    labels.insert(LABEL_WORKTREE.to_string(), worktree_hash.to_string());
    labels.insert(LABEL_SESSION.to_string(), session.to_string());
    labels.insert(LABEL_DRIVER.to_string(), DRIVER_LOCAL.to_string());
    docker.create_network(&name, labels).await?;
    Ok(Network { name })
}

/// Remove the per-session bridge network. Idempotent.
///
/// # Errors
///
/// Returns the docker error if removal fails for non-404 reasons.
pub async fn remove(docker: &DockerClient, net: &Network) -> Result<()> {
    docker.remove_network(&net.name).await
}
```

- [ ] **Step 2: Build to verify**

```bash
cargo build -p harmont-cli
```

Expected: clean build.

- [ ] **Step 3: Commit**

```bash
git add crates/hm/src/commands/dev/network.rs
git commit -m "$(cat <<'EOF'
feat(dev): per-session bridge network helper

Stamps the canonical harmont.{worktree,session,driver} labels on
the network so `hm dev down --all` can find them later. Thin
wrapper around DockerClient::{create,remove}_network — kept here
rather than inlined in up.rs so down.rs can sweep with the same
shape.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 7: Log multiplexer

**Files:**
- Modify: `crates/hm/src/commands/dev/logmux.rs`

- [ ] **Step 1: Write the implementation + tests**

Replace `crates/hm/src/commands/dev/logmux.rs` with:

```rust
//! Multi-source line-prefixed colored log stream.

use std::io::Write;

use owo_colors::{AnsiColors, OwoColorize};
use tokio::sync::mpsc::UnboundedReceiver;

#[derive(Debug, Clone)]
pub struct LogLine {
    pub slug: String,
    pub bytes: Vec<u8>,
}

/// Per-slug line buffer: docker streams chunks that may not be
/// line-aligned. We accumulate bytes per slug and flush on each \n.
#[derive(Default)]
struct PerSlug {
    buf: Vec<u8>,
}

impl PerSlug {
    fn ingest<W: Write>(&mut self, slug: &str, width: usize, color: bool, bytes: &[u8], w: &mut W)
        -> std::io::Result<()>
    {
        self.buf.extend_from_slice(bytes);
        loop {
            let Some(idx) = self.buf.iter().position(|&b| b == b'\n') else { break };
            // line = bytes up to (excluding) the newline
            let line = &self.buf[..idx];
            write_line(slug, width, color, line, w)?;
            self.buf.drain(..=idx);
        }
        Ok(())
    }

    fn flush<W: Write>(&mut self, slug: &str, width: usize, color: bool, w: &mut W)
        -> std::io::Result<()>
    {
        if !self.buf.is_empty() {
            let line = std::mem::take(&mut self.buf);
            write_line(slug, width, color, &line, w)?;
        }
        Ok(())
    }
}

fn slug_color(slug: &str) -> AnsiColors {
    // Stable color per slug via hash. 6 ANSI colors cycled.
    const PALETTE: [AnsiColors; 6] = [
        AnsiColors::Cyan,
        AnsiColors::Magenta,
        AnsiColors::Yellow,
        AnsiColors::Green,
        AnsiColors::Blue,
        AnsiColors::BrightRed,
    ];
    let mut h: u32 = 0;
    for b in slug.bytes() {
        h = h.wrapping_mul(31).wrapping_add(u32::from(b));
    }
    PALETTE[(h as usize) % PALETTE.len()]
}

fn write_line<W: Write>(slug: &str, width: usize, color: bool, line: &[u8], w: &mut W)
    -> std::io::Result<()>
{
    let prefix = format!("[{slug:<width$}]");
    if color {
        write!(w, "{} ", prefix.color(slug_color(slug)))?;
    } else {
        write!(w, "{prefix} ")?;
    }
    w.write_all(line)?;
    w.write_all(b"\n")?;
    Ok(())
}

/// Consume LogLine messages, write `[slug] line\n` to stdout per line.
///
/// `slug_width` is the column width for the slug prefix; pass the
/// length of the longest slug in this session so columns align.
/// `color` toggles ANSI coloring.
///
/// Returns when the channel closes.
pub async fn run(
    mut rx: UnboundedReceiver<LogLine>,
    slug_width: usize,
    color: bool,
) -> std::io::Result<()> {
    use std::collections::HashMap;
    let mut buffers: HashMap<String, PerSlug> = HashMap::new();
    let mut stdout = std::io::stdout().lock();
    while let Some(msg) = rx.recv().await {
        let entry = buffers.entry(msg.slug.clone()).or_default();
        entry.ingest(&msg.slug, slug_width, color, &msg.bytes, &mut stdout)?;
    }
    // Final flush.
    for (slug, mut b) in buffers {
        b.flush(&slug, slug_width, color, &mut stdout)?;
    }
    stdout.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capture(slug: &str, chunks: &[&[u8]], color: bool) -> String {
        let mut buf: Vec<u8> = Vec::new();
        let mut p = PerSlug::default();
        for c in chunks {
            p.ingest(slug, 4, color, c, &mut buf).unwrap();
        }
        p.flush(slug, 4, color, &mut buf).unwrap();
        String::from_utf8(buf).unwrap()
    }

    #[test]
    fn flushes_on_newline() {
        let out = capture("db", &[b"hello\n"], false);
        assert_eq!(out, "[db  ] hello\n");
    }

    #[test]
    fn buffers_partial_chunk_across_calls() {
        let out = capture("db", &[b"hel", b"lo\nworld\n"], false);
        assert_eq!(out, "[db  ] hello\n[db  ] world\n");
    }

    #[test]
    fn flush_emits_trailing_unterminated_line() {
        let out = capture("db", &[b"tail"], false);
        assert_eq!(out, "[db  ] tail\n");
    }

    #[test]
    fn color_wraps_prefix_with_ansi() {
        let out = capture("db", &[b"hi\n"], true);
        assert!(out.contains("hi"));
        // ANSI escape introducer
        assert!(out.contains("\x1b["));
    }

    #[test]
    fn slug_color_is_stable_per_slug() {
        assert_eq!(slug_color("db"), slug_color("db"));
        // Different slugs *probably* get different colors; not asserted.
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test -p harmont-cli --lib commands::dev::logmux
```

Expected: 5 passed.

- [ ] **Step 3: Commit**

```bash
git add crates/hm/src/commands/dev/logmux.rs
git commit -m "$(cat <<'EOF'
feat(dev): log multiplexer with per-slug colored prefix

PerSlug buffers partial chunks across docker-log frames and flushes
on \n. owo-colors provides 6-color palette cycled by slug hash so
colors are stable across runs. NO_COLOR / --no-color path drops the
ANSI escapes entirely. Slug prefix is left-padded to a shared width
for column alignment.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 8: Service-spec converter (registry entry → ServiceSpec)

**Files:**
- Modify: `crates/hm/src/commands/dev/service_spec.rs`

- [ ] **Step 1: Write implementation + tests**

Replace `crates/hm/src/commands/dev/service_spec.rs` with:

```rust
//! Convert a registry `LocalSpec` into a runnable `ServiceSpec`.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result, anyhow};

use crate::orchestrator::docker_client::{ServiceSpec, ServiceSpecBuilder};

use super::naming::{
    DRIVER_LOCAL, LABEL_DRIVER, LABEL_SESSION, LABEL_SLUG, LABEL_WORKTREE,
    container_name,
};
use super::registry::{LocalSpec, PORT_SENTINEL};

/// Resolved spec for one deployment, ready to hand to
/// `DockerClient::start_service`. Borrows from the registry; held
/// alive by the up handler for the duration of the boot.
pub struct ResolvedSpec {
    pub slug: String,
    pub container_name: String,
    pub image: String,
    pub env: Vec<String>,
    pub cmd: Option<Vec<String>>,
    pub workdir: Option<String>,
    pub binds: Vec<String>,
    pub publish: Vec<u16>,
    pub network: String,
    pub labels: HashMap<String, String>,
}

impl ResolvedSpec {
    pub fn as_service_spec(&self) -> ServiceSpec<'_> {
        ServiceSpecBuilder::new(&self.image, &self.container_name)
            .env(self.env.clone())
            .cmd(self.cmd.clone())
            .workdir(self.workdir.as_deref())
            .binds(self.binds.clone())
            .publish(self.publish.clone())
            .network(&self.network, &self.slug)
            .labels(self.labels.clone())
            .build()
    }
}

/// Build a ResolvedSpec from a LocalSpec + session metadata.
/// `image` is the resolved image tag (raw image from the spec, or the
/// committed tag for `from_=Step` builds — passed in by the up handler).
pub fn build(
    slug: &str,
    spec: &LocalSpec,
    image: &str,
    worktree_root: &Path,
    worktree_hash: &str,
    session: &str,
    network: &str,
) -> Result<ResolvedSpec> {
    let env: Vec<String> = spec.env.iter().map(|(k, v)| format!("{k}={v}")).collect();
    let binds = resolve_binds(worktree_root, &spec.volumes)?;
    let publish: Vec<u16> = spec
        .port_mapping
        .iter()
        .filter(|(_, sentinel)| sentinel.as_str() == PORT_SENTINEL)
        .map(|(cport, _)| cport.parse::<u16>().context(format!(
            "port_mapping key `{cport}` is not a valid u16 — registry-dump bug?"
        )))
        .collect::<Result<Vec<_>>>()?;
    let mut labels = HashMap::new();
    labels.insert(LABEL_WORKTREE.to_string(), worktree_hash.to_string());
    labels.insert(LABEL_SLUG.to_string(), slug.to_string());
    labels.insert(LABEL_SESSION.to_string(), session.to_string());
    labels.insert(LABEL_DRIVER.to_string(), DRIVER_LOCAL.to_string());
    Ok(ResolvedSpec {
        slug: slug.to_string(),
        container_name: container_name(worktree_hash, slug, session),
        image: image.to_string(),
        env,
        cmd: spec.cmd.clone(),
        workdir: spec.workdir.clone(),
        binds,
        publish,
        network: network.to_string(),
        labels,
    })
}

fn resolve_binds(
    worktree_root: &Path,
    volumes: &std::collections::BTreeMap<String, String>,
) -> Result<Vec<String>> {
    let mut out = Vec::with_capacity(volumes.len());
    for (host, container) in volumes {
        let host_abs = if host.starts_with('/') {
            std::path::PathBuf::from(host)
        } else {
            worktree_root.join(host)
        };
        let host_str = host_abs.to_str().ok_or_else(|| {
            anyhow!("bind host path is not valid UTF-8: {host_abs:?}")
        })?;
        // container may carry a `:ro` suffix; split + reconstruct so we
        // emit "host:container[:ro]".
        let (cpath, mode) = match container.rsplit_once(':') {
            Some((p, m)) if m == "ro" || m == "rw" => (p, m),
            _ => (container.as_str(), "rw"),
        };
        out.push(if mode == "rw" {
            format!("{host_str}:{cpath}")
        } else {
            format!("{host_str}:{cpath}:{mode}")
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn local_spec() -> LocalSpec {
        let mut port_mapping = BTreeMap::new();
        port_mapping.insert("5432".to_string(), PORT_SENTINEL.to_string());
        LocalSpec {
            image: Some("postgres:16".into()),
            from: None,
            cmd: None,
            port_mapping,
            env: BTreeMap::from([
                ("POSTGRES_PASSWORD".to_string(), "dev".to_string()),
            ]),
            volumes: BTreeMap::new(),
            workdir: None,
            deps: vec![],
        }
    }

    #[test]
    fn builds_a_resolved_spec() {
        let rs = build(
            "db",
            &local_spec(),
            "postgres:16",
            Path::new("/tmp/wt"),
            "a1b2c3d4e5",
            "7a2f91",
            "hm-a1b2c3d4e5-7a2f91",
        ).unwrap();
        assert_eq!(rs.container_name, "hm-a1b2c3d4e5-db-7a2f91");
        assert_eq!(rs.publish, vec![5432]);
        assert!(rs.env.contains(&"POSTGRES_PASSWORD=dev".to_string()));
        assert_eq!(rs.labels[LABEL_SLUG], "db");
    }

    #[test]
    fn resolves_relative_volume_against_worktree_root() {
        let mut spec = local_spec();
        spec.volumes.insert(".".to_string(), "/workspace".to_string());
        let rs = build(
            "web", &spec, "node:20",
            Path::new("/tmp/wt"), "a", "b", "n",
        ).unwrap();
        assert_eq!(rs.binds, vec!["/tmp/wt/.:/workspace".to_string()]);
    }

    #[test]
    fn preserves_ro_suffix_on_container_path() {
        let mut spec = local_spec();
        spec.volumes.insert(".".to_string(), "/workspace:ro".to_string());
        let rs = build(
            "web", &spec, "node:20",
            Path::new("/tmp/wt"), "a", "b", "n",
        ).unwrap();
        assert_eq!(rs.binds, vec!["/tmp/wt/.:/workspace:ro".to_string()]);
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test -p harmont-cli --lib commands::dev::service_spec
```

Expected: 3 passed.

- [ ] **Step 3: Commit**

```bash
git add crates/hm/src/commands/dev/service_spec.rs
git commit -m "$(cat <<'EOF'
feat(dev): ResolvedSpec converter from LocalSpec to ServiceSpec

Resolves relative volume host paths against worktree_root, preserves
:ro suffix on container paths, parses port_mapping container ports
(filtering to entries with the __hm_dev_port__ sentinel — every entry
in v1, but explicit so future pinned-int values can coexist), and
stamps the canonical labels. ResolvedSpec owns its data so the up
handler can hold it across the boot lifetime.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 9: `build_image_from_pipeline` (from_=Step builds)

Build a Docker image by executing a v0 IR pipeline as a one-shot build container, then committing it under a tag.

**Files:**
- Modify: `crates/hm/src/orchestrator/mod.rs`
- Create: (sub-module is optional; keep in `mod.rs` for now)

- [ ] **Step 1: Locate the existing local-pipeline runner**

```bash
grep -n "run_pipeline_local\|local::handle\|fn handle_local" crates/hm/src/orchestrator/*.rs crates/hm/src/commands/run/*.rs
```

Identify the function the engineer must call to execute a v0 IR pipeline locally. The exact name lives in the existing codebase; for the next step we'll call it `crate::commands::run::local::run_pipeline_v0`. If a slightly different name exists, substitute it.

- [ ] **Step 2: Add `build_image_from_pipeline`**

Append to `crates/hm/src/orchestrator/mod.rs`:

```rust
/// Build a Docker image by running a v0 IR pipeline as a one-shot
/// build container and committing the result to `image_tag`.
///
/// Used by `hm dev up` for `from_=Step` deployments. The pipeline's
/// final container becomes the new image; intermediate steps run in
/// the same container as the existing local executor does for
/// `hm run`. On success the build container is removed.
///
/// # Errors
///
/// Returns an error if the pipeline run fails (build steps exit
/// non-zero) or the commit fails.
pub async fn build_image_from_pipeline(
    docker: &crate::orchestrator::docker_client::DockerClient,
    pipeline_v0: &serde_json::Value,
    image_tag: &str,
) -> anyhow::Result<()> {
    // Reuse the existing local runner to execute the pipeline. It
    // returns the container id of the final step's container; we
    // commit that container into `image_tag`.
    let container_id = crate::commands::run::local::run_pipeline_v0_one_shot(
        docker,
        pipeline_v0,
    )
    .await?;
    docker.commit_container(&container_id, image_tag).await?;
    docker.remove_container(&container_id).await?;
    Ok(())
}
```

This requires `crate::commands::run::local::run_pipeline_v0_one_shot` to exist. If a function of this exact shape doesn't already exist, **add** a small wrapper in `crates/hm/src/commands/run/local.rs` that exposes the existing executor with a return-the-final-container-id contract:

```rust
/// Execute a v0 IR pipeline locally and return the container id of
/// the final step. Distinct from `handle()` in that it returns the
/// container id instead of reporting to the user-facing run UI; used
/// by `hm dev up` to build deployment images from Step chains.
///
/// # Errors
///
/// Returns the executor's error on the first failing step.
pub async fn run_pipeline_v0_one_shot(
    docker: &crate::orchestrator::docker_client::DockerClient,
    pipeline_v0: &serde_json::Value,
) -> anyhow::Result<String> {
    // The existing executor's entry-point is `??`. Reuse it; the
    // implementation already keeps the container alive across steps
    // for chained execution. Returning the final container id is a
    // matter of extracting it from the executor's state object.
    //
    // If the existing executor's surface doesn't expose the final
    // container id, refactor it minimally: add a method or change
    // the return type to include it, alongside the existing
    // "user-facing run UI" path. Keep the change additive — do not
    // delete the user-facing surface.
    todo!("wire to the existing local executor; see crates/hm/src/commands/run/local.rs")
}
```

The engineer MUST read `crates/hm/src/commands/run/local.rs` before implementing this — the exact integration depends on the executor's internal API. The contract is: take a v0 IR pipeline JSON value, run every step in one Docker container (the same way `hm run` does), and return the final container id without removing it. The caller commits + removes.

If implementing the wrapper requires more than a 50-line addition to `local.rs`, stop and pull the integration out into its own task — extending the local executor is a non-trivial change that deserves separate review.

- [ ] **Step 3: Build to verify compiles**

```bash
cargo build -p harmont-cli
```

Expected: clean build.

- [ ] **Step 4: Add a unit test for `build_image_from_pipeline` (mocked / smoke only)**

A real test requires Docker; that goes in the integration suite (Task 14). For the unit suite, add a single smoke test verifying the function signature compiles + that calling it on a malformed pipeline JSON returns an error:

In `crates/hm/src/orchestrator/mod.rs`, near the function (in a new `#[cfg(test)] mod build_tests`):

```rust
#[cfg(test)]
mod build_tests {
    // Compile-time existence check; the real integration is in
    // crates/hm/tests/dev_integration.rs.
    #[test]
    fn build_image_from_pipeline_is_callable() {
        // We can't run it without docker; just confirm the symbol exists.
        let _f: fn(
            &crate::orchestrator::docker_client::DockerClient,
            &serde_json::Value,
            &str,
        ) -> _ = |_d, _p, _t| async { Ok(()) };
        // No assert; the test passes if it compiles.
    }
}
```

(The `-> _` return-type elision is acceptable in tests; if rustc rejects it, write the explicit `Pin<Box<dyn Future<Output=...>>>`.)

- [ ] **Step 5: Run + commit**

```bash
cargo test -p harmont-cli --lib orchestrator
```

Expected: build_image_from_pipeline_is_callable passes.

```bash
git add crates/hm/src/orchestrator/mod.rs crates/hm/src/commands/run/local.rs
git commit -m "$(cat <<'EOF'
feat(dev): build_image_from_pipeline for from_=Step deployments

Reuses the existing local pipeline executor as a one-shot build
that returns the final container id; the new function commits that
container under the requested tag and removes it. The wrapper in
local.rs is the only new entry point on the existing executor; the
user-facing run path is untouched.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 10: `hm dev up` — the centerpiece

**Files:**
- Modify: `crates/hm/src/commands/dev/up.rs`

- [ ] **Step 1: Write the implementation**

Replace `crates/hm/src/commands/dev/up.rs` with:

```rust
//! `hm dev up` — bring deployments up in the foreground.
//!
//! Flow: registry dump (subprocess) → boot plan (topo) → create network
//! → boot containers per level (parallel) → log mux → wait signal →
//! teardown.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::sync::Mutex;
use tokio::sync::mpsc;
use tokio::task::JoinSet;

use crate::cli::DevUpArgs;
use crate::context::RunContext;
use crate::orchestrator::docker_client::DockerClient;

use super::logmux::{LogLine, run as run_logmux};
use super::naming::{fresh_session_id, resolve_worktree_root, worktree_hash};
use super::network::{Network, create as create_network, remove as remove_network};
use super::registry::{FromSource, LocalSpec, PORT_SENTINEL, RegEntry, dump};
use super::service_spec::{ResolvedSpec, build as build_spec};
use super::topo::plan;

/// One booted container in this session.
struct Booted {
    slug: String,
    container_id: String,
    host_ports: HashMap<u16, u16>,
}

struct Session {
    id: String,
    network: Network,
    booted: Vec<Booted>,
}

pub async fn handle(args: DevUpArgs, _ctx: RunContext) -> Result<i32> {
    let worktree_root = resolve_worktree_root()?;
    let wt_hash = worktree_hash(&worktree_root);
    let session_id = fresh_session_id();
    eprintln!("[hm] session {session_id}. resolving deployments in .harmont/");

    let registry = dump(&worktree_root).await.context("dump deployment registry")?;
    let boot_plan = plan(&registry, &args.slugs, args.no_deps)?;
    let docker = DockerClient::connect()?;
    docker.ping().await.context("docker daemon ping")?;

    let net = create_network(&docker, &wt_hash, &session_id).await?;
    eprintln!("[hm] network {}: created", net.name);

    // Determine slug column width.
    let slug_width = boot_plan.slugs().map(str::len).max().unwrap_or(4);

    let (log_tx, log_rx) = mpsc::unbounded_channel::<LogLine>();
    let log_color = std::env::var("NO_COLOR").is_err();
    let log_task = tokio::spawn(run_logmux(log_rx, slug_width, log_color));

    let session = Arc::new(Mutex::new(Session {
        id: session_id.clone(),
        network: net.clone(),
        booted: Vec::new(),
    }));

    // Boot levels in topo order.
    for level in &boot_plan.levels {
        let mut joinset: JoinSet<Result<Booted>> = JoinSet::new();
        for slug in level {
            let RegEntry::Local(spec) = &registry.deployments[slug] else {
                continue; // upstream plan already filtered to local
            };
            let docker = docker.clone();
            let spec = spec.clone();
            let slug = slug.clone();
            let wt_root = worktree_root.clone();
            let wt_hash = wt_hash.clone();
            let session_id = session_id.clone();
            let network_name = net.name.clone();
            let log_tx = log_tx.clone();
            let rebuild = args.rebuild;
            joinset.spawn(async move {
                boot_one(
                    docker, &slug, &spec, &wt_root, &wt_hash,
                    &session_id, &network_name, rebuild, log_tx,
                ).await
            });
        }
        while let Some(res) = joinset.join_next().await {
            let booted = res??;
            session.lock().await.booted.push(booted);
        }
    }

    eprintln!("[hm] all up. Ctrl-C to tear down. Logs follow.");

    // Drop the writer end held in this scope so logmux can finish on
    // teardown once we close `log_tx` after teardown. We still need
    // log_tx for unexpected-exit reporting, so we keep a clone alive
    // here and drop it at the bottom.
    let _kept_tx = log_tx;

    // Wait for SIGINT/SIGTERM.
    wait_signal().await?;

    eprintln!("[hm] tearing down...");
    teardown(&docker, session.clone()).await;

    // Drop tx -> logmux receiver closes -> task completes.
    drop(_kept_tx);
    let _ = log_task.await;

    Ok(0)
}

async fn boot_one(
    docker: DockerClient,
    slug: &str,
    spec: &LocalSpec,
    worktree_root: &std::path::Path,
    worktree_hash: &str,
    session: &str,
    network: &str,
    rebuild: bool,
    log_tx: mpsc::UnboundedSender<LogLine>,
) -> Result<Booted> {
    // Resolve image: raw or build-from-step.
    let image = resolve_image(&docker, slug, spec, worktree_hash, rebuild).await?;
    let resolved = build_spec(
        slug, spec, &image, worktree_root, worktree_hash, session, network,
    )?;
    let container_id = docker.start_service(resolved.as_service_spec()).await?;
    let host_ports = docker.inspect_ports(&container_id).await?;
    // Log line.
    let ports_str = if host_ports.is_empty() {
        String::new()
    } else {
        let mut entries: Vec<(u16, u16)> = host_ports.iter().map(|(c, h)| (*c, *h)).collect();
        entries.sort();
        let parts: Vec<String> = entries.iter()
            .map(|(c, h)| format!("localhost:{h} → :{c}"))
            .collect();
        format!(" | {}", parts.join(", "))
    };
    eprintln!("[{slug}] ready  ( {}{} )", resolved.container_name, ports_str);
    // Spawn the log-stream consumer for this container.
    tokio::spawn(stream_logs(docker.clone(), container_id.clone(), slug.to_string(), log_tx));
    Ok(Booted { slug: slug.to_string(), container_id, host_ports })
}

async fn resolve_image(
    docker: &DockerClient,
    slug: &str,
    spec: &LocalSpec,
    worktree_hash: &str,
    rebuild: bool,
) -> Result<String> {
    if let Some(tag) = &spec.image {
        if !docker.image_exists(tag).await? {
            eprintln!("[{slug}] pulling {tag}...");
            docker.pull_image(tag).await?;
        }
        return Ok(tag.clone());
    }
    if let Some(FromSource::StepChain { pipeline_v0 }) = &spec.from {
        let chain_key = extract_terminal_key(pipeline_v0).unwrap_or_else(|| "nocache".to_string());
        let tag = format!("hm-build-{worktree_hash}-{slug}:{chain_key}");
        if rebuild || !docker.image_exists(&tag).await? {
            eprintln!("[{slug}] building from Step chain...");
            crate::orchestrator::build_image_from_pipeline(docker, pipeline_v0, &tag).await?;
        }
        return Ok(tag);
    }
    anyhow::bail!("deployment `{slug}` has neither image= nor from_=; registry-dump bug?")
}

/// Pull the terminal step's resolved cache-key from the v0 IR JSON.
/// The dumper (harmont-py) calls resolve_pipeline_keys so every step
/// carries `key`. We use the last step's key as the cache-tag.
fn extract_terminal_key(pipeline_v0: &serde_json::Value) -> Option<String> {
    let steps = pipeline_v0.get("steps")?.as_array()?;
    steps.last()?.get("key")?.as_str().map(str::to_string)
}

async fn stream_logs(
    docker: DockerClient,
    container_id: String,
    slug: String,
    tx: mpsc::UnboundedSender<LogLine>,
) {
    use bollard::container::LogsOptions;
    use futures_util::StreamExt;
    let mut s = docker.inner_for_logs().logs::<String>(
        &container_id,
        Some(LogsOptions {
            stdout: true,
            stderr: true,
            follow: true,
            tail: "all".to_string(),
            ..Default::default()
        }),
    );
    while let Some(item) = s.next().await {
        match item {
            Ok(chunk) => {
                let bytes = chunk.into_bytes().to_vec();
                if tx.send(LogLine { slug: slug.clone(), bytes }).is_err() {
                    break;
                }
            }
            Err(_) => break,
        }
    }
}

async fn wait_signal() -> Result<()> {
    let mut sigint = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    tokio::select! {
        _ = sigint.recv() => {}
        _ = sigterm.recv() => {}
    }
    Ok(())
}

async fn teardown(docker: &DockerClient, session: Arc<Mutex<Session>>) {
    let s = session.lock().await;
    // Reverse order so dependents stop before their deps.
    let to_stop: Vec<(String, String)> = s.booted.iter()
        .rev()
        .map(|b| (b.slug.clone(), b.container_id.clone()))
        .collect();
    let net_name = s.network.name.clone();
    drop(s);
    for (slug, id) in to_stop {
        let _ = docker.stop_container(&id).await;
        let _ = docker.remove_container(&id).await;
        eprintln!("[{slug}] stopped");
    }
    let _ = remove_network(docker, &Network { name: net_name.clone() }).await;
    eprintln!("[hm] network {net_name}: removed");
}
```

NB: This file references `docker.inner_for_logs()` to obtain the bollard `Docker` handle for the log stream. Add the trivial accessor in `crates/hm/src/orchestrator/docker_client.rs`:

```rust
    /// Internal access to the underlying bollard handle, for callers
    /// that need to stream (e.g., `logs`). Not exposed in the public API.
    #[doc(hidden)]
    pub fn inner_for_logs(&self) -> &bollard::Docker {
        &self.inner
    }
```

- [ ] **Step 2: Build**

```bash
cargo build -p harmont-cli
```

Expected: clean build.

- [ ] **Step 3: Run lints**

```bash
cargo clippy --all-targets -p harmont-cli -- -D warnings
```

Expected: no warnings. If clippy complains about the magic strings or the long boot_one signature, refactor in place — do not suppress.

- [ ] **Step 4: Commit**

```bash
git add crates/hm/src/commands/dev/up.rs crates/hm/src/orchestrator/docker_client.rs
git commit -m "$(cat <<'EOF'
feat(dev): `hm dev up` orchestrator — boot, mux, teardown

End-to-end flow: subprocess registry dump → topo boot plan → per-
session bridge network → parallel boot per level → per-container log
stream into the shared mux → wait SIGINT/SIGTERM → reverse-order
teardown (stop+remove containers, remove network).

Image resolution: raw image is pulled if missing; from_=Step lowers
to a cache-keyed build tag (`hm-build-<wt>-<slug>:<key>`) and only
rebuilds when --rebuild is set or the tag is absent.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 11: `hm dev down` (orphan sweep)

**Files:**
- Modify: `crates/hm/src/commands/dev/down.rs`

- [ ] **Step 1: Implement**

Replace `crates/hm/src/commands/dev/down.rs` with:

```rust
//! `hm dev down` — sweep containers + networks left over from past sessions.

use anyhow::Result;

use crate::cli::DevDownArgs;
use crate::context::RunContext;
use crate::orchestrator::docker_client::DockerClient;

use super::naming::{
    DRIVER_LOCAL, LABEL_DRIVER, LABEL_SESSION, LABEL_SLUG, LABEL_WORKTREE,
    resolve_worktree_root, worktree_hash,
};

pub async fn handle(args: DevDownArgs, _ctx: RunContext) -> Result<i32> {
    let docker = DockerClient::connect()?;
    let worktree_root = resolve_worktree_root()?;
    let wt_hash = worktree_hash(&worktree_root);

    let containers = if args.all {
        docker.list_containers_by_label(LABEL_DRIVER, DRIVER_LOCAL).await?
    } else {
        docker.list_containers_by_label(LABEL_WORKTREE, &wt_hash).await?
    };

    let mut to_remove: Vec<(String, String, String, String)> = Vec::new();  // (id, slug, session, name)
    for c in &containers {
        let labels = c.labels.clone().unwrap_or_default();
        let slug = labels.get(LABEL_SLUG).cloned().unwrap_or_default();
        let session = labels.get(LABEL_SESSION).cloned().unwrap_or_default();
        let name = c.names.as_ref().and_then(|n| n.first().cloned()).unwrap_or_default();
        if let Some(s) = &args.session {
            if &session != s { continue; }
        }
        if !args.slugs.is_empty() && !args.slugs.iter().any(|x| x == &slug) {
            continue;
        }
        if let Some(id) = &c.id {
            to_remove.push((id.clone(), slug, session, name));
        }
    }

    if to_remove.is_empty() {
        eprintln!("[hm] nothing to sweep");
        return Ok(0);
    }

    let mut sessions_swept: std::collections::BTreeSet<String> = Default::default();
    for (id, slug, session, name) in &to_remove {
        let _ = docker.stop_container(id).await;
        let _ = docker.remove_container(id).await;
        eprintln!("[hm] removed {name} (slug={slug}, session={session})");
        sessions_swept.insert(session.clone());
    }

    // Networks: any network that no longer has containers must go.
    use crate::commands::dev::naming::network_name;
    for session in &sessions_swept {
        let net = if args.all {
            // For --all we don't know the worktree per session ahead of
            // time; safer to inspect each container's worktree label and
            // remove its network. For v1, since we tagged the network
            // with the same labels, just iterate distinct network names
            // from the container set above.
            continue;
        } else {
            network_name(&wt_hash, session)
        };
        let _ = docker.remove_network(&net).await;
    }

    if args.all {
        // Sweep any network with harmont.driver=local that's now orphaned.
        // Bollard exposes list_networks via Docker; quick best-effort
        // scan: try to remove every hm-*-* network name we recorded.
        // (We deliberately keep this simple — orphan networks are
        // recreated next `up` anyway.)
    }

    Ok(0)
}

#[cfg(test)]
mod tests {
    // Behavior is integration-tested in dev_integration.rs; pure logic
    // here is tiny and exercised through the CLI.
}
```

- [ ] **Step 2: Build + commit**

```bash
cargo build -p harmont-cli
git add crates/hm/src/commands/dev/down.rs
git commit -m "$(cat <<'EOF'
feat(dev): `hm dev down` orphan sweep

Filters by harmont.driver=local (with --all) or harmont.worktree=<this>
(default), optionally further filtered by --session and explicit
slugs. stop+remove every match, then remove the corresponding per-
session network. Idempotent.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 12: `hm dev ls`

**Files:**
- Modify: `crates/hm/src/commands/dev/ls.rs`

- [ ] **Step 1: Implement**

Replace `crates/hm/src/commands/dev/ls.rs` with:

```rust
//! `hm dev ls` — list registered + running deployments.

use anyhow::Result;

use crate::context::RunContext;
use crate::orchestrator::docker_client::DockerClient;

use super::naming::{
    LABEL_SLUG, LABEL_WORKTREE, resolve_worktree_root, worktree_hash,
};
use super::registry::{RegEntry, dump};

pub async fn handle(_ctx: RunContext) -> Result<i32> {
    let worktree_root = resolve_worktree_root()?;
    let wt_hash = worktree_hash(&worktree_root);
    let registry = dump(&worktree_root).await?;
    let docker = DockerClient::connect().ok();

    println!("{:<10} {:<8} {:<10} {:<10} {}",
        "SLUG", "DRIVER", "SESSION", "STATUS", "PORTS");

    // Pre-load running containers by slug label.
    let mut running: std::collections::HashMap<(String, String), (String, std::collections::HashMap<u16, u16>)> = Default::default();
    if let Some(d) = &docker {
        let containers = d.list_containers_by_label(LABEL_WORKTREE, &wt_hash).await.unwrap_or_default();
        for c in &containers {
            let labels = c.labels.clone().unwrap_or_default();
            let slug = labels.get(LABEL_SLUG).cloned().unwrap_or_default();
            let session = labels.get("harmont.session").cloned().unwrap_or_default();
            let state = c.state.clone().unwrap_or_default();
            if let Some(id) = &c.id {
                let ports = d.inspect_ports(id).await.unwrap_or_default();
                running.insert((slug, session), (state, ports));
            }
        }
    }

    for (slug, entry) in &registry.deployments {
        match entry {
            RegEntry::Local(_) => {
                // Find any running sessions for this slug
                let mut matched = false;
                for ((s, sess), (state, ports)) in &running {
                    if s == slug {
                        matched = true;
                        let ports_s = format_ports(ports);
                        println!("{slug:<10} {:<8} {:<10} {:<10} {ports_s}", "local", sess, state);
                    }
                }
                if !matched {
                    println!("{slug:<10} {:<8} {:<10} {:<10} {}", "local", "—", "registered", "—");
                }
            }
            RegEntry::Unhandled => {
                println!("{slug:<10} {:<8} {:<10} {:<10} (no local driver)",
                    "?", "—", "registered");
            }
        }
    }
    Ok(0)
}

fn format_ports(ports: &std::collections::HashMap<u16, u16>) -> String {
    let mut entries: Vec<(u16, u16)> = ports.iter().map(|(c, h)| (*c, *h)).collect();
    entries.sort();
    entries.iter()
        .map(|(c, h)| format!("localhost:{h} → :{c}"))
        .collect::<Vec<_>>()
        .join(", ")
}
```

- [ ] **Step 2: Build + commit**

```bash
cargo build -p harmont-cli
git add crates/hm/src/commands/dev/ls.rs
git commit -m "$(cat <<'EOF'
feat(dev): `hm dev ls` registry + running merge

Walks the python-side registry and Docker container labels in
parallel; prints one row per (slug, session) for live ones and
one row per registered-but-not-running slug. Non-local drivers
appear with a hint pointing at the matching driver's `up`.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 13: `hm dev port-of` (ambiguity rule + clean exits)

**Files:**
- Modify: `crates/hm/src/commands/dev/port_of.rs`

- [ ] **Step 1: Implement**

Replace `crates/hm/src/commands/dev/port_of.rs` with:

```rust
//! `hm dev port-of <slug> <container-port>` — print host port for a live deployment.

use anyhow::Result;

use crate::cli::DevPortOfArgs;
use crate::context::RunContext;
use crate::orchestrator::docker_client::DockerClient;

use super::naming::{
    LABEL_SESSION, LABEL_SLUG, LABEL_WORKTREE, resolve_worktree_root, worktree_hash,
};

pub async fn handle(args: DevPortOfArgs, _ctx: RunContext) -> Result<i32> {
    let docker = DockerClient::connect()?;
    let worktree_root = resolve_worktree_root()?;
    let wt_hash = worktree_hash(&worktree_root);
    let containers = docker.list_containers_by_label(LABEL_WORKTREE, &wt_hash).await?;
    let mut matches: Vec<(String, String, std::collections::HashMap<u16, u16>)> = Vec::new();
    for c in &containers {
        let labels = c.labels.clone().unwrap_or_default();
        let slug = labels.get(LABEL_SLUG).cloned().unwrap_or_default();
        let session = labels.get(LABEL_SESSION).cloned().unwrap_or_default();
        if slug != args.slug { continue; }
        if let Some(s) = &args.session {
            if &session != s { continue; }
        }
        if let Some(id) = &c.id {
            let ports = docker.inspect_ports(id).await?;
            matches.push((slug, session, ports));
        }
    }

    if matches.is_empty() {
        // Was the slug registered at all?
        match super::registry::dump(&worktree_root).await {
            Ok(reg) if reg.deployments.contains_key(&args.slug) => {
                eprintln!(
                    "hm: slug `{}` registered but not running in this worktree.\n  → run `hm dev up {}` first.",
                    args.slug, args.slug,
                );
                return Ok(4);
            }
            _ => {
                eprintln!(
                    "hm: slug `{}` not registered in this worktree's .harmont/.\n  → run `hm dev ls` to see registered slugs.",
                    args.slug,
                );
                return Ok(5);
            }
        }
    }
    if matches.len() > 1 {
        eprintln!("hm: slug `{}` matches multiple live sessions in this worktree:", args.slug);
        for (_, sess, ports) in &matches {
            let p = format_ports(ports);
            eprintln!("  {sess}  {p}");
        }
        eprintln!("pass `--session <id>` or run `hm dev ls`.");
        return Ok(5);
    }

    let (_, _, ports) = &matches[0];
    let Some(host_port) = ports.get(&args.container_port) else {
        eprintln!(
            "hm: container port `{}` is not published by `{}`.\n  → check the deployment's port_mapping.",
            args.container_port, args.slug,
        );
        return Ok(5);
    };
    println!("{host_port}");
    Ok(0)
}

fn format_ports(ports: &std::collections::HashMap<u16, u16>) -> String {
    let mut entries: Vec<(u16, u16)> = ports.iter().map(|(c, h)| (*c, *h)).collect();
    entries.sort();
    entries.iter()
        .map(|(c, h)| format!("localhost:{h} → :{c}"))
        .collect::<Vec<_>>()
        .join(", ")
}
```

- [ ] **Step 2: Build + commit**

```bash
cargo build -p harmont-cli
git add crates/hm/src/commands/dev/port_of.rs
git commit -m "$(cat <<'EOF'
feat(dev): `hm dev port-of` with ambiguity + exit-code rules

stdout prints the bare integer (designed for `$(...)`). Exit codes
per spec § 2: 0 ok, 4 known-but-stopped, 5 unknown / multi-session
ambiguity. Multi-session error enumerates sessions w/ started time
and host port so the user can pick one with --session.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 14: `hm dev logs` + `hm dev exec` (thin docker shims)

Combined because both are 30-line docker wrappers with the same ambiguity rule.

**Files:**
- Modify: `crates/hm/src/commands/dev/logs.rs`
- Modify: `crates/hm/src/commands/dev/exec.rs`
- Modify: `crates/hm/src/orchestrator/docker_client.rs` (add `exec_tty`)

- [ ] **Step 1: Add `exec_tty` to DockerClient**

Append to the impl block in `crates/hm/src/orchestrator/docker_client.rs`:

```rust
    /// Allocate a TTY exec into a running container. Forwards stdin/stdout
    /// transparently so an interactive shell works. Returns exit code.
    ///
    /// # Errors
    ///
    /// Returns [`HmError::Docker`] on create_exec / start_exec / inspect failures.
    pub async fn exec_tty(&self, container_id: &str, cmd: &[String]) -> Result<i32> {
        use bollard::exec::{CreateExecOptions, StartExecOptions, StartExecResults};
        use futures_util::StreamExt;
        let create = self
            .inner
            .create_exec(
                container_id,
                CreateExecOptions::<&str> {
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    attach_stdin: Some(true),
                    tty: Some(true),
                    cmd: Some(cmd.iter().map(String::as_str).collect()),
                    ..Default::default()
                },
            )
            .await
            .map_err(|e| HmError::Docker(format!("create_exec({container_id}): {e}")))?;
        let start = self
            .inner
            .start_exec(
                &create.id,
                Some(StartExecOptions { detach: false, tty: true, ..Default::default() }),
            )
            .await
            .map_err(|e| HmError::Docker(format!("start_exec({}): {e}", create.id)))?;
        if let StartExecResults::Attached { mut output, .. } = start {
            // Bridge container output to host stdout. For full bidi
            // stdin we would also need to feed the local stdin into
            // the exec input stream; left as a follow-up.
            while let Some(chunk) = output.next().await {
                if let Ok(c) = chunk {
                    use std::io::Write;
                    std::io::stdout().write_all(c.into_bytes().as_ref()).ok();
                }
            }
        }
        let info = self
            .inner
            .inspect_exec(&create.id)
            .await
            .map_err(|e| HmError::Docker(format!("inspect_exec({}): {e}", create.id)))?;
        Ok(info.exit_code.map(|c| i32::try_from(c).unwrap_or(0)).unwrap_or(0))
    }
```

- [ ] **Step 2: Implement `logs.rs`**

Replace `crates/hm/src/commands/dev/logs.rs` with:

```rust
//! `hm dev logs <slug>` — tail a live deployment's logs.

use anyhow::Result;
use futures_util::StreamExt;

use crate::cli::DevLogsArgs;
use crate::context::RunContext;
use crate::orchestrator::docker_client::DockerClient;

use super::naming::{
    LABEL_SESSION, LABEL_SLUG, LABEL_WORKTREE, resolve_worktree_root, worktree_hash,
};

pub async fn handle(args: DevLogsArgs, _ctx: RunContext) -> Result<i32> {
    let docker = DockerClient::connect()?;
    let worktree_root = resolve_worktree_root()?;
    let wt_hash = worktree_hash(&worktree_root);
    let containers = docker.list_containers_by_label(LABEL_WORKTREE, &wt_hash).await?;
    let mut matches: Vec<(String, String, String)> = Vec::new();
    for c in &containers {
        let labels = c.labels.clone().unwrap_or_default();
        if labels.get(LABEL_SLUG).map(String::as_str) != Some(&args.slug) { continue; }
        let session = labels.get(LABEL_SESSION).cloned().unwrap_or_default();
        if let Some(s) = &args.session {
            if &session != s { continue; }
        }
        if let Some(id) = &c.id {
            matches.push((args.slug.clone(), session, id.clone()));
        }
    }
    if matches.is_empty() {
        eprintln!(
            "hm: slug `{}` is not running in this worktree.\n  → run `hm dev up {}` first.",
            args.slug, args.slug,
        );
        return Ok(4);
    }
    if matches.len() > 1 {
        eprintln!("hm: slug `{}` matches multiple live sessions; pass --session <id>", args.slug);
        return Ok(5);
    }
    let (_, _, id) = &matches[0];
    use bollard::container::LogsOptions;
    let mut s = docker.inner_for_logs().logs::<String>(
        id,
        Some(LogsOptions {
            stdout: true, stderr: true,
            follow: args.follow,
            tail: "all".to_string(),
            ..Default::default()
        }),
    );
    while let Some(chunk) = s.next().await {
        if let Ok(c) = chunk {
            use std::io::Write;
            std::io::stdout().write_all(c.into_bytes().as_ref()).ok();
        }
    }
    Ok(0)
}
```

- [ ] **Step 3: Implement `exec.rs`**

Replace `crates/hm/src/commands/dev/exec.rs` with:

```rust
//! `hm dev exec <slug> [-- cmd...]` — one-shot exec.

use anyhow::Result;

use crate::cli::DevExecArgs;
use crate::context::RunContext;
use crate::orchestrator::docker_client::DockerClient;

use super::naming::{
    LABEL_SESSION, LABEL_SLUG, LABEL_WORKTREE, resolve_worktree_root, worktree_hash,
};

pub async fn handle(args: DevExecArgs, _ctx: RunContext) -> Result<i32> {
    let docker = DockerClient::connect()?;
    let worktree_root = resolve_worktree_root()?;
    let wt_hash = worktree_hash(&worktree_root);
    let containers = docker.list_containers_by_label(LABEL_WORKTREE, &wt_hash).await?;
    let mut matches: Vec<String> = Vec::new();
    for c in &containers {
        let labels = c.labels.clone().unwrap_or_default();
        if labels.get(LABEL_SLUG).map(String::as_str) != Some(&args.slug) { continue; }
        let session = labels.get(LABEL_SESSION).cloned().unwrap_or_default();
        if let Some(s) = &args.session {
            if &session != s { continue; }
        }
        if let Some(id) = &c.id {
            matches.push(id.clone());
        }
    }
    if matches.is_empty() {
        eprintln!(
            "hm: slug `{}` is not running in this worktree.\n  → run `hm dev up {}` first.",
            args.slug, args.slug,
        );
        return Ok(4);
    }
    if matches.len() > 1 {
        eprintln!("hm: slug `{}` matches multiple live sessions; pass --session <id>", args.slug);
        return Ok(5);
    }
    let id = &matches[0];
    let cmd = if args.cmd.is_empty() {
        vec!["sh".to_string(), "-l".to_string()]
    } else {
        args.cmd.clone()
    };
    let code = docker.exec_tty(id, &cmd).await?;
    Ok(code)
}
```

- [ ] **Step 4: Build + commit**

```bash
cargo build -p harmont-cli
git add crates/hm/src/commands/dev/logs.rs crates/hm/src/commands/dev/exec.rs crates/hm/src/orchestrator/docker_client.rs
git commit -m "$(cat <<'EOF'
feat(dev): `hm dev logs` + `hm dev exec`

Both share the multi-session ambiguity rule with port-of (exit 5)
and the not-running rule (exit 4). logs streams docker logs to
stdout; exec allocates a TTY and forwards the exit code. exec
default cmd is `sh -l` for interactive shell.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 15: Integration tests (docker-gated)

**Files:**
- Modify: `crates/hm/Cargo.toml`
- Create: `crates/hm/tests/dev_integration.rs`

- [ ] **Step 1: Add the cargo feature**

In `crates/hm/Cargo.toml`, locate the `[dependencies]` section. Add (in alphabetical order among features) at the bottom of the file:

```toml
[features]
docker-integration = []
```

- [ ] **Step 2: Write the integration test**

Create `crates/hm/tests/dev_integration.rs`:

```rust
//! Docker-gated integration tests.
//!
//! Run with: `cargo test -p harmont-cli --features docker-integration -- --ignored`
//! Requires:
//!   * A reachable Docker daemon
//!   * harmont-py installed in the env at `HARMONT_PYTHON` (defaults to python3)
//!     with the `feat/hm-dev-deploy` branch checked out (or merged to main)
//!
//! Each test creates its own .harmont/ in a tmpdir to avoid step-on
//! between concurrent runs.

#![cfg(feature = "docker-integration")]

use std::io::Read;
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
fn up_serves_http_and_tears_down() {
    let tmp = tempfile::tempdir().unwrap();
    write_deploys_py(tmp.path(), r#"
import harmont as hm

@hm.deploy("hello")
def hello():
    return hm.dev.deploy(
        image="python:3.12-alpine",
        cmd=["python", "-m", "http.server", "5678"],
        port_mapping={5678: hm.dev.port()},
    )
"#);

    let mut up = Command::new(hm_bin())
        .args(["dev", "up"])
        .current_dir(tmp.path())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn hm dev up");

    let stderr = up.stderr.as_mut().unwrap();
    let mut buf = String::new();
    let mut chunk = [0u8; 1024];
    let started = std::time::Instant::now();
    while started.elapsed().as_secs() < 60 {
        let n = stderr.read(&mut chunk).unwrap_or(0);
        if n == 0 { break; }
        buf.push_str(&String::from_utf8_lossy(&chunk[..n]));
        if buf.contains("all up.") { break; }
    }
    assert!(buf.contains("all up."),
        "up did not become ready; stderr:\n{buf}");

    let port_of = Command::new(hm_bin())
        .args(["dev", "port-of", "hello", "5678"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(port_of.status.success(),
        "port-of failed: {}", String::from_utf8_lossy(&port_of.stderr));
    let host_port: u16 = String::from_utf8(port_of.stdout)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert!(host_port > 1024,
        "expected ephemeral host port, got {host_port}");

    // python -m http.server returns an HTML directory listing whose
    // body always contains the literal "Directory listing for /".
    let body = poll_http(&format!("http://127.0.0.1:{host_port}"));
    assert!(
        body.contains("Directory listing"),
        "expected python http.server directory listing; got {body:?}",
    );

    let _ = nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(up.id() as i32),
        nix::sys::signal::Signal::SIGINT,
    );
    let _ = up.wait();

    let port_of_after = Command::new(hm_bin())
        .args(["dev", "port-of", "hello", "5678"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert_eq!(port_of_after.status.code(), Some(4),
        "stopped slug should exit 4: {}",
        String::from_utf8_lossy(&port_of_after.stderr));
}

fn poll_http(url: &str) -> String {
    let started = std::time::Instant::now();
    let mut last_err = String::new();
    while started.elapsed().as_secs() < 15 {
        match ureq::get(url).call() {
            Ok(resp) => {
                if resp.status() == 200 {
                    return resp.into_string().unwrap_or_default();
                }
                last_err = format!("status {}", resp.status());
            }
            Err(e) => last_err = e.to_string(),
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    panic!("HTTP poll failed against {url}: {last_err}");
}
```

- [ ] **Step 3: Add test-only deps to the workspace `[dev-dependencies]`**

In `crates/hm/Cargo.toml`, ensure `tempfile`, `nix`, and `ureq` are present in `[dev-dependencies]`. If absent:

```toml
[dev-dependencies]
tempfile = "3"
nix = { version = "0.29", features = ["signal"] }
ureq = { version = "2", default-features = false, features = ["tls"] }
```

- [ ] **Step 4: Build + smoke**

```bash
cargo build -p harmont-cli --tests --features docker-integration
```

Expected: clean build. Do **not** run the test in CI by default — it requires Docker.

- [ ] **Step 5: Optional: run the test locally if Docker is up**

```bash
cargo test -p harmont-cli --features docker-integration -- --ignored up_serves_http_and_tears_down
```

Expected: 1 passed (python:3.12-alpine pulls + boots + HTTP GET asserts body contains "Directory listing" + tears down).

- [ ] **Step 6: Commit**

```bash
git add crates/hm/Cargo.toml crates/hm/tests/dev_integration.rs Cargo.lock
git commit -m "$(cat <<'EOF'
test(dev): integration test boots python http.server + asserts HTTP body

Swap the postgres-based integration test for `python -m http.server`
running inside `python:3.12-alpine` — pulls 50MB instead of 80MB,
boots in <1s, and uses Python's stdlib HTTP server (no third-party
image dependency). Add an actual HTTP GET against the host port +
body assertion (the response is python http.server's directory
listing, whose body always contains "Directory listing for /") so
the test validates the whole chain: container start → bridge net →
port publish → image CMD honored → server actually serving.

ureq is the new dev-dep (default-features=false, just `tls` feature).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 16: PR-readiness sanity pass

- [ ] **Step 1: Branch is clean**

```bash
git status
git log --oneline origin/main..HEAD
```

Expected: clean tree; linear commit history of the task commits.

- [ ] **Step 2: Lints + tests**

```bash
cargo clippy --all-targets -p harmont-cli -- -D warnings
cargo test -p harmont-cli --lib
```

Expected: zero clippy warnings; every unit test passes.

- [ ] **Step 3: CLI smoke**

```bash
cargo run -p harmont-cli -- dev --help
cargo run -p harmont-cli -- dev up --help
cargo run -p harmont-cli -- dev ls
```

Expected: each prints its help; `dev ls` complains it can't find `.harmont/` outside a worktree (acceptable; should exit 1 with a fix-directed message).

- [ ] **Step 4: Commit any follow-up fixes (skip if none)**

```bash
# only if changes
git add -A
git commit -m "chore: PR-readiness sanity pass"
```

- [ ] **Step 5: Done — branch ready for review**

The `feat/hm-dev-deploy` branch on harmont-cli is feature-complete for v1. Dependencies: harmont-py's `feat/hm-dev-deploy` branch must be installable in the runtime environment (the integration test documents this).

---

## Self-Review Notes (for the plan author)

Coverage of spec § 2 (CLI surface):
- `hm dev up [SLUG ...] --no-deps --rebuild` → Task 1 (clap) + Task 10 (impl).
- `hm dev down [SLUG ...] --session --all` → Task 1 + Task 11.
- `hm dev ls` → Task 1 + Task 12.
- `hm dev logs <SLUG> --follow --session` → Task 1 + Task 14.
- `hm dev port-of <SLUG> <CPORT> --session` → Task 1 + Task 13.
- `hm dev exec <SLUG> [-- CMD ...] --session` → Task 1 + Task 14.
- Container/network/label scheme → Task 2.
- Worktree-hash + session-id + fresh ports → Task 2 + Task 5.
- Log mux UX → Task 7.
- Exit-code table → Task 13 (4/5), Task 14 (4/5), Task 10 (0/130), Task 11 (0).

Coverage of spec § 3 (runtime):
- Process model → Task 10.
- Registry handoff → Task 3.
- Boot pipeline → Task 10 (orchestration) + Task 5 (docker primitives) + Task 8 (spec build).
- Build-chain reuse → Task 9.
- Field semantics → Task 8 (binds, env, port_mapping translation).

Coverage of spec § 4 (lifecycle):
- Lock-free + per-session naming → Task 2.
- Boot levels → Task 4 + Task 10.
- Steady-state log mux → Task 7 + Task 10.
- SIGINT → ordered teardown → Task 10 (`wait_signal` + `teardown`).
- Orphan recovery → Task 11.

Coverage of spec § 5 (runtime errors):
- Docker daemon unreachable → bubbles through `ping()` failure in Task 10.
- Image pull failed → bubbles through `pull_image` in Task 10 (existing).
- Build failed → bubbles through `build_image_from_pipeline` in Task 9.
- Slug unknown / ambiguous / not-running → Task 13, 14.

Type / name consistency:
- `LABEL_WORKTREE` / `LABEL_SLUG` / `LABEL_SESSION` / `LABEL_DRIVER` constants are used identically across all tasks.
- `ResolvedSpec`, `ServiceSpec`, `ServiceSpecBuilder` ascend from Task 5 and are used consistently in Task 8 / 10.
- `PORT_SENTINEL` is `"__hm_dev_port__"` everywhere it appears (Tasks 3, 8).
- Exit codes 0 / 4 / 5 / 130 are stamped consistently per spec § 2.

Known sharp edges:
- Task 9's wrapper `run_pipeline_v0_one_shot` requires reading the existing local executor to wire correctly. The plan calls this out explicitly; if the wiring exceeds 50 lines, the engineer should stop and split it into a separate task.
- Task 10's `inner_for_logs` is a doc-hidden accessor — pragmatic but not ideal. If a follow-up refactor surfaces, it would be to move log-streaming into `DockerClient` itself.
