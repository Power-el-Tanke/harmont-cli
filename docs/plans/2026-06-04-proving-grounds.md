# Proving Grounds Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a test suite of 100 real-world OSS repos whose CI pipelines are rewritten in harmont's DSL, proving harmont can express and execute real-world build pipelines.

**Architecture:** Two-tier testing. *Compile tier* (fast, no Docker): render each pipeline to IR via `hm render`, validate structure via insta snapshots + structural assertions. *Execution tier* (slow, Docker): clone repo at pinned SHA, run `hm run`, verify exit 0. All fixtures live in `proving-grounds/` at repo root — pipeline.py rewrites checked in, original GHA YAML stored for reference, manifest.toml tracks metadata. Initial batch of 15 repos hand-written; remaining 85 added incrementally.

**Tech Stack:** Python DSL (harmont package), Rust integration tests (insta), shell scripts (fetch/execute), TOML manifests.

---

## Directory Layout

```
proving-grounds/
  manifest.toml                        # All 100 repos: slug, url, sha, lang, complexity, status
  scripts/
    fetch-workflows.py                 # Download GHA YAML from GitHub API
    compile-all.sh                     # hm render every fixture, report pass/fail
    execute.sh                         # Clone + hm run for a single repo
    execute-all.sh                     # Batch execution (nightly)
  repos/
    <org>--<repo>/
      metadata.toml                    # Per-repo: url, sha, language, complexity, notes
      workflows/                       # Original GHA YAML (reference only)
        ci.yml
      .harmont/
        pipeline.py                    # Harmont DSL rewrite
  tests/
    compile_fixtures.rs                # Rust integration test: render + validate IR
    snapshots/                         # insta snapshot files
```

---

### Task 1: Create directory structure and manifest format

**Files:**
- Create: `proving-grounds/manifest.toml`
- Create: `proving-grounds/repos/.gitkeep` (placeholder)

**Step 1: Create the directory tree**

Run:
```bash
mkdir -p proving-grounds/{scripts,repos,tests/snapshots}
```

**Step 2: Write the manifest with all 100 repos**

Create `proving-grounds/manifest.toml`:

```toml
# Proving Grounds Manifest
# status: "pending" | "compile-only" | "passing" | "failing" | "skip"
# complexity: "simple" | "medium" | "complex" | "extreme"

# ── SIMPLE (30) ─────────────────────────────────────────────────────
[[repos]]
slug = "BurntSushi--ripgrep"
url = "https://github.com/BurntSushi/ripgrep"
sha = ""  # pin after first fetch
language = "rust"
complexity = "simple"
status = "pending"
ci_features = ["lint", "test", "clippy"]

[[repos]]
slug = "sharkdp--bat"
url = "https://github.com/sharkdp/bat"
sha = ""
language = "rust"
complexity = "simple"
status = "pending"
ci_features = ["lint", "test", "clippy"]

[[repos]]
slug = "sharkdp--fd"
url = "https://github.com/sharkdp/fd"
sha = ""
language = "rust"
complexity = "simple"
status = "pending"
ci_features = ["lint", "test"]

[[repos]]
slug = "ajeetdsouza--zoxide"
url = "https://github.com/ajeetdsouza/zoxide"
sha = ""
language = "rust"
complexity = "simple"
status = "pending"
ci_features = ["lint", "test"]

[[repos]]
slug = "clap-rs--clap"
url = "https://github.com/clap-rs/clap"
sha = ""
language = "rust"
complexity = "simple"
status = "pending"
ci_features = ["lint", "test", "clippy"]

[[repos]]
slug = "serde-rs--serde"
url = "https://github.com/serde-rs/serde"
sha = ""
language = "rust"
complexity = "simple"
status = "pending"
ci_features = ["lint", "test", "msrv"]

[[repos]]
slug = "pallets--flask"
url = "https://github.com/pallets/flask"
sha = ""
language = "python"
complexity = "simple"
status = "pending"
ci_features = ["lint", "test"]

[[repos]]
slug = "psf--requests"
url = "https://github.com/psf/requests"
sha = ""
language = "python"
complexity = "simple"
status = "pending"
ci_features = ["lint", "test"]

[[repos]]
slug = "psf--black"
url = "https://github.com/psf/black"
sha = ""
language = "python"
complexity = "simple"
status = "pending"
ci_features = ["lint", "test", "fuzz"]

[[repos]]
slug = "python-poetry--poetry"
url = "https://github.com/python-poetry/poetry"
sha = ""
language = "python"
complexity = "simple"
status = "pending"
ci_features = ["lint", "test"]

[[repos]]
slug = "fastapi--fastapi"
url = "https://github.com/fastapi/fastapi"
sha = ""
language = "python"
complexity = "simple"
status = "pending"
ci_features = ["lint", "test", "typecheck"]

[[repos]]
slug = "tiangolo--sqlmodel"
url = "https://github.com/tiangolo/sqlmodel"
sha = ""
language = "python"
complexity = "simple"
status = "pending"
ci_features = ["lint", "test"]

[[repos]]
slug = "expressjs--express"
url = "https://github.com/expressjs/express"
sha = ""
language = "javascript"
complexity = "simple"
status = "pending"
ci_features = ["lint", "test"]

[[repos]]
slug = "sindresorhus--got"
url = "https://github.com/sindresorhus/got"
sha = ""
language = "typescript"
complexity = "simple"
status = "pending"
ci_features = ["lint", "test"]

[[repos]]
slug = "chalk--chalk"
url = "https://github.com/chalk/chalk"
sha = ""
language = "javascript"
complexity = "simple"
status = "pending"
ci_features = ["lint", "test"]

[[repos]]
slug = "prettier--prettier"
url = "https://github.com/prettier/prettier"
sha = ""
language = "javascript"
complexity = "simple"
status = "pending"
ci_features = ["lint", "test"]

[[repos]]
slug = "gin-gonic--gin"
url = "https://github.com/gin-gonic/gin"
sha = ""
language = "go"
complexity = "simple"
status = "pending"
ci_features = ["lint", "test"]

[[repos]]
slug = "go-chi--chi"
url = "https://github.com/go-chi/chi"
sha = ""
language = "go"
complexity = "simple"
status = "pending"
ci_features = ["lint", "test"]

[[repos]]
slug = "stretchr--testify"
url = "https://github.com/stretchr/testify"
sha = ""
language = "go"
complexity = "simple"
status = "pending"
ci_features = ["lint", "test"]

[[repos]]
slug = "junegunn--fzf"
url = "https://github.com/junegunn/fzf"
sha = ""
language = "go"
complexity = "simple"
status = "pending"
ci_features = ["lint", "test"]

[[repos]]
slug = "jqlang--jq"
url = "https://github.com/jqlang/jq"
sha = ""
language = "c"
complexity = "simple"
status = "pending"
ci_features = ["build", "test"]

[[repos]]
slug = "jekyll--jekyll"
url = "https://github.com/jekyll/jekyll"
sha = ""
language = "ruby"
complexity = "simple"
status = "pending"
ci_features = ["lint", "test"]

[[repos]]
slug = "rails--rails"
url = "https://github.com/rails/rails"
sha = ""
language = "ruby"
complexity = "simple"
status = "pending"
ci_features = ["test"]

[[repos]]
slug = "google--gson"
url = "https://github.com/google/gson"
sha = ""
language = "java"
complexity = "simple"
status = "pending"
ci_features = ["test"]

[[repos]]
slug = "square--okhttp"
url = "https://github.com/square/okhttp"
sha = ""
language = "kotlin"
complexity = "simple"
status = "pending"
ci_features = ["lint", "test"]

[[repos]]
slug = "google--guava"
url = "https://github.com/google/guava"
sha = ""
language = "java"
complexity = "simple"
status = "pending"
ci_features = ["test"]

[[repos]]
slug = "lodash--lodash"
url = "https://github.com/lodash/lodash"
sha = ""
language = "javascript"
complexity = "simple"
status = "pending"
ci_features = ["lint", "test"]

[[repos]]
slug = "redis--redis"
url = "https://github.com/redis/redis"
sha = ""
language = "c"
complexity = "simple"
status = "pending"
ci_features = ["build", "test"]

[[repos]]
slug = "JetBrains--kotlin"
url = "https://github.com/JetBrains/kotlin"
sha = ""
language = "kotlin"
complexity = "simple"
status = "pending"
ci_features = ["build", "test"]

[[repos]]
slug = "Alamofire--Alamofire"
url = "https://github.com/Alamofire/Alamofire"
sha = ""
language = "swift"
complexity = "simple"
status = "pending"
ci_features = ["build", "test"]

# ── MEDIUM (30) ─────────────────────────────────────────────────────
[[repos]]
slug = "tokio-rs--tokio"
url = "https://github.com/tokio-rs/tokio"
sha = ""
language = "rust"
complexity = "medium"
status = "pending"
ci_features = ["matrix", "caching", "multi-crate"]

[[repos]]
slug = "starship--starship"
url = "https://github.com/starship/starship"
sha = ""
language = "rust"
complexity = "medium"
status = "pending"
ci_features = ["matrix-os", "caching", "release"]

[[repos]]
slug = "nushell--nushell"
url = "https://github.com/nushell/nushell"
sha = ""
language = "rust"
complexity = "medium"
status = "pending"
ci_features = ["matrix-os", "caching", "multi-crate"]

[[repos]]
slug = "helix-editor--helix"
url = "https://github.com/helix-editor/helix"
sha = ""
language = "rust"
complexity = "medium"
status = "pending"
ci_features = ["matrix-os", "clippy", "caching"]

[[repos]]
slug = "astral-sh--ruff"
url = "https://github.com/astral-sh/ruff"
sha = ""
language = "rust"
complexity = "medium"
status = "pending"
ci_features = ["matrix-os", "caching", "benchmark"]

[[repos]]
slug = "django--django"
url = "https://github.com/django/django"
sha = ""
language = "python"
complexity = "medium"
status = "pending"
ci_features = ["matrix", "caching", "db-backends"]

[[repos]]
slug = "scikit-learn--scikit-learn"
url = "https://github.com/scikit-learn/scikit-learn"
sha = ""
language = "python"
complexity = "medium"
status = "pending"
ci_features = ["matrix", "caching", "doc-build"]

[[repos]]
slug = "pandas-dev--pandas"
url = "https://github.com/pandas-dev/pandas"
sha = ""
language = "python"
complexity = "medium"
status = "pending"
ci_features = ["matrix", "caching", "benchmark"]

[[repos]]
slug = "vercel--next.js"
url = "https://github.com/vercel/next.js"
sha = ""
language = "typescript"
complexity = "medium"
status = "pending"
ci_features = ["matrix", "caching", "turbopack"]

[[repos]]
slug = "sveltejs--svelte"
url = "https://github.com/sveltejs/svelte"
sha = ""
language = "typescript"
complexity = "medium"
status = "pending"
ci_features = ["matrix", "caching", "multi-package"]

[[repos]]
slug = "vitejs--vite"
url = "https://github.com/vitejs/vite"
sha = ""
language = "typescript"
complexity = "medium"
status = "pending"
ci_features = ["matrix", "caching", "ecosystem-ci"]

[[repos]]
slug = "withastro--astro"
url = "https://github.com/withastro/astro"
sha = ""
language = "typescript"
complexity = "medium"
status = "pending"
ci_features = ["matrix", "caching", "multi-package"]

[[repos]]
slug = "pnpm--pnpm"
url = "https://github.com/pnpm/pnpm"
sha = ""
language = "typescript"
complexity = "medium"
status = "pending"
ci_features = ["matrix-os", "caching"]

[[repos]]
slug = "denoland--deno"
url = "https://github.com/denoland/deno"
sha = ""
language = "rust"
complexity = "medium"
status = "pending"
ci_features = ["matrix-os", "caching", "multi-suite"]

[[repos]]
slug = "prometheus--prometheus"
url = "https://github.com/prometheus/prometheus"
sha = ""
language = "go"
complexity = "medium"
status = "pending"
ci_features = ["matrix", "caching", "docker-build"]

[[repos]]
slug = "hashicorp--terraform"
url = "https://github.com/hashicorp/terraform"
sha = ""
language = "go"
complexity = "medium"
status = "pending"
ci_features = ["matrix-os", "caching", "acceptance"]

[[repos]]
slug = "hashicorp--vault"
url = "https://github.com/hashicorp/vault"
sha = ""
language = "go"
complexity = "medium"
status = "pending"
ci_features = ["matrix", "caching", "integration"]

[[repos]]
slug = "etcd-io--etcd"
url = "https://github.com/etcd-io/etcd"
sha = ""
language = "go"
complexity = "medium"
status = "pending"
ci_features = ["matrix", "caching", "integration"]

[[repos]]
slug = "golang--go"
url = "https://github.com/golang/go"
sha = ""
language = "go"
complexity = "medium"
status = "pending"
ci_features = ["multi-builder", "caching"]

[[repos]]
slug = "curl--curl"
url = "https://github.com/curl/curl"
sha = ""
language = "c"
complexity = "medium"
status = "pending"
ci_features = ["matrix-os-compiler", "caching"]

[[repos]]
slug = "facebook--zstd"
url = "https://github.com/facebook/zstd"
sha = ""
language = "c"
complexity = "medium"
status = "pending"
ci_features = ["matrix-os-compiler", "caching"]

[[repos]]
slug = "spring-projects--spring-boot"
url = "https://github.com/spring-projects/spring-boot"
sha = ""
language = "java"
complexity = "medium"
status = "pending"
ci_features = ["matrix-jdk", "caching", "integration"]

[[repos]]
slug = "elastic--elasticsearch"
url = "https://github.com/elastic/elasticsearch"
sha = ""
language = "java"
complexity = "medium"
status = "pending"
ci_features = ["matrix", "caching", "integration"]

[[repos]]
slug = "apache--kafka"
url = "https://github.com/apache/kafka"
sha = ""
language = "java"
complexity = "medium"
status = "pending"
ci_features = ["matrix", "caching", "system-tests"]

[[repos]]
slug = "vapor--vapor"
url = "https://github.com/vapor/vapor"
sha = ""
language = "swift"
complexity = "medium"
status = "pending"
ci_features = ["matrix-swift", "linux-macos"]

[[repos]]
slug = "storybookjs--storybook"
url = "https://github.com/storybookjs/storybook"
sha = ""
language = "typescript"
complexity = "medium"
status = "pending"
ci_features = ["matrix", "caching", "multi-framework"]

[[repos]]
slug = "angular--angular"
url = "https://github.com/angular/angular"
sha = ""
language = "typescript"
complexity = "medium"
status = "pending"
ci_features = ["matrix", "caching", "bazel"]

[[repos]]
slug = "pytorch--pytorch"
url = "https://github.com/pytorch/pytorch"
sha = ""
language = "python"
complexity = "medium"
status = "pending"
ci_features = ["matrix", "caching", "cuda", "artifacts"]

[[repos]]
slug = "goreleaser--goreleaser"
url = "https://github.com/goreleaser/goreleaser"
sha = ""
language = "go"
complexity = "medium"
status = "pending"
ci_features = ["matrix", "docker-multi-arch", "release"]

[[repos]]
slug = "docker--compose"
url = "https://github.com/docker/compose"
sha = ""
language = "go"
complexity = "medium"
status = "pending"
ci_features = ["multi-platform", "docker-in-docker", "release"]

# ── COMPLEX (25) ────────────────────────────────────────────────────
[[repos]]
slug = "rust-lang--rust"
url = "https://github.com/rust-lang/rust"
sha = ""
language = "rust"
complexity = "complex"
status = "pending"
ci_features = ["massive-matrix", "cross-compilation", "artifacts", "bors"]

[[repos]]
slug = "alacritty--alacritty"
url = "https://github.com/alacritty/alacritty"
sha = ""
language = "rust"
complexity = "complex"
status = "pending"
ci_features = ["cross-platform", "release-artifacts", "caching"]

[[repos]]
slug = "tauri-apps--tauri"
url = "https://github.com/tauri-apps/tauri"
sha = ""
language = "rust"
complexity = "complex"
status = "pending"
ci_features = ["multi-platform", "release-automation", "docker", "cross-compile"]

[[repos]]
slug = "zed-industries--zed"
url = "https://github.com/zed-industries/zed"
sha = ""
language = "rust"
complexity = "complex"
status = "pending"
ci_features = ["multi-platform", "release", "caching", "signing"]

[[repos]]
slug = "oven-sh--bun"
url = "https://github.com/oven-sh/bun"
sha = ""
language = "zig"
complexity = "complex"
status = "pending"
ci_features = ["multi-platform", "cross-compilation", "docker", "release"]

[[repos]]
slug = "huggingface--transformers"
url = "https://github.com/huggingface/transformers"
sha = ""
language = "python"
complexity = "complex"
status = "pending"
ci_features = ["matrix", "gpu-runners", "doc-deploy", "model-tests"]

[[repos]]
slug = "tensorflow--tensorflow"
url = "https://github.com/tensorflow/tensorflow"
sha = ""
language = "cpp"
complexity = "complex"
status = "pending"
ci_features = ["multi-platform", "gpu", "docker", "release"]

[[repos]]
slug = "python--cpython"
url = "https://github.com/python/cpython"
sha = ""
language = "c"
complexity = "complex"
status = "pending"
ci_features = ["matrix-os-compiler", "cross-compilation", "release", "extensive-tests"]

[[repos]]
slug = "facebook--react"
url = "https://github.com/facebook/react"
sha = ""
language = "javascript"
complexity = "complex"
status = "pending"
ci_features = ["multi-package", "artifacts", "deploy", "canary"]

[[repos]]
slug = "microsoft--TypeScript"
url = "https://github.com/microsoft/TypeScript"
sha = ""
language = "typescript"
complexity = "complex"
status = "pending"
ci_features = ["matrix", "release", "benchmarks", "artifacts"]

[[repos]]
slug = "eslint--eslint"
url = "https://github.com/eslint/eslint"
sha = ""
language = "javascript"
complexity = "complex"
status = "pending"
ci_features = ["matrix", "caching", "semantic-release"]

[[repos]]
slug = "prisma--prisma"
url = "https://github.com/prisma/prisma"
sha = ""
language = "typescript"
complexity = "complex"
status = "pending"
ci_features = ["matrix-db", "docker-test-dbs", "release"]

[[repos]]
slug = "containerd--containerd"
url = "https://github.com/containerd/containerd"
sha = ""
language = "go"
complexity = "complex"
status = "pending"
ci_features = ["multi-platform", "cross-compilation", "integration", "release"]

[[repos]]
slug = "grpc--grpc"
url = "https://github.com/grpc/grpc"
sha = ""
language = "cpp"
complexity = "complex"
status = "pending"
ci_features = ["matrix-languages", "multi-platform", "docker"]

[[repos]]
slug = "llvm--llvm-project"
url = "https://github.com/llvm/llvm-project"
sha = ""
language = "cpp"
complexity = "complex"
status = "pending"
ci_features = ["multi-platform", "massive-matrix", "sanitizers", "release"]

[[repos]]
slug = "FFmpeg--FFmpeg"
url = "https://github.com/FFmpeg/FFmpeg"
sha = ""
language = "c"
complexity = "complex"
status = "pending"
ci_features = ["cross-compilation", "multi-platform", "configure-matrix"]

[[repos]]
slug = "flutter--flutter"
url = "https://github.com/flutter/flutter"
sha = ""
language = "dart"
complexity = "complex"
status = "pending"
ci_features = ["multi-platform", "device-testing", "release-channels"]

[[repos]]
slug = "expo--expo"
url = "https://github.com/expo/expo"
sha = ""
language = "typescript"
complexity = "complex"
status = "pending"
ci_features = ["multi-platform-mobile", "release", "monorepo", "caching"]

[[repos]]
slug = "grafana--grafana"
url = "https://github.com/grafana/grafana"
sha = ""
language = "go"
complexity = "complex"
status = "pending"
ci_features = ["multi-platform", "docker", "release", "frontend-backend"]

[[repos]]
slug = "cockroachdb--cockroach"
url = "https://github.com/cockroachdb/cockroach"
sha = ""
language = "go"
complexity = "complex"
status = "pending"
ci_features = ["multi-platform", "docker", "release", "integration"]

[[repos]]
slug = "tikv--tikv"
url = "https://github.com/tikv/tikv"
sha = ""
language = "rust"
complexity = "complex"
status = "pending"
ci_features = ["matrix", "cross-compilation", "integration", "release"]

[[repos]]
slug = "apache--spark"
url = "https://github.com/apache/spark"
sha = ""
language = "scala"
complexity = "complex"
status = "pending"
ci_features = ["matrix-jdk-scala-hadoop", "docker", "extensive-tests"]

[[repos]]
slug = "SignalApp--Signal-Android"
url = "https://github.com/SignalApp/Signal-Android"
sha = ""
language = "kotlin"
complexity = "complex"
status = "pending"
ci_features = ["android-build", "signing", "artifacts", "release"]

[[repos]]
slug = "neovim--neovim"
url = "https://github.com/neovim/neovim"
sha = ""
language = "c"
complexity = "complex"
status = "pending"
ci_features = ["multi-platform", "caching", "release", "functional-tests"]

[[repos]]
slug = "supabase--supabase"
url = "https://github.com/supabase/supabase"
sha = ""
language = "typescript"
complexity = "complex"
status = "pending"
ci_features = ["monorepo", "docker", "multi-service", "release"]

# ── EXTREME (15) ────────────────────────────────────────────────────
[[repos]]
slug = "kubernetes--kubernetes"
url = "https://github.com/kubernetes/kubernetes"
sha = ""
language = "go"
complexity = "extreme"
status = "pending"
ci_features = ["prow", "massive-matrix", "self-hosted", "conditional", "conformance"]

[[repos]]
slug = "NixOS--nixpkgs"
url = "https://github.com/NixOS/nixpkgs"
sha = ""
language = "nix"
complexity = "extreme"
status = "pending"
ci_features = ["hydra", "80k-packages", "self-hosted", "conditional"]

[[repos]]
slug = "Homebrew--homebrew-core"
url = "https://github.com/Homebrew/homebrew-core"
sha = ""
language = "ruby"
complexity = "extreme"
status = "pending"
ci_features = ["massive-formula-matrix", "bottle-builds", "self-hosted"]

[[repos]]
slug = "vercel--turborepo"
url = "https://github.com/vercel/turborepo"
sha = ""
language = "rust"
complexity = "extreme"
status = "pending"
ci_features = ["monorepo", "reusable-workflows", "remote-caching"]

[[repos]]
slug = "nrwl--nx"
url = "https://github.com/nrwl/nx"
sha = ""
language = "typescript"
complexity = "extreme"
status = "pending"
ci_features = ["monorepo", "affected-ci", "distributed-execution"]

[[repos]]
slug = "chromium--chromium"
url = "https://github.com/nicedoc/nicedoc.io"
sha = ""
language = "cpp"
complexity = "extreme"
status = "pending"
ci_features = ["luci", "massive-matrix", "self-hosted", "sanitizers"]
notes = "chromium uses LUCI, not GHA - reference only"

[[repos]]
slug = "microsoft--vscode"
url = "https://github.com/microsoft/vscode"
sha = ""
language = "typescript"
complexity = "extreme"
status = "pending"
ci_features = ["multi-platform", "electron", "release", "self-hosted", "insider-builds"]

[[repos]]
slug = "facebook--react-native"
url = "https://github.com/facebook/react-native"
sha = ""
language = "javascript"
complexity = "extreme"
status = "pending"
ci_features = ["multi-platform", "self-hosted", "conditional", "hermes"]

[[repos]]
slug = "rust-lang--cargo"
url = "https://github.com/rust-lang/cargo"
sha = ""
language = "rust"
complexity = "extreme"
status = "pending"
ci_features = ["reusable-workflows", "matrix", "cross-platform", "bors"]

[[repos]]
slug = "actions--runner"
url = "https://github.com/actions/runner"
sha = ""
language = "csharp"
complexity = "extreme"
status = "pending"
ci_features = ["self-referential", "multi-platform", "release", "reusable-workflows"]

[[repos]]
slug = "github--codeql"
url = "https://github.com/github/codeql"
sha = ""
language = "codeql"
complexity = "extreme"
status = "pending"
ci_features = ["reusable-workflows", "multi-language-matrix", "composite-actions"]

[[repos]]
slug = "nixos--nix"
url = "https://github.com/NixOS/nix"
sha = ""
language = "cpp"
complexity = "extreme"
status = "pending"
ci_features = ["multi-platform", "self-hosted", "flake-ci", "docker", "release"]

[[repos]]
slug = "envoyproxy--envoy"
url = "https://github.com/envoyproxy/envoy"
sha = ""
language = "cpp"
complexity = "extreme"
status = "pending"
ci_features = ["multi-platform", "bazel", "docker", "self-hosted", "sanitizers"]

[[repos]]
slug = "pulumi--pulumi"
url = "https://github.com/pulumi/pulumi"
sha = ""
language = "go"
complexity = "extreme"
status = "pending"
ci_features = ["monorepo", "multi-language-sdk", "integration", "release", "reusable-workflows"]

[[repos]]
slug = "hashicorp--nomad"
url = "https://github.com/hashicorp/nomad"
sha = ""
language = "go"
complexity = "extreme"
status = "pending"
ci_features = ["multi-platform", "integration", "self-hosted", "conditional", "release"]
```

