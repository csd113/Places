#!/usr/bin/env python3
"""Frozen Outdoors cameras in historical Places Demo; use a preserved matching payload."""
if __package__:
    from .capture_demo_views import main as capture_views
else:
    from capture_demo_views import main as capture_views

VIEWS = {
    "arrival": ("4.5,-3,0", "0,4"),
    "path": ("4.5,-14,0", "0,2"),
    "grove": ("4.5,-30,0", "75,8"),
    "ground": ("4.5,-14,0", "30,-48"),
    "lamp": ("4.5,-11,0", "320,-8"),
    "clearing": ("13.5,-50,0", "100,-8"),
    "walkway": ("13.5,-72,0", "0,-8"),
    "destination": ("13.5,-84,0", "0,8"),
    "porch": ("13.5,-88.5,0", "0,10"),
    "source": ("4.5,-7,0", "180,10"),
}


def main():
    return capture_views(__doc__, VIEWS, append_manifest=True)


if __name__ == "__main__":
    raise SystemExit(main())
