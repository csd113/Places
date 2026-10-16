#!/usr/bin/env python3
"""Audit the remaining static props, or apply reviewed geometry-only repairs.

No builder or texture painter is run. Existing PNGs, material definitions, UVs,
vertex colors, transforms and catalog entries are retained. Added closure faces
reuse adjacent atlas regions. Use --apply explicitly to write GLBs.
"""
import argparse
import hashlib
import json
import math
from pathlib import Path
import struct
import sys
from dataclasses import replace
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from execution import ordered_map, worker_count, atomic_write

import glb
from geometry import boundary_loops, inspect, _cross, _sub, _dot, glb_triangles, overlap_report, clip_attribute_triangle, transformed_triangles

ROOT = Path(__file__).resolve().parents[2]
NAMES = set('armchair bed bookshelf cardboard_box couch crate fridge lamp plant rug sink stove table tv washer_drum washing_machine cabinet_base cabinet_wall cabinet chair desk vending_machine water_cooler pool_chair pool_curtain_corner pool_curtain_end pool_curtain_straight pool_guardrail_corner pool_guardrail_end pool_guardrail_straight pool_ladder pool_table'.split())
# Only these reviewed boundary types are closed; basin mouths, cloth edges and
# independent textured face planes must never be filled by a generic hole pass.
CLOSE_ALL = {'bed', 'water_cooler', 'pool_chair', 'pool_table', 'pool_ladder',
             'pool_curtain_corner', 'pool_curtain_end', 'pool_curtain_straight', 'vending_machine'}

RECEPTION_ORIGINAL_SHA256 = "aba1a7685f98c67bf405829a4af220af5c68d90b6ef4a9b21eb417574770274d"


def canonical_repair_document(raw):
    """Reject layouts our JSON/BIN-only writer cannot preserve exactly.

    Read-only GLB inspection accepts unknown chunks. A repair must not silently
    drop one, mistake its header for vertex bytes, or consume trailing data.
    These mutators support exactly one JSON chunk followed by one BIN chunk.
    """
    if len(raw) < 28:
        raise ValueError("noncanonical GLB repair container: missing JSON/BIN chunks")
    magic, version, total = struct.unpack_from("<III", raw)
    if magic != glb.GLB_MAGIC or version != 2 or total != len(raw):
        raise ValueError("noncanonical GLB repair container: invalid header or trailing bytes")
    json_length, json_kind = struct.unpack_from("<II", raw, 12)
    bin_header = 20 + json_length
    if json_kind != glb.CHUNK_JSON or json_length % 4 or bin_header + 8 > total:
        raise ValueError("noncanonical GLB repair container: expected aligned JSON first")
    bin_length, bin_kind = struct.unpack_from("<II", raw, bin_header)
    if bin_kind != glb.CHUNK_BIN or bin_length % 4 or bin_header + 8 + bin_length != total:
        raise ValueError("noncanonical GLB repair container: expected one BIN and no unknown chunks")
    return json.loads(raw[20:bin_header]), bytearray(raw[bin_header + 8:total])


def reception_repair_plan(path):
    """The reviewed material precedence, addressed to one delivered export."""
    path = Path(path).resolve()
    raw = path.read_bytes()
    if hashlib.sha256(raw).hexdigest() != RECEPTION_ORIGINAL_SHA256:
        raise ValueError("reception desk differs from the reviewed original export")
    return {"format": "places-reviewed-model-clip", "version": 1,
            "source": str(path), "sha256": RECEPTION_ORIGINAL_SHA256,
            "material_precedence": {"keep": {"primitive": 3, "material": "Office_paint_edge"},
                                    "clip": {"primitive": 0, "material": "Office_wood"}},
            "edits": [{"mesh": 0, "primitive": 0, "triangles": list(range(8, 20)),
                       "axis": 1, "minimum": 0.9480000138282776,
                       "reason": "Remove wood only where the existing opaque paint edge owns the perimeter side; retain its original top and affine UVs."}]}


