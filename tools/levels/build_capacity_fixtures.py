#!/usr/bin/env python3
"""Generate the maintained capacity regression fixtures for Places.

`capacity_dense.json` is the deterministic hall that exercises instance, model,
character and lighting budgets. `capacity_beyond_former_limits.json` is the
2026 capacity pass's high-count source: one past the *previous* value of every
loader count cap it carries, with more than 65 536 distinct material ids so the
former 16-bit material index would have collapsed, and without reaching the new
`MAX_LEVEL_MATERIALS` budget. The former sparse playable fixture was retired
because its navigation grid cannot be compiled; far-coordinate CPU regressions
remain in src/zoo_audit.rs.

Run from the repository root:
    python3 tools/levels/build_capacity_fixtures.py
    python3 tools/levels/build_capacity_fixtures.py --check
"""

from __future__ import annotations

import argparse
import json
import math
import os
import sys
from typing import Dict, List, Optional, Set, Tuple

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
# Dense fixture
# --------------------------------------------------------------------------

DENSE_ROOM = {"x": -38.0, "z": -28.0, "width": 76.0, "depth": 56.0, "height": 5.2}
DENSE_TARGET_PROPS = 6_000
DENSE_TARGET_LIGHTS = 120
DENSE_FIXTURE_STEP = 0.47
# Fixed historical authoring inputs for the dense collision/instance witness.
# These are level placement extents/flags, not replacement catalog metadata.
# Catalog growth must not reshuffle every bay or reduce its >5000-instance load;
# the Model Zoo owns complete current-catalog coverage.
DENSE_PLACEABLE_SPECS: Tuple[Dict, ...] = (
    {"id": "core:armchair", "size": [0.9, 0.9, 0.9], "solid": True},
    {"id": "core:bed", "size": [1.4, 0.55, 2.0], "solid": True},
    {"id": "core:bookshelf", "size": [1.0, 1.8, 0.35], "solid": True},
    {"id": "core:cabinet", "size": [0.9, 0.85, 0.45], "solid": True},
    {"id": "core:cardboard_box", "size": [0.5, 0.5, 0.5], "solid": False},
    {"id": "core:chair", "size": [0.5, 0.9, 0.5], "solid": True},
    {"id": "core:couch", "size": [2.0, 0.9, 0.9], "solid": True},
    {"id": "core:crate", "size": [0.6, 0.6, 0.6], "solid": True},
    {"id": "core:desk", "size": [1.6, 0.75, 0.7], "solid": True},
    {"id": "core:exit_sign", "size": [0.45, 0.57, 0.08], "solid": False},
    {"id": "core:fridge", "size": [0.7, 1.8, 0.7], "solid": True},
    {"id": "core:lamp", "size": [0.35, 1.5, 0.35], "solid": False},
    {"id": "core:plant", "size": [0.4, 1.0, 0.4], "solid": False},
    {"id": "core:pool_chair", "size": [0.52, 0.85, 0.55], "solid": True},
    {"id": "core:pool_curtain_corner", "size": [0.6, 2.6, 0.6], "solid": False},
    {"id": "core:pool_curtain_end", "size": [0.6, 2.6, 0.22], "solid": False},
    {"id": "core:pool_curtain_straight", "size": [1.2, 2.6, 0.22], "solid": False},
    {"id": "core:pool_guardrail_corner", "size": [0.6, 1.05, 0.6], "solid": True},
    {"id": "core:pool_guardrail_end", "size": [0.6, 1.05, 0.08], "solid": True},
    {"id": "core:pool_guardrail_straight", "size": [2.0, 1.05, 0.08], "solid": True},
    {"id": "core:pool_ladder", "size": [0.55, 2.2, 0.45], "solid": True},
    {"id": "core:pool_table", "size": [0.8, 0.74, 0.8], "solid": True},
    {"id": "core:rubber_duck", "size": [0.1, 0.12, 0.14], "solid": False},
    {"id": "core:rug", "size": [2.0, 0.02, 1.4], "solid": False},
    {"id": "core:sink", "size": [0.6, 1.1, 0.55], "solid": True},
    {"id": "core:stop_sign", "size": [0.45, 1.8, 0.06], "solid": True},
    {"id": "core:stove", "size": [0.6, 0.9, 0.6], "solid": True},
    {"id": "core:table", "size": [1.4, 0.75, 0.8], "solid": True},
    {"id": "core:tv", "size": [1.1, 0.7, 0.1], "solid": False},
    {"id": "core:vending_machine", "size": [1.0, 1.9, 0.8], "solid": True},
    {"id": "core:washer_drum", "size": [0.42, 0.3, 0.42], "solid": False},
    {"id": "core:washing_machine", "size": [0.6, 0.85, 0.6], "solid": True},
    {"id": "core:water_cooler", "size": [0.35, 1.1, 0.35], "solid": True},
    {"id": "home:ball_light", "size": [0.2, 0.8, 0.2], "solid": False},
    {"id": "home:bowl", "size": [0.15, 0.065, 0.15], "solid": False},
    {"id": "home:cabinet_base", "size": [0.6, 0.9, 0.6], "solid": True},
    {"id": "home:cabinet_wall", "size": [0.6, 0.72, 0.33], "solid": True},
    {"id": "home:crt_tv", "size": [0.55, 0.48, 0.46], "solid": True},
    {"id": "home:fork", "size": [0.026, 0.012, 0.196], "solid": False},
    {"id": "home:knife", "size": [0.022, 0.016, 0.215], "solid": False},
    {"id": "home:plant_table", "size": [0.13, 0.28, 0.13], "solid": False},
    {"id": "home:plate", "size": [0.22, 0.022, 0.22], "solid": False},
    {"id": "home:spoon", "size": [0.036, 0.02, 0.185], "solid": False},
    {"id": "home:wall_switch", "size": [0.086, 0.12, 0.033], "solid": False},
    {"id": "mannequin", "size": [0.42, 1.72, 0.305], "solid": False},
    {"id": "rat", "size": [0.095, 0.139, 0.613], "solid": False},
    {"id": "skeleton", "size": [0.427, 1.72, 0.315], "solid": False},
    {"id": "spooner-man", "size": [0.165, 0.389, 0.626], "solid": False},
)
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
    # per-instance isolation are exercised well past the previous cap of eight.
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


