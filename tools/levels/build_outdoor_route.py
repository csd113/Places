#!/usr/bin/env python3
"""Authors the Places Demo night-route outdoor slice (deterministic).

Places Demo keeps its hand-authored interior; this tool owns only the outdoor
night extension that Job 04 added north of the front rooms, and nothing else.
Every element it emits carries an id in the ``night_`` namespace (grass uses
``grass_night_`` so it still matches the grass tools' level-wide filters), and
``--check`` re-derives the slice and fails on drift, exactly like
``tools/levels/scatter_grass.py --check``.

The slice contains:

* the ``night_yard`` open-ceiling grass room and the destination house room;
* the gravel route (``outdoor:dirt_gravel_01``), the parallel concrete walkway
  (``outdoor:concrete_pavement_01``) and the short concrete connector that joins
  them about 30 seconds along the gravel route;
* the source house's exterior read (facade panels, gable roof, eave lamps) and
  the destination house (walls, gable roof, centred doorway with two eave-hung
  lamps, a finished entry room);
* feathered dirt edges, dense grass bands, trees, path lamps, the buried
  invisible containment pegs, and the three Halloween encounters.

Run from the Places repository root::

    python3 tools/levels/build_outdoor_route.py            # apply the slice
    python3 tools/levels/build_outdoor_route.py --check    # verify it is current

Layout facts that the numbers below encode (world metres, -Z north, y=0 ground):

* source doorway centre x = 4.5 through ``walls[0]`` at z = -0.15..0.15;
* gravel route centre x = 4.5, walkway centre x = 13.5 (parallel, offset 9 m);
* connector centre z = -87.4: 87.40 m of ordinary forward walking at the
  measured 3.0 m/s -> 29.1 s;
* destination doorway centre x = 13.5, perpendicular to the walkway's far end.
"""

from __future__ import annotations

import argparse
import copy
import json
import os
import sys
import tempfile
from pathlib import Path
from typing import Dict, List, Optional, Sequence

APP_ROOT = Path(__file__).resolve().parent.parent.parent
LEVEL_PATH = APP_ROOT / "assets" / "levels" / "places_demo.json"
sys.path.insert(0, str(Path(__file__).resolve().parent))

import outdoors_dressing
import scatter_grass  # noqa: E402  (the directory is on sys.path above)

# --------------------------------------------------------------------------
# Layout constants
# --------------------------------------------------------------------------

#: The source house's north face (wall 0 spans x -0.15..19.15, wall 3 continues
#: to x 24.15): the yard is exactly this wide so the player can never walk
#: around either corner and see an undressed side.
FACADE_X0 = -0.15
FACADE_X1 = 24.15
FACADE_Z = -0.15  # the outer face of walls[0]/walls[3]

#: The source doorway: a real opening in walls[0], centred on a facade panel.
SOURCE_DOOR_X = 4.5
DOOR_WIDTH = 1.1
DOOR_HEIGHT = 2.15

#: The yard: one open-ceiling grass room from the source facade to the
#: destination house, at the world y=0 the front rooms already use.
YARD = {
    "x": FACADE_X0,
    "z": -91.85,
    "width": FACADE_X1 - FACADE_X0,
    "depth": 91.85,
    "height": 6.0,
}
YARD_Z1 = 0.0  # the south edge: the shared seam at walls[0]'s centre plane

#: The gravel route and the concrete walkway run the full length, 9 m apart.
ROUTE_X = SOURCE_DOOR_X
ROUTE_WIDTH = 2.6
WALKWAY_X = ROUTE_X + 9.0
WALKWAY_WIDTH = 2.0
ROUTE_Z0 = -91.6
ROUTE_Z1 = -0.2

#: The connector: about 30 seconds along the gravel route.
CONNECTOR_Z = -87.4
CONNECTOR_DEPTH = 2.2
CONNECTOR_X0 = ROUTE_X + ROUTE_WIDTH * 0.5
CONNECTOR_X1 = WALKWAY_X - WALKWAY_WIDTH * 0.5

#: The destination house, centred on the walkway.
HOUSE_CX = WALKWAY_X
HOUSE_X0 = HOUSE_CX - 4.5
HOUSE_X1 = HOUSE_CX + 4.5
HOUSE_Z0 = -97.1
HOUSE_Z1 = -91.7
PANEL_PITCH = 3.0

#: The entry room's ceiling pitch follows the roof props (1.75 m rise over a
#: 2.6 m slope).
ROOF_PITCH = 1.75 / 2.6

#: The roof planes are placed so their undersides are exactly the room's gable
#: ceiling planes: the room's eave is where the roof plane crosses the room's
#: own front edge (z = -91.85) with the roof eave underside at y = 2.7 at
#: z = -91.65. Solving for the room height gives 2.8346, and the side and
#: front/back walls then simply follow the ceiling, so the walls, gables and
#: roof form one watertight envelope with no rake slot.
HOUSE_EAVE = 2.7
HOUSE_EDGE_Z = HOUSE_Z1 - 0.15  # the room's own front edge (-91.85)
HOUSE_EAVE_Z = HOUSE_Z1 + 0.05  # the front roof eave underside (-91.65)
HOUSE_HEIGHT = round(HOUSE_EAVE + (HOUSE_EAVE_Z - HOUSE_EDGE_Z) * ROOF_PITCH, 4)
#: The back roof eave mirrors the front one across the room's ridge.
HOUSE_BACK_EAVE_Z = round(HOUSE_Z0 - 0.05, 4)
HOUSE_RIDGE_Z = round((HOUSE_Z0 + HOUSE_Z1) * 0.5, 4)
HOUSE_RIDGE_Y = round(HOUSE_HEIGHT + ROOF_PITCH * 2.55, 4)

#: The family roof slope's half-run from its centred origin to the eave
#: underside (the model spans local z -1.3..+1.3).
ROOF_HALF_RUN = 1.3
#: The stoop's raised floor height; props whose base belongs on the yard floor
#: author a negative y of this much because prop `y` is floor-relative.
STOOP_RISE = 0.24

#: Every emitted id starts with this prefix; grass uses ``grass_night_``.
ID_PREFIX = "night_"

#: Marks this tool's shared comment text, so un-keyed entries (rooms, walls,
#: floor patches, geometry intents) can be recognised across revisions.
SLICE_MARKER = "[night-route]"

#: The slice's own ground plan: ``(x0, z0, x1, z1)``. The base demo has nothing
#: inside it, so this tool owns every wall, room, floor patch and decal that
#: lies entirely within it — which is what lets ``--check`` and re-runs replace
#: a previous revision's geometry instead of accumulating it.
SLICE_REGION = (-1.0, -98.5, 26.5, 0.05)

#: The sky: near-black with faint stars, and no ambient fill of any kind.
SKY = {"texture": "outdoor:tex_sky_stars_01", "brightness": 1.0, "ambient": 0.0}

GRASS_MODELS = ["outdoor:grass_patch_small", "outdoor:grass_patch_large"]

#: ``night_tree_`` is written through a name so the id stays greppable.
ID_NAME_TREE = f"{ID_PREFIX}tree_"

# --------------------------------------------------------------------------
# Layout data
# --------------------------------------------------------------------------

#: Facade panels, in metres of their centre x. Every third panel is a window;
#: the doorway panel sits over the real opening. Eight panels of the kit's 3 m
#: module cover the 24.3 m north face.
FACADE_PANELS = [
    (1.5, "window"),
    (4.5, "doorway"),
    (7.5, "solid"),
    (10.5, "window"),
    (13.5, "solid"),
    (16.5, "window"),
    (19.5, "solid"),
    (22.5, "window"),
]

#: Roof slopes (8 x the kit's 3.4 m panel at 0.9 scale = 24.5 m) and the ridge
#: caps that close them. The eave hangs 0.10 m proud of the wall face.
ROOF_SCALE = 0.9
ROOF_X_STEP = 3.4 * ROOF_SCALE
ROOF_X0 = 12.0 - ROOF_X_STEP * 3.5  # 8 slopes centred on the facade
ROOF_EAVE_Z = FACADE_Z - 0.10
ROOF_EAVE_Y = 2.7
ROOF_RISE = 1.75 * ROOF_SCALE
ROOF_RIDGE_DROP = 0.22 * ROOF_SCALE

#: Path lamps: 12 stand lamps at 7.5 m spacing, alternating sides of the route.
LAMP_Z_STEP = 7.5
LAMP_Z0 = -5.0
LAMP_WEST_X = 2.6
LAMP_EAST_X = 6.4
LAMP_LIGHT = {
    "shape": "point",
    "offset": [0.0, 0.86, 0.0],
    "color": [1.0, 0.86, 0.68],
    "intensity": 0.7,
    "range": 7.0,
    "falloff": "smooth",
}

