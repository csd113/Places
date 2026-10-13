#!/usr/bin/env python3
"""Frozen Pool cameras in historical Places Demo; use a preserved matching payload."""
if __package__:
    from .capture_demo_views import main as capture_views
else:
    from capture_demo_views import main as capture_views

VIEWS = {
    "wide": ("10.5,17.9,0", "25,-12"),
    "basin": ("10.5,10.6,0", "130,-26"),
    "walls": ("12.0,9.0,0", "0,5"),
    "ceiling_windows": ("12.0,14.0,0", "0,38"),
    "edge": ("7.1,12.8,0", "90,-35"),
    "stairs": ("22.0,11.5,0", "90,-14"),
    "ladder": ("18.7,14.1,0", "25,-24"),
    "furniture": ("5.9,17.6,0", "325,-22"),
    "curtains": ("21.1,9.0,0", "70,-5"),
    "tile_close": ("7.1,12.8,0", "90,-62"),
    "basin_stairs": ("20.8,15.2,0", "270,-38"),
    "ladder_deck": ("21.5,13.8,0", "320,-18"),
    "bench": ("22.1,9.4,0", "0,-27"),
}


def main():
    return capture_views(__doc__, VIEWS)


if __name__ == "__main__":
    raise SystemExit(main())