def repair_reception_desk(raw, plan):
    """Clip the reviewed wood side band under the existing 9 mm paint edging.

    No surface is moved. Trim geometry, all original attribute buffer bytes,
    other primitives, texture bytes, material dictionaries and bounds survive.
    """
    if hashlib.sha256(raw).hexdigest() != RECEPTION_ORIGINAL_SHA256:
        raise ValueError("reception source sha256 changed; refusing stale plan")
    if (plan.get("format") != "places-reviewed-model-clip" or plan.get("version") != 1
            or plan.get("sha256") != RECEPTION_ORIGINAL_SHA256
            or plan.get("material_precedence") != {"keep": {"primitive": 3, "material": "Office_paint_edge"}, "clip": {"primitive": 0, "material": "Office_wood"}}
            or len(plan.get("edits", [])) != 1):
        raise ValueError("unrecognized reviewed reception plan")
    edit = plan["edits"][0]
    if any(edit.get(key) != value for key, value in {"mesh": 0, "primitive": 0, "triangles": list(range(8, 20)), "axis": 1, "minimum": 0.9480000138282776}.items()):
        raise ValueError("reviewed reception clipping plane or source faces changed")
    document, blob = canonical_repair_document(raw)
    primitive = document["meshes"][0]["primitives"][0]
    if document["materials"][primitive["material"]]["name"] != "Office_wood" or document["materials"][3]["name"] != "Office_paint_edge":
        raise ValueError("reviewed material slots changed")
    attributes = {name: list(glb._read_accessor(document, blob, accessor)) for name, accessor in primitive["attributes"].items()}
    if set(attributes) != {"POSITION", "NORMAL", "TEXCOORD_0"}:
        raise ValueError("unreviewed reception vertex attributes")
    indices = [int(row[0]) for row in glb._read_accessor(document, blob, primitive["indices"])]
    retained = []
    for start in range(0, len(indices), 3):
        corners = indices[start:start + 3]
        if start // 3 not in edit["triangles"]:
            retained.extend(corners)
            continue
        records = [tuple(sorted((name, tuple(values[i])) for name, values in attributes.items())) for i in corners]
        for triangle in clip_attribute_triangle(records, axis=edit["axis"], minimum=edit["minimum"]):
            for vertex in triangle:
                retained.append(len(attributes["POSITION"]))
                for name, value in vertex:
                    attributes[name].append(value)
    def append_accessor(rows, *, component, kind, fmt, target):
        while len(blob) % 4:
            blob.append(0)
        start = len(blob)
        for row in rows:
            blob.extend(struct.pack("<" + fmt, *row))
        view = len(document["bufferViews"])
        document["bufferViews"].append({"buffer": 0, "byteOffset": start, "byteLength": len(blob) - start, "target": target})
        accessor = {"bufferView": view, "componentType": component, "count": len(rows), "type": kind}
        if kind == "VEC3":
            accessor["min"] = [min(row[i] for row in rows) for i in range(3)]
            accessor["max"] = [max(row[i] for row in rows) for i in range(3)]
        index = len(document["accessors"])
        document["accessors"].append(accessor)
        return index
    for name, values in attributes.items():
        count = len(values[0])
        primitive["attributes"][name] = append_accessor(values, component=5126, kind=f"VEC{count}", fmt=f"{count}f", target=34962)
    primitive["indices"] = append_accessor([(i,) for i in retained], component=5125, kind="SCALAR", fmt="I", target=34963)
    document["buffers"][0]["byteLength"] = len(blob)
    payload = json.dumps(document, separators=(",", ":")).encode()
    payload += b" " * (-len(payload) % 4)
    blob.extend(b"\0" * (-len(blob) % 4))
    output = (struct.pack("<III", glb.GLB_MAGIC, 2, 28 + len(payload) + len(blob))
              + struct.pack("<II", len(payload), glb.CHUNK_JSON) + payload
              + struct.pack("<II", len(blob), glb.CHUNK_BIN) + blob)
    before, after = glb.read_glb(raw), glb.read_glb(output)
    if before.bounds() != after.bounds() or before.texture_pngs != after.texture_pngs or before.json["materials"] != after.json["materials"]:
        raise ValueError("reviewed reception contract changed")
    post = overlap_report(glb_triangles(output, source="reception"), min_area_m2=1e-8)
    if not post["complete"] or any(row["kind"] == "coplanar-overlap" and set(row["materials"]) == {"reception/materials/0", "reception/materials/3"} for row in post["findings"]):
        raise ValueError("reviewed wood/paint side overlap remains")
    return output


def cloth_triangle(name, positions, tri):
    if not name.startswith('pool_curtain_'):
        return False
    ys = [positions[i][1] for i in tri]
    return all(any(abs(y-h)<1e-5 for h in (0.06, 2.23, 2.49)) for y in ys) and max(ys)-min(ys)>0.2


