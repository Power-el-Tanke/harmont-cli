"""Harmont CI pipeline — dogfood."""
from __future__ import annotations

import harmont as hm
from harmont.py.uv import UvProject
from harmont.rust import RustProject


@hm.target()
def shared_base() -> hm.Step:
    return hm.apt_base(packages=(
        "curl",
        "ca-certificates",
        "build-essential",
        "pkg-config",
        "libssl-dev",
        "python3",
        "python3-venv",
    ))


@hm.target()
def rust_project(shared_base: hm.Target[hm.Step]) -> RustProject:
    return hm.rust.project(path=".", base=shared_base, test_flags=("--lib",))


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
    rust_project: hm.Target[RustProject],
    py_project: hm.Target[UvProject],
) -> tuple[hm.Step, ...]:
    return (
        rust_project.test,
        rust_project.clippy,
        rust_project.fmt,
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
