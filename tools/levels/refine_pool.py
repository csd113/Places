#!/usr/bin/env python3
"""Refine the active Pool Showcase or an explicit historical Demo working copy.

The standalone showcase restores its basin water and authors a reusable physical
float/sink demonstration. Historical Demo dressing preserves water, ladders,
entities and connected environments as parsed JSON values. Stable tags make both
paths idempotent; frozen archive files are never overwritten.
"""
import argparse
import json
import math
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
TAG = "[pool-concept]"
SHOWCASE_TAG = "[pool-water-demo]"


def polish_showcase(level):
    """Keep the existing depths, artwork and light strengths; finish their use."""
    for key in ("props", "thresholds", "decals", "water", "ladders"):
        level[key] = [piece for piece in level.get(key, [])
                      if not piece.get("comment", "").startswith(SHOWCASE_TAG)]
    level["spawn"] = dict(x=3.0, z=9.0, yaw_degrees=56.0)

    def append(key, note, **piece):
        piece["comment"] = SHOWCASE_TAG + " " + note
        level[key].append(piece)

    # Water describes the real, unchanged tiled recesses. The main basin is
    # 1.35 m deep: an upright player wades and deliberate crouch swims. The
    # adjacent shelf remains 20 cm deep and supports the ordinary rim pull-up.
    for x, z, width, depth, bottom, note in (
            (4.0, 3.0, 9.0, 4.5, -1.5, "Main basin: 1.35 m of standing-depth water."),
            (9.0, 7.5, 4.0, .9, -.35, "Walk-in shelf: 20 cm of shallow water.")):
        append("water", note, shape="rect", x=x, z=z, width=width, depth=depth,
               surface_y=-.15, bottom_y=bottom, material="core:water_pool_01",
               opacity=.42, attenuation_per_metre=.16, swimming=True)

    # Match the existing closed ladder's floor-to-deck rise and east-facing
    # return flanges. A generic solid bounds box would obstruct its climb.
    ladder = next(prop for prop in level["props"] if prop["model"] == "core:pool_ladder")
    ladder.update(x=12.805, solid=False)
    # Item-only metal collision follows the open rails and treads. The player
    # uses the climb volume instead of an impassable ladder bounds cuboid.
    parts = []
    for rail_x in (-.229, .229):
        for rail_y, rail_z, half_height in ((.9825, -.19, .9525), (1.7175, .19, .2175)):
            parts.append(dict(offset=[rail_x, rail_y, rail_z],
                              shape=dict(kind="cylinder", half_height=half_height, radius=.024)))
    for rung_y in (.35, .655, .96, 1.265):
        parts.append(dict(offset=[0, rung_y, -.18],
                          shape=dict(kind="cuboid", half_extents=[.2275, .014, .0525])))
    ladder["physics"] = dict(motion="fixed", shape=dict(kind="compound", parts=parts))
    # Sizes are model-local, then rotated by the shared collision resolver.
    # These west rail boxes previously stored already-rotated dimensions and
    # obstructed the dry deck across the rail instead of following its length.
    for rail in level["props"]:
        if rail["model"] == "core:pool_guardrail_straight":
            rail["size"] = [2.0, 1.05, .08]
        elif rail["model"] == "core:pool_guardrail_end":
            rail["size"] = [.6, 1.05, .08]
    append("ladders", "Climb towards the east deck through the real ladder rails.",
           x=12.35, z=4.95, width=.6, depth=.6, bottom_y=-1.5, top_y=0.0,
           facing_degrees=90.0)

    # Caps sit wholly on dry supporting deck. Closed ends retain small ceramic
    # joints; the east run leaves a clear landing around the ladder flanges.
    for x, z, length, yaw in (
            (8.5, 2.875, 9.244, 0), (3.875, 5.25, 4.5, 90),
            (6.4375, 7.625, 5.119, 0), (8.875, 7.95, .896, 90),
            (11.0, 8.525, 4.244, 0), (13.125, 3.9575, 1.919, 90),
            (13.125, 6.9925, 2.819, 90)):
        append("thresholds", "35 mm coping on dry deck; ladder landing and traversal remain clear.",
               x=x, z=z, length=length, rotation_degrees=yaw, y=0.0,
               thickness=.24, height=.035, material="pool:coping_01")

    drain = next(prop for prop in level["props"] if prop["model"] == "pool:pool_drain")
    drain.update(x=4.3, z=2.53)
    for index in range(1, 15):
        append("props", "Existing 60 cm overflow grate with 2 mm construction joint.",
               model="pool:pool_drain", x=round(4.3 + index * .602, 3), z=2.53,
               size=[.6, .014, .16], solid=False)
    for z in (4.45, 6.05):
        append("decals", "Existing navy basin lane sheet at its exact 32:1 fitted aspect.",
               x=8.5, z=z, y=-1.5, width=8.0, height=.25,
               material="pool:decal_lane_01", surface="floor")

    # Reuse the table as the visible E control, on a dry observation position.
    # Snapshot condition evaluation means one press runs one branch only.
    table = next(prop for prop in level["props"] if prop["model"] == "core:pool_table")
    table.update(id="pool_water_demo_control", display_name="Water physics demo",
                 x=11.3, z=9.1,
                 components=[dict(component="interactable", reach=2.5,
                                  prompt="Run water demo: duck floats, metal sinks"),
                             dict(component="state", name="running", value=False)],
                 bindings=[])
    for running, action, prompt in (
            (False, "activate_physics_group", "Reset water demo"),
            (True, "reset_physics_group", "Run water demo: duck floats, metal sinks")):
        table["bindings"].append(dict(
            on="interact", cooldown_seconds=.25,
            when=[dict(check="state", target="pool_water_demo_control",
                       name="running", equals=running)],
            actions=[dict(action=action, group="pool_water_demo"),
                     dict(action="set_state", name="running", value=not running),
                     dict(action="set_prompt", prompt=prompt)]))
    # Keep the existing seating clear of the hot-tub rim and the control aim.
    chairs = [prop for prop in level["props"] if prop["model"] == "core:pool_chair"]
    for chair, x, yaw in zip(chairs, (10.3, 12.3), (90.0, 270.0)):
        chair.update(x=x, z=9.1, rotation_degrees=yaw, size=[.55, .85, .52])

    # Authored bodies begin dormant, suspended a little above the water. E or
    # pickup starts physical motion; reset returns exactly to this staging.
    # Explicit source displacement scales by scale^3; mass stays total kg.
    common = dict(motion="dynamic", pickup=True, friction=.5, restitution=.02,
                  linear_damping=.1, angular_damping=.2, reset_group="pool_water_demo")
    append("props", "Enlarged existing rubber duck: physical buoyancy, never analytic float.",
           id="pool_physics_duck", model="core:rubber_duck", x=10.6, z=6.7,
           y=1.56, scale=2.5, rotation_degrees=180.0, size=[.1, .12, .145],
           solid=False, physics=dict(common,
               shape=dict(kind="cuboid", half_extents=[.05, .06, .0725]), mass_kg=1.0,
               water=dict(displaced_volume_m3=.00065, linear_drag=4.0, angular_drag=2.5)))
    append("props", "Dense perforated metal basket: metal displaces water, its open envelope does not.",
           id="pool_physics_sink", model="core:washer_drum", x=11.5, z=6.7,
           y=1.56, size=[.42, .3, .42], solid=False, physics=dict(common,
               shape=dict(kind="cuboid", half_extents=[.21, .15, .21]), mass_kg=6.4,
               water=dict(displaced_volume_m3=.0008, linear_drag=4.0, angular_drag=2.5)))


