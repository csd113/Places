"""Preserve the dense pressure witness as the catalogue grows, with real ID validation."""
import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("capacity_generator", ROOT / "tools/levels/build_capacity_fixtures.py")
CAPACITY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CAPACITY)
FIXTURE = ROOT / "tests/fixtures/levels/capacity_dense.json"


class CapacityGeneratorWitnessTests(unittest.TestCase):
    def generate(self, placeables):
        return CAPACITY.build_dense(CAPACITY.Rng(0x44_45_4E_53), placeables)

    def test_current_catalogue_reproduces_exact_maintained_fixture_and_pressure(self):
        generated = self.generate(CAPACITY.load_placeables())
        self.assertEqual((json.dumps(generated, indent=2) + "\n").encode(), FIXTURE.read_bytes())
        self.assertEqual(len(generated["props"]), 5312)
        self.assertEqual(len({prop["model"] for prop in generated["props"]}), 48)
        self.assertEqual(sum(prop.get("solid", False) for prop in generated["props"]), 2760)
        self.assertEqual(len(generated["ceiling_lights"]), 120)
        self.assertEqual(len(generated["routes"]), 18)

    def test_catalogue_growth_order_and_metadata_do_not_reduce_existing_pressure(self):
        placeables = copy.deepcopy(CAPACITY.load_placeables())
        for entry in placeables:
            entry.update(size=[9.0, 9.0, 9.0], solid=False)
        placeables.reverse()
        placeables.append(dict(id="test:unrelated_addition", asset_type="prop", model="unused.glb",
                               size=[1.0, 1.0, 1.0], solid=False))
        generated = self.generate(placeables)
        self.assertEqual((json.dumps(generated, indent=2) + "\n").encode(), FIXTURE.read_bytes())

    def test_removed_or_nonplaceable_current_witness_id_is_refused(self):
        current = json.loads((ROOT / "assets/catalog.json").read_text())
        witness = CAPACITY.DENSE_PLACEABLE_SPECS[0]["id"]
        for change in ("remove", "type", "model"):
            catalog = copy.deepcopy(current)
            if change == "remove":
                catalog["assets"] = [entry for entry in catalog["assets"] if entry["id"] != witness]
            else:
                entry = next(entry for entry in catalog["assets"] if entry["id"] == witness)
                if change == "type":
                    entry["asset_type"] = "material"
                else:
                    entry.pop("model")
            with tempfile.TemporaryDirectory() as directory:
                path = Path(directory) / "catalog.json"
                path.write_text(json.dumps(catalog))
                with patch.object(CAPACITY, "CATALOG_PATH", str(path)):
                    with self.assertRaisesRegex(ValueError, "dense witness models are missing from catalog placeables: " + witness):
                        self.generate(CAPACITY.load_placeables())


if __name__ == "__main__":
    unittest.main()
