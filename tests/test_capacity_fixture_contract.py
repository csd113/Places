"""Keep the saved dense compiler fixture useful without rebuilding it.

The fixture is read once. Check its count/lighting/route pressure and require
each distinct witness to resolve to a current on-disk catalog placeable.
"""
import json
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]


class CapacityFixtureContractTests(unittest.TestCase):
    def test_saved_dense_pressure_and_catalog_assets(self):
        fixture = ROOT / 'tests/fixtures/levels/capacity_dense.json'
        level = json.loads(fixture.read_text(encoding='utf-8'))
        props = level['props']
        models = {prop['model'] for prop in props}
        self.assertEqual((len(props), len(models)), (5312, 48))
        self.assertEqual(sum(prop.get('solid', False) for prop in props), 2760)
        self.assertEqual((len(level['ceiling_lights']), len(level['routes'])), (120, 18))
        assets = ROOT / 'assets'
        catalog = json.loads((assets / 'catalog.json').read_text(encoding='utf-8'))
        entries = {entry['id']: entry for entry in catalog['assets'] if entry['id'] in models}
        self.assertEqual(set(entries), models)
        for identity, entry in entries.items():
            with self.subTest(model=identity):
                self.assertIn(entry['asset_type'], ('prop', 'entity'))
                path = (assets / entry['model']).resolve()
                self.assertTrue(path.is_relative_to(assets))
                self.assertTrue(path.is_file(), path)