def add(level, key, **piece):
    piece["comment"] = TAG + " " + piece.get("comment", "")
    level.setdefault(key, []).append(piece)


def prop(level, model, x, z, size, yaw=0, y=0, solid=False, note=""):
    add(level,"props",model=model,x=x,z=z,y=y,rotation_degrees=yaw,size=size,
        solid=solid,comment=note)


def board(level, x, z, length, yaw, y, height, material, thickness=.018):
    add(level,"baseboards",x=x,z=z,length=length,rotation_degrees=yaw,y=y,
        height=height,thickness=thickness,material=material,
        comment="Surface-mounted ceramic/metal construction trim; no collision.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, required=True,
                        help="Pool Showcase source or an explicit historical Demo working copy")
    parser.add_argument("--out", type=Path, required=True,
                        help="Working output JSON; frozen archive sources are never overwritten")
    args = parser.parse_args()
    path = args.out.resolve()
    if path.is_relative_to(ROOT / "docs/archive"):
        parser.error("Preserve frozen historical sources; choose a working output outside docs/archive")
    level=json.loads(args.source.read_text())
    if level.get("id") == "pool_showcase":
        polish_showcase(level)
        path.write_text(json.dumps(level, indent=2) + "\n")
        print(f"Refined Pool Showcase water, physical demo and dry construction details in {path}")
        return
    for key in ("props","baseboards","thresholds","columns","floor_regions","decals"):
        level[key]=[p for p in level.get(key,[]) if not p.get("comment","").startswith(TAG)]
    level['walls']=[w for w in level['walls'] if not w.get('comment','').startswith(TAG)]
    # Replace the two oversized placeholder notice panels with real service
    # leaves. They carried no bindings/openings or object identity.
    level['walls']=[w for w in level['walls'] if not (
        w.get('comment','').startswith('Pool notice board:') or
        w.get('comment','').startswith('Second pool notice board,') or
        (w.get('material')=='core:metal_brushed_01' and
         ((25.7<w.get('x',0)<25.8 and 7.1<w.get('z',0)<9.1) or
          (5.0<w.get('x',0)<6.9 and 7.1<w.get('z',0)<7.2))))]
    for region in level['floor_regions']:
        if region.get('material')=='core:pool_tile_basin_01' and region['x']==8.0:
            region['edge_material']='core:pool_tile_basin_01'
    for decal in level['decals']:
        if (decal.get('material') == 'core:decal_no_diving_01'
                and decal.get('surface') == 'floor' and decal.get('x') == 10.5):
            decal['z'] = 8.9  # Keep the fitted sign clear of the overflow grate.
        if (decal.get('material') == 'core:decal_no_diving_01'
                and decal.get('surface') == 'wall_south' and decal.get('x') == 14.5):
            decal['y'] = .3  # Clear the ceramic band below the safety sign.
    for light in level['ceiling_lights']:
        if light['fixture'] in ('core:pool_light_round','core:pool_light_wall'):
            light['color']=[.82,.90,1.0]
            light['emission']=.65
            if light.get('mount')=='wall':
                light['y']=.65
    # Cap the complete notched perimeter, on the dry side of each recess.
    # 2–4 mm construction joints expose the real deck under the caps, keeping
    # every closed end separate and all footprint samples clear of the recess.
    caps=[(14,9.863,12.544,0),(7.863,13,5.996,90),(20.137,13,5.996,90),
          (8.727,16.137,1.998,0),(18.273,16.137,3.998,0),
          (9.863,16.451,.898,90),(16.137,16.451,.898,90),(13,17.037,6.544,0)]
    for x,z,length,yaw in caps:
        add(level,'thresholds',x=x,z=z,length=length,rotation_degrees=yaw,y=-1.5,
            thickness=.27,height=.045,material='pool:coping_01',
            comment='45 mm coping cap entirely over the dry deck; existing rim traversal unchanged.')
    # Five submerged 300 mm risers on the south-east side, in addition to the
    # existing shallow walk-in shelf. Region precedence clips the basin floor.
    for i in range(5):
        add(level,'floor_regions',x=17.6,z=14.5+i*.3,width=2.,depth=.3,
            offset_y=round(-1.2+i*.3,3),material='core:pool_tile_basin_01',
            edge_material='core:pool_tile_basin_01',
            comment='Blue ceramic submerged walk-in tread, 300 mm rise, closed skirt.')
    # Continuous restrained blue wainscot band and ceiling shadow line.
    for x,z,length,yaw in [(25.85,18.85,25.7,180),(.15,7.15,18.7,0),
                           (.15,18.85,11.7,90),(25.85,15.15,3.7,270)]:
        board(level,x,z,length,yaw,-.40,.16,'pool:band_01',.016)
        board(level,x,z,length,yaw,2.48,.16,'pool:metal_01',.025)
    # Pool-side window casing: expose the reveal thickness and a proud sill,
    # while preserving all office-facing surfaces, glazing and openings.
    for wall in level['walls']:
        if wall.get('material')!='core:pool_tile_wall_01' or wall.get('z')!=6.85:
            continue
        for opening in wall.get('openings',[]):
            if opening['kind']!='window':continue
            left=wall['x']+opening['offset']; width=opening['width']
            bottom=wall['y']+opening.get('sill',0); height=opening['height']
            # Surface-mounted, collision-free trim preserves the narrow hot
            # tub exit below these windows. Ends stop at the jamb strips.
            for wx,wy,ww,wh in [(left-.045,bottom,.045,height),
                                (left+width,bottom,.045,height),
                                (left-.045,bottom+height+.002,width+.09,.073),
                                (left-.045,bottom-.055,width+.09,.053)]:
                segments=math.ceil(wh)
                for segment in range(segments):
                    height=wh/segments
                    board(level,wx,7.15,ww,0,wy+segment*height,
                          height-(.002 if segment+1<segments else 0),
                          'pool:metal_01',.04)
    for x,z in [(8.83,7.16),(18.80,7.16),(22.95,18.48)]:
        add(level,'columns',x=x,z=z,width=.28,depth=.30,height=4.2,y=-1.5,
            material='core:pool_tile_basin_01',comment='Slim full-height blue ceramic pilaster.')
    prop(level,'pool:pool_service_door',6.,7.245,[1.,2.1,.15],note='Blue utility leaf replaces the blank notice-board placeholder.')
    prop(level,'pool:pool_service_door',25.755,8.2,[.15,2.1,1.],yaw=270,
         note='Closed service leaf behind changing screens; no interactive function removed.')
    for x,z,yaw in [(22.1,7.70,0),(24.85,17.6,90),(4.,18.20,0)]:
        prop(level,'pool:pool_bench',x,z,[.38,.45,1.6] if yaw==90 else [1.6,.45,.38],yaw,
             solid=True,note='Reference four-slat bench: one at the changing screens and two at the dry perimeter.')
    # Existing file-backed plant is the same silhouette shown by the concept.
    prop(level,'core:plant',2.8,17.5,[.4,.85,.4],note='Existing potted foliage beside resin seating, as in the concept; shared artwork retained.')
    for x in (8.5,9.2,10.0,10.7,11.4,12.1,12.8,13.5,14.2,14.9,15.6,16.3,17.,17.7,18.4,19.1):
        prop(level,'pool:pool_drain',x,9.54,[.6,.014,.16],note='Recessed deck overflow grate outside the coping, non-colliding.')
    for z in (11.6,14.7):
        add(level,'decals',x=13.4,z=z,y=-3.,width=8.,height=.25,
            material='pool:decal_lane_01',surface='floor',
            comment='32:1 navy basin lane, exact source aspect; the decal snaps to the basin floor.')
    path.write_text(json.dumps(level,indent=2)+'\n')
    print(f'Refined historical Pool architecture and static dressing in {path}')


if __name__=='__main__':main()
