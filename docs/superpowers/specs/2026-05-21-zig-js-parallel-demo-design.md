# zig + js Parallel Demo — Design

**Date:** 2026-05-21
**Status:** Approved
**Scope:** harmont-cli (new example) + harmont-py (none; uses existing surface)

## Goal

Add an 18th example, `examples/zig-js/`, whose pipeline visibly demonstrates harmont's chain-level parallelism: a single shared `apt-base` step that forks into two concurrent install chains — one for Zig, one for Node — each running in its own Docker container at the same time. Viewers running `hm run ci` from the example dir see `[zig: install] start` and `[node: install] start` events emitted with overlapping timestamps in the default human output.

## Non-goals

- No new harmont-py public surface. The `base=` kwarg on `hm.zig(...)` and `hm.npm(...)` and the `@hm.target()` decorator are pre-existing.
- No new orchestrator behavior. The chain-level scheduler with `Semaphore(parallelism)` already runs independent chains concurrently.
- No bespoke visualization tooling. The default human output is the demo.
- No marketing copy. A short example-level `README.md` is fine; no docs site changes.

## Architecture

### File layout

```
examples/zig-js/
├── README.md                              # what to look for in the output
├── .harmont/
│   └── pipeline.py                        # the demo pipeline
├── .gitignore                             # node_modules/, dist/, zig-out/, .zig-cache/
├── zig-src/                               # lifted verbatim from examples/zig/
│   ├── build.zig
│   └── src/{main.zig,root.zig}
└── web/                                   # lifted verbatim from examples/typescript/
    ├── package.json
    ├── package-lock.json
    ├── tsconfig.json
    ├── eslint.config.js
    └── src/index.ts
```

### Pipeline

```python
"""zig + js parallelism demo.

A single apt-base step forks into two install chains (Zig and Node)
that run in parallel containers. Watch [zig: install] and
[node: install] start events overlap when this runs locally.
"""
from __future__ import annotations

from datetime import timedelta
from typing import Annotated

import harmont as hm
from harmont.npm import NpmProject
from harmont.zig import ZigProject


@hm.target()
def apt_base(base: Annotated[hm.Step, hm.BaseImage("ubuntu:24.04")]) -> hm.Step:
    """Shared system-package prerequisites. Both language installs
    fork off the snapshot this step commits."""
    return base.sh(
        "apt-get update && "
        "apt-get install -y --no-install-recommends "
        "curl ca-certificates xz-utils",
        label=":apt: base",
        cache=hm.ttl(timedelta(days=1)),
    )


@hm.target()
def zig_project(apt_base: hm.Target[hm.Step]) -> ZigProject:
    return hm.zig(path="zig-src", base=apt_base)


@hm.target()
def web_project(apt_base: hm.Target[hm.Step]) -> NpmProject:
    return hm.npm(path="web", base=apt_base)


@hm.pipeline(
    "ci",
    env={"CI": "true"},
    default_image="ubuntu:24.04",
    triggers=[hm.push(branch="main")],
)
def ci(
    zig_project: hm.Target[ZigProject],
    web_project: hm.Target[NpmProject],
) -> tuple[hm.Step, ...]:
    return (
        zig_project.build(),
        zig_project.test(),
        zig_project.fmt(),
        web_project.run("build"),
        web_project.run("test"),
        web_project.run("lint"),
    )
```

### Resulting v0 IR DAG

```
apt-base                       [image=ubuntu:24.04, root, ttl=24h]
├── :zig: install              [builds_in=apt-base, cache=forever]
│   ├── :zig: build            [forks; sibling exists]
│   ├── :zig: test
│   └── :zig: fmt
└── :node: install             [builds_in=apt-base, cache=forever]
    └── :node: deps (npm ci)   [cache=on_change(web/package-lock.json)]
        ├── :node: build
        ├── :node: test
        └── :node: lint
```

The toolchains' `make_install_chain` accepts `base=` per `harmont/_toolchain.py:make_install_chain`: when a base step is supplied, the helper skips its own apt-base and the language-install command chains directly onto the supplied base. Both `hm.zig(base=apt_base)` and `hm.npm(base=apt_base)` produce install steps with `builds_in: <apt-base key>`. Memoization on `@hm.target()` guarantees `apt_base()` returns the same Step object both times, so the IR contains exactly one apt-base node, not two.

### Why this gets parallel containers

`crates/hm/src/orchestrator/graph.rs` partitions the DAG into maximal `builds_in` chains via `Graph::chains()`. `chain_deps` records inter-chain dependencies derived from `depends_on`. Because `:zig: install` and `:node: install` share a parent (`apt-base`) but no `depends_on` edge between them, they live in two independent chains whose only chain-dep is the apt-base chain. After apt-base completes and commits its snapshot, the scheduler's `tokio::sync::Semaphore` releases two permits and the two install chains start concurrently — each booting a fresh container from the apt-base snapshot tag.

The default `--parallelism` in `crates/hm/src/commands/run/local.rs` is the host's CPU count, which is ≥ 2 on every realistic developer machine and on GitHub-hosted runners.

## Components

### `examples/zig-js/.harmont/pipeline.py`

Source-of-truth for the demo. Single file, ~50 lines. Three `@hm.target()`s and one `@hm.pipeline("ci")`. No imperative logic beyond the toolchain factory calls.

### `examples/zig-js/zig-src/`

Verbatim copy of `examples/zig/{build.zig,src/}`. No edits — the zig toolchain expects exactly this layout, and the existing example already passes `hm.zig().build()/.test()/.fmt()`.

### `examples/zig-js/web/`

Verbatim copy of `examples/typescript/{package.json,package-lock.json,tsconfig.json,eslint.config.js,src/}`. Same rationale as zig-src.

