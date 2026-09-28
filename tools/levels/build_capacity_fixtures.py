#!/usr/bin/env python3
"""Generates the capacity regression fixtures for Places.

The shipped game holds one playable level; the two levels built here are
deliberately extreme development fixtures that pin the *capacity* contract
rather than a design:

* ``capacity_sparse.json`` — the same tiny settlement (four enclosed rooms, a
  prop cluster, fixtures, a trigger, a route and a floating prop) repeated in
  each quadrant of a 4 km square, so geometry, collision, lighting, triggers and
  entity routes all run kilometres from the world origin. It is the regression
  for the raised world extent and for the collision index's large-coordinate
  behaviour.
* ``capacity_dense.json`` — one large hall holding every registered placeable
  model many times over (thousands of instances), hundreds of fixtures, a cast
  of animated entities with independent routes, decals, curved architecture and
  a pool basin. It is the regression for the raised instance, prop-model,
  prop-vertex, character and lightmap-page budgets, and the frame-time evidence
  for the collision index.

Both are deterministic: a fixed seed drives every pseudo-random choice, ids are
stable per placement, and re-running the generator produces byte-identical
files. ``--check`` re-derives both documents and compares them against the
committed fixtures without writing.

Run from the Places repository root:

    python3 tools/levels/build_capacity_fixtures.py
    python3 tools/levels/build_capacity_fixtures.py --check
"""

from __future__ import annotations

import argparse
import json
import math
import os
import sys
from typing import Dict, List, Optional, Tuple

APP_ROOT = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", ".."))
FIXTURES_DIR = os.path.join(APP_ROOT, "tests", "fixtures", "levels")
CATALOG_PATH = os.path.join(APP_ROOT, "assets", "catalog.json")

# --------------------------------------------------------------------------
# Deterministic pseudo-randomness: a tiny LCG so the fixtures are stable across
# Python versions and platforms (no `random` module state).
# --------------------------------------------------------------------------


class Rng:
    def __init__(self, seed: int) -> None:
        self.state = seed & 0xFFFF_FFFF

    def next(self) -> int:
        self.state = (self.state * 1_664_525 + 1_013_904_223) & 0xFFFF_FFFF
        return self.state

    def unit(self) -> float:
        return self.next() / 0xFFFF_FFFF

    def between(self, low: float, high: float) -> float:
        return low + (high - low) * self.unit()

    def choice(self, values: List[str]) -> str:
        return values[self.next() % len(values)]


def load_placeables() -> List[Dict]:
    with open(CATALOG_PATH, encoding="utf-8") as handle:
        catalog = json.load(handle)
    placeables = [
        entry
        for entry in catalog["assets"]
        if entry.get("asset_type") in ("prop", "entity") and entry.get("model")
    ]
    placeables.sort(key=lambda entry: entry["id"])
    return placeables


def animation_component(actions: List[Dict]) -> Optional[Dict]:
    """The `animation` component the v3 loader requires for an animation action."""
    for action in actions:
        tag = action.get("action")
        if tag not in ("play_animation", "toggle_animation"):
            continue
        clip = action.get("clip")
        if not isinstance(clip, str) or not clip.strip():
            return None
        return {
            "component": "animation",
            "clip": clip.strip(),
            "looped": bool(action.get("loop", False)) if tag == "play_animation" else False,
            "playing": False,
        }
    return None


def interaction_v3(interaction: Dict) -> Tuple[List[Dict], List[Dict]]:
    """The format-v3 components/bindings for one interaction block."""
    component: Dict = {"component": "interactable"}
    prompt = interaction.get("prompt")
    if prompt is not None:
        component["prompt"] = prompt
    reach = interaction.get("reach")
    if reach is not None:
        component["reach"] = reach
    components = [component]
    animation = animation_component(interaction["actions"])
    if animation is not None:
        components.append(animation)
    return components, [{"on": "interact", "actions": interaction["actions"]}]


# --------------------------------------------------------------------------
# Sparse fixture
# --------------------------------------------------------------------------

