# Stabby FFI Types Implementation Plan

> **For Claude:** Execute this plan task-by-task.

**Goal:** Replace serde_json serialization at the plugin FFI boundary with native `#[stabby::stabby]` types. Every capability call (execute_step, on_hook_event, run_subcommand) and the manifest export should pass typed structs directly across the ABI instead of serializing to JSON bytes.

**Architecture:** The protocol crate's FFI modules (`executor.rs`, `subcommand.rs`, `error.rs`, `hook.rs`, `manifest.rs`, `host_abi.rs`) switch from `#[derive(Serialize, Deserialize)]` to `#[stabby::stabby]`. Host-internal types (`ir.rs` for Pipeline IR, `events.rs` for BuildEvent) stay serde — they never cross the FFI boundary. A new `value.rs` module provides `FfiValue`, a stabby-compatible dynamic value enum replacing `serde_json::Value`. The SDK crate provides a wrapper layer so plugin authors see std Rust types (String, Vec, BTreeMap) and never touch stabby types directly. The `RawPlugin` trait changes from byte-slice signatures to typed stabby signatures. The `hm_plugin!` macro drops serde_json entirely.

**Tech Stack:** stabby v72.1.1 (`#[stabby::stabby]`, `#[repr(u8)]` for matchable enums, `stabby::string::String`, `stabby::vec::Vec`, `stabby::option::Option`, `stabby::collections::arc_btree::ArcBTreeMap`).

**Conversion boundaries:**
- `ir::CommandStep` (serde) → `executor::CommandStep` (stabby): in orchestrator when building ExecutorInput
- `events::BuildEvent` (serde) → `hook::FfiBuildEvent` (stabby): in hook dispatcher
- `serde_json::Value` → `FfiValue`: in host when building SubcommandInput and ExecutorInput
- SDK types (std) ↔ protocol types (stabby): in macro-generated code

---

### Task 1: Add stabby dependency to protocol crate and create FfiValue

The protocol crate currently has no stabby dependency. FfiValue is the foundation type that all other FFI types build on — it replaces `serde_json::Value` for dynamic data.

**Files:**
- Modify: `crates/hm-plugin-protocol/Cargo.toml`
- Create: `crates/hm-plugin-protocol/src/value.rs`
- Modify: `crates/hm-plugin-protocol/src/lib.rs`

**Step 1: Add stabby dependency**

In `crates/hm-plugin-protocol/Cargo.toml`, add to `[dependencies]`:
```toml
stabby = { workspace = true }
```

Note: the protocol crate has `#![forbid(unsafe_code)]`. The `#[stabby::stabby]` derive macro generates safe code on the user side — the unsafe lives inside stabby's internals. This should compile without changing the forbid. If it doesn't, change to `#![deny(unsafe_code)]` with a crate-level `#[allow(unsafe_code)]` on the stabby-derived types only.

**Step 2: Create `value.rs`**

