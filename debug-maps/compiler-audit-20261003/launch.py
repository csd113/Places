#!/usr/bin/env python3
"""Open one preserved compiler regression package in the native Places game."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]


def relative_link(target, destination):
    destination.symlink_to(os.path.relpath(target, destination.parent),
                          target_is_directory=target.is_dir())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("alias", nargs="?")
    parser.add_argument("--quality", choices=["low", "medium", "high"], default="low")
    parser.add_argument("--binary", type=Path)
    parser.add_argument("--capture", type=Path, help="save authored frame 1 and exit")
    parser.add_argument("--spawn", help="optional existing Places spawn override")
    parser.add_argument("--camera", help="optional yaw,pitch override")
    args = parser.parse_args()
    entries = json.loads((HERE / "manifest.json").read_text())
    if args.alias is None:
        for entry in entries:
            print(f"{entry['alias']}: {entry['name']} ({','.join(entry['variants'])}); {entry['purpose']}")
        return 0
    entry = next((entry for entry in entries if entry["alias"] == args.alias), None)
    if entry is None:
        parser.error("unknown alias; run without arguments to list saved packages")
    variant = dict(low="off", medium="medium", high="full")[args.quality]
    if variant not in entry["variants"]:
        parser.error(f"this package has no {variant} variant")
    package = HERE / entry["package"]
    if hashlib.sha256(package.read_bytes()).hexdigest() != entry["package_sha256"]:
        parser.error("saved package hash differs from its manifest")
    saved_assets = HERE / "asset-root/assets"
    if not (saved_assets / "catalog.json").is_file():
        saved_assets = ROOT / "assets"
    state = HERE / "runtime" / entry["alias"] / args.quality
    state.mkdir(parents=True, exist_ok=True)
    payload = state / "payload"
    assets = payload / "assets"
    if not assets.exists():
        assets.mkdir(parents=True)
        for item in saved_assets.iterdir():
            if item.name == "levels":
                levels = assets / "levels"
                levels.mkdir()
                for level in item.iterdir():
                    if level.name != entry["id"] + ".placesmap":
                        relative_link(level, levels / level.name)
                relative_link(package, levels / (entry["id"] + ".placesmap"))
            elif item.name == "catalog.json" and entry.get("catalogue"):
                relative_link(HERE / entry["catalogue"], assets / item.name)
            else:
                relative_link(item, assets / item.name)
    settings = state / "settings.json"
    if not settings.exists() or args.capture:
        settings.write_text(json.dumps(dict(
            bindings=dict(forward="W", backward="S", strafe_left="A", strafe_right="D",
                          look_up="UP", look_down="DOWN", look_left="LEFT", look_right="RIGHT",
                          jump="SPACE", crouch="C", interact="E"),
            walk_speed=3, quality=args.quality, texture_filtering=args.quality,
            lightmaps=variant, reflections=variant, bloom=False, vsync=False,
            window_mode="windowed", window_width=640, window_height=360), indent=2) + "\n")
    binary = args.binary.resolve() if args.binary else HERE / "bin/places"
    if not binary.is_file():
        binary = ROOT / "target/release/places"
    if not binary.is_file():
        parser.error("build Places with cargo build --release or restore the saved native build")
    env = {key: value for key, value in os.environ.items() if not key.startswith("PLACES_")}
    env.update(PLACES_ASSET_ROOT=str(payload), PLACES_STATE_ROOT=str(state),
               PLACES_LEVEL=entry["id"], PLACES_QUALITY=args.quality, PLACES_BENCH="1")
    if args.spawn:
        env["PLACES_SPAWN"] = args.spawn
    if args.camera:
        env["PLACES_CAMERA"] = args.camera
    if args.capture:
        image = args.capture.resolve()
        if image.exists():
            parser.error("choose a new capture path to preserve earlier evidence")
        image.parent.mkdir(parents=True, exist_ok=True)
        env.update(PLACES_CAPTURE=str(image), PLACES_CAPTURE_FRAME="1", PLACES_BENCH_NOSWAP="1",
                   PLACES_VERBOSE="1", PLACES_LOAD_TRACE=str(image.with_suffix(".load.jsonl")))
    return subprocess.run([str(binary)], cwd=ROOT, env=env, check=False).returncode


if __name__ == "__main__":
    raise SystemExit(main())
