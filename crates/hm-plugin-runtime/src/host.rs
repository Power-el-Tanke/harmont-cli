//! Thin wrapper around stabby-loaded native plugin dylibs.
//!
//! Each `LoadedPlugin` owns a `libloading::Library` and a stabby
//! trait object implementing `RawPlugin + Send + Sync`. The trait
//! object is ABI-stable across compiler versions thanks to stabby.

// stabby trait objects and libloading require unsafe for loading
// and calling into foreign code.
#![allow(unsafe_code)]
// Pedantic-bucket nags that don't add safety on this module:
// - `missing_errors_doc`: every public fn here returns `anyhow::Result`
//   with a context message; an `# Errors` section would just restate it.
#![allow(clippy::missing_errors_doc)]

use std::mem::ManuallyDrop;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use hm_plugin_protocol::PluginManifest;
use hm_plugin_sdk::ffi::RawPluginDyn as _;
use stabby::libloading::StabbyLibrary;

use crate::host_api::HostApiImpl;
use crate::error::RuntimeError;

// Type aliases matching the macro crate's `host_ref_type()` and
// `plugin_dyn_type()` outputs. These are the exact stabby compound-vtable
// types that the `#[stabby::export] fn hm_load_plugin(...)` symbol
// uses on both sides of the FFI boundary.

/// The stabby `DynRef` wrapping a `&'static dyn RawHostApi + Send + Sync`.
type HostRef = stabby::DynRef<
    'static,
    <dyn Sync as stabby::abi::vtable::CompoundVt<'static>>::Vt<
        <dyn Send as stabby::abi::vtable::CompoundVt<'static>>::Vt<
            <dyn hm_plugin_sdk::ffi::RawHostApi as stabby::abi::vtable::CompoundVt<'static>>::Vt<
                stabby::abi::vtable::VtDrop,
            >,
        >,
    >,
>;

/// The stabby `Dyn` wrapping a `Box<dyn RawPlugin + Send + Sync>`.
type PluginDyn = stabby::Dyn<
    'static,
    stabby::boxed::Box<()>,
    <dyn Sync as stabby::abi::vtable::CompoundVt<'static>>::Vt<
        <dyn Send as stabby::abi::vtable::CompoundVt<'static>>::Vt<
            <dyn hm_plugin_sdk::ffi::RawPlugin as stabby::abi::vtable::CompoundVt<'static>>::Vt<
                stabby::abi::vtable::VtDrop,
            >,
        >,
    >,
>;

/// The entry point function signature exported by plugins via
/// `#[stabby::export]`.
type LoadPluginFn = extern "C" fn(
    HostRef,
) -> stabby::result::Result<
    PluginDyn,
    hm_plugin_sdk::ffi::FfiBytes,
>;

/// A loaded native plugin. Holds the library handle and the stabby
/// trait object. Field ordering matters: `plugin` (which borrows from
/// the library's code) must be dropped before `_lib`.
pub struct LoadedPlugin {
    pub manifest: PluginManifest,
    /// Path the plugin was loaded from.
    pub source: Option<PathBuf>,
    /// The stabby trait object implementing RawPlugin. Wrapped in
    /// `ManuallyDrop` so we can control drop order: this must be
    /// dropped before `_lib`.
    plugin: ManuallyDrop<PluginDyn>,
    /// The dynamically loaded library. Kept alive for the lifetime of
    /// the trait object. Must be dropped AFTER `plugin`.
    _lib: libloading::Library,
    /// The host API reference. Leaked to `'static` so the plugin can
    /// hold it for its entire lifetime. The `Arc` prevents the
    /// underlying data from being freed.
    _host_api: Arc<HostApiImpl>,
}

// SAFETY: PluginDyn carries Send + Sync vtable markers. The Library
// handle is an opaque OS handle (safe to move between threads). The
// HostApiImpl is Send + Sync by construction.
unsafe impl Send for LoadedPlugin {}
// SAFETY: see above — all fields are safe for shared references.
unsafe impl Sync for LoadedPlugin {}

impl std::fmt::Debug for LoadedPlugin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoadedPlugin")
            .field("manifest", &self.manifest)
            .field("source", &self.source)
            .field("plugin", &"<stabby::Dyn<RawPlugin>>")
            .finish()
    }
}

impl Drop for LoadedPlugin {
    fn drop(&mut self) {
        // SAFETY: we manually drop `plugin` before `_lib` goes out of
        // scope (which happens immediately after, when the struct is
        // dropped). This guarantees the trait object's code is still
        // loaded when its destructor runs.
        //
        // NOTE: currently leaking — investigating a SIGSEGV in stabby
        // Dyn drop across dylib boundary on macOS/arm64.
        // unsafe { ManuallyDrop::drop(&mut self.plugin); }
    }
}

