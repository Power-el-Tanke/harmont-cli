# harmont-cli Examples Pipeline Passing — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the GitHub Actions `examples` workflow in `harmont-dev/harmont-cli` green for all 17 example pipelines on push to `main`.

**Architecture:** Three bug clusters block the `run-example` matrix:
1. `harmont-cli` Docker plugin ignores `pipeline.default_image` and falls back to `alpine:latest`, so any example using `apt-get` (every Ubuntu-based example) dies with `sh: apt-get: not found`. Fix at host side in `graph.rs` so each root step inherits `default_image` when its `image` field is empty.
2. Six examples (`nextjs`, `react`, `typescript`, `ruby`, `php-laravel`, `python-uv`) are missing lockfiles that the toolchain helpers reference via `CacheOnChange`, so `harmont.dump_registry_json()` raises `FileNotFoundError` before the run starts. Generate and commit the lockfiles.
3. The `run-example` job in `.github/workflows/examples.yml` is hard-gated by `if: false`. Lift after the above land and local validation passes.

Add regression tests on both sides: a parametrized `harmont-py` pytest that renders every `harmont-cli/examples/*/.harmont/pipeline.py`, and a Rust unit test that asserts root-step image inherits `default_image`.

**Tech Stack:** harmont-py (Python 3.12, pytest, setuptools, croniter), harmont-cli (Rust workspace, Extism WASM plugins, docker daemon, python3 shellout).

**Direct-to-main:** Per user choice, commits land directly on `main` in both repos. No PRs, no branches.

**Working tree assumptions:**
- `/home/marko/harmont-py/` — clean working tree, branch `main`.
- `/home/marko/harmont-cli/` — clean working tree, branch `main`.
- Docker daemon running locally.
- `wasm32-wasip1` Rust target installed.
- `python3` resolvable on `PATH`; harmont-py installed via `pip install --break-system-packages /home/marko/harmont-py` so `import harmont` works for `hm run`.

---

## File Map

### `/home/marko/harmont-py/`
- **Create:** `tests/test_examples_render.py` — pytest parametrized over every `harmont-cli/examples/*/.harmont/pipeline.py`. Requires a local clone of harmont-cli; gated behind `HARMONT_CLI_PATH` env var so it skips when absent.
- **Create:** `tests/examples_render_conftest.py` — fixture loader that imports each example pipeline in a sandboxed module namespace and cleans up the harmont `REGISTRATIONS` registry between tests.

No source code changes needed in `harmont-py`. The toolchain helpers, `@hm.target()`, and fixture-style DI are already complete in `harmont/`.

### `/home/marko/harmont-cli/`
- **Modify:** `crates/hm/src/orchestrator/graph.rs` (around line 120, `from_pipeline`) — after building `nodes`, walk them and for each root step (`builds_in.is_none()`) whose `step.image` is `None`, set `step.image = pipeline.default_image.clone()`.
- **Modify:** `crates/hm-plugin-docker/src/image_name.rs` — update the doc comment so it no longer says "plan 3 will surface it from the Pipeline's default_image" (since plan 3 just landed: the host now sets `step.image`).
- **Modify:** `.github/workflows/examples.yml:80` — delete the `if: false` line on the `run-example` job.
- **Modify:** `.github/workflows/examples.yml` — also add `pull_request` to the `on:` trigger so future PRs exercise the matrix (kept minimal; main remains the gate).
- **Create:** `examples/nextjs/package-lock.json` (via `npm install` in that dir; commit only the lockfile, gitignore `node_modules`).
- **Create:** `examples/react/package-lock.json` (same).
- **Create:** `examples/typescript/package-lock.json` (same).
- **Create:** `examples/ruby/Gemfile.lock` (via `bundle install`).
- **Create:** `examples/php-laravel/composer.lock` (via `composer install --no-dev` or `composer update`; ensure `composer.json` is the source of truth).
- **Create:** `examples/python-uv/uv.lock` (via `uv lock`).
- **Create:** `examples/nextjs/.gitignore`, `examples/react/.gitignore`, `examples/typescript/.gitignore` — each adds `node_modules/`. `examples/php-laravel/.gitignore` adds `vendor/`. `examples/python-uv/.gitignore` adds `.venv/`. `examples/ruby/.gitignore` adds `vendor/bundle/`.
- **Create:** `crates/hm/tests/default_image_inheritance.rs` — integration test asserting root steps inherit `default_image` after graph construction.

---

## Task 1: harmont-py — pytest fixture that renders each example pipeline

**Why first:** A regression test that fails today, passes once we fix harmont-cli. TDD up front: write the failing test, then fix the bugs the test exposes.

**Files:**
- Create: `/home/marko/harmont-py/tests/test_examples_render.py`
- Create: `/home/marko/harmont-py/tests/examples_render_conftest.py`

- [ ] **Step 1: Write conftest helper**

Create `/home/marko/harmont-py/tests/examples_render_conftest.py`:

