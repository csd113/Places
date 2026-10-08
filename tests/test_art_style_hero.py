"""Capture contract checks; native pixels are verified by the recorded campaign."""
import importlib.util
import json
from pathlib import Path
import struct
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "hero_capture", ROOT / "tools/bench/capture_art_style_hero.py")
HERO = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(HERO)


class HeroCaptureContract(unittest.TestCase):
    def test_camera_selection_rejects_typos_and_duplicates(self):
        manifest = {"views": [{"name": "room"}, {"name": "entities"}]}
        for names in ("room,typo", "room,room", ""):
            with self.assertRaises(ValueError):
                HERO.select_views(manifest, names)
        self.assertEqual([v["name"] for v in HERO.select_views(manifest, "entities,room")],
                         ["entities", "room"])

    def test_receipt_requires_requested_world_and_valid_settings(self):
        log = "[renderer] wgpu | adapter: Apple M2 Pro\n[loading] committed art_style_hero\n"
        HERO.validate_native(log, "art_style_hero", None, [1280, 720])
        for invalid in (log.replace("art_style_hero", "places_demo"),
                        log + "not a valid settings file", log.replace("[renderer]", "")):
            with self.assertRaises(ValueError):
                HERO.validate_native(invalid, "art_style_hero", None, [1280, 720])

    def test_capture_requires_drawable_dimensions_and_write_receipt(self):
        with tempfile.TemporaryDirectory() as directory:
            image = Path(directory) / "room.png"
            image.write_bytes(b"\x89PNG\r\n\x1a\n" + b"\x00\x00\x00\rIHDR" + struct.pack(">II", 1280, 720))
            log = ("[renderer] wgpu | adapter: Apple M2 Pro\n[loading] committed art_style_hero\n"
                   f"PLACES_CAPTURE: wrote {image}\n")
            HERO.validate_native(log, "art_style_hero", image, [1280, 720])
            for invalid_log, dimensions in ((log, [640, 360]), (log.split("PLACES_CAPTURE")[0], [1280, 720])):
                with self.assertRaises(ValueError):
                    HERO.validate_native(invalid_log, "art_style_hero", image, dimensions)

    def test_effective_settings_must_match_requested_quality(self):
        log = ("[renderer] wgpu | adapter: Apple M2 Pro\n[loading] committed art_style_hero\n"
               "[settings] quality high (saved high) | lightmaps full | reflections full | filtering high | window Windowed\n")
        settings = dict(quality="high", lightmaps="full", reflections="full", texture_filtering="high")
        HERO.validate_native(log, "art_style_hero", None, [1280, 720], settings)
        with self.assertRaises(ValueError):
            HERO.validate_native(log, "art_style_hero", None, [1280, 720], dict(
                quality="low", lightmaps="off", reflections="off", texture_filtering="low"))

    def test_comparison_uses_identical_existing_model_without_special_renderer_paths(self):
        source = json.loads((ROOT / "tests/fixtures/levels/art_style_hero.json").read_text())
        static = next(prop for prop in source["props"] if prop["id"] == "comparison_static_chair")
        template = next(prop for prop in source["spawn_templates"] if prop["id"] == "comparison_chair")
        self.assertEqual(static["model"], template["model"])
        self.assertEqual(static.get("scale", 1), template["scale"])
        self.assertFalse(source.get("weather"))
        self.assertFalse(source.get("animated_emissions"))


if __name__ == "__main__":
    unittest.main()
