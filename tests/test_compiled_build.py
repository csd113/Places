#!/usr/bin/env python3
"""Source-only platform capability regression.

Native path/first-run/import acceptance is an explicit developer tool:
    python3 tools/verify-installation.py --help
It must not depend on a shipped demo or launch unisolated games during unittest.
"""

from __future__ import annotations

import os
import sys
import unittest

TEST_DIR = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, TEST_DIR)
from platform_support import display_available  # noqa: E402


class PlatformCapabilityTests(unittest.TestCase):
    """The shared display table needs neither a binary nor a display."""

    def test_display_available_follows_the_platform_table(self):
        # Native macOS and Windows desktops attempt the suite; Windows has no
        # DISPLAY/WAYLAND_DISPLAY variables at all.
        self.assertTrue(display_available("darwin", {}))
        self.assertTrue(display_available("win32", {}))
        self.assertTrue(display_available("win32", {"SESSIONNAME": "Console"}))
        # Other platforms need an explicit X11 or Wayland session.
        self.assertFalse(display_available("linux", {}))
        self.assertTrue(display_available("linux", {"DISPLAY": ":0"}))
        self.assertTrue(display_available("linux", {"WAYLAND_DISPLAY": "wayland-0"}))


# Native installation acceptance lives in tools/verify-installation.py. It uses
# explicit staged resources and one tiny revision-one project/package, never a
# shipped-demo bake or implicit writes to the real user's default data directory.

if __name__ == "__main__":
    unittest.main(verbosity=2)
