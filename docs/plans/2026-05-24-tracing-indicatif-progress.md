# tracing-indicatif Pipeline Progress Bars

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace the default wall-of-text build output with indicatif progress bars that show parallel step execution at a glance, while still surfacing logs on failure.

**Architecture:** The existing `OutputRenderer` trait + `EventBus` stays untouched. A new `ProgressRenderer` implements `OutputRenderer` and bridges `BuildEvent`s into tracing spans that the `IndicatifLayer` renders as progress bars. A `--logs` flag opts into the legacy streaming output. On failure, buffered logs for failed steps are printed after progress bars are cleared.

**Tech Stack:** `tracing-indicatif 0.3`, `indicatif 0.18` (bump from 0.17), existing `tracing-subscriber 0.3` (already `^0.3.22`)

---

## Design Decisions

**Why bridge events→spans instead of instrumenting the scheduler?**
Keeps rendering concerns out of the orchestrator. The `ProgressRenderer` is just another `OutputRenderer` — same interface, different visualization. The scheduler emits events; the renderer interprets them.

**Why `--logs` instead of `--format verbose`?**
Simpler ergonomics. `--format` selects the serialization (human vs json). `--logs` is orthogonal — it controls whether you see streaming logs or progress bars within human mode. `--format json` always uses `JsonRenderer` regardless of `--logs`.

**Why buffer ALL logs and replay on failure?**
Buffering only failed-step logs would require knowing ahead of time which steps fail. We buffer everything, then discard on success. Pipeline runs are short-lived local processes — memory is not a concern.

**Why `with_ansi(false)` on the fmt layer even when indicatif is active?**
The fmt layer controls tracing *event* formatting (log lines). The `IndicatifLayer` handles its own ANSI for spinners/bars. Keeping `with_ansi(false)` on fmt means log lines stay plain, which is consistent with the current codebase (owo-colors was just removed). Indicatif progress bars still render with Unicode spinners.

---

## Task 1: Update Dependencies

**Files:**
- Modify: `crates/hm/Cargo.toml`
- Modify: `Cargo.toml` (workspace root — add `tracing-indicatif` to workspace deps)

**Step 1: Bump indicatif and add tracing-indicatif**

In `crates/hm/Cargo.toml`, change:
```toml
indicatif = "0.17"
```
to:
```toml
indicatif = "0.18"
tracing-indicatif = "0.3"
```

**Step 2: Verify compilation**

Run: `cargo check`
Expected: clean build. The `output/spinner.rs` API is stable across indicatif 0.17→0.18, but check for breakage. If `enable_steady_tick` signature changed, fix it.

**Step 3: Commit**

```bash
git add crates/hm/Cargo.toml Cargo.lock
git commit -m "chore: bump indicatif to 0.18, add tracing-indicatif"
```

---

## Task 2: Add `--logs` CLI Flag

**Files:**
- Modify: `crates/hm/src/cli/run.rs:4-40`

**Step 1: Add the flag**

In `RunArgs`, add after the `format` field (line 39):

```rust
/// Stream full build logs instead of showing progress bars.
/// Has no effect with `--format json`.
#[arg(long)]
pub logs: bool,
```

**Step 2: Verify compilation**

Run: `cargo check`
Expected: clean build, no consumers of `logs` yet.

**Step 3: Commit**

```bash
git add crates/hm/src/cli/run.rs
git commit -m "feat: add --logs flag to hm run"
```

---

## Task 3: Create ProgressRenderer (Skeleton)

**Files:**
- Create: `crates/hm/src/output/progress.rs`
- Modify: `crates/hm/src/output/mod.rs` (add `pub mod progress;`)

**Step 1: Write failing test**

Create `crates/hm/src/output/progress.rs`:

```rust
use std::collections::HashMap;
use std::fmt;
use std::io::Write;

use hm_plugin_protocol::BuildEvent;
use tracing::Span;
use uuid::Uuid;

use crate::runner::OutputRenderer;

/// Renders pipeline progress as indicatif progress bars.
///
/// In default mode (no `--logs`), shows a root progress bar tracking
/// overall step completion plus per-step spinners. Logs are buffered
/// silently; on failure, the failed step's logs are replayed to stderr.
pub struct ProgressRenderer<W> {
    out: W,
    root_span: Option<Span>,
    step_spans: HashMap<Uuid, Span>,
    step_keys: HashMap<Uuid, String>,
    log_buffer: HashMap<Uuid, Vec<String>>,
    failed_steps: Vec<(Uuid, i32)>,
}

impl<W> fmt::Debug for ProgressRenderer<W> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProgressRenderer")
            .field("steps_tracked", &self.step_spans.len())
            .field("logs_buffered", &self.log_buffer.values().map(Vec::len).sum::<usize>())
            .finish()
    }
}

impl<W> ProgressRenderer<W> {
    pub fn new(out: W) -> Self {
        Self {
            out,
            root_span: None,
            step_spans: HashMap::new(),
            step_keys: HashMap::new(),
            log_buffer: HashMap::new(),
            failed_steps: Vec::new(),
        }
    }
}

impl<W: Write + Send + fmt::Debug> OutputRenderer for ProgressRenderer<W> {
    fn on_event(&mut self, event: &BuildEvent) {
        // TODO: implement in Task 4 and Task 5
        let _ = event;
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use hm_plugin_protocol::{PlanSummary, StdStream};

    fn renderer() -> ProgressRenderer<Vec<u8>> {
        ProgressRenderer::new(Vec::new())
    }

    fn output(r: &ProgressRenderer<Vec<u8>>) -> String {
        String::from_utf8(r.out.clone()).unwrap()
    }

    #[test]
    fn buffers_logs_silently() {
        let mut r = renderer();
        let step_id = Uuid::new_v4();

        r.on_event(&BuildEvent::StepQueued {
            step_id,
            key: "build".into(),
            chain_idx: 0,
        });
        r.on_event(&BuildEvent::StepLog {
            step_id,
            stream: StdStream::Stdout,
            line: "compiling...".into(),
            ts: chrono::Utc::now(),
        });

        // No output written — logs are buffered, not streamed.
        assert!(output(&r).is_empty());
        assert_eq!(r.log_buffer[&step_id].len(), 1);
    }
}
```

**Step 2: Register the module**

In `crates/hm/src/output/mod.rs`, add:
```rust
pub mod progress;
```

**Step 3: Run test to verify it fails**

Run: `cargo test -p harmont-cli --lib output::progress::tests::buffers_logs_silently`
Expected: FAIL — `on_event` is a stub that doesn't buffer anything yet.

**Step 4: Commit**

```bash
git add crates/hm/src/output/progress.rs crates/hm/src/output/mod.rs
git commit -m "feat: add ProgressRenderer skeleton with failing test"
```

---

## Task 4: Implement Event→Span Bridging

**Files:**
- Modify: `crates/hm/src/output/progress.rs`

This task implements the core progress-bar visualization. Each `BuildEvent` is translated into tracing span operations that the `IndicatifLayer` renders.

**Step 1: Implement `on_event`**

Replace the `on_event` stub:

```rust
use indicatif::ProgressStyle;
use tracing::info_span;
use tracing_indicatif::span_ext::IndicatifSpanExt;

impl<W: Write + Send + fmt::Debug> OutputRenderer for ProgressRenderer<W> {
    fn on_event(&mut self, event: &BuildEvent) {
        match event {
            BuildEvent::BuildStart { plan, .. } => {
                let span = info_span!("pipeline");
                span.pb_set_style(
                    &ProgressStyle::with_template(
                        "{spinner} {span_name}  {wide_bar} {pos}/{len} steps  ({elapsed})",
                    )
                    .expect("static template"),
                );
                span.pb_set_length(plan.step_count as u64);
                span.pb_start();
                self.root_span = Some(span);
            }

            BuildEvent::StepQueued { step_id, key, .. } => {
                self.step_keys.insert(*step_id, key.clone());

                let span = if let Some(ref root) = self.root_span {
                    info_span!(parent: root, "step", name = %key)
                } else {
                    info_span!("step", name = %key)
                };
                span.pb_set_style(
                    &ProgressStyle::with_template(
                        "{span_child_prefix}{spinner} {span_fields}  {wide_msg}  ({elapsed})",
                    )
                    .expect("static template"),
                );
                span.pb_set_message("queued");
                span.pb_start();
                self.step_spans.insert(*step_id, span);
            }

            BuildEvent::StepStart {
                step_id,
                runner,
                image,
            } => {
                if let Some(span) = self.step_spans.get(step_id) {
                    let msg = image.as_ref().map_or_else(
                        || format!("running ({runner})"),
                        |img| format!("running ({runner}: {img})"),
                    );
                    span.pb_set_message(&msg);
                }
            }

            BuildEvent::StepLog { step_id, line, .. } => {
                self.log_buffer
                    .entry(*step_id)
                    .or_default()
                    .push(line.clone());
            }

            BuildEvent::StepCacheHit { step_id, .. } => {
                if let Some(span) = self.step_spans.get(step_id) {
                    span.pb_set_message("cached");
                }
            }

            BuildEvent::StepEnd {
                step_id,
                exit_code,
                duration_ms,
                ..
            } => {
                if *exit_code != 0 {
                    self.failed_steps.push((*step_id, *exit_code));
                }
                // Drop the step span — removes its progress bar.
                self.step_spans.remove(step_id);
                // Advance the root bar.
                if let Some(ref root) = self.root_span {
                    root.pb_inc(1);
                }
            }

            BuildEvent::ChainFailed { .. } => {}

            BuildEvent::BuildEnd {
                exit_code,
                duration_ms,
            } => {
                // Clear all progress bars.
                self.step_spans.clear();
                self.root_span.take();

                if *exit_code != 0 {
                    self.print_failure_report();
                }
            }
        }
    }
}
```

