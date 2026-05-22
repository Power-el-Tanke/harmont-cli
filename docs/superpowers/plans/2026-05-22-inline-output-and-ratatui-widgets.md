# Inline Output Formatters + Ratatui Widget Refactor — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eliminate the WASM round-trip for built-in `human` / `json` output formatters by moving them into the `hm` crate, and rewrite the TUI widgets to use ratatui's built-in widget types (`Paragraph`, `List`, `Line`, `Span`) instead of hand-rolled `buf.cell_mut().set_symbol()` loops.

**Architecture:**

- **Phase A (output inline):** The `OutputFormatter` SDK trait + capability stay so external plugins can still register formatters. The two built-in implementations (`hm-plugin-output-human`, `hm-plugin-output-json`) move from separate WASM crates into `crates/hm/src/output/formatters/{human,json}.rs`. The orchestrator dispatcher prefers a built-in formatter when `format_name` matches; falls back to the WASM plugin lookup otherwise.
- **Phase B (ratatui widgets):** Each widget in `crates/hm/src/tui/widgets/` is rewritten to emit `Line`/`Span`/`Text` and render via ratatui's `Paragraph`, `List`, `Table`, or `Gauge`. The hand-rolled `for ch in line.chars() { buf.cell_mut(...).set_symbol(...) }` loops are deleted. `Block::default().borders(...).title(...)` stays where it's already used — that's already ratatui-native.

**Tech Stack:** Rust 1.x, ratatui 0.30.0, tokio broadcast, anyhow, tracing, insta snapshot tests.

---

## File Structure

### Phase A — files touched

- **Delete:** `crates/hm-plugin-output-human/` (entire crate)
- **Delete:** `crates/hm-plugin-output-json/` (entire crate)
- **Create:** `crates/hm/src/output/formatters/mod.rs` — `BuiltinFormatter` enum + dispatch
- **Create:** `crates/hm/src/output/formatters/human.rs` — moved from `hm-plugin-output-human/src/render.rs` + writer wrapper
- **Create:** `crates/hm/src/output/formatters/json.rs` — moved from `hm-plugin-output-json/src/lib.rs` render logic + writer wrapper
- **Modify:** `crates/hm/src/output/mod.rs` — add `pub mod formatters;`
- **Modify:** `crates/hm/src/orchestrator/output_subscriber.rs` — dispatch to `BuiltinFormatter` first, only fall through to the plugin registry for unknown formats
- **Modify:** `crates/hm/src/orchestrator/scheduler.rs` — drop the two embedded output WASMs from `embedded`; update the format-validation block to accept the built-in names
- **Modify:** `crates/hm/src/plugin/embedded.rs` — delete `OUTPUT_HUMAN_PLUGIN_WASM` and `OUTPUT_JSON_PLUGIN_WASM` constants
- **Modify:** `crates/hm/src/dispatcher.rs` — drop references to the two embedded output WASMs
- **Modify:** `crates/hm/build.rs` — remove `build_wasm_plugin("hm-plugin-output-human")` and `build_wasm_plugin("hm-plugin-output-json")` lines
- **Modify:** `Cargo.toml` (workspace) — remove the two member entries

### Phase B — files touched

- **Modify:** `crates/hm/src/tui/widgets/log.rs` — replace cell loop with `Paragraph::new(Text::from(lines))`
- **Modify:** `crates/hm/src/tui/widgets/graph.rs` — replace cell loop with per-row `Paragraph::new(Line::from(spans))`
- **Modify:** `crates/hm/src/tui/widgets/footer.rs` — `Paragraph` for key-hints row
- **Modify:** `crates/hm/src/tui/widgets/filter.rs` — `Paragraph` for the `/<query>` input row
- **Modify:** `crates/hm/src/tui/widgets/summary.rs` — `Paragraph` (multi-line text) inside the bordered block
- **Modify:** `crates/hm/src/tui/widgets/help.rs` — `Paragraph` for the help overlay
- **Modify:** `crates/hm/src/tui/widgets/timeline.rs` — `Paragraph` or `List` (whichever the rewrite picks; both yield the same `buffer_to_string` snapshot)

Snapshot tests in each widget file already exist (`buffer_to_string`-based); they pin the rendered output. The refactor must produce the same byte buffer (same glyphs, same positions, same styling) — `insta` accept the snapshot only after visually verifying it matches today's output.

---

## Phase A — Inline Output Formatters

### Task A1: Move the human render fn into the hm crate

**Files:**
- Create: `crates/hm/src/output/formatters/mod.rs`
- Create: `crates/hm/src/output/formatters/human.rs`
- Modify: `crates/hm/src/output/mod.rs`

