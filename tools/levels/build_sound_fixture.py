#!/usr/bin/env python3
"""Build opt-in, two-room native audio acceptance/stress sources without baking."""
from __future__ import annotations

import argparse
from copy import deepcopy
import json
from pathlib import Path

from soundscapes import emitter, tap_mug

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / "tests/fixtures/native/developer-console.level.json"
OUTPUT = ROOT / "tests/fixtures/native/sound.level.json"
STRESS_OUTPUT = ROOT / "tests/fixtures/native/sound-stress.level.json"


def build_level():
    reference = json.loads(SOURCE.read_text())
    # Reuse the developer-console fixture's established pair of rooms and
    # common wall. Remove its disconnected diagnostic room and north vestibule.
    level = {key: deepcopy(reference[key]) for key in (
        "schema", "format_version", "defaults", "ceiling_lights")}
    level.update(id="sound_native", name="Native Sound Fixture", author="Places",
        comment="Explicit opt-in audio acceptance only; no CI or Rust test bakes this map.",
        spawn=dict(x=0.0, z=1.6, yaw_degrees=0.0),
        rooms=deepcopy(reference["rooms"][:2]), walls=deepcopy(reference["walls"][:7]))
    level["walls"][0].pop("openings", None)
    level["walls"][3]["openings"][0]["kind"] = "door"
    for light in level["ceiling_lights"]:
        light["y"] = 3.19
    level["ceiling_lights"][1]["brightness"] = .8
    door = deepcopy(reference["doors"][0])
    door.update(x=3.2, z=.6, rotation_degrees=90.0, width=1.2, height=2.4)
    level["doors"] = [door]
    props = {prop.get("id"): prop for prop in reference["props"]}
    level["props"] = [deepcopy(props[key]) for key in (
        "fixture_activate", "fixture_reset", "fixture_switch", "fixture_table")]
    activate, reset, switch, _table = level["props"]
    emitter(activate, "water_emitter", gain=.6, looped=True)
    emitter(reset, "ambience_hvac", gain=.6, looped=True, time_domain="real")
    activate["components"][0]["prompt"] = "Play world-time water loop"
    activate["bindings"] = [dict(on="interact", actions=[
        dict(action="play_sound", target="fixture_activate", loop=True)])]
    reset["components"][0]["prompt"] = "Stop both test loops"
    reset["bindings"] = [dict(on="interact", actions=[
        dict(action="stop_sound", target="fixture_activate"),
        dict(action="stop_sound", target="fixture_reset")])]
    emitter(switch, "prop_bell", gain=.7)
    switch["components"][0]["prompt"] = "Ring bell and toggle light"
    switch["bindings"][0]["cooldown_seconds"] = .6
    switch["bindings"][0]["actions"].append(dict(action="play_sound"))
    for identity, x in (("native_duck", 0.0), ("other_duck", .4)):
        duck = dict(id=identity, model="core:rubber_duck", display_name=identity,
            x=x, y=.75, z=-.65, scale=2.0, size=[.1, .12, .145], solid=False,
            occludes=False, physics=dict(motion="dynamic", pickup=True,
                reset_group="sound_fixture", mass_kg=.15, friction=.5,
                restitution=.05, linear_damping=.1, angular_damping=.2,
                shape=dict(kind="cuboid", half_extents=[.05, .06, .0725])))
        emitter(duck, "prop_duck", gain=.8)
        level["props"].append(duck)
    mug = dict(model="home:mug", x=-.4, y=.75, z=-.65, solid=False)
    tap_mug(mug, "native_mug")
    level["props"].append(mug)
    level["floor_patches"] = [dict(x=x, z=-2.7, width=width, depth=5.4, material=material)
        for x, width, material in ((3.3, 1.5, "core:carpet_beige_01"),
            (4.8, 2.0, "core:pool_tile_deck_01"), (6.8, 2.0, "outdoor:dirt_gravel_01"))]
    level["volumes"] = [
        dict(id="audio_aggregate_region", x=4.0, z=-.5, width=4.0, depth=3.0,
            bottom_y=0.0, top_y=2.5, filter=dict(kind="any"), occupancy="aggregate",
            bindings=[dict(on="enter_volume", actions=[
                dict(action="play_sound", target="fixture_reset", loop=True)]),
                dict(on="exit_volume", actions=[dict(action="stop_sound", target="fixture_reset")])]),
        dict(id="audio_exact_held_duck", x=-1.0, z=.1, width=2.0, depth=1.0,
            bottom_y=0.0, top_y=2.5,
            filter=dict(kind="item", asset="core:rubber_duck", instance="native_duck", held=True),
            occupancy="per_actor", bindings=[dict(on="enter_volume", cooldown_seconds=.8,
                actions=[dict(action="play_sound", target="native_duck")])]),
        dict(id="audio_duck_type_unheld", x=5.0, z=-2.7, width=3.5, depth=1.0,
            bottom_y=0.0, top_y=2.5, filter=dict(kind="item", asset="core:rubber_duck", held=False),
            occupancy="per_actor", bindings=[dict(on="enter_volume", cooldown_seconds=.6,
                actions=[dict(action="play_sound", target="fixture_switch")])]),
        dict(id="audio_thin_once", x=3.4, z=-.6, width=.03, depth=1.2,
            bottom_y=0.0, top_y=2.5, filter=dict(kind="player"), occupancy="per_actor",
            bindings=[dict(on="enter_volume", once=True,
                actions=[dict(action="play_sound", target="fixture_switch")])]),
    ]
    return level


