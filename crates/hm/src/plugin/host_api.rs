//! Host-side implementation of `RawHostApi` for stabby-based plugins.
//!
//! `HostApiImpl` is the concrete type that backs every plugin's
//! `PluginContext`. It implements `hm_plugin_sdk::ffi::RawHostApi`
//! (all 11 methods, `extern "C"`, synchronous).

// The stabby trait impl requires unsafe for the FFI trampolines.
#![allow(unsafe_code)]
// Pedantic-bucket nags accepted at module scope:
// - `missing_errors_doc`: methods on `RawHostApi` don't return Result.
// - `cast_possible_truncation`: level/scope u8 conversions are bounded.
#![allow(clippy::missing_errors_doc, clippy::cast_possible_truncation)]

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

use hm_plugin_sdk::ffi::{FfiBytes, FfiSlice, RawHostApi};
use hm_plugin_protocol::BuildEvent;
use tokio::sync::broadcast;

use tokio_util::sync::CancellationToken;

/// Host-side state backing all 11 `RawHostApi` methods.
///
/// One instance is created per plugin-registry lifetime and shared
/// (via `Arc`) across all loaded plugins. Interior mutability uses
/// `std::sync::Mutex` (not tokio) because the FFI methods are
/// `extern "C"` and synchronous.
pub struct HostApiImpl {
    event_tx: broadcast::Sender<BuildEvent>,
    cancel_token: CancellationToken,
    kv_plugin: Mutex<BTreeMap<String, Vec<u8>>>,
    kv_build: Mutex<BTreeMap<String, Vec<u8>>>,
    kv_step: Mutex<BTreeMap<String, Vec<u8>>>,
    project_root: Option<PathBuf>,
}

impl std::fmt::Debug for HostApiImpl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostApiImpl")
            .field("project_root", &self.project_root)
            .finish_non_exhaustive()
    }
}

impl HostApiImpl {
    /// Create a new host API implementation.
    ///
    /// `event_tx` is the broadcast sender for `BuildEvent`s (the
    /// output subscriber drains the receiving end). `cancel_token`
    /// allows plugins to poll for cancellation.
    #[must_use]
    pub fn new(
        event_tx: broadcast::Sender<BuildEvent>,
        cancel_token: CancellationToken,
        project_root: Option<PathBuf>,
    ) -> Self {
        Self {
            event_tx,
            cancel_token,
            kv_plugin: Mutex::new(BTreeMap::new()),
            kv_build: Mutex::new(BTreeMap::new()),
            kv_step: Mutex::new(BTreeMap::new()),
            project_root,
        }
    }

    /// Create a minimal instance suitable for tests or non-orchestrator
    /// paths (e.g. `hm plugin list`, `hm version`).
    #[must_use]
    pub fn new_noop() -> Self {
        let (tx, _rx) = broadcast::channel(16);
        Self {
            event_tx: tx,
            cancel_token: CancellationToken::new(),
            kv_plugin: Mutex::new(BTreeMap::new()),
            kv_build: Mutex::new(BTreeMap::new()),
            kv_step: Mutex::new(BTreeMap::new()),
            project_root: None,
        }
    }

    /// Clear step-scoped KV state. Called by the scheduler between steps.
    pub fn clear_step_kv(&self) {
        if let Ok(mut m) = self.kv_step.lock() {
            m.clear();
        }
    }
}

// ---------------------------------------------------------------------------
// RawHostApi implementation
// ---------------------------------------------------------------------------

impl RawHostApi for HostApiImpl {
    extern "C" fn log(&self, level: u8, msg: FfiSlice<'_>) {
        let text = core::str::from_utf8(msg.as_ref()).unwrap_or("<invalid utf-8>");
        match level {
            0 => tracing::trace!(target: "plugin", "{text}"),
            1 => tracing::debug!(target: "plugin", "{text}"),
            2 => tracing::info!(target: "plugin", "{text}"),
            3 => tracing::warn!(target: "plugin", "{text}"),
            _ => tracing::error!(target: "plugin", "{text}"),
        }
    }

    extern "C" fn kv_get(
        &self,
        scope: u8,
        key: FfiSlice<'_>,
    ) -> stabby::option::Option<FfiBytes> {
        let key_str = core::str::from_utf8(key.as_ref()).unwrap_or("");
        let map = match scope {
            0 => &self.kv_plugin,
            1 => &self.kv_build,
            2 => &self.kv_step,
            _ => return stabby::option::Option::None(),
        };
        let guard = match map.lock() {
            Ok(g) => g,
            Err(_) => return stabby::option::Option::None(),
        };
        match guard.get(key_str) {
            Some(val) => stabby::option::Option::Some(FfiBytes::from(val.as_slice())),
            None => stabby::option::Option::None(),
        }
    }

