# Simplify Tracing: Remove output/ Module, Use Plain tracing_subscriber

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Delete the entire `output/` module and its custom CliLayer, replace all output with plain `tracing::info!` / `tracing::warn!` / `tracing::error!` calls through a single colorful `tracing_subscriber::fmt` subscriber.

**Architecture:** One `tracing_subscriber::fmt()` configured without timestamps, without targets, with ANSI colors. Default level is `info`; `--verbose` drops to `debug`. Build events formatted inline in `output_subscriber` via tracing — no renderer trait, no JSON output. Pure formatting helpers (`rel_time`, `status_pill`, etc.) relocated to a small `fmt.rs` utility file.

**Tech Stack:** `tracing 0.1`, `tracing-subscriber 0.3`

---

## Current State

- `output/` has 7 files: `cli_layer.rs`, `format.rs`, `human.rs`, `json.rs`, `spinner.rs`, `status.rs`, `mod.rs`
- 78 `tracing::info!(target: "user::*", ...)` calls across the workspace
- `OutputRenderer` trait + `HumanRenderer` + `JsonRenderer` — wired through scheduler → output_subscriber
- `OutputMode` enum in mod.rs — **dead code** (constructed in context.rs, never read)
- `Spinner` in spinner.rs — **dead code** (zero usages)
- `--format` flag on `hm run` selects human vs JSON renderer
- `owo_colors` used in main.rs (color override), format.rs, status.rs, and `commands/dev/logmux.rs`

## What Gets Deleted

| File | Reason |
|------|--------|
| `output/cli_layer.rs` | Custom subscriber replaced by plain fmt |
| `output/status.rs` | 4 functions → inlined as plain `tracing::info/warn/error` at callsites |
| `output/human.rs` | Build event formatting inlined into `output_subscriber.rs` |
| `output/json.rs` | JSON output removed per user request |
| `output/spinner.rs` | Dead code (zero usages) |
| `output/mod.rs` | OutputMode dead code; macros no longer needed |
| `runner::OutputRenderer` trait | No longer needed — output_subscriber formats directly |

## What Survives (Relocated)

| Function | From | To | Why |
|----------|------|----|-----|
| `rel_time()` | `output/format.rs` | `src/fmt.rs` | Pure string formatter, used by format helpers |
| `duration_human()` | `output/format.rs` | `src/fmt.rs` | Pure string formatter |
| `elapsed_between()` | `output/format.rs` | `src/fmt.rs` | Pure string formatter |
| `status_pill()` | `output/format.rs` | `src/fmt.rs` | Pure string formatter |
| `hyperlink()` / `hyperlink_with()` | `output/format.rs` | `src/fmt.rs` | Pure string formatter |

Note: `banner()`, `header()`, `kv()`, `step()`, `empty_state()` are deleted — they become plain `tracing::info!()` calls at their single callsite or are currently unused.

---

## Task 1: Create `src/fmt.rs` — Relocate Pure Formatting Helpers

**Files:**
- Create: `crates/hm/src/fmt.rs`
- Modify: `crates/hm/src/lib.rs`

**Step 1: Create fmt.rs with the pure functions and their tests**

Copy these functions from `output/format.rs` to `crates/hm/src/fmt.rs`:
- `rel_time(epoch: i64) -> String`
- `duration_human(secs: i64) -> String`
- `elapsed_between(start: i64, end: i64) -> String`
- `status_style(status: &str) -> (Style, &'static str)` (private)
- `status_pill(status: &str) -> String`
- `supports_hyperlinks() -> bool` (private)
- `hyperlink(url: &str, label: &str) -> String`
- `hyperlink_with(url: &str, label: &str, enabled: bool) -> String`

Copy ALL the `#[cfg(test)] mod tests` from `output/format.rs` unchanged.

Keep the same imports (`chrono`, `owo_colors`). No doc comment preamble — just the functions.

**Step 2: Register module in lib.rs**

Add `pub mod fmt;` to `crates/hm/src/lib.rs`.

**Step 3: Run tests**

Run: `cargo test -p harmont-cli -- fmt::`
Expected: all format tests pass

**Step 4: Commit**