```rust
//! ABI-stable dynamic value type, replacing `serde_json::Value` at
//! the plugin FFI boundary.

use stabby::collections::arc_btree::ArcBTreeMap;

/// Dynamic value type for data whose schema is not known at compile
/// time: parsed CLI args, `runner_args`, JSON Schema fragments.
///
/// All variants use stabby-stable types. Use `#[repr(u8)]` so
/// standard Rust `match` works (no `.match_ref()` closures).
#[stabby::stabby]
#[repr(u8)]
pub enum FfiValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(stabby::string::String),
    Array(stabby::vec::Vec<FfiValue>),
    Object(ArcBTreeMap<stabby::string::String, FfiValue>),
}
```

**Step 3: Add to lib.rs**

Add `pub mod value;` and `pub use value::FfiValue;` to `lib.rs`.

**Step 4: Verify it compiles**

Run: `cargo check -p hm-plugin-protocol`
Expected: PASS. Watch for:
- `forbid(unsafe_code)` conflict — fix as described in step 1
- Recursive type sizing issues — FfiValue references itself through Vec and ArcBTreeMap (both heap-allocated, fixed-size pointers). Should be fine.

**Step 5: Commit**

```
git add crates/hm-plugin-protocol/
git commit -m "feat(protocol): add stabby dep and FfiValue dynamic type"
```

---

### Task 2: Rewrite protocol FFI types to stabby

Convert all types that cross the plugin FFI boundary from serde to stabby. Keep `ir.rs` and `events.rs` untouched (host-internal). The old serde types in `executor.rs`, `hook.rs`, etc. are replaced wholesale.

**Files:**
- Rewrite: `crates/hm-plugin-protocol/src/executor.rs`
- Rewrite: `crates/hm-plugin-protocol/src/subcommand.rs`
- Rewrite: `crates/hm-plugin-protocol/src/error.rs`
- Rewrite: `crates/hm-plugin-protocol/src/hook.rs`
- Rewrite: `crates/hm-plugin-protocol/src/manifest.rs`
- Rewrite: `crates/hm-plugin-protocol/src/host_abi.rs`
- Modify: `crates/hm-plugin-protocol/src/lib.rs`

**Conventions for all types:**

- Use `#[stabby::stabby]` on all structs
- Use `#[stabby::stabby] #[repr(u8)]` on all enums (enables standard `match`)
- `String` → `stabby::string::String`
- `Vec<T>` → `stabby::vec::Vec<T>`
- `Option<T>` → `stabby::option::Option<T>` (note: must use stabby Option for ABI stability in struct fields)
- `BTreeMap<K,V>` → `stabby::collections::arc_btree::ArcBTreeMap<stabby::string::String, V>`
- `Uuid` → `stabby::string::String` (string representation)
- `DateTime<Utc>` → `stabby::string::String` (ISO 8601)
- `semver::Version` → `stabby::string::String`
- `serde_json::Value` → `FfiValue`
- Drop all `serde`, `schemars`, `chrono`, `uuid`, `semver` derives/imports from rewritten modules
- Keep `thiserror` on `ManifestError` (host-only error, doesn't cross FFI)

**Step 1: Rewrite `error.rs`** (simplest, no deps on other FFI types)

```rust
//! Error and exit-info types returned by plugin capability exports.

/// Returned by a subcommand plugin. The host translates `exit_code`
/// into the process exit code.
#[stabby::stabby]
pub struct ExitInfo {
    pub exit_code: i32,
    pub message: stabby::option::Option<stabby::string::String>,
}

/// Error returned from any capability export.
#[stabby::stabby]
pub struct PluginError {
    pub code: stabby::string::String,
    pub message: stabby::string::String,
    pub doc_url: stabby::option::Option<stabby::string::String>,
}
```

Note: `PluginError` currently derives `thiserror::Error` and has `impl PluginError { new(), with_doc() }`. These methods take `impl Into<String>` — change to `impl Into<stabby::string::String>` or accept `&str` and convert. The `thiserror::Error` derive won't work on stabby types (requires `Display` which stabby String does impl). Check if thiserror works; if not, implement `Display` and `Error` manually.

**Step 2: Rewrite `host_abi.rs`**

```rust
//! Wire types for host-function arguments and return values.

#[stabby::stabby]
#[repr(u8)]
pub enum Level {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

#[stabby::stabby]
#[repr(u8)]
pub enum KvScope {
    Plugin,
    Build,
    Step,
}

#[stabby::stabby]
pub struct ArchiveReadArgs {
    pub id: stabby::string::String,
    pub offset: u64,
    pub max: u64,
}
```

**Step 3: Rewrite `executor.rs`**

This module no longer imports from `ir.rs`. It defines its own `CommandStep` (trimmed — no `label`, `builds_in`, `cache` fields).

```rust
//! Wire types passed to and returned by step-executor plugins.

use crate::value::FfiValue;

#[stabby::stabby]
pub struct CommandStep {
    pub key: stabby::string::String,
    pub cmd: stabby::string::String,
    pub image: stabby::option::Option<stabby::string::String>,
    pub env: stabby::option::Option<
        stabby::collections::arc_btree::ArcBTreeMap<
            stabby::string::String,
            stabby::string::String,
        >,
    >,
    pub timeout_seconds: stabby::option::Option<u32>,
    pub runner: stabby::option::Option<stabby::string::String>,
    pub runner_args: stabby::option::Option<FfiValue>,
}

#[stabby::stabby]
#[repr(u8)]
pub enum CacheDecision {
    Hit { tag: stabby::string::String },
    MissBuildAs { tag: stabby::string::String },
    MissNoCommit,
}

#[stabby::stabby]
pub struct ExecutorInput {
    pub step: CommandStep,
    pub workspace_archive_id: stabby::string::String,
    pub env: stabby::collections::arc_btree::ArcBTreeMap<
        stabby::string::String,
        stabby::string::String,
    >,
    pub workdir: stabby::string::String,
    pub run_id: stabby::string::String,
    pub step_id: stabby::string::String,
    pub cache_lookup: CacheDecision,
    pub parent_snapshot: stabby::option::Option<stabby::string::String>,
}

#[stabby::stabby]
pub struct ArtifactRef {
    pub key: stabby::string::String,
    pub mime: stabby::string::String,
    pub size_bytes: u64,
}

#[stabby::stabby]
pub struct StepResult {
    pub exit_code: i32,
    pub committed_snapshot: stabby::option::Option<stabby::string::String>,
    pub artifacts: stabby::vec::Vec<ArtifactRef>,
}
```

Note: `ArchiveId` and `SnapshotRef` newtypes are replaced by plain `stabby::string::String`. The wrapper types added no runtime value — they were for documentation purposes. If keeping them is desired, they'd need `#[stabby::stabby]` wrappers, which is awkward for single-field structs. Use plain strings and document the semantics via field names.

**Step 4: Rewrite `subcommand.rs`**

```rust
//! Wire type for subcommand invocations.

use crate::value::FfiValue;

#[stabby::stabby]
pub struct SubcommandInput {
    pub verb_path: stabby::vec::Vec<stabby::string::String>,
    pub args: FfiValue,
    pub env: stabby::collections::arc_btree::ArcBTreeMap<
        stabby::string::String,
        stabby::string::String,
    >,
}
```

**Step 5: Rewrite `hook.rs`**

This no longer imports `BuildEvent` from `events.rs`. It defines its own `FfiBuildEvent` that mirrors the variants with stabby types.

```rust
//! Lifecycle hook wire types.

#[stabby::stabby]
#[repr(u8)]
pub enum HookPhase {
    Before,
    After,
}

#[stabby::stabby]
#[repr(u8)]
pub enum HookEventKind {
    BuildStart,
    StepQueued,
    StepStart,
    StepLog,
    StepCacheHit,
    StepEnd,
    ChainFailed,
    BuildEnd,
}

/// Stabby-safe mirror of `events::PlanSummary`.
#[stabby::stabby]
pub struct FfiPlanSummary {
    pub step_count: u64,
    pub chain_count: u64,
    pub default_runner: stabby::string::String,
}

/// Stabby-safe mirror of `events::StdStream`.
#[stabby::stabby]
#[repr(u8)]
pub enum FfiStdStream {
    Stdout,
    Stderr,
}

/// Stabby-safe mirror of `events::BuildEvent`. All Uuid/DateTime
/// fields are string-encoded.
#[stabby::stabby]
#[repr(u8)]
pub enum FfiBuildEvent {
    BuildStart {
        run_id: stabby::string::String,
        plan: FfiPlanSummary,
        started_at: stabby::string::String,
    },
    StepQueued {
        step_id: stabby::string::String,
        key: stabby::string::String,
        chain_idx: u64,
    },
    StepStart {
        step_id: stabby::string::String,
        runner: stabby::string::String,
        image: stabby::option::Option<stabby::string::String>,
    },
    StepLog {
        step_id: stabby::string::String,
        stream: FfiStdStream,
        line: stabby::string::String,
        ts: stabby::string::String,
    },
    StepCacheHit {
        step_id: stabby::string::String,
        key: stabby::string::String,
        tag: stabby::string::String,
    },
    StepEnd {
        step_id: stabby::string::String,
        exit_code: i32,
        duration_ms: u64,
        snapshot: stabby::option::Option<stabby::string::String>,
    },
    ChainFailed {
        chain_idx: u64,
        failed_step_id: stabby::string::String,
        failed_step_key: stabby::string::String,
        exit_code: i32,
        message: stabby::string::String,
        ts: stabby::string::String,
    },
    BuildEnd {
        exit_code: i32,
        duration_ms: u64,
    },
}

#[stabby::stabby]
pub struct HookEvent {
    pub event: FfiBuildEvent,
    pub phase: HookPhase,
}

#[stabby::stabby]
#[repr(u8)]
pub enum HookOutcome {
    Continue,
    Abort { reason: stabby::string::String },
}
```

**Step 6: Rewrite `manifest.rs`**

```rust
//! Plugin manifest types.

use crate::hook::{HookEventKind, HookPhase};
use crate::value::FfiValue;

#[stabby::stabby]
#[repr(u8)]
pub enum ValueType {
    String,
    Int,
    Bool,
}

#[stabby::stabby]
#[repr(u8)]
pub enum ArgSpec {
    Positional {
        name: stabby::string::String,
        help: stabby::option::Option<stabby::string::String>,
        required: bool,
        value_type: ValueType,
    },
    Option {
        long: stabby::string::String,
        short: stabby::option::Option<u32>,
        help: stabby::option::Option<stabby::string::String>,
        required: bool,
        value_type: ValueType,
        default: stabby::option::Option<stabby::string::String>,
    },
    Flag {
        long: stabby::string::String,
        short: stabby::option::Option<u32>,
        help: stabby::option::Option<stabby::string::String>,
    },
}

Note on `short` field: was `Option<char>`. `char` is 4 bytes and should work with stabby. If stabby doesn't support `char` in ABI-stable position, use `u32` and convert. Check at compile time.

#[stabby::stabby]
pub struct SubcommandSpec {
    pub verb: stabby::string::String,
    pub about: stabby::string::String,
    pub args: stabby::vec::Vec<ArgSpec>,
    pub subcommands: stabby::vec::Vec<SubcommandSpec>,
}

#[stabby::stabby]
pub struct StepExecutorSpec {
    pub runner: stabby::string::String,
    pub default: bool,
    pub step_schema: stabby::option::Option<FfiValue>,
}

#[stabby::stabby]
pub struct LifecycleHookSpec {
    pub events: stabby::vec::Vec<HookEventKind>,
    pub phase: HookPhase,
    pub timeout_ms: u32,
}

#[stabby::stabby]
#[repr(u8)]
pub enum Capability {
    Subcommand(SubcommandSpec),
    StepExecutor(StepExecutorSpec),
    LifecycleHook(LifecycleHookSpec),
}

#[stabby::stabby]
pub struct PluginManifest {
    pub api_version: u32,
    pub name: stabby::string::String,
    pub version: stabby::string::String,
    pub description: stabby::string::String,
    pub capabilities: stabby::vec::Vec<Capability>,
    pub config_schema: stabby::option::Option<FfiValue>,
}
```

Move `ManifestError` and `PluginManifest::validate()` to a separate file or keep in manifest.rs — they use `&str` comparisons which work fine on `stabby::string::String` via Deref. `ManifestError` stays as a regular Rust enum with `thiserror` (it's a host-side error, never crosses FFI).

**Step 7: Update `lib.rs` re-exports**

Update the pub-use block. Key changes:
- `executor.rs` no longer exports `ArchiveId` or `SnapshotRef` (collapsed to strings)
- `hook.rs` exports new `Ffi`-prefixed build event types alongside `HookEvent`/`HookOutcome`
- `value.rs` exports `FfiValue`
- `ir.rs` and `events.rs` exports unchanged

**Step 8: Verify**

Run: `cargo check -p hm-plugin-protocol`
Expected: PASS for the protocol crate itself. Downstream crates will break (expected — they still reference old types).

**Step 9: Commit**

```
git add crates/hm-plugin-protocol/
git commit -m "feat(protocol): rewrite FFI types to stabby ABI-stable structs"
```

---

### Task 3: Conversion functions — ir/events → FFI types

The host needs to convert between serde types (ir, events) and stabby FFI types when constructing inputs for plugins. These converters live in `hm-plugin-runtime` (the host crate).

**Files:**
- Create: `crates/hm-plugin-runtime/src/convert.rs`
- Modify: `crates/hm-plugin-runtime/src/lib.rs`

**Step 1: Create `convert.rs`**

```rust
//! Conversions from host-internal serde types to stabby FFI types.

use hm_plugin_protocol as ffi;

/// Convert `ir::CommandStep` (serde) to `ffi::CommandStep` (stabby).
pub fn command_step(ir: &crate::ir::CommandStep) -> ffi::CommandStep {
    ffi::CommandStep {
        key: ir.key.as_str().into(),
        cmd: ir.cmd.as_str().into(),
        image: ir.image.as_deref().map(Into::into).into(),
        env: ir.env.as_ref().map(|m| {
            m.iter()
                .map(|(k, v)| (k.as_str().into(), v.as_str().into()))
                .collect()
        }).into(),
        timeout_seconds: ir.timeout_seconds.into(),
        runner: ir.runner.as_deref().map(Into::into).into(),
        runner_args: ir.runner_args.as_ref().map(json_to_ffi).into(),
    }
}

/// Convert `serde_json::Value` to `ffi::FfiValue`.
pub fn json_to_ffi(v: &serde_json::Value) -> ffi::FfiValue {
    match v {
        serde_json::Value::Null => ffi::FfiValue::Null,
        serde_json::Value::Bool(b) => ffi::FfiValue::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                ffi::FfiValue::Int(i)
            } else {
                ffi::FfiValue::Float(n.as_f64().unwrap_or(0.0))
            }
        }
        serde_json::Value::String(s) => ffi::FfiValue::Str(s.as_str().into()),
        serde_json::Value::Array(arr) => {
            ffi::FfiValue::Array(arr.iter().map(json_to_ffi).collect())
        }
        serde_json::Value::Object(obj) => {
            ffi::FfiValue::Object(
                obj.iter()
                    .map(|(k, v)| (k.as_str().into(), json_to_ffi(v)))
                    .collect(),
            )
        }
    }
}

/// Convert `events::BuildEvent` (serde) to `hook::FfiBuildEvent` (stabby).
pub fn build_event(ev: &crate::events::BuildEvent) -> ffi::hook::FfiBuildEvent {
    use crate::events::BuildEvent;
    match ev {
        BuildEvent::BuildStart { run_id, plan, started_at } => {
            ffi::hook::FfiBuildEvent::BuildStart {
                run_id: run_id.to_string().as_str().into(),
                plan: ffi::hook::FfiPlanSummary {
                    step_count: plan.step_count as u64,
                    chain_count: plan.chain_count as u64,
                    default_runner: plan.default_runner.as_str().into(),
                },
                started_at: started_at.to_rfc3339().as_str().into(),
            }
        }
        // ... mirror all 7 variants
        // Each variant maps field-by-field:
        // - Uuid → .to_string().as_str().into()
        // - String → .as_str().into()
        // - usize → as u64
        // - DateTime → .to_rfc3339().as_str().into()
        // - Option<T> → .as_ref().map(convert).into()
    }
}
```

Note: ArcBTreeMap is `FromIterator` — `.collect()` on an iterator of `(K, V)` tuples should work. Verify at compile time.

**Step 2: Wire up in lib.rs**

Add `pub mod convert;` to `crates/hm-plugin-runtime/src/lib.rs`.

**Step 3: Verify**

Run: `cargo check -p hm-plugin-runtime`
Expected: May fail due to downstream changes needed. Get this module compiling in isolation first.

**Step 4: Commit**

```
git add crates/hm-plugin-runtime/src/convert.rs crates/hm-plugin-runtime/src/lib.rs
git commit -m "feat(runtime): add ir/events → stabby FFI conversion functions"
```

---

### Task 4: SDK wrapper types and Value type

The SDK provides std-Rust wrapper types so plugin authors never touch stabby types. Each wrapper has `From<ffi_type>` and `Into<ffi_type>` impls for the macro to use at the FFI boundary.

**Files:**
- Create: `crates/hm-plugin-sdk/src/value.rs`
- Create: `crates/hm-plugin-sdk/src/types.rs`
- Modify: `crates/hm-plugin-sdk/src/lib.rs`
- Modify: `crates/hm-plugin-sdk/src/executor.rs`
- Modify: `crates/hm-plugin-sdk/src/hook.rs`
- Modify: `crates/hm-plugin-sdk/src/subcommand.rs`

**Step 1: Create `value.rs` — Value wrapper**

```rust
//! Ergonomic dynamic value type for plugin authors.

use std::collections::BTreeMap;
use hm_plugin_protocol::FfiValue;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Array(Vec<Value>),
    Object(BTreeMap<String, Value>),
}

impl Value {
    pub fn as_str(&self) -> Option<&str> { ... }
    pub fn as_i64(&self) -> Option<i64> { ... }
    pub fn as_f64(&self) -> Option<f64> { ... }
    pub fn as_bool(&self) -> Option<bool> { ... }
    pub fn as_array(&self) -> Option<&[Value]> { ... }
    pub fn as_object(&self) -> Option<&BTreeMap<String, Value>> { ... }
    pub fn get(&self, key: &str) -> Option<&Value> { ... }
    pub fn is_null(&self) -> bool { ... }
}

impl From<FfiValue> for Value { /* recursive conversion */ }
impl From<Value> for FfiValue { /* recursive conversion */ }
```

**Step 2: Create `types.rs` — SDK wrapper types**

For each protocol FFI type, define a std-Rust equivalent with `From`/`Into`:

```rust
use std::collections::BTreeMap;
use uuid::Uuid;

pub struct ExecutorInput {
    pub step: CommandStep,
    pub workspace_archive_id: Uuid,
    pub env: BTreeMap<String, String>,
    pub workdir: String,
    pub run_id: Uuid,
    pub step_id: Uuid,
    pub cache_lookup: CacheDecision,
    pub parent_snapshot: Option<String>,
}

pub struct CommandStep {
    pub key: String,
    pub cmd: String,
    pub image: Option<String>,
    pub env: Option<BTreeMap<String, String>>,
    pub timeout_seconds: Option<u32>,
    pub runner: Option<String>,
    pub runner_args: Option<Value>,
}

pub enum CacheDecision {
    Hit { tag: String },
    MissBuildAs { tag: String },
    MissNoCommit,
}

pub struct StepResult {
    pub exit_code: i32,
    pub committed_snapshot: Option<String>,
    pub artifacts: Vec<ArtifactRef>,
}

pub struct ArtifactRef {
    pub key: String,
    pub mime: String,
    pub size_bytes: u64,
}

pub struct SubcommandInput {
    pub verb_path: Vec<String>,
    pub args: Value,
    pub env: BTreeMap<String, String>,
}

pub struct ExitInfo {
    pub exit_code: i32,
    pub message: Option<String>,
}

pub struct PluginError {
    pub code: String,
    pub message: String,
    pub doc_url: Option<String>,
}

// HookEvent, HookOutcome, HookPhase, etc.
// ... with From impls for each direction
```

Each type needs:
- `impl From<hm_plugin_protocol::X> for X` (stabby → std, used by macro on inputs)
- `impl From<X> for hm_plugin_protocol::X` (std → stabby, used by macro on outputs)

String conversion: `stabby::string::String` → `std::string::String` via `String::from(stabby_str.as_str())` or the `From` impl.
Vec conversion: iterate + collect.
Option conversion: `stabby::option::Option` → `std::option::Option` via the `From` impl, then map inner.
BTreeMap conversion: iterate + collect.
Uuid: `Uuid::parse_str(&stabby_str)` (fallible — use `expect` or propagate error).

For `PluginError`, keep the `new()` and `with_doc()` convenience methods.

**Step 3: Update SDK traits**

Change `executor.rs`, `hook.rs`, `subcommand.rs` to import from `crate::types` instead of `hm_plugin_protocol`:

```rust
// executor.rs
use crate::types::{ExecutorInput, StepResult, PluginError};
```

The trait signatures stay the same shape — just the concrete types change from protocol to SDK.

**Step 4: Update `lib.rs` re-exports**

Replace `pub use hm_plugin_protocol::*;` with selective re-exports:

```rust
pub use hm_plugin_protocol::HM_PLUGIN_API_VERSION;
pub use types::*;
pub use value::Value;
```

Plugin authors import from SDK, see std Rust types.

**Step 5: Verify**

Run: `cargo check -p hm-plugin-sdk`
Expected: PASS for SDK. Downstream (macro, plugins) will break.

**Step 6: Commit**

```
git add crates/hm-plugin-sdk/
git commit -m "feat(sdk): add Value wrapper and std-Rust SDK types with From/Into stabby"
```

---

### Task 5: Update RawPlugin trait to typed signatures

Change the FFI trait from byte slices to typed stabby types. This breaks the macro and host until they're updated (Tasks 6-7).

**Files:**
- Modify: `crates/hm-plugin-sdk/src/ffi.rs`

**Step 1: Change RawPlugin**

```rust
use stabby::future::DynFutureUnsync;
use hm_plugin_protocol::{
    ExecutorInput, StepResult, PluginError,
    HookEvent, HookOutcome,
    SubcommandInput, ExitInfo,
    PluginManifest,
};

pub type FfiPluginResult<T> = stabby::result::Result<T, PluginError>;

#[stabby::stabby]
pub trait RawPlugin: Send + Sync {
    extern "C" fn manifest(&self) -> PluginManifest;
    extern "C" fn execute_step<'a>(
        &'a self,
        input: ExecutorInput,
    ) -> DynFutureUnsync<'a, FfiPluginResult<StepResult>>;
    extern "C" fn on_hook_event<'a>(
        &'a self,
        event: HookEvent,
    ) -> DynFutureUnsync<'a, FfiPluginResult<HookOutcome>>;
    extern "C" fn run_subcommand<'a>(
        &'a self,
        input: SubcommandInput,
    ) -> DynFutureUnsync<'a, FfiPluginResult<ExitInfo>>;
}
```

Remove `FfiBytes`, `FfiSlice`, `FfiResult` type aliases (no longer used by RawPlugin). Keep them temporarily if `RawHostApi` still uses them — or update `RawHostApi` here too if convenient.

**Step 2: Update RawHostApi** (if changing now)

The `RawHostApi` trait currently uses raw `u8` for level/scope and `FfiSlice<u8>` for payloads. Consider updating to typed stabby enums:

```rust
#[stabby::stabby]
pub trait RawHostApi: Send + Sync {
    extern "C" fn log(&self, level: hm_plugin_protocol::Level, msg: FfiSlice<'_>);
    extern "C" fn kv_get(
        &self,
        scope: hm_plugin_protocol::KvScope,
        key: FfiSlice<'_>,
    ) -> stabby::option::Option<FfiBytes>;
    extern "C" fn kv_set(
        &self,
        scope: hm_plugin_protocol::KvScope,
        key: FfiSlice<'_>,
        val: FfiSlice<'_>,
    );
    // ... rest keep FfiSlice for raw byte payloads (log messages, kv values, archive chunks)
}
```

Or defer RawHostApi changes to a later task if it complicates this one.

**Step 3: Update compile test**

The static assertions in `ffi.rs` `tests` module verify object safety. Update them for the new types.

**Step 4: Commit** (won't compile yet — that's expected)

```
git add crates/hm-plugin-sdk/src/ffi.rs
git commit -m "feat(sdk): typed stabby signatures for RawPlugin trait"
```

---

### Task 6: Update `hm_plugin!` macro

Remove all serde_json from generated code. The macro bridges between the typed RawPlugin trait (stabby types) and the SDK user traits (std types).

**Files:**
- Rewrite: `crates/hm-plugin-macros/src/lib.rs`

**Key changes:**

1. **`__HmPluginImpl` struct**: Replace `manifest_bytes: FfiBytes` with `manifest: hm_plugin_protocol::PluginManifest` (stabby type, stored directly).

2. **`manifest()` method**: Return `self.manifest.clone()` instead of `self.manifest_bytes.clone()`.

3. **`execute_step()` method**: No serde. Convert input, call trait, convert output:
```rust
extern "C" fn execute_step<'a>(
    &'a self,
    input: hm_plugin_protocol::ExecutorInput,
) -> stabby::future::DynFutureUnsync<'a, hm_plugin_sdk::ffi::FfiPluginResult<hm_plugin_protocol::StepResult>> {
    let ctx = &self.ctx;
    let executor = &self.executor;
    stabby::boxed::Box::new(async move {
        let sdk_input: hm_plugin_sdk::types::ExecutorInput = input.into();
        match hm_plugin_sdk::StepExecutor::run(executor, ctx, sdk_input).await {
            Ok(r) => stabby::result::Result::Ok(r.into()),
            Err(e) => stabby::result::Result::Err(e.into()),
        }
    })
    .into()
}
```

4. Same pattern for `on_hook_event()` and `run_subcommand()` — convert in, call trait, convert out.

5. **Not-implemented stubs**: Return `PluginError` directly:
```rust
stabby::result::Result::Err(
    hm_plugin_protocol::PluginError {
        code: "not_implemented".into(),
        message: "this plugin does not implement this capability".into(),
        doc_url: stabby::option::Option::None(),
    }
)
```

6. **`hm_load_plugin` entry point**: Construct `PluginManifest` (stabby) from the manifest expression. The manifest expression in user code currently evaluates to a protocol `PluginManifest`. Since protocol types are now stabby, this just works — the user's `PluginManifest { ... }` already constructs a stabby type (they use SDK re-exports which... hmm, SDK types are std-Rust now).

**Important subtlety:** The `manifest = PluginManifest { ... }` expression in `hm_plugin!` invocation — which type is it? Currently it's `hm_plugin_protocol::PluginManifest` (accessible via `hm_plugin_sdk::PluginManifest` re-export). After the change:
- Protocol's `PluginManifest` is stabby
- SDK's `PluginManifest` is std-Rust
- Plugin code writes `hm_plugin_sdk::PluginManifest { ... }` (std types)
- The macro needs the protocol (stabby) version
- So the macro should convert: `let manifest: hm_plugin_protocol::PluginManifest = { #manifest_expr }.into();`

This means the SDK's `PluginManifest` needs `Into<hm_plugin_protocol::PluginManifest>`.

7. **Delete `__ffi_bytes` helper** — no longer needed.

8. **Remove `serde_json` from macro-generated code entirely.** The proc-macro crate itself doesn't depend on serde_json (it generates tokens that reference it). Stop generating those references.

**Step 1: Implement all changes**

**Step 2: Verify**

Run: `cargo check -p hm-plugin-macros`
Expected: PASS (proc-macro crate itself should compile — it just generates tokens).

**Step 3: Commit**

```
git add crates/hm-plugin-macros/
git commit -m "feat(macros): typed stabby FFI, remove serde_json from generated code"
```

---

### Task 7: Update host-side dispatch (LoadedPlugin)

The host no longer serializes/deserializes at the FFI boundary. It constructs stabby types directly and reads results directly.

**Files:**
- Rewrite: `crates/hm-plugin-runtime/src/host.rs`
- Modify: `crates/hm-plugin-runtime/src/host_api.rs` (if updating RawHostApi)

**Key changes to `host.rs`:**

1. **Type aliases**: `LoadPluginFn` return type changes from `Result<PluginDyn, FfiBytes>` to `Result<PluginDyn, hm_plugin_protocol::PluginError>`.

2. **`LoadedPlugin::load()`**:
   - Call `static_ref.manifest()` → returns `PluginManifest` (stabby) directly
   - No `serde_json::from_slice` — just store the manifest
   - Error path: read `PluginError` fields directly (`.code`, `.message`)

3. **`LoadedPlugin::execute_step()`**:
   - Accept `hm_plugin_protocol::ExecutorInput` (stabby) — caller constructs it
   - Call trait method directly, no serialization
   - Result is `hm_plugin_protocol::StepResult` (stabby) — read fields directly
   - Error is `hm_plugin_protocol::PluginError` — read fields directly

4. **`LoadedPlugin::on_hook_event()`**:
   - Accept `hm_plugin_protocol::HookEvent` (stabby)
   - Return `hm_plugin_protocol::HookOutcome` (stabby)

5. **`LoadedPlugin::run_subcommand()`**:
   - Accept `hm_plugin_protocol::SubcommandInput` (stabby)
   - Return `hm_plugin_protocol::ExitInfo` (stabby)

6. **Delete `staticify_slice`** — no more byte slices to transmute. But `plugin_static()` is still needed for the `&'static` lifetime on the stabby vtable.

7. **Delete `ffi_err_to_anyhow`** — replace with direct field reads:
```rust
fn plugin_err_to_anyhow(name: &str, capability: &str, err: &hm_plugin_protocol::PluginError) -> anyhow::Error {
    RuntimeError::PluginPanic {
        name: name.to_string(),
        capability: capability.to_string(),
        message: err.message.to_string(),
    }.into()
}
```

8. **Update callers**: The orchestrator's `scheduler.rs` constructs `ExecutorInput`. It currently builds the serde version. Change to construct the stabby version using `convert::command_step()`:
```rust
let ffi_step = hm_plugin_runtime::convert::command_step(&ir_step);
let ffi_input = hm_plugin_protocol::ExecutorInput {
    step: ffi_step,
    workspace_archive_id: run_id.to_string().as_str().into(),
    // ... etc
};
```

The hook dispatcher in the orchestrator builds `HookEvent`. Change to construct stabby version using `convert::build_event()`.

The subcommand dispatcher in `cli/external.rs` builds `SubcommandInput`. Change to construct stabby version using `convert::json_to_ffi()` for args.

**Step 1: Implement all host.rs changes**

**Step 2: Update scheduler.rs, external.rs, and any other callers**

**Step 3: Verify**

Run: `cargo check -p hm-plugin-runtime && cargo check -p harmont-cli`
Expected: PASS

**Step 4: Commit**

```
git add crates/hm-plugin-runtime/ crates/hm/src/
git commit -m "feat(runtime): typed stabby dispatch, remove serde at FFI boundary"
```

---

### Task 8: Update docker plugin

The docker plugin implements `StepExecutor`. With SDK wrapper types, the code changes minimally — SDK `ExecutorInput` has the same field names with std types.

**Files:**
- Modify: `crates/hm/plugins/hm-plugin-docker/src/lib.rs`
- Modify: `crates/hm/plugins/hm-plugin-docker/src/image_name.rs`
- Modify: `crates/hm/plugins/hm-plugin-docker/Cargo.toml`

**Key changes:**

1. Types come from `hm_plugin_sdk::*` (which now re-exports SDK types, not protocol types). Import paths don't change.

2. Manifest construction: `PluginManifest { ... }` — fields are now std types (SDK wrapper), which the macro converts to stabby. String fields use `.into()`, `Vec` fields use `vec![...]`, `Option` fields use `None`/`Some(...)`.

3. The `StepExecutor::run` implementation: `input.step.key` is now `String` (was `String` — same). `input.step.cmd` is `String`. `input.run_id` is now `Uuid` (was `Uuid`). Mostly unchanged.

4. `image_name.rs`: accepts `&CommandStep` — SDK `CommandStep` has `image: Option<String>` (same as before).

5. Remove `serde_json` from docker plugin's `Cargo.toml` if it was only used for FFI.

**Step 1: Update plugin code**

**Step 2: Verify**

Run: `cargo check -p hm-plugin-docker`

**Step 3: Commit**

```
git add crates/hm/plugins/hm-plugin-docker/
git commit -m "refactor(docker): use SDK types for stabby FFI"
```

---

### Task 9: Update cloud plugin

The cloud plugin implements `SubcommandPlugin`. More changes here because it works with `SubcommandInput.args` (now `Value` instead of `serde_json::Value`).

**Files:**
- Modify: `crates/hm/plugins/hm-plugin-cloud/src/lib.rs`
- Modify: `crates/hm/plugins/hm-plugin-cloud/src/cli.rs`
- Modify: `crates/hm/plugins/hm-plugin-cloud/src/manifest_schema.rs`
- Modify: verb modules under `crates/hm/plugins/hm-plugin-cloud/src/verbs/`

**Key changes:**

1. **`lib.rs`**: `SubcommandPlugin::run` receives `SubcommandInput` (SDK type). `input.args` is `Value`, `input.verb_path` is `Vec<String>`.

2. **`cli.rs`**: Dispatch function matches on `input.verb_path` (still `Vec<String>`). Passes `&Value` to verb handlers.

3. **Verb handlers**: Change `args: &serde_json::Value` to `args: &Value`. Accessors change:
   - `args["field"].as_str()` → `args.get("field").and_then(Value::as_str)`
   - `args["field"].as_i64()` → `args.get("field").and_then(Value::as_i64)`
   - `args["field"].as_bool()` → `args.get("field").and_then(Value::as_bool)`

   These are slightly different APIs. The SDK `Value` should provide a `[]`-index operator (`impl Index<&str>`) for ergonomics, or verb helpers should be updated.

4. **`manifest_schema.rs`**: `cloud_spec()` returns `SubcommandSpec`. This uses `spec_from_command()` from the SDK. That function returns protocol `SubcommandSpec` (now stabby). The manifest expression in `hm_plugin!` expects SDK `PluginManifest`. Need to bridge: `spec_from_command` should return SDK `SubcommandSpec`, or the manifest construction should convert.

   Best approach: `spec_from_clap::spec_from_command` returns SDK `SubcommandSpec` (std types). The macro's `Into` converts to protocol stabby type.

5. **Remove `serde_json`** from cloud plugin's dependency if possible (it may still use it for API response parsing — check).

**Step 1: Update all cloud plugin source files**

**Step 2: Update `spec_from_clap.rs` in SDK to return SDK types**

**Step 3: Verify**

Run: `cargo check -p hm-plugin-cloud`

**Step 4: Commit**

```
git add crates/hm/plugins/hm-plugin-cloud/ crates/hm-plugin-sdk/src/spec_from_clap.rs
git commit -m "refactor(cloud): use SDK Value type and stabby FFI"
```

---

### Task 10: Update test fixtures

Test fixture plugins (noop-executor, recording-hook, etc.) in `tests/fixtures/` need updating.

**Files:**
- Modify: all `tests/fixtures/*/src/lib.rs`
- Modify: integration tests in `crates/hm-plugin-runtime/tests/`

**Key changes:**

1. Fixture plugins use `hm_plugin!` macro — their manifest expressions and trait impls need SDK types.

2. Integration tests that construct `ExecutorInput`, `SubcommandInput`, etc. need to use the new types.

3. `dummy_subcommand_input()` in `host.rs` — update to construct stabby `SubcommandInput`.

**Step 1: Update all fixtures and integration tests**

**Step 2: Verify**

Run: `cargo test --workspace`
Expected: All tests pass.

**Step 3: Commit**

```
git add tests/ crates/hm-plugin-runtime/
git commit -m "test: update fixtures and integration tests for stabby FFI types"
```

---

### Task 11: Clean up dependencies and dead code

Remove serde-related dependencies from crates that no longer need them.

**Files:**
- Modify: `crates/hm-plugin-protocol/Cargo.toml` — serde, serde_json, schemars, chrono, uuid, semver may be needed only by `ir.rs`/`events.rs` now. Check each.
- Modify: `crates/hm-plugin-sdk/Cargo.toml` — remove serde_json if no longer needed
- Modify: `crates/hm-plugin-macros/Cargo.toml` — never had runtime deps, but verify generated code no longer references serde_json
- Modify: plugin Cargo.toml files — remove serde_json if unused

**Step 1: Audit each crate's serde usage**

For the protocol crate:
- `ir.rs` needs serde, serde_json (Pipeline deserialization)
- `events.rs` needs serde, schemars, chrono, uuid (BuildEvent)
- All other modules no longer need serde
- `semver` is no longer used (version is stabby String now)
- Keep serde + serde_json for ir.rs/events.rs

**Step 2: Remove unused dependencies**

**Step 3: Verify**

Run: `cargo check --workspace && cargo test --workspace`

**Step 4: Commit**

```
git add Cargo.toml crates/*/Cargo.toml
git commit -m "chore: remove serde deps from crates that switched to stabby FFI"
```

---

## Verification

1. `cargo check --workspace` — clean compile
2. `cargo test --workspace` — all tests pass
3. `cargo run -- --help` — shows plugin subcommands
4. `cargo run -- cloud --help` — cloud sub-subcommands work
5. No `serde_json::to_vec` or `serde_json::from_slice` calls remain in FFI paths (only in ir.rs, events.rs, and API response parsing)
6. `hm plugin info <name>` — output format TBD (deferred)

## Risk: `#[stabby::stabby] #[repr(u8)]` on enums with data

stabby v72.1.1 `#[repr(u8)]` enums with data variants (like `FfiBuildEvent`, `CacheDecision`, `ArgSpec`) need verification. If `repr(u8)` doesn't work with data-carrying variants, fall back to `repr(C)` or `repr(stabby)`. Test early in Task 2 with a simple enum:

```rust
#[stabby::stabby]
#[repr(u8)]
enum Test {
    A,
    B { x: stabby::string::String },
    C(i32),
}
```

If this doesn't compile, all enums with data use `#[repr(C)]` instead (still allows standard `match` in most cases) or `#[repr(stabby)]` with `.match_*()` methods.

## Risk: Recursive `FfiValue`

`FfiValue::Array(Vec<FfiValue>)` and `FfiValue::Object(ArcBTreeMap<String, FfiValue>)` are recursive through heap indirection. Should compile fine — Vec and ArcBTreeMap are fixed-size pointer types. But verify in Task 1.

## Risk: `forbid(unsafe_code)` in protocol crate

`#[stabby::stabby]` derive may generate `unsafe` in its expansion. If protocol crate's `#![forbid(unsafe_code)]` blocks compilation, change to `#![deny(unsafe_code)]` with a module-level `#[allow(unsafe_code)]` on the modules that use stabby derives.
