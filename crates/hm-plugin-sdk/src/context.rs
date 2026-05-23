#![allow(unsafe_code)]
//! Ergonomic wrapper around the FFI host API.
//!
//! [`PluginContext`] is handed to every user-facing trait method and
//! provides Rust-native access to the host functions that back
//! [`RawHostApi`](crate::ffi::RawHostApi).

use crate::ffi::{FfiBytes, FfiSlice, RawHostApi, RawHostApiDyn};
use hm_plugin_protocol::{BuildEvent, KvScope, Level};

/// Type alias for the stabby borrowed trait-object reference that
/// backs [`PluginContext`]. Equivalent to a stable `&'a dyn
/// RawHostApi + Send + Sync`.
///
/// The `'static` lifetime on `CompoundVt` is required because
/// `DynRef<'a, Vt>` demands `Vt: 'static`; the vtable is a set of
/// function pointers that live for the entire program.
type HostRef<'a> = stabby::DynRef<
    'a,
    <dyn Sync as stabby::abi::vtable::CompoundVt<'static>>::Vt<
        <dyn Send as stabby::abi::vtable::CompoundVt<'static>>::Vt<
            <dyn RawHostApi as stabby::abi::vtable::CompoundVt<'static>>::Vt<
                stabby::abi::vtable::VtDrop,
            >,
        >,
    >,
>;

/// Ergonomic wrapper around the host-provided [`RawHostApi`] trait
/// object. Every user-facing trait method receives a `&PluginContext`
/// so it can call host functions without touching FFI types directly.
pub struct PluginContext<'a> {
    raw: HostRef<'a>,
}

impl core::fmt::Debug for PluginContext<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PluginContext")
            .field("raw", &"<DynRef<RawHostApi>>")
            .finish()
    }
}

// SAFETY: The underlying DynRef holds a Send+Sync vtable (VtSend +
// VtSync markers). The pointee is guaranteed Send+Sync by the host
// contract. DynRef itself carries a PhantomData<*mut ()> that inhibits
// auto-trait inference, so we provide explicit impls.
unsafe impl Send for PluginContext<'_> {}
// SAFETY: see above — the vtable encodes Sync.
unsafe impl Sync for PluginContext<'_> {}

impl<'a> PluginContext<'a> {
    /// Create a new context wrapping a borrowed host API trait object.
    pub fn new(raw: HostRef<'a>) -> Self {
        Self { raw }
    }

    // -- Logging ----------------------------------------------------------

    /// Log a message at the given severity level.
    pub fn log(&self, level: Level, msg: &str) {
        let level_u8 = level_to_u8(level);
        let ffi_msg = FfiSlice::from(msg.as_bytes());
        self.raw.log(level_u8, ffi_msg);
    }

    // -- Key-value store --------------------------------------------------

    /// Read a value from the host key-value store.
    pub fn kv_get(&self, scope: KvScope, key: &str) -> Option<Vec<u8>> {
        let scope_u8 = kv_scope_to_u8(scope);
        let ffi_key = FfiSlice::from(key.as_bytes());
        let result: stabby::option::Option<FfiBytes> = self.raw.kv_get(scope_u8, ffi_key);
        let opt: Option<FfiBytes> = result.into();
        opt.map(|ffi_bytes| ffi_bytes.as_slice().to_vec())
    }

    /// Write a value into the host key-value store.
    pub fn kv_set(&self, scope: KvScope, key: &str, val: &[u8]) {
        let scope_u8 = kv_scope_to_u8(scope);
        let ffi_key = FfiSlice::from(key.as_bytes());
        let ffi_val = FfiSlice::from(val);
        self.raw.kv_set(scope_u8, ffi_key, ffi_val);
    }

    // -- Events -----------------------------------------------------------

    /// Emit a build event to the host.
    pub fn emit_event(&self, event: &BuildEvent) {
        let bytes =
            serde_json::to_vec(event).expect("BuildEvent serialization should never fail");
        let ffi = FfiSlice::from(bytes.as_slice());
        self.raw.emit_event(ffi);
    }

    // -- Step log streams -------------------------------------------------

    /// Stream bytes to the step's stdout log.
    pub fn emit_step_log_stdout(&self, bytes: &[u8]) {
        let ffi = FfiSlice::from(bytes);
        self.raw.emit_step_log(0, ffi);
    }

    /// Stream bytes to the step's stderr log.
    pub fn emit_step_log_stderr(&self, bytes: &[u8]) {
        let ffi = FfiSlice::from(bytes);
        self.raw.emit_step_log(1, ffi);
    }

    // -- Cancellation -----------------------------------------------------

    /// Check whether the host has requested cancellation.
    pub fn should_cancel(&self) -> bool {
        self.raw.should_cancel()
    }

    // -- Direct I/O -------------------------------------------------------

    /// Write bytes to the host process's stdout.
    pub fn write_stdout(&self, bytes: &[u8]) {
        let ffi = FfiSlice::from(bytes);
        self.raw.write_stdout(ffi);
    }

    /// Write bytes to the host process's stderr.
    pub fn write_stderr(&self, bytes: &[u8]) {
        let ffi = FfiSlice::from(bytes);
        self.raw.write_stderr(ffi);
    }

    // -- Archive I/O ------------------------------------------------------

    /// Read a chunk from an archive identified by `args_borsh` at the
    /// given `offset`, returning at most `max` bytes.
    pub fn archive_read(&self, args_borsh: &[u8], offset: u64, max: u64) -> Vec<u8> {
        let ffi = FfiSlice::from(args_borsh);
        let result: FfiBytes = self.raw.archive_read(ffi, offset, max);
        result.as_slice().to_vec()
    }

    /// Return the total size in bytes of an archive identified by
    /// `args_borsh`.
    pub fn archive_total_size(&self, args_borsh: &[u8]) -> u64 {
        let ffi = FfiSlice::from(args_borsh);
        self.raw.archive_total_size(ffi)
    }

    // -- Config -----------------------------------------------------------

    /// Read a configuration file relative to the project root.
    /// Returns `None` if the file does not exist.
    pub fn fs_read_config(&self, rel_path: &str) -> Option<Vec<u8>> {
        let ffi = FfiSlice::from(rel_path.as_bytes());
        let result: stabby::option::Option<FfiBytes> = self.raw.fs_read_config(ffi);
        let opt: Option<FfiBytes> = result.into();
        opt.map(|ffi_bytes| ffi_bytes.as_slice().to_vec())
    }
}

// -- Enum → u8 helpers ----------------------------------------------------

fn level_to_u8(level: Level) -> u8 {
    match level {
        Level::Trace => 0,
        Level::Debug => 1,
        Level::Info => 2,
        Level::Warn => 3,
        Level::Error => 4,
    }
}

fn kv_scope_to_u8(scope: KvScope) -> u8 {
    match scope {
        KvScope::Plugin => 0,
        KvScope::Build => 1,
        KvScope::Step => 2,
    }
}
