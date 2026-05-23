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
