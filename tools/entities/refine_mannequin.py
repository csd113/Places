#!/usr/bin/env python3
"""Refine the accepted encounter mannequin without changing its fitted skin contract.

Offline Blender Python geometry/animation authoring (no image synthesis):
  blender -b --python tools/entities/refine_mannequin.py -- SOURCE.glb OUTPUT.glb
The exact 25-clip source is pinned; rig, sockets, UVs, materials, PNGs, indices,
weights and unrelated clips remain unchanged. Run after add_encounter_clips.py.
"""
from pathlib import Path
import copy
import hashlib
import json
import math
import struct
import sys
from mathutils import Matrix, Quaternion, Vector
sys.path.insert(0, str(Path(__file__).resolve().parent))
from validate_entities import Model

SOURCE_SHA256 = 'dcaa55ad8065de0653a4821507fd19cd568386cdfb3257ec233f6ef4ff9d7e54'
source, output = map(Path, sys.argv[sys.argv.index('--') + 1:])
assert hashlib.sha256(source.read_bytes()).hexdigest() == SOURCE_SHA256
model = Model(source)
doc = copy.deepcopy(model.document)
binary = bytearray(model.binary)
node_index = {n['name']: i for i, n in enumerate(model.nodes)}

def matrix(flat):
    return Matrix([[flat[c * 4 + r] for c in range(4)] for r in range(4)])

rest = [matrix(v) for v in model._identity_globals()]
clips = {c['name']: c for c in model.clips}

def local_pose(name, phase):
    g = [matrix(v) for v in model._pose_globals(clips[name], phase * clips[name]['duration'])]
    return [g[model.parent[i]].inverted() @ v if model.parent[i] is not None else v for i, v in enumerate(g)]

stand = local_pose('pose_stand', 0)

def globals_for(local):
    result = [Matrix.Identity(4) for _ in local]
    for i in model.order:
        p = model.parent[i]
        result[i] = result[p] @ local[i] if p is not None else local[i]
    return result

def set_rotation(local, name, rotation):
    i = node_index[name]
    g = globals_for(local)
    desired = Matrix.LocRotScale(g[i].translation, rotation, g[i].to_scale())
    p = model.parent[i]
    local[i] = g[p].inverted() @ desired if p is not None else desired

def align(local, name, child, endpoint):
    i, j = node_index[name], node_index[child]
    origin = globals_for(local)[i].translation
    direction = endpoint - origin
    original = rest[j].translation - rest[i].translation
    set_rotation(local, name, original.rotation_difference(direction) @ rest[i].to_quaternion())

def accessor(rows, kind, bounds=False):
    binary.extend(b'\0' * (-len(binary) % 4))
    offset = len(binary)
    values = [v for row in rows for v in row]
    binary.extend(struct.pack('<' + 'f' * len(values), *values))
    view = len(doc['bufferViews'])
    doc['bufferViews'].append({'buffer': 0, 'byteOffset': offset, 'byteLength': len(values) * 4})
    entry = {'bufferView': view, 'componentType': 5126, 'count': len(rows), 'type': kind}
    if bounds or kind == 'SCALAR':
        entry.update(min=[min(row[a] for row in rows) for a in range(len(rows[0]))],
                     max=[max(row[a] for row in rows) for a in range(len(rows[0]))])
    index = len(doc['accessors'])
    doc['accessors'].append(entry)
    return index

def smooth(t):
    t = max(0, min(1, t))
    return t * t * (3 - 2*t)