#: Doorway lamps: the kit's documented mount and light profile.
DOOR_LAMP_LIGHT = {
    "shape": "point",
    "offset": [0.0, 0.18, 0.12],
    "color": [1.0, 0.86, 0.68],
    "intensity": 0.6,
    "range": 5.0,
    "falloff": "smooth",
}
DOOR_LAMP_MOUNT_X = 1.15
DOOR_LAMP_MOUNT_Y = 2.55
#: The doorway panel's half depth. The kit documents its lamp mount at local
#: [1.15, 2.55, 0.20] measured from the panel's centre plane, i.e. exactly on
#: the panel's outer face, so the lamp origin sits one panel depth out from the
#: wall face and never inside the panel's 0.40 m of jamb.
DOOR_PANEL_HALF_DEPTH = 0.20
DOOR_LAMP_MOUNT_Z = 0.20
#: Wall face to lamp origin: the panel's outer face (centre + half depth).
DOOR_LAMP_OUT = DOOR_PANEL_HALF_DEPTH + DOOR_LAMP_MOUNT_Z

#: Trees: ``(x, z, rotation, scale, model)``. Trunks are solid; the canopy opts
#: out of the bake (a coarse box shadow no leaf card could cast - documented on
#: the group). The three models are the leafy original, the pale birch and the
#: evergreen conifer; the boundary rows give each wall side a rhythm with
#: deliberate repeats, model changes and scale variation, and every entry stays
#: clear of the route, the connector, the doorways and the destination house.
TREE_LEAFY = "outdoor:tree_01"
TREE_BIRCH = "outdoor:tree_02"
TREE_EVERGREEN = "outdoor:tree_03"
TREES = [
    # ---- original grove (unchanged) -----------------------------------
    (1.5, -8.0, 15.0, 1.0, TREE_LEAFY),
    (1.5, -88.0, 210.0, 0.95, TREE_LEAFY),
    (7.4, -25.0, 25.0, 1.05, TREE_LEAFY),
    (10.8, -30.0, 300.0, 0.95, TREE_LEAFY),
    (7.0, -38.0, 190.0, 1.1, TREE_LEAFY),
    (11.4, -41.0, 70.0, 1.0, TREE_LEAFY),
    (10.5, -48.5, 240.0, 0.9, TREE_LEAFY),
    (6.9, -49.5, 130.0, 1.05, TREE_LEAFY),
    (6.6, -52.5, 20.0, 0.95, TREE_LEAFY),
    (11.0, -56.0, 350.0, 1.1, TREE_LEAFY),
    (7.6, -62.0, 100.0, 1.0, TREE_LEAFY),
    (10.4, -68.0, 285.0, 0.95, TREE_LEAFY),
    (6.8, -76.0, 55.0, 1.05, TREE_LEAFY),
    (11.2, -82.0, 160.0, 0.9, TREE_LEAFY),
    (16.5, -12.0, 40.0, 1.0, TREE_LEAFY),
    (21.0, -18.0, 220.0, 1.05, TREE_LEAFY),
    (15.5, -33.0, 310.0, 0.95, TREE_LEAFY),
    (20.5, -40.0, 80.0, 1.0, TREE_LEAFY),
    (17.0, -55.0, 250.0, 1.1, TREE_LEAFY),
    (22.0, -62.0, 120.0, 0.95, TREE_LEAFY),
    (16.0, -74.0, 330.0, 1.05, TREE_LEAFY),
    (20.0, -83.0, 30.0, 1.0, TREE_LEAFY),
    # ---- west boundary wall side --------------------------------------
    (1.0, -15.5, 30.0, 1.05, TREE_BIRCH),
    (1.2, -31.5, 200.0, 0.95, TREE_EVERGREEN),
    (0.9, -35.5, 165.0, 1.1, TREE_LEAFY),
    (1.3, -43.0, 75.0, 0.9, TREE_BIRCH),
    (1.0, -57.5, 245.0, 1.0, TREE_EVERGREEN),
    (1.2, -66.5, 40.0, 1.05, TREE_LEAFY),
    (0.9, -79.0, 190.0, 0.95, TREE_BIRCH),
    (1.4, -86.0, 320.0, 1.1, TREE_EVERGREEN),
    # ---- east boundary wall side --------------------------------------
    (23.0, -9.0, 340.0, 1.0, TREE_EVERGREEN),
    (22.8, -21.0, 160.0, 1.05, TREE_BIRCH),
    (23.2, -28.5, 60.0, 0.95, TREE_LEAFY),
    (22.9, -36.5, 230.0, 1.1, TREE_EVERGREEN),
    (23.1, -47.0, 15.0, 0.9, TREE_BIRCH),
    (22.8, -59.5, 300.0, 1.0, TREE_LEAFY),
    (23.2, -70.0, 105.0, 1.05, TREE_EVERGREEN),
    (22.9, -80.5, 265.0, 0.95, TREE_BIRCH),
]
TREE_SIZES = {
    TREE_LEAFY: [0.8, 6.4, 0.8],
    TREE_BIRCH: [0.6, 6.2, 0.6],
    TREE_EVERGREEN: [0.6, 6.8, 0.6],
}

#: Grass bands: (x, z, width, depth, profile, id suffix, seed).
GRASS_BANDS = [
    (1.0, -91.0, 2.2, 90.6, "medium", "west_dense", 4104),
    (5.8, -91.0, 2.2, 90.6, "medium", "east_dense", 4105),
    (8.0, -91.0, 4.5, 90.6, "low", "middle_medium", 4106),
    (0.0, -91.0, 1.0, 90.6, "medium", "west_medium", 4107),
    (14.5, -91.0, 9.5, 90.6, "low", "east_medium", 4108),
]

#: The Halloween encounters.
PUMPKIN = {
    "id": f"{ID_PREFIX}pumpkin",
    "x": WALKWAY_X,
    "z": -83.0,
    "size": [0.701, 0.654, 0.635],
    "segment_z": (-89.0, -83.0),
    "speed": 0.5,
    "glow": {
        "component": "glow",
        "color": [1.0, 0.52, 0.16],
        "intensity": 0.7,
        "range": 4.5,
        "socket": "flame",
        "fade": False,
    },
}
GHOSTS = [
    {
        "id": f"{ID_PREFIX}ghost_a",
        "start": (1.4, -18.0),
        "route": [(1.4, -18.0), (1.4, -23.0), (0.6, -23.0), (0.6, -18.0)],
    },
    {
        "id": f"{ID_PREFIX}ghost_b",
        "start": (0.7, -45.0),
        "route": [(0.7, -45.0), (1.6, -49.0), (0.5, -52.0), (0.7, -45.0)],
    },
    {
        "id": f"{ID_PREFIX}ghost_c",
        "start": (1.5, -70.0),
        "route": [(1.5, -70.0), (0.6, -73.5), (1.6, -76.5), (1.5, -70.0)],
    },
]
GHOST_SIZE = [0.994, 1.619, 0.716]
SKELETON = {
    "id": f"{ID_PREFIX}skeleton",
    "x": 9.6,
    "z": -52.0,
    "speed": 0.571,
    "wander_radius": 2.0,
    "idle_seconds": 2.0,
    "size": [0.442, 1.808, 0.403],
}

#: Invisible containment: each entry is one buried peg with the authored solid
#: box it carries. ``(x, z, width, depth)`` is the collider footprint; the box
#: is 3.5 m tall from -0.10, far above the 0.85 m jump apex and free of the
#: floor, so it can never become a step and never leaves a ground gap.
BOUNDARY_BOXES = [
    (-0.30, -45.925, 0.30, 91.85),  # west, flanking the source house's west face
    (24.30, -45.925, 0.30, 91.85),  # east, flanking the east corner
    (4.275, -92.00, 9.45, 0.30),  # north, west of the destination house
    (21.225, -92.00, 6.45, 0.30),  # north, east of the destination house
]
BOUNDARY_HEIGHT = 3.5
BOUNDARY_SINK = -0.10

#: Destination-house entry dressing (existing catalog assets only).
HOUSE_PROPS = [
    {"id": f"{ID_PREFIX}house_rug", "model": "core:rug", "x": HOUSE_CX, "z": -94.2},
    {
        "id": f"{ID_PREFIX}house_table",
        "model": "core:table",
        "x": HOUSE_CX - 2.9,
        "z": -94.6,
        "rotation_degrees": 90.0,
        "solid": True,
        "size": [0.8, 0.75, 1.4],
    },
    {
        "id": f"{ID_PREFIX}house_plant",
        "model": "core:plant",
        "x": HOUSE_CX + 3.1,
        "z": -96.0,
        "solid": True,
        "size": [0.4, 1.0, 0.4],
    },
]

