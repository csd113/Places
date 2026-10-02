"""Isolated growth/removal fixtures for the Model Zoo generator.

These run on the source tree without a GPU. They import the generator as a
module and drive it against *synthetic* catalogs in a temporary directory, so
they never touch the shipped catalog, the shipped zoo or the real assets:

* adding a catalogue entry adds exactly one display with a stable id, expands
  the hall when the floor rows fill, and never renumbers an existing display;
* removing an entry removes exactly its display and changes nothing else;
* regenerating from the shipped catalog is byte-identical to the shipped level
  (idempotence), and the serial and parallel inspection paths agree.

The worker paths are exercised with one worker (the serial reference) and with
two, over the same real catalog, comparing the resulting bytes.
"""

from __future__ import annotations

import copy
import json
import os
import sys
import tempfile
import unittest
from pathlib import Path

PACKAGE = Path(__file__).resolve().parent.parent
GENERATOR = PACKAGE / "tools" / "levels" / "build_model_zoo.py"

# Import the generator as a real module: the spawn-context worker pool requires
# an importable module path on this platform, so the test uses the same import
# the tool does when run as a script.
sys.path.insert(0, str(GENERATOR.parent))
import build_model_zoo as zoo  # noqa: E402


def synthetic_inspection(asset_id: str, model: str) -> dict:
    return {
        "id": asset_id,
        "model": model,
        "rest_bounds": [[0.0, 0.0, 0.0], [0.5, 0.9, 0.5]],
        "envelope": None,
        "envelope_clip": None,
        "clips": [],
        "skinned": False,
        "triangles": 12,
        "error": None,
    }


def inspections_for(catalog: dict) -> dict:
    return {
        entry["id"]: synthetic_inspection(entry["id"], entry["model"])
        for entry in zoo.placeables(catalog)
    }


