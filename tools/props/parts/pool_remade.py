"""Closed pool rail stock and thin, pleated curtain shells using source PNGs."""
import math
from pathlib import Path
from geometry import inspect
from parts.refreshed import load_atlas_from, solid_box, solid_cylinder, outward_lathe, padded_box

DIRECTORY = Path(__file__).resolve().parents[3]/'assets/environment/pool/props/models'
WHITE = (255,255,255)


def atlas(p, curtain):
    names=('cloth','post','plate','track') if curtain else ('post','rail','plate','spare')
    return load_atlas_from(p,DIRECTORY/(p.id.split(':')[1]+'.png'),names)


def post(p,x,z,t,curtain=False,axis='z'):
    h=2.60 if curtain else 1.05
    foot=(.06,.014,.06) if curtain else ((.048,.014,.08) if axis=='z' else
                                       (.08,.014,.048) if axis=='x' else (.08,.014,.08))
    solid_box(p,(x,.007,z),foot,uv=t.uv('plate',inset=1),color=WHITE)
    outward_lathe(p,(x,0,z),[(.007,.024),(h-.022,.024),(h,.01584)],
                  segments=8,uv=t.uv('post',inset=1),color=WHITE,cap_start=True,cap_end=True)



def rails(p):
    t=atlas(p,False)
    corner=p.id.endswith('corner')
    limit=p.size[0]/2
    if corner:
        station=-limit+.04
        for x,z,axis in ((limit-.024,station,'z'),(station,limit-.024,'x'),(station,station,'both')):
            post(p,x,z,t,axis=axis)
        spans=[('x',(-limit,station)),('z',(station,-limit))]
    else:
        for x in ((-limit+.024,0,limit-.024) if p.id.endswith('straight') else (-limit+.024,limit-.024)):
            post(p,x,0,t)
        spans=[('x',(-limit,0))]
    for axis,(x,z) in spans:
        for y in (.98,):
            solid_cylinder(p,(x,y,z),.021,2*limit,axis=axis,segments=8,
                           uv=t.uv('rail',inset=1),color=WHITE)
    p.add_note('single concept waist rail at 0.98 m, slim flanged posts, closed stock')


def cloth(p,paths,uv):
    """Four height rows with continuous UVs, a closed 2 mm edge and no coincident backfaces.

    Path entries hold track X/Z and full-fold X/Z offsets. Narrow gathering at
    the header spreads below the suspension tabs, with a shallow scalloped hem.
    """
    n=len(paths)
    rows=[]
    for y,spread in ((.075,1),(.13,1),(2.22,.92),(2.44,.20)):
        row=[]
        for i,(x,z,dx,dz) in enumerate(paths):
            sag=.012*(1-math.cos(i*math.pi/2)) if y<.2 else 0
            row.append((x+dx*spread,y+sag,z+dz*spread))
        rows.append(row)
    sides=[]
    for sign in (-1,1):
        side=[]
        for row in rows:
            pts=[]
            for i,(x,y,z) in enumerate(row):
                a,b=row[max(0,i-1)],row[min(n-1,i+1)]
                tx,tz=b[0]-a[0],b[2]-a[2];length=math.hypot(tx,tz)
                pts.append((x-sign*tz/length*.001,y,z+sign*tx/length*.001))
            side.append(pts)
        sides.append(side)
    start=len(p.mesh.indices)
    u0,v0,u1,v1=uv
    for side in sides:
        for r in range(3):
            for i in range(n-1):
                coords=[(u0+(u1-u0)*j/(n-1),v1-(v1-v0)*(rows[k][j][1]-.075)/2.38)
                        for k,j in ((r,i),(r,i+1),(r+1,i+1),(r+1,i))]
                p.mesh.quad(side[r][i],side[r][i+1],side[r+1][i+1],side[r+1][i],
                            uv=coords,color=WHITE,shade_mult=.91+.07*math.cos((i+.5)*math.pi/4))
    for r in (0,3):
        for i in range(n-1):
            p.mesh.quad(sides[0][r][i],sides[0][r][i+1],sides[1][r][i+1],sides[1][r][i],uv=uv,color=WHITE)
    for i in (0,n-1):
        for r in range(3):
            p.mesh.quad(sides[0][r][i],sides[0][r+1][i],sides[1][r+1][i],sides[1][r][i],uv=uv,color=WHITE)
    indices=p.mesh.indices[start:];inspect(p.mesh.positions,indices,repair=True);p.mesh.indices[start:]=indices