**Step 3: Commit**

```bash
git add proving-grounds/
git commit -m "feat: proving-grounds scaffold with 100-repo manifest"
```

---

### Task 2: Write the workflow fetch script

**Files:**
- Create: `proving-grounds/scripts/fetch-workflows.py`

**Step 1: Write the fetch script**

```python
#!/usr/bin/env python3
"""Fetch .github/workflows/ from repos listed in manifest.toml.

Usage:
    python fetch-workflows.py [--token GITHUB_TOKEN] [--slug SLUG]

Without --slug, fetches all repos with status != "skip".
With --slug, fetches only that repo.
Pins SHA to HEAD of default branch if sha is empty.
"""
import argparse
import json
import os
import sys
import urllib.request
from pathlib import Path

MANIFEST = Path(__file__).resolve().parent.parent / "manifest.toml"
REPOS_DIR = Path(__file__).resolve().parent.parent / "repos"


def load_manifest():
    # Minimal TOML parser for our simple format — avoids external dep
    import tomllib
    return tomllib.loads(MANIFEST.read_text())


def gh_api(path: str, token: str | None) -> dict:
    url = f"https://api.github.com{path}"
    req = urllib.request.Request(url)
    req.add_header("Accept", "application/vnd.github.v3+json")
    if token:
        req.add_header("Authorization", f"token {token}")
    with urllib.request.urlopen(req) as resp:
        return json.loads(resp.read())


def gh_raw(owner: str, repo: str, sha: str, path: str, token: str | None) -> bytes:
    url = f"https://raw.githubusercontent.com/{owner}/{repo}/{sha}/{path}"
    req = urllib.request.Request(url)
    if token:
        req.add_header("Authorization", f"token {token}")
    with urllib.request.urlopen(req) as resp:
        return resp.read()


def fetch_repo(entry: dict, token: str | None):
    slug = entry["slug"]
    url = entry["url"]
    parts = url.rstrip("/").split("/")
    owner, repo = parts[-2], parts[-1]

    sha = entry.get("sha", "")
    if not sha:
        info = gh_api(f"/repos/{owner}/{repo}", token)
        sha = gh_api(
            f"/repos/{owner}/{repo}/commits/{info['default_branch']}", token
        )["sha"]
        print(f"  pinned {slug} -> {sha[:12]}")

    repo_dir = REPOS_DIR / slug
    wf_dir = repo_dir / "workflows"
    wf_dir.mkdir(parents=True, exist_ok=True)

    # Write metadata
    meta = repo_dir / "metadata.toml"
    meta.write_text(
        f'url = "{url}"\n'
        f'sha = "{sha}"\n'
        f'language = "{entry["language"]}"\n'
        f'complexity = "{entry["complexity"]}"\n'
    )

    # Fetch workflow files
    try:
        contents = gh_api(
            f"/repos/{owner}/{repo}/contents/.github/workflows?ref={sha}", token
        )
    except Exception as e:
        print(f"  WARN: no workflows for {slug}: {e}")
        return sha

    for item in contents:
        if item["name"].endswith((".yml", ".yaml")):
            data = gh_raw(owner, repo, sha, item["path"], token)
            (wf_dir / item["name"]).write_bytes(data)
            print(f"  {slug}/workflows/{item['name']}")

    return sha


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--token", default=os.environ.get("GITHUB_TOKEN"))
    parser.add_argument("--slug", help="Fetch only this repo slug")
    args = parser.parse_args()

    manifest = load_manifest()
    repos = manifest.get("repos", [])

    if args.slug:
        repos = [r for r in repos if r["slug"] == args.slug]
        if not repos:
            print(f"Unknown slug: {args.slug}", file=sys.stderr)
            sys.exit(1)

    for entry in repos:
        if entry.get("status") == "skip":
            continue
        print(f"Fetching {entry['slug']}...")
        try:
            fetch_repo(entry, args.token)
        except Exception as e:
            print(f"  ERROR: {e}")


if __name__ == "__main__":
    main()
```

