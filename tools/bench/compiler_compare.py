#!/usr/bin/env python3
"""Alternate unchanged-quality baseline and optimized clean compiler builds.

Pairing within each map/repetition reduces bias from changing desktop load. Raw
child reports retain CPU time, peak RSS, UTC timestamps, host load and packages;
every optimized package must pass the strict physical-output comparison.
"""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import statistics
import subprocess
import sys

from compiler_bench import ROOT, save_report


def paired_order(index):
    return ["baseline", "optimized"] if index % 2 == 0 else ["optimized", "baseline"]


def host_snapshot():
    """Resource totals and executable names; never read command arguments."""
    result = subprocess.run(["ps", "-axo", "pid,ppid,%cpu,rss,comm"],
                            capture_output=True, text=True, check=False)
    rows = []
    for line in result.stdout.splitlines():
        fields = line.split(None, 4)
        if len(fields) == 5 and fields[0].isdigit():
            rows.append(dict(pid=int(fields[0]), parent_pid=int(fields[1]),
                             percent_cpu=float(fields[2]), rss_kib=int(fields[3]), executable=fields[4]))
    return dict(time_utc=datetime.now(timezone.utc).isoformat(), exit_code=result.returncode,
                processes=sorted(rows, key=lambda row: row["percent_cpu"], reverse=True)[:12])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--optimized", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--maps", nargs="+", default=["tests/fixtures/levels/test_room.json",
                        "assets/levels/places_demo.json", "assets/levels/lantern_hollow.json"])
    parser.add_argument("--repeat", type=int, default=3)
    parser.add_argument("--workers", type=int, default=12)
    parser.add_argument("--variants", default="off,medium,full")
    parser.add_argument("--timeout", type=float, default=7200)
    parser.add_argument("--label", default="")
    args = parser.parse_args()
    if args.repeat < 1 or args.workers < 1 or args.workers > 12 or args.timeout <= 0:
        parser.error("repeat/timeout must be positive; workers must be in 1..12")
    output = args.out.resolve()
    if output.exists():
        parser.error("use a new evidence directory; earlier measurements are never overwritten")
    output.mkdir(parents=True)
    binaries = {name: getattr(args, name).resolve() for name in ["baseline", "optimized"]}
    report = dict(label=args.label, order="alternating within map/repetition", runs=[], medians=[],
                  binaries={name: dict(path=str(binary), sha256=hashlib.sha256(binary.read_bytes()).hexdigest())
                            for name, binary in binaries.items()})
    save_report(output / "report.json", report)
    for source in args.maps:
        map_id = Path(source).stem
        for index in range(args.repeat):
            paths = {name: output / name / f"{map_id}-pair{index + 1}" for name in binaries}
            for name in paired_order(index):
                command = [sys.executable, str(ROOT / "tools/bench/compiler_bench.py"),
                           "--binary", str(binaries[name]), "--out", str(paths[name]),
                           "--maps", source, "--workers", str(args.workers), "--repeat", "1",
                           "--variants", args.variants, "--timeout", str(args.timeout),
                           "--label", args.label]
                # Comparison waits for both sides, allowing the run order to alternate.
                print(f"Pair {index + 1} {map_id}: {name}", flush=True)
                background_before = host_snapshot()
                result = subprocess.run(command, cwd=ROOT, check=False)
                background_after = host_snapshot()
                child = paths[name] / "report.json"
                if child.is_file():
                    for run in json.loads(child.read_text())["runs"]:
                        report["runs"].append(dict(side=name, pair=index + 1,
                                                   background_before=background_before,
                                                   background_after=background_after, **run))
                    save_report(output / "report.json", report)
                if result.returncode:
                    return result.returncode
            from compare_packages import compare
            packages = {name: paths[name] / f"{map_id}-w{args.workers}-r1.placesmap" for name in binaries}
            equality = compare(packages["baseline"], packages["optimized"], asset_root=ROOT / "assets")
            save_report(output / f"{map_id}-pair{index + 1}-quality.json", equality)
            if not equality["quality_equal"]:
                print(f"Rejected physical output difference: {map_id} pair {index + 1}", flush=True)
                return 1
        sides = {}
        for name in binaries:
            selected = [run for run in report["runs"] if run["side"] == name and Path(run["map"]).stem == map_id]
            sides[name] = {field: statistics.median(run[field] for run in selected)
                           for field in ["wall_seconds", "cpu_seconds", "effective_cores", "peak_rss_bytes"]}
        before, after = sides["baseline"]["wall_seconds"], sides["optimized"]["wall_seconds"]
        report["medians"].append(dict(map=map_id, count=args.repeat, **sides,
                                      seconds_saved=before - after, speedup=before / after,
                                      reduction_percent=100 * (before - after) / before))
        save_report(output / "report.json", report)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
