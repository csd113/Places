#!/usr/bin/env python3
"""Counts near-black pixels in captured PNGs, to catch holes in a level shell.

A shelled room never renders the clear colour, so a capture from inside it must
have almost no fully black pixels. A block of them is a missing wall, floor or
ceiling — the most common content bug in this project (a wall authored by its
centre instead of its minimum corner leaves exactly such a hole). Handy as an
automated pre-filter before looking at a capture grid by eye.

The default threshold is deliberately tiny (`8`): the clear colour is exactly
black, while an unlit room still renders its ambient-lit surfaces around 23, so
a low threshold separates "hole" from "dark room" without flagging the many
legitimately dim captures.

The decoder is pure stdlib and handles every PNG filter, because the game's
`PLACES_CAPTURE` writer uses zlib's filtered scanlines; a naive read of the
IDAT gives meaningless colours.

Usage::

    python3 tools/bench/check_holes.py target/agent-work/captures/pool/*.png
    python3 tools/bench/check_holes.py --threshold 8 --step 2 target/agent-work/captures/**/*.png
    python3 tools/bench/check_holes.py --json /tmp/holes.json target/agent-work/captures

Exits non-zero when any capture exceeds `--max-fraction` (default 0.05), so it
can gate a capture matrix.
"""

from __future__ import annotations

import argparse
import glob
import json
import os
import struct
import sys
import zlib


def read_png(path: str) -> tuple[int, int, int, bytes]:
    """Decodes an 8-bit PNG (any filter, RGB/RGBA/gray/palette) to raw pixels."""
    with open(path, "rb") as handle:
        data = handle.read()
    if not data.startswith(b"\x89PNG\r\n\x1a\n"):
        raise ValueError(f"{path}: not a PNG")
    offset = 8
    width = height = depth = colour = 0
    idat = b""
    while offset < len(data):
        length = struct.unpack(">I", data[offset : offset + 4])[0]
        tag = data[offset + 4 : offset + 8]
        payload = data[offset + 8 : offset + 8 + length]
        if tag == b"IHDR":
            width, height, depth, colour = struct.unpack(">IIBB", payload[:10])
        elif tag == b"IDAT":
            idat += payload
        offset += 12 + length
    if depth != 8:
        raise ValueError(f"{path}: only 8-bit PNGs are supported")
    channels = {0: 1, 2: 3, 3: 1, 4: 2, 6: 4}.get(colour)
    if channels is None:
        raise ValueError(f"{path}: unsupported colour type {colour}")
    raw = zlib.decompress(idat)
    stride = width * channels
    out = bytearray(height * stride)
    previous = bytearray(stride)
    for y in range(height):
        row_start = y * (stride + 1)
        filter_type = raw[row_start]
        line = bytearray(raw[row_start + 1 : row_start + 1 + stride])
        if filter_type == 1:
            for x in range(channels, stride):
                line[x] = (line[x] + line[x - channels]) & 0xFF
        elif filter_type == 2:
            for x in range(stride):
                line[x] = (line[x] + previous[x]) & 0xFF
        elif filter_type == 3:
            for x in range(stride):
                left = line[x - channels] if x >= channels else 0
                line[x] = (line[x] + ((left + previous[x]) >> 1)) & 0xFF
        elif filter_type == 4:
            for x in range(stride):
                left = line[x - channels] if x >= channels else 0
                up = previous[x]
                up_left = previous[x - channels] if x >= channels else 0
                p = left + up - up_left
                pa, pb, pc = abs(p - left), abs(p - up), abs(p - up_left)
                predictor = left if (pa <= pb and pa <= pc) else (up if pb <= pc else up_left)
                line[x] = (line[x] + predictor) & 0xFF
        elif filter_type != 0:
            raise ValueError(f"{path}: unknown filter {filter_type}")
        out[y * stride : (y + 1) * stride] = line
        previous = line
    return width, height, channels, bytes(out)


def near_black_fraction(path: str, threshold: int = 8, step: int = 4) -> tuple[float, int, int]:
    """Returns ``(fraction, dark, total)`` for a sampled pixel grid."""
    width, height, channels, pixels = read_png(path)
    dark = total = 0
    for y in range(0, height, step):
        row = y * width * channels
        for x in range(0, width, step):
            index = row + x * channels
            total += 1
            if (
                pixels[index] < threshold
                and pixels[index + 1] < threshold
                and pixels[index + 2] < threshold
            ):
                dark += 1
    return dark / max(1, total), dark, total


def collect(paths: list[str]) -> list[str]:
    """Expands directories and globs into a sorted PNG list."""
    found: list[str] = []
    for path in paths:
        if os.path.isdir(path):
            found.extend(sorted(glob.glob(os.path.join(path, "**", "*.png"), recursive=True)))
        elif any(character in path for character in "*?["):
            found.extend(sorted(glob.glob(path, recursive=True)))
        else:
            found.append(path)
    return found


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("paths", nargs="+", help="capture PNGs, directories or globs")
    parser.add_argument(
        "--threshold",
        type=int,
        default=8,
        help="per-channel dark threshold (default 8: the clear colour is black, ambient tiles are ~23)",
    )
    parser.add_argument("--step", type=int, default=4, help="sample every Nth pixel (default 4)")
    parser.add_argument("--max-fraction", type=float, default=0.05, help="fail above this dark fraction")
    parser.add_argument("--json", metavar="FILE", help="also write the per-capture results as JSON")
    parser.add_argument("--quiet", action="store_true", help="only print failures")
    args = parser.parse_args(argv)

    results = []
    failed = 0
    for path in collect(args.paths):
        try:
            fraction, dark, total = near_black_fraction(path, args.threshold, args.step)
        except (OSError, ValueError, zlib.error) as error:
            print(f"FAIL {path}: {error}")
            failed += 1
            continue
        results.append({"path": path, "fraction": round(fraction, 4), "dark": dark, "sampled": total})
        if fraction > args.max_fraction:
            failed += 1
            print(f"{fraction * 100:5.1f}% near-black  {path}  ({dark}/{total} sampled pixels)")
        elif not args.quiet:
            print(f"{fraction * 100:5.1f}% near-black  {path}")

    if args.json:
        with open(args.json, "w", encoding="utf-8") as handle:
            json.dump(results, handle, indent=2)
        if not args.quiet:
            print(f"wrote {args.json}")

    if failed:
        print(f"\n{failed} capture(s) over {args.max_fraction * 100:.0f}% near-black")
        return 1
    if not args.quiet:
        print(f"\nOK ({len(results)} capture(s) under {args.max_fraction * 100:.0f}% near-black)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
