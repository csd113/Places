#!/usr/bin/env python3
"""Convenience wrapper: (re)generate the spooner-man prop asset.

The real work lives in the pack's toolkit (`tools/props/`), so this script is a
thin, deterministic entry point with the exact command the prop is documented
with:

    python3 tools/generate_spooner_man.py

It builds `assets/entities/spooner-man/model/spooner-man.glb` from the low-poly cat module
(`tools/props/parts/spooner_man.py`), refreshes the derived editor proxy file
and thumbnails, and prints the asset's budget report. Blender is deliberately
not part of this: the pack has a Blender-free generator, and nothing here
becomes a runtime dependency.

To regenerate *every* prop instead:

    python3 tools/props/build.py --thumbs
"""

from __future__ import annotations

import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
APP_ROOT = os.path.abspath(os.path.join(HERE, ".."))
PROPS_TOOL = os.path.join(APP_ROOT, "tools", "props")


def main() -> int:
    sys.path.insert(0, PROPS_TOOL)
    import build  # noqa: PLC0415 - the toolkit lives in a sibling directory

    print("generating spooner-man via tools/props/build.py")
    # `--only` rebuilds just this prop while still merging its entry into the
    # shared editor proxy file, and `--thumbs` refreshes its browser preview.
    return build.main(["--only", "spooner-man", "--thumbs"])


if __name__ == "__main__":
    raise SystemExit(main())
