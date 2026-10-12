#!/usr/bin/env python3
"""Bounded native Office captures, held-control walks and frame measurements.

This explicit developer tool loads a normal compiled package through external
discovery. It is never invoked by default Rust tests or hosted CI. Supply an
output directory; disposable logs and traces remain there.
"""
from __future__ import annotations

import argparse
import csv
import json
import os
from pathlib import Path
import statistics
import subprocess

ROOT = Path(__file__).resolve().parents[2]
# Explicit eye heights select the player storey while preserving ordinary
# body-height support, movement, collision and headroom in every walk.
VIEWS = {
    'top': ([9.5, 8.8, 6.5, 20], [20, -8], '', .7),
    'top-hall': ([-2, 8.8, 9, 90], [90, 0], '', .7),
    'middle': ([7.5, 5.2, 11.5, 0], [0, -3], '', .7),
    'middle-odd': ([2.2, 5.2, 6, 0], [0, -12], '', .7),
    'basement': ([21, 1.6, 10, 0], [0, -3], '', .7),
    'ramps': ([26.5, 5.2, 16.2, 0], [0, 7], '', .7),
    'mannequin-idle': ([21, 1.6, 11.5, 20], [20, -3], '', 8),
    'mannequin-run': ([9, 1.6, 11, 30], [30, -3], '', .8),
    'failure-on': ([8.5, 1.6, 14, 0], [0, -5], '', 1.3),
    'failure-off': ([8.5, 1.6, 14, 0], [0, -5], '', 3),
    'roach-flee': ([3.5, 1.6, 2, 270], [270, -72], '', .7),
    'roach-hidden': ([3.5, 1.6, 2, 270], [270, -72], '', 4),
    'roach-return': ([3.5, 1.6, 2, 270], [270, -72], 'backward@4-5', 24),
    'roach-detail': ([5, 1.6, 4, 0], [0, -22], '', .7),
    'lower-up': ([26.5, 1.6, 2.5, 180], [180, -5], 'forward@0-5', 5.1),
    'lower-down': ([26.5, 5.2, 16, 0], [0, -5], 'forward@0-5', 5.1),
    'upper-up': ([32.5, 5.2, 16, 0], [0, -5], 'forward@0-5', 5.1),
    'upper-down': ([32.5, 8.8, 2.5, 180], [180, -5], 'forward@0-5', 5.1),
    'top-entry': ([-2, 8.8, 9, 90], [90, 0], 'forward@0-2.5', 2.6),
    'top-shaft': ([22, 8.8, 2, 90], [90, 0], 'forward@0-3.5', 3.6),
    'middle-shaft': ([22, 5.2, 16, 90], [90, 0], 'forward@0-1.4', 1.5),
    'basement-entry': ([26.5, 1.6, 2, 270], [270, 0], 'forward@0-2', 2.1),
    'basement-wall': ([9, 1.6, 5, 90], [90, 0], 'forward@0-2', 2.1),
    'walk-through': ([-2, 8.8, 9, 90], None,
        'forward@0-7.4333,look_left@7.6-8.6,forward@8.8-11.1,'
        'look_right@11.3-12.3,forward@12.5-16.5667,look_right@16.8-17.8,'
        'forward@18-22.6333,look_right@22.8-23.8,forward@24-26,'
        'look_right@26.2-27.2,forward@27.4-32.0667,look_left@32.2-33.2,'
        'forward@33.4-35.2333,look_left@35.4-36.4,forward@36.6-38.2,'
        'look_right@38.4-39.4,forward@39.6-41.6,look_left@41.8-42.8,'
        'forward@43-43.7333,look_right@43.9-44.9,forward@45.1-47.2667', 47.6),
}


