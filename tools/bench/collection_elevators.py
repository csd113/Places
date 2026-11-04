#!/usr/bin/env python3
"""Explicit direct-binary round trips through the Quiet Collection's sixteen lifts.

Uses the real exported project, normal movement and aimed button input. All
installation/settings/save state stays under --out. No build or bake occurs.
Initial staging poses position each test in front of its hub cabin; both travel
legs and boarding/exit use ordinary controls. Not part of automatic CI.
"""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import shutil
import subprocess
import time

ROOT = Path(__file__).resolve().parents[2]
PROJECT = ROOT / 'projects/environments'
DESTINATIONS = ('home', 'pool', 'office', 'arcade', 'lantern_hollow', 'winter', 'beach', 'aero')


def point(lift, x, y, z):
    angle = math.radians(lift['rotation_degrees'])
    return [lift['x'] + math.cos(angle)*x + math.sin(angle)*z,
            lift['stops'][0]['y'] + y,
            lift['z'] - math.sin(angle)*x + math.cos(angle)*z]


def local(lift, position):
    angle = math.radians(lift['rotation_degrees'])
    x, y, z = [a-b for a, b in zip(position, point(lift, 0, 0, 0))]
    return [math.cos(angle)*x - math.sin(angle)*z, y,
            math.sin(angle)*x + math.cos(angle)*z]


def actions(source, arrival, destination):
    steps = []
    hub, target = 'ready:hub', 'ready:'+destination
    def add(ms, kind, after=hub, **details):
        steps.append(dict(after=after, delay_ms=ms, action=dict(kind=kind, **details)))
    def aim(ms, lift, xyz, after=hub):
        add(ms, 'view_at', after, position=point(lift, *xyz))
    def click(ms, after=hub):
        add(ms, 'mouse', after, button='left', pressed=True)
        add(ms+70, 'mouse', after, button='left', pressed=False)
    def walk(ms, duration, after=hub):
        add(ms, 'key', after, key='W', pressed=True)
        add(ms+duration, 'key', after, key='W', pressed=False)
    add(100, 'key', key='F1', pressed=True)
    add(150, 'key', key='F1', pressed=False)
    add(250, 'checkpoint', name='campaign-start')
    add(50, 'key', key='Return', pressed=True)
    add(100, 'key', key='Return', pressed=False)
    add(150, 'player', position=point(source, 0, 1.6, -1), yaw=0, pitch=0)
    aim(200, source, (0, 1.6, 2)); walk(250, 450)
    add(750, 'checkpoint', name='closed_collision')
    add(800, 'player', position=point(source, -.96, 1.6, -1), yaw=0, pitch=0)
    aim(850, source, (-.96, 1.08, -.12)); click(950)
    add(2450, 'player', position=point(source, 0, 1.6, -1), yaw=0, pitch=0)
    aim(2500, source, (0, 1.6, 2)); walk(2550, 1250)
    add(3900, 'checkpoint', name='boarded')
    add(3950, 'capture', name='hub-interior')
    aim(4100, source, (.94, 1.58, .4)); click(4250)
    add(4450, 'checkpoint', name='source-settled')
    request = 'request:'+destination
    add(40, 'capture', request, name='frozen-a')
    add(220, 'capture', request, name='frozen-b')
    add(240, 'checkpoint', request, name='frozen')
    add(2000, 'checkpoint', target, name='destination-arrival')
    aim(2050, arrival, (0, 1.6, -2), target)
    add(2100, 'capture', target, name='arrival-open')
    walk(2300, 1450, target)
    add(3850, 'checkpoint', target, name='destination-hall')
    add(3900, 'capture', target, name='destination-hall')
    aim(4000, arrival, (-.96, 1.08, -.12), target)
    add(4150, 'checkpoint', target, name='return-call')
    add(4200, 'capture', target, name='return-call')
    click(4300, target)
    aim(4500, arrival, (0, 1.6, 2), target); walk(4600, 1450, target)
    add(6200, 'checkpoint', target, name='return-boarded')
    aim(6300, arrival, (.94, 1.58, .4), target); click(6450, target)
    add(6650, 'checkpoint', target, name='return-settled')
    add(12700, 'checkpoint', target, name='hub-return')
    aim(12800, source, (0, 1.6, -2), target)
    walk(13000, 1450, target)
    add(14700, 'checkpoint', target, name='hub-hall')
    add(14800, 'capture', target, name='hub-return-hall')
    add(15200, 'quit', target)
    add(38000, 'quit')
    for step in steps:
        if step['after'] == hub and step['action'].get('name') != 'campaign-start' and not (step['action'].get('key') == 'F1'):
            step['delay_ms'] += 300
    for anchor in (hub, target):
        steps.insert(0, dict(after=anchor, delay_ms=0, action=dict(kind='focus', focused=True)))
    return steps


