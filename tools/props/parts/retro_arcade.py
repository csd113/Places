"""Reusable, closed low-poly Retro Bowling & Arcade prop family.

Normal export loads real committed PNGs; author_retro_arcade.py is the explicit
offline artwork command. All dimensions are metres, +Y up, +Z front, and origins
are normalized to centered X/Z and base-contact Y=0. Deliberately flat geometric
normals follow the current exporter/importer convention. Graphic region ratios
and material ownership are part of the fitted atlas contract below.
"""
from __future__ import annotations

import math
from pathlib import Path

from geometry import inspect
from parts.beach_structures import _ear_triangles, beam, extrude, loft
from parts.frutiger_aero import rounded_outline
from parts.refreshed import load_atlas_from, outward_lathe, padded_box, solid_box, solid_cylinder

ROOT = Path(__file__).resolve().parents[3] / "assets/environments/arcade/props"
WHITE = (255, 255, 255)
PALETTE = (
    "ivory", "red", "blue", "grey", "metal", "dark", "black", "white",
    "purple", "burgundy", "green", "marble", "bottle", "label", "yellow", "paper",
)
PALETTE_REGIONS = {name: (i % 4*64, i // 4*64, 64, 64) for i,name in enumerate(PALETTE)}
CABINET_REGIONS = {
    "screen": (0,0,128,96), "marquee": (0,96,128,32),
    "control": (0,128,128,64), "coin": (0,192,64,64),
    "body": (64,192,32,32), "metal": (96,192,32,32),
    "dark": (64,224,32,32), "white": (96,224,32,32), "side": (128,0,128,256),
}
SCORE_REGIONS = {
    "screen": (0,0,192,144), "inactive": (0,144,128,96),
    "body": (192,0,64,64), "bezel": (192,64,64,64),
    "metal": (192,128,64,64), "white": (192,192,64,64),
}
VENDING_REGIONS = {
    "advert": (0,0,128,80), "lower": (0,176,128,80),
    **{f"drink_{i}": (i%4*32,80+i//4*32,32,32) for i in range(12)},
    "dispense": (128,192,64,32),
    "controls": (128,0,64,192), "body": (192,0,64,64),
    "trim": (192,64,64,64), "dark": (192,128,64,64), "metal": (192,192,64,64),
}
PINBALL_REGIONS = {
    "playfield": (0,0,128,256), "backbox": (128,0,128,80),
    "score": (128,80,128,32), "apron": (128,112,128,32),
    "side": (128,144,128,64), "body": (128,208,32,48),
    "metal": (160,208,32,48), "dark": (192,208,32,48), "white": (224,208,32,48),
}
UTILITY_REGIONS = {name:(i%4*64,i//4*64,64,64) for i,name in enumerate(
    ("body","metal","dark","white","red","blue","yellow","tube"))}
UTILITY_REGIONS.update(change=(0,128,128,128),backing=(128,128,128,128))
CAT_REGIONS = {name:(i%2*128,i//2*128,128,128) for i,name in enumerate(("black","white","eye","nose"))}
SIZES = {
    "bowling_pin": (.12,.38,.12),
    **{f"bowling_ball_{n}":(.216,.216,.216) for n in ("purple","burgundy","black","cobalt","red","green","marbled")},
    "ball_return": (.86,1.06,1.85), "ball_rack": (1.10,.72,.64),
    "score_monitor": (.80,1.12,.69), "score_monitor_inactive": (.80,1.12,.69),
    "fixed_chair_red": (.60,.92,.60), "fixed_chair_blue": (.60,.92,.60),
    "pedestal_table": (.90,.72,.90), "soda_bottle": (.095,.28,.095),
    "vending_machine": (.96,2.03,.86),
    "cabinet_racing": (.82,1.91,1.00), "cabinet_shooter": (.74,1.96,.84),
    "cabinet_action": (.76,1.88,.87), "cabinet_fighting": (.90,1.88,.93),
    "cabinet_scifi": (.78,2.00,.89),
    "pinball_cosmic": (.74,1.88,1.42), "pinball_comet": (.74,1.88,1.42),
    "change_machine": (.52,1.46,.48), "neon_bowl": (1.82,.86,.13),
    "blacklight_fixture": (1.24,.105,.13), "cat_plush": (.25,.36,.235),
    "litter_can": (.066,.12,.066), "litter_cup": (.09,.14,.09),
    "litter_wrapper": (.12,.016,.085), "litter_paper": (.21,.012,.148),
    "litter_bottle_fallen": (.095,.095,.28),
    "gutter_segment": (.27,.135,3.0), "lane_divider": (.075,.14,3.0),
}
LOW_PALETTE_PROPS = {
    "bowling_pin", "fixed_chair_red", "fixed_chair_blue", "pedestal_table",
    "ball_return", "ball_rack", "gutter_segment", "lane_divider",
    *(n for n in SIZES if n.startswith("bowling_ball_")),
}


class FittedAtlas:
    """Keep the 256-pixel authoring frame and insets stable at either native size."""
    def __init__(self,texture):
        self.texture=texture
        self.scale=texture.width/256

    def uv(self,name,inset=0):
        return self.texture.uv(name,inset*self.scale)


def atlas(p, filename="bowling_palette", regions=PALETTE_REGIONS):
    p.ao_strength = 0
    if filename=="bowling_palette" and p.id.split(':')[-1] in LOW_PALETTE_PROPS:
        filename="bowling_palette_128"
    if p.id.endswith("blacklight_fixture"):
        filename="utility_128"
    t=load_atlas_from(p,ROOT/(filename+".png"),())
    for name,rect in regions.items(): t.region(name,tuple(round(v*t.width/256) for v in rect))
    return FittedAtlas(t)


def material(p,name="plastic",roughness=.46,**kwargs):
    slot=p.material("arcade_"+name,roughness=roughness,**kwargs)
    p.begin_material(slot)
    return slot


def box(p,center,size,uv,**kwargs):
    solid_box(p,center,size,uv=uv,color=WHITE,shade=False,**kwargs)


def illuminated_panel(p,center,size,front_uv,stock_uv,face_slot,stock_slot):
    """Closed pane with emission only on its printed +Z display face."""
    cx,cy,cz=center;w,h,d=size
    front=[(cx-w/2,cy-h/2,cz+d/2),(cx+w/2,cy-h/2,cz+d/2),
           (cx+w/2,cy+h/2,cz+d/2),(cx-w/2,cy+h/2,cz+d/2)]
    rear=[(x,y,cz-d/2) for x,y,_ in front]
    p.begin_material(face_slot)
    p.mesh.quad(*front,uv=front_uv,color=WHITE)
    p.begin_material(stock_slot)
    p.mesh.quad(*reversed(rear),uv=stock_uv,color=WHITE)
    for i in range(4):
        j=(i+1)%4
        p.mesh.quad(front[i],rear[i],rear[j],front[j],uv=stock_uv,color=WHITE)


def cylinder(p,base,radius,height,uv,segments=12,**kwargs):
    solid_cylinder(p,base,radius,height,segments=segments,uv=uv,color=WHITE,shades=False,**kwargs)


def lathe(p,base,profile,uv,segments=12,**kwargs):
    outward_lathe(p,base,profile,segments=segments,uv=uv,color=WHITE,shades=False,**kwargs)


def rounded_box(p,center,size,uv,radius=.055,segments=2):
    cx,cy,cz=center;w,h,d=size
    contour=rounded_outline(w,d,radius,segments)
    loft(p,[[(cx+x,y,cz+z) for x,z in contour] for y in (cy-h/2,cy+h/2)],uv)


def transform_new(p,start,center=(0,0,0),x_angle=0,z_angle=0):
    cx,cy,cz=center;ax,az=math.radians(x_angle),math.radians(z_angle)
    for i in range(start,len(p.mesh.positions)):
        x,y,z=p.mesh.positions[i];x-=cx;y-=cy;z-=cz
        y,z=y*math.cos(ax)-z*math.sin(ax),y*math.sin(ax)+z*math.cos(ax)
        x,y=x*math.cos(az)-y*math.sin(az),x*math.sin(az)+y*math.cos(az)
        p.mesh.positions[i]=(x+cx,y+cy,z+cz)


def tilted_box(p,center,size,uv,angle=0):
    start=len(p.mesh.positions)
    box(p,center,size,uv)
    transform_new(p,start,center,x_angle=angle)


def frame(p,center,outer,inner,depth,uv,inner_center=None):
    """True open XY frame with closed front/rear/outer/inner stock faces."""
    cx,cy,cz=center;icx,icy=inner_center or (cx,cy)
    if len(outer)!=len(inner): raise ValueError("matching frame ring counts required")
    n=len(outer)
    rows=[[(cx+x,cy+y,cz+z) for x,y in outer] for z in (-depth/2,depth/2)]
    rows += [[(icx+x,icy+y,cz+z) for x,y in inner] for z in (-depth/2,depth/2)]
    for i in range(n):
        j=(i+1)%n
        for a,b in ((0,1),(2,3),(0,2),(1,3)):
            p.mesh.quad(rows[a][i],rows[a][j],rows[b][j],rows[b][i],uv=uv,color=WHITE)


def rect_frame(p,center,width,height,border,depth,uv):
    outer=[(-width/2,-height/2),(width/2,-height/2),(width/2,height/2),(-width/2,height/2)]
    inner=[(-width/2+border,-height/2+border),(width/2-border,-height/2+border),
           (width/2-border,height/2-border),(-width/2+border,height/2-border)]
    frame(p,center,outer,inner,depth,uv)


def curved_screen(p,center,width,height,uv,tilt=0,bulge=.012):
    """Sealed slightly convex 4:3 CRT glass; the rear is behind its bezel."""
    start=len(p.mesh.positions);cx,cy,cz=center
    u0,v0,u1,v1=uv;n=3
    rows=[]
    for rear in (False,True):
        grid=[]
        for iy in range(n+1):
            row=[]
            for ix in range(n+1):
                nx,ny=ix/n-.5,iy/n-.5
                z=cz-.006 if rear else cz+bulge*(1-4*nx*nx)*(1-4*ny*ny)
                row.append((cx+nx*width,cy+ny*height,z))
            grid.append(row)
        rows.append(grid)
        for iy in range(n):
            for ix in range(n):
                pts=[grid[iy][ix],grid[iy][ix+1],grid[iy+1][ix+1],grid[iy+1][ix]]
                coords=[(u0+(u1-u0)*ix/n,v1-(v1-v0)*iy/n),
                        (u0+(u1-u0)*(ix+1)/n,v1-(v1-v0)*iy/n),
                        (u0+(u1-u0)*(ix+1)/n,v1-(v1-v0)*(iy+1)/n),
                        (u0+(u1-u0)*ix/n,v1-(v1-v0)*(iy+1)/n)]
                p.mesh.quad(*pts,uv=coords,color=WHITE)
    perimeter=[(0,i) for i in range(n+1)]+[(i,n) for i in range(1,n+1)]+[(n,i) for i in range(n-1,-1,-1)]+[(i,0) for i in range(n-1,0,-1)]
    for k,(y,x) in enumerate(perimeter):
        yy,xx=perimeter[(k+1)%len(perimeter)]
        p.mesh.quad(rows[0][y][x],rows[0][yy][xx],rows[1][yy][xx],rows[1][y][x],uv=uv,color=WHITE)
    transform_new(p,start,center,x_angle=tilt)


def normalize(v):
    length=math.sqrt(sum(q*q for q in v))
    return tuple(q/length for q in v)


def cross(a,b):
    return (a[1]*b[2]-a[2]*b[1],a[2]*b[0]-a[0]*b[2],a[0]*b[1]-a[1]*b[0])


def sphere_data(subdivide=1):
    phi=(1+math.sqrt(5))/2
    vertices=[normalize(v) for v in ((-1,phi,0),(1,phi,0),(-1,-phi,0),(1,-phi,0),(0,-1,phi),(0,1,phi),(0,-1,-phi),(0,1,-phi),(phi,0,-1),(phi,0,1),(-phi,0,-1),(-phi,0,1))]
    faces=[(0,11,5),(0,5,1),(0,1,7),(0,7,10),(0,10,11),(1,5,9),(5,11,4),(11,10,2),(10,7,6),(7,1,8),(3,9,4),(3,4,2),(3,2,6),(3,6,8),(3,8,9),(4,9,5),(2,4,11),(6,2,10),(8,6,7),(9,8,1)]
    for _ in range(subdivide):
        cache={};new=[]
        def midpoint(a,b):
            key=tuple(sorted((a,b)))
            if key not in cache:
                cache[key]=len(vertices)
                vertices.append(normalize(tuple((vertices[a][k]+vertices[b][k])/2 for k in range(3))))
            return cache[key]
        for a,b,c in faces:
            ab,bc,ca=midpoint(a,b),midpoint(b,c),midpoint(c,a)
            new.extend(((a,ab,ca),(b,bc,ab),(c,ca,bc),(ab,bc,ca)))
        faces=new
    return vertices,faces


def sphere(p,center,radii,uv,subdivide=1):
    vertices,faces=sphere_data(subdivide)
    points=[tuple(center[k]+v[k]*radii[k] for k in range(3)) for v in vertices]
    u0,v0,u1,v1=uv
    for a,b,c in faces:
        coords=[]
        for i in (a,b,c):
            x,y,z=vertices[i]
            coords.append((u0+(math.atan2(z,x)/math.tau+.5)*(u1-u0),v0+(math.acos(max(-1,min(1,y)))/math.pi)*(v1-v0)))
        p.mesh.triangle(points[a],points[b],points[c],uvs=coords,color=WHITE)


def finish(p,note):
    low,high=p.mesh.bounds()
    p.author_origin_shift=((low[0]+high[0])/2,low[1],(low[2]+high[2])/2)
    p.mesh.normalize_origin()
    inspect(p.mesh.positions,p.mesh.indices,repair=True)
    report=inspect(p.mesh.positions,p.mesh.indices)
    invalid=("degenerate","boundary_edges","nonmanifold_edges","inconsistent_edges","flipped_triangles","contradictory_components")
    if any(report[key] for key in invalid): raise ValueError(f"{p.id}: invalid Arcade topology: {report}")
    if p.mesh.triangle_count>800: raise ValueError(f"{p.id}: {p.mesh.triangle_count} triangles exceeds this family's 800-triangle review ceiling")
    p.mesh.validate(p.size)
    p.add_note(note)


def bowling_pin(p):
    t=atlas(p);material(p,'pin_enamel',.24)
    ivory,red=t.uv('ivory',2),t.uv('red',2)
    # One continuous commercial pin silhouette, with real narrow double bands.
    profile=[(0,.035),(.025,.043),(.075,.057),(.135,.060),(.185,.046),(.227,.028),(.257,.024),(.272,.024),(.282,.024),(.294,.025),(.304,.031),(.321,.041),(.343,.042),(.364,.032),(.38,.013)]
    lathe(p,(0,0,0),profile,ivory,12)
    # Fitted narrow sleeves sit clear of the enamel without duplicating bodies.
    for y in (.258,.282): lathe(p,(0,0,0),[(y,.0244),(y+.010,.0244)],red,12)
    finish(p,'continuous twelve-sided pin, broad belly, neck/head and two red enamel bands')


def bowling_ball(p):
    t=atlas(p);name=p.id.split('_')[-1]
    region={'cobalt':'blue','marbled':'marble'}.get(name,name)
    material(p,'polished_ball',.16,metallic=.08)
    uv=t.uv(region,2);dark=t.uv('black',2);radius=.108
    vertices,faces=sphere_data(1)
    directions=[normalize(v) for v in ((-.40,.81,.43),(.40,.81,.43),(0,.35,.94))]
    chosen=[]
    for direction in directions:
        ranked=sorted(range(len(faces)),key=lambda i:sum(sum(vertices[j][k] for j in faces[i])/3*direction[k] for k in range(3)),reverse=True)
        chosen.append(next(i for i in ranked if i not in chosen))
    u0,v0,u1,v1=uv
    def uvpoint(q):
        return (u0+(math.atan2(q[2],q[0])/math.tau+.5)*(u1-u0),v0+math.acos(max(-1,min(1,q[1])))/math.pi*(v1-v0))
    for i,face in enumerate(faces):
        outer=[tuple(v*radius for v in vertices[j]) for j in face]
        if i not in chosen:
            p.mesh.triangle(*outer,uvs=[uvpoint(vertices[j]) for j in face],color=WHITE)
            continue
        center=tuple(sum(q[k] for q in outer)/3 for k in range(3))
        normal=normalize(center);basis=normalize(tuple(outer[0][k]-center[k] for k in range(3)));side=cross(normal,basis)
        angles=[]
        for q in outer:
            relative=tuple(q[k]-center[k] for k in range(3))
            angle=math.atan2(sum(relative[k]*side[k] for k in range(3)),sum(relative[k]*basis[k] for k in range(3)))%math.tau
            angles.append(angle)
        order=sorted(range(3),key=lambda j:angles[j]);outer=[outer[j] for j in order];angles=[angles[j] for j in order]
        hole_radius=.014
        ring=[];outer_to_inner=[]
        for j,count in enumerate((3,3,2)):
            outer_to_inner.append(len(ring));a=angles[j];b=angles[(j+1)%3]+(math.tau if j==2 else 0)
            for step in range(count):
                angle=a+(b-a)*step/count
                ring.append(tuple(center[k]+hole_radius*(basis[k]*math.cos(angle)+side[k]*math.sin(angle)) for k in range(3)))
        for j in range(3):
            start=outer_to_inner[j];end=outer_to_inner[(j+1)%3]
            end=end if end>start else end+8
            a,b=outer[j],outer[(j+1)%3]
            p.mesh.triangle(a,b,ring[end%8],uvs=[(u0,v1),(u1,v1),(u1,v0)],color=WHITE)
            for k in range(end,start,-1): p.mesh.triangle(a,ring[k%8],ring[(k-1)%8],uvs=[(u0,v1),(u1,v0),(u0,v0)],color=WHITE)
        # Finger holes are capped recessed tunnels with visible dark walls.
        bottom=[tuple(center[k]-normal[k]*.020+(q[k]-center[k])*.78 for k in range(3)) for q in ring]
        for j in range(8):
            n=(j+1)%8
            p.mesh.quad(ring[j],ring[n],bottom[n],bottom[j],uv=dark,color=WHITE)
            deep=tuple(center[k]-normal[k]*.020 for k in range(3))
            p.mesh.triangle(deep,bottom[j],bottom[n],uvs=[(dark[0],dark[1]),(dark[2],dark[1]),(dark[2],dark[3])],color=WHITE)
        # A thin dark bevel keeps each circular mouth legible at native scale.
        # Offset closed stock avoids coincident edges with the sphere tunnel.
        rim=[]
        for offset,r in ((.001,.016),(.002,.016),(.001,.0128),(.002,.0128)):
            row=[]
            for q in ring:
                direction=normalize(tuple(q[k]-center[k] for k in range(3)))
                row.append(tuple(center[k]+normal[k]*offset+direction[k]*r for k in range(3)))
            rim.append(row)
        for j in range(8):
            n=(j+1)%8
            for a,b in ((0,1),(2,3),(0,2),(1,3)):
                p.mesh.quad(rim[a][j],rim[a][n],rim[b][n],rim[b][j],uv=dark,color=WHITE)
    finish(p,'muted polished eighty-face sphere with three actual recessed eight-sided finger holes')


def ball_return(p):
    t=atlas(p);grey=t.uv('grey',2);dark=t.uv('dark',2);metal=t.uv('metal',2)
    material(p,'return_plastic',.35)
    rounded_box(p,(0,.18,-.51),(.62,.36,.73),grey,.10)
    # Sculpted hood with an actual opening, a back wall and recessed channel.
    outer=rounded_outline(.86,.68,.24,2);inner=rounded_outline(.46,.34,.135,2)
    frame(p,(0,.72,-.33),outer,inner,.24,grey,inner_center=(0,.76))
    # A tapered, rounded rear body reads as a commercial return hood instead
    # of a square cap balanced above a black plinth.
    hood_rings=[]
    for y,w,d,z in ((.02,.60,.43,-.67),(.10,.65,.44,-.66),
                    (.60,.73,.44,-.66),(.98,.65,.38,-.65),(1.04,.55,.29,-.65)):
        hood_rings.append([(x,y,z+zz) for x,zz in rounded_outline(w,d,.08,2)])
    loft(p,hood_rings,grey)
    for x in (-.33,.33):rounded_box(p,(x,.70,-.57),(.14,.50,.34),grey,.04)
    rounded_box(p,(0,.61,-.385),(.37,.025,.39),dark,.025)
    material(p,'rack_metal',.30,metallic=.28)
    for x in (-.245,.245):
        beam(p,(x,.08,.81),(x,.47,.79),.055,.055,metal)
        p.tube_path([(x,.46,-.20),(x,.43,.05),(x,.41,.73),(x,.45,.92)],radii=.025,segments=6,uv=metal,color=WHITE,shades=False,cap_start=True)
    # Separate close inner rails support a 216 mm ball; the broad outer frame
    # remains the hand guard. Eight-sided tubes have real contact geometry.
    for x in (-.08,.08): cylinder(p,(x,.43,.03),.025,.84,metal,8,axis='z')
    for z in (.20,.77): beam(p,(-.25,.3975,z),(.25,.3975,z),.035,.035,metal)
    beam(p,(-.10,.452,.89),(.10,.452,.89),.035,.035,metal)
    rounded_box(p,(0,.025,.76),(.63,.05,.33),dark,.075)
    finish(p,'tapered rounded grey commercial hood, real dark throat, broad guard frame and close twin ball-support rails')


def ball_rack(p):
    t=atlas(p);material(p,'rack',.30,metallic=.25);metal=t.uv('metal',2)
    for x in (-.50,.50):
        for z in (-.25,.25): beam(p,(x,0,z),(x,.70,z),.04,.04,metal)
        beam(p,(x,.71,-.30),(x,.71,.30),.04,.04,metal)
    for y in (.21,.56):
        for z in (-.08,.08): beam(p,(-.54,y,z),(.54,y,z),.035,.035,metal)
    for x in (-.51,.51):
        box(p,(x,.022,0),(.08,.044,.64),t.uv('dark',2))
    finish(p,'two open tiers of metal ball rails with continuous end frames; balls are separate reusable props')


def score_monitor(p):
    t=atlas(p,'score_monitor',SCORE_REGIONS)
    material(p,'crt_housing',.46)
    body,bezel,metal=t.uv('body',2),t.uv('bezel',2),t.uv('metal',2)
    rounded_box(p,(0,.305,-.047),(.80,.61,.575),body,.065)
    rect_frame(p,(0,.305,.291),.76,.575,.065,.09,bezel)
    material(p,'bracket',.36,metallic=.25)
    beam(p,(0,.61,-.125),(0,1.065,-.125),.065,.07,metal)
    box(p,(0,.62,-.125),(.28,.035,.25),metal)
    box(p,(0,1.10,-.125),(.36,.04,.25),metal)
    material(p,'crt_glass',.13,emissive=(1,1,1),strength=1.45 if 'inactive' not in p.id else .025)
    curved_screen(p,(0,.305,.324),.624,.468,t.uv('inactive' if 'inactive' in p.id else 'screen',2),bulge=.014)
    finish(p,'deep rounded CRT, thick open bezel, convex blue score screen and heavy ceiling bracket')


def fixed_chair(p):
    t=atlas(p);shell=t.uv('red' if p.id.endswith('red') else 'blue',2)
    material(p,'molded_seat',.29)
    rounded_box(p,(0,.445,.02),(.60,.095,.56),shell,.12,3)
    contour=lambda w,d,z:[(x,z+zz) for x,zz in rounded_outline(w,d,.035,2)]
    rings=[[(x,y,z) for x,z in contour(w,d,cz)] for y,w,d,cz in
           ((.44,.54,.10,-.22),(.50,.58,.10,-.225),(.87,.58,.10,-.245),(.92,.52,.08,-.26))]
    loft(p,rings,shell)
    material(p,'seat_pedestal',.38,metallic=.30)
    cylinder(p,(0,0,0),.15,.035,t.uv('dark',2),12)
    cylinder(p,(0,.025,0),.043,.365,t.uv('metal',2),8)
    finish(p,'broad rounded molded bowl seat, thick upright back and fixed metal pedestal')


def pedestal_table(p):
    t=atlas(p);material(p,'laminate',.28)
    lathe(p,(0,0,0),[(.67,.425),(.68,.45),(.705,.45),(.72,.438)],t.uv('white',2),16)
    material(p,'table_pedestal',.35,metallic=.25)
    lathe(p,(0,0,0),[(0,.28),(.025,.30),(.045,.26),(.065,.065),(.67,.05)],t.uv('metal',2),12)
    finish(p,'round white laminate top with real edge bevel, single tapered stem and broad round foot')


def soda_bottle(p):
    t=atlas(p);material(p,'bottle_glass',.16,metallic=.04,alpha_mode='blend',opacity=.87)
    lathe(p,(0,0,0),[(0,.037),(.018,.0475),(.195,.0475),(.225,.030),(.247,.019),(.265,.019)],t.uv('bottle',2),12)
    material(p,'printed_label',.43)
    lathe(p,(0,0,0),[(.070,.048),(.157,.048)],t.uv('label',2),12)
    material(p,'bottle_cap',.34)
    lathe(p,(0,0,0),[(.263,.022),(.277,.022),(.28,.019)],t.uv('blue',2),12)
    if p.id.endswith('fallen'): transform_new(p,0,(0,0,0),x_angle=90)
    finish(p,'slightly translucent dark soda bottle, visible shoulder/neck, blue cap and period STRIKE COLA label')


def vending_machine(p):
    t=atlas(p,'vending_machine',VENDING_REGIONS);body,trim,dark,metal=[t.uv(n,2) for n in ('body','trim','dark','metal')]
    body_slot=material(p,'vending_body',.39)
    # Main stock ends behind the delivery cavity; the door is assembled around
    # its opening rather than covering a shallow black rectangle with a frame.
    padded_box(p,(0,1.015,-.10),(.96,2.03,.66),body,bevel=.022)
    for x in (-.456,.456): box(p,(x,1.015,.32),(.048,2.03,.22),body)
    for y in (.029,2.002): box(p,(0,y,.32),(.862,.056,.22),body)
    for x in (-.438,.438): box(p,(x,1.015,.416),(.012,1.91,.024),trim)
    for y in (.064,1.966): box(p,(0,y,.416),(.864,.014,.024),trim)
    # Large 8:5 soda logo and twelve individual 1:1 selection labels. Ordinary
    # door artwork below them has a separate restrained backlit face material.
    face_slot=material(p,'vending_illuminated',.23,emissive=(1,1,1),strength=1.10)
    illuminated_panel(p,(0,1.670,.392),(.864,.54,.036),t.uv('advert',1),body,face_slot,body_slot)
    p.begin_material(body_slot)
    box(p,(0,1.010,.318),(.864,.77,.164),body)
    for row,y in enumerate((1.255,1.005,.755)):
        for col,x in enumerate((-.337,-.176,.176,.337)):
            p.begin_material(body_slot)
            box(p,(x,y,.402),(.158,.225,.016),dark)
            illuminated_panel(p,(x,y,.414),(.146,.213,.010),t.uv(f'drink_{row*4+col}',1),
                              trim,face_slot,body_slot)
    material(p,'vending_controls',.45)
    box(p,(0,1.007,.419),(.166,.77,.014),{'+z':t.uv('controls',1),'-z':body,'+x':trim,'-x':trim,'+y':trim,'-y':trim})
    # Three depth cues supply a readable bill mouth, coin slot and return lever.
    box(p,(0,1.181,.429),(.10,.022,.008),dark)
    box(p,(.027,1.113,.430),(.012,.065,.008),dark)
    box(p,(0,.904,.429),(.060,.021,.008),metal)
    # Raised press bars read as real drink selectors even with Low textures;
    # their 7 mm relief is clear of the printed pane rather than coplanar.
    for y in (1.255,1.005,.755):
        for x in (-.337,-.176,.176,.337):
            box(p,(x,y-.102,.426),(.106,.018,.012),metal)
    lower_slot=material(p,'vending_lower_lit',.28,emissive=(1,1,1),strength=.38)
    # Keep the lower icy artwork registered across four closed door pieces.
    # Their common print frame includes the real 42 x 20 cm aperture.
    def lower_piece(cx,cy,w,h):
        u0,v0,u1,v1=t.uv('lower',0)
        # Protect the exact atlas boundary from sub-ulp negative roundoff.
        unit=lambda value: max(0,min(1,value))
        uv=(u0+unit((cx-w/2+.432)/.864)*(u1-u0),v0+unit((.620-cy-h/2)/.564)*(v1-v0),
            u0+unit((cx+w/2+.432)/.864)*(u1-u0),v0+unit((.620-cy+h/2)/.564)*(v1-v0))
        illuminated_panel(p,(cx,cy,.340),(w,h,.140),uv,body,lower_slot,body_slot)
    lower_piece(0,.535,.864,.169)
    lower_piece(0,.137,.864,.161)
    for x in (-.337,.337): lower_piece(x,.334,.190,.231)
    material(p,'vending_delivery',.34)
    rect_frame(p,(0,.334,.418),.484,.264,.032,.024,trim)
    box(p,(0,.334,.250),(.42,.20,.016),t.uv('dispense',1))
    rect_frame(p,(0,.334,.327),.44,.22,.01,.15,dark)
    box(p,(0,.237,.327),(.418,.006,.148),metal)
    box(p,(0,.033,.411),(.83,.038,.03),dark)
    # This single-node opaque static model needs five material primitives,
    # not one primitive for each alternation between a pane and its stock.
    # Group only its index runs; geometry, UVs and material ownership stay exact.
    runs=p.mesh.groups_for_range(0,len(p.mesh.indices))
    grouped_indices=[];grouped_materials=[]
    for run in sorted(runs,key=lambda entry: entry['material']):
        start=run['first_index'];count=run['index_count']
        grouped_indices.extend(p.mesh.indices[start:start+count])
        grouped_materials.extend([run['material']]*(count//3))
    p.mesh.indices=grouped_indices
    p.mesh._triangle_materials=grouped_materials
    finish(p,'concept-proportioned blue Bepsi cabinet, large soda globe/logo, twelve lit labels, central payment hardware and 15 cm delivery recess')


def cabinet_side(p,polygon,x0,x1,uv,body,art_slot=None,body_slot=None):
    """Closed profile shell: fitted poster caps and ordinary stock edge faces."""
    zmin,zmax=min(q[0] for q in polygon),max(q[0] for q in polygon)
    ymin,ymax=min(q[1] for q in polygon),max(q[1] for q in polygon)
    u0,v0,u1,v1=uv
    rows=[[(x,y,z) for z,y in polygon] for x in (x0,x1)]
    if art_slot is not None: p.begin_material(art_slot)
    for row in rows:
        for ids in _ear_triangles(polygon):
            coords=[]
            for i in ids:
                _,y,z=row[i];s=(z-zmin)/(zmax-zmin)
                if x0>=0:s=1-s
                coords.append((u0+s*(u1-u0),v1-(y-ymin)/(ymax-ymin)*(v1-v0)))
            p.mesh.triangle(*(row[i] for i in ids),uvs=coords,color=WHITE)
    if body_slot is not None:p.begin_material(body_slot)
    for i in range(len(polygon)):
        j=(i+1)%len(polygon)
        p.mesh.quad(rows[0][i],rows[0][j],rows[1][j],rows[1][i],uv=body,color=WHITE)


def joystick(p,x,y,z,metal,dark):
    cylinder(p,(x,y,z),.026,.010,metal,8)
    cylinder(p,(x,y+.009,z),.011,.07,metal,6)
    sphere(p,(x,y+.086,z),(.029,.029,.029),dark,0)


def cabinet(p):
    kind=p.id.split('_')[-1];w,h,d=SIZES['cabinet_'+kind]
    t=atlas(p,'cabinet_'+kind,CABINET_REGIONS);body,metal,dark=[t.uv(n,2) for n in ('body','metal','dark')]
    body_slot=material(p,'cabinet_body',.52)
    side_settings={'uv_response':.6,'uv_response_mask':(ROOT/'cabinet_scifi_uv_mask.png').read_bytes()} if kind=='scifi' else {}
    side_slot=p.material('arcade_cabinet_side_print',roughness=.52,**side_settings)
    f=d/2;b=-d/2
    deck=.94 if kind!='scifi' else 1.01
    # Distinct cap, screen and deck slopes preserve recognizable cabinet profiles.
    screen_top=1.60 if kind=='shooter' else 1.55
    screen_z=f-(.23 if kind=='shooter' else .30)
    polygon=[(b,0),(f-.14,0),(f-.10,deck-.10),(f,deck),
             (f-.03,deck+.13),(screen_z,screen_top),(f-.12,h-.24),(f-.12,h),(b+.03,h),(b,h-.08)]
    for x0,x1 in ((-w/2,-w/2+.042),(w/2-.042,w/2)):
        cabinet_side(p,polygon,x0,x1,t.uv('side',2),body,side_slot,body_slot)
    # Narrow centre carcass stays behind the recessed CRT and controls.
    box(p,(0,.43,(b+f-.15)/2),(w-.085,.86,d-.15),body)
    box(p,(0,(h+.9)/2,b+.055),(w-.085,h-.9,.11),body)
    box(p,(0,h-.15,(b+f-.15)/2),(w-.085,.27,d-.15),body)
    box(p,(0,.04,0),(w-.08,.08,d-.10),dark)
    # Slanted control panel with fitted printed art; hardware is independent.
    tilted_box(p,(0,deck+.03,f-.155),(w-.10,.045,.32),t.uv('control',2),angle=10)
    box(p,(0,.46,f-.092),(.27,.29,.02),{'+z':t.uv('coin',2),'-z':body,'+x':metal,'-x':metal,'+y':metal,'-y':metal})
    box(p,(.014,.48,f-.077),(.105,.012,.012),dark)
    box(p,(.0,.365,f-.065),(.10,.035,.024),metal)
    # Deep CRT bezel, pitched with the glass, never a modern flat display.
    center=(0,1.345,f-.235)
    start=len(p.mesh.positions)
    rect_frame(p,center,w-.095,.54,.065,.12,dark)
    transform_new(p,start,center,x_angle=-16 if kind!='shooter' else -11)
    material(p,'cabinet_screen',.14,emissive=(1,1,1),strength=1.45)
    curved_screen(p,(0,1.345,f-.174),w-.23,(w-.23)*.75,t.uv('screen',2),tilt=-16 if kind!='shooter' else -11,bulge=.010)
    marquee_slot=material(p,'cabinet_marquee',.31,emissive=(1,1,1),strength=1.65)
    illuminated_panel(p,(0,h-.15,f-.089),(w-.105,.175,.035),t.uv('marquee',2),body,marquee_slot,body_slot)
    material(p,'control_hardware',.32,metallic=.15)
    if kind=='racing':
        # Real ring wheel with three spokes, broad driving deck and pedals.
        outer=[(.145*math.cos(math.tau*i/12),.145*math.sin(math.tau*i/12)) for i in range(12)]
        inner=[(.116*math.cos(math.tau*i/12),.116*math.sin(math.tau*i/12)) for i in range(12)]
        frame(p,(0,deck+.18,f-.05),outer,inner,.035,dark)
        for angle in (90,210,330):
            a=math.radians(angle)
            beam(p,(0,deck+.18,f-.05),(.12*math.cos(a),deck+.18+.12*math.sin(a),f-.05),.018,.018,metal)
        cylinder(p,(0,deck+.18,f-.087),.035,.05,metal,8,axis='z')
        for x in (-.11,.11): tilted_box(p,(x,.11,f-.11),(.09,.025,.14),metal,angle=-15)
    elif kind=='scifi':
        sphere(p,(-.12,deck+.088,f-.13),(.062,.045,.062),t.uv('white',2),1)
        for x in (.13,.23): cylinder(p,(x,deck+.065,f-.18),.021,.015,metal,8)
    else:
        xs=(-.25,.16) if kind=='fighting' else (-.17,)
        for x in xs: joystick(p,x,deck+.067,f-.14,metal,dark)
        clusters=(.0,.32) if kind=='fighting' else (.11,)
        for cx in clusters:
            for j in range(6 if kind=='fighting' else 3):
                x=cx+(j%3-.8)*.047;z=f-.12-(j//3)*.055
                cylinder(p,(x,deck+.065,z),.016,.014,t.uv('white',2),6 if kind=='fighting' else 8)
    finish(p,f'distinct {kind} commercial cabinet silhouette, fitted side art, thick tilted CRT, original game screen, printed marquee and raised controls')


def pinball(p):
    name=p.id.split(':')[-1];t=atlas(p,name,PINBALL_REGIONS)
    body,metal,dark=[t.uv(n,2) for n in ('body','metal','dark')]
    body_slot=material(p,'pinball_body',.48)
    # Sloped cabinet is a wedge, with an elevated rear and four independent legs.
    contour=[(-.63,.965),(.63,.72),(.63,.86),(-.63,1.11)]
    cabinet_side(p,contour,-.35,.35,t.uv('side',2),body)
    for x in (-.30,.30):
        for z in (-.54,.54): beam(p,(x,0,z),(x*.91,.80 if z>0 else 1.01,z*.94),.036,.036,metal)
    padded_box(p,(0,1.51,-.63),(.74,.74,.16),body,bevel=.01)
    # Real dense printed playfield remains non-emissive under a thin glass cover.
    tilted_box(p,(0,.999,0),(.62,.012,1.14),{'+y':t.uv('playfield',2),'-y':dark,'+x':metal,'-x':metal,'+z':metal,'-z':metal},angle=11.2)
    for x in (-.326,.326):
        beam(p,(x,.905,.61),(x,1.154,-.61),.030,.090,body)
        beam(p,(x,.949,.61),(x,1.198,-.61),.026,.03,metal)
    box(p,(0,.905,.647),(.70,.13,.025),{'+z':t.uv('apron',2),'-z':body,'+x':metal,'-x':metal,'+y':metal,'-y':metal})
    cylinder(p,(.255,.813,.694),.018,.017,metal,8,axis='z')
    for x,z in ((-.17,-.16),(.17,-.16),(0,-.36)):
        y=.999-.194*z+.015
        cylinder(p,(x,y,z),.045,.036,metal,8)
        cylinder(p,(x,y+.036,z),.036,.012,t.uv('white',2),8)
    # Raised flippers occupy the lower end of the actual sloped playfield.
    for x,angle in ((-.11,-21),(.11,21)):
        y=.999-.194*.39+.024
        start=len(p.mesh.positions)
        rounded_box(p,(x,y,.39),(.14,.02,.04),t.uv('white',2),.017,2)
        transform_new(p,start,(x,y,.39),z_angle=angle)
    glass_slot=material(p,'pinball_backglass',.18,emissive=(1,1,1),strength=1.40)
    illuminated_panel(p,(0,1.625,-.541),(.64,.40,.023),t.uv('backbox',2),body,glass_slot,body_slot)
    score_slot=material(p,'pinball_score',.25,emissive=(1,1,1),strength=1.15)
    illuminated_panel(p,(0,1.26,-.54),(.62,.12,.024),t.uv('score',2),body,score_slot,body_slot)
    material(p,'pinball_lamps',.25,emissive=(1,1,1),strength=2.20)
    for x,z in ((-.235,-.38),(.235,-.38),(-.21,.20),(.21,.20),(0,.30)):
        cylinder(p,(x,.999-.194*z+.021,z),.012,.008,t.uv('white',2),6)
    material(p,'pinball_glass',.10,alpha_mode='blend',opacity=.12)
    tilted_box(p,(0,1.064,0),(.62,.004,1.14),t.uv('white',2),angle=11.2)
    finish(p,'sloped glass playfield, printed targets, three physical bumpers, paired flippers, five small lamps, illustrated backbox and score, four metal legs')


def change_machine(p):
    t=atlas(p,'utility',UTILITY_REGIONS);material(p,'change_metal',.56,metallic=.12)
    padded_box(p,(0,.73,-.015),(.52,1.46,.45),t.uv('body',2),bevel=.008)
    box(p,(0,1.045,.219),(.44,.44,.018),{'+z':t.uv('change',2),'-z':t.uv('body',2),'+x':t.uv('metal',2),'-x':t.uv('metal',2),'+y':t.uv('metal',2),'-y':t.uv('metal',2)})
    rect_frame(p,(0,.385,.218),.36,.18,.035,.04,t.uv('metal',2))
    box(p,(0,.385,.178),(.30,.12,.015),t.uv('dark',2))
    box(p,(0,.74,.229),(.17,.03,.015),t.uv('dark',2))
    box(p,(0,.715,.234),(.20,.008,.012),t.uv('metal',2))
    finish(p,'ordinary dull utility metal change machine, printed instructions, bill slot and recessed coin tray; zero emission')


def neon_bowl(p):
    t=atlas(p,'utility',UTILITY_REGIONS);material(p,'neon_backing',.70)
    box(p,(0,.43,-.0225),(1.82,.86,.065),t.uv('backing',2))
    def tube(points,region,strength=2.2):
        material(p,'neon_'+region,.27,emissive=(1,1,1),strength=strength)
        p.tube_path([(x,y,.060) for x,y in points],radii=.014,segments=4,uv=t.uv(region,2),color=WHITE,shades=False,cap_start=True)
    tube([(.72,.24),(.72,.73),(-.80,.73),(-.80,.14),(.72,.14)],'blue')
    # Letters are actual red tubes, with the authentic small terminal gaps.
    x=-.65;y=.28;h=.36
    tube([(x,y),(x,y+h)],'red')
    tube([(x+.015,y+h),(x+.19,y+h),(x+.24,y+.29),(x+.22,y+.22),(x+.06,y+.19),(x+.21,y+.17),(x+.24,y+.08),(x+.19,y),(x+.015,y)],'red')
    cx=-.14;cy=y+h/2
    tube([(cx+.14*math.cos((math.tau-.10)*i/16+.06),cy+.18*math.sin((math.tau-.10)*i/16+.06)) for i in range(17)],'red')
    tube([(.055,y+h),(.115,y),(.215,y+.24),(.315,y),(.375,y+h)],'red')
    tube([(.47,y+h),(.47,y),(.68,y)],'red')
    star=[]
    for i in range(11):
        a=math.pi/2+math.tau*i/10;r=.10 if i%2==0 else .042
        star.append((.78+r*math.cos(a),.76+r*math.sin(a)))
    star[-1]=(.78+.007,.86)
    tube(star,'yellow',2.0)
    finish(p,'red BOWL tube lettering, blue tube border and yellow star; backing never emits')


def blacklight_fixture(p):
    t=atlas(p,'utility',UTILITY_REGIONS);material(p,'uv_fixture_hardware',.49,metallic=.2)
    box(p,(0,.085,0),(1.24,.04,.13),t.uv('body',2))
    for x in (-.578,.578): box(p,(x,.0475,0),(.056,.095,.065),t.uv('metal',2))
    material(p,'uv_tube',.22,emissive=(1,1,1),strength=.14)
    cylinder(p,(-.568,.028,0),.012,1.136,t.uv('tube',2),8,axis='x')
    finish(p,'old fluorescent UV tube and real metal mounting channel, subtle visible violet glass emission')


def cat_plush(p):
    t=atlas(p,'cat_plush',CAT_REGIONS);black,white,eye,nose=[t.uv(n,2) for n in ('black','white','eye','nose')]
    material(p,'plush_fabric',.92)
    lathe(p,(0,0,-.018),[(0,.047),(.035,.083),(.115,.090),(.175,.071),(.218,.043)],black,10,ellipse=(1,.88))
    sphere(p,(0,.255,.024),(.10,.073,.083),black,1)
    for x in (-.068,.068): sphere(p,(x,.027,.061),(.042,.027,.046),white,0)
    sphere(p,(0,.132,.056),(.057,.069,.026),white,0)
    for x in (-.036,.036): sphere(p,(x,.253,.094),(.052,.041,.016),white,0)
    # Upright triangular ears are closed fabric tetrahedra, not flat alpha cards.
    for sign in (-1,1):
        vertices=[(sign*.039,.304,-.003),(sign*.09,.295,.02),(sign*.076,.36,.006),(sign*.058,.306,.044)]
        for a,b,c in ((0,1,2),(0,3,1),(0,2,3),(1,3,2)):
            p.mesh.triangle(vertices[a],vertices[b],vertices[c],uvs=[(black[0],black[1]),(black[2],black[1]),(black[2],black[3])],color=WHITE)
    for x in (-.034,.034): sphere(p,(x,.272,.109),(.008,.010,.005),eye,0)
    sphere(p,(0,.245,.113),(.008,.006,.005),nose,0)
    p.tube_path([(.058,.053,-.074),(.101,.044,-.081),(.117,.085,-.058)],radii=.018,segments=5,uv=black,color=WHITE,shades=False,cap_start=True)
    finish(p,'small seated ordinary tuxedo plush, soft black body, white face/chest/paws, upright ears and dark embroidered eyes; no emission')


def litter(p):
    name=p.id.split(':')[-1];t=atlas(p);material(p,'litter',.70)
    if name=='litter_can':
        lathe(p,(0,0,0),[(0,.026),(.006,.033),(.112,.033),(.12,.026)],t.uv('label',2),10,cap_uv=t.uv('metal',2))
    elif name=='litter_cup':
        # Continuous outer wall, real lip, inner wall and shallow inside floor.
        lathe(p,(0,0,0),[(0,.030),(.006,.034),(.136,.045),(.14,.045),(.14,.040),(.020,.028)],t.uv('paper',2),10)
    elif name=='litter_wrapper':
        rings=[[(x,y,z) for x,y,z in ((-.06,0,-.035),(.045,.002,-.0425),(.06,0,.026),(-.045,.003,.0425))],
               [(x,y,z) for x,y,z in ((-.06,.006,-.035),(.045,.016,-.0425),(.06,.007,.026),(-.045,.011,.0425))]]
        loft(p,rings,t.uv('label',2))
    elif name=='litter_paper':
        rings=[[(x,y,z) for x,y,z in ((-.105,0,-.074),(.105,0,-.074),(.105,0,.074),(-.105,0,.074))],
               [(x,y,z) for x,y,z in ((-.105,.002,-.074),(.105,.004,-.074),(.105,.012,.074),(-.105,.009,.074))]]
        loft(p,rings,t.uv('paper',2))
    finish(p,'small ordinary discarded object, restrained colour and zero emission')


def gutter_segment(p):
    t=atlas(p);material(p,'gutter_plastic',.42)
    # A real concave curved channel, with closed outer stock and end caps.
    contour=[]
    for radius,steps in ((.12,range(13)),(.135,range(12,-1,-1))):
        for i in steps:
            a=math.pi*i/12
            contour.append((radius*math.cos(a),.14-radius*math.sin(a)))
    extrude(p,contour,-1.5,1.5,t.uv('dark',2))
    finish(p,'three-metre sealed curved gutter, 27 cm outer width, 13.5 cm high, true concave dark channel')


def lane_divider(p):
    t=atlas(p);material(p,'divider',.36)
    padded_box(p,(0,.07,0),(.075,.14,3.0),t.uv('grey',2),bevel=.007)
    finish(p,'three-metre narrow commercial lane divider with bevelled top edges')


PROPS={
    **{'arcade:'+n:bowling_ball for n in SIZES if n.startswith('bowling_ball_')},
    **{'arcade:'+n:fixed_chair for n in ('fixed_chair_red','fixed_chair_blue')},
    **{'arcade:'+n:score_monitor for n in ('score_monitor','score_monitor_inactive')},
    **{'arcade:'+n:cabinet for n in SIZES if n.startswith('cabinet_')},
    **{'arcade:'+n:pinball for n in ('pinball_cosmic','pinball_comet')},
    **{'arcade:'+n:litter for n in ('litter_can','litter_cup','litter_wrapper','litter_paper')},
    'arcade:bowling_pin':bowling_pin,'arcade:ball_return':ball_return,'arcade:ball_rack':ball_rack,
    'arcade:pedestal_table':pedestal_table,'arcade:soda_bottle':soda_bottle,
    'arcade:vending_machine':vending_machine,'arcade:change_machine':change_machine,
    'arcade:neon_bowl':neon_bowl,'arcade:blacklight_fixture':blacklight_fixture,
    'arcade:cat_plush':cat_plush,'arcade:litter_bottle_fallen':soda_bottle,
    'arcade:gutter_segment':gutter_segment,'arcade:lane_divider':lane_divider,
}
