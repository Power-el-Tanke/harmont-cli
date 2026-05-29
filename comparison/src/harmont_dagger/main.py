from collections.abc import Awaitable
from typing import Annotated

import anyio
import dagger
from dagger import DefaultPath, Ignore, dag, function, object_type

UBUNTU = "ubuntu:24.04"

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

APT_PACKAGES = (
    "curl ca-certificates build-essential pkg-config libssl-dev "
    "python3 python3-venv"
)

RUSTUP = (
    "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | "
    "sh -s -- -y --default-toolchain stable --profile minimal "
    "--component clippy,rustfmt && . $HOME/.cargo/env && "
    "rustc --version && cargo --version"
)

UV_INSTALL = (
    "curl -LsSf https://astral.sh/uv/install.sh | sh && "
    "ln -sf /root/.local/bin/uv /usr/local/bin/uv && uv --version"
)

PY_PATH = "crates/hm-dsl-engine/harmont-py"

@object_type
class HarmontDagger:
    @function
    def shared_base(self) -> dagger.Container:
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

    @function
    def rust_installed(self) -> dagger.Container:
        return self.shared_base().with_exec(["sh", "-c", RUSTUP])

    @function
    async def rust_fmt(self, source: Source) -> str:
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
        return await (
            self.py_synced(source)
            .with_exec(
                ["sh", "-c", f"cd {PY_PATH} && uv run ruff format --check ."]
            )
            .stdout()
        )

    @function
    async def py_typecheck(self, source: Source) -> str:
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

    @function
    async def ci(self, source: Source) -> str:
        results: dict[str, str] = {}

        async def run(name: str, coro: Awaitable[str]) -> None:
            results[name] = await coro

        async with anyio.create_task_group() as tg:
            tg.start_soon(run, "rust_test", self.rust_test(source))
            tg.start_soon(run, "rust_clippy", self.rust_clippy(source))
            tg.start_soon(run, "rust_fmt", self.rust_fmt(source))
            tg.start_soon(run, "py_lint", self.py_lint(source))
            tg.start_soon(run, "py_fmt", self.py_fmt(source))
            tg.start_soon(run, "py_typecheck", self.py_typecheck(source))
            tg.start_soon(run, "py_test", self.py_test(source))

        return "\n".join(
            f"=== {name} ===\n{out}" for name, out in sorted(results.items())
        )
