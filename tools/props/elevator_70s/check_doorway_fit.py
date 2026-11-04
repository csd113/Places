#!/usr/bin/env python3
"""Opt-in manufactured doorway fit and unchanged-cabin audit; no bake or CI.

Tests actual exported triangles against the documented wall planes and opening,
not only authoring constants. The baseline comparison protects the user's scope:
all geometry/UVs outside the doorway plus the entire button/atlas stay unchanged.
"""
import argparse
from collections import Counter
import json
from pathlib import Path
import subprocess
import sys
import numpy as np

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / 'tools/props'))
from glb import read_glb
from check_seals import blocked

ASSETS = 'assets/shared/props/elevator_70s/'

def data(name, baseline=None):
    path = ASSETS + name
    return subprocess.check_output(['git', 'show', f'{baseline}:{path}'], cwd=ROOT) if baseline else (ROOT / path).read_bytes()

def mesh(role, baseline=None):
    return read_glb(data(f'elevator_{role}_70s.glb', baseline))

def protected(role, baseline=None):
    model = mesh(role, baseline)
    result = Counter()
    for ids in np.asarray(model.indices).reshape(-1, 3):
        positions = np.asarray(model.positions)[ids]
        # Cabin front/threshold is the only permitted edit region. Frame
        # indicator and hall-call equipment remain outside the aperture stock.
        keep = positions[:, 2].min() > .42 or positions[:, 1].max() < -.02 if role == 'cabin' else positions[:, 1].min() > 2.3 or positions[:, 0].max() < -.82
        # User separately authorized closing the three bronze floor skirts.
        uv = np.asarray(model.uvs)[ids].mean(0)
        cell = int(uv[0]*4)+4*int(uv[1]*4)
        floor_skirt = role == 'cabin' and cell == 7 and positions[:,1].max() <= .13001
        if keep and not floor_skirt:
            vertices = [tuple(np.round(np.r_[model.positions[i], model.uvs[i]], 6)) for i in ids]
            result[tuple(sorted(vertices))] += 1
    return result

def run(baseline):
    models = {role: mesh(role) for role in ('cabin', 'frame', 'leaf')}
    triangles = {role: np.asarray(m.positions)[np.asarray(m.indices).reshape(-1, 3)] for role, m in models.items()}
    # Flush jambs and header: no floating stock before the wall, continuous
    # metal on the actual wall plane, no recess behind that plane.
    probes = [(x, y) for x in (-.72, .72) for y in (.1, 1.1, 2.19)] + [(x, 2.225) for x in (-.5, 0, .5)]
    for role, eye, plane, direction in [('frame', -1., 0., 1.), ('cabin', 1., .36, -1.)]:
        for x, y in probes:
            assert not blocked(triangles[role], (x,y,eye), (x,y,plane-direction*.0001)), (role, 'projects off wall', x,y)
            assert blocked(triangles[role], (x,y,eye), (x,y,plane+direction*.0001)), (role, 'missing flush stock', x,y)
    leaf = np.asarray(models['leaf'].positions)
    assert abs(leaf[:,2].min()+.01)<1e-6
    assert abs(leaf[:,2].max()-.014)<1e-6
    assert abs(leaf[:,0].max()-.695)<1e-6
    counts = {}
    if baseline:
        for role in ('cabin','frame'):
            before, after = protected(role, baseline), protected(role)
            assert before == after, (role, 'non-doorway geometry/UV changed', len(before-after), len(after-before))
            counts[role] = sum(after.values())
        for name in ('elevator_button_70s.glb','elevator_70s.png','elevator_70s_master.png'):
            if (ROOT / ASSETS / name).exists():
                assert data(name,baseline)==data(name), name
    return dict(flush_plane_probes=len(probes)*2, landing_wall_z=0, cabin_wall_z=.36,
                steel_leaf_thickness_m=.020, including_concealed_seal_m=.024,
                full_open_lip_m=.007, protected_triangles=counts,
                triangles={role:len(t) for role,t in triangles.items()})

if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--baseline', help='Git revision for unchanged non-doorway geometry and UVs')
    args=parser.parse_args()
    print(json.dumps(run(args.baseline),indent=2))
