#!/usr/bin/env python3
"""Reimport the shipped entity GLBs in Blender and render contact sheets.

The engine's own renderer is the authority, but it needs a GPU/window session;
this tool is the offline visual check: it loads each entity GLB in a clean
Blender scene, plays every clip (and holds every pose) inside the imported
armature, and renders a front / side / three-quarter cell per sampled time.
The parent process assembles the cells into one contact sheet per entity under
``target/entity-verify/`` with the repository's own PNG helpers.

Each entity is one independent Blender process writing only into its own
scratch directory; the parent alone assembles the final sheets. ``--workers``
bounds how many Blender processes run at once (each pinned to
``--blender-threads`` native threads), ``PLACES_TOOL_WORKERS`` sets the
default, and ``--workers 1`` is the serial reference.

    python3 tools/entities/render_contact_sheets.py --workers 3
    python3 tools/entities/render_contact_sheets.py --workers 1

Blender is an optional development dependency for this check only; the asset
builds and the geometric validator are pure Python.
"""

from __future__ import annotations

import argparse
import json
import multiprocessing
import os
import shutil
import struct
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from typing import Dict, List, Optional, Sequence, Tuple

REPO_ROOT = Path(__file__).resolve().parents[2]
if str(REPO_ROOT / "tools" / "props") not in sys.path:
    sys.path.insert(0, str(REPO_ROOT / "tools" / "props"))

from tex import decode_png, write_png  # noqa: E402  (tools/props on sys.path)

DEFAULT_ASSETS = (
    "assets/entities/rat/model/rat.glb",
    "assets/entities/mannequin/model/mannequin.glb",
    "assets/entities/skeleton/model/skeleton.glb",
)
DEFAULT_OUT = "target/entity-verify"
CELL = 256
MAX_WORKERS_CEILING = 12

BLENDER_SCRIPT = r'''
import bpy, json, math, sys
from mathutils import Vector

args = json.loads(sys.argv[sys.argv.index("--") + 1])
bpy.ops.wm.read_factory_settings(use_empty=True)
scene = bpy.context.scene
bpy.ops.import_scene.gltf(filepath=args["glb"])

meshes = [
    obj
    for obj in scene.objects
    if obj.type == "MESH" and not (obj.name.startswith("Icosphere") and len(obj.data.materials) == 0)
]
armatures = [obj for obj in scene.objects if obj.type == "ARMATURE"]
if not meshes:
    raise SystemExit("no mesh imported from " + args["glb"])
# The glTF importer adds a bone-display icosphere for skinned rigs; it is not
# part of the asset and must not be framed or rendered.
for obj in list(scene.objects):
    if obj.type == "MESH" and obj not in meshes:
        bpy.data.objects.remove(obj, do_unlink=True)

# World bounds from the imported meshes at rest. Blender is Z-up: the glTF
# +Y up axis arrives as Blender +Z, and the model's +Z front arrives as -Y.
points = []
for obj in meshes:
    for corner in obj.bound_box:
        points.append(obj.matrix_world @ Vector(corner))
low = Vector((min(p.x for p in points), min(p.y for p in points), min(p.z for p in points)))
high = Vector((max(p.x for p in points), max(p.y for p in points), max(p.z for p in points)))
centre = (low + high) * 0.5
extent = max(high.x - low.x, high.y - low.y, high.z - low.z, 0.2)

# Camera: orthographic, framed on the bounding sphere.
camera_data = bpy.data.cameras.new("camera")
camera_data.type = "ORTHO"
camera_data.ortho_scale = extent * 1.65
camera = bpy.data.objects.new("camera", camera_data)
scene.collection.objects.link(camera)
scene.camera = camera

# Lighting: a key sun plus a soft fill, and a light grey world.
world = bpy.data.worlds.new("world")
world.use_nodes = True
world.node_tree.nodes["Background"].inputs[0].default_value = (0.55, 0.57, 0.60, 1.0)
world.node_tree.nodes["Background"].inputs[1].default_value = 0.7
scene.world = world
key = bpy.data.lights.new("key", type="SUN")
key.energy = 3.5
key.angle = math.radians(15)
key_obj = bpy.data.objects.new("key", key)
scene.collection.objects.link(key_obj)
key_obj.rotation_euler = (math.radians(52), math.radians(8), math.radians(35))
fill = bpy.data.lights.new("fill", type="SUN")
fill.energy = 1.2
fill_obj = bpy.data.objects.new("fill", fill)
scene.collection.objects.link(fill_obj)
fill_obj.rotation_euler = (math.radians(60), 0.0, math.radians(215))

# A ground plane at z = 0 so contact reads.
bpy.ops.mesh.primitive_plane_add(size=extent * 8.0, location=(centre.x, centre.y, 0.0))
ground = bpy.context.active_object
material = bpy.data.materials.new("ground")
material.use_nodes = True
material.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = (0.42, 0.43, 0.45, 1.0)
ground.data.materials.append(material)

scene.render.engine = "CYCLES"
scene.cycles.device = "CPU"
scene.cycles.samples = args["samples"]
scene.render.resolution_x = args["cell"]
scene.render.resolution_y = args["cell"]
scene.render.film_transparent = False
scene.render.image_settings.file_format = "PNG"
scene.render.fps = 24

DIRECTIONS = {
    # Blender is Z-up: the model's glTF front (+Z) imports as -Y.
    "front": (0.0, -1.0, 0.28),
    "side": (1.0, 0.0, 0.28),
    "threequarter": (0.8, -0.85, 0.5),
}

def set_view(view):
    direction = Vector(DIRECTIONS[view]).normalized()
    camera.location = centre + direction * (extent * 3.0)
    look = centre - camera.location
    camera.rotation_euler = look.to_track_quat("-Z", "Y").to_euler()

armature = armatures[0] if armatures else None
actions = {action.name: action for action in bpy.data.actions}
written = []
for clip in args["clips"]:
    name = clip["name"]
    action = actions.get(name)
    if action is None:
        for candidate in actions.values():
            if name in candidate.name:
                action = candidate
                break
    if action is not None and armature is not None:
        if armature.animation_data is None:
            armature.animation_data_create()
        armature.animation_data.use_nla = False
        armature.animation_data.action = action
        if hasattr(action, "slots") and action.slots:
            armature.animation_data.action_slot = action.slots[0]
    duration = float(clip["duration"])
    if duration <= 0.0:
        times = [0.0]
    else:
        times = [duration * index / max(1, args["frames_per_clip"]) for index in range(args["frames_per_clip"] + 1)]
        times = times[:-1]
    for frame_index, when in enumerate(times):
        if action is not None:
            scene.frame_set(-1)
            scene.frame_set(1 + round(when * scene.render.fps))
        else:
            scene.frame_set(0)
        bpy.context.view_layer.update()
        for view in args["views"]:
            set_view(view)
            out = "%s/%s__%s__%d.png" % (args["out"], name.replace("/", "_"), view, frame_index)
            scene.render.filepath = out
            bpy.ops.render.render(write_still=True)
            written.append(out)
print("CONTACT_SHEET_OK " + json.dumps(written))
'''