def cavity_triangle(name, positions, tri):
    return name == 'washing_machine' and all(
        abs(positions[i][0]) <= .215001 and abs(positions[i][1]-.44) <= .215001
        and -.020001 <= positions[i][2] <= .300001 for i in tri)


def orient(name, positions, indices):
    original = list(indices)
    offsets = [i for i in range(0, len(indices), 3)
               if not cloth_triangle(name, positions, indices[i:i+3])
               and not cavity_triangle(name, positions, indices[i:i+3])]
    solid = [v for i in offsets for v in indices[i:i+3]]
    result = inspect(positions, solid, repair=True)
    for i, offset in enumerate(offsets):
        indices[offset:offset+3] = solid[i*3:i*3+3]
    # A recessed cavity is the inside of a vessel, not a standalone positive-
    # volume solid. Anchor its front ring/back plate to +Z, its outer bezel
    # outward and its inner barrel toward the axis; never seal the mouth.
    for offset in range(0,len(indices),3):
        tri=indices[offset:offset+3]
        if not cavity_triangle(name,positions,tri):
            continue
        a,b,c=[positions[i] for i in tri]
        normal=_cross(_sub(b,a),_sub(c,a))
        if max(a[2],b[2],c[2])-min(a[2],b[2],c[2])<1e-6:
            facing=normal[2]
        else:
            x=(a[0]+b[0]+c[0])/3; y=(a[1]+b[1]+c[1])/3-.44
            outer=max(math.hypot(p[0],p[1]-.44) for p in (a,b,c))>.18
            facing=(normal[0]*x+normal[1]*y)*(1 if outer else -1)
        if facing<0:
            indices[offset+1],indices[offset+2]=indices[offset+2],indices[offset+1]
    result['flipped_triangles']=sum(indices[i:i+3]!=original[i:i+3] for i in range(0,len(indices),3))
    return result


def should_close(name, points):
    lo = [min(p[a] for p in points) for a in range(3)]
    hi = [max(p[a] for p in points) for a in range(3)]
    flat = lambda axis, value: abs(lo[axis]-value)<1e-5 and abs(hi[axis]-value)<1e-5
    if name == 'pool_table' and flat(1, .728):
        # This is the tray's independent top sheet, not a missing box bottom.
        # Capping its boundary creates an opposite face on the same plane.
        return False
    if name in CLOSE_ALL:
        return True
    if name == 'washer_drum':
        return flat(1, 0.0)
    if name == 'tv':
        return flat(1, 0.1)
    if name == 'sink':
        # Keep the carcass open under the basin; close doors and faucet ends.
        return not flat(1, 0.872)
    if name == 'stove':
        # The cooking surface is a separate textured plane, not a hole.
        return not flat(1, 0.9)
    if name == 'washing_machine':
        return True
    return False


def remove_pool_table_cap(positions, indices):
    """Remove only the reviewed zero-thickness closure over the tray surface."""
    top = []
    bottom = []
    for offset in range(0, len(indices), 3):
        points = [positions[i] for i in indices[offset:offset+3]]
        if not all(abs(p[1]-.728)<1e-5 and abs(p[0])<.396 and abs(p[2])<.396 for p in points):
            continue
        normal = _cross(_sub(points[1], points[0]), _sub(points[2], points[0]))
        if abs(normal[1]) > .6:
            (top if normal[1] > 0 else bottom).append(offset)
    if not bottom:
        return 0
    keys = lambda offsets: {tuple(round(v, 6) for v in positions[i])
                            for offset in offsets for i in indices[offset:offset+3]}
    if len(top) != 2 or len(bottom) != 2 or keys(top) != keys(bottom):
        raise ValueError('pool_table: tray closure differs from the reviewed quad')
    removed = set(bottom)
    indices[:] = [i for offset in range(0, len(indices), 3) if offset not in removed
                  for i in indices[offset:offset+3]]
    return len(bottom)


