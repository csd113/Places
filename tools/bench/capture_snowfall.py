#!/usr/bin/env python3
"""Native snowfall capture and same-binary completed-frame A/B campaign.

Run after compiling Winter. Writes only the supplied evidence directory.
"""
from __future__ import annotations
import argparse
import csv
import hashlib
import json
import math
import os
from pathlib import Path
import statistics
import struct
import subprocess
import zipfile

ROOT = Path(__file__).resolve().parents[2]
VIEWS = {
    'lights-near': ([-3.5, 5, 0], [0, 15], '', 2),
    'lodge-near': ([-11.5, -5, 0], [0, 0], '', 2),
    'lodge-silhouette': ([-11.5, -3, 0], [0, 0], '', 2),
    'pond': ([11, -2, 0], [0, -25], '', 2),
    'forest-close': ([1.5, -28.5, 32], [32, 5], '', 2),
    'square': ([1, 10, -32], [-32, -3], '', 2),
    'pale-ground': ([0, 10, 0], [0, -35], '', 2),
    'dark-sky': ([0, 10, 0], [0, 25], '', 2),
    'interior': ([-11.5, -11.5, 180], [180, -5], '', 2),
    'door-in': ([-11.5, -3.5, 0], [0, -5], 'forward@0-3.1,backward@3.5-6.6', 3.1),
    'door-out': ([-11.5, -3.5, 0], [0, -5], 'forward@0-3.1,backward@3.5-6.6', 7),
    'forest-turn': ([0, 12, 0], None, 'forward@0-2,look_right@2-3,look_up@3-3.25', 4),
    'fast-strafe': ([0, 10, 0], None, 'strafe_right@0-2,strafe_left@2-4,look_right@1-3', 4.2),
}


def quantiles(rows, key):
    values = sorted(float(row[key]) for row in rows)
    return {'mean': statistics.mean(values), 'median': statistics.median(values),
            'p95': values[round((len(values)-1)*.95)], 'max': max(values)} if values else None


