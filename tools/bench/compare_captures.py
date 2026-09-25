#!/usr/bin/env python3
"""Compare two captured view sets numerically, per view and per profile.

The Stage 8 checkpoint needs the *size* of the difference between the wgpu
renderer and the OpenGL reference, not just a byte-equality verdict: the two
renderers deliberately differ this stage (no lightmap atlas yet, no props, no
fixtures, no fog or post-processing). This tool reports, per view:

    mean absolute difference per channel (0..255)
    the share of pixels whose worst channel differs by more than 8/255
    the maximum channel difference

Usage:
    python3 tools/bench/compare_captures.py LEFT RIGHT [--label LEFT RIGHT]
"""
from __future__ import annotations

import argparse
import zlib
from pathlib import Path


def read_png(path: Path) -> tuple[int, int, bytes]:
    """Minimal PNG reader for 8-bit RGB/RGBA files, no external deps."""
    data = path.read_bytes()
    if not data.startswith(b"\x89PNG\r\n\x1a\n"):
        raise SystemExit(f"{path} is not a PNG")
    pos = 8
    width = height = channels = 0
    idat = bytearray()
    while pos + 8 <= len(data):
        length = int.from_bytes(data[pos : pos + 4], "big")
        kind = data[pos + 4 : pos + 8]
        body = data[pos + 8 : pos + 8 + length]
        pos += 12 + length
        if kind == b"IHDR":
            width = int.from_bytes(body[0:4], "big")
            height = int.from_bytes(body[4:8], "big")
            depth = body[8]
            colour = body[9]
            channels = {0: 1, 2: 3, 4: 2, 6: 4}.get(colour, 0)
            if depth != 8 or channels == 0:
                raise SystemExit(f"{path}: unsupported depth {depth} colour {colour}")
        elif kind == b"IDAT":
            idat += body
        elif kind == b"IEND":
            break
    raw = zlib.decompress(bytes(idat))
    stride = width * channels
    out = bytearray(stride * height)
    previous = bytearray(stride)
    offset = 0
    for row in range(height):
        filter_type = raw[offset]
        offset += 1
        line = bytearray(raw[offset : offset + stride])
        offset += stride
        if filter_type == 1:
            for i in range(channels, stride):
                line[i] = (line[i] + line[i - channels]) & 0xFF
        elif filter_type == 2:
            for i in range(stride):
                line[i] = (line[i] + previous[i]) & 0xFF
        elif filter_type == 3:
            for i in range(stride):
                left = line[i - channels] if i >= channels else 0
                line[i] = (line[i] + ((left + previous[i]) >> 1)) & 0xFF
        elif filter_type == 4:
            for i in range(stride):
                left = line[i - channels] if i >= channels else 0
                up = previous[i]
                up_left = previous[i - channels] if i >= channels else 0
                p = left + up - up_left
                pa, pb, pc = abs(p - left), abs(p - up), abs(p - up_left)
                if pa <= pb and pa <= pc:
                    predictor = left
                elif pb <= pc:
                    predictor = up
                else:
                    predictor = up_left
                line[i] = (line[i] + predictor) & 0xFF
        elif filter_type != 0:
            raise SystemExit(f"{path}: unsupported PNG filter {filter_type}")
        out[row * stride : (row + 1) * stride] = line
        previous = line
    return width, height, bytes(out)


def compare(left: Path, right: Path, tolerance: int) -> tuple[float, float, int, int]:
    width, height, a = read_png(left)
    width_b, height_b, b = read_png(right)
    if (width, height) != (width_b, height_b):
        raise SystemExit(f"{left.name}: size {width}x{height} vs {width_b}x{height_b}")
    if a == b:
        return 0.0, 0.0, 0, width * height
    channels = len(a) // (width * height)
    # One flat difference list per row is compact enough in plain Python and
    # much faster than a per-pixel function call.
    total = 0
    worst = 0
    over = 0
    for index in range(0, len(a), channels):
        delta = 0
        for channel in range(3):
            value = abs(a[index + channel] - b[index + channel])
            if value > delta:
                delta = value
        total += delta
        if delta > worst:
            worst = delta
        if delta > tolerance:
            over += 1
    pixels = width * height
    return total / pixels, over / pixels * 100.0, worst, pixels


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("left", type=Path)
    parser.add_argument("right", type=Path)
    parser.add_argument("--label", nargs=2, default=["left", "right"])
    parser.add_argument("--tolerance", type=int, default=8)
    parser.add_argument("--top", type=int, default=100)
    args = parser.parse_args()
    rows = []
    for profile in ("high", "low"):
        left_dir = args.left / profile
        right_dir = args.right / profile
        if not left_dir.is_dir() or not right_dir.is_dir():
            print(f"SKIP {profile}: missing '{left_dir}' or '{right_dir}'")
            continue
        for path in sorted(left_dir.glob("*.png"))[: args.top]:
            right = right_dir / path.name
            if not right.exists():
                print(f"MISSING {profile}/{path.name}")
                continue
            mean, over, worst, pixels = compare(path, right, args.tolerance)
            rows.append((mean, profile, path.name, over, worst, pixels))
    rows.sort(reverse=True)
    print(f"# {args.label[0]} vs {args.label[1]} — per-channel |difference| in 0..255")
    for mean, profile, name, over, worst, pixels in rows:
        print(f"{mean:7.3f}  over{args.tolerance}={over:6.2f}%  max={worst:3d}  "
              f"{profile}/{name} ({pixels} px)")
    if rows:
        worst_row = rows[0]
        print(f"\n{len(rows)} view(s); largest mean difference {worst_row[0]:.3f} on "
              f"{worst_row[1]}/{worst_row[2]}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
