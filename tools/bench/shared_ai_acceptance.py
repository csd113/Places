"""Opt-in native AI acceptance: real movement, socket contact, original body ownership and reload lifetime. Raw evidence stays in --out; no builds or user-facing mode."""
from pathlib import Path
import argparse, os, json, subprocess, shutil, hashlib, math, time
R = Path(__file__).resolve().parents[2]
LEVEL = 'shared_ai_acceptance'

def write(p, d):
    p.write_text(json.dumps(d, indent=2) + '\n', encoding='utf8')

def sha(p):
    with p.open('rb') as f:
        return hashlib.file_digest(f, 'sha256').hexdigest()

class Plan:

    def __init__(self):
        self.steps = []

    def a(self, ms, kind, **data):
        self.steps.append(dict(after='ready:' + LEVEL, delay_ms=ms, action=dict(kind=kind, **data)))

    def control(self, ms, name):
        self.a(ms, 'activate', id=name)

    def snap(self, ms, name, capture=False):
        self.a(ms, 'capture' if capture else 'checkpoint', name=name)

    def tap(self, ms, key):
        self.a(ms, 'key', key=key, pressed=True)
        self.a(ms + 40, 'key', key=key, pressed=False)

    def player(self, ms, point, yaw=0, pitch=-15):
        self.a(ms, 'player', position=point, yaw=yaw, pitch=pitch)

    def finish(self, ms):
        for at in range(0, ms, 1000):
            self.a(at, 'focus', focused=True)
        self.a(ms, 'quit')
        self.steps.sort(key=lambda s: s['delay_ms'])
        assert len(self.steps) <= 128
        return self.steps