- [ ] **Step 1: Add the formatters module to `output/mod.rs`**

Open `crates/hm/src/output/mod.rs`. Add at the top of the file (after the existing module-doc comment if there is one, alongside whatever `pub mod ...` declarations are there already):

```rust
pub mod formatters;
```

If `crates/hm/src/output/mod.rs` does not exist yet (only `format.rs` / `status.rs`), check `crates/hm/src/lib.rs` (or `main.rs`) for how the `output` module is declared and add the `formatters` child there.

- [ ] **Step 2: Create `crates/hm/src/output/formatters/mod.rs`**

```rust
//! Built-in BuildEvent formatters. External plugins can still register
//! their own formatter via the `OutputFormatter` capability; these are
//! the in-tree implementations that ship with every build of `hm`.

use hm_plugin_protocol::BuildEvent;

pub mod human;
pub mod json;

/// A formatter that lives inside the `hm` binary. Returned by
/// [`builtin`] for names the orchestrator already knows. The
/// orchestrator's output subscriber falls through to the WASM
/// plugin registry only when this returns `None`.
pub enum Builtin {
    Human(human::Human),
    Json(json::Json),
}

impl Builtin {
    pub fn on_event(&mut self, event: &BuildEvent) {
        match self {
            Self::Human(h) => h.on_event(event),
            Self::Json(j) => j.on_event(event),
        }
    }

    pub fn finalize(&mut self) {
        match self {
            Self::Human(h) => h.finalize(),
            Self::Json(j) => j.finalize(),
        }
    }
}

#[must_use]
pub fn builtin(name: &str) -> Option<Builtin> {
    match name {
        "human" => Some(Builtin::Human(human::Human::default())),
        "json" => Some(Builtin::Json(json::Json::default())),
        _ => None,
    }
}
```

- [ ] **Step 3: Create `crates/hm/src/output/formatters/human.rs` with the moved render fn + a failing test**

Copy the body of `crates/hm-plugin-output-human/src/render.rs` (the `render` fn and its `STEP_KEYS` static) into the new file. Wrap it in a `Human` struct that writes to stderr. The full file:

```rust
//! Human-readable BuildEvent formatter — writes prefixed step logs and
//! brief status lines to stderr. Moved from the standalone
//! `hm-plugin-output-human` WASM crate into the `hm` binary so the
//! built-in formatter does not pay a WASM round-trip per event.

use hm_plugin_protocol::BuildEvent;
use std::collections::HashMap;
use std::io::Write;
use uuid::Uuid;

#[derive(Default)]
pub struct Human {
    step_keys: HashMap<Uuid, String>,
}

impl Human {
    pub fn on_event(&mut self, ev: &BuildEvent) {
        let bytes = self.render(ev);
        if !bytes.is_empty() {
            let _ = std::io::stderr().write_all(&bytes);
        }
    }

    pub fn finalize(&mut self) {}

    fn render(&mut self, ev: &BuildEvent) -> Vec<u8> {
        match ev {
            BuildEvent::BuildStart { plan, .. } => format!(
                "build: {} steps in {} chain(s)\n",
                plan.step_count, plan.chain_count
            )
            .into_bytes(),
            BuildEvent::StepQueued { step_id, key, .. } => {
                self.step_keys.insert(*step_id, key.clone());
                Vec::new()
            }
            BuildEvent::StepStart { step_id, runner, image } => {
                let key = self.key_for(*step_id);
                let line = match image {
                    Some(img) => format!("[{key}] start (runner={runner} image={img})\n"),
                    None => format!("[{key}] start (runner={runner})\n"),
                };
                line.into_bytes()
            }
            BuildEvent::StepLog { step_id, line, .. } => {
                let key = self.key_for(*step_id);
                format!("[{key}] {line}\n").into_bytes()
            }
            BuildEvent::StepCacheHit { step_id, tag, .. } => {
                let key = self.key_for(*step_id);
                format!("[{key}] cache hit ({tag})\n").into_bytes()
            }
            BuildEvent::StepEnd { step_id, exit_code, duration_ms, .. } => {
                let key = self.key_for(*step_id);
                format!("[{key}] end exit={exit_code} duration={duration_ms}ms\n").into_bytes()
            }
            BuildEvent::BuildEnd { exit_code, duration_ms } => format!(
                "build: end exit={exit_code} duration={duration_ms}ms\n"
            )
            .into_bytes(),
            BuildEvent::ChainFailed {
                chain_idx, failed_step_key, exit_code, message, ..
            } => format!(
                "chain {chain_idx}: FAILED at step '{failed_step_key}' (exit={exit_code}): {message}\n"
            )
            .into_bytes(),
        }
    }

    fn key_for(&self, id: Uuid) -> String {
        self.step_keys.get(&id).cloned().unwrap_or_else(|| "?".to_string())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use hm_plugin_protocol::{PlanSummary, StdStream};

    #[test]
    fn build_start_renders_step_and_chain_counts() {
        let mut h = Human::default();
        let s = String::from_utf8(h.render(&BuildEvent::BuildStart {
            run_id: Uuid::nil(),
            plan: PlanSummary {
                step_count: 3,
                chain_count: 2,
                default_runner: "docker".into(),
            },
            started_at: chrono::Utc::now(),
        }))
        .unwrap();
        assert!(s.contains("3 steps"));
        assert!(s.contains("2 chain"));
    }

    #[test]
    fn step_log_renders_with_prefix_after_step_queued_recorded_key() {
        let mut h = Human::default();
        let step_id = Uuid::new_v4();
        h.render(&BuildEvent::StepQueued {
            step_id,
            key: "build".into(),
            chain_idx: 0,
        });
        let s = String::from_utf8(h.render(&BuildEvent::StepLog {
            step_id,
            stream: StdStream::Stdout,
            line: "hello".into(),
            ts: chrono::Utc::now(),
        }))
        .unwrap();
        assert_eq!(s, "[build] hello\n");
    }

    #[test]
    fn step_log_with_unknown_key_renders_question_mark() {
        let mut h = Human::default();
        let s = String::from_utf8(h.render(&BuildEvent::StepLog {
            step_id: Uuid::new_v4(),
            stream: StdStream::Stdout,
            line: "x".into(),
            ts: chrono::Utc::now(),
        }))
        .unwrap();
        assert!(s.starts_with("[?] "));
    }
}
```

