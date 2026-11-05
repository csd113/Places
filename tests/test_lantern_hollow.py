"""Bounded source contracts for the native-authored Lantern Hollow rebuild.

These ten checks replace the retired bootstrap generator's layout assumptions.
They protect distinct homes, usable openings/fences, source collision clearances,
roof geometry, actor/quest wiring and the collection return path. They read one
small source, two Luau files and five tiny reused roof meshes; no bake, renderer,
controller simulation or generated replacement map participates in ordinary CI.
Real traversal, dialogue cycling and saved revisits require native acceptance.
"""
from __future__ import annotations

from collections import Counter
import json
import math
from pathlib import Path
import re
import sys
import unittest

from tools.levels.curated_maps import environment_source

ROOT = Path(__file__).resolve().parents[1]
EPSILON = 1e-4  # Native editor values retain f32 roundoff on ordinary JSON save.


def contains(room, point):
    return (room['x'] - EPSILON <= point['x'] <= room['x'] + room['width'] + EPSILON
            and room['z'] - EPSILON <= point['z'] <= room['z'] + room['depth'] + EPSILON)


def bounds(prop):
    width, _, depth = prop['size']
    scale = prop.get('scale', 1)
    yaw = math.radians(prop.get('rotation_degrees', 0))
    half_x = (width * abs(math.cos(yaw)) + depth * abs(math.sin(yaw))) * scale / 2
    half_z = (width * abs(math.sin(yaw)) + depth * abs(math.cos(yaw))) * scale / 2
    return prop['x'] - half_x, prop['z'] - half_z, prop['x'] + half_x, prop['z'] + half_z


def overlaps(a, b):
    return (min(a[2], b[2]) - max(a[0], b[0]) > EPSILON
            and min(a[3], b[3]) - max(a[1], b[1]) > EPSILON)


def distance_to_segment(point, start, end):
    dx, dz = end[0] - start[0], end[1] - start[1]
    denominator = dx * dx + dz * dz
    if denominator == 0:
        return math.dist(point, start)
    fraction = max(0, min(1, ((point[0] - start[0]) * dx
                              + (point[1] - start[1]) * dz) / denominator))
    return math.dist(point, (start[0] + fraction * dx, start[1] + fraction * dz))


class LanternHollowTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.source = environment_source('lantern_hollow')
        cls.level = json.loads(cls.source.read_text(encoding='utf-8'))
        cls.project = cls.source.parent.parent
        cls.homes = [room for room in cls.level['rooms']
                     if room.get('ceiling', {}).get('kind') != 'open']
        cls.props = cls.level['props']
        cls.fires = [prop for prop in cls.props if prop['model'] == 'outdoor:showcase_campfire']

    def furniture(self, room):
        return [prop for prop in self.props if contains(room, prop) and prop.get('solid')
                and not prop['model'].startswith('outdoor:house_')]

    def shell(self, room):
        return [wall for wall in self.level['walls']
                if wall.get('material') == 'outdoor:house_siding_01' and contains(room, wall)]

    def door(self, room):
        doors = [door for door in self.level['doors'] if contains(room, door)]
        self.assertEqual(len(doors), 1, 'Every home retains one usable front door')
        return doors[0]

    def test_authored_night_sky_moon_and_lit_homes_are_preserved(self):
        self.assertEqual(self.level['sky']['texture'], 'outdoor:tex_sky_stars_01')
        self.assertAlmostEqual(self.level['sky']['ambient'], .22)
        moons = [light for light in self.level['global_illuminators'] if light['kind'] == 'moon']
        self.assertEqual(len(moons), 1)
        moon = moons[0]
        self.assertTrue(moon['enabled'] and moon['cast_shadows'] and moon['bake'])
        self.assertGreater(moon['intensity'], 0)
        self.assertLess(moon['direction'][1], 0)
        for room in self.homes:
            self.assertTrue(any(contains(room, light) and light.get('brightness', 1) > 0
                                for light in self.level['ceiling_lights']), room)
        # Fog is optional source data: absence is not a missing-required-key error.
        for fog in self.level.get('fog_regions', []):
            self.assertGreaterEqual(fog['density'], 0)
            self.assertLess(fog['density'], .01, 'Night visibility must not be replaced by opaque fog')

    def test_eight_furnished_homes_have_distinct_real_partition_plans(self):
        self.assertEqual(len(self.homes), 8)
        self.assertEqual(Counter(round(room['z'], 3) for room in self.homes), {-10.4: 4, 9: 4})
        plans = []
        for room in self.homes:
            self.assertGreaterEqual(len(self.furniture(room)), 5)
            shell = self.shell(room)
            self.assertEqual(len(shell), 4)
            partitions = [wall for wall in self.level['walls'] if contains(room, wall)
                          and wall not in shell]
            self.assertGreaterEqual(len(partitions), 2)
            plans.append(tuple(sorted((round(wall['x'] - room['x'], 3),
                                       round(wall['z'] - room['z'], 3),
                                       round(wall['width'], 3), round(wall['depth'], 3))
                                      for wall in partitions)))
            door = self.door(room)
            self.assertEqual(door['initial_state'], 'closed')
            self.assertTrue(door['bidirectional'])
            openings = [opening for wall in shell for opening in wall.get('openings', [])
                        if opening['kind'] == 'door']
            self.assertEqual(len(openings), 1)
            self.assertGreaterEqual(openings[0]['width'], .8)
            self.assertGreaterEqual(openings[0]['height'], 2.0)
        self.assertEqual(len(set(plans)), 8, 'Different furnishings alone cannot mask repeated plans')

    def test_domestic_solids_and_front_door_approaches_remain_clear(self):
        for room in self.homes:
            furniture = self.furniture(room)
            self.assertTrue(furniture, 'Clearance checks must not pass over an empty ID filter')
            door = self.door(room)
            centre = door['x'] + door['width'] / 2
            toward_interior = 1 if door['z'] < room['z'] + room['depth'] / 2 else -1
            far_z = door['z'] + toward_interior * 1.2
            approach = (centre - .4, min(door['z'], far_z),
                        centre + .4, max(door['z'], far_z))
            for index, prop in enumerate(furniture):
                box = bounds(prop)
                self.assertGreaterEqual(box[0] + EPSILON, room['x'] + .3, prop['id'])
                self.assertLessEqual(box[2] - EPSILON, room['x'] + room['width'] - .3, prop['id'])
                self.assertGreaterEqual(box[1] + EPSILON, room['z'] + .3, prop['id'])
                self.assertLessEqual(box[3] - EPSILON, room['z'] + room['depth'] - .3, prop['id'])
                self.assertFalse(overlaps(box, approach), ('front door approach', prop['id']))
                for other in furniture[index + 1:]:
                    self.assertFalse(overlaps(box, bounds(other)), (prop['id'], other['id']))

    def test_floor_carriers_tile_the_compact_world_without_duplicate_surfaces(self):
        rooms = self.level['rooms']
        self.assertTrue(rooms)
        west = min(room['x'] for room in rooms)
        north = min(room['z'] for room in rooms)
        east = max(room['x'] + room['width'] for room in rooms)
        south = max(room['z'] + room['depth'] for room in rooms)
        self.assertAlmostEqual(east - west, 48, places=3)
        self.assertAlmostEqual(south - north, 50.8, places=3)
        for index, room in enumerate(rooms):
            self.assertGreater(room['width'], 0)
            self.assertGreater(room['depth'], 0)
            for other in rooms[index + 1:]:
                self.assertFalse(overlaps((room['x'], room['z'], room['x'] + room['width'],
                                           room['z'] + room['depth']),
                                          (other['x'], other['z'], other['x'] + other['width'],
                                           other['z'] + other['depth'])), (room, other))
        self.assertAlmostEqual(sum(room['width'] * room['depth'] for room in rooms),
                               (east - west) * (south - north), places=3)

    def test_seated_skeleton_circle_keeps_seven_seats_and_real_held_animation(self):
        self.assertEqual(len(self.fires), 1)
        fire = self.fires[0]
        skeletons = [prop for prop in self.props if prop['model'] == 'skeleton']
        seats = [prop for prop in self.props if prop['model'] == 'outdoor:showcase_stump_seat']
        self.assertEqual(len(skeletons), 6)
        self.assertEqual(len(seats), 7)
        self.assertEqual(len({prop['id'] for prop in skeletons + seats}), 13)
        routes = {route['id']: route for route in self.level['routes']}
        occupied = set()
        for skeleton in skeletons:
            self.assertAlmostEqual(math.dist((skeleton['x'], skeleton['z']),
                                             (fire['x'], fire['z'])), 2.6, places=3)
            matches = [seat for seat in seats if math.dist((skeleton['x'], skeleton['z']),
                                                           (seat['x'], seat['z'])) < .001]
            self.assertEqual(len(matches), 1)
            occupied.add(matches[0]['id'])
            cue = routes[skeleton['id']]['steps'][0]
            self.assertEqual(cue['clip'], 'pose_sit_chair')
            self.assertTrue(cue['loop'])
            self.assertGreater(cue['seconds'], 60)
            self.assertTrue(any(action.get('handler') == 'group'
                                and action.get('script') == 'scripts/lantern_hollow.luau'
                                for binding in skeleton['bindings'] if binding['on'] == 'interact'
                                for action in binding['actions']))
        self.assertEqual(len(occupied), 6, 'Two actors must not occupy the same stool')

    def test_ten_man_woman_neighbors_keep_thirty_unique_dialogue_lines(self):
        residents = [prop for prop in self.props if prop['model'] == 'mannequin']
        self.assertEqual(len(residents), 10)
        self.assertEqual(Counter(prop['character_texture'] for prop in residents),
                         {'core:mannequin_skin_man': 5, 'core:mannequin_skin_woman': 5})
        occupancy = [sum(contains(room, prop) for prop in residents) for room in self.homes]
        self.assertEqual(sorted(occupancy), [1, 1, 1, 1, 1, 1, 2, 2])
        scripts = set()
        for resident in residents:
            bindings = [action for binding in resident['bindings'] if binding['on'] == 'interact'
                        for action in binding['actions'] if action['action'] == 'run_script']
            self.assertEqual(len(bindings), 1)
            self.assertEqual(bindings[0]['handler'], 'resident')
            scripts.add(bindings[0]['script'])
        self.assertEqual(len(scripts), 1)
        self.assertTrue(scripts <= set(self.level['scripts']))
        script = (self.project / scripts.pop()).read_text(encoding='utf-8')
        records = re.findall(r'\["([^\"]+)"\]\s*=\s*\{name="([^\"]+)",\s*lines=\{(.*?)\}\s*\}',
                             script, flags=re.DOTALL)
        self.assertEqual(len(records), 10)
        self.assertEqual({identity for identity, _, _ in records}, {p['id'] for p in residents})
        names = {p['id']: p['display_name'].upper() for p in residents}
        all_lines = []
        for identity, name, body in records:
            self.assertEqual(name, names[identity])
            lines = [json.loads(value) for value in re.findall(r'"(?:\\.|[^"\\])*"', body)]
            self.assertEqual(len(lines), 3)
            self.assertTrue(all(line.strip() for line in lines))
            all_lines.extend(lines)
        self.assertEqual(len(set(all_lines)), 30)
        self.assertFalse(any('spectral' in p['model'] or p['model'] == 'car' for p in self.props))

    def test_each_home_has_its_own_fence_and_player_width_gate(self):
        fences = self.level['guardrails']
        self.assertEqual(len(fences), 8)
        owned = set()
        for fence in fences:
            path = [(fence['x'], fence['z']), *map(tuple, fence['path'])]
            self.assertGreaterEqual(len(path), 5)
            self.assertGreaterEqual(fence['height'], 1)
            self.assertAlmostEqual(math.dist(path[0], path[-1]), 1.7, places=3)
            self.assertAlmostEqual(path[0][1], path[-1][1], places=3)
            west, east = min(x for x, _ in path), max(x for x, _ in path)
            north, south = min(z for _, z in path), max(z for _, z in path)
            matches = [index for index, room in enumerate(self.homes)
                       if west < room['x'] and room['x'] + room['width'] < east
                       and north <= room['z'] and room['z'] + room['depth'] <= south]
            self.assertEqual(len(matches), 1)
            self.assertNotIn(matches[0], owned)
            owned.add(matches[0])
            door = self.door(self.homes[matches[0]])
            self.assertAlmostEqual((path[0][0] + path[-1][0]) / 2,
                                   door['x'] + door['width'] / 2, places=3)
        self.assertEqual(len(owned), 8)

    def test_house_shell_joins_and_real_roof_soffits_remain_outside_interior(self):
        sys.path.insert(0, str(ROOT / 'tools/props'))
        from glb import read_glb
        catalog = {asset['id']: asset for asset in
                   json.loads((ROOT / 'assets/catalog.json').read_text(encoding='utf-8'))['assets']}
        meshes = {}
        for room in self.homes:
            shell = self.shell(room)
            horizontal = sorted((wall for wall in shell if wall['width'] > wall['depth']),
                                key=lambda wall: wall['z'])
            vertical = [wall for wall in shell if wall['width'] < wall['depth']]
            self.assertEqual((len(horizontal), len(vertical)), (2, 2))
            back, front = horizontal
            for side in vertical:
                self.assertAlmostEqual(back['z'] + back['depth'], side['z'], places=3)
                self.assertAlmostEqual(side['z'] + side['depth'], front['z'], places=3)
                for end in horizontal:
                    self.assertLessEqual(end['x'] - EPSILON, side['x'])
                    self.assertGreaterEqual(end['x'] + end['width'] + EPSILON,
                                            side['x'] + side['width'])
            props = [prop for prop in self.props if contains(room, prop)]
            slopes = [prop for prop in props if prop['model'].endswith('_roof_slope')]
            caps = [prop for prop in props if prop['model'].endswith('_roof_ridge')]
            self.assertEqual((len(slopes), len(caps)), (6, 3))
            for slope in slopes:
                if slope['model'] not in meshes:
                    meshes[slope['model']] = read_glb(
                        (ROOT / 'assets' / catalog[slope['model']]['model']).read_bytes())
                mesh = meshes[slope['model']]
                self.assertAlmostEqual(slope['y'], room.get('floor_y', 0) + room['height'], places=3)
                low_points = [point for point in mesh.positions if point[1] <= .03501]
                self.assertTrue(low_points)
                sign = -1 if slope.get('rotation_degrees') == 180 else 1
                eave_z = [slope['z'] + sign * point[2] for point in low_points]
                if sign == 1:
                    self.assertGreaterEqual(min(eave_z) - front['z'], .0099)
                else:
                    self.assertGreaterEqual(back['z'] + back['depth'] - max(eave_z), .0099)
                aligned = [cap for cap in caps if abs(cap['x'] - slope['x']) < EPSILON]
                self.assertEqual(len(aligned), 1)
                cap = aligned[0]
                cap_size = catalog[cap['model']]['size']
                ridge = [point for point in mesh.positions if point[2] < -1.2999]
                self.assertTrue(ridge)
                ridge_z = slope['z'] + sign * ridge[0][2]
                self.assertGreater(ridge_z, cap['z'] - cap_size[2] / 2)
                self.assertLess(ridge_z, cap['z'] + cap_size[2] / 2)
                self.assertLess(slope['y'] + min(point[1] for point in ridge), cap['y'] + cap_size[1])
                self.assertGreater(slope['y'] + max(point[1] for point in ridge), cap['y'])

    def test_visible_perimeter_and_campsite_tree_clearance_replace_old_forest_routes(self):
        self.assertFalse(self.level.get('void_walls', []))
        self.assertFalse(any(prop['model'] == 'outdoor:collision_peg' for prop in self.props))
        boundary = [wall for wall in self.level['walls']
                    if wall.get('material') == 'home:handrail_wood_01'
                    and wall.get('height', 0) >= 2.4 - EPSILON]
        self.assertTrue(boundary)
        rooms = self.level['rooms']
        west = min(room['x'] for room in rooms)
        east = max(room['x'] + room['width'] for room in rooms)
        north = min(room['z'] for room in rooms)
        south = max(room['z'] + room['depth'] for room in rooms)
        sides = {side: [] for side in ('north', 'south', 'west', 'east')}
        for wall in boundary:
            if wall['width'] > wall['depth']:
                if abs(wall['z'] - north) < EPSILON:
                    side = 'north'
                elif abs(wall['z'] + wall['depth'] - south) < EPSILON:
                    side = 'south'
                else:
                    self.fail(('interior perimeter wall', wall))
                sides[side].append((wall['x'], wall['x'] + wall['width']))
            else:
                if abs(wall['x'] - west) < EPSILON:
                    side = 'west'
                elif abs(wall['x'] + wall['width'] - east) < EPSILON:
                    side = 'east'
                else:
                    self.fail(('interior perimeter wall', wall))
                sides[side].append((wall['z'], wall['z'] + wall['depth']))
        lift = next(lift for lift in self.level['elevators'] if lift['id'] == 'return_hub')
        self.assertEqual(lift['rotation_degrees'], 90)
        # The east fence is split around the authored four-metre lift surround.
        # Compare occupied intervals, allowing subdivision but no missing stock
        # or extra openings elsewhere. This replaces the obsolete segment count.
        expected = {'north': [(west, east)], 'south': [(west, east)],
                    'west': [(north + .2, south - .2)],
                    'east': [(north + .2, lift['z'] - 2), (lift['z'] + 2, south - .2)]}
        for side, intervals in sides.items():
            merged = []
            for low, high in sorted(intervals):
                if merged and low <= merged[-1][1] + EPSILON:
                    merged[-1] = (merged[-1][0], max(merged[-1][1], high))
                else:
                    merged.append((low, high))
            self.assertEqual(len(merged), len(expected[side]), side)
            for actual, wanted in zip(merged, expected[side]):
                for value, endpoint in zip(actual, wanted):
                    self.assertAlmostEqual(value, endpoint, places=3, msg=side)
        for intent in self.level.get('geometry_intent', []):
            self.assertIn(intent['check'], ('missing-wall', 'room-leak'))
            if intent['check'] == 'missing-wall':
                self.assertTrue(any(room.get('ceiling', {}).get('kind') == 'open'
                                    and all(abs(intent[key] - room[key]) < EPSILON
                                            for key in ('x', 'z', 'width', 'depth'))
                                    for room in self.level['rooms']))
            else:
                self.assertTrue(intent['x'] + intent['width'] <= -24
                                or intent['x'] >= 24 or intent['z'] + intent['depth'] <= -32
                                or intent['z'] >= 18.8)
        trees = [prop for prop in self.props if prop['model'].startswith('outdoor:tree_')]
        self.assertTrue(trees)
        fire = self.fires[0]
        for tree in trees:
            self.assertTrue(tree.get('solid'))
            self.assertAlmostEqual(tree['size'][0], .6, places=3)
            self.assertAlmostEqual(tree['size'][2], .6, places=3)
            self.assertGreater(math.dist((tree['x'], tree['z']), (fire['x'], fire['z'])), 4)
            self.assertFalse(any(contains(room, tree) for room in self.homes))
        friend = next(prop for prop in self.props if prop['model'] == 'collection:skeleton_friend')
        script = (self.project / 'scripts/lantern_hollow.luau').read_text(encoding='utf-8')
        route_body = script.split('local route =', 1)[1].split('local function', 1)[0]
        route = [(friend['x'], friend['z']), *[(float(x), float(z)) for x, z in
                 re.findall(r'\{x=([-\d.]+),z=([-\d.]+)\}', route_body)]]
        self.assertGreaterEqual(len(route), 2)
        for tree in trees:
            for start, end in zip(route, route[1:]):
                self.assertGreater(distance_to_segment((tree['x'], tree['z']), start, end), .55)

    def test_jun_sequence_actor_links_and_signed_hub_return_are_preserved(self):
        friends = [prop for prop in self.props if prop['model'] == 'collection:skeleton_friend']
        self.assertEqual(len(friends), 1)
        friend = friends[0]
        self.assertTrue(any(action.get('sequence') == 'jun_sits'
                            for binding in friend['bindings'] for action in binding['actions']))
        states = {component['name']: component['value'] for component in friend['components']
                  if component['component'] == 'state'}
        self.assertEqual(states, {'seating': False, 'seated': False})
        sequence = next(sequence for sequence in self.level['sequences']
                        if sequence['id'] == 'jun_sits')
        self.assertTrue(any(step.get('action') == {'action': 'play_animation', 'clip': 'sit_down'}
                            for step in sequence['steps']))
        self.assertTrue(any(step.get('action') == {'action': 'play_animation',
                                                  'clip': 'quest_seated', 'loop': True}
                            for step in sequence['steps']))
        self.assertEqual(sequence['steps'][-1], {'step': 'set_state', 'name': 'seated', 'value': True})
        scripts = [action['script'] for binding in friend['bindings'] if binding['on'] == 'interact'
                   for action in binding['actions'] if action['action'] == 'run_script']
        self.assertEqual(scripts, ['scripts/lantern_hollow.luau'])
        self.assertIn(scripts[0], self.level['scripts'])
        script = (self.project / scripts[0]).read_text(encoding='utf-8')
        actor_links = re.findall(r'api\.(?:navigate|prompt|actor|set_state|state)\("([^\"]+)"', script)
        self.assertTrue(actor_links)
        self.assertEqual(set(actor_links), {friend['id']}, 'Every quest actor reference must be retargeted')
        self.assertIn('hub', self.level['transitions'])
        returns = [lift for lift in self.level['elevators'] if any(
            stop.get('level') == 'hub' and stop.get('arrival_elevator') == 'hub_lantern_hollow'
            for stop in lift['stops'])]
        self.assertEqual(len(returns), 1)
        self.assertTrue(any(decal['material'] == 'collection:sign_hub' for decal in self.level['decals']))


if __name__ == '__main__':
    unittest.main()
