#!/usr/bin/env python3
"""Deterministically author Lantern Hollow, a navigable nighttime showcase.

Only the sibling JSON is generated here. The ordinary Places compiler owns
light transport, collision and package output; this tool never synthesizes
textures, adds invisible containment, or changes the rendering engine.
"""
from __future__ import annotations

import argparse
import json
import math
import random
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUTPUT = ROOT / "assets/levels/lantern_hollow.json"
SEED = 20260930
HOUSE_CENTERS = (-27.0, -9.0, 9.0, 27.0)
FIRE = (-18.0, -34.0)
POND = (-32.0, -15.0)
# Keep these routes available to controller tests and native capture scripts.
TRAILS = (
    ((-36, 10.2), (-36, -4), (-26, -8), (-25, -24), FIRE),
    ((-18, 10.2), (-18, -5), (-12, -16), (-18, -27), FIRE),
    ((0, 10.2), (0, -6), (-8, -19), (-18, -27), FIRE),
    ((18, 10.2), (18, -5), (11, -13), (2, -24), (-18, -27), FIRE),
    ((36, 10.2), (36, -7), (23, -19), (2, -24), (-18, -27), FIRE),
)


def serialise(level: dict) -> str:
    return json.dumps(level, indent=2, ensure_ascii=False) + "\n"


def segment_distance(x: float, z: float, a: tuple, b: tuple) -> float:
    dx, dz = b[0] - a[0], b[1] - a[1]
    t = max(0.0, min(1.0, ((x - a[0]) * dx + (z - a[1]) * dz) / (dx * dx + dz * dz)))
    return math.hypot(x - a[0] - t * dx, z - a[1] - t * dz)


def trail_distance(x: float, z: float) -> float:
    return min(segment_distance(x, z, a, b) for route in TRAILS for a, b in zip(route, route[1:]))


def light(y: float, intensity: float, radius: float, color=(1.0, 0.84, 0.65), z: float = 0.0) -> dict:
    return {"shape": "point", "offset": [0.0, y, z], "intensity": intensity,
            "range": radius, "color": list(color), "falloff": "smooth"}


