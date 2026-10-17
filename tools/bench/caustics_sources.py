#!/usr/bin/env python3
"""Author tiny caustics controls, or inspect an explicit compiled native package.

This opt-in developer campaign never builds Rust or bakes maps. `--author`
writes disposable JSON sources and compiler commands, referencing existing
committed artwork. `--run` launches the supplied executable directly, with a
fresh isolated installation, real SDL actions, native PNGs and bounded traces.
No app bundle, `open`, ordinary Rust test or automatic CI is involved.

Example:
  python3 tools/bench/caustics_sources.py --author local/caustics-0.11.0/controls
  # Run selected compile_commands from controls/manifest.json separately.
  python3 tools/bench/caustics_sources.py --run pool_light \
    --fixtures local/caustics-0.11.0/controls --binary target/release/places \
    --out local/caustics-0.11.0/native/pool-light

Source radiance controls affect the focused-water contribution. Existing baked
base illumination retains its prepared colour/intensity. Fixture actions also
exercise ordinary prepared on/off switching in the compact Pool-light case.
PNG deltas establish visible response; exact linear colour ratios are not
inferred from display-encoded pixels. FPS claims require a separate uncapped
frame_time.py campaign and verified physical drawable/presentation pacing.
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
import time

import item_physics as native

ROOT = Path(__file__).resolve().parents[2]
TRACE_LIMIT = 64 * 1024 * 1024
SOURCE_ID = "focus_source"
WHITE = [1.0, 1.0, 1.0]
WARM = [1.0, 0.08, 0.03]
GREEN = [0.05, 1.0, 0.1]
POOL_SOURCE_SCENARIOS = ("pool-only", "sun-only", "fluorescent-only", "moon-only", "none", "mixed-all", "excluded-only")


def digest(path: Path) -> str:
    checksum = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            checksum.update(block)
    return checksum.hexdigest()


def write_json(path: Path, value: object) -> None:
    path.write_text(json.dumps(value, indent=2, allow_nan=False) + "\n")


def control_source(name: str) -> tuple[dict, dict]:
    """Small source-type controls share geometry, water and neutral light colour."""
    level = "caustics_" + name
    room = dict(x=0, z=0, width=4, depth=4, floor_y=-1, height=4,
                material="core:pool_tile_basin_01", ceiling_material="core:pool_ceiling_01")
    source = dict(format_version=3, id=level, name="Caustics control: " + name,
                  author="Places explicit native diagnostics",
                  spawn=dict(x=0.2, z=3.8, yaw_degrees=45),
                  defaults=dict(wall="core:pool_tile_wall_01", floor="core:pool_tile_basin_01",
                                ceiling="core:pool_ceiling_01"),
                  environment=dict(fog=dict(density=0)), rooms=[room], walls=[],
                  water=[dict(x=0.4, z=0.4, width=3.2, depth=3.2, surface_y=0,
                              bottom_y=-1, material="core:water_pool_01", opacity=0,
                              swimming=True, caustics=dict(tint=WHITE, speed=0, strength=0.24))],
                  decals=[dict(x=2, y=-1, z=1.6, width=2.4, height=0.075,
                               surface="floor", material="pool:decal_lane_01")],
                  props=[dict(id="submerged_prop", model="core:washer_drum", x=1.1, y=0,
                              z=1.2, scale=1, size=[0.42, 0.3, 0.42], solid=False),
                         dict(id="dry_prop", model="core:washer_drum", x=0.21, y=0,
                              z=2.4, scale=0.5, size=[0.42, 0.3, 0.42], solid=False)])
    expected = name in ("pool_light", "sun", "thin_sun", "directional_optin", "mixed", "moving_blocker")
    info = dict(id=level, source_ids=[SOURCE_ID], eligible=expected,
                eye=[2, 2.2, 4.6], camera=[0, -50], source_controls=True,
                receiver_props=["submerged_prop", "dry_prop"],
                receiver_review=[dict(id="submerged_prop", world_base=[1.1, -1, 1.2],
                                      world_top_y=-0.7, scale=1),
                                 dict(id="dry_prop", world_base=[0.21, -1, 2.4],
                                      world_top_y=-0.85, scale=0.5, world_x_bounds=[0.105, 0.315])],
                visual_review=["Continuous floor/blue lane modulation", "Submerged prop receives focus",
                               "Dry perimeter and dry prop remain unchanged", "No wall or water-sheet focus"])
    if name in ("sun", "thin_sun", "moon", "moon_optin", "directional", "directional_optin", "blocked_sun"):
        if name != "blocked_sun":
            room["ceiling"] = dict(kind="open")
        kind = "moon" if name.startswith("moon") else "directional" if name.startswith("directional") else "sun"
        light = dict(id=SOURCE_ID, kind=kind, direction=[0, -1, 0], color=WHITE,
                     intensity=1, enabled=True, cast_shadows=True, bake=True, angular_size_degrees=0.5)
        if name in ("directional_optin", "moon_optin"):
            light["caustics"] = True
        if name == "moon_optin":
            info["expected_rejection"] = "is a moon and cannot enable caustics"
        source["global_illuminators"] = [light]
        if name == "blocked_sun":
            # The header permits self-excluded instance receivers even when
            # every shared lattice cell is shadowed. Pixel checks remain dark.
            info["source_potential"] = True
        if name == "thin_sun":
            room.update(x=-2, z=-2)
            source["spawn"] = dict(x=1.5, z=1.5, yaw_degrees=0)
            source["water"][0].update(x=-0.8, z=-0.8, width=1.6, depth=1.6)
            source["props"], source["decals"] = [], []
            source["walls"] = [dict(x=0.17, z=-0.7, width=0.02, depth=1.4,
                                    y=0.5, height=0.05, material="core:pool_tile_wall_01")]
            info.update(eye=[0.18, 2.2, 2.2], camera=[0, -56],
                        receiver_props=[], receiver_review=[],
                        shadow_review=dict(source="vertical Sun", receiver_y=-1,
                                           blocked_strip=dict(x=[0.17, 0.19], z=[-0.7, 0.7]),
                                           visible_control_points=[[-0.4, -1, 0], [0.6, -1, 0]]),
                        visual_review=["No focused energy reaches the floor directly below the 20 mm overhead strip",
                                       "Adjacent unblocked underwater floor still receives Sun focus",
                                       "Conservative coarse cells may widen the shadow but may not fill its interior"])
    elif name == "mixed":
        room.update(width=6, depth=4)
        source["water"][0]["width"] = 5.2
        source["decals"][0].update(x=3, width=4.8, height=0.15)
        source["walls"] = [dict(x=2.94, y=-1, z=0, width=0.12, depth=2.7,
                                height=4, material="core:pool_tile_wall_01")]
        source["ceiling_lights"] = [
            dict(id="focus_red", fixture="core:pool_light_round", x=1.5, z=1.4,
                 y=2.9, color=[1, 0.04, 0.04], brightness=1, range=8, align="none"),
            dict(id="focus_blue", fixture="core:pool_light_round", x=4.5, z=1.4,
                 y=2.9, color=[0.04, 0.04, 1], brightness=1, range=8, align="none"),
            dict(id="excluded_green", fixture="core:fluorescent_panel_01", x=3, z=3.2,
                 y=2.9, color=[0.04, 1, 0.04], brightness=0.3, range=8, align="none")]
        info.update(source_ids=["focus_red", "focus_blue"], eye=[3, 2.3, 5.2],
                    source_controls=False, visual_review=[
                        "Red focus stays in the left source's visible receiver region",
                        "Blue focus stays in the right source's visible receiver region",
                        "Opaque partition excludes source focus behind it",
                        "Excluded green fluorescent fill never supplies focus colour",
                        "The shared basin remains active around the partition's open end"])
    else:
        fluorescent = name.startswith("fluorescent")
        light = dict(id=SOURCE_ID,
                     fixture="core:fluorescent_panel_01" if fluorescent else "core:pool_light_round",
                     x=2, z=2, y=2.9, brightness=1, color=WHITE, range=6,
                     align="none", enabled=True, emission=0.5)
        if name == "pool_optout":
            light["caustics"] = False
        if name == "fluorescent_optin":
            light["caustics"] = True
            info["expected_rejection"] = "caustics is supported only by pool round/wall fixture families"
        if name == "pool_light":
            light.update(switchable=True, switch_group="focus_circuit", static_switch="prepared")
        source["ceiling_lights"] = [light]
        if name == "moving_blocker":
            source["props"].append(dict(
                id="moving_blocker", model="core:crate", x=3.2, y=0, z=3.2, scale=2,
                size=[0.6, 0.6, 0.6], solid=False,
                physics=dict(motion="dynamic", pickup=True, mass_kg=24, friction=0.8,
                             restitution=0, linear_damping=0.8, angular_damping=0.8,
                             water=dict(displaced_volume_m3=0.001),
                             shape=dict(kind="cuboid", half_extents=[0.3, 0.3, 0.3]))))
            info.update(source_controls=False, moving_blocker=True,
                        visual_review=["The actual 1.2 m crate casts a focused-light shadow after moving under the source",
                                       "The caustic field recovers after the actual crate returns to the perimeter",
                                       "Same-pose strength-zero captures isolate focused light from the moved crate's own pixels"])
    return source, info


def author(out: Path, compiler: Path) -> dict:
    if out == ROOT or out == ROOT / "assets" or ROOT / "assets" in out.parents:
        raise ValueError("fixture output must be separate from production assets")
    sources, packages = out / "sources", out / "packages"
    sources.mkdir(parents=True, exist_ok=True)
    packages.mkdir(exist_ok=True)
    cases = {}
    for name in ("pool_light", "pool_optout", "fluorescent", "fluorescent_optin", "sun", "moon", "moon_optin",
                 "directional", "directional_optin", "blocked_sun", "thin_sun", "mixed", "moving_blocker"):
        source, info = control_source(name)
        path = sources / f"{source['id']}.json"
        write_json(path, source)
        cases[name] = dict(**info, source=str(path), source_sha256=digest(path),
                           package=str(packages / f"{source['id']}.placesmap"))
    pool_path = ROOT / "levels/pool_showcase.json"
    pool = json.loads(pool_path.read_text())
    pool["id"], pool["name"] = "caustics_pool", "Pool caustics inspection clone"
    ids = []
    for index, light in enumerate(pool["ceiling_lights"]):
        light.setdefault("id", f"caustics_pool_light_{index}")
        ids.append(light["id"])
    path = sources / "caustics_pool.json"
    write_json(path, pool)
    cases["pool"] = dict(id="caustics_pool", source=str(path), source_sha256=digest(path),
                         package=str(packages / "caustics_pool.placesmap"),
                         source_ids=ids, eligible=True, source_controls=False,
                         eye=[3.8, 1.6, 5.4], camera=[90, -42],
                         original_source=str(pool_path), original_sha256=digest(pool_path),
                         visual_review=["Basin and blue lane stripes share the continuous projection",
                                        "Shallow shelf, ceramic steps and submerged props receive focus",
                                        "Dry deck and the circular tub's dry bounding-box corners stay dry",
                                        "Circle, shelf and waterline remain bounded during animation"])
    pool_sources, pool_source_info = pool_sources_control(pool_path)
    path = sources / "caustics_pool_sources.json"
    write_json(path, pool_sources)
    cases["pool_sources"] = dict(**pool_source_info, source=str(path), source_sha256=digest(path),
                                 package=str(packages / "caustics_pool_sources.placesmap"))
    commands, rejections = [], []
    for name, case in cases.items():
        command = [str(compiler), "build", case["source"], "--out", case["package"],
                   "--asset-root", str(ROOT / "assets"), "--workers", "8"]
        if "expected_rejection" in case:
            rejections.append(dict(case=name, command=command, expected_nonzero=True,
                                   expected_diagnostic=case["expected_rejection"]))
            continue
        commands.append(dict(case=name, build=command,
                             validate=[str(compiler), "validate", case["package"]],
                             verify=[str(compiler), "verify", case["source"], "--package", case["package"],
                                     "--require-current"]))
    manifest = dict(format_version=1, purpose="Explicit native caustics source, control and receiver validation",
                    asset_root=str(ROOT), cases=cases, compile_commands=commands, rejection_commands=rejections,
                    cost="Eleven 4x4/6x4 m controls, two invalid-source controls and two optional Pool clones; compile/run selected cases",
                    assertions=["Native executable and exact selected package commit successfully",
                                "Every real SDL action executes and every requested PNG exists",
                                "Eligible sources visibly change frozen underwater focus",
                                "Excluded fluorescent/Moon/directional and roof-blocked Sun add no focus",
                                "Runtime source off/intensity/colour and water tint/strength alter focused light",
                                "A zero animation speed holds while the 1.35 default animates gently"],
                    limitations=["Native PNG colour is display encoded; linear ratios are not inferred",
                                 "Baked base illumination remains prepared during radiance controls",
                                 "Visual review of occlusion, decals, props and dry boundaries is required",
                                 "This capture campaign supplies no FPS performance conclusion"])
    write_json(out / "manifest.json", manifest)
    return dict(manifest=str(out / "manifest.json"), cases=list(cases), builds_or_native_runs=0)


def pool_sources_control(original_path: Path) -> tuple[dict, dict]:
    """Retain real Pool receivers while exposing direct Sun through open ceilings."""
    source = json.loads(original_path.read_text())
    source["id"], source["name"] = "caustics_pool_sources", "Pool source-type acceptance control"
    for room in source["rooms"]:
        room["ceiling"] = dict(kind="open")
    fixture_ids = []
    for index, light in enumerate(source["ceiling_lights"]):
        light.setdefault("id", f"pool_control_fixture_{index}")
        fixture_ids.append(light["id"])
    sun, moon, fluorescent = "pool_control_sun", "pool_control_moon", "pool_control_fluorescent"
    source.setdefault("global_illuminators", []).extend([
        dict(id=sun, kind="sun", direction=[0, -1, 0], color=[1, 0.96, 0.86], intensity=0.9,
             enabled=True, cast_shadows=True, bake=True, angular_size_degrees=0.5),
        dict(id=moon, kind="moon", direction=[0, -1, 0], color=[0.7, 0.8, 1], intensity=0.8,
             enabled=True, cast_shadows=True, bake=True, angular_size_degrees=0.5)])
    source["ceiling_lights"].append(dict(
        id=fluorescent, fixture="core:fluorescent_panel_01", x=8, z=5.2, y=4.3,
        brightness=0.9, color=WHITE, range=10, align="none", enabled=True, emission=0.5))
    groups = dict(pool=fixture_ids, sun=[sun], fluorescent=[fluorescent], moon=[moon])
    return source, dict(
        id=source["id"], source_ids=[*fixture_ids, sun, moon, fluorescent], eligible=True,
        source_controls=False, water_count=len(source["water"]), pool_source_groups=groups,
        eye=[3.8, 1.6, 5.4], camera=[90, -42], original_source=str(original_path),
        original_sha256=digest(original_path),
        visual_review=["Real Pool basin, blue decals, shallow shelf, steps, props and circular tub retain their source geometry",
                       "Pool-only and Sun-only focus the same real Pool receivers",
                       "Fluorescent-only, Moon-only, no-source and excluded-only combinations add no focused light",
                       "Mixed eligible contributions add while excluded sources supply no focus energy"],
        source_variant_changes=["Open room ceilings for direct vertical Sun", "Stable explicit fixture ids",
                                "Add Sun, Moon and fluorescent control sources"])


def plan(case: dict, name: str, animation: bool = False, speed: float = 1.35,
         legacy: bool = False) -> list[dict]:
    p = native.Plan(case["id"])
    water_count = case.get("water_count", 3 if name == "pool" else 1)
    if animation:
        if not legacy:
            for index in range(water_count):
                p.action(0, "caustics_water", index=index, speed=speed)
        p.capture(600, "animation-initial")
        p.capture(8000, "animation-final")
        return p.finish(8400)
    if name == "pool_sources":
        return pool_sources_plan(p, case)
    if name == "pool" and not animation:
        p.action(0, "player", position=case["eye"], yaw=case["camera"][0], pitch=case["camera"][1])
    for index in range(water_count):
        p.action(0, "caustics_water", index=index, speed=0)
    p.capture(600, "frozen")
    for source_id in case["source_ids"]:
        p.action(900, "caustics_source", id=source_id, on=False)
    p.capture(1400, "source-off")
    for source_id in case["source_ids"]:
        p.action(1600, "caustics_source", id=source_id, on=True)
    if case["source_controls"]:
        p.action(1800, "caustics_source", id=case["source_ids"][0], intensity=0.2)
        p.capture(2300, "source-dim")
        p.action(2500, "caustics_source", id=case["source_ids"][0], intensity=1, color=WARM)
        p.capture(3000, "source-warm")
        p.action(3200, "caustics_source", id=case["source_ids"][0], color=WHITE)
    for index in range(water_count):
        p.action(3400, "caustics_water", index=index, tint=GREEN)
    p.capture(3900, "water-tint")
    for index in range(water_count):
        p.action(4100, "caustics_water", index=index, tint=WHITE, strength=0)
    p.capture(4600, "water-disabled")
    for index in range(water_count):
        p.action(4800, "caustics_water", index=index, strength=0.24, speed=1.35)
    p.capture(5500, "animation-a")
    p.capture(7300, "animation-b")
    for index in range(water_count):
        p.action(7500, "caustics_water", index=index, speed=0)
    p.capture(8100, "held-a")
    p.capture(9000, "held-b")
    if name == "mixed":
        mixed_plan(p)
        return p.finish(20_100)
    if name == "moving_blocker":
        blocker_plan(p)
        return p.finish(15_100)
    if name == "pool_light":
        p.action(9200, "fixture", id=SOURCE_ID, on=False)
        p.capture(9900, "fixture-off")
        p.action(10_100, "fixture", id=SOURCE_ID, on=True)
        p.capture(10_800, "fixture-on")
    if name == "pool":
        p.action(9100, "player", position=[12.6, 1.6, 8.9], yaw=307, pitch=-43)
        for at, label in ((9400, "pool-shelf-steps"), (10_900, "pool-circle")):
            if label == "pool-circle":
                p.action(10_500, "player", position=[5.7, 1.6, 10.4], yaw=66, pitch=-42)
            p.capture(at, label)
            for index in range(water_count):
                p.action(at + 100, "caustics_water", index=index, strength=0)
            p.capture(at + 500, label + "-no-focus")
            for index in range(water_count):
                p.action(at + 600, "caustics_water", index=index, strength=0.24)
            p.capture(at + 1000, label + "-restored")
        p.action(12_000, "player", position=case["eye"], yaw=case["camera"][0], pitch=case["camera"][1])
        p.action(12_200, "lightmaps", quality="static")
        p.capture(15_000, "pool-static")
        p.action(15_400, "quality", level="low")
        p.capture(19_000, "pool-static-low-textures")
        p.action(19_500, "lightmaps", quality="full")
        p.capture(23_000, "pool-full-low-textures")
        p.action(23_500, "quality", level="high")
        p.capture(27_000, "pool-full-high-restored")
        p.action(28_000, "player", position=[3.8, 1.6, 5.2], yaw=90, pitch=-35)
        p.item_pose(28_500, [8, -1.5, 5.2], [0, 0, 0, 1], id="pool_physics_sink")
        p.capture(31_800, "pool-submerged-dynamic")
        for index in range(water_count):
            p.action(32_000, "caustics_water", index=index, strength=0)
        p.capture(32_600, "pool-submerged-dynamic-no-focus")
        for index in range(water_count):
            p.action(32_800, "caustics_water", index=index, strength=0.24)
        p.capture(33_400, "pool-submerged-dynamic-restored")
    return p.finish(33_800 if name == "pool" else 11_200)


def pool_source_selection(case: dict, name: str) -> set[str]:
    groups = case["pool_source_groups"]
    selections = {"pool-only": groups["pool"], "sun-only": groups["sun"],
                  "fluorescent-only": groups["fluorescent"], "moon-only": groups["moon"],
                  "none": [], "mixed-all": case["source_ids"],
                  "excluded-only": groups["moon"] + groups["fluorescent"]}
    return set(selections[name])


def pool_sources_plan(p: native.Plan, case: dict) -> list[dict]:
    for index in range(case["water_count"]):
        p.action(0, "caustics_water", index=index, speed=0, tint=WHITE, strength=0.24)
    current = set(case["source_ids"])
    for ordinal, name in enumerate(POOL_SOURCE_SCENARIOS):
        ms = 100 + ordinal * 1400
        selected = pool_source_selection(case, name)
        for source_id in case["source_ids"]:
            if (source_id in current) != (source_id in selected):
                p.action(ms, "caustics_source", id=source_id, on=source_id in selected)
        current = selected
        p.capture(ms + 350, name)
        for index in range(case["water_count"]):
            p.action(ms + 600, "caustics_water", index=index, strength=0)
        p.capture(ms + 950, name + "-no-focus")
        for index in range(case["water_count"]):
            p.action(ms + 1150, "caustics_water", index=index, strength=0.24)
    return p.finish(10_000)



def mixed_plan(p: native.Plan) -> None:
    """Hold the pattern while changing exactly one named source per step."""
    p.capture(9400, "mixed-both")
    p.action(9600, "caustics_source", id="focus_blue", on=False)
    p.capture(10_100, "mixed-red-only")
    p.action(10_300, "caustics_source", id="focus_red", intensity=0.25)
    p.capture(10_800, "mixed-red-dim")
    p.action(11_000, "caustics_source", id="focus_red", intensity=1, color=GREEN)
    p.capture(11_500, "mixed-red-recolored")
    p.action(11_700, "caustics_source", id="focus_red", color=[1, 0.04, 0.04], on=False)
    p.action(11_800, "caustics_source", id="focus_blue", on=True)
    p.capture(12_300, "mixed-blue-only")
    p.action(12_500, "caustics_source", id="focus_blue", intensity=0.25)
    p.capture(13_000, "mixed-blue-dim")
    p.action(13_200, "caustics_source", id="focus_blue", intensity=1, color=WARM)
    p.capture(13_700, "mixed-blue-recolored")
    p.action(13_900, "caustics_source", id="focus_blue", color=[0.04, 0.04, 1])
    p.action(14_000, "caustics_source", id="focus_red", on=True)
    p.capture(14_500, "mixed-restored")
    p.action(14_700, "caustics_source", id="excluded_green", on=False, intensity=0, color=[1, 0, 1])
    p.capture(15_200, "mixed-excluded-off")
    p.action(15_400, "caustics_source", id="excluded_green", on=True, intensity=8, color=WHITE)
    p.capture(15_900, "mixed-excluded-bright")
    p.action(16_100, "caustics_source", id="excluded_green", intensity=0.3, color=[0.04, 1, 0.04])
    # Restore each source independently once more to expose accidental shared controls.
    p.action(16_300, "caustics_source", id="focus_red", on=False)
    p.capture(16_800, "mixed-blue-still-on")
    p.action(17_000, "caustics_source", id="focus_red", on=True)
    p.action(17_100, "caustics_source", id="focus_blue", on=False)
    p.capture(17_600, "mixed-red-still-on")
    p.action(17_800, "caustics_source", id="focus_blue", on=True)
    p.capture(18_300, "mixed-final")


def blocker_plan(p: native.Plan) -> None:
    """Move the real physical crate; camera, source and pattern remain fixed."""
    p.capture(9400, "blocker-clear")
    p.item_pose(9600, [2, -1, 2], [0, 0, 0, 1], id="moving_blocker")
    p.capture(10_600, "blocker-blocked")
    p.action(10_800, "caustics_water", index=0, strength=0)
    p.capture(11_300, "blocker-blocked-no-focus")
    p.action(11_500, "caustics_water", index=0, strength=0.24)
    p.capture(12_000, "blocker-blocked-restored")
    p.item_pose(12_200, [3.2, -1, 3.2], [0, 0, 0, 1], id="moving_blocker")
    p.capture(13_200, "blocker-returned")
    p.action(13_400, "caustics_water", index=0, strength=0)
    p.capture(13_900, "blocker-clear-no-focus")
    p.action(14_100, "caustics_water", index=0, strength=0.24)
    p.capture(14_600, "blocker-clear-restored")


def check(checks: list, name: str, passed: bool, **evidence: object) -> None:
    checks.append(dict(name=name, passed=passed, **evidence))


def audit_diagnostics(samples: dict, case: dict, checks: list, lightmaps: str = "full") -> None:
    if case.get("pool_source_groups"):
        audit_pool_sources(samples, case, checks, lightmaps)
        return
    snapshots = {name: sample.get("caustics") for name, sample in samples.items()}
    frozen = snapshots.get("frozen") or {}
    check(checks, "native snapshots expose installed caustic source and water state",
          bool(frozen.get("waters")) and isinstance(frozen.get("sources"), list))
    initial = samples.get("frozen", {})
    expected_props = set(case.get("receiver_props", []))
    if expected_props:
        fixed = {prop.get("id"): prop for prop in initial.get("fixed_instances", [])}
        check(checks, "native world retains both named receiver-control props", expected_props.issubset(fixed)
              and all(fixed[prop].get("entity_live") is True and fixed[prop].get("model") == "core:washer_drum"
                      for prop in expected_props))
    expected_lightmaps = "off" if lightmaps == "static" else "full"
    check(checks, "requested initial lighting mode commits independently of High textures",
          initial.get("renderer_lightmaps") == expected_lightmaps and initial.get("renderer_quality") == "high",
          requested_lightmaps=lightmaps, actual_lightmaps=initial.get("renderer_lightmaps"),
          actual_texture_quality=initial.get("renderer_quality"))
    if not frozen:
        return
    check(checks, "receiver and source-factor storage stay within declared budgets",
          0 < frozen.get("receiver_samples", 0) <= frozen.get("receiver_limit", 0)
          and 0 <= frozen.get("factor_entries", -1) <= frozen.get("factor_limit", -1)
          and frozen.get("sources_dropped_by_budget") == 0,
          receiver_samples=frozen.get("receiver_samples"), factor_entries=frozen.get("factor_entries"),
          dropped_sources=frozen.get("sources_dropped_by_budget"))
    expected_header = case.get("source_potential", case["eligible"])
    check(checks, "potential eligible receiver support determines the active shader header",
          (frozen.get("header_count", 0) > 0) == expected_header,
          expected_support=expected_header, header_count=frozen.get("header_count"))
    source_ids = set(case["source_ids"])
    def source_states(name: str) -> list[dict]:
        return [source for source in (snapshots.get(name) or {}).get("sources", []) if source.get("id") in source_ids]
    off = source_states("source-off")
    check(checks, "every requested source id is present and stores its disabled state",
          {source.get("id") for source in off} == source_ids and all(source.get("enabled") is False for source in off))
    if case["source_controls"]:
        dim, warm = source_states("source-dim"), source_states("source-warm")
        check(checks, "renderer stores the requested source intensity", bool(dim)
              and all(abs(source.get("intensity", -100) - 0.2) < 0.00001 for source in dim))
        check(checks, "renderer stores the requested source colour", bool(warm) and all(
            len(source.get("color", [])) == 3 and all(abs(a - b) < 0.00001 for a, b in zip(source["color"], WARM))
            for source in warm))
    for name, key, expected in (("frozen", "speed", 0), ("water-disabled", "strength", 0),
                                ("animation-a", "speed", 1.35), ("held-a", "speed", 0)):
        waters = (snapshots.get(name) or {}).get("waters", [])
        check(checks, name + ": renderer stores the requested water control",
              bool(waters) and all(abs(water.get(key, -100) - expected) < 0.00001 for water in waters),
              field=key, expected=expected)
    waters = (snapshots.get("water-tint") or {}).get("waters", [])
    check(checks, "renderer stores the independent green water tint", bool(waters) and all(
        len(water.get("tint", [])) == 3 and all(abs(a - b) < 0.00001 for a, b in zip(water["tint"], GREEN))
        for water in waters))
    for name in ("source-off", "water-disabled"):
        check(checks, name + ": shader header takes the all-off path",
              (snapshots.get(name) or {}).get("header_count") == 0)
    held_a, held_b = snapshots.get("held-a") or {}, snapshots.get("held-b") or {}
    check(checks, "steady held state does not repeat source-field uploads",
          held_a.get("source_uploads") is not None and held_a.get("source_uploads") == held_b.get("source_uploads"))
    if "fixture-off" in snapshots:
        check(checks, "ordinary fixture off selects the prepared circuit state",
              samples["fixture-off"].get("light_states") == [[0, False]]
              and (snapshots["fixture-off"] or {}).get("header_count") == 0)
        check(checks, "ordinary fixture on restores the prepared circuit and focus",
              samples["fixture-on"].get("light_states") == [[0, True]]
              and (snapshots["fixture-on"] or {}).get("header_count", 0) > 0)
    if "mixed-both" in snapshots:
        audit_mixed(snapshots, checks)
    if "blocker-clear" in snapshots:
        audit_blocker(samples, snapshots, checks)
    if "pool-static" in snapshots:
        audit_pool_settings(samples, snapshots, case, checks)
        audit_pool_dynamic(samples, snapshots, checks)


def audit_pool_settings(samples: dict, snapshots: dict, case: dict, checks: list) -> None:
    for name, lightmaps, textures in (("pool-static", "off", "high"),
                                      ("pool-static-low-textures", "off", "low"),
                                      ("pool-full-low-textures", "full", "low"),
                                      ("pool-full-high-restored", "full", "high")):
        sample = samples.get(name, {})
        snapshot = snapshots.get(name) or {}
        check(checks, name + ": lighting and texture quality commit independently",
              sample.get("renderer_lightmaps") == lightmaps and sample.get("renderer_quality") == textures,
              actual_lightmaps=sample.get("renderer_lightmaps"), actual_texture_quality=sample.get("renderer_quality"))
        waters = snapshot.get("waters", [])
        check(checks, name + ": runtime water controls survive graphics transitions", len(waters) == 3 and all(
            water.get("speed") == 0 and abs(water.get("strength", -100) - 0.24) < 0.00001
            and water.get("tint") == WHITE for water in waters))
        sources = [source for source in snapshot.get("sources", []) if source.get("id") in case["source_ids"]]
        check(checks, name + ": eligible source states survive graphics transitions",
              {source.get("id") for source in sources} == set(case["source_ids"])
              and all(source.get("enabled") is True for source in sources)
              and snapshot.get("header_count", 0) > 0)


def audit_pool_dynamic(samples: dict, snapshots: dict, checks: list) -> None:
    sample = samples.get("pool-submerged-dynamic", {})
    physics = sample.get("physics", {})
    ids = {entry[0]: entry[1] for entry in physics.get("authored", [])}
    bodies = [body for body in physics.get("bodies", []) if ids.get(body.get("key")) == "pool_physics_sink"]
    position = native.body_position(bodies[0]) if bodies else []
    check(checks, "real Pool sink reaches the fully submerged basin-floor pose", bool(position)
          and native.distance(position, [8, -1.5, 5.2]) < 0.04, actual_position=position)
    receivers = [receiver for receiver in (snapshots.get("pool-submerged-dynamic") or {}).get("dynamic_receivers", [])
                 if "washer_drum" in str(receiver.get("model", ""))]
    check(checks, "real Pool submerged dynamic receiver publishes positive focused radiance", bool(receivers) and any(
        any(channel > 0 for anchor in receiver.get("radiance", []) for channel in anchor[:3])
        for receiver in receivers), receivers=receivers)
    for name, strength in (("pool-submerged-dynamic-no-focus", 0), ("pool-submerged-dynamic-restored", 0.24)):
        waters = (snapshots.get(name) or {}).get("waters", [])
        check(checks, name + ": real Pool water strength stores the paired receiver control", len(waters) == 3 and all(
            abs(water.get("strength", -100) - strength) < 0.00001 for water in waters))


def receiver_energy(snapshot: dict | None) -> list[float]:
    energy = (snapshot or {}).get("sample_energy_rgb", [])
    return energy if len(energy) == 3 and all(math.isfinite(value) and value >= 0 for value in energy) else []


def audit_pool_sources(samples: dict, case: dict, checks: list, lightmaps: str) -> None:
    snapshots = {name: sample.get("caustics") or {} for name, sample in samples.items()}
    energy = {name: receiver_energy(snapshots.get(name)) for name in POOL_SOURCE_SCENARIOS}
    check(checks, "real Pool combinations expose finite composed receiver energy", all(len(rgb) == 3 for rgb in energy.values()))
    expected_lightmaps = "off" if lightmaps == "static" else "full"
    all_ids = set(case["source_ids"])
    for name in POOL_SOURCE_SCENARIOS:
        selected = pool_source_selection(case, name)
        eligible = name in ("pool-only", "sun-only", "mixed-all")
        sample, snapshot = samples.get(name, {}), snapshots.get(name, {})
        states = [source for source in snapshot.get("sources", []) if source.get("id") in all_ids]
        check(checks, name + ": real Pool installs exactly the requested named source combination",
              {source.get("id") for source in states} == all_ids and all(
                  source.get("enabled") is (source.get("id") in selected) for source in states))
        check(checks, name + ": actual lighting mode and High textures remain fixed",
              sample.get("renderer_lightmaps") == expected_lightmaps and sample.get("renderer_quality") == "high")
        check(checks, name + ": source eligibility selects the actual shader header",
              (snapshot.get("header_count", 0) > 0) == eligible)
        check(checks, name + ": receiver storage stays within declared budgets",
              0 < snapshot.get("receiver_samples", 0) <= snapshot.get("receiver_limit", 0)
              and 0 <= snapshot.get("factor_entries", -1) <= snapshot.get("factor_limit", -1)
              and snapshot.get("sources_dropped_by_budget") == 0)
        for suffix, strength in (("", 0.24), ("-no-focus", 0)):
            waters = snapshots.get(name + suffix, {}).get("waters", [])
            check(checks, name + suffix + ": the real Pool waters preserve the frozen paired controls",
                  len(waters) == case["water_count"] and all(
                      water.get("speed") == 0 and water.get("tint") == WHITE
                      and abs(water.get("strength", -100) - strength) < 0.00001 for water in waters))
        check(checks, name + ": strength-zero pair takes the actual shader all-off path",
              snapshots.get(name + "-no-focus", {}).get("header_count") == 0)
        if len(energy[name]) == 3:
            check(checks, name + ": composed focused energy follows source eligibility",
                  sum(energy[name]) > 0 if eligible else energy[name] == [0, 0, 0], rgb=energy[name])
            check(checks, name + ": water strength changes only focus presentation",
                  receiver_energy(snapshots.get(name + "-no-focus")) == energy[name])
    if all(len(rgb) == 3 for rgb in energy.values()):
        expected = [pool + sun for pool, sun in zip(energy["pool-only"], energy["sun-only"])]
        check(checks, "real Pool mixed focus equals Pool-source plus Sun-source energy", all(
            abs(a - b) <= max(1, abs(b)) * 0.000001 for a, b in zip(energy["mixed-all"], expected)),
            mixed=energy["mixed-all"], expected=expected)
        check(checks, "real Pool excluded-only focus equals the no-source result",
              energy["excluded-only"] == energy["none"] == [0, 0, 0])
    excluded_ids = set(case["pool_source_groups"]["moon"] + case["pool_source_groups"]["fluorescent"])
    excluded = [source for source in snapshots.get("mixed-all", {}).get("sources", []) if source.get("id") in excluded_ids]
    check(checks, "real Pool Moon and fluorescent controls stay explicitly excluded",
          {source.get("id") for source in excluded} == excluded_ids
          and all(source.get("eligible") is False and source.get("factor_entries") == 0 for source in excluded))


def audit_mixed(snapshots: dict, checks: list) -> None:
    energy = {name: receiver_energy(snapshot) for name, snapshot in snapshots.items() if name.startswith("mixed-")}
    valid = bool(energy) and all(len(value) == 3 for value in energy.values())
    check(checks, "mixed-light snapshots expose finite composed receiver RGB", valid)
    if not valid:
        return
    both, red, blue = (energy[name] for name in ("mixed-both", "mixed-red-only", "mixed-blue-only"))
    additive = [a + b for a, b in zip(red, blue)]
    check(checks, "mixed receiver energy equals independently retained red plus blue contributions", all(
        abs(a - b) <= max(1, abs(a)) * 0.000001 for a, b in zip(both, additive)), both=both, red=red, blue=blue)
    for color, original in (("red", red), ("blue", blue)):
        dim = energy[f"mixed-{color}-dim"]
        check(checks, f"{color} dims independently while the other source stays off", all(
            abs(a * 0.25 - b) <= max(1, abs(a)) * 0.000001 for a, b in zip(original, dim)), full=original, dim=dim)
    green, warm = energy["mixed-red-recolored"], energy["mixed-blue-recolored"]
    check(checks, "red source recolors its own retained contribution toward green", green[1] > max(green[0], green[2]) * 1.5, rgb=green)
    check(checks, "blue source recolors its own retained contribution toward red", warm[0] > max(warm[1:]) * 1.5, rgb=warm)
    for name in ("mixed-restored", "mixed-excluded-off", "mixed-excluded-bright", "mixed-final"):
        check(checks, name + ": excluded source changes leave composed RGB exactly unchanged",
              energy[name] == both, reference=both, actual=energy[name])
    for name, reference in (("mixed-blue-still-on", blue), ("mixed-red-still-on", red)):
        check(checks, name + ": toggling the other source preserves this contribution", energy[name] == reference)
    excluded = [source for source in (snapshots.get("mixed-excluded-bright") or {}).get("sources", [])
                if source.get("id") == "excluded_green"]
    check(checks, "bright excluded fluorescent remains ineligible with no receiver factors", bool(excluded)
          and all(source.get("eligible") is False and source.get("factor_entries") == 0
                  and source.get("intensity") == 8 for source in excluded))


def audit_blocker(samples: dict, snapshots: dict, checks: list) -> None:
    clear = receiver_energy(snapshots.get("blocker-clear"))
    blocked = receiver_energy(snapshots.get("blocker-blocked"))
    returned = receiver_energy(snapshots.get("blocker-returned"))
    valid = all(len(energy) == 3 for energy in (clear, blocked, returned))
    check(checks, "moving blocker snapshots expose finite composed receiver RGB", valid)
    if valid:
        check(checks, "moving the actual crate under the source reduces composed focused energy",
              0 < sum(blocked) < sum(clear) * 0.98, clear=clear, blocked=blocked)
        check(checks, "returning the actual crate restores its perimeter-shadow energy", all(
            abs(a - b) <= max(1, a) * 0.05 for a, b in zip(clear, returned)), before=clear, returned=returned)
    before, after = snapshots.get("blocker-clear") or {}, snapshots.get("blocker-blocked") or {}
    check(checks, "actual movable geometry advances visibility revision and traces its new pose",
          before.get("dynamic_revision") != after.get("dynamic_revision")
          and after.get("dynamic_visibility_queries", 0) > before.get("dynamic_visibility_queries", 0))
    for name, expected in (("blocker-blocked", [2, -1, 2]), ("blocker-returned", [3.2, -1, 3.2])):
        physics = samples.get(name, {}).get("physics", {})
        ids = {entry[0]: entry[1] for entry in physics.get("authored", [])}
        bodies = [body for body in physics.get("bodies", []) if ids.get(body.get("key")) == "moving_blocker"]
        position = native.body_position(bodies[0]) if bodies else []
        check(checks, name + ": real physics body reaches the requested supported pose", bool(position)
              and native.distance(position, expected) < 0.04, actual_position=position, expected_position=expected)


def pixel_audit(out: Path, case: dict, checks: list) -> dict:
    """Display-space response witness, deliberately separate from linear RGB claims."""
    if case.get("pool_source_groups"):
        return pixel_audit_pool_sources(out, checks)
    try:
        import numpy as np
        from PIL import Image
    except ImportError:
        return dict(status="unavailable", reason="Pillow and NumPy are needed for display-space pixel evidence")
    names = ["frozen", "source-off", "water-tint", "water-disabled", "animation-a", "animation-b", "held-a", "held-b"]
    if case["source_controls"]:
        names += ["source-dim", "source-warm"]
    if case["id"] == "caustics_mixed":
        names += ["mixed-both", "mixed-red-only", "mixed-blue-only", "mixed-excluded-off", "mixed-excluded-bright"]
    if case.get("moving_blocker"):
        names += ["blocker-blocked-restored", "blocker-blocked-no-focus", "blocker-clear-restored", "blocker-clear-no-focus"]
    images = {name: np.asarray(Image.open(out / f"{name}.png").convert("RGB"), dtype=np.int16) for name in names}
    shape = images["frozen"].shape
    # Ignore the upper scene/sky and a 6% frame border. The compact control's
    # receivers dominate this stable crop; manual review covers exact boundaries.
    region = (slice(int(shape[0] * 0.35), int(shape[0] * 0.92)), slice(int(shape[1] * 0.06), int(shape[1] * 0.94)))
    def difference(first: str, second: str) -> dict:
        delta = np.abs(images[first][region] - images[second][region])
        return dict(mean_rgb=[float(value) for value in delta.mean(axis=(0, 1))],
                    changed_pixels=int((delta.max(axis=2) > 1).sum()), peak=int(delta.max()))
    comparisons = {"focused_vs_disabled": difference("frozen", "water-disabled"),
                   "off_vs_disabled": difference("source-off", "water-disabled"),
                   "animation": difference("animation-a", "animation-b"),
                   "held": difference("held-a", "held-b"),
                   "tint_vs_disabled": difference("water-tint", "water-disabled")}
    visible = comparisons["focused_vs_disabled"]["changed_pixels"]
    check(checks, "eligible source visibly focuses; excluded source does not", visible > 32 if case["eligible"] else visible == 0,
          expected_eligible=case["eligible"], **comparisons["focused_vs_disabled"])
    check(checks, "source off and zero water strength produce the same receiver pixels",
          comparisons["off_vs_disabled"]["changed_pixels"] == 0, **comparisons["off_vs_disabled"])
    check(checks, "zero speed holds the native receiver pattern", comparisons["held"]["changed_pixels"] == 0, **comparisons["held"])
    if case["eligible"]:
        check(checks, "speed 1.35 changes the actual receiver pattern", comparisons["animation"]["changed_pixels"] > 32, **comparisons["animation"])
    if case["source_controls"] and case["eligible"]:
        comparisons["dim_vs_disabled"] = difference("source-dim", "water-disabled")
        comparisons["warm_vs_disabled"] = difference("source-warm", "water-disabled")
        full = sum(comparisons["focused_vs_disabled"]["mean_rgb"])
        dim = sum(comparisons["dim_vs_disabled"]["mean_rgb"])
        check(checks, "reduced source intensity visibly reduces the focused contribution", 0 < dim < full,
              full_display_delta=full, dim_display_delta=dim)
        warm = comparisons["warm_vs_disabled"]["mean_rgb"]
        tint = comparisons["tint_vs_disabled"]["mean_rgb"]
        check(checks, "source colour changes focused light toward red", warm[0] > max(warm[1:]) * 1.5, mean_absolute_display_delta_rgb=warm)
        check(checks, "water tint changes focused light toward green", tint[1] > max(tint[0], tint[2]) * 1.5, mean_absolute_display_delta_rgb=tint)
    if case["id"] == "caustics_mixed":
        for name in ("mixed-excluded-off", "mixed-excluded-bright"):
            comparisons[name] = difference("mixed-both", name)
            check(checks, name + ": excluded mutations leave native receiver pixels unchanged",
                  comparisons[name]["changed_pixels"] == 0, **comparisons[name])
        comparisons["independent_sources"] = difference("mixed-red-only", "mixed-blue-only")
        check(checks, "retaining each eligible source independently changes native receiver pixels",
              comparisons["independent_sources"]["changed_pixels"] > 32, **comparisons["independent_sources"])
    if case.get("moving_blocker"):
        comparisons["blocker_focused"] = difference("blocker-blocked-restored", "blocker-blocked-no-focus")
        comparisons["blocker_clear"] = difference("blocker-clear-restored", "blocker-clear-no-focus")
        check(checks, "same-pose captures witness focused light on both blocker placements",
              comparisons["blocker_focused"]["changed_pixels"] > 32
              and comparisons["blocker_clear"]["changed_pixels"] > 32,
              blocked=comparisons["blocker_focused"], clear=comparisons["blocker_clear"])
    return dict(status="measured", crop="x=6..94%, y=35..92%; absolute display-encoded RGB deltas", comparisons=comparisons)


def pixel_audit_pool_sources(out: Path, checks: list) -> dict:
    try:
        import numpy as np
        from PIL import Image
    except ImportError:
        return dict(status="unavailable", reason="Pillow and NumPy are needed for real-Pool pixel evidence")
    comparisons = {}
    for name in POOL_SOURCE_SCENARIOS:
        focus = np.asarray(Image.open(out / f"{name}.png").convert("RGB"), dtype=np.int16)
        disabled = np.asarray(Image.open(out / f"{name}-no-focus.png").convert("RGB"), dtype=np.int16)
        height, width = focus.shape[:2]
        region = (slice(int(height * 0.15), int(height * 0.95)), slice(int(width * 0.08), int(width * 0.92)))
        delta = np.abs(focus[region] - disabled[region])
        comparisons[name] = dict(changed_pixels=int((delta.max(axis=2) > 1).sum()),
                                 mean_rgb=[float(value) for value in delta.mean(axis=(0, 1))], peak=int(delta.max()))
        eligible = name in ("pool-only", "sun-only", "mixed-all")
        check(checks, name + ": real Pool PNG pair visibly follows focused-source eligibility",
              comparisons[name]["changed_pixels"] > 32 if eligible else comparisons[name]["changed_pixels"] == 0,
              **comparisons[name])
    return dict(status="measured", crop="x=8..92%, y=15..95%; paired absolute display-encoded RGB deltas",
                comparisons=comparisons,
                scope="Same fixed real-Pool camera/geometry and baked base lighting; only focused-source selection and strength change")


def video_command(args: argparse.Namespace, out: Path) -> list[str]:
    return [str(args.ffmpeg.resolve(strict=True)), "-hide_banner", "-nostdin", "-n", "-f", "rawvideo",
            "-pixel_format", "rgba", "-video_size", "1280x720", "-framerate", "30",
            "-i", str(out / "native-video.fifo"), "-an", "-c:v", "libx264", "-preset", "veryfast",
            "-crf", "18", "-threads", "2", "-vf",
            "scale=in_range=pc:out_range=tv:out_color_matrix=bt709,format=yuv420p,setparams=range=limited:color_primaries=bt709:color_trc=bt709:colorspace=bt709",
            "-colorspace", "bt709", "-color_primaries", "bt709", "-color_trc", "bt709", "-color_range", "tv",
            "-movflags", "+faststart", str(out / "caustics-native.mp4")]


def audit_video(args: argparse.Namespace, out: Path, events: list, checks: list) -> dict:
    frames = [json.loads(event["detail"]) for event in events if event["event"] == "native_video_frame"]
    source_frames = [frame["source_ready_frame"] for frame in frames]
    stride = args.simulation_hz // 30
    check(checks, "video uses consecutive native ready frames at the requested simulation cadence",
          len(frames) == args.video_frames
          and [frame["captured"] for frame in frames] == list(range(1, args.video_frames + 1))
          and bool(source_frames) and source_frames[0] == args.simulation_hz + 1
          and all(b - a == stride for a, b in zip(source_frames, source_frames[1:])),
          native_frames=len(frames), ready_frame_stride=stride, fixed_delta_seconds=1 / args.simulation_hz)
    path = out / "caustics-native.mp4"
    command = [str(args.ffprobe.resolve(strict=True)), "-v", "error", "-select_streams", "v:0", "-show_entries",
               "stream=codec_name,pix_fmt,width,height,nb_frames,avg_frame_rate,color_space,color_transfer,color_primaries,color_range:format=duration,size",
               "-of", "json", str(path)]
    result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, timeout=15)
    (out / "video-probe.log").write_text(result.stdout + result.stderr)
    if result.returncode:
        raise ValueError("ffprobe could not validate the native clip")
    metadata = json.loads(result.stdout)
    stream = metadata.get("streams", [{}])[0]
    check(checks, "clip preserves 1280x720 native frames at 30 fps", stream.get("codec_name") == "h264"
          and stream.get("pix_fmt") == "yuv420p" and [stream.get("width"), stream.get("height")] == [1280, 720]
          and int(stream.get("nb_frames", 0)) == args.video_frames and stream.get("avg_frame_rate") == "30/1"
          and abs(float(metadata.get("format", {}).get("duration", 0)) - args.video_frames / 30) < 0.05)
    check(checks, "clip declares actual BT.709 limited-range conversion", all(stream.get(key) == "bt709"
          for key in ("color_space", "color_transfer", "color_primaries")) and stream.get("color_range") == "tv")
    return dict(path=str(path), sha256=digest(path), metadata=metadata,
                source_simulation_hz=args.simulation_hz, simulation_seconds_per_output_frame=1 / 30,
                requested_speed=args.speed if not args.legacy else None,
                legacy_default_speed=args.legacy, synthesized_or_interpolated_frames=0)


def run(args: argparse.Namespace, case: dict, package: Path, binary: Path, out: Path) -> dict:
    out.mkdir(parents=True, exist_ok=False)
    (out / "levels").mkdir()
    installed = out / "levels" / f"{case['id']}.placesmap"
    shutil.copyfile(package, installed)
    assets = out / "assets"
    assets.mkdir()
    for entry in sorted((args.asset_root / "assets").iterdir()):
        if entry.name != "levels":
            (assets / entry.name).symlink_to(entry.resolve(strict=True), target_is_directory=entry.is_dir())
    settings = json.loads((ROOT / "tests/fixtures/native/retro-arcade-manifest.json").read_text())["settings"]
    width, height = map(int, args.size.split("x"))
    settings.update(quality="high", lightmaps=args.lightmaps, texture_filtering=args.texture_filtering, reflections="off",
                    bloom=False, vsync=True, use_low_quality_lighting=False, developer_console=False,
                    fov_degrees=60, window_mode="windowed", window_width=width, window_height=height)
    actions = plan(case, args.run, args.animation, args.speed, args.legacy)
    write_json(out / "settings.json", settings)
    write_json(out / "actions.json", actions)
    environment = {key: value for key, value in os.environ.items()
                   if not key.startswith("PLACES_") and key != "SDL_VIDEODRIVER"}
    environment.update(PLACES_BENCH="1", PLACES_VERBOSE="1", PLACES_LEVEL=case["id"],
                       PLACES_ASSET_ROOT=str(out), PLACES_STATE_ROOT=str(out),
                       PLACES_BENCH_ACTIONS=str(out / "actions.json"), PLACES_LOAD_TRACE=str(out / "trace.jsonl"),
                       PLACES_BENCH_WARMUP="0", PLACES_BENCH_FRAMES="3600",
                       PLACES_BENCH_FIXED_DELTA_SECONDS=str(1 / args.simulation_hz),
                       PLACES_BENCH_CAMERA_POSITION=",".join(map(str, case["eye"])),
                       PLACES_CAMERA=",".join(map(str, case["camera"])))
    if args.run == "pool" and not args.animation:
        # Timed captures and player actions share the wall-clock anchor. A
        # frame sequence can outrun these on a high-refresh display.
        del environment["PLACES_BENCH_CAMERA_POSITION"]
        del environment["PLACES_CAMERA"]
    if args.run == "pool":
        environment["PLACES_SPAWN"] = ",".join(map(str, [*case["eye"], case["camera"][0]]))
    encoder_command = None
    if args.animation:
        environment.update(PLACES_BENCH_VIDEO_PIPE=str(out / "native-video.fifo"), PLACES_BENCH_VIDEO_SIZE="1280x720",
                           PLACES_BENCH_VIDEO_FRAMES=str(args.video_frames),
                           PLACES_BENCH_VIDEO_STRIDE=str(args.simulation_hz // 30),
                           PLACES_BENCH_VIDEO_START_FRAME=str(args.simulation_hz))
        encoder_command = video_command(args, out)
    receipt = dict(case=args.run, status="planned", command=[str(binary)], binary_sha256=digest(binary),
                   package=str(package), package_sha256=digest(package), source=case["source"],
                   source_sha256=digest(Path(case["source"])), settings=settings,
                   environment={key: value for key, value in environment.items() if key.startswith("PLACES_")},
                   checks=[], screenshots=[], visual_review=case["visual_review"],
                   shadow_review=case.get("shadow_review"),
                   receiver_review=case.get("receiver_review", []),
                   video=dict(status="planned", encoder_command=encoder_command) if encoder_command else None,
                   limitations=["Source radiance controls change focused contribution; prepared base light stays baked",
                                "Pixel checks witness display response; manual receiver/occlusion review remains required",
                                "V-sync capture campaign; no FPS performance claim"])
    if case.get("pool_source_groups"):
        receipt.update(pool_source_groups=case["pool_source_groups"], original_source=case["original_source"],
                       original_source_sha256=case["original_sha256"], source_variant_changes=case["source_variant_changes"])
    write_json(out / "receipt.json", receipt)
    if args.plan_only:
        return receipt
    started = time.monotonic()
    receipt["started_utc"] = datetime.now(timezone.utc).isoformat()
    encoder, encoder_log = None, None
    try:
        if encoder_command:
            os.mkfifo(out / "native-video.fifo", mode=0o600)
            encoder_log = (out / "video-encoder.log").open("wb")
            encoder = subprocess.Popen(encoder_command, cwd=ROOT, stdin=subprocess.DEVNULL,
                                       stdout=encoder_log, stderr=subprocess.STDOUT)
        result = subprocess.run([str(binary)], cwd=ROOT, env=environment, capture_output=True, text=True, timeout=90)
        log = result.stdout + result.stderr
        (out / "native.log").write_text(log)
        receipt.update(exit_code=result.returncode, elapsed_seconds=time.monotonic() - started)
        checks = receipt["checks"]
        check(checks, "direct native process exits successfully", result.returncode == 0)
        filters = [json.loads(line.removeprefix("[visual-diagnostic] ")).get("applied", {}).get("filtering")
                   for line in log.splitlines() if line.startswith("[visual-diagnostic] ")]
        check(checks, "initial installed sampler matches its independent authored setting",
              bool(filters) and filters[0] == args.texture_filtering,
              requested=args.texture_filtering, installed=sorted(set(filters)))
        check(checks, "native world commits without validation/device errors", f"[loading] committed {case['id']}" in log
              and not any(term in log.lower() for term in ("validation error", "panicked", "fatal device", "catalog hash mismatch")))
        check(checks, "binary and original/installed package identities stay unchanged", digest(binary) == receipt["binary_sha256"]
              and digest(package) == digest(installed) == receipt["package_sha256"])
        identities = re.findall(r"^\[loading\] package identity level=\S+ variant=(\S+) sha256=([0-9a-f]{64}) source=(.+)$", log, re.MULTILINE)
        receipt["loaded_package_identity"] = [dict(variant=v, variant_sha256=h, source=p) for v, h, p in identities]
        check(checks, "native discovery loads the isolated selected package", bool(identities)
              and all(Path(path).resolve() == installed for _, _, path in identities))
        receipt["native_identity"] = [line for line in log.splitlines() if line.startswith(("[renderer]", "[window]", "[settings]", "[caustics]"))]
        trace = out / "trace.jsonl"
        if trace.stat().st_size > TRACE_LIMIT:
            raise ValueError("native trace exceeds the 64 MiB reader budget")
        events = [json.loads(line) for line in trace.read_text().splitlines()]
        samples, seen = native.snapshot_samples(events, actions)
        check(checks, "every bounded SDL action executes", seen == set(range(len(actions))))
        presented = sum(event["event"] == "present" and event["detail"] == "ready" for event in events)
        missed = sum(event["event"] == "surface_not_presented" and event["detail"] == "ready" for event in events)
        check(checks, "native receiver scene actually presents", presented > 0, presented_ready=presented, missed_ready=missed)
        for step in actions:
            action = step["action"]
            if action["kind"] == "capture":
                path = out / f"{action['name']}.png"
                header = path.read_bytes()[:24] if path.is_file() else b""
                size = [int.from_bytes(header[16:20], "big"), int.from_bytes(header[20:24], "big")] if len(header) == 24 and header[:8] == b"\x89PNG\r\n\x1a\n" else None
                expected = list(map(int, args.expected_drawable.split("x"))) if args.expected_drawable else None
                check(checks, action["name"] + ": actual native PNG exists at the physical drawable", bool(size) and (expected is None or size == expected), size=size)
                if size:
                    receipt["screenshots"].append(dict(name=action["name"], path=str(path), size=size, sha256=digest(path)))
        receipt["caustics_snapshots"] = {name: sample.get("caustics") for name, sample in samples.items()}
        if encoder:
            check(checks, "native video encoder exits successfully", encoder.wait(timeout=15) == 0)
            receipt["video"] = audit_video(args, out, events, checks)
        if not args.animation:
            audit_diagnostics(samples, case, checks, args.lightmaps)
        if not args.animation and args.run != "pool" and all(item["passed"] for item in checks):
            receipt["pixels"] = pixel_audit(out, case, checks)
        receipt["status"] = "passed" if all(item["passed"] for item in checks) else "failed"
    except (OSError, ValueError, KeyError, subprocess.TimeoutExpired) as error:
        receipt.update(status="failed", error=str(error), elapsed_seconds=time.monotonic() - started)
    finally:
        if encoder and encoder.poll() is None:
            encoder.terminate()
            try:
                encoder.wait(timeout=5)
            except subprocess.TimeoutExpired:
                encoder.kill()
                encoder.wait(timeout=5)
        if encoder_log:
            encoder_log.close()
        (out / "native-video.fifo").unlink(missing_ok=True)
    write_json(out / "receipt.json", receipt)
    return receipt


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--author", type=Path, help="write disposable fixture JSON and compiler command manifest")
    mode.add_argument("--run", help="run one named case from the authored manifest")
    parser.add_argument("--fixtures", type=Path, default=ROOT / "local/caustics-0.11.0/controls")
    parser.add_argument("--compiler", type=Path, default=ROOT / "target/release/places-compile")
    parser.add_argument("--binary", type=Path, default=ROOT / "target/release/places")
    parser.add_argument("--package", type=Path, help="override this case's explicitly built package")
    parser.add_argument("--asset-root", type=Path, default=ROOT, help="root containing the matching assets directory")
    parser.add_argument("--out", type=Path, help="new native evidence directory, never reused")
    parser.add_argument("--size", default="640x360", help="logical native window dimensions")
    parser.add_argument("--expected-drawable", help="require actual PNG drawable WIDTHxHEIGHT")
    parser.add_argument("--lightmaps", choices=("full", "static"), default="full", help="initial lighting mode, independent of texture quality")
    parser.add_argument("--texture-filtering", choices=("low", "medium", "high"), default="high", help="independent native sampler setting")
    parser.add_argument("--plan-only", action="store_true", help="stage command/settings/actions without launching")
    parser.add_argument("--animation", action="store_true", help="capture a matched native 1280x720/30fps clip")
    parser.add_argument("--speed", type=float, default=1.35, help="animation speed control (0..8); compare 1.0 and 1.35")
    parser.add_argument("--simulation-hz", type=int, choices=(30, 60), default=60, help="fixed simulation cadence; output stays 30fps")
    parser.add_argument("--video-frames", type=int, default=120, help="bounded native frame count, 30..180")
    parser.add_argument("--ffmpeg", type=Path, default=Path(shutil.which("ffmpeg") or "/opt/homebrew/bin/ffmpeg"))
    parser.add_argument("--ffprobe", type=Path, default=Path(shutil.which("ffprobe") or "/opt/homebrew/bin/ffprobe"))
    parser.add_argument("--legacy", action="store_true", help="animation-only original Pool package; retain the preserved binary's speed")
    parser.add_argument("--production-pool", action="store_true", help="inspect the original Pool package/source instead of its clone")
    args = parser.parse_args()
    if args.author:
        print(json.dumps(author(args.author.resolve(), args.compiler.resolve()), indent=2))
        return 0
    if args.out is None:
        parser.error("--run requires a new --out directory")
    if not 0 <= args.speed <= 8 or not 30 <= args.video_frames <= 180:
        parser.error("speed must be finite in 0..8; video frames must be 30..180")
    if args.legacy and (not args.animation or args.run != "pool" or args.package is None):
        parser.error("--legacy requires --animation --run pool and its explicit original --package")
    if args.production_pool and (args.run != "pool" or args.package is None):
        parser.error("--production-pool requires --run pool and an explicit --package")
    if args.animation and args.expected_drawable != "1280x720":
        parser.error("--animation requires --expected-drawable 1280x720")
    for value in (args.size, args.expected_drawable):
        if value is not None and (not re.fullmatch(r"\d+x\d+", value)
                                  or not all(64 <= int(edge) <= 8192 for edge in value.split("x"))):
            parser.error("window and drawable dimensions must be WIDTHxHEIGHT within 64..8192")
    manifest = json.loads((args.fixtures / "manifest.json").read_text())
    if args.run not in manifest["cases"]:
        parser.error("unknown case; choose " + ", ".join(manifest["cases"]))
    case = manifest["cases"][args.run]
    if "expected_rejection" in case:
        parser.error("this invalid-source control must be rejected during compilation: " + case["expected_rejection"])
    if args.legacy or args.production_pool:
        original = json.loads(Path(case["original_source"]).read_text())
        counters, source_ids = {}, []
        for light in original["ceiling_lights"]:
            if light.get("id", "").strip():
                source_ids.append(light["id"].strip())
            else:
                short = light["fixture"].rsplit(":", 1)[-1]
                counters[short] = counters.get(short, 0) + 1
                source_ids.append(f"{short}_{counters[short]}")
        case = dict(case, id=original["id"], source=case["original_source"], source_ids=source_ids)
    package = (args.package or Path(case["package"])).resolve(strict=True)
    binary = args.binary.resolve(strict=True)
    if any(component.endswith(".app") for component in binary.parts) or not os.access(binary, os.X_OK):
        parser.error("use a directly executable native player outside an app bundle")
    args.asset_root = args.asset_root.resolve(strict=True)
    result = run(args, case, package, binary, args.out.resolve())
    print(json.dumps(dict(status=result["status"], case=args.run, receipt=str(args.out / "receipt.json"),
                          checks=len(result["checks"]), failed=[check["name"] for check in result["checks"] if not check["passed"]]), indent=2))
    return 0 if result["status"] in ("planned", "passed") else 1


if __name__ == "__main__":
    raise SystemExit(main())
