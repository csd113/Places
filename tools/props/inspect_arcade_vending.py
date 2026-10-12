#!/usr/bin/env python3
"""Opt-in installed-Blender inspection of the toolkit-authored soda machine.

Run Blender --background --python this_file -- --out /tmp/vending-review.
The GLB builder remains authoritative; --save-source retains an editable
inspection scene. No map bake, rendering or Blender dependency enters tests.
"""
from __future__ import annotations

import argparse
from pathlib import Path
import sys


def main():
    import bpy
    from mathutils import Vector

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--model", type=Path)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--save-source", type=Path)
    args = parser.parse_args(sys.argv[sys.argv.index("--")+1:])
    root = Path(__file__).resolve().parents[2]
    model = args.model or root / "assets/environments/arcade/props/vending_machine.glb"
    args.out.mkdir(parents=True, exist_ok=True)
    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.object.delete(use_global=False)
    bpy.ops.import_scene.gltf(filepath=str(model.resolve()))
    machine = [obj for obj in bpy.context.scene.objects if obj.type == "MESH"]
    for obj in machine:
        obj.name = "Bepsi_"+obj.name
    # Places modulates material emission by albedo. Mirror that existing
    # runtime contract in the inspection shader (without editing the GLB).
    for material in bpy.data.materials:
        if material.use_nodes:
            nodes = material.node_tree.nodes
            shader = next((n for n in nodes if n.type == "BSDF_PRINCIPLED"), None)
            if shader and shader.inputs['Emission Strength'].default_value > 0:
                links = shader.inputs['Base Color'].links
                if links:
                    material.node_tree.links.new(links[0].from_socket, shader.inputs['Emission Color'])
    scene = bpy.context.scene
    scene.render.engine = "CYCLES"
    scene.cycles.device = "CPU"
    scene.cycles.samples = 16
    scene.render.resolution_x = 512
    scene.render.resolution_y = 640
    scene.render.resolution_percentage = 100
    scene.world.color = (.06, .06, .06)
    scene.view_settings.view_transform = "Standard"
    # Blender glTF importer maps +Y-up to its own +Z-up; +Z front becomes -Y.
    target = Vector((0, 0, 1.02))
    bpy.ops.object.camera_add(location=(2.45, -4.1, 2.45))
    camera = bpy.context.object
    camera.name = "Inspection_camera"
    camera.data.type = "ORTHO"
    camera.data.ortho_scale = 2.7
    scene.camera = camera
    for loc, energy, size in (((-3, -4, 5), 280, 4), ((3, 1, 3), 180, 3)):
        bpy.ops.object.light_add(type="AREA", location=loc)
        light = bpy.context.object
        light.data.energy = energy
        light.data.shape = "DISK"
        light.data.size = size
        light.rotation_euler = (target-light.location).to_track_quat("-Z", "Y").to_euler()
    for name, location in (("front", (0, -4.1, 1.30)),
                           ("quarter", (2.45, -4.1, 2.45))):
        camera.location = location
        camera.rotation_euler = (target-camera.location).to_track_quat("-Z", "Y").to_euler()
        scene.render.filepath = str((args.out / (name+".png")).resolve())
        bpy.ops.render.render(write_still=True)
    if args.save_source:
        # Pack the embedded artwork so reopening the editable inspection scene
        # never depends on Blender's temporary glTF extraction directory.
        bpy.ops.file.pack_all()
        bpy.context.preferences.filepaths.save_version = 0
        bpy.ops.wm.save_as_mainfile(filepath=str(args.save_source.resolve()))
    print("VENDING_INSPECTION", len(machine), "meshes",
          sum(len(obj.data.polygons) for obj in machine), "faces")


if __name__ == "__main__":
    main()
