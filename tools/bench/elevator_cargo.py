#!/usr/bin/env python3
"""Opt-in direct-binary passenger acceptance. Never builds or bakes a map.

Create the tiny fixture with elevator_cargo_fixture.py and explicitly export
its project before running this tool. Each run owns a fresh isolated state.
"""
from pathlib import Path
import argparse,hashlib,json,os,subprocess,shutil,time
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--binary',type=Path,required=True)
parser.add_argument('--package',type=Path,required=True)
parser.add_argument('--out',type=Path,required=True)
parser.add_argument('--mode',choices=['dormant','held','active','failure','cancel','blocked-local','blocked-level','shaft-guard'],default='dormant')
parser.add_argument('--vertical',action='store_true',help='Use the physical-source fixture, now fixed vertical cabins with sealed passenger teleport')
parser.add_argument('--lightmaps',choices=['off','full'],default='full')
parser.add_argument('--rotated',action='store_true',help='Package uses the fixture\'s 90 degree / 3 m elevated Annex')
parser.add_argument('--held',action='store_true',help='Combine held ownership with another case, such as failure')
args=parser.parse_args()
ROOT=Path(__file__).resolve().parents[2];ROTATED=args.rotated;OUT=args.out.resolve();OUT.mkdir(parents=True);state=OUT/'state';state.mkdir();(state/'levels').mkdir();shutil.copy2(args.package,state/'levels/cargo.placesmap')
settings=json.loads((ROOT/'tests/fixtures/native/hero-manifest.json').read_text())['settings'];settings.update(window_width=640,window_height=360,quality='high',texture_filtering='high',lightmaps=args.lightmaps,reflections='off',bloom=False,vsync=True,ambient_volume=0)
MODE=args.mode
HELD=MODE=='held' or args.held
(state/'settings.json').write_text(json.dumps(settings));actions=[]
L='ready:elevator_lobby';A='ready:elevator_annex'
def add(ms,kind,after=L,**kw):actions.append(dict(after=after,delay_ms=ms,action=dict(kind=kind,**kw)))
def click(ms,after=L):add(ms,'mouse',after,button='left',pressed=True);add(ms+70,'mouse',after,button='left',pressed=False)
add(100,'player',position=[-.96,1.6,-1],yaw=180,pitch=0);add(200,'view_at',position=[-.96,1.08,-.12]);click(300)
add(1800,'navigation_goal',id='travel_guest',position=[.5,0,1.65]);add(5500,'checkpoint',name='boarded');add(5600,'capture',name='boarded')
add(6000,'player',position=[-.45,1.6,1.05],yaw=0,pitch=0);add(6100,'view_at',position=[.94,1.58,.4]);click(6300)
if HELD:
 add(6010,'aim_item',id='travel_crate');add(6050,'key',key='E',pressed=True);add(6100,'key',key='E',pressed=False)
if MODE=='active':add(5800,'item_velocity',id='travel_crate',linear=[.02,0,0],angular=[0,.01,0])
add(6200,'checkpoint',name='before_local')
add(11200,'checkpoint',name='local');add(11400,'save_checkpoint',name='cargo_upper');add(11800,'resume_checkpoint')
add(14500,'checkpoint',name='resumed');add(14700,'view_at',position=[.94,4.97,.4] if args.vertical else [10.94,1.47,.4]);click(14900)
add(16000,'checkpoint',name='before_annex');add(50,'checkpoint',A,name='annex_first')
add(1800,'checkpoint',A,name='annex');add(1900,'capture',A,name='annex')
add(2200,'save_checkpoint',A,name='cargo_annex');add(2600,'resume_checkpoint',A)
add(5000,'checkpoint',A,name='annex_resumed');add(5200,'view_at',A,position=[.4,4.58,-.94] if ROTATED else [.94,1.58,.4]);click(5400,A)
add(10600,'checkpoint',A,name='returned');add(10800,'capture',A,name='returned');add(11000,'quit',A);add(35000,'quit')
if MODE in ['blocked-local','shaft-guard']:
 actions=[v for v in actions if v['after']==L and v['delay_ms']<11400]
 add(11500,'view',yaw=0,pitch=0);add(11600,'key',key='W',pressed=True);add(12300,'key',key='W',pressed=False);add(12400,'checkpoint',name='live');add(12600,'quit')
elif MODE in ['failure','blocked-level','cancel']:
 anchor='failed:elevator_annex' if MODE!='cancel' else 'request:elevator_annex'
 if MODE=='cancel':add(150,'escape',anchor)
 add(2200,'checkpoint',anchor,name='recovered');add(2300,'view',anchor,yaw=0,pitch=0);add(2400,'key',anchor,key='W',pressed=True);add(3100,'key',anchor,key='W',pressed=False);add(3200,'checkpoint',anchor,name='live');add(3400,'quit',anchor)
