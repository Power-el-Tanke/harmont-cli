//! JSON Schema snapshot test. Catches any unintentional change to the
//! wire format (field rename, type swap, required-vs-optional flip).
//! Run `cargo insta accept -p hm-plugin-protocol` to refresh after an
//! intended schema change.

#![allow(
    clippy::cargo_common_metadata,
    clippy::multiple_crate_versions,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

use hm_plugin_protocol::PluginManifest;
use schemars::schema_for;

#[test]
fn plugin_manifest_schema_is_stable() {
    let schema = schema_for!(PluginManifest);
    insta::assert_json_snapshot!("plugin_manifest", schema);
}
