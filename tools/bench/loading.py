#!/usr/bin/env python3
"""Measure already-built native startup with isolated cold/warm application caches.

Runs serially, preserves system caches, and records raw logs plus startup marks.
A cold run means an empty application cache, never a cold OS or GPU cache.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import re
import shutil
import signal
import subprocess
import time

ROOT = Path(__file__).resolve().parents[2]
MARK = re.compile(r"\[startup\]\s+(.+?): \+([\d.]+) ms \(([\d.]+) ms\)")


def trace_metrics(events: list[dict]) -> dict:
    """Summarize real trace observations; missing samples remain unavailable.

    Whole-process gaps include entry-to-first-observation, so initialization
    before the event loop cannot disappear from the reported maximum. Loading
    gaps retain the full observed interval when it intersects a load window;
    clipping at a request/ready boundary would hide a stall crossing it.

    The separately named metrics are:

    * ``binary_startup_ms`` — trace-relative time to the window-ready mark
      (``entry`` when window creation was not reached); the harness's
      process-seconds field is the wall-clock counterpart.
    * ``package_loading`` / ``loading_windows_ms`` — one window per request,
      from ``request`` to its completion, cancellation or failure.
    * ``first_usable_scene_ms`` — first ``scene_presented`` (the first correct
      scene the player could use), also kept as ``first_ready_present_ms``.
    * ``settings_transition`` / ``settings_transition_windows_ms`` — from a
      ``settings_change`` mark to its completion: an immediate
      ``settings_applied``, or the world ``gpu_ready``/``ready`` a rebuild
      required.
    * steady-state rendering is a different instrument: the ``BENCH_SUMMARY``
      and per-frame CSV produced by the release binary under ``PLACES_BENCH``
      (see ``tools/bench/bench_local.py``); this function never mixes a
      transition interval into a steady-state sample.
    """
    def summary(values):
        ordered = sorted(values)
        return {"count": len(ordered), **{
            label: ordered[math.ceil(len(ordered) * fraction) - 1] if ordered else None
            for label, fraction in [("p50_ms", .5), ("p95_ms", .95), ("p99_ms", .99), ("max_ms", 1)]}}

    cutoff = next((event["elapsed_ms"] for event in events
                   if event["event"] == "shutdown_requested"), None)
    active = [event for event in events if cutoff is None or event["elapsed_ms"] <= cutoff]
    origin = next((event["elapsed_ms"] for event in active if event["event"] == "entry"), None)
    windows = []
    pending = None
    for event in active:
        time_ms = event["elapsed_ms"]
        kind = event["event"]
        if kind == "request":
            if pending is not None:
                windows.append((pending[1], time_ms))
            pending = (event["request"], time_ms)
        elif pending is not None and event.get("request") == pending[0]:
            if kind in {"cancel", "failed", "scene_presented"} or (
                    kind == "present" and event.get("detail") == "ready"):
                windows.append((pending[1], time_ms))
                pending = None
    if pending is not None and active:
        windows.append((pending[1], cutoff if cutoff is not None else active[-1]["elapsed_ms"]))

    settings_windows = []
    settings_pending = None
    for event in active:
        kind = event["event"]
        if kind == "settings_change":
            if settings_pending is not None:
                settings_windows.append((settings_pending, event["elapsed_ms"]))
            settings_pending = event["elapsed_ms"]
        elif settings_pending is not None and kind in {
                "settings_applied", "gpu_ready", "ready", "failed", "cancel"}:
            settings_windows.append((settings_pending, event["elapsed_ms"]))
            settings_pending = None
    if settings_pending is not None and active:
        settings_windows.append((settings_pending, cutoff if cutoff is not None else active[-1]["elapsed_ms"]))

    first_usable = next((event["elapsed_ms"] for event in active if event["event"] == "scene_presented"), None)
    startup_ms = next((event["elapsed_ms"] for event in active if event["event"] == "window_ready"), None)
    if startup_ms is None:
        startup_ms = origin
    result = {"trace_available": bool(events), "loading_windows_ms": windows,
              "package_loading": summary([end - start for start, end in windows]),
              "binary_startup_ms": startup_ms,
              "first_present_ms": next((event["elapsed_ms"] for event in active if event["event"] == "present"), None),
              "first_ready_present_ms": first_usable,
              "first_usable_scene_ms": first_usable,
              "settings_transition_windows_ms": settings_windows,
              "settings_transition": summary([end - start for start, end in settings_windows])}
    for name, key in [("event_pump", "event_pump_gaps"), ("present", "present_gaps")]:
        times = [event["elapsed_ms"] for event in active if event["event"] == name]
        intervals = list(zip(times, times[1:]))
        if origin is not None and times:
            intervals.insert(0, (origin, times[0]))
        if any(end < start for start, end in intervals):
            raise ValueError(f"Non-monotonic {name} trace")
        result[key] = summary([end - start for start, end in intervals])
        result[key]["initial_wait_ms"] = times[0] - origin if origin is not None and times else None
        result[key]["includes_initial_interval"] = origin is not None and bool(times)
        loading = [end - start for start, end in intervals
                   if any(end > low and start < high for low, high in windows)]
        result[f"loading_{key}"] = summary(loading)
    latencies = [json.loads(event["detail"])["latency_ms"] for event in active if event["event"] == "action"]
    result["input_latency"] = summary(latencies)
    return result


def read_trace(path: Path) -> tuple[list[dict], str | None]:
    """Preserve diagnostics from an interrupted final JSON line without inventing data."""
    if not path.exists():
        return [], None
    events = []
    error = None
    for index, line in enumerate(path.read_text().splitlines(), 1):
        try:
            events.append(json.loads(line))
        except json.JSONDecodeError as failure:
            error = f"trace line {index}: {failure}"
            break
    return events, error


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--levels", nargs="+", default=["menu", "places_demo", "model_zoo", "level0_pit", "capacity_sparse", "capacity_dense"])
    parser.add_argument("--repeat", type=int, default=1)
    parser.add_argument("--timeout", type=float, default=1800)
    parser.add_argument("--workers", type=int, choices=(1, 2, 3), help="runtime chart workers; defaults to the bounded hardware choice")
    args = parser.parse_args()
    if args.repeat < 1:
        parser.error("--repeat must be positive")
    binary, root, out = args.binary.resolve(), args.root.resolve(), args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    report = {"binary": str(binary), "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
              "root": str(root), "platform": platform.platform(), "cache_definition": "application only; OS/GPU caches untouched", "runs": []}
    for level in args.levels:
        for repeat in range(args.repeat):
            state = out / f"{level}-{repeat}-state"
            (state / "levels").mkdir(parents=True)
            for source in [root / "levels/level0_pit.json", *sorted((root / "tests/fixtures/levels").glob("capacity_*.json"))]:
                if source.is_file():
                    shutil.copy2(source, state / "levels" / source.name)
            (state / "settings.json").write_text(json.dumps({"bindings": {"forward": "W", "backward": "S", "strafe_left": "A", "strafe_right": "D", "look_up": "UP", "look_down": "DOWN", "look_left": "LEFT", "look_right": "RIGHT"}, "window_mode": "windowed", "window_width": 640, "window_height": 360, "quality": "high", "vsync": True}))
            for cache in ("cold", "warm"):
                env = {k: v for k, v in os.environ.items() if not k.startswith("PLACES_")}
                env.update(PLACES_STATE_ROOT=str(state), PLACES_ASSET_ROOT=str(root / "assets"),
                           PLACES_VERBOSE="1", PLACES_BENCH="1", PLACES_BENCH_FRAMES="4", PLACES_BENCH_WARMUP="0",
                           PLACES_VSYNC="on", PLACES_QUALITY="high", PLACES_CAMERA="74,0")
                if level != "menu":
                    env["PLACES_LEVEL"] = level
                if args.workers is not None:
                    env["PLACES_LIGHTMAP_WORKERS"] = str(args.workers)
                trace = out / f"{level}-{repeat}-{cache}.jsonl"
                env["PLACES_LOAD_TRACE"] = str(trace)
                log = out / f"{level}-{repeat}-{cache}.log"
                command = [str(binary)]
                if platform.system() == "Darwin":
                    command = ["/usr/bin/time", "-l", *command]
                started = time.monotonic()
                with log.open("w") as stream:
                    with subprocess.Popen(command, cwd=root, env=env, stdout=stream,
                                          stderr=subprocess.STDOUT, start_new_session=os.name == "posix") as process:
                        try:
                            code = process.wait(timeout=args.timeout)
                        except subprocess.TimeoutExpired:
                            # The wrapper and game belong to this newly created group.
                            # Never leave an owned game running after timing out /usr/bin/time.
                            if os.name == "posix":
                                os.killpg(process.pid, signal.SIGTERM)
                            else:
                                process.terminate()
                            try:
                                process.wait(timeout=5)
                            except subprocess.TimeoutExpired:
                                if os.name == "posix":
                                    os.killpg(process.pid, signal.SIGKILL)
                                else:
                                    process.kill()
                                process.wait()
                            code = "timeout"
                elapsed = time.monotonic() - started
                text = log.read_text()
                marks = [{"name": m[1], "delta_ms": float(m[2]), "elapsed_ms": float(m[3])} for m in MARK.finditer(text)]
                rss = re.search(r"(\d+)\s+maximum resident set size", text)
                run = {"level": level, "repeat": repeat, "cache": cache, "command": command, "environment": {k: v for k, v in env.items() if k.startswith("PLACES_")},
                       "exit_code": code, "process_seconds": elapsed, "peak_rss_bytes": int(rss[1]) if rss else None, "requested_level_found": "PLACES_LEVEL: no level matches" not in text and "Requested level was not found" not in text, "startup": marks, "log": log.name}
                events, trace_error = read_trace(trace)
                worker_counts = re.findall(r"\[lightmaps\] fill workers=(\d+)", text)
                run["lightmap_workers"] = int(worker_counts[-1]) if worker_counts else None
                run["trace_error"] = trace_error
                run.update(trace_metrics(events))
                if events and level != "menu":
                    run["requested_level_found"] = run["requested_level_found"] and any(
                        event["event"] == "scene_presented" and event["detail"] == level for event in events)
                run["trace"] = trace.name if trace.exists() else None
                run["lifecycle"] = [event for event in events if event["event"] not in ["event_pump", "present"]]
                report["runs"].append(run)
                (out / "timings.json").write_text(json.dumps(report, indent=2) + "\n")
                print(f"{level} {cache}: {elapsed:.3f}s, exit={code}", flush=True)
                if code != 0 or trace_error is not None or not run["requested_level_found"]:
                    return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
