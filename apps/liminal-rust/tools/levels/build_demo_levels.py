#!/usr/bin/env python3
"""Generates the demo/showcase levels of the liminal-rust prop pack.

* ``levels/asset_demo.json`` — the walkable demo map: every catalogue asset
  (all twenty core props plus ``spooner-man``) placed in a small building made
  of four rooms around a corridor, with doorways, a passage, windows, a vent and
  ceiling lights. It uses the worn material set (stained wallpaper, damp
  carpet, stained ceiling) so it also shows the material variants Level 1 does
  not. It lives in the game's custom-level folder, so it appears in the level
  select menu.
* ``levels/asset_maintained.json`` — the same building on the maintained
  material set. A level carries exactly one wall/floor/ceiling material, so
  this is the only way to see both sets walkable side by side: switch between
  the two demos to compare the maintained and water-damaged surfaces.
* ``assets/levels/prop_showcase.json`` — the development fixture: every
  catalogue prop once, including deliberately sunk and overlapping placements
  (both legal in this game and never "corrected").
* ``assets/levels/prop_stress.json`` — a representative repeated-prop load
  (~150 placements across nine models) used to prove that instances reuse one
  decoded model, one texture and one draw call per model.

Run from ``apps/liminal-rust``:

    python3 tools/levels/build_demo_levels.py
"""

from __future__ import annotations

import json
import os
from typing import Dict, List

APP_ROOT = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", ".."))
LEVELS_DIR = os.path.join(APP_ROOT, "assets", "levels")
CUSTOM_LEVELS_DIR = os.path.join(APP_ROOT, "levels")

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
    # Deterministic ids keep the demo levels diffable and let the editor's 3D
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
        prop("core:fridge", 6.4, -4.9, solid=True),
        prop("core:washing_machine", 7.1, 0.6, rotation=270.0, solid=True),
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


