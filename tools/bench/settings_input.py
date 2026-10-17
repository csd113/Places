#!/usr/bin/env python3
"""Explicit settings/input native acceptance using an already compiled fixture.

Pass the existing developer-console fixture package with --map. This tool never
builds, bakes, packages an app, or uses LaunchServices. Each run owns isolated
state and records real SDL-handler snapshots, PNGs and complete process logs.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[2]
LEVEL = "developer_console_native"


class Plan:
    def __init__(self):
        self.steps = []
        self.ms = 200

    def action(self, kind, **fields):
        self.steps.append({"after": f"ready:{LEVEL}", "delay_ms": self.ms,
                           "action": {"kind": kind, **fields}})
        self.ms += 90

    def key(self, key, pressed=True, repeat=False):
        self.action("key", key=key, pressed=pressed, repeat=repeat)

    def tap(self, key):
        self.key(key)
        self.key(key, False)

    def capture(self, name):
        self.action("capture", name=name)
        self.ms += 1000


def controls_plan():
    p = Plan()
    p.action("focus", focused=True)
    p.tap("Escape")
    p.tap("Down")
    p.tap("Return")
    for _ in range(3):
        p.tap("Down")
    p.tap("Return")
    p.tap("Return")  # Forward binding capture.
    p.key("Return", repeat=True)
    p.capture("repeat-awaits-key")
    p.tap("Escape")
    p.tap("Return")
    p.tap("Q")
    for _ in range(10):
        p.tap("Down")
    p.tap("Return")
    p.tap("R")
    p.tap("Down")
    p.tap("Return")
    p.tap("T")
    p.tap("Down")
    p.tap("Return")
    p.tap("G")
    p.capture("rebound-controls")
    for _ in range(3):
        p.tap("Escape")
    p.key("Q")
    p.ms += 300
    p.key("Q", False)
    p.capture("rebound-forward")
    p.tap("F")
    p.capture("old-flashlight-key")
    p.key("T")
    p.key("T", repeat=True)
    p.key("T", repeat=True)
    p.capture("flashlight-held")
    p.key("T", False)
    p.tap("T")
    p.capture("flashlight-second-press")
    p.key("G")
    p.key("G", repeat=True)
    p.capture("console-held")
    p.key("G", False)
    p.tap("T")
    p.tap("Escape")
    p.action("player", position=[0, 1.6, .65], yaw=0, pitch=-30)
    p.action("aim_item", id="pickup_cup")
    p.key("R")
    p.key("R", repeat=True)
    p.capture("interact-held")
    p.key("R", False)
    p.tap("R")
    p.capture("interact-second-press")
    p.key("Q")
    p.tap("Escape")
    p.key("Q", False)  # Release while paused must not stick on resume.
    p.capture("paused")
    p.tap("Escape")
    p.capture("resumed")
    p.action("quit")
    return p


def reload_plan():
    p = Plan()
    p.action("focus", focused=True)
    p.tap("G")
    p.capture("reloaded-console")
    p.tap("Escape")
    p.tap("T")
    p.capture("reloaded-flashlight")
    p.action("quit")
    return p


def clock_plan():
    p = Plan()
    p.action("focus", focused=True)
    p.action("player", position=[6, 9.6, .65], yaw=0, pitch=0)
    p.action("focus", focused=False)
    p.ms += 700
    p.capture("unfocused")
    p.key("Q")  # Press outside focus must have no ownership on return.
    p.action("focus", focused=True)
    p.capture("refocused")
    p.tap("G")
    p.action("text", text="/time scale 0", submit=True)
    p.tap("Escape")
    p.action("player", position=[0, 1.6, .65], yaw=0, pitch=-30)
    p.action("aim_item", id="pickup_cup")
    p.tap("R")
    p.capture("stopped-held-start")
    p.key("Q")
    p.ms += 350
    p.key("Q", False)
    p.capture("stopped-held-moved")
    p.action("quit")
    return p


def execute(binary, state, out, plan, label, extra_env=None):
    assert len(plan.steps) <= 128, "native script exceeds engine action budget"
    script = out / f"{label}-actions.json"
    script.write_text(json.dumps(plan.steps, indent=2) + "\n")
    trace = out / f"{label}-trace.jsonl"
    env = {k: v for k, v in os.environ.items() if not k.startswith("PLACES_")}
    env.update({"PLACES_ASSET_ROOT": str(ROOT), "PLACES_STATE_ROOT": str(state),
                "PLACES_LEVEL": LEVEL, "PLACES_BENCH": "1", "PLACES_BENCH_FRAMES": "0",
                "PLACES_VERBOSE": "1", "PLACES_VSYNC": "on", "PLACES_LOAD_TRACE": str(trace),
                "PLACES_BENCH_ACTIONS": str(script)})
    env.update(extra_env or {})
    with (out / f"{label}-native.log").open("w") as log:
        result = subprocess.run([str(binary)], cwd=ROOT, env=env, stdout=log,
                                stderr=subprocess.STDOUT, timeout=90, check=False)
    events = [json.loads(line) for line in trace.read_text().splitlines()]
    assert result.returncode == 0, f"{label} exited {result.returncode}"
    assert events[-1]["event"] == "shutdown_complete", f"{label} did not shut down"
    return {event["event"].removeprefix("native_capture:"): json.loads(event["detail"])
            for event in events if event["event"].startswith("native_capture:")}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--map", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    binary, package, out = args.binary.resolve(), args.map.resolve(), args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    state = out / "state"
    (state / "levels").mkdir(parents=True)
    shutil.copy2(package, state / "levels" / f"{LEVEL}.placesmap")
    (state / "settings.json").write_text(json.dumps({"window_width":640,"window_height":360,
        "window_mode":"windowed","quality":"low","lightmaps":"full",
        "bloom":False,"developer_console":True}))
    receipts = {"binary":str(binary), "binary_sha256":hashlib.sha256(binary.read_bytes()).hexdigest(),
                "package":str(package), "package_sha256":hashlib.sha256(package.read_bytes()).hexdigest()}
    (out / "identity.json").write_text(json.dumps(receipts, indent=2) + "\n")
    checks = {}
    try:
        c = execute(binary, state, out, controls_plan(), "controls", {"PLACES_NO_BLOOM":"0"})
        checks["repeat_does_not_rebind"] = c["repeat-awaits-key"]["controls"]["rebinding_action"] == "forward"
        expected = {"forward":"Q", "interact":"R", "flashlight":"T", "console":"G"}
        saved = json.loads((state / "settings.json").read_text())
        checks["bindings_saved"] = all(saved["bindings"][k] == v for k,v in expected.items())
        checks["graphics_independent"] = c["rebound-forward"]["quality"] == "low" and c["rebound-forward"]["renderer_lighting"] == "high"
        checks["false_disable_preserves_bloom_off"] = not c["rebound-forward"]["bloom"]
        checks["rebound_forward_moves"] = c["rebound-forward"]["player_position"][2] < .3
        checks["old_flashlight_key_inactive"] = not c["old-flashlight-key"]["controls"]["flashlight_on"]
        checks["flashlight_once_per_press"] = c["flashlight-held"]["controls"]["flashlight_on"] and not c["flashlight-second-press"]["controls"]["flashlight_on"]
        checks["console_repeat_and_capture"] = c["console-held"]["console_open"] and not c["console-held"]["relative_mouse"]
        checks["interact_once_per_press"] = c["interact-held"]["physics"]["held"] is not None and c["interact-second-press"]["physics"]["held"] is None
        checks["pause_capture"] = c["paused"]["state"] == "Paused" and not c["paused"]["relative_mouse"]
        checks["resume_capture_and_release"] = c["resumed"]["relative_mouse"] and "held: 0," in c["resumed"]["input"]
        r = execute(binary, state, out, reload_plan(), "reload")
        checks["reload_console"] = r["reloaded-console"]["console_open"]
        checks["reload_flashlight"] = r["reloaded-flashlight"]["controls"]["flashlight_on"]
        k = execute(binary, state, out, clock_plan(), "clock")
        checks["unfocused_player_frozen"] = abs(k["unfocused"]["player_position"][1] - 9.6) < 0.1
        checks["unfocused_capture_released"] = not k["unfocused"]["relative_mouse"]
        checks["refocus_does_not_replay_key"] = "held: 0," in k["refocused"]["input"]
        start, moved = k["stopped-held-start"], k["stopped-held-moved"]
        checks["stopped_world_player_carry_control"] = start["physics"]["held"] is not None and moved["physics"]["held"] == start["physics"]["held"] and abs(moved["player_position"][2]-start["player_position"][2]) > .2 and moved["physics_clock"]["time_scale"] == 0
        held_start = next((body for body in start["physics"]["bodies"] if body["key"] == start["physics"]["held"]), None)
        held_moved = next((body for body in moved["physics"]["bodies"] if body["key"] == moved["physics"]["held"]), None)
        checks["stopped_world_item_follows"] = held_start is not None and held_moved is not None and abs(held_moved["pose"]["position"][2] - held_start["pose"]["position"][2]) > .2
    finally:
        (out / "result.json").write_text(json.dumps({"checks":checks, "limitations":[
            "Scripted focus exercises the actual handler; it is not an OS focus-switch certification.",
            "Mac/Metal functional acceptance with V-sync; no throughput comparison."]}, indent=2) + "\n")
    print(json.dumps(checks, indent=2), flush=True)
    return 0 if all(checks.values()) else 1


if __name__ == "__main__":
    raise SystemExit(main())
