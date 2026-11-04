#!/usr/bin/env python3
"""Opt-in native lost-sight search acceptance. Writes tiny sources; never bakes.

Export each fixture explicitly, then run the direct binary in isolated state.
The monster always uses ordinary AI. Acceptance player poses are in the trace.
"""
import argparse
import copy
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess

ROOT=Path(__file__).resolve().parents[2]
READY='ready:elevator_lobby'


def write_fixtures(out):
    out.mkdir(parents=True,exist_ok=False)
    source=ROOT/'projects/elevator-links'
    original=json.loads((source/'levels/elevator_lobby.json').read_text())
    for case in ('elevator','fast','ordinary','blocked'):
        folder=out/case;(folder/'levels').mkdir(parents=True)
        manifest=json.loads((source/'places.project.json').read_text())
        manifest.update(id='predator_search_'+case,package_version='1.0.0',
                        name='TEMPORARY / Monster search / '+case)
        manifest['levels']=manifest['levels'][:1]
        level=copy.deepcopy(original);level['transitions']=[]
        for lift in level['elevators']:lift['stops']=lift['stops'][:2]
        delay={'elevator':8,'fast':3,'ordinary':3,'blocked':.75}[case]
        ai=dict(component='ai',behavior='predator',reacts_to=['player'],walk_speed=.7,
                run_speed=.8,sight_range=12,sight_fov_degrees=360,hearing_range=6,
                pursue_distance=15,catch_radius=.65,can_open_doors=True,move_clip='walk',
                search_after_seconds=delay,search_radius=12)
        actor=dict(id='chaser',model='pumpkin-skeleton',x=0,y=0,z=-4.7,scale=1,
                   components=[dict(component='nav_agent',radius=.24,height=1.8,speed_mps=.8),ai])
        if case=='ordinary':
            level['elevators']=[]
            level['rooms']=[dict(x=-6,z=-8,width=12,depth=10,height=2.8,
                                 material='core:carpet_beige_01')]
            level['walls']=[dict(x=-.2,y=0,z=-7,width=.4,depth=7,height=2.8)]
            actor.update(x=-2,z=-5)
            level['spawn']=dict(x=-2,z=-3,yaw_degrees=0)
        elif case=='blocked':
            level['elevators']=[]
            level['rooms']=[dict(x=-1,z=-1,width=2,depth=2,height=2.8,
                                 material='core:carpet_beige_01'),
                            dict(x=10,z=-1,width=2,depth=2,height=2.8,
                                 material='core:carpet_beige_01')]
            level['walls']=[]
            for x in (-1,10):
                level['walls'] += [dict(x=x-.2,y=0,z=-1.2,width=2.4,depth=.2,height=2.8),
                                  dict(x=x-.2,y=0,z=1,width=2.4,depth=.2,height=2.8),
                                  dict(x=x-.2,y=0,z=-1,width=.2,depth=2,height=2.8),
                                  dict(x=x+2,y=0,z=-1,width=.2,depth=2,height=2.8)]
            actor.update(x=-.4,z=-.3)
            level['spawn']=dict(x=.4,z=.3,yaw_degrees=0)
        level['props']=[actor]
        if case!='elevator':level['ceiling_lights']=[]
        (folder/'places.project.json').write_text(json.dumps(manifest,indent=2)+'\n')
        (folder/'levels/elevator_lobby.json').write_text(json.dumps(level,indent=2)+'\n')
        print(folder)


def actions(mode):
    steps=[]
    def add(ms,kind,**kw):steps.append(dict(after=READY,delay_ms=ms,action=dict(kind=kind,**kw)))
    def cp(ms,name):add(ms,'checkpoint',name=name)
    def click(ms):
        add(ms-40,'view_at',position=[.94,1.3,.4]);add(ms,'mouse',button='left',pressed=True);add(ms+70,'mouse',button='left',pressed=False)
    if mode in ('elevator','fast'):
        import elevator_doors
        steps.extend(x for x in elevator_doors.actions('monster') if x['delay_ms']<11500)
        # The first part already proves ordinary walking into the cabin and
        # visual occlusion. Wait long enough for outward search, still inside.
        cp(14000,'search-start');cp(17000,'search-leg')
        if mode=='elevator':
            add(18000,'save_checkpoint',name='hidden_search')
            add(19000,'resume_checkpoint')
            cp(22500,'search-resumed')
            add(23000,'view',yaw=0,pitch=0);add(23200,'capture',name='hidden-long')
            open_ms=24500
        else:open_ms=18000
        click(open_ms)
        cp(open_ms+1700,'opened-after-search')
        add(open_ms+1750,'view',yaw=0,pitch=0)
        add(open_ms+1800,'capture',name='after-search-open')
        add(open_ms+1900,'key',key='W',pressed=True)
        add(open_ms+2500,'key',key='W',pressed=False)
        cp(open_ms+4300,'reacquired');add(open_ms+4500,'quit')
    else:
        initial=[-2,1.6,-3] if mode=='ordinary' else [.4,1.6,.3]
        hidden=[3,1.6,-3] if mode=='ordinary' else [11,1.6,0]
        add(40,'player',position=initial,yaw=0,pitch=0)
        cp(1200,'chasing');add(1500,'player',position=hidden,yaw=270,pitch=0)
        cp(2000,'lost');cp(6500,'search-start');cp(9500,'search-leg')
        add(9600,'capture',name='search');add(10000,'quit')
    add(45000,'quit')
    return steps