#: The yard is an authored open exterior: its perimeter is the invisible
#: containment boxes and its connection to the front room is the new front door,
#: so the checker's wall and leak heuristics are annotated here rather than
#: answered with visible walls (which would block the sky and the lamps).
GEOMETRY_INTENT = [
    {
        "check": "missing-wall",
        "x": FACADE_X0 - 0.4,
        "z": -92.35,
        "width": FACADE_X1 - FACADE_X0 + 0.8,
        "depth": 1.0,
        "note": SLICE_MARKER + " " + (
            "Night yard: the whole north face is the destination house and the "
            "invisible containment colliders, so no wall solid runs along it."
        ),
    },
    {
        "check": "missing-wall",
        "x": FACADE_X0 - 0.5,
        "z": -92.0,
        "width": 0.7,
        "depth": 92.3,
        "note": SLICE_MARKER + " Night yard: the west perimeter is an invisible containment collider, not a wall.",
    },
    {
        "check": "missing-wall",
        "x": FACADE_X1 - 0.2,
        "z": -92.0,
        "width": 0.7,
        "depth": 92.3,
        "note": SLICE_MARKER + " Night yard: the east perimeter is an invisible containment collider, not a wall.",
    },
    {
        "check": "room-leak",
        "x": FACADE_X0 - 0.45,
        "z": -3.0,
        "width": 1.0,
        "depth": 3.2,
        "note": SLICE_MARKER + " " + (
            "The front room's walkable space continues through the new front "
            "door into the open night yard; the west edge is the containment "
            "collider and the source house's own wall."
        ),
    },
    {
        "check": "room-leak",
        "x": FACADE_X1 - 0.25,
        "z": -92.5,
        "width": 3.1,
        "depth": 1.5,
        "note": SLICE_MARKER + " The yard's floor reaches its east containment collider; there is no wall to explain it.",
    },
    {
        "check": "room-leak",
        "x": HOUSE_X1 + 1.3,
        "z": -92.4,
        "width": 0.9,
        "depth": 0.7,
        "note": SLICE_MARKER + " Open yard floor east of the destination house, closed by an invisible containment collider.",
    },
]

#: Tall streetlights: ``(x, z, arm rotation)``. The arm leaves along the
#: prop's local +Z; the rotation turns it over the route (west posts turn +Z
#: onto +X, east posts onto -X). The emitter anchor is the fixture's own
#: documented offset - under the hood, 0.45 m inside the model's bounding box.
STREETLIGHTS = [
    (2.3, -14.0, 90.0),
    (6.7, -26.0, 270.0),
    (2.3, -38.0, 90.0),
    (6.7, -50.0, 270.0),
    (2.3, -62.0, 90.0),
    (6.7, -74.0, 270.0),
    (2.3, -84.0, 90.0),
]
STREETLIGHT_LIGHT = {
    "shape": "point",
    "offset": [0.0, 6.05, 0.43],
    "color": [1.0, 0.84, 0.66],
    "intensity": 1.5,
    "range": 14.0,
    "falloff": "smooth",
}

#: The ghost cat: a small hovering sheet ghost that drifts beside the walkway
#: and fades out when the player comes close (the shared proximity fade).
GHOST_CAT = {
    "id": f"{ID_PREFIX}ghost_cat",
    "start": (15.2, -44.0),
    "route": [(15.2, -44.0), (14.2, -48.0), (15.6, -52.0), (15.2, -44.0)],
    "size": [0.226, 0.323, 0.576],
    "near_radius": 2.2,
    "far_radius": 5.5,
    "fade_out_seconds": 1.2,
    "fade_in_seconds": 1.8,
}

#: The three pumpkin-head skeletons waiting inside the destination house, and
#: the entry zone that wakes them once per entry with the clip that actually
#: removes and reassembles the head.
GUARDS = [
    {"id": f"{ID_PREFIX}guard_a", "x": 10.6, "z": -95.6, "rotation": 0.0},
    {"id": f"{ID_PREFIX}guard_b", "x": 13.5, "z": -95.7, "rotation": 0.0},
    {"id": f"{ID_PREFIX}guard_c", "x": 16.2, "z": -94.6, "rotation": 0.0},
]
GUARD_SIZE = [0.442, 1.808, 0.403]
GUARD_CLIP = "collapse_reassemble"
GUARD_ZONE = {"x": 12.7, "z": -93.0, "width": 1.6, "depth": 1.2}

#: Restrained regional low mist: one thin ground layer over the western and
#: middle yard, well clear of the destination house (which must read dry).
FOG_REGIONS = [
    {
        "id": f"{ID_PREFIX}low_mist",
        "min": [-0.15, -0.30, -90.5],
        "max": [24.15, 0.55, -2.0],
        "density": 0.055,
        "color": [0.58, 0.62, 0.68],
        "falloff_m": 9.0,
        "ground_y": -0.20,
        "top_y": 0.50,
    }
]

#: Invisible containment geometry (the void-wall system): one broad, thin
#: ground slab 2 cm under the yard's own floor, extending far past every
#: reachable boundary, so looking over an edge or down a transition shows a
#: continuous night ground instead of the void. It is non-solid (the buried
#: pegs own containment) and never occludes, so the bake is unchanged.
VOID_WALLS = [
    {
        "id": f"{ID_PREFIX}ground_slab",
        "min": [-6.0, -1.2, -98.0],
        "max": [30.0, -0.08, 3.0],
        "material": "outdoor:dirt_gravel_01",
        "faces": "outward",
        "solid": False,
        "occludes": False,
    }
]

#: Destination-house interior light: three warm residential flush mounts, so
#: the finished room reads noticeably bright through the open doorway and from
#: inside, and the gable ceiling carries real fixtures rather than glow.
HOUSE_LIGHTS = [
    {
        "id": f"{ID_PREFIX}house_light",
        "fixture": "home:ceiling_light_round",
        "x": HOUSE_CX,
        "z": -94.3,
        "brightness": 1.25,
        "color": [1.0, 0.9, 0.78],
        "range": 7.0,
        "emission": 1.1,
        "align": "none",
    },
    {
        "id": f"{ID_PREFIX}house_light_west",
        "fixture": "home:ceiling_light_round",
        "x": HOUSE_CX - 3.1,
        "z": -95.6,
        "brightness": 1.1,
        "color": [1.0, 0.9, 0.78],
        "range": 6.0,
        "emission": 1.0,
        "align": "none",
    },
    {
        "id": f"{ID_PREFIX}house_light_east",
        "fixture": "home:ceiling_light_round",
        "x": HOUSE_CX + 3.1,
        "z": -93.4,
        "brightness": 1.1,
        "color": [1.0, 0.9, 0.78],
        "range": 6.0,
        "emission": 1.0,
        "align": "none",
    },
]


# --------------------------------------------------------------------------
# Helpers
# --------------------------------------------------------------------------


def _room_floor(rooms: Sequence[dict], x: float, z: float) -> float:
    """The room floor under ``(x, z)``, mirroring the engine's lookup.

    Rooms own their rectangle within the engine's 0.01 m edge tolerance and the
    first containing room wins; a point outside every room falls back to the
    global ground plane at 0.0, exactly as ``LevelSurfaces`` does.
    """
    for room in rooms:
        x0 = float(room.get("x", 0.0))
        z0 = float(room.get("z", 0.0))
        x1 = x0 + float(room["width"])
        z1 = z0 + float(room["depth"])
        if x0 - 0.01 <= x <= x1 + 0.01 and z0 - 0.01 <= z <= z1 + 0.01:
            return float(room.get("floor_y", 0.0))
    return 0.0


def _source_ridge_floor(level: dict, x: float, z: float) -> float:
    """Floor at the source ridge, which lies over room/floor-region surfaces.

    The east end is over the stair hall's raised floor regions. Prop y is
    measured from that walkable surface, not from the room's lower base.
    """
    offset = 0.0
    for region in reversed(level.get("floor_regions", [])):
        if (region["x"] <= x <= region["x"] + region["width"]
                and region["z"] <= z <= region["z"] + region["depth"]):
            offset = float(region.get("offset_y", 0.0))
            break
    return _room_floor(level["rooms"], x, z) + offset


