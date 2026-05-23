# Extract Plugin Runtime Crate

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Extract the plugin loading/registry/host-API code from `crates/hm/src/plugin/` into a standalone `crates/hm-plugin-runtime/` crate so the runtime is reusable, testable in isolation, and decoupled from CLI concerns.

**Architecture:** Move 6 modules (`host.rs`, `host_api.rs`, `registry.rs`, `manifest.rs`, `paths.rs`, `install.rs`) into `crates/hm-plugin-runtime/src/`. The only coupling to the binary is `crate::error::HmError` — extract plugin-specific error variants into a new `RuntimeError` enum owned by the runtime crate. The binary's `HmError` wraps `RuntimeError` via `#[from]`. The binary's `plugin` module becomes a thin re-export shim. Integration tests keep working because they import from `harmont_cli::plugin`, which re-exports from the new crate.

**Tech Stack:** Rust, stabby, libloading, tokio, serde_json, reqwest (install only)

---

## Task 1: Create the `hm-plugin-runtime` crate scaffold

**Files:**
- Create: `crates/hm-plugin-runtime/Cargo.toml`
- Create: `crates/hm-plugin-runtime/src/lib.rs`
- Modify: `Cargo.toml` (workspace root)

### Step 1: Create the crate directory

```bash
mkdir -p crates/hm-plugin-runtime/src
```

### Step 2: Create `Cargo.toml`

```toml
[package]
name = "hm-plugin-runtime"
version = "0.0.0-dev"
edition.workspace = true
license.workspace = true
repository.workspace = true
description = "Plugin loading, discovery, and host-API runtime for Harmont CLI."

[dependencies]
hm-plugin-protocol = { workspace = true }
hm-plugin-sdk      = { workspace = true }
hm-util            = { workspace = true }
stabby             = { workspace = true }
libloading         = "0.8"
tokio              = { workspace = true }
tokio-util         = { workspace = true }
serde_json         = { workspace = true }
anyhow             = "1"
thiserror          = { workspace = true }
semver             = { workspace = true }
tracing            = "0.1"
chrono             = { workspace = true }
uuid               = { workspace = true }
reqwest            = { version = "0.13", default-features = false, features = ["rustls"] }
sha2               = "0.10"
hex                = "0.4"
tempfile           = "3"

[lints]
workspace = true
```

### Step 3: Create `src/lib.rs`

```rust
//! Plugin loading, discovery, and host-API runtime.

pub mod error;
pub mod host;
pub mod host_api;
pub mod install;
pub mod manifest;
pub mod paths;
pub mod registry;

pub use host::LoadedPlugin;
pub use registry::{PluginRegistry, RegistryConfig};
```

### Step 4: Add to workspace

In root `Cargo.toml`, add `"crates/hm-plugin-runtime"` to the `members` array and `default-members` array. Add to `[workspace.dependencies]`:

```toml
hm-plugin-runtime = { path = "crates/hm-plugin-runtime", version = "0.0.0-dev" }
```

### Step 5: Verify

```bash
# Won't compile yet — modules are empty. Just check structure.
ls crates/hm-plugin-runtime/src/
```

### Step 6: Commit

```bash
git add crates/hm-plugin-runtime/ Cargo.toml
git commit -m "feat(plugin-runtime): scaffold hm-plugin-runtime crate"
```

---

## Task 2: Define `RuntimeError` in the new crate

**Files:**
- Create: `crates/hm-plugin-runtime/src/error.rs`

### Step 1: Create `error.rs`

Extract the 6 plugin-specific error variants from `crates/hm/src/error.rs` into a new `RuntimeError` enum. These are the variants used by `host.rs` and `registry.rs`:

