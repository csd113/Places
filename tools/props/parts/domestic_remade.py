"""Domestic prop remakes: closed low-poly forms, existing file-backed palettes.

Only the named builders opt in; unrelated legacy props retain their authoring.
"""
import math
from pathlib import Path

from geometry import inspect
from parts.refreshed import load_atlas_from, padded_box, solid_box, solid_cylinder, outward_lathe, orient_outward

ROOT = Path(__file__).resolve().parents[3] / 'assets'
WHITE = (255, 255, 255)


def atlas(p, names):
    folder = ('core/props/models' if p.id in ('core:bed', 'core:couch') else
              'environment/office/props/models' if p.id == 'core:chair' else
              'environment/home/props/models')
    return load_atlas_from(p, ROOT / folder / (p.id.split(':')[1] + '.png'), names)


def loft(p, rings, uv, color=WHITE):
    """Closed equal-sided ring loft, with cap fans and independent face UVs."""
    start = len(p.mesh.indices)
    n = len(rings[0])
    for a, b in zip(rings, rings[1:]):
        for i in range(n):
            j = (i + 1) % n
            p.mesh.quad(a[i], a[j], b[j], b[i], uv=uv, color=color,
                        shade_mult=(.86, .92, 1, .94, .83, .78, .80, .83)[i % 8])
    for ring in (rings[0], rings[-1]):
        center = tuple(sum(v[a] for v in ring) / n for a in range(3))
        u0, v0, u1, v1 = uv
        for i in range(n):
            a, b = 2*math.pi*i/n, 2*math.pi*(i+1)/n
            coords = [((u0+u1)/2, (v0+v1)/2)] + [
                ((u0+u1)/2 + math.cos(t)*(u1-u0)/2,
                 (v0+v1)/2 + math.sin(t)*(v1-v0)/2) for t in (a,b)]
            p.mesh.triangle(center, ring[i], ring[(i+1)%n], uvs=coords, color=color)
    # Closed shells permit signed-volume orientation, including concave bends.
    subset = p.mesh.indices[start:]
    inspect(p.mesh.positions, subset, repair=True)
    p.mesh.indices[start:] = subset


def section(z, x, y, width, height):
    """Octagonal rectangle in XY, travelling along Z."""
    return [(x+a*width/2, y+b*height/2, z) for a,b in
            ((-1,-.65),(-.65,-1),(.65,-1),(1,-.65),(1,.65),(.65,1),(-.65,1),(-1,.65))]


def handle(p, uv, start, end, height):
    loft(p, [section(start,0,height/2,.009,height*.65),
             section(start+.007,0,height/2,.014,height),
             section(end-.012,0,height/2,.012,height*.85),
             section(end,0,height/2,.007,height*.55)],uv)


def knife(p):
    t=atlas(p,('blade','handle','bolster','spare'))
    handle(p,t.uv('handle', inset=1),-.1075,-.011,.016)
    padded_box(p,(0,.008,-.010),(.017,.014,.009),t.uv('bolster', inset=1),bevel=.002)
    # Asymmetric table-knife blade: straight spine, broad heel, rounded nose.
    loft(p,[section(-.009,0,.007,.018,.004),
            section(.010,.002,.007,.020,.0035),
            section(.072,.002,.007,.020,.003),
            section(.099,.000,.007,.015,.0025),
            section(.1075,-.003,.007,.005,.002)],t.uv('blade', inset=1))


def fork(p):
    t=atlas(p,('head','tine','handle','neck'))
    handle(p,t.uv('handle', inset=1),-.098,.003,.012)
    loft(p,[section(-.004,0,.006,.007,.004),section(.029,0,.004,.012,.004),
            section(.049,0,.005,.026,.004),section(.059,0,.0055,.026,.003)],t.uv('head', inset=1))
    for x in (-.0105,-.0035,.0035,.0105):
        loft(p,[section(.055,x,.005,.005,.003),section(.080,x,.007,.0045,.0025),
                section(.098,x,.010,.0028,.002)],t.uv('tine', inset=1))


def spoon(p):
    t=atlas(p,('bowl','handle','neck','spare'))
    handle(p,t.uv('handle', inset=1),-.0925,.010,.012)
    loft(p,[section(.006,0,.006,.007,.004),section(.040,0,.013,.009,.003),
            section(.039,0,.018,.010,.003)],t.uv('neck', inset=1))
    outward_lathe(p,(0,0,.065),[(.002,.003),(.003,.010),(.011,.016),(.020,.018),
                   (.019,.0165),(.011,.0145),(.005,.008)],segments=16,
                  ellipse=(1,1.52777778),uv=t.uv('bowl', inset=1),color=WHITE,cap_start=True,cap_end=True)


