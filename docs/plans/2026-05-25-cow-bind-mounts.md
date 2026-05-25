# COW Bind-Mount Workspace Injection

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Eliminate per-step tar extraction overhead by bind-mounting COW workspace clones into Docker containers, cutting step startup from ~10-15s to <1s.

**Architecture:** Instead of piping a tar.gz archive into each container via `docker exec`, we unpack the workspace once on the host, create a COW (copy-on-write) clone per chain, and bind-mount each clone into the container. Steps within a chain reuse the same container (just `docker exec` successive commands). This preserves the existing Docker-image-based caching for toolchain steps while making leaf steps (lint, fmt, test) near-instant.

**Tech Stack:** Rust, Bollard 0.18, APFS `clonefile(2)` on macOS, `reflink` or `cp -a` on Linux, Docker bind mounts via `HostConfig.binds`.

---

## Context & Problem

Current per-step overhead breakdown:
| Phase | Cost |
|-------|------|
| `create_container` + `start` | ~200ms |
| Pipe workspace tar.gz → shell extract | **5-10s** |
| `docker exec` the actual command | varies |
| `docker commit` | ~1-2s |
| `stop_remove` | ~200ms |

A `cargo fmt --check` that takes <1s outside Docker takes 15-20s inside because of the extraction overhead. This repeats for EVERY step, even parallel leaf steps that all need the same source files.

## Design Decisions

1. **Bind-mount mode is local-only.** CI and cloud runs continue using archive mode (tar extraction + full Docker commit) because (a) cache images must capture workspace-resident artifacts for cold-start scenarios and (b) CI runners have no persistent host filesystem.

2. **Container reuse per chain.** Steps connected by `BuildsIn` edges share a container — the first step creates it, subsequent steps `docker exec` into it. This eliminates per-step container lifecycle AND per-step extraction.

3. **COW clones for parallel isolation.** Each chain gets its own copy-on-write clone of the workspace. On macOS (APFS), `clonefile(2)` is instant (<1ms for any size). On Linux, we use `cp --reflink=auto` (instant on btrfs/xfs, falls back to regular copy on ext4).

4. **Cached parent images still work as boot images.** A cached `rustup` image provides system-level toolchain. The bind-mount overlays fresh source at `/workspace`. Workspace-resident artifacts (like `target/`) from the cached image are hidden by the mount — this is acceptable because (a) toolchain steps that produce them will re-run in bind-mount mode, and (b) the warm cache artifacts build up within the chain via the shared mount.

5. **No Docker commit in bind-mount mode.** Steps don't commit their container state. Caching is purely for image-exists checks that short-circuit execution. The user's workflow: first run populates caches (archive mode or naturally), subsequent local dev runs use bind-mount for speed.

---

## Task 1: Platform COW Clone Utility

**Files:**
- Create: `crates/hm-util/src/cow_clone.rs`
- Modify: `crates/hm-util/src/lib.rs`
- Test: inline `#[cfg(test)]` in `cow_clone.rs`

**Step 1: Write the failing test**

```rust
// crates/hm-util/src/cow_clone.rs
#[cfg(test)]
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

        assert_eq!(fs::read_to_string(dst_path.join("hello.txt")).unwrap(), "world");
        assert_eq!(fs::read_to_string(dst_path.join("sub/nested.txt")).unwrap(), "deep");

        // Writes to clone don't affect source
        fs::write(dst_path.join("hello.txt"), "modified").unwrap();
        assert_eq!(fs::read_to_string(src.path().join("hello.txt")).unwrap(), "world");
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p hm-util cow_clone -- --nocapture`
Expected: FAIL — module doesn't exist yet.

**Step 3: Write minimal implementation**

```rust
// crates/hm-util/src/cow_clone.rs

use std::path::Path;
use anyhow::{Context, Result};

/// Create a copy-on-write clone of `src` at `dst`.
///
/// On macOS (APFS), uses `clonefile(2)` via `cp -c` for instant O(1) clones.
/// On Linux, uses `cp --reflink=auto` (instant on btrfs/xfs, regular copy on ext4).
/// Falls back to a recursive copy if platform-specific methods fail.
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
    // Fallback: regular recursive copy
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
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p hm-util cow_clone -- --nocapture`
Expected: PASS

