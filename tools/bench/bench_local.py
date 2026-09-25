#!/usr/bin/env python3
"""Runs the Places benchmark on this machine and prints min/median/max numbers.

This is the current local (macOS) benchmark runner: the release binary is
measured directly, with no device or SSH involved. It repeats one configuration
`--repeat` times and reports the minimum, median and maximum of every headline
field, so a renderer change can be compared against the previous build with
everything (level, assets, camera, frame count, swap interval) held fixed, and
the Full / Low and offscreen / direct variants of one build can be compared with
only that switch changed. The minimum is the run least contaminated by unrelated
system work and is the more stable estimator; the median and maximum show the
spread. Nothing outside ``target/agent-work/bench/`` is written.

Usage::

    python3 tools/bench/bench_local.py --binary target/release/places \
        --label current --repeat 3

    python3 tools/bench/bench_local.py --label current_low --quality low
    python3 tools/bench/bench_local.py --label current_direct --direct
    python3 tools/bench/bench_local.py --label current_nolightmaps --no-lightmaps

Every run is a release build of the *current* working tree unless `--binary`
names another executable (the usual way to compare against a baseline checkout).
Each run's per-field min/median/max is written as JSON to
``target/agent-work/bench/<label>.json``.
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
    "loop_median_ms",
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
            "PLACES_BENCH": "1",
            "PLACES_BENCH_FRAMES": str(args.frames),
            "PLACES_BENCH_WARMUP": str(args.warmup),
            "PLACES_VSYNC": "off",
            "PLACES_LEVEL": args.level,
            "PLACES_CAMERA": args.camera,
        }
    )
    if args.finish:
        env["PLACES_BENCH_FINISH"] = "1"
    if args.noswap:
        env["PLACES_BENCH_NOSWAP"] = "1"
    if args.quality:
        env["PLACES_QUALITY"] = args.quality
    if args.direct:
        env["PLACES_NO_OFFSCREEN"] = "1"
    if args.no_lightmaps:
        env["PLACES_NO_LIGHTMAPS"] = "1"

    try:
        result = subprocess.run(
            [binary],
            cwd=PACKAGE_ROOT,
            env=env,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            timeout=args.timeout,
            check=False,
        )
    except subprocess.TimeoutExpired as error:
        raise SystemExit(
            f"{binary} did not finish within {args.timeout:.0f}s"
            " (a sleeping display can block SDL_GL_SwapWindow; try --noswap)"
        ) from error
    for line in result.stdout.splitlines():
        if line.startswith("BENCH_SUMMARY "):
            return json.loads(line[len("BENCH_SUMMARY ") :])
    raise SystemExit(f"no BENCH_SUMMARY from {binary}:\n{result.stdout[-2000:]}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", default=os.path.join(PACKAGE_ROOT, "target", "release", "places"))
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
    parser.add_argument(
        "--noswap",
        action="store_true",
        help="skip SDL_GL_SwapWindow (keeps a run from blocking on a sleeping display)",
    )
    parser.add_argument(
        "--timeout",
        type=float,
        default=300.0,
        help="seconds before one run is treated as hung (default 300)",
    )
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
            low = min(values)
            median = statistics.median(values)
            high = max(values)
        except statistics.StatisticsError:  # pragma: no cover - defensive
            continue
        summary[field] = {"min": low, "median": median, "max": high}
        print(
            f"  {field:16s} min {low:12.3f}   median {median:12.3f}   max {high:12.3f}"
        )

    out_path = os.path.join(OUT_DIR, f"{args.label}.json")
    with open(out_path, "w", encoding="utf-8") as handle:
        json.dump(
            {
                "args": vars(args),
                "runs": runs,
                "summary": summary,
                # Kept for readers of the earlier flat shape.
                "median": {field: stats["median"] for field, stats in summary.items()},
            },
            handle,
            indent=2,
        )
    print(f"  written  {os.path.relpath(out_path, PACKAGE_ROOT)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