- [ ] **Step 4: Run the human tests — they should pass immediately**

Run: `cargo test -p harmont-cli output::formatters::human`

Expected: 3 tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/hm/src/output/formatters/mod.rs crates/hm/src/output/formatters/human.rs crates/hm/src/output/mod.rs
git commit -m "feat(hm): inline human output formatter as native code"
```

---

### Task A2: Move the json render fn into the hm crate

**Files:**
- Create: `crates/hm/src/output/formatters/json.rs`

- [ ] **Step 1: Create `crates/hm/src/output/formatters/json.rs` with a failing test first**

```rust
//! JSON-lines BuildEvent formatter — one event per line to stdout.
//! Moved from the standalone `hm-plugin-output-json` WASM crate.

use hm_plugin_protocol::BuildEvent;
use std::io::Write;

#[derive(Default)]
pub struct Json;

impl Json {
    pub fn on_event(&mut self, ev: &BuildEvent) {
        let Ok(mut bytes) = serde_json::to_vec(ev) else {
            return;
        };
        bytes.push(b'\n');
        let _ = std::io::stdout().write_all(&bytes);
    }

    pub fn finalize(&mut self) {}
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use hm_plugin_protocol::{PlanSummary};
    use uuid::Uuid;

    #[test]
    fn build_start_serialises_to_json_line() {
        let ev = BuildEvent::BuildStart {
            run_id: Uuid::nil(),
            plan: PlanSummary {
                step_count: 1,
                chain_count: 1,
                default_runner: "docker".into(),
            },
            started_at: chrono::Utc::now(),
        };
        let bytes = serde_json::to_vec(&ev).unwrap();
        let s = std::str::from_utf8(&bytes).unwrap();
        assert!(s.contains(r#""BuildStart""#) || s.contains(r#""build_start""#));
        assert!(s.contains(r#""step_count":1"#));
    }
}
```

- [ ] **Step 2: Run the json test**

Run: `cargo test -p harmont-cli output::formatters::json`

Expected: 1 test passes.

- [ ] **Step 3: Commit**

```bash
git add crates/hm/src/output/formatters/json.rs
git commit -m "feat(hm): inline json output formatter as native code"
```

---

### Task A3: Wire the built-in dispatch into `output_subscriber`

**Files:**
- Modify: `crates/hm/src/orchestrator/output_subscriber.rs`

- [ ] **Step 1: Read the current `output_subscriber.rs` end-to-end**

You need to see the full file before changing it because the WASM-dispatch loop has subtle drop-the-lock-before-await behaviour. Open `crates/hm/src/orchestrator/output_subscriber.rs` and read all 107 lines.

- [ ] **Step 2: Replace the loop body so built-in formatters short-circuit the plugin lookup**

Replace the `tokio::spawn(async move { loop { ... } })` block with the version below. Keep the surrounding `pub fn spawn(...)` signature and the surrounding doc-comments unchanged.

```rust
    let mut rx = bus.subscribe();
    let mut builtin = crate::output::formatters::builtin(&format_name);
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    let is_end = matches!(event, BuildEvent::BuildEnd { .. });
                    if let Some(b) = builtin.as_mut() {
                        b.on_event(&event);
                        if is_end {
                            b.finalize();
                            return Ok(());
                        }
                        continue;
                    }
                    // Fall through: format_name is not a built-in;
                    // resolve from the plugin registry.
                    let plugin = {
                        let reg = registry.lock().await;
                        let Some(&idx) = reg.output_formatter_index.get(&format_name) else {
                            if is_end { return Ok(()); }
                            continue;
                        };
                        let Some(p) = reg.get(idx) else {
                            if is_end { return Ok(()); }
                            continue;
                        };
                        p
                    };
                    let _: Result<()> =
                        plugin.call_capability("hm_output_on_event", &event).await;
                    if is_end {
                        let _: Result<Vec<u8>> =
                            plugin.call_capability("hm_output_finalize", &()).await;
                        return Ok(());
                    }
                }
                Err(RecvError::Closed) => return Ok(()),
                Err(RecvError::Lagged(n)) => {
                    tracing::warn!(
                        target: "orchestrator",
                        "output_subscriber: dropped {n} build events (subscriber fell behind)"
                    );
                    eprintln!("[output] dropped {n} build events (subscriber fell behind)");
                }
            }
        }
    })