```python
"""Shared helpers for rendering external example pipelines.

These tests render the pipeline definitions in harmont-cli/examples/
to v0 IR JSON. They are gated behind HARMONT_CLI_PATH so they only
run when a sibling harmont-cli checkout is available.
"""
from __future__ import annotations

import importlib.util
import os
import pathlib
import sys
from contextlib import contextmanager
from typing import Iterator


def harmont_cli_examples_root() -> pathlib.Path | None:
    raw = os.environ.get("HARMONT_CLI_PATH")
    if not raw:
        return None
    p = pathlib.Path(raw) / "examples"
    return p if p.is_dir() else None


@contextmanager
def isolated_registry() -> Iterator[None]:
    """Snapshot and restore the global @hm.pipeline registry so that
    each example renders against an empty slate. Without this, every
    parametrized case would accumulate pipelines from prior cases and
    duplicate slugs would raise.
    """
    import harmont._registry as reg

    saved_pipelines = list(reg.REGISTRATIONS)
    saved_targets = dict(reg.TARGETS) if hasattr(reg, "TARGETS") else {}
    reg.REGISTRATIONS.clear()
    if hasattr(reg, "TARGETS"):
        reg.TARGETS.clear()
    try:
        yield
    finally:
        reg.REGISTRATIONS.clear()
        reg.REGISTRATIONS.extend(saved_pipelines)
        if hasattr(reg, "TARGETS"):
            reg.TARGETS.clear()
            reg.TARGETS.update(saved_targets)


def load_pipeline_module(example_dir: pathlib.Path) -> None:
    """Load .harmont/pipeline.py from `example_dir`, executing decorator
    side-effects. Run with cwd = example_dir so on_change cache paths
    resolve correctly.
    """
    pipeline_py = example_dir / ".harmont" / "pipeline.py"
    spec = importlib.util.spec_from_file_location(
        f"_harmont_example_{example_dir.name}", pipeline_py
    )
    assert spec is not None and spec.loader is not None
    mod = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = mod
    try:
        spec.loader.exec_module(mod)
    finally:
        sys.modules.pop(spec.name, None)
```

- [ ] **Step 2: Inspect harmont._registry to confirm public-ish names**

Run: `python3 -c "import harmont._registry as r; print(dir(r))"`

Expected: confirm `REGISTRATIONS` and (if present) `TARGETS` attribute names. If the registry uses different names (e.g. `_REGISTRATIONS`), update the conftest helper to match exactly. **Do not invent names — read the file first.**

- [ ] **Step 3: Write the failing test**

Create `/home/marko/harmont-py/tests/test_examples_render.py`:

```python
"""End-to-end render checks against harmont-cli example pipelines.

Gated: skipped when HARMONT_CLI_PATH is unset. CI sets it after
cloning harmont-cli.
"""
from __future__ import annotations

import json
import os
import pathlib

import pytest

from tests.examples_render_conftest import (
    harmont_cli_examples_root,
    isolated_registry,
    load_pipeline_module,
)

EXAMPLES_ROOT = harmont_cli_examples_root()

pytestmark = pytest.mark.skipif(
    EXAMPLES_ROOT is None,
    reason="HARMONT_CLI_PATH not set or examples/ missing",
)


def _example_dirs() -> list[pathlib.Path]:
    if EXAMPLES_ROOT is None:
        return []
    return sorted(
        p for p in EXAMPLES_ROOT.iterdir()
        if p.is_dir() and (p / ".harmont" / "pipeline.py").is_file()
    )


EXAMPLE_IDS = [p.name for p in _example_dirs()]


@pytest.mark.parametrize("example_dir", _example_dirs(), ids=EXAMPLE_IDS)
def test_example_renders_to_v0_ir(
    example_dir: pathlib.Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    import harmont as hm

    monkeypatch.chdir(example_dir)
    with isolated_registry():
        load_pipeline_module(example_dir)
        envelope_json = hm.dump_registry_json()

    envelope = json.loads(envelope_json)
    assert envelope["schema_version"] == "1"
    assert envelope["pipelines"], f"{example_dir.name}: no pipelines registered"

    ci_pipeline = next(
        (p for p in envelope["pipelines"] if p["slug"] == "ci"), None
    )
    assert ci_pipeline is not None, (
        f"{example_dir.name}: no 'ci' pipeline registered; "
        f"got slugs {[p['slug'] for p in envelope['pipelines']]}"
    )
    definition = ci_pipeline["definition"]
    assert definition["version"] == "0"
    assert definition.get("steps"), (
        f"{example_dir.name}: ci pipeline has no steps"
    )
    assert definition.get("default_image"), (
        f"{example_dir.name}: ci pipeline missing default_image — local "
        "executor falls back to alpine and apt-get-based examples die"
    )
```

- [ ] **Step 4: Run the test — expect failures or skips**

Run from `/home/marko/harmont-py`:

```bash
HARMONT_CLI_PATH=/home/marko/harmont-cli python3 -m pytest tests/test_examples_render.py -v
```

Expected (today): six failures with `FileNotFoundError: on_change path does not exist: …` for `nextjs`, `react`, `typescript`, `ruby`, `php-laravel`, `python-uv`. Other 11 should PASS. The failures are the lockfile bug we are about to fix; they confirm the test catches it.

