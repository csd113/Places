#!/usr/bin/env python3
"""Explicit Blender authoring/three-pass inspection of commercial restroom stock.

Blender --background --threads 2 --python this.py -- --pass A|B|C --preview PATH
Only pass C publishes GLBs, measured metadata and the editable source scene.
The existing committed 32-square palette is loaded, never repainted. No Blender
or shipped-map bake is part of default tests. +Y up, +Z front, metres.
"""
from __future__ import annotations
import argparse
import json
import math
from pathlib import Path
import struct
import sys

REPO = Path(__file__).resolve().parents[2]
ROOT = REPO / 'assets/environments/arcade/restroom'
PALETTE = REPO / 'assets/environments/arcade/party/models/party_palette.png'
SWATCH = {'cream': 0, 'white': 2, 'metal': 3, 'dark': 4}


def author(stage, preview):
    import bpy
    import bmesh
    from mathutils import Vector
    bpy.ops.object.select_all(action='SELECT')
    bpy.ops.object.delete(use_global=False)
    image = bpy.data.images.load(str(PALETTE), check_existing=True)
    mats = {}
    pieces = []
    kits = []
    metadata = {'authoring_tool': bpy.app.version_string, 'refinement_pass': stage,
                'palette': str(PALETTE.relative_to(REPO / 'assets')), 'models': {}}

    def point(v):
        return (v[0], -v[2], v[1])

    def material(name, rough=.65, metal=0, tint=(1, 1, 1)):
        if name not in mats:
            mat = bpy.data.materials.new('restroom_' + name)
            mat.use_nodes = True
            node = mat.node_tree.nodes.get('Principled BSDF')
            node.inputs['Roughness'].default_value = rough
            node.inputs['Metallic'].default_value = metal
            tex = mat.node_tree.nodes.new('ShaderNodeTexImage')
            tex.image = image
            tex.extension = 'EXTEND'
            # A material factor is exported as glTF baseColorFactor.
            if tint != (1, 1, 1):
                mix = mat.node_tree.nodes.new('ShaderNodeMixRGB')
                mix.blend_type = 'MULTIPLY'
                mix.inputs[0].default_value = 1
                mix.inputs[2].default_value = (*tint, 1)
                mat.node_tree.links.new(tex.outputs['Color'], mix.inputs[1])
                mat.node_tree.links.new(mix.outputs[0], node.inputs['Base Color'])
            else:
                mat.node_tree.links.new(tex.outputs['Color'], node.inputs['Base Color'])
            mats[name] = mat
        return mats[name]

    porcelain = material('porcelain', .24)
    metal = material('chrome', .22, .8)
    laminate = material('muted_teal_laminate', .6, tint=(.19, .34, .32))
    stock = material('stock')
    dark = material('recess', .86)

    def finish(obj, mat, color, smooth=False):
        obj.data.materials.append(mat)
        uv = obj.data.uv_layers.active or obj.data.uv_layers.new(name='UVMap')
        i = SWATCH[color]
        for vertex in uv.data:
            vertex.uv = ((i % 4 + .5) / 4, 1 - (i // 4 + .5) / 4)
        for poly in obj.data.polygons:
            poly.use_smooth = smooth
        bm = bmesh.new()
        bm.from_mesh(obj.data)
        bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
        bm.to_mesh(obj.data)
        bm.free()
        pieces.append(obj)
        return obj

    def mesh(name, vertices, faces, mat, color, smooth=False):
        data = bpy.data.meshes.new(name)
        data.from_pydata([point(v) for v in vertices], [], faces)
        data.update()
        obj = bpy.data.objects.new(name, data)
        bpy.context.collection.objects.link(obj)
        return finish(obj, mat, color, smooth)

    def box(name, c, size, mat=stock, color='white', bevel=0):
        bpy.ops.mesh.primitive_cube_add(size=1, location=point(c))
        obj = bpy.context.object
        obj.name = name
        obj.dimensions = (size[0], size[2], size[1])
        bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
        if bevel and stage != 'A':
            mod = obj.modifiers.new('selective stock edge', 'BEVEL')
            mod.width = bevel
            mod.segments = 1
            bpy.ops.object.modifier_apply(modifier=mod.name)
        return finish(obj, mat, color)

    def beam(name, a, b, radius=.016, mat=metal, color='metal', segments=8):
        a, b = Vector(point(a)), Vector(point(b))
        bpy.ops.mesh.primitive_cylinder_add(vertices=segments, radius=radius,
            depth=(b-a).length, location=(a+b)/2)
        obj = bpy.context.object
        obj.name = name
        obj.rotation_euler = (b-a).to_track_quat('Z', 'Y').to_euler()
        bpy.ops.object.transform_apply(location=False, rotation=True, scale=True)
        return finish(obj, mat, color)

    def oval(name, center, profile, mat=porcelain, color='white', segments=12, loop=False):
        cx, cy, cz = center
        verts = [(cx+rx*math.cos(i*math.tau/segments), cy+y,
                  cz+rz*math.sin(i*math.tau/segments))
                 for y, rx, rz in profile for i in range(segments)]
        faces = []
        for row in range(len(profile) if loop else len(profile)-1):
            nxt = (row+1) % len(profile)
            for i in range(segments):
                j = (i+1) % segments
                faces.append((row*segments+i, row*segments+j, nxt*segments+j, nxt*segments+i))
        if not loop:
            faces.extend((tuple(reversed(range(segments))),
                tuple((len(profile)-1)*segments+i for i in range(segments))))
        return mesh(name, verts, faces, mat, color, smooth=stage != 'A')

    def publish(name, **extra):
        nonlocal pieces
        bpy.ops.object.select_all(action='DESELECT')
        for obj in pieces:
            obj.select_set(True)
        bpy.context.view_layer.objects.active = pieces[0]
        bpy.ops.object.join()
        obj = bpy.context.object
        obj.name = name
        # Exact centering/contact is measured from real geometry, not estimates.
        coords = [obj.matrix_world @ v.co for v in obj.data.vertices]
        low = [min(v[i] for v in coords) for i in range(3)]
        high = [max(v[i] for v in coords) for i in range(3)]
        shift = Vector((-(low[0]+high[0])/2, -(low[1]+high[1])/2, -low[2]))
        for v in obj.data.vertices:
            v.co = obj.matrix_world @ v.co + shift
        obj.matrix_world.identity()
        size = [high[0]-low[0], high[2]-low[2], high[1]-low[1]]
        for key in ('seat_height', 'deck_height'):
            if key in extra:
                extra[key] = round(extra[key] - low[2], 6)
        triangles = sum(len(p.vertices)-2 for p in obj.data.polygons)
        if triangles > 800:
            raise ValueError(f'{name}: {triangles} triangles exceeds review threshold')
        relative = f'environments/arcade/restroom/{name}.glb'
        metadata['models']['arcade:'+name] = dict(model=relative,
            size=[round(v, 6) for v in size], triangles=triangles,
            source_scene='environments/arcade/restroom/restroom.blend', **extra)
        if stage == 'C':
            ROOT.mkdir(parents=True, exist_ok=True)
            bpy.ops.export_scene.gltf(filepath=str(ROOT / (name+'.glb')),
                export_format='GLB', use_selection=True, export_yup=True,
                export_texcoords=True, export_normals=True, export_materials='EXPORT',
                export_image_format='AUTO', export_animations=False,
                export_cameras=False, export_lights=False)
            # Blender's exporter drops a Multiply node's scalar on this small
            # swatch shader. Keep the same authored tint as an ordinary glTF
            # baseColorFactor, a field supported by the Places importer.
            path = ROOT / (name+'.glb')
            payload = path.read_bytes()
            length = struct.unpack_from('<I', payload, 12)[0]
            document = json.loads(payload[20:20+length])
            for exported in document.get('materials', []):
                if exported.get('name') == 'restroom_muted_teal_laminate':
                    exported['pbrMetallicRoughness']['baseColorFactor'] = [.19,.34,.32,1]
            encoded = json.dumps(document,separators=(',',':')).encode()
            encoded += b' '*((-len(encoded))%4)
            tail = payload[20+length:]
            path.write_bytes(struct.pack('<III',0x46546c67,2,20+len(encoded)+len(tail)) +
                struct.pack('<II',len(encoded),0x4e4f534a) + encoded + tail)
        kits.append(obj)
        pieces = []

    oval('floor foot and porcelain bowl', (0, 0, .08),
        [(0,.11,.16),(.025,.12,.17),(.12,.095,.125),(.24,.115,.15),
         (.39,.215,.285),(.435,.215,.285),(.435,.158,.22),(.30,.075,.11)], segments=12)
    oval('real open seat ring', (0,0,.08),
        [(.45,.221,.294),(.474,.221,.294),(.474,.162,.227),(.45,.162,.227)],
        color='cream', loop=True)
    box('tank', (0,.655,-.225), (.38,.40,.19), porcelain, bevel=.025)
    box('tank lid', (0,.869,-.225), (.40,.028,.205), porcelain, bevel=.008)
    if stage != 'A':
        box('seat hinge bridge', (0,.467,-.192), (.22,.035,.06), porcelain)
        beam('flush lever', (-.202,.73,-.17),(-.202,.73,-.24),.018)
        oval('ordinary water at bowl bottom', (0,0,.08),
            [(.302,.073,.108),(.305,.073,.108)], stock, 'metal')
    publish('restroom_toilet', seat_height=.474)

    # Sink origin is the lowest trap contact. Its deck is placed at 0.83 m.
    oval('porcelain basin with real concave interior', (0,0,.015),
        [(.16,.11,.09),(.22,.25,.21),(.29,.33,.265),(.315,.34,.275),
         (.315,.275,.21),(.29,.27,.205),(.21,.08,.055)], segments=12)
    # A raised integral ledge laps the bowl behind its rim; its top is clear
    # of the rim plane, avoiding the coplanar dark seam found in pass A.
    box('rear mounting ledge', (0,.328,-.20), (.64,.04,.12), porcelain, bevel=.008)
    beam('tap pedestal',(0,.348,-.19),(0,.47,-.19),.027)
    beam('tap spout',(0,.47,-.19),(0,.47,-.05),.022)
    if stage != 'A':
        beam('aerator',(0,.47,-.05),(0,.437,-.05),.021)
        for x in (-.13,.13):
            beam('tap valve',(x,.348,-.19),(x,.385,-.19),.022)
            beam('tap lever',(x-.035,.385,-.19),(x+.035,.385,-.19),.012)
        beam('drain tail',(0,.2,.015),(0,.025,.015),.035)
        beam('trap bottom',(0,.025,.015),(0,.025,-.09),.035)
        beam('trap rear',(0,.025,-.09),(0,.13,-.09),.035)
        beam('trap wall return',(0,.13,-.09),(0,.13,-.26),.035)
        oval('drain strainer', (0,0,.015),[(.212,.035,.035),(.215,.035,.035)],metal,'metal',segments=8)
    publish('restroom_sink', deck_height=.348)

    for name, length in [('divider',2.50),('front',.90),('front_wide',1.50)]:
        box('laminate slab',(0,1.005,0),(length,1.71,.042),laminate,bevel=.004)
        for x in (-length/2+.055,length/2-.055):
            box('pilaster',(x,1.0,0),(.082,1.98,.06),laminate)
            box('steel foot',(x,.075,0),(.10,.15,.078),metal,'metal')
        if stage == 'C':
            box('head rail',(0,1.977,0),(length,.038,.042),metal,'metal')
        publish('restroom_stall_'+name)

    box('flat commercial stall leaf',(0,.84,0),(1.12,1.68,.042),laminate,bevel=.005)
    if stage != 'A':
        for y in (.18,1.50):
            beam('hinge barrel',(-.548,y-.045,.018),(-.548,y+.045,.018),.018)
        box('slide latch',(.42,.87,.032),(.13,.055,.020),metal,'metal')
        box('latch pull',(.42,.87,.055),(.07,.018,.023),metal,'metal')
        box('rear latch plate',(.42,.87,-.03),(.10,.06,.018),metal,'metal')
    publish('restroom_stall_door', fixed_open=True)

    # Open metal frame; this asset deliberately contains no mirror-face image.
    outer=[(-.484,0),(.484,0),(.484,.91),(-.484,.91)]
    inner=[(-.45,.034),(.45,.034),(.45,.876),(-.45,.876)]
    vertices=[(x,y,z) for z in (-.021,.021) for ring in (outer,inner) for x,y in ring]
    faces=[]
    for i in range(4):
        j=(i+1)%4
        for a,b in ((0,4),(8,12),(0,8),(4,12)):
            faces.append((a+i,a+j,b+j,b+i))
    mesh('one closed open mirror frame',vertices,faces,metal,'metal')
    publish('restroom_mirror_frame', opening_size=[.9,.842])

    box('soap case',(0,.12,0),(.16,.24,.11),stock,'cream',bevel=.01)
    if stage != 'A':
        box('soap push bar',(0,.075,.06),(.11,.07,.018),metal,'metal')
        box('soap viewing slot',(0,.18,.059),(.025,.055,.009),dark,'dark')
    publish('restroom_soap_dispenser')

    # Simple bent grab bar and flange stock for the larger stall.
    beam('grab bar',(-.42,.05,.055),(.42,.05,.055),.018)
    for x in (-.42,.42):
        beam('return',(x,.05,.055),(x,.05,-.006),.018)
        box('mount flange',(x,.05,-.014),(.065,.10,.018),metal,'metal')
    publish('restroom_grab_bar')

    beam('paper spindle',(-.055,.075,0),(.055,.075,0),.052,stock,'white')
    for x in (-.083,.083):
        box('paper bracket',(x,.07,-.017),(.025,.14,.08),metal,'metal')
    publish('restroom_paper_holder')

    # Purposeful inspection arrangement: porcelain detail and assembly stock.
    for i, obj in enumerate(kits):
        obj.location=(i%5*1.75-3.5, i//5*2.5, 0)
    scene=bpy.context.scene
    scene.render.engine='CYCLES'
    scene.cycles.samples=12
    scene.cycles.device='CPU'
    scene.render.resolution_x=1400
    scene.render.resolution_y=850
    scene.render.resolution_percentage=100
    scene.world.color=(.12,.12,.12)
    scene.view_settings.view_transform='Standard'
    for loc,energy in [((-3,-4,7),600),((5,2,6),450)]:
        bpy.ops.object.light_add(type='AREA',location=loc)
        light=bpy.context.object
        light.data.energy=energy
        light.data.shape='DISK'
        light.data.size=5
        light.rotation_euler=(Vector((0,0,.8))-light.location).to_track_quat('-Z','Y').to_euler()
    bpy.ops.object.camera_add(location=(7,-11,9))
    camera=bpy.context.object
    camera.data.type='ORTHO'
    camera.data.ortho_scale=11
    camera.rotation_euler=(Vector((0,.8,.7))-camera.location).to_track_quat('-Z','Y').to_euler()
    scene.camera=camera
    if preview:
        output=Path(preview)
        output.parent.mkdir(parents=True,exist_ok=True)
        scene.render.filepath=str(output)
        bpy.ops.render.render(write_still=True)
        camera.location=(-1.4,-3.8,2.7)
        camera.rotation_euler=(Vector((-2.65,0,.45))-camera.location).to_track_quat('-Z','Y').to_euler()
        camera.data.ortho_scale=2.8
        scene.render.resolution_x=1000
        scene.render.resolution_y=800
        scene.render.filepath=str(output.with_name(output.stem+'-porcelain.png'))
        bpy.ops.render.render(write_still=True)
    if stage=='C':
        image.pack()
        bpy.context.preferences.filepaths.save_version=0
        bpy.ops.wm.save_as_mainfile(filepath=str(ROOT/'restroom.blend'),compress=True)
        (ROOT/'asset_metadata.json').write_text(json.dumps(metadata,indent=2)+'\n')
    print('RESTROOM_INSPECTION',json.dumps(metadata))


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--pass',dest='stage',choices=['A','B','C'],required=True)
    parser.add_argument('--preview')
    args=parser.parse_args(sys.argv[sys.argv.index('--')+1:])
    author(args.stage,args.preview)


if __name__=='__main__':
    main()
