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

    def test_source_is_current_and_repeatable(self):
        self.assertEqual(author.OUTPUT.read_text(), author.serialise(author.build_level()))
        self.assertEqual(author.build_level(), author.build_level())

    def test_namespace_uses_real_shared_images_and_keeps_original_evergreen(self):
        catalog = json.loads((ROOT / 'assets/catalog.json').read_text())
        assets = {a['id']: a for a in catalog['assets']}
        for material in (a for a in catalog['assets'] if a.get('theme') == 'winter'):
            texture = assets[material['texture']]
            self.assertTrue((ROOT / 'assets' / texture['model']).is_file())
            self.assertEqual(texture['source'], 'file')
        tree = assets['outdoor:tree_03']
        self.assertEqual(tree['model'], 'environment/outdoor/props/models/tree_03.glb')
        self.assertEqual(tree['size'], [3.2, 6.8, 3.2])
        level = author.build_level()
        self.assertFalse(level.get('water'), 'foundation ice is a physical floor, not swimming water')
        self.assertTrue(any(p['model'] == 'outdoor:tree_03' for p in level['props']))
        # Leak waivers must remain outside the world, never conceal an indoor hole.
        for intent in level['geometry_intent']:
            if intent['check'] == 'room-leak':
                self.assertTrue(intent['x'] + intent['width'] <= -24 or intent['x'] >= 24
                                or intent['z'] + intent['depth'] <= -40 or intent['z'] >= 20)


if __name__ == '__main__':
    unittest.main()
