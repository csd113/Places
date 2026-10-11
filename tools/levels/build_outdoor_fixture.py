#!/usr/bin/env python3
"""Generates ``tests/fixtures/levels/outdoor_kit_showcase.json``.

The outdoor kit's engine fixture: a night yard with the three grass-density
profiles, a dirt path with feathered edges, a concrete walkway and step, a
tree, the exterior lamp family on all three mounts, the modular house facade
around a real doorway, and a faint-star sky above an open-ceiling room. A
closed shed on the east side is the indoor control: it has a real ceiling, so
the sky can never leak through it, and its doorway looks back onto the yard.

The expanded kit's yard sits in the yard's south-east quarter: the three tree
models (leafy, birch, conifer), two streetlights whose heads reach over the
dirt path, five assembled facade shells (each family's doorway front, window
side, gable, roof slopes, ridge, corner boards and porch deck with posts) and
a porch railing run with a 90 degree corner and two terminals.

The fixture is deterministic and derived from the real tools:

* grass placements come from ``tools/levels/scatter_grass.py`` (the same
  seeded LCG and keep-out rules an author uses);
* every asset id is a real catalogue entry;
* the level is what ``python3 tools/assets/validate.py`` vets.

Run from the Places repository root:

    python3 tools/levels/build_outdoor_fixture.py           # write the level
    python3 tools/levels/build_outdoor_fixture.py --check    # verify it is current
"""

from __future__ import annotations

import argparse
import json
import math
import os
import sys
from pathlib import Path
from typing import Dict, List

APP_ROOT = Path(__file__).resolve().parent.parent.parent
OUTPUT_PATH = APP_ROOT / "tests" / "fixtures" / "levels" / "outdoor_kit_showcase.json"
sys.path.insert(0, str(Path(__file__).resolve().parent))

import scatter_grass  # noqa: E402  (the directory is on sys.path above)

# ---------------------------------------------------------------- constants

#: The yard: an open-ceiling exterior room, grass ground, 26 x 20 m.
YARD = {"x": 0.0, "z": 0.0, "width": 26.0, "depth": 20.0, "height": 6.0}
#: The house: a closed gable room behind the facade wall.
HOUSE = {"x": 2.5, "z": -3.8, "width": 15.0, "depth": 4.8, "height": 2.7}
#: The shed: the indoor control room, closed flat ceiling.
SHED = {"x": 26.0, "z": 6.0, "width": 5.0, "depth": 5.0, "height": 2.6}
#: Gable rise matching the roof props' ~32.5 degree pitch over a 2.4 m run.
RIDGE_RISE = 1.75

GRASS_LOW = (13.0, 1.0, 12.5, 8.0)
GRASS_MEDIUM = (0.5, 9.0, 8.0, 10.0)
GRASS_DENSE = (3.0, 3.0, 5.0, 4.5)
#: Seed per profile: fixed, so the fixture is byte-stable.
SEEDS = {"low": 21, "medium": 22, "dense": 23}
#: Rectangles no grass may enter: path, walkway, house, shed, fence run, lamps,
#: tree trunk, spawn and the expanded kit's shells, trees and streetlights.
KEEP_OUTS = [
    (9.2, 4.2, 1.6, 14.4),   # dirt path
    (8.0, 1.6, 4.0, 2.6),    # concrete walkway
    (2.5, -4.1, 15.0, 5.4),  # house
    (25.7, 5.7, 5.6, 5.6),   # shed
    (0.9, 2.0, 0.6, 16.0),   # fence run
    (7.9, 5.7, 0.6, 0.6),    # stand lamp
    (11.5, 10.7, 0.6, 0.6),  # stand lamp
    (7.9, 15.7, 0.6, 0.6),   # stand lamp
    (13.1, 3.3, 0.6, 0.6),   # stand lamp
    (5.0, 5.0, 1.0, 1.0),    # tree trunk
    (9.4, 15.4, 1.2, 1.2),   # spawn
    # --- the expanded outdoor kit's yard -----------------------------------
    (11.4, 13.9, 11.2, 5.5),  # three assembled facade shells, south row
    (16.6, 5.6, 7.6, 5.5),    # two assembled facade shells, north row
    (3.1, 11.7, 1.0, 1.0),    # birch trunk (outdoor:tree_02)
    (5.5, 15.7, 1.0, 1.0),    # conifer trunk (outdoor:tree_03)
    (11.2, 12.9, 0.6, 0.6),   # streetlight at the street's west end
    (24.3, 12.7, 0.6, 0.6),   # streetlight at the street's east end
]

