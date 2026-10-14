#!/usr/bin/env python3
"""Explicit Blender authoring of the reusable bowling birthday party kit.

Texture authoring: python3 tools/props/author_birthday_party.py --textures
Native editable source/export: Blender --background --threads 2 --python this.py
    -- --pass A|B|C [--preview /tmp/places-party-model-scratch/pass-b.png]

Normal builds only read committed PNG/GLB assets. This command deliberately
authors retained 1024-square masters, full 1024-square cloth natives,
512-square gift wraps, 256-square numeric ink masks and 128-square
small-prop print natives, then uses
installed Blender to construct, UV, refine and export actual native meshes.
No concept-board pixels are copied into texture artwork.
"""
from __future__ import annotations

import argparse
import json
import math
from pathlib import Path
import random
import sys

REPO = Path(__file__).resolve().parents[2]
ROOT = REPO / "assets/environments/arcade/party"
MODELS = ROOT / "models"
COLORS = {
    "cream": (238, 226, 200), "frosting": (249, 241, 219),
    "white": (244, 240, 226), "metal": (139, 146, 149),
    "charcoal": (35, 39, 45), "red": (205, 35, 53),
    "blue": (31, 78, 190), "yellow": (238, 187, 37),
    "green": (38, 145, 67), "purple": (102, 44, 155),
    "pink": (221, 53, 137), "orange": (230, 101, 39),
    "board": (199, 191, 166), "wax": (247, 235, 211),
    "string": (215, 210, 190), "latex": (248, 248, 248),
}
PATTERNS = ["confetti", "balloons", "stars", "arcade", "dots",
            "stripes", "geometric", "border", "red", "green", "pale_stars",
            "blue_confetti", "purple_shapes"]
PRINT_SEEDS = {name:1700+index for index,name in enumerate(
    ("confetti","balloons","stars","arcade","dots","shapes","stripes",
     "geometric","border","red","green","pale_stars","blue_confetti","purple_shapes"))}
CLOTH_PATTERNS = ("confetti", "balloons", "stars", "arcade")
WRAP_PATTERNS = ("stars", "confetti", "stripes", "arcade", "red", "green")
INK_PATTERNS = (*CLOTH_PATTERNS, "stripes", "red", "green")


def paint_textures():
    """Offline original print artwork, retained as PNGs before geometry export."""
    from PIL import Image, ImageDraw
    MODELS.mkdir(parents=True, exist_ok=True)
    palette = Image.new("RGB", (1024, 1024))
    draw = ImageDraw.Draw(palette)
    for index, color in enumerate(COLORS.values()):
        x, y = index % 4 * 256, index // 4 * 256
        draw.rectangle((x, y, x + 255, y + 255), fill=color)
    palette.save(MODELS / "party_palette_master.png")
    # A flat sixteen-swatch sheet needs no detail pixels. Small props stay at
    # legal lightweight native sizes while large cloth uses its full master.
    palette.resize((32, 32), Image.Resampling.LANCZOS).save(MODELS / "party_palette.png")
    accents = [COLORS[c] for c in ("red", "blue", "yellow", "green", "purple", "pink", "orange")]

    def star(d, x, y, radius, color, turn=0):
        points = []
        for i in range(10):
            angle = turn + i * math.pi / 5 - math.pi / 2
            r = radius if i % 2 == 0 else radius * .44
            points.append((x + r * math.cos(angle), y + r * math.sin(angle)))
        d.polygon(points, fill=color)

    for pattern in PATTERNS:
        bg = {
            "stars": (28, 63, 151), "arcade": (52, 25, 83),
            "red": (192, 39, 51), "green": (27, 104, 53),
            "blue_confetti": (29,67,145), "purple_shapes": (94,42,134),
        }.get(pattern, (236, 228, 213))
        art = Image.new("RGB", (1024, 1024), bg)
        d = ImageDraw.Draw(art)
        rng = random.Random(PRINT_SEEDS[pattern])
        if pattern == "stripes":
            for i in range(28):
                x = round(i * 1024 / 28)
                d.rectangle((x, 0, round((i+1)*1024/28), 1024), fill=accents[i % len(accents)])
        elif pattern == "border":
            for inset, color in ((26, accents[1]), (47, accents[5]), (63, accents[2])):
                d.ellipse((inset, inset, 1024-inset, 1024-inset), outline=color, width=13)
            for i in range(24):
                angle = i * math.tau / 24
                x, y = 512+410*math.cos(angle), 512+410*math.sin(angle)
                d.ellipse((x-10,y-10,x+10,y+10),fill=accents[i % 7])
        else:
            # Broad ink motifs survive the native derivative and normal view distances.
            for row in range(9):
                for col in range(9):
                    x = col*116 + 44 + (row % 2)*28 + rng.uniform(-15, 15)
                    y = row*116 + 52 + rng.uniform(-17, 17)
                    color = accents[(col + row*3) % 7]
                    if pattern in ("stars", "pale_stars"):
                        star(d, x, y, 19 + rng.random()*9, COLORS["yellow"] if pattern=="stars" else color, rng.random()*.8)
                        if (row+col) % 2:
                            d.line((x-30,y+29,x-17,y+35),fill=accents[5],width=9)
                    elif pattern == "balloons":
                        d.ellipse((x-19,y-27,x+19,y+21),fill=color)
                        d.polygon([(x-5,y+21),(x+5,y+21),(x,y+28)],fill=color)
                        d.line([(x,y+28),(x-6,y+42),(x-1,y+54)],fill=(112,103,97),width=4)
                    elif pattern == "dots":
                        r = 15 + (row+col) % 3*5
                        d.ellipse((x-r,y-r,x+r,y+r),fill=color)
                    elif pattern in ("shapes","purple_shapes"):
                        color = COLORS["yellow"] if (row+col)%2 or pattern=="purple_shapes" else COLORS["purple"]
                        d.polygon([(x-24,y+18),(x,y-26),(x+24,y+18)],fill=color)
                        d.rectangle((x+28,y+22,x+42,y+38),fill=COLORS["pink"])
                    elif pattern in ("arcade", "geometric", "green"):
                        color = COLORS["yellow"] if pattern == "green" else color
                        d.line([(x-23,y),(x,y-25),(x+23,y),(x,y+25),(x-23,y)],fill=color,width=9)
                        d.line((x+27,y-23,x+47,y-6),fill=COLORS["orange"],width=7)
                    elif pattern == "red":
                        color = (243,237,218)
                        d.arc((x-23,y-26,x+25,y+20),10,290,fill=color,width=8)
                        d.line([(x-17,y+24),(x-3,y+33),(x+10,y+18)],fill=color,width=8)
                    else:
                        if (row+col)%4 == 0:
                            star(d,x,y,20,color,rng.random())
                        elif (row+col)%4 == 1:
                            d.polygon([(x-17,y+14),(x,y-21),(x+16,y+15)],fill=color)
                        elif (row+col)%4 == 2:
                            d.ellipse((x-12,y-12,x+12,y+12),fill=color)
                        else:
                            d.line((x-14,y-8,x+18,y+11),fill=color,width=10)
                        d.rectangle((x+32,y+25,x+40,y+33),fill=accents[(row+col+2)%7])
        art.save(MODELS / f"party_print_{pattern}_master.png")
        if pattern not in ("arcade", "red", "green"):
            # These three patterns now belong only to full-fidelity cloth/wraps;
            # retain their original master without an unused small-prop derivative.
            art.resize((128,128),Image.Resampling.LANCZOS).save(MODELS / f"party_print_{pattern}.png")
        if pattern in CLOTH_PATTERNS:
            art.save(MODELS / f"party_cloth_{pattern}.png")
        if pattern in WRAP_PATTERNS:
            art.resize((512,512),Image.Resampling.LANCZOS).save(MODELS / f"party_wrap_{pattern}.png")
        if pattern in INK_PATTERNS:
            # Numeric coverage follows the actual original printed pixels.
            # Backing is nonreactive; each coloured printed mark reacts only
            # under an enabled UV fixture. Striped wrapping is entirely inked.
            pixels = art.tobytes()
            values = bytes(255 if tuple(pixels[i:i+3]) != bg else 0
                           for i in range(0, len(pixels), 3))
            coverage = Image.frombytes("L", art.size, values).convert("RGB")
            coverage.save(MODELS / f"party_ink_{pattern}_master.png")
            coverage.resize((256,256),Image.Resampling.LANCZOS).save(MODELS / f"party_ink_{pattern}.png")


