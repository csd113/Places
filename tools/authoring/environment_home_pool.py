#!/usr/bin/env python3
"""Explicit source authoring for the migrated Home and Pool collection maps.

This is a developer recipe, never a runtime asset generator or CI map bake.
Inputs must be the real editor-imported legacy sources. Import/autofix custody
and native authoring/Play receipts are recorded by the collection campaign.
Only geometry, placements and existing catalogue references are emitted here.
"""

from __future__ import annotations

import argparse
import copy
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CATALOG = {a["id"]: a for a in json.loads((ROOT / "assets/catalog.json").read_text())["assets"]}
PROFILES = json.loads((ROOT / "assets/gameplay.json").read_text())["assets"]


def room(x, z, width, depth, material, height=3.0, ceiling="home:ceiling_warm_01", **extra):
    return dict(x=x, z=z, width=width, depth=depth, height=height,
                material=material, ceiling_material=ceiling, **extra)


def wall(x, z, width, depth, material, openings=(), **extra):
    value = dict(x=x, z=z, width=width, depth=depth, material=material, **extra)
    if openings:
        value["openings"] = [dict(kind="passage", offset=o, width=w, height=h, sill=0.0)
                             for o, w, h in openings]
    return value


def prop(identity, model, x, z, y=0.0, rotation=0.0, *, movable=False,
         fixed=False, group=None, solid=None, **extra):
    entry = CATALOG[model]
    value = dict(id=identity, model=model, x=x, z=z, y=y,
                 rotation_degrees=rotation, size=entry["size"],
                 solid=entry.get("solid", False) if solid is None else solid)
    value.update(extra)
    if movable:
        profile = copy.deepcopy(PROFILES[model]["physics"])
        if group:
            profile["reset_group"] = group
        value["physics"] = profile
        value["solid"] = False
    elif fixed:
        profile = PROFILES.get(model, {}).get("physics")
        value["physics"] = copy.deepcopy(profile) if profile and profile["motion"] == "fixed" else {"motion": "fixed"}
    return value


def leaf(identity, x, z, *, width=1.2, rotation=0, material="home:door_white_01",
         frame="home:baseboard_white_01", prompt="Open door", direction="left", transition=None):
    action = {"action": "change_level", "level": transition} if transition else {"action": "toggle"}
    return dict(id=identity, x=x, z=z, width=width, height=2.2,
                thickness=.045, rotation_degrees=rotation, initial_state="closed",
                open_direction=direction, swing_degrees=90, obstruction="stop",
                material=material, frame_material=frame,
                components=[dict(component="interactable", prompt=prompt, reach=2.2)],
                bindings=[dict(on="interact", actions=[action])])


def hub_sign(x, z, surface, rotation=0):
    return dict(x=x, y=2.55, z=z, width=2.4, height=.6,
                material="collection:sign_hub", surface=surface, rotation_degrees=rotation)


def flush(identity, x, z, *, brightness=.42, color=(1.0, .9, .75), **extra):
    return dict(id=identity, fixture="home:ceiling_light_round", x=x, z=z,
                brightness=brightness, emission=.7, color=list(color), align="none", **extra)


def switch(identity, x, z, y, rotation, prompt, actions):
    return prop(identity, "home:wall_switch", x, z, y, rotation, solid=False,
                components=[dict(component="interactable", reach=2.0, prompt=prompt),
                            dict(component="animation", clip="toggle", looped=False, playing=False)],
                bindings=[dict(on="interact", cooldown_seconds=.25,
                               actions=[dict(action="toggle_animation", clip="toggle"), *actions])])