def dense_placeables(placeables: List[Dict]) -> List[Dict]:
    """Resolve the fixed witness against actual current catalog placeables."""
    current = {entry["id"] for entry in placeables
               if entry.get("asset_type") in ("prop", "entity") and entry.get("model")}
    missing = [entry["id"] for entry in DENSE_PLACEABLE_SPECS if entry["id"] not in current]
    if missing:
        raise ValueError("dense witness models are missing from catalog placeables: " + ", ".join(missing))
    return [{"id": entry["id"], "size": list(entry["size"]), "solid": entry["solid"]}
            for entry in DENSE_PLACEABLE_SPECS]


def build_dense(rng: Rng, placeables: List[Dict]) -> Dict:
    placeables = dense_placeables(placeables)
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

    # The instance field: the fixed historical models, spread over a fine bay grid
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
    # They demonstrate a character count well past the previous cap of eight,
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
# Beyond-former-limits fixture
# --------------------------------------------------------------------------
#
# The 2026 capacity pass raised every loader count cap (see
# docs/MAP_AUTHORING_GUIDE.md). This fixture carries *one past the previous
# value* of each cap it exercises, so the raised caps are proven by content the
# old validators rejected, and it crosses the former 16-bit material index
# boundary with more than 65 536 distinct material ids while staying under the
# new MAX_LEVEL_MATERIALS budget (131 072). Like the dense fixture it is
# deterministic, and `--check` verifies it has not drifted.