def asset_demo_level() -> Dict:
    """The walkable demo map: every prop asset plus every structural feature.

    East of the main building is originally a lighting demonstration wing
    reached through the lobby's east wall: a spine corridor with spaced fixtures
    (local light pools), a row of four equal rooms with 0/1/2/4 fixtures (light
    density), two equal rooms with weak/strong fixtures (intensity), two equal
    rooms at 2.6 m/4.2 m ceiling heights (height effect), and a bright room
    facing a dark one through a wide doorway (opening bleed). Three props in the
    two-fixture room sit under a fixture, between fixtures and in a dark corner,
    and one more spooner-man stands in the bright room near the doorway.
    """
    living = {"x": -14.0, "z": -12.0, "width": 10.0, "depth": 10.0, "height": WALL_HEIGHT}
    kitchen = {"x": -4.0, "z": -12.0, "width": 10.0, "depth": 10.0, "height": WALL_HEIGHT}
    bedroom = {"x": -14.0, "z": 2.0, "width": 10.0, "depth": 10.0, "height": WALL_HEIGHT}
    lobby = {"x": -4.0, "z": 2.0, "width": 10.0, "depth": 10.0, "height": WALL_HEIGHT}
    corridor = {"x": -14.0, "z": -2.0, "width": 20.0, "depth": 4.0, "height": 2.6}

    def doorway(offset, width=1.2, height=2.1):
        return {"kind": "door", "offset": offset, "width": width, "height": height, "sill": 0.0}

    def window(offset, width=2.0, height=1.2, sill=1.0):
        return {"kind": "window", "offset": offset, "width": width, "height": height, "sill": sill}

    walls = [
        # Outer shell, with windows on the long north and south faces. The east
        # wall gets a doorway into the lighting wing corridor.
        wall(-14.4, -12.4, 20.8, WALL_THICKNESS, openings=[window(7.6), window(14.0)]),
        wall(-14.4, 12.0, 20.8, WALL_THICKNESS, openings=[window(9.2)]),
        wall(-14.4, -12.0, WALL_THICKNESS, 24.0),
        wall(6.0, -12.0, WALL_THICKNESS, 24.0, openings=[doorway(18.4)]),
        # Corridor walls: a door into each room, plus a vent grille.
        wall(-14.0, -2.4, 20.0, WALL_THICKNESS, openings=[doorway(6.0), doorway(14.0)]),
        wall(-14.0, 2.0, 20.0, WALL_THICKNESS,
             openings=[doorway(6.0), doorway(14.0),
                       {"kind": "vent", "offset": 2.0, "width": 0.8, "height": 0.6, "sill": 2.0}]),
        # Divider between the two north rooms, with a wide passage.
        wall(-4.2, -12.0, WALL_THICKNESS, 9.6,
             openings=[{"kind": "passage", "offset": 4.0, "width": 1.6, "height": 2.2, "sill": 0.0}]),
        # Divider between the two south rooms (they connect through the corridor).
        wall(-4.2, 2.4, WALL_THICKNESS, 9.6),
    ]

    props = [
        # --- living room -----------------------------------------------------
        prop("core:couch", -9.0, -11.4, rotation=0.0, solid=True),
        prop("core:armchair", -11.6, -8.4, rotation=135.0, solid=True),
        prop("core:rug", -9.0, -7.0),
        prop("core:table", -9.0, -7.0, rotation=0.0, solid=True),
        prop("core:tv", -9.0, -2.5, rotation=180.0),
        prop("core:chair", -6.2, -8.6, rotation=205.0, solid=True),
        prop("core:lamp", -13.2, -10.6),
        prop("core:bookshelf", -4.45, -5.4, rotation=270.0, solid=True),
        prop("core:cabinet", -13.75, -7.0, rotation=90.0, solid=True),
        prop("core:plant", -13.0, -2.9),
        prop("spooner-man", -8.2, -6.2, rotation=35.0),
        # --- kitchen / utility ----------------------------------------------
        prop("core:sink", 0.6, -11.4, rotation=0.0, solid=True),
        prop("core:stove", -1.2, -11.4, rotation=0.0, solid=True),
        prop("core:fridge", 2.6, -11.3, rotation=0.0, solid=True),
        prop("core:washing_machine", 4.3, -11.3, rotation=0.0, solid=True),
        prop("core:water_cooler", 5.6, -9.0, solid=True),
        prop("core:crate", 0.0, -4.0, rotation=12.0, solid=True),
        prop("core:crate", 0.64, -4.08, rotation=28.0, solid=True),
        prop("core:crate", 0.3, -4.7, rotation=5.0, solid=True),
        # a prop standing on another prop: the vertical offset is supported by
        # the ordinary prop system and is never "corrected".
        prop("core:plant", 0.3, -4.7, y=0.6),
        prop("core:cardboard_box", 2.3, -4.2, rotation=20.0),
        prop("core:cardboard_box", 2.62, -4.62, rotation=5.0),
        prop("core:chair", 4.6, -6.6, rotation=250.0, solid=True),
        # --- bedroom / office ------------------------------------------------
        prop("core:bed", -11.6, 10.9, rotation=0.0, solid=True),
        prop("spooner-man", -11.7, 10.3, y=0.44, rotation=255.0),
        prop("core:desk", -4.6, 5.2, rotation=270.0, solid=True),
        prop("core:chair", -6.1, 5.2, rotation=90.0, solid=True),
        prop("core:chair", -7.2, 8.2, rotation=300.0, solid=True),
        prop("core:cabinet", -13.6, 2.7, rotation=0.0, solid=True),
        prop("core:bookshelf", -8.0, 11.8, rotation=180.0, solid=True),
        prop("core:lamp", -13.4, 8.6),
        prop("core:plant", -13.2, 11.3),
        prop("core:plant", -5.0, 11.3),
        prop("core:rug", -8.4, 7.6),
        # --- lobby / waiting area -------------------------------------------
        prop("core:vending_machine", 5.57, 5.0, rotation=270.0, solid=True),
        prop("core:water_cooler", 5.5, 8.6, solid=True),
        prop("core:chair", 0.0, 5.4, rotation=0.0, solid=True),
        prop("core:chair", 1.2, 5.4, rotation=0.0, solid=True),
        prop("core:chair", 2.4, 5.4, rotation=0.0, solid=True),
        prop("core:crate", 3.0, 10.5, rotation=8.0, solid=True),
        prop("core:crate", 3.66, 10.6, rotation=32.0, solid=True),
        prop("core:cardboard_box", 0.4, 10.6, rotation=15.0),
        prop("core:cardboard_box", -0.3, 10.35, rotation=40.0),
        prop("core:tv", -3.2, 11.9, rotation=180.0),
        prop("core:desk", -3.2, 11.6, rotation=180.0, solid=True),
        prop("core:chair", -3.2, 10.6, rotation=0.0, solid=True),
        prop("core:plant", 5.3, 11.2),
        # --- corridor --------------------------------------------------------
        prop("core:chair", -0.6, 0.0, rotation=90.0, solid=True),
        prop("core:crate", 3.6, 1.2, rotation=25.0, solid=True),
        prop("core:cardboard_box", 4.15, -0.9, rotation=60.0),
        prop("core:plant", -13.1, -1.2),
        prop("spooner-man", -4.4, 0.5, rotation=88.0),
    ]

    # All twelve fixtures of the original building stay at the standard output,
    # which keeps the demo map a live proof that levels without an intensity
    # field behave as 1.0.
    lights = [
        light(-11.0, -7.0),
        light(-7.0, -10.0),
        light(-1.5, -6.0),
        light(2.5, -10.0),
        light(-11.0, 7.0),
        light(-7.0, 10.0),
        light(-1.5, 6.0),
        light(3.0, 10.0),
        light(-12.0, 0.0, rotation=90.0),
        light(-7.0, 0.0, rotation=90.0),
        light(-1.0, 0.0, rotation=90.0),
        light(4.0, 0.0, rotation=90.0),
    ]

    wing_rooms, wing_walls, wing_lights, wing_props = lighting_wing(doorway)
    rooms = [living, kitchen, bedroom, lobby, corridor] + wing_rooms
    walls = walls + wing_walls
    lights = lights + wing_lights
    props = props + wing_props

    return {
        "format_version": 1,
        "id": "asset_demo",
        "name": "Asset Demo (Water Damage)",
        "author": "Liminal Team",
        # The worn material set, so the demo also shows the variants Level 1
        # does not use (stained wallpaper, damp carpet, stained ceiling).
        "defaults": {
            "wall": "core:wallpaper_stained_01",
            "floor": "core:carpet_damp_01",
            "ceiling": "core:ceiling_stained_01",
        },
        "spawn": {"x": -13.0, "z": 0.0, "yaw_degrees": 90.0},
        "rooms": rooms,
        "walls": walls,
        "ceiling_lights": lights,
        "props": props,
    }


