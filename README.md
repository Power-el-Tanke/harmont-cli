# harmont

[![CI](https://img.shields.io/github/actions/workflow/status/harmont-dev/harmont-cli/ci.yml?branch=main&logo=github)](https://github.com/harmont-dev/harmont-cli/actions)
[![crates.io](https://img.shields.io/crates/v/harmont-cli?logo=rust)](https://crates.io/crates/harmont-cli)
[![PyPI](https://img.shields.io/pypi/v/harmont?logo=python&logoColor=white)](https://pypi.org/project/harmont/)
[![license](https://img.shields.io/crates/l/harmont-cli.svg)](#license)

Run CI pipelines locally, in Docker, from a pipeline definition checked into your repo. The same definition runs unchanged on [Harmont Cloud](https://harmont.dev).

Define pipelines in Python or TypeScript. `hm run` builds a fresh container per chain, runs the steps, caches snapshots across runs, and parallelizes forks automatically.

## Quick start

### 1. Install

```sh
cargo install harmont-cli
pip install harmont          # Python DSL
```

Docker and Python 3.11+ required. [Build from source →](#build-from-source)

### 2. Write a pipeline

Save as `.harmont/ci.py`:

```python
import harmont as hm


@hm.pipeline("ci")
def ci() -> hm.Step:
    return (
        hm.sh("echo 'hello from harmont'", label="hello")
          .sh("uname -a", label="env")
    )
```

### 3. Run

```sh
hm run ci
```

Walks `.harmont/*.py`, resolves the slug, renders to IR, schedules chains across Docker containers. Forks run in parallel up to `--parallelism N` (default: CPU count).

## Features

- **Local-first CI** — run pipelines on your machine before pushing
- **Docker isolation** — fresh container per chain, deterministic builds
- **Snapshot caching** — container snapshots reuse across runs, skipping redundant work
- **Automatic parallelism** — forked chains run concurrently
- **Cloud parity** — `hm cloud run` executes the same definition on Harmont Cloud
- **Plugin system** — extend with custom executors, formatters, and lifecycle hooks via Extism/WASM
- **Two DSLs** — Python and TypeScript, both compile to the same IR

## Toolchain helpers

Built-in helpers eliminate boilerplate for common language toolchains:

```python
rust    = hm.rust("1.80.0")      # build, test, clippy, fmt, doc
go      = hm.go("1.22.0")        # build, test, vet, fmt
python  = hm.python()            # test, lint, fmt, typecheck (uv)
npm     = hm.npm()               # install, run, test, lint, fmt
gradle  = hm.gradle(jdk=21)      # build, test, lint (Java/Kotlin)
dotnet  = hm.dotnet()            # build, test, fmt (C#)
cmake   = hm.cmake(lang="cpp")   # configure, build, test, fmt
zig     = hm.zig("0.13.0")       # build, test, fmt
haskell = hm.haskell("9.6.7")    # build, test, lint, fmt
ocaml   = hm.ocaml()             # build, test, fmt
elm     = hm.elm()               # make, test, review, fmt
```

Each handles installation, dependency caching, and smart defaults. Or use `hm.sh()` for anything custom.

## DSL surface

The DSL is small. See [`harmont-py`](https://github.com/harmont-dev/harmont-py) for the full reference.

| Primitive | What it does |
|---|---|
| `hm.sh(cmd, label=...)` | Start a chain with one shell command |
| `.sh(cmd, label=..., cwd=...)` | Chain another command (shares container state) |
| `.fork(label=...)` | Branch into parallel work |
| `hm.wait()` | Explicit synchronization barrier |
| `@hm.target()` | Reusable, memoized building block |
| `@hm.pipeline("slug")` | Register a pipeline (multiple per file OK) |

## CLI reference

```sh
hm run                             # run the only pipeline
hm run ci                          # run by slug
hm run --parallelism 4             # cap concurrent chains
hm run --env FOO=bar               # inject env vars
hm run --dir path/to/source        # different source root
hm run --format json               # machine-readable event stream
hm run --no-watch                  # submit and exit
hm run --help                      # full flag reference
```

## Cloud

`hm cloud <verb>` talks to `api.harmont.dev`. Credentials stored at `~/.harmont/credentials.toml`.

| Command | What it does |
|---|---|
| `hm cloud login` | Browser-loopback OAuth (`--paste` for token) |
| `hm cloud logout` | Forget credentials |
| `hm cloud whoami` | Show user + active org |
| `hm cloud org switch <slug>` | Set active organization |
| `hm cloud pipeline list` / `show <slug>` | List or inspect pipelines |
| `hm cloud build list -p <slug>` | List builds |
| `hm cloud build show` / `watch` / `cancel` | Inspect or control a build |
| `hm cloud run` | Submit a build to the cloud |

<details>
<summary>Billing</summary>

| Command | What it does |
|---|---|
| `hm cloud billing balance` | Credit balance |
| `hm cloud billing transactions` | Transaction history |
| `hm cloud billing usage` | Usage over time window |
| `hm cloud billing topup` | Top up credits |
| `hm cloud billing redeem` | Redeem promo code |

</details>

## Examples

Starter projects live under [`examples/`](./examples). Each has a `.harmont/pipeline.py` ready to run:

```sh
cd examples/rust && hm run ci
```

**Languages:** Rust, Go, Python, TypeScript, React, Next.js, Java, Kotlin, C, C++, C#, OCaml, Zig — plus a [Zig+JS monorepo](./examples/zig-js) demonstrating fork/join parallelism with shared toolchain installs.

<details>
<summary>Fork-based pipeline with a shared base image</summary>

```python
import harmont as hm


@hm.pipeline("ci")
def ci() -> hm.Step:
    setup = hm.sh("apt-get update && apt-get install -y curl", label="apt")
    fetch = setup.fork(label="branch-a").sh("curl -fsSL https://example.com", label="fetch")
    work  = setup.fork(label="branch-b").sh("echo independent work", label="other")
    return hm.pipeline(fetch, work, default_image="ubuntu:24.04")
```

</details>

<details>
<summary>Composing pipelines with typed targets</summary>

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

Dependencies resolved by parameter name. `Target[T]` and `Annotated[Step, BaseImage("...")]` both type-check under mypy/pyright.

</details>

## Plugin authoring

`hm` is plugin-driven via [Extism](https://extism.org). Start a `cdylib` crate:

```sh
cargo new --lib my-plugin && cd my-plugin
cargo add --git https://github.com/harmont-dev/harmont-cli hm-plugin-sdk
```

Implement `StepExecutor`, `SubcommandPlugin`, `LifecycleHook`, or `OutputFormatter`. Build to WASM and install:

```sh
cargo build --target wasm32-wasip1 --release
hm plugin install ./target/wasm32-wasip1/release/my_plugin.wasm
```

## Build from source

```sh
git clone https://github.com/harmont-dev/harmont-cli && cd harmont-cli
cargo build
cargo test                          # local_* tests need Docker
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

OpenAPI client generated at build time from vendored `openapi.json` via [progenitor](https://github.com/oxidecomputer/progenitor).

## Repository layout

| Path | What |
|---|---|
| `crates/hm/` | CLI binary |
| `crates/hm-pipeline-ir/` | Pipeline IR schema (serde structs) |
| `crates/hm-plugin-protocol/`, `hm-plugin-sdk/` | Plugin API for third-party authors |
| `crates/hm-plugin-cloud/` | Bundled cloud client plugin |
| `dsls/harmont-py/` | Python DSL |
| `dsls/harmont-ts/` | TypeScript DSL |
| `examples/` | Starter pipeline repos |

## See also

- [`harmont-py`](https://github.com/harmont-dev/harmont-py) — Python DSL

## License

Dual-licensed: [Apache-2.0](LICENSE-APACHE) OR [MIT](LICENSE-MIT).
