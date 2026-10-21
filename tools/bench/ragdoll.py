#!/usr/bin/env python3
"""Explicit native ragdoll acceptance on an already compiled editor project.

No map bake, Cargo integration, quality reduction or frame limiter. Recording is
an optional separate 10 Hz diagnostic and must not be used as throughput data.
Every activation goes through the ordinary console or authored JSON/Luau action.
"""
from pathlib import Path
import argparse
import hashlib
import json
import os
import subprocess
import time

ROOT = Path(__file__).resolve().parents[2]
ACTORS = ['floor_actor', 'wall_actor', 'stair_actor', 'prop_actor',
          'extra_1', 'extra_2', 'extra_3', 'extra_4']


def actions_for(case):
    steps = []
    anchor = 'ready:ragdoll_lab'

    def action(ms, kind, **fields):
        steps.append(dict(after=anchor, delay_ms=ms, action=dict(kind=kind, **fields)))

    def key(ms, name):
        action(ms, 'key', key=name, pressed=True)
        action(ms + 12, 'key', key=name, pressed=False)

    def command(ms, text):
        key(ms, '`')
        action(ms + 35, 'text', text=text, submit=True)
        key(ms + 80, 'Escape')

    def cap(ms, name):
        action(ms, 'capture', name=name)

    action(50, 'console_enabled', enabled=True)
    if case == 'demo':
        action(100, 'view_at', position=[-3.6, .7, 1.7])
        key(500, 'F'); cap(1200, 'animated')
        command(2200, '/ragdoll floor_actor on 1.5 0 -0.4')
        cap(2500, 'falling'); cap(3800, 'contact'); cap(18000, 'settled')
        command(19000, '/ragdoll floor_actor reset'); cap(19900, 'reset-animated')
        command(21800, '/trigger script_demo'); cap(22200, 'repeat-falling')
        cap(37500, 'repeat-settled'); command(38000, '/ragdoll floor_actor reset')
        cap(38900, 'repeat-reset'); end = 39500
    elif case == 'contacts':
        command(150, '/noclip on')
        for index, (actor, eye, point, trigger) in enumerate([
            ('wall', [-4, 1.8, -.2], [-4, .7, -4], 'drop_wall'),
            ('stair', [-3, 3.1, 2.5], [0, 1.4, -1.5], 'drop_stair'),
            ('prop', [2, 2.2, 3.8], [4, .5, .6], 'drop_prop'),
        ]):
            start = 1000 + index * 19000
            action(start, 'player', position=eye, yaw=0, pitch=-15)
            action(start + 150, 'view_at', position=point)
            cap(start + 500, actor + '-animated')
            command(start + 1000, '/trigger ' + trigger)
            cap(start + 1450, actor + '-impact')
            if actor == 'stair':
                action(start + 16500, 'player', position=[-3, 2.2, -3.5], yaw=0, pitch=-15)
                action(start + 16600, 'view_at', position=[0, .5, -2.2])
            cap(start + 17500, actor + '-settled')
        end = 59000
    elif case == 'stress':
        command(150, '/noclip on')
        action(400, 'player', position=[2, 5, 8], yaw=0, pitch=-25)
        for index in range(3):
            command(1500 + index * 6000, '/trigger drop_all')
            if index == 0: command(2600, '/trigger drop_all')  # duplicate refusal must not allocate
            if index < 2: command(6200 + index * 6000, '/trigger reset_all')
        cap(23500, 'eight-settled'); command(24500, '/trigger reset_all')
        cap(25300, 'eight-reset'); end = 26000
    elif case == 'carry':
        action(100, 'player', position=[-4, 1.6, 4.4], yaw=0, pitch=-40)
        action(500, 'aim_item', id='carry_cushion'); key(700, 'E')
        action(900, 'aim_entity', id='floor_actor'); command(1100, '/trigger carry_demo'); cap(1600, 'socket-carried')
        command(2200, '/ragdoll floor_actor on .8 0 -.3')
        cap(2700, 'carry-release'); cap(11500, 'carry-settled')
        command(12500, '/ragdoll floor_actor reset'); cap(13600, 'carry-reset')
        end = 14500
    elif case == 'lifecycle':
        command(900, '/trigger drop_all'); command(1250, '/physics pause')
        cap(1700, 'physics-paused-before'); cap(3700, 'physics-paused-after')
        command(4100, '/physics step'); cap(4500, 'single-step')
        command(4900, '/physics resume'); cap(6000, 'resumed')
        action(6100, 'quality', level='medium'); cap(7200, 'graphics-medium')
        action(7600, 'quality', level='high'); cap(9000, 'graphics-high')
        key(10000, 'Escape'); cap(10400, 'menu-paused-before'); cap(12400, 'menu-paused-after')
        key(13000, 'Escape')
        # Real map unload through the normal pause menu; then select the exact installed lab via the production loader.
        key(14000, 'Escape'); key(14200, 'Down'); key(14400, 'Down'); key(14600, 'Return')
        cap(16000, 'returned-main'); key(16800, 'Return'); action(17200, 'load', level='ragdoll_lab')
        cap(19500, 'reloaded-authored'); end = 20500
    elif case == 'unsupported':
        command(500, '/ragdoll contact_table on'); cap(1200, 'unsupported-rigid')
        command(1700, '/ragdoll missing_actor on'); cap(2400, 'unsupported-missing')
        command(2800, '/ragdoll floor_actor on nan 0 0'); cap(3500, 'unsupported-velocity')
        end = 4200
    else:
        raise ValueError(case)
    action(end, 'quit')
    return sorted(steps, key=lambda step: step['delay_ms']), end


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/release/places')
    parser.add_argument('--package', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--case', choices=['demo', 'contacts', 'stress', 'carry', 'lifecycle', 'unsupported'], default='demo')
    parser.add_argument('--video', action='store_true')
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True); package = args.package.resolve(strict=True)
    out = args.out.resolve(); out.mkdir(parents=True, exist_ok=False)
    (out / 'levels').mkdir(); (out / 'levels/lab.placesmap').symlink_to(package)
    settings = json.loads((ROOT / 'tests/fixtures/native/retro-arcade-manifest.json').read_text())['settings']
    settings.update(window_width=800, window_height=450, vsync=False, quality='high',
                    texture_filtering='high', lightmaps='full', reflections='full', bloom=True,
                    developer_console=True, visual_style='standard')
    (out / 'settings.json').write_text(json.dumps(settings, indent=2) + '\n')
    steps, end = actions_for(args.case)
    (out / 'actions.json').write_text(json.dumps(steps, indent=2) + '\n')
    env = {key: value for key, value in os.environ.items() if not key.startswith('PLACES_') and key != 'SDL_VIDEODRIVER'}
    env.update(PLACES_ASSET_ROOT=str(ROOT), PLACES_STATE_ROOT=str(out), PLACES_LEVEL='ragdoll_lab',
               PLACES_BENCH='1', PLACES_BENCH_FRAMES='0', PLACES_BENCH_WARMUP='0', PLACES_VERBOSE='1',
               PLACES_BENCH_ACTIONS=str(out / 'actions.json'), PLACES_LOAD_TRACE=str(out / 'trace.jsonl'),
               PLACES_PHYSICS_TRACE='1', PLACES_PHYSICS_LOG=str(out / 'physics.csv'),
               PLACES_BENCH_OUT=str(out / 'frames.csv'), PLACES_BENCH_RESOURCE_TRACE='1')
    encoder = None
    if args.video:
        fifo = out / 'video.fifo'; os.mkfifo(fifo)
        env.update(PLACES_BENCH_VIDEO_PIPE=str(fifo), PLACES_BENCH_VIDEO_SIZE='1600x900',
                   PLACES_BENCH_VIDEO_FRAMES='1800', PLACES_BENCH_VIDEO_STRIDE='1', PLACES_BENCH_VIDEO_INTERVAL_MS='100')
        with (out / 'encoder.log').open('w') as log:
            encoder = subprocess.Popen(['ffmpeg', '-hide_banner', '-loglevel', 'warning', '-y', '-f', 'rawvideo',
                '-pixel_format', 'rgba', '-video_size', '1600x900', '-framerate', '10', '-i', str(fifo),
                '-an', '-c:v', 'libx264', '-preset', 'veryfast', '-crf', '20', '-pix_fmt', 'yuv420p',
                '-movflags', '+frag_keyframe+empty_moov+default_base_moof', str(out / 'ragdoll.mp4')], stdout=log, stderr=subprocess.STDOUT)
    (out / 'environment.json').write_text(json.dumps({key: value for key, value in env.items() if key.startswith('PLACES_')}, indent=2) + '\n')
    started = time.monotonic()
    try:
        with (out / 'native.log').open('w') as log:
            result = subprocess.run([str(binary)], cwd='/tmp', env=env, stdout=log, stderr=subprocess.STDOUT, timeout=end / 1000 + 90)
    finally:
        if encoder:
            try: encoder.wait(timeout=20)
            except subprocess.TimeoutExpired:
                encoder.terminate(); encoder.wait(timeout=10)
    receipt = dict(case=args.case, exit_code=result.returncode, wall_seconds=time.monotonic()-started,
                   binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
                   package_sha256=hashlib.sha256(package.read_bytes()).hexdigest(), video=args.video,
                   video_nominal_hz=10 if args.video else None, performance_claim=False)
    (out / 'process.json').write_text(json.dumps(receipt, indent=2) + '\n')
    print(json.dumps(receipt), flush=True)
    raise SystemExit(result.returncode)


if __name__ == '__main__':
    main()