SPARSE_QUADRANTS = [
    (2_000.0, 2_000.0),
    (-2_000.0, 2_000.0),
    (-2_000.0, -2_000.0),
    (2_000.0, -2_000.0),
]

SPARSE_ROOM = {"width": 26.0, "depth": 20.0, "height": 4.0}

SPARSE_FLOOR = "core:pool_tile_deck_01"
SPARSE_WALL = "core:pool_tile_wall_01"
SPARSE_CEILING = "core:pool_ceiling_01"


def sparse_room_shell(cx: float, cz: float) -> Tuple[List[Dict], Dict]:
    """One enclosed room centred on ``(cx, cz)`` with one doorway."""
    width = SPARSE_ROOM["width"]
    depth = SPARSE_ROOM["depth"]
    height = SPARSE_ROOM["height"]
    x = cx - width / 2.0
    z = cz - depth / 2.0
    room = {"x": x, "z": z, "width": width, "depth": depth, "height": height}
    thickness = 0.4
    # Walls are placed by minimum corner, exactly as the authoring guide
    # describes. The south wall carries the doorway; the other three are solid.
    walls = [
        {
            "x": x - thickness,
            "z": z - thickness,
            "width": width + 2 * thickness,
            "depth": thickness,
            "openings": [
                {
                    "kind": "door",
                    "offset": width / 2.0 - 0.8,
                    "width": 1.6,
                    "height": 2.2,
                    "sill": 0.0,
                }
            ],
        },
        {
            "x": x - thickness,
            "z": z + depth,
            "width": width + 2 * thickness,
            "depth": thickness,
        },
        {"x": x - thickness, "z": z, "width": thickness, "depth": depth},
        {"x": x + width, "z": z, "width": thickness, "depth": depth},
    ]
    return walls, room


