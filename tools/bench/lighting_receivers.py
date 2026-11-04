#!/usr/bin/env python3
"""Opt-in native receiver regressions. Requires the shared lighting validation lock.

Small dedicated fixtures only; ordinary tests/CI never bake or start a window.
Each invocation requires a fresh output directory and records exact identities.
"""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[2]
CONTACT_CASES = ('contact', 'internal', 'low-wall', 'doorway', 'slope', 'stacked')
SCREEN_CASES = ('screen', 'clear', 'screen-rotated', 'clear-rotated')


def write(path, value):
    path.write_text(json.dumps(value, indent=2) + '\n')


def fixture(case):
    contact = case in CONTACT_CASES
    source = ROOT / 'tests/fixtures/levels' / (
        'lighting_contact_walls.json' if contact else 'elevator_light_screen.json')
    value = json.loads(source.read_text())
    if contact and case != 'contact':
        value['rooms'] = [dict(id='same_room', x=-3, z=-3, width=6, depth=6, height=3)]
        if case == 'low-wall':
            value['walls'][0]['height'] = .4
        elif case == 'doorway':
            value['walls'][0]['openings'] = [dict(kind='passage', offset=2.4, width=1.2, height=2.2, sill=0)]
        elif case == 'slope':
            value['walls'].pop(0)
            value['ramps'] = [dict(x=-2, z=-3, width=4, depth=6, rise=.6)]
        elif case == 'stacked':
            value['rooms'].append(dict(id='above', x=-3, z=-3, width=6, depth=6, height=3, floor_y=3.2))
            value['ceiling_lights'] += [dict(light, y=6.2) for light in value['ceiling_lights']]
    if not contact and case.startswith('clear'):
        value['walls'].pop()
    if case.endswith('-rotated'):
        for item in value['rooms'] + value['walls']:
            x, z, width, depth = (item[k] for k in ('x', 'z', 'width', 'depth'))
            item.update(x=z, z=-x-width, width=depth, depth=width)
        value['elevators'][0]['rotation_degrees'] = 90
        value['spawn'].update(x=-2.5, z=0, yaw_degrees=270)
    return value


def contacts(out, upper=False):
    eye_y, local_y = (5.1, 3.2) if upper else (1.9, 0)
    def spawn(key, position, scale=1, character=False):
        return dict(action='spawn', key=key, model='mannequin' if character else 'core:crate',
                    character=character, position=position, yaw=0, scale=scale)
    return [
        dict(frame=2, actions=[dict(action='camera', position=[2.65, eye_y, 2.1], yaw=-48, pitch=-32)]),
        dict(frame=15, capture=str(out / 'baseline.png')),
        dict(frame=25, actions=[spawn(1, [-.65, 0, 0], 2)]),
        dict(frame=40, capture=str(out / 'behind-wall.png')),
        dict(frame=50, actions=[dict(action='despawn', key=1)]),
        dict(frame=65, capture=str(out / 'restored.png')),
        dict(frame=75, actions=[spawn(2, [1, local_y, 0])]),
        dict(frame=90, capture=str(out / 'local-shadow.png')),
        dict(frame=100, actions=[dict(action='despawn', key=2)] +
             [spawn(10+i, [-2, 0, -2]) for i in range(8)] + [spawn(2, [1, local_y, 0])]),
        dict(frame=120, capture=str(out / 'ninth-local.png')),
        dict(frame=130, actions=[dict(action='despawn', key=10)]),
        dict(frame=150, capture=str(out / 'slot-restored.png')),
        dict(frame=160, actions=[dict(action='despawn', key=2), spawn(30, [1, local_y, 0], character=True)]),
        dict(frame=180, capture=str(out / 'nearby-character.png')),
        dict(frame=190, actions=[dict(action='camera', position=[2.6, eye_y, 2.15], yaw=-47, pitch=-32)]),
        dict(frame=210, capture=str(out / 'camera-moved.png')),
    ]