# Small silhouette changes on the existing low-poly cages; no subdivision,
# topology, joint pivot, UV, weight or image changes. Hands stay exact for grips.
foot_points = {'L': [], 'R': []}
changed_vertices = 0
all_positions = []
for primitive in doc['meshes'][0]['primitives']:
    attrs = primitive['attributes']
    positions = model._read_accessor(attrs['POSITION'], 'VEC3')
    joints = model._read_accessor(attrs['JOINTS_0'], 'VEC4')
    weights = model._read_accessor(attrs['WEIGHTS_0'], 'VEC4')
    refined = []
    for xyz, js, ws in zip(positions, joints, weights):
        p = Vector(xyz)
        name = model.nodes[model.joints[js[ws.index(max(ws))]]]['name']
        if primitive['material'] == 0:
            if name == 'head':
                # Round crown / taper jaw while keeping head height and face UVs.
                crown = smooth((p.y - 1.63) / .093)
                jaw = 1 - smooth((p.y - 1.474) / .075)
                p.x *= 1 - .28*crown - .12*jaw
                p.z = .003 + (p.z-.003) * (1 - .23*crown - .06*jaw)
            elif name == 'spine':
                shoulder = math.exp(-((p.y-1.335)/.055)**2)
                p.x *= 1 - .055*shoulder
                p.z *= 1 - .035*shoulder
            elif name == 'pelvis':
                p.x *= 1 - .025*math.sin(math.pi*max(0,min(1,(p.y-.839)/.154)))
            elif name.startswith('foot.'):
                cx = rest[node_index[name]].translation.x
                toe = smooth((p.z - .06) / .10)
                p.x = cx + (p.x-cx) * (1 - .19*toe)
                # Rounded toe corners, with the sole and heel support retained.
                p.z -= .014*toe*(abs(p.x-cx)/.04)**2
                heel = 1 - smooth((p.z+.043)/.05)
                p.y *= 1 - .12*heel
        if (p-Vector(xyz)).length > 1e-8:
            changed_vertices += 1
        refined.append(list(p))
        if name.startswith('foot.'):
            foot_points[name[-1]].append(rest[node_index[name]].inverted() @ p)
    all_positions.extend(refined)
    attrs['POSITION'] = accessor(refined, 'VEC3', True)
    # Preserve the original hard-edge groups; recompute area-weighted normals
    # only within each original vertex, rather than smoothing across UV seams.
    indices = [int(v[0]) for v in model._read_accessor(primitive['indices'], 'SCALAR')]
    normals = [Vector((0,0,0)) for _ in refined]
    for a,b,c in zip(indices[::3],indices[1::3],indices[2::3]):
        normal = (Vector(refined[b])-Vector(refined[a])).cross(Vector(refined[c])-Vector(refined[a]))
        for i in (a,b,c): normals[i] += normal
    attrs['NORMAL'] = accessor([list(n.normalized()) for n in normals], 'VEC3')

def gait(name, phase, running):
    # Keep variant-specific upper body and fingers. Only the shared root/pelvis and
    # leg chains are replaced; crowbar socket roll and zombie forward reach stay.
    local = local_pose(name, phase)
    # The source's vertical gait displacement lives in root, not pelvis.
    # Replace it too: keeping it would double the authored compression.
    local[node_index['root']] = stand[node_index['root']].copy()
    duration = clips[name]['duration']
    speed = 3.2 if running else 1.0
    duty = .30 if running else .62
    stride = speed * duration * duty
    p = node_index['pelvis']
    local[p] = stand[p].copy()
    # Restrained hip motion rises through walking support; the complete
    # trajectory is limited smoothly against both leg reaches below.
    bob = (.020 if running else .028) * math.cos(2*math.tau*phase)
    local[p].translation.y -= (.050 if running else .048) + bob
    local[p].translation.x += (.008 if running else .012)*math.sin(math.tau*phase)
    # Restrained pelvis yaw makes the stride less like two sagittal pistons.
    position, rotation, scale = local[p].decompose()
    local[p] = Matrix.LocRotScale(position, Quaternion((0,1,0),.035*math.cos(math.tau*phase)) @ rotation, scale)
    targets = []
    for side, shift in [('L',0),('R',.5)]:
        t = (phase+shift) % 1
        if t < duty:
            step = stride*.5 - speed*duration*t
            # Heel settles softly, then rolls to toe as the other leg recovers.
            heel = 1-smooth(t/(duty*.20))
            push = smooth((t/duty-.72)/.28)
            pitch = math.radians(-10*heel + (24 if running else 16)*push)
            lift = 0.0
        else:
            u = (t-duty)/(1-duty)
            # Cubic Hermite: same backward velocity at lift-off and touchdown,
            # with smooth reversal in flight instead of a high-knee march.
            v = -speed*duration*(1-duty)
            step = (2*u**3-3*u*u+1)*(-stride*.5) + (u**3-2*u*u+u)*v + (-2*u**3+3*u*u)*(stride*.5) + (u**3-u*u)*v
            lift = (.155 if running else .055)*math.sin(math.pi*u)**2
            pitch = math.radians((24 if running else 16)*(1-smooth(u/.55))-10*smooth((u-.65)/.35))
        thigh, shin, foot = [node_index[f'{part}.{side}'] for part in ('thigh','shin','foot')]
        foot_rotation = Quaternion((1,0,0),pitch) @ rest[foot].to_quaternion()
        ankle = rest[foot].translation.copy()
        ankle.z += step
        ankle.y = -min((foot_rotation @ v).y for v in foot_points[side]) + lift
        targets.append((side, thigh, shin, foot, ankle, foot_rotation))
    # Limit the complete pelvis curve against both actual ankle targets before
    # solving either leg. Smooth positive-part avoids knee-locking or a hard
    # height clamp when the trailing foot reaches full extension.
    corrections = []
    globals_ = globals_for(local)
    for side, thigh, shin, foot, ankle, foot_rotation in targets:
        hip = globals_[thigh].translation
        reach = ((rest[shin].translation-rest[thigh].translation).length
                 + (rest[foot].translation-rest[shin].translation).length - .008)
        horizontal = (ankle.x-hip.x)**2 + (ankle.z-hip.z)**2
        height_limit = ankle.y + math.sqrt(max(0, reach*reach-horizontal))
        excess = hip.y-height_limit
        corrections.append(.5*(excess+math.sqrt(excess*excess+.003**2)))
    a,b = corrections
    local[p].translation.y -= .5*(a+b+math.sqrt((a-b)**2+.001**2))
    for side, thigh, shin, foot, ankle, foot_rotation in targets:
        hip = globals_for(local)[thigh].translation
        a = (rest[shin].translation-rest[thigh].translation).length
        b = (rest[foot].translation-rest[shin].translation).length
        delta = ankle-hip
        distance = delta.length
        assert distance < a+b-.0001, (name,phase,side,distance,a+b)
        axis = delta.normalized()
        along = (a*a-b*b+distance*distance)/(2*distance)
        pole = Vector((0,0,1))
        bend = (pole-axis*axis.dot(pole)).normalized()
        knee = hip + axis*along + bend*math.sqrt(max(0,a*a-along*along))
        align(local,f'thigh.{side}',f'shin.{side}',knee)
        align(local,f'shin.{side}',f'foot.{side}',ankle)
        set_rotation(local,f'foot.{side}',foot_rotation)
    return local

