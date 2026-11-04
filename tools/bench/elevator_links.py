#!/usr/bin/env python3
"""Author opt-in native editor acceptance fixtures for simple linked elevators.

Creates disposable sources and bounded native editor scripts; it does not build
Rust, bake a map, launch an app bundle or alter an installed game. `--run` uses
explicit direct binaries. Groups includes same-height rooms, two adjacent
four-storey structures and an isolated second network. Demo records the final
simple tutorial workflow, saving/exporting the complete project.
"""
import argparse
import copy
import json
from pathlib import Path
import shutil
import subprocess

ROOT=Path(__file__).resolve().parents[2]


def save(path,value):path.write_text(json.dumps(value,indent=2)+'\n')


def hall(x,y,material='core:carpet_beige_01'):
    room=dict(x=x-3,z=-5,width=6,depth=5,floor_y=y,height=2.8,material=material)
    walls=[dict(x=x-3.2,z=-5.2,y=y,width=6.4,depth=.2,height=2.8),dict(x=x-3.2,z=-5,y=y,width=.2,depth=5.2,height=2.8),dict(x=x+3,z=-5,y=y,width=.2,depth=5.2,height=2.8),dict(x=x-3,z=0,y=y,width=6,depth=.2,height=2.8,openings=[dict(kind='door',offset=2.25,width=1.5,height=2.25,sill=0)])]
    light=dict(id=f'hall_{x}_{y}'.replace('.','_'),fixture='home:ceiling_light_round',x=x,z=-2.2,y=y+2.76,brightness=1.1)
    return room,walls,light


