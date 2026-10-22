#!/usr/bin/env python3
"""Bounded native frame-time/capture run; requires an existing playable package.

No map baking or graphics quality reduction. Motion uses fixed simulation steps
to match workload, while timings use real wall time. Explicit diagnostic options
may drain the GPU, omit scene draws or discard presentation; these runs are
labeled separately and must not be reported as normal presented throughput.
"""
from __future__ import annotations

import argparse
import csv
from datetime import datetime, timezone
import hashlib
import json
import math
import os
from pathlib import Path
import re
import statistics
import subprocess
import sys
import time
import zipfile

ROOT = Path(__file__).resolve().parents[2]


def statistics_for(values: list[float]) -> dict | None:
    if not values:
        return None
    ordered = sorted(values)
    return {
        "samples": len(ordered),
        "mean": statistics.fmean(ordered),
        "median": statistics.median(ordered),
        "p95": ordered[max(0, math.ceil(len(ordered) * 0.95) - 1)],
        "p99": ordered[max(0, math.ceil(len(ordered) * 0.99) - 1)],
        "worst": ordered[-1],
    }


def lateral_script() -> str:
    """At 3 m/s, travel between Z approximately 1.6 and 4.0 in the door view."""
    holds = ["strafe_right@2-2.4"]
    for index in range(14):
        start = 2.4 + index * 0.8
        control = "strafe_left" if index % 2 == 0 else "strafe_right"
        holds.append(f"{control}@{start:.3f}-{start + 0.8:.3f}")
    return ",".join(holds)


