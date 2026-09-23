"""Repository-level checks for Places.

These run on the source tree without a GPU. They cover the shipped level files,
the asset catalog those levels reference, the Spooner-Man entity migration and
the crate release metadata.
"""

from __future__ import annotations

import json
import re
import struct
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

PACKAGE = Path(__file__).resolve().parent.parent

# The asset validator is the single source of catalog truth for tooling.
sys.path.insert(0, str(PACKAGE / "tools" / "assets"))
import validate  # noqa: E402

DECAL_SURFACES = {"floor", "ceiling", "wall_north", "wall_south", "wall_east", "wall_west"}

# Texture dimension policy, mirrored from `src/assets.rs`: `MAX_TEXTURE_DIMENSION`
# is the hard runtime limit, `PREFERRED_TEXTURE_DIMENSION` the soft warning
# budget, and `MAX_SURFACE_TEXTURE_BYTES` the per-sheet decoded budget. Python
# cannot import the Rust constants, so the shared numbers are named here and the
# Rust policy unit tests (`assets::tests`) pin the same contract.
MAX_TEXTURE_DIMENSION = 1024
PREFERRED_TEXTURE_DIMENSION = 256
# The upgraded artwork is deliberately high resolution, not seed-size: the
# Office/Pool surfaces and the NO DIVING sign were raised to the hard budget.
HIGH_RESOLUTION_TEXTURE_MINIMUM = 512


def cargo_version() -> str:
    text = (PACKAGE / "Cargo.toml").read_text(encoding="utf-8")
    match = re.search(r'^version = "([^"]+)"', text, re.MULTILINE)
    assert match, "Cargo.toml has no package version"
    return match.group(1)


def catalog() -> dict:
    return validate.load_catalog(str(PACKAGE / "assets" / "catalog.json"))


def catalog_entries(asset_type: str | None = None) -> list[dict]:
    entries = validate.catalog_entries(catalog())
    if asset_type is None:
        return entries
    return [entry for entry in entries if entry.get("asset_type") == asset_type]


def catalog_ids() -> set[str]:
    return {entry["id"] for entry in catalog_entries()}


def level_files() -> list[Path]:
    return sorted((PACKAGE / "assets" / "levels").glob("*.json"))


def fixture_files() -> list[Path]:
    """Engine regression fixtures; these are not shipped with the game."""
    return sorted((PACKAGE / "tests" / "fixtures" / "levels").glob("*.json"))


def custom_level_files() -> list[Path]:
    return sorted((PACKAGE / "levels").glob("*.json"))


