# Colorize CLI Output — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Colorize the default `hm run` output (progress bars + post-build summary) so it looks polished enough for an HN launch.

**Architecture:** Add raw ANSI color helpers to `progress.rs` (consistent with `logmux.rs` pattern), thread the existing `--no-color` flag into renderers, colorize indicatif progress styles + step summary + success/failure banners. No new crate dependencies — indicatif handles its own color via `console` internally, and our summary output uses raw ANSI codes gated on a `color: bool` field.

**Tech Stack:** Rust, indicatif (already dep), tracing-indicatif (already dep), raw ANSI escape codes.

**Visual targets:**

During build (indicatif progress bars):
```
⠋ pipeline  ████████░░░░░░░░  5/8 steps  (12.3s)
  ✓ :apt: base  (cached)          ← green ✓
  ✓ :python: uv-sync  (2.1s)     ← green ✓
  ⠋ :rust: clippy  (8.4s)        ← cyan spinner
  ⠋ :python: lint  (6.1s)        ← cyan spinner
```

After build (summary):
```
  ✓ :apt: base            cached     ← green ✓, dim "cached"
  ✓ :python: uv-sync      2.1s      ← green ✓, dim timing
  ✓ :rust: clippy          24.3s
  ✗ :python: test          FAILED    ← red ✗, red "FAILED"
  - :rust: build           —         ← dim dash for cancelled

  --- :python: test failed (exit 1) ---   ← red header
  assertion failed at line 42             ← normal text
  expected 5, got 3

✗ Build failed in 12.3s                  ← red bold
```

Or on success:
```
  ✓ :apt: base            cached
  ✓ :python: uv-sync      2.1s
  ✓ :rust: clippy          24.3s
  ✓ :python: test          4.2s

✓ Build succeeded in 34.2s              ← green bold
```

---

### Task 1: Thread color flag into `ProgressRenderer` + add ANSI helpers

**Context:** `ProgressRenderer` currently has no awareness of color. The `--no-color` flag exists in CLI args and flows into `OutputMode::Human { color, .. }` in `context.rs`, but is never threaded to the renderer. We need a `color: bool` field and small helper functions for ANSI styling (consistent with the raw ANSI pattern in `logmux.rs`).

**Files:**
- Modify: `crates/hm/src/output/progress.rs` — add `color` field, helpers, update constructor
- Modify: `crates/hm/src/commands/run/local.rs:99-106` — pass color flag to `ProgressRenderer::new`
- Modify: `crates/hm/src/context.rs:33-36` — also respect `NO_COLOR` env and stderr isatty

**Step 1: Write the failing test**

In `crates/hm/src/output/progress.rs`, add to the test module:

```rust
#[test]
fn color_flag_stored() {
    let r = ProgressRenderer::new(Vec::new(), true);
    assert!(r.color);
    let r2 = ProgressRenderer::new(Vec::new(), false);
    assert!(!r2.color);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p harmont-cli --lib output::progress::tests::color_flag_stored`
Expected: FAIL — `ProgressRenderer::new` takes 1 arg, not 2.

**Step 3: Implement color field + helpers**

In `crates/hm/src/output/progress.rs`:

1. Add `color: bool` field to `ProgressRenderer`:
```rust
pub struct ProgressRenderer<W> {
    out: W,
    color: bool,
    // ... rest unchanged
}
```

2. Update `new()`:
```rust
pub fn new(out: W, color: bool) -> Self {
    Self {
        out,
        color,
        // ... rest unchanged
    }
}
```

3. Add ANSI helper functions (private, at module level — consistent with `logmux.rs`):
```rust
const GREEN: &str = "\x1b[32m";
const RED: &str = "\x1b[31m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const RESET: &str = "\x1b[0m";

fn ansi(text: &str, codes: &str, color: bool) -> String {
    if color {
        format!("{codes}{text}{RESET}")
    } else {
        text.to_string()
    }
}
```

