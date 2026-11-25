#!/usr/bin/env python3
"""Explicit native acceptance for the environment collection; never CI or baking.

Supply reviewed ordinary actions or use the current collection elevator
harness for built-in travel coverage. Review any custom room or quest route
against actual geometry before native use.
`run` imports an existing package into unique isolated state and launches the
supplied executable directly. Capture/video and uncapped timing are separate.
"""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import math
import os
from pathlib import Path
import signal
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[3]
ORDINARY = {"key", "mouse", "view", "view_at", "aim_entity", "aim_item", "focus", "capture", "quit"}


def write(path: Path, value) -> None:
    path.write_text(json.dumps(value, indent=2) + "\n")


def digest(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def bundled_package(asset_root: Path, package: Path) -> Path:
    """Find unchanged real bundle bytes, without installing a duplicate copy."""
    levels = asset_root / "levels"
    if levels.is_symlink() or not levels.is_dir():
        raise ValueError("--bundled requires a real asset-root/levels directory")
    package_hash = digest(package)
    for candidate in sorted(levels.rglob("*.placesmap")):
        relative = candidate.relative_to(levels)
        if any(levels.joinpath(*relative.parts[:index]).is_symlink()
               for index in range(1, len(relative.parts)+1)):
            continue
        if candidate.is_file() and digest(candidate) == package_hash:
            return candidate
    raise ValueError("--bundled requires supplied package bytes to match a real asset-root/levels/*.placesmap file")


class Walk:
    """Held input from an authored spawn; pos is planning only, never teleported."""

    def __init__(self, level: str, position, speed: float = 3.0):
        self.level = level
        self.pos = list(position)
        self.speed = speed
        self.ms = 350.0
        self.steps = []
        self.walking = False
        self.carrying = False

    def add(self, kind: str, **fields) -> None:
        self.steps.append(dict(after=f"ready:{self.level}", delay_ms=round(self.ms),
                               action=dict(kind=kind, **fields)))

    def pause(self, ms: float) -> None:
        self.ms += ms

    def focus(self) -> None:
        self.add("focus", focused=True)
        self.pause(350)

    def tap(self, key: str = "E") -> None:
        self.add("key", key=key, pressed=True)
        self.pause(55)
        self.add("key", key=key, pressed=False)
        self.pause(180)

    def click(self, button: str = "left") -> None:
        self.add("mouse", button=button, pressed=True)
        self.pause(55)
        self.add("mouse", button=button, pressed=False)
        self.pause(180)

    def view(self, yaw: float, pitch: float = -4) -> None:
        self.add("view", yaw=yaw, pitch=pitch)

    def aim(self, point) -> None:
        self.add("view_at", position=list(point))
        self.pause(180)

    def item(self, instance: str) -> None:
        self.add("aim_item", id=instance)
        self.pause(180)

    def entity(self, instance: str, yaw_offset: float = 0) -> None:
        self.add("aim_entity", id=instance, yaw_offset_degrees=yaw_offset)
        self.pause(180)

    def stop(self) -> None:
        if self.walking:
            self.add("key", key="W", pressed=False)
            self.walking = False
            self.pause(160)

    def move(self, x: float, z: float, stop: bool = True) -> None:
        dx, dz = x-self.pos[0], z-self.pos[1]
        if self.carrying:
            self.stop()
        self.view(math.degrees(math.atan2(dx, -dz)))
        if self.carrying:
            # Let the production collision-constrained hand follow a turn;
            # this does not prescribe the item's pose or bypass a blocked hand.
            self.pause(500)
        if not self.walking:
            self.add("key", key="W", pressed=True)
            self.walking = True
        self.pause(math.hypot(dx, dz)/self.speed*1000)
        self.pos = [x, z]
        if stop:
            self.stop()

    def capture(self, name: str) -> None:
        self.stop()
        self.add("capture", name=name)
        self.pause(180)

    def finish(self) -> list[dict]:
        self.stop()
        self.add("quit")
        return validate_steps(self.steps, ordinary=True)


def validate_steps(steps: list[dict], ordinary: bool) -> list[dict]:
    if not isinstance(steps, list) or not 1 <= len(steps) <= 128:
        raise ValueError("native scripts must contain 1..128 actions; split long routes")
    for step in steps:
        if not isinstance(step, dict) or not 0 <= step["delay_ms"] <= 60000:
            raise ValueError("action delay must be within 0..60000 ms per anchor")
        if ordinary and step["action"]["kind"] not in ORDINARY:
            raise ValueError(f"diagnostic action in ordinary route: {step['action']}")
    # Preserve file order for simultaneous events; the driver independently
    # waits for each first-occurrence level-ready anchor.
    return steps


WINDOW_INFO_SOURCE = r'''import Foundation
import CoreGraphics
let target = Int(CommandLine.arguments[1])!
let rows = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] ?? []
let windows = rows.filter { ($0[kCGWindowOwnerPID as String] as? Int) == target }
let data = try JSONSerialization.data(withJSONObject: windows, options: [.sortedKeys])
FileHandle.standardOutput.write(data)
'''


def prepare_window_info(out: Path) -> Path:
    out.mkdir(parents=True, exist_ok=False)
    source = out / "window_info.swift"
    source.write_text(WINDOW_INFO_SOURCE)
    binary = out / "window-info"
    subprocess.run(["/usr/bin/swiftc", "-module-cache-path", str(out / "module-cache"),
                    str(source), "-o", str(binary)], check=True)
    return binary


def window_for_pid(helper: Path, child: subprocess.Popen, out: Path) -> dict:
    for _ in range(100):
        rows = json.loads(subprocess.check_output([str(helper), str(child.pid)], text=True))
        windows = [row for row in rows if row.get("kCGWindowLayer") == 0
                   and row.get("kCGWindowBounds", {}).get("Width", 0) > 100]
        if windows:
            write(out / "window.json", windows[0])
            return windows[0]
        if child.poll() is not None:
            break
        time.sleep(.1)
    raise RuntimeError("No native window for launched game PID; see native.log")


def run(args) -> int:
    binary, package = args.binary.resolve(strict=True), args.package.resolve(strict=True)
    if any(part.endswith(".app") for part in binary.parts):
        raise ValueError("native acceptance requires a direct binary, not a .app")
    if args.performance and args.record:
        raise ValueError("WindowServer recording and performance must be separate runs")
    asset_root = args.asset_root.resolve(strict=True)
    bundled = bundled_package(asset_root, package) if args.bundled else None
    steps = json.loads(args.actions.read_text()) if args.actions else [
        dict(after=f"ready:{args.level}", delay_ms=0, action=dict(kind="focus", focused=True)),
        dict(after=f"ready:{args.level}", delay_ms=300, action=dict(kind="key", key="Return", pressed=True)),
        dict(after=f"ready:{args.level}", delay_ms=355, action=dict(kind="key", key="Return", pressed=False)),
    ]
    validate_steps(steps, ordinary=not args.diagnostic)
    if args.no_capture:
        if not args.record:
            raise ValueError("--no-capture is reserved for smooth WindowServer video")
        steps = [step for step in steps if step["action"]["kind"] != "capture"]
    if not args.performance and not any(step["action"]["kind"] == "quit" for step in steps):
        raise ValueError("non-performance scripts must end with an explicit quit")
    if args.performance and any(step["action"]["kind"] in {"capture", "quit"} for step in steps):
        raise ValueError("performance scripts cannot capture or quit before measured frames finish")
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    settings = dict(window_width=args.width, window_height=args.height, window_mode="windowed",
                    quality="high", texture_filtering="high", lightmaps=args.lightmaps, reflections="full",
                    bloom=True, vsync=False, developer_console=False, walk_speed=3, fov_degrees=60)
    write(out / "settings.json", settings)
    write(out / "actions.json", steps)
    if args.expectations and not args.no_capture:
        write(out / "expectations.json", json.loads(args.expectations.read_text()))
    env = {key: value for key, value in os.environ.items()
           if not key.startswith("PLACES_") and key != "SDL_VIDEODRIVER"}
    env.update(PLACES_ASSET_ROOT=str(asset_root), PLACES_STATE_ROOT=str(out),
               PLACES_LEVEL=args.level, PLACES_BENCH="1", PLACES_VERBOSE="1", PLACES_VSYNC="off",
               PLACES_BENCH_FRAMES=str(args.frames if args.performance else 0),
               PLACES_BENCH_ACTIONS=str(out / "actions.json"), PLACES_LOAD_TRACE=str(out / "trace.jsonl"))
    if args.level == "menu":
        env.pop("PLACES_LEVEL")
    if args.performance:
        env.update(PLACES_BENCH_WARMUP=str(args.warmup), PLACES_BENCH_OUT=str(out / "frames.csv"))
    else:
        env["PLACES_AUDIO_CAPTURE"] = "1"
    if args.spawn:
        if not args.diagnostic:
            raise ValueError("--spawn is diagnostic setup; label this run with --diagnostic")
        env["PLACES_SPAWN"] = args.spawn
    if args.camera:
        env["PLACES_CAMERA"] = args.camera
    write(out / "environment.json", {key: value for key, value in env.items() if key.startswith("PLACES_")})
    identity = dict(binary=str(binary), package=str(package), level=args.level,
                    asset_root=env["PLACES_ASSET_ROOT"], binary_sha256=digest(binary),
                    package_sha256=digest(package), package_bytes=package.stat().st_size,
                    started_utc=datetime.now(timezone.utc).isoformat(),
                    launch="direct native executable; verified bundled package" if bundled else "direct native executable; isolated package import",
                    import_mode="skipped: verified bundled custody" if bundled else "fresh isolated import",
                    import_skipped=bool(bundled),
                    bundled_custody=dict(file=str(bundled), sha256=digest(bundled), real_file=True) if bundled else None,
                    diagnostic_setup=args.diagnostic,
                    performance=args.performance, smooth_video=args.no_capture,
                    requested_refresh_hz=args.refresh_hz)
    write(out / "identity.json", identity)
    with (out / "import.log").open("w") as log:
        if bundled:
            log.write(f"Import skipped (--bundled). Verified real bundled file {bundled}\nSHA256 {identity['package_sha256']}\n")
            import_code = 0
        else:
            imported = subprocess.run([str(binary), "--import", str(package)], cwd=ROOT, env=env,
                                      stdout=log, stderr=subprocess.STDOUT, timeout=120)
            import_code = imported.returncode
    if import_code:
        write(out / "receipt.json", dict(**identity, stage="import", exit_code=import_code, shutdown=False))
        raise RuntimeError(f"Import failed; see {out / 'import.log'}")
    if sys.platform == "darwin" and args.performance:
        with (out / "display.json").open("w") as log:
            subprocess.run(["/usr/sbin/system_profiler", "SPDisplaysDataType", "-json"],
                           stdout=log, stderr=subprocess.STDOUT, timeout=30, check=True)
    helper = (args.window_info.resolve(strict=True) if args.window_info else prepare_window_info(out / "capture-helper")) if args.record else None
    recorder = None
    launch_error = None
    rss_samples = []
    rss_error = None
    started = time.monotonic()
    with (out / "native.log").open("w") as log:
        child = subprocess.Popen([str(binary)], cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT)
        write(out / "process.json", dict(pid=child.pid, binary=str(binary)))
        try:
            if args.record:
                window = window_for_pid(helper, child, out)
                recorder = subprocess.Popen(["/usr/sbin/screencapture", "-v", "-o",
                                             "-l"+str(window["kCGWindowNumber"]), str(out / "gameplay-os.mov")],
                                            stdout=(out / "recording.log").open("w"), stderr=subprocess.STDOUT)
                write(out / "recording-timing.json", dict(launch_to_recorder_seconds=time.monotonic()-started,
                      method="WindowServer recording of this direct game PID; no engine framebuffer readback; real-time motion"))
            # A bounded route timeout is not a retry. Terminate this exact child
            # and retain evidence if a route fails to finish.
            if args.performance:
                # Reuse frame_time.py's PID-only, once-per-second RSS sampling.
                # This is an observed sample peak, never an OS true high-water mark.
                while child.poll() is None:
                    if time.monotonic()-started >= args.timeout:
                        raise subprocess.TimeoutExpired([str(binary)], args.timeout)
                    if rss_error is None:
                        try:
                            sample = subprocess.run(["ps", "-o", "rss=", "-p", str(child.pid)],
                                                    capture_output=True, text=True, check=False, timeout=2)
                            if sample.returncode == 0 and sample.stdout.strip().isdigit():
                                rss_samples.append(dict(seconds=time.monotonic()-started,
                                                        rss_bytes=int(sample.stdout.strip())*1024))
                            elif sample.stderr.strip():
                                # A denied/unavailable diagnostic is retained and
                                # never retried while the native child keeps running.
                                rss_error = sample.stderr.strip()
                        except (OSError, subprocess.SubprocessError) as error:
                            rss_error = str(error)
                    try:
                        child.wait(timeout=1)
                    except subprocess.TimeoutExpired:
                        pass
            else:
                child.wait(timeout=args.timeout)
        except BaseException as error:
            launch_error = str(error)
            if child.poll() is None:
                child.terminate()
                try:
                    child.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    child.kill()
                    child.wait(timeout=10)
        finally:
            if recorder is not None:
                recorder.send_signal(signal.SIGINT)
                recorder.wait(timeout=30)
    events = [json.loads(line) for line in (out / "trace.jsonl").read_text().splitlines()] if (out / "trace.jsonl").exists() else []
    samples = {event["event"].split(":", 1)[1]: json.loads(event["detail"])
               for event in events if event["event"].startswith("native_capture:")}
    write(out / "snapshots.json", samples)
    focus_states = [json.loads(event["detail"]) for event in events
                    if event["event"] == "action_result"]
    ready_times = [event["elapsed_ms"]/1000 for event in events if event["event"] == "ready"]
    for sample in rss_samples:
        sample["phase"] = "ready world: warmup or measured frames" if ready_times and sample["seconds"] >= min(ready_times) else "startup/loading"
    native_memory = dict(method="PID-only ps RSS sampling",
                         nominal_interval_seconds=1, samples=rss_samples,
                         sampled_peak_rss_bytes=max((sample["rss_bytes"] for sample in rss_samples), default=None),
                         sampled_ready_peak_rss_bytes=max((sample["rss_bytes"] for sample in rss_samples
                                                          if sample["phase"].startswith("ready")), default=None),
                         os_true_peak_measured=False, sampling_error=rss_error,
                         scope="Selected map process RSS including startup/loading; sampled peak can miss brief transients. Not a separate GPU atlas/residency budget.") if args.performance else None
    receipt = dict(**identity, exit_code=child.returncode,
                   shutdown=bool(events) and events[-1]["event"] == "shutdown_complete",
                   seconds=time.monotonic()-started, captures=list(samples),
                   actual_focus={name: [sample.get("window_focused"), sample.get("window_native_focus")]
                                 for name, sample in samples.items()},
                   focused_action_results=sum(bool(state.get("window_focused") and state.get("window_native_focus")) for state in focus_states),
                   immutable_inputs=digest(binary) == identity["binary_sha256"] and digest(package) == identity["package_sha256"]
                       and (bundled is None or not bundled.is_symlink() and digest(bundled) == identity["bundled_custody"]["sha256"]),
                   recorder_exit_code=recorder.returncode if recorder is not None else None,
                   native_memory=native_memory,
                   launch_error=launch_error)
    write(out / "receipt.json", receipt)
    print(json.dumps(dict(out=str(out), exit_code=child.returncode, captures=list(samples))), flush=True)
    if launch_error:
        raise RuntimeError(f"Native run stopped: {launch_error}; retained receipt at {out}")
    return child.returncode


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    r = commands.add_parser("run", help="explicitly launch direct native player")
    r.add_argument("--binary", type=Path, required=True)
    r.add_argument("--package", type=Path, required=True)
    r.add_argument("--out", type=Path, required=True)
    r.add_argument("--level", default="hub")
    r.add_argument("--actions", type=Path)
    r.add_argument("--expectations", type=Path)
    r.add_argument("--asset-root", type=Path, default=ROOT / "assets")
    r.add_argument("--bundled", "--no-import", action="store_true",
                   help="skip import only when supplied bytes match a real package under asset-root/levels; final canonical smoke")
    r.add_argument("--lightmaps", choices=["static", "full"], default="full",
                   help="Static is for an explicitly labeled early diagnostic fixture only")
    r.add_argument("--diagnostic", action="store_true", help="explicitly allow pose/setup/activation diagnostics")
    r.add_argument("--spawn", help="diagnostic X,Y,Z,yaw setup; never natural route evidence")
    r.add_argument("--camera", help="fixed yaw,pitch for a representative performance view")
    r.add_argument("--performance", action="store_true")
    r.add_argument("--frames", type=int, default=1200)
    r.add_argument("--warmup", type=int, default=1200)
    r.add_argument("--refresh-hz", type=float, help="actual active display refresh, independently verified")
    r.add_argument("--width", type=int, default=640)
    r.add_argument("--height", type=int, default=360)
    r.add_argument("--timeout", type=float, default=180)
    r.add_argument("--record", action="store_true", help="macOS PID-only WindowServer recording, separate from timing")
    r.add_argument("--no-capture", action="store_true", help="omit PNG captures during smooth video; keep ordinary inputs and state traces")
    r.add_argument("--window-info", type=Path, help="existing compiled CoreGraphics PID window helper")
    args = parser.parse_args()
    if not 1 <= args.frames <= 120000 or not 0 <= args.warmup <= 120000:
        parser.error("invalid bounded benchmark frame counts")
    if not 64 <= args.width <= 8192 or not 64 <= args.height <= 8192 or args.timeout <= 0:
        parser.error("invalid window size or timeout")
    return run(args)


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
        print(f"Environment acceptance: {error}", file=sys.stderr)
        raise SystemExit(1) from error
