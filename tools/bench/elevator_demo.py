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
        add(ms, 'mouse', after, button='left', pressed=True)
        add(ms + 70, 'mouse', after, button='left', pressed=False)
    def pan(ms, duration, yaw0, yaw1, pitch0=0, pitch1=0, after=LOBBY):
        for offset in range(0, duration, 16):
            t = offset / duration
            eased = t * t * (3 - 2 * t)
            add(ms + offset, 'view', after, yaw=yaw0 + (yaw1-yaw0)*eased,
                pitch=pitch0 + (pitch1-pitch0)*eased)
        add(ms + duration, 'view', after, yaw=yaw1, pitch=pitch1)
    def walk(ms, duration, after=LOBBY, key='W'):
        add(ms, 'key', after, key=key, pressed=True)
        add(ms + duration, 'key', after, key=key, pressed=False)
    # One initial acceptance pose; subsequent travel uses ordinary movement,
    # strafing and buttons. Eased view events avoid hard camera snaps.
    add(50, 'player', position=[0, 1.6, -2.65], yaw=180, pitch=-8)
    pan(100, 450, 180, 210, -8, 0)
    walk(600, 950)
    pan(1650, 350, 210, 180, 0, -30.5)
    add(2010, 'view_at', position=[-.96, 1.08, -.12]); click(2200)
    pan(2450, 350, 180, 180, -30.5, 0)
    walk(2850, 475, key='A')
    walk(3700, 900)
    pan(5100, 700, 180, 67, 0, -1)
    add(5810, 'view_at', position=[.94, 1.58, .4]); click(6000)
    pan(6200, 350, 67, 0, -1, 0)
    add(11200, 'checkpoint', name='upper')
    walk(11250, 1100)
    pan(12300, 350, 0, 0, 0, -5.4)
    add(12700, 'checkpoint', name='upper_hall')
    pan(12900, 900, 0, 180, -5.4, 0)
    walk(13900, 1100)
    pan(15000, 700, 180, 67, 0, -5.4)
    add(15710, 'view_at', position=[10.94, 1.47, .4]); click(16000)
    pan(16200, 350, 67, 0, -5.4, 0)
    add(1600, 'checkpoint', ANNEX, name='annex_arrival')
    walk(1700, 1000, ANNEX)
    add(2750, 'checkpoint', ANNEX, name='annex_hall')
    pan(2800, 700, 0, 180, after=ANNEX)
    walk(3600, 1000, ANNEX)
    pan(4700, 600, 180, 67, 0, -1, after=ANNEX)
    add(5310, 'view_at', ANNEX, position=[.94, 1.58, .4]); click(5500, ANNEX)
    pan(5700, 300, 67, 0, -1, 0, after=ANNEX)
    add(10300, 'checkpoint', ANNEX, name='lobby_return')
    add(10500, 'quit', ANNEX)
    add(44000, 'quit')
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
                    vsync=bool(args.play),fov_degrees=65 if args.play else 75,walk_speed=2,ambient_volume=0,master_volume=.75,effects_volume=.8)
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
    # The native action-script reader intentionally caps input at 64 KiB.
    actions=out/'actions.json';actions.write_text(json.dumps(route(),separators=(',', ':'))+'\n')
    trace=out/'trace.jsonl';fifo=state/'native-video.fifo';os.mkfifo(fifo,mode=0o600)
    env.update(PLACES_BENCH='1',PLACES_BENCH_FRAMES='6000',PLACES_BENCH_WARMUP='3',PLACES_VERBOSE='1',
               PLACES_PREPARE_DELAY_MS='500',PLACES_LOAD_TRACE=str(trace),PLACES_BENCH_ACTIONS=str(actions),
               PLACES_BENCH_OUT=str(out/'frames.csv'),PLACES_AUDIO_CAPTURE='1',
               PLACES_BENCH_VIDEO_PIPE=str(fifo),PLACES_BENCH_VIDEO_SIZE='1280x720',PLACES_BENCH_VIDEO_FRAMES='1800',
               PLACES_BENCH_VIDEO_STRIDE='1')
    encoder=[str(args.ffmpeg),'-hide_banner','-nostdin','-n','-f','rawvideo','-pixel_format','rgba','-video_size','1280x720',
             '-framerate','60','-i',str(fifo),'-an','-c:v','ffv1','-level','3','-threads','2',
             str(out/'native-gameplay.mkv')]
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
    assert abs(checkpoints['lobby_return']['player_position'][0])<.08 and abs(checkpoints['lobby_return']['player_position'][1]-1.6)<.05,checkpoints['lobby_return']
    assert checkpoints['annex_arrival']['current_level_id']=='elevator_annex'
    assert checkpoints['lobby_return']['current_level_id']=='elevator_lobby'
    for name in ('upper_hall','annex_hall'):
        assert checkpoints[name]['player_position'][2]<-.4,checkpoints[name]
    verify_passengers(checkpoints)
    audio=state/'native-audio.wav'
    probe=shutil.which('ffprobe')
    if not probe:
        raise RuntimeError('ffprobe is required to read the native float-WAV sample rate')
    audio_rate=int(subprocess.check_output([probe,'-v','error','-select_streams','a:0',
        '-show_entries','stream=sample_rate','-of','default=nw=1:nk=1',str(audio)],text=True).strip())
    offset=frames[0]['audio_output_frames']/audio_rate
    stills=out/'native-frames';stills.mkdir()
    subprocess.run([str(args.ffmpeg),'-hide_banner','-nostdin','-i',str(out/'native-gameplay.mkv'),'-fps_mode','passthrough',str(stills/'%05d.png')],check=True,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
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
                 checkpoints=checkpoints,checks=['same-height logical-floor travel without reloading','walk into upper hall','annex loading and walk out','return level loading','NPC and prop retain cabin offsets and state through every stop without duplicates','continuous walking and strafing after one initial pose','every rendered frame sampled before 30 fps timestamp-correct delivery'],
                 scope='Actual direct-binary renderer and native game audio. One initial acceptance pose; subsequent movement and controls use ordinary input, with eased view events. No desktop capture or synthetic frames.')
    (out/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
    print(json.dumps(dict(video=str(out/'elevator-gameplay.mp4'),frames=len(frames),checks=receipt['checks'])))


def verify_passengers(checkpoints):
    for name,shift in [('upper',10),('annex_arrival',0),('lobby_return',0)]:
        data=checkpoints[name]
        agents=data['entity_simulation']['agents']
        assert len(agents)==1,(name,agents)
        assert max(abs(a-b) for a,b in zip(agents[0]['position'],[shift+.55,0,1.85]))<.025,(name,agents)
        assert any(['demo_passenger',1] in state['values'] for state in data['entity_simulation']['states']),(name,data['entity_simulation']['states'])
        bodies=data['physics']['bodies']
        assert len(bodies)==1,(name,bodies)
        assert max(abs(a-b) for a,b in zip(bodies[0]['pose']['position'],[shift-.55,.02,1.95]))<.025,(name,bodies)
        if name!='annex_arrival':
            assert agents[0]['id']=='demo_guest',(name,agents)
            assert data['physics']['authored'][0][1]=='demo_crate',(name,data['physics']['authored'])


if __name__=='__main__':main()
