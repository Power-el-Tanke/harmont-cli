# Remove Output Formatter Plugin Support

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Delete the output formatter plugin capability entirely. Move human and JSON formatting into the core `hm` binary so build output doesn't cross an FFI boundary.

**Architecture:** Replace the plugin-mediated `output_subscriber → LoadedPlugin → on_output_event` path with a direct `output_subscriber → match on OutputMode { Human, Json }` that calls formatting functions in `crates/hm/src/output/`. The `render.rs` module from the human plugin moves into the binary. The JSON formatter is trivial (serde_json + newline to stdout). The `OutputFormatter` trait, `OutputFormatterSpec`, and the `output` keyword in `hm_plugin!` are all deleted.

**Tech Stack:** Rust, tokio broadcast channel, serde_json

---

## Task 1: Move formatting logic into the binary

**Files:**
- Create: `crates/hm/src/output/build_events.rs`
- Modify: `crates/hm/src/output/mod.rs`

### Step 1: Create `build_events.rs`

Move the rendering logic from `crates/hm/plugins/hm-plugin-output-human/src/render.rs` into `crates/hm/src/output/build_events.rs`. This is the step-key tracking + event → bytes logic.

Also add a JSON rendering function. The full module should contain:

```rust
use std::collections::HashMap;
use hm_plugin_protocol::BuildEvent;
use uuid::Uuid;

/// Tracks step_id → key mappings accumulated from StepQueued events.
pub(crate) struct BuildEventRenderer {
    step_keys: HashMap<Uuid, String>,
}

impl BuildEventRenderer {
    pub fn new() -> Self {
        Self { step_keys: HashMap::new() }
    }

    /// Render a BuildEvent as human-readable stderr bytes.
    pub fn render_human(&mut self, ev: &BuildEvent) -> Vec<u8> {
        // Port the match arms from render.rs, using self.step_keys
        // instead of a static Mutex.
    }

    /// Render a BuildEvent as a JSON line (stdout).
    pub fn render_json(&self, ev: &BuildEvent) -> Vec<u8> {
        let mut bytes = serde_json::to_vec(ev).unwrap_or_default();
        bytes.push(b'\n');
        bytes
    }
}
```

Key change from the plugin version: use `&mut self` with a `HashMap` field instead of a `static Mutex<HashMap>`. The renderer is owned by the output subscriber task, so no shared state needed.

Port the tests from `render.rs` too (`build_start_renders_step_and_chain_counts`, `step_log_renders_with_prefix_after_step_queued_recorded_key`, `step_log_with_unknown_key_renders_question_mark`).

### Step 2: Re-export from `output/mod.rs`

Add `pub mod build_events;` to `crates/hm/src/output/mod.rs`. Add `OutputMode` variant awareness — the existing `OutputMode::Human` and `OutputMode::Json` already exist and will drive the formatting choice.

### Step 3: Verify

Run: `cargo test -p harmont-cli -- output::build_events`

### Step 4: Commit

```bash
git add crates/hm/src/output/
git commit -m "feat(output): move build event formatting into core binary"
```

---

## Task 2: Rewrite output_subscriber to use direct formatting

**Files:**
- Rewrite: `crates/hm/src/orchestrator/output_subscriber.rs`
- Modify: `crates/hm/src/orchestrator/scheduler.rs`

### Step 1: Rewrite output_subscriber.rs

Replace the plugin-dispatch loop with direct formatting. The subscriber no longer needs the plugin registry — it owns a `BuildEventRenderer` and writes to stdout/stderr directly.

New signature:
```rust
pub fn spawn(
    bus: Arc<EventBus>,
    format: OutputMode,  // was: registry + format_name string
) -> tokio::task::JoinHandle<Result<()>>
```

Inside the loop:
- Create a `BuildEventRenderer` before the loop
- On each event, call `renderer.render_human(&event)` or `renderer.render_json(&event)` based on `format`
- Human output → write to stderr (matching current plugin behavior)
- JSON output → write to stdout
- On `BuildEnd`, just return (no finalize step needed — both formatters stream)
- Keep the `Lagged` error handling as-is

Use `std::io::Write` (locked stdout/stderr) for the actual writes — no async needed since formatting is CPU-bound and the writes are small.

### Step 2: Update scheduler.rs

Remove the format validation block (lines 126-141 in scheduler.rs) — the `--format` flag is now a compile-time enum, not a runtime string lookup.

Change the `output_subscriber::spawn` call:
```rust
// Before:
let sink_handle = super::output_subscriber::spawn(bus.clone(), registry.clone(), format_name.clone());

// After:
let format = if format_name == "json" { OutputMode::Json } else {
    OutputMode::Human { color: true, interactive: true }
};
let sink_handle = super::output_subscriber::spawn(bus.clone(), format);
```

Remove the `format_name` parameter from `pub async fn run(...)`. Instead, pass `OutputMode` directly. Update the call site in `crates/hm/src/commands/run/local.rs` to convert the CLI string into `OutputMode` before calling the orchestrator.

### Step 3: Verify

Run: `cargo check -p harmont-cli`

### Step 4: Commit

```bash
git add crates/hm/src/orchestrator/ crates/hm/src/commands/run/
git commit -m "refactor(orchestrator): output subscriber uses direct formatting, not plugins"
```

---

## Task 3: Remove output formatter from plugin system

