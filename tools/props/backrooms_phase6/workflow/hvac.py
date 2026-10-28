import math
import assetkit as a

def collar_x(x,y=.2,z=0):
 for s in [-1,1]:a.box('rolled_side_flange',(x,y,z+s*.303),(.05,.44,.035),'brushed');a.box('rolled_top_flange',(x,y+s*.203,z),(.05,.035,.6),'brushed')
def collar_z(z,y=.2,x=0):
 for s in [-1,1]:a.box('rolled_side_flange',(x+s*.303,y,z),(.035,.44,.05),'brushed');a.box('rolled_top_flange',(x,y+s*.203,z),(.6,.035,.05),'brushed')
def straight(pass_no=1,length=2.4,vertical=False):
 a.init(pass_no);n='hvac_duct_vertical'if vertical else('hvac_duct_short'if length==1.2 else'hvac_duct_straight')
 for s in [-1,1]:
  if vertical:
   a.box('folded_side',(s*.194,length/2,0),(.012,length,.6),'galvanized');a.box('folded_front',(0,length/2,s*.294),(.376,length,.012),'galvanized')
  else:
   a.box('folded_side',(0,.2,s*.294),(length,.4,.012),'galvanized');a.box('folded_top',(0,.2+s*.194,0),(length,.012,.576),'galvanized')
 if pass_no>=2:
  for q in [-length/2+.025,length/2-.025]:
   if vertical:
    for s in [-1,1]:a.box('slip_joint',(s*.203,q+length/2,0),(.035,.05,.64),'brushed');a.box('slip_joint',(0,q+length/2,s*.303),(.4,.05,.035),'brushed')
   else:collar_x(q)
 if pass_no>=3:
  if vertical:
   a.box('folded_seam',(.204,length/2,.267),(.012,length,.027),'brushed')
  else:
   a.box('folded_seam',(0,.408,.255),(length,.016,.025),'brushed')
   for x in [-length*.26,length*.26]:
    a.box('strap_top',(x,.413,0),(.034,.025,.635),'charcoal');
    for z in [-.311,.311]:a.box('strap_side',(x,.2,z),(.034,.4,.018),'charcoal')
 return a.finish(n,pass_no,['Hollow 12 mm sheet stock; no caps or grille blocking passage.','Use connector anchors from manifest, not bounding box dimensions.'])

def swept(name,path,frame_a,frame_b,w=.6,h=.4,th=.012):
 # Closed wall solids made from outer/inner rings. End apertures are fully open.
 verts=[]
 for p,u,v in zip(path,frame_a,frame_b):
  for ww,hh in [(w/2,h/2),(-w/2,h/2),(-w/2,-h/2),(w/2,-h/2),(w/2-th,h/2-th),(-w/2+th,h/2-th),(-w/2+th,-h/2+th),(w/2-th,-h/2+th)]:
   verts.append(tuple(p[k]+u[k]*ww+v[k]*hh for k in range(3)))
 faces=[]
 for j in range(len(path)-1):
  for i in range(4):
   k=(i+1)%4;s=j*8;t=(j+1)*8
   faces += [(s+i,s+k,t+k,t+i),(s+4+k,s+4+i,t+4+i,t+4+k)]
 for j in [0,len(path)-1]:
  s=j*8
  for i in range(4):k=(i+1)%4;faces.append((s+i,s+4+i,s+4+k,s+k)if j==0 else(s+k,s+4+k,s+4+i,s+i))
 return a.mesh(name,verts,faces,'galvanized')

def bend(pass_no=1,mode='horizontal'):
 a.init(pass_no);n='hvac_elbow_'+mode;path=[];us=[];vs=[]
 segs=3 if pass_no<3 else 5
 # .6 m centre-line radius, .3 m tangent stubs; square orthogonal connection planes.
 angles=[-math.pi/2+i*(math.pi/2)/segs for i in range(segs+1)]
 sign=-1 if mode=='down' else 1
 if mode=='horizontal':
  path=[(-.9,.2,0)]+[(-.6+.6*math.cos(t),.2,.6+.6*math.sin(t))for t in angles]+[(0,.2,.9)]
  us=[(0,0,1)]+[(-math.cos(t),0,-math.sin(t))for t in angles]+[(-1,0,0)];vs=[(0,1,0)]*len(path)
 else:
  # normal rotates through XY; side width stays .6 across local Z.
  y0=.2 if sign>0 else 1.1
  path=[(-.9,y0,0)]+[(-.6+.6*math.cos(t),y0+sign*(.6+.6*math.sin(t)),0)for t in angles]+[(0,y0+sign*.9,0)]
  us=[(0,0,1)]*len(path);vs=[(0,1,0)]+[(-sign*math.cos(t),-math.sin(t),0)for t in angles]+[(-sign,0,0)]
 swept('segmented_sheet_elbow',path,us,vs)
 if pass_no>=2:
  # Narrow flanges follow four edges of each actual cross-section, not closed caps.
  for idx in [0,len(path)-1]:
   p=path[idx];u=us[idx];v=vs[idx]
   for s in [-1,1]:
    aa=tuple(p[k]+u[k]*(s*.309)-v[k]*.218 for k in range(3));bb=tuple(p[k]+u[k]*(s*.309)+v[k]*.218 for k in range(3));a.rod('end_flange',aa,bb,.016,'brushed',4)
    aa=tuple(p[k]+v[k]*(s*.209)-u[k]*.3 for k in range(3));bb=tuple(p[k]+v[k]*(s*.209)+u[k]*.3 for k in range(3));a.rod('end_flange',aa,bb,.016,'brushed',4)
 if pass_no>=3:
  for idx in range(1,len(path)-1):
   p=path[idx];u=us[idx];v=vs[idx]
   # Thin outside seam across top cap, gives segmented metal construction at no UV trick.
   aa=tuple(p[k]+v[k]*.207-u[k]*.3 for k in range(3));bb=tuple(p[k]+v[k]*.207+u[k]*.3 for k in range(3));a.rod('folded_segment_seam',aa,bb,.007,'brushed',4)
 return a.finish(n,pass_no,['Hollow swept rectangular elbow with sealed sheet-stock walls and open ends.','Horizontal, up and down variants are separate editor-ready meshes; ordinary yaw provides either direction.'])

BUILDERS={'hvac_duct_straight':lambda p:straight(p),'hvac_duct_short':lambda p:straight(p,1.2),'hvac_duct_vertical':lambda p:straight(p,2.4,True),'hvac_elbow_horizontal':lambda p:bend(p,'horizontal'),'hvac_elbow_up':lambda p:bend(p,'up'),'hvac_elbow_down':lambda p:bend(p,'down')}