```rust
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("plugin '{name}' failed to load from {path}: {reason}")]
    PluginLoad {
        name: String,
        path: PathBuf,
        reason: String,
        doc_url: &'static str,
    },

    #[error("plugin '{name}': API version mismatch (plugin={found_api}, host={expected_api})")]
    PluginManifest {
        name: String,
        expected_api: u32,
        found_api: u32,
    },

    #[error(
        "plugin '{name}': required host fn '{fn_name}' is unavailable (this hm build is too old; needs >= {min_hm_version})"
    )]
    PluginMissingHostFn {
        name: String,
        fn_name: String,
        min_hm_version: semver::Version,
    },

    #[error("plugin '{name}' panicked during '{capability}': {message}")]
    PluginPanic {
        name: String,
        capability: String,
        message: String,
    },

    #[error("plugin '{name}' timed out after {after_ms}ms during '{capability}'")]
    PluginTimeout {
        name: String,
        capability: String,
        after_ms: u32,
    },

    #[error("plugin conflict: both '{plugin_a}' and '{plugin_b}' claim '{verb}'")]
    PluginConflict {
        verb: String,
        plugin_a: String,
        plugin_b: String,
    },
}
```

### Step 2: Verify

```bash
cargo check -p hm-plugin-runtime
```

### Step 3: Commit

```bash
git add crates/hm-plugin-runtime/src/error.rs
git commit -m "feat(plugin-runtime): define RuntimeError for plugin system errors"
```

---

## Task 3: Move modules into the new crate

**Files:**
- Move: `crates/hm/src/plugin/host.rs` → `crates/hm-plugin-runtime/src/host.rs`
- Move: `crates/hm/src/plugin/host_api.rs` → `crates/hm-plugin-runtime/src/host_api.rs`
- Move: `crates/hm/src/plugin/registry.rs` → `crates/hm-plugin-runtime/src/registry.rs`
- Move: `crates/hm/src/plugin/manifest.rs` → `crates/hm-plugin-runtime/src/manifest.rs`
- Move: `crates/hm/src/plugin/paths.rs` → `crates/hm-plugin-runtime/src/paths.rs`
- Move: `crates/hm/src/plugin/install.rs` → `crates/hm-plugin-runtime/src/install.rs`

### Step 1: Copy files

```bash
cp crates/hm/src/plugin/host.rs crates/hm-plugin-runtime/src/host.rs
cp crates/hm/src/plugin/host_api.rs crates/hm-plugin-runtime/src/host_api.rs
cp crates/hm/src/plugin/registry.rs crates/hm-plugin-runtime/src/registry.rs
cp crates/hm/src/plugin/manifest.rs crates/hm-plugin-runtime/src/manifest.rs
cp crates/hm/src/plugin/paths.rs crates/hm-plugin-runtime/src/paths.rs
cp crates/hm/src/plugin/install.rs crates/hm-plugin-runtime/src/install.rs
```

### Step 2: Fix imports in all 6 files

In every moved file, replace:
- `use crate::error::HmError;` → `use crate::error::RuntimeError;`
- `HmError::PluginPanic` → `RuntimeError::PluginPanic` (and all other variant references)
- `HmError::PluginLoad` → `RuntimeError::PluginLoad`
- `HmError::PluginManifest` → `RuntimeError::PluginManifest`
- `HmError::PluginMissingHostFn` → `RuntimeError::PluginMissingHostFn`
- `HmError::PluginConflict` → `RuntimeError::PluginConflict`
- `use super::` → `use crate::` (modules are now siblings in the new crate)

Specific changes per file:

**host.rs:**
- `use super::host_api::HostApiImpl;` → `use crate::host_api::HostApiImpl;`
- `use crate::error::HmError;` → `use crate::error::RuntimeError;`
- `HmError::PluginPanic` → `RuntimeError::PluginPanic` (in `ffi_err_to_anyhow`)

**host_api.rs:**
- No `crate::` imports to fix (only uses external crates and `hm_plugin_sdk`)
- `use tokio_util::sync::CancellationToken;` stays as-is

