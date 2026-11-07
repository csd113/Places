#!/usr/bin/env python3
"""Regenerate the reviewed Tall Guest's fixed cavity lighting data from a GPU dump.

The original GLB is read only. Use the held-idle native reference with
real_character_self_shadows=true in settings, PLACES_BENCH=1 and
PLACES_POSED_AMBIENT_DUMP=1.
The log must contain exactly one ready 5,138-vertex opaque receiver.
"""
import argparse
import hashlib
import json
import math
from pathlib import Path
import struct

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--log', type=Path, required=True)
parser.add_argument('--model', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
source = args.model.read_bytes()
assert hashlib.sha256(source).hexdigest() == '74bc2d7882f1952ac2a97db4a3e5d4cab4e3f769e82c362b4fff866625611be1'
length = struct.unpack_from('<I', source, 12)[0]
gltf = json.loads(source[20:20+length])
binary = source[28+length:]
positions = []
interior = []
for primitive in gltf['meshes'][0]['primitives']:
    accessor = gltf['accessors'][primitive['attributes']['POSITION']]
    view = gltf['bufferViews'][accessor['bufferView']]
    assert accessor['componentType'] == 5126 and accessor['type'] == 'VEC3'
    stride = view.get('byteStride', 12)
    offset = view.get('byteOffset', 0) + accessor.get('byteOffset', 0)
    interior.extend([primitive['material'] == 1] * accessor['count'])
    positions.extend(struct.unpack_from('<3f', binary, offset+i*stride) for i in range(accessor['count']))
dumps = [json.loads(line[len('[posed-ambient] '):])['values'] for line in args.log.read_text().splitlines() if line.startswith('[posed-ambient] ')]
assert len(dumps) == 1
values = dumps[0]
assert len(values) == len(positions) == 5138
assert all(len(row) == 4 and all(math.isfinite(v) for v in row) and row[2] == 1 for row in values)
data = bytes(round(min(max(row[0], 0), 1)*255) if mouth else 255 for mouth, row in zip(interior, values))
args.output.mkdir(parents=True, exist_ok=True)
path = args.output/'cavity_visibility.bin'
with path.open('xb') as output:
    output.write(data)
print(json.dumps(dict(bytes=len(data), sha256=hashlib.sha256(data).hexdigest(), path=str(path))))
