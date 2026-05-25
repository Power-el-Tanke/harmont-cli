# README Overhaul + Language Pruning

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Transform the README from a dry reference doc into a compelling, scannable landing page — and prune half-baked language toolchains (Perl, Ruby, PHP/Composer) from both DSLs and examples.

**Architecture:** Two-phase approach. Phase 1 removes dead weight (3 thin toolchains + their examples/tests). Phase 2 rewrites the README following patterns from top-tier CLI repos (Ruff, Bun, bat, Deno). The hsrs repo's badge style (5 shields.io badges, left-aligned, no centering) is our visual reference.

**Tech Stack:** Markdown, shields.io badges, Python DSL (`harmont-py`), TypeScript DSL (`harmont-ts`)

---

## Phase 1: Prune Half-Baked Languages

These toolchains are thin wrappers (2 methods each, ~100 LOC) that provide negligible value over raw `hm.sh()`. Ruby's version parameter literally raises `NotImplementedError`.

### Task 1: Remove Perl Toolchain

**Files:**
- Delete: `dsls/harmont-py/harmont/perl.py`
- Delete: `dsls/harmont-py/tests/test_perl.py`
- Delete: `dsls/harmont-ts/src/toolchains/perl.ts`
- Delete: `dsls/harmont-ts/tests/toolchains/perl.test.ts`
- Modify: `dsls/harmont-py/harmont/__init__.py` (remove `perl` export)
- Modify: `dsls/harmont-ts/src/index.ts` (remove `perl` export)
- Modify: `dsls/harmont-ts/src/toolchains/index.ts` (remove `perl` re-export if present)
- Delete: `examples/perl/` (entire directory)

**Step 1: Delete Perl toolchain files from Python DSL**

```bash
rm dsls/harmont-py/harmont/perl.py dsls/harmont-py/tests/test_perl.py
```

**Step 2: Delete Perl toolchain files from TypeScript DSL**

```bash
rm dsls/harmont-ts/src/toolchains/perl.ts dsls/harmont-ts/tests/toolchains/perl.test.ts
```

**Step 3: Delete Perl example**

```bash
rm -rf examples/perl/
```

**Step 4: Remove `perl` from Python DSL exports**

Open `dsls/harmont-py/harmont/__init__.py`, find and remove the `perl` import/export line.

**Step 5: Remove `perl` from TypeScript DSL exports**

Open `dsls/harmont-ts/src/index.ts` and `dsls/harmont-ts/src/toolchains/index.ts`, remove `perl` re-exports.

**Step 6: Run Python DSL tests to verify nothing breaks**

```bash
cd dsls/harmont-py && python -m pytest tests/ -x -q
```
Expected: All pass, no import errors referencing perl.

**Step 7: Run TypeScript DSL tests to verify nothing breaks**

```bash
cd dsls/harmont-ts && npm test
```
Expected: All pass.

**Step 8: Commit**

```bash
git add -A dsls/harmont-py/harmont/perl.py dsls/harmont-py/tests/test_perl.py \
  dsls/harmont-ts/src/toolchains/perl.ts dsls/harmont-ts/tests/toolchains/perl.test.ts \
  examples/perl/ dsls/harmont-py/harmont/__init__.py dsls/harmont-ts/src/index.ts \
  dsls/harmont-ts/src/toolchains/index.ts
git commit -m "chore: remove Perl toolchain (thin wrapper, no real value over hm.sh)"
```

---

### Task 2: Remove Ruby Toolchain

**Files:**
- Delete: `dsls/harmont-py/harmont/ruby.py`
- Delete: `dsls/harmont-py/tests/test_ruby.py`
- Delete: `dsls/harmont-ts/src/toolchains/ruby.ts`
- Delete: `dsls/harmont-ts/tests/toolchains/ruby.test.ts`
- Modify: `dsls/harmont-py/harmont/__init__.py` (remove `ruby` export)
- Modify: `dsls/harmont-ts/src/index.ts` (remove `ruby` export)
- Modify: `dsls/harmont-ts/src/toolchains/index.ts` (remove `ruby` re-export if present)
- Delete: `examples/ruby/` (entire directory)

**Step 1: Delete Ruby toolchain files from Python DSL**

```bash
rm dsls/harmont-py/harmont/ruby.py dsls/harmont-py/tests/test_ruby.py
```

**Step 2: Delete Ruby toolchain files from TypeScript DSL**

