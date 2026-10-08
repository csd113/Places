"""Deliberate static concept layers for the existing night route.

All positions are curated groups beside established traversal. No encounters,
room boundaries, sky or global lighting are owned by this module.
"""

def props():
    result=[]
    def put(name,x,z,y=0,yaw=0,scale=1,**extra):
        result.append(dict(id=f'night_concept_{name}_{len(result):03d}',model='outdoor:'+name,
                           x=x,z=z,y=y,rotation_degrees=yaw,scale=scale,**extra))
    # Foreground islands, mid-route verge groups, and a thicker destination edge.
    # Irregular islands replace the former continuous wall of high grass.
    groups=((1.6,-6,1),(7.7,-11,.95),(8.5,-22,1.1),(1.0,-28,.8),
            (7.8,-34,.9),(18.0,-38,1),(20.0,-49,1.1),(7.9,-60,.85),
            (17.5,-67,1),(1.6,-79,.9),(8,-85,1),(20,-87,1.1),
            (9.0,-89,.8),(18.4,-89,.9))
    for i,(x,z,s) in enumerate(groups):
        for j,(dx,dz,ss) in enumerate(((0,0,1),(.72,-.45,.7),(-.55,.7,.66))):
            put('bush_round' if (i+j)%3 else 'bush_low',round(x+dx,3),round(z+dz,3),
                yaw=(i*47+j*103)%360,scale=round(s*ss,3),occludes=False)
        put('showcase_boulder',x+.25,z+1.08,y=-.035,yaw=(i*31)%360,scale=.48+(i%3)*.17,
            solid=True,size=[1.1,.65,1.0])
    # Pale capped fence rhythms frame the front/destination; the central path
    # and all encounter motion remain open. Each 2m piece owns its end posts.
    for x,z,count in ((7.6,-5,4),(17,-79,4),(21,-15,3)):
        for i in range(count):put('fence_two_rail',x,z-i*2.08,yaw=90)
    for x,z in ((7.6,-3.8),(17,-77.8),(10.2,-89.1),(16.8,-89.1)):
        put('masonry_pier',x,z)
        put('lamp_fence',x,z,y=1.2,occludes=False,lights=[dict(shape='point',
            offset=[0,.30,.05],color=[1,.86,.68],intensity=.5,range=4,falloff='smooth')])
    # The added fixtures use the published fence-lamp emitter profile.
    put('porch_canopy',13.5,-90.68,y=2.20)
    for x in (12.07,14.93):put('porch_railing_straight',x,-90.68,y=0,yaw=90,scale=.68)
    # The reference includes a stump/campfire. A quiet seating pocket is off
    # the walking axis; it retains the fire model's bind pose without actions.
    put('campfire_static',20,-50,occludes=False,lights=[dict(shape='point',
        offset=[0,.50,0],color=[1,.58,.25],intensity=.6,range=5,falloff='smooth')])
    for x,z,yaw in ((18.8,-50.7,60),(20.7,-51.2,300),(21.3,-49.7,210)):
        put('showcase_stump_seat',x,z,yaw=yaw,solid=True,size=[.7,.45,.6])
    put('road_barrier',20,-81,yaw=0)
    # Static geology closes distant sightlines beyond the invisible containment.
    for x,z,yaw,scale in ((-5,-26,90,1),(-5,-59,90,1.15),(29,-37,90,1.05),(29,-77,90,1.1),(13,-108,0,1.4)):
        put('boundary_ridge',x,z,y=-1.0,yaw=yaw,scale=scale,occludes=False)
    return result