def blender_binary() -> Optional[str]:
    found = shutil.which("blender")
    if found:
        return found
    candidate = Path("/opt/homebrew/bin/blender")
    return str(candidate) if candidate.is_file() else None


def clip_table(glb: Path) -> List[dict]:
    sys.path.insert(0, str(REPO_ROOT / "tools" / "entities"))
    import rig  # noqa: E402  (tools/entities on sys.path)

    document, _binary = rig._load_glb(glb)
    clips = []
    durations: Dict[str, float] = {}
    for animation in document.get("animations", []):
        name = animation.get("name", "")
        duration = 0.0
        for channel in animation.get("channels", []):
            sampler = animation["samplers"][channel["sampler"]]
            accessor = document["accessors"][sampler["input"]]
            if accessor.get("max"):
                duration = max(duration, float(accessor["max"][0]))
        durations[name] = duration
    marker = document.get("asset", {}).get("extras", {}).get("places_entity_clips", {})
    for name, duration in durations.items():
        clips.append({"name": name, "duration": duration})
    return clips


def render_asset(task: Tuple[str, str, int, int, int, int, List[str], int]) -> dict:
    glb, out_dir, workers_threads, samples, cell, frames_per_clip, views, _worker = task
    glb_path = Path(glb)
    scratch = Path(out_dir) / "cells" / glb_path.stem
    scratch.mkdir(parents=True, exist_ok=True)
    clips = clip_table(glb_path)
    script = Path(tempfile.mkstemp(prefix="places_entity_", suffix=".py", dir=scratch)[1])
    script.write_text(BLENDER_SCRIPT)
    arguments = {
        "glb": str(glb_path),
        "clips": clips,
        "out": str(scratch),
        "views": views,
        "samples": samples,
        "cell": cell,
        "frames_per_clip": frames_per_clip,
    }
    binary = blender_binary()
    if binary is None:
        return {"asset": str(glb_path), "ok": False, "error": "Blender not found", "images": {}}
    command = [
        binary,
        "--background",
        "--factory-startup",
        "--threads",
        str(workers_threads),
        "--python",
        str(script),
        "--",
        json.dumps(arguments),
    ]
    started = time.perf_counter()
    completed = subprocess.run(command, capture_output=True, text=True, check=False)
    elapsed = time.perf_counter() - started
    images: Dict[str, List[str]] = {}
    for clip in clips:
        for index in range(max(1, frames_per_clip if clip["duration"] > 0 else 1)):
            for view in views:
                path = scratch / f"{clip['name']}__{view}__{index}.png"
                if path.is_file():
                    images.setdefault(clip["name"], []).append(str(path))
    ok = bool(images) and "CONTACT_SHEET_OK" in completed.stdout
    return {
        "asset": str(glb_path),
        "ok": ok,
        "seconds": elapsed,
        "images": images,
        "error": None if ok else (completed.stderr or completed.stdout)[-400:],
    }


def read_png(path: str) -> Tuple[int, int, bytes]:
    data = Path(path).read_bytes()
    return decode_png(data)


