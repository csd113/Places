#!/usr/bin/env python3
"""Summarise liminal-rust benchmark runs.

Reads the per-frame CSVs written by `LIMINAL_BENCH_OUT` plus the
`BENCH_SUMMARY` lines captured in each run's stdout, and writes:

* `summary.csv`  — one row per run: the headline metrics;
* `perf_curve.csv` — one row per (phase, scene, repeat);
* a text report on stdout.

Frame time is the primary metric. Two period measurements are reported:

* `loop_ms` — begin-of-frame to begin-of-frame, i.e. the real presentation
  cadence including the swap. This is the honest "frame time".
* `frame_ms` — begin-of-frame to end-of-swap, i.e. everything the renderer
  itself spent on the frame.

`fps_*` values are derived from the loop period, never from a count of
renderer submissions.
"""

from __future__ import annotations

import argparse
import csv
import json
import math
import re
import statistics
from pathlib import Path

CSV_COLUMNS = [
    "frame",
    "update_ms",
    "render_ms",
    "swap_ms",
    "frame_ms",
    "loop_ms",
    "total_vertices",
    "visible_vertices",
    "culled_vertices",
    "total_batches",
    "visible_batches",
    "draw_calls",
    "vbo_bytes",
    "index_bytes",
]

SUMMARY_RE = re.compile(r"BENCH_SUMMARY (\{.*\})")


def read_frames(path: Path) -> list[dict[str, float]]:
    """Reads one per-frame CSV, tolerating a missing or partial trailing row."""
    frames: list[dict[str, float]] = []
    with path.open(newline="") as handle:
        for row in csv.reader(handle):
            if len(row) != len(CSV_COLUMNS):
                continue
            try:
                frames.append({name: float(value) for name, value in zip(CSV_COLUMNS, row)})
            except ValueError:
                continue
    return frames


def percentile(values: list[float], fraction: float) -> float:
    """Nearest-rank percentile; 0.0 for an empty list."""
    if not values:
        return 0.0
    ordered = sorted(values)
    index = round((len(ordered) - 1) * fraction)
    return ordered[index]


def fps(ms: float) -> float:
    return 1000.0 / ms if ms > 0 else 0.0


def summarise_frames(frames: list[dict[str, float]]) -> dict[str, float]:
    """Headline metrics from the per-frame rows, ignoring the first frame."""
    loops = [frame["loop_ms"] for frame in frames[1:]] or [f["loop_ms"] for f in frames]
    frame_ms = [frame["frame_ms"] for frame in frames]
    median_loop = percentile(loops, 0.5)
    p95_loop = percentile(loops, 0.95)
    p99_loop = percentile(loops, 0.99)
    return {
        "frames": float(len(frames)),
        "loop_median_ms": median_loop,
        "loop_mean_ms": statistics.fmean(loops) if loops else 0.0,
        "loop_p95_ms": p95_loop,
        "loop_p99_ms": p99_loop,
        "loop_min_ms": min(loops) if loops else 0.0,
        "loop_max_ms": max(loops) if loops else 0.0,
        "loop_stdev_ms": statistics.pstdev(loops) if len(loops) > 1 else 0.0,
        "frame_median_ms": percentile(frame_ms, 0.5),
        "frame_mean_ms": statistics.fmean(frame_ms) if frame_ms else 0.0,
        "fps_median": fps(median_loop),
        "fps_mean": fps(statistics.fmean(loops)) if loops else 0.0,
        "fps_1pct_low": fps(p99_loop),
        "fps_worst": fps(max(loops)) if loops else 0.0,
    }


def parse_summary(stdout: str) -> dict[str, object]:
    """Last `BENCH_SUMMARY` object in a run's stdout."""
    matches = SUMMARY_RE.findall(stdout)
    if not matches:
        return {}
    return json.loads(matches[-1])