```

- [ ] **Step 3: Build to confirm types line up**

Run: `cargo build -p harmont-cli`

Expected: clean build. If `crate::output::formatters` is not visible, double-check that `crates/hm/src/lib.rs` (or wherever `output` is declared) re-exports it correctly.

- [ ] **Step 4: Run the orchestrator integration tests**

Run: `cargo test -p harmont-cli orchestrator`

Expected: pass. Any test that mocked the WASM output plugin needs to be checked — if a test asserted on plugin-pool acquire / release for the human or json formatter, that test is now stale.

- [ ] **Step 5: Commit**

```bash
git add crates/hm/src/orchestrator/output_subscriber.rs
git commit -m "feat(hm): dispatch built-in formatters before plugin lookup"
```

---

### Task A4: Update the format-validation block in `scheduler.rs`

**Files:**
- Modify: `crates/hm/src/orchestrator/scheduler.rs:138-153`

- [ ] **Step 1: Read the current validation block**

Open `crates/hm/src/orchestrator/scheduler.rs` and look at lines 133–153. Today it bails when `format_name` is missing from `reg.output_formatter_index`. After Task A3, built-in names like `human` / `json` will not be in that registry index — the bail needs to skip them.

- [ ] **Step 2: Replace the validation block**

Replace the block from `let bad_format: Option<Vec<String>> = { ... };` through the `if let Some(available) = bad_format { ... }` with:

```rust
    let bad_format: Option<Vec<String>> = {
        if crate::output::formatters::builtin(&format_name).is_some() {
            None
        } else {
            let reg = registry.lock().await;
            if reg.output_formatter_index.contains_key(&format_name) {
                None
            } else {
                let mut names: Vec<String> =
                    reg.output_formatter_index.keys().cloned().collect();
                names.push("human".to_string());
                names.push("json".to_string());
                names.sort();
                names.dedup();
                Some(names)
            }
        }
    };
    if let Some(available) = bad_format {
        anyhow::bail!(
            "unknown --format '{format_name}'; available: {}",
            available.join(", ")
        );
    }
