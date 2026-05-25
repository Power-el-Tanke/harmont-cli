# Tear Out Plugin System, Wire Up Static DI

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace the Extism/WASM plugin system with plain Rust traits and static linking — no FFI, no dynamic loading, no WASM cross-compilation.

**Architecture:** Define `StepRunner` and `OutputRenderer` traits in the binary crate. `DockerRunner` implements `StepRunner` by calling `DockerClient` (Bollard) directly — no host-function indirection. Output renderers become direct function calls. Cloud becomes a regular clap subcommand. Everything statically linked.

**Tech Stack:** Rust 2024, tokio, bollard (Docker), clap (CLI), daggy (DAG scheduling). Removing: extism, extism-pdk, wasm32-wasip1 toolchain requirement.

---

## Background

The current codebase on `origin/main` has four WASM plugins embedded via `include_bytes!`:

| Plugin | Role | How it calls the host |
|--------|------|----------------------|
| `hm-plugin-docker` | Default step executor | `extism_host::*` → host fns → `DockerClient` (Bollard) |
| `hm-plugin-output-human` | Stderr build log renderer | `host::write_stderr` |
| `hm-plugin-output-json` | JSON-lines build log | `host::write_stdout` |
| `hm-plugin-cloud` | `hm cloud *` subcommands | HTTP, keyring, TTY, browser via host fns |

Every plugin call goes: scheduler → `PluginRegistry` → `LoadedPlugin::call_capability` → Extism WASM VM → plugin code → `extern "ExtismHost"` host function → real implementation.

**After this refactoring:** scheduler → `DockerRunner::execute()` → `DockerClient` methods. No serialization boundary, no VM, no host functions.

### Crate map (what gets deleted)

```
DELETE  crates/hm-plugin-sdk/          (extism authoring SDK)
DELETE  crates/hm-plugin-docker/       (WASM docker executor)
DELETE  crates/hm-plugin-output-human/ (WASM human formatter)
DELETE  crates/hm-plugin-output-json/  (WASM JSON formatter)
DELETE  crates/hm-plugin-cloud/        (WASM cloud client — migrated in Task 7)
DELETE  crates/hm-fixtures/            (WASM test fixture plugins)
DELETE  crates/hm/build.rs             (WASM cross-compilation)
DELETE  crates/hm/src/plugin/          (host.rs, host_fns.rs, pool.rs, etc.)
```

### Crate map (what stays or gets created)

```
KEEP    crates/hm-pipeline-ir/         (unchanged)
KEEP    crates/hm-util/                (unchanged)
MODIFY  crates/hm-plugin-protocol/     (remove manifest/host-abi types, keep wire types)
MODIFY  crates/hm/                     (new runner module, rewired scheduler+CLI)
CREATE  crates/hm/src/runner/mod.rs    (StepRunner trait, RunnerRegistry)
CREATE  crates/hm/src/runner/docker.rs (DockerRunner implementation)
```

---

### Task 1: Create branch from origin/main

**Files:**
- No file changes — git operations only

**Step 1: Abort the in-progress merge**

```bash
git merge --abort
```

**Step 2: Create fresh branch from origin/main**

```bash
git checkout -b refactor/static-di origin/main
```

**Step 3: Verify clean build**

Run: `cargo check --workspace`
Expected: clean (this is origin/main as-is)

**Step 4: Commit**

No commit needed — clean checkout.

---

### Task 2: Define StepRunner trait and RunnerRegistry

**Files:**
- Create: `crates/hm/src/runner/mod.rs`
- Modify: `crates/hm/src/lib.rs` (add `pub mod runner;`)

This task adds new code alongside the old plugin system. Both compile together — nothing is wired up yet.

**Step 1: Create the runner module**

Create `crates/hm/src/runner/mod.rs`:

```rust
//! Step-runner interface and static registry.
//!
//! Replaces the WASM plugin system with plain Rust traits and
//! compile-time registration.

pub mod docker;

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use hm_plugin_protocol::{BuildEvent, ExecutorInput, StepResult};
use tokio_util::sync::CancellationToken;

use crate::orchestrator::archive::ArchiveStore;
use crate::orchestrator::docker_client::DockerClient;
use crate::orchestrator::events::EventBus;

/// Shared context passed to every runner invocation. Replaces the
/// old host-function global state pattern with explicit DI.
#[derive(Clone)]
pub struct RunContext {
    pub docker: DockerClient,
    pub event_bus: Arc<EventBus>,
    pub archives: Arc<ArchiveStore>,
    pub cancel: CancellationToken,
}

/// A step executor. The scheduler calls `execute` for each pipeline
/// step whose `runner` field matches `name()`.
#[allow(async_fn_in_trait)]
pub trait StepRunner: Send + Sync {
    fn name(&self) -> &str;
    async fn execute(&self, ctx: &RunContext, input: ExecutorInput) -> Result<StepResult>;
}

/// Renders build events to the terminal (stderr) or structured
/// output (stdout). Replaces the WASM output-formatter plugins.
pub trait OutputRenderer: Send + Sync {
    fn on_event(&mut self, event: &BuildEvent);
}

/// Static registry of runners, built at startup.
pub struct RunnerRegistry {
    runners: HashMap<String, Arc<dyn StepRunner>>,
    default: Option<String>,
}

impl RunnerRegistry {
    pub fn new() -> Self {
        Self {
            runners: HashMap::new(),
            default: None,
        }
    }

    /// Register a runner. If `is_default` is true, steps that omit
    /// `runner` will dispatch here.
    pub fn register(&mut self, runner: Arc<dyn StepRunner>, is_default: bool) {
        let name = runner.name().to_string();
        if is_default {
            self.default = Some(name.clone());
        }
        self.runners.insert(name, runner);
    }

    /// Look up a runner by name. Falls back to the default runner when
    /// `name` is `None`.
    pub fn resolve(&self, name: Option<&str>) -> Option<Arc<dyn StepRunner>> {
        let key = name.or(self.default.as_deref())?;
        self.runners.get(key).cloned()
    }

    pub fn default_runner_name(&self) -> Option<&str> {
        self.default.as_deref()
    }

    pub fn runner_names(&self) -> Vec<String> {
        self.runners.keys().cloned().collect()
    }
}

impl std::fmt::Debug for RunnerRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunnerRegistry")
            .field("runners", &self.runners.keys().collect::<Vec<_>>())
            .field("default", &self.default)
            .finish()
    }
}
```

