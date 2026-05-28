"""Dagger mirror of the dogfood CI pipeline in .harmont/ci.py.

Hand-wired equivalent of the harmont pipeline, for comparing Dagger's
authoring ergonomics against the harmont DSL. Every shell command below is
copied verbatim from what the harmont toolchains emit
(harmont.rust / harmont.py.uv / harmont._toolchain).
"""

from typing import Annotated

import anyio
import dagger
from dagger import DefaultPath, Ignore, dag, function, object_type

UBUNTU = "ubuntu:24.04"

# Source directory argument shared by every leaf. Unlike harmont (which
# snapshots the git working tree, implicitly excluding gitignored paths), Dagger
# uploads the host directory verbatim and does NOT honor .gitignore — so the
# 33GB `target/` dir would be streamed into the engine unless excluded
# explicitly. `Ignore` is Dagger's mechanism for that. `DefaultPath` is resolved
# relative to the module dir (comparison/), so ".." points at the repo root and
# lets callers omit `--source`.
#
# node_modules is excluded to MATCH harmont: harmont's container has no node and
# its git-tree snapshot omits the (gitignored) node_modules, so the hm-dsl-engine
# build.rs finds no esbuild and writes stub TS bundles. Mounting the host
# node_modules would instead ship macOS esbuild binaries into a Linux container
# and break the build — so excluding it is both faithful and correct.
Source = Annotated[
    dagger.Directory,
    DefaultPath(".."),
    Ignore(
        [
            "target",
            ".git",
            "comparison",
            "**/node_modules",
            "**/__pycache__",
            "**/.venv",
        ]
    ),
]

# Packages from .harmont/ci.py shared_base().
APT_PACKAGES = (
    "curl ca-certificates build-essential pkg-config libssl-dev "
    "python3 python3-venv"
)

# rustup install — verbatim from harmont.rust._rustup_cmd("stable", ("clippy", "rustfmt")).
RUSTUP = (
    "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | "
    "sh -s -- -y --default-toolchain stable --profile minimal "
    "--component clippy,rustfmt && . $HOME/.cargo/env && "
    "rustc --version && cargo --version"
)

# uv install — verbatim from harmont.py.uv._uv_install_cmd("latest").
UV_INSTALL = (
    "curl -LsSf https://astral.sh/uv/install.sh | sh && "
    "ln -sf /root/.local/bin/uv /usr/local/bin/uv && uv --version"
)

# .harmont/ci.py passes path="dsls/harmont-py"; in this tree the package is at
# crates/hm-dsl-engine/harmont-py (the path .github/workflows/ci.yml uses). Point
# at the directory that exists on disk so the Python leaves run.
PY_PATH = "crates/hm-dsl-engine/harmont-py"


@object_type
class HarmontDagger:
    @function
    def shared_base(self) -> dagger.Container:
        """ubuntu:24.04 + apt packages + CI=true (mirrors hm.apt_base)."""
        return (
            dag.container()
            .from_(UBUNTU)
            .with_env_variable("CI", "true")
            .with_exec(
                [
                    "sh",
                    "-c",
                    f"apt-get update && apt-get install -y {APT_PACKAGES}",
                ]
            )
        )

    # ---- Rust: mirrors hm.rust.project(path=".", base=shared_base) ----

    @function
    def rust_installed(self) -> dagger.Container:
        """shared_base + rustup stable with clippy & rustfmt. No source mounted."""
        return self.shared_base().with_exec(["sh", "-c", RUSTUP])

    @function
    async def rust_fmt(self, source: Source) -> str:
        """cargo fmt --check. Forks the toolchain (no warmup build), as harmont does."""
        return await (
            self.rust_installed()
            .with_directory("/src", source)
            .with_workdir("/src")
            .with_exec(
                ["sh", "-c", ". $HOME/.cargo/env && cd . && cargo fmt --check"]
            )
            .stdout()
        )

    @function
    def rust_warmup(self, source: Source) -> dagger.Container:
        """rust_installed + source + the warmup build that test/clippy fork from."""
        return (
            self.rust_installed()
            .with_directory("/src", source)
            .with_workdir("/src")
            .with_exec(
                [
                    "sh",
                    "-c",
                    ". $HOME/.cargo/env && cd . && "
                    "cargo build --workspace --tests --locked",
                ]
            )
        )

    @function
    async def rust_test(self, source: Source) -> str:
        """cargo test -p harmont-cli --locked --lib (forks warmup)."""
        return await (
            self.rust_warmup(source)
            .with_exec(
                [
                    "sh",
                    "-c",
                    ". $HOME/.cargo/env && cd . && "
                    "cargo test -p harmont-cli --locked --lib",
                ]
            )
            .stdout()
        )

    @function
    async def rust_clippy(self, source: Source) -> str:
        """cargo clippy --workspace --tests --locked -- -D warnings (forks warmup)."""
        return await (
            self.rust_warmup(source)
            .with_exec(
                [
                    "sh",
                    "-c",
                    ". $HOME/.cargo/env && cd . && "
                    "cargo clippy --workspace --tests --locked -- -D warnings",
                ]
            )
            .stdout()
        )

    # ---- Python uv: mirrors hm.py.uv(path=PY_PATH, base=shared_base) ----

    @function
    def py_synced(self, source: Source) -> dagger.Container:
        """shared_base + uv install + uv sync --all-extras."""
        return (
            self.shared_base()
            .with_exec(["sh", "-c", UV_INSTALL])
            .with_directory("/src", source)
            .with_workdir("/src")
            .with_exec(["sh", "-c", f"cd {PY_PATH} && uv sync --all-extras"])
        )

    @function
    async def py_lint(self, source: Source) -> str:
        """uv run ruff check . in PY_PATH."""
        return await (
            self.py_synced(source)
            .with_exec(["sh", "-c", f"cd {PY_PATH} && uv run ruff check ."])
            .stdout()
        )

    @function
    async def py_fmt(self, source: Source) -> str:
        """uv run ruff format --check . in PY_PATH."""
        return await (
            self.py_synced(source)
            .with_exec(
                ["sh", "-c", f"cd {PY_PATH} && uv run ruff format --check ."]
            )
            .stdout()
        )

    @function
    async def py_typecheck(self, source: Source) -> str:
        """uv run ty check harmont in PY_PATH (mirrors typecheck(paths="harmont"))."""
        return await (
            self.py_synced(source)
            .with_exec(["sh", "-c", f"cd {PY_PATH} && uv run ty check harmont"])
            .stdout()
        )

    @function
    async def py_test(self, source: Source) -> str:
        """uv run pytest with the same deselects as .harmont/ci.py."""
        return await (
            self.py_synced(source)
            .with_exec(
                [
                    "sh",
                    "-c",
                    f"cd {PY_PATH} && uv run pytest -v "
                    "--deselect tests/test_gradle.py "
                    "--deselect tests/test_haskell.py",
                ]
            )
            .stdout()
        )
