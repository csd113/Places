"""Authoring contracts complement the real controller audits in Rust."""
from __future__ import annotations
import json
import math
from pathlib import Path
import sys
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools/levels"))
import build_lantern_hollow as author


class LanternHollowTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.level = author.build_level()

    def test_authored_source_is_current_and_repeatable(self):
        self.assertEqual(author.serialise(self.level), author.OUTPUT.read_text())
        self.assertEqual(author.serialise(self.level), author.serialise(author.build_level()))

    def test_four_real_furnished_cottages_and_clear_front_doors(self):
        homes = [r for r in self.level['rooms'] if r['ceiling']['kind'] == 'gable']
        self.assertEqual(len(homes), 4)
        self.assertEqual(len(self.level['doors']), 4)
        for i in range(4):
            models = {p['model'] for p in self.level['props'] if p['id'].startswith(f'house_{i}_')}
            self.assertTrue({'core:bed', 'core:couch', 'core:fridge', 'core:sink', 'home:cabinet_base'} <= models)
        self.assertFalse(any('_wall_window' in p['model'] for p in self.level['props']),
                         'Opaque kit windows must not conceal the real glazed openings')
        self.assertTrue(all(o.get('solid') and o.get('glass') for w in self.level['walls']
                            for o in w.get('openings', []) if o['kind'] == 'window'))

    def test_domestic_solids_do_not_overlap_and_kitchen_aisles_fit_a_player(self):
        solids = [p for p in self.level['props'] if p.get('solid') and '_furniture_' in p['id']]
        def bounds(p):
            w, _, d = p['size']
            yaw = math.radians(p.get('rotation_degrees', 0))
            hx = (w * abs(math.cos(yaw)) + d * abs(math.sin(yaw))) / 2
            hz = (w * abs(math.sin(yaw)) + d * abs(math.cos(yaw))) / 2
            return p['x'] - hx, p['z'] - hz, p['x'] + hx, p['z'] + hz
        for i, p in enumerate(solids):
            a = bounds(p)
            for q in solids[i + 1:]:
                b = bounds(q)
                ox = min(a[2], b[2]) - max(a[0], b[0])
                oz = min(a[3], b[3]) - max(a[1], b[1])
                self.assertFalse(ox > 1e-4 and oz > 1e-4, (p['id'], q['id']))
        for i in range(4):
            furniture = {p['model']: p for p in solids if p['id'].startswith(f'house_{i}_')}
            table_left = bounds(furniture['core:table'])[0]
            counter_right = bounds(furniture['home:cabinet_base'])[2]
            self.assertGreaterEqual(table_left - counter_right, .8)

    def test_floor_rooms_partition_the_world_without_duplicate_surfaces(self):
        rooms = self.level['rooms']
        self.assertEqual(len(rooms), 54)
        for i, a in enumerate(rooms):
            for b in rooms[i + 1:]:
                overlap_x = min(a['x'] + a['width'], b['x'] + b['width']) - max(a['x'], b['x'])
                overlap_z = min(a['z'] + a['depth'], b['z'] + b['depth']) - max(a['z'], b['z'])
                self.assertFalse(overlap_x > 1e-5 and overlap_z > 1e-5, (a['comment'], b['comment']))
        self.assertAlmostEqual(sum(r['width'] * r['depth'] for r in rooms), 88 * 78, places=4)

    def test_seated_figures_are_skeletons_with_real_held_animation(self):
        skeletons = [p for p in self.level['props'] if p['model'] == 'skeleton']
        self.assertEqual(len(skeletons), 7)
        routes = {r['id']: r for r in self.level['routes']}
        for p in skeletons:
            self.assertAlmostEqual(math.dist((p['x'], p['z']), author.FIRE), 2.6, places=3)
            cue = routes[p['id']]['steps'][0]
            self.assertEqual(cue['clip'], 'pose_sit_chair')
            self.assertTrue(cue['loop'])
            self.assertGreater(cue['seconds'], 60)

    def test_existing_sheet_cats_only_and_no_cars(self):
        cats = [p for p in self.level['props'] if p['model'] == 'sheet-ghost-cat']
        self.assertEqual(len(cats), 3)
        self.assertTrue(all(p['scale'] == 2 for p in cats))
        self.assertFalse(any('spectral' in p['model'] or 'car' == p['model'] for p in self.level['props']))

    def test_natural_visible_edges_and_narrow_intent_only(self):
        self.assertFalse(self.level['void_walls'])
        self.assertFalse(any(p['model'] == 'outdoor:collision_peg' for p in self.level['props']))
        edges = [p for p in self.level['props'] if p['id'].startswith(('boundary_rock_', 'road_gate_'))]
        self.assertGreater(len(edges), 70)
        self.assertTrue(all(p['solid'] and p['size'][1] >= 3.6 for p in edges))
        for intent in self.level['geometry_intent']:
            self.assertIn(intent['check'], ('missing-wall', 'room-leak'))
            if intent['check'] == 'missing-wall':
                self.assertTrue(any(all(abs(intent[k] - r[k]) < 1e-5 for k in ('x', 'z', 'width', 'depth'))
                                    and r['ceiling']['kind'] == 'open' for r in self.level['rooms']))
            else:
                # Void heuristics may only be waived beyond the authored floor,
                # never in a cottage or along an internal forest/room seam.
                x, z, w, d = (intent[k] for k in ('x', 'z', 'width', 'depth'))
                self.assertTrue(x + w <= -44 or x >= 44 or z + d <= -46 or z >= 32)


    def test_trees_keep_walkable_trails_pond_and_clearing_open(self):
        trees = [p for p in self.level['props'] if p['id'].startswith('forest_tree_')]
        self.assertEqual(len(trees), 247)
        for p in trees:
            self.assertGreater(author.trail_distance(p['x'], p['z']), 2.09)
            self.assertGreater(math.dist((p['x'], p['z']), author.POND), 6.19)
            self.assertGreater(math.dist((p['x'], p['z']), author.FIRE), 7.49)

    def test_pond_is_circular_with_two_safe_step_depths(self):
        water = self.level['water'][0]
        self.assertEqual(water['shape'], 'circle')
        self.assertEqual((water['x'] + water['radius'], water['z'] + water['radius']), author.POND)
        depths = sorted({r['offset_y'] for r in self.level['floor_regions'] if r['offset_y'] < 0})
        self.assertEqual(depths, [-.65, -.3])
        self.assertLess(.65 - .3, .4)
        self.assertLess(.3, .4)


if __name__ == '__main__':
    unittest.main()