def china(p, bowl=False):
    t=atlas(p,('outside','inside','foot','rim') if bowl else ('face','rim','edge','foot'))
    if bowl:
        bands=[('foot',[(0,.026),(.005,.030),(.009,.030)],True,False),
               ('outside',[(.009,.030),(.022,.047),(.043,.066),(.061,.075)],False,False),
               ('rim',[(.061,.075),(.065,.0735),(.065,.0705),(.060,.070)],False,False),
               ('inside',[(.060,.070),(.040,.060),(.020,.040),(.012,.026)],False,True)]
    else:
        bands=[('foot',[(0,.052),(.004,.055),(.006,.054)],True,False),
               ('edge',[(.006,.054),(.008,.080),(.018,.108),(.020,.110)],False,False),
               ('rim',[(.020,.110),(.022,.108),(.022,.091),(.012,.077)],False,False),
               ('face',[(.012,.077),(.010,.060)],False,True)]
    for name,profile,first,last in bands:
        outward_lathe(p,(0,0,0),profile,segments=20,uv=t.uv(name),color=WHITE,
                      cap_start=first,cap_end=last)


def plate(p): china(p)
def bowl(p): china(p,True)


def couch(p):
    t=atlas(p,('body','seat','back','wood'))
    # Source wood cell also contains the old throw-pillow swatch; sample its wood half.
    u,v,s,w=t.uv('wood', inset=1); wood=(u,v,s,(v+w)/2)
    for x in (-.86,.86):
        for z in (-.33,.33):
            solid_cylinder(p,(x,0,z),.043,.15,segments=6,taper=.8,uv=wood,color=WHITE)
    padded_box(p,(0,.235,0),(1.94,.23,.86),t.uv('body', inset=1),bevel=.025)
    padded_box(p,(0,.62,-.345),(1.96,.56,.21),t.uv('body', inset=1),bevel=.038)
    for x in (-.895,.895):
        # Squared upholstered arms with a sloped leading edge, rather than tube rolls.
        loft(p,[section(-.41,x,.475,.21,.44),section(.32,x,.455,.21,.40),
                section(.45,x,.435,.19,.35)],t.uv('body', inset=1))
    for x in (-.535,0,.535):
        padded_box(p,(x,.40,.075),(.516,.16,.70),t.uv('seat', inset=1),bevel=.026)
        padded_box(p,(x,.665,-.183),(.516,.39,.20),t.uv('back', inset=1),bevel=.032,rotation=(-9,0,0))
    p.add_note('three-seat sofa; tapered feet, sloped upholstered arms, separate seat/back cushions')


def bed(p):
    t=atlas(p,('frame','mattress','linen','board'))
    for x in (-.62,.62):
        for z in (-.90,.90):
            solid_box(p,(x,.09,z),(.09,.18,.09),uv=t.uv('frame', inset=1),color=WHITE)
    for x in (-.665,.665):
        padded_box(p,(x,.23,0),(.07,.22,2),t.uv('frame', inset=1),bevel=.008)
    padded_box(p,(0,.325,-.965),(1.4,.45,.07),t.uv('board', inset=1),bevel=.009)
    padded_box(p,(0,.225,.965),(1.4,.20,.07),t.uv('board', inset=1),bevel=.009)
    padded_box(p,(0,.337,0),(1.30,.16,1.89),t.uv('mattress', inset=1),bevel=.023)
    # Closed top and side folds wrap the mattress shoulders.
    padded_box(p,(0,.436,.26),(1.27,.035,1.35),t.uv('linen', inset=1),bevel=.009)
    for x in (-.643,.643):
        padded_box(p,(x,.389,.26),(.025,.12,1.35),t.uv('linen', inset=1),bevel=.008)
    padded_box(p,(0,.465,-.35),(1.26,.026,.15),t.uv('linen', inset=1),bevel=.009)
    for x,angle in ((-.325,-3),(.325,3)):
        padded_box(p,(x,.467,-.682),(.57,.09,.40),t.uv('mattress', inset=1),bevel=.034,rotation=(0,angle,0))
    p.add_note('low bed: recessed feet, solid timber rails, bevelled mattress, folded blanket and pillows')


