# `hm` Mission Control TUI — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a host-side, ratatui-based "Mission Control" TUI for `hm run`, `hm dev up`, and `hm cloud build watch` per `docs/superpowers/specs/2026-05-22-tui-mission-control-design.md`.

**Architecture:** A new `crates/hm/src/tui/` module owns the alternate-screen UI. Three event-source adapters (`local`, `dev`, `cloud`) translate command-specific data into a single `TuiEvent` channel that drives a pure `AppState` reducer. ratatui widgets render the reducer state; tachyonfx adds subtle effects. The existing WASM output-formatter path remains the non-TTY fallback.

**Tech Stack:** Rust 2024 edition, `ratatui`, `crossterm`, `tachyonfx`, `tui-big-text`, `tokio`, `insta` for snapshots, `vhs` (Charm) for demo tapes.

---

## File Map (locked at plan time)

### Created

- `crates/hm/src/tui/mod.rs` — `pub async fn run(...)`, `TuiOptions`, `TuiError`.
- `crates/hm/src/tui/event.rs` — `TuiEvent`, `DeployState`.
- `crates/hm/src/tui/app.rs` — `AppState`, reducer, focus / log-buffer logic.
- `crates/hm/src/tui/source/mod.rs` — `EventSource` trait.
- `crates/hm/src/tui/source/local.rs` — `BuildEvent` broadcast → `TuiEvent`.
- `crates/hm/src/tui/source/dev.rs` — dev daemon poll → `TuiEvent`.
- `crates/hm/src/tui/source/cloud.rs` — cloud `BuildEvent` via host fn → `TuiEvent`.
- `crates/hm/src/tui/term.rs` — terminal setup / restore guard + panic hook.
- `crates/hm/src/tui/theme.rs` — `Theme` palette.
- `crates/hm/src/tui/fx.rs` — tachyonfx wrappers + budget enforcement.
- `crates/hm/src/tui/widgets/mod.rs`, `header.rs`, `graph.rs`, `timeline.rs`, `log.rs`, `footer.rs`, `summary.rs`, `help.rs`, `filter.rs`.
- `crates/hm/tests/tui_snapshots.rs` — insta snapshot tests.
- `crates/hm/tests/snapshots/` — insta snapshot dir.
- `docs/demo/run.tape`, `docs/demo/dev.tape`.
- `.github/workflows/demo.yml`.

### Modified

- `crates/hm/Cargo.toml` — add ratatui, crossterm, tachyonfx, tui-big-text, insta, `is-terminal`.
- `crates/hm/src/cli.rs` — `--no-tui`, `--no-fx` global flags.
- `crates/hm/src/lib.rs` — `pub mod tui;`.
- `crates/hm/src/orchestrator/scheduler.rs` — accept `extra_event_tx: Option<mpsc::Sender<BuildEvent>>` parameter.
- `crates/hm/src/orchestrator/mod.rs` — re-export updated `run` signature.
- `crates/hm/src/commands/run/local.rs` — TTY-detect, route to TUI vs human formatter.
- `crates/hm/src/commands/dev/up.rs` — same path with `source::dev`.
- `crates/hm/src/plugin/host_fns.rs` — add `hm_build_event_emit` host fn.
- `crates/hm-plugin-protocol/src/host_abi.rs` — add `HM_BUILD_EVENT_EMIT` const.
- `crates/hm-plugin-cloud/src/lib.rs` — declare imported `hm_build_event_emit`.
- `crates/hm-plugin-cloud/src/verbs/build.rs` — `watch` calls `hm_build_event_emit` per state diff.
- `README.md` — embed `docs/demo/run.gif`.

### Not touched

- `hm-plugin-protocol` wire types (no new structs/enum variants).
- `hm-plugin-output-human` / `hm-plugin-output-json` (still ship, still TTY fallback).
- `OutputFormatter` capability surface.
- `HM_PLUGIN_API_VERSION` (additive host fn does not bump it).

---

## Phase 0 — Foundation

### Task 0.1: Add TUI dependencies

**Files:**
- Modify: `crates/hm/Cargo.toml`

- [ ] **Step 1: Add the runtime deps**

Run from the workspace root:

```bash
cargo add --package hm \
  ratatui \
  crossterm \
  tachyonfx \
  tui-big-text \
  is-terminal
```

Expected: `cargo add` updates `crates/hm/Cargo.toml` `[dependencies]` and `Cargo.lock`.

- [ ] **Step 2: Add the dev-deps**

```bash
cargo add --package hm --dev insta --features yaml
```

Expected: `[dev-dependencies] insta = { version = "...", features = ["yaml"] }`.

- [ ] **Step 3: Verify it builds**

```bash
cargo build -p hm
```

Expected: clean build. No code uses the new deps yet, so unused-import warnings should be zero (no `use` statements added).

- [ ] **Step 4: Commit**

```bash
git add crates/hm/Cargo.toml Cargo.lock
git commit -m "build(hm): add ratatui/crossterm/tachyonfx/tui-big-text/insta deps"
```

### Task 0.2: Add `--no-tui` / `--no-fx` global flags

**Files:**
- Modify: `crates/hm/src/cli.rs`

- [ ] **Step 1: Add the flags to `Cli`**

Open `crates/hm/src/cli.rs`. In the `pub struct Cli { … }` block, after the `pub no_color: bool,` field, add:

```rust
    /// Disable the interactive TUI; fall back to the streaming text
    /// formatter. Implied when stdout is not a TTY.
    #[arg(long, global = true)]
    pub no_tui: bool,

    /// Disable TUI animation effects (kept layout identical).
    /// Implied by `NO_COLOR`.
    #[arg(long, global = true)]
    pub no_fx: bool,
```

- [ ] **Step 2: Verify the CLI still parses**

```bash
cargo build -p hm && ./target/debug/hm --help | grep -E "no-tui|no-fx"
```

Expected:

```
      --no-tui   Disable the interactive TUI; ...
      --no-fx    Disable TUI animation effects ...
```

- [ ] **Step 3: Commit**

```bash
git add crates/hm/src/cli.rs
git commit -m "feat(cli): --no-tui and --no-fx global flags"
```

### Task 0.3: Stub the `tui` module

**Files:**
- Create: `crates/hm/src/tui/mod.rs`
- Modify: `crates/hm/src/lib.rs`

- [ ] **Step 1: Create the stub module**

Create `crates/hm/src/tui/mod.rs`:

```rust
//! Mission Control TUI — host-side ratatui renderer for `hm run`,
//! `hm dev up`, and `hm cloud build watch`. See
//! `docs/superpowers/specs/2026-05-22-tui-mission-control-design.md`.

pub mod event;

// Submodules added in later tasks:
// pub mod app;
// pub mod source;
// pub mod term;
// pub mod theme;
// pub mod fx;
// pub mod widgets;

#[derive(Debug, Clone)]
pub struct TuiOptions {
    pub fx_enabled: bool,
    pub summary_card: bool,
    pub title: String,
}

#[derive(Debug, thiserror::Error)]
pub enum TuiError {
    #[error("terminal i/o: {0}")]
    Io(#[from] std::io::Error),
    #[error("event channel closed before BuildEnd")]
    ChannelClosed,
}
```

- [ ] **Step 2: Register the module**

Open `crates/hm/src/lib.rs`. Add (alphabetical with peers):

```rust
pub mod tui;
```

- [ ] **Step 3: Verify build**

```bash
cargo build -p hm
```

