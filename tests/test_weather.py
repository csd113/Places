"""Snow authoring, asset contracts and validator regressions."""
import json
from pathlib import Path
import sys
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'tools/assets'))
sys.path.insert(0, str(ROOT / 'tools/props'))
import validate
from tex import decode_png


class WeatherTests(unittest.TestCase):
    def test_weather_is_only_opted_in_by_winter(self):
        for path in (ROOT / 'assets/levels').glob('*.json'):
            level = json.loads(path.read_text())
            if level['id'] == 'winter':
                self.assertEqual(level['weather'], {'kind': 'snow'})
            else:
                self.assertNotIn('weather', level, path)

    def test_configuration_defaults_and_bounds(self):
        errors = []
        validate.validate_weather({'weather': {'kind': 'snow'}}, 'fixture', errors)
        self.assertEqual(errors, [])
        for patch in ({'count': 0}, {'count': 2049}, {'count': True}, {'radius': 33},
                      {'height': float('nan')}, {'opacity': -1}, {'wind': [1, 0]},
                      {'size': [.1, .01]}, {'speed': [.5, 3]}, {'material': '../bad'},
                      {'cout': 10}, {'kind': 'rain'}):
            errors = []
            validate.validate_weather({'weather': {'kind': 'snow', **patch}}, 'fixture', errors)
            self.assertTrue(errors, patch)

    def test_sheet_is_full_uv_square_with_zero_edge_alpha(self):
        catalog = {a['id']: a for a in json.loads((ROOT / 'assets/catalog.json').read_text())['assets']}
        material = catalog['core:snowflake_01']
        self.assertEqual(material['alpha_mode'], 'blend')
        self.assertEqual(material['opacity'], 1)
        self.assertNotIn('emissive', material)
        width, height, rgba = decode_png((ROOT / 'assets' / catalog[material['texture']]['model']).read_bytes())
        self.assertEqual((width, height), (256, 256))
        alpha = rgba[3::4]
        for y in range(height):
            for x in range(width):
                if x in (0, width - 1) or y in (0, height - 1):
                    self.assertEqual(alpha[y * width + x], 0)
        self.assertGreater(max(alpha), 200)
        self.assertGreater(sum(a == 0 for a in alpha), width * height // 3)

    def test_default_weather_material_is_a_compiler_reference(self):
        refs = list(validate.level_ids({'weather': {'kind': 'snow'}}))
        self.assertIn(('core:snowflake_01', 'weather material'), refs)