def verify(out, source, arrival, destination, completed):
    from PIL import Image, ImageChops
    rows = [json.loads(line) for line in (out/'trace.jsonl').read_text().splitlines()]
    checkpoints = {r['event'][11:]: json.loads(r['detail']) for r in rows if r['event'].startswith('checkpoint:')}
    def at(name, level, lift, inside):
        data = checkpoints[name]
        assert data['current_level_id'] == level and data['state'] == 'Playing', (name, data)
        assert not data['transition_frozen'], (name, data)
        assert data['window_focused'], (name, 'Native window lost focus; rerun this case')
        assert not data['script_diagnostics'], (name, data['script_diagnostics'])
        pose = local(lift, data['player_position'])
        assert abs(pose[0]) < .08 and abs(pose[1]-1.6) < .03, (name, pose)
        assert (.4 < pose[2] < 2.1) if inside else (pose[2] < -.35), (name, pose)
        assert data['audio']['enabled'] and data['audio']['errors'] == 0, (name, data['audio'])
        return data
    if len(completed) != 8:
        assert str(len(completed))+' of 8 favors remembered' in str(checkpoints['campaign-start']['guidance']), checkpoints['campaign-start']['guidance']
    closed = checkpoints['closed_collision']
    assert local(source, closed['player_position'])[2] < .1, closed
    at('boarded', 'hub', source, True)
    entered = at('destination-arrival', destination, arrival, True)
    assert entered['elevators'][0]['phase'] in ('opening', 'open'), entered['elevators']
    destination_hall = at('destination-hall', destination, arrival, False)
    assert destination_hall['elevators'][0]['phase'] == 'open', destination_hall['elevators']
    # Captures sample the refreshed interaction target at presentation. A slow
    # PNG readback can deliver a timed camera and checkpoint in the same tick.
    called = next(json.loads(r['detail']) for r in rows if r['event'] == 'native_capture:return-call')
    assert 'call' in str(called['interaction_target']), called['interaction_target']
    at('return-boarded', destination, arrival, True)
    returned = at('hub-return', 'hub', source, True)
    index = next(i for i, e in enumerate(json.loads((PROJECT/'levels/hub.json').read_text())['elevators']) if e['id'] == source['id'])
    assert returned['elevators'][index]['phase'] in ('opening', 'open'), returned['elevators']
    for before, after, src, dst in [('source-settled', 'destination-arrival', source, arrival),
                                    ('return-settled', 'hub-return', arrival, source)]:
        offset0 = local(src, checkpoints[before]['player_position'])
        offset1 = local(dst, checkpoints[after]['player_position'])
        assert max(abs(a-b) for a,b in zip(offset0, offset1)) < .025, (before, offset0, offset1)
    hall = at('hub-hall', 'hub', source, False)
    assert hall['elevators'][index]['phase'] == 'open', hall['elevators']
    assert [r['detail'] for r in rows if r['event'] == 'request'] == ['hub', destination, 'hub']
    assert checkpoints['frozen']['transition_frozen']
    assert returned['doors'][0]['locked'] == (len(completed) != 8), returned['doors']
    saved = list((out/'state/saves').glob('campaign-*.json'))
    assert len(saved) == 1, saved
    progress = json.loads(saved[0].read_text())
    assert sorted(progress['completed']) == sorted(completed) and progress['generation'] == 7, progress
    assert returned['audio']['peak_amplitude'] < 1 and returned['audio']['dropped'] == 0, returned['audio']
    with Image.open(out/'state/frozen-a.png') as a, Image.open(out/'state/frozen-b.png') as b:
        assert ImageChops.difference(a, b).getbbox() is None, 'Loading continuity changed'
    return dict(passed=True, destination=destination, checkpoints=list(checkpoints),
                checks=['closed-door collision', 'call and walk into hub cabin', 'button-triggered destination load',
                        'identical frozen loading frames', 'exact arrival cabin pose', 'walk out and back in',
                        'reachable return hall call', 'button-triggered return to matching hub cabin', 'walk into hub',
                        'saved favors and reward lock retained', 'native audio without errors or clipping'])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--package', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--only', choices=DESTINATIONS)
    parser.add_argument('--progress', choices=['none', 'partial', 'all'], default='none',
                        help='Seed a disposable campaign fixture to verify persistence and reward locking')
    args = parser.parse_args()
    binary, package = args.binary.resolve(strict=True), args.package.resolve(strict=True)
    args.out = args.out.resolve(); args.out.mkdir(parents=True, exist_ok=False)
    hub = json.loads((PROJECT/'levels/hub.json').read_text())
    for destination in ([args.only] if args.only else DESTINATIONS):
        source = next(e for e in hub['elevators'] if e['id'] == 'hub_'+destination)
        arrival = json.loads((PROJECT/f'levels/{destination}.json').read_text())['elevators'][0]
        out = args.out/destination; state = out/'state'; (state/'levels').mkdir(parents=True)
        # Keep a private real package to exercise normal discovery/admission.
        installed = state/'levels/environments.placesmap'
        shutil.copy2(package, installed)
        completed = list(DESTINATIONS) if args.progress == 'all' else ['pool', 'winter'] if args.progress == 'partial' else []
        identity = hashlib.sha256(('project:environments\n'+str(installed.resolve())).encode()).hexdigest()
        saves = state/'saves'; saves.mkdir()
        (saves/f'campaign-{identity}.json').write_text(json.dumps(dict(completed=completed, generation=7)))
        settings = json.loads((ROOT/'tests/fixtures/native/hero-manifest.json').read_text())['settings']
        settings.update(window_width=960, window_height=640, quality='high', texture_filtering='high',
                        lightmaps='off', reflections='off', bloom=False, vsync=True,
                        walk_speed=2, fov_degrees=75, master_volume=.75, effects_volume=.8, ambient_volume=0)
        (state/'settings.json').write_text(json.dumps(settings))
        (out/'actions.json').write_text(json.dumps(actions(source, arrival, destination)))
        env = {k:v for k,v in os.environ.items() if not k.startswith('PLACES_')}
        env.update(PLACES_ASSET_ROOT=str(ROOT/'assets'), PLACES_STATE_ROOT=str(state), PLACES_LEVEL='hub',
                   PLACES_BENCH='1', PLACES_BENCH_FRAMES='6000', PLACES_BENCH_WARMUP='3', PLACES_VERBOSE='1',
                   PLACES_PREPARE_DELAY_MS='500', PLACES_LOAD_TRACE=str(out/'trace.jsonl'),
                   PLACES_BENCH_ACTIONS=str(out/'actions.json'), PLACES_BENCH_OUT=str(out/'frames.csv'),
                   PLACES_AUDIO_CAPTURE='1')
        start = time.monotonic()
        with (out/'game.log').open('w') as log:
            subprocess.run([str(binary)], cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT, check=True, timeout=55)
        receipt = verify(out, source, arrival, destination, completed)
        receipt.update(binary=str(binary), binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
                       package_sha256=hashlib.sha256(package.read_bytes()).hexdigest(), seconds=time.monotonic()-start)
        (out/'passed.json').write_text(json.dumps(receipt, indent=2)+'\n')
        print(destination+': passed', flush=True)


if __name__ == '__main__':
    main()