**Step 5: Expose from lib.rs**

Add `pub mod cow_clone;` to `crates/hm-util/src/lib.rs`.

**Step 6: Commit**

```bash
git add crates/hm-util/src/cow_clone.rs crates/hm-util/src/lib.rs
git commit -m "feat: add platform COW clone utility (APFS clonefile / reflink)"
```

---

## Task 2: Add Bind-Mount Support to DockerClient

**Files:**
- Modify: `crates/hm/src/orchestrator/docker_client.rs:132-162`
- Test: inline test or integration test

**Step 1: Write the failing test**

```rust
// At bottom of docker_client.rs, in #[cfg(test)] mod
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn container_options_include_bind_when_specified() {
        let opts = ContainerOpts {
            image: "alpine:latest".into(),
            env: vec![],
            workdir: "/workspace".into(),
            name: "test-container".into(),
            binds: vec!["/tmp/src:/workspace:rw".into()],
        };
        let (cfg, host_cfg) = opts.into_docker_config();
        assert_eq!(cfg.working_dir, Some("/workspace".into()));
        assert_eq!(
            host_cfg.binds,
            Some(vec!["/tmp/src:/workspace:rw".into()])
        );
    }

    #[test]
    fn container_options_no_bind_when_empty() {
        let opts = ContainerOpts {
            image: "alpine:latest".into(),
            env: vec![],
            workdir: "/workspace".into(),
            name: "test-container".into(),
            binds: vec![],
        };
        let (_, host_cfg) = opts.into_docker_config();
        assert_eq!(host_cfg.binds, None);
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p harmont-cli docker_client::tests -- --nocapture`
Expected: FAIL — `ContainerOpts` doesn't exist.

**Step 3: Implement ContainerOpts and refactor start_long_lived**

```rust
// In docker_client.rs, add above start_long_lived:

use bollard::models::HostConfig;

/// Options for creating a long-lived container.
pub struct ContainerOpts {
    pub image: String,
    pub env: Vec<String>,
    pub workdir: String,
    pub name: String,
    /// Bind mounts in Docker format: `"/host/path:/container/path:rw"`.
    /// Empty vec means no bind mounts.
    pub binds: Vec<String>,
}

impl ContainerOpts {
    fn into_docker_config(self) -> (Config<String>, HostConfig) {
        let host_config = HostConfig {
            binds: if self.binds.is_empty() {
                None
            } else {
                Some(self.binds)
            },
            ..Default::default()
        };
        let cfg = Config {
            image: Some(self.image),
            cmd: Some(vec!["sh".into(), "-c".into(), "sleep infinity".into()]),
            env: Some(self.env),
            working_dir: Some(self.workdir),
            host_config: Some(host_config.clone()),
            ..Default::default()
        };
        (cfg, host_config)
    }
}
```

Then refactor `start_long_lived` to use `ContainerOpts`:

```rust
pub async fn start_long_lived(&self, opts: ContainerOpts) -> Result<String> {
    let name = opts.name.clone();
    let (cfg, _) = opts.into_docker_config();
    let create = self
        .inner
        .create_container(
            Some(CreateContainerOptions {
                name: &name,
                ..Default::default()
            }),
            cfg,
        )
        .await
        .map_err(|e| HmError::Docker(format!("create_container: {e}")))?;
    self.inner
        .start_container(&create.id, None::<StartContainerOptions<String>>)
        .await
        .map_err(|e| HmError::Docker(format!("start_container: {e}")))?;
    Ok(create.id)
}
```

**Step 4: Update the single caller in `runner/docker.rs`**

Change line 121-125 from:
```rust
let cid = ctx.docker
    .start_long_lived(&image, &env_vec, &input.workdir, &container_name)
    .await
```
to:
```rust
use crate::orchestrator::docker_client::ContainerOpts;

let cid = ctx.docker
    .start_long_lived(ContainerOpts {
        image: image.clone(),
        env: env_vec.clone(),
        workdir: input.workdir.clone(),
        name: container_name,
        binds: vec![], // archive mode: no bind mounts
    })
    .await
```

