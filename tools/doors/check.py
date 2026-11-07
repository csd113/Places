#!/usr/bin/env python3
"""Read-only identity/profile gate for the original Blender/GIMP exit doors."""
from pathlib import Path
import argparse
import hashlib
import json
import struct

ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent
ASSETS = ROOT / 'assets/shared/doors'


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true', help='read-only; also the default')
    parser.parse_args()
    identities = json.loads((HERE / 'metadata/artifact_identities.json').read_text())
    for item in identities['files']:
        relative = Path(item['path'])
        target = HERE / relative if relative.parts[0] == 'source' else ASSETS / relative.name
        assert target.stat().st_size == item['bytes'], target
        assert sha(target) == item['sha256'], target
    reports = json.loads((HERE / 'metadata/asset_gate_report.json').read_text())['models']
    catalog = {item['id']: item for item in json.loads((ROOT / 'assets/catalog.json').read_text())['assets']}
    ledger = json.loads((ROOT / 'assets/completed-models.json').read_text())
    for variant, triangles, box_count, contacts in [('single', 728, 9, 3), ('double', 1268, 15, 4)]:
        stem = 'hollow_metal_exit_door_' + variant
        model = ASSETS / (stem + '.glb')
        data = model.read_bytes()
        assert data[:4] == b'glTF' and struct.unpack_from('<II', data, 4) == (2, len(data))
        size, kind = struct.unpack_from('<I4s', data, 12)
        assert kind == b'JSON'
        document = json.loads(data[20:20 + size])
        report = next(item for item in reports if item['variant'] == variant)
        assert sha(model) == report['sha256']
        assert triangles == sum(document['accessors'][primitive['indices']]['count'] // 3
                                for mesh in document['meshes'] for primitive in mesh['primitives'])
        assert triangles <= 1500 and triangles <= 6000
        assert not document.get('skins') and len(document['materials']) == 2
        assert len(document['images']) == 1
        assert all(material.get('alphaMode', 'OPAQUE') == 'OPAQUE' and not material.get('doubleSided', False)
                   for material in document['materials'])
        assert [clip['name'] for clip in document['animations']] == ['exit_open']
        names = {node['name'] for node in document['nodes']}
        preset = json.loads((ASSETS / (stem + '.json')).read_text())
        assert preset['solid'] is False and preset['occludes'] is False
        boxes = preset['animated_door']['boxes']
        assert len(boxes) == box_count and sum(box['grounding'] for box in boxes) == contacts
        assert all(box['node'] in names for box in boxes)
        assert not any(box['node'].endswith('_hinges') for box in boxes)
        assert preset['bindings'] == [{'on': 'interact', 'actions': [{'action': 'toggle'}]}]
        assert preset['animated_door']['clip'] == 'exit_open'
        logical = 'core:' + stem
        source = 'shared/doors/' + stem + '.glb'
        assert catalog[logical]['model'] == source and catalog[logical]['solid'] is False
        assert ledger[source]['sha256'] == sha(model) and ledger[source]['triangles'] == triangles
        print(f'PASS {logical}: {triangles} triangles, 0 joints, 2 opaque materials, {box_count} posed solids, {contacts} grounding subjects; {sha(model)}')
    print('PASS editable Blender/GIMP source and shared PNG identities; no budgets increased')


if __name__ == '__main__':
    main()