def load_runs(root: Path) -> list[dict[str, object]]:
    """Collects every run described by `<root>/out/*.csv` + `<root>/out/*.log`."""
    out_dir = root / "out"
    runs: list[dict[str, object]] = []
    for csv_path in sorted(out_dir.glob("*.csv")):
        label = csv_path.stem
        frames = read_frames(csv_path)
        if not frames:
            continue
        log_path = out_dir / f"{label}.log"
        summary = parse_summary(log_path.read_text()) if log_path.exists() else {}
        metrics = summarise_frames(frames)
        metrics.update(
            {
                key: float(value)
                for key, value in summary.items()
                if isinstance(value, (int, float)) and not isinstance(value, bool)
            }
        )
        # Per-run labels follow "<phase>__<scene>__r<repeat>".
        parts = label.split("__")
        run = {
            "label": label,
            "phase": parts[0] if len(parts) > 0 else label,
            "scene": parts[1] if len(parts) > 1 else "",
            "repeat": parts[2] if len(parts) > 2 else "",
            "level": summary.get("level", ""),
            "total_vertices": float(frames[-1].get("total_vertices", 0)),
            "visible_vertices": float(frames[-1].get("visible_vertices", 0)),
            "culled_vertices": float(frames[-1].get("culled_vertices", 0)),
            "total_batches": float(frames[-1].get("total_batches", 0)),
            "visible_batches": float(frames[-1].get("visible_batches", 0)),
            "draw_calls": float(frames[-1].get("draw_calls", 0)),
            "vbo_bytes": float(frames[-1].get("vbo_bytes", 0)),
            "index_bytes": float(frames[-1].get("index_bytes", 0)),
        }
        run.update(metrics)
        runs.append(run)
    return runs


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path, help="benchmark root directory (contains out/)")
    parser.add_argument("--csv", type=Path, default=None, help="write summary CSV here")
    parser.add_argument("--json", type=Path, default=None, help="write summary JSON here")
    args = parser.parse_args()

    runs = load_runs(args.root)
    if not runs:
        raise SystemExit(f"no runs found under {args.root}/out")

    runs.sort(key=lambda run: (str(run["phase"]), str(run["scene"]), str(run["repeat"])))

    header = [
        "phase",
        "scene",
        "repeat",
        "loop_median_ms",
        "loop_mean_ms",
        "loop_p95_ms",
        "loop_p99_ms",
        "fps_median",
        "fps_mean",
        "fps_1pct_low",
        "render_mean_ms",
        "swap_mean_ms",
        "total_vertices",
        "visible_vertices",
        "culled_vertices",
        "total_batches",
        "visible_batches",
        "draw_calls",
        "vbo_bytes",
        "index_bytes",
        "frames",
    ]

    if args.csv:
        with args.csv.open("w", newline="") as handle:
            writer = csv.writer(handle)
            writer.writerow(header)
            for run in runs:
                writer.writerow(
                    [
                        run.get(key, "") if key in ("phase", "scene", "repeat") else round(float(run.get(key, 0.0)), 3)
                        for key in header
                    ]
                )
    if args.json:
        args.json.write_text(json.dumps(runs, indent=2) + "\n")

    width = max(len(header[0]), max(len(str(run["phase"])) for run in runs))
    print(f"{'phase':<{width}} {'scene':<26} {'med ms':>8} {'p95 ms':>8} {'fps':>7} {'1%low':>7} "
          f"{'tot vtx':>9} {'vis vtx':>9} {'cull vtx':>9} {'b':>4} {'vb':>4} {'draw':>5}")
    for run in runs:
        print(
            f"{str(run['phase']):<{width}} {str(run['scene']):<26} "
            f"{run['loop_median_ms']:8.2f} {run['loop_p95_ms']:8.2f} {run['fps_median']:7.1f} "
            f"{run['fps_1pct_low']:7.1f} {int(run['total_vertices']):9d} "
            f"{int(run['visible_vertices']):9d} {int(run['culled_vertices']):9d} "
            f"{int(run['total_batches']):4d} {int(run['visible_batches']):4d} {int(run['draw_calls']):5d}"
        )


if __name__ == "__main__":
    main()
