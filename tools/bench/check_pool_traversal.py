#!/usr/bin/env python3
"""Three focused native Pool exits; preserves CSV/log/PNG evidence per case."""
import argparse
import csv
import json
import math
import os
from pathlib import Path
import subprocess

CASES = {
    "submerged_stairs": ("18.6,14.1,180", "180,-15", "jump@0-3,forward@0-3", 3.2),
    "ladder_exit": ("19.7,12.0,90", "90,0", "forward@0-1.6", 1.8),
    "hot_tub_exit": ("3.6,10.9,0", "0,-10", "forward@0-5", 5.2),
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    root, binary, out = args.root.resolve(), args.binary.resolve(), args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    state = out / "state"
    state.mkdir(exist_ok=True)
    (state / "settings.json").write_text(json.dumps(dict(
        bindings=dict(forward="W", backward="S", strafe_left="A", strafe_right="D",
                      look_up="UP", look_down="DOWN", look_left="LEFT", look_right="RIGHT",
                      jump="SPACE", crouch="C", interact="E"), walk_speed=3,
        quality="high", texture_filtering="high", lightmaps="full", reflections="full",
        bloom=True, vsync=False, fov_degrees=60, window_mode="windowed",
        window_width=640, window_height=360)) + "\n")
    records = []
    for name, (spawn, camera, movement, seconds) in CASES.items():
        capture, trace = out / (name + ".png"), out / (name + ".csv")
        if capture.exists() or trace.exists():
            raise RuntimeError(f"Preserve existing evidence: {name}")
        env = {k: v for k, v in os.environ.items() if not k.startswith("PLACES_")}
        env.update(PLACES_ASSET_ROOT=str(root), PLACES_STATE_ROOT=str(state),
                   PLACES_LEVEL="places_demo", PLACES_QUALITY="high", PLACES_BENCH="1",
                   PLACES_SPAWN=spawn, PLACES_CAMERA=camera, PLACES_MOVE_SCRIPT=movement,
                   PLACES_CAPTURE_TIME=str(seconds), PLACES_CAPTURE=str(capture),
                   PLACES_STATE_LOG=str(trace), PLACES_VERBOSE="1",
                   DYLD_LIBRARY_PATH=str(binary.parent))
        result = subprocess.run([str(binary)], env=env, text=True, stdout=subprocess.PIPE,
                                stderr=subprocess.STDOUT, timeout=180, check=False)
        (out / (name + ".log")).write_text(result.stdout)
        if (result.returncode or not capture.exists() or "backend: Metal" not in result.stdout
                or "[loading] committed places_demo" not in result.stdout
                or "0 failed)" not in result.stdout):
            raise RuntimeError(f"{name}: native capture failed")
        with trace.open() as source:
            rows = list(csv.reader(source))
        final = [float(v) for v in rows[-1][1:4]]
        if not all(math.isfinite(v) for v in final):
            raise RuntimeError(f"{name}: non-finite player state")
        # The unchanged standing eye sits near y=0.1 on the y=-1.5 dry deck.
        # A swimmer stranded in either basin finishes more than a metre lower.
        if not -.1 < final[1] < .4:
            raise RuntimeError(f"{name}: player did not finish standing on dry deck: {final}")
        if name == "submerged_stairs" and final[2] <= 16:
            raise RuntimeError(f"{name}: did not cross the submerged treads")
        if name == "ladder_exit" and final[0] <= 20:
            raise RuntimeError(f"{name}: did not reach the east deck")
        if name == "hot_tub_exit" and final[2] >= 7.6:
            raise RuntimeError(f"{name}: did not reach the north dry rim")
        records.append(dict(name=name, spawn=spawn, camera=camera, movement=movement,
                            seconds=seconds, exit_code=result.returncode,
                            state_rows=len(rows), final_eye_xyz=final))
        print(f"OK {name}: {final}", flush=True)
    (out / "summary.json").write_text(json.dumps(records, indent=2) + "\n")


if __name__ == "__main__":
    main()
