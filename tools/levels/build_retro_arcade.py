#!/usr/bin/env python3
"""Compose connected bowling, arcade, birthday and restroom spaces.

The original lanes and arcade retain their quiet commercial character beside a
separate prepared party room. Geometry/material edits belong here; the compiler prepares
lighting, collision and probes. No artwork is synthesized by this generator.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path

if __package__:
    from .birthday_compositions import PartyComposer
    from .arcade_restroom import compose_restroom, tile_trim
else:
    from birthday_compositions import PartyComposer
    from arcade_restroom import compose_restroom, tile_trim

ROOT = Path(__file__).resolve().parents[2]
OUTPUT = ROOT / "levels/retro_arcade_demo.json"
CARPET = "arcade:carpet_cosmic_bowl_01"
WALL = "arcade:wall_navy_01"
CEILING = "arcade:ceiling_acoustic_01"
WOOD = "arcade:lane_wood_01"
GUTTER = "arcade:gutter_dark_01"
PARTY_WALL = "arcade:wall_party_purple_01"


def baseboard_profile(level):
    """Two modest stock bands on real interior wall faces, clear of passages.

    Each strip uses the actual adjoining floor material and floor-folded UVs.
    Existing baseboard geometry owns corner clipping and has no collision. The
    upper inset band is a small stepped profile, not a second coplanar layer.
    Runs merge along a continuous face so adjacent wall records never introduce
    paired end caps. This helper authors source geometry; it does no rendering.
    """
    def subtract(spans, cuts):
        for cut_lo, cut_hi in cuts:
            pieces = []
            for lo, hi in spans:
                if cut_hi <= lo or cut_lo >= hi:
                    pieces.append((lo, hi))
                else:
                    if lo < cut_lo:
                        pieces.append((lo, cut_lo))
                    if cut_hi < hi:
                        pieces.append((cut_hi, hi))
            spans = pieces
        return spans

    def solids(wall):
        horizontal = wall["width"] >= wall["depth"]
        lo = wall["x"] if horizontal else wall["z"]
        length = wall["width"] if horizontal else wall["depth"]
        cuts = [(lo+opening["offset"], lo+opening["offset"]+opening["width"])
                for opening in wall.get("openings", [])
                if opening.get("sill", 0) < .10]
        for start, end in subtract([(lo, lo+length)], cuts):
            yield ((start, wall["z"], end, wall["z"]+wall["depth"]) if horizontal
                   else (wall["x"], start, wall["x"]+wall["width"], end))

    grouped = {}
    for index, wall in enumerate(level["walls"]):
        horizontal = wall["width"] >= wall["depth"]
        along = "x" if horizontal else "z"
        across = "z" if horizontal else "x"
        length = wall["width"] if horizontal else wall["depth"]
        thickness = wall["depth"] if horizontal else wall["width"]
        faces = (("south", wall[across]+thickness, 1, 0),
                 ("north", wall[across], -1, 180)) if horizontal else (
                 ("east", wall[across]+thickness, 1, 90),
                 ("west", wall[across], -1, 270))
        for face, plane, normal, yaw in faces:
            spans = []
            for room in level["rooms"]:
                # The commercial restroom uses ceramic skirting, separately
                # authored with ordinary wall UVs instead of a carpet fold.
                if room.get("material") == "arcade:restroom_tile_01":
                    continue
                extent = room["depth"] if horizontal else room["width"]
                if not room[across] <= plane+normal*.005 <= room[across]+extent:
                    continue
                room_length = room["width"] if horizontal else room["depth"]
                lo, hi = max(wall[along], room[along]), min(wall[along]+length, room[along]+room_length)
                if hi > lo:
                    spans.append((lo, hi, room.get("material", level["defaults"]["floor"])))
            cuts = [(wall[along]+o["offset"], wall[along]+o["offset"]+o["width"])
                    for o in wall.get("openings", []) if o.get("sill", 0) < .10]
            # A perpendicular wall's stock closes a corner or splits this face;
            # hidden trim is omitted instead of continuing through that stock.
            for other_index, other in enumerate(level["walls"]):
                if other_index == index:
                    continue
                for x0, z0, x1, z1 in solids(other):
                    sample = plane+normal*.009
                    if horizontal and z0 < sample < z1:
                        cuts.append((x0, x1))
                    elif not horizontal and x0 < sample < x1:
                        cuts.append((z0, z1))
            for lo, hi, room_material in spans:
                floor_spans = [(start, end, room_material)
                               for start, end in subtract([(lo, hi)], cuts)]
                # Later floor patches win, then floor regions. Splitting their
                # boundaries keeps wood/deck stock ordinary wherever it reaches
                # a wall, instead of assigning fluorescent carpet by room name.
                for patch in [*level["floor_patches"], *level["floor_regions"]]:
                    extent = patch["depth"] if horizontal else patch["width"]
                    if not patch[across] <= plane+normal*.020 <= patch[across]+extent:
                        continue
                    patch_length = patch["width"] if horizontal else patch["depth"]
                    patch_lo, patch_hi = patch[along], patch[along]+patch_length
                    pieces = []
                    for start, end, material in floor_spans:
                        near, far = max(start, patch_lo), min(end, patch_hi)
                        if far <= near:
                            pieces.append((start, end, material))
                            continue
                        if start < near:
                            pieces.append((start, near, material))
                        pieces.append((near, far, patch.get("material", room_material)))
                        if far < end:
                            pieces.append((far, end, material))
                    floor_spans = pieces
                for start, end, material in floor_spans:
                    key = (horizontal, round(plane, 6), normal, yaw, material)
                    grouped.setdefault(key, []).append((start, end))

    boards = []
    for (horizontal, plane, normal, yaw, material), spans in grouped.items():
        merged = []
        for lo, hi in sorted(spans):
            if merged and lo <= merged[-1][1]+1e-6:
                merged[-1] = (merged[-1][0], max(merged[-1][1], hi))
            elif hi-lo > .001:
                merged.append((lo, hi))
        for lo, hi in merged:
            if not horizontal:
                # Butt the long north/south stock 2 mm clear of the east/west
                # body's front. Quarter-turn f32 endpoint drift otherwise
                # turns overlapping corner caps into metre-long micro-slivers.
                # The wall closes these tiny fitting seams; no surface offset
                # or geometry-check suppression is involved.
                for cross_wall in level["walls"]:
                    if cross_wall["width"] < cross_wall["depth"]:
                        continue
                    for x0, z0, x1, z1 in solids(cross_wall):
                        if not x0 <= plane+normal*.009 <= x1:
                            continue
                        if abs(lo-z1) < 1e-6:
                            lo += .020
                        if abs(hi-z0) < 1e-6:
                            hi -= .020
            start = lo if yaw in (0, 270) else hi
            x, z = (start, plane) if horizontal else (plane, start)
            for y, height, thickness in ((0, .09, .018), (.09, .010, .009)):
                boards.append(dict(x=round(x, 6), z=round(z, 6), y=y,
                    length=round(hi-lo, 6), rotation_degrees=yaw,
                    height=height, thickness=thickness, material=material,
                    uv_projection="floor_fold"))
    return boards


def build_level():
    catalog = json.loads((ROOT / "assets/catalog.json").read_text())
    gameplay = json.loads((ROOT / "assets/gameplay.json").read_text())["assets"]
    adopted = json.loads((ROOT / "assets/environments/arcade/adopted_placement.json").read_text())
    sizes = {e["id"]: e["size"] for e in catalog["assets"]
             if e.get("asset_type") in {"prop", "entity"}}
    level = dict(schema="places-level", format_version=1, id="retro_arcade_demo", name="Cosmic Lanes & Arcade",
                 author="Places", spawn=dict(x=6.8, z=28.0, yaw_degrees=25),
                 defaults=dict(wall=WALL, floor=CARPET, ceiling=CEILING),
                 environment=dict(fog=dict(color=[.025, .035, .09], density=0,
                                           reference_y=1, height_gain=0)),
                 rooms=[], walls=[], floor_patches=[], floor_regions=[],
                 props=[], decals=[], ceiling_lights=[], baseboards=[])
    # Three established volumes own their respective carpets and lights. The
    # additional commercial restroom is composed below without moving them.
    # Floors meet at the centre planes of the two shared passage walls, so
    # threshold support has one owner on either side rather than a spare slab.
    level["rooms"] = [
        dict(x=0, z=0, width=9.2, depth=30, height=3.2,
             material="arcade:carpet_retro_swirl_01"),
        dict(x=9.2, z=0, width=10.8, depth=30, height=3.2, material=CARPET),
        dict(x=20, z=17, width=10, depth=13, height=3.2,
             material="arcade:carpet_neon_confetti_01"),
    ]
    level["walls"] = [
        dict(x=-.24, z=-.24, width=20.48, depth=.24, height=3.2, material=WALL),
        dict(x=-.24, z=30, width=20.36, depth=.24, height=3.2, material=WALL),
        dict(x=-.24, z=0, width=.24, depth=30, height=3.2, material=WALL),
        dict(x=19.88, z=0, width=.24, depth=17, height=3.2, material=WALL),
        dict(x=9.08, z=0, width=.24, depth=30, height=3.2, material=WALL,
             openings=[dict(kind="passage", offset=23.2, width=3.2, height=2.55)]),
        dict(x=19.88, z=17, width=.24, depth=13, height=3.2, material=WALL,
             faces=dict(east=PARTY_WALL),
             openings=[dict(kind="passage", offset=7, width=3.2, height=2.55)]),
        dict(x=20.12, z=16.76, width=10.12, depth=.24, height=3.2, material=PARTY_WALL),
        dict(x=30, z=17, width=.24, depth=13, height=3.2, material=PARTY_WALL),
        dict(x=20.12, z=30, width=10.12, depth=.24, height=3.2, material=PARTY_WALL),
    ]

    def prop(name, x, z, identity, y=0, yaw=0, solid=None, **extra):
        identity_model = "arcade:" + name
        if solid is None:
            solid = name.startswith(("cabinet_", "pinball_", "fixed_chair_")) or name in {
                "ball_return", "ball_rack", "pedestal_table", "vending_machine", "change_machine"}
        entry = dict(id=identity, model=identity_model, x=x, y=y, z=z,
                     rotation_degrees=yaw, size=sizes[identity_model], solid=solid,
                     occludes=False, **extra)
        level["props"].append(entry)
        return entry

    def patch(x, z, width, depth, material):
        level["floor_patches"].append(dict(x=x, z=z, width=width, depth=depth, material=material))

    def lane_physics(model):
        return dict(gameplay[model]["physics"], reset_group="arcade_lane_1")

    def decal(material, x, z, width, height, surface="floor", y=0, yaw=0):
        level["decals"].append(dict(material="arcade:decal_" + material + "_01",
            x=x, z=z, y=y, width=width, height=height, surface=surface,
            rotation_degrees=yaw))

    # One fitted 4:1 motion mural marks the pin-deck wall. The arcade side
    # stays navy, rather than repeating a second identical composition.
    decal("bowling_mural", 14.65, .003, 8.0, 2.0, surface="wall_south", y=2.03)

    # Warm lane stock uses normal world-aligned material coordinates. Curved
    # gutter bodies sit above a genuinely recessed support surface, not a
    # black strip painted over the lane. No coincident carpet remains below it.
    lane_centres = (10.65, 12.65, 14.65, 16.65)
    # Ordinary dark deck stock connects the lane/gutter pairs. Fluorescent
    # carpet belongs to the concourse, never between the lanes.
    for x in lane_centres[:-1]:
        patch(x+.8, 1.5, .4, 18.5, GUTTER)
    for lane, x in enumerate(lane_centres, 1):
        patch(x-.525, 1.5, 1.05, 18.5, WOOD)
        for side in (-1, 1):
            gx = x + side*.665
            level["floor_regions"].append(dict(x=gx-.135, z=1.75, width=.27,
                depth=18, offset_y=-.17, material=GUTTER, edge_material=GUTTER))
            for segment in range(6):
                z = 3.25 + segment*3
                # Prop y is relative to the recessed support (-.17 m). The
                # body starts at -.125 m and its rim meets the lane at .01 m.
                # The fixed convex strips follow the existing curved stock;
                # its recessed floor alone sits 6 cm below the visible bowl.
                prop("gutter_segment", gx, z, f"lane_{lane}_gutter_{side}_{segment}", y=.045,
                     physics=gameplay["arcade:gutter_segment"]["physics"])
        for segment in range(6):
            prop("lane_divider", x+.96, 3.25+segment*3, f"lane_{lane}_divider_{segment}")
        decal("foul_line", x, 20.02, 1.05, .525)
        for arrow in range(7):
            dx = (arrow-3)*.11
            decal("lane_arrow", x+dx, 15.45+abs(arrow-3)*.14, .10, .10)
        # Ten pins per lane, recognisable triangular arrangement and red bands.
        for row in range(4):
            for index in range(row+1):
                extra = dict(physics=lane_physics("arcade:bowling_pin")) if lane == 1 else {}
                prop("bowling_pin", x+(index-row/2)*.21, 3.6-row*.19,
                     f"lane_{lane}_pin_{row}_{index}", **extra)
        monitor = "score_monitor_inactive" if lane == 4 else "score_monitor"
        spill = [] if lane == 4 else [dict(shape="point", offset=[0, .28, .37],
            color=[.17, .35, 1], intensity=.10, range=2.4)]
        prop(monitor, x, 19.7, f"lane_{lane}_score_crt", y=2.05, lights=spill)
    patch(9.72, 20, 8.0, 3.0, WOOD)

    # Two common ball-return groups serve the four lanes, with varied muted
    # balls supported by the upper rails. Their holes are geometry, not dots.
    for group, x in enumerate((11.65, 15.65)):
        prop("ball_return", x, 22.5, f"return_{group}")
        prop("ball_rack", x, 24.2, f"rack_{group}")
        for slot, color in enumerate(("purple", "burgundy", "cobalt") if group == 0
                                      else ("black", "red", "green")):
            if group == 0 and slot == 2:
                # The reusable rack keeps its existing solid bounds. The
                # playable ball starts on clear approach floor instead of
                # intersecting that legacy collider below its 0.72 m top.
                prop("bowling_ball_"+color, lane_centres[0], 21.2,
                     f"rack_{group}_ball_{slot}", y=.006, yaw=slot*38,
                     physics=lane_physics("arcade:bowling_ball_cobalt"))
            else:
                prop("bowling_ball_"+color, x+(slot-1)*.28, 24.2,
                     f"rack_{group}_ball_{slot}", y=adopted['ball_support_heights'][slot],
                     yaw=slot*38)
    prop("bowling_ball_marbled", 15.65, 23.05, "return_marbled_ball", y=adopted['return_ball_height'], yaw=18)
    # The rear housing face is exactly Z=-.905 in the accepted export. Mount
    # the reset outside that face and its conservative solid box; the tapered
    # left face would put the control behind invisible collision stock.
    level["props"].append(dict(id="lane_1_reset_switch", model="home:wall_switch",
        x=11.65, y=.65, z=21.585, rotation_degrees=180, size=sizes["home:wall_switch"],
        solid=False, occludes=False,
        components=[dict(component="interactable", prompt="Reset lane", reach=1.7),
                    dict(component="animation", clip="toggle", looped=False, playing=False)],
        bindings=[dict(on="interact", actions=[dict(action="toggle_animation", clip="toggle"),
            dict(action="reset_physics_group", group="arcade_lane_1")])]))

    # Comfortable fixed seating and ordinary laminate, plastic and metal all
    # remain nonemitters under the same mixed practical/blacklight scene.
    for group, x in enumerate((10.8, 16.5)):
        prop("pedestal_table", x, 27.1, f"table_{group}")
        prop("soda_bottle", x+.16, 27.0, f"table_{group}_bottle", y=.72, yaw=20)
        for slot, (dx, dz, yaw, color) in enumerate((
            (-1.0, 0, 90, "red"), (1.0, 0, 270, "blue"),
            (0, 1.15, 180, "blue"), (0, -1.15, 0, "red"))):
            prop("fixed_chair_"+color, x+dx, 27.1+dz, f"chair_{group}_{slot}", yaw=yaw)

    # The six carpets remain reusable catalog assets. This composed interior
    # chooses one different design per room rather than showroom floor bays.

    def cabinet(name, x, z, identity, yaw=90, color=(.24, .43, 1), dim=False):
        offsets = adopted['cabinets']["arcade:"+name]['light_offsets']
        scale = .16 if dim else 1
        entry = prop(name, x, z, identity, yaw=yaw, lights=[
            dict(shape="point", offset=offsets[0],
                 color=list(color), intensity=.36*scale, range=2.4),
            dict(shape="point", offset=offsets[1],
                 color=list(color), intensity=.18*scale, range=1.8)])
        if dim:
            entry["components"] = [dict(component="material", current="standby",
                variants=[dict(name="standby", emission_scale=scale)])]

    for name, z, color in (("racing", 6.0, (.22, .48, 1)),
                          ("shooter", 8.0, (.15, .7, .55)),
                          ("action", 10.0, (1, .28, .14)),
                          ("fighting", 12.1, (1, .16, .55)),
                          ("scifi", 14.3, (.52, .22, 1))):
        cabinet("cabinet_"+name, .82, z, "west_"+name, color=color)
    cabinet("cabinet_fighting", 5.8, 1.2, "north_fighting", yaw=0, dim=True)
    cabinet("cabinet_racing", 7.1, 1.2, "north_racing", yaw=0)
    # Closely fitted wall rows and two three-wide, back-to-back islands turn
    # the empty floor into a working arcade. Keep the restroom landing, the
    # southern pinball/concourse and the bowling passage clear. The islands'
    # side edges leave at least 1.8 m to both facing wall rows.
    cabinet("cabinet_shooter", 3.2, 1.2, "north_shooter", yaw=0, color=(.25,.62,1))
    cabinet("cabinet_action", 4.5, 1.2, "north_action", yaw=0, color=(1,.28,.42))
    for index, (name,z,color) in enumerate((
        ("action",5.0,(1,.28,.42)), ("fighting",7.0,(1,.18,.62)),
        ("scifi",9.0,(.52,.25,1)), ("racing",11.0,(.30,.52,1)),
        ("shooter",13.2,(.25,.62,1)), ("action",15.5,(1,.28,.42)),
    )):
        cabinet("cabinet_"+name,.82,z,f"west_infill_{index}",color=color)
    games=(("racing",(.30,.52,1)),("fighting",(1,.18,.62)),
           ("shooter",(.25,.62,1)),("scifi",(.52,.25,1)),("action",(1,.28,.42)))
    for index,z in enumerate((5.0,6.2,7.4,8.6,9.8,11.0,12.2,13.4,14.6)):
        name,color=games[index%len(games)]
        cabinet("cabinet_"+name,8.4,z,f"east_{index}",yaw=270,color=color)
    for island,z in enumerate((9.5,15.5)):
        for slot,x in enumerate((3.65,4.6,5.55)):
            for face,yaw in enumerate((180,0)):
                name,color=games[(island*3+slot+face)%len(games)]
                depth=adopted['cabinets']["arcade:cabinet_"+name]['layout_depth']
                cabinet("cabinet_"+name,x,z+(-1 if face==0 else 1)*(depth/2+.025),
                        f"island_{island}_{slot}_{face}",yaw=yaw,color=color)
    for index, name in enumerate(("pinball_cosmic", "pinball_comet")):
        prop(name, 1.1, 18.0+index*2.1, f"pinball_{index}", yaw=90,
             lights=[dict(shape="point", offset=[0, 1.65, -.46],
                          color=[.82, .30, 1], intensity=.24, range=2.0),
                     dict(shape="point", offset=[0, 1.14, .12],
                          color=[1, .82, .55], intensity=.40, range=1.15)])
    prop("change_machine", .5, 23, "change_machine", yaw=90)
    # Paired waiting seats fit the east Arcade wall, ahead of the passage.
    # Upholstery swaps preserve one structural design and an open concourse.
    for name, z in (("red_blue_red", 18.6), ("blue_red_blue", 21.3)):
        prop("bench_three_seat_"+name, 8.65, z, "waiting_bench_"+name,
             yaw=270, solid=True)
    prop("vending_machine", .9, 25, "vending_machine", yaw=90,
         lights=[dict(shape="point", offset=[0, 1.35, .51], color=[.3, .55, 1],
                      intensity=.46, range=3.8),
                 dict(shape="point", offset=[0, 2.13, .5], color=[.5, .65, 1],
                      intensity=.15, range=1.5)])
    prop("neon_bowl", 19.77, 21.5, "bowl_neon", y=1.72, yaw=270,
         lights=[dict(shape="point", offset=[0, .4, .09], color=[1, .08, .14],
                      intensity=.18, range=3.2),
                 dict(shape="point", offset=[.65, .45, .09], color=[.12, .25, 1],
                      intensity=.10, range=2.8),
                 dict(shape="point", offset=[-.65, .6, .09], color=[1, .7, .12],
                      intensity=.06, range=2.2)])
    # Operational battens below supply both visible lenses and controllable
    # ultraviolet excitation. The reusable static batten also appears in Zoo.
    for name, x, z in (("litter_can", 2.5, 20.5), ("litter_cup", 17.8, 27.9),
                       ("litter_wrapper", 12.2, 25.6), ("litter_paper", 3.1, 23.4),
                       ("litter_bottle_fallen", 8.8, 28.0)):
        prop(name, x, z, name, yaw=38)

    # Arcade end troffers retain pale readable faces around two violet
    # practical battens. These two lamps cast ordinary coloured light with
    # zero UV excitation; the four switch-controlled sources below still own
    # all fluorescence. Short local pools leave space for screens and darkness.
    # Keep fixture order and the strong bowling/party keys unchanged so the
    # accepted balloon illumination retains its selected direct-source slots.
    for index, (x, z, brightness, color, emission, reach, fixture) in enumerate((
        (4.2, 3.9, .24, [.64, .70, 1], .72, 5.5, "fluorescent_panel_01"),
        (4.2, 10.5, .42, [1, .12, .68], .64, 6.0, "blacklight_tube"),
        (4.2, 19.5, .42, [.54, .18, 1], .64, 6.0, "blacklight_tube"),
        (4.2, 25.5, .24, [.64, .70, 1], .72, 6.0, "fluorescent_panel_01"),
        (10.2, 5.1, .45, [1, .95, .88], .95, 6.5, "fluorescent_panel_01"),
        (16.2, 5.1, .45, [1, .95, .88], .95, 6.5, "fluorescent_panel_01"),
        (10.2, 18.3, .42, [1, .95, .88], .95, 6.5, "fluorescent_panel_01"),
        (16.2, 18.3, .42, [1, .95, .88], .95, 6.5, "fluorescent_panel_01"),
        (10.2, 27.3, .30, [1, .24, .66], .78, 5.1, "fluorescent_panel_01"),
        (16.2, 27.3, .42, [1, .95, .88], .95, 6.5, "fluorescent_panel_01"),
    )):
        entry = dict(id=f"troffer_{index}", fixture="arcade:"+fixture,
            x=x, z=z, align="none", brightness=brightness, color=color,
            emission=emission, range=reach)
        if fixture == "blacklight_tube":
            entry["ultraviolet"] = 0
        level["ceiling_lights"].append(entry)
    for index, (x, z, reach) in enumerate(((4.2, 7.5, 8.0), (4.2, 17.1, 8.0),
                                        (13.8, 24.3, 8.5), (25.4, 23.1, 8.0))):
        level["ceiling_lights"].append(dict(id=f"cosmic_uv_{index}",
            fixture="arcade:blacklight_tube", x=x, z=z, align="none",
            brightness=.07, color=[.32, .12, .8], emission=.5, ultraviolet=1.8,
            range=reach, enabled=True, switchable=True, switch_group="cosmic"))
    level["props"].append(dict(id="cosmic_blacklight_switch", model="home:wall_switch",
        x=.051, y=1.2, z=26.3, rotation_degrees=90, size=sizes["home:wall_switch"],
        solid=False, occludes=False,
        components=[dict(component="interactable", prompt="Cosmic blacklights", reach=1.7),
                    dict(component="animation", clip="toggle", looped=False, playing=False)],
        bindings=[dict(on="interact", actions=[dict(action="toggle_animation", clip="toggle")]
            +[dict(action="toggle", target="cosmic_uv_0")])]))

    party_metadata = json.loads((ROOT / "assets/environments/arcade/party/asset_metadata.json").read_text())
    weight_anchor = party_metadata["models"]["arcade:party_balloon_weight"]["anchor_height"]
    party = PartyComposer(sizes, weight_anchor_height=weight_anchor)
    party.simple_rect(23.4, 28.0, "party_simple")
    party.present_table(28.4, 18.5, "party_gifts")
    party.round_table(23.4, 20.6, "party_round")
    party.cake_table(27.6, 25.4, "party_cake", yaw=90)
    # Small hanging curls attach beside the walls, never spanning an entrance
    # or obscuring the cake sightline. Each colour remains ordinary paper.
    for index, (color, x, z, yaw) in enumerate((
        ("purple", 21.15, 17.012, 0), ("pink", 21.60, 17.012, 0),
        ("yellow", 22.05, 17.012, 0), ("blue", 24.40, 17.012, 0),
        ("orange", 24.85, 17.012, 0), ("purple", 29.987, 21.6, 270),
        ("pink", 29.987, 22.05, 270), ("yellow", 29.987, 27.5, 270),
        ("blue", 29.987, 27.95, 270), ("orange", 29.987, 28.4, 270),
    )):
        party.prop("streamer_"+color, x, z, f"party_streamer_{index}", y=2.49983, yaw=yaw)
    level["props"].extend(party.props)
    level["timers"] = party.timers
    # Two central local keys enter the existing four always-on entity-direct
    # slots, giving latex genuine normal-dependent illumination. Redistribute
    # the same 1.24 total output; room-wide brightness is not increased.
    for index, (x, z, brightness, color) in enumerate((
        (25.8, 20.7, .43, [1, .94, .88]),
        (28.2, 19.5, .18, [1, .92, .84]),
        (22.2, 27.3, .20, [.76, .82, 1]),
        (25.8, 26.1, .43, [1, .92, .85]),
    )):
        level["ceiling_lights"].append(dict(id=f"party_troffer_{index}",
            fixture="arcade:fluorescent_panel_01", x=x, z=z, align="none",
            brightness=brightness, color=color, emission=.85, range=5.5))
    restroom_metadata = json.loads((ROOT / "assets/environments/arcade/restroom/asset_metadata.json").read_text())["models"]
    compose_restroom(level, sizes, restroom_metadata)
    level["baseboards"] = baseboard_profile(level) + tile_trim()
    return level


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    text = json.dumps(build_level(), indent=2, ensure_ascii=False)+"\n"
    if args.check:
        if not OUTPUT.exists() or OUTPUT.read_text() != text:
            raise SystemExit("Retro Arcade source is stale; run its generator")
        print("Retro Arcade source current")
    else:
        OUTPUT.write_text(text)
        print(OUTPUT)


if __name__ == "__main__":
    main()