def curtains(p):
    t=atlas(p,True)
    corner=p.id.endswith('corner');limit=p.size[0]/2
    if corner:
        post(p,-.27,-.27,t,True)
        for center,size in (((0,2.50,-.27),(.6,.02,.035)),((-.27,2.50,0),(.035,.02,.6))):
            solid_box(p,center,size,uv=t.uv('track',inset=1),color=WHITE)
        # Pleats open outwards from each leg, keeping the inside corner clear.
        paths=[]
        for i in range(13):
            paths.append((.29-.56*i/12,-.27,0,-.026*(1-math.cos(i*math.pi/3))/2))
        for i in range(1,13):
            paths.append((-.27,-.27+.56*i/12,-.026*(1-math.cos(i*math.pi/3))/2,0))
    else:
        px=limit-.03
        for x in ((-px,px) if p.id.endswith('straight') else (px,)):
            post(p,x,0,t,True)
        solid_box(p,(0,2.5,0),(2*limit,.02,.035),uv=t.uv('track',inset=1),color=WHITE)
        left=-px+.027 if p.id.endswith('straight') else -limit+.009
        right=px-.027
        count=24 if p.id.endswith('straight') else 12
        paths=[(left+(right-left)*i/count,0,0,.106*math.sin(i*math.pi/4)) for i in range(count+1)]
    cloth(p,paths,t.uv('cloth',inset=1))
    for i,(x,z,dx,dz) in enumerate(paths):
        if i%4==0:
            solid_box(p,(x,2.465,z),(.016,.07,.016),uv=t.uv('post',inset=1),color=WHITE)
    p.add_note('closed 2 mm pleated cloth, continuous header/hem UVs, gathered top and hanging tabs')


def source(p, names):
    return load_atlas_from(p,DIRECTORY/(p.id.split(':')[1]+'.png'),names)


def closed_path(p, points, radius, uv, segments=8):
    start=len(p.mesh.indices)
    p.tube_path(points,radii=radius,segments=segments,uv=uv,color=WHITE,
                cap_start=True,cap_end=True)
    indices=p.mesh.indices[start:]
    inspect(p.mesh.positions,indices,repair=True)
    p.mesh.indices[start:]=indices


def table(p):
    t=source(p,('tray','trim','leg','brace'))
    p.begin_material(p.material('satin_resin',roughness=.78))
    # Broad round moulded top, gently rolled edge and a shallow underside skirt.
    outward_lathe(p,(0,0,0),[(.701,.370),(.714,.400),(.735,.400),(.740,.389)],
                  segments=24,uv=t.uv('tray',inset=2),color=WHITE,
                  cap_start=True,cap_end=True)
    outward_lathe(p,(0,0,0),[(.656,.325),(.704,.345)],segments=16,
                  uv=t.uv('trim',inset=2),color=WHITE,cap_start=True,cap_end=True)
    for x in (-.235,.235):
        for z in (-.235,.235):
            # Square tapered resin legs: broad feet, narrow attachment at apron.
            start=len(p.mesh.indices)
            p.lathe((x,0,z),[(0,.034),(.665,.024)],segments=4,
                    rotation=math.pi/4,uv=t.uv('leg',inset=2),color=WHITE,
                    cap_start=True,cap_end=True)
            indices=p.mesh.indices[start:];inspect(p.mesh.positions,indices,repair=True)
            p.mesh.indices[start:]=indices
    p.add_note('round 0.8 m white resin table, rolled edge, recessed skirt, four tapered feet')


def chair(p):
    t=source(p,('seat','frame','leg','slat'))
    p.begin_material(p.material('satin_resin',roughness=.78))
    padded_box(p,(0,.43,.028),(.472,.044,.438),t.uv('seat',inset=2),bevel=.014)
    for x in (-.203,.203):
        for z in (-.218,.205):
            start=len(p.mesh.indices)
            p.lathe((x,0,z),[(0,.032),(.42,.021)],segments=4,
                    rotation=math.pi/4,uv=t.uv('leg',inset=2),color=WHITE,
                    cap_start=True,cap_end=True)
            indices=p.mesh.indices[start:];inspect(p.mesh.positions,indices,repair=True)
            p.mesh.indices[start:]=indices
        # Integrated side-arm silhouette; the arm returns into the back shell.
        arm_x=math.copysign(.237,x)
        closed_path(p,[(arm_x,.415,.205),(arm_x,.632,.175),(arm_x,.653,-.13),
                       (arm_x,.755,-.252)],.023,t.uv('frame',inset=2),segments=6)
    # Crown arcs up over five vertical slots, rather than horizontal bench slats.
    closed_path(p,[(-.226,.635,-.218),(-.209,.774,-.251),(-.13,.827,-.263),
                   (0,.832,-.263),(.13,.827,-.263),(.209,.774,-.251),
                   (.226,.635,-.218)],.018,t.uv('frame',inset=2),segments=6)
    for x in (-.16,-.08,0,.08,.16):
        height=.325-abs(x)*.19
        solid_box(p,(x,.445+height/2,-.225-height*.10),(.030,height,.020),
                  uv=t.uv('slat',inset=2),color=WHITE,rotation=(-11,0,0))
    solid_box(p,(0,.446,-.202),(.438,.040,.04),uv=t.uv('frame',inset=2),color=WHITE)
    p.mesh.normalize_origin()
    p.add_note('moulded armchair, crowned back with vertical slots, rolled seat, tapered floor-contact feet')


