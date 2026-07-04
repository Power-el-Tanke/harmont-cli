"""Internal Step dataclass — the chain primitive.

Public callers go through `scratch`, `wait`, `Step.sh`, `Step.fork`
re-exported from ``harmont/__init__.py``. This module is private; nothing
outside ``harmont`` should import from it.
"""

from __future__ import annotations

import hashlib

from dataclasses import dataclass
from collections.abc import Collection
from typing import TYPE_CHECKING, Any
from pathlib import Path

if TYPE_CHECKING:
    from .cache import CachePolicy

from .generated.StepAction import StepAction, StepActionVariantCommand as Command, StepActionVariantMount as Mount
from .generated.Step import Step as SerStep

class Step:
    def __init__(
        self,
        *,
        parent: Step | None = None,
        key: str | None = None,
        label: str | None = None,
        timeout_seconds: int | None  = None,
        image: str | None = None,
        runner: str | None = None,
        runner_args: dict[str, Any],
        cache: Cache | None = None,
        cmd: str | None = None,
        env: dic[str, str] | None = None
        from_: str | None = None,
        to: str | None = None
    ):
        action = None
        feature = None
        
        if cmd and (not (from_ or to)):
            action = Command(cmd=cmd, env=env)
            feature = (
                f"env "
                f"{' '.join[f'{k}={v}' for (k,v) in env]} "
                f"{cmd}"
            ) if env else cmd
        elif (from_ and to) and not (cmd or env):
            action = Mount(from_=from_, to=to)
            feature = f"from {from_} to {to}"
        else:
            raise ValueError(f"Expected from_ and to or cmd with an optional env argument. But got the following: cmd -> {cmd}, env -> {env}, from_ -> {from_}, to -> {to}")
            
        step_key = key 
                   if key 
                   else hash_key(parent.key if parent else "root node", feature)
                   
        self.parent = parent
        self.inner = SerStep(
            action = action,
            key = key,
            label = label,
            timeout_seconds: int | None  = None,
            image = image,
            runner = runner,
            runner_args = runner_args,
            cache = cache
        )
    
    def fork(self, child: Step) -> Step:
        return Step(
            parent = self,
            label= child.inner.label,
            timeout_seconds=child.inner.timeout_seconds,
            image=child.inner.image,
            runner=child.inner.runner,
            runner_args=child.inner.runner_args,
            cache=child.inner.cache,
            cmd=child.inner.cmd,
            env=child.inner.env 
            from_=child.inner.from_,
            to=child.inner.to 
        )
        
    def fork_many(self, children: Collection[Step]) -> list[Step]:
        return [self.fork(child) for child in children]
        
    def sh(
        self,
        cmd: str,
        *,
        cwd: str | None = None,
        label: str | None = None,
        cache: CachePolicy | None = None,
        env: dict[str, str] | None = None,
        image: str | None = None,
        runner: str | None = None,
        runner_args: dict[str, Any] | None = None,
        key: str | None = None,
    ) -> Step:
        return sh(
            cmd,
            cwd=cwd,
            parent=self,
            label=label,
            cache=cache,
            env=env,
            image=effective_image,
            runner=runner,
            runner_args=runner_args,
            key=key,
        )
        
    def mount(
        self,
        from_: str,
        to: str,
        *,
        label: str | None = None,
        cache: CachePolicy | None = None,
        image: str | None = None,
        runner: str | None = None,
        runner_args: dict[str, Any] | None = None,
        key: str | None = None,
    ) -> Step:
        return mount(
            from_,
            to,
            parent=self,
            label=label,
            cache=cache,
            image=effective_image,
            runner=runner,
            runner_args=runner_args,
            key=key,
        )

    def __or__(self, child) -> Step:
        return self.fork(child)
        
    def __rshift__(self, children: Collection[Step]) -> list[Step]:
        return self.fork_many(children)
    
def sh(
    cmd: str,
    *,
    parent: Step | None = None,
    cwd: str | None = None,
    label: str | None = None,
    cache: CachePolicy | None = None,
    env: dict[str, str] | None = None,
    image: str | None = None,
    runner: str | None = None,
    runner_args: dict[str, Any] | None = None,
    key: str | None = None,
) -> Step:
    """Append a shell command to this chain.

    Returns a new ``Step``; the receiver is unchanged (steps are immutable).

    To set a timeout, wrap the result with ``hm.timeout(duration, step)``.

    Args:
        parent: The steps's parent
        cmd: Shell command to run.
        cwd: Directory to run in, relative to the workspace root. Omit to
            run in the root; pass a non-empty path to change directory first.
        label: Human-facing label shown in the UI. Defaults to the command.
        cache: Cache policy controlling result reuse across builds.
        env: Per-step environment variables, merged on top of pipeline-level
            env at render time.
        image: Local-mode Docker base image for this step. Ignored when the
            step has a ``builds_in`` parent (the parent's snapshot wins).
        runner: Executor plugin runner name. ``None`` selects the default
            Docker runner.
        runner_args: Plugin-specific arguments validated by the runner's
            schema.
        key: Manual key override for this step in the v0 IR. Auto-derived
            from the command when omitted.

    Returns:
        A new ``Step`` with this command appended to the chain.

    Raises:
        ValueError: If ``cwd`` is an empty string.
    """
    if cwd == "":
        msg = (
            "hm: cwd must be a non-empty path\n"
            "  → omit cwd= to run in the workspace root, "
            'or pass cwd="some/dir"'
        )
        raise ValueError(msg)
    effective_cmd = f"cd {cwd} && {cmd}" if cwd is not None else cmd

    return Step(
        cmd=effective_cmd,
        parent=parent,
        label=label,
        cache=cache,
        env=env,
        image=effective_image,
        runner=runner,
        runner_args=runner_args,
        key_override=key,
    )

def mount(
    from_: str | Path,
    to: str | Path,
    *,
    parent: Step | None = None,
    label: str | None = None,
    cache: CachePolicy | None = None,
    image: str | None = None,
    runner: str | None = None,
    runner_args: dict[str, Any] | None = None,
    key: str | None = None,
) -> Step:
    if isinstance(to, str):
        to = Path(to)
    if not Path().absolute in to.resolve().parts:
        raise ValueError(f"export path ({to}) not in working directory")
    
    return Step(
        parent=parent,
        from_=from_,
        to=to,
        label=label,
        cache=cache,
        image=effective_image,
        runner=runner,
        runner_args=runner_args,
        key_override=key,
    )

def hash_key(parent_key: str, feat: str) -> str:
        h = hashlib.sha256()
        h.update(parent_key.encode("utf-8"))
        h.update(b"\x00")
        h.update(feat.encode("utf-8"))
        return h.hexdigest()[:12]