def maintained_demo_level() -> Dict:
    """The Asset Demo building on the maintained material set.

    A level carries one wall, one floor and one ceiling material, so the two
    faces of the material comparison (maintained vs water damaged) have to be
    two levels. This is the same building as ``asset_demo`` with the three
    maintained defaults, which is exactly what makes the difference between
    the sets obvious: walk the same rooms twice and only the surfaces change.
    """
    level = asset_demo_level()
    level["id"] = "asset_maintained"
    level["name"] = "Asset Demo (Maintained)"
    level["defaults"] = dict(DEFAULTS)
    return level


def lighting_wing(doorway) -> tuple:
    """The lighting demonstration wing east of the main building.

    Layout (metres): one long spine corridor is x 6.4..40.4, z 5.2..8.8. Its
    first 16 m carry the eight comparison rooms (four north at z 9.2..13.2, four
    south at z 0.8..4.8, each with its own doorway); its last 18 m are deliberately
    door-free and dim so two widely spaced fixtures read as bright pools with a
    genuinely darker gap between them. Two 6x6 m rooms hang off the east end: a
    bright one (four fixtures) and a dark one (no fixtures) joined by a wide
    doorway, so the player can look from the dark room into the light.
    """
    rooms = []
    walls = []
    lights = []
    props = []

    # --- spine corridor -----------------------------------------------------
    # The comparison rooms' doorways light the western gallery; the eastern
    # stretch has no doors, so its two fixtures 12 m apart (their 6 m pools just
    # meet in the middle) read as bright pool -> dim gap -> bright pool.
    corridor = {"x": 6.4, "z": 5.2, "width": 34.0, "depth": 3.6, "height": 2.6}
    rooms.append(corridor)
    for x in (25.0, 37.0):
        lights.append(light(x, 7.0))

    # Corridor walls with one doorway into every comparison room.
    door_offsets = (1.4, 5.4, 9.4, 13.4)
    walls.append(wall(6.4, 8.8, 34.0, WALL_THICKNESS,
                      openings=[doorway(offset) for offset in door_offsets]))
    walls.append(wall(6.4, 4.8, 34.0, WALL_THICKNESS, height=4.2,
                      openings=[doorway(offset) for offset in door_offsets]))
    # North/south outer arms, the room rows' east walls and the corridor's east
    # wall (with the doorway into the bright/dim pair). The lobby's east wall
    # (built by the caller) forms the wing's west boundary. The 4.2 m heights
    # close the tall comparison room; the extra height hides above the lower
    # ceilings.
    walls.append(wall(6.0, 12.0, WALL_THICKNESS, 1.6))
    walls.append(wall(6.4, 13.2, 16.0, WALL_THICKNESS))
    walls.append(wall(6.4, 0.4, 16.0, WALL_THICKNESS, height=4.2))
    walls.append(wall(22.4, 9.2, WALL_THICKNESS, 4.0))
    walls.append(wall(22.4, 0.8, WALL_THICKNESS, 4.0, height=4.2))
    walls.append(wall(40.4, -2.4, WALL_THICKNESS, 16.0, height=4.2,
                      openings=[{"kind": "door", "offset": 8.7, "width": 1.4, "height": 2.2, "sill": 0.0}]))

    # --- light density row: 0, 1, 2 and 4 fixtures in equal rooms ----------
    # Identical 4x4 m rooms at the same 3.0 m ceiling on the same materials:
    # walking east along the corridor the doorways read dim -> under-lit ->
    # normally lit -> bright.
    for index, (room_x, fixture_positions) in enumerate(
        [
            (6.4, []),
            (10.4, [(0.5, 0.5)]),
            (14.4, [(0.35, 0.5), (0.65, 0.5)]),
            (18.4, [(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)]),
        ]
    ):
        room_z = 9.2
        rooms.append({"x": room_x, "z": room_z, "width": 4.0, "depth": 4.0, "height": WALL_HEIGHT})
        if index > 0:
            walls.append(wall(room_x, room_z, WALL_THICKNESS, 4.0))
        for fx, fz in fixture_positions:
            lights.append(light(room_x + fx * 4.0, room_z + fz * 4.0))

    # --- fixture intensity pair --------------------------------------------
    # Same 4x4 m room, same single fixture: 0.5 on the left, 1.8 on the right.
    for room_x, intensity in ((6.4, 0.5), (10.4, 1.8)):
        rooms.append({"x": room_x, "z": 0.8, "width": 4.0, "depth": 4.0, "height": WALL_HEIGHT})
        lights.append(light(room_x + 2.0, 2.8, intensity=intensity))
    walls.append(wall(10.4, 0.8, WALL_THICKNESS, 4.0))

    # --- ceiling height pair ------------------------------------------------
    # Same area, same two fixtures, same intensity: 2.6 m on the left (fixtures
    # read higher and their pools tighter) and 4.2 m on the right.
    for room_x, height in ((14.4, 2.6), (18.4, 4.2)):
        rooms.append({"x": room_x, "z": 0.8, "width": 4.0, "depth": 4.0, "height": height})
        lights.append(light(room_x + 1.4, 2.8))
        lights.append(light(room_x + 2.6, 2.8))
    walls.append(wall(14.4, 0.8, WALL_THICKNESS, 4.0))
    # The tall room's walls must reach its 4.2 m ceiling or the gap above them
    # would be visible from inside.
    walls.append(wall(18.4, 0.8, WALL_THICKNESS, 4.0, height=4.2))

    # --- doorway bleed: bright room facing a dark room ----------------------
    bright = {"x": 40.8, "z": 4.0, "width": 6.0, "depth": 6.0, "height": WALL_HEIGHT}
    dark = {"x": 40.8, "z": -2.0, "width": 6.0, "depth": 6.0, "height": WALL_HEIGHT}
    rooms.extend([bright, dark])
    for fx, fz in ((0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)):
        lights.append(light(bright["x"] + fx * 6.0, bright["z"] + fz * 6.0))
    # The two rooms share a wall with a wide, floor-level doorway, so standing
    # in the dark room and looking north shows the light spilling through.
    walls.append(wall(40.8, 4.0, 6.0, WALL_THICKNESS,
                      openings=[{"kind": "door", "offset": 2.3, "width": 1.4, "height": 2.2, "sill": 0.0}]))
    walls.append(wall(40.8, 10.0, 6.0, WALL_THICKNESS))
    walls.append(wall(40.8, -2.4, 6.0, WALL_THICKNESS))
    walls.append(wall(46.8, -2.0, WALL_THICKNESS, 12.4))

    # --- prop lighting demonstration ---------------------------------------
    # Three props in the four-fixture room: directly beneath one fixture,
    # between fixtures, and in the darker corner. They also keep the environment
    # lighting on real prop geometry easy to demonstrate up close.
    props.extend(
        [
            prop("core:chair", 19.4, 10.2, rotation=180.0, solid=True),
            prop("core:crate", 20.4, 12.0, rotation=18.0, solid=True),
            prop("core:plant", 21.8, 12.8),
            # One extra spooner-man under the bright room's first fixture, so his
            # shading reads clearly against the doorway spill.
            prop("spooner-man", 42.3, 5.5, rotation=20.0),
        ]
    )
    return rooms, walls, lights, props


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
    os.makedirs(LEVELS_DIR, exist_ok=True)
    os.makedirs(CUSTOM_LEVELS_DIR, exist_ok=True)
    game_levels = [
        (asset_demo_level(), CUSTOM_LEVELS_DIR),
        (maintained_demo_level(), CUSTOM_LEVELS_DIR),
    ]
    dev_fixtures = [(showcase_level(), LEVELS_DIR), (stress_level(), LEVELS_DIR)]
    for level, directory in game_levels + dev_fixtures:
        assign_prop_ids(level)
        path = os.path.join(directory, f"{level['id']}.json")
        with open(path, "w", encoding="utf-8") as handle:
            json.dump(level, handle, indent=2)
            handle.write("\n")
        print(f"wrote {path} ({len(level['props'])} props)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