def build_sparse(rng: Rng) -> Dict:
    rooms: List[Dict] = []
    walls: List[Dict] = []
    props: List[Dict] = []
    lights: List[Dict] = []
    volumes: List[Dict] = []
    routes: List[Dict] = []
    decals: List[Dict] = []
    water: List[Dict] = []

    for index, (cx, cz) in enumerate(SPARSE_QUADRANTS):
        shell, room = sparse_room_shell(cx, cz)
        room["material"] = SPARSE_FLOOR
        room["ceiling_material"] = SPARSE_CEILING
        for wall in shell:
            wall["material"] = SPARSE_WALL
        walls.extend(shell)
        rooms.append(room)

        # Fixtures on a 4 m grid, plus one wall luminaire.
        for row in range(3):
            for column in range(3):
                lights.append(
                    {
                        "id": f"far_light_{index}_{row}_{column}",
                        "fixture": "core:pool_light_round",
                        "x": cx - 8.0 + column * 8.0,
                        "z": cz - 6.0 + row * 6.0,
                        "brightness": 0.55,
                    }
                )
        lights.append(
            {
                "id": f"far_wall_light_{index}",
                "fixture": "core:pool_light_wall",
                "x": cx - 12.6,
                "z": cz,
                "y": 2.2,
                "rotation_degrees": 90.0,
                "brightness": 0.5,
            }
        )

        # A small prop cluster and one label interaction per quadrant.
        components, bindings = interaction_v3(
            {"prompt": "Toggle name", "actions": [{"action": "toggle_label"}]}
        )
        props.append(
            {
                "id": f"far_table_{index}",
                "display_name": f"Quadrant {index} Table",
                "model": "core:pool_table",
                "x": cx - 7.0,
                "z": cz + 4.0,
                "size": [0.8, 0.74, 0.8],
                "solid": True,
                "components": components,
                "bindings": bindings,
            }
        )
        props.append(
            {
                "id": f"far_chair_{index}",
                "model": "core:pool_chair",
                "x": cx - 5.4,
                "z": cz + 4.0,
                "rotation_degrees": 270.0,
                "size": [0.52, 0.85, 0.55],
                "solid": True,
            }
        )
        props.append(
            {
                "id": f"far_sign_{index}",
                "model": "core:stop_sign",
                "x": cx - 11.5,
                "z": cz - 8.0,
                "size": [0.45, 1.8, 0.06],
                "solid": True,
            }
        )

        decals.append(
            {
                "x": cx,
                "z": cz,
                "width": 1.2,
                "height": 1.2,
                "material": "core:decal_arrow_01",
                "surface": "floor",
            }
        )

        if index == 0:
            # A shallow water dish: the floating duck must sit in it and reach
            # the dynamic path at 2 km from the origin, not just in the demo.
            water.append(
                {
                    "x": cx + 2.0,
                    "z": cz - 2.0,
                    "width": 6.0,
                    "depth": 4.0,
                    "surface_y": 0.35,
                    "bottom_y": 0.0,
                    "swimming": False,
                }
            )
            props.append(
                {
                    "id": "far_duck",
                    "model": "core:rubber_duck",
                    "x": cx + 5.0,
                    "z": cz,
                    "size": [0.1, 0.12, 0.14],
                    "solid": False,
                    "float": {
                        "draft": 0.03,
                        "bob": 0.012,
                        "bob_seconds": 2.8,
                        "heel_degrees": 6.0,
                        "heel_seconds": 3.6,
                        "phase": 0.25,
                    },
                }
            )
            # A rat route in the far quadrant, and a reset trigger in the
            # opposite one: both systems must run at 2 km.
            props.append(
                {
                    "id": "far_rat",
                    "model": "rat",
                    "x": cx - 10.0,
                    "z": cz + 6.0,
                    "rotation_degrees": 0.0,
                    "size": [0.2, 0.2, 0.7],
                }
            )
            routes.append(
                {
                    "id": "far_rat",
                    "loop": True,
                    "steps": [
                        {"step": "move_to", "x": cx - 2.0, "z": cz + 6.0, "speed": 0.1985},
                        {"step": "wait", "seconds": 0.5},
                        {"step": "move_to", "x": cx - 10.0, "z": cz + 6.0, "speed": 0.5731},
                        {"step": "wait", "seconds": 0.5},
                    ],
                }
            )
        if index == 2:
            volumes.append(
                {
                    "id": "far_trigger",
                    "x": cx - 2.0,
                    "z": cz - 2.0,
                    "width": 4.0,
                    "depth": 4.0,
                    "bindings": [
                        {
                            "on": "enter_volume",
                            "actions": [{"action": "reset_to_start"}],
                            "cooldown_seconds": 0.5,
                        }
                    ],
                }
            )

    return {
        "format_version": 3,
        "id": "capacity_sparse",
        "name": "Capacity: Sparse World (dev)",
        "author": "Places Team",
        "spawn": {"x": SPARSE_QUADRANTS[0][0], "z": SPARSE_QUADRANTS[0][1] - 6.0, "yaw_degrees": 180.0},
        "defaults": {
            "wall": SPARSE_WALL,
            "floor": SPARSE_FLOOR,
            "ceiling": SPARSE_CEILING,
        },
        "rooms": rooms,
        "walls": walls,
        "ceiling_lights": lights,
        "props": props,
        "decals": decals,
        "water": water,
        "volumes": volumes,
        "routes": routes,
        # Each island's doorway deliberately opens onto the void: this fixture
        # exists to prove that geometry, lighting, a trigger, a route and a
        # float all run 2 km from the origin, not to be a walkable settlement.
        # The annotation is one narrow rectangle per doorway.
        "geometry_intent": [
            {
                "check": "room-leak",
                "x": cx - 1.0,
                "z": cz - SPARSE_ROOM["depth"] / 2.0 - 2.6,
                "width": 2.0,
                "depth": 2.2,
                "note": "The island's only doorway opens onto the void by design.",
            }
            for cx, cz in SPARSE_QUADRANTS
        ],
    }


# --------------------------------------------------------------------------
# Dense fixture
# --------------------------------------------------------------------------