def verify(mode,checkpoints):
    def agent(name):return checkpoints[name]['entity_simulation']['agents'][0]
    assert agent('chasing')['state']=='pursue'
    if mode in ('elevator','fast'):
        sealed=checkpoints['sealed'];origin=agent('sealed')['position']
        assert sealed['elevators'][0]['phase']=='closed'
        expected=8 if mode=='elevator' else 3
        assert abs(agent('sealed')['search']['until']-agent('sealed')['last_seen_at']-expected)<.01
        assert agent('hidden-long')['search']['stage']==('investigate' if mode=='elevator' else 'search')
        hidden_names=['hidden-long','search-start','search-leg']
        if mode=='elevator':hidden_names.append('search-resumed')
        moved=[]
        for name in hidden_names:
            snap=checkpoints[name];npc=agent(name)
            assert snap['current_level_id']=='elevator_lobby'
            assert snap['elevators'][0]['phase']=='closed', (name,snap['elevators'])
            assert snap['elevators'][0]['stop']==snap['elevators'][0]['target']==0
            assert npc['contact'] is None or npc['contact']['heard_only'],(name,npc)
            moved.append(sum((a-b)**2 for a,b in zip(npc['position'],origin))**.5)
        assert max(moved)>=1.5,moved
        for name in ['search-start','search-leg']:
            assert agent(name)['search']['stage']=='search',(name,agent(name))
        if mode=='elevator':
            assert agent('search-resumed')['search']['stage']=='search'
            assert abs(agent('search-resumed')['search']['until']-agent('search-leg')['search']['until'])<.001
        assert agent('reacquired')['state']=='pursue',agent('reacquired')
        assert agent('reacquired')['contact'] and not agent('reacquired')['contact']['heard_only']
        assert checkpoints['opened-after-search']['elevators'][0]['phase']=='open'
    elif mode=='ordinary':
        assert agent('lost')['contact'] is None or agent('lost')['contact']['heard_only']
        assert agent('search-start')['search']['stage']=='search'
        assert agent('search-leg')['search']['stage']=='search'
        assert agent('search-leg')['last_seen_at']<2
    else:
        for name in ['search-start','search-leg']:
            npc=agent(name);assert npc['search']['stage']=='search'
            assert npc['state']=='idle' and npc['path_goal'] is None,npc
            assert npc['position'][0]<1 and npc['position'][2]<1,npc
            assert npc['contact'] is None or npc['contact']['heard_only']
            assert npc['path_queries']<300,npc


def run(args):
    out=args.out.resolve();out.mkdir(parents=True,exist_ok=False)
    state=out/'state';(state/'levels').mkdir(parents=True)
    shutil.copy2(args.package,state/'levels/search.placesmap')
    settings=json.loads((ROOT/'tests/fixtures/native/hero-manifest.json').read_text())['settings']
    settings.update(window_width=640,window_height=360,quality='high',lightmaps='full',
                    texture_filtering='high',reflections='off',bloom=False,vsync=True,
                    walk_speed=3,fov_degrees=90)
    (state/'settings.json').write_text(json.dumps(settings))
    script=out/'actions.json';script.write_text(json.dumps(actions(args.mode),indent=2)+'\n')
    trace=out/'trace.jsonl';env={k:v for k,v in os.environ.items() if not k.startswith('PLACES_')}
    env.update(PLACES_ASSET_ROOT=str(ROOT/'assets'),PLACES_STATE_ROOT=str(state),PLACES_LEVEL='elevator_lobby',
               PLACES_BENCH='1',PLACES_BENCH_FRAMES='6000',PLACES_BENCH_WARMUP='3',
               PLACES_LOAD_TRACE=str(trace),PLACES_BENCH_ACTIONS=str(script))
    with (out/'game.log').open('w') as log:
        subprocess.run([str(args.binary.resolve())],cwd=ROOT,env=env,stdout=log,stderr=subprocess.STDOUT,check=True,timeout=60)
    rows=[json.loads(line) for line in trace.read_text().splitlines()]
    snapshots={r['event'][11:]:json.loads(r['detail']) for r in rows if r['event'].startswith('checkpoint:')}
    (out/'checkpoints.json').write_text(json.dumps(snapshots,indent=2)+'\n');verify(args.mode,snapshots)
    receipt=dict(passed=True,mode=args.mode,binary_sha256=hashlib.sha256(args.binary.read_bytes()).hexdigest(),
                 package_sha256=hashlib.sha256(args.package.read_bytes()).hexdigest(),checkpoints=list(snapshots),
                 scope='Actual direct binary and ordinary monster AI; acceptance player poses/actions retained; macOS Metal')
    (out/'passed.json').write_text(json.dumps(receipt,indent=2)+'\n');print('Native predator search passed: '+args.mode)


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--write-fixtures',type=Path);parser.add_argument('--binary',type=Path)
    parser.add_argument('--package',type=Path);parser.add_argument('--out',type=Path)
    parser.add_argument('--mode',choices=['elevator','fast','ordinary','blocked'],default='elevator')
    args=parser.parse_args()
    if args.write_fixtures:write_fixtures(args.write_fixtures)
    elif args.binary and args.package and args.out:run(args)
    else:parser.error('Supply --write-fixtures or --binary/--package/--out')


if __name__=='__main__':main()