def home(source):
    """Complete composition replacement after the actual legacy editor import."""
    assert source["id"] == "home_showcase", "Home must derive from the imported legacy map"
    paint = "home:wallpaper_offwhite_01"
    pattern = "home:wallpaper_pattern_01"
    warm = "home:wall_paint_warm_01"
    d = dict(schema="places-level", format_version=1, id="home", name="Home — A Quiet Favour",
             author="Places", spawn=dict(x=7.0, z=10.2, yaw_degrees=0.0),
             defaults=dict(wall=paint, floor="home:hardwood_oak_warm_01", ceiling="home:ceiling_warm_01"),
             transitions=["hub"], scripts=["scripts/home.luau"],
             comment="Migrated through the editor from home_showcase; redesigned with accepted Home/Shared exports. Source-authoring recipe is explicit developer tooling.")
    d["rooms"] = [
        room(0, 0, 8, 7, "home:hardwood_oak_warm_01"),
        room(8, 0, 6, 7, "home:tile_home_01", ceiling="home:ceiling_plaster_warm_01"),
        room(0, 7, 6, 5, "home:carpet_cream_01"),
        room(6, 7, 2, 5, "home:hardwood_walnut_02"),
        room(8, 7, 6, 5, "home:hardwood_oak_warm_01"),
        room(-3, 0, 3, 6, "home:hardwood_walnut_02"),
        room(-9, 0, 6, 3, "home:hardwood_walnut_02"),
    ]
    d["walls"] = [
        wall(-.15, -.15, 14.3, .3, paint),
        wall(13.85, .15, .3, 11.7, warm),
        wall(-.15, 11.85, 14.3, .3, pattern, [(6.53, 1.24, 2.2)]),
        wall(-.15, .15, .3, 11.7, paint, [(3.45, 1.5, 2.3)]),
        wall(7.85, .15, .3, 11.7, warm, [(1.65, 1.6, 2.35), (8.05, 1.6, 2.35)]),
        wall(.15, 6.85, 13.7, .3, paint,
             [(1.83, 1.34, 2.2), (6.25, 1.2, 2.3), (9.75, 2.2, 2.35)],
             faces={"south": pattern}),
        wall(5.85, 7.15, .3, 4.7, pattern, [(1.85, 1.2, 2.2)]),
        # The service passage is L-shaped: the second leg cannot be seen from
        # the living-room opening. Following players keep the caretaker alive.
        wall(-9.15, -.15, 9, .3, warm),
        wall(-9.15, .15, .3, 2.7, warm),
        wall(-9.15, 2.85, 6, .3, warm, height=3),
        wall(-3.15, 3.15, .3, 2.7, warm, height=3),
        wall(-3.15, 5.85, 3, .3, warm, height=3),
        # Continuous tiled counter-height band, coalesced into the north wall.
        wall(8.2, -.15, 4.8, .3, "home:backsplash_01", y=.9, height=.6),
    ]
    d["doors"] = [
        leaf("return_hub_home", 6.4, 12, transition="hub", prompt="Return to the Hub"),
        leaf("home_bedroom_door", 2, 7, width=1.3, prompt="Bedroom", direction="left"),
    ]
    d["decals"] = [hub_sign(7, 11.842, "wall_north")]
    d["ceiling_lights"] = [
        flush("home_living_main", 4.5, 3.0, brightness=.5),
        flush("home_living_entry", 6.7, 5.5, brightness=.32),
        flush("home_kitchen_a", 9.8, 2.2, brightness=.5, color=(1, .96, .87)),
        flush("home_kitchen_b", 12.2, 5.0, brightness=.38, color=(1, .96, .87)),
        flush("home_bedroom_light", 3, 9.5, brightness=.43, switchable=True,
              switch_group="home_bedroom", static_switch="prepared"),
        flush("home_hall_light", 7, 9.5, brightness=.42),
        flush("home_dining_light", 11, 9.5, brightness=.5),
        flush("home_service_a", -1.5, 4.3, brightness=.24),
        flush("home_service_b", -1.5, 1.5, brightness=.2),
        flush("home_service_c", -5, 1.5, brightness=.16),
    ]
    p = d["props"] = []
    # Deliberately generous route clearance behind the seating composition.
    p += [prop("home_sofa", "home:sofa", 4.7, .9, fixed=True),
          prop("home_armchair", "home:armchair", 6.9, 2.1, rotation=270, fixed=True),
          prop("home_coffee_table", "home:coffee_table", 4.6, 2.7, fixed=True),
          prop("home_living_rug", "home:rug", 4.6, 2.7, solid=False),
          prop("home_tv_console", "home:tv_console", .65, 1.7, rotation=90, fixed=True),
          prop("home_television", "home:crt_tv", .65, 1.7, y=.55, rotation=90,
               movable=True, group="home_objects"),
          prop("home_living_lamp", "home:floor_lamp", 7.15, .8, fixed=True),
          prop("home_living_plant", "core:plant", .7, 6.1, fixed=True, solid=True),
          prop("home_living_books", "home:book_stack", 4.95, 2.7, y=.46,
               movable=True, group="home_objects"),
          prop("home_wrong_mug", "home:mug", 4.25, 2.7, y=.46,
               movable=True, group="home_objects")]
    # Cabinet doors face into the kitchen; fitted upper stock is non-solid.
    for index, x in enumerate([8.8, 9.4, 10.6, 11.8]):
        p.append(prop(f"home_base_{index}", "home:cabinet_base", x, .5, fixed=True))
    p += [prop("home_kitchen_sink", "home:sink", 10, .5, fixed=True, size=[.6, .9, .6]),
          prop("home_kitchen_stove", "home:stove", 11.2, .5, fixed=True),
          prop("home_fridge", "core:fridge", 12.7, .6, fixed=True)]
    for index, x in enumerate([8.8, 9.4, 10, 10.6, 11.8]):
        p.append(prop(f"home_upper_{index}", "home:cabinet_wall", x, .325, y=1.5, solid=False))
        p.append(prop(f"home_strip_{index}", "home:cabinet_strip", x, .4, y=1.47,
                      solid=False, lights=[dict(shape="line", offset=[0, -.01, .025],
                                               length=.5, color=[1, .84, .59],
                                               intensity=.12, range=2.0)]))
    p += [prop("home_kettle", "home:kettle", 8.8, .55, y=.9,
               movable=True, group="home_objects"),
          prop("home_toaster", "home:toaster", 9.4, .55, y=.9,
               movable=True, group="home_objects"),
          prop("home_kitchen_plant", "home:plant_table", 11.8, .55, y=.9,
               movable=True, group="home_objects"),
          prop("home_island_a", "home:cabinet_base", 10.5, 3.7, rotation=180, fixed=True),
          prop("home_island_b", "home:cabinet_base", 11.1, 3.7, rotation=180, fixed=True),
          prop("home_island_bowl", "home:bowl", 10.5, 3.7, y=.9,
               movable=True, group="home_objects")]
    p += [prop("home_dining_table", "home:dining_table", 11, 9.6, fixed=True),
          prop("home_dining_chair_a", "home:dining_chair_refined", 11, 8.8, fixed=True),
          prop("home_dining_chair_b", "home:dining_chair_refined", 11, 10.4, rotation=180, fixed=True),
          prop("home_dining_chair_c", "home:dining_chair_refined", 9.9, 9.6, rotation=90, fixed=True),
          prop("home_dining_chair_d", "home:dining_chair_refined", 12.1, 9.6, rotation=270, fixed=True),
          prop("home_dining_plate_a", "home:plate", 11, 9.35, y=.75,
               movable=True, group="home_objects"),
          prop("home_dining_plate_b", "home:plate", 11, 9.85, y=.75,
               movable=True, group="home_objects"),
          prop("home_dining_spoon", "home:spoon", 11.3, 9.35, y=.75,
               movable=True, group="home_objects"),
          prop("home_dining_mug", "home:mug", 10.6, 9.35, y=.75,
               movable=True, group="home_objects"),
          prop("home_dining_bookshelf", "home:bookshelf", 13.6, 10.7, rotation=270, fixed=True),
          prop("home_dining_plant", "core:plant", 13.25, 7.7, fixed=True, solid=True)]
    p += [prop("home_bed", "core:bed", 1.9, 10.2, fixed=True),
          prop("home_bedside_table", "home:coffee_table", 4.25, 8.2, fixed=True),
          prop("home_bedroom_lamp", "home:floor_lamp", .7, 8.2, fixed=True),
          prop("home_cushion_chair", "home:dining_chair_refined", 4.8, 10.6, rotation=180,
               fixed=True, size=[.5, .471, .49],
               comment="Standable seat mass ends at measured seat top Y.471; the tall thin back stays rendered without trapping the cushion in a full-height box."),
          prop("home_requested_cushion", "home:cushion", 4.8, 10.6, y=.471,
               movable=True, group="home_objects", display_name="Checked cushion"),
          prop("home_bedroom_books", "home:book_stack", 4.25, 8.2, y=.46,
               movable=True, group="home_objects"),
          prop("home_hall_frame", "home:landscape_frame", 7.83, 10.7, y=1.35, rotation=270, solid=False),
          prop("home_bedroom_frame", "home:landscape_frame", 3.8, 11.83, y=1.35, rotation=180, solid=False),
          prop("home_service_box", "core:cardboard_box", -8.3, .65,
               movable=True, group="home_objects")]
    p += [switch("home_bedroom_switch", 5.82, 8.2, 1.2, 270, "Bedroom light",
                 [dict(action="toggle", target="home_bedroom_light")]),
          switch("home_restore", 6.17, 10.9, 1.2, 90, "Reset this visit",
                 [dict(action="run_script", script="scripts/home.luau", handler="reset")])]
    # This is an ordinary scripted NPC using the shared handoff capability.
    p.append(prop("home_caretaker", "mannequin", 5.5, 4.3, rotation=0,
                  solid=False, display_name="The caretaker",
                  components=[dict(component="interactable", prompt="Speak to the caretaker", reach=2.5),
                              dict(component="nav_agent", radius=.25, height=1.8,
                                   speed_mps=.75, step_height=.3, max_slope=2),
                              dict(component="ai", behavior="idler", walk_speed=.75,
                                   run_speed=.75, sight_range=0, hearing_range=0),
                              dict(component="carry", item="home_requested_cushion", socket="hand.R",
                                   offset=[-.1292, 0, -.0059], rotation_degrees=[0, 0, 0],
                                   idle_clip="carry_idle", walk_clip="carry_walk")],
                  bindings=[dict(on="interact", cooldown_seconds=.25,
                                 actions=[dict(action="run_script", script="scripts/home.luau", handler="talk")])]))
    return d


