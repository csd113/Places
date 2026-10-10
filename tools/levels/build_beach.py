#!/usr/bin/env python3
"""Author Beach: one connected coast, pier, seaside town and palm garden.

The existing compiler owns transport/collision/packages. Source artwork is
retained unchanged; scenery uses the registered Beach kit.
"""
from __future__ import annotations

import argparse
import json
import math
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path[:0] = [str(ROOT), str(ROOT / 'tools/props')]
from tools.levels.beach_components import daylight, shore_segment
from tools.props.parts.beach_structures import placed_components
from tools.props.parts.beach_nature import placed_components as nature_components

OUTPUT = ROOT / 'assets/levels/beach_demo.json'
FLOOR = -2.2
COAST_WIDTH = .75


def coast_z(x):
    # Binary-exact joints retain watertight support while keeping the visible
    # upper shore's lateral risers below one centimetre.
    distance = abs(x+1)
    tangent = 25/6
    bend = (.01*distance*distance if distance <= tangent
            else .01*tangent*tangent + (distance-tangent)/12)
    return round((-.9 + bend)*64)/64


COAST = tuple((x, coast_z(x+COAST_WIDTH/2))
              for x in (-24 + index*COAST_WIDTH for index in range(64)))
HERO = (20, 4.8, -62)


def serialise(value):
    return json.dumps(value, indent=2, ensure_ascii=False) + '\n'


def floor_at(level, x, z):
    """Authoring height query over this single seabed floor and its supports."""
    if not (-24.01 <= x <= 24.01 and -24.01 <= z <= 28.01):
        return 0.0  # Runtime prop authoring fallback outside every room.
    height = FLOOR
    for region in level['floor_regions']:
        if region['x'] <= x <= region['x'] + region['width'] and region['z'] <= z <= region['z'] + region['depth']:
            height = FLOOR + region['offset_y']
    for ramp in level['ramps']:
        if ramp['x'] <= x <= ramp['x'] + ramp['width'] and ramp['z'] <= z <= ramp['z'] + ramp['depth']:
            along_x = ramp['width'] >= ramp['depth']
            run = ramp['width'] if along_x else ramp['depth']
            distance = x - ramp['x'] if along_x else z - ramp['z']
            height = FLOOR + ramp['offset_y'] + ramp['rise'] * distance / run
    return height


def cut_rectangle(piece, cut):
    """Disjoint remainder; ramp cuts used here only split their X width."""
    x, z, w, d = cut
    ax, az = piece['x'], piece['z']
    bx, bz = ax + piece['width'], az + piece['depth']
    left, right, top, bottom = max(ax, x), min(bx, x+w), max(az, z), min(bz, z+d)
    if right <= left or bottom <= top:
        return [piece]
    rectangles = [(ax, az, left-ax, bz-az), (right, az, bx-right, bz-az),
                  (left, az, right-left, top-az), (left, bottom, right-left, bz-bottom)]
    return [{**piece, 'x': px, 'z': pz, 'width': pw, 'depth': pd}
            for px, pz, pw, pd in rectangles if pw > .00001 and pd > .00001]


