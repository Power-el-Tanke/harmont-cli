# Tracing Migration: Replace All println!/eprintln! with tracing

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Eliminate every `println!`, `eprintln!`, `print!`, and `eprint!` macro from the codebase, routing all output through the `tracing` crate with a custom subscriber layer that preserves the CLI's current user-facing formatting.

**Architecture:** A custom `CliLayer` tracing subscriber handles user-facing output by routing events with target `"user::stdout"` to stdout and `"user::stderr"` to stderr, with no tracing metadata decoration. A standard `tracing_subscriber::fmt` layer handles diagnostic output (visible only with `--verbose`). The subscriber is always initialized, not gated on `--verbose`. Convenience macros `ui_println!`/`ui_eprintln!` wrap the target boilerplate for crate-internal use; `hm-plugin-cloud` uses raw `tracing::info!(target: ...)` since the macros live in the downstream `hm` crate.

**Tech Stack:** `tracing 0.1`, `tracing-subscriber 0.3` (env-filter, fmt, registry features)

---

## Current State Summary

- **93 total print macro occurrences** across `hm` (67) and `hm-plugin-cloud` (26)
- tracing + tracing-subscriber already in `hm`; tracing already in `hm-plugin-cloud`
- 3 existing tracing macro calls (info, warn, error)
- Subscriber only initialised when `--verbose` flag is set
- `output/status.rs` centralises success/warning/error/info prints
- `output/format.rs` centralises visual formatting (banners, headers, kv, steps)
- `HumanRenderer` and `JsonRenderer` already use `Write` trait — no print macros
- Workspace lints already have `print_stdout = "warn"`, `print_stderr = "warn"`

## Target Convention

| Target | Stream | When to use |
|--------|--------|-------------|
| `"user::stdout"` | stdout | Data output: tables, JSON, values, version, ports, visual formatting |
| `"user::stderr"` | stderr | Status/progress messages, confirmations, errors, warnings |
| Module path (default) | stderr (via fmt layer) | Diagnostic/debug output, only visible with `--verbose` |

The `CliLayer` filters on `target.starts_with("user")`. The `FmtLayer` filters on `!target.starts_with("user")`.

---

## Task 1: Workspace Dependency Consolidation

**Files:**
- Modify: `Cargo.toml` (workspace root, `[workspace.dependencies]` section)
- Modify: `crates/hm/Cargo.toml:46-47`
- Modify: `crates/hm-plugin-cloud/Cargo.toml:29`

**Step 1: Add tracing to workspace.dependencies**

In `Cargo.toml` (workspace root), add to `[workspace.dependencies]`:

```toml
tracing     = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "fmt", "registry"] }
```

Note: add `"registry"` feature — needed for layered subscriber composition.

**Step 2: Update hm/Cargo.toml to use workspace versions**

Replace lines 46-47:
```toml
tracing = { workspace = true }
tracing-subscriber = { workspace = true }
```

**Step 3: Update hm-plugin-cloud/Cargo.toml to use workspace version**

Replace line 29:
```toml
tracing = { workspace = true }
```

**Step 4: Verify build**

Run: `cargo check --workspace`
Expected: compiles clean (warnings OK at this stage)

**Step 5: Commit**

```bash
git add Cargo.toml crates/hm/Cargo.toml crates/hm-plugin-cloud/Cargo.toml
git commit -m "refactor: move tracing deps to workspace.dependencies"
```

---

## Task 2: CliLayer Implementation

**Files:**
- Create: `crates/hm/src/output/cli_layer.rs`
- Modify: `crates/hm/src/output/mod.rs`

**Step 1: Write the failing test**

Create `crates/hm/src/output/cli_layer.rs` with the test first:

```rust
use std::fmt::Write as FmtWrite;
use std::io::Write;
use std::sync::{Arc, Mutex};

use tracing::Subscriber;
use tracing_subscriber::layer::Context;
use tracing_subscriber::Layer;

/// A tracing layer that routes user-facing output to stdout/stderr
/// based on event target.
///
/// Events with target `"user::stdout"` are written to `stdout_sink`.
/// Events with target `"user::stderr"` are written to `stderr_sink`.
/// All other events are ignored (handled by other layers).
#[derive(Debug, Clone)]
pub struct CliLayer<O, E> {
    stdout_sink: Arc<Mutex<O>>,
    stderr_sink: Arc<Mutex<E>>,
}

impl CliLayer<std::io::Stdout, std::io::Stderr> {
    /// Create a layer that writes to the real stdout/stderr.
    pub fn real() -> Self {
        Self {
            stdout_sink: Arc::new(Mutex::new(std::io::stdout())),
            stderr_sink: Arc::new(Mutex::new(std::io::stderr())),
        }
    }
}

impl<O, E> CliLayer<O, E> {
    /// Create a layer with custom sinks (for testing).
    pub fn with_sinks(stdout: O, stderr: E) -> Self {
        Self {
            stdout_sink: Arc::new(Mutex::new(stdout)),
            stderr_sink: Arc::new(Mutex::new(stderr)),
        }
    }
}

/// Visitor that extracts the formatted message from a tracing event.
#[derive(Default)]
struct MessageVisitor(String);

impl tracing::field::Visit for MessageVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            write!(self.0, "{value:?}").ok();
        }
    }
}

impl<S, O, E> Layer<S> for CliLayer<O, E>
where
    S: Subscriber,
    O: Write + Send + 'static,
    E: Write + Send + 'static,
{
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        let target = event.metadata().target();

        let is_stdout = target == "user::stdout";
        let is_stderr = target == "user::stderr";
        if !is_stdout && !is_stderr {
            return;
        }

        let mut visitor = MessageVisitor::default();
        event.record(&mut visitor);
        let msg = visitor.0;

        if is_stdout {
            if let Ok(mut w) = self.stdout_sink.lock() {
                writeln!(w, "{msg}").ok();
            }
        } else {
            if let Ok(mut w) = self.stderr_sink.lock() {
                writeln!(w, "{msg}").ok();
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;

    fn capture_layer() -> (CliLayer<Vec<u8>, Vec<u8>>, Arc<Mutex<Vec<u8>>>, Arc<Mutex<Vec<u8>>>) {
        let stdout_buf: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
        let stderr_buf: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));

        let layer = CliLayer {
            stdout_sink: Arc::clone(&stdout_buf),
            stderr_sink: Arc::clone(&stderr_buf),
        };

        (layer, stdout_buf, stderr_buf)
    }

    fn stdout_str(buf: &Arc<Mutex<Vec<u8>>>) -> String {
        String::from_utf8(buf.lock().unwrap().clone()).unwrap()
    }

    #[test]
    fn routes_stdout_target_to_stdout_sink() {
        let (layer, stdout_buf, stderr_buf) = capture_layer();

        let subscriber = tracing_subscriber::registry().with(layer);
        let _guard = tracing::subscriber::set_default(subscriber);

        tracing::info!(target: "user::stdout", "hello world");

        assert_eq!(stdout_str(&stdout_buf), "hello world\n");
        assert!(stdout_str(&stderr_buf).is_empty());
    }

    #[test]
    fn routes_stderr_target_to_stderr_sink() {
        let (layer, stdout_buf, stderr_buf) = capture_layer();

        let subscriber = tracing_subscriber::registry().with(layer);
        let _guard = tracing::subscriber::set_default(subscriber);

        tracing::info!(target: "user::stderr", "warning msg");

        assert!(stdout_str(&stdout_buf).is_empty());
        assert_eq!(stdout_str(&stderr_buf), "warning msg\n");
    }

    #[test]
    fn ignores_other_targets() {
        let (layer, stdout_buf, stderr_buf) = capture_layer();

        let subscriber = tracing_subscriber::registry().with(layer);
        let _guard = tracing::subscriber::set_default(subscriber);

        tracing::info!("diagnostic message");

        assert!(stdout_str(&stdout_buf).is_empty());
        assert!(stdout_str(&stderr_buf).is_empty());
    }

    #[test]
    fn handles_format_args() {
        let (layer, stdout_buf, _) = capture_layer();

        let subscriber = tracing_subscriber::registry().with(layer);
        let _guard = tracing::subscriber::set_default(subscriber);

        let name = "alice";
        let count = 42;
        tracing::info!(target: "user::stdout", "{name} has {count} items");

        assert_eq!(stdout_str(&stdout_buf), "alice has 42 items\n");
    }

    #[test]
    fn handles_empty_message() {
        let (layer, stdout_buf, _) = capture_layer();

        let subscriber = tracing_subscriber::registry().with(layer);
        let _guard = tracing::subscriber::set_default(subscriber);

        tracing::info!(target: "user::stdout", "");

        assert_eq!(stdout_str(&stdout_buf), "\n");
    }
}
```

