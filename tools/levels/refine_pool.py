#!/usr/bin/env python3
"""Apply the concept-derived static Pool dressing to the playable demo only.

Stable tags make the dressing idempotent. Water, ladders, entities and the
connected environments are retained byte-for-byte as parsed JSON values.
"""
import json
import math
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
TAG = "[pool-concept]"


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
    path=ROOT/"assets/levels/places_demo.json"
    level=json.loads(path.read_text())
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
    print('Refined Pool architecture and static dressing in places_demo.json')


if __name__=='__main__':main()
