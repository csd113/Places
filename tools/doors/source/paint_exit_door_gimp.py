"""Original fitted steel-door surface sheet, painted and exported by GIMP 3.
Offline authoring only. No photographs, third-party models or image generation.
"""
from gi.repository import Gimp,Gegl,Gio
import os, random, json
from pathlib import Path
R=Path(os.environ['EXIT_DOOR_ROOT']); N=1024; S=256
im=Gimp.Image.new(N,N,Gimp.ImageBaseType.RGB); im.undo_disable()
roles=[
 ('01 Painted steel - cool grey',(144,148,149)),
 ('02 Frame powdercoat',(116,121,123)),
 ('03 Satin hardware steel',(180,185,185)),
 ('04 Rubber and latch recess',(37,42,43)),
 ('05 Leaf edge steel',(126,132,134)),
 ('06 Subtle painted wear',(139,144,145)),
 ('07 Slotted fixing',(158,165,166)),
 ('08 Threshold brushed aluminium',(159,166,168)),
 ('09 Closer cover',(164,170,171)),
 ('10 Crossbar satin face',(178,184,184)),
 ('11 Strike and latch plates',(149,155,157)),
 ('12 Exit device end cap',(119,127,130)),
 ('13 Reverse painted steel',(144,148,149)),
 ('14 Hinge knuckle',(162,170,172)),
 ('15 Rod guide saddle',(158,166,168)),
 ('16 Reserved neutral',(126,132,134))]
def layer(name,opacity=100):
 l=Gimp.Layer.new(im,name,N,N,Gimp.ImageType.RGBA_IMAGE,opacity,Gimp.LayerMode.NORMAL)
 im.insert_layer(l,None,0);l.fill(Gimp.FillType.TRANSPARENT);return l
def rect(l,x,y,w,h,c):
 im.select_rectangle(Gimp.ChannelOps.REPLACE,x,y,w,h)
 Gimp.context_set_foreground(Gegl.Color.new('#%02x%02x%02x'%tuple(max(0,min(255,round(a))) for a in c)))
 l.edit_fill(Gimp.FillType.FOREGROUND)
base=layer('01 Material role base colours')
for i,(nm,c) in enumerate(roles):rect(base,(i%4)*S,(i//4)*S,S,S,c)
grain=layer('02 Restrained powdercoat grain',38);rng=random.Random(531170)
for i in (0,1,5,8,12):
 x=(i%4)*S;y=(i//4)*S;c=roles[i][1]
 for k in range(330):
  d=rng.choice([-5,-4,-3,3,4,5]);w=rng.randint(2,8);h=rng.randint(2,8)
  rect(grain,x+rng.randrange(S-w),y+rng.randrange(S-h),w,h,tuple(v+d for v in c))
brush=layer('03 Fine non-directional surface finish',30)
for i in (2,4,7,9,10,11,13,14,15):
 x=(i%4)*S;y=(i//4)*S;c=roles[i][1]
 for row in range(2,S-2,3):
  d=rng.randint(-8,8);rect(brush,x+2,y+row,S-4,1,tuple(v+d for v in c))
wear=layer('04 Sparse handling marks',27)
for i in (0,5,9,12):
 x=(i%4)*S;y=(i//4)*S;c=roles[i][1]
 for k in range(13):
  xx=x+rng.randint(12,235);yy=y+rng.randint(12,235)
  rect(wear,xx,yy,rng.randint(2,9),1,tuple(v-14 for v in c))
detail=layer('05 Fitted slotted fixing and threshold grooves')
# One fixing cell is sampled per head, rather than modelling a screw thread.
x=(6%4)*S;y=(6//4)*S
rect(detail,x+82,y+119,92,18,(72,79,81));rect(detail,x+89,y+123,78,8,(37,43,44))
# Narrow threshold strips use this role without stretching whole cells on bevels.
x=(7%4)*S;y=(7//4)*S
for yy in (55,105,155,205):rect(detail,x+8,y+yy,240,4,(112,121,124))
Gimp.Selection.none(im)
assert Gimp.file_save(Gimp.RunMode.NONINTERACTIVE,im,Gio.File.new_for_path(str(R/'source/exit_door_materials.xcf')))
p=Gimp.get_pdb().lookup_procedure('file-png-export')
def export(img,path):
 cfg=p.create_config();cfg.set_property('run-mode',Gimp.RunMode.NONINTERACTIVE);cfg.set_property('image',img)
 cfg.set_property('file',Gio.File.new_for_path(str(path)));cfg.set_property('compression',9)
 cfg.set_property('include-color-profile',False)
 assert p.run(cfg).index(0)==Gimp.PDBStatusType.SUCCESS
export(im,R/'source/exit_door_materials_master_1024.png')
im.scale(256,256);export(im,R/'textures/exit_door_materials.png')
json.dump({'application':'GIMP 3.0.4','source_size':[1024,1024],'runtime_size':[256,256],
 'roles':[{'cell':i,'name':n,'base_srgb':c}for i,(n,c)in enumerate(roles)],
 'layers':5,'seed':531170,'alpha':'opaque','uv':'4x4 cells, fitted, clamped, inset by 3 runtime pixels',
 'light_bake':'None. Marks are material variation, not fixed lighting or AO.',
 'rights':'Original asset artwork authored for Connor Dawkins / Places.'},
 open(R/'metadata/materials.json','w'),indent=2)
im.delete();print('EXIT_DOOR_GIMP_AUTHORING_OK',flush=True)

