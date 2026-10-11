#!/usr/bin/env python3
"""Normalize a small longitude-edge bias in an existing sky PNG offline.

This preserves the supplied artwork, dimensions, colour type and ancillary
chunks. Only RGB in the requested narrow left/right strips changes; alpha and
the clamped north/south poles are never joined. The player loads the saved PNG.
Use --repair explicitly; --check is read-only.
"""
from __future__ import annotations

import argparse
import math
from pathlib import Path

from seam_repair import PngImage, read_png, write_png


def normalize(image: PngImage, band: int = 16) -> PngImage:
    if image.width != image.height * 2 or image.colour_type not in (2, 6):
        raise ValueError('sky must be a 2:1 RGB/RGBA PNG')
    if not 2 <= band <= image.width // 4:
        raise ValueError('edge band must be in 2..width/4')
    pixels = bytearray(image.pixels)
    stride = image.width * image.channels
    for y in range(image.height):
        left = y * stride
        right = left + (image.width - 1) * image.channels
        for channel in range(3):
            a, b = image.pixels[left + channel], image.pixels[right + channel]
            target = (a + b + 1) // 2
            for offset in range(band):
                weight = .5 + .5 * math.cos(math.pi * offset / (band - 1))
                for edge, direction, correction in ((left, 1, target - a), (right, -1, target - b)):
                    at = edge + direction * offset * image.channels + channel
                    pixels[at] = max(0, min(255, int(image.pixels[at] + weight * correction + .5)))
    return PngImage(image.width, image.height, image.colour_type, bytes(pixels), image.ancillary)


def aligned(image: PngImage) -> bool:
    stride = image.width * image.channels
    return all(image.pixels[y * stride:y * stride + 3] ==
               image.pixels[(y + 1) * stride - image.channels:(y + 1) * stride - image.channels + 3]
               for y in range(image.height))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument('--check', action='store_true')
    mode.add_argument('--repair', action='store_true')
    parser.add_argument('--band', type=int, default=16)
    parser.add_argument('paths', type=Path, nargs='+')
    args = parser.parse_args()
    failed = False
    for path in args.paths:
        image = read_png(str(path))
        if args.repair and not aligned(image):
            image = normalize(image, args.band)
            write_png(str(path), image)
        success = aligned(image)
        print(f'{"OK" if success else "MISMATCH"} {path}')
        failed |= not success
    return int(failed)


if __name__ == '__main__':
    raise SystemExit(main())
