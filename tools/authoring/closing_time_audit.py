#!/usr/bin/env python3
"""Audit explicit native evidence; no map baking or automatic CI fixture."""
import argparse
import json
from pathlib import Path


def audit(folder, case):
    samples = json.loads((folder / 'snapshots.json').read_text())
    receipt = json.loads((folder / 'receipt.json').read_text())
    checks = []

    def require(condition, name):
        checks.append({'check': name, 'passed': bool(condition)})

    def locked(sample):
        return sample['doors'][0]['locked']

    def loops(sample):
        return sum(v['looping'] for v in sample['audio_voices'])

    def complete(sample):
        return any('CLOSING TIME COMPLETE' in s for s in sample['script_diagnostics'])

    def reset(sample, original):
        return (not locked(sample) and sample['physics']['held'] is None
                and sample['light_states'] == original['light_states']
                and loops(sample) == 4
                and all(a['pose'] == b['pose'] for a, b in zip(
                    sample['physics']['bodies'], original['physics']['bodies'], strict=True)))

    require(receipt['exit_code'] == 0 and receipt['shutdown'], 'clean native shutdown')
    for name, sample in samples.items():
        stats = sample['physics']['stats']
        require(stats['recovered_bodies'] == stats['suspended_bodies'] == 0,
                f'{name}: no recovered or suspended bodies')
        require(sample['audio']['errors'] == sample['audio']['dropped'] == 0,
                f'{name}: no mixer errors or dropped voices')
        require(not any('error' in d.lower() for d in sample['script_diagnostics']),
                f'{name}: no script errors')
    if case == 'natural':
        sample = samples['natural-win']
        actions = json.loads((folder / 'actions.json').read_text())
        require(not any(a['action']['kind'] in ('player', 'item_pose', 'activate',
                    'item_velocity') for a in actions), 'natural route uses ordinary gameplay inputs')
        require(locked(sample) and complete(sample), 'natural route reaches locked completion')
        require(loops(sample) == 0, 'all four ambience loops stopped at completion')
        require(sample['physics']['held'] is None, 'no held return at completion')
        require(sample['physics']['stats']['sleeping_bodies'] == 3, 'all three returns settled')
        require(dict(sample['light_states'])[5] and dict(sample['light_states'])[6]
                and sum(v for _, v in sample['light_states']) == 2,
                'three circuits off; permanent lobby and aisle lamps remain')
    elif case == 'edges':
        original = samples['authored']
        require(not locked(samples['early-lock-refused']), 'early entrance cannot win')
        require(samples['repeat-bowling']['script_diagnostics'].count('bowling power off') == 1,
                'repeated circuit activation is idempotent')
        require(all(f'{item} returned' in samples['all-ready']['script_diagnostics']
                    for item in ('ball', 'gift', 'plush')), 'returns and switches accept alternate order')
        removed = samples['removed-item-refuses-win']
        require(not locked(removed) and 'gift outstanding' in removed['script_diagnostics'],
                'removed return invalidates completion')
        held = samples['held-return-invalid']
        require(held['physics']['held'] is not None and 'plush outstanding' in held['script_diagnostics'],
                'held return invalidates completion')
        require(reset(samples['reset-from-held'], original), 'restart releases held item and restores shift')
        require(locked(samples['win']) and complete(samples['win']), 'ordinary entrance interaction wins')
        require(reset(samples['restart-after-win'], original), 'ordinary second interaction restarts completed shift')
        require(loops(original) == 4 and loops(samples['repeat-bowling']) == 3
                and loops(samples['all-ready']) == 1 and loops(samples['win']) == 0,
                'spatial ambience follows 4, 3, 1, 0 loop shutdown states')
        cues = {v['cue'] for s in samples.values() for v in s['audio_voices']}
        require('switch_click' in cues and 'door_close' in cues, 'actual mixer uses installed switch and lock substitute cues')
    elif case == 'collisions':
        for name in ('north', 'entrance', 'south', 'west', 'east', 'southwest'):
            x, _, z = samples[name]['player_position']
            require(.47 <= x <= 17.53 and .47 <= z <= 21.6,
                    f'{name}: moving/jumping/crouching cannot escape shell')
        require(samples['cabinet']['player_position'][2] > 2.65, 'arcade cabinet blocks walking')
        require(samples['party-table']['player_position'][2] > 15.0, 'party table blocks walking')
        ball = samples['ball-wall-settled']['physics']['bodies'][2]['pose']['position']
        require(ball[0] < 17.8 and ball[1] > -.1, 'fast loose ball remains inside venue')
    result = {'case': case, 'source_receipt': receipt, 'checks': checks,
              'passed': all(c['passed'] for c in checks)}
    (folder / 'acceptance.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps({'case': case, 'passed': result['passed'],
                      'failed': [c for c in checks if not c['passed']]}))
    if not result['passed']:
        raise SystemExit(1)


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('folder', type=Path)
    parser.add_argument('case', choices=['smoke', 'natural', 'edges', 'collisions'])
    args = parser.parse_args()
    audit(args.folder, args.case)