def cap_loop(positions, uvs, colors, indices, loop, neighbors):
    points = [positions[i] for i in loop]
    # Convex planar rings use a triangle fan; L-shaped missing box faces use a
    # fan rooted on their shared corner. Reject non-planar diagonal bridges.
    lo = [min(p[a] for p in points) for a in range(3)]
    hi = [max(p[a] for p in points) for a in range(3)]
    flat_axes = [a for a in range(3) if hi[a]-lo[a]<1e-6]
    selected = None
    edge_keys = {tuple(sorted((tuple(positions[a]), tuple(positions[b]))))
                 for offset in range(0, len(indices), 3)
                 for a, b in zip(indices[offset:offset+3], indices[offset+1:offset+3]+indices[offset:offset+1])}
    boundary_keys = {tuple(sorted((tuple(positions[a]), tuple(positions[b]))))
                     for a, b in zip(loop, loop[1:]+loop[:1])}
    for root in range(len(loop)):
        order = loop[root:]+loop[:root]
        faces=[]; valid=True
        for k in range(1,len(order)-1):
            tri=[order[0],order[k],order[k+1]]
            a,b,c=[positions[i] for i in tri]
            normal=_cross(_sub(b,a),_sub(c,a))
            if _dot(normal,normal)<1e-20:
                continue
            if not flat_axes and sum(abs(n)>1e-8 for n in normal)!=1:
                valid=False;break
            faces.append(tri)
        if valid and faces:
            diagonals = {tuple(sorted((tuple(positions[a]), tuple(positions[b]))))
                         for tri in faces for a,b in zip(tri,tri[1:]+tri[:1])} - boundary_keys
            if diagonals & edge_keys:
                continue
            selected=faces;break
    if selected is None:
        return 0
    neighbor_vertices={v for i in neighbors for v in indices[i:i+3]}
    uv_lo=[min(uvs[v][a] for v in neighbor_vertices) for a in range(2)]
    uv_hi=[max(uvs[v][a] for v in neighbor_vertices) for a in range(2)]
    tint=tuple(sum(colors[v][a] for v in neighbor_vertices)/len(neighbor_vertices) for a in range(4))
    # Separate seam vertices for each planar cap triangle retain hard edges and
    # never modify UVs on an existing surface.
    for tri in selected:
        pts=[positions[v] for v in tri]
        normal=_cross(_sub(pts[1],pts[0]),_sub(pts[2],pts[0]))
        omit=max(range(3),key=lambda a:abs(normal[a]))
        axes=[a for a in range(3) if a!=omit]
        for p in pts:
            indices.append(len(positions));positions.append(p)
            uvs.append(tuple(uv_lo[i]+(uv_hi[i]-uv_lo[i])*(p[a]-lo[a])/max(hi[a]-lo[a],1e-9) for i,a in enumerate(axes)))
            colors.append(tint)
    return len(selected)


def close_box_faces(positions, uvs, colors, indices, center, size, faces):
    """Close reviewed omitted box faces whose touching edges obscure hole loops."""
    corners = {}
    for sx in (-1,1):
        for sy in (-1,1):
            for sz in (-1,1):
                key=(sx,sy,sz)
                point=tuple(center[a]+key[a]*size[a]/2 for a in range(3))
                matches=[i for i,p in enumerate(positions) if all(abs(p[a]-point[a])<1e-5 for a in range(3))]
                if not matches:
                    return 0
                corners[key]=matches[0]
    added=0
    orders={'-y':[(-1,-1,-1),(1,-1,-1),(1,-1,1),(-1,-1,1)],
            '-z':[(1,-1,-1),(-1,-1,-1),(-1,1,-1),(1,1,-1)]}
    for face in faces:
        loop=[corners[k] for k in orders[face]]
        keys={tuple(round(v,5) for v in positions[i]) for i in loop}
        if any(all(tuple(round(v,5) for v in positions[i]) in keys for i in indices[n:n+3]) for n in range(0,len(indices),3)):
            continue
        boxkeys={tuple(round(v,5) for v in positions[i]) for i in corners.values()}
        neighbors=[n for n in range(0,len(indices),3) if all(tuple(round(v,5) for v in positions[i]) in boxkeys for i in indices[n:n+3])]
        added+=cap_loop(positions,uvs,colors,indices,loop,neighbors)
    return added


def compact(positions, uvs, colors, indices):
    """Weld only identical full vertex records; retain UV and shading seams."""
    records={}; out=[[],[],[]]; remap=[]
    for i in indices:
        key=(tuple(positions[i]),tuple(uvs[i]),tuple(colors[i]))
        if key not in records:
            records[key]=len(out[0])
            for array,value in zip(out,key):array.append(value)
        remap.append(records[key])
    return *out, remap