4. Update the `renderer()` test helper:
```rust
fn renderer() -> ProgressRenderer<Vec<u8>> {
    ProgressRenderer::new(Vec::new(), false)
}
```

5. Update `local.rs` to pass color:
```rust
"human" => {
    let color = !std::io::stderr().is_terminal()
        .then_some(false)
        .unwrap_or(true)
        && std::env::var("NO_COLOR").is_err();
    // Actually, simpler: derive from RunArgs + env
    Box::new(crate::output::progress::ProgressRenderer::new(
        std::io::stderr(),
        color,
    ))
}
```

Wait — we need the `--no-color` flag. It's on `Cli` not `RunArgs`. The `handle` function in `local.rs` receives `RunArgs` and `RunContext`. `RunContext` has `output: OutputMode` which has `color_enabled()`. Use that:

```rust
"human" => Box::new(crate::output::progress::ProgressRenderer::new(
    std::io::stderr(),
    _ctx.output.color_enabled(),
)),
```

But `_ctx` is currently ignored (prefixed `_`). Rename to `ctx` and use it.

6. Update `context.rs` to also check `NO_COLOR` env var and stderr isatty:
```rust
let color = !cli.no_color
    && std::env::var("NO_COLOR").is_err()
    && std::io::stderr().is_terminal();
```

Note: we check stderr because that's where progress output goes.

**Step 4: Run test to verify it passes**

Run: `cargo test -p harmont-cli --lib output::progress::tests::color_flag_stored`
Expected: PASS

**Step 5: Run full test suite**

Run: `cargo test -p harmont-cli`
Expected: All pass (existing tests use `renderer()` helper which passes `false`).

**Step 6: Commit**

```bash
git add crates/hm/src/output/progress.rs crates/hm/src/commands/run/local.rs crates/hm/src/context.rs
git commit -m "feat: thread color flag into ProgressRenderer + add ANSI helpers"
```

---

### Task 2: Colorize indicatif progress bar styles

**Context:** The three `ProgressStyle` functions (`active_style`, `completed_style`, `failed_style`) and the root bar style in `BuildStart` use plain templates. Indicatif supports inline color syntax like `{spinner:.cyan}` and `{wide_bar:.green}`. We parameterize these by the `color` flag.

**Files:**
- Modify: `crates/hm/src/output/progress.rs` — update `active_style()`, `completed_style()`, `failed_style()`, and root bar in `BuildStart` handler

**Step 1: Write the failing test**

```rust
#[test]
fn active_style_has_spinner() {
    // Verify the style compiles and contains spinner placeholder
    let style = active_style(true);
    // Just assert it doesn't panic — the template is valid
    let _ = style;

    let style_no_color = active_style(false);
    let _ = style_no_color;
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p harmont-cli --lib output::progress::tests::active_style_has_spinner`
Expected: FAIL — `active_style` takes 0 args.

**Step 3: Implement colored styles**

Update the three style functions to take `color: bool`:

```rust
fn active_style(color: bool) -> ProgressStyle {
    let tpl = if color {
        "{span_child_prefix}{spinner:.cyan} {wide_msg}  ({elapsed})"
    } else {
        "{span_child_prefix}{spinner} {wide_msg}  ({elapsed})"
    };
    ProgressStyle::with_template(tpl)
        .unwrap_or_else(|_| ProgressStyle::default_spinner())
}

fn completed_style(color: bool) -> ProgressStyle {
    let tpl = if color {
        "{span_child_prefix}\x1b[32m✓\x1b[0m {wide_msg}"
    } else {
        "{span_child_prefix}✓ {wide_msg}"
    };
    ProgressStyle::with_template(tpl)
        .unwrap_or_else(|_| ProgressStyle::default_spinner())
}

fn failed_style(color: bool) -> ProgressStyle {
    let tpl = if color {
        "{span_child_prefix}\x1b[31m✗\x1b[0m {wide_msg}"
    } else {
        "{span_child_prefix}✗ {wide_msg}"
    };
    ProgressStyle::with_template(tpl)
        .unwrap_or_else(|_| ProgressStyle::default_spinner())
}
```