def pool(source):
    """Keep the imported main hall; add a second actual basin and utility room."""
    assert source["id"] == "pool_showcase", "Pool must derive from the imported legacy map"
    d = copy.deepcopy(source)
    d.update(id="pool", name="Pool — After Hours", transitions=["hub"],
             spawn=dict(x=2.7, z=2.2, yaw_degrees=140.0),
             comment="Editor-imported Pool hall retained; second swimming room and separate janitor closet added with current terrain and physics facilities.")
    # The legacy checker confirms a 0.0575 m² overlap where these two coping
    # strips turn around the shallow shelf. Import autofix correctly offers no
    # architectural guess. Trim this one run to its neighbour's actual face.
    shelf_coping = d["thresholds"][2]
    assert abs(shelf_coping["x"] - 6.4375) < .001 and abs(shelf_coping["length"] - 5.119) < .001
    shelf_coping.update(x=6.3155, length=4.875)
    shelf_coping["comment"] = "Reviewed manual import correction: terminate the horizontal coping at x8.753, leaving a 2mm construction joint before the vertical run's west face x8.755; removes the old double top and coincident caps."
    # The old shared shell already encloses the hall and changing annex.
    d["walls"][0].setdefault("openings", []).append(dict(kind="door", offset=1.13, width=1.24, height=2.2))
    d["walls"][1].setdefault("openings", []).append(dict(kind="door", offset=2.33, width=1.44, height=2.2))
    d["rooms"] += [room(0, 11, 16, 11, "core:pool_tile_deck_01", height=4,
                         ceiling="core:pool_ceiling_01"),
                   room(16, 11, 5, 5, "home:tile_home_01", height=3,
                        ceiling="home:ceiling_plaster_01")]
    tile = "core:pool_tile_wall_01"
    d["walls"] += [
        wall(-.15, 11.15, .3, 10.7, tile, height=4),
        wall(-.15, 21.85, 16.3, .3, tile, height=4),
        wall(15.85, 11.15, .3, 10.7, tile, [(1.53, 1.44, 2.2)], height=4),
        wall(16.15, 10.85, 5, .3, "home:wall_paint_offwhite_01", height=3),
        wall(16.15, 15.85, 5, .3, "home:wall_paint_offwhite_01", height=3),
        wall(20.85, 11.15, .3, 4.7, "home:wall_paint_offwhite_01", height=3),
    ]
    d.setdefault("doors", []).extend([
        leaf("return_hub_pool", 1, 0, prompt="Return to the Hub", transition="hub"),
        leaf("pool_second_room_door", 2.2, 11, width=1.4,
             prompt="Quiet pool room", direction="right"),
        leaf("pool_closet_door", 16, 14.1, width=1.4, rotation=90,
             prompt="Janitor closet", direction="left",
             material="core:pool_tile_wall_01", frame="core:metal_brushed_01"),
    ])
    d["decals"].append(hub_sign(1.6, .158, "wall_south"))
    # A second, independently enclosed pool, not a decorative water sheet.
    d["floor_regions"].append(dict(x=4, z=14, width=8, depth=4.5, offset_y=-1.2,
                                    material="core:pool_tile_basin_01", edge_material=tile))
    d.setdefault("ramps", []).append(dict(x=4, z=18.5, width=1.4, depth=2.5,
                                         offset_y=-1.2, rise=1.2,
                                         material="core:pool_tile_basin_01", edge_material=tile))
    d["water"] += [dict(shape="rect", x=4, z=14, width=8, depth=4.5,
                        surface_y=-.15, bottom_y=-1.2, material="core:water_pool_01",
                        opacity=.42, attenuation_per_metre=.16, swimming=True),
                   dict(shape="rect", x=4, z=18.5, width=1.4, depth=2.1875,
                        surface_y=-.15, bottom_y=-1.2, material="core:water_pool_01",
                        opacity=.42, attenuation_per_metre=.16, swimming=True)]
    # Three-sided low coping leaves the sloped entry entirely open.
    for x, z, length, rotation in [(8, 13.875, 8.24, 0), (3.875, 16.25, 4.5, 90),
                                    (12.125, 16.25, 4.5, 90), (8.75, 18.625, 6.5, 0)]:
        d["thresholds"].append(dict(x=x, z=z, length=length, rotation_degrees=rotation,
                                    y=0, height=.035, thickness=.24, material="pool:coping_01"))
    for index, (x, z) in enumerate([(3, 12.5), (8, 12.5), (13, 12.5),
                                    (3, 20.5), (8, 20.5), (13, 20.5)]):
        d["ceiling_lights"].append(dict(id=f"pool_second_light_{index}", fixture="core:pool_light_round",
                                          x=x, z=z, brightness=.65, color=[.74, .9, 1], align="none"))
    d["ceiling_lights"].append(dict(id="pool_closet_light", fixture="core:fluorescent_panel_01",
                                      x=18.5, z=13.5, y=2.998, brightness=.38, emission=.65,
                                      range=5, align="none", switchable=True,
                                      switch_group="pool_service", static_switch="prepared"))
    for index, identity in [(7, "pool_changing_light_a"), (8, "pool_changing_light_b")]:
        d["ceiling_lights"][index].update(id=identity, switchable=True,
                                          switch_group="pool_service", static_switch="prepared")
    p = d["props"]
    # The old decorative service-door slab overlapped the real return opening.
    p[:] = [v for v in p if v["model"] != "pool:pool_service_door"]
    for index, v in enumerate(p):
        v.setdefault("id", f"pool_imported_{index}")
        model = v["model"]
        if model in ("core:crate", "core:cardboard_box"):
            v["physics"] = copy.deepcopy(PROFILES[model]["physics"])
            v["physics"]["reset_group"] = "pool_water_demo"
            v["solid"] = False
        elif model == "pool:pool_bench":
            v["solid"] = True
            v["physics"] = {"motion": "fixed"}
        elif not v.get("physics") and v.get("solid"):
            v["physics"] = {"motion": "fixed"}
        if v.get("id") == "pool_physics_duck":
            # Real metre-scale duck rests on a dry table until picked up.
            v.update(x=13.7, z=17.6, y=.74, scale=1.0, size=[.1, .12, .145])
            v["physics"] = copy.deepcopy(PROFILES["core:rubber_duck"]["physics"])
            v["physics"]["reset_group"] = "pool_water_demo"
            v["comment"] = "Accepted duck; physical pickup and buoyancy, dormant on a dry table at load."
        if v.get("id") == "pool_water_demo_control":
            v["components"] = [c for c in v["components"] if c["component"] != "state"]
            for c in v["components"]:
                if c["component"] == "interactable":
                    c["prompt"] = "Restore pool objects"
            v["bindings"] = [dict(on="interact", cooldown_seconds=.25,
                                  actions=[dict(action="reset_physics_group", group="pool_water_demo")])]
    p += [prop("pool_second_duck_table", "core:pool_table", 13.7, 17.6, fixed=True),
          prop("pool_second_bench", "pool:pool_bench", 1.4, 16.2, rotation=90, fixed=True),
          prop("pool_second_plant", "pool:pool_potted_plant", 14.6, 20.5,
               fixed=True, solid=True),
          prop("pool_second_notice", "pool:pool_safety_sign", 8, 21.835,
               y=1.35, rotation=180, solid=False)]
    p += [prop("pool_changing_bench", "pool:pool_bench", 22, 6.8, rotation=180, fixed=True),
          prop("pool_changing_towel", "janitor:folded_towel_blue", 22, 6.8, y=.45,
               movable=True, group="pool_supplies"),
          switch("pool_changing_switch", 19.67, 6.7, 1.2, 90,
                 "Service lights", [dict(action="toggle", target="pool_changing_light_a")])]
    # Reuse the accepted closet's exact shelf/load poses and profiles.
    closet = json.loads((ROOT / "projects/janitor-supplies/levels/janitor_physics.json").read_text())
    for original in closet["props"]:
        if original["model"] == "home:wall_switch":
            continue
        v = copy.deepcopy(original)
        v["id"] = "pool_closet_" + original["id"]
        v["x"] += 16.5
        v["z"] += 11.5
        if v["model"] == "janitor:cardboard_supply_box":
            # Keep the reusable water-safe portable profile rather than the
            # dry preview's historical implicit full-box displacement.
            v["physics"] = copy.deepcopy(PROFILES[v["model"]]["physics"])
        if v.get("physics", {}).get("motion") in ("dynamic", "kinematic"):
            v["physics"]["reset_group"] = "pool_supplies"
        p.append(v)
    p += [switch("pool_supply_restore", 17, 15.83, 1.2, 180,
                 "Restore supplies", [dict(action="reset_physics_group", group="pool_supplies")]),
          switch("pool_closet_light_switch", 16.17, 12.0, 1.2, 90,
                 "Service lights", [dict(action="toggle", target="pool_closet_light")])]
    return d


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--imported-home", type=Path, required=True)
    parser.add_argument("--imported-pool", type=Path, required=True)
    parser.add_argument("--out", type=Path, default=ROOT / "projects/environments/levels")
    args = parser.parse_args()
    inputs = [json.loads(args.imported_home.read_text()), json.loads(args.imported_pool.read_text())]
    args.out.mkdir(parents=True, exist_ok=True)
    for filename, value in [("home.json", home(inputs[0])), ("pool.json", pool(inputs[1]))]:
        path = args.out / filename
        path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n")
        print(path)


if __name__ == "__main__":
    main()
