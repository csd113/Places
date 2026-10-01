#!/usr/bin/env python3
"""Capture authored Lantern Hollow views through the native desktop renderer."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import time

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--root', type=Path, required=True)
parser.add_argument('--quality', choices=('low', 'medium', 'high'), default='high')
parser.add_argument('--views', default='street,campfire,pond,sheet-ghost,sheet-cat,overview')
parser.add_argument('--low-lighting', action='store_true')
args = parser.parse_args()
root = args.root.resolve()
views = json.loads((root / 'tools/bench/lantern_hollow_views.json').read_text())
selected = args.views.split(',')
assert all(name in {view['name'] for view in views} for name in selected)
label = args.quality + ('-low-lighting' if args.low_lighting else '')
out = root / 'target/showcase-evidence' / label
out.mkdir(parents=True, exist_ok=True)
manifest = []
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
    env = {key: value for key, value in os.environ.items() if not key.startswith('PLACES_')}
    env.update(PLACES_ASSET_ROOT=str(root), PLACES_STATE_ROOT=str(state),
               PLACES_LEVEL='lantern_hollow', PLACES_QUALITY=args.quality,
               PLACES_SPAWN=','.join(map(str, view['spawn'])),
               PLACES_CAMERA=','.join(map(str, view['camera'])),
               PLACES_BENCH='1', PLACES_BENCH_FRAMES='1200', PLACES_BENCH_WARMUP='120',
               PLACES_CAPTURE=str(path), PLACES_CAPTURE_TIME=str(view.get('capture_time', 1.5)),
               PLACES_BENCH_OUT=str(out / (name + '-frames.csv')),
               PLACES_LOAD_TRACE=str(out / (name + '-load.csv')))
    if name == 'overview':
        env['PLACES_CAPTURE_TIME'] = '0'
    start = time.monotonic()
    result = subprocess.run([str(root / 'target/release/places')], cwd=root, env=env,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                            text=True, timeout=120, check=False)
    (out / (name + '.log')).write_text(result.stdout)
    item = {'view': view, 'quality': args.quality, 'low_lighting': args.low_lighting,
            'capture': str(path), 'exit': result.returncode,
            'elapsed_seconds': time.monotonic() - start, 'captured': path.is_file()}
    manifest.append(item)
    (out / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    print(json.dumps(item), flush=True)
    if result.returncode or not path.is_file():
        raise SystemExit('Native capture failed; inspect ' + str(out / (name + '.log')))
