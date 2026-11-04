#!/usr/bin/env python3
"""Opt-in coplanar-face audit of the actual assembled elevator GLBs.

Find overlapping axis-aligned triangle faces, then test whether their shared
area can be seen from cabin/hall eye positions through all other real model
triangles. Hidden internal box contacts are allowed. Checks five door poses;
native motion inspection is still required. No bake or automatic CI involved.
"""
import json
from collections import defaultdict
import numpy as np
from check_seals import triangles, assembly, blocked


def cross(a,b):return a[0]*b[1]-a[1]*b[0]


def intersect(subject,clip):
    polygon=[p.copy() for p in subject]
    sign=1 if cross(clip[1]-clip[0],clip[2]-clip[0])>=0 else -1
    for a,b in zip(clip,np.roll(clip,-1,axis=0)):
        output=[]
        for p,q in zip(polygon,polygon[1:]+polygon[:1]):
            dp,dq=sign*cross(b-a,p-a),sign*cross(b-a,q-a)
            if dp>=-1e-10:output.append(p)
            if (dp>0)!=(dq>0) and abs(dp-dq)>1e-12:output.append(p+(q-p)*dp/(dp-dq))
        polygon=output
        if not polygon:return None
    return np.asarray(polygon)


def audit(opening):
    body=triangles('cabin')
    modules=[triangles('button')+[0,-row*.11,0] for row in range(3)]
    mesh=np.concatenate([body,assembly(opening,None),*modules])
    groups=defaultdict(list)
    for i,tri in enumerate(mesh):
        spans=np.ptp(tri,axis=0)
        for axis in range(3):
            if spans[axis]<1e-7 and np.linalg.norm(np.cross(tri[1]-tri[0],tri[2]-tri[0]))>1e-10:
                groups[(axis,round(float(tri[0,axis]),5))].append(i)
                break
    candidates=[]
    for (axis,coordinate),indices in groups.items():
        dims=[a for a in range(3) if a!=axis]
        for slot,i in enumerate(indices):
            first=mesh[i][:,dims]
            for j in indices[slot+1:]:
                second=mesh[j][:,dims]
                if np.any(np.minimum(first.max(0),second.max(0))-np.maximum(first.min(0),second.min(0))<=1e-7):continue
                polygon=intersect(first,second)
                if polygon is None:continue
                area=abs(sum(cross(p,q) for p,q in zip(polygon,np.roll(polygon,-1,axis=0))))/2
                if area<1e-7:continue
                point=np.zeros(3);point[axis]=coordinate;point[dims]=polygon.mean(0)
                candidates.append((i,j,area,point))
    visible=[]
    cameras=[(0,1.6,1.2),(-.6,1.6,.85),(.6,1.6,.85),(-.7,1.6,1.9),(.7,2.1,1.9),(0,.7,1.2),(0,2.1,1.2),
             (0,1.6,-2),(-1.4,1.6,-1),(1.4,1.6,-1),(0,.7,-1),(0,2.1,-1)]
    for i,j,area,point in candidates:
        rest=np.delete(mesh,[i,j],axis=0)
        for eye in cameras:
            # Stop just before the shared plane so its other triangulated half
            # is not mistaken for a nearer occluder.
            origin=np.asarray(eye);target=point+(origin-point)*1e-5
            if not blocked(rest,origin,target):
                visible.append(dict(triangles=[i,j],area_m2=area,point=point.tolist(),eye=eye))
                break
    return dict(opening=opening,coplanar_contacts=len(candidates),visible_overlaps=visible)


if __name__=='__main__':
    result=[audit(opening) for opening in (0,.25,.5,.75,1)]
    print(json.dumps(result,indent=2))
    assert all(not case['visible_overlaps'] for case in result),'Exposed coplanar elevator faces'
