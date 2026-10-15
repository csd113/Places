#!/usr/bin/env python3
"""Explicit native rail/fence campaign; uses supplied binaries and packages.

No build, map bake, source changes, or archived-map discovery. Camera captures
and held gameplay controls use the production Metal player. Run serially in
an active desktop session. Receipts distinguish captures from frame timings.
"""
from __future__ import annotations

import argparse
import csv
import hashlib
import json
import os
from pathlib import Path
import statistics
import subprocess

ROOT = Path(__file__).resolve().parents[2]
# level, eye x/y/z/yaw, yaw/pitch, held controls, simulation seconds
CASES = {
    'office-slope': ('office_demo', [26.5, 3.4, 9, 270], [270, -22], '', .8),
    'office-joint': ('office_demo', [26.4, 5.2, 15.8, 325], [325, -24], '', .8),
    'office-upper': ('office_demo', [32.5, 8.8, 2.4, 180], [180, -20], '', .8),
    'office-up': ('office_demo', [26.5, 1.6, 2.5, 180], [180, -5], 'forward@0-5', 5.1),
    'office-down': ('office_demo', [32.5, 8.8, 2.5, 180], [180, -5], 'forward@0-5', 5.1),
    'office-side': ('office_demo', [26.5, 3.4, 9, 270], [270, -16], 'forward@0-2', 2.1),
    'office-jump': ('office_demo', [26.5, 3.4, 9, 270], [270, -16], 'forward@0-2,jump@.25-.32', 2.1),
    'winter-fence': ('winter', [14.9, 1.6, -11, 90], [90, -20], '', 1.2),
    'winter-end': ('winter', [15.3, 1.6, -16.4, 145], [145, -20], '', 1.2),
    'winter-porch': ('winter', [-13.5, 1.6, -5.4, 0], [0, -12], '', 1.2),
    'winter-side': ('winter', [15.2, 1.6, -10, 90], [90, -8], 'forward@0-2,backward@2.2-2.8', 3),
    'winter-jump': ('winter', [15.2, 1.6, -10, 90], [90, -8], 'forward@0-2,jump@.28-.35', 2.1),
    'winter-ramp': ('winter', [-11.5, 1.6, -3.5, 0], [0, -8], 'forward@0-3,backward@3.2-6.2', 6.3),
    'outdoor-fence': ('lantern_hollow', [-29.9, 1.6, 11.4, 0], [0, -18], '', 1.2),
    'outdoor-side': ('lantern_hollow', [-29.9, 1.6, 11.4, 0], [0, -8], 'forward@0-2', 2.1),
}


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--packages', type=Path, default=ROOT/'assets/levels')
    parser.add_argument('--asset-source', type=Path, default=ROOT/'assets')
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--cases', default='office-slope,office-joint,winter-fence,winter-porch,outdoor-fence')
    parser.add_argument('--lighting', choices=('off', 'full'), default='full')
    parser.add_argument('--flashlight', action='store_true')
    parser.add_argument('--performance', action='store_true')
    args = parser.parse_args()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    binary = args.binary.resolve()
    packages = args.packages.resolve()
    assets = out/'payload/assets'
    (assets/'levels').mkdir(parents=True, exist_ok=True)
    for source in args.asset_source.resolve().iterdir():
        if source.name != 'levels' and not (assets/source.name).exists():
            (assets/source.name).symlink_to(source, target_is_directory=source.is_dir())
    results = []
    for case in args.cases.split(','):
        level, spawn, camera, movement, seconds = CASES[case]
        package = packages/(level+'.placesmap')
        link = assets/'levels'/package.name
        if not link.exists():
            link.symlink_to(package)
        state = out/(case+'-state')
        state.mkdir(exist_ok=True)
        (state/'settings.json').write_text(json.dumps(dict(
            bindings=dict(forward='W', backward='S', strafe_left='A', strafe_right='D',
                          look_up='UP', look_down='DOWN', look_left='LEFT', look_right='RIGHT',
                          jump='SPACE', crouch='C', interact='E', flashlight='F'),
            quality='high', texture_filtering='high',
            lightmaps=args.lighting, reflections='off', bloom=True, vsync=not args.performance,
            window_mode='windowed', window_width=800, window_height=450, fov_degrees=65,
            walk_speed=3, look_speed_h=90, look_speed_v=60)))
        env = {k:v for k,v in os.environ.items() if not k.startswith('PLACES_')}
        env.update(PLACES_LEVEL=level, PLACES_ASSET_ROOT=str(out/'payload'),
            PLACES_STATE_ROOT=str(state), PLACES_BENCH='1', PLACES_VERBOSE='1',
            PLACES_SPAWN=','.join(map(str, spawn)), PLACES_CAMERA=','.join(map(str, camera)),
            PLACES_BENCH_FRAMES='300' if args.performance else '2400',
            PLACES_BENCH_WARMUP='60' if args.performance else '0',
            PLACES_BENCH_OUT=str(out/(case+'-frames.csv')),
            PLACES_LOAD_TRACE=str(out/(case+'-load.jsonl')))
        if args.flashlight:
            movement = 'flashlight@0-.035'+(','+movement if movement else '')
        env['PLACES_MOVE_SCRIPT'] = movement
        if args.performance:
            env.update(PLACES_BENCH_FINISH='1', PLACES_VSYNC='off')
        else:
            env.update(PLACES_BENCH_FIXED_DELTA_SECONDS='0.016666667',
                PLACES_CAPTURE_TIME=str(seconds), PLACES_CAPTURE=str(out/(case+'.png')),
                PLACES_STATE_LOG=str(out/(case+'-player.csv')))
        result = subprocess.run([str(binary)], cwd=ROOT, env=env, text=True,
            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=180, check=False)
        (out/(case+'.log')).write_text(result.stdout)
        if result.returncode or 'backend: Metal' not in result.stdout:
            raise RuntimeError(f'{case}: native failure; inspect saved log')
        for failure in ('Validation Error', 'panicked', 'fatal device', 'using the catalogue placeholder', 'not a valid settings'):
            if failure in result.stdout:
                raise RuntimeError(f'{case}: {failure}')
        events = [json.loads(line) for line in (out/(case+'-load.jsonl')).read_text().splitlines()]
        worlds = [json.loads(e['detail']) for e in events if e['event'] == 'world_committed']
        ready = sum(e['event'] == 'present' and e['detail'] == 'ready' for e in events)
        missed = sum(e['event'] == 'surface_not_presented' and e['detail'] == 'ready' for e in events)
        if not worlds or worlds[-1]['current_level_id'] != level or not ready or missed:
            raise RuntimeError(f'{case}: missing world/presentation evidence')
        if args.flashlight and not any(e['event'] == 'flashlight_toggled'
                and json.loads(e['detail']).get('flashlight_on') for e in events):
            raise RuntimeError(f'{case}: flashlight control was not witnessed')
        frames = list(csv.DictReader((out/(case+'-frames.csv')).open()))
        if not frames or any(int(row['draw_calls']) <= 0 for row in frames):
            raise RuntimeError(f'{case}: missing world draws')
        if args.performance and len(frames) != 300:
            raise RuntimeError(f'{case}: incomplete 300-frame measurement')
        receipt = dict(case=case, lighting=args.lighting, flashlight=args.flashlight,
            binary_sha256=digest(binary), package_sha256=digest(package),
            ready_presentations=ready, frames=len(frames), performance=args.performance,
            metrics={key:statistics.median(float(row[key]) for row in frames)
                     for key in ('frame_ms','render_ms','update_ms','draw_calls','total_vertices',
                                 'texture_binds','material_changes') if key in frames[0]})
        gpu = {row['gpu_sample_id']:float(row['gpu_render_ms']) for row in frames
               if row.get('gpu_timing_status') == 'measured' and row.get('gpu_render_ms')}
        receipt['gpu_render_ms_median'] = statistics.median(gpu.values()) if gpu else None
        receipt['gpu_samples'] = len(gpu)
        if not args.performance:
            capture = out/(case+'.png')
            if not capture.is_file():
                raise RuntimeError(f'{case}: no native capture')
            points = [tuple(map(float,row[1:4])) for row in csv.reader((out/(case+'-player.csv')).open())
                      if float(row[2]) != 0]
            receipt.update(capture_sha256=digest(capture), final_player=points[-1],
                           player_bounds=[[min(axis),max(axis)] for axis in zip(*points)])
            xs, ys, zs = zip(*points)
            checks = {
                'office-up': abs(ys[-1]-5.2) < .03,
                'office-down': abs(ys[-1]-5.2) < .03,
                'office-side': min(xs) >= 25.35 and min(ys) > 3.3,
                'office-jump': min(xs) >= 25.35 and max(ys) > 3.8,
                'winter-side': 16.3 < max(xs) < 16.55 and xs[-1] < 15.5,
                # z=-10 aims at a shared capped post; jumping must not tunnel
                # through that full-height support between the lower rails.
                'winter-jump': max(xs) < 16.6 and max(ys) > 2.3,
                'winter-ramp': max(ys) >= 2.19 and min(ys) >= 1.59 and zs[-1] > -3.7,
                'outdoor-side': min(zs) >= 9.95 and min(ys) >= 1.59,
            }
            if case in checks:
                receipt['movement_passed'] = checks[case]
                if not checks[case]:
                    (out/(case+'-failed.json')).write_text(json.dumps(receipt, indent=2)+'\n')
                    raise RuntimeError(f'{case}: movement acceptance failed; inspect saved trajectory')
        results.append(receipt)
        (out/'summary.json').write_text(json.dumps(results, indent=2)+'\n')
        print(json.dumps(receipt), flush=True)


if __name__ == '__main__':
    main()
