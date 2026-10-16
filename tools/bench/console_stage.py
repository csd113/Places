#!/usr/bin/env python3
"""Explicit native developer-console acceptance; never builds or bakes a map.

Compile tests/fixtures/native/developer-console.level.json separately, then run:
  python3 tools/bench/console_stage.py --binary /absolute/places --map /absolute/fixture.placesmap --mode controls
  python3 tools/bench/console_stage.py --binary /absolute/places --map /absolute/fixture.placesmap --mode catalog --chunk 0
  python3 tools/bench/console_stage.py --binary /absolute/places --map /absolute/fixture.placesmap --mode clock
  python3 tools/bench/console_stage.py --binary /absolute/places --map /absolute/fixture.placesmap --mode models

Catalog chunks 0..3 cover every reviewed pickup. Each item is given, captured,
removed by disabling the console, captured again, and the console is reopened at
the same pose. Run all four chunks for complete catalog evidence. --plan-only
writes a reviewable plan without executing the binary; --map is optional there.
Receipts distinguish plans from executed passes and retain failed assertions.
Screenshots need visual review in addition to the machine-checked native trace.
"""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import math
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import time

from item_physics import Plan, reserve_capture_latency

ROOT = Path(__file__).resolve().parents[2]
FIXTURE = ROOT / "tests/fixtures/native/developer-console.level.json"
RUN_ROOT = ROOT / "local/developer-console-0.11.0"
LEVEL = "developer_console_native"
CUP = "arcade:party_cup_confetti"
CUP_IDS = {"pickup_cup", "second_cup", "party_cup_confetti_1"}


def write_json(path: Path, value: object) -> None:
    path.write_text(json.dumps(value, indent=2, allow_nan=False) + "\n")