```bash
git add crates/hm/src/fmt.rs crates/hm/src/lib.rs
git commit -m "refactor: extract pure formatting helpers to fmt.rs"
```

---

## Task 2: Rewrite Subscriber Init in main.rs

**Files:**
- Modify: `crates/hm/src/main.rs`

**Step 1: Replace the subscriber setup**

The current main.rs uses `CliLayer::real()` + optional fmt layer + registry. Replace the entire subscriber block with:

```rust
use tracing_subscriber::EnvFilter;

let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
    EnvFilter::new(if args.verbose { "debug" } else { "info" })
});

tracing_subscriber::fmt()
    .with_env_filter(filter)
    .without_time()
    .with_target(false)
    .init();
```

Remove these imports:
- `use tracing_subscriber::layer::SubscriberExt;`
- `use tracing_subscriber::util::SubscriberInitExt;`
- `use tracing_subscriber::Layer as _;`
- `use harmont_cli::output::cli_layer::CliLayer;`

Change `use harmont_cli::output::status;` → remove it. The `handle_error` function currently calls `status::print_error(...)`. Replace with:

```rust
fn handle_error(err: &anyhow::Error) -> i32 {
    if let Some(hm_err) = err.downcast_ref::<HmError>() {
        tracing::error!("{hm_err}");
        return hm_err.exit_code();
    }

    tracing::error!("{err:#}");
    error::EXIT_BUILD_FAILED
}
```

**Step 2: Verify build**

