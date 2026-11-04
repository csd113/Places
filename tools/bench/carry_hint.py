#!/usr/bin/env python3
"""Opt-in native carry-advisory check; never builds Rust or prepares maps.

Supply the straight fixture exported from elevator_cargo_fixture.py. The
prescribed starting pose is logged; pickup, turning, walking, settings changes
and release use the production paths. Run native validators sequentially.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[2]


def actions():
    steps = []

    def add(ms, kind, **fields):
        steps.append(dict(after='ready:elevator_lobby', delay_ms=ms,
                          action=dict(kind=kind, **fields)))

    add(50, 'player', position=[-.45, 1.6, 1.1], yaw=180, pitch=0)
    add(120, 'aim_item', id='travel_crate')
    add(250, 'key', key='E', pressed=True)
    add(320, 'key', key='E', pressed=False)
    add(1600, 'checkpoint', name='held')
    add(1800, 'view', yaw=0, pitch=0)
    add(2700, 'key', key='W', pressed=True)
    add(2950, 'key', key='W', pressed=False)
    add(4500, 'checkpoint', name='blocked')
    add(4600, 'capture', name='blocked-normal')
    add(5400, 'console_enabled', enabled=True)
    add(6000, 'capture', name='blocked-developer')
    add(6100, 'checkpoint', name='blocked-developer')
    add(6500, 'console_enabled', enabled=False)
    add(7100, 'capture', name='blocked-disabled')
    add(7200, 'checkpoint', name='blocked-disabled')
    add(7600, 'key', key='E', pressed=True)
    add(7670, 'key', key='E', pressed=False)
    add(8100, 'checkpoint', name='released')
    add(8200, 'console_enabled', enabled=True)
    add(8700, 'checkpoint', name='empty-developer')
    add(8800, 'capture', name='empty-developer')
    add(9900, 'quit')
    return steps


def verify(checkpoints):
    held = checkpoints['held']['physics']['held']
    assert held is not None, 'Ordinary pickup must succeed'
    for name, enabled in [('blocked', False), ('blocked-developer', True),
                          ('blocked-disabled', False)]:
        state = checkpoints[name]
        assert state['current_level_id'] == 'elevator_lobby'
        assert state['state'] == 'Playing'
        assert state['console_enabled'] == enabled
        assert not state['console_open'], 'Enabling diagnostics must not pause play'
        assert state['physics']['held'] == held, 'Visibility cannot change custody'
        assert state['item_hint']['held_movement_blocked'], 'Real collision is required'
        expected = 'The item cannot follow here.' if enabled else None
        assert state['item_hint']['text'] == expected, (name, state['item_hint'])
        assert state['elevators'][0]['phase'] == 'closed'
        assert state['elevators'][0]['stop'] == 0
    first = next(body for body in checkpoints['blocked']['physics']['bodies']
                 if body['held'])
    last = next(body for body in checkpoints['blocked-disabled']['physics']['bodies']
                if body['held'])
    assert first['pose'] == last['pose'], 'Mode changes cannot bypass the obstruction'
    for name in ('released', 'empty-developer'):
        assert checkpoints[name]['current_level_id'] == 'elevator_lobby'
        assert checkpoints[name]['physics']['held'] is None, 'Ordinary release must succeed'
        assert not checkpoints[name]['item_hint']['held_movement_blocked']
        assert checkpoints[name]['item_hint']['text'] is None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--package', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    state = out / 'state'
    (state / 'levels').mkdir(parents=True)
    shutil.copy2(args.package, state / 'levels/cargo.placesmap')
    settings = json.loads((ROOT / 'tests/fixtures/native/hero-manifest.json').read_text())['settings']
    settings.update(window_width=640, window_height=360, quality='high',
                    texture_filtering='high', lightmaps='full', reflections='off',
                    bloom=False, vsync=True, developer_console=False, ambient_volume=0)
    (state / 'settings.json').write_text(json.dumps(settings))
    plan = out / 'actions.json'
    plan.write_text(json.dumps(actions(), indent=2) + '\n')
    trace = out / 'trace.jsonl'
    env = {key: value for key, value in os.environ.items() if not key.startswith('PLACES_')}
    env.update(PLACES_ASSET_ROOT=str(ROOT / 'assets'), PLACES_STATE_ROOT=str(state),
               PLACES_LEVEL='elevator_lobby', PLACES_BENCH='1',
               PLACES_BENCH_FRAMES='6000', PLACES_BENCH_WARMUP='3',
               PLACES_LOAD_TRACE=str(trace), PLACES_BENCH_ACTIONS=str(plan),
               PLACES_PHYSICS_TRACE='1')
    with (out / 'game.log').open('w') as log:
        subprocess.run([str(args.binary.resolve(strict=True))], cwd=ROOT, env=env,
                       stdout=log, stderr=subprocess.STDOUT, check=True, timeout=60)
    rows = [json.loads(line) for line in trace.read_text().splitlines()]
    checkpoints = {row['event'][11:]: json.loads(row['detail']) for row in rows
                   if row['event'].startswith('checkpoint:')}
    (out / 'checkpoints.json').write_text(json.dumps(checkpoints, indent=2) + '\n')
    verify(checkpoints)
    receipt = dict(passed=True,
                   binary_sha256=hashlib.sha256(args.binary.read_bytes()).hexdigest(),
                   package_sha256=hashlib.sha256(args.package.read_bytes()).hexdigest(),
                   checks=['normal mode hides advisory', 'developer mode shows advisory',
                           'disabling hides it immediately', 'blocked pose/custody unchanged',
                           'ordinary drop succeeds', 'empty hand hides advisory'],
                   scope='Actual direct binary, macOS Metal, correctness capture, paced gameplay')
    (out / 'passed.json').write_text(json.dumps(receipt, indent=2) + '\n')
    print('Native carry advisory checks passed.')


if __name__ == '__main__':
    main()