**Step 2: Make executable**

Run: `chmod +x proving-grounds/scripts/fetch-workflows.py`

**Step 3: Commit**

```bash
git add proving-grounds/scripts/fetch-workflows.py
git commit -m "feat: workflow fetch script for proving-grounds"
```

---

### Task 3: Write compile-tier test script

**Files:**
- Create: `proving-grounds/scripts/compile-all.sh`

**Step 1: Write the compile test script**

This script iterates over all repos that have a `.harmont/pipeline.py`, runs `hm render`, and reports pass/fail.

```bash
#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPOS_DIR="$SCRIPT_DIR/../repos"
HM="${HM_BIN:-hm}"

pass=0
fail=0
skip=0
failures=()

for repo_dir in "$REPOS_DIR"/*/; do
    slug="$(basename "$repo_dir")"
    pipeline="$repo_dir/.harmont/pipeline.py"

    if [[ ! -f "$pipeline" ]]; then
        skip=$((skip + 1))
        continue
    fi

    if $HM render ci --dir "$repo_dir" > /dev/null 2>&1; then
        echo "PASS  $slug"
        pass=$((pass + 1))
    else
        echo "FAIL  $slug"
        fail=$((fail + 1))
        failures+=("$slug")
    fi
done

echo ""
echo "── Results ──"
echo "pass: $pass  fail: $fail  skip: $skip"

if [[ ${#failures[@]} -gt 0 ]]; then
    echo ""
    echo "Failures:"
    for f in "${failures[@]}"; do
        echo "  - $f"
    done
    exit 1
fi
```

