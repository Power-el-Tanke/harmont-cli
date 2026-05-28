"""Dagger mirror of the dogfood CI pipeline in .harmont/ci.py.

Hand-wired equivalent of the harmont pipeline, for comparing Dagger's
authoring ergonomics against the harmont DSL. Every shell command below is
copied verbatim from what the harmont toolchains emit
(harmont.rust / harmont.py.uv / harmont._toolchain).
"""

import anyio
import dagger
from dagger import dag, function, object_type

UBUNTU = "ubuntu:24.04"

# Packages from .harmont/ci.py shared_base().
APT_PACKAGES = (
    "curl ca-certificates build-essential pkg-config libssl-dev "
    "python3 python3-venv"
)


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
