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
EXPECTED = "4042ec146859a9184b6f5799a611d049091c2c03f58211ccef2e12479a04de8a"
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
    """Original forged bar built as a closed Blender mesh, bevelled before export."""
    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.object.delete(use_global=False)
    path = [(y, 0.0, 0.012, 0.012) for y in (-0.16, -0.10, 0.0, 0.22, 0.38)]
    for i in range(1, 17):
        angle = math.radians(155) * i / 16
        taper = max(0.0, (i-9)/7)
        path.append((0.38 + 0.075*math.sin(angle), 0.075*(1-math.cos(angle)),
                     0.012 + 0.009*taper, 0.012 - 0.0085*taper))
    vertices, faces, regions = [], [], []
    for i, (y,z,width,thick) in enumerate(path):
        prev = Vector((0,*path[max(0,i-1)][:2]))
        nxt = Vector((0,*path[min(len(path)-1,i+1)][:2]))
        tangent = (nxt-prev).normalized()
        side = tangent.cross(Vector((1,0,0))).normalized()
        for j in range(8):
            angle = math.tau*j/8 + math.pi/8
            vertices.append(tuple(Vector((0,y,z)) + Vector((width*math.cos(angle),0,0))
                                  + side*thick*math.sin(angle)))
    for i in range(len(path)-1):
        for j in range(8):
            faces.append((i*8+j,i*8+(j+1)%8,(i+1)*8+(j+1)%8,(i+1)*8+j))
            regions.append((i,j))
    faces.extend([tuple(reversed(range(8))), tuple((len(path)-1)*8+j for j in range(8))])
    regions.extend([(0,0),(len(path)-2,0)])
    data = bpy.data.meshes.new('Forged crowbar')
    data.from_pydata(vertices,[],faces); data.update()
    obj = bpy.data.objects.new('Crowbar / forged hook',data)
    authored = [obj]
    bpy.context.collection.objects.link(obj)
    bpy.context.view_layer.objects.active=obj; obj.select_set(True)
    uv = data.uv_layers.new(name='Fitted enamel and steel')
    for poly, (i,j) in zip(data.polygons,regions):
        bare = i == 0 or i >= 12
        v0,v1 = ((.025,.22) if bare else (.27,.97))
        for k, loop in enumerate(poly.loop_indices):
            u = .01 + .98*(j + (k in (1,2)))/8
            v = v0 + (v1-v0)*(i + (k in (2,3)))/(len(path)-1)
            uv.data[loop].uv=(u,v)
    bevel=obj.modifiers.new('Rounded forged edges','BEVEL');bevel.width=.0012;bevel.segments=2
    bpy.ops.object.modifier_apply(modifier=bevel.name)
    # Split flattened heel with genuine open notch, not a painted stripe.
    for sign in (-1,1):
        bpy.ops.mesh.primitive_cube_add(size=1,location=(sign*.008,-.183,.006))
        tine=bpy.context.object;tine.name='Split pry heel'
        authored.append(tine)
        tine.dimensions=(.012,.055,.008);bpy.ops.object.transform_apply(location=False,rotation=False,scale=True)
        bevel=tine.modifiers.new('Soft forged edge','BEVEL');bevel.width=.0012;bevel.segments=2
        bpy.ops.object.modifier_apply(modifier=bevel.name)
        for loop in tine.data.uv_layers.active.data: loop.uv=(.55,.12)
    mat=bpy.data.materials.new('Oxblood enamel / worn forged steel');mat.use_nodes=True
    shader=mat.node_tree.nodes.get('Principled BSDF');shader.inputs['Roughness'].default_value=.42
    shader.inputs['Metallic'].default_value=.55
    image=bpy.data.images.load(str(OUT/'crowbar.png'),check_existing=True)
    texture=mat.node_tree.nodes.new('ShaderNodeTexImage');texture.image=image
    mat.node_tree.links.new(texture.outputs['Color'],shader.inputs['Base Color'])
    mesh=Mesh();mesh.roughness=.42;mesh.metallic=.55
    for obj in authored:
        obj.data.materials.append(mat)
        data=obj.data;data.calc_loop_triangles();uv=data.uv_layers.active.data
        for tri in data.loop_triangles:
            mesh.triangle(*(tuple(obj.matrix_world@data.vertices[i].co) for i in tri.vertices),
                          [(uv[i].uv.x,1.0-uv[i].uv.y) for i in tri.loops],(255,255,255))
    image.pack()
    # Imported rigs may leave hidden custom bone-shape meshes. Only the three
    # authored objects belong to this asset, even when Blender hides a helper.
    for other in list(bpy.data.objects):
        if other not in authored:
            bpy.data.objects.remove(other, do_unlink=True)
    low, high = mesh.bounds()
    assert high[0]-low[0] < .06 and .6 < high[1]-low[1] < .7 and high[2]-low[2] < .2, (low,high)
    bpy.ops.wm.save_as_mainfile(filepath=str(ROOT/'tools/player/source/crowbar.blend'))
    return mesh


OUT.mkdir(parents=True, exist_ok=True)
results = {}
for name, mesh, sheet in [
    ("right_arm", arm_mesh(), ROOT / "assets/entities/mannequin/textures/mannequin-skin.png"),
    ("crowbar", bar_mesh(), OUT / "crowbar.png"),
]:
    target = OUT / (name + ".glb")
    target.write_bytes(write_glb(mesh, sheet.read_bytes(), name))
    results[name] = {"triangles": len(mesh.indices) // 3, "bounds": mesh.bounds(),
                     "sha256": hashlib.sha256(target.read_bytes()).hexdigest(),
                     "png": str(sheet.relative_to(ROOT)),
                     "png_sha256": hashlib.sha256(sheet.read_bytes()).hexdigest()}
(OUT / "provenance.json").write_text(json.dumps({"source_mannequin": EXPECTED,
    "contract": "grip origin; +Y up; +Z toward eye; retained mannequin arm UVs/PNG and original GIMP crowbar atlas",
    "models": results}, indent=2) + "\n")
print(json.dumps(results, indent=2))