def plan(case):
    p = Plan()
    p.snap(100, 'initial', True)
    if case in ['search', 'moving_item']:
        p.player(200, [-6, 1.6, 0.6])
        p.a(300, 'aim_entity', id='porter_a')
        p.a(250 if case == 'search' else 700, 'item_pose', id='parcel', position=[3, 0, -1], rotation=[0, 0, 0, 1])
        p.control(400, 'fetch_a')
        p.player(23000, [-3, 1.6, 4])
        p.a(23100, 'aim_entity', id='porter_a')
        p.player(39000, [5, 1.6, 3])
        p.a(39100, 'aim_entity', id='porter_a')
        for at in [1000, 4000, 8000, 14000, 22000, 30000, 36000, 44000, 52000, 57000]:
            p.snap(at, f't{at}', at in [22000, 36000, 52000])
        return p.finish(58500)
    if case == 'layers':
        p.control(400, 'fetch_a')
        p.player(19000, [6, 4.6, 6])
        p.a(19100, 'aim_entity', id='porter_a')
        p.a(19900, 'aim_entity', id='porter_a')
        p.player(28000, [14, 1.6, 6.5])
        p.a(28100, 'aim_entity', id='porter_a')
        p.a(29900, 'aim_entity', id='porter_a')
        p.a(49900, 'aim_entity', id='porter_a')
        for at in [1000, 6000, 10000] + list(range(14000, 46000, 2000)) + [50000, 56000]:
            p.snap(at, f't{at}', at in [20000, 30000, 50000])
        return p.finish(58500)
    if case == 'light_cargo':
        p.control(400, 'fetch_a')
        p.player(2900, [-2, 1.6, 3])
        p.a(3000, 'aim_entity', id='porter_a')
        p.tap(3300, 'F')
        p.snap(3600, 'cargo-lit')
        p.snap(4500, 'cargo-held', True)
        p.tap(4800, 'F')
        p.snap(6500, 'cargo-resumed')
        p.snap(16000, 'cargo-delivered', True)
        return p.finish(20000)
    if case in ['fetch', 'competing', 'fetch_shelf']:
        p.player(200, [-6, 1.6, 0.6], yaw=135)
        p.a(300, 'aim_entity', id='porter_a')
        p.control(400, 'fetch_shelf' if case == 'fetch_shelf' else 'fetch_a' if case == 'fetch' else 'competing')
        p.player(5500, [-3, 1.6, 3], yaw=90)
        p.a(5600, 'aim_entity', id='porter_a')
        p.player(11500, [5, 1.6, 3], yaw=0)
        p.a(11600, 'aim_entity', id='porter_a')
        for at in [1000, 2000, 3000, 4500, 6500, 9000, 12000, 16000, 20000, 24000]:
            p.snap(at, f't{at}', at in [2000, 6500, 16000])
        return p.finish(24500)
    if case in ['held', 'removed', 'stolen']:
        if case == 'stolen':
            p.control(250, 'fetch_a')
        p.player(200, [-3.8, 1.6, -0.8])
        p.a(350, 'aim_item', id='parcel')
        p.tap(500, 'E')
        p.snap(750, 'player-held')
        p.control(900, 'fetch_a')
        p.snap(1600, 'npc-refused-held')
        if case == 'removed':
            p.tap(1800, '`')
            p.a(2000, 'text', text='/remove', submit=True)
            p.a(2200, 'escape')
            p.snap(3000, 'removed')
            p.snap(8000, 'removed-recovery')
            return p.finish(8500)
        p.a(1800, 'view', yaw=90, pitch=-20)
        p.tap(1900, 'E')
        p.player(2300, [6, 1.6, 5])
        p.snap(2600, 'loose-again')
        for at in [4000, 6500, 10000, 15000, 22000, 27000]:
            p.snap(at, f't{at}', at in [6500, 22000])
        return p.finish(27500)
    if case == 'interruptions':
        p.control(400, 'fetch_a')
        p.control(700, 'pause_a')
        p.snap(1000, 'paused-pickup')
        p.snap(1800, 'paused-held')
        p.control(2000, 'resume_a')
        p.snap(3500, 'resumed-pickup')
        p.a(4200, 'ragdoll', id='porter_a', enabled=True)
        p.snap(4500, 'ragdoll-with-cargo', True)
        p.a(5500, 'ragdoll', id='porter_a', enabled=False)
        p.snap(6000, 'reset-ragdoll')
        p.control(6500, 'fetch_a')
        p.control(9000, 'cancel_a')
        p.snap(9500, 'cancelled', True)
        p.snap(12500, 'cancelled-later')
        return p.finish(13000)
    if case == 'light':
        p.player(200, [-4, 1.6, 0.6], yaw=0)
        p.a(300, 'aim_entity', id='porter_a')
        p.control(400, 'fetch_a')
        p.tap(600, 'F')
        p.snap(1000, 'lit')
        p.snap(2000, 'light-held', True)
        p.tap(2200, 'F')
        p.snap(4000, 'released')
        p.snap(8500, 'moving-cargo', True)
        return p.finish(11000)
    if case == 'routine':
        p.control(400, 'routine_start')
        p.snap(2000, 'routine-moving')
        p.control(2500, 'routine_interrupt')
        p.snap(3200, 'urgent')
        p.snap(6000, 'resumed')
        p.control(6500, 'sequence')
        p.snap(7200, 'sequence')
        p.snap(9000, 'after-sequence')
        p.a(10500, 'activate', id='pause_routine')
        p.snap(11000, 'routine-paused')
        p.snap(12500, 'routine-paused-later')
        p.a(13000, 'activate', id='resume_routine')
        p.snap(15500, 'routine-resumed', True)
        p.snap(21000, 'routine-cycle', True)
        return p.finish(21500)
    if case == 'follow':
        p.control(400, 'follow')
        p.snap(1200, 'seen')
        p.player(2000, [-0.7, 1.6, -3])
        p.snap(2400, 'hidden-target')
        p.snap(3500, 'last-known')
        p.snap(6500, 'lost')
        p.player(7500, [6, 1.6, 2])
        p.snap(8500, 'reacquired')
        p.player(8700, [6, 1.6, -2])
        p.a(9000, 'item_pose', id='follow_blocker', position=[5.5, 0.005, 1.4], rotation=[0, 0, 0, 1])
        p.snap(9300, 'dynamic-before-moving')
        p.player(9400, [7, 1.6, -2])
        p.snap(9700, 'dynamic-hidden')
        p.snap(10500, 'obstructed')
        p.a(11000, 'item_pose', id='follow_blocker', position=[-6, 0, -3], rotation=[0, 0, 0, 1])
        p.player(12000, [3, 1.6, 5])
        p.snap(15500, 'route-changed', True)
        return p.finish(16500)
    if case == 'senses':
        p.control(400, 'hear_door')
        p.control(700, 'noise')
        p.snap(1100, 'wrong-category')
        p.control(1500, 'hear_small')
        p.control(1800, 'noise')
        p.snap(2200, 'outside-radius')
        p.control(2600, 'hear_any')
        p.control(2900, 'noise')
        p.snap(3300, 'heard-any')
        p.control(4400, 'see_object')
        p.snap(4800, 'object-occluded')
        p.a(5200, 'item_pose', id='parcel', position=[6, 0, -3], rotation=[0, 0, 0, 1])
        p.snap(5500, 'object-seen', True)
        p.control(6700, 'see_player')
        p.snap(7100, 'player-seen')
        p.control(8300, 'see_entity')
        p.snap(8700, 'entity-seen')
        p.control(9900, 'see_type')
        p.snap(10300, 'type-seen')
        p.control(11000, 'see_fov')
        p.snap(11400, 'outside-fov')
        p.control(12400, 'see_range')
        p.snap(12800, 'outside-range')
        return p.finish(13500)
    if case == 'alerts':
        p.control(400, 'noise')
        p.snap(1000, 'propagated')
        p.snap(2500, 'single-alert')
        p.snap(7000, 'expired')
        p.snap(10000, 'no-feedback')
        return p.finish(10500)
    if case in ['switch', 'door', 'chair']:
        p.player(200, [6, 1.6, -1.5])
        p.a(300, 'aim_entity', id='routine')
        p.control(400, 'use_' + case)
        if case == 'chair':
            p.player(4500, [6, 1.6, 3.5])
            p.a(4600, 'aim_entity', id='routine')
            p.snap(7500, 'seated-contact', True)
        for at in [600, 900, 1300, 2000, 3500, 6000, 9000, 12000]:
            p.snap(at, f't{at}', at in [900, 2000, 9000])
        return p.finish(12500)
    if case == 'caught':
        p.control(400, 'caught_start')
        p.player(500, [-3, 1.6, 2])
        p.a(550, 'aim_entity', id='caught_predator#2')
        for at, name in [(600, 'spawned'), (1100, 'caught'), (1800, 'caught-sequence'), (4000, 'consumed'), (7500, 'recovered')]:
            p.snap(at, name, at in [1800, 7500])
        return p.finish(8000)
    if case == 'reload':
        p.control(400, 'fetch_a')
        p.snap(2500, 'owned-before-unload', True)
        p.a(4000, 'load', level='navigation_layers')
        p.snap(13000, 'other-world-1')
        for at in range(0, 4000, 1000):
            p.a(at, 'focus', focused=True)
        anchor = 'checkpoint:other-world-1'
        stages = [(LEVEL, 'reloaded-1'), ('navigation_layers', 'other-world-2'), (LEVEL, 'reloaded-2'), ('navigation_layers', 'other-world-3'), (LEVEL, 'reloaded-stable')]
        for level, name in stages:
            p.steps.append(dict(after=anchor, delay_ms=1000, action=dict(kind='load', level=level)))
            for at in [500, 1500, 3000, 5000, 7000, 8500]:
                p.steps.append(dict(after=anchor, delay_ms=at, action=dict(kind='focus', focused=True)))
            p.steps.append(dict(after=anchor, delay_ms=9000, action=dict(kind='checkpoint', name=name)))
            anchor = 'checkpoint:' + name
        p.steps.append(dict(after=anchor, delay_ms=500, action=dict(kind='quit')))
        assert len(p.steps) <= 128
        return p.steps
    raise ValueError(case)

