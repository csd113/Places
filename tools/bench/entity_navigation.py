#!/usr/bin/env python3
"""Opt-in entity acceptance with existing packages and a direct native binary.

No build, bake, bundle or frame-throughput measurement. Settings, action plans,
native images, snapshots and identities are retained for visual review.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[2]
OFFICE = "office_demo"
HOME = "home_showcase"


def write(path, value):
    path.write_text(json.dumps(value, indent=2, allow_nan=False) + "\n")


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


class Plan:
    def __init__(self):
        self.steps = []

    def action(self, ms, kind, after=OFFICE, **fields):
        self.steps.append({"after": f"ready:{after}", "delay_ms": ms,
                           "action": {"kind": kind, **fields}})

    def tap(self, ms, key):
        self.action(ms, "key", key=key, pressed=True)
        self.action(ms + 60, "key", key=key, pressed=False)

    def capture(self, ms, name, **fields):
        self.action(ms, "capture", name=name, **fields)

    def command(self, ms, text):
        self.tap(ms, "`")
        self.action(ms + 160, "text", text=text, submit=True)
        self.action(ms + 320, "escape")


def plan_for(mode):
    p = Plan()
    p.action(0, "focus", focused=True)
    if mode == "clock":
        p.action(50, "player", position=[8.5, 1.6, 11], yaw=0, pitch=-13)
        p.capture(700, "moving")
        p.action(900, "aim_entity", id="basement_mannequin")
        p.tap(1050, "F")
        p.capture(1600, "lit")
        p.capture(2200, "lit-held")
        p.tap(2400, "F")
        p.capture(3200, "released")
        p.command(3500, "/time scale 0")
        p.capture(4200, "stopped")
        p.action(4400, "key", key="W", pressed=True)
        p.action(4850, "key", key="W", pressed=False)
        p.capture(5300, "stopped-player-moved")
        p.command(5600, "/time scale 1")
        p.action(6100, "focus", focused=True)
        p.capture(6600, "resumed")
        p.action(6700, "focus", focused=True)
        p.tap(6800, "Escape")
        p.capture(7200, "paused")
        p.capture(8000, "paused-held")
        p.action(8100, "focus", focused=True)
        p.tap(8200, "Escape")
        p.action(8800, "focus", focused=True)
        p.capture(9200, "unpaused")
        p.capture(11000, "rest-or-moving")
        p.action(11300, "quit")
    elif mode == "shelter":
        p.action(50, "player", position=[3.5, 1.6, 2], yaw=270, pitch=-72)
        p.capture(700, "retreat")
        p.capture(4200, "sheltered")
        p.action(4400, "player", position=[7, 1.6, 5], yaw=0, pitch=-10)
        p.action(13000, "focus", focused=True)
        p.capture(14000, "hidden-hold")
        p.capture(18000, "emerging")
        p.capture(23000, "returned")
        # Native image/tool windows can take desktop focus during this longer
        # hold. Restore it periodically so wall-time actions exercise enough
        # actual simulation time; the snapshots retain the observed clocks.
        for ms in range(1000, 23001, 1000):
            p.action(ms, "focus", focused=True)
        p.action(23500, "quit")
    elif mode == "reload":
        p.capture(800, "office-before")
        p.action(1200, "load", level=HOME)
        p.capture(900, "home-environment", after=HOME)
        p.action(1500, "load", after=HOME, level=OFFICE)
        p.capture(8000, "office-reloaded", after=HOME)
        p.action(8500, "quit", after=HOME)
    return sorted(p.steps, key=lambda step: (step["after"], step["delay_ms"]))


def agent(sample, name="basement_mannequin"):
    return next(item for item in sample["entity_simulation"]["agents"] if item["id"] == name)


def audit(mode, samples):
    checks = {}
    if mode == "clock":
        lit, held = (agent(samples[name]) for name in ("lit", "lit-held"))
        checks["flashlight_holds_position"] = lit["light_held"] and held["light_held"] and math.dist(lit["position"], held["position"]) < .001
        checks["held_uses_rest_pose"] = "pose_stand" in held["pose"] and held["speed_mps"] == 0
        eye = samples["lit-held"]["player_position"]
        desired_yaw = math.degrees(math.atan2(eye[0] - held["position"][0], eye[2] - held["position"][2]))
        checks["held_looks_toward_player"] = abs((held["yaw_degrees"] - desired_yaw + 180) % 360 - 180) < 2
        checks["beam_release_restores_locomotion"] = not agent(samples["released"])["light_held"]
        stopped, later = (samples[name] for name in ("stopped", "stopped-player-moved"))
        checks["zero_world_time_freezes_agents"] = stopped["entity_simulation"] == later["entity_simulation"]
        checks["player_moves_while_world_stopped"] = math.dist(stopped["player_position"], later["player_position"]) > .3
        checks["positive_time_resumes_agents"] = samples["resumed"]["entity_simulation"]["time"] > later["entity_simulation"]["time"]
        checks["pause_preserves_entities"] = samples["paused"]["entity_simulation"] == samples["paused-held"]["entity_simulation"]
        checks["pause_state_observed"] = samples["paused"]["state"] == samples["paused-held"]["state"] == "Paused"
        checks["unpause_advances_entities"] = samples["unpaused"]["entity_simulation"]["time"] > samples["paused-held"]["entity_simulation"]["time"]
        checks["authored_move_clip_retained"] = any("run_arms_forward" in agent(sample)["pose"] for sample in samples.values())
        checks["basement_only"] = all(abs(item["position"][1]) < .1 for sample in samples.values() for item in sample["entity_simulation"]["agents"])
    elif mode == "shelter":
        roach = "basement_roach_0"
        hidden = agent(samples["sheltered"], roach)
        hold = agent(samples["hidden-hold"], roach)
        returned = agent(samples["returned"], roach)
        checks["physical_shelter_before_hiding"] = hidden["state"] == "hidden" and hidden["opacity"] == 0 and math.dist(hidden["position"], [2.8, 0, 2]) > .2
        checks["bounded_hidden_hold"] = hold["state"] == "hidden" and 0 < hold["hidden_remaining"] < hidden["hidden_remaining"]
        checks["visible_return"] = returned["opacity"] == 1 and returned["state"] != "hidden" and math.dist(returned["position"], hidden["position"]) > .3
    else:
        before, home, after = (samples[name] for name in ("office-before", "home-environment", "office-reloaded"))
        checks["other_environment_installed"] = home["current_level_id"] == HOME and home["renderer_level_id"] == HOME
        checks["office_reinstalled"] = after["current_level_id"] == OFFICE and after["renderer_level_id"] == OFFICE
        checks["fresh_agent_roster"] = [item["id"] for item in before["entity_simulation"]["agents"]] == [item["id"] for item in after["entity_simulation"]["agents"]]
        checks["reloaded_agents_live"] = all(item["clock"] > 0 and item["state"] not in ("scripted", "caught") for item in after["entity_simulation"]["agents"])
    return checks


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--mode", choices=("clock", "shelter", "reload", "all"), default="all")
    args = parser.parse_args()
    binary, out = args.binary.resolve(strict=True), args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    identity = {"binary": str(binary), "binary_sha256": digest(binary), "packages": {
        level: digest(ROOT / "assets/levels" / f"{level}.placesmap") for level in (OFFICE, HOME)}}
    write(out / "identity.json", identity)
    modes = ("clock", "shelter", "reload") if args.mode == "all" else (args.mode,)
    results = {}
    for mode in modes:
        run = out / mode
        run.mkdir()
        settings = {"window_width": 640, "window_height": 360, "window_mode": "windowed",
                    "quality": "high", "lightmaps": "full", "reflections": "off",
                    "bloom": True, "vsync": True, "developer_console": True}
        write(run / "settings.json", settings)
        steps = plan_for(mode)
        assert len(steps) <= 128, "bounded native action budget"
        write(run / "actions.json", steps)
        trace = run / "trace.jsonl"
        env = {key: value for key, value in os.environ.items() if not key.startswith("PLACES_") and key != "SDL_VIDEODRIVER"}
        env.update(PLACES_ASSET_ROOT=str(ROOT), PLACES_STATE_ROOT=str(run), PLACES_LEVEL=OFFICE,
                   PLACES_BENCH="1", PLACES_BENCH_FRAMES="0", PLACES_VERBOSE="1", PLACES_VSYNC="on",
                   PLACES_BENCH_ACTIONS=str(run / "actions.json"), PLACES_LOAD_TRACE=str(trace),
                   PLACES_NAV_DEBUG=str(run / "nav"))
        with (run / "native.log").open("w") as log:
            result = subprocess.run([str(binary)], cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT, timeout=75, check=False)
        events = [json.loads(line) for line in trace.read_text().splitlines()]
        samples = {event["event"].split(":", 1)[1]: json.loads(event["detail"])
                   for event in events if event["event"].startswith("native_capture:")}
        write(run / "snapshots.json", samples)
        checks = audit(mode, samples)
        checks["direct_process_clean_shutdown"] = result.returncode == 0 and events[-1]["event"] == "shutdown_complete"
        checks["all_actions_executed"] = {json.loads(event["detail"])["id"] for event in events if event["event"] == "action"} == set(range(len(steps)))
        presented = sum(event["event"] == "present" and event["detail"] == "ready" for event in events)
        missed = sum(event["event"] == "surface_not_presented" and event["detail"] == "ready" for event in events)
        checks["ready_frames_presented"] = presented > 0
        log_text = (run / "native.log").read_text()
        checks["no_native_validation_errors"] = not any(token in log_text.lower() for token in ("panicked", "validation error", "fatal device", "catalog hash mismatch"))
        images = {}
        for name in samples:
            header = (run / f"{name}.png").read_bytes()[:24]
            assert header[:8] == b"\x89PNG\r\n\x1a\n", f"missing native image: {name}"
            images[name] = [int.from_bytes(header[16:20], "big"), int.from_bytes(header[20:24], "big")]
        results[mode] = {"checks": checks, "ready_presentations": presented, "ready_surface_skips": missed,
                         "native_identity": [line for line in log_text.splitlines() if re.match(r"\[(renderer|window|wgpu)\]", line)], "captures": images}
        write(out / "receipt.json", results)
        print(json.dumps({"mode": mode, "checks": checks}), flush=True)
        assert all(checks.values()), f"{mode}: failed native checks; inspect {out / 'receipt.json'}"


if __name__ == "__main__":
    main()
