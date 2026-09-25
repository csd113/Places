#!/usr/bin/env python3
"""Compiled-build smoke tests: the real executable, outside the repository.

These tests exist because `cargo run` from the repository root hides several
classes of failure that only appear in a compiled build:

* the binary must locate its payload from its own directory, not from the
  working directory it happens to be started in;
* a genuinely fresh install must create `levels/`, `import/` and a default
  `settings.json`, and must boot the embedded Places Demo even when no asset
  tree is installed at all;
* a restart must load the saved configuration;
* malformed configuration and malformed custom levels must be reported and
  skipped without a crash;
* normal operation must be quiet: no developer telemetry on stdout.

The tests need a graphical session because Places creates an SDL window and a
Metal/Vulkan/D3D12 surface; they skip themselves when no display is available
or when no release binary has been built. Set ``PLACES_SMOKE_BIN`` to test a
specific executable, or ``PLACES_SKIP_SMOKE=1`` to skip explicitly.

All scratch state lives under ``target/agent-work/smoke/`` (never the system
temporary directory), matching the repository's temporary-file rule.
"""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import unittest

ROOT = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
SMOKE_ROOT = os.path.join(ROOT, "target", "agent-work", "smoke")
DEFAULT_BINARY = os.path.join(ROOT, "target", "release", "places")


def _display_available() -> bool:
    if sys.platform == "darwin":
        return True
    return bool(os.environ.get("DISPLAY") or os.environ.get("WAYLAND_DISPLAY"))


def _clean_env() -> dict:
    env = {
        key: value
        for key, value in os.environ.items()
        if not key.startswith("PLACES_")
    }
    # Keep SDL from picking up a developer override; the smoke test wants the
    # ordinary desktop path.
    env.pop("SDL_VIDEODRIVER", None)
    return env


