#!/usr/bin/env python3
"""Display-capability gates shared by the release-binary smoke suites.

Both smoke modules (``test_compiled_build`` and ``test_wgpu_bootstrap``) need
the same answer to "can this host open an SDL window?". Keeping the table here
means it can be exercised headless, without a release binary or a display.

The native macOS and Windows desktops always attempt the suite: a normal
Windows desktop has neither ``DISPLAY`` nor ``WAYLAND_DISPLAY``, and the smoke
binary itself reports a meaningful failure if window creation is unavailable.
Other platforms (Linux and the BSD family) gate on an explicit X11 or Wayland
session so a headless host skips instead of failing for the wrong reason.
"""

from __future__ import annotations

import os
import sys
from typing import Mapping


def display_available(
    platform: str = sys.platform,
    environ: Mapping[str, str] | None = None,
) -> bool:
    """Whether a graphical session can be attempted on ``platform``.

    ``environ`` defaults to ``os.environ``; pass any mapping to unit-test the
    capability table without a display.
    """
    if platform in {"darwin", "win32"}:
        return True
    environment = os.environ if environ is None else environ
    return bool(environment.get("DISPLAY") or environment.get("WAYLAND_DISPLAY"))
