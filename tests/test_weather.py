"""Snow authoring, asset contracts and validator regressions."""
import copy
import json
import os
from pathlib import Path
import struct
import sys
import unittest
import zipfile

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'tools/assets'))
sys.path.insert(0, str(ROOT / 'tools/props'))
sys.path.insert(0, str(ROOT / 'tools/levels'))
import build_winter
import validate
from tex import decode_png


def assert_non_winter_climate(test, level, stem):
    catalog = {a['id']: a for a in json.loads((ROOT / 'assets/catalog.json').read_text())['assets']}
    test.assertIsNone(level.get('weather'), stem)
    sky = level.get('sky')
    if stem in ('beach_demo', 'frutiger_aero_demo'):
        test.assertIsNotNone(sky, stem)
        theme = 'beach' if stem == 'beach_demo' else 'frutiger_aero'
        test.assertEqual(sky['texture'], theme+':tex_sky_day_01', stem)
    elif sky:
        test.assertEqual(sky['texture'], 'outdoor:tex_sky_stars_01', stem)
    # Model Zoo intentionally exhibits the reusable winter props;
    # its walkable ground and climate still use ordinary materials.
    for key, value in level.items():
        if key != 'props':
            test.assertNotIn('winter:', json.dumps(value), (stem, key))
    materials = {asset_id for asset_id, _ in validate.level_ids(level)}
    test.assertFalse(any(catalog.get(m, {}).get('ground_surface') == 'ice'
                         for m in materials), stem)