- [ ] **Step 5: Commit harmont-py test additions**

```bash
cd /home/marko/harmont-py
git add tests/test_examples_render.py tests/examples_render_conftest.py
git commit -m "$(cat <<'EOF'
test: add render checks against harmont-cli/examples

Parametrized pytest that loads each harmont-cli example pipeline
and asserts it renders to v0 IR JSON. Gated by HARMONT_CLI_PATH so
the harmont-py test suite does not regress when run in isolation.

Catches missing lockfiles (CacheOnChange path-not-found) and
malformed pipeline definitions before they reach the local executor.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

- [ ] **Step 6: Push to main**

```bash
cd /home/marko/harmont-py && git push origin main
```

---

## Task 2: harmont-cli — failing test for `default_image` inheritance

**Why next:** TDD the Rust fix. The bug is that root steps with no per-step `image` boot from `alpine:latest`, ignoring the pipeline's `default_image`. Write a graph-level test that proves root steps inherit `default_image` after construction.

**Files:**
- Create: `/home/marko/harmont-cli/crates/hm/tests/default_image_inheritance.rs`

- [ ] **Step 1: Inspect graph.rs to confirm public types**

Read `/home/marko/harmont-cli/crates/hm/src/orchestrator/graph.rs` lines 1–130. Confirm:
- The type that holds parsed pipeline data (`Pipeline` from `hm_plugin_protocol`).
- The graph constructor name (`Graph::from_pipeline` or similar).
- How to access an individual node's `step.image` after construction.

If the type names differ, update Step 2 below to match.

- [ ] **Step 2: Write the failing test**

Create `/home/marko/harmont-cli/crates/hm/tests/default_image_inheritance.rs`:

```rust
//! Regression test: root steps with no per-step image must inherit
//! the pipeline's default_image. Without this, the docker plugin
//! falls back to alpine:latest and apt-get-based examples die with
//! `sh: apt-get: not found`.

use harmont_cli::orchestrator::graph::Graph;
use hm_plugin_protocol::{CacheSpec, CommandStep, Pipeline, Step};

fn root_command(key: &str) -> Step {
    Step::Command(CommandStep {
        key: key.into(),
        label: None,
        cmd: "true".into(),
        builds_in: None,
        image: None,
        env: None,
        timeout_seconds: None,
        cache: None,
        runner: None,
        runner_args: None,
    })
}

#[test]
fn root_step_inherits_default_image() {
    let pipeline = Pipeline {
        version: "0".into(),
        env: None,
        default_image: Some("ubuntu:24.04".into()),
        steps: vec![root_command("apt-base")],
    };

    let graph = Graph::from_pipeline(&pipeline).expect("graph builds");
    let node = graph.node(0);

    assert_eq!(
        node.step.image.as_deref(),
        Some("ubuntu:24.04"),
        "root step image must inherit pipeline default_image"
    );
}

#[test]
fn root_step_explicit_image_wins() {
    let mut step = root_command("explicit");
    if let Step::Command(ref mut c) = step {
        c.image = Some("rust:1.82".into());
    }
    let pipeline = Pipeline {
        version: "0".into(),
        env: None,
        default_image: Some("ubuntu:24.04".into()),
        steps: vec![step],
    };

    let graph = Graph::from_pipeline(&pipeline).expect("graph builds");
    let node = graph.node(0);

    assert_eq!(
        node.step.image.as_deref(),
        Some("rust:1.82"),
        "explicit per-step image must override default_image",
    );
}

#[test]
fn child_step_unchanged_by_default_image() {
    let parent = root_command("parent");
    let mut child = root_command("child");
    if let Step::Command(ref mut c) = child {
        c.builds_in = Some("parent".into());
    }
    let pipeline = Pipeline {
        version: "0".into(),
        env: None,
        default_image: Some("ubuntu:24.04".into()),
        steps: vec![parent, child],
    };

    let graph = Graph::from_pipeline(&pipeline).expect("graph builds");
    let child_node = graph.node(1);

    assert!(
        child_node.step.image.is_none(),
        "non-root step must not be tagged with default_image — it \
         inherits the parent container snapshot at runtime"
    );
}
```

> NOTE: If `Graph::node(usize)` or the `harmont_cli::orchestrator::graph::Graph` path is not public, expose it minimally for testing (preferred via `pub(crate)` + a `#[cfg(test)] pub` accessor, or via existing `pub use` chain). Do not refactor the orchestrator just for this test — minimal surface only.

- [ ] **Step 3: Verify Cargo wires up the test**

The `harmont-cli` crate is named `harmont-cli` in `crates/hm/Cargo.toml`. Confirm `tests/` integration tests already compile against the lib (see other tests in `crates/hm/tests/`). If `lib.rs` does not currently re-export `orchestrator`, add a minimal `pub mod orchestrator;` re-export gate, or use the existing pattern from neighboring integration tests.

Run:

```bash
cd /home/marko/harmont-cli && cargo build --tests -p harmont-cli 2>&1 | tail -20
```

Expected: compiles. If not, the access path needs adjusting — match the style of existing tests like `cmd_run_local_orchestrated.rs`.

