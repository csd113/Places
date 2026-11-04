#!/usr/bin/env python3
"""Opt-in native host hollowing campaign; tiny source fixture, no CI bake."""
import argparse
import json
import math
import os
from pathlib import Path
import subprocess
ROOT=Path(__file__).resolve().parents[2]
def save(path,value):path.write_text(json.dumps(value,indent=2)+'\n')
def author(out):
    out.mkdir(parents=True,exist_ok=False);project=out/'project';(project/'levels').mkdir(parents=True)
    level=dict(schema='places-level',format_version=1,id='elevator_embedding',name='Elevator host clearance',spawn=dict(x=0,z=-2,yaw_degrees=180),defaults=dict(wall='home:wall_paint_offwhite_01',floor='core:carpet_beige_01',ceiling='core:ceiling_panel_01'),rooms=[],walls=[],columns=[],pillars=[],ceiling_lights=[])
    cases=[];steps=[]
    def add(kind,**kw):steps.append(dict(delay_ms=150,action=dict(kind=kind,**kw)))
    add('wait_idle');add('resize',width=1280,height=720)
    configurations=[('wall',0,.35,True),('wall',90,.60,True),('wall',180,.20,True),('wall',270,.10,True),('column',0,3.0,True),('column',90,3.0,True),('round',180,3.0,True),('round',270,3.0,True),('column',0,2.4,False),('round',0,1.0,False),('gap',0,.60,False)]
    for i,(kind,rotation,size,accepted) in enumerate(configurations):
        ox=i*22;oz=.137;rad=math.radians(rotation);sn=round(math.sin(rad));cs=round(math.cos(rad))
        def point(x,y,z):return [ox+cs*x+sn*z,y,oz-sn*x+cs*z]
        def rectangle(x,z,w,d):
            corners=[point(a,0,b) for a in (x,x+w) for b in (z,z+d)];xs=[p[0] for p in corners];zs=[p[2] for p in corners]
            return dict(x=min(xs),z=min(zs),width=max(xs)-min(xs),depth=max(zs)-min(zs))
        shift=0.0
        if kind in ('wall','gap'):
            gap=2.47 if kind=='gap' else 2.48
            level['rooms'].extend([dict(**rectangle(-3,-5,6,5),height=3.2),dict(**rectangle(-3,gap,6,5),height=3.2)])
            level['walls'].append(dict(**rectangle(-3,0,6,size),height=3.2))
        else:
            level['rooms'].append(dict(**rectangle(-8,-5,16,14),height=3.2))
            if kind=='column':level['columns'].append(dict(**rectangle(-1.7 if accepted else -1.4,0,3.4 if accepted else 2.8,size),height=3.2,material='home:wall_paint_offwhite_01'))
            else:
                centre=point(0,0,size)
                level['pillars'].append(dict(x=centre[0],z=centre[2],radius=size,height=3.2,segments=24,material='home:wall_paint_offwhite_01'))
                if accepted:shift=size-size*math.cos(math.pi/4)
        light=point(0,0,-2)
        level['ceiling_lights'].append(dict(id=f'host_light_{i}',fixture='home:ceiling_light_round',x=light[0],z=light[2],y=3.16,brightness=1.1))
        cases.append(dict(kind=kind,origin=point((.05 if rotation==0 else -.05) if kind=='column' and accepted else 0,0,shift),rotation=rotation,accepted=accepted))
        add('control',action='Elevator(Place)');add('camera',eye=point(0,1.6,-3.5),yaw=180-rotation,pitch=0)
        target=point(0,1.6,0)
        add('world_pointer',position=target);add('capture',name=f'{i}-preview');add('world_click',position=target);add('wait_idle');add('capture',name=f'{i}-result');add('checkpoint',name=f'{i}-result');add('key',key='Escape')
    add('control',action='Save');add('checkpoint',name='saved');add('close')
    save(project/'levels/elevator_embedding.json',level)
    save(project/'places.project.json',dict(schema='places-project',project_format=1,id='elevator_embedding',name='Elevator host acceptance',entry_level='elevator_embedding',menu_entry_only=True,levels=[dict(id='elevator_embedding',source='levels/elevator_embedding.json')]))
    script=out/'editor-actions.json';save(script,dict(output=str(out/'captures'),actions=steps));save(out/'manifest.json',dict(cases=cases));return project,script,cases

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--out',type=Path,required=True);p.add_argument('--editor',type=Path,required=True);p.add_argument('--compiler',type=Path,required=True);a=p.parse_args()
    out=a.out.resolve();project,script,cases=author(out)
    env=os.environ.copy();env['PLACES_STATE_ROOT']=str(out/'state')
    with (out/'editor.log').open('w') as log:subprocess.run([str(a.editor.resolve()),str(project),'--asset-root',str(ROOT/'assets'),'--compiler',str(a.compiler.resolve()),'--verify-script',str(script)],stdout=log,stderr=subprocess.STDOUT,check=True,timeout=150,env=env)
    count=0
    for i,case in enumerate(cases):
        result=json.loads((out/f'captures/{i}-result.json').read_text());lifts=result['document'].get('elevators',[])
        if case['accepted']:
            count+=1;assert len(lifts)==count,(i,result['status']);lift=lifts[-1]
            assert abs(lift['x']-case['origin'][0])<.001 and abs(lift['z']-case['origin'][2])<.001,(i,lift,case)
            assert abs(lift['rotation_degrees']-case['rotation'])<.001,(i,lift)
        else:assert len(lifts)==count and 'Not enough space' in result['status'],(i,result['status'])
    assert not json.loads((out/'captures/saved.json').read_text())['dirty']
    save(out/'passed.json',dict(accepted=count,normal_and_thick_walls=4,rectangular_pillars=2,circular_pillars=2,refused_small_pillars=2,refused_short_room_gap=True))
    print('Native elevator pillar and room clearance passed')
if __name__=='__main__':main()
