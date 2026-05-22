//! Embedded plugin bytes. Compiled by `build.rs`.

/// Bytes of the in-tree Docker step-executor plugin. Always loaded
/// by the orchestrator at run start.
pub static DOCKER_PLUGIN_WASM: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/hm_plugin_docker.wasm"));

/// Bytes of the in-tree cloud client plugin (`hm cloud …`). Loaded by
/// the host dispatcher whenever the user invokes the `cloud` verb.
pub static CLOUD_PLUGIN_WASM: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/hm_plugin_cloud.wasm"));