**Step 2: Register the module**

In `crates/hm/src/output/mod.rs`, add:
```rust
pub mod cli_layer;
```

**Step 3: Run tests to verify they compile and pass**

Run: `cargo test -p harmont-cli -- output::cli_layer --nocapture`
Expected: all 5 tests pass

**Step 4: Commit**

```bash
git add crates/hm/src/output/cli_layer.rs crates/hm/src/output/mod.rs
git commit -m "feat: add CliLayer tracing subscriber for user-facing output"
```

---

## Task 3: Subscriber Initialization + Convenience Macros

**Files:**
- Modify: `crates/hm/src/main.rs`
- Modify: `crates/hm/src/output/mod.rs`

**Step 1: Add convenience macros to output/mod.rs**

Add to the top of `crates/hm/src/output/mod.rs` (before module declarations):

```rust
/// Write a line to stdout via tracing. Equivalent to `println!`.
macro_rules! ui_println {
    () => { ::tracing::info!(target: "user::stdout", "") };
    ($($arg:tt)*) => { ::tracing::info!(target: "user::stdout", $($arg)*) };
}

/// Write a line to stderr via tracing. Equivalent to `eprintln!`.
macro_rules! ui_eprintln {
    ($($arg:tt)*) => { ::tracing::info!(target: "user::stderr", $($arg)*) };
}

pub(crate) use ui_eprintln;
pub(crate) use ui_println;
```

**Step 2: Rewrite subscriber init in main.rs**

Replace the entire `main.rs` with:

```rust
use clap::Parser;
use owo_colors::OwoColorize;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

use harmont_cli::cli::{self, Cli};
use harmont_cli::context::RunContext;
use harmont_cli::error::{self, HmError};
use harmont_cli::output::cli_layer::CliLayer;
use harmont_cli::output::status;

#[tokio::main]
async fn main() {
    let args = Cli::parse();

    // Build the layered subscriber:
    // 1. CliLayer — always active, routes user::stdout / user::stderr
    // 2. fmt layer — only active with --verbose, for diagnostic tracing
    let cli_layer = CliLayer::real();

    let fmt_layer = if args.verbose {
        let filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("debug"));
        Some(
            tracing_subscriber::fmt::layer()
                .with_target(false)
                .with_filter(filter),
        )
    } else {
        None
    };

    tracing_subscriber::registry()
        .with(cli_layer)
        .with(fmt_layer)
        .init();

    let color_enabled = !args.no_color
        && std::env::var_os("NO_COLOR").is_none()
        && console::Term::stderr().is_term();
    owo_colors::set_override(color_enabled);

    let code = match run(args).await {
        Ok(code) => code,
        Err(e) => handle_error(&e),
    };

    std::process::exit(code);
}

async fn run(args: Cli) -> Result<i32, anyhow::Error> {
    let command = args.command.clone();
    let ctx = RunContext::from_cli(&args)?;
    cli::dispatch(command, ctx).await
}

fn handle_error(err: &anyhow::Error) -> i32 {
    if let Some(hm_err) = err.downcast_ref::<HmError>() {
        status::print_error(&format!("{hm_err}"));
        return hm_err.exit_code();
    }

    let msg = format!("{err:#}");
    tracing::info!(target: "user::stderr", "{} {msg}", "error:".red().bold());
    error::EXIT_BUILD_FAILED
}
```

Note: the `#[allow(clippy::print_stderr)]` attribute at the top of main.rs is now removed. The `handle_error` function uses tracing instead of `eprintln!`.

**Step 3: Add `filter` import**

The `fmt_layer` uses `.with_filter()` which requires `tracing_subscriber::layer::Filter` via the `registry` feature. Verify `Cargo.toml` has `features = ["env-filter", "fmt", "registry"]` (added in Task 1).

**Step 4: Verify build**

Run: `cargo check -p harmont-cli`
Expected: compiles clean (warnings from remaining println!/eprintln! in other files expected)

**Step 5: Commit**

```bash
git add crates/hm/src/main.rs crates/hm/src/output/mod.rs
git commit -m "feat: always-on CliLayer subscriber + ui_println/ui_eprintln macros"
```

---

## Task 4: Convert output/status.rs

**Files:**
- Modify: `crates/hm/src/output/status.rs`

**Step 1: Rewrite status.rs**