### `examples/zig-js/.gitignore`

```
node_modules/
dist/
zig-out/
.zig-cache/
```

### `examples/zig-js/README.md`

~30 lines. States the goal (demo of parallelism), the command (`hm run ci`), what to look for in the output (two `start` lines for `:zig: install` and `:node: install` with overlapping timestamps), and a one-paragraph note explaining the diamond-DAG mechanism.

### `.github/workflows/examples.yml`

Add `- zig-js` to the `run-example` matrix list, alphabetically between `typescript` and `zig`. `validate-matrix` already diffs the matrix against `ls examples/`, so the slug must be added or CI fails.

## Data flow

`hm run ci` from `examples/zig-js/`:

1. `render_pipeline_json` (cli) shells out to `/usr/bin/python3 -c <render-script>`, imports harmont, executes `.harmont/pipeline.py` (triggers the decorators), calls `dump_registry_json()`, prints the `ci` pipeline's v0 IR.
2. `orchestrator::run` decodes the IR into `Pipeline`.
3. `Graph::build` constructs nodes. The host-side `default_image` patch (added in the previous plan) sets `apt-base.step.image = "ubuntu:24.04"` because apt-base has no `builds_in`.
4. `Graph::chains()` produces ~3 chains: `[apt-base, :zig: install, :zig: build]` (chain extends as long as single-child), `[:node: install, :node: deps, :node: build]`, and short leaves. `chain_deps` records that the zig and node chains both depend on the apt-base chain — but not on each other.
5. Scheduler awaits apt-base completion, then releases the two language-install chains concurrently. Their `start` events are emitted by the docker plugin and surface through the broadcast bus into the human output plugin with their wall-clock timestamps.

## Testing

### Local

- `cd examples/zig-js && python3 -c "import importlib.util; ..."` (the canonical render harness in the harmont-py tests) confirms the example renders to v0 IR with exactly one apt-base node and two `builds_in: <apt-base-key>` install nodes.
- `cd examples/zig-js && HM_NONINTERACTIVE=1 /home/marko/harmont-cli/target/debug/hm run ci` completes with `build: end exit=0`.
- Visual check: the human output shows `[:zig: install] start` and `[:node: install] start` lines with timestamps differing by less than the apt-base step's own duration.

### CI

- `validate-matrix` passes (slug added to matrix).
- `run-example (zig-js)` matrix leg completes with exit 0.
- The new leg's duration is roughly `apt-base + max(zig-chain, node-chain) + epsilon`, not the sum — empirical evidence of parallelism. Sequential execution would put the leg's wall-clock at `apt-base + zig-chain + node-chain` (~3-4 minutes); parallel execution puts it at ~2 minutes on GitHub's 2-vCPU runners (still 2 cores → 2 concurrent containers).

### Render regression

The pre-existing parametrized test in `harmont-py/tests/test_examples_render.py` automatically picks up `examples/zig-js/` because it globs `examples/*/.harmont/pipeline.py`. The 18th case is exercised on every harmont-py CI run.

## Error handling

The pipeline uses pre-existing toolchain machinery. Failure modes are inherited:

- Missing `web/package-lock.json` → `FileNotFoundError` at render time (caught by the harmont-py render test).
- `npm ci` fails (e.g., lockfile out of sync with package.json) → step exits non-zero, pipeline reports `chain N: FAILED`.
- Zig install URL 404s → `:zig: install` exits non-zero; same surface.
- Docker daemon unreachable → `hm run` aborts with a clear message before any step runs.

No new error paths are introduced.

## YAGNI / out of scope

- No abstraction for "parallel chains" — the diamond DAG falls out of two targets depending on one parent; that's the abstraction.
- No new `hm.fork()` semantics. Existing `.fork(label=...)` is a passthrough used for IR-level branding only and is unrelated to parallelism (parallelism is a scheduler property, not a DSL property).
- No second pipeline slug (e.g., a `ci-fast` variant). One pipeline, six leaves.
- No `examples/zig-js/tests/` directory. The subprojects' own tests are enough.

## Risks

- **Render-time path resolution:** `hm.zig(path="zig-src", base=apt_base)` and `hm.npm(path="web", ...)` must produce `cd zig-src && ...` and `cd web && ...` commands inside the shared container. Both toolchains already implement this via their `cwd` semantics; the existing examples that use a non-root `path=...` (none in current matrix, but the toolchains support it) confirm.
- **Cache key contamination:** Memoizing `apt_base()` means the same `Step` object is reused, and the keygen pass derives one stable cache key. Two consumers (`zig_install`, `node_install`) reference the same parent key in their `builds_in` field. This is the intended path; if it weren't, the existing rust/python/etc. examples would be broken (they all chain off a per-toolchain apt-base via the same `make_install_chain`).
- **GitHub 2-vCPU runner:** Two containers running simultaneously is feasible (no hyperthread fight; both spend most of their wall-clock waiting on package downloads, not CPU). If a leg flakes due to runner saturation, fall back to documenting "parallelism is bounded by `--parallelism`" rather than reducing the demo.

## Acceptance criteria

1. `examples/zig-js/.harmont/pipeline.py` exists and renders to v0 IR with exactly one apt-base node.
2. The harmont-py parametrized render test (gated by `HARMONT_CLI_PATH`) lists `zig-js` and passes.
3. Local `hm run ci` from `examples/zig-js/` completes with `build: end exit=0`, and inspection of the timestamped output shows `:zig: install` and `:node: install` `start` events overlap.
4. `validate-matrix` passes after `- zig-js` is added to `.github/workflows/examples.yml`.
5. The `run-example (zig-js)` matrix leg completes successfully on push to `main`.
