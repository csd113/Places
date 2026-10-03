#!/usr/bin/env python3
"""Capture fixed production/diagnostic views or benchmark those same views.

Use identical arguments with --binary and --packages pointing to preserved
before/after builds. State and results are isolated under the requested output.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
import time

ROOT = Path(__file__).resolve().parents[2]
PRODUCTION = [
    ("pool_window", "places_demo", [5.8,10.2,0], [-25,9]),
    ("pool_wide", "places_demo", [14,13,45], [45,-8]),
    ("corridor", "places_demo", [30.5,13,90], [90,-4]),
    ("home", "places_demo", [61,9.4,0], [0,-2]),
    ("zoo", "model_zoo", [15.6,21.1,0], [0,-12]),
    ("zoo_fridge", "model_zoo", [30.6,20.8,0], [0,-6]),
    ("zoo_curved", "model_zoo", [30.6,38.8,0], [0,-8]),
    ("hollow_street", "lantern_hollow", [34,21.8,-58], [-58,-3]),
    ("hollow_house", "lantern_hollow", [-26.4,5.2,-35], [-35,-7]),
    ("hollow_pond", "lantern_hollow", [-25,-15,-90], [-90,-9]),
]
HOUSE_VIEWS = [dict(name=f"hollow_house{i}_{label}", level="lantern_hollow",
    spawn=[centre+offset,3.6,yaw], camera=[yaw,pitch],capture_time=0)
    for i,centre in enumerate((-27,-9,9,27))
    for label,offset,yaw,pitch in (("gable",2.9,-90,18),("floor",-2.9,90,-25))]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/release/places")
    parser.add_argument("--packages", type=Path, help="Directory containing selected .placesmap overrides")
    parser.add_argument("--views", default="production", help="production, diagnostics, or comma-separated names")
    parser.add_argument("--qualities", default="medium,high")
    parser.add_argument("--frames", type=int, default=0, help="Benchmark instead of capture; discard 120 warm-up frames")
    args = parser.parse_args()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    views = [dict(name=n, level=l, spawn=s, camera=c, capture_time=0) for n,l,s,c in PRODUCTION]
    views += json.loads((ROOT / "tools/bench/lighting_quality_views.json").read_text())
    views += HOUSE_VIEWS
    names = args.views.split(",")
    selected = views[:len(PRODUCTION)] if names == ["production"] else views[len(PRODUCTION):-len(HOUSE_VIEWS)] if names == ["diagnostics"] else HOUSE_VIEWS if names == ["houses"] else [view for view in views if view["name"] in names]
    if not selected or (names not in (["production"],["diagnostics"],["houses"]) and len(selected) != len(set(names))):
        parser.error("Unknown view name")
    qualities = args.qualities.split(",")
    if any(q not in ("low","medium","high") for q in qualities) or args.frames < 0:
        parser.error("Qualities must be low,medium,high; frames must be nonnegative")
    asset_root = ROOT
    if args.packages:
        # Bundled IDs deliberately win over installed duplicates. Use a
        # separate bundled asset directory, rather than installed overrides.
        asset_root = out / "package-root"
        payload = asset_root / "assets"
        payload.mkdir(parents=True, exist_ok=True)
        for item in (ROOT / "assets").iterdir():
            if item.name == "levels":
                continue
            link = payload / item.name
            if not link.exists():
                link.symlink_to(item.resolve(), target_is_directory=item.is_dir())
        bundled = payload / "levels"
        bundled.mkdir(exist_ok=True)
        for view in selected:
            package = args.packages.resolve() / (view["level"] + ".placesmap")
            if not package.is_file():
                parser.error(f"Missing package {package}")
            shutil.copy2(package, bundled / package.name)
    manifest = []
    for quality in qualities:
        for view in selected:
            package = asset_root / "assets/levels" / (view["level"] + ".placesmap")
            if not package.is_file():
                parser.error(f"Missing capture package {package}; select --packages for diagnostic overrides")
            package_hash = hashlib.sha256(package.read_bytes()).hexdigest()
            name = f"{view['name']}-{quality}"
            state = out / "state" / name
            state.mkdir(parents=True, exist_ok=True)
            settings = {"bindings": {"forward":"W", "backward":"S", "strafe_left":"A", "strafe_right":"D",
                "look_up":"UP", "look_down":"DOWN", "look_left":"LEFT", "look_right":"RIGHT",
                "jump":"SPACE", "crouch":"C", "interact":"E"}, "look_speed_h":90.0, "look_speed_v":60.0,
                "walk_speed":3.0, "invert_look":False, "mouse_sensitivity":0.12, "quality":quality,
                "texture_filtering":quality, "lightmaps":{"low":"off","medium":"medium","high":"full"}[quality],
                "reflections":"off", "use_low_quality_lighting":False, "vsync":False, "window_mode":"windowed",
                "window_width":1920, "window_height":1080, "fov_degrees":60.0}
            (state / "settings.json").write_text(json.dumps(settings))
            env = {key:value for key,value in os.environ.items() if not key.startswith("PLACES_")}
            env.update(PLACES_ASSET_ROOT=str(asset_root), PLACES_STATE_ROOT=str(state), PLACES_LEVEL=view["level"],
                PLACES_QUALITY=quality, PLACES_SPAWN=",".join(map(str,view["spawn"])),
                PLACES_CAMERA=",".join(map(str,view["camera"])), PLACES_BENCH="1", PLACES_BENCH_NOSWAP="1",
                PLACES_VERBOSE="1", PLACES_NO_REFLECTIONS="1", PLACES_CAPTURE_TIME=str(view.get("capture_time",0)))
            image = out / (name + ".png")
            image.unlink(missing_ok=True)
            if args.frames:
                env.update(PLACES_BENCH_FRAMES=str(args.frames), PLACES_BENCH_WARMUP="120", PLACES_BENCH_OUT=str(out / (name+"-frames.csv")))
            else:
                env.update(PLACES_CAPTURE=str(image), PLACES_BENCH_FRAMES="2400", PLACES_BENCH_WARMUP="0")
            start = time.monotonic()
            with (out / (name+".log")).open("w") as log:
                result = subprocess.run([str(args.binary.resolve())], cwd=ROOT, env=env, stdout=log,
                                        stderr=subprocess.STDOUT, text=True, timeout=300, check=False)
            native_log = (out / (name+".log")).read_text()
            if "initial level preparation failed" in native_log or f"[loading] committed {view['level']}" not in native_log:
                raise SystemExit(f"Requested level did not load: {out / (name+'.log')}")
            if hashlib.sha256(package.read_bytes()).hexdigest() != package_hash:
                raise SystemExit(f"Package changed during the native run: {package}")
            item = dict(view=view, quality=quality, settings=settings, elapsed_seconds=time.monotonic()-start,
                exit_code=result.returncode, binary_sha256=hashlib.sha256(args.binary.read_bytes()).hexdigest(),
                package_sha256=package_hash)
            if image.is_file():
                item["image_sha256"] = hashlib.sha256(image.read_bytes()).hexdigest()
                with image.open("rb") as stream:
                    header = stream.read(24)
                if header[:8] != b"\x89PNG\r\n\x1a\n" or len(header) != 24:
                    raise SystemExit(f"Invalid native capture PNG: {image}")
                item["image_dimensions"] = list(struct.unpack(">II",header[16:24]))
            manifest.append(item)
            (out / "manifest.json").write_text(json.dumps(manifest, indent=2)+"\n")
            print(json.dumps({"name":name,"exit_code":result.returncode,"seconds":item["elapsed_seconds"]}), flush=True)
            if result.returncode or (not args.frames and not image.is_file()):
                raise SystemExit(f"Native run failed: {out / (name+'.log')}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