**Step 2: Make executable**

Run: `chmod +x proving-grounds/scripts/compile-all.sh`

**Step 3: Commit**

```bash
git add proving-grounds/scripts/compile-all.sh
git commit -m "feat: compile-tier test script for proving-grounds"
```

---

### Task 4: Write execution-tier test scripts

**Files:**
- Create: `proving-grounds/scripts/execute.sh`
- Create: `proving-grounds/scripts/execute-all.sh`

**Step 1: Write the single-repo execution script**

```bash
#!/usr/bin/env bash
set -euo pipefail

# Usage: execute.sh <slug> [--keep]
# Clones repo at pinned SHA, copies .harmont/, runs hm run ci.

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPOS_DIR="$SCRIPT_DIR/../repos"
HM="${HM_BIN:-hm}"
CACHE_DIR="${PROVING_GROUNDS_CACHE:-/tmp/proving-grounds}"

slug="$1"
keep="${2:-}"
repo_dir="$REPOS_DIR/$slug"
meta="$repo_dir/metadata.toml"

if [[ ! -f "$meta" ]]; then
    echo "ERROR: no metadata.toml for $slug" >&2
    exit 1
fi

url=$(grep '^url' "$meta" | cut -d'"' -f2)
sha=$(grep '^sha' "$meta" | cut -d'"' -f2)

if [[ -z "$sha" ]]; then
    echo "ERROR: no pinned SHA for $slug" >&2
    exit 1
fi

clone_dir="$CACHE_DIR/$slug"

if [[ -d "$clone_dir/.git" ]]; then
    echo "Using cached clone: $clone_dir"
    git -C "$clone_dir" checkout "$sha" 2>/dev/null || {
        git -C "$clone_dir" fetch origin "$sha"
        git -C "$clone_dir" checkout "$sha"
    }
else
    echo "Cloning $url @ $sha ..."
    git clone --depth 1 "$url" "$clone_dir" 2>/dev/null || {
        git clone "$url" "$clone_dir"
    }
    git -C "$clone_dir" checkout "$sha" 2>/dev/null || true
fi

# Copy harmont pipeline into clone
if [[ -d "$repo_dir/.harmont" ]]; then
    cp -r "$repo_dir/.harmont" "$clone_dir/.harmont"
fi

echo "Running: $HM run ci --dir $clone_dir"
$HM run ci --dir "$clone_dir"
result=$?

if [[ "$keep" != "--keep" ]]; then
    rm -rf "$clone_dir"
fi

exit $result
```

**Step 2: Write the batch execution script**

```bash
#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPOS_DIR="$SCRIPT_DIR/../repos"

pass=0
fail=0
skip=0
failures=()

for repo_dir in "$REPOS_DIR"/*/; do
    slug="$(basename "$repo_dir")"

    if [[ ! -d "$repo_dir/.harmont" ]]; then
        skip=$((skip + 1))
        continue
    fi

    echo "══════════════════════════════════════"
    echo "  $slug"
    echo "══════════════════════════════════════"

    if "$SCRIPT_DIR/execute.sh" "$slug"; then
        pass=$((pass + 1))
    else
        fail=$((fail + 1))
        failures+=("$slug")
    fi
    echo ""
done

echo ""
echo "── Execution Results ──"
echo "pass: $pass  fail: $fail  skip: $skip"

if [[ ${#failures[@]} -gt 0 ]]; then
    echo ""
    echo "Failures:"
    for f in "${failures[@]}"; do
        echo "  - $f"
    done
    exit 1
fi
```

**Step 3: Make executable**

Run:
```bash
chmod +x proving-grounds/scripts/execute.sh proving-grounds/scripts/execute-all.sh
```

**Step 4: Commit**

```bash
git add proving-grounds/scripts/execute.sh proving-grounds/scripts/execute-all.sh
git commit -m "feat: execution-tier test scripts for proving-grounds"
```

---

### Task 5: Write Rust integration test for compile tier

**Files:**
- Create: `proving-grounds/tests/compile_fixtures.rs`
- Modify: `cli/Cargo.toml` (add proving-grounds test target if needed)

**Step 1: Write the Rust compile test**

This test auto-discovers all repos with `.harmont/pipeline.py` and validates their IR.

