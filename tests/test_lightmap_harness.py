"""Lightmap-report process configuration checks; every game launch is mocked."""
import contextlib
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
from tools.bench import lightmap_report


class LightmapHarnessTests(unittest.TestCase):
    def test_external_graphics_controls_survive_but_other_launch_state_does_not(self):
        inherited = {"PATH": "/bin", "DISPLAY": ":7", "HOME": "/home/test",
                     "PLACES_NO_LIGHTMAPS": "1", "PLACES_QUALITY": "low",
                     "PLACES_LEVEL": "wrong", "PLACES_STATE_ROOT": "/do/not/touch",
                     "PLACES_CAPTURE": "/old.png"}
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with mock.patch.dict(os.environ, inherited, clear=True), mock.patch.object(
                    lightmap_report.subprocess, "run", return_value=subprocess.CompletedProcess([], 0, "out", "err")) as launch:
                result = lightmap_report.run_shot(root / "places", root, "requested", {}, root / "shot.png", 1, {})
            self.assertEqual(result, (0, "outerr"))
            env = launch.call_args.kwargs["env"]
            self.assertEqual(env["PLACES_NO_LIGHTMAPS"], "1")
            self.assertEqual(env["PLACES_QUALITY"], "low")
            self.assertEqual(env["PLACES_LEVEL"], "requested")
            self.assertEqual(env["PLACES_STATE_ROOT"], str(root.resolve()))
            self.assertEqual(env["PLACES_CAPTURE"], str((root / "shot.png").resolve()))
            self.assertEqual(env["PLACES_VERBOSE"], "1")
            self.assertEqual(env["DISPLAY"], ":7")
            self.assertEqual(env["HOME"], "/home/test")

    def test_multiframe_capture_follows_effective_warmup_and_explicit_overrides(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            extra = {"PLACES_BENCH_WARMUP": "3", "PLACES_BENCH_FRAMES": "7",
                     "PLACES_NO_LIGHTMAPS": "full"}
            with mock.patch.dict(os.environ, {"PLACES_NO_LIGHTMAPS": "1"}, clear=True), mock.patch.object(
                    lightmap_report.subprocess, "run", return_value=subprocess.CompletedProcess([], 0, "", "")) as launch:
                lightmap_report.run_shot(root / "places", root, "small", {}, root / "shot.png", 5, extra)
            env = launch.call_args.kwargs["env"]
            self.assertEqual(env["PLACES_CAPTURE_FRAME"], "10")
            self.assertEqual(env["PLACES_BENCH_OUT"], str((root / "shot.csv").resolve()))
            self.assertEqual(env["PLACES_CAPTURE"], str((root / "shot.png").resolve()))
            self.assertEqual(env["PLACES_NO_LIGHTMAPS"], "full")

    def test_explicit_cli_capture_and_state_destinations_are_preserved(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            explicit = {"PLACES_STATE_ROOT": str(root / "chosen-state"),
                        "PLACES_CAPTURE": str(root / "chosen.png"), "PLACES_CAPTURE_FRAME": "20"}
            with mock.patch.object(lightmap_report.subprocess, "run",
                                   return_value=subprocess.CompletedProcess([], 0, "", "")) as launch:
                lightmap_report.run_shot(root / "places", root, "small", {}, root / "default.png", 5, explicit)
            for key, value in explicit.items():
                self.assertEqual(launch.call_args.kwargs["env"][key], value)

    def test_zero_exit_without_measurements_is_a_failed_report(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary = root / "places"
            binary.write_bytes(b"fixture executable is never run")
            args = ["lightmap_report", "--binary", str(binary), "--out", str(root / "report"),
                    "--run-dir", str(root / "run"), "--shots", "demo_spawn"]
            with mock.patch.object(sys, "argv", args), mock.patch.object(lightmap_report, "stage_run_dir"), \
                    mock.patch.object(lightmap_report, "run_shot", return_value=(0, "")), \
                    contextlib.redirect_stdout(io.StringIO()):
                code = lightmap_report.main()
            self.assertEqual(code, 1)
            report = json.loads((root / "report" / "report.json").read_text())
            errors = report["shots"]["demo_spawn"]["errors"]
            self.assertTrue(any("missing required telemetry" in error for error in errors))
            self.assertTrue(any("capture PNG" in error for error in errors))

    def test_cold_does_not_clear_a_different_explicit_state_root(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary = root / "places"
            binary.write_bytes(b"not executed")
            args = ["lightmap_report", "--binary", str(binary), "--out", str(root / "report"),
                    "--run-dir", str(root / "run"), "--cold", "--env", f"PLACES_STATE_ROOT={root / 'external'}"]
            with mock.patch.object(sys, "argv", args), mock.patch.object(lightmap_report, "stage_run_dir") as stage, \
                    contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit) as raised:
                lightmap_report.main()
            self.assertEqual(raised.exception.code, 2)
            stage.assert_not_called()


if __name__ == "__main__":
    unittest.main()