**Files:**
- Modify: `crates/hm-plugin-protocol/src/manifest.rs` — remove `OutputFormatter` variant from `Capability`, delete `OutputFormatterSpec`
- Modify: `crates/hm-plugin-protocol/src/lib.rs` — remove `OutputFormatterSpec` re-export
- Modify: `crates/hm/src/plugin/registry.rs` — remove `output_formatter_index` field and indexing logic
- Modify: `crates/hm/src/plugin/host.rs` — remove `on_output_event()` and `finalize_output()` methods
- Modify: `crates/hm-plugin-sdk/src/ffi.rs` — remove `on_output_event` and `finalize_output` from `RawPlugin` trait
- Modify: `crates/hm-plugin-sdk/src/lib.rs` — remove `pub mod output` and `OutputFormatter` re-export
- Delete: `crates/hm-plugin-sdk/src/output.rs`
- Modify: `crates/hm-plugin-macros/src/lib.rs` — remove `output` keyword parsing and `gen_on_output_event`/`gen_finalize_output` codegen
- Delete: `crates/hm/src/orchestrator/output_subscriber.rs` (replaced in Task 2, but the module decl stays if we renamed it — actually, we rewrote it in Task 2, so it stays; but remove the `pub mod output_subscriber` if it was renamed)

### Step 1: Remove from protocol crate

In `manifest.rs`: delete `OutputFormatter(OutputFormatterSpec)` from `Capability` enum, delete the `OutputFormatterSpec` struct. In `lib.rs`: remove `OutputFormatterSpec` from the re-exports.

### Step 2: Remove from SDK

Delete `crates/hm-plugin-sdk/src/output.rs`. In `lib.rs`: remove `pub mod output` and the `pub use output::OutputFormatter` re-export.

In `ffi.rs`: remove the two trait methods from `RawPlugin`:
```rust
// DELETE:
extern "C" fn on_output_event<'a>(&'a self, event: FfiSlice<'a>) -> DynFutureUnsync<'a, FfiResult>;
extern "C" fn finalize_output<'a>(&'a self) -> DynFutureUnsync<'a, FfiResult>;
```

### Step 3: Remove from proc macro

In `crates/hm-plugin-macros/src/lib.rs`:
- Remove `output` from the keyword parser (~line 60)
- Remove `output: Option<Path>` from `HmPluginArgs` (~line 81)
- Remove the `output` match arm in parsing (~lines 132-138)
- Remove `gen_on_output_event()` function (~line 360+)
- Remove the corresponding `gen_finalize_output()` function
- Remove the `output_field` and `output_init` generation in `expand()`
- Remove the calls to `gen_on_output_event` and `gen_finalize_output` in the trait impl generation

### Step 4: Remove from host binary

In `crates/hm/src/plugin/host.rs`: remove `on_output_event()` and `finalize_output()` async methods from `LoadedPlugin`.

In `crates/hm/src/plugin/registry.rs`: remove `output_formatter_index` field, its initialization in `new()`, and the `Capability::OutputFormatter` arm in `index_capabilities()`.

### Step 5: Fix compilation cascade

Run `cargo check --workspace` and fix any remaining references. Expected breakage in test fixtures and protocol tests that reference `OutputFormatter` or `OutputFormatterSpec`.

### Step 6: Commit

```bash
git add -A
git commit -m "refactor: remove OutputFormatter capability from plugin system"
```

---

## Task 4: Delete output plugin crates

**Files:**
- Delete: `crates/hm/plugins/hm-plugin-output-human/` (entire directory)
- Delete: `crates/hm/plugins/hm-plugin-output-json/` (entire directory)
- Modify: `Cargo.toml` (workspace root) — remove from `members`

### Step 1: Delete crate directories

```bash
rm -rf crates/hm/plugins/hm-plugin-output-human
rm -rf crates/hm/plugins/hm-plugin-output-json
```

### Step 2: Remove from workspace

In root `Cargo.toml`, remove from `members`:
```toml
"crates/hm/plugins/hm-plugin-output-human",
"crates/hm/plugins/hm-plugin-output-json",
```

### Step 3: Update docs

In `crates/hm/CLAUDE.md`: remove references to output plugins. Update `RELEASING.md` if it mentions them.

### Step 4: Verify

```bash
cargo check --workspace
cargo test --workspace
```

### Step 5: Commit

```bash
git add -A
git commit -m "chore: delete output formatter plugin crates"
```

---

## Task 5: Clean up --format flag

**Files:**
- Modify: `crates/hm/src/cli/run.rs`
- Modify: `crates/hm/src/commands/run/local.rs`

### Step 1: Change --format to an enum

In `cli/run.rs`, change the `format` field from `String` to a proper enum with `clap::ValueEnum`:

```rust
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum OutputFormat {
    Human,
    Json,
}

// In RunArgs:
#[arg(long, value_name = "NAME", default_value = "human")]
pub format: OutputFormat,
```

### Step 2: Convert at call site

In `commands/run/local.rs`, convert `OutputFormat` to `OutputMode` when calling the orchestrator:
```rust
let output_mode = match args.format {
    OutputFormat::Human => OutputMode::Human { color: color_enabled, interactive: is_tty },
    OutputFormat::Json => OutputMode::Json,
};
```

### Step 3: Verify

```bash
cargo check -p harmont-cli
cargo test -p harmont-cli
```

### Step 4: Commit

```bash
git add crates/hm/src/cli/ crates/hm/src/commands/run/
git commit -m "refactor(cli): --format flag uses enum instead of string"
```
