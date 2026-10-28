"""Repository-level checks for Places.

These run on the source tree without a GPU. They cover the shipped level files,
the asset catalog those levels reference, the single canonical Spooner-Man
entity resource and the crate release metadata.
"""

from __future__ import annotations

import hashlib
import copy
import json
import re
import struct
import subprocess
import sys
import tempfile
import unittest
import zipfile
import zlib
from pathlib import Path

from tools.levels.curated_maps import environment_source, lineup, source_levels

PACKAGE = Path(__file__).resolve().parent.parent

# The asset validator is the single source of catalog truth for tooling.
sys.path.insert(0, str(PACKAGE / "tools" / "assets"))
import validate  # noqa: E402

# The prop painter's decoder is the repository's trusted PNG reader: it
# unfilters scanlines (filter types 0..4) and returns RGBA bytes. Importing
# `tex` resolves its own `from palette import shade` from this directory.
sys.path.insert(0, str(PACKAGE / "tools" / "props"))
import tex  # noqa: E402

sys.path.insert(0, str(PACKAGE / "tools" / "levels"))

DECAL_SURFACES = {"floor", "ceiling", "wall_north", "wall_south", "wall_east", "wall_west"}

# Texture dimension policy, mirrored from `src/engine/assets.rs`: `MAX_TEXTURE_DIMENSION`
# is the ordinary hard runtime limit, `MAX_SKY_TEXTURE_DIMENSION` the sky-only
# limit, `PREFERRED_TEXTURE_DIMENSION` the soft warning
# budget, and `MAX_SURFACE_TEXTURE_BYTES` the per-sheet decoded budget. Python
# cannot import the Rust constants, so the shared numbers are named here and the
# Rust policy unit tests (`assets::tests`) pin the same contract.
MAX_TEXTURE_DIMENSION = 1024
MAX_SKY_TEXTURE_DIMENSION = 2048
PREFERRED_TEXTURE_DIMENSION = 256
# The shipped artwork is deliberately high resolution, not placeholder-size: the
# Office/Pool surfaces and the NO DIVING sign were raised to the hard budget.
HIGH_RESOLUTION_TEXTURE_MINIMUM = 512


def cargo_version() -> str:
    text = (PACKAGE / "Cargo.toml").read_text(encoding="utf-8")
    match = re.search(r'^version = "([^"]+)"', text, re.MULTILINE)
    assert match, "Cargo.toml has no package version"
    return match.group(1)



def decode_png(path: Path):
    """Decodes a repository PNG as ``(width, height, pixels, channels)``.

    test_package is stdlib-only like the tooling it mirrors, so the decoder
    comes from the texture toolkit rather than from an image library.
    """
    sys.path.insert(0, str(PACKAGE / "tools" / "textures"))
    from seam_repair import read_png  # noqa: PLC0415

    image = read_png(str(path))
    channels = len(image.pixels) // (image.width * image.height)
    return image.width, image.height, image.pixels, channels


def pixel_mean(decoded) -> float:
    """Mean luminance of a decoded image, in 0..255."""
    _width, _height, pixels, channels = decoded
    total = 0
    count = 0
    for index in range(0, len(pixels), channels):
        total += pixels[index] + pixels[index + 1] + pixels[index + 2]
        count += 3
    return total / count


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
    """Authoring sources for the bundled levels."""
    return [path for _, source in lineup() for _, path in source_levels(source)]


def package_files() -> list[Path]:
    """Compiled packages for the bundled levels; these are what players load."""
    return sorted((PACKAGE / "assets" / "levels").glob("*.placesmap"))


def fixture_files() -> list[Path]:
    """Engine regression fixtures; these are not shipped with the game."""
    return sorted((PACKAGE / "tests" / "fixtures" / "levels").glob("*.json"))


def custom_level_files() -> list[Path]:
    maintained = set(level_files())
    return sorted(path for path in (PACKAGE / "levels").glob("*.json") if path not in maintained)


