"""Explicit Backrooms bootstrap. Existing saved project JSON remains editable.
Run with --root and --wallpaper; no other project or existing asset is regenerated.
"""
from pathlib import Path
import argparse, json, math, sys, shutil, hashlib
from PIL import Image
p=argparse.ArgumentParser()
p.add_argument("--root",type=Path,required=True)
p.add_argument("--wallpaper",type=Path,required=True)
p.add_argument("--evidence",type=Path,required=True)
args=p.parse_args(); R=args.root.resolve(); O=args.evidence.resolve(); O.mkdir(parents=True,exist_ok=True)
sys.path.insert(0,str(R/"tools/props"))
from mesh import Mesh
from glb import write_glb
catpath=R/"assets/catalog.json"; cat=json.loads(catpath.read_text())
existing={a["id"]:a for a in cat["assets"]}
kit=R/"assets/environments/backrooms"; (kit/"textures/walls").mkdir(parents=True,exist_ok=True); (kit/"props").mkdir(exist_ok=True)
# Uniform resampling enforces the source-resolution contract; no imagery is painted in code.
Image.open(args.wallpaper).convert("RGB").resize((1024,1024),Image.Resampling.LANCZOS).save(kit/"textures/walls/chevron_01.png")
white=R/"assets"/existing["core:tex_white_01"]["model"]
Image.open(white).resize((64,64),Image.Resampling.LANCZOS).save(kit/"props/palette.png")
png=(kit/"props/palette.png").read_bytes(); uv=(0.,0.,1.,1.); wood=(117,91,57); green=(51,83,65)
class BackroomsMesh(Mesh):
 """Orient only new Backrooms convex primitives; shared accepted assets are untouched."""
 def orient(self, start, centre):
  for i in range(start,len(self.indices),3):
   a,b,c=[self.positions[self.indices[i+j]] for j in range(3)]
   u=[b[j]-a[j] for j in range(3)];v=[c[j]-a[j] for j in range(3)]
   n=[u[1]*v[2]-u[2]*v[1],u[2]*v[0]-u[0]*v[2],u[0]*v[1]-u[1]*v[0]]
   dot=sum(n[j]*((a[j]+b[j]+c[j])/3-centre[j]) for j in range(3))
   assert abs(dot)>1e-10,'degenerate new prop primitive'
   if dot<0:self.indices[i+1],self.indices[i+2]=self.indices[i+2],self.indices[i+1]
 def box(self,center,size,**kw):
  start=len(self.indices);super().box(center,size,**kw);self.orient(start,center)
 def cylinder(self,base,radius,height,**kw):
  assert kw.get('axis','y')=='y'
  start=len(self.indices);super().cylinder(base,radius,height,**kw)
  self.orient(start,[base[0],base[1]+height/2,base[2]])

def chair():
 m=BackroomsMesh()
 for x in (-.22,.22):
  for z in (-.22,.22):m.box((x,.215,z),(.06,.43,.06),uv=uv,color=wood)
 m.box((0,.465,0),(.5,.07,.5),uv=uv,color=green)
 for x in (-.22,.22):m.box((x,.72,-.22),(.06,.44,.06),uv=uv,color=wood)
 for y in (.58,.72,.88):m.box((0,y,-.22),(.38,.055,.055),uv=uv,color=wood)
 return m
def stool():
 m=BackroomsMesh()
 for x in (-.14,.14):
  for z in (-.14,.14):m.box((x,.275,z),(.045,.55,.045),uv=uv,color=wood)
 m.cylinder((0,.55,0),.21,.07,segments=12,uv=uv,color=green,bottom=True)
 return m
def vent():
 # Ceiling-origin square frame with a recessed circular grille, no painted imagery.
 m=BackroomsMesh();offwhite=(202,202,184);dark=(65,70,63)
 for x,z,w,d in [(-.38,0,.04,.8),(.38,0,.04,.8),(0,-.38,.72,.04),(0,.38,.72,.04)]:
  m.box((x,-.025,z),(w,.05,d),uv=uv,color=offwhite)
 m.cylinder((0,-.05,0),.34,.025,segments=16,uv=uv,color=dark,bottom=True)
 for z in [-.28,-.21,-.14,-.07,0,.07,.14,.21,.28]:
  width=2*math.sqrt(.33**2-z**2)
  m.box((0,-.058,z),(width,.016,.025),uv=uv,color=offwhite)
 return m
