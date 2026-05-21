# Publishable `harmont-cli` Crate — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `cargo publish -p harmont-cli` succeed end-to-end (including the publish-verify build inside cargo's sandbox) AND make `cargo install harmont-cli` work for end users — by pre-building the four embedded WASM plugins in CI and bundling them inside the published crate tarball.

**Architecture:** Today `crates/hm/build.rs` cross-compiles four sibling crates (`hm-plugin-docker`, `hm-plugin-output-human`, `hm-plugin-output-json`, `hm-plugin-cloud`) to `wasm32-wasip1` and stages the artifacts under `$OUT_DIR` for `include_bytes!`. That works in-workspace but fails when `cargo publish` extracts the crate into `target/package/harmont-cli-<ver>/` because the sibling crates are no longer reachable. Same failure shape would hit any end user doing `cargo install harmont-cli`. Fix: introduce a `crates/hm/embedded/` directory; have `build.rs` prefer `embedded/<name>.wasm` when present (the "published" path) and fall back to the existing in-workspace cross-compile (the dev path); add the `embedded/` directory to the crate's `include = [...]` so the wasms ship inside the published tarball; have `release.yml` pre-build the wasms and stage them into `embedded/` before invoking `cargo publish`.

**Tech Stack:** Rust workspace, `cargo publish` verify build, `wasm32-wasip1` target, Extism WASM plugins, GitHub Actions.

**Direct-to-main:** Commits land on `main` of `/home/marko/harmont-cli/`. The user will re-tag `v0.0.1` (still free on crates.io) or move to `v0.0.2` once the plan lands.

---

## File Map

### `/home/marko/harmont-cli/`

