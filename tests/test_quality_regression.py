"""Quality validation must reject absent draws and requested/resident mismatches."""
import copy
import importlib.util
import json
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("quality_regression", ROOT / "tools/bench/check_quality_regression.py")
QUALITY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(QUALITY)


class QualityRegressionReceiptTests(unittest.TestCase):
    def setUp(self):
        self.spec = dict(quality="high", filtering="low", lightmaps="full",
                         reflections="full", lighting="high", bloom=True)
        self.receipt = dict(event="capture", mode="final", level="real_map",
                            requested=dict(self.spec), applied=dict(self.spec),
                            resident=dict(quality="high", filtering="low", lightmaps="full",
                                          dynamic=dict(objects=1), movable_visibility_error=None),
                            capture_submission=dict(draw_calls=1, submitted_indices=3),
                            frame=dict(drawable=[1280, 720], diagnostic_identity_post=False))

    def log(self, receipt):
        return "[visual-diagnostic] " + json.dumps(receipt)

    def test_real_capture_and_independent_filtering_request_are_required(self):
        result = QUALITY.inspect(self.log(self.receipt), [self.spec], "real_map", 1)
        self.assertEqual(result[0]["resident"]["filtering"], "low")
        with self.assertRaises(ValueError):
            QUALITY.inspect("", [self.spec], "real_map", 1)

    def test_stale_requests_resident_settings_and_empty_submissions_fail(self):
        mutations = [
            lambda value: value["requested"].update(lightmaps="off"),
            lambda value: value["applied"].update(quality="low"),
            lambda value: value["resident"].update(filtering="high"),
            lambda value: value["resident"]["dynamic"].update(objects=0),
            lambda value: value["capture_submission"].update(submitted_indices=0),
            lambda value: value["frame"].update(diagnostic_identity_post=True),
            lambda value: value.update(level="other_map"),
        ]
        for mutate in mutations:
            receipt = copy.deepcopy(self.receipt)
            mutate(receipt)
            with self.assertRaises(ValueError):
                QUALITY.inspect(self.log(receipt), [self.spec], "real_map", 1)

    def test_requested_full_cannot_hide_a_missing_or_wrong_actual_atlas(self):
        atlas = dict(pages=2, page_edge=1024, charts=5, chart_texels=512, bytes=33554432)
        resources = dict(full=dict(atlas=atlas, lightmaps_resident=True, irradiance_present=True,
                                   lightmap_failure=None))
        self.receipt["resident"].update(atlas=dict(atlas), lightmaps_resident=True,
                                        irradiance_field=dict(slots=1), reflection_enabled=True)
        QUALITY.inspect(self.log(self.receipt), [self.spec], "real_map", 1, resources)
        for mutate in (lambda value: value["resident"].update(lightmaps_resident=False),
                       lambda value: value["resident"]["atlas"].update(bytes=0),
                       lambda value: value["resident"].update(irradiance_field=None),
                       lambda value: value["resident"].update(reflection_enabled=False)):
            receipt = copy.deepcopy(self.receipt)
            mutate(receipt)
            with self.assertRaises(ValueError):
                QUALITY.inspect(self.log(receipt), [self.spec], "real_map", 1, resources)

    def test_scripted_chair_must_survive_even_when_an_unrelated_dynamic_object_remains(self):
        chair = dict(path="dynamic:chair.glb", centre=[2, .451, 3], tolerance_m=.15)
        witness = dict(name="DynamicId(7)", path=chair["path"], position=[2, .45, 3], spatial=dict(enabled=True))
        unrelated = dict(name="DynamicId(9)", path="dynamic:door.glb", position=[8, 0, 8], spatial={})
        block = lambda entities: "\n".join("[entity-spatial] " + json.dumps(entity) for entity in entities) + "\n" + self.log(self.receipt)
        valid = block([witness, unrelated]) + "\n" + block([unrelated, witness])
        checked = QUALITY.inspect(valid, [self.spec, self.spec], "real_map", 1, chair=chair)
        self.assertEqual(checked[1]["checked_scripted_chair"]["name"], witness["name"])
        mutations = [dict(witness, name="DynamicId(8)"), dict(witness, path="dynamic:other.glb"),
                     dict(witness, position=[2, .45, 3.1]), dict(witness, spatial=None)]
        for changed in [None, *mutations]:
            log = block([witness, unrelated]) + "\n" + block([unrelated] + ([changed] if changed else []))
            with self.assertRaises(ValueError):
                QUALITY.inspect(log, [self.spec, self.spec], "real_map", 1, chair=chair)


if __name__ == "__main__":
    unittest.main()
