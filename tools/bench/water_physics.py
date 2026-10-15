#!/usr/bin/env python3
"""Explicit, serial native Pool water acceptance. Never builds or bakes a map.

Examples (compile the desired Pool package separately first):
  python3 tools/bench/water_physics.py --mode baseline --binary /preserved/places --package /preserved/pool_showcase.placesmap --out local/water-before
  python3 tools/bench/water_physics.py --mode views --package /fresh/pool_showcase.placesmap
  python3 tools/bench/water_physics.py --mode acceptance --package /fresh/pool_showcase.placesmap
  python3 tools/bench/water_physics.py --mode acceptance --case ownership --package /fresh/pool_showcase.placesmap
  python3 tools/bench/water_physics.py --mode stress --package /fresh/pool_showcase.placesmap
  python3 tools/bench/water_physics.py --mode video --package /fresh/pool_showcase.placesmap
  python3 tools/bench/water_physics.py --mode performance --package /fresh/pool_showcase.placesmap --baseline-binary /preserved/places
  python3 tools/bench/water_physics.py --mode performance --activate-demo --package /fresh/pool_showcase.placesmap

Every case owns fresh settings/state, an immutable copied package, bounded SDL
actions, native captures/logs, hashes, and a compact receipt. Ownership uses E
and left mouse through the production event handler. Player/item poses, item
aiming, velocities and direct actor activation are diagnostic setup seams and
are labelled in receipts; their resulting motion comes from the native solver.
Video streams actual consecutive rendered RGBA frames to ffmpeg through a FIFO,
with no image interpolation or PNG-to-video synthesis. Its source is 1280x720,
every second 60 Hz native ready frame, encoded at 30 fps with BT.709 conversion.
Performance runs use fixed matched views, no PNG/video or per-frame body trace,
and report CPU, asynchronous unique GPU samples, physics cost and native RSS.
This developer campaign is deliberately outside cargo test and automatic CI.
"""
from __future__ import annotations

import argparse
import csv
from datetime import datetime, timezone
import json
import math
import os
from pathlib import Path
import re
import resource
import shutil
import subprocess
import sys
import time

import item_physics as native

ROOT = native.ROOT
SOURCE = ROOT / "levels/pool_showcase.json"
LEVEL = "pool_showcase"
DUCK = "pool_physics_duck"
SINK = "pool_physics_sink"
CONTROL = "pool_water_demo_control"
GROUP = "pool_water_demo"
SURFACE = -0.15
BOTTOM = -1.5
CASES = ("dormancy", "settling", "ownership", "quality", "player", "contacts", "caustics")
VIEWS = {"main": [8, 1.6, 10.2, 0, -20], "west": [3, 1.6, 5.4, 90, -27],
         "hero": [3, 1.6, 9, 56, -12]}
MAX_TRACE_BYTES = 64 * 1024 * 1024


def pose(p: native.Plan, ms: int, xyz: list[float], yaw: float = 0, pitch: float = 0) -> None:
    p.action(ms, "player", position=xyz, yaw=yaw, pitch=pitch)


def control_view(p: native.Plan, ms: int, source: dict) -> None:
    prop = next(prop for prop in source["props"] if prop.get("id") == CONTROL)
    x, z = prop["x"], prop["z"]
    target_y = prop.get("y", 0) + prop["size"][1] * 0.5
    pose(p, ms, [x, 1.6, z + 1.3], 0, native.pitch_to(1.6, target_y, 1.3))


def setup_body(p: native.Plan, ms: int, id: str, xyz: list[float]) -> None:
    p.item_pose(ms, xyz, [0, 0, 0, 1], id=id)


