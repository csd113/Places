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
import math
import os
from pathlib import Path
import shutil
import struct
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[2]
MANIFEST = ROOT / "tests/fixtures/native/hero-manifest.json"


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


def visual_receipt(log: str, requested: str | None, captured: Path | None = None) -> dict | None:
    """Require a real feature-build capture receipt rather than an ignored env var."""
    if requested is None:
        return None
    records = []
    pending = None
    for line in log.splitlines():
        if line.startswith("[visual-diagnostic] "):
            try:
                value = json.loads(line.removeprefix("[visual-diagnostic] "))
            except json.JSONDecodeError:
                continue
            if value.get("event") == "capture":
                records.append(value)
                pending = value
        if captured is not None and line == f"PLACES_CAPTURE: wrote {captured}":
            if pending is None or pending.get("mode") != requested:
                raise ValueError("No matching capture-time receipt for the requested visual diagnostic")
            return pending
        if line.startswith("PLACES_CAPTURE: wrote "):
            pending = None
    if captured is not None:
        raise ValueError("Missing image-bound visual diagnostic receipt")
    if not records or records[-1].get("mode") != requested:
        raise ValueError("No capture-time receipt for the requested visual diagnostic")
    return records[-1]


def validate_native(log: str, level: str, image: Path | None, expected: list[int],
                    settings: dict | None = None) -> None:
    if (f"[loading] committed {level}" not in log
            or "[renderer] wgpu | adapter:" not in log
            or "not a valid settings file" in log
            or "initial level preparation failed" in log
            or "PLACES_MOVE_SCRIPT: ignored" in log):
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
    parser.add_argument("--asset-root", type=Path, default=ROOT, help="Runtime root containing assets; supports preserved milestone snapshots")
    parser.add_argument("--manifest", type=Path, default=MANIFEST)
    parser.add_argument("--views", help="Comma-separated manifest camera names")
    parser.add_argument("--quality", choices=("low", "medium", "high"), default="high")
    parser.add_argument("--lightmaps", choices=("off", "medium", "full"))
    parser.add_argument("--filtering", choices=("low", "medium", "high"))
    parser.add_argument("--low-lighting", action="store_true", help="Existing independent Low lighting override")
    parser.add_argument("--diagnostic", help="Opt-in visual-diagnostics build mode; final keeps normal composition")
    parser.add_argument("--entity-light-trace", action="store_true", help="Existing actual entity payload/anchor trace")
    parser.add_argument("--finish-gpu", action="store_true", help="Existing benchmark GPU drain before swap; renderer completion timing, not GPU timestamps")
    parser.add_argument("--quality-cycle", help="Existing live settings script, e.g. 60:low,180:high")
    parser.add_argument("--graphics-cycle", help="Existing Advanced settings script, e.g. 60:lightmaps=off")
    parser.add_argument("--lighting-sequence", type=Path, help="Bounded native model movement and multi-capture JSON; requires --frames and one view")
    parser.add_argument("--capture-frame", type=int, help="Ready frame to capture instead of the fixed half-second")
    parser.add_argument("--frames", type=int, default=0, help="Measure frames after 120 warmup frames")
    parser.add_argument("--move-script", help="Existing held-control script; records actual player state")
    parser.add_argument("--fixed-delta", type=float, help="Explicit benchmark simulation step in (0, 0.1] seconds; real telemetry remains measured")
    parser.add_argument("--camera-position", help="Independent benchmark render eye x,y,z; the gameplay player remains at spawn")
    parser.add_argument("--follow-player-camera", action="store_true", help="Render actual controller yaw/pitch during movement instead of the manifest's fixed angles")
    parser.add_argument("--play", action="store_true")
    parser.add_argument("--weather-trace", action="store_true", help="Record active weather and reserved resource counters")
    parser.add_argument("--native-actions", type=Path, help="Existing bounded native load/settings acceptance script")
    args = parser.parse_args()
    if args.fixed_delta is not None and (not math.isfinite(args.fixed_delta) or not 0 < args.fixed_delta <= 0.1):
        parser.error("Fixed delta must be finite and in (0, 0.1] seconds")
    if args.camera_position:
        try:
            eye = [float(value) for value in args.camera_position.split(",")]
        except ValueError:
            parser.error("Camera position must contain three finite coordinates")
        if len(eye) != 3 or not all(math.isfinite(value) for value in eye):
            parser.error("Camera position must contain three finite coordinates")
    manifest = json.loads(args.manifest.read_text())
    try:
        views = select_views(manifest, args.views)
    except ValueError as error:
        parser.error(str(error))
    sequence = None
    if args.lighting_sequence:
        if not args.frames or len(views) != 1 or args.play:
            parser.error("Lighting sequences require --frames and exactly one view")
        sequence = json.loads(args.lighting_sequence.read_text())
        for step in sequence:
            if step.get("capture"):
                captured = (ROOT / step["capture"]).resolve()
                step["capture"] = str(captured)
                if captured.exists():
                    parser.error("Sequence captures must be new; preserve previous evidence")
                captured.parent.mkdir(parents=True, exist_ok=True)
    if (args.frames < 0 or (args.play and args.frames) or (not args.play and not args.out)
            or (args.capture_frame is not None and args.capture_frame < 1)):
        parser.error("Use --out for evidence, nonnegative --frames, or --play")
    asset_root = args.asset_root.resolve()
    if not (asset_root / "assets/catalog.json").is_file():
        parser.error("Asset root must contain assets/catalog.json")
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
                    catalog_sha256=digest(asset_root / "assets/catalog.json"),
                    asset_root=str(asset_root),
                    git_revision=subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT,
                                                         text=True).strip(),
                    git_diff_sha256=hashlib.sha256(subprocess.check_output(
                        ["git", "diff", "HEAD"], cwd=ROOT)).hexdigest())
    receipts = []
    with tempfile.TemporaryDirectory(prefix="places-art-style-hero-") as staging:
        root = Path(staging)
        if sequence is not None:
            staged_sequence = root / "lighting-sequence.json"
            staged_sequence.write_text(json.dumps(sequence, indent=2) + "\n")
        payload = root / "assets"
        payload.mkdir()
        for item in (asset_root / "assets").iterdir():
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
            if args.follow_player_camera:
                env.pop("PLACES_CAMERA")
            if args.move_script:
                env["PLACES_MOVE_SCRIPT"] = args.move_script
                if out:
                    env["PLACES_STATE_LOG"] = str(out / (view["name"] + "-state.csv"))
            if args.fixed_delta is not None:
                env["PLACES_BENCH_FIXED_DELTA_SECONDS"] = str(args.fixed_delta)
            if args.camera_position:
                env["PLACES_BENCH_CAMERA_POSITION"] = args.camera_position
            if args.diagnostic:
                env["PLACES_VISUAL_DIAGNOSTIC"] = args.diagnostic
            if args.native_actions:
                env["PLACES_BENCH_ACTIONS"] = str(args.native_actions.resolve())
                if out:
                    env["PLACES_LOAD_TRACE"] = str(out / (view["name"] + "-load.jsonl"))
            if args.weather_trace and out:
                env["PLACES_WEATHER_TRACE"] = str(out / (view["name"] + "-weather.csv"))
            if args.entity_light_trace:
                env["PLACES_ENTITY_LIGHT_TRACE"] = "all"
            if args.finish_gpu:
                env["PLACES_BENCH_FINISH"] = "1"
            if args.quality_cycle:
                env["PLACES_BENCH_QUALITY_CYCLE"] = args.quality_cycle
            if args.graphics_cycle:
                env["PLACES_BENCH_GRAPHICS_CYCLE"] = args.graphics_cycle
            if args.lighting_sequence:
                env["PLACES_BENCH_LIGHTING_SEQUENCE"] = str(staged_sequence)
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
            sequence_captures = []
            for step in sequence or []:
                if step.get("capture"):
                    captured = Path(step["capture"])
                    validate_native(log, manifest["level"], captured, manifest["capture"]["expected_drawable"])
                    sequence_captures.append(dict(frame=step["frame"], path=str(captured), sha256=digest(captured),
                                                  diagnostic_receipt=visual_receipt(log, args.diagnostic, captured)))
            diagnostic_receipt = visual_receipt(log, args.diagnostic, image) if image else None
            if digest(package) != identity["package_sha256"] or digest(binary) != identity["binary_sha256"]:
                raise RuntimeError("Binary/package changed during capture")
            receipts.append(dict(view=view, settings=settings, identity=identity,
                                 diagnostic=args.diagnostic, quality_cycle=args.quality_cycle,
                                 move_script=args.move_script,
                                 fixed_delta_seconds=args.fixed_delta, camera_position=args.camera_position,
                                 entity_light_trace=args.entity_light_trace,
                                 finish_gpu=args.finish_gpu,
                                 graphics_cycle=args.graphics_cycle, capture_frame=args.capture_frame,
                                 lighting_sequence=sequence, sequence_captures=sequence_captures,
                                 lighting_sequence_input=str(args.lighting_sequence.resolve()) if args.lighting_sequence else None,
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