- [ ] **Step 4: Run the test — expect failures**

```bash
cd /home/marko/harmont-cli && cargo test -p harmont-cli --test default_image_inheritance 2>&1 | tail -20
```

Expected: `root_step_inherits_default_image` FAILS with `node.step.image` being `None`. The other two tests pass (they assert behavior that already holds — explicit image, child step).

- [ ] **Step 5: Commit the failing test**

```bash
cd /home/marko/harmont-cli
git add crates/hm/tests/default_image_inheritance.rs
git commit -m "$(cat <<'EOF'
test: assert root steps inherit default_image

Failing test that reproduces the alpine-fallback bug: when the
pipeline carries default_image="ubuntu:24.04" but a root step has
image=None, the docker plugin falls back to alpine:latest and any
apt-get command dies with "sh: apt-get: not found".

Fix arrives in the next commit.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: harmont-cli — fix `default_image` inheritance in graph builder

**Files:**
- Modify: `/home/marko/harmont-cli/crates/hm/src/orchestrator/graph.rs` (around line 120, after `nodes` populated)
- Modify: `/home/marko/harmont-cli/crates/hm-plugin-docker/src/image_name.rs` (doc comment update only)

- [ ] **Step 1: Patch `Graph::from_pipeline` to populate root images**

Edit `/home/marko/harmont-cli/crates/hm/src/orchestrator/graph.rs`. Locate the block where `nodes` is finalised (right before `let g = Self { nodes, default_image: pipeline.default_image.clone(), };` at line ~120). Insert:

```rust
        // Root steps (no `builds_in`) with no explicit `image` inherit
        // the pipeline's `default_image`. Without this the docker
        // plugin's resolver falls back to alpine:latest, which breaks
        // every apt-based example. We patch at the host so the plugin
        // remains pipeline-agnostic.
        if let Some(default_img) = pipeline.default_image.as_deref() {
            for node in nodes.iter_mut() {
                if node.builds_in.is_none() && node.step.image.is_none() {
                    node.step.image = Some(default_img.to_string());
                }
            }
        }
```

> Field names: `step.image` must match the `CommandStep` field on the node. If `Node::step` is typed as the `Step` enum (not directly `CommandStep`), match on `Step::Command(ref mut c)` and patch `c.image`. Read graph.rs lines 30–60 to confirm the exact node type before editing.

- [ ] **Step 2: Run the unit test to verify the fix**

```bash
cd /home/marko/harmont-cli && cargo test -p harmont-cli --test default_image_inheritance 2>&1 | tail -20
```

Expected: all three tests PASS.

- [ ] **Step 3: Run the existing local-run integration tests to confirm no regressions**

```bash
cd /home/marko/harmont-cli && cargo test -p harmont-cli 2>&1 | tail -40
```

Expected: all existing tests still pass. Pay attention to `cmd_run_local_*` and `local_fork_cache` — these use `alpine:3.20` as default_image, so they exercise the fixed path.

- [ ] **Step 4: Update doc comment in image_name.rs**

Edit `/home/marko/harmont-cli/crates/hm-plugin-docker/src/image_name.rs`. Replace the "Plan 2 keeps a hardcoded fallback of alpine:latest; plan 3 will surface it from the Pipeline's default_image" paragraph with:

```rust
/// 4. Fall back to `"alpine:latest"`. Root steps that want a different
///    default are tagged with `step.image = default_image` by the host
///    before dispatch (see `orchestrator::graph::Graph::from_pipeline`),
///    so the plugin only reaches the alpine fallback when both the
///    pipeline and step omit an image — which is a misconfiguration
///    we let the user feel rather than paper over.
```

- [ ] **Step 5: Commit the fix**

```bash
cd /home/marko/harmont-cli
git add crates/hm/src/orchestrator/graph.rs crates/hm-plugin-docker/src/image_name.rs
git commit -m "$(cat <<'EOF'
fix(orchestrator): root steps inherit pipeline default_image

The docker plugin's image resolver falls back to alpine:latest when a
step has no `image` field — but Pipeline.default_image was never
propagated, so every ubuntu-based example died with
`sh: apt-get: not found`.

Patch at the host side: when Graph::from_pipeline finishes building
nodes, any root step (builds_in=None) with no explicit image inherits
pipeline.default_image. Child steps stay untouched — they boot from
the parent's committed snapshot at runtime.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: harmont-cli — commit lockfile for `python-uv`

**Why:** The python toolchain's `uv sync` step is cached on `uv.lock` + `pyproject.toml`. `dump_registry_json()` hashes lockfile contents to derive the cache key, so the file must exist.

**Files:**
- Create: `/home/marko/harmont-cli/examples/python-uv/uv.lock`
- Create: `/home/marko/harmont-cli/examples/python-uv/.gitignore`

- [ ] **Step 1: Install uv locally if missing**

```bash
which uv || curl -LsSf https://astral.sh/uv/install.sh | sh
```

Expected: `uv` resolves on PATH. (User pre-flight: confirm `uv --version` works.)

- [ ] **Step 2: Generate the lockfile**

```bash
cd /home/marko/harmont-cli/examples/python-uv && uv lock
```

