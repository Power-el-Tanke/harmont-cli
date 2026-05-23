# Extism → Stabby Plugin System Rewrite

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace the extism/WASM plugin system with stabby-based native dylibs to enable async plugin code, reduce boilerplate, and give plugins full Rust ecosystem access.

**Architecture:** Plugins become native shared libraries (`.dylib`/`.so`/`.dll`) loaded via stabby's `libloading` integration. A single `RawPlugin` stabby trait defines the FFI boundary; complex types cross the boundary as borsh-serialized bytes in `stabby::vec::Vec<u8>`. User-facing SDK provides ergonomic async Rust traits plus an `hm_plugin!` macro that generates all FFI glue. Host capabilities are passed as a `RawHostApi` stabby trait object instead of 32+ individual host functions. Plugins share the host's tokio runtime.

**Tech Stack:** stabby `=72.1.1` (ABI locked), libloading (via stabby), borsh (wire format at FFI boundary), tokio (shared runtime)

---

## Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Sandboxing | Trust plugins fully | Plugins are first-party or vetted; no WASM sandbox overhead |
| Built-in plugins | All as dylibs | Consistent model; built-ins go through same load path as third-party |
| Distribution | Pre-built per-platform | Plugin registry serves per-target-triple dylibs |
| Async model | Shared host runtime | Plugins run on host's tokio; simplest approach |
| ABI version | stabby `=72.1.1` locked | SemVer Prime guarantees same-version ABI compat |
| Wire format at boundary | borsh bytes | Faster/smaller than JSON; deterministic binary encoding; avoids making all protocol types `IStable` |

## Architecture Overview

```
┌────────────────────────────────────────────────────────────────┐
│  hm-plugin-protocol  (mostly unchanged)                        │
│  • Serde structs: PluginManifest, ExecutorInput, BuildEvent…   │
│  • Remove host_abi.rs (Docker/keyring/socket/tty types)        │
│  • Keep Level, KvScope (used by reduced host API)              │
│  • Remove allowed_hosts, required_host_fns from manifest       │
└────────────────────────────────────────────────────────────────┘
                              │
┌────────────────────────────────────────────────────────────────┐
│  hm-plugin-sdk  (rewritten)                                    │
│  • ffi.rs: RawPlugin + RawHostApi stabby traits (FFI boundary) │
│  • traits.rs: async StepExecutor, LifecycleHook, etc.          │
│  • context.rs: PluginContext wrapping RawHostApi ergonomically  │
│  • macros.rs: hm_plugin! macro (generates RawPlugin + export)  │
│  • Depends on: stabby =72.1.1, hm-plugin-protocol, borsh      │
│  • NO extism-pdk dependency                                    │
└────────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────┼──────────────────────────────────┐
│  Plugin dylibs (.dylib/.so) │  hm binary (host)                │
│  • crate-type = ["cdylib"]  │  • stabby libloading for loading │
│  • #[stabby::export] entry  │  • RawHostApi implementation     │
│  • Uses async freely        │  • No more PluginPool/semaphore  │
│  • Full crate ecosystem     │  • Registry discovers *.dylib    │
│  • No #![no_main]           │  • No build.rs plugin compile    │
└─────────────────────────────┴──────────────────────────────────┘
```

## FFI Boundary Design

### RawPlugin (plugin → host)

```rust
// crates/hm-plugin-sdk/src/ffi.rs

use stabby::future::DynFuture;

type FfiBytes = stabby::vec::Vec<u8>;
type FfiSlice<'a> = stabby::slice::Slice<'a, u8>;
type FfiResult = stabby::result::Result<FfiBytes, FfiBytes>;

#[stabby::stabby]
pub trait RawPlugin: Send + Sync {
    extern "C" fn manifest(&self) -> FfiBytes;
    fn execute_step<'a>(&'a self, input: FfiSlice<'a>) -> DynFuture<'a, FfiResult>;
    fn on_hook_event<'a>(&'a self, event: FfiSlice<'a>) -> DynFuture<'a, FfiResult>;
    fn run_subcommand<'a>(&'a self, input: FfiSlice<'a>) -> DynFuture<'a, FfiResult>;
    fn on_output_event<'a>(&'a self, event: FfiSlice<'a>) -> DynFuture<'a, FfiResult>;
    fn finalize_output<'a>(&'a self) -> DynFuture<'a, FfiResult>;
}
```

### RawHostApi (host → plugin)

```rust
// crates/hm-plugin-sdk/src/ffi.rs (continued)

#[stabby::stabby]
pub trait RawHostApi: Send + Sync {
    extern "C" fn log(&self, level: u8, msg: FfiSlice<'_>);
    extern "C" fn kv_get(&self, scope: u8, key: FfiSlice<'_>) -> stabby::option::Option<FfiBytes>;
    extern "C" fn kv_set(&self, scope: u8, key: FfiSlice<'_>, val: FfiSlice<'_>);
    extern "C" fn emit_event(&self, event_borsh: FfiSlice<'_>);
    extern "C" fn emit_step_log(&self, stream: u8, bytes: FfiSlice<'_>);
    extern "C" fn should_cancel(&self) -> bool;
    extern "C" fn write_stdout(&self, bytes: FfiSlice<'_>);
    extern "C" fn write_stderr(&self, bytes: FfiSlice<'_>);
    extern "C" fn archive_read(&self, id_borsh: FfiSlice<'_>, offset: u64, max: u64) -> FfiBytes;
    extern "C" fn archive_total_size(&self, id_borsh: FfiSlice<'_>) -> u64;
    extern "C" fn fs_read_config(&self, rel_path: FfiSlice<'_>) -> stabby::option::Option<FfiBytes>;
}
```

11 host methods (down from 32+). Docker/keyring/socket/tty/loopback/browser operations are now direct crate usage by plugins.

### Plugin entry point

```rust
// Generated by hm_plugin! — each plugin dylib exports this symbol:
#[stabby::export]
extern "C" fn hm_load_plugin(
    ctx: /* DynRef<'static, vtable!(RawHostApi + Send + Sync)> */
) -> stabby::result::Result<
    /* Dyn<'static, Box<()>, vtable!(RawPlugin + Send + Sync)> */,
    FfiBytes
>
```

### User-facing traits (async, ergonomic)

```rust
// crates/hm-plugin-sdk/src/traits.rs

pub trait StepExecutor: Send + Sync + Default {
    fn run(&self, ctx: &PluginContext, input: ExecutorInput)
        -> impl Future<Output = Result<StepResult, PluginError>> + Send + '_;
}

pub trait LifecycleHook: Send + Sync + Default {
    fn on_event(&self, ctx: &PluginContext, event: HookEvent)
        -> impl Future<Output = Result<HookOutcome, PluginError>> + Send + '_;
}

pub trait SubcommandPlugin: Send + Sync + Default {
    fn run(&self, ctx: &PluginContext, input: SubcommandInput)
        -> impl Future<Output = Result<ExitInfo, PluginError>> + Send + '_;
}

pub trait OutputFormatter: Send + Sync + Default {
    fn on_event(&self, ctx: &PluginContext, event: BuildEvent)
        -> impl Future<Output = Result<(), PluginError>> + Send + '_;

    fn finalize(&self, ctx: &PluginContext)
        -> impl Future<Output = Result<Vec<u8>, PluginError>> + Send + '_ {
        async { Ok(Vec::new()) }
    }
}
```