def load_level(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def rooms_of(level: dict) -> list[dict]:
    """Every room section of the level's single `rooms` array."""
    return list(level.get("rooms", []))


class RepositoryTests(unittest.TestCase):
    def test_version_matches_the_changelog(self):
        version = cargo_version()
        newest = re.search(
            r"^## (\S+) — (?:Unreleased|\d{4}-\d{2}-\d{2})$",
            (PACKAGE / "CHANGELOG.md").read_text(encoding="utf-8"),
            re.MULTILINE,
        )
        self.assertIsNotNone(newest, "CHANGELOG.md needs a versioned release or Unreleased heading")
        self.assertEqual(newest.group(1), version)

    def test_crate_metadata_describes_places(self):
        cargo = (PACKAGE / "Cargo.toml").read_text(encoding="utf-8")
        self.assertIn('name = "places"', cargo)
        self.assertIn('description = "Places: a slow first-person exploration experience"', cargo)
        self.assertIn('repository = "https://github.com/csd113/Places"', cargo)

    def test_icon_is_a_small_non_interlaced_png(self):
        data = (PACKAGE / "assets/shared/branding/places.png").read_bytes()
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
    def test_the_collection_contains_the_hub_and_every_environment(self):
        self.assertEqual([identity for identity, _ in lineup()], ["environments", "backrooms"])
        project = load_level(PACKAGE / "projects/environments/places.project.json")
        shipped = {
            entry["id"]: load_level(PACKAGE / "projects/environments" / entry["source"])
            for entry in project["levels"]
        }
        self.assertEqual(
            set(shipped),
            {"hub", "home", "pool", "office", "lantern_hollow", "winter", "beach", "aero", "arcade"},
            "the hub and eight environments are the complete editable collection",
        )
        self.assertEqual(
            {path.stem for path in level_files()}, set(shipped) | {"the_backrooms"},
            "the independent Backrooms map must remain in the curated lineup",
        )
        showcase = shipped["lantern_hollow"]
        self.assertEqual(showcase["id"], "lantern_hollow")
        self.assertEqual(showcase["name"], "Lantern Hollow")
        self.assertEqual(showcase["format_version"], 1)

        # Protect actual travel links as well as inventory, without compiling
        # worlds. A declared destination alone does not make its door usable.
        self.assertEqual(project["entry_level"], "hub")
        self.assertTrue(project["menu_entry_only"])
        destinations = set(shipped) - {"hub"}
        for identity, level in shipped.items():
            expected = destinations if identity == "hub" else {"hub"}
            self.assertEqual(set(level["transitions"]), expected, identity)
            links = []
            for door in level.get("doors", []):
                for binding in door.get("bindings", []):
                    for action in binding.get("actions", []):
                        if action["action"] == "change_level":
                            self.assertEqual(binding["on"], "interact", door["id"])
                            self.assertTrue(any(component["component"] == "interactable"
                                                for component in door.get("components", [])), door["id"])
                            links.append(action["level"])
            self.assertCountEqual(links, expected, identity)

    def test_retired_maps_preserve_historical_content_outside_active_discovery(self):
        # Two small hashes prevent accidental regeneration of the historical
        # record. No model inspection, renderer or production-map bake runs.
        expected = {
            "places_demo": "65833e98579a696167d616ce0cc94ebab85f744fdd72837ed247c483db1f9dec",
            "model_zoo": "71ff7d364cd8620f6800c0ab0bb65a41a43226b1579ec6ff8e9fa29477aedfd7",
        }
        for identity, digest in expected.items():
            path = PACKAGE / "docs/archive/levels" / (identity + ".json")
            source = path.read_bytes()
            self.assertEqual(json.loads(source)["schema"], "places-level")
            self.assertEqual(json.loads(source)["format_version"], 1)
            # Git text checkouts may use CRLF; pin the original LF source bytes.
            historical = source.replace(b'\r\n', b'\n').replace(b'  "schema": "places-level",\n', b'', 1)
            historical = historical.replace(b'"format_version": 1,', b'"format_version": 3,', 1)
            self.assertEqual(hashlib.sha256(historical).hexdigest(), digest)
            self.assertFalse((PACKAGE / "levels" / path.name).exists())
            self.assertNotIn(identity, (PACKAGE / "levels/lineup.txt").read_text().splitlines())

    def test_every_shipped_level_has_rooms_lighting_and_an_inside_spawn(self):
        for path in level_files():
            level = load_level(path)
            rooms = rooms_of(level)
            self.assertGreaterEqual(len(rooms), 1, f"{path.name} has no rooms")
            if level["id"] in ("beach", "beach_demo"):
                self.assertTrue(all(room.get("ceiling", {}).get("kind") == "open" for room in rooms))
                self.assertTrue(any(light["kind"] == "sun" and light["intensity"] > 0
                                    and light.get("enabled", True) and light.get("bake", True)
                                    for light in level.get("global_illuminators", [])),
                                "Beach has no daylight source")
                self.assertEqual(level["sky"]["texture"], "beach:tex_sky_day_01")
                self.assertTrue(level.get("props"), "Beach has no structural scenery")
            elif level["id"] in ("aero", "frutiger_aero_demo"):
                self.assertEqual(level["sky"]["texture"], "frutiger_aero:tex_sky_day_01")
                self.assertTrue(any(room.get("ceiling", {}).get("kind") == "open" for room in rooms))
                self.assertTrue(any(light["kind"] == "sun" and light["intensity"] > 0
                                    for light in level.get("global_illuminators", [])))
                self.assertTrue(any(prop.get("lights") for prop in level["props"]))
            else:
                self.assertGreaterEqual(len(level.get("walls", [])), 1, f"{path.name} has no walls")
                self.assertGreaterEqual(
                    len(level.get("ceiling_lights", [])), 1, f"{path.name} has no fixtures"
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
            for wall in level.get("walls", []):
                used.add(wall.get("material", defaults["wall"]))
                used.update(wall.get("faces", {}).values())
            for patch in level.get("floor_patches", []):
                used.add(patch["material"])
            for light in level.get("ceiling_lights", []):
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
            # The QA course deliberately exercises near-body clearances;
            # the showcases retain their existing comfortable doorway margins.
            minimum_width, minimum_height = (
                (0.7, 1.85) if level["id"] == "movement_test" else (1.0, 1.9)
            )
            for index, wall in enumerate(level.get("walls", [])):
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
                            minimum_width,
                            f"{path.name}: wall {index} has an unpastable opening",
                        )
                        # A passage starting above the standing 1.8 m body is
                        # a sightline aperture, not a walking route. Backrooms
                        # G deliberately has one; floor passages and all doors
                        # retain the existing standing-clearance requirement.
                        if opening["kind"] == "door" or opening.get("sill", 0.0) < 1.8:
                            self.assertGreaterEqual(opening["height"], minimum_height)

    def test_light_intensities_are_sane(self):
        for path in level_files():
            level = load_level(path)
            for light in level.get("ceiling_lights", []):
                brightness = light.get("brightness")
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
                by_id[prop_id]["model"].startswith("environments/office/"),
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
        # The bounded prop fixture mixes furniture and an entity through one
        # catalog; theme labels do not restrict placement.
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
            (PACKAGE / "assets" / "shared" / "props" / "spooner-man.glb").exists(),
            "the entity has exactly one canonical resource; no prop copy may exist",
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
        missing = dict(placeables[0], model="environments/office/props/nope.glb")
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
            "schema": "places-level", "format_version": 1,
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
                        {"shape": "point", "intensity": 0.5},
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

    def test_the_validator_checks_ultraviolet_material_contract(self):
        material_id = "arcade:carpet_cosmic_bowl_01"
        for value in (-0.01, 2.01, float("nan"), True, "bright"):
            errors = self._emissive_errors(material_id, uv_response=value)
            self.assertTrue(any("uv_response must be a finite" in e for e in errors), errors)
        errors = self._emissive_errors(material_id, uv_response=0)
        self.assertTrue(any("requires positive uv_response" in e for e in errors), errors)
        errors = self._emissive_errors(material_id, uv_response_mask="arcade:bowling_pin")
        self.assertTrue(any("is not a texture asset" in e for e in errors), errors)
        errors = self._emissive_errors(material_id, uv_response_mask="arcade:tex_absent_mask")
        self.assertTrue(any("is not in the catalog" in e for e in errors), errors)

        def mutate_prop(entries):
            prop = next(entry for entry in entries if entry["id"] == "core:desk")
            prop["uv_response"] = 1

        errors, _ = self._validate_with_mutation(mutate_prop)
        self.assertTrue(any("uv_response is only valid on a definition material" in e for e in errors), errors)

    def test_the_validator_counts_disabled_ultraviolet_sources(self):
        level = {
            "schema": "places-level", "format_version": 1, "id": "uv_probe", "name": "UV Probe",
            "spawn": {"x": 0, "z": 0},
            "rooms": [{"x": -5, "z": -5, "width": 10, "depth": 10, "height": 3.5}],
            "ceiling_lights": [
                {"fixture": "arcade:blacklight_tube", "x": x, "z": 0,
                 "ultraviolet": 1, "brightness": 0, "enabled": False}
                for x in (-2, -1, 0, 1)
            ],
        }
        errors, _ = self._validate_level_document(level)
        self.assertEqual(errors, [], errors)
        for value in (-0.1, 8.1, float("inf"), True, "UV"):
            invalid = copy.deepcopy(level)
            invalid["ceiling_lights"][0]["ultraviolet"] = value
            errors, _ = self._validate_level_document(invalid)
            self.assertTrue(any("ultraviolet must be a finite" in e for e in errors), errors)
        level["ceiling_lights"].append(dict(level["ceiling_lights"][0]))
        errors, _ = self._validate_level_document(level)
        self.assertTrue(any("at most 4 ultraviolet fixtures" in e for e in errors), errors)

    def test_the_validator_rejects_malformed_prop_lights(self):
        def level_with(lights, enabled="__missing__"):
            ceiling = {"fixture": "core:fluorescent_panel_01", "x": 0.0, "z": 0.0}
            if enabled != "__missing__":
                ceiling["enabled"] = enabled
            return {
                "schema": "places-level", "format_version": 1,
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

        # The larger static source set does not expand the runtime switch set.
        for count in (11, 16):
            errors, _ = self._validate_level_document(level_with([{"shape": "point"}] * count))
            self.assertEqual(errors, [], (count, errors))
        errors, _ = self._validate_level_document(level_with([{"shape": "point"}] * 17))
        self.assertTrue(any("limit is 16" in error for error in errors), errors)
        switched_level = level_with([{"shape": "point"}] * 9)
        switched_level["props"][0]["components"] = [{"component": "light", "switchable": True}]
        errors, _ = self._validate_level_document(switched_level)
        self.assertTrue(any("eight switched light sources" in error for error in errors), errors)
        errors = []
        twin_sources = validate.retained_fixture_source_count({"fixture": "core:pool_light_wall", "enabled": False})
        self.assertEqual(twin_sources, 2, "disabled physical strips retain both source identities")
        validate.validate_light_source_counts([999_998, twin_sources], "level", errors)
        self.assertEqual(errors, [], errors)
        validate.validate_light_source_counts([999_999, twin_sources], "level", errors)
        self.assertTrue(any("local light sources exceed 1000000" in error for error in errors), errors)

        # Slot overrides are numeric source metadata, so exercise malformed
        # boundaries directly without catalog I/O, model decoding, or map bakes.
        luminous_prop = {
            "components": [{"component": "light", "switchable": True}],
            "emission_overrides": {"4": {"color": [1, .8, .5], "intensity": .5}},
        }
        override_cases = (
            ("emission_overrides", None, "must be an object"),
            ("emission_overrides", [], "must be an object"),
            ("emission_overrides", {"16": {"color": [1, 1, 1], "intensity": 1}}, "slots in 0..15"),
            ("emission_overrides", {str(slot): {} for slot in range(17)}, "at most 16 material slots"),
            ("emission_overrides", {"-1": {}}, "slots in 0..15"),
            ("emission_overrides", {"１": {}}, "slots in 0..15"),
            ("emission_overrides", {"4": False}, "must be an object with color and intensity"),
            ("emission_overrides", {"4": {"color": [1, True, 0], "intensity": 1}}, "color must be three finite"),
            ("emission_overrides", {"4": {"color": [1, 0], "intensity": 1}}, "color must be three finite"),
            ("emission_overrides", {"4": {"color": [1, 1.01, 0], "intensity": 1}}, "color must be three finite"),
            ("emission_overrides", {"4": {"color": [1, -.01, 0], "intensity": 1}}, "color must be three finite"),
            ("emission_overrides", {"4": {"color": [1, float("nan"), 0], "intensity": 1}}, "color must be three finite"),
            ("emission_overrides", {"4": {"color": [1, 1, 1], "intensity": True}}, "intensity must be a finite"),
            ("emission_overrides", {"4": {"color": [1, 1, 1], "intensity": float("inf")}}, "intensity must be a finite"),
            ("emission_overrides", {"4": {"color": [1, 1, 1], "intensity": 8.01}}, "intensity must be a finite"),
            ("components", [], "fixed prop with a light component"),
            ("float", {}, "fixed prop with a light component"),
            ("physics", {"motion": "dynamic"}, "fixed prop with a light component"),
            ("components", [{"component": "light"}, {"component": "animation"}], "fixed prop with a light component"),
        )
        for field, value, needle in override_cases:
            invalid = copy.deepcopy(luminous_prop)
            invalid[field] = value
            errors = []
            validate.validate_prop_emission_overrides(invalid, "prop", errors)
            self.assertTrue(any(needle in error for error in errors), (field, value, errors))
        errors = []
        validate.validate_prop_emission_overrides(luminous_prop, "prop", errors)
        self.assertEqual(errors, [], errors)

        # The shared circuit requires matching authored states before baking.
        first_owner = dict(luminous_prop, switch_group="vending")
        second_owner = copy.deepcopy(first_owner)
        circuit = {"props": [first_owner, second_owner]}
        errors = []
        validate.validate_prop_light_circuits(circuit, "level", errors)
        self.assertEqual(errors, [], errors)
        second_owner["components"][0]["enabled"] = False
        validate.validate_prop_light_circuits(circuit, "level", errors)
        self.assertTrue(any("members must share enabled" in error for error in errors), errors)

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
                "schema": "places-level", "format_version": 1,
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

    def test_ramp_rise_is_signed_but_stair_rise_stays_positive(self):
        def errors_for(key, rise):
            errors = []
            piece = {"width": 1.0, "depth": 2.0, "rise": rise, "steps": 4}
            validate.validate_architecture({key: [piece]}, "probe", errors)
            return errors

        for rise in (-1.0, 1.0):
            self.assertEqual(errors_for("ramps", rise), [])
        for rise in (0.0, -0.001, 0.001, float("nan"), float("inf"), True, "steep"):
            self.assertTrue(errors_for("ramps", rise), f"invalid rise {rise!r}")
        self.assertTrue(errors_for("stairs", -1.0))

    def test_the_validator_checks_per_surface_shine(self):
        def level_with(**fields):
            level = {
                "schema": "places-level", "format_version": 1,
                "id": "shine_probe",
                "name": "Shine Probe",
                "spawn": {"x": 0.0, "z": 0.0, "yaw_degrees": 0.0},
                "defaults": {
                    "wall": "core:wallpaper_yellow_01",
                    "floor": "core:carpet_beige_01",
                    "ceiling": "core:ceiling_panel_01",
                },
                "rooms": [{"x": -5.0, "z": -5.0, "width": 10.0, "depth": 10.0, "height": 3.5}],
                "floor_patches": [
                    {
                        "x": 0.0,
                        "z": 0.0,
                        "width": 1.0,
                        "depth": 1.0,
                        "material": "core:linoleum_polished_01",
                    }
                ],
            }
            level["floor_patches"][0].update(fields)
            return level

        # The documented range, including both ends, validates.
        for shine in (0.0, 0.05, 0.5, 1.0):
            errors, _ = self._validate_level_document(level_with(shine=shine))
            self.assertEqual(errors, [], f"shine {shine}: {errors}")

        # The shipped demo's override is part of the validated set.
        errors, _ = validate.validate_levels(catalog())
        self.assertEqual(errors, [], errors)

        for value in (-0.1, 1.5, "bright"):
            errors, _ = self._validate_level_document(level_with(shine=value))
            self.assertTrue(
                any("shine must be a number between 0 and 1" in e for e in errors),
                f"{value!r}: {errors}",
            )

    def test_a_broken_catalog_surfaces_in_the_validators_exit_code(self):
        with tempfile.TemporaryDirectory() as directory:
            base = catalog()
            broken = dict(base)
            broken["assets"] = base["assets"] + [base["assets"][0]]
            catalog_path = Path(directory) / "catalog.json"
            catalog_path.write_text(json.dumps(broken), encoding="utf-8")
            self.assertEqual(validate.main(["--catalog", str(catalog_path), "--quiet"]), 1)


class EnvironmentTextureTests(unittest.TestCase):
    """Environment surfaces ship as file-backed PNG texture assets."""

    CORE_MATERIAL_IDS = (
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
            sky = texture.get("surface") == "sky"
            max_edge = MAX_SKY_TEXTURE_DIMENSION if sky else MAX_TEXTURE_DIMENSION
            self.assertLessEqual(width, max_edge, texture["id"])
            self.assertLessEqual(height, max_edge, texture["id"])
            if sky:
                self.assertEqual(width, 2 * height, texture["id"])
                self.assertEqual(width & (width - 1), 0, texture["id"])
                self.assertEqual(height & (height - 1), 0, texture["id"])

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
            and entry.get("surface") != "sky"
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

    def test_every_sky_panorama_joins_on_longitude(self):
        """Sky sampling wraps U and clamps V; horizon and zenith differ."""
        sys.path.insert(0, str(PACKAGE / "tools" / "textures"))
        from seam_repair import file_verdict, measure, read_png

        skies = [entry for entry in catalog_entries("texture")
                 if entry.get("surface") == "sky"]
        self.assertTrue(skies, "the catalog declares no sky panoramas")
        for texture in skies:
            image = read_png(str(PACKAGE / "assets" / texture["model"]))
            passes, reasons = file_verdict(
                measure(image.pixels, image.width, image.height, image.channels),
                axes=("lr",),
            )
            self.assertTrue(passes, f"{texture['id']}: {reasons}")

    def test_the_six_core_material_ids_still_exist(self):
        by_id = {entry["id"]: entry for entry in catalog_entries()}
        for material_id in self.CORE_MATERIAL_IDS:
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
            # Fixture faces are fitted (their UVs never leave the sheet) and
            # mipmapped, so both dimensions must be powers of two.
            for dimension in (width, height):
                self.assertEqual(dimension & (dimension - 1), 0, f"{fixture['id']}: {dimension}")


class HomeContentTests(unittest.TestCase):
    """The Home theme is shipped content, and its materials stay clean."""

    HOME_MATERIALS = (
        "home:wallpaper_offwhite_01",
        "home:wallpaper_pattern_01",
        "home:wall_paint_offwhite_01",
        "home:hardwood_oak_01",
        "home:hardwood_walnut_02",
        "home:carpet_cream_01",
        "home:tile_home_01",
        "home:ceiling_white_01",
        "home:ceiling_plaster_01",
        "home:baseboard_wood_01",
        "home:baseboard_white_01",
        "home:handrail_wood_01",
        "home:threshold_wood_01",
    )
    HOME_FIXTURE = "home:ceiling_light_round"
    HOME_PROPS = ("home:cabinet_base", "home:cabinet_wall")

    def test_home_content_is_classified_and_organized(self):
        by_id = {entry["id"]: entry for entry in catalog_entries()}
        for material_id in self.HOME_MATERIALS:
            material = by_id.get(material_id)
            self.assertIsNotNone(material, material_id)
            self.assertEqual(material["theme"], "home", material_id)
            self.assertEqual(material["asset_type"], "material", material_id)
            self.assertEqual(material["source"], "definition", material_id)
            texture = by_id[material["texture"]]
            self.assertTrue(texture["model"].startswith("environments/home/"), material_id)
            self.assertTrue((PACKAGE / "assets" / texture["model"]).is_file(), material_id)
        for prop_id in self.HOME_PROPS:
            prop = by_id[prop_id]
            self.assertEqual(prop["theme"], "home", prop_id)
            self.assertTrue(prop["model"].startswith("environments/home/"), prop_id)
            self.assertTrue((PACKAGE / "assets" / prop["model"]).is_file(), prop_id)
        fixture = by_id[self.HOME_FIXTURE]
        self.assertEqual(fixture["theme"], "home", self.HOME_FIXTURE)
        self.assertEqual(fixture["asset_type"], "light", self.HOME_FIXTURE)
        self.assertTrue(fixture["model"].startswith("environments/home/"), self.HOME_FIXTURE)
        self.assertTrue((PACKAGE / "assets" / fixture["model"]).is_file(), self.HOME_FIXTURE)

    def test_the_home_surfaces_are_clean_and_hardwood_has_two_tones(self):
        """The canonical Home set carries no damage and offers two woods.

        "Clean" is enforced at the palette level: no Home surface sheet may
        ship a stain, a water tide or mould as its default appearance. A stain
        is a dark *region*, so the check finds connected runs of pixels that
        fall far below the sheet's own mean and caps the largest one; the
        upgraded production sheets carry real paint/plaster grain (scattered
        1-4 px speckle) that a brightest/darkest margin would misread as a
        stain. Dirty variants stay in their own materials, as the office set
        already does.
        """
        by_id = {entry["id"]: entry for entry in catalog_entries()}

        def decoded(model: str):
            return decode_png(PACKAGE / "assets" / model)

        # A visible stain is tens of pixels across; the shipped speckle is a
        # handful. 64 px (~8x8) leaves the grain several times' margin while
        # still failing any patch the eye would read as damage.
        MAX_DARK_COMPONENT = 64

        def largest_dark_region(width, height, pixels, channels, thresholds):
            """Size of the largest 4-connected region below the thresholds."""
            dark = bytearray(width * height)
            count = 0
            for position in range(width * height):
                index = position * channels
                if any(pixels[index + c] < thresholds[c] for c in range(3)):
                    dark[position] = 1
                    count += 1
            seen = bytearray(width * height)
            largest = 0
            for start in range(width * height):
                if not dark[start] or seen[start]:
                    continue
                stack = [start]
                seen[start] = 1
                size = 0
                while stack:
                    point = stack.pop()
                    size += 1
                    x = point % width
                    y = point // width
                    for neighbour in (
                        point - 1 if x > 0 else None,
                        point + 1 if x + 1 < width else None,
                        point - width if y > 0 else None,
                        point + width if y + 1 < height else None,
                    ):
                        if neighbour is not None and dark[neighbour] and not seen[neighbour]:
                            seen[neighbour] = 1
                            stack.append(neighbour)
                largest = max(largest, size)
            return largest, count

        for material_id, minimum_mean in (
            ("home:wallpaper_offwhite_01", 150),
            ("home:wallpaper_pattern_01", 150),
            ("home:wall_paint_offwhite_01", 150),
            ("home:hardwood_oak_01", 60),
            ("home:hardwood_walnut_02", 40),
            ("home:carpet_cream_01", 120),
            ("home:tile_home_01", 150),
            ("home:ceiling_white_01", 150),
            ("home:ceiling_plaster_01", 150),
        ):
            texture = by_id[by_id[material_id]["texture"]]
            width, height, pixels, channels = decoded(texture["model"])
            totals = [0, 0, 0]
            count = width * height
            for index in range(0, len(pixels), channels):
                totals[0] += pixels[index]
                totals[1] += pixels[index + 1]
                totals[2] += pixels[index + 2]
            means = [total / count for total in totals]
            for channel in range(3):
                self.assertGreater(
                    means[channel],
                    minimum_mean,
                    f"{material_id} is darker than its clean base tone",
                )
            thresholds = [mean - 60 for mean in means]
            largest, dark = largest_dark_region(width, height, pixels, channels, thresholds)
            self.assertLessEqual(
                largest,
                MAX_DARK_COMPONENT,
                f"{material_id} has a {largest}-pixel dark region (a stain, not grain)",
            )
            self.assertLess(
                dark,
                count // 100,
                f"{material_id} has {dark} pixels far below its base tone",
            )

        # Two genuinely different hardwoods: tone and plank scale differ.
        oak = by_id["home:hardwood_oak_01"]
        walnut = by_id["home:hardwood_walnut_02"]
        self.assertNotEqual(oak["tile_metres"], walnut["tile_metres"])
        oak_mean = pixel_mean(decoded(by_id[oak["texture"]]["model"]))
        walnut_mean = pixel_mean(decoded(by_id[walnut["texture"]]["model"]))
        self.assertGreater(
            oak_mean - walnut_mean,
            20,
            "the two hardwoods must read as different floors, not a tint",
        )

    def test_home_collection_uses_domestic_assets_and_resettable_portable_objects(self):
        # One source parse protects the redesigned room set and real portable
        # profiles. Generic architecture samplers no longer define Home's layout.
        path = environment_source('home')
        level = load_level(path)
        self.assertGreaterEqual(len(rooms_of(level)), 5)
        models = {prop['model'] for prop in level['props']}
        self.assertTrue({'home:sofa', 'home:armchair', 'home:coffee_table',
                         'home:dining_table', 'home:sink', 'home:kettle',
                         'home:cabinet_base', 'home:cabinet_wall'} <= models)
        requested = next(prop for prop in level['props'] if prop['id'] == 'home_requested_cushion')
        self.assertEqual(requested['model'], 'home:cushion')
        self.assertTrue(requested['physics']['pickup'])
        self.assertEqual(requested['physics']['reset_group'], 'home_objects')
        self.assertTrue(any(prop['id'] == 'home_restore' and
                            any(component['component'] == 'interactable'
                                for component in prop.get('components', []))
                            for prop in level['props']))


def _png_chunk(tag: bytes, payload: bytes) -> bytes:
    """One 8-bit PNG chunk with its length and CRC, built in memory."""
    return (
        struct.pack(">I", len(payload))
        + tag
        + payload
        + struct.pack(">I", zlib.crc32(tag + payload) & 0xFFFFFFFF)
    )


def _filter_scanline(filter_type: int, raw: bytes, previous: bytes) -> bytes:
    """Applies one PNG scanline filter, as an encoder would store it."""
    bpp = 4  # 8-bit RGBA
    filtered = bytearray(raw)
    for index in range(len(raw)):
        left = raw[index - bpp] if index >= bpp else 0
        up = previous[index]
        up_left = previous[index - bpp] if index >= bpp else 0
        if filter_type == 0:
            predictor = 0
        elif filter_type == 1:
            predictor = left
        elif filter_type == 2:
            predictor = up
        elif filter_type == 3:
            predictor = (left + up) // 2
        elif filter_type == 4:
            estimate = left + up - up_left
            pa = abs(estimate - left)
            pb = abs(estimate - up)
            pc = abs(estimate - up_left)
            predictor = left if (pa <= pb and pa <= pc) else (up if pb <= pc else up_left)
        else:
            raise ValueError(f"unknown PNG filter {filter_type}")
        filtered[index] = (filtered[index] - predictor) & 0xFF
    return bytes(filtered)


def synthetic_png(width: int, height: int, rows: list[bytes], filters: list[int]) -> bytes:
    """Builds an in-memory 8-bit RGBA PNG whose rows use the given filters.

    The controls exist only as bytes: nothing is written to the repository.
    """
    if len(rows) != height or len(filters) != height:
        raise ValueError("one filter per row is required")
    if any(len(row) != width * 4 for row in rows):
        raise ValueError("every row must contain width RGBA pixels")
    stream = bytearray()
    previous = bytes(width * 4)
    for row, filter_type in zip(rows, filters):
        stream.append(filter_type)
        stream += _filter_scanline(filter_type, row, previous)
        previous = row
    header = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    return (
        b"\x89PNG\r\n\x1a\n"
        + _png_chunk(b"IHDR", header)
        + _png_chunk(b"IDAT", zlib.compress(bytes(stream), 9))
        + _png_chunk(b"IEND", b"")
    )


def decompressed_scanlines(data: bytes) -> bytes:
    """The zlib-decompressed IDAT stream: filter bytes followed by deltas."""
    offset = 8
    idat = bytearray()
    while offset + 8 <= len(data):
        length = struct.unpack(">I", data[offset : offset + 4])[0]
        tag = data[offset + 4 : offset + 8]
        if tag == b"IDAT":
            idat += data[offset + 8 : offset + 8 + length]
        offset += 12 + length
    return zlib.decompress(bytes(idat))


def decoded_has_empty_alpha(data: bytes) -> bool:
    """True when the unfiltered image has at least one alpha == 0 pixel."""
    _width, _height, rgba = tex.decode_png(data)
    return 0 in rgba[3::4]


class PngTransparencyGateTests(unittest.TestCase):
    """The cut-out check must unfilter scanlines, not trust raw IDAT deltas.

    ``zlib.decompress`` yields filter bytes and per-filter deltas, not RGBA
    pixels; the historical raw-index predicate read transparency out of those
    deltas. The opaque Sub-filtered control below is the documented negative
    control for that defect (AUD-008), and every control is bytes in memory.
    """

    @staticmethod
    def raw_indexed_has_transparency(scanlines: bytes, width: int, height: int) -> bool:
        """The historical defect: treat filtered scanline bytes as pixels."""
        stride = width * 4
        return any(
            scanlines[row * (stride + 1) + 1 + column * 4 + 3] == 0
            for row in range(height)
            for column in range(width)
        )

    def test_an_opaque_sub_filtered_row_defeats_the_raw_index_predicate(self):
        # Pixel (100,100,100,255) twice under Sub filtering: the first pixel is
        # stored absolutely, the second as zero deltas, so the raw alpha bytes
        # are (255, 0) although both decoded alphas are 255.
        opaque = bytes([100, 100, 100, 255, 100, 100, 100, 255])
        png = synthetic_png(2, 1, [opaque], [1])
        scanlines = decompressed_scanlines(png)
        self.assertEqual(scanlines, bytes([1, 100, 100, 100, 255, 0, 0, 0, 0]))
        self.assertTrue(self.raw_indexed_has_transparency(scanlines, 2, 1))
        self.assertFalse(decoded_has_empty_alpha(png))

    def test_a_zero_alpha_pixel_reads_as_transparency_after_decoding(self):
        transparent = bytes([10, 20, 30, 0, 40, 50, 60, 255])
        png = synthetic_png(2, 1, [transparent], [1])
        self.assertTrue(decoded_has_empty_alpha(png))

    def test_the_decoder_unfilters_every_png_filter_type(self):
        # One row per PNG filter type 0..4; the later rows make the decoder
        # reconstruct real Sub/Up/Average/Paeth deltas through its unfiltering
        # path rather than a stored copy of the pixels.
        rows = [
            bytes([1, 2, 3, 4, 5, 6, 7, 8]),
            bytes([9, 10, 11, 12, 13, 14, 15, 16]),
            bytes([17, 18, 19, 20, 21, 22, 23, 24]),
            bytes([25, 26, 27, 28, 29, 30, 31, 32]),
            bytes([33, 34, 35, 36, 37, 38, 39, 40]),
        ]
        png = synthetic_png(2, 5, rows, [0, 1, 2, 3, 4])
        width, height, rgba = tex.decode_png(png)
        self.assertEqual((width, height), (2, 5))
        self.assertEqual(rgba, b"".join(rows))


class PoolContentTests(unittest.TestCase):
    """The Pool theme is shipped content, not a reserved category."""

    POOL_MATERIALS = (
        "core:pool_tile_deck_01",
        "core:pool_tile_basin_01",
        "core:pool_tile_wall_01",
        "core:pool_ceiling_01",
        "pool:coping_01",
        "pool:band_01",
        "pool:metal_01",
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
        "pool:pool_bench",
        "pool:pool_drain",
        "pool:pool_service_door",
    )
    POOL_FIXTURES = ("core:pool_light_round", "core:pool_light_wall")

    def test_pool_content_is_classified_and_organized(self):
        by_id = {entry["id"]: entry for entry in catalog_entries()}
        for material_id in self.POOL_MATERIALS:
            material = by_id[material_id]
            self.assertEqual(material["theme"], "pool", material_id)
            self.assertTrue(by_id[material["texture"]]["model"].startswith("environments/pool/"), material_id)
        for prop_id in self.POOL_PROPS:
            prop = by_id[prop_id]
            self.assertEqual(prop["theme"], "pool", prop_id)
            self.assertTrue(prop["model"].startswith("environments/pool/"), prop_id)
            self.assertTrue((PACKAGE / "assets" / prop["model"]).is_file(), prop_id)
        for fixture_id in self.POOL_FIXTURES:
            fixture = by_id[fixture_id]
            self.assertEqual(fixture["theme"], "pool", fixture_id)
            self.assertEqual(fixture["asset_type"], "light", fixture_id)
            self.assertEqual(fixture["source"], "file", fixture_id)
            self.assertTrue(fixture["model"].startswith("environments/pool/"), fixture_id)
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
        # The current intentional contract: the sign sheet is a square
        # power-of-two sheet authored at the hard 1024x1024 budget, not a
        # 128x128 placeholder. The numbers come from the mirrored policy constants
        # above (see `src/engine/assets.rs`).
        self.assertEqual(width, height, "the sign sheet must be square")
        self.assertTrue(
            width > 0 and (width & (width - 1)) == 0,
            f"the sign sheet must be power-of-two, found {width}",
        )
        self.assertGreaterEqual(
            width,
            HIGH_RESOLUTION_TEXTURE_MINIMUM,
            "the sign must be high-resolution artwork",
        )
        self.assertGreater(
            width,
            PREFERRED_TEXTURE_DIMENSION,
            "the sign intentionally exceeds the soft preferred budget",
        )
        self.assertLessEqual(
            width,
            MAX_TEXTURE_DIMENSION,
            "the sign must stay within the hard 1024 limit",
        )
        self.assertEqual(
            (width, height),
            (MAX_TEXTURE_DIMENSION, MAX_TEXTURE_DIMENSION),
            "the sign is the 1024x1024 sheet",
        )
        # The sheet must be a cut-out: some pixel is fully transparent, so the
        # decal pass has a silhouette to discard instead of a floating plate.
        # Decode through the unfiltering reader: raw IDAT bytes are filter
        # bytes and deltas, so indexing them directly accepted an opaque sheet.
        self.assertTrue(
            decoded_has_empty_alpha(data),
            "the sign sheet has no transparent pixels",
        )

    def test_the_pool_showcase_is_real_lowered_floor_geometry(self):
        level = load_level(environment_source('pool'))
        regions = level.get("floor_regions", [])
        self.assertTrue(regions, "the pool basin must be a floor region, not a prop")
        offsets = [float(region.get("offset_y", 0.0)) for region in regions]
        self.assertTrue(any(offset <= -1.0 for offset in offsets), "no recessed basin")
        self.assertTrue(any(-0.4 < offset < 0 for offset in offsets), "no walkable entry step")
        # Every region is tiled, not left on the Office default.
        supports = sorted((region for region in regions
                           if region.get("edge_material") == "core:pool_tile_basin_01"),
                          key=lambda region: region["z"])
        for region in regions:
            self.assertEqual(region.get("material"), "core:pool_tile_basin_01", region)
            if region not in supports:
                self.assertEqual(region.get("edge_material"), "core:pool_tile_wall_01", region)
        # One existing JSON parse protects the new model/support Y interface:
        # six small collision surfaces must stay concealed below real treads.
        # No model build, full-map preparation or native process is needed.
        self.assertEqual(len(supports), 6)
        steps = sorted((prop for prop in level["props"]
                        if prop["model"] == "pool:pool_entry_steps"), key=lambda prop: prop["z"])
        self.assertEqual(len(steps), 2)
        for index, support in enumerate(supports):
            with self.subTest(tread=index):
                self.assertAlmostEqual(support["x"], 9.105)
                self.assertAlmostEqual(support["z"], 5.5 + index / 3, places=6)
                self.assertAlmostEqual(support["width"], 1.19)
                self.assertAlmostEqual(support["depth"], 1 / 3, places=6)
                self.assertAlmostEqual(support["offset_y"], -1.305 + index * .2)
                module_index = index // 3
                module = steps[module_index]
                self.assertAlmostEqual(module["x"], 9.7)
                self.assertAlmostEqual(module["z"], 6 + module_index)
                self.assertEqual(module["rotation_degrees"], 180)
                self.assertFalse(module["solid"], "one bounds box would seal the step approach")
                base_y = supports[module_index * 3 + 1]["offset_y"] + module["y"]
                self.assertAlmostEqual(base_y, -1.5 + module_index * .6)
                self.assertAlmostEqual(support["offset_y"], base_y + (index % 3 + 1) * .2 - .005)
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

    def test_pool_collection_keeps_pool_and_janitor_assets_with_real_room_doors(self):
        level = load_level(environment_source('pool'))
        placed = {prop["model"] for prop in level.get("props", [])}
        # The old decorative service-door prop is replaced by a usable door;
        # retain the basin kit and the separate closet's actual supply models.
        for prop_id in set(self.POOL_PROPS) - {"pool:pool_service_door"}:
            self.assertIn(prop_id, placed, prop_id)
        self.assertTrue({'janitor:utility_sink', 'janitor:metal_storage_rack',
                         'janitor:janitor_cart', 'janitor:wet_floor_sign'} <= placed)
        self.assertGreaterEqual(len(level['water']), 2)
        closet = next(door for door in level['doors'] if door['id'] == 'pool_closet_door')
        self.assertTrue(any(component['component'] == 'interactable'
                            for component in closet['components']))

    def test_every_pool_solid_prop_authors_its_collision_box(self):
        level = load_level(environment_source('pool'))
        for prop in level.get("props", []):
            if prop.get("solid") is True:
                self.assertIn("size", prop, f"{prop['model']} needs an explicit collision size")
                self.assertTrue(all(value > 0 for value in prop["size"]), prop)

    def test_guardrails_block_and_the_open_ladder_has_item_collision_and_a_climb_volume(self):
        level = load_level(environment_source('pool'))
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
        for rail in level["props"]:
            if rail["model"] in ("core:pool_guardrail_straight", "core:pool_guardrail_end"):
                self.assertGreater(rail["size"][0], rail["size"][2],
                                   "rail sizes are model-local; yaw rotates their length into world Z")
        ladder = next(prop for prop in level["props"] if prop["model"] == "core:pool_ladder")
        self.assertFalse(ladder["solid"], "the player must be able to enter the open climb volume")
        self.assertEqual(ladder["physics"]["motion"], "fixed")
        self.assertEqual(ladder["physics"]["shape"]["kind"], "compound")
        self.assertTrue(any(climb["bottom_y"] == -1.5 and climb["top_y"] == 0
                            for climb in level.get("ladders", [])))


class SourceHygieneTests(unittest.TestCase):
    def test_the_package_excludes_build_output(self):
        # Cargo output and benchmark results are local development artifacts.
        ignored = (PACKAGE / ".gitignore").read_text(encoding="utf-8")
        for entry in ("/target", "tools/bench/results/"):
            self.assertIn(entry, ignored)

    def test_player_and_developer_documentation_have_their_respective_guidance(self):
        # Two small text reads retain setup coverage after its move from promotion.
        readme = (PACKAGE / "README.md").read_text(encoding="utf-8")
        for needle in ("## Controls", "docs/DEVELOPMENT.md"):
            self.assertIn(needle, readme)
        developer = (PACKAGE / "docs/DEVELOPMENT.md").read_text(encoding="utf-8")
        self.assertIn("SDL3", developer)
        self.assertIn("settings.json", developer)

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
        # The previous keyboard defaults must no longer be presented
        # as the normal controls.
        for old_row in (
            "| Walk backward | `Z` |",
            "| Strafe right | `S` |",
            "| Look up | `O` |",
            "| Look down | `.` |",
            "| Look left | `K` |",
            "| Look right | `L` |",
        ):
            self.assertNotIn(old_row, readme, f"README still lists the old row {old_row!r}")


if __name__ == "__main__":
    unittest.main()
