#!/usr/bin/env python3
"""Generates the shipped ``Model Zoo`` level from the asset catalog.

The zoo is a pool-themed showroom that displays **every** registered placeable
model at least once, plus the demonstrations that need more than a single copy
(three mannequin poses, three skeleton poses including one on a real chair,
walking and running rats, Spooner-Man's sit/wait/stand route, independently
animated copies of a shared model, and a floating duck). It is generated, never
hand-written: the layout, the room size, the lighting grid and the display ids
are all derived from ``assets/catalog.json`` and the models' real bounds, so
adding a catalog entry adds a display on the next run and removing one removes
it -- without renumbering any other instance.

Run from the Places repository root:

    python3 tools/levels/build_model_zoo.py                 # write the level
    python3 tools/levels/build_model_zoo.py --check         # verify it is current
    python3 tools/levels/build_model_zoo.py --workers 8     # bounded inspection
    python3 tools/levels/build_model_zoo.py --no-cache      # ignore the bounds cache

The expensive half is asset inspection: every model is parsed and its animation
envelope is sampled, which is independent work per asset. ``--workers N`` (or
``PLACES_TOOL_WORKERS``) runs it on ``N`` spawn-context worker processes; the
parent alone merges the results (keyed by stable asset id, never by completion
order) and alone writes the level, so serial and parallel runs produce
byte-identical output. Inspection results are cached under ``cache/`` keyed by
each model's path, size and mtime plus the tool's cache version; a changed
model, clip or catalogue entry invalidates its own entry only.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import multiprocessing
import os
import sys
import time
from pathlib import Path
from typing import Dict, Iterable, List, Optional, Sequence, Tuple

APP_ROOT = Path(__file__).resolve().parent.parent.parent
ASSETS_DIR = APP_ROOT / "assets"
CATALOG_PATH = ASSETS_DIR / "catalog.json"
OUTPUT_PATH = ASSETS_DIR / "levels" / "model_zoo.json"
CACHE_DIR = APP_ROOT / "cache"
CACHE_PATH = CACHE_DIR / "zoo_inspection.json"
ENTITIES_DIR = Path(__file__).resolve().parent.parent / "entities"

# Bump when the inspection's meaning changes (bounds method, envelope sampling,
# clip metadata), so no stale cache can hide an updated model.
CACHE_VERSION = 2

# Hard ceiling on CPU-heavy execution slots this tool may use, shared with the
# other Places tools (see tools/entities/validate_entities.py).
CPU_CEILING = 12
MEMORY_GUARD_BYTES = 96 * 1024 * 1024

# ---------------------------------------------------------------------------
# Catalogue and display plan
# ---------------------------------------------------------------------------

# Which display class each catalogue entry needs. Anything not listed stands on
# the floor. A class fixes the mount, the placement rule and the label, not the
# asset: the model, its size, its scale and its textures are untouched.
WALL_MOUNTS = {
    "core:tv": 1.30,
    "home:wall_switch": 1.20,
    "home:cabinet_wall": 1.45,
}
CEILING_MOUNTS = {
    "home:ball_light",
    "core:exit_sign",
}
TABLE_TOP = {
    "home:knife",
    "home:fork",
    "home:spoon",
    "home:plate",
    "home:bowl",
    "home:plant_table",
    "home:crt_tv",
}
WATER_DISPLAY = {"core:rubber_duck"}
ROUTED = {"rat", "spooner-man"}
POSED = {"mannequin", "skeleton"}
# Level-primitive companions: the prop is a visual, the behaviour lives in a
# `ladders[]` volume (the ladder) or in the guardrail primitive. They are still
# displayed in full.
LADDER_COMPANION = "core:pool_ladder"

# Emissive props that must actually illuminate: a light is authored into the
# placement's own `lights` array, exactly as Places Demo does.
PROP_LIGHTS = {
    "core:exit_sign": {
        "shape": "rect",
        "offset": [0.0, 0.0, 0.08],
        "color": [0.35, 1.0, 0.45],
        "intensity": 0.55,
        "range": 3.2,
        "half_width": 0.22,
        "half_depth": 0.28,
    },
    "home:ball_light": {
        "shape": "point",
        "offset": [0.0, -0.10, 0.0],
        "color": [1.0, 0.93, 0.82],
        "intensity": 0.7,
        "range": 5.0,
    },
}

# The demonstrations the zoo must contain beyond one display per model, each
# with its own stable instance id, pose/route and mount.
POSE_DEMOS: List[Tuple[str, str, str]] = [
    # (model, clip, role) -- one instance per pose, all on shared display rows.
    ("mannequin", "pose_stand", "stand"),
    ("mannequin", "pose_arms_up", "arms-up"),
    ("mannequin", "pose_arms_forward", "arms-forward"),
    ("skeleton", "pose_stand", "stand"),
    ("skeleton", "pose_sit_floor", "floor-sit"),
    ("skeleton", "pose_sit_chair", "chair-sit"),
]
SWITCH_DEMO = "home:wall_switch"


# Set by --quiet; the worker's progress lines honour it (workers inherit the
# flag through their own imported module, so it must be module-level).
_QUIET = False


def progress(message: str) -> None:
    if not _QUIET:
        print(message, flush=True)


def load_catalog(path: Optional[Path] = None) -> Dict:
    with open(path or CATALOG_PATH, encoding="utf-8") as handle:
        return json.load(handle)


def placeables(catalog: Dict) -> List[Dict]:
    found = [
        entry
        for entry in catalog["assets"]
        if entry.get("asset_type") in ("prop", "entity") and entry.get("model")
    ]
    # Stable catalogue order: the id, not the file order, decides every display.
    found.sort(key=lambda entry: entry["id"])
    return found


def sound_entries(catalog: Dict) -> List[Dict]:
    entries = catalog.get("assets", [])
    return sorted(entries, key=lambda entry: entry.get("id", ""))


# ---------------------------------------------------------------------------
# Asset inspection (bounds + animation envelope), cacheable and parallelisable
# ---------------------------------------------------------------------------


def cache_key(model_path: str) -> str:
    full = ASSETS_DIR / model_path
    try:
        stat = full.stat()
    except OSError:
        return f"{model_path}:missing"
    return f"{model_path}:{stat.st_size}:{int(stat.st_mtime)}:{CACHE_VERSION}"


def inspect_asset(entry: Dict) -> Dict:
    """Bounds, skin and clip metadata for one catalogue placeable.

    Runs in a worker process for the parallel path and in-process for the
    serial reference; it only reads the model file and returns plain data.
    """
    sys.path.insert(0, str(ENTITIES_DIR))
    sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "props"))
    import glb  # tools/props/glb.py

    model_path = entry["model"]
    full = ASSETS_DIR / model_path
    result: Dict = {
        "id": entry["id"],
        "model": model_path,
        "rest_bounds": None,
        "envelope": None,
        "envelope_clip": None,
        "clips": [],
        "skinned": False,
        "triangles": 0,
        "error": None,
    }
    try:
        data = full.read_bytes()
        mesh = glb.read_glb(data)
        low, high = mesh.bounds()
        result["rest_bounds"] = [list(low), list(high)]
        result["triangles"] = mesh.triangle_count
        gltf = mesh.json
        clips = [animation.get("name", "") for animation in gltf.get("animations", [])]
        marker = gltf.get("asset", {}).get("extras", {}).get("places_entity_clips", {})
        # The marker ships in two shapes: v1 keys clips directly by name, v2
        # carries a `clips` list of entries. Read both, so a model is inspected
        # the same way whichever exporter wrote it.
        by_name: Dict[str, Dict] = {}
        if isinstance(marker, dict):
            listed = marker.get("clips")
            if isinstance(listed, list):
                for clip_entry in listed:
                    if isinstance(clip_entry, dict) and isinstance(
                        clip_entry.get("name"), str
                    ):
                        by_name[clip_entry["name"]] = clip_entry
            else:
                by_name = {
                    key: value
                    for key, value in marker.items()
                    if isinstance(value, dict) and not key.startswith("base_")
                }
        declared = []
        for clip in clips:
            info = by_name.get(clip, {})
            declared.append(
                {
                    "name": clip,
                    "loop": bool(info.get("loop", False)),
                    "reference_speed_mps": info.get("reference_speed_mps"),
                }
            )
        result["clips"] = declared
        result["skinned"] = bool(gltf.get("skins"))
        if result["skinned"]:
            result["envelope"], result["envelope_clip"] = skinned_envelope(
                full, declared
            )
        progress(f"[zoo] inspected {entry['id']}")
    except Exception as error:  # noqa: BLE001 - reported per asset, never fatal
        result["error"] = f"{type(error).__name__}: {error}"
        progress(f"[zoo] inspection failed for {entry['id']}: {result['error']}")
    return result


def skinned_envelope(path: Path, clips: Sequence[Dict]) -> Tuple[Optional[List], Optional[str]]:
    """Union of a skinned model's posed vertex bounds across its clips.

    Uses the entity toolkit's own skinning and clip sampling, so the envelope
    the layout reserves is the same deformation the validator measures.
    """
    sys.path.insert(0, str(ENTITIES_DIR))
    from validate_entities import Model  # type: ignore[import-not-found]

    model = Model(path)
    if model.bind_skin:
        low = [min(point[axis] for point in model.bind_skin) for axis in range(3)]
        high = [max(point[axis] for point in model.bind_skin) for axis in range(3)]
    else:
        return None, None
    widest_clip = None
    widest_span = 0.0
    for clip in model.clips:
        name = clip.get("name", "")
        duration = float(clip.get("duration", 0.0))
        samples = 48 if duration > 0.0 else 1
        for sample in range(samples):
            fraction = sample / samples if samples > 1 else 0.0
            posed = model._skin(model._pose_globals(clip, duration * fraction))
            for point in posed:
                for axis in range(3):
                    low[axis] = min(low[axis], point[axis])
                    high[axis] = max(high[axis], point[axis])
        span = max(high[axis] - low[axis] for axis in range(3))
        if span > widest_span:
            widest_span = span
            widest_clip = name
    return [low, high], widest_clip


def usable_cpu_count() -> int:
    if hasattr(os, "process_cpu_count"):
        count = os.process_cpu_count()
        if count:
            return int(count)
    return os.cpu_count() or 1


def parse_worker_count(requested: Optional[str]) -> Tuple[int, str]:
    sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
    from execution import worker_count
    try:
        count = worker_count(requested)
    except ValueError as error:
        raise SystemExit(str(error)) from error
    if requested is None:
        return count, "automatic"
    if count < int(requested):
        return count, f"requested {requested}, reduced to {count} (CPU/allocation budget)"
    return count, "requested"



_worker_paths: Sequence[str] = ()


def _worker_init(paths: Sequence[str], quiet: bool) -> None:
    global _worker_paths, _QUIET
    _worker_paths = paths
    _QUIET = quiet


def _worker_inspect(index: int) -> Dict:
    return inspect_asset(json.loads(_worker_paths[index]))


def inspect_all(
    entries: Sequence[Dict],
    workers: int,
    use_cache: bool,
) -> Dict[str, Dict]:
    """Inspects every entry, reusing the cache and merging deterministically."""
    cached: Dict[str, Dict] = {}
    if use_cache and CACHE_PATH.exists():
        try:
            with open(CACHE_PATH, encoding="utf-8") as handle:
                stored = json.load(handle)
            if stored.get("version") == CACHE_VERSION:
                cached = stored.get("assets", {})
        except (OSError, ValueError):
            cached = {}

    todo: List[Dict] = []
    results: Dict[str, Dict] = {}
    keys: Dict[str, str] = {}
    for entry in entries:
        key = cache_key(entry["model"])
        keys[entry["id"]] = key
        hit = cached.get(entry["id"])
        if hit is not None and hit.get("key") == key:
            results[entry["id"]] = hit["data"]
        else:
            todo.append(entry)

    if todo:
        started = time.perf_counter()
        serial_payload = [json.dumps(entry) for entry in todo]
        effective = workers
        if workers > 1 and len(todo) < workers:
            effective = max(1, len(todo))
        if effective > 1:
            context = multiprocessing.get_context("spawn")
            # One worker holds one decoded model plus scratch; the guard caps the
            # pool so a very large asset set cannot exhaust memory.
            memory_cap = max(1, MEMORY_GUARD_BYTES // (32 * 1024 * 1024))
            effective = max(1, min(effective, memory_cap))
            with context.Pool(
                processes=effective,
                initializer=_worker_init,
                initargs=(serial_payload, _QUIET),
            ) as pool:
                # Mapped by task index, never by completion order.
                for index, item in enumerate(
                    pool.imap_unordered(_worker_inspect, range(len(todo)), chunksize=1)
                ):
                    del index
                    results[item["id"]] = item
        else:
            _worker_init(serial_payload, _QUIET)
            for index in range(len(todo)):
                item = _worker_inspect(index)
                results[item["id"]] = item
        elapsed = time.perf_counter() - started
        progress(
            f"[zoo] inspected {len(todo)} asset(s) with {effective} worker(s) in "
            f"{elapsed:.2f}s ({len(results) - len(todo)} cache hit(s))"
        )

    if use_cache:
        CACHE_DIR.mkdir(parents=True, exist_ok=True)
        payload = {
            "version": CACHE_VERSION,
            "assets": {
                asset_id: {"key": keys[asset_id], "data": data}
                for asset_id, data in sorted(results.items())
            },
        }
        with open(CACHE_PATH, "w", encoding="utf-8") as handle:
            json.dump(payload, handle, sort_keys=True)
            handle.write("\n")
    return results


# ---------------------------------------------------------------------------
# Derived dimensions
# ---------------------------------------------------------------------------

# Aisle rule: the clear gap between any two display footprints, and between a
# display and a wall, is at least this. The bay pitch below is the largest
# single-prop footprint the catalogue ships plus this clearance.
CLEAR_AISLE_M = 1.4
BAY_PITCH_M = 4.8
COLUMNS = 7
WALL_ZONE_M = 5.5
ROUTE_ZONE_M = 10.0
MARGIN_M = 3.6
HALL_HEIGHT_M = 5.0


def footprint(entry: Dict, inspection: Dict) -> Tuple[float, float]:
    """Real transformed X/Z footprint of one placeable, in metres."""
    size = entry.get("size")
    if size:
        return float(size[0]), float(size[2])
    envelope = inspection.get("envelope") or inspection.get("rest_bounds")
    if envelope:
        low, high = envelope
        return abs(high[0] - low[0]), abs(high[2] - low[2])
    return 0.6, 0.6


def height_range(entry: Dict, inspection: Dict) -> Tuple[float, float]:
    envelope = inspection.get("envelope") or inspection.get("rest_bounds")
    if envelope:
        low, high = envelope
        return float(low[1]), float(high[1])
    size = entry.get("size") or [0.6, 0.9, 0.6]
    return 0.0, float(size[1])


def display_class(entry: Dict) -> str:
    asset_id = entry["id"]
    if asset_id in TABLE_TOP:
        return "table"
    if asset_id in WALL_MOUNTS:
        return "wall"
    if asset_id in CEILING_MOUNTS:
        return "ceiling"
    if asset_id in WATER_DISPLAY:
        return "water"
    if asset_id in ROUTED:
        return "route"
    if asset_id in POSED:
        return "pose"
    return "floor"


def instance_id(entry: Dict, role: str) -> str:
    """Stable per-display identity: catalogue id plus its display role."""
    return f"zoo:{entry['id'].replace(':', '-')}:{role}"


def ordered_displays(catalog: Dict) -> List[Dict]:
    """Every display the zoo must contain, in a deterministic order.

    One entry per catalogue placeable, plus the extra pose/route/copy
    demonstrations. Each carries the stable instance id and its display class.
    """
    displays: List[Dict] = []
    for entry in placeables(catalog):
        klass = display_class(entry)
        role = klass
        displays.append({"entry": entry, "class": klass, "role": role, "clip": None})
    for model, clip, role in POSE_DEMOS:
        entry = next((item for item in placeables(catalog) if item["id"] == model), None)
        if entry is None:
            continue
        displays.append(
            {"entry": entry, "class": "pose", "role": role, "clip": clip}
        )
    # Independently animated copies of a shared model: a second switch and a
    # second rat, each with its own identity and its own interaction state.
    switch = next(
        (item for item in placeables(catalog) if item["id"] == SWITCH_DEMO), None
    )
    if switch is not None:
        displays.append(
            {"entry": switch, "class": "wall", "role": "switch-b", "clip": "toggle"}
        )
    rat = next((item for item in placeables(catalog) if item["id"] == "rat"), None)
    if rat is not None:
        displays.append({"entry": rat, "class": "route", "role": "run", "clip": "run"})
    spooner = next(
        (item for item in placeables(catalog) if item["id"] == "spooner-man"), None
    )
    if spooner is not None:
        displays.append(
            {"entry": spooner, "class": "route", "role": "companion", "clip": "idle"}
        )
    return displays


def plan_hall(displays: Sequence[Dict], inspections: Dict[str, Dict]) -> Dict:
    """Room extents and the placement grid derived from the content."""
    floor_items = [
        item for item in displays if item["class"] in ("floor", "pose")
    ]
    widest = 0.6
    deepest = 0.6
    for item in floor_items:
        width, depth = footprint(item["entry"], inspections[item["entry"]["id"]])
        widest = max(widest, width)
        deepest = max(deepest, depth)
    # The bay must hold the widest footprint plus the clear aisle.
    pitch = max(BAY_PITCH_M, max(widest, deepest) + CLEAR_AISLE_M)
    rows = max(1, math.ceil(len(floor_items) / COLUMNS))
    width = MARGIN_M * 2.0 + COLUMNS * pitch
    depth = (
        MARGIN_M * 2.0
        + WALL_ZONE_M
        + rows * pitch
        + ROUTE_ZONE_M
    )
    return {
        "pitch": pitch,
        "columns": COLUMNS,
        "rows": rows,
        "width": round(width, 3),
        "depth": round(depth, 3),
        "height": HALL_HEIGHT_M,
    }


def light_grid(plan: Dict) -> List[Dict]:
    """Ceiling fixtures on a grid covering the whole hall."""
    spacing = 4.5
    columns = max(1, int(plan["width"] // spacing))
    rows = max(1, int(plan["depth"] // spacing))
    # Centre the grid inside the hall.
    x0 = (plan["width"] - (columns - 1) * spacing) / 2.0
    z0 = (plan["depth"] - (rows - 1) * spacing) / 2.0
    lights: List[Dict] = []
    for row in range(rows):
        for column in range(columns):
            lights.append(
                {
                    "id": f"zoo:light:{row}:{column}",
                    "fixture": "core:pool_light_round",
                    "x": round(x0 + column * spacing, 3),
                    "z": round(z0 + row * spacing, 3),
                    "brightness": 0.52,
                }
            )
    return lights


# ---------------------------------------------------------------------------
# Layout
# ---------------------------------------------------------------------------


class Layout:
    """Accumulates every authored element, tracking the zones as it goes."""

    def __init__(self, plan: Dict) -> None:
        self.plan = plan
        self.origin_x = 0.0
        self.origin_z = 0.0
        self.walls: List[Dict] = []
        self.props: List[Dict] = []
        self.decals: List[Dict] = []
        self.routes: List[Dict] = []
        self.triggers: List[Dict] = []
        self.ladders: List[Dict] = []
        self.water: List[Dict] = []
        self.floor_regions: List[Dict] = []
        self.arc_walls: List[Dict] = []
        self.pillars: List[Dict] = []
        self.guardrails: List[Dict] = []
        self.half_walls: List[Dict] = []
        self.geometry_intent: List[Dict] = []
        # Zones no floor display may occupy (the table run and the basin), so a
        # showcase never lands inside a neighbouring exhibit.
        self.reserved: List[Tuple[float, float, float, float]] = []

    def reserve(self, x0: float, z0: float, x1: float, z1: float) -> None:
        self.reserved.append((min(x0, x1), min(z0, z1), max(x0, x1), max(z0, z1)))

    def is_reserved(self, x: float, z: float) -> bool:
        return any(
            x0 <= x <= x1 and z0 <= z <= z1 for (x0, z0, x1, z1) in self.reserved
        )

    def add_prop(self, entry: Dict, role: str, x: float, z: float, **fields) -> Dict:
        size = entry.get("size") or [0.6, 0.9, 0.6]
        placement: Dict = {
            "id": instance_id(entry, role),
            "display_name": entry.get("display_name", entry["id"]),
            "model": entry["id"],
            "x": round(x, 3),
            "z": round(z, 3),
            "size": [round(float(value), 3) for value in size],
            "solid": bool(entry.get("solid")),
        }
        interaction = fields.pop("interaction", None)
        if interaction is None:
            interaction = {
                "prompt": "Show name",
                "actions": [{"action": "toggle_label"}],
            }
        if interaction is not False:
            placement["interaction"] = interaction
        placement.update(fields)
        self.props.append(placement)
        return placement


def place_floor_items(layout: Layout, displays: Sequence[Dict], inspections: Dict) -> Dict[str, Dict]:
    """Places every floor/pose/route display on a bay grid.

    Returns the display-key -> placement map so the route and pose passes can
    address the instance it already laid out.
    """
    plan = layout.plan
    pitch = plan["pitch"]
    columns = plan["columns"]
    # Floor rows start after the wall zone at the north side.
    row0_z = MARGIN_M + WALL_ZONE_M + pitch * 0.5
    placed: Dict[str, Dict] = {}
    index = 0
    for item in displays:
        if item["class"] not in ("floor", "pose"):
            continue
        row = index // columns
        column = index % columns
        x = MARGIN_M + pitch * 0.5 + column * pitch
        z = row0_z + row * pitch
        # A reserved bay advances to the next one instead of shrinking the
        # aisle; the reserved zones are always large enough to hold their own
        # content, so nothing is dropped.
        while layout.is_reserved(x, z):
            index += 1
            row = index // columns
            column = index % columns
            x = MARGIN_M + pitch * 0.5 + column * pitch
            z = row0_z + row * pitch
        entry = item["entry"]
        fields: Dict = {}
        if item["class"] == "pose" and item["clip"]:
            fields["interaction"] = {
                "prompt": "Pose",
                "actions": [
                    {"action": "play_animation", "clip": item["clip"], "loop": False},
                    {"action": "toggle_label"},
                ],
            }
        placement = layout.add_prop(entry, item["role"], x, z, **fields)
        placed[f"{entry['id']}:{item['role']}"] = placement
        index += 1
    return placed


def place_route_items(
    layout: Layout, displays: Sequence[Dict], inspections: Dict[str, Dict]
) -> Dict[str, Dict]:
    """Animated displays in their own clear lane at the south of the hall.

    Each route walks along the lane's X axis with a reserved span: no floor
    display is placed in this band, so a route can never be blocked by (or
    walk into) an exhibit, and visitors can watch from the aisle.
    """
    plan = layout.plan
    z = plan["depth"] - MARGIN_M - ROUTE_ZONE_M * 0.5
    lane_span = 7.0
    placed: Dict[str, Dict] = {}
    route_items = [item for item in displays if item["class"] == "route"]
    for index, item in enumerate(route_items):
        entry = item["entry"]
        x = MARGIN_M + 10.5 + index * lane_span
        fields: Dict = {"interaction": {"prompt": "Show name", "actions": [{"action": "toggle_label"}]}}
        placement = layout.add_prop(entry, item["role"], x, z, **fields)
        placed[f"{entry['id']}:{item['role']}"] = placement
    return placed


def place_wall_items(layout: Layout, displays: Sequence[Dict]) -> Dict[str, Dict]:
    """Wall-mounted displays grouped into one readable run on the north wall.

    The run is centred on the wall and laid out from each asset's real X
    footprint plus a fixed gap, so the switch / television / switch / wall
    cabinet read as one exhibit instead of four lonely objects on a 40 m wall.
    Every one faces +Z, into the room.
    """
    plan = layout.plan
    wall_items = [item for item in displays if item["class"] == "wall"]
    placed: Dict[str, Dict] = {}
    gap = 1.1
    widths = [max(0.2, float((item["entry"].get("size") or [0.6, 0.9, 0.6])[0])) for item in wall_items]
    total = sum(widths) + gap * max(0, len(widths) - 1)
    cursor = (plan["width"] - total) * 0.5
    for index, item in enumerate(wall_items):
        entry = item["entry"]
        x = cursor + widths[index] * 0.5
        cursor += widths[index] + gap
        z = 0.22
        y = WALL_MOUNTS.get(entry["id"], 1.2)
        fields: Dict = {"rotation_degrees": 0.0, "y": y}
        if entry["id"] == SWITCH_DEMO:
            fields["interaction"] = {
                "prompt": "Switch",
                "reach": 1.6,
                "actions": [
                    {"action": "toggle_animation", "clip": "toggle"},
                    {"action": "toggle_label"},
                ],
            }
        placement = layout.add_prop(entry, item["role"], x, z, **fields)
        placed[f"{entry['id']}:{item['role']}"] = placement
    return placed


def place_ceiling_items(layout: Layout, displays: Sequence[Dict]) -> Dict[str, Dict]:
    """Suspended displays along the hall's centre line."""
    plan = layout.plan
    ceiling_items = [item for item in displays if item["class"] == "ceiling"]
    placed: Dict[str, Dict] = {}
    spacing = plan["width"] / (len(ceiling_items) + 1)
    for index, item in enumerate(ceiling_items):
        entry = item["entry"]
        low, high = height_range(entry, {})
        height = max(0.05, high - low)
        x = spacing * (index + 1)
        z = plan["depth"] - MARGIN_M - 2.0
        fields: Dict = {"y": round(plan["height"] - height, 3)}
        if entry["id"] in PROP_LIGHTS:
            fields["lights"] = [PROP_LIGHTS[entry["id"]]]
        placement = layout.add_prop(entry, "ceiling", x, z, **fields)
        placed[f"{entry['id']}:ceiling"] = placement
    return placed