new=[]
def entry(id,typ,**kw):new.append(dict(id="backrooms:"+id,display_name=id.replace("_"," ").title(),asset_class="environment",asset_type=typ,source="file" if typ in ("texture","prop") else "definition",**kw))
if not any(t['id']=='backrooms' for t in cat['themes']):cat['themes'].append(dict(id='backrooms',display_name='The Backrooms',description='Matte institutional chevron wallpaper, green-seated timber furniture and fluorescent circulation.'))
entry("tex_chevron_01","texture",model="environments/backrooms/textures/walls/chevron_01.png",surface="wall")
for id,tex,tint,extra in [
 ("chevron_01","backrooms:tex_chevron_01",[1.,1.,.94],{}),
 ("fine_wall_01","core:tex_wallpaper_yellow_01",[.94,.92,.78],{}),
 ("plain_cream_01","home:tex_wall_paint_offwhite_01",[1.,.99,.92],{}),
 ("ochre_01","home:tex_wall_paint_offwhite_01",[.97,.91,.72],{}),
 ("cool_cream_01","home:tex_wall_paint_offwhite_01",[.79,.86,.79],{})]:
 entry(id,"material",texture=tex,surface="wall",tile_metres=6. if id=="chevron_01" else 2.,tint=tint,specular=0.,shine=0.,**extra)
for id,tex,surface,tint in [
 ("acoustic_cream_01","core:tex_ceiling_panel_01","ceiling",[1.,1.,.98]),
 ("carpet_olive_01","core:tex_carpet_damp_01","floor",[.96,1.,.85]),
 ("carpet_tan_01","core:tex_carpet_beige_01","floor",[1.,.97,.87]),
 ("carpet_cream_01","home:tex_carpet_cream_01","floor",[1.,1.,.97]),
 ("carpet_cool_01","home:tex_carpet_cream_01","floor",[.84,.92,.87]),
 ("wood_cap_01","home:tex_handrail_wood_01","wall",[.44,.36,.28])]:
 entry(id,"material",texture=tex,surface=surface,tile_metres=2.,tint=tint,specular=0.,shine=0.)