class WeatherTests(unittest.TestCase):
    def test_non_winter_sources_keep_their_climate_and_ground(self):
        sources = sorted((ROOT / 'levels').glob('*.json'))
        self.assertTrue(sources, 'maintained level sources must exist')
        for path in sources:
            if path.stem != 'winter':
                assert_non_winter_climate(self, json.loads(path.read_text()), path.stem)

    def test_weather_is_only_opted_in_by_winter(self):
        sources = sorted((ROOT / 'levels').glob('*.json'))
        self.assertTrue(sources, 'maintained level sources must exist')
        for path in sources:
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

    def test_alternate_weather_validates_independently_and_packages_its_material(self):
        storm = {'kind': 'snow', 'storm_severity': 1, 'wind': [8, 3],
                 'material': 'core:steam_01'}
        level = {'weather': {'kind': 'snow'}, 'weather_alternate': storm}
        errors = []
        validate.validate_weather(level, 'fixture', errors)
        self.assertEqual(errors, [])
        refs = list(validate.level_ids(level))
        self.assertIn(('core:snowflake_01', 'weather material'), refs)
        self.assertIn(('core:steam_01', 'weather_alternate material'), refs)
        for patch in ({'count': 0}, {'storm_severity': -1}, {'visibility_m': 1},
                      {'wind': [21, 0]}, {'kind': 'rain'}, {'cout': 10}):
            errors = []
            validate.validate_weather({**level, 'weather_alternate': {**storm, **patch}},
                                      'fixture', errors)
            self.assertTrue(errors, patch)
            self.assertTrue(any('weather_alternate' in error for error in errors), errors)
        errors = []
        validate.validate_weather({'weather_alternate': storm}, 'fixture', errors)
        self.assertTrue(any('requires authored default weather' in error for error in errors))
        errors = []
        validate.validate_weather({**level, 'rooms': [{}] * 33}, 'fixture', errors)
        self.assertTrue(any('at most 32' in error for error in errors))

    def test_toggle_weather_action_requires_both_authored_states(self):
        switch = {'id': 'weather_switch', 'model': 'core:switch', 'x': 1, 'z': 1,
                  'components': [{'component': 'interactable'}],
                  'bindings': [{'on': 'interact', 'actions': [{'action': 'toggle_weather'}]}]}
        level = {'props': [switch]}
        for weather in ({}, {'weather': {'kind': 'snow'}},
                        {'weather_alternate': {'kind': 'snow'}}):
            errors = []
            validate.validate_interactions({**level, **weather}, 'fixture', errors)
            self.assertTrue(any('requires weather and weather_alternate' in error
                                for error in errors), errors)
        errors = []
        validate.validate_interactions({**level, 'weather': {'kind': 'snow'},
                                       'weather_alternate': {'kind': 'snow', 'storm_severity': 1}},
                                      'fixture', errors)
        self.assertEqual(errors, [])

    def test_prompt_action_requires_interaction_and_nonblank_text(self):
        switch = {'id': 'weather_switch', 'model': 'home:wall_switch', 'x': 1, 'z': 1,
                  'components': [{'component': 'interactable'}],
                  'bindings': [{'on': 'interact', 'actions': [
                      {'action': 'set_prompt', 'prompt': 'Disable blizzard'}]}]}
        errors = []
        validate.validate_interactions({'props': [switch]}, 'fixture', errors)
        self.assertEqual(errors, [])
        for prompt in (' ', None, 3):
            switch['bindings'][0]['actions'][0]['prompt'] = prompt
            errors = []
            validate.validate_interactions({'props': [switch]}, 'fixture', errors)
            self.assertTrue(any('prompt must not be blank' in error for error in errors), errors)
        switch['bindings'][0]['actions'][0] = {'action': 'set_prompt', 'target': 'crate',
                                             'prompt': 'Next'}
        crate = {'id': 'crate', 'model': 'core:crate', 'x': 2, 'z': 2}
        errors = []
        validate.validate_interactions({'props': [switch, crate]}, 'fixture', errors)
        self.assertTrue(any('interactable' in error for error in errors), errors)

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

    def test_weather_cycle_is_opt_in_bounded_and_uses_compatible_resources(self):
        level = {'weather': {'kind': 'snow'}, 'weather_alternate': {'kind': 'snow', 'storm_severity': 1},
                 'weather_cycle': {'enabled': False, 'max_strength': .45, 'period_seconds': 120,
                                   'transition_seconds': 15}}
        errors = []
        validate.validate_weather(level, 'fixture', errors)
        self.assertEqual(errors, [])
        for patch in ({'enabled': 1}, {'min_strength': .6, 'max_strength': .3},
                      {'period_seconds': 0}, {'period_seconds': float('inf')},
                      {'transition_seconds': 61}, {'min_strength': -.1}, {'max_strength': 1.1},
                      {'period': 60}):
            errors = []
            validate.validate_weather({**level, 'weather_cycle': {**level['weather_cycle'], **patch}},
                                      'fixture', errors)
            self.assertTrue(errors, patch)
        for patch in ({'count': 2048}, {'material': 'core:steam_01'}):
            errors = []
            validate.validate_weather({**level, 'weather_alternate': {**level['weather_alternate'], **patch}},
                                      'fixture', errors)
            self.assertTrue(any('equal count and material' in error for error in errors), errors)

    def test_weather_strength_and_cycle_actions_validate_parameters_and_dependencies(self):
        switch = {'id': 'weather_switch', 'model': 'home:wall_switch', 'x': 1, 'z': 1,
                  'components': [{'component': 'interactable'}],
                  'bindings': [{'on': 'interact', 'actions': []}]}
        level = {'props': [switch], 'weather': {'kind': 'snow'},
                 'weather_alternate': {'kind': 'snow', 'storm_severity': 1}, 'weather_cycle': {}}
        for action in ({'action': 'set_weather_strength', 'strength': .35, 'transition_seconds': 2},
                       {'action': 'set_weather_strength', 'strength': 0},
                       {'action': 'set_weather_cycle', 'enabled': True}):
            switch['bindings'][0]['actions'] = [action]
            errors = []
            validate.validate_interactions(level, 'fixture', errors)
            self.assertEqual(errors, [], action)
        for action in ({'action': 'set_weather_strength', 'strength': True},
                       {'action': 'set_weather_strength', 'strength': -.1},
                       {'action': 'set_weather_strength', 'strength': .5, 'transition_seconds': -1},
                       {'action': 'set_weather_strength', 'strength': .5, 'transition_seconds': float('nan')},
                       {'action': 'set_weather_cycle', 'enabled': 1}):
            switch['bindings'][0]['actions'] = [action]
            errors = []
            validate.validate_interactions(level, 'fixture', errors)
            self.assertTrue(errors, action)
        switch['bindings'][0]['actions'] = [{'action': 'set_weather_cycle', 'enabled': True}]
        errors = []
        validate.validate_interactions({**level, 'weather_cycle': None}, 'fixture', errors)
        self.assertTrue(any('requires weather_cycle' in error for error in errors), errors)
        switch['bindings'][0]['actions'] = [{'action': 'set_weather_strength', 'strength': .35}]
        errors = []
        validate.validate_interactions({**level, 'weather_alternate': {'kind': 'snow', 'count': 2048}},
                                      'fixture', errors)
        self.assertTrue(any('equal count and material' in error for error in errors), errors)

    def test_derived_severe_control_preserves_winter_content(self):
        original = json.loads((ROOT / 'levels/winter.json').read_text())
        snapshot = copy.deepcopy(original)
        review = build_winter.build_severe_review(original)
        self.assertEqual(original, snapshot, 'deriving the control must not mutate Winter')
        self.assertEqual(original['weather_alternate'],
                         json.loads(build_winter.SEVERE_WEATHER.read_text()))
        for key in original.keys() - {'id', 'name', 'weather', 'weather_alternate',
                                     'weather_cycle', 'props'}:
            self.assertEqual(original[key], review[key], key)
        self.assertEqual(review['weather_cycle'], {
            **original['weather_cycle'],
            'min_strength': round(1 - original['weather_cycle']['max_strength'], 4),
            'max_strength': round(1 - original['weather_cycle']['min_strength'], 4),
        })
        for calm_prop, storm_prop in zip(original['props'], review['props'], strict=True):
            expected = copy.deepcopy(calm_prop)
            for binding in expected.get('bindings', []):
                for action in binding['actions']:
                    if action['action'] == 'set_weather_strength':
                        action['strength'] = round(1 - action['strength'], 4)
            if calm_prop['id'] == 'winter_blizzard_button':
                interactable = next(c for c in expected['components']
                                    if c['component'] == 'interactable')
                state = next(c for c in expected['components'] if c['component'] == 'state')
                self.assertEqual(interactable['prompt'], 'Enable moderate blizzard')
                self.assertFalse(state['value'])
                interactable['prompt'] = 'Stop blizzard'
                state['value'] = True
            self.assertEqual(expected, storm_prop)
        self.assertEqual(review.get('weather_alternate'), original.get('weather'))
        self.assertEqual(original.get('weather_alternate'), review.get('weather'))
        self.assertEqual(review['weather']['storm_severity'], 1)
        self.assertEqual(review['weather'].get('count', 1400), 1400)