def load_level(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def rooms_of(level: dict) -> list[dict]:
    """Every room section, merging the single-room and multi-room spellings."""
    rooms = list(level.get("rooms", []))
    if level.get("room"):
        rooms.append(level["room"])
    return rooms


class RepositoryTests(unittest.TestCase):
    def test_version_matches_the_changelog(self):
        version = cargo_version()
        newest = re.search(
            r"^## (\S+) — (\d{4}-\d{2}-\d{2})$",
            (PACKAGE / "CHANGELOG.md").read_text(encoding="utf-8"),
            re.MULTILINE,
        )
        self.assertIsNotNone(newest, "CHANGELOG.md needs a dated release heading")
        self.assertEqual(newest.group(1), version)

    def test_crate_metadata_describes_places(self):
        cargo = (PACKAGE / "Cargo.toml").read_text(encoding="utf-8")
        self.assertIn('name = "liminal-rust"', cargo)
        self.assertIn('description = "Places: a slow first-person exploration experience"', cargo)
        self.assertIn('repository = "https://github.com/csd113/Places"', cargo)

    def test_icon_is_a_small_non_interlaced_png(self):
        data = (PACKAGE / "icon.png").read_bytes()
        self.assertTrue(data.startswith(b"\x89PNG\r\n\x1a\n"))
        width, height, depth, color, _, _, interlace = struct.unpack(
            ">IIBBBBB", data[16:29]
        )
        self.assertLessEqual(width, 512)
        self.assertLessEqual(height, 512)
        self.assertGreater(width, 0)
        self.assertGreater(height, 0)
        self.assertEqual(interlace, 0, "the icon must not be interlaced")
        self.assertIn((color, depth), {(6, 8), (2, 8), (3, 8), (0, 8)})


class ShippedLevelTests(unittest.TestCase):
    def test_places_demo_is_the_only_bundled_level(self):
        shipped = {path.stem: load_level(path) for path in level_files()}
        self.assertEqual(
            set(shipped),
            {"places_demo"},
            "Places Demo must be the only level bundled with the game",
        )
        demo = shipped["places_demo"]
        self.assertEqual(demo["id"], "places_demo")
        self.assertEqual(demo["name"], "Places Demo")
        self.assertEqual(demo["format_version"], 1)

    def test_every_shipped_level_has_rooms_walls_light_and_an_inside_spawn(self):
        for path in level_files():
            level = load_level(path)
            rooms = rooms_of(level)
            self.assertGreaterEqual(len(rooms), 1, f"{path.name} has no rooms")
            self.assertGreaterEqual(len(level["walls"]), 1, f"{path.name} has no walls")
            self.assertGreaterEqual(
                len(level["ceiling_lights"]), 1, f"{path.name} has no fixtures"
            )
            spawn = level["spawn"]
            inside = any(
                room["x"] <= spawn["x"] <= room["x"] + room["width"]
                and room["z"] <= spawn["z"] <= room["z"] + room["depth"]
                for room in rooms
            )
            self.assertTrue(inside, f"{path.name}: the spawn is outside every room")

    def test_materials_are_real_core_ids(self):
        known_materials = {
            entry["id"]
            for entry in catalog_entries()
            if entry["asset_type"] in ("material", "light")
        }
        for path in level_files():
            level = load_level(path)
            defaults = level["defaults"]
            used = {defaults["wall"], defaults["floor"], defaults["ceiling"]}
            for room in rooms_of(level):
                used.add(room.get("material", defaults["floor"]))
                used.add(room.get("ceiling_material", defaults["ceiling"]))
            for wall in level["walls"]:
                used.add(wall.get("material", defaults["wall"]))
                used.update(wall.get("faces", {}).values())
            for patch in level.get("floor_patches", []):
                used.add(patch["material"])
            for light in level["ceiling_lights"]:
                used.add(light["fixture"])
            unknown = {mid for mid in used if mid.startswith("core:")} - known_materials
            self.assertEqual(unknown, set(), f"{path.name} uses unknown core ids")

    def test_decals_use_known_sheets_and_surfaces(self):
        known_sheets = {entry["id"] for entry in catalog_entries("decal")}
        levels_with_decals = 0
        for path in level_files():
            level = load_level(path)
            for index, decal in enumerate(level.get("decals", [])):
                self.assertIn(
                    decal["material"],
                    known_sheets,
                    f"{path.name}: decal {index} uses an unknown sheet",
                )
                self.assertIn(
                    decal["surface"],
                    DECAL_SURFACES,
                    f"{path.name}: decal {index} targets an unknown surface",
                )
                for axis in ("width", "height"):
                    self.assertGreater(decal[axis], 0.0, f"{path.name}: decal {index} {axis}")
                    self.assertLessEqual(decal[axis], 10.0, f"{path.name}: decal {index} {axis}")
            if level.get("decals"):
                levels_with_decals += 1
        self.assertGreaterEqual(levels_with_decals, 1, "no shipped level demonstrates decals")

    def test_props_come_from_the_shipped_catalogue(self):
        known = {entry["id"] for entry in validate.placeable_entries(catalog())}
        for path in level_files():
            level = load_level(path)
            for prop in level.get("props", []):
                self.assertIn(prop["model"], known, f"{path.name} places {prop['model']}")

    def test_openings_are_wide_enough_to_walk_through(self):
        for path in level_files():
            level = load_level(path)
            for index, wall in enumerate(level["walls"]):
                length = max(wall["width"], wall["depth"])
                for opening in wall.get("openings", []):
                    self.assertGreater(opening["width"], 0.0)
                    self.assertGreaterEqual(opening.get("sill", 0.0), 0.0)
                    self.assertLessEqual(
                        opening["offset"] + opening["width"],
                        length + 1e-3,
                        f"{path.name}: wall {index} opening runs past the wall",
                    )
                    if opening.get("kind") in ("door", "passage"):
                        self.assertGreaterEqual(
                            opening["width"],
                            1.0,
                            f"{path.name}: wall {index} has an unpastable opening",
                        )
                        self.assertGreaterEqual(opening["height"], 1.9)

    def test_light_intensities_are_sane(self):
        for path in level_files():
            level = load_level(path)
            for light in level["ceiling_lights"]:
                brightness = light.get("brightness", light.get("intensity"))
                if brightness is None:
                    continue
                self.assertGreaterEqual(brightness, 0.0)
                self.assertLessEqual(brightness, 8.0, "intensity would be clamped")


class AssetCatalogTests(unittest.TestCase):
    """The asset architecture: identity, classes, themes, entities, resources."""

    def test_the_catalog_and_shipped_levels_validate_cleanly(self):
        errors, _ = validate.validate_catalog(catalog())
        self.assertEqual(errors, [], "the shipped catalog does not validate")
        errors, _ = validate.validate_levels(catalog())
        self.assertEqual(errors, [], "shipped levels reference unknown assets")

    def test_the_builtin_environment_themes_exist(self):
        themes = {theme["id"]: theme for theme in catalog().get("themes", [])}
        for theme_id in ("office", "pool"):
            self.assertIn(theme_id, themes, f"the {theme_id} theme is missing")
            self.assertTrue(themes[theme_id].get("display_name"))

    def test_logical_ids_are_separate_from_physical_paths(self):
        # Levels store logical ids; a physical path never leaks into level JSON.
        for path in level_files() + fixture_files() + custom_level_files():
            text = path.read_text(encoding="utf-8")
            self.assertNotIn(".glb", text, f"{path.name} stores a model file path")
            self.assertNotIn(".png", text, f"{path.name} stores a texture file path")
            self.assertNotIn("assets/", text, f"{path.name} stores a physical asset path")

    def test_office_content_is_classified_but_generic_content_is_not_forced(self):
        by_id = {entry["id"]: entry for entry in catalog_entries()}
        office_props = [
            "core:desk",
            "core:chair",
            "core:cabinet",
            "core:water_cooler",
            "core:vending_machine",
        ]
        for prop_id in office_props:
            self.assertEqual(by_id[prop_id].get("theme"), "office", prop_id)
            self.assertTrue(
                by_id[prop_id]["model"].startswith("environment/office/"),
                f"{prop_id} is not organized under the office environment",
            )
        # Shared props stay generic rather than being forced into a theme.
        self.assertNotIn("theme", by_id["core:couch"])
        self.assertNotIn("theme", by_id["core:bed"])
        # The office material set and fixture carry the theme.
        for material_id in (
            "core:wallpaper_yellow_01",
            "core:carpet_beige_01",
            "core:ceiling_panel_01",
            "core:wallpaper_stained_01",
            "core:carpet_damp_01",
            "core:ceiling_stained_01",
            "core:fluorescent_panel_01",
        ):
            self.assertEqual(by_id[material_id].get("theme"), "office", material_id)

    def test_themes_organize_without_restricting_placement(self):
        # The official demo mixes Office, generic and Pool assets, and the prop
        # regression fixture mixes in an entity; nothing in the catalog or level
        # format gates placement by theme.
        demo = load_level(PACKAGE / "assets" / "levels" / "places_demo.json")
        placed = {prop["model"] for prop in demo.get("props", [])}
        for expected in ("core:desk", "core:pool_ladder"):
            self.assertIn(expected, placed)
        fixture = load_level(PACKAGE / "tests" / "fixtures" / "levels" / "prop_showcase.json")
        fixture_placed = {prop["model"] for prop in fixture.get("props", [])}
        for expected in ("core:couch", "spooner-man"):
            self.assertIn(expected, fixture_placed)
        self.assertTrue(
            validate.placeable_entries(catalog()),
            "every theme's assets resolve through one placeable lookup",
        )

    def test_spooner_man_is_one_canonical_entity_resource(self):
        entries = [entry for entry in catalog_entries() if entry["id"] == "spooner-man"]
        self.assertEqual(len(entries), 1, "Spooner-Man needs exactly one catalog entry")
        spooner = entries[0]
        self.assertEqual(spooner["asset_class"], "entity")
        self.assertEqual(spooner["asset_type"], "entity")
        self.assertNotIn("theme", spooner, "an entity is a class, not a theme")
        self.assertIn("entities/spooner-man/", spooner["model"])
        self.assertTrue((PACKAGE / "assets" / spooner["model"]).is_file())
        self.assertFalse(
            (PACKAGE / "assets" / "props" / "models" / "spooner-man.glb").exists(),
            "the legacy prop copy must not survive the migration",
        )
        referencing = [
            path.name
            for path in level_files() + fixture_files() + custom_level_files()
            if '"model": "spooner-man"' in path.read_text(encoding="utf-8")
        ]
        self.assertTrue(referencing, "no level still references spooner-man")

    def _validate(self, entries, themes=None):
        base = catalog()
        return validate.validate_catalog(
            {"themes": base["themes"] if themes is None else themes, "assets": entries}
        )

    def _validate_with_mutation(self, mutate):
        """Validates the shipped catalog with one entry changed in place."""
        entries = [dict(entry) for entry in catalog_entries()]
        mutate(entries)
        return validate.validate_catalog({"themes": catalog()["themes"], "assets": entries})

    def _validate_level_document(self, level):
        """Runs the level checks against one temporary level document."""
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "probe.json"
            path.write_text(json.dumps(level), encoding="utf-8")
            return validate.validate_levels(catalog(), level_dirs=(directory,))

    def _emissive_errors(self, material_id="core:wallpaper_yellow_01", **fields):
        def mutate(entries):
            material = next(entry for entry in entries if entry["id"] == material_id)
            material.update(fields)
        errors, _ = self._validate_with_mutation(mutate)
        return errors

    def test_the_validator_rejects_broken_catalogs(self):
        placeables = validate.placeable_entries(catalog())
        spooner = next(entry for entry in catalog_entries() if entry["id"] == "spooner-man")
        without_spooner = [entry for entry in placeables if entry["id"] != "spooner-man"]

        # Duplicate logical ids are an error, never last-one-wins.
        errors, _ = self._validate(placeables + [placeables[0]])
        self.assertTrue(any("duplicate logical asset id" in e for e in errors), errors)

        # Missing file assets and missing canonical resources are errors.
        missing = dict(placeables[0], model="environment/office/props/models/nope.glb")
        errors, _ = self._validate([missing])
        self.assertTrue(any("does not exist below assets/" in e for e in errors), errors)
        errors, _ = self._validate(without_spooner)
        self.assertTrue(any("spooner-man" in e for e in errors), errors)

        # Unknown classes and types are rejected; a future theme is a warning.
        errors, _ = self._validate([dict(placeables[0], asset_class="enviroment")])
        self.assertTrue(any("unknown asset_class" in e for e in errors), errors)
        errors, _ = self._validate([dict(placeables[0], asset_type="furniture")])
        self.assertTrue(any("unknown asset_type" in e for e in errors), errors)
        _, warnings = self._validate([dict(placeables[0], theme="hotel")])
        self.assertTrue(any("not declared" in w for w in warnings), warnings)

        # The built-in environment themes cannot silently disappear.
        errors, _ = self._validate(without_spooner, themes=[])
        self.assertTrue(any("office" in e for e in errors), errors)
        self.assertTrue(any("pool" in e for e in errors), errors)

        # Spooner-Man must be an entity, not a themed prop.
        errors, _ = self._validate(without_spooner + [dict(spooner, theme="office")])
        self.assertTrue(any("entity must not carry" in e for e in errors), errors)

    def test_the_validator_accepts_emissive_materials_and_prop_lights(self):
        # Emission is authored on a definition material: an RGB colour, an
        # optional intensity and an optional mask that resolves to a real
        # file-backed PNG texture exactly like the material's own texture.
        errors = self._emissive_errors(
            emissive=[1.0, 0.53, 0.2],
            emissive_intensity=2.0,
            emissive_mask="core:tex_wallpaper_yellow_01",
        )
        self.assertEqual(errors, [], "the shipped catalog plus emission must validate")

        # A minimal level that uses every new field: a ceiling fixture switched
        # off, and rect/point/line prop lights with the documented defaults.
        level = {
            "format_version": 1,
            "id": "schema_probe",
            "name": "Schema Probe",
            "spawn": {"x": 0.0, "z": 0.0, "yaw_degrees": 0.0},
            "defaults": {
                "wall": "core:wallpaper_yellow_01",
                "floor": "core:carpet_beige_01",
                "ceiling": "core:ceiling_panel_01",
            },
            "rooms": [{"x": -5.0, "z": -5.0, "width": 10.0, "depth": 10.0, "height": 3.5}],
            "props": [
                {
                    "model": "core:desk",
                    "x": 0.0,
                    "z": 0.0,
                    "lights": [
                        {
                            "shape": "rect",
                            "half_width": 0.3,
                            "half_depth": 0.05,
                            "offset": [0.0, 0.9, 0.25],
                            "rotation_degrees": 0.0,
                            "color": [0.53, 0.73, 1.0],
                            "intensity": 0.4,
                            "range": 3.0,
                            "falloff": "smooth",
                            "enabled": True,
                        },
                        {"shape": "point", "brightness": 0.5},
                        {"shape": "line", "length": 1.2, "falloff": "linear", "enabled": False},
                        {"color": [1.0, 0.9, 0.8]},  # no shape and no dimensions: a point
                    ],
                }
            ],
            "ceiling_lights": [
                {"fixture": "core:fluorescent_panel_01", "x": 0.0, "z": 0.0, "enabled": False},
                {"fixture": "core:fluorescent_panel_01", "x": 2.0, "z": 2.0},
            ],
        }
        errors, warnings = self._validate_level_document(level)
        self.assertEqual(errors, [], "a level authored with the new fields must validate")
        self.assertEqual(warnings, [], "a level authored with the new fields must not warn")

    def test_the_validator_rejects_malformed_emissive_materials(self):
        emissive = [1.0, 0.5, 0.25]
        for errors in (
            self._emissive_errors(emissive=[1.5, 0.0, 0.0]),
            self._emissive_errors(emissive="warm"),
            self._emissive_errors(emissive=[0.0, 0.0]),
            self._emissive_errors(emissive=[0.0, float("inf"), 0.0]),
        ):
            self.assertTrue(
                any("emissive must be three numbers in 0..1" in e for e in errors), errors
            )

        for errors in (
            self._emissive_errors(emissive=emissive, emissive_intensity=9.0),
            self._emissive_errors(emissive=emissive, emissive_intensity=float("nan")),
        ):
            self.assertTrue(
                any("emissive_intensity must be a number between 0 and 8" in e for e in errors),
                errors,
            )

        errors = self._emissive_errors(emissive_intensity=2.0)
        self.assertTrue(any("emissive_intensity requires emissive" in e for e in errors), errors)
        errors = self._emissive_errors(emissive_mask="core:tex_wallpaper_yellow_01")
        self.assertTrue(any("emissive_mask requires emissive" in e for e in errors), errors)

        for mask, needle in (
            ("core:tex_not_shipped", "is not in the catalog"),
            ("core:desk", "is not a texture asset"),
            ("not a valid id!", "malformed emissive_mask id"),
        ):
            errors = self._emissive_errors(emissive=emissive, emissive_mask=mask)
            self.assertTrue(any(needle in e for e in errors), (mask, errors))

    def test_the_validator_rejects_emissive_outside_definition_materials(self):
        def mutate_prop(entries):
            prop = next(entry for entry in entries if entry["id"] == "core:desk")
            prop["emissive"] = [1.0, 0.0, 0.0]

        errors, _ = self._validate_with_mutation(mutate_prop)
        self.assertTrue(
            any("emissive is only valid on a definition material" in e for e in errors), errors
        )

        def mutate_material(entries):
            material = next(entry for entry in entries if entry["id"] == "core:wallpaper_yellow_01")
            material["source"] = "generated"
            material["emissive"] = [1.0, 0.0, 0.0]
            material["emissive_intensity"] = 2.0

        errors, _ = self._validate_with_mutation(mutate_material)
        self.assertTrue(
            any("emissive is only valid on a definition material" in e for e in errors), errors
        )

    def test_the_validator_rejects_malformed_prop_lights(self):
        def level_with(lights, enabled="__missing__"):
            ceiling = {"fixture": "core:fluorescent_panel_01", "x": 0.0, "z": 0.0}
            if enabled != "__missing__":
                ceiling["enabled"] = enabled
            return {
                "format_version": 1,
                "id": "light_probe",
                "name": "Light Probe",
                "spawn": {"x": 0.0, "z": 0.0, "yaw_degrees": 0.0},
                "defaults": {
                    "wall": "core:wallpaper_yellow_01",
                    "floor": "core:carpet_beige_01",
                    "ceiling": "core:ceiling_panel_01",
                },
                "rooms": [{"x": -5.0, "z": -5.0, "width": 10.0, "depth": 10.0, "height": 3.5}],
                "props": [{"model": "core:desk", "x": 0.0, "z": 0.0, "lights": lights}],
                "ceiling_lights": [ceiling],
            }

        cases = (
            ("unknown shape", [{"shape": "sphere"}], "shape must be one of point, rect, line"),
            ("rect missing half_depth", [{"shape": "rect", "half_width": 0.3}], "a rect light needs half_depth"),
            ("rect zero half_width", [{"shape": "rect", "half_width": 0.0, "half_depth": 0.05}], "half_width must be a finite number > 0"),
            ("rect non-finite half_depth", [{"shape": "rect", "half_width": 0.3, "half_depth": float("inf")}], "half_depth must be a finite number > 0"),
            ("line missing length", [{"shape": "line"}], "a line light needs length"),
            ("line negative length", [{"shape": "line", "length": -1.0}], "length must be a finite number > 0"),
            ("bad falloff", [{"shape": "point", "falloff": "quadratic"}], "falloff must be one of smooth, linear, constant"),
            ("offset of two", [{"shape": "point", "offset": [0.0, 1.0]}], "offset must be exactly three finite numbers"),
            ("offset non-finite", [{"shape": "point", "offset": [0.0, float("nan"), 0.0]}], "offset must be exactly three finite numbers"),
            ("colour over one", [{"shape": "point", "color": [2.0, 0.0, 0.0]}], "color must be three numbers in 0..1"),
            ("colour wrong length", [{"shape": "point", "color": [0.0, 0.0]}], "color must be three numbers in 0..1"),
            ("negative intensity", [{"shape": "point", "intensity": -0.1}], "intensity cannot be negative"),
            ("non-finite brightness", [{"shape": "point", "brightness": float("nan")}], "brightness must be a finite number >= 0"),
            ("zero range", [{"shape": "point", "range": 0.0}], "range must be a finite number > 0"),
            ("non-boolean enabled", [{"shape": "point", "enabled": "on"}], "enabled must be a boolean"),
            ("non-object entry", ["bright"], "must be an object"),
            ("non-array lights", {"shape": "point"}, "lights must be an array"),
        )
        for label, lights, needle in cases:
            errors, _ = self._validate_level_document(level_with(lights))
            self.assertTrue(any(needle in error for error in errors), f"{label}: {errors}")

        # The engine default shape is inferred from the authored dimensions.
        errors, _ = self._validate_level_document(level_with([
            {"shape": "rect", "half_width": 0.3, "half_depth": 0.05},
            {"shape": "line", "length": 1.2},
            {"shape": "point"},
            {"color": [0.0, 1.0, 0.0]},
        ]))
        self.assertEqual(errors, [], "the documented minimum must validate")

        # Above the engine clamp the level still loads, so the validator warns.
        errors, warnings = self._validate_level_document(level_with([{"shape": "point", "intensity": 20.0}]))
        self.assertEqual(errors, [])
        self.assertTrue(any("clamped" in warning for warning in warnings), warnings)

        # `enabled` must be a boolean on a ceiling fixture too; null is not.
        for value in ("on", 1, None):
            errors, _ = self._validate_level_document(level_with([{"shape": "point"}], enabled=value))
            self.assertTrue(
                any("ceiling light 0 enabled must be a boolean" in e for e in errors), (value, errors)
            )

    def test_the_validator_checks_fixture_pool_and_emission_fields(self):
        def level_with(fields):
            ceiling = {"fixture": "core:fluorescent_panel_01", "x": 0.0, "z": 0.0}
            ceiling.update(fields)
            return {
                "format_version": 1,
                "id": "fixture_probe",
                "name": "Fixture Probe",
                "spawn": {"x": 0.0, "z": 0.0, "yaw_degrees": 0.0},
                "defaults": {
                    "wall": "core:wallpaper_yellow_01",
                    "floor": "core:carpet_beige_01",
                    "ceiling": "core:ceiling_panel_01",
                },
                "rooms": [{"x": -5.0, "z": -5.0, "width": 10.0, "depth": 10.0, "height": 3.5}],
                "ceiling_lights": [ceiling],
            }

        # The documented pool/emission fields validate.
        errors, warnings = self._validate_level_document(
            level_with({"range": 4.0, "falloff": "linear", "emission": 1.5, "enabled": False})
        )
        self.assertEqual(errors, [], errors)
        self.assertEqual(warnings, [])

        cases = (
            ("zero range", {"range": 0.0}, "range must be a finite number > 0"),
            ("nan range", {"range": float("nan")}, "range must be a finite number > 0"),
            ("unknown falloff", {"falloff": "inverse-square"}, "falloff must be one of smooth, linear, constant"),
            ("negative emission", {"emission": -0.5}, "emission must be a finite number >= 0"),
            ("nan emission", {"emission": float("nan")}, "emission must be a finite number >= 0"),
        )
        for label, fields, needle in cases:
            errors, _ = self._validate_level_document(level_with(fields))
            self.assertTrue(any(needle in error for error in errors), f"{label}: {errors}")

    def test_a_broken_catalog_surfaces_in_the_validators_exit_code(self):
        with tempfile.TemporaryDirectory() as directory:
            base = catalog()
            broken = dict(base)
            broken["assets"] = base["assets"] + [base["assets"][0]]
            catalog_path = Path(directory) / "catalog.json"
            catalog_path.write_text(json.dumps(broken), encoding="utf-8")
            self.assertEqual(validate.main(["--catalog", str(catalog_path), "--quiet"]), 1)


class EnvironmentTextureTests(unittest.TestCase):
    """Goal 4.5: environment surfaces ship as file-backed PNG texture assets."""

    LEGACY_MATERIAL_IDS = (
        "core:wallpaper_yellow_01",
        "core:carpet_beige_01",
        "core:ceiling_panel_01",
        "core:wallpaper_stained_01",
        "core:carpet_damp_01",
        "core:ceiling_stained_01",
    )

    def test_every_material_resolves_to_a_texture_asset(self):
        by_id = {entry["id"]: entry for entry in catalog_entries()}
        materials = catalog_entries("material")
        self.assertTrue(materials, "the catalog declares no materials")
        for material in materials:
            texture_id = material.get("texture")
            self.assertIn(texture_id, by_id, f"{material['id']}: texture {texture_id!r} is missing")
            texture = by_id[texture_id]
            self.assertEqual(texture["asset_type"], "texture", texture_id)
            self.assertEqual(texture["source"], "file", texture_id)
            self.assertTrue(texture["model"].endswith(".png"), texture_id)

    def test_every_texture_asset_file_exists_and_is_a_png(self):
        textures = catalog_entries("texture")
        self.assertGreaterEqual(len(textures), 15, "the texture set is incomplete")
        for texture in textures:
            path = PACKAGE / "assets" / texture["model"]
            self.assertTrue(path.is_file(), f"{texture['id']}: {texture['model']} is missing")
            data = path.read_bytes()
            self.assertTrue(data.startswith(b"\x89PNG\r\n\x1a\n"), texture["id"])
            width, height = struct.unpack(">II", data[16:24])
            self.assertGreater(width, 0, texture["id"])
            self.assertGreater(height, 0, texture["id"])
            self.assertLessEqual(width, 1024, texture["id"])
            self.assertLessEqual(height, 1024, texture["id"])

    def test_the_seed_texture_tool_validates_the_shipped_set(self):
        result = subprocess.run(
            [sys.executable, str(PACKAGE / "tools" / "textures" / "build.py"), "--check", "--quiet"],
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_every_shipped_environment_surface_tiles_cleanly(self):
        """A repeated surface must not show a seam where its edges meet.

        This is the tool-level half of the tiling contract: the Rust render test
        compares the wrapped edge against the sheet's own interior variation
        with its own implementation, and `tools/textures/seam_repair.py --check`
        independently measures the same property from the source tree. Both must
        agree before a sheet ships.
        """
        surfaces = [
            entry["model"]
            for entry in catalog_entries("texture")
            if entry.get("asset_class") == "environment"
        ]
        self.assertGreaterEqual(
            len(surfaces), 10, "the environment surface set is incomplete"
        )
        result = subprocess.run(
            [
                sys.executable,
                str(PACKAGE / "tools" / "textures" / "seam_repair.py"),
                "--check",
                *[str(PACKAGE / "assets" / model) for model in surfaces],
            ],
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_the_six_legacy_material_ids_still_exist(self):
        by_id = {entry["id"]: entry for entry in catalog_entries()}
        for material_id in self.LEGACY_MATERIAL_IDS:
            self.assertIn(material_id, by_id, f"{material_id} disappeared from the catalog")
            self.assertEqual(by_id[material_id]["asset_type"], "material", material_id)
            self.assertEqual(by_id[material_id]["source"], "definition", material_id)

    def test_every_light_fixture_ships_its_visible_face_as_a_png(self):
        """A fixture's mesh is generated, but its visible face is real artwork."""
        fixtures = catalog_entries("light")
        self.assertGreaterEqual(len(fixtures), 3, "the fixture set is incomplete")
        seen_sheets = set()
        for fixture in fixtures:
            self.assertEqual(fixture["source"], "file", fixture["id"])
            model = fixture.get("model")
            self.assertTrue(str(model).endswith(".png"), fixture["id"])
            self.assertNotIn(model, seen_sheets, f"{fixture['id']}: {model} is reused")
            seen_sheets.add(model)
            path = PACKAGE / "assets" / model
            self.assertTrue(path.is_file(), f"{fixture['id']}: {model} is missing")
            data = path.read_bytes()
            self.assertTrue(data.startswith(b"\x89PNG\r\n\x1a\n"), fixture["id"])
            width, height = struct.unpack(">II", data[16:24])
            self.assertGreater(width, 0, fixture["id"])
            self.assertGreater(height, 0, fixture["id"])
            self.assertLessEqual(width, 1024, fixture["id"])
            self.assertLessEqual(height, 1024, fixture["id"])
            # Fixture UVs never leave the sheet, so its dimensions must both be
            # powers of two for the ES 2.0 target.
            for dimension in (width, height):
                self.assertEqual(dimension & (dimension - 1), 0, f"{fixture['id']}: {dimension}")


class PoolContentTests(unittest.TestCase):
    """Goal 5: the Pool theme is real content, not a reserved category."""

    POOL_MATERIALS = (
        "core:pool_tile_deck_01",
        "core:pool_tile_basin_01",
        "core:pool_tile_wall_01",
        "core:pool_ceiling_01",
    )
    POOL_PROPS = (
        "core:pool_table",
        "core:pool_chair",
        "core:pool_ladder",
        "core:pool_curtain_straight",
        "core:pool_curtain_end",
        "core:pool_curtain_corner",
        "core:pool_guardrail_straight",
        "core:pool_guardrail_end",
        "core:pool_guardrail_corner",
    )
    POOL_FIXTURES = ("core:pool_light_round", "core:pool_light_wall")

    def test_pool_content_is_classified_and_organized(self):
        by_id = {entry["id"]: entry for entry in catalog_entries()}
        for material_id in self.POOL_MATERIALS:
            material = by_id[material_id]
            self.assertEqual(material["theme"], "pool", material_id)
            self.assertTrue(by_id[material["texture"]]["model"].startswith("environment/pool/"), material_id)
        for prop_id in self.POOL_PROPS:
            prop = by_id[prop_id]
            self.assertEqual(prop["theme"], "pool", prop_id)
            self.assertTrue(prop["model"].startswith("environment/pool/"), prop_id)
            self.assertTrue((PACKAGE / "assets" / prop["model"]).is_file(), prop_id)
        for fixture_id in self.POOL_FIXTURES:
            fixture = by_id[fixture_id]
            self.assertEqual(fixture["theme"], "pool", fixture_id)
            self.assertEqual(fixture["asset_type"], "light", fixture_id)
            self.assertEqual(fixture["source"], "file", fixture_id)
            self.assertTrue(fixture["model"].startswith("environment/pool/"), fixture_id)
            self.assertTrue(fixture["model"].endswith(".png"), fixture_id)
            self.assertTrue((PACKAGE / "assets" / fixture["model"]).is_file(), fixture_id)

    def test_the_no_diving_sign_is_external_cut_out_artwork(self):
        by_id = {entry["id"]: entry for entry in catalog_entries()}
        sign = by_id["core:decal_no_diving_01"]
        self.assertEqual(sign["asset_type"], "decal", "the sign is a decal")
        self.assertEqual(sign["source"], "file", "the sign must be external PNG artwork")
        path = PACKAGE / "assets" / sign["model"]
        self.assertTrue(path.is_file(), sign["model"])
        data = path.read_bytes()
        self.assertTrue(data.startswith(b"\x89PNG\r\n\x1a\n"), "the sign is not a PNG")
        colour_type = data[25]
        self.assertEqual(colour_type, 6, "the sign needs an alpha channel")
        width, height = struct.unpack(">II", data[16:24])
        # The current intentional contract: the upgraded sign is a square
        # power-of-two sheet authored at the hard 1024x1024 budget, not the old
        # 128x128 seed. The numbers come from the mirrored policy constants
        # above (see `src/assets.rs`).
        self.assertEqual(width, height, "the sign sheet must be square")
        self.assertTrue(
            width > 0 and (width & (width - 1)) == 0,
            f"the sign sheet must be power-of-two, found {width}",
        )
        self.assertGreaterEqual(
            width,
            HIGH_RESOLUTION_TEXTURE_MINIMUM,
            "the sign must be high-resolution upgraded artwork",
        )
        self.assertGreater(
            width,
            PREFERRED_TEXTURE_DIMENSION,
            "the upgraded sign intentionally exceeds the soft preferred budget",
        )
        self.assertLessEqual(
            width,
            MAX_TEXTURE_DIMENSION,
            "the sign must stay within the hard 1024 limit",
        )
        self.assertEqual(
            (width, height),
            (MAX_TEXTURE_DIMENSION, MAX_TEXTURE_DIMENSION),
            "the sign is the upgraded 1024x1024 sheet",
        )
        # The sheet must be a cut-out: some pixel is fully transparent, so the
        # decal pass has a silhouette to discard instead of a floating plate.
        import zlib

        offset = 8
        idat = b""
        while offset < len(data):
            length = struct.unpack(">I", data[offset : offset + 4])[0]
            tag = data[offset + 4 : offset + 8]
            if tag == b"IDAT":
                idat += data[offset + 8 : offset + 8 + length]
            offset += 12 + length
        raw = zlib.decompress(idat)
        stride = width * 4
        transparent = any(
            raw[row * (stride + 1) + 1 + column * 4 + 3] == 0
            for row in range(height)
            for column in range(width)
        )
        self.assertTrue(transparent, "the sign sheet has no transparent pixels")

    def test_the_pool_showcase_is_real_lowered_floor_geometry(self):
        level = load_level(PACKAGE / "tests" / "fixtures" / "levels" / "pool_showcase.json")
        regions = level.get("floor_regions", [])
        self.assertTrue(regions, "the pool basin must be a floor region, not a prop")
        offsets = [float(region.get("offset_y", 0.0)) for region in regions]
        self.assertTrue(any(offset <= -1.0 for offset in offsets), "no recessed basin")
        self.assertTrue(any(-0.4 < offset < 0 for offset in offsets), "no walkable entry step")
        # Every region is tiled, not left on the Office default.
        for region in regions:
            self.assertEqual(region.get("material"), "core:pool_tile_basin_01", region)
            self.assertEqual(region.get("edge_material"), "core:pool_tile_wall_01", region)
        # The walls put the sign on the deck and the fixtures in the room.
        materials = {decal.get("material") for decal in level.get("decals", [])}
        self.assertIn("core:decal_no_diving_01", materials)
        fixtures = {light.get("fixture") for light in level.get("ceiling_lights", [])}
        self.assertIn("core:pool_light_round", fixtures)
        self.assertIn("core:pool_light_wall", fixtures)
        # Wall fixtures carry a mount and a world height.
        for light in level.get("ceiling_lights", []):
            if light.get("fixture") == "core:pool_light_wall":
                self.assertEqual(light.get("mount"), "wall", light)
                self.assertIn("y", light, light)

    def test_the_pool_showcase_places_every_pool_prop(self):
        level = load_level(PACKAGE / "tests" / "fixtures" / "levels" / "pool_showcase.json")
        placed = {prop["model"] for prop in level.get("props", [])}
        for prop_id in self.POOL_PROPS:
            self.assertIn(prop_id, placed, f"pool_showcase.json must place {prop_id}")

    def test_every_pool_solid_prop_authors_its_collision_box(self):
        level = load_level(PACKAGE / "tests" / "fixtures" / "levels" / "pool_showcase.json")
        for prop in level.get("props", []):
            if prop.get("solid") is True:
                self.assertIn("size", prop, f"{prop['model']} needs an explicit collision size")
                self.assertTrue(all(value > 0 for value in prop["size"]), prop)

    def test_guardrails_and_the_ladder_have_collision(self):
        level = load_level(PACKAGE / "tests" / "fixtures" / "levels" / "pool_showcase.json")
        solid = {
            prop["model"]: prop
            for prop in level.get("props", [])
            if prop.get("solid") is True
        }
        for guardrail in (
            "core:pool_guardrail_straight",
            "core:pool_guardrail_end",
            "core:pool_guardrail_corner",
        ):
            self.assertIn(guardrail, solid, f"{guardrail} must block the player")
            self.assertIn("size", solid[guardrail], f"{guardrail} needs an explicit collision box")
        self.assertIn("core:pool_ladder", solid)


class SourceHygieneTests(unittest.TestCase):
    def test_the_package_excludes_build_output(self):
        # Cargo output and benchmark results are local development artifacts.
        ignored = (PACKAGE / ".gitignore").read_text(encoding="utf-8")
        for entry in ("/target", "tools/bench/results/"):
            self.assertIn(entry, ignored)

    def test_readme_documents_controls_and_prerequisites(self):
        readme = (PACKAGE / "README.md").read_text(encoding="utf-8")
        for needle in ("## Controls", "## Desktop prerequisites", "settings.json"):
            self.assertIn(needle, readme)
        self.assertIn("SDL2", readme)

    def test_readme_documents_wasd_and_arrow_defaults(self):
        readme = (PACKAGE / "README.md").read_text(encoding="utf-8")
        for row in (
            "| Walk forward | `W` |",
            "| Walk backward | `S` |",
            "| Strafe left | `A` |",
            "| Strafe right | `D` |",
            "| Look up | `UP` |",
            "| Look down | `DOWN` |",
            "| Look left | `LEFT` |",
            "| Look right | `RIGHT` |",
        ):
            self.assertIn(row, readme, f"README is missing the default row {row!r}")
        # The previous PocketCHIP-oriented defaults must no longer be presented
        # as the normal controls.
        for legacy in (
            "| Walk backward | `Z` |",
            "| Strafe right | `S` |",
            "| Look up | `O` |",
            "| Look down | `.` |",
            "| Look left | `K` |",
            "| Look right | `L` |",
        ):
            self.assertNotIn(legacy, readme, f"README still lists the old row {legacy!r}")


if __name__ == "__main__":
    unittest.main()
