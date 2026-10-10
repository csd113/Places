"""Beach authoring support and runtime-envelope regression checks."""
import copy
import json
import math
from pathlib import Path
import sys
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path[:0] = [str(ROOT / "tools/entities"), str(ROOT / "tools/props")]
from validate_entities import Model, _slerp, verify_report
from parts.beach_structures import placed_components
from parts.beach_nature import placed_components as nature_components
from tools.levels.beach_components import shore_segment


class BeachTests(unittest.TestCase):
    def test_rotation_sweep_matches_spherical_runtime_interpolation(self):
        # At one quarter of a 120-degree turn, nlerp produces 27.8 degrees;
        # the renderer's SLERP produces 30. Bounds must sample the same pose.
        rotation = _slerp([0, 0, 0, 1], [0, math.sin(math.pi/3), 0, .5], .25)
        self.assertAlmostEqual(rotation[1], math.sin(math.pi/12), places=7)
        self.assertAlmostEqual(rotation[3], math.cos(math.pi/12), places=7)
        self.assertEqual(_slerp([0, 0, 0, 1], [0, 0, 0, -1], .5), [0, 0, 0, 1])

    def test_step_channel_holds_until_the_next_key(self):
        sample = Model.__new__(Model)._interpolate
        self.assertEqual(sample([0, 1], [[0], [4]], .999, False, interpolation="STEP"), [0])
        self.assertEqual(sample([0, 1], [[0], [4]], 1, False, interpolation="STEP"), [4])

    def test_horizontal_escape_fails_the_maintained_entity_gate(self):
        path = "assets/entities/beach_fish/model/beach_fish.glb"
        asset = dict(path=path, vertices=3, triangles=1, joints=1,
                     weight_problems=[], bind_min_m=[-1, 0, -1], bind_max_m=[1, 1, 1])
        clip = dict(asset=path, name="swim", duration=1, kind="swim", min_y_m=0,
                    max_edge_stretch=1, contact_frames=0,
                    posed_min_m=[-1, 0, -1], posed_max_m=[1, 1, 1])
        report = dict(assets=[asset], clips=[clip])
        self.assertEqual(verify_report(report), [])
        escaped = copy.deepcopy(report)
        escaped["clips"][0]["posed_max_m"][0] = 1.21
        self.assertTrue(any("axis 0" in failure for failure in verify_report(escaped)))

    def test_shore_support_reaches_the_waterline_on_the_implemented_ramp_axis(self):
        shore = shore_segment(0, 0, 14, 13)
        self.assertTrue(all(r["depth"] > r["width"] for r in shore["ramps"]))
        first, second = shore["ramps"][:2]
        self.assertAlmostEqual(first["offset_y"] + first["rise"], second["offset_y"])
        base = shore["rooms"][0]["floor_y"]
        self.assertAlmostEqual(base + second["offset_y"] + second["rise"], 0)
        self.assertEqual(base, -2.2)  # Underwater entity anchors belong to this room.
        water = shore["water"][1]
        end = water["z"] + water["depth"]
        end_floor = base + second["offset_y"] + second["rise"] * (end-second["z"])/second["depth"]
        self.assertLess(end_floor, water["surface_y"])
        self.assertGreater(0, water["surface_y"])

    def test_dock_support_and_posts_reconstruct_bases_above_an_existing_sea_floor(self):
        parts = placed_components("dock", 10, 10, base_y=-.5, floor_y=0,
                                  floor_at=lambda _x, _z: -2.2)
        region = parts["floor_regions"][0]
        def floor(prop):
            if (region["x"] <= prop["x"] <= region["x"]+region["width"] and
                    region["z"] <= prop["z"] <= region["z"]+region["depth"]):
                return region["offset_y"]
            return -2.2
        self.assertAlmostEqual(floor(parts["props"][0]) + parts["props"][0]["y"], -.5)
        for post in parts["props"][1:5]:
            self.assertAlmostEqual(floor(post) + post["y"], -.5)
        self.assertTrue(all(p.get("occludes") is False for p in parts["props"][1:]))

    def test_curved_shore_water_corners_keep_their_own_room(self):
        segments = [shore_segment(0, 0, 14, 6), shore_segment(6, 0, 10, 6)]
        rooms = [s["rooms"][0] for s in segments]
        for segment, owner in zip(segments, rooms):
            for water in segment["water"]:
                for x in (water["x"], water["x"]+water["width"]):
                    for z in (water["z"], water["z"]+water["depth"]):
                        room = next(r for r in rooms if r["x"]-.01 <= x <= r["x"]+r["width"]+.01
                                    and r["z"]-.01 <= z <= r["z"]+r["depth"]+.01)
                        self.assertIs(room, owner)

    def test_all_new_animals_have_explicit_zoo_playback_actions(self):
        zoo = json.loads((ROOT / "assets/levels/model_zoo.json").read_text())
        animals = {p["model"]: p for p in zoo["props"]
                   if p["model"] in ("beach:seagull", "beach:crab", "beach:fish")}
        self.assertEqual(len(animals), 3)
        actions = [a for t in zoo["timers"] for b in t["bindings"] for a in b["actions"]]
        for animal in animals.values():
            self.assertTrue(any(a["action"] == "play_animation" and a["target"] == animal["id"]
                                for a in actions))

    def test_zoo_animal_lighting_anchors_belong_to_real_rooms(self):
        zoo = json.loads((ROOT / "assets/levels/model_zoo.json").read_text())
        for animal in zoo["props"]:
            if animal["model"] not in ("beach:seagull", "beach:crab", "beach:fish"):
                continue
            with self.subTest(model=animal["model"]):
                room = next(r for r in zoo["rooms"]
                            if r["x"] <= animal["x"] <= r["x"]+r["width"]
                            and r["z"] <= animal["z"] <= r["z"]+r["depth"])
                base = room.get("floor_y", 0)
                support = base
                for region in zoo["floor_regions"]:
                    if (region["x"] <= animal["x"] <= region["x"]+region["width"]
                            and region["z"] <= animal["z"] <= region["z"]+region["depth"]):
                        support = base + region["offset_y"]
                centre = support + animal.get("y", 0) + animal["size"][1]*animal.get("scale", 1)*.5
                self.assertGreaterEqual(centre, base)
                self.assertLessEqual(centre, base+room["height"])

    def test_zoo_fish_water_corners_belong_to_the_lowered_room(self):
        zoo = json.loads((ROOT / "assets/levels/model_zoo.json").read_text())
        water = next(w for w in zoo["water"] if w.get("material") == "beach:water_shallow_01")
        for x in (water["x"], water["x"]+water["width"]):
            for z in (water["z"], water["z"]+water["depth"]):
                room = next(r for r in zoo["rooms"] if r["x"]-.01 <= x <= r["x"]+r["width"]+.01
                            and r["z"]-.01 <= z <= r["z"]+r["depth"]+.01)
                self.assertGreater(water["surface_y"], room.get("floor_y", 0))

    def test_town_portals_allow_a_standing_body_at_every_quarter_turn(self):
        for name in ("town_arch", "town_house_cream", "town_house_blue", "town_house_coral"):
            for turn in range(4):
                with self.subTest(name=name, turn=turn):
                    parts = placed_components(name, 10, 10, rotation_degrees=turn*90)
                    self.assertEqual(parts["archways"], [])
                    def point(px, pz):
                        return ((10+px, 10+pz), (10+pz, 10-px),
                                (10-px, 10-pz), (10-pz, 10+px))[turn]
                    def floor(x, z):
                        for region in parts["floor_regions"]:
                            if (region["x"] <= x <= region["x"]+region["width"] and
                                    region["z"] <= z <= region["z"]+region["depth"]):
                                return region["offset_y"]
                        return 0
                    for sample in range(31):
                        x, z = point(0, sample/10)
                        feet = floor(x, z)
                        for proxy in parts["props"][1:]:
                            width, height, depth = [v*proxy["scale"] for v in proxy["size"]]
                            if turn % 2:
                                width, depth = depth, width
                            base = floor(proxy["x"], proxy["z"]) + proxy["y"]
                            if (abs(proxy["x"]-x) < width/2+.3 and
                                    abs(proxy["z"]-z) < depth/2+.3 and
                                    base+height > feet+.01):
                                self.assertGreaterEqual(base, feet+1.8)

    def test_sea_arch_blocks_rock_and_preserves_the_standing_passage(self):
        floor = lambda x, z: -2.2 + .01*x + .02*z
        for turn in range(4):
            parts = nature_components("sea_arch", 10, 10, base_y=1.25,
                                      floor_at=floor, rotation_degrees=turn*90)
            def point(px, pz):
                return ((10+px, 10+pz), (10+pz, 10-px),
                        (10-px, 10-pz), (10-pz, 10+px))[turn]
            def blocked(px, pz):
                x, z = point(px, pz)
                for proxy in parts["props"][1:]:
                    width, height, depth = [v*proxy["scale"] for v in proxy["size"]]
                    if turn % 2:
                        width, depth = depth, width
                    base = floor(proxy["x"], proxy["z"]) + proxy["y"]
                    if (abs(proxy["x"]-x) < width/2+.3 and abs(proxy["z"]-z) < depth/2+.3
                            and base < 1.25+1.8 and base+height > 1.25+.01):
                        return True
                return False
            self.assertTrue(blocked(-4.5, 0))
            self.assertTrue(blocked(4.5, 0))
            for sample in range(31):
                self.assertFalse(blocked(0, sample*.2-3))


if __name__ == "__main__":
    unittest.main()
