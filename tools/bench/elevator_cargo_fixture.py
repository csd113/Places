#!/usr/bin/env python3
"""Write tiny passenger fixtures for explicit offline export; never bake them.

The ordinary workspace tests do not invoke this tool. Native acceptance checks
real boarding, ownership, held state, rotated arrivals, save/resume and rollback.
"""
import argparse
import copy
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=False)
    source = ROOT / 'projects/elevator-links'
    manifest = json.loads((source / 'places.project.json').read_text())
    levels = {name: json.loads((source / f'levels/{name}.json').read_text())
              for name in ('elevator_lobby', 'elevator_annex')}
    levels['elevator_lobby']['props'] = [
        dict(id='travel_crate', model='core:cardboard_box', x=-.55, y=.02,
             z=2.05, scale=.35, physics=dict(motion='dynamic', pickup=True)),
        dict(id='travel_guest', model='mannequin', x=.5, y=0, z=-1.5,
             scale=1, components=[
                 dict(component='nav_agent', radius=.4, height=1.8, speed_mps=1.25),
                 dict(component='ai', behavior='idler', sight_range=0,
                      hearing_range=0, can_open_doors=False, idle_clip='pose_stand'),
                 dict(component='state', name='cargo_health', value=61)]),
        dict(id='outside_crate', model='core:cardboard_box', x=1.8, y=.02,
             z=-2, scale=.35, physics=dict(motion='dynamic', pickup=True))]
    for case in ('straight', 'rotated', 'blocked-local', 'blocked-level',
                 'blocked-rider-local', 'blocked-rider-level',
                 'blocked-npc-local', 'blocked-npc-level', 'physical', 'blocked-vertical'):
        project = args.out / case
        (project / 'levels').mkdir(parents=True)
        document = copy.deepcopy(manifest)
        document['id'] = f'elevator_cargo_{case.replace("-", "_")}'
        (project / 'places.project.json').write_text(json.dumps(document, indent=2)+'\n')
        authored = copy.deepcopy(levels)
        if case in ('physical', 'blocked-vertical'):
            lab = ROOT / 'projects/elevator-lab'
            authored = {name: json.loads((lab / f'levels/{name}.json').read_text())
                        for name in levels}
            authored['elevator_lobby']['props'] = copy.deepcopy(levels['elevator_lobby']['props'])
        if case == 'rotated':
            annex = authored['elevator_annex']
            annex['elevators'][0]['rotation_degrees'] = 90
            for stop in annex['elevators'][0]['stops']:
                stop['y'] = 3
            # The destination cabin is self-contained. A generous receiving
            # hall makes this an arrival-frame check, not a wall-placement test.
            annex['rooms'] = [dict(x=-3, z=-3, width=8, depth=6,
                                   floor_y=3, height=3, material='core:carpet_beige_01')]
            annex['walls'] = []
            annex['ceiling_lights'] = []
            annex['spawn'] = dict(x=3, z=0, yaw_degrees=270)
        if case.startswith('blocked'):
            local = case.endswith('local') or case == 'blocked-vertical'
            level = authored['elevator_lobby' if local else 'elevator_annex']
            shift = 10 if local and case != 'blocked-vertical' else 0
            if 'npc' in case:
                guest = copy.deepcopy(levels['elevator_lobby']['props'][1])
                guest.update(id='resident_guest', x=shift-.55, z=2.05)
                level.setdefault('props', []).append(guest)
            else:
                level.setdefault('props', []).append(dict(
                    id='arrival_blocker', model='core:cardboard_box',
                    x=shift-.45 if 'rider' in case else shift+.5,
                    y=3.52 if case == 'blocked-vertical' else .02,
                    z=1.05 if 'rider' in case else 1.65,
                    scale=.8, physics=dict(motion='dynamic', pickup=True)))
        for name, level in authored.items():
            (project / f'levels/{name}.json').write_text(json.dumps(level, indent=2)+'\n')
        print(project)


if __name__ == '__main__':
    main()