Plugin authors implement these with `async fn` (RPITIT, stable since Rust 1.75). The `hm_plugin!` macro wraps them in `DynFuture` at the FFI boundary.

## Host-side changes

### What gets removed

| File | Why |
|------|-----|
| `src/plugin/pool.rs` | Native dylibs are thread-safe; no instance pooling needed |
| `src/plugin/host_fns.rs` (1065 lines) | 32 extism host functions → 11-method RawHostApi trait object |
| `src/plugin/signal.rs` | Cancellation signal reimplemented in RawHostApi |
| `src/plugin/embedded.rs` | No more embedding — plugins installed to `~/.harmont/plugins/` by `install.sh` |
| `build.rs` | No more WASM or plugin compilation at build time |

### What gets rewritten

| File | Change |
|------|--------|
| `src/plugin/host.rs` | `LoadedPlugin` wraps stabby `Library` + trait object instead of `PluginPool` |
| `src/plugin/registry.rs` | Discover `*.dylib`/`*.so` from `~/.harmont/plugins/` + `.harmont/plugins/`; no `HOST_FN_NAMES` validation; no `embedded` config field |
| `src/plugin/manifest.rs` | Simplified validation (no `required_host_fns`, no `allowed_hosts`) |
| `src/plugin/paths.rs` | Discovery paths: `~/.harmont/plugins/` (user/built-in) + `.harmont/plugins/` (project). Extension `.wasm` → platform dylib ext |

### What stays unchanged

- `src/orchestrator/scheduler.rs` — calls `plugin.call_capability()` which we'll preserve as async method
- `src/orchestrator/output_subscriber.rs` — same pattern, call into output plugin
- `src/dispatcher.rs` — same pattern, call into subcommand plugin
- `src/orchestrator/events.rs`, `graph.rs`, `cache.rs`, `archive.rs` — untouched

## Protocol crate changes

### Add borsh derives

All wire types that cross the FFI boundary need `BorshSerialize` + `BorshDeserialize` derives in addition to existing serde derives. This includes: `PluginManifest`, `Capability`, `ExecutorInput`, `StepResult`, `HookEvent`, `HookOutcome`, `SubcommandInput`, `ExitInfo`, `BuildEvent`, `PluginError`, and all their transitive field types. Add `borsh = { workspace = true }` to `hm-plugin-protocol/Cargo.toml` and derive on each struct/enum.

Types that DON'T need borsh (only used for JSON config/output, not FFI boundary): `Pipeline`, `CommandStep`, `WaitStep`, `Cache` (IR types parsed from YAML/JSON config files).

`serde_json::Value` fields (`config_schema`, `args_schema`, `runner_args`) need special handling — borsh can't directly serialize `serde_json::Value`. Options: (a) serialize the Value to a JSON string first, then borsh the string, or (b) change these fields to `Option<Vec<u8>>` in the manifest (raw bytes). Option (a) is simplest: wrap in a newtype with a custom borsh impl that round-trips through JSON string.

### Remove from `host_abi.rs`

Move to docker plugin crate (internal types):
- `DockerStartArgs`, `DockerExecArgs`, `DockerCommitArgs`, `DockerExtractArgs`

Delete entirely (plugins use native crates):
- `SocketHandle`, `SocketReadArgs`, `SocketWriteArgs`
- `LoopbackHandle`, `LoopbackRecvArgs`, `CallbackData`
- `KeyringArgs`, `KeyringSetArgs`
- `TtyPromptArgs`, `TtyConfirmArgs`

Keep in `host_abi.rs`:
- `Level` (logging through host)
- `KvScope` (KV through host)
- `ArchiveReadArgs` (archive access through host)

### Remove from `PluginManifest`

- `allowed_hosts: Vec<String>` — no HTTP sandboxing with native dylibs
- `required_host_fns: Vec<String>` — no host function declaration needed

---

## Task 1: Add stabby + define FFI boundary traits

**Files:**
- Modify: `Cargo.toml` (workspace root)
- Modify: `crates/hm-plugin-sdk/Cargo.toml`
- Create: `crates/hm-plugin-sdk/src/ffi.rs`
- Modify: `crates/hm-plugin-sdk/src/lib.rs`

**Step 1: Add stabby workspace dependency**

In `Cargo.toml` (workspace root), add under `[workspace.dependencies]`:
```toml
stabby = { version = "=72.1.1", features = ["libloading"] }
borsh  = { version = "1", features = ["derive"] }
```

Remove:
```toml
extism      = "1"
extism-pdk  = "1"
```

(Don't remove yet — crates still reference them. We'll remove at cleanup.)

**Step 2: Update hm-plugin-sdk Cargo.toml**

Replace `extism-pdk` with `stabby` + `borsh`:
```toml
[dependencies]
hm-plugin-protocol = { workspace = true }
stabby             = { workspace = true }
borsh              = { workspace = true }
serde              = { workspace = true }
serde_json         = { workspace = true }
```

**Step 3: Write the FFI trait definitions**

Create `crates/hm-plugin-sdk/src/ffi.rs`:

```rust
#![allow(unsafe_code)]

use stabby::future::DynFuture;

pub type FfiBytes = stabby::vec::Vec<u8>;
pub type FfiSlice<'a> = stabby::slice::Slice<'a, u8>;
pub type FfiResult = stabby::result::Result<FfiBytes, FfiBytes>;

#[stabby::stabby]
pub trait RawPlugin: Send + Sync {
    extern "C" fn manifest(&self) -> FfiBytes;
    fn execute_step<'a>(&'a self, input: FfiSlice<'a>) -> DynFuture<'a, FfiResult>;
    fn on_hook_event<'a>(&'a self, event: FfiSlice<'a>) -> DynFuture<'a, FfiResult>;
    fn run_subcommand<'a>(&'a self, input: FfiSlice<'a>) -> DynFuture<'a, FfiResult>;
    fn on_output_event<'a>(&'a self, event: FfiSlice<'a>) -> DynFuture<'a, FfiResult>;
    fn finalize_output<'a>(&'a self) -> DynFuture<'a, FfiResult>;
}

#[stabby::stabby]
pub trait RawHostApi: Send + Sync {
    extern "C" fn log(&self, level: u8, msg: FfiSlice<'_>);
    extern "C" fn kv_get(&self, scope: u8, key: FfiSlice<'_>) -> stabby::option::Option<FfiBytes>;
    extern "C" fn kv_set(&self, scope: u8, key: FfiSlice<'_>, val: FfiSlice<'_>);
    extern "C" fn emit_event(&self, event_borsh: FfiSlice<'_>);
    extern "C" fn emit_step_log(&self, stream: u8, bytes: FfiSlice<'_>);
    extern "C" fn should_cancel(&self) -> bool;
    extern "C" fn write_stdout(&self, bytes: FfiSlice<'_>);
    extern "C" fn write_stderr(&self, bytes: FfiSlice<'_>);
    extern "C" fn archive_read(&self, id_borsh: FfiSlice<'_>, offset: u64, max: u64) -> FfiBytes;
    extern "C" fn archive_total_size(&self, id_borsh: FfiSlice<'_>) -> u64;
    extern "C" fn fs_read_config(&self, rel_path: FfiSlice<'_>) -> stabby::option::Option<FfiBytes>;
}
```

