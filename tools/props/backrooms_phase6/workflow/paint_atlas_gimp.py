from gi.repository import Gimp,Gegl,Gio
import math,random,os,json
BASE=os.environ.get('PLACES_PHASE6_ROOT','/workspace/shared/places-phase6');S=1024;C=256
roles=['galvanized','charcoal','paint_cream','paint_sage','acoustic','waterstain','wireglass','diffuser','clockface','rubber','laminate','label','brushed','rust','tube','paper']
colors=[(145,152,148),(44,48,46),(188,190,169),(106,123,113),(193,191,164),(165,153,116),(136,158,148),(211,216,184),(215,213,186),(37,42,39),(174,168,137),(186,174,99),(127,136,131),(118,82,53),(225,228,200),(205,199,167)]
im=Gimp.Image.new(S,S,Gimp.ImageBaseType.RGB);im.undo_disable();Gimp.context_set_antialias(False);Gimp.context_set_feather(False)
def col(c):return Gegl.Color.new('#%02x%02x%02x'%tuple(max(0,min(255,round(v)))for v in c))
def layer(name):
 global lay
 lay=Gimp.Layer.new(im,name,S,S,Gimp.ImageType.RGBA_IMAGE,100,Gimp.LayerMode.NORMAL);im.insert_layer(lay,None,0)
def rect(x,y,w,h,c):
 im.select_rectangle(Gimp.ChannelOps.REPLACE,float(x),float(y),float(w),float(h));Gimp.context_set_foreground(col(c));lay.edit_fill(Gimp.FillType.FOREGROUND)
def poly(pts,c):
 im.select_polygon(Gimp.ChannelOps.REPLACE,[float(v)for p in pts for v in p]);Gimp.context_set_foreground(col(c));lay.edit_fill(Gimp.FillType.FOREGROUND)
def line(a,b,c,w=2):
 dx=b[0]-a[0];dy=b[1]-a[1];d=math.hypot(dx,dy)
 if not d:return
 xx=-dy/d*w/2;yy=dx/d*w/2;poly([(a[0]+xx,a[1]+yy),(b[0]+xx,b[1]+yy),(b[0]-xx,b[1]-yy),(a[0]-xx,a[1]-yy)],c)
def ell(x,y,w,h,c):
 im.select_ellipse(Gimp.ChannelOps.REPLACE,float(x),float(y),float(w),float(h));Gimp.context_set_foreground(col(c));lay.edit_fill(Gimp.FillType.FOREGROUND)
