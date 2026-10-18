#!/usr/bin/env python3
"""Move the editor-imported Office basement flight into its own west shaft.

This is an explicit, offline design migration, not an import repair. Supply the
saved editor import as --source; the standalone historical source is never
overwritten. Run the actual editor safety review again after this migration.
"""
from __future__ import annotations

import argparse
import copy
import hashlib
import json
from pathlib import Path

from railings import RailingAuthor, floor_height

ROOT = Path(__file__).resolve().parents[2]
CLEAN = 'core:wallpaper_yellow_01'
DIRTY = 'core:wallpaper_stained_01'
CARPET = 'core:carpet_beige_01'
DAMP = 'core:carpet_damp_01'
TILE = 'core:ceiling_panel_01'
STAINED = 'core:ceiling_stained_01'
TRIM = 'core:baseboard_office_01'


def migrate(source: dict) -> tuple[dict, dict]:
    level = copy.deepcopy(source)
    if level['id'] not in ('office', 'office_demo'):
        raise ValueError('Expected the actual editor-imported Office source')
    if len(level['ramps']) != 2 or level['ramps'][0]['x'] != 25:
        raise ValueError('The two-flight Office baseline changed; review before migration')
    catalog = {entry['id']: entry for entry in
               json.loads((ROOT / 'assets/catalog.json').read_text())['assets']}
    gameplay = json.loads((ROOT / 'assets/gameplay.json').read_text())['assets']
    level['id'] = 'office'
    level['name'] = 'Office: After Hours'

    # The old shared south landing allowed top -> basement without entering the
    # middle rooms. Keep the upper flight, with all of its surroundings now at
    # the middle floor, and remove the old basement opening from that shaft.
    east = level['rooms'][4]
    east['floor_y'], east['height'] = 3.6, 6.4
    level['ramps'][1]['offset_y'] = 3.6
    level['floor_regions'] = []
    level['walls'][3]['openings'] = [opening for opening in level['walls'][3]['openings']
                                   if opening.get('sill', 0) != 0]

    # Extend west without placing a slope beneath the upper vestibule. A ramp
    # must overlap only one carrier floor plane, even on stacked maps.
    level['rooms'].extend([
        dict(x=-8, z=0, width=4, depth=18,
             floor_y=0, height=6.4, material=DAMP, ceiling_material=STAINED),
        dict(x=-8, z=0, width=4, depth=4,
             floor_y=3.6, height=2.8, material=CARPET, ceiling=dict(kind='open')),
        dict(x=-4, z=0, width=4, depth=4,
             floor_y=3.6, height=2.8, material=CARPET, ceiling_material=STAINED),
        dict(x=-4, z=14, width=4, depth=4,
             floor_y=0, height=2.8, material=DAMP, ceiling_material=STAINED),
    ])
    level['ramps'][0].update(x=-7.5, z=4, width=3, depth=10,
                             offset_y=3.6, rise=-3.6)
    west = level['walls'][2]
    west['openings'].extend([
        dict(kind='passage', offset=.88, width=2.2, height=2.35, sill=3.6),
        dict(kind='passage', offset=14.88, width=2.2, height=2.35, sill=0),
    ])

    def wall(x, z, width, depth, y, height, material, openings=()):
        piece = dict(x=x, z=z, width=width, depth=depth, y=y,
                     height=height, material=material)
        if openings:
            piece['openings'] = list(openings)
        level['walls'].append(piece)

    # One shell surrounds the west shaft; its two connector corridors are
    # capped independently at their real storey heights. No invisible blockers.
    wall(-8.12, -.12, 8, .24, 0, 6.4, DIRTY)
    wall(-8.12, 17.88, 8, .24, 0, 6.4, DIRTY)
    wall(-8.12, .12, .24, 17.76, 0, 6.4, DIRTY)
    wall(-4.12, .12, .24, 17.76, 0, 6.4, DIRTY, [
        dict(kind='passage', offset=.88, width=2.2, height=2.35, sill=3.6),
        dict(kind='passage', offset=14.88, width=2.2, height=2.35, sill=0),
    ])
    wall(-3.88, 3.88, 3.76, .24, 3.6, 2.8, DIRTY)
    wall(-3.88, 13.88, 3.76, .24, 0, 2.8, DIRTY)

    # Trim at the former eastern basement opening must close too, while the
    # two new western apertures must not retain a strip across their thresholds.
    trimmed = []
    for board in level.get('baseboards', []):
        if (board.get('rotation_degrees') in (90, 270)
                and board.get('x') in (-.12, .12)
                and board.get('y') in (0, 3.6)):
            direction = -1 if board['rotation_degrees'] == 90 else 1
            start, end = sorted((board['z'], board['z']+direction*board['length']))
            opening = (15, 17.2) if board['y'] == 0 else (1, 3.2)
            spans = [(start, end)]
            a, b = opening
            spans = [(lo, min(hi, a)) for lo, hi in spans if lo < a] + [
                (max(lo, b), hi) for lo, hi in spans if hi > b]
            for lo, hi in spans:
                if hi-lo > .025:
                    edited = dict(board, z=round(lo if direction > 0 else hi, 4),
                                  length=round(hi-lo, 4))
                    trimmed.append(edited)
        else:
            trimmed.append(board)
    level['baseboards'] = [board for board in trimmed if not (
        board.get('x') == 23.88 and board.get('y') == 0
        and board.get('rotation_degrees') == 270)]
    # Merge the former doorway's adjoining boards, then stop at both real
    # perpendicular basement partitions. Separate touching endcaps z-fight.
    for start, end in ((.12, 2.88), (3.12, 13.88), (14.12, 17.88)):
        level['baseboards'].append(dict(x=23.88, z=start, length=round(end-start, 4),
            rotation_degrees=270, y=0, height=.09, thickness=.018, material=TRIM))

    # Regenerate only shaft rails from their actual carrier/slope support. The
    # east deck change otherwise moves old floor-relative rail offsets upward.
    level['props'] = [prop for prop in level['props']
                      if not prop.get('id', '').startswith('shaft_')]
    railings = RailingAuthor(level, catalog)
    paint = {'0': TRIM, '1': TRIM}
    for name, points in (
        ('basement_west', [(-7.8, 3.6, 3.932), (-7.432, 3.6, 3.932),
                           (-7.432, .02448, 13.932)]),
        ('basement_east', [(-4.568, .02448, 13.932), (-4.568, 3.6, 3.932),
                           (-4.2, 3.6, 3.932)]),
        ('upper_west', [(24.32, 7.2, 3.932), (31.068, 7.2, 3.932),
                        (31.068, 3.6, 14.068), (30.3, 3.6, 14.068)]),
        ('upper_east', [(35.7, 7.2, 3.932), (33.932, 7.2, 3.932),
                        (33.932, 3.6, 14.068), (35.7, 3.6, 14.068)]),
    ):
        railings.run('shaft_'+name, points, spacing=1.5, materials=paint,
                     post_materials={'0': TRIM})

    # All existing movable desk items get real reusable physical definitions;
    # the engine preserves their exact authored starting poses until touched.
    for prop in level['props']:
        if prop.get('id') in ('top_crt_0', 'top_crt_1', 'top_mug', 'top_books'):
            prop['physics'] = copy.deepcopy(gameplay[prop['model']]['physics'])
            prop['physics']['reset_group'] = 'office_desk_items'
            # Pickup owns E now. Movable props keep optional audio emitters,
            # while their old fixed-placement interaction/bindings must retire.
            if 'components' in prop:
                prop['components'] = [component for component in prop['components']
                                      if component.get('component') == 'audio']
            prop.pop('bindings', None)

    # A compact ordinary switch restores the office's few loose desk items.
    level['props'].append(dict(
        id='office_items_reset', display_name='Desk item reset',
        model='home:wall_switch', x=-1, y=1.35, z=7.145, rotation_degrees=0,
        size=catalog['home:wall_switch']['size'], solid=False,
        components=[dict(component='interactable', prompt='Reset desk items', reach=2),
                    dict(component='animation', clip='toggle', looped=False, playing=False)],
        bindings=[dict(on='interact', cooldown_seconds=.3, actions=[
            dict(action='toggle_animation'),
            dict(action='reset_physics_group', group='office_desk_items')])]))

    # Physical return leaf stays closed: the collection controller owns the
    # map transition. Its hinge is at the upper vestibule's southwest jamb.
    level['walls'][6]['openings'] = [dict(kind='door', offset=1.18, width=1.4,
                                         height=2.35, sill=0)]
    level.setdefault('doors', []).append(dict(
        id='office_return_hub', x=-4, y=0, z=9.7, rotation_degrees=90,
        width=1.4, height=2.35, thickness=.055, initial_state='closed',
        swing_degrees=90, kind='interior', frame_material=TRIM,
        components=[dict(component='interactable', prompt='Return to hub', reach=2.4)],
        bindings=[]))

    # Fixed light pools expose the complete moved descent and both landings.
    # The old eastern lower wall light was beneath the newly raised deck.
    for light in level['ceiling_lights']:
        if light.get('id') == 'shaft_lower':
            light['y'] = 5.7
    for identity, x, z, y, power, reach in (
        ('basement_shaft_roof', -6.5, 8.5, 6.39, .28, 5.5),
        ('middle_connector_light', -1.5, 2.5, 6.39, .32, 4.2),
        ('basement_connector_light', -1.5, 15.5, 2.79, .24, 4.2),
    ):
        level['ceiling_lights'].append(dict(
            id=identity, fixture='core:fluorescent_panel_01', x=x, z=z, y=y,
            brightness=power, emission=.7, range=reach, color=[1, .92, .76]))
    for identity, z in (('basement_shaft_upper', 2), ('basement_shaft_lower', 16)):
        level['ceiling_lights'].append(dict(
            id=identity, fixture='core:pool_light_wall', x=-7.86, z=z,
            y=5.7 if z == 2 else 2.1, mount='wall', rotation_degrees=90,
            brightness=.3, emission=.55, range=5.5, color=[1, .92, .76]))

    level.setdefault('geometry_intent', []).append(dict(
        check='missing-wall', x=-8, z=3.5, width=4, depth=1,
        note='The middle landing opens onto the descending basement ramp; linked physical rails protect its two drop edges.'))

    # Quantify actual nine-point stock support for every linked post.
    contacts = []
    for prop in level['props']:
        if prop.get('model') != 'outdoor:porch_post' or not prop['id'].startswith('shaft_'):
            continue
        half = .066
        ground = [floor_height(level, prop['x']+dx, prop['z']+dz,
                               floor_height(level, prop['x'], prop['z'])+prop['y'])
                  for dx in (-half, 0, half) for dz in (-half, 0, half)]
        base = floor_height(level, prop['x'], prop['z'])+prop['y']
        contacts.append(dict(id=prop['id'], base_y=round(base, 6),
                             minimum_penetration_m=round(min(ground)-base, 6),
                             maximum_penetration_m=round(max(ground)-base, 6)))
    evidence = dict(
        level_id='office',
        entry_spawn=level['spawn'], expected_entry_feet_y=7.2,
        return_door=dict(id='office_return_hub', hinge=[-4, 7.2, 9.7],
                         center=[-4, 8.375, 9], closed_yaw=90),
        geometry_changes=[
            'Lower flight moved from east X25..28 to west X-7.5..-4.5; Z4..14, Y3.6 down to0.',
            'Vacated east shaft floor raised to Y3.6 and its basement opening removed.',
            'West middle opening Z1..3.2 at Y3.6; basement opening Z15..17.2 at Y0.',
        ],
        railing_post_contacts=contacts,
        ai_preserved=['basement_mannequin']+[f'basement_roach_{i}' for i in range(4)],
        pending=['Native normal-input routes and jump/side probes',
                 'Final editor safety and save/reopen roundtrip', 'Final package validation'],
    )
    return level, evidence


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', required=True, type=Path)
    parser.add_argument('--out', required=True, type=Path)
    parser.add_argument('--report', type=Path)
    args = parser.parse_args()
    if args.source.resolve() == (ROOT / 'levels/office_demo.json').resolve():
        parser.error('Import through the editor first; supply the saved project level')
    raw = args.source.read_bytes()
    level, evidence = migrate(json.loads(raw))
    evidence['imported_source'] = str(args.source)
    evidence['imported_source_sha256'] = hashlib.sha256(raw).hexdigest()
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(level, indent=2, ensure_ascii=False)+'\n')
    evidence['migrated_source_sha256'] = hashlib.sha256(args.out.read_bytes()).hexdigest()
    if args.report:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(json.dumps(evidence, indent=2)+'\n')
    print(f'Wrote {args.out}; editor safety and native acceptance remain required')


if __name__ == '__main__':
    main()
