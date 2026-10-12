"""Asset behavior classes and playable showcase wiring, without a GPU."""
import json
import math
from pathlib import Path
import struct
import unittest

from tools.levels.birthday_compositions import PartyComposer

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
        self.assertEqual({room["material"] for room in self.level["rooms"]},
                         {f"arcade:carpet_{family}_01" for family in
                          ("cosmic_bowl", "retro_swirl", "neon_confetti")})
        for family in CARPETS:
            identity = f"arcade:carpet_{family}_01"
            material = self.assets[identity]
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

    def test_party_room_preserves_a_clear_three_room_walking_route(self):
        # This source-only witness catches blocked passages/approaches without
        # compiling or baking a production map. About 370 samples and the
        # authored collision boxes are involved; no renderer/assets are loaded.
        self.assertEqual(len(self.level["rooms"]), 3)
        # The small authored trim profile adds no colliders and must leave both
        # sides of each passage completely bare. This protects the new join
        # treatment with only source rectangles, adding no native/bake cost.
        boards = self.level["baseboards"]
        self.assertTrue(boards)
        self.assertEqual(len(boards) % 2, 0)
        self.assertEqual({b["material"] for b in boards},
                         {room["material"] for room in self.level["rooms"]})
        for board in boards:
            self.assertEqual(board["uv_projection"], "floor_fold")
            angle = math.radians(board["rotation_degrees"])
            for fraction in (.001, .5, .999):
                x = board["x"]+math.cos(angle)*board["length"]*fraction + math.sin(angle)*.020
                z = board["z"]-math.sin(angle)*board["length"]*fraction + math.cos(angle)*.020
                room = next(r for r in self.level["rooms"] if
                            r["x"] <= x <= r["x"]+r["width"] and r["z"] <= z <= r["z"]+r["depth"])
                floor_material = room["material"]
                for patch in [*self.level["floor_patches"], *self.level["floor_regions"]]:
                    if patch["x"] <= x <= patch["x"]+patch["width"] and patch["z"] <= z <= patch["z"]+patch["depth"]:
                        floor_material = patch.get("material", room["material"])
                self.assertEqual(board["material"], floor_material)
            material = self.assets[board["material"]]
            self.assertGreater(material["uv_response"], 0)
            self.assertEqual(material["tile_metres"], 4)
            self.assertNotIn("emissive", material)
        for body, cap in zip(boards[::2], boards[1::2]):
            for field in ("x", "z", "length", "rotation_degrees", "material"):
                self.assertEqual(body[field], cap[field])
            self.assertAlmostEqual(body["y"]+body["height"], cap["y"])
            self.assertGreaterEqual(body["height"], .01)
            self.assertGreaterEqual(cap["height"], .01)
            self.assertGreater(body["thickness"], cap["thickness"])
            self.assertLess(body["height"]+cap["height"], .12)
            self.assertNotIn("solid", body)
            self.assertNotIn("solid", cap)
        for wall in self.level["walls"]:
            horizontal = wall["width"] >= wall["depth"]
            across, along = ("z", "x") if horizontal else ("x", "z")
            extent = wall["depth"] if horizontal else wall["width"]
            for opening in wall.get("openings", []):
                lo = wall[along]+opening["offset"]
                hi = lo+opening["width"]
                for board in boards:
                    if not any(abs(board[across]-face) < 1e-6
                               for face in (wall[across], wall[across]+extent)):
                        continue
                    angle = math.radians(board["rotation_degrees"])
                    direction = math.cos(angle) if horizontal else -math.sin(angle)
                    end = board[along]+direction*board["length"]
                    low, high = sorted((board[along], end))
                    self.assertTrue(high <= lo+1e-6 or low >= hi-1e-6, board)
        # North/south stock butts clear of the perpendicular strip's front,
        # rather than creating overlapping corner caps sensitive to 90-degree
        # floating-point drift. Passage ends are kept at their true jambs.
        for board in boards:
            if board["rotation_degrees"] not in (90, 270):
                continue
            sign = -1 if board["rotation_degrees"] == 90 else 1
            low, high = sorted((board["z"], board["z"]+sign*board["length"]))
            self.assertGreaterEqual(low, .01999)
            self.assertLessEqual(high, 29.98001)
        party = self.level["rooms"][2]
        party_props = [p for p in self.level["props"] if p["id"].startswith("party_")]
        for prop in party_props:
            self.assertTrue(party["x"] <= prop["x"] <= party["x"]+party["width"], prop["id"])
            self.assertTrue(party["z"] <= prop["z"] <= party["z"]+party["depth"], prop["id"])
        boxes = []
        for wall in self.level["walls"]:
            horizontal = wall["width"] >= wall["depth"]
            near = wall["x"] if horizontal else wall["z"]
            end = near + (wall["width"] if horizontal else wall["depth"])
            spans = []
            for opening in wall.get("openings", []):
                self.assertEqual(opening.get("sill", 0), 0)
                self.assertGreaterEqual(opening["height"], 1.8)
                spans.append((near, (wall["x"] if horizontal else wall["z"])+opening["offset"]))
                near = spans[-1][1] + opening["width"]
            spans.append((near, end))
            for lo, hi in spans:
                boxes.append((lo, wall["z"], hi, wall["z"]+wall["depth"]) if horizontal
                             else (wall["x"], lo, wall["x"]+wall["width"], hi))
        for prop in self.level["props"]:
            if not prop["solid"] or prop["y"] >= 1.8:
                continue
            angle = math.radians(prop.get("rotation_degrees", 0))
            c, s = abs(math.cos(angle)), abs(math.sin(angle))
            width, _, depth = prop["size"]
            hx, hz = (width*c+depth*s)/2, (depth*c+width*s)/2
            boxes.append((prop["x"]-hx, prop["z"]-hz, prop["x"]+hx, prop["z"]+hz))
        routes = [((6.8, 28), (6.8, 25.2), (18.4, 25.2), (21, 25.4), (25.9, 25.4)),
                  ((21, 25.4), (21, 23.2), (25.9, 23.2), (25.9, 19.8), (26.6, 19.8)),
                  ((21, 25.4), (23.4, 26.1))]
        for route in routes:
            for start, end in zip(route, route[1:]):
                steps = math.ceil(math.dist(start, end)/.1)
                for step in range(steps+1):
                    t = step/steps
                    x, z = (start[0]+(end[0]-start[0])*t, start[1]+(end[1]-start[1])*t)
                    self.assertTrue(any(r["x"] <= x <= r["x"]+r["width"] and
                                        r["z"] <= z <= r["z"]+r["depth"]
                                        for r in self.level["rooms"]), (x, z))
                    self.assertFalse(any(lo_x-.30 < x < hi_x+.30 and lo_z-.30 < z < hi_z+.30
                                         for lo_x, lo_z, hi_x, hi_z in boxes), (x, z))

    def test_bouquets_share_fixed_anchors_and_allow_sway_without_interpenetration(self):
        # A bounded placement-envelope check protects the authored spacing,
        # independent phases and roof clearance, complementing runtime motion
        # tests. It reads 17 source entities rather than evaluating frame poses.
        bouquets = {}
        metadata = json.loads((ROOT / "assets/environments/arcade/party/asset_metadata.json").read_text())["models"]
        weight_metadata = metadata["arcade:party_balloon_weight"]
        for prop in self.level["props"]:
            component = next((c for c in prop.get("components", []) if c["component"] == "balloon"), None)
            if component is None:
                continue
            key = prop["id"].rsplit("_", 1)[0]
            bouquets.setdefault(key, []).append((prop, component))
        self.assertEqual(sorted(map(len, bouquets.values())), [1, 1, 3, 5, 7])
        # One small maximum-count fixture protects reusable layouts beyond the
        # demo's three/five/seven presets, without changing any shipped bytes.
        sizes = {identity: entry["size"] for identity, entry in self.assets.items()
                 if entry["asset_type"] in ("prop", "entity")}
        configurable = PartyComposer(sizes)
        configurable.balloon_bouquet(0, 0, "maximum_bouquet", count=16)
        maximum = [(prop, prop["components"][0]) for prop in configurable.props]
        self.assertEqual(len(maximum), 16)
        for bouquet in bouquets.values():
            self.assertEqual(len({(p["x"], p["y"], p["z"], tuple(c["anchor"]))
                                  for p, c in bouquet}), 1)
            if len(bouquet) > 1:
                self.assertGreater(len({c["phase"] for _, c in bouquet}), 1)
                prop, _ = bouquet[0]
                weight_id = prop["id"].rsplit("_", 1)[0]+"_weight"
                weight = next(p for p in self.level["props"] if p["id"] == weight_id)
                self.assertEqual(weight["model"], "arcade:party_balloon_weight")
                self.assertEqual((weight["x"], weight["z"]), (prop["x"], prop["z"]))
                self.assertAlmostEqual(prop["y"]-weight["y"], weight_metadata["anchor_height"])
                table_id = weight_id.rsplit("_bouquet", 1)[0]
                cloth = next(p for p in self.level["props"] if p["id"] == table_id+"_cloth")
                surface_y = cloth["y"]+metadata[cloth["model"]]["bounds_max"][1]
                self.assertAlmostEqual(weight["y"], surface_y, places=5)
                self.assertGreater(prop["y"], surface_y)
            for prop, component in bouquet:
                self.assertFalse(prop["solid"])
                self.assertLess(prop["y"]+component["string_length"] +
                                .43*component["scale"]+component["bob_amplitude"], 3.0)
        for bouquet in [*bouquets.values(), maximum]:
            self.assertEqual(len({(c["shared_phase"], c["shared_sway"])
                                  for _, c in bouquet}), 1)
            def envelope_radius(balloon):
                tilt = math.sin(math.radians(balloon["rotation_degrees"]))
                return .15*balloon["scale"] + math.sqrt(2)*( \
                    balloon["sway_amplitude"] + .43*balloon["scale"]*tilt)
            for index, (_, component) in enumerate(bouquet):
                for _, other in bouquet[index+1:]:
                    separation = math.dist(component["offset"], other["offset"])
                    # Both horizontal sway axes and both body-tilt axes may
                    # approach the neighbour. Shared sway cancels within a
                    # bouquet because its phase and amplitude are identical.
                    envelope = envelope_radius(component)+envelope_radius(other)
                    self.assertGreater(separation, envelope)

    def test_equal_gift_groups_keep_outward_bows_and_one_shared_candle_light(self):
        # Three tiny reusable fixtures prove cardinal bow orientation and
        # cube-scale consistency without a full-map bake. The metadata asserts
        # real authored cube geometry; source scales cannot alter that contract.
        metadata = json.loads((ROOT / "assets/environments/arcade/party/asset_metadata.json").read_text())["models"]
        sizes = {identity: entry["size"] for identity, entry in self.assets.items()
                 if entry["asset_type"] in ("prop", "entity")}
        for design in ("stars", "confetti", "stripes", "arcade", "red", "green"):
            self.assertEqual(metadata["arcade:party_gift_"+design]["box_size"], [.3, .3, .3])
        for count in (3, 5, 7):
            composer = PartyComposer(sizes)
            composer.gift_group(0, 0, "gift_fixture", count=count)
            self.assertEqual(len(composer.props), count)
            for prop in composer.props:
                self.assertEqual(prop["scale"], 1)
            for prop in composer.props[:2 if count == 3 else 4]:
                bow_x, bow_z = metadata[prop["model"]]["bow_offset_xz"]
                angle = math.radians(prop["rotation_degrees"])
                dx = math.cos(angle)*bow_x+math.sin(angle)*bow_z
                dz = -math.sin(angle)*bow_x+math.cos(angle)*bow_z
                self.assertGreater(prop["x"]*dx+prop["z"]*dz, 0, prop["id"])
        gifts = [p for p in self.level["props"] if p["model"].startswith("arcade:party_gift_")]
        self.assertEqual(len({p["model"] for p in gifts}), 6)
        self.assertTrue(all(p["scale"] == 1 for p in gifts))
        cake = next(p for p in self.level["props"] if p["model"] == "arcade:party_cake_lit")
        glows = [c for c in cake["components"] if c["component"] == "glow"]
        self.assertEqual(len(glows), 1)
        self.assertTrue(glows[0]["enabled"])
        self.assertGreater(glows[0]["intensity"], 0)
        self.assertLessEqual(glows[0]["range"], .85)
        self.assertNotIn("lights", cake)
        start = next(timer for timer in self.level["timers"] if timer["autostart"])
        self.assertFalse(start["repeat"])
        self.assertEqual(start["bindings"][0]["actions"], [
            dict(action="play_animation", target=cake["id"], clip="candle_flicker", loop=True)])


if __name__ == "__main__":
    unittest.main()