Replace the entire file:

```rust
use owo_colors::OwoColorize;

/// Print a success message to stdout.
pub fn print_success(msg: &str) {
    let check = format!("{}", "\u{2714}".green().bold());
    tracing::info!(target: "user::stdout", "{check} {msg}");
}

/// Print a warning message to stderr.
pub fn print_warning(msg: &str) {
    let bang = format!("{}", "!".yellow().bold());
    tracing::info!(target: "user::stderr", "{bang} {msg}");
}

/// Print an error message to stderr.
pub fn print_error(msg: &str) {
    let cross = format!("{}", "\u{2718}".red().bold());
    tracing::info!(target: "user::stderr", "{cross} {msg}");
}

/// Print an info message to stdout.
pub fn print_info(msg: &str) {
    let arrow = format!("{}", "\u{25b6}".cyan());
    tracing::info!(target: "user::stdout", "{arrow} {msg}");
}
```

The `#[allow(clippy::print_stdout, clippy::print_stderr)]` at the top is removed — no longer needed.

**Step 2: Run tests**

Run: `cargo test -p harmont-cli`
Expected: all tests pass

**Step 3: Commit**

```bash
git add crates/hm/src/output/status.rs
git commit -m "refactor: route status output through tracing CliLayer"
```

---

## Task 5: Convert output/format.rs

**Files:**
- Modify: `crates/hm/src/output/format.rs`

**Step 1: Replace all println! calls with tracing**

Remove the `#[allow(clippy::print_stdout)]` block at the top of the file.

Replace each function's print calls. The functions to change:

**`header()`** (lines 158-164):
```rust
pub fn header(title: &str) {
    let rule_len = title.chars().count() + 4;
    let rule: String = "─".repeat(rule_len);
    tracing::info!(target: "user::stdout", "");
    tracing::info!(target: "user::stdout", "  {}", title.bold());
    tracing::info!(target: "user::stdout", "  {}", rule.bright_black());
}
```

**`kv()`** (lines 167-172):
```rust
pub fn kv(label: &str, value: impl Display) {
    let label_with_colon = format!("{label}:");
    let padded = format!("{label_with_colon:<10}");
    tracing::info!(target: "user::stdout", "  {} {value}", padded.bright_black());
}
```

**`empty_state()`** (lines 176-180):
```rust
pub fn empty_state(title: &str, hint: &str) {
    tracing::info!(target: "user::stdout", "");
    tracing::info!(target: "user::stdout", "  {}", title.bold());
    tracing::info!(target: "user::stdout", "  {}", hint.bright_black());
    tracing::info!(target: "user::stdout", "");
}
```

**`banner()`** (lines 184-192):
```rust
pub fn banner(command: &str, subtitle: &str) {
    tracing::info!(
        target: "user::stdout",
        "{} {} {} {}",
        "▌".cyan().bold(),
        "hm".bold(),
        command.cyan(),
        format!("· {subtitle}").bright_black()
    );
    tracing::info!(target: "user::stdout", "");
}
```

**`step()`** (lines 196-202):
```rust
pub fn step(verb: &str, result: impl Display) {
    tracing::info!(
        target: "user::stdout",
        "  {} {} {}",
        "✓".green().bold(),
        verb.bright_black(),
        result
    );
}
```

**Step 2: Run tests**

Run: `cargo test -p harmont-cli -- output::format`
Expected: all existing format tests pass (they test pure functions like `rel_time`, not the print functions)

**Step 3: Commit**

```bash
git add crates/hm/src/output/format.rs
git commit -m "refactor: route format output through tracing CliLayer"
```

---

## Task 6: Convert hm Commands (dev/*)

**Files:**
- Modify: `crates/hm/src/commands/dev/up.rs`
- Modify: `crates/hm/src/commands/dev/down.rs`
- Modify: `crates/hm/src/commands/dev/exec.rs`
- Modify: `crates/hm/src/commands/dev/logs.rs`
- Modify: `crates/hm/src/commands/dev/port_of.rs`
- Modify: `crates/hm/src/commands/dev/ls.rs`

**Step 1: Convert up.rs**

Replace every `eprintln!` with `tracing::info!(target: "user::stderr", ...)`. The messages are progress/status for the user.

