#!/usr/bin/env python3
"""Read-only wall-alignment plan verification and optional native planner tests.

The Python verifier checks a saved plan against the source bytes and rejects
stale or malformed plans. The Rust planner checks real fixtures when a Places
binary is available; set PLACES_BIN to select it. Only plans and private copies
are written in the test scratch directory, never checked-in level sources.
"""

from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

from release.curated_maps import environment_source

TEST_DIR = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(TEST_DIR, ".."))
REPAIR_TOOL = os.path.join(ROOT, "src", "map_compiler", "geometry_check", "repair_alignment.py")
FIXTURES = os.path.join(ROOT, "tests", "fixtures", "levels", "repair")
DEMO = str(environment_source('home', Path(ROOT)))
SCRATCH = os.environ.get("PLACES_REPAIR_TEST_ROOT")

# Job 06 defect D1 belongs to the historical Home envelope, which subsequent
# Home authoring replaced. Keep its geometry independent of current demo array
# indices. This compact repaired control preserves the authored fields (only
# comments omitted) from the retained pre-Home source:
# debug-maps/pool-style-20261007/evidence/before/levels/places_demo.json
# Original walls [32, 35, 19], baseboards [1, 5, 6, 9, 10], floor_regions [11]
# and rooms [5, 6] are reindexed here; no ignored evidence file is needed at run
# time. Reverse the same eight accepted scalar edits to seed the known defect.
HISTORICAL_DEMO_REPAIRED = {
    "schema": "places-level", "format_version": 1,
    "id": "repair_historical_home_south",
    "name": "Historical Home South Repair Control",
    "author": "Places Team",
    "spawn": {"x": 56.0, "z": 8.0, "yaw_degrees": 0.0},
    "defaults": {
        "wall": "core:wallpaper_yellow_01",
        "floor": "core:carpet_beige_01",
        "ceiling": "core:ceiling_panel_01",
    },
    "walls": [
        {"x": 36.15, "z": 14.85, "width": 16.85, "depth": 0.3, "y": -0.9,
         "height": 3.0, "material": "core:wallpaper_yellow_01"},
        {"x": 53.0, "z": 14.85, "width": 12.0, "depth": 0.3, "height": 4.5,
         "y": -0.9, "material": "home:wallpaper_offwhite_01"},
        {"x": 32.85, "z": 14.85, "width": 3.3, "depth": 0.3, "y": -0.9,
         "height": 3.0},
    ],
    "baseboards": [
        {"x": 53.3, "z": 14.87, "length": 0.948, "material": "home:baseboard_white_01",
         "y": -0.9, "rotation_degrees": 90.0},
        {"x": 64.7, "z": 5.8, "length": 9.05, "material": "home:baseboard_white_01",
         "y": -0.9, "rotation_degrees": 270.0},
        {"x": 64.7, "z": 14.85, "length": 11.4, "material": "home:baseboard_white_01",
         "y": -0.9, "rotation_degrees": 180.0},
        {"x": 64.7, "z": 14.85, "length": 6.7, "material": "home:baseboard_wood_01",
         "y": 1.2, "rotation_degrees": 180.0},
        {"x": 64.7, "z": 11.0, "length": 3.85, "material": "home:baseboard_wood_01",
         "y": 1.2, "rotation_degrees": 270.0},
    ],
    "floor_regions": [
        {"x": 58.0, "z": 11.0, "width": 6.75, "depth": 3.9, "offset_y": 2.1,
         "material": "home:hardwood_walnut_02", "edge_material": "home:wall_paint_offwhite_01"},
    ],
    "rooms": [
        {"x": 33.0, "z": 11.0, "width": 20.0, "depth": 4.0, "height": 3.0,
         "floor_y": -0.9, "material": "core:carpet_damp_01",
         "ceiling_material": "core:ceiling_stained_01"},
        {"x": 53.0, "z": 3.0, "width": 12.0, "depth": 12.0, "height": 4.5,
         "floor_y": -0.9, "ceiling": {"kind": "gable", "ridge": "x", "ridge_rise": 1.4},
         "material": "home:hardwood_oak_01", "ceiling_material": "home:ceiling_white_01"},
    ],
}