def build_level() -> dict:
    catalog = {a["id"]: a for a in json.loads((ROOT / "assets/catalog.json").read_text())["assets"]}
    level = {"format_version": 3, "id": "lantern_hollow", "name": "Lantern Hollow", "author": "Places",
             "spawn": {"x": 18.0, "z": 21.6, "yaw_degrees": -35.0},
             "defaults": {"wall": "home:wall_paint_offwhite_01", "floor": "outdoor:grass_ground_01",
                          "ceiling": "home:ceiling_white_01"},
             "sky": {"texture": "outdoor:tex_sky_stars_01", "brightness": 1.0, "ambient": 0.06}}
    for key in ("rooms", "walls", "floor_patches", "floor_regions", "props", "ceiling_lights", "doors",
                "routes", "water", "geometry_intent", "decals", "fog_regions", "void_walls"):
        level[key] = []
    # Supported cool sky radiance and authored fog keep the forest readable at night.
    level["fog_regions"].append({
        "id": "lantern_hollow_night_fog", "min": [-50.0, -3.0, -52.0],
        "max": [50.0, 12.0, 38.0], "density": 0.001,
        "color": [0.045, 0.06, 0.095], "ground_y": 8.0, "top_y": 12.0, "falloff_m": 2.0,
    })

    def prop(model, x, z, identity, *, y=0.0, yaw=0.0, scale=1.0, solid=False, size=None, **extra):
        item = {"id": identity.replace(".", "_"), "model": model, "x": round(x, 4), "z": round(z, 4)}
        if y: item["y"] = round(y, 4)
        if yaw: item["rotation_degrees"] = round(yaw, 4)
        if scale != 1: item["scale"] = round(scale, 4)
        if solid:
            item["solid"] = True
            item["size"] = list(size or catalog[model]["size"])
        item.update(extra)
        level["props"].append(item)
        return item

    def patch(x, z, w, d, material, comment=None):
        item = {"x": round(x, 4), "z": round(z, 4), "width": round(w, 4), "depth": round(d, 4),
                "material": material}
        if comment: item["comment"] = comment
        level["floor_patches"].append(item)

    def region(x, z, w, d, height, material):
        level["floor_regions"].append({"x": round(x, 4), "z": round(z, 4), "width": round(w, 4),
            "depth": round(d, 4), "offset_y": height, "material": material, "edge_material": material})

    def outdoor_room(x, z, w, d, name):
        level["rooms"].append({"x": x, "z": z, "width": w, "depth": d, "height": 8,
            "ceiling": {"kind": "open"}, "material": "outdoor:grass_ground_01", "comment": name})
        # Only these exact, deliberately open outdoor footprints waive the
        # checker perimeter heuristics. Physical boundaries are visible props.
        for check in ("missing-wall",):
            level["geometry_intent"].append({"check": check, "x": x, "z": z, "width": w, "depth": d,
                "note": f"{name}: deliberate outdoor floor, contained by visible 5 m rock faces and closed road gates."})

    # Separate floor rectangles give every point exactly one floor. Houses
    # are not laid on top of a second, coplanar world-sized grass room.
    # Small adjacent floor tiles bound the per-room material grid without
    # overlapping floors or introducing physical partitions in the forest.
    for row in range(6):
        for col in range(7):
            outdoor_room(-44 + col * 88 / 7, -46 + row * 7.8,
                         88 / 7, 7.8, f"North forest tile {row}:{col}")
    for col in range(3):
        outdoor_room(-44 + col * 88 / 3, 6.2, 88 / 3, 25.8,
                     f"Road, gardens and south forest tile {col}")
    gap_edges = (-44, -31.5, -22.5, -13.5, -4.5, 4.5, 13.5, 22.5, 31.5, 44)
    for i in range(0, len(gap_edges) - 1, 2):
        outdoor_room(gap_edges[i], .8, gap_edges[i + 1] - gap_edges[i], 5.4, f"House gap {i // 2}")

    # The asset-less checker does not flood against GLB prop colliders.
    # Its only unexplained void samples are outside this authored world. Keep
    # that waiver outside the floor, so an interior leak cannot be hidden.
    for x, z, width, depth, name in ((-46.2, -48.2, 2.2, 82.4, "West rock boundary"),
                                    (44, -48.2, 2.2, 82.4, "East rock boundary"),
                                    (-44, -48.2, 88, 2.2, "North rock boundary"),
                                    (-44, 32, 88, 2.2, "South rock boundary")):
        level["geometry_intent"].append({"check": "room-leak", "x": x, "z": z, "width": width, "depth": depth,
            "note": f"{name}: outside all authored floors and behind visible collidable rock faces; fixed-step controller audits verify containment."})

    # Real raised sidewalk and porch floor owns walkability. Matching kit
    # undersides sit 1 mm below it, avoiding duplicate exposed top surfaces.
    region(-40, 10.2, 80, 2.4, .14, "outdoor:concrete_pavement_01")
    region(-40, 20.6, 80, 2.4, .14, "outdoor:concrete_pavement_01")
    patch(-40, 12.6, 80, 8, "outdoor:showcase_asphalt", "Road asphalt is dressed by the modular road geometry below the floor.")
    for i in range(10):
        prop("outdoor:showcase_road_unmarked", -36 + i * 8, 16.6, f"road_{i}", y=-.071)
    for side, z in enumerate((11.4, 21.8)):
        for i in range(14):
            prop("outdoor:showcase_sidewalk", -39 + i * 6, z, f"sidewalk_{side}_{i}", y=-.141, yaw=90)
            prop("outdoor:showcase_curb", -39 + i * 6, 12.48 if side == 0 else 20.72,
                 f"curb_{side}_{i}", y=-.141, yaw=90)
    for i in range(20):
        patch(-38 + i * 4, 16.53, 1.5, .14, "outdoor:showcase_road_marking")
    for i, x in enumerate((-35, -19, -1, 17, 35)):
        prop("outdoor:streetlight", x, 11.05, f"streetlight_{i}", y=-.14, solid=True,
             size=[.16, 6.4, .16], lights=[light(6.05, 1.5, 14, z=.43)])

    # Reuse the five-family facade convention and the domestic/Home palette.
    # Families 02/01/05/03 supply restrained blue, cream, white and ochre.
    family_windows = {"02": [(-.5, 1.35, 1, 1)], "01": [(-.4, 1.35, .8, .94)],
                      "05": [(-.95, 1.3, .6, .6), (.35, 1.3, .6, .6)], "03": [(-.25, 1.32, .5, 1.3)]}
    # Measured committed GLBs: slope underside y=.02 at local z=+1.29
    # and y=1.67 at z=-1.30, with the shingle edge at y=1.75. The cap
    # has a flat bottom; an ordinary attic void keeps the Home ceiling and
    # fixtures below all opaque roof meshes rather than inside their slabs.
    eave = 2.7
    ceiling_rise = 1.6
    # Cap top at z=3.65 is base+.11, matching the slope shingle edge 4.45.
    ridge = 4.56
    gable_body_base = {"01": .045217, "02": .060122, "03": .060122, "05": .045217}
    for index, (cx, family) in enumerate(zip(HOUSE_CENTERS, ("02", "01", "05", "03"))):
        prefix = f"house_{index}"
        kit = f"outdoor:house_{family}"
        level["rooms"].append({"x": cx - 4.5, "z": .8, "width": 9, "depth": 5.4,
            "height": round(eave, 4), "ceiling": {"kind": "gable", "ridge": "x", "ridge_rise": ceiling_rise},
            "material": "home:hardwood_oak_01" if index % 2 == 0 else "home:hardwood_walnut_02",
            "ceiling_material": "home:ceiling_white_01", "comment": f"{prefix}: furnished cottage {family}, one sealed gable volume."})
        openings = [{"kind": "door", "offset": 3.93, "width": 1.14, "height": 2.15}]
        for dx in (-3, 3):
            for wx, wy, ww, wh in family_windows[family]:
                openings.append({"kind": "window", "offset": 4.5 + dx + wx, "width": ww, "height": wh,
                                 "sill": round(wy - wh * .5, 4), "glass": "core:glass_window_clear_01", "solid": True})
        level["walls"].extend([
            {"x": cx - 4.5, "z": 5.9, "width": 9, "depth": .3, "material": "outdoor:house_siding_01",
             "faces": {"north": "home:wall_paint_offwhite_01"}, "openings": openings},
            {"x": cx - 4.5, "z": .8, "width": 9, "depth": .3, "material": "outdoor:house_siding_01",
             "faces": {"south": "home:wall_paint_offwhite_01"}},
            {"x": cx - 4.5, "z": 1.12, "width": .3, "depth": 4.76, "material": "outdoor:house_siding_01",
             "faces": {"east": "home:wall_paint_offwhite_01"}},
            {"x": cx + 4.2, "z": 1.12, "width": .3, "depth": 4.76, "material": "outdoor:house_siding_01",
             "faces": {"west": "home:wall_paint_offwhite_01"}},
            {"x": cx + 1.25, "z": 1.12, "width": .12, "depth": 4.76,
             "material": "home:wallpaper_pattern_01", "openings": [{"kind": "passage", "offset": 2.38, "width": 1.1, "height": 2.15}]},
        ])
        for dx, kind in ((0, "doorway"),):
            prop(f"{kit}_wall_{kind}", cx + dx, 6.4 if kind == "doorway" else 6.32,
                 f"{prefix}_front_{kind}_{dx}", y=-.24 if dx == 0 else 0)
        for dx in (-3.4, 0, 3.4):
            prop(f"{kit}_roof_slope", cx + dx, 4.95, f"{prefix}_roof_front_{dx}", y=2.7)
            prop(f"{kit}_roof_slope", cx + dx, 2.05, f"{prefix}_roof_back_{dx}", y=2.7, yaw=180)
            prop(f"{kit}_roof_ridge", cx + dx, 3.5, f"{prefix}_ridge_{dx}", y=round(ridge - .22, 4))
        # The kit's bargeboards extend below its cladding triangle. Measured
        # triangle bases seat at the wall's 2.7 m eave instead of leaving an
        # 8–11 cm slot beneath the decorative gable panel.
        for sign, yaw in ((-1, 270), (1, 90)):
            prop(f"{kit}_gable", cx + sign * 4.65, 3.5, f"{prefix}_gable_{sign}",
                 y=round(eave - gable_body_base[family] * 1.8, 4), yaw=yaw, scale=1.8)
        for dx in (-4.5, 4.5):
            for z in (.8, 6.2):
                prop(f"{kit}_corner_trim", cx + dx, z, f"{prefix}_trim_{dx}_{z}")
        region(cx - 1.5, 6.21, 3, 1.5, .24, "outdoor:concrete_pavement_01")
        prop(f"{kit}_porch_deck", cx, 6.96, f"{prefix}_porch", y=-.238)
        # This is an uncovered stoop: omit freestanding tall porch posts
        # until an actual supported awning exists in the asset kit.
        patch(cx - .85, 7.71, 1.7, 2.49, "outdoor:concrete_pavement_01")
        level["doors"].append({"id": f"{prefix}_door", "x": cx - .55, "z": 6.05, "width": 1.1,
            "height": 2.15, "thickness": .045, "open_direction": "left", "swing_degrees": 90,
            "open_speed_degrees": 100, "initial_state": "open", "kind": "interior", "frame_depth": .7,
            "frame_center": .2, "components": [{"component": "interactable", "prompt": f"Cottage {index + 1} door"}],
            "bindings": [{"on": "interact", "actions": [{"action": "toggle"}]}]})
        for dx in (-1.15, 1.15):
            prop("outdoor:lamp_wall", cx + dx, 6.6, f"{prefix}_porch_lamp_{dx}", y=1.79,
                 lights=[light(.18, .6, 5, z=.12)])
        for x, z, name in ((cx - 1.65, 4.4, "living"), (cx + 2.8, 2.6, "bed")):
            level["ceiling_lights"].append({"id": f"{prefix}_{name}_light", "fixture": "home:ceiling_light_round",
                "x": x, "z": z, "brightness": 1.55, "color": [1, .9, .78], "range": 7, "emission": 1.1, "align": "none"})
        # Every central aisle is >1.1 m. Kitchen upper cabinets remain non-solid
        # so counter jumps cannot trap the player's standing body beneath them.
        items = [("core:couch", -2.5, 2.0, 0), ("core:table", -1.9, 3.65, 0),
                 ("core:bookshelf", -.35, 1.4, 0), ("core:armchair", -.85, 2.4, 90),
                 ("core:bed", 3.0, 2.4, 0), ("core:cabinet", 3.0, 5.4, 180),
                 ("core:fridge", -3.8, 5.3, 90), ("core:stove", -3.8, 4.45, 90),
                 ("core:sink", -3.8, 3.6, 90), ("home:cabinet_base", -3.8, 2.75, 90)]
        for j, (model, dx, z, yaw) in enumerate(items):
            size = [.6, .9, .55] if model == "core:sink" else None
            prop(model, cx + dx, z, f"{prefix}_furniture_{j}", yaw=yaw, solid=True, size=size)
        prop("home:cabinet_wall", cx - 4.025, 2.1, f"{prefix}_upper_cabinet", y=1.5, yaw=90)
        prop("home:crt_tv", cx + 3, 5.35, f"{prefix}_tv", y=.85, yaw=180)
        prop("home:plate", cx - 2.05, 3.6, f"{prefix}_plate", y=.75)
        prop("home:bowl", cx - 1.6, 3.75, f"{prefix}_bowl", y=.75)
        prop("home:plant_table", cx + 3.2, 5.45, f"{prefix}_plant", y=.85)
        patch(cx - 4.15, 1.25, .95, 4.35, "home:tile_home_01")
        for j, dx in enumerate((-2.1, 2.1)):
            prop("carved-pumpkin", cx + dx, 7.5, f"{prefix}_pumpkin_{j}", scale=.8,
                 components=[{"component": "animation", "clip": "laugh", "looped": True, "playing": True}],
                 lights=[light(.25, .18, 2.8)])
        # Fenced front gardens leave the path and both side passages open.
        for j in range(3):
            for sign in (-1, 1):
                prop("outdoor:porch_railing_straight", cx + sign * (1.8 + j * 1.8), 9.65,
                     f"{prefix}_garden_rail_{sign}_{j}", solid=True)

    # Shared coordinates keep adjacent trail cells continuous and bound each
    # room's floor grid. Arbitrary overlapping sample rectangles would multiply
    # both grid axes and create tens of thousands of unnecessary floor cells.
    step = .8
    for row in range(66):
        z0 = -42 + row * step
        run = None
        for col in range(101):
            x0 = -40 + col * step
            on_path = col < 100 and trail_distance(x0 + step / 2, z0 + step / 2) <= 1.3
            if on_path and run is None:
                run = col
            elif not on_path and run is not None:
                patch(-40 + run * step, z0, (col - run) * step, step, "outdoor:dirt_gravel_01")
                run = None
    for j in range(20):
        z0 = FIRE[1] - 6 + j * .6
        dz = max(abs(z0 - FIRE[1]), abs(z0 + .6 - FIRE[1]))
        width = 2 * math.sqrt(max(0, 36 - dz * dz))
        if width > 0:
            patch(FIRE[0] - width * .5, z0, width, .6, "outdoor:dirt_gravel_01")
    # A broad, uncluttered pond approach joins the western forest trail.
    patch(-29, -16.2, 4, 2.4, "outdoor:dirt_gravel_01")
    for radius, strips, y in ((4.5, 36, -.3), (3.7, 30, -.65)):
        depth = 2 * radius / strips
        for j in range(strips):
            z0 = POND[1] - radius + j * depth
            dz = max(abs(z0 - POND[1]), abs(z0 + depth - POND[1]))
            width = 2 * math.sqrt(max(0, radius * radius - dz * dz))
            if width > .001:
                region(POND[0] - width / 2, z0, width, depth, y, "outdoor:dirt_gravel_01")
    level["water"].append({"shape": "circle", "x": -36.5, "z": -19.5, "radius": 4.5,
        "surface_y": .015, "bottom_y": -.65, "material": "core:water_pool_01", "opacity": .62, "swimming": True})
    for i in range(18):
        if i in (0, 1, 3, 4): continue
        a = 2 * math.pi * i / 18
        prop("outdoor:showcase_boulder", POND[0] + 4.6 * math.cos(a), POND[1] + 4.6 * math.sin(a),
             f"pond_rock_{i}", yaw=i * 37, scale=.7 + (i % 3) * .12, solid=True, size=[1.4, .65, 1.3])
    prop("outdoor:showcase_campfire", *FIRE, "campfire", lights=[light(1.48, 2.6, 9, (1, .56, .2))],
         components=[{"component": "animation", "clip": "flicker", "looped": True, "playing": True}])
    for i in range(7):
        a = 2 * math.pi * i / 7 + .15
        x, z = FIRE[0] + 2.6 * math.cos(a), FIRE[1] + 2.6 * math.sin(a)
        yaw = math.degrees(math.atan2(FIRE[0] - x, FIRE[1] - z))
        prop("outdoor:showcase_stump_seat", x, z, f"campfire_seat_{i}", solid=True, size=[.7, .45, .6])
        prop("skeleton", x, z, f"campfire_skeleton_{i}", yaw=yaw, occludes=False)
        level["routes"].append({"id": f"campfire_skeleton_{i}", "loop": True,
                               "steps": [{"step": "play", "clip": "pose_sit_chair", "seconds": 3600, "loop": True}]})
    for i in range(10):
        a = 2 * math.pi * i / 10
        # A broken rocky ring leaves the southern trail approach open.
        if i in (2, 3): continue
        prop("outdoor:showcase_boulder", FIRE[0] + 6.3 * math.cos(a), FIRE[1] + 6.3 * math.sin(a),
             f"clearing_rock_{i}", yaw=i * 29, scale=1.1 + (i % 3) * .15, solid=True, size=[1.4, .65, 1.3])
    for i, (x, z, yaw) in enumerate(((-34, -5, 90), (-19.6, -13, 270), (-10, -23, 90), (1.8, -8, 270),
                                   (18.6, -4, 270), (30, -15, 90), (-20, -28, 90))):
        prop("outdoor:lamp_stand", x, z, f"trail_lamp_{i}", lights=[light(.86, .7, 7)], occludes=True)
    for i, (x, z, yaw) in enumerate(((-29, -10, 180), (13, -13, 270), (-25, -23, 90))):
        prop("outdoor:lamp_stand", x, z, f"cool_clearing_lamp_{i}", yaw=yaw,
             lights=[light(.86, .8, 8, (.65, .78, 1))])
    for i, (model, x, z, scale) in enumerate((("sheet-ghost", 6, -23, 1), ("sheet-ghost-cat", -27, -8, 2),
                                          ("sheet-ghost-cat", 11, -13, 2), ("sheet-ghost-cat", -23, -23, 2))):
        prop(model, x, z, f"forest_ghost_{i}", scale=scale, occludes=False)
        level["routes"].append({"id": f"forest_ghost_{i}", "loop": True,
            "steps": [{"step": "move_to", "x": x - .5, "z": z, "speed": .24},
                      {"step": "move_to", "x": x + .5, "z": z, "speed": .24}]})

    # Visible geology encloses all four edges. At the road openings, tall
    # closed timber gates overlap their adjacent rock faces without fake walls.
    boundary = []
    for x in range(-40, 41, 4):
        boundary.extend(((x, -43.7, 0), (x, 29.7, 180)))
    boundary.extend(((-41.6, -42, 90), (41.6, -42, 270)))
    for z in range(-40, 29, 4):
        if 14 < z < 19: continue
        boundary.extend(((-41.6, z, 90), (41.6, z, 270)))
    for i, (x, z, yaw) in enumerate(boundary):
        prop("outdoor:showcase_rock_face", x, z, f"boundary_rock_{i}", yaw=yaw,
             solid=True, size=[4, 5, 2.2])
    for i, x in enumerate((-41.6, 41.6)):
        prop("outdoor:showcase_road_gate", x, 16.6, f"road_gate_{i}", yaw=90,
             solid=True, size=[8, 3.6, .24])

    # Deterministic varied canopy: clearance belongs to circulation, not to
    # a visually sparse lawn. Models preserve the established low-poly art.
    rng = random.Random(SEED)
    candidates = []
    for z in range(-41, 29, 3):
        for x in range(-38, 40, 3):
            px, pz = x + rng.uniform(-.6, .6), z + rng.uniform(-.6, .6)
            if -.5 < pz < 24: continue
            if trail_distance(px, pz) < 2.1 or math.dist((px, pz), FIRE) < 7.5 or math.dist((px, pz), POND) < 6.2: continue
            if any(math.dist((px, pz), q) < 1.8 for q in ((6, -23), (-27, -8), (11, -13), (-23, -23))): continue
            candidates.append((px, pz))
    rng.shuffle(candidates)
    for i, (x, z) in enumerate(candidates[:247]):
        model = ("outdoor:tree_03", "outdoor:tree_03", "outdoor:tree_01", "outdoor:tree_02")[i % 4]
        h = catalog[model]["size"][1]
        prop(model, x, z, f"forest_tree_{i}", yaw=rng.uniform(0, 360), scale=rng.uniform(.85, 1.18),
             solid=True, size=[.6, h, .6], occludes=False)
    for i in range(459):
        for _ in range(100):
            x, z = rng.uniform(-39, 39), rng.uniform(-41, 28)
            if -.5 < z < 24 or trail_distance(x, z) < 1.6 or math.dist((x, z), FIRE) < 6.5 or math.dist((x, z), POND) < 5.1: continue
            break
        else: continue
        prop("outdoor:grass_patch_large" if i % 3 else "outdoor:grass_patch_small", x, z, f"grass_forest_{i}",
             yaw=rng.uniform(0, 360), scale=rng.uniform(.85, 1.15), occludes=False)
    # At a shared footprint edge the engine's first room owns floor/ceiling
    # lookups. Put sealed cottages before adjoining open floor tiles so a
    # perimeter wall cannot accidentally inherit the outdoor 8 m ceiling.
    level["rooms"].sort(key=lambda room: room["ceiling"]["kind"] != "gable")
    return level


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="Fail if committed JSON differs from authored output")
    args = parser.parse_args()
    expected = serialise(build_level())
    if args.check:
        if not OUTPUT.exists() or OUTPUT.read_text() != expected:
            print(f"FAIL: {OUTPUT} is stale; run {Path(__file__).name}")
            return 1
        print("Lantern Hollow source is deterministic and current")
        return 0
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT.write_text(expected)
    print(f"Authored {OUTPUT}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
