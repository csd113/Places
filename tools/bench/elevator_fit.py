#!/usr/bin/env python3
"""Opt-in direct editor wall-fit/space campaign; tiny sources, no automatic bake.

Four cardinal walls receive elevators by eye-level wall clicks. The same pointer
workflow refuses a short room gap and accepts a deeper wall by hollowing its cab volume. Coordinates deliberately
miss the world grid. A complete saved project remains available for an explicit export.
"""
import argparse
import json
import math
from pathlib import Path
import subprocess

ROOT=Path(__file__).resolve().parents[2]
def save(path,value):path.write_text(json.dumps(value,indent=2)+'\n')
def author(out):
    out.mkdir(parents=True,exist_ok=False);project=out/'project';(project/'levels').mkdir(parents=True)
    level=dict(schema='places-level',format_version=1,id='elevator_fit',name='Elevator automatic wall fit',spawn=dict(x=0,z=-2,yaw_degrees=180),defaults=dict(wall='home:wall_paint_offwhite_01',floor='core:carpet_beige_01',ceiling='core:ceiling_panel_01'),rooms=[],walls=[],ceiling_lights=[])
    cases=[];steps=[]
    def add(kind,**kw):steps.append(dict(delay_ms=150,action=dict(kind=kind,**kw)))
    add('wait_idle');add('resize',width=1280,height=720)
    for i,rotation in enumerate((0,90,180,270,0,0)):
        ox=i*12;oz=.137;rad=math.radians(rotation);sn=round(math.sin(rad));cs=round(math.cos(rad))
        def point(x,y,z):return [ox+cs*x+sn*z,y,oz-sn*x+cs*z]
        def rectangle(x,z,w,d):
            corners=[point(a,0,b) for a in (x,x+w) for b in (z,z+d)]
            xs=[p[0] for p in corners];zs=[p[2] for p in corners]
            return dict(x=min(xs),z=min(zs),width=max(xs)-min(xs),depth=max(zs)-min(zs))
        gap=2.47 if i==4 else 2.48
        level['rooms'].extend([dict(**rectangle(-3,-5,6,5),height=2.8),dict(**rectangle(-3,gap,6,5),height=2.8)])
        level['walls'].append(dict(**rectangle(-3,0,6,(.10,.20,.24,.20,.20,.35)[i]),height=2.8))
        level['ceiling_lights'].append(dict(id=f'fit_light_{i}',fixture='home:ceiling_light_round',x=point(0,0,-2)[0],z=point(0,0,-2)[2],y=2.76,brightness=1.1))
        cases.append(dict(origin=point(.25,0,0),rotation=rotation,accepted=i!=4))
        add('control',action='Elevator(Place)');add('camera',eye=point(0,1.6,-3.5),yaw=180-rotation,pitch=0)
        # Horizontal ray directly at wall, no floor ray and no coordinate fields.
        target=point(.17,1.6,0)
        add('world_pointer',position=target);add('capture',name=f'{i}-preview');add('checkpoint',name=f'{i}-preview')
        add('world_click',position=target);add('wait_idle');add('capture',name=f'{i}-result');add('checkpoint',name=f'{i}-result')
        add('key',key='Escape')
    add('control',action='Save');add('checkpoint',name='saved');add('close')
    save(project/'levels/elevator_fit.json',level)
    save(project/'places.project.json',dict(schema='places-project',project_format=1,id='elevator_fit',name='Elevator fit acceptance',entry_level='elevator_fit',menu_entry_only=True,levels=[dict(id='elevator_fit',source='levels/elevator_fit.json')]))
    script=out/'editor-actions.json';save(script,dict(output=str(out/'captures'),actions=steps));save(out/'manifest.json',dict(cases=cases,scope='Actual native editor pointer previews and eye-level wall clicks; no coordinate form editing.'))
    return project,script,cases

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--out',type=Path,required=True);p.add_argument('--editor',type=Path,required=True);p.add_argument('--compiler',type=Path,required=True);a=p.parse_args()
    out=a.out.resolve();project,script,cases=author(out)
    with (out/'editor.log').open('w') as log:
        subprocess.run([str(a.editor.resolve()),str(project),'--asset-root',str(ROOT/'assets'),'--compiler',str(a.compiler.resolve()),'--verify-script',str(script)],stdout=log,stderr=subprocess.STDOUT,check=True,timeout=120)
    count=0
    for i,case in enumerate(cases):
        result=json.loads((out/f'captures/{i}-result.json').read_text());lifts=result['document'].get('elevators',[])
        if case['accepted']:
            count+=1
            assert len(lifts)==count,(i,result['status'])
            lift=lifts[-1];assert abs(lift['x']-case['origin'][0])<.001 and abs(lift['z']-case['origin'][2])<.001,(i,lift)
            assert abs(lift['rotation_degrees']-case['rotation'])<.001,(i,lift)
            if i<4:
                opening=result['document']['walls'][i]['openings'][0];assert abs(opening['width']-1.5)<.001 and abs(opening['height']-2.25)<.001
        else:assert len(lifts)==count and 'Not enough space' in result['status'],result['status']
    saved=json.loads((out/'captures/saved.json').read_text());assert not saved['dirty']
    save(out/'passed.json',dict(cardinal_wall_faces=4,refused_short_gap=True,accepted_thick_wall=True,eye_level_wall_clicks=True))
    print('Native elevator wall-fit and clearance passed')
if __name__=='__main__':main()
