"""Asset behavior classes and playable showcase wiring, without a GPU."""
import json
from pathlib import Path
import struct
import unittest

ROOT = Path(__file__).resolve().parents[1]
CARPETS = ("cosmic_bowl", "neon_confetti", "starburst", "retro_swirl",
           "geometric", "arcade_grid")


class RetroArcadeContracts(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.assets = {entry["id"]: entry for entry in json.loads(
            (ROOT / "assets/catalog.json").read_text())["assets"]}
        cls.level = json.loads((ROOT / "levels/retro_arcade_demo.json").read_text())

    def document(self, name):
        path = ROOT / "assets" / self.assets["arcade:" + name]["model"]
        data = path.read_bytes()
        length = struct.unpack_from("<I", data, 12)[0]
        return json.loads(data[20:20 + length])

    def test_six_carpet_designs_react_without_emission_or_attached_lights(self):
        displayed = {self.level["defaults"]["floor"]}
        displayed.update(patch["material"] for patch in self.level["floor_patches"])
        for family in CARPETS:
            identity = f"arcade:carpet_{family}_01"
            material = self.assets[identity]
            self.assertIn(identity, displayed)
            self.assertGreater(material["uv_response"], 0)
            self.assertEqual(material["tile_metres"], 4)
            self.assertNotIn("emissive", material)
            self.assertNotIn("emissive_mask", material)
            dimensions = []
            for field in ("texture", "uv_response_mask"):
                asset = self.assets[material[field]]
                self.assertEqual(asset["asset_type"], "texture")
                data = (ROOT / "assets" / asset["model"]).read_bytes()
                dimensions.append(struct.unpack_from(">II", data, 16))
            self.assertEqual(dimensions, [(256, 256), (256, 256)])

    def test_ordinary_models_and_fluorescent_side_print_keep_distinct_roles(self):
        ordinary = ("bowling_pin", "bowling_ball_purple", "bowling_ball_marbled",
                    "fixed_chair_red", "fixed_chair_blue", "pedestal_table",
                    "ball_rack", "ball_return", "cat_plush", "change_machine")
        for name in ordinary:
            for material in self.document(name)["materials"]:
                self.assertFalse(any(material.get("emissiveFactor", [0, 0, 0])), name)
                self.assertEqual(material.get("extras", {}).get("uv_response", 0), 0, name)
        cabinet = self.document("cabinet_scifi")
        reactive = [m for m in cabinet["materials"] if m.get("extras", {}).get("uv_response", 0)]
        self.assertEqual(len(reactive), 1)
        self.assertFalse(any(reactive[0].get("emissiveFactor", [0, 0, 0])))
        self.assertEqual(reactive[0]["extras"]["uv_response_mask"]["texCoord"], 0)
        self.assertTrue(any(any(m.get("emissiveFactor", [0, 0, 0]))
                            for m in cabinet["materials"]))

    def test_reachable_switch_controls_all_four_operational_blacklights(self):
        sources = [light for light in self.level["ceiling_lights"]
                   if light.get("ultraviolet", 0) > 0]
        self.assertEqual(len(sources), 4)
        self.assertTrue(all(light["switchable"] and light["enabled"] for light in sources))
        switch = next(p for p in self.level["props"] if p["id"] == "cosmic_blacklight_switch")
        actions = switch["bindings"][0]["actions"]
        self.assertEqual({a["target"] for a in actions if a["action"] == "toggle"},
                         {light["id"] for light in sources})
        self.assertTrue(any(a["action"] == "toggle_animation" for a in actions))
        self.assertFalse(switch["solid"])
        self.assertGreater(switch["components"][0]["reach"], switch["x"])


if __name__ == "__main__":
    unittest.main()