# ------------------------------------------------------------- kit yard
#
# The second kit's reference assembly: five facade shells (walls, gable, two
# roof slopes, ridge, corner trims and a porch deck with posts), the two new
# trees, two streetlights and a porch railing run with both a corner and an
# end. Every placement below is the convention the prop notes document:
#
# * a shell is a 3 m bay with 3 m returns; its front and back walls carry the
#   gable, the two roof slopes are rotated +/-90 degrees with their eave at
#   local x = +/-1.31 and their origin 1.98 m above the floor (the roof plane
#   then passes through the 2.7 m wall top at x = 1.62 and the gable apex at
#   x = 0), and the ridge cap seats 0.22 m under that plane;
# * the porch deck's origin sits 0.75 m out from the wall face and the two
#   porch posts stand at the deck's front corners;
# * a railing run starts 0.15 m out from a corner's module point, and each
#   leg ends in a terminal piece whose newel closes the run.

KIT_SHELLS = (
    # Two rows facing each other across a 3.5 m street: the south row (01-03)
    # faces north, the north row (04-05) faces south, so every porch is
    # visible from the street between them.
    ("01", 13.2, 17.4, 180.0),
    ("02", 16.8, 17.4, 180.0),
    ("03", 20.4, 17.4, 180.0),
    ("04", 18.6, 7.6, 0.0),
    ("05", 22.2, 7.6, 0.0),
)

#: The yaw of each shell's four walls and two gables, in shell-local degrees.
_SHELL_FRONT, _SHELL_BACK = 0.0, 180.0
_SHELL_EAST, _SHELL_WEST = 90.0, 270.0
#: Corner board rotation per local corner: its outer faces look at -X/-Z, so
#: the four corners need 0/90/180/270 degrees.
_CORNER_ROTATIONS = {(-1.5, -1.5): 0.0, (-1.5, 1.5): 90.0, (1.5, 1.5): 180.0, (1.5, -1.5): 270.0}


def _rotated(x, z, yaw, local_x, local_z):
    """Local kit offset to world, using the prop yaw convention.

    A prop at yaw 0 has +Z towards world +Z and +X towards world +X; yaw is a
    right-handed rotation about Y, so a local point maps to
    ``(x + lx*cos + lz*sin, z - lx*sin + lz*cos)``.
    """
    radians = math.radians(yaw)
    return (x + local_x * math.cos(radians) + local_z * math.sin(radians),
            z - local_x * math.sin(radians) + local_z * math.cos(radians))


