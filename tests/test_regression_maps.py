"""Inventory scope and command safety; actual results come from the native campaign."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("regression_maps", ROOT / "tools/bench/regression_maps.py")
MAPS = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MAPS)


class MapRegressionInventoryTests(unittest.TestCase):
    def test_progressive_order_keeps_every_actual_path_and_coincident_id(self):
        rows = MAPS.inventory(ROOT)
        self.assertEqual([row["source"] for row in rows[:6]], list(MAPS.PROGRESSIVE_SOURCES))
        actual = {path.relative_to(ROOT).as_posix() for directory in MAPS.MAP_ROOTS
                  for path in (ROOT / directory).rglob("*.json")}
        self.assertEqual({row["source"] for row in rows}, actual)
        self.assertEqual(len(rows), len(actual))
        remaining = [row["source"] for row in rows[6:]]
        self.assertEqual(remaining, sorted(remaining))
        self.assertGreater(sum(row["level"] == "art_style_hero" for row in rows), 1)

    def test_nested_controls_and_coincident_level_ids_are_not_omitted(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name in ("first", "nested/second"):
                path = root / "tests/fixtures/levels" / (name + ".json")
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(json.dumps(dict(id="same_id", spawn={})))
            rows = MAPS.inventory(root)
            self.assertEqual(len(rows), 2)
            self.assertEqual(len({row["key"] for row in rows}), 2)
            self.assertEqual({row["level"] for row in rows}, {"same_id"})

    def test_intended_negative_and_synthetic_cpu_contracts_are_explicit(self):
        rows = MAPS.inventory(ROOT)
        by_source = {row["source"]: row for row in rows}
        invalid = by_source["tests/fixtures/levels/invalid/geometry_invalid.json"]
        boundary = by_source["tests/fixtures/levels/capacity_beyond_former_limits.json"]
        broken = by_source["tests/fixtures/levels/geometry_broken.json"]
        self.assertFalse(MAPS.supported(invalid))
        self.assertFalse(MAPS.supported(boundary))
        self.assertTrue(MAPS.supported(broken))
        self.assertIn("synthetic", boundary["scope_reason"])
        self.assertIn("Loader-valid", broken["scope_reason"])

    def test_normal_compile_does_not_reduce_variants_or_mutate_source_siblings(self):
        row = dict(key="fixture__one", source="tests/fixtures/levels/one.json", classification="playable")
        commands = MAPS.planned_commands(row, Path("/compiler"), Path("/evidence"))
        self.assertEqual([command[1] for command in commands], ["build", "verify", "validate"])
        self.assertNotIn("--variants", commands[0])
        self.assertEqual(commands[0][commands[0].index("--out") + 1], "/evidence/packages/fixture__one.placesmap")
        self.assertIn("--require-current", commands[1])

    def test_evidence_is_never_overwritten(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "receipt.json"
            MAPS.write_new(path, dict(first=True))
            with self.assertRaises(FileExistsError):
                MAPS.write_new(path, dict(first=False))
            self.assertEqual(json.loads(path.read_text()), dict(first=True))

    def test_installation_is_explicit_and_fixtures_reuse_a_stable_prepared_root(self):
        shipped = dict(key="shipped", source="assets/levels/demo.json", classification="playable")
        fixture = dict(key="fixture", source="tests/fixtures/levels/sample.json", classification="playable")
        compiler, out, prepared = Path("/compiler"), Path("/evidence"), Path("/stable")
        for row, expected in ((shipped, str(ROOT / "assets/levels/demo.placesmap")),
                              (fixture, "/stable/fixture.placesmap")):
            command = MAPS.planned_commands(row, compiler, out, prepared_root=prepared,
                                            install_source_packages=True)[0]
            self.assertEqual(command[command.index("--out") + 1], expected)
        command = MAPS.planned_commands(shipped, compiler, out)[0]
        self.assertEqual(command[command.index("--out") + 1], "/evidence/packages/shipped.placesmap")


if __name__ == "__main__":
    unittest.main()
