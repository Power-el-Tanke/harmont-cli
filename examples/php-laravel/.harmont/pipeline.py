"""PHP / Laravel example pipeline.

Uses ``laravel=False`` because this example ships a minimal
PHPUnit suite without the full Laravel scaffolding (no
``artisan`` script). Real Laravel projects flip the flag to
``True`` so the test action runs ``php artisan test``.
"""
from __future__ import annotations

import harmont as hm


@hm.pipeline(
    "ci",
    env={"CI": "true", "APP_ENV": "testing"},
    default_image="ubuntu:24.04",
    triggers=[hm.push(branch="main")],
)
def ci() -> tuple[hm.Step, ...]:
    project = hm.composer(path=".", laravel=False)
    return (
        project.test(),
        project.lint(),
    )