replaced = ['walk','carry_walk','crowbar_walk','run','run_arms_forward','crowbar_run']
for animation in doc['animations']:
    name = animation['name']
    if name not in replaced: continue
    running = name.startswith('run') or name == 'crowbar_run'
    count = 121
    poses = [gait(name,i/(count-1),running) for i in range(count)]
    poses[-1] = [m.copy() for m in poses[0]]
    times = accessor([(clips[name]['duration']*i/(count-1),) for i in range(count)],'SCALAR')
    changed_nodes = {node_index[n] for n in ['root','pelvis','thigh.L','shin.L','foot.L','thigh.R','shin.R','foot.R']}
    for channel in animation['channels']:
        node = channel['target']['node']
        if node not in changed_nodes: continue
        path = channel['target']['path']
        component,kind = {'translation':(0,'VEC3'),'rotation':(1,'VEC4'),'scale':(2,'VEC3')}[path]
        rows=[]
        for pose in poses:
            value = pose[node].decompose()[component]
            row = [value.x,value.y,value.z,value.w] if path=='rotation' else list(value)
            if path=='rotation' and rows and sum(a*b for a,b in zip(rows[-1],row))<0: row=[-x for x in row]
            rows.append(row)
        # Append an independent sampler: original samplers may be shared.
        channel['sampler']=len(animation['samplers'])
        animation['samplers'].append({'input':times,'output':accessor(rows,kind),'interpolation':'LINEAR'})

for key in ('nodes','skins','materials','textures','images','samplers','scenes','scene'):
    assert doc.get(key)==model.document.get(key),key
assert binary[:len(model.binary)]==model.binary
for old,new in zip(model.animations,doc['animations']):
    if old['name'] not in replaced: assert old==new,old['name']
for entry in doc['asset']['extras']['places_entity_clips']['clips']:
    if entry['name'] in replaced:
        entry['authoring_revision']='2026-11-11 grounded stride'
        if 'run' in entry['name']:
            entry['kind']='run'
            entry['reference_speed_mps']=3.2
doc['asset']['extras']['places_mannequin_refinement']={
    'source_sha256':SOURCE_SHA256,'generator':'tools/entities/refine_mannequin.py',
    'clips':replaced,'preserved':'rig/UV/PNG/weights/indices/materials/interaction clips',
    'contact':'offline flat-floor foot roll; no runtime IK'}
doc['buffers'][0]['byteLength']=len(binary)
raw=json.dumps(doc,separators=(',',':')).encode();raw+=b' '*(-len(raw)%4)
binary.extend(b'\0'*(-len(binary)%4))
output.write_bytes(struct.pack('<III',0x46546C67,2,28+len(raw)+len(binary))+struct.pack('<II',len(raw),0x4E4F534A)+raw+struct.pack('<II',len(binary),0x004E4942)+binary)
print(json.dumps({'sha256':hashlib.sha256(output.read_bytes()).hexdigest(),'source_sha256':SOURCE_SHA256,'changed_vertices':changed_vertices,'triangles':len(model.indices)//3,'bounds_min':[min(v[a] for v in all_positions) for a in range(3)],'bounds_max':[max(v[a] for v in all_positions) for a in range(3)],'clips':replaced},indent=2))
