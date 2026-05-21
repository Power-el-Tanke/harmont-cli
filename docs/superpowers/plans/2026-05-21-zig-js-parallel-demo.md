# zig + js Parallel Demo — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `examples/zig-js/`, an 18th example whose pipeline shows harmont's chain-level parallelism — a single `apt-base` step that forks into two concurrent install chains (Zig and Node).

**Architecture:** New example dir at `examples/zig-js/` containing copies of the existing `zig` and `typescript` sub-projects plus a single `.harmont/pipeline.py` that declares one shared `@hm.target() apt_base` and two project targets that thread `base=apt_base` into `hm.zig(...)` and `hm.npm(...)`. The toolchains' `make_install_chain(base=...)` path skips its own apt-base and chains the language install directly onto the shared parent. Result: one apt-base node, two `builds_in: <apt-base>` siblings, scheduler runs them in parallel containers. No harmont-py changes — uses pre-existing surface.

**Tech Stack:** harmont-py DSL (Python 3.11+), harmont-cli (Rust workspace), Docker, GitHub Actions.

**Working assumptions:**
- Reference design: `/home/marko/harmont-cli/docs/superpowers/specs/2026-05-21-zig-js-parallel-demo-design.md`.
- Working directory: `/home/marko/harmont-cli/` (on `main`, clean tree). harmont-py is installed system-wide.
- Docker daemon running locally; `hm` binary already built at `target/debug/hm`.
- The previous plan (`examples-passing`) already landed: all 17 existing examples are green in CI.

**Direct-to-main:** Per the project's established workflow, commits land directly on `main`.

---

## File Map

### `/home/marko/harmont-cli/`

- **Create:** `examples/zig-js/.harmont/pipeline.py` — the pipeline (3 targets + 1 pipeline factory)
- **Create:** `examples/zig-js/README.md` — what to look for in the output
- **Create:** `examples/zig-js/.gitignore`
- **Create:** `examples/zig-js/zig-src/build.zig` — copy from `examples/zig/build.zig`
- **Create:** `examples/zig-js/zig-src/src/main.zig` — copy from `examples/zig/src/main.zig`
- **Create:** `examples/zig-js/zig-src/src/root.zig` — copy from `examples/zig/src/root.zig`
- **Create:** `examples/zig-js/web/package.json` — copy from `examples/typescript/package.json` with `"name"` renamed
- **Create:** `examples/zig-js/web/package-lock.json` — regenerated for the renamed package
- **Create:** `examples/zig-js/web/tsconfig.json` — copy from `examples/typescript/tsconfig.json`
- **Create:** `examples/zig-js/web/eslint.config.js` — copy from `examples/typescript/eslint.config.js`
- **Create:** `examples/zig-js/web/src/index.ts` — copy from `examples/typescript/src/index.ts`
- **Create:** `examples/zig-js/web/src/index.test.ts` — copy from `examples/typescript/src/index.test.ts`
- **Modify:** `.github/workflows/examples.yml` — insert `- zig-js` in the matrix list alphabetically (between `typescript` and `zig`).

No harmont-py changes.

---

## Task 1: Scaffold the example dir and subprojects

**Why first:** Establish the file structure before authoring the pipeline. Each later task can assume the layout exists.

**Files:**
- Create: `examples/zig-js/zig-src/{build.zig,src/main.zig,src/root.zig}`
- Create: `examples/zig-js/web/{package.json,tsconfig.json,eslint.config.js,src/index.ts,src/index.test.ts}`
- Create: `examples/zig-js/.gitignore`

- [ ] **Step 1: Make the directory tree and copy zig-src verbatim**

```bash
cd /home/marko/harmont-cli
mkdir -p examples/zig-js/.harmont examples/zig-js/zig-src/src examples/zig-js/web/src
cp examples/zig/build.zig examples/zig-js/zig-src/build.zig
cp examples/zig/src/main.zig examples/zig-js/zig-src/src/main.zig
cp examples/zig/src/root.zig examples/zig-js/zig-src/src/root.zig
```

Expected: three zig files present under `examples/zig-js/zig-src/`. Verify with `ls -R examples/zig-js/zig-src/`.