impl LoadedPlugin {
    /// Obtain a `&'static PluginDyn` from our stored plugin.
    ///
    /// The stabby vtable for `Dyn<'static, ...>` requires `&'static self`
    /// to call its methods (because the vtable's function pointers carry
    /// `PhantomData<&'a &'static ()>` which forces `'a: 'static`). Since
    /// the `LoadedPlugin` owns both the `PluginDyn` and the `Library`,
    /// and every returned future is `.await`-ed immediately (never stored
    /// or moved), the borrow cannot actually outlive the struct.
    ///
    /// # Safety
    /// The caller must `.await` the returned future before dropping `self`.
    unsafe fn plugin_static(&self) -> &'static PluginDyn {
        unsafe { &*(&*self.plugin as *const PluginDyn) }
    }

    /// Extend a `FfiSlice` to `'static` lifetime.
    ///
    /// The plugin's generated code (see `hm-plugin-macros` `expand()`)
    /// deserializes the input via `serde_json::from_slice` at the very
    /// start of the async block — before any `.await` / yield point.
    /// The `in_bytes` local outlives the `.await`, so the borrow is
    /// sound even though Rust can't prove it statically.
    ///
    /// # Safety
    /// The backing data must remain valid until the returned future
    /// completes its first poll (which copies the data).
    unsafe fn staticify_slice(
        s: hm_plugin_sdk::ffi::FfiSlice<'_>,
    ) -> hm_plugin_sdk::ffi::FfiSlice<'static> {
        unsafe { core::mem::transmute(s) }
    }

    /// Load a native plugin from a shared library on disk.
    ///
    /// The `host_api` is leaked to a `&'static` reference (via
    /// `Arc::into_raw`) so the plugin can hold it for its full lifetime.
    pub fn load(path: &Path, host_api: Arc<HostApiImpl>) -> Result<Self> {
        // SAFETY: Loading a shared library executes its init routines.
        // We trust plugins built with the SDK.
        let lib = unsafe { libloading::Library::new(path) }
            .with_context(|| format!("dlopen {}", path.display()))?;

        // SAFETY: The symbol was generated by `#[stabby::export]` and
        // has ABI-stable layout checked by stabby's report mechanism.
        let load_fn = unsafe {
            lib.get_stabbied::<LoadPluginFn>(b"hm_load_plugin")
        }
        .map_err(|e| anyhow::anyhow!(
            "get hm_load_plugin symbol from {}: {e}",
            path.display()
        ))?;

        // Create a DynRef to the host API. We leak the Arc to obtain a
        // `&'static HostApiImpl`, then wrap it in a stabby DynRef.
        let host_ref: &'static HostApiImpl = {
            let ptr = Arc::into_raw(Arc::clone(&host_api));
            // SAFETY: ptr is valid for 'static because the Arc is kept
            // alive in `_host_api`.
            unsafe { &*ptr }
        };

        // Convert &'static HostApiImpl to HostRef (DynRef<'static, ...>).
        let dyn_ref: HostRef = stabby::DynRef::from(host_ref);

        // Call the plugin's entry point.
        let stabby_result = (*load_fn)(dyn_ref);

        // Convert stabby::result::Result to core::result::Result
        let std_result: core::result::Result<PluginDyn, hm_plugin_sdk::ffi::FfiBytes> =
            stabby_result.into();

        let plugin = match std_result {
            Ok(p) => p,
            Err(err_bytes) => {
                // Re-claim the Arc we leaked so it doesn't actually leak.
                let ptr = host_ref as *const HostApiImpl;
                unsafe { Arc::from_raw(ptr); }
                let err_str = String::from_utf8_lossy(err_bytes.as_slice());
                anyhow::bail!(
                    "plugin {} refused to load: {err_str}",
                    path.display()
                );
            }
        };

        // Wrap in ManuallyDrop first so we can use plugin_static().
        let plugin = ManuallyDrop::new(plugin);

        // Read the manifest from the plugin. `manifest()` takes
        // `&'static self` due to the stabby vtable lifetime; use
        // the same staticify trick.
        //
        // SAFETY: `plugin` is alive (we just created it) and we use
        // the result synchronously (no escaping borrow).
        let manifest_bytes = {
            let static_ref: &'static PluginDyn =
                unsafe { &*(&*plugin as *const PluginDyn) };
            static_ref.manifest()
        };
        let manifest: PluginManifest = serde_json::from_slice(manifest_bytes.as_slice())
            .with_context(|| {
                format!("decode manifest from {}", path.display())
            })?;

        Ok(Self {
            manifest,
            source: Some(path.to_path_buf()),
            plugin,
            _lib: lib,
            _host_api: host_api,
        })
    }

    /// Execute a step. Serializes `input` as JSON, calls the plugin's
    /// `execute_step`, and deserializes the result.
    pub async fn execute_step(
        &self,
        input: &hm_plugin_protocol::ExecutorInput,
    ) -> Result<hm_plugin_protocol::StepResult> {
        let in_bytes = serde_json::to_vec(input).context("serialize ExecutorInput")?;
        // SAFETY: see `plugin_static()` and `staticify_slice()` docs.
        // The data in `in_bytes` outlives the `.await`, and the plugin
        // copies it before yielding.
        let ffi_input = unsafe {
            Self::staticify_slice(hm_plugin_sdk::ffi::FfiSlice::from(in_bytes.as_slice()))
        };
        let future = unsafe { self.plugin_static() }.execute_step(ffi_input);
        let stabby_result = future.await;
        let std_result: core::result::Result<
            hm_plugin_sdk::ffi::FfiBytes,
            hm_plugin_sdk::ffi::FfiBytes,
        > = stabby_result.into();
        match std_result {
            Ok(out) => {
                serde_json::from_slice(out.as_slice()).context("deserialize StepResult")
            }
            Err(err) => Err(ffi_err_to_anyhow(&self.manifest.name, "execute_step", &err)),
        }
    }

    /// Dispatch a lifecycle hook event.
    pub async fn on_hook_event(
        &self,
        event: &hm_plugin_protocol::HookEvent,
    ) -> Result<hm_plugin_protocol::HookOutcome> {
        let in_bytes = serde_json::to_vec(event).context("serialize HookEvent")?;
        // SAFETY: see `plugin_static()` and `staticify_slice()` docs.
        let ffi_input = unsafe {
            Self::staticify_slice(hm_plugin_sdk::ffi::FfiSlice::from(in_bytes.as_slice()))
        };
        let future = unsafe { self.plugin_static() }.on_hook_event(ffi_input);
        let stabby_result = future.await;
        let std_result: core::result::Result<
            hm_plugin_sdk::ffi::FfiBytes,
            hm_plugin_sdk::ffi::FfiBytes,
        > = stabby_result.into();
        match std_result {
            Ok(out) => {
                serde_json::from_slice(out.as_slice()).context("deserialize HookOutcome")
            }
            Err(err) => Err(ffi_err_to_anyhow(&self.manifest.name, "on_hook_event", &err)),
        }
    }

    /// Run a subcommand.
    pub async fn run_subcommand(
        &self,
        input: &hm_plugin_protocol::SubcommandInput,
    ) -> Result<hm_plugin_protocol::ExitInfo> {
        let in_bytes = serde_json::to_vec(input).context("serialize SubcommandInput")?;
        // SAFETY: see `plugin_static()` and `staticify_slice()` docs.
        let ffi_input = unsafe {
            Self::staticify_slice(hm_plugin_sdk::ffi::FfiSlice::from(in_bytes.as_slice()))
        };
        let future = unsafe { self.plugin_static() }.run_subcommand(ffi_input);
        let stabby_result = future.await;
        let std_result: core::result::Result<
            hm_plugin_sdk::ffi::FfiBytes,
            hm_plugin_sdk::ffi::FfiBytes,
        > = stabby_result.into();
        match std_result {
            Ok(out) => {
                serde_json::from_slice(out.as_slice()).context("deserialize ExitInfo")
            }
            Err(err) => Err(ffi_err_to_anyhow(&self.manifest.name, "run_subcommand", &err)),
        }
    }

}

