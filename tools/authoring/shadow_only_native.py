#!/usr/bin/env python3
"""Explicit native shadow-only evidence; never a Cargo/CI or map-bake dependency.

Uses direct native binaries and existing prepared packages. Diagnostic camera
placement is retained in actions.json; sweep/flashlight/interaction keys pass
through production input handlers. Video records consecutive native RGBA frames.
Performance runs use the existing uncapped-presentation frame-time receipt.
"""
from pathlib import Path
import argparse
import hashlib
import json
import os
import subprocess
import sys

ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT/'tools/bench'))
from flashlight import video_command  # noqa: E402


def write(path,value):
    path.write_text(json.dumps(value,indent=2)+'\n')


def actions(level,case):
    steps=[]
    def add(ms,kind,**fields):
        steps.append(dict(after=f'ready:{level}',delay_ms=ms,action=dict(kind=kind,**fields)))
    def tap(ms,key):
        add(ms,'key',key=key,pressed=True);add(ms+60,'key',key=key,pressed=False)
    add(0,'focus',focused=True)
    add(150,'player',position=[3,1.6,6] if case=='default-pose' else [5.8,1.6,28],yaw=0,pitch=-6)
    if case=='default-pose':
        add(250,'capture',name='initial')
        add(400,'key',key='W',pressed=True);add(900,'capture',name='walking')
        add(1100,'key',key='W',pressed=False);tap(1300,'F')
        add(1600,'capture',name='held-runtime');add(2000,'quit')
    elif case=='video':
        tap(1100,'F')
        add(2200,'key',key='Left',pressed=True);add(3100,'key',key='Left',pressed=False)
        add(3700,'key',key='Right',pressed=True);add(5400,'key',key='Right',pressed=False)
        add(6200,'key',key='Left',pressed=True);add(7100,'key',key='Left',pressed=False)
        tap(10200,'F');tap(11500,'F')
        add(17000,'capture',name='wall-shadow')
        add(24000,'quit')
    elif case=='performance':
        pass
    elif case=='compare':
        add(700,'capture',name='off')
        tap(900,'F');add(1500,'capture',name='on')
        add(2000,'player',position=[9,1.6,27],yaw=0,pitch=0)
        add(2500,'capture',name='blocked')
        add(3000,'player',position=[.65,1.6,30],yaw=0,pitch=0)
        add(3500,'view_at',position=[.65,1,2]);add(4000,'capture',name='beyond-range')
        add(4500,'player',position=[4,1.6,26],yaw=-65,pitch=-5)
        add(5000,'capture',name='mirror');add(5500,'quit')
    else:
        add(700,'capture',name='off')
        tap(900,'F');add(1500,'capture',name='animated-a')
        add(2400,'capture',name='animated-b')
        add(2700,'player',position=[5.1,1.6,25],yaw=0,pitch=-30)
        add(3300,'capture',name='floor-near')
        add(3600,'player',position=[9,1.6,27],yaw=0,pitch=0)
        add(4200,'capture',name='blocked')
        add(4500,'player',position=[9,1.6,22.4],yaw=0,pitch=-8)
        add(5100,'capture',name='unblocked')
        add(5400,'player',position=[.65,1.6,30],yaw=0,pitch=0)
        add(5600,'view_at',position=[.65,1,2])
        add(6300,'capture',name='beyond-range')
        add(6600,'player',position=[.65,1.6,9],yaw=0,pitch=-5)
        add(7200,'capture',name='far-in-range')
        add(7500,'player',position=[2.5,1.6,25],yaw=0,pitch=-8)
        add(8100,'capture',name='ordinary-control')
        add(8250,'player',position=[4,1.6,26],yaw=-65,pitch=-5)
        add(8350,'capture',name='mirror')
        add(8400,'player',position=[5,1.6,24.1],yaw=0,pitch=-10)
        tap(8700,'E');add(9400,'capture',name='hidden-interaction')
        add(9700,'key',key='W',pressed=True);add(10500,'key',key='W',pressed=False)
        add(10700,'capture',name='solid-collision')
        add(11000,'lightmaps',quality='off');add(11600,'capture',name='static')
        add(11900,'quality',level='low');add(12600,'capture',name='low')
        add(12900,'low_lighting',enabled=True);add(13600,'capture',name='low-lighting')
        add(13900,'low_lighting',enabled=False);add(14200,'quality',level='high')
        add(14500,'lightmaps',quality='full');add(15500,'capture',name='restored-full')
        add(15800,'console_enabled',enabled=True);tap(16100,'`')
        add(16400,'text',text='/become mannequin',submit=True);tap(16700,'`')
        add(17400,'capture',name='third-person')
        tap(17700,'`');add(18000,'text',text='/become reset',submit=True);tap(18300,'`')
        add(19000,'capture',name='first-person-restored')
        add(19500,'activate',id='reset_marker');add(20400,'capture',name='reset')
        add(21000,'load',level=level);add(24000,'focus',focused=True)
        add(25000,'capture',name='reloaded');add(25700,'quit')
    return sorted(steps,key=lambda step:step['delay_ms'])


