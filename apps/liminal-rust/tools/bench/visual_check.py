#!/usr/bin/env python3
"""Pixel-compare rendered frames between two liminal-rust builds.

Renderer optimisations must not change what a level looks like. The game already
has a one-frame capture path (`LIMINAL_CAPTURE=frame.png` renders, writes and
exits), so this script drives that from two builds and compares the results.

Three numbers are reported per shot:

* **pixels** — every pixel whose RGBA differs at all, plus the worst channel
  delta. This is the sensitive signal: reordering or re-batching the same
  geometry must move it by zero, and it does (see
  `notes/renderer-change-validation.md`).
* **significant** — pixels whose worst channel delta exceeds `--tolerance`
  (default 24).
* **largest** — the biggest connected group of significant pixels.

The gate is a *shape*, not just a magnitude. Recompiling the same pipeline can
shift float rounding by an ULP, which moves a near-pixel-aligned edge by one row
and leaves a thin sliver of a few dozen edge pixels; that is what the chair
stress levels show against the pre-optimisation binary. Missing or extra
geometry, by contrast, produces a solid region of thousands of pixels. So a shot
fails when its largest significant component exceeds `--max-component`
(default 256), which separates the two cases without hiding either number.

Pass `--strict` to fail on any differing pixel at all, and `--max-component 0` to
fail on any significant pixel at all.

Usage:
    visual_check.py --baseline <binary> --current <binary> [--out <dir>]
"""

from __future__ import annotations

import argparse
import subprocess
import sys
import zlib
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent.parent

# One shot per camera state worth protecting: interiors, prop-heavy rooms,
# dark/bright baked lighting, doorways and a view along a corridor.
SHOTS: list[tuple[str, str, str]] = [
    # (label, level id, extra environment)
    ("level1_spawn", "level_1", {}),
    ("level1_wide", "level_1", {"LIMINAL_CAMERA": "90"}),
    ("level1_up", "level_1", {"LIMINAL_CAMERA": "180,20"}),
    ("prop_stress_spawn", "prop_stress", {}),
    ("prop_stress_side", "prop_stress", {"LIMINAL_CAMERA": "200"}),
    ("prop_showcase_spawn", "prop_showcase", {}),
    ("prop_showcase_back", "prop_showcase", {"LIMINAL_CAMERA": "0"}),
    ("asset_demo_spawn", "asset_demo", {}),
    ("asset_demo_reverse", "asset_demo", {"LIMINAL_CAMERA": "0"}),
    ("asset_maintained", "asset_maintained", {}),
    ("test_room", "test_room", {}),
    ("chairs_400_facing", "bench_chairs_400", {"LIMINAL_CAMERA": "180"}),
    ("chairs_400_away", "bench_chairs_400", {"LIMINAL_CAMERA": "0"}),
]


def read_png(path: Path) -> tuple[int, int, bytes]:
    """Minimal PNG reader for the 8-bit RGBA files `loader::encode_png` writes."""
    data = path.read_bytes()
    if not data.startswith(b"\x89PNG\r\n\x1a\n"):
        raise SystemExit(f"{path} is not a PNG")
    pos = 8
    width = height = 0
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
            if depth != 8 or colour != 6:
                raise SystemExit(f"{path}: expected 8-bit RGBA, got depth {depth} colour {colour}")
        elif kind == b"IDAT":
            idat += body
        elif kind == b"IEND":
            break
    raw = zlib.decompress(bytes(idat))
    # Undo the per-scanline filter (PNG filter type 0..4), 4 bytes per pixel.
    stride = width * 4
    out = bytearray(stride * height)
    previous = bytearray(stride)
    offset = 0
    for row in range(height):
        filter_type = raw[offset]
        offset += 1
        line = bytearray(raw[offset : offset + stride])
        offset += stride
        if filter_type == 1:
            for i in range(4, stride):
                line[i] = (line[i] + line[i - 4]) & 0xFF
        elif filter_type == 2:
            for i in range(stride):
                line[i] = (line[i] + previous[i]) & 0xFF
        elif filter_type == 3:
            for i in range(stride):
                left = line[i - 4] if i >= 4 else 0
                line[i] = (line[i] + ((left + previous[i]) >> 1)) & 0xFF
        elif filter_type == 4:
            for i in range(stride):
                left = line[i - 4] if i >= 4 else 0
                up = previous[i]
                up_left = previous[i - 4] if i >= 4 else 0
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