def digest(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def pickup_ids() -> list[str]:
    source = (ROOT / "src/game/pickup_catalog.rs").read_text()
    body = source.split("pub const ITEMS", 1)[1].split("];", 1)[0]
    ids = re.findall(r'id:\s*"([^"]+)"', body)
    if len(ids) != 58 or ids != sorted(set(ids)):
        raise ValueError("Reviewed pickup list changed; review native chunk coverage first")
    return ids


class Stage:
    """Sequential real input edges, with at least 1 s after every capture."""

    def __init__(self) -> None:
        self.plan = Plan(LEVEL)
        self.ms = 150
        self.markers: dict[str, int] = {}

    def action(self, kind: str, **fields: object) -> None:
        self.plan.action(self.ms, kind, **fields)
        self.ms += 180

    def tap(self, key: str) -> None:
        self.plan.tap(self.ms, key)
        self.ms += 180

    def command(self, name: str, text: str) -> None:
        self.action("text", text=text, submit=True)
        self.markers[name] = len(self.plan.steps) - 1

    def capture(self, name: str) -> None:
        self.plan.capture(self.ms, name)
        self.ms += 1200

    def player(self, x: float = 6, z: float = .65, yaw: float = 0,
               pitch: float = 0, eye_y: float = 1.6) -> None:
        self.action("player", position=[x, eye_y, z], yaw=yaw, pitch=pitch)

    def open(self) -> None:
        self.tap("`")

    def close(self) -> None:
        self.action("escape")

    def hold_key(self, key: str, duration_ms: int = 500) -> None:
        self.plan.key(self.ms, key, True)
        self.plan.key(self.ms + duration_ms, key, False)
        self.ms += duration_ms + 180

    def tap_mouse(self) -> None:
        self.plan.tap_mouse(self.ms)
        self.ms += 180

    def finish(self) -> tuple[list[dict], dict[str, int]]:
        return reserve_capture_latency(self.plan.finish(self.ms)), self.markers


def make_plan(mode: str, chunk: int) -> tuple[list[dict], dict[str, int], list[str]]:
    stage = Stage()
    stage.player(pitch=-32 if mode == "catalog" else 0)
    stage.action("console_enabled", enabled=True)
    stage.open()
    stage.capture("baseline")
    if mode == "catalog":
        # Balanced contiguous chunks: 15, 15, 14, 14; each run stays below 128
        # input actions without accumulating overlapping given props.
        ids = pickup_ids()
        start = sum(15 if index < 2 else 14 for index in range(chunk))
        selected = ids[start:start + (15 if chunk < 2 else 14)]
        for index, model in enumerate(selected):
            stage.command(f"give-{index:02}", f"/give {model}")
            stage.capture(f"given-{index:02}")
            stage.action("console_enabled", enabled=False)
            stage.capture(f"clean-{index:02}")
            stage.action("console_enabled", enabled=True)
            stage.open()
        steps, markers = stage.finish()
        return steps, markers, selected

    if mode == "clock":
        for name, value in (("negative", "-1"), ("inf", "inf"),
                            ("infinity", "Infinity"), ("nan", "NaN"),
                            ("huge", "1e999")):
            stage.command(f"invalid-scale-{name}", f"/time scale {value}")
        stage.command("remove-empty", "/remove")
        stage.command("remove-all-empty", "/remove all")
        stage.command("clock-representation", "/become party_cup")
        stage.command("clock-give", "/give core:rubber_duck")
        stage.close()
        stage.action("aim_item", id="core:rubber_duck")
        stage.tap("E")
        stage.capture("clock-held")
        stage.open()
        stage.command("invalid-held-become", "/become outdoor:tree_01")
        # Each setup establishes the same origin; the measured segment then
        # contains ordinary W input only, never a prescribed player transform.
        for tag, value in (("0", "0"), ("025", "0.25"), ("1", "1"), ("2", "2"), ("4", "4"), ("paused", "4")):
            stage.command(f"scale-{tag}", f"/time scale {value}")
            if tag == "paused":
                stage.command("clock-pause", "/physics pause")
            stage.player(yaw=90, pitch=-25)
            stage.close()
            # The hand sweeps back from the preceding segment's end pose.
            # Establish the settled start before measuring ordinary W motion.
            stage.ms += 1000
            stage.capture(f"walk-{tag}-before")
            stage.hold_key("W")
            stage.capture(f"walk-{tag}-after")
            stage.open()
        stage.close()
        stage.capture("yaw-paused-before")
        stage.hold_key("Right")
        stage.capture("yaw-paused-after")
        stage.open()
        stage.command("drop-scale-zero", "/time scale 0")
        stage.close()
        stage.tap("E")
        stage.capture("clock-dropped-a")
        stage.capture("clock-dropped-b")
        stage.open()
        stage.command("clock-step", "/physics step")
        stage.command("clock-resume-zero", "/physics resume")
        stage.capture("resumed-zero-a")
        stage.capture("resumed-zero-b")
        stage.command("clock-normal", "/time scale 1")
        stage.action("console_enabled", enabled=False)
        stage.capture("clock-cleanup")
        steps, markers = stage.finish()
        return steps, markers, []

    if mode == "models":
        cake = "arcade:party_cake_lit"
        tilt = [0, 0, math.sqrt(.5), math.sqrt(.5)]
        for name, model in (("building", "outdoor:house_wall_solid"),
                            ("unknown", "arcade:not_real"),
                            ("texture", "core:tex_carpet_beige_01")):
            stage.command(f"reject-{name}", f"/give {model}")
        stage.command("cake-give", f"/give {cake}")
        stage.close()
        stage.action("aim_item", id=cake)
        stage.capture("cake-given")
        stage.tap("E")
        stage.capture("cake-held")
        stage.open()
        stage.command("cake-zero", "/time scale 0")
        stage.close()
        stage.hold_key("Right")
        stage.capture("cake-turned")
        stage.tap("E")
        stage.capture("cake-dropped-zero")
        stage.capture("cake-dropped-wait")
        stage.action("aim_item", id=cake)
        stage.tap("E")
        stage.capture("cake-held-again")
        stage.tap_mouse()
        stage.capture("cake-thrown-zero")
        stage.action("item_pose", id=cake, position=[6, 1.4, -.5], rotation=tilt)
        stage.player(6, 1.2, pitch=-5)
        stage.capture("cake-tilted")
        stage.open()
        stage.player()
        stage.command("contact-cup-give", f"/give {CUP}")
        stage.close()
        stage.action("item_pose", id=CUP, position=[6, 0, -.5], rotation=[0, 0, 0, 1])
        stage.action("item_pose", id=cake, position=[6, .6, -.5], rotation=[0, 0, 0, 1])
        stage.open()
        stage.command("cake-normal", "/time scale 1")
        stage.close()
        stage.player(6, 1.4, pitch=-35)
        stage.capture("cake-contact")
        stage.capture("cake-settled")
        stage.action("console_enabled", enabled=False)
        stage.capture("cake-cleanup")

        stage.action("console_enabled", enabled=True)
        stage.open()
        stage.player()
        stage.command("spooner-select", "/become spooner_man")
        stage.close()
        stage.capture("spooner-selected")
        stage.open()
        stage.command("spooner-physics", "/become phys on")
        stage.command("spooner-place", "/become place")
        stage.close()
        stage.capture("spooner-dormant")
        stage.player(6, 2.1)
        stage.action("aim_item", id="spooner-man")
        stage.tap("E")
        stage.capture("spooner-interacted")
        stage.tap("E")
        stage.open()
        stage.command("spooner-zero", "/time scale 0")
        stage.close()
        stage.action("item_pose", id="spooner-man", position=[6, 1.4, -.5], rotation=tilt)
        stage.player(6, 1.2)
        stage.capture("spooner-tilted")
        stage.player(8.4, 0, yaw=270)
        stage.capture("spooner-wall-camera")
        stage.action("console_enabled", enabled=False)
        stage.capture("spooner-cleanup")

        # The small Spooner-Man copy fits pickup reach. A full-height skeleton
        # separately exercises the dormant-body activation fallback for a rig
        # whose radius cannot fit the carry reach contract.
        stage.action("console_enabled", enabled=True)
        stage.open()
        stage.player()
        stage.command("large-select", "/become skeleton")
        stage.command("large-physics", "/become phys on")
        stage.command("large-place", "/become place")
        stage.close()
        stage.capture("large-dormant")
        stage.action("aim_item", id="skeleton")
        stage.tap("E")
        stage.capture("large-overlap-refused")
        stage.hold_key("S", 600)
        stage.capture("large-stepped-clear")
        stage.action("aim_item", id="skeleton")
        stage.tap("E")
        stage.capture("large-activated")
        stage.action("console_enabled", enabled=False)
        stage.capture("models-cleanup")
        steps, markers = stage.finish()
        return steps, markers, []

    stage.command("help", "/help")
    stage.command("give-help", "/give help")
    stage.command("physics-help", "/help physics")
    stage.command("give-list", "/give list")
    stage.command("become-list", "/become list")
    stage.capture("help-list")
    for name, text in (("invalid-time", "/time scale NaN"),
                       ("invalid-range", "/time scale 5"),
                       ("invalid-physics", "/physics race"),
                       ("invalid-trigger", "/trigger pickup_cup"),
                       ("invalid-give", "/give outdoor:tree_01"),
                       ("invalid-become", "/become ../../file")):
        stage.command(name, text)
    stage.command("position", "/pos")
    stage.command("noclip-on", "/noclip on")
    stage.close()
    stage.hold_key("Space", 200)
    stage.open()
    stage.command("airborne-before", "/pos")
    stage.command("noclip-off-air", "/noclip off")
    stage.command("respawn", "/respawn")
    stage.capture("respawned")

    # Native E owns pickup; command requests only remove an actually held body.
    stage.close()
    stage.player(0, .65)
    stage.action("aim_item", id="pickup_cup")
    stage.tap("E")
    stage.capture("held-authored")
    stage.open()
    stage.command("remove-one", "/remove")
    stage.capture("after-remove")
    stage.close()
    stage.player(-.35, .65)
    stage.action("aim_item", id="second_cup")
    stage.tap("E")
    stage.open()
    stage.command("remove-all", "/remove all")
    stage.capture("after-remove-all")

    stage.player()
    stage.command("give-clock", "/give core:rubber_duck")
    stage.close()
    stage.action("aim_item", id="core:rubber_duck")
    stage.tap("E")
    stage.open()
    stage.command("scale-zero", "/time scale 0")
    stage.command("pause", "/physics pause")
    stage.capture("held-paused-a")
    stage.close()
    stage.player(6.65, .65, yaw=30, pitch=-10)
    stage.capture("held-paused-b")
    stage.tap("E")
    stage.capture("dropped-zero-a")
    stage.capture("dropped-zero-b")
    stage.open()
    stage.command("single-step", "/physics step")
    stage.command("paused-scale-two", "/time scale 2")
    stage.command("resume", "/physics resume")
    stage.command("scale-normal", "/time scale 1")
    stage.action("console_enabled", enabled=False)
    stage.action("console_enabled", enabled=True)
    stage.open()

    stage.player()
    stage.command("labels-on", "/trigger show on")
    stage.close()
    stage.capture("trigger-labels")
    stage.open()
    stage.command("trigger-once", "/trigger console_visible")
    stage.command("trigger-once-repeat", "/trigger console_visible")
    stage.command("trigger-cooldown", "/trigger console_cooldown")
    stage.command("trigger-cooldown-repeat", "/trigger console_cooldown")
    stage.command("become-cup", "/become party_cup")
    stage.close()
    stage.capture("representation-cup")
    stage.open()
    stage.command("place-off", "/become place")
    stage.capture("placed-off")
    stage.command("place-physics", "/become phys on")
    stage.player(7, .65)
    stage.command("place-on", "/become place")
    stage.capture("placed-dormant")
    stage.close()
    stage.player(7, 1.9)
    stage.action("aim_item", id=CUP)
    stage.tap("E")
    stage.capture("placed-picked-up")
    stage.open()
    stage.command("become-rig", "/become spooner_man")
    stage.close()
    stage.capture("representation-rig")
    stage.open()
    stage.command("respawn-representation", "/respawn")
    stage.command("trigger-after-respawn", "/trigger console_visible")
    stage.command("become-reset", "/become reset")
    stage.command("become-before-trigger-reset", "/become spooner_man")
    stage.command("trigger-reset", "/trigger console_reset")
    stage.capture("reset-first-person")
    stage.action("console_enabled", enabled=False)
    stage.capture("disabled-cleanup")
    stage.action("console_enabled", enabled=True)
    stage.open()
    stage.command("labels-before-reload", "/trigger show on")
    stage.action("load", level=LEVEL)
    stage.ms += 4000
    stage.capture("reloaded")
    steps, markers = stage.finish()
    return steps, markers, []


def detail(event: dict) -> dict:
    raw = event.get("detail", {})
    if isinstance(raw, dict):
        return raw
    try:
        value = json.loads(raw)
        return value if isinstance(value, dict) else {}
    except (ValueError, TypeError):
        return {}


def evidence(events: list[dict]) -> tuple[dict, dict, set[int]]:
    captures, results, seen = {}, {}, set()
    current = None
    for event in events:
        name = event.get("event", "")
        state = detail(event)
        if name == "action":
            current = state.get("id")
            if isinstance(current, int):
                seen.add(current)
        elif name == "action_result" and current is not None:
            results[current] = state
        elif name.startswith("native_capture:"):
            captures[name.split(":", 1)[1]] = state
    return captures, results, seen


def bodies(state: dict) -> dict[int, dict]:
    return {body["key"]: body for body in state.get("physics", {}).get("bodies", [])}


def authored(state: dict) -> set[str]:
    return {row[1] for row in state.get("physics", {}).get("authored", [])}


def held_position(state: dict) -> list[float] | None:
    held = state.get("physics", {}).get("held")
    return bodies(state).get(held, {}).get("pose", {}).get("position")


def validate(receipt: dict, events: list[dict], steps: list[dict], markers: dict[str, int],
             selected: list[str], out: Path, log: str) -> None:
    checks = receipt["checks"]
    def require(name: str, passed: bool) -> None:
        checks.append(dict(name=name, passed=bool(passed)))
    captures, results, seen = evidence(events)
    receipt.update(snapshots=captures, command_results={name: results.get(index) for name, index in markers.items()})
    require("native exit zero", receipt.get("exit_code") == 0)
    require("all bounded native actions execute", seen == set(range(len(steps))))
    require("fixture committed natively", any(e.get("event") == "world_committed" and detail(e).get("current_level_id") == LEVEL for e in events))
    require("native renderer has no validation failure", not any(token in log.lower() for token in ("validation error", "panicked", "fatal device", "catalog hash mismatch")))
    require("actual frames were presented", any(e.get("event") == "present" and e.get("detail") == "ready" for e in events))
    require("no missed ready presentation", not any(e.get("event") == "surface_not_presented" and e.get("detail") == "ready" for e in events))
    require("ordinary controls do not require escape recovery", not any(e.get("event") == "physics_recovery" for e in events))
    for step in steps:
        action = step["action"]
        if action["kind"] != "capture":
            continue
        name = action["name"]
        path = out / f"{name}.png"
        header = path.read_bytes()[:24] if path.is_file() else b""
        valid = len(header) == 24 and header[:8] == b"\x89PNG\r\n\x1a\n"
        require(f"{name}: actual native PNG and snapshot", valid and name in captures)
        if valid:
            receipt["screenshots"].append(dict(path=str(path), sha256=digest(path), size=[int.from_bytes(header[16:20], "big"), int.from_bytes(header[20:24], "big")]))
    base = captures.get("baseline", {})
    require("baseline physics evidence exists", bool(bodies(base)))
    def command(name: str) -> dict:
        return results.get(markers.get(name), {})
    def message(name: str) -> str:
        return str(command(name).get("console_last", ""))
    if receipt["mode"] == "catalog":
        for index, model in enumerate(selected):
            given = captures.get(f"given-{index:02}", {})
            clean = captures.get(f"clean-{index:02}", {})
            require(f"{model}: exact successful give receipt", f"Placed `{model}`" in message(f"give-{index:02}") and "movable pickup physics" in message(f"give-{index:02}"))
            require(f"{model}: one actual movable body", len(bodies(given)) == len(bodies(base)) + 1)
            added = [body for key, body in bodies(given).items() if key not in bodies(base)]
            require(f"{model}: given body starts dynamic and active", len(added) == 1 and added[0].get("motion") == "dynamic" and added[0].get("dormant") is False)
            props = given.get("console_session", {}).get("props", [])
            require(f"{model}: native session identity matches requested pickup", len(props) == 1 and props[0].get("model") == model and props[0].get("physical") is True)
            require(f"{model}: disable removes bodies and visuals", bool(clean) and set(bodies(clean)) == set(bodies(base)) and all(clean.get(key) == base.get(key) for key in ("characters", "dynamic_objects")))
            require(f"{model}: session registry is empty after disable", clean.get("console_session", {}).get("props") == [])
            require(f"{model}: disabled console is closed", clean.get("console_enabled") is False and clean.get("console_open") is False)
            require(f"{model}: repeated same spawn pose", given.get("player_position") is not None and base.get("player_position") is not None and math.dist(given["player_position"], base["player_position"]) < .005)
        return

    if receipt["mode"] == "models":
        cake = "arcade:party_cake_lit"
        def session_key(state: dict, model: str) -> int | None:
            return next((prop.get("key") for prop in state.get("console_session", {}).get("props", []) if prop.get("model") == model), None)
        def tilted(state: dict, key: int | None) -> bool:
            rotation = bodies(state).get(key, {}).get("render_pose", {}).get("rotation")
            target = [0, 0, math.sqrt(.5), math.sqrt(.5)]
            return rotation is not None and len(rotation) == 4 and abs(sum(a * b for a, b in zip(rotation, target))) > .999
        given = captures.get("cake-given", {})
        cake_key = session_key(given, cake)
        for name in ("building", "unknown", "texture"):
            require(f"{name}: give rejected without physical side effects", message(f"reject-{name}").startswith("Error:") and set(bodies(command(f"reject-{name}"))) == set(bodies(base)))
        require("lit cake spawns one original animated character and body", cake_key is not None and cake_key in bodies(given) and given.get("characters") == base.get("characters", -100) + 1 and f"Placed `{cake}`" in message("cake-give"))
        for name in ("cake-held", "cake-turned", "cake-held-again"):
            state = captures.get(name, {})
            require(f"{name}: same physical cake is held", cake_key is not None and state.get("physics", {}).get("held") == cake_key and state.get("characters") == given.get("characters"))
        original, turned = held_position(captures.get("cake-held", {})), held_position(captures.get("cake-turned", {}))
        require("ordinary look input carries cake at zero scale", original is not None and turned is not None and math.dist(original, turned) > .2 and captures.get("cake-turned", {}).get("physics_clock", {}).get("time_scale") == 0)
        dropped, waited = captures.get("cake-dropped-zero", {}), captures.get("cake-dropped-wait", {})
        require("cake drop releases and freezes at zero world scale", cake_key is not None and dropped.get("physics", {}).get("held") is None and cake_key in bodies(dropped) and bodies(dropped).get(cake_key, {}).get("pose") == bodies(waited).get(cake_key, {}).get("pose"))
        require("cake can be picked up twice through ordinary E", sum(event.get("event") == "item_picked_up" and detail(event).get("physics", {}).get("held") == cake_key for event in events) >= 2)
        require("ordinary mouse throw releases physical lit cake", cake_key is not None and any(event.get("event") == "item_thrown" and cake_key in bodies(detail(event)) and detail(event).get("physics", {}).get("held") is None for event in events))
        require("cake diagnostic keeps full 90-degree body render orientation", cake_key is not None and tilted(captures.get("cake-tilted", {}), cake_key))
        cup_key = session_key(captures.get("cake-contact", {}), CUP)
        frame_states = [detail(event) for event in events if event.get("event") == "physics_frame"]
        require("continuous native physical contact evidence exists", bool(frame_states))
        require("lit cake and cup make actual solver contact", cake_key is not None and cup_key is not None and any(set(pair) == {cake_key, cup_key} for state in frame_states for pair in state.get("contacts", [])))
        dormant = captures.get("spooner-dormant", {})
        spooner_key = session_key(dormant, "spooner-man")
        require("Spooner-Man physical duplicate is dormant with two animated visuals", spooner_key is not None and bodies(dormant).get(spooner_key, {}).get("dormant") is True and dormant.get("characters") == base.get("characters", -100) + 2)
        interacted = captures.get("spooner-interacted", {})
        require("ordinary E activates physical Spooner-Man copy", spooner_key is not None and bodies(interacted).get(spooner_key, {}).get("dormant") is False and any(event.get("event") in ("item_picked_up", "item_activated") and spooner_key in bodies(detail(event)) and bodies(detail(event))[spooner_key].get("dormant") is False for event in events))
        require("Spooner-Man diagnostic retains full 90-degree body render orientation", spooner_key is not None and tilted(captures.get("spooner-tilted", {}), spooner_key))
        wall = captures.get("spooner-wall-camera", {})
        require("near-wall screenshot retains selected rig and third person", wall.get("representation", {}).get("model") == "spooner-man" and wall.get("representation", {}).get("third_person") is True)
        large = captures.get("large-dormant", {})
        large_key = session_key(large, "skeleton")
        activated = captures.get("large-activated", {})
        require("large rig placement starts dormant", large_key is not None and bodies(large).get(large_key, {}).get("dormant") is True)
        overlapping = captures.get("large-overlap-refused", {})
        require("overlapping player cannot activate a dormant full-size copy", large_key is not None and bodies(overlapping).get(large_key, {}).get("dormant") is True and bodies(overlapping).get(large_key, {}).get("pose") == bodies(large).get(large_key, {}).get("pose") and any(event.get("event") == "item_activation_blocked" for event in events))
        stepped = captures.get("large-stepped-clear", {})
        require("ordinary movement can leave a dormant enclosing copy", bool(stepped) and math.dist(stepped.get("player_position", [0, 0, 0]), overlapping.get("player_position", [0, 0, 0])) > 1 and bodies(stepped).get(large_key, {}).get("dormant") is True)
        require("ordinary E activates uncarryable dormant rig without holding it", large_key is not None and activated.get("physics", {}).get("held") is None and bodies(activated).get(large_key, {}).get("dormant") is False and any(event.get("event") == "item_activated" and bodies(detail(event)).get(large_key, {}).get("dormant") is False for event in events))
        for name in ("cake-cleanup", "spooner-cleanup", "models-cleanup"):
            state = captures.get(name, {})
            require(f"{name}: no orphaned body character or session visual", bool(state) and set(bodies(state)) == set(bodies(base)) and state.get("characters") == base.get("characters") and state.get("dynamic_objects") == base.get("dynamic_objects") and state.get("console_session", {}).get("props") == [] and state.get("representation", {}).get("third_person") is False)
        require("model screenshots show unobstructed gameplay", all(captures.get(step["action"]["name"], {}).get("console_open") is False for step in steps if step["action"]["kind"] == "capture" and step["action"]["name"] != "baseline"))
        receipt["model_visual_review"] = ["Cake texture, seven embedded flame meshes and candle flicker before time freeze", "Warm cake light spill in the dim room", "Cake and Spooner-Man tilted geometry follow the traced full quaternion", "Third-person camera remains outside the nearby wall", "No orphaned animated copies after each cleanup"]
        return

    if receipt["mode"] == "clock":
        for name in ("negative", "inf", "infinity", "nan", "huge"):
            state = command(f"invalid-scale-{name}")
            require(f"scale {name}: rejected without changing clock", message(f"invalid-scale-{name}").startswith("Error:") and state.get("physics_clock", {}).get("time_scale") == 1)
        for name in ("remove-empty", "remove-all-empty"):
            require(f"{name}: rejected without removing authored objects", message(name).startswith("Error:") and set(bodies(command(name))) == set(bodies(base)) and authored(command(name)) == authored(base))
        held = captures.get("clock-held", {})
        held_key = held.get("physics", {}).get("held")
        rejected = command("invalid-held-become")
        require("unsupported become preserves held ownership and selected visual", held_key is not None and message("invalid-held-become").startswith("Error:") and rejected.get("physics", {}).get("held") == held_key and rejected.get("representation") == held.get("representation"))
        metrics = []
        for tag, scale in (("0", 0), ("025", .25), ("1", 1), ("2", 2), ("4", 4), ("paused", 4)):
            before, after = captures.get(f"walk-{tag}-before", {}), captures.get(f"walk-{tag}-after", {})
            start, end = before.get("player_position"), after.get("player_position")
            displacement = math.hypot(end[0] - start[0], end[2] - start[2]) if start is not None and end is not None else None
            initial_clock, final_clock = before.get("physics_clock", {}), after.get("physics_clock", {})
            ticks = final_clock.get("fixed_steps", 0) - initial_clock.get("fixed_steps", 0) if "fixed_steps" in initial_clock and "fixed_steps" in final_clock else None
            metrics.append(dict(tag=tag, scale=scale, paused=tag == "paused", start=start, end=end,
                                displacement_m=displacement, world_fixed_steps=ticks, ordinary_w_duration_ms=500))
            require(f"walk {tag}: same real starting pose", start is not None and math.dist(start, [6, 1.6, .65]) < .03)
            require(f"walk {tag}: ordinary 500 ms W travels 0.9..2 metres", displacement is not None and .9 <= displacement <= 2)
            require(f"walk {tag}: normal player remains collision controlled", after.get("noclip") is False and after.get("console_open") is False and after.get("state") == "Playing")
            require(f"walk {tag}: requested scale and pause reach native clock", initial_clock.get("time_scale") == scale and initial_clock.get("paused") is (tag == "paused"))
            first_position, last_position = held_position(before), held_position(after)
            require(f"walk {tag}: same held duck follows unscaled input", held_key is not None and before.get("physics", {}).get("held") == held_key and after.get("physics", {}).get("held") == held_key and first_position is not None and last_position is not None and math.dist(first_position, last_position) > .8)
            require(f"walk {tag}: world tick behavior", ticks is not None and (ticks == 0 if tag in ("0", "paused") else ticks > 0))
        receipt["clock_walk_metrics"] = metrics
        distances = [metric["displacement_m"] for metric in metrics]
        require("all scales preserve similar player speed", all(value is not None for value in distances) and max(distances) - min(distances) <= .25)
        yaw_before, yaw_after = captures.get("yaw-paused-before", {}), captures.get("yaw-paused-after", {})
        first_yaw, last_yaw = yaw_before.get("player_yaw"), yaw_after.get("player_yaw")
        turn = abs(math.atan2(math.sin(last_yaw - first_yaw), math.cos(last_yaw - first_yaw))) if first_yaw is not None and last_yaw is not None else None
        require("ordinary Right input turns unscaled while paused", turn is not None and .3 <= turn <= 1.2 and yaw_before.get("physics_clock") == yaw_after.get("physics_clock"))
        first_position, last_position = held_position(yaw_before), held_position(yaw_after)
        require("held duck follows ordinary paused look input", first_position is not None and last_position is not None and math.dist(first_position, last_position) > .2 and yaw_after.get("physics", {}).get("held") == held_key)
        dropped_a, dropped_b = captures.get("clock-dropped-a", {}), captures.get("clock-dropped-b", {})
        poses_a = {key: body.get("pose") for key, body in bodies(dropped_a).items()}
        poses_b = {key: body.get("pose") for key, body in bodies(dropped_b).items()}
        require("actual E releases held duck at paused zero time", dropped_a.get("physics", {}).get("held") is None and held_key in bodies(dropped_a))
        require("released body remains frozen", bool(dropped_a) and poses_a == poses_b and dropped_a.get("physics_clock") == dropped_b.get("physics_clock"))
        ticks_before = dropped_b.get("physics_clock", {}).get("fixed_steps")
        stepped = command("clock-step").get("physics_clock", {})
        require("explicit step advances exactly one tick at zero scale", isinstance(ticks_before, int) and stepped.get("fixed_steps") == ticks_before + 1 and stepped.get("paused") is True and stepped.get("time_scale") == 0)
        resumed_a, resumed_b = captures.get("resumed-zero-a", {}), captures.get("resumed-zero-b", {})
        require("resume at zero keeps world stopped", bool(resumed_a) and resumed_a.get("physics_clock") == resumed_b.get("physics_clock") and resumed_a.get("physics_clock", {}).get("paused") is False and resumed_a.get("physics_clock", {}).get("time_scale") == 0)
        cleanup = captures.get("clock-cleanup", {})
        require("clock run cleans session resources", cleanup.get("console_session", {}).get("props") == [] and cleanup.get("representation", {}).get("third_person") is False and set(bodies(cleanup)) == set(bodies(base)))
        return

    for name in ("invalid-time", "invalid-range", "invalid-physics", "invalid-trigger", "invalid-give", "invalid-become", "trigger-once-repeat", "trigger-cooldown-repeat"):
        require(f"{name}: explicit refusal", message(name).startswith("Error:"))
    require("help/list commands produce output", all(message(name) and not message(name).startswith("Error:") for name in ("help", "give-help", "physics-help", "give-list", "become-list")))
    require("invalid time leaves scale unchanged", command("invalid-time").get("physics_clock", {}).get("time_scale") == 1)
    require("position reports eye and feet", "Eye (" in message("position") and "feet Y" in message("position"))
    require("noclip enables", command("noclip-on").get("noclip") is True)
    air = command("noclip-off-air")
    flying = command("airborne-before")
    require("ordinary Space input establishes actual noclip flight", flying.get("noclip") is True and flying.get("player_position", [0, 0, 0])[1] > 2)
    require("noclip disable retains clear airborne pose", air.get("noclip") is False and air.get("player_position") is not None and flying.get("player_position") is not None and math.dist(air["player_position"], flying["player_position"]) < .005 and "walking restored here" in message("noclip-off-air").lower())
    respawn = command("respawn")
    require("respawn uses validated authored destination", "Respawned" in message("respawn") and math.dist(respawn.get("player_position", [0, 0, 0]), [6, 1.6, .65]) < .03)
    require("native E picks up original authored cup", captures.get("held-authored", {}).get("physics", {}).get("held") is not None)
    require("remove one preserves other exact cup identities", "pickup_cup" not in authored(captures.get("after-remove", {})) and {"second_cup", "party_cup_confetti_1"} <= authored(captures.get("after-remove", {})))
    all_removed = captures.get("after-remove-all", {})
    require("remove all removes explicit and generated matching movables", not (CUP_IDS & authored(all_removed)) and "Removed 2 physics instance(s)" in message("remove-all"))
    require("fixed cup remains in native fixed-prop identity trace", any(row.get("id") == "fixed_cup" and row.get("model") == CUP and row.get("entity_live") is True for row in all_removed.get("fixed_instances", [])))
    first, second = captures.get("held-paused-a", {}), captures.get("held-paused-b", {})
    require("held item control moves while world is paused at zero scale", held_position(first) is not None and held_position(second) is not None and math.dist(held_position(first), held_position(second)) > .1)
    require("pause at zero scale advances no ticks", first.get("physics_clock") == second.get("physics_clock") and first.get("physics_clock", {}).get("paused") is True)
    drop_a, drop_b = captures.get("dropped-zero-a", {}), captures.get("dropped-zero-b", {})
    poses_a = {key: body.get("pose") for key, body in bodies(drop_a).items()}
    poses_b = {key: body.get("pose") for key, body in bodies(drop_b).items()}
    require("dropped body stays frozen at zero scale", bool(drop_a) and drop_a.get("physics", {}).get("held") is None and poses_a == poses_b and drop_a.get("physics_clock") == drop_b.get("physics_clock"))
    before_steps = drop_b.get("physics_clock", {}).get("fixed_steps")
    clock = command("single-step").get("physics_clock", {})
    require("step advances exactly one 120 Hz tick and stays paused", isinstance(before_steps, int) and clock.get("fixed_steps") == before_steps + 1 and clock.get("paused") is True and clock.get("time_scale") == 0)
    require("pause takes precedence over nonzero time scale", command("paused-scale-two").get("physics_clock", {}).get("paused") is True and command("paused-scale-two").get("physics_clock", {}).get("time_scale") == 2)
    require("resume restores clock execution", command("resume").get("physics_clock", {}).get("paused") is False)
    labels = captures.get("trigger-labels", {}).get("trigger_labels", [])
    require("representative scene captures close the console overlay", all(captures.get(name, {}).get("console_open") is False for name in ("trigger-labels", "representation-cup", "representation-rig")))
    require("trigger labels include nearby visible ID", "console_visible" in labels)
    require("trigger labels exclude occluded far and behind candidates", not ({"console_occluded", "console_far", "console_behind"} & set(labels)))
    require("trigger follows authored one-shot dispatch", "dispatched" in message("trigger-once") and "one-shot" in message("trigger-once-repeat"))
    require("trigger executes the actual authored physics action", any(event.get("event") == "physics_group_activated" and detail(event).get("group") == "native_fixture" for event in events))
    require("trigger follows authored cooldown", "dispatched" in message("trigger-cooldown") and "cooling down" in message("trigger-cooldown-repeat"))
    for capture, model in (("representation-cup", CUP), ("representation-rig", "spooner-man")):
        representation = captures.get(capture, {}).get("representation", {})
        require(f"{capture}: actual selected model and third person", representation.get("model") == model and representation.get("third_person") is True)
    require("respawn preserves representation and one-shot trigger state", command("respawn-representation").get("representation", {}).get("model") == "spooner-man" and command("respawn-representation").get("representation", {}).get("third_person") is True and "one-shot" in message("trigger-after-respawn"))
    off, on = captures.get("placed-off", {}), captures.get("placed-dormant", {})
    require("visual placement adds no rigid body", len(bodies(off)) == len(bodies(captures.get("representation-cup", {})) or {}))
    require("placement physics toggle is traced", off.get("console_session", {}).get("placement_physics") is False and on.get("console_session", {}).get("placement_physics") is True)
    require("physical placement adds a dormant body", len(bodies(on)) == len(bodies(off)) + 1 and any(body.get("dormant") for key, body in bodies(on).items() if key not in bodies(off)))
    require("placed dormant copy can be picked up", captures.get("placed-picked-up", {}).get("physics", {}).get("held") is not None)
    require("explicit become reset restores first person", command("become-reset").get("representation", {}).get("third_person") is False)
    require("trigger reset restores first person", captures.get("reset-first-person", {}).get("representation", {}).get("third_person") is False)
    cleaned = captures.get("disabled-cleanup", {})
    require("disable clears all session visuals and physics", cleaned.get("representation", {}).get("third_person") is False and cleaned.get("console_session", {}).get("props") == [])
    reloaded = captures.get("reloaded", {})
    require("real reload commits another world", sum(e.get("event") == "world_committed" for e in events) >= 2)
    require("reload restores exact authored movable identities", authored(reloaded) == authored(base) and CUP_IDS <= authored(reloaded))
    require("reload clears representation labels and clock", reloaded.get("representation", {}).get("third_person") is False and not reloaded.get("trigger_labels", ["missing"]) and reloaded.get("physics_clock", {}).get("time_scale") == 1 and reloaded.get("physics_clock", {}).get("paused") is False)
    require("reload clears session props and developer toggle state", reloaded.get("console_session", {}).get("props") == [] and reloaded.get("console_session", {}).get("trigger_labels") is False and reloaded.get("console_session", {}).get("placement_physics") is False)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--binary", type=Path, required=True, help="Direct native executable; no app wrapper")
    parser.add_argument("--map", type=Path, help="Previously compiled developer_console_native .placesmap")
    parser.add_argument("--mode", choices=("catalog", "controls", "clock", "models"), required=True)
    parser.add_argument("--chunk", type=int, choices=range(4), default=0)
    parser.add_argument("--out", type=Path)
    parser.add_argument("--size", default="1280x720")
    parser.add_argument("--fullscreen", action="store_true", help="Use native desktop fullscreen for visible surface acceptance")
    parser.add_argument("--plan-only", action="store_true")
    args = parser.parse_args()
    if not args.plan_only and args.map is None:
        parser.error("--map is required for a native run; this tool never compiles fixtures")
    if not re.fullmatch(r"[1-9]\d{2,3}x[1-9]\d{2,3}", args.size):
        parser.error("--size must be WIDTHxHEIGHT, for example 1280x720")
    out = (args.out or RUN_ROOT / f"{datetime.now(timezone.utc):%Y%m%dT%H%M%SZ}-{args.mode}-{args.chunk}").resolve()
    out.mkdir(parents=True, exist_ok=False)
    binary = args.binary.resolve(strict=not args.plan_only)
    steps, markers, selected = make_plan(args.mode, args.chunk)
    settings = json.loads((ROOT / "tests/fixtures/native/retro-arcade-manifest.json").read_text())["settings"]
    width, height = map(int, args.size.split("x"))
    settings.update(window_width=width, window_height=height, window_mode="fullscreen" if args.fullscreen else "windowed", vsync=False,
                    developer_console=False, quality="high", texture_filtering="high", lightmaps="full", reflections="full")
    write_json(out / "settings.json", settings)
    write_json(out / "actions.json", steps)
    receipt = dict(mode=args.mode, chunk=args.chunk if args.mode == "catalog" else None,
                   status="planned", binary=str(binary), fixture_sha256=digest(FIXTURE),
                   reviewed_ids=selected, actions=len(steps), duration_ms=steps[-1]["delay_ms"],
                   screenshots=[], checks=[], command_markers=markers, requested_vsync=False,
                   requested_window_mode=settings["window_mode"],
                   visual_review="required: inspect retained native PNGs; trace assertions do not prove appearance")
    if binary.is_file():
        receipt["binary_sha256"] = digest(binary)
    if args.map is not None:
        package = args.map.resolve(strict=True)
        if package.suffix != ".placesmap":
            raise ValueError("--map must be a separately compiled .placesmap")
        (out / "levels").mkdir()
        shutil.copyfile(package, out / "levels" / f"{LEVEL}.placesmap")
        receipt.update(package=str(package), package_sha256=digest(package))
    write_json(out / "receipt.json", receipt)
    if args.plan_only:
        print(json.dumps(dict(status="planned", out=str(out), actions=len(steps), duration_ms=receipt["duration_ms"])))
        return 0
    env = {key: value for key, value in os.environ.items() if not key.startswith("PLACES_") and key != "SDL_VIDEODRIVER"}
    env.update(PLACES_ASSET_ROOT=str(ROOT), PLACES_STATE_ROOT=str(out), PLACES_LEVEL=LEVEL,
               PLACES_SPAWN="6,1.6,0.65,0", PLACES_BENCH="1", PLACES_VERBOSE="1",
               PLACES_VSYNC="0", PLACES_BENCH_FRAMES="0", PLACES_BENCH_WARMUP="0",
               PLACES_BENCH_ACTIONS=str(out / "actions.json"), PLACES_LOAD_TRACE=str(out / "trace.jsonl"),
               PLACES_STATE_LOG=str(out / "player.csv"), PLACES_BENCH_OUT=str(out / "frames.csv"))
    if args.mode == "models":
        env["PLACES_PHYSICS_TRACE"] = "1"
    receipt["environment"] = {key: value for key, value in env.items() if key.startswith("PLACES_")}
    started = time.monotonic()
    try:
        result = subprocess.run([str(binary)], cwd=ROOT, env=env, capture_output=True, text=True, timeout=90)
        log = result.stdout + result.stderr
        (out / "native.log").write_text(log)
        receipt["exit_code"] = result.returncode
        events = [json.loads(line) for line in (out / "trace.jsonl").read_text().splitlines()]
        receipt["native_identity"] = [line for line in log.splitlines() if line.startswith(("[renderer]", "[window]", "[settings]"))]
        validate(receipt, events, steps, markers, selected, out, log)
        unchanged = digest(binary) == receipt["binary_sha256"] and digest(package) == receipt["package_sha256"]
        receipt["checks"].append(dict(name="binary and package remain unchanged", passed=unchanged))
        failures = [check["name"] for check in receipt["checks"] if not check["passed"]]
        if failures:
            raise ValueError("Native assertions failed: " + "; ".join(failures))
        receipt["status"] = "passed"
    except subprocess.TimeoutExpired as error:
        def decoded(value: str | bytes | None) -> str:
            return value.decode(errors="replace") if isinstance(value, bytes) else value or ""
        (out / "native.log").write_text(decoded(error.stdout) + decoded(error.stderr))
        receipt.update(status="failed", error="Native process exceeded 90 seconds")
    except Exception as error:
        receipt.update(status="failed", error=str(error))
    finally:
        receipt["elapsed_seconds"] = time.monotonic() - started
        write_json(out / "receipt.json", receipt)
    print(json.dumps(dict(status=receipt["status"], out=str(out), checks=len(receipt["checks"]), error=receipt.get("error"))))
    return 0 if receipt["status"] == "passed" else 1


if __name__ == "__main__":
    sys.exit(main())