DEMO_REPAIR_EDITS = (
    ("/walls/1/z", 14.7, 14.85, "wall"),
    ("/baseboards/0/z", 14.72, 14.87, "baseboard-end"),
    ("/baseboards/0/length", 0.798, 0.948, "baseboard-end"),
    ("/baseboards/1/length", 8.9, 9.05, "baseboard-end"),
    ("/baseboards/2/z", 14.7, 14.85, "baseboard-parallel"),
    ("/baseboards/3/z", 14.7, 14.85, "baseboard-parallel"),
    ("/baseboards/4/length", 3.7, 3.85, "baseboard-end"),
    ("/floor_regions/0/depth", 3.75, 3.9, "floor-tuck"),
)

sys.path.insert(0, os.path.dirname(REPAIR_TOOL))
import repair_alignment as ra  # noqa: E402


def places_binary() -> str:
    configured = os.environ.get("PLACES_BIN")
    if configured:
        return configured
    name = "places.exe" if os.name == "nt" else "places"
    for profile in ("release", "debug"):
        path = os.path.join(ROOT, "target", profile, name)
        if os.path.isfile(path) and os.access(path, os.X_OK):
            return path
    raise unittest.SkipTest("No native places binary; set PLACES_BIN to exercise the Rust planner.")


def run(command: list, **kwargs) -> subprocess.CompletedProcess:
    return subprocess.run(command, capture_output=True, text=True, cwd=ROOT, check=False, **kwargs)




class RepairScratch(unittest.TestCase):
    """Base class: one scratch directory per test, cleaned up afterwards."""

    def setUp(self) -> None:
        if SCRATCH:
            os.makedirs(SCRATCH, exist_ok=True)
        self.work = tempfile.mkdtemp(prefix="repair-", dir=SCRATCH)

    def tearDown(self) -> None:
        shutil.rmtree(self.work, ignore_errors=True)

    def copy_fixture(self, name: str) -> str:
        source = os.path.join(self.work, f"{name}.json")
        shutil.copyfile(os.path.join(FIXTURES, f"{name}.json"), source)
        return source

    def load_fixture(self, name: str) -> dict:
        with open(os.path.join(FIXTURES, f"{name}.json"), "r", encoding="utf-8") as handle:
            return json.load(handle)

    def write_level(self, name: str, document: dict) -> str:
        path = os.path.join(self.work, f"{name}.json")
        with open(path, "wb") as handle:
            handle.write((json.dumps(document, indent=2, ensure_ascii=False) + "\n").encode("utf-8"))
        return path

    def seed_demo_repair(self) -> str:
        """Seed the historical D1 geometry without relying on current demo indices."""
        document = json.loads(json.dumps(HISTORICAL_DEMO_REPAIRED))
        for pointer, old, new, _coupled in DEMO_REPAIR_EDITS:
            current = ra.resolve_pointer(document, pointer)
            self.assertTrue(
                ra.values_match(current, new),
                f"{pointer}: the historical control carries {current!r}, expected {new!r}",
            )
            parent, key = pointer.rsplit("/", 1)
            ra.resolve_pointer(document, parent)[key] = old
        return self.write_level("historical_home_south", document)

    def assert_demo_repair_plan(self, plan: dict) -> None:
        """The historical Home-south-wall plan: one step and the exact eight edits."""
        self.assertEqual(len(plan["findings"]), 1, plan["findings"])
        finding = plan["findings"][0]
        self.assertEqual((finding["first"], finding["second"]), (0, 1))
        self.assertEqual(finding["kind"], "step")
        self.assertTrue(finding["auto_repairable"])
        self.assertAlmostEqual(finding["shift"], 0.15, places=4)
        self.assertEqual(finding["authority"], "chain")
        self.assertEqual((finding["first_support"], finding["second_support"]), (1, 0))
        self.assertEqual(
            [edit["pointer"] for edit in plan["edits"]],
            [pointer for pointer, _old, _new, _coupled in DEMO_REPAIR_EDITS],
        )
        for edit, (pointer, old, new, coupled) in zip(plan["edits"], DEMO_REPAIR_EDITS):
            self.assertEqual(edit["pointer"], pointer)
            self.assertAlmostEqual(edit["old"], old, places=4)
            self.assertAlmostEqual(edit["new"], new, places=4)
            self.assertEqual(edit["coupled"], coupled)
            self.assertEqual(edit["finding"], 0)
        self.assertEqual(plan["post_check"]["errors"], 0)
        self.assertEqual(plan["review"], [])

    def plan_for(self, source: str) -> dict:
        plan_path = os.path.join(self.work, "plan.json")
        result = run(
            [
                places_binary(),
                "--repair-geometry",
                "--level",
                source,
                "--plan",
                plan_path,
            ]
        )
        self.assertIn(
            result.returncode, (0, 1), f"planner failed: {result.stdout}{result.stderr}"
        )
        with open(plan_path, "r", encoding="utf-8") as handle:
            return json.load(handle)

    def verify(self, plan_path: str) -> subprocess.CompletedProcess:
        return run([sys.executable, REPAIR_TOOL, "--plan", plan_path, "--check"])

    def write_probe_plan(self, source: str) -> str:
        # Keep the actual checked-in wall-step fixture untouched. A single
        # known field is enough to cover the verifier without embedding native
        # planner logic or requiring a compiled game for these cases.
        raw = self.read(source)
        document = json.loads(raw)
        plan_path = os.path.join(self.work, "plan.json")
        with open(plan_path, "w", encoding="utf-8") as handle:
            json.dump({
                "format": ra.PLAN_FORMAT, "version": ra.PLAN_VERSION,
                "level": {"source": source, "sha256": hashlib.sha256(raw).hexdigest()},
                "edits": [{"pointer": "/walls/1/z", "old": document["walls"][1]["z"],
                           "new": 2.15}],
            }, handle)
        return plan_path

    def read(self, path: str) -> bytes:
        with open(path, "rb") as handle:
            return handle.read()

    def temp_siblings(self) -> list:
        return sorted(
            name
            for name in os.listdir(self.work)
            if name.startswith(".") and "places-repair" in name
        )