Expected: `uv.lock` written next to `pyproject.toml`. If `uv lock` complains about missing project metadata, run `uv init --no-readme --package` first to ensure `[project]` is complete, then re-run `uv lock`. Do not introduce new pyproject.toml dependencies beyond what is already there.

- [ ] **Step 3: Add gitignore entry for .venv**

Create `/home/marko/harmont-cli/examples/python-uv/.gitignore`:

```
.venv/
__pycache__/
```

- [ ] **Step 4: Verify render now succeeds**

```bash
cd /home/marko/harmont-cli/examples/python-uv && python3 -c "
import importlib.util
spec = importlib.util.spec_from_file_location('pipeline', '.harmont/pipeline.py')
m = importlib.util.module_from_spec(spec); spec.loader.exec_module(m)
import harmont as hm
print('OK', len(hm.dump_registry_json()))
"
```

Expected: `OK <N>` printed; no `FileNotFoundError`.

- [ ] **Step 5: Commit**

```bash
cd /home/marko/harmont-cli
git add examples/python-uv/uv.lock examples/python-uv/.gitignore
git commit -m "$(cat <<'EOF'
examples/python-uv: commit uv.lock for cache-on-change keying

harmont.python's uv sync step is cached on uv.lock + pyproject.toml;
without uv.lock the pipeline fails to render before docker is even
invoked.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 5: harmont-cli — commit lockfile for `nextjs`

**Files:**
- Create: `/home/marko/harmont-cli/examples/nextjs/package-lock.json`
- Create: `/home/marko/harmont-cli/examples/nextjs/.gitignore`

- [ ] **Step 1: Verify Node is available**

```bash
node --version && npm --version
```

Expected: Node 20.x or newer; npm present. If Node is missing on the host, install Node 20 via `nvm install 20 && nvm use 20`.

- [ ] **Step 2: Generate the lockfile**

```bash
cd /home/marko/harmont-cli/examples/nextjs && npm install --package-lock-only
```

The `--package-lock-only` flag generates the lockfile without populating `node_modules`, keeping the working tree clean. If the project layout requires resolved transitive deps and `--package-lock-only` produces an incomplete file, drop the flag and rely on the `.gitignore` to exclude `node_modules`.

- [ ] **Step 3: Add gitignore**

Create `/home/marko/harmont-cli/examples/nextjs/.gitignore`:

```
node_modules/
.next/
```

- [ ] **Step 4: Verify render succeeds**

```bash
cd /home/marko/harmont-cli/examples/nextjs && python3 -c "
import importlib.util
spec = importlib.util.spec_from_file_location('pipeline', '.harmont/pipeline.py')
m = importlib.util.module_from_spec(spec); spec.loader.exec_module(m)
import harmont as hm
print('OK', len(hm.dump_registry_json()))
"
```

Expected: `OK <N>`.

- [ ] **Step 5: Commit**

```bash
cd /home/marko/harmont-cli
git add examples/nextjs/package-lock.json examples/nextjs/.gitignore
git commit -m "$(cat <<'EOF'
examples/nextjs: commit package-lock.json for npm-ci caching

harmont.npm runs `npm ci`, which requires a committed lockfile and
caches on it via CacheOnChange. Without it, render aborts with
FileNotFoundError before docker is invoked.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 6: harmont-cli — commit lockfile for `react`

**Files:**
- Create: `/home/marko/harmont-cli/examples/react/package-lock.json`
- Create: `/home/marko/harmont-cli/examples/react/.gitignore`

- [ ] **Step 1: Generate the lockfile**

```bash
cd /home/marko/harmont-cli/examples/react && npm install --package-lock-only
```

- [ ] **Step 2: Add gitignore**

Create `/home/marko/harmont-cli/examples/react/.gitignore`:

```
node_modules/
dist/
```

- [ ] **Step 3: Verify render succeeds**

```bash
cd /home/marko/harmont-cli/examples/react && python3 -c "
import importlib.util
spec = importlib.util.spec_from_file_location('pipeline', '.harmont/pipeline.py')
m = importlib.util.module_from_spec(spec); spec.loader.exec_module(m)
import harmont as hm
print('OK', len(hm.dump_registry_json()))
"
```

Expected: `OK <N>`.

- [ ] **Step 4: Commit**

```bash
cd /home/marko/harmont-cli
git add examples/react/package-lock.json examples/react/.gitignore
git commit -m "$(cat <<'EOF'
examples/react: commit package-lock.json for npm-ci caching

Same rationale as nextjs: harmont.npm caches `npm ci` on the lockfile.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 7: harmont-cli — commit lockfile for `typescript`

**Files:**
- Create: `/home/marko/harmont-cli/examples/typescript/package-lock.json`
- Create: `/home/marko/harmont-cli/examples/typescript/.gitignore`

- [ ] **Step 1: Generate the lockfile**

```bash
cd /home/marko/harmont-cli/examples/typescript && npm install --package-lock-only
```

- [ ] **Step 2: Add gitignore**

Create `/home/marko/harmont-cli/examples/typescript/.gitignore`:

```
node_modules/
dist/
```

- [ ] **Step 3: Verify render succeeds**

```bash
cd /home/marko/harmont-cli/examples/typescript && python3 -c "
import importlib.util
spec = importlib.util.spec_from_file_location('pipeline', '.harmont/pipeline.py')
m = importlib.util.module_from_spec(spec); spec.loader.exec_module(m)
import harmont as hm
print('OK', len(hm.dump_registry_json()))
"
```

Expected: `OK <N>`.

- [ ] **Step 4: Commit**

```bash
cd /home/marko/harmont-cli
git add examples/typescript/package-lock.json examples/typescript/.gitignore
git commit -m "$(cat <<'EOF'
examples/typescript: commit package-lock.json for npm-ci caching

