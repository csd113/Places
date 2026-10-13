#!/usr/bin/env python3
"""Frozen Home cameras in historical Places Demo; use a preserved matching payload."""
if __package__:
    from .capture_demo_views import main as capture_views
else:
    from capture_demo_views import main as capture_views

VIEWS = {
    "living": ("60.1,8.4,0", "35,-12"),
    "living_reverse": ("63.5,8.3,0", "310,-10"),
    "kitchen": ("57.2,6.6,0", "325,-16"),
    "cabinet_close": ("54.6,4.75,0", "0,-22"),
    "dining": ("56.0,9.5,0", "0,-24"),
    "entry": ("54.4,12.6,0", "90,-2"),
    "stairs": ("54.0,13.6,0", "90,-10"),
    "balcony": ("59.6,12.5,0", "90,-6"),
    "under_balcony": ("62.6,13.6,0", "0,-2"),
    "ceiling": ("61.0,7.0,0", "180,38"),
    "door": ("61.0,5.0,0", "0,-5"),
    "floor": ("60.0,8.8,0", "90,-62"),
}


def main():
    return capture_views(__doc__, VIEWS)


if __name__ == "__main__":
    raise SystemExit(main())