def encode(document, blob, positions, uvs, colors, indices):
    primitive=document['meshes'][0]['primitives'][0]
    values=[('POSITION',positions,'3f',5126,'VEC3'),('TEXCOORD_0',uvs,'2f',5126,'VEC2'),
            ('COLOR_0',[[round(max(0,min(1,c))*255) for c in row] for row in colors],'4B',5121,'VEC4')]
    def replace(accessor, data, fmt, component, kind, target):
        while len(blob)%4:blob.append(0)
        start=len(blob)
        for row in data:blob.extend(struct.pack('<'+fmt,*row))
        view=len(document['bufferViews'])
        document['bufferViews'].append({'buffer':0,'byteOffset':start,'byteLength':len(blob)-start,'target':target})
        definition={'bufferView':view,'componentType':component,'count':len(data),'type':kind}
        if kind=='VEC4':definition['normalized']=True
        if kind=='VEC3':
            definition['min']=[min(p[a] for p in data) for a in range(3)]
            definition['max']=[max(p[a] for p in data) for a in range(3)]
        document['accessors'][accessor]=definition
    for semantic,data,fmt,component,kind in values:
        replace(primitive['attributes'][semantic],data,fmt,component,kind,34962)
    replace(primitive['indices'],[(i,) for i in indices],'H',5123,'SCALAR',34963)
    # Drop now-unreferenced accessors before packing, too.
    used_accessors = sorted(set(primitive['attributes'].values()) | {primitive['indices']})
    accessor_map = {old:new for new,old in enumerate(used_accessors)}
    document['accessors'] = [document['accessors'][i] for i in used_accessors]
    primitive['attributes'] = {name:accessor_map[i] for name,i in primitive['attributes'].items()}
    primitive['indices'] = accessor_map[primitive['indices']]
    # Repack all referenced buffer views, removing obsolete geometry buffers.
    live={a['bufferView'] for a in document['accessors']}
    live.update(image['bufferView'] for image in document.get('images',[]))
    newblob=bytearray();views=[];mapping={}
    for i in sorted(live):
        view=dict(document['bufferViews'][i]); start=view.get('byteOffset',0)
        while len(newblob)%4:newblob.append(0)
        view['byteOffset']=len(newblob)
        newblob.extend(blob[start:start+view['byteLength']]); mapping[i]=len(views);views.append(view)
    for item in document['accessors']+document.get('images',[]):item['bufferView']=mapping[item['bufferView']]
    document['bufferViews']=views;document['buffers'][0]['byteLength']=len(newblob)
    payload=json.dumps(document,separators=(',',':')).encode()
    payload+=b' '*((-len(payload))%4);newblob.extend(b'\0'*((-len(newblob))%4))
    return struct.pack('<III',glb.GLB_MAGIC,2,28+len(payload)+len(newblob))+struct.pack('<II',len(payload),glb.CHUNK_JSON)+payload+struct.pack('<II',len(newblob),glb.CHUNK_BIN)+newblob


