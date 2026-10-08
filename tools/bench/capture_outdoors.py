#!/usr/bin/env python3
"""Matched Outdoors views through the native wgpu renderer; immutable camera set for the serial visual journal."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time

VIEWS = {
    "arrival": ("4.5,-3,0", "0,4"),
    "path": ("4.5,-14,0", "0,2"),
    "grove": ("4.5,-30,0", "75,8"),
    "ground": ("4.5,-14,0", "30,-48"),
    "lamp": ("4.5,-11,0", "320,-8"),
    "clearing": ("13.5,-50,0", "100,-8"),
    "walkway": ("13.5,-72,0", "0,-8"),
    "destination": ("13.5,-84,0", "0,8"),
    "porch": ("13.5,-88.5,0", "0,10"),
    "source": ("4.5,-7,0", "180,10"),
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--quality", choices=("low", "medium", "high"), default="high")
    parser.add_argument("--views", default=",".join(VIEWS))
    args = parser.parse_args()
    root, out, binary = args.root.resolve(), args.out.resolve(), args.binary.resolve()
    out.mkdir(parents=True, exist_ok=True)
    state = out / "state"
    state.mkdir(exist_ok=True)
    variant = {"low": "off", "medium": "medium", "high": "full"}[args.quality]
    settings = dict(bindings={"forward": "W", "backward": "S", "strafe_left": "A",
                              "strafe_right": "D", "look_up": "UP", "look_down": "DOWN",
                              "look_left": "LEFT", "look_right": "RIGHT", "jump": "SPACE",
                              "crouch": "C", "interact": "E"},
                    look_speed_h=90, look_speed_v=60, walk_speed=3, invert_look=False,
                    quality=args.quality, texture_filtering=args.quality, lightmaps=variant,
                    reflections=variant, bloom=True, vsync=False, fov_degrees=60,
                    window_mode="windowed", window_width=640, window_height=360)
    (state / "settings.json").write_text(json.dumps(settings, indent=2) + "\n")
    manifest_path = out / "manifest.json"
    manifest = json.loads(manifest_path.read_text()) if manifest_path.exists() else []
    for name in args.views.split(","):
        spawn, camera = VIEWS[name]
        capture = out / (name + ".png")
        if capture.exists():
            raise RuntimeError(f"Preserve existing evidence: {capture}")
        env = {k: v for k, v in os.environ.items() if not k.startswith("PLACES_")}
        env.update(PLACES_ASSET_ROOT=str(root), PLACES_STATE_ROOT=str(state),
                   PLACES_LEVEL="places_demo", PLACES_QUALITY=args.quality,
                   PLACES_SPAWN=spawn, PLACES_CAMERA=camera, PLACES_BENCH="1",
                   PLACES_BENCH_FRAMES="100", PLACES_BENCH_WARMUP="15",
                   PLACES_CAPTURE=str(capture), PLACES_CAPTURE_FRAME="60",
                   PLACES_VERBOSE="1", PLACES_BENCH_OUT=str(out / (name + ".csv")),
                   DYLD_LIBRARY_PATH=str(binary.parent))
        start = time.monotonic()
        result = subprocess.run([str(binary)], env=env, text=True, stdout=subprocess.PIPE,
                                stderr=subprocess.STDOUT, timeout=180, check=False)
        (out / (name + ".log")).write_text(result.stdout)
        renderer = next((line for line in result.stdout.splitlines()
                         if line.startswith("[renderer] wgpu | adapter:")), None)
        if (result.returncode or not capture.is_file() or renderer is None
                or "not a valid settings file" in result.stdout
                or "[loading] committed places_demo" not in result.stdout
                or "0 failed)" not in result.stdout):
            raise RuntimeError(f"{name}: capture failed; inspect {out / (name + '.log')}")
        manifest.append(dict(name=name, spawn=spawn, camera=camera, quality=args.quality,
                             level="places_demo", elapsed_seconds=time.monotonic() - start,
                             renderer=renderer, sha256=hashlib.sha256(capture.read_bytes()).hexdigest()))
        print(f"OK {args.quality}/{name}", flush=True)
    manifest_path.write_text(json.dumps(manifest, indent=2) + "\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
