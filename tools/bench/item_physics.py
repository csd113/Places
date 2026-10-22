#!/usr/bin/env python3
"""Opt-in native item acceptance and matched bathroom-view performance checks.

Compile tests/fixtures/native/item-physics.level.json explicitly first, then:
  python3 tools/bench/item_physics.py --mode pickup --package /path/to/fixture.placesmap
  python3 tools/bench/item_physics.py --mode stress --package /path/to/fixture.placesmap --performance
  python3 tools/bench/item_physics.py --mode timesteps --package /path/to/fixture.placesmap --sim-delta 0.5
  python3 tools/bench/item_physics.py --mode video --package /path/to/fixture.placesmap --size 640x360
  python3 tools/bench/item_physics.py --mode lighting --package /path/to/new-v4-fixture.placesmap
  python3 tools/bench/item_physics.py --mode lighting-ball --package /path/to/new-v4-fixture.placesmap
  python3 tools/bench/item_physics.py --mode contacts --package /path/to/fixture.placesmap
  python3 tools/bench/item_physics.py --mode escape --package /path/to/fixture.placesmap --performance
  python3 tools/bench/item_physics.py --mode targeting --package /path/to/fixture.placesmap
  python3 tools/bench/item_physics.py --mode dormancy --package /path/to/fixture.placesmap --performance
  python3 tools/bench/item_physics.py --mode surfaces --package /path/to/item-surfaces.level.placesmap
  python3 tools/bench/item_physics.py --mode bowling --package /path/to/new-retro_arcade_demo.placesmap --performance
  python3 tools/bench/item_physics.py --mode bowling --package /path/to/new-retro_arcade_demo.placesmap --size 640x360 --expected-drawable 1280x720 --record-video
  python3 tools/bench/item_physics.py --mode compare --binary /path/to/new/places

This driver never builds Rust or bakes maps. Native key/text actions use the
production SDL handler. Player poses use the existing benchmark player reset
seam; captures are actual native frames. Each run owns a fresh state directory,
copied fixture package, settings, plans, logs, hashes, assertions and receipt.
The comparison delegates to frame_time.py at its established bathroom-door view.
The surface mode requires the separate one-body item-surfaces fixture. Lighting
requires a fresh compiler bake containing switchable indirect probe planes.
Labelled item_pose/item_velocity actions establish physical test conditions;
all measured motion, ownership, lighting and screenshots come from native play.
The authored fixtures start dormant. Existing gravity cases activate the actual
authored group binding; dormancy and bowling exercise the untouched startup.
Video recording streams consecutive native RGBA frames through a private FIFO
to ffmpeg; it has no PNG intermediates and retains encoder/probe evidence.
"""
from __future__ import annotations

import argparse
import csv
from datetime import datetime, timezone
from functools import lru_cache
import hashlib
import json
import math
import os
from pathlib import Path
import re
try:
    import resource
except ImportError:
    resource = None  # Unix child-resource counters are unavailable on Windows.
import shutil
import statistics
import struct
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[2]
FIXTURE = ROOT / "tests/fixtures/native/item-physics.level.json"
SURFACE_FIXTURE = ROOT / "tests/fixtures/native/item-surfaces.level.json"
BOWLING_FIXTURE = ROOT / "levels/retro_arcade_demo.json"
LEVEL = "item_physics_native"
SURFACE_LEVEL = "item_surfaces_native"
BOWLING_LEVEL = "retro_arcade_demo"
BOWLING_GROUP = "arcade_lane_1"
BOWLING_BALL = "rack_0_ball_2"
BOWLING_RESET = "lane_1_reset_switch"
CUP_MODEL = "arcade:party_cup_confetti"
RUN_ROOT = ROOT / "local/item-physics-0.10.0"
BASELINE = ROOT / "local/developer-console-0.10.0/bin/places"
HOLD_DISTANCE = 0.85
HOLD_RIGHT = 0.0
BODY_COUNT = 4
COLLISION_TOLERANCE_M = 0.006
ESCAPE_PIN = "arcade:bowling_pin"


def escape_cases() -> tuple[dict, ...]:
    """Prescribed diagnostic starts; the native solver owns every trajectory."""
    sideways = [0, 0, math.sqrt(0.5), math.sqrt(0.5)]
    return (
        dict(name="cup-floor", id="pickup_cup", position=[-0.9, 1.4, 1.0], rotation=sideways,
             linear=[0, -20, 0], barrier="floor"),
        dict(name="ball-floor", id="stairs_ball", position=[6.5, 1.4, 1.5], rotation=[0, 0, 0, 1],
             linear=[0, -20, 0], barrier="floor"),
        # Door x is its hinge. Aim inside the +X leaf, accounting for the
        # sideways cup's centre offset, rather than hitting the adjacent wall.
        dict(name="cup-thin-door", id="pickup_cup", position=[-1.8, 1.2, -2.2], rotation=sideways,
             linear=[0, 0, -20], barrier="door"),
        dict(name="cup-west-wall", id="pickup_cup", position=[-2.0, 1.1, -0.7], rotation=sideways,
             linear=[-20, 0, 0], barrier="west"),
        dict(name="ball-corner", id="stairs_ball", position=[-2.0, 1.1, 2.0], rotation=[0, 0, 0, 1],
             linear=[-14, 0, 14], barrier="corner"),
        dict(name="pin-ramp", id=ESCAPE_PIN, position=[2.0, 1.6, 2.05], rotation=sideways,
             linear=[0, -20, 0], barrier="ramp"),
        dict(name="ball-stair", id="stairs_ball", position=[-1.47, 1.5, 2.05], rotation=[0, 0, 0, 1],
             linear=[0, -20, 0], barrier="stair"),
    )