def digest(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def package_manifest_identity(package: Path, level: str) -> str:
    """Match the runtime's member-manifest identity, including project archives."""
    with zipfile.ZipFile(package) as archive:
        def metadata(name: str) -> bytes:
            if archive.getinfo(name).file_size > 2 * 1024 ** 2:
                raise ValueError(f"package index/manifest exceeds the runtime bound: {name}")
            return archive.read(name)

        name = "manifest.json"
        if "project.json" in archive.namelist():
            project = json.loads(metadata("project.json"))
            members = [member for member in project["levels"] if member["id"] == level]
            if len(members) != 1:
                raise ValueError(f"expected one package member named {level}")
            name = members[0]["records"][name]
        raw = metadata(name)
        if json.loads(raw)["id"] != level:
            raise ValueError(f"package manifest does not describe {level}")
        return hashlib.sha256(raw).hexdigest()


def verify_runtime_identity(log: str, level: str, variant: str | None, expected: str) -> None:
    observed = re.findall(
        r"^\[loading\] package identity level=(\S+) variant=(\S+) sha256=([0-9a-f]{64}) source=",
        log, re.MULTILINE,
    )
    if not observed or any(
        member != level or digest != expected or (variant is not None and lighting != variant)
        for member, lighting, digest in observed
    ):
        raise RuntimeError(f"native package/member/lighting identity differs: {observed}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/release/places")
    parser.add_argument("--asset-root", type=Path, default=ROOT, help="package root containing assets; supports preserved baseline bundles")
    parser.add_argument("--out", type=Path, required=True, help="new isolated result directory")
    parser.add_argument("--level", default="retro_arcade_demo")
    parser.add_argument("--package", type=Path, help="existing project/single-map package to install into isolated state; filename need not match the member ID")
    parser.add_argument("--spawn", default="-1.6,1.6,2.8,90", help="X,Y,Z,yaw; use --spawn= for negative X")
    parser.add_argument("--camera", default="90,0", help="yaw,pitch in degrees")
    parser.add_argument("--eye", default="", help="fixed render-eye X,Y,Z independent of player physics")
    parser.add_argument("--size", default="1280x720", help="logical window size; actual drawable is recorded")
    parser.add_argument("--expected-drawable", help="reject a native run unless its drawable matches WIDTHxHEIGHT")
    parser.add_argument("--fullscreen", action="store_true")
    parser.add_argument("--console-open", action="store_true", help="enable/open the console through the SDL acceptance action path")
    parser.add_argument("--vsync", action="store_true", help="otherwise request Immediate presentation")
    parser.add_argument("--texture-quality", choices=("low", "medium", "high"), default="high", help="independent texture residency for matched timing")
    parser.add_argument("--texture-filtering", choices=("low", "medium", "high"), default="high", help="independent sampler setting for matched timing")
    parser.add_argument("--frames", type=int, default=600)
    parser.add_argument("--seconds", type=float, help="matched wall-clock ready interval (2..60 s), ending through a native quit action")
    parser.add_argument("--warmup", type=int, default=120)
    parser.add_argument("--motion", action="store_true", help="matched bathroom lateral trajectory")
    parser.add_argument("--move-script", default="", help="explicit held controls in simulation seconds")
    parser.add_argument("--interact", default="", help="authored interactable IDs at startup, e.g. restroom_entry")
    parser.add_argument("--graphics-cycle", default="", help="existing benchmark Advanced-settings cycle")
    parser.add_argument("--window-cycle", default="", help="diagnostic native window actions by frame: resize/minimize/restore/raise")
    parser.add_argument("--quality-cycle", default="", help="existing benchmark texture-quality cycle")
    parser.add_argument("--actions", type=Path, help="bounded native SDL action script")
    parser.add_argument("--lighting-sequence", type=Path, help="existing bounded entity/light/door/capture actions")
    parser.add_argument("--capture-frame", type=int, help="capture instead of measuring performance")
    parser.add_argument("--gpu-trace", action="store_true", help="three labelled raw GPU samples; diagnostic run")
    parser.add_argument("--no-caustics", action="store_true", help="same-build disabled-effect control")
    parser.add_argument("--no-occlusion", action="store_true", help="diagnostic only: retain frustum culling but disable conservative solid-triangle occlusion")
    parser.add_argument("--no-gpu-timing", action="store_true", help="same-build instrumentation control")
    parser.add_argument("--no-present", action="store_true", help="diagnostic only: render and discard acquired frames; no presented-FPS claim")
    parser.add_argument("--no-render", action="store_true", help="diagnostic only: cycle the native surface without scene/UI work")
    parser.add_argument("--finish", action="store_true", help="diagnostic only: wait for GPU completion before presentation/discard")
    parser.add_argument("--runtime-workers", type=int, choices=range(1, 9), help="bounded CPU pool; 1 runs inline")
    parser.add_argument("--low-lighting", action="store_true", help="independent Low lighting mode; preserve texture quality")
    parser.add_argument("--lightmaps", choices=("static", "full"), default="full", help="matched lighting control")
    parser.add_argument("--entity-light-trace", default="", help="capture-only entity lighting payload witness")
    parser.add_argument("--cpu-trace", action="store_true", help="three steady frames of CPU stage spans; diagnostic")
    parser.add_argument("--render-trace", action="store_true", help="bounded render preparation/acquisition/encode/submit spans; diagnostic")
    parser.add_argument("--surface-trace", action="store_true", help="periodic native surface/window and GPU resource state; diagnostic")
    parser.add_argument("--hide-models", default="", help="diagnostic model paths separated by semicolons; trailing * selects a prefix")
    parser.add_argument("--physics-log", action="store_true", help="bounded per-frame body/step counters")
    parser.add_argument("--physics-trace", action="store_true", help="item state snapshots in the load trace; diagnostic")
    parser.add_argument("--real-delta", action="store_true", help="use normal wall-clock gameplay delta for interaction/traversal reproduction")
    args = parser.parse_args()
    if not 1 <= args.frames <= 3600 or not 0 <= args.warmup <= 1200:
        parser.error("frames must be 1..3600; warmup must be 0..1200")
    if args.seconds is not None and (not math.isfinite(args.seconds) or not 2 <= args.seconds <= 60 or args.capture_frame is not None):
        parser.error("--seconds must be finite, 2..60 and cannot capture")
    if args.motion and args.move_script:
        parser.error("select --motion or --move-script")
    if args.motion and args.warmup != 120:
        parser.error("the matched lateral trajectory uses 120 warmup frames")
    size = re.fullmatch(r"(\d+)x(\d+)", args.size)
    if size is None:
        parser.error("size must be WIDTHxHEIGHT")
    width, height = map(int, size.groups())
    if not (64 <= width <= 8192 and 64 <= height <= 8192):
        parser.error("window dimensions must be 64..8192")
    expected_drawable = None
    if args.expected_drawable:
        expected = re.fullmatch(r"(\d+)x(\d+)", args.expected_drawable)
        if expected is None:
            parser.error("expected drawable must be WIDTHxHEIGHT")
        expected_drawable = tuple(map(int, expected.groups()))
        if not all(64 <= edge <= 16384 for edge in expected_drawable):
            parser.error("expected drawable dimensions must be 64..16384")
    binary = args.binary.resolve(strict=True)
    asset_root = args.asset_root.resolve(strict=True)
    package = (args.package or asset_root / "assets/levels" / f"{args.level}.placesmap").resolve(strict=True)
    manifest_identity = package_manifest_identity(package, args.level)
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    if args.package:
        (out / "levels").mkdir()
        (out / "levels" / package.name).symlink_to(package)
    settings = json.loads((ROOT / "tests/fixtures/native/retro-arcade-manifest.json").read_text())["settings"]
    settings.update(
        window_width=width, window_height=height,
        window_mode="fullscreen" if args.fullscreen else "windowed", vsync=args.vsync,
        quality=args.texture_quality, texture_filtering=args.texture_filtering, lightmaps=args.lightmaps, reflections="full",
        bloom=True, use_low_quality_lighting=args.low_lighting, fov_degrees=60, developer_console=args.console_open,
    )
    # A direct native child uses the same isolated state on every platform.
    settings_path = out / "settings-input.json"
    settings_path.write_text(json.dumps(settings, indent=2) + "\n")
    (out / "settings.json").write_text(json.dumps(settings, indent=2) + "\n")
    env = {key: value for key, value in os.environ.items()
           if not key.startswith("PLACES_") and key != "SDL_VIDEODRIVER"}
    env.update(
        PLACES_ASSET_ROOT=str(asset_root), PLACES_STATE_ROOT=str(out), PLACES_LEVEL=args.level,
        PLACES_BENCH="1", PLACES_VERBOSE="1", PLACES_SPAWN=args.spawn, PLACES_CAMERA=args.camera,
        PLACES_BENCH_WARMUP=str(args.warmup), PLACES_BENCH_FRAMES=str(args.frames),
        PLACES_BENCH_FIXED_DELTA_SECONDS="0.016666667", PLACES_BENCH_OUT=str(out / "frames.csv"),
        PLACES_STATE_LOG=str(out / "player.csv"), PLACES_LOAD_TRACE=str(out / "trace.jsonl"),
    )
    optional = {
        "PLACES_RUNTIME_WORKERS": str(args.runtime_workers) if args.runtime_workers else "",
        "PLACES_BENCH_CAMERA_POSITION": args.eye,
        "PLACES_ENTITY_LIGHT_TRACE": args.entity_light_trace,
        "PLACES_BENCH_CPU_TRACE": "1" if args.cpu_trace else "",
        "PLACES_BENCH_RENDER_TRACE": "1" if args.render_trace else "",
        "PLACES_GPU_RESOURCE_TRACE": "1" if args.surface_trace else "",
        "PLACES_BENCH_HIDE_MODELS": args.hide_models,
        "PLACES_MOVE_SCRIPT": lateral_script() if args.motion else args.move_script,
        "PLACES_INTERACT": args.interact,
        "PLACES_BENCH_GRAPHICS_CYCLE": args.graphics_cycle,
        "PLACES_BENCH_WINDOW_CYCLE": args.window_cycle,
        "PLACES_BENCH_QUALITY_CYCLE": args.quality_cycle,
        "PLACES_BENCH_LIGHTING_SEQUENCE": str(args.lighting_sequence.resolve(strict=True)) if args.lighting_sequence else "",
        "PLACES_BENCH_ACTIONS": str(args.actions.resolve(strict=True)) if args.actions else "",
        "PLACES_BENCH_OCCLUSION": "off" if args.no_occlusion else "",
        "PLACES_BENCH_GPU_TRACE": "1" if args.gpu_trace else "",
        "PLACES_BENCH_GPU_TIMING": "off" if args.no_gpu_timing else "",
        "PLACES_BENCH_NOSWAP": "1" if args.no_present else "",
        "PLACES_BENCH_NORENDER": "1" if args.no_render else "",
        "PLACES_BENCH_FINISH": "1" if args.finish else "",
        "PLACES_BENCH_CAUSTICS": "off" if args.no_caustics else "",
        "PLACES_PHYSICS_LOG": str(out / "physics.csv") if args.physics_log else "",
        "PLACES_PHYSICS_TRACE": "1" if args.physics_trace else "",
    }
    env.update({key: value for key, value in optional.items() if value})
    if args.real_delta:
        env.pop("PLACES_BENCH_FIXED_DELTA_SECONDS")
    if args.console_open and args.actions:
        parser.error("select --console-open or --actions")
    if args.console_open:
        actions_path = out / "console-actions.json"
        actions_path.write_text(json.dumps([
            dict(after=f"ready:{args.level}", delay_ms=0, action=dict(kind="key", key="`", pressed=True)),
            dict(after=f"ready:{args.level}", delay_ms=10, action=dict(kind="text", text="/help", submit=True)),
        ]) + "\n")
        env["PLACES_BENCH_ACTIONS"] = str(actions_path)

    if args.seconds is not None:
        timed_actions = json.loads(args.actions.read_text()) if args.actions else []
        if any(step["action"]["kind"] == "quit" for step in timed_actions):
            parser.error("timed runs own their quit action")
        timed_actions.append(dict(after=f"ready:{args.level}", delay_ms=math.ceil(args.seconds * 1000), action=dict(kind="quit")))
        timed_path = out / "timed-actions.json"
        timed_path.write_text(json.dumps(timed_actions, indent=2) + "\n")
        env.update(PLACES_BENCH_FRAMES="0", PLACES_BENCH_ACTIONS=str(timed_path))

    if args.capture_frame is not None:
        if not 1 <= args.capture_frame <= 2400:
            parser.error("capture frame must be 1..2400")
        env.update(PLACES_CAPTURE=str(out / "view.png"), PLACES_CAPTURE_FRAME=str(args.capture_frame),
                   PLACES_BENCH_FRAMES="3600")
    started_utc = datetime.now(timezone.utc).isoformat()
    started = time.monotonic()
    load_before = os.getloadavg()
    binary_sha256, package_sha256 = digest(binary), digest(package)
    command = [str(binary)]
    native_out = out / "native"
    native_out.mkdir()
    (out / "environment.json").write_text(json.dumps(
        {key: value for key, value in env.items() if key.startswith("PLACES_")}, indent=2) + "\n")
    # Execute the actual player, never a .app/LaunchServices wrapper. Do not retry
    # or time out a native permission wait: another launch would orphan the first.
    rss_samples = []
    rss_error = None
    usage = None
    with (native_out / "stdout.log").open("w") as stdout, (native_out / "stderr.log").open("w") as stderr:
        child = subprocess.Popen(command, cwd=ROOT, env=env, stdout=stdout, stderr=stderr)
        (native_out / "identity.json").write_text(json.dumps(dict(
            launch_method="direct", source_binary=str(binary), binary_sha256=binary_sha256,
            pid=child.pid, cwd=str(ROOT)), indent=2) + "\n")
        while True:
            if hasattr(os, "wait4"):
                pid, status, observed = os.wait4(child.pid, os.WNOHANG)
                if pid:
                    usage = observed
                    child.returncode = os.waitstatus_to_exitcode(status)
                    break
            elif child.poll() is not None:
                break
            try:
                sample = subprocess.run(["ps", "-o", "rss=", "-p", str(child.pid)],
                                        capture_output=True, text=True, check=False)
                if sample.returncode == 0 and sample.stdout.strip().isdigit():
                    rss_samples.append(dict(seconds=time.monotonic() - started,
                                            rss_bytes=int(sample.stdout.strip()) * 1024))
                elif sample.stderr.strip():
                    rss_error = sample.stderr.strip()
            except OSError as error:
                # Unavailable diagnostics must never abandon a live player.
                rss_error = str(error)
            time.sleep(1)
    (native_out / "launch.json").write_text(json.dumps(dict(
        returncode=child.returncode, seconds=time.monotonic() - started,
        native_rss_samples=rss_samples, rss_sampling_error=rss_error,
        peak_native_rss_bytes=max((row["rss_bytes"] for row in rss_samples), default=None)), indent=2) + "\n")
    log = "".join((native_out / name).read_text() for name in ("stdout.log", "stderr.log"))
    (out / "native.log").write_text(log)
    if (digest(binary), digest(package)) != (binary_sha256, package_sha256):
        raise RuntimeError(f"binary/package changed during native run; see {out / 'native.log'}")
    if child.returncode or f"[loading] committed {args.level}" not in log:
        raise RuntimeError(f"native run failed ({child.returncode}); see {out / 'native.log'}")
    # Discovery can prefer another same-ID bundled archive over --package.
    # Accept only the planned member and lighting payload, independent of ZIP layout.
    # Explicit action/settings scripts may deliberately change lighting variants.
    planned_variant = None if args.actions or args.graphics_cycle else "off" if args.lightmaps == "static" else "full"
    verify_runtime_identity(log, args.level, planned_variant, manifest_identity)
    if any(message in log.lower() for message in ("validation error", "panicked", "fatal device", "catalog hash mismatch")):
        raise RuntimeError(f"native validation failed; see {out / 'native.log'}")
    if (re.search(r"\[props\] \d+ models cached \([1-9]\d* failed\)", log)
            or "drawing the diagnostic sheet instead" in log):
        raise RuntimeError(f"native asset realization failed; see {out / 'native.log'}")
    if args.lighting_sequence:
        last_frame = args.capture_frame or args.frames + args.warmup
        for step in json.loads(args.lighting_sequence.read_text()):
            if step.get("capture") and step["frame"] <= last_frame:
                path = Path(step["capture"]).resolve()
                marker = f"capture_attempt ready_frame={step['frame']} path={path}"
                if marker not in log or not path.is_file():
                    raise RuntimeError(f"native sequence did not capture {path}; see {out / 'native.log'}")
    if expected_drawable is not None:
        drawables = [tuple(map(int, match)) for match in re.findall(
            r"^\[(?:renderer|window)\].*drawable[: ]+(\d+)x(\d+)", log, re.MULTILINE)]
        if not drawables or any(size != expected_drawable for size in drawables):
            raise RuntimeError(f"drawable mismatch: expected {expected_drawable}, observed {drawables}; see {out / 'native.log'}")
    events = [json.loads(line) for line in (out / "trace.jsonl").read_text().splitlines()]
    presented = sum(event["event"] == "present" and event["detail"] == "ready" for event in events)
    missed = sum(event["event"] == "surface_not_presented" and event["detail"] == "ready" for event in events)
    if not args.no_present and (not presented or missed):
        raise RuntimeError(f"native surface was occluded/unavailable: {presented} ready, {missed} skipped")
    with (out / "frames.csv").open() as stream:
        rows = list(csv.DictReader(stream))
    capture = args.capture_frame is not None
    if capture:
        if not (out / "view.png").is_file():
            raise RuntimeError("missing native capture")
        if not rows or int(rows[-1]["draw_calls"]) == 0:
            raise RuntimeError(f"native capture contains no submitted scene geometry; see {out / 'native.log'}")
    elif (args.seconds is None and len(rows) != args.frames) or not rows or (not args.no_render and not all(int(row["draw_calls"]) > 0 for row in rows)):
        raise RuntimeError("missing measured frames or real scene submissions")
    gpu = {}
    for row in rows:
        if row["gpu_sample_id"] and row["gpu_render_ms"]:
            gpu[row["gpu_sample_id"]] = float(row["gpu_render_ms"])
    with (out / "player.csv").open() as stream:
        player = [list(map(float, row))[1:4] for row in csv.reader(stream) if float(row[2]) != 0]
    metrics = None if capture else {
        key: statistics_for([float(row[key]) for row in rows])
        for key in ("loop_ms", "frame_ms", "update_ms", "render_ms", "swap_ms")
    }
    receipt = dict(
        kind="capture" if capture else "diagnostic" if (args.cpu_trace or args.render_trace or args.surface_trace or args.hide_models or args.no_present or args.no_render or args.no_occlusion or args.finish or args.gpu_trace or args.graphics_cycle or args.window_cycle or args.quality_cycle or args.lighting_sequence or args.physics_trace or args.actions) else "timing",
        requested_ready_seconds=args.seconds, warmup_frames=args.warmup,
        started_utc=started_utc, elapsed_seconds=time.monotonic() - started,
        binary=str(binary), binary_sha256=binary_sha256, package=str(package), package_sha256=package_sha256,
        member_manifest_sha256=manifest_identity,
        loaded_package_identities=[line for line in log.splitlines()
                                   if line.startswith("[loading] package identity ")],
        asset_root=str(asset_root), settings=settings, spawn=args.spawn, camera=args.camera,
        expected_drawable=expected_drawable,
        environment={key: value for key, value in env.items() if key.startswith("PLACES_")},
        load_before=load_before, load_after=os.getloadavg(), presented_ready=presented, missed_ready=missed,
        native_identity=[line for line in log.splitlines()
                         if line.startswith(("[renderer]", "[window]", "[settings]", "[wgpu] GPU timing", "[wgpu] present mode"))],
        metrics=metrics, gpu_elapsed_ms=None if capture else statistics_for(list(gpu.values())),
        average_fps=None if capture or args.no_present else 1000 / statistics.fmean(float(row["loop_ms"]) for row in rows),
        render_iterations_per_second=(1000 / statistics.fmean(float(row["loop_ms"]) for row in rows)
                                      if args.no_present and not capture else None),
        process_usage=None if usage is None else dict(
            user_seconds=usage.ru_utime, system_seconds=usage.ru_stime,
            peak_rss_bytes=usage.ru_maxrss if sys.platform == "darwin" else usage.ru_maxrss * 1024,
            scope="whole native process including load; CPU seconds include worker threads"),
        peak_native_rss_bytes=json.loads((native_out / "launch.json").read_text())["peak_native_rss_bytes"],
        base_scene_draws=statistics_for([int(row["draw_calls"]) for row in rows]),
        base_scene_triangles=statistics_for([int(row["visible_indices"]) // 3 for row in rows]),
        reflection_passes=sorted({int(row["reflection_passes"]) for row in rows}),
        player_bounds=[[min(values), max(values)] for values in zip(*player)] if player else None,
        capture_sha256=digest(out / "view.png") if capture else None,
    )
    (out / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
    print(json.dumps(dict(out=str(out), kind=receipt["kind"], metrics=metrics, gpu_elapsed_ms=receipt["gpu_elapsed_ms"])))


if __name__ == "__main__":
    main()