```rust
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn proving_grounds_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../proving-grounds/repos")
}

fn hm_bin() -> String {
    std::env::var("HM_BIN").unwrap_or_else(|_| "hm".to_string())
}

fn repo_slugs() -> Vec<String> {
    let dir = proving_grounds_dir();
    if !dir.exists() {
        return vec![];
    }
    let mut slugs = vec![];
    for entry in fs::read_dir(&dir).unwrap() {
        let entry = entry.unwrap();
        let harmont = entry.path().join(".harmont/pipeline.py");
        if harmont.exists() {
            slugs.push(entry.file_name().to_string_lossy().to_string());
        }
    }
    slugs.sort();
    slugs
}

#[test]
fn all_proving_ground_fixtures_render() {
    let slugs = repo_slugs();
    if slugs.is_empty() {
        eprintln!("SKIP: no proving-ground fixtures with .harmont/ found");
        return;
    }

    let mut failures = vec![];

    for slug in &slugs {
        let dir = proving_grounds_dir().join(slug);
        let output = Command::new(hm_bin())
            .args(["render", "ci", "--dir"])
            .arg(&dir)
            .output()
            .unwrap_or_else(|e| panic!("spawn hm: {e}"));

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            failures.push(format!("{slug}: {stderr}"));
            continue;
        }

        // Validate JSON parses
        let stdout = String::from_utf8_lossy(&output.stdout);
        let _: serde_json::Value = serde_json::from_str(&stdout)
            .unwrap_or_else(|e| panic!("{slug}: invalid JSON: {e}"));
    }

    if !failures.is_empty() {
        panic!(
            "{} fixture(s) failed to render:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }
}

#[test]
fn all_proving_ground_fixtures_have_valid_structure() {
    let slugs = repo_slugs();
    if slugs.is_empty() {
        return;
    }

    for slug in &slugs {
        let dir = proving_grounds_dir().join(slug);
        let output = Command::new(hm_bin())
            .args(["render", "ci", "--dir"])
            .arg(&dir)
            .output()
            .unwrap();

        if !output.status.success() {
            continue; // caught by render test above
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let ir: serde_json::Value = serde_json::from_str(&stdout).unwrap();

        // Validate v0 IR structure
        assert_eq!(ir["version"], "0", "{slug}: version != 0");

        let nodes = ir["graph"]["nodes"].as_array()
            .unwrap_or_else(|| panic!("{slug}: missing graph.nodes"));
        assert!(!nodes.is_empty(), "{slug}: no nodes");

        for node in nodes {
            let key = node["step"]["key"].as_str()
                .unwrap_or_else(|| panic!("{slug}: node missing step.key"));
            assert!(!key.is_empty(), "{slug}: empty key");

            let cmd = node["step"]["cmd"].as_str()
                .unwrap_or_else(|| panic!("{slug}: node missing step.cmd"));
            assert!(!cmd.is_empty(), "{slug}: empty cmd for key {key}");
        }

        let edges = ir["graph"]["edges"].as_array()
            .unwrap_or_else(|| panic!("{slug}: missing graph.edges"));
        assert!(!edges.is_empty(), "{slug}: no edges");

        // No self-loops
        for edge in edges {
            let src = edge[0].as_u64().unwrap();
            let dst = edge[1].as_u64().unwrap();
            assert_ne!(src, dst, "{slug}: self-loop on node {src}");
        }
    }
}
```

**Step 2: Run test to verify it compiles (will skip if no fixtures yet)**

Run: `cargo test -p hm --test compile_fixtures -- --nocapture`
Expected: SKIP message (no fixtures yet)

**Step 3: Commit**

```bash
git add proving-grounds/tests/compile_fixtures.rs
git commit -m "feat: compile-tier Rust integration test for proving-grounds"
```

---

### Task 6: Write pipeline rewrite — BurntSushi/ripgrep (Rust, simple)

**Files:**
- Create: `proving-grounds/repos/BurntSushi--ripgrep/.harmont/pipeline.py`

**Step 1: Study the original CI**

Run: `python proving-grounds/scripts/fetch-workflows.py --slug BurntSushi--ripgrep`
Then read the downloaded workflow YAML to understand what the CI does.

**Step 2: Write the harmont rewrite**

```python
"""ripgrep — Rust CLI for recursive regex search."""
from __future__ import annotations

import harmont as hm
from harmont.rust import RustToolchain


@hm.target()
def project() -> RustToolchain:
    return hm.rust.toolchain(path=".")


@hm.pipeline(
    "ci",
    env={"CI": "true"},
    default_image="ubuntu:24.04",
    triggers=[hm.push(branch="master")],
)
def ci(project: hm.Target[RustToolchain]) -> tuple[hm.Step, ...]:
    return (
        project.build(),
        project.test(),
        project.clippy(),
        project.fmt(),
    )
```

**Step 3: Verify compile**

Run: `hm render ci --dir proving-grounds/repos/BurntSushi--ripgrep`
Expected: Valid JSON IR output

**Step 4: Commit**

```bash
git add proving-grounds/repos/BurntSushi--ripgrep/
git commit -m "feat(proving-grounds): add ripgrep pipeline rewrite"
```

---

### Task 7: Write pipeline rewrite — pallets/flask (Python, simple)

**Files:**
- Create: `proving-grounds/repos/pallets--flask/.harmont/pipeline.py`

**Step 1: Fetch and study original CI**

Run: `python proving-grounds/scripts/fetch-workflows.py --slug pallets--flask`

**Step 2: Write the harmont rewrite**

```python
"""Flask — lightweight Python web framework."""
from __future__ import annotations

import harmont as hm
from harmont.python import PythonToolchain


@hm.target()
def project() -> PythonToolchain:
    return hm.python(path=".")


@hm.pipeline(
    "ci",
    env={"CI": "true"},
    default_image="ubuntu:24.04",
    triggers=[hm.push(branch="main")],
)
def ci(project: hm.Target[PythonToolchain]) -> tuple[hm.Step, ...]:
    return (
        project.test(),
        project.lint(),
        project.typecheck(),
    )
```

**Step 3: Verify compile**

Run: `hm render ci --dir proving-grounds/repos/pallets--flask`

**Step 4: Commit**

```bash
git add proving-grounds/repos/pallets--flask/
git commit -m "feat(proving-grounds): add flask pipeline rewrite"
```

---

### Task 8: Write pipeline rewrite — expressjs/express (JS, simple)

**Files:**
- Create: `proving-grounds/repos/expressjs--express/.harmont/pipeline.py`

**Step 1: Fetch and study original CI**

Run: `python proving-grounds/scripts/fetch-workflows.py --slug expressjs--express`

**Step 2: Write the harmont rewrite**

```python
"""Express — minimal Node.js web framework."""
from __future__ import annotations

import harmont as hm
from harmont.npm import NpmProject


@hm.target()
def project() -> NpmProject:
    return hm.npm(path=".")


@hm.pipeline(
    "ci",
    env={"CI": "true"},
    default_image="ubuntu:24.04",
    triggers=[hm.push(branch="master")],
)
def ci(project: hm.Target[NpmProject]) -> tuple[hm.Step, ...]:
    return (
        project.test(),
        project.lint(),
    )
```

**Step 3: Verify compile, commit**

---

### Task 9: Write pipeline rewrite — gin-gonic/gin (Go, simple)

**Files:**
- Create: `proving-grounds/repos/gin-gonic--gin/.harmont/pipeline.py`

**Step 2: Write the harmont rewrite**

```python
"""Gin — HTTP web framework for Go."""
from __future__ import annotations

import harmont as hm
from harmont.go import GoToolchain


@hm.target()
def project() -> GoToolchain:
    return hm.go(path=".")


@hm.pipeline(
    "ci",
    env={"CI": "true"},
    default_image="ubuntu:24.04",
    triggers=[hm.push(branch="master")],
)
def ci(project: hm.Target[GoToolchain]) -> tuple[hm.Step, ...]:
    return (
        project.build(),
        project.test(),
        project.vet(),
        project.fmt(),
    )
```

**Step 3: Verify compile, commit**

---

### Task 10: Write pipeline rewrite — clap-rs/clap (Rust, simple)

**Files:**
- Create: `proving-grounds/repos/clap-rs--clap/.harmont/pipeline.py`

**Step 2: Write the harmont rewrite**

