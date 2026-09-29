#!/usr/bin/env python3
"""Inventory maintained sources and capture matched room/region runtime views.

Extends capture_views/visual_check to every maintained source. Negative geometry
fixtures are inventoried but excluded from playable acceptance. Each capture
pins all quality settings and retains logs, load traces and package hashes.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import shutil
import subprocess
import zipfile

REPO = Path(__file__).resolve().parents[2]

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def require_matching_package(staged, requested):
    """Never silently reuse an old staged package after a rebake."""
    if staged.exists() and digest(staged) != digest(requested):
        raise RuntimeError(
            f'Staged package differs from requested rebuild: {staged}; '
            'use a fresh --out directory to retain matched capture evidence')

def inventory():
    entries = {}
    for directory in ('assets/levels', 'tests/fixtures/levels', 'levels'):
        for source in sorted((REPO / directory).glob('*.json')):
            data = json.loads(source.read_text())
            key = data['id']
            entry = entries.setdefault(key, dict(id=key, source=str(source), copies=[], packages=[],
                playable=key != 'geometry_broken'))
            entry['copies'].append(dict(path=str(source), sha256=digest(source)))
            package = source.with_suffix('.placesmap')
            if package.exists():
                entry['packages'].append(dict(path=str(package), sha256=digest(package)))
    return sorted(entries.values(), key=lambda e: e['id'])

def views(data):
    result = []
    # At most 12 m between inspection sites in large halls and fixture clusters.
    for i, room in enumerate(data['rooms']):
        nx, nz = (max(1, math.ceil(room[k] / 12)) for k in ('width', 'depth'))
        for iz in range(nz):
            for ix in range(nx):
                x = room['x'] + room['width'] * (ix + .5) / nx
                z = room['z'] + room['depth'] * (iz + .5) / nz
                # Explicit eye height preserves storeys with overlapping XZ.
                y = room.get('floor_y', data.get('floor_y', 0)) + 1.6
                for yaw in (0, 90, 180, 270):
                    result.append((f'room_{i}_{ix}_{iz}_{yaw}', f'{x},{y},{z},{yaw}', f'{yaw},-8'))
                if room.get('height', data.get('wall_height', 3)) > 4:
                    result.append((f'room_{i}_{ix}_{iz}_ceiling', f'{x},{y},{z},0', '0,55'))
        # Small-room cardinal views can contain only walls at the pinned FOV.
        # Inspect the floor and ordinary-height ceiling explicitly as well.
        x = room['x'] + room['width'] * .5
        z = room['z'] + room['depth'] * .5
        y = room.get('floor_y', data.get('floor_y', 0)) + 1.6
        result.append((f'surface_room_{i}_floor', f'{x},{y},{z},0', '0,-70'))
        if room.get('height', data.get('wall_height', 3)) <= 4:
            result.append((f'surface_room_{i}_ceiling', f'{x},{y},{z},0', '0,70'))
    for kind in ('floor_regions', 'water'):
        for i, region in enumerate(data.get(kind, [])):
            if not all(k in region for k in ('x','z','width','depth')):
                continue
            x,z = region['x'] + region['width']*.5, region['z'] + region['depth']*.5
            room = next((r for r in data['rooms'] if r['x'] <= x <= r['x']+r['width'] and r['z'] <= z <= r['z']+r['depth']), {})
            y = room.get('floor_y', 0) + region.get('offset_y', 0) + 1.6
            result.append((f'{kind}_{i}',f'{x},{y},{z},45','45,-35'))
    # A room-centre camera can land inside a curved partition (the demo shower
    # screen is one example). Inspect both faces independently of that grid.
    for i, arc in enumerate(data.get('arc_walls', [])):
        yaw = arc.get('start_degrees', 0) + arc.get('sweep_degrees', 90) * .5
        angle = math.radians(yaw)
        clearance = arc.get('thickness', .3) * .5 + .6
        for side, radius, facing in (
            ('inner', arc['radius'] - clearance, yaw),
            ('outer', arc['radius'] + clearance, yaw + 180),
        ):
            if radius <= 0:
                continue
            x = arc['x'] + math.sin(angle) * radius
            z = arc['z'] - math.cos(angle) * radius
            room = next((r for r in data['rooms'] if r['x'] <= x <= r['x']+r['width']
                         and r['z'] <= z <= r['z']+r['depth']), None)
            if room is None:
                continue
            y = arc.get('y', room.get('floor_y', 0)) + 1.6
            result.append((f'arc_walls_{i}_{side}',f'{x},{y},{z},{facing}',f'{facing},-8'))
            result.append((f'arc_walls_{i}_{side}_away',f'{x},{y},{z},{facing+180}',f'{facing+180},-8'))
    return result

def runtime_state(trace):
    """Retain load evidence without hundreds of thousands of idle pump rows."""
    kept=[]
    counts={}
    committed=None
    for line in trace.read_text().splitlines():
        row=json.loads(line)
        event=row['event']
        counts[event]=counts.get(event,0)+1
        if event=='world_committed':
            committed=json.loads(row['detail'])
        if event not in {'event_pump','surface_not_presented','upload_step_begin','upload_step_end','trace_counts'}:
            kept.append(row)
    kept.append(dict(event='trace_counts',counts=counts))
    trace.write_text(''.join(json.dumps(row)+'\n' for row in kept))
    if committed is None:
        raise RuntimeError(f'No committed runtime world in {trace}')
    return committed

def require_prepared_load(trace):
    """Positive runtime proof: package loading did zero lighting/atlas work."""
    preparations = [json.loads(row['detail'])
                    for row in map(json.loads, trace.read_text().splitlines())
                    if row['event'] == 'preparation_result']
    if not preparations or any(row.get('lighting_ms') != 0 or row.get('atlas_ms') != 0
                               for row in preparations):
        raise RuntimeError(f'Runtime did not confirm a bake-free prepared load: {trace}')


def main():
    ap=argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--out',type=Path,required=True)
    ap.add_argument('--binary',type=Path,default=REPO/'target/debug/places')
    ap.add_argument('--compiler',type=Path,default=REPO/'target/debug/places-compile')
    ap.add_argument('--profiles',default='high,medium,low')
    ap.add_argument('--workers',type=int,default=12)
    ap.add_argument('--levels',default='')
    ap.add_argument('--view-prefix',default='',help='Capture only matching named regions')
    ap.add_argument('--inventory-only',action='store_true')
    ap.add_argument('--build-missing',action='store_true')
    ap.add_argument('--package-dir',type=Path)
    ap.add_argument('--quality-sample',action='store_true',help='Medium/Low use one cardinal view per site, plus ceilings and regions')
    ap.add_argument('--require-identity',action='store_true')
    args=ap.parse_args(); out=args.out.resolve();out.mkdir(parents=True,exist_ok=True)
    if not 1 <= args.workers <= 12:
        ap.error('--workers must be between 1 and 12')
    entries=inventory(); (out/'inventory.json').write_text(json.dumps(entries,indent=2)+'\n')
    excluded=[dict(path=str(p),sha256=digest(p),reason='negative geometry/repair input, not a maintained playable level') for p in sorted((REPO/'tests/fixtures/levels').glob('*/*.json'))]
    (out/'negative-inputs.json').write_text(json.dumps(excluded,indent=2)+'\n')
    if args.inventory_only:return
    stage=out/'runtime'; (stage/'assets/levels').mkdir(parents=True,exist_ok=True)
    for item in (REPO/'assets').iterdir():
        link=stage/'assets'/item.name
        if item.name != 'levels' and not link.exists():link.symlink_to(item,target_is_directory=item.is_dir())
    manifest=out/'captures.json'
    records={row['image']:row for row in json.loads(manifest.read_text())} if manifest.exists() else {}
    binary_hash = digest(args.binary.resolve())
    for entry in entries:
        if not entry['playable'] or (args.levels and entry['id'] not in args.levels.split(',')):continue
        source=Path(entry['source']); package=stage/'assets/levels'/f"{entry['id']}.placesmap"
        if args.package_dir:
            require_matching_package(package,args.package_dir/f"{entry['id']}.placesmap")
        if not package.exists():
            if args.package_dir:
                shutil.copy2(args.package_dir/f"{entry['id']}.placesmap",package)
            elif entry['packages']:
                shutil.copy2(entry['packages'][0]['path'],package)
            elif args.build_missing:
                cmd=[str(args.compiler.resolve()),'build',str(source),'--out',str(package),'--workers',str(args.workers),'--force','--json']
                with (out/f"{entry['id']}-build.log").open('w') as log:
                    subprocess.run(cmd,cwd=REPO,stdout=log,stderr=subprocess.STDOUT,check=True)
            else:
                raise RuntimeError(f'Missing package for {source}; use --build-missing')
        data=json.loads(source.read_text()); shots=[shot for shot in views(data) if shot[0].startswith(args.view_prefix)]
        package_hash=digest(package)
        with zipfile.ZipFile(package) as archive:
            manifest_hash=hashlib.sha256(archive.read("manifest.json")).hexdigest()
        for profile in args.profiles.split(','):
            dest=out/entry['id']/profile; dest.mkdir(parents=True,exist_ok=True)
            state=dest/'state';state.mkdir(exist_ok=True)
            quality={'high':'full','medium':'medium','low':'off'}[profile]
            settings=dict(bindings=dict(forward='W',backward='S',strafe_left='A',strafe_right='D',look_up='UP',look_down='DOWN',look_left='LEFT',look_right='RIGHT'),look_speed_h=90.,look_speed_v=60.,walk_speed=3.,fov_degrees=60.,invert_look=False,bloom=True,window_mode='windowed')
            settings.update(quality=profile,texture_filtering=profile,lightmaps=quality,reflections=quality,window_width=640,window_height=360,vsync=False)
            (state/'settings.json').write_text(json.dumps(settings))
            profile_shots = shots
            if args.quality_sample and profile != 'high':
                profile_shots = [shot for shot in shots if not shot[0].startswith('room_') or shot[0].endswith(('_0','_ceiling'))]
            for name,spawn,camera in profile_shots:
                png=dest/f'{name}.png';log=dest/f'{name}.log'
                record=dict(level=entry['id'],profile=profile,view=name,spawn=spawn,camera=camera,package_sha256=package_hash,manifest_sha256=manifest_hash,image=str(png))
                previous_binary = records.get(str(png), {}).get('binary_sha256')
                if png.exists() and args.require_identity and previous_binary != binary_hash:
                    raise RuntimeError(f'Capture binary identity is missing or changed: {png}; use a fresh --out')
                record['binary_sha256'] = previous_binary if png.exists() else binary_hash
                if not png.exists():
                    env=os.environ.copy();env.update(PLACES_ASSET_ROOT=str(stage),PLACES_STATE_ROOT=str(state),PLACES_LEVEL=entry['id'],PLACES_QUALITY=profile,PLACES_SPAWN=spawn,PLACES_CAMERA=camera,PLACES_CAPTURE=str(png),PLACES_TOOL_WORKERS='1',PLACES_BENCH='1',PLACES_BENCH_NOSWAP='1',PLACES_VERBOSE='1',PLACES_LOAD_TRACE=str(dest/f'{name}.jsonl'))
                    with log.open('w') as output:
                        subprocess.run([str(args.binary.resolve())],cwd=stage,env=env,stdout=output,stderr=subprocess.STDOUT,check=True,timeout=180)
                if not png.exists():raise RuntimeError(f'No capture: {png}')
                log_text=log.read_text()
                if args.require_identity and f"sha256={record['manifest_sha256']}" not in log_text:
                    raise RuntimeError(f'Runtime package identity missing or wrong: {log}')
                if '[lightmaps] transport workers=' in log_text:
                    raise RuntimeError(f'Player attempted a bake: {log}')
                state_loaded=runtime_state(dest/f'{name}.jsonl')
                if args.require_identity:
                    require_prepared_load(dest/f'{name}.jsonl')
                for key,expected in [('renderer_level_id',entry['id']),('renderer_quality',profile),('renderer_lightmaps',quality)]:
                    if state_loaded[key]!=expected:
                        raise RuntimeError(f'{png}: {key}={state_loaded[key]}, expected {expected}')
                expected_position = list(map(float, spawn.split(',')[:3]))
                if any(abs(actual-expected) > 0.001 for actual,expected in
                       zip(state_loaded['player_position'],expected_position)):
                    raise RuntimeError(f'{png}: runtime moved the explicit inspection camera')
                record['runtime']=state_loaded
                records[str(png)]=record
            (out/'captures.json').write_text(json.dumps(list(records.values()),indent=2)+'\n')
            print(entry['id'],profile,len(profile_shots),'captured',flush=True)
if __name__=='__main__':main()