```

- [ ] **Step 3: Build and run the format-validation tests**

Run: `cargo test -p harmont-cli -- --include-ignored unknown_format`

Expected: existing tests still pass; the `unknown_format` error path lists the built-ins.

- [ ] **Step 4: Commit**

```bash
git add crates/hm/src/orchestrator/scheduler.rs
git commit -m "feat(hm): accept built-in formatters in format validation"
```

---

### Task A5: Drop the embedded WASM bytes for the two output plugins

**Files:**
- Modify: `crates/hm/src/plugin/embedded.rs`
- Modify: `crates/hm/src/orchestrator/scheduler.rs` (the `embedded:` vec around line 113)
- Modify: `crates/hm/src/dispatcher.rs` (lines around 82 / 86)
- Modify: `crates/hm/build.rs:72-73`

- [ ] **Step 1: Delete the two constants from `embedded.rs`**

Open `crates/hm/src/plugin/embedded.rs`. Delete the `OUTPUT_HUMAN_PLUGIN_WASM` and `OUTPUT_JSON_PLUGIN_WASM` constants and their doc comments (lines 8–16 in today's file). Keep `DOCKER_PLUGIN_WASM` and `CLOUD_PLUGIN_WASM`.

- [ ] **Step 2: Delete the two entries from the `embedded:` vec in `scheduler.rs`**

In `crates/hm/src/orchestrator/scheduler.rs` around lines 114–127, remove the two `("harmont-output-human", ...)` and `("harmont-output-json", ...)` tuples. The vec keeps only the `("harmont-docker", ...)` tuple.

- [ ] **Step 3: Delete the two references in `dispatcher.rs`**

In `crates/hm/src/dispatcher.rs` around lines 82 / 86, delete any code that references `OUTPUT_HUMAN_PLUGIN_WASM` or `OUTPUT_JSON_PLUGIN_WASM`. If those lines were inside a vec literal mirroring the scheduler's, delete them the same way.

- [ ] **Step 4: Delete the two `build_wasm_plugin` calls in `build.rs`**

In `crates/hm/build.rs` around line 72, delete:

```rust
    build_wasm_plugin("hm-plugin-output-human");
    build_wasm_plugin("hm-plugin-output-json");
```

- [ ] **Step 5: Build to confirm nothing else references the deleted constants**

Run: `cargo build -p harmont-cli`

Expected: clean build. Any unresolved reference here is a sign you missed a file — search for `OUTPUT_HUMAN_PLUGIN_WASM` and `OUTPUT_JSON_PLUGIN_WASM` and clean up.

- [ ] **Step 6: Commit**

```bash
git add crates/hm/src/plugin/embedded.rs crates/hm/src/orchestrator/scheduler.rs crates/hm/src/dispatcher.rs crates/hm/build.rs
git commit -m "chore(hm): drop embedded output-formatter wasms; built-ins are native"
```

---

### Task A6: Delete the two WASM crates from the workspace

**Files:**
- Delete: `crates/hm-plugin-output-human/` (entire directory)
- Delete: `crates/hm-plugin-output-json/` (entire directory)
- Modify: `Cargo.toml` (workspace `members`)

- [ ] **Step 1: Remove the two members from the workspace `Cargo.toml`**

Open the workspace `Cargo.toml` at the repo root (`/home/marko/harmont-cli/Cargo.toml`). Find the `[workspace] members = [...]` block. Delete the two lines:

```toml
    "crates/hm-plugin-output-human",
    "crates/hm-plugin-output-json",
```

- [ ] **Step 2: Delete the two crate directories**

```bash
rm -rf crates/hm-plugin-output-human crates/hm-plugin-output-json
```

- [ ] **Step 3: Build + test the workspace**

Run: `cargo build && cargo test -p harmont-cli`

Expected: clean build, all hm tests pass.

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml crates/hm-plugin-output-human crates/hm-plugin-output-json
git commit -m "chore(hm): delete output-formatter wasm crates"
```

---

### Task A7: End-to-end smoke test of both formatters

- [ ] **Step 1: Run a real `hm run` with `--no-tui --format human`**

```bash
cd ~/simci && RUST_LOG=error ../harmont-cli/target/debug/hm run --no-tui --format human 2>&1 | head -10
```

Expected: same `build: N steps in M chain(s)`, `[key] start (...)`, `[key] end exit=0 ...` lines as before.

- [ ] **Step 2: Run with `--no-tui --format json`**

```bash
cd ~/simci && RUST_LOG=error ../harmont-cli/target/debug/hm run --no-tui --format json 2>&1 | head -5
```

Expected: one JSON object per line on stdout, each line a serialised `BuildEvent`.

- [ ] **Step 3: Run with default (TUI) and confirm no scrolling-log regression**

```bash
cd ~/simci && ../harmont-cli/target/debug/hm run
```

Expected: TUI renders cleanly; no log lines escape the log pane (Phase A doesn't fix that bug — it was fixed in a prior commit — but confirm it's still fixed).

---

## Phase B — Ratatui Built-in Widgets

Every widget below today contains a `for ch in line.chars() { buf.cell_mut(...).set_symbol(...) }` loop or equivalent. Ratatui provides `Paragraph`, `List`, `Line`, and `Span` for this exact job. The refactor replaces hand-rolled rendering with these types and deletes the inner loops.

Each widget already has a snapshot test in its file (uses `crate::tui::widgets::buffer_to_string`). The snapshot guards the visual output — if the rewrite changes the rendered glyphs or styling, `cargo insta review` will surface the diff.

### Task B1: Rewrite `log.rs` to use `Paragraph` + `List`

**Files:**
- Modify: `crates/hm/src/tui/widgets/log.rs:27-86`