**Step 2: Add module to lib.rs**

In `crates/hm/src/lib.rs`, add `pub mod runner;` alongside the existing module declarations.

**Step 3: Create empty docker module**

Create `crates/hm/src/runner/docker.rs`:

```rust
//! Docker step runner — executes pipeline steps in Docker containers.
//!
//! This is the native replacement for the WASM `hm-plugin-docker`
//! crate. It calls `DockerClient` (Bollard) directly instead of
//! going through Extism host functions.
```

**Step 4: Verify build**

Run: `cargo check -p harmont-cli`
Expected: compiles (new module exists alongside old plugin system)

**Step 5: Write unit test for RunnerRegistry**

Add to `crates/hm/src/runner/mod.rs`:

```rust
#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    struct FakeRunner {
        runner_name: String,
    }

    impl StepRunner for FakeRunner {
        fn name(&self) -> &str {
            &self.runner_name
        }

        async fn execute(&self, _ctx: &RunContext, _input: ExecutorInput) -> Result<StepResult> {
            Ok(StepResult {
                exit_code: 0,
                committed_snapshot: None,
                artifacts: vec![],
            })
        }
    }

    #[test]
    fn resolve_by_name() {
        let mut reg = RunnerRegistry::new();
        reg.register(
            Arc::new(FakeRunner { runner_name: "docker".into() }),
            true,
        );
        assert!(reg.resolve(Some("docker")).is_some());
        assert!(reg.resolve(Some("unknown")).is_none());
    }

    #[test]
    fn resolve_default() {
        let mut reg = RunnerRegistry::new();
        reg.register(
            Arc::new(FakeRunner { runner_name: "docker".into() }),
            true,
        );
        let r = reg.resolve(None).unwrap();
        assert_eq!(r.name(), "docker");
    }

    #[test]
    fn no_default_returns_none() {
        let reg = RunnerRegistry::new();
        assert!(reg.resolve(None).is_none());
    }
}
```

**Step 6: Run tests**

Run: `cargo test -p harmont-cli runner::tests`
Expected: 3 tests pass

**Step 7: Commit**

```bash
git add crates/hm/src/runner/ crates/hm/src/lib.rs
git commit -m "feat(runner): define StepRunner trait and RunnerRegistry"
```

---

### Task 3: Implement DockerRunner

**Files:**
- Modify: `crates/hm/src/runner/docker.rs`
- Reference (read-only): `crates/hm-plugin-docker/src/lib.rs:31-198` (orchestration logic)
- Reference (read-only): `crates/hm/src/orchestrator/docker_host_fns.rs` (Bollard wrappers)
- Reference (read-only): `crates/hm-plugin-docker/src/decision.rs` (cache plan)
- Reference (read-only): `crates/hm-plugin-docker/src/image_name.rs` (image resolution)

This task merges the docker plugin's orchestration logic with the host-side Bollard wrappers into a single async implementation. The WASM plugin was synchronous (WASM is single-threaded); the new code is natively async.

**Step 1: Write the DockerRunner implementation**

The implementation merges three sources:
1. `crates/hm-plugin-docker/src/lib.rs:31-198` — orchestration flow (start→extract→exec→commit→cleanup)
2. `crates/hm/src/orchestrator/docker_host_fns.rs` — async Bollard wrapper calls
3. `crates/hm-plugin-docker/src/decision.rs` + `image_name.rs` — pure helper functions

Write `crates/hm/src/runner/docker.rs`:

```rust
//! Docker step runner — executes pipeline steps in Docker containers.
//!
//! Merges the orchestration logic from the old WASM `hm-plugin-docker`
//! with the Bollard host-function wrappers. No FFI boundary — calls
//! `DockerClient` directly.

use anyhow::{Context, Result};
use hm_plugin_protocol::{
    CacheDecision, CommandStep, ExecutorInput, SnapshotRef, StepResult,
};
use uuid::Uuid;

use super::{RunContext, StepRunner};
use crate::orchestrator::docker_client::DockerClient;

pub struct DockerRunner;

impl StepRunner for DockerRunner {
    fn name(&self) -> &str {
        "docker"
    }

    async fn execute(&self, ctx: &RunContext, input: ExecutorInput) -> Result<StepResult> {
        run_step(ctx, input).await
    }
}

async fn run_step(ctx: &RunContext, input: ExecutorInput) -> Result<StepResult> {
    let plan = decision_plan(&input.cache_lookup);

    if !plan.run_command {
        return Ok(StepResult {
            exit_code: 0,
            committed_snapshot: plan.hit_tag.clone(),
            artifacts: vec![],
        });
    }

    let image = resolve_image(
        &input.step,
        plan.hit_tag.as_ref(),
        input.parent_snapshot.as_ref(),
    );
    let container_name = sanitize_container_name(&input.run_id.to_string(), &input.step.key);
    let docker = &ctx.docker;

    // Pull if needed.
    if !docker.image_exists(&image).await.unwrap_or(false) {
        docker
            .pull_image(&image)
            .await
            .with_context(|| format!("pull '{image}'"))?;
    }

    let env_vec: Vec<String> = input
        .env
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect();

    let cid = docker
        .start_long_lived(&image, &env_vec, &input.workdir, &container_name)
        .await
        .context("start container")?;

    // From here on, always clean up the container.
    let result = run_in_container(ctx, docker, &cid, &input, &plan).await;
    docker.stop_remove(&cid).await;
    result
}

/// Container lifecycle after start: extract workspace → exec → commit.
async fn run_in_container(
    ctx: &RunContext,
    docker: &DockerClient,
    cid: &str,
    input: &ExecutorInput,
    plan: &DecisionPlan,
) -> Result<StepResult> {
    // Extract workspace archive into the container.
    let archive_bytes = ctx.archives.read(input.workspace_archive_id, 0, u64::MAX);
    if archive_bytes.is_empty() {
        anyhow::bail!("archive {} is empty or unknown", input.workspace_archive_id);
    }
    extract_workspace(docker, cid, &archive_bytes, &input.workdir)
        .await
        .context("extract workspace")?;

    // Execute the step command.
    let cmd = vec![
        "sh".to_string(),
        "-c".to_string(),
        input.step.cmd.clone(),
    ];
    let env_vec: Vec<String> = input
        .env
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect();
    let step_id = input.step_id;

    // Stream logs through the event bus.
    let (exit_code, _) = exec_streaming_with_events(
        docker, cid, &cmd, &env_vec, &input.workdir, step_id, &ctx.event_bus,
    )
    .await
    .context("exec step command")?;

    // Commit snapshot on success.
    let committed = if exit_code == 0 {
        let target_tag = plan.commit_to.clone().unwrap_or_else(|| {
            let safe: String = input
                .step
                .key
                .chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                        c
                    } else {
                        '-'
                    }
                })
                .collect();
            SnapshotRef::from(format!(
                "harmont-local-ephemeral/{safe}:run-{}",
                input.step_id.simple()
            ))
        });
        docker
            .commit_container(cid, &target_tag.to_string())
            .await
            .context("commit snapshot")?;
        Some(target_tag)
    } else {
        None
    };

    Ok(StepResult {
        exit_code,
        committed_snapshot: committed,
        artifacts: vec![],
    })
}

/// Extract a tar.gz archive into the container's workdir.
/// Mirrors the shell script from `docker_host_fns.rs`.
async fn extract_workspace(
    docker: &DockerClient,
    cid: &str,
    archive_bytes: &[u8],
    workdir: &str,
) -> Result<()> {
    // The extraction script handles idempotent replacement of previous
    // workspace contents (manifest-based cleanup for snapshot reuse).
    let cmd = vec![
        "sh".to_string(),
        "-c".to_string(),
        EXTRACT_CMD_SH.replace("$WORKDIR", workdir),
    ];
    let mut sink = tokio::io::sink();
    docker
        .exec_streaming_stdin(cid, &cmd, &[], workdir, archive_bytes, &mut sink)
        .await
        .context("extract workspace archive")?;
    Ok(())
}

/// Execute a command and stream stdout/stderr as `StepLog` events.
async fn exec_streaming_with_events(
    docker: &DockerClient,
    cid: &str,
    cmd: &[String],
    env: &[String],
    workdir: &str,
    step_id: Uuid,
    bus: &crate::orchestrator::events::EventBus,
) -> Result<(i32, ())> {
    use hm_plugin_protocol::{BuildEvent, StdStream};
    use tokio::io::AsyncBufReadExt;

    let mut output = Vec::new();
    let exit_code = docker
        .exec_streaming(cid, cmd, env, workdir, &mut output)
        .await?;

    // Emit each line as a StepLog event.
    let cursor = std::io::Cursor::new(&output);
    let reader = tokio::io::BufReader::new(tokio::io::AsyncReadExt::take(
        futures::io::AllowStdIo::new(cursor),
        output.len() as u64,
    ));
    // Simple line-by-line emission from the captured output.
    for line in String::from_utf8_lossy(&output).lines() {
        bus.emit(BuildEvent::StepLog {
            step_id,
            stream: StdStream::Stdout,
            line: line.to_string(),
            ts: chrono::Utc::now(),
        });
    }

    Ok((exit_code, ()))
}

// --- Pure helper functions (from hm-plugin-docker) ---

#[derive(Debug, Clone)]
struct DecisionPlan {
    run_command: bool,
    commit_to: Option<SnapshotRef>,
    hit_tag: Option<SnapshotRef>,
}

fn decision_plan(decision: &CacheDecision) -> DecisionPlan {
    match decision {
        CacheDecision::Hit { tag } => DecisionPlan {
            run_command: false,
            commit_to: None,
            hit_tag: Some(tag.clone()),
        },
        CacheDecision::MissBuildAs { tag } => DecisionPlan {
            run_command: true,
            commit_to: Some(tag.clone()),
            hit_tag: None,
        },
        CacheDecision::MissNoCommit => DecisionPlan {
            run_command: true,
            commit_to: None,
            hit_tag: None,
        },
    }
}

fn resolve_image(
    step: &CommandStep,
    hit_tag: Option<&SnapshotRef>,
    parent_snapshot: Option<&SnapshotRef>,
) -> String {
    if let Some(tag) = hit_tag {
        return tag.to_string();
    }
    if let Some(snap) = parent_snapshot {
        return snap.to_string();
    }
    if let Some(image) = &step.image {
        return image.clone();
    }
    "alpine:latest".to_string()
}

fn sanitize_container_name(run_id: &str, step_key: &str) -> String {
    let run_short: String = run_id.chars().take(8).collect();
    let key: String = step_key
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    format!("harmont-{run_short}-{key}")
}

const EXTRACT_CMD_SH: &str = r#"set -e
mkdir -p "$WORKDIR"
cd "$WORKDIR"
manifest="$WORKDIR/.harmont-extracted"
if [ -f "$manifest" ]; then
  sort -r "$manifest" | while IFS= read -r p; do
    [ -n "$p" ] || continue
    if [ -d "$p" ] && [ ! -L "$p" ]; then
      rmdir "$p" 2>/dev/null || true
    else
      rm -f "$p" 2>/dev/null || true
    fi
  done
  rm -f "$manifest"
fi
tmp=$(mktemp)
trap 'rm -f "$tmp"' EXIT
cat > "$tmp"
tar -tzf "$tmp" > "$manifest"
tar -xzf "$tmp"
"#;

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn step_with_image(image: Option<&str>) -> CommandStep {
        CommandStep {
            key: "k".into(),
            label: None,
            cmd: "true".into(),
            image: image.map(String::from),
            env: None,
            timeout_seconds: None,
            cache: None,
            runner: None,
            runner_args: None,
        }
    }

    #[test]
    fn hit_tag_wins() {
        let s = step_with_image(Some("rust:1.82"));
        let hit = SnapshotRef("cache:tag".into());
        let parent = SnapshotRef("parent:tag".into());
        assert_eq!(resolve_image(&s, Some(&hit), Some(&parent)), "cache:tag");
    }

    #[test]
    fn parent_snapshot_beats_step_image() {
        let s = step_with_image(Some("rust:1.82"));
        let parent = SnapshotRef("parent:tag".into());
        assert_eq!(resolve_image(&s, None, Some(&parent)), "parent:tag");
    }

    #[test]
    fn step_image_otherwise() {
        let s = step_with_image(Some("rust:1.82"));
        assert_eq!(resolve_image(&s, None, None), "rust:1.82");
    }

    #[test]
    fn fallback_alpine() {
        let s = step_with_image(None);
        assert_eq!(resolve_image(&s, None, None), "alpine:latest");
    }

    #[test]
    fn decision_hit_skips_command() {
        let p = decision_plan(&CacheDecision::Hit {
            tag: SnapshotRef("t".into()),
        });
        assert!(!p.run_command);
        assert!(p.hit_tag.is_some());
    }

    #[test]
    fn decision_miss_build_as_runs_and_commits() {
        let p = decision_plan(&CacheDecision::MissBuildAs {
            tag: SnapshotRef("t".into()),
        });
        assert!(p.run_command);
        assert!(p.commit_to.is_some());
    }

    #[test]
    fn decision_miss_no_commit() {
        let p = decision_plan(&CacheDecision::MissNoCommit);
        assert!(p.run_command);
        assert!(p.commit_to.is_none());
    }

    #[test]
    fn container_name_sanitizes() {
        let name = sanitize_container_name("abc-12345", "build/app");
        assert_eq!(name, "harmont-abc-1234-build-app");
    }
}
```