def compose_sheet(asset: str, images: Dict[str, List[str]], views: Sequence[str], cell: int, out: Path) -> Optional[Path]:
    rows: List[Tuple[str, int, bytes]] = []
    clips = sorted(images)
    for clip in clips:
        paths = sorted(
            images[clip],
            key=lambda path: (
                int(Path(path).stem.rsplit("__", 1)[-1]),
                views.index(Path(path).stem.split("__")[1]) if "__" in Path(path).name else 0,
            ),
        )
        for path in paths:
            width, height, rgba = read_png(path)
            rows.append((Path(path).stem, width, rgba))
    if not rows:
        return None
    columns = len(views)
    grid_rows = (len(rows) + columns - 1) // columns
    gap = 2
    sheet_width = columns * cell + (columns + 1) * gap
    sheet_height = grid_rows * cell + (grid_rows + 1) * gap
    canvas = bytearray(sheet_width * sheet_height * 4)
    for index in range(sheet_width * sheet_height):
        canvas[index * 4 : index * 4 + 3] = bytes((24, 24, 28))
        canvas[index * 4 + 3] = 255
    for index, (name, width, rgba) in enumerate(rows):
        column = index % columns
        row = index // columns
        x0 = gap + column * (cell + gap)
        y0 = gap + row * (cell + gap)
        for y in range(min(cell, width and rows[0][1] or cell)):
            source = y * width * 4
            destination = ((y0 + y) * sheet_width + x0) * 4
            canvas[destination : destination + cell * 4] = rgba[source : source + cell * 4]
    sheet_path = out / f"{Path(asset).stem}_contact_sheet.png"
    sheet_path.parent.mkdir(parents=True, exist_ok=True)
    sheet_path.write_bytes(write_png(sheet_width, sheet_height, bytes(canvas)))
    return sheet_path


def parse_workers(requested: Optional[str]) -> Tuple[int, str]:
    raw = requested if requested is not None else os.environ.get("PLACES_TOOL_WORKERS")
    usable = os.process_cpu_count() if hasattr(os, "process_cpu_count") else os.cpu_count()
    ceiling = min(MAX_WORKERS_CEILING, usable or 1)
    if raw is None or str(raw).strip() == "":
        return ceiling, f"automatic: min(12, {usable} usable CPUs)"
    try:
        value = int(str(raw).strip())
    except ValueError:
        raise SystemExit(f"--workers must be an integer, got {raw!r}")
    if value < 1:
        raise SystemExit("--workers must be >= 1")
    if value > ceiling:
        return ceiling, f"requested {value}, reduced to {ceiling} (CPU budget)"
    return value, "requested"


def main(argv: Optional[List[str]] = None) -> int:
    parser = argparse.ArgumentParser(description="Blender contact sheets for the entity assets")
    parser.add_argument("--glb", action="append", default=None, help="entity GLB (repeatable)")
    parser.add_argument("--workers", default=None, help="concurrent Blender processes")
    parser.add_argument("--blender-threads", type=int, default=2, help="native threads per Blender process")
    parser.add_argument("--samples", type=int, default=16, help="Cycles samples per cell")
    parser.add_argument("--cell", type=int, default=CELL)
    parser.add_argument("--frames-per-clip", type=int, default=3, help="loop samples per clip")
    parser.add_argument("--views", nargs="*", default=["front", "side", "threequarter"])
    parser.add_argument("--out", default=DEFAULT_OUT)
    args = parser.parse_args(argv)

    assets = [Path(path) for path in (args.glb or DEFAULT_ASSETS)]
    missing = [str(path) for path in assets if not path.is_file()]
    if missing:
        raise SystemExit("missing entity GLB(s): " + ", ".join(missing))
    workers, note = parse_workers(args.workers)
    effective = min(workers, len(assets))
    print(
        f"[entities] render: {len(assets)} asset(s), workers={effective} ({note}; "
        f"blender threads={args.blender_threads}), {args.cell}px x {args.cell}px"
    )
    if effective < workers:
        print(
            f"[entities] worker count reduced to {effective}: only {len(assets)} "
            "independent asset(s) to render in parallel"
        )
    tasks = [
        (
            str(asset),
            args.out,
            args.blender_threads,
            args.samples,
            args.cell,
            args.frames_per_clip,
            list(args.views),
            index,
        )
        for index, asset in enumerate(assets)
    ]
    started = time.perf_counter()
    if effective <= 1:
        results = [render_asset(task) for task in tasks]
    else:
        context = multiprocessing.get_context("spawn")
        with context.Pool(processes=effective) as pool:
            results = pool.map(render_asset, tasks)
    elapsed = time.perf_counter() - started
    failures = 0
    for result in results:
        if not result["ok"]:
            failures += 1
            print(f"[entities] FAILED {result['asset']}: {result.get('error')}")
            continue
        sheet = compose_sheet(result["asset"], result["images"], args.views, args.cell, Path(args.out))
        print(
            f"[entities] {Path(result['asset']).name}: {sum(len(v) for v in result['images'].values())} "
            f"cell(s), {result['seconds']:.2f} s -> {sheet}"
        )
    print(f"[entities] render done in {elapsed:.2f} s wall")
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
