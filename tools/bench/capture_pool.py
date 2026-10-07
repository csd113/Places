#!/usr/bin/env python3
"""Matched Pool views through the native wgpu renderer; no other environments."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time

VIEWS = {
    "wide": ("10.5,17.9,0", "25,-12"),
    "basin": ("10.5,10.6,0", "130,-26"),
    "walls": ("12.0,9.0,0", "0,5"),
    "ceiling_windows": ("12.0,14.0,0", "0,38"),
    "edge": ("7.1,12.8,0", "90,-35"),
    "stairs": ("22.0,11.5,0", "90,-14"),
    "ladder": ("18.7,14.1,0", "25,-24"),
    "furniture": ("5.9,17.6,0", "325,-22"),
    "curtains": ("21.1,9.0,0", "70,-5"),
    "tile_close": ("7.1,12.8,0", "90,-62"),
    "basin_stairs": ("20.8,15.2,0", "270,-38"),
    "ladder_deck": ("21.5,13.8,0", "320,-18"),
    "bench": ("22.1,9.4,0", "0,-27"),
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
    manifest = []
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
    (out / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