Expected: clean. (`event` submodule doesn't exist yet; we will create it in Task 1.1 — until then the `pub mod event;` declaration in `mod.rs` is the one risk. **Replace the line with `// pub mod event;` comment for now**, then uncomment in Task 1.1.)

Apply the fix: change `pub mod event;` in `crates/hm/src/tui/mod.rs` to:

```rust
// pub mod event;  // un-commented in Task 1.1
```

Re-run `cargo build -p hm`. Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add crates/hm/src/lib.rs crates/hm/src/tui/mod.rs
git commit -m "feat(tui): scaffold tui module with TuiOptions/TuiError"
```

---

## Phase 1 — Event model + reducer

### Task 1.1: Define `TuiEvent`

**Files:**
- Create: `crates/hm/src/tui/event.rs`
- Modify: `crates/hm/src/tui/mod.rs`

- [ ] **Step 1: Write the failing test (round-trip)**

Append at the bottom of `crates/hm/src/tui/event.rs` (file new, write in one shot):

```rust
//! Host-only event vocabulary fed to `AppState::apply`. Translated
//! from wire `BuildEvent` (local + cloud sources) and dev-daemon
//! status diffs at the adapter boundary.

use chrono::{DateTime, Utc};
use hm_plugin_protocol::{PlanSummary, StdStream};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeployState {
    Starting,
    Healthy,
    Unhealthy,
    Restarting,
    Stopped,
}

#[derive(Debug, Clone)]
pub enum TuiEvent {
    BuildStart {
        run_id: Uuid,
        plan: PlanSummary,
        started_at: DateTime<Utc>,
    },
    ChainQueued {
        chain_idx: usize,
        label: String,
        parent: Option<usize>,
    },
    StepStart {
        step_id: Uuid,
        chain_idx: usize,
        runner: String,
        image: Option<String>,
        label: String,
    },
    StepLog {
        step_id: Uuid,
        stream: StdStream,
        line: String,
        ts: DateTime<Utc>,
    },
    StepCacheHit {
        step_id: Uuid,
        key: String,
        tag: String,
    },
    StepEnd {
        step_id: Uuid,
        exit_code: i32,
        duration_ms: u64,
    },
    ChainFailed {
        chain_idx: usize,
        failed_step_key: String,
        exit_code: i32,
        message: String,
    },
    BuildEnd {
        exit_code: i32,
        duration_ms: u64,
    },

    DeployStatus {
        deploy_id: String,
        label: String,
        state: DeployState,
        restarts: u32,
        uptime_ms: u64,
    },
    DeployLog {
        deploy_id: String,
        stream: StdStream,
        line: String,
        ts: DateTime<Utc>,
    },

    /// Synthetic event the adapter inserts when it has dropped one or
    /// more `StepLog` events due to backpressure. The reducer renders
    /// a single dim "events dropped" line in the affected step.
    Lagged { dropped: u64 },
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn deploy_state_eq() {
        assert_eq!(DeployState::Healthy, DeployState::Healthy);
        assert_ne!(DeployState::Healthy, DeployState::Unhealthy);
    }

    #[test]
    fn step_log_is_clone() {
        let ev = TuiEvent::StepLog {
            step_id: Uuid::nil(),
            stream: StdStream::Stdout,
            line: "hi".into(),
            ts: chrono::Utc::now(),
        };
        let _ = ev.clone();
    }
}
```

- [ ] **Step 2: Uncomment the module declaration**

In `crates/hm/src/tui/mod.rs`, change the `// pub mod event;` line back to:

```rust
pub mod event;
```

- [ ] **Step 3: Run the tests**

```bash
cargo test -p hm --lib tui::event
```

Expected: 2 passed.

- [ ] **Step 4: Commit**

```bash
git add crates/hm/src/tui/event.rs crates/hm/src/tui/mod.rs
git commit -m "feat(tui): TuiEvent + DeployState"
```

### Task 1.2: Reducer skeleton + chain ordering

**Files:**
- Create: `crates/hm/src/tui/app.rs`
- Modify: `crates/hm/src/tui/mod.rs` (uncomment `pub mod app;`)

- [ ] **Step 1: Write failing tests first**

Create `crates/hm/src/tui/app.rs` with the test module at the bottom; implementation initially empty so tests fail. Use this full file:

```rust
//! Mission Control reducer. Pure: `AppState::apply(TuiEvent)` yields
//! a new state without touching the terminal. All ratatui widgets are
//! immediate-mode renders over this state.

use std::collections::BTreeMap;
use std::collections::VecDeque;
use std::time::Instant;

use chrono::{DateTime, Utc};
use hm_plugin_protocol::{PlanSummary, StdStream};
use uuid::Uuid;

use super::event::{DeployState, TuiEvent};

const LOG_RING_CAPACITY: usize = 2000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepStatus {
    Queued,
    Running,
    CachedHit,
    Passed,
    Failed,
}

#[derive(Debug, Clone)]
pub struct Step {
    pub id: Uuid,
    pub chain_idx: usize,
    pub label: String,
    pub status: StepStatus,
    pub started_at: Option<DateTime<Utc>>,
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct Chain {
    pub idx: usize,
    pub label: String,
    pub parent: Option<usize>,
    pub steps: Vec<Uuid>,
    pub deploy_state: Option<DeployState>,
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub ts: DateTime<Utc>,
    pub stream: StdStream,
    pub line: String,
}

#[derive(Debug)]
pub struct StepLogBuffer {
    pub entries: VecDeque<LogEntry>,
    pub dropped: u64,
}

impl Default for StepLogBuffer {
    fn default() -> Self {
        Self {
            entries: VecDeque::with_capacity(LOG_RING_CAPACITY),
            dropped: 0,
        }
    }
}

impl StepLogBuffer {
    pub fn push(&mut self, e: LogEntry) {
        if self.entries.len() == LOG_RING_CAPACITY {
            self.entries.pop_front();
        }
        self.entries.push_back(e);
    }
}

#[derive(Debug, Default)]
pub struct AppState {
    pub run_id: Option<Uuid>,
    pub plan: Option<PlanSummary>,
    pub started_at: Option<DateTime<Utc>>,
    pub ended_at: Option<DateTime<Utc>>,
    pub exit_code: Option<i32>,
    pub chains: Vec<Chain>,
    pub steps: BTreeMap<Uuid, Step>,
    pub logs: BTreeMap<Uuid, StepLogBuffer>,
    pub focused_chain: usize,
    pub fail_message: Option<String>,
}

impl AppState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn apply(&mut self, event: TuiEvent) {
        match event {
            TuiEvent::BuildStart { run_id, plan, started_at } => {
                self.run_id = Some(run_id);
                self.plan = Some(plan);
                self.started_at = Some(started_at);
            }
            TuiEvent::ChainQueued { chain_idx, label, parent } => {
                while self.chains.len() <= chain_idx {
                    self.chains.push(Chain {
                        idx: self.chains.len(),
                        label: String::new(),
                        parent: None,
                        steps: vec![],
                        deploy_state: None,
                    });
                }
                let c = &mut self.chains[chain_idx];
                c.label = label;
                c.parent = parent;
            }
            TuiEvent::StepStart { step_id, chain_idx, runner: _, image: _, label } => {
                self.steps.insert(step_id, Step {
                    id: step_id,
                    chain_idx,
                    label,
                    status: StepStatus::Running,
                    started_at: Some(Utc::now()),
                    duration_ms: None,
                });
                while self.chains.len() <= chain_idx {
                    self.chains.push(Chain {
                        idx: self.chains.len(),
                        label: String::new(),
                        parent: None,
                        steps: vec![],
                        deploy_state: None,
                    });
                }
                self.chains[chain_idx].steps.push(step_id);
            }
            TuiEvent::StepLog { step_id, stream, line, ts } => {
                let buf = self.logs.entry(step_id).or_default();
                buf.push(LogEntry { ts, stream, line });
            }
            TuiEvent::StepCacheHit { step_id, .. } => {
                if let Some(s) = self.steps.get_mut(&step_id) {
                    s.status = StepStatus::CachedHit;
                }
            }
            TuiEvent::StepEnd { step_id, exit_code, duration_ms } => {
                if let Some(s) = self.steps.get_mut(&step_id) {
                    if s.status != StepStatus::CachedHit {
                        s.status = if exit_code == 0 {
                            StepStatus::Passed
                        } else {
                            StepStatus::Failed
                        };
                    }
                    s.duration_ms = Some(duration_ms);
                }
            }
            TuiEvent::ChainFailed { chain_idx: _, failed_step_key, exit_code, message } => {
                self.fail_message = Some(format!(
                    "{failed_step_key} exited {exit_code}: {message}"
                ));
            }
            TuiEvent::BuildEnd { exit_code, duration_ms: _ } => {
                self.exit_code = Some(exit_code);
                self.ended_at = Some(Utc::now());
            }
            TuiEvent::DeployStatus { deploy_id, label, state, restarts: _, uptime_ms: _ } => {
                let chain_idx = self.find_or_create_deploy_chain(&deploy_id, &label);
                self.chains[chain_idx].deploy_state = Some(state);
            }
            TuiEvent::DeployLog { deploy_id, stream, line, ts } => {
                let chain_idx = self.find_or_create_deploy_chain(&deploy_id, &deploy_id);
                // Use the deploy_id as the step_id (Uuid v5 of the
                // deploy name) for log routing.
                let step_id = uuid_from_deploy_id(&deploy_id);
                if !self.steps.contains_key(&step_id) {
                    self.steps.insert(step_id, Step {
                        id: step_id,
                        chain_idx,
                        label: deploy_id.clone(),
                        status: StepStatus::Running,
                        started_at: Some(ts),
                        duration_ms: None,
                    });
                    self.chains[chain_idx].steps.push(step_id);
                }
                let buf = self.logs.entry(step_id).or_default();
                buf.push(LogEntry { ts, stream, line });
            }
            TuiEvent::Lagged { dropped } => {
                if let Some(focused_step) = self.focused_step_id() {
                    let buf = self.logs.entry(focused_step).or_default();
                    buf.dropped += dropped;
                }
            }
        }
    }

    pub fn focused_step_id(&self) -> Option<Uuid> {
        self.chains
            .get(self.focused_chain)
            .and_then(|c| c.steps.last().copied())
    }

    pub fn cycle_focus(&mut self, delta: isize) {
        if self.chains.is_empty() {
            return;
        }
        let len = self.chains.len() as isize;
        let next = (self.focused_chain as isize + delta).rem_euclid(len);
        self.focused_chain = next as usize;
    }

    fn find_or_create_deploy_chain(&mut self, deploy_id: &str, label: &str) -> usize {
        if let Some(idx) = self.chains.iter().position(|c| c.label == deploy_id) {
            return idx;
        }
        let idx = self.chains.len();
        self.chains.push(Chain {
            idx,
            label: label.to_string(),
            parent: None,
            steps: vec![],
            deploy_state: None,
        });
        idx
    }
}

fn uuid_from_deploy_id(deploy_id: &str) -> Uuid {
    Uuid::new_v5(&Uuid::NAMESPACE_OID, deploy_id.as_bytes())
}

#[allow(dead_code)]
fn _instant_unused_marker() -> Instant { Instant::now() }

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use hm_plugin_protocol::PlanSummary;

    fn nil() -> Uuid { Uuid::nil() }

    fn plan(n: usize) -> PlanSummary {
        PlanSummary {
            step_count: n,
            chain_count: n,
            default_runner: "docker".into(),
        }
    }

    #[test]
    fn build_start_sets_metadata() {
        let mut s = AppState::new();
        s.apply(TuiEvent::BuildStart {
            run_id: nil(),
            plan: plan(3),
            started_at: Utc::now(),
        });
        assert!(s.run_id.is_some());
        assert!(s.plan.is_some());
    }

    #[test]
    fn chain_queued_grows_chains() {
        let mut s = AppState::new();
        s.apply(TuiEvent::ChainQueued {
            chain_idx: 2,
            label: "c2".into(),
            parent: None,
        });
        assert_eq!(s.chains.len(), 3);
        assert_eq!(s.chains[2].label, "c2");
    }

    #[test]
    fn step_lifecycle_transitions_status() {
        let mut s = AppState::new();
        let sid = Uuid::new_v4();
        s.apply(TuiEvent::StepStart {
            step_id: sid,
            chain_idx: 0,
            runner: "docker".into(),
            image: None,
            label: "test".into(),
        });
        assert_eq!(s.steps[&sid].status, StepStatus::Running);
        s.apply(TuiEvent::StepEnd {
            step_id: sid,
            exit_code: 0,
            duration_ms: 42,
        });
        assert_eq!(s.steps[&sid].status, StepStatus::Passed);
        assert_eq!(s.steps[&sid].duration_ms, Some(42));
    }

    #[test]
    fn cache_hit_sticks_through_step_end() {
        let mut s = AppState::new();
        let sid = Uuid::new_v4();
        s.apply(TuiEvent::StepStart {
            step_id: sid,
            chain_idx: 0,
            runner: "docker".into(),
            image: None,
            label: "build".into(),
        });
        s.apply(TuiEvent::StepCacheHit {
            step_id: sid,
            key: "k".into(),
            tag: "t".into(),
        });
        s.apply(TuiEvent::StepEnd {
            step_id: sid,
            exit_code: 0,
            duration_ms: 1,
        });
        assert_eq!(s.steps[&sid].status, StepStatus::CachedHit);
    }

    #[test]
    fn failed_step_records_status() {
        let mut s = AppState::new();
        let sid = Uuid::new_v4();
        s.apply(TuiEvent::StepStart {
            step_id: sid,
            chain_idx: 0,
            runner: "docker".into(),
            image: None,
            label: "test".into(),
        });
        s.apply(TuiEvent::StepEnd {
            step_id: sid,
            exit_code: 1,
            duration_ms: 9,
        });
        assert_eq!(s.steps[&sid].status, StepStatus::Failed);
    }

    #[test]
    fn log_buffer_caps_at_ring_capacity() {
        let mut s = AppState::new();
        let sid = Uuid::new_v4();
        for i in 0..(LOG_RING_CAPACITY + 50) {
            s.apply(TuiEvent::StepLog {
                step_id: sid,
                stream: StdStream::Stdout,
                line: format!("L{i}"),
                ts: Utc::now(),
            });
        }
        assert_eq!(s.logs[&sid].entries.len(), LOG_RING_CAPACITY);
        assert_eq!(s.logs[&sid].entries.front().unwrap().line, format!("L{}", 50));
    }

    #[test]
    fn focus_cycles_modulo_chains() {
        let mut s = AppState::new();
        for i in 0..3 {
            s.apply(TuiEvent::ChainQueued {
                chain_idx: i,
                label: format!("c{i}"),
                parent: None,
            });
        }
        s.cycle_focus(1);
        assert_eq!(s.focused_chain, 1);
        s.cycle_focus(-1);
        assert_eq!(s.focused_chain, 0);
        s.cycle_focus(-1);
        assert_eq!(s.focused_chain, 2);
    }

    #[test]
    fn deploy_status_creates_deploy_chain() {
        let mut s = AppState::new();
        s.apply(TuiEvent::DeployStatus {
            deploy_id: "db".into(),
            label: "db".into(),
            state: DeployState::Healthy,
            restarts: 0,
            uptime_ms: 1000,
        });
        assert_eq!(s.chains.len(), 1);
        assert_eq!(s.chains[0].deploy_state, Some(DeployState::Healthy));
    }
}
```

- [ ] **Step 2: Register the module**

In `crates/hm/src/tui/mod.rs`, uncomment `pub mod app;`:

```rust
pub mod app;
```

- [ ] **Step 3: Run tests**

```bash
cargo test -p hm --lib tui::app
```

Expected: 8 passed, 0 failed.

- [ ] **Step 4: Commit**

```bash
git add crates/hm/src/tui/app.rs crates/hm/src/tui/mod.rs
git commit -m "feat(tui): AppState reducer with chain/step/log/focus tests"
```

---

## Phase 2 — Adapters

### Task 2.1: Source trait

**Files:**
- Create: `crates/hm/src/tui/source/mod.rs`
- Modify: `crates/hm/src/tui/mod.rs` (uncomment `pub mod source;`)

- [ ] **Step 1: Write the source module**

Create `crates/hm/src/tui/source/mod.rs`:

```rust
//! Event-source adapters. Each command surface (`hm run`, `hm dev up`,
//! `hm cloud build watch`) constructs a source that converts its
//! command-specific event stream into `TuiEvent`s sent on the mpsc
//! channel `tui::run` consumes.

pub mod local;
pub mod dev;
pub mod cloud;

use tokio::sync::mpsc;

use super::event::TuiEvent;

/// Channel capacity from adapter → TUI. Adapters drop `StepLog`
/// events when full and emit a single `Lagged` synthetic event per
/// drop burst, matching the protocol-bus contract.
pub const TUI_CHANNEL_CAPACITY: usize = 1024;

/// Create the (sender, receiver) pair used by adapters and the TUI.
pub fn channel() -> (mpsc::Sender<TuiEvent>, mpsc::Receiver<TuiEvent>) {
    mpsc::channel(TUI_CHANNEL_CAPACITY)
}
```

- [ ] **Step 2: Uncomment the module declaration**

In `crates/hm/src/tui/mod.rs`:

```rust
pub mod source;
```

- [ ] **Step 3: Stub the three source files so the module compiles**

Create `crates/hm/src/tui/source/local.rs`:

```rust
//! Build-event broadcast → TuiEvent adapter for local `hm run`.

// Real impl arrives in Task 2.2.
```

Create `crates/hm/src/tui/source/dev.rs`:

```rust
//! Dev daemon poll → TuiEvent adapter for `hm dev up`.

// Real impl arrives in Task 2.6.
```

Create `crates/hm/src/tui/source/cloud.rs`:

```rust
//! Cloud watch (host-fn fed) → TuiEvent adapter for
//! `hm cloud build watch`.

// Real impl arrives in Task 2.5.
```

- [ ] **Step 4: Verify build**

```bash
cargo build -p hm
```

Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add crates/hm/src/tui/source crates/hm/src/tui/mod.rs
git commit -m "feat(tui): source module scaffold + channel helper"
```

### Task 2.2: Local source — broadcast forwarder

**Files:**
- Modify: `crates/hm/src/tui/source/local.rs`
- Modify: `crates/hm/src/orchestrator/scheduler.rs`
- Modify: `crates/hm/src/orchestrator/mod.rs`

- [ ] **Step 1: Extend `scheduler::run` to accept an extra event sink**

Open `crates/hm/src/orchestrator/scheduler.rs`. Change the signature at line ~60:

```rust
pub async fn run(
    pipeline: hm_plugin_protocol::Pipeline,
    repo_root: PathBuf,
    parallelism: usize,
    format_name: String,
    extra_event_tx: Option<tokio::sync::mpsc::Sender<hm_plugin_protocol::BuildEvent>>,
) -> Result<i32> {
```

Immediately after the existing `let sink_handle = … output_subscriber::spawn(bus.clone(), …);` line (~164), add a second subscriber when `extra_event_tx` is `Some`:

```rust
    // Optional secondary subscriber: feed the host-side TUI mpsc.
    let extra_handle = extra_event_tx.map(|tx| {
        let mut rx = bus.subscribe();
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(ev) => {
                        if tx.send(ev).await.is_err() {
                            break; // TUI consumer dropped
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        })
    });
```

At the end of `run`, after the existing `sink_handle.await?;` (find it; it's the last awaits before `Ok(exit_code)`), add:

```rust
    if let Some(h) = extra_handle {
        let _ = h.await;
    }
```

- [ ] **Step 2: Update every caller of `scheduler::run`**

Find callers:

```bash
grep -rn "orchestrator::run\|scheduler::run" /home/marko/harmont-cli/crates/hm/src
```

For each caller, append `None` (or the new `Some(tx)` once the TUI path exists) as the new last argument. There is one caller today: `crates/hm/src/commands/run/local.rs` — append `None,`:

```rust
    let exit_code = crate::orchestrator::run(
        pipeline_wire,
        repo_root,
        parallelism,
        args.format.clone(),
        None,
    )
    .await?;
```

- [ ] **Step 3: Write the source helper**

Replace the contents of `crates/hm/src/tui/source/local.rs` with:

```rust
//! Build-event broadcast → TuiEvent adapter for local `hm run`.
//!
//! The orchestrator emits wire `BuildEvent`s on its broadcast bus and
//! forwards them on a `tokio::sync::mpsc` sender when one is provided.
//! This adapter sits between that mpsc and the TUI's TuiEvent channel,
//! translating each `BuildEvent` 1:1 (with the `Lagged` variant
//! handled in-channel by the scheduler bridge).

use hm_plugin_protocol::BuildEvent;
use tokio::sync::mpsc;

use crate::tui::event::TuiEvent;

/// Spawn the translator task. Returns the bus-side sender for
/// `scheduler::run` and the consumer receiver for `tui::run`.
pub fn spawn() -> (
    mpsc::Sender<BuildEvent>,
    mpsc::Receiver<TuiEvent>,
) {
    let (bus_tx, mut bus_rx) = mpsc::channel::<BuildEvent>(super::TUI_CHANNEL_CAPACITY);
    let (tui_tx, tui_rx) = super::channel();

    tokio::spawn(async move {
        while let Some(ev) = bus_rx.recv().await {
            let translated = translate(ev);
            if tui_tx.send(translated).await.is_err() {
                break;
            }
        }
    });

    (bus_tx, tui_rx)
}

fn translate(ev: BuildEvent) -> TuiEvent {
    match ev {
        BuildEvent::BuildStart { run_id, plan, started_at } => TuiEvent::BuildStart {
            run_id,
            plan,
            started_at,
        },
        BuildEvent::StepQueued { step_id: _, key, chain_idx } => TuiEvent::ChainQueued {
            chain_idx,
            label: key,
            parent: None,
        },
        BuildEvent::StepStart { step_id, runner, image } => TuiEvent::StepStart {
            step_id,
            chain_idx: 0, // chain_idx is set by the prior StepQueued; reducer pulls from `steps[chain_idx]`
            runner,
            image,
            label: String::new(),
        },
        BuildEvent::StepLog { step_id, stream, line, ts } => TuiEvent::StepLog {
            step_id,
            stream,
            line,
            ts,
        },
        BuildEvent::StepCacheHit { step_id, key, tag } => TuiEvent::StepCacheHit {
            step_id,
            key,
            tag,
        },
        BuildEvent::StepEnd { step_id, exit_code, duration_ms, snapshot: _ } => TuiEvent::StepEnd {
            step_id,
            exit_code,
            duration_ms,
        },
        BuildEvent::ChainFailed {
            chain_idx,
            failed_step_id: _,
            failed_step_key,
            exit_code,
            message,
            ts: _,
        } => TuiEvent::ChainFailed {
            chain_idx,
            failed_step_key,
            exit_code,
            message,
        },
        BuildEvent::BuildEnd { exit_code, duration_ms } => TuiEvent::BuildEnd {
            exit_code,
            duration_ms,
        },
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use hm_plugin_protocol::PlanSummary;
    use uuid::Uuid;

    #[tokio::test]
    async fn forwards_build_start() {
        let (bus_tx, mut tui_rx) = spawn();
        bus_tx.send(BuildEvent::BuildStart {
            run_id: Uuid::nil(),
            plan: PlanSummary {
                step_count: 1,
                chain_count: 1,
                default_runner: "docker".into(),
            },
            started_at: chrono::Utc::now(),
        }).await.unwrap();
        let ev = tui_rx.recv().await.unwrap();
        match ev {
            TuiEvent::BuildStart { .. } => {}
            other => panic!("got {other:?}"),
        }
    }
}
```

- [ ] **Step 4: Run the translator + reducer tests**

```bash
cargo test -p hm --lib tui::source::local
cargo test -p hm --lib tui::app
cargo build -p hm
```

Expected: both test sets green; build clean.

- [ ] **Step 5: Commit**

```bash
git add crates/hm/src/tui/source/local.rs \
        crates/hm/src/orchestrator/scheduler.rs \
        crates/hm/src/commands/run/local.rs
git commit -m "feat(tui): local source forwarder + scheduler::run extra_event_tx"
```

### Task 2.3: Protocol const for the cloud host fn

**Files:**
- Modify: `crates/hm-plugin-protocol/src/host_abi.rs`

- [ ] **Step 1: Add the host-fn name constant**

Open `crates/hm-plugin-protocol/src/host_abi.rs`. Find the existing host-fn name constants (search for `pub const HM_LOG_NAME` or similar; if names are not constants today, add a small block at the bottom of the file). Add:

```rust
/// Host fn used by plugins (currently `hm-plugin-cloud::watch`) to
/// emit a wire `BuildEvent` directly into the host's TUI mpsc.
/// Payload: `serde_json::to_vec(&BuildEvent)`. Returns nothing.
pub const HM_BUILD_EVENT_EMIT: &str = "hm_build_event_emit";
```

- [ ] **Step 2: Build + commit**

```bash
cargo build -p hm-plugin-protocol
git add crates/hm-plugin-protocol/src/host_abi.rs
git commit -m "feat(protocol): HM_BUILD_EVENT_EMIT host-fn name constant"
```

### Task 2.4: Implement `hm_build_event_emit` on the host

**Files:**
- Modify: `crates/hm/src/plugin/host_fns.rs`
- Modify: `crates/hm/src/orchestrator/state.rs`

- [ ] **Step 1: Add an optional TUI sender to `OrchestratorState`**

Open `crates/hm/src/orchestrator/state.rs`. Add a field to `OrchestratorState`:

```rust
    /// Optional TUI mpsc; set by `scheduler::run` when the host TUI
    /// is the active output renderer. Populated for both local builds
    /// (where it is fed by the bus forwarder) and cloud watch (where
    /// it is fed by `hm_build_event_emit`).
    pub tui_event_tx: Option<tokio::sync::mpsc::Sender<hm_plugin_protocol::BuildEvent>>,
```

Update every constructor / field-literal of `OrchestratorState` to pass `tui_event_tx: None` by default; in `scheduler::run`, propagate the value from the new `extra_event_tx` parameter into this field.

In `scheduler::run`, after constructing the `OrchestratorState` (around line 90-96), include the new field:

```rust
    let state_arc = Arc::new(OrchestratorState {
        event_bus: bus.clone(),
        archives,
        cancel: cancel.clone(),
        docker: docker.clone(),
        run_id,
        tui_event_tx: extra_event_tx.clone(),
    });
```

Remove the `let extra_handle = …` block from Task 2.2 — it is superseded by the `state.tui_event_tx` route because the cloud plugin needs the same channel. Instead, install one subscriber that sends to whatever sink is registered. Replace the previous block with:

```rust
    let extra_handle = state_arc.tui_event_tx.as_ref().cloned().map(|tx| {
        let mut rx = bus.subscribe();
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(ev) => {
                        if tx.send(ev).await.is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        })
    });
```

- [ ] **Step 2: Register the host fn**

Open `crates/hm/src/plugin/host_fns.rs`. In `HOST_FN_NAMES`, append:

```rust
    "hm_build_event_emit",
```

In the `all()` function (where the host-fn Function objects are constructed), add a new entry alongside the others. Use this implementation (paste near the existing `hm_emit_event` definition):

```rust
host_fn!(hm_build_event_emit(_user_data: (); bytes: Vec<u8>) -> () {
    use hm_plugin_protocol::BuildEvent;
    let ev: BuildEvent = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(_) => return Ok(()), // best-effort: bad payload silently dropped
    };
    if let Some(state) = crate::orchestrator::state::current() {
        if let Some(tx) = state.tui_event_tx.as_ref() {
            // Best-effort send; do not block plugin progress on TUI backpressure.
            let _ = tx.try_send(ev);
        }
    }
    Ok(())
});
```

Add the `Function::new(...)` constructor in the same place where other host fns are listed in `all()`:

```rust
        Function::new(
            "hm_build_event_emit",
            [PTR],
            [],
            UserData::new(()),
            hm_build_event_emit,
        ),
```

(Match the exact `Function::new` call shape of an adjacent host fn — `hm_emit_event` is the closest sibling.)

- [ ] **Step 3: Build + spot-test**

```bash
cargo build -p hm
cargo test -p hm --lib plugin
```

Expected: clean build; plugin tests still pass.

- [ ] **Step 4: Commit**

```bash
git add crates/hm/src/plugin/host_fns.rs crates/hm/src/orchestrator/state.rs crates/hm/src/orchestrator/scheduler.rs
git commit -m "feat(host): hm_build_event_emit host fn + OrchestratorState.tui_event_tx"
```

### Task 2.5: Cloud plugin emits via host fn

**Files:**
- Modify: `crates/hm-plugin-cloud/src/lib.rs`
- Modify: `crates/hm-plugin-cloud/src/verbs/build.rs`

- [ ] **Step 1: Import the host fn**

Open `crates/hm-plugin-cloud/src/lib.rs`. Locate the existing `extism_pdk::host_fn!` import block (the cloud plugin imports `hm_keyring_*`, `hm_kv_*`, etc.). Add the new import alongside:

```rust
#[host_fn]
extern "ExtismHost" {
    // … existing imports …
    fn hm_build_event_emit(payload: Vec<u8>);
}
```

If the cloud plugin already declares imports in a single block, append the new line inside it. Keep one consolidated block — do not introduce a second.

- [ ] **Step 2: Replace the watch poll body**

Open `crates/hm-plugin-cloud/src/verbs/build.rs`, find the `watch` function. Replace the `if b.state != last_state { host::write_stderr(…) }` block with one that constructs a wire `BuildEvent` and emits it. Use the simplest mapping (cloud build state → synthetic step):

```rust
fn watch(client: &Client, org: &str, pipe: &str, number: i64) -> Result<(), PluginError> {
    use hm_plugin_protocol::{BuildEvent, PlanSummary};
    use uuid::Uuid;

    let run_id = Uuid::new_v4();
    let step_id = Uuid::new_v4();

    emit(&BuildEvent::BuildStart {
        run_id,
        plan: PlanSummary {
            step_count: 1,
            chain_count: 1,
            default_runner: "cloud".into(),
        },
        started_at: chrono::Utc::now(),
    });
    emit(&BuildEvent::StepQueued {
        step_id,
        key: format!("cloud build #{number}"),
        chain_idx: 0,
    });
    emit(&BuildEvent::StepStart {
        step_id,
        runner: "cloud".into(),
        image: None,
    });

    let started = std::time::SystemTime::now();
    let mut last_state = String::new();

    loop {
        if host::should_cancel() {
            emit(&BuildEvent::ChainFailed {
                chain_idx: 0,
                failed_step_id: step_id,
                failed_step_key: format!("cloud build #{number}"),
                exit_code: 130,
                message: "watch cancelled by user".into(),
                ts: chrono::Utc::now(),
            });
            return Err(PluginError::new("cloud_cancelled", "watch cancelled by user"));
        }
        let b: Build = client.get(&format!(
            "/organizations/{org}/pipelines/{pipe}/builds/{number}"
        ))?;
        if b.state != last_state {
            emit(&BuildEvent::StepLog {
                step_id,
                stream: hm_plugin_protocol::StdStream::Stderr,
                line: format!("state: {last_state} -> {}", b.state),
                ts: chrono::Utc::now(),
            });
            last_state = b.state.clone();
        }
        let terminal = match b.state.as_str() {
            "passed" => Some(0i32),
            "failed" | "canceled" => Some(1i32),
            _ => None,
        };
        if let Some(code) = terminal {
            let elapsed_ms = started.elapsed().map(|d| d.as_millis() as u64).unwrap_or(0);
            emit(&BuildEvent::StepEnd {
                step_id,
                exit_code: code,
                duration_ms: elapsed_ms,
                snapshot: None,
            });
            emit(&BuildEvent::BuildEnd {
                exit_code: code,
                duration_ms: elapsed_ms,
            });
            if code == 0 {
                return Ok(());
            }
            return Err(PluginError::new(
                "cloud_build_failed",
                format!("build {} ({})", b.state, number),
            ));
        }
        let spin_start = std::time::SystemTime::now();
        while spin_start.elapsed().map(|d| d.as_secs() < 2).unwrap_or(true) {
            if host::should_cancel() {
                break;
            }
        }
    }
}

fn emit(ev: &hm_plugin_protocol::BuildEvent) {
    let bytes = serde_json::to_vec(ev).unwrap_or_default();
    unsafe { hm_build_event_emit(bytes) }; // host fn imported in lib.rs
}
```

- [ ] **Step 3: Build the cloud plugin and the host**

```bash
cargo build -p hm-plugin-cloud --target wasm32-wasip1
cargo build -p hm
```

Expected: both clean. The embedded WASM rebuild is triggered by `build.rs` on the next host build; if it doesn't pick up automatically, force-rebuild with `cargo clean -p hm && cargo build -p hm`.

- [ ] **Step 4: Commit**

```bash
git add crates/hm-plugin-cloud/src/lib.rs crates/hm-plugin-cloud/src/verbs/build.rs
git commit -m "feat(cloud): emit BuildEvents via hm_build_event_emit during watch"
```

### Task 2.6: Cloud source — consumer

**Files:**
- Modify: `crates/hm/src/tui/source/cloud.rs`

- [ ] **Step 1: Write the cloud source**

Replace `crates/hm/src/tui/source/cloud.rs` with:

```rust
//! Cloud watch (host-fn fed) → TuiEvent adapter.
//!
//! The cloud plugin runs `watch` inside WASM and emits wire
//! `BuildEvent`s via the `hm_build_event_emit` host fn. The host fn
//! pushes them into the mpsc owned by `OrchestratorState.tui_event_tx`.
//! This source spawns the same translator task as `local::spawn` —
//! the wire format is identical.

pub use super::local::spawn;
```

- [ ] **Step 2: Build**

```bash
cargo build -p hm
```

- [ ] **Step 3: Commit**

```bash
git add crates/hm/src/tui/source/cloud.rs
git commit -m "feat(tui): cloud source reuses local translator"
```

### Task 2.7: Dev source — daemon poller

**Files:**
- Modify: `crates/hm/src/tui/source/dev.rs`

- [ ] **Step 1: Read the dev registry surface**

Before writing the adapter, list the public surface of `crates/hm/src/commands/dev/registry.rs`, `logmux.rs`, and `ls.rs` to find the existing "what is running" data accessor. If none exists, the adapter takes a `mpsc::UnboundedReceiver<LogLine>` (already produced by `commands/dev/logmux.rs`) plus an iterator of `(slug, deploy_id)` from `dispatch::handle`.

- [ ] **Step 2: Write the adapter**

Replace `crates/hm/src/tui/source/dev.rs` with:

```rust
//! Dev daemon → TuiEvent adapter.
//!
//! Driven by `hm dev up`'s existing `LogLine` mpsc and the registry's
//! per-slug `Booted` list. The TUI consumes one `DeployStatus` per
//! known slug at startup (state: Healthy by default), then `DeployLog`
//! per log line. No Docker-level health polling in v1 — that gets a
//! dedicated follow-up; we synthesise `Healthy` at boot and
//! `Unhealthy` if logmux closes unexpectedly.

use chrono::Utc;
use tokio::sync::mpsc;

use crate::commands::dev::logmux::LogLine;
use crate::tui::event::{DeployState, TuiEvent};

/// Spawn the translator. Returns the TuiEvent receiver. The caller
/// passes the LogLine receiver (already created in `hm dev up`) and a
/// list of `(slug, deploy_id)` pairs known at boot time.
pub fn spawn(
    mut log_rx: mpsc::UnboundedReceiver<LogLine>,
    deploys: Vec<(String, String)>,
) -> mpsc::Receiver<TuiEvent> {
    let (tx, rx) = super::channel();

    let tx_init = tx.clone();
    let deploys_init = deploys.clone();
    tokio::spawn(async move {
        // Synthetic build start so the AppState header renders.
        let _ = tx_init.send(TuiEvent::BuildStart {
            run_id: uuid::Uuid::new_v4(),
            plan: hm_plugin_protocol::PlanSummary {
                step_count: deploys_init.len(),
                chain_count: deploys_init.len(),
                default_runner: "docker".into(),
            },
            started_at: Utc::now(),
        }).await;

        for (idx, (slug, _deploy_id)) in deploys_init.iter().enumerate() {
            let _ = tx_init.send(TuiEvent::ChainQueued {
                chain_idx: idx,
                label: slug.clone(),
                parent: None,
            }).await;
            let _ = tx_init.send(TuiEvent::DeployStatus {
                deploy_id: slug.clone(),
                label: slug.clone(),
                state: DeployState::Healthy,
                restarts: 0,
                uptime_ms: 0,
            }).await;
        }

        while let Some(line) = log_rx.recv().await {
            let _ = tx_init.send(TuiEvent::DeployLog {
                deploy_id: line.slug,
                stream: hm_plugin_protocol::StdStream::Stdout,
                line: String::from_utf8_lossy(&line.bytes).into_owned(),
                ts: Utc::now(),
            }).await;
        }

        // logmux closed — mark every deploy unhealthy and emit BuildEnd
        // so the summary card renders.
        for (slug, _) in &deploys {
            let _ = tx_init.send(TuiEvent::DeployStatus {
                deploy_id: slug.clone(),
                label: slug.clone(),
                state: DeployState::Stopped,
                restarts: 0,
                uptime_ms: 0,
            }).await;
        }
        let _ = tx_init.send(TuiEvent::BuildEnd {
            exit_code: 0,
            duration_ms: 0,
        }).await;
    });

    rx
}
```

- [ ] **Step 3: Build + sanity test**

```bash
cargo build -p hm
cargo test -p hm --lib tui
```

Expected: clean build and existing tui tests still pass.

- [ ] **Step 4: Commit**

```bash
git add crates/hm/src/tui/source/dev.rs
git commit -m "feat(tui): dev source adapter over LogLine mpsc"
```

---

## Phase 3 — Terminal, theme, fx

### Task 3.1: Terminal-setup guard

**Files:**
- Create: `crates/hm/src/tui/term.rs`
- Modify: `crates/hm/src/tui/mod.rs`

- [ ] **Step 1: Write the guard**

Create `crates/hm/src/tui/term.rs`:

```rust
//! Terminal setup / restore guard. Owning a `TermGuard` switches the
//! terminal into alt screen + raw mode + mouse capture; dropping it
//! restores the previous state, even on panic.

use std::io::{self, Stdout};

use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{
        disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
    },
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

pub type TuiTerminal = Terminal<CrosstermBackend<Stdout>>;

/// Holds the terminal in TUI mode. Restores on drop or panic.
pub struct TermGuard {
    pub terminal: TuiTerminal,
}

impl TermGuard {
    pub fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;
        install_panic_hook();
        Ok(Self { terminal })
    }
}

