#!/usr/bin/env python3
"""Capture authored Winter views through the native desktop renderer."""
import argparse
import csv
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--root', type=Path, required=True)
parser.add_argument('--package-root', type=Path, help='optional isolated package containing assets/')
parser.add_argument('--quality', choices=('low', 'medium', 'high'), default='high')
parser.add_argument('--views', default='square,lodge,pond,forest,interior,overview')
parser.add_argument('--low-lighting', action='store_true')
args = parser.parse_args()
root = args.root.resolve()
package_root = args.package_root.resolve() if args.package_root else root
asset_root = package_root / 'assets'
views = json.loads((root / 'tools/bench/winter_views.json').read_text())
selected = args.views.split(',')
if not all(name in {view['name'] for view in views} for name in selected):
    parser.error('unknown view; see tools/bench/winter_views.json')
label = args.quality + ('-low-lighting' if args.low_lighting else '')
out = root / 'target/winter-evidence' / label
out.mkdir(parents=True, exist_ok=True)
manifest_path = out / 'manifest.json'
manifest = json.loads(manifest_path.read_text()) if manifest_path.is_file() else []
verification = subprocess.run([str(root / 'target/release/places-compile'), 'verify',
                               str(root / 'assets/levels/winter.json'), '--package',
                               str(root / 'assets/levels/winter.placesmap'), '--require-current',
                               '--asset-root', str(asset_root)],
                              cwd=root, check=False, text=True, stdout=subprocess.PIPE,
                              stderr=subprocess.STDOUT)
if verification.returncode:
    raise SystemExit('Compile Winter before capturing:\n' + verification.stdout)