```bash
rm dsls/harmont-ts/src/toolchains/ruby.ts dsls/harmont-ts/tests/toolchains/ruby.test.ts
```

**Step 3: Delete Ruby example**

```bash
rm -rf examples/ruby/
```

**Step 4: Remove `ruby` from Python DSL exports**

Open `dsls/harmont-py/harmont/__init__.py`, find and remove the `ruby` import/export.

**Step 5: Remove `ruby` from TypeScript DSL exports**

Open `dsls/harmont-ts/src/index.ts` and `dsls/harmont-ts/src/toolchains/index.ts`, remove `ruby` re-exports.

**Step 6: Run Python DSL tests**

```bash
cd dsls/harmont-py && python -m pytest tests/ -x -q
```
Expected: All pass.

**Step 7: Run TypeScript DSL tests**

```bash
cd dsls/harmont-ts && npm test
```
Expected: All pass.

**Step 8: Commit**

```bash
git add -A dsls/harmont-py/harmont/ruby.py dsls/harmont-py/tests/test_ruby.py \
  dsls/harmont-ts/src/toolchains/ruby.ts dsls/harmont-ts/tests/toolchains/ruby.test.ts \
  examples/ruby/ dsls/harmont-py/harmont/__init__.py dsls/harmont-ts/src/index.ts \
  dsls/harmont-ts/src/toolchains/index.ts
git commit -m "chore: remove Ruby toolchain (version pinning unimplemented, thin wrapper)"
```

---

### Task 3: Remove Composer/PHP Toolchain

**Files:**
- Delete: `dsls/harmont-py/harmont/composer.py`
- Delete: `dsls/harmont-py/tests/test_composer.py`
- Delete: `dsls/harmont-ts/src/toolchains/composer.ts`
- Delete: `dsls/harmont-ts/tests/toolchains/composer.test.ts`
- Modify: `dsls/harmont-py/harmont/__init__.py` (remove `composer` export)
- Modify: `dsls/harmont-ts/src/index.ts` (remove `composer` export)
- Modify: `dsls/harmont-ts/src/toolchains/index.ts` (remove `composer` re-export if present)
- No example directory to delete (no PHP example exists).

**Step 1: Delete Composer toolchain files from Python DSL**

```bash
rm dsls/harmont-py/harmont/composer.py dsls/harmont-py/tests/test_composer.py
```

**Step 2: Delete Composer toolchain files from TypeScript DSL**

```bash
rm dsls/harmont-ts/src/toolchains/composer.ts dsls/harmont-ts/tests/toolchains/composer.test.ts
```

**Step 3: Remove `composer` from Python DSL exports**

Open `dsls/harmont-py/harmont/__init__.py`, find and remove the `composer` import/export.

**Step 4: Remove `composer` from TypeScript DSL exports**

Open `dsls/harmont-ts/src/index.ts` and `dsls/harmont-ts/src/toolchains/index.ts`, remove `composer` re-exports.

**Step 5: Run Python DSL tests**

```bash
cd dsls/harmont-py && python -m pytest tests/ -x -q
```
Expected: All pass.

**Step 6: Run TypeScript DSL tests**

```bash
cd dsls/harmont-ts && npm test
```
Expected: All pass.

**Step 7: Commit**

```bash
git add -A dsls/harmont-py/harmont/composer.py dsls/harmont-py/tests/test_composer.py \
  dsls/harmont-ts/src/toolchains/composer.ts dsls/harmont-ts/tests/toolchains/composer.test.ts \
  dsls/harmont-py/harmont/__init__.py dsls/harmont-ts/src/index.ts \
  dsls/harmont-ts/src/toolchains/index.ts
git commit -m "chore: remove Composer/PHP toolchain (thin wrapper, minimal value)"
```

---

### Task 4: Check for stale references to removed languages

**Step 1: Grep for remaining references**

```bash
grep -rn "perl\|ruby\|composer\|laravel\|php" --include="*.py" --include="*.ts" --include="*.md" \
  dsls/ examples/ README.md | grep -vi "copyright\|license\|perl5\|perlcritic"
```

**Step 2: Fix any stale references found**

Remove or update any remaining mentions in docs, comments, or configuration files.

**Step 3: Commit if changes were needed**

```bash
git add -A && git commit -m "chore: clean up stale references to removed toolchains"
```

---

## Phase 2: README Overhaul

### Task 5: Write new README structure

Replace the entire `README.md` with the new structure below. This is the core task.

**Files:**
- Rewrite: `README.md`