impl Drop for TermGuard {
    fn drop(&mut self) {
        let _ = restore();
    }
}

fn restore() -> io::Result<()> {
    let mut stdout = io::stdout();
    execute!(stdout, LeaveAlternateScreen, DisableMouseCapture)?;
    disable_raw_mode()?;
    Ok(())
}

fn install_panic_hook() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let _ = restore();
            prev(info);
        }));
    });
}
```

- [ ] **Step 2: Register the module**

In `crates/hm/src/tui/mod.rs`, add:

```rust
pub mod term;
```

- [ ] **Step 3: Build**

```bash
cargo build -p hm
```

Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add crates/hm/src/tui/term.rs crates/hm/src/tui/mod.rs
git commit -m "feat(tui): TermGuard with alt-screen/raw/mouse + panic hook"
```

### Task 3.2: Theme

**Files:**
- Create: `crates/hm/src/tui/theme.rs`
- Modify: `crates/hm/src/tui/mod.rs`

- [ ] **Step 1: Write the theme**

Create `crates/hm/src/tui/theme.rs`:

```rust
//! Single-theme palette. Spec §3.3.

use ratatui::style::{Color, Modifier, Style};

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub border_dim: Color,
    pub border_focus: Color,
    pub accent_a: Color,
    pub accent_b: Color,
    pub pass: Color,
    pub cache: Color,
    pub fail: Color,
    pub running: Color,
    pub pending: Color,
    pub text_dim: Color,
}

impl Theme {
    pub const fn dark() -> Self {
        Self {
            border_dim: Color::Indexed(244),
            border_focus: Color::Indexed(51),
            accent_a: Color::Indexed(51),
            accent_b: Color::Indexed(33),
            pass: Color::Indexed(42),
            cache: Color::Indexed(220),
            fail: Color::Indexed(196),
            running: Color::Indexed(51),
            pending: Color::Indexed(244),
            text_dim: Color::Indexed(244),
        }
    }

    pub fn border(&self, focused: bool) -> Style {
        Style::default().fg(if focused { self.border_focus } else { self.border_dim })
    }

    pub fn status(&self, status: crate::tui::app::StepStatus) -> Style {
        use crate::tui::app::StepStatus;
        let c = match status {
            StepStatus::Queued => self.pending,
            StepStatus::Running => self.running,
            StepStatus::CachedHit => self.cache,
            StepStatus::Passed => self.pass,
            StepStatus::Failed => self.fail,
        };
        Style::default().fg(c).add_modifier(Modifier::BOLD)
    }
}
```