- [ ] **Step 1: Run the existing snapshot test to capture the current output as the baseline**

Run: `cargo test -p harmont-cli tui::widgets::log::tests`

Expected: PASS (the snapshot is already accepted).

- [ ] **Step 2: Replace the `render` body**

Open `crates/hm/src/tui/widgets/log.rs`. Replace the whole `impl Widget for LogPane<'_>` block with:

```rust
impl Widget for LogPane<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        use ratatui::style::Style;
        use ratatui::text::{Line, Span};
        use ratatui::widgets::Paragraph;

        let chain_label = self
            .state
            .chains
            .get(self.state.focused_chain)
            .map(|c| c.label.clone())
            .unwrap_or_default();
        let title = format!(" log · {chain_label} ");
        let block = Block::default()
            .borders(Borders::ALL)
            .title(title)
            .border_style(self.theme.border(true));
        let inner = block.inner(area);
        block.render(area, buf);

        let Some(step_id) = self.state.focused_step_id() else { return };
        let Some(log) = self.state.logs.get(&step_id) else { return };

        let entries: Vec<_> = log
            .entries
            .iter()
            .filter(|e| self.filter.map_or(true, |f| e.line.contains(f)))
            .collect();

        let height = inner.height as usize;
        let start = entries.len().saturating_sub(height + self.scroll);
        let visible: Vec<Line> = entries
            .iter()
            .skip(start)
            .take(height)
            .map(|entry| {
                let prefix = match entry.stream {
                    hm_plugin_protocol::StdStream::Stdout => "  ",
                    hm_plugin_protocol::StdStream::Stderr => "! ",
                };
                let style = if entry.stream == hm_plugin_protocol::StdStream::Stderr {
                    Style::default().fg(self.theme.text_dim)
                } else {
                    Style::default()
                };
                Line::from(vec![
                    Span::styled(prefix.to_string(), style),
                    Span::styled(entry.line.clone(), style),
                ])
            })
            .collect();

        Paragraph::new(visible).render(inner, buf);

        if log.dropped > 0 {
            let drop_msg = format!("  … {} events dropped (lagged) …", log.dropped);
            let style = Style::default().fg(self.theme.text_dim);
            let line = Line::styled(drop_msg, style);
            let drop_area = Rect::new(inner.x, inner.y, inner.width, 1);
            Paragraph::new(line).render(drop_area, buf);
        }
    }
}
```

- [ ] **Step 3: Re-run the snapshot test and inspect any diff**

Run: `cargo test -p harmont-cli tui::widgets::log::tests`

Expected: PASS. If `insta` reports a diff, run `cargo insta review` and visually confirm the new output renders the same characters in the same positions; only then accept.

- [ ] **Step 4: Commit**

```bash
git add crates/hm/src/tui/widgets/log.rs
git commit -m "refactor(tui): render log pane via ratatui Paragraph"
```

---

### Task B2: Rewrite `graph.rs` to use `Line` + `Paragraph` per row

**Files:**
- Modify: `crates/hm/src/tui/widgets/graph.rs:32-65`

- [ ] **Step 1: Run the existing snapshot test**

Run: `cargo test -p harmont-cli tui::widgets::graph::tests`

Expected: PASS.

- [ ] **Step 2: Replace the `render` body**

Open `crates/hm/src/tui/widgets/graph.rs`. Replace the `impl Widget for Graph<'_>` block with:

```rust
impl Widget for Graph<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        use ratatui::style::Style;
        use ratatui::text::{Line, Span};
        use ratatui::widgets::Paragraph;

        let block = Block::default()
            .borders(Borders::ALL)
            .title(" graph ")
            .border_style(self.theme.border(false));
        let inner = block.inner(area);
        block.render(area, buf);

        let max_rows = inner.height as usize;
        let rows: Vec<Line> = self
            .state
            .chains
            .iter()
            .take(max_rows)
            .map(|chain| {
                let mut spans: Vec<Span> = Vec::new();
                let mut first = true;
                for sid in &chain.steps {
                    let Some(step) = self.state.steps.get(sid) else { continue };
                    if !first {
                        spans.push(Span::raw("─"));
                    }
                    spans.push(Span::styled(
                        glyph(&step.status).to_string(),
                        self.theme.status(step.status.clone()),
                    ));
                    first = false;
                }
                if spans.is_empty() {
                    Line::from(Span::styled(String::new(), Style::default()))
                } else {
                    Line::from(spans)
                }
            })
            .collect();

        Paragraph::new(rows).render(inner, buf);
    }
}
```

- [ ] **Step 3: Re-run the snapshot, review any diff**