layer('01 Opaque fitted material regions')
for i,c in enumerate(colors):rect((i%4)*C,(i//4)*C,C,C,c)
layer('02 Neutral material grain - no directional baked light')
rng=random.Random(131)
for i,c in enumerate(colors):
 x=i%4*C;y=i//4*C
 for k in range(250):
  xx=x+rng.randrange(C);yy=y+rng.randrange(C);d=rng.choice([-11,-7,5,8]);rect(xx,yy,rng.randrange(2,8),rng.randrange(1,4),[v+d for v in c])
layer('03 Manufacturing details, pores and restrained age')
for i,role in enumerate(roles):
 x=i%4*C;y=i//4*C;c=colors[i]
 if role in ['galvanized','brushed']:
  for yy in range(16,248,8):line((x+10,y+yy),(x+246,y+yy),(c[0]-6,c[1]-5,c[2]-5),1)
  for k in range(90):
   xx=x+rng.randrange(14,240);yy=y+rng.randrange(14,240);sz=rng.randrange(3,12);delta=rng.choice([-9,7]);poly([(xx,yy-sz),(xx+sz,yy),(xx,yy+sz),(xx-sz,yy)],tuple(v+delta for v in c))
 if role in ['paint_cream','paint_sage','laminate']:
  for k in range(15):
   xx=x+rng.randrange(20,236);yy=y+rng.randrange(20,236);line((xx,yy),(xx+7,yy+2),tuple(v-14 for v in c),1.4)
 if role in ['acoustic','waterstain']:
  for k in range(340):
   xx=x+rng.randrange(9,247);yy=y+rng.randrange(9,247);line((xx,yy),(xx+2,yy+rng.randrange(1,5)),tuple(v-28 for v in c),1.8)
 if role=='waterstain':
  for rr in range(92,26,-9):
   # Narrow irregular water rings, not baked shading.
   for a in range(0,360,10):
    a1=math.radians(a);a2=math.radians(a+10);r1=rr*(1+.10*math.sin(a1*5));r2=rr*(1+.10*math.sin(a2*5));line((x+128+math.cos(a1)*r1,y+132+math.sin(a1)*r1*.72),(x+128+math.cos(a2)*r2,y+132+math.sin(a2)*r2*.72),(148,131,88),2)
 if role=='wireglass':
  for q in range(-240,500,25):
   # Diamond wire pattern. Clip per cell by limiting each segment.
   for yy in range(14,242):
    xx=q+yy
    if 14<xx<242:rect(x+xx,y+yy,1.5,1.5,(85,102,93))
    xx=q-yy
    if 14<xx<242:rect(x+xx,y+yy,1.5,1.5,(85,102,93))
 if role=='diffuser':
  for q in range(12,245,8):line((x+q,y+10),(x+q,y+246),(188,197,171),1.6)
  for q in range(12,245,12):line((x+10,y+q),(x+246,y+q),(198,205,176),1.3)
 if role=='clockface':
  ell(x+17,y+17,222,222,(46,51,45));ell(x+24,y+24,208,208,(215,213,186))
  for n in range(60):
   a=math.radians(n*6-90);r=92;rr=80 if n%5==0 else 87;line((x+128+math.cos(a)*r,y+128+math.sin(a)*r),(x+128+math.cos(a)*rr,y+128+math.sin(a)*rr),(49,55,48),4 if n%5==0 else 1.5)
  # Distinct 10:08 hands, centred pivot with no text font dependency.
  for n,r,w in [(50,54,5),(8,79,3)]:
   a=math.radians(n*6-90);line((x+128,y+128),(x+128+math.cos(a)*r,y+128+math.sin(a)*r),(40,47,42),w)
  ell(x+122,y+122,12,12,(50,55,46))
 if role=='label':
  rect(x+31,y+40,194,176,(197,184,108));poly([(x+128,y+60),(x+199,y+177),(x+57,y+177)],(53,54,43));poly([(x+129,y+83),(x+116,y+125),(x+136,y+122),(x+123,y+153),(x+153,y+111),(x+133,y+114)],(208,191,104))
 if role=='paper':
  for yy in range(50,220,18):line((x+42,y+yy),(x+rng.randrange(170,217),y+yy),(119,122,102),2)
Gimp.Selection.none(im)
assert Gimp.file_save(Gimp.RunMode.NONINTERACTIVE,im,Gio.File.new_for_path(BASE+'/sources/liminal_utility_materials.xcf'))
def png(path):
 p=Gimp.get_pdb().lookup_procedure('file-png-export');cfg=p.create_config();cfg.set_property('run-mode',Gimp.RunMode.NONINTERACTIVE);cfg.set_property('image',im);cfg.set_property('file',Gio.File.new_for_path(path));cfg.set_property('compression',9);cfg.set_property('include-color-profile',False);assert p.run(cfg).index(0)==Gimp.PDBStatusType.SUCCESS
png(BASE+'/textures/liminal_utility_master.png')
im.scale(256,256);png(BASE+'/textures/liminal_utility_256.png')
im.scale(128,128);png(BASE+'/textures/liminal_utility_128.png')
json.dump({'source_tool':'GIMP 3.0.4','master':[1024,1024],'native':[256,256],'layers':3,'roles':roles,'cell_layout':'4x4 row-major image top-left; .055 cell inset, .89 interior UV range'},open(BASE+'/metadata/texture_contract.json','w'),indent=2)
print('GIMP_ATLAS_COMPLETE',flush=True);im.delete()