```python
"""clap — CLI argument parser for Rust."""
from __future__ import annotations

import harmont as hm
from harmont.rust import RustToolchain


@hm.target()
def project() -> RustToolchain:
    return hm.rust.toolchain(path=".")


@hm.pipeline(
    "ci",
    env={"CI": "true"},
    default_image="ubuntu:24.04",
    triggers=[hm.push(branch="master")],
)
def ci(project: hm.Target[RustToolchain]) -> tuple[hm.Step, ...]:
    return (
        project.build(),
        project.test(),
        project.clippy(),
        project.fmt(),
    )
```

---

### Task 11: Write pipeline rewrite — tokio-rs/tokio (Rust, medium)

**Files:**
- Create: `proving-grounds/repos/tokio-rs--tokio/.harmont/pipeline.py`

Tokio is a Cargo workspace with multiple crates. The pipeline needs to build/test the workspace, plus run clippy and fmt. Harmont's Rust toolchain operates at workspace level.

**Step 2: Write the harmont rewrite**

```python
"""Tokio — async runtime for Rust."""
from __future__ import annotations

import harmont as hm
from harmont.rust import RustToolchain


@hm.target()
def project() -> RustToolchain:
    return hm.rust.toolchain(path=".")


@hm.pipeline(
    "ci",
    env={"CI": "true", "RUSTFLAGS": "-Dwarnings"},
    default_image="ubuntu:24.04",
    triggers=[hm.push(branch="master")],
)
def ci(project: hm.Target[RustToolchain]) -> tuple[hm.Step, ...]:
    return (
        project.build(),
        project.test(),
        project.clippy(),
        project.fmt(),
    )
```

---

### Task 12: Write pipeline rewrite — vitejs/vite (TypeScript, medium)

**Files:**
- Create: `proving-grounds/repos/vitejs--vite/.harmont/pipeline.py`

**Step 2: Write the harmont rewrite**

```python
"""Vite — next-generation frontend build tool."""
from __future__ import annotations

import harmont as hm
from harmont.npm import NpmProject


@hm.target()
def project() -> NpmProject:
    return hm.npm(path=".")


@hm.pipeline(
    "ci",
    env={"CI": "true"},
    default_image="ubuntu:24.04",
    triggers=[hm.push(branch="main")],
)
def ci(project: hm.Target[NpmProject]) -> tuple[hm.Step, ...]:
    return (
        project.run("build"),
        project.run("test"),
        project.run("lint"),
    )
```

---

### Task 13: Write pipeline rewrite — hashicorp/terraform (Go, medium)

**Files:**
- Create: `proving-grounds/repos/hashicorp--terraform/.harmont/pipeline.py`

**Step 2: Write the harmont rewrite**

```python
"""Terraform — infrastructure as code."""
from __future__ import annotations

import harmont as hm
from harmont.go import GoToolchain


@hm.target()
def project() -> GoToolchain:
    return hm.go(path=".")


@hm.pipeline(
    "ci",
    env={"CI": "true"},
    default_image="ubuntu:24.04",
    triggers=[hm.push(branch="main")],
)
def ci(project: hm.Target[GoToolchain]) -> tuple[hm.Step, ...]:
    return (
        project.build(),
        project.test(),
        project.vet(),
        project.fmt(),
    )
```

---

### Task 14: Write pipeline rewrite — django/django (Python, medium)

**Files:**
- Create: `proving-grounds/repos/django--django/.harmont/pipeline.py`

**Step 2: Write the harmont rewrite**

```python
"""Django — Python web framework."""
from __future__ import annotations

import harmont as hm
from harmont.python import PythonToolchain


@hm.target()
def project() -> PythonToolchain:
    return hm.python(path=".")


@hm.pipeline(
    "ci",
    env={"CI": "true"},
    default_image="ubuntu:24.04",
    triggers=[hm.push(branch="main")],
)
def ci(project: hm.Target[PythonToolchain]) -> tuple[hm.Step, ...]:
    return (
        project.test(),
        project.lint(),
        project.fmt(),
    )
```

---

### Task 15: Write pipeline rewrite — astral-sh/ruff (Rust, medium)

**Files:**
- Create: `proving-grounds/repos/astral-sh--ruff/.harmont/pipeline.py`

**Step 2: Write the harmont rewrite**

```python
"""Ruff — fast Python linter/formatter written in Rust."""
from __future__ import annotations

import harmont as hm
from harmont.rust import RustToolchain


@hm.target()
def project() -> RustToolchain:
    return hm.rust.toolchain(path=".")


@hm.pipeline(
    "ci",
    env={"CI": "true"},
    default_image="ubuntu:24.04",
    triggers=[hm.push(branch="main")],
)
def ci(project: hm.Target[RustToolchain]) -> tuple[hm.Step, ...]:
    return (
        project.build(),
        project.test(),
        project.clippy(),
        project.fmt(),
    )
```

---

### Task 16: Write pipeline rewrite — jekyll/jekyll (Ruby, simple)

**Files:**
- Create: `proving-grounds/repos/jekyll--jekyll/.harmont/pipeline.py`

**Step 2: Write the harmont rewrite**

```python
"""Jekyll — static site generator in Ruby."""
from __future__ import annotations

import harmont as hm
from harmont.ruby import RubyToolchain


@hm.target()
def project() -> RubyToolchain:
    return hm.ruby(path=".")


@hm.pipeline(
    "ci",
    env={"CI": "true"},
    default_image="ubuntu:24.04",
    triggers=[hm.push(branch="master")],
)
def ci(project: hm.Target[RubyToolchain]) -> tuple[hm.Step, ...]:
    return (
        project.test(),
        project.lint(),
    )
```

---

### Task 17: Write pipeline rewrite — starship/starship (Rust, medium)

**Files:**
- Create: `proving-grounds/repos/starship--starship/.harmont/pipeline.py`

**Step 2: Write the harmont rewrite**

```python
"""Starship — cross-shell prompt."""
from __future__ import annotations

import harmont as hm
from harmont.rust import RustToolchain


@hm.target()
def project() -> RustToolchain:
    return hm.rust.toolchain(path=".")


@hm.pipeline(
    "ci",
    env={"CI": "true"},
    default_image="ubuntu:24.04",
    triggers=[hm.push(branch="master")],
)
def ci(project: hm.Target[RustToolchain]) -> tuple[hm.Step, ...]:
    return (
        project.build(),
        project.test(),
        project.clippy(),
        project.fmt(),
    )
```

---

### Task 18: Write pipeline rewrite — grafana/grafana (Go+TS, complex monorepo)

**Files:**
- Create: `proving-grounds/repos/grafana--grafana/.harmont/pipeline.py`

Grafana is a Go backend + TypeScript frontend monorepo. This demonstrates harmont's multi-toolchain composition — the same pattern as the `zig-js` example.

**Step 2: Write the harmont rewrite**

