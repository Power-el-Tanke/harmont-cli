"""Harmont CI pipeline — dogfood."""
from __future__ import annotations

import harmont as hm
from harmont.py.uv import UvProject
from harmont.rust import RustToolchain

ALL_APT = (
    "curl",
    "ca-certificates",
    "build-essential",
    "pkg-config",
    "libssl-dev",
    "python3",
    "python3-venv",
)


@hm.target()
def shared_base() -> hm.Step:
    return hm.apt_base(packages=ALL_APT)


@hm.target()
def rust_project(shared_base: hm.Target[hm.Step]) -> RustToolchain:
    return hm.rust(path=".", base=shared_base)


@hm.target()
def py_project(shared_base: hm.Target[hm.Step]) -> UvProject:
    return hm.py.uv(path="dsls/harmont-py", base=shared_base)


@hm.pipeline(
    "ci",
    env={"CI": "true"},
    default_image="ubuntu:24.04",
    triggers=[
        hm.push(branch="main"),
        hm.pull_request(branches="main"),
    ],
)
def ci(
    rust_project: hm.Target[RustToolchain],
    py_project: hm.Target[UvProject],
) -> tuple[hm.Step, ...]:
    warm = rust_project.warmup()
    return (
        warm.sh(
            ". $HOME/.cargo/env && cd . && cargo test --workspace --locked --no-fail-fast",
            label=":rust: test",
        ),
        warm.sh(
            ". $HOME/.cargo/env && cd . && cargo clippy --workspace --tests --locked -- -D warnings",
            label=":rust: clippy",
        ),
        rust_project.fmt(),
        py_project.lint(),
        py_project.fmt(),
        py_project.typecheck(paths="harmont"),
        py_project.run(
            "pytest -v"
            " --deselect tests/test_gradle.py"
            " --deselect tests/test_haskell.py",
            label=":python: test",
        ),
    )
