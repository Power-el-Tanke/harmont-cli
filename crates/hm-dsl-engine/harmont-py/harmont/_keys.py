"""Key derivation for chain-DSL steps.

Order of precedence per the design doc:
  1. explicit `key=` override on .sh()
  2. slugified label (when unique within the pipeline)
  3. stable 12-char hash of (parent_resolved_key, cmd, position)

Collision policy: when two steps' label-slugs collide and neither
claimed the slug via explicit `key=`, both fall back to hash. An
explicit override always wins, even if it would collide with another
step's natural slug.
"""

from __future__ import annotations

import re
import hashlib
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from collections.abc import Iterable

    from ._step import Step

_EMOJI_SHORTCODE_RE = re.compile(r":[a-z0-9_+-]+:")
_NON_ALNUM_RE = re.compile(r"[^a-z0-9]+")

def slugify_label(label: str) -> str:
    """Lowercase, strip ``:emoji_codes:``, replace non-alnum runs with ``-``,
    trim leading/trailing dashes.

    Slugs are ASCII-only by policy (matches Buildkite). Non-ASCII
    letters are treated as separators: ``"Café Build"`` slugs to
    ``"caf-build"`` and ``"构建"`` slugs to ``""``. Labels that reduce
    to the empty string fall back to a hash key in ``resolve_keys``;
    the user's label is preserved on the step's ``label`` field for
    display, only the cross-reference key is hash-based.
    """
    s = label.lower()
    s = _EMOJI_SHORTCODE_RE.sub(" ", s)
    s = _NON_ALNUM_RE.sub("-", s)
    return s.strip("-")

def resolve_keys(steps: Iterable[Step]) -> dict[int, str]:
    """Resolve each Step's key. Returns ``{id(step): key}``.

    The ``id()`` indexing is deliberate: two structurally-equal Steps
    that arose from independent fork branches must keep distinct keys,
    and frozen-dataclass equality would conflate them.
    """
    steps_list = list(steps)
    keys: dict[int, str] = {}
    existing_keys: set[str] = {}
    for position, s in enumerate(steps_list):
        sid = id(s)
        key = s.inner.key
        while key in existing_keys:
            key = hashlib.sha256(key.encode("utf-8")).hexdigest()[:12]
        existing_keys.add(key)
        keys[sid] = key
    return keys
