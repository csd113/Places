"""Closed Outdoors concept construction, loading committed fitted PNGs.

Foliage uses opaque faceted lobes: no layered leaf-card overdraw. Old model
ids keep their placement origins, lamp anchors and modular join conventions.
"""
from pathlib import Path
import math

from parts.refreshed import load_atlas_from, orient_outward, solid_box, solid_cylinder

ROOT=Path(__file__).resolve().parents[3]/'assets/environment/outdoor/props/models'
WHITE=(255,255,255)


def atlas(p):
    p.ao_strength=0
    return load_atlas_from(p,ROOT/(p.id.split(':')[1]+'.png'),('main','stock','foliage','detail'))


def box(p,center,size,uv,color=WHITE):
    solid_box(p,center,size,uv=uv,color=color,shade=False)


def rings(p,levels,uv,color=WHITE):
    start=len(p.mesh.indices)
    n=len(levels[0])
    for a,b in zip(levels,levels[1:]):
        for i in range(n):
            j=(i+1)%n
            p.mesh.quad(a[i],a[j],b[j],b[i],uv=uv,color=color)
    for a in (levels[0],levels[-1]):
        c=tuple(sum(x[k] for x in a)/n for k in range(3))
        for i in range(n):
            p.mesh.triangle(c,a[i],a[(i+1)%n],uvs=[((uv[0]+uv[2])/2,(uv[1]+uv[3])/2),(uv[0],uv[1]),(uv[2],uv[3])],color=color)
    center=tuple(sum(x[k] for a in levels for x in a)/(n*len(levels)) for k in range(3))
    orient_outward(p.mesh,start,center)


def fit(p):
    lo,hi=p.mesh.bounds()
    p.mesh.positions=[((q[0]-(lo[0]+hi[0])/2)*p.width/(hi[0]-lo[0]),(q[1]-lo[1])*p.height/(hi[1]-lo[1]),(q[2]-(lo[2]+hi[2])/2)*p.depth/(hi[2]-lo[2])) for q in p.mesh.positions]


def lobe(p,center,size,uv,seed,color=WHITE):
    levels=[]
    for j,(y,r) in enumerate(((-.5,.38),(.04,1),(.5,.30))):
        a=[]
        for i in range(6):
            theta=math.tau*i/6
            ripple=1+.14*math.sin(i*2.1+seed+j*.7)
            a.append((center[0]+math.cos(theta)*r*ripple*size[0]/2,center[1]+(y+.035*math.sin(i*1.9+seed))*size[1],center[2]+math.sin(theta)*r*ripple*size[2]/2))
        levels.append(a)
    rings(p,levels,uv,color)


def limb(p,a,b,ra,rb,uv):
    axis=[b[i]-a[i] for i in range(3)];length=math.sqrt(sum(x*x for x in axis));axis=[x/length for x in axis]
    ref=(1,0,0) if abs(axis[1])>.9 else (0,1,0)
    cross=lambda u,v:(u[1]*v[2]-u[2]*v[1],u[2]*v[0]-u[0]*v[2],u[0]*v[1]-u[1]*v[0])
    right=cross(axis,ref);length=math.sqrt(sum(x*x for x in right));right=[x/length for x in right];up=cross(axis,right)
    levels=[]
    for c,r in ((a,ra),(b,rb)):
        levels.append([tuple(c[k]+r*(right[k]*math.cos(math.tau*i/6)+up[k]*math.sin(math.tau*i/6)) for k in range(3)) for i in range(6)])
    rings(p,levels,uv)


