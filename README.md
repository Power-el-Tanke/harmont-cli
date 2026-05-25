<p align="center">
  <strong>harmont</strong>
</p>

<p align="center">
  <em>Pipelines as code. Run locally in Docker. Ship to cloud when ready.</em>
</p>

<p align="center">
  <a href="https://github.com/harmont-dev/harmont-cli/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/harmont-dev/harmont-cli/ci.yml?branch=main&logo=github" alt="CI"></a>
  <a href="https://crates.io/crates/harmont-cli"><img src="https://img.shields.io/crates/v/harmont-cli?logo=rust" alt="crates.io"></a>
  <a href="https://discord.gg/hm-dev"><img src="https://img.shields.io/discord/1503184719578136576?logo=discord&label=discord" alt="Discord"></a>
  <a href="https://join.slack.com/t/harmont-dev/shared_invite/zt-3yt0tiv7r-qHm1O0p0nVh2GU~KKhUk9A"><img src="https://img.shields.io/badge/slack-join-brightgreen?logo=slack" alt="Slack"></a>
  <a href="#license"><img src="https://img.shields.io/badge/license-MIT%2FApache--2.0-blue" alt="License"></a>
</p>

<p align="center">
  <a href="https://harmont.dev">Website</a> · <a href="https://harmont.dev/docs">Docs</a> · <a href="https://discord.gg/hm-dev">Discord</a> · <a href="https://join.slack.com/t/harmont-dev/shared_invite/zt-3yt0tiv7r-qHm1O0p0nVh2GU~KKhUk9A">Slack</a>
</p>

