#!/usr/bin/env python3
"""Audit the shipped Winter light receiver dump against an unlit control.

Both dumps must use identical geometry and quality. Requires NumPy, like the
existing lightmap dump inspector; it is an offline diagnostic only.
"""
import argparse
import json
from pathlib import Path
import numpy as np

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--on', type=Path, required=True)
parser.add_argument('--off', type=Path, required=True)
parser.add_argument('--out', type=Path, required=True)
args = parser.parse_args()
charts = json.loads((args.on/'charts.json').read_text())
control = json.loads((args.off/'charts.json').read_text())
if charts != control:
    raise SystemExit('The lighting control must preserve receiver geometry.')
offset = 0
for r in charts:
    r['offset'] = offset
    offset += r['rectangle'][2]*r['rectangle'][3]


def load(folder, stage):
    values = np.fromfile(folder/(stage+'.rgb-f32le'),dtype='<f4').reshape(-1,3)
    if len(values) != offset or not np.isfinite(values).all():
        raise SystemExit('Lighting buffers must match the chart count and be finite.')
    return values


on = load(args.on,'filled')
off = load(args.off,'filled')


def sample(values, x, y, z, kind='floor', room=None):
    for r in charts:
        if r['kind'] != kind or (room is not None and r['room'] != room): continue
        origin, u, v = (np.array(r[k]) for k in ('origin','u_axis','v_axis'))
        matrix = np.array([u,v]).T
        uv = np.linalg.lstsq(matrix,np.array([x,y,z])-origin,rcond=None)[0]
        if (np.linalg.norm(origin+matrix@uv-[x,y,z]) > .0001 or
                np.any(uv < -1e-7) or np.any(uv > 1+1e-7) or
                (r['triangle'] and sum(uv) > 1+1e-7)): continue
        w,h = r['rectangle'][2:]
        image = values[r['offset']:r['offset']+w*h].reshape(h,w,3)
        px,py = np.clip(uv,0,1)*[w-1,h-1]
        ix,iy = int(px),int(py)
        fx,fy = px-ix,py-iy
        return (image[iy,ix]*(1-fx)*(1-fy)+image[iy,min(ix+1,w-1)]*fx*(1-fy)+
                image[min(iy+1,h-1),ix]*(1-fx)*fy+image[min(iy+1,h-1),min(ix+1,w-1)]*fx*fy)
    raise ValueError(('no receiver',x,y,z,kind,room))


points = {'square_snow':(.1,0,3.8),'square_path':(.1,0,-3.6),
          'path_crossing':(.1,0,-8),'lodge_ground':(-13.5,0,-6.5),
          'cottage_ground':(12.5,0,-23.1),'forest':(.1,0,-32),
          'pond_far_shore':(17.8,0,-10)}
report = {'receiver_texels':offset,'pages':len({r['page'] for r in charts}),
          'finite':True,'points':{},'seams':{}}
for name,xyz in points.items():
    a,b = sample(on,*xyz),sample(off,*xyz)
    delta = a-b
    report['points'][name] = {'position':xyz,'on_rgb':a.tolist(),'off_rgb':b.tolist(),
                               'string_delta_rgb':delta.tolist()}
for z in (3.8,-3.6,-8):
    a,b = sample(on,-.00001,0,z),sample(on,.00001,0,z)
    report['seams'][str(z)] = {'boundary':'x=0','absolute_rgb_difference':abs(a-b).tolist()}
floors = [r for r in charts if r['kind']=='floor' and r['room'] not in (0,1,2)]
floor_values = np.concatenate([on[r['offset']:r['offset']+r['rectangle'][2]*r['rectangle'][3]] for r in floors])
report['outdoor_floor'] = {'maximum_channel':float(floor_values.max()),
    'fraction_above_1_4':float((floor_values.max(axis=1)>1.4).mean())}
for name,low,high in [('porch_snow',(-15,1.64,-7.8),(-8,1.8,-7.5)),
                      ('cottage_hood_snow',(11.6,2.37,-24),(13.4,2.6,-23.4)),
                      ('porch_rail',(-15,1.4,-7.8),(-8,1.65,-7.5)),
                      ('square_seat',(-6,0,0),(6,1,4))]:
    subsets = []
    for r in charts:
        if r['kind'] != 'prop': continue
        center=np.array(r['origin'])+(np.array(r['u_axis'])+np.array(r['v_axis']))/3
        if np.any(center<low) or np.any(center>high): continue
        n=r['rectangle'][2]*r['rectangle'][3]
        subsets.append((r['offset'],n))
    if subsets:
        delta = np.concatenate([on[i:i+n]-off[i:i+n] for i,n in subsets])
        report[name]={'charts':len(subsets),'mean_delta_rgb':delta.mean(axis=0).tolist(),
                      'peak_delta_rgb':delta.max(axis=0).tolist()}
args.out.write_text(json.dumps(report,indent=2)+'\n')
print(f'Compared {offset} receiver texels across {report["pages"]} pages; wrote {args.out}')