def broad_tree(p,slim=False):
    t=atlas(p);bark=t.uv('main',inset=2)
    p.begin_material(p.material('bark'))
    trunk=[(0,0,0),(.06,.9,-.04),(-.04,2.1,.07),(.13,3.4,.0),(.18,5.0,.04)]
    radii=(.18,.12,.085,.055,.015) if slim else (.28,.21,.16,.105,.025)
    for i in range(4): limb(p,trunk[i],trunk[i+1],radii[i],radii[i+1],bark)
    if not slim:
        for i in range(4):
            a=math.tau*i/4+.3
            limb(p,(math.cos(a)*.05,.21+i*.011,math.sin(a)*.05),(math.cos(a)*.46,.015,math.sin(a)*.46),.14,.05,bark)
    clusters=[]
    for i in range(7 if slim else 6):
        a=math.tau*i/(7 if slim else 6)+.27
        y=(2.5+i*.38) if slim else (2.9+(i%3)*.75)
        reach=(.46+.09*(i%2)) if slim else (1.12+.25*(i%3))
        mid=(math.cos(a)*reach*.55,y-.5,math.sin(a)*reach*.55)
        tip=(math.cos(a)*reach,y+.14,math.sin(a)*reach)
        limb(p,(.03,(1.8+i*.027) if not slim else y-1.2,0),mid,.085 if not slim else .045,.05 if not slim else .025,bark)
        limb(p,mid,tip,.05 if not slim else .025,.016,bark)
        clusters.append((tip,(1.75,1.60,1.70) if not slim else (1.06,1.48,1.02)))
    clusters.extend([((.05,5.6,0),(1.65,1.7,1.6)),((-.3,4.35,.1),(1.8,1.8,1.9))] if not slim else [((.05,5.6,0),(.94,1.5,.92))])
    p.begin_material(p.material('foliage'))
    for i,(c,s) in enumerate(clusters): lobe(p,c,s,t.uv('foliage' if i%3 else 'detail',inset=2),i+4)
    fit(p)
    p.add_note('closed tapered limbs and distinct asymmetric faceted foliage masses; opaque leaves, no leaf-card overdraw; trunk collider unchanged')


def tree01(p): broad_tree(p)
def tree02(p): broad_tree(p,True)


def tree03(p):
    t=atlas(p);bark=t.uv('main',inset=2)
    p.begin_material(p.material('pine_bark'))
    limb(p,(0,0,0),(.06,6.55,.02),.15,.01,bark)
    p.begin_material(p.material('pine_boughs'))
    for j in range(6):
        y=.65+j*.88;radius=1.47-j*.20
        for i in range(3):
            a=math.tau*i/3+j*.69
            c=(math.cos(a)*radius*.65,y+.25,math.sin(a)*radius*.65)
            lobe(p,c,(radius*1.12,1.2,radius*.85),t.uv('foliage' if (i+j)%3 else 'detail',inset=2),j*7+i,(237,246,237))
    # A pointed crown closes the evergreen silhouette without stacked cones.
    lobe(p,(.04,6.10,0),(.45,1.3,.43),t.uv('foliage',inset=2),47)
    fit(p);p.add_note('six staggered whorls of drooping faceted boughs; opaque needles; no fringe cards')


def bush(p):
    t=atlas(p)
    for i,(c,s) in enumerate([((-.35,.35,0),(.85,.65,.78)),((.30,.46,.1),(1,.88,.8)),((.05,.57,-.2),(.83,.87,.82)),((-.1,.22,.35),(.72,.46,.65))]):
        lobe(p,c,s,t.uv('foliage' if i%2 else 'detail',inset=2),13+i)
    fit(p);p.add_note('four unequal foliage lobes, grounded lower skirt; no alpha overdraw')


def lantern_head(p,t,c,bottom,height,width,depth):
    metal=t.uv('main',inset=2);pane=t.uv('stock',inset=2)
    metal_slot=p.material('lantern_cast_metal');glass_slot=p.material('lantern_amber',emissive=(1,.83,.58),strength=1.15)
    p.begin_material(glass_slot)
    # Tapered luminous panes; stock and real seams frame each side.
    levels=[]
    for yy,s in ((bottom,.70),(bottom+height,1)):
        levels.append([(c[0]+x*width*s/2,yy,c[1]+z*depth*s/2) for x,z in ((-1,-1),(1,-1),(1,1),(-1,1))])
    rings(p,levels,pane)
    p.begin_material(metal_slot)
    for x,z in ((-1,-1),(1,-1),(1,1),(-1,1)):
        limb(p,(c[0]+x*width*.35,bottom,c[1]+z*depth*.35),(c[0]+x*width*.5,bottom+height,c[1]+z*depth*.5),.012,.012,metal)
    box(p,(c[0],bottom-.013,c[1]),(width*.78,.026,depth*.78),metal)
    rings(p,[[(c[0]+x*width*.67,bottom+height+.015,c[1]+z*depth*.67) for x,z in ((-1,-1),(1,-1),(1,1),(-1,1))],[(c[0]+x*.026,bottom+height+.11,c[1]+z*.026) for x,z in ((-1,-1),(1,-1),(1,1),(-1,1))]],metal)
    solid_cylinder(p,(c[0],bottom+height+.11,c[1]),.014,.035,segments=6,uv=metal,color=WHITE,shades=False)