def build_level():
    level = dict(format_version=3, id='beach_demo', name='Beach', author='Places',
                 spawn=dict(x=HERO[0], z=HERO[1], yaw_degrees=HERO[2]),
                 defaults=dict(wall='beach:rock_01', floor='beach:sand_01', ceiling='beach:roof_yellow_01'))
    for key in ('rooms', 'floor_regions', 'floor_patches', 'ramps', 'stairs', 'archways',
                'props', 'water', 'timers', 'routes', 'geometry_intent', 'void_walls'):
        level[key] = []
    level.update(daylight())
    level['environment'] = dict(presentation=dict(exposure=1, tone_knee=.8, saturation=1.04, contrast=1.02),
                               fog=dict(color=[.61, .82, .90], density=.002, reference_y=1, height_gain=.01))
    # One continuous seabed volume preserves irradiance probe coverage.
    level['rooms'] = [dict(x=-24, z=-24, width=48, depth=52, floor_y=FLOOR,
                           height=17, ceiling=dict(kind='open'), material='beach:sand_01')]
    # Inclusive shared edges resolve to the deeper support, so continuous
    # water never becomes buried under a neighbour's dry shore on that line.
    for x, shore in sorted(COAST, key=lambda item: item[1]):
        pieces = shore_segment(x, -24, shore, COAST_WIDTH)
        # A swimmer's bounded vertical rise must keep pace with the seabed.
        # Retain the upper beach endpoint, but halve the underwater grade.
        for ramp in pieces['ramps']:
            if ramp['rise'] == 1.6:
                ramp['z'] -= 2
                ramp['depth'] = 4
        level['ramps'] += pieces['ramps']
        region = pieces['floor_regions'][0]
        # Keep the dry floor and upper ramp on the same exact boundary.
        region['z'] = max(ramp['z'] + ramp['depth'] for ramp in pieces['ramps'])
        region['depth'] = 28 - region['z']
        level['floor_regions'].append(region)
        # Shared-edge water volumes meet exactly in the continuous seabed room,
        # without the helper's neighbour-room 4 cm inset.
        for water in pieces['water']:
            water.update(x=x, width=COAST_WIDTH)
            # One continuous turquoise cove. Distant ocean scenery retains
            # the deeper blue family; room/volume depth still drives extinction.
            water.update(material='beach:water_shallow_01', opacity=.60)
            level['water'].append(water)
    # Pier pier/approach owns this lane; never overlap it with a shoreline ramp.
    pier_cut = (6.96875, -10.25, 2.0625, 17.875)
    for key in ('ramps', 'floor_regions'):
        level[key] = [remainder for piece in level[key] for remainder in cut_rectangle(piece, pier_cut)]

    def region(x, z, w, d, y, material='beach:sand_01'):
        level['floor_regions'].append(dict(x=x, z=z, width=w, depth=d, offset_y=y-FLOOR,
                                          material=material, edge_material=material))

    # Quiet inland garden. Sand paths interrupt the grass rather than painting
    # a rectangular second surface over the supported floor.
    region(-22, 12, 24, 14, 0, 'beach:grass_01')
    region(-12, 12, 2.2, 14, 0)
    region(-22, 17.8, 24, 2, 0)
    region(-3.2, 12, 3.2, 7.8, 0)

    absolute_bases = {}

    def prop(name, x, z, identity, base=0, yaw=0, scale=1, **extra):
        value = dict(id=identity, model='beach:'+name, x=x, z=z,
                     rotation_degrees=yaw, scale=scale, solid=False, **extra)
        level['props'].append(value)
        absolute_bases[identity] = base
        return value

    def collider(identity, x, z, size, base, yaw=0):
        # The scaled committed peg carrier is buried within stock/seabed.
        value = dict(id=identity, model='outdoor:collision_peg', x=x, z=z,
                     scale=.05, size=[v/.05 for v in size], solid=True,
                     occludes=False, rotation_degrees=yaw)
        level['props'].append(value)
        absolute_bases[identity] = base

    def module(name, x, z, identity, base=0, yaw=0, nature=False):
        helper = nature_components if nature else placed_components
        pieces = helper(name, x, z, identity=identity, base_y=base, floor_y=FLOOR,
                        rotation_degrees=yaw, floor_at=lambda px, pz: floor_at(level, px, pz))
        # Recover world bases against the helper's original supports before
        # moving its hidden skirts or consolidating the pier floor.
        for value in pieces['props']:
            support_y = floor_at(level, value['x'], value['z'])
            for support in pieces['floor_regions']:
                if (support['x']-1e-8 <= value['x'] <= support['x']+support['width']+1e-8 and
                        support['z']-1e-8 <= value['z'] <= support['z']+support['depth']+1e-8):
                    support_y = FLOOR+support['offset_y']
            absolute_bases[value['id']] = support_y+value['y']
        if name == 'town_stairs':
            if yaw != 180:
                raise ValueError('Beach lookout support recipe is oriented toward north')
            # Hide support skirts inside the closed visual stock. Shared
            # risers move toward the higher tread, retaining contiguous tops.
            for support in pieces['floor_regions']:
                support['x'] += .006
                support['width'] -= .012
                support['z'] -= .006
            first = min(pieces['floor_regions'], key=lambda item: item['z'])
            first['z'] += .012
            first['depth'] -= .012
        elif name in ('town_terrace', 'stilt_house_blue', 'hut_brown'):
            for support in pieces['floor_regions']:
                support['x'] += .006
                support['z'] += .006
                support['width'] -= .012
                support['depth'] -= .012
        elif name == 'dock':
            # One continuous support below the stock also bridges module
            # joints. Individual skirts would coincide with plank end caps.
            pieces['floor_regions'] = []
        for key in ('floor_regions', 'stairs', 'archways'):
            level[key] += pieces[key]
        for value in pieces['props']:
            level['props'].append(value)

    # Low timber pier with a widened end and yellow-roof stall. The gentle
    # land approach reaches its 22.5 cm deck from real dry sand.
    module('dock', 8, 3.1, 'pier_land', base=-.7, yaw=90)
    # 4.42 m clears the actual 4.40 m post envelope (the planks extend
    # 4.267 m, beyond the helper's 4.10 m walking support). No coplanar joins.
    module('dock', 8, -1.32, 'pier_sea', base=-.7, yaw=90)
    module('dock', 8, -5.74, 'pier_outer', base=-.7, yaw=90)
    module('dock', 8, -9.1, 'pier_head', base=-.7)
    module('kiosk', 8, -9.1, 'pier_kiosk', base=.23)
    # Fill the invisible 8 mm under the entry step to meet its supported
    # deck. Otherwise an eye initialized on the 5 mm inset support can read
    # the floating step underside as overhead and be pushed beneath the pier.
    step = next(value for value in level['props'] if value['id'] == 'pier_kiosk_solid_5')
    absolute_bases[step['id']] -= .008
    step['size'][1] += .008/step['scale']
    collider('pier_kiosk_roof', 8, -9.1, [3.18, .11, 2.88], 3.21)
    region(6.96875, -8.125, 2.0625, 13.25, .225, 'beach:wood_plank_01')
    region(5.96875, -10.125, 4.0625, 2, .225, 'beach:wood_plank_01')
    for cut in ((6.96875, -8.125, 2.0625, 13.25), (5.96875, -10.125, 4.0625, 2)):
        cut = (cut[0]-.002, cut[1]-.002, cut[2]+.004, cut[3]+.004)
        level['water'] = [remainder for piece in level['water']
                          for remainder in cut_rectangle(piece, cut)]
    level['ramps'].append(dict(x=6.96875, z=5.125, width=2.0625, depth=2.5,
                              offset_y=2.425, rise=-.225, material='beach:wood_plank_01',
                              edge_material='beach:wood_plank_01'))
    module('stilt_house_blue', 12, 7, 'waterfront_blue_house', base=-.7)
    module('hut_brown', 21, 11, 'beach_brown_hut', base=-.015)
    collider('beach_brown_hut_roof', 21, 11, [3.48, .11, 3.22], 2.665)
    # Reach the raised plank decks before the controller's radius touches
    # their undersides. Floating slabs otherwise read as overhead stock while
    # the player still stands on sand. Keep supports inside the model skins.
    for x, z, width, depth, height in ((11.5, 8.746, 1, 1.5, .355),
                                      (20.5, 12.141, 1, 1.2, .300)):
        cut = (x, z, width, depth)
        level['floor_regions'] = [remainder for piece in level['floor_regions']
                                  for remainder in cut_rectangle(piece, cut)]
        level['ramps'].append(dict(x=x, z=z, width=width, depth=depth,
            offset_y=height-FLOOR, rise=-height, material='beach:wood_plank_01',
            edge_material='beach:wood_plank_01'))
    module('crate', 15, 8.6, 'delivery_crate')
    module('crate', 20, 13.5, 'hut_crate')
    module('signpost', 17, 11.8, 'town_sign')

    # Resting cove keeps the middle sand clear and gives furniture a purpose.
    module('umbrella', -2.5, 7, 'cove_umbrella')
    module('lounge_chair', -3.25, 7.5, 'cove_chair_west', yaw=180)
    module('lounge_chair', -1.65, 7.6, 'cove_chair_east', yaw=180)
    module('umbrella', -14, 10, 'garden_umbrella')
    module('lounge_chair', -13.4, 10.6, 'garden_chair', yaw=180)

    # Compact inhabited-looking town around a court and an ocean lookout.
    module('town_house_cream', 18, 17.2, 'town_cream')
    module('town_house_blue', 17.2, 25, 'town_blue', yaw=180)
    module('town_house_coral', 22, 22, 'town_coral', yaw=270)
    module('town_arch', 8, 24, 'town_gate')
    module('town_stairs', 12, 20, 'lookout_stairs', yaw=180)
    module('town_terrace', 12, 16.4, 'lookout_terrace', base=2.22)
    module('town_parapet', 12, 14.78, 'lookout_parapet', base=2.395)
    prop('bunting', 17.4, 21.2, 'town_bunting', base=3.45)
    for i, (x, z) in enumerate(((15.5, 19), (20, 24.3), (6.3, 23), (9.8, 23))):
        prop('town_shrub', x, z, f'town_shrub_{i}')

    # The arch can be approached and swum through; the lighthouse island is
    # deliberately distant scenery beyond the bounded cove's swim area.
    module('sea_arch', -17, -6, 'cove_sea_arch', base=-1.6, yaw=90, nature=True)
    prop('island', -19, -17, 'lighthouse_island', base=-1.2)
    module('lighthouse', -19, -17, 'island_lighthouse', base=2.25)
    prop('island', 2, -22, 'far_island', base=-.95, scale=.6)
    prop('palm_small', -18, -17, 'island_palm', base=2.2, scale=.55)
    prop('coastal_rock_wide', -8.5, -15, 'sea_rock', base=-.8, scale=1.5)

    # Landward escarpment and side headlands finish every accessible edge.
    for i, x in enumerate((-24, -18, -10, -2, 6, 24)):
        prop('island', x, 31.1, f'inland_ridge_{i}', base=-.1, scale=1.1)
    for side in (-1, 1):
        for i, z in enumerate((-7, 1, 10, 19)):
            if side == -1 and z == -7:
                continue  # Open ocean beyond the real sea-arch passage.
            prop('island', side*27.5, z, f'headland_{side}_{i}', base=-1.5 if z < 3 else -.1,
                 yaw=90, scale=1.05)
    for i, (x, z, wide) in enumerate(((-21, 8, True), (-17, 12, False), (-5.5, 11, False),
                                      (3.5, 14, True), (-20, 23, True), (22, 5.8, False))):
        name = 'coastal_rock_wide' if wide else 'coastal_rock'
        prop(name, x, z, f'land_rock_{i}', base=-.06)
        # Tight low stock blocker; no invisible high plane above the rock.
        collider(f'land_rock_{i}_solid', x, z, [2.3, 1.47, 1.8] if wide else [1.05, 1.16, .94], -.06)

    def palm(x, z, identity, small=False, yaw=0):
        name = 'palm_small' if small else 'palm'
        prop(name, x, z, identity, yaw=yaw)
        angle = math.radians(yaw)
        # Narrow banded trunk, not the full crown envelope. Three short bands
        # follow the bend; leaves remain freely traversable below their crown.
        for i, (lx, y, height) in enumerate(((-1.10, 0, 1.8), (-.89, 1.8, 1.7), (-.48, 3.5, 1.6))):
            if small:
                lx, y, height = lx*.84, y*.7, height*.7
            wx, wz = x+lx*math.cos(angle), z-lx*math.sin(angle)
            collider(f'{identity}_trunk_{i}', wx, wz, [.35, height, .36], y)

    for i, (x, z, small, yaw) in enumerate(((22, 7.5, False, 180), (16.5, 4, True, 30),
          (3.4, 3.7, True, -20), (-7.5, 4.5, False, 20), (-19, 5.4, True, 80),
          (-19, 14.2, False, -25), (-15.2, 21.8, False, 110), (-5.8, 23, True, 30),
          (-1, 16, True, 240), (5, 20, False, 140), (22, 26, False, 90))):
        palm(x, z, f'palm_{i}', small, yaw)
    # Decorative terrain sits under the authored grass/sand, with exposed
    # sloping borders only in planted beds outside the walking lines.
    prop('grassy_bank', 3, 30, 'garden_bank', base=-.10)
    prop('sand_patch', -19, 9, 'headland_sand', base=-.25)
    # Foam is scenery, independent of the narrow collision support bands.
    # Longer fitted ribbons preserve the asset's irregular breadth rather
    # than shrinking its Z silhouette into a ruler-thin line.
    for i in range(16):
        x = -22.5 + i*3
        slope = max(-1/12, min(1/12, .02*(x+1)))
        prop('shoreline_foam', x, coast_z(x)+.75, f'foam_{i}', base=-.12,
             yaw=-math.degrees(math.atan(slope)), scale=math.sqrt(1+slope*slope)*3.08/8,
             occludes=False)

    # Named clips are started explicitly, retaining fixed fly/swim anchors.
    def animal(name, clip, x, z, base, identity, yaw=0, scale=1):
        prop(name, x, z, identity, base=base, yaw=yaw, scale=scale, occludes=False,
             components=[dict(component='animation', clip=clip, looped=True, playing=False)])
        level['timers'].append(dict(id=identity+'_start', seconds=.01, autostart=True, repeat=False,
            bindings=[dict(on='timer', actions=[dict(action='play_animation', target=identity, clip=clip, loop=True)])]))
    animal('seagull', 'idle', 7.3, -8.4, .23, 'pier_gull', yaw=110)
    animal('seagull', 'fly', 5.4, -10.8, 2.7, 'flying_gull', yaw=220)
    animal('crab', 'idle', .8, 3.5, 0, 'resting_crab', yaw=70)
    prop('crab', -6, 7, 'walking_crab', occludes=False)
    level['routes'].append(dict(id='walking_crab', loop=True, steps=[
        dict(step='play', clip='idle', seconds=2.4, loop=True),
        dict(step='move_to', x=-5, z=7, speed=.160624564),
        dict(step='play', clip='idle', seconds=2.4, loop=True),
        dict(step='move_to', x=-6, z=7, speed=.160624564)]))
    animal('fish', 'swim', .7, -5.6, -1.08, 'shallows_fish', yaw=65)
    animal('fish', 'swim', 1.5, -5.8, -1.23, 'shallows_fish_pair', yaw=-25, scale=.8)

    # Opaque distant sea uses the actual single-surface water renderer, with
    # no box side/bottom faces to sort over its top. Five disjoint volumes
    # touch the real seabed perimeter and remain outside controller bounds.
    for x, z, w, d in ((-220, -220, 196, 196), (-24, -220, 48, 196),
                       (24, -220, 196, 196), (-220, -24, 196, 32), (24, -24, 196, 32)):
        level['water'].append(dict(x=x, z=z, width=w, depth=d, surface_y=-.12,
            bottom_y=FLOOR, material='beach:water_deep_01', opacity=1,
            attenuation_per_metre=0, swimming=False))
    # Real dressing ground extends under the tapering cliff feet outside
    # controller containment; no sky-coloured gap can show beneath the ridge.
    for identity, bounds in (('ridge_ground', ((-35, -.2, 28), (35, -.005, 40))),
                             ('west_ground', ((-35, -.2, 8), (-24, -.005, 28))),
                             ('east_ground', ((24, -.2, 8), (35, -.005, 28)))):
        level['void_walls'].append(dict(id=identity, min=list(bounds[0]), max=list(bounds[1]),
            material='beach:sand_01', faces='outward', solid=False, occludes=False))
    # Buried containment is behind visible rock stock on land. At sea it is
    # a bounded swimming area; the ocean beyond remains visual scenery.
    for identity, x, z, size in (('west', -24, 8, [.24, 12, 40]), ('east', 24, 8, [.24, 12, 40]),
                                 ('south', 0, 27.5, [48, 12, .24]), ('swim_limit', 0, -12, [48, 5, .24])):
        collider('boundary_'+identity, x, z, size, -2.35)
    level['geometry_intent'].append(dict(check='missing-wall', x=-24, z=-24, width=48, depth=52,
        note='Deliberate open beach/sea floor; side and inland cliffs contain the route, bounded cove swimming faces distant sea scenery.'))
    for x, z, w, d in ((-26.2, -26.2, 2.2, 56.4), (24, -26.2, 2.2, 56.4),
                       (-24, -26.2, 48, 2.2), (-24, 28, 48, 2.2)):
        level['geometry_intent'].append(dict(check='room-leak', x=x, z=z, width=w, depth=d,
            note='Outside authored floor and behind retained boundary stock; controller traversal verifies containment.'))
    # All support is frozen before floor-relative offsets are finalized.
    for value in level['props']:
        value['y'] = round(absolute_bases[value['id']] - floor_at(level, value['x'], value['z']), 6)
    # The checker sees the joined bands' closed vertical caps as reversed
    # faces. They are inside neighbouring supported solids, never duplicated
    # walking tops. Keep these declarations on the cap planes themselves.
    for edge in (-24 + index*COAST_WIDTH for index in range(len(COAST)+1)):
        for check in ('reversed-face', 'coplanar-sliver'):
            level['geometry_intent'].append(dict(check=check, x=edge-.005, z=-24,
                width=.01, depth=52,
                note='Back-to-back vertical shore-band caps; one walking top per point, lateral steps at most 2.5 cm.'))
    for x, z, w, d in ((-35, 27.995, 70, .01), (-35, 7.995, 11, .01), (24, 7.995, 11, .01)):
        for check in ('reversed-face', 'coplanar-sliver'):
            level['geometry_intent'].append(dict(check=check, x=x, z=z, width=w, depth=d,
                note='Buried opposing cliff-ground box caps behind containment; dressing tops sit below supported sand.'))
    for edge in (6.96875, 9.03125):
        level['geometry_intent'].append(dict(check='reversed-face', x=edge-.005, z=-10.25,
            width=.01, depth=17.875,
            note='Buried opposing vertical caps where the pier support replaces shoreline support; walking surfaces are disjoint.'))
    for value in level['props']:
        if value['model'] != 'outdoor:collision_peg':
            continue
        top = absolute_bases[value['id']] + value['size'][1]*value['scale']
        if abs(top-floor_at(level, value['x'], value['z'])) <= .02:
            level['geometry_intent'].append(dict(check='prop-layer-coplanar',
                x=value['x']-.005, z=value['z']-.005, width=.01, depth=.01,
                note='Buried 3 mm visual peg carries structural collision; size describes its blocker, not a rendered dressing slab.'))
    return level


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    expected = serialise(build_level())
    if args.check:
        if not OUTPUT.is_file() or OUTPUT.read_text() != expected:
            print('Beach source is stale; run tools/levels/build_beach.py')
            return 1
        print('Beach source is deterministic and current')
    else:
        OUTPUT.write_text(expected)
        print(f'Authored {OUTPUT}')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