| Line | Old | New |
|------|-----|-----|
| 51 | `eprintln!("[hm] session {session_id}...")` | `tracing::info!(target: "user::stderr", "[hm] session {session_id}...")` |
| 59 | `eprintln!("[hm] network {}: created", net.name)` | `tracing::info!(target: "user::stderr", "[hm] network {}: created", net.name)` |
| 100 | `eprintln!("[hm] all up...")` | `tracing::info!(target: "user::stderr", "[hm] all up...")` |
| 105 | `eprintln!("[hm] tearing down...")` | `tracing::info!(target: "user::stderr", "[hm] tearing down...")` |
| 149 | `eprintln!("[{slug}] ready ...")` | `tracing::info!(target: "user::stderr", "[{slug}] ready ...")` |
| 171 | `eprintln!("[{slug}] pulling {tag}...")` | `tracing::info!(target: "user::stderr", "[{slug}] pulling {tag}...")` |
| 184 | `eprintln!("[{slug}] building from Step chain...")` | `tracing::info!(target: "user::stderr", "[{slug}] building from Step chain...")` |
| 251 | `eprintln!("[{}] stopped", b.slug)` | `tracing::info!(target: "user::stderr", "[{}] stopped", b.slug)` |
| 254 | `eprintln!("[hm] network {}: removed", net.name)` | `tracing::info!(target: "user::stderr", "[hm] network {}: removed", net.name)` |

**Step 2: Convert down.rs**

| Line | Old | New |
|------|-----|-----|
| 56 | `eprintln!("[hm] nothing to sweep")` | `tracing::info!(target: "user::stderr", "[hm] nothing to sweep")` |
| 64 | `eprintln!("[hm] removed {name}...")` | `tracing::info!(target: "user::stderr", "[hm] removed {name}...")` |

**Step 3: Convert exec.rs**

| Line | Old | New |
|------|-----|-----|
| 45-48 | `eprintln!("hm: slug...")` | `tracing::info!(target: "user::stderr", "hm: slug...")` |
| 52-55 | `eprintln!("hm: slug...")` | `tracing::info!(target: "user::stderr", "hm: slug...")` |

**Step 4: Convert logs.rs**

Same pattern as exec.rs — lines 48-51 and 55-58.

**Step 5: Convert port_of.rs**

All `eprintln!` → `tracing::info!(target: "user::stderr", ...)`.

The one `println!("{host_port}")` at line 83 → `tracing::info!(target: "user::stdout", "{host_port}")`.

**Step 6: Convert ls.rs**

All `println!` → `tracing::info!(target: "user::stdout", ...)` (these are table output).

**Step 7: Verify build**

Run: `cargo check -p harmont-cli`
Expected: compiles clean

**Step 8: Run tests**

Run: `cargo test -p harmont-cli`
Expected: all tests pass

**Step 9: Commit**

```bash
git add crates/hm/src/commands/dev/
git commit -m "refactor: replace print macros with tracing in dev commands"
```

---

## Task 7: Convert hm CLI Helpers + Orchestrator

**Files:**
- Modify: `crates/hm/src/cli/version.rs`
- Modify: `crates/hm/src/cli/plugin.rs`
- Modify: `crates/hm/src/orchestrator/output_subscriber.rs`
- Modify: `crates/hm/src/orchestrator/signal.rs`

**Step 1: Convert version.rs**

Line 10: `println!("hm {}", env!("CARGO_PKG_VERSION"))` → `tracing::info!(target: "user::stdout", "hm {}", env!("CARGO_PKG_VERSION"))`

**Step 2: Convert plugin.rs**

Line 23: `println!("Registered runners:")` → `tracing::info!(target: "user::stdout", "Registered runners:")`
Line 24: `println!("  docker (default, built-in)")` → `tracing::info!(target: "user::stdout", "  docker (default, built-in)")`

**Step 3: Convert output_subscriber.rs**

Line 45: `eprintln!("[output] dropped {n} build events")` — this is a diagnostic message, not user-facing. Use standard tracing:

`tracing::warn!("output: dropped {n} build events")`