BEYOND_ROOMS_PER_SIDE = 45
BEYOND_ROOM_PITCH = 6.0
BEYOND_ROOM_EXTENT = 5.0
BEYOND_ROOM_HEIGHT = 4.0
BEYOND_WALL_TARGET = 20_001
BEYOND_PROP_TARGET = 20_001
BEYOND_LIGHT_TARGET = 400
BEYOND_DECAL_TARGET = 5_001
BEYOND_REGION_TARGET = 2_001
BEYOND_PATCH_TARGET = 2_001
BEYOND_WATER_TARGET = 2_001
BEYOND_LADDER_TARGET = 257
BEYOND_RAMP_TARGET = 501
BEYOND_STAIR_TARGET = 501
BEYOND_HALF_WALL_TARGET = 2_001
BEYOND_COLUMN_TARGET = 2_001
BEYOND_ARC_WALL_TARGET = 1_001
BEYOND_PILLAR_TARGET = 2_001
BEYOND_ARCHWAY_TARGET = 501
BEYOND_GUARDRAIL_TARGET = 2_001
BEYOND_THRESHOLD_TARGET = 1_001
BEYOND_BASEBOARD_TARGET = 2_001
# The property the material-id generator must satisfy: above the former 16-bit
# collapse point (65 536), below the new explicit budget (131 072).
BEYOND_MATERIAL_FLOOR = 65_536
BEYOND_MATERIAL_CEILING = 131_072


