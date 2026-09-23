#!/usr/bin/env python3
"""Generates the engine regression fixture levels for Places.

The game ships one playable level, ``Places Demo``; the levels built here are
test fixtures and are deliberately kept out of the shipped content:

* ``tests/fixtures/levels/prop_showcase.json`` — every catalogue prop once,
  including deliberately sunk and overlapping placements (both legal in this
  game and never "corrected").
* ``tests/fixtures/levels/prop_stress.json`` — a representative repeated-prop
  load (~150 placements across nine models) used to prove that instances reuse
  one decoded model, one texture and one draw call per model.

Run from the Places repository root:

    python3 tools/levels/build_fixture_levels.py
"""

from __future__ import annotations

import json
import os
from typing import Dict, List

APP_ROOT = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", ".."))
FIXTURES_DIR = os.path.join(APP_ROOT, "tests", "fixtures", "levels")

DEFAULTS = {
    "wall": "core:wallpaper_yellow_01",
    "floor": "core:carpet_beige_01",
    "ceiling": "core:ceiling_panel_01",
}

WALL_HEIGHT = 3.0
WALL_THICKNESS = 0.4


def wall(x, z, width, depth, height=WALL_HEIGHT, openings=None) -> Dict:
    entry: Dict = {"x": x, "z": z, "width": width, "depth": depth, "height": height}
    if openings:
        entry["openings"] = openings
    return entry


def prop(model, x, z, rotation=0.0, y=0.0, scale=1.0, solid=None, size=None) -> Dict:
    entry: Dict = {"model": model, "x": x, "z": z}
    # Deterministic ids keep the fixture levels diffable and let the editor's 3D
    # viewport tag (and pick) each placement without inventing ids on load.
    if rotation:
        entry["rotation_degrees"] = rotation
    if y:
        entry["y"] = y
    if scale != 1.0:
        entry["scale"] = scale
    if solid is not None:
        entry["solid"] = solid
    if size is not None:
        entry["size"] = size
    return entry


def light(x, z, rotation=0.0, intensity=None) -> Dict:
    """Ceiling fixture entry. `intensity` is omitted for the standard 1.0 panel."""
    entry: Dict = {"fixture": "core:fluorescent_panel_01", "x": x, "z": z}
    if rotation:
        entry["rotation_degrees"] = rotation
    if intensity is not None:
        entry["intensity"] = intensity
    return entry


def room_box(room: Dict) -> List[Dict]:
    """Four walls around a room, with a doorway in the front (+Z) wall."""
    x, z, width, depth = room["x"], room["z"], room["width"], room["depth"]
    return [
        wall(x - WALL_THICKNESS, z - WALL_THICKNESS, width + 2 * WALL_THICKNESS, WALL_THICKNESS),
        wall(x - WALL_THICKNESS, z + depth, width + 2 * WALL_THICKNESS, WALL_THICKNESS),
        wall(x - WALL_THICKNESS, z, WALL_THICKNESS, depth),
        wall(x + width, z, WALL_THICKNESS, depth),
    ]


def showcase_level() -> Dict:
    # West room: domestic. East room: office / utility. A passage joins them.
    west = {"x": -12.0, "z": -6.0, "width": 10.0, "depth": 10.0, "height": WALL_HEIGHT}
    east = {"x": -2.0, "z": -6.0, "width": 10.0, "depth": 10.0, "height": WALL_HEIGHT}

    # One outer shell plus one dividing wall, so no two wall boxes overlap
    # (overlapping coplanar faces would z-fight in the renderer).
    x0, x1 = west["x"] - WALL_THICKNESS, east["x"] + east["width"]
    z0, z1 = west["z"], west["z"] + west["depth"]
    walls = [
        wall(x0, z0 - WALL_THICKNESS, (x1 - x0) + WALL_THICKNESS, WALL_THICKNESS),
        wall(x0, z1, (x1 - x0) + WALL_THICKNESS, WALL_THICKNESS),
        wall(x0, z0, WALL_THICKNESS, z1 - z0),
        wall(x1, z0, WALL_THICKNESS, z1 - z0),
        # Divider straddling the shared edge, with a walk-through passage.
        wall(
            -2.2,
            z0,
            WALL_THICKNESS,
            z1 - z0,
            openings=[
                {"kind": "passage", "offset": 4.0, "width": 1.4, "height": 2.2, "sill": 0.0}
            ],
        ),
    ]

    props = [
        # --- west room: living / domestic -----------------------------------
        prop("core:couch", -8.6, -4.4, rotation=0.0, solid=True),
        prop("core:armchair", -6.0, -4.6, rotation=214.0, solid=True),
        prop("core:rug", -7.6, -2.4),
        prop("core:table", -7.6, -2.4, rotation=0.0, solid=True),
        prop("core:tv", -7.6, -5.72, rotation=0.0),
        prop("core:lamp", -10.6, -4.6),
        prop("core:plant", -11.0, 2.6),
        prop("core:bookshelf", -3.1, -1.0, rotation=270.0, solid=True),
        # spooner-man: the only non-catalogue-namespaced prop id in the game.
        prop("spooner-man", -6.4, -1.2, rotation=126.0),
        # intentional clipping: a crate sunk into the floor and a box
        # overlapping it. Both are legal and must never be "fixed".
        prop("core:cardboard_box", -10.9, -5.4, rotation=24.0),
        prop("core:crate", -10.9, -5.4, y=-0.12, rotation=12.0, solid=True),
        # --- east room: office / kitchen / utility --------------------------
        prop("core:desk", 1.6, -4.6, solid=True),
        prop("core:chair", 1.6, -3.2, rotation=180.0, solid=True),
        prop("core:cabinet", 6.6, -5.6, solid=True),
        prop("core:bed", 4.6, 2.4, solid=True),
        prop("core:water_cooler", 2.6, 2.6, solid=True),
        prop("core:vending_machine", -1.6, -0.2, rotation=90.0, solid=True),
        prop("core:sink", 0.9, -5.7, solid=True),
        prop("core:stove", 3.3, -5.7, solid=True),
        # The Home kitchen run: a base cabinet flush with the sink and its
        # wall unit hung above it, backs against the north wall.
        prop("home:cabinet_base", 1.5, -5.7, solid=True),
        prop("home:cabinet_wall", 1.5, -5.835, y=1.45, solid=True),
        prop("core:fridge", 6.4, -4.9, solid=True),
        prop("core:washing_machine", 7.1, 0.6, rotation=270.0, solid=True),
        # The loose drum sits in front of the machine's door. In the shipped
        # demo the same model is the dynamic demonstrator (see
        # src/render/dynamic.rs); here it is an ordinary static placement, so
        # the shipping tests cover its geometry, budget and lighting.
        prop("core:washer_drum", 6.57, 0.6, rotation=270.0),
    ]

    return {
        "format_version": 1,
        "id": "prop_showcase",
        "name": "Prop Showcase (dev)",
        "author": "Liminal Team",
        "spawn": {"x": -7.6, "z": 1.6, "yaw_degrees": 0.0},
        "defaults": dict(DEFAULTS),
        "rooms": [west, east],
        "walls": walls,
        "ceiling_lights": [
            # Deliberately mixed fixture outputs, so the development fixture also
            # exercises the optional intensity field (0.8 low output, 1.4 high).
            light(-7.0, -1.0, intensity=0.8),
            light(3.0, -1.0, intensity=1.4),
        ],
        "props": props,
    }


