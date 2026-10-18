"""Office asset export contracts, checked against the actual shipped meshes."""
import json
from pathlib import Path
import sys
import unittest

from tools.levels.curated_maps import environment_source

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
MODELS = ROOT / "assets/environments/office/props"


class OfficeAssetTests(unittest.TestCase):
    def test_fluorescent_backplates_clear_their_local_ceiling(self):
        # One JSON parse protects the real ceiling/backplate z-fighting bug;
        # no mesh build, lighting bake or native process is needed.
        level = json.loads((environment_source('office')).read_text())
        for light in level["ceiling_lights"]:
            if light["fixture"] != "core:fluorescent_panel_01":
                continue
            with self.subTest(light=light["id"]):
                ceilings = [room.get("floor_y", 0) + room["height"]
                    for room in level["rooms"]
                    if room["x"] <= light["x"] <= room["x"] + room["width"]
                    and room["z"] <= light["z"] <= room["z"] + room["depth"]
                    and room.get("floor_y", 0) + room["height"] >= light["y"]]
                self.assertTrue(ceilings, "fixture must have a local ceiling")
                clearance = min(ceilings) - light["y"]
                self.assertGreater(clearance, .005, "backplate cannot share the roof plane")
                self.assertLess(clearance, .02, "retain the documented flush mounting")

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
                if name == "vending_machine":
                    # R2 has two delivered sheets, replacing the old single
                    # downsampled master. Protect both fitted pixel frames.
                    mesh = glb.read_glb((MODELS / (name + ".glb")).read_bytes())
                    sources = [(MODELS / source).read_bytes() for source in
                               ("drinks_materials.png", "drinks_fascia.png")]
                    self.assertEqual(set(mesh.texture_pngs), set(sources))
                    for source, size in zip(sources, ((256, 256), (256, 512))):
                        width, height, pixels = decode_png(source)
                        self.assertEqual((width, height), size)
                        self.assertEqual(set(pixels[3::4]), {255})
                    continue
                width, height, pixels = decode_png((MODELS / (name + "_master.png")).read_bytes())
                self.assertEqual((width, height), (1024, 1024))
                w, h, expected = box_downsample(width, height, bytearray(pixels), 256)
                nw, nh, native = decode_png((MODELS / (name + ".png")).read_bytes())
                self.assertEqual((nw, nh), (w, h))
                self.assertEqual(native, expected)
                mesh = glb.read_glb((MODELS / (name + ".glb")).read_bytes())
                self.assertEqual(decode_png(mesh.texture_png), (nw, nh, native))
                self.assertEqual(set(native[3::4]), {255})

    def test_shipped_components_have_valid_geometry_and_reviewed_budgets(self):
        catalog = json.loads((ROOT / "assets/catalog.json").read_text())
        by_id = {asset["id"]: asset for asset in catalog["assets"]}
        for name in NAMES:
            with self.subTest(name=name):
                mesh = glb.read_glb((MODELS / (name + ".glb")).read_bytes())
                topology = geometry.inspect(mesh.positions, mesh.indices)
                fields = ["degenerate", "inconsistent_edges", "flipped_triangles",
                          "contradictory_components"]
                if name != "vending_machine":
                    fields.extend(("boundary_edges", "nonmanifold_edges"))
                # R2 intentionally has open vent/cavity/label surfaces. It must
                # retain valid winding and UVs, but is not a watertight solid.
                for field in fields:
                    self.assertEqual(topology[field], 0, field)
                self.assertLessEqual(mesh.triangle_count, 2869 if name == "vending_machine" else 800)
                low, high = mesh.bounds()
                self.assertAlmostEqual(low[1], 0, places=6)
                for axis, expected in enumerate(by_id["core:" + name]["size"]):
                    self.assertLessEqual(abs(high[axis] - low[axis] - expected), max(.02, .06 * expected))
                self.assertTrue(all(0 <= coordinate <= 1 for uv in mesh.uvs for coordinate in uv))

    def test_drinks_fascia_and_lcd_emit_without_lighting_the_metal_housing(self):
        # One saved mesh and source JSON protect the new LCD's power wiring;
        # the native acceptance checks actual switching and nearby illumination.
        mesh = glb.read_glb((MODELS / "vending_machine.glb").read_bytes())
        emitting = {slot for slot, material in enumerate(mesh.json["materials"])
                    if any(material.get("emissiveFactor", []))}
        self.assertEqual(emitting, {4, 6})
        self.assertEqual(mesh.material_names[4], "Office_vending_mountain_art")
        self.assertEqual(mesh.material_names[6], "DRINKS_07_LCD_EMISSION")
        self.assertTrue(all(material.get("alphaMode", "OPAQUE") == "OPAQUE"
                            for material in mesh.json["materials"]))
        level = json.loads((environment_source('office')).read_text())
        for machine in (p for p in level["props"] if p["id"] in
                        ("top_vending", "top_snack_vending")):
            self.assertEqual(machine["switch_group"], "break_room_vending")
            self.assertIn(dict(component="light", enabled=True, switchable=True,
                               emission_scale=1), machine["components"])
            self.assertEqual(len(machine["lights"]), 2)
        drinks = next(p for p in level["props"] if p["id"] == "top_vending")
        self.assertEqual(set(drinks["emission_overrides"]), {"4"})


if __name__ == "__main__":
    unittest.main()