> [!WARNING]
> Harmont is in **early alpha**. Today it's a powerful local task runner — think `make` or `just`, but with Python-defined pipelines and automatic Docker isolation. The cloud CI/CD platform at [harmont.dev](https://harmont.dev) is under active development. APIs will change. We'd love your feedback — [join the Discord](https://discord.gg/hm-dev).

## What is Harmont?

Harmont lets you define CI/CD pipelines in Python and run them instantly on your machine in Docker containers. No YAML. No waiting for CI. Each pipeline step runs in an isolated container with automatic caching, parallel execution, and reproducible builds.

```python
import harmont as hm
from harmont.python import PythonToolchain

@hm.target()
def project() -> PythonToolchain:
    return hm.python(path=".")

@hm.pipeline(
    "ci",
    default_image="ubuntu:24.04",
    triggers=[hm.push(branch="main")],
)
def ci(project: hm.Target[PythonToolchain]) -> tuple[hm.Step, ...]:
    return (
        project.test(),
        project.lint(),
        project.fmt(),
        project.typecheck(),
    )
```

```sh
curl -fsSL https://get.harmont.dev/install.sh | sh
hm run ci
```

Typed toolchains. Parallel steps. Push triggers. Real Python, not YAML — in two commands.

## Highlights

- **Python-native pipelines** — Full language, not YAML. Loops, conditionals, type checking, IDE autocomplete — it's just Python.
- **Docker isolation** — Every chain runs in a fresh container. No "works on my machine" surprises.
- **Parallel by default** — Forked chains run concurrently, bounded by `--parallelism N`.
- **Snapshot caching** — Container state is snapshotted between steps. Re-runs skip work that hasn't changed.
- **16 starter templates** — Rust, Go, Python, Java, C++, React, Next.js, and more in [`examples/`](./examples).
- **Cloud-ready** — Same pipeline definition runs on [Harmont Cloud](https://harmont.dev) with zero changes (coming soon).

## Install

The recommended way to install Harmont:

```sh
curl -fsSL https://get.harmont.dev/install.sh | sh
```

**Prerequisites:** [Docker](https://docs.docker.com/get-docker/) and Python 3.11+ with the Harmont DSL:

```sh
pip install harmont
```

<details>
<summary>Other installation methods</summary>

### From crates.io

```sh
cargo install harmont-cli
```

### From source

```sh
git clone https://github.com/harmont-dev/harmont-cli
cd harmont-cli
cargo build --release
install -m 0755 target/release/hm /usr/local/bin/hm   # or any dir on $PATH

git clone https://github.com/harmont-dev/harmont-py
pip install -e ./harmont-py
```

</details>

Verify:

```sh
hm --version
```

## Quick Start

### 1. Create a pipeline

Save this as `.harmont/ci.py`:

```python
import harmont as hm

@hm.pipeline("ci")
def ci() -> hm.Step:
    return (
        hm.sh("echo 'hello from harmont'", label="hello")
          .sh("uname -a", label="env")
    )
```

### 2. Run it

```sh
hm run ci
```

If the repo declares only one pipeline, the slug is optional — just `hm run`.

Browse the [16 example projects](./examples) for idiomatic pipelines in Rust, Go, Python, Java, C++, React, Next.js, and more.

## Where we're headed

**Today:** Harmont is a local task runner with Docker isolation. Define pipelines in Python, run them on your machine, get reproducible results every time.

**Tomorrow:** The same pipelines run on [Harmont Cloud](https://harmont.dev) — managed caching, secrets, team dashboards, and zero config changes. One definition, local and cloud.

Want to shape the roadmap? [Join the Discord](https://discord.gg/hm-dev) and tell us what you're building.

## Community

- **Discord** — [discord.gg/hm-dev](https://discord.gg/hm-dev)
- **Slack** — [harmont-dev.slack.com](https://join.slack.com/t/harmont-dev/shared_invite/zt-3yt0tiv7r-qHm1O0p0nVh2GU~KKhUk9A)
- **Website** — [harmont.dev](https://harmont.dev)
- **GitHub Issues** — [harmont-dev/harmont-cli/issues](https://github.com/harmont-dev/harmont-cli/issues)

## Reference

<details>
<summary>DSL surface</summary>

The DSL is small. See [`harmont-py`](https://github.com/harmont-dev/harmont-py) for the full reference.

| Primitive | What it does |
|---|---|
| `hm.sh(cmd, label=...)` | Start a chain with one shell command |
| `.sh(cmd, label=..., cwd=...)` | Chain another command; shares container state with the parent |
| `.fork(label=...)` | Branch a shared base into parallel work |
| `hm.wait()` | Explicit synchronization barrier |
| `@hm.target()` | Reusable, memoized building block |
| `@hm.pipeline("slug")` | Register a pipeline (multiple per file are fine) |

</details>

<details>
<summary>Common flags</summary>

```sh
hm run --parallelism 4         # cap concurrent chains
hm run --env FOO=bar           # inject env vars
hm run --dir path/to/source    # run against a different source root
hm run --format json           # machine-readable event stream
hm run --no-watch              # create the build and exit (don't stream events)
hm run --help                  # full flag reference
```

</details>

<details>
<summary>Cloud commands</summary>

`hm cloud <verb>` talks to `api.harmont.dev`. Credentials are stored at `~/.harmont/credentials.toml`.

| Command | What it does |
|---|---|
| `hm cloud login` | Browser-loopback OAuth (`--paste` to paste a token) |
| `hm cloud logout` | Forget stored credentials |
| `hm cloud whoami` | Show user + active org |
| `hm cloud org switch <slug>` | Set the active organization |
| `hm cloud pipeline list` / `show <slug>` | List or inspect pipelines |
| `hm cloud build list` / `show` / `watch` / `cancel` | Manage builds |
| `hm cloud job list` / `show` | Inspect jobs in a build |
| `hm cloud billing balance` / `usage` / `topup` / `redeem` | Credits and billing |
| `hm cloud run` | Submit a pipeline run to the cloud |

</details>

<details>
<summary>Plugin authoring</summary>

`hm` is plugin-driven via [Extism](https://extism.org). Start a `cdylib` crate with the SDK:

```sh
cargo new --lib my-plugin
cd my-plugin
cargo add --git https://github.com/harmont-dev/harmont-cli hm-plugin-sdk
```

Implement `StepExecutor`, `SubcommandPlugin`, `LifecycleHook`, or `OutputFormatter`, declare a `PluginManifest`, and call `register_plugin!(...)`. Build to WebAssembly and install:

```sh
cargo build --target wasm32-wasip1 --release
hm plugin install ./target/wasm32-wasip1/release/my_plugin.wasm
```

Built-in output formatters: `human` (default), `json`. Select with `hm run --format <name>`.

</details>

## Contributing

<details>
<summary>Development setup</summary>

```sh
git clone https://github.com/harmont-dev/harmont-cli
cd harmont-cli
cargo build
cargo test                          # local_* tests need a running Docker daemon
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

The OpenAPI client is generated at build time from the vendored `openapi.json` via [progenitor](https://github.com/oxidecomputer/progenitor).

</details>

## License

Dual-licensed under either of

- Apache License, Version 2.0 ([`LICENSE-APACHE`](LICENSE-APACHE))
- MIT license ([`LICENSE-MIT`](LICENSE-MIT))

at your option.
