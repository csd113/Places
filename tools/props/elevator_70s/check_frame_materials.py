#!/usr/bin/env python3
"""Opt-in actual-GLB material check for the elevator's doorway returns.

The exposed pocket folds and overhead underside must use metal atlas cells.
Side and rear cabin wall panels retain walnut. No map bake or native window.
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
        fold = np.all(np.abs(np.abs(vertices[:, 0])-.715)<1e-5) and np.ptp(vertices[:, 0])<1e-5 and vertices[:, 2].min()>=.0199 and vertices[:, 2].max()<=.3601
        overhead = np.all(np.abs(vertices[:, 1]-2.205)<1e-5) and vertices[:, 0].min()>=-.7151 and vertices[:, 0].max()<=.7151 and vertices[:, 2].min()>=.0199 and vertices[:, 2].max()<=.3601
        fascia = np.all(np.abs(vertices[:, 2]-.36)<1e-5) and vertices[:, 0].min()>=-1.4251 and vertices[:, 0].max()<=1.4251 and np.min(np.abs(vertices[:, 0]))>=.7149
        if not (fold or overhead or fascia):
            continue
        checked += 1
        if cell != 3:
            failures.append(dict(triangle=index, atlas_cell=cell))
    return dict(revision=revision or 'working tree', doorway_return_triangles=checked,
                nonmetal_returns=len(failures), retained_walnut_triangles=int(walnut), examples=failures[:12])


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
    assert result['current']['retained_walnut_triangles']>100, 'Cabin walnut removed'


if __name__ == '__main__':
    main()