def process(path, apply, *, proposed=False):
    raw=path.read_bytes()
    document,blob=canonical_repair_document(raw)
    if document.get('skins') or document.get('animations') or len(document['meshes'])!=1 or len(document['meshes'][0]['primitives'])!=1:
        raise ValueError(f'{path}: requires manual review of rig or multiple meshes')
    if any(any(key in node for key in ('matrix','translation','rotation','scale')) for node in document['nodes']):
        raise ValueError(f'{path}: transformed nodes require local-space review')
    primitive=document['meshes'][0]['primitives'][0]
    if set(primitive['attributes']) != {'POSITION','TEXCOORD_0','COLOR_0'}:
        raise ValueError(f'{path}: unsupported vertex attributes')
    mesh=glb.read_glb(raw); positions=list(mesh.positions); uvs=list(mesh.uvs); colors=list(mesh.colors);indices=list(mesh.indices)
    before=inspect(positions,indices); name=path.stem; changes=[]
    if name=='pool_table':
        removed=remove_pool_table_cap(positions,indices)
        if removed:changes.append(f'removed {removed} coplanar tray closure triangles')
    if name=='tv':
        count=0
        for i,(x,y,z) in enumerate(positions):
            if abs(z-.061)<1e-5:
                positions[i]=(x,y,.04);count+=1
        if count:changes.append(f'recessed {count} screen vertices 21 mm behind their old plane')
    if name=='washer_drum':
        count=0
        for i,(x,y,z) in enumerate(positions):
            if abs(y-.26)<1e-5 and abs(math.hypot(x,z)-.21)<1e-5:
                positions[i]=(x,.3,z);count+=1
        if count:changes.append(f'joined {count} outer-wall vertices to the rim, closing the 40 mm gap')
    added=0
    if name=='tv':
        # Four bezel bars: their rear/bottom faces were omitted even where
        # the frame overhangs the smaller electronics housing.
        boxes=[((sx*.5325,.395,.039),(.035,.61,.022)) for sx in (-1,1)]
        boxes += [((0,.685,.039),(1.03,.03,.022)),((0,.1225,.039),(1.03,.065,.022))]
        for center,size in boxes:
            added+=close_box_faces(positions,uvs,colors,indices,center,size,('-y','-z'))
    if name=='bed':
        boxes=[((sx*.67,.18,0),(.06,.2,2)) for sx in (-1,1)]
        boxes += [((0,.18,sz*.97),(1.28,.2,.06)) for sz in (-1,1)]
        for center,size in boxes:
            added+=close_box_faces(positions,uvs,colors,indices,center,size,('-y',))
    for loop,neighbors in boundary_loops(positions,indices):
        if should_close(name,[positions[i] for i in loop]):
            added+=cap_loop(positions,uvs,colors,indices,loop,neighbors)
    if added:changes.append(f'added {added} closure triangles to reviewed missing component faces')
    oriented=orient(name,positions,indices)
    if oriented['flipped_triangles']:changes.append(f"corrected winding on {oriented['flipped_triangles']} triangles")
    if changes:
        old_count=len(positions)
        if name=='pool_table':
            # Preserve every surviving record/order; only drop vertices that
            # belonged exclusively to the removed coplanar closure.
            used=sorted(set(indices)); remap={old:new for new,old in enumerate(used)}
            positions=[positions[i] for i in used]
            uvs=[uvs[i] for i in used]; colors=[colors[i] for i in used]
            indices=[remap[i] for i in indices]
        else:
            positions,uvs,colors,indices=compact(positions,uvs,colors,indices)
        changes.append(f'removed {old_count-len(positions)} redundant full-attribute vertex records')
        output=encode(document,blob,positions,uvs,colors,indices)
        checked=glb.read_glb(output)
        if checked.texture_png != mesh.texture_png:raise ValueError('texture bytes changed')
        if apply:atomic_write(path, output)
    else:output=raw
    return {'file':str(path.relative_to(ROOT)),'changed':bool(changes),'changes':changes,
            'before':before,'after':inspect(positions,indices),
            'embedded_png_sha256':hashlib.sha256(mesh.texture_png).hexdigest(),
            'sha256':hashlib.sha256(output).hexdigest(),
            **({'payload':output,'original':hashlib.sha256(raw).hexdigest()} if proposed else {})}


def _process_job(path):
    return process(path, False, proposed=True)


def overlap_audit(path, *, max_candidates=2_000_000):
    """A reproducible read-only report and hash-bound conservative edit plan."""
    path = Path(path).resolve()
    source = str(path)
    raw = path.read_bytes()
    return {"source": source, "sha256": hashlib.sha256(raw).hexdigest(),
            **overlap_report(glb_triangles(raw, source=source), max_candidates=max_candidates)}


def instance_overlap_audit(path, *, max_candidates=2_000_000):
    """Audit distinct fixed placements exported by the engine, not guessed JSON.

    The matrix includes the engine's floor reference, uniform scale, yaw and
    span/shear. Animated/morph assets and live movable poses are explicitly
    listed outside this static scope. Per-slot map materials retain identity.
    Cross-instance findings are review only: a structural intersection never
    authorizes deleting an entire placement or one of its source model faces.
    """
    document = json.loads(Path(path).read_text())
    if document.get("format") != "places-prop-instances" or document.get("version") != 1:
        raise ValueError("expected an engine-exported places-prop-instances manifest")
    cache = {}
    triangles = []
    excluded = list(document.get("excluded", []))
    models = {}
    for instance in document["instances"]:
        model = str(Path(instance["model"]).resolve())
        if model not in cache:
            raw = Path(model).read_bytes()
            models[model] = hashlib.sha256(raw).hexdigest()
            try:
                cache[model] = glb_triangles(raw, source=model)
            except ValueError as error:
                cache[model] = str(error)
        if isinstance(cache[model], str):
            excluded.append({"owner": instance["owner"], "reason": cache[model]})
            continue
        placed = transformed_triangles(cache[model], instance["matrix"], owner=instance["owner"])
        overrides = instance.get("material_overrides", {})
        triangles.extend(replace(triangle, material=f"catalog:{overrides[triangle.material.rsplit('/', 1)[1]]}", alpha_mode="UNKNOWN_OVERRIDE")
                         if triangle.material.rsplit('/', 1)[1] in overrides else triangle for triangle in placed)
    report = overlap_report(triangles, max_candidates=max_candidates, cross_instances=True)
    # Source faces cannot be deleted to resolve a placement-to-placement clash.
    report["repair_plan"] = []
    for finding in report["findings"]:
        finding["auto_repairable"] = False
    return {"format": "places-instance-overlap", "version": 1,
            "level": document["level"], "source_sha256": document["source_sha256"],
            "scope": "distinct fixed static GLB instances; no architecture or live poses",
            "models": models, "excluded": excluded, **report}