Update the root bar style in `BuildStart` handler:

```rust
BuildEvent::BuildStart { plan, .. } => {
    let root = info_span!("pipeline");
    let tpl = if self.color {
        "{spinner:.green} {span_name}  {wide_bar:.green/white} {pos}/{len} steps  ({elapsed})"
    } else {
        "{spinner} {span_name}  {wide_bar} {pos}/{len} steps  ({elapsed})"
    };
    root.pb_set_style(
        &ProgressStyle::with_template(tpl)
            .unwrap_or_else(|_| ProgressStyle::default_bar()),
    );
    root.pb_set_length(plan.step_count as u64);
    root.pb_start();
    self.root_span = Some(root);
}
```

Update all call sites to pass `self.color`:
- `StepQueued`: `span.pb_set_style(&active_style(self.color));`
- `StepCacheHit`: `span.pb_set_style(&completed_style(self.color));`
- `StepEnd` success: `span.pb_set_style(&completed_style(self.color));`
- `StepEnd` failure: `span.pb_set_style(&failed_style(self.color));`
- `StepEnd` cancelled: `span.pb_set_style(&completed_style(self.color));`

**Step 4: Run tests**

Run: `cargo test -p harmont-cli --lib output::progress`
Expected: All pass.

**Step 5: Manual test**

Run: `cargo build -p harmont-cli && ./target/debug/hm run`
Verify: Spinner is cyan, ✓ is green, ✗ is red, progress bar is green.

**Step 6: Commit**

```bash
git add crates/hm/src/output/progress.rs
git commit -m "feat: colorize indicatif progress bar styles"
```

---

### Task 3: Refactor `StepTiming` → `StepOutcome` for richer status tracking

**Context:** The step summary needs to show ✓/✗ per step, but `StepTiming` only tracks duration and cache hits — not success/failure. Refactor to `StepOutcome` with four variants covering all terminal states. This makes summary colorization clean.

**Files:**
- Modify: `crates/hm/src/output/progress.rs` — rename enum, add variants, update all usage

**Step 1: Write the failing test**

```rust
#[test]
fn step_outcome_tracks_failure() {
    let mut r = renderer();
    let step_id = Uuid::new_v4();

    r.on_event(&BuildEvent::BuildStart {
        run_id: Uuid::nil(),
        plan: PlanSummary { step_count: 1, chain_count: 1, default_runner: "docker".into() },
        started_at: chrono::Utc::now(),
    });
    r.on_event(&BuildEvent::StepQueued {
        step_id, key: "test".into(), chain_idx: 0,
        parent_key: None, display_name: "test".into(),
    });
    r.on_event(&BuildEvent::StepEnd {
        step_id, exit_code: 1, duration_ms: 500, snapshot: None,
    });

    match r.step_outcomes.get(&step_id) {
        Some(StepOutcome::Failed { exit_code: 1, .. }) => {}
        other => panic!("expected Failed, got {other:?}"),
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p harmont-cli --lib output::progress::tests::step_outcome_tracks_failure`
Expected: FAIL — `step_outcomes` doesn't exist, `StepOutcome` doesn't exist.

**Step 3: Implement StepOutcome**

Replace `StepTiming` with:

```rust
#[derive(Debug)]
enum StepOutcome {
    Succeeded { duration_ms: u64 },
    Failed { duration_ms: u64, exit_code: i32 },
    Cancelled { duration_ms: u64 },
    Cached,
}
```

In `ProgressRenderer`:
- Rename `step_timings` → `step_outcomes`
- Type: `HashMap<Uuid, StepOutcome>`

Update `StepCacheHit` handler:
```rust
self.step_outcomes.insert(*step_id, StepOutcome::Cached);
```

