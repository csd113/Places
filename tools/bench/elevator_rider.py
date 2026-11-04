#!/usr/bin/env python3
"""Opt-in exact cabin-relative rider pose checks through both level boundaries.

Use a straight or rotated package from elevator_cargo_fixture.py. This tool
does not build or bake; only initial acceptance poses bypass normal controls.
"""
from pathlib import Path
import argparse,hashlib,json,os,subprocess,shutil
import numpy as np
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--binary',type=Path,required=True)
parser.add_argument('--package',type=Path,required=True)
parser.add_argument('--out',type=Path,required=True)
parser.add_argument('--rotated',action='store_true')
args=parser.parse_args()
ROOT=Path(__file__).resolve().parents[2];BIN=args.binary.resolve(strict=True)
rotated=args.rotated;PACKAGE=args.package.resolve(strict=True)
OUT=args.out.resolve();OUT.mkdir(parents=True)
for n,(x,z,yaw,pitch) in enumerate([(-.3,1.15,57,-12),(.3,1.6,298,18),(0,1.35,13,0)]):
 out=OUT/f'offset-{n}';out.mkdir();state=out/'state';state.mkdir();(state/'levels').mkdir();shutil.copy2(PACKAGE,state/'levels/rider.placesmap')
 settings=json.loads((ROOT/'tests/fixtures/native/hero-manifest.json').read_text())['settings'];settings.update(window_width=640,window_height=360,quality='high',texture_filtering='high',lightmaps='full',reflections='off',bloom=False,vsync=True,ambient_volume=0)
 (state/'settings.json').write_text(json.dumps(settings));actions=[]
 def add(ms,kind,after='ready:elevator_lobby',**kw):actions.append(dict(after=after,delay_ms=ms,action=dict(kind=kind,**kw)))
 def click(ms,after='ready:elevator_lobby'):
  add(ms,'mouse',after,button='left',pressed=True);add(ms+70,'mouse',after,button='left',pressed=False)
 add(100,'player',position=[-.96,1.6,-1],yaw=180,pitch=0);add(200,'view_at',position=[-.96,1.08,-.12]);click(300)
 add(1850,'player',position=[x,1.6,z],yaw=yaw,pitch=pitch);add(1950,'view_at',position=[.94,1.47,.4]);click(2050)
 add(2150,'view',yaw=yaw,pitch=pitch);add(3200,'checkpoint',name='before');add(3250,'capture',name='before')
 A='ready:elevator_annex';add(50,'checkpoint',A,name='annex');add(80,'capture',A,name='arrival')
 button=[.4,4.58,-.94] if rotated else [.94,1.58,.4]
 add(1700,'view_at',A,position=button);click(1800,A)
 # Keep an arbitrary look through closure; this also tests the inverse rotation.
 add(1900,'view',A,yaw=137,pitch=9);add(2900,'checkpoint',A,name='before_return')
 add(5400,'checkpoint',A,name='returned');add(5500,'capture',A,name='returned');add(5700,'quit',A);add(22000,'quit')
 script=out/'actions.json';script.write_text(json.dumps(actions));trace=out/'trace.jsonl'
 env={k:v for k,v in os.environ.items() if not k.startswith('PLACES_')};env.update(PLACES_ASSET_ROOT=str(ROOT/'assets'),PLACES_STATE_ROOT=str(state),PLACES_LEVEL='elevator_lobby',PLACES_BENCH='1',PLACES_BENCH_FRAMES='6000',PLACES_BENCH_WARMUP='3',PLACES_PREPARE_DELAY_MS='500',PLACES_LOAD_TRACE=str(trace),PLACES_BENCH_ACTIONS=str(script),PLACES_BENCH_OUT=str(out/'frames.csv'))
 with (out/'game.log').open('w') as log:subprocess.run([str(BIN)],cwd=ROOT,env=env,stdout=log,stderr=subprocess.STDOUT,timeout=50,check=True)
 rows=[json.loads(l) for l in trace.read_text().splitlines()];c={r['event'][11:]:json.loads(r['detail']) for r in rows if r['event'].startswith('checkpoint:')}
 before,annex,back=c['before'],c['annex'],c['returned'];expected=np.array([z,4.6,-x]) if rotated else np.array([x,1.6,z])
 assert annex['current_level_id']=='elevator_annex',annex['current_level_id']
 assert np.linalg.norm(np.array(annex['player_position'])-expected)<.002,(annex['player_position'],expected)
 view=np.array(before['view_direction']);expected_view=np.array([view[2],view[1],-view[0]]) if rotated else view
 assert np.linalg.norm(np.array(annex['view_direction'])-expected_view)<.002,(annex['view_direction'],expected_view)
 assert back['current_level_id']=='elevator_lobby',back['current_level_id']
 assert np.linalg.norm(np.array(back['player_position'])-np.array([x,1.6,z]))<.002,back['player_position']
 view=np.array(c['before_return']['view_direction']);expected_view=np.array([-view[2],view[1],view[0]]) if rotated else view
 assert np.linalg.norm(np.array(back['view_direction'])-expected_view)<.002,(back['view_direction'],expected_view)
 (out/'passed.json').write_text(json.dumps(dict(rotated=rotated,offset=[x,z],before=before['player_position'],annex=annex['player_position'],back=back['player_position'],yaw=yaw,pitch=pitch,binary_sha256=hashlib.sha256(BIN.read_bytes()).hexdigest(),package_sha256=hashlib.sha256(PACKAGE.read_bytes()).hexdigest()),indent=2)+'\n')
 print('Rider pose PASS',rotated,n,flush=True)