**registry.rs:**
- `use super::host::LoadedPlugin;` → `use crate::host::LoadedPlugin;`
- `use super::host_api::HostApiImpl;` → `use crate::host_api::HostApiImpl;`
- `use super::manifest::{ManifestError, validate_standalone};` → `use crate::manifest::{ManifestError, validate_standalone};`
- `use super::paths;` → `use crate::paths;`
- `use crate::error::HmError;` → `use crate::error::RuntimeError;`
- All `HmError::PluginConflict` → `RuntimeError::PluginConflict`
- All `HmError::PluginManifest` → `RuntimeError::PluginManifest`
- All `HmError::PluginLoad` → `RuntimeError::PluginLoad`

**manifest.rs:**
- No `crate::` import changes needed (only uses `hm_plugin_protocol`)

**paths.rs:**
- No changes needed (only uses `hm_util` and `std`)

**install.rs:**
- `use super::host::LoadedPlugin;` → `use crate::host::LoadedPlugin;`
- `use super::host_api::HostApiImpl;` → `use crate::host_api::HostApiImpl;`
- `use super::paths;` → `use crate::paths;`

### Step 3: Verify

```bash
cargo check -p hm-plugin-runtime
```

### Step 4: Commit

```bash
git add crates/hm-plugin-runtime/src/
git commit -m "feat(plugin-runtime): move plugin modules into runtime crate"
```

---

## Task 4: Wire `HmError` to wrap `RuntimeError`

**Files:**
- Modify: `crates/hm/Cargo.toml` — add `hm-plugin-runtime` dependency
- Modify: `crates/hm/src/error.rs` — replace plugin variants with `#[from] RuntimeError`
- Modify: `crates/hm/src/plugin/mod.rs` — re-export from runtime crate

### Step 1: Add dependency

In `crates/hm/Cargo.toml`, add:

```toml
hm-plugin-runtime = { workspace = true }
```

### Step 2: Update `error.rs`

Replace the 6 plugin-specific variants in `HmError` with a single wrapper:

```rust
#[error(transparent)]
PluginRuntime(#[from] hm_plugin_runtime::error::RuntimeError),
```

Delete these variants from `HmError`:
- `PluginLoad { name, path, reason, doc_url }`
- `PluginManifest { name, expected_api, found_api }`
- `PluginMissingHostFn { name, fn_name, min_hm_version }`
- `PluginPanic { name, capability, message }`
- `PluginTimeout { name, capability, after_ms }`
- `PluginConflict { verb, plugin_a, plugin_b }`

Update the `category()` match in `HmError` to handle the new wrapper variant. The `RuntimeError` variants map to two categories:

```rust
Self::PluginRuntime(ref e) => {
    use hm_plugin_runtime::error::RuntimeError;
    match e {
        RuntimeError::PluginLoad { .. }
        | RuntimeError::PluginManifest { .. }
        | RuntimeError::PluginMissingHostFn { .. }
        | RuntimeError::PluginConflict { .. } => ErrorCategory::PluginLoad,
        RuntimeError::PluginPanic { .. }
        | RuntimeError::PluginTimeout { .. } => ErrorCategory::PluginRuntime,
    }
},
```

### Step 3: Rewrite `crates/hm/src/plugin/mod.rs` as re-export shim

Replace the entire module with re-exports from the new crate:

```rust
//! Plugin system — re-exports from `hm_plugin_runtime`.

pub use hm_plugin_runtime::host;
pub use hm_plugin_runtime::host_api;
pub use hm_plugin_runtime::install;
pub use hm_plugin_runtime::manifest;
pub use hm_plugin_runtime::paths;
pub use hm_plugin_runtime::registry;

pub use hm_plugin_runtime::{LoadedPlugin, PluginRegistry, RegistryConfig};
```

### Step 4: Delete original source files

```bash
rm crates/hm/src/plugin/host.rs
rm crates/hm/src/plugin/host_api.rs
rm crates/hm/src/plugin/registry.rs
rm crates/hm/src/plugin/manifest.rs
rm crates/hm/src/plugin/paths.rs
rm crates/hm/src/plugin/install.rs
```

### Step 5: Verify

```bash
cargo check --workspace
```

All callers in the binary (`cli/plugin.rs`, `cli/external.rs`, `cli/version.rs`, `orchestrator/scheduler.rs`) import via `crate::plugin::` which now re-exports from the runtime crate. They should compile without changes.