def _prop(
    model: str,
    x: float,
    z: float,
    *,
    prop_id: str,
    y: float = 0.0,
    rotation: float = 0.0,
    scale: float = 1.0,
    size: Optional[Sequence[float]] = None,
    solid: bool = False,
    occludes: bool = True,
    lights: Optional[List[dict]] = None,
    components: Optional[List[dict]] = None,
    comment: Optional[str] = None,
) -> dict:
    """One ``props[]`` entry with the fields this slice actually uses."""
    entry: Dict[str, object] = {"id": prop_id, "model": model, "x": round(x, 4), "z": round(z, 4)}
    if y:
        entry["y"] = round(y, 4)
    if rotation:
        entry["rotation_degrees"] = rotation
    if scale != 1.0:
        entry["scale"] = scale
    if size is not None:
        entry["size"] = [round(value, 4) for value in size]
    if solid:
        entry["solid"] = True
    if not occludes:
        entry["occludes"] = False
    if lights:
        entry["lights"] = lights
    if components:
        entry["components"] = components
    if comment:
        entry["comment"] = comment
    return entry


# --------------------------------------------------------------------------
# The slice
# --------------------------------------------------------------------------


def build_slice(level: dict) -> dict:
    """Builds the outdoor slice as arrays keyed by level field.

    Returns a fresh dict; the caller merges it into a level. ``level`` is read
    only for its rooms, so prop ``y`` offsets resolve against the real floors
    the demo already has.
    """
    rooms: List[dict] = []
    walls: List[dict] = []
    floor_patches: List[dict] = []
    floor_regions: List[dict] = []
    props: List[dict] = []
    decals: List[dict] = []
    doors: List[dict] = []
    routes: List[dict] = []
    ceiling_lights: List[dict] = []
    volumes: List[dict] = []
    sequences: List[dict] = []
    fog_regions: List[dict] = []
    void_walls: List[dict] = []

    # ---- the yard -------------------------------------------------------
    rooms.append(
        {
            "x": YARD["x"],
            "z": YARD["z"],
            "width": YARD["width"],
            "depth": YARD["depth"],
            "height": YARD["height"],
            "ceiling": {"kind": "open"},
            "material": "outdoor:grass_ground_01",
            "comment": SLICE_MARKER + " " + (
                "The night route's exterior: grass ground at y=0, open to the "
                "faint-star sky, lit only by the path and doorway lamps."
            ),
        }
    )

    # ---- the destination house -----------------------------------------
    house_room = {
        "x": HOUSE_X0 + 0.15,
        "z": HOUSE_Z0 + 0.15,
        "width": (HOUSE_X1 - HOUSE_X0) - 0.3,
        "depth": (HOUSE_Z1 - HOUSE_Z0) - 0.3,
        # The eave is solved from the roof plane (see HOUSE_HEIGHT): the
        # ceiling and the roof's underside are the same plane, so every wall
        # that follows the ceiling is also sealed against the roof.
        "height": HOUSE_HEIGHT,
        "ceiling": {"kind": "gable", "ridge": "x", "ridge_rise": round(ROOF_PITCH * 2.55, 4)},
        "material": "home:hardwood_oak_01",
        "ceiling_material": "home:ceiling_white_01",
        "comment": "Finished entry room behind the destination doorway.",
    }
    rooms.append(house_room)

    door_offset = HOUSE_CX - DOOR_WIDTH * 0.5 - HOUSE_X0
    walls.extend(
        [
            {
                # No authored height: the front wall's top follows the eave
                # ceiling, which is the roof's underside.
                "x": HOUSE_X0,
                "z": HOUSE_Z1 - 0.3,
                "width": 9.0,
                "depth": 0.3,
                # A rigid eave height: this wall is parallel to the ridge, so
                # its top is the constant eave plane, and an authored height
                # keeps it there instead of trusting the room lookup at a
                # boundary shared with the yard's open 6 m ceiling.
                "height": HOUSE_HEIGHT,
                "material": "outdoor:house_siding_01",
                "faces": {"north": "home:wall_paint_offwhite_01"},
                "openings": [
                    {
                        "kind": "door",
                        "offset": round(door_offset - OPENING_CLEARANCE, 4),
                        "width": round(DOOR_WIDTH + OPENING_CLEARANCE * 2.0, 4),
                        "height": DOOR_HEIGHT,
                        "sill": 0.0,
                    }
                ],
                "comment": "Destination front wall: the centred doorway under a rigid eave-height top.",
            },
            {
                "x": HOUSE_X0,
                "z": HOUSE_Z0,
                "width": 9.0,
                "depth": 0.3,
                "height": HOUSE_HEIGHT,
                "material": "outdoor:house_siding_01",
                "faces": {"south": "home:wall_paint_offwhite_01"},
                "comment": "Destination back wall: a rigid eave height, like the front wall.",
            },
            {
                # The gable end walls: with no authored height their tops
                # follow the room's gable ceiling, which is exactly the roof
                # plane, so wall, rake and roof meet with no slot.
                "x": HOUSE_X0,
                # 2 cm short of each front/back wall's inner face: the gable
                # end faces stay clear of the perpendicular walls instead of
                # landing exactly on their plane (a coincident surface), and
                # the corner reads as an ordinary 2 cm return.
                "z": HOUSE_Z0 + 0.32,
                "width": 0.3,
                "depth": 4.76,
                "material": "outdoor:house_siding_01",
                "faces": {"east": "home:wall_paint_offwhite_01"},
                "comment": "West gable wall: its top follows the roof plane to the ridge.",
            },
            {
                "x": HOUSE_X1 - 0.3,
                "z": HOUSE_Z0 + 0.32,
                "width": 0.3,
                "depth": 4.76,
                "material": "outdoor:house_siding_01",
                "faces": {"west": "home:wall_paint_offwhite_01"},
                "comment": "East gable wall: its top follows the roof plane to the ridge.",
            },
        ]
    )

    # Front facade panels: three modules of the blue-clapboard family (house
    # 02), the centred one carrying the real door; a covered stoop dresses the
    # destination with the same kit. The five families stay available in the
    # catalogue and the kit fixture; the demo needs only one.
    HOUSE_FACADE = "outdoor:house_02"
    # The stoop is a raised floor region under the porch; panels, lamps and the
    # deck must still meet the yard floor (their authored `y` is relative to
    # the local walkable floor), so their origin is lowered by the stoop rise.
    for x, kind in ((10.5, "window"), (13.5, "doorway"), (16.5, "window")):
        props.append(
            _prop(
                f"{HOUSE_FACADE}_{'wall_' + kind}",
                x,
                HOUSE_Z1 + (0.20 if kind == "doorway" else 0.12),
                prop_id=f"{ID_PREFIX}house_facade_{int(x * 10)}",
                y=-STOOP_RISE,
                comment=(
                    "Centred destination doorway panel; its 0.40 m jamb depth "
                    "reads on both faces of the real wall opening."
                    if kind == "doorway"
                    else "Destination facade panel (blue clapboard family)."
                ),
            )
        )

    # The stoop is real floor: a 0.24 m raised region outside the door, so the
    # step rule walks the player up and down it (a rigid prop box would block).
    floor_regions.append(
        {
            "x": HOUSE_CX - 1.5,
            # 1 cm south of the wall's outer face: the stoop's 0.24 m skirt
            # would otherwise be exactly coplanar with the wall face above it.
            "z": HOUSE_Z1 + 0.01,
            "width": 3.0,
            "depth": 1.5,
            "offset_y": 0.24,
            "material": "outdoor:concrete_pavement_01",
            "edge_material": "outdoor:concrete_pavement_01",
            "comment": (
                "[night-route] The destination doorway's 0.24 m stoop: real "
                "raised floor, so the step rule walks it; the porch deck prop "
                "dresses this exact footprint."
            ),
        }
    )
    # A three-module porch deck outside the door, carried by two posts.
    props.append(
        _prop(
            f"{HOUSE_FACADE}_porch_deck",
            HOUSE_CX,
            HOUSE_Z1 + 0.75,
            prop_id=f"{ID_PREFIX}house_porch",
            y=-STOOP_RISE - 0.025,
            comment="Porch deck sunk 25 mm below the walkable stoop to avoid coplanar top faces; the 0.24 m step itself is real floor so it walks.",
        )
    )
    for side in (-1, 1):
        props.append(
            _prop(
                f"{HOUSE_FACADE}_porch_post",
                HOUSE_CX + side * 1.43,
                HOUSE_Z1 + 1.43,
                prop_id=f"{ID_PREFIX}house_porch_post_{'w' if side < 0 else 'e'}",
                size=[0.14, 2.3, 0.14],
                solid=True,
                comment="Porch post standing through the deck's front edge.",
            )
        )

    # Corner boards on the destination house.
    for x in (HOUSE_X0, HOUSE_X1):
        for z in (HOUSE_Z1, HOUSE_Z0):
            props.append(
                _prop(
                    f"{HOUSE_FACADE}_corner_trim",
                    x,
                    z,
                    prop_id=f"{ID_PREFIX}house_trim_{int(abs(x) * 10)}_{int(abs(z) * 10)}",
                )
            )

    # Destination roof: two slopes meeting on a ridge cap over the front door.
    # The slopes' undersides are the room's own gable planes (HOUSE_EAVE at
    # HOUSE_EAVE_Z / HOUSE_BACK_EAVE_Z, ridge at HOUSE_RIDGE_Y), so the gable
    # walls that follow the ceiling are sealed against the roof.
    # The family's roof slope is centred: its eave underside sits at local
    # z = +1.3 and its ridge edge at -1.3 (the original kit's panel instead
    # puts the origin *at* the eave). Place the origin half a run inboard of
    # the eave so the underside plane is exactly the room's gable plane.
    slope_x_centers = [HOUSE_CX - 3.4, HOUSE_CX, HOUSE_CX + 3.4]
    for index, x in enumerate(slope_x_centers):
        props.append(
            _prop(
                f"{HOUSE_FACADE}_roof_slope",
                x,
                HOUSE_EAVE_Z - ROOF_HALF_RUN,
                prop_id=f"{ID_PREFIX}house_roof_front_{index}",
                rotation=0.0,
                y=HOUSE_EAVE,
                comment="Front roof slope: its underside is the room's front gable plane.",
            )
        )
        props.append(
            _prop(
                f"{HOUSE_FACADE}_roof_slope",
                x,
                HOUSE_BACK_EAVE_Z + ROOF_HALF_RUN,
                prop_id=f"{ID_PREFIX}house_roof_back_{index}",
                rotation=180.0,
                y=HOUSE_EAVE,
                comment="Back roof slope: its underside is the room's back gable plane.",
            )
        )
        props.append(
            _prop(
                f"{HOUSE_FACADE}_roof_ridge",
                x,
                HOUSE_RIDGE_Z,
                prop_id=f"{ID_PREFIX}house_ridge_{index}",
                y=round(HOUSE_RIDGE_Y - 0.22, 4),
            )
        )

    # Entry-room dressing and its lit ceiling.
    for entry in HOUSE_PROPS:
        props.append(_prop(entry["model"], entry["x"], entry["z"], prop_id=entry["id"],
                           rotation=entry.get("rotation_degrees", 0.0),
                           size=entry.get("size"), solid=entry.get("solid", False)))
    for light in HOUSE_LIGHTS:
        ceiling_lights.append(dict(light))

    # Destination doorway lamps: one each side, hung from the eave mounts.
    for side in (-1, 1):
        x = HOUSE_CX + side * DOOR_LAMP_MOUNT_X
        props.append(
            _prop(
                "outdoor:lamp_wall",
                x,
                HOUSE_Z1 + DOOR_LAMP_OUT,
                prop_id=f"{ID_PREFIX}house_lamp_{'w' if side < 0 else 'e'}",
                y=round(DOOR_LAMP_MOUNT_Y - 0.52 - STOOP_RISE, 4),
                lights=[copy.deepcopy(DOOR_LAMP_LIGHT)],
                occludes=False,
                comment="Hangs from the eave on the doorway mount the kit documents.",
            )
        )

    # Destination door: open, so the walk in and back out needs no key hunt.
    doors.append(
        {
            "id": f"{ID_PREFIX}house_door",
            "x": round(HOUSE_CX - DOOR_WIDTH * 0.5, 4),
            "y": 0.0,
            "z": round(HOUSE_Z1 - 0.15, 4),
            "rotation_degrees": 0.0,
            "width": DOOR_WIDTH,
            "height": DOOR_HEIGHT,
            "thickness": 0.045,
            "open_direction": "left",
            # The hinge stands on the opening's edge and the engine sweeps the
            # leaf's centre plane, so a swing past 90 degrees puts the leaf's
            # own hinge-side quarter inside the jamb: every candidate pose is
            # refused and the leaf can never move. Keep an edge-hinged leaf at
            # 90 degrees or less.
            "swing_degrees": 90.0,
            "open_speed_degrees": 100.0,
            "initial_state": "open",
            "kind": "interior",
            # The visible reveal is not the 0.30 m wall slab alone: the 0.40 m
            # doorway panel 0.20 m in front of it builds the jamb depth too, so
            # the leaf authors the full 0.70 m tunnel through both, centred
            # 0.20 m along its closed normal (+Z).
            "frame_depth": 0.7,
            "frame_center": 0.2,
            "components": [{"component": "interactable", "prompt": "House Door"}],
            "bindings": [{"on": "interact", "actions": [{"action": "toggle"}]}],
            "comment": "The destination doorway's leaf; starts open so the entry space is walkable.",
        }
    )

    # ---- the source house's exterior read -------------------------------
    for x, kind in FACADE_PANELS:
        model = {
            "window": "outdoor:house_wall_window",
            "solid": "outdoor:house_wall_solid",
            "doorway": "outdoor:house_wall_doorway",
        }[kind]
        z = FACADE_Z + (-0.20 if kind == "doorway" else -0.12)
        props.append(
            _prop(
                model,
                x,
                z,
                prop_id=f"{ID_PREFIX}source_facade_{int(x * 10)}",
                comment=(
                    "Source facade doorway panel over the real opening; the "
                    "player walks out through both holes."
                    if kind == "doorway"
                    else "Source facade panel over the exposed north wall."
                ),
            )
        )

    for x in (FACADE_X0, FACADE_X1):
        props.append(
            _prop(
                "outdoor:house_corner_trim",
                x,
                FACADE_Z - 0.12,
                prop_id=f"{ID_PREFIX}source_trim_{int(abs(x) * 10)}",
            )
        )

    for index in range(8):
        x = ROOF_X0 + ROOF_X_STEP * index
        props.append(
            _prop(
                "outdoor:house_roof_slope",
                x,
                ROOF_EAVE_Z + ROOF_HALF_RUN * ROOF_SCALE,
                prop_id=f"{ID_PREFIX}source_roof_{index}",
                rotation=180.0,
                scale=ROOF_SCALE,
                y=ROOF_EAVE_Y,
                comment="North slope of the source house's gable: the model origin is half a run inboard of the eave, rising away from the yard.",
            )
        )
        ridge_z = ROOF_EAVE_Z + 2.6 * ROOF_SCALE
        ridge_y = ROOF_EAVE_Y + ROOF_RISE - ROOF_RIDGE_DROP
        props.append(
            _prop(
                "outdoor:house_roof_ridge",
                x,
                ridge_z,
                prop_id=f"{ID_PREFIX}source_ridge_{index}",
                scale=ROOF_SCALE,
                y=round(ridge_y - _source_ridge_floor(level, x, ridge_z), 4),
            )
        )

    # Source doorway lamps, matching the destination's read.
    for side in (-1, 1):
        x = SOURCE_DOOR_X + side * DOOR_LAMP_MOUNT_X
        props.append(
            _prop(
                "outdoor:lamp_wall",
                x,
                FACADE_Z - DOOR_LAMP_OUT,
                prop_id=f"{ID_PREFIX}source_lamp_{'w' if side < 0 else 'e'}",
                rotation=180.0,
                y=round(DOOR_LAMP_MOUNT_Y - 0.52, 4),
                lights=[copy.deepcopy(DOOR_LAMP_LIGHT)],
                occludes=False,
            )
        )

    doors.append(
        {
            "id": f"{ID_PREFIX}source_door",
            "x": round(SOURCE_DOOR_X - DOOR_WIDTH * 0.5, 4),
            "y": 0.0,
            "z": 0.0,
            "rotation_degrees": 0.0,
            "width": DOOR_WIDTH,
            "height": DOOR_HEIGHT,
            "thickness": 0.045,
            "open_direction": "left",
            # The hinge stands on the opening's edge and the engine sweeps the
            # leaf's centre plane, so a swing past 90 degrees puts the leaf's
            # own hinge-side quarter inside the jamb: every candidate pose is
            # refused and the leaf can never move. Keep an edge-hinged leaf at
            # 90 degrees or less.
            "swing_degrees": 90.0,
            "open_speed_degrees": 100.0,
            "initial_state": "open",
            "kind": "interior",
            # The visible reveal is not the 0.30 m wall slab alone: the 0.40 m
            # facade doorway panel 0.20 m in front of it builds the jamb depth
            # too, so the leaf authors the full 0.70 m tunnel through both,
            # centred 0.20 m back along its closed normal (-Z).
            "frame_depth": 0.7,
            "frame_center": -0.2,
            "components": [{"component": "interactable", "prompt": "Front Door"}],
            "bindings": [{"on": "interact", "actions": [{"action": "toggle"}]}],
            "comment": SLICE_MARKER + " " + (
                "The new front door: starts open so the interior's light spills "
                "through a real, open doorway and the route is never sealed."
            ),
        }
    )

    # ---- the route -------------------------------------------------------
    floor_patches.extend(
        [
            {
                "x": round(ROUTE_X - ROUTE_WIDTH * 0.5, 4),
                "z": ROUTE_Z0,
                "width": ROUTE_WIDTH,
                "depth": round(ROUTE_Z1 - ROUTE_Z0, 4),
                "material": "outdoor:dirt_gravel_01",
                "comment": SLICE_MARKER + " The gravel route: 87.4 m from the door to the connector centre.",
            },
            {
                "x": round(WALKWAY_X - WALKWAY_WIDTH * 0.5, 4),
                "z": ROUTE_Z0,
                "width": WALKWAY_WIDTH,
                "depth": round(ROUTE_Z1 - ROUTE_Z0, 4),
                "material": "outdoor:concrete_pavement_01",
                "comment": SLICE_MARKER + " The concrete walkway, parallel to and 9 m east of the gravel route.",
            },
            {
                "x": round(CONNECTOR_X0, 4),
                "z": round(CONNECTOR_Z - CONNECTOR_DEPTH * 0.5, 4),
                "width": round(CONNECTOR_X1 - CONNECTOR_X0, 4),
                "depth": CONNECTOR_DEPTH,
                "material": "outdoor:concrete_pavement_01",
                "comment": SLICE_MARKER + " The connector: joins the gravel route to the walkway and the house.",
            },
        ]
    )

    # Feathered dirt edges: 2.0 m sheets overlapped by 0.2 m, one per side.
    step = 1.8
    z = ROUTE_Z1 - 1.0
    strip_index = 0
    while z + 1.0 > ROUTE_Z0:
        for side, rotation in ((-1, 180.0), (1, 0.0)):
            x = ROUTE_X + side * ROUTE_WIDTH * 0.5
            decals.append(
                {
                    "x": round(x, 4),
                    "y": 0.0,
                    "z": round(z, 4),
                    "width": 1.0,
                    "height": 2.0,
                    "rotation_degrees": rotation,
                    "material": "outdoor:decal_path_edge_01",
                    "surface": "floor",
                }
            )
        strip_index += 1
        z -= step
    # Ends and the connector's four corners.
    for z_end, rotation in ((ROUTE_Z1 - 0.4, 0.0), (ROUTE_Z0 + 0.4, 180.0)):
        decals.append(
            {
                "x": round(ROUTE_X, 4),
                "y": 0.0,
                "z": round(z_end, 4),
                "width": 1.0,
                "height": 1.0,
                "rotation_degrees": rotation,
                "material": "outdoor:decal_path_end_01",
                "surface": "floor",
            }
        )
    for x in (CONNECTOR_X0, CONNECTOR_X1):
        decals.append(
            {
                "x": round(x, 4),
                "y": 0.0,
                "z": round(CONNECTOR_Z - CONNECTOR_DEPTH * 0.5, 4),
                "width": 1.0,
                "height": 1.0,
                "rotation_degrees": 0.0,
                "material": "outdoor:decal_path_corner_01",
                "surface": "floor",
            }
        )
        decals.append(
            {
                "x": round(x, 4),
                "y": 0.0,
                "z": round(CONNECTOR_Z + CONNECTOR_DEPTH * 0.5, 4),
                "width": 1.0,
                "height": 1.0,
                "rotation_degrees": 180.0,
                "material": "outdoor:decal_path_corner_01",
                "surface": "floor",
            }
        )

    # ---- lamps, trees, grass --------------------------------------------
    for index in range(12):
        z = LAMP_Z0 - LAMP_Z_STEP * index
        x = LAMP_WEST_X if index % 2 == 0 else LAMP_EAST_X
        if index == 11:
            # The connector crosses the route at z -88.5..-86.3: the last lamp
            # moves north of the junction so the crossing has a clear mouth.
            z = CONNECTOR_Z - 3.1
        props.append(
            _prop(
                "outdoor:lamp_stand",
                x,
                z,
                prop_id=f"{ID_PREFIX}lamp_{index:02d}",
                size=[0.34, 1.05, 0.34],
                solid=True,
                lights=[copy.deepcopy(LAMP_LIGHT)],
                occludes=False,
                comment=(
                    "Path lamp: the environmental light along the route is these "
                    "lamps, not any ambient or sky term."
                ),
            )
        )

    for index, (x, z, rotation, scale, model) in enumerate(TREES):
        props.append(
            _prop(
                model,
                x,
                z,
                prop_id=f"{ID_NAME_TREE}{index:02d}",
                rotation=rotation,
                scale=scale,
                size=TREE_SIZES[model],
                solid=True,
                occludes=False,
                comment=(
                    "Night tree: trunk-sized collider, canopy opted out of the "
                    "bake so no leaf blob shadow is ground into the lightmap."
                ),
            )
        )

    # Tall streetlights: one every ~12 m, alternating sides, arm turned over
    # the route. The emitter anchor sits under the downward hood, and the light
    # is a bounded warm point so the pool lands on the route and never reads
    # through the houses.
    for index, (x, z, rotation) in enumerate(STREETLIGHTS):
        props.append(
            _prop(
                "outdoor:streetlight",
                x,
                z,
                prop_id=f"{ID_PREFIX}streetlight_{index:02d}",
                rotation=rotation,
                lights=[copy.deepcopy(STREETLIGHT_LIGHT)],
                occludes=False,
                comment=(
                    "Canopy-height streetlight: warm, bounded, downward light "
                    "from the real fixture under the hood."
                ),
            )
        )

    keep_outs: List[tuple] = [
        (ROUTE_X - ROUTE_WIDTH * 0.5, ROUTE_Z0, ROUTE_WIDTH, ROUTE_Z1 - ROUTE_Z0),
        (WALKWAY_X - WALKWAY_WIDTH * 0.5, ROUTE_Z0, WALKWAY_WIDTH, ROUTE_Z1 - ROUTE_Z0),
        (CONNECTOR_X0, CONNECTOR_Z - CONNECTOR_DEPTH * 0.5,
         CONNECTOR_X1 - CONNECTOR_X0, CONNECTOR_DEPTH),
        (HOUSE_X0 - 0.6, HOUSE_Z0 - 0.6, 10.2, 6.3),
        (3.2, -2.4, 2.6, 2.4),
        (11.9, -92.6, 3.2, 2.6),
    ]
    for x, z, _rotation, _scale, _model in TREES:
        keep_outs.append((x - 0.6, z - 0.6, 1.2, 1.2))
    for index in range(12):
        z = LAMP_Z0 - LAMP_Z_STEP * index
        x = LAMP_WEST_X if index % 2 == 0 else LAMP_EAST_X
        keep_outs.append((x - 0.5, z - 0.5, 1.0, 1.0))
    for x, z, _rotation in STREETLIGHTS:
        keep_outs.append((x - 0.7, z - 0.7, 1.4, 1.4))

    for area_x, area_z, width, depth, profile, suffix, seed in GRASS_BANDS:
        band = scatter_grass.scatter(
            [(area_x, area_z, width, depth)],
            keep_outs,
            profile,
            seed,
            GRASS_MODELS,
            id_prefix=f"grass_night_{suffix}_",
        )
        props.extend(band)

    props.extend(outdoors_dressing.props())

    # ---- invisible containment ------------------------------------------
    for index, (x, z, width, depth) in enumerate(BOUNDARY_BOXES):
        props.append(
            _prop(
                "outdoor:collision_peg",
                x,
                z,
                prop_id=f"{ID_PREFIX}boundary_{index}",
                y=BOUNDARY_SINK,
                size=[width, BOUNDARY_HEIGHT, depth],
                solid=True,
                occludes=False,
                comment=(
                    "Invisible containment: the 6 cm peg is buried below the "
                    "floor, so only its authored solid box exists. occludes: "
                    "false keeps it out of the lightmap bake, and the box is "
                    "taller than the jump apex with no ground gap."
                ),
            )
        )

    # ---- the Halloween encounters ---------------------------------------
    pumpkin_x, pumpkin_z = PUMPKIN["x"], PUMPKIN["z"]
    props.append(
        _prop(
            "carved-pumpkin",
            pumpkin_x,
            pumpkin_z,
            prop_id=PUMPKIN["id"],
            size=PUMPKIN["size"],
            components=[copy.deepcopy(PUMPKIN["glow"])],
        )
    )
    z_far, z_near = PUMPKIN["segment_z"]
    routes.append(
        {
            "id": PUMPKIN["id"],
            "loop": True,
            "steps": [
                {"step": "move_to", "x": pumpkin_x, "z": z_near, "speed": PUMPKIN["speed"]},
                {"step": "wait", "seconds": 0.6},
                {"step": "play", "clip": "laugh", "seconds": 2.4},
                {"step": "move_to", "x": pumpkin_x, "z": z_far, "speed": PUMPKIN["speed"]},
                {"step": "wait", "seconds": 0.6},
                {"step": "play", "clip": "laugh", "seconds": 2.4},
            ],
        }
    )

    for ghost in GHOSTS:
        props.append(
            _prop(
                "sheet-ghost",
                ghost["start"][0],
                ghost["start"][1],
                prop_id=ghost["id"],
                size=GHOST_SIZE,
                components=[
                    {
                        # The proximity form replaces the old cosine cycle: the
                        # ghost fades out as the player comes inside 3 m and
                        # returns only beyond 7 m, with a scale-appropriate
                        # in/out time for a 1.6 m figure.
                        "component": "fade",
                        "min_opacity": 0.0,
                        "max_opacity": 0.85,
                        "near_radius": 3.0,
                        "far_radius": 7.0,
                        "fade_out_seconds": 1.6,
                        "fade_in_seconds": 2.4,
                    },
                    {
                        "component": "glow",
                        "color": [0.4, 0.95, 1.0],
                        "intensity": 0.45,
                        "range": 3.0,
                        "socket": "body",
                        "fade": True,
                    },
                ],
            )
        )
        routes.append(
            {
                "id": ghost["id"],
                "loop": True,
                "steps": [
                    {"step": "move_to", "x": x, "z": z, "speed": 0.3}
                    for (x, z) in ghost["route"]
                ],
            }
        )

    props.append(
        _prop(
            "pumpkin-skeleton",
            SKELETON["x"],
            SKELETON["z"],
            prop_id=SKELETON["id"],
            rotation=90.0,
            size=SKELETON["size"],
            components=[
                {
                    "component": "nav_agent",
                    "radius": 0.25,
                    "height": 1.81,
                    "speed_mps": SKELETON["speed"],
                    "step_height": 0.4,
                    "max_slope": 2.6667,
                },
                {
                    "component": "ai",
                    "behavior": "wanderer",
                    "walk_speed": SKELETON["speed"],
                    "wander_radius": SKELETON["wander_radius"],
                    "idle_seconds": SKELETON["idle_seconds"],
                },
                {
                    "component": "glow",
                    "color": [1.0, 0.62, 0.25],
                    "intensity": 0.55,
                    "range": 3.5,
                    "socket": "piece_pumpkin_head",
                    "fade": False,
                },
            ],
        )
    )

    # ---- the ghost cat ---------------------------------------------------
    props.append(
        _prop(
            "sheet-ghost-cat",
            GHOST_CAT["start"][0],
            GHOST_CAT["start"][1],
            prop_id=GHOST_CAT["id"],
            size=GHOST_CAT["size"],
            components=[
                {
                    # Proximity fade: inside near_radius the cat fades out,
                    # beyond the farther far_radius it fades back in.
                    "component": "fade",
                    "min_opacity": 0.0,
                    "max_opacity": 0.85,
                    "near_radius": GHOST_CAT["near_radius"],
                    "far_radius": GHOST_CAT["far_radius"],
                    "fade_out_seconds": GHOST_CAT["fade_out_seconds"],
                    "fade_in_seconds": GHOST_CAT["fade_in_seconds"],
                },
                {
                    "component": "glow",
                    "color": [0.4, 0.95, 1.0],
                    "intensity": 0.4,
                    "range": 2.6,
                    "socket": "body",
                    "fade": True,
                },
            ],
        )
    )
    routes.append(
        {
            "id": GHOST_CAT["id"],
            "loop": True,
            "steps": [
                {"step": "move_to", "x": x, "z": z, "speed": 0.2}
                for (x, z) in GHOST_CAT["route"]
            ],
        }
    )

    # ---- the three waiting guards ---------------------------------------
    for guard in GUARDS:
        props.append(
            _prop(
                "pumpkin-skeleton",
                guard["x"],
                guard["z"],
                prop_id=guard["id"],
                rotation=guard["rotation"],
                size=GUARD_SIZE,
                components=[
                    {
                        "component": "animation",
                        "clip": GUARD_CLIP,
                        "looped": False,
                        "playing": False,
                    }
                ],
                comment=(
                    "One of the three waiting skeletons inside the house: it "
                    "rests on the authored pose until the entry zone plays "
                    "collapse_reassemble on it."
                ),
            )
        )
    volumes.append(
        {
            "id": f"{ID_PREFIX}guard_zone",
            "x": GUARD_ZONE["x"],
            "z": GUARD_ZONE["z"],
            "width": GUARD_ZONE["width"],
            "depth": GUARD_ZONE["depth"],
            "bottom_y": -0.1,
            "top_y": 2.0,
            "bindings": [
                {
                    "id": "wake",
                    "on": "enter_volume",
                    "cooldown_seconds": 1.0,
                    "when": [
                        {
                            "check": "sequence_idle",
                            "target": f"{ID_PREFIX}guard_zone",
                        }
                    ],
                    "actions": [
                        {
                            "action": "start_sequence",
                            "sequence": f"{ID_PREFIX}guard_wake",
                        }
                    ],
                }
            ],
            "comment": (
                "The house's entry band just inside the doorway. Entering it "
                "starts the shared wake sequence once; because the binding "
                "needs the sequence idle, walking out and back in during the "
                "12 s clip does nothing, and the edge re-arms after exit and "
                "completion. The player's own feet are the only trigger."
            ),
        }
    )
    sequences.append(
        {
            "id": f"{ID_PREFIX}guard_wake",
            "steps": [
                {"step": "wait", "seconds": 0.4},
                {
                    "step": "action",
                    "action": {
                        "action": "play_animation",
                        "target": GUARDS[0]["id"],
                        "clip": GUARD_CLIP,
                        "loop": False,
                    },
                },
                {
                    "step": "action",
                    "action": {
                        "action": "play_animation",
                        "target": GUARDS[1]["id"],
                        "clip": GUARD_CLIP,
                        "loop": False,
                    },
                },
                {
                    "step": "action",
                    "action": {
                        "action": "play_animation",
                        "target": GUARDS[2]["id"],
                        "clip": GUARD_CLIP,
                        "loop": False,
                    },
                },
                {"step": "wait", "seconds": 12.6},
                {"step": "stop"},
            ],
            "comment": (
                "Runs on the entry zone: one step starts all three guards' "
                "collapse_reassemble clips together, then the sequence holds "
                "until the 12 s clip is done so a re-entry cannot interleave."
            ),
        }
    )

    # ---- mist and containment ground ------------------------------------
    for region in FOG_REGIONS:
        fog_regions.append(copy.deepcopy(region))
    for slab in VOID_WALLS:
        void_walls.append(copy.deepcopy(slab))

    return {
        "geometry_intent": [copy.deepcopy(entry) for entry in GEOMETRY_INTENT],
        "rooms": rooms,
        "walls": walls,
        "floor_patches": floor_patches,
        "floor_regions": floor_regions,
        "props": props,
        "decals": decals,
        "doors": doors,
        "routes": routes,
        "ceiling_lights": ceiling_lights,
        "volumes": volumes,
        "sequences": sequences,
        "fog_regions": fog_regions,
        "void_walls": void_walls,
    }


