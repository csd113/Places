"""Repository-level checks for Places.

These run on the source tree without a GPU. They cover the shipped level files,
the asset catalog those levels reference, the Spooner-Man entity migration and
the crate release metadata.
"""

from __future__ import annotations

import json
import re
import struct
import sys
import tempfile
import unittest
from pathlib import Path

PACKAGE = Path(__file__).resolve().parent.parent

# The asset validator is the single source of catalog truth for tooling.
sys.path.insert(0, str(PACKAGE / "tools" / "assets"))
import validate  # noqa: E402

DECAL_SURFACES = {"floor", "ceiling", "wall_north", "wall_south", "wall_east", "wall_west"}

RESIDENTIAL_LEVELS = {
    "the_residence": "The Residence",
    "quiet_apartments": "Quiet Apartments",
    "after_the_leak": "After the Leak",
}


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
        self.assertIn('description = "First-person liminal walking game"', cargo)
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
    def test_the_three_residential_levels_ship_with_matching_ids(self):
        shipped = {path.stem: load_level(path) for path in level_files()}
        for level_id, name in RESIDENTIAL_LEVELS.items():
            self.assertIn(level_id, shipped, f"{level_id}.json is missing")
            level = shipped[level_id]
            self.assertEqual(level["id"], level_id)
            self.assertEqual(level["name"], name)
            self.assertEqual(level["format_version"], 1)

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

    def test_the_residential_levels_are_large_interiors(self):
        for level_id in RESIDENTIAL_LEVELS:
            level = load_level(PACKAGE / "assets" / "levels" / f"{level_id}.json")
            rooms = rooms_of(level)
            self.assertGreaterEqual(len(rooms), 35, f"{level_id} is too small")
            floor_area = sum(room["width"] * room["depth"] for room in rooms)
            self.assertGreater(floor_area, 700.0, f"{level_id} has too little floor")
            # Residential rooms, not chambers: every room fits in a house.
            for room in rooms:
                self.assertLessEqual(max(room["width"], room["depth"]), 12.0)

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
        for path in level_files() + custom_level_files():
            text = path.read_text(encoding="utf-8")
            self.assertNotIn(".glb", text, f"{path.name} stores a model file path")
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
        # The asset demo mixes office, generic and entity assets in one level;
        # nothing in the catalog or level format gates placement by theme.
        demo = load_level(PACKAGE / "levels" / "asset_demo.json")
        placed = {prop["model"] for prop in demo.get("props", [])}
        for expected in ("core:desk", "core:couch", "spooner-man"):
            self.assertIn(expected, placed)
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
            for path in level_files() + custom_level_files()
            if '"model": "spooner-man"' in path.read_text(encoding="utf-8")
        ]
        self.assertTrue(referencing, "no shipped level still references spooner-man")

    def _validate(self, entries, themes=None):
        base = catalog()
        return validate.validate_catalog(
            {"themes": base["themes"] if themes is None else themes, "assets": entries}
        )

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

    def test_a_broken_catalog_surfaces_in_the_validators_exit_code(self):
        with tempfile.TemporaryDirectory() as directory:
            base = catalog()
            broken = dict(base)
            broken["assets"] = base["assets"] + [base["assets"][0]]
            catalog_path = Path(directory) / "catalog.json"
            catalog_path.write_text(json.dumps(broken), encoding="utf-8")
            self.assertEqual(validate.main(["--catalog", str(catalog_path), "--quiet"]), 1)


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