def place_table(layout: Layout, table_entry: Dict, table_items: Sequence[Dict]) -> Dict[str, Dict]:
    """Two real `core:table`s: a place setting and a media table.

    A 1.4 m x 0.8 m table cannot carry a 0.55 m CRT and six place settings
    without the models intersecting; splitting the run keeps every display at
    its real scale on a real tabletop instead of shrinking or overlapping them.
    """
    plan = layout.plan
    x = plan["width"] - MARGIN_M - 3.0
    settings = {item["entry"]["id"] for item in table_items} - {"home:crt_tv"}
    settings_x = x
    settings_z = MARGIN_M + WALL_ZONE_M + 1.2
    media_x = x
    media_z = MARGIN_M + WALL_ZONE_M + 4.6
    layout.add_prop(table_entry, "table-host", settings_x, settings_z, rotation_degrees=0.0)
    layout.add_prop(table_entry, "media-host", media_x, media_z, rotation_degrees=0.0)
    table_height = float((table_entry.get("size") or [1.4, 0.75, 0.8])[1])
    placed: Dict[str, Dict] = {}
    # One seat setting around the table's south edge: knife and fork flank the
    # plate, the spoon sits beside it, the bowl opposite and the plant in the
    # middle across the table.
    offsets = {
        "home:plate": (-0.32, -0.20),
        "home:fork": (-0.46, -0.20),
        "home:knife": (-0.18, -0.20),
        "home:spoon": (0.30, -0.24),
        "home:bowl": (-0.02, 0.16),
        "home:plant_table": (0.52, 0.18),
        "home:crt_tv": (0.0, 0.0),
    }
    used: Dict[Tuple[float, float], bool] = {}

    def offset_for(asset_id: str, index: int) -> Tuple[float, float]:
        if asset_id in offsets and not used.get(offsets[asset_id], False):
            position = offsets[asset_id]
            used[position] = True
            return position
        angle = (index * 0.7) % (2.0 * math.pi)
        radius = 0.30 + 0.04 * index
        return (radius * math.cos(angle), radius * math.sin(angle))

    for index, item in enumerate(table_items):
        entry = item["entry"]
        dx, dz = offset_for(entry["id"], index)
        host_x = media_x if entry["id"] == "home:crt_tv" else settings_x
        host_z = media_z if entry["id"] == "home:crt_tv" else settings_z
        placement = layout.add_prop(
            entry,
            item["role"],
            host_x + dx,
            host_z + dz,
            y=round(table_height, 3),
            rotation_degrees=round(0.0, 1),
        )
        placed[f"{entry['id']}:{item['role']}"] = placement
    # Only the place-setting table participates in the setting set; the media
    # table's CRT is already placed.
    del settings
    return placed