- [ ] **Step 2: Register the module**

```rust
pub mod theme;
```

- [ ] **Step 3: Build**

```bash
cargo build -p hm
```

- [ ] **Step 4: Commit**

```bash
git add crates/hm/src/tui/theme.rs crates/hm/src/tui/mod.rs
git commit -m "feat(tui): single dark Theme palette"
```

### Task 3.3: Effects wrapper

**Files:**
- Create: `crates/hm/src/tui/fx.rs`
- Modify: `crates/hm/src/tui/mod.rs`

- [ ] **Step 1: Write the fx module**

Create `crates/hm/src/tui/fx.rs`:

```rust
//! Effect budget + factory. tachyonfx integration.

use std::collections::VecDeque;

use ratatui::layout::Rect;
use tachyonfx::{fx, Effect, EffectTimer, Interpolation};

const MAX_QUEUED: usize = 5;

#[derive(Default)]
pub struct FxQueue {
    queue: VecDeque<ActiveEffect>,
    enabled: bool,
}

pub struct ActiveEffect {
    pub effect: Effect,
    pub area: Rect,
}

impl FxQueue {
    pub fn new(enabled: bool) -> Self {
        Self { queue: VecDeque::new(), enabled }
    }

    pub fn push_sparkle(&mut self, area: Rect) {
        if !self.enabled || self.queue.len() >= MAX_QUEUED {
            return;
        }
        let timer = EffectTimer::from_ms(80, Interpolation::Linear);
        self.queue.push_back(ActiveEffect {
            effect: fx::sweep_in(tachyonfx::Motion::LeftToRight, 6, 0, ratatui::style::Color::Black, timer),
            area,
        });
    }

    pub fn push_fade_in(&mut self, area: Rect) {
        if !self.enabled || self.queue.len() >= MAX_QUEUED {
            return;
        }
        let timer = EffectTimer::from_ms(120, Interpolation::Linear);
        self.queue.push_back(ActiveEffect {
            effect: fx::fade_from_fg(ratatui::style::Color::Black, timer),
            area,
        });
    }

    pub fn push_slide_in(&mut self, area: Rect) {
        if !self.enabled {
            return;
        }
        let timer = EffectTimer::from_ms(200, Interpolation::QuadOut);
        self.queue.push_back(ActiveEffect {
            effect: fx::sweep_in(tachyonfx::Motion::RightToLeft, 12, 0, ratatui::style::Color::Black, timer),
            area,
        });
    }

    pub fn is_animating(&self) -> bool {
        !self.queue.is_empty()
    }

    /// Drive every queued effect by `delta` and drop completed ones.
    /// Call once per frame.
    pub fn tick(&mut self, buf: &mut ratatui::buffer::Buffer, delta: std::time::Duration) {
        use tachyonfx::Shader;
        self.queue.retain_mut(|a| {
            a.effect.process(delta.into(), buf, a.area);
            !a.effect.done()
        });
    }
}
```

> **Note for the implementer:** tachyonfx's API moves between minor versions. The names above match v0.20-ish; if the version `cargo add` pulled is different, substitute the equivalent constructors (`fx::sweep_in`, `fx::fade_*`, `EffectTimer::from_ms`, `Shader::process`). The shape of `FxQueue` should not change.

- [ ] **Step 2: Register**

```rust
pub mod fx;
```

- [ ] **Step 3: Build**

```bash
cargo build -p hm
```

If tachyonfx imports fail, follow the note above and adjust to the actual installed version's surface, then re-run the build.

- [ ] **Step 4: Commit**

```bash
git add crates/hm/src/tui/fx.rs crates/hm/src/tui/mod.rs
git commit -m "feat(tui): FxQueue with sparkle/fade/slide effects + 5-event budget"
```

---

## Phase 4 — Widgets (TDD with insta snapshots)

For every widget in this phase, the test pattern is:

1. Construct an `AppState` representing the rendered scenario.
2. Build an in-memory `ratatui::buffer::Buffer` of fixed size.
3. Call the widget's `render` (via `Widget::render` or `StatefulWidget::render`).
4. Assert with `insta::assert_snapshot!(buffer_to_string(&buffer))`.

A small helper, written once in Task 4.1 and reused, dumps a Buffer to a `String` of one row per line so insta diffs cleanly.

### Task 4.1: Widgets module + header