def digest(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def write_json(path: Path, value: object) -> None:
    path.write_text(json.dumps(value, indent=2, allow_nan=False) + "\n")


def pitch_to(eye_y: float, target_y: float, distance: float) -> float:
    return math.degrees(math.atan2(target_y - eye_y, distance))


def reserve_capture_latency(steps: list[dict]) -> list[dict]:
    """Keep real key edges apart while a large fullscreen PNG blocks the loop."""
    out = []
    offset = 0
    ready_after = 0
    for step in steps:
        deadline = step["delay_ms"] + offset
        if deadline < ready_after:
            offset += ready_after - deadline
            deadline = ready_after
        out.append({**step, "delay_ms": deadline})
        if step["action"]["kind"] == "capture":
            ready_after = deadline + 1000
    if out and out[-1]["delay_ms"] > 60_000:
        raise ValueError("fullscreen capture spacing exceeds the native 60000 ms action budget")
    return out


class Plan:
    def __init__(self, level: str = LEVEL) -> None:
        self.steps: list[dict] = []
        self.level = level

    def action(self, ms: int, kind: str, **fields: object) -> None:
        self.steps.append(dict(after=f"ready:{self.level}", delay_ms=ms,
                               action=dict(kind=kind, **fields)))

    def key(self, ms: int, key: str, pressed: bool = True) -> None:
        self.action(ms, "key", key=key, pressed=pressed)

    def tap(self, ms: int, key: str) -> None:
        self.key(ms, key)
        self.key(ms + 50, key, False)

    def player(self, ms: int, x: float, z: float, yaw: float, pitch: float = 0) -> None:
        self.action(ms, "player", position=[x, 1.6, z], yaw=yaw, pitch=pitch)

    def capture(self, ms: int, name: str) -> None:
        self.action(ms, "capture", name=name)

    def command(self, ms: int, text: str) -> None:
        self.action(ms, "text", text=text, submit=True)

    def mouse(self, ms: int, pressed: bool = True, recapture: bool = False) -> None:
        self.action(ms, "mouse", button="left", pressed=pressed, recapture=recapture)

    def tap_mouse(self, ms: int, recapture: bool = False) -> None:
        self.mouse(ms, recapture=recapture)
        self.mouse(ms + 50, pressed=False)

    def item_pose(self, ms: int, position: list[float], rotation: list[float], id: str = "pickup_cup") -> None:
        self.action(ms, "item_pose", id=id, position=position, rotation=rotation)

    def item_velocity(self, ms: int, id: str, linear: list[float]) -> None:
        self.action(ms, "item_velocity", id=id, linear=linear, angular=[0, 0, 0])

    def aim_item(self, ms: int, id: str = "pickup_cup") -> None:
        self.action(ms, "aim_item", id=id)

    def initial_pickup(self) -> None:
        self.capture(500, "initial")
        self.player(650, 0, 0.65, 0, pitch_to(1.6, 0.811, 1.3))
        self.tap(900, "E")
        self.capture(1200, "held")

    def finish(self, ms: int) -> list[dict]:
        self.action(ms, "quit")
        self.steps.sort(key=lambda step: step["delay_ms"])
        if not 1 <= len(self.steps) <= 128:
            raise ValueError("native action plan must contain 1..128 actions")
        if any(step["delay_ms"] > 60_000 for step in self.steps):
            raise ValueError("native action delay exceeds 60000 ms")
        if len(json.dumps(self.steps).encode()) > 65_536:
            raise ValueError("native action plan exceeds 64 KiB")
        return self.steps


def plan_for(mode: str) -> list[dict]:
    p = Plan(SURFACE_LEVEL if mode == "surfaces" else BOWLING_LEVEL if mode == "bowling" else LEVEL)
    if mode not in ("bowling", "dormancy"):
        # Native gravity cases explicitly activate their authored group.
        # Ordinary maps, including the bowling lane, keep the dormant default.
        p.action(100, "activate", id="surface_activate" if mode == "surfaces" else "fixture_activate")
    if mode == "dormancy":
        p.capture(500, "initial")
        p.capture(1300, "dormant-wait")
        p.aim_item(1500)
        p.capture(1650, "dormant-aim")
        p.tap(1750, "E")
        p.capture(2150, "dormant-held")
        p.action(2450, "quality", level="medium")
        p.capture(4250, "dormant-medium")
        p.action(4400, "quality", level="high")
        p.capture(6400, "dormant-high")
        p.player(6550, -1.8, 0.8, 270, pitch_to(1.6, 1.29, 1.15))
        p.capture(6800, "dormant-held-facing-trigger")
        p.player(6900, 0, 0.65, 90)
        p.capture(7000, "dormant-held-away")
        p.tap(7050, "E")
        p.capture(7300, "dormant-drop")
        p.player(7500, -1.8, 0.8, 270, pitch_to(1.6, 1.29, 1.15))
        p.capture(7750, "dormant-trigger-target")
        p.tap(7850, "E")
        p.capture(8200, "dormant-activated")
        p.capture(10_200, "dormant-fallen")
        p.player(10_350, -1.8, 0, 270, pitch_to(1.6, 1.29, 1.15))
        p.capture(10_600, "dormant-reset-target")
        p.tap(10_700, "E")
        p.capture(11_150, "dormant-reset")
        p.capture(12_600, "dormant-reset-wait")
        return p.finish(12_800)
    if mode == "bowling":
        switch = next(p for p in json.loads(BOWLING_FIXTURE.read_text())['props'] if p.get('id') == BOWLING_RESET)
        switch_x, switch_z = switch['x'], switch['z']
        switch_y = switch['y'] + .06
        def pin_camera(ms: int) -> None:
            # Stand on the ordinary inter-lane deck, ahead and to the side
            # of the rack. The full-lane ball continues under the solver.
            p.player(ms, 11.70, 5.5, math.degrees(math.atan2(-1.05, 2.25)) % 360,
                     pitch_to(1.6, 0.19, math.hypot(1.05, 2.25)))
        p.capture(600, "bowling-initial")
        p.action(750, "console_enabled", enabled=True); p.tap(850, "`")
        p.player(950, 18.4, 27.8, 0)
        p.command(1150, "/give arcade:party_cup_confetti")
        p.capture(1400, "bowling-unrelated-given")
        p.action(1500, "escape")
        p.player(1650, 10.65, 22.2, 0)
        p.aim_item(1800, BOWLING_BALL)
        p.tap(1950, "E")
        p.capture(2350, "bowling-held-1")
        p.player(2500, 10.65, 20.2, 0, -27)
        p.capture(3200, "bowling-ready-1")
        p.tap_mouse(3400)
        p.capture(3600, "bowling-launch-1")
        p.capture(4300, "bowling-roll-1")
        pin_camera(4500)
        # Preserve the complete consecutive native sequence around the
        # expected full-lane strike. Every numbered PNG is captured in play.
        for index in range(20):
            p.capture(9200 + index * 200, f"bowling-video-{index:03}")
        p.capture(16_000, "bowling-scatter-1")
        # Occupy the authored ball destination while the scattered ball is
        # elsewhere. Aim through the real switch's narrow front and press E.
        p.player(16_200, 10.65, 21.2, math.degrees(math.atan2(switch_x-10.65, 21.2-switch_z)),
                 pitch_to(1.6, switch_y, math.hypot(switch_x-10.65, switch_z-21.2)))
        p.capture(16_450, "bowling-reset-blocked-target")
        p.tap(16_500, "E")
        p.capture(16_750, "bowling-reset-blocked")
        p.player(17_000, 10.25, switch_z, 90, pitch_to(1.6, switch_y, switch_x-10.25))
        p.capture(17_300, "bowling-reset-target-1")
        p.tap(17_500, "E")
        p.capture(18_100, "bowling-reset-1")
        pin_camera(18_300)
        p.capture(18_550, "bowling-reset-pins-1")
        p.player(18_750, 10.65, 22.2, 0)
        p.aim_item(18_950, BOWLING_BALL)
        p.tap(19_100, "E")
        p.capture(19_450, "bowling-held-2")
        p.player(19_650, 10.65, 20.2, 0, -27)
        p.capture(20_350, "bowling-ready-2")
        p.tap_mouse(20_550)
        p.capture(20_750, "bowling-launch-2")
        p.capture(21_350, "bowling-roll-2")
        pin_camera(21_550)
        p.capture(33_500, "bowling-scatter-2")
        p.player(33_800, 10.25, switch_z, 90, pitch_to(1.6, switch_y, switch_x-10.25))
        p.capture(34_100, "bowling-reset-target-2")
        p.tap(34_300, "E")
        p.capture(34_900, "bowling-reset-2")
        pin_camera(35_100)
        p.capture(35_350, "bowling-reset-pins-2")
        p.player(35_550, 10.65, 22.2, 0)
        p.aim_item(35_750, BOWLING_BALL)
        p.tap(35_900, "E")
        p.capture(36_250, "bowling-held-before-reset")
        # E while holding always drops. This explicitly labelled bench action
        # exercises the same authored reset actor binding while a member is
        # held, covering future scripted/button dispatch without changing E.
        p.action(36_450, "activate", id=BOWLING_RESET)
        p.capture(37_050, "bowling-held-reset")
        pin_camera(37_300)
        p.capture(37_550, "bowling-reset-pins-held")
        p.capture(39_350, "bowling-rest")
        return p.finish(39_550)
    if mode == "escape":
        p.capture(500, "initial")
        p.action(600, "console_enabled", enabled=True)
        p.tap(700, "`")
        p.player(800, 6.0, 0.7, 0)
        p.command(950, f"/give {ESCAPE_PIN}")
        p.capture(1150, "escape-given")
        p.action(1200, "escape")
        # Leave unrelated authored bodies exactly dormant so each trajectory
        # measures map contact rather than an accidental body-body obstruction.
        p.action(1350, "activate", id="fixture_reset")
        p.player(1500, 8.0, 0.0, 270)
        for index, case in enumerate(escape_cases()):
            ms = 1800 + index * 1100
            p.item_pose(ms, case["position"], case["rotation"], id=case["id"])
            p.item_velocity(ms + 50, case["id"], case["linear"])
            p.capture(ms + 150, f"escape-{case['name']}-impact")
            p.capture(ms + 950, f"escape-{case['name']}-after")
        p.action(9700, "activate", id="fixture_reset")
        p.player(9900, 0, 0.65, 0)
        p.aim_item(10_050)
        p.tap(10_150, "E")
        p.capture(10_450, "escape-held")
        p.player(10_600, 2.6, -1.8, 90)
        p.key(10_750, "W")
        p.key(11_100, "W", False)
        p.capture(11_400, "escape-held-wall")
        p.tap(11_550, "E")
        p.capture(12_100, "escape-wall-release")
        p.player(12_250, 8.0, 0.0, 270)
        # These two labelled teleports are deliberately invalid diagnostic
        # inputs. They exclusively exercise recovery; collision cases above
        # require zero recovery events and strict pre-recovery geometry bounds.
        p.item_pose(13_000, [6.0, -8.0, 1.0], [0, 0, 0, 1])
        p.capture(13_150, "escape-authored-original")
        p.capture(15_650, "escape-authored-recovered")
        p.item_pose(16_000, [25.0, 1.0, 12.0], [0, 0, 0, 1], id=ESCAPE_PIN)
        p.capture(16_150, "escape-spawned-original")
        p.capture(18_650, "escape-spawned-recovered")
        p.capture(21_650, "escape-rest")
        return p.finish(21_850)
    if mode == "targeting":
        given = "arcade:party_cup_stars"
        p.capture(500, "initial")
        p.action(650, "console_enabled", enabled=True); p.tap(750, "`")
        p.player(850, 1.0, 1.65, 0)
        p.command(1000, f"/give {given}")
        p.action(1300, "escape")
        for index, (name, id) in enumerate((("left", "pickup_cup"), ("right", given))):
            ms = 1400 + index * 1600
            p.player(ms, 0, 1.0, 0)
            p.item_pose(ms + 50, [-0.2, 0.006, 0], [0, 0, 0, 1])
            p.item_pose(ms + 50, [0.2, 0.006, 0], [0, 0, 0, 1], given)
            p.aim_item(ms + 300, id)
            p.capture(ms + 450, f"target-{name}")
            p.tap(ms + 550, "E")
            p.capture(ms + 950, f"target-{name}-held")
            p.player(ms + 1000, 0, 1.5, 90)
            p.tap(ms + 1300, "E")
            p.capture(ms + 1450, f"target-{name}-dropped")
        p.item_pose(4500, [-0.2, 0.006, 0], [0, 0, 0, 1])
        p.item_pose(4500, [0.2, 0.006, 0], [0, 0, 0, 1], given)
        p.player(4550, 0, 1, 0, pitch_to(1.6, 0.067, 1))
        p.capture(4800, "target-off-axis")
        p.tap(4900, "E")
        p.capture(5100, "target-off-axis-unused")
        # The centre is beyond 2.2 m in both trials. The first physical
        # cylinder surface is just inside reach in the near trial, and just
        # outside it in the far trial. AimItem changes the real view only.
        near_z = math.sqrt(2.24 ** 2 - (1.6 - 0.067) ** 2)
        far_z = math.sqrt(2.30 ** 2 - (1.6 - 0.067) ** 2)
        p.item_pose(5300, [0, 0.006, 0], [0, 0, 0, 1])
        p.item_pose(5300, [1.2, 0.006, -1.8], [0, 0, 0, 1], given)
        p.player(5400, 0, near_z, 0)
        p.aim_item(5550)
        p.capture(5750, "target-reach-near")
        p.tap(5850, "E")
        p.capture(6250, "target-reach-near-held")
        p.player(6300, 0, 1.5, 90)
        p.tap(6600, "E")
        p.capture(6750, "target-reach-near-dropped")
        p.item_pose(6900, [0, 0.006, 0], [0, 0, 0, 1])
        p.player(6950, 0, far_z, 0)
        p.aim_item(7100)
        p.capture(7300, "target-reach-far")
        p.tap(7400, "E")
        p.capture(7600, "target-reach-far-unused")
        p.item_pose(7800, [3.45, 0.006, -1.8], [0, 0, 0, 1], given)
        p.player(7850, 2.4, -1.8, 90)
        p.aim_item(8000, given)
        p.capture(8200, "target-wall-occluded")
        p.tap(8300, "E")
        p.capture(8500, "target-wall-unused")
        p.player(8700, -1.9, -1.8, 270)
        p.capture(9000, "target-switch-off-axis")
        p.tap(9100, "E")
        p.capture(9300, "target-switch-off-axis-unused")
        p.player(9500, -1.9, -1.8, 270, pitch_to(1.6, 1.26, 1.05))
        p.capture(9800, "target-switch")
        p.tap(9900, "E")
        p.capture(10_200, "target-switch-used")
        # A labelled airborne setup places a real solver cup on the switch
        # ray. The prompt and E must select it before the fixture behind it.
        foreground_y = 1.6 + (1.26 - 1.6) * (0.5 / 1.05) - 0.061
        p.item_pose(10_300, [-2.4, foreground_y, -1.8], [0, 0, 0, 1])
        p.capture(10_340, "target-foreground")
        # The unsupported cup falls under the real solver. Re-aim immediately
        # before E so PNG latency cannot move it off the narrow selection ray.
        p.aim_item(10_369)
        p.tap(10_370, "E")
        p.capture(10_750, "target-foreground-held")
        p.player(10_800, -1.9, -1.8, 270, pitch_to(1.6, 1.29, 1.05))
        p.tap(10_900, "E")
        p.capture(11_100, "target-foreground-dropped")
        p.item_pose(11_300, [0, 0.006, 0], [0, 0, 0, 1])
        p.item_pose(11_300, [1.2, 0.006, -1.8], [0, 0, 0, 1], given)
        p.player(11_500, -1.335, -1.7, 0)
        p.capture(11_800, "target-door-off-axis")
        p.tap(11_900, "E")
        p.capture(12_100, "target-door-off-axis-unused")
        p.player(12_300, -1.8, -1.7, 0)
        p.capture(12_600, "target-door")
        p.tap(12_700, "E")
        p.capture(13_000, "target-door-used")
        p.capture(15_200, "target-rest")
        return p.finish(15_400)
    if mode == "surfaces":
        p.capture(500, "surface-initial")
        for index, (name, x) in enumerate((("ice", 1.25), ("wood", 5.25), ("tile", 9.25), ("carpet", 13.25))):
            ms = 650 + index * 2500
            p.player(ms, x, 2.0, 180, -15)
            p.item_pose(ms + 50, [x, 0.006, 6], [0, 0, 0, 1], "surface_box")
            p.item_velocity(ms + 300, "surface_box", [0, 0, -2])
            p.capture(ms + 650, f"surface-{name}")
        p.player(10_500, 16.35, 4.0, 90, -20)
        p.item_pose(10_550, [17.25, 0.006, 7.4], [0, 0, 0, 1], "surface_box")
        p.aim_item(10_750, "surface_box")
        p.capture(10_800, "surface-seam-before")
        p.item_velocity(10_850, "surface_box", [0, 0, -6])
        p.aim_item(11_075, "surface_box")
        p.capture(11_100, "surface-seam-ice")
        p.aim_item(11_525, "surface_box")
        p.capture(11_550, "surface-seam-wood")
        p.aim_item(12_125, "surface_box")
        p.capture(12_150, "surface-seam-carpet")
        p.aim_item(14_450, "surface_box")
        p.capture(14_500, "surface-final")
        return p.finish(14_700)
    if mode in ("lighting", "lighting-ball"):
        receiver = "stairs_ball" if mode == "lighting-ball" else "pickup_cup"
        receiver_half_height = 0.108 if mode == "lighting-ball" else 0.061
        if mode == "lighting-ball":
            p.capture(500, "initial")
            p.item_pose(650, [-1.7, 0.006, 0.2], [0, 0, 0, 1])
            p.item_pose(650, [0, 0.75, -0.65], [0, 0, 0, 1], receiver)
            p.player(750, 0, 0.65, 0, pitch_to(1.6, 0.75 + receiver_half_height, 1.3))
            p.aim_item(850, receiver)
            p.tap(950, "E")
            p.capture(1250, "held")
        else:
            p.initial_pickup()
        p.player(1450, 0, 1.2, 0)
        p.capture(2100, "lighting-bright")
        p.player(2300, 1.8, 0.15, 90)
        p.key(2600, "W"); p.key(3600, "W", False)
        p.player(3750, 6.5, 0.15, 90)
        p.capture(4550, "lighting-dim")
        p.tap(4750, "E")
        p.capture(4850, "lighting-dropped")
        p.player(4900, -1.9, -1.8, 270, pitch_to(1.6, 1.29, 1.05))
        p.tap(5150, "E")
        p.item_pose(5450, [0.18, 0.006, 0.35], [0, 0, 0, 1], receiver)
        p.player(5600, 0.18, 1.35, 0, pitch_to(1.6, 0.006 + receiver_half_height, 1))
        p.aim_item(5750, receiver)
        p.tap(5850, "E")
        p.player(6100, 0, 1.2, 0)
        p.capture(6700, "lighting-off")
        p.action(6800, "fixture", id="fixture_light", on=True)
        p.capture(7300, "lighting-on")
        p.action(7450, "low_lighting", enabled=True)
        p.capture(8000, "lighting-static-on")
        p.action(8150, "fixture", id="fixture_light", on=False)
        p.capture(8700, "lighting-static-off")
        p.action(8850, "fixture", id="fixture_light", on=True)
        p.action(9000, "low_lighting", enabled=False)
        p.capture(9600, "lighting-full-returned")
        p.action(9750, "quality", level="medium")
        p.capture(11_750, "lighting-quality-medium")
        p.action(11_900, "quality", level="high")
        p.capture(13_900, "lighting-quality-high")
        p.tap(14_100, "E")
        p.capture(14_350, "lighting-second-drop")
        p.item_pose(14_450, [1.5, 0.006, 0], [0, 0, 0, 1], receiver)
        p.player(14_600, 1.5, 1, 0, pitch_to(1.6, 0.006 + receiver_half_height, 1))
        p.aim_item(14_750, receiver)
        p.tap(14_900, "E")
        p.capture(15_150, "lighting-reheld")
        p.player(15_300, 1.8, 0, 90)
        p.capture(16_150, "lighting-throw-ready")
        p.tap_mouse(16_300)
        p.capture(16_420, "lighting-launch")
        p.capture(16_750, "lighting-flight")
        p.capture(17_500, "lighting-rotation")
        p.capture(21_100, "lighting-rest")
        return p.finish(21_300)
    if mode == "contacts":
        p.capture(500, "initial")
        p.action(650, "console_enabled", enabled=True); p.tap(750, "`")
        given = ("arcade:bowling_ball_cobalt", "arcade:bowling_ball_red",
                 "arcade:party_cup_stars", "arcade:party_cup_balloons")
        for index, (id, x) in enumerate(zip(given, (0, 1.1, 4.8, 6.0))):
            ms = 900 + 220 * index
            p.player(ms, x, 1.65, 0); p.command(ms + 80, f"/give {id}")
        p.action(1900, "escape"); p.player(2000, 8, 2.5, 270, -20)
        p.capture(2200, "contacts-given")
        for index, (name, source, target, gap) in enumerate((
            ("ball-ball", given[0], given[1], 0.55),
            ("ball-cup", given[0], given[2], 0.45),
            ("cup-cup", given[2], given[3], 0.24),
        )):
            ms = 2400 + index * 2200
            for parked, id in enumerate(given):
                p.item_pose(ms, [4.4 + parked * 0.5, 0.006, -2.2], [0, 0, 0, 1], id)
            p.item_pose(ms + 50, [5, 0.006, 0], [0, 0, 0, 1], source)
            p.item_pose(ms + 50, [5 + gap, 0.006, 0], [0, 0, 0, 1], target)
            p.capture(ms + 200, f"contacts-{name}-before")
            p.item_velocity(ms + 300, source, [2, 0, 0])
            p.capture(ms + 1100, f"contacts-{name}-after")
        p.item_pose(9100, [6, 0.006, -1], [0, 0, 0, 1], "floor_box")
        p.item_pose(9100, [6, 0.25, -1], [0, 0, 0, 1], given[2])
        p.item_pose(9100, [6, 0.376, -1], [0, 0, 0, 1], given[3])
        p.player(9250, 6, 1, 0, -28)
        p.capture(11_100, "contacts-stack")
        p.item_pose(11_300, [0, 0.006, 1.5], [0, 0, 0, 1], given[0])
        p.player(11_450, 0, 2.5, 0, pitch_to(1.6, 0.114, 1))
        p.aim_item(11_550, given[0])
        p.tap(11_700, "E")
        # The real carried sphere contacts a dynamic cup resting on the
        # existing table. Native walking closes the initial gap; the target
        # is never moved after the before snapshot.
        p.player(11_950, 0, 0.2, 0, -50)
        p.item_pose(13_950, [0, 0.756, -0.53], [0, 0, 0, 1], given[2])
        p.capture(14_150, "contacts-carry-before")
        p.key(14_250, "W"); p.key(14_650, "W", False)
        p.capture(14_900, "contacts-carry-after")
        p.player(15_100, 0, 1.5, 0)
        p.tap(16_100, "E")
        p.item_pose(16_400, [5, 0.006, 0.5], [0, 0, 0, 1], given[0])
        p.item_pose(16_400, [7.15, 0.006, 1.5], [0, 0, 0, 1], given[2])
        p.player(16_550, 5, 1.5, 0, pitch_to(1.6, 0.114, 1))
        p.aim_item(16_650, given[0])
        p.tap(16_800, "E")
        p.player(17_050, 5, 1.5, 90, -20)
        p.capture(17_700, "contacts-held")
        p.tap_mouse(17_850)
        p.capture(18_950, "contacts-thrown")
        p.capture(23_000, "contacts-rest")
        return p.finish(23_200)
    if mode == "source":
        p.capture(500, "initial")
        # A labelled benchmark fixture reset creates an actual tipped solver
        # body. Its local centre rotates from +Y to +Z; gameplay E then owns
        # the correction to the default upright hold orientation.
        p.item_pose(650, [0.18, 0.043, 0.35], [math.sqrt(0.5), 0, 0, math.sqrt(0.5)])
        p.capture(900, "source-tipped")
        p.player(1050, 0.18, 1.411, 0, pitch_to(1.6, 0.043, 1))
        p.aim_item(1200)
        p.tap(1300, "E")
        p.player(1430, 0.18, 1.411, 0)
        p.capture(1700, "source-upright")
        p.player(1900, 0.18, 1.411, 75, -25)
        p.capture(2350, "source-pitch")
        p.player(2550, 2.0, -1.8, 90)
        p.capture(3450, "source-wall-start")
        p.key(3550, "W")
        p.key(3950, "W", False)
        p.capture(4200, "source-toward-wall")
        p.player(4400, 2.6, -1.8, 0)
        p.capture(5300, "source-along-start")
        p.key(5400, "W")
        p.key(5750, "W", False)
        p.capture(5950, "source-along-wall")
        p.player(6150, 2.6, -1.5, 270)
        p.capture(7050, "source-away-start")
        p.key(7150, "W")
        p.key(7500, "W", False)
        p.capture(7700, "source-away-wall")
        p.player(7900, 2.6, -2.6, 45)
        p.capture(9050, "source-corner")
        p.tap(9250, "E")
        p.capture(9500, "source-dropped")
        p.capture(11_800, "source-rest")
        return p.finish(12_000)
    if mode == "throw":
        p.capture(500, "initial")
        for index, (name, yaw, pitch, x, z) in enumerate((
            ("flat", 0, 0, 0, 1.2), ("up", 0, 25, 0, 1.2),
            ("down", 90, -20, 0, 1.2), ("wall", 90, 0, 2.45, -1.8),
        )):
            ms = 650 + index * 2200
            # Independent, explicit bench setup for each directional trial.
            # Only the subsequent normal E/LMB events pick up and launch it.
            p.item_pose(ms, [0.18, 0.003, 0.35], [0, 0, 0, 1])
            p.player(ms + 150, 0.18, 1.35, 0, pitch_to(1.6, 0.064, 1))
            p.aim_item(ms + 250)
            p.tap(ms + 350, "E")
            p.capture(ms + 600, f"throw-{name}-held")
            p.player(ms + 750, x, z, yaw, pitch)
            p.capture(ms + 1300, f"throw-{name}-ready")
            p.tap_mouse(ms + 1450)
            p.capture(ms + 1530, f"throw-{name}-launched")
            p.capture(ms + 2100, f"throw-{name}-flight")
        p.capture(11_200, "throw-rest")
        return p.finish(11_400)
    if mode == "throw-controls":
        p.initial_pickup()
        p.action(1500, "escape")
        p.capture(1700, "throw-paused-before")
        p.tap_mouse(1850)
        p.capture(2150, "throw-paused-after")
        p.action(2300, "escape")
        p.action(2400, "console_enabled", enabled=True)
        p.tap(2500, "`")
        p.capture(2650, "throw-console-before")
        p.tap_mouse(2800)
        p.capture(3050, "throw-console-after")
        p.action(3200, "escape")
        p.action(3400, "focus", focused=False)
        p.capture(3550, "throw-unfocused-before")
        p.tap_mouse(3650)
        p.capture(3850, "throw-unfocused-after")
        # Same-deadline events exercise the focus-gain click guard in one
        # native pump; the release then arms a later deliberate gameplay click.
        p.action(4000, "focus", focused=True)
        p.tap_mouse(4000)
        p.capture(4250, "throw-focus-click")
        p.tap_mouse(4400, recapture=True)
        p.capture(4650, "throw-recapture-click")
        p.tap_mouse(4850)
        p.capture(5100, "throw-deliberate-click")
        p.capture(7100, "throw-controls-rest")
        return p.finish(7300)
    if mode == "timesteps":
        # No movement or held keys: gravity and native contacts alone exercise
        # a 30 Hz, 144 Hz or catch-up simulation delta selected on the CLI.
        p.capture(500, "initial")
        p.capture(1000, "timestep-1000")
        p.capture(2000, "timestep-2000")
        p.capture(4000, "timestep-4000")
        return p.finish(4200)
    if mode == "video":
        p.initial_pickup()
        p.player(1300, 0.5, 1.2, 30, 10)
        p.capture(1550, "video-carry")
        p.player(1600, 0, 1.2, 0)
        p.tap(1900, "E")
        p.capture(2100, "video-released")
        p.capture(20_000, "video-rest")
        # Every numbered frame is captured by the running native renderer.
        # Keep the full consecutive sequence; encoding belongs to the caller.
        for index in range(20):
            p.capture(800 + index * 100, f"video-{index:03}")
        return p.finish(20_200)
    if mode == "blocked-release":
        p.initial_pickup()
        initial_pitch = pitch_to(1.6, 0.811, 1.3)
        held_z = 0.65 - HOLD_DISTANCE * math.cos(math.radians(initial_pitch))
        held_y = 1.6 + HOLD_DISTANCE * math.sin(math.radians(initial_pitch))
        # A labelled unheld-box setup obstructs the real held cup at the
        # command boundary. The next ordinary SDL E must retain ownership.
        # Both actions share a batch; the controller stays in its valid pose.
        p.item_pose(1450, [0, held_y - 0.12, held_z], [0, 0, 0, 1], "floor_box")
        p.tap(1450, "E")
        p.capture(1650, "release-blocked")
        p.item_pose(1700, [2, 0.006, 0.5], [0, 0, 0, 1], "floor_box")
        p.player(1800, 0, 1.2, 0)
        p.capture(2200, "release-clear")
        p.tap(2350, "E")
        p.capture(2550, "clear-released")
        p.capture(4300, "blocked-rest")
        return p.finish(4500)
    if mode == "ball":
        p.capture(500, "initial")
        p.action(650, "console_enabled", enabled=True)
        p.tap(750, "`")
        p.player(850, 0, 1.65, 0)
        p.command(1000, "/give arcade:bowling_ball_cobalt")
        p.capture(1200, "ball-given")
        p.action(1300, "escape")
        spawn_distance = math.hypot(0.108, 0.108) + 0.3 + 1
        p.player(1450, 0, 1.65, 0, pitch_to(1.6, 0.108, spawn_distance))
        p.aim_item(1600, "arcade:bowling_ball_cobalt")
        p.tap(1700, "E")
        p.capture(2000, "ball-held")
        p.player(2300, -1.3, 0.8, 90)
        p.capture(2500, "ball-carry")
        p.key(2600, "W")
        p.tap(2800, "E")
        p.key(2820, "W", False)
        p.capture(3100, "ball-released")
        p.capture(4300, "ball-rolled")
        p.capture(6500, "ball-rest")
        return p.finish(6700)
    if mode == "pickup":
        p.initial_pickup()
        p.player(1450, 0.8, 1.2, 60, 5)
        p.capture(2100, "view-carry")
        p.player(2300, 0, 1.2, 0)
        p.capture(3000, "before-drop")
        p.tap(3200, "E")
        p.capture(3400, "released")
        p.capture(5200, "settled")
        # Stationary release has no throw. Aim at the hand's known X/Z floor
        # projection after gravity settles it; this is a second real pickup.
        p.player(5450, HOLD_RIGHT, 1.2 - HOLD_DISTANCE + 1, 0,
                 pitch_to(1.6, 0.061, 1))
        p.aim_item(5600)
        p.tap(5700, "E")
        p.capture(6100, "reheld")
        p.tap(6400, "E")
        p.capture(6600, "redropped")
        p.capture(8500, "resettled")
        p.player(8800, 0.35, 0.65, 0, pitch_to(1.6, 0.811, 1.3))
        p.tap(9100, "E")
        p.capture(9300, "fixed-refused")
        return p.finish(9500)
    if mode == "wall":
        p.initial_pickup()
        p.player(1450, 2.60, -1.8, 90)
        p.key(1600, "W")
        p.key(1950, "W", False)
        p.capture(2300, "wall-held")
        p.tap(2500, "E")
        p.capture(2750, "wall-release")
        p.key(3000, "W")
        p.key(3500, "W", False)
        p.capture(3800, "wall-contact")
        p.capture(5500, "wall-rest")
        return p.finish(5700)
    if mode == "corner":
        p.initial_pickup()
        p.player(1450, 2.6, -2.6, 45)
        p.capture(2550, "corner-held")
        p.tap(2750, "E")
        p.capture(3000, "corner-release")
        p.capture(5000, "corner-rest")
        return p.finish(5200)
    if mode == "transitions":
        p.initial_pickup()
        p.action(1500, "escape")
        p.capture(1700, "paused-before")
        p.capture(2300, "paused-after")
        p.action(2500, "escape")
        p.action(2650, "console_enabled", enabled=True)
        p.tap(2750, "`")
        p.capture(2900, "console-before")
        p.tap(3000, "E")
        p.action(3100, "text", text="e", submit=False)
        p.tap(3200, "Backspace")
        p.capture(3400, "console-after")
        p.command(3550, "/noclip on")
        p.capture(3700, "noclip-refused")
        p.action(3800, "escape")
        p.tap(3950, "E")
        p.capture(4150, "transition-drop")
        p.tap(4300, "`")
        p.command(4400, "/noclip on")
        p.action(4550, "escape")
        p.capture(4750, "noclip")
        p.tap(4900, "`")
        p.command(5000, "/noclip off")
        p.action(5150, "escape")
        p.action(5350, "quality", level="medium")
        p.capture(7100, "quality-medium")
        p.action(7300, "quality", level="high")
        p.capture(8600, "quality-high")
        p.action(8900, "load", level=LEVEL)
        p.capture(11_700, "reloaded")
        return p.finish(12_000)
    if mode == "stress":
        # Zero given props and 16 given props, over the same four authored
        # bodies. This keeps realistic load bounded and exercises the actual
        # give cap instead of injecting synthetic physics bodies.
        p.capture(500, "stress-zero")
        p.action(650, "console_enabled", enabled=True)
        p.tap(750, "`")
        p.command(900, "/give core:not_an_item")
        p.capture(1100, "invalid-refused")
        i = 0
        for x in (-2.3, -1.3, 1.3, 2.3):
            for target_z in (-1.7, -1.1, -0.5, 0.1):
                ms = 1250 + i * 180
                # The production give helper uses radius + player radius +
                # one metre. These poses keep every footprint on flat floor.
                p.player(ms, x, target_z + 1.36, 0)
                p.command(ms + 70, f"/give {CUP_MODEL}")
                i += 1
        p.capture(4400, "stress-16-console")
        p.command(4550, f"/give {CUP_MODEL}")
        p.capture(4750, "cap-refused")
        p.action(4900, "escape")
        p.capture(5500, "stress-16")
        p.capture(7500, "stress-settled")
        p.action(7800, "console_enabled", enabled=False)
        p.capture(8100, "given-cleared")
        return p.finish(8300)
    raise ValueError(mode)


def vector(value: object, length: int) -> list[float]:
    if isinstance(value, dict):
        value = [value[key] for key in ("x", "y", "z", "w")[:length]]
    if not isinstance(value, list) or len(value) != length:
        raise ValueError(f"expected finite {length}-component vector, got {value!r}")
    result = [float(component) for component in value]
    if not all(math.isfinite(component) for component in result):
        raise ValueError("nonfinite physics vector")
    return result


def body_position(body: dict) -> list[float]:
    return vector(body["pose"]["position"], 3)


def physics_step_seconds(stats: dict) -> float:
    """Use native configuration; only pre-metadata receipts use 120 Hz."""
    seconds = float(stats.get("fixed_step_seconds", 1 / 120))
    if not math.isfinite(seconds) or not 0 < seconds <= 0.1:
        raise ValueError("invalid recorded physics fixed_step_seconds")
    return seconds


def physics_step_cap(stats: dict) -> int:
    """Use the native catch-up cap; historical receipts used twelve steps."""
    cap = float(stats.get("max_substeps", 12))
    if not math.isfinite(cap) or not cap.is_integer() or not 1 <= cap <= 256:
        raise ValueError("invalid recorded physics max_substeps")
    return int(cap)


def physics_frame_seconds(stats: dict) -> float:
    steps = int(stats["substeps"])
    if not 0 <= steps <= physics_step_cap(stats):
        raise ValueError("recorded solver work exceeds its native catch-up cap")
    return steps * physics_step_seconds(stats)


def physics_configuration(samples: dict) -> dict:
    configurations = set()
    metadata_sources = set()
    for sample in samples.values():
        stats = sample["physics"]["stats"]
        fields = ("fixed_step_seconds" in stats, "max_substeps" in stats)
        if fields[0] != fields[1]:
            raise ValueError("recorded native step configuration is incomplete")
        seconds, cap = physics_step_seconds(stats), physics_step_cap(stats)
        if seconds * cap > 0.100001:
            raise ValueError("recorded catch-up configuration exceeds the 0.1 second acceptance budget")
        configurations.add((seconds, cap))
        metadata_sources.add("native_stats" if fields[0] else "historical_120_hz_12_step_receipt")
    return dict(configurations=[dict(fixed_step_seconds=seconds, max_substeps=cap, maximum_catch_up_seconds=seconds * cap)
                                for seconds, cap in sorted(configurations)], sources=sorted(metadata_sources))


def distance(a: list[float], b: list[float]) -> float:
    return math.sqrt(sum((x - y) ** 2 for x, y in zip(a, b)))


def cup_bounds(body: dict) -> tuple[list[float], list[float]]:
    # ItemPose is the original model origin. The native collider is centred
    # 0.061 m above that origin, so a tipped cup cannot be checked using its
    # origin height alone. Project the cylinder's real half-height and radius.
    x, y, z, w = vector(body["pose"]["rotation"], 4)
    up = [2 * (x * y - z * w), 1 - 2 * (x * x + z * z), 2 * (y * z + x * w)]
    centre = [origin + 0.061 * axis for origin, axis in zip(body_position(body), up)]
    half = [0.061 * abs(axis) + 0.0425 * math.sqrt(max(0, 1 - axis * axis)) for axis in up]
    return ([position - extent for position, extent in zip(centre, half)],
            [position + extent for position, extent in zip(centre, half)])


def rotation_axes(rotation: object) -> list[list[float]]:
    x, y, z, w = vector(rotation, 4)
    return [[1 - 2 * (y * y + z * z), 2 * (x * y + z * w), 2 * (x * z - y * w)],
            [2 * (x * y - z * w), 1 - 2 * (x * x + z * z), 2 * (y * z + x * w)],
            [2 * (x * z + y * w), 2 * (y * z - x * w), 1 - 2 * (x * x + y * y)]]


def shape_support_offset(shape: dict, direction: list[float]) -> float:
    """Exact minimum dot product of an authored shape in its local frame."""
    kind = shape["kind"]
    if kind == "sphere":
        return -shape["radius"] * distance(direction, [0, 0, 0])
    if kind == "cylinder":
        return -shape["half_height"] * abs(direction[1]) - shape["radius"] * math.hypot(direction[0], direction[2])
    if kind == "cuboid":
        return -sum(abs(component) * half for component, half in zip(direction, shape["half_extents"]))
    if kind == "convex_hull":
        if not 4 <= len(shape["points"]) <= 96:
            raise ValueError("native audit requires a bounded authored convex hull")
        return min(point[0] * direction[0] + point[1] * direction[1] + point[2] * direction[2] for point in shape["points"])
    if kind == "compound":
        if not 1 <= len(shape["parts"]) <= 8:
            raise ValueError("native audit requires a bounded authored compound")
        return min(sum(a * b for a, b in zip(vector(part["offset"], 3), direction))
                   + shape_support_offset(part["shape"], direction) for part in shape["parts"])
    raise ValueError(f"unsupported native audit collider {kind}")


def body_support(body: dict, prop: dict, direction: list[float]) -> float:
    axes = rotation_axes(body["pose"]["rotation"])
    scale, shape = prop.get("scale", 1), prop["physics"]["shape"]
    local_direction = [scale * sum(a * b for a, b in zip(axis, direction)) for axis in axes]
    # Standalone primitives use model-bounds centres. Compound part offsets
    # are explicitly model-origin coordinates, matching the runtime contract.
    origin = body_position(body) if shape["kind"] == "compound" else fixture_body_center(body, prop)
    return sum(a * b for a, b in zip(origin, direction)) + shape_support_offset(shape, local_direction)


def fixture_floor_gap(body: dict, prop: dict) -> float:
    """Exact lowest physical point, including bounded authored hull vertices."""
    return body_support(body, prop, [0, 1, 0])


def fixture_body_center(body: dict, prop: dict) -> list[float]:
    x, y, z, w = vector(body["pose"]["rotation"], 4)
    up = [2 * (x * y - z * w), 1 - 2 * (x * x + z * z), 2 * (y * z + x * w)]
    offset = prop["size"][1] * prop.get("scale", 1) * 0.5
    return [position + offset * axis for position, axis in zip(body_position(body), up)]


def fixture_body_bounds(body: dict, prop: dict) -> tuple[list[float], list[float]]:
    """Exact primitive bounds, independent of the renderer's mesh AABB."""
    directions = ([1, 0, 0], [0, 1, 0], [0, 0, 1])
    return ([body_support(body, prop, direction) for direction in directions],
            [-body_support(body, prop, [-value for value in direction]) for direction in directions])


@lru_cache(maxsize=8)
def model_mesh(model: str) -> tuple[tuple[tuple[float, ...], ...], tuple[tuple[tuple[float, ...], ...], ...], Path]:
    """Read the actual bounded static GLB geometry used by these native cases.

    The audited stock models have one identity root node. Refuse a changed
    transform/skin contract instead of silently treating its vertices as world
    geometry. This offline audit never loads or generates assets during play.
    """
    catalog = json.loads((ROOT / "assets/catalog.json").read_text())
    entry = next(entry for entry in catalog["assets"] if entry["id"] == model)
    path = (ROOT / "assets" / entry["model"]).resolve(strict=True)
    if not path.is_relative_to(ROOT / "assets"):
        raise ValueError("audit model must be inside repository assets")
    data = path.read_bytes()
    if len(data) > 16 * 1024 * 1024 or len(data) < 20:
        raise ValueError("audit GLB exceeds the bounded static fixture size")
    magic, version, length = struct.unpack_from("<III", data)
    if magic != 0x46546C67 or version != 2 or length != len(data):
        raise ValueError("invalid audit GLB header")
    chunks, offset = {}, 12
    while offset < len(data):
        size, kind = struct.unpack_from("<II", data, offset)
        offset += 8
        if offset + size > len(data) or kind in chunks:
            raise ValueError("invalid audit GLB chunk bounds")
        chunks[kind] = data[offset:offset + size]
        offset += size
    gltf = json.loads(chunks[0x4E4F534A])
    binary = chunks[0x004E4942]
    nodes = gltf.get("nodes", [])
    if (len(nodes) != 1 or nodes[0].get("mesh") != 0
            or any(field in nodes[0] for field in ("matrix", "translation", "rotation", "scale", "skin", "children"))
            or gltf.get("animations") or len(gltf["meshes"]) != 1):
        raise ValueError("audit GLB static identity-node contract changed")

    def accessor(index: int, expected_type: str) -> list[tuple]:
        item = gltf["accessors"][index]
        if item.get("type") != expected_type or "sparse" in item or not 1 <= item["count"] <= 200_000:
            raise ValueError("unsupported or unbounded audit GLB accessor")
        view = gltf["bufferViews"][item["bufferView"]]
        if view.get("buffer", 0) != 0:
            raise ValueError("audit GLB accessor requires its own binary buffer")
        format_code = {5121: "B", 5123: "H", 5125: "I", 5126: "f"}[item["componentType"]]
        if expected_type == "SCALAR" and format_code == "f":
            raise ValueError("audit GLB requires integer triangle indices")
        components = 3 if expected_type == "VEC3" else 1
        fmt = "<" + format_code * components
        size = struct.calcsize(fmt)
        start = view.get("byteOffset", 0) + item.get("byteOffset", 0)
        stride = view.get("byteStride", size)
        if (stride < size or start < view.get("byteOffset", 0)
                or start + stride * (item["count"] - 1) + size > view.get("byteOffset", 0) + view["byteLength"]
                or start + stride * (item["count"] - 1) + size > len(binary)):
            raise ValueError("audit GLB accessor exceeds its declared view")
        values = [struct.unpack_from(fmt, binary, start + stride * i) for i in range(item["count"])]
        if expected_type == "VEC3" and (format_code != "f" or not all(math.isfinite(value) for row in values for value in row)):
            raise ValueError("audit GLB requires finite float vertex positions")
        return values

    triangles = []
    for primitive in gltf["meshes"][0]["primitives"]:
        if primitive.get("mode", 4) != 4 or primitive.get("targets"):
            raise ValueError("audit GLB requires static triangle geometry")
        positions = accessor(primitive["attributes"]["POSITION"], "VEC3")
        indices = [record[0] for record in accessor(primitive["indices"], "SCALAR")]
        if len(indices) % 3 or any(index >= len(positions) for index in indices):
            raise ValueError("audit GLB has invalid triangle indices")
        triangles.extend(tuple(positions[index] for index in indices[start:start + 3]) for start in range(0, len(indices), 3))
    vertices = tuple(sorted({point for triangle in triangles for point in triangle}))
    return vertices, tuple(triangles), path


@lru_cache(maxsize=1)
def bowling_support_surfaces() -> tuple[tuple, tuple]:
    """Actual upper cross-section edges from the unchanged gutter GLB."""
    _, triangles, _ = model_mesh("arcade:gutter_segment")
    segments = set()
    for triangle in triangles:
        if max(point[2] for point in triangle) - min(point[2] for point in triangle) < 2.9:
            continue
        points = sorted({(point[0], point[1]) for point in triangle})
        if len(points) == 2:
            segments.add(tuple(points))
    if len(segments) < 20:
        raise ValueError("audit cannot resolve the gutter's actual closed cross-section")
    level = json.loads(BOWLING_FIXTURE.read_text())
    regions = tuple(level["floor_regions"])
    gutters = []
    for prop in level["props"]:
        if prop["model"] != "arcade:gutter_segment":
            continue
        region = next(region for region in regions if region["x"] <= prop["x"] <= region["x"] + region["width"]
                      and region["z"] <= prop["z"] <= region["z"] + region["depth"])
        if prop.get("rotation_degrees", 0) != 0 or prop.get("scale", 1) != 1:
            raise ValueError("gutter audit requires the existing unscaled lane stock")
        gutters.append((prop["x"], prop["z"], region["offset_y"] + prop.get("y", 0), tuple(segments)))
    return regions, tuple(gutters)


def bowling_floor_height(x: float, z: float, surfaces: tuple[tuple, tuple]) -> float:
    """Visible support: ordinary wood/deck, recessed floor, or curved stock."""
    regions, gutters = surfaces
    height = 0.0
    for region in regions:
        if region["x"] <= x <= region["x"] + region["width"] and region["z"] <= z <= region["z"] + region["depth"]:
            height = region["offset_y"]
    if height >= 0:
        return height
    for centre_x, centre_z, origin_y, segments in gutters:
        if abs(x - centre_x) > 0.1350001 or abs(z - centre_z) > 1.5000001:
            continue
        at = x - centre_x
        for first, last in segments:
            if first[0] - 1e-7 <= at <= last[0] + 1e-7:
                width = last[0] - first[0]
                y = max(first[1], last[1]) if width < 1e-9 else first[1] + (last[1] - first[1]) * (at - first[0]) / width
                height = max(height, origin_y + y)
    return height


def mesh_floor_gap(body: dict, prop: dict, vertices: tuple, surfaces: tuple, rendered: bool) -> tuple[float, list[float]]:
    pose = body["render_pose"] if rendered else body["pose"]
    axes, origin = rotation_axes(pose["rotation"]), vector(pose["position"], 3)
    scale = prop.get("scale", 1)
    minimum, witness = math.inf, []
    for local in vertices:
        point = [origin[dimension] + scale * sum(axis[dimension] * value for axis, value in zip(axes, local)) for dimension in range(3)]
        gap = point[1] - bowling_floor_height(point[0], point[2], surfaces)
        if gap < minimum:
            minimum, witness = gap, point
    return minimum, witness


def cup_player_clearance(sample: dict, body: dict) -> float:
    """Standing native capsule-to-cylinder separation along their centre line."""
    x, y, z, w = vector(body["pose"]["rotation"], 4)
    up = [2 * (x * y - z * w), 1 - 2 * (x * x + z * z), 2 * (y * z + x * w)]
    centre = [origin + 0.061 * axis for origin, axis in zip(body_position(body), up)]
    eye = vector(sample["player_position"], 3)
    feet_y = eye[1] - 1.6
    capsule_axis = [eye[0], min(feet_y + 1.5, max(feet_y + 0.3, centre[1])), eye[2]]
    delta = [position - axis for position, axis in zip(centre, capsule_axis)]
    separation = distance(delta, [0, 0, 0])
    if separation <= 1e-9:
        return -0.3 - 0.061
    direction = [component / separation for component in delta]
    alignment = sum(axis * normal for axis, normal in zip(up, direction))
    support = 0.061 * abs(alignment) + 0.0425 * math.sqrt(max(0, 1 - alignment * alignment))
    return separation - 0.3 - support


def cup_center(body: dict) -> list[float]:
    low, high = cup_bounds(body)
    return [(minimum + maximum) * 0.5 for minimum, maximum in zip(low, high)]


def cup_up_y(body: dict) -> float:
    x, _, z, _ = vector(body["pose"]["rotation"], 4)
    return 1 - 2 * (x * x + z * z)


def cup_ray_distance(sample: dict, body: dict) -> float | None:
    """Independent closed-form ray intersection with the actual cup cylinder."""
    x, y, z, w = vector(body["pose"]["rotation"], 4)
    up = [2 * (x * y - z * w), 1 - 2 * (x * x + z * z), 2 * (y * z + x * w)]
    view = vector(sample["view_direction"], 3)
    length = distance(view, [0, 0, 0])
    if length <= 0.0001:
        return None
    view = [component / length for component in view]
    origin = [eye - centre for eye, centre in zip(vector(sample["player_position"], 3), cup_center(body))]
    origin_y = sum(component * axis for component, axis in zip(origin, up))
    view_y = sum(component * axis for component, axis in zip(view, up))
    origin_radial = [component - origin_y * axis for component, axis in zip(origin, up)]
    view_radial = [component - view_y * axis for component, axis in zip(view, up)]
    a = sum(component * component for component in view_radial)
    b = 2 * sum(left * right for left, right in zip(origin_radial, view_radial))
    c = sum(component * component for component in origin_radial) - 0.0425 ** 2
    candidates = []
    discriminant = b * b - 4 * a * c
    if a > 1e-12 and discriminant >= 0:
        for sign in (-1, 1):
            t = (-b + sign * math.sqrt(discriminant)) / (2 * a)
            if t >= 0 and abs(origin_y + t * view_y) <= 0.061 + 1e-7:
                candidates.append(t)
    if abs(view_y) > 1e-12:
        for cap in (-0.061, 0.061):
            t = (cap - origin_y) / view_y
            radial = [left + t * right for left, right in zip(origin_radial, view_radial)]
            if t >= 0 and sum(component * component for component in radial) <= 0.0425 ** 2 + 1e-7:
                candidates.append(t)
    return min(candidates) if candidates else None


def check_centered_hold(name: str, sample: dict, body: dict, checks: list[dict]) -> None:
    """Compare the actual body centre against the actual post-frame view ray."""
    value = sample.get("view_direction")
    if value is None:
        checks.append(dict(name=f"{name}: actual camera view ray is traced", passed=False))
        return
    view = vector(value, 3)
    length = distance(view, [0, 0, 0])
    if length < 0.0001:
        checks.append(dict(name=f"{name}: actual camera view ray is normalized", passed=False))
        return
    delta = [centre - eye for centre, eye in zip(cup_center(body), vector(sample["player_position"], 3))]
    direction = [component / length for component in view]
    forward = sum(component * axis for component, axis in zip(delta, direction))
    perpendicular = distance([component - forward * axis for component, axis in zip(delta, direction)], [0, 0, 0])
    checks.append(dict(name=f"{name}: clear-space held body is centered on the actual view ray",
                       passed=abs(length - 1) <= 0.001 and perpendicular <= 0.02
                       and abs(forward - HOLD_DISTANCE) <= 0.03,
                       perpendicular_m=perpendicular, forward_m=forward))


def snapshot_samples(events: list[dict], steps: list[dict]) -> tuple[dict, set[int]]:
    samples, seen = {}, set()
    current_id = None
    for event in events:
        if event["event"] == "world_committed" and "world-start" not in samples:
            samples["world-start"] = json.loads(event["detail"])
        elif event["event"].startswith("native_capture:"):
            samples[event["event"].split(":", 1)[1]] = json.loads(event["detail"])
        elif event["event"] == "action":
            detail = json.loads(event["detail"])
            current_id = int(detail["id"])
            seen.add(current_id)
        elif event["event"] == "action_result" and current_id is not None:
            action = steps[current_id]["action"]
            if action["kind"] == "capture" and action["name"] not in samples:
                samples[action["name"]] = json.loads(event["detail"])
    return samples, seen


def lighting_snapshots(sample: dict) -> dict:
    """Read both historical tuple traces and current keyed native snapshots."""
    found = {}
    for entry in sample["physics"].get("lighting", []):
        if isinstance(entry, dict) and "key" in entry:
            key, lighting = entry["key"], entry
        elif isinstance(entry, list) and len(entry) == 2 and isinstance(entry[1], dict):
            key, lighting = entry
        else:
            raise ValueError("malformed native item lighting snapshot")
        if key in found:
            raise ValueError(f"duplicate native item lighting snapshot for {key}")
        found[key] = lighting
    return found


def light_states(sample: dict) -> dict:
    """Native Game light state records serialize as [fixture index, on]."""
    states = sample["light_states"]
    if all(isinstance(state, bool) for state in states):
        return dict(enumerate(states))
    result = {}
    for record in states:
        if not isinstance(record, list) or len(record) != 2 or not isinstance(record[1], bool) or record[0] in result:
            raise ValueError("malformed native fixture light states")
        result[record[0]] = record[1]
    return result


def check_samples(mode: str, samples: dict, checks: list[dict], sim_delta: float = 1 / 60) -> None:
    def require(name: str, condition: bool) -> None:
        checks.append(dict(name=name, passed=bool(condition)))

    expected = {step["action"]["name"] for step in plan_for(mode)
                if step["action"]["kind"] == "capture"}
    require("every capture has a native physics snapshot", expected <= samples.keys())
    if not expected <= samples.keys():
        return
    for name, sample in samples.items():
        target = sample.get("interaction_target") or {}
        held_key = sample.get("physics", {}).get("held")
        if held_key is not None:
            require(f"{name}: holding suppresses the interaction hand", sample.get("interaction_hand_visible") is False)
        elif name in ("dormant-aim", "target-left", "target-right", "target-switch", "target-door", "bowling-reset-target-1", "bowling-reset-target-2"):
            require(f"{name}: the hand uses the selected native target", target.get("kind") in ("item", "world")
                    and sample.get("interaction_hand_visible") is True)
        elif name in ("target-off-axis", "target-switch-off-axis", "target-door-off-axis"):
            require(f"{name}: an off-axis view has no hand", sample.get("interaction_hand_visible") is False)
    if mode == "bowling":
        check_bowling_samples(samples, checks)
        return
    if mode == "escape":
        check_escape_samples(samples, checks)
        return
    if mode == "dormancy":
        check_dormancy_samples(samples, checks)
        return
    if mode == "surfaces":
        for name, sample in samples.items():
            items = sample.get("physics", {}).get("bodies", [])
            require(f"{name}: one unchanged surface-test body", len(items) == 1)
            for item in items:
                position = body_position(item)
                vector(item["linear_velocity"], 3); vector(item["angular_velocity"], 3)
                require(f"{name}: body remains in surface fixture", -0.2 <= position[0] <= 18.7
                        and -0.03 <= position[1] <= 3.4 and -0.2 <= position[2] <= 8.2)
        return
    for name, sample in samples.items():
        physics = sample.get("physics", {})
        bodies = physics.get("bodies", [])
        stats = physics.get("stats", {})
        require(f"{name}: real body snapshots", bool(bodies) and bool(stats))
        require(f"{name}: bounded body count", 0 < len(bodies) <= 128
                and (name == "world-start" or stats.get("body_count") == len(bodies)))
        require(f"{name}: bounded fixed-step work", 0 <= stats.get("substeps", -1) <= physics_step_cap(stats)
                and math.isfinite(float(stats.get("clamped_seconds", math.nan)))
                and stats.get("clamped_seconds", -1) >= 0)
        for body in bodies:
            position = body_position(body)
            rotation = vector(body["pose"]["rotation"], 4)
            vector(body["linear_velocity"], 3)
            vector(body["angular_velocity"], 3)
            max_x = 9.2 if mode in ("lighting", "lighting-ball", "contacts", "targeting") else 3.2
            require(f"{name}: body {body['key']} inside fixture", -3.2 <= position[0] <= max_x
                    and -0.03 <= position[1] <= 3.4 and abs(position[2]) <= 3.2)
            require(f"{name}: body {body['key']} normalized rotation",
                    abs(sum(component * component for component in rotation) - 1) < 0.02)
    initial = samples["stress-zero" if mode == "stress" else "initial"]["physics"]
    require("four authored movable bodies", len(initial["bodies"]) == BODY_COUNT)
    if len(initial["bodies"]) != BODY_COUNT:
        return

    # Named ids are preferred; nearest known authored centre also works with
    # the minimal ItemWorld snapshot API, which exposes stable keys only.
    def identify(instance: str, centre: list[float]) -> dict:
        candidates = [body for body in initial["bodies"] if body.get("id") == instance]
        keys = [entry[0] for entry in initial.get("authored", [])
                if isinstance(entry, list) and len(entry) >= 2 and entry[1] == instance]
        candidates.extend(body for body in initial["bodies"] if body["key"] in keys)
        return candidates[0] if candidates else min(
            initial["bodies"], key=lambda body: distance(body_position(body), centre))

    cup = identify("pickup_cup", [0, 0.75, -0.65])
    cup_key = cup["key"]
    require("cup starts on the table", distance(body_position(cup), [0, 0.75, -0.65]) < 0.03)
    authored_ids = [entry[1] for entry in initial.get("authored", [])
                    if isinstance(entry, list) and len(entry) >= 2]
    require("fixed scenery cup is outside the movable lifecycle", "fixed_cup" not in authored_ids)

    if "world-start" in samples:
        spawned = {item["key"]: item for item in samples["world-start"]["physics"]["bodies"]}
        rested = samples[{"pickup": "resettled", "wall": "wall-rest",
                          "transitions": "quality-high", "stress": "stress-settled",
                          "timesteps": "timestep-4000", "video": "video-rest",
                          "blocked-release": "blocked-rest", "corner": "corner-rest",
                          "ball": "ball-rest", "source": "source-rest",
                          "throw": "throw-rest", "throw-controls": "throw-controls-rest",
                          "lighting": "lighting-rest", "lighting-ball": "lighting-rest", "contacts": "contacts-rest", "targeting": "target-rest"}[mode]]["physics"]["bodies"]
        rested = {item["key"]: item for item in rested}
        for instance in ("stairs_ball", "ramp_bottle", "floor_box"):
            keys = [entry[0] for entry in initial.get("authored", [])
                    if isinstance(entry, list) and len(entry) >= 2 and entry[1] == instance]
            require(f"{instance}: native gravity reaches authored contact surfaces", bool(keys)
                    and keys[0] in spawned and keys[0] in rested
                    and body_position(spawned[keys[0]])[1] - body_position(rested[keys[0]])[1] > 0.3)

    def held(name: str) -> object:
        return samples[name]["physics"]["held"]

    def body(name: str, key: int = cup_key) -> dict:
        return next(item for item in samples[name]["physics"]["bodies"] if item["key"] == key)

    def frozen(before: str, after: str) -> bool:
        left = {item["key"]: item for item in samples[before]["physics"]["bodies"]}
        right = {item["key"]: item for item in samples[after]["physics"]["bodies"]}
        return left.keys() == right.keys() and all(
            distance(body_position(left[key]), body_position(right[key])) < 0.0001
            and left[key]["pose"]["rotation"] == right[key]["pose"]["rotation"]
            and left[key]["linear_velocity"] == right[key]["linear_velocity"]
            for key in left)

    if mode in ("pickup", "wall", "corner", "transitions", "video", "blocked-release", "throw-controls"):
        require("native Interact picks up the table cup", held("held") == cup_key)
    if mode == "targeting":
        given_key = 2 ** 64 - 1
        for name, sample in samples.items():
            require(f"{name}: native common interaction target is traced", "interaction_target" in sample)
        def is_target(name: str, kind: str, value: object) -> bool:
            selected = samples[name].get("interaction_target")
            return isinstance(selected, dict) and selected.get("kind") == kind and selected.get("id" if kind == "world" else "key") == value
        require("normal give creates one additional targeting cup", len(samples["target-left"]["physics"]["bodies"]) == BODY_COUNT + 1)
        for name, key in (("left", cup_key), ("right", given_key)):
            require(f"{name}: direct view ray selects only the corresponding physical cup", is_target(f"target-{name}", "item", key))
            require(f"{name}: normal E picks up exactly the hinted cup", held(f"target-{name}-held") == key)
            require(f"{name}: held target changes to drop", is_target(f"target-{name}-held", "drop", key))
            require(f"{name}: later E drops the same cup", held(f"target-{name}-dropped") is None)
        for name in ("target-off-axis", "target-off-axis-unused", "target-reach-far", "target-reach-far-unused",
                     "target-wall-occluded", "target-wall-unused", "target-switch-off-axis", "target-switch-off-axis-unused",
                     "target-door-off-axis", "target-door-off-axis-unused"):
            require(f"{name}: no off-ray, distant or occluded interaction hint", samples[name].get("interaction_target") is None)
            require(f"{name}: invalid E never picks up an item", held(name) is None)
        require("off-axis switch E leaves the light state unchanged", samples["target-switch-off-axis"]["light_states"] == samples["target-switch-off-axis-unused"]["light_states"])
        near = samples["target-reach-near"].get("interaction_target") or {}
        actual_near = cup_ray_distance(samples["target-reach-near"], body("target-reach-near"))
        actual_far = cup_ray_distance(samples["target-reach-far"], body("target-reach-far"))
        require("near physical surface is inside 2.2 m although body centre is farther", is_target("target-reach-near", "item", cup_key)
                and actual_near is not None and 2.14 <= actual_near <= 2.2
                and abs(float(near.get("distance", math.inf)) - actual_near) <= 0.002
                and distance(vector(samples["target-reach-near"]["player_position"], 3), cup_center(body("target-reach-near"))) > 2.2)
        checks.append(dict(name="far physical surface falls just outside 2.2 m", passed=actual_far is not None and 2.2 < actual_far < 2.3,
                           near_surface_m=actual_near, far_surface_m=actual_far))
        require("normal E accepts the near surface", held("target-reach-near-held") == cup_key)
        require("the thin switch exposes its own world target", is_target("target-switch", "world", "fixture_switch"))
        before_lights, after_lights = light_states(samples["target-switch"]), light_states(samples["target-switch-used"])
        require("normal E toggles exactly the hinted fixture", before_lights.get(0) is True and after_lights.get(0) is False
                and {key: value for key, value in before_lights.items() if key != 0} == {key: value for key, value in after_lights.items() if key != 0})
        require("foreground physical cup wins over the switch behind it", is_target("target-foreground", "item", cup_key))
        require("normal E picks up the foreground cup", held("target-foreground-held") == cup_key
                and is_target("target-foreground-held", "drop", cup_key))
        require("E while holding drops even when the view points at the switch", held("target-foreground-dropped") is None
                and samples["target-foreground-held"]["light_states"] == samples["target-switch-used"]["light_states"]
                and samples["target-foreground-dropped"]["light_states"] == samples["target-switch-used"]["light_states"])
        require("the thin door exposes its own world target", is_target("target-door", "world", "fixture_door"))
        def door_angle(name: str) -> float:
            return float(next(door["angle_degrees"] for door in samples[name]["doors"] if door["id"] == "fixture_door"))
        require("off-axis door E leaves its closed state unchanged", abs(door_angle("target-door-off-axis")) < 0.01
                and abs(door_angle("target-door-off-axis-unused")) < 0.01)
        require("normal E opens exactly the hinted door", abs(door_angle("target-door")) < 0.01 and abs(door_angle("target-door-used")) > 10)
    elif mode in ("lighting", "lighting-ball"):
        receiver = "stairs_ball" if mode == "lighting-ball" else "pickup_cup"
        receiver_key = next(entry[0] for entry in initial["authored"] if entry[1] == receiver)
        prop = next(prop for prop in json.loads(FIXTURE.read_text())["props"] if prop.get("id") == receiver)
        require(f"physical {receiver} is held while crossing to dim room", held("held") == receiver_key
                and held("lighting-bright") == receiver_key and held("lighting-dim") == receiver_key
                and fixture_body_center(body("lighting-dim", receiver_key), prop)[0] > 6.5)
        require("native E releases the receiver in the dim room", held("lighting-dropped") is None)
        require("real E wall switch turns main circuit off", light_states(samples["lighting-off"]).get(0) is False)
        for name in ("lighting-off", "lighting-on", "lighting-static-on", "lighting-static-off", "lighting-full-returned",
                     "lighting-quality-medium", "lighting-quality-high", "lighting-reheld", "lighting-throw-ready"):
            require(f"{name}: same held physical receiver receives lighting", held(name) == receiver_key)
        for name, quality in (("lighting-quality-medium", "medium"), ("lighting-quality-high", "high")):
            require(f"{name}: texture quality changes independently of Full lighting", samples[name]["quality"] == quality
                    and samples[name]["lighting"] == "high" and samples[name]["renderer_lighting"] == "high"
                    and samples[name]["lightmaps"] == "full" and samples[name]["renderer_lightmaps"] == "full")
        for name in ("lighting-static-on", "lighting-static-off"):
            require(f"{name}: native lighting control selects Static", samples[name]["lighting"] == "low"
                    and samples[name]["renderer_lighting"] == "low" and samples[name]["lightmaps"] == "off"
                    and samples[name]["renderer_lightmaps"] == "off")
        require("native LMB throws the receiver through the opening", held("lighting-launch") is None
                and held("lighting-flight") is None and fixture_body_center(body("lighting-flight", receiver_key), prop)[0] > 3.2)
        require("a dropped receiver moves under native gravity", body_position(body("lighting-dropped", receiver_key))[1]
                < body_position(body("lighting-dim", receiver_key))[1] - 0.01)
        ready_rotation = vector(body("lighting-throw-ready", receiver_key)["pose"]["rotation"], 4)
        rotations = [vector(body(name, receiver_key)["pose"]["rotation"], 4)
                     for name in ("lighting-flight", "lighting-rotation", "lighting-rest")]
        changes = [math.degrees(2 * math.acos(min(1, abs(sum(a * b for a, b in zip(ready_rotation, rotation)))))) for rotation in rotations]
        checks.append(dict(name="native throw and contacts rotate the lit receiver", passed=max(changes) > 2,
                           orientation_change_degrees=max(changes)))
        for name in ("lighting-launch", "lighting-flight", "lighting-rotation", "lighting-rest"):
            require(f"{name}: actual receiver shape stays above the floor", fixture_body_bounds(body(name, receiver_key), prop)[0][1] >= -0.03)
    elif mode == "contacts":
        require("normal give creates four physical contact participants", len(samples["contacts-given"]["physics"]["bodies"]) == 8)
        require("native walking retains ownership during item contact", held("contacts-carry-before") == 2 ** 64 - 1
                and held("contacts-carry-after") == 2 ** 64 - 1)
        carried_target = 2 ** 64 - 3
        require("the held sphere moves an existing dynamic table cup",
                distance(body_position(body("contacts-carry-before", carried_target)),
                         body_position(body("contacts-carry-after", carried_target))) > 0.02)
        require("normal E holds the heavy contact sphere", held("contacts-held") == 2 ** 64 - 1)
        require("normal mouse throw releases the heavy contact sphere", held("contacts-thrown") is None)
        keys = {"box": next(entry[0] for entry in initial["authored"] if entry[1] == "floor_box"),
                "lower": 2 ** 64 - 3, "upper": 2 ** 64 - 4}
        stack = [body("contacts-stack", key) for key in keys.values()]
        require("three-body stack retains ordered supports", all(abs(body_position(item)[0] - 6) < 0.15
                and abs(body_position(item)[2] + 1) < 0.15 for item in stack)
                and body_position(stack[0])[1] < body_position(stack[1])[1] < body_position(stack[2])[1])
        require("contact campaign settles a bounded set of bodies", samples["contacts-rest"]["physics"]["stats"]["sleeping_bodies"] >= 5)
    elif mode == "source":
        require("native fixture setup produces an actual tipped cup", abs(cup_up_y(body("source-tipped"))) < 0.15
                and held("source-tipped") is None)
        require("normal Interact corrects tipped cup to upright", held("source-upright") == cup_key
                and cup_up_y(body("source-upright")) >= 0.98)
        for name in ("source-upright", "source-pitch"):
            check_centered_hold(name, samples[name], body(name), checks)
        for name in ("source-pitch", "source-wall-start", "source-toward-wall", "source-along-start",
                     "source-along-wall", "source-away-start", "source-away-wall", "source-corner"):
            require(f"{name}: Source hold stays upright", held(name) == cup_key and cup_up_y(body(name)) >= 0.98)
            clearance = cup_player_clearance(samples[name], body(name))
            checks.append(dict(name=f"{name}: held cup clears player capsule", passed=clearance >= -0.01,
                               clearance_m=clearance))
            low, high = cup_bounds(body(name))
            require(f"{name}: held cup remains within solid room", low[0] >= -3.01 and high[0] <= 3.01
                    and low[2] >= -3.01 and high[2] <= 3.01)
        require("native walking approaches the wall", samples["source-toward-wall"]["player_position"][0]
                > samples["source-wall-start"]["player_position"][0] + 0.15)
        require("native walking travels along the wall", samples["source-along-wall"]["player_position"][2]
                < samples["source-along-start"]["player_position"][2] - 0.15)
        require("native walking moves away from the wall", samples["source-away-wall"]["player_position"][0]
                < samples["source-away-start"]["player_position"][0] - 0.15)
        near_forward = cup_center(body("source-toward-wall"))[0] - samples["source-toward-wall"]["player_position"][0]
        far_forward = samples["source-away-wall"]["player_position"][0] - cup_center(body("source-away-wall"))[0]
        checks.append(dict(name="wall contact shortens the relative hold distance", passed=0.1 <= near_forward < 0.7
                           and far_forward > near_forward + 0.1, wall_forward_m=near_forward, clear_forward_m=far_forward))
        require("Interact drops the upright Source-held cup", held("source-dropped") is None)
    elif mode == "throw":
        for name in ("flat", "up", "down", "wall"):
            require(f"{name}: normal Interact holds the cup", held(f"throw-{name}-held") == cup_key)
            require(f"{name}: upright hold precedes mouse throw", held(f"throw-{name}-ready") == cup_key
                    and cup_up_y(body(f"throw-{name}-ready")) >= 0.98)
        for name in ("flat", "up", "down"):
            require(f"{name}: native left click throws", held(f"throw-{name}-launched") is None)
        require("close-wall throw has a valid ownership result", held("throw-wall-launched") in (None, cup_key))
        for name in ("throw-wall-ready", "throw-wall-launched", "throw-wall-flight", "throw-rest"):
            require(f"{name}: cup never passes through close wall", cup_bounds(body(name))[1][0] <= 3.01)
    elif mode == "throw-controls":
        for name in ("throw-paused-before", "throw-paused-after", "throw-console-before", "throw-console-after",
                     "throw-unfocused-before", "throw-unfocused-after", "throw-focus-click", "throw-recapture-click"):
            require(f"{name}: gated mouse click retains held ownership", held(name) == cup_key)
        require("paused mouse input does not advance physics", frozen("throw-paused-before", "throw-paused-after"))
        require("console mouse input does not advance physics", frozen("throw-console-before", "throw-console-after"))
        require("focus gain click restores focus without throwing", samples["throw-focus-click"]["window_focused"])
        require("recapture click restores relative mouse without throwing", samples["throw-recapture-click"]["relative_mouse"])
        require("later deliberate mouse click throws", held("throw-deliberate-click") is None)
    elif mode == "ball":
        original_keys = {item["key"] for item in initial["bodies"]}
        given = [item for item in samples["ball-given"]["physics"]["bodies"] if item["key"] not in original_keys]
        require("console gives one real sphere body", len(given) == 1
                and len(samples["ball-given"]["physics"]["bodies"]) == BODY_COUNT + 1)
        if not given:
            return
        key = given[0]["key"]
        require("given sphere uses reserved developer key", key == 2 ** 64 - 1)
        require("native Interact picks up given sphere", held("ball-held") == key)
        require("view turn carries the same sphere", held("ball-carry") == key
                and distance(body_position(body("ball-carry", key)), body_position(body("ball-held", key))) > 0.3)
        release_velocity = vector(body("ball-released", key)["linear_velocity"], 3)
        require("moving gameplay release drops the sphere", held("ball-released") is None
                and math.hypot(release_velocity[0], release_velocity[2]) > 0.3
                and samples["ball-released"]["player_position"][0]
                > samples["ball-carry"]["player_position"][0] + 0.1)
        require("sphere falls after native drop", body_position(body("ball-rest", key))[1]
                < body_position(body("ball-carry", key))[1] - 0.7)
        for name in ("ball-given", "ball-held", "ball-carry", "ball-released", "ball-rolled", "ball-rest"):
            require(f"{name}: sphere remains in live lifecycle", len(samples[name]["physics"]["bodies"]) == BODY_COUNT + 1)
    elif mode == "timesteps":
        props = {prop["id"]: prop for prop in json.loads(FIXTURE.read_text())["props"]
                 if prop.get("physics", {}).get("motion") == "dynamic"}
        names = {entry[0]: entry[1] for entry in initial.get("authored", [])}
        expected_clamp = max(0, sim_delta - 0.1)
        for name in ("initial", "timestep-1000", "timestep-2000", "timestep-4000"):
            sample = samples[name]
            stats = sample["physics"]["stats"]
            require(f"{name}: native delta clamp matches request", abs(
                stats["clamped_seconds"] - expected_clamp) < 0.00001)
            require(f"{name}: bounded timestep work matches request", stats["substeps"]
                    <= min(physics_step_cap(stats), math.ceil(sim_delta / physics_step_seconds(stats) + 0.00001)))
            require(f"{name}: no unexpected held ownership", held(name) is None)
            for item in sample["physics"]["bodies"]:
                prop = props[names[item["key"]]]
                require(f"{name}: {prop['id']} stays above native floor",
                        fixture_floor_gap(item, prop) >= -COLLISION_TOLERANCE_M)
        box_key = next(key for key, name in names.items() if name == "floor_box")
        require("native falling box settles on the floor", abs(fixture_floor_gap(
            body("timestep-4000", box_key), props["floor_box"])) < COLLISION_TOLERANCE_M)
    elif mode == "blocked-release":
        require("live-body obstruction refuses release and retains the cup", held("release-blocked") == cup_key)
        require("moving the blocking box clear preserves the same held cup", held("release-clear") == cup_key)
        require("fresh Interact releases after moving clear", held("clear-released") is None)
        require("safe release cup settles on native floor", abs(cup_bounds(body("blocked-rest"))[0][1]) < 0.025)
    elif mode == "video":
        require("video captures carried view and pitch motion", held("video-carry") == cup_key
                and distance(body_position(body("video-carry")), body_position(body("held"))) > 0.2)
        require("video captures real drop", held("video-released") is None)
        require("video captures gravity after release", body_position(body("video-rest"))[1]
                < body_position(body("video-carry"))[1] - 0.6)
        require("video cup rests on native floor", abs(cup_bounds(body("video-rest"))[0][1]) < 0.025)
    elif mode == "pickup":
        require("held cup follows translated and rotated view", held("view-carry") == cup_key
                and distance(body_position(body("view-carry")), body_position(body("held"))) > 0.4)
        require("stationary drop releases ownership", held("released") is None)
        require("released cup falls under gravity", body_position(body("settled"))[1]
                < body_position(body("before-drop"))[1] - 0.6)
        require("cup rests on the real floor", abs(cup_bounds(body("settled"))[0][1]) < 0.025)
        require("second Interact picks up the same body", held("reheld") == cup_key)
        require("second drop releases the same body", held("redropped") is None)
        require("fixed cup refuses pickup", held("fixed-refused") is None)
    elif mode == "wall":
        require("wall constrains held body", held("wall-held") == cup_key
                and cup_bounds(body("wall-held"))[1][0] <= 3.01)
        require("wall release keeps a valid ownership result", held("wall-release") in (None, cup_key))
        require("walking into wall stops the player", samples["wall-contact"]["player_position"][0] <= 2.71)
        require("released or held cup never crosses wall", all(
            cup_bounds(body(name))[1][0] <= 3.01
            for name in ("wall-release", "wall-contact", "wall-rest")))
        clearance = cup_player_clearance(samples["wall-held"], body("wall-held"))
        checks.append(dict(name="held cup stays clear of walking player and camera",
                           passed=clearance >= -0.01, clearance_m=clearance))
        checks.append(dict(name="wall release outcome", passed=True,
                           detail="released" if held("wall-release") is None else "blocked and retained"))
    elif mode == "corner":
        require("corner constrains held cup", held("corner-held") == cup_key)
        for name in ("corner-held", "corner-release", "corner-rest"):
            low, high = cup_bounds(body(name))
            checks.append(dict(name=f"{name}: cup remains inside both corner walls",
                               passed=high[0] <= 3.01 and low[2] >= -3.01,
                               east_edge_m=high[0], north_edge_m=low[2]))
        clearance = cup_player_clearance(samples["corner-held"], body("corner-held"))
        checks.append(dict(name="corner-held cup stays clear of player and camera",
                           passed=clearance >= -0.01, clearance_m=clearance))
        require("corner release retains a valid ownership result", held("corner-release") in (None, cup_key))
    elif mode == "transitions":
        require("pause owns gameplay", all(samples[name]["state"] == "Paused"
                for name in ("paused-before", "paused-after")))
        require("pause freezes physics and retains held cup", frozen("paused-before", "paused-after")
                and held("paused-after") == cup_key)
        require("console owns gameplay", all(samples[name]["console_open"]
                for name in ("console-before", "console-after")))
        require("console keys do not drop or advance held cup", frozen("console-before", "console-after")
                and held("console-after") == cup_key)
        require("noclip refuses while the cup is held", not samples["noclip-refused"]["noclip"]
                and held("noclip-refused") == cup_key
                and "drop" in (samples["noclip-refused"].get("console_last") or "").lower())
        require("gameplay drop clears ownership before noclip", held("transition-drop") is None)
        require("noclip works after the held item is dropped", samples["noclip"]["noclip"] and held("noclip") is None)
        for name, quality in (("quality-medium", "medium"), ("quality-high", "high")):
            require(f"{quality} rebuild preserves item lifecycle", len(samples[name]["physics"]["bodies"]) == BODY_COUNT
                    and samples[name]["quality"].lower() == quality
                    and samples[name]["renderer_quality"].lower() == quality)
        require("map reload clears held ownership", held("reloaded") is None)
        require("map reload restores authored cup", distance(body_position(body("reloaded")), [0, 0.75, -0.65]) < 0.03)
    elif mode == "stress":
        require("invalid give does not create a body", len(samples["invalid-refused"]["physics"]["bodies"]) == BODY_COUNT
                and "error" in (samples["invalid-refused"].get("console_last") or "").lower())
        for name in ("stress-16-console", "cap-refused", "stress-16", "stress-settled"):
            require(f"{name}: exactly 16 physical given items", len(samples[name]["physics"]["bodies"]) == BODY_COUNT + 16)
        require("17th give reports the bounded cap", "16" in (samples["cap-refused"].get("console_last") or "")
                and "limit" in (samples["cap-refused"].get("console_last") or "").lower())
        require("disable console removes all given physics bodies", len(samples["given-cleared"]["physics"]["bodies"]) == BODY_COUNT)


def check_dormancy_samples(samples: dict, checks: list[dict]) -> None:
    original = {body["key"]: body for body in samples["initial"]["physics"]["bodies"]}
    keys = {entry[1]: entry[0] for entry in samples["initial"]["physics"]["authored"]}
    cup_key = keys["pickup_cup"]
    def require(name: str, condition: bool) -> None:
        checks.append(dict(name=name, passed=bool(condition)))
    def bodies(name: str) -> dict:
        return {body["key"]: body for body in samples[name]["physics"]["bodies"]}
    require("an empty initial view has no interaction hand", samples["initial"].get("interaction_hand_visible") is False)
    require("the normal direct cup aim shows the interaction hand before pickup", samples["dormant-aim"].get("interaction_hand_visible") is True)
    for name in ("initial", "dormant-wait", "dormant-aim", "dormant-reset", "dormant-reset-wait"):
        current = bodies(name)
        require(f"{name}: all authored bodies remain exact and dormant", set(current) == set(original)
                and samples[name]["physics"]["stats"].get("dormant_bodies") == BODY_COUNT
                and all(current[key].get("dormant") is True and current[key]["pose"] == original[key]["pose"]
                        and vector(current[key]["linear_velocity"], 3) == [0, 0, 0]
                        and vector(current[key]["angular_velocity"], 3) == [0, 0, 0] for key in original))
    require("dormancy fixture contains genuinely unsupported initial placements", original[keys["floor_box"]]["pose"]["position"][1] >= 0.65)
    for name in ("dormant-held", "dormant-medium", "dormant-high", "dormant-held-facing-trigger", "dormant-held-away"):
        current = bodies(name)
        require(f"{name}: normal pickup activates only the selected cup", samples[name]["physics"]["held"] == cup_key
                and current[cup_key].get("dormant") is False
                and all(current[key].get("dormant") is True and current[key]["pose"] == original[key]["pose"]
                        for key in original if key != cup_key))
    for name, quality in (("dormant-medium", "medium"), ("dormant-high", "high")):
        require(f"{name}: quality rebuild retains dormant keys and held ownership", samples[name]["quality"].lower() == quality
                and samples[name]["renderer_quality"].lower() == quality)
    for name, id in (("dormant-trigger-target", "fixture_activate"), ("dormant-reset-target", "fixture_reset")):
        target = samples[name].get("interaction_target") or {}
        require(f"{name}: normal view ray selects the actual authored switch", target.get("kind") == "world" and target.get("id") == id)
        require(f"{name}: the unheld authored switch shows the interaction hand", samples[name].get("interaction_hand_visible") is True)
    for name in ("dormant-activated", "dormant-fallen"):
        require(f"{name}: explicit native E binding activates all four bodies", all(body.get("dormant") is False for body in bodies(name).values()))
    require("explicit activation permits real gravity at the unsupported box", body_position(bodies("dormant-fallen")[keys["floor_box"]])[1]
            < body_position(original[keys["floor_box"]])[1] - 0.25)
    require("ordinary E drop and authored reset leave no held item", samples["dormant-drop"]["physics"]["held"] is None
            and samples["dormant-reset"]["physics"]["held"] is None)


def check_bowling_samples(samples: dict, checks: list[dict]) -> None:
    def require(name: str, condition: bool) -> None:
        checks.append(dict(name=name, passed=bool(condition)))
    initial = samples["bowling-initial"]["physics"]
    ids = {entry[1]: entry[0] for entry in initial["authored"]}
    pin_keys = [key for id, key in ids.items() if id.startswith("lane_1_pin_")]
    require("playable lane owns exactly ten actual pins and one ball", len(pin_keys) == 10
            and len(initial["bodies"]) == len(ids) == 11 and BOWLING_BALL in ids)
    if len(pin_keys) != 10 or BOWLING_BALL not in ids:
        return
    ball_key = ids[BOWLING_BALL]
    originals = {body["key"]: body for body in initial["bodies"]}
    def bodies(name: str) -> dict:
        return {body["key"]: body for body in samples[name]["physics"]["bodies"]}
    for name, sample in samples.items():
        physical = sample["physics"]
        expected_count = 11 if name in ("world-start", "bowling-initial") else 12
        require(f"{name}: scoped body count remains constant", len(physical["bodies"]) == expected_count
                and (name == "world-start" or physical["stats"]["body_count"] == expected_count))
        require(f"{name}: fixed-step solver work stays bounded", 0 <= physical["stats"]["substeps"] <= physics_step_cap(physical["stats"]))
        for body in physical["bodies"]:
            position = body_position(body)
            rotation = vector(body["pose"]["rotation"], 4)
            vector(body["linear_velocity"], 3); vector(body["angular_velocity"], 3)
            require(f"{name}: physical body {body['key']} remains finite inside the arcade",
                    8.9 <= position[0] <= 20.2 and -0.20 <= position[1] <= 3.4 and -0.2 <= position[2] <= 30.2
                    and abs(sum(component * component for component in rotation) - 1) <= 0.02)
    require("all ten compound pins stand stably before play", all(cup_up_y(originals[key]) >= 0.98
            and abs(float(originals[key].get("mass_kg", 0)) - 1.5) < 0.05 for key in pin_keys))
    require("the playable sphere retains its ordinary five-kilogram mass", abs(float(originals[ball_key].get("mass_kg", 0)) - 5) < 0.05)
    require("all eleven authored bowling members load exact and dormant", all(body.get("dormant") is True for body in originals.values())
            and initial["stats"].get("dormant_bodies") == 11)
    for trial in (1, 2):
        require(f"shot {trial}: normal E picks up the same ball", samples[f"bowling-held-{trial}"]["physics"]["held"] == ball_key)
        held_bodies = bodies(f"bowling-held-{trial}")
        require(f"shot {trial}: pickup wakes the ball while the untouched pin rack stays dormant", held_bodies[ball_key].get("dormant") is False
                and all(held_bodies[key].get("dormant") is True and held_bodies[key]["pose"] == originals[key]["pose"] for key in pin_keys))
        require(f"shot {trial}: real shot begins at the existing foul line", samples[f"bowling-ready-{trial}"]["physics"]["held"] == ball_key
                and distance(vector(samples[f"bowling-ready-{trial}"]["player_position"], 3), [10.65, 1.6, 20.2]) < 0.03)
        require(f"shot {trial}: normal LMB releases the ball", samples[f"bowling-launch-{trial}"]["physics"]["held"] is None)
        scattered = bodies(f"bowling-scatter-{trial}")
        moved = sum(distance(body_position(scattered[key]), body_position(originals[key])) > 0.04 for key in pin_keys)
        tipped = sum(cup_up_y(scattered[key]) < 0.7 for key in pin_keys)
        checks.append(dict(name=f"shot {trial}: full-lane native play scatters and topples real pins", passed=moved >= 3 and tipped >= 2,
                           displaced_pins=moved, tipped_pins=tipped))
        target = samples[f"bowling-reset-target-{trial}"].get("interaction_target") or {}
        require(f"reset {trial}: actual view selects the authored reset switch", target.get("kind") == "world" and target.get("id") == BOWLING_RESET)
    target = samples["bowling-reset-blocked-target"].get("interaction_target") or {}
    require("blocked reset is requested through the real switch and normal E", target.get("kind") == "world" and target.get("id") == BOWLING_RESET)
    occupied = vector(samples["bowling-reset-blocked-target"]["player_position"], 3)
    require("blocked reset places the actual player over the authored ball destination", math.hypot(occupied[0] - 10.65, occupied[2] - 21.2) < 0.03
            and distance(body_position(bodies("bowling-reset-blocked-target")[ball_key]), body_position(originals[ball_key])) > 2)
    require("blocked reset preserves scattered ownership and member keys", samples["bowling-reset-blocked"]["physics"]["held"] is None
            and set(bodies("bowling-reset-blocked")) == set(bodies("bowling-reset-blocked-target")))
    for name in ("bowling-reset-1", "bowling-reset-2", "bowling-held-reset", "bowling-reset-pins-1", "bowling-reset-pins-2", "bowling-reset-pins-held", "bowling-rest"):
        reset = bodies(name)
        require(f"{name}: all eleven group members return to authored poses", all(key in reset
                and distance(body_position(reset[key]), body_position(originals[key])) < 0.025
                and abs(sum(a * b for a, b in zip(vector(reset[key]["pose"]["rotation"], 4), vector(originals[key]["pose"]["rotation"], 4)))) > 0.995
                for key in originals))
        require(f"{name}: reset clears group motion", all(distance(vector(reset[key]["linear_velocity"], 3), [0, 0, 0]) < 0.1
                and distance(vector(reset[key]["angular_velocity"], 3), [0, 0, 0]) < 0.25 for key in originals))
        require(f"{name}: reset restores all members to dormant", all(reset[key].get("dormant") is True for key in originals))
        require(f"{name}: all ten reset pins remain upright", all(cup_up_y(reset[key]) >= 0.98 for key in pin_keys))
        renderers = lighting_snapshots(samples[name])
        for key in originals:
            live = (renderers.get(key) or {}).get("live") or {}
            published = (renderers.get(key) or {}).get("published") or {}
            cache = ((renderers.get(key) or {}).get("sample") or {}).get("cache") or {}
            def matching(left: object, right: object, length: int) -> bool:
                if not isinstance(left, list) or not isinstance(right, list):
                    return False
                a, b = [value for row in left for value in row], [value for row in right for value in row]
                return len(a) == len(b) == length and all(math.isfinite(x) and math.isfinite(y) and abs(x - y) < 0.0001 for x, y in zip(a, b))
            require(f"{name}: reset body {key} publishes its live model and normal transform",
                    bool(live) and bool(published) and bool(cache)
                    and distance(vector(live["position"], 3), body_position(reset[key])) <= 0.001
                    and matching(live.get("model"), cache.get("sampled_model"), 16)
                    and matching(live.get("model"), published.get("model"), 16)
                    and matching(live.get("normal_model"), published.get("normal_model"), 12))
    other_key = 2 ** 64 - 1
    for before, after in (("bowling-reset-target-1", "bowling-reset-1"), ("bowling-reset-target-2", "bowling-reset-2"),
                          ("bowling-held-before-reset", "bowling-held-reset")):
        left, right = bodies(before)[other_key], bodies(after)[other_key]
        require(f"{after}: unrelated cup keeps its exact solver state", left["pose"] == right["pose"]
                and left["linear_velocity"] == right["linear_velocity"] and left["angular_velocity"] == right["angular_velocity"])
        require(f"{after}: reset leaves player and existing lights unchanged", distance(vector(samples[before]["player_position"], 3),
                vector(samples[after]["player_position"], 3)) < 0.001 and samples[before]["light_states"] == samples[after]["light_states"])
    require("labelled actor-binding reset releases its held member", samples["bowling-held-before-reset"]["physics"]["held"] == ball_key
            and samples["bowling-held-reset"]["physics"]["held"] is None)
    props = {prop["id"]: prop for prop in json.loads(BOWLING_FIXTURE.read_text())["props"] if prop.get("physics", {}).get("motion") == "dynamic"}
    for name in [name for name in samples if name.startswith("bowling-video-")] + ["bowling-scatter-1", "bowling-scatter-2", "bowling-reset-pins-1", "bowling-reset-pins-2", "bowling-reset-pins-held"]:
        eye = vector(samples[name]["player_position"], 3)
        require(f"{name}: close pin-deck camera stays on the ordinary inter-lane deck", math.hypot(eye[0] - 11.7, eye[2] - 5.5) < 0.03)
        clearance = math.inf
        for id, key in ids.items():
            low, high = fixture_body_bounds(bodies(name)[key], props[id])
            clearance = min(clearance, math.hypot(max(low[0] - eye[0], 0, eye[0] - high[0]),
                                                 max(low[2] - eye[2], 0, eye[2] - high[2])) - 0.3)
        checks.append(dict(name=f"{name}: the evidence camera stays clear of every live lane body", passed=clearance >= 0.025, clearance_m=clearance))


def check_escape_samples(samples: dict, checks: list[dict]) -> None:
    def require(name: str, passed: bool, **fields: object) -> None:
        checks.append(dict(name=name, passed=bool(passed), **fields))
    initial = samples["initial"]["physics"]
    ids = {entry[1]: entry[0] for entry in initial["authored"]}
    require("escape cases retain four original authored items", len(initial["bodies"]) == BODY_COUNT
            and set(ids) == {"pickup_cup", "stairs_ball", "ramp_bottle", "floor_box"})
    pin_key = 2 ** 64 - 1
    for name, sample in samples.items():
        physics = sample["physics"]
        expected_count = BODY_COUNT if name in ("world-start", "initial") else BODY_COUNT + 1
        require(f"{name}: escape diagnostics retain bounded stable body ownership", len(physics["bodies"]) == expected_count)
        for body in physics["bodies"]:
            vector(body["pose"]["position"], 3)
            rotation = vector(body["pose"]["rotation"], 4)
            vector(body["linear_velocity"], 3); vector(body["angular_velocity"], 3)
            require(f"{name}: body {body['key']} retains a finite normalized transform", abs(sum(value * value for value in rotation) - 1) < 0.02)
            require(f"{name}: no body is suspended", body.get("state") != "suspended")
    given = {body["key"]: body for body in samples["escape-given"]["physics"]["bodies"]}
    require("normal give supplies a supported physical convex pin", pin_key in given and given[pin_key].get("motion") == "dynamic"
            and abs(given[pin_key].get("solver_mass_kg", math.nan) - 1.5) < 0.001)
    cup_key = ids["pickup_cup"]
    require("normal E holds the authored cup for constrained-wall coverage", samples["escape-held"]["physics"]["held"] == cup_key)
    wall = samples["escape-held-wall"]
    held_body = next(body for body in wall["physics"]["bodies"] if body["key"] == cup_key)
    require("native held item remains owned at the wall", wall["physics"]["held"] == cup_key)
    require("held-wall shape stays on the playable side of the wall", cup_bounds(held_body)[1][0] <= 3.0 + COLLISION_TOLERANCE_M)
    require("held-wall shape stays clear of the player capsule", cup_player_clearance(wall, held_body) >= -0.015)
    require("normal E releases the safely constrained item", samples["escape-wall-release"]["physics"]["held"] is None)
    for name, key, dormant in (("escape-authored-recovered", cup_key, True), ("escape-spawned-recovered", pin_key, False)):
        body = next(body for body in samples[name]["physics"]["bodies"] if body["key"] == key)
        position = body_position(body)
        require(f"{name}: recovery returns to an ordinary covered room", -3 <= position[0] <= 9 and -3 <= position[2] <= 3
                and -COLLISION_TOLERANCE_M <= position[1] <= 3.2)
        require(f"{name}: recovery preserves authored versus spawned lifecycle", body.get("dormant") is dormant and not body["held"])
        if dormant:
            require(f"{name}: authored recovery remains exactly at rest", distance(vector(body["linear_velocity"], 3), [0, 0, 0]) < 0.00001
                    and distance(vector(body["angular_velocity"], 3), [0, 0, 0]) < 0.00001)
        else:
            # A spawned body resumes gravity/contact simulation after its
            # recovery velocity is cleared. Initial clearing is proven from
            # the event's first solver frame below; later roll/tip is measured.
            require(f"{name}: spawned recovery resumes its normal dynamic lifecycle", body["motion"] == "dynamic"
                    and body.get("state") in ("active", "sleeping"))
    require("escape campaign ends with no stale held owner", samples["escape-rest"]["physics"]["held"] is None)


def solved_contact_pairs(frame: dict) -> list[list[int]]:
    """Select physical response independently of the participant wake state."""
    return [row["pair"] for row in frame.get("contact_impulses", [])
            if math.isfinite(row.get("impulse", math.nan)) and row["impulse"] >= 0.01]


def audit_bowling_mass_contact(frames: list[dict], ball_key: int, pin_keys: set[int], checks: list[dict]) -> dict:
    """Report real first-impact mass, COM, inertia and momentum on both sides.

    This is an observation of the solved ball/rack/floor contact, not an
    isolated elastic two-body formula: floor friction and other pins supply
    external impulses. The receipt retains numerical changes for review.
    """
    impact = next((index for index, frame in enumerate(frames) if any(ball_key in pair and any(key in pin_keys for key in pair)
                   for pair in solved_contact_pairs(frame))), None)
    if impact is None or impact == 0:
        checks.append(dict(name="first bowling impact has adjacent native mass/contact evidence", passed=False))
        return dict(first_impact=None)
    before, after = frames[impact - 1], frames[impact]
    touched = {key for pair in solved_contact_pairs(after) if ball_key in pair for key in pair if key in pin_keys}
    keys = touched | {ball_key}
    def metadata(body: dict) -> dict:
        velocity = vector(body["linear_velocity"], 3)
        mass = body.get("solver_mass_kg")
        return dict(authored_mass_kg=body.get("mass_kg"), solver_mass_kg=mass,
                    effective_mass_kg=body.get("effective_mass_kg"), local_center_of_mass=body.get("local_center_of_mass"),
                    world_center_of_mass=body.get("world_center_of_mass"), principal_inertia=body.get("principal_inertia"),
                    state=body.get("state"), dormant=body.get("dormant"), linear_velocity=velocity,
                    angular_velocity=body["angular_velocity"], momentum_kg_m_s=[mass * component for component in velocity] if mass is not None else None)
    old = {body["key"]: body for body in before["bodies"]}
    new = {body["key"]: body for body in after["bodies"]}
    rows = {str(key): dict(before=metadata(old[key]), after=metadata(new[key])) for key in sorted(keys)}
    active = [new[key] for key in keys]
    checks.append(dict(name="first bowling impact reports actual solver mass, COM and inertia", passed=all(
        body.get("solver_mass_kg", 0) > 0 and all(field in body for field in ("effective_mass_kg", "local_center_of_mass", "world_center_of_mass", "principal_inertia"))
        and all(value > 0 for value in vector(body["principal_inertia"], 3)) for body in active)))
    checks.append(dict(name="actual bowling impact retains the authored 5 kg ball and 1.5 kg pins", passed=abs(new[ball_key].get("solver_mass_kg", math.nan) - 5) < 0.001
                       and all(abs(new[key].get("solver_mass_kg", math.nan) - 1.5) < 0.001 for key in touched)))
    checks.append(dict(name="contact activates finite pin effective masses before response", passed=bool(touched) and all(
        new[key].get("dormant") is False and all(abs(value - 1.5) < 0.001 for value in vector(new[key].get("effective_mass_kg", [0, 0, 0]), 3)) for key in touched)))
    response_window = []
    for frame in frames[impact:impact + 3]:
        participants = {body["key"]: body for body in frame["bodies"] if body["key"] in keys}
        response_window.append(dict(frame=frame["frame"], bodies={str(key): metadata(body) for key, body in participants.items()}))
    pin_speed = max(distance(vector(body["linear_velocity"], 3), [0, 0, 0]) for frame in frames[impact:impact + 3]
                    for body in frame["bodies"] if body["key"] in touched)
    checks.append(dict(name="the first native rack impact gives real motion to activated pins", passed=pin_speed > 0.05))
    def total(side: str) -> list[float] | None:
        momenta = [row[side]["momentum_kg_m_s"] for row in rows.values()]
        return [sum(momentum[dimension] for momentum in momenta) for dimension in range(3)] if all(momentum is not None for momentum in momenta) else None
    approach = vector(old[ball_key]["linear_velocity"], 3)
    approach[1] = 0
    approach_length = distance(approach, [0, 0, 0])
    direction = [value / approach_length for value in approach] if approach_length > 1e-9 else [0, 0, 0]
    longitudinal = {}
    for key, row in rows.items():
        left, right = row["before"]["momentum_kg_m_s"], row["after"]["momentum_kg_m_s"]
        longitudinal[key] = dict(before=sum(a * b for a, b in zip(left, direction)) if left is not None else None,
                                 after=sum(a * b for a, b in zip(right, direction)) if right is not None else None)
    return dict(before_frame=before["frame"], contact_frame=after["frame"], contacts=after.get("contacts", []), contact_impulses=after.get("contact_impulses", []), participants=rows,
                first_three_contact_frames=response_window, maximum_contacted_pin_speed_m_s=pin_speed,
                approach_axis=direction, longitudinal_momentum_kg_m_s=longitudinal,
                combined_momentum_before_kg_m_s=total("before"), combined_momentum_after_kg_m_s=total("after"),
                expected_ball_to_pin_mass_ratio=5 / 1.5,
                caveat="Solved floor/rack contact has external friction and other-pin impulses; the recorded pair momentum is not an isolated conservation test.")


def audit_bowling_frames(events: list[dict], samples: dict, checks: list[dict]) -> dict:
    ids = {entry[1]: entry[0] for entry in samples["bowling-initial"]["physics"]["authored"]}
    pin_keys = {key for id, key in ids.items() if id.startswith("lane_1_pin_")}
    ball_key = ids[BOWLING_BALL]
    props = {prop["id"]: prop for prop in json.loads(BOWLING_FIXTURE.read_text())["props"] if prop.get("physics", {}).get("motion") == "dynamic"}
    shapes = {key: props[id] for id, key in ids.items()}
    shapes[2 ** 64 - 1] = dict(size=[0.085, 0.122, 0.085], physics=dict(shape=dict(kind="cylinder", half_height=0.061, radius=0.0425)))
    frames = [json.loads(event["detail"]) for event in events if event["event"] == "physics_frame"]
    pairs = {tuple(sorted(pair)) for frame in frames for pair in frame.get("contacts", [])}
    ball_pin = [pair for pair in pairs if ball_key in pair and any(key in pin_keys for key in pair)]
    pin_pin = [pair for pair in pairs if set(pair) <= pin_keys]
    impact_wakes = []
    for frame in frames:
        frame_bodies = {body["key"]: body for body in frame["bodies"]}
        for pair in solved_contact_pairs(frame):
            if ball_key in pair:
                for key in pair:
                    if key in pin_keys:
                        impact_wakes.append(frame_bodies[key].get("dormant") is False)
    minimum_ground_gap, minimum_deck_gap = math.inf, math.inf
    surfaces = bowling_support_surfaces()
    meshes = {key: model_mesh(props[id]["model"])[0] for id, key in ids.items()}
    visual_gaps = {"solver_pose": dict(gap_m=math.inf), "render_pose": dict(gap_m=math.inf)}
    ball_positions, ball_spin = [], []
    for frame in frames:
        for body in frame["bodies"]:
            low, high = fixture_body_bounds(body, shapes[body["key"]])
            minimum_ground_gap = min(minimum_ground_gap, low[1] + 0.17)
            if low[0] >= 10.125 and high[0] <= 11.175 and low[2] >= 1.5 and high[2] <= 20:
                minimum_deck_gap = min(minimum_deck_gap, low[1])
            if body["key"] in meshes:
                for pose_name, rendered in (("solver_pose", False), ("render_pose", True)):
                    if rendered and "render_pose" not in body:
                        continue
                    gap, vertex = mesh_floor_gap(body, shapes[body["key"]], meshes[body["key"]], surfaces, rendered)
                    if gap < visual_gaps[pose_name]["gap_m"]:
                        visual_gaps[pose_name] = dict(gap_m=gap, frame=frame["frame"], key=body["key"],
                                                      world_vertex=vertex, visible_support_y=bowling_floor_height(vertex[0], vertex[2], surfaces))
            if body["key"] == ball_key and not body["held"]:
                ball_positions.append(body_position(body)[2])
                ball_spin.append(distance(vector(body["angular_velocity"], 3), [0, 0, 0]))
    resets = [json.loads(event["detail"]) for event in events if event["event"] == "physics_group_reset"]
    blocked_indices = [index for index, event in enumerate(events) if event["event"] == "physics_group_reset_blocked"]
    blocked = [json.loads(events[index]["detail"]) for index in blocked_indices]
    checks.append(dict(name="bowling has continuous actual solver contact evidence", passed=len(frames) >= 100))
    checks.append(dict(name="actual solver reports ball-to-pin contacts", passed=bool(ball_pin)))
    checks.append(dict(name="active ball impacts wake dormant pins for the actual solver response", passed=bool(impact_wakes) and all(impact_wakes)))
    checks.append(dict(name="actual solver reports pin-to-pin contacts", passed=bool(pin_pin)))
    checks.append(dict(name="ordinary throw rolls the actual full lane to the pin deck", passed=bool(ball_positions)
                       and min(ball_positions) < 3.75 and max(ball_spin, default=0) > 1))
    checks.append(dict(name="physical compound pins and sphere stay above the lane and recessed base", passed=minimum_ground_gap >= -COLLISION_TOLERANCE_M
                       and minimum_deck_gap >= -COLLISION_TOLERANCE_M,
                       minimum_ground_gap_m=minimum_ground_gap, minimum_deck_gap_m=minimum_deck_gap, tolerance_m=COLLISION_TOLERANCE_M))
    for pose_name, evidence in visual_gaps.items():
        checks.append(dict(name=f"{pose_name}: actual GLB pin/ball vertices stay above the visible wood and curved gutter stock",
                           passed=math.isfinite(evidence["gap_m"]) and evidence["gap_m"] >= -COLLISION_TOLERANCE_M,
                           tolerance_m=COLLISION_TOLERANCE_M, **evidence))
    mass_contact = audit_bowling_mass_contact(frames, ball_key, pin_keys, checks)
    checks.append(dict(name="three scoped resets succeed after exactly one deliberately blocked attempt", passed=len(resets) == 3 and len(blocked) == 1
                       and all(row.get("group") == BOWLING_GROUP and row.get("reset_count") == 11 for row in resets)))
    checks.append(dict(name="native reset refusal identifies the occupied ball destination", passed=len(blocked) == 1
                       and blocked[0].get("group") == BOWLING_GROUP and str(ball_key) in blocked[0].get("reason", "")))
    for row in blocked:
        checks.append(dict(name="blocked native reset atomically preserves every pose, velocity and dormant state",
                           passed=bool(row.get("before")) and row.get("before") == row.get("after")
                           and row.get("held_before") == row.get("held_after")))
    continuity = []
    for index in blocked_indices:
        before = next((json.loads(event["detail"]) for event in reversed(events[:index]) if event["event"] == "physics_frame"), None)
        after = next((json.loads(event["detail"]) for event in events[index + 1:] if event["event"] == "physics_frame"), None)
        if before is None or after is None:
            checks.append(dict(name="blocked reset has adjacent actual solver frames", passed=False))
            continue
        left, right = {body["key"]: body for body in before["bodies"]}, {body["key"]: body for body in after["bodies"]}
        dt = max(1, int(after["stats"]["substeps"])) * physics_step_seconds(after["stats"])
        for key in pin_keys | {ball_key}:
            a, b = left[key], right[key]
            move = distance(body_position(a), body_position(b))
            speed = max(distance(vector(body["linear_velocity"], 3), [0, 0, 0]) for body in (a, b))
            spin = max(distance(vector(body["angular_velocity"], 3), [0, 0, 0]) for body in (a, b))
            bound = 2 * dt * (speed + 0.4 * spin) + 0.006
            unchanged = a["pose"] == b["pose"] and a["linear_velocity"] == b["linear_velocity"] and a["angular_velocity"] == b["angular_velocity"]
            stable = (a.get("sleeping") and b.get("sleeping")) or (a.get("dormant") and b.get("dormant"))
            checks.append(dict(name=f"blocked reset preserves body {key} without a pose discontinuity", passed=unchanged if stable else move <= bound,
                               displacement_m=move, native_motion_bound_m=bound, sleeping_or_dormant=bool(stable)))
            continuity.append(dict(key=key, before_frame=before["frame"], after_frame=after["frame"], displacement_m=move,
                                   linear_velocity_before=a["linear_velocity"], linear_velocity_after=b["linear_velocity"]))
    checks.append(dict(name="the held reset reports exactly its released member", passed=sum(row.get("released_held") == ball_key for row in resets) == 1))
    def resources_at(name: str) -> dict | None:
        return lighting_snapshots(samples[name]).get(ball_key, {}).get("resources")
    for before, after in (("bowling-reset-target-1", "bowling-reset-1"), ("bowling-reset-target-2", "bowling-reset-2"),
                          ("bowling-held-before-reset", "bowling-held-reset")):
        left, right = resources_at(before), resources_at(after)
        checks.append(dict(name=f"{after}: reset reuses loaded models and GPU objects", passed=bool(left) and bool(right)
                           and "models_loaded" in left and "models_loaded" in right and "gpu" in left and "gpu" in right
                           and left["models_loaded"] == right["models_loaded"]
                           and all(field in left["gpu"] and field in right["gpu"] and left["gpu"][field] == right["gpu"][field]
                                   for field in ("meshes", "objects"))))
    return dict(frames=len(frames), ball_pin_contacts=ball_pin, pin_pin_contacts=pin_pin, resets=resets, blocked_resets=blocked,
                first_impact_mass_contact=mass_contact, actual_glb_floor_clearance=visual_gaps,
                mesh_evidence={model: dict(path=str(model_mesh(model)[2]), sha256=digest(model_mesh(model)[2]))
                               for model in ("arcade:bowling_pin", "arcade:bowling_ball_cobalt", "arcade:gutter_segment")},
                blocked_adjacent_frame_continuity=continuity,
                held_reset_dispatch="labelled benchmark Activate dispatches the actual authored actor binding",
                minimum_ground_gap=minimum_ground_gap if math.isfinite(minimum_ground_gap) else None,
                minimum_deck_gap=minimum_deck_gap if math.isfinite(minimum_deck_gap) else None,
                minimum_ball_z=min(ball_positions, default=None), maximum_ball_angular_speed=max(ball_spin, default=None))


def distribution(values: list[float]) -> dict | None:
    if not values:
        return None
    values = sorted(values)
    return dict(samples=len(values), median=statistics.median(values),
                p95=values[max(0, math.ceil(len(values) * 0.95) - 1)], worst=values[-1])


def physics_metrics(path: Path) -> dict:
    with path.open() as stream:
        rows = list(csv.DictReader(stream))
    if not rows or "step_millis" not in rows[0]:
        raise ValueError("physics CSV needs native step_millis samples")
    values = [float(row["step_millis"]) for row in rows]
    if not all(math.isfinite(value) and value >= 0 for value in values):
        raise ValueError("invalid native physics timing")
    by_count = {}
    for count in sorted({int(row["body_count"]) for row in rows}):
        measured = [float(row["step_millis"]) for row in rows
                    if int(row["body_count"]) == count and int(row["substeps"]) > 0]
        by_count[str(count)] = distribution(measured)
    report = dict(step_millis=distribution(values), active_step_millis_by_body_count=by_count,
                timing_scope="solver_and_context_step" if "item_update_millis" in rows[0] else "item_update_legacy_column",
                item_update_millis=distribution([float(row["item_update_millis"]) for row in rows]) if "item_update_millis" in rows[0] else None,
                max_substeps=max(int(row["substeps"]) for row in rows),
                clamped_seconds=sum(float(row["clamped_seconds"]) for row in rows),
                body_count=dict(minimum=min(int(row["body_count"]) for row in rows), maximum=max(int(row["body_count"]) for row in rows)),
                active_bodies=dict(peak=max(int(row["active_bodies"]) for row in rows), final=int(rows[-1]["active_bodies"])),
                sleeping_bodies=dict(peak=max(int(row["sleeping_bodies"]) for row in rows), final=int(rows[-1]["sleeping_bodies"])))
    if "dormant_bodies" in rows[0]:
        report["dormant_bodies"] = dict(peak=max(int(row["dormant_bodies"]) for row in rows), final=int(rows[-1]["dormant_bodies"]))
    return report


def audit_physics_frames(events: list[dict], samples: dict, checks: list[dict]) -> dict:
    """Prove motion from native solver frames, never from image selection."""
    frames = [json.loads(event["detail"]) for event in events if event["event"] == "physics_frame"]
    checks.append(dict(name="video contains per-frame native physics evidence", passed=len(frames) >= 20))
    if not frames:
        return dict(frames=0, bodies={})
    checks.append(dict(name="every native video physics frame has bounded solver work",
                       passed=all(0 <= frame["stats"]["substeps"] <= physics_step_cap(frame["stats"]) for frame in frames)))
    initial = samples["initial"]["physics"]
    props = {prop["id"]: prop for prop in json.loads(FIXTURE.read_text())["props"]
             if prop.get("physics", {}).get("motion") == "dynamic"}
    names = {entry[0]: entry[1] for entry in initial["authored"]}
    report = dict(frames=len(frames), simulation_seconds=sum(physics_frame_seconds(frame["stats"]) for frame in frames), bodies={})
    for key, instance in names.items():
        bodies = [body for frame in frames for body in frame["bodies"] if body["key"] == key]
        dynamic = [body for body in bodies if not body["held"]]
        checks.append(dict(name=f"{instance}: continuous native snapshots", passed=len(dynamic) >= 20))
        if not dynamic:
            continue
        velocities = [vector(body["linear_velocity"], 3) for body in dynamic]
        angular = [vector(body["angular_velocity"], 3) for body in dynamic]
        rotations = [vector(body["pose"]["rotation"], 4) for body in dynamic]
        checks.append(dict(name=f"{instance}: every native orientation stays normalized",
                           passed=all(abs(sum(component * component for component in rotation) - 1) < 0.02
                                      for rotation in rotations)))
        gaps = [fixture_floor_gap(body, props[instance]) for body in bodies]
        downward = False
        bounces = 0
        maximum_rebound = 0.0
        for velocity in velocities:
            if velocity[1] < -0.05:
                downward = True
            elif downward and velocity[1] > 0.02:
                bounces += 1
                maximum_rebound = max(maximum_rebound, velocity[1])
                downward = False
        changes = [math.degrees(2 * math.acos(min(1, abs(sum(a * b for a, b in zip(rotations[0], rotation))))))
                   for rotation in rotations]
        linear_speeds = [distance(velocity, [0, 0, 0]) for velocity in velocities]
        angular_speeds = [distance(velocity, [0, 0, 0]) for velocity in angular]
        settled = dynamic[-1]["sleeping"] or (max(linear_speeds[-10:]) < 0.15
                                               and max(angular_speeds[-10:]) < 0.5)
        evidence = dict(samples=len(dynamic), minimum_y_velocity=min(velocity[1] for velocity in velocities),
                        bounce_events=bounces, maximum_rebound_y_velocity=maximum_rebound,
                        max_angular_speed=max(angular_speeds), max_orientation_change_degrees=max(changes),
                        minimum_floor_gap=min(gaps), final_linear_speed=linear_speeds[-1],
                        final_angular_speed=angular_speeds[-1], final_sleeping=dynamic[-1]["sleeping"],
                        settled=settled)
        report["bodies"][instance] = evidence
        checks.append(dict(name=f"{instance}: real downward gravity", passed=evidence["minimum_y_velocity"] < -0.2))
        checks.append(dict(name=f"{instance}: native collision rebound", passed=bounces >= 1))
        checks.append(dict(name=f"{instance}: no per-frame floor penetration", passed=min(gaps) >= -0.03))
        checks.append(dict(name=f"{instance}: native body settles", passed=settled))
        # A level cuboid may correctly remain upright after a straight drop.
        # The curved profiles additionally prove actual rotational motion.
        if instance in ("stairs_ball", "ramp_bottle"):
            checks.append(dict(name=f"{instance}: native angular motion changes orientation",
                               passed=max(angular_speeds) > 0.03 and max(changes) > 2))
    checks.append(dict(name="native frame audit covers all four authored shape profiles",
                       passed=set(report["bodies"]) == set(props)))
    return report


def audit_ball_frames(events: list[dict], samples: dict, checks: list[dict]) -> dict:
    original = {item["key"] for item in samples["initial"]["physics"]["bodies"]}
    given = [item for item in samples["ball-given"]["physics"]["bodies"] if item["key"] not in original]
    if len(given) != 1:
        return dict(samples=0)
    key = given[0]["key"]
    frames = [json.loads(event["detail"]) for event in events if event["event"] == "physics_frame"]
    bodies = [body for frame in frames for body in frame["bodies"] if body["key"] == key and not body["held"]]
    checks.append(dict(name="native sphere release has continuous solver evidence", passed=len(bodies) >= 20))
    if not bodies:
        return dict(samples=0)
    prop = dict(size=[0.216, 0.216, 0.216], physics=dict(shape=dict(kind="sphere", radius=0.108)))
    angular = [distance(vector(body["angular_velocity"], 3), [0, 0, 0]) for body in bodies]
    gaps = [fixture_floor_gap(body, prop) for body in bodies]
    rotations = [vector(body["pose"]["rotation"], 4) for body in bodies]
    orientation = max(math.degrees(2 * math.acos(min(1, abs(sum(a * b for a, b in zip(rotations[0], rotation))))))
                      for rotation in rotations)
    report = dict(samples=len(bodies), key=key, max_angular_speed=max(angular),
                  max_orientation_change_degrees=orientation, minimum_floor_gap=min(gaps),
                  final_linear_speed=distance(vector(bodies[-1]["linear_velocity"], 3), [0, 0, 0]),
                  final_sleeping=bodies[-1]["sleeping"])
    checks.append(dict(name="moving release causes real native sphere rolling", passed=max(angular) > 0.5 and orientation > 5))
    checks.append(dict(name="given sphere never passes through native floor", passed=min(gaps) >= -0.03))
    return report


def audit_throw_frames(events: list[dict], samples: dict, checks: list[dict]) -> dict:
    key = next(entry[0] for entry in samples["initial"]["physics"]["authored"] if entry[1] == "pickup_cup")
    frames = [json.loads(event["detail"]) for event in events if event["event"] == "physics_frame"]
    bodies = [body for frame in frames for body in frame["bodies"] if body["key"] == key and not body["held"]]
    checks.append(dict(name="aimed throws have continuous native solver evidence", passed=len(bodies) >= 20))
    # Screenshot delays use wall time; uncapped fixed-step physics may already
    # have bounced by that capture. Inspect the real dispatcher launch snapshot.
    launches = [json.loads(event["detail"]) for event in events if event["event"] == "item_thrown"]
    velocities = [vector(next(body for body in sample["physics"]["bodies"] if body["key"] == key)["linear_velocity"], 3)
                  for sample in launches]
    checks.append(dict(name="three aimed throws retain original body identity at launch", passed=len(velocities) >= 3))
    if len(velocities) >= 3:
        flat, up, down = velocities[:3]
        checks.append(dict(name="flat throw follows the forward view", passed=flat[2] < -2 and abs(flat[0]) < 1))
        checks.append(dict(name="upward aim gives an upward forward throw", passed=up[2] < -2 and up[1] > 1))
        checks.append(dict(name="downward east aim gives a downward east throw", passed=down[0] > 2 and down[1] < -0.5))
    if not bodies:
        return dict(samples=0)
    angular = [distance(vector(body["angular_velocity"], 3), [0, 0, 0]) for body in bodies]
    linear = [distance(vector(body["linear_velocity"], 3), [0, 0, 0]) for body in bodies]
    gaps = [cup_bounds(body)[0][1] for body in bodies]
    report = dict(samples=len(bodies), key=key, launch_velocities=velocities, max_angular_speed=max(angular), max_linear_speed=max(linear),
                  minimum_floor_gap=min(gaps), final_linear_speed=linear[-1], final_sleeping=bodies[-1]["sleeping"])
    checks.append(dict(name="native mouse throw supplies real launch velocity", passed=max(linear) > 4))
    checks.append(dict(name="thrown cup develops actual angular motion at contact", passed=max(angular) > 0.1))
    checks.append(dict(name="thrown cup never crosses the floor", passed=min(gaps) >= -0.03))
    return report


def audit_surface_frames(events: list[dict], steps: list[dict], samples: dict, checks: list[dict]) -> dict:
    key = samples["surface-initial"]["physics"]["bodies"][0]["key"]
    names = iter(("ice", "wood", "tile", "carpet", "seam"))
    phase = None
    elapsed = 0.0
    measured: dict[str, list[dict]] = {}
    prop = next(prop for prop in json.loads(SURFACE_FIXTURE.read_text())["props"] if prop["id"] == "surface_box")
    seam_rows = []
    for event in events:
        if event["event"] == "action":
            action = steps[int(json.loads(event["detail"])["id"])]["action"]
            if action["kind"] == "item_pose":
                phase = None
            elif action["kind"] == "item_velocity":
                phase, elapsed = next(names), 0.0
                measured[phase] = []
        elif event["event"] == "physics_frame" and phase:
            detail = json.loads(event["detail"])
            elapsed += physics_frame_seconds(detail["stats"])
            body = next(item for item in detail["bodies"] if item["key"] == key)
            velocity = vector(body["linear_velocity"], 3)
            if phase == "seam":
                z = body_position(body)[2]
                low, high = fixture_body_bounds(body, prop)
                region = "ice" if low[2] >= 5.502 else "wood" if low[2] >= 2.502 and high[2] <= 5.498 else "carpet" if high[2] <= 2.498 else "boundary"
                seam_rows.append(dict(simulation_seconds=elapsed, region=region, z=z,
                                      horizontal_speed=math.hypot(velocity[0], velocity[2]),
                                      linear_velocity=velocity, angular_speed=distance(vector(body["angular_velocity"], 3), [0, 0, 0]),
                                      upright_y=cup_up_y(body), physical_bounds=dict(min=low, max=high),
                                      floor_gap=fixture_floor_gap(body, prop), key=body["key"], sleeping=body["sleeping"]))
            elif 0.3 <= elapsed <= 0.4:
                measured[phase].append(dict(simulation_seconds=elapsed, horizontal_speed=math.hypot(velocity[0], velocity[2]),
                                            mass_kg=body.get("mass_kg"), key=key))
    report = dict(body_key=key, initial_velocity=[0, 0, -2], comparison_simulation_seconds=0.35, surfaces={})
    for name in ("ice", "wood", "tile", "carpet"):
        rows = measured.get(name, [])
        checks.append(dict(name=f"{name}: native samples bracket the same simulation interval", passed=bool(rows)))
        if rows:
            report["surfaces"][name] = min(rows, key=lambda row: abs(row["simulation_seconds"] - 0.35))
    if len(report["surfaces"]) == 4:
        speed = {name: row["horizontal_speed"] for name, row in report["surfaces"].items()}
        checks.append(dict(name="ice preserves more horizontal speed than wood", passed=speed["ice"] > speed["wood"] + 0.2))
        checks.append(dict(name="wood slows less than tile", passed=speed["wood"] > speed["tile"] + 0.02))
        checks.append(dict(name="tile slows less than carpet", passed=speed["tile"] > speed["carpet"] + 0.02))
    seam = dict(initial_velocity=[0, 0, -6], rows=seam_rows, comparison_contact_seconds=0.05, deceleration_m_s2={}, deceleration_windows={})
    checks.append(dict(name="same native box crosses both continuous material seams", passed=bool(seam_rows)
                       and {row["region"] for row in seam_rows} >= {"ice", "wood", "carpet"}
                       and all(row["key"] == key for row in seam_rows)))
    checks.append(dict(name="material seam crossing has no floor gap or tunneling", passed=bool(seam_rows)
                       and min(row["floor_gap"] for row in seam_rows) >= -0.03
                       and all(after["z"] <= before["z"] + 0.01 for before, after in zip(seam_rows, seam_rows[1:]))))
    checks.append(dict(name="material-only seams do not launch the sliding box off its continuous slab", passed=bool(seam_rows)
                       and max(row["floor_gap"] for row in seam_rows) <= 0.005))
    for region in ("ice", "wood", "carpet"):
        rows = [row for row in seam_rows if row["region"] == region and row["linear_velocity"][2] < -0.2]
        if len(rows) >= 4:
            target = rows[0]["simulation_seconds"] + 0.05
            end = min(rows[3:], key=lambda row: abs(row["simulation_seconds"] - target))
            window = [row for row in rows if row["simulation_seconds"] <= end["simulation_seconds"]]
        else:
            window = rows
        duration = window[-1]["simulation_seconds"] - window[0]["simulation_seconds"] if window else 0
        valid = len(window) >= 4 and 0.049 <= duration <= 0.067
        valid = valid and all(abs(row["floor_gap"]) <= 0.003 and row["upright_y"] >= 0.995 and row["angular_speed"] <= 0.25 for row in window)
        checks.append(dict(name=f"{region}: matching early full-shape contact window samples native deceleration", passed=valid,
                           simulation_seconds=duration, samples=len(window)))
        seam["deceleration_windows"][region] = window
        if valid:
            seam["deceleration_m_s2"][region] = (window[0]["horizontal_speed"] - window[-1]["horizontal_speed"]) / duration
    if len(seam["deceleration_m_s2"]) == 3:
        decel = seam["deceleration_m_s2"]
        checks.append(dict(name="crossing ice into wood increases actual deceleration", passed=decel["wood"] > decel["ice"] + 0.25))
        checks.append(dict(name="crossing wood into carpet increases actual deceleration", passed=decel["carpet"] > decel["wood"] + 0.15))
    checks.append(dict(name="the seam trajectory settles without a player obstruction", passed=bool(seam_rows)
                       and (seam_rows[-1]["sleeping"] or seam_rows[-1]["horizontal_speed"] < 0.15)))
    report["seam"] = seam
    return report


def audit_contact_frames(events: list[dict], steps: list[dict], samples: dict, checks: list[dict]) -> dict:
    given_ids = ("arcade:bowling_ball_cobalt", "arcade:bowling_ball_red", "arcade:party_cup_stars", "arcade:party_cup_balloons")
    keys = {id: 2 ** 64 - 1 - index for index, id in enumerate(given_ids)}
    phase = None
    elapsed = 0.0
    phases = iter((("ball-ball", given_ids[0], given_ids[1]), ("ball-cup", given_ids[0], given_ids[2]),
                   ("cup-cup", given_ids[2], given_ids[3])))
    authored = {entry[0]: entry[1] for entry in samples["initial"]["physics"]["authored"]}
    props = {prop["id"]: prop for prop in json.loads(FIXTURE.read_text())["props"]
             if prop.get("physics", {}).get("motion") == "dynamic"}
    ball_shape = dict(size=[0.216, 0.216, 0.216], physics=dict(shape=dict(kind="sphere", radius=0.108)))
    cup_shape = dict(size=[0.085, 0.122, 0.085], physics=dict(shape=dict(kind="cylinder", half_height=0.061, radius=0.0425)))
    shapes = {key: props[id] for key, id in authored.items()}
    shapes.update({keys[id]: ball_shape if index < 2 else cup_shape for index, id in enumerate(given_ids)})
    report: dict = dict(cases={}, frames=0, minimum_floor_gap=math.inf, minimum_ball_ball_surface_gap=math.inf)
    for event in events:
        if event["event"] == "action":
            action = steps[int(json.loads(event["detail"])["id"])]["action"]
            if action["kind"] == "item_pose":
                phase = None
            elif action["kind"] == "item_velocity":
                phase, source, target = next(phases)
                elapsed = 0.0
                report["cases"][phase] = dict(source=source, target=target, source_mass_kg=None, target_mass_kg=None,
                                              maximum_target_x_velocity=0.0, minimum_source_x_velocity=2.0, samples=0)
        elif event["event"] == "physics_frame":
            detail = json.loads(event["detail"])
            report["frames"] += 1
            for body in detail["bodies"]:
                vector(body["linear_velocity"], 3); vector(body["angular_velocity"], 3)
                report["minimum_floor_gap"] = min(report["minimum_floor_gap"], fixture_floor_gap(body, shapes[body["key"]]))
            if not phase:
                continue
            elapsed += physics_frame_seconds(detail["stats"])
            if elapsed > 0.8:
                continue
            bodies = {item["key"]: item for item in detail["bodies"]}
            src, dst = bodies[keys[source]], bodies[keys[target]]
            if phase == "ball-ball":
                def sphere_centre(body: dict) -> list[float]:
                    x, y, z, w = vector(body["pose"]["rotation"], 4)
                    up = [2 * (x * y - z * w), 1 - 2 * (x * x + z * z), 2 * (y * z + x * w)]
                    return [position + 0.108 * axis for position, axis in zip(body_position(body), up)]
                gap = distance(sphere_centre(src), sphere_centre(dst)) - 0.216
                report["minimum_ball_ball_surface_gap"] = min(report["minimum_ball_ball_surface_gap"], gap)
            row = report["cases"][phase]
            row["source_mass_kg"], row["target_mass_kg"] = src.get("mass_kg"), dst.get("mass_kg")
            row["maximum_target_x_velocity"] = max(row["maximum_target_x_velocity"], vector(dst["linear_velocity"], 3)[0])
            row["minimum_source_x_velocity"] = min(row["minimum_source_x_velocity"], vector(src["linear_velocity"], 3)[0])
            row["samples"] += 1
    checks.append(dict(name="contact campaign has continuous finite native body evidence", passed=report["frames"] >= 100))
    checks.append(dict(name="contact bodies stay within the strict floor penetration bound", passed=report["minimum_floor_gap"] >= -COLLISION_TOLERANCE_M,
                       minimum_floor_gap_m=report["minimum_floor_gap"] if math.isfinite(report["minimum_floor_gap"]) else None,
                       tolerance_m=COLLISION_TOLERANCE_M))
    checks.append(dict(name="equal spheres stay separated throughout their collision", passed=report["minimum_ball_ball_surface_gap"] >= -0.015))
    if not math.isfinite(report["minimum_floor_gap"]):
        report["minimum_floor_gap"] = None
    if not math.isfinite(report["minimum_ball_ball_surface_gap"]):
        report["minimum_ball_ball_surface_gap"] = None
    for name, row in report["cases"].items():
        checks.append(dict(name=f"{name}: contact transfers actual horizontal momentum", passed=row["samples"] >= 5
                           and row["maximum_target_x_velocity"] > 0.1))
        if name in ("ball-ball", "cup-cup"):
            checks.append(dict(name=f"{name}: equal-mass source slows at collision", passed=row["minimum_source_x_velocity"] < 1.9))
            checks.append(dict(name=f"{name}: actual solver masses match", passed=row["source_mass_kg"] is not None
                               and abs(row["source_mass_kg"] - row["target_mass_kg"]) < 0.001))
        else:
            checks.append(dict(name="heavy ball knocks a substantially lighter cup", passed=row["source_mass_kg"] is not None
                               and row["target_mass_kg"] is not None and row["source_mass_kg"] > 100 * row["target_mass_kg"]))
    def cup_at(name: str) -> dict:
        return next(body for body in samples[name]["physics"]["bodies"] if body["key"] == keys[given_ids[2]])
    before, after = cup_at("contacts-held"), cup_at("contacts-thrown")
    report["thrown_ball_target_displacement"] = distance(body_position(before), body_position(after))
    checks.append(dict(name="real mouse throw strikes an existing dynamic cup",
                       passed=report["thrown_ball_target_displacement"] > 0.08))
    return report


def audit_escape_frames(events: list[dict], steps: list[dict], samples: dict, checks: list[dict]) -> dict:
    """Collision prevention and deliberate escape recovery have separate proof.

    Every measured collision point comes from native solver transforms before
    any recovery event. Teleporting below/outside the fixture is never counted
    as a collision success, and ordinary trajectories require zero recoveries.
    """
    fixture = json.loads(FIXTURE.read_text())
    props = {prop["id"]: prop for prop in fixture["props"] if prop.get("physics", {}).get("motion") == "dynamic"}
    door = next(door for door in fixture["doors"] if door["id"] == "fixture_door")
    gameplay = json.loads((ROOT / "assets/gameplay.json").read_text())
    catalog = json.loads((ROOT / "assets/catalog.json").read_text())
    pin_asset = next(entry for entry in catalog["assets"] if entry["id"] == ESCAPE_PIN)
    props[ESCAPE_PIN] = dict(size=pin_asset["size"], physics=gameplay["assets"][ESCAPE_PIN]["physics"])
    keys = {entry[1]: entry[0] for entry in samples["initial"]["physics"]["authored"]}
    keys[ESCAPE_PIN] = 2 ** 64 - 1
    case_index, phase = 0, None
    deliberate_key = None
    recoveries, unexpected, rows = [], [], {}
    recovery_motion = {}
    prescribed_poses = {}
    frames = 0
    for event in events:
        if event["event"] == "action":
            action = steps[int(json.loads(event["detail"])["id"])]["action"]
            if action["kind"] == "item_pose":
                phase = None
                prescribed_poses[action["id"]] = action
                position = action["position"]
                if position[1] < -6 or position[0] > 20:
                    deliberate_key = keys[action["id"]]
            elif action["kind"] == "item_velocity":
                phase = escape_cases()[case_index]
                case_index += 1
                if phase["id"] != action["id"] or phase["linear"] != action["linear"]:
                    raise ValueError("escape action phase does not match its bounded diagnostic case")
                setup = prescribed_poses[phase["id"]]
                rows[phase["name"]] = dict(samples=0, prescribed_position=setup["position"], prescribed_rotation=setup["rotation"],
                    prescribed_linear_velocity=phase["linear"], minimum_floor_gap_m=math.inf, minimum_barrier_gap_m=math.inf,
                    barrier_samples=0, maximum_y_velocity=-math.inf, minimum_speed=math.inf, contact_frames=0)
                if phase["barrier"] == "door":
                    low, high = fixture_body_bounds(dict(pose=dict(position=setup["position"], rotation=setup["rotation"])), props[phase["id"]])
                    target_is_leaf = door["x"] + 0.01 <= low[0] and high[0] <= door["x"] + door["width"] - 0.01 and low[1] > 0 and high[1] < door["height"]
                    checks.append(dict(name="cup-thin-door: prescribed physical shape aims inside the actual closed leaf",
                                       passed=target_is_leaf, prescribed_bounds=dict(min=low, max=high),
                                       closed_leaf_x_bounds=[door["x"], door["x"] + door["width"]]))
            elif action["kind"] == "activate":
                phase = None
        elif event["event"] == "physics_recovery":
            recovered = json.loads(event["detail"])
            recovered["deliberately_requested_key"] = deliberate_key
            recoveries.append(recovered)
            recovery_motion[recovered["key"]] = dict(samples=0, first_frame=None, minimum_floor_gap_m=math.inf,
                maximum_linear_speed_m_s=0.0, maximum_angular_speed_rad_s=0.0, simulation_seconds=0.0)
            if deliberate_key is None or recovered["key"] != deliberate_key:
                unexpected.append(recovered)
        elif event["event"] == "physics_frame":
            frame = json.loads(event["detail"])
            frames += 1
            for recovered_key, motion in recovery_motion.items():
                body = next(body for body in frame["bodies"] if body["key"] == recovered_key)
                prop_id = next(id for id, key in keys.items() if key == recovered_key)
                linear = vector(body["linear_velocity"], 3)
                angular = vector(body["angular_velocity"], 3)
                motion["samples"] += 1
                motion["simulation_seconds"] += physics_frame_seconds(frame["stats"])
                motion["minimum_floor_gap_m"] = min(motion["minimum_floor_gap_m"], fixture_floor_gap(body, props[prop_id]))
                motion["maximum_linear_speed_m_s"] = max(motion["maximum_linear_speed_m_s"], distance(linear, [0, 0, 0]))
                motion["maximum_angular_speed_rad_s"] = max(motion["maximum_angular_speed_rad_s"], distance(angular, [0, 0, 0]))
                motion["final_pose"] = body["pose"]
                motion["final_linear_velocity"] = linear
                motion["final_angular_velocity"] = angular
                motion["final_sleeping"] = body["sleeping"]
                if motion["first_frame"] is None:
                    motion["first_frame"] = dict(frame=frame["frame"], pose=body["pose"], render_pose=body["render_pose"],
                        linear_velocity=linear, angular_velocity=angular, dormant=body["dormant"],
                        fixed_step_seconds=physics_step_seconds(frame["stats"]), frame_simulation_seconds=physics_frame_seconds(frame["stats"]))
            if not phase:
                continue
            body = next(body for body in frame["bodies"] if body["key"] == keys[phase["id"]])
            prop = props[phase["id"]]
            low, high = fixture_body_bounds(body, prop)
            row = rows[phase["name"]]
            row["samples"] += 1
            row["minimum_floor_gap_m"] = min(row["minimum_floor_gap_m"], low[1])
            velocity = vector(body["linear_velocity"], 3)
            row["maximum_y_velocity"] = max(row["maximum_y_velocity"], velocity[1])
            row["minimum_speed"] = min(row["minimum_speed"], distance(velocity, [0, 0, 0]))
            if low[1] < row.get("worst_floor_gap_m", math.inf):
                row.update(worst_floor_gap_m=low[1], worst_floor_frame=frame["frame"], worst_floor_pose=body["pose"])
            barrier = phase["barrier"]
            gap = None
            if barrier == "floor":
                gap = low[1]
            elif barrier == "door":
                gap = low[2] - door["z"] - door["thickness"] / 2
            elif barrier == "west":
                gap = low[0] + 3.0
            elif barrier == "corner":
                gap = min(low[0] + 3.0, 3.0 - high[2])
            elif barrier == "ramp" and low[0] >= 1.3 and high[0] <= 2.7 and low[2] >= 1.7 and high[2] <= 2.4:
                slope = 0.6 / 1.4
                gap = (body_support(body, prop, [-slope, 1, 0]) + slope * 1.3) / math.hypot(slope, 1)
            elif barrier == "stair" and low[2] >= 1.7 and high[2] <= 2.4:
                for tread in range(4):
                    start, end = -2.7 + tread * 1.4 / 4, -2.7 + (tread + 1) * 1.4 / 4
                    if low[0] >= start and high[0] <= end:
                        gap = low[1] - 0.6 * (tread + 1) / 4
            if gap is not None:
                row["barrier_samples"] += 1
                row["minimum_barrier_gap_m"] = min(row["minimum_barrier_gap_m"], gap)
                if gap <= 0.03:
                    row["contact_frames"] += 1
    checks.append(dict(name="escape campaign retains continuous native solver evidence", passed=frames >= 100))
    checks.append(dict(name="all seven prescribed high-speed collision cases execute", passed=case_index == len(escape_cases()) and len(rows) == len(escape_cases())))
    checks.append(dict(name="ordinary fast floor/wall/corner/ramp/stair/held trajectories never invoke recovery", passed=not unexpected,
                       unexpected_recoveries=unexpected))
    for name, row in rows.items():
        checks.append(dict(name=f"{name}: every native frame stays above the actual physical floor", passed=row["samples"] >= 5
                           and row["minimum_floor_gap_m"] >= -COLLISION_TOLERANCE_M,
                           minimum_gap_m=row["minimum_floor_gap_m"], tolerance_m=COLLISION_TOLERANCE_M))
        checks.append(dict(name=f"{name}: actual collision reaches and stays on the playable side of its barrier", passed=row["barrier_samples"] >= 3
                           and row["contact_frames"] >= 1 and row["minimum_barrier_gap_m"] >= -COLLISION_TOLERANCE_M,
                           minimum_gap_m=row["minimum_barrier_gap_m"] if math.isfinite(row["minimum_barrier_gap_m"]) else None,
                           contact_frames=row["contact_frames"], tolerance_m=COLLISION_TOLERANCE_M))
        checks.append(dict(name=f"{name}: actual collision changes the prescribed 20 m/s motion", passed=row["minimum_speed"] < 18))
        for field, value in row.items():
            if isinstance(value, float) and not math.isfinite(value):
                row[field] = None
    checks.append(dict(name="only two deliberately invalid placements produce recovery events", passed=len(recoveries) == 2
                       and {row["key"] for row in recoveries} == {keys["pickup_cup"], keys[ESCAPE_PIN]}))
    for row in recoveries:
        escaped = row.get("escaped_pose")
        position = vector(escaped["position"], 3) if escaped is not None else [0, 0, 0]
        expected_reason = "below_playable_space" if row["key"] == keys["pickup_cup"] else "outside_coverage"
        checks.append(dict(name=f"recovery {row['key']}: original invalid pose and reason remain visible", passed=escaped is not None
                           and row.get("reason") == expected_reason and (position[1] < -6 if expected_reason == "below_playable_space" else position[0] > 20)))
        checks.append(dict(name=f"recovery {row['key']}: bounded search finds a supported valid destination", passed=row.get("outcome") == "restored"
                           and row.get("destination") is not None and 1 <= row.get("candidate_tests", 0) <= 30
                           and row.get("outside_seconds", 0) >= 1 and row.get("released_held") is False))
        motion = recovery_motion[row["key"]]
        first, destination = motion["first_frame"], row.get("destination")
        checks.append(dict(name=f"recovery {row['key']}: an adjacent actual solver frame proves the cleared motion", passed=first is not None and destination is not None))
        if first is not None and destination is not None:
            elapsed, step = first["frame_simulation_seconds"], first["fixed_step_seconds"]
            # The fixture uses the shared player's 9.8 m/s² gravity. Clearing
            # occurs inside a fixed-step batch, so remaining fresh steps may
            # add gravity before the first frame is published. The bound is
            # semi-implicit integration of that batch, not a settling clamp.
            gravity = 9.8
            max_fall_speed = gravity * elapsed + 0.0001
            max_displacement = gravity * elapsed * (elapsed + step) * 0.5 + 0.00001
            linear, angular = first["linear_velocity"], first["angular_velocity"]
            position = vector(first["pose"]["position"], 3)
            render_position = vector(first["render_pose"]["position"], 3)
            expected = vector(destination["position"], 3)
            require_zero = row["key"] == keys["pickup_cup"]
            cleared = distance(linear, [0, 0, 0]) < 0.00001 if require_zero else (
                math.hypot(linear[0], linear[2]) < 0.0001 and -max_fall_speed <= linear[1] <= 0.0001)
            checks.append(dict(name=f"recovery {row['key']}: old escape velocity/spin is replaced only by fresh gravity", passed=cleared
                               and distance(angular, [0, 0, 0]) < 0.0001 and distance(position, expected) <= max_displacement
                               and distance(render_position, expected) <= max_displacement,
                               first_velocity=linear, first_angular_velocity=angular, destination_displacement_m=distance(position, expected),
                               render_destination_displacement_m=distance(render_position, expected),
                               fresh_gravity_speed_bound_m_s=max_fall_speed, fresh_gravity_displacement_bound_m=max_displacement))
        checks.append(dict(name=f"recovery {row['key']}: subsequent native motion stays above the supported floor", passed=motion["samples"] >= 5
                           and motion["minimum_floor_gap_m"] >= -COLLISION_TOLERANCE_M,
                           minimum_floor_gap_m=motion["minimum_floor_gap_m"] if math.isfinite(motion["minimum_floor_gap_m"]) else None,
                           tolerance_m=COLLISION_TOLERANCE_M))
        if not math.isfinite(motion["minimum_floor_gap_m"]):
            motion["minimum_floor_gap_m"] = None
    return dict(frames=frames, collision_tolerance_m=COLLISION_TOLERANCE_M, cases=rows, recoveries=recoveries,
                post_recovery_motion=recovery_motion,
                ordinary_recovery_events=unexpected, deliberate_escapes_are_collision_proof=False,
                prescribed_inputs="Labelled native ItemPose/ItemVelocity starts; real Rapier trajectories and SDL pickup/drop afterwards.")


def audit_lighting_samples(mode: str, samples: dict, checks: list[dict]) -> dict:
    instance = "stairs_ball" if mode == "lighting-ball" else "pickup_cup"
    key = next(entry[0] for entry in samples["initial"]["physics"]["authored"] if entry[1] == instance)
    prop = next(prop for prop in json.loads(FIXTURE.read_text())["props"] if prop.get("id") == instance)
    report = {}
    def flatten(value: object) -> list[float]:
        return [number for row in value for number in (flatten(row) if isinstance(row, list) else [float(row)])]
    def matrix_equal(left: object, right: object, size: int = 16) -> bool:
        a, b = flatten(left), flatten(right)
        return len(a) == len(b) == size and all(math.isfinite(x) and math.isfinite(y) and abs(x - y) < 0.0001 for x, y in zip(a, b))
    names = ("lighting-bright", "lighting-dim", "lighting-dropped", "lighting-off", "lighting-on", "lighting-static-on", "lighting-static-off",
             "lighting-full-returned", "lighting-quality-medium", "lighting-quality-high", "lighting-second-drop", "lighting-reheld",
             "lighting-throw-ready", "lighting-launch", "lighting-flight", "lighting-rotation", "lighting-rest")
    for name in names:
        lighting = lighting_snapshots(samples[name]).get(key)
        checks.append(dict(name=f"{name}: native renderer exposes the physical {instance} receiver", passed=lighting is not None))
        if not lighting:
            continue
        live, sample, published = lighting["live"], lighting.get("sample") or {}, lighting.get("published")
        cache = sample.get("cache") or {}
        physical = next(body for body in samples[name]["physics"]["bodies"] if body["key"] == key)
        render_body = dict(physical, pose=physical.get("render_pose", physical["pose"]))
        checks.append(dict(name=f"{name}: solver exposes its actual interpolated render pose", passed=isinstance(physical.get("render_pose"), dict)))
        physical_centre = fixture_body_center(render_body, prop)
        physical_low, physical_high = fixture_body_bounds(render_body, prop)
        static = samples[name]["lighting"] == "low"
        checks.append(dict(name=f"{name}: receiver model origin and pivot follow the actual solver shape",
                           passed=distance(vector(live["position"], 3), body_position(render_body)) <= 0.001
                           and distance(vector(live["centre"], 3), physical_centre) <= 0.001))
        render_low = vector(live["world_bounds"]["min"], 3)
        render_high = vector(live["world_bounds"]["max"], 3)
        checks.append(dict(name=f"{name}: renderer bounds contain the actual rotated receiver primitive",
                           passed=all(actual >= rendered - 0.002 for actual, rendered in zip(physical_low, render_low))
                           and all(actual <= rendered + 0.002 for actual, rendered in zip(physical_high, render_high))))
        checks.append(dict(name=f"{name}: sampled and published transforms match actual live model",
                           passed=published is not None and matrix_equal(live["model"], published["model"])
                           and (not cache if static else cache.get("sampled_model") is not None and matrix_equal(live["model"], cache["sampled_model"]))))
        checks.append(dict(name=f"{name}: actual published normal matrix follows the model",
                           passed=published is not None and matrix_equal(live["normal_model"], published["normal_model"], 12)))
        checks.append(dict(name=f"{name}: native lighting samples the current receiver pivot",
                           passed=sample.get("source") == ("NoField" if static else "Prepared") and sample.get("probe_position") is not None
                           and distance(vector(sample["probe_position"], 3), vector(live["centre"], 3)) < 0.001))
        spatial = sample.get("spatial") or {}
        direct = spatial.get("direct", [])
        strengths = [source["color_strength"][3] for source in direct]
        anchors = spatial.get("anchors", [])
        irradiance = [value for anchor in anchors for value in vector(anchor["irradiance"], 4)[:3]]
        residual_energy = statistics.mean(irradiance) if irradiance else None
        resources = lighting["resources"]
        if static:
            fallback = vector(published["light_scale"], 3) if published else []
            checks.append(dict(name=f"{name}: Static publishes the finite native fallback without prepared payloads",
                               passed=published is not None and bool(fallback) and min(fallback) >= 0
                               and distance(fallback, vector(sample["hdr"], 3)) <= 0.001
                               and sample.get("prepared") is None and sample.get("spatial") is None
                               and published["entity_irradiance"][3] == -1
                               and not resources.get("switchable_indirect_sources")))
        else:
            fallback = None
            checks.append(dict(name=f"{name}: newly baked switchable bounce planes are present",
                               passed=bool(resources.get("switchable_indirect_sources"))))
            checks.append(dict(name=f"{name}: all eight receiver anchors contain finite nonnegative irradiance",
                               passed=len(anchors) == 8 and bool(irradiance) and min(irradiance) >= 0))
        report[name] = dict(position=live["position"], probe_position=sample.get("probe_position"), hdr=sample.get("hdr"),
                            source=sample.get("source"), direct_strengths=strengths, anchors=anchors, residual_energy=residual_energy,
                            physical_key=key, instance=instance, shape=prop["physics"]["shape"], rotation=physical["pose"]["rotation"],
                            physical_centre=physical_centre, physical_bounds=dict(min=physical_low, max=physical_high),
                            solver_pose=physical["pose"], render_pose=physical.get("render_pose"), fallback_published=fallback,
                            direct_sources=direct,
                            quality=samples[name]["quality"], lighting=samples[name]["lighting"],
                            cache=cache, resources=lighting["resources"])
    for off_name, on_name in (("lighting-off", "lighting-on"), ("lighting-static-off", "lighting-static-on")):
        if off_name in report and on_name in report:
            off, on = report[off_name], report[on_name]
            if off_name == "lighting-static-off":
                off_fallback, on_fallback = off["fallback_published"], on["fallback_published"]
                checks.append(dict(name="Static circuit changes actual published fallback lighting",
                                   passed=bool(off_fallback) and bool(on_fallback)
                                   and all(bright > dark + 0.1 for dark, bright in zip(off_fallback, on_fallback)),
                                   off_published=off_fallback, on_published=on_fallback))
                continue
            slots = [index for index, source in enumerate(on["direct_sources"])
                     if abs(source["position_range"][0]) <= 0.001 and abs(source["position_range"][2]) <= 0.001
                     and abs(source["position_range"][3] - 6) <= 0.001]
            source_slot = slots[0] if len(slots) == 1 else None
            off_strength = off["direct_strengths"][source_slot] if source_slot is not None and source_slot < len(off["direct_strengths"]) else None
            on_strength = on["direct_strengths"][source_slot] if source_slot is not None else 0
            checks.append(dict(name=f"{off_name}: circuit disables and restores held direct source",
                               passed=off_strength is not None and off_strength < 0.00001 and on_strength > 0.1,
                               selected_main_source_slot=source_slot, off_strength=off_strength, on_strength=on_strength))
            off_energy, on_energy = off["residual_energy"], on["residual_energy"]
            checks.append(dict(name=f"{off_name}: circuit disables and restores physical receiver bounced light",
                               passed=off_energy is not None and on_energy is not None and on_energy > off_energy + 0.0001,
                               off_mean_irradiance=off_energy, on_mean_irradiance=on_energy))
            moments_off = flatten([anchor["moment"] for anchor in off["anchors"]])
            moments_on = flatten([anchor["moment"] for anchor in on["anchors"]])
            checks.append(dict(name=f"{off_name}: receiver direction moments change with the circuit",
                               passed=len(moments_off) == len(moments_on) == 32
                               and max(abs(a - b) for a, b in zip(moments_off, moments_on)) > 0.00001))
            checks.append(dict(name=f"{off_name}: circuit invalidates the actual lighting cache mask",
                               passed=off["cache"].get("enabled_mask") != on["cache"].get("enabled_mask")))
    if "lighting-bright" in report and "lighting-dim" in report:
        bright, dim = report["lighting-bright"]["residual_energy"], report["lighting-dim"]["residual_energy"]
        checks.append(dict(name="carrying into the dim room reduces actual receiver irradiance",
                           passed=bright is not None and dim is not None and bright > dim * 1.5 + 0.0001,
                           bright_mean_irradiance=bright, dim_mean_irradiance=dim))
    for phase, phase_names in (("carry/switch", names[:8]), ("drop/throw", names[9:])):
        revisions = {report[name]["resources"].get("geometry_revision") for name in phase_names if name in report}
        checks.append(dict(name=f"{phase}: moving, turning and switching the same receiver reuse geometry within its texture-quality phase",
                           passed=bool(revisions) and len(revisions) == 1))
    if "lighting-throw-ready" in report and "lighting-flight" in report:
        checks.append(dict(name="actual thrown receiver resamples after crossing into the dim room",
                           passed=report["lighting-throw-ready"]["physical_centre"][0] < 3
                           and report["lighting-flight"]["physical_centre"][0] > 3.2
                           and distance(report["lighting-throw-ready"]["probe_position"], report["lighting-flight"]["probe_position"]) > 0.5))
    return report


def check_native_video(args: argparse.Namespace, out: Path, events: list[dict], receipt: dict, saved_metadata: dict | None = None) -> None:
    path = out / "bowling-native.mp4"
    recorded = [json.loads(event["detail"]) for event in events if event["event"] == "native_video_frame"]
    sources = [int(frame["source_ready_frame"]) for frame in recorded]
    receipt["checks"].append(dict(name="the MP4 contains consecutive actual native render captures",
                                 passed=len(recorded) == args.video_frames
                                 and [int(frame["captured"]) for frame in recorded] == list(range(1, args.video_frames + 1))
                                 and sources[0] == args.video_start_frame + 1
                                 and all(b - a == 2 for a, b in zip(sources, sources[1:]))))
    if not path.is_file():
        receipt["checks"].append(dict(name="continuous native gameplay MP4 exists", passed=False))
        receipt["video"]["status"] = "missing"
        return
    if saved_metadata is None:
        command = [str(args.ffprobe), "-v", "error", "-select_streams", "v:0", "-show_entries",
                   "stream=codec_name,pix_fmt,width,height,nb_frames,avg_frame_rate,color_space,color_transfer,color_primaries,color_range:format=duration,size",
                   "-of", "json", str(path)]
        probe = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, timeout=15)
        (out / "video-probe.log").write_text(probe.stdout + probe.stderr)
        if probe.returncode:
            raise ValueError("ffprobe could not validate the native gameplay video")
        metadata = json.loads(probe.stdout)
    else:
        metadata = saved_metadata
    streams = metadata.get("streams", [])
    stream = streams[0] if len(streams) == 1 else {}
    receipt["checks"].append(dict(name="native video encodes every frame at the actual 1280x720 drawable and 30fps",
                                 passed=stream.get("codec_name") == "h264" and stream.get("pix_fmt") == "yuv420p"
                                 and (stream.get("width"), stream.get("height")) == (1280, 720)
                                 and int(stream.get("nb_frames", 0)) == args.video_frames
                                 and stream.get("avg_frame_rate") == "30/1"
                                 and abs(float(metadata.get("format", {}).get("duration", 0)) - args.video_frames / 30) < 0.05))
    receipt["checks"].append(dict(name="native video declares the actual encoder's BT.709 conversion",
                                 passed=all(stream.get(key) == "bt709" for key in ("color_space", "color_transfer", "color_primaries"))
                                 and stream.get("color_range") == "tv"))
    receipt["video"].update(status="encoded", path=str(path), sha256=digest(path), bytes=path.stat().st_size,
                            metadata=metadata, first_source_ready_frame=sources[0] if sources else None,
                            last_source_ready_frame=sources[-1] if sources else None,
                            peak_rss_scope="game and ffmpeg children; native-only RSS is measured in separate non-recording runs")