#: The source wall the new doorway is cut into, and the opening itself.
SOURCE_WALL_INDEX = 0
#: How much wider than its leaf a doorway hole is cut, in metres. A leaf that
#: fills its hole exactly can sit one f32 ULP inside the wall's collision slice
#: after rounding, which the loader refuses; 5 mm per side keeps the closed leaf
#: provably clear of both slices and is invisible behind the doorway panel.
OPENING_CLEARANCE = 0.005

SOURCE_OPENING = {
    "kind": "door",
    "offset": round(SOURCE_DOOR_X - DOOR_WIDTH * 0.5 - FACADE_X0 - OPENING_CLEARANCE, 4),
    "width": round(DOOR_WIDTH + OPENING_CLEARANCE * 2.0, 4),
    "height": DOOR_HEIGHT,
    "sill": 0.0,
}



def _is_ours(entry: dict, generated: Sequence[dict], field: str) -> bool:
    """True when one array entry belongs to this tool's slice.

    Four independent rules, because the level format gives rooms, walls, floor
    patches and decals no id: the id namespace, the shared comment/note marker,
    a footprint wholly inside the slice's region, and exact equality with what
    this tool emits today. A hand-authored entry the base demo owns can satisfy
    none of them.
    """
    ident = entry.get("id")
    if isinstance(ident, str) and (ident.startswith(ID_PREFIX) or ident.startswith("grass_night_")):
        return True
    for key in ("comment", "note"):
        text = entry.get(key)
        if isinstance(text, str) and text.startswith(SLICE_MARKER):
            return True
    if field in ("rooms", "walls", "floor_patches", "decals") and _inside_slice_region(entry):
        return True
    return entry in generated