def kit_shell(number: str, x: float, z: float, yaw: float) -> List[Dict]:
    """One assembled facade shell for family ``number`` at ``(x, z)``."""
    props: List[Dict] = []

    def place(model, local_x, local_z, local_yaw=0.0, y=0.0, **kwargs):
        world_x, world_z = _rotated(x, z, yaw, local_x, local_z)
        props.append(prop(model, world_x, world_z,
                          rotation=(yaw + local_yaw) % 360.0, y=y,
                          identifier=f"kit_house_{number}_{len(props)}", **kwargs))

    # Walls: a real doorway on the front, the type's window on both side
    # returns (so the window pattern reads from the street in perspective),
    # and a plain panel closing the back.
    place(f"outdoor:house_{number}_wall_doorway", 0.0, 1.5)
    place(f"outdoor:house_{number}_wall_window", 1.5, 0.0, _SHELL_EAST)
    place(f"outdoor:house_{number}_wall_window", -1.5, 0.0, _SHELL_WEST)
    place(f"outdoor:house_{number}_wall_solid", 0.0, -1.5, _SHELL_BACK)
    # Gable panels on both ends of the bay's roof.
    place(f"outdoor:house_{number}_gable", 0.0, 1.5, _SHELL_FRONT, y=2.7)
    place(f"outdoor:house_{number}_gable", 0.0, -1.5, _SHELL_BACK, y=2.7)
    # Corner boards on the four vertical corners.
    for (corner_x, corner_z), rotation in _CORNER_ROTATIONS.items():
        place(f"outdoor:house_{number}_corner_trim", corner_x, corner_z, rotation)
    # The roof: two slopes rotated +/-90 degrees meeting under the ridge cap.
    place(f"outdoor:house_{number}_roof_slope", 1.31, 0.0, 90.0, y=1.98)
    place(f"outdoor:house_{number}_roof_slope", -1.31, 0.0, 270.0, y=1.98)
    place(f"outdoor:house_{number}_roof_ridge", 0.0, 0.0, 90.0, y=3.51)
    # The porch: deck 0.75 m out from the wall face, posts on its front corners.
    place(f"outdoor:house_{number}_porch_deck", 0.0, 2.37)
    place(f"outdoor:house_{number}_porch_post", 1.43, 3.05)
    place(f"outdoor:house_{number}_porch_post", -1.43, 3.05)
    return props


def kit_yard() -> List[Dict]:
    """The expanded kit's yard: shells, trees, streetlights and railings."""
    props: List[Dict] = []

    for number, x, z, yaw in KIT_SHELLS:
        props.extend(kit_shell(number, x, z, yaw))

    # The two new trees: a birch and a conifer among the medium grass band,
    # each placed solid with the trunk-sized collider its notes document.
    props.append(
        prop("outdoor:tree_02", 3.6, 12.2, rotation=15.0, scale=1.05,
             size=[0.6, 6.2, 0.6], solid=True, occludes=False, identifier="kit_birch")
    )
    props.append(
        prop("outdoor:tree_03", 6.0, 16.2, rotation=200.0, scale=1.1,
             size=[0.6, 6.8, 0.6], solid=True, occludes=False, identifier="kit_conifer")
    )

    # Two streetlights: one head reaches west over the dirt path, the other
    # closes the east end of the shell street and lights both rows. The light
    # offset is the asset's real pane centre (see the report's streetlight
    # anchor note: the catalogue's [0, 6.05, 0.85] cannot sit under the pane
    # of a 1.2 m deep, bounding-box-centred model).
    for index, (x, z, rotation) in enumerate(((11.5, 13.2, 270.0), (24.6, 13.0, 270.0))):
        props.append(
            prop(
                "outdoor:streetlight", x, z, rotation=rotation,
                identifier=f"kit_streetlight_{index + 1}",
                lights=[{
                    "shape": "point",
                    "offset": [0.0, 6.05, 0.43],
                    "color": [1.0, 0.84, 0.66],
                    "intensity": 1.5,
                    "range": 14.0,
                    "falloff": "smooth",
                }],
            )
        )

    # A railing run in the south-east corner: one corner with its support
    # post, two straight modules and two terminals, so the 90 degree join and
    # the run ends are both visible from the yard.
    corner_x, corner_z = 23.0, 16.4
    props.append(prop("outdoor:porch_post", corner_x, corner_z,
                      identifier="kit_rail_post", comment=(
                          "Railing support on the module corner: the corner piece wraps it.")))
    props.append(prop("outdoor:porch_railing_corner", corner_x, corner_z,
                      identifier="kit_rail_corner"))
    props.append(prop("outdoor:porch_railing_straight", corner_x + 1.05, corner_z,
                      identifier="kit_rail_east_1"))
    props.append(prop("outdoor:porch_railing_end", corner_x + 2.10, corner_z,
                      identifier="kit_rail_east_end"))
    props.append(prop("outdoor:porch_railing_straight", corner_x, corner_z + 1.05,
                      identifier="kit_rail_south_1"))
    props.append(prop("outdoor:porch_railing_end", corner_x, corner_z + 2.10,
                      identifier="kit_rail_south_end"))
    return props


