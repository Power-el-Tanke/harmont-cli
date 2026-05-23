The `crates/` directory holds a Cargo workspace rooted at the repo root.

- `crates/hm/` — the `hm` binary (today's CLI body).
- `crates/hm-plugin-protocol/` — wire types (serde structs only).
- `crates/hm-plugin-sdk/` — authoring SDK for plugin writers; exposes the stabby-based FFI traits.
- `crates/hm-plugin-macros/` — proc-macro crate powering `register_plugin!`.
- `crates/hm-plugin-docker/`, `crates/hm-plugin-cloud/`, `crates/hm-plugin-output-human/`, `crates/hm-plugin-output-json/` — bundled plugins (native cdylib dylibs).
- `tests/fixtures/` — test-only cdylib crates (`noop-executor`, `recording-hook`, etc.) built via `cargo build` as native shared libraries.

Run `cargo build` from the workspace root. Run `cargo test --workspace` to exercise all crates.

For cross-cutting doctrine see [PRINCIPLES.md](../PRINCIPLES.md).
