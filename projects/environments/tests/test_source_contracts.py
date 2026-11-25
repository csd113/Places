"""Maintained level regressions that read only editor-authored sources.

These content-owner witnesses complement the native loader and geometry tests:
the native tests cannot tell whether a particular signed route, switch or
support was accidentally moved in a shipped map. No generator, bake or GPU.
"""
from collections import Counter
import json
import math
from pathlib import Path
import re
import unittest

from assets.inspection.glb import read_glb_chunks

from PIL import Image

PROJECT = Path(__file__).resolve().parents[1]
ROOT = PROJECT.parents[1]
LEVELS = PROJECT / 'levels'
ROOM_EDGE_EPS = .01
JOIN_EPS = 1e-4  # Authored f32 roundoff.
PLAYER_RADIUS = .30
PLAYER_HEIGHT = 1.8


def read(path):
    return json.loads(path.read_text(encoding='utf-8'))


def contains(piece, x, z, epsilon=0):
    return (piece.get("x", 0)-epsilon <= x <= piece.get("x", 0)+piece["width"]+epsilon
            and piece.get("z", 0)-epsilon <= z <= piece.get("z", 0)+piece["depth"]+epsilon)


def authored_floor(level, x, z):
    """LevelSurfaces authoring query, including its room edge tolerance."""
    room = next((room for room in level["rooms"] if contains(room, x, z, ROOM_EDGE_EPS)), None)
    if room is None:
        return None
    offset = 0
    for region in level.get("floor_regions", []):
        if contains(region, x, z):
            offset = region.get("offset_y", 0)
    for ramp in level.get("ramps", []):
        if contains(ramp, x, z):
            axis = "x" if ramp["width"] >= ramp["depth"] else "z"
            length = ramp["width"] if axis == "x" else ramp["depth"]
            coordinate = x if axis == "x" else z
            offset = ramp.get("offset_y", 0) + ramp["rise"]*(coordinate-ramp[axis])/length
    return room.get("floor_y", 0)+offset


def base_y(level, prop):
    support = authored_floor(level, prop["x"], prop["z"])
    return (0 if support is None else support)+prop.get("y", 0)


def collider_box(level, prop):
    width, height, depth = [v*prop.get("scale", 1) for v in prop.get("size", [.6, .9, .6])]
    yaw = math.radians(prop.get("rotation_degrees", 0))
    ex = (abs(math.cos(yaw))*width+abs(math.sin(yaw))*depth)/2
    ez = (abs(math.sin(yaw))*width+abs(math.cos(yaw))*depth)/2
    bottom = base_y(level, prop)
    return prop["x"]-ex, prop["x"]+ex, prop["z"]-ez, prop["z"]+ez, bottom, bottom+height


def disc_touches(box, x, z, radius):
    return ((x-min(max(x, box[0]), box[1]))**2
            +(z-min(max(z, box[2]), box[3]))**2 < radius**2)


def world_point(prop, x, z):
    yaw = math.radians(prop.get("rotation_degrees", 0))
    return (prop["x"]+x*math.cos(yaw)+z*math.sin(yaw),
            prop["z"]-x*math.sin(yaw)+z*math.cos(yaw))


def sample_axis(low, high, edges):
    """Sample both sides of every support edge and ordinary quarter metres."""
    values = {low, high, (low+high)/2}
    for edge in edges:
        for delta in (-ROOM_EDGE_EPS, -.001, 0, .001, ROOM_EDGE_EPS):
            point = edge+delta
            if low <= point <= high:
                values.add(point)
    count = math.ceil((high-low)/.25)
    values.update(low+(high-low)*i/count for i in range(count+1))
    return sorted(values)


class BeachRouteTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.level = read(LEVELS / 'beach.json')
        cls.props = {prop["id"]: prop for prop in cls.level["props"]}
        cls.solids = [collider_box(cls.level, prop) for prop in cls.level["props"] if prop.get("solid")]

    def test_water_never_covers_higher_support_including_shared_edges(self):
        pieces = self.level["rooms"]+self.level["floor_regions"]+self.level["ramps"]
        x_edges = [value for piece in pieces for value in (piece.get("x", 0), piece.get("x", 0)+piece["width"])]
        z_edges = [value for piece in pieces for value in (piece.get("z", 0), piece.get("z", 0)+piece["depth"])]
        failures = []
        for index, water in enumerate(self.level["water"]):
            outside_interior = (water['x']+water['width'] <= -24 or water['x'] >= 24
                                or water['z']+water['depth'] <= -24)
            if outside_interior:
                self.assertFalse(water['swimming'])
                self.assertEqual(water['opacity'], 1)
                self.assertEqual(water['bottom_y'], -2.2)
                continue  # Distant sea touches the floor perimeter behind containment.
            for x in sample_axis(water["x"], water["x"]+water["width"], x_edges):
                for z in sample_axis(water["z"], water["z"]+water["depth"], z_edges):
                    floor = authored_floor(self.level, x, z)
                    if floor is None or floor > water["surface_y"]+.010001:
                        failures.append((index, round(x, 5), round(z, 5), floor, water["surface_y"]))
                        if len(failures) >= 12:
                            break
                if len(failures) >= 12:
                    break
            if len(failures) >= 12:
                break
        self.assertEqual(failures, [], "water/support ownership conflicts: "+repr(failures))

    def test_sideways_shore_steps_fit_the_controller_step_limit(self):
        # A gentle north/south ramp can still make a tall sideways ledge when
        # adjacent coast bands have different shore coordinates.
        edges = sorted({ramp['x'] for ramp in self.level['ramps'] if ramp['x'] < 6.95})
        edges += sorted({ramp['x'] for ramp in self.level['ramps'] if ramp['x'] > 9.05})
        for x in edges:
            if x <= -24:
                continue
            for index in range(161):
                z = -12+index*.1
                left = authored_floor(self.level, x-.001, z)
                right = authored_floor(self.level, x+.001, z)
                self.assertLessEqual(abs(left-right), .4+1e-5, (x, z, left, right))

    def test_standing_portal_lanes_and_lookout_landings(self):
        for identity in ("waterfront_blue_house", "beach_brown_hut", "town_cream", "town_blue", "town_coral", "town_gate"):
            prop = self.props[identity]
            for offset in (-.15, 0, .15):
                for step in range(61):
                    x, z = world_point(prop, offset, 3-step*.05)
                    floor = authored_floor(self.level, x, z)
                    self.assertIsNotNone(floor, (identity, x, z))
                    tops = [box[5] for box in self.solids if disc_touches(box, x, z, PLAYER_RADIUS)
                            and box[5] <= floor+.4+.001]
                    feet = max([floor]+tops)
                    blockers = [box for box in self.solids if disc_touches(box, x, z, PLAYER_RADIUS)
                                and box[4] < feet+PLAYER_HEIGHT and box[5] > feet+.001]
                    self.assertEqual(blockers, [], (identity, offset, x, z, feet, blockers))
        # The stock treads/slab carry collision across the millimetre support
        # inset at the stair/terrace junction, just as the real controller does.
        stocks = [collider_box(self.level, prop) for identity, prop in self.props.items()
                  if identity.startswith('lookout_stairs_solid_') or identity == 'lookout_terrace_solid_0']
        def surface(z):
            return max([authored_floor(self.level, 12, z)] +
                       [box[5] for box in stocks if box[0] <= 12 <= box[1] and box[2] <= z <= box[3]])
        previous = surface(22.2)
        for index in range(90):
            z = 22.2-index*.05
            floor = surface(z)
            self.assertLessEqual(abs(floor-previous), .4+.001, (z, floor, previous))
            previous = floor
        self.assertAlmostEqual(authored_floor(self.level, 12, 17.8), 2.395, places=5)

    def test_crab_loop_has_dry_supported_unobstructed_translation(self):
        route = next(route for route in self.level["routes"] if route["id"] == "walking_crab")
        actor = self.props[route["id"]]
        self.assertFalse(actor.get("solid"))
        x, z = actor["x"], actor["z"]
        self.assertAlmostEqual(base_y(self.level, actor), 0, places=5)
        for step in route["steps"]:
            if step["step"] != "move_to":
                continue
            length = math.hypot(step["x"]-x, step["z"]-z)
            count = max(1, math.ceil(length/.025))
            for index in range(count+1):
                px, pz = x+(step["x"]-x)*index/count, z+(step["z"]-z)*index/count
                floor = authored_floor(self.level, px, pz)
                self.assertAlmostEqual(floor, 0, places=5)
                self.assertFalse(any(contains(water, px, pz) for water in self.level["water"]))
                # No authored body uses the runtime's .6 x .9 x .6 fallback.
                blockers = [box for box in self.solids if disc_touches(box, px, pz, .3)
                            and box[4] < floor+.9 and box[5] > floor+.001]
                self.assertEqual(blockers, [], (px, pz, blockers))
            x, z = step["x"], step["z"]
        self.assertAlmostEqual(x, actor["x"])
        self.assertAlmostEqual(z, actor["z"])


class AeroRouteTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.level = read(LEVELS / 'aero.json')
        cls.solids = [(p['id'], collider_box(cls.level, p)) for p in cls.level['props'] if p.get('solid')]

    def test_connected_circulation_has_supported_body_clearance(self):
        # These reciprocal lanes deliberately pass through each true aperture;
        # the native controller campaign separately verifies real movement.
        lanes = [((10, 6), (27, 6)), ((6, 10), (6, 16)),
                 ((6, 15), (26.5, 15)), ((26.5, 15), (26.5, 9)),
                 ((2.0, 9), (4.1, 9)), ((4.1, 9), (8.0, 9)),
                 ((27, 7), (29, 7)), ((25.2, 2), (25.2, 4.0)), ((30.8, 7), (30.8, 2))]
        failures = []
        for start, end in lanes:
            for index in range(161):
                t = index/160
                x, z = (start[axis]+(end[axis]-start[axis])*t for axis in range(2))
                floor = authored_floor(self.level, x, z)
                if floor is None:
                    failures.append(('unsupported', x, z))
                    continue
                for identity, box in self.solids:
                    if box[4] < floor+1.8 and box[5] > floor+.4 and disc_touches(box, x, z, .30):
                        failures.append((identity, x, z))
        self.assertEqual(failures, [])

    def test_fountain_has_shallow_supported_water_and_fitted_rim(self):
        water, = self.level['water']
        self.assertEqual(water['shape'], 'circle')
        self.assertFalse(water['swimming'])
        self.assertLess(water['radius'], 1.32)
        self.assertGreater(water['surface_y'], water['bottom_y'])
        self.assertEqual(authored_floor(self.level, 6, 6), 0)
        supports = [box for identity, box in self.solids if identity.startswith('fountain_floor_')]
        self.assertTrue(any(disc_touches(box, 6.9, 6.3, .01) for box in supports))
        self.assertTrue(all(abs(box[5]-.095) < 1e-7 for box in supports))
        self.assertLess(water['surface_y']-.10, .55)
        rim = [box for identity, box in self.solids if identity.startswith('atrium_fountain_solid_')]
        self.assertEqual(len(rim), 20)
        self.assertTrue(all(not disc_touches(box, 6, 6, .30) for box in rim))

    def test_hollow_architecture_never_uses_its_visual_box(self):
        hollow = {'lime_arch', 'glass_canopy', 'atrium_dome', 'double_doors_open'}
        for prop in self.level['props']:
            if prop['model'].split(':')[-1] in hollow:
                self.assertFalse(prop['solid'], prop['id'])
                # Audio capabilities do not make hollow visuals collidable.
                self.assertFalse(any(c.get('component') == 'collider'
                                     for c in prop.get('components', [])), prop['id'])
        self.assertFalse(self.level.get('animated_emissions'))


