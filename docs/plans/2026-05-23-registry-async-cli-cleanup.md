# Registry Async Load + CLI Cleanup Implementation Plan

> **For Claude:** Execute this plan task-by-task.

**Goal:** Make plugin registry loading async/parallel, move spec collection onto `PluginRegistry`, make CLI augmentation a `Cli` associated function, and eliminate the fragile `BUILTIN_SUBCOMMANDS` array.

**Architecture:** `PluginRegistry::load` becomes `async` and loads dylibs concurrently via `tokio::task::spawn_blocking` + `JoinSet`. Spec-related helpers move to their natural homes (`PluginRegistry`, `Cli`). The built-in subcommand list is derived from clap introspection instead of a hand-maintained const.

**Tech Stack:** tokio (JoinSet, spawn_blocking), clap 4 (CommandFactory introspection).

---

### Task 1: Make `PluginRegistry::load` async with parallel dylib loading

Currently `PluginRegistry::load` is synchronous. Each `LoadedPlugin::load()` call does `dlopen` (blocking syscall) + FFI init + manifest deserialization. With N plugins, these are independent and can run concurrently.

**Files:**
- Modify: `crates/hm-plugin-runtime/src/registry.rs`

**Step 1: Change `load` signature to async**

In `crates/hm-plugin-runtime/src/registry.rs`, change:

```rust
// OLD (line 141):
pub fn load(config: RegistryConfig) -> Result<Self> {

// NEW:
pub async fn load(config: RegistryConfig) -> Result<Self> {
```

**Step 2: Replace sequential loading with `JoinSet`**

Replace the body of `load`. The key change: instead of calling `LoadedPlugin::load()` inline in the loop, collect all dylib paths first, then spawn a `tokio::task::spawn_blocking` for each one, and collect results via `JoinSet`.

```rust
pub async fn load(config: RegistryConfig) -> Result<Self> {
    let dll_ext = std::env::consts::DLL_EXTENSION;
    let mut paths: Vec<PathBuf> = Vec::new();

    // 1. Collect dylib paths from discovery dirs.
    if config.auto_discover {
        for dir in hm_util::dirs::plugin_discovery_dirs() {
            if !dir.is_dir() {
                continue;
            }
            let entries =
                std::fs::read_dir(&dir).with_context(|| format!("read {}", dir.display()))?;
            for ent in entries {
                let Ok(ent) = ent else { continue };
                let path = ent.path();
                if path.extension().and_then(|s| s.to_str()) == Some(dll_ext) {
                    paths.push(path);
                }
            }
        }
    }

    // 2. Add explicit extra paths.
    paths.extend(config.extra_paths.iter().cloned());

    // 3. Load all plugins concurrently.
    let mut set = tokio::task::JoinSet::new();
    for path in paths {
        let host_api = config.host_api.clone();
        set.spawn_blocking(move || {
            let p = LoadedPlugin::load(&path, host_api)
                .with_context(|| format!("load {}", path.display()))?;
            p.manifest.validate().map_err(RuntimeError::from)?;
            Ok(Arc::new(p))
        });
    }

    let mut plugins: Vec<Arc<LoadedPlugin>> = Vec::new();
    while let Some(result) = set.join_next().await {
        plugins.push(result.context("plugin load task panicked")??);
    }

    let capabilities = CapabilityIndex::build(&plugins)?;

    Ok(Self {
        plugins,
        capabilities,
    })
}
```

Add `use tokio::task::JoinSet;` is not needed — use the full path inline, or add to imports. The existing imports already include `std::sync::Arc` and `anyhow::{Context, Result}`.

**Step 3: Run to verify it compiles**

Run: `cargo check -p hm-plugin-runtime`
Expected: PASS (the crate itself compiles; downstream callers will break until updated).

**Step 4: Update all call sites to `.await`**

Five call sites need `.await` added:

1. `crates/hm/src/main.rs:50` — already inside `async fn run()`:
```rust
// OLD:
let registry = PluginRegistry::load(RegistryConfig { ... }).ok();
// NEW:
let registry = PluginRegistry::load(RegistryConfig { ... }).await.ok();
```

2. `crates/hm/src/cli/version.rs:13`:
```rust
// OLD:
let reg = PluginRegistry::load(RegistryConfig { ... })?;
// NEW:
let reg = PluginRegistry::load(RegistryConfig { ... }).await?;
```
Also remove the `#[allow(clippy::unused_async)]` on `run()` — the async is now used.

