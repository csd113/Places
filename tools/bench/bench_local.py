#!/usr/bin/env python3
"""Runs the Places benchmark on this machine and prints the headline numbers.

This is the *local* (macOS) companion to `tools/bench/run_bench.py`, which drives
the PocketCHIP over SSH. It exists so a renderer change can be compared against
the previous build with everything (level, assets, camera, frame count, swap
interval) held fixed, and so the Full / Low and offscreen / direct variants of
one build can be compared with only that switch changed.

Usage::

    python3 tools/bench/bench_local.py --binary target/release/liminal-rust \
        --label batch3 --repeat 3

Every run is a release build of the *current* working tree unless `--binary`
names another executable (the usual way to compare against a baseline checkout).
Nothing outside ``target/agent-work/bench/`` is written.
"""

from __future__ import annotations

import argparse
import json
import os
import statistics
import subprocess
import sys

PACKAGE_ROOT = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", ".."))
OUT_DIR = os.path.join(PACKAGE_ROOT, "target", "agent-work", "bench")

FIELDS = (
    "render_mean_ms",
    "frame_median_ms",
    "frame_p95_ms",
    "swap_mean_ms",
    "draw_calls",
    "visible_batches",
    "total_vertices",
    "vbo_bytes",
    "index_bytes",
    "texture_binds",
    "material_changes",
)


def run_once(args, binary: str) -> dict:
    env = dict(os.environ)
    env.update(
        {
            "LIMINAL_BENCH": "1",
            "LIMINAL_BENCH_FRAMES": str(args.frames),
            "LIMINAL_BENCH_WARMUP": str(args.warmup),
            "LIMINAL_VSYNC": "off",
            "LIMINAL_LEVEL": args.level,
            "LIMINAL_CAMERA": args.camera,
        }
    )
    if args.finish:
        env["LIMINAL_BENCH_FINISH"] = "1"
    if args.quality:
        env["LIMINAL_QUALITY"] = args.quality
    if args.direct:
        env["LIMINAL_NO_OFFSCREEN"] = "1"
    if args.no_lightmaps:
        env["LIMINAL_NO_LIGHTMAPS"] = "1"

    result = subprocess.run(
        [binary],
        cwd=PACKAGE_ROOT,
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        check=False,
    )
    for line in result.stdout.splitlines():
        if line.startswith("BENCH_SUMMARY "):
            return json.loads(line[len("BENCH_SUMMARY ") :])
    raise SystemExit(f"no BENCH_SUMMARY from {binary}:\n{result.stdout[-2000:]}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", default=os.path.join(PACKAGE_ROOT, "target", "release", "liminal-rust"))
    parser.add_argument("--label", default="current")
    parser.add_argument("--level", default="places_demo")
    parser.add_argument("--camera", default="74,0")
    parser.add_argument("--frames", type=int, default=120)
    parser.add_argument("--warmup", type=int, default=20)
    parser.add_argument("--repeat", type=int, default=3)
    parser.add_argument("--quality", default=None, help="full or low (default: the settings file)")
    parser.add_argument("--direct", action="store_true", help="disable the offscreen scene path")
    parser.add_argument("--no-lightmaps", action="store_true", help="force the vertex-lit path")
    parser.add_argument("--finish", action="store_true", help="insert glFinish before the swap")
    args = parser.parse_args()

    os.makedirs(OUT_DIR, exist_ok=True)
    runs = [run_once(args, args.binary) for _ in range(args.repeat)]

    summary = {}
    print(f"{args.label}: {args.level} x{args.repeat} ({args.frames} frames each)")
    for field in FIELDS:
        values = [run.get(field, 0) for run in runs]
        if not values:
            continue
        try:
            median = statistics.median(values)
        except statistics.StatisticsError:  # pragma: no cover - defensive
            continue
        summary[field] = median
        print(f"  {field:16s} median {median:12.3f}   runs {values}")

    out_path = os.path.join(OUT_DIR, f"{args.label}.json")
    with open(out_path, "w", encoding="utf-8") as handle:
        json.dump({"args": vars(args), "runs": runs, "median": summary}, handle, indent=2)
    print(f"  written  {os.path.relpath(out_path, PACKAGE_ROOT)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