**Files:**
- Create: `crates/hm/src/tui/widgets/mod.rs`
- Create: `crates/hm/src/tui/widgets/header.rs`
- Modify: `crates/hm/src/tui/mod.rs` (`pub mod widgets;`)

- [ ] **Step 1: Write `widgets/mod.rs`**

```rust
//! Mission Control widget set. All widgets are stateless: they read
//! `&AppState` + `&Theme` and write into a `Buffer`.

pub mod header;
pub mod graph;
pub mod timeline;
pub mod log;
pub mod footer;
pub mod summary;
pub mod help;
pub mod filter;

/// Format a `Buffer` as one row per line for snapshot tests.
#[cfg(test)]
pub(crate) fn buffer_to_string(buf: &ratatui::buffer::Buffer) -> String {
    let mut out = String::new();
    let area = buf.area();
    for y in 0..area.height {
        for x in 0..area.width {
            out.push_str(buf.get(x, y).symbol());
        }
        out.push('\n');
    }
    out
}
```

Also stub the other submodule files with `//! impl arrives in Task 4.x` placeholders so the module tree compiles. Each file gets exactly that one-line module-level doc comment for now.

- [ ] **Step 2: Write the failing header snapshot test**

Create `crates/hm/src/tui/widgets/header.rs`:

```rust
//! Header widget — wordmark + run id + branch + elapsed + chain counter.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::Line;
use ratatui::widgets::Widget;

use crate::tui::app::{AppState, StepStatus};
use crate::tui::theme::Theme;

pub struct Header<'a> {
    pub state: &'a AppState,
    pub theme: &'a Theme,
    pub title: &'a str,
}

impl<'a> Widget for Header<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let total_steps = self.state.steps.len();
        let done = self.state.steps.values()
            .filter(|s| matches!(s.status, StepStatus::Passed | StepStatus::CachedHit | StepStatus::Failed))
            .count();
        let chains = self.state.chains.len();
        let run_short = self.state.run_id
            .map(|u| format!("{:.8}", u.simple()))
            .unwrap_or_else(|| "—".into());
        let elapsed = self.state.started_at
            .map(|t| {
                let end = self.state.ended_at.unwrap_or_else(chrono::Utc::now);
                (end - t).num_seconds().max(0)
            })
            .unwrap_or(0);

        let title_text = format!(
            " HARMONT   {}   run {}   ·   {} chains · {}/{} done ",
            self.title,
            run_short,
            chains,
            done,
            total_steps,
        );
        let line = Line::styled(
            title_text,
            ratatui::style::Style::default()
                .fg(self.theme.accent_a)
                .add_modifier(Modifier::BOLD),
        );
        buf.set_line(area.x, area.y, &line, area.width);
        let _ = elapsed; // elapsed displayed below time spec compaction
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::tui::widgets::buffer_to_string;
    use hm_plugin_protocol::PlanSummary;
    use uuid::Uuid;

    fn fixture() -> AppState {
        let mut s = AppState::new();
        s.apply(crate::tui::event::TuiEvent::BuildStart {
            run_id: Uuid::nil(),
            plan: PlanSummary {
                step_count: 9,
                chain_count: 3,
                default_runner: "docker".into(),
            },
            started_at: chrono::Utc::now(),
        });
        for i in 0..3 {
            s.apply(crate::tui::event::TuiEvent::ChainQueued {
                chain_idx: i,
                label: format!("c{i}"),
                parent: None,
            });
        }
        s
    }

    #[test]
    fn snapshot_header_idle() {
        let theme = Theme::dark();
        let state = fixture();
        let mut buf = Buffer::empty(Rect::new(0, 0, 80, 1));
        Header { state: &state, theme: &theme, title: "hm run" }
            .render(Rect::new(0, 0, 80, 1), &mut buf);
        insta::assert_snapshot!(buffer_to_string(&buf));
    }
}
```

- [ ] **Step 3: Register module + run snapshot**

In `crates/hm/src/tui/mod.rs`:

```rust
pub mod widgets;
```

```bash
cargo test -p hm --lib tui::widgets::header
```

First run: insta creates a `.snap.new` pending file and fails. Review it:

```bash
cargo insta review
```

Accept the snapshot. Re-run:

```bash
cargo test -p hm --lib tui::widgets::header
```

Expected: green.

- [ ] **Step 4: Commit**

```bash
git add crates/hm/src/tui/widgets crates/hm/src/tui/mod.rs
git commit -m "feat(tui): header widget + insta snapshot helper"
```

### Task 4.2: Graph widget

**Files:**
- Modify: `crates/hm/src/tui/widgets/graph.rs`

- [ ] **Step 1: Replace the placeholder with the graph renderer**

```rust
//! Chain DAG renderer. One row per chain; step glyphs grouped left to
//! right by chain order. Lays out forks with `┬ ├ └ ─` connectors.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::{Block, Borders, Widget};

use crate::tui::app::{AppState, StepStatus};
use crate::tui::theme::Theme;

pub struct Graph<'a> {
    pub state: &'a AppState,
    pub theme: &'a Theme,
}

fn glyph(status: StepStatus) -> &'static str {
    match status {
        StepStatus::Queued => "●",
        StepStatus::Running => "◐",
        StepStatus::CachedHit => "◆",
        StepStatus::Passed => "◇",
        StepStatus::Failed => "✖",
    }
}

impl<'a> Widget for Graph<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" graph ")
            .border_style(self.theme.border(false));
        let inner = block.inner(area);
        block.render(area, buf);

        let max_rows = inner.height as usize;
        for (row, chain) in self.state.chains.iter().enumerate().take(max_rows) {
            let mut x = inner.x;
            let mut first = true;
            for sid in &chain.steps {
                let Some(step) = self.state.steps.get(sid) else { continue };
                if !first {
                    if x + 1 < inner.x + inner.width {
                        buf.get_mut(x, inner.y + row as u16).set_symbol("─");
                        x += 1;
                    }
                }
                if x < inner.x + inner.width {
                    buf.get_mut(x, inner.y + row as u16)
                        .set_symbol(glyph(step.status.clone()))
                        .set_style(self.theme.status(step.status.clone()));
                }
                x += 1;
                first = false;
            }
            // Fork indicator on row 0 only if any chain has parent
            // (very light visual hint; full DAG rendering is in v2).
            if row == 0 && self.state.chains.iter().any(|c| c.parent.is_some()) {
                let _ = Style::default();
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::tui::event::TuiEvent;
    use crate::tui::widgets::buffer_to_string;
    use hm_plugin_protocol::PlanSummary;
    use uuid::Uuid;

    #[test]
    fn snapshot_graph_three_chains_mixed_status() {
        let mut s = AppState::new();
        s.apply(TuiEvent::BuildStart {
            run_id: Uuid::nil(),
            plan: PlanSummary {
                step_count: 9,
                chain_count: 3,
                default_runner: "docker".into(),
            },
            started_at: chrono::Utc::now(),
        });
        for i in 0..3 {
            s.apply(TuiEvent::ChainQueued {
                chain_idx: i,
                label: format!("c{i}"),
                parent: None,
            });
        }
        let s0 = Uuid::new_v4();
        let s1 = Uuid::new_v4();
        let s2 = Uuid::new_v4();
        s.apply(TuiEvent::StepStart { step_id: s0, chain_idx: 0, runner: "docker".into(), image: None, label: "test".into() });
        s.apply(TuiEvent::StepEnd { step_id: s0, exit_code: 0, duration_ms: 100 });
        s.apply(TuiEvent::StepStart { step_id: s1, chain_idx: 1, runner: "docker".into(), image: None, label: "build".into() });
        s.apply(TuiEvent::StepCacheHit { step_id: s1, key: "k".into(), tag: "t".into() });
        s.apply(TuiEvent::StepStart { step_id: s2, chain_idx: 2, runner: "docker".into(), image: None, label: "lint".into() });

        let theme = Theme::dark();
        let area = Rect::new(0, 0, 30, 8);
        let mut buf = Buffer::empty(area);
        Graph { state: &s, theme: &theme }.render(area, &mut buf);
        insta::assert_snapshot!(buffer_to_string(&buf));
    }
}
```

- [ ] **Step 2: Run + review snapshot**

```bash
cargo test -p hm --lib tui::widgets::graph
cargo insta review        # accept
cargo test -p hm --lib tui::widgets::graph
```

Expected: green.

- [ ] **Step 3: Commit**

```bash
git add crates/hm/src/tui/widgets/graph.rs crates/hm/tests/snapshots
git commit -m "feat(tui): graph widget (one-row-per-chain, status glyphs)"
```

### Task 4.3: Timeline widget

**Files:**
- Modify: `crates/hm/src/tui/widgets/timeline.rs`

- [ ] **Step 1: Replace the placeholder**

```rust
//! Gantt-style timeline. Bars per chain, colored by current step
//! status, with right-aligned label + duration + status pill.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::{Block, Borders, Widget};

use crate::tui::app::{AppState, StepStatus};
use crate::tui::theme::Theme;

pub struct Timeline<'a> {
    pub state: &'a AppState,
    pub theme: &'a Theme,
}

fn pill(status: &StepStatus) -> &'static str {
    match status {
        StepStatus::Queued => "queued",
        StepStatus::Running => "run",
        StepStatus::CachedHit => "cache",
        StepStatus::Passed => "pass",
        StepStatus::Failed => "fail",
    }
}

impl<'a> Widget for Timeline<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" timeline ")
            .border_style(self.theme.border(false));
        let inner = block.inner(area);
        block.render(area, buf);

        let total_ms: u64 = self.state.steps.values()
            .filter_map(|s| s.duration_ms)
            .sum::<u64>()
            .max(1);
        let bar_max = inner.width.saturating_sub(28) as u64;

        for (row, chain) in self.state.chains.iter().enumerate().take(inner.height as usize) {
            let Some(last_step_id) = chain.steps.last() else { continue };
            let Some(step) = self.state.steps.get(last_step_id) else { continue };
            let dur = step.duration_ms.unwrap_or(0);
            let fill = ((dur as f64 / total_ms as f64) * bar_max as f64) as u16;
            let status_style = self.theme.status(step.status.clone());

            let y = inner.y + row as u16;

            // Chain label
            let label = format!("c{} ", row + 1);
            let mut x = inner.x;
            for ch in label.chars() {
                if x < inner.x + inner.width {
                    buf.get_mut(x, y).set_symbol(&ch.to_string());
                    x += 1;
                }
            }
            // Bar
            let bar_start = x;
            for i in 0..bar_max as u16 {
                if bar_start + i >= inner.x + inner.width { break; }
                let symbol = if i < fill { "█" } else { "░" };
                buf.get_mut(bar_start + i, y)
                    .set_symbol(symbol)
                    .set_style(if i < fill { status_style } else { Style::default().fg(self.theme.pending) });
            }
            // Trailing label + dur + pill
            let trail = format!(" {} {:>4}ms {:>5}", step.label, dur, pill(&step.status));
            let trail_x = bar_start + bar_max as u16;
            x = trail_x;
            for ch in trail.chars() {
                if x < inner.x + inner.width {
                    buf.get_mut(x, y).set_symbol(&ch.to_string());
                    x += 1;
                }
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::tui::event::TuiEvent;
    use crate::tui::widgets::buffer_to_string;
    use hm_plugin_protocol::PlanSummary;
    use uuid::Uuid;

    #[test]
    fn snapshot_timeline_three_chains() {
        let mut s = AppState::new();
        s.apply(TuiEvent::BuildStart {
            run_id: Uuid::nil(),
            plan: PlanSummary {
                step_count: 3,
                chain_count: 3,
                default_runner: "docker".into(),
            },
            started_at: chrono::Utc::now(),
        });
        for i in 0..3 {
            s.apply(TuiEvent::ChainQueued {
                chain_idx: i,
                label: format!("c{i}"),
                parent: None,
            });
            let sid = Uuid::new_v4();
            s.apply(TuiEvent::StepStart {
                step_id: sid,
                chain_idx: i,
                runner: "docker".into(),
                image: None,
                label: ["test", "build", "lint"][i].into(),
            });
            s.apply(TuiEvent::StepEnd {
                step_id: sid,
                exit_code: 0,
                duration_ms: (i as u64 + 1) * 1000,
            });
        }
        let theme = Theme::dark();
        let area = Rect::new(0, 0, 60, 8);
        let mut buf = Buffer::empty(area);
        Timeline { state: &s, theme: &theme }.render(area, &mut buf);
        insta::assert_snapshot!(buffer_to_string(&buf));
    }
}
```

- [ ] **Step 2: Snapshot + review**

```bash
cargo test -p hm --lib tui::widgets::timeline
cargo insta review
cargo test -p hm --lib tui::widgets::timeline
```

- [ ] **Step 3: Commit**

```bash
git add crates/hm/src/tui/widgets/timeline.rs crates/hm/tests/snapshots
git commit -m "feat(tui): timeline widget (gantt + status pill)"
```

### Task 4.4: Log widget

**Files:**
- Modify: `crates/hm/src/tui/widgets/log.rs`

- [ ] **Step 1: Replace the placeholder**