Run: `cargo check -p harmont-cli`
Expected: errors about missing `output::*` imports in other files (expected — we'll fix those in subsequent tasks)

Actually, this should compile because `output/` module still exists at this point. The only change is main.rs no longer importing CliLayer or status.

**Step 3: Commit**

```bash
git add crates/hm/src/main.rs
git commit -m "refactor: replace layered subscriber with plain tracing_subscriber::fmt"
```

---

## Task 3: Remove OutputRenderer Trait + Inline Build Event Formatting

**Files:**
- Modify: `crates/hm/src/orchestrator/output_subscriber.rs`
- Modify: `crates/hm/src/orchestrator/scheduler.rs`
- Modify: `crates/hm/src/commands/run/local.rs`
- Modify: `crates/hm/src/runner/mod.rs`
- Modify: `crates/hm/src/cli/run.rs`

**Step 1: Rewrite output_subscriber.rs to format events directly**

Replace the entire file. The new version formats `BuildEvent`s using tracing — no renderer parameter. Port the formatting logic from `output/human.rs`:

```rust
#![allow(clippy::needless_pass_by_value)]

use std::collections::HashMap;
use std::sync::Arc;

use hm_plugin_protocol::BuildEvent;
use tokio::sync::broadcast::error::RecvError;
use uuid::Uuid;

use super::events::EventBus;

#[must_use]
pub fn spawn(bus: Arc<EventBus>) -> tokio::task::JoinHandle<()> {
    let mut rx = bus.subscribe();
    let mut step_keys: HashMap<Uuid, String> = HashMap::new();

    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    let is_end = event.is_build_end();
                    log_event(&event, &mut step_keys);
                    if is_end {
                        return;
                    }
                }
                Err(RecvError::Closed) => return,
                Err(RecvError::Lagged(n)) => {
                    tracing::warn!("output: dropped {n} events");
                }
            }
        }
    })
}

fn step_key<'a>(keys: &'a HashMap<Uuid, String>, id: &Uuid) -> &'a str {
    keys.get(id).map_or("?", String::as_str)
}

fn log_event(event: &BuildEvent, keys: &mut HashMap<Uuid, String>) {
    match event {
        BuildEvent::BuildStart { plan, .. } => {
            tracing::info!(
                "build: {} steps in {} chain(s)",
                plan.step_count,
                plan.chain_count,
            );
        }
        BuildEvent::StepQueued { step_id, key, .. } => {
            keys.insert(*step_id, key.clone());
        }
        BuildEvent::StepStart {
            step_id,
            runner,
            image,
        } => {
            let key = step_key(keys, step_id);
            if let Some(img) = image {
                tracing::info!("[{key}] start (runner={runner} image={img})");
            } else {
                tracing::info!("[{key}] start (runner={runner})");
            }
        }
        BuildEvent::StepLog {
            step_id, line, ..
        } => {
            let key = step_key(keys, step_id);
            tracing::info!("[{key}] {line}");
        }
        BuildEvent::StepCacheHit {
            step_id, tag, ..
        } => {
            let key = step_key(keys, step_id);
            tracing::info!("[{key}] cache hit ({tag})");
        }
        BuildEvent::StepEnd {
            step_id,
            exit_code,
            duration_ms,
            ..
        } => {
            let key = step_key(keys, step_id);
            tracing::info!("[{key}] end exit={exit_code} duration={duration_ms}ms");
        }
        BuildEvent::BuildEnd {
            exit_code,
            duration_ms,
        } => {
            tracing::info!("build: end exit={exit_code} duration={duration_ms}ms");
        }
        BuildEvent::ChainFailed {
            chain_idx,
            failed_step_key,
            exit_code,
            message,
            ..
        } => {
            tracing::error!(
                "chain {chain_idx}: FAILED at step '{failed_step_key}' (exit={exit_code}): {message}"
            );
        }
    }
}
```

**Step 2: Update scheduler.rs**

The `run()` function signature currently takes `renderer: Box<dyn OutputRenderer>`. Change it to not take a renderer — pass only the bus to `output_subscriber::spawn`.

In `crates/hm/src/orchestrator/scheduler.rs`, find the `run()` function (around line 69):

Change the signature from:
```rust
pub async fn run(
    graph: PipelineGraph,
    repo_root: PathBuf,
    parallelism: usize,
    runner_registry: Arc<RunnerRegistry>,
    renderer: Box<dyn OutputRenderer>,
) -> Result<i32> {
```
To:
```rust
pub async fn run(
    graph: PipelineGraph,
    repo_root: PathBuf,
    parallelism: usize,
    runner_registry: Arc<RunnerRegistry>,
) -> Result<i32> {
```

Find the line that calls `output_subscriber::spawn(bus.clone(), renderer)` and change to `output_subscriber::spawn(bus.clone())`.

Remove the `OutputRenderer` import: `use crate::runner::{OutputRenderer, RunContext, RunnerRegistry}` → `use crate::runner::{RunContext, RunnerRegistry}`.

**Step 3: Update commands/run/local.rs**

Remove the renderer construction and the `--format` match. Change the `orchestrator::run()` call:

```rust
// DELETE these lines:
// let renderer: Box<dyn crate::runner::OutputRenderer> = match args.format.as_str() {
//     "json" => Box::new(crate::output::json::JsonRenderer::new(std::io::stdout())),
//     _ => Box::new(crate::output::human::HumanRenderer::new(std::io::stderr())),
// };

// Change orchestrator::run call — remove renderer argument:
let exit_code =
    crate::orchestrator::run(graph, repo_root, parallelism, runner_registry).await?;
```

Also change the banner call — replace `use crate::output::format::banner` with an inline `tracing::info!`:

```rust
if args.format == "human" {
    tracing::info!("▌ hm run --local · slug={slug}");
}
```

(The `args.format` field stays for now — it'll be removed in Task 4.)

**Step 4: Remove OutputRenderer from runner/mod.rs**

Delete the `OutputRenderer` trait definition (around lines 74-82):
```rust
// DELETE:
// pub trait OutputRenderer: Send + fmt::Debug {
//     fn on_event(&mut self, event: &BuildEvent);
// }
```

Remove the `BuildEvent` import if no longer used: `use hm_plugin_protocol::{BuildEvent, ExecutorInput, StepResult}` → `use hm_plugin_protocol::{ExecutorInput, StepResult}`.

**Step 5: Verify build**

Run: `cargo check -p harmont-cli`
Expected: compiles (output/ module still exists but human.rs and json.rs are now unreferenced)

**Step 6: Commit**

```bash
git add crates/hm/src/orchestrator/ crates/hm/src/commands/run/local.rs crates/hm/src/runner/mod.rs
git commit -m "refactor: inline build event formatting, remove OutputRenderer trait"
```

---

## Task 4: Remove `--format` Flag

**Files:**
- Modify: `crates/hm/src/cli/run.rs`
- Modify: `crates/hm/src/commands/run/local.rs`

**Step 1: Remove --format from RunArgs**

In `crates/hm/src/cli/run.rs`, delete:
```rust
    /// Output formatter (matches an installed output-formatter plugin
    /// `name`). Built-ins: `human`, `json`. Default: `human`.
    #[arg(long, value_name = "NAME", default_value = "human", global = false)]
    pub format: String,
```

**Step 2: Remove format check in local.rs**

In `crates/hm/src/commands/run/local.rs`, replace:
```rust
if args.format == "human" {
    tracing::info!("▌ hm run --local · slug={slug}");
}
```
With just:
```rust
tracing::info!("▌ hm run --local · slug={slug}");
```

**Step 3: Verify build**

Run: `cargo check -p harmont-cli`
Expected: compiles clean

**Step 4: Commit**

```bash
git add crates/hm/src/cli/run.rs crates/hm/src/commands/run/local.rs
git commit -m "refactor: remove --format flag (JSON output removed)"
```

---

## Task 5: Strip `target:` from All Tracing Calls

**Files:**
- Modify: every file that has `target: "user::stdout"` or `target: "user::stderr"`

**Step 1: Bulk replace**

There are 78 tracing calls with explicit targets across the workspace. Replace them:

- `tracing::info!(target: "user::stdout", ...)` → `tracing::info!(...)`
- `tracing::info!(target: "user::stderr", ...)` → `tracing::info!(...)` (for status/progress messages)
- `tracing::info!(target: "user::stderr", ...)` → `tracing::warn!(...)` (for warning messages — check context)
- `tracing::info!(target: "user::stderr", ...)` → `tracing::error!(...)` (for error messages — check context)

Semantic mapping guide:
- **Success/info messages** (✓, ▶, data tables, values) → `tracing::info!(...)`
- **Progress/status** ("[hm] session...", "logged in as...", "submitted build...") → `tracing::info!(...)`
- **Warnings** ("!", "couldn't auto-open browser") → `tracing::warn!(...)`
- **Errors** ("✗", "hm: slug not running", error messages) → `tracing::error!(...)`

Files to modify:
- `crates/hm/src/output/status.rs` — will be deleted later, but fix for now so intermediate builds work
- `crates/hm/src/output/format.rs` — will be deleted later
- `crates/hm/src/commands/dev/up.rs`
- `crates/hm/src/commands/dev/down.rs`
- `crates/hm/src/commands/dev/exec.rs`
- `crates/hm/src/commands/dev/logs.rs`
- `crates/hm/src/commands/dev/port_of.rs`
- `crates/hm/src/commands/dev/ls.rs`
- `crates/hm/src/cli/version.rs`
- `crates/hm/src/cli/plugin.rs`
- `crates/hm/src/orchestrator/signal.rs`
- `crates/hm-plugin-cloud/src/cli.rs`
- `crates/hm-plugin-cloud/src/auth/login.rs`
- `crates/hm-plugin-cloud/src/auth/logout.rs`
- `crates/hm-plugin-cloud/src/auth/whoami.rs`
- `crates/hm-plugin-cloud/src/verbs/job.rs`
- `crates/hm-plugin-cloud/src/verbs/billing.rs`
- `crates/hm-plugin-cloud/src/verbs/run.rs`
- `crates/hm-plugin-cloud/src/verbs/org.rs`
- `crates/hm-plugin-cloud/src/verbs/build.rs`
- `crates/hm-plugin-cloud/src/verbs/pipeline.rs`

**Step 2: Verify no targets remain**

Run: `grep -rn 'target: "user' crates/ --include='*.rs'`
Expected: zero results

**Step 3: Verify build**

Run: `cargo check --workspace`
Expected: compiles

**Step 4: Commit**

```bash
git add crates/
git commit -m "refactor: strip target: annotations from all tracing calls"
```

---

## Task 6: Delete output/ Module + Dead Code Cleanup

**Files:**
- Delete: `crates/hm/src/output/cli_layer.rs`
- Delete: `crates/hm/src/output/format.rs`
- Delete: `crates/hm/src/output/status.rs`
- Delete: `crates/hm/src/output/human.rs`
- Delete: `crates/hm/src/output/json.rs`
- Delete: `crates/hm/src/output/spinner.rs`
- Delete: `crates/hm/src/output/mod.rs`
- Modify: `crates/hm/src/lib.rs`
- Modify: `crates/hm/src/context.rs`
- Modify: `crates/hm/src/output/mod.rs` macros removed (file deleted)

**Step 1: Remove `pub mod output;` from lib.rs**

In `crates/hm/src/lib.rs`, delete the line `pub mod output;`.

Also remove the stale `#[allow(clippy::print_stdout, clippy::print_stderr)]` on `pub mod cli;` if it still exists — that was for the old print-based output.

**Step 2: Remove OutputMode from context.rs**

`OutputMode` is dead code. Remove the import and field:

```rust
// DELETE: use crate::output::OutputMode;
// Change RunContext:
pub struct RunContext {
    pub config: Config,
    // DELETE: pub output: OutputMode,
}

// Change from_cli:
pub fn from_cli(cli: &Cli) -> Result<Self> {
    let config = Config::load()?;
    // DELETE: let output = OutputMode::Human { ... };
    Ok(Self { config })
}
```

**Step 3: Fix any `ctx.output` references**

Search for `ctx.output` — should be zero references (confirmed dead code). If any exist, delete them.

**Step 4: Delete all output/ files**

```bash
rm -rf crates/hm/src/output/
```

**Step 5: Remove unused dependencies from hm/Cargo.toml**

Check if these are still used after removing output/:
- `indicatif` — was only used by spinner.rs → **remove**
- `comfy-table` — grep to check usage → remove if unused
- `console` — used in main.rs for `Term::stderr().is_term()` → **keep**
- `owo-colors` — used in main.rs and commands/dev/logmux.rs → **keep**

Run: `grep -rn 'indicatif\|comfy.table' crates/hm/src/ --include='*.rs'` to verify.

**Step 6: Verify build**

Run: `cargo check --workspace && cargo test --workspace`
Expected: compiles and all tests pass (format tests now in fmt.rs)

**Step 7: Commit**

```bash
git add -A crates/hm/src/ crates/hm/Cargo.toml
git commit -m "refactor: delete output/ module and dead code"
```

---

## Task 7: Final Cleanup

**Files:**
- Modify: `Cargo.toml` (workspace root) — verify lints still deny print_stdout/print_stderr
- Modify: `crates/hm/src/lib.rs` — clean up any stale allows
- Possibly modify: any remaining files with compilation issues

**Step 1: Verify clippy is clean**

Run: `cargo clippy --workspace 2>&1 | grep -E '(error|print_stdout|print_stderr|unused)'`
Expected: no errors, no print lint violations

**Step 2: Verify full test suite**

Run: `cargo test --workspace`
Expected: all tests pass (except pre-existing `cmd_run_local_autoselect` Python failures)

**Step 3: Quick smoke test**

Run: `cargo run -- --version`
Expected: prints version to stderr (via tracing)

Run: `cargo run -- --verbose --version`
Expected: prints version with debug-level tracing visible

**Step 4: Commit if any fixes needed**

```bash
git add -A
git commit -m "refactor: final cleanup after output module removal"
```

---

## Notes

- **All output now goes to stderr** via tracing_subscriber::fmt. This is standard for CLI tools (stdout reserved for data, stderr for diagnostics). If stdout data output is needed later (piping `hm cloud job list | grep ...`), individual callsites can use `println!` with scoped `#[allow]`.
- **`owo_colors::set_override()`** in main.rs still controls ANSI color globally. The `tracing_subscriber::fmt` subscriber respects terminal color detection separately via its own ANSI support. Both should agree since they check similar signals. If colors are doubled or missing, consolidate to one mechanism.
- **HumanRenderer tests** are deleted. The build event formatting is now inline in output_subscriber. If needed, test via integration tests that run `hm run` and check stderr.