**Step 1: Write the new README**

The new README follows this structure (modeled after hsrs, Ruff, Bun):

```markdown
# harmont

[![CI](https://img.shields.io/github/actions/workflow/status/harmont-dev/harmont-cli/ci.yml?branch=main&logo=github)](https://github.com/harmont-dev/harmont-cli/actions)
[![crates.io](https://img.shields.io/crates/v/harmont-cli?logo=rust)](https://crates.io/crates/harmont-cli)
[![PyPI](https://img.shields.io/pypi/v/harmont?logo=python&logoColor=white)](https://pypi.org/project/harmont/)
[![license](https://img.shields.io/crates/l/harmont-cli.svg)](LICENSE-MIT)
[![Discord](https://img.shields.io/discord/DISCORD_ID?logo=discord)](https://discord.gg/INVITE)

Run CI pipelines locally, in Docker, from a pipeline definition checked into your repo. The same definition runs unchanged on [Harmont Cloud](https://harmont.dev).

Define pipelines in Python or TypeScript. `hm run` builds a fresh container per chain, runs the steps, caches snapshots across runs, and parallelizes forks automatically.

## Quick start

### 1. Install

\```sh
cargo install harmont-cli
pip install harmont          # Python DSL
\```

Docker and Python 3.11+ required. [Build from source →](#build-from-source)

### 2. Write a pipeline

Save as `.harmont/ci.py`:

\```python
import harmont as hm

@hm.pipeline("ci")
def ci() -> hm.Step:
    return (
        hm.sh("echo 'hello from harmont'", label="hello")
          .sh("uname -a", label="env")
    )
\```

### 3. Run

\```sh
hm run ci
\```

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

\```python
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
\```

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

\```sh
hm run                             # run the only pipeline
hm run ci                          # run by slug
hm run --parallelism 4             # cap concurrent chains
hm run --env FOO=bar               # inject env vars
hm run --dir path/to/source        # different source root
hm run --format json               # machine-readable event stream
hm run --no-watch                  # submit and exit
hm run --help                      # full flag reference
\```

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

\```sh
cd examples/rust && hm run ci
\```

**Languages:** Rust, Go, Python, TypeScript, React, Next.js, Java, Kotlin, C, C++, C#, OCaml, Zig — plus a [Zig+JS monorepo](./examples/zig-js-monorepo) demonstrating fork/join parallelism with shared toolchain installs.

<details>
<summary>Fork-based pipeline with a shared base image</summary>

\```python
import harmont as hm

@hm.pipeline("ci")
def ci() -> hm.Step:
    setup = hm.sh("apt-get update && apt-get install -y curl", label="apt")
    fetch = setup.fork(label="branch-a").sh("curl -fsSL https://example.com", label="fetch")
    work  = setup.fork(label="branch-b").sh("echo independent work", label="other")
    return hm.pipeline(fetch, work, default_image="ubuntu:24.04")
\```

</details>

<details>
<summary>Composing pipelines with typed targets</summary>

\```python
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
\```

Dependencies resolved by parameter name. `Target[T]` and `Annotated[Step, BaseImage("...")]` both type-check under mypy/pyright.

</details>

## Plugin authoring

`hm` is plugin-driven via [Extism](https://extism.org). Start a `cdylib` crate:

\```sh
cargo new --lib my-plugin && cd my-plugin
cargo add --git https://github.com/harmont-dev/harmont-cli hm-plugin-sdk
\```

Implement `StepExecutor`, `SubcommandPlugin`, `LifecycleHook`, or `OutputFormatter`. Build to WASM and install:

\```sh
cargo build --target wasm32-wasip1 --release
hm plugin install ./target/wasm32-wasip1/release/my_plugin.wasm
\```

## Build from source

\```sh
git clone https://github.com/harmont-dev/harmont-cli && cd harmont-cli
cargo build
cargo test                          # local_* tests need Docker
cargo clippy --all-targets -- -D warnings
cargo fmt --check
\```

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
- [`harmont-ts`](https://github.com/harmont-dev/harmont-ts) — TypeScript DSL (if separate repo)
- [harmont.dev](https://harmont.dev) — Harmont Cloud

## License

Dual-licensed: [Apache-2.0](LICENSE-APACHE) OR [MIT](LICENSE-MIT).
```