def _inside_slice_region(entry: dict) -> bool:
    """True when a rectangular entry lies entirely inside the slice's region."""
    x = entry.get("x")
    z = entry.get("z")
    if not isinstance(x, (int, float)) or not isinstance(z, (int, float)):
        return False
    width = entry.get("width")
    depth = entry.get("depth")
    if not isinstance(width, (int, float)) or not isinstance(depth, (int, float)):
        # A decal carries width/height; only the two world axes matter here.
        height = entry.get("height")
        if not isinstance(width, (int, float)) or not isinstance(height, (int, float)):
            return False
        depth = height
    x0, x1 = sorted((float(x), float(x) + float(width)))
    z0, z1 = sorted((float(z), float(z) + float(depth)))
    rx0, rz0, rx1, rz1 = SLICE_REGION
    tolerance = 0.01
    return (
        x0 >= rx0 - tolerance
        and x1 <= rx1 + tolerance
        and z0 >= rz0 - tolerance
        and z1 <= rz1 + tolerance
    )


def _is_source_opening(opening: dict) -> bool:
    """True when an opening in the source wall is this tool's front doorway.

    Matched by position rather than by exact values, so a slice written by an
    earlier revision of this tool is replaced instead of duplicated (the loader
    refuses a wall whose openings overlap).
    """
    if opening.get("kind") != "door":
        return False
    start = FACADE_X0 + float(opening.get("offset", 0.0))
    end = start + float(opening.get("width", 0.0))
    return end > SOURCE_DOOR_X - 1.0 and start < SOURCE_DOOR_X + 1.0