**Important notes for the implementer:**
- The `exec_streaming_with_events` function above is a sketch. The actual implementation must match how `docker_host_fns.rs:exec_impl` streams logs — read `crates/hm/src/orchestrator/docker_host_fns.rs:139-200` for the real streaming pattern using `exec_streaming` and the `StepLogWriter`.
- The `extract_workspace` function uses `exec_streaming_stdin` which pipes archive bytes through container stdin. Read `crates/hm/src/orchestrator/docker_host_fns.rs:63-112` for the actual implementation.
- The `ArchiveStore::read()` method signature and the archive ID flow must match what `crates/hm/src/orchestrator/archive.rs` provides.

**Step 2: Verify build**

Run: `cargo check -p harmont-cli`
Expected: compiles (DockerRunner exists alongside old plugin system)

**Step 3: Run unit tests**

Run: `cargo test -p harmont-cli runner::docker::tests`
Expected: all pure-function tests pass (image resolution, decision plan, sanitize)

**Step 4: Commit**

```bash
git add crates/hm/src/runner/docker.rs
git commit -m "feat(runner): implement DockerRunner (native, no FFI)"
```

---

### Task 4: Implement output renderers

**Files:**
- Create: `crates/hm/src/output/human.rs`
- Create: `crates/hm/src/output/json.rs`
- Modify: `crates/hm/src/output/mod.rs` (add renderer modules)

Moves rendering logic from the WASM output plugins into native modules. The human renderer comes from `crates/hm-plugin-output-human/src/render.rs` (147 lines). The JSON renderer is trivial.

**Step 1: Create human renderer**

Create `crates/hm/src/output/human.rs`. This is a direct adaptation of `crates/hm-plugin-output-human/src/render.rs` — same logic, but writes to an `io::Write` target instead of calling `host::write_stderr`:

```rust
//! Human-readable build-event renderer.
//!
//! Adapted from the old `hm-plugin-output-human` WASM plugin.

use std::collections::HashMap;
use std::io::Write;

use hm_plugin_protocol::BuildEvent;
use uuid::Uuid;

use super::super::runner::OutputRenderer;

pub struct HumanRenderer<W: Write> {
    out: W,
    step_keys: HashMap<Uuid, String>,
}

impl<W: Write> HumanRenderer<W> {
    pub fn new(out: W) -> Self {
        Self {
            out,
            step_keys: HashMap::new(),
        }
    }

    fn step_key_for(&self, id: Uuid) -> &str {
        self.step_keys
            .get(&id)
            .map(String::as_str)
            .unwrap_or("?")
    }
}

impl<W: Write + Send + Sync> OutputRenderer for HumanRenderer<W> {
    fn on_event(&mut self, event: &BuildEvent) {
        let bytes = render(event, &mut self.step_keys);
        if !bytes.is_empty() {
            let _ = self.out.write_all(&bytes);
        }
    }
}

fn render(ev: &BuildEvent, step_keys: &mut HashMap<Uuid, String>) -> Vec<u8> {
    match ev {
        BuildEvent::BuildStart { plan, .. } => format!(
            "build: {} steps in {} chain(s)\n",
            plan.step_count, plan.chain_count
        )
        .into_bytes(),
        BuildEvent::StepQueued { step_id, key, .. } => {
            step_keys.insert(*step_id, key.clone());
            Vec::new()
        }
        BuildEvent::StepStart {
            step_id,
            runner,
            image,
        } => {
            let key = step_keys.get(step_id).map_or("?", String::as_str);
            match image {
                Some(img) => format!("[{key}] start (runner={runner} image={img})\n"),
                None => format!("[{key}] start (runner={runner})\n"),
            }
            .into_bytes()
        }
        BuildEvent::StepLog { step_id, line, .. } => {
            let key = step_keys.get(step_id).map_or("?", String::as_str);
            format!("[{key}] {line}\n").into_bytes()
        }
        BuildEvent::StepCacheHit { step_id, tag, .. } => {
            let key = step_keys.get(step_id).map_or("?", String::as_str);
            format!("[{key}] cache hit ({tag})\n").into_bytes()
        }
        BuildEvent::StepEnd {
            step_id,
            exit_code,
            duration_ms,
            ..
        } => {
            let key = step_keys.get(step_id).map_or("?", String::as_str);
            format!("[{key}] end exit={exit_code} duration={duration_ms}ms\n").into_bytes()
        }
        BuildEvent::BuildEnd {
            exit_code,
            duration_ms,
        } => format!("build: end exit={exit_code} duration={duration_ms}ms\n").into_bytes(),
        BuildEvent::ChainFailed {
            chain_idx,
            failed_step_key,
            exit_code,
            message,
            ..
        } => format!(
            "chain {chain_idx}: FAILED at step '{failed_step_key}' (exit={exit_code}): {message}\n"
        )
        .into_bytes(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use hm_plugin_protocol::{PlanSummary, StdStream};

    #[test]
    fn build_start_renders_counts() {
        let mut keys = HashMap::new();
        let ev = BuildEvent::BuildStart {
            run_id: Uuid::nil(),
            plan: PlanSummary {
                step_count: 3,
                chain_count: 2,
                default_runner: "docker".into(),
            },
            started_at: chrono::Utc::now(),
        };
        let s = String::from_utf8(render(&ev, &mut keys)).unwrap();
        assert!(s.contains("3 steps"));
        assert!(s.contains("2 chain"));
    }

    #[test]
    fn step_log_with_key() {
        let mut keys = HashMap::new();
        let step_id = Uuid::new_v4();
        render(
            &BuildEvent::StepQueued {
                step_id,
                key: "build".into(),
                chain_idx: 0,
            },
            &mut keys,
        );
        let s = String::from_utf8(render(
            &BuildEvent::StepLog {
                step_id,
                stream: StdStream::Stdout,
                line: "hello".into(),
                ts: chrono::Utc::now(),
            },
            &mut keys,
        ))
        .unwrap();
        assert_eq!(s, "[build] hello\n");
    }
}
```

**Step 2: Create JSON renderer**

Create `crates/hm/src/output/json.rs`:

```rust
//! JSON-lines build-event renderer.

use std::io::Write;

use hm_plugin_protocol::BuildEvent;

use super::super::runner::OutputRenderer;

pub struct JsonRenderer<W: Write> {
    out: W,
}

impl<W: Write> JsonRenderer<W> {
    pub fn new(out: W) -> Self {
        Self { out }
    }
}

impl<W: Write + Send + Sync> OutputRenderer for JsonRenderer<W> {
    fn on_event(&mut self, event: &BuildEvent) {
        if let Ok(mut bytes) = serde_json::to_vec(event) {
            bytes.push(b'\n');
            let _ = self.out.write_all(&bytes);
        }
    }
}
```

**Step 3: Wire into output/mod.rs**

Add `pub mod human;` and `pub mod json;` to `crates/hm/src/output/mod.rs`.

**Step 4: Verify build**

Run: `cargo check -p harmont-cli`
Expected: compiles

**Step 5: Run tests**

Run: `cargo test -p harmont-cli output::human::tests`
Expected: 2 tests pass

**Step 6: Commit**

```bash
git add crates/hm/src/output/human.rs crates/hm/src/output/json.rs crates/hm/src/output/mod.rs
git commit -m "feat(output): inline human and JSON renderers (no WASM)"
```

---

### Task 5: Rewire scheduler to use direct dispatch

**Files:**
- Modify: `crates/hm/src/orchestrator/scheduler.rs`
- Modify: `crates/hm/src/orchestrator/output_subscriber.rs`
- Modify: `crates/hm/src/commands/run/local.rs` (caller of `scheduler::run`)

This is the critical switchover. The scheduler stops using `PluginRegistry` and starts using `RunnerRegistry` + `OutputRenderer`.

**Step 1: Change scheduler::run signature**

The current `scheduler::run` signature is:

```rust
pub async fn run(
    graph: PipelineGraph,
    repo_root: PathBuf,
    parallelism: usize,
    format_name: String,
) -> Result<i32>
```

It internally creates a `PluginRegistry`. Change it to receive a `RunnerRegistry` via DI:

```rust
pub async fn run(
    graph: PipelineGraph,
    repo_root: PathBuf,
    parallelism: usize,
    format_name: String,
    runner_registry: Arc<RunnerRegistry>,
) -> Result<i32>
```

**Step 2: Replace plugin registry setup in `run`**

Remove the block that creates `PluginRegistry::load(RegistryConfig { embedded: [...], ... })`. The `runner_registry` is now passed in.

**Step 3: Replace `execute_step` dispatch**