    extern "C" fn kv_set(&self, scope: u8, key: FfiSlice<'_>, val: FfiSlice<'_>) {
        let key_str = match core::str::from_utf8(key.as_ref()) {
            Ok(s) => s,
            Err(_) => return,
        };
        let map = match scope {
            0 => &self.kv_plugin,
            1 => &self.kv_build,
            2 => &self.kv_step,
            _ => return,
        };
        match map.lock() {
            Ok(mut guard) => { guard.insert(key_str.to_string(), val.to_vec()); }
            Err(_) => tracing::warn!(target: "plugin::host_api", "kv_set: mutex poisoned"),
        }
    }

    extern "C" fn emit_event(&self, event_json: FfiSlice<'_>) {
        let Ok(event) = serde_json::from_slice::<BuildEvent>(event_json.as_ref()) else {
            tracing::warn!(target: "plugin::host_api", "failed to deserialize BuildEvent from plugin");
            return;
        };
        // Best-effort: if nobody is listening the send fails silently.
        let _ = self.event_tx.send(event);
    }

    extern "C" fn emit_step_log(&self, stream: u8, bytes: FfiSlice<'_>) {
        // Stream: 0 = stdout, 1 = stderr. For now, just emit as a
        // BuildEvent. Full step-id tagging will be wired up in Task 6.
        let line = String::from_utf8_lossy(bytes.as_ref()).into_owned();
        let stream_enum = if stream == 0 {
            hm_plugin_protocol::StdStream::Stdout
        } else {
            hm_plugin_protocol::StdStream::Stderr
        };
        // TODO(task-7): replace nil UUID with actual step_id (needs per-step HostApiImpl or field)
        let event = BuildEvent::StepLog {
            step_id: uuid::Uuid::nil(),
            stream: stream_enum,
            line,
            ts: chrono::Utc::now(),
        };
        let _ = self.event_tx.send(event);
    }

    extern "C" fn should_cancel(&self) -> bool {
        self.cancel_token.is_cancelled()
    }

    #[allow(
        clippy::print_stdout,
        reason = "this method's purpose is user-facing stdout output"
    )]
    extern "C" fn write_stdout(&self, bytes: FfiSlice<'_>) {
        use std::io::Write;
        let mut out = std::io::stdout().lock();
        let _ = out.write_all(bytes.as_ref());
        let _ = out.flush();
    }

    #[allow(
        clippy::print_stderr,
        reason = "this method's purpose is user-facing stderr output"
    )]
    extern "C" fn write_stderr(&self, bytes: FfiSlice<'_>) {
        use std::io::Write;
        let mut err = std::io::stderr().lock();
        let _ = err.write_all(bytes.as_ref());
        let _ = err.flush();
    }

    extern "C" fn archive_read(
        &self,
        _id_json: FfiSlice<'_>,
        _offset: u64,
        _max: u64,
    ) -> FfiBytes {
        // Minimal stub — full archive I/O will be wired up when
        // callers are connected (Tasks 5-8).
        FfiBytes::from(&[] as &[u8])
    }

    extern "C" fn archive_total_size(&self, _id_json: FfiSlice<'_>) -> u64 {
        0
    }

    extern "C" fn fs_read_config(
        &self,
        rel_path: FfiSlice<'_>,
    ) -> stabby::option::Option<FfiBytes> {
        let rel = match core::str::from_utf8(rel_path.as_ref()) {
            Ok(s) => s,
            Err(_) => return stabby::option::Option::None(),
        };
        let root = match &self.project_root {
            Some(r) => r.join(".harmont"),
            None => match std::env::current_dir() {
                Ok(cwd) => cwd.join(".harmont"),
                Err(_) => return stabby::option::Option::None(),
            },
        };
        let Ok(canonical_root) = root.canonicalize() else {
            return stabby::option::Option::None();
        };
        let candidate = canonical_root.join(rel);
        let Ok(canonical) = candidate.canonicalize() else {
            return stabby::option::Option::None();
        };
        if !canonical.starts_with(&canonical_root) {
            return stabby::option::Option::None();
        }
        match std::fs::read(&canonical) {
            Ok(bytes) => stabby::option::Option::Some(FfiBytes::from(bytes.as_slice())),
            Err(_) => stabby::option::Option::None(),
        }
    }
}