DENSE_ROOM = {"x": -38.0, "z": -28.0, "width": 76.0, "depth": 56.0, "height": 5.2}
DENSE_TARGET_PROPS = 6_000
DENSE_TARGET_LIGHTS = 120
DENSE_FIXTURE_STEP = 0.47
DENSE_ANIMATED: List[Tuple[str, str]] = [
    ("mannequin", "pose_stand"),
    ("mannequin", "pose_arms_up"),
    ("mannequin", "pose_arms_forward"),
    ("skeleton", "pose_stand"),
    ("skeleton", "pose_sit_floor"),
    ("skeleton", "pose_sit_chair"),
    ("spooner-man", "walk"),
    ("spooner-man", "idle"),
    ("rat", "walk"),
    ("rat", "run"),
    # Independent copies of shared models: nine more actors, all animated on
    # their own route with their own pose, so the character budget and the
    # per-instance isolation are exercised well past the historical cap of 8.
    ("mannequin", "pose_arms_up"),
    ("mannequin", "pose_arms_forward"),
    ("skeleton", "pose_sit_floor"),
    ("skeleton", "pose_sit_chair"),
    ("spooner-man", "walk"),
    ("rat", "walk"),
    ("rat", "run"),
    ("spooner-man", "idle"),
]


def dense_walls(room: Dict) -> List[Dict]:
    x, z, width, depth = room["x"], room["z"], room["width"], room["depth"]
    thickness = 0.4
    # The east wall carries three doorways; the other three are solid. Walls are
    # placed by minimum corner and never overlap each other.
    east_door_openings = [
        {
            "kind": "door",
            "offset": offset,
            "width": 1.6,
            "height": 2.2,
            "sill": 0.0,
        }
        for offset in (8.0, 28.0, 48.0)
    ]
    return [
        {"x": x - thickness, "z": z - thickness, "width": width + 2 * thickness, "depth": thickness},
        {"x": x - thickness, "z": z + depth, "width": width + 2 * thickness, "depth": thickness},
        {"x": x - thickness, "z": z, "width": thickness, "depth": depth},
        {
            "x": x + width,
            "z": z,
            "width": thickness,
            "depth": depth,
            "openings": east_door_openings,
        },
    ]