def place_water(layout: Layout, displays: Sequence[Dict]) -> Dict[str, Dict]:
    """A contained basin with a floating duck, a ladder and guardrails."""
    plan = layout.plan
    basin_width = 8.0
    basin_depth = 6.0
    basin_x = MARGIN_M
    basin_z = plan["depth"] - MARGIN_M - basin_depth
    layout.floor_regions.append(
        {
            "x": round(basin_x, 3),
            "z": round(basin_z, 3),
            "width": basin_width,
            "depth": basin_depth,
            "offset_y": -1.5,
            "material": "core:pool_tile_basin_01",
            "edge_material": "core:pool_tile_wall_01",
        }
    )
    layout.water.append(
        {
            "x": round(basin_x, 3),
            "z": round(basin_z, 3),
            "width": basin_width,
            "depth": basin_depth,
            "surface_y": -0.2,
            "bottom_y": -1.5,
            "swimming": True,
        }
    )
    placed: Dict[str, Dict] = {}
    for item in displays:
        if item["class"] != "water":
            continue
        entry = item["entry"]
        placement = layout.add_prop(
            entry,
            "float",
            basin_x + basin_width * 0.5,
            basin_z + basin_depth * 0.5,
            interaction=False,
            float={
                "draft": 0.03,
                "bob": 0.012,
                "bob_seconds": 2.8,
                "heel_degrees": 6.0,
                "heel_seconds": 3.6,
                "phase": 0.25,
            },
        )
        placed[f"{entry['id']}:float"] = placement
    # A real ladder volume facing the deck, with the ladder prop non-solid so
    # the climb approach is never blocked.
    layout.ladders.append(
        {
            "x": round(basin_x + basin_width * 0.5 - 0.3, 3),
            "z": round(basin_z - 0.3, 3),
            "width": 0.6,
            "depth": 0.6,
            "bottom_y": -1.5,
            "top_y": 0.0,
            "facing_degrees": 180.0,
        }
    )
    ladder_entry = next(
        (item for item in placeables(load_catalog()) if item["id"] == LADDER_COMPANION),
        None,
    )
    if ladder_entry is not None:
        layout.add_prop(
            ladder_entry,
            "basin",
            basin_x + basin_width * 0.5 - 0.35,
            basin_z - 0.55,
            rotation_degrees=0.0,
            solid=False,
        )
        placed[f"{LADDER_COMPANION}:basin"] = layout.props[-1]
    return placed


