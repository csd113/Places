#!/usr/bin/env python3
"""Opt-in real-GLB cabin shell and floor-corner seam audit, not a map bake."""
import argparse
import json
import numpy as np
from check_seals import triangles, blocked

def audit(revision=None):
    mesh=triangles('cabin',revision)
    points=[]
    for side in (-1,1):
        for z in (.365,.4,.6,1.2,2.0,2.38,2.395):
            for y in (.001,.008,.024,.04,.129,.14,1.,2.46,2.48,2.54):
                points.append((side*1.2,y,z))
    for x in (-1.195,-1.18,-.8,0,.8,1.18,1.195):
        for y in (.001,.008,.024,.04,.129,.14,1.,2.46,2.48,2.54):
            points.append((x,y,2.4))
    for x in (-1.195,-.8,0,.8,1.195):
        for z in (.365,.4,1.2,2.38,2.395):
            points.extend(((x,0,z),(x,2.55,z)))
    # Front wall and its two corners; omit the intentional door aperture.
    for x in (-1.195,-1.18,-.8,.8,1.18,1.195):
        for y in (.001,.008,.024,.14,1.,2.46,2.54):
            points.append((x,y,.36))
    leaks=[]
    for point in points:
        eye=np.asarray((0,1.55,1.2));target=np.asarray(point)
        beyond=target+(target-eye)*.2
        if not blocked(mesh,eye,beyond):leaks.append(point)
    # Direct close horizontal rays catch the 25 mm gap under raised skirting,
    # even where a distant view would intersect the floor before the wall.
    for side in (-1,1):
        for z in (.4,1.2,2.38):
            for y in (.001,.008,.024):
                if not blocked(mesh,(side*1.1,y,z),(side*1.205,y,z)):leaks.append((side*1.2,y,z))
    for x in (-1.18,0,1.18):
        for y in (.001,.008,.024):
            if not blocked(mesh,(x,y,2.3),(x,y,2.405)):leaks.append((x,y,2.4))
    return dict(revision=revision or 'working tree',shell_probes=len(points),floor_join_probes=27,leaks=len(leaks),examples=leaks[:20])
if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--baseline');args=p.parse_args()
    result=dict(current=audit())
    if args.baseline:result['baseline']=audit(args.baseline)
    print(json.dumps(result,indent=2));assert not result['current']['leaks'], 'Cabin corner gap'