def wall(x, z, width, depth, height=2.7, material=None, openings=None) -> Dict:
    entry: Dict = {"x": x, "z": z, "width": width, "depth": depth, "height": height}
    if material:
        entry["material"] = material
    if openings:
        entry["openings"] = openings
    return entry


def prop(model, x, z, rotation=0.0, y=0.0, scale=1.0, solid=None, size=None,
         occludes=None, lights=None, components=None, bindings=None, identifier=None,
         comment=None) -> Dict:
    entry: Dict = {"model": model, "x": x, "z": z}
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
    if occludes is not None:
        entry["occludes"] = occludes
    if identifier:
        entry["id"] = identifier
    if components:
        entry["components"] = components
    if bindings:
        entry["bindings"] = bindings
    if lights:
        entry["lights"] = lights
    if comment:
        entry["comment"] = comment
    return entry


def lamp_light(offset, intensity, reach) -> Dict:
    """One lamp's documented profile: a warm point just in front of the pane."""
    return {
        "shape": "point",
        "offset": offset,
        "color": [1.0, 0.86, 0.68],
        "intensity": intensity,
        "range": reach,
        "falloff": "smooth",
    }


def half_wall(x, z, width, depth) -> Dict:
    return {
        "x": x,
        "z": z,
        "width": width,
        "depth": depth,
        "height": 1.05,
        "material": "outdoor:house_siding_01",
    }


def structure() -> Dict:
    panel_centres = [4.0, 7.0, 10.0, 13.0, 16.0]
    props: List[Dict] = []

    # --- house facade: five panels, a real doorway at the centre -----------
    for centre in panel_centres:
        if centre == 10.0:
            model, depth = "outdoor:house_wall_doorway", 0.40
        elif centre in (4.0, 16.0):
            model, depth = "outdoor:house_wall_window", 0.24
        else:
            model, depth = "outdoor:house_wall_solid", 0.24
        # Back face flush with the wall's front face at z = 1.15.
        props.append(prop(model, centre, 1.15 + depth * 0.5, identifier=f"facade_{int(centre)}"))
    for x in (2.5, 17.5):
        props.append(prop("outdoor:house_corner_trim", x, 1.0))

    # --- roof: two slopes meeting at the ridge cap --------------------------
    props.append(prop("outdoor:house_roof_slope", 10.0, 1.2, y=2.7))
    props.append(prop("outdoor:house_roof_slope", 10.0, -4.0, rotation=180.0, y=2.7))
    props.append(prop("outdoor:house_roof_ridge", 10.0, -1.4, y=4.23))

    # --- doorway lamps on the documented eave mounts ------------------------
    for side in (-1.0, 1.0):
        x = 10.0 + side * 1.15
        props.append(
            prop(
                "outdoor:lamp_wall", x, 1.55, y=2.03,
                identifier=f"door_wall_lamp_{'left' if side < 0 else 'right'}",
                lights=[lamp_light([0.0, 0.18, 0.12], 0.6, 5.0)],
            )
        )

    # --- concrete step and walkway, then the dirt path ----------------------
    props.append(
        prop("outdoor:concrete_step", 10.0, 1.8, size=[1.4, 0.18, 0.7], solid=True)
    )

    # --- tree and fence run -------------------------------------------------
    props.append(
        prop("outdoor:tree_01", 5.5, 5.5, rotation=20.0, scale=1.05,
             size=[0.8, 6.4, 0.8], solid=True, occludes=False, identifier="yard_tree")
    )
    for index, z in enumerate(range(3, 18, 2)):
        props.append(prop("outdoor:fence_post", 1.2, z, identifier=f"fence_post_{index + 1}"))
        if z in (5, 9, 13):
            props.append(
                prop(
                    "outdoor:lamp_fence", 1.2, z, y=1.05,
                    identifier=f"fence_lamp_{index + 1}",
                    lights=[lamp_light([0.0, 0.30, 0.05], 0.5, 4.0)],
                )
            )

    # --- standing lamps along the path --------------------------------------
    for index, (x, z) in enumerate([(8.2, 6.0), (11.8, 11.0), (8.2, 16.0), (13.4, 3.6)]):
        props.append(
            prop(
                "outdoor:lamp_stand", x, z, identifier=f"path_lamp_{index + 1}",
                lights=[lamp_light([0.0, 0.86, 0.0], 0.7, 7.0)],
            )
        )

    # --- buried containment collider along the yard's east boundary ---------
    # The peg is a deliberately tiny 6 cm cube sunk 6 cm below the floor, so
    # its drawn geometry is never visible and the bake never sees it; the level
    # sizes its solid box (0.3 x 3.0 m) flush along the east perimeter rail,
    # up to the north-east corner, to contain the player there.
    props.append(
        prop(
            "outdoor:collision_peg", 25.55, 18.2, y=-0.06,
            size=[0.3, 3.0, 3.0], solid=True, occludes=False,
            identifier="kit_boundary_peg",
            comment="Buried invisible containment collider: a 6 cm peg sunk 6 cm below the "
                    "floor, so only its level-sized solid box exists. The box lies flush "
                    "along the east perimeter wall up to the north-east corner; "
                    "occludes: false keeps it out of the bake.",
        )
    )

    return {"props": props}