def native_run(args: argparse.Namespace, out: Path) -> None:
    binary = args.binary.resolve(strict=True)
    package = args.package.resolve(strict=True)
    if package.suffix != ".placesmap":
        raise ValueError("--package must be an explicitly compiled .placesmap")
    (out / "levels").mkdir()
    level = SURFACE_LEVEL if args.mode == "surfaces" else BOWLING_LEVEL if args.mode == "bowling" else LEVEL
    fixture = SURFACE_FIXTURE if args.mode == "surfaces" else BOWLING_FIXTURE if args.mode == "bowling" else FIXTURE
    installed = out / "levels" / f"{level}.placesmap"
    shutil.copyfile(package, installed)
    settings = json.loads((ROOT / "tests/fixtures/native/retro-arcade-manifest.json").read_text())["settings"]
    width, height = map(int, args.size.split("x"))
    settings.update(window_width=width, window_height=height,
                    window_mode="fullscreen" if args.fullscreen else "windowed", vsync=False,
                    developer_console=False, quality="high", texture_filtering="high",
                    lightmaps="full", reflections="full", bloom=True, fov_degrees=60,
                    use_low_quality_lighting=False)
    steps = plan_for(args.mode)
    if args.fullscreen:
        steps = reserve_capture_latency(steps)
    write_json(out / "settings.json", settings)
    write_json(out / "actions.json", steps)
    env = {key: value for key, value in os.environ.items()
           if not key.startswith("PLACES_") and key != "SDL_VIDEODRIVER"}
    env.update(PLACES_ASSET_ROOT=str(ROOT), PLACES_STATE_ROOT=str(out), PLACES_LEVEL=level,
               PLACES_BENCH="1", PLACES_VERBOSE="1",
               PLACES_SPAWN="1.25,1.6,1,180" if args.mode == "surfaces" else "10.65,1.6,22.2,0" if args.mode == "bowling" else "0,1.6,0.65,0",
               PLACES_BENCH_ACTIONS=str(out / "actions.json"),
               PLACES_LOAD_TRACE=str(out / "trace.jsonl"), PLACES_STATE_LOG=str(out / "player.csv"),
               PLACES_BENCH_OUT=str(out / "frames.csv"), PLACES_BENCH_FRAMES="3600",
               PLACES_BENCH_WARMUP="120", PLACES_BENCH_FIXED_DELTA_SECONDS="0.016666667",
               PLACES_PHYSICS_FIXED_DELTA_SECONDS=str(args.sim_delta))
    if args.performance:
        env["PLACES_PHYSICS_LOG"] = str(out / "physics.csv")
    if args.mode in ("video", "ball", "source", "throw", "surfaces", "contacts", "lighting", "lighting-ball", "bowling", "dormancy", "escape"):
        env["PLACES_PHYSICS_TRACE"] = "1"
    video_command = None
    if args.record_video:
        env.update(PLACES_BENCH_VIDEO_PIPE=str(out / "native-video.fifo"), PLACES_BENCH_VIDEO_SIZE="1280x720",
                   PLACES_BENCH_VIDEO_FRAMES=str(args.video_frames), PLACES_BENCH_VIDEO_STRIDE="2",
                   PLACES_BENCH_VIDEO_START_FRAME=str(args.video_start_frame))
        video_command = [str(args.ffmpeg), "-hide_banner", "-nostdin", "-n", "-f", "rawvideo", "-pixel_format", "rgba",
                         "-video_size", "1280x720", "-framerate", "30", "-i", str(out / "native-video.fifo"),
                         "-an", "-c:v", "libx264", "-preset", "veryfast", "-crf", "18", "-threads", "2",
                         "-vf", "scale=in_range=pc:out_range=tv:out_color_matrix=bt709,format=yuv420p,setparams=range=limited:color_primaries=bt709:color_trc=bt709:colorspace=bt709",
                         "-colorspace", "bt709", "-color_primaries", "bt709", "-color_trc", "bt709", "-color_range", "tv",
                         "-movflags", "+faststart", str(out / "bowling-native.mp4")]
    receipt = dict(mode=args.mode, status="planned", binary=str(binary), binary_sha256=digest(binary),
                   package=str(package), package_sha256=digest(package), fixture_sha256=digest(fixture),
                   requested_sim_delta_seconds=args.sim_delta, expected_drawable=args.expected_drawable,
                   settings=settings, environment={key: value for key, value in env.items() if key.startswith("PLACES_")},
                   checks=[], screenshots=[], physics_metrics=None, peak_native_rss_bytes=None, peak_child_rss_bytes=None,
                   video=dict(status="planned", encoder_command=video_command, source="consecutive native rendered RGBA frames",
                              requested_frames=args.video_frames, stride=2, start_ready_frame=args.video_start_frame) if args.record_video else None)
    write_json(out / "receipt.json", receipt)
    if args.plan_only:
        print(json.dumps(dict(out=str(out), status="planned", actions=len(steps))))
        return
    started = time.monotonic()
    receipt.update(started_utc=datetime.now(timezone.utc).isoformat(), load_before=os.getloadavg() if hasattr(os, "getloadavg") else None)
    encoder = None
    encoder_log = None
    try:
        if video_command is not None:
            args.ffmpeg.resolve(strict=True); args.ffprobe.resolve(strict=True)
            os.mkfifo(out / "native-video.fifo", mode=0o600)
            encoder_log = (out / "video-encoder.log").open("wb")
            encoder = subprocess.Popen(video_command, cwd=ROOT, stdin=subprocess.DEVNULL, stdout=encoder_log, stderr=subprocess.STDOUT)
        result = subprocess.run([str(binary)], cwd=ROOT, env=env, capture_output=True, text=True, timeout=120)
        peak_rss = resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss if resource else None
        if encoder is None:
            receipt["peak_native_rss_bytes"] = int(peak_rss) * (1 if sys.platform == "darwin" else 1024) if peak_rss is not None else None
        else:
            encoder_exit = encoder.wait(timeout=30)
            peak_rss = resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss if resource else None
            receipt["peak_child_rss_bytes"] = int(peak_rss) * (1 if sys.platform == "darwin" else 1024) if peak_rss is not None else None
            receipt["video"]["encoder_exit_code"] = encoder_exit
        log = result.stdout + result.stderr
        (out / "native.log").write_text(log)
        receipt.update(exit_code=result.returncode, elapsed_seconds=time.monotonic() - started,
                       load_after=os.getloadavg() if hasattr(os, "getloadavg") else None)
        checks = receipt["checks"]
        if encoder is not None:
            checks.append(dict(name="continuous native video encoder exits successfully", passed=receipt["video"]["encoder_exit_code"] == 0))
        checks.append(dict(name="native exit zero", passed=result.returncode == 0))
        checks.append(dict(name="binary and package stay unchanged", passed=(digest(binary), digest(package), digest(installed))
                           == (receipt["binary_sha256"], receipt["package_sha256"], receipt["package_sha256"])))
        checks.append(dict(name="fixture is committed natively", passed=f"[loading] committed {level}" in log))
        checks.append(dict(name="no native validation error", passed=not any(message in log.lower()
                           for message in ("validation error", "panicked", "fatal device", "catalog hash mismatch"))))
        drawables = [tuple(map(int, match)) for match in re.findall(
            r"^\[(?:renderer|window)\].*drawable[: ]+(\d+)x(\d+)", log, re.MULTILINE)]
        receipt["native_drawables"] = drawables
        receipt["native_identity"] = [line for line in log.splitlines()
                                      if line.startswith(("[renderer]", "[window]", "[settings]"))]
        if args.expected_drawable:
            expected = tuple(map(int, args.expected_drawable.split("x")))
            checks.append(dict(name="native drawable matches requested DPI dimensions",
                               passed=bool(drawables) and all(size == expected for size in drawables)))
        events = [json.loads(line) for line in (out / "trace.jsonl").read_text().splitlines()]
        if args.record_video:
            check_native_video(args, out, events, receipt)
        receipt["item_events"] = {name: sum(event["event"] == name for event in events)
                                  for name in ("item_picked_up", "item_dropped", "item_drop_blocked",
                                               "item_thrown", "item_throw_blocked")}
        receipt["recovery_events"] = [json.loads(event["detail"]) for event in events if event["event"] == "physics_recovery"]
        if args.mode != "escape":
            checks.append(dict(name="ordinary native gameplay never relies on prop escape recovery", passed=not receipt["recovery_events"]))
        if args.mode == "dormancy":
            activations = [json.loads(event["detail"]) for event in events if event["event"] == "physics_group_activated"]
            resets = [json.loads(event["detail"]) for event in events if event["event"] == "physics_group_reset"]
            receipt["dormancy_group_events"] = dict(activations=activations, resets=resets)
            checks.append(dict(name="normal authored E dispatch activates only the three still-dormant members", passed=len(activations) == 1
                               and activations[0].get("group") == "native_fixture" and activations[0].get("activated_count") == 3))
            checks.append(dict(name="normal authored E reset restores exactly four members", passed=len(resets) == 1
                               and resets[0].get("group") == "native_fixture" and resets[0].get("reset_count") == BODY_COUNT))
        if args.mode == "blocked-release":
            checks.append(dict(name="native dispatcher reports blocked and successful releases",
                               passed=receipt["item_events"]["item_drop_blocked"] >= 1
                               and receipt["item_events"]["item_dropped"] >= 1))
        if args.mode == "throw":
            checks.append(dict(name="native dispatcher accepts three aimed mouse throws",
                               passed=receipt["item_events"]["item_thrown"] >= 3))
        elif args.mode == "throw-controls":
            checks.append(dict(name="only deliberate armed gameplay click throws",
                               passed=receipt["item_events"]["item_thrown"] == 1))
        presented = sum(event["event"] == "present" and event["detail"] == "ready" for event in events)
        missed = sum(event["event"] == "surface_not_presented" and event["detail"] == "ready" for event in events)
        receipt.update(presented_ready=presented, missed_ready=missed)
        checks.append(dict(name="real unobstructed native presentation", passed=presented > 0 and missed == 0))
        samples, seen = snapshot_samples(events, steps)
        receipt["snapshots"] = samples
        receipt["physics_configuration"] = physics_configuration(samples)
        checks.append(dict(name="all bounded native actions execute", passed=seen == set(range(len(steps)))))
        for step in steps:
            action = step["action"]
            if action["kind"] == "capture":
                path = out / f"{action['name']}.png"
                header = path.read_bytes()[:24] if path.is_file() else b""
                valid = len(header) == 24 and header[:8] == b"\x89PNG\r\n\x1a\n"
                checks.append(dict(name=f"native capture {action['name']}", passed=valid))
                if valid:
                    png_size = [int.from_bytes(header[16:20], "big"), int.from_bytes(header[20:24], "big")]
                    receipt["screenshots"].append(dict(path=str(path), sha256=digest(path),
                                                       after=step["after"], delay_ms=step["delay_ms"], size=png_size))
                    if args.expected_drawable:
                        checks.append(dict(name=f"{action['name']}: capture preserves native drawable",
                                           passed=png_size == list(map(int, args.expected_drawable.split("x")))))
        if args.mode in ("video", "bowling"):
            prefix = "bowling-video" if args.mode == "bowling" else "video"
            receipt["video_frames"] = [capture for capture in receipt["screenshots"]
                                       if re.fullmatch(prefix + r"-\d{3}\.png", Path(capture["path"]).name)]
            checks.append(dict(name="all 20 consecutive raw native video captures exist",
                               passed=len(receipt["video_frames"]) == 20))
        check_samples(args.mode, samples, checks, args.sim_delta)
        if args.mode == "video":
            receipt["physics_frame_audit"] = audit_physics_frames(events, samples, checks)
        elif args.mode == "ball":
            receipt["sphere_frame_audit"] = audit_ball_frames(events, samples, checks)
        elif args.mode == "throw":
            receipt["throw_frame_audit"] = audit_throw_frames(events, samples, checks)
        elif args.mode == "surfaces":
            receipt["surface_frame_audit"] = audit_surface_frames(events, steps, samples, checks)
        elif args.mode == "contacts":
            receipt["contact_frame_audit"] = audit_contact_frames(events, steps, samples, checks)
        elif args.mode == "escape":
            receipt["escape_frame_audit"] = audit_escape_frames(events, steps, samples, checks)
        elif args.mode in ("lighting", "lighting-ball"):
            receipt["lighting_audit"] = audit_lighting_samples(args.mode, samples, checks)
        elif args.mode == "bowling":
            receipt["bowling_frame_audit"] = audit_bowling_frames(events, samples, checks)
        if args.performance:
            receipt["physics_metrics"] = physics_metrics(out / "physics.csv")
        failures = [check["name"] for check in checks if not check["passed"]]
        if failures:
            raise ValueError("native assertions failed: " + "; ".join(failures))
        receipt["status"] = "passed"
    except subprocess.TimeoutExpired as error:
        def decoded(value: str | bytes | None) -> str:
            return value.decode(errors="replace") if isinstance(value, bytes) else value or ""
        (out / "native.log").write_text(decoded(error.stdout) + decoded(error.stderr))
        receipt.update(status="failed", error="native run timed out after 120 seconds")
        raise
    except Exception as error:
        receipt.update(status="failed", error=str(error))
        raise
    finally:
        if encoder is not None and encoder.poll() is None:
            encoder.terminate()
            try:
                encoder.wait(timeout=5)
            except subprocess.TimeoutExpired:
                encoder.kill(); encoder.wait(timeout=5)
        if encoder_log is not None:
            encoder_log.close()
        write_json(out / "receipt.json", receipt)
    print(json.dumps(dict(out=str(out), status=receipt["status"], checks=len(receipt["checks"]),
                          physics_metrics=receipt["physics_metrics"], peak_native_rss_bytes=receipt["peak_native_rss_bytes"],
                          peak_child_rss_bytes=receipt["peak_child_rss_bytes"], video=receipt["video"])))


