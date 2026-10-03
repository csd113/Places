#!/usr/bin/env python3
"""Launch a preserved movement fixture, or list its recorded purpose."""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('map_id', nargs='?')
    parser.add_argument('--yaw', type=float)
    parser.add_argument('--binary', type=Path, help='optional preserved before/after build')
    parser.add_argument('--script', default='', help='existing PLACES_MOVE_SCRIPT held controls')
    parser.add_argument('--seconds', type=float, default=0.5)
    parser.add_argument('--capture', type=Path, help='capture and exit after 0.5 seconds')
    args = parser.parse_args()
    entries = json.loads((HERE / 'manifest.json').read_text())
    if args.map_id is None:
        for entry in entries:
            print(f"{entry['id']}: {entry['test']} (walls={entry['wall_count']}, heights={entry['room_heights']})")
        return 0
    entry = next((item for item in entries if item['id'] == args.map_id), None)
    if entry is None:
        parser.error('unknown map id; run without an id to list fixtures')
    binary = args.binary.resolve() if args.binary else HERE / 'bin/places'
    if args.binary is None and not binary.is_file():
        binary = ROOT / 'target/release/places'
    if not binary.is_file():
        parser.error('build the current game with cargo build --release')
    state = HERE / 'runtime'
    installed = state / 'levels'
    installed.mkdir(parents=True, exist_ok=True)
    settings = state / 'settings.json'
    if not settings.is_file():
        settings.write_text(json.dumps(dict(
            bindings=dict(forward='W', backward='S', strafe_left='A', strafe_right='D',
                          look_up='UP', look_down='DOWN', look_left='LEFT', look_right='RIGHT'),
            walk_speed=3, quality='low', texture_filtering='low', lightmaps='off',
            reflections='off', bloom=False, vsync=False, window_mode='windowed',
            window_width=640, window_height=360)))
    shutil.copy2(HERE / entry['package'], installed / (entry['id'] + '.placesmap'))
    yaw = entry['yaw_degrees'] if args.yaw is None else args.yaw
    asset_root = HERE / 'asset-root' if (HERE / 'asset-root/assets/catalog.json').is_file() else ROOT
    env = dict(os.environ, PLACES_ASSET_ROOT=str(asset_root), PLACES_STATE_ROOT=str(state),
               PLACES_LEVEL=entry['id'], PLACES_BENCH='1', PLACES_QUALITY='low',
               PLACES_SPAWN=','.join(map(str, entry['eye_spawn'] + [yaw])))
    for key in ['PLACES_MOVE_SCRIPT', 'PLACES_CAPTURE', 'PLACES_CAPTURE_TIME', 'PLACES_SCREEN', 'PLACES_INTERACT']:
        env.pop(key, None)
    if args.script:
        env['PLACES_MOVE_SCRIPT'] = args.script
    if args.capture:
        image = args.capture.resolve()
        image.parent.mkdir(parents=True, exist_ok=True)
        env.update(PLACES_CAPTURE=str(image), PLACES_CAPTURE_TIME=str(args.seconds),
                   PLACES_STATE_LOG=str(image.with_suffix('.csv')))
    return subprocess.run([str(binary)], cwd=ROOT, env=env, check=False).returncode

if __name__ == '__main__':
    raise SystemExit(main())