**Step 5: Run tests**

Run: `cargo test -p harmont-cli -- --nocapture`
Expected: All pass (behavior unchanged, just refactored).

**Step 6: Commit**

```bash
git add crates/hm/src/orchestrator/docker_client.rs crates/hm/src/runner/docker.rs
git commit -m "refactor: extract ContainerOpts with bind-mount support"
```

---

## Task 3: Add `upload_to_container` to DockerClient

**Files:**
- Modify: `crates/hm/src/orchestrator/docker_client.rs`

This replaces the slow `exec_streaming_stdin` tar extraction with Docker's native PUT archive API.

**Step 1: Write the method**

```rust
// In docker_client.rs, add:

use bollard::container::UploadToContainerOptions;
use bytes::Bytes;

/// Upload a tar archive directly into a running container at `path`.
/// Uses Docker's PUT /containers/{id}/archive endpoint — no shell
/// process or stdin pipe needed.
pub async fn upload_archive(&self, container_id: &str, path: &str, tar: &[u8]) -> Result<()> {
    let opts = UploadToContainerOptions {
        path: path.to_string(),
        ..Default::default()
    };
    self.inner
        .upload_to_container(container_id, Some(opts), Bytes::copy_from_slice(tar))
        .await
        .map_err(|e| HmError::Docker(format!("upload_to_container: {e}")))?;
    Ok(())
}
```

**Step 2: Verify compile**

