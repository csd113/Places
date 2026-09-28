#!/usr/bin/env python3
"""Applier tests for the wall-alignment repair plans (``tools/levels/repair_alignment.py``).

The applier is the write side of ``places --repair-geometry``: it verifies a
plan against the current bytes, applies its JSON-pointer edits with order
preserving serialization, checks the result, and refuses anything it cannot
prove. These tests drive the real ``places`` executable (set ``PLACES_BIN`` to
choose one; release is preferred) on scratch copies of the repair fixtures under
``target/agent-work/06-wall-alignment-audit-and-repair/runs/agent-a/`` — no
maintained source is ever touched.

Covered: dry-run mutates nothing, apply is atomic, concurrent change and stale
``old`` values are refused, malformed pointers are refused, a non-clean
post-check refuses before publishing, a non-empty second plan restores the
published bytes, a clean map writes nothing, unrelated fields and key order
survive byte-for-byte, and the wall-37 acceptance case (both the current
maintained source and a seeded pre-repair scratch copy).
"""

from __future__ import annotations

import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest

TEST_DIR = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(TEST_DIR, ".."))
REPAIR_TOOL = os.path.join(ROOT, "tools", "levels", "repair_alignment.py")
FIXTURES = os.path.join(ROOT, "tests", "fixtures", "levels", "repair")
DEMO = os.path.join(ROOT, "assets", "levels", "places_demo.json")
MAINTAINED = (
    os.path.join(ROOT, "assets", "levels", "places_demo.json"),
    os.path.join(ROOT, "assets", "levels", "model_zoo.json"),
    os.path.join(ROOT, "levels", "level0_pit.json"),
)
SCRATCH = os.path.join(
    ROOT, "target", "agent-work", "06-wall-alignment-audit-and-repair", "runs", "agent-a", "python-tests"
)

# The confirmed wall-37 repair plan (Job 06 defect D1): the exact pointers,
# values and coupling of the accepted plan. The maintained source was repaired
# with it, so the demo test reconstructs the pre-repair state from its inverse.
DEMO_REPAIR_EDITS = (
    ("/walls/37/z", 14.7, 14.85, "wall"),
    ("/baseboards/3/z", 14.72, 14.87, "baseboard-end"),
    ("/baseboards/3/length", 0.798, 0.948, "baseboard-end"),
    ("/baseboards/10/length", 2.9713333, 3.1213, "baseboard-end"),
    ("/baseboards/11/z", 14.7, 14.85, "baseboard-parallel"),
    ("/baseboards/12/z", 14.7, 14.85, "baseboard-parallel"),
    ("/baseboards/13/z", 14.7, 14.85, "baseboard-parallel"),
    ("/baseboards/16/z", 14.7, 14.85, "baseboard-parallel"),
    ("/baseboards/17/z", 14.7, 14.85, "baseboard-parallel"),
    ("/baseboards/18/length", 3.7, 3.85, "baseboard-end"),
    ("/floor_regions/11/depth", 3.75, 3.9, "floor-tuck"),
)

sys.path.insert(0, os.path.join(ROOT, "tools", "levels"))
import repair_alignment as ra  # noqa: E402


def places_binary() -> str:
    try:
        return ra.places_binary()
    except ra.Refused as error:
        raise unittest.SkipTest(str(error)) from error


def run(command: list, **kwargs) -> subprocess.CompletedProcess:
    return subprocess.run(command, capture_output=True, text=True, cwd=ROOT, check=False, **kwargs)


def flatten(document, trail: str = "") -> dict:
    """Every scalar leaf keyed by its JSON pointer."""
    leaves = {}
    if isinstance(document, dict):
        for key, value in document.items():
            leaves.update(flatten(value, f"{trail}/{key}"))
    elif isinstance(document, list):
        for index, value in enumerate(document):
            leaves.update(flatten(value, f"{trail}/{index}"))
    else:
        leaves[trail] = document
    return leaves


