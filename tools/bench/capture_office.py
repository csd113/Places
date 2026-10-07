#!/usr/bin/env python3
"""Capture only Office cameras through Places' native wgpu renderer.

Compile office_asset_review.json into <root>/assets/levels before use.
All captures, settings, logs and timing evidence remain in the chosen output.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time

VIEWS = {
    "wide": ("6.6,6.5,0", "320,-4"),
    "desk": ("2.0,4.7,0", "0,-18"),
    "cabinet": (".9,2.5,0", "0,-26"),
    "cooler": ("4.4,2.0,0", "0,-28"),
    "vending": ("6.0,2.8,0", "0,-4"),
    "ceiling": ("3.4,4.1,0", "30,45"),
    "transition": ("7.0,4.0,90", "90,-6"),
    "damaged": ("13.8,6.5,0", "320,-10"),
}
DEMO_VIEWS = {
    "reception": ("1.4,3.1,90", "90,-5"),
    "workroom": ("9.5,3.5,90", "90,-5"),
    "desk_demo": ("11.4,3.1,180", "180,-10"),
    "hallway": ("34.0,13.0,90", "90,-1"),
    "ceiling_demo": ("13.5,3.5,45", "45,55"),
    "window": ("3.1,6.5,180", "180,-16"),
    "vent": ("7.5,3.6,90", "90,12"),
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--quality", choices=("low", "medium", "high"), default="high")
    parser.add_argument("--level", choices=("office_asset_review", "places_demo"), default="office_asset_review")
    parser.add_argument("--binary", type=Path, default=Path("target/release/places"))
    args = parser.parse_args()
    root, out, binary = args.root.resolve(), args.out.resolve(), args.binary.resolve()
    out.mkdir(parents=True, exist_ok=True)
    state = out / "state"
    state.mkdir(exist_ok=True)
    settings = dict(bindings={"forward": "W", "backward": "S", "strafe_left": "A",
                              "strafe_right": "D", "look_up": "UP", "look_down": "DOWN",
                              "look_left": "LEFT", "look_right": "RIGHT", "jump": "SPACE",
                              "crouch": "C", "interact": "E"},
                    look_speed_h=90, look_speed_v=60, walk_speed=3, invert_look=False,
                    quality=args.quality, texture_filtering=args.quality,
                    lightmaps={"low": "off", "medium": "medium", "high": "full"}[args.quality],
                    reflections={"low": "off", "medium": "medium", "high": "full"}[args.quality],
                    bloom=True, vsync=False, fov_degrees=60, window_mode="windowed",
                    window_width=640, window_height=360)
    (state / "settings.json").write_text(json.dumps(settings))
    manifest = []
    views = DEMO_VIEWS if args.level == "places_demo" else VIEWS
    for name, (spawn, camera) in views.items():
        capture = out / (name + ".png")
        capture.unlink(missing_ok=True)
        env = {k: v for k, v in os.environ.items() if not k.startswith("PLACES_")}
        env.update(PLACES_ASSET_ROOT=str(root), PLACES_STATE_ROOT=str(state),
                   PLACES_LEVEL=args.level, PLACES_QUALITY=args.quality,
                   PLACES_SPAWN=spawn, PLACES_CAMERA=camera, PLACES_BENCH="1",
                   PLACES_BENCH_FRAMES="180", PLACES_BENCH_WARMUP="15",
                   PLACES_CAPTURE=str(capture), PLACES_CAPTURE_FRAME="60",
                   PLACES_VERBOSE="1", PLACES_BENCH_OUT=str(out / (name + ".csv")))
        start = time.monotonic()
        result = subprocess.run([str(binary)], env=env, text=True, stdout=subprocess.PIPE,
                                stderr=subprocess.STDOUT, timeout=120, check=False)
        (out / (name + ".log")).write_text(result.stdout)
        renderer = next((line for line in result.stdout.splitlines()
                         if line.startswith("[renderer] wgpu | adapter:")), None)
        if (result.returncode or not capture.is_file() or renderer is None
                or "not a valid settings file" in result.stdout
                or f"[loading] committed {args.level}" not in result.stdout
                or "0 failed)" not in result.stdout):
            raise RuntimeError(f"{name}: native capture failed; inspect {out / (name + '.log')}")
        manifest.append(dict(name=name, spawn=spawn, camera=camera, quality=args.quality,
                             level=args.level, elapsed_seconds=time.monotonic() - start,
                             renderer=renderer,
                             sha256=hashlib.sha256(capture.read_bytes()).hexdigest()))
        print(f"OK {args.quality}/{name}", flush=True)
    (out / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