def actor(sample, name):
    return next((a for a in sample['entity_simulation']['agents'] if a['id'] == name))

def tasks(sample, name):
    return actor(sample, name).get('tasks', [])

def attachments(sample):
    return sample['carry_assemblies']['attachments']

def body_key(sample, name='parcel'):
    return next((row[0] for row in sample['physics']['authored'] if row[1] == name))

def large_blocker_ray(sample):
    origin = [a + b for a, b in zip(actor(sample, 'escort')['position'], [0, 0.9, 0])]
    target = [sample['player_position'][0], sample['player_body']['feet_y'] + 0.9, sample['player_position'][2]]
    for low, high in sample['physical_obstacles']:
        if max((b - a for a, b in zip(low, high))) < 1:
            continue
        enter, leave = (0.0, 1.0)
        for a, b, lo, hi in zip(origin, target, low, high):
            delta = b - a
            if abs(delta) < 1e-08:
                if a < lo or a > hi:
                    enter = 2
                    break
            else:
                first, last = sorted([(lo - a) / delta, (hi - a) / delta])
                enter = max(enter, first)
                leave = min(leave, last)
        if enter <= leave:
            return True
    return False

def check(rows, name, passed, **data):
    rows.append(dict(name=name, passed=bool(passed), **data))

def audit(case, samples, events):
    checks = []
    samples = dict(samples)
    for index, event in enumerate(events):
        if event['event'] in ['npc_pickup_hand_contact', 'npc_putdown_hand_contact']:
            detail = json.loads(event['detail'])
            if isinstance(detail, dict) and 'carry_assemblies' in detail and ('physics' in detail):
                samples['physical-contact-' + str(index)] = detail
    values = list(samples.values())
    check(checks, 'native snapshots exist', bool(values))
    if not values:
        return checks
    for name, s in samples.items():
        check(checks, name + ': no script failure', not any(('error' in m.lower() or 'budget exhausted' in m.lower() for m in s.get('script_diagnostics', []))))
    if case in ['fetch', 'competing', 'held', 'stolen', 'fetch_shelf', 'search', 'moving_item', 'layers']:
        seen = [(name, a) for name, s in samples.items() for a in attachments(s) if a['item'] == 'parcel']
        check(checks, 'actual original item attached to an NPC', bool(seen), actors=[a['actor'] for _, a in seen])
        key = body_key(samples['initial'])
        check(checks, 'same original render/physics identity', bool(seen) and all((a['key'] == key for _, a in seen)), key=key)
        check(checks, 'no duplicate physics body while carried', all((not any((b['key'] == a['key'] for b in samples[name]['physics']['bodies'])) for name, a in seen)))
        check(checks, 'original item render resource remains visible', bool(seen) and all((a.get('item_mesh_visible') for _, a in seen)))
        check(checks, 'at most one NPC owns the parcel', all((sum((a['item'] == 'parcel' for a in attachments(s))) <= 1 for s in values)))
        complete = [s for s in values if any((a.get('last_result') == 'delivery:complete' for a in s['entity_simulation']['agents']))]
        check(checks, 'physical delivery completes', bool(complete))
        check(checks, 'destination interaction after release', any((any((i == (2 if case == 'fetch_shelf' else 0) and (not on) for i, on in s['light_states'])) and (not attachments(s)) for s in complete)))
        if case in ['held', 'stolen']:
            check(checks, 'player hold refused by NPC', samples['player-held']['physics']['held'] == key and (not attachments(samples['npc-refused-held'])))
    if case in ['search', 'moving_item']:
        check(checks, 'hidden item is not physically claimed early', not attachments(samples['t1000']) and (not attachments(samples['t4000'])))
    if case == 'layers':
        check(checks, 'actual carry reaches upper storey', any((actor(s, 'porter_a')['position'][1] > 2.9 and attachments(s) for s in values)))
        check(checks, 'actual cargo descends the continuous ramp', any((0.2 < actor(s, 'porter_a')['position'][1] < 2.8 and attachments(s) for s in values)))
        check(checks, 'delivery returns to ground storey', any((abs(actor(s, 'porter_a')['position'][1]) < 0.05 for s in complete)))
    if case == 'light_cargo':
        check(checks, 'light holds the same physical cargo', bool(attachments(samples['cargo-lit'])) and bool(attachments(samples['cargo-held'])))
        check(checks, 'cargo light hold stops locomotion', actor(samples['cargo-held'], 'porter_a')['light_held'] and math.dist(actor(samples['cargo-lit'], 'porter_a')['position'], actor(samples['cargo-held'], 'porter_a')['position']) < 0.001)
    if case == 'fetch_shelf':
        check(checks, 'item rests on actual raised surface', any((b['world_center_of_mass'][1] > 0.8 for sample in complete for b in sample['physics']['bodies'] if b['key'] == key)))
    if case == 'fetch':
        check(checks, 'released parcel does not trap baseline movement', math.dist(actor(samples['t12000'], 'porter_a')['position'], actor(samples['t24000'], 'porter_a')['position']) > 1)
    if case == 'removed':
        check(checks, 'removed item has no remaining body', not any((body['key'] == body_key(samples['initial']) for body in samples['removed']['physics']['bodies'])))
        check(checks, 'removed target leaves no claimed reservation', samples['removed-recovery']['entity_simulation']['ai_runtime']['reservations'] == 0 and (not attachments(samples['removed-recovery'])))
    if case == 'interruptions':
        check(checks, 'paused pickup has no attachment', not attachments(samples['paused-held']))
        check(checks, 'paused task clock preserved', tasks(samples['paused-pickup'], 'porter_a')[0]['progress']['elapsed'] == tasks(samples['paused-held'], 'porter_a')[0]['progress']['elapsed'])
        check(checks, 'ragdoll releases original cargo', not attachments(samples['ragdoll-with-cargo']))
        check(checks, 'cancel clears task and reservations', not tasks(samples['cancelled-later'], 'porter_a') and samples['cancelled-later']['entity_simulation']['ai_runtime']['reservations'] == 0)
    if case == 'light':
        a, b = (actor(samples['lit'], 'porter_a'), actor(samples['light-held'], 'porter_a'))
        check(checks, 'real light holds locomotion', a['light_held'] and b['light_held'] and (math.dist(a['position'], b['position']) < 0.001))
        check(checks, 'light hold cannot claim a loose body', not attachments(samples['light-held']))
        check(checks, 'release resumes task', not actor(samples['released'], 'porter_a')['light_held'])
    if case == 'follow':
        seen = tasks(samples['seen'], 'escort')[0]['progress']['last_known']
        hidden = tasks(samples['last-known'], 'escort')[0]['progress']['last_known']
        check(checks, 'hidden player does not update last-known state', seen == hidden, seen=seen, hidden=hidden)
        check(checks, 'fair target-loss state', tasks(samples['lost'], 'escort')[0]['progress']['status'] == 'target_lost')
        check(checks, 'reacquires an actual new sighting', tasks(samples['reacquired'], 'escort')[0]['progress']['last_known'] != seen)
        check(checks, 'follow makes physical progress', math.dist(actor(samples['seen'], 'escort')['position'], actor(samples['route-changed'], 'escort')['position']) > 1)
        check(checks, 'temporary solid actually occludes the moving player', large_blocker_ray(samples['dynamic-hidden']))
        check(checks, 'solid-hidden movement cannot update memory', tasks(samples['dynamic-hidden'], 'escort')[0]['progress']['last_known'] == tasks(samples['dynamic-before-moving'], 'escort')[0]['progress']['last_known'])
    if case == 'routine':
        check(checks, 'urgent task preempts routine', any((t['key'] == 'urgent' and t['active'] for t in tasks(samples['urgent'], 'routine'))))
        check(checks, 'routine continuation survives interruption', any((t['key'] == 'daily' for t in tasks(samples['after-sequence'], 'routine'))))
        first = tasks(samples['routine-paused'], 'routine')[0]['progress']
        later = tasks(samples['routine-paused-later'], 'routine')[0]['progress']
        check(checks, 'explicit pause preserves routine clock', first == later)
        check(checks, 'resume progresses routine', tasks(samples['routine-resumed'], 'routine')[0]['progress'] != later)
    if case == 'alerts':
        for name in ['alert_1', 'alert_2']:
            check(checks, name + ' receives one bounded alert', sum((t['key'].startswith('alert_') for t in tasks(samples['single-alert'], name))) == 1)
        check(checks, 'alerts expire without feedback', samples['no-feedback']['entity_simulation']['ai_runtime']['alerts'] == 0 and (not any((t['key'].startswith('alert_') for a in samples['no-feedback']['entity_simulation']['agents'] for t in a['tasks']))))
    if case in ['switch', 'chair']:
        check(checks, 'environment use completes', any((actor(s, 'routine').get('last_result') == 'environment:complete' for s in values)))
    if case == 'switch':
        check(checks, 'real hand contact changes mapped switch', any((any((i == 1 and (not on) for i, on in s['light_states'])) for s in values)))
    if case == 'door':
        check(checks, 'real door leaf opens', any((any((d['id'] == 'task_door' and abs(d['angle_degrees']) > 30 for d in s['doors'])) for s in values)))
        check(checks, 'door use completes', any((actor(s, 'routine').get('last_result') == 'environment:complete' for s in values)))
    if case == 'senses':
        check(checks, 'selected category rejects other noises', not tasks(samples['wrong-category'], 'alert_0'))
        check(checks, 'NPC hearing radius enforced', not tasks(samples['outside-radius'], 'alert_0'))
        check(checks, 'any filter maps hearing to task', any((t['key'] == 'hear_any' for t in tasks(samples['heard-any'], 'alert_0'))))
        check(checks, 'real wall blocks object sight', not any((t['key'] == 'saw_object' for t in tasks(samples['object-occluded'], 'routine'))))
        for snap, key, npc in [('object-seen', 'saw_object', 'routine'), ('player-seen', 'saw_player', 'alert_0'), ('entity-seen', 'saw_entity', 'routine'), ('type-seen', 'saw_type', 'routine')]:
            check(checks, snap + ' typed sight mapping', any((t['key'] == key for t in tasks(samples[snap], npc))))
    if case == 'senses':
        check(checks, 'configured FOV rejects side object', not any((t['key'] == 'saw_see_fov' for t in tasks(samples['outside-fov'], 'routine'))))
        check(checks, 'configured sight range enforced', not any((t['key'] == 'saw_see_range' for t in tasks(samples['outside-range'], 'routine'))))
    if case == 'caught':
        prey = actor(samples['caught'], 'caught_prey#1')
        pred = actor(samples['caught-sequence'], 'caught_predator#2')
        check(checks, 'caught overrides and cancels highest-priority task', prey['state'] == 'caught' and prey['owner'] == 'caught' and (not prey['tasks']))
        check(checks, 'predator authored catch sequence advances', pred['state'] == 'catch' and samples['caught-sequence']['entity_simulation']['sequences'] > 0)
        check(checks, 'caught prey despawns through authored sequence', not any((a['id'] == 'caught_prey#1' for a in samples['recovered']['entity_simulation']['agents'])))
        check(checks, 'predator recovers baseline after catch sequence', actor(samples['recovered'], 'caught_predator#2')['owner'] == 'baseline' and samples['recovered']['entity_simulation']['sequences'] == 0)
    if case == 'reload':
        old = samples['owned-before-unload']['entity_simulation']['ai_runtime']['epoch']
        new = samples['reloaded-stable']['entity_simulation']['ai_runtime']
        check(checks, 'new world epoch on reload', new['epoch'] != old)
        check(checks, 'three reload cycles have distinct epochs', len({samples[name]['entity_simulation']['ai_runtime']['epoch'] for name in ['reloaded-1', 'reloaded-2', 'reloaded-stable']}) == 3)
        check(checks, 'physics body count returns to authored bound', all((len(samples[name]['physics']['bodies']) == len(samples['initial']['physics']['bodies']) for name in ['reloaded-1', 'reloaded-2', 'reloaded-stable'])))
        check(checks, 'no old tasks/reservations on reload', new['tasks'] == new['reservations'] == 0 and (not attachments(samples['reloaded-stable'])))
        check(checks, 'actual authored item restored', len(samples['reloaded-stable']['physics']['bodies']) >= 1)
    return checks

