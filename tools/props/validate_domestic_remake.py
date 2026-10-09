#!/usr/bin/env python3
"""Focused geometry/source/rig audit for the ten domestic remakes (no Rust)."""
import json
import math
from pathlib import Path
import struct

import build
import geometry
import glb
from tex import decode_png
from parts.domestic_remade import switch
from mesh import PropBuilder

IDS = {'home:'+n for n in ('knife','fork','spoon','plate','bowl','wall_switch','crt_tv')}
IDS |= {'core:'+n for n in ('couch','bed','chair')}
ROOT = Path(__file__).resolve().parents[2]


def main():
    results = []
    for entry in build.catalog_placeables(build.load_catalog()):
        if entry['id'] not in IDS:
            continue
        path = ROOT/'assets'/entry['model']
        model = glb.read_glb(path.read_bytes())
        audit = geometry.inspect(model.positions, model.indices)
        for key in ('degenerate','boundary_edges','nonmanifold_edges','inconsistent_edges',
                    'flipped_triangles','contradictory_components'):
            assert audit[key] == 0, (entry['id'], key, audit[key])
        assert all(math.isfinite(v) and 0 <= v <= 1 for uv in model.uvs for v in uv)
        assert model.triangle_count < 800
        assert decode_png(model.texture_png) == decode_png(path.with_suffix('.png').read_bytes())
        assert all(math.isfinite(v) for pos in model.positions for v in pos)
        audit.update(id=entry['id'], path=str(path.relative_to(ROOT)), bounds=model.bounds(),
                     texture_matches_source=True, animations=model.animation_names)
        if entry['id'] == 'home:wall_switch':
            assert model.animation_names == ['toggle']
            assert model.node_names == ['switch','lever_pivot','lever']
            blob = path.read_bytes()
            json_length = struct.unpack_from('<I',blob,12)[0]
            binary = blob[28+json_length:]
            sampler = model.json['animations'][0]['samplers'][0]
            times = glb._read_accessor(model.json,binary,sampler['input'])
            rotations = glb._read_accessor(model.json,binary,sampler['output'])
            assert abs(times[-1][0]-.35) < 1e-6
            assert rotations[0] == (0.,0.,0.,1.)
            assert abs(rotations[-1][0]-math.sin(math.radians(-15))) < 1e-6
            # Sweep the actual rocker geometry around its hinge through both stops.
            p=PropBuilder(entry['id'],'switch',entry['size']);switch(p)
            # Last component is the 96-vertex bevelled rocker.
            rocker=p.mesh.positions[-96:]
            minimum_z=1.
            for frame in range(31):
                angle=math.radians(-frame)
                for x,y,z in rocker:
                    yy,zz=y-.066,z-.022
                    depth=.022+yy*math.sin(angle)+zz*math.cos(angle)
                    minimum_z=min(minimum_z,depth)
                    assert depth >= .010, 'rocker penetrates backing plate'
            audit['toggle_sweep_samples']=31
            audit['rocker_min_z']=minimum_z
        results.append(audit)
    assert len(results)==10
    out=ROOT/'debug-maps/domestic-model-remake/geometry-validation.json'
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(results,indent=2)+'\n')
    print('PASS: ten closed meshes, finite positions/UVs, outward winding, source textures, switch toggle sweep')


if __name__ == '__main__':
    main()