def grass_props() -> List[Dict]:
    """The three density bands, from the shipped scatter tool."""
    out: List[Dict] = []
    for density, area in (("low", GRASS_LOW), ("medium", GRASS_MEDIUM), ("dense", GRASS_DENSE)):
        out.extend(
            scatter_grass.scatter(
                [area],
                KEEP_OUTS,
                density,
                SEEDS[density],
                ["outdoor:grass_patch_small", "outdoor:grass_patch_large"],
                f"grass_{density}_",
            )
        )
    return out


def level() -> Dict:
    panels = {
        "wall": "outdoor:house_siding_01",
        "floor": "outdoor:grass_ground_01",
        "ceiling": "home:ceiling_white_01",
    }
    structure_level = structure()
    grass = grass_props()

    house_walls = [
        wall(2.5, 0.85, 15.0, 0.3, height=2.7,
             openings=[{"kind": "door", "offset": 6.95, "width": 1.1,
                        "height": 2.15, "sill": 0.0}]),
        wall(2.5, -4.1, 15.0, 0.3),
        wall(2.2, -4.1, 0.3, 5.25),
        wall(17.5, -4.1, 0.3, 5.25),
    ]
    shed_walls = [
        wall(25.7, 6.0, 0.3, 5.0,
             openings=[{"kind": "door", "offset": 2.5, "width": 1.2,
                        "height": 2.1, "sill": 0.0}]),
        wall(26.0, 5.7, 5.0, 0.3, height=2.6),
        wall(26.0, 10.7, 5.0, 0.3, height=2.6),
        wall(30.7, 6.0, 0.3, 5.0, height=2.6),
    ]
    # The horizontal runs own the yard corners; side runs meet their ends.
    # House side walls own the two bottom terminal strips. These ownership
    # boundaries keep the same occupied solid union without doubled faces.
    perimeter = [
        half_wall(0.0, 19.8, 26.0, 0.2),
        half_wall(0.0, 0.2, 0.2, 19.6),
        half_wall(25.8, 0.2, 0.2, 5.8),
        half_wall(25.8, 11.0, 0.2, 8.8),
        half_wall(0.0, 0.0, 2.2, 0.2),
        half_wall(17.8, 0.0, 8.2, 0.2),
    ]

    return {
        "format_version": 3,
        "id": "outdoor_kit_showcase",
        "name": "Outdoor Kit Showcase (dev)",
        "author": "Places Team",
        "sky": {
            "texture": "outdoor:tex_sky_stars_01",
            "brightness": 1.0,
            "ambient": 0.05,
        },
        "spawn": {"x": 10.0, "z": 16.0, "yaw_degrees": 0.0},
        "defaults": dict(panels),
        "rooms": [
            {**YARD, "ceiling": {"kind": "open"}, "material": "outdoor:grass_ground_01"},
            {**HOUSE, "ceiling": {"kind": "gable", "ridge": "x", "ridge_rise": RIDGE_RISE},
             "material": "home:hardwood_oak_01",
             "ceiling_material": "home:ceiling_white_01"},
            {**SHED, "material": "outdoor:concrete_pavement_01",
             "ceiling_material": "home:ceiling_white_01"},
        ],
        "walls": house_walls + shed_walls,
        "half_walls": perimeter,
        "floor_patches": [
            {"x": 9.2, "z": 4.2, "width": 1.6, "depth": 14.4,
             "material": "outdoor:dirt_gravel_01"},
            {"x": 8.0, "z": 1.6, "width": 4.0, "depth": 2.6,
             "material": "outdoor:concrete_pavement_01"},
        ],
        "decals": path_feathers(),
        "ceiling_lights": [
            {"fixture": "home:ceiling_light_round", "x": 28.5, "z": 8.6,
             "brightness": 0.8, "color": [1.0, 0.86, 0.7]},
        ],
        "doors": [
            {
                "id": "front_door",
                "x": 9.45,
                "y": 0.0,
                "z": 1.0,
                "rotation_degrees": 0.0,
                "width": 1.1,
                "height": 2.15,
                "kind": "interior",
                "components": [{"component": "interactable", "prompt": "Front Door"}],
                "bindings": [{"on": "interact", "actions": [{"action": "toggle"}]}],
            }
        ],
        "props": structure_level["props"] + kit_yard() + grass,
    }