ledgerpath=R/"assets/completed-models.json"; ledger=json.loads(ledgerpath.read_text())
for name,mesh in [("ladder_chair",chair()),("round_stool",stool()),("ceiling_vent",vent())]:
 (kit/"props"/(name+".glb")).write_bytes(write_glb(mesh,png,name))
 low,high=mesh.bounds(); size=[round(b-a,6) for a,b in zip(low,high)]
 model="environments/backrooms/props/"+name+".glb"
 entry(name,"prop",model=model,size=size,color="#cacaB8" if name=="ceiling_vent" else "#75613f",category="Fixtures" if name=="ceiling_vent" else "Furniture",solid=name!="ceiling_vent",authoring={"tool":"completed","source":"completed-models.json"})
 ledger[model]=dict(sha256=hashlib.sha256((R/"assets"/model).read_bytes()).hexdigest(),triangles=len(mesh.indices)//3,texture_edge=64,identities=["backrooms:"+name],bounds_min=low,bounds_max=high)
ledgerpath.write_text(json.dumps(ledger,indent=2)+"\n",encoding="utf-8")
for asset in new:asset['theme']='backrooms'
cat["assets"]=[a for a in cat["assets"] if not a["id"].startswith("backrooms:")]+new
catpath.write_text(json.dumps(cat,indent=2)+"\n",encoding="utf-8")
M="backrooms:chevron_01"; plain="backrooms:plain_cream_01"
scene=dict(schema="places-level",format_version=1,id="the_backrooms",name="The Backrooms",author="Places",
 spawn=dict(x=30,z=9,yaw_degrees=180),
 defaults=dict(wall=M,floor="core:carpet_beige_01",ceiling="backrooms:acoustic_cream_01"),
 rooms=[],walls=[],columns=[],half_walls=[],baseboards=[],stairs=[],props=[],ceiling_lights=[],decals=[])
scene["environment"]={"presentation":{"exposure":2.8,"tone_knee":.8,"saturation":.95,"contrast":.9}}
scene['environment']['fog']=dict(color=[.47,.46,.37],density=.0025,reference_y=0,height_gain=0)
H=3.4; UP=3.55; T=.3
zones={"A":(0,0,60,60,0),"B":(60,0,80,60,0),"C":(60,60,80,80,0),"D":(0,60,60,80,0),
"E":(8,29.6,48,18,UP),"F":(8,100,10,10,UP),"G":(90,22,44,26,UP),"H":(80,88,54,24,UP)}
names=["Chevron Weave","Offset Colonnade","Pillar Field","Half-wall Reception","Chair Gallery","Stool Court","Recess Hall","Office Gallery"]
stairs=[(12,24,2.4,5.6),(94,16.4,2.4,5.6),(82,82.4,2.4,5.6),(10,94.4,2.4,5.6)]
shafts=[(x-.35,z-.35,w+.7,d+.35) for x,z,w,d in stairs]
def room(x,z,w,d,y=0,h=H,comment=""):
 # Same geometry/density; half-page chart spans pack tightly in the existing atlas budget.
 for ix in range(math.ceil(w/31.5)):
  for iz in range(math.ceil(d/31.5)):
   xx=x+ix*31.5;zz=z+iz*31.5
   if min(31.5,x+w-xx)<.001 or min(31.5,z+d-zz)<.001:continue
   scene["rooms"].append(dict(x=xx,z=zz,width=min(31.5,x+w-xx),depth=min(31.5,z+d-zz),floor_y=y,height=h,comment=comment))
def subtract(a,b):
 x,z,w,d=a; xx,zz,ww,dd=b; a1=x+w;a2=z+d;b1=xx+ww;b2=zz+dd
 ix=max(x,xx); iz=max(z,zz); ex=min(a1,b1); ez=min(a2,b2)
 if ix>=ex or iz>=ez:return [a]
 return [q for q in [(x,z,ix-x,d),(ex,z,a1-ex,d),(ix,z,ex-ix,iz-z),(ix,ez,ex-ix,a2-ez)] if q[2]>.001 and q[3]>.001]
for k in "ABCD":
 x,z,w,d,y=zones[k]; patches=[(x,z,w,d)]
 for s in shafts:patches=[v for q in patches for v in subtract(q,s)]
 for q in patches:room(*q,comment=k+" "+names[ord(k)-65])
for i,s in enumerate(stairs):
 room(*shafts[i],h=UP+H,comment="Vertical link "+["A-E","B-G","C-H","D-F"][i])
 scene["stairs"].append(dict(x=s[0],z=s[1],width=s[2],depth=s[3],rise=UP,steps=20,material="core:carpet_beige_01",riser_material=plain,side_material=plain))
# Upper rings weave through central cross corridors; two entries per gallery remain open.
upper=[(*zones[k][:4],k) for k in "EFGH"]
upper += [(4,29.6,4,92.4,"EF"),(4,118,34,4,"FG"),(34,68,4,54,"FG"),(34,68,52,4,"FG"),(82,22,4,50,"FG"),(82,22,8,4,"FG"),
 (134,22,4,90,"GH"),(56,52,36,4,"HE"),(88,52,4,36,"HE"),(52,43.6,4,12.4,"HE"),(56,43.6,4,12.4,"HE")]
# E/F west links, G east two links, H east link, and E/H cross-link.
upper += [(4,32,8,4,"EF"),(4,42,8,4,"EF"),(4,102,8,4,"EF"),(4,118,8,4,"EF"),
 (130,26,8,4,"GH"),(130,42,8,4,"GH"),(130,92,8,4,"GH"),(130,108,8,4,"GH")]
upper += [(18,102,20,4,"FG"),(110,48,28,4,"GR")]
xs=sorted({q[0] for q in upper+shafts}|{q[0]+q[2] for q in upper+shafts}); zs=sorted({q[1] for q in upper+shafts}|{q[1]+q[3] for q in upper+shafts})
cells={}
for i in range(len(xs)-1):
 for j in range(len(zs)-1):
  cx=(xs[i]+xs[i+1])/2;cz=(zs[j]+zs[j+1])/2
  hit=next((q for q in upper if q[0]<=cx<q[0]+q[2] and q[1]<=cz<q[1]+q[3]),None)
  if hit and not any(s[0]<cx<s[0]+s[2] and s[1]<cz<s[1]+s[3] for s in shafts):cells[i,j]=hit[4]
# Merge same-owner adjacent grid intervals into disjoint editable rectangles.
used=set()
for (i,j),owner in cells.items():
 if (i,j) in used:continue
 ei=i+1
 while cells.get((ei,j))==owner and (ei,j) not in used:ei+=1
 ej=j+1
 while ej<len(zs)-1 and all(cells.get((ii,ej))==owner and (ii,ej) not in used for ii in range(i,ei)):ej+=1
 for ii in range(i,ei):
  for jj in range(j,ej):used.add((ii,jj))
 room(xs[i],zs[j],xs[ei]-xs[i],zs[ej]-zs[j],UP,comment=owner+" upper circulation")
def wall(x,z,w,d,y=0,h=H,mat=M,openings=None):
 if w<.001 or d<.001:return
 q=dict(x=x,z=z,width=w,depth=d,y=y,height=h,material=mat)
 if openings:q["openings"]=openings
 scene["walls"].append(q)
def passage(offset,width=3.6,height=2.6,sill=0):
 return dict(offset=offset,width=width,height=height,sill=sill,kind="passage")
# One owner per lower wall span; exterior seams meet edge-to-edge.
wall(-T,-T,140+2*T,T)
wall(-T,140,60+T,T,mat=plain);wall(60,140,80+T,T,mat="backrooms:ochre_01")
wall(-T,0,T,60);wall(-T,60,T,80,mat=plain)
wall(140,0,T,60);wall(140,60,T,80,mat="backrooms:ochre_01")
wall(59.85,0,T,60,openings=[passage(12),passage(42)])
wall(59.85,60,T,80,openings=[passage(12),passage(58)])
scene['walls'][-1]['faces']={'west':plain,'east':'backrooms:ochre_01'}
wall(0,59.85,59.85,T,openings=[passage(12),passage(28.2),passage(42)])
scene['walls'][-1]['faces']={'north':M,'south':plain}
wall(60.15,59.85,79.85,T,openings=[passage(12),passage(58)])
scene['walls'][-1]['faces']={'north':M,'south':'backrooms:ochre_01'}
# Boundary segments of the upper union, merged into contiguous collinear runs.
edges=[]
for (i,j) in cells:
 for di,dj,axis,pos,a,b in [(-1,0,"z",xs[i],zs[j],zs[j+1]),(1,0,"z",xs[i+1],zs[j],zs[j+1]),(0,-1,"x",zs[j],xs[i],xs[i+1]),(0,1,"x",zs[j+1],xs[i],xs[i+1])]:
  if (i+di,j+dj) not in cells:edges.append((axis,pos,a,b,di or dj))
groups={}
for axis,pos,a,b,side in edges:groups.setdefault((axis,pos,side),[]).append((a,b))
for (axis,pos,side),runs in groups.items():
 merged=[]
 for a,b in sorted(runs):
  if merged and abs(merged[-1][1]-a)<.001:merged[-1]=(merged[-1][0],b)
  else:merged.append((a,b))
 for a,b in merged:
  ops=[]
  cx=(a+b)/2 if axis=='x' else pos;cz=pos if axis=='x' else (a+b)/2
  boundary_mat='backrooms:fine_wall_01' if 89.9<=cx<=134.1 and 21.9<=cz<=48.1 else 'backrooms:cool_cream_01' if 79.9<=cx<=134.1 and 87.9<=cz<=112.1 else plain
  if axis=="x":
   for sx,sz,sw,sd in stairs:
    if abs(pos-(sz+sd))<.001 and a<=sx and sx+sw<=b:ops.append(passage(sx-a,sw,3.4))
   wall(a,pos-.15,b-a,.3,UP,mat=boundary_mat,openings=ops)
  else:wall(pos-.15,a,.3,b-a,UP,mat=boundary_mat)
def col(x,z,w=.8,d=None,y=0,mat=plain,h=H):
 scene["columns"].append(dict(x=x,z=z,width=w,depth=d or w,y=y,height=h,material=mat))
def half(x,z,w,d,y=0,h=1.25):
 scene["half_walls"].append(dict(x=x,z=z,width=w,depth=d,y=y,height=h,material=plain,cap_material="backrooms:wood_cap_01"))
# Close each whole shaft, including faces above the surrounding lower ceiling.
for x,z,w,d in shafts:
 wall(x,z,.3,d,0,UP+H,plain);wall(x+w-.3,z,.3,d,0,UP+H,plain)
 wall(x,z,w,.3,0,UP+H,plain,[passage(.35,2.4,2.6)])
# F's two near-side exits keep its far wall simple; G's high window sees real circulation.
wall(7.85,100,.3,10,UP,mat=plain,openings=[passage(2,2.8)])
wall(17.85,102,.3,4,UP,mat=plain,openings=[passage(.6,2.8)])
wall(110,47.85,24,.3,UP,mat='backrooms:fine_wall_01',openings=[passage(.5,2.8),passage(4,2,1.,1.85),passage(20,2.8)])
# A: deep staggered opaque wall masses, narrow centre, wrap-around exploration.
for x,z in [(20,18),(32,22),(17,34),(35,38),(20,48),(32,48),(2,24),(50,36)]:wall(x,z,6 if x==2 else 8,10,mat=M)
for x,z in [(5,10),(52,10),(5,50),(52,50)]:col(x,z,.9,mat=M)
# B: plain dominant pillar, offset repeated bays and projections.
col(76,18,1.15)
scene['half_walls'].append(dict(x=75.96,z=17.96,width=1.23,depth=1.23,y=0,height=.12,material='home:wall_paint_offwhite_01',cap_material='home:wall_paint_offwhite_01'))
for x in range(70,136,12):
 for z in (8,34,48):col(x,z,.85)
# Alternating substantial masses frame the receding right passage and open cross-bays.
for x,z,w,d in [(82,10,10,5),(106,24,17,5),(83,40,12,6),(114,45,15,5)]:wall(x,z,w,d,mat=M)
# C: largest continuous chamber, overlapping orderly rows, four distributed doors.
for x in range(66,138,6):
 for z in range(66,138,6):
  if any(x+1>sx-1 and x<sx+sw+1 and z+1>sz-1 and z<sz+sd+1 for sx,sz,sw,sd in shafts):continue
  col(x,z,.85,mat="backrooms:ochre_01")
  # Real small cap/base bands avoid coplanar skins.
  half(x-.035,z-.035,.92,.92,h=.12)
  scene['half_walls'].append(dict(x=x-.035,z=z-.035,width=.92,depth=.92,y=H-.125,height=.12,material=plain,cap_material=plain))
# D: figure-eight around two cores, capped reception bays, ceiling headers.
for x,z,w,d in [(14,76,12,20),(34,106,12,20),(35,80,9,10),(15,110,9,10)]:
 wall(x,z,w,d,mat=plain)
for x,z,w,d in [(8,72,20,.35),(8,100,20,.35),(30,100,22,.35),(30,130,22,.35),(31,76,18,.35),(10,124,19,.35)]:half(x,z,w,d)
for x,z,w in [(8,72,20),(30,100,22)]:wall(x,z,w,.35,2.75,.65,plain)
for x,z in [(9,73),(49,100),(9,129),(49,129)]:col(x,z,.65,y=1.25 if (x,z)==(49,100) else 0,h=1.5 if (x,z)==(49,100) else H)
# E: capped divider, sparse separated seating and a shaded parallel return.
half(13,36,37,.3,UP)
for x in (18,30,42,51):col(x,36,.4,.5,UP+1.25 if x<50 else UP,h=H-1.25 if x<50 else H)
# F: little empty court, singular far-right stool and four paired panels.
# Low projecting trim stops at actual openings. No hidden lower-storey ceiling decal.
for x,z,w,d in [(8.15,109.78,9.7,.07),(8.15,100.15,1.7,.07),(12.55,100.15,5.3,.07),(8.15,100.22,.07,1.63),(8.15,104.95,.07,4.83),(17.78,100.22,.07,2.23),(17.78,105.55,.07,4.23)]:
 wall(x,z,w,d,UP+.28,.08,plain)
# G: shallow wall panels and piers, clear foreground, framed high aperture to return.
# Shallow backing sections between pilasters form framed inset panels along the left wall.
for x in (98,108,118,128):
 wall(x,22.16,.5,1.05,UP,mat=plain)
for x,w in [(90.3,3.5),(96.6,1.4),(98.5,9.5),(108.5,9.5),(118.5,9.5),(128.5,5.2)]:
 wall(x,22.17,w,.22,UP,mat='backrooms:fine_wall_01')
 wall(x,22.39,w,.12,UP+.18,.08,plain)
 wall(x,22.39,w,.12,UP+H-.24,.08,plain)
# The high aperture is in G's right perimeter and looks onto the reachable return weave.
# H: long partition, furniture left, clear right return with both ends open.
wall(92,94,34,.35,UP,mat="backrooms:cool_cream_01")
wall(92,96.6,34,.35,UP,mat="backrooms:cool_cream_01")
# Fixtures use explicit world heights to avoid wrong-storey ownership.
for k,(x,z,w,d,y) in zones.items():
 if k=="F": points=[(10,105),(16,105),(10,108),(16,108)]
 else:
  points=[(xx,zz) for xx in range(math.ceil(x+4),math.floor(x+w-2),8) for zz in range(math.ceil(z+4),math.floor(z+d-2),8)]
 for n,(xx,zz) in enumerate(points):
  # Preserve the saved sparse fixture gap on the B-G stair approach.
  if k=="B" and n==29:continue
  # Resolve real emitter anchors outside column bodies as well as wall masses.
  def clear_column(cx,cz):
   return not any(a['y']==y and a['x']-.8<cx<a['x']+a['width']+.8 and a['z']-.8<cz<a['z']+a['depth']+.8 for a in scene['columns'])
  if not clear_column(xx,zz):
   candidates=[(xx+dx,zz+dz) for dx,dz in [(1.8,0),(0,1.8),(-1.8,0),(0,-1.8),(1.8,1.8)]]
   xx,zz=next((v for v in candidates if clear_column(*v)),(xx,zz))
  assert clear_column(xx,zz),'emitter in a column'
  # Keep architectural solids and shafts clear; never light from inside a wall mass.
  if any(a["y"]==y and a["x"]-.8<xx<a["x"]+a["width"]+.8 and a["z"]-.8<zz<a["z"]+a["depth"]+.8 for a in scene["walls"] if a["width"]>.4 and a["depth"]>.4):continue
  scene["ceiling_lights"].append(dict(id=f"{k}_panel_{n:03}",fixture="core:fluorescent_panel_01",x=xx,z=zz,y=y+H-.002,align="none",brightness=.4 if k=="F" else .7 if k!="C" else .65,emission=.8,range=8.5 if k=="F" else 11.,color=[1,.99,.96] if k=="F" else [1,.97,.88] if k!="H" else [.91,1,.96]))
# Corridor illumination is sparse and regular; only actual corridor centres are used.
for n,q in enumerate(upper):
 x,z,w,d,k=q
 if len(k)!=2:continue
 axis=w>d; length=w if axis else d
 for s in range(2,max(3,int(length)),8):
  xx=x+s if axis else x+w/2; zz=z+d/2 if axis else z+s
  if any((l["x"]-xx)**2+(l["z"]-zz)**2<16 and l["y"]>6 for l in scene["ceiling_lights"]):continue
  scene["ceiling_lights"].append(dict(id=f"link_panel_{n}_{s}",fixture="core:fluorescent_panel_01",x=xx,z=zz,y=UP+H-.002,align="none",brightness=.6,emission=.8,range=11.,color=[1,.97,.9]))
audit=[]
def prop(id,model,x,z,y=0,yaw=0,wrong=False,kind="sunken"):
 e=next(a for a in cat["assets"] if a["id"]==model)
 q=dict(id=id,model=model,x=x,z=z,y=y,rotation_degrees=yaw,scale=1.,size=e["size"],solid=True)
 normal=dict(x=x,y=y,z=z,rotation_degrees=yaw,pitch_degrees=0,roll_degrees=0,scale=1.)
 if wrong:
  if kind=="sunken":q["y"]-=.22
  elif kind=="tilted":q["roll_degrees"]=23;q["y"]+=.08
  elif kind=="scale":q["scale"]=.55 if len(audit)%2 else 1.6
 scene["props"].append(q)
 audit.append(dict(id=id,model=model,catalog_type=e["asset_type"],normal=normal,actual={k:q.get(k,0) for k in normal},anomaly=kind if wrong else None,collision="requires native review"))
# All placements remain sparse and outside principal thoroughfares.
for i,(x,z) in enumerate([(17,30.5),(29,34.8),(42,30.6),(51,44.8),(17,45.8),(39,38)]):
 prop(f"E_chair_{i}","backrooms:ladder_chair",x,z,UP,90,wrong=i%3!=2,kind=["sunken","tilted","scale"][i%3])
prop("F_solitary_stool","backrooms:round_stool",9.5,108.5,UP,wrong=True,kind="scale")
for i,(x,z) in enumerate([(9,12),(50,12),(7,53),(51,54),(66,5),(133,53),(65,135),(135,135),(5,75),(52,135)]):
 prop(f"peripheral_chair_{i}","backrooms:ladder_chair",x,z,wrong=i%3!=2,kind=["sunken","tilted","scale"][i%3])
for i,(x,z) in enumerate([(84,91),(88,91),(91,91),(85,108)]):
 prop(f"H_cabinet_{i}","core:office_filing_cabinet_tall",x,z,UP,180,wrong=i!=3,kind="sunken" if i%2 else "scale")
for i,(x,z) in enumerate([(86,91),(90,91)]):
 prop(f"H_desk_{i}","core:desk",x,z,UP,180,wrong=i==1,kind="scale")
 prop(f"H_crt_{i}","home:crt_tv",x,z,UP+.75,180,wrong=i==1,kind="tilted")
 if i==1:
  scene["props"][-1]["y"]=UP+.75*.55+.08
  audit[-1]["actual"]["y"]=scene["props"][-1]["y"]
for i,x in enumerate((18,27,38,49)):
 prop(f"E_cabinet_{i}","core:cabinet",x,46,UP,0,wrong=i!=3,kind="scale" if i%2 else "sunken")
# Eight true model fixtures: audit denominator includes every one.
for i,x in enumerate(range(84,132,6)):
 prop(f"H_troffer_{i}","core:office_fluorescent_panel_housing",x,102,UP+H-.003,wrong=i<5,kind="scale")
 q=scene["props"][-1];q["solid"]=False
 # Correct actual mount after scaling; wrong scale remains intentional.
 q["y"]=UP+H-.003
 q["lights"]=[dict(shape="rect",half_width=.5,half_depth=.2,offset=[0,-.05,0],intensity=.3,range=6.,color=[.93,1,.96])]
 audit[-1]["actual"]["y"]=q["y"]
prop("F_ceiling_vent","backrooms:ceiling_vent",13,106.5,UP+H-.003)
scene['props'][-1]['solid']=False
# World coordinates and ordinary prop dimensions are metres throughout.
S=1.
# Shared physical tile boundaries reduce source-query cost and atlas fragmentation.
# Zones remain authored by architecture, instance IDs and the camera/layout audit.
upper_rooms=[q for q in scene['rooms'] if q['floor_y']==UP]
scene["rooms"]=[]
cuts=[0,31.5,63,94.5,126,140]
for plane in (0,):
 for x0,x1 in zip(cuts,cuts[1:]):
  for z0,z1 in zip(cuts,cuts[1:]):
   patches=[(x0,z0,x1-x0,z1-z0)]
   for shaft in shafts:patches=[v for q in patches for v in subtract(q,shaft)]
   for x,z,w,d in patches:
     scene["rooms"].append(dict(x=x,z=z,width=w,depth=d,floor_y=plane,height=H,comment="Lower circulation"))
scene['rooms'].extend(upper_rooms)
for i,s in enumerate(shafts):scene["rooms"].append(dict(x=s[0],z=s[1],width=s[2],depth=s[3],floor_y=0,height=UP+H,comment="Shaft "+["A-E","B-G","C-H","D-F"][i]))
for key in ("rooms","walls","columns","half_walls","stairs","props","ceiling_lights","decals"):
 for q in scene[key]:
  for field in ("x","z"):q[field]=round(q[field]*S,6)
  if key not in ("props","ceiling_lights","decals"):
   for field in ("width","depth"):q[field]=round(q[field]*S,6)
  for op in q.get("openings",[]):
   for field in ("offset","width"):op[field]=round(op[field]*S,6)
# Material ownership follows actual floor rectangles, including stacked galleries.
split=[]
for r in scene['rooms']:
 pieces=[r]
 if r['floor_y']==0 and r['height']==H:
  for axis,span,cut in [('x','width',60),('z','depth',60)]:
   more=[]
   for q in pieces:
    if q[axis]+.001<cut<q[axis]+q[span]-.001:
     a=dict(q);b=dict(q);a[span]=cut-q[axis];b[axis]=cut;b[span]=q[axis]+q[span]-cut;more.extend([a,b])
    else:more.append(q)
   pieces=more
 for q in pieces:
  cx=q['x']+q['width']/2;cz=q['z']+q['depth']/2
  if q['floor_y']==0:
   zone='A' if cx<60 and cz<60 else 'B' if cz<60 else 'D' if cx<60 else 'C'
  else:zone=q['comment'].split()[0]
  q['material']='backrooms:carpet_olive_01' if zone=='A' else 'backrooms:carpet_tan_01' if zone in ('B','C','F') else 'backrooms:carpet_cool_01' if zone=='H' else 'backrooms:carpet_cream_01'
  split.append(q)
scene['rooms']=split

def repair_upper_wall_joins(scene, floor=3.55, height=3.4, thickness=.3):
 """Explicit Backrooms construction: continuous T runs and one L run own the joint."""
 walls=scene['walls'];eps=.0001;half=thickness/2
 horizontal=[(i,q) for i,q in enumerate(walls) if abs(q.get('y',0)-floor)<eps and abs(q.get('height',0)-height)<eps and abs(q['depth']-thickness)<eps and q['width']>thickness+eps]
 vertical=[(i,q) for i,q in enumerate(walls) if abs(q.get('y',0)-floor)<eps and abs(q.get('height',0)-height)<eps and abs(q['width']-thickness)<eps and q['depth']>thickness+eps]
 original=json.loads(json.dumps(walls));changes=[]
 def vertical_continues(x,z):
  spans=[]
  for _,q in vertical:
   if abs(q['x']+half-x)>eps:continue
   intervals=[(q['z'],q['z']+q['depth'])]
   for op in q.get('openings',[]):
    c=q['z']+op['offset'];d=c+op['width'];new=[]
    for a,b in intervals:
     if d<=a or c>=b:new.append((a,b));continue
     if c>a:new.append((a,c))
     if d<b:new.append((d,b))
    intervals=new
   spans.extend(intervals)
  cursor=z-half
  for a,b in sorted(spans):
   if b<cursor-eps:continue
   if a>cursor+eps:return False
   cursor=max(cursor,b)
   if cursor>=z+half-eps:return True
  return False
 # Extend horizontal ends to the far face of a meeting perpendicular wall.
 # A collinear continuation already owns that corner; never overlap its body.
 for i,q in horizontal:
  z=q['z']+half;a=q['x'];b=a+q['width'];aa=a;bb=b
  for end,side in [(a,-1),(b,1)]:
   if not any(abs(v['x']+half-end)<eps and v['z']-eps<=z<=v['z']+v['depth']+eps for _,v in vertical):continue
   if vertical_continues(end,z):
    if side<0:aa=end+half
    else:bb=end-half
    continue
   proposed=end+side*half
   for j,w in horizontal:
    if i==j or abs(w['z']-q['z'])>eps:continue
    c=original[j]['x'];d=c+original[j]['width']
    if side<0 and c<end-eps and d>=proposed-eps:proposed=min(end,max(proposed,d))
    if side>0 and d>end+eps and c<=proposed+eps:proposed=max(end,min(proposed,c))
   if side<0:aa=proposed
   else:bb=proposed
  q['x']=round(aa,6);q['width']=round(bb-aa,6)
  for opening in q.get('openings',[]):opening['offset']=round(opening['offset']+a-aa,6)
 # Trim the perpendicular runs to the actual horizontal slab faces. Determine
 # full solid coverage independently of run splits; openings are never filled.
 def covered(x0,x1,z):
  spans=[]
  for _,q in horizontal:
   if abs(q['z']+half-z)>eps:continue
   intervals=[(q['x'],q['x']+q['width'])]
   for op in q.get('openings',[]):
    c=q['x']+op['offset'];d=c+op['width'];new=[]
    for a,b in intervals:
     if d<=a or c>=b:new.append((a,b));continue
     if c>a:new.append((a,c))
     if d<b:new.append((d,b))
    intervals=new
   spans.extend(intervals)
  cursor=x0
  for a,b in sorted(spans):
   if b<cursor-eps:continue
   if a>cursor+eps:return False
   cursor=max(cursor,b)
   if cursor>=x1-eps:return True
  return False
 for i,q in vertical:
  a=q['z'];b=a+q['depth'];aa=a+half if not vertical_continues(q['x']+half,a) and covered(q['x'],q['x']+thickness,a) else a;bb=b-half if not vertical_continues(q['x']+half,b) and covered(q['x'],q['x']+thickness,b) else b
  assert bb-aa>.001,'A short wall needs individual corner ownership'
  q['z']=round(aa,6);q['depth']=round(bb-aa,6)
  for opening in q.get('openings',[]):opening['offset']=round(opening['offset']+a-aa,6)
 for i,(a,b) in enumerate(zip(original,walls)):
  if a==b:continue
  for op in b.get('openings',[]):assert op['offset']>=-eps and op['offset']+op['width']<=max(b['width'],b['depth'])+eps
  changes.append(dict(index=i,before=a,after=b))
 return changes

repair_upper_wall_joins(scene,UP,H,T)
scene["spawn"]["x"]*=S;scene["spawn"]["z"]*=S
for a in audit:
 for side in ("normal","actual"):
  for field in ("x","z"):a[side][field]=round(a[side][field]*S,6)
project=R/"projects/backrooms";(project/"levels").mkdir(parents=True,exist_ok=True)
(project/"levels/the_backrooms.json").write_text(json.dumps(scene,indent=2)+"\n",encoding="utf-8")
manifest=dict(schema="places-project",project_format=1,id="backrooms",name="The Backrooms",author="Places",description="An immense connected two-storey fluorescent complex. Every route returns by another way.",entry_level="the_backrooms",menu_entry_only=True,levels=[dict(id="the_backrooms",source="levels/the_backrooms.json")])
(project/"places.project.json").write_text(json.dumps(manifest,indent=2)+"\n",encoding="utf-8")
cams={k:dict(position=[x+min(w*.4,16),y+1.6,z+min(d*.15,6)],yaw=180,pitch=0) for k,(x,z,w,d,y) in zones.items()}
cams["D"]=dict(position=[30,1.6,67],yaw=180,pitch=0)
cams["A"]=dict(position=[30,1.6,8],yaw=180,pitch=0)
cams["E"]=dict(position=[11,5.2,32],yaw=90,pitch=0)
cams["F"]=dict(position=[13,UP+1.6,100.8],yaw=180,pitch=3)
cams["G"]=dict(position=[94,UP+1.6,40],yaw=90,pitch=0)
cams["H"]=dict(position=[82,UP+1.6,93.5],yaw=90,pitch=0)
for camera in cams.values():
 camera["position"][0]*=S;camera["position"][2]*=S
 if camera["position"][1]>4:camera["position"][1]=UP+1.6
for filename,data in [("prop-anomaly-audit.json",audit),("acceptance-cameras.json",cams),("layout.json",dict(design_grid_scale=S,zones=zones,stairs=stairs,upper=upper,actual_stairs=scene['stairs'],actual_zones={k:[x*S,z*S,w*S,d*S,y] for k,(x,z,w,d,y) in zones.items()}))]:
 (O/filename).write_text(json.dumps(data,indent=2)+"\n",encoding="utf-8")
print(json.dumps(dict(rooms=len(scene["rooms"]),walls=len(scene["walls"]),columns=len(scene["columns"]),lights=len(scene["ceiling_lights"]),props=len(audit),anomalous=sum(bool(a["anomaly"]) for a in audit),gross_room_area=sum(r["width"]*r["depth"] for r in scene["rooms"]))))