def build_stress_level():
    level = build_level()
    level.update(id="sound_stress_native", name="Native Sound Saturation Fixture",
        comment="Explicit opt-in 60-loop saturation/lifecycle acceptance. Same two-room "
                "geometry as sound.level.json; no CI or Rust test bakes this map.")
    # The stress controls own every start/stop. Region crossings must not create
    # unrelated voices while measuring the voice cap and deferred cleanup.
    level["volumes"] = []
    targets = []
    for index in range(60):
        identity = f"sound_stress_{index:02d}"
        targets.append(identity)
        piece = dict(id=identity, model="outdoor:collision_peg",
            x=round(-2.3 + (index % 10) * .4, 3),
            z=round(-2.4 + (index // 10) * .08, 3), y=-.1,
            scale=.1, size=[.06, .06, .06], solid=False, occludes=False,
            comment="Buried 6 mm source carrier; no collision or visible geometry.")
        emitter(piece, "ambience_hvac", gain=.03, looped=True, playing=False)
        level["props"].append(piece)
    for control, action, prompt in (
            ("fixture_activate", "play_sound", "Start 60 quiet stress loops"),
            ("fixture_reset", "stop_sound", "Stop all 60 stress loops")):
        piece = next(prop for prop in level["props"] if prop.get("id") == control)
        # Per binding the established action bound is eight; all eight bindings
        # receive this one interaction, giving sixty starts/stops without raising it.
        piece["bindings"] = []
        for offset in range(0, len(targets), 8):
            actions = [dict(action=action, target=target) for target in targets[offset:offset + 8]]
            if action == "play_sound":
                for item in actions:
                    item["loop"] = True
            piece["bindings"].append(dict(on="interact", actions=actions))
        for component in piece["components"]:
            if component.get("component") == "interactable":
                component["prompt"] = prompt
    return level


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    for path, level in ((OUTPUT, build_level()), (STRESS_OUTPUT, build_stress_level())):
        expected = json.dumps(level, indent=2, ensure_ascii=False) + "\n"
        if args.check:
            if not path.is_file() or path.read_text() != expected:
                raise SystemExit(f"{path}: stale; run tools/levels/build_sound_fixture.py")
        else:
            path.write_text(expected)
        print(f"Native sound fixture current: {path.name}" if args.check else f"Wrote {path}")


if __name__ == "__main__":
    main()
