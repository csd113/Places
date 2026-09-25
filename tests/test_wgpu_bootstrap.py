#!/usr/bin/env python3
"""wgpu runtime smoke tests: the real executable on the wgpu path.

These tests prove the whole wgpu chain on the running host, not just the
renderer logic:

    SDL initializes
    -> the window is created with the platform flags the wgpu surface needs
    -> wgpu instance / native adapter / device / queue exist
    -> the surface is configured and the depth target exists
    -> the static world geometry is built, uploaded and drawn
    -> frames are acquired, cleared, submitted and presented
    -> the engine shuts down cleanly

They bound the run with the existing benchmark harness
(``PLACES_BENCH=1 PLACES_BENCH_FRAMES=n``) instead of adding a public game
feature, and they assert the adapter's reported backend so a silent fallback to
another API cannot pass. The renderer is the only one: since Stage 11 there is
no runtime backend selection.

The Stage 5 tests read the one load-time world-upload diagnostic and assert
non-empty geometry, draw ranges and a successful present. A second level is
installed into a scratch state root so the same process boots Places Demo and
then *replaces* it through ``PLACES_LEVEL``, which exercises the level-reload
path (upload, drop the old buffers, draw the new world) without menu input.
The Stage 7 tests read the material-resolution diagnostic and assert the
opaque/cut-out/translucent breakdown matches the draw set, the demo's glass and
grille are present, the response/reflection metadata follows the quality
profile and nothing is created per frame.

The tests need a graphical session and a release binary; they skip themselves
otherwise. All scratch state lives under ``target/agent-work/wgpu-smoke/``.
"""

from __future__ import annotations

import json
import os
import re
import shutil
import struct
import subprocess
import sys
import unittest

ROOT = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
SMOKE_ROOT = os.path.join(ROOT, "target", "agent-work", "wgpu-smoke")
DEFAULT_BINARY = os.path.join(ROOT, "target", "release", "places")

# The native backend each desktop target must report in the startup line
# (`[renderer] wgpu | ... | backend: <Debug>`); Stage 4 accepts no other.
EXPECTED_BACKEND = {
    "darwin": "Metal",
    "linux": "Vulkan",
    "win32": "Dx12",
}

# One load-time diagnostic names what the Stage 5 upload produced.
WORLD_UPLOAD = re.compile(
    r"\[wgpu\] world upload: (\d+) vertices, (\d+) indices, (\d+) draws "
    r"in (\d+) chunk"
)

# One load-time diagnostic names what the Stage 6 texture resolution produced:
# unique textures, GPU uploads, cache hits, fallback draws, diagnostic missing
# textures / draws, resident bytes, longest edge and the filtering mode.
TEXTURE_LOAD = re.compile(
    r"\[wgpu\] textures: (\d+) unique, (\d+) uploaded, (\d+) cache hits, "
    r"(\d+) fallbacks, (\d+) missing of (\d+) draws "
    r"\((\d+) bytes resident, max edge (\d+)px, (\w+) filtering\)"
)

# One load-time diagnostic names what the Stage 7 material resolution produced:
# distinct resolved material states (with how many carry the response and how
# many are reflection-eligible), normal maps and their uploads, and the
# material-defined pass breakdown of the world draw set.
MATERIAL_LOAD = re.compile(
    r"\[wgpu\] materials: (\d+) resolved \((\d+) response, (\d+) reflection-eligible\), "
    r"(\d+) normal maps \((\d+) uploaded, (\d+) cache hits\), "
    r"(\d+) opaque / (\d+) cutout / (\d+) translucent of (\d+) draws "
    r"\((\w+) response, (\w+) profile\)"
)

SECOND_LEVEL_ID = "wgpu_smoke_second"
EMPTY_LEVEL_ID = "wgpu_smoke_empty"
NOTEXTURE_LEVEL_ID = "wgpu_smoke_notexture"
MISSING_LEVEL_ID = "wgpu_smoke_missing"


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
    env.pop("SDL_VIDEODRIVER", None)
    return env


