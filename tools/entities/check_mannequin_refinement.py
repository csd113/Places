#!/usr/bin/env python3
"""Explicit offline contract/stride review; never runs as an ordinary Rust test.

Usage: python3 tools/entities/check_mannequin_refinement.py BEFORE.glb AFTER.glb
This bounds the changed asset's precise contracts and samples six short gaits;
no map bake, renderer or runtime IK is involved.
"""
from pathlib import Path
import hashlib
import json
import math
import sys
from validate_entities import Model
from check_clip_boundaries import clip_is_looped, compare, endpoints

before, after = [Model(Path(p)) for p in sys.argv[1:]]
changed = {'walk','carry_walk','crowbar_walk','run','run_arms_forward','crowbar_run'}
changed_nodes = {'root','pelvis','thigh.L','shin.L','foot.L','thigh.R','shin.R','foot.R'}
for key in ('nodes','skins','materials','textures','images','samplers','scenes','scene'):
    assert before.document.get(key) == after.document.get(key), key
assert after.binary[:len(before.binary)] == before.binary
assert len(before.animations) == len(after.animations) == 25
assert before.joints == after.joints and len(after.joints) == 37
assert before.indices == after.indices
assert before.vertex_joints == after.vertex_joints
assert before.vertex_weights == after.vertex_weights
# The held first-person arm exporter selects these exact right-side chains.
for old,new,js,ws in zip(before.positions,after.positions,after.vertex_joints,after.vertex_weights):
    names=[after.nodes[after.joints[j]]['name'] for j,w in zip(js,ws) if w>0]
    if any(name.endswith('.R') and name not in {'upper_arm.R','thigh.R','shin.R','foot.R'} for name in names):
        assert old==new,('first-person arm position',names)
for old,new in zip(before.document['meshes'][0]['primitives'],after.document['meshes'][0]['primitives']):
    assert old['material'] == new['material']
    for attr in ('TEXCOORD_0','JOINTS_0','WEIGHTS_0'):
        assert old['attributes'][attr] == new['attributes'][attr],attr
for old,new in zip(before.animations,after.animations):
    assert old['name'] == new['name']
    if old['name'] not in changed:
        assert old == new,old['name']
    else:
        for old_ch,new_ch in zip(old['channels'],new['channels']):
            assert old_ch['target'] == new_ch['target']
            name=after.nodes[new_ch['target']['node']]['name']
            if name not in changed_nodes:
                assert old_ch == new_ch,(old['name'],name)
    if clip_is_looped(after.document,new['name']):
        compare(after,endpoints(after,new,False),endpoints(after,new,True),new['name'])

def distance(a,b):
    return math.sqrt(sum((x-y)**2 for x,y in zip(a,b)))

def knee_flex(hip,knee,ankle):
    a=[h-k for h,k in zip(hip,knee)];b=[f-k for f,k in zip(ankle,knee)]
    cosine=sum(x*y for x,y in zip(a,b))/(distance(hip,knee)*distance(ankle,knee))
    return 180-math.degrees(math.acos(max(-1,min(1,cosine))))

report={'before_sha256':hashlib.sha256(before.path.read_bytes()).hexdigest(),
        'after_sha256':hashlib.sha256(after.path.read_bytes()).hexdigest(),
        'preserved_contracts':['37 joints and rest transforms','25 clip names','19 unrelated clips',
        'all local upper body channels','primitive order/materials','triangle indices and weights',
        'fitted UVs and exact embedded PNG bytes','original BIN payload'], 'gaits':{}}
for name in sorted(changed):
    results={}
    for version,model in [('before',before),('after',after)]:
        ids={n['name']:i for i,n in enumerate(model.nodes)}
        clip=next(c for c in model.clips if c['name']==name)
        knees=[];feet=[];contact=[];hips=[];roots=[]
        foot_vertices={side:[i for i,(js,ws) in enumerate(zip(model.vertex_joints,model.vertex_weights))
            if any(model.joints[j]==ids[f'foot.{side}'] and w>.99 for j,w in zip(js,ws))] for side in ('L','R')}
        for frame in range(121):
            phase=frame/120
            g=model._pose_globals(clip,phase*clip['duration'])
            vertices=model._skin(g)
            roots.append(g[ids['root']][12:15])
            hips.extend([g[ids['thigh.L']][13],g[ids['thigh.R']][13]])
            contact.append(min(v[1] for v in vertices))
            for side in ('L','R'):
                hip,knee,ankle=[g[ids[f'{part}.{side}']][12:15] for part in ('thigh','shin','foot')]
                knees.append(knee_flex(hip,knee,ankle))
                feet.append(min(vertices[i][1] for i in foot_vertices[side]))
        results[version]={'max_knee_flex_degrees':max(knees),'min_floor_m':min(contact),
            'max_lowest_body_point_m':max(contact),'max_foot_sole_lift_m':max(feet),
            'min_hip_y_m':min(hips),'max_hip_y_m':max(hips),
            'max_absolute_root_translation_m':max(abs(v) for xyz in roots for v in xyz),'samples':121}
        if version=='after':
            assert min(contact)>-.003,(name,min(contact))
            # The faster run's longer recovery uses the smooth reach limit.
            # Its hip minimum must improve on the source run; walk retains
            # its independently reviewed 0.79 m lower bound.
            hip_limit=results['before']['min_hip_y_m'] if 'run' in name else .79
            assert min(hips)>hip_limit,(name,'excessive pelvis lowering',min(hips),hip_limit)
            assert clip['reference_speed_mps']==(3.2 if 'run' in name else 1),name
            assert max(abs(v) for xyz in roots for v in xyz)<1e-6,(name,'stacked root motion')
            # Stance ankle velocity cancels the catalog reference speed; heel
            # and toe rolls are excluded so this measures flat planted contact.
            speed=3.2 if 'run' in name else 1
            duty=.30 if 'run' in name else .62
            p0,p1=duty*.25,duty*.65
            a=model._pose_globals(clip,p0*clip['duration'])[ids['foot.L']][14]
            b=model._pose_globals(clip,p1*clip['duration'])[ids['foot.L']][14]
            drift=abs((b-a)+speed*(p1-p0)*clip['duration'])
            results[version]['midstance_forward_drift_m']=drift
            assert drift<.0001,(name,drift)
    report['gaits'][name]=results
print(json.dumps(report,indent=2))
