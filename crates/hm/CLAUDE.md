## Orchestrator

`crates/hm/src/orchestrator/` is the entry point for local builds.
`hm run` calls into `orchestrator::run()`, which:

- Builds a wire-typed `Graph` (`graph.rs`) from the parsed `Pipeline`
  and partitions it into chains for scheduling.
- Loads the plugin registry (discovers native cdylib plugins from
  `~/.harmont/plugins/` and `<project>/.harmont/plugins/`) and
  resolves each step's `runner` field to a registered plugin in
  `scheduler.rs`.
- Publishes `BuildEvent`s on a `tokio::sync::broadcast` (`events.rs`);
  the `output_subscriber` task drains the bus and renders events
  directly via `BuildEventRenderer` (`output/build_events.rs`).
  `--format human` (default) writes coloured progress to stderr;
  `--format json` writes one JSON event per line to stdout.
- Streams cache decisions host-side (`cache.rs`), reads the workspace
  archive once into memory (`archive.rs` + `source.rs`), and drives
  the Docker daemon via the Bollard wrapper (`docker_client.rs`).
- Owns run-wide cancellation (`tokio_util::sync::CancellationToken`) and shared mutable state
  (`state.rs`) so step plugins can coordinate without reaching across
  module boundaries.

## Plugin system

Plugins are native cdylib shared libraries (`.dylib`/`.so`/`.dll`)
loaded via stabby + libloading. Each plugin exports a stabby
ABI-stable trait object.

- `plugin/registry.rs` — discovery and capability indexing. Scans
  `~/.harmont/plugins/` and `<cwd>/.harmont/plugins/` for dylibs.
  Extra paths can be added via `RegistryConfig.extra_paths`.
- `plugin/host.rs` — `LoadedPlugin`, the loaded-library handle.
- `plugin/host_api.rs` — `HostApiImpl`, the host-side implementation
  of `RawHostApi` (11 methods, `extern "C"`, synchronous). Exposes
  KV storage (plugin/build/step scoped), archive reads, logging,
  cancellation, stdout/stderr writes, build event emission, and
  config file reads.
- `plugin/paths.rs` — filesystem locations for plugin discovery.
- `plugin/install.rs` — `hm plugin install` implementation.
- `plugin/manifest.rs` — manifest validation.

No PluginPool, no embedded plugins, no WASM. Each `LoadedPlugin` is
`Send + Sync` and can be invoked concurrently from multiple tasks.

## Cloud functionality

Every cloud verb runs through the `hm-plugin-cloud` native plugin
under the `hm cloud` namespace: `hm cloud {login,logout,whoami,org,
pipeline,build,job,billing,run}`.

The cloud plugin uses reqwest directly for HTTP and axum for the
browser-loopback OAuth flow. Token storage is file-backed at
`~/.harmont/credentials.toml` (mode 0o600). Persistent state (active
org slug) uses KV storage via the `RawHostApi` trait's `kv_get`/
`kv_set` methods (KvScope::Plugin).

`hm cloud run` is partial: it submits a pre-rendered plan JSON
(default path: `.harmont/plan.json`, override with `--plan-file`).
Source-archive upload to the cloud is future work.

Broadcast lag in `output_subscriber` surfaces a `tracing::warn!` plus
an `eprintln!` line; full lag-recovery (e.g., per-step backpressure)
is a future concern.