def build_dense(rng: Rng, placeables: List[Dict]) -> Dict:
    room = dict(DENSE_ROOM)
    room["material"] = "core:pool_tile_deck_01"
    room["ceiling_material"] = "core:pool_ceiling_01"

    walls = dense_walls(room)
    for wall in walls:
        wall["material"] = "core:pool_tile_wall_01"

    x0, z0 = room["x"], room["z"]
    width, depth = room["width"], room["depth"]

    props: List[Dict] = []
    lights: List[Dict] = []
    decals: List[Dict] = []
    arc_walls: List[Dict] = []
    pillars: List[Dict] = []
    water: List[Dict] = []

    # A contained water basin in the south-east corner: the duck and the basin
    # step demonstrate real water at scale.
    basin = {
        "x": x0 + width - 20.0,
        "z": z0 + depth - 14.0,
        "width": 16.0,
        "depth": 10.0,
        "offset_y": -1.4,
        "material": "core:pool_tile_basin_01",
        "edge_material": "core:pool_tile_wall_01",
    }
    water.append(
        {
            "x": basin["x"],
            "z": basin["z"],
            "width": basin["width"],
            "depth": basin["depth"],
            "surface_y": -0.2,
            "bottom_y": -1.4,
            "swimming": True,
        }
    )

    # Curved architecture and pillars along the hall so the round primitives
    # carry real load at this instance count, in more than one material.
    for index in range(6):
        arc_walls.append(
            {
                "x": x0 + 12.0 + index * 10.0,
                "z": z0 + 28.0,
                "radius": 3.0,
                "thickness": 0.24,
                "height": 5.2,
                "start_degrees": 0.0,
                "sweep_degrees": 180.0,
                "segments": 24,
                "material": "core:pool_tile_wall_01",
                "inner_material": "core:pool_tile_deck_01",
            }
        )
    for index in range(8):
        pillars.append(
            {
                "x": x0 + 8.0 + (index % 4) * 18.0,
                "z": z0 + 10.0 + (index // 4) * 18.0,
                "radius": 0.3,
                "height": 5.2,
                "segments": 16,
                "material": "core:pool_tile_wall_01",
                "cap_material": "core:pool_tile_deck_01",
            }
        )

    # Fixtures on an even grid across the hall, capped at the target count.
    columns = 13
    rows = 10
    for row in range(rows):
        for column in range(columns):
            if len(lights) >= DENSE_TARGET_LIGHTS:
                break
            lights.append(
                {
                    "id": f"dense_light_{row}_{column}",
                    "fixture": "core:pool_light_round",
                    "x": round(x0 + (column + 1) * width / (columns + 1), 3),
                    "z": round(z0 + (row + 1) * depth / (rows + 1), 3),
                    "brightness": 0.55,
                }
            )

    # Three clear east-west corridors through the hall, reserved for the
    # animated cast: no prop whose footprint reaches inside a corridor is
    # placed there, so every actor route runs along a genuinely clear line.
    # Their z values avoid the pillars (z0+12, z0+34), the arc walls (z0+34)
    # and the water basin.
    corridors = [z0 + 5.0, z0 + 15.0, z0 + 36.0]
    corridor_half_width = 2.6

    def in_corridor(pz: float, depth: float) -> bool:
        return any(abs(pz - cz) < corridor_half_width + depth * 0.5 for cz in corridors)

    # The instance field: every registered model, spread over a fine bay grid
    # so the hall holds thousands of instances but keeps a clear spawn plaza,
    # the reserved cast corridors and a regular aisle lattice. Every placement
    # authors `id`, `size` and `solid` explicitly, so the fixture is
    # deterministic down to the last field.
    step = DENSE_FIXTURE_STEP
    margin = 7.0
    spawn_plaza = (x0 + 3.0, z0 + 3.0, 12.0, 12.0)
    model_index = 0
    row = 0
    z = z0 + margin
    while z <= z0 + depth - margin and len(props) < DENSE_TARGET_PROPS:
        column = 0
        x = x0 + margin
        while x <= x0 + width - margin and len(props) < DENSE_TARGET_PROPS:
            in_aisle = row % 8 == 7 or column % 8 == 7
            px, pz = x, z
            in_plaza = (
                spawn_plaza[0] <= px <= spawn_plaza[0] + spawn_plaza[2]
                and spawn_plaza[1] <= pz <= spawn_plaza[1] + spawn_plaza[3]
            )
            in_basin = (
                basin["x"] - 1.0 <= px <= basin["x"] + basin["width"] + 1.0
                and basin["z"] - 1.0 <= pz <= basin["z"] + basin["depth"] + 1.0
            )
            if not in_aisle and not in_plaza and not in_basin:
                entry = placeables[model_index % len(placeables)]
                size = entry.get("size") or [0.6, 0.9, 0.6]
                if not in_corridor(pz, float(size[2])):
                    model_index += 1
                    props.append(
                        {
                            "id": f"dense_{row}_{column}",
                            "model": entry["id"],
                            "x": round(px + rng.between(-0.12, 0.12), 3),
                            "z": round(pz + rng.between(-0.12, 0.12), 3),
                            "rotation_degrees": round(rng.between(0.0, 359.0), 1),
                            "size": [round(float(value), 3) for value in size],
                            "solid": bool(entry.get("solid")),
                        }
                    )
            column += 1
            x += step
        row += 1
        z += step

    # Animated cast: independent instances of the four rigs, all placed on the
    # reserved corridors and each on its own short route along its corridor.
    # They demonstrate a character count well past the historical cap of eight,
    # with per-instance pose state.
    animated_routes: List[Dict] = []
    for index, (model, clip) in enumerate(DENSE_ANIMATED):
        corridor = corridors[index % len(corridors)]
        x = x0 + 9.0 + (index // len(corridors)) * 11.0
        z = corridor
        prop_id = f"dense_actor_{index}"
        components, bindings = interaction_v3(
            {
                "prompt": "Animate",
                "actions": [
                    {
                        "action": "play_animation",
                        "clip": clip,
                        "loop": clip in ("walk", "run", "idle"),
                    }
                ],
            }
        )
        props.append(
            {
                "id": prop_id,
                "model": model,
                "x": round(x, 3),
                "z": round(z, 3),
                "size": [0.7, 1.8, 0.7],
                "components": components,
                "bindings": bindings,
            }
        )
        animated_routes.append(
            {
                "id": prop_id,
                "loop": True,
                "steps": [
                    {"step": "play", "clip": clip, "seconds": 1.0, "loop": True},
                    {"step": "move_to", "x": round(x + 3.0, 3), "z": round(z, 3), "speed": 0.5},
                    {"step": "play", "clip": clip, "seconds": 1.0, "loop": True},
                    {"step": "move_to", "x": round(x, 3), "z": round(z, 3), "speed": 0.5},
                ],
            }
        )

    # Decals on the deck grid.
    for index in range(48):
        decals.append(
            {
                "x": x0 + 5.0 + (index % 12) * 6.0,
                "z": z0 + 4.0 + (index // 12) * 5.0,
                "width": 1.0,
                "height": 1.0,
                "material": "core:decal_arrow_01",
                "surface": "floor",
            }
        )

    return {
        "format_version": 3,
        "id": "capacity_dense",
        "name": "Capacity: Dense Content (dev)",
        "author": "Places Team",
        "spawn": {"x": x0 + 3.0, "z": z0 + 3.0, "yaw_degrees": 45.0},
        "defaults": {
            "wall": "core:pool_tile_wall_01",
            "floor": "core:pool_tile_deck_01",
            "ceiling": "core:pool_ceiling_01",
        },
        "rooms": [room],
        "walls": walls,
        "floor_regions": [basin],
        "water": water,
        "arc_walls": arc_walls,
        "pillars": pillars,
        "ceiling_lights": lights,
        "props": props,
        "decals": decals,
        "routes": animated_routes,
        # The three east doorways are the hall's exits; beyond them is the
        # void, which is what this capacity fixture is about. One narrow
        # annotation per doorway covers the leak check.
        "geometry_intent": [
            {
                "check": "room-leak",
                "x": x0 + width - 1.0,
                "z": z0 + offset - 2.0,
                "width": 4.5,
                "depth": 5.0,
                "note": "The hall's exit doorway opens onto the void by design.",
            }
            for offset in (8.0, 28.0, 48.0)
        ],
    }


# --------------------------------------------------------------------------


def write_level(level: Dict, path: str) -> None:
    with open(path, "w", encoding="utf-8") as handle:
        json.dump(level, handle, indent=2, sort_keys=False)
        handle.write("\n")


def main(argv: Optional[List[str]] = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--check",
        action="store_true",
        help="derive both fixtures and compare them to the committed files",
    )
    args = parser.parse_args(argv)

    placeables = load_placeables()
    levels = [
        build_sparse(Rng(0x50_41_52_53)),
        build_dense(Rng(0x44_45_4E_53), placeables),
    ]
    os.makedirs(FIXTURES_DIR, exist_ok=True)
    failures = 0
    for level in levels:
        path = os.path.join(FIXTURES_DIR, f"{level['id']}.json")
        rendered = json.dumps(level, indent=2) + "\n"
        if args.check:
            try:
                with open(path, encoding="utf-8") as handle:
                    current = handle.read()
            except FileNotFoundError:
                print(f"missing {path}")
                failures += 1
                continue
            if current != rendered:
                print(f"stale {path}: regenerate with tools/levels/build_capacity_fixtures.py")
                failures += 1
            else:
                print(f"ok {path}")
        else:
            write_level(level, path)
            size = os.path.getsize(path)
            print(
                f"wrote {path} ({size / 1024.0:.0f} KiB, {len(level.get('props', []))} props, "
                f"{len(level.get('ceiling_lights', []))} lights, {len(level.get('walls', []))} walls)"
            )
    if args.check and failures:
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