Same rationale as nextjs/react.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 8: harmont-cli — commit lockfile for `ruby`

**Files:**
- Create: `/home/marko/harmont-cli/examples/ruby/Gemfile.lock`
- Create: `/home/marko/harmont-cli/examples/ruby/.gitignore`

- [ ] **Step 1: Verify bundler is available**

```bash
ruby --version && bundle --version
```

Expected: Ruby 3.x and Bundler present. If missing, install via `gem install bundler` or use docker:

```bash
docker run --rm -v "$PWD:/work" -w /work ruby:3.3-slim bash -c "bundle lock"
```

- [ ] **Step 2: Generate the lockfile**

```bash
cd /home/marko/harmont-cli/examples/ruby && bundle lock
```

`bundle lock` resolves dependencies and writes `Gemfile.lock` without installing gems, keeping the working tree clean.

- [ ] **Step 3: Add gitignore**

Create `/home/marko/harmont-cli/examples/ruby/.gitignore`:

```
vendor/bundle/
.bundle/
```

- [ ] **Step 4: Verify render succeeds**

```bash
cd /home/marko/harmont-cli/examples/ruby && python3 -c "
import importlib.util
spec = importlib.util.spec_from_file_location('pipeline', '.harmont/pipeline.py')
m = importlib.util.module_from_spec(spec); spec.loader.exec_module(m)
import harmont as hm
print('OK', len(hm.dump_registry_json()))
"
```

Expected: `OK <N>`.

- [ ] **Step 5: Commit**

```bash
cd /home/marko/harmont-cli
git add examples/ruby/Gemfile.lock examples/ruby/.gitignore
git commit -m "$(cat <<'EOF'
examples/ruby: commit Gemfile.lock for bundle-deps caching

harmont.ruby caches `bundle install` on Gemfile.lock via
CacheOnChange. Without the lockfile, render aborts before docker.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 9: harmont-cli — commit lockfile for `php-laravel`

**Files:**
- Create: `/home/marko/harmont-cli/examples/php-laravel/composer.lock`
- Create: `/home/marko/harmont-cli/examples/php-laravel/.gitignore`

- [ ] **Step 1: Verify composer is available**

```bash
composer --version
```

Expected: Composer 2.x. If missing, use docker:

```bash
docker run --rm -v "$PWD:/app" -w /app composer:2 composer update --no-install --ignore-platform-reqs
```

- [ ] **Step 2: Generate the lockfile (no-install mode)**

```bash
cd /home/marko/harmont-cli/examples/php-laravel && composer update --no-install --ignore-platform-reqs
```

`--no-install --ignore-platform-reqs` writes `composer.lock` without populating `vendor/` and without requiring the matching PHP version on the host.

- [ ] **Step 3: Add gitignore**

Create `/home/marko/harmont-cli/examples/php-laravel/.gitignore`:

```
vendor/
.env
```

- [ ] **Step 4: Verify render succeeds**

```bash
cd /home/marko/harmont-cli/examples/php-laravel && python3 -c "
import importlib.util
spec = importlib.util.spec_from_file_location('pipeline', '.harmont/pipeline.py')
m = importlib.util.module_from_spec(spec); spec.loader.exec_module(m)
import harmont as hm
print('OK', len(hm.dump_registry_json()))
"
```

Expected: `OK <N>`.

- [ ] **Step 5: Commit**

```bash
cd /home/marko/harmont-cli
git add examples/php-laravel/composer.lock examples/php-laravel/.gitignore
git commit -m "$(cat <<'EOF'
examples/php-laravel: commit composer.lock for composer-deps caching

harmont.composer caches `composer install` on composer.lock via
CacheOnChange. Without the lockfile, render aborts before docker.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 10: harmont-cli — verify all 17 examples render

**Why:** Catch any remaining keygen/cache-path bugs before re-running locally.

- [ ] **Step 1: Sweep render every example**

```bash
cd /home/marko/harmont-cli && for ex in examples/*/; do
  name=$(basename "$ex")
  [ "$name" = "README.md" ] && continue
  result=$(cd "$ex" && python3 -c "
import importlib.util
spec = importlib.util.spec_from_file_location('pipeline', '.harmont/pipeline.py')
m = importlib.util.module_from_spec(spec); spec.loader.exec_module(m)
import harmont as hm
out = hm.dump_registry_json()
print('OK', len(out))
" 2>&1 | tail -3)
  echo "$name: $result"
done
```

