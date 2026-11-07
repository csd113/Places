#!/usr/bin/env python3
"""Create the small explicit exit-door acceptance project, never a shipped map."""
from pathlib import Path
import argparse
import json
import shutil
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--out', type=Path, required=True, help='fresh private evidence directory')
args = parser.parse_args()
R = args.out.resolve()
REPO = Path(__file__).resolve().parents[2]
if (R/'private-project').exists():
 parser.error('private-project already exists; choose a fresh evidence directory')
P=R/'private-project';(P/'levels').mkdir(parents=True,exist_ok=True);(P/'scripts').mkdir(exist_ok=True)
def write(p,d):p.parent.mkdir(parents=True,exist_ok=True);p.write_text(json.dumps(d,indent=2)+'\n')
write(P/'places.project.json',dict(schema='places-project',project_format=1,id='exit_door_acceptance',name='Private exit-door acceptance',package_version='1.0.0',entry_level='exit_doors',levels=[dict(id='exit_doors',source='levels/exit_doors.json')]))
props=[]
for v,x in [('single',-2),('double',2)]:
 d=json.loads((REPO/f'assets/shared/doors/hollow_metal_exit_door_{v}.json').read_text());d.update(id=v,x=x);props.append(d)
props.append(dict(id='walker',model='mannequin',x=2,y=0,z=2.5,solid=False,components=[dict(component='nav_agent',radius=.22,height=1.8,speed_mps=1.1,step_height=.3,max_slope=2),dict(component='ai',behavior='idler',walk_speed=1.1,run_speed=1.1,sight_range=0,hearing_range=0)]))
controls=[]
for name,actions in [('open_all',[dict(action='open',target=t)for t in ['single','double']]),('close_all',[dict(action='close',target=t)for t in ['single','double']]),('lock_single',[dict(action='lock',target='single')]),('unlock_single',[dict(action='unlock',target='single')]),('reset',[dict(action='reset_to_start')])]:
 props.append(dict(id=name,model='core:rubber_duck',x=3.6,z=3.6,y=0,scale=.2,solid=False,components=[dict(component='interactable',prompt='Feature control',reach=2)],bindings=[dict(on='interact',actions=actions)]))
level=dict(schema='places-level',format_version=1,id='exit_doors',name='Private exit-door acceptance',author='Places feature acceptance',comment='Bounded disposable feature fixture. Not final duck-map construction.',spawn=dict(x=-1.965,z=1.65,yaw_degrees=0),defaults=dict(wall='home:wall_paint_offwhite_01',floor='arcade:lane_wood_01',ceiling='core:ceiling_panel_01'),rooms=[dict(x=-4,z=-3,width=8,depth=8,height=3)],walls=[dict(x=-4,z=-.08,width=8,depth=.16,height=3,openings=[dict(kind='passage',offset=1.5,width=1.0,height=2.15),dict(kind='passage',offset=5.05,width=1.9,height=2.15)]),dict(x=-4.16,z=-3.16,width=8.32,depth=.16,height=3),dict(x=-4.16,z=5,width=8.32,depth=.16,height=3),dict(x=-4.16,z=-3,width=.16,depth=8,height=3),dict(x=4,z=-3,width=.16,depth=8,height=3)],ceiling_lights=[dict(id='front_light',fixture='core:fluorescent_panel_01',x=0,z=2.4,y=2.99,brightness=1.6,range=8,align='none',switchable=True),dict(id='rear_light',fixture='core:fluorescent_panel_01',x=0,z=-1.8,y=2.99,brightness=1.3,range=8,align='none',switchable=True)],props=props,volumes=controls)
write(P/'levels/exit_doors.json',level)
print(P)

shutil.copytree(P, R/'editor-project')
recipes = Path(__file__).resolve().parent/'acceptance'
for name, folder, kind in [('behavior','native-behavior','actions'), ('culling','native-culling-final','actions'), ('npc-flashlight','native-npc-flashlight','actions'), ('editor','native-editor-final','script'), ('editor-reopen','native-editor-reopen','script'), ('editor-select','native-editor-select','script')]:
 run = R/folder
 run.mkdir()
 data = json.loads((recipes/(name+'.json')).read_text())
 if kind == 'script':data['output'] = str(run)
 write(run/(kind+'.json'), data)
 shutil.copyfile(recipes/'native-settings.json', run/'settings.json')