Run: `cargo test -p harmont-cli tui::widgets::graph::tests`

Expected: PASS, or accept the snapshot only after `cargo insta review` shows identical visible output.

- [ ] **Step 4: Commit**

```bash
git add crates/hm/src/tui/widgets/graph.rs
git commit -m "refactor(tui): render graph via ratatui Paragraph"
```

---

### Task B3: Rewrite `footer.rs` to use `Paragraph`

**Files:**
- Modify: `crates/hm/src/tui/widgets/footer.rs`

- [ ] **Step 1: Read `footer.rs` end-to-end**

Open `crates/hm/src/tui/widgets/footer.rs`. Identify the cell loop that draws the key-hint string and the styling currently applied (likely `theme.text_dim` for the hints).

- [ ] **Step 2: Run the snapshot test**

Run: `cargo test -p harmont-cli tui::widgets::footer::tests`

Expected: PASS.

- [ ] **Step 3: Replace the cell loop with a `Paragraph`**

In the `impl Widget for Footer<'_> { fn render(...) { ... } }` block, replace the cell-rendering loop with:

```rust
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

let line = Line::styled(hint_text, Style::default().fg(self.theme.text_dim));
Paragraph::new(line).render(area, buf);
```

Where `hint_text` is the same `String` the loop was iterating over. If the footer composes multiple coloured segments, build a `Vec<Span>` and pass `Line::from(spans)` into `Paragraph::new`.

- [ ] **Step 4: Re-run the snapshot, review any diff**

Run: `cargo test -p harmont-cli tui::widgets::footer::tests`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/hm/src/tui/widgets/footer.rs
git commit -m "refactor(tui): render footer via ratatui Paragraph"
```

---

### Task B4: Rewrite `filter.rs` to use `Paragraph`

**Files:**
- Modify: `crates/hm/src/tui/widgets/filter.rs`

- [ ] **Step 1: Read `filter.rs` end-to-end and note any tests**

Open `crates/hm/src/tui/widgets/filter.rs`. The widget renders a one-line `/<query>` prompt at the bottom of the log pane.

- [ ] **Step 2: Replace the cell loop with a `Paragraph`**

```rust
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

let line = Line::from(vec![
    Span::styled("/", Style::default().fg(self.theme.accent_a)),
    Span::raw(self.query.to_string()),
]);
Paragraph::new(line).render(area, buf);
```

- [ ] **Step 3: Run the snapshot tests (if any) and the binary**

Run: `cargo test -p harmont-cli tui::widgets::filter`

Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/hm/src/tui/widgets/filter.rs
git commit -m "refactor(tui): render filter input via ratatui Paragraph"
```

---

### Task B5: Rewrite `summary.rs` to use `Paragraph` inside the block

**Files:**
- Modify: `crates/hm/src/tui/widgets/summary.rs`

- [ ] **Step 1: Read the file end-to-end**

`summary.rs` renders the end-of-build card with totals and durations. Identify the lines composed (one Line per summary row) and their styles.

- [ ] **Step 2: Run the snapshot test**

Run: `cargo test -p harmont-cli tui::widgets::summary`

Expected: PASS.

- [ ] **Step 3: Replace the cell rendering with a `Vec<Line>` + `Paragraph`**

Compose each summary row as a `Line::from(vec![Span::styled(label, label_style), Span::raw(value)])` and render the whole `Vec<Line>` via:

```rust
use ratatui::widgets::Paragraph;

let block = Block::default()
    .borders(Borders::ALL)
    .title(" summary ")
    .border_style(self.theme.border(false));
let inner = block.inner(area);
block.render(area, buf);
Paragraph::new(lines).render(inner, buf);
```

Where `lines: Vec<Line>` is built from the same fields the previous code consulted (chain count, step count, pass/fail/cache totals, total duration). Preserve exact label text and ordering — the snapshot test will catch any deviation.

- [ ] **Step 4: Run the snapshot test, review any diff**

Run: `cargo test -p harmont-cli tui::widgets::summary`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/hm/src/tui/widgets/summary.rs
git commit -m "refactor(tui): render summary card via ratatui Paragraph"
```

---

### Task B6: Rewrite `help.rs` to use `Paragraph`

**Files:**
- Modify: `crates/hm/src/tui/widgets/help.rs`

- [ ] **Step 1: Read the file end-to-end**

`help.rs` renders the `?` keyboard-shortcuts overlay. Identify the rows (key + description) and styling.

- [ ] **Step 2: Replace the cell loops with `Paragraph`**

```rust
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

