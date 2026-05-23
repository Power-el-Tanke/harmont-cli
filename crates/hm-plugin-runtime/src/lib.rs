//! Plugin loading, discovery, and host-API runtime.

pub mod clap_bridge;
pub mod error;
pub mod host;
pub mod host_api;
pub mod install;
pub mod registry;

pub use host::LoadedPlugin;
pub use registry::{CapabilityIndex, PluginRegistry, RegistryConfig};