def add_architecture(layout: Layout) -> None:
    """Curved walls, pillars and the ceiling vent, in more than one material."""
    plan = layout.plan
    z = MARGIN_M + WALL_ZONE_M + 1.0
    for index in range(2):
        layout.arc_walls.append(
            {
                "x": round(MARGIN_M + 5.0 + index * 9.0, 3),
                "z": round(z, 3),
                "radius": 2.4,
                "thickness": 0.24,
                "height": plan["height"],
                "start_degrees": 0.0,
                "sweep_degrees": 180.0,
                "segments": 24,
                # More than one material across the curved examples.
                "material": "core:pool_tile_wall_01",
                "inner_material": "core:pool_tile_deck_01",
                "cap_material": "core:pool_tile_wall_01" if index == 0 else "core:pool_tile_deck_01",
            }
        )
    for index in range(2):
        layout.pillars.append(
            {
                "x": round(plan["width"] - MARGIN_M - 4.0 - index * 3.0, 3),
                "z": round(plan["depth"] - MARGIN_M - ROUTE_ZONE_M - 1.0, 3),
                "radius": 0.3,
                "height": plan["height"],
                "segments": 16,
                "material": "core:pool_tile_wall_01" if index == 0 else "core:pool_tile_deck_01",
                "cap_material": "core:pool_tile_deck_01",
            }
        )
    # Ceiling-grid-aligned vent decals over the hall: the alignment is derived
    # from the ceiling material's own 2 m panel module, never hand-tuned.
    for index in range(4):
        layout.decals.append(
            {
                "x": round(MARGIN_M + 3.0 + index * 6.0, 3),
                "z": round(plan["depth"] * 0.5, 3),
                "width": 0.6,
                "height": 0.6,
                "material": "core:decal_ceiling_vent_01",
                "surface": "ceiling",
                "align": "ceiling_grid",
            }
        )
    for index in range(2):
        layout.decals.append(
            {
                "x": round(MARGIN_M + 2.0 + index * 3.0, 3),
                "z": round(plan["depth"] - MARGIN_M - 1.4, 3),
                "width": 1.2,
                "height": 1.2,
                "material": "core:decal_arrow_01",
                "surface": "floor",
            }
        )