let lines: Vec<Line> = HELP_ROWS
    .iter()
    .map(|(key, desc)| {
        Line::from(vec![
            Span::styled(
                format!("  {key:<8} "),
                Style::default().fg(self.theme.accent_a).add_modifier(Modifier::BOLD),
            ),
            Span::raw(desc.to_string()),
        ])
    })
    .collect();

let block = Block::default()
    .borders(Borders::ALL)
    .title(" help ")
    .border_style(self.theme.border(true));
let inner = block.inner(area);
block.render(area, buf);
Paragraph::new(lines).render(inner, buf);
```

Where `HELP_ROWS` is the existing `&[(&str, &str)]` slice the previous code iterated over. Keep the same key/desc pairs and ordering.

- [ ] **Step 3: Run tests**

Run: `cargo test -p harmont-cli tui::widgets::help`

Expected: PASS (or accept identical snapshot).

- [ ] **Step 4: Commit**

```bash
git add crates/hm/src/tui/widgets/help.rs
git commit -m "refactor(tui): render help overlay via ratatui Paragraph"
```

---

### Task B7: Rewrite `timeline.rs` to use `Paragraph` per row

**Files:**
- Modify: `crates/hm/src/tui/widgets/timeline.rs`

- [ ] **Step 1: Read the file end-to-end and note the row composition**

`timeline.rs` renders one row per running/completed step with a label and a relative duration bar. Identify the per-row composition (label + bar glyphs).

- [ ] **Step 2: Run the snapshot test**

Run: `cargo test -p harmont-cli tui::widgets::timeline`

Expected: PASS.

- [ ] **Step 3: Replace the cell loops with `Line` + `Paragraph`**

Build `Vec<Line>` where each `Line` is composed of a label `Span` and a bar `Span` (the bar is the same `█`-or-similar glyph the previous code wrote into cells, just placed in a `Span::styled(bar_string, style)`). Render with:

```rust
use ratatui::widgets::Paragraph;

let block = Block::default()
    .borders(Borders::ALL)
    .title(" timeline ")
    .border_style(self.theme.border(false));
let inner = block.inner(area);
block.render(area, buf);
Paragraph::new(rows).render(inner, buf);
```

- [ ] **Step 4: Re-run the snapshot, review any diff**

Run: `cargo test -p harmont-cli tui::widgets::timeline`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/hm/src/tui/widgets/timeline.rs
git commit -m "refactor(tui): render timeline via ratatui Paragraph"
```

---

### Task B8: Final pass — confirm no `cell_mut` remains in widgets

- [ ] **Step 1: Grep for any leftover `cell_mut` or `set_symbol` in the widget files**

Run: `grep -rn "cell_mut\|set_symbol" crates/hm/src/tui/widgets/`

Expected: zero hits. If anything remains, that widget still has hand-rolled rendering — repeat the pattern from the earlier tasks.

- [ ] **Step 2: Run the full TUI test suite**

Run: `cargo test -p harmont-cli tui`

Expected: PASS.

- [ ] **Step 3: Smoke-test by hand**

```bash
cd ~/simci && ../harmont-cli/target/debug/hm run
```

Expected: TUI renders identically to before the refactor. The log pane shows logs; the graph row shows step glyphs; the footer shows key hints; `?` opens help; `/` opens filter.

- [ ] **Step 4: If everything looks good, commit any leftover snapshot changes**

```bash
git add crates/hm/src/tui/widgets/snapshots/
git commit -m "test(tui): accept ratatui-rendered widget snapshots"
```

(If `cargo insta review` already rolled these into per-task commits, this step is a no-op.)

---

## Self-Review Notes

- **Spec coverage:** Phase A merges the human + json output plugins into the CLI binary (the first half of the user's ask). Phase B converts each TUI widget from hand-rolled `cell_mut` rendering to ratatui's built-in widget types (the second half). No spec requirement is unaddressed.
- **External output plugins:** the `OutputFormatter` SDK trait, the `Capability::OutputFormatter` variant, the `hm_output_on_event` capability export, and the registry-based fallback in `output_subscriber` all stay intact. External plugins keep working — the change is internal: built-ins skip the WASM path.
- **Tests:** every modified widget already has a snapshot test (`buffer_to_string`-based). The plan asks the engineer to run each before and after; if `insta` flags a diff, the engineer reviews visually before accepting. The output formatter logic is preserved verbatim (same `render` body for human; same `serde_json::to_vec` + newline for json), so existing unit tests transfer with the file.
- **Commits:** one commit per task, every task ends with a `git add` + `git commit`. DRY: per-widget tasks share the same `Block` / `Paragraph` skeleton but the code is repeated in each task because an engineer may execute them out of order.
