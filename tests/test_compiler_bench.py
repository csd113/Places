"""Measurement correctness for the offline compiler benchmark harness."""
from datetime import datetime
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
import zipfile

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("compiler_bench", ROOT / "tools/bench/compiler_bench.py")
BENCH = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BENCH)
COMPARE_SPEC = importlib.util.spec_from_file_location("compare_packages", ROOT / "tools/bench/compare_packages.py")
COMPARE = importlib.util.module_from_spec(COMPARE_SPEC)
COMPARE_SPEC.loader.exec_module(COMPARE)


class CompilerBenchTests(unittest.TestCase):
    def test_native_identity_uses_canonical_manifest_not_zip_packaging(self):
        import hashlib
        with tempfile.TemporaryDirectory() as directory:
            paths = []
            manifest = b'{"compiler_fingerprint":"same","entries":[]}\n'
            for name in ["first", "second"]:
                path = Path(directory) / (name + ".placesmap")
                with zipfile.ZipFile(path, "w") as archive:
                    archive.writestr("manifest.json", manifest)
                    archive.comment = name.encode()
                paths.append(path)
            self.assertNotEqual(hashlib.sha256(paths[0].read_bytes()).hexdigest(),
                                hashlib.sha256(paths[1].read_bytes()).hexdigest())
            expected = hashlib.sha256(manifest).hexdigest()
            self.assertEqual(BENCH.native_package_identity(paths[0]), expected)
            self.assertEqual(BENCH.native_package_identity(paths[1]), expected)

    def test_paired_builds_alternate_order_without_skipping_either_binary(self):
        spec = importlib.util.spec_from_file_location("compiler_compare", ROOT / "tools/bench/compiler_compare.py")
        module = importlib.util.module_from_spec(spec)
        sys.path.insert(0, str(ROOT / "tools/bench"))
        try:
            spec.loader.exec_module(module)
        finally:
            sys.path.pop(0)
        self.assertEqual(module.paired_order(0), ["baseline", "optimized"])
        self.assertEqual(module.paired_order(1), ["optimized", "baseline"])
        self.assertEqual(module.paired_order(2), ["baseline", "optimized"])

    def test_parser_retains_nested_phase_timings_and_workloads(self):
        report = {"rebuilt": True, "phases": [{"phase": "prepare_encode_full", "millis": 210.0}]}
        log = '\n'.join([
            '[compiler-timing] phase=prepare_encode_full millis=210.000',
            '[lightmap-timing] indirect_ms=100.00 direct_ms=25.00',
            '[lightmaps] transport scene triangles=14 (2 skipped) emitters=3 (1 switchable)',
            '[lightmap-work] triangles=14 texels=80 direct_rays=64 bounce_rays=320 cache_cells=8',
            json.dumps(report, indent=2),
        ])
        actual, phases, scenes = BENCH.parse_log(log)
        self.assertEqual(actual, report)
        self.assertEqual(len(phases), 2)
        self.assertEqual(phases[1]["indirect_ms"], "100.00")
        self.assertEqual(scenes[0]["work"]["bounce_rays"], 320)
        self.assertEqual(scenes[0]["skipped"], 2)

    def test_owned_child_records_resources_and_absolute_timestamps(self):
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / "child.log"
            result = BENCH.run_owned([sys.executable, "-c", "print('complete')"], log,
                                     os.environ.copy(), 10)
            self.assertEqual(result["exit_code"], 0)
            self.assertGreater(result["pid"], 0)
            self.assertGreater(result["wall_seconds"], 0)
            self.assertLessEqual(datetime.fromisoformat(result["started_utc"]),
                                 datetime.fromisoformat(result["finished_utc"]))
            self.assertEqual(log.read_text().strip(), "complete")
            if hasattr(os, "wait4"):
                self.assertGreater(result["peak_rss_bytes"], 0)
                self.assertGreaterEqual(result["cpu_seconds"], 0)

    def test_timeout_terminates_only_owned_child(self):
        with tempfile.TemporaryDirectory() as directory:
            pid_file = Path(directory) / "child.pid"
            child = f"import os,time; open({str(pid_file)!r},'w').write(str(os.getpid())); time.sleep(30)"
            with self.assertRaises(subprocess.TimeoutExpired):
                BENCH.run_owned([sys.executable, "-c", child], Path(directory) / "child.log",
                                 os.environ.copy(), 0.5)
            if os.name == "posix":
                with self.assertRaises(ProcessLookupError):
                    os.kill(int(pid_file.read_text()), 0)

    def test_atomic_report_does_not_leave_partial_file(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "report.json"
            BENCH.save_report(path, {"runs": [1]})
            BENCH.save_report(path, {"runs": [1, 2]})
            self.assertEqual(json.loads(path.read_text()), {"runs": [1, 2]})
            self.assertFalse(path.with_suffix(".partial").exists())

    def test_package_comparison_rejects_any_lighting_or_metadata_difference(self):
        with tempfile.TemporaryDirectory() as directory:
            def package(name, stage="v1", title="Room", lighting=b"exact HDR bytes"):
                path = Path(directory) / name
                manifest = dict(name=title, lighting_fingerprint=stage,
                                entries=[dict(name="blobs/atlas.lightmaps", role="lightmaps")])
                with zipfile.ZipFile(path, "w") as archive:
                    archive.writestr("manifest.json", json.dumps(manifest))
                    archive.writestr("blobs/atlas.lightmaps", lighting)
                return path
            original = package("before.zip")
            same = package("same.zip")
            self.assertTrue(COMPARE.compare(original, same)["quality_equal"])
            stage_edit = package("stage.zip", stage="v2")
            self.assertFalse(COMPARE.compare(original, stage_edit)["quality_equal"])
            self.assertTrue(COMPARE.compare(original, stage_edit, True)["quality_equal"])
            self.assertFalse(COMPARE.compare(original, package("title.zip", title="Other"), True)["quality_equal"])
            self.assertFalse(COMPARE.compare(original, package("lighting.zip", lighting=b"wrong HDR bytes"), True)["quality_equal"])

    def test_input_key_exception_verifies_only_new_texture_identities(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            image = root / "normal.png"
            image.write_bytes(b"existing input bytes; never decoded by the comparator")
            import hashlib
            dependency = dict(kind="texture", path=image.name, bytes=image.stat().st_size,
                              sha256=hashlib.sha256(image.read_bytes()).hexdigest())
            def package(filename, dependencies, **changes):
                path = root / filename
                value = dict(compiler_fingerprint="v1", lighting_fingerprint="stage1",
                             dependencies=dependencies, entries=[], name="Test")
                value.update(changes)
                with zipfile.ZipFile(path, "w") as archive:
                    archive.writestr("manifest.json", json.dumps(value))
                return path
            original = package("old.zip", [])
            updated = package("new.zip", [dependency], compiler_fingerprint="v2", lighting_fingerprint="stage2")
            self.assertFalse(COMPARE.compare(original, updated, True)["quality_equal"])
            self.assertTrue(COMPARE.compare(original, updated, asset_root=root)["quality_equal"])
            self.assertFalse(COMPARE.compare(original, package("name.zip", [dependency], name="Changed"), asset_root=root)["quality_equal"])
            image.write_bytes(b"corrupt")
            self.assertFalse(COMPARE.compare(original, updated, asset_root=root)["quality_equal"])

    def test_incremental_cases_preserve_original_and_change_only_the_selected_input(self):
        spec = importlib.util.spec_from_file_location("compiler_incremental", ROOT / "tools/bench/compiler_incremental.py")
        module = importlib.util.module_from_spec(spec)
        sys.path.insert(0, str(ROOT / "tools/bench"))
        try:
            spec.loader.exec_module(module)
        finally:
            sys.path.pop(0)
        original = json.loads((ROOT / "tests/fixtures/levels/test_room.json").read_text())
        snapshot = json.loads(json.dumps(original))
        for case in ["unchanged", "metadata", "light", "prop", "material"]:
            result = module.edited_source(original, case)
            self.assertEqual(original, snapshot)
            self.assertEqual(result == original, case == "unchanged")
            if case == "metadata":
                self.assertEqual(result["rooms"], original["rooms"])
                self.assertEqual(result["props"], original["props"])
        with self.assertRaises(ValueError):
            module.edited_source(original, "unsupported")


    def test_hero_incremental_inputs_change_intended_fields_and_assets_stay_separate(self):
        spec = importlib.util.spec_from_file_location("compiler_incremental", ROOT / "tools/bench/compiler_incremental.py")
        module = importlib.util.module_from_spec(spec)
        sys.path.insert(0, str(ROOT / "tools/bench"))
        try:
            spec.loader.exec_module(module)
        finally:
            sys.path.pop(0)
        hero = json.loads((ROOT / "tests/fixtures/levels/art_style_hero_transparency.json").read_text())
        before = json.dumps(hero, sort_keys=True)
        for case in ["presentation", "geometry", "entity", "combined"]:
            changed = module.edited_source(hero, case)
            self.assertNotEqual(changed, hero)
            self.assertEqual(json.dumps(hero, sort_keys=True), before)
        self.assertEqual(module.edited_source(hero, "texture"), hero)
        self.assertEqual(module.edited_source(hero, "model"), hero)
        self.assertNotEqual(module.edited_source(hero, "material")["defaults"]["wall"], hero["defaults"]["wall"])
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "assets"
            root.mkdir()
            target = root / "wall.png"
            target.write_bytes(b"first")
            replacement = Path(directory) / "replacement.png"
            replacement.write_bytes(b"second")
            relative, actual = module.asset_edit("wall.png=" + str(replacement), root, ".png")
            self.assertEqual(relative, Path("wall.png"))
            self.assertEqual(actual, replacement.resolve())
            for value in ["../replacement.png=" + str(target), "wall.png=" + str(target),
                          "wall.png=" + str(replacement.with_suffix(".glb"))]:
                with self.assertRaises(ValueError):
                    module.asset_edit(value, root, ".png")

    def test_scene_budget_checks_identity_payloads_and_warning_thresholds(self):
        import hashlib
        spec = importlib.util.spec_from_file_location("scene_budget", ROOT / "tools/bench/scene_budget.py")
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        metrics = dict(draw_calls=9, package_bytes=200)
        self.assertEqual(module.warning_budgets(metrics, dict(draw_calls=10)), [])
        self.assertEqual(module.warning_budgets(metrics, dict(draw_calls=8))[0]["observed"], 9)
        for limits in [dict(other=5), dict(draw_calls=0), dict(draw_calls=True),
                       dict(draw_calls=float("nan")), dict(draw_calls=float("inf"))]:
            with self.assertRaises(ValueError):
                module.warning_budgets(metrics, limits)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            package, log = root / "hero.placesmap", root / "native.log"
            data = b"prepared geometry"
            manifest = dict(id="hero", variants=[dict(lightmap_quality="full", entries={})],
                            entries=[dict(name="geometry.mesh", role="mesh", bytes=len(data),
                                          sha256=hashlib.sha256(data).hexdigest())])
            raw = json.dumps(manifest).encode()
            def write(payload):
                with zipfile.ZipFile(package, "w") as archive:
                    archive.writestr("manifest.json", raw)
                    archive.writestr("geometry.mesh", payload)
            write(data)
            identity = hashlib.sha256(raw).hexdigest()
            native = dict(level="hero", total_vertices=20, visible_vertices=12, draw_calls=3,
                          visible_indices=18, vbo_bytes=200, index_bytes=36, material_changes=2)
            log.write_text("[loading] package identity level=hero variant=full sha256=" + identity
                           + "\nBENCH_SUMMARY " + json.dumps(native) + "\n")
            self.assertEqual(module.inspect(package, log)["metrics"]["submitted_triangles"], 6)
            valid_log = log.read_text()
            log.write_text(valid_log.replace('"draw_calls": 3', '"draw_calls": 0'))
            with self.assertRaises(ValueError):
                module.inspect(package, log)
            capture = dict(event="capture", mode="final", level="hero",
                           resident=dict(lightmaps="full"), capture_submission=dict(draw_calls=4,
                               frustum_visible_distinct_vertices=15, material_binds=2,
                               submitted_indices=24, submitted_triangles=8))
            log.write_text(valid_log.replace('"draw_calls": 3', '"draw_calls": 0')
                           + "[visual-diagnostic] " + json.dumps(capture) + "\n")
            self.assertEqual(module.inspect(package, log)["metrics"]["submitted_triangles"], 8)
            log.write_text(valid_log)
            log.write_text(log.read_text().replace(identity, "0" * 64))
            with self.assertRaises(ValueError):
                module.inspect(package, log)
            write(b"corrupt")
            with self.assertRaises(ValueError):
                module.inspect(package, log)


if __name__ == "__main__":
    unittest.main()