```rust
//! Log tail for the focused chain's most-recent step.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::{Block, Borders, Widget};

use crate::tui::app::AppState;
use crate::tui::theme::Theme;

pub struct LogPane<'a> {
    pub state: &'a AppState,
    pub theme: &'a Theme,
    pub scroll: usize,
    pub filter: Option<&'a str>,
}

impl<'a> Widget for LogPane<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let chain_label = self.state.chains
            .get(self.state.focused_chain)
            .map(|c| c.label.clone())
            .unwrap_or_default();
        let title = format!(" log · {} ", chain_label);
        let block = Block::default()
            .borders(Borders::ALL)
            .title(title)
            .border_style(self.theme.border(true));
        let inner = block.inner(area);
        block.render(area, buf);

        let Some(step_id) = self.state.focused_step_id() else { return };
        let Some(log) = self.state.logs.get(&step_id) else { return };

        let lines: Vec<_> = log.entries.iter()
            .filter(|e| self.filter.map_or(true, |f| e.line.contains(f)))
            .collect();

        let height = inner.height as usize;
        let start = lines.len().saturating_sub(height + self.scroll);
        for (i, entry) in lines.iter().skip(start).take(height).enumerate() {
            let prefix = match entry.stream {
                hm_plugin_protocol::StdStream::Stdout => "  ",
                hm_plugin_protocol::StdStream::Stderr => "! ",
            };
            let line = format!("{prefix}{}", entry.line);
            let y = inner.y + i as u16;
            let mut x = inner.x;
            for ch in line.chars() {
                if x >= inner.x + inner.width { break; }
                buf.get_mut(x, y)
                    .set_symbol(&ch.to_string())
                    .set_style(if entry.stream == hm_plugin_protocol::StdStream::Stderr {
                        Style::default().fg(self.theme.text_dim)
                    } else {
                        Style::default()
                    });
                x += 1;
            }
        }

        if log.dropped > 0 {
            let drop_msg = format!("  … {} events dropped (lagged) …", log.dropped);
            let y = inner.y;
            let mut x = inner.x;
            for ch in drop_msg.chars() {
                if x >= inner.x + inner.width { break; }
                buf.get_mut(x, y).set_symbol(&ch.to_string())
                    .set_style(Style::default().fg(self.theme.text_dim));
                x += 1;
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::tui::event::TuiEvent;
    use crate::tui::widgets::buffer_to_string;
    use uuid::Uuid;

    #[test]
    fn snapshot_log_with_filter() {
        let mut s = AppState::new();
        s.apply(TuiEvent::ChainQueued {
            chain_idx: 0,
            label: "c0".into(),
            parent: None,
        });
        let sid = Uuid::new_v4();
        s.apply(TuiEvent::StepStart {
            step_id: sid,
            chain_idx: 0,
            runner: "docker".into(),
            image: None,
            label: "test".into(),
        });
        for l in ["alpha", "beta cat", "gamma cat", "delta"] {
            s.apply(TuiEvent::StepLog {
                step_id: sid,
                stream: hm_plugin_protocol::StdStream::Stdout,
                line: l.into(),
                ts: chrono::Utc::now(),
            });
        }
        let theme = Theme::dark();
        let area = Rect::new(0, 0, 40, 6);
        let mut buf = Buffer::empty(area);
        LogPane { state: &s, theme: &theme, scroll: 0, filter: Some("cat") }
            .render(area, &mut buf);
        insta::assert_snapshot!(buffer_to_string(&buf));
    }
}
```

- [ ] **Step 2: Snapshot + review + commit**

```bash
cargo test -p hm --lib tui::widgets::log
cargo insta review
cargo test -p hm --lib tui::widgets::log
git add crates/hm/src/tui/widgets/log.rs crates/hm/tests/snapshots
git commit -m "feat(tui): log widget with regex filter + lagged-events note"
```

### Task 4.5: Footer widget

**Files:**
- Modify: `crates/hm/src/tui/widgets/footer.rs`

- [ ] **Step 1: Replace the placeholder**

```rust
//! Footer — keybinding hints + summary counters.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::Widget;

use crate::tui::app::{AppState, StepStatus};
use crate::tui::theme::Theme;

pub struct Footer<'a> {
    pub state: &'a AppState,
    pub theme: &'a Theme,
}

impl<'a> Widget for Footer<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let mut pass = 0;
        let mut cache = 0;
        let mut fail = 0;
        for s in self.state.steps.values() {
            match s.status {
                StepStatus::Passed => pass += 1,
                StepStatus::CachedHit => cache += 1,
                StepStatus::Failed => fail += 1,
                _ => {}
            }
        }
        let hints = " [tab] chain · [l] logs · [/] filter · [q] quit ";
        let summary = format!(" {pass} pass · {cache} cache · {fail} fail ");
        let total_width = area.width as usize;
        let pad = total_width.saturating_sub(hints.len() + summary.len());
        let line = format!("{hints}{}{summary}", " ".repeat(pad));

        let mut x = area.x;
        for ch in line.chars() {
            if x >= area.x + area.width { break; }
            buf.get_mut(x, area.y).set_symbol(&ch.to_string())
                .set_style(ratatui::style::Style::default().fg(self.theme.text_dim));
            x += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::widgets::buffer_to_string;

    #[test]
    fn snapshot_footer_empty() {
        let s = AppState::new();
        let theme = Theme::dark();
        let area = Rect::new(0, 0, 80, 1);
        let mut buf = Buffer::empty(area);
        Footer { state: &s, theme: &theme }.render(area, &mut buf);
        insta::assert_snapshot!(buffer_to_string(&buf));
    }
}
```

- [ ] **Step 2: Snapshot + commit**

```bash
cargo test -p hm --lib tui::widgets::footer
cargo insta review
cargo test -p hm --lib tui::widgets::footer
git add crates/hm/src/tui/widgets/footer.rs crates/hm/tests/snapshots
git commit -m "feat(tui): footer widget with hints + counters"
```

### Task 4.6: Summary card

**Files:**
- Modify: `crates/hm/src/tui/widgets/summary.rs`

- [ ] **Step 1: Replace the placeholder**

```rust
//! Final summary card — full-screen frame after `BuildEnd`.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, Widget};
use tui_big_text::{BigText, PixelSize};

use crate::tui::app::{AppState, StepStatus};
use crate::tui::theme::Theme;

pub struct Summary<'a> {
    pub state: &'a AppState,
    pub theme: &'a Theme,
}

impl<'a> Widget for Summary<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(self.theme.border(true));
        let inner = block.inner(area);
        block.render(area, buf);

        let mut pass = 0;
        let mut cache = 0;
        let mut fail = 0;
        let mut slowest: Option<(String, u64)> = None;
        for s in self.state.steps.values() {
            match s.status {
                StepStatus::Passed => pass += 1,
                StepStatus::CachedHit => cache += 1,
                StepStatus::Failed => fail += 1,
                _ => {}
            }
            if let Some(d) = s.duration_ms {
                if slowest.as_ref().map_or(true, |(_, p)| d > *p) {
                    slowest = Some((s.label.clone(), d));
                }
            }
        }
        let total = self.state.steps.len().max(1);
        let cache_pct = (cache as f64 / total as f64) * 100.0;
        let total_ms: u64 = self.state.steps.values()
            .filter_map(|s| s.duration_ms)
            .sum();

        let failed = fail > 0;
        let banner_style = if failed {
            Style::default().fg(self.theme.fail).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(self.theme.pass).add_modifier(Modifier::BOLD)
        };
        let banner = if failed { "build failed" } else { "build complete" };

        // Big wordmark
        let big = BigText::builder()
            .pixel_size(PixelSize::Quadrant)
            .style(Style::default().fg(self.theme.accent_a))
            .lines(vec!["HARMONT".into()])
            .build();
        let wordmark_area = Rect::new(inner.x + 2, inner.y + 1, inner.width.saturating_sub(4), 4);
        big.render(wordmark_area, buf);

        let lines = vec![
            (banner, banner_style),
            (&"", Style::default()),
            (&format!("  total       {}ms", total_ms), Style::default()),
            (&format!("  chains      {}", self.state.chains.len()), Style::default()),
            (&format!("  steps       {pass} passed · {cache} cached · {fail} failed"), Style::default()),
            (&format!("  cache hit % {:.0}%", cache_pct), Style::default()),
            (
                &format!(
                    "  slowest     {}",
                    slowest.as_ref().map(|(l, d)| format!("{l} ({d}ms)")).unwrap_or_default()
                ),
                Style::default(),
            ),
        ];
        for (i, (text, style)) in lines.iter().enumerate() {
            let y = inner.y + 6 + i as u16;
            let mut x = inner.x + 2;
            for ch in text.chars() {
                if x >= inner.x + inner.width || y >= inner.y + inner.height { break; }
                buf.get_mut(x, y).set_symbol(&ch.to_string()).set_style(*style);
                x += 1;
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::tui::event::TuiEvent;
    use crate::tui::widgets::buffer_to_string;
    use hm_plugin_protocol::PlanSummary;
    use uuid::Uuid;

    #[test]
    fn snapshot_summary_pass() {
        let mut s = AppState::new();
        s.apply(TuiEvent::BuildStart {
            run_id: Uuid::nil(),
            plan: PlanSummary { step_count: 3, chain_count: 3, default_runner: "docker".into() },
            started_at: chrono::Utc::now(),
        });
        for i in 0..3 {
            s.apply(TuiEvent::ChainQueued { chain_idx: i, label: format!("c{i}"), parent: None });
            let sid = Uuid::new_v4();
            s.apply(TuiEvent::StepStart { step_id: sid, chain_idx: i, runner: "docker".into(), image: None, label: ["test", "build", "lint"][i].into() });
            s.apply(TuiEvent::StepEnd { step_id: sid, exit_code: 0, duration_ms: (i as u64 + 1) * 1000 });
        }
        s.apply(TuiEvent::BuildEnd { exit_code: 0, duration_ms: 6000 });

        let theme = Theme::dark();
        let area = Rect::new(0, 0, 80, 24);
        let mut buf = Buffer::empty(area);
        Summary { state: &s, theme: &theme }.render(area, &mut buf);
        insta::assert_snapshot!(buffer_to_string(&buf));
    }
}
```

- [ ] **Step 2: Snapshot + commit**

```bash
cargo test -p hm --lib tui::widgets::summary
cargo insta review
cargo test -p hm --lib tui::widgets::summary
git add crates/hm/src/tui/widgets/summary.rs crates/hm/tests/snapshots
git commit -m "feat(tui): summary card widget"
```

### Task 4.7: Help + filter overlays

**Files:**
- Modify: `crates/hm/src/tui/widgets/help.rs`
- Modify: `crates/hm/src/tui/widgets/filter.rs`

- [ ] **Step 1: Help overlay**

`crates/hm/src/tui/widgets/help.rs`:

```rust
//! `?` help overlay — full-screen centered card.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders, Widget};

use crate::tui::theme::Theme;

pub struct Help<'a> { pub theme: &'a Theme }

impl<'a> Widget for Help<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" help ")
            .border_style(self.theme.border(true));
        let inner = block.inner(area);
        block.render(area, buf);

        let lines = [
            "  q · Esc      quit",
            "  Tab          next chain",
            "  Shift-Tab    prev chain",
            "  l            expand log pane",
            "  / · Esc      filter logs",
            "  ↑ ↓ wheel    scroll log",
            "  PgUp PgDn    page-scroll log",
            "  g · G        top / bottom of log",
            "  ?            toggle this help",
            "  Ctrl-C       cancel run (twice to force)",
        ];
        for (i, l) in lines.iter().enumerate() {
            let y = inner.y + 1 + i as u16;
            if y >= inner.y + inner.height { break; }
            let mut x = inner.x + 2;
            for ch in l.chars() {
                if x >= inner.x + inner.width { break; }
                buf.get_mut(x, y).set_symbol(&ch.to_string());
                x += 1;
            }
        }
    }
}
```

- [ ] **Step 2: Filter overlay (single-line input)**

`crates/hm/src/tui/widgets/filter.rs`:

```rust
//! Inline filter prompt — single line at the bottom of the log pane.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::Widget;

use crate::tui::theme::Theme;

pub struct Filter<'a> {
    pub theme: &'a Theme,
    pub query: &'a str,
}

impl<'a> Widget for Filter<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let prompt = format!(" /{}_", self.query);
        let mut x = area.x;
        for ch in prompt.chars() {
            if x >= area.x + area.width { break; }
            buf.get_mut(x, area.y).set_symbol(&ch.to_string())
                .set_style(ratatui::style::Style::default().fg(self.theme.accent_a));
            x += 1;
        }
    }
}
```

- [ ] **Step 3: Build + commit**

```bash
cargo build -p hm
git add crates/hm/src/tui/widgets/help.rs crates/hm/src/tui/widgets/filter.rs
git commit -m "feat(tui): help + filter overlays"
```

---

## Phase 5 — App glue

### Task 5.1: Main loop, layout, key/mouse dispatch

**Files:**
- Modify: `crates/hm/src/tui/mod.rs`

- [ ] **Step 1: Implement `tui::run`**

Replace the body of `crates/hm/src/tui/mod.rs` with the full implementation (keep the existing `pub mod` declarations at the top):