Run: `cargo check -p harmont-cli`
Expected: compiles (no callers yet, that's Task 5).

**Step 3: Commit**

```bash
git add crates/hm/src/orchestrator/docker_client.rs
git commit -m "feat: add upload_archive using Docker PUT archive API"
```

---

## Task 4: Workspace Preparation in Scheduler

**Files:**
- Create: `crates/hm/src/orchestrator/workspace.rs`
- Modify: `crates/hm/src/orchestrator/mod.rs`
- Modify: `crates/hm/src/orchestrator/scheduler.rs`

This task adds a `WorkspaceManager` that handles both modes: archive (existing) and bind-mount (new).

**Step 1: Write the failing test**

```rust
// crates/hm/src/orchestrator/workspace.rs

#[cfg(test)]
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
    fn archive_mode_returns_archive_id() {
        let src = TempDir::new().unwrap();
        fs::write(src.path().join("main.rs"), "fn main() {}").unwrap();

        let archives = Arc::new(super::super::archive::ArchiveStore::new());
        let mgr = WorkspaceManager::archive(src.path(), archives.clone()).unwrap();

        assert!(mgr.archive_id().is_some());
        assert!(mgr.clone_for_chain(0).is_err()); // not in bind-mount mode
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p harmont-cli workspace -- --nocapture`
Expected: FAIL — module doesn't exist.

**Step 3: Implement WorkspaceManager**

```rust
// crates/hm/src/orchestrator/workspace.rs

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{Context, Result, bail};
use hm_plugin_protocol::ArchiveId;

use super::archive::ArchiveStore;
use super::source::build_archive_bytes;

use std::sync::Arc;

/// Manages workspace provisioning for pipeline steps.
///
/// Two modes:
/// - **Archive**: builds a tar.gz once, each step extracts it in-container (current behavior).
/// - **Bind-mount**: unpacks workspace to a temp dir on host, creates COW clones per chain.
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
        temp_dir: tempfile::TempDir,
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
    /// Creates a temp dir for chain clones.
    pub fn bind_mount(repo_root: PathBuf) -> Result<Self> {
        let temp_dir = tempfile::Builder::new()
            .prefix("hm-workspace-")
            .tempdir()
            .context("create workspace temp dir")?;
        Ok(Self {
            mode: WorkspaceMode::BindMount {
                base_dir: repo_root,
                clones: Mutex::new(HashMap::new()),
                temp_dir,
            },
        })
    }

    /// Get the archive ID (only valid in archive mode).
    pub fn archive_id(&self) -> Option<ArchiveId> {
        match &self.mode {
            WorkspaceMode::Archive { archive_id } => Some(*archive_id),
            WorkspaceMode::BindMount { .. } => None,
        }
    }

    /// Is this manager in bind-mount mode?
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
                temp_dir,
            } => {
                let mut map = clones.lock().unwrap();
                if let Some(path) = map.get(&chain_id) {
                    return Ok(path.clone());
                }
                let clone_path = temp_dir.path().join(format!("chain-{chain_id}"));
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
```

**Step 4: Expose from orchestrator mod**

Add `pub mod workspace;` to `crates/hm/src/orchestrator/mod.rs`.

**Step 5: Run tests**

Run: `cargo test -p harmont-cli workspace -- --nocapture`
Expected: PASS

**Step 6: Commit**

```bash
git add crates/hm/src/orchestrator/workspace.rs crates/hm/src/orchestrator/mod.rs
git commit -m "feat: add WorkspaceManager with archive and bind-mount modes"
```

---

## Task 5: Refactor DockerRunner to Support Both Modes

**Files:**
- Modify: `crates/hm/src/runner/docker.rs`
- Modify: `crates/hm/src/runner/mod.rs`

This is the core change: the runner checks if a bind-mount path is provided and either (a) skips extraction and uses the mount, or (b) uses the existing archive extraction (with the faster `upload_archive` API).

**Step 1: Add workspace host path to ExecutorInput**

In `crates/hm-plugin-protocol/src/lib.rs` (or wherever `ExecutorInput` is defined), add a field:

```rust
// In ExecutorInput struct:
/// If set, the workspace is bind-mounted from this host path.
/// The runner should skip archive extraction.
pub workspace_host_path: Option<std::path::PathBuf>,
```

Check where `ExecutorInput` is defined:

Run: `grep -rn "pub struct ExecutorInput" crates/`

**Step 2: Modify the runner to branch on bind-mount**

In `crates/hm/src/runner/docker.rs`, replace the `run_in_container` function body. The key change:

```rust
async fn run_in_container(
    ctx: &RunContext,
    cid: &str,
    input: &ExecutorInput,
    env_vec: &[String],
    plan: &DecisionPlan,
) -> Result<StepResult> {
    // --- Workspace injection ---
    if input.workspace_host_path.is_none() {
        // Archive mode: upload tar into container (faster than exec_streaming_stdin)
        let archive = ctx.archives.read(input.workspace_archive_id, 0, u64::MAX);
        if archive.is_empty() {
            anyhow::bail!("archive {} is empty or unknown", input.workspace_archive_id);
        }
        ctx.docker
            .upload_archive(cid, &input.workdir, &archive)
            .await
            .context("workspace upload failed")?;
    }
    // Bind-mount mode: workspace already mounted, nothing to do.

    // --- Exec step command ---
    let mut writer = StepLogWriter::new(input.step_id, Arc::clone(&ctx.event_bus));
    let docker = ctx.docker.clone();
    let cancel = ctx.cancel.clone();
    let cid_owned = cid.to_owned();
    let cmd = vec!["sh".into(), "-c".into(), input.step.cmd.clone()];
    let workdir = input.workdir.clone();
    let env_owned = env_vec.to_vec();
    let exec_fut = async move {
        let rc = docker
            .exec_streaming(&cid_owned, &cmd, &env_owned, &workdir, &mut writer)
            .await?;
        writer.flush_remaining();
        Ok::<i64, anyhow::Error>(rc)
    };

    let rc = tokio::select! {
        result = exec_fut => result.context("docker exec failed")?,
        () = ctx.cancel.cancelled() => {
            return Ok(StepResult {
                exit_code: 130,
                committed_snapshot: None,
                artifacts: vec![],
            });
        }
    };

    #[allow(clippy::cast_possible_truncation)]
    let exit_code = rc as i32;

    // --- Commit snapshot on success (skip in bind-mount mode) ---
    let committed = if exit_code == 0 && input.workspace_host_path.is_none() {
        // Only commit in archive mode (bind-mount changes are on host)
        let target_tag = plan.commit_to.clone().unwrap_or_else(|| {
            // ... existing ephemeral tag logic ...
        });
        let snap = ctx.docker.commit_container(cid, &target_tag).await?;
        Some(SnapshotRef(snap))
    } else {
        None
    };

    Ok(StepResult {
        exit_code,
        committed_snapshot: committed,
        artifacts: vec![],
    })
}
```

**Step 3: Update container creation to include bind mount**

In `run_step()`, when creating the container:

```rust
let binds = input.workspace_host_path.as_ref().map_or_else(
    || vec![],
    |host_path| vec![format!("{}:{}:rw", host_path.display(), input.workdir)],
);

let cid = ctx.docker
    .start_long_lived(ContainerOpts {
        image: image.clone(),
        env: env_vec.clone(),
        workdir: input.workdir.clone(),
        name: container_name,
        binds,
    })
    .await
    .context("docker start failed")?;
```

**Step 4: Run full test suite**

Run: `cargo test -p harmont-cli -- --nocapture`
Expected: All existing tests pass (they use archive mode with `workspace_host_path: None`).

**Step 5: Commit**

```bash
git add crates/hm/src/runner/docker.rs crates/hm/src/runner/mod.rs crates/hm-plugin-protocol/
git commit -m "feat: DockerRunner supports bind-mount mode (skip extraction)"
```

---

## Task 6: Wire Bind-Mount Mode Through Scheduler

**Files:**
- Modify: `crates/hm/src/orchestrator/scheduler.rs`
- Modify: `crates/hm/src/runner/mod.rs` (add workspace manager to RunContext)

**Step 1: Add WorkspaceManager to RunContext**

```rust
// In runner/mod.rs, modify RunContext:
pub struct RunContext {
    pub docker: DockerClient,
    pub event_bus: Arc<EventBus>,
    pub archives: Arc<ArchiveStore>,
    pub cancel: CancellationToken,
    pub workspace: Arc<WorkspaceManager>,  // NEW
}
```

**Step 2: Modify scheduler::run() to accept a mode flag**

Add a `bind_mount: bool` parameter to `scheduler::run()`. When true:
- Create `WorkspaceManager::bind_mount(repo_root)`
- Pass chain_id through to `execute_step`
- In `execute_step`, call `workspace.clone_for_chain(chain_id)` and set `input.workspace_host_path`

```rust
// In execute_step, before building ExecutorInput:
let workspace_host_path = if run_ctx.workspace.is_bind_mount() {
    Some(run_ctx.workspace.clone_for_chain(chain_id)?)
} else {
    None
};

let input = ExecutorInput {
    step: step_wire,
    workspace_archive_id: archive_id,
    env: env_map,
    workdir: "/workspace".to_string(),
    run_id,
    step_id,
    cache_lookup: decision,
    parent_snapshot,
    workspace_host_path,  // NEW
};
```

**Step 3: Update scheduler::run() signature and initialization**

```rust
pub async fn run(
    graph: PipelineGraph,
    repo_root: PathBuf,
    parallelism: usize,
    runner_registry: Arc<RunnerRegistry>,
    renderer: Box<dyn OutputRenderer>,
    bind_mount: bool,  // NEW
) -> Result<i32> {
    let archives = Arc::new(ArchiveStore::new());

    let workspace = Arc::new(if bind_mount {
        WorkspaceManager::bind_mount(repo_root.clone())?
    } else {
        WorkspaceManager::archive(&repo_root, archives.clone())?
    });

    // archive_id is only used in archive mode
    let archive_id = workspace.archive_id().unwrap_or(ArchiveId(Uuid::nil()));

    let run_ctx = RunContext {
        docker: docker.clone(),
        event_bus: bus.clone(),
        archives: archives.clone(),
        cancel: cancel.clone(),
        workspace,  // NEW
    };
    // ...
}
```

**Step 4: Update caller in commands/run/local.rs**

Pass `bind_mount` flag (hardcode `false` for now, wired in Task 7):

```rust
orchestrator::run(graph, repo_root, parallelism, registry, renderer, false).await
```

**Step 5: Run test suite**

Run: `cargo test -p harmont-cli -- --nocapture`
Expected: PASS (bind_mount=false preserves existing behavior).

**Step 6: Commit**

```bash
git add crates/hm/src/orchestrator/scheduler.rs crates/hm/src/runner/mod.rs crates/hm/src/commands/run/local.rs
git commit -m "feat: wire WorkspaceManager through scheduler and RunContext"
```

---

## Task 7: CLI Flag and Auto-Detection

**Files:**
- Modify: `crates/hm/src/commands/run/local.rs`
- Modify: `crates/hm/src/cli/run.rs` (or wherever CLI args are defined)

**Step 1: Add `--bind-mount` / `--no-bind-mount` flag**

Find the run command args struct and add:

```rust
/// Mount workspace directly instead of extracting archive into containers.
/// Faster for local development. Default: auto-detect (bind-mount unless
/// HM_NONINTERACTIVE is set).
#[arg(long, default_value_t = false)]
pub bind_mount: bool,

/// Force archive mode (tar extraction into containers). Overrides auto-detection.
#[arg(long, default_value_t = false, conflicts_with = "bind_mount")]
pub no_bind_mount: bool,
```

**Step 2: Auto-detection logic in local.rs**

```rust
let bind_mount = if args.bind_mount {
    true
} else if args.no_bind_mount {
    false
} else {
    // Auto: use bind-mount for local interactive runs
    std::env::var("HM_NONINTERACTIVE").is_err()
};
```

**Step 3: Pass to orchestrator::run()**

```rust
orchestrator::run(graph, repo_root, parallelism, registry, renderer, bind_mount).await
```

**Step 4: Verify**

Run: `cargo build -p harmont-cli && ./target/debug/hm run --help`
Expected: `--bind-mount` and `--no-bind-mount` flags appear in help text.

**Step 5: Commit**

```bash
git add crates/hm/src/commands/run/local.rs crates/hm/src/cli/
git commit -m "feat: add --bind-mount/--no-bind-mount flags with auto-detection"
```

---

## Task 8: Container Reuse Per Chain

**Files:**
- Modify: `crates/hm/src/runner/docker.rs`
- Modify: `crates/hm/src/runner/mod.rs`

This is the second major optimization: steps within a chain reuse the same container instead of creating/destroying one per step. Only applies in bind-mount mode (archive mode needs per-step commits for caching).

**Step 1: Add container pool to RunContext**

```rust
// In runner/mod.rs, add:
use std::collections::HashMap;
use std::sync::Mutex;

/// Pool of reusable containers keyed by chain ID.
/// In bind-mount mode, steps within a chain reuse the same container.
#[derive(Debug, Default)]
pub struct ContainerPool {
    containers: Mutex<HashMap<usize, String>>, // chain_id -> container_id
}

impl ContainerPool {
    pub fn get(&self, chain_id: usize) -> Option<String> {
        self.containers.lock().unwrap().get(&chain_id).cloned()
    }

    pub fn put(&self, chain_id: usize, cid: String) {
        self.containers.lock().unwrap().insert(chain_id, cid);
    }

    pub fn take_all(&self) -> Vec<String> {
        let mut map = self.containers.lock().unwrap();
        map.drain().map(|(_, cid)| cid).collect()
    }
}
```

Add `pub container_pool: Arc<ContainerPool>` to `RunContext`.

**Step 2: Modify DockerRunner to check pool before creating**

In `run_step()`:

```rust
// Check if we can reuse an existing container for this chain
let (cid, created_new) = if let Some(existing) = ctx.container_pool.get(chain_id) {
    (existing, false)
} else {
    let binds = input.workspace_host_path.as_ref().map_or_else(
        || vec![],
        |host_path| vec![format!("{}:{}:rw", host_path.display(), input.workdir)],
    );
    let new_cid = ctx.docker
        .start_long_lived(ContainerOpts { /* ... */ })
        .await?;
    if input.workspace_host_path.is_some() {
        ctx.container_pool.put(chain_id, new_cid.clone());
    }
    (new_cid, true)
};
```

Note: `chain_id` needs to be passed through `ExecutorInput`. Add `pub chain_id: usize` to the struct.

**Step 3: Skip cleanup for reused containers**

Only stop/remove if we created the container AND it's not pooled:

```rust
// At end of run_step:
if input.workspace_host_path.is_none() {
    // Archive mode: always cleanup
    ctx.docker.stop_remove(&cid).await;
}
// Bind-mount mode: container stays alive for next step in chain
```

**Step 4: Cleanup pooled containers at end of run**

In `scheduler::run()`, after all steps complete:

```rust
// Clean up reusable containers
for cid in run_ctx.container_pool.take_all() {
    ctx.docker.stop_remove(&cid).await;
}
```

**Step 5: Run test suite**

Run: `cargo test -p harmont-cli -- --nocapture`
Expected: PASS

**Step 6: Commit**

```bash
git add crates/hm/src/runner/ crates/hm/src/orchestrator/scheduler.rs
git commit -m "feat: container reuse per chain in bind-mount mode"
```

---

## Task 9: Integration Test

**Files:**
- Create: `crates/hm/tests/bind_mount_integration.rs` (or add to existing integration tests)

**Step 1: Write end-to-end test**

```rust
//! Integration test: bind-mount mode runs a pipeline correctly.
//! Requires Docker and the `docker-integration` feature.

#[cfg(feature = "docker-integration")]
#[tokio::test]
async fn bind_mount_pipeline_executes_steps() {
    use tempfile::TempDir;
    use std::fs;

    let workspace = TempDir::new().unwrap();
    fs::write(workspace.path().join("hello.txt"), "world").unwrap();

    // Build a minimal pipeline with 2 chained steps:
    // step1: cat hello.txt (should see bind-mounted file)
    // step2: echo "done" (should reuse container)

    // ... construct PipelineGraph, run with bind_mount=true ...
    // Assert both steps exit 0
    // Assert step2 runs faster than step1 (no extraction)
}
```

**Step 2: Run integration test**

Run: `cargo test -p harmont-cli --features docker-integration bind_mount -- --nocapture --ignored`
Expected: PASS (if Docker is available).

**Step 3: Commit**

```bash
git add crates/hm/tests/
git commit -m "test: integration test for bind-mount pipeline execution"
```

---

## Task 10: Update CI to Use Archive Mode

**Files:**
- Modify: `.github/workflows/ci.yml`

Ensure the dogfood job uses archive mode (no bind-mount) since CI needs consistent caching:

**Step 1: Add --no-bind-mount to dogfood**

```yaml
      - name: hm run ci
        env:
          HM_NONINTERACTIVE: '1'
        run: ./target/debug/hm run ci --no-bind-mount
```

Actually, since `HM_NONINTERACTIVE=1` is already set, auto-detection will choose archive mode. But being explicit is safer:

```yaml
        run: ./target/debug/hm run ci --no-bind-mount
```

**Step 2: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: explicitly use archive mode in dogfood job"
```

---

## Summary of Speed Improvement

**Before (every non-cached step):**
```
create container    ~200ms
tar extraction      ~5-10s    ← ELIMINATED
exec command        varies
docker commit       ~1-2s     ← ELIMINATED (bind-mount mode)
stop/remove         ~200ms    ← ELIMINATED (container reuse)
─────────────────────────────
overhead:           ~7-13s per step
```

**After (bind-mount mode, first step in chain):**
```
create container    ~200ms
(workspace already mounted)
exec command        varies
─────────────────────────────
overhead:           ~200ms
```

**After (bind-mount mode, subsequent steps in chain):**
```
(container already running)
exec command        varies
─────────────────────────────
overhead:           ~0ms
```

Expected wall-clock for the user's pipeline: from ~20s to ~3-4s (actual command time dominates).