def lamp(p):
    t=atlas(p);metal=t.uv('main',inset=2)
    name=p.id.split(':')[1]
    if name=='lamp_stand':
        box(p,(0,.025,0),(.24,.05,.24),metal)
        solid_cylinder(p,(0,.05,0),.034,.63,segments=8,taper=.8,uv=metal,color=WHITE,shades=False)
        lantern_head(p,t,(0,0),.70,.20,.25,.25)
    elif name=='lamp_fence':
        box(p,(0,.019,0),(.32,.038,.36),metal)
        lantern_head(p,t,(0,0),.065,.205,.235,.235)
    elif name=='lamp_wall':
        box(p,(0,.31,-.12),(.12,.42,.042),t.uv('foliage',inset=2))
        box(p,(0,.44,-.04),(.034,.034,.20),metal)
        limb(p,(0,.44,.03),(0,.335,.075),.014,.014,metal)
        lantern_head(p,t,(0,.035),.032,.21,.21,.19)
    else:
        box(p,(0,.08,-.29),(.34,.16,.34),metal)
        solid_cylinder(p,(0,.16,-.29),.049,5.6,segments=8,taper=.7,uv=metal,color=WHITE,shades=False)
        p.tube_path([(0,5.70,-.29),(0,6.18,-.29),(0,6.27,.02),(0,6.12,.38)],radii=[.035]*4,segments=6,uv=metal,color=WHITE,cap_start=True,cap_end=True)
        lantern_head(p,t,(0,.38),5.78,.34,.35,.32)
    fit(p);p.add_note('cast foot/collar, tapered four-pane lantern, standoff corner bars, pyramidal hood and finial; published light anchor retained')


def rock(p):
    t=atlas(p);name=p.id.split(':')[1];cliff='rock_face' in name;seed=31 if 'variant' in name else 13
    levels=[]
    count=9
    profile=((0,.93),(.18,1),(.60,.86),(1,.48)) if cliff else ((0,1),(.18,.97),(.60,.86),(1,.48))
    for j,(y,r) in enumerate(profile):
        a=[]
        for i in range(count):
            theta=math.tau*i/count
            radius=r*(1+.19*math.sin(i*1.8+seed+(j*.65 if cliff else 0)))
            # Homothetic boulder rings form deliberate planar shoulders rather
            # than warped quads with tiny grazing-light facets. Cliffs retain
            # their broken, uneven crowns.
            height=y+(0 if j==0 or not cliff else .095*math.sin(i*2.2+seed+j))
            a.append((math.cos(theta)*radius+(j*.06 if cliff else -.05*j),height,math.sin(theta)*radius))
        levels.append(a)
    rings(p,levels,t.uv('detail',inset=2));fit(p);p.add_note('closed asymmetric hewn rock, unequal shoulders and tapered broken crown; original footprint/origin retained')


def fence(p):
    t=atlas(p);uv=t.uv('stock',inset=2)
    for x in (-.92,.92):
        box(p,(x,.50,0),(.12,1,.12),uv)
        box(p,(x,1.025,0),(.16,.05,.16),uv)
        box(p,(x,.05,0),(.145,.10,.145),uv)
    for y in (.40,.79): box(p,(0,y,0),(1.78,.11,.065),uv)
    fit(p);p.add_note('two horizontal timber rails between capped pale newels, with separate feet; 2m module')