def add_routes(layout: Layout, placed: Dict[str, Dict]) -> None:
    """Deterministic routes for the pose and locomotion demonstrations.

    A pose display holds its clip as its rest state through a long `play` step,
    so a visitor sees standing / arms-up / arms-forward and the two skeleton
    sits without pressing anything; the instance's own interaction can still
    replay the clip on demand.
    """
    for model, clip, role in POSE_DEMOS:
        placement = placed.get(f"{model}:{role}")
        if placement is None:
            continue
        layout.routes.append(
            {
                "id": placement["id"],
                "loop": True,
                "steps": [
                    {"step": "play", "clip": clip, "seconds": 1_800.0, "loop": True}
                ],
            }
        )

    rat = placed.get("rat:route")
    if rat:
        layout.routes.append(
            {
                "id": rat["id"],
                "loop": True,
                "steps": [
                    {"step": "play", "clip": "walk", "seconds": 1.5, "loop": True},
                    {"step": "move_to", "x": round(rat["x"] + 3.2, 3), "z": rat["z"], "speed": 0.1985},
                    {"step": "play", "clip": "idle", "seconds": 1.0, "loop": True},
                    {"step": "move_to", "x": rat["x"], "z": rat["z"], "speed": 0.1985},
                ],
            }
        )
    rat_run = placed.get("rat:run")
    if rat_run:
        layout.routes.append(
            {
                "id": rat_run["id"],
                "loop": True,
                "steps": [
                    {"step": "play", "clip": "run", "seconds": 1.0, "loop": True},
                    {"step": "move_to", "x": round(rat_run["x"] + 4.0, 3), "z": rat_run["z"], "speed": 0.5731},
                    {"step": "play", "clip": "idle", "seconds": 0.8, "loop": True},
                    {"step": "move_to", "x": rat_run["x"], "z": rat_run["z"], "speed": 0.5731},
                ],
            }
        )
    spooner = placed.get("spooner-man:route")
    if spooner:
        layout.routes.append(
            {
                "id": spooner["id"],
                "loop": True,
                "steps": [
                    {"step": "play", "clip": "walk", "seconds": 2.0, "loop": True},
                    {"step": "move_to", "x": round(spooner["x"] + 2.4, 3), "z": spooner["z"], "speed": 0.26},
                    {"step": "play", "clip": "sit_down", "seconds": 1.7, "loop": False},
                    {"step": "wait", "seconds": 1.0},
                    {"step": "play", "clip": "sit_idle", "seconds": 3.0, "loop": True},
                    {"step": "play", "clip": "stand_up", "seconds": 1.5, "loop": False},
                    {"step": "move_to", "x": spooner["x"], "z": spooner["z"], "speed": 0.26},
                ],
            }
        )
    companion = placed.get("spooner-man:companion")
    if companion:
        layout.routes.append(
            {
                "id": companion["id"],
                "loop": True,
                "steps": [
                    {"step": "play", "clip": "idle", "seconds": 2.5, "loop": True},
                    {"step": "face", "yaw_degrees": 270.0},
                    {"step": "play", "clip": "walk", "seconds": 1.5, "loop": True},
                    {"step": "move_to", "x": companion["x"], "z": round(companion["z"] + 2.0, 3), "speed": 0.26},
                    {"step": "face", "yaw_degrees": 90.0},
                ],
            }
        )


