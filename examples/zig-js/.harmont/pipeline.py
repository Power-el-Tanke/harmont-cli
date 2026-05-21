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
