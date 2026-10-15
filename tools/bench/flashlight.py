#!/usr/bin/env python3
"""Opt-in native flashlight acceptance, Office capture and matched frame timings.

Uses already-built Office packages; never builds Rust or bakes maps. The doors
case uses Arcade's existing restroom door because Office has no authored doors.
Key/text actions use the production SDL input path. Diagnostic player/aim poses
are retained in the plan; the walkthrough moves with real held gameplay keys.
Video streams actual consecutive native RGBA frames to a private ffmpeg FIFO.
Screenshots and traces require visual review; numeric checks do not certify art.

Example (a 2x display produces the requested 1280x720 drawable):
  python3 tools/bench/flashlight.py --mode walkthrough --binary /path/to/places \
    --out local/flashlight/walkthrough --size 640x360 --expected-drawable 1280x720
  python3 tools/bench/flashlight.py --mode performance --binary /path/to/places \
    --out local/flashlight/performance --size 1280x720 --expected-drawable 2560x1440
Use --plan-only to write settings, actions and commands without starting a game.
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
import statistics
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[2]
OFFICE = "office_demo"
ARCADE = "retro_arcade_demo"
MANNEQUIN = "basement_mannequin"
VIEWS = {
    "dark": ("8.5,1.6,11,0", "0,-13"),
    "busy": ("13,8.8,7,0", "0,-9"),
}


def write_json(path: Path, value: object) -> None:
    path.write_text(json.dumps(value, indent=2, allow_nan=False) + "\n")


def digest(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


class Plan:
    def __init__(self, level: str = OFFICE):
        self.level = level
        self.steps: list[dict] = []

    def action(self, ms: int, kind: str, **fields: object) -> None:
        self.steps.append(dict(after=f"ready:{self.level}", delay_ms=ms,
                               action=dict(kind=kind, **fields)))

    def key(self, ms: int, key: str, pressed: bool = True) -> None:
        self.action(ms, "key", key=key, pressed=pressed)

    def tap(self, ms: int, key: str) -> None:
        self.key(ms, key)
        self.key(ms + 60, key, False)

    def hold(self, first: int, last: int, key: str) -> None:
        self.key(first, key)
        self.key(last, key, False)

    def capture(self, ms: int, name: str) -> None:
        self.action(ms, "capture", name=name)

    def pose(self, ms: int, position: list[float], yaw: float, pitch: float) -> None:
        self.action(ms, "player", position=position, yaw=yaw, pitch=pitch)

    def aim(self, ms: int, offset: float = 0) -> None:
        self.action(ms, "aim_entity", id=MANNEQUIN, yaw_offset_degrees=offset)

    def command(self, ms: int, text: str) -> None:
        self.action(ms, "text", text=text, submit=True)

    def finish(self, ms: int) -> list[dict]:
        self.action(ms, "quit")
        self.steps.sort(key=lambda step: step["delay_ms"])
        if len(self.steps) > 128 or ms > 60_000:
            raise ValueError("plan exceeds the native action budget")
        return self.steps


def plan_for(mode: str, video_seconds: int = 15) -> list[dict]:
    p = Plan(ARCADE if mode == "doors" else OFFICE)
    if mode == "sweep":
        # All three shapes are existing authored basement models: cabinet at
        # (14.5,10.1), chair at (15.7,10.6), desk at (15.2,11.8). Only initial
        # camera setup is direct; continuous look keys own the visible sweep.
        p.pose(0, [13, 1.6, 12.6], 45, -12); p.tap(100, "F")
        p.hold(600, 1800, "Left"); p.hold(2200, 5200, "Right")
        p.capture(2600, "sweep-cabinet-chair")
        p.hold(5400, 5900, "Up"); p.hold(6300, 7300, "Down")
        p.capture(7500, "sweep-floor-shadows")
        p.hold(7600, 10400, "Left"); p.hold(10700, 11200, "Up")
        p.hold(11600, 13000, "Right"); p.capture(13500, "sweep-materials")
        for step in p.steps:
            step["delay_ms"] = round(step["delay_ms"] * video_seconds / 15)
        return p.finish(video_seconds * 1000 + 4000)
    if mode == "walkthrough":
        p.capture(300, "office-dark")
        # Stop the normally moving actor while it is still in the starting
        # room. Aim changes only the camera; it never moves the mannequin.
        p.aim(350); p.aim(550); p.tap(650, "F")
        p.capture(1100, "mannequin-lit")
        p.hold(1400, 1650, "W"); p.hold(1900, 2150, "S"); p.aim(2300)
        p.capture(2800, "mannequin-stopped")
        p.tap(3500, "F"); p.capture(5000, "mannequin-resumed")
        p.aim(5300); p.tap(5400, "F")
        p.hold(6400, 7000, "Right"); p.hold(7500, 8100, "W")
        p.capture(8500, "office-beam-wall")
        p.hold(9000, 9600, "S"); p.hold(10000, 10600, "Left")
        # Real look input returns toward the failing fixture for its long
        # blackout, short return and next blackout, all with the beam off.
        p.hold(12800, 13535, "Left")
        p.tap(15000, "F"); p.hold(15500, 16400, "Up")
        p.capture(17500, "fluorescent-dark"); p.capture(24650, "fluorescent-return")
        p.capture(26500, "office-prolonged-dark")
        return p.finish(30000)
    if mode == "occlusion":
        p.aim(100); p.tap(200, "F")
        p.capture(800, "visible-first"); p.capture(1200, "visible-held")
        # Check real dim body-edge samples before occlusion releases its route.
        for ms, offset, name in ((1500, 21.8, "edge-a"), (1950, 22.0, "edge-b"),
                                 (2400, 21.8, "edge-c"), (2850, 22.0, "edge-d")):
            p.aim(ms, offset); p.capture(ms + 250, name)
        p.aim(3400, 80); p.capture(4200, "outside-cone")
        p.pose(4500, [4.5, 1.6, 8.3], 90, -9); p.aim(4600)
        p.capture(5400, "wall-blocked")
        p.pose(5700, [8.5, 5.2, 8.3], 0, -85); p.aim(5800)
        p.capture(6600, "floor-blocked")
        p.pose(7000, [6.38, 1.6, 7], 270, 0); p.capture(7600, "near-wall")
        p.pose(8000, [6.38, 1.6, 7], 270, -70); p.capture(8600, "near-wall-floor")
        p.pose(9000, [9.7, 1.6, 3.5], 0, -25); p.capture(9600, "cabinet-shadow")
        p.tap(10000, "F"); p.capture(10600, "cabinet-off")
        return p.finish(11000)
    if mode == "furniture":
        p.aim(100); p.tap(200, "F"); p.capture(800, "body-lit")
        # The actual north cabinet blocks a crouched source; standing sends
        # the same beam over it. Actor pose/movement remain wholly autonomous.
        p.pose(1100, [9.7, 1.6, 0.4], 180, -4); p.aim(1200)
        p.capture(1500, "cabinet-over")
        p.tap(1650, "C"); p.capture(2150, "cabinet-blocked")
        p.tap(2300, "C"); p.aim(2600); p.capture(3000, "cabinet-over-again")
        p.tap(3200, "F"); p.capture(4000, "body-resumed")
        return p.finish(4300)
    if mode == "nearwall":
        # Normal collision stops the eye 30 cm from this authored wall's
        # x=6.12 face. Developer flight approaches inside the 14 cm right
        # attachment, then looks along the wall. This exercises retraction
        # while keeping the eye beyond the renderer's near clip plane.
        p.pose(0, [6.42, 1.6, 7], 270, 0); p.tap(100, "F")
        p.action(200, "console_enabled", enabled=True); p.tap(300, "`")
        p.command(400, "/noclip on"); p.action(500, "escape")
        p.hold(700, 760, "W"); p.hold(900, 1900, "Left")
        p.capture(2150, "retracted-near-wall"); p.capture(2600, "retracted-near-wall-held")
        p.hold(2800, 2870, "Down"); p.capture(3200, "near-wall-angle")
        p.tap(3500, "`"); p.command(3650, "/noclip off"); p.action(3800, "escape")
        return p.finish(4100)
    if mode == "input":
        p.tap(300, "F"); p.capture(650, "f-on")
        p.key(800, "F"); p.key(950, "F"); p.key(1100, "F")
        p.capture(1300, "held-no-repeat"); p.key(1450, "F", False)
        p.tap(1550, "F"); p.capture(1850, "f-on-again")
        p.action(2000, "escape"); p.tap(2120, "Down"); p.tap(2240, "Return")
        p.tap(2360, "Down"); p.tap(2480, "Down"); p.tap(2600, "Return")
        # Flashlight is binding row 11, immediately after Interact. Exercise
        # the real menu rebind path rather than rewriting a live settings file.
        for index in range(11):
            p.tap(2720 + index * 120, "Down")
        p.tap(4100, "Return"); p.tap(4250, "W"); p.capture(4350, "binding-conflict")
        p.tap(4400, "Return"); p.tap(4500, "R")
        p.capture(4650, "rebound-settings")
        for ms in (4950, 5150, 5350):
            p.action(ms, "escape")
        p.tap(5550, "F"); p.capture(5850, "old-key-ignored")
        p.tap(6050, "R"); p.capture(6350, "rebound-off")
        p.action(6550, "console_enabled", enabled=True); p.tap(6700, "`")
        p.tap(6900, "R"); p.command(7100, "/help"); p.tap(7250, "F")
        p.capture(7600, "console-typing")
        p.action(7850, "escape"); p.tap(8050, "R")
        p.capture(8400, "console-closed-on")
        p.action(8700, "escape"); p.tap(8900, "R")
        p.capture(9250, "menu-focus"); p.action(9500, "escape")
        p.action(9750, "focus", focused=False); p.tap(9950, "R")
        p.action(10250, "focus", focused=True); p.capture(10600, "focus-restored")
        p.action(11000, "lightmaps", quality="static"); p.capture(14000, "static-on")
        for ms, quality in ((15000, "low"), (19000, "medium"), (23000, "high")):
            p.action(ms, "quality", level=quality); p.capture(ms + 3000, f"static-{quality}")
        p.action(27000, "lightmaps", quality="full"); p.capture(30000, "full-on")
        p.action(31000, "load", level=OFFICE); p.capture(36500, "reloaded-off")
        p.tap(37500, "R"); p.capture(38000, "reloaded-rebound-on")
        return p.finish(38500)
    if mode == "objects":
        p.action(0, "console_enabled", enabled=True); p.tap(100, "`")
        p.command(250, "/give core:rubber_duck"); p.action(500, "escape")
        p.pose(650, [8.5, 1.6, 9.95], 0, -60)
        p.action(800, "aim_item", id="core:rubber_duck"); p.tap(950, "F")
        p.capture(1350, "dropped-lit"); p.tap(1550, "E")
        p.capture(2000, "held-lit"); p.tap(2250, "E")
        p.action(2450, "item_velocity", id="core:rubber_duck", linear=[0.6, 0.5, 0], angular=[0, 1, 0])
        p.capture(2850, "moving-lit")
        p.action(3300, "lightmaps", quality="static"); p.capture(6300, "object-static")
        p.action(7000, "lightmaps", quality="full"); p.capture(10000, "object-full")
        p.action(11000, "quality", level="low"); p.capture(14000, "object-low-full")
        p.action(15000, "quality", level="high"); p.capture(18000, "object-high-full")
        p.pose(18500, [9.7, 1.6, 3.5], 0, -25); p.capture(19200, "static-furniture-lit")
        p.pose(19800, [8.5, 1.6, 11], 0, -13); p.aim(20000)
        p.capture(20700, "animated-lit"); p.tap(21200, "F")
        p.capture(22500, "animated-resume")
        p.pose(23000, [22.4, 1.6, 6], 0, -22); p.tap(23200, "F")
        p.capture(23700, "water-cooler-on"); p.tap(24000, "F")
        p.capture(24400, "water-cooler-off")
        return p.finish(24800)
    if mode == "console":
        p.tap(100, "`"); p.capture(350, "console-default-disabled")
        p.action(500, "escape"); p.tap(650, "Down"); p.tap(800, "Return")
        p.tap(950, "Down"); p.tap(1100, "Down"); p.tap(1250, "Return")
        for index in range(18):
            p.tap(1400 + index * 100, "Down")
        p.tap(3300, "Return"); p.capture(3650, "console-setting-enabled")
        for ms in (3900, 4100, 4300):
            p.action(ms, "escape")
        p.tap(4500, "`"); p.command(4700, "/unknown_flashlight")
        p.capture(4950, "unknown-command")
        p.command(5150, "/give core:not_an_item"); p.capture(5400, "unknown-item")
        p.command(5600, "/give ../../file"); p.capture(5850, "invalid-item")
        p.command(6050, "/give core:rubber_duck"); p.capture(6400, "console-physical-give")
        p.command(6600, "/noclip on"); p.action(6800, "escape")
        p.hold(7000, 7600, "Space"); p.tap(7800, "`")
        p.capture(8000, "noclip-flying"); p.command(8200, "/noclip off")
        p.capture(8500, "noclip-safe-return"); p.action(8700, "escape")
        return p.finish(9100)
    if mode == "doors":
        p.capture(250, "door-closed-off"); p.tap(400, "F")
        p.capture(900, "door-closed-on")
        p.action(1200, "activate", id="restroom_entry")
        p.capture(2350, "door-open-on"); p.tap(2600, "F")
        p.capture(3000, "door-open-off"); p.tap(3250, "F")
        p.pose(3500, [-0.43, 1.6, 2.8], 90, 0); p.capture(4050, "door-near-open")
        p.pose(4400, [-1.9, 1.6, 1.4], 128, 0)
        p.action(4700, "activate", id="restroom_entry")
        p.capture(5950, "door-closed-again")
        p.action(6100, "console_enabled", enabled=True); p.tap(6250, "`")
        p.command(6450, "/noclip on"); p.action(6650, "escape")
        p.action(6800, "activate", id="restroom_entry")
        p.capture(7950, "door-noclip-open"); p.tap(8100, "`")
        p.command(8250, "/noclip off"); p.action(8400, "escape")
        # Existing restroom planar mirror, with the beam still enabled.
        p.pose(8600, [-1.1, 1.6, 6.4], 180, 0); p.capture(9100, "restroom-mirror-on")
        p.tap(9300, "F"); p.capture(9650, "restroom-mirror-off")
        return p.finish(10000)
    raise ValueError(mode)


def check(checks: list[dict], name: str, passed: bool, **evidence: object) -> None:
    checks.append(dict(name=name, passed=bool(passed), **evidence))


def response(sample: dict) -> dict:
    return next((entry for entry in sample.get("light_responses", [])
                 if entry.get("instance_id") == MANNEQUIN), {})


def distance(a: list[float], b: list[float]) -> float:
    return math.dist(a, b)


def audit(mode: str, samples: dict, checks: list[dict], out: Path) -> None:
    if mode == "sweep":
        for name in ("sweep-cabinet-chair", "sweep-floor-shadows", "sweep-materials"):
            sample = samples.get(name, {})
            check(checks, f"{name}: real Office beam remains on", sample.get("flashlight_on") is True
                  and sample.get("current_level_id") == OFFICE and sample.get("renderer_level_id") == OFFICE)
    if mode in ("walkthrough", "occlusion"):
        names = ("mannequin-lit", "mannequin-stopped") if mode == "walkthrough" else ("visible-first", "visible-held")
        first, held = (response(samples.get(name, {})) for name in names)
        check(checks, "directly illuminated mannequin is stopped", bool(first) and bool(held)
              and first.get("active") is True and held.get("active") is True
              and distance(first["position"], held["position"]) < 0.015)
        if held and held.get("yaw") is not None:
            player = samples[names[1]]["player_position"]
            pos = held["position"]
            wanted = math.atan2(player[0] - pos[0], player[2] - pos[2])
            error = abs(math.atan2(math.sin(held["yaw"] - wanted), math.cos(held["yaw"] - wanted)))
            check(checks, "illuminated mannequin faces the live player", error < math.radians(8), error_degrees=math.degrees(error))
        if mode == "walkthrough":
            resumed = response(samples.get("mannequin-resumed", {}))
            check(checks, "mannequin resumes ordinary movement after beam ends", bool(held) and bool(resumed)
                  and resumed.get("active") is False and distance(held["position"], resumed["position"]) > 0.1)
            lights = json.loads((ROOT / "levels/office_demo.json").read_text())["ceiling_lights"]
            failing = next(index for index, light in enumerate(lights) if light.get("id") == "basement_failing_a")
            for name, enabled in (("fluorescent-dark", False), ("fluorescent-return", True), ("office-prolonged-dark", False)):
                sample = samples.get(name, {})
                check(checks, f"{name}: real failing light state with beam off", sample.get("flashlight_on") is False
                      and dict(sample.get("light_states", [])).get(failing) is enabled)
        else:
            for name in ("wall-blocked", "floor-blocked", "outside-cone"):
                seen = response(samples.get(name, {}))
                check(checks, f"{name}: geometry/cone prevents response", bool(seen)
                      and seen.get("active") is False and seen.get("intensity", 1) < 0.001)
            edges = [response(samples.get(f"edge-{letter}", {})) for letter in "abcd"]
            states = [entry.get("active") for entry in edges]
            drift = max((distance(edges[0]["position"], entry["position"])
                         for entry in edges if entry), default=math.inf) if all(edges) else math.inf
            check(checks, "nearby beam-edge samples avoid repeated state twitching", all(edges)
                  and sum(a != b for a, b in zip(states, states[1:])) <= 1
                  and all(states) and drift < 0.03
                  and any(entry.get("intensity", 0) > 0.0 for entry in edges)
                  and any(0.005 < entry.get("intensity", 0) < 0.035 for entry in edges)
                  and max(entry.get("intensity", 0) for entry in edges) < first.get("intensity", 0) * 0.9,
                  states=states, maximum_position_drift_metres=drift)
    if mode == "furniture":
        for name in ("body-lit", "cabinet-over", "cabinet-over-again"):
            seen = response(samples.get(name, {}))
            check(checks, f"{name}: visible actor receives real beam", bool(seen) and seen.get("active") is True)
        blocked = response(samples.get("cabinet-blocked", {}))
        check(checks, "actual cabinet geometry prevents crouched-source reaction", bool(blocked)
              and blocked.get("active") is False and blocked.get("intensity", 1) < 0.001)
        check(checks, "cabinet case uses normal crouch", samples.get("cabinet-blocked", {}).get("player_body", {}).get("crouched") is True)
        check(checks, "beam-off releases normal route", response(samples.get("body-resumed", {})).get("active") is False)
    if mode == "nearwall":
        first, held = (samples.get(name, {}) for name in ("retracted-near-wall", "retracted-near-wall-held"))
        x = first.get("player_position", [math.inf])[0]
        check(checks, "native eye is outside wall but within the right attachment", first.get("noclip") is True
              and first.get("flashlight_on") is True and 6.20 < x < 6.26
              and math.cos(first.get("player_yaw", 0)) < -0.97, eye_x=x, authored_wall_face_x=6.12)
        check(checks, "close source remains steady", bool(first) and bool(held)
              and distance(first["player_position"], held["player_position"]) < 0.002)
    if mode == "input":
        expected = {"f-on": True, "held-no-repeat": False, "f-on-again": True,
                    "old-key-ignored": True, "rebound-off": False, "console-typing": False,
                    "console-closed-on": True, "menu-focus": True, "focus-restored": True,
                    "static-on": True, "full-on": True, "reloaded-off": False,
                    "reloaded-rebound-on": True}
        for name, on in expected.items():
            check(checks, f"{name}: flashlight state", samples.get(name, {}).get("flashlight_on") is on)
        saved = json.loads((out / "settings.json").read_text())
        check(checks, "actual Controls rebind persists R", saved.get("bindings", {}).get("flashlight") == "R")
        check(checks, "console captures text focus", samples.get("console-typing", {}).get("console_open") is True)
        for quality in ("low", "medium", "high"):
            sample = samples.get(f"static-{quality}", {})
            check(checks, f"{quality} textures preserve applied Static lighting", sample.get("quality") == quality
                  and sample.get("renderer_quality") == quality and sample.get("renderer_lightmaps") == "off"
                  and sample.get("flashlight_on") is True)
        check(checks, "live Full lighting reapplies while beam stays on", samples.get("full-on", {}).get("renderer_lightmaps") == "full")
    if mode == "objects":
        check(checks, "given item is held through ordinary E input", samples.get("held-lit", {}).get("physics", {}).get("held") is not None)
        check(checks, "ordinary E drops the lit item", samples.get("moving-lit", {}).get("physics", {}).get("held") is None)
        for name, lighting, quality in (("object-static", "off", "high"), ("object-full", "full", "high"),
                                       ("object-low-full", "full", "low"), ("object-high-full", "full", "high")):
            sample = samples.get(name, {})
            check(checks, f"{name}: applied lighting/texture combination", sample.get("renderer_lightmaps") == lighting
                  and sample.get("renderer_quality") == quality and sample.get("flashlight_on") is True)
    if mode == "console":
        check(checks, "backtick is inactive before console is enabled", samples.get("console-default-disabled", {}).get("console_open") is False)
        saved = json.loads((out / "settings.json").read_text())
        check(checks, "actual Controls console setting persists", saved.get("developer_console") is True)
        for name in ("unknown-command", "unknown-item", "invalid-item"):
            sample = samples.get(name, {})
            check(checks, f"{name}: console handles rejection without a spawned body", sample.get("console_open") is True
                  and bool(sample.get("console_last")) and not sample.get("physics", {}).get("bodies"))
        given = samples.get("console-physical-give", {}).get("physics", {}).get("bodies", [])
        check(checks, "give creates a real physics body", len(given) == 1)
        flying, returned = (samples.get(name, {}) for name in ("noclip-flying", "noclip-safe-return"))
        check(checks, "noclip flies then restores a safe body", flying.get("noclip") is True
              and returned.get("noclip") is False and returned.get("player_body", {}).get("grounded") is True)
    if mode == "doors":
        for name, open_door in (("door-closed-on", False), ("door-open-on", True),
                                ("door-closed-again", False), ("door-noclip-open", True)):
            door = next((entry for entry in samples.get(name, {}).get("doors", []) if entry["id"] == "restroom_entry"), {})
            angle = abs(door.get("angle_degrees", -999))
            check(checks, f"{name}: actual authored door pose", bool(door) and (angle > 80 if open_door else angle < 0.1), angle=angle)
        check(checks, "door animation advances while noclip is enabled", samples.get("door-noclip-open", {}).get("noclip") is True)


def make_bundle(out: Path, asset_root: Path, package: Path, level: str) -> Path:
    assets = out / "assets"
    assets.mkdir()
    for entry in sorted((asset_root / "assets").iterdir()):
        if entry.name != "levels":
            target = entry.resolve(strict=True)
            (assets / entry.name).symlink_to(target, target_is_directory=target.is_dir())
    (assets / "levels").mkdir()
    installed = assets / "levels" / f"{level}.placesmap"
    shutil.copyfile(package, installed)
    return installed


def video_command(args: argparse.Namespace, out: Path) -> list[str]:
    return [str(args.ffmpeg), "-hide_banner", "-nostdin", "-n", "-f", "rawvideo", "-pixel_format", "rgba",
            "-video_size", args.expected_drawable, "-framerate", "30", "-i", str(out / "native-video.fifo"),
            "-an", "-c:v", "libx264", "-preset", "veryfast", "-crf", "18", "-threads", "2", "-vf",
            "scale=in_range=pc:out_range=tv:out_color_matrix=bt709,format=yuv420p,setparams=range=limited:color_primaries=bt709:color_trc=bt709:colorspace=bt709",
            "-colorspace", "bt709", "-color_primaries", "bt709", "-color_trc", "bt709", "-color_range", "tv",
            "-movflags", "+faststart", str(video_path(args, out))]


def video_path(args: argparse.Namespace, out: Path) -> Path:
    return out / ("office-flashlight-sweep-native.mp4" if args.mode == "sweep" else "office-flashlight-native.mp4")


def audit_video(args: argparse.Namespace, out: Path, events: list[dict], checks: list[dict]) -> dict:
    frames = [json.loads(event["detail"]) for event in events if event["event"] == "native_video_frame"]
    sources = [frame["source_ready_frame"] for frame in frames]
    count = args.video_seconds * 30
    check(checks, "video records consecutive actual native source frames", len(frames) == count
          and [frame["captured"] for frame in frames] == list(range(1, count + 1))
          and bool(sources) and sources[0] == 1 and all(b - a == 2 for a, b in zip(sources, sources[1:])))
    path = video_path(args, out)
    probe = subprocess.run([str(args.ffprobe), "-v", "error", "-select_streams", "v:0", "-show_entries",
                            "stream=codec_name,pix_fmt,width,height,nb_frames,avg_frame_rate,color_space,color_transfer,color_primaries,color_range:format=duration,size",
                            "-of", "json", str(path)], capture_output=True, text=True, timeout=15)
    (out / "video-probe.log").write_text(probe.stdout + probe.stderr)
    metadata = json.loads(probe.stdout) if probe.returncode == 0 else {}
    streams = metadata.get("streams", [])
    stream = streams[0] if len(streams) == 1 else {}
    check(checks, "native video preserves every requested frame at 1280x720 / 30 fps", probe.returncode == 0
          and stream.get("codec_name") == "h264" and stream.get("pix_fmt") == "yuv420p"
          and (stream.get("width"), stream.get("height")) == (1280, 720)
          and int(stream.get("nb_frames", 0)) == count and stream.get("avg_frame_rate") == "30/1"
          and abs(float(metadata.get("format", {}).get("duration", 0)) - args.video_seconds) < 0.05)
    check(checks, "video explicitly converts and declares BT.709 limited range", all(stream.get(key) == "bt709"
          for key in ("color_space", "color_transfer", "color_primaries")) and stream.get("color_range") == "tv")
    return dict(path=str(path), sha256=digest(path) if path.is_file() else None, metadata=metadata,
                first_source_ready_frame=sources[0] if sources else None, last_source_ready_frame=sources[-1] if sources else None)


def run_native(args: argparse.Namespace, out: Path) -> None:
    level = ARCADE if args.mode == "doors" else OFFICE
    package = args.arcade_package if args.mode == "doors" else args.package
    installed = make_bundle(out, args.asset_root, package, level)
    width, height = map(int, args.size.split("x"))
    settings = json.loads((ROOT / "tests/fixtures/native/retro-arcade-manifest.json").read_text())["settings"]
    settings["bindings"]["flashlight"] = "F"
    # The explicit recorder paces native source frames; FIFO presentation
    # would add another wait and shorten the requested consecutive-frame clip.
    settings.update(window_width=width, window_height=height, window_mode="windowed", vsync=args.mode not in ("walkthrough", "sweep"),
                    developer_console=False, quality="high", texture_filtering="high", lightmaps="full",
                    reflections="full", bloom=True, use_low_quality_lighting=False, fov_degrees=60)
    if args.mode == "sweep":
        settings.update(look_speed_h=24 * 15 / args.video_seconds, look_speed_v=35 * 15 / args.video_seconds)
    steps = plan_for(args.mode, args.video_seconds)
    write_json(out / "settings.json", settings); write_json(out / "actions.json", steps)
    env = {key: value for key, value in os.environ.items() if not key.startswith("PLACES_") and key != "SDL_VIDEODRIVER"}
    # PLACES_CAMERA pins the rendered view independently of gameplay looking;
    # leave it unset so key turns and AimEntity remain visible in these runs.
    spawn = "-1.9,1.6,1.4,128" if level == ARCADE else VIEWS["dark"][0]
    if args.mode == "sweep":
        spawn = "13,1.6,12.6,45"
    env.update(PLACES_ASSET_ROOT=str(out), PLACES_STATE_ROOT=str(out), PLACES_LEVEL=level,
               PLACES_SPAWN=spawn, PLACES_BENCH="1", PLACES_VERBOSE="1",
               PLACES_BENCH_ACTIONS=str(out / "actions.json"), PLACES_LOAD_TRACE=str(out / "trace.jsonl"),
               PLACES_STATE_LOG=str(out / "player.csv"), PLACES_BENCH_OUT=str(out / "frames.csv"),
               PLACES_BENCH_FRAMES="3600", PLACES_BENCH_WARMUP="120")
    command = video_command(args, out) if args.mode in ("walkthrough", "sweep") else None
    if command:
        env.update(PLACES_BENCH_FIXED_DELTA_SECONDS="0.016666667", PLACES_BENCH_VIDEO_PIPE=str(out / "native-video.fifo"), PLACES_BENCH_VIDEO_SIZE=args.expected_drawable,
                   PLACES_BENCH_VIDEO_FRAMES=str(args.video_seconds * 30), PLACES_BENCH_VIDEO_STRIDE="2", PLACES_BENCH_VIDEO_START_FRAME="0")
    receipt = dict(mode=args.mode, status="planned", binary=str(args.binary),
                   binary_sha256=digest(args.binary) if args.binary.is_file() else None,
                   package=str(package), package_sha256=digest(package), settings=settings,
                   environment={key: value for key, value in env.items() if key.startswith("PLACES_")},
                   actions=len(steps), checks=[], screenshots=[], snapshots={},
                   video=dict(encoder_command=command) if command else None,
                   visual_review="Required: cone/falloff, near-wall placement, shadows, reflections/transparency, legitimate darkness and animation quality.",
                   scope="Explicit native diagnostics; camera/pose setup is visible in actions.json. No map bake or whole-map CI test.")
    write_json(out / "receipt.json", receipt)
    if args.plan_only:
        return
    encoder = encoder_log = None
    started = time.monotonic()
    receipt["started_utc"] = datetime.now(timezone.utc).isoformat()
    try:
        if command:
            args.ffmpeg.resolve(strict=True); args.ffprobe.resolve(strict=True)
            os.mkfifo(out / "native-video.fifo", mode=0o600)
            encoder_log = (out / "video-encoder.log").open("wb")
            encoder = subprocess.Popen(command, cwd=ROOT, stdin=subprocess.DEVNULL, stdout=encoder_log, stderr=subprocess.STDOUT)
        result = subprocess.run([str(args.binary)], cwd=ROOT, env=env, capture_output=True, text=True, timeout=180)
        log = result.stdout + result.stderr
        (out / "native.log").write_text(log)
        receipt.update(exit_code=result.returncode, elapsed_seconds=time.monotonic() - started)
        checks = receipt["checks"]
        if encoder:
            encoder_exit = encoder.wait(timeout=30)
            receipt["video"]["encoder_exit_code"] = encoder_exit
            check(checks, "native video encoder exits successfully", encoder_exit == 0)
        check(checks, "native process exits successfully", result.returncode == 0)
        check(checks, "binary/package remain unchanged", digest(args.binary) == receipt["binary_sha256"]
              and digest(package) == digest(installed) == receipt["package_sha256"])
        check(checks, "expected map commits without native validation failures", f"[loading] committed {level}" in log
              and not any(word in log.lower() for word in ("validation error", "panicked", "fatal device", "catalog hash mismatch")))
        drawables = [list(map(int, match)) for match in re.findall(r"^\[(?:renderer|window)\].*drawable[: ]+(\d+)x(\d+)", log, re.MULTILINE)]
        expected = list(map(int, args.expected_drawable.split("x"))) if args.expected_drawable else None
        receipt["native_drawables"] = drawables
        receipt["native_identity"] = [line for line in log.splitlines() if line.startswith(("[renderer]", "[window]", "[settings]", "[wgpu] GPU timing"))]
        if expected:
            check(checks, "actual drawable matches request", bool(drawables) and all(size == expected for size in drawables))
        trace = out / "trace.jsonl"
        if trace.stat().st_size > 64 * 1024 * 1024:
            raise ValueError("native trace exceeds bounded 64 MiB budget")
        events = [json.loads(line) for line in trace.read_text().splitlines()]
        samples = {event["event"].split(":", 1)[1]: json.loads(event["detail"]) for event in events if event["event"].startswith("native_capture:")}
        seen = {json.loads(event["detail"])["id"] for event in events if event["event"] == "action"}
        check(checks, "every bounded native action executes", seen == set(range(len(steps))))
        presented = sum(event["event"] == "present" and event["detail"] == "ready" for event in events)
        missed = sum(event["event"] == "surface_not_presented" and event["detail"] == "ready" for event in events)
        check(checks, "native ready frames present without missed surfaces", presented > 0 and missed == 0, presented=presented, missed=missed)
        for step in steps:
            action = step["action"]
            if action["kind"] != "capture":
                continue
            path = out / f"{action['name']}.png"
            if path.is_file():
                with path.open("rb") as stream:
                    header = stream.read(24)
            else:
                header = b""
            valid = len(header) == 24 and header[:8] == b"\x89PNG\r\n\x1a\n"
            size = [int.from_bytes(header[16:20], "big"), int.from_bytes(header[20:24], "big")] if valid else None
            check(checks, f"{action['name']}: native capture and snapshot exist", valid and (expected is None or size == expected) and action["name"] in samples)
            if valid:
                receipt["screenshots"].append(dict(name=action["name"], path=str(path), size=size, sha256=digest(path)))
        audit(args.mode, samples, checks, out)
        receipt["snapshots"] = samples
        if command:
            receipt["video"].update(audit_video(args, out, events, checks))
        receipt["status"] = "native_checks_passed_visual_review_required" if all(entry["passed"] for entry in checks) else "failed"
        write_json(out / "receipt.json", receipt)
        if receipt["status"] == "failed":
            raise RuntimeError(f"native acceptance checks failed; inspect {out / 'receipt.json'}")
    except subprocess.TimeoutExpired as error:
        def timeout_text(value: str | bytes | None) -> str:
            return value.decode(errors="replace") if isinstance(value, bytes) else value or ""

        (out / "native.log").write_text(timeout_text(error.stdout) + timeout_text(error.stderr))
        receipt.update(status="failed", error=str(error))
        write_json(out / "receipt.json", receipt)
        raise
    except Exception as error:
        receipt.update(status="failed", error=str(error))
        write_json(out / "receipt.json", receipt)
        raise
    finally:
        if encoder and encoder.poll() is None:
            encoder.terminate()
            try:
                encoder.wait(timeout=5)
            except subprocess.TimeoutExpired:
                encoder.kill(); encoder.wait(timeout=5)
        if encoder_log:
            encoder_log.close()
        (out / "native-video.fifo").unlink(missing_ok=True)


def run_performance(args: argparse.Namespace, out: Path) -> None:
    make_bundle(out, args.asset_root, args.package, OFFICE)
    commands, results = [], []
    # Reverse order in the second repeat; keep camera, package, quality, frame
    # budget and asynchronous GPU timing identical. Never record video here.
    for repeat in range(2):
        for view, (spawn, camera) in VIEWS.items():
            for on in ((False, True) if repeat == 0 else (True, False)):
                run = out / f"{view}-{'on' if on else 'off'}-{repeat + 1}"
                command = [sys.executable, str(ROOT / "tools/bench/frame_time.py"), "--binary", str(args.binary),
                           "--asset-root", str(out), "--out", str(run), "--level", OFFICE, f"--spawn={spawn}",
                           f"--camera={camera}", "--size", args.size, "--frames", "600", "--warmup", "120",
                           "--lightmaps", args.lightmaps]
                if args.expected_drawable:
                    command += ["--expected-drawable", args.expected_drawable]
                if on:
                    command += ["--move-script", "flashlight@0-0.035"]
                commands.append(dict(view=view, flashlight_on=on, repeat=repeat + 1, command=command, receipt=str(run / "receipt.json")))
    for view, (spawn, camera) in VIEWS.items():
        run = out / f"{view}-off-after-on"
        command = [sys.executable, str(ROOT / "tools/bench/frame_time.py"), "--binary", str(args.binary),
                   "--asset-root", str(out), "--out", str(run), "--level", OFFICE, f"--spawn={spawn}",
                   f"--camera={camera}", "--size", args.size, "--frames", "600", "--warmup", "120",
                   "--lightmaps", args.lightmaps, "--move-script", "flashlight@0-0.035,flashlight@0.5-0.535"]
        if args.expected_drawable:
            command += ["--expected-drawable", args.expected_drawable]
        commands.append(dict(view=view, flashlight_on=False, repeat=None, off_after_on=True,
                             command=command, receipt=str(run / "receipt.json")))
    receipt = dict(mode="performance", status="planned", commands=commands, binary=str(args.binary),
                   binary_sha256=digest(args.binary) if args.binary.is_file() else None,
                   package=str(args.package), package_sha256=digest(args.package), frames=600, warmup=120,
                   summary_method="Means of two paired run medians/p95/p99; maximum of the two worst frames. OFF-after-ON controls remain separate. Individual distributions remain in results.",
                   limitations="Matched fixed views on this host. Unique asynchronous GPU elapsed samples are render-pass timings; no universal frame-rate guarantee.", results=[], comparisons=[])
    write_json(out / "receipt.json", receipt)
    if args.plan_only:
        return
    for entry in commands:
        result = subprocess.run(entry["command"], cwd=ROOT, capture_output=True, text=True, timeout=300)
        suffix = "off-after-on" if entry.get("off_after_on") else f"{'on' if entry['flashlight_on'] else 'off'}-{entry['repeat']}"
        (out / f"{entry['view']}-{suffix}.log").write_text(result.stdout + result.stderr)
        if result.returncode:
            receipt.update(status="failed", error=f"frame_time failed: {entry['receipt']}", results=results)
            write_json(out / "receipt.json", receipt)
            raise RuntimeError(receipt["error"])
        native = json.loads(Path(entry["receipt"]).read_text())
        trace = Path(entry["receipt"]).parent / "trace.jsonl"
        states = [(event["event"], json.loads(event["detail"]).get("flashlight_on"))
                  for line in trace.read_text().splitlines()
                  if (event := json.loads(line))["event"] in ("flashlight_toggled", "world_committed", "benchmark_complete")]
        toggles = [on for event, on in states if event == "flashlight_toggled"]
        expected_toggles = [True, False] if entry.get("off_after_on") else [True] if entry["flashlight_on"] else []
        control = toggles == expected_toggles
        control = control and ("benchmark_complete", entry["flashlight_on"]) in states
        results.append(dict(view=entry["view"], flashlight_on=entry["flashlight_on"], repeat=entry["repeat"],
                            off_after_on=entry.get("off_after_on", False),
                            receipt=entry["receipt"], cpu_ms=native["metrics"], gpu_elapsed_ms=native["gpu_elapsed_ms"],
                            flashlight_state_verified=control, state_witnesses=states,
                            draw_calls=native["base_scene_draws"], reflection_passes=native["reflection_passes"],
                            peak_native_rss_bytes=native["peak_native_rss_bytes"]))
        receipt["results"] = results
        write_json(out / "receipt.json", receipt)
    for view in VIEWS:
        controls = {on: [row for row in results if row["view"] == view and row["flashlight_on"] is on
                        and not row["off_after_on"]] for on in (False, True)}
        comparison = dict(view=view, cpu_ms={}, gpu_elapsed_ms={})
        for label, source in (("cpu_ms", "cpu_ms"), ("gpu_elapsed_ms", "gpu_elapsed_ms")):
            keys = ("loop_ms", "frame_ms", "update_ms", "render_ms", "swap_ms") if label == "cpu_ms" else ("render_pass",)
            for key in keys:
                target = comparison[label][key] = {}
                for metric in ("median", "p95", "p99", "worst"):
                    values = {}
                    for on in (False, True):
                        records = [row[source][key] if label == "cpu_ms" else row[source] for row in controls[on]]
                        valid = [record[metric] for record in records if record is not None]
                        values[on] = (max(valid) if metric == "worst" else statistics.mean(valid)) if len(valid) == 2 else None
                    target[metric] = dict(off=values[False], on=values[True], delta=None if None in values.values() else values[True] - values[False])
        receipt["comparisons"].append(comparison)
    receipt["status"] = "measured" if all(row["flashlight_state_verified"] and row["gpu_elapsed_ms"] for row in results) else "needs_attention"
    write_json(out / "receipt.json", receipt)
    if receipt["status"] != "measured":
        raise RuntimeError("flashlight state witness or hardware GPU timings unavailable; inspect receipt")


def size(value: str) -> str:
    if not re.fullmatch(r"\d+x\d+", value) or not all(64 <= edge <= 8192 for edge in map(int, value.split("x"))):
        raise argparse.ArgumentTypeError("size must be WIDTHxHEIGHT with edges 64..8192")
    return value


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mode", required=True, choices=("walkthrough", "sweep", "occlusion", "furniture", "nearwall", "input", "objects", "console", "doors", "performance"))
    parser.add_argument("--binary", type=Path, default=ROOT / "target/release/places")
    parser.add_argument("--asset-root", type=Path, default=ROOT)
    parser.add_argument("--package", type=Path, default=ROOT / "assets/levels/office_demo.placesmap")
    parser.add_argument("--arcade-package", type=Path, default=ROOT / "assets/levels/retro_arcade_demo.placesmap")
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--size", type=size, default="640x360", help="logical window; 640x360 is a 1280x720 drawable on a 2x display")
    parser.add_argument("--expected-drawable", type=size)
    parser.add_argument("--lightmaps", choices=("static", "full"), default="full", help="matched performance lighting path")
    parser.add_argument("--ffmpeg", type=Path, default=Path("/opt/homebrew/bin/ffmpeg"))
    parser.add_argument("--ffprobe", type=Path, default=Path("/opt/homebrew/bin/ffprobe"))
    parser.add_argument("--video-seconds", type=int, help="walkthrough: 24; sweep: 8..15 (default 15)")
    parser.add_argument("--plan-only", action="store_true")
    args = parser.parse_args()
    args.video_seconds = args.video_seconds if args.video_seconds is not None else (15 if args.mode == "sweep" else 24)
    if args.mode in ("walkthrough", "sweep"):
        minimum, maximum = (8, 15) if args.mode == "sweep" else (24, 24)
        if not minimum <= args.video_seconds <= maximum:
            parser.error(f"{args.mode} video seconds must be {minimum}..{maximum}")
    args.binary = args.binary.resolve(strict=not args.plan_only)
    args.asset_root = args.asset_root.resolve(strict=True)
    args.package = args.package.resolve(strict=args.mode != "doors")
    args.arcade_package = args.arcade_package.resolve(strict=args.mode == "doors")
    if args.mode in ("walkthrough", "sweep") and args.expected_drawable != "1280x720":
        parser.error("native video requires --expected-drawable 1280x720; use --size 640x360 on a 2x display")
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    if args.mode == "performance":
        run_performance(args, out)
    else:
        run_native(args, out)
    print(f"{args.mode}: {'planned without native execution' if args.plan_only else 'native evidence saved'} at {out}")


if __name__ == "__main__":
    main()