Expected: every example prints `OK <N>`. If any prints a traceback, debug before continuing. Likely causes:
- The lockfile was written in a different directory than the toolchain expects (e.g., `harmont.python` looks under `path` arg — if the example calls `hm.python(path=".")`, lockfile must sit at `examples/python-uv/uv.lock`, **not** under `src/`).
- A new toolchain helper hardcodes another file we missed; grep `harmont/` for `CacheOnChange(paths=`.

- [ ] **Step 2: Cross-check the harmont-py parametrized test**

```bash
HARMONT_CLI_PATH=/home/marko/harmont-cli python3 -m pytest \
  /home/marko/harmont-py/tests/test_examples_render.py -v
```

Expected: all 17 cases pass (no skips, no failures). If a render mismatch appears between Task 10/Step 1 (raw render) and pytest (with `isolated_registry`), debug the registry-restore logic in `examples_render_conftest.py` — likely the wrong global name.

---

## Task 11: harmont-cli — verify all 17 examples actually execute under `hm run`

**Why:** Render succeeding does not guarantee the docker plugin can pull each image and run the steps. This is the real smoke test.

**Pre-flight:**
- Docker daemon running and accessible to the current user.
- `wasm32-wasip1` Rust target installed.
- `hm` binary built (`cargo build -p harmont-cli`).
- harmont-py installed system-wide (`pip install --break-system-packages /home/marko/harmont-py`).

- [ ] **Step 1: Rebuild hm with the default_image fix in place**

```bash
cd /home/marko/harmont-cli && cargo build -p harmont-cli 2>&1 | tail -5
```

Expected: `Finished … target(s)`.

- [ ] **Step 2: Run the rust example end-to-end as the canary**

```bash
cd /home/marko/harmont-cli/examples/rust && \
  HM_NONINTERACTIVE=1 /home/marko/harmont-cli/target/debug/hm run ci 2>&1 | tail -40
```

Expected: pipeline completes with `build: end exit=0`. If a step fails:
- `apt-get not found` → default_image fix didn't land or the new code path isn't hit; recheck Task 3.
- `pull failed` for an image → network blocked or image name typo in toolchain; capture exact image and adjust.
- toolchain-specific error (e.g. `cargo not found`) → the example needs additional apt-packages in the toolchain's `make_install_chain`; that's a harmont-py fix.

- [ ] **Step 3: Run every example sequentially, collecting outcomes**

```bash
cd /home/marko/harmont-cli && rm -rf /tmp/hm-example-logs && mkdir /tmp/hm-example-logs
for ex in examples/*/; do
  name=$(basename "$ex")
  [ "$name" = "README.md" ] && continue
  echo "=== $name ===" | tee -a /tmp/hm-example-logs/summary.txt
  (cd "$ex" && HM_NONINTERACTIVE=1 timeout 1200 \
    /home/marko/harmont-cli/target/debug/hm run ci) \
    > /tmp/hm-example-logs/$name.log 2>&1
  code=$?
  echo "$name exit=$code" | tee -a /tmp/hm-example-logs/summary.txt
done
cat /tmp/hm-example-logs/summary.txt
```

Expected: every example reports `exit=0`. For each non-zero exit, open `/tmp/hm-example-logs/<name>.log` and fix the underlying issue. Likely fix locations:
- Toolchain missing an apt package → edit the corresponding `harmont/<lang>.py` `APT_PACKAGES`; recommit to harmont-py.
- Step times out → adjust timeout (but examples should finish in <20 minutes per the CI budget).
- Image pull rate-limited → use the GH runner-friendly retry pattern; not relevant locally with a logged-in Docker.

- [ ] **Step 4: If any toolchain fix landed in harmont-py during Step 3, retest the parametrized renderer**

```bash
pip install --break-system-packages --force-reinstall --no-deps /home/marko/harmont-py
HARMONT_CLI_PATH=/home/marko/harmont-cli python3 -m pytest \
  /home/marko/harmont-py/tests/test_examples_render.py -v
```

Expected: all 17 cases still pass.

- [ ] **Step 5: Commit any harmont-cli changes uncovered by end-to-end runs**

```bash
cd /home/marko/harmont-cli
git status
# For each surfaced bug, add the relevant files and commit individually:
git add <files>
git commit -m "$(cat <<'EOF'
<scoped message describing the specific fix uncovered by example run>

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

> If no further bugs surface, skip this step.

---

## Task 12: harmont-cli — lift the `if: false` gate on the CI matrix

**Why:** All 17 examples now render and run locally; CI can be trusted.

**Files:**
- Modify: `/home/marko/harmont-cli/.github/workflows/examples.yml`

- [ ] **Step 1: Remove `if: false` and the now-stale comment**

Edit `/home/marko/harmont-cli/.github/workflows/examples.yml`. Locate the block (around lines 73–80):

```yaml
  # Examples are gated until harmont-py main grows the toolchain
  # helpers (hm.cmake / hm.npm / hm.gradle / hm.python / hm.rust /
  # hm.go / hm.haskell / hm.ocaml / hm.perl / hm.composer / hm.ruby /
  # hm.zig / hm.dotnet) + the @hm.target system. They live in the
  # private simci monorepo today and need porting to the public
  # harmont-py repo. Re-enable by removing this `if:` once that lands.
  run-example:
    if: false
    needs: build-hm