class RepairScratch(unittest.TestCase):
    """Base class: one scratch directory per test, cleaned up afterwards."""

    def setUp(self) -> None:
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
            handle.write(ra.serialise(document, True))
        return path

    def seed_demo_repair(self) -> str:
        """A scratch demo copy in the confirmed pre-repair (D1) state.

        The maintained source was repaired by Agent C, so this reconstructs the
        defect from the accepted plan's own inverse: a pointer still at its
        repaired value is put back to the plan's ``old``. A source that is
        already pre-repair is copied as-is; anything else fails loudly.
        """
        source = os.path.join(self.work, "places_demo.json")
        shutil.copyfile(DEMO, source)
        with open(source, "r", encoding="utf-8") as handle:
            document = json.load(handle)
        for pointer, old, new, _coupled in DEMO_REPAIR_EDITS:
            current = ra.resolve_pointer(document, pointer)
            if ra.values_match(current, new):
                ra.assign_pointer(document, pointer, old)
            else:
                self.assertTrue(
                    ra.values_match(current, old),
                    f"{pointer}: the demo carries {current!r}, expected the repaired "
                    f"{new!r} or the seeded {old!r}",
                )
        with open(source, "wb") as handle:
            handle.write(ra.serialise(document, True))
        return source

    def assert_demo_repair_plan(self, plan: dict) -> None:
        """The accepted wall-37 plan: one step finding and the exact 11 edits."""
        self.assertEqual(len(plan["findings"]), 1, plan["findings"])
        finding = plan["findings"][0]
        self.assertEqual((finding["first"], finding["second"]), (31, 37))
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

    def applier(self, plan_path: str, mode: str) -> subprocess.CompletedProcess:
        return run([sys.executable, REPAIR_TOOL, "--plan", plan_path, mode])

    def read(self, path: str) -> bytes:
        with open(path, "rb") as handle:
            return handle.read()

    def temp_siblings(self) -> list:
        return sorted(
            name
            for name in os.listdir(self.work)
            if name.startswith(".") and "places-repair" in name
        )