class CompiledBuildSmokeTests(unittest.TestCase):
    binary = DEFAULT_BINARY

    @classmethod
    def setUpClass(cls):
        cls.binary = os.environ.get("PLACES_SMOKE_BIN", DEFAULT_BINARY)
        if os.environ.get("PLACES_SKIP_SMOKE") == "1":
            raise unittest.SkipTest("PLACES_SKIP_SMOKE=1")
        if not os.path.isfile(cls.binary):
            raise unittest.SkipTest(
                "no release binary at target/release/places; "
                "run `cargo build --release` first (or set PLACES_SMOKE_BIN)"
            )
        if not _display_available():
            raise unittest.SkipTest("no graphical session for the SDL window")
        os.makedirs(SMOKE_ROOT, exist_ok=True)

    def run_binary(self, cwd, capture, extra_env=None, timeout=180, binary=None):
        """Runs one capture frame and returns (exit_code, combined_output)."""
        env = _clean_env()
        env.update(
            {
                "PLACES_BENCH": "1",
                "PLACES_BENCH_NOSWAP": "1",
                "PLACES_CAPTURE": capture,
            }
        )
        if extra_env:
            env.update(extra_env)
        result = subprocess.run(
            [binary or self.binary],
            cwd=cwd,
            env=env,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            timeout=timeout,
            check=False,
        )
        return result.returncode, result.stdout

    def fresh_case(self, name):
        directory = os.path.join(SMOKE_ROOT, name)
        shutil.rmtree(directory, ignore_errors=True)
        os.makedirs(directory)
        return directory

    def make_package(self, name):
        """A portable install: the executable plus a copy of the asset tree."""
        package = self.fresh_case(name)
        shutil.copy2(self.binary, os.path.join(package, "places"))
        shutil.copytree(
            os.path.join(ROOT, "assets"),
            os.path.join(package, "assets"),
            ignore=shutil.ignore_patterns(".DS_Store"),
        )
        return package

    # -- 1. Portable package: binary and payload together, run elsewhere ----

    def test_packaged_build_runs_from_an_unrelated_directory(self):
        package = self.make_package("package")
        unrelated = self.fresh_case("unrelated-cwd")
        capture = os.path.join(package, "first.png")

        code, output = self.run_binary(
            unrelated,
            capture,
            {"PLACES_LEVEL": "places_demo"},
            binary=os.path.join(package, "places"),
        )

        self.assertEqual(code, 0, f"packaged build failed:\n{output}")
        self.assertTrue(os.path.isfile(capture), "no capture was written")
        self.assertGreater(os.path.getsize(capture), 10_000, "capture is suspiciously small")
        # First-run state appears next to the payload, not in the cwd.
        self.assertTrue(os.path.isdir(os.path.join(package, "levels")))
        self.assertTrue(os.path.isdir(os.path.join(package, "import")))
        self.assertTrue(os.path.isfile(os.path.join(package, "settings.json")))
        self.assertFalse(os.path.exists(os.path.join(unrelated, "settings.json")))
        # The shipped demo resolved its materials: no unresolved-material noise.
        self.assertNotIn("[materials]", output, output)
        # Normal operation is quiet when PLACES_VERBOSE is unset.
        for line in output.splitlines():
            self.assertFalse(
                line.startswith(("[package]", "[props]", "[level]", "[lighting]", "[lightmaps]")),
                f"developer telemetry leaked into a normal run: {line}",
            )

    # -- 2. Fresh empty directory: embedded demo and created state ----------

    def test_empty_install_boots_the_embedded_demo_and_creates_state(self):
        runtime = self.fresh_case("empty")
        binary = os.path.join(runtime, "places")
        shutil.copy2(self.binary, binary)
        capture = os.path.join(runtime, "first.png")

        code, output = self.run_binary(
            runtime, capture, {"PLACES_LEVEL": "places_demo"}, binary=binary
        )

        self.assertEqual(code, 0, f"empty install failed:\n{output}")
        self.assertTrue(os.path.isfile(capture))
        self.assertIn(
            "'Places Demo' (places_demo)",
            output,
            "the embedded demo must be selectable by id with no asset tree",
        )
        self.assertTrue(os.path.isdir(os.path.join(runtime, "levels")), "levels/ not created")
        self.assertTrue(os.path.isdir(os.path.join(runtime, "import")), "import/ not created")
        self.assertTrue(os.path.isdir(os.path.join(runtime, "cache")), "cache/ not created")
        settings_path = os.path.join(runtime, "settings.json")
        self.assertTrue(os.path.isfile(settings_path), "settings.json not created")
        with open(settings_path, encoding="utf-8") as handle:
            settings = json.load(handle)
        self.assertEqual(settings["bindings"]["forward"], "W")
        self.assertEqual(settings["quality"], "full")
        self.assertTrue(settings["lightmaps"])
        # A missing asset root is reported exactly once, not once per caller.
        self.assertEqual(output.count("no asset root found"), 1, output)
        self.assertNotIn("panicked", output)

    # -- 3. Restart loads the saved configuration ---------------------------

    def test_restart_loads_the_saved_configuration(self):
        runtime = self.make_package("restart-package")
        binary = os.path.join(runtime, "places")
        code, output = self.run_binary(
            runtime, os.path.join(runtime, "first.png"), binary=binary
        )
        self.assertEqual(code, 0, output)
        settings_path = os.path.join(runtime, "settings.json")
        self.assertTrue(os.path.isfile(settings_path), "first run must write settings")
        with open(settings_path, encoding="utf-8") as handle:
            settings = json.load(handle)
        settings["fov_degrees"] = 75.0
        settings["texture_filtering"] = "nearest"
        with open(settings_path, "w", encoding="utf-8") as handle:
            json.dump(settings, handle, indent=2)

        capture = os.path.join(runtime, "second.png")
        code, output = self.run_binary(
            runtime, capture, {"PLACES_LEVEL": "places_demo"}, binary=binary
        )
        self.assertEqual(code, 0, output)

        with open(settings_path, encoding="utf-8") as handle:
            reloaded = json.load(handle)
        self.assertEqual(reloaded["fov_degrees"], 75.0)
        self.assertEqual(reloaded["texture_filtering"], "nearest")

    # -- 4. Malformed configuration recovers --------------------------------

    def test_malformed_settings_recover_without_a_crash(self):
        runtime = self.fresh_case("bad-settings")
        binary = os.path.join(runtime, "places")
        shutil.copy2(self.binary, binary)
        settings_path = os.path.join(runtime, "settings.json")
        with open(settings_path, "w", encoding="utf-8") as handle:
            handle.write("{ this is not a settings file")

        capture = os.path.join(runtime, "frame.png")
        code, output = self.run_binary(runtime, capture, binary=binary)

        self.assertEqual(code, 0, output)
        self.assertTrue(
            os.path.isfile(os.path.join(runtime, "settings.json.invalid")),
            "the unreadable file must be preserved for inspection",
        )
        self.assertTrue(
            os.path.isfile(settings_path),
            "a clean default settings file must be written after recovery",
        )
        with open(settings_path, encoding="utf-8") as handle:
            settings = json.load(handle)
        self.assertEqual(settings["bindings"]["forward"], "W")
        self.assertIn("settings.json", output)

    # -- 5. Malformed custom content is rejected, not fatal ------------------

    def test_malformed_custom_levels_are_skipped_with_reasons(self):
        runtime = self.fresh_case("bad-levels")
        binary = os.path.join(runtime, "places")
        shutil.copy2(self.binary, binary)
        shutil.copytree(
            os.path.join(ROOT, "assets"),
            os.path.join(runtime, "assets"),
            ignore=shutil.ignore_patterns(".DS_Store"),
        )
        levels = os.path.join(runtime, "levels")
        os.makedirs(levels, exist_ok=True)

        with open(os.path.join(levels, "broken.json"), "w", encoding="utf-8") as handle:
            handle.write('{ "format_version": 1, ')  # truncated JSON
        with open(os.path.join(levels, "bad_geometry.json"), "w", encoding="utf-8") as handle:
            json.dump(
                {
                    "format_version": 1,
                    "id": "bad_geometry",
                    "name": "Bad Geometry",
                    "spawn": {"x": 0.0, "z": 0.0},
                    "rooms": [{"x": 0.0, "z": 0.0, "width": -4.0, "depth": 4.0}],
                },
                handle,
            )

        capture = os.path.join(runtime, "frame.png")
        code, output = self.run_binary(runtime, capture, binary=binary)

        self.assertEqual(code, 0, output)
        self.assertTrue(os.path.isfile(capture), "the game must still boot")
        self.assertIn("broken.json", output, "the skipped file must be named")
        self.assertIn("bad_geometry.json", output, "the skipped file must be named")

    # -- 6. Degraded content loads with diagnostics --------------------------

    def test_unknown_material_prop_and_fixture_degrade_without_a_crash(self):
        runtime = self.fresh_case("degraded")
        binary = os.path.join(runtime, "places")
        shutil.copy2(self.binary, binary)
        shutil.copytree(
            os.path.join(ROOT, "assets"),
            os.path.join(runtime, "assets"),
            ignore=shutil.ignore_patterns(".DS_Store"),
        )
        levels = os.path.join(runtime, "levels")
        os.makedirs(levels, exist_ok=True)
        level = {
            "format_version": 1,
            "id": "degraded_content",
            "name": "Degraded Content",
            "spawn": {"x": 2.0, "z": 2.0, "yaw_degrees": 0.0},
            "rooms": [
                {
                    "x": 0.0,
                    "z": 0.0,
                    "width": 6.0,
                    "depth": 6.0,
                    "material": "nope:missing_floor",
                }
            ],
            "props": [
                {"model": "nope:missing_prop", "x": 1.0, "z": 1.0, "solid": True}
            ],
            "ceiling_lights": [
                {"fixture": "nope:missing_fixture", "x": 3.0, "z": 3.0}
            ],
            "walls": [
                {
                    "x": 0.0,
                    "z": 3.0,
                    "width": 6.0,
                    "depth": 0.2,
                    "openings": [
                        {
                            "kind": "window",
                            "offset": 1.0,
                            "width": 1.5,
                            "height": 1.0,
                            "sill": 0.9,
                            "glass": "nope:missing_glass",
                        }
                    ],
                }
            ],
        }
        with open(os.path.join(levels, "degraded.json"), "w", encoding="utf-8") as handle:
            json.dump(level, handle, indent=2)

        capture = os.path.join(runtime, "frame.png")
        code, output = self.run_binary(
            runtime, capture, {"PLACES_LEVEL": "degraded_content"}, binary=binary
        )

        self.assertEqual(code, 0, output)
        self.assertTrue(os.path.isfile(capture), "a degraded level must still render")
        self.assertIn("nope:missing_floor", output, "the unknown material must be named")

    # -- 7. Places Demo is listed even with no custom levels -----------------

    def test_places_demo_is_always_available(self):
        runtime = self.make_package("demo-package")
        binary = os.path.join(runtime, "places")
        self.assertTrue(
            os.path.isfile(os.path.join(runtime, "assets", "levels", "places_demo.json"))
        )
        capture = os.path.join(runtime, "demo.png")
        code, output = self.run_binary(
            runtime,
            capture,
            {"PLACES_LEVEL": "places_demo", "PLACES_VERBOSE": "1"},
            binary=binary,
        )
        self.assertEqual(code, 0, output)
        self.assertTrue(
            "loading 'Places Demo'" in output
            or "'Places Demo' (places_demo) is already the loaded level" in output,
            "the demo must load by id",
        )
        self.assertTrue(os.path.isfile(capture))

    # -- 8. Every declared sampler is bound before the world is first drawn --
    #
    # The level-load reflection-probe bake draws the world program before the
    # first frame exists. The lightmap and reflection units must already hold
    # complete textures there: when they did not, the Apple GL driver logged
    # "GLD_TEXTURE_INDEX_2D is unloadable ... using zero texture" and the probe
    # bakes sampled a zero lightmap (reflections baked black).
    def test_the_probe_bake_draws_with_complete_samplers(self):
        runtime = self.make_package("probe-bake")
        binary = os.path.join(runtime, "places")
        capture = os.path.join(runtime, "frame.png")
        code, output = self.run_binary(
            runtime, capture, {"PLACES_LEVEL": "places_demo"}, binary=binary
        )
        self.assertEqual(code, 0, output)
        self.assertTrue(os.path.isfile(capture))
        self.assertNotIn("GLD_TEXTURE_INDEX_2D", output, output)
        self.assertNotIn("texture unloadable", output, output)


if __name__ == "__main__":
    unittest.main(verbosity=2)
