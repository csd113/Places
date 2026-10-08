#!/usr/bin/env python3
"""Run the normal native player with the versioned Art-style hero cameras.

Compile the fixture explicitly first. Captures, logs and provenance are never
overwritten. --frames measures the same scene without screenshot readback;
--play opens the fixture interactively. Quality controls reuse normal settings;
the optional diagnostic selector requires the development feature build.
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
import tempfile
import time

ROOT = Path(__file__).resolve().parents[2]
MANIFEST = ROOT / "docs/art-style/hero-manifest.json"


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def select_views(manifest: dict, names: str | None) -> list[dict]:
    views = manifest["views"]
    if names is None:
        return views
    selected = names.split(",")
    known = {view["name"]: view for view in views}
    if len(selected) != len(set(selected)) or any(name not in known for name in selected):
        raise ValueError("Unknown or duplicate hero view")
    return [known[name] for name in selected]


def visual_receipt(log: str, requested: str | None) -> dict | None:
    """Require a real feature-build capture receipt rather than an ignored env var."""
    if requested is None:
        return None
    records = []
    for line in log.splitlines():
        if line.startswith("[visual-diagnostic] "):
            try:
                value = json.loads(line.removeprefix("[visual-diagnostic] "))
            except json.JSONDecodeError:
                continue
            if value.get("event") == "capture":
                records.append(value)
    if not records or records[-1].get("mode") != requested:
        raise ValueError("No capture-time receipt for the requested visual diagnostic")
    return records[-1]


def validate_native(log: str, level: str, image: Path | None, expected: list[int],
                    settings: dict | None = None) -> None:
    if (f"[loading] committed {level}" not in log
            or "[renderer] wgpu | adapter:" not in log
            or "not a valid settings file" in log
            or "initial level preparation failed" in log):
        raise ValueError("Native hero failed to load with valid settings/renderer")
    if settings is not None:
        receipt = next((line for line in log.splitlines() if line.startswith("[settings]")), "")
        lightmaps = "off" if settings.get("use_low_quality_lighting") else settings['lightmaps']
        reflections = "off" if settings.get("use_low_quality_lighting") else settings['reflections']
        expected_fields = [f"quality {settings['quality']} (saved {settings['quality']})",
                           f"| lightmaps {lightmaps} |",
                           f"| reflections {reflections} |",
                           f"| filtering {settings['texture_filtering']} |"]
        if any(field not in receipt for field in expected_fields):
            raise ValueError("Native effective graphics settings do not match the request")
    if image is not None:
        header = image.read_bytes()[:24]
        if (header[:8] != b"\x89PNG\r\n\x1a\n" or len(header) != 24
                or list(struct.unpack(">II", header[16:24])) != expected
                or f"PLACES_CAPTURE: wrote {image}" not in log):
            raise ValueError("Missing native capture receipt or unexpected drawable dimensions")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, help="New output directory, required except --play")
    parser.add_argument("--binary", type=Path, default=ROOT / "target/release/places")
    parser.add_argument("--package", type=Path)
    parser.add_argument("--manifest", type=Path, default=MANIFEST)
    parser.add_argument("--views", help="Comma-separated manifest camera names")
    parser.add_argument("--quality", choices=("low", "medium", "high"), default="high")
    parser.add_argument("--lightmaps", choices=("off", "medium", "full"))
    parser.add_argument("--filtering", choices=("low", "medium", "high"))
    parser.add_argument("--low-lighting", action="store_true", help="Existing independent Low lighting override")
    parser.add_argument("--diagnostic", help="Opt-in visual-diagnostics build mode; final keeps normal composition")
    parser.add_argument("--entity-light-trace", action="store_true", help="Existing actual entity payload/anchor trace")
    parser.add_argument("--quality-cycle", help="Existing live settings script, e.g. 60:low,180:high")
    parser.add_argument("--graphics-cycle", help="Existing Advanced settings script, e.g. 60:lightmaps=off")
    parser.add_argument("--capture-frame", type=int, help="Ready frame to capture instead of the fixed half-second")
    parser.add_argument("--frames", type=int, default=0, help="Measure frames after 120 warmup frames")
    parser.add_argument("--play", action="store_true")
    args = parser.parse_args()
    manifest = json.loads(args.manifest.read_text())
    try:
        views = select_views(manifest, args.views)
    except ValueError as error:
        parser.error(str(error))
    if (args.frames < 0 or (args.play and args.frames) or (not args.play and not args.out)
            or (args.capture_frame is not None and args.capture_frame < 1)):
        parser.error("Use --out for evidence, nonnegative --frames, or --play")
    binary = args.binary.resolve()
    package = (args.package or ROOT / manifest["package"]).resolve()
    if not binary.is_file() or not package.is_file():
        parser.error("Missing native binary or compiled hero package; compile explicitly first")
    out = args.out.resolve() if args.out else None
    if out:
        if out.exists() and any(out.iterdir()):
            parser.error("Output must be empty; preserve previous evidence")
        out.mkdir(parents=True, exist_ok=True)
    identity = dict(binary_sha256=digest(binary), package_sha256=digest(package),
                    source_sha256=digest(ROOT / manifest["source"]),
                    camera_manifest_sha256=digest(args.manifest),
                    capture_tool_sha256=digest(Path(__file__)),
                    catalog_sha256=digest(ROOT / "assets/catalog.json"),
                    git_revision=subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT,
                                                         text=True).strip(),
                    git_diff_sha256=hashlib.sha256(subprocess.check_output(
                        ["git", "diff", "HEAD"], cwd=ROOT)).hexdigest())
    receipts = []
    with tempfile.TemporaryDirectory(prefix="places-art-style-hero-") as staging:
        root = Path(staging)
        payload = root / "assets"
        payload.mkdir()
        for item in (ROOT / "assets").iterdir():
            if item.name != "levels":
                (payload / item.name).symlink_to(item.resolve(), target_is_directory=item.is_dir())
        bundled = payload / "levels"
        bundled.mkdir()
        shutil.copy2(package, bundled / package.name)
        state = root / "state"
        state.mkdir()
        settings = dict(manifest["settings"])
        settings.update(quality=args.quality, texture_filtering=args.quality,
                        lightmaps={"low": "off", "medium": "medium", "high": "full"}[args.quality],
                        reflections={"low": "off", "medium": "medium", "high": "full"}[args.quality])
        if args.lightmaps:
            settings["lightmaps"] = args.lightmaps
        if args.filtering:
            settings["texture_filtering"] = args.filtering
        settings["use_low_quality_lighting"] = args.low_lighting
        (state / "settings.json").write_text(json.dumps(settings, indent=2) + "\n")
        for view in views[:1] if args.play else views:
            env = {k: v for k, v in os.environ.items() if not k.startswith("PLACES_")}
            env.update(PLACES_ASSET_ROOT=str(root), PLACES_STATE_ROOT=str(state),
                       PLACES_LEVEL=manifest["level"], PLACES_QUALITY=args.quality,
                       PLACES_SPAWN=",".join(map(str, view["spawn"])),
                       PLACES_CAMERA=",".join(map(str, view["camera"])), PLACES_VERBOSE="1",
                       DYLD_LIBRARY_PATH=str(binary.parent))
            if args.diagnostic:
                env["PLACES_VISUAL_DIAGNOSTIC"] = args.diagnostic
            if args.entity_light_trace:
                env["PLACES_ENTITY_LIGHT_TRACE"] = "all"
            if args.quality_cycle:
                env["PLACES_BENCH_QUALITY_CYCLE"] = args.quality_cycle
            if args.graphics_cycle:
                env["PLACES_BENCH_GRAPHICS_CYCLE"] = args.graphics_cycle
            if args.play:
                return subprocess.run([str(binary)], env=env, cwd=ROOT, check=False).returncode
            name = view["name"]
            image = None if args.frames else out / (name + ".png")
            env.update(PLACES_BENCH="1", PLACES_BENCH_FRAMES=str(args.frames or manifest["capture"]["limit_frames"]),
                       PLACES_BENCH_WARMUP="120" if args.frames else str(manifest["capture"]["warmup_frames"]),
                       PLACES_BENCH_OUT=str(out / (name + ".csv")))
            if args.quality_cycle or args.graphics_cycle:
                env["PLACES_LOAD_TRACE"] = str(out / (name + "-load.jsonl"))
            if image:
                env["PLACES_CAPTURE"] = str(image)
                if args.capture_frame is not None:
                    env["PLACES_CAPTURE_FRAME"] = str(args.capture_frame)
                else:
                    env["PLACES_CAPTURE_TIME"] = str(manifest["capture"]["ready_world_seconds"])
            start = time.monotonic()
            # macOS time reports process peak RSS and CPU time, independently
            # of the engine's CPU submission telemetry. It is not GPU timing.
            command = ["/usr/bin/time", "-l", str(binary)] if os.uname().sysname == "Darwin" else [str(binary)]
            with (out / (name + ".log")).open("w") as log_file:
                result = subprocess.run(command, cwd=ROOT, env=env, stdout=log_file,
                                        stderr=subprocess.STDOUT, timeout=300, check=False)
            log = (out / (name + ".log")).read_text()
            if result.returncode:
                raise RuntimeError(f"{name}: native exit {result.returncode}; inspect log")
            validate_native(log, manifest["level"], image, manifest["capture"]["expected_drawable"], settings)
            diagnostic_receipt = visual_receipt(log, args.diagnostic) if image else None
            if digest(package) != identity["package_sha256"] or digest(binary) != identity["binary_sha256"]:
                raise RuntimeError("Binary/package changed during capture")
            receipts.append(dict(view=view, settings=settings, identity=identity,
                                 diagnostic=args.diagnostic, quality_cycle=args.quality_cycle,
                                 entity_light_trace=args.entity_light_trace,
                                 graphics_cycle=args.graphics_cycle, capture_frame=args.capture_frame,
                                 diagnostic_receipt=diagnostic_receipt,
                                 ready_world_seconds=None if args.frames or args.capture_frame is not None
                                 else manifest["capture"]["ready_world_seconds"],
                                 measured_frames=args.frames, elapsed_seconds=time.monotonic()-start,
                                 exit_code=result.returncode, image_sha256=digest(image) if image else None,
                                 settings_receipt=next(line for line in log.splitlines() if line.startswith("[settings]")),
                                 renderer=next(line for line in log.splitlines() if line.startswith("[renderer] wgpu | adapter:"))))
            (out / "manifest.json").write_text(json.dumps(receipts, indent=2) + "\n")
            print(f"OK {args.quality}/{name}", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
