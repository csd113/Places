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

TEST_DIR = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, TEST_DIR)
from platform_support import display_available  # noqa: E402

ROOT = os.path.abspath(os.path.join(TEST_DIR, ".."))
SMOKE_ROOT = os.path.join(ROOT, "target", "agent-work", "smoke")
DEFAULT_BINARY = os.path.join(ROOT, "target", "release", "places")


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


class PlatformCapabilityTests(unittest.TestCase):
    """The shared display table needs neither a binary nor a display."""

    def test_display_available_follows_the_platform_table(self):
        # Native macOS and Windows desktops attempt the suite; Windows has no
        # DISPLAY/WAYLAND_DISPLAY variables at all.
        self.assertTrue(display_available("darwin", {}))
        self.assertTrue(display_available("win32", {}))
        self.assertTrue(display_available("win32", {"SESSIONNAME": "Console"}))
        # Other platforms need an explicit X11 or Wayland session.
        self.assertFalse(display_available("linux", {}))
        self.assertTrue(display_available("linux", {"DISPLAY": ":0"}))
        self.assertTrue(display_available("linux", {"WAYLAND_DISPLAY": "wayland-0"}))


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
        if not display_available():
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

    def compile_package(self, source_path, asset_root, out_dir):
        """Compiles one authoring source into a package with the release CLI."""
        compiler = os.environ.get(
            "PLACES_COMPILE_BIN", os.path.join(ROOT, "target", "release", "places-compile")
        )
        if not os.path.isfile(compiler):
            raise unittest.SkipTest(
                "no release compiler at target/release/places-compile; "
                "run `cargo build --release` first"
            )
        os.makedirs(out_dir, exist_ok=True)
        out = os.path.join(
            out_dir, os.path.splitext(os.path.basename(source_path))[0] + ".placesmap"
        )
        result = subprocess.run(
            [
                compiler,
                "build",
                source_path,
                "--out",
                out,
                "--asset-root",
                asset_root,
                "--workers",
                "2",
            ],
            cwd=ROOT,
            env=_clean_env(),
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            timeout=600,
            check=False,
        )
        if result.returncode != 0:
            self.fail(f"places-compile failed:\n{result.stdout}")
        return out

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
        trace = os.path.join(runtime, "loading.jsonl")

        code, output = self.run_binary(
            runtime,
            capture,
            {"PLACES_LEVEL": "places_demo", "PLACES_LOAD_TRACE": trace},
            binary=binary,
        )

        self.assertEqual(code, 0, f"empty install failed:\n{output}")
        self.assertTrue(os.path.isfile(capture))
        with open(trace, encoding="utf-8") as handle:
            events = [json.loads(line) for line in handle if line.strip()]
        self.assertEqual(
            [json.loads(event["detail"])["current_level_id"]
             for event in events if event["event"] == "world_committed"],
            ["places_demo"],
            "the embedded demo must be committed and captured with no asset tree",
        )
        self.assertTrue(os.path.isdir(os.path.join(runtime, "levels")), "levels/ not created")
        self.assertTrue(os.path.isdir(os.path.join(runtime, "import")), "import/ not created")
        # The player prepares nothing, so it owns no lightmap cache: `cache/`
        # belongs to developer tooling and is not created on a fresh install.
        self.assertFalse(
            os.path.exists(os.path.join(runtime, "cache")),
            "the player must not create a preparation cache directory",
        )
        settings_path = os.path.join(runtime, "settings.json")
        self.assertTrue(os.path.isfile(settings_path), "settings.json not created")
        with open(settings_path, encoding="utf-8") as handle:
            settings = json.load(handle)
        self.assertEqual(settings["bindings"]["forward"], "W")
        self.assertEqual(settings["quality"], "high")
        self.assertEqual(settings["texture_filtering"], "high")
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
        settings["texture_filtering"] = "medium"
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
        self.assertEqual(reloaded["texture_filtering"], "medium")

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
            handle.write('{ "format_version": 3, ')  # truncated authoring source
        with open(os.path.join(levels, "bad_geometry.json"), "w", encoding="utf-8") as handle:
            json.dump(
                {
                    "format_version": 3,
                    "id": "bad_geometry",
                    "name": "Bad Geometry",
                    "spawn": {"x": 0.0, "z": 0.0},
                    "rooms": [{"x": 0.0, "z": 0.0, "width": -4.0, "depth": 4.0}],
                },
                handle,
            )
        # A corrupt package and a foreign file with the package extension must
        # be skipped by name, not crash the boot.
        with open(os.path.join(levels, "broken.placesmap"), "wb") as handle:
            handle.write(b"not a zip archive at all")
        with open(os.path.join(levels, "truncated.placesmap"), "wb") as handle:
            handle.write(b"PK\x03\x04garbage")

        capture = os.path.join(runtime, "frame.png")
        code, output = self.run_binary(runtime, capture, binary=binary)

        self.assertEqual(code, 0, output)
        self.assertTrue(os.path.isfile(capture), "the game must still boot")
        # Authoring sources are expected content beside their packages and are
        # skipped silently: neither the malformed source nor the invalid one is
        # a playable row, and neither is a startup warning to report.
        self.assertNotIn("broken.json", output, "a malformed source is skipped silently")
        self.assertNotIn("bad_geometry.json", output, "an invalid source is skipped silently")
        self.assertNotIn("authoring source", output, "sources are never reported at discovery")
        self.assertIn("broken.placesmap", output, "the corrupt package must be named")
        self.assertIn("truncated.placesmap", output, "the truncated package must be named")

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
            "format_version": 3,
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
        source = os.path.join(levels, "degraded.json")
        with open(source, "w", encoding="utf-8") as handle:
            json.dump(level, handle, indent=2)
        # The player never compiles: the degraded content must arrive as a
        # package produced by the explicit compiler command.
        package = self.compile_package(source, os.path.join(runtime, "assets"), levels)
        self.assertTrue(os.path.isfile(package), "the fixture package was published")
        os.remove(source)

        capture = os.path.join(runtime, "frame.png")
        code, output = self.run_binary(
            runtime, capture, {"PLACES_LEVEL": "degraded_content"}, binary=binary
        )

        self.assertEqual(code, 0, output)
        self.assertTrue(os.path.isfile(capture), "a degraded level must still render")
        self.assertIn("nope:missing_floor", output, "the unknown material must be named")

    def test_the_player_performs_no_static_preparation(self):
        package = self.make_package("no-preparation")
        binary = os.path.join(package, "places")
        capture = os.path.join(package, "frame.png")
        code, output = self.run_binary(
            package,
            capture,
            {"PLACES_LEVEL": "places_demo", "PLACES_VERBOSE": "1"},
            binary=binary,
        )
        self.assertEqual(code, 0, output)
        self.assertTrue(os.path.isfile(capture), "the player must render")
        self.assertIn("[loading] compiled-cache miss", output, "a package decode happened")
        # The prohibited preparation leaves named traces; none may appear.
        for forbidden in (
            "fill workers=",          # lightmap atlas fill
            "face(s), face edge",     # reflection probe bake
            "chart worker",           # atlas chart planning
            "atlas plan failed",
        ):
            self.assertNotIn(
                forbidden, output, f"the player ran static preparation: {forbidden}"
            )
        self.assertIn(
            "uploaded", output, "packaged reflection captures must be uploaded"
        )
        cache = os.path.join(package, "cache", "lightmaps")
        if os.path.isdir(cache):
            self.assertEqual(
                [name for name in os.listdir(cache) if name.endswith(".lmc")],
                [],
                "the player must not write a lightmap bake cache",
            )

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
            "[loading] committed places_demo" in output,
            "the demo must load by id",
        )
        self.assertTrue(os.path.isfile(capture))

    # -- 8. Every declared sampler is bound before the world is first drawn --
    #
    # The level-load reflection-probe bake draws the world program before the
    # first frame exists. The lightmap and reflection units must already hold
    # complete textures there: an incomplete binding is a wgpu validation error
    # or a fatal device error, and the probe bakes would sample a zero lightmap
    # (reflections baked black).
    def test_packaged_reflection_probes_draw_with_complete_samplers(self):
        # The player uploads the compiler's captured cubemaps instead of
        # rendering its own probe bake; both must produce a complete, valid
        # sampler set with no wgpu validation error.
        runtime = self.make_package("probe-captures")
        binary = os.path.join(runtime, "places")
        capture = os.path.join(runtime, "frame.png")
        code, output = self.run_binary(
            runtime, capture, {"PLACES_LEVEL": "places_demo"}, binary=binary
        )
        self.assertEqual(code, 0, output)
        self.assertTrue(os.path.isfile(capture))
        self.assertGreater(os.path.getsize(capture), 10_000, "capture is suspiciously small")
        self.assertNotIn("panicked", output, output)
        # A validation error or a fatal device error is a hard failure, not a
        # warning to skim past.
        self.assertNotIn("Validation Error", output, output)
        self.assertNotIn("[wgpu] fatal device error", output, output)


if __name__ == "__main__":
    unittest.main(verbosity=2)