3. `crates/hm/src/cli/plugin.rs:53` (inside `list()`):
```rust
let reg = PluginRegistry::load(RegistryConfig { ... }).await?;
```
Also remove the `#[allow(clippy::unused_async)]` on `list()`.

4. `crates/hm/src/cli/plugin.rs:81` (inside `info()`):
```rust
let reg = PluginRegistry::load(RegistryConfig { ... }).await?;
```
Also remove the `#[allow(clippy::unused_async)]` on `info()`.

5. `crates/hm/src/orchestrator/scheduler.rs:114`:
```rust
// OLD:
let registry = Arc::new(Mutex::new(
    PluginRegistry::load(RegistryConfig { ... })
        .context("load plugin registry")?,
));
// NEW:
let registry = Arc::new(Mutex::new(
    PluginRegistry::load(RegistryConfig { ... })
        .await
        .context("load plugin registry")?,
));
```
Verify the surrounding function is already `async`. (It is — `scheduler::run` is `async fn`.)

**Step 5: Run to verify everything compiles**

Run: `cargo check --workspace`
Expected: PASS

**Step 6: Run tests**

Run: `cargo test --workspace`
Expected: Same as before (63 passed, 2 pre-existing Python failures).

**Step 7: Commit**

```bash
git add crates/hm-plugin-runtime/src/registry.rs crates/hm/src/main.rs crates/hm/src/cli/version.rs crates/hm/src/cli/plugin.rs crates/hm/src/orchestrator/scheduler.rs
git commit -m "feat(runtime): make PluginRegistry::load async with parallel dylib loading"
```

---

### Task 2: Move `collect_plugin_specs` to `PluginRegistry::subcommand_specs()`

The `collect_plugin_specs` free function in `main.rs:35` only reads manifests from the registry. It belongs on `PluginRegistry` as a method.

**Files:**
- Modify: `crates/hm-plugin-runtime/src/registry.rs`
- Modify: `crates/hm/src/main.rs`

**Step 1: Add the method to `PluginRegistry`**

In `crates/hm-plugin-runtime/src/registry.rs`, inside `impl PluginRegistry`, after the existing `manifests()` method (line 182), add:

```rust
/// Collect all `SubcommandSpec`s declared by loaded plugins.
pub fn subcommand_specs(&self) -> Vec<SubcommandSpec> {
    self.manifests()
        .flat_map(|m| {
            m.capabilities.iter().filter_map(|c| match c {
                Capability::Subcommand(s) => Some(s.clone()),
                _ => None,
            })
        })
        .collect()
}
```

Add `SubcommandSpec` to the existing `use hm_plugin_protocol::{...}` at the top. The current import is:
```rust
use hm_plugin_protocol::{Capability, PluginManifest};
```
Change to:
```rust
use hm_plugin_protocol::{Capability, PluginManifest, SubcommandSpec};
```

**Step 2: Update `main.rs` to use the new method**

Delete the `collect_plugin_specs` free function (lines 35–45) and update the call site:

```rust
// OLD (lines 57-60):
let plugin_specs = registry
    .as_ref()
    .map(collect_plugin_specs)
    .unwrap_or_default();

// NEW:
let plugin_specs = registry
    .as_ref()
    .map(PluginRegistry::subcommand_specs)
    .unwrap_or_default();
```

Also remove `use hm_plugin_protocol::{Capability, SubcommandSpec};` from main.rs — no longer needed there.

**Step 3: Verify**

Run: `cargo check --workspace`
Expected: PASS

**Step 4: Commit**

```bash
git add crates/hm-plugin-runtime/src/registry.rs crates/hm/src/main.rs
git commit -m "refactor: move collect_plugin_specs to PluginRegistry::subcommand_specs()"
```

---

### Task 3: Make `build_augmented_command` an associated function on `Cli`

The free function `build_augmented_command` in `cli/mod.rs` wraps `Cli::command()`. It's a natural associated function on `Cli`.

**Files:**
- Modify: `crates/hm/src/cli/mod.rs`
- Modify: `crates/hm/src/main.rs`

**Step 1: Move the function into an `impl Cli` block**

In `crates/hm/src/cli/mod.rs`, replace the free function (lines 64–76):

