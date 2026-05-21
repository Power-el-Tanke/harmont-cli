# harmont-cli

[![license](https://img.shields.io/crates/l/harmont-cli.svg)](#license)

Run CI pipelines on your own machine, in Docker, from a Python pipeline definition checked into your repo.

Define the pipeline with the [`harmont-py`](https://github.com/harmont-dev/harmont-py) DSL, then `hm run --local` builds a fresh container per chain, runs the steps, and reuses snapshots across runs. The same definition runs unchanged on the hosted [Harmont](https://harmont.dev) cloud via `hm cloud run`.

## Quick start

### 1. Write a pipeline

Pipelines live in `.harmont/<slug>.py`. Save this as `.harmont/hello.py`:

```python
import harmont as hm


@hm.pipeline("hello")
def hello() -> hm.Step:
    return (
        hm.sh("echo 'hello from harmont'", label="hello")
          .sh("uname -a", label="env")
    )
```

### 2. Install

`harmont-cli` is not yet on crates.io. Install from source:

```sh
git clone https://github.com/harmont-dev/harmont-cli
cd harmont-cli
cargo build --release
install -m 0755 target/release/hm /usr/local/bin/hm   # or any dir on $PATH
```

`hm run --local` also needs **Docker** and **Python 3.11+** with [`harmont-py`](https://github.com/harmont-dev/harmont-py):

```sh
git clone https://github.com/harmont-dev/harmont-py
pip install -e ./harmont-py
```

Verify:

```sh
hm --version
```

### 3. Run

From the repo root:

```sh
hm run hello --local
```

The CLI walks `.harmont/*.py`, resolves the `hello` slug, renders the pipeline to JSON, and schedules chains across Docker containers. Forks run in parallel up to `--parallelism N` (default: host CPU count).

If the repo declares only one pipeline, the slug is optional:

```sh
hm run --local
```

## DSL surface

The DSL is small. See [`harmont-py`](https://github.com/harmont-dev/harmont-py) for the full reference.

| Primitive | What it does |
|---|---|
| `hm.sh(cmd, label=...)` | Start a chain with one shell command |
| `.sh(cmd, label=..., cwd=...)` | Chain another command; shares container state with the parent |
| `.fork(label=...)` | Branch a shared base into parallel work |
| `hm.wait()` | Explicit synchronization barrier |
| `@hm.target()` | Reusable, memoized building block |
| `@hm.pipeline("slug")` | Register a pipeline (multiple per file are fine) |

## Common flags

```sh
hm run --local --parallelism 4         # cap concurrent chains
hm run --local --env FOO=bar           # inject env vars
hm run --local --dir path/to/source    # run against a different source root
hm run --format json                   # machine-readable event stream
hm run --help                          # full flag reference
```

## Cloud

`hm cloud <verb>` talks to `api.harmont.dev`. Tokens live in the OS keyring; the active org slug persists under `~/.config/harmont/state/cloud.kv`.

| Command | What it does |
|---|---|
| `hm cloud login` | Browser-loopback OAuth (`--paste` to paste a token) |
| `hm cloud logout` | Forget stored credentials |
| `hm cloud whoami` | Show user + active org |
| `hm cloud org list` / `org use <slug>` | List / pick active org |
| `hm cloud pipeline list` | Pipelines in the active org |
| `hm cloud build list` / `build show <id>` / `build watch <id>` | Inspect builds |
| `hm cloud job show <id>` | Inspect a single job |
| `hm cloud billing show` | Org billing summary |
| `hm cloud run [--plan-file PATH]` | Submit a pre-rendered plan JSON (defaults to `.harmont/plan.json`) |

Source-archive upload for `cloud run` is in progress — pre-render to `.harmont/plan.json` for now.

## Examples

Sixteen idiomatic starter projects (one per toolchain) live under [`examples/`](./examples). Each has a `.harmont/pipeline.py` you can read, copy, and run:

```sh
cd examples/rust
hm run ci --local
```

Languages covered: Rust, Haskell, Go, Python (uv), Java/Kotlin (Gradle), C/C++ (CMake), C#, Ruby, Perl, PHP (Composer + Laravel), OCaml, Zig, npm-based stacks (React, Next.js, TypeScript).

<details>
<summary>A two-branch pipeline using forks and a shared base image</summary>

```python
import harmont as hm


@hm.pipeline("ci")
def ci() -> hm.Step:
    setup = hm.sh(
        "apt-get update && apt-get install -y curl",
        label="apt",
    )
    fetch = setup.fork(label="branch-a").sh(
        "curl -fsSL https://example.com",
        label="fetch",
    )
    work = setup.fork(label="branch-b").sh(
        "echo independent work",
        label="other",
    )
    return hm.pipeline(fetch, work, default_image="ubuntu:24.04")
```

</details>

<details>
<summary>Composing larger pipelines with typed fixture-style targets</summary>

```python
from typing import Annotated

import harmont as hm


@hm.target()
def apt_base(base: Annotated[hm.Step, hm.BaseImage("ubuntu:24.04")]) -> hm.Step:
    return base.sh("apt-get update && apt-get install -y curl", label="apt")


@hm.target()
def smoke(apt_base: hm.Target[hm.Step]) -> hm.Step:
    return apt_base.sh("curl -fsSL https://example.com", label="smoke")


@hm.pipeline("ci")
def ci(smoke: hm.Target[hm.Step]) -> hm.Step:
    return smoke
```

Dependencies are resolved by parameter name from the `@hm.target` registry. `Target[T]` and `Annotated[Step, BaseImage("...")]` both unwrap cleanly under mypy and pyright.

</details>

## Build from source

```sh
git clone https://github.com/harmont-dev/harmont-cli
cd harmont-cli
cargo build
cargo test                          # `local_*` tests need a running Docker daemon
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

The OpenAPI client is generated at build time from the vendored `openapi.json` via [progenitor](https://github.com/oxidecomputer/progenitor).

## Repository layout

Cargo workspace:

- `crates/hm/` — the `hm` binary.
- `crates/hm-plugin-protocol/`, `crates/hm-plugin-sdk/` — public API for third-party plugins.
- `crates/hm-plugin-*` — bundled plugins (Docker executor, output formatters, cloud client).
- `examples/` — sample pipeline repos to `hm run --local` against.

This repo mirrors the `cli/` and `examples/` directories of the private Harmont monorepo. Open issues and PRs here; maintainers land them upstream and a CI sync replays the result back.

## Plugin authoring

`hm` is plugin-driven via [Extism](https://extism.org). To write one:

```sh
cargo new --lib my-plugin
cd my-plugin
cargo add --git https://github.com/harmont-dev/harmont-cli hm-plugin-sdk
cargo build --target wasm32-wasip1 --release
```

Implement one of `StepExecutor`, `SubcommandPlugin`, `LifecycleHook`, or `OutputFormatter`, declare a `PluginManifest`, and call `register_plugin!(...)`. Install with:

```sh
hm plugin install ./target/wasm32-wasip1/release/my_plugin.wasm
```

Built-in output formatters: `human` (default), `json`. Select with `hm run --format <name>`. Working examples live in `crates/hm-fixtures/src/bin/`.

## See also

- [`harmont-py`](https://github.com/harmont-dev/harmont-py) — the Python DSL this CLI consumes.

## License

Dual-licensed under either of

- Apache License, Version 2.0 ([`LICENSE-APACHE`](LICENSE-APACHE))
- MIT license ([`LICENSE-MIT`](LICENSE-MIT))

at your option.