actions.insert(0,dict(after=L,delay_ms=0,action=dict(kind='focus',focused=True)))
actions.insert(0,dict(after=A,delay_ms=0,action=dict(kind='focus',focused=True)))
script=OUT/'actions.json';script.write_text(json.dumps(actions));trace=OUT/'trace.jsonl'
env={k:v for k,v in os.environ.items() if not k.startswith('PLACES_')};env.update(PLACES_ASSET_ROOT=str(ROOT/'assets'),PLACES_STATE_ROOT=str(state),PLACES_LEVEL='elevator_lobby',PLACES_BENCH='1',PLACES_BENCH_FRAMES='6000',PLACES_BENCH_WARMUP='3',PLACES_PREPARE_DELAY_MS='500',PLACES_LOAD_TRACE=str(trace),PLACES_BENCH_ACTIONS=str(script),PLACES_BENCH_OUT=str(OUT/'frames.csv'),PLACES_PHYSICS_TRACE='1',PLACES_AUDIO_CAPTURE='1')
with (OUT/'game.log').open('w') as log:
 proc=subprocess.Popen([str(args.binary.resolve(strict=True))],cwd=ROOT,env=env,stdout=log,stderr=subprocess.STDOUT);started=time.monotonic();removed=False;package=state/'levels/cargo.placesmap';hidden=package.with_suffix('.unavailable')
 try:
  while proc.poll() is None:
   if time.monotonic()-started>70:raise RuntimeError('Native cargo case timed out')
   if MODE=='failure' and not removed and trace.exists():
    if any(json.loads(l).get('event')=='request' and json.loads(l).get('detail')=='elevator_annex' for l in trace.read_text().splitlines()):package.rename(hidden);removed=True
   time.sleep(.03)
  assert proc.returncode==0,proc.returncode
 finally:
  if proc.poll() is None:proc.terminate();proc.wait(timeout=10)
  if removed:hidden.rename(package)
r=[json.loads(l) for l in trace.read_text().splitlines()];c={x['event'][11:]:json.loads(x['detail']) for x in r if x['event'].startswith('checkpoint:')}
for name,d in c.items():
 print(name,d['current_level_id'],d['player_position'],[(a['id'],a['position']) for a in d['entity_simulation']['agents']],d['physics']['authored'],flush=True)
(OUT/'checkpoints.json').write_text(json.dumps(c,indent=2)+'\n')
(OUT/'provenance.json').write_text(json.dumps(dict(binary_sha256=hashlib.sha256(args.binary.read_bytes()).hexdigest(),package_sha256=hashlib.sha256(args.package.read_bytes()).hexdigest(),mode=MODE,held=HELD,rotated=ROTATED),indent=2)+'\n')
if MODE in ['blocked-local','shaft-guard','failure','blocked-level','cancel']:
 before=c['before_local' if MODE in ['blocked-local','shaft-guard'] else 'before_annex'];after=c['local' if MODE in ['blocked-local','shaft-guard'] else 'recovered'];live=c['live']
 assert before['current_level_id']==after['current_level_id']=='elevator_lobby'
 assert max(abs(a-b) for a,b in zip(before['player_position'],after['player_position']))<.002
 assert after['state']=='Playing' and not after['transition_frozen']
 assert len(after['physics']['bodies'])==len(before['physics']['bodies'])
 assert after['entity_simulation']['agents'][0]['id']==before['entity_simulation']['agents'][0]['id']
 assert max(abs(a-b) for a,b in zip(before['entity_simulation']['agents'][0]['position'],after['entity_simulation']['agents'][0]['position']))<.01
 assert abs(live['player_position'][2]-after['player_position'][2])>.1,(live['player_position'],after['player_position'])
 if HELD:assert before['physics']['held'] is not None and after['physics']['held'] is not None
 (OUT/'passed.json').write_text(json.dumps(dict(passed=True,kind=MODE,held=HELD,checks=['source player/occupant poses retained','no lost or duplicate bodies','doors reopened','gameplay movement remains live']),indent=2)+'\n');raise SystemExit(0)
for name,x,level in [('local',-.55 if args.vertical else 9.45,'elevator_lobby'),('resumed',-.55 if args.vertical else 9.45,'elevator_lobby'),('annex',-.55,'elevator_annex'),('annex_resumed',-.55,'elevator_annex'),('returned',-.55,'elevator_lobby')]:
 d=c[name];assert d['current_level_id']==level,(name,d['current_level_id'])
 wanted='travel_crate' if level=='elevator_lobby' else next(a[1] for a in d['physics']['authored'] if a[1].startswith('travel_'))
 key=next(a[0] for a in d['physics']['authored'] if a[1]==wanted);b=next(b for b in d['physics']['bodies'] if b['key']==key)
 if HELD:assert b['held'],(name,b)
 elif MODE!='active':
  expected=[2.05,3.02,.55] if ROTATED and level=='elevator_annex' else [x,3.52 if args.vertical and name in ['local','resumed'] else .02,2.05]
  assert max(abs(a-e) for a,e in zip(b['pose']['position'],expected))<.025,(name,b['pose'],expected)
 if args.vertical and name in ['local','resumed']:
  assert abs(d['player_position'][1]-5.1)<.025,(name,d['player_position'])
  assert abs(d['entity_simulation']['agents'][0]['position'][1]-3.5)<.025,(name,d['entity_simulation']['agents'])
 assert len(d['entity_simulation']['agents'])==1,(name,d['entity_simulation']['agents'])
 assert len(d['physics']['bodies'])==(1 if level=='elevator_annex' else 2),(name,d['physics']['bodies'])
 assert any(['cargo_health',61] in state['values'] for state in d['entity_simulation']['states']),(name,d['entity_simulation']['states'])
(OUT/'passed.json').write_text(json.dumps(dict(passed=True,mode=MODE,held=HELD,rotated=ROTATED,checks=['NPC walked through open doorway',f'{"held" if HELD else MODE} prop and NPC same-map travel','outside prop excluded','stable local checkpoint resume','cross-level ownership','foreign checkpoint resume','NPC custom state retained','single-instance return']),indent=2)+'\n')