```rust
// DELETE:
/// Build a `clap::Command` that contains both the derive-defined
/// built-in subcommands and any plugin-provided subcommands.
///
/// The caller parses with the returned command, then routes based on
/// whether the matched subcommand is a built-in or plugin verb.
#[must_use]
pub fn build_augmented_command(plugin_specs: &[SubcommandSpec]) -> clap::Command {
    let mut cmd = Cli::command();
    for spec in plugin_specs {
        cmd = cmd.subcommand(hm_plugin_runtime::clap_bridge::build_command(spec));
    }
    cmd
}
```

Add an `impl Cli` block (place it right after the `Cli` struct definition, before the `Command` enum):

```rust
impl Cli {
    /// Build a `clap::Command` with plugin subcommands appended to
    /// the derive-defined built-in set.
    #[must_use]
    pub fn command_with_plugins(plugin_specs: &[SubcommandSpec]) -> clap::Command {
        let mut cmd = Self::command();
        for spec in plugin_specs {
            cmd = cmd.subcommand(hm_plugin_runtime::clap_bridge::build_command(spec));
        }
        cmd
    }
}
```

Remove the `use hm_plugin_protocol::SubcommandSpec;` import from the top of `cli/mod.rs` — it's now only used inside `impl Cli`, so move it there or keep it at file scope (either works; keep at file scope is simpler).

**Step 2: Update the call site in `main.rs`**

```rust
// OLD (line 64):
let cmd = cli::build_augmented_command(&plugin_specs);

// NEW:
let cmd = Cli::command_with_plugins(&plugin_specs);
```

The `Cli` import already exists in main.rs (line 16: `use harmont_cli::cli::{self, Cli};`).

**Step 3: Verify**

Run: `cargo check --workspace`
Expected: PASS

**Step 4: Commit**

```bash
git add crates/hm/src/cli/mod.rs crates/hm/src/main.rs
git commit -m "refactor: move build_augmented_command to Cli::command_with_plugins()"
```

---

### Task 4: Replace `BUILTIN_SUBCOMMANDS` with clap introspection

The hand-maintained `BUILTIN_SUBCOMMANDS` array drifts if someone adds a new `Command` variant. Replace it with runtime introspection of the derive-generated base command.

**Files:**
- Modify: `crates/hm/src/cli/mod.rs`
- Modify: `crates/hm/src/main.rs`

**Step 1: Delete `BUILTIN_SUBCOMMANDS` from `cli/mod.rs`**

Delete lines 78–81:
```rust
// DELETE:
/// Names of built-in subcommands defined in the [`Command`] derive enum.
/// Used by the two-phase parser to decide whether to reconstruct `Cli`
/// via `from_arg_matches` or route to the plugin dispatcher.
pub const BUILTIN_SUBCOMMANDS: &[&str] = &["run", "version", "plugin", "dev"];
```

**Step 2: Update `main.rs` to introspect the base command**

Before building the augmented command, snapshot the built-in names:

```rust
// OLD (around lines 62-64):
let cmd = Cli::command_with_plugins(&plugin_specs);
let matches = cmd.get_matches();

// NEW:
use std::collections::HashSet;

let base_cmd = Cli::command();
let builtins: HashSet<String> = base_cmd
    .get_subcommands()
    .map(|c| c.get_name().to_owned())
    .collect();

let cmd = Cli::command_with_plugins(&plugin_specs);
let matches = cmd.get_matches();
```

Note: we call `Cli::command()` once to snapshot names, then `Cli::command_with_plugins()` builds a fresh one with plugins appended. This is two `command()` calls, but they're cheap (no allocation beyond the Command tree).

Alternatively, to avoid the double build, we can change `Cli::command_with_plugins()` to also return the builtin set. But that couples the API. Simpler to keep them separate — `command()` is instantaneous.

Then update the routing logic:

```rust
// OLD (line 91):
if cli::BUILTIN_SUBCOMMANDS.contains(&sub_name) {

// NEW:
if builtins.contains(sub_name) {
```

**Step 3: Verify**

Run: `cargo check --workspace`
Expected: PASS

**Step 4: Run tests**

Run: `cargo test --workspace`
Expected: Same results as before.

**Step 5: Commit**

```bash
git add crates/hm/src/cli/mod.rs crates/hm/src/main.rs
git commit -m "refactor: derive builtin subcommand names from clap introspection"
```

---

## Verification

1. `cargo check --workspace` — clean compile
2. `cargo test --workspace` — all tests pass (except pre-existing Python-dep failures)
3. `cargo run -- --help` — shows plugin subcommands alongside built-ins
4. `cargo run -- cloud --help` — shows cloud sub-subcommands