def build_level(catalog: Dict, inspections: Dict[str, Dict]) -> Dict:
    displays = ordered_displays(catalog)
    plan = plan_hall(displays, inspections)
    layout = Layout(plan)

    floor_items = [item for item in displays if item["class"] == "floor"]
    posed = [item for item in displays if item["class"] == "pose"]
    routed = [item for item in displays if item["class"] == "route"]
    # Reserve the table run before the floor pass: reservations are what stop a
    # showcase bay landing on top of the tables. (The tables themselves are
    # placed after the floor rows have taken every unreserved bay.)
    table_x = plan["width"] - MARGIN_M - 3.0
    table_z = MARGIN_M + WALL_ZONE_M + 1.2
    layout.reserve(table_x - 2.8, table_z - 2.4, table_x + 2.8, table_z + 5.8)
    placed = place_floor_items(layout, floor_items + posed, inspections)
    placed.update(place_route_items(layout, routed, inspections))
    placed.update(place_wall_items(layout, displays))
    placed.update(place_ceiling_items(layout, displays))

    by_id = {entry["id"]: entry for entry in placeables(catalog)}
    table_entry = by_id.get("core:table")
    table_items = [item for item in displays if item["class"] == "table"]
    if table_entry is not None and table_items:
        placed.update(place_table(layout, table_entry, table_items))
    placed.update(place_water(layout, displays))
    add_architecture(layout)
    add_routes(layout, placed)

    # A chair for the skeleton's chair pose, placed with the documented fit:
    # the chair is the anchor and the skeleton sits `dz = +0.0025 m` along the
    # chair's own facing, both at the same yaw (see
    # assets/entities/skeleton/README.md).
    chair_entry = by_id.get("core:chair")
    chair_sit = placed.get("skeleton:chair-sit")
    if chair_entry is not None and chair_sit is not None:
        yaw = 180.0
        chair_x = chair_sit["x"]
        chair_z = round(chair_sit["z"] + 0.0025, 4)
        chair_sit["z"] = round(chair_z - 0.0025 * math.cos(math.radians(yaw)), 4)
        chair_sit["x"] = round(chair_x - 0.0025 * math.sin(math.radians(yaw)), 4)
        chair_sit["rotation_degrees"] = yaw
        layout.add_prop(
            chair_entry,
            "skeleton-seat",
            chair_x,
            chair_z,
            rotation_degrees=yaw,
        )

    width = plan["width"]
    depth = plan["depth"]
    height = plan["height"]
    thickness = 0.4
    walls = [
        {
            "x": -thickness,
            "z": -thickness,
            "width": width + 2 * thickness,
            "depth": thickness,
            "material": "core:pool_tile_wall_01",
        },
        {
            "x": -thickness,
            "z": depth,
            "width": width + 2 * thickness,
            "depth": thickness,
            "material": "core:pool_tile_wall_01",
        },
        {
            "x": -thickness,
            "z": 0.0,
            "width": thickness,
            "depth": depth,
            "material": "core:pool_tile_wall_01",
        },
        {
            "x": width,
            "z": 0.0,
            "width": thickness,
            "depth": depth,
            "material": "core:pool_tile_wall_01",
            "openings": [
                {
                    "kind": "passage",
                    "offset": depth * 0.5 - 2.0,
                    "width": 4.0,
                    "height": 3.0,
                    "sill": 0.0,
                }
            ],
        },
    ]
    # The exit passage opens onto the void; annotate it for the checker.
    intent = [
        {
            "check": "room-leak",
            "x": width - 1.0,
            "z": depth * 0.5 - 3.0,
            "width": 4.0,
            "depth": 6.0,
            "note": "The showroom's exit passage opens onto the void by design.",
        }
    ]

    return {
        "format_version": 2,
        "id": "model_zoo",
        "name": "Model Zoo",
        "author": "Places",
        # The spawn sits on the aisle lattice (never in a display bay), facing the
        # wall displays across the floor rows.
        "spawn": {
            "x": round(MARGIN_M + plan["pitch"] * 2.0, 3),
            "z": round(MARGIN_M + WALL_ZONE_M + plan["pitch"] * 2.0, 3),
            "yaw_degrees": 0.0,
        },
        "defaults": {
            "wall": "core:pool_tile_wall_01",
            "floor": "core:pool_tile_deck_01",
            "ceiling": "core:pool_ceiling_01",
        },
        "rooms": [
            {
                "x": 0.0,
                "z": 0.0,
                "width": round(width, 3),
                "depth": round(depth, 3),
                "height": height,
                "material": "core:pool_tile_deck_01",
                "ceiling_material": "core:pool_ceiling_01",
            }
        ],
        "walls": walls,
        "floor_regions": layout.floor_regions,
        "water": layout.water,
        "ladders": layout.ladders,
        "arc_walls": layout.arc_walls,
        "pillars": layout.pillars,
        "guardrails": layout.guardrails,
        "half_walls": layout.half_walls,
        "ceiling_lights": light_grid(plan),
        "props": layout.props,
        "decals": layout.decals,
        "routes": layout.routes,
        "area_triggers": layout.triggers,
        "geometry_intent": intent,
    }


