#!/usr/bin/env python3
"""Convenience wrapper: (re)generate the spooner-man prop asset.

The real work lives in the pack's toolkit this wrapper sits in (`tools/props/`),
so this script is a thin, deterministic entry point with the exact command the
prop is documented with:

    python3 tools/props/generate_spooner_man.py

It builds the toolkit's *static* low-poly cat from
(`tools/props/parts/spooner_man.py`), refreshes the derived editor proxy file
and thumbnails, and prints the asset's budget report. Blender is deliberately
not part of this: the pack has a Blender-free generator, and nothing here
becomes a runtime dependency.

The shipped entity is a hand-authored skinned Blender export and is the
canonical asset (see `assets/entities/spooner-man/README.md`); this generator
refuses to overwrite it unless `--force` is passed, because its static cat has
no skeleton and would silently drop the character rig.

To regenerate *every* prop instead:

    python3 tools/props/build.py --thumbs
"""

from __future__ import annotations

import os
import sys

# The wrapper lives beside the toolkit it drives, so the import path is its own
# directory; `build.py` resolves the repository root from its own location.
PROPS_TOOL = os.path.dirname(os.path.abspath(__file__))


def main() -> int:
    sys.path.insert(0, PROPS_TOOL)
    import build  # noqa: PLC0415 - the toolkit lives in a sibling directory

    print("generating spooner-man via tools/props/build.py")
    # `--only` rebuilds just this prop while still merging its entry into the
    # shared editor proxy file, and `--thumbs` refreshes its browser preview.
    return build.main(["--only", "spooner-man", "--thumbs"])


if __name__ == "__main__":
    raise SystemExit(main())