def expected_budgets(config, qualities):
    """Mirror SnowfallDef defaults and SnowSystem's f32 prefix calculation."""
    def f32(value):
        return struct.unpack('f', struct.pack('f', value))[0]
    intensity = f32(config.get('intensity', 1.0))
    count = config.get('count', 1400)
    return {math.floor(f32((count * {'high':4, 'medium':3, 'low':2}[q] // 4)
                          * intensity)) for q in qualities}


def validate_run(events, frames, snow, budgets, off=False, performance=False, cycle=False):
    """Reject incomplete native evidence before publishing a passing summary."""
    ready = sum(e['event'] == 'present' and e['detail'] == 'ready' for e in events)
    missed = sum(e['event'] == 'surface_not_presented' and e['detail'] == 'ready' for e in events)
    if not ready or missed:
        raise RuntimeError(f'Incomplete native presentation: {ready} ready, {missed} missed')
    if not frames or (performance and (len(frames) != 600 or ready != 720)):
        raise RuntimeError('Missing measured frames or warmup presentations')
    for row in frames:
        if any(int(row[key]) <= 0 for key in ('total_vertices', 'visible_vertices', 'draw_calls')):
            raise RuntimeError('Native frame did not draw the installed world')
        if any(not math.isfinite(float(row[key])) or float(row[key]) < 0
               for key in ('frame_ms', 'render_ms')):
            raise RuntimeError('Invalid native frame timing')
    if not performance and not off:
        if not snow:
            raise RuntimeError('Missing weather diagnostics')
        previous = -1.0
        seen = set()
        for row in snow:
            seconds = float(row['seconds'])
            counts = [int(row[k]) for k in ('evaluated', 'submitted', 'sheltered', 'culled')]
            evaluated, submitted, sheltered, culled = counts
            if (not math.isfinite(seconds) or seconds < previous
                    or any(n < 0 for n in counts) or evaluated not in budgets
                    or evaluated != submitted + sheltered + culled):
                raise RuntimeError('Invalid weather clock, budget or rejection accounting')
            if int(row['capacity_growth']) != 0:
                raise RuntimeError('Weather particle buffers grew during native play')
            if any(not math.isfinite(float(row[k])) or float(row[k]) < 0
                   for k in ('quad_screen_coverage', 'sync_us')):
                raise RuntimeError('Invalid weather coverage or CPU timing')
            previous = seconds
            seen.add(evaluated)
        if cycle and seen != set(budgets):
            raise RuntimeError('Live quality cycle did not exercise every weather budget')
    return {'ready_presentations': ready, 'ready_not_presented': missed}


def run(out, quality, name, off=False, performance=False, cycle=False, level='winter'):
    spawn, camera, movement, seconds = VIEWS[name]
    label = quality + '-' + name + ('-off' if off else '-on') + ('-perf' if performance else '') + ('-cycle' if cycle else '')
    state = out / ('state-' + label)
    state.mkdir(exist_ok=True)
    settings = {'bindings': {'forward':'W','backward':'S','strafe_left':'A','strafe_right':'D',
                'look_up':'UP','look_down':'DOWN','look_left':'LEFT','look_right':'RIGHT',
                'jump':'SPACE','crouch':'C','interact':'E'}, 'quality': quality, 'texture_filtering': quality,
                'lightmaps': {'low':'off','medium':'medium','high':'full'}[quality],
                'reflections': {'low':'off','medium':'medium','high':'full'}[quality],
                'vsync': not performance, 'window_width': 960, 'window_height': 540,
                'window_mode': 'windowed', 'fov_degrees': 60}
    (state / 'settings.json').write_text(json.dumps(settings))
    package_root = ROOT
    if level != 'winter':
        # Runtime discovery reads assets/levels. Mount this debug package in an
        # isolated payload rather than adding it to the shipped map directory.
        package_root = state / 'payload'
        assets = package_root / 'assets'
        levels = assets / 'levels'
        levels.mkdir(parents=True, exist_ok=True)
        for item in (ROOT / 'assets').iterdir():
            if item.name != 'levels' and not (assets / item.name).exists():
                (assets / item.name).symlink_to(item, target_is_directory=item.is_dir())
        package = ROOT / 'levels' / (level + '.placesmap')
        if not (levels / package.name).exists():
            (levels / package.name).symlink_to(package)
    env = {k:v for k,v in os.environ.items() if not k.startswith('PLACES_')}
    env.update(PLACES_LEVEL=level, PLACES_ASSET_ROOT=str(package_root), PLACES_STATE_ROOT=str(state),
               PLACES_QUALITY=quality, PLACES_BENCH='1', PLACES_BENCH_FRAMES='600' if performance else '2400',
               PLACES_BENCH_WARMUP='120' if performance else '0', PLACES_VERBOSE='1',
               PLACES_SPAWN=','.join(map(str,spawn)), PLACES_STATE_LOG=str(out / (label+'-player.csv')),
               PLACES_BENCH_OUT=str(out / (label+'-frames.csv')),
               PLACES_WEATHER_TRACE=str(out / (label+'-snow.csv')),
               PLACES_LOAD_TRACE=str(out / (label+'-load.jsonl')),
               PLACES_BENCH_WEATHER_OFF='1' if off else '0')
    if camera is not None:
        env['PLACES_CAMERA'] = ','.join(map(str,camera))
    if movement:
        env['PLACES_MOVE_SCRIPT'] = movement
    if cycle:
        env['PLACES_BENCH_QUALITY_CYCLE'] = '30:low,80:medium,130:high'
    (out / (label+'-snow.csv')).unlink(missing_ok=True)
    capture = out / (label+'.png')
    capture.unlink(missing_ok=True)
    if performance:
        env.pop('PLACES_WEATHER_TRACE', None)
        env.update(PLACES_BENCH_FINISH='1',PLACES_VSYNC='off')
    else:
        env.update(PLACES_CAPTURE=str(capture), PLACES_CAPTURE_TIME=str(seconds))
    result = subprocess.run([str(ROOT / 'target/release/places')], cwd=ROOT, env=env,
                            text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=180, check=False)
    (out / (label+'.log')).write_text(result.stdout)
    if result.returncode or (not performance and not capture.is_file()):
        raise RuntimeError(f'{label} failed: {result.stdout[-4000:]}')
    if 'is not a valid settings file' in result.stdout:
        raise RuntimeError('The pinned native settings were rejected')
    for failure in ('panicked', '[wgpu] fatal device error', 'Validation Error', 'invalid surface'):
        if failure in result.stdout:
            raise RuntimeError(f'Native renderer failure: {failure}')
    if 'backend: Metal' not in result.stdout:
        raise RuntimeError('Campaign requires actual native Metal renderer on this host')
    events = [json.loads(line) for line in (out / (label+'-load.jsonl')).read_text().splitlines()]
    worlds = [json.loads(event['detail']) for event in events if event['event'] == 'world_committed']
    if not worlds or worlds[-1]['renderer_quality'] != quality or worlds[-1]['current_level_id'] != level:
        raise RuntimeError('Native campaign did not install the requested level and quality')
    frames = list(csv.DictReader((out / (label+'-frames.csv')).open()))
    snow_path = out / (label+'-snow.csv')
    snow = list(csv.DictReader(snow_path.open())) if snow_path.is_file() else []
    with zipfile.ZipFile(package_root / 'assets/levels' / (level + '.placesmap')) as archive:
        config = json.loads(archive.read('semantics.json'))['weather']
    qualities = ('high', 'medium', 'low') if cycle else (quality,)
    budgets = expected_budgets(config, qualities)
    presentation = validate_run(events, frames, snow, budgets, off, performance, cycle)
    summary = {'label':label,'exit':result.returncode,'capture':str(capture) if capture.is_file() else None,
               'frame_rows':len(frames),'frame_ms':quantiles(frames,'frame_ms'),'render_ms':quantiles(frames,'render_ms'),
               'snow_rows':len(snow),'snow_sync_us':quantiles(snow,'sync_us'),
               'snow_submitted':quantiles(snow,'submitted'),'snow_sheltered':quantiles(snow,'sheltered'),
               'coverage':quantiles(snow,'quad_screen_coverage'),
               'capacity_growth':sum(int(row['capacity_growth']) for row in snow),
               **presentation}
    (out / (label+'-summary.json')).write_text(json.dumps(summary,indent=2)+'\n')
    print(json.dumps(summary),flush=True)
    return summary


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--mode', choices=('capture','perf','cycle'), default='capture')
    parser.add_argument('--qualities', default='high,medium,low')
    parser.add_argument('--views', default='square,pale-ground,dark-sky,interior,door-in,door-out,forest-turn,fast-strafe')
    parser.add_argument('--level', default='winter', choices=('winter','snowfall_contrast','blizzard_review'))
    args = parser.parse_args()
    out = args.out.resolve(); out.mkdir(parents=True,exist_ok=True)
    summaries = []
    for quality in args.qualities.split(','):
        for name in args.views.split(','):
            for off in ((False,True,True,False) if args.mode == 'perf' else (False,)):
                if args.mode == 'perf':
                    # Retain repetitions separately instead of overwriting evidence.
                    repeat = out / ('repeat-'+str(len(summaries)))
                    repeat.mkdir(exist_ok=True)
                else:
                    repeat = out
                summaries.append(run(repeat,quality,name,off,args.mode == 'perf',args.mode == 'cycle',args.level))
    package = ROOT/'assets/levels/winter.placesmap' if args.level == 'winter' else ROOT/'levels'/(args.level+'.placesmap')
    manifest = {'binary_sha256':hashlib.sha256((ROOT/'target/release/places').read_bytes()).hexdigest(),
                'level':args.level, 'package_sha256':hashlib.sha256(package.read_bytes()).hexdigest(),
                'runs':summaries}
    (out/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')

if __name__ == '__main__':
    main()