def author_blender(stage, preview):
    import bpy
    from mathutils import Vector
    MODELS.mkdir(parents=True, exist_ok=True)
    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.object.delete(use_global=False)
    for datablock in list(bpy.data.collections):
        if datablock.name != "Collection":
            bpy.data.collections.remove(datablock)
    for mat in list(bpy.data.materials):
        bpy.data.materials.remove(mat)
    mats = {}
    images = {}
    metadata = {"authoring_tool": bpy.app.version_string, "refinement_pass": stage,
                "asset_class": "fitted model PNG (opaque)", "models": {}}
    all_objects = []
    current_objects = []

    def material(name, image="party_palette", roughness=.8, metal=0, emission=None):
        key = (name,image,roughness,metal,emission)
        if key in mats:
            return mats[key]
        m = bpy.data.materials.new(name)
        m.use_nodes = True
        bsdf = m.node_tree.nodes.get("Principled BSDF")
        bsdf.inputs["Roughness"].default_value = roughness
        bsdf.inputs["Metallic"].default_value = metal
        if image not in images:
            images[image] = bpy.data.images.load(str(MODELS / f"{image}.png"), check_existing=True)
        tex = m.node_tree.nodes.new("ShaderNodeTexImage")
        tex.image = images[image]
        tex.interpolation = "Linear"
        tex.extension = "EXTEND"
        m.node_tree.links.new(tex.outputs["Color"],bsdf.inputs["Base Color"])
        prefix = next((p for p in ("party_cloth_", "party_gift_paper_")
                       if name.startswith(p)), None)
        if prefix is not None:
            pattern = name.removeprefix(prefix)
            mask_name = "party_ink_" + pattern
            if mask_name not in images:
                images[mask_name] = bpy.data.images.load(str(MODELS / f"{mask_name}.png"), check_existing=True)
                images[mask_name].colorspace_settings.name = "Non-Color"
            mask_tex = m.node_tree.nodes.new("ShaderNodeTexImage")
            mask_tex.image = images[mask_name]
            mask_tex.label = "Numeric ink coverage: UV fluorescence, no emission"
            mask_tex.location = (-320, -320)
            m["uv_response"] = .65
            m["places_uv_mask_source"] = f"{mask_name}.png"
        if emission:
            bsdf.inputs["Emission Color"].default_value = (*emission,1)
            bsdf.inputs["Emission Strength"].default_value = .8
        mats[key] = m
        return m

    stock = material("party_matte_stock")
    frosting = material("party_cream_frosting",roughness=.86)
    metal = material("party_folded_grey_metal",roughness=.52,metal=.45)
    plastic = material("party_thin_printed_plastic",roughness=.49)
    ribbon = material("party_folded_ribbon",roughness=.62)
    latex = material("party_latex",roughness=.23)
    flame = material("party_tiny_candle_flame",roughness=1,emission=(1,.55,.09))

    def swatch(name):
        index = list(COLORS).index(name)
        return ((index%4+.5)/4,1-(index//4+.5)/4)

    def uv_color(obj, color):
        uv = obj.data.uv_layers.active or obj.data.uv_layers.new(name="UVMap")
        point = swatch(color)
        for d in uv.data:
            d.uv = point

    def uv_project(obj, mode="cube"):
        uv = obj.data.uv_layers.active or obj.data.uv_layers.new(name="UVMap")
        verts = obj.data.vertices
        low = [min(v.co[i] for v in verts) for i in range(3)]
        high = [max(v.co[i] for v in verts) for i in range(3)]
        for p in obj.data.polygons:
            axis = max(range(3),key=lambda i:abs(p.normal[i]))
            plane = ((1,2),(0,2),(0,1))[axis]
            for li in p.loop_indices:
                co = verts[obj.data.loops[li].vertex_index].co
                u = (co[plane[0]]-low[plane[0]]) / max(1e-7,high[plane[0]]-low[plane[0]])
                v = (co[plane[1]]-low[plane[1]]) / max(1e-7,high[plane[1]]-low[plane[1]])
                uv.data[li].uv = (.004+.992*u,.004+.992*v)

    def track(obj, mat, color=None, smooth=False):
        obj.data.materials.append(mat)
        if color:
            uv_color(obj,color)
        else:
            uv_project(obj)
        for p in obj.data.polygons:
            p.use_smooth = smooth
        current_objects.append(obj)
        return obj

    def box(name, center, size, mat=stock, color="white", bevel=0):
        bpy.ops.mesh.primitive_cube_add(size=1,location=center)
        obj = bpy.context.object
        obj.name=name
        obj.dimensions=size
        bpy.ops.object.transform_apply(location=False,rotation=False,scale=True)
        if bevel:
            modifier=obj.modifiers.new("authored edge thickness","BEVEL")
            modifier.width=bevel
            modifier.segments=1
            bpy.ops.object.modifier_apply(modifier=modifier.name)
        return track(obj,mat,color)

    def mesh(name, vertices, faces, mat=stock, color=None, smooth=False, uv=None):
        data = bpy.data.meshes.new(name+" mesh")
        data.from_pydata(vertices,[],faces)
        data.update()
        obj = bpy.data.objects.new(name,data)
        bpy.context.collection.objects.link(obj)
        track(obj,mat,color,smooth)
        if uv:
            layer=data.uv_layers.active
            for poly,coords in zip(data.polygons,uv):
                for li,co in zip(poly.loop_indices,coords):
                    layer.data[li].uv=co
        return obj

    def lathe(name, center, profile, mat=stock, color="white", segments=16, smooth=False, printed=False):
        verts=[]
        for z,r in profile:
            for i in range(segments):
                a=i*math.tau/segments
                verts.append((center[0]+r*math.cos(a),center[1]+r*math.sin(a),center[2]+z))
        faces=[];uv=[]
        for row in range(len(profile)-1):
            for i in range(segments):
                j=(i+1)%segments
                faces.append((row*segments+i,row*segments+j,(row+1)*segments+j,(row+1)*segments+i))
                z_min=min(z for z,r in profile);z_span=max(z for z,r in profile)-z_min
                v0=(profile[row][0]-z_min)/max(1e-7,z_span)
                v1=(profile[row+1][0]-z_min)/max(1e-7,z_span)
                uv.append(((i/segments,v0),((i+1)/segments,v0),((i+1)/segments,v1),(i/segments,v1)))
        faces += [tuple(reversed(range(segments))),tuple((len(profile)-1)*segments+i for i in range(segments))]
        for row in (0,len(profile)-1):
            coords=[(.5+.495*math.cos(i*math.tau/segments),.5+.495*math.sin(i*math.tau/segments)) for i in range(segments)]
            uv.append(list(reversed(coords)) if row==0 else coords)
        return mesh(name,verts,faces,mat,None if printed else color,smooth,uv if printed else None)

    def sphere(name,center,radius,mat=stock,color="white",scale=(1,1,1),segments=8,rings=4):
        bpy.ops.mesh.primitive_uv_sphere_add(segments=segments,ring_count=rings,radius=radius,location=center)
        obj=bpy.context.object;obj.name=name
        obj.scale=scale
        bpy.ops.object.transform_apply(location=False,rotation=False,scale=True)
        return track(obj,mat,color,True)

    def beam(name,a,b,radius,mat=metal,color="metal",segments=6):
        a,b=Vector(a),Vector(b)
        bpy.ops.mesh.primitive_cylinder_add(vertices=segments,radius=radius,depth=(b-a).length,location=(a+b)/2)
        obj=bpy.context.object;obj.name=name
        obj.rotation_euler=(b-a).to_track_quat("Z","Y").to_euler()
        bpy.ops.object.transform_apply(location=False,rotation=True,scale=True)
        return track(obj,mat,color)

    def merge(objects,name):
        bpy.ops.object.select_all(action="DESELECT")
        for obj in objects:
            obj.select_set(True)
        bpy.context.view_layer.objects.active=objects[0]
        if len(objects)>1:
            bpy.ops.object.join()
        obj=bpy.context.object;obj.name=name
        # Keep an exact base-origin pivot rather than an arbitrary component origin.
        bpy.context.scene.cursor.location=(0,0,0)
        bpy.ops.object.origin_set(type="ORIGIN_CURSOR")
        return obj

    def publish(name, separate=None, extras=None):
        nonlocal current_objects
        separate=separate or []
        static=[o for o in current_objects if o not in separate]
        objects=([merge(static,name+"_body")] if static else [])+separate
        if name=="party_balloon":
            objects[0].name="balloon_body"
            objects[0].location.z=1.1
            objects[0].keyframe_insert(data_path="location",frame=1)
            objects[0].keyframe_insert(data_path="location",frame=25)
            objects[0].animation_data.action.name="balloon_rest"
        root=bpy.data.objects.new(name,None)
        bpy.context.collection.objects.link(root)
        for obj in objects:
            obj.parent=root
        if name=="party_cake_lit":
            for i,obj in enumerate(separate):
                for frame,value in ((1,1),(7,1.07+i*.006),(13,.94),(19,1.04-i*.004),(25,1)):
                    obj.scale=(1,1,value)
                    obj.keyframe_insert(data_path="scale",frame=frame)
                obj.animation_data.action.name="candle_flicker"
        # Every exported animation is LINEAR; cubic glTF samplers are unsupported.
        for obj in objects:
            if obj.animation_data and obj.animation_data.action:
                for layer in obj.animation_data.action.layers:
                    for strip in layer.strips:
                        for bag in strip.channelbags:
                            for curve in bag.fcurves:
                                for point in curve.keyframe_points:
                                    point.interpolation="LINEAR"
        bpy.context.scene.frame_set(1)
        bpy.ops.object.select_all(action="DESELECT")
        root.select_set(True)
        for obj in objects:
            obj.select_set(True)
        bpy.context.view_layer.objects.active=root
        path=MODELS / f"{name}.glb"
        bpy.ops.export_scene.gltf(filepath=str(path),export_format="GLB",use_selection=True,
            export_yup=True,export_texcoords=True,export_normals=True,
            export_materials="EXPORT",export_image_format="AUTO",export_animations=bool(separate),
            export_animation_mode="ACTIVE_ACTIONS",export_force_sampling=True,
            export_anim_slide_to_zero=True,export_current_frame=False,
            export_cameras=False,export_lights=False,export_extras=True)
        if separate or name.startswith(("party_cloth_", "party_gift_")):
            # Preserve semantic clip names and register file-backed numeric masks
            # through the existing ordinary glTF material-extras contract. Blender
            # keeps the named mask images/properties in the editable source, but
            # does not embed disconnected numeric mask nodes in its PBR export.
            import struct
            payload=path.read_bytes()
            length=struct.unpack_from("<I",payload,12)[0]
            document=json.loads(payload[20:20+length])
            for animation in document.get("animations",[]):
                animation["name"]="balloon_rest" if name=="party_balloon" else "candle_flicker"
            blob = bytearray(payload[28+length:])
            blob = blob[:document["buffers"][0]["byteLength"]]
            mask_slots = {}
            for exported_material in document.get("materials", []):
                material_extras = exported_material.get("extras", {})
                mask_source = material_extras.pop("places_uv_mask_source", None)
                if mask_source is None:
                    continue
                if mask_source not in mask_slots:
                    mask_png = (MODELS / mask_source).read_bytes()
                    blob.extend(b"\0" * (-len(blob) % 4))
                    offset = len(blob)
                    blob.extend(mask_png)
                    view = len(document["bufferViews"])
                    document["bufferViews"].append({"buffer":0,"byteOffset":offset,"byteLength":len(mask_png)})
                    image_slot = len(document["images"])
                    document["images"].append({"bufferView":view,"mimeType":"image/png","name":Path(mask_source).stem})
                    mask_slots[mask_source] = len(document["textures"])
                    document["textures"].append({"sampler":0,"source":image_slot})
                material_extras["uv_response_mask"] = {"index":mask_slots[mask_source],"texCoord":0}
                exported_material["extras"] = material_extras
            document["buffers"][0]["byteLength"] = len(blob)
            blob.extend(b"\0" * (-len(blob) % 4))
            encoded=json.dumps(document,separators=(",",":")).encode()
            encoded+=b" "*((-len(encoded))%4)
            tail=struct.pack("<II",len(blob),0x004e4942)+blob
            path.write_bytes(struct.pack("<III",0x46546c67,2,20+len(encoded)+len(tail))+
                struct.pack("<II",len(encoded),0x4e4f534a)+encoded+tail)
        bounds=[obj.matrix_world@Vector(corner) for obj in objects for corner in obj.bound_box]
        # Convert Blender Z-up coordinates into the runtime +Y-up frame.
        low=[min(v[i] for v in bounds) for i in (0,2,1)]
        high=[max(v[i] for v in bounds) for i in (0,2,1)]
        tris=sum(sum(len(p.vertices)-2 for p in obj.data.polygons) for obj in objects)
        metadata["models"]["arcade:"+name]={
            "model":str(path.relative_to(REPO/"assets")),
            "size":[round(b-a,5) for a,b in zip(low,high)],
            "bounds_min":[round(v,5) for v in low],"bounds_max":[round(v,5) for v in high],
            "triangles":tris,"source_tool":"blender",
            "source_scene":"environments/arcade/party/birthday_party.blend",
            **(extras or {})}
        root["logical_id"]="arcade:"+name
        root["refinement_pass"]=stage
        current_objects=[]
        all_objects.append((root,objects,name))

    # Commercial table bases remain reusable independently of the thin cloth.
    box("rectangular thick commercial tabletop",(0,0,.725),(2.10,.85,.05),plastic,"cream",.012)
    for x in (-.72,.72):
        for y in (-.30,.30):
            beam("folding grey leg",(x,y,.03),(x*.88,y*.76,.69),.019)
            lathe("rubber foot",(x,y,0),[(0,.026),(.025,.026)],stock,"charcoal",6)
        beam("folding cross brace",(x,-.28,.34),(x,.28,.34),.012)
        beam("underside folding rail",(x-.13,0,.685),(x+.13,0,.685),.016)
        for y in (-.27,.27):
            beam("diagonal latch support",(x,y,.39),(x*.50,y,.69),.011)
    box("underside support channel",(0,0,.68),(1.70,.035,.035),metal,"metal")
    publish("party_table_rect",extras={"tabletop_y":.75})
    lathe("round thick party tabletop",(0,0,0),[(.70,.58),(.715,.60),(.745,.60),(.75,.58)],plastic,"cream",24)
    lathe("round pedestal support",(0,0,0),[(.045,.072),(.09,.05),(.69,.05),(.70,.14)],metal,"metal",10)
    for a in range(4):
        angle=a*math.pi/2
        beam("round cross foot",(0,0,.07),(.47*math.cos(angle),.47*math.sin(angle),.023),.026,segments=6)
        lathe("round rubber foot",(.47*math.cos(angle),.47*math.sin(angle),0),[(0,.031),(.018,.031)],stock,"charcoal",6)
    publish("party_table_round",extras={"tabletop_y":.75})

    def rectangular_cloth(pattern):
        m=material("party_cloth_"+pattern,"party_cloth_"+pattern,.49)
        nx,ny=6,4
        verts=[];uv=[];faces=[]
        # Full tabletop is flat; short hanging panels use the same world print scale.
        for j in range(ny+1):
            for i in range(nx+1):
                x=-1.07+i*2.14/nx;y=-.445+j*.89/ny
                verts.append((x,y,.25))
        for j in range(ny):
            for i in range(nx):
                a=j*(nx+1)+i
                faces.append((a,a+1,a+nx+2,a+nx+1))
                uv.append(tuple((.5+verts[k][0]/2.8,.5+verts[k][1]/2.8) for k in faces[-1]))
        edge=[i for i in range(nx+1)]+[j*(nx+1)+nx for j in range(1,ny+1)]+[ny*(nx+1)+i for i in range(nx-1,-1,-1)]+[j*(nx+1) for j in range(ny-1,0,-1)]
        lower=[]
        for k,index in enumerate(edge):
            x,y,_=verts[index]
            corner=abs(x)>1.06 and abs(y)>.44
            fold=(.026*math.sin(k*2.2) if stage!="A" else 0)
            drop=.25 if corner else .21+.022*math.sin(k*1.7)
            verts.append((x*(1.035 if corner else 1.02)+fold,y*(1.11 if corner else 1.10),.25-drop))
            lower.append(len(verts)-1)
        for k,index in enumerate(edge):
            j=(k+1)%len(edge)
            face=(index,lower[k],lower[j],edge[j])
            faces.append(face)
            coords=[]
            for index in face:
                x,y,z=verts[index];drop=.25-z
                x+=math.copysign(drop,x) if abs(x)>1.06 else 0
                y+=math.copysign(drop,y) if abs(y)>.44 else 0
                coords.append((.5+x/2.8,.5+y/2.8))
            uv.append(coords)
        obj=mesh("economical thin cloth drape",verts,faces,m,None,False,uv)
        # True two-millimetre printed plastic thickness, no reliance on ignored doubleSided.
        modifier=obj.modifiers.new("plastic sheet two millimetres","SOLIDIFY")
        modifier.thickness=.002;modifier.offset=0
        bpy.context.view_layer.objects.active=obj
        bpy.ops.object.modifier_apply(modifier=modifier.name)
        publish("party_cloth_rect_"+pattern,extras={"cloth_top_y":.25})

    def round_cloth(pattern):
        m=material("party_cloth_"+pattern,"party_cloth_"+pattern,.49)
        n=24;verts=[(0,0,.24)];faces=[];uv=[]
        for row,r in enumerate((.613,.64)):
            for i in range(n):
                a=i*math.tau/n
                z=.24 if row==0 else (.010+ .014*(1+math.sin(a*8))/2)
                verts.append((r*math.cos(a),r*math.sin(a),z))
        for i in range(n):
            j=(i+1)%n
            face=(0,i+1,j+1);faces.append(face)
            uv.append(tuple((.5+verts[k][0]/2.8,.5+verts[k][1]/2.8) for k in face))
            face=(i+1,n+i+1,n+j+1,j+1);faces.append(face)
            coords=[]
            for k in face:
                x,y,z=verts[k]
                radius=math.hypot(x,y)
                unfolded=(radius+.24-z)/radius
                coords.append((.5+x*unfolded/2.8,.5+y*unfolded/2.8))
            uv.append(coords)
        obj=mesh("round party cloth short drape",verts,faces,m,None,False,uv)
        modifier=obj.modifiers.new("plastic sheet two millimetres","SOLIDIFY")
        modifier.thickness=.002;modifier.offset=0
        bpy.context.view_layer.objects.active=obj
        bpy.ops.object.modifier_apply(modifier=modifier.name)
        minimum=min(v.co.z for v in obj.data.vertices)
        for v in obj.data.vertices:
            v.co.z-=minimum
        publish("party_cloth_round_"+pattern,extras={"cloth_top_y":round(.24-minimum,5)})

    for pattern in ("confetti","balloons","stars","arcade"):
        rectangular_cloth(pattern);round_cloth(pattern)

    for pattern in ("dots","stars","confetti","shapes","stripes"):
        source={"stars":"pale_stars","confetti":"blue_confetti","shapes":"purple_shapes"}.get(pattern,pattern)
        m=material("party_paper_"+pattern,"party_print_"+source,.91)
        lathe("printed cone paper hat",(0,0,0),[(0,.075),(.006,.074),(.217,.002),(.22,.0004)],m,None,12,printed=True)
        if stage!="A":
            beam("fine elastic strap",(-.069,0,.013),(-.050,0,.002),.0008,stock,"string",4)
            beam("fine elastic strap",(-.050,0,.002),(.050,0,.002),.0008,stock,"string",4)
            beam("fine elastic strap",(.050,0,.002),(.069,0,.013),.0008,stock,"string",4)
        publish("party_hat_"+pattern)
    for pattern in ("confetti","balloons","stars","geometric"):
        m=material("party_paper_"+pattern,"party_print_"+pattern,.87)
        # Continuous closed cup cross-section: base, tapered wall, rolled rim and inner wall.
        lathe("tapered paper cup with rolled rim",(0,0,0),[(0,.029),(.006,.031),(.113,.041),(.120,.0425),(.122,.040),(.117,.038),(.009,.027),(.006,.001)],m,None,12,printed=True)
        publish("party_cup_"+pattern)
    for pattern in ("confetti","balloons","stars","border"):
        m=material("party_plate_"+pattern,"party_print_"+pattern,.91)
        plate=lathe("shallow paper plate raised rim",(0,0,0),[(0,.074),(.002,.108),(.007,.11),(.012,.107),(.016,.103),(.006,.077),(.004,.001)],m,None,16,printed=True)
        for loop in plate.data.loops:
            co=plate.data.vertices[loop.vertex_index].co
            plate.data.uv_layers.active.data[loop.index].uv=(.5+.492*co.x/.11,.5+.492*co.y/.11)
        publish("party_plate_"+pattern)

    def bow(color):
        cx,cy=.065,.065
        if stage=="A":
            for sx in (-1,1):
                sphere("simple folded bow loop",(cx+sx*.041,cy,.324),.035,ribbon,color,scale=(1,.4,.43),segments=6,rings=3)
        else:
            # Four economical closed folded ribbon strips with visible arch and return.
            for axis in range(2):
                for sign in (-1,1):
                    points=[]
                    count=8 if stage=="C" else 7
                    for i in range(count):
                        t=i/count if stage=="C" else i/6
                        if stage=="C":
                            # A closed oval return leaves genuine open air between
                            # the folded upper/lower ribbon, visible in the game.
                            distance=sign*.0335*(1-math.cos(math.tau*t))
                            z=.317+.024*(1-math.cos(math.tau*t))+.021*math.sin(math.tau*t)
                        else:
                            distance=sign*.067*math.sin(math.pi*t)
                            z=.31+.032*math.sin(math.pi*t)+.002*t
                        if axis==0:
                            points += [(cx+distance,cy-.015,z),(cx+distance,cy+.015,z)]
                        else:
                            points += [(cx-.015,cy+distance,z),(cx+.015,cy+distance,z)]
                    faces=[(i*2,i*2+1,((i+1)%count)*2+1,((i+1)%count)*2)
                           for i in range(count if stage=="C" else count-1)]
                    o=mesh("folded open ribbon loop",points,faces,ribbon,color)
                    modifier=o.modifiers.new("ribbon folded stock","SOLIDIFY");modifier.thickness=.002
                    bpy.context.view_layer.objects.active=o;bpy.ops.object.modifier_apply(modifier=modifier.name)
            for direction in (-1,1):
                verts=[(cx-.014,cy,.306),(cx+.014,cy,.306),(cx+.021,cy+direction*.070,.309),(cx,cy+direction*.057,.309),(cx-.021,cy+direction*.070,.309)]
                o=mesh("ribbon forked tail",verts,[(0,1,2,3,4)],ribbon,color)
                mod=o.modifiers.new("ribbon tail stock","SOLIDIFY");mod.thickness=.0015
                bpy.context.view_layer.objects.active=o;bpy.ops.object.modifier_apply(modifier=mod.name)
        box("ribbon folded knot",(cx,cy,.320),(.025,.025,.017),ribbon,color,.003)

    for pattern,ribbon_color in (("stars","red"),("confetti","blue"),("stripes","green"),("arcade","pink"),("red","yellow"),("green","red")):
        m=material("party_gift_paper_"+pattern,"party_wrap_"+pattern,.86)
        gift=box("identical thirty centimetre gift cube",(0,0,.15),(.30,.30,.30),m,None)
        for sample in gift.data.uv_layers.active.data:
            sample.uv=(.25+.5*sample.uv.x,.25+.5*sample.uv.y)
        if pattern=="stripes":
            # A single horizontal coordinate field crosses every physical fold.
            # Walls retain vertical stripes and the lid carries diagonal stripes;
            # repeated corner positions sample exactly the same printed ink.
            for loop in gift.data.loops:
                vertex=gift.data.vertices[loop.vertex_index].co
                sample=gift.data.uv_layers.active.data[loop.index]
                sample.uv.x=.5+.496*(vertex.x+vertex.y)/.6
        for x in (-.151,.151):
            box("vertical ribbon on side",(x,.065,.15),(.002,.028,.30),ribbon,ribbon_color)
        for y in (-.151,.151):
            box("vertical ribbon on face",(.065,y,.15),(.028,.002,.30),ribbon,ribbon_color)
        box("ribbon band across lid x",(0,.065,.301),(.30,.028,.002),ribbon,ribbon_color)
        box("ribbon band across lid y",(.065,0,.302),(.028,.30,.002),ribbon,ribbon_color)
        if stage!="A":
            for axis in range(2):
                for sign in (-1,1):
                    center=(sign*.1503,0,.276) if axis==0 else (0,sign*.1503,.276)
                    size=(.0008,.30,.0012) if axis==0 else (.30,.0008,.0012)
                    box("subtle folded lid seam",center,size,stock,"charcoal")
        bow(ribbon_color)
        publish("party_gift_"+pattern,extras={"box_size":[.30,.30,.30],"bow_offset_xz":[.065,-.065],"stack_step_y":.30})

    def cake(lit):
        # Frosting is nonemissive matte cream; all live light belongs to candle cluster.
        # C spends its visible piping budget on rounded crests and removes four
        # inconspicuous body/board ring segments, keeping the hero below1500.
        cake_segments={"A":24,"B":20,"C":16}[stage]
        lathe("round cardboard cake board",(0,0,0),[(0,.168),(.006,.18),(.010,.172)],stock,"board",cake_segments)
        lathe("cream cylindrical cake body",(0,0,.009),[(0,.137),(.098,.150),(.108,.145)],frosting,"cream",cake_segments,True)
        lathe("soft icing top rounded edge",(0,0,.113),[(0,.139),(.006,.151),(.017,.135)],frosting,"frosting",cake_segments,True)
        if stage!="A":
            def shell(name,center,angle,width,height):
                # An eight-face icing shell keeps a scalloped crest and a soft
                # smooth normal without multiplying invisible sphere rings.
                if stage=="C":
                    local=[(-width*.35,0,height*.75),(width*.35,0,height*.75),
                           (0,0,-height*.6),(-width,0,0),(0,-width*.57,0),
                           (width,0,0),(0,width*.57,0)]
                    faces=[(0,3,4),(0,4,1),(1,4,5),(1,5,6),(1,6,0),(0,6,3),
                           (2,4,3),(2,5,4),(2,6,5),(2,3,6)]
                else:
                    local=[(0,0,height),(0,0,-height*.6),(-width,0,0),
                           (0,-width*.57,0),(width,0,0),(0,width*.57,0)]
                    faces=[(0,2,3),(0,3,4),(0,4,5),(0,5,2),
                           (1,3,2),(1,4,3),(1,5,4),(1,2,5)]
                verts=[(center[0]+x*math.cos(angle)-y*math.sin(angle),
                        center[1]+x*math.sin(angle)+y*math.cos(angle),center[2]+z) for x,y,z in local]
                return mesh(name,verts,faces,frosting,"frosting",True)
            # Individually piped shell beads and slight icing scallops replace
            # the blockout's plain cylinder while preserving its 30 cm scale.
            for row,height in ((0,.023),(1,.123)):
                for i in range(22):
                    a=i*math.tau/22
                    shell("piped frosting shell",(.147*math.cos(a),.147*math.sin(a),height),a+math.pi/2,.023,.012)
            for i in range(8):
                a=i*math.tau/8+.12
                x,y=.145*math.cos(a),.145*math.sin(a)
                shell("soft icing scallop",(x,y,.093),a,.007,.019)
            rng=random.Random(784)
            for i in range(16 if stage=="C" else 18):
                if i<12:
                    a=i*math.tau/12+.18
                    center=(.1515*math.cos(a),.1515*math.sin(a),.043+(i%3)*.020)
                    angle=a+math.pi/2
                else:
                    a=rng.uniform(0,math.tau);r=rng.uniform(.080,.125)
                    center=(r*math.cos(a),r*math.sin(a),.131)
                    angle=rng.uniform(0,math.tau)
                o=box("broad coloured sugar sprinkle",center,(.010,.003,.003),stock,("red","blue","yellow","green","purple","pink")[i%6])
                o.rotation_euler.z=angle
        flames=[]
        candle_colors=("red","blue","pink","yellow","purple","blue","red")
        for i,color in enumerate(candle_colors):
            a=i*math.tau/7+.12
            x,y=.067*math.cos(a),.067*math.sin(a)
            h=.075+(i%3)*.006
            lathe("thin coloured birthday candle",(x,y,.128),[(0,.0033),(h,.0033)],stock,color,6)
            if stage!="A":
                # Economical continuous spiral ribbon illustrates striped wax without emission.
                verts=[];faces=[]
                for j in range(11):
                    angle=j*math.tau/4
                    z=.132+h*j/11
                    verts += [(x+.00345*math.cos(angle),y+.00345*math.sin(angle),z),
                              (x+.00345*math.cos(angle),y+.00345*math.sin(angle),z+.0022)]
                for j in range(10):
                    faces.append((j*2,j*2+1,j*2+3,j*2+2))
                mesh("white stripe on candle wax",verts,faces,stock,"wax")
            beam("tiny candle wick",(x,y,.128+h),(x,y,.131+h),.00065,stock,"charcoal",4)
            if lit:
                obj=lathe(f"candle_flame_{i:02}",(0,0,0),[(0,.0003),(.005,.0032),(.018,.0002)],flame,"yellow",4,True)
                obj.location=(x,y,.131+h)
                flames.append(obj)
        publish("party_cake_lit" if lit else "party_cake_unlit",separate=flames,
                extras={"cake_body_diameter":.30,"cake_top_y":.130,"flame_cluster_offset":[0,.218,0],
                        "candle_count":7,"cake_emissive":False,"clip":"candle_flicker" if lit else None})
    cake(True);cake(False)

    for color in ("purple","pink","yellow","blue","orange"):
        verts=[];faces=[];n=20 if stage!="A" else 12
        for i in range(n+1):
            t=i/n;a=t*math.tau*2.15
            x=.020*math.sin(a);y=.013*math.cos(a);z=.70*(1-t)
            tilt=a*.60
            dx=.023*math.cos(tilt);dy=.023*math.sin(tilt)
            verts += [(x-dx,y-dy,z),(x+dx,y+dy,z)]
        for i in range(n):
            faces.append((i*2,i*2+1,i*2+3,i*2+2))
        o=mesh("twisted lightweight crepe paper streamer",verts,faces,stock,color)
        mod=o.modifiers.new("paper thickness","SOLIDIFY");mod.thickness=.0008
        bpy.context.view_layer.objects.active=o;bpy.ops.object.modifier_apply(modifier=mod.name)
        minimum=min(v.co.z for v in o.data.vertices)
        for v in o.data.vertices:v.co.z-=minimum
        publish("party_streamer_"+color)

    # Balloon neck is narrowed to a genuine tied knot; shared runtime rig poses strings.
    # A broad inflated shoulder and one economical cap ring avoid the former
    # pointed top transition; authored smooth normals remain the live light frame.
    profile=[(.018,.004),(.040,.006),(.067,.031),(.103,.073),(.151,.120),
             (.211,.147),(.275,.150),(.338,.132),(.388,.088),(.417,.038),(.430,.004)]
    lathe("shared latex teardrop balloon",(0,0,0),profile,latex,"latex",16,True)
    lathe("tied latex knot",(0,0,0),[(0,.010),(.013,.004),(.019,.005)],latex,"latex",6)
    body=merge(current_objects,"balloon_body")
    current_objects=[body]
    strings=[]
    for i in range(12):
        string_radius=.0017 if stage=="C" else .001
        obj=lathe(f"balloon_string_{i:02}",(0,0,0),[(0,string_radius),(1,string_radius)],stock,"string",4)
        obj.scale.z=1.1/12;obj.location.z=i*1.1/12
        strings.append(obj)
    publish("party_balloon",separate=strings,
            extras={"body_size":[.30,.43,.30],"body_bind_translation":[0,1.1,0],
                    "string_segment_count":12,"string_radius":string_radius,"clip":"balloon_rest"})

    if stage=="C":
        lathe("molded charcoal balloon weight puck",(0,0,0),
              [(0,.032),(.003,.035),(.036,.035),(.040,.031)],stock,"charcoal",12)
        bpy.ops.mesh.primitive_torus_add(major_segments=10,minor_segments=4,
            major_radius=.006,minor_radius=.0018,location=(0,0,.047),rotation=(math.pi/2,0,0))
        hoop=bpy.context.object;hoop.name="molded balloon string tie loop"
        track(hoop,stock,"charcoal",True)
        bpy.context.view_layer.update()
        tie_height=round(max((hoop.matrix_world@Vector(c)).z for c in hoop.bound_box),5)
        publish("party_balloon_weight",extras={"puck_body_size":[.07,.04,.07],
                    "tabletop_anchor_offset":[0,tie_height,0],"anchor_height":tie_height})

    # Arrange editable meshes into a modest native kit inspection scene.
    for i,(root,objects,name) in enumerate(all_objects):
        if name.startswith("party_table"):
            loc=(-3+i*3.0,0,0)
        elif name.startswith("party_cloth"):
            ci=int(i-2);loc=(-4+(ci%4)*2.7,3.0+(ci//4)*1.8,0)
        elif name.startswith("party_gift"):
            gi=sum(1 for r,o,n in all_objects[:i] if n.startswith("party_gift"));loc=(-1.5+gi*.62,-1.6,0)
        elif name.startswith("party_cake"):
            loc=(-.55 if name.endswith("lit") and not name.endswith("unlit") else .55,-2.9,0)
        elif name.startswith("party_streamer"):
            si=sum(1 for r,o,n in all_objects[:i] if n.startswith("party_streamer"));loc=(3.5+si*.18,-1.5,.6)
        elif name=="party_balloon":
            loc=(4.7,-2.5,0)
        else:
            pi=sum(1 for r,o,n in all_objects[:i] if n.startswith(("party_hat","party_cup","party_plate")))
            loc=(-3.5+(pi%7)*.48,-2.2-(pi//7)*.55,0)
        root.location=loc
    bpy.context.scene.render.engine="BLENDER_EEVEE"
    bpy.context.scene.render.resolution_x=1600
    bpy.context.scene.render.resolution_y=1100
    bpy.context.scene.render.resolution_percentage=100
    bpy.context.scene.world.color=(.06,.06,.06)
    bpy.ops.mesh.primitive_plane_add(size=200,location=(0,0,-.015))
    ground=bpy.context.object;ground.name="preview studio floor"
    floor=bpy.data.materials.new("preview floor");floor.use_nodes=True
    floor.node_tree.nodes.get("Principled BSDF").inputs["Base Color"].default_value=(.075,.07,.09,1)
    ground.data.materials.append(floor)
    for loc,power,size in (((-3,-5,8),450,7),((5,1,7),300,6),((-5,5,4),250,5)):
        bpy.ops.object.light_add(type="AREA",location=loc)
        obj=bpy.context.object;obj.data.energy=power;obj.data.shape="DISK";obj.data.size=size
        obj.rotation_euler=(Vector((0,0,.4))-obj.location).to_track_quat("-Z","Y").to_euler()
    bpy.ops.object.camera_add(location=(9,-13,12))
    camera=bpy.context.object
    camera.rotation_euler=(Vector((.5,1,.3))-camera.location).to_track_quat("-Z","Y").to_euler()
    camera.data.type="ORTHO";camera.data.ortho_scale=15
    bpy.context.scene.camera=camera
    bpy.context.scene.view_settings.view_transform="Standard"
    bpy.context.scene.render.fps=24
    bpy.context.scene.frame_end=25
    # Useful opening state in desktop Blender: rendered materials, entire authored kit.
    for screen in bpy.data.screens:
        for area in screen.areas:
            if area.type=="VIEW_3D":
                area.spaces.active.shading.type="MATERIAL"
                area.spaces.active.region_3d.view_distance=13
                area.spaces.active.region_3d.view_location=(0,0,.5)
    bpy.context.preferences.filepaths.save_version=0
    for bitmap in images.values():
        bitmap.filepath="//models/"+Path(bitmap.filepath).name
    bpy.ops.wm.save_as_mainfile(filepath=str(ROOT / "birthday_party.blend"),check_existing=False,compress=False)
    (ROOT/"asset_metadata.json").write_text(json.dumps(metadata,indent=2)+"\n")
    if preview:
        p=Path(preview);p.parent.mkdir(parents=True,exist_ok=True)
        bpy.context.scene.render.filepath=str(p)
        bpy.ops.render.render(write_still=True)
        # One restrained close view makes the cake refinement reviewable; this is
        # native Blender output, while the game supplies final lighting acceptance.
        camera.location=(.10,-3.85,.70)
        camera.rotation_euler=(Vector((-.55,-2.9,.10))-camera.location).to_track_quat("-Z","Y").to_euler()
        camera.data.ortho_scale=.68
        bpy.context.scene.render.resolution_x=960
        bpy.context.scene.render.resolution_y=700
        bpy.context.scene.render.filepath=str(p.with_name(p.stem+"-cake.png"))
        bpy.ops.render.render(write_still=True)
        for root,objects,name in all_objects:
            for obj in objects:
                obj.hide_render=not name.startswith("party_gift")
        camera.location=(2.0,-4.8,1.7)
        camera.rotation_euler=(Vector((.05,-1.6,.17))-camera.location).to_track_quat("-Z","Y").to_euler()
        camera.data.ortho_scale=3.95
        bpy.context.scene.render.resolution_x=1200
        bpy.context.scene.render.resolution_y=650
        bpy.context.scene.render.filepath=str(p.with_name(p.stem+"-gifts.png"))
        bpy.ops.render.render(write_still=True)
        for root,objects,name in all_objects:
            for obj in objects:
                obj.hide_render=name!="party_balloon"
        camera.location=(5.40,-3.50,1.75)
        camera.rotation_euler=(Vector((4.7,-2.5,1.31))-camera.location).to_track_quat("-Z","Y").to_euler()
        camera.data.ortho_scale=.62
        bpy.context.scene.render.resolution_x=800
        bpy.context.scene.render.resolution_y=700
        bpy.context.scene.render.filepath=str(p.with_name(p.stem+"-balloon.png"))
        bpy.ops.render.render(write_still=True)
        for root,objects,name in all_objects:
            for obj in objects:
                obj.hide_render=name!="party_cloth_rect_confetti"
        camera.location=(-2.10,.90,1.90)
        camera.rotation_euler=(Vector((-4,3,.18))-camera.location).to_track_quat("-Z","Y").to_euler()
        camera.data.ortho_scale=2.65
        bpy.context.scene.render.resolution_x=1000
        bpy.context.scene.render.resolution_y=700
        bpy.context.scene.render.filepath=str(p.with_name(p.stem+"-cloth.png"))
        bpy.ops.render.render(write_still=True)
    print(f"PARTY_AUTHOR_COMPLETE pass={stage} models={len(metadata['models'])}")


def check_assets():
    """Bounded source-only checks; no Blender invocation or production map bake."""
    import io
    from PIL import Image
    sys.path.insert(0,str(REPO/"tools/props"))
    import glb
    metadata=json.loads((ROOT/"asset_metadata.json").read_text())
    native_pixels={}
    for path in MODELS.glob("*.png"):
        if path.stem.endswith("_master"):
            continue
        with Image.open(path) as image:
            ceiling=(1024 if path.stem.startswith("party_cloth_") else
                     512 if path.stem.startswith("party_wrap_") else 256)
            if image.mode!="RGB" or max(image.size)>ceiling:
                raise ValueError(f"{path.name}: fitted native must be opaque RGB within{ceiling}")
            native_pixels[(image.size,image.tobytes())]=path
            master_name=path.stem
            for prefix in ("party_cloth_", "party_wrap_"):
                if master_name.startswith(prefix):
                    master_name=master_name.replace(prefix,"party_print_",1)
                    break
            master=path.with_name(master_name+"_master.png")
            with Image.open(master) as source:
                expected=source.resize(image.size,Image.Resampling.LANCZOS)
                if expected.tobytes()!=image.tobytes():
                    raise ValueError(f"{path.name}: native differs from master derivative")
            if path.stem.startswith("party_ink_"):
                r,g,b=image.split()
                if r.tobytes()!=g.tobytes() or r.tobytes()!=b.tobytes():
                    raise ValueError(f"{path.name}: pigment mask must be numeric greyscale RGB")
    decoded=0;triangles=0;export_bytes=0
    for logical,entry in metadata["models"].items():
        path=REPO/"assets"/entry["model"]
        payload=path.read_bytes();model=glb.read_glb(payload)
        if model.triangle_count>1500:
            raise ValueError(f"{logical}: {model.triangle_count} triangles exceeds art ceiling")
        if any(not math.isfinite(c) or not -.01<=c<=1.01 for uv in model.uvs for c in uv):
            raise ValueError(f"{logical}: invalid fitted UV")
        low,high=model.bounds()
        if abs(low[1])>.012 or any(abs((low[a]+high[a])/2)>.02 for a in (0,2)):
            raise ValueError(f"{logical}: origin or horizontal centering mismatch")
        if any(abs(high[a]-low[a]-entry["size"][a])>.0001 for a in range(3)):
            raise ValueError(f"{logical}: metadata size differs from actual mesh bounds")
        for i in range(0,len(model.indices),3):
            a,b,c=[model.positions[k] for k in model.indices[i:i+3]]
            e=[b[j]-a[j] for j in range(3)];f=[c[j]-a[j] for j in range(3)]
            cross=(e[1]*f[2]-e[2]*f[1],e[2]*f[0]-e[0]*f[2],e[0]*f[1]-e[1]*f[0])
            if sum(v*v for v in cross)<1e-24:
                raise ValueError(f"{logical}: zero-area triangle")
        document=model.json
        if set(document.get("extensionsUsed",[]))-{"KHR_materials_emissive_strength"}:
            raise ValueError(f"{logical}: unsupported glTF material extension")
        for image in model.texture_pngs:
            with Image.open(io.BytesIO(image)) as bitmap:
                if (bitmap.size,bitmap.convert("RGB").tobytes()) not in native_pixels:
                    raise ValueError(f"{logical}: embedded PNG lacks equivalent committed source")
                decoded+=bitmap.width*bitmap.height*4
        if logical.endswith("party_balloon"):
            names={n.get("name") for n in document["nodes"]}
            expected={"balloon_body",*(f"balloon_string_{i:02}" for i in range(12))}
            if not expected<=names:
                raise ValueError("balloon rig node missing")
        if logical.endswith("party_gift_stripes"):
            # The observed fold bug was independent face projection. Compare
            # exported shared corners directly, without reusing the author formula.
            slot=model.material_names.index("party_gift_paper_stripes")
            paper_indices={index for triangle,material in enumerate(model.triangle_materials)
                           if material==slot for index in model.indices[triangle*3:triangle*3+3]}
            if len(paper_indices)!=24:
                raise ValueError("striped gift must retain six four-corner paper faces")
            corners={}
            for index in paper_indices:
                position=tuple(round(value,6) for value in model.positions[index])
                u=model.uvs[index][0]
                if position in corners and abs(u-corners[position])>1e-6:
                    raise ValueError("striped gift print U breaks at a shared physical corner")
                corners[position]=u
            if len(corners)!=8:
                raise ValueError("striped gift paper must retain eight shared cube corners")
        if "party_cloth_round_" in logical:
            # The observed bug projected the 23cm vertical skirt onto a 2.7cm
            # planar band. Measure actual exported radial edges: their print
            # density must remain comparable to the tabletop, not stretch 8x.
            edges=set()
            for i in range(0,len(model.indices),3):
                triangle=model.indices[i:i+3]
                edges.update(tuple(sorted((a,b))) for a,b in zip(triangle,triangle[1:]+triangle[:1]))
            ratios=[]
            for a,b in edges:
                p,q=model.positions[a],model.positions[b]
                if abs(p[1]-q[1])<.18:
                    continue
                radius_product=math.hypot(p[0],p[2])*math.hypot(q[0],q[2])
                if abs(p[0]*q[2]-p[2]*q[0])/max(radius_product,1e-8)>.01:
                    continue
                ratios.append(math.dist(model.uvs[a],model.uvs[b])*2.8/math.dist(p,q))
            if len(ratios)<24 or any(not .8<=ratio<=1.3 for ratio in ratios):
                raise ValueError(f"{logical}: round skirt print stretches relative to world scale")
        if "party_cloth_" in logical or "party_gift_" in logical:
            paper=[material for material in document["materials"]
                   if material.get("name","").startswith(("party_cloth_","party_gift_paper_"))]
            if len(paper)!=1:
                raise ValueError(f"{logical}: expected one printed reactive material")
            for material in document["materials"]:
                extras=material.get("extras",{})
                if material not in paper:
                    if extras.get("uv_response",0)>0:
                        raise ValueError(f"{logical}: backing/ribbon material unexpectedly reactive")
                    continue
                if extras.get("uv_response")!=.65 or extras.get("uv_response_mask",{}).get("texCoord")!=0:
                    raise ValueError(f"{logical}: registered ink response missing")
                texture_index=extras["uv_response_mask"]["index"]
                with Image.open(io.BytesIO(model.texture_pngs[texture_index])) as mask:
                    if mask.size!=(256,256):
                        raise ValueError(f"{logical}: expected bounded256 pigment mask")
                if any(material.get("emissiveFactor",[0,0,0])):
                    raise ValueError(f"{logical}: printed paper must remain non-emissive")
        if entry.get("clip"):
            if entry["clip"] not in {a["name"] for a in document.get("animations",[])}:
                raise ValueError(f"{logical}: declared animation clip missing")
        triangles+=model.triangle_count;export_bytes+=len(payload)
    source=ROOT/"birthday_party.blend"
    if not source.read_bytes().startswith(b"BLENDER"):
        raise ValueError("editable native source missing")
    print(f"PARTY_CHECK_OK models={len(metadata['models'])} triangles={triangles} "
          f"glb_bytes={export_bytes} decoded_rgba_bytes={decoded}")


def main():
    argv=sys.argv[sys.argv.index("--")+1:] if "--" in sys.argv else sys.argv[1:]
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("--textures",action="store_true")
    p.add_argument("--check",action="store_true")
    p.add_argument("--pass",dest="stage",choices=("A","B","C"),default="A")
    p.add_argument("--preview")
    args=p.parse_args(argv)
    reviews = json.loads((REPO / 'assets/completed-models.json').read_text())
    adopted = {path: record for path, record in reviews.items()
               if path.startswith('environments/arcade/party/models/')}
    if adopted:
        import hashlib
        for path, record in adopted.items():
            if hashlib.sha256((REPO / 'assets' / path).read_bytes()).hexdigest() != record['sha256']:
                raise ValueError(f'{path}: completed export differs from accepted bytes')
        print(f'PARTY_COMPLETED_EXPORTS_OK models={len(adopted)}; original authoring scene is historical')
        if not args.check:
            raise SystemExit('Use the delivered editable masters for completed assets; the original writer cannot replace them')
    elif args.check:
        check_assets()
    elif args.textures:
        paint_textures()
    else:
        author_blender(args.stage,args.preview)


if __name__=="__main__":
    main()
