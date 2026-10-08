#!/usr/bin/env python3
"""Frozen matched Winter journal cameras, captured by the real native player."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time

VIEWS = {
    'square': ('1,10,-32', '-32,-3'),
    'lodge': ('-5,-3,-42', '-42,0'),
    'entrance': ('-11.5,-6,0', '0,10'),
    'cottage': ('12.5,-20.2,0', '0,12'),
    'tree': ('-2.5,2,-69', '-69,19'),
    'forest': ('0,-22,0', '0,-2'),
    'pond': ('4,-10,90', '90,-14'),
    'ice': ('9,-10,90', '90,-32'),
    'rail': ('14,-13,90', '90,-5'),
    'path': ('0,6,0', '0,-36'),
    'overview': ('0,32,22,0', '0,-47'),
    'interior': ('-11.5,-11.5,-28', '-28,-5'),
}


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, required=True)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--quality', choices=('low', 'medium', 'high'), default='high')
    parser.add_argument('--level', default='winter')
    parser.add_argument('--views', default=','.join(VIEWS))
    args = parser.parse_args()
    root, binary, out = args.root.resolve(), args.binary.resolve(), args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    state = out / 'state'
    state.mkdir(exist_ok=True)
    # Blizzard is a drop-in review package, with the same art and scene.
    if args.level != 'winter':
        import shutil
        (state / 'levels').mkdir(exist_ok=True)
        shutil.copy2(root / 'levels' / (args.level + '.placesmap'), state / 'levels')
    variant = {'low': 'off', 'medium': 'medium', 'high': 'full'}[args.quality]
    settings = dict(bindings={'forward': 'W', 'backward': 'S', 'strafe_left': 'A',
                             'strafe_right': 'D', 'look_up': 'UP', 'look_down': 'DOWN',
                             'look_left': 'LEFT', 'look_right': 'RIGHT', 'jump': 'SPACE',
                             'crouch': 'C', 'interact': 'E'},
                    look_speed_h=90, look_speed_v=60, walk_speed=3, invert_look=False,
                    quality=args.quality, texture_filtering=args.quality, lightmaps=variant,
                    reflections=variant, bloom=True, vsync=False, fov_degrees=60,
                    window_mode='windowed', window_width=640, window_height=360)
    (state / 'settings.json').write_text(json.dumps(settings, indent=2) + '\n')
    package = (root / 'assets/levels/winter.placesmap' if args.level == 'winter'
               else root / 'levels' / (args.level + '.placesmap'))
    manifest_path = out / 'manifest.json'
    manifest = json.loads(manifest_path.read_text()) if manifest_path.exists() else []
    for name in args.views.split(','):
        spawn, camera = VIEWS[name]
        capture = out / (name + '.png')
        if capture.exists():
            raise RuntimeError(f'Preserve existing evidence: {capture}')
        env = {k: v for k, v in os.environ.items() if not k.startswith('PLACES_')}
        env.update(PLACES_ASSET_ROOT=str(root), PLACES_STATE_ROOT=str(state),
                   PLACES_LEVEL=args.level, PLACES_QUALITY=args.quality,
                   PLACES_SPAWN=spawn, PLACES_CAMERA=camera, PLACES_BENCH='1',
                   PLACES_BENCH_FRAMES='2400', PLACES_BENCH_WARMUP='15',
                   PLACES_CAPTURE=str(capture), PLACES_CAPTURE_TIME='0' if name == 'overview' else '1.5',
                   PLACES_VERBOSE='1', PLACES_BENCH_OUT=str(out / (name + '.csv')),
                   PLACES_LOAD_TRACE=str(out / (name + '-load.jsonl')),
                   PLACES_WEATHER_TRACE=str(out / (name + '-weather.csv')),
                   DYLD_LIBRARY_PATH=str(binary.parent))
        start = time.monotonic()
        result = subprocess.run([str(binary)], env=env, text=True, stdout=subprocess.PIPE,
                                stderr=subprocess.STDOUT, timeout=180, check=False)
        (out / (name + '.log')).write_text(result.stdout)
        renderer = next((line for line in result.stdout.splitlines()
                         if line.startswith('[renderer] wgpu | adapter:')), None)
        events = [json.loads(line) for line in (out / (name + '-load.jsonl')).read_text().splitlines()]
        presented = any(e['event'] == 'scene_presented' and e['detail'] == args.level for e in events)
        if (result.returncode or not capture.is_file() or renderer is None or not presented
                or 'not a valid settings file' in result.stdout or '0 failed)' not in result.stdout):
            raise RuntimeError(f'{name}: capture failed; inspect {out / (name + ".log")}')
        manifest.append(dict(name=name, spawn=spawn, camera=camera, quality=args.quality,
                             level=args.level, settings=settings, capture_time=0 if name == 'overview' else 1.5,
                             elapsed_seconds=time.monotonic() - start, renderer=renderer,
                             package_sha256=digest(package), binary_sha256=digest(binary),
                             sha256=digest(capture)))
        manifest_path.write_text(json.dumps(manifest, indent=2) + '\n')
        print(f'OK {args.level}/{args.quality}/{name}', flush=True)
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