def remove_duplicate_triangles(raw, plan, *, source):
    """Change only index accessors for a freshly re-verified duplicate plan.

    Material definitions, image buffers and every existing vertex accessor stay
    byte-identical. Appending independent index accessors also preserves other
    primitives that happened to share the old accessor.
    """
    document, blob = canonical_repair_document(raw)
    report = overlap_report(glb_triangles(raw, source=source))
    if not report["complete"] or report["repair_plan"] != plan:
        raise ValueError("duplicate repair plan does not match the current complete audit")
    if not plan:
        return raw
    removals = {}
    for edit in plan:
        suffix = edit["reference"].removeprefix(source + "/nodes/")
        fields = suffix.split("/")
        if len(fields) != 7 or fields[1::2] != ["meshes", "primitives", "triangles"]:
            raise ValueError("invalid source triangle address")
        mesh, primitive, triangle = (int(fields[i]) for i in (2, 4, 6))
        removals.setdefault((mesh, primitive), set()).add(triangle)
    for (mesh, slot), removed in sorted(removals.items()):
        primitive = document["meshes"][mesh]["primitives"][slot]
        indices = [int(row[0]) for row in glb._read_accessor(document, blob, primitive["indices"])] if "indices" in primitive else list(range(document["accessors"][primitive["attributes"]["POSITION"]]["count"]))
        retained = [index for start in range(0, len(indices), 3) if start // 3 not in removed
                    for index in indices[start:start + 3]]
        while len(blob) % 4:
            blob.append(0)
        start = len(blob)
        for index in retained:
            blob.extend(struct.pack("<I", index))
        view = len(document["bufferViews"])
        document["bufferViews"].append({"buffer": 0, "byteOffset": start,
                                      "byteLength": len(blob) - start, "target": 34963})
        primitive["indices"] = len(document["accessors"])
        document["accessors"].append({"bufferView": view, "componentType": 5125,
                                      "count": len(retained), "type": "SCALAR"})
    document["buffers"][0]["byteLength"] = len(blob)
    payload = json.dumps(document, separators=(",", ":")).encode()
    payload += b" " * (-len(payload) % 4)
    blob.extend(b"\0" * (-len(blob) % 4))
    result = (struct.pack("<III", glb.GLB_MAGIC, 2, 28 + len(payload) + len(blob))
              + struct.pack("<II", len(payload), glb.CHUNK_JSON) + payload
              + struct.pack("<II", len(blob), glb.CHUNK_BIN) + blob)
    before, after = glb.read_glb(raw), glb.read_glb(result)
    if before.texture_pngs != after.texture_pngs or before.json.get("materials") != after.json.get("materials"):
        raise ValueError("duplicate repair changed material or texture identity")
    post = overlap_report(glb_triangles(result, source=source))
    if not post["complete"] or post["repair_plan"]:
        raise ValueError("duplicate repair did not produce an empty second plan")
    return result


