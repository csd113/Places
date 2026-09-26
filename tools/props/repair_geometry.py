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

import glb
from geometry import boundary_loops, inspect, _cross, _sub, _dot

ROOT = Path(__file__).resolve().parents[2]
NAMES = set('armchair bed bookshelf cardboard_box couch crate fridge lamp plant rug sink stove table tv washer_drum washing_machine cabinet_base cabinet_wall cabinet chair desk vending_machine water_cooler pool_chair pool_curtain_corner pool_curtain_end pool_curtain_straight pool_guardrail_corner pool_guardrail_end pool_guardrail_straight pool_ladder pool_table'.split())
# Only these reviewed boundary types are closed; basin mouths, cloth edges and
# independent textured face planes must never be filled by a generic hole pass.
CLOSE_ALL = {'bed', 'water_cooler', 'pool_chair', 'pool_table', 'pool_ladder',
             'pool_curtain_corner', 'pool_curtain_end', 'pool_curtain_straight', 'vending_machine'}


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
    # Drop now-unreferenced legacy accessors before packing, too.
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


def process(path, apply):
    raw=path.read_bytes();n=struct.unpack_from('<I',raw,12)[0]
    document=json.loads(raw[20:20+n]);blob=bytearray(raw[28+n:])
    if document.get('skins') or document.get('animations') or len(document['meshes'])!=1 or len(document['meshes'][0]['primitives'])!=1:
        raise ValueError(f'{path}: requires manual review of rig or multiple meshes')
    if any(any(key in node for key in ('matrix','translation','rotation','scale')) for node in document['nodes']):
        raise ValueError(f'{path}: transformed nodes require local-space review')
    primitive=document['meshes'][0]['primitives'][0]
    if set(primitive['attributes']) != {'POSITION','TEXCOORD_0','COLOR_0'}:
        raise ValueError(f'{path}: unsupported vertex attributes')
    mesh=glb.read_glb(raw); positions=list(mesh.positions); uvs=list(mesh.uvs); colors=list(mesh.colors);indices=list(mesh.indices)
    before=inspect(positions,indices); name=path.stem; changes=[]
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
        positions,uvs,colors,indices=compact(positions,uvs,colors,indices)
        changes.append(f'removed {old_count-len(positions)} redundant full-attribute vertex records')
        output=encode(document,blob,positions,uvs,colors,indices)
        checked=glb.read_glb(output)
        if checked.texture_png != mesh.texture_png:raise ValueError('texture bytes changed')
        if apply:path.write_bytes(output)
    else:output=raw
    return {'file':str(path.relative_to(ROOT)),'changed':bool(changes),'changes':changes,
            'before':before,'after':inspect(positions,indices),
            'embedded_png_sha256':hashlib.sha256(mesh.texture_png).hexdigest(),
            'sha256':hashlib.sha256(output).hexdigest()}


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--apply',action='store_true');parser.add_argument('--report',type=Path)
    args=parser.parse_args()
    rows=[process(p,args.apply) for p in sorted((ROOT/'assets').rglob('*.glb')) if p.stem in NAMES]
    if args.report:
        args.report.parent.mkdir(parents=True,exist_ok=True);args.report.write_text(json.dumps(rows,indent=2)+'\n')
    for row in rows:print(Path(row['file']).stem, '; '.join(row['changes']) or 'inspected; no repair needed')


if __name__=='__main__':main()
