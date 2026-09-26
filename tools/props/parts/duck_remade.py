"""Low-poly moulded rubber duck with a continuous body/neck/head shell."""
import math
from pathlib import Path
from geometry import inspect
from mesh import _rotate
from parts.refreshed import load_atlas_from, outward_lathe, solid_cylinder
from parts.domestic_remade import loft, section


def build(p):
    path=Path(__file__).resolve().parents[3]/'assets/environment/pool/props/models/rubber_duck.png'
    t=load_atlas_from(p,path,('body','head','beak','eye'))
    # (height, half-width, half-depth, forward centre). Neck flows into the head.
    profile=[(0,.026,.031,-.012),(.012,.043,.049,-.012),(.035,.050,.051,-.012),
             (.053,.043,.043,-.008),(.066,.024,.025,.009),(.077,.024,.026,.024),
             (.093,.031,.029,.025),(.108,.028,.027,.025),(.118,.017,.017,.024),
             (.120,.007,.008,.024)]
    segments=12
    rings=[[(rx*math.cos(i*2*math.pi/segments),y,z+rz*math.sin(i*2*math.pi/segments))
            for i in range(segments)] for y,rx,rz,z in profile]
    for r,(a,b) in enumerate(zip(rings,rings[1:])):
        uv=t.uv('body' if r<4 else 'head',inset=3)
        u0,v0,u1,v1=uv
        for i in range(segments):
            j=(i+1)%segments
            coords=[(u0+(u1-u0)*u/segments,v) for u,v in ((i,v1),(j if j else segments,v1),(j if j else segments,v0),(i,v0))]
            p.mesh.quad(a[i],a[j],b[j],b[i],uv=coords,color=(255,255,255),
                        shade_mult=.86+.11*math.sin((i+.5)*2*math.pi/segments))
    for ring,region in ((rings[0],'body'),(rings[-1],'head')):
        center=tuple(sum(v[k] for v in ring)/segments for k in range(3))
        u0,v0,u1,v1=t.uv(region,inset=3)
        for i in range(segments):
            p.mesh.triangle(center,ring[i],ring[(i+1)%segments],
                            uvs=[((u0+u1)/2,(v0+v1)/2)]+[
                                ((u0+u1)/2+math.cos(j*2*math.pi/segments)*(u1-u0)/2,
                                 (v0+v1)/2+math.sin(j*2*math.pi/segments)*(v1-v0)/2)
                                for j in (i,i+1)],color=(255,255,255))
    # Flattened, broad toy bill with a rounded tip and a narrow moulded edge.
    loft(p,[section(.045,0,.085,.031,.009),section(.058,0,.084,.041,.010),
            section(.072,0,.082,.034,.007),section(.076,0,.082,.023,.004)],t.uv('beak',inset=3))
    # Tail rises out of the rump, broad at its root and pinched at the tip.
    loft(p,[section(-.040,0,.048,.035,.019),section(-.058,0,.060,.027,.018),
            section(-.069,0,.068,.020,.007)],t.uv('body',inset=3))
    for side in (-1,1):
        # Shallow moulded wings; no loose feather shapes or texture planes.
        outward_lathe(p,(side*.046,.023,-.013),[(0,.004),(.006,.013),(.016,.014),(.024,.006)],
                      segments=8,ellipse=(.33,1.6),uv=t.uv('body',inset=3),
                      cap_start=True,cap_end=True,color=(255,244,205))
        first=len(p.mesh.positions)
        solid_cylinder(p,(0,0,0),.0038,.0012,segments=10,axis='z',
                       uv=t.uv('eye',inset=8),color=(255,255,255))
        for i in range(first,len(p.mesh.positions)):
            x,y,z=_rotate(p.mesh.positions[i],(0,side*55,0))
            p.mesh.positions[i]=(x+side*.024,y+.102,z+.0398)
    inspect(p.mesh.positions,p.mesh.indices,repair=True)
    p.mesh.normalize_origin()
    p.add_note('continuous body/neck/head, raised broad tail, flattened bill, moulded wings and closed inset eye discs')