**Key changes from current README:**
1. **Badges**: 5 shields.io badges (CI, crates.io, PyPI, license, Discord) — matches hsrs pattern
2. **Tagline**: Two short paragraphs instead of one long one
3. **Quick start**: Numbered H3 steps (install → write → run) — install FIRST, not second
4. **Features section**: New. 7 bullet points, each bold-labeled.
5. **Toolchain helpers**: New section showcasing the language helpers as a code block (much more scannable than the old prose list)
6. **CLI reference**: Consolidated into one block instead of prose + table
7. **Cloud**: Condensed, billing in collapsible
8. **Examples**: Updated language list (removed Perl, Ruby, PHP). Highlighted the zig-js monorepo as the advanced example.
9. **Pruned content**: Removed "eighteen" count (now wrong), removed "Toolchains covered:" prose list
10. **Plugin authoring**: Condensed from 15 lines to 8
11. **Repo layout**: Table format instead of bullets
12. **Structure**: Features before reference material (sell before tell)

**Step 2: Verify all internal links resolve**

```bash
grep -oP '\(\.\/[^)]+\)' README.md | tr -d '()' | while read f; do
  [ -e "$f" ] || echo "BROKEN: $f"
done
```

**Step 3: Verify badge URLs are valid (manual check)**

- Confirm the GitHub Actions workflow filename is correct (check `.github/workflows/`)
- Confirm crates.io package name is `harmont-cli`
- Get the actual Discord server ID if one exists, or remove the Discord badge
- Confirm PyPI package name is `harmont`

**Step 4: Commit**

```bash
git add README.md
git commit -m "docs: overhaul README — hero section, badges, features, pruned languages"
```

---

### Task 6: Update examples README index (if one exists)

**Step 1: Check if there's a top-level examples index**

```bash
ls examples/README.md 2>/dev/null || echo "no index"
```

**Step 2: If it exists, update to remove Perl, Ruby, PHP references and ensure language list matches**

**Step 3: Commit if changed**

```bash
git add examples/README.md && git commit -m "docs: update examples index after language pruning"
```

---

### Task 7: Final verification

**Step 1: Run full Python DSL test suite**

```bash
cd dsls/harmont-py && python -m pytest tests/ -v
```
Expected: All pass, no references to removed toolchains.

**Step 2: Run full TypeScript DSL test suite**

```bash
cd dsls/harmont-ts && npm test
```
Expected: All pass.

**Step 3: Build the CLI to make sure nothing broke**

```bash
cargo build
```
Expected: Clean build.

**Step 4: Verify examples count matches README claim**

```bash
ls -d examples/*/  | wc -l
```
Expected: 14 directories (was 16, minus perl and ruby).

**Step 5: Verify no broken cross-references**

```bash
grep -rn "perl\|ruby\|composer\|laravel\|php" README.md examples/ dsls/ \
  --include="*.md" --include="*.py" --include="*.ts" | grep -vi "license\|copyright"
```
Expected: No results.

---

## Decision Log

| Decision | Rationale |
|---|---|
| Remove Perl, Ruby, Composer/PHP | THIN toolchains (~100 LOC, 2 methods each). Ruby version pinning raises NotImplementedError. All three are trivially replicated with hm.sh(). |
| Keep Elm, OCaml, Haskell | ADEQUATE-to-SOLID implementations with real orchestration value (version management, multi-package support, dependency caching). |
| Keep Zig | ADEQUATE with smart multi-project pattern; growing ecosystem. |
| Badge style: left-aligned, no centering | Matches hsrs repo pattern. Clean, no-nonsense. |
| Install before pipeline example | Users need to install before they can try anything. Current README shows pipeline first, install second — backwards for new users. |
| Features section added | Current README has zero "why should I use this" content. Every top-tier repo (Ruff, Bun, Deno) leads with value propositions. |
| Toolchain helpers as code block | More scannable than a prose list. Shows the actual API surface at a glance. |
| No logo/banner for now | hsrs doesn't use one either. Can add later when brand assets exist. |
| No demo GIF for now | Would be ideal but requires recording terminal sessions. Can be a follow-up task. |

## Open Questions

1. **Discord badge**: Does a Discord server exist? If not, remove that badge.
2. **Elm**: Borderline — ADEQUATE toolchain but niche. No example project exists. Keep or cut?
3. **TypeScript DSL link**: Is `harmont-ts` a separate repo or only in this monorepo? Adjust "See also" accordingly.
4. **Demo GIF**: Worth recording as a follow-up task? A 15-second GIF of `hm run` on the Rust example would be compelling.