```python
"""Grafana — observability platform (Go + TypeScript monorepo)."""
from __future__ import annotations

from typing import Annotated

import harmont as hm
from harmont.go import GoToolchain
from harmont.npm import NpmProject


@hm.target()
def backend() -> GoToolchain:
    return hm.go(path=".")


@hm.target()
def frontend() -> NpmProject:
    return hm.npm(path=".")


@hm.pipeline(
    "ci",
    env={"CI": "true"},
    default_image="ubuntu:24.04",
    triggers=[hm.push(branch="main")],
)
def ci(
    backend: hm.Target[GoToolchain],
    frontend: hm.Target[NpmProject],
) -> tuple[hm.Step, ...]:
    return (
        # Go backend
        backend.build(),
        backend.test(),
        backend.vet(),
        # TypeScript frontend
        frontend.run("build"),
        frontend.run("test"),
        frontend.run("lint"),
    )
```

---

### Task 19: Write pipeline rewrite — fastapi/fastapi (Python, simple)

**Files:**
- Create: `proving-grounds/repos/fastapi--fastapi/.harmont/pipeline.py`

**Step 2: Write the harmont rewrite**

```python
"""FastAPI — modern Python web framework."""
from __future__ import annotations

import harmont as hm
from harmont.python import PythonToolchain


@hm.target()
def project() -> PythonToolchain:
    return hm.python(path=".")


@hm.pipeline(
    "ci",
    env={"CI": "true"},
    default_image="ubuntu:24.04",
    triggers=[hm.push(branch="master")],
)
def ci(project: hm.Target[PythonToolchain]) -> tuple[hm.Step, ...]:
    return (
        project.test(),
        project.lint(),
        project.fmt(),
        project.typecheck(),
    )
```

---

### Task 20: Write pipeline rewrite — sveltejs/svelte (TypeScript, medium)

**Files:**
- Create: `proving-grounds/repos/sveltejs--svelte/.harmont/pipeline.py`

**Step 2: Write the harmont rewrite**

```python
"""Svelte — cybernetically enhanced web apps."""
from __future__ import annotations

import harmont as hm
from harmont.npm import NpmProject


@hm.target()
def project() -> NpmProject:
    return hm.npm(path=".")


@hm.pipeline(
    "ci",
    env={"CI": "true"},
    default_image="ubuntu:24.04",
    triggers=[hm.push(branch="main")],
)
def ci(project: hm.Target[NpmProject]) -> tuple[hm.Step, ...]:
    return (
        project.run("build"),
        project.run("test"),
        project.run("lint"),
    )
```

---

### Task 21: Batch commit all pipeline rewrites

**Step 1: Verify all fixtures compile**

Run: `proving-grounds/scripts/compile-all.sh`
Expected: All 15 PASS

**Step 2: Commit all rewrites**

```bash
git add proving-grounds/repos/
git commit -m "feat(proving-grounds): initial 15 pipeline rewrites

Repos: ripgrep, flask, express, gin, clap, tokio, vite, terraform,
django, ruff, jekyll, starship, grafana, fastapi, svelte"
```

---

### Task 22: Add CI integration for compile tier

**Files:**
- Modify: `.github/workflows/ci.yml` (or create proving-grounds-specific workflow)

**Step 1: Add proving-grounds compile step to CI**

Add a job to the CI workflow:

```yaml
  proving-grounds-compile:
    name: Proving Grounds (compile)
    runs-on: ubuntu-latest
    needs: [build]  # depends on hm binary being built
    steps:
      - uses: actions/checkout@v4
      - name: Download hm binary
        uses: actions/download-artifact@v4
        with:
          name: hm-linux
          path: /usr/local/bin/
      - run: chmod +x /usr/local/bin/hm
      - name: Install Python deps
        run: |
          pip install harmont
      - name: Compile all proving-ground fixtures
        run: proving-grounds/scripts/compile-all.sh
```

**Step 2: Commit**

```bash
git add .github/workflows/
git commit -m "ci: add proving-grounds compile tier to CI"
```

---

### Task 23: Add nightly execution tier workflow

**Files:**
- Create: `.github/workflows/proving-grounds-nightly.yml`

**Step 1: Write the nightly workflow**

```yaml
name: Proving Grounds (nightly)

on:
  schedule:
    - cron: '0 4 * * *'  # 4am UTC daily
  workflow_dispatch:

jobs:
  execute:
    name: Execute proving-ground pipelines
    runs-on: ubuntu-latest
    timeout-minutes: 120
    steps:
      - uses: actions/checkout@v4
      - name: Build hm
        run: cargo build --release -p hm
      - name: Install Python deps
        run: pip install harmont
      - name: Run execution tests
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          HM_BIN: ./target/release/hm
        run: proving-grounds/scripts/execute-all.sh
```

**Step 2: Commit**

```bash
git add .github/workflows/proving-grounds-nightly.yml
git commit -m "ci: add nightly proving-grounds execution workflow"
```

---

## Expansion Guide: Adding More Repos

For each new repo (remaining 85 of 100):

1. **Verify it's in `manifest.toml`** — entry exists with correct metadata
2. **Fetch workflows**: `python proving-grounds/scripts/fetch-workflows.py --slug <slug>`
3. **Study the original CI** — read the downloaded YAML in `repos/<slug>/workflows/`
4. **Write `.harmont/pipeline.py`** — follow the pattern from existing rewrites:
   - Pick the right toolchain(s) for the repo's language
   - Express the core pipeline: build → test → lint → fmt
   - For monorepos, use multi-toolchain composition (Task 18 pattern)
   - For repos with custom build steps, use `sh()` directly
5. **Verify compile**: `hm render ci --dir proving-grounds/repos/<slug>`
6. **Update manifest status**: Change `status = "pending"` to `"compile-only"` or `"passing"`
7. **Commit**

### When a repo's build needs custom steps

Some repos need steps beyond what toolchains provide. Use `sh()`:

```python
@hm.pipeline("ci", default_image="ubuntu:24.04")
def ci(project: hm.Target[RustToolchain]) -> tuple[hm.Step, ...]:
    installed = project.installed
    custom = installed.sh("cargo xtask codegen", label=":codegen:")
    return (
        custom,
        project.build(),
        project.test(),
    )
```

### What harmont currently CANNOT express

These features appear in real-world CI but have no harmont equivalent yet. When rewriting, express only what harmont supports:

| GHA Feature | Harmont Status | Workaround |
|-------------|---------------|------------|
| Matrix builds | Not supported | Write single-config pipeline (one OS, one version) |
| Conditional steps (`if:`) | Not supported | Include unconditionally or omit |
| Artifact upload/download | Not needed | Harmont uses snapshot lineage |
| Reusable workflows | Not supported | Inline the logic |
| Self-hosted runners | Not supported | Use default `ubuntu:24.04` image |
| Service containers | Not supported | Install services in pipeline steps |

### Priority order for remaining 85 repos

1. **Simple repos with matching toolchains** — bat, fd, zoxide, serde, requests, black, poetry, got, chalk, prettier, chi, testify, fzf (quick wins, ~5 min each)
2. **Medium repos with matching toolchains** — nushell, helix, ruff, pnpm, deno, prometheus, vault, etcd (slightly more complex but same pattern)
3. **Complex repos** — these may surface DSL gaps; each needs careful study of original CI
4. **Extreme repos** — mostly reference/aspiration; many use non-GHA CI systems

---

## Success Metrics

| Metric | Target |
|--------|--------|
| Repos with compile-only status | 80/100 within 2 weeks |
| Repos with passing execution | 30/100 within 1 month |
| Compile tier CI green | Every commit |
| Execution tier nightly green | 80% of repos with pipelines |
| DSL gaps identified | Documented per repo in metadata.toml |
