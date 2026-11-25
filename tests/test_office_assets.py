"""Office asset export contracts, checked against the actual shipped meshes."""
import json
from pathlib import Path
import unittest

from release.curated_maps import environment_source

ROOT = Path(__file__).resolve().parents[1]
from assets.inspection import glb

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
