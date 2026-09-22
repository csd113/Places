"""Package-level checks for the Liminal app.

These run on the source tree before a catalog release, without a GPU and without
a PocketCHIP. They cover what a release has to get right: manifest/version
consistency, the shipped level files, the material and prop ids those levels
reference, and the declared payload path.
"""

from __future__ import annotations

import json
import re
import struct
import unittest
from pathlib import Path

PACKAGE = Path(__file__).resolve().parent.parent

CORE_MATERIALS = {
    "core:wallpaper_yellow_01",
    "core:wallpaper_stained_01",
    "core:carpet_beige_01",
    "core:carpet_damp_01",
    "core:ceiling_panel_01",
    "core:ceiling_stained_01",
    "core:fluorescent_panel_01",
    "core:decal_test_01",
    "core:decal_no_diving_01",
    "core:decal_arrow_01",
    "core:decal_stripes_01",
}

DECAL_SURFACES = {"floor", "ceiling", "wall_north", "wall_south", "wall_east", "wall_west"}

RESIDENTIAL_LEVELS = {
    "the_residence": "The Residence",
    "quiet_apartments": "Quiet Apartments",
    "after_the_leak": "After the Leak",
}

PAYLOAD = "bin/armv7-unknown-linux-gnueabihf/app"


def manifest_fields() -> dict:
    """Reads the small manifest (manifest v1) without a TOML dependency."""
    fields: dict[str, object] = {}
    section = None
    for line in (PACKAGE / "app.toml").read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        if line.startswith("[") and line.endswith("]"):
            section = line[1:-1]
            fields.setdefault(section, {})
            continue
        key, _, value = line.partition("=")
        key = key.strip()
        value = value.strip()
        if value.startswith('"') and value.endswith('"'):
            parsed: object = value[1:-1]
        elif value in ("true", "false"):
            parsed = value == "true"
        else:
            parsed = int(value)
        if section is None:
            fields[key] = parsed
        else:
            fields[section][key] = parsed
    return fields


def cargo_version() -> str:
    text = (PACKAGE / "Cargo.toml").read_text(encoding="utf-8")
    match = re.search(r'^version = "([^"]+)"', text, re.MULTILINE)
    assert match, "Cargo.toml has no package version"
    return match.group(1)


def level_files() -> list[Path]:
    return sorted((PACKAGE / "assets" / "levels").glob("*.json"))


def load_level(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def rooms_of(level: dict) -> list[dict]:
    """Every room section, merging the single-room and multi-room spellings."""
    rooms = list(level.get("rooms", []))
    if level.get("room"):
        rooms.append(level["room"])
    return rooms


class ManifestTests(unittest.TestCase):
    def setUp(self):
        self.manifest = manifest_fields()

    def test_manifest_declares_the_native_runtime_form(self):
        self.assertEqual(self.manifest["manifest_version"], 1)
        self.assertEqual(self.manifest["runtime"], "rust")
        self.assertEqual(self.manifest["id"], "io.vitrallis.liminalrust")
        self.assertEqual(self.manifest["name"], "Liminal")
        self.assertNotIn("entry", self.manifest)

    def test_version_matches_the_crate_and_the_changelog(self):
        version = self.manifest["version"]
        self.assertEqual(version, cargo_version())
        newest = re.search(
            r"^## (\S+) — (\d{4}-\d{2}-\d{2})$",
            (PACKAGE / "CHANGELOG.md").read_text(encoding="utf-8"),
            re.MULTILINE,
        )
        self.assertIsNotNone(newest, "CHANGELOG.md needs a dated release heading")
        self.assertEqual(newest.group(1), version)

    def test_binaries_declare_the_staged_arm_payload(self):
        binaries = self.manifest["binaries"]
        self.assertEqual(list(binaries), ["armv7-unknown-linux-gnueabihf"])
        self.assertEqual(binaries["armv7-unknown-linux-gnueabihf"], PAYLOAD)

    def test_permissions_are_the_closed_key_set(self):
        permissions = self.manifest["permissions"]
        self.assertEqual(set(permissions), {"network", "audio", "storage"})
        self.assertIs(permissions["network"], False)
        self.assertIs(permissions["audio"], False)

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
            unknown = {mid for mid in used if mid.startswith("core:")} - CORE_MATERIALS
            self.assertEqual(unknown, set(), f"{path.name} uses unknown core ids")

    def test_decals_use_known_sheets_and_surfaces(self):
        levels_with_decals = 0
        for path in level_files():
            level = load_level(path)
            for index, decal in enumerate(level.get("decals", [])):
                self.assertIn(
                    decal["material"],
                    CORE_MATERIALS,
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
        catalogue = json.loads(
            (PACKAGE / "assets" / "props" / "props.json").read_text(encoding="utf-8")
        )
        known = {prop["id"] for prop in catalogue["props"]}
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


class SourceHygieneTests(unittest.TestCase):
    def test_the_package_excludes_build_output(self):
        # Packaging stages the release binary with `tools/build_rust_app.py` into
        # a fresh directory, so Cargo output and benchmark results never ship.
        ignored = (PACKAGE / ".gitignore").read_text(encoding="utf-8")
        for entry in ("/target", "tools/bench/results/"):
            self.assertIn(entry, ignored)

    def test_readme_documents_controls_and_prerequisites(self):
        readme = (PACKAGE / "README.md").read_text(encoding="utf-8")
        for needle in ("## Controls", "## Runtime prerequisites", "settings.json"):
            self.assertIn(needle, readme)
        self.assertIn("SDL2", readme)


if __name__ == "__main__":
    unittest.main()