def screen(case):
    actions = []
    anchor = 'ready:screen_audit'
    angle = math.pi / 2 if case.endswith('-rotated') else 0

    def add(ms, kind, **fields):
        actions.append(dict(after=anchor, delay_ms=ms, action=dict(kind=kind, **fields)))

    def rotate(point):
        x, y, z = point
        return [math.cos(angle)*x+math.sin(angle)*z, y, -math.sin(angle)*x+math.cos(angle)*z]

    def pose(ms, position, target):
        add(ms, 'player', position=rotate(position), yaw=180+math.degrees(angle), pitch=0)
        add(ms+20, 'view_at', position=rotate(target))

    def capture(ms, name):
        nonlocal anchor
        add(ms, 'checkpoint', name=name)
        add(ms+20, 'capture', name=name)
        add(ms+450, 'checkpoint', name=name+'_done')
        anchor = 'checkpoint:'+name+'_done'

    pose(100, [0, 1.6, -1.5], [0, 0, -.5])
    capture(300, 'closed')
    pose(100, [-.96, 1.6, -.9], [-.96, 1.08, -.12])
    add(300, 'checkpoint', name='aimed')
    anchor = 'checkpoint:aimed'
    add(150, 'mouse', button='left', pressed=True)
    add(160, 'checkpoint', name='pressed')
    anchor = 'checkpoint:pressed'
    add(200, 'mouse', button='left', pressed=False)
    add(220, 'checkpoint', name='released')
    anchor = 'checkpoint:released'
    pose(100, [0, 1.6, -1.5], [0, 0, -.5])
    capture(120, 'partial')
    capture(1600, 'open')
    add(100, 'quit')
    return actions


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--compiler', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--case', choices=CONTACT_CASES+SCREEN_CASES, required=True)
    parser.add_argument('--quality', choices=('off', 'full'), default='full')
    parser.add_argument('--upper', action='store_true', help='Inspect the upper storey of the stacked contact fixture')
    args = parser.parse_args()
    if args.upper and args.case != 'stacked':
        parser.error('--upper requires --case stacked')
    owner = json.loads(Path('/tmp/places-audit-validation.lock/owner.json').read_text())
    if owner.get('audit') != 'lighting':
        raise SystemExit('Lighting must own the shared validation lock')
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    binary, compiler = args.binary.resolve(strict=True), args.compiler.resolve(strict=True)
    value = fixture(args.case)
    source, package = out/'source.json', out/'fixture.placesmap'
    write(source, value)
    env = {k: v for k, v in os.environ.items() if not k.startswith('PLACES_')}
    with (out/'build.log').open('w') as log:
        subprocess.run([str(compiler), 'build', str(source), '--out', str(package),
                        '--asset-root', str(ROOT/'assets'), '--workers', '1',
                        '--variants', 'static,full', '--json'], cwd=ROOT, env=env,
                       stdout=log, stderr=subprocess.STDOUT, check=True, timeout=240)
    state = out/'state'
    (state/'levels').mkdir(parents=True)
    shutil.copy2(package, state/'levels/fixture.placesmap')
    settings = json.loads((ROOT/'tests/fixtures/native/hero-manifest.json').read_text())['settings']
    settings.update(window_width=640, window_height=360, quality='high', lightmaps=args.quality,
                    reflections='off', bloom=False, visual_style='standard', vsync=True,
                    ambient_volume=0, master_volume=0, fov_degrees=60)
    write(state/'settings.json', settings)
    contact = args.case in CONTACT_CASES
    script = out/'actions.json'
    write(script, contacts(out, args.upper) if contact else screen(args.case))
    env.update(PLACES_ASSET_ROOT=str(ROOT/'assets'), PLACES_STATE_ROOT=str(state), PLACES_LEVEL=value['id'],
               PLACES_BENCH='1', PLACES_BENCH_FRAMES='225' if contact else '3000',
               PLACES_BENCH_WARMUP='0', PLACES_VERBOSE='1', PLACES_BENCH_OUT=str(out/'frames.csv'))
    env['PLACES_BENCH_LIGHTING_SEQUENCE' if contact else 'PLACES_BENCH_ACTIONS'] = str(script)
    env['PLACES_LOAD_TRACE'] = str(out/'trace.jsonl')
    with (out/'game.log').open('w') as log:
        result = subprocess.run([str(binary)], cwd=ROOT, env=env, stdout=log,
                                stderr=subprocess.STDOUT, timeout=120, check=True)
    digest = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
    receipt = dict(case=args.case, quality=args.quality, exit_code=result.returncode,
                   binary_sha256=digest(binary), compiler_sha256=digest(compiler),
                   source_sha256=digest(source), package_sha256=digest(package),
                   captures=[str(p.relative_to(out)) for p in out.rglob('*.png')])
    write(out/'receipt.json', receipt)
    print(json.dumps(receipt))


if __name__ == '__main__':
    main()
