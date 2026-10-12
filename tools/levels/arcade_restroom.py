"""A fourth connected commercial room; source geometry only, no runtime imagery."""
from __future__ import annotations

FLOOR = 'arcade:restroom_tile_01'
WALL = 'core:pool_tile_wall_01'
MIRROR = 'arcade:restroom_mirror_01'


def compose_restroom(level, sizes, metadata):
    # The shared west wall already encloses Arcade. Add a real passage without
    # moving its first cabinet, bowling lanes, accepted party or existing lights.
    shared = level['walls'][2]
    shared['faces'] = dict(west=WALL)
    shared['openings'] = [dict(kind='door', offset=2.1, width=1.4, height=2.45)]
    level['rooms'].append(dict(x=-6.6, z=0, width=6.6, depth=7.8, height=3.2,
        material=FLOOR, ceiling_material='home:ceiling_white_01'))
    level['walls'].extend([
        dict(x=-6.84,z=-.24,width=6.60,depth=.24,height=3.2,material=WALL),
        dict(x=-6.84,z=0,width=.24,depth=7.8,height=3.2,material=WALL),
        dict(x=-6.84,z=7.8,width=6.6,depth=.24,height=3.2,material=WALL,
            openings=[dict(kind='window',offset=x+6.84-.45,width=.9,
                           height=.842,sill=1.134,glass=MIRROR,solid=True)
                      for x in (-2.7,-1.1)]),
    ])

    def prop(name, identity, x, z, y=0, yaw=0, solid=False, size=None):
        model='arcade:restroom_'+name
        level['props'].append(dict(id='restroom_'+identity,model=model,x=x,y=y,z=z,
            rotation_degrees=yaw,size=size or sizes[model],solid=solid,occludes=True))

    def door(identity, x, z, width, height, yaw, y=0, direction='right'):
        level.setdefault('doors',[]).append(dict(id='restroom_'+identity,
            kind='restroom', x=x, y=y, z=z, width=width, height=height,
            thickness=.042, rotation_degrees=yaw, initial_state='closed',
            open_direction=direction, swing_degrees=90, open_speed_degrees=100,
            obstruction='reverse', components=[dict(component='interactable',
                prompt='Restroom door' if identity=='entry' else 'Stall door',reach=1.7)],
            bindings=[dict(on='interact',actions=[dict(action='toggle')])]))

    door('entry',-.12,3.5,1.4,2.40,90,direction='left')

    # Separate thin colliders preserve the interiors. Three broad stalls have
    # 1.2–1.3 m entrances; interactive leaves start closed and swing into the bays.
    for i,z in enumerate((3.15,5.40)):
        prop('stall_divider',f'divider_{i}',-5.33,z,solid=True)
    for i,(lo,hi,panel) in enumerate(((.20,3.10,1.50),(3.20,5.35,.90),(5.45,7.60,.90))):
        prop('stall_front_wide' if i==0 else 'stall_front',f'front_{i}',
             -4.02,lo+panel/2,yaw=90,solid=True)
        door(f'door_{i}',-4.02,round(lo+panel+.055,6),round(hi-lo-panel-.085,6),1.68,270,y=.15)
        prop('toilet',f'toilet_{i}',-6.19,(lo+hi)/2,yaw=90,solid=True,
             size=[sizes['arcade:restroom_toilet'][0],.474,sizes['arcade:restroom_toilet'][2]])
        # Rear bracket on the real north wall / preceding divider's south
        # face. The model is horizontally centred, including its paper roll.
        mounting_face=(0,3.171,5.421)[i]
        prop('paper_holder',f'paper_{i}',-5.80,round(mounting_face+.0545,6),y=.68)
    prop('grab_bar','wide_stall_bar',-6.552,2.1,y=.92,yaw=90)

    for i,x in enumerate((-2.7,-1.1)):
        deck=metadata['arcade:restroom_sink']['deck_height']
        prop('sink',f'sink_{i}',x,7.515,y=round(.83-deck,6),yaw=180,solid=True,
             size=[sizes['arcade:restroom_sink'][0],deck,sizes['arcade:restroom_sink'][2]])
        prop('mirror_frame',f'mirror_{i}',x,7.779,y=1.1,yaw=180)
    prop('soap_dispenser','soap',-1.9,7.738,y=1.03,yaw=180)

    for i,(x,z) in enumerate(((-4.5,1.5),(-4.5,4.5),(-1.5,2.5),(-2.7,6.5))):
        level['ceiling_lights'].append(dict(id=f'restroom_troffer_{i}',
            fixture='arcade:fluorescent_panel_01',x=x,z=z,align='none',
            brightness=.38,color=[.90,.95,1],emission=.8,range=4.3,
            enabled=True,switchable=True,switch_group='restroom',static_switch='prepared'))
    level['props'].append(dict(id='restroom_light_switch',model='home:wall_switch',
        x=-.291,y=1.2,z=3.92,rotation_degrees=270,size=sizes['home:wall_switch'],
        solid=False,occludes=False,
        components=[dict(component='interactable',prompt='Bathroom lights',reach=1.7),
                    dict(component='animation',clip='toggle',looped=False,playing=False)],
        bindings=[dict(on='interact',actions=[dict(action='toggle_animation',clip='toggle'),
                                           dict(action='toggle',target='restroom_troffer_0')])]))
    for x,z in ((-5.1,6.9),(-1.5,.9)):
        level['decals'].append(dict(material='core:decal_ceiling_vent_01',
            x=x,z=z,width=.54,height=.54,surface='ceiling',rotation_degrees=0))
    level.setdefault('thresholds',[]).append(dict(x=-.12,z=2.8,length=1.38,
        rotation_degrees=90,thickness=.08,height=.008,material='home:baseboard_white_01'))


def tile_trim():
    """Ordinary ceramic skirting on actual room faces, clear of the doorway."""
    runs=[(-6.6,0,6.36,0),(-.24,7.8,6.36,180),(-6.6,7.78,7.76,90),
          (-.24,.02,2.08,270),(-.24,3.5,4.28,270)]
    return [dict(x=x,z=z,length=length,rotation_degrees=yaw,height=.10,
                 thickness=.018,material=WALL) for x,z,length,yaw in runs]