Update `StepEnd` handler:
```rust
let cancelled = *exit_code == 130;
let outcome = if *exit_code == 0 {
    StepOutcome::Succeeded { duration_ms: *duration_ms }
} else if cancelled {
    StepOutcome::Cancelled { duration_ms: *duration_ms }
} else {
    StepOutcome::Failed { duration_ms: *duration_ms, exit_code: *exit_code }
};
self.step_outcomes.insert(*step_id, outcome);
```

Update `print_step_summary` to use `step_outcomes` (keep existing format for now — next task colorizes):
```rust
fn print_step_summary(&mut self) {
    let _ = writeln!(self.out);
    for step_id in &self.step_order {
        let name = self.step_names.get(step_id).map_or("?", String::as_str);
        let timing = match self.step_outcomes.get(step_id) {
            Some(StepOutcome::Succeeded { duration_ms }) => format_duration(*duration_ms),
            Some(StepOutcome::Failed { duration_ms, .. }) => format_duration(*duration_ms),
            Some(StepOutcome::Cancelled { .. }) => "cancelled".into(),
            Some(StepOutcome::Cached) => "cached".into(),
            None => "—".into(),
        };
        let _ = writeln!(self.out, "  {name}  {timing}");
    }
}
```

**Step 4: Run tests**

Run: `cargo test -p harmont-cli --lib output::progress`
Expected: All pass.

**Step 5: Commit**

```bash
git add crates/hm/src/output/progress.rs
git commit -m "refactor: StepTiming → StepOutcome with success/failure/cancelled variants"
```

---

### Task 4: Colorize step summary with aligned columns

**Context:** The step summary currently prints `"  {name}  {timing}"` with no color and no alignment. We want: colored ✓/✗ prefix, right-aligned timing column, dim styling for cached/durations.

**Files:**
- Modify: `crates/hm/src/output/progress.rs` — rewrite `print_step_summary`

**Step 1: Write the failing test**

```rust
#[test]
fn summary_includes_status_indicators() {
    let mut r = renderer();
    let s1 = Uuid::new_v4();
    let s2 = Uuid::new_v4();

    r.on_event(&BuildEvent::BuildStart {
        run_id: Uuid::nil(),
        plan: PlanSummary { step_count: 2, chain_count: 1, default_runner: "docker".into() },
        started_at: chrono::Utc::now(),
    });

    // Step 1: succeeds
    r.on_event(&BuildEvent::StepQueued {
        step_id: s1, key: "build".into(), chain_idx: 0,
        parent_key: None, display_name: "build".into(),
    });
    r.on_event(&BuildEvent::StepEnd { step_id: s1, exit_code: 0, duration_ms: 200, snapshot: None });

    // Step 2: fails
    r.on_event(&BuildEvent::StepQueued {
        step_id: s2, key: "test".into(), chain_idx: 0,
        parent_key: None, display_name: "test".into(),
    });
    r.on_event(&BuildEvent::StepEnd { step_id: s2, exit_code: 1, duration_ms: 300, snapshot: None });

    r.on_event(&BuildEvent::BuildEnd { exit_code: 1, duration_ms: 600 });

    let s = output(&r);
    // Test renderer has color=false, so plain ✓/✗
    assert!(s.contains("✓"), "expected ✓ in summary: {s}");
    assert!(s.contains("✗"), "expected ✗ in summary: {s}");
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p harmont-cli --lib output::progress::tests::summary_includes_status_indicators`
Expected: FAIL — current summary has no ✓/✗ prefix.

**Step 3: Implement colorized summary**

Rewrite `print_step_summary`:

```rust
fn print_step_summary(&mut self) {
    let max_name_len = self.step_order.iter()
        .filter_map(|id| self.step_names.get(id))
        .map(|n| n.len())
        .max()
        .unwrap_or(0);

    let _ = writeln!(self.out);
    for step_id in &self.step_order {
        let name = self.step_names.get(step_id).map_or("?", String::as_str);
        let (indicator, timing) = match self.step_outcomes.get(step_id) {
            Some(StepOutcome::Succeeded { duration_ms }) => (
                ansi("✓", GREEN, self.color),
                ansi(&format_duration(*duration_ms), DIM, self.color),
            ),
            Some(StepOutcome::Failed { exit_code, .. }) => (
                ansi("✗", RED, self.color),
                ansi(&format!("FAILED (exit {exit_code})"), RED, self.color),
            ),
            Some(StepOutcome::Cancelled { .. }) => (
                ansi("-", DIM, self.color),
                ansi("cancelled", DIM, self.color),
            ),
            Some(StepOutcome::Cached) => (
                ansi("✓", GREEN, self.color),
                ansi("cached", DIM, self.color),
            ),
            None => (
                ansi("-", DIM, self.color),
                ansi("—", DIM, self.color),
            ),
        };
        let _ = writeln!(self.out, "  {indicator} {name:<max_name_len$}  {timing}");
    }
}
```

**Step 4: Run tests**

Run: `cargo test -p harmont-cli --lib output::progress`
Expected: All pass. The `no_output_on_success` test should still pass since it checks for "Build succeeded" which comes from the banner, not the summary.

**Step 5: Manual test**

Run: `cargo build -p harmont-cli && ./target/debug/hm run`
Verify: Green ✓, dim timings, aligned columns.

**Step 6: Commit**

```bash
git add crates/hm/src/output/progress.rs
git commit -m "feat: colorize step summary with status indicators and aligned columns"
```

---

### Task 5: Colorize success/failure banners and failure report

**Context:** The "✓ Build succeeded" / failure report currently uses plain text. Make them bold+colored for visual impact.

**Files:**
- Modify: `crates/hm/src/output/progress.rs` — update `BuildEnd` handler and `print_failure_report`

**Step 1: Write the failing test**

```rust
#[test]
fn success_banner_contains_checkmark() {
    let mut r = ProgressRenderer::new(Vec::new(), true);
    let step_id = Uuid::new_v4();

    r.on_event(&BuildEvent::BuildStart {
        run_id: Uuid::nil(),
        plan: PlanSummary { step_count: 1, chain_count: 1, default_runner: "docker".into() },
        started_at: chrono::Utc::now(),
    });
    r.on_event(&BuildEvent::StepQueued {
        step_id, key: "build".into(), chain_idx: 0,
        parent_key: None, display_name: "build".into(),
    });
    r.on_event(&BuildEvent::StepEnd { step_id, exit_code: 0, duration_ms: 200, snapshot: None });
    r.on_event(&BuildEvent::BuildEnd { exit_code: 0, duration_ms: 250 });

    let s = output(&r);
    // With color=true, should contain green ANSI code
    assert!(s.contains("\x1b["), "expected ANSI codes in colored output: {s}");
    assert!(s.contains("Build succeeded"), "expected success message: {s}");
}

#[test]
fn failure_report_header_contains_failed() {
    let mut r = ProgressRenderer::new(Vec::new(), true);
    let step_id = Uuid::new_v4();

    r.on_event(&BuildEvent::BuildStart {
        run_id: Uuid::nil(),
        plan: PlanSummary { step_count: 1, chain_count: 1, default_runner: "docker".into() },
        started_at: chrono::Utc::now(),
    });
    r.on_event(&BuildEvent::StepQueued {
        step_id, key: "test".into(), chain_idx: 0,
        parent_key: None, display_name: "test".into(),
    });
    r.on_event(&BuildEvent::StepLog {
        step_id, stream: StdStream::Stderr,
        line: "error line".into(), ts: chrono::Utc::now(),
    });
    r.on_event(&BuildEvent::StepEnd { step_id, exit_code: 1, duration_ms: 500, snapshot: None });
    r.on_event(&BuildEvent::BuildEnd { exit_code: 1, duration_ms: 600 });

    let s = output(&r);
    assert!(s.contains("\x1b["), "expected ANSI in failure report: {s}");
    assert!(s.contains("failed"), "expected 'failed' in report: {s}");
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p harmont-cli --lib output::progress::tests::success_banner_contains_checkmark output::progress::tests::failure_report_header_contains_failed`
Expected: FAIL — no ANSI codes in output yet.

