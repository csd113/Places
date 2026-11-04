#!/usr/bin/env python3
"""Opt-in UV derivative audit for the elevator's thin fitted faces.

Whole-cell UVs squeezed across millimetre-wide faces caused neighbouring atlas
colours to appear at grazing angles. Check that fitted faces do not amplify that
anisotropy and that uniform charcoal/rubber stock uses a single committed texel.
This is a CPU/source guard, not a substitute for native animation inspection.
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
    failures = []
    checked = 0
    microscopic_weld_slivers = 0
    for role in ('cabin', 'frame', 'leaf', 'button'):
        path = f'assets/shared/props/elevator_70s/elevator_{role}_70s.glb'
        data = subprocess.check_output(['git', 'show', f'{revision}:{path}'], cwd=ROOT) if revision else (ROOT / path).read_bytes()
        mesh = read_glb(data)
        positions, uvs = np.asarray(mesh.positions), np.asarray(mesh.uvs)
        for index, ids in enumerate(np.asarray(mesh.indices).reshape(-1, 3)):
            uv = uvs[ids]
            centre = uv.mean(0)
            cell = int(centre[0] * 4) + 4 * int(centre[1] * 4)
            edge = (positions[ids][1:] - positions[ids][0]).T
            # Existing Boolean-weld slivers below 0.1 mm² have an ill-
            # conditioned derivative, rather than a visible fitted face.
            # The native sweep still inspects their actual rasterization.
            if np.linalg.norm(np.cross(edge[:, 0], edge[:, 1])) / 2 < 1e-7:
                microscopic_weld_slivers += 1
                continue
            checked += 1
            delta = (uv[1:] - uv[0]).T
            if cell in (4, 11):
                if np.max(np.abs(delta)) > 1e-7:
                    failures.append(dict(part=role, triangle=index, reason='uniform stock spans an atlas cell'))
                continue
            singular = np.linalg.svd(delta @ np.linalg.pinv(edge), compute_uv=False)
            ratio = singular[0] / max(singular[-1], 1e-10)
            if ratio > 8.1:
                failures.append(dict(part=role, triangle=index, reason='excess fitted-face UV anisotropy', ratio=float(ratio)))
    return dict(revision=revision or 'working tree', checked_triangles=checked,
                violations=len(failures), ignored_microscopic_weld_slivers=microscopic_weld_slivers,
                examples=failures[:12])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--baseline', help='Optional Git revision for comparison')
    args = parser.parse_args()
    result = dict(current=audit())
    if args.baseline:
        result['baseline'] = audit(args.baseline)
    print(json.dumps(result, indent=2))
    assert result['current']['violations'] == 0, 'Unstable fitted elevator UVs'


if __name__ == '__main__':
    main()
