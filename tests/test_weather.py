"""Snow authoring, asset contracts and validator regressions."""
import copy
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
                if path.stem in ('beach_demo', 'frutiger_aero_demo'):
                    self.assertIsNotNone(sky, path)
                    theme = 'beach' if path.stem == 'beach_demo' else 'frutiger_aero'
                    self.assertEqual(sky['texture'], theme+':tex_sky_day_01', path)
                elif sky:
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

    def test_blizzard_review_preserves_winter_content(self):
        original = json.loads((ROOT / 'assets/levels/winter.json').read_text())
        review = json.loads((ROOT / 'debug-maps/blizzard-20261007/sources/blizzard_review.json').read_text())
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
