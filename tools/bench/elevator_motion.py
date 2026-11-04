#!/usr/bin/env python3
"""Opt-in lossless native elevator opening sweep, with every rendered frame.

Supply direct executables and a tiny exported elevator-lab package. This never
builds Rust, bakes a map, captures the desktop or alters a normal installation.
The Unix FIFO FFV1 stream is lossless; trace frame times, rather than its nominal
60 fps, are authoritative. PNGs permit inspection without video loss.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[2]
VIEWS = {
    'center': ([0, 1.6, 1.2], [0, 1.45, -.3]),
    'near': ([0, 1.6, .82], [0, 1.45, -.3]),
    'far': ([0, 1.6, 1.9], [0, 1.3, -.3]),
    'left': ([-.55, 1.6, 1.2], [0, 1.45, -.3]),
    'right': ([.55, 1.6, 1.2], [0, 1.45, -.3]),
    'near-left': ([-.5, 1.6, .82], [.65, 1.45, .1]),
    'near-right': ([.5, 1.6, .82], [-.65, 1.45, .1]),
    'lintel': ([.5, 1.6, 1.2], [-.45, 2.15, .1]),
    'sill': ([-.5, 1.6, 1.5], [.45, .2, .1]),
    'outside': ([0, 1.6, -2], [0, 1.3, .3]),
    'outside-left': ([-1.4, 1.6, -1.5], [-.6, 1.45, .15]),
    'outside-right': ([1.4, 1.6, -1.5], [.6, 1.45, .15]),
}


def capture(args):
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    state = out / 'state'
    (state / 'levels').mkdir(parents=True)
    shutil.copy2(args.package, state / 'levels/lab.placesmap')
    settings = json.loads((ROOT / 'tests/fixtures/native/hero-manifest.json').read_text())['settings']
    settings.update(window_width=640, window_height=360, quality='high',
                    texture_filtering=args.filtering, lightmaps=args.lightmaps,
                    reflections=args.reflections, bloom=args.bloom, vsync=False, fov_degrees=args.fov,
                    ambient_volume=0, master_volume=.6, effects_volume=.8)
    (state / 'settings.json').write_text(json.dumps(settings))
    position, target = VIEWS[args.view]
    actions = []

    def add(ms, kind, **details):
        actions.append(dict(after='ready:elevator_lobby', delay_ms=ms,
                            action=dict(kind=kind, **details)))

    add(0, 'focus', focused=True)
    add(20, 'player', position=[0, 1.6, 1.2] if args.observe_departure else position, yaw=0, pitch=0)
    if args.arrival:
        # Stay inside throughout the sealed simulated ride and the entire arrival
        # opening. Only initial acceptance positioning bypasses ordinary input.
        add(100, 'view_at', position=[.94, 1.58, .4])
        add(200, 'mouse', button='left', pressed=True)
        add(270, 'mouse', button='left', pressed=False)
        if args.observe_departure:
            # An explicit acceptance observer move after an ordinary cabin
            # button press exposes the source deck and shell throughout.
            add(290, 'player', position=position, yaw=0, pitch=0)
        add(300, 'view_at', position=target)
        poses = (400, 1250, 2500, 3500, 3800, 4000, 4200, 4400,
                 4600, 4800, 5000, 5200, 5500, 6000)
    else:
        # Call through the ordinary hall button, then inspect from inside. The
        # explicit camera reposition is kept in the trace and receipt.
        add(180, 'player', position=[-.96, 1.6, -1], yaw=180, pitch=0)
        add(200, 'view_at', position=[-.96, 1.08, -.12])
        add(300, 'mouse', button='left', pressed=True)
        add(370, 'mouse', button='left', pressed=False)
        add(400, 'player', position=position, yaw=0, pitch=0)
        add(410, 'view_at', position=target)
        poses = (440, 750, 1000, 1250, 1500, 1750, 2000, 2300)
    for ms in poses:
        add(ms, 'checkpoint', name=f'pose-{ms}')
    add(6500 if args.arrival else 2500, 'capture', name='open-native')
    add(7300 if args.arrival else 3300, 'quit')
    (out / 'actions.json').write_text(json.dumps(actions, indent=2) + '\n')
    fifo = state / 'video.fifo'
    os.mkfifo(fifo, 0o600)
    env = {k: v for k, v in os.environ.items() if not k.startswith('PLACES_')}
    env.update(PLACES_ASSET_ROOT=str(args.asset_root.resolve()),
               PLACES_STATE_ROOT=str(state), PLACES_LEVEL='elevator_lobby',
               PLACES_BENCH='1', PLACES_BENCH_FRAMES='6000', PLACES_BENCH_WARMUP='3',
               PLACES_VERBOSE='1', PLACES_AUDIO_CAPTURE='1', PLACES_LOAD_TRACE=str(out / 'trace.jsonl'),
               PLACES_BENCH_ACTIONS=str(out / 'actions.json'),
               PLACES_BENCH_VIDEO_PIPE=str(fifo), PLACES_BENCH_VIDEO_SIZE='1280x720',
               PLACES_BENCH_VIDEO_FRAMES='600', PLACES_BENCH_VIDEO_STRIDE='1')
    if args.diagnostic != 'final':
        env['PLACES_VISUAL_DIAGNOSTIC'] = args.diagnostic
    with (out / 'encoder.log').open('w') as log, (out / 'game.log').open('w') as game_log:
        encoder = subprocess.Popen([
            args.ffmpeg, '-v', 'error', '-nostdin', '-n', '-f', 'rawvideo',
            '-pixel_format', 'rgba', '-video_size', '1280x720', '-framerate', '60',
            '-i', str(fifo), '-an', '-c:v', 'ffv1', '-level', '3', '-threads', '2',
            str(out / 'native.mkv'),
        ], stdout=log, stderr=subprocess.STDOUT)
        game = None
        try:
            game = subprocess.Popen([str(args.binary.resolve())], cwd=ROOT, env=env,
                                    stdout=game_log, stderr=subprocess.STDOUT)
            game.wait(timeout=40)
            if game.returncode:
                raise RuntimeError(f'Native game exited {game.returncode}; see {out}')
            encoder.wait(timeout=10)
            if encoder.returncode:
                raise RuntimeError(f'Encoder exited {encoder.returncode}; see {out}')
        finally:
            for process in (game, encoder):
                if process is not None and process.poll() is None:
                    process.terminate()
                    process.wait(timeout=10)
            fifo.unlink(missing_ok=True)
    rows = [json.loads(line) for line in (out / 'trace.jsonl').read_text().splitlines()]
    checkpoints = {row['event']: json.loads(row['detail']) for row in rows
                   if row['event'].startswith('checkpoint:')}
    final = checkpoints[f'checkpoint:pose-{poses[-1]}']
    assert final['elevators'][0]['opening'] == 1
    if args.arrival:
        assert final['elevators'][0]['stop'] == 1
        assert abs(final['player_position'][1] - (1.6 if args.observe_departure else 5.1)) < .01
        # The reported floating deck must never recur at an intermediate pose.
        # The CPU replay also checks actual renderer parts at every simulation
        # tick; these checkpoints establish the same invariant in the binary.
        poses_seen = [data['elevators'][0] for data in checkpoints.values()]
        assert any(lift['phase'] == 'moving' for lift in poses_seen)
        assert all(min(abs(lift['y']), abs(lift['y']-3.5)) < .0001 for lift in poses_seen)
        if not args.observe_departure:
            assert all(abs(data['player_position'][1]-data['elevators'][0]['y']-1.6) < .01
                       for data in checkpoints.values())
    frames = out / 'frames'
    frames.mkdir()
    subprocess.run([args.ffmpeg, '-v', 'error', '-nostdin', '-i', str(out / 'native.mkv'),
                    '-fps_mode', 'passthrough', str(frames / '%05d.png')], check=True)
    events = [row for row in rows if row['event'] == 'native_video_frame']
    count = len(list(frames.glob('*.png')))
    assert count == len(events) and count > 100, (count, len(events))
    receipt = dict(view=args.view, position=position, target=target,
                   lightmaps=args.lightmaps, filtering=args.filtering, diagnostic=args.diagnostic,
                   reflections=args.reflections, bloom=args.bloom, fov=args.fov,
                   binary_sha256=hashlib.sha256(args.binary.read_bytes()).hexdigest(),
                   package_sha256=hashlib.sha256(args.package.read_bytes()).hexdigest(),
                   frame_count=count, checkpoints=checkpoints, frames=events,
                   arrival=args.arrival, observer=args.observe_departure,
                   scope='Actual direct binary, every rendered frame, lossless FFV1/PNG; '
                         'acceptance camera, ordinary hall or cabin controls. No FPS claim.')
    (out / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
    print(f'{args.view}/{args.lightmaps}/{args.filtering}: {count} native frames; doors fully open')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--package', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--asset-root', type=Path, default=ROOT / 'assets')
    parser.add_argument('--view', choices=VIEWS, default='center')
    parser.add_argument('--lightmaps', choices=['off', 'full'], default='full')
    parser.add_argument('--filtering', choices=['low', 'high'], default='high')
    parser.add_argument('--reflections', choices=['off', 'full'], default='off')
    parser.add_argument('--bloom', action='store_true')
    parser.add_argument('--fov', type=float, default=80)
    parser.add_argument('--arrival', action='store_true', help='Stay inside through the sealed ride, transfer and arrival opening')
    parser.add_argument('--observe-departure', action='store_true',
                        help='With --arrival and an outside view, stage an exterior observer after the cabin press')
    parser.add_argument('--diagnostic', choices=['final', 'albedo', 'vertex-normal'], default='final')
    parser.add_argument('--ffmpeg', default=shutil.which('ffmpeg') or 'ffmpeg')
    args = parser.parse_args()
    if not 45 <= args.fov <= 110:
        parser.error('--fov must be between 45 and 110 degrees')
    if args.observe_departure and not (args.arrival and args.view.startswith('outside')):
        parser.error('--observe-departure requires --arrival and an outside view')
    if args.arrival and args.view.startswith('outside') and not args.observe_departure:
        parser.error('--arrival requires an inside view to remain in the cabin')
    if not hasattr(os, 'mkfifo'):
        parser.error('This lossless capture tool requires a Unix FIFO host')
    if not args.binary.is_file() or not args.package.is_file():
        parser.error('--binary and --package must name existing files')
    if not shutil.which(args.ffmpeg):
        parser.error('FFmpeg is required for lossless capture')
    capture(args)


if __name__ == '__main__':
    main()