Current code (lines ~353-382):
```rust
let plugin = {
    let reg = registry.lock().await;
    let idx = reg.runner_index.get(&runner) ...;
    reg.get(idx) ...
};
plugin.call_capability("hm_executor_run", &input).await
```

Replace with:
```rust
let runner = runner_registry
    .resolve(input.step.runner.as_deref())
    .ok_or_else(|| HmError::UnknownRunner { ... })?;
runner.execute(&ctx, input).await
```

**Step 4: Replace output_subscriber**

Current `output_subscriber.rs` dispatches events through `plugin.call_capability("hm_output_on_event", &event)`. Replace with:

```rust
pub fn spawn(
    bus: Arc<EventBus>,
    mut renderer: Box<dyn OutputRenderer>,
) -> tokio::task::JoinHandle<()> {
    let mut rx = bus.subscribe();
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    let is_end = event.is_build_end();
                    renderer.on_event(&event);
                    if is_end {
                        return;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    eprintln!("output: dropped {n} events (slow renderer)");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
            }
        }
    })
}
```

**Step 5: Update `local.rs` (the caller)**

Read `crates/hm/src/commands/run/local.rs` to find where `scheduler::run` is called. Build the `RunnerRegistry` there:

```rust
use crate::runner::{RunnerRegistry, docker::DockerRunner};

let mut runner_registry = RunnerRegistry::new();
runner_registry.register(Arc::new(DockerRunner), true);
let runner_registry = Arc::new(runner_registry);

scheduler::run(graph, repo_root, parallelism, format, runner_registry).await
```

**Step 6: Remove OrchestratorState global**

The old `state::install(state_arc)` / `state::current()` pattern was needed because WASM host functions are `extern "C"` with no way to pass context. With DI, `RunContext` is passed explicitly. Remove the `state.rs` thread-local pattern and pass `RunContext` through function arguments.

In `scheduler::run`, build the `RunContext`:
```rust
let run_ctx = RunContext {
    docker: docker.clone(),
    event_bus: bus.clone(),
    archives: Arc::new(archives),
    cancel: cancel.clone(),
};
```

Pass it to `execute_step`, which passes it to `runner.execute(&run_ctx, input)`.

**Step 7: Remove host_fns step-id tracking**

Delete the `set_current_step_id` / `clear_current_step_id` calls from `execute_step` — those were for the WASM host functions to know which step was running.

**Step 8: Verify build**

Run: `cargo check -p harmont-cli`
Expected: compiles (may have warnings about unused old plugin code)

**Step 9: Run existing e2e tests that don't depend on WASM**

Run: `cargo test -p harmont-cli -- --skip plugin_ --skip runner_dispatch --skip cmd_cloud`
Expected: non-plugin tests pass

**Step 10: Commit**

```bash
git add crates/hm/src/orchestrator/ crates/hm/src/commands/run/
git commit -m "refactor(scheduler): replace plugin dispatch with direct runner DI"
```

---

### Task 6: Rewire CLI and delete plugin infrastructure

**Files:**
- Modify: `crates/hm/src/cli/mod.rs`
- Delete: `crates/hm/src/cli/external.rs`
- Modify: `crates/hm/src/cli/plugin.rs`
- Delete: `crates/hm/src/plugin/` (entire module)
- Delete: `crates/hm/build.rs`
- Delete: `crates/hm/src/orchestrator/docker_host_fns.rs`
- Delete: `crates/hm/src/orchestrator/state.rs`
- Modify: `crates/hm/src/orchestrator/mod.rs`
- Modify: `crates/hm/src/lib.rs`
- Modify: `crates/hm/Cargo.toml`
- Modify: `Cargo.toml` (workspace root)
- Delete: `crates/hm-plugin-sdk/` (entire crate)
- Delete: `crates/hm-plugin-docker/` (entire crate)
- Delete: `crates/hm-plugin-output-human/` (entire crate)
- Delete: `crates/hm-plugin-output-json/` (entire crate)
- Delete: `crates/hm-fixtures/` (entire crate)

**Step 1: Modify CLI Command enum**

In `crates/hm/src/cli/mod.rs`, remove the `External(Vec<String>)` variant and add an explicit `Cloud` command:

```rust
#[derive(Debug, Clone, Subcommand)]
pub enum Command {
    Run(RunArgs),
    Version,
    #[command(subcommand)]
    Plugin(PluginCommand),
    #[command(subcommand)]
    Dev(DevCommand),
    /// Interact with the Harmont cloud platform.
    Cloud {
        #[command(subcommand)]
        subcmd: Option<crate::commands::cloud::CloudSubcommand>,
    },
}
```

Until Task 7 (cloud migration), the `Cloud` variant can print a message:

```rust
Command::Cloud { .. } => {
    eprintln!("hm cloud: temporarily unavailable (migrating from WASM)");
    Ok(1)
}
```

Delete `cli::external` from the module tree and delete `external.rs`.

**Step 2: Simplify cli/plugin.rs**

The `hm plugin` subcommand (`list`, `install`, etc.) currently scans for WASM files. With static plugins there's nothing to discover. Simplify to list the statically registered runners:

```rust
#[derive(Debug, Clone, Subcommand)]
pub enum PluginCommand {
    /// List registered runners.
    List,
}

pub async fn run(cmd: PluginCommand) -> Result<()> {
    match cmd {
        PluginCommand::List => {
            println!("Registered runners:");
            println!("  docker (default, built-in)");
            Ok(())
        }
    }
}
```

**Step 3: Delete plugin module**

```bash
rm -rf crates/hm/src/plugin/
```

Remove `pub mod plugin;` from `crates/hm/src/lib.rs`.

**Step 4: Delete build.rs**

```bash
rm crates/hm/build.rs
```

**Step 5: Delete docker_host_fns.rs and state.rs**