- **Modify:** `crates/hm/build.rs` — branch on the existence of `embedded/<name>.wasm`. If present, copy from there. Otherwise, fall through to the existing cross-compile path.
- **Modify:** `crates/hm/Cargo.toml` — add `include = [...]` enumerating the source dirs/files that should be in the published tarball, with `embedded/*.wasm` among them. Without this, cargo defaults to "everything tracked by git", but `embedded/` will be gitignored (next bullet).
- **Modify:** `crates/hm/.gitignore` (create it if absent) — ignore `embedded/*.wasm`. The wasms are build artifacts produced by CI, not source code, so they should not live in git.
- **Modify:** `.github/workflows/release.yml` — before the harmont-cli publish step, build the 4 wasms via `cargo build --target wasm32-wasip1 -p <plugin> --release` and copy each `target/wasm32-wasip1/release/<name>.wasm` to `crates/hm/embedded/<name>.wasm`. This way cargo's publish-verify-build sees the pre-built wasms in the extracted sandbox.
- **Create:** `crates/hm/embedded/.gitkeep` — ensure the directory exists even without committed wasms (so `include = ["embedded/*.wasm"]` in dev checkouts doesn't trip over a missing dir; cargo treats glob misses as zero matches, but a missing dir is fine — `.gitkeep` is just for repo cleanliness).

No source code changes inside `crates/hm/src/`. The `include_bytes!(concat!(env!("OUT_DIR"), …))` line in `plugin/embedded.rs` continues to work unchanged because `build.rs` still writes wasms to `$OUT_DIR` — just with two possible *sources* for those bytes now.

---

## Task 1: Teach `build.rs` to prefer bundled wasms when present

**Why first:** This is the load-bearing change. Once `build.rs` accepts both paths, the rest is plumbing.

**Files:**
- Modify: `/home/marko/harmont-cli/crates/hm/build.rs`

- [ ] **Step 1: Read the current file**

Open `/home/marko/harmont-cli/crates/hm/build.rs`. Confirm it matches the shape captured in the plan: a 58-line file with one helper `build_wasm_plugin(crate_name)` that runs `cargo build --target wasm32-wasip1 -p <crate>` from `../..` then copies the output to `$OUT_DIR`. The plan's edit below assumes that exact shape; if the file diverged, adapt minimally.

- [ ] **Step 2: Replace the helper with a two-mode version**

Edit `/home/marko/harmont-cli/crates/hm/build.rs`. Replace the existing `build_wasm_plugin` function in its entirety with:

```rust
fn build_wasm_plugin(crate_name: &str) {
    let underscore = crate_name.replace('-', "_");
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let dest = out_dir.join(format!("{underscore}.wasm"));

    // Bundled-wasm path: `cargo publish` extracts this crate into an
    // isolated `target/package/harmont-cli-<ver>/` and runs build.rs
    // there. The sibling plugin crates aren't reachable from that
    // sandbox (and aren't reachable for `cargo install harmont-cli`
    // end users either). Release CI pre-builds the wasms and stages
    // them under `crates/hm/embedded/` before invoking `cargo publish`,
    // and the `include = [...]` in Cargo.toml carries them into the
    // tarball. When build.rs sees a pre-built file there, just copy.
    let bundled = PathBuf::from(format!("embedded/{underscore}.wasm"));
    println!("cargo:rerun-if-changed={}", bundled.display());
    if bundled.is_file() {
        fs::copy(&bundled, &dest).unwrap_or_else(|e| {
            panic!("copy bundled {} -> {}: {e}", bundled.display(), dest.display())
        });
        return;
    }

    // Dev path: cross-compile from the sibling crate in the workspace.
    use std::process::Command;

    let src = format!("../{crate_name}/src");
    let cargo_toml = format!("../{crate_name}/Cargo.toml");
    println!("cargo:rerun-if-changed={src}");
    println!("cargo:rerun-if-changed={cargo_toml}");

    let status = Command::new(env::var("CARGO").as_deref().unwrap_or("cargo"))
        .args([
            "build",
            "--target",
            "wasm32-wasip1",
            "-p",
            crate_name,
            "--release",
        ])
        .current_dir("../..")
        .status()
        .unwrap_or_else(|e| panic!("invoke cargo build for {crate_name}: {e}"));
    assert!(status.success(), "{crate_name} wasm build failed");

    let src_wasm = PathBuf::from(format!(
        "../../target/wasm32-wasip1/release/{underscore}.wasm"
    ));
    fs::copy(&src_wasm, &dest)
        .unwrap_or_else(|e| panic!("copy {} -> {}: {e}", src_wasm.display(), dest.display()));
}
```

Leave the `main`, `build_embedded_plugins`, and the file's allow-list at the top unchanged.

- [ ] **Step 3: Confirm the dev path still works**

```bash
cd /home/marko/harmont-cli
cargo build -p harmont-cli 2>&1 | tail -5
```

Expected: `Finished … target(s) in …`. The build succeeds because `embedded/<name>.wasm` doesn't exist, so build.rs falls through to the dev (cross-compile) path. This is the regression check that we didn't accidentally break the workspace dev workflow.

- [ ] **Step 4: Confirm the bundled path works**

```bash
cd /home/marko/harmont-cli
mkdir -p crates/hm/embedded
cp target/wasm32-wasip1/release/hm_plugin_docker.wasm crates/hm/embedded/
cp target/wasm32-wasip1/release/hm_plugin_output_human.wasm crates/hm/embedded/
cp target/wasm32-wasip1/release/hm_plugin_output_json.wasm crates/hm/embedded/
cp target/wasm32-wasip1/release/hm_plugin_cloud.wasm crates/hm/embedded/
ls crates/hm/embedded/

# Force a rebuild of crates/hm to make build.rs run again.
touch crates/hm/build.rs
cargo build -p harmont-cli 2>&1 | tail -5
```

Expected: `Finished … target(s) in …`. Inspect the build.rs `cargo:rerun-if-changed` output (visible with `-vv` if you want) — the bundled branch should have been taken. Don't try to assert "no cargo build of sibling crate happened" automatically; the prior `cargo build` step populated those crates' build artifacts so they wouldn't re-link anyway. The functional check is "build succeeds with `embedded/*.wasm` in place"; that's enough.

- [ ] **Step 5: Clean up the staged wasms**

```bash
rm -rf /home/marko/harmont-cli/crates/hm/embedded/
```

The `embedded/` dir is .gitignored (Task 3); we don't want stale wasms persisting on disk between dev sessions.

- [ ] **Step 6: Commit**

```bash
cd /home/marko/harmont-cli
git add crates/hm/build.rs
git commit -m "$(cat <<'EOF'
fix(build): prefer pre-built embedded/*.wasm when present

cargo publish extracts harmont-cli into a sandbox without the
sibling plugin crates, so the in-workspace cross-compile path in
build.rs fails. Same failure hits `cargo install harmont-cli` for
end users.

Add a fast path: if crates/hm/embedded/<name>.wasm exists, copy it
straight to OUT_DIR for include_bytes!. Otherwise fall through to
the existing cross-compile path. Release CI populates embedded/
just before `cargo publish`, and the next commit adds the
include = [...] that carries the staged wasms into the tarball.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: Ship the embedded wasms inside the published tarball

**Why next:** With Task 1 alone, `cargo publish` still fails — even if CI stages wasms into `crates/hm/embedded/`, cargo's default file inclusion is "everything tracked by git", which excludes our soon-to-be-gitignored `embedded/` directory. Add an explicit `include = [...]` so cargo packages source files **plus** the staged wasms.

**Files:**
- Modify: `/home/marko/harmont-cli/crates/hm/Cargo.toml`

- [ ] **Step 1: Identify the package shape to include**

```bash
cd /home/marko/harmont-cli/crates/hm
ls -F
```

Expected entries the published tarball must carry: `src/`, `build.rs`, `Cargo.toml`, `README.md`, plus the new `embedded/`. `tests/` and `benches/` (if any) are not needed in the published crate — `cargo publish` is for downstream consumers, not for running our test suite.

- [ ] **Step 2: Add the include list**

Edit `/home/marko/harmont-cli/crates/hm/Cargo.toml`. After the line `categories = [...]` (around line 10) and before the next section header (`[lib]`), insert:

```toml
# Explicit include list: cargo's default ("everything tracked by git")
# would drop crates/hm/embedded/*.wasm (gitignored — they are CI-built
# artifacts, not source). The release workflow stages the four
# embedded plugin wasms into embedded/ before invoking `cargo publish`,
# and this glob carries them into the tarball.
include = [
    "src/**/*",
    "build.rs",
    "Cargo.toml",
    "README.md",
    "embedded/*.wasm",
]
```

The block ends up at lines 11–22 of the new file. Don't touch any other field.

- [ ] **Step 3: Local `cargo package` smoke test (no publish)**

```bash
cd /home/marko/harmont-cli
mkdir -p crates/hm/embedded
cp target/wasm32-wasip1/release/hm_plugin_docker.wasm crates/hm/embedded/
cp target/wasm32-wasip1/release/hm_plugin_output_human.wasm crates/hm/embedded/
cp target/wasm32-wasip1/release/hm_plugin_output_json.wasm crates/hm/embedded/
cp target/wasm32-wasip1/release/hm_plugin_cloud.wasm crates/hm/embedded/

cargo package -p harmont-cli --allow-dirty --no-verify 2>&1 | tail -20
```

`--allow-dirty` is required because we staged untracked files (the wasms) into `crates/hm/embedded/`. `--no-verify` is required because `cargo package` would otherwise try to invoke the same verify build we're about to test in the next step; we want to inspect the tarball first.

Expected: a tarball is written to `target/package/harmont-cli-0.0.0-dev.crate`. (The current `Cargo.toml` has `version = "0.0.0-dev"`; that's the dev marker `release.yml` substitutes from the tag — so the local tarball name carries `-dev`.)

```bash
tar tzf /home/marko/harmont-cli/target/package/harmont-cli-0.0.0-dev.crate | grep -E '\.wasm$'
```

Expected output: four `.wasm` entries, paths ending in `harmont-cli-0.0.0-dev/embedded/hm_plugin_docker.wasm`, `…/hm_plugin_output_human.wasm`, `…/hm_plugin_output_json.wasm`, `…/hm_plugin_cloud.wasm`. If any are missing, the `include` glob is wrong — fix and re-run.

- [ ] **Step 4: Verify build inside the extracted sandbox**

This mimics what `cargo publish` runs:

```bash
cd /home/marko/harmont-cli/target/package
tar xzf harmont-cli-0.0.0-dev.crate
cd harmont-cli-0.0.0-dev
ls embedded/
# Build in isolation. We need to pretend the sibling workspace
# isn't there; use --offline + a fresh target dir.
CARGO_TARGET_DIR=/tmp/verify-harmont-cli-target cargo build --offline 2>&1 | tail -20 || \
  CARGO_TARGET_DIR=/tmp/verify-harmont-cli-target cargo build 2>&1 | tail -20
```

Expected: `Finished … target(s) in …`. The build script will not invoke any sibling-crate cross-compile because `embedded/*.wasm` is present; it'll just copy. If the build fails complaining about `target/wasm32-wasip1/release/<name>.wasm` not existing, the bundled-path branch in build.rs is not being taken — re-inspect the `bundled.is_file()` check.

Try `--offline` first; if cargo needs to fetch transitive crates it doesn't have cached, drop the flag. Either way the verify build should not need any of the sibling plugin crates.

- [ ] **Step 5: Clean up**

```bash
rm -rf /home/marko/harmont-cli/target/package/harmont-cli-0.0.0-dev/
rm -rf /home/marko/harmont-cli/crates/hm/embedded/
rm -rf /tmp/verify-harmont-cli-target
```

- [ ] **Step 6: Commit**

```bash
cd /home/marko/harmont-cli
git add crates/hm/Cargo.toml
git commit -m "$(cat <<'EOF'
build(hm): ship embedded/*.wasm in the published crate tarball

cargo's default file-inclusion is "everything tracked by git", but
crates/hm/embedded/ is gitignored — those four .wasm files are
release-CI artifacts, not source. Add an explicit `include = [...]`
so the staged wasms ride inside the published tarball next to
src/, build.rs, Cargo.toml, and README.md.

Pairs with the build.rs bundled-path fast path: with both commits,
cargo publish's verify-build (and `cargo install harmont-cli` on
an end-user's machine) finds the wasms already cooked.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: Gitignore `embedded/*.wasm`

**Why:** Wasms are build artifacts. They must NOT live in git (binary churn, bloat, signing risk). The release workflow stages them on every run.

**Files:**
- Create or modify: `/home/marko/harmont-cli/crates/hm/.gitignore`

- [ ] **Step 1: Check what already exists**

```bash
ls /home/marko/harmont-cli/crates/hm/.gitignore 2>&1
cat /home/marko/harmont-cli/.gitignore 2>&1 | head -20
```

If `crates/hm/.gitignore` does not exist, create it. If it does, append to it. Either way, the parent `.gitignore` likely has a workspace-wide `target/` rule; we want a narrower rule co-located with `embedded/`.

- [ ] **Step 2: Write the ignore rule**

Create or append `/home/marko/harmont-cli/crates/hm/.gitignore`:

```
# Pre-built WASM plugins staged by .github/workflows/release.yml
# before `cargo publish` runs. Generated on every release; never
# committed.
embedded/*.wasm
```

If the file already exists, add the block at the bottom (separate with a blank line if there's other content).

- [ ] **Step 3: Confirm git ignores it**

```bash
cd /home/marko/harmont-cli
mkdir -p crates/hm/embedded
touch crates/hm/embedded/fake.wasm
git status --short crates/hm/
```

Expected: `crates/hm/.gitignore` is the only path showing as new/modified (assuming you haven't committed Task 3 yet). The `fake.wasm` does NOT appear in `git status`.

```bash
rm crates/hm/embedded/fake.wasm
rmdir crates/hm/embedded
```

- [ ] **Step 4: Commit**

```bash
cd /home/marko/harmont-cli
git add crates/hm/.gitignore
git commit -m "$(cat <<'EOF'
build(hm): gitignore embedded/*.wasm

The embedded/ directory is populated by the release workflow with
freshly-built plugin wasms before each `cargo publish`. The .wasm
artifacts are CI output, not source — keep them out of git so the
working tree stays clean and binaries don't bloat history.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: Pre-build wasms in the release workflow

**Why:** This is the CI side of the change. Without it, no one stages `embedded/*.wasm` before `cargo publish -p harmont-cli` runs, and the publish fails the same way it did at the start of this plan.

**Files:**
- Modify: `/home/marko/harmont-cli/.github/workflows/release.yml`

- [ ] **Step 1: Re-read the current workflow**

Open `/home/marko/harmont-cli/.github/workflows/release.yml`. The current order is:

1. checkout
2. dtolnay/rust-toolchain@stable (with `targets: wasm32-wasip1`)
3. Swatinem/rust-cache@v2
4. Set version from tag (sed + `cargo check --workspace --exclude hm-fixtures`)
5. Publish hm-plugin-protocol
6. Sleep 30
7. Publish hm-plugin-sdk
8. Sleep 30
9. Publish harmont-cli   ← this is where it fails

We need a new step **between 8 and 9** that builds the 4 wasms and stages them into `crates/hm/embedded/`.

- [ ] **Step 2: Insert the wasm-staging step**

Edit `/home/marko/harmont-cli/.github/workflows/release.yml`. Find the second `Wait for crates.io index` step (the one between the hm-plugin-sdk publish and the harmont-cli publish — currently around lines 56–58). After it, before the `Publish harmont-cli` step, insert:

```yaml
      - name: Build embedded WASM plugins
        # The harmont-cli build.rs prefers crates/hm/embedded/*.wasm
        # over the in-workspace cross-compile. Stage them here so the
        # `cargo publish -p harmont-cli` verify-build (which runs in
        # target/package/harmont-cli-<ver>/ without the sibling plugin
        # crates) and any downstream `cargo install harmont-cli` both
        # find the wasms already cooked. The include = [...] in
        # crates/hm/Cargo.toml carries them into the tarball.
        run: |
          set -euo pipefail
          for crate in hm-plugin-docker hm-plugin-output-human hm-plugin-output-json hm-plugin-cloud; do
            cargo build --target wasm32-wasip1 -p "$crate" --release
          done
          mkdir -p crates/hm/embedded
          for name in hm_plugin_docker hm_plugin_output_human hm_plugin_output_json hm_plugin_cloud; do
            cp "target/wasm32-wasip1/release/$name.wasm" "crates/hm/embedded/$name.wasm"
          done
          ls -la crates/hm/embedded/
```

The step must run after the version-sed has happened (because the wasms are baked into a tarball stamped with the tag's version) and after the upstream plugin crates have been published (so the publish-verify dependency-resolve doesn't have to wait on crates.io indexing — that's already handled by the existing sleeps). Inserting it directly before the harmont-cli publish satisfies both.

- [ ] **Step 3: Add `--allow-dirty` to the harmont-cli publish call**

The wasm-staging step in Step 2 creates `crates/hm/embedded/*.wasm` files that aren't tracked by git. `cargo publish` refuses to package a dirty tree without `--allow-dirty`. Find the `Publish harmont-cli` step:

```yaml
      - name: Publish harmont-cli
        run: |
          if curl -sf "https://crates.io/api/v1/crates/harmont-cli/$VERSION" > /dev/null 2>&1; then
            echo "harmont-cli@$VERSION already published, skipping"
          else
            cargo publish -p harmont-cli --token ${{ secrets.CRATES_IO_TOKEN }} --allow-dirty
          fi
```

The `--allow-dirty` flag is already there (it was added at the start of the release work because the version-sed dirties Cargo.toml). No edit needed for this step — just confirm.

If the flag is missing for any reason, add it. Don't remove it from the other publish steps either; the version-sed dirties all of them.

- [ ] **Step 4: Validate yaml**

```bash
cd /home/marko/harmont-cli
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/release.yml'))" && echo yaml-ok
```

Expected: `yaml-ok`.

- [ ] **Step 5: Commit**

```bash
cd /home/marko/harmont-cli
git add .github/workflows/release.yml
git commit -m "$(cat <<'EOF'
ci(release): pre-build embedded WASM plugins before publish

Stages crates/hm/embedded/<name>.wasm for each of the four embedded
plugins (docker, output-human, output-json, cloud) before
`cargo publish -p harmont-cli` runs. The verify-build inside
cargo's publish sandbox now finds the pre-built wasms via the
build.rs bundled-path branch, and the tarball's include = [...]
carries them downstream so `cargo install harmont-cli` works
without needing the sibling plugin crates' source.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 5: Push and instruct the user to retag

**Files:** none.

- [ ] **Step 1: Confirm the staged commits**

```bash
cd /home/marko/harmont-cli
git log --oneline origin/main..HEAD
```

Expected: four commits — build.rs branch, Cargo.toml include, .gitignore, release.yml wasm-stage.

- [ ] **Step 2: Push**

```bash
cd /home/marko/harmont-cli
git fetch origin
git pull --rebase origin main
git push origin main
```

If the rebase surfaces a conflict, abort and report.

- [ ] **Step 3: Hand off to the user**

Report back:
- Four commit SHAs that landed on origin/main.
- That v0.0.1 is still free on crates.io for all three publishable crates (already verified: `hm-plugin-protocol`, `hm-plugin-sdk`, `harmont-cli` all return NOT-FOUND).
- The retag command sequence the user should run themselves:

  ```sh
  git push --delete origin v0.0.1
  git tag -d v0.0.1
  git tag v0.0.1            # lands on HEAD = the four new commits
  git push origin v0.0.1
  ```

  Or, if the user prefers a clean release-tag history with no force-delete:

  ```sh
  git tag v0.0.2
  git push origin v0.0.2
  ```

- That the user should watch `gh run watch <id> --repo harmont-dev/harmont-cli --exit-status` and report back if any new failure mode surfaces.

---

## Out of scope

- **Pre-built binary releases.** This plan makes `cargo install harmont-cli` work; it does NOT add a `cargo-dist`-style GH-Releases binary distribution. That's a follow-up if the user wants a faster install path than "build from source on every install."
- **Publishing the plugin crates.** `hm-plugin-docker`, `hm-plugin-output-human`, `hm-plugin-output-json`, `hm-plugin-cloud`, and `hm-fixtures` keep `publish = false`. They ship embedded in `hm` (now even more literally — as bytes in the tarball).
- **Reproducible builds.** The embedded wasms are built fresh on every release run. Two releases of the same version *should* produce byte-identical wasms (deterministic rustc + identical sources), but we're not auditing that. Out of scope.
- **Cross-platform wasm sanity.** The wasms are `wasm32-wasip1`. They run on any host where `hm` runs (the extism runtime is in-process; no OS-level wasm dependency). No matrix needed.
- **Replacing `--allow-dirty`.** The release workflow uses `--allow-dirty` because the version-sed dirties `Cargo.toml`. Adding `embedded/*.wasm` doesn't change the calculus. A cleaner long-term shape would be `cargo set-version` + commit + publish without `--allow-dirty`, but that's a release-engineering refactor outside this plan.

---

## Self-review

- **Spec coverage:** build.rs branch ✓ (Task 1); include = [...] ✓ (Task 2); gitignore ✓ (Task 3); release.yml wasm-staging ✓ (Task 4); push + handoff ✓ (Task 5). The verify-build inside cargo's publish sandbox is exercised in Task 2 Step 4 (locally) and by the release workflow itself (Task 4).
- **Placeholder scan:** no "TBD", "implement later", "as needed". Every step has exact paths and exact commands.
- **Type consistency:** crate names (`hm-plugin-docker`, etc.), file names (`hm_plugin_docker.wasm`, etc.), and the `embedded/` directory path are used identically across all tasks. The hyphen-vs-underscore split is preserved (crate names use hyphens; output wasm filenames use underscores — the existing build.rs already does the `replace('-', "_")` conversion).
- **Backwards compat:** the dev workflow (`cargo build` from the workspace root with no `embedded/` directory present) continues to work via the fall-through branch in build.rs. Task 1 Step 3 verifies this explicitly.
