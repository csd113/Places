"""Office asset export contracts, checked against the actual shipped meshes."""
import json
from pathlib import Path
import sys
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools/props"))
sys.path.insert(0, str(ROOT / "tools/textures"))
import geometry
import glb
from tex import decode_png
from resize_embedded_textures import box_downsample
import office_art
import lights_art

NAMES = ("desk", "chair", "cabinet", "water_cooler", "vending_machine")
MODELS = ROOT / "assets/environment/office/props/models"


class OfficeAssetTests(unittest.TestCase):
    def test_surface_exporters_preserve_authored_pixels_and_fixture_ratio(self):
        entries = list(office_art.ART.values())
        entries.append(lights_art.ART["core:fluorescent_panel_01"])
        for entry in entries:
            with self.subTest(model=entry["model"]):
                width, height, pixels = decode_png((ROOT / "assets" / entry["model"]).read_bytes())
                canvas = entry["build"]()
                self.assertEqual((canvas.width, canvas.height), (width, height))
                self.assertEqual(bytes(canvas.pixels), pixels)

    def test_native_art_is_derived_from_masters_and_embedded_without_drift(self):
        for name in NAMES:
            with self.subTest(name=name):
                width, height, pixels = decode_png((MODELS / (name + "_master.png")).read_bytes())
                self.assertEqual((width, height), (1024, 1024))
                w, h, expected = box_downsample(width, height, bytearray(pixels), 256)
                nw, nh, native = decode_png((MODELS / (name + ".png")).read_bytes())
                self.assertEqual((nw, nh), (w, h))
                self.assertEqual(native, expected)
                mesh = glb.read_glb((MODELS / (name + ".glb")).read_bytes())
                self.assertEqual(decode_png(mesh.texture_png), (nw, nh, native))
                self.assertEqual(set(native[3::4]), {255})

    def test_shipped_components_are_closed_outward_and_within_existing_budgets(self):
        catalog = json.loads((ROOT / "assets/catalog.json").read_text())
        by_id = {asset["id"]: asset for asset in catalog["assets"]}
        for name in NAMES:
            with self.subTest(name=name):
                mesh = glb.read_glb((MODELS / (name + ".glb")).read_bytes())
                topology = geometry.inspect(mesh.positions, mesh.indices)
                for field in ("degenerate", "boundary_edges", "inconsistent_edges",
                              "flipped_triangles", "nonmanifold_edges", "contradictory_components"):
                    self.assertEqual(topology[field], 0, field)
                self.assertLessEqual(mesh.triangle_count, 800)
                low, high = mesh.bounds()
                self.assertAlmostEqual(low[1], 0, places=6)
                for axis, expected in enumerate(by_id["core:" + name]["size"]):
                    self.assertLessEqual(abs(high[axis] - low[axis] - expected), max(.02, .06 * expected))
                self.assertTrue(all(0 <= coordinate <= 1 for uv in mesh.uvs for coordinate in uv))


if __name__ == "__main__":
    unittest.main()
