#!/usr/bin/env python3
"""Compare compiled maps through the same native renderer and authored frame.

No production asset is replaced. Each package gets an isolated payload whose
unchanged assets link to the preserved collection. Sources, packages, settings,
captures and native load traces remain outside target and temporary directories.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time

from compare_captures import read_png
from compiler_bench import native_package_identity, save_report

ROOT = Path(__file__).resolve().parents[2]
VIEWS = [
    ("demo_office", "places_demo", [9.5, 3.5, 90], [90, -3]),
    ("demo_window", "places_demo", [3.1, 6.5, 0], [180, -16]),
    ("demo_pool", "places_demo", [3, 10, 180], [135, -18]),
    ("demo_ceiling", "places_demo", [13.5, 3.5, 45], [45, 55]),
    ("demo_arch", "places_demo", [54.4, 12.6, 90], [90, -2]),
    ("demo_desk", "places_demo", [3.6, 4.6, 140], [140, -18]),
    ("hollow_street", "lantern_hollow", [34, 21.8, -58], [-58, -3]),
    ("hollow_pond", "lantern_hollow", [-25, -15, -90], [-90, -9]),
    ("hollow_living", "lantern_hollow", [-26.4, 5.2, -35], [-35, -7]),
    ("hollow_overview", "lantern_hollow", [0, 30, 36, 0], [0, -45]),
]


def link(target, destination):
    destination.symlink_to(os.path.relpath(target, destination.parent),
                          target_is_directory=target.is_dir())


def payload(directory, saved_assets, map_id, package):
    """Link immutable inputs; replace only the selected package in this payload."""
    assets = directory / "assets"
    assets.mkdir(parents=True)
    for item in saved_assets.iterdir():
        destination = assets / item.name
        if item.name != "levels":
            link(item, destination)
            continue
        destination.mkdir()
        for level in item.iterdir():
            if level.name != map_id + ".placesmap":
                link(level, destination / level.name)
        link(package, destination / (map_id + ".placesmap"))
    return directory


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--collection", type=Path, required=True)
    parser.add_argument("--before", type=Path, required=True)
    parser.add_argument("--after", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True,
                        help="new durable evidence directory; existing runs are never overwritten")
    parser.add_argument("--profiles", nargs="+", choices=["low", "medium", "high"],
                        default=["low", "medium", "high"])
    parser.add_argument("--views", nargs="+", default=[view[0] for view in VIEWS])
    args = parser.parse_args()
    if any(name not in {view[0] for view in VIEWS} for name in args.views):
        parser.error("unknown view")
    args.out = args.out.resolve()
    if args.out.exists():
        parser.error("use a new evidence directory to preserve earlier captures")
    args.out.mkdir(parents=True)
    saved_assets = args.collection.resolve() / "asset-root/assets"
    if not (saved_assets / "catalog.json").is_file():
        parser.error("preserved assets/catalog.json is required")
    binary = args.binary.resolve()
    report = dict(binary=str(binary), binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
                  authored_frame=1, captures=[], comparisons=[])
    roots = {}
    for label, packages in [("before", args.before), ("after", args.after)]:
        for map_id in {view[1] for view in VIEWS if view[0] in args.views}:
            package = (packages / f"{map_id}-w12-r2.placesmap").resolve()
            if not package.is_file():
                package = (packages / f"{map_id}-w12-r1.placesmap").resolve()
            if not package.is_file():
                parser.error(f"missing compiled package: {package}")
            roots[label, map_id] = payload(args.out / label / map_id / "payload",
                                          saved_assets, map_id, package)
    for profile in args.profiles:
        for name, map_id, spawn, camera in VIEWS:
            if name not in args.views:
                continue
            images = []
            for label in ["before", "after"]:
                directory = args.out / label / profile / name
                directory.mkdir(parents=True)
                state = directory / "state"
                state.mkdir()
                settings = dict(
                    bindings=dict(forward="W", backward="S", strafe_left="A", strafe_right="D",
                                  look_up="UP", look_down="DOWN", look_left="LEFT", look_right="RIGHT",
                                  jump="SPACE", crouch="C", interact="E"),
                    walk_speed=3, quality=profile, texture_filtering=profile,
                    lightmaps=dict(low="off", medium="medium", high="full")[profile],
                    reflections=dict(low="off", medium="medium", high="full")[profile],
                    bloom=False, vsync=False, window_mode="windowed", window_width=640,
                    window_height=360, fov_degrees=60.0)
                (state / "settings.json").write_text(json.dumps(settings, indent=2) + "\n")
                image = directory / "capture.png"
                env = {key: value for key, value in os.environ.items() if not key.startswith("PLACES_")}
                env.update(PLACES_ASSET_ROOT=str(roots[label, map_id]),
                           PLACES_STATE_ROOT=str(state), PLACES_LEVEL=map_id, PLACES_QUALITY=profile,
                           PLACES_SPAWN=",".join(map(str, spawn)), PLACES_CAMERA=",".join(map(str, camera)),
                           PLACES_BENCH="1", PLACES_BENCH_NOSWAP="1", PLACES_VERBOSE="1",
                           PLACES_CAPTURE=str(image), PLACES_CAPTURE_FRAME="1",
                           PLACES_LOAD_TRACE=str(directory / "load.jsonl"),
                           PLACES_STATE_LOG=str(directory / "player.csv"))
                started = time.monotonic()
                result = subprocess.run([str(binary)], cwd=ROOT, env=env, capture_output=True,
                                        text=True, timeout=180, check=False)
                native_log = result.stdout + result.stderr
                (directory / "native.log").write_text(native_log)
                package = roots[label, map_id] / "assets/levels" / (map_id + ".placesmap")
                package_hash = hashlib.sha256(package.read_bytes()).hexdigest()
                manifest_hash = native_package_identity(package)
                expected_identity = (f"package identity level={map_id} variant={settings['lightmaps']} "
                                     f"sha256={manifest_hash} source={package}")
                identity_verified = expected_identity in native_log
                traces = [json.loads(line) for line in (directory / "load.jsonl").read_text().splitlines()
                          if line.strip()]
                preparation = [json.loads(event["detail"]) for event in traces
                               if event["event"] == "preparation_result"]
                precompiled_verified = bool(preparation) and all(
                    event["level"] == map_id and all(event[field] == 0.0 for field in
                    ["lighting_ms", "props_ms", "surfaces_ms", "atlas_ms"])
                    for event in preparation)
                backend = dict(darwin="Metal", linux="Vulkan", win32="Dx12").get(sys.platform)
                backend_verified = backend is not None and f"backend: {backend}" in native_log
                gpu_failure = any(message in native_log for message in
                                  ["panicked", "[wgpu] fatal device error", "Validation Error", "invalid surface"])
                item = dict(label=label, profile=profile, view=name, map_id=map_id,
                            spawn=spawn, camera=camera, exit_code=result.returncode,
                            wall_seconds=time.monotonic() - started, captured=image.is_file(),
                            image=str(image), payload=str(roots[label, map_id]),
                            package_sha256=package_hash, package_identity_verified=identity_verified,
                            manifest_sha256=manifest_hash,
                            precompiled_verified=precompiled_verified, preparation=preparation,
                            backend=backend, backend_verified=backend_verified, gpu_failure=gpu_failure)
                report["captures"].append(item)
                save_report(args.out / "report.json", report)
                print(f"{label} {profile} {name}: exit={result.returncode}, captured={image.is_file()}",
                      flush=True)
                if (result.returncode or not image.is_file() or not identity_verified
                        or not precompiled_verified or not backend_verified or gpu_failure):
                    return 1
                images.append(image)
            left, right = map(read_png, images)
            equal = left == right
            differences = [abs(a - b) for a, b in zip(left[2], right[2])] if left[:2] == right[:2] else []
            report["comparisons"].append(dict(
                profile=profile, view=name, pixel_equal=equal, before_dimensions=list(left[:2]),
                after_dimensions=list(right[:2]), changed_channels=sum(value > 0 for value in differences),
                maximum_channel_difference=max(differences, default=0)))
            save_report(args.out / "report.json", report)
            if not equal:
                print(f"Rejected unexplained native pixel difference: {profile} {name}", flush=True)
                return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
