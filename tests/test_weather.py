"""Snow authoring, asset contracts and validator regressions."""
import json
from pathlib import Path
import sys
import unittest
import zipfile

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'tools/assets'))
sys.path.insert(0, str(ROOT / 'tools/props'))
import validate
from tex import decode_png


class WeatherTests(unittest.TestCase):
    def test_non_winter_sources_and_packages_keep_their_climate_and_ground(self):
        catalog = {a['id']: a for a in json.loads((ROOT / 'assets/catalog.json').read_text())['assets']}
        for path in (ROOT / 'assets/levels').glob('*.json'):
            if path.stem == 'winter':
                continue
            source = json.loads(path.read_text())
            with zipfile.ZipFile(path.with_suffix('.placesmap')) as archive:
                packaged = json.loads(archive.read('semantics.json'))
            for level in (source, packaged):
                self.assertIsNone(level.get('weather'), path)
                sky = level.get('sky')
                if sky:
                    self.assertEqual(sky['texture'], 'outdoor:tex_sky_stars_01', path)
                # Model Zoo intentionally exhibits the reusable winter props;
                # its walkable ground and climate still use ordinary materials.
                for key, value in level.items():
                    if key != 'props':
                        self.assertNotIn('winter:', json.dumps(value), (path, key))
                materials = {asset_id for asset_id, _ in validate.level_ids(level)}
                self.assertFalse(any(catalog.get(m, {}).get('ground_surface') == 'ice'
                                     for m in materials), path)

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

    def test_blizzard_configuration_and_shelter_limit(self):
        storm = {'kind': 'snow', 'storm_severity': 1, 'visibility_m': 5, 'wind': [8, 3]}
        errors = []
        validate.validate_weather({'weather': storm}, 'fixture', errors)
        self.assertEqual(errors, [])
        for patch in ({'storm_severity': -1}, {'storm_severity': 1.1}, {'visibility_m': 1},
                      {'visibility_m': float('inf')}, {'intensity': 2}, {'wind': [21, 0]},
                      {'fog_color': [.5, .5]}, {'fog_color': [.5, .5, -1]}):
            errors = []
            validate.validate_weather({'weather': {**storm, **patch}}, 'fixture', errors)
            self.assertTrue(errors, patch)
        errors = []
        validate.validate_weather({'weather': storm, 'rooms': [{}] * 33}, 'fixture', errors)
        self.assertTrue(errors)

    def test_blizzard_review_preserves_winter_content(self):
        original = json.loads((ROOT / 'assets/levels/winter.json').read_text())
        review = json.loads((ROOT / 'debug-maps/blizzard-20261007/sources/blizzard_review.json').read_text())
        for key in original.keys() - {'id', 'name', 'weather'}:
            self.assertEqual(original[key], review[key], key)
        self.assertEqual(review['weather']['storm_severity'], 1)
        self.assertEqual(review['weather'].get('count', 1400), 1400)