def reaudit_run(args: argparse.Namespace) -> None:
    """Recheck preserved native artifacts without running the game or encoder."""
    out = args.re_audit.resolve(strict=True)
    source_path = out / "receipt.json"
    source = json.loads(source_path.read_text())
    if source["mode"] != args.mode:
        raise ValueError("re-audit mode must match the preserved native receipt")
    steps = json.loads((out / "actions.json").read_text())
    events = [json.loads(line) for line in (out / "trace.jsonl").read_text().splitlines()]
    samples, seen = snapshot_samples(events, steps)
    base_names = {"native exit zero", "binary and package stay unchanged", "fixture is committed natively",
                  "no native validation error", "native drawable matches requested DPI dimensions",
                  "real unobstructed native presentation", "continuous native video encoder exits successfully"}
    checks = [check for check in source["checks"] if check["name"] in base_names]
    required = base_names - {"native drawable matches requested DPI dimensions", "continuous native video encoder exits successfully"}
    checks.append(dict(name="preserved receipt contains all native execution proofs", passed=required <= {check["name"] for check in checks}))
    checks.append(dict(name="all bounded native actions execute", passed=seen == set(range(len(steps)))))
    captures = {Path(capture["path"]).name: capture for capture in source.get("screenshots", [])}
    for step in steps:
        action = step["action"]
        if action["kind"] == "capture":
            path = out / f"{action['name']}.png"
            capture = captures.get(path.name)
            checks.append(dict(name=f"native capture {action['name']} retains its recorded hash", passed=bool(capture)
                               and path.is_file() and digest(path) == capture["sha256"]))
    report = dict(mode=args.mode, status="auditing", source_receipt=str(source_path), source_receipt_sha256=digest(source_path),
                  original_status=source["status"], original_error=source.get("error"), audit_driver_sha256=digest(Path(__file__)),
                  trace_sha256=digest(out / "trace.jsonl"), actions_sha256=digest(out / "actions.json"),
                  audited_utc=datetime.now(timezone.utc).isoformat(), checks=checks, native_rerun=False)
    report["physics_configuration"] = physics_configuration(samples)
    if source.get("video") and source["video"].get("metadata"):
        args.video_frames = source["video"]["requested_frames"]
        args.video_start_frame = source["video"]["start_ready_frame"]
        report["video"] = dict(source["video"])
        path = out / "bowling-native.mp4"
        checks.append(dict(name="native MP4 retains its recorded hash", passed=path.is_file() and digest(path) == source["video"].get("sha256")))
        check_native_video(args, out, events, report, saved_metadata=source["video"]["metadata"])
    check_samples(args.mode, samples, checks, source.get("requested_sim_delta_seconds", 1 / 60))
    report["recovery_events"] = [json.loads(event["detail"]) for event in events if event["event"] == "physics_recovery"]
    if args.mode != "escape":
        checks.append(dict(name="ordinary native gameplay never relies on prop escape recovery", passed=not report["recovery_events"]))
    if args.mode == "bowling":
        report["bowling_frame_audit"] = audit_bowling_frames(events, samples, checks)
    elif args.mode in ("lighting", "lighting-ball"):
        report["lighting_audit"] = audit_lighting_samples(args.mode, samples, checks)
    elif args.mode == "contacts":
        report["contact_frame_audit"] = audit_contact_frames(events, steps, samples, checks)
    elif args.mode == "escape":
        report["escape_frame_audit"] = audit_escape_frames(events, steps, samples, checks)
    elif args.mode == "surfaces":
        report["surface_frame_audit"] = audit_surface_frames(events, steps, samples, checks)
    elif args.mode == "throw":
        report["throw_frame_audit"] = audit_throw_frames(events, samples, checks)
    elif args.mode == "ball":
        report["sphere_frame_audit"] = audit_ball_frames(events, samples, checks)
    elif args.mode == "video":
        report["physics_frame_audit"] = audit_physics_frames(events, samples, checks)
    if (out / "physics.csv").is_file():
        report["physics_metrics"] = physics_metrics(out / "physics.csv")
    failures = [check for check in checks if not check["passed"]]
    report.update(status="failed" if failures else "passed", failures=failures)
    write_json(out / "reaudit.json", report)
    print(json.dumps(dict(out=str(out / "reaudit.json"), status=report["status"], checks=len(checks), failures=failures)))
    if failures:
        raise SystemExit(1)


