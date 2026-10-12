#!/usr/bin/env python3
"""Build the connected, three-storey Office using the existing modular kit.

The basement is first in resolution order: its compiled navigation is the
only AI surface. Player support resolves all three storeys by body height.
The side shaft carries two separate ramps, so no floor/ceiling holes or
multi-storey navigation representation are needed.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUTPUT = ROOT / 'levels/office_demo.json'
CLEAN = 'core:wallpaper_yellow_01'
DIRTY = 'core:wallpaper_stained_01'
CARPET = 'core:carpet_beige_01'
DAMP = 'core:carpet_damp_01'
TILE = 'core:ceiling_panel_01'
STAINED_TILE = 'core:ceiling_stained_01'
TRIM = 'core:baseboard_office_01'


def build_level():
    level = dict(format_version=3, id='office_demo', name='Office: After Hours',
                 author='Places', spawn=dict(x=-2, z=9, yaw_degrees=90),
                 defaults=dict(wall=CLEAN, floor=CARPET, ceiling=TILE),
                 environment=dict(presentation=dict(exposure=1, tone_knee=.8,
                     saturation=.98, contrast=1.02), fog=dict(color=[.22, .23, .20],
                     density=.003, reference_y=0, height_gain=0)))
    for key in ('rooms', 'walls', 'props', 'ceiling_lights', 'ramps', 'floor_regions',
                'guardrails', 'baseboards', 'columns', 'timers', 'sequences', 'geometry_intent'):
        level[key] = []
    # Three real overlapping floors; a unique top vestibule seeds the spawn.
    for y, floor, ceiling in ((0, DAMP, STAINED_TILE), (3.6, CARPET, STAINED_TILE),
                               (7.2, CARPET, TILE)):
        level['rooms'].append(dict(x=0, z=0, width=24, depth=18, floor_y=y,
                                  height=2.8, material=floor, ceiling_material=ceiling))
    level['rooms'].extend([
        dict(x=-4, z=7, width=4, depth=4, floor_y=7.2, height=2.8,
             material=CARPET, ceiling_material=TILE),
        dict(x=24, z=0, width=12, depth=18, floor_y=0, height=10,
             material=DAMP, ceiling_material=TILE),
        # Upper landing is an actual floor above the lower landing. The shaft
        # already owns the roof; open avoids a duplicate ceiling at Y=10.
        dict(x=24, z=0, width=12, depth=4, floor_y=7.2, height=2.8,
             ceiling=dict(kind='open'), material=CARPET),
    ])
    catalog = json.loads((ROOT / 'assets/catalog.json').read_text())
    sizes = {a['id']: a['size'] for a in catalog['assets'] if 'size' in a}

    def opening(offset, width=1.4, height=2.35, sill=0):
        return dict(kind='passage', offset=offset, width=width, height=height, sill=sill)

    def board(x, z, length, yaw, y):
        if length > .02:
            level['baseboards'].append(dict(x=round(x, 4), z=round(z, 4),
                length=round(length, 4), rotation_degrees=yaw, y=y,
                height=.09, thickness=.018, material=TRIM))

    def wall(x, z, width, depth, y, material=CLEAN, openings=(), height=2.8,
             trim=True, emit=True):
        piece = dict(x=x, z=z, width=width, depth=depth, y=y,
                     height=height, material=material)
        if openings:
            piece['openings'] = list(openings)
        if emit:
            level['walls'].append(piece)
        # Pin the trim to its actual storey. First-room point ownership would
        # otherwise place top/middle automatic trim on the basement floor.
        if trim:
            axis_x = width >= depth
            length = width if axis_x else depth
            cursor = 0
            for start, end in [(o['offset'], o['offset']+o['width']) for o in openings
                               if o.get('sill', 0) < .09] + [(length, length)]:
                run = start-cursor
                if axis_x:
                    board(x+start, z, run, 180, y)
                    board(x+cursor, z+depth, run, 0, y)
                else:
                    board(x, z+cursor, run, 270, y)
                    board(x+width, z+start, run, 90, y)
                cursor = end

    # One continuous building shell avoids coplanar bottom caps where separate
    # spandrels would meet lower-storey ceiling planes. The maintained internal
    # upper partitions contrast with this older outer wallpaper.
    wall(-.12, -.12, 24.24, .24, 0, DIRTY, height=10, trim=False)
    wall(-.12, 17.88, 24.24, .24, 0, DIRTY, height=10, trim=False)
    wall(-.12, .12, .24, 17.76, 0, DIRTY, [opening(7.88, 2, sill=7.2)],
         height=10, trim=False)
    wall(23.88, .12, .24, 17.76, 0, DIRTY,
         [opening(.88, 2.2), opening(14.88, 2.2, sill=3.6),
          opening(.88, 2.2, sill=7.2)], height=10, trim=False)
    # Generate floor-pinned trim without duplicating any physical shell wall.
    for y, material, east_door in ((0, DIRTY, 1), (3.6, DIRTY, 15), (7.2, CLEAN, 1)):
        wall(-.12, -.12, 24.24, .24, y, material, emit=False)
        wall(-.12, 17.88, 24.24, .24, y, material, emit=False)
        wall(-.12, .12, .24, 17.76, y, material,
             [opening(7.88, 2)] if y == 7.2 else [], emit=False)
        wall(23.88, .12, .24, 17.76, y, material,
             [opening(east_door-.12, 2.2)], emit=False)
    # Enclosed top vestibule; the east opening is owned by the office shell.
    wall(-4.12, 6.88, 4, .24, 7.2)
    wall(-4.12, 10.88, 4, .24, 7.2)
    wall(-4.12, 7.12, .24, 3.76, 7.2)

    # Upper floor: clear east/west corridor, workrooms, two offices, records
    # and a break room. Desk positions leave generous walking lanes.
    wall(.12, 7.88, 23.76, .24, 7.2, openings=[opening(2, 1.4),
         opening(10, 1.6), opening(19.5, 1.5)])
    wall(.12, 9.88, 23.76, .24, 7.2, openings=[opening(3, 1.5), opening(14, 1.8)])
    wall(7.88, .12, .24, 7.76, 7.2)
    wall(17.88, .12, .24, 7.76, 7.2)
    wall(7.88, 10.12, .24, 7.76, 7.2)
    # Middle: offset repeated passages and a return loop; all clearances are
    # deliberate. Slightly buried furniture is confined to wall-side pockets.
    wall(4.88, .12, .24, 12.76, 3.6, openings=[opening(3, 1.6), opening(9, 1.6)])
    wall(10.88, 4.12, .24, 13.76, 3.6, DIRTY, [opening(2, 1.6), opening(9.5, 1.6)])
    wall(16.88, .12, .24, 12.76, 3.6, openings=[opening(6, 1.8)])
    wall(5.12, 3.88, 11.76, .24, 3.6, DIRTY, [opening(2.8, 1.6)])
    wall(.12, 12.88, 10.76, .24, 3.6, DIRTY, [opening(1.8, 1.8)])
    wall(17.12, 9.88, 6.76, .24, 3.6, DIRTY, [opening(3.1, 1.6)])
    wall(11.12, 15.88, 6.76, .24, 3.6, DIRTY, [opening(1.7, 1.6)])
    # Basement: a reachable loop with staggered short cross halls, dark
    # termini and a few filing/cooler landmarks. No furniture in throat lanes.
    wall(5.88, .12, .24, 12.76, 0, DIRTY, [opening(1.9, 1.8), opening(8.9, 1.8)])
    wall(11.88, 3.12, .24, 14.76, 0, DIRTY, [opening(5.1, 1.8)])
    wall(17.88, .12, .24, 13.76, 0, DIRTY, [opening(5.9, 1.8)])
    wall(.12, 5.88, 5.76, .24, 0, DIRTY)
    wall(.12, 11.88, 11.76, .24, 0, DIRTY, [opening(1.9, 1.8), opening(7.9, 1.8)])
    wall(12.12, 2.88, 11.76, .24, 0, DIRTY, [opening(7.9, 1.8)])
    wall(12.12, 13.88, 11.76, .24, 0, DIRTY, [opening(1.9, 2)])

    # Enclosed shaft. Central spine divides the flights but leaves both
    # landings open. Upper bridge closes the space above the lower landing.
    wall(24.12, -.12, 12, .24, 0, CLEAN, height=10, trim=False)
    wall(24.12, 17.88, 12, .24, 0, CLEAN, height=10, trim=False)
    wall(35.88, .12, .24, 17.76, 0, CLEAN, height=10, trim=False)
    wall(29.88, 4, .24, 10.24, 0, DIRTY, height=10, trim=False)
    level['ramps'] = [
        dict(x=25, z=4, width=3, depth=10, offset_y=0, rise=3.6,
             material=DAMP, edge_material=DIRTY),
        dict(x=31, z=4, width=3, depth=10, offset_y=7.2, rise=-3.6,
             material=CARPET, edge_material=CLEAN),
    ]
    level['floor_regions'] = [dict(x=24, z=14, width=12, depth=4, offset_y=3.6,
        material=CARPET, edge_material=DIRTY)]

    def rail(x, z, length, yaw, y, rise=0):
        level['guardrails'].append(dict(x=x, z=z, length=length,
            rotation_degrees=yaw, y=y, rise=rise, height=1.05,
            post_spacing=1.25, material=TRIM))

    for x in (24.8, 28.2):
        rail(x, 4, 10, 270, 0, 3.6)
    for x in (30.8, 34.2):
        rail(x, 14, 10, 90, 3.6, 3.6)
    for x, length in ((24.32, .48), (28.2, 1.5), (30.3, .5), (34.2, 1.5)):
        rail(x, 14.18, length, 0, 3.6)
    for x, length in ((24.32, 6.48), (34.2, 1.5)):
        rail(x, 3.82, length, 0, 7.2)
    level['geometry_intent'].append(dict(check='missing-wall', x=24, z=3.5,
        width=12, depth=1, note='Upper bridge edge opens to the ramp shaft; authored guardrails protect the drops.'))

    def prop(model, x, z, identity, y=0, yaw=0, solid=True, **extra):
        piece = dict(id=identity, model=model, x=x, z=z, y=y,
            rotation_degrees=yaw, size=sizes[model], solid=solid, **extra)
        level['props'].append(piece)
        return piece

    # Office kit gives the plan its scale; only two existing CRTs and a few
    # mugs/books add equipment. The sparse reference remains the authority.
    for index, (x, z, yaw) in enumerate(((3.2, 3.6, 0), (11, 3, 0), (15, 3, 0),
            (11, 6.2, 180), (15, 6.2, 180), (11, 13.5, 0), (17, 13.5, 0))):
        prop('core:desk', x, z, f'top_desk_{index}', 7.2, yaw)
        prop('core:chair', x, z+(1 if yaw == 0 else -1), f'top_chair_{index}',
             7.2, (yaw+180) % 360)
    for index, (x, z, yaw) in enumerate(((1.1, 16.6, 0), (3.3, 16.6, 0),
                                        (6.3, 16.6, 0), (22.8, 12, 270))):
        prop('core:cabinet', x, z, f'top_records_{index}', 7.2, yaw)
    prop('core:water_cooler', 19.3, 5.9, 'top_water', 7.2)
    prop('core:vending_machine', 22.5, 5.8, 'top_vending', 7.2)
    for index, x in enumerate((3.2, 15)):
        prop('home:crt_tv', x, 3.5 if index == 0 else 2.9, f'top_crt_{index}',
             7.95, yaw=0, solid=False, scale=.55)
    prop('home:mug', 10.7, 13.5, 'top_mug', 7.95, solid=False)
    prop('home:book_stack', 17.4, 13.4, 'top_books', 7.95, solid=False)
    prop('core:desk', 2.1, 2.6, 'middle_rotated_desk', 3.6, 27)
    prop('core:chair', 2.5, 3.7, 'middle_rotated_chair', 3.6, 133)
    prop('core:chair', 7.1, 5.2, 'middle_sunken_chair', 3.34, 72,
         comment='Intentional 0.26 m floor burial, away from the through route.')
    prop('core:desk', 16.68, 3, 'middle_wall_desk', 3.6, 90,
         comment='Intentional desktop penetration into the partition; no coplanar faces.')
    prop('core:cabinet', 9.3, 14.8, 'middle_cabinet', 3.6, 31)
    prop('core:water_cooler', 19.2, 16.7, 'middle_landmark', 3.6, 180)
    prop('core:desk', 15.2, 11.8, 'basement_abandoned_desk', 0, 180)
    prop('core:chair', 15.7, 10.6, 'basement_abandoned_chair', 0, 205)
    prop('core:water_cooler', 22.4, 4.3, 'basement_water_landmark')
    # Three tangible hide cavities under raised filing plinths. Short support
    # feet and the front skirting lip conceal a 35mm-tall insect without using
    # an inaccessible point inside wall collision.
    anchors = [[2, 0, 1.2], [14.5, 0, 10.1], [21.5, 0, 16.7]]
    for index, (x, _, z) in enumerate(anchors):
        prop('core:cabinet', x, z, f'basement_hide_{index}', .09)
        for dx in (-.37, .37):
            for dz in (-.16, .16):
                level['columns'].append(dict(x=x+dx-.025, z=z+dz-.025,
                    width=.05, depth=.05, y=0, height=.09, material=TRIM))
        level['baseboards'].append(dict(x=x-.44, z=z+.24, length=.88, y=.045,
            height=.035, thickness=.02, material=TRIM, rotation_degrees=0,
            comment='Raised filing-plinth skirting; reachable dark insect refuge.'))
    bounds = dict(x=.45, z=.45, width=23.1, depth=17.1, min_y=-.05, max_y=.1)
    prop('mannequin', 8.5, 8.3, 'basement_mannequin', solid=False,
         # The moving clip reaches 0.68 m forward; a conservative 0.72 m
         # disc keeps the arms and running legs clear while rounding corners.
         components=[dict(component='nav_agent', radius=.72, height=1.72,
                          speed_mps=2.244582, step_height=.3, max_slope=1),
                     dict(component='ai', behavior='wanderer', walk_speed=2.244582,
                          run_speed=2.244582, sight_range=0, hearing_range=0,
                          wander_radius=30, idle_seconds=3.2, home_bounds=bounds,
                          idle_clip='pose_stand', move_clip='run_arms_forward')])
    for index, (x, z) in enumerate(((2.8, 2), (20.8, 15.8))):
        prop('entity:cockroach', x, z, f'basement_roach_{index}', solid=False,
             components=[dict(component='nav_agent', radius=.055, height=.035,
                              speed_mps=.16, step_height=.1, max_slope=1),
                         dict(component='ai', behavior='skittish', role='insect',
                              reacts_to=['player'], walk_speed=.16, run_speed=.62,
                              sight_range=2.1, sight_fov_degrees=360, hearing_range=0,
                              wander_radius=1.7, idle_seconds=5.8+index*1.7,
                              hide_anchors=anchors, hide_seconds=15+index*4,
                              home_bounds=bounds)])

    def light(identity, x, z, y, brightness, reach=4.5, color=(1, .95, .85), **extra):
        level['ceiling_lights'].append(dict(id=identity, fixture='core:fluorescent_panel_01',
            x=x, z=z, y=y, brightness=brightness, emission=.8, range=reach,
            color=list(color), rotation_degrees=0, **extra))

    for index, (x, z) in enumerate(((2.5, 2.5), (5.5, 5.5), (10.5, 2.5), (14.5, 5.5),
            (20.5, 3.5), (3.5, 9.5), (9.5, 9.5), (16.5, 9.5), (21.5, 9.5),
            (3.5, 14.5), (11.5, 14.5), (19.5, 15.5))):
        light(f'top_light_{index}', x, z, 10, .52, 4.8)
    light('vestibule_light', -1.5, 9.5, 10, .48, 4)
    for index, (x, z, power) in enumerate(((2.5, 3.5, .3), (2.5, 10.5, .23),
            (7.5, 6.5, .26), (8.5, 15.5, .25), (14.5, 8.5, .26),
            (13.5, 2.5, .24), (20.5, 5.5, .28), (21.5, 14.5, .34))):
        light(f'middle_light_{index}', x, z, 6.4, power, 3.4, (1, .91, .72))
    for index, (x, z, power) in enumerate(((3.5, 3.5, .18), (3.5, 14.5, .2),
            (8.5, 4.5, .18), (15.5, 16.5, .2), (21.5, 6.5, .22))):
        light(f'basement_light_{index}', x, z, 2.8, power, 2.7, (.87, .94, .79))
    # Two independent failures use the real prepared switchable contributions.
    # Long quiet periods and >=180ms interruptions avoid a strobing animation.
    for identity, x, z, delay, period, waits in (
            ('basement_failing_a', 8.5, 9.5, 1.8, 17.3, (.25, .5, 1.1)),
            ('basement_failing_b', 20.5, 12.5, 7.1, 23.7, (.35, .7, 1.4))):
        light(identity, x, z, 2.8, .32, 3, (.86, .95, .81), switchable=True,
              hold_in_static=True)
        sequence = identity+'_failure'
        steps = []
        for on, seconds in ((False, waits[0]), (True, waits[1]), (False, waits[2]), (True, 0)):
            steps.append(dict(step='action', action=dict(action='set_light', on=on)))
            if seconds:
                steps.append(dict(step='wait', seconds=seconds))
        level['sequences'].append(dict(id=sequence, looped=False, steps=steps))
        level['timers'].append(dict(id=identity+'_start', seconds=delay, autostart=True,
            bindings=[dict(on='timer', actions=[dict(action='start_sequence',
                target=identity, sequence=sequence), dict(action='start_timer',
                target=identity+'_repeat')])]))
        level['timers'].append(dict(id=identity+'_repeat', seconds=period, repeat=True,
            bindings=[dict(on='timer', actions=[dict(action='start_sequence',
                target=identity, sequence=sequence)])]))
    # Shaft lamps are fixed. They keep landings and both flights legible while
    # allowing the basement itself to retain genuinely dark spaces.
    light('shaft_roof_a', 26.5, 8.5, 10, .32, 4.4)
    light('shaft_roof_b', 32.5, 9.5, 10, .36, 4.4)
    light('shaft_upper', 26.5, 2.5, 10, .44, 5)
    for identity, x, z, y in (('shaft_lower', 35.86, 2, 2.1),
                               ('shaft_middle', 35.86, 16, 5.7)):
        level['ceiling_lights'].append(dict(id=identity, fixture='core:pool_light_wall',
            x=x, z=z, y=y, mount='wall', rotation_degrees=270,
            brightness=.38, emission=.6, range=6, color=[1, .92, .76]))
    # Stop skirting at every perpendicular partition, including junctions
    # halfway along long shell runs. This avoids buried trim and keeps one
    # physical board per visible face rather than relying on mesh overlap.
    clipped = []
    directions = {0: (1, 0), 90: (0, -1), 180: (-1, 0), 270: (0, 1)}
    for b in level['baseboards']:
        dx, dz = directions[b['rotation_degrees']]
        sx = b['x']-dz*b['thickness']/2
        sz = b['z']+dx*b['thickness']/2
        y = b['y']+b['height']/2
        spans = [(0, b['length'])]
        for w in level['walls']:
            if not w['y'] < y < w['y']+w['height']:
                continue
            solids = [(w['x'], w['x']+w['width'], w['z'], w['z']+w['depth'])]
            axis_x = w['width'] >= w['depth']
            for o in w.get('openings', []):
                if w['y']+o.get('sill', 0) <= y <= w['y']+o.get('sill', 0)+o['height']:
                    next_solids = []
                    for x0, x1, z0, z1 in solids:
                        lo = (w['x'] if axis_x else w['z'])+o['offset']
                        hi = lo+o['width']
                        if axis_x:
                            if x0 < lo:
                                next_solids.append((x0, min(x1, lo), z0, z1))
                            if x1 > hi:
                                next_solids.append((max(x0, hi), x1, z0, z1))
                        else:
                            if z0 < lo:
                                next_solids.append((x0, x1, z0, min(z1, lo)))
                            if z1 > hi:
                                next_solids.append((x0, x1, max(z0, hi), z1))
                    solids = next_solids
            for x0, x1, z0, z1 in solids:
                if dx and z0+.001 < sz < z1-.001:
                    a, c = sorted(((x0-sx)/dx, (x1-sx)/dx))
                elif dz and x0+.001 < sx < x1-.001:
                    a, c = sorted(((z0-sz)/dz, (z1-sz)/dz))
                else:
                    continue
                next_spans = []
                for lo, hi in spans:
                    if c <= lo or a >= hi:
                        next_spans.append((lo, hi))
                    else:
                        if lo < a:
                            next_spans.append((lo, a))
                        if c < hi:
                            next_spans.append((c, hi))
                spans = next_spans
        for lo, hi in spans:
            if hi-lo > .025:
                clipped.append(dict(b, x=round(b['x']+dx*lo, 4),
                    z=round(b['z']+dz*lo, 4), length=round(hi-lo, 4)))
    level['baseboards'] = clipped
    return level


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    serialized = json.dumps(build_level(), indent=2, ensure_ascii=False)+'\n'
    if args.check:
        if not OUTPUT.is_file() or OUTPUT.read_text() != serialized:
            raise SystemExit(f'{OUTPUT}: regenerate with tools/levels/build_office.py')
        print('Office source is current')
    else:
        OUTPUT.write_text(serialized)
        print(f'Wrote {OUTPUT}')


if __name__ == '__main__':
    main()
