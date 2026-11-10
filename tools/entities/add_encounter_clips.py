"""Append crouch and crowbar clips to the accepted mannequin without re-exporting it.

Run with Blender's Python (mathutils only): blender -b --python this_file -- SOURCE OUT.
Original JSON objects, accessor payloads, PNGs, geometry and rig are preserved.
The source must be the accepted R2 export, making regeneration deterministic.
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

SOURCE_HASH = 'a27aa5b4fda530152ba888535e06b21442bb4c05c476ba0028435a4e05291051'
args = sys.argv[sys.argv.index('--') + 1:]
source, output = map(Path, args)
assert hashlib.sha256(source.read_bytes()).hexdigest() == SOURCE_HASH
model = Model(source)
doc = copy.deepcopy(model.document)
binary = bytearray(model.binary)
indices = {n['name']: i for i, n in enumerate(model.nodes)}

def mat(flat):
    return Matrix([[flat[c * 4 + r] for c in range(4)] for r in range(4)])

rest = [mat(v) for v in model._identity_globals()]
clips = {c['name']: c for c in model.clips}

def sampled(name, time):
    return [mat(v) for v in model._pose_globals(clips[name], time)]

def local_from_global(globals_):
    return [(globals_[model.parent[i]].inverted() @ g if model.parent[i] is not None else g)
            for i, g in enumerate(globals_)]

stand = local_from_global(sampled('pose_stand', 0))

def globals_for(local):
    out = [Matrix.Identity(4) for _ in local]
    for i in model.order:
        p = model.parent[i]
        out[i] = out[p] @ local[i] if p is not None else local[i]
    return out

def rotation_global(local, name, rotation):
    i = indices[name]
    g = globals_for(local)
    p = model.parent[i]
    desired = Matrix.LocRotScale(g[i].translation, rotation, g[i].to_scale())
    local[i] = g[p].inverted() @ desired if p is not None else desired

def align_bone(local, name, child, endpoint):
    g = globals_for(local)
    i, j = indices[name], indices[child]
    origin = g[i].translation
    original = rest[j].translation - rest[i].translation
    desired = endpoint - origin
    rotation_global(local, name, original.rotation_difference(desired) @ rest[i].to_quaternion())

def crouch(phase, walking):
    local = [m.copy() for m in stand]
    # Deep squat: lowered pelvis and forward torso, with grounded two-bone legs.
    pelvis = indices['pelvis']
    local[pelvis].translation.y -= 0.53
    local[pelvis].translation.z -= 0.18
    rotation_global(local, 'spine', Quaternion((1, 0, 0), math.radians(62)))
    for side, shift in [('L', 0), ('R', 0.5)]:
        t = (phase + shift) % 1
        step = 0.0
        lift = 0.0
        if walking:
            if t < 0.5:
                step = 0.14 - 0.56 * t
            else:
                swing = (t - 0.5) * 2
                step = -0.14 + 0.28 * (swing * swing * (3 - 2 * swing))
                lift = 0.045 * math.sin(math.pi * swing)
        thigh, shin, foot = (indices[f'{n}.{side}'] for n in ('thigh', 'shin', 'foot'))
        ankle = rest[foot].translation.copy()
        ankle.z += step
        ankle.y += lift
        hip = globals_for(local)[thigh].translation
        a = (rest[shin].translation - rest[thigh].translation).length
        b = (rest[foot].translation - rest[shin].translation).length
        delta = ankle - hip
        distance = delta.length
        axis = delta.normalized()
        along = (a*a - b*b + distance*distance) / (2*distance)
        bend = (Vector((0, 0, 1)) - axis * axis.z).normalized()
        knee = hip + axis * along + bend * math.sqrt(max(0, a*a - along*along))
        align_bone(local, f'thigh.{side}', f'shin.{side}', knee)
        align_bone(local, f'shin.{side}', f'foot.{side}', ankle)
        rotation_global(local, f'foot.{side}', rest[foot].to_quaternion())
    # Arms comfortably forward, hands clear of knees.
    for side in ('L', 'R'):
        for part, angle in [('upper_arm', -65), ('forearm', -100), ('hand', -100)]:
            name = f'{part}.{side}'
            rotation_global(local, name, Quaternion((1, 0, 0), math.radians(angle)) @ rest[indices[name]].to_quaternion())
    return local

def smooth_keys(t, values):
    keys = [0, 0.18, 0.32, 0.55, 1]
    for i in range(1, len(keys)):
        if t <= keys[i]:
            u = (t-keys[i-1])/(keys[i]-keys[i-1]); u = u*u*(3-2*u)
            return values[i-1]*(1-u)+values[i]*u
    return values[-1]

def bar_pose(phase, walking=False, attack=False):
    local = local_from_global(sampled('walk', phase*1.1)) if walking else [m.copy() for m in stand]
    upper = smooth_keys(phase, [-30, -120, -65, -10, -30]) if attack else -30
    lower = smooth_keys(phase, [-90, -170, -80, -30, -90]) if attack else -90
    for part, angle in [('upper_arm', upper), ('forearm', lower), ('hand', lower)]:
        name = f'{part}.R'
        rotation = Quaternion((1, 0, 0), math.radians(angle)) @ rest[indices[name]].to_quaternion()
        # Roll the closed hand around its forearm axis: the grasp and shaft
        # rotate together, so the crowbar points upward rather than sideways.
        if part == 'hand': rotation = rotation @ Quaternion((0, 1, 0), math.pi / 2)
        rotation_global(local, name, rotation)
    for name, i in indices.items():
        if name.endswith('.R') and any(n in name for n in ('index.', 'middle.', 'ring.', 'pinky.')):
            p, q, scale = local[i].decompose()
            local[i] = Matrix.LocRotScale(p, q @ Quaternion((1, 0, 0), 1.1 if '.01.' in name else 1.25), scale)
    return local

def accessor(rows, kind):
    binary.extend(b'\0' * (-len(binary) % 4))
    offset = len(binary)
    values = [v for row in rows for v in row]
    binary.extend(struct.pack('<' + 'f'*len(values), *values))
    view = len(doc['bufferViews'])
    doc['bufferViews'].append({'buffer': 0, 'byteOffset': offset, 'byteLength': len(values)*4})
    entry = {'bufferView': view, 'componentType': 5126, 'count': len(rows), 'type': kind}
    if kind == 'SCALAR': entry.update(min=[min(values)], max=[max(values)])
    index = len(doc['accessors']); doc['accessors'].append(entry)
    return index

metadata = doc['asset']['extras']['places_entity_clips']['clips']
for name, duration, loop, speed, sample in [
    ('crouch_idle', 1, True, None, lambda t: crouch(0, False)),
    ('crouch_walk', 1.0, True, 0.56, lambda t: crouch(t, True)),
    ('crowbar_idle', 1, True, None, lambda t: bar_pose(0)),
    ('crowbar_walk', 1.1, True, 1.0, lambda t: bar_pose(t, walking=True)),
    ('crowbar_swing', 1, False, None, lambda t: bar_pose(t, attack=True)),
]:
    count = 61
    poses = [sample(i/(count-1)) for i in range(count)]
    if loop: poses[-1] = [m.copy() for m in poses[0]]
    times = accessor([(duration*i/(count-1),) for i in range(count)], 'SCALAR')
    animation = {'name': name, 'channels': [], 'samplers': []}
    for node in model.joints:
        decomposed = [pose[node].decompose() for pose in poses]
        for path, component, kind in [('translation', 0, 'VEC3'), ('rotation', 1, 'VEC4'), ('scale', 2, 'VEC3')]:
            rows = []
            for values in decomposed:
                v = values[component]
                row = [v.x, v.y, v.z, v.w] if path == 'rotation' else list(v)
                if path == 'rotation' and rows and sum(a*b for a,b in zip(rows[-1],row)) < 0:
                    row = [-x for x in row]
                rows.append(row)
            animation['channels'].append({'sampler': len(animation['samplers']), 'target': {'node': node, 'path': path}})
            animation['samplers'].append({'input': times, 'output': accessor(rows, kind), 'interpolation': 'LINEAR'})
    doc['animations'].append(animation)
    metadata.append({'name': name, 'duration': duration, 'loop': loop, 'kind': 'walk' if speed else 'interaction' if not loop else 'idle', 'reference_speed_mps': speed, 'channels': len(animation['channels'])})

# Fail if any original asset contract or binary byte was rewritten.
for key in ('nodes', 'skins', 'meshes', 'materials', 'textures', 'images', 'samplers', 'scenes', 'scene'):
    assert doc.get(key) == model.document.get(key), key
assert bytes(binary[:len(model.binary)]) == model.binary
assert doc['animations'][:len(model.animations)] == model.animations

doc['buffers'][0]['byteLength'] = len(binary)
raw = json.dumps(doc, separators=(',', ':')).encode(); raw += b' ' * (-len(raw)%4)
binary.extend(b'\0' * (-len(binary)%4))
output.parent.mkdir(parents=True, exist_ok=True)
output.write_bytes(struct.pack('<III', 0x46546C67, 2, 28+len(raw)+len(binary)) + struct.pack('<II',len(raw),0x4E4F534A) + raw + struct.pack('<II',len(binary),0x004E4942) + binary)
verified = Model(output)
report = {'source_sha256': SOURCE_HASH, 'sha256': hashlib.sha256(output.read_bytes()).hexdigest(), 'preserved_binary_bytes': len(model.binary), 'samples': {}}
for clip in verified.clips[-5:]:
    extents = []
    for t in (0, .18, .32, .5, .75, 1):
        vertices = verified._skin(verified._pose_globals(clip, t*clip['duration']))
        extents.append([min(p[1] for p in vertices), max(p[1] for p in vertices)])
    report['samples'][clip['name']] = extents
print(json.dumps(report, indent=2))