### Step 6: Commit

```bash
git add crates/hm/ crates/hm-plugin-runtime/ Cargo.lock
git commit -m "refactor: wire HmError to wrap RuntimeError, delete original plugin sources"
```

---

## Task 5: Remove plugin-only dependencies from the binary crate

**Files:**
- Modify: `crates/hm/Cargo.toml`

### Step 1: Remove dependencies that are now only used by `hm-plugin-runtime`

These were only used by the plugin modules and can be removed from `crates/hm/Cargo.toml`:

- `stabby` — only used in `host.rs` (now in runtime crate)
- `libloading` — only used in `host.rs` (now in runtime crate)

Do NOT remove these — they are still used elsewhere in the binary:
- `sha2` — used by `creds_store.rs`
- `hex` — used by `creds_store.rs`
- `reqwest` — used by cloud/API code
- `tempfile` — used by tests
- `hm-plugin-sdk` — still used by integration tests that import `ffi` types directly
- `hm-plugin-protocol` — used by orchestrator, commands, output

### Step 2: Verify

```bash
cargo check --workspace
```

### Step 3: Commit

```bash
git add crates/hm/Cargo.toml Cargo.lock
git commit -m "chore: remove stabby/libloading from binary crate (moved to plugin-runtime)"
```

---

## Task 6: Fix integration tests

**Files:**
- Modify: `crates/hm/tests/plugin_host_fns.rs`
- Modify: `crates/hm/tests/plugin_manifest.rs`
- Modify: `crates/hm/tests/plugin_registry.rs`
- Modify: `crates/hm/tests/plugin_kv_concurrency.rs`
- Modify: `crates/hm/tests/runner_dispatch.rs`

### Step 1: Check if tests compile

```bash
cargo test --workspace --no-run 2>&1 | head -30
```

Integration tests import `harmont_cli::plugin::*`. Since `mod.rs` re-exports everything, they should already work. If any test directly imports a type that moved (like `harmont_cli::plugin::host::dummy_subcommand_input`), the re-export shim handles it via `pub use hm_plugin_runtime::host`.

If there are compilation errors, fix the imports. The pattern is always:
- `harmont_cli::plugin::Foo` → still works (re-exported)
- `harmont_cli::plugin::host::Foo` → still works (`pub use hm_plugin_runtime::host`)

### Step 2: Run tests

```bash
cargo test -p harmont-cli --test plugin_host_fns --test plugin_manifest --test plugin_registry --test runner_dispatch --test plugin_kv_concurrency
```

### Step 3: Run full workspace check

```bash
cargo check --workspace
cargo test --workspace -- --skip cmd_run_local_autoselect --skip zero_pipelines --skip many_pipelines --skip version_prints_api --skip cmd_cloud
```

### Step 4: Commit (if any fixes were needed)

```bash
git add crates/hm/tests/
git commit -m "fix: update integration test imports for plugin-runtime extraction"
```

---

## Task 7: Update documentation

**Files:**
- Modify: `CLAUDE.md`
- Modify: `crates/hm/CLAUDE.md`
- Modify: `crates/hm-plugin-runtime/src/lib.rs` (module doc)

### Step 1: Update root `CLAUDE.md`

Add `crates/hm-plugin-runtime/` to the crate listing:

```
- `crates/hm-plugin-runtime/` — plugin loading, discovery, host-API runtime. Owns `LoadedPlugin`, `PluginRegistry`, `HostApiImpl`.
```

### Step 2: Update `crates/hm/CLAUDE.md`

In the "Plugin system" section, note that the runtime is now in a separate crate:

```
Plugin runtime lives in `crates/hm-plugin-runtime/`. The `plugin/`
module in this crate re-exports everything from the runtime crate.
```

### Step 3: Verify

```bash
cargo check --workspace
```

### Step 4: Commit

```bash
git add CLAUDE.md crates/hm/CLAUDE.md
git commit -m "docs: update CLAUDE.md for plugin-runtime extraction"
```
