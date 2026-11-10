"""Offline tool meshes; run Blender --python this_file from the repository root.

Reuses the accepted mannequin and committed PNGs verbatim. This exports only
new cropped/posed geometry; it never rewrites the world mannequin or paints an
image. Both view meshes use a shared grip origin, +Y up, +Z toward the eye.
"""
from pathlib import Path
import hashlib
import json
import math
import sys

import bpy
from mathutils import Vector

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))
sys.path.insert(0, str(ROOT / "tools/props"))
from tools.props.mesh import Mesh
from tools.props.glb import write_glb

SOURCE = ROOT / "assets/entities/mannequin/model/mannequin.glb"
EXPECTED = "a29104cafb8d78beccce14c353cea0436a97a5f40973c38959fe7cea02f85605"
OUT = ROOT / "assets/shared/tools"


def arm_mesh():
    assert hashlib.sha256(SOURCE.read_bytes()).hexdigest() == EXPECTED, "Review the replaced rig first"
    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.object.delete(use_global=False)
    bpy.ops.import_scene.gltf(filepath=str(SOURCE))
    rig = next(o for o in bpy.context.scene.objects if o.type == "ARMATURE")
    action = next(a for a in bpy.data.actions if "carry_idle" in a.name)
    rig.animation_data_create()
    rig.animation_data.action = action
    if hasattr(action, "slots") and action.slots:
        rig.animation_data.action_slot = action.slots[0]
    bpy.context.scene.frame_set(1)
    # Close the original articulated fingers around the shaft. Rotations are
    # baked into this small static view mesh, never into the installed rig.
    for bone in rig.pose.bones:
        if bone.name.endswith(".R") and any(n in bone.name for n in ("index", "middle", "ring", "pinky")):
            bone.rotation_mode = "XYZ"
            bone.rotation_euler.x = 1.1 if ".01." in bone.name else 1.25
    bpy.context.view_layer.update()
    hand = rig.matrix_world @ rig.pose.bones["hand.R"].matrix
    inverse = hand.inverted()
    grip = Vector((0.0, 0.055, 0.012))
    mesh = Mesh()
    for obj in list(bpy.context.scene.objects):
        if obj.type != "MESH":
            continue
        evaluated = obj.evaluated_get(bpy.context.evaluated_depsgraph_get())
        data = evaluated.to_mesh()
        data.calc_loop_triangles()
        groups = {g.index: g.name for g in obj.vertex_groups}
        def included(index):
            v = obj.data.vertices[index]
            return sum(g.weight for g in v.groups if groups[g.group].endswith(".R")
                       and groups[g.group] not in {"upper_arm.R", "thigh.R", "shin.R", "foot.R"}) > 0.99
        uv = data.uv_layers.active.data
        for tri in data.loop_triangles:
            if not all(included(i) for i in tri.vertices):
                continue
            points = []
            for index in tri.vertices:
                local = inverse @ (evaluated.matrix_world @ data.vertices[index].co) - grip
                points.append((-local.y, local.x, local.z))
            mesh.triangle(*points, [tuple(uv[i].uv) for i in tri.loops], (255, 255, 255))
        evaluated.to_mesh_clear()
    assert 100 < len(mesh.indices) // 3 < 1500, "Unexpected cropped arm topology"
    return mesh


def bar_mesh():
    mesh = Mesh()
    mesh.roughness = 0.7
    # Hexagonal painted shaft, a rounded heel and a tapered pry hook. The hook
    # bends away from the camera; bare steel ends read independently of paint.
    path = [(0.0, y, 0.0, 0.011) for y in (-0.15, -0.10, 0.0, 0.20, 0.32)]
    for i in range(1, 9):
        a = math.pi * i / 12
        path.append((0.0, 0.32 + 0.065 * math.sin(a), -0.065 + 0.065 * math.cos(a),
                     0.011 * (1.0 - i / 16)))
    rings = []
    for i, (x, y, z, radius) in enumerate(path):
        previous = Vector(path[max(0, i - 1)][:3])
        following = Vector(path[min(len(path) - 1, i + 1)][:3])
        tangent = (following - previous).normalized()
        right = Vector((1, 0, 0))
        side = tangent.cross(right).normalized()
        centre = Vector((x, y, z))
        rings.append([tuple(centre + radius * (math.cos(j * math.tau / 6) * right
                      + math.sin(j * math.tau / 6) * side)) for j in range(6)])
    for i in range(len(rings) - 1):
        colour = (138, 42, 31) if 0 < i < 7 else (126, 132, 136)
        for j in range(6):
            k = (j + 1) % 6
            mesh.quad(rings[i][j], rings[i][k], rings[i + 1][k], rings[i + 1][j],
                      (0.0, 0.0, 1.0, 1.0), colour)
    for ring, reverse in [(rings[0], True), (rings[-1], False)]:
        for i in range(1, 5):
            points = (ring[0], ring[i], ring[i + 1])
            mesh.triangle(*(points[::-1] if reverse else points), [(0.5, 0.5)] * 3, (126,132,136))
    # Split claw on the straight heel, using two closed tapered steel tines.
    for x in (-0.007, 0.007):
        mesh.box((x, -0.169, -0.009), (0.009, 0.045, 0.008),
                 color=(126,132,136), uv=(0.0,0.0,1.0,1.0), shade=False)
    return mesh


OUT.mkdir(parents=True, exist_ok=True)
results = {}
for name, mesh, sheet in [
    ("right_arm", arm_mesh(), ROOT / "assets/entities/mannequin/textures/mannequin-skin.png"),
    ("crowbar", bar_mesh(), ROOT / "assets/shared/textures/white_01.png"),
]:
    target = OUT / (name + ".glb")
    target.write_bytes(write_glb(mesh, sheet.read_bytes(), name))
    results[name] = {"triangles": len(mesh.indices) // 3, "bounds": mesh.bounds(),
                     "sha256": hashlib.sha256(target.read_bytes()).hexdigest(),
                     "png": str(sheet.relative_to(ROOT)),
                     "png_sha256": hashlib.sha256(sheet.read_bytes()).hexdigest()}
(OUT / "provenance.json").write_text(json.dumps({"source_mannequin": EXPECTED,
    "contract": "grip origin; +Y up; +Z toward eye; original PNGs and arm UVs",
    "models": results}, indent=2) + "\n")
print(json.dumps(results, indent=2))