```

Replace with:

```yaml
  run-example:
    needs: build-hm
```

- [ ] **Step 2: Add `pull_request` trigger so PRs exercise the matrix**

Edit the `on:` block at the top of the file. Change:

```yaml
on:
  push:
    branches: [main]
```

to:

```yaml
on:
  push:
    branches: [main]
  pull_request:
    branches: [main]
```

- [ ] **Step 3: Verify workflow syntax locally**

```bash
cd /home/marko/harmont-cli && \
  python3 -c "import yaml, sys; yaml.safe_load(open('.github/workflows/examples.yml'))" && \
  echo "yaml ok"
```

Expected: `yaml ok`. If you have `actionlint` installed, also run `actionlint .github/workflows/examples.yml`.

- [ ] **Step 4: Verify validate-matrix sanity (no new examples added)**

The workflow's `validate-matrix` job diffs `examples/` against the matrix list. Run the same shell logic locally:

```bash
cd /home/marko/harmont-cli && \
  on_disk="$(ls examples | grep -v '^README\.md$' | sort)" && \
  in_matrix="$(awk '/^        example:$/{flag=1;next}/^    steps:$/{flag=0}flag' .github/workflows/examples.yml \
    | sed -n 's/^\s*- //p' | sort)" && \
  diff <(printf '%s\n' "$on_disk") <(printf '%s\n' "$in_matrix") && \
  echo "matrix in sync"
```

Expected: `matrix in sync` (no diff). If the matrix is out of sync, fix it before lifting the gate.

- [ ] **Step 5: Commit**

```bash
cd /home/marko/harmont-cli
git add .github/workflows/examples.yml
git commit -m "$(cat <<'EOF'
ci(examples): re-enable run-example matrix

harmont-py main now ships all toolchain helpers and the @hm.target
system; lockfiles for the 6 examples that needed them are committed;
the default_image inheritance bug is fixed at the orchestrator.
Lifts the if:false gate and adds pull_request as a trigger so PRs
exercise the matrix before merge.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 13: Push, watch CI, react

- [ ] **Step 1: Push harmont-cli main**

```bash
cd /home/marko/harmont-cli && git push origin main
```

- [ ] **Step 2: Watch the workflow run**

```bash
gh run watch --repo harmont-dev/harmont-cli \
  $(gh run list --repo harmont-dev/harmont-cli --workflow examples.yml --limit 1 --json databaseId --jq '.[0].databaseId')
```

Expected: every matrix leg reports `completed success`. Total wall-clock typically <30 minutes given the shared `build-hm` artifact.

- [ ] **Step 3: For any failed leg, pull logs and triage**

```bash
gh run view --repo harmont-dev/harmont-cli --log-failed \
  $(gh run list --repo harmont-dev/harmont-cli --workflow examples.yml --limit 1 --json databaseId --jq '.[0].databaseId')
```

Likely CI-only failure modes (not seen locally):
- Docker daemon flakiness on a runner → re-run the leg.
- Image pull rate-limit from anonymous DockerHub → may need to add a `docker/login-action` step (deferred; do not add without confirming the limit was hit).
- harmont-py install fails because `pip install /tmp/harmont-py` runs without `--break-system-packages` on Ubuntu 24.04 runners → add the flag in the workflow.

- [ ] **Step 4: Iterate**

For each CI-only failure, write a minimal fix, commit, push, repeat Step 2.

- [ ] **Step 5: Done**

Once all 17 legs go green on a `main` push, the goal is met. Update repo-level docs (`harmont-cli/README.md` examples section if it claims the matrix is gated) in a follow-up commit if any such claim exists.

---

## Out of scope (do not do)

- **Cloud verbs:** `hm cloud …` is not exercised by the examples workflow. Leave alone.
- **Release tagging:** Examples workflow does not consume a tagged harmont-py release — it `pip install`s from `main`. No version bumps needed.
- **Refactoring the docker plugin's image resolver chain:** the host-side patch in `graph.rs` keeps the plugin pipeline-agnostic. Don't introduce a new `default_image` field on `ExecutorInput` — overkill for the bug.
- **Adding `hm.elm`, `hm.gradle(kotlin=…)`, or any toolchain not already present:** the CLAUDE.md surface is already complete; no example introduces a new helper.

---

## Self-review checklist (run after writing)

- Spec coverage: 17 examples ✓; default_image bug ✓; CI gate ✓; lockfiles for 6 examples ✓; regression tests on both sides ✓.
- No placeholders: every step has exact paths, exact commands, expected output.
- Type consistency: `Pipeline`, `CommandStep`, `Step::Command`, `Graph::from_pipeline`, `dump_registry_json`, `HARMONT_CLI_PATH`, `REGISTRATIONS` — all used consistently across tasks.
- Each commit is independently verifiable; pre-commit hooks (if any) operate on small, focused changes.