Note: stabby `#[stabby::stabby]` on traits generates `RawPluginDyn`/`RawPluginDynMut` extension traits and ABI-stable vtables. Import these with `use ffi::{RawPluginDyn, RawHostApiDyn}` when calling through trait objects.

**Step 4: Wire ffi module into lib.rs**

Add `pub mod ffi;` to `crates/hm-plugin-sdk/src/lib.rs`. Comment out the old `extism_pdk` re-export for now (it will be removed in cleanup).

**Step 5: Verify compilation**

Run: `cargo check -p hm-plugin-sdk`
Expected: compiles (may have warnings about unused old modules)

**Step 6: Write a compile-test for trait object creation**

Add test in `crates/hm-plugin-sdk/src/ffi.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    // Compile-time check: RawPlugin can be made into a trait object
    fn _assert_raw_plugin_object_safe(_: stabby::Dyn<'_, stabby::boxed::Box<()>, stabby::vtable!(RawPlugin + Send + Sync)>) {}
    fn _assert_raw_host_api_object_safe(_: stabby::Dyn<'_, stabby::boxed::Box<()>, stabby::vtable!(RawHostApi + Send + Sync)>) {}
}
```

Run: `cargo test -p hm-plugin-sdk`
Expected: compiles and passes (these are compile-time assertions)

**Step 7: Commit**

```bash
git add crates/hm-plugin-sdk/src/ffi.rs crates/hm-plugin-sdk/Cargo.toml crates/hm-plugin-sdk/src/lib.rs Cargo.toml
git commit -m "feat(sdk): define RawPlugin + RawHostApi stabby FFI traits"
```

---

## Task 2: User-facing async traits + PluginContext

**Files:**
- Modify: `crates/hm-plugin-sdk/src/executor.rs`
- Modify: `crates/hm-plugin-sdk/src/hook.rs`
- Modify: `crates/hm-plugin-sdk/src/output.rs`
- Modify: `crates/hm-plugin-sdk/src/subcommand.rs`
- Create: `crates/hm-plugin-sdk/src/context.rs`
- Modify: `crates/hm-plugin-sdk/src/lib.rs`

**Step 1: Write PluginContext**

Create `crates/hm-plugin-sdk/src/context.rs`. This wraps `RawHostApi` with ergonomic Rust-native methods:

```rust
use std::sync::Arc;
use hm_plugin_protocol::{BuildEvent, KvScope, Level};
use crate::ffi::{FfiBytes, FfiSlice, RawHostApiDyn};

pub struct PluginContext {
    // Holds the raw stabby trait object reference.
    // The exact type here depends on stabby's DynRef — the key idea
    // is that this stores the host-provided API for the plugin's lifetime.
    raw: /* stabby trait object ref */,
}

impl PluginContext {
    pub fn log(&self, level: Level, msg: &str) { /* marshal level→u8, msg→FfiSlice, call raw.log() */ }
    pub fn kv_get(&self, scope: KvScope, key: &str) -> Option<Vec<u8>> { /* marshal, call raw.kv_get(), unmarshal */ }
    pub fn kv_set(&self, scope: KvScope, key: &str, val: &[u8]) { /* marshal, call raw.kv_set() */ }
    pub fn emit_event(&self, event: &BuildEvent) { /* borsh::to_vec, call raw.emit_event() */ }
    pub fn emit_step_log(&self, stream: hm_plugin_protocol::StdStream, bytes: &[u8]) { /* marshal stream→u8, call raw */ }
    pub fn should_cancel(&self) -> bool { /* call raw.should_cancel() */ }
    pub fn write_stdout(&self, bytes: &[u8]) { /* call raw.write_stdout() */ }
    pub fn write_stderr(&self, bytes: &[u8]) { /* call raw.write_stderr() */ }
    pub fn archive_read(&self, id: hm_plugin_protocol::ArchiveId, offset: u64, max: u64) -> Vec<u8> { /* borsh::to_vec(id), call raw, convert result */ }
    pub fn archive_total_size(&self, id: hm_plugin_protocol::ArchiveId) -> u64 { /* marshal, call raw */ }
    pub fn fs_read_config(&self, rel_path: &str) -> Option<Vec<u8>> { /* marshal, call raw */ }
}
```

The exact stabby type for storing the trait object reference needs to be worked out during implementation. Key patterns:
- `DynRef<'static, vtable!(RawHostApi + Send + Sync)>` for borrowed
- `Dyn<'static, Arc<()>, vtable!(RawHostApi + Send + Sync)>` for owned

**Step 2: Rewrite user-facing traits**

Replace the contents of:

`crates/hm-plugin-sdk/src/executor.rs`:
```rust
use crate::context::PluginContext;
use hm_plugin_protocol::{ExecutorInput, PluginError, StepResult};

pub trait StepExecutor: Send + Sync + Default {
    fn run(&self, ctx: &PluginContext, input: ExecutorInput)
        -> impl Future<Output = Result<StepResult, PluginError>> + Send + '_;
}
```

`crates/hm-plugin-sdk/src/hook.rs`:
```rust
use crate::context::PluginContext;
use hm_plugin_protocol::{HookEvent, HookOutcome, PluginError};

pub trait LifecycleHook: Send + Sync + Default {
    fn on_event(&self, ctx: &PluginContext, event: HookEvent)
        -> impl Future<Output = Result<HookOutcome, PluginError>> + Send + '_;
}
```

`crates/hm-plugin-sdk/src/subcommand.rs`:
```rust
use crate::context::PluginContext;
use hm_plugin_protocol::{ExitInfo, PluginError, SubcommandInput};

pub trait SubcommandPlugin: Send + Sync + Default {
    fn run(&self, ctx: &PluginContext, input: SubcommandInput)
        -> impl Future<Output = Result<ExitInfo, PluginError>> + Send + '_;
}
```

`crates/hm-plugin-sdk/src/output.rs`:
```rust
use crate::context::PluginContext;
use hm_plugin_protocol::{BuildEvent, PluginError};

pub trait OutputFormatter: Send + Sync + Default {
    fn on_event(&self, ctx: &PluginContext, event: BuildEvent)
        -> impl Future<Output = Result<(), PluginError>> + Send + '_;

    fn finalize(&self, _ctx: &PluginContext)
        -> impl Future<Output = Result<Vec<u8>, PluginError>> + Send + '_ {
        async { Ok(Vec::new()) }
    }
}
```

**Step 3: Update lib.rs exports**

