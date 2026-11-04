#!/usr/bin/env python3
"""Opt-in actual-GLB material check for the elevator's doorway returns.

The exposed pocket folds and narrow frame must use metal atlas cells.
The surrounding front wall must remain walnut, like the side/rear panels.
No map bake or native window.
"""
import argparse
import json
from pathlib import Path
import subprocess
import sys

import numpy as np

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / 'tools/props'))
from glb import read_glb


def first_cell(mesh, origin, target):
    """Material at the nearest actual surface along a finite cabin sightline."""
    ids = np.asarray(mesh.indices).reshape(-1, 3)
    triangles = np.asarray(mesh.positions)[ids]
    origin, target = np.asarray(origin), np.asarray(target)
    direction = target-origin
    a, b, c = triangles[:, 0], triangles[:, 1], triangles[:, 2]
    e1, e2 = b-a, c-a
    p = np.cross(direction, e2)
    determinant = np.einsum('ij,ij->i', e1, p)
    valid = np.abs(determinant)>1e-10
    inverse = np.divide(1., determinant, out=np.zeros_like(determinant), where=valid)
    distance = origin-a
    u = np.einsum('ij,ij->i', distance, p)*inverse
    q = np.cross(distance, e1)
    v = np.einsum('j,ij->i', direction, q)*inverse
    t = np.einsum('ij,ij->i', e2, q)*inverse
    valid &= (u>=-1e-7)&(v>=-1e-7)&(u+v<=1+1e-7)&(t>1e-7)&(t<1)
    if not valid.any():
        return None
    chosen = np.argmin(np.where(valid, t, np.inf))
    uv = np.asarray(mesh.uvs)[ids[chosen]].mean(0)
    return int(uv[0]*4)+4*int(uv[1]*4)


def audit(revision=None):
    path = 'assets/shared/props/elevator_70s/elevator_cabin_70s.glb'
    data = subprocess.check_output(['git', 'show', f'{revision}:{path}'], cwd=ROOT) if revision else (ROOT/path).read_bytes()
    mesh = read_glb(data)
    positions, uvs = np.asarray(mesh.positions), np.asarray(mesh.uvs)
    checked, walnut, failures = 0, 0, []
    for index, ids in enumerate(np.asarray(mesh.indices).reshape(-1, 3)):
        vertices = positions[ids]
        centre = uvs[ids].mean(0)
        cell = int(centre[0]*4)+4*int(centre[1]*4)
        walnut += cell in (0, 12)
        fold = np.all(np.abs(np.abs(vertices[:, 0])-.715)<1e-5) and np.ptp(vertices[:, 0])<1e-5 and vertices[:, 2].min()>=.0199 and vertices[:, 2].max()<=.3601 and vertices[:, 1].min()<2.2
        overhead = np.all(np.abs(vertices[:, 1]-2.205)<1e-5) and vertices[:, 0].min()>=-.7151 and vertices[:, 0].max()<=.7151 and vertices[:, 2].min()>=.0199 and vertices[:, 2].max()<=.3601
        if not (fold or overhead):
            continue
        checked += 1
        if cell != 3:
            failures.append(dict(triangle=index, atlas_cell=cell))
    wood_points = [(-.95,.4),(-.95,1.3),(-.95,2.4),(.95,.4),(.95,1.9),(.95,2.4),(-.4,2.4),(0,2.4),(.4,2.4)]
    frame_points = [(x,y) for x in (-.71,.71,-.745,.745) for y in (.4,1.3,2.1)]+[(x,2.22) for x in (-.5,0,.5)]
    wood_failures = [dict(x=x,y=y,cell=first_cell(mesh,(x,y,.8),(x,y,-.02))) for x,y in wood_points if first_cell(mesh,(x,y,.8),(x,y,-.02)) not in (0,12)]
    frame_failures = [dict(x=x,y=y,cell=first_cell(mesh,(x,y,.8),(x,y,-.02))) for x,y in frame_points if first_cell(mesh,(x,y,.8),(x,y,-.02))!=3]
    return dict(revision=revision or 'working tree', doorway_return_triangles=checked,
                nonmetal_returns=len(failures), retained_walnut_triangles=int(walnut),
                wall_probes=len(wood_points), nonwood_wall_probes=wood_failures,
                frame_probes=len(frame_points), nonmetal_frame_probes=frame_failures,
                examples=failures[:12])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--baseline')
    args = parser.parse_args()
    result = dict(current=audit())
    if args.baseline:
        result['baseline'] = audit(args.baseline)
    print(json.dumps(result, indent=2))
    assert result['current']['doorway_return_triangles']>=14
    assert result['current']['nonmetal_returns']==0, 'Non-metal doorway return'
    assert not result['current']['nonwood_wall_probes'], 'Front wall changed from walnut'
    assert not result['current']['nonmetal_frame_probes'], 'Frame is not metal'
    assert result['current']['retained_walnut_triangles']>100, 'Cabin walnut removed'


if __name__ == '__main__':
    main()
