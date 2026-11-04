#!/usr/bin/env python3
"""Opt-in direct-binary door-only / predator acceptance, with tiny fixtures.

--write-fixtures writes editable sources only. Export each chosen project with
places-compile explicitly; this tool never builds Rust or bakes shipped maps.
"""
import argparse
import copy
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[2]
LOBBY = 'ready:elevator_lobby'
ANNEX = 'ready:elevator_annex'


def fixtures(destination):
    destination.mkdir(parents=True, exist_ok=False)
    source = ROOT / 'projects/elevator-links'
    for case in ('two', 'three', 'monster'):
        folder = destination / case
        (folder / 'levels').mkdir(parents=True)
        manifest = json.loads((source / 'places.project.json').read_text())
        manifest.update(id='door_acceptance_'+case, package_version='1.0.0')
        levels = {name: json.loads((source/f'levels/{name}.json').read_text())
                  for name in ('elevator_lobby', 'elevator_annex')}
        for level in levels.values():
            level['props'] = []
        if case != 'three':
            manifest['levels'] = manifest['levels'][:1]
            levels.pop('elevator_annex')
            levels['elevator_lobby']['transitions'] = []
            for lift in levels['elevator_lobby']['elevators']:
                lift['stops'] = lift['stops'][:2]
        if case == 'monster':
            levels['elevator_lobby']['props'] = [dict(
                id='chaser', model='pumpkin-skeleton', x=0, y=0, z=-4.7,
                rotation_degrees=0, scale=1, components=[
                    dict(component='nav_agent', radius=.24, height=1.8, speed_mps=.8),
                    dict(component='ai', behavior='predator', reacts_to=['player'],
                         walk_speed=.7, run_speed=.8, sight_range=12,
                         sight_fov_degrees=360, hearing_range=6, pursue_distance=15,
                         catch_radius=.65, can_open_doors=True,
                         move_clip='walk')])]
        (folder/'places.project.json').write_text(json.dumps(manifest, indent=2)+'\n')
        for name, level in levels.items():
            (folder/f'levels/{name}.json').write_text(json.dumps(level, indent=2)+'\n')
        print(folder)


