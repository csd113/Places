"""Cheap authoring boundary coverage; no map bake or native dependency."""
import unittest

from tools.assets.validate import validate_physics_profile


class WaterPhysicsValidationTests(unittest.TestCase):
    def test_water_profile_rejects_invalid_motion_volume_drag_and_unknown_fields(self):
        """Prevent Python authoring checks accepting profiles Rust will refuse."""
        profile = {"motion": "dynamic", "water": {"displaced_volume_m3": 0.0008}}
        self.assertEqual(validate_physics_profile(profile, "fixture"), [])
        cases = (
            {"motion": "fixed", "water": {}},
            {"motion": "kinematic", "water": {}},
            {"motion": "dynamic", "water": {"density": 1000}},
            {"motion": "dynamic", "water": {"displaced_volume_m3": True}},
            {"motion": "dynamic", "water": {"displaced_volume_m3": 0}},
            {"motion": "dynamic", "mass_kg": 0.001, "water": {"displaced_volume_m3": 1}},
            {"motion": "dynamic", "water": {"linear_drag": float("nan")}},
            {"motion": "dynamic", "water": {"angular_drag": 101}},
        )
        for case in cases:
            with self.subTest(case=case):
                self.assertTrue(validate_physics_profile(case, "fixture"))
        self.assertTrue(validate_physics_profile(
            {"motion": "dynamic", "water": {"displaced_volume_m3": 0.008}}, "fixture", 100))
        self.assertTrue(validate_physics_profile(profile, "fixture", 0.001))


if __name__ == "__main__":
    unittest.main()
