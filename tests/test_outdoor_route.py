"""Placement regressions for the maintained demo's outdoor authoring slice."""

import json
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "tools" / "levels"))
import build_outdoor_route as route  # noqa: E402


class OutdoorPlacementTests(unittest.TestCase):
    def setUp(self):
        self.level = json.loads((ROOT / "assets/levels/places_demo.json").read_text())
        self.slice = route.build_slice(self.level)

    def test_ridge_stays_joined_over_the_lowered_stair_hall(self):
        ridges = [p for p in self.slice["props"]
                  if p.get("id", "").startswith("night_source_ridge_")]
        self.assertEqual(len(ridges), 8)
        for ridge in ridges:
            # At z=2.09 the east stair landing stands at -1.5 + 1.2 = -0.3.
            floor = -0.3 if ridge["x"] > 19.0 else 0.0
            self.assertAlmostEqual(floor + ridge["y"], 4.077, places=4)

    def test_rotated_table_collision_matches_its_visible_footprint(self):
        table = next(p for p in self.slice["props"] if p.get("id") == "night_house_table")
        self.assertEqual(table["rotation_degrees"], 90.0)
        self.assertEqual(table["size"], [0.8, 0.75, 1.4])

    def test_center_pivot_roof_meets_the_source_eave_and_ridge(self):
        props = {p.get("id"): p for p in self.slice["props"]}
        for index in range(8):
            slope = props[f"night_source_roof_{index}"]
            ridge = props[f"night_source_ridge_{index}"]
            self.assertEqual(slope["rotation_degrees"], 180.0)
            # Current GLB: eave at local +Z=1.3, ridge at -Z=-1.3.
            half_run = 1.3 * slope["scale"]
            self.assertAlmostEqual(slope["z"] - half_run, -0.25)
            self.assertAlmostEqual(slope["z"] + half_run, ridge["z"])

    def test_regeneration_preserves_the_interior_and_is_idempotent(self):
        before = {key: self.level[key] for key in ("baseboards", "stairs", "archways")}
        route.apply_slice(self.level)
        self.assertEqual(before, {key: self.level[key] for key in before})
        once = route._render(self.level)
        route.apply_slice(self.level)
        self.assertEqual(once, route._render(self.level))


if __name__ == "__main__":
    unittest.main()
