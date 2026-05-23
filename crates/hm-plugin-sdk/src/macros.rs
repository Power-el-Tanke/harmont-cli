//! Re-exports the `hm_plugin!` proc macro from `hm-plugin-macros`.
//!
//! Plugin authors invoke this macro in their `lib.rs` to generate the
//! FFI entry point and `RawPlugin` implementation:
//!
//! ```ignore
//! use hm_plugin_sdk::*;
//!
//! hm_plugin!(
//!     manifest = PluginManifest { /* ... */ },
//!     executor = MyExec,
//! );
//! ```

pub use hm_plugin_macros::hm_plugin;