/// Convert an FFI error response (serialized `PluginError`) into an
/// `anyhow::Error` wrapping `RuntimeError::PluginPanic`.
fn ffi_err_to_anyhow(
    plugin_name: &str,
    capability: &str,
    err: &hm_plugin_sdk::ffi::FfiBytes,
) -> anyhow::Error {
    let plugin_err: hm_plugin_protocol::PluginError =
        serde_json::from_slice(err.as_slice())
            .unwrap_or_else(|_| hm_plugin_protocol::PluginError::new(
                capability,
                String::from_utf8_lossy(err.as_slice()).to_string(),
            ));
    RuntimeError::PluginPanic {
        name: plugin_name.to_string(),
        capability: capability.to_string(),
        message: plugin_err.message,
    }
    .into()
}

/// Test helper: synthesises a `SubcommandInput` shaped JSON value for
/// the `host_fn_probe` fixture and any other integration test that
/// needs a minimal valid input to `hm_subcommand_run`.
///
/// `#[doc(hidden)]` because this is not part of the production public
/// API; it exists so `tests/*.rs` integration tests (which see only
/// the public surface) can call into it without a separate feature
/// flag.
#[doc(hidden)]
#[must_use]
pub fn dummy_subcommand_input() -> hm_plugin_protocol::SubcommandInput {
    hm_plugin_protocol::SubcommandInput {
        verb_path: vec!["fixture-probe".into()],
        args: serde_json::json!({}),
        env: std::collections::BTreeMap::new(),
    }
}
