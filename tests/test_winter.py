"""Winter authoring invariants that complement the actual controller audits."""
import json
from pathlib import Path
import sys
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'tools/levels'))
import build_winter as author


class WinterTests(unittest.TestCase):
    def test_ground_and_interiors_partition_the_entire_world(self):
        rooms = author.build_level()['rooms']
        self.assertAlmostEqual(sum(r['width'] * r['depth'] for r in rooms), 48 * 60)
        for index, a in enumerate(rooms):
            for b in rooms[index + 1:]:
                dx = min(a['x'] + a['width'], b['x'] + b['width']) - max(a['x'], b['x'])
                dz = min(a['z'] + a['depth'], b['z'] + b['depth']) - max(a['z'], b['z'])
                self.assertFalse(dx > 1e-6 and dz > 1e-6, (a, b))

    def test_ice_has_real_albedo_traction_backing_and_clear_shore_access(self):
        assets = {a['id']: a for a in json.loads((ROOT / 'assets/catalog.json').read_text())['assets']}
        ice = assets['winter:ice_01']
        self.assertEqual(ice['ground_surface'], 'ice')
        self.assertEqual(ice['alpha_mode'], 'blend')
        self.assertGreater(ice['opacity'], .7)
        self.assertLess(ice['opacity'], 1)
        self.assertNotIn('reflection_mode', ice)
        self.assertEqual(assets[ice['texture']]['model'], 'environment/winter/textures/floors/ice_01.png')
        level = author.build_level()
        backing = next(b for b in level['void_walls'] if b['id'] == 'pond_ice_depth')
        self.assertFalse(backing['solid'])
        self.assertLess(backing['max'][1], -.16)
        self.assertEqual(sum(p['id'].startswith('pond_shore_snow_') for p in level['props']), 6)
        # The approach at z=-10 is not buried beneath snow props.
        self.assertFalse(any(p['id'].startswith('pond_shore_snow_') and abs(p['z'] + 10) < 1
                             for p in level['props']))

    def test_source_is_current_and_repeatable(self):
        self.assertEqual(author.OUTPUT.read_text(), author.serialise(author.build_level()))
        self.assertEqual(author.build_level(), author.build_level())

    def test_namespace_uses_real_shared_images_and_keeps_original_evergreen(self):
        catalog = json.loads((ROOT / 'assets/catalog.json').read_text())
        assets = {a['id']: a for a in catalog['assets']}
        for material in (a for a in catalog['assets'] if a.get('theme') == 'winter'
                         and a['asset_type'] == 'material'):
            texture = assets[material['texture']]
            self.assertTrue((ROOT / 'assets' / texture['model']).is_file())
            self.assertEqual(texture['source'], 'file')
        tree = assets['outdoor:tree_03']
        self.assertEqual(tree['model'], 'environment/outdoor/props/models/tree_03.glb')
        self.assertEqual(tree['size'], [3.2, 6.8, 3.2])
        level = author.build_level()
        self.assertFalse(level.get('water'), 'frozen pond replaces swimming water with a solid floor')
        self.assertTrue(any(p['model'] == 'winter:tree_snow_01' for p in level['props']))
        self.assertTrue(any(p['model'] == 'winter:tree_snow_02' for p in level['props']))
        # Leak waivers must remain outside the world, never conceal an indoor hole.
        for intent in level['geometry_intent']:
            if intent['check'] == 'room-leak':
                self.assertTrue(intent['x'] + intent['width'] <= -24 or intent['x'] >= 24
                                or intent['z'] + intent['depth'] <= -40 or intent['z'] >= 20)


if __name__ == '__main__':
    unittest.main()