def run(args):
    out=args.out.resolve();out.mkdir(parents=True,exist_ok=False)
    package=args.package.resolve();binary=args.binary.resolve()
    settings=dict(window_width=640,window_height=360,window_mode='windowed',quality='high',
                  texture_filtering='high',lightmaps='full',reflections='full',bloom=True,
                  vsync=False,developer_console=False,fov_degrees=60,look_speed_h=30,look_speed_v=35)
    write(out/'settings.json',settings);write(out/'actions.json',actions(args.level,args.case))
    env={k:v for k,v in os.environ.items() if not k.startswith('PLACES_') and k!='SDL_VIDEODRIVER'}
    env.update(PLACES_ASSET_ROOT=str(ROOT/'assets'),PLACES_STATE_ROOT=str(out),PLACES_LEVEL=args.level,
        PLACES_BENCH='1',PLACES_VERBOSE='1',PLACES_VSYNC='off',PLACES_BENCH_FRAMES='0',
        PLACES_SPAWN='5.8,1.6,28,0',PLACES_BENCH_ACTIONS=str(out/'actions.json'),
        PLACES_LOAD_TRACE=str(out/'trace.jsonl'),PLACES_BENCH_FIXED_DELTA_SECONDS='0.016666667')
    if args.case=='default-pose':
        env.pop('PLACES_BENCH_FIXED_DELTA_SECONDS',None)
        env['PLACES_SPAWN']='3,1.6,6,0'
    if args.case=='performance':
        env.update(PLACES_BENCH_FRAMES='1200',PLACES_BENCH_WARMUP='120',PLACES_CAMERA='0,-6',
                   PLACES_BENCH_OUT=str(out/'frames.csv'),PLACES_MOVE_SCRIPT='flashlight@0-0.035')
    encoder=None
    if args.case=='video':
        args.ffmpeg=Path('/opt/homebrew/bin/ffmpeg');args.expected_drawable='1280x720';args.mode='shadow-only'
        fifo=out/'native-video.fifo';os.mkfifo(fifo,mode=0o600)
        env.update(PLACES_BENCH_VIDEO_PIPE=str(fifo),PLACES_BENCH_VIDEO_SIZE='1280x720',
                   PLACES_BENCH_VIDEO_FRAMES='540',PLACES_BENCH_VIDEO_STRIDE='2',PLACES_BENCH_VIDEO_START_FRAME='0')
        with (out/'encoder.log').open('w') as log:
            command=video_command(args,out);command[-1]=str(out/'shadow-only-native.mp4')
            encoder=subprocess.Popen(command,stdout=log,stderr=subprocess.STDOUT)
    write(out/'environment.json',{k:v for k,v in env.items() if k.startswith('PLACES_')})
    with (out/'import.log').open('w') as log:
        imported=subprocess.run([str(binary),'--import',str(package)],cwd=ROOT,env=env,stdout=log,stderr=subprocess.STDOUT,timeout=60)
    if imported.returncode:raise RuntimeError((out/'import.log').read_text())
    with (out/'native.log').open('w') as log:
        r=subprocess.run([str(binary)],cwd=ROOT,env=env,stdout=log,stderr=subprocess.STDOUT,timeout=180)
    encoder_exit=encoder.wait(timeout=30) if encoder else None
    if encoder:(out/'native-video.fifo').unlink()
    if not (out/'trace.jsonl').exists():raise RuntimeError((out/'native.log').read_text())
    events=[json.loads(line) for line in (out/'trace.jsonl').read_text().splitlines()]
    samples={e['event'].split(':',1)[1]:json.loads(e['detail']) for e in events if e['event'].startswith('native_capture:')}
    write(out/'snapshots.json',samples)
    receipt=dict(exit=r.returncode,encoder_exit=encoder_exit,binary=str(binary),package=str(package),
        binary_sha256=hashlib.file_digest(binary.open('rb'),'sha256').hexdigest(),
        package_sha256=hashlib.file_digest(package.open('rb'),'sha256').hexdigest(),
        captures=list(samples),focus={name:s.get('window_native_focus') for name,s in samples.items()},
        shutdown=bool(events) and events[-1]['event']=='shutdown_complete',
        provenance='Direct native executable; ordinary action/input handlers and diagnostic camera setup. Native framebuffer video; no synthesized frames. Capture pacing is not a performance benchmark.')
    if args.case=='performance':
        receipt['benchmark']=[json.loads(line.split(' ',1)[1]) for line in (out/'native.log').read_text().splitlines() if line.startswith('BENCH_SUMMARY ')][-1]
        receipt['presentation']=[line for line in (out/'native.log').read_text().splitlines() if line.startswith(('[window]', '[wgpu] present mode', '[renderer]'))]
    write(out/'receipt.json',receipt);print(json.dumps(receipt),flush=True)
    if r.returncode or encoder_exit not in (None,0):raise SystemExit(1)


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--binary',type=Path,required=True);p.add_argument('--package',type=Path,required=True)
    p.add_argument('--level',required=True);p.add_argument('--out',type=Path,required=True)
    p.add_argument('--case',choices=['acceptance','video','compare','performance','default-pose'],default='acceptance')
    run(p.parse_args())
