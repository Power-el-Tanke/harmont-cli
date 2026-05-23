//! In-process plugin host.
//!
//! Loads native shared-library plugins (`.dylib`/`.so`/`.dll`) via
//! stabby's ABI-stable trait objects. Replaces the prior extism/WASM
//! pipeline.

pub mod host;
pub mod host_api;
pub mod host_fns;
pub mod install;
pub mod manifest;
pub mod paths;
pub mod pool;
pub mod registry;
pub mod signal;

pub use host::LoadedPlugin;
pub use registry::{PluginRegistry, RegistryConfig};
