#!/usr/bin/env python3
"""Idempotent Home-only concept dressing, preserving encounters and routes."""
import json
import math
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
TAG = "[home-concept]"


def add(level, key, **piece):
    piece["comment"] = TAG + " " + piece.get("comment", "")
    level.setdefault(key, []).append(piece)


def prop(level, name, x, z, size, y=0, yaw=0, lights=None, scale=1):
    piece = dict(model="home:" + name, x=x, z=z, y=y, rotation_degrees=yaw,
                 size=size, solid=False, scale=scale)
    if lights:
        piece["lights"] = lights
    add(level, "props", **piece)


def board(level, x, z, length, yaw, y, height, thickness=.028):
    add(level, "baseboards", x=x, z=z, length=length, rotation_degrees=yaw,
        y=y, height=height, thickness=thickness, material="home:baseboard_white_01",
        comment="Raised painted timber profile; wall-mounted, collision-free.")


def main():
    path = ROOT / "assets/levels/places_demo.json"
    level = json.loads(path.read_text())
    local_materials = {
        "home:wall_paint_offwhite_01": "home:wall_paint_warm_01",
        "home:hardwood_oak_01": "home:hardwood_oak_warm_01",
        "home:ceiling_white_01": "home:ceiling_warm_01",
        "home:ceiling_plaster_01": "home:ceiling_plaster_warm_01",
    }
    def local(piece):
        for key, value in piece.items():
            if isinstance(value, str) and value in local_materials:
                piece[key] = local_materials[value]
    for key in ("rooms", "walls", "floor_regions", "stairs", "archways"):
        for piece in level.get(key, []):
            if piece.get("x", 0) >= 52.98:
                local(piece)
    for key in ("props", "baseboards", "void_walls", "thresholds"):
        level[key] = [p for p in level.get(key, []) if not p.get("comment", "").startswith(TAG)]
    # Existing placements keep their identity/order; shared assets elsewhere
    # are not modified. In particular no encounter prop is removed or moved.
    replacements = {
        ("core:sink", 54.8, 3.425): ("sink", [ .6, .9, .6]),
        ("core:stove", 56.0, 3.45): ("stove", [.6, .9, .6]),
        ("core:table", 56.0, 7.6): ("dining_table", [1.4, .75, .8]),
        ("core:chair", 54.95, 7.6): ("dining_chair", [.5, .902, .49]),
        ("core:chair", 57.05, 7.6): ("dining_chair", [.5, .902, .49]),
        ("core:couch", 61.3, 5.6): ("sofa", [2, .9, .9]),
        ("core:table", 62.7, 5.6): ("coffee_table", [1.4, .46, .8]),
        ("core:tv", 64.5, 6.55): ("tv_console", [1.55, .55, .45]),
        ("core:rug", 62.7, 5.6): ("rug", [2, .02, 1.4]),
        ("core:armchair", 62.7, 7.3): ("armchair", [.9, .9, .9]),
        ("core:lamp", 60.6, 4.4): ("floor_lamp", [.35, 1.5, .35]),
        ("core:bookshelf", 63.9, 3.35): ("bookshelf", [1, 1.8, .35]),
        ("core:rug", 60.5, 12.9): ("rug", [2, .02, 1.4]),
        ("core:lamp", 61.9, 13.9): ("floor_lamp", [.35, 1.5, .35]),
        ("core:armchair", 59.2, 14.0): ("armchair", [.9, .9, .9]),
        ("core:bookshelf", 64.45, 14.0): ("bookshelf", [1, 1.8, .35]),
    }
    local_sizes = {"home:sofa": [2, .9, .9],
                   "home:coffee_table": [1.4, .46, .8],
                   "home:tv_console": [1.55, .55, .45],
                   "home:bookshelf": [1, 1.8, .35]}
    for p in level["props"]:
        key = p["model"], p.get("x"), p.get("z")
        if key in replacements:
            name, size = replacements[key]
            p["model"], p["size"] = "home:" + name, size
        # Size is local model stock; the engine rotates its collision box.
        # Do not pre-swap dimensions for yaw, which creates phantom blockers.
        if p["model"] in local_sizes and 53 <= p["x"] <= 65 and 3 < p["z"] < 15:
            p["size"] = local_sizes[p["model"]]
        if p["model"] == "home:tv_console" and p["x"] in (64.5, 64.4):
            p["x"] = 64.4  # Actual 450mm cabinet depth, clear of wall face.
            p["comment"] = "Home CRT console in the existing television position."
        if p["model"] == "home:sink" and p["x"] == 54.8:
            p["z"] = 3.45  # 600mm stock sits on the same wall face as the bases.
        if p["model"] == "home:rug" and p["x"] == 62.7:
            p["rotation_degrees"] = 90
            p["scale"] = 1.38
        if p["model"] == "home:coffee_table" and p["x"] == 62.7:
            p["rotation_degrees"] = 90
            p["size"] = [1.4, .46, .8]
        if p["model"] == "home:floor_lamp":
            p["lights"] = [dict(shape="point", offset=[0, 1.24, 0], intensity=.6,
                                range=4.4, color=[1, .88, .68], falloff="smooth")]
        if p.get("id") == "kitchen_pendant":
            p["lights"][0]["intensity"] = .55
    # Retain the original domestic fill positions using the committed globe
    # model, at half scale. The asset-less CPU mesh needs only one placeholder
    # box per globe instead of the flush fixture's many drum/rim quads.
    level["ceiling_lights"] = [
        light for light in level["ceiling_lights"]
        if not (53 <= light["x"] <= 65 and 3 < light["z"] < 15)
    ]
    fill_positions = ((55.4, 4.4), (55.4, 7.2), (61.0, 5.2), (61.0, 9.1),
                      (55.6, 10.6), (59.4, 12.4), (62.6, 12.4), (62.6, 14.1))
    for x, z in fill_positions:
        ceiling = 3.6 + 1.4 * (1 - abs(z - 9) / 6)
        floor = 1.2 if x >= 58 and z >= 11 else -.9
        # The half-scale rose's actual top is .395 m above the model origin.
        prop(level, "ball_light", x, z, [.38, .8, .38],
             y=round(ceiling - .01 - .395 - floor, 6), scale=.5,
             lights=[dict(shape="point", offset=[0, -.02, 0],
                          intensity=.58 if z < 11 else .45,
                          # Prop lights measure 3D distance, whereas the old
                          # ceiling family measured horizontal reach. Longer
                          # reach preserves useful fill down this tall vault.
                          range=12, color=[1, .91, .77], falloff="smooth")])
    prop(level, "crt_tv", 64.4, 6.55, [.55, .48, .46], y=.55, yaw=270)
    prop(level, "plant_table", 64.4, 6.0, [.13, .28, .13], y=.55, scale=1.6)
    prop(level, "plant_table", 62.72, 5.57, [.13, .28, .13], y=.46, scale=1.7)
    prop(level, "mug", 62.45, 5.38, [.117, .09, .078], y=.46, yaw=35)
    prop(level, "book_stack", 62.87, 5.78, [.262, .057, .197], y=.46)
    prop(level, "book_stack", 62.7, 5.55, [.262, .057, .197], y=.136, yaw=15)
    prop(level, "cushion", 61.52, 5.16, [.33, .32, .115], y=.44, yaw=90)
    prop(level, "cushion", 61.52, 6.10, [.33, .32, .115], y=.44, yaw=90)
    prop(level, "plant_table", 56.0, 7.6, [.13, .28, .13], y=.75, scale=1.5)
    prop(level, "kettle", 55.40, 3.36, [.217, .201, .162], y=.90)
    prop(level, "toaster", 56.60, 3.36, [.25, .16, .169], y=.90)
    # Additional matching uppers above the right-hand worktop, already shown
    # in the reference kitchen; kept non-solid above the standable counter.
    prop(level, "cabinet_wall", 56.6, 3.315, [.6, .72, .33], y=1.45)
    prop(level, "cabinet_strip", 56.6, 3.42, [.55, .026, .07], y=1.424,
         lights=[dict(shape="line", length=.48, offset=[0, -.025, .08],
                      intensity=.24, range=2.5, color=[1, .88, .68], falloff="smooth")])
    for x in (53.6, 54.2, 54.8):
        prop(level, "cabinet_strip", x, 3.42, [.55, .026, .07], y=1.424,
             lights=[dict(shape="line", length=.48, offset=[0, -.025, .08],
                          intensity=.24, range=2.5, color=[1, .88, .68], falloff="smooth")])
    # A regular wall front gives the ceramic a supported lightmap chart.
    # Its backing is embedded in the north wall, with an 80mm tile build-up.
    level['walls'] = [w for w in level['walls'] if not w.get('comment', '').startswith(TAG)]
    add(level, "walls", x=53.305, z=3.151, width=3.695, depth=.08, y=.003, height=.544,
        material="home:backsplash_01", comment="Working ceramic splash, up to the wall-unit underside.")
    # Domestic outlets beside the counters and living console; detailed
    # plates are static decorations, with no added entities or bindings.
    # The rear mounting stock meets the actual wall/splash face within 1 mm.
    for x, z, y, yaw in [(55.55, 3.235, 1.12, 0), (64.694, 6.55, .23, 270)]:
        prop(level, "outlet", x, z, [.085, .12, .014], y=y, yaw=yaw)
    prop(level, "landscape_frame", 64.687, 7.95, [.45, .5, .032], y=1.5, yaw=270)
    # A shade-free ball pendant over the existing living group. The
    # rose remains on the gable plane; the original kitchen pendant is kept.
    prop(level, "ball_light", 62.0, 6.6, [.38, .8, .38], y=4.54,
         lights=[dict(shape="point", offset=[0, -.10, 0], intensity=.55,
                      range=5, color=[1, .9, .72], falloff="smooth")])
    # Real raised skirting shoulders above the original lower boards. These
    # keep their backs on the room faces and split at the existing openings.
    for b in list(level.get("baseboards", [])):
        angle = math.radians(b.get("rotation_degrees", 0))
        end_x = b["x"] + b["length"] * math.cos(angle)
        end_z = b["z"] - b["length"] * math.sin(angle)
        in_home = (min(b["x"], end_x) >= 53 - 1e-6
                   and max(b["x"], end_x) <= 65 + 1e-6
                   and min(b["z"], end_z) >= 3.15 - 1e-6
                   and max(b["z"], end_z) <= 14.85 + 1e-6)
        if in_home and not b.get("comment", "").startswith(TAG):
            y = b.get("y", -.9) + b.get("height", .09)
            board(level, b["x"], b["z"], b["length"], b.get("rotation_degrees", 0), y, .028, .023)
    # Proud of the engine's existing frame stock, without coincident faces.
    prop(level, "door_casing", 61.0, 3.25, [1.58, 2.18, .054])
    path.write_text(json.dumps(level, indent=2) + "\n")


if __name__ == "__main__":
    main()
