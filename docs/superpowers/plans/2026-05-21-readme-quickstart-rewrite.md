# README Quickstart Rewrite Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace `harmont-cli/README.md` and `harmont-py/README.md` with concise quickstart-first guides modeled on the [`hsrs` README](https://github.com/harmont-dev/hsrs) — tagline → numbered quick start → small surface tables → details collapsed.

**Architecture:** Each README is a single Markdown file. Both READMEs are rewritten to share a parallel structure: one-sentence tagline, an elaboration paragraph, a `## Quick start` with three numbered subsections (write a pipeline → install → run), a `## DSL surface` table, a `## What you can build` table or list, and a collapsed `<details>` "Full example" block. Repository-layout, plugin-authoring, and build-from-source content is demoted to short sections at the bottom (or linked out). Cross-references between the two READMEs are bidirectional and consistent.

**Tech Stack:** Pure Markdown (CommonMark + GFM tables and `<details>` blocks). No code or tests change. Verification is by visual reading and link/anchor checks.

---

## File Structure

- Modify: `/home/marko/harmont-cli/README.md` — full replacement.
- Modify: `/home/marko/harmont-py/README.md` — full replacement.
- Plan lives in: `/home/marko/harmont-cli/docs/superpowers/plans/2026-05-21-readme-quickstart-rewrite.md` (this file).

No new files. No code changes. No test changes.

---

### Task 1: Rewrite `harmont-cli/README.md`

**Files:**
- Modify: `/home/marko/harmont-cli/README.md` (full replacement, currently 230 lines)

- [ ] **Step 1: Read current README**

Run: `cat /home/marko/harmont-cli/README.md`

Confirms the file you're about to replace. No assertion — just orient.

- [ ] **Step 2: Replace the file with the content below**

Use the Write tool. The full replacement content is:

````markdown
# harmont-cli

