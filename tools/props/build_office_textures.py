#!/usr/bin/env python3
"""Derive Office native 256px atlases from committed 1024px authored masters.

Uses the existing deterministic box filter; never paints or repacks artwork.
Run before tools/props/build.py --only <the five Office ids>.
"""
import argparse
from pathlib import Path
import sys

from resize_embedded_textures import box_downsample
from tex import decode_png, write_png

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from execution import atomic_write

ROOT = Path(__file__).resolve().parents[2] / "assets/environment/office/props/models"
NAMES = ("desk", "chair", "cabinet", "water_cooler", "vending_machine")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    pending = []
    for name in NAMES:
        source = ROOT / (name + "_master.png")
        width, height, pixels = decode_png(source.read_bytes())
        if (width, height) != (1024, 1024) or any(a != 255 for a in pixels[3::4]):
            raise ValueError(f"{source}: expected opaque 1024x1024 master")
        w, h, native = box_downsample(width, height, bytearray(pixels), 256)
        destination = ROOT / (name + ".png")
        payload = write_png(w, h, native)
        pending.append((name, destination, payload))
    # Validate every source before publishing any native atlas.
    for name, destination, payload in pending:
        if args.check:
            if destination.read_bytes() != payload:
                raise ValueError(f"{destination}: native differs; rebuild Office textures")
        else:
            atomic_write(destination, payload)
        print(f"OK {name}: authored 1024 master -> native 256x256")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
