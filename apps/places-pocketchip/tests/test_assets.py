"""Check that the published asset graph can be loaded by the game."""

import importlib.util
import json
import unittest
from pathlib import Path


PACKAGE = Path(__file__).resolve().parent.parent
VALIDATOR = PACKAGE / "tools" / "assets" / "validate.py"


class PublishedAssetsTests(unittest.TestCase):
    def test_catalog_and_levels_resolve(self):
        spec = importlib.util.spec_from_file_location("places_asset_validator", VALIDATOR)
        self.assertIsNotNone(spec)
        self.assertIsNotNone(spec.loader)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)

        catalog = json.loads((PACKAGE / "assets" / "catalog.json").read_text())
        catalog_errors, _ = module.validate_catalog(catalog, str(PACKAGE / "assets"))
        level_errors, _ = module.validate_levels(catalog)
        self.assertEqual([], catalog_errors + level_errors)


if __name__ == "__main__":
    unittest.main()
