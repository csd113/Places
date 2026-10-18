#!/usr/bin/env python3
"""Opt-in native navigation acceptance using one compact, precompiled fixture.

The diagnostic navigation_goal action changes an actor's goal, never its pose.
Actors then use the ordinary live planner, local avoidance and collision mover.
This tool does not build, bake, package an app, or run in ordinary CI. Screenshots,
action plans, live route snapshots and timing evidence stay outside target/.
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
import shutil
import statistics
import subprocess
import time

ROOT = Path(__file__).resolve().parents[2]
LEVEL = "navigation_layers"
CASES = ("layers", "clearance", "door", "obstacle", "meeting", "unreachable", "performance")


def write(path: Path, value: object) -> None:
    path.write_text(json.dumps(value, indent=2, allow_nan=False) + "\n")


def digest(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


class Plan:
    def __init__(self) -> None:
        self.steps: list[dict] = []

    def action(self, ms: int, kind: str, **fields: object) -> None:
        self.steps.append(dict(after=f"ready:{LEVEL}", delay_ms=ms,
                               action=dict(kind=kind, **fields)))

    def goal(self, ms: int, actor: str, position: list[float]) -> None:
        self.action(ms, "navigation_goal", id=actor, position=position)

    def camera(self, ms: int, position: list[float], yaw: float, pitch: float = -12) -> None:
        self.action(ms, "player", position=position, yaw=yaw, pitch=pitch)

    def capture(self, ms: int, name: str) -> None:
        self.action(ms, "capture", name=name)

    def blocker(self, ms: int, blocked: bool) -> None:
        self.action(ms, "item_pose", id="movable_blocker",
                    position=[17, 0, 16.5 if blocked else 20.5], rotation=[0, 0, 0, 1])

    def finish(self, ms: int, *, quit: bool = True) -> list[dict]:
        # Captures and desktop tooling can steal focus. Restoring it exercises
        # the real clock gate; every retained capture also records that gate.
        for at in range(0, ms, 1000):
            self.action(at, "focus", focused=True)
        if quit:
            self.action(ms, "quit")
        result = sorted(self.steps, key=lambda row: row["delay_ms"])
        if not 1 <= len(result) <= 128 or any(row["delay_ms"] > 60_000 for row in result):
            raise ValueError("native action budget exceeded")
        if len(json.dumps(result).encode()) > 65_536:
            raise ValueError("native action byte budget exceeded")
        return result


def plan_for(case: str) -> list[dict]:
    p = Plan()
    if case == "layers":
        p.camera(10, [10.6, 1.6, 6.5], 40)
        p.goal(50, "climber", [3, 3, 4])
        p.goal(50, "upper_walker", [8, 3, 4.8])
        for index in range(35):
            p.capture(200 + index * 1000, f"route-{index:02}")
        p.camera(7200, [19.3, 4.6, 1.2], 235, -18)
        p.camera(19200, [8.5, 4.6, 1], 225)
        return p.finish(35_500)
    if case == "clearance":
        p.camera(10, [3, 1.6, 10.5], 110)
        p.goal(50, "small_walker", [8, 0, 11.4])
        # Test aperture clearance independently: the rejected wide actor would
        # otherwise park across the only entrance and legitimately block the
        # smaller actor. The separate meeting case exercises actor avoidance.
        p.goal(5000, "large_walker", [8, 0, 12])
        for index, at in enumerate((200, 1200, 2400, 4400, 6000, 8000, 11_000)):
            p.capture(at, f"clearance-{index}")
        return p.finish(11_300)
    if case == "door":
        p.camera(10, [14.3, 1.6, 10.5], 110)
        p.goal(50, "door_walker", [20.5, 0, 11.5])
        p.capture(300, "locked-start")
        p.capture(2200, "locked-hold")
        p.action(2400, "activate", id="door_unlock")
        p.capture(3000, "opening")
        p.capture(6500, "outbound-arrived")
        p.goal(6800, "door_walker", [13.5, 0, 11.5])
        p.action(6800, "activate", id="door_close_lock")
        p.camera(7000, [20, 1.6, 10.5], 250)
        p.capture(9000, "return-locked")
        p.capture(11_000, "return-held")
        p.action(11_200, "activate", id="door_unlock")
        p.capture(12_000, "reopened")
        p.capture(15_500, "return-arrived")
        return p.finish(15_800)
    if case == "obstacle":
        p.camera(10, [14.2, 1.6, 15.5], 110)
        p.goal(50, "obstacle_walker", [20.5, 0, 16.5])
        p.capture(300, "obstructed-start")
        p.capture(2200, "obstructed")
        p.capture(4200, "obstructed-held")
        p.blocker(4400, False)
        p.capture(5000, "cleared")
        p.capture(8500, "outbound-arrived")
        p.blocker(9000, True)
        p.goal(9000, "obstacle_walker", [13.5, 0, 16.5])
        p.camera(9200, [20, 1.6, 15.5], 250)
        p.capture(11_200, "live-reblocked")
        p.capture(13_200, "live-reblocked-held")
        p.blocker(13_400, False)
        p.capture(14_000, "cleared-again")
        p.capture(18_000, "return-arrived")
        return p.finish(18_300)
    if case == "meeting":
        p.camera(10, [5, 1.6, 20.4], 180, -20)
        p.goal(50, "meeting_east", [8, 0, 21.5])
        p.goal(50, "meeting_west", [2, 0, 21.5])
        for index in range(40):
            p.capture(150 + index * 250, f"meeting-{index:02}")
        return p.finish(10_300)
    if case == "unreachable":
        p.camera(10, [27.8, 1.6, 3.8], 315)
        p.goal(50, "unreachable_walker", [3, 0, 4])
        for index, at in enumerate((200, 1200, 2400, 4000, 6000, 8000, 10_000)):
            p.capture(at, f"unreachable-{index}")
        return p.finish(10_300)
    if case == "performance":
        # This run deliberately has no capture/video actions. Frame timing uses
        # real wall time, while a fixed simulation delta keeps workload explicit.
        p.camera(0, [10.6, 1.6, 6.5], 40)
        for actor, goal in (
            ("climber", [3, 3, 4]), ("upper_walker", [8, 3, 4.8]),
            ("small_walker", [8, 0, 11.4]), ("large_walker", [8, 0, 12]),
            ("door_walker", [20.5, 0, 11.5]), ("obstacle_walker", [20.5, 0, 16.5]),
            ("meeting_east", [8, 0, 21.5]), ("meeting_west", [2, 0, 21.5]),
            ("unreachable_walker", [3, 0, 4]),
        ):
            p.goal(0, actor, goal)
        return p.finish(1000, quit=False)
    raise ValueError(case)


def agent(sample: dict, id: str) -> dict:
    return next(row for row in sample["entity_simulation"]["agents"] if row["id"] == id)


def positions(samples: dict, id: str) -> list[list[float]]:
    return [agent(sample, id)["position"] for sample in samples.values()]


def check(checks: list[dict], name: str, passed: bool, **evidence: object) -> None:
    checks.append(dict(name=name, passed=bool(passed), **evidence))


def travel(points: list[list[float]]) -> float:
    return sum(math.dist(a, b) for a, b in zip(points, points[1:]))


def audit(case: str, samples: dict) -> list[dict]:
    checks: list[dict] = []
    if case == "performance":
        return checks
    check(checks, "captures observe the real playing clock", all(
        sample["state"] == "Playing" and sample["window_focused"] for sample in samples.values()))
    if case == "layers":
        climber = positions(samples, "climber")
        upper = positions(samples, "upper_walker")
        check(checks, "upper actor remains on its own stacked floor", all(abs(p[1] - 3) < .06 for p in upper), y=[p[1] for p in upper])
        check(checks, "lower actor physically reaches upper goal", math.dist(climber[-1], [3, 3, 4]) < .5, final=climber[-1])
        check(checks, "climb uses the sole real side-ramp route", travel(climber) > 28 and any(p[0] > 17 and 2.4 < p[1] < 2.92 for p in climber), trace_metres=travel(climber))
        check(checks, "no interpolation or shortcut through stacked floor", all(
            not (.08 < p[1] < 2.92) or (10.7 <= p[0] <= 18.8 and 2.2 <= p[2] <= 4.8) for p in climber))
        check(checks, "planned upper goal retains its elevation", any(
            agent(sample, "climber")["path_goal"] == [3.0, 3.0, 4.0] for sample in samples.values()))
    elif case == "clearance":
        small = positions(samples, "small_walker")
        large = positions(samples, "large_walker")
        check(checks, "small body traverses 0.8 metre aperture", math.dist(small[-1], [8, 0, 11.4]) < .5, final=small[-1])
        check(checks, "wide body never penetrates the narrow aperture", all(p[0] < 4.6 for p in large), x=[p[0] for p in large])
        check(checks, "rejected wide route reports incomplete", not agent(next(reversed(samples.values())), "large_walker")["path_complete"])
    elif case == "door":
        door = lambda name: next(row for row in samples[name]["doors"] if row["id"] == "lane_door")
        check(checks, "initial locked closed leaf blocks actual motion", door("locked-hold")["locked"] is True and door("locked-hold")["phase"] == "Closed" and abs(door("locked-hold")["angle_degrees"]) < 1 and agent(samples["locked-hold"], "door_walker")["position"][0] < 16.6)
        check(checks, "AI opens the live leaf after an unlock-only action", door("opening")["locked"] is False and door("opening")["phase"] in ("Opening", "Open") and abs(door("opening")["angle_degrees"]) > 0)
        check(checks, "AI-opened leaf allows outbound arrival", door("outbound-arrived")["phase"] == "Open" and abs(door("outbound-arrived")["angle_degrees"]) > 85 and math.dist(agent(samples["outbound-arrived"], "door_walker")["position"], [20.5, 0, 11.5]) < .5)
        check(checks, "closing and locking interrupts the return route", door("return-held")["locked"] is True and door("return-held")["phase"] == "Closed" and abs(door("return-held")["angle_degrees"]) < 1 and agent(samples["return-held"], "door_walker")["position"][0] > 17.4)
        check(checks, "AI reopens and recovers without actor reset", door("return-arrived")["locked"] is False and door("return-arrived")["phase"] == "Open" and abs(door("return-arrived")["angle_degrees"]) > 85 and math.dist(agent(samples["return-arrived"], "door_walker")["position"], [13.5, 0, 11.5]) < .5)
    elif case == "obstacle":
        pos = lambda name: agent(samples[name], "obstacle_walker")["position"]
        check(checks, "dynamic body blocks the initial route", pos("obstructed-held")[0] < 15.6)
        check(checks, "removing live body permits recovery and arrival", math.dist(pos("outbound-arrived"), [20.5, 0, 16.5]) < .5)
        check(checks, "return observes a body moved back into the lane", pos("live-reblocked-held")[0] > 18.4)
        check(checks, "second removal recovers without resetting actor", math.dist(pos("return-arrived"), [13.5, 0, 16.5]) < .5)
        authored = {entry[1]: entry[0] for entry in samples["obstructed-start"]["physics"]["authored"]}
        key = authored["movable_blocker"]
        blocker = lambda name: next(row for row in samples[name]["physics"]["bodies"] if row["key"] == key)
        check(checks, "obstruction is a real installed dynamic physics body", blocker("obstructed-start")["motion"] == "dynamic")
        check(checks, "authored dynamic obstruction starts dormant", blocker("obstructed-start")["dormant"] is True)
        check(checks, "clearance action changes the real physics pose", blocker("cleared")["pose"] != blocker("obstructed-start")["pose"])
        check(checks, "moved dynamic crate settles on the real parking floor",
              not blocker("cleared")["dormant"] and math.dist(blocker("cleared")["pose"]["position"], [17, 0, 20.5]) < .1)
        check(checks, "returned dynamic crate remains stable in the blocking lane",
              math.dist(blocker("live-reblocked-held")["pose"]["position"], [17, 0, 16.5]) < .1)
    elif case == "meeting":
        east, west = positions(samples, "meeting_east"), positions(samples, "meeting_west")
        minimum = min(math.dist(a, b) for a, b in zip(east, west))
        check(checks, "opposing actors pass and both reach their goals", math.dist(east[-1], [8, 0, 21.5]) < .5 and math.dist(west[-1], [2, 0, 21.5]) < .5, east=east[-1], west=west[-1])
        check(checks, "observed body separation respects both radii", minimum >= .57, minimum_metres=minimum)
        check(checks, "meeting creates actual lateral avoidance", max(abs(p[2] - 21.5) for p in east + west) > .15)
    elif case == "unreachable":
        points = positions(samples, "unreachable_walker")
        final = agent(next(reversed(samples.values())), "unreachable_walker")
        check(checks, "unreachable goal never snaps actor into another island", all(24.25 <= p[0] <= 28.75 and .25 <= p[2] <= 4.75 and abs(p[1]) < .05 for p in points))
        check(checks, "unreachable route remains explicitly incomplete", not final["path_complete"])
        check(checks, "unreachable recovery settles with bounded movement", math.dist(points[-1], points[-2]) < .1 and final["speed_mps"] < .1, final=points[-1], trace_metres=travel(points))
        check(checks, "unreachable retry does not freeze world time", final["clock"] > 6)
        check(checks, "unreachable retries stay bounded instead of running every frame",
              0 < final["path_queries"] <= max(3, math.ceil(final["clock"] * 3)),
              path_queries=final["path_queries"], simulation_seconds=final["clock"])
    return checks


def performance_receipt(run: Path, log: str, refresh_hz: float | None, events: list[dict]) -> dict:
    with (run / "frames.csv").open() as stream:
        rows = list(csv.DictReader(stream))
    if len(rows) != 1200:
        raise RuntimeError(f"native benchmark emitted {len(rows)} of 1200 measured frames")
    loops = [float(row["loop_ms"]) for row in rows]
    mean_loop = statistics.mean(loops)
    fps = 1000 / mean_loop
    modes = [line for line in log.splitlines()
             if line.startswith(("[renderer]", "[wgpu]")) and "present mode" in line]
    selected = re.findall(r"present mode:?\s+(\w+)", modes[-1]) if modes else []
    immediate = bool(selected) and selected[-1].lower() == "immediate"
    drawable = re.findall(r"^\[(?:window|renderer)\][^\n]*drawable[: ]+(\d+)x(\d+)", log, re.MULTILINE)
    # Surpassing the observed display refresh by a substantial margin proves
    # presentation is not imposing that refresh ceiling. A plateau is retained
    # as inconclusive, even if the request and actual mode both say Immediate.
    uncapped = immediate and refresh_hz is not None and fps > refresh_hz * 1.15
    final_events = [event for event in events if event["event"] == "benchmark_complete"]
    if not final_events:
        raise RuntimeError("native benchmark did not record completion")
    final = json.loads(final_events[-1]["detail"])
    agents = final.get("entity_simulation", {}).get("agents", [])
    states = {}
    for row in agents:
        states[row["state"]] = states.get(row["state"], 0) + 1
    return dict(average_fps=fps, measured_frames=len(rows), mean_loop_ms=mean_loop,
                p95_loop_ms=sorted(loops)[math.ceil(len(loops) * .95) - 1],
                actual_present_mode_lines=modes, actual_drawable_sizes=drawable,
                display_refresh_hz=refresh_hz, verified_uncapped=uncapped,
                throughput_conclusion="uncapped throughput observed" if uncapped else "inconclusive for throughput gains: display pacing not excluded",
                workload="compact furnished stacked-floor fixture; nine diagnostic goals dispatched, three authored body classes plus reference class, one dynamic body, one locked door; fixed 1/60 s simulation; High textures, Static lighting, reflections off; navigation file dumps and capture/video recorder disabled",
                actual_final_workload=dict(actors=len(agents), states=states,
                    retained_goals=sum(row.get("navigation_goal") is not None for row in agents),
                    complete_paths=sum(row["path_complete"] for row in agents),
                    moving_actors=sum(row["speed_mps"] > .05 for row in agents),
                    path_queries=sum(row["path_queries"] for row in agents),
                    physics_bodies=final.get("physics", {}).get("stats", {}).get("body_count"),
                    simulation_seconds=final.get("entity_simulation", {}).get("time")),
                mean_draw_calls=statistics.mean(int(row["draw_calls"]) for row in rows),
                mean_visible_triangles=statistics.mean(int(row["visible_indices"]) / 3 for row in rows),
                mean_update_ms=statistics.mean(float(row["update_ms"]) for row in rows),
                mean_swap_ms=statistics.mean(float(row["swap_ms"]) for row in rows))


def prepare(run: Path, package: Path, case: str, width: int, height: int) -> list[dict]:
    run.mkdir()
    (run / "levels").mkdir()
    shutil.copy2(package, run / "levels" / f"{LEVEL}.placesmap")
    write(run / "settings.json", dict(window_width=width, window_height=height,
        window_mode="windowed", quality="high", lightmaps="static", reflections="off",
        bloom=True, vsync=False, developer_console=True))
    steps = plan_for(case)
    write(run / "actions.json", steps)
    return steps


def run_case(binary: Path, package: Path, out: Path, case: str,
             steps: list[dict], refresh_hz: float | None) -> dict:
    run = out / case
    env = {key: value for key, value in os.environ.items()
           if not key.startswith("PLACES_") and key != "SDL_VIDEODRIVER"}
    env.update(PLACES_ASSET_ROOT=str(ROOT), PLACES_STATE_ROOT=str(run), PLACES_LEVEL=LEVEL,
               PLACES_BENCH="1", PLACES_BENCH_FRAMES="1200" if case == "performance" else "0",
               PLACES_VERBOSE="1", PLACES_VSYNC="off", PLACES_BENCH_ACTIONS=str(run / "actions.json"),
               PLACES_LOAD_TRACE=str(run / "trace.jsonl"))
    if case == "performance":
        env.update(PLACES_BENCH_WARMUP="120", PLACES_BENCH_FIXED_DELTA_SECONDS="0.016666667",
                   PLACES_BENCH_OUT=str(run / "frames.csv"))
    else:
        env["PLACES_NAV_DEBUG"] = str(run / "nav")
    write(run / "environment.json", {key: value for key, value in env.items() if key.startswith("PLACES_")})
    started = time.monotonic()
    with (run / "native.log").open("w") as log:
        child = subprocess.Popen([str(binary)], cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT)
        write(run / "launch.json", dict(binary=str(binary), binary_sha256=digest(binary),
              package=str(package), package_sha256=digest(package), pid=child.pid,
              launch_method="direct", started_utc=datetime.now(timezone.utc).isoformat()))
        # No timeout/relaunch can orphan a native permission wait.
        while child.poll() is None:
            try:
                child.wait(timeout=1)
            except subprocess.TimeoutExpired:
                pass
    events, samples, parse_errors = [], {}, []
    trace = run / "trace.jsonl"
    if trace.is_file():
        for line_number, line in enumerate(trace.read_text().splitlines(), 1):
            try:
                event = json.loads(line)
                events.append(event)
                if event["event"].startswith("native_capture:"):
                    samples[event["event"].split(":", 1)[1]] = json.loads(event["detail"])
            except (KeyError, TypeError, ValueError) as error:
                parse_errors.append(f"trace line {line_number}: {type(error).__name__}: {error}")
    else:
        parse_errors.append("native trace was not created")
    write(run / "snapshots.json", samples)
    log = (run / "native.log").read_text()
    expected_captures = [step["action"]["name"] for step in steps if step["action"]["kind"] == "capture"]
    missing_captures = [name for name in expected_captures if name not in samples]
    checks, audit_error = [], None
    if missing_captures:
        check(checks, "scenario captures are available for movement audit", False,
              missing=missing_captures)
    else:
        try:
            checks.extend(audit(case, samples))
        except (KeyError, StopIteration, TypeError, ValueError) as error:
            audit_error = f"{type(error).__name__}: {error}"
            check(checks, "live snapshots contain the movement audit fields", False, error=audit_error)
    check(checks, "direct process shuts down cleanly", child.returncode == 0 and bool(events) and events[-1]["event"] == "shutdown_complete")
    executed = set()
    for event in events:
        if event.get("event") == "action":
            try:
                executed.add(json.loads(event["detail"])["id"])
            except (KeyError, TypeError, ValueError) as error:
                parse_errors.append(f"action detail: {type(error).__name__}: {error}")
    check(checks, "native trace records decode completely", not parse_errors, errors=parse_errors)
    check(checks, "all bounded native actions execute", executed == set(range(len(steps))),
          executed=sorted(executed), expected=len(steps))
    check(checks, "no native validation or diagnostic-goal errors", not any(token in log.lower() for token in ("panicked", "validation error", "fatal device", "catalog hash mismatch", "diagnostic goal refused")))
    identity = json.loads((out / "identity.json").read_text())
    check(checks, "binary and package retain their recorded identities", digest(binary) == identity["binary_sha256"] and digest(package) == identity["package_sha256"])
    presented = sum(event["event"] == "present" and event["detail"] == "ready" for event in events)
    skipped = sum(event["event"] == "surface_not_presented" and event["detail"] == "ready" for event in events)
    check(checks, "ready scene is actually presented", presented > 0 and skipped == 0, presented=presented, skipped=skipped)
    images = {}
    for step in steps:
        action = step["action"]
        if action["kind"] != "capture":
            continue
        name = action["name"]
        image = run / f"{name}.png"
        header = image.read_bytes()[:24] if image.is_file() else b""
        valid = len(header) == 24 and header[:8] == b"\x89PNG\r\n\x1a\n" and name in samples
        check(checks, f"{name}: native image and live snapshot exist", valid)
        if valid:
            images[name] = dict(sha256=digest(image), drawable=[int.from_bytes(header[16:20], "big"), int.from_bytes(header[20:24], "big")])
    result = dict(checks=checks, captures=images, elapsed_seconds=time.monotonic() - started,
                  native_returncode=child.returncode, missing_captures=missing_captures,
                  audit_error=audit_error, trace_parse_errors=parse_errors,
                  final_trace_events=[event.get("event") for event in events[-8:]],
                  native_identity=[line for line in log.splitlines() if line.startswith(("[renderer]", "[window]", "[wgpu] present mode"))])
    if case == "performance":
        try:
            result["performance"] = performance_receipt(run, log, refresh_hz, events)
            check(checks, "performance run records actual mode and drawable", bool(result["performance"]["actual_present_mode_lines"]) and bool(result["performance"]["actual_drawable_sizes"]))
        except (OSError, KeyError, TypeError, ValueError, RuntimeError) as error:
            result["performance_error"] = f"{type(error).__name__}: {error}"
            check(checks, "performance evidence is available", False, error=result["performance_error"])
    write(run / "receipt.json", result)
    return result


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--package", type=Path, required=True, help="existing Static package compiled from navigation_layers.json")
    parser.add_argument("--out", type=Path, required=True, help="new evidence directory outside target/")
    parser.add_argument("--mode", choices=(*CASES, "all"), default="all")
    parser.add_argument("--size", default="800x450", help="logical window size; actual drawable retained")
    parser.add_argument("--display-refresh-hz", type=float, help="verified physical display refresh, used only to exclude pacing caps")
    parser.add_argument("--prepare-only", action="store_true", help="retain plans/settings and do not launch native")
    args = parser.parse_args()
    size = re.fullmatch(r"(\d+)x(\d+)", args.size)
    if size is None or not all(64 <= int(v) <= 4096 for v in size.groups()):
        parser.error("size must be WIDTHxHEIGHT with dimensions 64..4096")
    if args.display_refresh_hz is not None and not 1 <= args.display_refresh_hz <= 1000:
        parser.error("display refresh must be 1..1000 Hz")
    binary, package, out = args.binary.resolve(strict=True), args.package.resolve(strict=True), args.out.resolve()
    if ROOT / "target" in out.parents:
        parser.error("evidence must survive cargo clean; choose an output outside target/")
    out.mkdir(parents=True, exist_ok=False)
    write(out / "identity.json", dict(binary=str(binary), binary_sha256=digest(binary),
          package=str(package), package_sha256=digest(package),
          source=str(ROOT / "tests/fixtures/levels/navigation_layers.json"),
          source_sha256=digest(ROOT / "tests/fixtures/levels/navigation_layers.json")))
    modes = CASES if args.mode == "all" else (args.mode,)
    results = {}
    for case in modes:
        steps = prepare(out / case, package, case, *map(int, size.groups()))
        if args.prepare_only:
            print(json.dumps(dict(mode=case, actions=len(steps), prepared=True)), flush=True)
            continue
        result = run_case(binary, package, out, case, steps, args.display_refresh_hz)
        results[case] = result
        write(out / "receipt.json", results)
        failed = [row["name"] for row in result["checks"] if not row["passed"]]
        print(json.dumps(dict(mode=case, failed=failed,
              performance=result.get("performance"))), flush=True)
    failed = {case: [row["name"] for row in result["checks"] if not row["passed"]]
              for case, result in results.items() if any(not row["passed"] for row in result["checks"])}
    if failed:
        raise SystemExit(f"Native checks failed: {failed}; inspect {out / 'receipt.json'}")


if __name__ == "__main__":
    main()