def summary(rows, field):
    values = sorted(float(row[field]) for row in rows)
    return dict(mean=statistics.mean(values), median=statistics.median(values),
                p95=values[round((len(values)-1)*.95)]) if values else None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--views', default='top,middle,basement,ramps')
    parser.add_argument('--lighting', choices=('static', 'full'), default='full')
    parser.add_argument('--textures', choices=('low', 'medium', 'high'), default='high')
    parser.add_argument('--performance', action='store_true')
    parser.add_argument('--graphics-cycle', default='')
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/release/places')
    args = parser.parse_args()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    payload = out / 'payload'
    assets = payload / 'assets'
    (assets / 'levels').mkdir(parents=True, exist_ok=True)
    for item in (ROOT / 'assets').iterdir():
        if item.name != 'levels' and not (assets / item.name).exists():
            (assets / item.name).symlink_to(item, target_is_directory=item.is_dir())
    package = ROOT / 'assets/levels/office_demo.placesmap'
    (assets / 'levels/office_demo.placesmap').unlink(missing_ok=True)
    (assets / 'levels/office_demo.placesmap').symlink_to(package)
    results = []
    for name in args.views.split(','):
        spawn, camera, movement, seconds = VIEWS[name]
        label = f'{name}-{args.lighting}-{args.textures}'
        state = out / ('state-'+label)
        state.mkdir(exist_ok=True)
        (state / 'settings.json').write_text(json.dumps(dict(
            bindings=dict(forward='W', backward='S', strafe_left='A', strafe_right='D',
                look_up='UP', look_down='DOWN', look_left='LEFT', look_right='RIGHT',
                jump='SPACE', crouch='C', interact='E'),
            quality=args.textures, texture_filtering=args.textures,
            lightmaps='off' if args.lighting == 'static' else 'full', reflections='off',
            bloom=True, vsync=not args.performance, window_mode='windowed',
            window_width=800, window_height=450, walk_speed=3,
            look_speed_h=90, look_speed_v=60, fov_degrees=65)))
        env = {k: v for k, v in os.environ.items() if not k.startswith('PLACES_')}
        env.update(PLACES_LEVEL='office_demo', PLACES_ASSET_ROOT=str(payload),
            PLACES_STATE_ROOT=str(state), PLACES_BENCH='1', PLACES_VERBOSE='1',
            PLACES_BENCH_WARMUP='60' if args.performance else '0',
            PLACES_BENCH_FRAMES='300' if args.performance else str(max(2400, round(seconds*60)+30)),
            PLACES_SPAWN=','.join(map(str, spawn)),
            PLACES_MOVE_SCRIPT=movement, PLACES_STATE_LOG=str(out / (label+'-player.csv')),
            PLACES_BENCH_OUT=str(out / (label+'-frames.csv')),
            PLACES_LOAD_TRACE=str(out / (label+'-load.jsonl')),
            PLACES_NAV_DEBUG=str(out / (label+'-nav')))
        if camera is not None:
            env['PLACES_CAMERA'] = ','.join(map(str, camera))
        if args.performance:
            env.pop('PLACES_NAV_DEBUG', None)
            env.pop('PLACES_STATE_LOG', None)
            env.update(PLACES_BENCH_FINISH='1', PLACES_VSYNC='off')
        else:
            env.update(PLACES_BENCH_FIXED_DELTA_SECONDS='0.016666667',
                       PLACES_CAPTURE=str(out / (label+'.png')), PLACES_CAPTURE_TIME=str(seconds))
        if args.graphics_cycle:
            env['PLACES_BENCH_GRAPHICS_CYCLE'] = args.graphics_cycle
        if name == 'roach-detail':
            env['PLACES_BENCH_CAMERA_POSITION'] = '2.8,0.28,2.6'
        result = subprocess.run([str(args.binary.resolve())], cwd=ROOT, env=env,
            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, timeout=150, check=False)
        (out / (label+'.log')).write_text(result.stdout)
        if result.returncode or 'backend: Metal' not in result.stdout:
            raise RuntimeError(f'{label} failed: {result.stdout[-3000:]}')
        if any(token in result.stdout for token in ('panicked', 'Validation Error', 'fatal device',
                                                    'not a valid settings', 'using the catalogue placeholder')):
            raise RuntimeError(f'{label}: invalid native run')
        events = [json.loads(line) for line in (out / (label+'-load.jsonl')).read_text().splitlines()]
        ready = sum(e['event'] == 'present' and e['detail'] == 'ready' for e in events)
        missed = sum(e['event'] == 'surface_not_presented' and e['detail'] == 'ready' for e in events)
        if not ready or missed:
            raise RuntimeError(f'{label}: {ready} ready presentations, {missed} missed')
        frames = list(csv.DictReader((out / (label+'-frames.csv')).open()))
        points = [tuple(map(float, row[1:4])) for row in csv.reader(
            (out / (label+'-player.csv')).open()) if float(row[2]) != 0] if not args.performance else [tuple(spawn[:3])]
        item = dict(view=name, lighting=args.lighting, textures=args.textures,
            presented=ready, frame_rows=len(frames), frame_ms=summary(frames, 'frame_ms'),
            render_ms=summary(frames, 'render_ms'), update_ms=summary(frames, 'update_ms'),
            final_player=points[-1], player_bounds=[[min(axis), max(axis)] for axis in zip(*points)])
        if name in ('lower-up', 'upper-down', 'middle-shaft'):
            assert abs(points[-1][1]-5.2) < .03, f'{name}: middle landing not reached'
        elif name in ('upper-up', 'top-entry', 'top-shaft'):
            assert abs(points[-1][1]-8.8) < .03, f'{name}: top landing not reached'
        elif name in ('lower-down', 'basement-entry', 'basement-wall'):
            assert abs(points[-1][1]-1.6) < .03, f'{name}: basement support lost'
        elif name == 'walk-through':
            assert min(p[1] for p in points) >= 1.59, 'walk-through: unintended floor leak'
            assert abs(points[-1][0]-8.5) < .4 and abs(points[-1][2]-9) < .4, 'walk-through: route blocked'
            assert abs(points[-1][1]-1.6) < .03, 'walk-through: basement not reached'
        results.append(item)
        (out / 'summary.json').write_text(json.dumps(results, indent=2)+'\n')
        print(json.dumps(item), flush=True)


if __name__ == '__main__':
    main()