def strip_slice(level: dict) -> None:
    """Removes this tool's slice from ``level`` in place, leaving the interior.

    Every id-namespaced element is dropped by prefix; the un-keyed arrays
    (rooms, walls, floor_patches, decals) are matched against the slice this
    tool would emit today, so a hand-edited generated element fails ``--check``
    loudly instead of being silently replaced.
    """
    # Check the document as loaded first: a duplicate that the strip is about to
    # normalise away is still a defect worth failing on loudly.
    _assert_slice_has_no_overlapping_walls(level)
    slice_data = build_slice(level)
    for field in (
        "geometry_intent",
        "rooms",
        "walls",
        "floor_patches",
        "floor_regions",
        "props",
        "decals",
        "doors",
        "routes",
        "ceiling_lights",
        "volumes",
        "sequences",
        "fog_regions",
        "void_walls",
    ):
        existing = level.get(field)
        if not isinstance(existing, list):
            continue
        generated = slice_data[field]
        remaining = []
        for entry in existing:
            if _is_ours(entry, generated, field):
                continue
            remaining.append(entry)
        level[field] = remaining
    # The doorway in walls[0] belongs to this slice.
    wall = level["walls"][SOURCE_WALL_INDEX]
    openings = wall.get("openings", [])
    wall["openings"] = [
        opening for opening in openings if not _is_source_opening(opening)
    ]
    if level.get("sky") == SKY:
        del level["sky"]