**Step 3: Implement colored banners and failure report**

Update `BuildEnd` handler in `on_event`:

```rust
BuildEvent::BuildEnd { exit_code, duration_ms } => {
    self.step_spans.clear();
    self.root_span.take();

    self.print_step_summary();

    if *exit_code != 0 {
        self.print_failure_report();
        let dur = format_duration(*duration_ms);
        let msg = format!("✗ Build failed in {dur}");
        let _ = writeln!(self.out, "{}", ansi(&msg, &format!("{RED}{BOLD}"), self.color));
    } else {
        let dur = format_duration(*duration_ms);
        let msg = format!("✓ Build succeeded in {dur}");
        let _ = writeln!(self.out, "{}", ansi(&msg, &format!("{GREEN}{BOLD}"), self.color));
    }
}
```

Update `print_failure_report`:

```rust
fn print_failure_report(&mut self) {
    for (step_id, exit_code) in &self.failed_steps {
        let key = self.step_keys.get(step_id).map_or("?", String::as_str);
        let header = format!("--- {key} failed (exit {exit_code}) ---");
        let _ = writeln!(self.out, "\n{}", ansi(&header, RED, self.color));
        if let Some(lines) = self.log_buffer.get(step_id) {
            for line in lines {
                let _ = writeln!(self.out, "{line}");
            }
        }
    }
}
```

Note: `ansi` helper needs to work with both `&str` codes and concatenated codes. Since we defined `ansi(text, codes, color)` where `codes: &str`, for compound styles like bold+red, pass `"\x1b[31m\x1b[1m"` or define a helper:

```rust
const GREEN_BOLD: &str = "\x1b[32;1m";
const RED_BOLD: &str = "\x1b[31;1m";
```

**Step 4: Run tests**

Run: `cargo test -p harmont-cli --lib output::progress`
Expected: All pass.

**Step 5: Manual test**

Run: `cargo build -p harmont-cli && ./target/debug/hm run`
Verify:
- Success: green bold "✓ Build succeeded in Xs"
- Failure: red header, red bold "✗ Build failed in Xs"

**Step 6: Commit**

```bash
git add crates/hm/src/output/progress.rs
git commit -m "feat: colorize success/failure banners and failure report"
```

---

### Task 6: Enable ANSI in tracing fmt layer when color is enabled

**Context:** `main.rs` hardcodes `.with_ansi(false)` on the tracing fmt layer. This means all `tracing::info!`, `tracing::warn!`, `tracing::error!` output is uncolored. We should enable ANSI when color is enabled, so tracing output (errors, warnings) also gets colored.

**Files:**
- Modify: `crates/hm/src/main.rs` — derive color flag from CLI args, pass to `with_ansi()`

**Step 1: Implement (no test needed — this is a wiring change in main)**

In `main.rs`, after parsing args:

```rust
let color = !args.no_color
    && std::env::var("NO_COLOR").is_err()
    && std::io::stderr().is_terminal();
```

Then replace both `.with_ansi(false)` occurrences with `.with_ansi(color)`:

In the indicatif branch:
```rust
.with_ansi(color)
```

In the else branch:
```rust
.with_ansi(color)
```

**Step 2: Manual test**

Run: `RUST_LOG=debug cargo build -p harmont-cli && ./target/debug/hm run nonexistent-pipeline`
Verify: Error messages are colored (red) when terminal supports it.

Run: `./target/debug/hm --no-color run nonexistent-pipeline`
Verify: Error messages are plain text.

**Step 3: Commit**