def ladder(p):
    t=source(p,('tube','tread','grip','boot'))
    chrome=p.material('chrome',roughness=.48,metallic=.65)
    rubber=p.material('rubber')
    for x in (-.229,.229):
        # Full inverted-U grab rail: the return is bolted onto the dry deck.
        points=[(x,.03,-.192),(x,1.935,-.192)]
        for i in range(1,9):
            a=math.pi*i/8
            points.append((x,1.935+.241*math.sin(a),.049-.241*math.cos(a)))
        points.append((x,1.5,.290))
        # Keep the established 0.45 m depth and climb-volume alignment.
        points=[(px,py,(pz-.049)*.828+.025) for px,py,pz in points]
        p.begin_material(chrome)
        closed_path(p,points,.024,t.uv('tube',inset=2),segments=8)
        p.begin_material(rubber)
        outward_lathe(p,(x,0,-.17455),[(0,.027),(.045,.027)],segments=8,
                      uv=t.uv('boot',inset=2),color=WHITE,cap_start=True,cap_end=True)
        p.begin_material(chrome)
        outward_lathe(p,(x,1.5,.22455),[(0,.046),(.016,.046),(.028,.032)],segments=8,
                      uv=t.uv('tube',inset=2),color=WHITE,cap_start=True,cap_end=True)
    for y in (.35,.655,.96,1.265):
        p.begin_material(chrome)
        solid_box(p,(0,y,-.168),(.455,.028,.105),uv=t.uv('tread',inset=2),color=WHITE)
        p.begin_material(rubber)
        solid_box(p,(0,y+.016,-.168),(.385,.004,.076),uv=t.uv('grip',inset=2),color=WHITE)
    low,high=p.mesh.bounds()
    depth_scale=.45/(high[2]-low[2])
    p.mesh.positions=[(x,y,z*depth_scale) for x,y,z in p.mesh.positions]
    p.mesh.normalize_origin()
    p.add_note('closed inverted-U chrome rails; real deck return flanges at 1.5 m, four inset non-slip treads')


def bench(p):
    t=source(p,('slat','edge','metal','foot'))
    for z in (-.142,-.047,.047,.142):
        padded_box(p,(0,.432,z),(1.6,.036,.082),t.uv('slat',inset=2),bevel=.008)
    for x in (-.57,.57):
        for z in (-.145,.145):
            solid_box(p,(x,.202,z),(.045,.404,.045),uv=t.uv('metal',inset=2),color=WHITE)
            solid_box(p,(x,.009,z),(.059,.018,.059),uv=t.uv('foot',inset=2),color=WHITE)
        solid_box(p,(x,.396,0),(.065,.03,.38),uv=t.uv('metal',inset=2),color=WHITE)
    p.add_note('four raised resin slats on two closed bolted metal trestles; 45 cm seat')


def drain(p):
    t=source(p,('metal','recess','edge','tile'))
    solid_box(p,(0,.004,0),(.6,.008,.16),uv=t.uv('recess',inset=2),color=WHITE)
    for z in (-.074,.074):
        solid_box(p,(0,.011,z),(.6,.006,.012),uv=t.uv('edge',inset=2),color=WHITE)
    for x in (-.294,.294):
        solid_box(p,(x,.011,0),(.012,.006,.136),uv=t.uv('edge',inset=2),color=WHITE)
    for i in range(15):
        solid_box(p,(-.276+i*.0394,.011,0),(.022,.006,.136),uv=t.uv('metal',inset=2),color=WHITE)
    p.add_note('closed recessed dark gutter and raised 15-bar stainless grate, 14 mm proud, non-colliding')


def service_door(p):
    t=source(p,('leaf','frame','metal','dark'))
    padded_box(p,(0,1.035,-.019),(.884,2.03,.075),t.uv('leaf',inset=2),bevel=.009)
    for x in (-.476,.476):
        solid_box(p,(x,1.025,0),(.048,2.05,.15),uv=t.uv('frame',inset=2),color=WHITE)
    solid_box(p,(0,2.075,0),(1,.05,.15),uv=t.uv('frame',inset=2),color=WHITE)
    solid_box(p,(0,.012,0),(.904,.024,.14),uv=t.uv('metal',inset=2),color=WHITE)
    solid_box(p,(.32,1.00,.038),(.05,.20,.006),uv=t.uv('metal',inset=2),color=WHITE)
    closed_path(p,[(.30,.935,.043),(.30,.935,.069),(.30,1.065,.069),(.30,1.065,.043)],
                .007,t.uv('metal',inset=2),segments=6)
    # Recessed ventilation grille at the leaf base; frame and slats are separate.
    solid_box(p,(0,.28,.020),(.50,.29,.007),uv=t.uv('dark',inset=2),color=WHITE)
    for i in range(6):
        solid_box(p,(0,.168+i*.042,.030),(.478,.012,.014),uv=t.uv('frame',inset=2),color=WHITE)
    p.add_note('blue closed service leaf, deep jambs, metal kick threshold, pull handle and recessed six-slat vent')
