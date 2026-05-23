//! Authoring SDK for `hm` plugins.
//!
//! Plugins build to `cdylib` and target `wasm32-wasip1`. The canonical
//! plugin entry point is the [`register_plugin!`] macro, which wires
//! every capability the plugin implements to the right Extism export.
//!
//! ```ignore
//! use hm_plugin_sdk::*;
//! use hm_plugin_protocol::*;
//!
//! struct MyExec;
//! impl StepExecutor for MyExec {
//!     fn run(&self, input: ExecutorInput) -> Result<StepResult, PluginError> {
//!         host::log(Level::Info, &format!("running {}", input.step.key));
//!         Ok(StepResult { exit_code: 0, committed_snapshot: None, artifacts: vec![] })
//!     }
//! }
//!
//! register_plugin!(
//!     manifest = PluginManifest {
//!         api_version: HM_PLUGIN_API_VERSION,
//!         name: "my-exec".into(),
//!         version: semver::Version::parse("0.1.0").unwrap(),
//!         description: "demo".into(),
//!         capabilities: vec![Capability::StepExecutor(StepExecutorSpec {
//!             runner: "demo".into(), default: false, step_schema: None,
//!         })],
//!         required_host_fns: vec!["hm_log".into()],
//!         config_schema: None,
//!         allowed_hosts: vec![],
//!     },
//!     executor = MyExec,
//! );
//! ```

// The SDK calls into `extern "ExtismHost"` host functions declared via
// `extism-pdk`'s `host_fn!` macro. Those imports are inherently unsafe FFI,
// so this crate cannot use `#![forbid(unsafe_code)]` the way `hm-plugin-protocol`
// does — the only unsafe blocks live in `host.rs`, gated by an explicit
// module-level allow.
//
// schemars 0.8 pulls older indexmap and wit-bindgen via its transitive tree
// (inherited through hm-plugin-protocol). Keep the same crate-level allows as
// the protocol crate so noisy cargo-group lints don't drown out real issues.
#![allow(clippy::multiple_crate_versions, clippy::cargo_common_metadata)]

pub mod executor;
pub mod ffi;
pub mod hook;
// pub mod host; // Commented out: will be removed in the extism→stabby migration
pub mod manifest;
pub mod output;
pub mod subcommand;

#[doc(hidden)]
pub mod macros;

pub use executor::StepExecutor;
pub use hm_plugin_protocol::*;
pub use hook::LifecycleHook;
pub use output::OutputFormatter;
pub use subcommand::{SubcommandInput, SubcommandPlugin};

// Re-export commented out: extism-pdk is no longer a dependency.
// pub use extism_pdk;