class ZooGeneratorFixtureTests(unittest.TestCase):
    def setUp(self) -> None:
        self.catalog = zoo.load_catalog()

    def level_for(self, catalog: dict) -> dict:
        return zoo.build_level(catalog, inspections_for(catalog))

    def display_ids(self, level: dict) -> list[str]:
        return [prop["id"] for prop in level["props"]]

    def test_regeneration_matches_the_shipped_zoo(self):
        level = self.level_for(self.catalog)
        shipped = json.loads(
            (PACKAGE / "assets" / "levels" / "model_zoo.json").read_text(encoding="utf-8")
        )
        self.assertEqual(
            zoo.serialise(level),
            zoo.serialise(shipped),
            "the shipped zoo must be exactly what the generator produces",
        )

    def test_adding_an_entry_adds_one_display_without_renumbering(self):
        base_level = self.level_for(self.catalog)
        base_ids = self.display_ids(base_level)

        grown = copy.deepcopy(self.catalog)
        template = next(
            entry for entry in grown["assets"] if entry["id"] == "core:crate"
        )
        extra = copy.deepcopy(template)
        extra["id"] = "fixture:new_prop"
        extra["display_name"] = "Fixture New Prop"
        extra["model"] = "core/props/models/crate.glb"
        extra["size"] = [0.4, 0.4, 0.4]
        grown["assets"].append(extra)

        grown_level = self.level_for(grown)
        grown_ids = self.display_ids(grown_level)
        self.assertEqual(
            len(grown_ids),
            len(base_ids) + 1,
            "one catalogue entry must add exactly one display",
        )
        self.assertIn("zoo:fixture-new_prop:floor", grown_ids)
        # Every existing identity is untouched by the addition.
        for identity in base_ids:
            self.assertIn(identity, grown_ids)
        # The room grew or stayed the same; it never shrank.
        base_room = base_level["rooms"][0]
        grown_room = grown_level["rooms"][0]
        self.assertGreaterEqual(grown_room["width"], base_room["width"])
        self.assertGreaterEqual(grown_room["depth"], base_room["depth"])
        self.assertGreaterEqual(
            len(grown_level["ceiling_lights"]), len(base_level["ceiling_lights"])
        )

    def test_layout_expands_when_the_floor_rows_fill(self):
        base_level = self.level_for(self.catalog)
        base_depth = base_level["rooms"][0]["depth"]

        grown = copy.deepcopy(self.catalog)
        template = next(
            entry for entry in grown["assets"] if entry["id"] == "core:crate"
        )
        for index in range(40):
            extra = copy.deepcopy(template)
            extra["id"] = f"fixture:bulk_{index:02d}"
            extra["display_name"] = f"Fixture Bulk {index}"
            extra["model"] = "core/props/models/crate.glb"
            grown["assets"].append(extra)

        grown_level = self.level_for(grown)
        self.assertGreater(
            grown_level["rooms"][0]["depth"],
            base_depth,
            "forty new floor displays must expand the hall, not overlap",
        )
        # The lighting grid follows the room.
        self.assertGreater(
            len(grown_level["ceiling_lights"]),
            len(base_level["ceiling_lights"]),
        )

    def test_tall_exhibits_clear_the_ceiling_at_native_scale(self):
        level = self.level_for(self.catalog)
        ceiling = level["rooms"][0]["height"]
        for asset_id in ("outdoor:streetlight", "outdoor:tree_01",
                         "outdoor:tree_02", "outdoor:tree_03"):
            entry = next(e for e in self.catalog["assets"] if e["id"] == asset_id)
            self.assertGreaterEqual(ceiling - entry["size"][1], 0.5)
            display = next(p for p in level["props"] if p["model"] == asset_id)
            self.assertEqual(display.get("scale", 1.0), 1.0)
        # Suspended exhibits still attach to the newly resolved ceiling.
        for asset_id in ("core:exit_sign", "home:ball_light"):
            display = next(p for p in level["props"] if p["model"] == asset_id)
            self.assertAlmostEqual(display["y"] + display["size"][1], ceiling)

    def test_ceiling_clears_a_taller_animation_envelope(self):
        inspections = inspections_for(self.catalog)
        inspections["mannequin"]["envelope"] = [[-0.5, 0.0, -0.5], [0.5, 9.0, 0.5]]
        level = zoo.build_level(self.catalog, inspections)
        self.assertGreaterEqual(level["rooms"][0]["height"], 9.5)

    def test_removing_an_entry_removes_only_its_display(self):
        base_level = self.level_for(self.catalog)
        reduced = copy.deepcopy(self.catalog)
        removed_id = "core:plant"
        reduced["assets"] = [
            entry for entry in reduced["assets"] if entry["id"] != removed_id
        ]
        level = self.level_for(reduced)
        ids = self.display_ids(level)
        self.assertNotIn("zoo:core-plant:floor", ids)
        for identity in self.display_ids(base_level):
            if identity != "zoo:core-plant:floor":
                self.assertIn(identity, ids)
        # No real asset was removed: the model still exists on disk.
        self.assertTrue((PACKAGE / "assets" / "core" / "props" / "models" / "plant.glb").exists())

    def test_the_catalog_id_is_the_only_input_that_decides_identity(self):
        # Reordering the catalogue's own array (a file edit that changes nothing
        # semantic) must not move a single display.
        shuffled = copy.deepcopy(self.catalog)
        shuffled["assets"] = list(reversed(shuffled["assets"]))
        level = self.level_for(shuffled)
        base = self.level_for(self.catalog)
        self.assertEqual(self.display_ids(level), self.display_ids(base))

    def test_serial_and_parallel_inspection_agree(self):
        entries = zoo.placeables(self.catalog)
        original = zoo.CACHE_PATH
        try:
            with tempfile.TemporaryDirectory() as directory:
                zoo.CACHE_PATH = Path(directory) / "cache.json"
                serial = zoo.inspect_all(entries, workers=1, use_cache=False)
                zoo.CACHE_PATH = Path(directory) / "cache2.json"
                parallel = zoo.inspect_all(entries, workers=2, use_cache=False)
        finally:
            zoo.CACHE_PATH = original
        self.assertEqual(sorted(serial), sorted(parallel))
        for asset_id in serial:
            self.assertEqual(serial[asset_id], parallel[asset_id], asset_id)

    def test_worker_reduction_and_requested_counts(self):
        workers, note = zoo.parse_worker_count("99")
        self.assertEqual(workers, min(zoo.CPU_CEILING, zoo.usable_cpu_count(),
                                      int(os.environ.get("PLACES_TOOL_WORKERS", "12"))))
        self.assertIn("reduced", note)
        workers, note = zoo.parse_worker_count("1")
        self.assertEqual(workers, 1)
        self.assertEqual(note, "requested")

    def test_a_missing_or_corrupt_model_inspection_is_reported_not_fatal(self):
        # A catalogue entry whose model file is missing or unreadable must not
        # abort generation: the worker reports the asset by name, the display
        # still exists and the loader's own fallback handles it at run time.
        result = zoo.inspect_asset(
            {"id": "fixture:missing", "model": "core/props/models/does_not_exist.glb"}
        )
        self.assertIsNotNone(result["error"])
        self.assertIsNone(result["rest_bounds"])
        self.assertEqual(result["id"], "fixture:missing")


if __name__ == "__main__":
    unittest.main()