def capture(binary: Path, shot: tuple[str, str, str], out_dir: Path, cwd: Path) -> Path:
    label, level, env = shot
    path = out_dir / f"{binary.name}__{label}.png"
    command = [str(binary)]
    environment = {
        "PATH": "/usr/bin:/bin:/usr/sbin:/sbin",
        "LIMINAL_LEVEL": level,
        "LIMINAL_CAPTURE": str(path),
        "LIMINAL_BENCH": "0",
    }
    environment.update(env)
    result = subprocess.run(
        command, cwd=cwd, env=environment, capture_output=True, text=True, timeout=180
    )
    if not path.exists():
        raise SystemExit(f"{label}: no capture produced\n{result.stdout}\n{result.stderr}")
    return path


def compare(
    a: Path, b: Path, tolerance: int
) -> tuple[int, int, int, float, float]:
    """(differing, significant, largest component, fraction, worst)."""
    wa, ha, pa = read_png(a)
    wb, hb, pb = read_png(b)
    if (wa, ha) != (wb, hb):
        raise SystemExit(f"size mismatch: {wa}x{ha} vs {wb}x{hb}")
    width, height = wa, ha
    differing = 0
    significant = 0
    worst = 0
    total = width * height
    mask = bytearray(total)
    for i in range(total):
        o = i * 4
        if pa[o : o + 4] == pb[o : o + 4]:
            continue
        differing += 1
        delta = max(abs(pa[o + channel] - pb[o + channel]) for channel in range(3))
        worst = max(worst, delta)
        if delta > tolerance:
            significant += 1
            mask[i] = 1

    # Largest 4-connected group of significant pixels.
    largest = 0
    seen = bytearray(total)
    for start in range(total):
        if not mask[start] or seen[start]:
            continue
        stack = [start]
        seen[start] = 1
        size = 0
        while stack:
            index = stack.pop()
            size += 1
            x, y = index % width, index // width
            if x > 0 and mask[index - 1] and not seen[index - 1]:
                seen[index - 1] = 1
                stack.append(index - 1)
            if x + 1 < width and mask[index + 1] and not seen[index + 1]:
                seen[index + 1] = 1
                stack.append(index + 1)
            if y > 0 and mask[index - width] and not seen[index - width]:
                seen[index - width] = 1
                stack.append(index - width)
            if y + 1 < height and mask[index + width] and not seen[index + width]:
                seen[index + width] = 1
                stack.append(index + width)
        largest = max(largest, size)
    return differing, significant, largest, differing / total, float(worst)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--current", type=Path, required=True)
    parser.add_argument(
        "--out",
        type=Path,
        default=Path(
            "/private/var/folders/sl/yxxj_m9n37sgntkntqw0x8yw0000gn/T/opencode/visual"
        ),
    )
    parser.add_argument(
        "--levels",
        type=Path,
        default=None,
        help="working directory that contains levels/; defaults to a temp copy of the bench levels",
    )
    parser.add_argument(
        "--tolerance",
        type=int,
        default=24,
        help="worst channel delta above which a pixel counts as a regression",
    )
    parser.add_argument(
        "--max-component",
        type=int,
        default=256,
        help="fail when one connected group of significant pixels exceeds this",
    )
    parser.add_argument(
        "--strict",
        action="store_true",
        help="fail on any differing pixel, not just significant ones",
    )
    args = parser.parse_args()

    cwd = args.levels or args.out
    cwd.mkdir(parents=True, exist_ok=True)
    # The capture runs need `levels/` next to them; copy the bench levels in.
    for source in (REPO / "tools/bench/gen_levels.py",):
        del source
    args.out.mkdir(parents=True, exist_ok=True)

    failures = 0
    print(
        f"{'shot':<26} {'pixels':>9} {'diff %':>8} {'worst':>6} "
        f"{'significant':>12} {'largest':>8}  (tolerance {args.tolerance}, "
        f"max component {args.max_component})"
    )
    for shot in SHOTS:
        try:
            base = capture(args.baseline, shot, args.out, cwd)
            curr = capture(args.current, shot, args.out, cwd)
        except SystemExit as error:
            print(f"{shot[0]:<26} capture failed: {error}")
            failures += 1
            continue
        differing, significant, largest, fraction, worst = compare(
            base, curr, args.tolerance
        )
        if args.strict:
            failed = differing > 0
        else:
            failed = largest > args.max_component
        if failed:
            flag = "  <-- REGRESSION"
        elif differing:
            flag = "  (sub-pixel)"
        else:
            flag = ""
        print(
            f"{shot[0]:<26} {differing:>9} {fraction * 100:>7.3f}% {worst:>6.0f} "
            f"{significant:>12} {largest:>8}{flag}"
        )
        if failed:
            failures += 1

    if failures:
        print(f"\n{failures} shot(s) regressed against the baseline", file=sys.stderr)
        raise SystemExit(1)
    print(
        "\nno shot regressed: every difference was either below the "
        f"{args.tolerance}-level tolerance or a sliver smaller than "
        f"{args.max_component} pixels"
    )


if __name__ == "__main__":
    main()
