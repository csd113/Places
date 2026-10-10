"""Independent Beach source checks for support, water and actual prop bases.

These inspect the committed source without compiling a map or invoking a GPU.
The controller/native route campaign remains a separate integration gate.
"""
import json
import math
import os
from pathlib import Path
import subprocess
import sys
import unittest


ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "assets/levels/beach_demo.json"
ROOM_EDGE_EPS = .01
PLAYER_RADIUS = .30
PLAYER_HEIGHT = 1.8


def contains(piece, x, z, epsilon=0):
    return (piece.get("x", 0)-epsilon <= x <= piece.get("x", 0)+piece["width"]+epsilon
            and piece.get("z", 0)-epsilon <= z <= piece.get("z", 0)+piece["depth"]+epsilon)


def authored_floor(level, x, z):
    """LevelSurfaces authoring query, including its room edge tolerance."""
    room = next((room for room in level["rooms"] if contains(room, x, z, ROOM_EDGE_EPS)), None)
    if room is None:
        return None
    offset = 0
    for region in level["floor_regions"]:
        if contains(region, x, z):
            offset = region.get("offset_y", 0)
    for ramp in level["ramps"]:
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


class BeachDemoTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.level = json.loads(SOURCE.read_text())
        cls.props = {prop["id"]: prop for prop in cls.level["props"]}
        cls.solids = [collider_box(cls.level, prop) for prop in cls.level["props"] if prop.get("solid")]

    def test_authoritative_generator_is_current(self):
        result = subprocess.run([sys.executable, str(ROOT / "tools/levels/build_beach.py"), "--check"],
                                cwd=ROOT, env={**os.environ, "PYTHONDONTWRITEBYTECODE": "1"},
                                text=True, capture_output=True, check=False)
        self.assertEqual(result.returncode, 0, result.stdout+result.stderr)

    def test_complete_beach_model_coverage_and_unique_placements(self):
        catalog = json.loads((ROOT / "assets/catalog.json").read_text())
        expected = {entry["id"] for entry in catalog["assets"]
                    if entry.get("theme") == "beach" and entry["asset_type"] in ("prop", "entity")}
        present = {prop["model"] for prop in self.level["props"]}
        self.assertTrue(expected)
        self.assertEqual(expected-present, set())
        self.assertEqual(len(self.props), len(self.level["props"]))

    def test_continuous_seabed_plane_and_nonoverlapping_ramp_support(self):
        rooms = self.level["rooms"]
        self.assertEqual(len(rooms), 1)
        for room in rooms:
            self.assertEqual(room["ceiling"]["kind"], "open")
            self.assertEqual(room["floor_y"], -2.2)
            self.assertGreater(room["width"], 0)
            self.assertGreater(room["depth"], 0)
            self.assertGreaterEqual(room["x"], -24)
            self.assertGreaterEqual(room["z"], -24)
            self.assertLessEqual(room["x"]+room["width"], 24)
            self.assertLessEqual(room["z"]+room["depth"], 28)
        x_edges = sorted({-24, 24} | {edge for room in rooms
                         for edge in (room["x"], room["x"]+room["width"])})
        z_edges = sorted({-24, 28} | {edge for room in rooms
                         for edge in (room["z"], room["z"]+room["depth"])})
        # Every arrangement cell has exactly one owner: this proves the full
        # 48x52m footprint has neither gaps nor overlapping room interiors.
        for x0, x1 in zip(x_edges, x_edges[1:]):
            for z0, z1 in zip(z_edges, z_edges[1:]):
                owners = [room for room in rooms
                          if contains(room, (x0+x1)/2, (z0+z1)/2)]
                self.assertEqual(len(owners), 1, (x0, x1, z0, z1))
        self.assertAlmostEqual(sum(room["width"]*room["depth"] for room in rooms),
                               48*52, places=7)
        pieces = self.level["ramps"]+self.level["floor_regions"]
        for index, ramp in enumerate(self.level["ramps"]):
            run = max(ramp["width"], ramp["depth"])
            self.assertLessEqual(abs(ramp["rise"])/run, 2)
            for other in pieces[index+1:]:
                overlap_x = min(ramp["x"]+ramp["width"], other["x"]+other["width"])-max(ramp["x"], other["x"])
                overlap_z = min(ramp["z"]+ramp["depth"], other["z"]+other["depth"])-max(ramp["z"], other["z"])
                self.assertFalse(overlap_x > 1e-7 and overlap_z > 1e-7, (ramp, other))

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

    def test_coast_joins_keep_upper_sand_risers_below_one_centimetre(self):
        # A legally walkable shore can still draw conspicuous transverse sand
        # bars. At an uncut join, both supports are affine along Z, so their
        # greatest separation is at an overlap endpoint; no dense grid is needed.
        coastal = [ramp for ramp in self.level['ramps']
                   if ramp.get('material') == 'beach:sand_01']
        checked = {1.6: 0, .6: 0}
        for rise, limit in ((1.6, .025), (.6, .009375)):
            ramps = [ramp for ramp in coastal if abs(ramp['rise']-rise) < 1e-7]
            starts = {ramp['x']: ramp for ramp in ramps}
            for left in ramps:
                edge = left['x']+left['width']
                right = starts.get(edge)
                if right is None or abs(left['width']-right['width']) > 1e-7:
                    continue  # The pier's disjoint cuts have separate contracts.
                near = max(left['z'], right['z'])
                far = min(left['z']+left['depth'], right['z']+right['depth'])
                if near >= far:
                    continue
                for z in (near, far):
                    a = authored_floor(self.level, edge-1e-6, z)
                    b = authored_floor(self.level, edge+1e-6, z)
                    self.assertIsNotNone(a, (edge, z))
                    self.assertIsNotNone(b, (edge, z))
                    self.assertLessEqual(abs(a-b), limit+1e-5,
                                         ('transverse sand riser', rise, edge, z, a, b))
                checked[rise] += 1
        self.assertGreaterEqual(checked[1.6], 50, 'The actual lower coast joins must be checked')
        self.assertGreaterEqual(checked[.6], 50, 'The actual upper coast joins must be checked')

    def test_scenic_outside_room_props_keep_their_world_bases(self):
        # PropDef::solid_collider and rendered entities use floor.unwrap_or(0).
        # The seabed reference is not a synthetic floor beyond the room.
        outside = [prop for prop in self.level["props"]
                   if authored_floor(self.level, prop["x"], prop["z"]) is None]
        self.assertTrue(outside)
        for prop in outside:
            if prop["id"].startswith("inland_ridge_"):
                expected = -.1
            elif prop["id"].startswith("headland_"):
                expected = -1.5 if prop["z"] < 3 else -.1
            elif prop["id"] == "garden_bank":
                expected = -.1
            else:
                self.fail("Document the outside-room world base for "+prop["id"])
            self.assertAlmostEqual(base_y(self.level, prop), expected, places=5, msg=prop["id"])

    def test_module_and_proxy_bases_reconstruct_real_decks(self):
        visual_bases = {"pier_land": -.7, "pier_sea": -.7, "pier_outer": -.7, "pier_head": -.7,
                        "pier_kiosk": .23, "waterfront_blue_house": -.7, "beach_brown_hut": -.015,
                        "lookout_stairs": 0, "lookout_terrace": 2.22, "cove_sea_arch": -1.6,
                        "town_cream": 0, "town_blue": 0, "town_coral": 0, "town_gate": 0}
        for identity, expected in visual_bases.items():
            self.assertAlmostEqual(base_y(self.level, self.props[identity]), expected, places=5, msg=identity)
        deck_tops = {identity+"_solid_4": .23 for identity in ("pier_land", "pier_sea", "pier_outer", "pier_head")}
        deck_tops.update(waterfront_blue_house_solid_6=.36, beach_brown_hut_solid_6=.305,
                         lookout_terrace_solid_0=2.4)
        deck_tops.update({f"lookout_stairs_solid_{i}": (i+1)*.2 for i in range(12)})
        for identity, top in deck_tops.items():
            self.assertAlmostEqual(collider_box(self.level, self.props[identity])[5], top, places=5, msg=identity)

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

    def test_entry_ramps_clear_the_raised_deck_underside_before_contact(self):
        # The controller tests overhead clearance at the pre-step feet, so
        # a floating deck cannot be approached from flat sand as a lone step.
        for identity, x in (("waterfront_blue_house_solid_6", 12),
                            ("beach_brown_hut_solid_6", 21)):
            deck = collider_box(self.level, self.props[identity])
            first_contact_z = deck[3]+PLAYER_RADIUS
            self.assertGreater(authored_floor(self.level, x, first_contact_z), deck[4]+.001,
                               identity)
            self.assertAlmostEqual(authored_floor(self.level, x, deck[3]-.02),
                                   deck[5]-.005, places=5, msg=identity)

    def test_kiosk_entry_step_meets_its_supported_deck(self):
        step = collider_box(self.level, self.props['pier_kiosk_solid_5'])
        deck_floor = authored_floor(self.level, 8, -8.3)
        self.assertLess(step[4], deck_floor+.001)
        self.assertLessEqual(step[5]-deck_floor, .4)
        self.assertAlmostEqual(step[5], .49, places=5)

    def test_containment_encloses_playable_floor_without_ground_gaps(self):
        bounds = {name: collider_box(self.level, self.props["boundary_"+name])
                  for name in ("west", "east", "south", "swim_limit")}
        room = self.level["rooms"][0]
        highest = max(room.get("floor_y", 0)+r.get("offset_y", 0) for r in self.level["floor_regions"])
        for name, box in bounds.items():
            self.assertLess(box[4], room["floor_y"], name)
            self.assertGreater(box[5], (highest if name != "swim_limit" else 0)+1, name)
        for side in ("west", "east"):
            self.assertLessEqual(bounds[side][2], bounds["swim_limit"][3])
            self.assertGreaterEqual(bounds[side][3], bounds["south"][2])
        x, z = self.level["spawn"]["x"], self.level["spawn"]["z"]
        self.assertIsNotNone(authored_floor(self.level, x, z))
        self.assertTrue(all(not disc_touches(box, x, z, PLAYER_RADIUS) for box in self.solids))

    def test_buried_terrain_does_not_raise_an_accessible_visual_floor(self):
        catalog = json.loads((ROOT / "assets/catalog.json").read_text())
        sizes = {entry["id"]: entry["size"] for entry in catalog["assets"] if "size" in entry}
        for identity in ("garden_bank", "headland_sand"):
            prop = self.props[identity]
            top = base_y(self.level, prop)+sizes[prop["model"]][1]*prop.get("scale", 1)
            support = authored_floor(self.level, prop["x"], prop["z"])
            if support is None:
                # The bank can instead be scenic stock behind containment.
                min_z = prop["z"]-sizes[prop["model"]][2]*prop.get("scale", 1)/2
                inner_boundary = collider_box(self.level, self.props["boundary_south"])[2]
                self.assertGreaterEqual(min_z, inner_boundary, identity)
                continue
            self.assertLessEqual(top, support+.01, (identity, top, support))

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

    def test_swimming_fish_poses_clear_the_sloped_seabed_and_waterline(self):
        # Checking only the placement center misses fins on the uphill side.
        # The generic validator reads the committed rig and the runtime's SLERP.
        sys.path.insert(0, str(ROOT / "tools/entities"))
        from validate_entities import Model
        model = Model(ROOT / "assets/entities/beach_fish/model/beach_fish.glb")
        clip = next(clip for clip in model.clips if clip["name"] == "swim")
        fish = [prop for prop in self.level["props"] if prop["model"] == "beach:fish"]
        self.assertTrue(fish)
        for prop in fish:
            minimum = math.inf
            for frame in range(16):
                posed = model._skin(model._pose_globals(clip, clip["duration"]*frame/16))
                for vx, vy, vz in posed:
                    scale = prop.get("scale", 1)
                    x, z = world_point(prop, vx*scale, vz*scale)
                    y = base_y(self.level, prop)+vy*scale
                    floor = authored_floor(self.level, x, z)
                    self.assertIsNotNone(floor, (prop["id"], x, z))
                    minimum = min(minimum, y-floor)
                    water = next((water for water in reversed(self.level["water"]) if contains(water, x, z)), None)
                    self.assertIsNotNone(water, (prop["id"], x, z))
                    self.assertLess(y, water["surface_y"], prop["id"])
            self.assertGreaterEqual(minimum, .02-1e-5, (prop["id"], minimum))


if __name__ == "__main__":
    unittest.main()
