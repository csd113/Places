#!/usr/bin/env python3
"""Opt-in native regression routes for the combined Closing Time/Beam stage.

Uses the existing transparent controls drivers and actual framebuffer, mixer and
world receipts. This is explicit developer acceptance, never routine CI.
"""
import argparse
import json
from pathlib import Path
import beam_acceptance as beam
import beam_audit
import closing_time_acceptance as closing
import closing_time_audit


def beam_win_reset():
    steps = beam.natural()
    end = max(step['delay_ms'] for step in steps)
    steps = [step for step in steps if step['action']['kind'] != 'quit']
    for offset, kind, fields in [
        (100,'key',{'key':'Return','pressed':True}),
        (140,'key',{'key':'Return','pressed':False}),
        (500,'capture',{'name':'win-restart-opening'}),
        (650,'key',{'key':'Return','pressed':True}),
        (690,'key',{'key':'Return','pressed':False}),
        (1100,'capture',{'name':'win-reset-world'}),
        (1300,'quit',{}),
    ]:
        steps.append(beam.action(end+offset,kind,**fields))
    assert len(steps)<=128 and end+1300<60000
    return sorted(steps,key=lambda step:step['delay_ms'])


def circuit_quality():
    items = [(0,'focus',{'focused':True}), (100,'activate',{'id':'arcade_switch'}),
        (900,'capture',{'name':'full-off'}), (1200,'quality',{'level':'low'}),
        (3200,'capture',{'name':'low-off'}), (3500,'quality',{'level':'high'}),
        (5500,'capture',{'name':'high-off'}), (5800,'lightmaps',{'quality':'static'}),
        (7800,'capture',{'name':'static-refused'}), (8100,'lightmaps',{'quality':'full'}),
        (10300,'capture',{'name':'full-recovered'}), (10600,'activate',{'id':'exit_sign'}),
        (11700,'capture',{'name':'reset-restored'}), (12000,'quit',{})]
    return [closing.action(ms,kind,**fields) for ms,kind,fields in items]


def held_door():
    p = beam.Walk()
    p.enter(); p.press('F')
    p.move(12.8,13.1); p.move(12.8,5.6)
    p.aim(12.8,4.1,4.9); p.press(); p.ms += 850
    p.move(12.8,4.2); p.move(12.8,2.2); p.move(11,2.2)
    p.item('gift_green'); p.press(); p.carrying = True
    p.capture('green-held')
    p.move(10.8,2.2)
    # Open leaf is 90 degrees from its hinge, along the side of the passage.
    p.aim(11.92,4.0,3.6); p.ms += 500
    p.capture('held-door-aim'); p.press(); p.ms += 900
    p.capture('held-door-closed')
    p.move(12.8,2.7); p.aim(12.8,4.0,4.9); p.ms += 500; p.press(); p.ms += 950
    p.capture('held-door-reopened')
    p.move(12.8,6.2); p.capture('carried-through')
    p.aim(12.8,2.8,7.2); p.ms += 500; p.capture('deliberate-drop-aim')
    p.press(); p.carrying = False; p.ms += 600; p.capture('deliberate-drop')
    return p.finish()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--closing', type=Path, required=True)
    parser.add_argument('--beam', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--case', default='all', choices=['all','closing-natural','closing-edges','beam-natural','beam-death','beam-edges','beam-beam','held-door','closing-quality'])
    args = parser.parse_args()
    base = args.out.resolve(); base.mkdir(parents=True, exist_ok=True)
    beam.BASE = base; closing.BASE = base
    cases = [('closing-natural',closing,closing.natural,'natural'),('closing-edges',closing,closing.edge_cases,'edges'),('beam-natural',beam,beam_win_reset,'natural'),('beam-death',beam,beam.death,'death'),('beam-edges',beam,beam.edges,'edges'),('beam-beam',beam,beam.beam,'beam'),('held-door',beam,held_door,None),('closing-quality',closing,circuit_quality,None)]
    results = []
    for name, driver, recipe, audit_case in cases:
        if args.case not in ('all', name): continue
        package = args.closing if driver is closing else args.beam
        out, samples = driver.run(name,package.resolve(),args.binary.resolve(),recipe())
        focused = all(sample['window_focused'] and sample['window_native_focus'] for sample in samples.values())
        if audit_case:
            if driver is closing: closing_time_audit.audit(out,audit_case)
            elif not beam_audit.audit(out,audit_case): raise SystemExit(name+' outcome failed')
        elif name == 'closing-quality':
            checks = {'actual_full_retained':all(sample['renderer_lightmaps']=='full' for sample in samples.values()), 'quality_changes_apply': samples['low-off']['renderer_quality']=='low' and samples['high-off']['renderer_quality']=='high', 'circuits_survive_graphics':all(not dict(samples[n]['light_states'])[0] and not dict(samples[n]['light_states'])[1] for n in ['full-off','low-off','high-off','static-refused','full-recovered']), 'reset_restores_circuits':all(dict(samples['reset-restored']['light_states']).values()), 'named_static_refusal':any('requires Full lighting' in line for line in (out/'trace.jsonl').read_text().splitlines())}
            (out/'acceptance.json').write_text(json.dumps({'passed':all(checks.values()),'checks':checks},indent=2)+'\n')
            if not all(checks.values()):raise SystemExit('circuit-quality failed: '+str(checks))
        else:
            def held(n): return samples[n]['physics']['held']
            checks = {
                'held_door_targets_world': samples['held-door-aim']['interaction_target']['kind'] == 'world',
                'closed_with_item_held': samples['held-door-closed']['doors'][1]['angle_degrees'] < 1 and held('held-door-closed') == held('green-held'),
                'reopened_with_item_held': samples['held-door-reopened']['doors'][1]['angle_degrees'] > 75 and held('held-door-reopened') == held('green-held'),
                'crossed_while_carrying': held('carried-through') == held('green-held') and samples['carried-through']['player_position'][2] > 5,
                'look_away_offers_drop': samples['deliberate-drop-aim']['interaction_target']['kind'] == 'drop',
                'deliberate_drop_releases': held('deliberate-drop') is None,
            }
            (out/'acceptance.json').write_text(json.dumps({'passed':all(checks.values()),'checks':checks},indent=2)+'\n')
            if not all(checks.values()): raise SystemExit('held-door failed: '+str(checks))
        if name == 'beam-natural':
            reset = samples['win-reset-world']
            assert reset['outcome'] is None and reset['guidance'] is None
            assert reset['player_body']['reset_count'] == 1 and reset['physics']['held'] is None
            assert not reset['flashlight_on'] and reset['doors'][0]['locked']
            (out/'win-reset-acceptance.json').write_text(json.dumps({'passed':True,'checks':['ordinary Enter after win creates fresh run','opening dismisses','hands, flashlight, doors and outcome reset']},indent=2)+'\n')
        results.append({'case':name,'passed':True,'actual_window_focus':focused})
        if not focused: raise SystemExit(name+': actual window focus missing')
    (base/('campaign-'+args.case+'.json')).write_text(json.dumps(results,indent=2)+'\n')

if __name__ == '__main__': main()