- [ ] **Step 2: Copy the typescript sources into `web/`**

```bash
cp examples/typescript/package.json examples/zig-js/web/package.json
cp examples/typescript/tsconfig.json examples/zig-js/web/tsconfig.json
cp examples/typescript/eslint.config.js examples/zig-js/web/eslint.config.js
cp examples/typescript/src/index.ts examples/zig-js/web/src/index.ts
cp examples/typescript/src/index.test.ts examples/zig-js/web/src/index.test.ts
```

Verify: `ls -R examples/zig-js/web/` shows package.json, tsconfig.json, eslint.config.js, src/index.ts, src/index.test.ts.

- [ ] **Step 3: Rename the web package**

Edit `examples/zig-js/web/package.json` — change `"name": "harmont-example-typescript"` to `"name": "harmont-example-zig-js-web"`. Leave every other field as-is.

Verify with `grep '"name"' examples/zig-js/web/package.json` — expected output: `  "name": "harmont-example-zig-js-web",`.

- [ ] **Step 4: Regenerate package-lock.json for the renamed package**

```bash
cd /home/marko/harmont-cli/examples/zig-js/web
npm install --package-lock-only
```

The lockfile embeds the package name; regenerating from the renamed `package.json` keeps it in sync. Expected: `package-lock.json` written; no `node_modules/` populated (the `--package-lock-only` flag skips that).

If `npm install --package-lock-only` errors with a peer-deps conflict, retry once with `--legacy-peer-deps`. Do not run `npm install` (without the flag) — we don't want `node_modules/` on disk.

Verify: `ls package-lock.json` succeeds.

- [ ] **Step 5: Write the .gitignore**

Create `/home/marko/harmont-cli/examples/zig-js/.gitignore`:

```
node_modules/
dist/
zig-out/
.zig-cache/
```

