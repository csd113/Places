#!/usr/bin/env python3
"""Repeatedly runs one configuration and prints a robust min/median summary.

The single-shot benchmark is noisy on a desktop machine (the driver and the
desktop compositor dominate a half-millisecond frame), so this helper exists to
report the *minimum* as well as the median: the minimum is the run least
contaminated by unrelated system work, and it is the more stable estimator when
the same command is repeated.

Usage::

    python3 tools/bench/bench_repeat.py --label b3_offscreen --runs 9
    python3 tools/bench/bench_repeat.py --label b2_baseline --runs 9 \
        --binary target/agent-work/baseline/target/release/liminal-rust
"""

from __future__ import annotations

import argparse
import json
import os
import statistics
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
PACKAGE_ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))
OUT_DIR = os.path.join(PACKAGE_ROOT, "target", "agent-work", "bench")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--label", required=True)
    parser.add_argument("--binary", default=os.path.join(PACKAGE_ROOT, "target", "release", "liminal-rust"))
    parser.add_argument("--level", default="places_demo")
    parser.add_argument("--camera", default="74,0")
    parser.add_argument("--frames", type=int, default=120)
    parser.add_argument("--warmup", type=int, default=20)
    parser.add_argument("--runs", type=int, default=9)
    parser.add_argument("--quality", default=None)
    parser.add_argument("--direct", action="store_true")
    parser.add_argument("--no-lightmaps", action="store_true")
    parser.add_argument("--no-finish", action="store_true")
    args = parser.parse_args()

    os.makedirs(OUT_DIR, exist_ok=True)
    samples = []
    for run in range(args.runs):
        out = os.path.join(OUT_DIR, f"{args.label}_run{run}.csv")
        env = dict(os.environ)
        env.update(
            {
                "LIMINAL_BENCH": "1",
                "LIMINAL_BENCH_OUT": out,
                "LIMINAL_BENCH_FRAMES": str(args.frames),
                "LIMINAL_BENCH_WARMUP": str(args.warmup),
                "LIMINAL_VSYNC": "off",
                "LIMINAL_LEVEL": args.level,
                "LIMINAL_CAMERA": args.camera,
            }
        )
        if not args.no_finish:
            env["LIMINAL_BENCH_FINISH"] = "1"
        if args.quality:
            env["LIMINAL_QUALITY"] = args.quality
        if args.direct:
            env["LIMINAL_NO_OFFSCREEN"] = "1"
        if args.no_lightmaps:
            env["LIMINAL_NO_LIGHTMAPS"] = "1"
        result = subprocess.run(
            [args.binary],
            cwd=PACKAGE_ROOT,
            env=env,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            check=False,
        )
        for line in result.stdout.splitlines():
            if line.startswith("BENCH_SUMMARY "):
                samples.append(json.loads(line[len("BENCH_SUMMARY ") :]))
                break
        else:
            raise SystemExit(f"no BENCH_SUMMARY:\n{result.stdout[-1000:]}")

    fields = (
        "render_mean_ms",
        "frame_median_ms",
        "loop_median_ms",
        "draw_calls",
        "texture_binds",
        "material_changes",
        "vbo_bytes",
    )
    summary = {}
    print(f"{args.label}: {args.runs} runs x {args.frames} frames")
    for field in fields:
        values = [sample.get(field, 0) for sample in samples]
        summary[field] = {
            "min": min(values),
            "median": statistics.median(values),
            "max": max(values),
        }
        print(
            f"  {field:16s} min {min(values):9.3f}  median {statistics.median(values):9.3f}  max {max(values):9.3f}"
        )
    with open(os.path.join(OUT_DIR, f"{args.label}.json"), "w", encoding="utf-8") as handle:
        json.dump({"args": vars(args), "runs": samples, "summary": summary}, handle, indent=2)
    return 0


if __name__ == "__main__":
    sys.exit(main())