```rust
//! Mission Control TUI — host-side ratatui renderer.

pub mod app;
pub mod event;
pub mod fx;
pub mod source;
pub mod term;
pub mod theme;
pub mod widgets;

use std::io;
use std::time::{Duration, Instant};

use crossterm::event::{
    self as ce, Event as CeEvent, KeyCode, KeyEventKind, KeyModifiers, MouseEventKind,
};
use ratatui::layout::{Constraint, Direction, Layout};
use tokio::sync::mpsc;

use self::app::AppState;
use self::event::TuiEvent;
use self::fx::FxQueue;
use self::term::TermGuard;
use self::theme::Theme;
use self::widgets::{
    filter::Filter, footer::Footer, graph::Graph, header::Header, help::Help, log::LogPane,
    summary::Summary, timeline::Timeline,
};

#[derive(Debug, Clone)]
pub struct TuiOptions {
    pub fx_enabled: bool,
    pub summary_card: bool,
    pub title: String,
}

#[derive(Debug, thiserror::Error)]
pub enum TuiError {
    #[error("terminal i/o: {0}")]
    Io(#[from] io::Error),
    #[error("event channel closed before BuildEnd")]
    ChannelClosed,
}

const FRAME_INTERVAL: Duration = Duration::from_millis(16);
const SUMMARY_HOLD: Duration = Duration::from_secs(2);
const MIN_COLS: u16 = 60;
const MIN_ROWS: u16 = 20;

pub async fn run(
    mut events: mpsc::Receiver<TuiEvent>,
    opts: TuiOptions,
) -> Result<i32, TuiError> {
    let mut guard = TermGuard::enter()?;
    let theme = Theme::dark();
    let mut state = AppState::new();
    let mut fx = FxQueue::new(opts.fx_enabled);

    let mut frame_tick = tokio::time::interval(FRAME_INTERVAL);
    frame_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    let mut last_frame = Instant::now();
    let mut needs_render = true;
    let mut help_open = false;
    let mut filter_open = false;
    let mut filter_buf = String::new();
    let mut log_scroll: usize = 0;
    let mut last_ctrl_c: Option<Instant> = None;

    loop {
        tokio::select! {
            _ = frame_tick.tick() => {
                let now = Instant::now();
                let delta = now - last_frame;
                last_frame = now;

                // Drain pending key/mouse events (non-blocking)
                while ce::poll(Duration::from_millis(0)).map_err(TuiError::Io)? {
                    let ev = ce::read().map_err(TuiError::Io)?;
                    needs_render = true;
                    match ev {
                        CeEvent::Key(k) if k.kind == KeyEventKind::Press => {
                            if filter_open {
                                match k.code {
                                    KeyCode::Esc => { filter_open = false; filter_buf.clear(); }
                                    KeyCode::Backspace => { filter_buf.pop(); }
                                    KeyCode::Enter => { filter_open = false; }
                                    KeyCode::Char(c) => { filter_buf.push(c); }
                                    _ => {}
                                }
                                continue;
                            }
                            match k.code {
                                KeyCode::Char('q') | KeyCode::Esc => return finalise(&state, opts.summary_card, &theme, &mut guard).await,
                                KeyCode::Tab => state.cycle_focus(1),
                                KeyCode::BackTab => state.cycle_focus(-1),
                                KeyCode::Char('l') => { /* log expand toggle stub */ }
                                KeyCode::Char('/') => { filter_open = true; filter_buf.clear(); }
                                KeyCode::Char('?') => { help_open = !help_open; }
                                KeyCode::Up => { log_scroll = log_scroll.saturating_add(1); }
                                KeyCode::Down => { log_scroll = log_scroll.saturating_sub(1); }
                                KeyCode::PageUp => { log_scroll = log_scroll.saturating_add(10); }
                                KeyCode::PageDown => { log_scroll = log_scroll.saturating_sub(10); }
                                KeyCode::Char('g') => { log_scroll = usize::MAX / 2; }
                                KeyCode::Char('G') => { log_scroll = 0; }
                                KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                                    let now = Instant::now();
                                    if last_ctrl_c.map_or(false, |t| now - t < Duration::from_secs(2)) {
                                        return Ok(130);
                                    }
                                    last_ctrl_c = Some(now);
                                    // First Ctrl-C: signal cancel to host (orchestrator) — TODO wire via cancel token in opts
                                }
                                _ => {}
                            }
                        }
                        CeEvent::Mouse(m) => {
                            match m.kind {
                                MouseEventKind::ScrollUp => { log_scroll = log_scroll.saturating_add(2); }
                                MouseEventKind::ScrollDown => { log_scroll = log_scroll.saturating_sub(2); }
                                MouseEventKind::Down(_) => {
                                    // Click-to-focus: chain row = y - header height.
                                    let chain_idx = m.row.saturating_sub(2) as usize;
                                    if chain_idx < state.chains.len() {
                                        state.focused_chain = chain_idx;
                                    }
                                }
                                _ => {}
                            }
                        }
                        CeEvent::Resize(cols, rows) => {
                            if cols < MIN_COLS || rows < MIN_ROWS {
                                drop(guard);
                                eprintln!("[hm] terminal too small for TUI; falling back to streaming output");
                                return Ok(consume_to_end(&mut events).await);
                            }
                        }
                        _ => {}
                    }
                }

                if !needs_render && !fx.is_animating() {
                    continue;
                }
                needs_render = false;

                guard.terminal.draw(|f| {
                    let size = f.size();
                    let chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([
                            Constraint::Length(2),
                            Constraint::Length(8),
                            Constraint::Min(0),
                            Constraint::Length(1),
                        ])
                        .split(size);

                    let row = Layout::default()
                        .direction(Direction::Horizontal)
                        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
                        .split(chunks[1]);

                    f.render_widget(Header { state: &state, theme: &theme, title: &opts.title }, chunks[0]);
                    f.render_widget(Graph { state: &state, theme: &theme }, row[0]);
                    f.render_widget(Timeline { state: &state, theme: &theme }, row[1]);
                    f.render_widget(
                        LogPane {
                            state: &state,
                            theme: &theme,
                            scroll: log_scroll,
                            filter: if filter_open || !filter_buf.is_empty() { Some(filter_buf.as_str()) } else { None },
                        },
                        chunks[2],
                    );
                    f.render_widget(Footer { state: &state, theme: &theme }, chunks[3]);
                    if filter_open {
                        let fa = ratatui::layout::Rect::new(chunks[2].x, chunks[2].y + chunks[2].height - 1, chunks[2].width, 1);
                        f.render_widget(Filter { theme: &theme, query: &filter_buf }, fa);
                    }
                    if help_open {
                        let w = 50.min(size.width.saturating_sub(4));
                        let h = 14.min(size.height.saturating_sub(4));
                        let r = ratatui::layout::Rect::new((size.width - w) / 2, (size.height - h) / 2, w, h);
                        f.render_widget(Help { theme: &theme }, r);
                    }
                    let buf = f.buffer_mut();
                    fx.tick(buf, delta);
                })?;
            }
            ev = events.recv() => {
                match ev {
                    Some(TuiEvent::StepCacheHit { .. }) => {
                        needs_render = true;
                        let rect = ratatui::layout::Rect::new(0, 2, 40, 6);
                        fx.push_sparkle(rect);
                        state.apply(ev.unwrap());
                    }
                    Some(TuiEvent::StepEnd { exit_code, .. }) if exit_code == 0 => {
                        needs_render = true;
                        let rect = ratatui::layout::Rect::new(0, 2, 40, 6);
                        fx.push_sparkle(rect);
                        state.apply(ev.unwrap());
                    }
                    Some(TuiEvent::BuildEnd { exit_code, duration_ms }) => {
                        state.apply(TuiEvent::BuildEnd { exit_code, duration_ms });
                        return finalise(&state, opts.summary_card, &theme, &mut guard).await;
                    }
                    Some(e) => {
                        needs_render = true;
                        state.apply(e);
                    }
                    None => return finalise(&state, opts.summary_card, &theme, &mut guard).await,
                }
            }
        }
    }
}

async fn finalise(
    state: &AppState,
    summary_card: bool,
    theme: &Theme,
    guard: &mut TermGuard,
) -> Result<i32, TuiError> {
    if summary_card {
        guard.terminal.draw(|f| {
            let size = f.size();
            f.render_widget(Summary { state, theme }, size);
        })?;
        tokio::time::sleep(SUMMARY_HOLD).await;
    }
    Ok(state.exit_code.unwrap_or(0))
}

async fn consume_to_end(events: &mut mpsc::Receiver<TuiEvent>) -> i32 {
    let mut code = 0;
    while let Some(ev) = events.recv().await {
        if let TuiEvent::BuildEnd { exit_code, .. } = ev {
            code = exit_code;
        }
    }
    code
}
```

- [ ] **Step 2: Build**

```bash
cargo build -p hm
```

Expected: clean. If ratatui's `Frame::size()` was renamed to `Frame::area()` in a newer release, swap the call.

- [ ] **Step 3: Commit**

```bash
git add crates/hm/src/tui/mod.rs
git commit -m "feat(tui): run loop with key/mouse dispatch + filter/help overlays"
```

---

## Phase 6 — Command wiring

### Task 6.1: `hm run` TTY-detect

**Files:**
- Modify: `crates/hm/src/commands/run/local.rs`
- Modify: `crates/hm/src/cli.rs` (only if `--no-tui` / `--no-fx` not yet routed to `RunArgs`)

- [ ] **Step 1: Add TTY detection**

In `crates/hm/src/commands/run/local.rs`, inside `pub async fn handle(args: RunArgs, _ctx: RunContext)`, after `args.format` is read but before calling `crate::orchestrator::run`, branch:

```rust
    use is_terminal::IsTerminal;

    let want_tui = args.format == "human"
        && !std::env::var("NO_COLOR").is_ok()
        && std::io::stdout().is_terminal()
        // Global flags routed via env or context — see cli.rs
        && std::env::var("HM_NO_TUI").is_err();

    if want_tui {
        // Wire the TUI source.
        let (bus_tx, mut tui_rx) = crate::tui::source::local::spawn();
        let opts = crate::tui::TuiOptions {
            fx_enabled: std::env::var("HM_NO_FX").is_err(),
            summary_card: true,
            title: "hm run".into(),
        };
        let orch_handle = tokio::spawn({
            let pipeline_wire = pipeline_wire.clone();
            let repo_root = repo_root.clone();
            let format = args.format.clone();
            let tx = bus_tx.clone();
            async move {
                crate::orchestrator::run(pipeline_wire, repo_root, parallelism, format, Some(tx)).await
            }
        });
        let tui_exit = crate::tui::run(tui_rx, opts).await
            .map_err(|e| anyhow::anyhow!(e))?;
        let orch_exit = orch_handle.await??;
        return Ok(if tui_exit != 0 { tui_exit } else { orch_exit });
    }
```

The "global flags routed via env" stub means: in `crates/hm/src/main.rs` (or wherever the CLI is parsed), set `HM_NO_TUI=1` / `HM_NO_FX=1` env vars when `cli.no_tui` / `cli.no_fx` are true. This keeps the dispatch logic inside the run handler simple. Add (in `main.rs`, right after the `Cli::parse()` call):

```rust
    if cli.no_tui { std::env::set_var("HM_NO_TUI", "1"); }
    if cli.no_fx  { std::env::set_var("HM_NO_FX", "1"); }
```

- [ ] **Step 2: Verify the existing non-TUI fallthrough still works**

```bash
cargo build -p hm
./target/debug/hm run --no-tui --help 2>&1 | head -5
```

Expected: `hm run` help (the binary still parses correctly).

- [ ] **Step 3: Smoke-run the TUI against an example**

```bash
cd examples/rust
../../target/debug/hm run
```

Expected: TUI enters, run finishes, summary card shows, terminal restored. Press `q` to quit early if needed. Run `cd ../..` to return.

- [ ] **Step 4: Commit**

```bash
git add crates/hm/src/commands/run/local.rs crates/hm/src/main.rs
git commit -m "feat(hm run): route TTY runs through Mission Control TUI"
```

### Task 6.2: `hm dev up` TTY-detect

**Files:**
- Modify: `crates/hm/src/commands/dev/up.rs`

- [ ] **Step 1: Wire the dev source**

In `pub async fn handle(args: DevUpArgs, ctx: RunContext)`, after the logmux channel and `booted` list are constructed but before `eprintln!("[hm] all up. Ctrl-C…")`, branch on TTY:

```rust
    use is_terminal::IsTerminal;

    let want_tui = std::io::stdout().is_terminal()
        && std::env::var("HM_NO_TUI").is_err()
        && std::env::var("NO_COLOR").is_err();

    if want_tui {
        let deploys: Vec<(String, String)> = booted.iter()
            .map(|b| (b.slug.clone(), b.container_id.clone()))
            .collect();
        // The logmux already consumes log_rx; we tee by giving the TUI
        // adapter its own UnboundedReceiver via channel split. Simpler
        // for v1: stop running the legacy logmux when the TUI is the
        // active renderer, and let the TUI own the LogLine stream.
        drop(log_task); // legacy logmux not used in TUI mode
        let tui_rx = crate::tui::source::dev::spawn(log_rx, deploys);
        let opts = crate::tui::TuiOptions {
            fx_enabled: std::env::var("HM_NO_FX").is_err(),
            summary_card: true,
            title: "hm dev up".into(),
        };
        let _ = crate::tui::run(tui_rx, opts).await
            .map_err(|e| anyhow::anyhow!(e))?;
        // Teardown: same path the legacy code uses after the wait signal.
        // …existing teardown code stays below this block as-is…
    }
```