def actions(mode):
    result = []
    def add(ms, kind, after=LOBBY, **kw):
        result.append(dict(after=after, delay_ms=ms, action=dict(kind=kind, **kw)))
    def click(ms, point, after=LOBBY, keyboard=False):
        add(ms-40, 'view_at', after, position=point)
        if keyboard:
            add(ms, 'key', after, key='E', pressed=True)
            add(ms+70, 'key', after, key='E', pressed=False)
        else:
            add(ms, 'mouse', after, button='left', pressed=True)
            add(ms+70, 'mouse', after, button='left', pressed=False)
    def checkpoint(ms, name, after=LOBBY):
        add(ms, 'checkpoint', after, name=name)
    def walk(ms, duration, key):
        add(ms, 'key', key=key, pressed=True)
        add(ms+duration, 'key', key=key, pressed=False)
    if mode == 'shaft':
        add(50, 'player', position=[0, 1.6, 1.2], yaw=0, pitch=0)
        click(200, [.94, 1.58, .4])
        # This starts closed, so the sealed ride starts immediately. Aim at
        # the fixed control and press well before its 1.8-second transfer.
        add(220, 'view_at', position=[.94, 1.30, .4])
        add(1300, 'mouse', button='left', pressed=True)
        add(1370, 'mouse', button='left', pressed=False)
        checkpoint(1450, 'moving-sealed')
        checkpoint(7000, 'upper-arrived')
        click(7200, [.94, 4.80, .4])
        checkpoint(8900, 'upper-closed')
        click(9100, [.94, 4.80, .4])
        checkpoint(10700, 'upper-reopened')
        click(10900, [.94, 5.08, .4])
        checkpoint(17500, 'shaft-returned')
        add(17700, 'quit')
        return result
    if mode == 'monster':
        add(40, 'player', position=[-.96, 1.6, -1], yaw=180, pitch=0)
        click(180, [-.96, 1.08, -.12])
        checkpoint(1400, 'chasing')
        add(1750, 'view', yaw=180, pitch=0)
        # Leave room for the controller's normal coast after releasing keys.
        walk(1800, 220, 'A')
        walk(2150, 650, 'W')
        checkpoint(3000, 'boarded')
        click(3100, [.94, 1.30, .4])
        checkpoint(4800, 'sealed')
        # Walk toward the closed door and sideways behind it: a live target
        # lookup must not keep updating the monster's visual contact.
        add(4900, 'view', yaw=0, pitch=0)
        walk(5000, 240, 'W')
        walk(5300, 100, 'A')
        walk(5500, 200, 'D')
        checkpoint(6200, 'hidden')
        add(6300, 'capture', name='hidden-inside')
        checkpoint(11200, 'hidden-long')
        click(11500, [.94, 1.30, .4])
        checkpoint(13200, 'reopened')
        add(13300, 'view_at', position=[0, 1.2, -1])
        add(13400, 'capture', name='monster-outside')
        checkpoint(15100, 'reacquired')
        # A separate acceptance pose puts the player clear of the leaves and
        # lets the live pursuer occupy the threshold. Only the monster should
        # stop this closure; its AI/movement is never scripted or teleported.
        add(15150, 'player', position=[0, 1.6, 1.0], yaw=0, pitch=0)
        click(15400, [.94, 1.30, .4])
        checkpoint(15500, 'monster-close-start')
        checkpoint(17400, 'monster-obstruction-reopened')
        add(17600, 'quit')
        return result
    add(50, 'player', position=[0, 1.6, 1.2], yaw=0, pitch=0)
    click(200, [.94, 1.30, .4])
    click(400, [.94, 1.30, .4])
    checkpoint(900, 'rapid-closed')
    click(1000, [.94, 1.30, .4])
    checkpoint(1800, 'opened')
    add(1850, 'capture', name='panel-open')
    click(2000, [.94, 1.30, .4], keyboard=True)
    checkpoint(3600, 'closed')
    add(3650, 'save_checkpoint', name='doors_closed')
    add(3900, 'resume_checkpoint')
    checkpoint(5200, 'closed-resumed')
    click(5400, [.94, 1.30, .4])
    checkpoint(7000, 'reopened')
    click(7200, [.94, 1.30, .4])
    add(7300, 'player', position=[0, 1.6, .4], yaw=0, pitch=0)
    checkpoint(8800, 'obstruction-reopened')
    add(8900, 'player', position=[0, 1.6, 1.2], yaw=0, pitch=0)
    click(9100, [.94, 1.58, .4])
    click(9300, [.94, 1.30, .4])
    checkpoint(11000, 'trip-cancelled')
    click(11200, [.94, 1.58, .4])
    checkpoint(16000, 'local-arrived')
    click(16200, [10.94, 1.30, .4])
    checkpoint(17800, 'local-closed')
    click(18000, [10.94, 1.30, .4])
    checkpoint(19600, 'local-reopened')
    if mode == 'two':
        click(19800, [10.94, 1.58, .4])
        checkpoint(24600, 'returned')
        add(24800, 'quit')
    else:
        click(19800, [10.94, 1.47, .4])
        checkpoint(1700, 'annex-arrived', ANNEX)
        click(1900, [.94, 1.30, .4], ANNEX)
        checkpoint(3500, 'annex-closed', ANNEX)
        click(3700, [.94, 1.30, .4], ANNEX)
        checkpoint(5300, 'annex-reopened', ANNEX)
        click(5500, [.94, 1.58, .4], ANNEX)
        checkpoint(10400, 'returned', ANNEX)
        add(10600, 'quit', ANNEX)
    add(45000, 'quit')
    for step in result:
        if step['after']==LOBBY and step['delay_ms']>=1800:
            step['delay_ms']+=1000
    return result


def verify(mode, rows):
    checkpoints = {r['event'][11:]: json.loads(r['detail']) for r in rows
                   if r['event'].startswith('checkpoint:')}
    def lift(name, index=0):
        return checkpoints[name]['elevators'][index]
    if mode == 'shaft':
        assert lift('moving-sealed')['phase']=='moving'
        assert lift('moving-sealed')['opening']==0
        target=checkpoints['moving-sealed']['interaction_target']
        assert target and target['id'].endswith(':door'), target
        for name,phase in [('upper-arrived','open'),('upper-closed','closed'),('upper-reopened','open')]:
            assert lift(name)['phase']==phase, (name,lift(name))
            assert lift(name)['stop']==lift(name)['target']==1
            assert abs(checkpoints[name]['player_position'][1]-5.1)<.01
        assert lift('shaft-returned')['stop']==0
        assert lift('shaft-returned')['phase']=='open'
        assert abs(checkpoints['shaft-returned']['player_position'][1]-1.6)<.01
    elif mode == 'monster':
        def monster(name):
            return checkpoints[name]['entity_simulation']['agents'][0]
        assert monster('chasing')['state'] == 'pursue'
        assert checkpoints['boarded']['player_position'][2] > 1
        for name in ('sealed', 'hidden', 'hidden-long'):
            assert lift(name)['phase'] == 'closed', (name, lift(name))
            assert lift(name)['stop'] == lift(name)['target'] == 0
            agent = monster(name)
            assert agent['position'][2] < 0, (name, agent)
            assert agent['contact'] is None or agent['contact']['heard_only'], agent
        assert abs(monster('hidden')['last_seen_at']-monster('hidden-long')['last_seen_at']) < .001
        assert monster('reacquired')['last_seen_at'] > monster('hidden-long')['last_seen_at']
        assert lift('reopened')['phase'] == 'open'
        assert lift('monster-close-start')['phase']=='closing'
        assert lift('monster-obstruction-reopened')['phase']=='open'
        assert abs(checkpoints['monster-obstruction-reopened']['player_position'][2]-1)<.01
        assert -.2 < monster('monster-obstruction-reopened')['position'][2] < .56
        assert not any(r['event']=='request' and r['detail']=='elevator_annex' for r in rows)
    else:
        for name in ('opened','reopened','obstruction-reopened','trip-cancelled'):
            assert lift(name)['phase']=='open', (name,lift(name))
            assert lift(name)['stop']==lift(name)['target']==0
            assert checkpoints[name]['current_level_id']=='elevator_lobby'
        for name in ('rapid-closed','closed','closed-resumed'):
            assert lift(name)['phase']=='closed', (name,lift(name))
            assert lift(name)['stop']==lift(name)['target']==0
        for name, phase in [('local-arrived','open'),('local-closed','closed'),('local-reopened','open')]:
            assert lift(name,1)['phase']==phase, (name,lift(name,1))
            assert abs(checkpoints[name]['player_position'][0]-10)<.01
        if mode=='three':
            for name,phase in [('annex-arrived','open'),('annex-closed','closed'),('annex-reopened','open')]:
                assert checkpoints[name]['current_level_id']=='elevator_annex'
                assert lift(name)['phase']==phase, (name,lift(name))
        else:
            assert not any(r['event']=='request' and r['detail']=='elevator_annex' for r in rows)
        assert checkpoints['returned']['current_level_id']=='elevator_lobby'
        assert abs(checkpoints['returned']['player_position'][0])<.01
    return checkpoints