(This replaces the existing `tracing::warn!` on a nearby line if it's the same message, or supplements it. Check the actual code — there may already be a `tracing::warn!` for this; if so, just remove the `eprintln!`.)

**Step 4: Convert signal.rs**

Line 34: `eprintln!("\nforce-exit on second Ctrl-C")` → `tracing::info!(target: "user::stderr", "\nforce-exit on second Ctrl-C")`
Line 37: `eprintln!("\ncancelling… (Ctrl-C again to force)")` → `tracing::info!(target: "user::stderr", "\ncancelling… (Ctrl-C again to force)")`

**Step 5: Verify build + tests**

Run: `cargo check -p harmont-cli && cargo test -p harmont-cli`
Expected: compiles and all tests pass

**Step 6: Commit**

```bash
git add crates/hm/src/cli/ crates/hm/src/orchestrator/
git commit -m "refactor: replace print macros in cli helpers and orchestrator"
```

---

## Task 8: Convert hm-plugin-cloud

**Files:**
- Modify: `crates/hm-plugin-cloud/src/cli.rs`
- Modify: `crates/hm-plugin-cloud/src/auth/login.rs`
- Modify: `crates/hm-plugin-cloud/src/auth/logout.rs`
- Modify: `crates/hm-plugin-cloud/src/auth/whoami.rs`
- Modify: `crates/hm-plugin-cloud/src/verbs/job.rs`
- Modify: `crates/hm-plugin-cloud/src/verbs/billing.rs`
- Modify: `crates/hm-plugin-cloud/src/verbs/run.rs`
- Modify: `crates/hm-plugin-cloud/src/verbs/org.rs`
- Modify: `crates/hm-plugin-cloud/src/verbs/build.rs`
- Modify: `crates/hm-plugin-cloud/src/verbs/pipeline.rs`

**Step 1: Convert cli.rs**

Line 164 — `print!("{msg}")` for clap help/version: This is a `print!` (no newline). Clap's help text already contains a trailing newline. Handle with:

```rust
// Clap's help/version text already includes a trailing newline;
// write it directly to avoid the extra newline from tracing.
#[allow(clippy::print_stdout)]
{
    use std::io::Write;
    std::io::stdout().write_all(msg.as_bytes()).ok();
}
```

Line 168 — `eprint!("{msg}")` for clap errors: Same approach:
```rust
#[allow(clippy::print_stderr)]
{
    use std::io::Write;
    std::io::stderr().write_all(msg.as_bytes()).ok();
}
```

Line 196 — `eprintln!("{e:#}")` → `tracing::info!(target: "user::stderr", "{e:#}")`

**Step 2: Convert auth/login.rs**

Line 42: Keep existing `tracing::info!("opening browser to {auth_url}")` (diagnostic, already correct).

Lines 44-46: `eprintln!("couldn't auto-open...")` → `tracing::info!(target: "user::stderr", "couldn't auto-open the browser. Open this URL manually:\n  {auth_url}")`

Line 103: `eprintln!("Open this URL...")` → `tracing::info!(target: "user::stderr", "Open this URL in your browser, then paste the code:\n  {auth_url}")`

Lines 137-141: `eprintln!("logged in as...")` → `tracing::info!(target: "user::stderr", "logged in as {} ({})", ..., ...)`

**Step 3: Convert auth/logout.rs**

Line 13: `eprintln!("logged out of {}", cfg.api_base)` → `tracing::info!(target: "user::stderr", "logged out of {}", cfg.api_base)`

**Step 4: Convert auth/whoami.rs**

Lines 22-27: `println!(...)` → `tracing::info!(target: "user::stdout", "{} <{}> (id {})", ..., ..., ...)`

**Step 5: Convert verbs/job.rs**

Lines 43-48: `println!(...)` → `tracing::info!(target: "user::stdout", ...)`
Lines 65-68: `println!(...)` → `tracing::info!(target: "user::stdout", ...)`
Line 85: `println!("{}", chunk.line)` → `tracing::info!(target: "user::stdout", "{}", chunk.line)`

**Step 6: Convert verbs/billing.rs**

All `println!` → `tracing::info!(target: "user::stdout", ...)`
All `eprintln!` → `tracing::info!(target: "user::stderr", ...)`

Key lines: 43, 54-60, 86-92, 107, 109, 110, 126.

**Step 7: Convert verbs/run.rs**

Line 78: `eprintln!(...)` → `tracing::info!(target: "user::stderr", ...)`

**Step 8: Convert verbs/org.rs**

Line 33: `eprintln!(...)` → `tracing::info!(target: "user::stderr", ...)`

**Step 9: Convert verbs/build.rs**

Lines 38-43: `println!(...)` → `tracing::info!(target: "user::stdout", ...)`
Line 55: `println!(...)` → `tracing::info!(target: "user::stdout", ...)`
Line 66: `eprintln!(...)` → `tracing::info!(target: "user::stderr", ...)`
Line 79: `eprintln!(...)` → `tracing::info!(target: "user::stderr", ...)`

**Step 10: Convert verbs/pipeline.rs**

Lines 30-34: `println!(...)` → `tracing::info!(target: "user::stdout", ...)`
Line 50: `println!(...)` → `tracing::info!(target: "user::stdout", ...)`

**Step 11: Remove any `#[allow(clippy::print_stdout)]` or `#[allow(clippy::print_stderr)]` attributes from all modified files**

Check each file for these attributes and remove them (except the scoped `#[allow]` in cli.rs for the clap edge case from Step 1).

**Step 12: Verify build + tests**

Run: `cargo check -p hm-plugin-cloud && cargo test -p hm-plugin-cloud`
Expected: compiles and all tests pass

**Step 13: Commit**

```bash
git add crates/hm-plugin-cloud/
git commit -m "refactor: replace print macros with tracing in cloud plugin"
```

---

## Task 9: Enforce Lint + Final Verification

**Files:**
- Modify: `Cargo.toml` (workspace root, `[workspace.lints.clippy]` section)
- Modify: any remaining files with `#[allow(clippy::print_stdout)]` or `#[allow(clippy::print_stderr)]`

**Step 1: Upgrade lint from warn to deny**

In `Cargo.toml`, change:
```toml
print_stdout = "deny"
print_stderr = "deny"
```

**Step 2: Run clippy to find remaining violations**

Run: `cargo clippy --workspace 2>&1 | grep -E 'print_stdout|print_stderr'`
Expected: only the two scoped `#[allow]` blocks in `hm-plugin-cloud/src/cli.rs` (for clap's raw write edge case)

If there are other violations, fix them by converting to tracing.

**Step 3: Remove stale #[allow] attributes**

Search and remove all `#[allow(clippy::print_stdout)]` and `#[allow(clippy::print_stderr)]` except:
- `crates/hm-plugin-cloud/src/cli.rs` — scoped allow for clap help/error raw writes

Also remove the module-level allows from:
- `crates/hm/src/main.rs` — both `#![allow(...)]` blocks (first one was for print_stderr; the second for multiple_crate_versions stays if still needed)
- `crates/hm/src/output/format.rs` — remove `#![allow(clippy::print_stdout)]` block
- `crates/hm/src/output/status.rs` — remove `#![allow(clippy::print_stdout, clippy::print_stderr)]` block

**Step 4: Full workspace build + test**

Run: `cargo clippy --workspace && cargo test --workspace`
Expected: zero warnings about print_stdout/print_stderr, all tests pass

**Step 5: Verify behaviour**

Run `cargo run -- --version` and confirm version prints to stdout.
Run `cargo run -- --verbose --version` and confirm diagnostic output also appears.

**Step 6: Commit**

```bash
git add Cargo.toml crates/
git commit -m "refactor: deny print_stdout/print_stderr lints, remove stale allows"
```

---

## Edge Cases & Notes

1. **Clap help/version text:** Clap's `Error::print()` writes text that already includes newlines. Using `tracing::info!` would add an extra trailing newline. The two occurrences in `hm-plugin-cloud/src/cli.rs` use raw `std::io::Write` with a scoped `#[allow]`.

2. **HumanRenderer / JsonRenderer:** These already use `Write` trait with no print macros. No changes needed.

3. **Tests:** Tests that use `assert_cmd` capture stdout/stderr at the process level — they work regardless of the output mechanism. Unit tests that need to verify output should use `CliLayer::with_sinks()` to capture into buffers.

4. **`--verbose` flag:** The `fmt` layer is only added when `--verbose` is set. Without it, only `user::*` target events produce output. With it, all tracing events (info, warn, error, debug) are also printed with tracing metadata to stderr.

5. **Signal handler output:** The Ctrl-C handler in `signal.rs` runs in a tokio spawned task (not a real signal handler), so tracing calls are safe.

6. **`multiple_crate_versions` allow in main.rs:** This `#![allow]` is for clippy::multiple_crate_versions, not print-related. **Keep it** — it addresses transitive dependency conflicts.

7. **Flushing:** The CliLayer uses `Mutex<Stdout>` / `Mutex<Stderr>`, which provides line-buffered output by default. For stdout, this means output may be buffered when piped. If this causes issues, add an explicit `flush()` call after each write in the CliLayer.
