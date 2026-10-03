#!/usr/bin/env python3
"""Author the deterministic lighting quality fixture and its native camera list.

Only geometry/placements are generated. All materials and models are existing
catalog assets; no textures are synthesized. Compile the JSON explicitly.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / "tests/fixtures/levels/lighting_quality.json"
VIEWS = ROOT / "tools/bench/lighting_quality_views.json"


def author() -> tuple[dict, list[dict]]:
    level = {
        "format_version": 3, "id": "lighting_quality", "name": "Lighting Quality Controls",
        "author": "Places", "spawn": {"x": 4, "z": 4, "yaw_degrees": 0},
        "defaults": {"wall": "home:wall_paint_offwhite_01",
                     "floor": "home:ceiling_white_01", "ceiling": "home:ceiling_white_01"},
        "global_illuminators": [{"id": "control_moon", "kind": "directional",
            "direction": [-0.36, -0.8, -0.48], "color": [0.85, 0.9, 1.0],
            "intensity": 0.12, "enabled": True, "cast_shadows": True,
            "bake": True, "angular_size_degrees": 0.5}],
        "rooms": [], "walls": [], "props": [], "ceiling_lights": [],
        "pillars": [], "arc_walls": [], "ramps": [], "geometry_intent": [],
    }
    views = []

    def cell(name: str, *, ceiling=None, lit=True) -> tuple[float, float]:
        index = len(views)
        x, z = (index % 4) * 11.0, (index // 4) * 9.0
        level["rooms"].append({"x": x, "z": z, "width": 8, "depth": 6, "height": 3,
                               "ceiling": ceiling or {"kind": "open"}, "comment": name})
        if ceiling is None:
            level["geometry_intent"].append({"check": "missing-wall", "x": x, "z": z,
                "width": 8, "depth": 6, "note": f"{name}: intentionally open diagnostic bay."})
        if lit:
            level["ceiling_lights"].append({"fixture": "core:pool_light_round", "x": x+2,
                "z": z+3, "brightness": 0.5, "align": "none"})
        views.append({"name": name, "level": "lighting_quality", "spawn": [x+4, z+4.8, 0],
                      "camera": [0, -3], "capture_time": 0})
        return x, z

    def wall(x, z, width=8, depth=0.15, height=3, y=0, **extra):
        level["walls"].append(dict(x=x, y=y, z=z, width=width, depth=depth,
                                   height=height, **extra))

    def prop(model, x, z, identity, *, yaw=0, scale=1, y=0):
        level["props"].append({"id": identity.replace(".", "_"), "model": model, "x": x, "z": z,
                               "rotation_degrees": yaw, "scale": scale, "y": y})

    def enclosure(x, z, thickness=0.15, opening=False):
        wall(x-thickness, z-thickness, 8+2*thickness, thickness)
        wall(x-thickness, z+6, 8+2*thickness, thickness,
             openings=[{"kind": "door", "offset": 3.5+thickness, "width": 1,
                        "height": 2.1}] if opening else [])
        wall(x-thickness, z, thickness, 6)
        wall(x+8, z, thickness, 6)

    x, z = cell("continuous_wall")
    wall(x, z)
    x, z = cell("vertical_strips")
    for strip in range(4):
        wall(x+2*strip, z, 2)
    x, z = cell("horizontal_strips")
    for strip in range(3):
        wall(x, z, height=1, y=strip)
    x, z = cell("adjacent_coplanar")
    wall(x, z, 4)
    wall(x+4, z, 4)

    x, z = cell("window_topology")
    wall(x, z, openings=[{"kind": "window", "offset": 3, "width": 2,
                         "height": 1.2, "sill": 0.9}])
    x, z = cell("window_separate_pieces")
    wall(x, z, 3)
    wall(x+5, z, 3)
    wall(x+3, z, 2, height=0.9)
    wall(x+3, z, 2, height=0.9, y=2.1)
    x, z = cell("doorway_inset")
    wall(x, z, depth=0.6, openings=[{"kind": "door", "offset": 3.4, "width": 1.2,
                                                   "height": 2.1}])
    x, z = cell("concave_convex_corners")
    wall(x, z, 4)
    wall(x+3.85, z, 0.15, 3)
    wall(x+3.85, z+2.85, 3)

    x, z = cell("thin_closed_corners", ceiling={"kind": "flat"}, lit=False)
    enclosure(x, z, 0.01)
    x, z = cell("thick_closed_corners", ceiling={"kind": "flat"}, lit=False)
    enclosure(x, z, 0.6)
    x, z = cell("connected_rooms", ceiling={"kind": "flat"})
    enclosure(x, z, opening=True)
    wall(x+4, z, 0.15, 6, openings=[{"kind": "door", "offset": 2.4,
                                                  "width": 1.2, "height": 2.1}])
    x, z = cell("stacked_rooms", ceiling={"kind": "flat"})
    enclosure(x, z)
    level["rooms"].append({"x": x, "z": z, "width": 8, "depth": 6, "height": 3,
                           "floor_y": 3.2, "ceiling": {"kind": "flat"}})
    for dx, dz, w, d in ((0, 0, 8, .15), (0, 6, 8, .15), (0, 0, .15, 6), (8, 0, .15, 6)):
        wall(x+dx, z+dz, w, d, y=3.2)

    x, z = cell("ceiling_pitched_roof", ceiling={"kind": "flat"})
    enclosure(x, z)
    for offset in (1.7, 5.1):
        prop("outdoor:house_02_roof_slope", x+offset, z+1.5, f"roof_front_{offset}", y=3.3, yaw=180)
        prop("outdoor:house_02_roof_slope", x+offset, z+4.5, f"roof_back_{offset}", y=3.3)
    x, z = cell("rotated_cubes")
    for index, angle in enumerate((15, 30, 45, 60)):
        prop("core:crate", x+1+index*2, z+2, f"cube_{angle}", yaw=angle, scale=1.8)
    x, z = cell("curved_cylindrical")
    level["pillars"].append({"x": x+2, "z": z+2, "radius": .55, "height": 2, "segments": 48})
    level["arc_walls"].append({"x": x+5.3, "z": z+2.5, "radius": 1.2, "thickness": .15,
        "height": 2, "start_degrees": 0, "sweep_degrees": 120, "segments": 24})
    prop("home:ball_light", x+4, z+2, "sphere")
    x, z = cell("scaled_proximity")
    wall(x, z)
    for index, scale in enumerate((.1, .5, 1, 3)):
        prop("core:crate", x+1+index*1.8, z+0.15+.3*scale+.001,
             f"scale_{index}", scale=scale, y=.001)

    x, z = cell("production_models")
    for index, model in enumerate(("core:chair", "core:water_cooler", "core:armchair",
                                   "outdoor:showcase_stump_seat")):
        prop(model, x+1+index*1.8, z+2, f"production_{index}", yaw=30)
    x, z = cell("isolated_curved_model")
    prop("core:rubber_duck", x+4, z+2.5, "isolated_duck", scale=6)
    x, z = cell("diagonal_thin_caster", lit=False)
    prop("outdoor:house_02_roof_slope", x+4, z+2.5, "diagonal_sheet", yaw=45)
    views[-1].update(spawn=[x+7, 6, z+5.3, -45], camera=[-45, -60])
    x, z = cell("grazing_slight_normal_difference")
    level["ramps"].extend([{"x": x, "z": z, "width": 4, "depth": 3, "rise": .002},
                            {"x": x+4, "z": z, "width": 4, "depth": 3, "rise": .01}])
    return level, views


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    level, views = author()
    for path, value in ((SOURCE, level), (VIEWS, views)):
        expected = json.dumps(value, indent=2) + "\n"
        if args.check:
            if not path.is_file() or path.read_text() != expected:
                print(f"FAIL: stale {path}")
                return 1
        else:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(expected)
    print("Lighting quality fixture and fixed cameras are current")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