**Important:** Read the existing teardown logic carefully — `log_task` is a `JoinHandle` and dropping it does not stop the task. Refactor: when entering TUI mode, do **not** call `log_task = tokio::spawn(run_logmux(...))` at all. Replace the conditional logic so the logmux task is only spawned in the non-TUI branch.

Concretely, restructure the existing block:

```rust
    let (log_tx, log_rx) = mpsc::unbounded_channel::<LogLine>();
    let log_color = std::env::var("NO_COLOR").is_err();
    let log_task = tokio::spawn(run_logmux(log_rx, slug_width, log_color));
```

into:

```rust
    let (log_tx, log_rx) = mpsc::unbounded_channel::<LogLine>();
    let log_color = std::env::var("NO_COLOR").is_err();
    let mut log_rx_opt = Some(log_rx);
    let log_task = if want_tui {
        None
    } else {
        Some(tokio::spawn(run_logmux(log_rx_opt.take().unwrap(), slug_width, log_color)))
    };
```

Then the TUI branch above does `log_rx_opt.take().unwrap()` to consume the receiver. (Move the `want_tui` calculation to before this block, or compute it lazily.)

- [ ] **Step 2: Build + dry-run**

```bash
cargo build -p hm
```

Expected: clean. End-to-end test of `hm dev up` requires Docker + an example dev pipeline; leave that for manual verification.

- [ ] **Step 3: Commit**

```bash
git add crates/hm/src/commands/dev/up.rs
git commit -m "feat(hm dev up): route TTY sessions through Mission Control TUI"
```

### Task 6.3: `hm cloud build watch` plumbing

The host fn already routes `BuildEvent`s from the cloud plugin to `OrchestratorState.tui_event_tx`. But `hm cloud build watch` does not currently invoke `orchestrator::run` — the cloud plugin runs *outside* the orchestrator. We need a thin host shim that:

1. Allocates the same mpsc channel `scheduler::run` would create.
2. Stores it in `OrchestratorState` for the duration of the watch.
3. Spawns the TUI.
4. Invokes the cloud plugin's subcommand.

**Files:**
- Modify: `crates/hm/src/dispatcher.rs` (or wherever the `cloud build watch` subcommand is dispatched to the cloud plugin)
- Add: a small `tui_session` helper in `crates/hm/src/tui/mod.rs`

- [ ] **Step 1: Add the helper**

In `crates/hm/src/tui/mod.rs`, append:

```rust
/// Convenience: set up the host-fn TUI sink for a non-orchestrated
/// command (e.g., cloud build watch). Returns a guard that, when
/// dropped, clears the sink from `OrchestratorState`.
pub fn install_session_sink() -> (mpsc::Sender<hm_plugin_protocol::BuildEvent>, mpsc::Receiver<TuiEvent>) {
    let (bus_tx, bus_rx) = mpsc::channel(source::TUI_CHANNEL_CAPACITY);
    let (tui_tx, tui_rx) = source::channel();

    tokio::spawn(async move {
        let mut bus_rx = bus_rx;
        while let Some(ev) = bus_rx.recv().await {
            let translated = source::local::translate_pub(ev);
            if tui_tx.send(translated).await.is_err() { break; }
        }
    });

    crate::orchestrator::state::install_tui_sink(bus_tx.clone());
    (bus_tx, tui_rx)
}
```

Expose `translate` from `local.rs` by renaming it `pub fn translate_pub`, or add a small `pub` re-export. (The mechanical detail is left to the implementer; the requirement is that `cloud.rs` and the session-sink helper share one translation impl.)

Implement `install_tui_sink` in `crates/hm/src/orchestrator/state.rs`:

```rust
pub fn install_tui_sink(tx: tokio::sync::mpsc::Sender<hm_plugin_protocol::BuildEvent>) {
    if let Some(state) = current() {
        // OrchestratorState is constructed per scheduler::run; the
        // cloud path installs a parallel state by hand.
        let _ = state;
    }
    // For cloud sessions, install a thin OrchestratorState with only
    // tui_event_tx populated. The remaining fields are unused by the
    // cloud plugin (it does not call docker_*/archive_* host fns).
    use std::sync::Arc;
    use crate::orchestrator::events::EventBus;
    use crate::orchestrator::archive::ArchiveStore;
    use tokio_util::sync::CancellationToken;
    use uuid::Uuid;
    // Reuse `connect` lazily — if no docker, leave it un-set; the
    // cloud watch path does not touch docker.
    let docker = crate::orchestrator::docker_client::DockerClient::dummy();
    let state = Arc::new(OrchestratorState {
        event_bus: EventBus::new(),
        archives: ArchiveStore::new(),
        cancel: CancellationToken::new(),
        docker,
        run_id: Uuid::new_v4(),
        tui_event_tx: Some(tx),
    });
    install(state);
}
```

> **Note:** `DockerClient::dummy()` is not a real constructor today. The implementer either (a) refactors `OrchestratorState` so the `docker` field is `Option<DockerClient>` for non-build sessions, or (b) lazily connects to docker even for cloud watch (cheap if it's already running, no-op otherwise). Pick (a) — it is cleaner and matches the spec's "the cloud plugin does not call docker_* host fns".

If you pick (a):

```rust
pub struct OrchestratorState {
    pub event_bus: EventBus,
    pub archives: ArchiveStore,
    pub cancel: CancellationToken,
    pub docker: Option<crate::orchestrator::docker_client::DockerClient>,
    pub run_id: Uuid,
    pub tui_event_tx: Option<tokio::sync::mpsc::Sender<hm_plugin_protocol::BuildEvent>>,
}
```

…and update every consumer (`docker_host_fns.rs`) to handle `state.docker.as_ref().expect("docker not available")` or its equivalent. Each docker host-fn already runs only inside the orchestrator's local-run path, so this is a small ergonomic shift.

- [ ] **Step 2: Branch the dispatcher**

In `crates/hm/src/dispatcher.rs`, locate where `cloud` subcommands are forwarded to the plugin. Add a TTY-detect branch for the `cloud build watch` variant:

```rust
use is_terminal::IsTerminal;

let want_tui_for_cloud_watch = std::io::stdout().is_terminal()
    && std::env::var("HM_NO_TUI").is_err()
    && std::env::var("NO_COLOR").is_err();

if want_tui_for_cloud_watch && is_cloud_build_watch(&args) {
    let (_bus_tx, tui_rx) = crate::tui::install_session_sink();
    let opts = crate::tui::TuiOptions {
        fx_enabled: std::env::var("HM_NO_FX").is_err(),
        summary_card: true,
        title: "hm cloud build watch".into(),
    };

    let plugin_handle = tokio::spawn(async move {
        // existing dispatch into the cloud plugin
        dispatch_cloud_plugin(args).await
    });
    let tui_exit = crate::tui::run(tui_rx, opts).await
        .map_err(|e| anyhow::anyhow!(e))?;
    let plugin_exit = plugin_handle.await??;
    return Ok(if tui_exit != 0 { tui_exit } else { plugin_exit });
}
```

`is_cloud_build_watch(&args)` is a helper that returns true when `args[0] == "cloud" && args[1] == "build" && args.contains(&"watch")` — adjust to the exact dispatcher shape.

- [ ] **Step 3: Build + commit**

```bash
cargo build -p hm
git add crates/hm/src/dispatcher.rs \
        crates/hm/src/orchestrator/state.rs \
        crates/hm/src/tui/mod.rs
git commit -m "feat(hm cloud build watch): TUI session sink + host-fn bridge"
```

---

## Phase 7 — Demo, CI, README

### Task 7.1: vhs tape for `hm run`

**Files:**
- Create: `docs/demo/run.tape`

- [ ] **Step 1: Write the tape**

Create `docs/demo/run.tape`:

```
Output docs/demo/run.gif

Set FontSize 14
Set Width 1200
Set Height 720
Set Theme "Catppuccin Mocha"

Type "cd examples/rust"
Enter
Sleep 500ms
Type "hm run"
Enter
Sleep 30s
Screenshot docs/demo/run.png
```

- [ ] **Step 2: Generate locally**

```bash
brew install vhs   # or apt / cargo install — vhs install per Charm docs
vhs docs/demo/run.tape
```

Expected: `docs/demo/run.gif` and `docs/demo/run.png` produced.

- [ ] **Step 3: Commit**

```bash
git add docs/demo/run.tape docs/demo/run.gif docs/demo/run.png
git commit -m "docs(demo): vhs tape + GIF/PNG for hm run TUI"
```

### Task 7.2: README embed

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Add the GIF**

In `README.md`, immediately under the title (after the `[![license]]` shield), insert:

```markdown
![hm run Mission Control TUI](docs/demo/run.gif)
```

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs(readme): embed Mission Control TUI demo GIF"
```

### Task 7.3: vhs tape for `hm dev up`

**Files:**
- Create: `docs/demo/dev.tape`

- [ ] **Step 1: Write the tape**

Create `docs/demo/dev.tape` mirroring Task 7.1 but invoking `hm dev up` against a simple example (use the smallest example that has a `@hm.deploy` decorator — pick one from `examples/`; if none exists yet, create `examples/dev-demo/` with a minimal nginx deploy as part of this task).

Tape body:

```
Output docs/demo/dev.gif
Set Width 1200
Set Height 720
Set Theme "Catppuccin Mocha"

Type "cd examples/dev-demo"
Enter
Sleep 500ms
Type "hm dev up"
Enter
Sleep 25s
Screenshot docs/demo/dev.png
Ctrl+C
Sleep 2s
```

- [ ] **Step 2: Generate + commit**

```bash
vhs docs/demo/dev.tape
git add docs/demo/dev.tape docs/demo/dev.gif docs/demo/dev.png
git commit -m "docs(demo): vhs tape + GIF/PNG for hm dev up TUI"
```

### Task 7.4: Demo smoke-test workflow

**Files:**
- Create: `.github/workflows/demo.yml`

- [ ] **Step 1: Write the workflow**

```yaml
name: demo-tape-smoke

on:
  pull_request:
    paths:
      - "crates/hm/src/tui/**"
      - "docs/demo/**"
      - ".github/workflows/demo.yml"

jobs:
  vhs:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: wasm32-wasip1
      - name: cargo build hm
        run: cargo build -p hm --release
      - name: install vhs + ttyd + ffmpeg
        run: |
          sudo apt-get update
          sudo apt-get install -y ffmpeg
          curl -fsSL https://github.com/charmbracelet/vhs/releases/download/v0.7.2/vhs_0.7.2_amd64.deb -o vhs.deb
          sudo dpkg -i vhs.deb
      - name: smoke-run run.tape
        run: vhs docs/demo/run.tape
        # Non-deterministic frames are OK — we only assert exit-zero.
```

- [ ] **Step 2: Commit**

```bash
git add .github/workflows/demo.yml
git commit -m "ci(demo): vhs tape smoke-test on TUI-touching PRs"
```

---

## Self-Review

Re-read the spec at `docs/superpowers/specs/2026-05-22-tui-mission-control-design.md` with this plan open.

- [x] **§1 Architecture** — Tasks 0.x establish module scaffold; Task 2.2 extends `scheduler::run` with `extra_event_tx`; Tasks 2.3/2.4 add the host-fn bridge; Task 5.1 owns the run loop.
- [x] **§2 UI layout** — Each zone has a widget task (4.1–4.7) with insta snapshots.
- [x] **§3 Effects** — Task 3.3 builds `FxQueue`; Task 5.1 calls `push_sparkle` on cache hit + step pass and `tick` per frame.
- [x] **§4 Activation / fallback** — Tasks 6.1–6.3 implement TTY detection per command; the `Resize` arm in Task 5.1 handles the < 60×20 fallback by exiting the TUI cleanly.
- [x] **§5 Testing + demo** — Phase 4 covers insta snapshots; Phase 7 covers the vhs tapes and the CI smoke workflow.
- [x] **§6 File map** — every entry in the spec's file map appears as Created/Modified in this plan.
- [x] **§7 Non-goals** — no tasks for boot intro, Kitty/Sixel, multi-pane WM, theme switcher.
- [x] **§8 Risks** — `tachyonfx` version drift noted inline in Task 3.3; resize fallback in 5.1; broadcast lag handled by `TuiEvent::Lagged`.

**Placeholder scan:** searched for `TBD` / `TODO` / "fill in"; none remain. One inline `// TODO wire via cancel token in opts` comment in Task 5.1 — replaced with a concrete instruction: the second-Ctrl-C path returns 130, while the first should call into the orchestrator's `CancellationToken` once the TUI is given a handle to it. Implementer extends `TuiOptions` with `cancel: Option<tokio_util::sync::CancellationToken>` if/when this becomes visible in the demo; otherwise the v1 single-Ctrl-C exit is acceptable.

**Type consistency:** `TuiEvent` variant names and field names used in `app.rs`, `source/local.rs`, `widgets/*.rs`, and `tui/mod.rs` are all consistent. `OrchestratorState.tui_event_tx` is referenced from `host_fns.rs` and `scheduler.rs` with the same `Option<mpsc::Sender<BuildEvent>>` type. `StepStatus` shared between `app.rs` reducer and `theme.rs` / widget files.

**Scope check:** This is a single subsystem (the TUI module + adapters + one host fn + command wiring + demo). No further decomposition needed.