@unittest.skipUnless(os.environ.get('PLACES_WEATHER_PACKAGE_DIR'),
                     'set PLACES_WEATHER_PACKAGE_DIR after compiling maintained levels')
class PackagedWeatherTests(unittest.TestCase):
    def assert_stored_weather(self, actual, authored, context):
        """Compare every authored value at the engine's exact f32 precision."""
        if isinstance(authored, float):
            expected = struct.unpack('<f', struct.pack('<f', authored))[0]
            self.assertEqual(actual, expected, context)
        elif isinstance(authored, list):
            self.assertIsInstance(actual, list, context)
            self.assertEqual(len(actual), len(authored), context)
            for index, (stored, value) in enumerate(zip(actual, authored, strict=True)):
                self.assert_stored_weather(stored, value, (context, index))
        elif isinstance(authored, dict):
            self.assertIsInstance(actual, dict, context)
            self.assertEqual(actual.keys(), authored.keys(), context)
            for key, value in authored.items():
                self.assertIn(key, actual, context)
                self.assert_stored_weather(actual[key], value, (context, key))
        else:
            self.assertEqual(actual, authored, context)

    def test_explicit_packages_preserve_authored_climate_and_ground(self):
        directory = Path(os.environ['PLACES_WEATHER_PACKAGE_DIR'])
        self.assertTrue(directory.is_dir(), directory)
        from tests.built_maps import selected_sources
        sources = selected_sources()
        self.assertTrue(sources, 'maintained level sources must exist')
        for source in sources:
            path = directory / source.with_suffix('.placesmap').name
            self.assertTrue(path.is_file(), f'compile {source} before package validation')
            with zipfile.ZipFile(path) as archive:
                manifest = json.loads(archive.read('manifest.json'))
                level = json.loads(archive.read('semantics.json'))
            self.assertEqual(manifest['id'], source.stem, path)
            self.assertEqual(level['id'], source.stem, path)
            if source.stem == 'winter':
                self.assertEqual(level['weather']['kind'], 'snow')
                for key, value in json.loads(build_winter.SEVERE_WEATHER.read_text()).items():
                    self.assert_stored_weather(level['weather_alternate'][key], value, (path, key))
                self.assert_stored_weather(level['weather_cycle'],
                                           json.loads(source.read_text())['weather_cycle'], path)
            else:
                assert_non_winter_climate(self, level, source.stem)
            errors = []
            validate.validate_weather(level, str(path), errors)
            self.assertEqual(errors, [], path)