def inside_room(room, item):
    return contains(room, item['x'], item['z'], JOIN_EPS)


def overlaps(a, b):
    return (min(a[2], b[2]) - max(a[0], b[0]) > JOIN_EPS
            and min(a[3], b[3]) - max(a[1], b[1]) > JOIN_EPS)


def plan_box(level, prop):
    left, right, near, far, _, _ = collider_box(level, prop)
    return left, near, right, far


class LanternHollowQuestTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.level = read(LEVELS / 'lantern_hollow.json')
        cls.project = PROJECT
        cls.props = cls.level['props']
        cls.homes = [room for room in cls.level['rooms']
                     if room.get('ceiling', {}).get('kind') != 'open']

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


    def furniture(self, room):
        return [prop for prop in self.props if inside_room(room, prop) and prop.get('solid')
                and not prop['model'].startswith('outdoor:house_')]

    def shell(self, room):
        return [wall for wall in self.level['walls']
                if wall.get('material') == 'outdoor:house_siding_01' and inside_room(room, wall)]

    def door(self, room):
        doors = [door for door in self.level['doors'] if inside_room(room, door)]
        self.assertEqual(len(doors), 1, 'Every home retains one usable front door')
        return doors[0]

    def test_eight_furnished_homes_have_distinct_real_partition_plans(self):
        self.assertEqual(len(self.homes), 8)
        self.assertEqual(Counter(round(room['z'], 3) for room in self.homes), {-10.4: 4, 9: 4})
        plans = []
        for room in self.homes:
            self.assertGreaterEqual(len(self.furniture(room)), 5)
            shell = self.shell(room)
            self.assertEqual(len(shell), 4)
            partitions = [wall for wall in self.level['walls'] if inside_room(room, wall)
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
                box = plan_box(self.level, prop)
                self.assertGreaterEqual(box[0] + JOIN_EPS, room['x'] + .3, prop['id'])
                self.assertLessEqual(box[2] - JOIN_EPS, room['x'] + room['width'] - .3, prop['id'])
                self.assertGreaterEqual(box[1] + JOIN_EPS, room['z'] + .3, prop['id'])
                self.assertLessEqual(box[3] - JOIN_EPS, room['z'] + room['depth'] - .3, prop['id'])
                self.assertFalse(overlaps(box, approach), ('front door approach', prop['id']))
                for other in furniture[index + 1:]:
                    self.assertFalse(overlaps(box, plan_box(self.level, other)), (prop['id'], other['id']))

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

    def test_each_house_exterior_keeps_four_overlapping_wall_joins(self):
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
                    self.assertLessEqual(end['x'] - JOIN_EPS, side['x'])
                    self.assertGreaterEqual(end['x'] + end['width'] + JOIN_EPS,
                                            side['x'] + side['width'])


class ArcadeCircuitTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.assets = {entry["id"]: entry for entry in read(ROOT / "assets/catalog.json")["assets"]}
        cls.level = read(LEVELS / 'arcade.json')
        cls.party_measurements = read(ROOT / "assets/environments/arcade/party/asset_metadata.json")["models"]

    def document(self, name):
        path = ROOT / "assets" / self.assets["arcade:" + name]["model"]
        doc, _ = read_glb_chunks(path.read_bytes())
        return doc

    def party_position_accessors(self, identity):
        # These fitted exports have one untransformed mesh, so their saved
        # POSITION bounds are model-space measurements, not catalog envelopes.
        doc = self.document(identity.removeprefix("arcade:"))
        node, = doc["nodes"]
        self.assertFalse({"matrix", "translation", "rotation", "scale", "skin"} & node.keys(), identity)
        for primitive in doc["meshes"][node["mesh"]]["primitives"]:
            yield doc["materials"][primitive["material"]]["name"], doc["accessors"][primitive["attributes"]["POSITION"]]

    def test_installed_floor_surfaces_keep_their_effects_separate(self):
        # The editor offers all six shipped carpet pairs, even though the
        # authored rooms use only three. Decode twelve small PNGs, no bake.
        carpets = {r["material"] for r in self.level["rooms"][:3]}
        self.assertEqual(carpets, {"arcade:carpet_cosmic_bowl_01",
                                   "arcade:carpet_retro_swirl_01",
                                   "arcade:carpet_neon_confetti_01"})
        for family in ("cosmic_bowl", "neon_confetti", "starburst", "retro_swirl", "geometric", "arcade_grid"):
            identity = f"arcade:carpet_{family}_01"
            with self.subTest(material=identity):
                material = self.assets[identity]
                self.assertGreater(material["uv_response"], 0)
                self.assertEqual(material["tile_metres"], 4)
                self.assertNotIn("emissive", material)
                self.assertNotIn("emissive_mask", material)
                for field in ("texture", "uv_response_mask"):
                    asset = self.assets[material[field]]
                    self.assertEqual(asset["asset_type"], "texture")
                    with Image.open(ROOT / "assets" / asset["model"]) as image:
                        self.assertEqual(image.format, "PNG")
                        self.assertEqual(image.size, (256, 256), field)
                        image.load()
        restroom = self.level["rooms"][3]
        self.assertEqual(restroom["material"], "arcade:restroom_tile_01")
        self.assertEqual(self.assets[restroom["material"]].get("uv_response", 0), 0)
        trim = [b for b in self.level["baseboards"] if b["material"] == "core:pool_tile_wall_01"]
        self.assertTrue(trim)
        self.assertTrue(all(b.get("uv_projection", "wall") == "wall" for b in trim))

    def test_ordinary_models_and_fluorescent_side_print_keep_distinct_roles(self):
        ordinary = ("bowling_pin", "bowling_ball_purple", "bowling_ball_marbled",
                    "fixed_chair_red", "fixed_chair_blue", "pedestal_table",
                    "ball_rack", "ball_return", "cat_plush", "change_machine")
        for name in ordinary:
            for material in self.document(name)["materials"]:
                self.assertFalse(any(material.get("emissiveFactor", [0, 0, 0])), name)
                self.assertEqual(material.get("extras", {}).get("uv_response", 0), 0, name)
        cabinet = self.document("cabinet_scifi")
        reactive = [m for m in cabinet["materials"] if m.get("extras", {}).get("uv_response", 0)]
        self.assertEqual(len(reactive), 1)
        self.assertFalse(any(reactive[0].get("emissiveFactor", [0, 0, 0])))
        self.assertEqual(reactive[0]["extras"]["uv_response_mask"]["texCoord"], 0)
        self.assertTrue(any(any(m.get("emissiveFactor", [0, 0, 0]))
                            for m in cabinet["materials"]))

    def test_reachable_switch_controls_all_four_operational_blacklights(self):
        sources = [light for light in self.level["ceiling_lights"]
                   if light.get("ultraviolet", 0) > 0]
        self.assertEqual(len(sources), 4)
        self.assertTrue(all(light["switchable"] and light["enabled"] for light in sources))
        switch = next(p for p in self.level["props"] if p["id"] == "cosmic_blacklight_switch")
        actions = switch["bindings"][0]["actions"]
        self.assertEqual({a["target"] for a in actions if a["action"] == "toggle"},
                         {"cosmic_uv_0"})
        self.assertEqual({light["switch_group"] for light in sources}, {"cosmic"})
        self.assertTrue(any(a["action"] == "toggle_animation" for a in actions))
        self.assertFalse(switch["solid"])
        self.assertGreater(switch["components"][0]["reach"], switch["x"])

    def test_restroom_closed_doors_and_switch_use_playable_room_controls(self):
        # Small source witness: closed leaves must be interactive and the one
        # switch must address the complete real circuit in both lighting modes.
        doors = [d for d in self.level.get("doors", []) if d["id"].startswith("restroom_")]
        self.assertEqual(len(doors), 4)
        self.assertTrue(all(d["initial_state"] == "closed" and d["kind"] == "restroom" for d in doors))
        self.assertTrue(all(d["width"] >= 1.1 and d["components"][0]["component"] == "interactable" for d in doors))
        fixtures = [light for light in self.level["ceiling_lights"] if light.get("switch_group") == "restroom"]
        self.assertEqual(len(fixtures), 4)
        self.assertTrue(all(light["switchable"] and light["static_switch"] == "prepared" for light in fixtures))
        switch = next(p for p in self.level["props"] if p["id"] == "restroom_light_switch")
        targets = {a["target"] for a in switch["bindings"][0]["actions"] if a["action"] == "toggle"}
        self.assertEqual(targets, {fixtures[0]["id"]})
        self.assertTrue(-6.6 < switch["x"] < 0 and 0 < switch["z"] < 7.8)
        # The ceiling replacement must use real static receivers, not fixed
        # decal colours. Two source placements and one small GLB material record
        # protect circuit response, mounting and non-emission without a map bake.
        vents = [p for p in self.level["props"] if p.get("model") == "core:ceiling_diffuser_01"]
        self.assertEqual({(p["x"], p["z"]) for p in vents}, {(-5.1, 6.9), (-1.5, .9)})
        self.assertTrue(all(p["size"] == [.6, .065, .6] and not p["solid"] for p in vents))
        self.assertTrue(all(math.isclose(p["y"] + p["size"][1], 3.2) for p in vents))
        self.assertFalse(any(d["material"] == "core:decal_ceiling_vent_01" for d in self.level["decals"]))
        path = ROOT / "assets" / self.assets["core:ceiling_diffuser_01"]["model"]
        document, _ = read_glb_chunks(path.read_bytes())
        self.assertFalse(any(any(m.get("emissiveFactor", [0, 0, 0])) for m in document["materials"]))
        self.assertTrue(all("baseColorTexture" in m["pbrMetallicRoughness"] for m in document["materials"]))


    def test_party_room_preserves_a_clear_three_room_walking_route(self):
        # This source-only witness catches blocked passages/approaches without
        # compiling or baking a production map. About 370 samples and the
        # authored collision boxes are involved; no renderer/assets are loaded.
        self.assertEqual(len(self.level["rooms"]), 4)
        # The small authored trim profile adds no colliders and must leave both
        # sides of each passage completely bare. This protects the new join
        # treatment with only source rectangles, adding no native/bake cost.
        boards = [b for b in self.level["baseboards"] if b.get("uv_projection") == "floor_fold"]
        self.assertTrue(boards)
        self.assertEqual(len(boards) % 2, 0)
        self.assertEqual({b["material"] for b in boards},
                         {room["material"] for room in self.level["rooms"][:3]})
        for board in boards:
            self.assertEqual(board["uv_projection"], "floor_fold")
            angle = math.radians(board["rotation_degrees"])
            for fraction in (.001, .5, .999):
                x = board["x"]+math.cos(angle)*board["length"]*fraction + math.sin(angle)*.020
                z = board["z"]-math.sin(angle)*board["length"]*fraction + math.cos(angle)*.020
                room = next(r for r in self.level["rooms"] if
                            r["x"] <= x <= r["x"]+r["width"] and r["z"] <= z <= r["z"]+r["depth"])
                floor_material = room["material"]
                for patch in [*self.level["floor_patches"], *self.level["floor_regions"]]:
                    if patch["x"] <= x <= patch["x"]+patch["width"] and patch["z"] <= z <= patch["z"]+patch["depth"]:
                        floor_material = patch.get("material", room["material"])
                self.assertEqual(board["material"], floor_material)
            material = self.assets[board["material"]]
            self.assertGreater(material["uv_response"], 0)
            self.assertEqual(material["tile_metres"], 4)
            self.assertNotIn("emissive", material)
        for body, cap in zip(boards[::2], boards[1::2]):
            for field in ("x", "z", "length", "rotation_degrees", "material"):
                self.assertEqual(body[field], cap[field])
            self.assertAlmostEqual(body["y"]+body["height"], cap["y"])
            self.assertGreaterEqual(body["height"], .01)
            self.assertGreaterEqual(cap["height"], .01)
            self.assertGreater(body["thickness"], cap["thickness"])
            self.assertLess(body["height"]+cap["height"], .12)
            self.assertNotIn("solid", body)
            self.assertNotIn("solid", cap)
        for wall in self.level["walls"]:
            horizontal = wall["width"] >= wall["depth"]
            across, along = ("z", "x") if horizontal else ("x", "z")
            extent = wall["depth"] if horizontal else wall["width"]
            for opening in wall.get("openings", []):
                if opening.get("sill", 0) > 0:
                    continue
                lo = wall[along]+opening["offset"]
                hi = lo+opening["width"]
                for board in boards:
                    if not any(abs(board[across]-face) < 1e-6
                               for face in (wall[across], wall[across]+extent)):
                        continue
                    angle = math.radians(board["rotation_degrees"])
                    direction = math.cos(angle) if horizontal else -math.sin(angle)
                    end = board[along]+direction*board["length"]
                    low, high = sorted((board[along], end))
                    self.assertTrue(high <= lo+1e-6 or low >= hi-1e-6, board)
        # North/south stock butts clear of the perpendicular strip's front,
        # rather than creating overlapping corner caps sensitive to 90-degree
        # floating-point drift. Passage ends are kept at their true jambs.
        for board in boards:
            if board["rotation_degrees"] not in (90, 270):
                continue
            sign = -1 if board["rotation_degrees"] == 90 else 1
            low, high = sorted((board["z"], board["z"]+sign*board["length"]))
            self.assertGreaterEqual(low, .01999)
            self.assertLessEqual(high, 29.98001)
        party = self.level["rooms"][2]
        party_props = [p for p in self.level["props"] if p["id"].startswith("party_")]
        for prop in party_props:
            self.assertTrue(party["x"] <= prop["x"] <= party["x"]+party["width"], prop["id"])
            self.assertTrue(party["z"] <= prop["z"] <= party["z"]+party["depth"], prop["id"])
        boxes = []
        for wall in self.level["walls"]:
            horizontal = wall["width"] >= wall["depth"]
            near = wall["x"] if horizontal else wall["z"]
            end = near + (wall["width"] if horizontal else wall["depth"])
            spans = []
            for opening in wall.get("openings", []):
                # Raised opaque mirror panes retain the wall's floor blocker.
                if opening.get("sill", 0) > 0:
                    continue
                self.assertEqual(opening.get("sill", 0), 0)
                self.assertGreaterEqual(opening["height"], 1.8)
                spans.append((near, (wall["x"] if horizontal else wall["z"])+opening["offset"]))
                near = spans[-1][1] + opening["width"]
            spans.append((near, end))
            for lo, hi in spans:
                boxes.append((lo, wall["z"], hi, wall["z"]+wall["depth"]) if horizontal
                             else (wall["x"], lo, wall["x"]+wall["width"], hi))
        for prop in self.level["props"]:
            if not prop["solid"] or prop["y"] >= 1.8:
                continue
            boxes.append(plan_box(self.level, prop))
        routes = [((6.8, 28), (6.8, 25.2), (18.4, 25.2), (21, 25.4), (25.9, 25.4)),
                  ((21, 25.4), (21, 23.2), (25.9, 23.2), (25.9, 19.8), (26.6, 19.8)),
                  ((21, 25.4), (23.4, 26.1)),
                  # The added route uses the same inexpensive body-clearance
                  # witness to prevent a combined stall collider sealing a bay.
                  ((2.0,2.8),(-2.8,2.8),(-2.8,6.85),(-1.1,6.85)),
                  ((-2.8,2.8),(-5.2,2.8)),
                  ((-2.8,2.8),(-2.8,4.85),(-5.2,4.85)),
                  ((-2.8,4.85),(-3.35,4.85),(-3.35,7.05),(-5.2,7.05))]
        for route in routes:
            for start, end in zip(route, route[1:]):
                steps = math.ceil(math.dist(start, end)/.1)
                for step in range(steps+1):
                    t = step/steps
                    x, z = (start[0]+(end[0]-start[0])*t, start[1]+(end[1]-start[1])*t)
                    self.assertTrue(any(r["x"] <= x <= r["x"]+r["width"] and
                                        r["z"] <= z <= r["z"]+r["depth"]
                                        for r in self.level["rooms"]), (x, z))
                    self.assertFalse(any(lo_x-.30 < x < hi_x+.30 and lo_z-.30 < z < hi_z+.30
                                         for lo_x, lo_z, hi_x, hi_z in boxes), (x, z))

    def test_bouquets_share_fixed_anchors_and_allow_sway_without_interpenetration(self):
        # A bounded placement-envelope check protects the authored spacing,
        # independent phases and roof clearance, complementing runtime motion
        # tests. It reads 17 source entities rather than evaluating frame poses.
        bouquets = {}
        for prop in self.level["props"]:
            component = next((c for c in prop.get("components", []) if c["component"] == "balloon"), None)
            if component is None:
                continue
            key = prop["id"].rsplit("_", 1)[0]
            bouquets.setdefault(key, []).append((prop, component))
        self.assertEqual(sorted(map(len, bouquets.values())), [1, 1, 3, 5, 7])
        for bouquet in bouquets.values():
            self.assertEqual(len({(p["x"], p["y"], p["z"], tuple(c["anchor"]))
                                  for p, c in bouquet}), 1)
            if len(bouquet) > 1:
                self.assertGreater(len({c["phase"] for _, c in bouquet}), 1)
                prop, component = bouquet[0]
                weight_id = prop["id"].rsplit("_", 1)[0]+"_weight"
                self.assertEqual(component["anchor"], [0, 0, 0], weight_id)
                weight = next(p for p in self.level["props"] if p["id"] == weight_id)
                self.assertEqual(weight["model"], "arcade:party_balloon_weight")
                self.assertEqual((weight["x"], weight["z"]), (prop["x"], prop["z"]))
                anchor_y = self.party_measurements[weight["model"]]["anchor_height"]
                self.assertAlmostEqual(prop["y"]-weight["y"], anchor_y * weight.get("scale", 1))
                weight_top = max(a["max"][1] for _, a in self.party_position_accessors(weight["model"]))
                self.assertAlmostEqual(weight_top, anchor_y, places=5)
                table_id = weight_id.rsplit("_bouquet", 1)[0]
                cloth = next(p for p in self.level["props"] if p["id"] == table_id+"_cloth")
                cloth_top = max(a["max"][1] for _, a in self.party_position_accessors(cloth["model"]))
                self.assertAlmostEqual(cloth_top, self.party_measurements[cloth["model"]]["bounds_max"][1], places=5)
                surface_y = cloth["y"] + cloth_top * cloth.get("scale", 1)
                self.assertAlmostEqual(weight["y"], surface_y, places=5)
                self.assertGreater(prop["y"], surface_y)
            for prop, component in bouquet:
                self.assertFalse(prop["solid"])
                self.assertLess(prop["y"]+component["string_length"] +
                                .43*component["scale"]+component["bob_amplitude"], 3.0)
        for bouquet in bouquets.values():
            self.assertEqual(len({(c["shared_phase"], c["shared_sway"])
                                  for _, c in bouquet}), 1)
            def envelope_radius(balloon):
                tilt = math.sin(math.radians(balloon["rotation_degrees"]))
                return .15*balloon["scale"] + math.sqrt(2)*( \
                    balloon["sway_amplitude"] + .43*balloon["scale"]*tilt)
            for index, (_, component) in enumerate(bouquet):
                for _, other in bouquet[index+1:]:
                    separation = math.dist(component["offset"], other["offset"])
                    # Both horizontal sway axes and both body-tilt axes may
                    # approach the neighbour. Shared sway cancels within a
                    # bouquet because its phase and amplitude are identical.
                    envelope = envelope_radius(component)+envelope_radius(other)
                    self.assertGreater(separation, envelope)

    def test_authored_party_gifts_and_shared_candle_light(self):
        # Test the shipped placements, not the retired layout generator.
        gifts = [p for p in self.level["props"] if p["model"].startswith("arcade:party_gift_")]
        models = {p["model"] for p in gifts}
        designs = ("stars", "confetti", "stripes", "arcade", "red", "green")
        self.assertEqual(models, {f"arcade:party_gift_{design}" for design in designs})
        self.assertTrue(all(p["scale"] == 1 for p in gifts))
        for design in designs:
            identity = f"arcade:party_gift_{design}"
            with self.subTest(model=identity):
                # The authored 30 cm cube excludes the overhanging lid and bow.
                self.assertEqual(self.party_measurements[identity]["box_size"], [.3, .3, .3])
                paper, = (a for m, a in self.party_position_accessors(identity) if m == f"party_gift_paper_{design}")
                for axis in (0, 2):
                    self.assertAlmostEqual(paper["max"][axis] - paper["min"][axis], .3, places=5)
        cake = next(p for p in self.level["props"] if p["model"] == "arcade:party_cake_lit")
        glows = [c for c in cake["components"] if c["component"] == "glow"]
        self.assertEqual(len(glows), 1)
        self.assertTrue(glows[0]["enabled"])
        self.assertGreater(glows[0]["intensity"], 0)
        self.assertLessEqual(glows[0]["range"], .85)
        self.assertNotIn("lights", cake)
        start = next(timer for timer in self.level["timers"] if timer["autostart"])
        self.assertFalse(start["repeat"])
        self.assertEqual(start["bindings"][0]["actions"], [
            dict(action="play_animation", target=cake["id"], clip="candle_flicker", loop=True)])


class OfficeCircuitTests(unittest.TestCase):
    def test_fluorescent_backplates_clear_their_local_ceiling(self):
        # One JSON parse protects the real ceiling/backplate z-fighting bug;
        # no mesh build, lighting bake or native process is needed.
        level = read(LEVELS / 'office.json')
        for light in level["ceiling_lights"]:
            if light["fixture"] != "core:fluorescent_panel_01":
                continue
            with self.subTest(light=light["id"]):
                ceilings = [room.get("floor_y", 0) + room["height"]
                    for room in level["rooms"]
                    if room["x"] <= light["x"] <= room["x"] + room["width"]
                    and room["z"] <= light["z"] <= room["z"] + room["depth"]
                    and room.get("floor_y", 0) + room["height"] >= light["y"]]
                self.assertTrue(ceilings, "fixture must have a local ceiling")
                clearance = min(ceilings) - light["y"]
                self.assertGreater(clearance, .005, "backplate cannot share the roof plane")
                self.assertLess(clearance, .02, "retain the documented flush mounting")

    def test_drinks_fascia_and_lcd_emit_without_lighting_the_metal_housing(self):
        # One saved mesh and source JSON protect the new LCD's power wiring;
        # the native acceptance checks actual switching and nearby illumination.
        data = (ROOT / 'assets/environments/office/props/vending_machine.glb').read_bytes()
        document, _ = read_glb_chunks(data)
        materials = document["materials"]
        emitting = {slot for slot, material in enumerate(materials)
                    if any(material.get("emissiveFactor", []))}
        self.assertEqual(emitting, {4, 6})
        self.assertEqual(materials[4]["name"], "Office_vending_mountain_art")
        self.assertEqual(materials[6]["name"], "DRINKS_07_LCD_EMISSION")
        self.assertTrue(all(material.get("alphaMode", "OPAQUE") == "OPAQUE"
                            for material in materials))
        level = read(LEVELS / 'office.json')
        for machine in (p for p in level["props"] if p["id"] in
                        ("top_vending", "top_snack_vending")):
            self.assertEqual(machine["switch_group"], "break_room_vending")
            self.assertIn(dict(component="light", enabled=True, switchable=True,
                               emission_scale=1), machine["components"])
            self.assertEqual(len(machine["lights"]), 2)
        drinks = next(p for p in level["props"] if p["id"] == "top_vending")
        self.assertEqual(set(drinks["emission_overrides"]), {"4"})


class WinterClimateTests(unittest.TestCase):
    def test_ground_and_interiors_partition_the_entire_world(self):
        rooms = read(LEVELS / 'winter.json')['rooms']
        self.assertAlmostEqual(sum(r['width'] * r['depth'] for r in rooms), 48 * 60)
        for index, a in enumerate(rooms):
            for b in rooms[index + 1:]:
                dx = min(a['x'] + a['width'], b['x'] + b['width']) - max(a['x'], b['x'])
                dz = min(a['z'] + a['depth'], b['z'] + b['depth']) - max(a['z'], b['z'])
                self.assertFalse(dx > 1e-6 and dz > 1e-6, (a, b))

    def test_ice_has_real_albedo_traction_backing_and_clear_shore_access(self):
        assets = {a['id']: a for a in read(ROOT / 'assets/catalog.json')['assets']}
        ice = assets['winter:ice_01']
        self.assertEqual(ice['ground_surface'], 'ice')
        self.assertEqual(ice['alpha_mode'], 'blend')
        self.assertGreater(ice['opacity'], .7)
        self.assertLess(ice['opacity'], 1)
        # Stage 5's restrained probe response is independent of physical traction.
        self.assertEqual(ice['reflection_mode'], 'probe')
        self.assertEqual(ice['reflection_strength'], .28)
        self.assertEqual(assets[ice['texture']]['model'], 'environments/winter/textures/floors/ice_01.png')
        level = read(LEVELS / 'winter.json')
        backing = next(b for b in level['void_walls'] if b['id'] == 'pond_ice_depth')
        self.assertFalse(backing['solid'])
        self.assertLess(backing['max'][1], -.16)
        self.assertEqual(sum(p['id'].startswith('pond_shore_snow_') for p in level['props']), 6)
        # The approach at z=-10 is not buried beneath snow props.
        self.assertFalse(any(p['id'].startswith('pond_shore_snow_') and abs(p['z'] + 10) < 1
                             for p in level['props']))

    def test_frozen_pond_and_exterior_geometry_waivers_stay_local(self):
        level = read(LEVELS / 'winter.json')
        self.assertFalse(level.get('water'), "the frozen pond must not trigger swimming")
        models = {prop['model'] for prop in level['props']}
        self.assertTrue({'winter:tree_snow_01', 'winter:tree_snow_02'} <= models)
        for intent in level['geometry_intent']:
            if intent['check'] == 'room-leak':
                self.assertTrue(intent['x'] + intent['width'] <= -24 or intent['x'] >= 24
                                or intent['z'] + intent['depth'] <= -40 or intent['z'] >= 20,
                                "an exterior waiver must not conceal a walkable room leak")

    def test_weather_is_only_opted_in_by_winter(self):
        manifest = read(PROJECT / 'places.project.json')
        sources = [PROJECT / member['source'] for member in manifest['levels']]
        catalog = read(ROOT / 'assets/catalog.json')
        ice_materials = {entry['id'] for entry in catalog['assets'] if entry.get('ground_surface') == 'ice'}
        self.assertTrue(sources, 'maintained level sources must exist')
        for path in sources:
            level = read(path)
            if level['id'] == 'winter':
                self.assertEqual(level['weather'], {'kind': 'snow'})
            else:
                for field in ('weather', 'weather_alternate', 'weather_cycle'):
                    self.assertNotIn(field, level, path)
                sky = level.get('sky')
                themed_sky = {'beach': 'beach:tex_sky_day_01', 'aero': 'frutiger_aero:tex_sky_day_01'}
                if level['id'] in themed_sky:
                    self.assertEqual(sky['texture'], themed_sky[level['id']], path)
                elif sky:
                    self.assertEqual(sky['texture'], 'outdoor:tex_sky_stars_01', path)
                # Decorative Winter trim is allowed; walkable non-Winter
                # surfaces must not inherit ice traction.
                ground = {level.get('defaults', {}).get('floor')}
                for key in ('rooms', 'floor_regions', 'ramps', 'stairs'):
                    ground.update(surface.get('material') for surface in level.get(key, []))
                self.assertFalse(ground & ice_materials, path)

    def test_sheet_is_full_uv_square_with_zero_edge_alpha(self):
        catalog = {a['id']: a for a in read(ROOT / 'assets/catalog.json')['assets']}
        material = catalog['core:snowflake_01']
        self.assertEqual(material['alpha_mode'], 'blend')
        self.assertEqual(material['opacity'], 1)
        self.assertNotIn('emissive', material)
        with Image.open(ROOT / 'assets' / catalog[material['texture']]['model']) as image:
            width, height = image.size
            rgba = image.convert('RGBA').tobytes()
        self.assertEqual((width, height), (256, 256))
        alpha = rgba[3::4]
        for y in range(height):
            for x in range(width):
                if x in (0, width - 1) or y in (0, height - 1):
                    self.assertEqual(alpha[y * width + x], 0)
        self.assertGreater(max(alpha), 200)
        self.assertGreater(sum(a == 0 for a in alpha), width * height // 3)


class CollectionPlayabilityTests(unittest.TestCase):
    def test_the_collection_contains_the_hub_and_every_environment(self):
        self.assertEqual((ROOT / 'levels/lineup.txt').read_text(encoding='utf-8').splitlines(), ["environments"])
        project = read(PROJECT / 'places.project.json')
        shipped = {
            entry["id"]: read(PROJECT / entry["source"])
            for entry in project["levels"]
        }
        self.assertEqual(
            set(shipped),
            {"hub", "home", "pool", "office", "lantern_hollow", "winter", "beach", "aero", "arcade"},
            "the hub and eight environments are the complete editable collection",
        )
        self.assertEqual(
            {Path(row["source"]).stem for row in project["levels"]}, set(shipped),
            "the curated lineup must contain only the Quiet Collection",
        )
        showcase = shipped["lantern_hollow"]
        self.assertEqual(showcase["id"], "lantern_hollow")
        self.assertEqual(showcase["name"], "Lantern Hollow")
        self.assertEqual(showcase["format_version"], 1)

        # Protect every authored elevator route and its reciprocal arrival cabin
        # using nine small sources; no package bake or native launch is needed.
        self.assertEqual(project["entry_level"], "hub")
        self.assertTrue(project["menu_entry_only"])
        destinations = set(shipped) - {"hub"}
        for identity, level in shipped.items():
            expected = destinations if identity == "hub" else {"hub"}
            self.assertEqual(set(level["transitions"]), expected, identity)
            links = []
            for lift in level.get("elevators", []):
                self.assertTrue(any(not stop.get("level") and not stop.get("arrival_elevator")
                                    for stop in lift["stops"]), lift["id"])
                for stop in lift["stops"]:
                    if not stop.get("level"):
                        continue
                    destination = stop["level"]
                    self.assertIn(destination, expected, lift["id"])
                    arrivals = [arrival for arrival in shipped[destination].get("elevators", [])
                                if arrival["id"] == stop.get("arrival_elevator")]
                    self.assertEqual(len(arrivals), 1, (identity, lift["id"], stop))
                    self.assertTrue(any(back.get("level") == identity
                                        and back.get("arrival_elevator") == lift["id"]
                                        for back in arrivals[0]["stops"]), (identity, stop))
                    links.append(destination)
            self.assertCountEqual(links, expected, identity)

    def test_home_collection_uses_domestic_assets_and_resettable_portable_objects(self):
        # One source parse protects the redesigned room set and real portable
        # profiles. Generic architecture samplers no longer define Home's layout.
        path = (LEVELS / 'home.json')
        level = read(path)
        self.assertGreaterEqual(len(level["rooms"]), 5)
        models = {prop['model'] for prop in level['props']}
        self.assertTrue({'home:sofa', 'home:armchair', 'home:coffee_table',
                         'home:dining_table', 'home:sink', 'home:kettle',
                         'home:cabinet_base', 'home:cabinet_wall'} <= models)
        requested = next(prop for prop in level['props'] if prop['id'] == 'home_requested_cushion')
        self.assertEqual(requested['model'], 'home:cushion')
        self.assertTrue(requested['physics']['pickup'])
        self.assertEqual(requested['physics']['reset_group'], 'home_objects')
        self.assertTrue(any(prop['id'] == 'home_restore' and
                            any(component['component'] == 'interactable'
                                for component in prop.get('components', []))
                            for prop in level['props']))

    def test_the_pool_showcase_is_real_lowered_floor_geometry(self):
        level = read((LEVELS / 'pool.json'))
        regions = level.get("floor_regions", [])
        self.assertTrue(regions, "the pool basin must be a floor region, not a prop")
        offsets = [float(region.get("offset_y", 0.0)) for region in regions]
        self.assertTrue(any(offset <= -1.0 for offset in offsets), "no recessed basin")
        self.assertTrue(any(-0.4 < offset < 0 for offset in offsets), "no walkable entry step")
        # Every region is tiled, not left on the Office default.
        supports = sorted((region for region in regions
                           if region.get("edge_material") == "core:pool_tile_basin_01"),
                          key=lambda region: region["z"])
        for region in regions:
            self.assertEqual(region.get("material"), "core:pool_tile_basin_01", region)
            if region not in supports:
                self.assertEqual(region.get("edge_material"), "core:pool_tile_wall_01", region)
        # One existing JSON parse protects the new model/support Y interface:
        # six small collision surfaces must stay concealed below real treads.
        # No model build, full-map preparation or native process is needed.
        self.assertEqual(len(supports), 6)
        steps = sorted((prop for prop in level["props"]
                        if prop["model"] == "pool:pool_entry_steps"), key=lambda prop: prop["z"])
        self.assertEqual(len(steps), 2)
        for index, support in enumerate(supports):
            with self.subTest(tread=index):
                self.assertAlmostEqual(support["x"], 9.105)
                self.assertAlmostEqual(support["z"], 5.5 + index / 3, places=6)
                self.assertAlmostEqual(support["width"], 1.19)
                self.assertAlmostEqual(support["depth"], 1 / 3, places=6)
                self.assertAlmostEqual(support["offset_y"], -1.305 + index * .2)
                module_index = index // 3
                module = steps[module_index]
                self.assertAlmostEqual(module["x"], 9.7)
                self.assertAlmostEqual(module["z"], 6 + module_index)
                self.assertEqual(module["rotation_degrees"], 180)
                self.assertFalse(module["solid"], "one bounds box would seal the step approach")
                base_y = supports[module_index * 3 + 1]["offset_y"] + module["y"]
                self.assertAlmostEqual(base_y, -1.5 + module_index * .6)
                self.assertAlmostEqual(support["offset_y"], base_y + (index % 3 + 1) * .2 - .005)
        # The walls put the sign on the deck and the fixtures in the room.
        materials = {decal.get("material") for decal in level.get("decals", [])}
        self.assertIn("core:decal_no_diving_01", materials)
        fixtures = {light.get("fixture") for light in level.get("ceiling_lights", [])}
        self.assertIn("core:pool_light_round", fixtures)
        self.assertIn("core:pool_light_wall", fixtures)
        # Wall fixtures carry a mount and a world height.
        for light in level.get("ceiling_lights", []):
            if light.get("fixture") == "core:pool_light_wall":
                self.assertEqual(light.get("mount"), "wall", light)
                self.assertIn("y", light, light)
