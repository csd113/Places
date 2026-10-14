#!/usr/bin/env python3
"""Explicit matched native multicore/GPU campaign; never builds or bakes maps.

Without --baseline-binary compare one-worker and automatic execution of the
same binary. With it compare published and candidate builds. Alternate order,
retain each frame_time receipt, and reject mismatched drawable/settings/scene.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[2]
SCENES = {
    "door": ("retro_arcade_demo", "-0.65,1.6,2.8,90", "90,0", False),
    "lateral": ("retro_arcade_demo", "-1.6,1.6,2.8,90", "90,0", True),
    "cabinets": ("retro_arcade_demo", "6.7,1.6,9.5,275", "275,0", False),
    "party": ("retro_arcade_demo", "22.4,1.6,20.4,140", "140,0", False),
    "lanes": ("retro_arcade_demo", "14.65,1.6,24.8,0", "0,0", False),
    "home": ("home_showcase", "1.2,1.6,1,90", "90,0", False),
    "pool": ("pool_showcase", "3,1.6,9,56", "56,0", False),
    "office": ("office_demo", "-2,8.8,9,90", "90,0", False),
}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--baseline-binary", type=Path)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--scenes", nargs="+", choices=SCENES, default=["door", "cabinets", "home"])
    parser.add_argument("--pairs", type=int, choices=range(1, 4), default=3)
    parser.add_argument("--frames", type=int, default=600)
    parser.add_argument("--size", default="1280x720")
    parser.add_argument("--drawable", default="3024x1898")
    parser.add_argument("--windowed", action="store_true")
    parser.add_argument("--vsync", action="store_true")
    parser.add_argument("--lightmaps", choices=("static", "full"), default="full")
    parser.add_argument("--plan-only", action="store_true")
    args = parser.parse_args()
    if not 120 <= args.frames <= 3600:
        parser.error("frames must be 120..3600")
    candidate = args.binary.resolve(strict=True)
    baseline = args.baseline_binary.resolve(strict=True) if args.baseline_binary else candidate
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    plan = []
    for pair in range(args.pairs):
        for scene in args.scenes:
            level, spawn, camera, motion = SCENES[scene]
            for role in (["before", "after"] if pair % 2 == 0 else ["after", "before"]):
                result_dir = out / f"{scene}-{pair}-{role}"
                command = [sys.executable, str(ROOT / "tools/bench/frame_time.py"),
                           "--binary", str(baseline if role == "before" else candidate),
                           "--out", str(result_dir), "--level", level,
                           f"--spawn={spawn}", f"--camera={camera}", "--size", args.size,
                           "--expected-drawable", args.drawable, "--frames", str(args.frames),
                           "--lightmaps", args.lightmaps]
                if not args.windowed:
                    command.append("--fullscreen")
                if args.vsync:
                    command.append("--vsync")
                if motion:
                    command.append("--motion")
                if role == "before" and not args.baseline_binary:
                    command.extend(["--runtime-workers", "1"])
                plan.append(dict(scene=scene, pair=pair, role=role,
                                 command=command, receipt=str(result_dir / "receipt.json")))
    (out / "commands.json").write_text(json.dumps(plan, indent=2) + "\n")
    if args.plan_only:
        print(json.dumps(dict(out=str(out), commands=len(plan))))
        return
    runs = []
    for entry in plan:
        result = subprocess.run(entry["command"], cwd=ROOT, capture_output=True, text=True, check=False)
        result_dir = Path(entry["receipt"]).parent
        (result_dir / "runner.log").write_text(result.stdout + result.stderr)
        if result.returncode:
            raise RuntimeError(f"native campaign failed: {entry['receipt']}; see runner.log")
        receipt = json.loads(Path(entry["receipt"]).read_text())
        runs.append({**entry, "result": receipt})
        (out / "runs.json").write_text(json.dumps(runs, indent=2) + "\n")
        print(json.dumps(dict(scene=entry["scene"], pair=entry["pair"], role=entry["role"],
                              update_ms=receipt["metrics"]["update_ms"],
                              loop_ms=receipt["metrics"]["loop_ms"], gpu_ms=receipt["gpu_elapsed_ms"])), flush=True)
        # Let the native desktop finish closing a fullscreen Space before the
        # next fresh launch. This occurs outside every measured player run.
        time.sleep(2)
    for scene in args.scenes:
        for pair in range(args.pairs):
            matched = [entry["result"] for entry in runs if entry["scene"] == scene and entry["pair"] == pair]
            first, second = matched
            for key in ("settings", "package_sha256", "expected_drawable", "player_bounds", "reflection_passes"):
                if first[key] != second[key]:
                    raise RuntimeError(f"unmatched {scene}/{pair}: {key}")
            for key in ("base_scene_draws", "base_scene_triangles"):
                if first[key]["median"] != second[key]["median"]:
                    raise RuntimeError(f"unmatched {scene}/{pair}: {key}")
    (out / "summary.json").write_text(json.dumps(dict(
        comparison="published-vs-candidate" if args.baseline_binary else "inline-vs-automatic",
        pairs=args.pairs, frames=args.frames, matching_checks="passed", runs=runs), indent=2) + "\n")


if __name__ == "__main__":
    main()