def run(case, binary, package, nav, out, source):
    out.mkdir(parents=True, exist_ok=False)
    (out / 'levels').mkdir()
    shutil.copy2(package, out / 'levels' / package.name)
    shutil.copy2(nav, out / 'levels' / nav.name)
    settings = dict(window_width=960, window_height=540, window_mode='windowed', quality='high', lightmaps='static', reflections='off', bloom=True, vsync=False, developer_console=True)
    write(out / 'settings.json', settings)
    actions = plan(case)
    write(out / 'actions.json', actions)
    env = {k: v for k, v in os.environ.items() if not k.startswith('PLACES_') and k != 'SDL_VIDEODRIVER'}
    env.update(PLACES_ASSET_ROOT=str(R / 'assets'), PLACES_STATE_ROOT=str(out), PLACES_LEVEL=LEVEL, PLACES_BENCH='1', PLACES_BENCH_FRAMES='0', PLACES_VERBOSE='1', PLACES_VSYNC='off', PLACES_BENCH_ACTIONS=str(out / 'actions.json'), PLACES_LOAD_TRACE=str(out / 'trace.jsonl'), PLACES_BENCH_RESOURCE_TRACE='1')
    write(out / 'environment.json', {k: v for k, v in env.items() if k.startswith('PLACES_')})
    write(out / 'identity.json', dict(binary=str(binary), binary_sha256=sha(binary), package=str(package), package_sha256=sha(package), source_sha256=sha(source)))
    print('Native shared AI', case, flush=True)
    with (out / 'native.log').open('w', encoding='utf8') as log:
        child = subprocess.run([str(binary)], cwd=R, env=env, stdout=log, stderr=subprocess.STDOUT, timeout=180)
    events = [json.loads(line) for line in (out / 'trace.jsonl').read_text(encoding='utf8').splitlines()]
    samples = {e['event'].split(':', 1)[1]: json.loads(e['detail']) for e in events if e['event'].startswith(('native_capture:', 'checkpoint:'))}
    write(out / 'snapshots.json', samples)
    try:
        checks = audit(case, samples, events)
    except (KeyError, IndexError, StopIteration, TypeError) as error:
        checks = [dict(name='audit fields available', passed=False, error=repr(error))]
    executed = {json.loads(e['detail'])['id'] for e in events if e['event'] == 'action'}
    check(checks, 'all planned native actions execute', executed == set(range(len(actions))), executed=len(executed), planned=len(actions))
    logtext = (out / 'native.log').read_text(encoding='utf8')
    check(checks, 'no native fatal/validation/goal refusal', not any((word in logtext.lower() for word in ['panicked', 'validation error', 'fatal device', 'catalog hash mismatch', 'diagnostic goal refused'])))
    check(checks, 'direct native process shutdown', child.returncode == 0 and events[-1]['event'] == 'shutdown_complete')
    result = dict(case=case, exit_code=child.returncode, checks=checks, binary_sha256=sha(binary), package_sha256=sha(package), captures=[e['event'] for e in events if e['event'].startswith('native_capture:')])
    write(out / 'receipt.json', result)
    print(case, 'failed:', [c['name'] for c in checks if not c['passed']], flush=True)
    return result
if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--package', type=Path, required=True)
    parser.add_argument('--nav-package', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--cases', default='fetch')
    parser.add_argument('--level', default=LEVEL)
    parser.add_argument('--source', type=Path, default=R / 'tests/fixtures/levels/shared_ai_acceptance.json')
    args = parser.parse_args()
    LEVEL = args.level
    results = []
    for case in args.cases.split(','):
        results.append(run(case, args.binary.resolve(), args.package.resolve(), args.nav_package.resolve(), args.out.resolve() / case, args.source.resolve()))
        write(args.out / 'results.json', results)
    raise SystemExit(0 if all((c['passed'] for r in results for c in r['checks'])) else 1)