def run(args):
    out=args.out.resolve();out.mkdir(parents=True,exist_ok=False)
    state=out/'state';(state/'levels').mkdir(parents=True)
    shutil.copy2(args.package,state/'levels/doors.placesmap')
    settings=json.loads((ROOT/'tests/fixtures/native/hero-manifest.json').read_text())['settings']
    settings.update(window_width=640,window_height=360,quality='high',lightmaps='full',
                    texture_filtering='high',reflections='off',bloom=False,vsync=True,
                    walk_speed=3,fov_degrees=90)
    (state/'settings.json').write_text(json.dumps(settings))
    scripted=actions(args.mode)
    for anchor in (LOBBY,ANNEX):
        scripted.insert(0,dict(after=anchor,delay_ms=0,action=dict(kind='focus',focused=True)))
    script=out/'actions.json';script.write_text(json.dumps(scripted,indent=2)+'\n')
    trace=out/'trace.jsonl'
    env={k:v for k,v in os.environ.items() if not k.startswith('PLACES_')}
    env.update(PLACES_ASSET_ROOT=str(ROOT/'assets'),PLACES_STATE_ROOT=str(state),
               PLACES_LEVEL='elevator_lobby',PLACES_BENCH='1',PLACES_BENCH_FRAMES='6000',
               PLACES_BENCH_WARMUP='3',PLACES_LOAD_TRACE=str(trace),PLACES_BENCH_ACTIONS=str(script),
               PLACES_AUDIO_CAPTURE='1')
    with (out/'game.log').open('w') as log:
        subprocess.run([str(args.binary.resolve())],cwd=ROOT,env=env,stdout=log,
                       stderr=subprocess.STDOUT,check=True,timeout=60)
    rows=[json.loads(line) for line in trace.read_text().splitlines()]
    checkpoints={r['event'][11:]:json.loads(r['detail']) for r in rows if r['event'].startswith('checkpoint:')}
    (out/'checkpoints.json').write_text(json.dumps(checkpoints,indent=2)+'\n')
    verify(args.mode,rows)
    receipt=dict(passed=True,mode=args.mode,binary_sha256=hashlib.sha256(args.binary.read_bytes()).hexdigest(),
                 package_sha256=hashlib.sha256(args.package.read_bytes()).hexdigest(),
                 checkpoints=list(checkpoints),scope='Actual direct binary; ordinary mouse/E controls and movement after explicit initial acceptance poses; macOS Metal')
    (out/'passed.json').write_text(json.dumps(receipt,indent=2)+'\n')
    print('Native door acceptance passed: '+args.mode)


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--write-fixtures',type=Path)
    parser.add_argument('--binary',type=Path)
    parser.add_argument('--package',type=Path)
    parser.add_argument('--out',type=Path)
    parser.add_argument('--mode',choices=['two','three','monster','shaft'],default='three')
    args=parser.parse_args()
    if args.write_fixtures:
        fixtures(args.write_fixtures)
    elif args.binary and args.package and args.out:
        run(args)
    else:
        parser.error('Supply --write-fixtures, or --binary/--package/--out')


if __name__=='__main__':
    main()