```bash
rm crates/hm/src/orchestrator/docker_host_fns.rs
rm crates/hm/src/orchestrator/state.rs
```

Remove their `mod` declarations from `orchestrator/mod.rs`.

**Step 6: Delete WASM plugin crates**

```bash
rm -rf crates/hm-plugin-sdk
rm -rf crates/hm-plugin-docker
rm -rf crates/hm-plugin-output-human
rm -rf crates/hm-plugin-output-json
rm -rf crates/hm-fixtures
```

**Step 7: Update workspace Cargo.toml**

In root `Cargo.toml`, remove the deleted crates from `[workspace].members` and `default-members`. Remove `hm-plugin-sdk` from `[workspace.dependencies]`. Remove `extism` and `extism-pdk` from workspace deps if present.

**Step 8: Update hm/Cargo.toml**

Remove dependencies on:
- `hm-plugin-sdk`
- `extism`
- Any WASM-related deps

Keep:
- `hm-plugin-protocol`
- `hm-pipeline-ir`
- `hm-util`
- `bollard`, `tokio`, `clap`, `daggy`, etc.

**Step 9: Fix all compilation errors**

Walk through every `use crate::plugin::*` import in the `hm` crate and remove or replace them. Key patterns:

- `use crate::plugin::{PluginRegistry, RegistryConfig}` → remove (scheduler now uses RunnerRegistry)
- `use crate::plugin::host_fns::*` → remove (no more host functions)
- `use crate::plugin::embedded::*` → remove (no more embedded WASM)
- `crate::plugin::signal::install_ctrlc(cancel)` → replace with direct `tokio_util::sync::CancellationToken` + ctrl-c handler

The signal handler is probably simple — check `crates/hm/src/plugin/signal.rs` and inline the ctrl-c setup.

**Step 10: Verify build**

Run: `cargo check --workspace`
Expected: compiles clean

**Step 11: Run tests**

Run: `cargo test --workspace -- --skip plugin_ --skip runner_dispatch --skip cmd_cloud`
Expected: non-plugin tests pass

**Step 12: Commit**

```bash
git add -A
git commit -m "refactor: delete WASM plugin system, wire static DI"
```

---

### Task 7: Migrate cloud client

**Files:**
- Modify: `crates/hm-plugin-cloud/` (heavy rewrite — remove extism, add direct deps)
- Modify: `crates/hm-plugin-cloud/Cargo.toml`
- Modify: `crates/hm/src/cli/mod.rs` (wire cloud commands)
- Create: `crates/hm/src/commands/cloud/mod.rs` (thin wrapper)

This task converts the cloud client from a WASM plugin to a regular library crate that the `hm` binary depends on. The cloud crate keeps its internal structure (`verbs/`, `auth/`, `api/`, etc.) but replaces all host function calls.

**Important:** If the cloud crate is too deeply entangled with extism-pdk, an alternative approach is to delete `crates/hm-plugin-cloud` entirely and recreate the `crates/hm-cloud` crate from scratch by porting each verb module individually. Evaluate the entanglement before choosing.

**Step 1: Evaluate extism entanglement**

Read these files in `crates/hm-plugin-cloud/src/`:
- `http.rs` — how HTTP requests are made
- `creds.rs` — how credentials are stored
- `state.rs` — how plugin-scoped state works
- `cli.rs` — how command dispatch works

Count the host function call sites. If they're concentrated in a few wrapper modules, in-place refactoring is feasible. If they're scattered everywhere, delete and rewrite.

**Step 2: Change crate type**

In `crates/hm-plugin-cloud/Cargo.toml`:
- Remove `[lib] crate-type = ["cdylib"]`
- Remove `extism-pdk` dependency
- Remove `hm-plugin-sdk` dependency
- Add: `reqwest = { version = "0.12", features = ["json"] }`
- Add: `keyring = "3"` (if keyring host fn was used)
- Add: `dialoguer = "0.11"` (if TTY host fn was used)
- Add: `open = "5"` (if browser host fn was used)
- Add: `tokio = { workspace = true }` (the crate is now async-native)

**Step 3: Replace host function calls**

Pattern replacements throughout the cloud crate:

| Old (extism host fn) | New (direct) |
|----------------------|-------------|
| `host::write_stdout(bytes)` | `std::io::stdout().write_all(bytes)` |
| `host::write_stderr(bytes)` | `std::io::stderr().write_all(bytes)` |
| `host::log(Level::Info, msg)` | `tracing::info!("{msg}")` |
| `extism_pdk::HttpRequest::new(url).send()` | `reqwest::get(url).await` |
| `host::kv_get(KvScope::Plugin, key)` | File-backed state (reuse creds_store pattern from `hm`) |
| `host::tty_prompt(args)` | `dialoguer::Input::new().with_prompt(args.prompt).interact()` |
| `host::tty_confirm(args)` | `dialoguer::Confirm::new().with_prompt(args.prompt).interact()` |
| `host::browser_open(url)` | `open::that(url)` |
| `host::keyring_get(args)` | `keyring::Entry::new(service, account).get_password()` |
| `host::keyring_set(args)` | `keyring::Entry::new(service, account).set_password(secret)` |

**Step 4: Make cloud dispatch async**

The cloud crate's `cli::dispatch()` was synchronous (WASM is single-threaded). HTTP calls were synchronous via extism-pdk. Now make it async:

```rust
pub async fn dispatch(subcmd: CloudSubcommand) -> Result<i32> {
    match subcmd {
        CloudSubcommand::Login(args) => verbs::login::run(args).await,
        CloudSubcommand::Whoami => verbs::whoami::run().await,
        // ... etc
    }
}
```

**Step 5: Expose public API**

The cloud crate's `lib.rs` should export:
```rust
pub mod cli;  // CloudSubcommand enum + dispatch function
```