def author(args):
    out=args.out.resolve();out.mkdir(parents=True,exist_ok=False);project=out/'project';(project/'levels').mkdir(parents=True)
    if args.mode=='demo':
        shutil.copytree(ROOT/'projects/elevator-links',project,dirs_exist_ok=True)
        main=project/'levels/elevator_lobby.json';v=json.loads(main.read_text());v.pop('elevators');v.pop('transitions');save(main,v)
        annex=project/'levels/elevator_annex.json';v=json.loads(annex.read_text());v['elevators'][0]['stops']=v['elevators'][0]['stops'][:1];v['transitions']=[];save(annex,v)
        placements=[(0,0,'G / Ground lobby'),(10,0,'1 / Upper office')]
    else:
        level=dict(schema='places-level',format_version=1,id='elevator_groups',name='Temporary linked elevator groups',saving=True,spawn=dict(x=0,z=-2,yaw_degrees=180),defaults=dict(wall='home:wall_paint_offwhite_01',floor='core:carpet_beige_01',ceiling='core:ceiling_panel_01'),rooms=[],walls=[],ceiling_lights=[])
        for x,heights in [(0,[0,3.5,7,10.5]),(10,[0,3.5,7,10.5]),(20,[0,3.5])]:
            for y in heights:
                room,walls,light=hall(x,y,'backrooms:carpet_cool_01' if x==10 else 'core:carpet_beige_01')
                level['rooms'].append(room);level['walls'].extend(walls);level['ceiling_lights'].append(light)
        save(project/'levels/elevator_groups.json',level)
        save(project/'places.project.json',dict(schema='places-project',project_format=1,id='elevator_groups',name='TEMPORARY linked elevator groups',entry_level='elevator_groups',menu_entry_only=True,levels=[dict(id='elevator_groups',source='levels/elevator_groups.json')]))
        placements=[(0,0,'1 / Ground'),(10,0,'4 / Adjacent hall'),(10,7,'8 / Upper gallery'),(20,0,'A / Other group'),(20,3.5,'B / Other group')]
    steps=[]
    def add(kind,**kw):steps.append(dict(delay_ms=110,action=dict(kind=kind,**kw)))
    def control(action):add('control',action=action)
    def capture(name):
        add('wait_idle');add('capture',name=name);add('checkpoint',name=name)
    def camera(x,y):add('camera',eye=[x,y+1.9,-3.5],yaw=180,pitch=-25)
    def name(text):
        control('Elevator(Name)');add('key',key='A',command=True);add('text',text=text);add('key',key='Return')
    add('wait_idle');add('resize',width=1280,height=720);capture('00-build-elevator')
    for i,(x,y,label) in enumerate(placements):
        control('Elevator(Place)');camera(x,y);add('world_pointer',position=[x,y,0]);
        if i==0:capture('01-click-placement-preview')
        add('world_click',position=[x,y,0]);name(label);capture(f'02-placed-{i}')
    for source,target in ([(0,1)] if args.mode=='demo' else [(0,1),(0,2),(3,4)]):
        control(f'Elevator(Select({source}))');control('Elevator(Link)');x,y,_=placements[target];camera(x,y);add('world_click',position=[x,y+1.1,.1]);capture(f'03-linked-{source}-{target}')
    add('checkpoint',name='linked')
    if args.mode=='groups':
        control('Elevator(Select(0))');control('Elevator(Link)');camera(20,0);add('world_click',position=[20,1.1,.1]);add('checkpoint',name='limit-refused');capture('04-limit-explained');add('key',key='Escape');control('Save');add('checkpoint',name='saved');add('close')
    else:
        for source in (0,1):
            control(f'Elevator(Select({source}))');control('Elevator(Level)');capture(f'04-level-destination-{source}');control('Elevator(ApplyLevel)')
        control('ClosePopup');control('Save');add('checkpoint',name='saved-local-and-level');control('Project');control('ProjectSelect(1)');control('ClosePopup')
        control('Elevator(Place)');camera(0,0);add('world_click',position=[0,1.1,.1])
        for other in (0,1):
            control('Elevator(Level)')
            if other:control('Elevator(NextArrival)')
            capture(f'05-annex-return-{other}');control('Elevator(ApplyLevel)')
        control('ClosePopup');control('Save');add('checkpoint',name='saved-annex');control('Project');capture('06-export-all-levels');control('ProjectField(Export)');add('key',key='A',command=True);add('text',text=str(out/'tutorial-demo.placesmap'));capture('07-export-path');control('ProjectApply');add('wait_idle');capture('08-exported');add('checkpoint',name='exported');add('close')
    script=out/'editor-actions.json';save(script,dict(output=str(out/'captures'),actions=steps))
    save(out/'manifest.json',dict(mode=args.mode,project=str(project),script=str(script),placements=placements,scope='Direct native editor controls and viewport clicks; no source-form coordinate entry or JSON editing.'))
    return out,project,script


def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--out',type=Path,required=True);parser.add_argument('--mode',choices=['groups','demo'],default='groups');parser.add_argument('--run',action='store_true');parser.add_argument('--editor',type=Path,default=ROOT/'target/debug/places-editor');parser.add_argument('--compiler',type=Path,default=ROOT/'target/debug/places-compile');args=parser.parse_args()
    out,project,script=author(args)
    if args.run:
        with (out/'editor.log').open('w') as log:
            subprocess.run([str(args.editor.resolve()),str(project),'--asset-root',str(ROOT/'assets'),'--compiler',str(args.compiler.resolve()),'--verify-script',str(script)],stdout=log,stderr=subprocess.STDOUT,check=True,timeout=180)
        linked=json.loads((out/'captures/linked.json').read_text());assert len(linked['document']['elevators'])==len(json.loads((out/'manifest.json').read_text())['placements'])
        if args.mode=='groups':
            refused=json.loads((out/'captures/limit-refused.json').read_text());assert refused['document']==linked['document'];assert 'three stops' in refused['status']
            saved=json.loads((out/'captures/saved.json').read_text());assert not saved['dirty'];assert [len(e['stops']) for e in saved['document']['elevators']]==[3,3,3,2,2]
        else:
            exported=json.loads((out/'captures/exported.json').read_text());assert not exported['dirty'];assert 'Exported complete project' in exported['status']
        print('Native simple elevator authoring passed:',args.mode)
    else:print(project,script)


if __name__=='__main__':main()