```rust
pub mod context;
pub mod executor;
pub mod ffi;
pub mod hook;
pub mod output;
pub mod subcommand;

#[doc(hidden)]
pub mod macros;

pub use context::PluginContext;
pub use executor::StepExecutor;
pub use hm_plugin_protocol::*;
pub use hook::LifecycleHook;
pub use output::OutputFormatter;
pub use subcommand::SubcommandPlugin;
```

Remove the old `pub mod host;` and `pub use extism_pdk;`.

**Step 4: Verify compilation**

Run: `cargo check -p hm-plugin-sdk`
Expected: compiles (old `host.rs` module removed, macros.rs may have errors — that's Task 3)

**Step 5: Commit**

```bash
git add crates/hm-plugin-sdk/src/
git commit -m "feat(sdk): async user-facing traits + PluginContext"
```

---

## Task 3: Create hm-plugin-macros proc-macro crate + hm_plugin! macro

**Files:**
- Create: `crates/hm-plugin-macros/Cargo.toml`
- Create: `crates/hm-plugin-macros/src/lib.rs`
- Modify: `crates/hm-plugin-sdk/Cargo.toml` (add dep on hm-plugin-macros)
- Modify: `crates/hm-plugin-sdk/src/macros.rs` (re-export proc macro)
- Modify: `Cargo.toml` (workspace members)

**Why proc macro:** The macro must parse keyword args (`manifest = ..., executor = T, hook = U`), accumulate which capabilities are registered, and emit one cohesive `impl RawPlugin` block with 6 methods — registered ones delegate with borsh ser/de + DynFuture wrapping, unregistered ones return error stubs. Declarative macros can't accumulate state across recursive arms without painful token-tree gymnastics.

**Step 1: Create proc-macro crate**

`crates/hm-plugin-macros/Cargo.toml`:
```toml
[package]
name = "hm-plugin-macros"
version = "0.0.0-dev"
edition.workspace = true

[lib]
proc-macro = true

[dependencies]
syn = { version = "2", features = ["full"] }
quote = "1"
proc-macro2 = "1"
```

Add to workspace `members` list (not `default-members`).

**Step 2: Implement the proc macro**

`crates/hm-plugin-macros/src/lib.rs`:

The proc macro parses the input, identifies which capabilities are registered, and generates:

Key implementation pattern for each capability method:

```rust
// For a registered executor:
fn execute_step<'a>(&'a self, input: $crate::ffi::FfiSlice<'a>)
    -> stabby::future::DynFuture<'a, $crate::ffi::FfiResult>
{
    let ctx = self.ctx.clone();
    ::std::boxed::Box::new(async move {
        let parsed: $crate::ExecutorInput = match ::borsh::from_slice(input.as_ref()) {
            Ok(v) => v,
            Err(e) => return stabby::result::Result::Err(
                ::borsh::to_vec(&$crate::PluginError::new("deserialize", e.to_string()))
                    .unwrap_or_default().into()
            ),
        };
        let plugin = <$ty as ::core::default::Default>::default();
        match $crate::StepExecutor::run(&plugin, &ctx, parsed).await {
            Ok(r) => stabby::result::Result::Ok(
                ::borsh::to_vec(&r).unwrap_or_default().into()
            ),
            Err(e) => stabby::result::Result::Err(
                ::borsh::to_vec(&e).unwrap_or_default().into()
            ),
        }
    }).into()
}

// For an unregistered capability (stub):
fn execute_step<'a>(&'a self, _input: $crate::ffi::FfiSlice<'a>)
    -> stabby::future::DynFuture<'a, $crate::ffi::FfiResult>
{
    ::std::boxed::Box::new(async {
        stabby::result::Result::Err(
            ::borsh::to_vec(&$crate::PluginError::new(
                "not_implemented",
                "this plugin does not implement this capability"
            )).unwrap_or_default().into()
        )
    }).into()
}
```

**Step 3: Wire into SDK**

Add `hm-plugin-macros` as dependency in `crates/hm-plugin-sdk/Cargo.toml`. Re-export the proc macro from `crates/hm-plugin-sdk/src/macros.rs` (or `lib.rs`) so plugin authors use `hm_plugin_sdk::hm_plugin!`. Delete old `register_plugin!` and `__rp_dispatch!` macros.

**Step 4: Verify macro compiles with a minimal test**

Since the macro generates `#[stabby::export]`, full validation requires building a cdylib. This is tested end-to-end in Task 5 (first real plugin migration). For now, verify:

Run: `cargo check -p hm-plugin-sdk`

**Step 5: Commit**

```bash
git add crates/hm-plugin-macros/ crates/hm-plugin-sdk/src/macros.rs crates/hm-plugin-sdk/Cargo.toml Cargo.toml
git commit -m "feat(sdk): hm_plugin! proc macro for stabby FFI code generation"
```

---

## Task 4: Host-side plugin loading rewrite

**Files:**
- Modify: `crates/hm/Cargo.toml`
- Rewrite: `crates/hm/src/plugin/host.rs`
- Delete: `crates/hm/src/plugin/pool.rs`
- Delete: `crates/hm/src/plugin/embedded.rs`
- Delete: `crates/hm/build.rs`
- Create: `crates/hm/src/plugin/host_api.rs` (replaces `host_fns.rs`)
- Rewrite: `crates/hm/src/plugin/registry.rs`
- Rewrite: `crates/hm/src/plugin/manifest.rs`
- Modify: `crates/hm/src/plugin/paths.rs`
- Modify: `crates/hm/src/plugin/mod.rs`

### Step 1: Update hm Cargo.toml

Add `stabby` + `borsh` dependencies. Remove `extism`:
```toml
[dependencies]
# ... existing deps ...
stabby = { workspace = true }  # replaces extism
borsh  = { workspace = true }  # FFI boundary serialization
hm-plugin-sdk = { workspace = true }  # NEW: host needs SDK for ffi traits
# Remove: extism = { workspace = true }
```

Note: `hm` now depends on `hm-plugin-sdk` for the `RawPlugin`/`RawHostApi` trait definitions. Previously the host only used `hm-plugin-protocol`.

### Step 2: Rewrite host.rs — LoadedPlugin with stabby

Replace `pool.rs` + current `host.rs` with a new `host.rs`:

```rust
use std::path::{Path, PathBuf};
use std::sync::Arc;
use anyhow::{Context, Result};
use hm_plugin_protocol::PluginManifest;
use hm_plugin_sdk::ffi::{RawPlugin, RawPluginDyn, RawHostApi, FfiBytes};
use stabby::libloading::StabbyLibrary;

pub struct LoadedPlugin {
    pub manifest: PluginManifest,
    pub source: Option<PathBuf>,
    _lib: libloading::Library,
    plugin: /* Dyn<'static, Box<()>, vtable!(RawPlugin + Send + Sync)> */,
}

impl LoadedPlugin {
    pub fn load(path: &Path, host_api: /* stabby trait object */) -> Result<Self> {
        let lib = unsafe { libloading::Library::new(path)? };
        // Load hm_load_plugin symbol with stabby type checking
        let create_fn = unsafe {
            lib.get_stabbied::</* fn signature */>(b"hm_load_plugin")?
        };
        let plugin = create_fn(host_api)
            .map_err(|err_bytes| /* borsh::from_slice PluginError from err_bytes */)?;
        let manifest_bytes: FfiBytes = plugin.manifest();
        let manifest: PluginManifest = borsh::from_slice(manifest_bytes.as_ref())?;
        Ok(Self { manifest, source: Some(path.to_owned()), _lib: lib, plugin })
    }

    /// Call a capability. Replaces the old generic `call_capability<I, O>`.
    /// Each capability has its own typed async method.
    pub async fn execute_step(&self, input: &hm_plugin_protocol::ExecutorInput) -> Result<hm_plugin_protocol::StepResult> {
        let in_bytes = borsh::to_vec(input)?;
        let ffi_input: hm_plugin_sdk::ffi::FfiSlice<'_> = in_bytes.as_slice().into();
        let result = self.plugin.execute_step(ffi_input).await;
        match result {
            stabby::result::Result::Ok(bytes) => Ok(borsh::from_slice(bytes.as_ref())?),
            stabby::result::Result::Err(bytes) => {
                let err: hm_plugin_protocol::PluginError = borsh::from_slice(bytes.as_ref())?;
                Err(err.into())
            }
        }
    }

    // Similar methods for on_hook_event, run_subcommand, on_output_event, finalize_output
}
```

Key difference from old `LoadedPlugin`: no `PluginPool`, no semaphore, no instance management. The stabby trait object is `Send + Sync`, so concurrent callers can invoke methods directly.

`#[allow(unsafe_code)]` on this module — `Library::new()` and `get_stabbied` are unsafe.

### Step 3: Implement RawHostApi on host side

Create `crates/hm/src/plugin/host_api.rs`:

```rust
use hm_plugin_sdk::ffi::{FfiBytes, FfiSlice, RawHostApi};

pub struct HostApiImpl {
    // Fields for state the host API needs:
    // - tracing subscriber handle (for log)
    // - KV stores (plugin-scope file paths, build/step in-memory maps)
    // - event bus sender (tokio::sync::broadcast::Sender<BuildEvent>)
    // - cancellation token
    // - archive data
    // - project config path
}

impl RawHostApi for HostApiImpl {
    extern "C" fn log(&self, level: u8, msg: FfiSlice<'_>) {
        // Convert level u8 → tracing::Level, emit via tracing macros
    }

    extern "C" fn kv_get(&self, scope: u8, key: FfiSlice<'_>) -> stabby::option::Option<FfiBytes> {
        // Dispatch on scope:
        // 0 (Plugin) → read from file ~/.config/harmont/state/<plugin>.kv
        // 1 (Build) → read from in-memory BTreeMap
        // 2 (Step) → read from in-memory BTreeMap
    }

    extern "C" fn kv_set(&self, scope: u8, key: FfiSlice<'_>, val: FfiSlice<'_>) {
        // Dispatch on scope, write to appropriate store
    }

    extern "C" fn emit_event(&self, event_borsh: FfiSlice<'_>) {
        // borsh::from_slice → BuildEvent, send on broadcast channel
    }

    extern "C" fn emit_step_log(&self, stream: u8, bytes: FfiSlice<'_>) {
        // Forward to event bus as StepLog event
    }

    extern "C" fn should_cancel(&self) -> bool {
        // Check cancellation token
    }

    extern "C" fn write_stdout(&self, bytes: FfiSlice<'_>) {
        // Write to stdout
    }

    extern "C" fn write_stderr(&self, bytes: FfiSlice<'_>) {
        // Write to stderr
    }

    extern "C" fn archive_read(&self, id_borsh: FfiSlice<'_>, offset: u64, max: u64) -> FfiBytes {
        // borsh::from_slice → ArchiveId, read from archive store
    }

    extern "C" fn archive_total_size(&self, id_borsh: FfiSlice<'_>) -> u64 {
        // borsh::from_slice → ArchiveId, return size
    }

    extern "C" fn fs_read_config(&self, rel_path: FfiSlice<'_>) -> stabby::option::Option<FfiBytes> {
        // Read from project .harmont/ directory
    }
}
```

Port logic from current `host_fns.rs` (1065 lines → ~300 lines, since docker/keyring/socket/tty/loopback/browser functions are removed). Uses `borsh::from_slice`/`borsh::to_vec` for deserializing/serializing event and archive ID parameters.

### Step 4: Rewrite registry.rs

Key changes:
- Discover `*.dylib`/`*.so` files instead of `*.wasm`
- Use `std::env::consts::DLL_EXTENSION` for platform-appropriate extension
- No `HOST_FN_NAMES` validation
- No `allowed_hosts` validation
- Remove `RegistryConfig.embedded` field entirely — no embedded plugins; all plugins discovered from disk
- Remove `pool_sizes` (no pooling needed)
- Discovery paths: `~/.harmont/plugins/` (user + built-in, installed by `install.sh`) and `.harmont/plugins/` (project-local)

### Step 5: Delete embedded.rs and build.rs

No more embedding. Built-in plugins are installed to `~/.harmont/plugins/` by `install.sh`. The `hm` binary discovers them like any other plugin.

- Delete `crates/hm/src/plugin/embedded.rs`
- Delete `crates/hm/build.rs`
- Remove `include = [... "embedded/*.wasm" ...]` from `crates/hm/Cargo.toml`
- Delete `crates/hm/embedded/` directory if it exists

**Dev workflow:** During development, either:
- Run `cargo build -p hm-plugin-docker` etc., then symlink from `target/debug/` into `~/.harmont/plugins/`
- Use `RegistryConfig.extra_paths` in integration tests to point at build output directly

### Step 6: Update mod.rs

```rust
pub mod embedded;
pub mod host;
pub mod host_api;
pub mod install;
pub mod manifest;
pub mod paths;
pub mod registry;
// Removed: pool, host_fns, signal
```

### Step 8: Update paths.rs

Change `.wasm` extension to platform dylib extension in discovery path patterns.

### Step 9: Verify host-side compilation

Run: `cargo check -p harmont-cli`
Expected: compilation errors from callers (scheduler, dispatcher, output_subscriber) that still use old API. Those are updated in Task 5+.

### Step 10: Commit

```bash
git add crates/hm/
git commit -m "feat(host): rewrite plugin loading for stabby native dylibs"
```

---

## Task 5: Migrate output-json plugin (validates full pipeline)

**Why first:** Simplest plugin (47 lines). Validates the entire pipeline: SDK → macro → build → embed → load → call.

**Files:**
- Modify: `crates/hm-plugin-output-json/Cargo.toml`
- Rewrite: `crates/hm-plugin-output-json/src/lib.rs`

### Step 1: Update Cargo.toml

```toml
[package]
name = "hm-plugin-output-json"
# ...

[lib]
crate-type = ["cdylib"]  # native dylib, not WASM

[dependencies]
hm-plugin-sdk      = { workspace = true }
hm-plugin-protocol = { workspace = true }
stabby             = { workspace = true }
borsh              = { workspace = true }
serde              = { workspace = true }
serde_json         = { workspace = true }  # this plugin's output IS JSON — serde_json still needed for its business logic
```

### Step 2: Rewrite lib.rs

```rust
#![allow(unsafe_code)]

use hm_plugin_sdk::*;

#[derive(Default)]
struct Json;

impl OutputFormatter for Json {
    async fn on_event(&self, ctx: &PluginContext, event: BuildEvent) -> Result<(), PluginError> {
        let mut bytes = serde_json::to_vec(&event)
            .map_err(|e| PluginError::new("output_json_serde", e.to_string()))?;
        bytes.push(b'\n');
        ctx.write_stdout(&bytes);
        Ok(())
    }
}

hm_plugin!(
    manifest = PluginManifest {
        api_version: HM_PLUGIN_API_VERSION,
        name: "harmont-output-json".into(),
        version: semver::Version::new(0, 1, 0),
        description: "JSON-lines build output formatter.".into(),
        capabilities: vec![Capability::OutputFormatter(OutputFormatterSpec {
            name: "json".into(),
            mime: "application/x-ndjson".into(),
        })],
        config_schema: None,
    },
    output = Json,
);
```

Key changes from old:
- `async fn on_event` instead of `fn on_event`
- `ctx.write_stdout()` instead of `host::write_stdout()`
- `hm_plugin!` instead of `register_plugin!`
- No `required_host_fns`, no `allowed_hosts`
- No `#![no_main]`
- `crate-type = ["cdylib"]` targets native platform

### Step 3: Build the plugin

Run: `cargo build -p hm-plugin-output-json --release`
Expected: produces `target/release/libhm_plugin_output_json.dylib` (macOS) or `.so` (Linux)

### Step 4: Wire into host + update output_subscriber

Update `crates/hm/src/orchestrator/output_subscriber.rs` to use new `LoadedPlugin` API:
- Replace `plugin.call_capability::<BuildEvent, ()>("hm_output_on_event", &event)` with `plugin.on_output_event(&event).await`
- Replace `plugin.call_capability::<(), Vec<u8>>("hm_output_finalize", &())` with `plugin.finalize_output().await`

### Step 5: Integration test

Write a test that loads the output-json plugin as a dylib and calls `on_output_event` with a test event:
```rust
#[tokio::test]
async fn output_json_plugin_loads_and_formats() {
    let host_api = test_host_api(); // minimal RawHostApi impl for tests
    let path = /* path to built dylib */;
    let plugin = LoadedPlugin::load(&path, host_api).unwrap();
    assert_eq!(plugin.manifest.name, "harmont-output-json");

    let event = BuildEvent::BuildEnd { exit_code: 0, duration_ms: 100 };
    plugin.on_output_event(&event).await.unwrap();
    // Verify stdout received JSON line
}
```

### Step 6: Commit

```bash
git add crates/hm-plugin-output-json/ crates/hm/src/orchestrator/output_subscriber.rs
git commit -m "feat: migrate output-json plugin to stabby native dylib"
```

---

## Task 6: Migrate output-human plugin

**Files:**
- Modify: `crates/hm-plugin-output-human/Cargo.toml`
- Rewrite: `crates/hm-plugin-output-human/src/lib.rs`
- Review: any `render` module this plugin uses

Same pattern as Task 5. Change `crate-type` to `cdylib`, rewrite with `hm_plugin!` macro, use `ctx.write_stdout()`.

### Step 1–4: Mirror Task 5 steps

### Step 5: Commit

```bash
git add crates/hm-plugin-output-human/
git commit -m "feat: migrate output-human plugin to stabby native dylib"
```

---

## Task 7: Migrate docker plugin

**Most complex migration.** Docker plugin currently uses 9 host functions (`hm_docker_*`) that call into the host's bollard client. Post-migration, the plugin uses bollard directly.

**Files:**
- Modify: `crates/hm-plugin-docker/Cargo.toml`
- Rewrite: `crates/hm-plugin-docker/src/lib.rs`
- Delete: `crates/hm-plugin-docker/src/extism_host.rs`
- Create: `crates/hm-plugin-docker/src/docker.rs` (bollard wrapper)
- Modify: `crates/hm-plugin-docker/src/decision.rs` (if needed)
- Modify: `crates/hm-plugin-docker/src/image_name.rs` (if needed)

### Step 1: Update Cargo.toml

```toml
[lib]
crate-type = ["cdylib"]

[dependencies]
hm-plugin-sdk      = { workspace = true }
hm-plugin-protocol = { workspace = true }
stabby             = { workspace = true }
borsh              = { workspace = true }
serde              = { workspace = true }
serde_json         = { workspace = true }
bollard            = "0.18"     # Direct docker API
tokio              = { workspace = true }  # Shared runtime
```

### Step 2: Create docker.rs — bollard wrapper

Port the docker operations from `crates/hm/src/plugin/host_fns.rs` (the `docker_host_fns::*_impl` async functions) and `crates/hm/src/orchestrator/docker_client.rs` into the plugin:

```rust
use bollard::Docker;

pub(crate) struct DockerClient {
    docker: Docker,
}

impl DockerClient {
    pub fn connect() -> Result<Self, PluginError> {
        let docker = Docker::connect_with_local_defaults()
            .map_err(|e| PluginError::new("docker_connect", e.to_string()))?;
        Ok(Self { docker })
    }

    pub async fn image_exists(&self, tag: &str) -> bool { ... }
    pub async fn pull(&self, tag: &str) -> Result<(), PluginError> { ... }
    pub async fn start_container(&self, args: DockerStartArgs) -> Result<String, PluginError> { ... }
    pub async fn extract_workspace(&self, args: DockerExtractArgs) -> Result<(), PluginError> { ... }
    pub async fn exec(&self, args: DockerExecArgs) -> Result<i32, PluginError> { ... }
    pub async fn commit(&self, args: DockerCommitArgs) -> Result<(), PluginError> { ... }
    pub async fn stop_remove(&self, container_id: &str) { ... }
}
```

Move `DockerStartArgs`, `DockerExecArgs`, `DockerCommitArgs`, `DockerExtractArgs` from `hm-plugin-protocol/src/host_abi.rs` into the docker plugin crate (they're now internal types).

### Step 3: Rewrite lib.rs with async

```rust
#![allow(unsafe_code)]

use hm_plugin_sdk::*;

mod decision;
mod docker;
mod image_name;

#[derive(Default)]
struct DockerExec;

impl StepExecutor for DockerExec {
    async fn run(&self, ctx: &PluginContext, input: ExecutorInput) -> Result<StepResult, PluginError> {
        let client = docker::DockerClient::connect()?;
        run_step(&client, ctx, input).await
    }
}

async fn run_step(
    client: &docker::DockerClient,
    ctx: &PluginContext,
    input: ExecutorInput,
) -> Result<StepResult, PluginError> {
    // Same logic as current run_step(), but:
    // - Use client.* instead of host::*
    // - All operations are async
    // - ctx.log() for logging
    // - ctx.should_cancel() for cancellation checks
    // ...
}
```

### Step 4: Update host-side scheduler

Update `crates/hm/src/orchestrator/scheduler.rs`:
- Replace `plugin.call_capability::<ExecutorInput, StepResult>("hm_executor_run", &input)` with `plugin.execute_step(&input).await`
- Remove docker host function setup
- Remove bollard from host's direct dependencies (it's now in the docker plugin)

### Step 5: Test docker plugin loads and executes

Integration test with docker (requires Docker daemon running):
```rust
#[tokio::test]
#[cfg(feature = "docker-integration")]
async fn docker_plugin_runs_step() {
    // Load plugin, create minimal ExecutorInput, execute, verify StepResult
}
```

### Step 6: Commit

```bash
git add crates/hm-plugin-docker/ crates/hm/src/orchestrator/scheduler.rs
git commit -m "feat: migrate docker plugin to stabby — uses bollard directly"
```

---

## Task 8: Migrate cloud plugin

**Files:**
- Modify: `crates/hm-plugin-cloud/Cargo.toml`
- Rewrite: `crates/hm-plugin-cloud/src/lib.rs`
- Rewrite: `crates/hm-plugin-cloud/src/http.rs` (use reqwest)
- Rewrite: `crates/hm-plugin-cloud/src/creds.rs` (use keyring crate or file-based)
- Rewrite: `crates/hm-plugin-cloud/src/auth.rs` (use axum for OAuth loopback)
- Modify: other internal modules as needed

### Step 1: Update Cargo.toml

```toml
[lib]
crate-type = ["cdylib"]

[dependencies]
hm-plugin-sdk      = { workspace = true }
hm-plugin-protocol = { workspace = true }
stabby             = { workspace = true }
borsh              = { workspace = true }
serde              = { workspace = true }
serde_json         = { workspace = true }
tokio              = { workspace = true }
reqwest            = { version = "0.13", features = ["rustls", "json"] }  # replaces extism HTTP
# Optional: for credential storage
# keyring = "2"  (or keep file-based as current)
# For OAuth loopback:
axum               = { version = "0.7", default-features = false, features = ["tokio", "http1", "query"] }
webbrowser         = "1"
dialoguer          = "0.11"  # For TTY prompts
```

### Step 2: Rewrite http.rs — use reqwest

Replace extism-pdk HTTP with reqwest:
```rust
use reqwest::Client as ReqwestClient;

pub(crate) struct Client {
    inner: ReqwestClient,
    base: String,
    token: Option<String>,
}

impl Client {
    pub(crate) async fn get<O: DeserializeOwned>(&self, path: &str) -> Result<O, PluginError> {
        let resp = self.inner.get(format!("{}{}", self.base, path))
            .bearer_auth(self.token.as_deref().unwrap_or(""))
            .send().await
            .map_err(|e| PluginError::new("cloud_http_request", e.to_string()))?;
        // ... handle response
    }
    // ... post, delete
}
```

### Step 3: Rewrite creds.rs — direct file/keyring access

Replace `host::keyring_get/set/delete` with direct file operations (current implementation stores at `~/.harmont/credentials.toml`).

### Step 4: Rewrite auth.rs — direct OAuth loopback

Replace `host::spawn_loopback/loopback_recv` with direct axum server + `webbrowser::open`. Replace `host::tty_prompt/tty_confirm` with `dialoguer`.

### Step 5: Rewrite lib.rs

```rust
impl SubcommandPlugin for Cloud {
    async fn run(&self, ctx: &PluginContext, input: SubcommandInput) -> Result<ExitInfo, PluginError> {
        cli::dispatch(input.verb_path, input.env).await
    }
}
```

Note: `cli::dispatch` becomes async since HTTP calls and OAuth flow are now async.

### Step 6: Update dispatcher.rs

Update `crates/hm/src/dispatcher.rs`:
- Replace `plugin.call_capability::<SubcommandInput, ExitInfo>("hm_subcommand_run", &input)` with `plugin.run_subcommand(&input).await`

### Step 7: Commit

```bash
git add crates/hm-plugin-cloud/ crates/hm/src/dispatcher.rs
git commit -m "feat: migrate cloud plugin to stabby — uses reqwest/axum directly"
```

---

## Task 9: Migrate test fixtures

Each fixture becomes its own cdylib crate under `tests/fixtures/`. The old `crates/hm-fixtures/` crate is deleted. Each sub-crate is a workspace member (not in `default-members`).

```
tests/fixtures/
├── noop-executor/
│   ├── Cargo.toml      (cdylib, name = "hm-fixture-noop-executor")
│   └── src/lib.rs
├── recording-hook/
│   ├── Cargo.toml
│   └── src/lib.rs
├── failing-subcommand/
│   ├── Cargo.toml
│   └── src/lib.rs
├── host-fn-probe/
│   ├── Cargo.toml
│   └── src/lib.rs
├── bad-api-version/
│   ├── Cargo.toml
│   └── src/lib.rs
└── freestyle-runner/
    ├── Cargo.toml
    └── src/lib.rs
```

Update workspace root `Cargo.toml`: remove `crates/hm-fixtures` from `members`, add `tests/fixtures/*` entries.

### Step 1: Create `tests/fixtures/` structure + delete `crates/hm-fixtures/`

Create per-fixture crate directories under `tests/fixtures/`. Each has:
- `Cargo.toml` with `crate-type = ["cdylib"]`, name prefixed `hm-fixture-`
- `src/lib.rs` using `hm_plugin!` macro

Delete `crates/hm-fixtures/` entirely. Update workspace `Cargo.toml` members list.

### Step 2: Port each fixture

Each fixture is small (20-80 lines). Port pattern:
- Remove `#![no_main]`
- Replace `register_plugin!` with `hm_plugin!`
- Replace `host::*` calls with `ctx.*` calls
- Add `async` to trait methods

### Step 3: Update test infrastructure

Modify `crates/hm/tests/common/fixtures.rs`:
- Change `ensure_built()` to compile cdylib crates from `tests/fixtures/<name>` instead of wasm32-wasip1 bins
- Change `fixture_path(name)` to return path to `target/debug/libhm_fixture_<name>.{dylib,so}` instead of `.wasm`

### Step 4: Update integration tests

Modify tests in `crates/hm/tests/`:
- Verify fixture plugins load and run correctly through the new host API

### Step 5: Commit

```bash
git rm -r crates/hm-fixtures/
git add tests/fixtures/ crates/hm/tests/ Cargo.toml
git commit -m "feat: migrate test fixtures to tests/fixtures/ as stabby native dylibs"
```

---

## Task 10: Protocol crate cleanup

**Files:**
- Modify: `crates/hm-plugin-protocol/src/host_abi.rs`
- Modify: `crates/hm-plugin-protocol/src/manifest.rs`
- Modify: `crates/hm-plugin-protocol/src/lib.rs`

### Step 1: Slim down host_abi.rs

Keep only:
- `Level` enum
- `KvScope` enum
- `ArchiveReadArgs` struct

Move to docker plugin crate:
- `DockerStartArgs`, `DockerExecArgs`, `DockerCommitArgs`, `DockerExtractArgs`

Delete:
- `SocketHandle`, `SocketReadArgs`, `SocketWriteArgs`
- `LoopbackHandle`, `LoopbackRecvArgs`, `CallbackData`
- `KeyringArgs`, `KeyringSetArgs`
- `TtyPromptArgs`, `TtyConfirmArgs`

### Step 2: Update PluginManifest

Remove fields:
```rust
pub struct PluginManifest {
    pub api_version: u32,
    pub name: String,
    pub version: semver::Version,
    pub description: String,
    pub capabilities: Vec<Capability>,
    // REMOVED: pub required_host_fns: Vec<String>,
    pub config_schema: Option<JsonSchema>,
    // REMOVED: pub allowed_hosts: Vec<String>,
}
```

Bump `HM_PLUGIN_API_VERSION` to `2` (wire format changed).

### Step 3: Update lib.rs re-exports

Remove re-exports for deleted types.

### Step 4: Fix compilation cascade

Removing fields from `PluginManifest` and types from `host_abi.rs` will cause compilation errors in:
- All `hm_plugin!` macro invocations (remove `required_host_fns` and `allowed_hosts`)
- `crates/hm/src/plugin/manifest.rs` validation (remove host_fn validation)
- `crates/hm/src/plugin/registry.rs` (remove `HOST_FN_NAMES`)
- Any test that constructs `PluginManifest`

Fix all of these.

### Step 5: Verify full workspace compiles

Run: `cargo check --workspace`

### Step 6: Commit

```bash
git add crates/hm-plugin-protocol/ crates/hm/ crates/hm-plugin-*/
git commit -m "refactor(protocol): remove WASM-era host_abi types + manifest fields"
```

---

## Task 11: Final cleanup

**Files:**
- Modify: `Cargo.toml` (workspace root)
- Delete: `crates/hm/src/plugin/pool.rs`
- Delete: `crates/hm/src/plugin/host_fns.rs`
- Delete: `crates/hm/src/plugin/signal.rs` (if exists and unused)
- Delete: `crates/hm-plugin-sdk/src/host.rs` (old extism host wrappers)
- Modify: `crates/hm/Cargo.toml`
- Modify: `CLAUDE.md`
- Modify: `crates/hm/CLAUDE.md`

### Step 1: Remove extism workspace dependencies

From `Cargo.toml`:
```toml
# DELETE these lines:
extism      = "1"
extism-pdk  = "1"
```

### Step 2: Remove extism from hm Cargo.toml

Remove `extism = { workspace = true }` from `[dependencies]`.

### Step 3: Remove bollard from hm Cargo.toml (if docker plugin now owns it)

The `hm` binary no longer needs `bollard` since docker operations moved to the plugin. Verify no other code uses it, then remove.

Also remove `axum` and `webbrowser` from `hm`'s deps if they were only used for cloud plugin host functions.

### Step 4: Delete dead files

- `crates/hm/src/plugin/pool.rs`
- `crates/hm/src/plugin/host_fns.rs`
- `crates/hm/src/plugin/signal.rs` (verify unused first)
- `crates/hm-plugin-sdk/src/host.rs`
- `crates/hm/embedded/*.wasm` (if any exist)

### Step 5: Remove wasm32-wasip1 references

Search workspace for `wasm32-wasip1` and remove all references:
```bash
grep -r "wasm32-wasip1" --include="*.rs" --include="*.toml" --include="*.md" --include="*.yml" --include="*.yaml"
```

### Step 6: Update CLAUDE.md files

Update `/CLAUDE.md`:
```
- Remove mention of wasm32-wasip1 target
- Update crate descriptions to mention stabby
- Note: plugins are native dylibs, not WASM
```

Update `/crates/hm/CLAUDE.md`:
```
- Update plugin parallelism section (no more PluginPool)
- Update cloud functionality section (no more extism HTTP, uses reqwest)
- Remove mention of extism host functions
```

### Step 7: Update workspace include list

In `crates/hm/Cargo.toml`, change:
```toml
include = [
    "src/**/*",
    "build.rs",
    "Cargo.toml",
    "README.md",
    "embedded/*.dylib",  # or platform-specific pattern
    "embedded/*.so",
]
```

### Step 8: Full workspace build + test

Run:
```bash
cargo build --workspace
cargo test --workspace
```

Expected: everything compiles and tests pass.

### Step 9: Commit

```bash
git add -A
git commit -m "chore: remove extism, dead WASM code, update docs"
```

---

## Risk register

| Risk | Mitigation |
|------|-----------|
| stabby trait with DynFuture may not compile as expected | Task 1 validates this immediately. **STOP and ask the operator** before choosing a fallback — options include manual vtable, separate async entry points, or sync traits with `block_on`. Do not pick an alternative unilaterally. |
| `unsafe_code = "deny"` workspace lint blocks stabby macros | Add `#[allow(unsafe_code)]` on specific modules that use stabby FFI |
| hm_plugin! macro complexity | Use proc-macro crate `hm-plugin-macros` from the start — the macro must accumulate state across keyword args to emit one cohesive `impl RawPlugin` block, which is painful with declarative macros |
| Built-in plugin availability | `install.sh` places dylibs in `~/.harmont/plugins/`. No embedding. Dev workflow: `cargo build` + symlink or `--extra-paths` in tests |
| Docker container cleanup on host crash | Entirely plugin-side: docker plugin uses bollard directly and owns full container lifecycle (`--rm`, labels, cleanup-on-drop). Core binary has no docker awareness. |
| Rust 1.78+ stabby vtable perf regression | Only ~5 trait object types total; unlikely to hit O(n) scaling issues |
| Cross-platform dylib extension in build.rs/embedded.rs | Use `std::env::consts::DLL_EXTENSION`/`DLL_PREFIX` consistently |
| Cloud plugin's internal modules (1500 lines) need async rewrite | Most changes are mechanical: add `.await`, replace extism HTTP with reqwest |

## Dependency changes summary

### Added
- `stabby = "=72.1.1"` (workspace)
- `borsh = "1"` (workspace; FFI boundary serialization — faster/smaller than JSON)
- `bollard = "0.18"` (docker plugin; moved from hm binary)
- `reqwest` (cloud plugin)
- `axum` (cloud plugin; moved from hm binary)
- `webbrowser` (cloud plugin; moved from hm binary)
- `dialoguer` (cloud plugin; moved from hm binary)

### Removed
- `extism = "1"` (workspace)
- `extism-pdk = "1"` (workspace)
- `bollard` from hm binary (moved to docker plugin)
- `axum` from hm binary (moved to cloud plugin)
- `webbrowser` from hm binary (moved to cloud plugin)

### Unchanged
- `hm-plugin-protocol` — wire types, serde structs
- `tokio`, `serde`, `serde_json`, `clap`, etc.