Remove the `register_plugin!` macro call, the `impl SubcommandPlugin`, and the WASM entry point.

**Step 6: Wire into hm binary**

Add `hm-plugin-cloud` (or renamed `hm-cloud`) to `hm/Cargo.toml` as a normal dependency.

In `crates/hm/src/cli/mod.rs`, update the `Cloud` variant:
```rust
Command::Cloud { subcmd } => {
    match subcmd {
        Some(cmd) => hm_plugin_cloud::cli::dispatch(cmd).await.map(|_| 0),
        None => { /* print help */ Ok(0) }
    }
}
```

In `crates/hm/src/commands/cloud/mod.rs`, re-export the cloud subcommand enum for clap integration.

**Step 7: Add workspace dependency**

Update root `Cargo.toml`:
```toml
[workspace.members]
# ... add back hm-plugin-cloud (now a lib, not cdylib)
```

**Step 8: Verify build**

Run: `cargo check --workspace`
Expected: compiles

**Step 9: Run cloud tests**

Run: `cargo test -p hm-plugin-cloud`
Expected: unit tests pass (HTTP tests may need mocking setup)

**Step 10: Commit**

```bash
git add -A
git commit -m "refactor(cloud): migrate from WASM plugin to direct library"
```

---

### Task 8: Protocol and test cleanup

**Files:**
- Modify: `crates/hm-plugin-protocol/src/lib.rs`
- Delete: `crates/hm-plugin-protocol/src/manifest.rs` (or gut it)
- Modify: `crates/hm-plugin-protocol/src/host_abi.rs` (remove Docker types)
- Delete: `crates/hm/tests/plugin_host_fns.rs`
- Delete: `crates/hm/tests/plugin_manifest.rs`
- Delete: `crates/hm/tests/plugin_registry.rs`
- Delete: `crates/hm/tests/runner_dispatch.rs`
- Delete: `crates/hm/tests/plugin_kv_concurrency.rs`
- Delete: `crates/hm/tests/common/fixtures.rs`
- Modify: `crates/hm/tests/common/mod.rs`

**Step 1: Clean up protocol crate**

In `crates/hm-plugin-protocol/src/lib.rs`:
- Remove `pub const HM_PLUGIN_API_VERSION: u32 = 1;`
- Remove re-exports of manifest types
- Remove re-exports of Docker host-function types

In `manifest.rs`:
- Remove `PluginManifest`, `Capability`, `StepExecutorSpec`, `SubcommandSpec`, `LifecycleHookSpec`, `OutputFormatterSpec`, `ManifestError`
- Keep `ArgSpec` and `ValueType` only if still used somewhere
- If nothing remains, delete the file

In `host_abi.rs`:
- Remove `DockerStartArgs`, `DockerExecArgs`, `DockerExtractArgs`, `DockerCommitArgs`
- Remove `SocketHandle`, `LoopbackHandle`, `CallbackData`, and related types that existed only for the plugin FFI
- Keep `Level`, `KvScope` if they're used elsewhere (e.g., logging)
- If only Docker types remain, delete the file

**Step 2: Delete plugin-specific tests**

```bash
rm crates/hm/tests/plugin_host_fns.rs
rm crates/hm/tests/plugin_manifest.rs
rm crates/hm/tests/plugin_registry.rs
rm crates/hm/tests/runner_dispatch.rs
rm crates/hm/tests/plugin_kv_concurrency.rs
rm crates/hm/tests/cmd_plugin.rs
rm crates/hm/tests/common/fixtures.rs
```

Update `crates/hm/tests/common/mod.rs` to remove the `fixtures` module.

**Step 3: Update remaining integration tests**

Tests like `local_e2e.rs`, `local_parallelism.rs`, `default_image_inheritance.rs` may reference `harmont_cli::plugin::*`. Update imports to use the new `runner` module where needed.

**Step 4: Remove test fixture directory**

```bash
rm -rf tests/fixtures/
```

Remove fixture crates from root `Cargo.toml` workspace members if still present.

**Step 5: Clean up Cargo.toml dependencies**

In `crates/hm-plugin-protocol/Cargo.toml`:
- Remove any unused dependencies left over from the manifest system

In root `Cargo.toml`:
- Remove `extism-pdk` workspace dependency if present
- Remove any other dead deps

**Step 6: Final build and test**

Run: `cargo check --workspace`
Run: `cargo test --workspace`
Run: `cargo clippy --workspace`
Expected: clean build, all remaining tests pass, no clippy warnings

**Step 7: Commit**

```bash
git add -A
git commit -m "chore: clean up protocol types and delete plugin-specific tests"
```

---

## Verification Checklist

After all tasks:

1. `cargo check --workspace` — clean compile, no warnings
2. `cargo test --workspace` — all tests pass
3. `cargo clippy --workspace` — no warnings
4. No WASM artifacts in the build pipeline (no `build.rs`, no `include_bytes!`, no `wasm32-wasip1`)
5. No extism dependency anywhere in `Cargo.lock`
6. No `hm-plugin-sdk` dependency anywhere
7. `hm run` still executes pipelines via Docker (integration test or manual test)
8. `hm cloud` works (after Task 7)
9. `git log --oneline` shows clean progression of commits

## Dependency Graph (After)

```
hm (binary)
├── hm-plugin-protocol   (wire types: ExecutorInput, StepResult, BuildEvent, etc.)
├── hm-pipeline-ir       (PipelineGraph, CommandStep, daggy)
├── hm-util              (OS utilities)
├── hm-plugin-cloud      (lib crate, direct reqwest/keyring/dialoguer)
├── bollard              (Docker API)
├── tokio                (async runtime)
├── clap                 (CLI parsing)
└── daggy                (DAG scheduling)
```

No more: extism, extism-pdk, stabby, borsh, libloading, wasm32-wasip1.
