#!/usr/bin/env python3
"""Play or film the explicitly temporary place-and-link elevator demo using a direct binary.

This opt-in tool does not build, bake, use app bundles or alter the normal
installation. Recording contains renderer frames and game audio only.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import time

ROOT = Path(__file__).resolve().parents[2]
LOBBY = 'ready:elevator_lobby'
ANNEX = 'ready:elevator_annex'


def route():
    result = []
    def add(ms, kind, after=LOBBY, **kw):
        result.append(dict(after=after, delay_ms=ms, action=dict(kind=kind, **kw)))
    def click(ms, after=LOBBY):
        add(ms,'mouse',after,button='left',pressed=True)
        add(ms+70,'mouse',after,button='left',pressed=False)
    def view(ms, yaw=0, pitch=0, after=LOBBY):
        add(ms,'view',after,yaw=yaw,pitch=pitch)
    def walk(ms, duration, after=LOBBY):
        add(ms,'key',after,key='W',pressed=True)
        add(ms+duration,'key',after,key='W',pressed=False)
    add(50,'player',position=[0,1.6,-2.65],yaw=180,pitch=0)
    add(100,'view_at',position=[0,1.15,0])
    add(950,'player',position=[-.96,1.6,-1],yaw=180,pitch=0)
    add(1000,'view_at',position=[-.96,1.08,-.12]);click(1200)
    add(2700,'player',position=[0,1.6,-1],yaw=180,pitch=0)
    walk(2900,1200)
    add(4200,'view_at',position=[.94,1.58,.40]);click(4700)
    view(4900)
    add(13500,'checkpoint',name='upper')
    walk(14000,900)
    add(15600,'view_at',position=[10,1.2,-4.6])
    add(16000,'checkpoint',name='upper_hall')
    view(16500,yaw=180)
    walk(16800,900)
    add(18300,'view_at',position=[10.94,1.58,.40]);click(19000)
    view(19200)
    add(28000,'checkpoint',name='ground_return')
    add(29000,'view_at',position=[.94,1.47,.40]);click(29700)
    view(29900)
    add(100,'capture','request:elevator_annex',name='frozen-loading')
    add(1600,'checkpoint',ANNEX,name='annex_arrival')
    walk(2000,900,ANNEX)
    add(3200,'view_at',ANNEX,position=[0,1.2,-4.6])
    add(3600,'checkpoint',ANNEX,name='annex_hall')
    view(4200,yaw=180,after=ANNEX)
    walk(4500,900,ANNEX)
    add(5800,'view_at',ANNEX,position=[.94,1.58,.40]);click(6400,ANNEX)
    view(6600,after=ANNEX)
    add(11200,'checkpoint',ANNEX,name='lobby_return')
    add(11600,'capture',ANNEX,name='last-frame')
    add(12400,'quit',ANNEX)
    add(59000,'quit')
    return result


def setup(args):
    out = args.out.resolve()
    out.mkdir(parents=True,exist_ok=args.play)
    state = out / 'state'; state.mkdir(exist_ok=args.play)
    levels = state / 'levels'; levels.mkdir(exist_ok=args.play)
    shutil.copy2(args.package,levels/'elevator-lab.placesmap')
    settings = json.loads((ROOT/'tests/fixtures/native/hero-manifest.json').read_text())['settings']
    settings.update(window_width=640 if not args.play else 1100, window_height=360 if not args.play else 700,
                    quality='high',texture_filtering='high',lightmaps='full',reflections='off',bloom=False,
                    vsync=bool(args.play),fov_degrees=65,walk_speed=2,ambient_volume=0,master_volume=.75,effects_volume=.8)
    if not (state/'settings.json').exists():
        (state/'settings.json').write_text(json.dumps(settings))
    env = {k:v for k,v in os.environ.items() if not k.startswith('PLACES_')}
    env.update(PLACES_ASSET_ROOT=str(ROOT/'assets'),PLACES_STATE_ROOT=str(state),PLACES_LEVEL='elevator_lobby')
    return out,state,env


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary',type=Path,required=True)
    parser.add_argument('--package',type=Path,required=True)
    parser.add_argument('--out',type=Path,required=True)
    parser.add_argument('--play',action='store_true',help='Play normally; retain this isolated demo save/settings folder')
    parser.add_argument('--ffmpeg',type=Path,default=Path(shutil.which('ffmpeg') or 'ffmpeg'))
    args=parser.parse_args()
    binary=args.binary.resolve(strict=True);args.package=args.package.resolve(strict=True)
    out,state,env=setup(args)
    if args.play:
        raise SystemExit(subprocess.call([str(binary)],cwd=ROOT,env=env))
    actions=out/'actions.json';actions.write_text(json.dumps(route(),indent=2)+'\n')
    trace=out/'trace.jsonl';fifo=state/'native-video.fifo';os.mkfifo(fifo,mode=0o600)
    env.update(PLACES_BENCH='1',PLACES_BENCH_FRAMES='6000',PLACES_BENCH_WARMUP='3',PLACES_VERBOSE='1',
               PLACES_PREPARE_DELAY_MS='500',PLACES_LOAD_TRACE=str(trace),PLACES_BENCH_ACTIONS=str(actions),
               PLACES_BENCH_OUT=str(out/'frames.csv'),PLACES_AUDIO_CAPTURE='1',
               PLACES_BENCH_VIDEO_PIPE=str(fifo),PLACES_BENCH_VIDEO_SIZE='1280x720',PLACES_BENCH_VIDEO_FRAMES='1800',
               PLACES_BENCH_VIDEO_STRIDE='2')
    encoder=[str(args.ffmpeg),'-hide_banner','-nostdin','-n','-f','rawvideo','-pixel_format','rgba','-video_size','1280x720',
             '-framerate','30','-i',str(fifo),'-an','-c:v','libx264','-preset','veryfast','-crf','18','-threads','2',
             '-vf','scale=in_range=pc:out_range=tv:out_color_matrix=bt709,format=yuv420p',
             '-colorspace','bt709','-color_primaries','bt709','-color_trc','bt709','-color_range','tv',
             '-movflags','+faststart',str(out/'native-gameplay.mp4')]
    started=time.monotonic()
    with (out/'encoder.log').open('w') as elog,(out/'game.log').open('w') as glog:
        enc=subprocess.Popen(encoder,stdout=elog,stderr=subprocess.STDOUT)
        game=subprocess.Popen([str(binary)],cwd=ROOT,env=env,stdout=glog,stderr=subprocess.STDOUT)
        try:
            game.wait(timeout=100)
            if game.returncode: raise RuntimeError(f'Game exited {game.returncode}; see game.log')
            enc.wait(timeout=20)
            if enc.returncode: raise RuntimeError(f'Encoder exited {enc.returncode}; see encoder.log')
        finally:
            for proc in (game,enc):
                if proc.poll() is None:
                    proc.terminate();proc.wait(timeout=10)
            fifo.unlink(missing_ok=True)
    finish_capture(args,out,state,binary,started)


def finish_capture(args,out,state,binary,started):
    trace=out/'trace.jsonl'
    rows=[json.loads(line) for line in trace.read_text().splitlines()]
    frames=[json.loads(row['detail']) for row in rows if row['event']=='native_video_frame']
    assert len(frames)>600,'Missing native video frames'
    checkpoints={row['event'].removeprefix('checkpoint:'):json.loads(row['detail']) for row in rows if row['event'].startswith('checkpoint:')}
    assert abs(checkpoints['upper']['player_position'][0]-10)<.05 and abs(checkpoints['upper']['player_position'][1]-1.6)<.05,checkpoints['upper']
    assert checkpoints['upper']['current_level_id']=='elevator_lobby'
    assert abs(checkpoints['ground_return']['player_position'][0])<.05 and abs(checkpoints['ground_return']['player_position'][1]-1.6)<.05,checkpoints['ground_return']
    assert checkpoints['annex_arrival']['current_level_id']=='elevator_annex'
    assert checkpoints['lobby_return']['current_level_id']=='elevator_lobby'
    for name in ('upper_hall','annex_hall'):
        assert checkpoints[name]['player_position'][2]<0,checkpoints[name]
    audio=state/'native-audio.wav'
    probe=shutil.which('ffprobe')
    if not probe:
        raise RuntimeError('ffprobe is required to read the native float-WAV sample rate')
    audio_rate=int(subprocess.check_output([probe,'-v','error','-select_streams','a:0',
        '-show_entries','stream=sample_rate','-of','default=nw=1:nk=1',str(audio)],text=True).strip())
    offset=frames[0]['audio_output_frames']/audio_rate
    stills=out/'native-frames';stills.mkdir()
    subprocess.run([str(args.ffmpeg),'-hide_banner','-nostdin','-i',str(out/'native-gameplay.mp4'),'-fps_mode','passthrough',str(stills/'%05d.png')],check=True,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
    events=[row for row in rows if row['event']=='native_video_frame']
    concat=out/'native-timing.ffconcat'
    lines=['ffconcat version 1.0']
    for i,event in enumerate(events):
        dt=(events[i+1]['elapsed_ms']-event['elapsed_ms'])/1000 if i+1<len(events) else 1/30
        lines.extend([f"file '{stills.name}/{i+1:05d}.png'",'option framerate 1000',f'duration {dt:.6f}'])
    lines.extend([f"file '{stills.name}/{len(events):05d}.png'",'option framerate 1000'])
    concat.write_text('\n'.join(lines)+'\n')
    mux=[str(args.ffmpeg),'-hide_banner','-nostdin','-n','-safe','0','-f','concat','-i',str(concat),'-ss',str(offset),'-i',str(audio),
         '-map','0:v:0','-map','1:a:0','-r','30','-c:v','libx264','-preset','fast','-crf','18','-threads','2','-pix_fmt','yuv420p',
         '-c:a','aac','-b:a','160k','-shortest','-movflags','+faststart',str(out/'elevator-gameplay.mp4')]
    subprocess.run(mux,check=True,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
    receipt=dict(binary=str(binary),binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
                 package_sha256=hashlib.sha256(args.package.read_bytes()).hexdigest(),native_frames=len(frames),
                 drawable=[1280,720],fps=30,audio_offset_seconds=offset,capture_seconds=(events[-1]['elapsed_ms']-events[0]['elapsed_ms'])/1000,
                 workflow_seconds=time.monotonic()-started,
                 checkpoints=checkpoints,checks=['same-height logical-floor travel and return without reloading','walk into upper hall','annex loading and walk out','return level loading'],
                 scope='Actual direct-binary renderer and native game audio. Initial establishing shots use explicit acceptance camera positions; route walking and controls use ordinary input. No desktop capture.')
    (out/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
    print(json.dumps(dict(video=str(out/'elevator-gameplay.mp4'),frames=len(frames),checks=receipt['checks'])))


if __name__=='__main__':main()
