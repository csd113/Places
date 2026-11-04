#!/usr/bin/env python3
"""Opt-in direct-binary automatic-drop acceptance; never builds or bakes.

Use the exported straight elevator_cargo_fixture.py source. Prescribed initial
player/item poses are logged; measured movement/release uses ordinary input.
Serialize native sessions. The closed-cabin case establishes separation with a
diagnostic player pose beyond the door, then walks across the release boundary.
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


def actions(mode):
    steps = []

    def add(ms, kind, **fields):
        steps.append(dict(after='ready:elevator_lobby', delay_ms=ms,
                          action=dict(kind=kind, **fields)))

    if mode == 'normal':
        add(50, 'player', position=[-1.6, 1.6, -4], yaw=180, pitch=0)
        add(80, 'item_pose', id='travel_crate', position=[-1.6, .02, -3], rotation=[0, 0, 0, 1])
        add(150, 'aim_item', id='travel_crate')
        add(250, 'key', key='E', pressed=True)
        add(320, 'key', key='E', pressed=False)
        add(1800, 'view', yaw=180, pitch=0)
        add(3000, 'checkpoint', name='held')
        add(3100, 'key', key='A', pressed=True)
        add(3500, 'checkpoint', name='walking')
        add(3900, 'key', key='A', pressed=False)
        add(4200, 'checkpoint', name='followed')
        add(4300, 'save_checkpoint', name='carry_stable')
        add(4700, 'resume_checkpoint')
        add(7000, 'checkpoint', name='held-resumed')
        add(7200, 'key', key='E', pressed=True)
        add(7270, 'key', key='E', pressed=False)
        add(7800, 'checkpoint', name='manual-released')
        add(8000, 'quit')
    else:
        import carry_hint
        steps.extend(step for step in carry_hint.actions() if step['delay_ms'] <= 4500)
        add(5000, 'player', position=[-.25, 1.6, -.8], yaw=0, pitch=0)
        add(7000, 'checkpoint', name='near')
        add(7200, 'player', position=[-.25, 1.6, -2.25], yaw=0, pitch=0)
        add(7900, 'checkpoint', name='below')
        add(8200, 'key', key='W', pressed=True)
        add(8700, 'key', key='W', pressed=False)
        add(8900, 'checkpoint', name='dropped')
        add(9000, 'save_checkpoint', name='auto_dropped')
        add(9400, 'resume_checkpoint')
        add(12000, 'checkpoint', name='dropped-resumed')
        add(12100, 'capture', name='released-after-walking-away')
        add(12500, 'quit')
    return steps


def held_body(state):
    return next(body for body in state['physics']['bodies'] if body['held'])


def verify(mode, rows, checkpoints):
    drops = [json.loads(row['detail']) for row in rows if row['event'] == 'item_auto_dropped']
    if mode == 'normal':
        assert not drops
        for name in ('held', 'walking', 'followed', 'held-resumed'):
            state = checkpoints[name]
            assert state['physics']['held'] is not None, (name, state['physics'])
            assert math.dist(state['player_position'], held_body(state)['world_center_of_mass']) < 1.5
        assert math.dist(checkpoints['held']['player_position'], checkpoints['followed']['player_position']) > 1
        assert checkpoints['manual-released']['physics']['held'] is None
        return dict(normal_following=True, held_save_resume=True, manual_release=True)
    assert len(drops) == 1, 'One release must clear ownership exactly once'
    before = checkpoints['below']
    key = before['physics']['held']
    assert key is not None
    limit = before['item_hint']['auto_drop_distance']
    near_distance = math.dist(before['player_position'], held_body(before)['world_center_of_mass'])
    assert before['item_hint']['held_movement_blocked']
    assert limit - .5 < near_distance < limit, (near_distance, limit)
    event = drops[0]
    body = next(body for body in event['physics']['bodies'] if body['key'] == key)
    drop_distance = math.dist(event['player_position'], body['world_center_of_mass'])
    assert limit < drop_distance < limit + .3, (drop_distance, limit)
    assert body['linear_velocity'] == body['angular_velocity'] == [0, 0, 0]
    release_index = next(index for index, row in enumerate(rows) if row['event'] == 'item_auto_dropped')
    previous = [json.loads(row['detail']) for row in rows[:release_index]
                if row['event'] == 'physics_frame'][-1]
    assert previous['held'] == key
    retained = next(item for item in previous['bodies'] if item['key'] == key)
    assert math.dist(body['pose']['position'], retained['pose']['position']) < .006
    assert abs(sum(a*b for a, b in zip(body['pose']['rotation'], retained['pose']['rotation']))) > .999
    assert event['physics']['held'] is None
    assert event['interaction_target'] is None, 'A queued carry action must be invalidated'
    for name in ('dropped', 'dropped-resumed'):
        state = checkpoints[name]
        assert state['physics']['held'] is None
        assert len(state['physics']['bodies']) == 2
        assert state['current_level_id'] == 'elevator_lobby'
        assert state['elevators'][0]['phase'] == 'closed'
    return dict(limit_m=limit, retained_below_m=near_distance, released_above_m=drop_distance,
                zero_release_velocity=True, retained_pose=True, dropped_save_resume=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--package', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--mode', choices=['normal', 'obstructed'], required=True)
    parser.add_argument('--lightmaps', choices=['off', 'full'], default='full')
    args = parser.parse_args()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    state = out / 'state'
    (state / 'levels').mkdir(parents=True)
    shutil.copy2(args.package, state / 'levels/cargo.placesmap')
    settings = json.loads((ROOT / 'tests/fixtures/native/hero-manifest.json').read_text())['settings']
    settings.update(window_width=640, window_height=360, quality='high',
                    texture_filtering='high', lightmaps=args.lightmaps, reflections='off',
                    bloom=False, vsync=True, developer_console=False, ambient_volume=0, walk_speed=3)
    (state / 'settings.json').write_text(json.dumps(settings))
    plan = out / 'actions.json'
    plan.write_text(json.dumps(actions(args.mode), indent=2) + '\n')
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
    result = verify(args.mode, rows, checkpoints)
    result.update(passed=True, mode=args.mode, lightmaps=args.lightmaps,
                  binary_sha256=hashlib.sha256(args.binary.read_bytes()).hexdigest(),
                  package_sha256=hashlib.sha256(args.package.read_bytes()).hexdigest(),
                  scope='macOS Metal direct binary; diagnostic starts, ordinary input; paced correctness capture')
    (out / 'passed.json').write_text(json.dumps(result, indent=2) + '\n')
    print('Native automatic-drop check passed: ' + args.mode)


if __name__ == '__main__':
    main()
