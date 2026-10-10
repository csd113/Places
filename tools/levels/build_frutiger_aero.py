#!/usr/bin/env python3
"""Compose the Aero reference into a connected atrium, corridor and reception.

Visual modules load committed PNGs. Separate tight structural colliders keep
glazed openings, the vaulted tube and open doors usable by the real controller.
"""
from __future__ import annotations

import argparse
import json
import math
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path[:0] = [str(ROOT / 'tools/props')]
from parts.frutiger_aero import placed_components

OUTPUT = ROOT / 'assets/levels/frutiger_aero_demo.json'
WHITE = 'frutiger_aero:white_panel_01'
FLOOR = 'frutiger_aero:white_floor_01'
AQUA = 'frutiger_aero:aqua_tile_01'


def build_level():
    level = dict(format_version=3, id='frutiger_aero_demo', name='Frutiger Aero', author='Places',
                 spawn=dict(x=2.5, z=10.1, yaw_degrees=38),
                 defaults=dict(wall=WHITE, floor=FLOOR, ceiling=WHITE),
                 sky=dict(texture='frutiger_aero:tex_sky_day_01', brightness=1,
                          ambient=.32, ambient_color=[.58, .75, 1]),
                 global_illuminators=[dict(id='aero_sun', kind='directional',
                     direction=[-.5, -math.sqrt(.5), .5], color=[1, .94, .82],
                     intensity=.9, angular_size_degrees=.8, cast_shadows=True, enabled=True, bake=True)],
                 environment=dict(presentation=dict(exposure=1, tone_knee=.8, saturation=1.04, contrast=1.02),
                                  fog=dict(color=[.66, .84, .94], density=.002, reference_y=1, height_gain=.01)))
    for key in ('rooms', 'walls', 'floor_regions', 'floor_patches', 'ramps', 'stairs',
                'props', 'water', 'decals', 'ceiling_lights', 'geometry_intent', 'thresholds', 'guardrails'):
        level[key] = []
    level['rooms'] = [
        dict(x=0, z=0, width=12, depth=12, height=7, ceiling=dict(kind='open'), material=FLOOR),
        dict(x=12, z=4, width=10, depth=4, height=3.4, material=FLOOR, ceiling_material='frutiger_aero:white_ceiling_01'),
        dict(x=22, z=1, width=10, depth=11, height=3.4, material=FLOOR, ceiling_material='frutiger_aero:white_ceiling_01'),
        dict(x=0, z=12, width=32, depth=8, height=12, ceiling=dict(kind='open'), material=FLOOR),
    ]
    catalog = json.loads((ROOT / 'assets/catalog.json').read_text())
    sizes = {entry['id']: entry['size'] for entry in catalog['assets'] if entry.get('asset_type') == 'prop'}

    def prop(name, x, z, identity, y=0, yaw=0, **extra):
        entry = dict(id=identity, model='frutiger_aero:'+name, x=x, z=z, y=y,
                     rotation_degrees=yaw, size=sizes['frutiger_aero:'+name], solid=False,
                     occludes=False, **extra)
        level['props'].append(entry)
        pieces = placed_components(name, x, z, base_y=y, rotation_degrees=yaw, identity=identity)
        level['props'].extend(pieces['props'][1:])
        return entry

    def blocker(identity, x, z, width, height, depth, base=-.06, yaw=0):
        level['props'].append(dict(id=identity, model='outdoor:collision_peg', x=x, z=z,
            y=base, rotation_degrees=yaw, scale=.05, size=[width/.05, height/.05, depth/.05],
            solid=True, occludes=False, comment='Buried carrier; separate structural collision inside visible stock.'))

    def wall(x, z, width, depth, height=3.4, y=0, **extra):
        level['walls'].append(dict(x=x, z=z, width=width, depth=depth,
                                   height=height, y=y, material=WHITE, **extra))

    def bay(x, z, identity, yaw=0, tall=False):
        prop('wall_bay_tall' if tall else 'wall_bay', x, z, identity, yaw=yaw)

    # A circular glazed roof sits on a continuous white upper ring. The lower
    # segmented windows carry modelled seams, rounded returns and leaf marks.
    for index, x in enumerate((1.8, 5.4, 9.0)):
        bay(x, .10, 'atrium_north_'+str(index), tall=True)
    wall(10.8, -.12, 1.32, .24, 4.2)
    for index, z in enumerate((1.8, 5.4, 9.0)):
        bay(.10, z, 'atrium_west_'+str(index), yaw=90, tall=True)
    wall(-.12, 10.8, .24, 1.32, 4.2)
    for edge in (0, 12):
        wall(-.12, edge-.12, 12.24, .24, .20, y=4.0)
        wall(edge-.12, -.12, .24, 12.24, .20, y=4.0)
    bay(1.8, 11.9, 'atrium_south_west', yaw=180, tall=True)
    bay(10.2, 11.9, 'atrium_south_east', yaw=180, tall=True)
    for index, z in enumerate((1.8, 10.2)):
        bay(11.9, z, 'atrium_east_'+str(index), yaw=270, tall=True)
    # Straight white spandrels close the bay/portal junctions without a box
    # through the opening. Portal stock itself owns matching tight blockers.
    for x in (3.6, 7.8):
        wall(x, 11.78, .6, .24, 4.2)
    for z in (3.6, 7.8):
        wall(11.78, z, .24, .6, 4.2)

    def arch(x, z, identity, yaw=0):
        prop('lime_arch', x, z, identity, yaw=yaw)

    arch(12, 6, 'atrium_corridor_arch', yaw=90)
    arch(6, 11.9, 'atrium_terrace_arch')
    prop('atrium_dome', 6, 6, 'glazed_dome', y=4.2)
    # Aqua/white floor variation follows circulation and the focal basin.
    for x in (1.2, 2.4, 3.6, 7.2, 8.4, 9.6):
        for z in (1.2, 2.4, 3.6, 7.2, 8.4, 9.6):
            if (round(x/1.2)+round(z/1.2)) % 2 == 0:
                level['floor_patches'].append(dict(x=x, z=z, width=1.2, depth=1.2, material=AQUA))
    prop('fountain_basin', 6, 6, 'atrium_fountain')
    prop('bubble_sculpture', 6, 6, 'three_glass_bubbles', y=.16)
    level['water'].append(dict(shape='circle', x=4.70, z=4.70, radius=1.30,
        surface_y=.37, bottom_y=.11, material='frutiger_aero:cyan_water_01',
        opacity=.44, attenuation_per_metre=.1, swimming=False))
    blocker('sculpture_base', 6, 6, .95, 2.65, .95, base=.1)

    def planter(x, z, identity, yaw=0):
        prop('tree_planter', x, z, identity, yaw=yaw)

    def bench(x, z, identity, yaw=0):
        prop('seating_pod', x, z, identity, yaw=yaw)

    planter(1.2, 2, 'atrium_tree_north')
    planter(10.6, 9.6, 'atrium_tree_south')
    bench(3.0, 2.0, 'atrium_bench')
    bench(9.5, 8.3, 'atrium_bench_east', yaw=270)
    prop('display_kiosk', 1.3, 8.0, 'welcome_kiosk', yaw=90)
    prop('glass_partition', 3.0, 4.3, 'atrium_folded_glass', yaw=90)
    prop('banner_atrium', 9.0, .40, 'nature_people_banner', y=1.0)
    prop('light_pod', 6, 6, 'atrium_suspended_pod', y=3.0,
         lights=[dict(shape='point', offset=[0, -.12, 0], color=[.70, .94, 1],
                      intensity=.38, range=6, falloff='smooth')])

    # Corridor: repeated inset circular cyan lights, segmented glazed south
    # wall and white north branding wall; open terminal leaves lead onward.
    wall(12.18, 3.88, 9.70, .24)
    for index, x in enumerate((13.8, 17.4, 21.0)):
        bay(x, 7.90, 'corridor_window_'+str(index), yaw=180)
    for index, x in enumerate((13.4, 15.8, 18.2, 20.6)):
        prop('ceiling_ring', x, 6, 'corridor_ring_'+str(index), y=3.25,
             lights=[dict(shape='rect', half_width=.38, half_depth=.38, offset=[0, -.08, 0], color=[.72, .94, 1],
                          intensity=.25, range=4.5, falloff='smooth')])
    planter(18.8, 4.75, 'corridor_planter')
    prop('double_doors_open', 22.503, 6, 'open_terminal_doors', yaw=90)
    prop('glass_canopy', 24.4, 6, 'reception_glass_tube', yaw=90)
    level['decals'].append(dict(x=15.9, z=4.121, y=1.65, width=2.2, height=2.2,
        surface='wall_south', material='frutiger_aero:decal_corridor_typography_01'))

    # Reception has space in front, beside and behind the curved counter.
    wall(22.12, .88, 10, .24, openings=[dict(kind='window', offset=6.68, width=3.0,
        height=2.65, sill=.4, glass='frutiger_aero:cyan_glass_01', solid=True)])
    for index, z in enumerate((2.8, 6.4, 10.0)):
        bay(31.9, z, 'reception_east_window_'+str(index), yaw=270)
    wall(21.88, 1, .24, 3.15)
    wall(21.88, 7.85, .24, 4.15)
    wall(22, 11.88, 3, .24)
    wall(28, 11.88, 4, .24)
    wall(25, 11.88, 3, .24, .4, y=3.0)
    for x in (26.6, 29.4):
        level['ceiling_lights'].append(dict(fixture='home:ceiling_light_round', x=x, z=7.7,
            brightness=.60, range=7, color=[1, .96, .88], emission=.7))
    prop('reception_counter', 28, 3.0, 'curved_reception_counter')
    prop('reception_soffit', 28, 3, 'rounded_reception_soffit', y=2.92,
         lights=[dict(shape='point', offset=[x, -.12, .2], color=[1, .94, .77],
                      intensity=.22, range=4.5, falloff='smooth') for x in (-1.6, 0, 1.6)])
    prop('accent_panel', 31.55, 4.1, 'reception_diagonal_accent', y=.65, yaw=270)
    prop('banner_reception', 28, 1.16, 'places_reception_banner', y=.60)
    prop('double_doors', 23.6, 1.08, 'service_double_doors')
    bench(29.9, 9.8, 'reception_waiting_bench', yaw=270)
    planter(23.1, 10.2, 'reception_tree')
    prop('banner_corridor', 31.6, 8.1, 'cleaner_spaces_banner', y=.5, yaw=270)
    level['floor_patches'].append(dict(x=25.8, z=4.0, width=4.4, depth=1.0, material=AQUA))

    # The unwalked pocket behind corridor glazing has a finished planted edge.
    wall(12, 11.8, 10, .2, .65)
    blocker('corridor_garden_boundary', 17, 11.9, 10, 3.5, .2)
    prop('green_backdrop', 17, 10, 'corridor_garden_backdrop', scale=.4)

    # Accessible garden terrace closes the exploration loop back to the hero.
    for x in (3.0, 14.0, 29.0):
        planter(x, 17.4, 'terrace_tree_'+str(int(x)))
    bench(9.0, 17.6, 'terrace_bench')
    prop('accent_panel', 24, 17.6, 'terrace_accent', yaw=180)
    for x in (0.0, 31.8):
        wall(x, 12, .2, 8, .65)
        blocker('terrace_side_'+str(int(x)), x+.1, 16, .2, 3.5, 8)
    wall(0, 19.8, 32, .2, .65)
    blocker('terrace_south_boundary', 16, 19.9, 32, 3.5, .2)
    # Low garden boundary with foliage and an uncluttered city beyond the glass.
    prop('green_backdrop', 12, 23.3, 'south_green_landscape')
    prop('city_backdrop', 16, -13, 'north_blue_city')
    prop('green_backdrop', 12, -5.8, 'north_green_landscape')
    prop('city_backdrop', -11, 8, 'west_blue_city', yaw=90)
    prop('green_backdrop', -4.0, 8, 'west_green_landscape', yaw=90)
    # Kit glazed envelopes own player containment; annotations describe actual
    # structural intent rather than suppressing a door or support defect.
    for x, z, w, d, note in ((-.3, -.3, 12.6, .7, 'Glazed northern kit wall, tight pane collision.'),
        (-.3, -.3, .7, 12.6, 'Glazed western kit wall, tight pane collision.'),
        (-.3, 11.5, 12.6, .8, 'Glazed southern kit wall with genuine terrace portal.'),
        (11.5, -.3, .8, 12.6, 'Glazed eastern kit wall with genuine corridor portal.'),
        (12, 7.5, 10, .8, 'Glazed corridor kit wall with tight collision.'),
        (31.5, 1, .8, 11, 'Glazed reception kit wall with tight collision.'),
        (21.5, 4, 1, 4, 'Open gray terminal doors join supported corridor and reception floors.'),
        (25, 11.5, 3, .8, 'Reception garden passage on continuous support.'),
        (0, 12, .3, 8, 'Garden parapet and buried containment.'),
        (31.7, 12, .3, 8, 'Garden parapet and buried containment.'),
        (0, 19.7, 32, .3, 'Garden parapet and buried containment.')):
        for check in ('missing-wall', 'room-leak'):
            level['geometry_intent'].append(dict(check=check, x=x, z=z, width=w, depth=d, note=note))
    # The checker flood-fill ignores decorative model panes / prop carriers;
    # bound each reported outside witness to the specific glazed perimeter.
    for x, z, w, d, note in ((-2.1, 0, 2.4, 12, 'West kit glazing has tight pane collision.'),
        (0, -2.1, 12, 2.4, 'North kit glazing has tight pane collision.'),
        (12, 7.5, 10, 2.6, 'Corridor kit glazing has tight pane collision.'),
        (31.5, 1, 2.6, 11, 'East reception kit glazing has tight pane collision.')):
        level['geometry_intent'].append(dict(check='room-leak', x=x, z=z, width=w, depth=d, note=note))
    # Inscribed collider strips lie within the real circular GLB floor slab.
    # Their .095 m top is 5 mm below its .10 m visual top; no rendered square
    # regions or duplicate surfaces interrupt the round basin under the water.
    count, radius = 26, 1.30
    for index in range(count):
        ax = -radius+index*2*radius/count
        bx = -radius+(index+1)*2*radius/count
        half_depth = math.sqrt(max(0, radius*radius-max(abs(ax), abs(bx))**2))
        if half_depth > 0:
            blocker('fountain_floor_'+str(index), 6+(ax+bx)/2, 6,
                    bx-ax, .095, 2*half_depth, base=0)
    return level


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    text = json.dumps(build_level(), indent=2, ensure_ascii=False)+'\n'
    if args.check:
        if not OUTPUT.exists() or OUTPUT.read_text() != text:
            raise SystemExit('Frutiger Aero source is stale; run its generator')
        print('Frutiger Aero source current')
    else:
        OUTPUT.write_text(text)
        print(OUTPUT)


if __name__ == '__main__':
    main()