def compare_run(args: argparse.Namespace, out: Path) -> None:
    candidate = args.binary.resolve(strict=True)
    baseline = args.baseline.resolve(strict=True)
    receipt = dict(mode="compare", status="planned", binary=str(candidate), binary_sha256=digest(candidate),
                   baseline=str(baseline), baseline_sha256=digest(baseline),
                   view="retro_arcade_demo bathroom door; spawn -1.6,1.6,2.8,90; camera 90,0",
                   frames=args.frames, runs=[])
    if args.comparison_asset_root:
        receipt["comparison_asset_root"] = str(args.comparison_asset_root.resolve(strict=True))
    write_json(out / "receipt.json", receipt)
    if args.plan_only:
        print(json.dumps(dict(out=str(out), status="planned", pairs=args.pairs)))
        return
    try:
        for pair in range(args.pairs):
            # Alternate order across pairs so thermal/load drift does not
            # consistently favor the same binary. Never overlap native jobs.
            order = (("baseline", baseline), ("candidate", candidate))
            if pair % 2:
                order = tuple(reversed(order))
            for label, binary in order:
                run_out = out / f"pair-{pair + 1}-{label}"
                command = [sys.executable, str(ROOT / "tools/bench/frame_time.py"),
                           "--binary", str(binary), "--out", str(run_out),
                           "--frames", str(args.frames), "--warmup", "120", "--size", args.size]
                if args.expected_drawable:
                    command.extend(("--expected-drawable", args.expected_drawable))
                if args.fullscreen:
                    command.append("--fullscreen")
                if args.comparison_asset_root:
                    command.extend(("--asset-root", str(args.comparison_asset_root.resolve(strict=True))))
                result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, timeout=300)
                (out / f"pair-{pair + 1}-{label}.log").write_text(result.stdout + result.stderr)
                if result.returncode:
                    raise ValueError(f"frame_time.py {label} failed; see {out}")
                report = json.loads((run_out / "receipt.json").read_text())
                receipt["runs"].append(dict(pair=pair + 1, label=label, receipt=str(run_out / "receipt.json"),
                                            binary_sha256=report["binary_sha256"], metrics=report["metrics"],
                                            peak_native_rss_bytes=report.get("peak_native_rss_bytes"),
                                            gpu_elapsed_ms=report["gpu_elapsed_ms"]))
        ratios = {}
        for metric in ("loop_ms", "frame_ms", "update_ms", "render_ms"):
            ratios[metric] = {}
            for statistic in ("median", "p95"):
                values = {label: statistics.median(run["metrics"][metric][statistic]
                          for run in receipt["runs"] if run["label"] == label)
                          for label in ("baseline", "candidate")}
                ratios[metric][statistic] = dict(**values, candidate_over_baseline=(
                    values["candidate"] / values["baseline"] if values["baseline"] else None))
        receipt.update(status="measured", comparisons=ratios)
    except Exception as error:
        receipt.update(status="failed", error=str(error))
        raise
    finally:
        write_json(out / "receipt.json", receipt)
    print(json.dumps(dict(out=str(out), status=receipt["status"], comparisons=ratios)))


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--mode", choices=("dormancy", "bowling", "escape", "targeting", "lighting", "lighting-ball", "surfaces", "contacts", "source", "throw", "throw-controls", "pickup", "ball", "wall", "corner", "blocked-release", "transitions", "stress", "timesteps", "video", "compare"), required=True)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/release/places")
    parser.add_argument("--package", type=Path, help="existing explicitly baked tiny fixture package")
    parser.add_argument("--out", type=Path, help="new run directory; defaults under local/item-physics-0.10.0")
    parser.add_argument("--performance", action="store_true", help="require per-frame native physics CPU CSV")
    parser.add_argument("--record-video", action="store_true", help="bowling only: encode consecutive native rendered RGBA frames through a private FIFO")
    parser.add_argument("--video-frames", type=int, default=600, help="native video frames at 30fps (1..900)")
    parser.add_argument("--video-start-frame", type=int, default=90, help="first eligible native ready frame (0..3600)")
    parser.add_argument("--ffmpeg", type=Path, default=Path("/opt/homebrew/bin/ffmpeg"))
    parser.add_argument("--ffprobe", type=Path, default=Path("/opt/homebrew/bin/ffprobe"))
    parser.add_argument("--sim-delta", type=float, default=1 / 60, help="exact native physics seconds per frame (finite >0..1)")
    parser.add_argument("--size", choices=("640x360", "1280x720"), default="1280x720", help="logical native window dimensions")
    parser.add_argument("--fullscreen", action="store_true", help="present on the native display; measure its actual drawable explicitly")
    parser.add_argument("--expected-drawable", help="reject a native run unless its actual drawable matches WIDTHxHEIGHT")
    parser.add_argument("--baseline", type=Path, default=BASELINE, help="preserved developer-console binary for compare")
    parser.add_argument("--comparison-asset-root", type=Path, help="same preserved asset/package bundle for every comparison pair")
    parser.add_argument("--pairs", type=int, default=2, help="sequential alternating native comparison pairs (1..3)")
    parser.add_argument("--frames", type=int, default=600, help="frames per comparison run (120..1800)")
    parser.add_argument("--plan-only", action="store_true", help="write inputs and hashes without launching the app")
    parser.add_argument("--re-audit", type=Path, help="recheck a preserved native run into reaudit.json without launching the game or encoder")
    args = parser.parse_args()
    if args.re_audit is not None:
        if args.mode == "compare" or args.out is not None or args.plan_only or args.record_video:
            parser.error("re-audit requires an acceptance mode and existing run; omit out, plan-only and record-video")
        reaudit_run(args)
        return
    if args.mode != "compare" and args.package is None:
        parser.error("acceptance modes require --package; compile the fixture explicitly first")
    if not 1 <= args.pairs <= 3 or not 120 <= args.frames <= 1800:
        parser.error("pairs must be 1..3 and frames must be 120..1800")
    if not math.isfinite(args.sim_delta) or not 0 < args.sim_delta <= 1:
        parser.error("sim-delta must be finite, positive and no greater than 1 second")
    if not 1 <= args.video_frames <= 900 or not 0 <= args.video_start_frame <= 3600:
        parser.error("video frames must be 1..900 and start frame 0..3600")
    if args.record_video and (args.fullscreen or args.mode != "bowling" or args.size != "640x360" or args.expected_drawable != "1280x720"
                              or args.sim_delta != 1 / 60):
        parser.error("record-video requires bowling, --size 640x360, --expected-drawable 1280x720 and --sim-delta 1/60")
    if args.mode == "compare" and args.sim_delta != 1 / 60:
        parser.error("sim-delta applies to acceptance modes; compare uses frame_time.py's matched simulation step")
    if args.expected_drawable:
        expected = re.fullmatch(r"(\d+)x(\d+)", args.expected_drawable)
        if expected is None or not all(64 <= int(edge) <= 16384 for edge in expected.groups()):
            parser.error("expected drawable must be WIDTHxHEIGHT, with dimensions 64..16384")
    timestamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%S.%fZ")
    out = (args.out or RUN_ROOT / f"{timestamp}-{args.mode}").resolve()
    out.mkdir(parents=True, exist_ok=False)
    if args.mode == "compare":
        compare_run(args, out)
    else:
        native_run(args, out)


if __name__ == "__main__":
    main()
