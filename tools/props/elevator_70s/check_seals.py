#!/usr/bin/env python3
"""Opt-in real-GLB sightline check; never bakes a map or runs in default CI.

Casts rays through the door aperture from both sides, including millimetre
edge/seam probes and oblique eye positions. Also checks the entire clear
opening after retraction. Native screenshots/travel remain a separate check.
"""
import argparse
import itertools
import json
from pathlib import Path
import subprocess
import sys

import numpy as np

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / 'tools/props'))
from glb import read_glb


def triangles(role, revision=None):
    path = f'assets/shared/props/elevator_70s/elevator_{role}_70s.glb'
    data = subprocess.check_output(['git', 'show', f'{revision}:{path}'], cwd=ROOT) if revision else (ROOT / path).read_bytes()
    mesh = read_glb(data)
    return np.asarray(mesh.positions)[np.asarray(mesh.indices).reshape(-1, 3)]


def assembly(opened, revision, car_only=False, landing_only=False):
    panels = [triangles('cabin' if car_only else 'frame', revision)]
    leaf = triangles('leaf', revision)
    width = .675 if revision else .695
    cabin_z = .07 if revision else .30
    for z in ((.15,) if landing_only else (cabin_z,) if car_only else (cabin_z, .15)):
        for side in (-1, 1):
            travel = width if revision else .681
            x = -width-travel*opened if side < 0 else travel*opened
            if revision and side > 0:
                x += .003
            panels.append(leaf + [x, 0, z])
    return np.concatenate(panels)


def blocked(mesh, origin, target):
    """Two-sided triangle intersections on the finite sightline."""
    origin, target = np.asarray(origin), np.asarray(target)
    direction = target - origin
    a, b, c = mesh[:, 0], mesh[:, 1], mesh[:, 2]
    e1, e2 = b - a, c - a
    p = np.cross(direction, e2)
    det = np.einsum('ij,ij->i', e1, p)
    valid = np.abs(det) > 1e-10
    inverse = np.divide(1., det, out=np.zeros_like(det), where=valid)
    tvec = origin - a
    u = np.einsum('ij,ij->i', tvec, p) * inverse
    q = np.cross(tvec, e1)
    v = np.einsum('j,ij->i', direction, q) * inverse
    t = np.einsum('ij,ij->i', e2, q) * inverse
    return bool(np.any(valid & (u >= -1e-7) & (v >= -1e-7) & (u + v <= 1 + 1e-7) & (t > 1e-7) & (t < 1)))


def check(revision=None, car_only=False, landing_only=False):
    closed, opened = assembly(0, revision, car_only, landing_only), assembly(1, revision, car_only, landing_only)
    misses, checked = [], 0
    for inside in (False, True):
        cameras = itertools.product((-.5, 0, .5), (.8, 1.6, 2.05), (.82, 1., 1.5) if inside else (-.35, -1.2, -2.8))
        for camera in cameras:
            for x, y in itertools.product((-.714,-.71,-.70,-.696,-.69,-.674,-.67,-.65,-.002,0,.002,.65,.67,.674,.69,.696,.70,.71,.714), (.001, .008, .15, 1.1, 2.18, 2.195, 2.199)):
                # Continue the sightline beyond both door pairs.
                target = np.asarray((x, y, -.25 if inside else .45))
                origin = np.asarray(camera)
                entry_z = .36 if inside else -.17
                entry = origin + (target - origin) * ((entry_z - origin[2]) / (target[2] - origin[2]))
                if abs(entry[0]) > (.675 if revision else .715) or not 0 < entry[1] < 2.2:
                    continue
                checked += 1
                if not blocked(closed, origin, target):
                    misses.append(dict(inside=inside, eye=camera, target=target.tolist()))
    obstructed = []
    for x, y in itertools.product(np.linspace(-.674, .674, 31), (.015, .025, 1.1, 2.19, 2.199)):
        if blocked(opened, (x,y,-.4), (x,y,.5)):
            obstructed.append((float(x), y))
    return dict(revision=revision or 'working tree', closed_rays=checked, leaks=len(misses), examples=misses[:8], open_rays=155, obstructed_open_rays=len(obstructed))


def visible_edges():
    body=np.concatenate([triangles('cabin'),triangles('frame')])
    leaf=triangles('leaf')
    for side in (-1,1):
        pose=-.695-.681 if side<0 else .681
        hall_x=.702*side;car_x=.684*side
        outside=(hall_x,1.1,-.5);inside=(car_x,1.1,.8)
        assert blocked(leaf+[pose,0,.15],outside,(hall_x,1.1,.128))
        assert not blocked(body,outside,(hall_x,1.1,.128))
        assert blocked(leaf+[pose,0,.30],inside,(car_x,1.1,.3224))
        assert not blocked(body,inside,(car_x,1.1,.3224))
    return 'Both full-open panel lips visible from cabin and hall; clear aperture checked separately'

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--baseline', help='Optional Git revision to demonstrate the original leak')
    args = parser.parse_args()
    result = dict(current=check(), moving_cabin=check(car_only=True), landing_only=check(landing_only=True))
    if args.baseline:
        result['baseline'] = check(args.baseline)
    result['edge_visibility']=visible_edges()
    print(json.dumps(result, indent=2))
    assert result['landing_only']['leaks'] == 0, 'Closed landing reveals the shaft'
    assert result['moving_cabin']['leaks'] == 0, 'Moving cabin has unblocked sightlines'
    assert result['current']['leaks'] == 0, 'Closed portal has unblocked sightlines'
    assert result['current']['obstructed_open_rays'] == 0, 'Retracted leaves block the clear opening'


if __name__ == '__main__':
    main()
