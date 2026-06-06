"""Proving grounds — exercise harmont's DSL against real-world OSS repos.

Each repo is cloned into /opt/<name> and built/tested/linted using
harmont's toolchain abstractions. All repos run as parallel chains
from a shared apt base.
"""
from __future__ import annotations

from datetime import timedelta
from typing import Annotated

import harmont as hm

RUST_REPOS = [
    ("ripgrep", "https://github.com/BurntSushi/ripgrep"),
    ("clap", "https://github.com/clap-rs/clap"),
    ("tokio", "https://github.com/tokio-rs/tokio"),
    ("starship", "https://github.com/starship/starship"),
    ("ruff", "https://github.com/astral-sh/ruff"),
]

PYTHON_REPOS = [
    ("flask", "https://github.com/pallets/flask"),
    ("django", "https://github.com/django/django"),
    ("fastapi", "https://github.com/fastapi/fastapi"),
]

NPM_REPOS = [
    ("express", "https://github.com/expressjs/express"),
    ("vite", "https://github.com/vitejs/vite"),
    ("svelte", "https://github.com/sveltejs/svelte"),
]

GO_REPOS = [
    ("gin", "https://github.com/gin-gonic/gin"),
    ("terraform", "https://github.com/hashicorp/terraform"),
]

RUBY_REPOS = [
    ("jekyll", "https://github.com/jekyll/jekyll"),
]


@hm.target()
def apt(root: Annotated[hm.Step, hm.BaseImage("ubuntu:24.04")]) -> hm.Step:
    return root.sh(
        "apt-get update && "
        "apt-get install -y --no-install-recommends "
        "git ca-certificates curl build-essential pkg-config",
        label=":apt: base",
        cache=hm.ttl(timedelta(days=1)),
    )


def _clone(base: hm.Step, name: str, url: str) -> hm.Step:
    return base.fork(name).sh(
        f"git clone --depth 1 {url} /opt/{name}",
        label=f":git: {name}",
    )


def _rust_leaves(apt: hm.Step, name: str, url: str) -> list[hm.Step]:
    cloned = _clone(apt, name, url)
    tc = hm.rust.toolchain(path=f"/opt/{name}", base=cloned)
    return [tc.build(), tc.test(), tc.clippy(), tc.fmt()]


def _python_leaves(apt: hm.Step, name: str, url: str) -> list[hm.Step]:
    cloned = _clone(apt, name, url)
    tc = hm.python(path=f"/opt/{name}", base=cloned)
    return [tc.test(), tc.lint(), tc.typecheck()]


def _npm_leaves(apt: hm.Step, name: str, url: str) -> list[hm.Step]:
    cloned = _clone(apt, name, url)
    project = hm.npm(path=f"/opt/{name}", base=cloned)
    return [project.run("build"), project.run("test"), project.run("lint")]


def _go_leaves(apt: hm.Step, name: str, url: str) -> list[hm.Step]:
    cloned = _clone(apt, name, url)
    tc = hm.go(path=f"/opt/{name}", base=cloned)
    return [tc.build(), tc.test(), tc.vet(), tc.fmt()]


def _ruby_leaves(apt: hm.Step, name: str, url: str) -> list[hm.Step]:
    cloned = _clone(apt, name, url)
    project = hm.ruby(path=f"/opt/{name}", base=cloned)
    return [project.test(), project.lint()]


@hm.pipeline(
    "ci",
    env={"CI": "true"},
    default_image="ubuntu:24.04",
)
def ci(apt: hm.Target[hm.Step]) -> tuple[hm.Step, ...]:
    leaves: list[hm.Step] = []

    for name, url in RUST_REPOS:
        leaves.extend(_rust_leaves(apt, name, url))

    for name, url in PYTHON_REPOS:
        leaves.extend(_python_leaves(apt, name, url))

    for name, url in NPM_REPOS:
        leaves.extend(_npm_leaves(apt, name, url))

    for name, url in GO_REPOS:
        leaves.extend(_go_leaves(apt, name, url))

    for name, url in RUBY_REPOS:
        leaves.extend(_ruby_leaves(apt, name, url))

    # Grafana — multi-toolchain monorepo (Go + npm)
    grafana_clone = _clone(apt, "grafana", "https://github.com/grafana/grafana")
    grafana_go = hm.go(path="/opt/grafana", base=grafana_clone)
    grafana_npm = hm.npm(path="/opt/grafana", base=grafana_clone)
    leaves.extend([
        grafana_go.build(),
        grafana_go.test(),
        grafana_go.vet(),
        grafana_npm.run("build"),
        grafana_npm.run("test"),
        grafana_npm.run("lint"),
    ])

    return tuple(leaves)