def apply_overlap_plan(document):
    """Verify every source and proposed payload before publishing any model."""
    if document.get("format") != "places-model-overlap" or document.get("version") != 1:
        raise ValueError("unsupported model overlap report")
    proposals = []
    for row in document["models"]:
        path = Path(row["source"]).resolve()
        raw = path.read_bytes()
        if hashlib.sha256(raw).hexdigest() != row["sha256"]:
            raise ValueError(f"{path}: source sha256 changed; refusing stale plan")
        if not row.get("complete"):
            raise ValueError(f"{path}: incomplete overlap audit")
        payload = remove_duplicate_triangles(raw, row["repair_plan"], source=str(path))
        proposals.append((path, raw, payload))
    for path, raw, _ in proposals:
        if path.read_bytes() != raw:
            raise ValueError(f"{path}: changed during verification; no repairs published")
    for path, raw, payload in proposals:
        if payload != raw:
            atomic_write(path, payload)
    return sum(raw != payload for _, raw, payload in proposals)


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--apply',action='store_true');parser.add_argument('--report',type=Path)
    parser.add_argument("--workers",type=int,help="CPU workers for model audits")
    parser.add_argument("--audit-model", type=Path, nargs="+", help="read-only coplanar/near-coplanar audit; does not run legacy named repairs")
    parser.add_argument("--audit-instances", type=Path, help="read-only cross-instance audit of an engine --prop-instances manifest")
    parser.add_argument("--apply-overlap-plan", type=Path, help="apply only freshly verified exact duplicates from a hash-bound audit report")
    parser.add_argument("--max-candidates", type=int, default=2_000_000, help="hard pair-work budget per overlap audit")
    parser.add_argument("--plan-reception-desk", type=Path, help="plan the reviewed wood/paint edge clipping repair for the original export")
    parser.add_argument("--apply-reception-plan", type=Path, help="apply a reviewed reception plan after hash and contract verification")
    args=parser.parse_args()
    if args.audit_instances:
        if args.apply or args.audit_model or args.apply_overlap_plan or args.plan_reception_desk or args.apply_reception_plan:
            parser.error("select one repair/audit operation")
        report = instance_overlap_audit(args.audit_instances, max_candidates=args.max_candidates)
        if args.report:
            args.report.parent.mkdir(parents=True, exist_ok=True)
            args.report.write_text(json.dumps(report, indent=2) + "\n")
        print(report["level"], "complete" if report["complete"] else "INCOMPLETE", report["totals"], f"{len(report['excluded'])} excluded live/animated placements")
        return
    if args.plan_reception_desk or args.apply_reception_plan:
        if args.apply or args.audit_model or args.apply_overlap_plan or (args.plan_reception_desk and args.apply_reception_plan):
            parser.error("select one repair/audit operation")
        if args.plan_reception_desk:
            plan = reception_repair_plan(args.plan_reception_desk)
            if args.report:
                args.report.parent.mkdir(parents=True, exist_ok=True)
                args.report.write_text(json.dumps(plan, indent=2) + "\n")
            print(json.dumps(plan, indent=2))
        else:
            plan = json.loads(args.apply_reception_plan.read_text())
            path = Path(plan["source"])
            raw = path.read_bytes()
            payload = repair_reception_desk(raw, plan)
            if path.read_bytes() != raw:
                raise ValueError("reception source changed during verification")
            atomic_write(path, payload)
            print(f"clipped reviewed wood/paint overlap: {path} sha256 {hashlib.sha256(payload).hexdigest()}")
        return
    if args.audit_model or args.apply_overlap_plan:
        if args.apply or (args.audit_model and args.apply_overlap_plan):
            parser.error("overlap audits/plans and legacy --apply are separate operations")
        if args.apply_overlap_plan:
            print(f"repaired {apply_overlap_plan(json.loads(args.apply_overlap_plan.read_text()))} model(s)")
        else:
            report = {"format": "places-model-overlap", "version": 1,
                      "models": [overlap_audit(path, max_candidates=args.max_candidates) for path in args.audit_model]}
            if args.report:
                args.report.parent.mkdir(parents=True, exist_ok=True)
                args.report.write_text(json.dumps(report, indent=2) + "\n")
            for row in report["models"]:
                print(row["source"], "complete" if row["complete"] else "INCOMPLETE", row["totals"], f"{len(row['repair_plan'])} safe deletion(s)")
        return
    try: worker_count(args.workers)
    except ValueError as error: parser.error(str(error))
    paths=[p for p in sorted((ROOT/'assets').rglob('*.glb')) if p.stem in NAMES]
    rows=ordered_map(_process_job,paths,args.workers,automatic=8,progress="geometry")
    if args.apply:
        for path,row in zip(paths,rows):
            if hashlib.sha256(path.read_bytes()).hexdigest()!=row['original']:
                raise ValueError(f'{path}: changed during audit; no repairs published')
    for path,row in zip(paths,rows):
        payload=row.pop('payload');row.pop('original')
        if args.apply and row['changed']:atomic_write(path,payload)
    if args.report:
        args.report.parent.mkdir(parents=True,exist_ok=True);args.report.write_text(json.dumps(rows,indent=2)+'\n')
    for row in rows:print(Path(row['file']).stem, '; '.join(row['changes']) or 'inspected; no repair needed')


if __name__=='__main__':main()