[![license](https://img.shields.io/crates/l/harmont-cli.svg)](#license)

Run CI pipelines on your own machine, in Docker, from a Python pipeline definition checked into your repo.

Define the pipeline with the [`harmont-py`](https://github.com/harmont-dev/harmont-py) DSL, then `hm run` builds a fresh container per chain, runs the steps, and reuses snapshots across runs. The same definition runs unchanged on the hosted [Harmont](https://harmont.dev) cloud via `hm cloud run`.

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

`hm run` also needs **Docker** and **Python 3.11+** with [`harmont-py`](https://github.com/harmont-dev/harmont-py):

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
hm run hello
```

The CLI walks `.harmont/*.py`, resolves the `hello` slug, renders the pipeline to JSON, and schedules chains across Docker containers. Forks run in parallel up to `--parallelism N` (default: host CPU count).

If the repo declares only one pipeline, the slug is optional:

```sh
hm run
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
hm run --parallelism 4         # cap concurrent chains
hm run --env FOO=bar           # inject env vars
hm run --dir path/to/source    # run against a different source root
hm run --format json           # machine-readable event stream
hm run --no-watch              # create the build and exit (don't stream events)
hm run --help                  # full flag reference
```

## Cloud

`hm cloud <verb>` talks to `api.harmont.dev`. Credentials are stored file-backed at `~/.harmont/credentials.toml` (mode `0600`); the active org slug persists under `~/.config/harmont/state/harmont-cloud.kv`.

| Command | What it does |
|---|---|
| `hm cloud login` | Browser-loopback OAuth (`--paste` to paste a token) |
| `hm cloud logout` | Forget stored credentials |
| `hm cloud whoami` | Show user + active org |
| `hm cloud org switch <slug>` | Set the active organization |
| `hm cloud pipeline list` / `pipeline show <slug>` | List or inspect pipelines |
| `hm cloud build list -p <slug>` | List builds for a pipeline |
| `hm cloud build show -p <slug> <n>` / `watch -p <slug> <n>` / `cancel -p <slug> <n>` | Inspect or control a build |
| `hm cloud job list -p <slug> -b <n>` / `job show -p <slug> -b <n> <id>` | Inspect jobs in a build |
| `hm cloud billing balance` / `transactions` / `usage` / `topup` / `redeem` | Credit balance, transaction log, usage, top-ups, redeem codes |
| `hm cloud run [--plan-file PATH]` | Submit a pre-rendered plan JSON (defaults to `.harmont/plan.json`) |

Source-archive upload for `cloud run` is in progress — pre-render to `.harmont/plan.json` for now.

## Examples

Eighteen idiomatic starter projects live under [`examples/`](./examples). Each has a `.harmont/pipeline.py` you can read, copy, and run:

```sh
cd examples/rust
hm run ci
```

Toolchains covered: Rust, Haskell, Go, Python (uv), Java/Kotlin (Gradle), C and C++ (CMake), C# (dotnet), Ruby, Perl, PHP (Composer + Laravel), OCaml, Zig, Zig+JS monorepo, and npm-based stacks (React, Next.js, TypeScript).

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
- `examples/` — sample pipeline repos to `hm run` against.

This repo mirrors the `cli/` and `examples/` directories of the private Harmont monorepo. Open issues and PRs here; maintainers land them upstream and a CI sync replays the result back.

## Plugin authoring

`hm` is plugin-driven via [Extism](https://extism.org). To write one, start a `cdylib` crate and depend on the SDK:

```sh
cargo new --lib my-plugin
cd my-plugin
cargo add --git https://github.com/harmont-dev/harmont-cli hm-plugin-sdk
```

Implement one of `StepExecutor`, `SubcommandPlugin`, `LifecycleHook`, or `OutputFormatter`, declare a `PluginManifest`, and call `register_plugin!(...)`. Then build to WebAssembly:

```sh
cargo build --target wasm32-wasip1 --release
```

Install the resulting `.wasm`:

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
````

- [ ] **Step 3: Verify Markdown renders cleanly**

Run: `grep -nE '^#{1,6} ' /home/marko/harmont-cli/README.md`

Expected: exactly one `# harmont-cli`, exactly one each of `## Quick start`, `## DSL surface`, `## Common flags`, `## Cloud`, `## Examples`, `## Build from source`, `## Repository layout`, `## Plugin authoring`, `## See also`, `## License`. Subheaders `### 1. Write a pipeline`, `### 2. Install`, `### 3. Run` appear in that order.

- [ ] **Step 4: Verify internal links resolve**

Run: `grep -oE '\]\([^)]+\)' /home/marko/harmont-cli/README.md | sort -u`

Expected: every relative link (`./examples`, `LICENSE-APACHE`, `LICENSE-MIT`) corresponds to a file or directory that exists. Spot-check with:

```sh
ls /home/marko/harmont-cli/examples
ls /home/marko/harmont-cli/LICENSE-APACHE /home/marko/harmont-cli/LICENSE-MIT
```

Expected: all paths exist.

- [ ] **Step 5: Verify code blocks balance**

Run: `grep -cE '^```' /home/marko/harmont-cli/README.md`

Expected: an even integer (every opening fence has a closing fence).

- [ ] **Step 6: Commit**

```sh
cd /home/marko/harmont-cli
git add README.md
git commit -m "docs(readme): rewrite as quickstart-first guide"
```

---

### Task 2: Rewrite `harmont-py/README.md`

**Files:**
- Modify: `/home/marko/harmont-py/README.md` (full replacement, currently 146 lines)

- [ ] **Step 1: Read current README**

Run: `cat /home/marko/harmont-py/README.md`

Orient against the existing structure before replacement.

- [ ] **Step 2: Replace the file with the content below**

Use the Write tool. The full replacement content is:

````markdown
# harmont-py

[![license](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Python DSL for defining [Harmont](https://harmont.dev) CI pipelines.

Pipelines are chains of shell commands, branched with `.fork()`, synchronized with `hm.wait()`, registered with a decorator, and rendered to a JSON IR. The companion [`harmont-cli`](https://github.com/harmont-dev/harmont-cli) consumes that IR and runs the pipeline locally in Docker or on the hosted Harmont cloud.

The package installs as `harmont` and you import it as `harmont`:

```python
import harmont as hm
```

## Quick start

### 1. Write a pipeline

A pipeline file lives at `.harmont/<slug>.py` in your repo:

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

Not yet on PyPI. Install from source (Python 3.11+):

```sh
git clone https://github.com/harmont-dev/harmont-py
cd harmont-py
pip install -e .
```

Development extras (pytest, mypy, ruff):

```sh
pip install -e '.[dev]'
```

### 3. Run

Use the [Harmont CLI](https://github.com/harmont-dev/harmont-cli):

```sh
hm run hello
```

`hm run` walks `.harmont/*.py`, imports each file (triggering the decorators), renders the registered pipeline to JSON, and executes it in Docker.

## DSL surface

| Primitive | Returns | What it does |
|---|---|---|
| `hm.sh(cmd, cwd=..., label=...)` | `Step` | Start a chain in one call (= `hm.scratch().sh(cmd, ...)`) |
| `hm.scratch()` | `Step` | Empty root; chain with `.sh(...)` for an explicit start |
| `Step.sh(cmd, cwd=..., ...)` | `Step` | Run a shell command; chained `.sh` shares container state |
| `Step.fork(label=...)` | `Step` | Branch a shared base into parallel work |
| `hm.wait()` | `Step` | Explicit synchronization barrier |
| `@hm.target()` | decorator | Reusable, memoized building block |
| `@hm.pipeline("slug")` | decorator | Register a pipeline (multiple per file are fine) |
| `hm.pipeline(*leaves, env=...)` | `dict` | Factory form — build the v0 IR dict directly (used in tests) |

Cache policies (`hm.ttl`, `hm.on_change`, `hm.forever`, `hm.compose`), triggers (`hm.push`, `hm.pull_request`, `hm.schedule`), and matrix axes are documented in the module docstrings; start at `harmont/__init__.py`.

## Language toolchains

`harmont` ships first-class wrappers for the common toolchains. Each exposes the actions that make sense for that ecosystem (e.g. `.build()`, `.test()`, `.clippy()`, `.fmt()` for Rust; `.test()`, `.lint()`, `.fmt()`, `.typecheck()` for Python):

| Call | Project type |
|---|---|
| `hm.rust(path=..., version="stable")` | cargo + clippy + rustfmt |
| `hm.haskell(ghc=..., cabal="latest")` | cabal (call `.package(path)` for a package) |
| `hm.python(path=..., uv_version="latest")` | uv-based Python project |
| `hm.go(path=..., version="1.23.2")` | go build/test/vet/fmt |
| `hm.npm(path=..., version="20")` | npm + arbitrary scripts |
| `hm.gradle(path=..., jdk="21", kotlin=False)` | Java or Kotlin via Gradle |
| `hm.cmake(path=..., lang="c"\|"cpp")` | C/C++ via CMake + CTest |
| `hm.dotnet(path=..., channel="8.0")` | .NET via dotnet CLI |
| `hm.ruby(path=..., version="default")` | Bundler + Rake |
| `hm.ocaml(path=..., compiler="5.1.1")` | opam + Dune |
| `hm.zig(version=..., ...)` | zig build/test/fmt |
| `hm.perl(path=...)` | cpanm + prove |
| `hm.composer(path=..., laravel=False)` | PHP / Laravel via Composer |
| `hm.elm(path=..., elm_version="0.19.1")` | Elm |

Working examples for each toolchain live in [`harmont-cli/examples/`](https://github.com/harmont-dev/harmont-cli/tree/main/examples).

## Composing with targets

For larger pipelines, factor toolchain setup into `@hm.target()` and let pipelines depend on them by parameter name. `Target[T]` and `Annotated[Step, BaseImage("...")]` are typed markers that unwrap cleanly under mypy and pyright.

```python
from typing import Annotated

import harmont as hm
from harmont.haskell import HaskellPackage, HaskellToolchain


@hm.target()
def apt_base(base: Annotated[hm.Step, hm.BaseImage("ubuntu-24.04")]) -> hm.Step:
    return base.sh("apt-get update").sh("apt-get install -y python3")


@hm.target()
def api(ghc: hm.Target[HaskellToolchain]) -> HaskellPackage:
    return ghc.cabal(path="api")


@hm.pipeline("ci")
def ci(
    apt_base: hm.Target[hm.Step],
    api: hm.Target[HaskellPackage],
) -> tuple[hm.Step, ...]:
    return (apt_base.sh("./run-smoke"), api)
```

Every fixture parameter must carry a marker or default value; unmarked parameters raise at decoration time. Memoization scope is one `dump_registry_json` render, so two targets that depend on the same `apt_base` share a single step.

<details>
<summary>How rendering works</summary>

`hm.sh(...).sh(...)` builds a chain of frozen `Step` dataclasses. Each `.sh()` returns a new `Step` carrying the parent reference. The `hm.pipeline()` factory walks back from each leaf, topo-sorts, and emits a `version: "0"` IR dict matching the schema in `harmont-pipeline` (Haskell side).

When used as a decorator, `@hm.pipeline("slug")` registers the wrapped function with a module-level registry. `hm.dump_registry_json()` walks every `.harmont/*.py`, imports each (which triggers the decorators), and returns the full envelope.

A chain edge — `parent.sh(cmd, ...)` — emits `builds_in: "<parent key>"` in the v0 IR JSON. The edge encodes synchronisation and state inheritance: the local executor reuses the parent's container; the cloud planner boots from its snapshot. A step rooted at `scratch()` has `builds_in: null` and boots from `image="..."` (or the pipeline's `default_image`) locally; the cloud planner ignores `image` (it always boots from the Freestyle base).

The JSON wire format and cache-key algorithm are stable; see module docstrings under `harmont/` for the contract.

</details>

## Build & test

```sh
python3 -m venv .venv && source .venv/bin/activate
pip install -e '.[dev]'

pytest                                  # all tests
pytest -v --tb=short
mypy --strict harmont
ruff check .
```

`pytest` is configured to treat warnings as errors (`filterwarnings = ["error"]`).

## See also

- [`harmont-cli`](https://github.com/harmont-dev/harmont-cli) — the CLI that runs pipelines defined with this package (`hm run`).

## License

MIT. See [`LICENSE`](LICENSE).
````

- [ ] **Step 3: Verify Markdown structure**

Run: `grep -nE '^#{1,6} ' /home/marko/harmont-py/README.md`

Expected: exactly one `# harmont-py`, exactly one each of `## Quick start`, `## DSL surface`, `## Language toolchains`, `## Composing with targets`, `## Build & test`, `## See also`, `## License`. Subheaders `### 1. Write a pipeline`, `### 2. Install`, `### 3. Run` appear in that order.

- [ ] **Step 4: Verify internal links resolve**

Run: `ls /home/marko/harmont-py/LICENSE`

Expected: file exists.

- [ ] **Step 5: Verify code blocks balance**

Run: `grep -cE '^```' /home/marko/harmont-py/README.md`

Expected: an even integer.

- [ ] **Step 6: Verify every toolchain row matches a real module**

Run: `ls /home/marko/harmont-py/harmont/{rust,haskell,python,go,npm,gradle,cmake,dotnet,ruby,ocaml,zig,perl,composer,elm}.py`

Expected: all 14 files exist (one per row in the "Language toolchains" table).

- [ ] **Step 7: Commit**

```sh
cd /home/marko/harmont-py
git add README.md
git commit -m "docs(readme): rewrite as quickstart-first guide"
```

---

### Task 3: Cross-check the two READMEs are consistent

**Files:**
- Read: `/home/marko/harmont-cli/README.md`
- Read: `/home/marko/harmont-py/README.md`

- [ ] **Step 1: Both READMEs link to each other**

Run:

```sh
grep -c 'harmont-py' /home/marko/harmont-cli/README.md
grep -c 'harmont-cli' /home/marko/harmont-py/README.md
```

Expected: both counts ≥ 2 (each appears at least in the intro paragraph and in `## See also`).

- [ ] **Step 2: The `hello` pipeline example matches semantically in both READMEs**

Both READMEs intentionally show the same `hello` pipeline (decorator name, slug, commands, labels). Indentation differs across files (two-space continuation in cli, four-space in py) to match each repo's existing house style. Verify the substance, not the bytes:

```sh
grep -E '@hm\.pipeline\("hello"\)|echo .hello from harmont.|uname -a' /home/marko/harmont-cli/README.md
grep -E '@hm\.pipeline\("hello"\)|echo .hello from harmont.|uname -a' /home/marko/harmont-py/README.md
```

Expected: each command emits three matching lines (decorator, echo, uname).

- [ ] **Step 3: No reference to a function, flag, or module that does not exist**

Run:

```sh
grep -nE 'hm\.(sh|scratch|wait|target|pipeline|push|pull_request|schedule|ttl|on_change|forever|compose|BaseImage|Target|rust|haskell|python|go|npm|gradle|cmake|dotnet|ruby|ocaml|zig|perl|composer|elm)' \
  /home/marko/harmont-cli/README.md /home/marko/harmont-py/README.md
```

Spot-check that each referenced symbol exists in `/home/marko/harmont-py/harmont/__init__.py` or one of the per-toolchain modules.

Quick check:

```sh
grep -E '^(from|import|__all__|def |class )' /home/marko/harmont-py/harmont/__init__.py | head -80
```

Expected: every symbol used in either README appears in the public surface listed there.

- [ ] **Step 4: Commit any reconciliation edits**

If Step 2 or Step 3 forced edits to either README, stage and commit them in each repo:

```sh
cd /home/marko/harmont-cli && git status --short
cd /home/marko/harmont-py  && git status --short
```

If anything is unstaged, commit per-repo with `docs(readme): reconcile cross-references`. If both are clean, skip — no commit needed.

---

## Self-review notes (for the implementer)

- The plan does not introduce code, types, tests, or CI changes. Every step is either a Markdown rewrite, a `grep`/`ls`/`diff` verification, or a commit.
- The drafts in Tasks 1 and 2 are the actual final content — copy them into the Write tool verbatim. Do not paraphrase.
- The hsrs-inspired shape: one-sentence tagline → elaboration paragraph → numbered quick start → small surface tables → collapsed details → footer (build, layout, plugin authoring, license).
- Plugin authoring stays in the cli README but is moved below the user-facing flow. It is brief because the canonical reference is the SDK crate's own README.
- Cross-references between cli ↔ py are bidirectional and appear in both the intro paragraph and the `## See also` footer.
