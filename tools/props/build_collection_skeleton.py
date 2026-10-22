#!/usr/bin/env python3
"""Append quest walk/sit clips to the accepted skeleton; never rewrite its art.

This is an explicit offline geometry/animation derivative. All original buffer
bytes, PNGs, mesh accessors, skin, nodes and three seated/standing clips remain.
"""
import hashlib
import json
import math
from pathlib import Path
import struct
import sys

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / 'assets/entities/skeleton/model/skeleton.glb'
DEST = ROOT / 'assets/entities/skeleton/model/collection_skeleton.glb'
SOURCE_SHA256 = '449cf146e3a1768e3cbd4f99614d4f424b08d932d66876e34c17af7fe8357e07'


def multiply(a, b):
    x, y, z, w = a
    X, Y, Z, W = b
    return [w*X+x*W+y*Z-z*Y, w*Y-x*Z+y*W+z*X,
            w*Z+x*Y-y*X+z*W, w*W-x*X-y*Y-z*Z]


def mix(a, b, fraction, quaternion=False):
    if quaternion and sum(x*y for x, y in zip(a, b)) < 0:
        b = [-v for v in b]
    result = [x+(y-x)*fraction for x, y in zip(a, b)]
    if quaternion:
        length = math.sqrt(sum(v*v for v in result))
        result = [v/length for v in result]
    return result


def build():
    original = SOURCE.read_bytes()
    if hashlib.sha256(original).hexdigest() != SOURCE_SHA256:
        raise ValueError("Accepted skeleton source changed; review before deriving motion")
    json_len = struct.unpack_from('<I', original, 12)[0]
    document = json.loads(original[20:20+json_len])
    binary = bytearray(original[28+json_len:])
    binary = binary[:document['buffers'][0]['byteLength']]
    original_binary = bytes(binary)

    def values(accessor):
        entry = document['accessors'][accessor]
        view = document['bufferViews'][entry['bufferView']]
        count = {'SCALAR':1, 'VEC3':3, 'VEC4':4}[entry['type']]
        offset = view.get('byteOffset',0)+entry.get('byteOffset',0)
        return list(struct.unpack_from('<'+'f'*count, original_binary, offset))

    poses = {}
    for clip in document['animations']:
        poses[clip['name']] = {(c['target']['node'],c['target']['path']):
            values(clip['samplers'][c['sampler']]['output']) for c in clip['channels']}
    stand, seated = poses['pose_stand'], poses['pose_sit_chair']

    def append(rows, kind):
        while len(binary) % 4: binary.append(0)
        offset = len(binary)
        flat = [v for row in rows for v in row]
        binary.extend(struct.pack('<'+'f'*len(flat), *flat))
        view = len(document['bufferViews'])
        document['bufferViews'].append({'buffer':0,'byteOffset':offset,'byteLength':len(flat)*4})
        accessor = {'bufferView':view,'componentType':5126,'count':len(rows),'type':kind}
        if kind == 'SCALAR': accessor.update(min=[min(flat)],max=[max(flat)])
        index = len(document['accessors']); document['accessors'].append(accessor)
        return index

    def clip(name, duration, sampler, loop, kind, speed=None):
        times = [duration*i/32 for i in range(33)]
        clock = append([[t] for t in times], 'SCALAR')
        animation = {'name':name,'samplers':[],'channels':[]}
        for (node, path), base in stand.items():
            rows = [sampler(node,path,base,i/32) for i in range(33)]
            index = len(animation['samplers'])
            animation['samplers'].append({'input':clock,'output':append(rows,'VEC4' if path=='rotation' else 'VEC3'),'interpolation':'LINEAR'})
            animation['channels'].append({'sampler':index,'target':{'node':node,'path':path}})
        document['animations'].append(animation)
        document['asset']['extras']['places_entity_clips']['clips'].append({
            'name':name,'duration':duration,'kind':kind,'loop':loop,
            'reference_speed_mps':speed,'channels':len(animation['channels'])})

    def walk(node, path, base, t):
        name = document['nodes'][node]['name']
        phase = math.sin(math.tau*t + (math.pi if name.endswith('_r') else 0))
        angle = 0
        if path == 'rotation':
            if name.startswith('thigh_'): angle = -.30*phase
            elif name.startswith('shin_'): angle = .50*max(0,-phase)
            elif name.startswith('foot_'): angle = -.18*max(0,-phase)
            elif name.startswith('upperarm_'): angle = .20*phase
            elif name.startswith('forearm_'): angle = -.10
            return multiply(base,[math.sin(angle/2),0,0,math.cos(angle/2)])
        if path == 'translation' and name == 'pelvis':
            return [base[0],base[1]+.013*(1-math.cos(2*math.tau*t)),base[2]]
        return base

    clip('walk',1.2,walk,True,'walk',.65)
    def sitting(node, path):
        result = list(seated[(node,path)])
        if document['nodes'][node]['name'] == 'root' and path == 'translation':
            result[2] -= .7
        return result

    clip('sit_down',1.6,lambda n,p,b,t:mix(b,sitting(n,p),t*t*(3-2*t),p=='rotation'),False,'interaction')
    clip('quest_seated',1.0,lambda n,p,b,t:sitting(n,p),True,'pose')
    document['buffers'][0]['byteLength'] = len(binary)
    document['asset']['extras']['places_collection_derivative'] = {
        'source_sha256':hashlib.sha256(original).hexdigest(),
        'tool':'tools/props/build_collection_skeleton.py',
        'preserved':'original geometry, skin, PNG and pose clips; append-only BIN'}
    header = json.dumps(document,separators=(',',':')).encode()
    header += b' '*((-len(header))%4)
    binary += b'\0'*((-len(binary))%4)
    result = struct.pack('<III',0x46546c67,2,28+len(header)+len(binary))
    result += struct.pack('<II',len(header),0x4e4f534a)+header
    result += struct.pack('<II',len(binary),0x004e4942)+binary
    if "--check" in sys.argv:
        if DEST.read_bytes() != result:
            raise ValueError("Collection skeleton differs from its reproducible derivative")
    else:
        DEST.write_bytes(result)
    print(f'{DEST.relative_to(ROOT)} {len(result)} bytes; original BIN prefix preserved')


if __name__ == '__main__':
    build()