def stress_level() -> Dict:
    room = {"x": -12.0, "z": -10.0, "width": 24.0, "depth": 20.0, "height": WALL_HEIGHT}
    walls = room_box(room)
    # A doorway so the stress level stays walkable end to end.
    walls[1] = wall(
        room["x"] - WALL_THICKNESS,
        room["z"] + room["depth"],
        room["width"] + 2 * WALL_THICKNESS,
        WALL_THICKNESS,
        openings=[{"kind": "door", "offset": 11.0, "width": 1.4, "height": 2.2, "sill": 0.0}],
    )

    props: List[Dict] = []
    # 60 chairs in six rows of ten: the classic "repeated asset" check.
    for row in range(6):
        for column in range(10):
            props.append(
                prop(
                    "core:chair",
                    -10.0 + column * 2.0,
                    -8.0 + row * 2.6,
                    rotation=90.0 * (row % 4),
                    solid=True,
                )
            )
    # 20 desks facing the chairs.
    for row in range(4):
        for column in range(5):
            props.append(
                prop("core:desk", -9.0 + column * 4.0, -6.7 + row * 5.2, solid=True)
            )
    # 24 crates in two stacks (overlapping placements are intentional).
    for index in range(24):
        props.append(
            prop(
                "core:crate",
                -11.0 + (index % 6) * 0.62,
                7.4 + (index // 6) * 0.62,
                y=-0.06 * (index % 3),
                rotation=8.0 * (index % 5),
                solid=True,
            )
        )
    # Appliances along the walls plus scattered decoration.
    for index in range(8):
        props.append(
            prop("core:vending_machine", -11.4 + index * 1.1, -9.5, rotation=0.0, solid=True)
        )
    for index in range(6):
        props.append(prop("core:washing_machine", -11.4 + index * 0.9, 9.4, solid=True))
    for index in range(6):
        props.append(prop("core:water_cooler", 10.6 - index * 0.7, -4.0, solid=True))
    for index in range(10):
        props.append(prop("core:lamp", -10.4 + index * 2.2, 4.4))
    for index in range(8):
        props.append(prop("core:rug", -9.0 + index * 2.5, 0.6, rotation=90.0))
    for index in range(4):
        props.append(prop("core:plant", -11.2 + index * 7.4, -2.0))
    for index in range(6):
        props.append(prop("core:cardboard_box", 9.6, -8.0 + index * 0.62, y=-0.02 * index))

    return {
        "format_version": 1,
        "id": "prop_stress",
        "name": "Prop Stress Test (dev)",
        "author": "Liminal Team",
        "spawn": {"x": -11.0, "z": 0.0, "yaw_degrees": 30.0},
        "defaults": dict(DEFAULTS),
        "room": room,
        "walls": walls,
        "ceiling_lights": [
            light(-6.0, -4.0),
            light(6.0, -4.0),
            light(-6.0, 6.0),
            light(6.0, 6.0),
        ],
        "props": props,
    }


def assign_prop_ids(level: Dict) -> None:
    counters: Dict[str, int] = {}
    for prop_entry in level["props"]:
        short = prop_entry["model"].split(":")[-1]
        counters[short] = counters.get(short, 0) + 1
        prop_entry["id"] = f"{short}_{counters[short]}"
        # Keep the id first so the files read like the editor writes them.
        ordered = {"id": prop_entry.pop("id")}
        ordered.update(prop_entry)
        prop_entry.clear()
        prop_entry.update(ordered)


def main() -> int:
    os.makedirs(FIXTURES_DIR, exist_ok=True)
    fixtures = [showcase_level(), stress_level()]
    for level in fixtures:
        assign_prop_ids(level)
        path = os.path.join(FIXTURES_DIR, f"{level['id']}.json")
        with open(path, "w", encoding="utf-8") as handle:
            json.dump(level, handle, indent=2)
            handle.write("\n")
        print(f"wrote {path} ({len(level['props'])} props)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