- [ ] **Step 6: Render-time sanity check (the pipeline file doesn't exist yet, so this is a layout-only check)**

```bash
cd /home/marko/harmont-cli/examples/zig-js
ls .harmont/   # should be empty for now
ls zig-src/    # build.zig src/
ls web/        # eslint.config.js package-lock.json package.json src tsconfig.json
```

- [ ] **Step 7: Commit**

```bash
cd /home/marko/harmont-cli
git add examples/zig-js/zig-src examples/zig-js/web examples/zig-js/.gitignore
git commit -m "$(cat <<'EOF'
examples/zig-js: scaffold zig-src + web subprojects

Lifts examples/zig/{build.zig,src/} into zig-src/ and
examples/typescript/{package.json,package-lock.json,tsconfig.json,
eslint.config.js,src/} into web/ verbatim, with the npm package
renamed to harmont-example-zig-js-web and the lockfile regenerated
against the new name.

Pipeline + README come in the next two commits.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: Author the demo pipeline

**Why next:** The pipeline is the load-bearing artifact. The harmont-py render test in `tests/test_examples_render.py` automatically picks up the new `zig-js` directory once `.harmont/pipeline.py` is in place — TDD via the existing test.

**Files:**
- Create: `examples/zig-js/.harmont/pipeline.py`

- [ ] **Step 1: Confirm the existing render test fails on absent pipeline.py**

```bash
HARMONT_CLI_PATH=/home/marko/harmont-cli python3 -m pytest \
  /home/marko/harmont-py/tests/test_examples_render.py -v 2>&1 | tail -5
```

Expected: `17 passed` (zig-js not yet collected because the harness filters on `p / ".harmont" / "pipeline.py"` being a file — see `tests/test_examples_render.py:_example_dirs`). This is the baseline.

- [ ] **Step 2: Write the pipeline file**

Create `/home/marko/harmont-cli/examples/zig-js/.harmont/pipeline.py`:

```python
"""zig + js parallelism demo.

A single apt-base step forks into two install chains (Zig and Node)
that run in parallel containers. Watch [:zig: install] and
[:node: install] start events overlap when this runs locally.
"""
from __future__ import annotations

from datetime import timedelta
from typing import Annotated

import harmont as hm
from harmont.npm import NpmProject
from harmont.zig import ZigProject


@hm.target()
def apt_base(base: Annotated[hm.Step, hm.BaseImage("ubuntu:24.04")]) -> hm.Step:
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

- [ ] **Step 3: Render the pipeline directly to confirm it produces valid IR**

```bash
cd /home/marko/harmont-cli/examples/zig-js
python3 -c "
import importlib.util, json
spec = importlib.util.spec_from_file_location('pipeline', '.harmont/pipeline.py')
m = importlib.util.module_from_spec(spec); spec.loader.exec_module(m)
import harmont as hm
envelope = json.loads(hm.dump_registry_json())
ci = next(p for p in envelope['pipelines'] if p['slug'] == 'ci')
steps = ci['definition']['steps']
apt = [s for s in steps if s['key'].startswith('apt-base') or s.get('label') == ':apt: base']
zig_install = [s for s in steps if 'zig' in (s.get('label') or '') and 'install' in (s.get('label') or '')]
node_install = [s for s in steps if 'node' in (s.get('label') or '') and 'install' in (s.get('label') or '')]
print('apt-base count:', len(apt), [s['key'] for s in apt])
print('zig-install builds_in:', [s.get('builds_in') for s in zig_install])
print('node-install builds_in:', [s.get('builds_in') for s in node_install])
assert len(apt) == 1, 'expected exactly one apt-base node'
shared_parent = apt[0]['key']
assert all(s.get('builds_in') == shared_parent for s in zig_install + node_install), \
    'zig-install and node-install must both builds_in to the shared apt-base'
print('OK: diamond confirmed')
"
```

Expected: prints `apt-base count: 1`, two `builds_in` values equal to the apt-base key, and `OK: diamond confirmed`. If `apt-base count` is 2, the `@hm.target()` memoization is not working — investigate before continuing.

- [ ] **Step 4: Run the harmont-py parametrized render test**

```bash
HARMONT_CLI_PATH=/home/marko/harmont-cli python3 -m pytest \
  /home/marko/harmont-py/tests/test_examples_render.py -v 2>&1 | tail -10
```

Expected: `18 passed` (the harness now picks up `zig-js`). If `zig-js` fails, the test output points at the assertion that triggered (likely `default_image` missing or `ci` slug not registered).

- [ ] **Step 5: Commit**

```bash
cd /home/marko/harmont-cli
git add examples/zig-js/.harmont/pipeline.py
git commit -m "$(cat <<'EOF'
examples/zig-js: pipeline with shared apt-base fork

Three @hm.target()s: apt_base (the shared root, ubuntu:24.04 + curl/
ca-certs/xz-utils), zig_project (hm.zig with base=apt_base), and
web_project (hm.npm with base=apt_base). The ci pipeline emits six
leaves — three zig actions, three npm actions — chained off the two
install steps that both build_in to apt_base.

The orchestrator's chain partitioner places zig-install and
node-install in independent chains with the same chain-dep (apt-base
chain) and no inter-chain dep, so they run in parallel containers.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: Write the README

**Files:**
- Create: `examples/zig-js/README.md`

- [ ] **Step 1: Write the README**

Create `/home/marko/harmont-cli/examples/zig-js/README.md`:

```markdown
# zig + js parallel demo

This example exists to make harmont's chain-level parallelism
visible. The pipeline declares one shared `apt-base` step that
both a Zig install chain and a Node install chain build off of.
The orchestrator places those two chains in independent slots in
its chain DAG, so once `apt-base` commits its snapshot the Zig and
Node installs run in two parallel Docker containers — each
booting from the same apt-base snapshot.

## Run it

```sh
hm run ci
```

## What to look for

In the human-format output, the two language-install steps emit
their `start` events within milliseconds of each other and their
`end` events with overlapping wall-clock spans:

```
[:apt: base] start (runner=docker)
[:apt: base] end exit=0 duration=11234ms
[:zig: install] start (runner=docker)
[:node: install] start (runner=docker)         ← starts before zig-install ends
[:zig: install] end exit=0 duration=27411ms
[:node: install] end exit=0 duration=18302ms
```

If the two `start` lines are separated by more than ~100ms, the
scheduler did not parallelise the chains — check `--parallelism`
(default = CPU count) and that the two install steps share no
`depends_on` edge in the rendered v0 IR.

## How the diamond is built

The pipeline registers three `@hm.target()`s and one
`@hm.pipeline("ci")`. The `apt_base` target is declared as a
parameter on both `zig_project` and `web_project`; harmont-py's
fixture-style DI passes the same memoized `Step` to both, so the
emitted IR contains exactly one apt-base node and both language
installs carry `builds_in: <apt-base key>`. Memoization is the
mechanism — there is no special "fork" primitive at the DSL level.

## Subprojects

- `zig-src/` — a tiny Zig library + test, copied from
  `examples/zig/`.
- `web/` — a tiny TypeScript library + vitest, copied from
  `examples/typescript/`.

Both are built and tested by their respective toolchains
(`hm.zig`, `hm.npm`) using the standard action methods.
```

- [ ] **Step 2: Commit**

```bash
cd /home/marko/harmont-cli
git add examples/zig-js/README.md
git commit -m "$(cat <<'EOF'
examples/zig-js: document what to look for in the output

Short README pointing readers at the two language-install `start`
events that should overlap, and explaining the memoized
@hm.target apt_base as the mechanism (no special fork primitive).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: Run the example end-to-end locally

**Why:** Confirm the example actually executes under `hm run` against the local Docker daemon, and the parallelism is observable.

**Files:** none.

- [ ] **Step 1: Confirm hm binary is present**

```bash
ls /home/marko/harmont-cli/target/debug/hm
```

Expected: file present. If missing, run `cargo build -p harmont-cli` from `/home/marko/harmont-cli`.

- [ ] **Step 2: First execution (cold cache)**

```bash
cd /home/marko/harmont-cli/examples/zig-js
HM_NONINTERACTIVE=1 /home/marko/harmont-cli/target/debug/hm run ci 2>&1 | tee /tmp/zig-js-cold.log | tail -50
```

Expected: pipeline completes with `build: end exit=0`. Total wall-clock typically 1–3 minutes cold. Inspect `/tmp/zig-js-cold.log` and verify:
- Exactly one `[:apt: base] start` line.
- Two install-step `start` lines (`[:zig: install]` and `[:node: install]`) appearing AFTER `[:apt: base] end` and within ~1s of each other.
- Six action leaves (`:zig: build`, `:zig: test`, `:zig: fmt`, `:node: build`, `:node: test`, `:node: lint`) all end with `exit=0`.

- [ ] **Step 3: Confirm parallelism in the timestamps**

```bash
awk '/start \(runner=docker\)/ || /end exit=/' /tmp/zig-js-cold.log | head -30
```

Expected: lines for `:zig: install` and `:node: install` interleaved — typically the `start` events appear back-to-back, and the `end` of `:node: install` arrives during the runtime of `:zig: install` (or vice versa). If the two are strictly sequential (`:zig: install start → :zig: install end → :node: install start`), parallelism is broken; investigate `chain_deps` in the rendered IR.

- [ ] **Step 4: Re-run warm (snapshot hits) to confirm idempotence**

```bash
cd /home/marko/harmont-cli/examples/zig-js
HM_NONINTERACTIVE=1 /home/marko/harmont-cli/target/debug/hm run ci 2>&1 | tail -10
```

Expected: completes in <10s with cache hits on apt-base, zig-install, node-install, and npm-ci.

- [ ] **Step 5: No commit needed (verification-only task)**

Continue to Task 5.

---

## Task 5: Wire into the CI matrix

**Files:**
- Modify: `.github/workflows/examples.yml`

- [ ] **Step 1: Add the matrix slug alphabetically**

Edit `/home/marko/harmont-cli/.github/workflows/examples.yml`. Locate the matrix list:

```yaml
        example:
          - c
          - cpp
          - csharp
          - go
          - haskell
          - java
          - kotlin
          - nextjs
          - ocaml
          - perl
          - php-laravel
          - python-uv
          - react
          - ruby
          - rust
          - typescript
          - zig
```

Insert `- zig-js` between `typescript` and `zig`:

```yaml
        example:
          - c
          - cpp
          - csharp
          - go
          - haskell
          - java
          - kotlin
          - nextjs
          - ocaml
          - perl
          - php-laravel
          - python-uv
          - react
          - ruby
          - rust
          - typescript
          - zig-js
          - zig
```

- [ ] **Step 2: Run validate-matrix locally**

```bash
cd /home/marko/harmont-cli
on_disk="$(ls examples | grep -v '^README\.md$' | sort)"
in_matrix="$(awk '/^        example:$/{flag=1;next}/^    steps:$/{flag=0}flag' .github/workflows/examples.yml \
  | sed -n 's/^\s*- //p' | sort)"
diff <(printf '%s\n' "$on_disk") <(printf '%s\n' "$in_matrix") && echo "matrix in sync"
```

Expected: `matrix in sync` printed; `diff` exits 0. If a discrepancy appears, the new slug is missing or misspelt — re-edit.

- [ ] **Step 3: Validate yaml**

```bash
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/examples.yml'))" && echo yaml-ok
```

Expected: `yaml-ok`.

- [ ] **Step 4: Commit**

```bash
cd /home/marko/harmont-cli
git add .github/workflows/examples.yml
git commit -m "$(cat <<'EOF'
ci(examples): add zig-js to the run-example matrix

New 18th leg exercising the @hm.target apt_base fork pattern that
runs Zig and Node installs in parallel containers off a single
shared apt-base snapshot.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 6: Push, watch CI, react

- [ ] **Step 1: Push main**

```bash
cd /home/marko/harmont-cli
git log --oneline origin/main..HEAD
git push origin main
```

Expected: 4 commits pushed (scaffold, pipeline, README, CI).

- [ ] **Step 2: Watch the new run**

```bash
RUN_ID=$(gh run list --repo harmont-dev/harmont-cli --workflow examples.yml --limit 1 --json databaseId --jq '.[0].databaseId')
echo "$RUN_ID"
gh run watch "$RUN_ID" --repo harmont-dev/harmont-cli --exit-status 2>&1 | tail -30
```

Expected: every job ends with ✓, including the new `zig-js` leg.

- [ ] **Step 3: On failure, triage**

```bash
gh run view --repo harmont-dev/harmont-cli --log-failed "$RUN_ID" | head -200
```

Likely failures:
- `npm ci` peer-deps conflict — fix the lockfile or add a peer-deps workaround in `package.json`.
- `:zig: install` URL 404 — the zig install tarball URL changed upstream; bump the version in `harmont/zig.py` if needed (harmont-py change, push separately).
- Sequential timestamps — open `/tmp/zig-js-cold.log` from the leg's logs and verify the IR has no spurious `depends_on` edges connecting the two install chains.

- [ ] **Step 4: Done**

When the run is green, the demo is live. The example appears in the matrix and the harmont-py parametrized render test counts 18 cases.

---

## Out of scope

- No new harmont-py public API. The `base=` toolchain kwarg and `@hm.target()` are pre-existing.
- No orchestrator changes. Parallelism is the default scheduler behavior for independent chains.
- No screenshot or recording artifacts checked into the repo.
- No second pipeline slug or "fast/slow" variant. Six leaves is enough.
- No edits to `examples/zig/` or `examples/typescript/` — the originals stay untouched.

---

## Self-review checklist (run after writing)

- Spec coverage: pipeline ✓ (Task 2); subproject sources ✓ (Task 1); README ✓ (Task 3); local validation ✓ (Task 4); CI wiring ✓ (Task 5); push + watch ✓ (Task 6); regression coverage via existing render test ✓ (Task 2 Step 4).
- No placeholders: every step has exact paths, exact commands, exact expected output.
- Type consistency: `apt_base`, `zig_project`, `web_project`, `ci`, `ZigProject`, `NpmProject`, `hm.Target[…]`, `hm.BaseImage("…")`, `hm.ttl(timedelta(...))` — all match harmont-py's actual public surface (verified during design phase).
- File paths: every Create/Modify path is absolute or repo-relative; no ambiguous "the file".