def pier(p):
    t=atlas(p);uv=t.uv('detail',inset=2)
    for row in range(5):
        for col in range(2):
            x=(col-.5)*.248
            box(p,(x,.13+row*.205,0),(.242,.198,.47),uv,(238-row*2,240-row*2,245-row*2))
    box(p,(0,.04,0),(.54,.08,.54),t.uv('main',inset=2))
    box(p,(0,1.14,0),(.58,.12,.58),t.uv('main',inset=2))
    fit(p);p.add_note('five masonry courses, inset mortar joints, foot and projecting concrete cap; top supports fence lamp')


def canopy(p):
    t=atlas(p);roof=t.uv('foliage',inset=2);trim=t.uv('stock',inset=2)
    # Closed mono-pitch porch cover: high at house (-Z), low at front (+Z).
    levels=[]
    for y in (0,.075): levels.append([(-1.7,y+.60,-.95),(1.7,y+.60,-.95),(1.7,y,.95),(-1.7,y,.95)])
    rings(p,levels,roof)
    box(p,(0,.02,.93),(3.4,.14,.04),trim)
    for x in (-1.45,1.45):
        limb(p,(x,-.015,.70),(x,.40,-.53),.045,.045,trim)
    fit(p);p.add_note('real closed pitched porch cover, pale fascia and diagonal knee supports; high edge faces local -Z')


def barrier(p):
    t=atlas(p)
    for x in (-1.5,1.5):
        box(p,(x,.53,0),(.16,1.06,.19),t.uv('stock',inset=2))
        box(p,(x,.05,0),(.36,.10,.42),t.uv('stock',inset=2))
    box(p,(0,.83,.05),(3.4,.18,.055),t.uv('main',inset=2))
    for x in (-1.45,1.45):box(p,(x,.83,.092),(.065,.27,.038),t.uv('detail',inset=2))
    fit(p);p.add_note('low striped timber barrier with hinge straps and stable feet; decorative dead-end placement')


def ridge(p):
    t=atlas(p)
    peaks=(.18,.46,.34,.91,.62,1,.40,.66,.29)
    levels=[]
    for z in (-2,2): levels.append([(i*2.5-10,peaks[i]*10,z) for i in range(9)])
    # A closed silhouette extrusion, no flat painted mountain/sky substitute.
    start=len(p.mesh.indices);uv=t.uv('detail',inset=2)
    for i in range(8):
        a,b=levels[0][i:i+2];c,d=levels[1][i:i+2]
        for x,y,z in ((a,b,d),(a,d,c),(a,(b[0],0,-2),b),(a,(a[0],0,-2),(b[0],0,-2)),(c,d,(d[0],0,2)),(c,(d[0],0,2),(c[0],0,2))):
            p.mesh.triangle(x,y,z,uvs=[(uv[0],uv[1]),(uv[2],uv[1]),(uv[2],uv[3])],color=(117,138,176))
        p.mesh.quad((a[0],0,-2),(b[0],0,-2),(d[0],0,2),(c[0],0,2),uv=uv,color=(117,138,176))
    for i in (0,8):p.mesh.quad(levels[0][i],levels[1][i],(levels[1][i][0],0,2),(levels[0][i][0],0,-2),uv=uv,color=(117,138,176))
    from geometry import inspect
    inspect(p.mesh.positions,p.mesh.indices,repair=True)
    fit(p);p.add_note('closed faceted distant ridge; visual boundary only, zero gameplay collision')


def campfire_static(p):
    """Retains the real stone/log/flame meshes in their static bind pose."""
    from parts.showcase import build_campfire
    build_campfire(p)
    p.clips.clear()
    p.add_note('static concept seating fire; no animation or character ownership')


REBUILDS={
 'outdoor:tree_01':tree01,'outdoor:tree_02':tree02,'outdoor:tree_03':tree03,
 **{'outdoor:'+name:lamp for name in ('lamp_stand','lamp_fence','lamp_wall','streetlight')},
 **{'outdoor:showcase_'+name:rock for name in ('boulder','rock_face','rock_face_variant')},
}
PROPS={
 'outdoor:bush_round':bush,'outdoor:bush_low':bush,
 'outdoor:fence_two_rail':fence,'outdoor:masonry_pier':pier,
 'outdoor:porch_canopy':canopy,'outdoor:road_barrier':barrier,'outdoor:boundary_ridge':ridge,
 'outdoor:campfire_static':campfire_static,
}