def plan_for(case: str, source: dict) -> list[dict]:
    p = native.Plan(LEVEL)
    if case == "caustics-video":
        p.player(0, 3.8, 5.4, 90, -42)
        p.capture(300, "initial")
        p.capture(800, "caustics-video-start")
        p.capture(8500, "caustics-video-end")
        return p.finish(10_000)
    if case == "caustics":
        p.player(0, 3, 5.4, 90, -27)
        p.capture(300, "initial")
        # The actual Pool basin, shallow shelf and circular tub. Native input
        # and settings own all transitions; poses only select inspection views.
        p.capture(900, "caustics-full-a")
        p.capture(3300, "caustics-full-b")
        p.player(3500, 12.6, 8.9, 315, -48)
        p.capture(4100, "caustics-shelf-edge")
        p.player(4400, 7.5, 10.5, 0, -65)
        p.capture(5000, "caustics-circle")
        p.player(5200, 3, 5.4, 90, -27)
        p.tap(5350, "F")
        p.capture(6100, "caustics-flashlight-above")
        p.tap(6350, "F")
        p.action(6600, "activate", id=CONTROL)
        p.capture(10_500, "caustics-moving-props")
        pose(p, 11_000, [10, .1, 6], 0, -35)
        p.tap(11_250, "C")
        p.tap(11_550, "F")
        p.capture(13_000, "caustics-flashlight-submerged")
        p.tap(13_350, "F")
        p.tap(13_650, "C")
        p.player(14_000, 3, 5.4, 90, -27)
        p.action(14_500, "lightmaps", quality="static")
        p.capture(18_000, "caustics-static")
        p.action(18_500, "quality", level="low")
        p.capture(21_500, "caustics-static-low-textures")
        p.action(22_000, "lightmaps", quality="full")
        p.capture(25_000, "caustics-full-low-textures")
        p.action(25_500, "quality", level="high")
        p.capture(28_500, "caustics-restored")
        return p.finish(29_000)
    if case in VIEWS:
        p.capture(900, f"view-{case}")
        return p.finish(1100)
    if case == "performance":
        return []  # Native frame budget quits this matched timing run.
    p.player(100, 3, 5.4, 90, -27)
    p.capture(300, "initial")
    if case == "dormancy":
        p.capture(1800, "dormant-wait")
        control_view(p, 2000, source)
        p.capture(2200, "activate-target")
        p.tap(2350, "E")
        p.capture(2850, "activated")
        p.capture(8000, "running")
        control_view(p, 8200, source)
        p.tap(8450, "E")
        p.capture(8850, "reset")
        p.capture(10_200, "reset-wait")
        return p.finish(10_400)
    if case in ("settling", "video"):
        p.action(500, "activate", id=CONTROL)
        # Sink coverage deliberately releases the basket over the basin;
        # its authored initial/reset pose remains grounded on the dry deck.
        setup_body(p, 550, SINK, [11.5, .06, 6.7])
        p.capture(700, "falling")
        p.capture(1600, "water-entry")
        p.capture(6500, "settled")
        p.capture(10_500, "settled-wait")
        if case == "video":
            pose(p, 6900, [14, 1.6, 5.8], 270)
            p.key(7000, "W")
            p.key(8100, "W", False)
            p.capture(8500, "video-standing-entry")
            pose(p, 8700, [12.2, .1, 6.6], 180)
            p.key(8800, "W")
            p.key(10_600, "W", False)
            p.capture(11_100, "video-standing-exit")
            pose(p, 11_300, [10.6, .1, 7.25])
            p.aim_item(11_550, DUCK)
            p.tap(11_700, "E")
            p.capture(12_100, "video-held")
            pose(p, 12_200, [10.6, 0.1, 7.25], 270, 20)
            p.tap_mouse(12_700)
            p.capture(13_000, "video-thrown")
            p.player(13_500, 3, 5.4, 90, -27)
            p.capture(50_800, "video-final")
            # Leave enough wall time for all requested ready-frame captures
            # on slower GPUs; video cadence is native simulation cadence.
            return p.finish(59_000)
        return p.finish(10_800)
    if case == "ownership":
        # Four controlled placements establish dry/wet pickup conditions.
        # E and mouse own every acquisition and release.
        for index, (label, xyz, player, yaw, throwing) in enumerate((
            ("dry-drop", [14.7, 0.1, 5.5], [14.7, 1.6, 6.7], 0, False),
            ("dry-throw", [3, 0.1, 5.4], [3, 1.6, 6.6], 0, True),
            ("wet-drop", [10.6, -0.3, 6.0], [10.6, 0.1, 7.1], 0, False),
            ("wet-throw", [11.8, -0.35, 5.0], [11.8, 0.1, 6.1], 0, True),
        )):
            ms = 600 + index * 4500
            setup_body(p, ms, DUCK, xyz)
            pose(p, ms + 100, player, yaw)
            p.aim_item(ms + 350, DUCK)
            p.capture(ms + 450, f"{label}-target")
            p.tap(ms + 550, "E")
            p.capture(ms + 950, f"{label}-held")
            if throwing:
                # From dry west deck throw into the basin; from wet basin
                # throw east onto the dry deck. These are ordinary throws.
                pose(p, ms + 1100, player, 90, 20 if label.startswith("wet") else 0)
                p.capture(ms + 1350, f"{label}-ready")
                p.tap_mouse(ms + 1450)
            else:
                p.tap(ms + 1450, "E")
            p.capture(ms + 1700, f"{label}-released")
            p.capture(ms + 4050, f"{label}-final")
        return p.finish(19_000)
    if case == "quality":
        p.action(500, "activate", id=CONTROL)
        setup_body(p, 550, SINK, [11.5, .06, 6.7])
        p.capture(5000, "quality-high")
        p.action(5300, "quality", level="medium")
        p.capture(8000, "quality-medium")
        p.action(8300, "quality", level="low")
        p.capture(11_000, "quality-low")
        p.action(11_300, "low_lighting", enabled=True)
        p.capture(14_000, "low-fallback-lighting")
        p.action(14_300, "low_lighting", enabled=False)
        p.capture(17_000, "low-full-lighting")
        p.action(17_300, "lightmaps", quality="static")
        p.capture(20_000, "low-static-lighting")
        p.action(20_300, "lightmaps", quality="full")
        p.action(20_400, "quality", level="high")
        p.capture(23_800, "quality-restored")
        return p.finish(24_000)
    if case == "player":
        # Start on clear east deck, then walk into the real basin. The
        # standing body wades in this 1.35 m basin; crouching is explicit.
        pose(p, 500, [14, 1.6, 5.8], 270)
        p.capture(800, "player-dry")
        p.key(950, "W")
        p.key(2050, "W", False)
        p.capture(2700, "player-entry")
        p.capture(4200, "player-standing-depth")
        p.tap(4500, "C")
        p.capture(6500, "player-crouched-depth")
        p.key(6800, "Space")
        p.capture(8300, "player-surface")
        p.key(8400, "Space", False)
        p.tap(8650, "C")
        p.capture(10_300, "player-standing-again")
        # Diagnostic orientation only; ordinary W crosses the authored
        # basin/shelf/deck route and the real controller resolves its height.
        pose(p, 10_500, [12.2, 0.1, 6.6], 180)
        p.key(10_700, "W")
        p.key(11_170, "W", False)
        p.capture(11_700, "player-shelf")
        p.key(12_000, "W")
        p.key(12_500, "W", False)
        p.capture(13_200, "player-exit")
        p.capture(14_700, "player-exit-wait")
        return p.finish(15_000)
    if case in ("contacts", "stress"):
        p.action(500, "activate", id=CONTROL)
        # Fully submerged diagnostic starts, separated before the impact.
        # The dense item strikes the duck, then both encounter the real rim.
        setup_body(p, 700, DUCK, [12.05, -0.7, 6.3])
        setup_body(p, 700, SINK, [11.4, -0.7, 6.3])
        p.item_velocity(700, SINK, [8, 0, 0])
        p.capture(800, "contact-start")
        p.capture(1400, "contact-impact")
        p.capture(3500, "contact-wall")
        p.capture(9000, "contact-settled")
        if case == "stress":
            # Repeat a bounded near-wall impact at the action velocity cap.
            setup_body(p, 9500, DUCK, [12.4, -0.65, 6.3])
            setup_body(p, 9500, SINK, [11.65, -0.65, 6.3])
            p.item_velocity(9500, SINK, [20, 0, 0])
            p.capture(9650, "stress-start")
            p.capture(10_300, "stress-wall")
            p.action(10_700, "console_enabled", enabled=True)
            p.tap(10_800, "`")
            for index in range(16):
                ms = 11_000 + index * 180
                # The normal give path supplies these sixteen active items;
                # diagnostic player poses distribute their real spawn points.
                pose(p, ms, [5 + (index % 4) * 1.5, .1, 4.86 + (index // 4) * .6])
                p.command(ms + 70, f"/give {native.CUP_MODEL}")
            p.capture(14_200, "stress-eighteen-console")
            p.action(14_300, "escape")
            p.player(14_400, 3, 5.4, 90, -27)
            p.capture(15_500, "stress-eighteen-active")
            p.capture(23_500, "stress-settled")
            return p.finish(23_800)
        return p.finish(9300)
    raise ValueError(f"unknown native water case: {case}")


def bodies(sample: dict) -> dict[str, dict]:
    authored = {entry[0]: entry[1] for entry in sample["physics"]["authored"]}
    return {authored[body["key"]]: body for body in sample["physics"]["bodies"] if body["key"] in authored}


def check(checks: list[dict], name: str, passed: bool, **evidence: object) -> None:
    checks.append(dict(name=name, passed=bool(passed), **evidence))


def speed(body: dict) -> float:
    return native.distance(native.vector(body["linear_velocity"], 3), [0, 0, 0])


def settled(body: dict) -> bool:
    return bool(body["sleeping"]) or (speed(body) < 0.15 and native.distance(
        native.vector(body["angular_velocity"], 3), [0, 0, 0]) < 0.5)


def audit_samples(case: str, samples: dict, events: list[dict], checks: list[dict]) -> None:
    if case in VIEWS or case == "performance":
        return
    initial = bodies(samples["initial"])
    check(checks, "both authored rigid water examples exist", {DUCK, SINK}.issubset(initial))
    if not {DUCK, SINK}.issubset(initial):
        return
    check(checks, "authored water examples start dormant", all(initial[id]["dormant"] for id in (DUCK, SINK)))
    check(checks, "dense basket starts grounded and dry on the east deck",
          native.distance(native.body_position(initial[SINK]), [14, 0, 6.7]) < .002
          and initial[SINK].get("submerged_fraction", 0) == 0)
    if case == "caustics":
        for name, mode, texture in (("caustics-static", "off", "high"),
                                    ("caustics-static-low-textures", "off", "low"),
                                    ("caustics-full-low-textures", "full", "low"),
                                    ("caustics-restored", "full", "high")):
            sample = samples[name]
            check(checks, f"{name}: independent settings actually commit",
                  sample["renderer_lightmaps"] == mode and sample["renderer_quality"] == texture)
        check(checks, "above-water flashlight is actually enabled", samples["caustics-flashlight-above"]["flashlight_on"])
        wet = samples["caustics-flashlight-submerged"]
        check(checks, "submerged flashlight uses a real crouched underwater player",
              wet["flashlight_on"] and wet["player_body"]["underwater"] and wet["player_body"]["crouched"])
    if case == "dormancy":
        check(checks, "untouched dormant startup spends no active solver substeps", all(
            samples[name]["physics"]["stats"]["active_bodies"] == 0
            and samples[name]["physics"]["stats"]["substeps"] == 0 for name in ("initial", "dormant-wait")))
        for id in (DUCK, SINK):
            check(checks, f"{id}: untouched startup retains authored pose", all(
                bodies(samples[name])[id]["dormant"] and bodies(samples[name])[id]["pose"] == initial[id]["pose"]
                for name in ("initial", "dormant-wait")))
            check(checks, f"{id}: reset restores dormant authored pose", all(
                bodies(samples[name])[id]["dormant"] and bodies(samples[name])[id]["pose"] == initial[id]["pose"]
                and speed(bodies(samples[name])[id]) < 1e-5 for name in ("reset", "reset-wait")))
        for name in ("activate-target",):
            target = samples[name]["interaction_target"] or {}
            check(checks, "E aims at the real authored water demo control", target.get("id") == CONTROL, target=target)
        for event, count_key in (("physics_group_activated", "activated_count"), ("physics_group_reset", "reset_count")):
            records = [json.loads(entry["detail"]) for entry in events if entry["event"] == event]
            check(checks, f"ordinary E produces exactly one {event}", len(records) == 1
                  and records[0].get("group") == GROUP and records[0].get(count_key) == 2, events=records)
    elif case in ("settling", "video", "quality"):
        if case == "video":
            entry = samples["video-standing-entry"]["player_body"]
            exit_body = samples["video-standing-exit"]["player_body"]
            check(checks, "video shows normal upright player water entry and exit",
                  abs(entry["feet_y"] - BOTTOM) < .03 and abs(exit_body["feet_y"]) < .03
                  and all(abs(body["body_height"] - 1.8) < .02 and not body["crouched"]
                          for body in (entry, exit_body)))
        names = ("settled", "settled-wait") if case != "quality" else (
            "quality-high", "quality-medium", "quality-low", "low-fallback-lighting", "low-full-lighting", "low-static-lighting", "quality-restored")
        for name in names:
            values = bodies(samples[name])
            duck, sink = values[DUCK], values[SINK]
            fraction = duck.get("submerged_fraction")
            volume = duck.get("displaced_volume_m3")
            check(checks, f"{name}: duck floats partially submerged", isinstance(fraction, (int, float))
                  and 0.02 < fraction < 0.98 and isinstance(volume, (int, float)) and volume > 0,
                  submerged_fraction=fraction, displaced_volume_m3=volume)
            check(checks, f"{name}: dense item sinks below the duck", native.body_position(sink)[1]
                  < native.body_position(duck)[1] - 0.4 and sink.get("submerged_fraction", 0) > 0.98)
            check(checks, f"{name}: both bodies settle", settled(duck) and settled(sink),
                  linear_speed_mps={DUCK: speed(duck), SINK: speed(sink)})
        if case == "quality":
            expected = {"quality-high": "high", "quality-medium": "medium", "quality-low": "low", "quality-restored": "high"}
            for name, quality in expected.items():
                check(checks, f"{name}: real quality switch applied", samples[name]["quality"].lower() == quality
                      and samples[name]["renderer_quality"].lower() == quality)
            check(checks, "independent lighting overrides preserve Low texture quality", all(
                samples[name]["quality"].lower() == "low" for name in ("low-fallback-lighting", "low-full-lighting", "low-static-lighting")))
            for name in ("quality-high", "quality-medium", "quality-low", "low-full-lighting", "quality-restored"):
                receivers = samples[name]["physics"]["lighting"]
                check(checks, f"{name}: floating and submerged receivers use prepared light and shadow probes",
                      len(receivers) == 2 and all(
                          receiver["sample"]["source"] == "Prepared"
                          and receiver["sample"]["prepared"] is not None
                          and receiver["sample"]["spatial"] is not None for receiver in receivers))
            check(checks, "independent low-lighting switch enables the fallback", samples["low-fallback-lighting"]["use_low_quality_lighting"]
                  and samples["low-fallback-lighting"]["renderer_lighting"].lower() == "low")
            check(checks, "independent low-lighting switch disables the fallback", not samples["low-full-lighting"]["use_low_quality_lighting"])
            check(checks, "independent Full lighting applied", samples["low-full-lighting"]["renderer_lighting"].lower() == "high")
            check(checks, "independent Static lighting applied", samples["low-static-lighting"]["renderer_lighting"].lower() == "low"
                  and samples["low-static-lighting"]["renderer_lightmaps"].lower() == "off")
            for id in (DUCK, SINK):
                positions = [native.body_position(bodies(samples[name])[id]) for name in names]
                check(checks, f"{id}: graphics changes preserve settled physical state", max(
                    native.distance(positions[0], position) for position in positions) < 0.12)
        elif case == "video":
            check(checks, "video demonstrates ordinary pickup and mouse throw", bodies(samples["video-held"])[DUCK]["held"]
                  and not bodies(samples["video-thrown"])[DUCK]["held"]
                  and any(event["event"] == "item_thrown" for event in events))
    elif case == "ownership":
        for label in ("dry-drop", "dry-throw", "wet-drop", "wet-throw"):
            target = samples[f"{label}-target"]["interaction_target"] or {}
            body = bodies(samples[f"{label}-held"])[DUCK]
            check(checks, f"{label}: real item ray target then E acquisition", target.get("kind") == "item"
                  and target.get("key") == body["key"] and body["held"])
            check(checks, f"{label}: ordinary input releases ownership", not bodies(samples[f"{label}-released"])[DUCK]["held"])
            fraction = bodies(samples[f"{label}-target"])[DUCK].get("submerged_fraction")
            check(checks, f"{label}: pickup condition is {'wet' if label.startswith('wet') else 'dry'}",
                  isinstance(fraction, (int, float)) and (fraction > 0 if label.startswith("wet") else fraction == 0),
                  submerged_fraction=fraction)
        counts = {event: sum(entry["event"] == event for entry in events)
                  for event in ("item_picked_up", "item_dropped", "item_thrown")}
        check(checks, "normal input makes four pickups, two drops and two throws", counts == dict(
            item_picked_up=4, item_dropped=2, item_thrown=2), events=counts)
        for label, wet in (("dry-throw", True), ("wet-throw", False)):
            fraction = bodies(samples[f"{label}-final"])[DUCK].get("submerged_fraction")
            check(checks, f"{label}: throw crosses the water boundary", isinstance(fraction, (int, float))
                  and (fraction > 0 if wet else fraction == 0), submerged_fraction=fraction)
    elif case == "player":
        for name in ("player-dry", "player-entry", "player-standing-depth", "player-standing-again", "player-shelf", "player-exit", "player-exit-wait"):
            body = samples[name].get("player_body", {})
            check(checks, f"{name}: upright standing body remains explicit", not body.get("crouched", True)
                  and abs(body.get("body_height", 0) - 1.8) < 0.02 and abs(body.get("eye_height", 0) - 1.6) < 0.02,
                  player_body=body, position=samples[name]["player_position"])
        depth = samples["player-standing-depth"].get("player_body", {})
        check(checks, "ordinary entry reaches the physical basin floor", abs(depth.get("feet_y", 100) - BOTTOM) < 0.03
              and depth.get("grounded", False) and not depth.get("swimming", True))
        crouch = samples["player-crouched-depth"].get("player_body", {})
        check(checks, "explicit crouch reaches underwater with a crouched body", crouch.get("crouched", False)
              and crouch.get("underwater", False) and abs(crouch.get("body_height", 0) - 0.9) < 0.02)
        shelf = samples["player-shelf"].get("player_body", {})
        check(checks, "normal W climbs onto the real shallow shelf", abs(shelf.get("feet_y", 100) + 0.35) < 0.04
              and shelf.get("grounded", False))
        check(checks, "normal W exits to stable dry deck", all(abs(samples[name].get("player_body", {}).get("feet_y", 100)) < 0.03
              and not samples[name].get("player_body", {}).get("swimming", True)
              for name in ("player-exit", "player-exit-wait")))


def audit_frames(events: list[dict], samples: dict, source: dict, checks: list[dict], contacts: bool) -> dict:
    frames = [frame for event in events if event["event"] == "physics_frame"
              for frame in [json.loads(event["detail"])] if frame["bodies"]]
    check(checks, "bounded native per-frame physics evidence exists", 20 <= len(frames) <= 10_000, frames=len(frames))
    if not frames:
        return dict(frames=0)
    check(checks, "all solver work stays within its recorded catch-up budget", all(
        0 <= frame["stats"]["substeps"] <= native.physics_step_cap(frame["stats"])
        and frame["stats"].get("recovered_bodies", 0) == 0 for frame in frames))
    authored = {entry[0]: entry[1] for entry in samples["initial"]["physics"]["authored"]}
    props = {prop["id"]: prop for prop in source["props"] if prop.get("id") in (DUCK, SINK)}
    report = dict(frames=len(frames), body_contacts=0, bodies={})
    for key, id in authored.items():
        if id not in props:
            continue
        dynamic = [body for frame in frames for body in frame["bodies"] if body["key"] == key and not body["held"]]
        positions = [native.body_position(body) for body in dynamic]
        fractions = [body.get("submerged_fraction") for body in dynamic]
        valid_water = all(isinstance(value, (int, float)) and math.isfinite(value) and 0 <= value <= 1 for value in fractions)
        check(checks, f"{id}: finite bounded submersion on every native frame", valid_water)
        check(checks, f"{id}: normalized finite physical transforms and velocities", all(
            abs(sum(value * value for value in native.vector(body["pose"]["rotation"], 4)) - 1) < 0.02
            and all(math.isfinite(value) for value in native.vector(body["linear_velocity"], 3)
                    + native.vector(body["angular_velocity"], 3)) for body in dynamic))
        bounds = [native.fixture_body_bounds(body, props[id]) for body in dynamic]
        minimum_y = min(bound[0][1] for bound in bounds)
        # Pool envelope includes dry deck ownership starts and normal throws.
        check(checks, f"{id}: physical collider stays above the lowest basin floor", minimum_y >= BOTTOM - 0.035, minimum_y=minimum_y)
        check(checks, f"{id}: collider remains inside the authored Pool room", all(
            lower[0] >= -0.035 and upper[0] <= 16.035 and lower[2] >= -0.035 and upper[2] <= 11.035
            for lower, upper in bounds))
        report["bodies"][id] = dict(samples=len(dynamic), position_bounds=[[min(value), max(value)] for value in zip(*positions)],
            minimum_physical_y=minimum_y, maximum_speed_mps=max(speed(body) for body in dynamic),
            submerged_fraction_bounds=[min(fractions), max(fractions)] if valid_water else None,
            final_speed_mps=speed(dynamic[-1]), final_sleeping=dynamic[-1]["sleeping"])
    keys = {key for key, id in authored.items() if id in (DUCK, SINK)}
    report["body_contacts"] = sum(set(pair) == keys for frame in frames for pair in frame.get("contacts", []))
    if contacts:
        check(checks, "two submerged bodies produce actual native contact pairs", report["body_contacts"] > 0,
              contact_frames=report["body_contacts"])
        # Positive pair impulses are reported by the native contact solver.
        impulses = [record for frame in frames for record in frame.get("contact_impulses", []) if set(record["pair"]) == keys]
        report["body_contact_impulses"] = len(impulses)
        check(checks, "submerged multiobject contact has solver impulse evidence", bool(impulses))
        for id in (DUCK, SINK):
            records = [body for frame in frames for body in frame["bodies"] if authored.get(body["key"]) == id]
            wet = [body for body in records if body.get("submerged_fraction", 0) > 0.01]
            rim = [native.fixture_body_bounds(body, props[id]) for body in wet]
            check(checks, f"{id}: submerged rim contact stays inside the basin", all(
                lower[0] >= 4 - 0.04 and upper[0] <= 13 + 0.04 and lower[2] >= 3 - 0.04 and upper[2] <= 7.5 + 0.04
                for lower, upper in rim))
        sink_records = [body for frame in frames for body in frame["bodies"] if authored.get(body["key"]) == SINK]
        check(checks, "high-speed impact reaches and is constrained by the east basin wall", any(
            native.fixture_body_bounds(body, props[SINK])[1][0] > 12.8 for body in sink_records)
            and any(body["linear_velocity"][0] < 0.1 for body in sink_records[20:]))
        final_name = "stress-settled" if "stress-settled" in samples else "contact-settled"
        check(checks, "both bodies settle after water contacts", all(settled(bodies(samples[final_name])[id]) for id in (DUCK, SINK)))
        if final_name == "stress-settled":
            counts = [len(frame["bodies"]) for frame in frames]
            check(checks, "water stress uses eighteen actual physical bodies", max(counts) == 18,
                  peak_bodies=max(counts))
            check(checks, "all stress bodies have finite bounded submersion", all(
                math.isfinite(body.get("submerged_fraction", float("nan")))
                and 0 <= body.get("submerged_fraction", -1) <= 1
                for frame in frames for body in frame["bodies"]))
    return report


def frame_metrics(path: Path) -> dict:
    with path.open() as stream:
        rows = list(csv.DictReader(stream))
    if not rows or not all(int(row["draw_calls"]) > 0 for row in rows):
        raise ValueError("missing real measured native scene submissions")
    cpu = {key: native.distribution([float(row[key]) for row in rows])
           for key in ("loop_ms", "frame_ms", "update_ms", "render_ms", "swap_ms")}
    gpu = {row["gpu_sample_id"]: float(row["gpu_render_ms"]) for row in rows
           if row.get("gpu_sample_id") and row.get("gpu_render_ms")}
    if not all(math.isfinite(value) and value >= 0 for values in cpu.values() for value in (values["median"], values["p95"], values["worst"])):
        raise ValueError("invalid native frame metrics")
    return dict(cpu_ms=cpu, gpu_elapsed_ms=native.distribution(list(gpu.values())),
                gpu_scope="asynchronous unique hardware render-pass elapsed samples; not utilization",
                draw_calls=native.distribution([int(row["draw_calls"]) for row in rows]),
                reflection_passes=sorted({int(row["reflection_passes"]) for row in rows}))


def video_command(args: argparse.Namespace, out: Path) -> list[str]:
    return [str(args.ffmpeg), "-hide_banner", "-nostdin", "-n", "-f", "rawvideo", "-pixel_format", "rgba",
            "-video_size", "1280x720", "-framerate", "30", "-i", str(out / "native-video.fifo"), "-an",
            "-c:v", "libx264", "-preset", "veryfast", "-crf", "18", "-threads", "2", "-vf",
            "scale=in_range=pc:out_range=tv:out_color_matrix=bt709,format=yuv420p,setparams=range=limited:color_primaries=bt709:color_trc=bt709:colorspace=bt709",
            "-colorspace", "bt709", "-color_primaries", "bt709", "-color_trc", "bt709", "-color_range", "tv",
            "-movflags", "+faststart", str(out / "water-native.mp4")]


def audit_video(args: argparse.Namespace, out: Path, events: list[dict], checks: list[dict]) -> dict:
    path = out / "water-native.mp4"
    frames = [json.loads(event["detail"]) for event in events if event["event"] == "native_video_frame"]
    sources = [int(frame["source_ready_frame"]) for frame in frames]
    check(checks, "video is an uninterrupted sequence of native rendered frames", len(frames) == args.video_frames
          and [int(frame["captured"]) for frame in frames] == list(range(1, args.video_frames + 1))
          and bool(sources) and sources[0] == args.video_start_frame + 1
          and all(b - a == 2 for a, b in zip(sources, sources[1:])))
    command = [str(args.ffprobe), "-v", "error", "-select_streams", "v:0", "-show_entries",
               "stream=codec_name,pix_fmt,width,height,nb_frames,avg_frame_rate,color_space,color_transfer,color_primaries,color_range:format=duration,size",
               "-of", "json", str(path)]
    probe = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, timeout=15)
    (out / "video-probe.log").write_text(probe.stdout + probe.stderr)
    if probe.returncode:
        raise ValueError("ffprobe failed to validate water-native.mp4")
    metadata = json.loads(probe.stdout)
    streams = metadata.get("streams", [])
    stream = streams[0] if len(streams) == 1 else {}
    check(checks, "every native source frame is encoded at 1280x720 and 30 fps", stream.get("codec_name") == "h264"
          and stream.get("pix_fmt") == "yuv420p" and (stream.get("width"), stream.get("height")) == (1280, 720)
          and int(stream.get("nb_frames", 0)) == args.video_frames and stream.get("avg_frame_rate") == "30/1"
          and abs(float(metadata.get("format", {}).get("duration", 0)) - args.video_frames / 30) < 0.05)
    check(checks, "video declares actual BT.709 limited-range conversion", all(stream.get(key) == "bt709"
          for key in ("color_space", "color_transfer", "color_primaries")) and stream.get("color_range") == "tv")
    return dict(path=str(path), sha256=native.digest(path), bytes=path.stat().st_size, metadata=metadata,
                first_source_ready_frame=sources[0] if sources else None, last_source_ready_frame=sources[-1] if sources else None)


def run_case(args: argparse.Namespace, out: Path, case: str, source: dict, binary: Path, view: str | None = None) -> dict:
    out.mkdir(parents=True, exist_ok=False)
    (out / "levels").mkdir()
    installed = out / "levels" / f"{LEVEL}.placesmap"
    shutil.copyfile(args.package, installed)
    # Bundled assets/levels wins discovery over state/levels. Isolate that
    # directory while retaining exactly the requested real asset bundle.
    assets = out / "assets"
    assets.mkdir()
    links = {}
    for entry in sorted((args.asset_root / "assets").iterdir()):
        if entry.name == "levels":
            continue
        target = entry.resolve(strict=True)
        (assets / entry.name).symlink_to(target, target_is_directory=target.is_dir())
        links[entry.name] = str(target)
    settings = json.loads((ROOT / "tests/fixtures/native/retro-arcade-manifest.json").read_text())["settings"]
    width, height = map(int, args.size.split("x"))
    settings.update(window_width=width, window_height=height,
                    window_mode="fullscreen" if args.fullscreen else "windowed", vsync=case != "performance",
                    developer_console=False, quality="high", texture_filtering="high", lightmaps=args.lightmaps,
                    reflections="full", bloom=True, use_low_quality_lighting=False, fov_degrees=60)
    steps = plan_for(case, source)
    if args.fullscreen:
        steps = native.reserve_capture_latency(steps)
    if case == "performance" and args.activate_demo:
        active = native.Plan(LEVEL)
        active.action(100, "activate", id=CONTROL)
        if args.given_cups:
            active.action(150, "console_enabled", enabled=True)
            active.tap(180, "`")
            for index in range(args.given_cups):
                ms = 300 + index * 180
                pose(active, ms, [5 + (index % 4) * 1.5, .1, 4.86 + (index // 4) * .6])
                active.command(ms + 70, f"/give {native.CUP_MODEL}")
            active.action(3400, "escape")
            active.player(3450, 3, 5.4, 90, -27)
        steps = active.steps
    native.write_json(out / "settings.json", settings)
    native.write_json(out / "actions.json", steps)
    env = {key: value for key, value in os.environ.items() if not key.startswith("PLACES_") and key != "SDL_VIDEODRIVER"}
    env.update(PLACES_ASSET_ROOT=str(out), PLACES_STATE_ROOT=str(out), PLACES_LEVEL=LEVEL,
               PLACES_BENCH="1", PLACES_VERBOSE="1", PLACES_SPAWN="3,1.6,5.4,90",
               PLACES_LOAD_TRACE=str(out / "trace.jsonl"), PLACES_STATE_LOG=str(out / "player.csv"),
               PLACES_BENCH_OUT=str(out / "frames.csv"), PLACES_BENCH_WARMUP=str(args.warmup if case == "performance" else 120),
               PLACES_BENCH_FRAMES=str(args.frames if case == "performance" else 3600),
               PLACES_BENCH_FIXED_DELTA_SECONDS="0.016666667", PLACES_PHYSICS_FIXED_DELTA_SECONDS="0.016666667",
               PLACES_PHYSICS_LOG=str(out / "physics.csv"))
    if steps:
        env["PLACES_BENCH_ACTIONS"] = str(out / "actions.json")
    if case not in (*VIEWS, "performance", "dormancy", "player"):
        env["PLACES_PHYSICS_TRACE"] = "1"
    if case in VIEWS or case == "performance":
        camera = VIEWS[view or case]
        env.update(PLACES_BENCH_CAMERA_POSITION=",".join(map(str, camera[:3])),
                   PLACES_CAMERA=",".join(map(str, camera[3:])))
    command = None
    if case in ("video", "caustics-video"):
        env.update(PLACES_BENCH_VIDEO_PIPE=str(out / "native-video.fifo"), PLACES_BENCH_VIDEO_SIZE="1280x720",
                   PLACES_BENCH_VIDEO_FRAMES=str(args.video_frames), PLACES_BENCH_VIDEO_STRIDE="2",
                   PLACES_BENCH_VIDEO_START_FRAME=str(args.video_start_frame))
        command = video_command(args, out)
    receipt = dict(mode=args.mode, case=case, view=view, status="planned", binary=str(binary), binary_sha256=native.digest(binary),
                   package=str(args.package), package_sha256=native.digest(args.package), source=str(args.source),
                   source_sha256=native.digest(args.source), asset_bundle_source=str(args.asset_root), asset_links=links, settings=settings,
                   performance_workload=f"activated authored demo plus {args.given_cups} given cups" if case == "performance" and args.activate_demo else "dormant startup" if case == "performance" else None,
                   environment={key: value for key, value in env.items() if key.startswith("PLACES_")},
                   diagnostic_setup=[dict(action_index=index, **step) for index, step in enumerate(steps)
                                     if step["action"]["kind"] in ("player", "item_pose", "item_velocity", "aim_item", "activate")],
                   checks=[], screenshots=[], metrics=None, physics_metrics=None,
                   video=dict(status="planned", encoder_command=command) if command else None)
    native.write_json(out / "receipt.json", receipt)
    if args.plan_only:
        return receipt
    started = time.monotonic()
    receipt.update(started_utc=datetime.now(timezone.utc).isoformat(), load_before=os.getloadavg())
    encoder = None
    encoder_log = None
    try:
        if command:
            args.ffmpeg.resolve(strict=True)
            args.ffprobe.resolve(strict=True)
            os.mkfifo(out / "native-video.fifo", mode=0o600)
            encoder_log = (out / "video-encoder.log").open("wb")
            encoder = subprocess.Popen(command, cwd=ROOT, stdin=subprocess.DEVNULL, stdout=encoder_log, stderr=subprocess.STDOUT)
        result = subprocess.run([str(binary)], cwd=ROOT, env=env, capture_output=True, text=True, timeout=120)
        log = result.stdout + result.stderr
        (out / "native.log").write_text(log)
        checks = receipt["checks"]
        receipt.update(exit_code=result.returncode, elapsed_seconds=time.monotonic() - started, load_after=os.getloadavg())
        rss = int(resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss) * (1 if sys.platform == "darwin" else 1024)
        receipt["peak_child_rss_bytes" if encoder else "peak_native_rss_bytes"] = rss
        receipt["rss_scope"] = "campaign child high-water mark; game plus encoder" if encoder else "campaign native child high-water mark"
        check(checks, "native process exits successfully", result.returncode == 0)
        check(checks, "binary and supplied/installed package hashes remain unchanged", native.digest(binary) == receipt["binary_sha256"]
              and native.digest(args.package) == native.digest(installed) == receipt["package_sha256"])
        check(checks, "Pool package commits without native validation errors", f"[loading] committed {LEVEL}" in log and not any(
            message in log.lower() for message in ("validation error", "panicked", "fatal device", "catalog hash mismatch")))
        identities = [dict(variant=variant, variant_sha256=sha, source=path) for variant, sha, path in re.findall(
            r"^\[loading\] package identity level=pool_showcase variant=(\S+) sha256=([0-9a-f]{64}) source=(.+)$", log, re.MULTILINE)]
        receipt["loaded_package_identity"] = identities
        check(checks, "native discovery loads exactly the selected package path and bytes", bool(identities) and all(
            Path(identity["source"]).resolve() == installed.resolve()
            and native.digest(Path(identity["source"])) == receipt["package_sha256"] for identity in identities),
            sources=sorted({identity["source"] for identity in identities}))
        drawables = [list(map(int, match)) for match in re.findall(r"^\[(?:renderer|window)\].*drawable[: ]+(\d+)x(\d+)", log, re.MULTILINE)]
        receipt["native_drawables"] = drawables
        receipt["native_identity"] = [line for line in log.splitlines() if line.startswith(("[renderer]", "[window]", "[settings]", "[wgpu] GPU timing"))]
        expected = list(map(int, args.expected_drawable.split("x"))) if args.expected_drawable else None
        if expected:
            check(checks, "actual native drawable matches the requested dimensions", bool(drawables) and all(size == expected for size in drawables))
        trace = out / "trace.jsonl"
        if trace.stat().st_size > MAX_TRACE_BYTES:
            raise ValueError("native water trace exceeds the bounded 64 MiB receipt input budget")
        events = [json.loads(line) for line in trace.read_text().splitlines()]
        presented = sum(event["event"] == "present" and event["detail"] == "ready" for event in events)
        missed = sum(event["event"] == "surface_not_presented" and event["detail"] == "ready" for event in events)
        check(checks, "native scene presents without missed ready surfaces", presented > 0 and missed == 0,
              presented_ready=presented, missed_ready=missed)
        recoveries = [json.loads(event["detail"]) for event in events if event["event"] == "physics_recovery"]
        check(checks, "ordinary water scenarios never use prop escape recovery", not recoveries, recovery_count=len(recoveries))
        samples, seen = native.snapshot_samples(events, steps)
        if case not in (*VIEWS, "performance"):
            reset_counts = [sample.get("player_body", {}).get("reset_count") for sample in samples.values()]
            check(checks, "ordinary water scenarios never return the player to spawn", bool(reset_counts)
                  and all(count == 0 for count in reset_counts), reset_counts=sorted({count for count in reset_counts if count is not None}))
        check(checks, "every bounded native action executes", seen == set(range(len(steps))))
        for step in steps:
            action = step["action"]
            if action["kind"] != "capture":
                continue
            path = out / f"{action['name']}.png"
            header = path.read_bytes()[:24] if path.is_file() else b""
            valid = len(header) == 24 and header[:8] == b"\x89PNG\r\n\x1a\n"
            size = [int.from_bytes(header[16:20], "big"), int.from_bytes(header[20:24], "big")] if valid else None
            check(checks, f"{action['name']}: real native PNG preserves drawable", valid and (expected is None or size == expected))
            if valid:
                receipt["screenshots"].append(dict(name=action["name"], path=str(path), size=size, sha256=native.digest(path)))
        audit_samples(case, samples, events, checks)
        receipt["snapshots"] = {name: dict(player_position=sample["player_position"], player_body=sample.get("player_body"),
            quality=sample["quality"], lighting=sample["lighting"], interaction_target=sample["interaction_target"],
            bodies=bodies(sample)) for name, sample in samples.items() if name != "world-start"}
        if env.get("PLACES_PHYSICS_TRACE"):
            receipt["physics_frame_audit"] = audit_frames(events, samples, source, checks, case in ("contacts", "stress"))
        receipt["physics_metrics"] = native.physics_metrics(out / "physics.csv")
        if case in ("performance", "stress", "settling"):
            receipt["metrics"] = frame_metrics(out / "frames.csv")
            receipt["metrics"]["scope"] = "matched fixed-view timing" if case == "performance" else "diagnostic scenario with native capture and body trace overhead"
        if case == "performance":
            check(checks, "complete matched native timing frame budget", receipt["metrics"]["cpu_ms"]["loop_ms"]["samples"] == args.frames)
            with (out / "physics.csv").open(newline="") as file:
                physics_rows = list(csv.DictReader(file))
            measured_rows = physics_rows[-args.frames:]
            if measured_rows:
                with (out / "measured-physics.csv").open("w", newline="") as file:
                    writer = csv.DictWriter(file, fieldnames=list(measured_rows[0]))
                    writer.writeheader()
                    writer.writerows(measured_rows)
                receipt["measured_physics_metrics"] = native.physics_metrics(out / "measured-physics.csv")
            if args.activate_demo:
                expected_bodies = 2 + args.given_cups
                check(checks, "requested active workload exists throughout measured frames", len(measured_rows) == args.frames
                      and all(int(row["body_count"]) == expected_bodies and int(row["dormant_bodies"]) == 0
                              and row["console_open"] == "false" for row in measured_rows), expected_bodies=expected_bodies)
        if encoder:
            exit_code = encoder.wait(timeout=30)
            check(checks, "continuous native encoder exits successfully", exit_code == 0, exit_code=exit_code)
            receipt["video"] = dict(status="encoded", encoder_command=command, **audit_video(args, out, events, checks))
        receipt["artifacts"] = {path.name: dict(bytes=path.stat().st_size, sha256=native.digest(path))
                                for path in (trace, out / "actions.json", out / "settings.json", out / "frames.csv", out / "physics.csv", out / "player.csv", out / "native.log")}
        failures = [value["name"] for value in checks if not value["passed"]]
        receipt["status"] = "failed" if failures else "passed"
        if failures:
            receipt["error"] = "; ".join(failures)
    except Exception as error:
        receipt.update(status="failed", error=str(error), elapsed_seconds=time.monotonic() - started)
        if isinstance(error, subprocess.TimeoutExpired):
            def decode(value: str | bytes | None) -> str:
                return value.decode(errors="replace") if isinstance(value, bytes) else value or ""
            (out / "native.log").write_text(decode(error.stdout) + decode(error.stderr))
    finally:
        if encoder is not None and encoder.poll() is None:
            encoder.terminate()
            try:
                encoder.wait(timeout=5)
            except subprocess.TimeoutExpired:
                encoder.kill()
                encoder.wait(timeout=5)
        if encoder_log is not None:
            encoder_log.close()
        native.write_json(out / "receipt.json", receipt)
    return receipt


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--mode", choices=("baseline", "views", "acceptance", "stress", "video", "caustics-video", "performance"), required=True)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/release/places")
    parser.add_argument("--package", type=Path, required=True, help="existing explicitly compiled Pool .placesmap")
    parser.add_argument("--source", type=Path, default=SOURCE, help="source matching the supplied package; receipts hash this provenance")
    parser.add_argument("--asset-root", type=Path, default=ROOT, help="matching real PNG/model asset root; use a preserved root for historical baselines")
    parser.add_argument("--out", type=Path, help="new directory, never overwrite a prior campaign")
    parser.add_argument("--case", choices=CASES, help="narrow acceptance to one fresh-state case")
    parser.add_argument("--view", choices=(*VIEWS, "both"), default="both", help="fixed exact matched view(s) for baseline/views/performance")
    parser.add_argument("--baseline-binary", type=Path, help="optional performance comparison on the identical supplied package/assets/settings")
    parser.add_argument("--activate-demo", action="store_true", help="performance only: activate authored demo during warmup to measure active/settling physics")
    parser.add_argument("--given-cups", type=int, default=0, help="performance only: 0..16 real given props during warmup")
    parser.add_argument("--warmup", type=int, default=120, help="performance warmup frames; use at least 600 with given props")
    parser.add_argument("--pairs", type=int, default=2, help="serial alternating performance pairs (1..3)")
    parser.add_argument("--frames", type=int, default=600, help="performance measured frames after 120 warmup (120..1800)")
    parser.add_argument("--lightmaps", choices=("static", "full"), default="full", help="independent lighting selection")
    parser.add_argument("--size", choices=("640x360", "1280x720"), default="640x360", help="logical window; 640x360 produces 1280x720 on a 2x display")
    parser.add_argument("--fullscreen", action="store_true", help="present on the native display; measure its actual drawable explicitly")
    parser.add_argument("--expected-drawable", default="1280x720", help="required actual native source/capture dimensions")
    parser.add_argument("--video-frames", type=int, default=600, help="actual consecutive video samples at 30 fps (1..900)")
    parser.add_argument("--video-start-frame", type=int, default=0, help="eligible first 60 Hz native ready frame (0..90)")
    parser.add_argument("--ffmpeg", type=Path, default=Path("/opt/homebrew/bin/ffmpeg"))
    parser.add_argument("--ffprobe", type=Path, default=Path("/opt/homebrew/bin/ffprobe"))
    parser.add_argument("--plan-only", action="store_true", help="write action plans, settings and hashes without launching the game")
    args = parser.parse_args()
    if args.case and args.mode != "acceptance":
        parser.error("--case applies only to acceptance")
    if args.baseline_binary and args.mode != "performance":
        parser.error("--baseline-binary applies only to performance")
    if args.activate_demo and args.mode != "performance":
        parser.error("--activate-demo applies only to performance")
    if not 0 <= args.given_cups <= 16 or not 120 <= args.warmup <= 1800:
        parser.error("given cups must be 0..16 and warmup 120..1800")
    if args.given_cups and (args.mode != "performance" or not args.activate_demo or args.warmup < 600):
        parser.error("given cups require performance, activation and at least 600 warmup frames")
    if not 1 <= args.pairs <= 3 or not 120 <= args.frames <= 1800:
        parser.error("pairs must be 1..3 and frames 120..1800")
    if not 1 <= args.video_frames <= 900 or not 0 <= args.video_start_frame <= 90:
        parser.error("video frames must be 1..900 and start frame 0..90")
    if args.mode in ("video", "caustics-video") and (args.fullscreen or args.expected_drawable != "1280x720"):
        parser.error("video requires actual 1280x720 source dimensions")
    match = re.fullmatch(r"(\d+)x(\d+)", args.expected_drawable)
    if match is None or not all(64 <= int(edge) <= 16384 for edge in match.groups()):
        parser.error("expected drawable must be WIDTHxHEIGHT with edges 64..16384")
    args.binary = args.binary.resolve(strict=True)
    args.package = args.package.resolve(strict=True)
    args.source = args.source.resolve(strict=True)
    args.asset_root = args.asset_root.resolve(strict=True)
    if args.package.suffix != ".placesmap":
        parser.error("package must be an existing explicitly compiled .placesmap")
    source = json.loads(args.source.read_text())
    if source.get("id") != LEVEL:
        parser.error("this campaign targets pool_showcase only")
    if args.mode in ("acceptance", "stress", "video", "caustics-video") or args.activate_demo:
        ids = {prop.get("id") for prop in source.get("props", [])}
        if not {DUCK, SINK, CONTROL}.issubset(ids):
            parser.error("supplied source needs the authored Pool duck, dense item and demo control")
    timestamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%S.%fZ")
    out = (args.out or ROOT / "local/water-physics-0.10.0" / f"{timestamp}-{args.mode}").resolve()
    out.mkdir(parents=True, exist_ok=False)
    campaign = dict(mode=args.mode, status="planned", out=str(out), runs=[], serial=True)
    native.write_json(out / "receipt.json", campaign)
    views = ["main", "west"] if args.view == "both" else [args.view]
    runs = []
    if args.mode in ("baseline", "views"):
        runs = [(view, args.binary, view, view) for view in views]
    elif args.mode == "acceptance":
        runs = [(case, args.binary, case, None) for case in ([args.case] if args.case else CASES)]
    elif args.mode in ("stress", "video", "caustics-video"):
        runs = [(args.mode, args.binary, args.mode, None)]
    elif args.mode == "performance":
        baseline = args.baseline_binary.resolve(strict=True) if args.baseline_binary else None
        for view in views:
            for pair in range(args.pairs):
                builds = [("baseline", baseline), ("candidate", args.binary)] if baseline else [("candidate", args.binary)]
                if pair % 2:
                    builds.reverse()
                runs.extend((f"{view}-{label}-{pair + 1}", binary, "performance", view) for label, binary in builds)
    for name, binary, case, view in runs:
        receipt = run_case(args, out / name, case, source, binary, view)
        campaign["runs"].append(dict(name=name, status=receipt["status"], receipt=str(out / name / "receipt.json"),
                                     binary_sha256=receipt["binary_sha256"], metrics=receipt["metrics"],
                                     failures=[check["name"] for check in receipt["checks"] if not check["passed"]], error=receipt.get("error")))
        campaign["status"] = "planned" if args.plan_only else "failed" if any(run["status"] == "failed" for run in campaign["runs"]) else "passed"
        native.write_json(out / "receipt.json", campaign)
        print(json.dumps(dict(out=str(out / name), status=receipt["status"], checks=len(receipt["checks"]), error=receipt.get("error"))), flush=True)
        # A launch/validation failure affects all following cases. Ordinary
        # assertion failures still allow independent scenarios to run.
        if receipt["status"] == "failed" and (receipt.get("exit_code") != 0 or not receipt["checks"]):
            break
    print(json.dumps(dict(out=str(out), status=campaign["status"], runs=len(campaign["runs"]))))
    if campaign["status"] == "failed":
        raise SystemExit(1)


if __name__ == "__main__":
    main()