def _assert_slice_has_no_overlapping_walls(level: dict) -> None:
    """Fails when two slice-region walls overlap in plan and height.

    A generator revision that changes a wall's footprint cannot match the
    previous revision's entry by value, so the strip rules above are what
    remove it; this check is the tripwire that turns a missed removal into a
    loud failure instead of duplicate architecture in the shipped level.
    """
    region_walls = [
        wall
        for wall in level.get("walls", [])
        if _inside_slice_region(wall) and _is_ours(wall, [], "walls")
    ]
    for index, first in enumerate(region_walls):
        for second in region_walls[index + 1 :]:
            fx0, fx1 = sorted((float(first["x"]), float(first["x"]) + float(first["width"])))
            fz0, fz1 = sorted((float(first["z"]), float(first["z"]) + float(first["depth"])))
            sx0, sx1 = sorted((float(second["x"]), float(second["x"]) + float(second["width"])))
            sz0, sz1 = sorted((float(second["z"]), float(second["z"]) + float(second["depth"])))
            overlap = (
                min(fx1, sx1) - max(fx0, sx0) > 1.0e-4
                and min(fz1, sz1) - max(fz0, sz0) > 1.0e-4
            )
            if overlap:
                raise SystemExit(
                    "build_outdoor_route: two slice walls overlap in plan "
                    f"({first['x']},{first['z']},{first['width']}x{first['depth']}) and "
                    f"({second['x']},{second['z']},{second['width']}x{second['depth']}); "
                    "a previous revision's wall survived the strip"
                )


def apply_slice(level: dict) -> None:
    """Replaces the slice in place, preserving later generators' ordering."""
    previous = build_slice(level)
    insertion_points = {}
    for field, generated in previous.items():
        retained = 0
        for entry in level.get(field, []):
            if _is_ours(entry, generated, field):
                insertion_points[field] = retained
                break
            retained += 1
    strip_slice(level)
    slice_data = build_slice(level)
    for field, generated in slice_data.items():
        target = level.setdefault(field, [])
        index = insertion_points.get(field, len(target))
        target[index:index] = copy.deepcopy(generated)
    level.setdefault("walls", [])
    level["walls"][SOURCE_WALL_INDEX].setdefault("openings", []).append(copy.deepcopy(SOURCE_OPENING))
    level["sky"] = copy.deepcopy(SKY)
    _assert_slice_has_no_overlapping_walls(level)


# --------------------------------------------------------------------------
# CLI
# --------------------------------------------------------------------------


def _render(level: dict) -> str:
    return json.dumps(level, indent=2) + "\n"


def _atomic_write(path: Path, payload: str) -> None:
    handle = tempfile.NamedTemporaryFile(
        "w", encoding="utf-8", delete=False, dir=str(path.parent), prefix=path.name, suffix=".tmp"
    )
    try:
        handle.write(payload)
        handle.flush()
        os.fsync(handle.fileno())
    finally:
        handle.close()
    os.replace(handle.name, path)


def _first_difference(expected: str, actual: str) -> str:
    expected_lines = expected.splitlines()
    actual_lines = actual.splitlines()
    for index in range(max(len(expected_lines), len(actual_lines))):
        want = expected_lines[index] if index < len(expected_lines) else "<end>"
        have = actual_lines[index] if index < len(actual_lines) else "<end>"
        if want != have:
            return f"line {index + 1}:\n  expected: {want}\n  actual:   {have}"
    return "files differ in trailing whitespace"


def main(argv: Optional[List[str]] = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--check", action="store_true", help="verify the level matches this tool")
    parser.add_argument("--summary", action="store_true", help="print the slice's counts only")
    args = parser.parse_args(argv)

    raw = LEVEL_PATH.read_text(encoding="utf-8")
    level = json.loads(raw)
    if args.summary:
        slice_data = build_slice(level)
        for field, entries in slice_data.items():
            print(f"{field}: {len(entries)}")
        grass = [p for p in slice_data["props"] if str(p.get("id", "")).startswith("grass_")]
        print(f"grass tufts: {len(grass)}")
        return 0

    if args.check:
        expected_level = json.loads(raw)
        apply_slice(expected_level)
        expected = _render(expected_level)
        if expected == raw:
            print(f"{LEVEL_PATH}: current with tools/levels/build_outdoor_route.py")
            return 0
        print(f"{LEVEL_PATH}: stale (run python3 tools/levels/build_outdoor_route.py)")
        print(_first_difference(expected, raw))
        return 1

    apply_slice(level)
    payload = _render(level)
    _atomic_write(LEVEL_PATH, payload)
    slice_data = build_slice(level)
    print(f"wrote {LEVEL_PATH} ({len(payload)} bytes)")
    for field, entries in slice_data.items():
        print(f"  {field}: {len(entries)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
