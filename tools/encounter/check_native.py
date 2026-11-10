#!/usr/bin/env python3
"""Explicit Mac encounter lifecycle campaign; never run by Cargo or CI.

Requires a freshly built encounter_lab.placesmap. Runs one visible game process,
uses ordinary authored button dispatch, and measures real process RSS at native
checkpoints. Lethal health actions accelerate corpse creation; the campaign is
not a substitute for manual/visual crowbar and companion combat acceptance.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]

def actions(rounds, respawns=4):
    steps = []
    def add(anchor, ms, kind, **fields):
        steps.append(dict(after=anchor, delay_ms=ms, action=dict(kind=kind, **fields)))
    anchor = 'ready:encounter_lab'
    add(anchor, 0, 'focus', focused=True)
    # Keep the player alive through the same crowded native simulation. Health
    # stays enabled. No robot is present during forced twenty-corpse cohorts.
    add(anchor, 100, 'health_rules', rules=dict(enabled=True, regeneration=True,
        regeneration_hp_per_second=100, regeneration_delay_seconds=0, damage_cooldown_seconds=.35))
    add(anchor, 300, 'view', yaw=0, pitch=10)
    # Initialize lazy world physics before comparing retained ownership.
    add(anchor, 200, 'activate', id='companion_button')
    add(anchor, 600, 'health', target=dict(kind='entity', id='encounter:robot'), change=dict(kind='damage', hp=65535))
    add(anchor, 1800, 'activate', id='companion_button')
    add(anchor, 2500, 'checkpoint', name='baseline')
    anchor = 'checkpoint:baseline'
    for number in range(rounds):
        label = f'round-{number:02d}'
        add(anchor, 100, 'activate', id='encounter_control')
        add(anchor, 300, 'activate', id='encounter_control')
        add(anchor, 1300, 'checkpoint', name=label+'-first')
        add(anchor, 12500, 'checkpoint', name=label+'-full')
        if number in (0, rounds-1): add(anchor, 12600, 'capture', name=label+'-living')
        for zombie in range(20):
            add(anchor, 13000, 'health', target=dict(kind='entity', id=f'encounter:zombie:{zombie}'), change=dict(kind='damage', hp=65535))
        add(anchor, 16000, 'checkpoint', name=label+'-dead')
        if number in (0, rounds-1): add(anchor, 16100, 'capture', name=label+'-corpses')
        add(anchor, 16500, 'activate', id='clear_button')
        add(anchor, 17200, 'checkpoint', name=label+'-clear')
        add(anchor, 17400, 'activate', id='companion_button')
        add(anchor, 17800, 'activate', id='companion_button')
        add(anchor, 18100, 'activate', id='encounter_control')
        add(anchor, 19300, 'activate', id='clear_button')
        add(anchor, 20100, 'checkpoint', name=label+'-end')
        anchor = 'checkpoint:'+label+'-end'
    for toggle in range(64):
        add(anchor, 100+toggle*150, 'activate', id='companion_button')
    add(anchor, 11000, 'checkpoint', name='toggle-stress')
    add(anchor, 11500, 'health_rules', rules=dict(enabled=True, damage_cooldown_seconds=.35))
    for reset in range(respawns):
        offset = 12000 if reset == 0 else 100
        add(anchor, offset, 'activate', id='encounter_control')
        add(anchor, offset+200, 'activate', id='companion_button')
        add(anchor, offset+1600, 'health', target=dict(kind='player'), change=dict(kind='damage', hp=65535))
        add(anchor, offset+2400, 'capture', name=f'death-{reset}')
        add(anchor, offset+3200, 'key', key='E', pressed=True)
        add(anchor, offset+3500, 'key', key='E', pressed=False)
        add(anchor, offset+4200, 'checkpoint', name=f'respawn-{reset}')
        add(anchor, offset+4500, 'activate', id='encounter_control')
        add(anchor, offset+5800, 'checkpoint', name=f'after-respawn-start-{reset}')
        add(anchor, offset+6000, 'activate', id='clear_button')
        add(anchor, offset+6500, 'checkpoint', name=f'reset-end-{reset}')
        anchor = f'checkpoint:reset-end-{reset}'
    add(anchor, 500, 'checkpoint', name='final-clear')
    add(anchor, 1000, 'quit')
    assert len(steps) <= 512, len(steps)
    assert all(0 <= step['delay_ms'] <= 60000 for step in steps)
    return steps

def analyze(trace, rounds):
    states = {}
    for line in trace.read_text().splitlines():
        row = json.loads(line)
        if row['event'].startswith('checkpoint:'):
            states[row['event'][11:]] = json.loads(row['detail'])
    baseline = states['baseline']
    samples = []
    for number in range(rounds):
        label = f'round-{number:02d}'
        full, dead, cleared = [states[label+suffix] for suffix in ('-full', '-dead', '-clear')]
        e = full['encounter']
        assert len(e['zombies']) == 20 and sum(z['fast'] for z in e['zombies']) == 4, (label,e)
        assert all(z['dead'] for z in dead['encounter']['zombies']), label
        assert len(dead['ragdolls']) == 20, (label,dead['solver_resources'])
        for name in (label+'-clear',label+'-end'):
            state=states[name]
            assert not state['encounter']['started'] and not state['encounter']['zombies'], name
            assert not state['ragdolls'] and state['solver_resources'] == baseline['solver_resources'], (name,state['solver_resources'])
            assert state['characters'] == baseline['characters'] and state['dynamic_objects'] == baseline['dynamic_objects'], name
            for key in ('entities','components','bindings','live_spawns','ai_agents','dynamic_keys'):
                assert state['encounter'][key] == baseline['encounter'][key], (name,key)
            assert not state['encounter']['failure'], name
        assert cleared['map_ownership']['benchmark_records']['retained_bytes'] == 0
        samples.append(dict(round=number, rss_bytes=cleared['rss_bytes'],
            solver=cleared['solver_resources'], telemetry_bytes=cleared.get('map_ownership',{}).get('benchmark_records',{}).get('retained_bytes')))
    respawns = [name for name in states if name == 'respawn' or name.startswith('respawn-')]
    for name in ('toggle-stress','final-clear',*respawns):
        state=states[name]
        assert not state['ragdolls'] and state['solver_resources']==baseline['solver_resources'], name
        assert state['encounter']['robot'] is None and not state['encounter']['zombies'], name
        assert not state['encounter']['started'] and not state['encounter']['failure'], name
        assert state['characters'] == baseline['characters'] and state['dynamic_objects'] == baseline['dynamic_objects'], name
        for key in ('entities','components','bindings','live_spawns','ai_agents','dynamic_keys'):
            assert state['encounter'][key] == baseline['encounter'][key], (name,key)
        assert state['player_health']['current_hp'] > 0, name
    for name in states:
        if name.startswith('after-respawn-start'):
            assert len(states[name]['encounter']['zombies']) == 2, name
    # Report evidence, not an arbitrary universal "leak-free" RSS threshold.
    reset_samples = [dict(reset=name, rss_bytes=states[name]['rss_bytes']) for name in respawns]
    settled = samples[3:] or samples or reset_samples[3:] or reset_samples
    corpse_states = [states[f'round-{n:02d}-dead'] for n in range(rounds)]
    stability = {key:max((r[key] for state in corpse_states for r in state['ragdolls']), default=0)
        for key in ('max_anchor_error_m','max_penetration_m','max_speed_mps')}
    requests = json.loads((trace.parent/'actions.json').read_text())
    toggles = sum(row['action'].get('id') == 'companion_button' for row in requests)
    return dict(rounds=rounds, full_round_zombies=20*rounds, companion_toggle_requests=toggles, respawns=len(respawns),
        baseline_rss_bytes=baseline['rss_bytes'], clear_samples=samples, respawn_samples=reset_samples,
        post_warmup_rss_min=min(s['rss_bytes'] for s in settled),
        post_warmup_rss_max=max(s['rss_bytes'] for s in settled),
        post_warmup_first_to_last_bytes=settled[-1]['rss_bytes']-settled[0]['rss_bytes'],
        corpse_checkpoint_maxima=stability, ownership_checks='passed', limitations='Short native campaign, not proof against every leak; forced lethal health actions exercise ragdoll cleanup, not player aim.')

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('package',type=Path)
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--rounds',type=int,default=10,choices=(0,*range(4,11)),
        help='Full cohorts before the reset campaign; zero runs only toggle/respawn checks.')
    parser.add_argument('--respawns',type=int,default=4,choices=range(4,17))
    args=parser.parse_args()
    out=args.output.resolve();out.mkdir(parents=True,exist_ok=False)
    env={k:v for k,v in os.environ.items() if not k.startswith('PLACES_')}
    env.update(PLACES_ASSET_ROOT=str(ROOT/'assets'),PLACES_STATE_ROOT=str(out),PLACES_CONFIG_ROOT=str(out),PLACES_CACHE_ROOT=str(out/'cache'))
    (out/'settings.json').write_text(json.dumps(dict(player_colour='#4F7774', walk_speed=3.75,
        master_volume=0,vsync=False,quality='high',bloom=False,reflections='off',lightmaps='static',
        window_mode='windowed',window_width=960,window_height=540,developer_console=True)))
    script=out/'actions.json'
    encoded=json.dumps(actions(args.rounds, args.respawns), separators=(",", ":"))
    assert len(encoded.encode()) <= 65536, 'Native script exceeds 64 KiB'
    script.write_text(encoded)
    with (out/'import.log').open('w') as log:
        subprocess.run([str(ROOT/'target/release/places'),'--import',str(args.package.resolve())],env=env,stdout=log,stderr=subprocess.STDOUT,check=True)
    env.update(PLACES_LEVEL='encounter_lab',PLACES_BENCH='1',PLACES_BENCH_FRAMES='0',
        PLACES_BENCH_WARMUP='1000000',PLACES_BENCH_RESOURCE_TRACE='1',PLACES_VSYNC='off',
        PLACES_BENCH_ACTIONS=str(script),PLACES_LOAD_TRACE=str(out/'trace.jsonl'),PLACES_VERBOSE='1')
    with (out/'native.log').open('w') as log:
        subprocess.run([str(ROOT/'target/release/places')],env=env,stdout=log,stderr=subprocess.STDOUT,check=True,timeout=600)
    report=analyze(out/'trace.jsonl',args.rounds)
    (out/'report.json').write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps(report,indent=2))

if __name__ == '__main__': main()