class RepairVerifierTests(RepairScratch):
    def test_check_verifies_a_plan_and_writes_nothing(self) -> None:
        source = self.copy_fixture("wall_step_x")
        before = self.read(source)
        plan_path = self.write_probe_plan(source)
        plan_before = self.read(plan_path)
        result = self.verify(plan_path)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("1 edit(s) verified", result.stdout)
        self.assertEqual((self.read(source), self.read(plan_path)), (before, plan_before))
        self.assertEqual(self.temp_siblings(), [])

    def test_removed_write_mode_is_rejected_without_touching_source(self) -> None:
        source = self.copy_fixture("wall_step_x")
        plan_path = self.write_probe_plan(source)
        before = (self.read(source), self.read(plan_path))
        rejected = run([sys.executable, REPAIR_TOOL, "--plan", plan_path, "--apply"])
        self.assertEqual(rejected.returncode, 2)
        self.assertIn("--check", rejected.stderr)
        self.assertEqual((self.read(source), self.read(plan_path)), before)


    def test_concurrent_change_is_refused(self) -> None:
        source = self.copy_fixture("wall_step_x")
        plan_path = self.write_probe_plan(source)
        with open(source, "r", encoding="utf-8") as handle:
            document = json.load(handle)
        document["walls"][0]["z"] = 2.2
        with open(source, "w", encoding="utf-8") as handle:
            json.dump(document, handle, indent=2)
            handle.write("\n")
        changed = self.read(source)
        result = self.verify(plan_path)
        self.assertEqual(result.returncode, 1, result.stdout)
        self.assertIn("sha256", result.stdout)
        self.assertEqual(self.read(source), changed)

    def test_stale_old_value_is_refused(self) -> None:
        source = self.copy_fixture("wall_step_x")
        plan_path = self.write_probe_plan(source)
        with open(plan_path, "r", encoding="utf-8") as handle:
            plan = json.load(handle)
        plan["edits"][0]["old"] = 99.9
        with open(plan_path, "w", encoding="utf-8") as handle:
            json.dump(plan, handle, indent=2)
        before = self.read(source)
        result = self.verify(plan_path)
        self.assertEqual(result.returncode, 1, result.stdout)
        self.assertIn("expects", result.stdout)
        self.assertEqual(self.read(source), before)

    def test_clean_map_plans_nothing_and_writes_nothing(self) -> None:
        source = self.copy_fixture("clean")
        before = self.read(source)
        plan = self.plan_for(source)
        self.assertEqual(plan["findings"], [])
        self.assertEqual(plan["edits"], [])
        plan_path = os.path.join(self.work, "plan.json")
        result = self.verify(plan_path)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(self.read(source), before)
        self.assertEqual(self.temp_siblings(), [])

    def test_a_review_only_plan_makes_no_writes(self) -> None:
        source = self.copy_fixture("gap_review")
        before = self.read(source)
        plan = self.plan_for(source)
        self.assertTrue(plan["findings"], "the door gap must be a review candidate")
        self.assertEqual(plan["edits"], [])
        plan_path = os.path.join(self.work, "plan.json")
        result = self.verify(plan_path)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("0 edit(s) verified", result.stdout)
        self.assertEqual(self.read(source), before)



    def test_the_demo_source_plans_only_its_current_state(self) -> None:
        # Mirrors the Rust test `the_shipped_demo_has_no_confirmed_geometry_defects`.
        # The maintained source must plan nothing and verify cleanly. Historical
        # D1 acceptance is tested independently, never allowed as a source regression.
        before = self.read(DEMO)
        plan = self.plan_for(DEMO)
        self.assertEqual(self.read(DEMO), before, "the planner must be read-only")
        plan_path = os.path.join(self.work, "plan.json")
        checked = self.verify(plan_path)
        self.assertEqual(checked.returncode, 0, checked.stdout + checked.stderr)
        self.assertEqual(self.read(DEMO), before)
        self.assertEqual(plan["findings"], [])
        self.assertEqual(plan["edits"], [])
        self.assertEqual(plan["review"], [])
        self.assertEqual(plan["post_check"]["errors"], 0)

    def test_the_seeded_demo_repair_plan_is_pinned_and_source_is_unchanged(self) -> None:
        maintained_before = self.read(DEMO)
        source = self.seed_demo_repair()
        before = self.read(source)
        plan = self.plan_for(source)
        self.assert_demo_repair_plan(plan)
        result = self.verify(os.path.join(self.work, "plan.json"))
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(self.read(source), before)
        self.assertEqual(self.read(DEMO), maintained_before)
        self.assertEqual(self.temp_siblings(), [])


    def test_malformed_pointer_is_refused_and_mutates_nothing(self) -> None:
        source = self.copy_fixture("wall_step_x")
        plan_path = self.write_probe_plan(source)
        before = self.read(source)
        with open(plan_path, "r", encoding="utf-8") as handle:
            plan = json.load(handle)
        for pointer in ("walls/1/z", "/walls/99/z"):
            corrupted = json.loads(json.dumps(plan))
            corrupted["edits"][0]["pointer"] = pointer
            with open(plan_path, "w", encoding="utf-8") as handle:
                json.dump(corrupted, handle, indent=2)
            result = self.verify(plan_path)
            self.assertEqual(result.returncode, 1, f"{pointer}: {result.stdout}")
            self.assertIn("REFUSED", result.stdout)
            self.assertEqual(self.read(source), before)
        self.assertEqual(self.temp_siblings(), [])

    def test_post_check_refusal_leaves_the_original_bytes(self) -> None:
        # The Rust planner must report the unrelated, unrepairable error
        # even though it can propose a repair for the original wall step.
        fixture = self.load_fixture("wall_step_x")
        duplicate = {"x": 20.0, "z": 20.0, "width": 1.0, "depth": 0.3, "y": 0.0, "height": 3.0}
        fixture["walls"].extend([dict(duplicate), dict(duplicate)])
        fixture["id"] = "repair_post_check_refusal"
        source = self.write_level("doomed", fixture)
        before = self.read(source)
        plan = self.plan_for(source)
        self.assertTrue(plan["findings"], plan)
        self.assertTrue(plan["edits"], plan)
        self.assertGreater(
            plan["post_check"]["errors"], 0, "the defect must stay unrepairable"
        )
        plan_path = os.path.join(self.work, "plan.json")
        checked = self.verify(plan_path)
        self.assertEqual(checked.returncode, 0, checked.stdout)
        self.assertEqual(self.read(source), before)
        self.assertEqual(self.temp_siblings(), [])

    def test_review_finding_remains_visible_beside_the_wall_step(self) -> None:
        # A review-only coplanar doorway gap survives the Rust planner's preview
        # of the wall step correction.
        fixture = self.load_fixture("wall_step_x")
        fixture["walls"].extend(
            [
                {"x": 20.0, "z": 2.0, "width": 2.0, "depth": 0.3, "y": 0.0, "height": 3.0},
                {"x": 22.25, "z": 2.0, "width": 2.0, "depth": 0.3, "y": 0.0, "height": 3.0},
            ]
        )
        fixture["id"] = "repair_step_plus_review"
        source = self.write_level("step_plus_review", fixture)
        before = self.read(source)
        plan = self.plan_for(source)
        self.assertTrue(plan["edits"], plan)
        self.assertEqual(len(plan["findings"]), 2, plan["findings"])
        self.assertTrue(
            any(finding["kind"] == "near-step" for finding in plan["findings"]),
            plan["findings"],
        )
        result = self.verify(os.path.join(self.work, "plan.json"))
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(self.read(source), before)
        self.assertEqual(self.temp_siblings(), [])


if __name__ == "__main__":
    unittest.main()