**Step 2: Implement `print_failure_report`**

Add this method to the `impl<W>` block:

```rust
impl<W: Write> ProgressRenderer<W> {
    fn print_failure_report(&mut self) {
        for (step_id, exit_code) in &self.failed_steps {
            let key = self.step_keys.get(step_id).map_or("?", String::as_str);
            let _ = writeln!(self.out, "\n--- {key} failed (exit {exit_code}) ---");
            if let Some(lines) = self.log_buffer.get(step_id) {
                for line in lines {
                    let _ = writeln!(self.out, "{line}");
                }
            }
        }
    }
}
```

**Step 3: Run the test from Task 3**

Run: `cargo test -p harmont-cli --lib output::progress::tests::buffers_logs_silently`
Expected: PASS — `StepLog` events now buffer into `log_buffer`.

**Step 4: Add failure-replay test**

Append to the `tests` module:

```rust
#[test]
fn replays_logs_on_failure() {
    let mut r = renderer();
    let step_id = Uuid::new_v4();

    r.on_event(&BuildEvent::BuildStart {
        run_id: Uuid::nil(),
        plan: PlanSummary {
            step_count: 1,
            chain_count: 1,
            default_runner: "docker".into(),
        },
        started_at: chrono::Utc::now(),
    });
    r.on_event(&BuildEvent::StepQueued {
        step_id,
        key: "test".into(),
        chain_idx: 0,
    });
    r.on_event(&BuildEvent::StepLog {
        step_id,
        stream: StdStream::Stderr,
        line: "FAIL src/app.test.ts".into(),
        ts: chrono::Utc::now(),
    });
    r.on_event(&BuildEvent::StepEnd {
        step_id,
        exit_code: 1,
        duration_ms: 500,
        snapshot: None,
    });
    r.on_event(&BuildEvent::BuildEnd {
        exit_code: 1,
        duration_ms: 600,
    });

    let s = output(&r);
    assert!(s.contains("test failed (exit 1)"), "got: {s}");
    assert!(s.contains("FAIL src/app.test.ts"), "got: {s}");
}

#[test]
fn no_output_on_success() {
    let mut r = renderer();
    let step_id = Uuid::new_v4();

    r.on_event(&BuildEvent::BuildStart {
        run_id: Uuid::nil(),
        plan: PlanSummary {
            step_count: 1,
            chain_count: 1,
            default_runner: "docker".into(),
        },
        started_at: chrono::Utc::now(),
    });
    r.on_event(&BuildEvent::StepQueued {
        step_id,
        key: "build".into(),
        chain_idx: 0,
    });
    r.on_event(&BuildEvent::StepLog {
        step_id,
        stream: StdStream::Stdout,
        line: "compiling...".into(),
        ts: chrono::Utc::now(),
    });
    r.on_event(&BuildEvent::StepEnd {
        step_id,
        exit_code: 0,
        duration_ms: 3000,
        snapshot: None,
    });
    r.on_event(&BuildEvent::BuildEnd {
        exit_code: 0,
        duration_ms: 3100,
    });

    // On success, no text output (progress bars handle everything).
    assert!(output(&r).is_empty());
}
```

**Step 5: Run all progress tests**

Run: `cargo test -p harmont-cli --lib output::progress`
Expected: all 3 tests PASS.

**Step 6: Commit**

```bash
git add crates/hm/src/output/progress.rs
git commit -m "feat: implement ProgressRenderer event→span bridging"
```

---

## Task 5: Conditional Subscriber Setup in main.rs

**Files:**
- Modify: `crates/hm/src/main.rs:1-40`

The tracing subscriber is global and set once. When `hm run` is invoked without `--logs` and without `--format json`, we install the `IndicatifLayer` alongside the fmt layer. All other commands get the plain fmt subscriber.

**Step 1: Refactor main.rs subscriber setup**

Replace the current subscriber block with:

```rust
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

// Detect whether we need the indicatif progress-bar layer.
let use_indicatif = matches!(
    &args.command,
    cli::Command::Run(ref r) if !r.logs && r.format != "json"
);

let default_level = if args.verbose { "debug" } else { "info" };
let filter =
    EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_level));

if use_indicatif {
    let indicatif_layer = tracing_indicatif::IndicatifLayer::new();

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(indicatif_layer.get_stderr_writer())
                .with_target(false)
                .without_time()
                .with_ansi(false)
                .with_filter(filter),
        )
        .with(indicatif_layer)
        .init();
} else {
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .without_time()
        .with_ansi(false)
        .init();
}
```

Note: the `EnvFilter` is applied only to the fmt layer (via `.with_filter(filter)`), not to the indicatif layer. This means `RUST_LOG=warn` hides log lines but doesn't suppress progress bars.

**Step 2: Verify compilation**

Run: `cargo check`
Expected: clean build. May need to add `use tracing_subscriber::Layer;` for `.with_filter()` on the fmt layer.

**Step 3: Verify existing behavior unchanged**

Run: `cargo build && ./target/debug/hm version`
Expected: prints version without crash. The `use_indicatif` branch is only taken for `Command::Run` without `--logs`.

**Step 4: Commit**

```bash
git add crates/hm/src/main.rs
git commit -m "feat: conditional IndicatifLayer in subscriber setup"
```

---

## Task 6: Wire Up Renderer Selection

**Files:**
- Modify: `crates/hm/src/commands/run/local.rs:97-104`

**Step 1: Update renderer construction**

Replace the renderer block:

```rust
let renderer: Box<dyn crate::runner::OutputRenderer> = match args.format.as_str() {
    "json" => Box::new(crate::output::json::JsonRenderer::new(std::io::stdout())),
    _ if args.logs => Box::new(crate::output::human::HumanRenderer::new(std::io::stderr())),
    _ => Box::new(crate::output::progress::ProgressRenderer::new(std::io::stderr())),
};
```

Logic:
- `--format json` → `JsonRenderer` (machine output, unchanged)
- `--logs` → `HumanRenderer` (legacy streaming logs)
- Default → `ProgressRenderer` (progress bars)

**Step 2: Verify compilation**

Run: `cargo check`
Expected: clean build.

**Step 3: Commit**

```bash
git add crates/hm/src/commands/run/local.rs
git commit -m "feat: wire ProgressRenderer as default for hm run"
```

---

## Task 7: Manual Testing

This task cannot be unit-tested — it requires a running Docker daemon and a real pipeline.

**Step 1: Build the binary**

Run: `cargo build`

**Step 2: Test progress-bar mode (default)**

Run: `./target/debug/hm run` (in a repo with `.harmont/*.py` pipelines)

Expected:
- A root progress bar appears: `⠙ pipeline  ████░░░░░░ 2/5 steps  (3.2s)`
- Per-step spinners appear nested underneath as steps start
- Spinners disappear as steps complete
- On success: all bars clear, clean exit
- No streaming log lines visible

**Step 3: Test failure log replay**

Induce a failure (e.g., a step with `exit 1`), then run `./target/debug/hm run`.

Expected:
- Progress bars show until failure
- After `BuildEnd`, progress bars clear
- Failure report printed: step name, exit code, full logs

**Step 4: Test `--logs` mode**

Run: `./target/debug/hm run --logs`

Expected: identical to old behavior — streaming `[step] line` output, no progress bars.

**Step 5: Test `--format json`**

Run: `./target/debug/hm run --format json | head -20`

Expected: JSON lines on stdout, no progress bars, unchanged from before.

**Step 6: Test non-run commands unaffected**

Run: `./target/debug/hm version`

Expected: plain output, no progress bar artifacts.

---

## File Summary

| File | Action | Purpose |
|------|--------|---------|
| `crates/hm/Cargo.toml` | Modify | Bump indicatif 0.17→0.18, add tracing-indicatif |
| `crates/hm/src/cli/run.rs` | Modify | Add `--logs` flag |
| `crates/hm/src/output/mod.rs` | Modify | Register `progress` module |
| `crates/hm/src/output/progress.rs` | Create | `ProgressRenderer` — event→span bridge + log buffering |
| `crates/hm/src/main.rs` | Modify | Conditional `IndicatifLayer` subscriber setup |
| `crates/hm/src/commands/run/local.rs` | Modify | Renderer selection based on `--logs` |
| `crates/hm/src/output/spinner.rs` | Maybe modify | If indicatif 0.18 breaks the API (unlikely) |
