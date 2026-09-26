"""Closed pool rail stock and thin, pleated curtain shells using source PNGs."""
import math
from pathlib import Path
from geometry import inspect
from parts.refreshed import load_atlas_from, solid_box, solid_cylinder, outward_lathe

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
        for y in (.525,.98):
            solid_cylinder(p,(x,y,z),.021,2*limit,axis=axis,segments=8,
                           uv=t.uv('rail',inset=1),color=WHITE)
    p.add_note('matching upper and middle rails at 0.98/0.525 m; both legs of corner carry both rails')


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