package_hash = hashlib.sha256((root / 'assets/levels/winter.placesmap').read_bytes()).hexdigest()
binary_hash = hashlib.sha256((root / 'target/release/places').read_bytes()).hexdigest()
for name in selected:
    view = next(view for view in views if view['name'] == name)
    state = out / ('state-' + name)
    state.mkdir(exist_ok=True)
    settings = {'bindings': {'forward': 'W', 'backward': 'S', 'strafe_left': 'A',
                            'strafe_right': 'D', 'look_up': 'UP', 'look_down': 'DOWN',
                            'look_left': 'LEFT', 'look_right': 'RIGHT', 'jump': 'SPACE',
                            'crouch': 'C', 'interact': 'E'},
                'look_speed_h': 90.0, 'look_speed_v': 60.0, 'walk_speed': 3.0,
                'invert_look': False, 'mouse_sensitivity': 0.12,
                'quality': args.quality, 'texture_filtering': args.quality,
                'lightmaps': {'low': 'off', 'medium': 'medium', 'high': 'full'}[args.quality],
                'reflections': {'low': 'off', 'medium': 'medium', 'high': 'full'}[args.quality],
                'use_low_quality_lighting': args.low_lighting, 'vsync': True,
                'window_mode': 'windowed', 'window_width': 1920, 'window_height': 1080,
                'fov_degrees': 60.0}
    (state / 'settings.json').write_text(json.dumps(settings))
    path = out / (name + '.png')
    path.unlink(missing_ok=True)  # A stale earlier image must never pass acceptance.
    for suffix in ('-frames.csv', '-load.csv', '-player.csv', '.log'):
        (out / (name + suffix)).unlink(missing_ok=True)
    env = {key: value for key, value in os.environ.items() if not key.startswith('PLACES_')}
    env.update(PLACES_ASSET_ROOT=str(package_root), PLACES_STATE_ROOT=str(state),
               PLACES_LEVEL='winter', PLACES_QUALITY=args.quality,
               PLACES_SPAWN=','.join(map(str, view['spawn'])),
               PLACES_CAMERA=','.join(map(str, view['camera'])),
               PLACES_BENCH='1', PLACES_BENCH_FRAMES='2400', PLACES_BENCH_WARMUP='120',
               PLACES_VERBOSE='1',
               PLACES_CAPTURE=str(path), PLACES_CAPTURE_TIME=str(view.get('capture_time', 1.5)),
               PLACES_BENCH_OUT=str(out / (name + '-frames.csv')),
               PLACES_LOAD_TRACE=str(out / (name + '-load.csv')),
               PLACES_STATE_LOG=str(out / (name + '-player.csv')))
    if 'move_script' in view:
        env['PLACES_MOVE_SCRIPT'] = view['move_script']
    if 'actions' in view:
        actions = state / 'actions.json'
        actions.write_text(json.dumps(view['actions']))
        env['PLACES_BENCH_ACTIONS'] = str(actions)
    if name == 'overview':
        env['PLACES_CAPTURE_TIME'] = '0'
    start = time.monotonic()
    result = subprocess.run([str(root / 'target/release/places')], cwd=root, env=env,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                            text=True, timeout=120, check=False)
    (out / (name + '.log')).write_text(result.stdout)
    item = {'view': view, 'quality': args.quality, 'low_lighting': args.low_lighting,
            'capture': str(path), 'exit': result.returncode,
            'package_sha256': package_hash, 'binary_sha256': binary_hash,
            'elapsed_seconds': time.monotonic() - start, 'captured': path.is_file()}
    item['sky_upload_log'] = [line for line in result.stdout.splitlines() if '[wgpu] sky ' in line]
    trace = out / (name + '-load.csv')
    events = [json.loads(line) for line in trace.read_text().splitlines()] if trace.is_file() else []
    item['winter_presented'] = any(event['event'] == 'scene_presented' and event['detail'] == 'winter'
                                   for event in events)
    worlds = [json.loads(event['detail']) for event in events if event['event'] == 'world_committed']
    item['quality_applied'] = bool(worlds and worlds[-1]['quality'] == args.quality
                                  and worlds[-1]['renderer_quality'] == args.quality
                                  and worlds[-1]['current_level_id'] == 'winter'
                                  and worlds[-1]['renderer_level_id'] == 'winter'
                                  and worlds[-1]['renderer_lightmaps'] == ('off' if args.low_lighting else settings['lightmaps'])
                                  and worlds[-1]['use_low_quality_lighting'] == args.low_lighting)
    if name.startswith('walk-'):
        with (out / (name + '-player.csv')).open() as player_log:
            # The startup game logs a zero pose before committing the ready world.
            points = [tuple(map(float, row[1:4])) for row in csv.reader(player_log) if float(row[2]) != 0]
        if not points:
            raise SystemExit('No ready-world player states for ' + name)
        xs, ys, zs = zip(*points)
        checks = {
            'walk-lodge': max(ys) >= 2.19 and min(zs) < -12 and zs[-1] > -3.7,
            'walk-stairs': max(ys) >= 2.19 and max(xs) > -15.1 and xs[-1] < -18,
            'walk-pond': min(ys) <= 1.45 and max(ys) > 2.3 and max(xs) > 10.5 and xs[-1] < 4.2,
            'walk-forest': min(zs) < -34 and max(ys) < 1.61,
            'walk-ice-coast': 11.8 < xs[-1] < 12.6 and max(ys) < 1.45,
            'walk-snow-stop': 10.8 < xs[-1] < 11.15 and min(ys) > 1.59 and max(ys) < 1.61,
            'walk-ice-wall': 16.3 < xs[-1] < 16.55 and max(ys) < 1.61,
        }
        item['traversal_passed'] = min(ys) >= 1.39 and checks[name]
        item['player_bounds'] = [[min(axis), max(axis)] for axis in (xs, ys, zs)]
        item['player_final'] = list(points[-1])
    manifest = [previous for previous in manifest if previous['view']['name'] != name]
    manifest.append(item)
    (out / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    print(json.dumps(item), flush=True)
    if (result.returncode or not path.is_file() or not item['winter_presented']
            or not item['quality_applied'] or not item.get('traversal_passed', True)):
        raise SystemExit('Native capture failed; inspect ' + str(out / (name + '.log')))