def path_feathers() -> List[Dict]:
    """The path's feathered border: both sides, an end and two corners.

    The edge sheet is opaque on one side of its width and fades to alpha 0 on
    the other, so each side of the path uses the rotation that lays the opaque
    half over the dirt and the fade over the grass (left border 180 degrees,
    right border 0 here); swapping the sides swaps the rotations. The fixture's
    grazing captures validate the orientation.
    """
    decals: List[Dict] = []
    for index in range(8):
        z = 5.2 + 1.8 * index
        decals.append({"x": 9.2, "y": 0.0, "z": z, "width": 1.0, "height": 2.0,
                       "rotation_degrees": 180.0,
                       "material": "outdoor:decal_path_edge_01", "surface": "floor"})
        decals.append({"x": 10.8, "y": 0.0, "z": z + 0.9, "width": 1.0, "height": 2.0,
                       "rotation_degrees": 0.0,
                       "material": "outdoor:decal_path_edge_01", "surface": "floor"})

    decals.append({"x": 10.0, "y": 0.0, "z": 18.4, "width": 1.8, "height": 1.8,
                   "rotation_degrees": 0.0,
                   "material": "outdoor:decal_path_end_01", "surface": "floor"})
    decals.append({"x": 9.2, "y": 0.0, "z": 4.2, "width": 1.0, "height": 1.0,
                   "rotation_degrees": 0.0,
                   "material": "outdoor:decal_path_corner_01", "surface": "floor"})
    decals.append({"x": 10.8, "y": 0.0, "z": 4.2, "width": 1.0, "height": 1.0,
                   "rotation_degrees": 90.0,
                   "material": "outdoor:decal_path_corner_01", "surface": "floor"})
    return decals


def render(level_def: Dict) -> str:
    return json.dumps(level_def, indent=2, ensure_ascii=False) + "\n"


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--check", action="store_true", help="verify the shipped fixture matches")
    args = parser.parse_args(argv)

    level_def = level()
    payload = render(level_def)
    if args.check:
        existing = OUTPUT_PATH.read_text(encoding="utf-8") if OUTPUT_PATH.exists() else ""
        if existing == payload:
            print(f"ok {OUTPUT_PATH.relative_to(APP_ROOT)}: current")
            return 0
        print(f"stale {OUTPUT_PATH.relative_to(APP_ROOT)}: regenerate with "
              f"python3 tools/levels/build_outdoor_fixture.py", file=sys.stderr)
        return 1
    tmp = OUTPUT_PATH.with_suffix(".json.tmp")
    tmp.write_text(payload, encoding="utf-8")
    os.replace(tmp, OUTPUT_PATH)
    print(f"wrote {OUTPUT_PATH.relative_to(APP_ROOT)} ({len(level_def['props'])} prop(s))")
    return 0


if __name__ == "__main__":
    sys.exit(main())