def _write_level(root: str, level: dict) -> None:
    """Installs one standalone level into a scratch state root."""
    levels_dir = os.path.join(root, "levels")
    os.makedirs(levels_dir, exist_ok=True)
    path = os.path.join(levels_dir, f"{level['id']}.json")
    with open(path, "w", encoding="utf-8") as handle:
        json.dump(level, handle)


def _second_level() -> dict:
    """A small but real level: one room with a floor, ceiling and walls."""
    return {
        "format_version": 1,
        "id": SECOND_LEVEL_ID,
        "name": "wgpu Smoke Second",
        "author": "wgpu runtime smoke test",
        "spawn": {"x": 2.0, "z": 2.0},
        "rooms": [
            {"x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0, "height": 3.0}
        ],
    }


def _empty_level() -> dict:
    """A valid level that emits no static world geometry at all."""
    return {
        "format_version": 1,
        "id": EMPTY_LEVEL_ID,
        "name": "wgpu Smoke Empty",
        "author": "wgpu runtime smoke test",
        "spawn": {"x": 0.0, "z": 0.0},
    }


def _notexture_level() -> dict:
    """A room whose surfaces name no material at all.

    Every stage 6 draw must resolve to the shared fallback sheet; the level is
    a valid one, not an error case.
    """
    return {
        "format_version": 1,
        "id": NOTEXTURE_LEVEL_ID,
        "name": "wgpu Smoke No Texture",
        "author": "wgpu runtime smoke test",
        "spawn": {"x": 2.0, "z": 2.0},
        "defaults": {"wall": "", "floor": "", "ceiling": ""},
        "rooms": [
            {"x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0, "height": 3.0}
        ],
    }


def _missing_level() -> dict:
    """A room whose default materials do not exist.

    Resolution degrades each entry to the shared diagnostic texture; the level
    must still upload and present, with the missing count reported.
    """
    return {
        "format_version": 1,
        "id": MISSING_LEVEL_ID,
        "name": "wgpu Smoke Missing",
        "author": "wgpu runtime smoke test",
        "spawn": {"x": 2.0, "z": 2.0},
        "defaults": {
            "wall": "core:does_not_exist_wall",
            "floor": "core:does_not_exist_floor",
            "ceiling": "core:does_not_exist_ceiling",
        },
        "rooms": [
            {"x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0, "height": 3.0}
        ],
    }


class WgpuRuntimeSmokeTests(unittest.TestCase):
    binary = DEFAULT_BINARY
    backend = "unsupported"

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
        cls.backend = EXPECTED_BACKEND.get(sys.platform)
        if cls.backend is None:
            raise unittest.SkipTest(
                f"the wgpu smoke test has no expected backend for {sys.platform!r}"
            )
        os.makedirs(SMOKE_ROOT, exist_ok=True)

    def run_binary(self, extra_env=None, timeout=240):
        """Runs a bounded frame loop and returns (exit_code, combined_output)."""
        env = _clean_env()
        env.update(
            {
                "PLACES_BENCH": "1",
                "PLACES_BENCH_FRAMES": "5",
                "PLACES_VERBOSE": "1",
                "PLACES_LEVEL": "places_demo",
                # Pinned so a previous test's `PLACES_QUALITY=low` (persisted in
                # the shared smoke state root) cannot leak into this run.
                "PLACES_QUALITY": "full",
            }
        )
        if extra_env:
            env.update(extra_env)
        result = subprocess.run(
            [self.binary],
            cwd=ROOT,
            env=env,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            timeout=timeout,
            check=False,
        )
        return result.returncode, result.stdout

    def assert_no_gpu_failure(self, output: str) -> None:
        """Fails on the failure modes a broken frame must never pass with."""
        self.assertNotIn("panicked", output, output)
        self.assertNotIn("[wgpu] fatal device error", output, output)
        # A validation error is a hard failure, not a warning to skim past.
        self.assertNotIn("Validation Error", output, output)
        self.assertNotIn("invalid surface", output, output)
        self.assertIn("BENCH_SUMMARY", output, output)

    def world_uploads(self, output: str) -> list[tuple[int, int, int, int]]:
        """Every load-time world-upload diagnostic, as int tuples."""
        return [
            tuple(int(group) for group in match.groups())
            for match in WORLD_UPLOAD.finditer(output)
        ]

    def texture_loads(self, output: str) -> list[tuple[int, int, int, int, int, int, int, int, str]]:
        """Every load-time texture-resolution diagnostic, as typed tuples.

        Fields: unique, uploaded, cache hits, fallbacks, missing, draws,
        resident bytes, max edge, filtering mode.
        """
        return [
            (
                int(match.group(1)),
                int(match.group(2)),
                int(match.group(3)),
                int(match.group(4)),
                int(match.group(5)),
                int(match.group(6)),
                int(match.group(7)),
                int(match.group(8)),
                match.group(9),
            )
            for match in TEXTURE_LOAD.finditer(output)
        ]

    def material_loads(
        self, output: str
    ) -> list[tuple[int, int, int, int, int, int, int, int, int, int, str, str]]:
        """Every load-time material-resolution diagnostic, as typed tuples.

        Fields: materials, response materials, reflection-eligible materials,
        normal maps, normal uploads, normal cache hits, opaque draws, cut-out
        draws, translucent draws, draws, response mode, quality profile.
        """
        return [
            (
                int(match.group(1)),
                int(match.group(2)),
                int(match.group(3)),
                int(match.group(4)),
                int(match.group(5)),
                int(match.group(6)),
                int(match.group(7)),
                int(match.group(8)),
                int(match.group(9)),
                int(match.group(10)),
                match.group(11),
                match.group(12),
            )
            for match in MATERIAL_LOAD.finditer(output)
        ]

    def assert_material_resolution_is_sane(self, load, output: str) -> None:
        """Shape checks every material-resolution line must satisfy."""
        (
            materials,
            response,
            reflections,
            normal_maps,
            _uploads,
            _hits,
            opaque,
            cutout,
            translucent,
            draws,
            response_mode,
            profile,
        ) = load
        self.assertIn(response_mode, ("enabled", "disabled"), output)
        self.assertIn(profile, ("full", "low"), output)
        self.assertLessEqual(materials, draws, output)
        self.assertLessEqual(response, materials, output)
        self.assertLessEqual(reflections, response, output)
        self.assertLessEqual(normal_maps, response, output)
        self.assertEqual(
            opaque + cutout + translucent,
            draws,
            "every draw belongs to exactly one material pass",
        )
        if draws == 0:
            self.assertEqual((materials, opaque, cutout, translucent), (0, 0, 0, 0), output)
        else:
            self.assertGreater(materials, 0, output)
            self.assertGreater(opaque, 0, "a world with architecture is not all glass")

    def assert_texture_resolution_is_sane(self, load, output: str) -> None:
        """Shape checks every texture-resolution line must satisfy."""
        unique, uploaded, hits, fallbacks, missing, draws, resident, edge, mode = load
        self.assertLessEqual(uploaded, unique, output)
        self.assertLessEqual(unique, draws, output)
        self.assertLessEqual(fallbacks, draws, output)
        self.assertIn(mode, ("linear", "nearest"), output)
        if draws == 0:
            self.assertEqual((unique, uploaded, fallbacks, missing), (0, 0, 0, 0), output)
            self.assertEqual(resident, 0, output)
        else:
            self.assertGreater(unique, 0, "a drawn world needs at least one texture")
            self.assertGreater(resident, 0, "sampled textures must be resident")
            self.assertGreater(edge, 0, output)

    # ------------------------------------------------------ Stage 4 lifecycle

    def test_the_wgpu_renderer_draws_the_world_and_exits_cleanly(self):
        code, output = self.run_binary({})

        self.assertEqual(code, 0, f"wgpu run failed:\n{output}")
        self.assertIn("[renderer] wgpu | adapter: ", output, output)
        self.assertIn(f"backend: {self.backend}", output, output)
        self.assertIn("surface format:", output, output)
        self.assertIn("present mode: Fifo", output, output)
        self.assertIn("depth format: Depth32Float", output, output)
        self.assertIn("drawable: ", output, output)
        self.assertIn("[wgpu] world pipeline for surface format", output, output)
        self.assert_no_gpu_failure(output)

        drawable = re.search(r"drawable: (\d+)x(\d+)", output)
        self.assertIsNotNone(drawable, output)
        self.assertGreater(int(drawable.group(1)), 0, output)
        self.assertGreater(int(drawable.group(2)), 0, output)

    def test_places_demo_uploads_non_empty_world_geometry(self):
        code, output = self.run_binary({})

        self.assertEqual(code, 0, f"wgpu run failed:\n{output}")
        uploads = self.world_uploads(output)
        self.assertEqual(len(uploads), 1, f"expected one boot upload:\n{output}")
        vertices, indices, draws, chunks = uploads[0]
        self.assertGreater(vertices, 0, "the world must have vertices")
        self.assertGreater(indices, 0, "the world must have indices")
        self.assertGreater(draws, 0, "the world must draw at least one range")
        self.assertGreaterEqual(chunks, 1, "the world needs a chunk")
        self.assertEqual(indices % 3, 0, "indices are whole triangles")
        self.assert_no_gpu_failure(output)

    # ------------------------------------------------- Stage 6 texture system

    def test_places_demo_uploads_each_base_texture_once(self):
        code, output = self.run_binary({})

        self.assertEqual(code, 0, f"wgpu run failed:\n{output}")
        loads = self.texture_loads(output)
        self.assertEqual(
            len(loads), 1, f"textures resolve once per level load, not per frame:\n{output}"
        )
        load = loads[0]
        self.assert_texture_resolution_is_sane(load, output)
        unique, uploaded, _hits, fallbacks, missing, draws, resident, edge, mode = load

        # The demo's opaque architecture samples the shipped environment
        # sheets; every unseen sheet of the world draw set uploads exactly once.
        # Stage 9 adds the fixture-sheet family, whose sheets upload through
        # their own (cached) path, and the placeholder-box/light-housing draws
        # that share the fallback sheet, so the uploaded count is at most the
        # unique count and fallbacks are legitimate here.
        self.assertGreaterEqual(unique, 25, output)
        self.assertGreater(uploaded, 25, "the first load uploads the world's sheets")
        self.assertLessEqual(
            uploaded, unique, "an identity is never uploaded twice on one load"
        )
        self.assertEqual(missing, 0, "the demo has no broken materials")
        self.assertEqual(mode, "linear", "linear is the shipped default")
        self.assertEqual(edge, 1024, "the shipped sheets are native 1024px")
        self.assertGreater(resident, 0, output)
        self.assertLess(
            unique,
            draws,
            "surfaces share one GPU upload per texture identity, not one per draw",
        )

    def test_a_hundred_frames_reuse_the_same_uploaded_textures(self):
        # The load-time lines are emitted once per level upload, so a run of
        # many frames must still show exactly one world upload and one texture
        # resolution: no per-frame decode, upload, mip generation or bind.
        code, output = self.run_binary(
            {"PLACES_BENCH_FRAMES": "100"}
        )

        self.assertEqual(code, 0, f"wgpu hundred-frame run failed:\n{output}")
        self.assertEqual(len(self.world_uploads(output)), 1, output)
        loads = self.texture_loads(output)
        self.assertEqual(len(loads), 1, f"100 frames, one texture load:\n{output}")
        self.assertGreater(loads[0][1], 0, "the load uploaded the world's sheets")
        self.assertLessEqual(
            loads[0][1], loads[0][0], "an identity is never uploaded twice"
        )
        material_loads = self.material_loads(output)
        self.assertEqual(
            len(material_loads), 1, f"100 frames, one material resolution:\n{output}"
        )
        self.assert_material_resolution_is_sane(material_loads[0], output)
        self.assert_no_gpu_failure(output)

    # ------------------------------------------------ Stage 7 material system

    def test_places_demo_resolves_materials_for_every_pass(self):
        code, output = self.run_binary({})

        self.assertEqual(code, 0, f"wgpu run failed:\n{output}")
        loads = self.material_loads(output)
        self.assertEqual(
            len(loads), 1, f"materials resolve once per level load:\n{output}"
        )
        load = loads[0]
        self.assert_material_resolution_is_sane(load, output)
        (
            materials,
            response,
            reflections,
            normal_maps,
            _uploads,
            _hits,
            _opaque,
            cutout,
            translucent,
            draws,
            response_mode,
            profile,
        ) = load

        # The demo's material-defined alpha and response must be present, not
        # silently dropped: its windows are translucent, its grille is a
        # cut-out, its panels carry normal maps and some surfaces are
        # reflection-eligible.
        self.assertGreaterEqual(materials, 20, output)
        self.assertGreaterEqual(response, 1, "the demo authors sheen and normals")
        self.assertGreaterEqual(reflections, 1, "the demo authors reflective materials")
        self.assertGreaterEqual(normal_maps, 2, "the metal and plastic panels")
        self.assertGreaterEqual(cutout, 1, "the demo's grille is a cut-out")
        self.assertGreaterEqual(translucent, 1, "the demo's glass panes blend")
        self.assertEqual(response_mode, "enabled", "Full draws the surface response")
        self.assertEqual(profile, "full", "full is the shipped default")
        self.assertLess(materials, draws, "materials are shared across draws")

    def test_the_quality_profile_gates_the_material_response(self):
        _full_code, full = self.run_binary(
            {"PLACES_QUALITY": "full"}
        )
        _low_code, low = self.run_binary(
            {"PLACES_QUALITY": "low"}
        )

        full_load = self.material_loads(full)[0]
        low_load = self.material_loads(low)[0]
        self.assert_material_resolution_is_sane(full_load, full)
        self.assert_material_resolution_is_sane(low_load, low)
        self.assertEqual(full_load[10], "enabled")
        self.assertEqual(low_load[10], "disabled")
        self.assertEqual(low_load[11], "low")
        # The response gate zeroes the sheen and therefore the reflection
        # weight, and no normal map is bound on Low; the albedo, tint, alpha
        # and pass classification are untouched.
        self.assertEqual(low_load[1], 0, "Low draws no surface response")
        self.assertEqual(low_load[2], 0, "Low reflects nothing")
        self.assertEqual(low_load[3], 0, "Low binds no normal map")
        self.assertEqual(full_load[6], low_load[6], "opaque draw count is profile-independent")
        self.assertEqual(full_load[7], low_load[7], "cut-out draw count is profile-independent")
        self.assertEqual(
            full_load[8], low_load[8], "translucent draw count is profile-independent"
        )

    def test_quality_profiles_fit_the_same_textures_to_their_budget(self):
        _full_code, full = self.run_binary(
            {"PLACES_QUALITY": "full"}
        )
        _low_code, low = self.run_binary(
            {"PLACES_QUALITY": "low"}
        )

        full_load = self.texture_loads(full)[0]
        low_load = self.texture_loads(low)[0]
        self.assert_texture_resolution_is_sane(full_load, full)
        self.assert_texture_resolution_is_sane(low_load, low)
        self.assertEqual(
            full_load[0], low_load[0], "both profiles use the same texture identities"
        )
        self.assertEqual(full_load[7], 1024, "Full keeps the native sheet size")
        # Low fits every world sheet to the 256px budget. The shared fallback
        # sheet is a profile-independent 1024 renderer-lifetime resource and is
        # part of the draw set, so the reported maximum edge can stay 1024;
        # what must shrink is the resident texel storage. A complete
        # 1024 -> 256 fit is a 16x texel reduction, so a quarter of Full's
        # total is a generous ceiling that still fails if the fit stops
        # running.
        self.assertLess(
            low_load[6] * 4,
            full_load[6],
            "the Low fit must reduce resident texels to a fraction of Full's",
        )

    # -------------------------------------------------------- Stage 5 reload

    def test_a_second_level_replaces_the_uploaded_world(self):
        state = os.path.join(SMOKE_ROOT, "reload-state")
        shutil.rmtree(state, ignore_errors=True)
        _write_level(state, _second_level())

        code, output = self.run_binary(
            {
                "PLACES_STATE_ROOT": state,
                "PLACES_LEVEL": SECOND_LEVEL_ID,
            }
        )

        self.assertEqual(code, 0, f"wgpu reload run failed:\n{output}")
        self.assertIn(
            f"PLACES_LEVEL: loading 'wgpu Smoke Second' ({SECOND_LEVEL_ID})",
            output,
            output,
        )
        uploads = self.world_uploads(output)
        self.assertEqual(
            len(uploads), 2, f"boot demo then the requested level:\n{output}"
        )
        first, second = uploads
        self.assertGreater(first[2], 0, "the boot level uploads draws")
        self.assertGreater(second[0], 0, "the second level uploads vertices")
        self.assertGreater(second[1], 0, "the second level uploads indices")
        self.assertGreater(second[2], 0, "the second level uploads draws")
        self.assertNotEqual(
            first, second, "the second level must replace the first world"
        )

        # One texture resolution per level load, never per frame.
        loads = self.texture_loads(output)
        self.assertEqual(len(loads), 2, output)
        for load in loads:
            self.assert_texture_resolution_is_sane(load, output)
        # The second level's default materials are the ones the demo already
        # uploaded, so the renderer-lifetime cache serves them without a
        # re-decode and without a re-upload.
        self.assertEqual(
            loads[1][1],
            0,
            f"a reload must reuse cached textures, not upload them again:\n{output}",
        )
        self.assertEqual(loads[1][3], 0, "the second level resolves every material")

        # Materials resolve once per level load too, and the second level's
        # materials are all fresh GPU states (uniforms and bind groups are
        # level-scoped) with no normal-map uploads.
        material_loads = self.material_loads(output)
        self.assertEqual(len(material_loads), 2, output)
        for load in material_loads:
            self.assert_material_resolution_is_sane(load, output)
        self.assertGreaterEqual(material_loads[1][0], 1, output)
        self.assertEqual(material_loads[1][4], 0, "no normal maps in the second level")
        self.assert_no_gpu_failure(output)

    def test_an_empty_level_draws_nothing_and_still_presents(self):
        state = os.path.join(SMOKE_ROOT, "empty-state")
        shutil.rmtree(state, ignore_errors=True)
        _write_level(state, _empty_level())

        code, output = self.run_binary(
            {
                "PLACES_STATE_ROOT": state,
                "PLACES_LEVEL": EMPTY_LEVEL_ID,
            }
        )

        self.assertEqual(code, 0, f"wgpu empty-level run failed:\n{output}")
        uploads = self.world_uploads(output)
        self.assertEqual(len(uploads), 2, output)
        self.assertEqual(
            uploads[1],
            (0, 0, 0, 0),
            "a level with no architecture must upload nothing and not panic",
        )
        loads = self.texture_loads(output)
        self.assertEqual(len(loads), 2, output)
        self.assertEqual(
            loads[1],
            (0, 0, 0, 0, 0, 0, 0, 0, "linear"),
            "an empty world samples no textures",
        )
        material_loads = self.material_loads(output)
        self.assertEqual(len(material_loads), 2, output)
        self.assert_material_resolution_is_sane(material_loads[1], output)
        self.assertEqual(
            material_loads[1][:10],
            (0, 0, 0, 0, 0, 0, 0, 0, 0, 0),
            "an empty world resolves no materials and draws nothing",
        )
        self.assert_no_gpu_failure(output)

    def test_a_material_less_level_uses_the_fallback_sheet(self):
        state = os.path.join(SMOKE_ROOT, "notexture-state")
        shutil.rmtree(state, ignore_errors=True)
        _write_level(state, _notexture_level())

        code, output = self.run_binary(
            {
                "PLACES_STATE_ROOT": state,
                "PLACES_LEVEL": NOTEXTURE_LEVEL_ID,
            }
        )

        self.assertEqual(code, 0, f"wgpu no-texture run failed:\n{output}")
        loads = self.texture_loads(output)
        self.assertEqual(len(loads), 2, output)
        unique, uploaded, _hits, fallbacks, missing, draws, resident, edge, _mode = loads[1]
        self.assertEqual(unique, 0, "a material-less level resolves no base texture")
        self.assertEqual(uploaded, 0, "the fallback is uploaded once at startup")
        self.assertEqual(fallbacks, draws, "every draw samples the fallback sheet")
        self.assertEqual(missing, 0, "an absent material is not a broken one")
        # The fallback is the committed production white sheet, measured from
        # the asset: one un-mipped RGBA8 level, so the resident bytes are
        # exactly the sheet's own texel storage and the edge is its longest
        # side. Pinning the asset's real dimensions keeps this honest without
        # freezing a historical placeholder size.
        fallback = os.path.join(ROOT, "assets", "core", "textures", "white_01.png")
        with open(fallback, "rb") as handle:
            header = handle.read(24)
        self.assertEqual(
            header[:8], b"\x89PNG\r\n\x1a\n", "the fallback sheet must be a PNG"
        )
        width, height = struct.unpack(">II", header[16:24])
        self.assertEqual(edge, max(width, height), "the fallback's level-0 edge")
        self.assertEqual(
            resident,
            width * height * 4,
            "the fallback is one un-mipped RGBA8 level of the committed sheet",
        )

        # Every draw resolves the plain material state: no response, no normal
        # map, all opaque.
        material_loads = self.material_loads(output)
        self.assertEqual(len(material_loads), 2, output)
        self.assert_material_resolution_is_sane(material_loads[1], output)
        self.assertEqual(material_loads[1][1:4], (0, 0, 0), output)
        self.assertEqual(material_loads[1][7:9], (0, 0), "no cut-out or translucent range")
        self.assert_no_gpu_failure(output)

    def test_an_unknown_material_resolves_to_the_diagnostic_texture(self):
        state = os.path.join(SMOKE_ROOT, "missing-state")
        shutil.rmtree(state, ignore_errors=True)
        _write_level(state, _missing_level())

        code, output = self.run_binary(
            {
                "PLACES_STATE_ROOT": state,
                "PLACES_LEVEL": MISSING_LEVEL_ID,
            }
        )

        self.assertEqual(code, 0, f"wgpu missing-material run failed:\n{output}")
        loads = self.texture_loads(output)
        self.assertEqual(len(loads), 2, output)
        unique, uploaded, _hits, fallbacks, missing, draws, _resident, edge, _mode = loads[1]
        self.assertEqual(unique, 1, "every broken material shares one diagnostic sheet")
        self.assertEqual(uploaded, 1, output)
        self.assertEqual(fallbacks, 0, "a broken material still names a texture")
        self.assertEqual(missing, draws, "every draw reports the missing pattern")
        self.assertEqual(edge, 64, "the diagnostic pattern is 64x64")

        # The diagnostic material is opaque and carries no response, so the
        # degraded whole-material rule is visible in the material line.
        material_loads = self.material_loads(output)
        self.assertEqual(len(material_loads), 2, output)
        self.assert_material_resolution_is_sane(material_loads[1], output)
        self.assertEqual(material_loads[1][1], 0, "a broken material loses its response")
        self.assertEqual(material_loads[1][3], 0, "and its normal map")
        self.assert_no_gpu_failure(output)

    # ------------------------------------------------------ Stage 8 lighting

    def test_the_wgpu_build_reports_the_baked_lighting_and_its_occluders(self):
        # The reference has no realtime lights: the bake is the lighting. The
        # one load-time lighting diagnostic must show a real bake with real
        # occluders, and the Stage 8 build must draw no lightmap atlas.
        code, output = self.run_binary({})

        self.assertEqual(code, 0, f"wgpu run failed:\n{output}")
        line = re.search(
            r"\[lighting\] baked (\d+) room\(s\) / (\d+) baseline area\(s\) from "
            r"(\d+) fixture\(s\): (\d+) wall \+ (\d+) slab \+ (\d+) prop blocker\(s\), "
            r"baselines ([\d.]+)\.\.([\d.]+) \(avg ([\d.]+)\)",
            output,
        )
        self.assertIsNotNone(line, f"no lighting diagnostic:\n{output}")
        rooms, zones, fixtures, walls, slabs, props = (int(line.group(i)) for i in range(1, 7))
        minimum, maximum, average = (float(line.group(i)) for i in range(7, 10))
        self.assertGreater(rooms, 0, "Places Demo has rooms")
        self.assertGreaterEqual(zones, rooms, "an unpartitioned room is one baseline area")
        self.assertGreater(fixtures, 0, "Places Demo has fixtures")
        self.assertGreater(walls, 0, "the bake must have wall solids")
        self.assertGreater(slabs, 0, "the bake must have floor/ceiling slabs")
        self.assertGreater(props, 0, "the bake must have prop occluders")
        self.assertGreater(minimum, 0.0, "the ambient floor is positive")
        self.assertLessEqual(minimum, maximum)
        self.assertLessEqual(maximum, 1.0, "the bake clamps to MAX_BRIGHTNESS")
        self.assertLessEqual(average, maximum)
        self.assertGreaterEqual(average, minimum)
        # Stage 9 builds the reference's atlas: Places Demo packs two pages at
        # both profiles, so the diagnostic must name a real atlas.
        atlas = re.search(r"\[lightmaps\] (\d+) page\(s\), (\d+) chart\(s\)", output)
        self.assertIsNotNone(atlas, f"no lightmap diagnostic:\n{output}")
        self.assertGreaterEqual(int(atlas.group(1)), 1, "the demo bakes an atlas")
        self.assertGreaterEqual(int(atlas.group(2)), 1, "the atlas has charts")
        self.assert_no_gpu_failure(output)

    def test_the_wgpu_capture_writes_the_drawable_as_a_png(self):
        capture = os.path.join(SMOKE_ROOT, "stage8-capture.png")
        if os.path.exists(capture):
            os.remove(capture)

        code, output = self.run_binary(
            {"PLACES_CAPTURE": capture}
        )

        self.assertEqual(code, 0, f"wgpu capture run failed:\n{output}")
        self.assertIn("PLACES_CAPTURE: wrote", output, output)
        self.assert_no_gpu_failure(output)
        self.assertTrue(os.path.isfile(capture), output)
        data = open(capture, "rb").read()
        self.assertEqual(data[:8], b"\x89PNG\r\n\x1a\n", "not a PNG")
        self.assertEqual(data[12:16], b"IHDR", "the first chunk is IHDR")
        width, height = struct.unpack(">II", data[16:24])
        drawable = re.search(r"drawable: (\d+)x(\d+)", output)
        self.assertIsNotNone(drawable, output)
        self.assertEqual(width, int(drawable.group(1)), "capture width")
        self.assertEqual(height, int(drawable.group(2)), "capture height")
        self.assertGreater(
            len(data), 8_192, "an all-one-colour image compresses far smaller"
        )

    # ------------------------------------------------------------- profiles

    def test_both_quality_profiles_draw_the_same_world(self):
        full_code, full = self.run_binary(
            {"PLACES_QUALITY": "full"}
        )
        low_code, low = self.run_binary(
            {"PLACES_QUALITY": "low"}
        )

        self.assertEqual(full_code, 0, f"full-profile run failed:\n{full}")
        self.assertEqual(low_code, 0, f"low-profile run failed:\n{low}")
        # Stage 9 builds the reference's lightmap atlas; the profile selects its
        # texel density and page size, and the world draw set (geometry, ranges
        # and chunks) stays the same.
        self.assertEqual(
            self.world_uploads(full)[0],
            self.world_uploads(low)[0],
            "the world draw set is profile-independent",
        )
        self.assert_no_gpu_failure(full)
        self.assert_no_gpu_failure(low)


if __name__ == "__main__":
    unittest.main(verbosity=2)