def chair(p):
    t=atlas(p,('shell','pad','metal','dark'))
    for i in range(5):
        angle=math.radians(i*72+90)
        x,z=math.cos(angle)*.205,math.sin(angle)*.205
        start = len(p.mesh.indices)
        p.box((x/2,.068,z/2),(.031,.033,.205),uv=t.uv('metal', inset=1),color=WHITE,
                  rotation=(0,90-math.degrees(angle),0))
        orient_outward(p.mesh,start,(x/2,.068,z/2))
        # Horizontal axle casters, not vertical cylinders.
        solid_cylinder(p,(x-.017,.032,z),.032,.034,segments=8,axis='x',uv=t.uv('dark', inset=1),color=WHITE)
    solid_cylinder(p,(0,.048,0),.041,.19,segments=8,uv=t.uv('dark', inset=1),color=WHITE)
    solid_cylinder(p,(0,.22,0),.024,.19,segments=8,uv=t.uv('metal', inset=1),color=WHITE)
    padded_box(p,(0,.405,.017),(.49,.048,.44),t.uv('shell', inset=1),bevel=.018)
    # Rising rear edge and waterfall nose make the seat visibly moulded.
    loft(p,[section(-.202,0,.463,.42,.052),section(-.11,0,.448,.45,.056),
            section(.16,0,.451,.49,.058),section(.237,0,.430,.41,.041)],t.uv('pad', inset=1))
    padded_box(p,(0,.593,-.183),(.055,.33,.042),t.uv('metal', inset=1),bevel=.008,rotation=(-8,0,0))
    padded_box(p,(0,.744,-.199),(.43,.306,.065),t.uv('shell', inset=1),bevel=.022,rotation=(-5,0,0))
    padded_box(p,(0,.740,-.155),(.389,.264,.055),t.uv('pad', inset=1),bevel=.023,rotation=(-5,0,0))
    p.add_note('five-star office chair with horizontal casters, shaped waterfall seat and lumbar pad')


def switch(p):
    t=atlas(p,('plate','rocker','metal'))
    p.begin_material(p.material('switch_body'))
    p.begin_mesh('plate')
    padded_box(p,(0,.06,.005),(.086,.12,.010),t.uv('plate', inset=1),bevel=.003)
    padded_box(p,(0,.066,.012),(.044,.066,.014),t.uv('plate', inset=1),bevel=.004)
    solid_box(p,(0,.066,.018),(.035,.056,.003),uv=t.uv('metal', inset=1),color=(80,83,80))
    for y in (.012,.108):
        solid_cylinder(p,(0,y,.010),.0034,.0015,segments=8,axis='z',uv=t.uv('metal', inset=1),color=WHITE)
        solid_box(p,(0,y,.0116),(.0045,.00065,.00025),uv=t.uv('metal', inset=1),color=(70,70,70))
    p.begin_mesh('rocker')
    padded_box(p,(0,.066,.022),(.030,.049,.010),t.uv('rocker', inset=1),bevel=.0025,rotation=(15,0,0))
    p.node('switch',mesh='plate',children=[1])
    p.node('lever_pivot',translation=(0,.066,.022),children=[2])
    p.node('lever',translation=(0,-.066,-.022),mesh='rocker')
    p.clip('toggle',[dict(node=1,path='rotation',times=[0,.35],
                        values=[[0,0,0,1],[math.sin(math.radians(-15)),0,0,math.cos(math.radians(-15))]],
                        interpolation='LINEAR')])


def crt(p):
    t=atlas(p,('screen','bezel','body','panel'))
    # Full closed tapered rear case, front moulding and rounded glass lens.
    loft(p,[section(-.23,0,.260,.36,.32),section(-.185,0,.260,.41,.36),
            section(.130,0,.270,.55,.42),section(.195,0,.270,.55,.42)],t.uv('body', inset=1))
    for x in (-.20,.20):
        for z in (-.16,.15):
            padded_box(p,(x,.035,z),(.07,.070,.09),t.uv('body', inset=1),bevel=.009)
    # Screen-surround solid sits in front of cabinet; lens is proud of its recess.
    loft(p,[section(.193,-.025,.289,.475,.343),section(.209,-.025,.289,.459,.327)],t.uv('bezel', inset=1))
    lens_start = len(p.mesh.positions)
    loft(p,[section(.2095,-.025,.294,.404,.277),section(.218,-.025,.294,.393,.267),
            section(.224,-.025,.294,.33,.218)],t.uv('screen', inset=1),color=(230,238,238))
    u0,v0,u1,v1 = t.uv('screen', inset=2)
    for i in range(lens_start,len(p.mesh.positions)):
        x,y,_ = p.mesh.positions[i]
        p.mesh.uvs[i]=(u0+(x+.025+.202)/.404*(u1-u0), v0+(1-(y-.294+.1385)/.277)*(v1-v0)*.75)
        p.mesh.colors[i]=(230,238,238,255)
    # Lower control panel keeps the source's speaker marks and legends.
    padded_box(p,(0,.092,.208),(.485,.052,.020),t.uv('panel', inset=1),bevel=.004)
    for x,r in ((.130,.016),(.193,.012)):
        solid_cylinder(p,(x,.091,.216),r,.014,segments=10,axis='z',uv=t.uv('bezel', inset=1),color=WHITE)
    for i in range(6):
        solid_box(p,(.205,.23+i*.022,-.123),(.003,.007,.087),uv=t.uv('body', inset=1),color=(95,92,83))
    p.add_note('tapered octagonal CRT housing, rounded convex glass, lower speaker/control fascia; unlit')