def serialise(level: Dict) -> str:
    return json.dumps(level, indent=2) + "\n"


def main(argv: Optional[List[str]] = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="verify the shipped zoo is current")
    parser.add_argument("--workers", default=None, help="inspection worker count")
    parser.add_argument("--no-cache", action="store_true", help="ignore the bounds cache")
    parser.add_argument("--stats", action="store_true", help="print display coverage")
    parser.add_argument("--quiet", action="store_true", help="suppress the progress lines")
    parser.add_argument(
        "--catalog",
        default=None,
        help="alternate catalog path (isolated growth/removal fixtures)",
    )
    parser.add_argument(
        "--out",
        default=None,
        help="alternate output path (isolated growth/removal fixtures)",
    )
    args = parser.parse_args(argv)
    if args.quiet:
        global _QUIET
        _QUIET = True

    workers, note = parse_worker_count(args.workers)
    catalog = load_catalog(Path(args.catalog) if args.catalog else None)
    entries = placeables(catalog)
    output = Path(args.out) if args.out else OUTPUT_PATH
    inspections = inspect_all(entries, workers, not args.no_cache)
    level = build_level(catalog, inspections)
    rendered = serialise(level)

    if args.stats:
        classes: Dict[str, int] = {}
        for entry in entries:
            classes[display_class(entry)] = classes.get(display_class(entry), 0) + 1
        print(f"[zoo] workers: {workers} ({note})")
        print(f"[zoo] catalogue placeables: {len(entries)} {classes}")
        print(
            f"[zoo] hall {level['rooms'][0]['width']} x {level['rooms'][0]['depth']} x "
            f"{level['rooms'][0]['height']} m, {len(level['props'])} placements, "
            f"{len(level['ceiling_lights'])} fixtures, {len(level['routes'])} route(s)"
        )

    if args.check:
        try:
            current = output.read_text(encoding="utf-8")
        except FileNotFoundError:
            print(f"[zoo] MISSING {output}; run tools/levels/build_model_zoo.py")
            return 1
        if current != rendered:
            # A targeted diff summary, so the failure is actionable.
            try:
                stored = json.loads(current)
            except ValueError:
                print("[zoo] STALE: the shipped zoo is not valid JSON")
                return 1
            stored_ids = {prop.get("id") for prop in stored.get("props", [])}
            wanted_ids = {prop.get("id") for prop in level.get("props", [])}
            missing = sorted(wanted_ids - stored_ids)
            extra = sorted(stored_ids - wanted_ids)
            print("[zoo] STALE: the shipped zoo differs from the catalogue")
            if missing:
                print(f"[zoo]   missing displays: {missing}")
            if extra:
                print(f"[zoo]   displays no longer in the catalogue: {extra}")
            if not missing and not extra:
                print("[zoo]   display ids match; another field changed")
            return 1
        print(f"[zoo] ok {output}")
        return 0

    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(rendered, encoding="utf-8")
    digest = hashlib.sha256(rendered.encode("utf-8")).hexdigest()[:12]
    print(
        f"[zoo] wrote {output} ({len(rendered.encode('utf-8')) / 1024.0:.0f} KiB, "
        f"sha256 {digest}, {len(level['props'])} placements)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