class RepairApplierTests(RepairScratch):
    def test_dry_run_verifies_every_edit_and_writes_nothing(self) -> None:
        source = self.copy_fixture("wall_step_x")
        before = self.read(source)
        self.plan_for(source)
        plan_path = os.path.join(self.work, "plan.json")
        result = self.applier(plan_path, "--check")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(self.read(source), before)
        self.assertEqual(self.temp_siblings(), [])
        self.assertIn("4 edit(s) verified", result.stdout)

    def test_apply_is_atomic_and_a_second_apply_is_refused(self) -> None:
        source = self.copy_fixture("wall_step_x")
        self.plan_for(source)
        plan_path = os.path.join(self.work, "plan.json")
        result = self.applier(plan_path, "--apply")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(self.temp_siblings(), [], "the temp file must not survive")
        applied = self.read(source)
        self.assertNotEqual(applied, b"")
        # The plan's hash is now stale: applying it again must refuse and never
        # change the already-repaired bytes.
        again = self.applier(plan_path, "--apply")
        self.assertEqual(again.returncode, 1, again.stdout)
        self.assertIn("REFUSED", again.stdout)
        self.assertEqual(self.read(source), applied)

    def test_apply_fixes_the_joint_and_the_second_plan_is_empty(self) -> None:
        source = self.copy_fixture("wall_step_x")
        self.plan_for(source)
        result = self.applier(os.path.join(self.work, "plan.json"), "--apply")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        with open(source, "r", encoding="utf-8") as handle:
            fixed = json.load(handle)
        self.assertEqual(fixed["walls"][1]["z"], 2.15)
        self.assertEqual(fixed["baseboards"][0]["z"], 2.15)
        self.assertEqual(fixed["baseboards"][1]["length"], 0.77)
        self.assertEqual(fixed["floor_regions"][0]["depth"], 1.7)
        second = run(
            [places_binary(), "--repair-geometry", "--level", source, "--json"]
        )
        self.assertEqual(second.returncode, 0, second.stdout + second.stderr)
        self.assertEqual(json.loads(second.stdout)["edits"], [])

    def test_concurrent_change_is_refused(self) -> None:
        source = self.copy_fixture("wall_step_x")
        self.plan_for(source)
        plan_path = os.path.join(self.work, "plan.json")
        with open(source, "r", encoding="utf-8") as handle:
            document = json.load(handle)
        document["walls"][0]["z"] = 2.2
        with open(source, "w", encoding="utf-8") as handle:
            json.dump(document, handle, indent=2)
            handle.write("\n")
        changed = self.read(source)
        for mode in ("--check", "--apply"):
            result = self.applier(plan_path, mode)
            self.assertEqual(result.returncode, 1, f"{mode}: {result.stdout}")
            self.assertIn("sha256", result.stdout)
            self.assertEqual(self.read(source), changed)

    def test_stale_old_value_is_refused(self) -> None:
        source = self.copy_fixture("wall_step_x")
        self.plan_for(source)
        plan_path = os.path.join(self.work, "plan.json")
        with open(plan_path, "r", encoding="utf-8") as handle:
            plan = json.load(handle)
        plan["edits"][0]["old"] = 99.9
        with open(plan_path, "w", encoding="utf-8") as handle:
            json.dump(plan, handle, indent=2)
        before = self.read(source)
        result = self.applier(plan_path, "--check")
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
        result = self.applier(plan_path, "--apply")
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
        result = self.applier(plan_path, "--apply")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("no edits", result.stdout)
        self.assertEqual(self.read(source), before)

    def test_unrelated_fields_and_key_order_are_byte_preserved(self) -> None:
        source = self.copy_fixture("wall_step_x")
        plan = self.plan_for(source)
        plan_path = os.path.join(self.work, "plan.json")
        with open(source, "r", encoding="utf-8") as handle:
            before_text = handle.read()
        result = self.applier(plan_path, "--apply")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        with open(source, "r", encoding="utf-8") as handle:
            after_text = handle.read()
        before = json.loads(before_text)
        after = json.loads(after_text)
        changed = {
            pointer
            for pointer, value in flatten(before).items()
            if flatten(after).get(pointer) != value
        }
        expected = {edit["pointer"] for edit in plan["edits"]}
        self.assertEqual(changed, expected)
        # Insertion order is untouched at every object level.
        self.assertEqual(list(before.keys()), list(after.keys()))
        self.assertEqual(list(before["walls"][1].keys()), list(after["walls"][1].keys()))
        self.assertEqual(
            list(before["baseboards"][0].keys()), list(after["baseboards"][0].keys())
        )

    def test_serialiser_round_trips_the_maintained_sources_byte_identically(self) -> None:
        for path in MAINTAINED:
            with open(path, "rb") as handle:
                raw = handle.read()
            rendered = ra.serialise(json.loads(raw), raw.endswith(b"\n"))
            self.assertEqual(
                hashlib.sha256(rendered).hexdigest(),
                hashlib.sha256(raw).hexdigest(),
                f"{path} does not round-trip byte-identically",
            )

    def test_the_demo_source_plans_only_its_current_state(self) -> None:
        # Mirrors the Rust test `the_shipped_demo_reports_only_the_confirmed_wall_step`:
        # the maintained source is currently repaired, so it must plan nothing
        # and verify cleanly; if it ever regresses, the pre-repair shape below
        # is asserted exactly. The plan is always re-checked against the real
        # source in place (read-only).
        before = self.read(DEMO)
        plan = self.plan_for(DEMO)
        self.assertEqual(self.read(DEMO), before, "the planner must be read-only")
        plan_path = os.path.join(self.work, "plan.json")
        checked = self.applier(plan_path, "--check")
        self.assertEqual(checked.returncode, 0, checked.stdout + checked.stderr)
        self.assertEqual(self.read(DEMO), before)
        if plan["findings"]:
            # A source regression: only the known wall-37 step may appear.
            self.assert_demo_repair_plan(plan)
        else:
            self.assertEqual(plan["edits"], [])
            self.assertEqual(plan["review"], [])
            self.assertEqual(plan["post_check"]["errors"], 0)

    def test_the_seeded_demo_repair_plan_and_apply_are_pinned(self) -> None:
        source = self.seed_demo_repair()
        before = self.read(source)
        plan = self.plan_for(source)
        self.assertEqual(before, self.read(source), "the planner must be read-only")
        self.assert_demo_repair_plan(plan)

        result = self.applier(os.path.join(self.work, "plan.json"), "--apply")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(self.temp_siblings(), [])
        with open(source, "r", encoding="utf-8") as handle:
            fixed = json.load(handle)
        self.assertEqual(fixed["walls"][37]["z"], 14.85)
        self.assertEqual(fixed["baseboards"][3]["z"], 14.87)
        self.assertEqual(fixed["baseboards"][3]["length"], 0.948)
        self.assertEqual(fixed["baseboards"][10]["length"], 3.1213)
        self.assertEqual(fixed["baseboards"][11]["z"], 14.85)
        self.assertEqual(fixed["baseboards"][16]["z"], 14.85)
        self.assertEqual(fixed["baseboards"][18]["length"], 3.85)
        self.assertEqual(fixed["floor_regions"][11]["depth"], 3.9)

        second = run(
            [places_binary(), "--repair-geometry", "--level", source, "--json"]
        )
        self.assertEqual(second.returncode, 0, second.stdout + second.stderr)
        self.assertEqual(json.loads(second.stdout)["findings"], [])
        self.assertEqual(json.loads(second.stdout)["edits"], [])

    def test_malformed_pointer_is_refused_and_mutates_nothing(self) -> None:
        source = self.copy_fixture("wall_step_x")
        self.plan_for(source)
        plan_path = os.path.join(self.work, "plan.json")
        before = self.read(source)
        with open(plan_path, "r", encoding="utf-8") as handle:
            plan = json.load(handle)
        for pointer in ("walls/1/z", "/walls/99/z"):
            corrupted = json.loads(json.dumps(plan))
            corrupted["edits"][0]["pointer"] = pointer
            with open(plan_path, "w", encoding="utf-8") as handle:
                json.dump(corrupted, handle, indent=2)
            for mode in ("--check", "--apply"):
                result = self.applier(plan_path, mode)
                self.assertEqual(
                    result.returncode, 1, f"{pointer} {mode}: {result.stdout}"
                )
                self.assertIn("REFUSED", result.stdout)
                self.assertEqual(self.read(source), before)
        self.assertEqual(self.temp_siblings(), [])

    def test_post_check_refusal_leaves_the_original_bytes(self) -> None:
        # Two identical extra walls are an unrelated, unrepairable confirmed
        # error, so the level can never pass --check-geometry even after the
        # plan's own step edit. The applier's own gate must refuse before any
        # os.replace and leave the bytes exactly as they were.
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
        checked = self.applier(plan_path, "--check")
        self.assertEqual(checked.returncode, 0, checked.stdout)
        applied = self.applier(plan_path, "--apply")
        self.assertEqual(applied.returncode, 1, applied.stdout)
        self.assertIn("does not pass --check-geometry", applied.stdout)
        self.assertEqual(
            self.read(source), before, "the refusal must happen before os.replace"
        )
        self.assertEqual(self.temp_siblings(), [])

    def test_restore_after_a_non_empty_second_plan(self) -> None:
        # A review-only coplanar doorway gap plus the wall step: the plan's own
        # edit is applied and passes the checker gate (warnings only), so the
        # applier publishes it, but the second plan can never be empty and the
        # applier must restore the bytes it just wrote.
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
        applied = self.applier(os.path.join(self.work, "plan.json"), "--apply")
        self.assertEqual(applied.returncode, 1, applied.stdout)
        self.assertIn("the second plan is not empty", applied.stdout)
        self.assertIn("restored the original bytes", applied.stdout)
        self.assertEqual(self.read(source), before)
        self.assertEqual(self.temp_siblings(), [])


if __name__ == "__main__":
    unittest.main()