```bash
git add crates/hm/src/main.rs
git commit -m "feat: enable ANSI colors in tracing output when terminal supports it"
```

---

### Task 7: Colorize `HumanRenderer` log prefixes (--logs mode)

**Context:** When `--logs` is passed, `HumanRenderer` is used. Currently prints `[key] line` with no color. Adding colored `[key]` prefixes (using the `logmux.rs` palette pattern) makes `--logs` mode also look polished. This is a low-effort, high-polish finishing touch.

**Files:**
- Modify: `crates/hm/src/output/human.rs` — add color field, color `[key]` prefix
- Modify: `crates/hm/src/commands/run/local.rs:101-103` — pass color flag

**Step 1: Write the failing test**

In `crates/hm/src/output/human.rs` test module:

```rust
#[test]
fn colored_output_wraps_key_in_ansi() {
    let mut r = HumanRenderer::new(Vec::new(), true);
    let step_id = Uuid::new_v4();

    r.on_event(&BuildEvent::StepQueued {
        step_id,
        key: "build".into(),
        chain_idx: 0,
        parent_key: None,
        display_name: "build".into(),
    });
    r.on_event(&BuildEvent::StepLog {
        step_id,
        stream: StdStream::Stdout,
        line: "hello".into(),
        ts: chrono::Utc::now(),
    });

    let s = output(&r);
    assert!(s.contains("\x1b["), "expected ANSI codes: {s}");
    assert!(s.contains("hello"), "expected log line: {s}");
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p harmont-cli --lib output::human::tests::colored_output_wraps_key_in_ansi`
Expected: FAIL — `HumanRenderer::new` takes 1 arg.

**Step 3: Implement colored HumanRenderer**

1. Add `color: bool` field:
```rust
pub struct HumanRenderer<W> {
    out: W,
    color: bool,
    step_keys: HashMap<Uuid, String>,
}
```

2. Update `new()`:
```rust
pub fn new(out: W, color: bool) -> Self {
    Self { out, color, step_keys: HashMap::new() }
}
```

3. Add the same color palette function from `logmux.rs`:
```rust
fn key_color(key: &str) -> &'static str {
    const PALETTE: [&str; 6] = [
        "\x1b[36m", "\x1b[35m", "\x1b[33m",
        "\x1b[32m", "\x1b[34m", "\x1b[91m",
    ];
    let mut h: u32 = 0;
    for b in key.bytes() {
        h = h.wrapping_mul(31).wrapping_add(u32::from(b));
    }
    PALETTE[(h as usize) % PALETTE.len()]
}
```

4. Update the `StepLog` formatting:
```rust
BuildEvent::StepLog { step_id, line, .. } => {
    let key = self.step_key(step_id);
    if self.color {
        format!("{color}[{key}]\x1b[0m {line}\n", color = key_color(key))
    } else {
        format!("[{key}] {line}\n")
    }.into_bytes()
}
```

Apply similar coloring to `StepStart`, `StepEnd`, `StepCacheHit`, `BuildEnd`, `ChainFailed`.

5. Update `local.rs`:
```rust
"human" if args.logs => {
    Box::new(crate::output::human::HumanRenderer::new(
        std::io::stderr(),
        _ctx.output.color_enabled(),
    ))
}
```

Again, rename `_ctx` → `ctx`.

6. Update test helper:
```rust
fn renderer() -> HumanRenderer<Vec<u8>> {
    HumanRenderer::new(Vec::new(), false)
}
```

**Step 4: Run tests**

Run: `cargo test -p harmont-cli --lib output::human`
Expected: All pass.

**Step 5: Manual test**

Run: `cargo build -p harmont-cli && ./target/debug/hm run --logs`
Verify: `[key]` prefixes are colored.

**Step 6: Commit**

```bash
git add crates/hm/src/output/human.rs crates/hm/src/commands/run/local.rs
git commit -m "feat: colorize HumanRenderer log prefixes in --logs mode"
```
