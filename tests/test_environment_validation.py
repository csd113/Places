"""Keep source validation aligned with the authored HDR/fog/water contracts."""
import importlib.util
import copy
import json
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("environment_validator", ROOT / "tools/assets/validate.py")
VALIDATE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(VALIDATE)


class EnvironmentValidationTests(unittest.TestCase):
    def test_project_actions_accept_real_links_and_refuse_unsafe_or_undeclared_targets(self):
        # The collection scan exposed rejection of every real transition and
        # Home script link. One tiny switch protects those action types and
        # their boundaries without files, a VM, assets or a map bake.
        source = dict(transitions=['hall'], props=[dict(id='switch', model='home:wall_switch',
            components=[dict(component='interactable', prompt='Continue')],
            bindings=[dict(on='interact', actions=[dict(action='change_level', level='hall'),
                dict(action='run_script', script='scripts/main.luau', handler='continue_1')])])])
        errors = []
        VALIDATE.validate_interactions(source, 'fixture', errors)
        self.assertEqual(errors, [])
        for action in (dict(action='change_level', level='missing'),
                       dict(action='run_script', script='../main.luau', handler='start'),
                       dict(action='run_script', script='scripts/CON.luau', handler='start'),
                       dict(action='run_script', script='scripts/COM¹.luau', handler='start'),
                       dict(action='run_script', script='scripts/ main.luau', handler='start'),
                       dict(action='run_script', script='scripts/.luau', handler='start'),
                       dict(action='run_script', script='scripts/main.luau', handler='start()')):
            changed = copy.deepcopy(source)
            changed['props'][0]['bindings'][0]['actions'] = [action]
            errors = []
            VALIDATE.validate_interactions(changed, 'fixture', errors)
            self.assertTrue(errors, action)
        for transitions in ('hall', ['hall', 'hall'], ['con']):
            changed = dict(source, transitions=transitions)
            errors = []
            VALIDATE.validate_interactions(changed, 'fixture', errors)
            self.assertTrue(errors, transitions)

    def check_environment(self, environment):
        errors = []
        VALIDATE.validate_environment(dict(environment=environment), "fixture", errors)
        return errors

    def check_sky(self, patch):
        errors = []
        VALIDATE.validate_sky(dict(sky=dict(texture="outdoor:tex_sky_stars_01", **patch)), "fixture", errors)
        return errors

    def test_optional_defaults_and_strict_nested_objects(self):
        for value in (None, {}, dict(presentation={}), dict(fog={})):
            self.assertEqual(self.check_environment(value), [], value)
        for value in (True, [], dict(presentation=None), dict(fog=None),
                      dict(exposre=1), dict(presentation=dict(exposre=1)),
                      dict(fog=dict(densitiy=.01))):
            self.assertTrue(self.check_environment(value), value)
        errors = []
        VALIDATE.validate_environment({}, "fixture", errors)
        VALIDATE.validate_sky(dict(sky=None), "fixture", errors)
        self.assertEqual(errors, [])

    def test_numeric_bounds_are_inclusive_and_malformed_values_fail(self):
        fields = [("presentation", "exposure", .125, 8),
                  ("presentation", "tone_knee", .25, .95),
                  ("presentation", "saturation", .8, 1.2),
                  ("presentation", "contrast", .8, 1.2),
                  ("fog", "density", 0, .5),
                  ("fog", "reference_y", -10000, 10000),
                  ("fog", "height_gain", 0, 1)]
        for section, field, low, high in fields:
            for value in (low, high):
                self.assertEqual(self.check_environment({section: {field: value}}), [], (field, value))
            for value in (low - .001, high + .001, True, None, "1", float("nan"), float("inf")):
                errors = self.check_environment({section: {field: value}})
                self.assertTrue(any(field in error for error in errors), (field, value, errors))

    def test_fog_and_sky_colors_have_three_finite_numeric_channels(self):
        for color in ([0, 0, 0], [1, 1, 1], [.1, .2, .3]):
            self.assertEqual(self.check_environment(dict(fog=dict(color=color))), [])
            self.assertEqual(self.check_sky(dict(ambient_color=color)), [])
        for color in (None, [], [0, 1], [0, 1, 0, 1], [0, True, 1], [0, -1, 1],
                      [0, 1.001, 1], [0, float("nan"), 1], [0, "1", 1]):
            self.assertTrue(self.check_environment(dict(fog=dict(color=color))), color)
            if color is not None:
                self.assertTrue(self.check_sky(dict(ambient_color=color)), color)
        self.assertEqual(self.check_sky(dict(ambient_color=None)), [])

    def test_sky_defaults_permissive_names_and_energy_bounds(self):
        self.assertEqual(self.check_sky({}), [])
        self.assertEqual(self.check_sky(dict(unrecognized_field=True)), [])
        for field, maximum in (("brightness", 4), ("ambient", 1)):
            for value in (0, maximum):
                self.assertEqual(self.check_sky({field: value}), [])
            for value in (-.001, maximum + .001, True, None, float("nan"), float("inf")):
                self.assertTrue(self.check_sky({field: value}), (field, value))
        for sky in (True, [], dict(texture=True), dict(texture="../escape.png")):
            errors = []
            VALIDATE.validate_sky(dict(sky=sky), "fixture", errors)
            self.assertTrue(errors, sky)

    def test_water_extinction_defaults_and_bounds_preserve_shape_contracts(self):
        water = dict(x=0, z=0, width=2, depth=2, surface_y=0)
        for patch in ({}, dict(attenuation_per_metre=0), dict(attenuation_per_metre=16)):
            errors = []
            VALIDATE.validate_water_shapes(dict(water=[dict(water, **patch)]), "fixture", errors)
            self.assertEqual(errors, [], patch)
        for value in (-.001, 16.001, True, None, "1", float("nan"), float("inf")):
            errors = []
            VALIDATE.validate_water_shapes(dict(water=[dict(water, attenuation_per_metre=value)]), "fixture", errors)
            self.assertTrue(any("attenuation_per_metre" in error for error in errors), (value, errors))

    def test_level_scan_validates_controls_and_resolves_the_sky_texture(self):
        catalog = VALIDATE.load_catalog()
        source = json.loads((ROOT / "tests/fixtures/levels/test_room.json").read_text())
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "fixture.json"
            source["environment"] = dict(presentation=dict(exposure=True))
            source["sky"] = dict(texture="missing:sky", ambient_color=[False, 0, 0])
            path.write_text(json.dumps(source))
            errors, _ = VALIDATE.validate_levels(catalog, (directory,))
        for expected in ("exposure", "ambient_color", "missing:sky"):
            self.assertTrue(any(expected in error for error in errors), (expected, errors))


if __name__ == "__main__":
    unittest.main()