def build_beyond_former_limits(rng: Rng, placeables: List[Dict]) -> Dict:
    """One past the former loader caps, with 65 536+ distinct materials."""
    # The fixture places two cheap models many times; the catalogue list is
    # intentionally unused so the prop budget is exercised without dragging
    # every registered model's geometry into one package record.
    del placeables

    material_ids: Set[str] = set()
    counter = 0

    def material(tag: str) -> str:
        nonlocal counter
        counter += 1
        value = f"cap:{tag}_{counter:06d}"
        material_ids.add(value)
        return value

    rooms: List[Dict] = []
    for row in range(BEYOND_ROOMS_PER_SIDE):
        for column in range(BEYOND_ROOMS_PER_SIDE):
            rooms.append(
                {
                    "x": column * BEYOND_ROOM_PITCH,
                    "z": row * BEYOND_ROOM_PITCH,
                    "width": BEYOND_ROOM_EXTENT,
                    "depth": BEYOND_ROOM_EXTENT,
                    "height": BEYOND_ROOM_HEIGHT,
                    "material": material("floor"),
                    "ceiling_material": material("ceiling"),
                }
            )

    # Walls live in the 1 m gaps between rooms: 450 stubs along each of the 45
    # vertical gap lines. Each stub carries its own two face materials, which is
    # what pushes the distinct-id count past the former 16-bit boundary; the
    # body material is shared so the count stays inside the new budget.
    wall_body = material("wall_body")
    walls: List[Dict] = []
    for index in range(BEYOND_WALL_TARGET):
        line = index // 450
        along = index % 450
        walls.append(
            {
                "x": round(line * BEYOND_ROOM_PITCH + BEYOND_ROOM_EXTENT + 0.5, 3),
                "z": round(along * 0.55, 3),
                "width": 0.4,
                "depth": 0.4,
                "height": 2.0,
                "material": wall_body,
                "faces": {
                    "north": material("wall_north"),
                    "south": material("wall_south"),
                },
            }
        )

    # Props on a 1.9 m field, jittered by the shared LCG so the fixture is
    # deterministic; the two cheapest catalogue models keep the prepared prop
    # record inside the package's per-entry byte budget at 20 001 placements.
    # The whole fixture stays inside a ~270 m square so the baked navigation
    # grid stays inside the package's cell budget.
    prop_models = ["core:crate", "core:cardboard_box"]
    props: List[Dict] = []
    for index in range(BEYOND_PROP_TARGET):
        column = index % 142
        row = index // 142
        props.append(
            {
                "id": f"cap_prop_{index:05d}",
                "model": prop_models[index % len(prop_models)],
                "x": round(column * 1.9 + rng.between(0.0, 1.0), 3),
                "z": round(row * 1.9 + rng.between(0.0, 1.0), 3),
                "rotation_degrees": round(rng.between(0.0, 359.0), 1),
                "size": [0.6, 0.6, 0.6],
                "solid": False,
            }
        )

    lights: List[Dict] = []
    for index in range(BEYOND_LIGHT_TARGET):
        room = rooms[index % len(rooms)]
        lights.append(
            {
                "id": f"cap_light_{index:04d}",
                "fixture": "core:fluorescent_panel_01",
                "x": round(room["x"] + BEYOND_ROOM_EXTENT * 0.5, 3),
                "z": round(room["z"] + BEYOND_ROOM_EXTENT * 0.5, 3),
                "brightness": 0.45,
            }
        )

    decals: List[Dict] = []
    # Decals live in rooms 501 and up: the ramp and stair rooms carry raised
    # walking surfaces, and a decal that straddles one would span a floor
    # height change (which is exactly what the validator refuses). The patch,
    # region and pool of the first 2 001 rooms sit at the floor plane (the
    # region's `offset_y` is zero), so they add no height change.
    decal_rooms = rooms[BEYOND_RAMP_TARGET:]
    for index in range(BEYOND_DECAL_TARGET):
        room = decal_rooms[index % len(decal_rooms)]
        slot = index // len(decal_rooms)
        decals.append(
            {
                "x": round(room["x"] + 1.0 + (slot % 2) * 2.0, 3),
                "z": round(room["z"] + 1.0 + (slot // 2) * 2.0, 3),
                "width": 0.8,
                "height": 0.8,
                "material": "core:decal_arrow_01",
                "surface": "floor",
            }
        )

    # A patch, a flat region and a decorative pool share the first 2 001 rooms.
    # Each sits in a different corner so the region and the water never
    # overlap, and the patch is only a material override.
    patches: List[Dict] = []
    regions: List[Dict] = []
    water: List[Dict] = []
    for index in range(BEYOND_PATCH_TARGET):
        room = rooms[index]
        patches.append(
            {
                "x": round(room["x"] + BEYOND_ROOM_EXTENT * 0.5 - 1.0, 3),
                "z": round(room["z"] + BEYOND_ROOM_EXTENT * 0.5 - 1.0, 3),
                "width": 2.0,
                "depth": 2.0,
                "material": material("patch"),
            }
        )
    for index in range(BEYOND_REGION_TARGET):
        room = rooms[index]
        regions.append(
            {
                "x": round(room["x"] + 0.5, 3),
                "z": round(room["z"] + 0.5, 3),
                "width": 1.0,
                "depth": 1.0,
                "offset_y": 0.0,
                "material": material("region"),
                "edge_material": material("region_edge"),
            }
        )
    for index in range(BEYOND_WATER_TARGET):
        room = rooms[index]
        water.append(
            {
                "x": round(room["x"] + 3.0, 3),
                "z": round(room["z"] + 3.0, 3),
                "width": 1.0,
                "depth": 1.0,
                "surface_y": 0.05,
                "bottom_y": -0.5,
                "material": material("water"),
                "swimming": False,
            }
        )

    ladders: List[Dict] = []
    for index in range(BEYOND_LADDER_TARGET):
        room = rooms[index]
        ladders.append(
            {
                "x": round(room["x"] + 3.5, 3),
                "z": round(room["z"] + 0.5, 3),
                "width": 0.6,
                "depth": 0.2,
                "bottom_y": 0.0,
                "top_y": 2.5,
                "facing_degrees": 0.0,
            }
        )

    ramps: List[Dict] = []
    for index in range(BEYOND_RAMP_TARGET):
        room = rooms[index]
        ramps.append(
            {
                "x": round(room["x"] + 0.5, 3),
                "z": round(room["z"] + 2.0, 3),
                "width": 1.0,
                "depth": 2.0,
                "offset_y": 0.0,
                "rise": 0.25,
                "material": material("ramp"),
                "edge_material": material("ramp_edge"),
            }
        )

    stairs: List[Dict] = []
    for index in range(BEYOND_STAIR_TARGET):
        room = rooms[index]
        stairs.append(
            {
                "x": round(room["x"] + 2.0, 3),
                "z": round(room["z"] + 0.5, 3),
                "width": 1.0,
                "depth": 1.2,
                "offset_y": 0.0,
                "rise": 0.4,
                "steps": 3,
                "material": material("stair"),
                "riser_material": material("stair_riser"),
                "side_material": material("stair_side"),
            }
        )

    half_walls: List[Dict] = []
    for index in range(BEYOND_HALF_WALL_TARGET):
        room = rooms[index]
        half_walls.append(
            {
                "x": round(room["x"] + 1.5, 3),
                "z": round(room["z"] + 0.5, 3),
                "width": 2.0,
                "depth": 0.2,
                "height": 1.0,
                "material": material("half_wall"),
                "end_material": material("half_wall_end"),
                "cap_material": material("half_wall_cap"),
            }
        )

    columns: List[Dict] = []
    for index in range(BEYOND_COLUMN_TARGET):
        room = rooms[index]
        columns.append(
            {
                "x": round(room["x"] + 0.5, 3),
                "z": round(room["z"] + 3.5, 3),
                "width": 0.3,
                "depth": 0.3,
                "material": material("column"),
                "cap_material": material("column_cap"),
            }
        )

    arc_walls: List[Dict] = []
    for index in range(BEYOND_ARC_WALL_TARGET):
        room = rooms[index]
        arc_walls.append(
            {
                "x": round(room["x"] + 2.5, 3),
                "z": round(room["z"] + 2.5, 3),
                "radius": 1.2,
                "thickness": 0.2,
                "height": 2.4,
                "start_degrees": 0.0,
                "sweep_degrees": 90.0,
                "segments": 8,
                "material": material("arc"),
                "inner_material": material("arc_inner"),
                "outer_material": material("arc_outer"),
                "cap_material": material("arc_cap"),
                "end_material": material("arc_end"),
            }
        )

    pillars: List[Dict] = []
    for index in range(BEYOND_PILLAR_TARGET):
        room = rooms[index]
        pillars.append(
            {
                "x": round(room["x"] + 3.5, 3),
                "z": round(room["z"] + 0.5, 3),
                "radius": 0.2,
                "height": 3.0,
                "segments": 8,
                "material": material("pillar"),
                "cap_material": material("pillar_cap"),
            }
        )

    archways: List[Dict] = []
    for index in range(BEYOND_ARCHWAY_TARGET):
        room = rooms[index]
        archways.append(
            {
                "x": round(room["x"] + 1.0, 3),
                "z": round(room["z"] + 3.5, 3),
                "width": 2.4,
                "depth": 0.4,
                "height": 2.5,
                "opening_width": 1.0,
                "opening_height": 2.0,
                "arch_rise": 0.2,
                "material": material("archway"),
                "reveal_material": material("archway_reveal"),
            }
        )

    guardrails: List[Dict] = []
    for index in range(BEYOND_GUARDRAIL_TARGET):
        room = rooms[index]
        guardrails.append(
            {
                "x": round(room["x"] + 4.0, 3),
                "z": round(room["z"] + 2.0, 3),
                "length": 2.0,
                "rotation_degrees": 0.0,
                "height": 0.9,
                "material": material("guardrail"),
                "post_material": material("guardrail_post"),
            }
        )

    thresholds: List[Dict] = []
    for index in range(BEYOND_THRESHOLD_TARGET):
        room = rooms[index]
        thresholds.append(
            {
                "x": round(room["x"] + 2.5, 3),
                "z": round(room["z"] + 3.5, 3),
                "length": 1.0,
                "thickness": 0.08,
                "height": 0.02,
                "rotation_degrees": 90.0,
                "material": material("threshold"),
            }
        )

    baseboards: List[Dict] = []
    for index in range(BEYOND_BASEBOARD_TARGET):
        room = rooms[index]
        baseboards.append(
            {
                "x": round(room["x"] + 2.5, 3),
                "z": round(room["z"] + 1.5, 3),
                "length": 2.0,
                "rotation_degrees": 90.0,
                "material": material("baseboard"),
            }
        )

    # Entities: a handful of authored records, enough that a package carries
    # the whole interaction schema without duplicating the loader's per-cap
    # at/over test coverage.
    spawn_templates = [
        {"id": "cap_crate", "model": "core:crate", "scale": 1.0},
        {"id": "cap_box", "model": "core:cardboard_box", "scale": 1.0},
    ]
    spawn_points = [
        {
            "id": f"cap_point_{index}",
            "x": round(rooms[index]["x"] + 1.0, 3),
            "z": round(rooms[index]["z"] + 1.5, 3),
            "template": spawn_templates[index % len(spawn_templates)]["id"],
        }
        for index in range(4)
    ]
    spawn_groups = [{"id": "cap_group", "at_most_one_active": True}]
    sequences = [
        {
            "id": "cap_sequence",
            "steps": [
                {"step": "wait", "seconds": 0.5},
                {
                    "step": "action",
                    "action": {
                        "action": "spawn_entity",
                        "point": "cap_point_0",
                        "group": "cap_group",
                    },
                },
            ],
        }
    ]
    volumes = [
        {
            "id": f"cap_trigger_{index}",
            "x": round(rooms[index]["x"] + 2.0, 3),
            "z": round(rooms[index]["z"] + 2.0, 3),
            "width": 2.0,
            "depth": 2.0,
            "bindings": [
                {
                    "on": "enter_volume",
                    "actions": [{"action": "reset_to_start"}],
                    "cooldown_seconds": 1.0,
                }
            ],
        }
        for index in range(4)
    ]

    distinct = len(material_ids)
    if not BEYOND_MATERIAL_FLOOR < distinct < BEYOND_MATERIAL_CEILING:
        raise SystemExit(
            f"the beyond-former-limits fixture declares {distinct} distinct materials; "
            f"it must be above {BEYOND_MATERIAL_FLOOR} (the former 16-bit boundary) "
            f"and below {BEYOND_MATERIAL_CEILING} (MAX_LEVEL_MATERIALS)"
        )

    return {
        "format_version": 3,
        "id": "capacity_beyond_former_limits",
        "name": "Capacity: Beyond Former Limits (dev)",
        "author": "Places Team",
        "spawn": {"x": 4.5, "z": 0.5, "yaw_degrees": 0.0},
        "defaults": {
            "wall": wall_body,
            "floor": material("default_floor"),
            "ceiling": material("default_ceiling"),
        },
        "rooms": rooms,
        "walls": walls,
        "floor_patches": patches,
        "floor_regions": regions,
        "water": water,
        "ladders": ladders,
        "ramps": ramps,
        "stairs": stairs,
        "half_walls": half_walls,
        "columns": columns,
        "arc_walls": arc_walls,
        "pillars": pillars,
        "archways": archways,
        "guardrails": guardrails,
        "thresholds": thresholds,
        "baseboards": baseboards,
        "ceiling_lights": lights,
        "props": props,
        "decals": decals,
        "spawn_templates": spawn_templates,
        "spawn_points": spawn_points,
        "spawn_groups": spawn_groups,
        "sequences": sequences,
        "volumes": volumes,
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
        help="derive the dense fixture and compare it to the committed file",
    )
    args = parser.parse_args(argv)

    placeables = load_placeables()
    levels = [
        build_dense(Rng(0x44_45_4E_53), placeables),
        build_beyond_former_limits(Rng(0x42_45_59_4F), placeables),
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
