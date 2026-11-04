"""Author the four elevator mechanism models in Blender; explicit offline export."""
import bpy, bmesh, math, json, os
from pathlib import Path
ROOT=Path(os.environ.get('PLACES_ELEVATOR_ART', Path(__file__).resolve().parents[3]/'assets/shared/props/elevator_70s')); ROLES=['walnut','wood_rail','linoleum','brushed_steel','charcoal','ceiling_cream','door_satin','aged_brass','warm_diffuser','indicator','button','rubber','walnut_light','linoleum_inset','vent','service_plate']
bpy.ops.object.select_all(action='SELECT');bpy.ops.object.delete(use_global=False)
image=bpy.data.images.load(str(ROOT/'elevator_70s.png'));image.pack()
def material(name,emission=False):
 m=bpy.data.materials.new(name);m.use_nodes=True;p=m.node_tree.nodes.get('Principled BSDF');t=m.node_tree.nodes.new('ShaderNodeTexImage');t.image=image;t.interpolation='Linear';m.node_tree.links.new(t.outputs['Color'],p.inputs['Base Color']);p.inputs['Roughness'].default_value=.72
 if emission:m.node_tree.links.new(t.outputs['Color'],p.inputs['Emission Color']);p.inputs['Emission Strength'].default_value=.8
 return m
surface=material('Satin wood, linoleum and hardware');luminous=material('Warm prismatic diffuser',True)
parts={};current=[]
def xyz(p):return(p[0],-p[2],p[1])
def uv(o,role,emission=False,front_role=None):
 o.data.materials.append(luminous if emission else surface)
 if not o.data.uv_layers:o.data.uv_layers.new()
 for f in o.data.polygons:
  cell=ROLES.index(front_role if front_role and f.normal.y<-.8 else role);cx=cell%4;cy=cell//4
  n=f.normal;axes=(0,1) if abs(n.z)>.8 else ((0,2) if abs(n.y)>.7 else (1,2))
  ps=[o.data.vertices[o.data.loops[k].vertex_index].co for k in f.loop_indices]
  lo=[min(p[a] for p in ps)for a in axes];hi=[max(p[a]for p in ps)for a in axes]
  for k,p in zip(f.loop_indices,ps):
   q=[(p[a]-lo[j])/max(1e-7,hi[j]-lo[j])for j,a in enumerate(axes)]
   spans=[hi[j]-lo[j] for j in range(2)]
   if role=='wood_rail' and spans[0]>spans[1]:q=q[::-1];spans=spans[::-1]
   # Flat pocket/guide stock must not acquire neighbouring atlas colours at
   # grazing angles. Thin hardware faces use an aspect-preserving centre crop
   # rather than squeezing a whole cell across a millimetre-wide bevel.
   if role in ('charcoal','rubber'):
    q=[.5,.5]
   elif min(spans)*8<max(spans):
    longest=max(spans)
    q=[.5+(value-.5)*span/max(longest,1e-7) for value,span in zip(q,spans)]
   o.data.uv_layers.active.data[k].uv=((cx+.045+q[0]*.91)/4,1-(cy+.955-q[1]*.91)/4)
 current.append(o);return o

def box(name,lo,hi,role,bevel=0,emission=False):
 p=[(a+b)/2 for a,b in zip(lo,hi)];s=[b-a for a,b in zip(lo,hi)]
 bpy.ops.mesh.primitive_cube_add(size=1,location=xyz(p));o=bpy.context.object;o.name=name;o.dimensions=(s[0],s[2],s[1]);bpy.ops.object.transform_apply(location=False,rotation=False,scale=True)
 if bevel:
  m=o.modifiers.new('Low-poly softened manufactured edges','BEVEL');m.width=bevel;m.segments=1;bpy.ops.object.modifier_apply(modifier=m.name)
 return uv(o,role,emission)
def button_cylinder(name,x,y,z,r,depth,role,front_role=None,hide_back=False):
 bpy.ops.mesh.primitive_cylinder_add(vertices=8,radius=r,depth=depth,location=xyz((x,y,z)));o=bpy.context.object;o.name=name;o.rotation_euler.x=math.pi/2;bpy.ops.object.transform_apply(location=False,rotation=True,scale=True)
 if hide_back:
  mesh=bmesh.new();mesh.from_mesh(o.data);mesh.normal_update();bmesh.ops.delete(mesh,geom=[f for f in mesh.faces if f.normal.y>.8],context='FACES');mesh.to_mesh(o.data);mesh.free()
 return uv(o,role,front_role=front_role)

def start(name):
 global current
 current=[];parts[name]=current
start('elevator_cabin_70s')
box('Deck structural base',[-1.2,-.08,0],[1.2,-.003,2.4],'rubber')
for ix in range(4):
 for iz in range(4):
  x=-1.2+ix*.6;z=iz*.6
  box('Warm speckled resilient flooring',[x+.001,-.003,z+.001],[x+.599,0,z+.599],'linoleum' if (ix+iz)%2 else 'linoleum_inset')
box('Back reveal backing',[-1.23,0,2.43],[1.23,2.55,2.48],'charcoal')
for i in range(5):
 x=-1.2+i*.48;box('Book-matched walnut rear panel',[x+.008,.13,2.4],[x+.472,2.46,2.455],'walnut' if i%2 else 'walnut_light')
for side in [-1,1]:
 x0,x1=(-1.28,-1.23) if side<0 else (1.23,1.28)
 box('Side reveal backing',[x0,0,.24],[x1,2.55,2.48],'charcoal')
 for i in range(5):
  z=.24+i*.432;x0,x1=(-1.255,-1.2)if side<0 else(1.2,1.255)
  box('Vertical walnut side panel',[x0,.13,max(z+.008,.368)],[x1,2.46,z+.424],'walnut' if i%2 else 'walnut_light')
 x0,x1=(-1.2,-1.185)if side<0 else(1.185,1.2)
 box('Side bronze skirting',[x0,.025,.368],[x1,.13,2.38],'aged_brass')
 # Two recessed channels house the landing and cabin leaves. The folds are
 # metal, while the passenger-facing wall outside the narrow frame is wood.
 x0,x1=(-1.425,-.715)if side<0 else(.715,1.425)
 for z0,z1 in [(.02,.10),(.20,.24)]:
  box('Metal-clad door-pocket skin',[x0,0,z0],[x1,2.55,z1],'brushed_steel')
 x0,x1=(-.755,-.715)if side<0 else(.715,.755)
 box('Metal pocket mouth collar',[x0,0,.345],[x1,2.24,.36],'brushed_steel')
 x0,x1=(-1.425,-.755)if side<0 else(.755,1.425)
 box('Walnut passenger front wall',[x0,0,.345],[x1,2.24,.36],'walnut')
 x0,x1=(-1.425,-.715)if side<0 else(.715,1.425)
 box('Walnut upper front wall',[x0,2.24,.345],[x1,2.55,.36],'walnut')
 x0,x1=(-1.45,-1.425)if side<0 else(1.425,1.45)
 box('Closed pocket outer end',[x0,0,.018],[x1,2.55,.362],'charcoal')
 x0,x1=(-1.45,-.679)if side<0 else(.679,1.45)
 box('Pocket lower rail',[x0,-.04,.02],[x1,-.015,.36],'charcoal')
 box('Pocket top enclosure',[x0,2.55,.02],[x1,2.57,.36],'charcoal')
 x0,x1=(-1.425,-.679)if side<0 else(.679,1.425)
 box('Cabin dark pocket return',[x0,-.012,.235],[x1,2.23,.26],'charcoal')
 # Wood rails align with their separate waist-height collision stock.
 x0,x1=(-1.14,-1.06)if side<0 else(1.06,1.14)
 box('Softened walnut side handrail',[x0,.95,.55],[x1,1.03,2.25],'wood_rail',.018)
 for z in [.72,1.98]:
  x0,x1=(-1.2,-1.10)if side<0 else(1.10,1.2)
  box('Brass handrail bracket',[x0,.963,z-.018],[x1,1.005,z+.018],'aged_brass')
box('Rear bronze skirting',[-1.2,.025,2.385],[1.2,.13,2.4],'aged_brass')
box('Softened walnut rear handrail',[-1.11,.95,2.26],[1.11,1.03,2.34],'wood_rail',.018)
for x in [-.83,.83]:box('Rear brass rail bracket',[x-.018,.963,2.30],[x+.018,1.005,2.4],'aged_brass')
box('Cream ceiling',[-1.2,2.55,.24],[1.2,2.63,2.48],'ceiling_cream')
box('Metal front overhead return',[-.715,2.205,.02],[.715,2.24,.36],'brushed_steel')
box('Walnut wall above doorway',[-.715,2.24,.02],[.715,2.55,.36],'walnut')
box('Cabin metal recessed header',[-.715,2.205,.365],[.715,2.24,.383],'brushed_steel')
# A fitted passenger-side stop overlaps the closed stock and metal returns.
# The deliberately wider pocket mouth retains a dark metal track recess.
for x0,x1 in [(-.735,-.688),(.688,.735)]:
 box('Cabin steel door stop',[x0,-.012,.385],[x1,2.2,.403],'brushed_steel')
box('Cabin steel header stop',[-.735,2.2,.385],[.735,2.24,.403],'brushed_steel')
box('Brass diffuser surround',[-.76,2.515,.62],[.76,2.55,1.82],'aged_brass',.006)
box('Warm prismatic lens',[-.70,2.506,.68],[.70,2.515,1.76],'warm_diffuser',emission=True)
for x in [-1.18,1.13]:box('Ceiling edge batten',[x,2.475,.368],[x+.05,2.55,2.345],'wood_rail')
box('Rear ceiling batten',[-1.18,2.475,2.35],[1.18,2.55,2.4],'wood_rail')
# One opaque sill projects beyond both jambs and across both door tracks.
# Recesses have a real backing; carpet/linoleum cannot show through them.
box('Full doorway threshold backing',[-.86,-.012,-.32],[.86,.006,.42],'brushed_steel')
for x0,x1 in [(-.86,-.82),(.82,.86)]:
 box('Threshold end cap',[x0,.006,-.32],[x1,.012,.42],'brushed_steel')
cursor=-.32
for z0,z1 in [(-.27,-.26),(-.21,-.20),(-.15,-.14),(-.09,-.08),(-.03,-.02),(.119,.181),(.269,.331)]:
 box('Threshold steel tread',[-.82,.006,cursor],[.82,.012,z0],'brushed_steel')
 box('Recessed opaque door track',[-.82,.006,z0],[.82,.008,z1],'charcoal')
 cursor=z1
box('Threshold steel tread',[-.82,.006,cursor],[.82,.012,.42],'brushed_steel')
box('Recessed control surround',[.79,.62,.365],[1.09,1.74,.386],'aged_brass',.006)
box('Control panel inset',[.81,.64,.387],[1.07,1.72,.393],'charcoal')
box('Etched service plate',[.83,.655,.394],[1.05,.73,.398],'service_plate')
start('elevator_frame_70s')
jambs=[]
for x0,x1 in [(-.795,-.715),(.715,.795)]:jambs.append(box('Brushed metal jamb',[x0,-.004,-.17],[x1,2.28,-.105],'brushed_steel',.006))
lintel=box('Brushed metal lintel',[-.795,2.225,-.17],[.795,2.29,-.105],'brushed_steel',.006)
# Manufacture one continuous frame: remove the coplanar stock intersections,
# preserving the fitted steel UVs on the exterior faces.
for jamb in jambs:
 modifier=lintel.modifiers.new('Welded frame stock','BOOLEAN');modifier.operation='UNION';modifier.solver='EXACT';modifier.object=jamb
 bpy.context.view_layer.objects.active=lintel;bpy.ops.object.modifier_apply(modifier=modifier.name)
 current.remove(jamb);bpy.data.objects.remove(jamb,do_unlink=True)
box('Recessed dark transom',[-.715,2.2,-.15],[.715,2.24,.24],'charcoal')
box('Landing sill',[-.795,-.014,-.17],[.795,-.002,.24],'brushed_steel')
for x0,x1 in [(-1.425,-.675),(.675,1.425)]:
 box('Landing dark pocket liner',[x0,-.012,.195],[x1,2.23,.225],'charcoal')
for x0,x1 in [(-.79,-.710),(.710,.79)]:
 box('Dark pocket mouth return',[x0,-.012,-.105],[x1,2.23,.025],'charcoal')
box('Indicator brass rim',[-.105,2.41,-.16],[.105,2.57,-.13],'aged_brass',.004)
box('Amber arrival indicator',[-.083,2.43,-.165],[.083,2.55,-.161],'indicator')
box('Hall call plate',[-1.06,.98,-.15],[-.86,1.18,-.11],'brushed_steel',.007)
button_cylinder('Hall call brass bezel',-.96,1.08,-.151,.036,.016,'aged_brass')
button_cylinder('Hall call button',-.96,1.08,-.162,.026,.012,'button')
start('elevator_leaf_70s')
box('Satin gray sliding door',[0,-.01,-.0225],[.695,2.22,.0225],'door_satin',.003)
box('Concealed center-edge seal',[-.004,-.01,.0225],[.004,2.22,.03],'charcoal')
start('elevator_button_70s')
backing=box('Button backing',[.865,1.54,.394],[1.015,1.62,.408],'charcoal')
# Its rear face is concealed by the opaque panel; keep three-button cabins
# inside the ordinary assembled art budget without adding competing faces.
mesh=bmesh.new();mesh.from_mesh(backing.data);mesh.normal_update();bmesh.ops.delete(mesh,geom=[f for f in mesh.faces if f.normal.y>.8],context='FACES');mesh.to_mesh(backing.data);mesh.free()
button_cylinder('Brass octagonal bezel',.94,1.58,.410,.035,.018,'aged_brass',hide_back=True)
button_cylinder('Cream passenger button',.94,1.58,.423,.027,.010,'button','vent',True)
# Join each rigid part so the runtime receives two primitives, not dozens of nodes.
for name,objects in list(parts.items()):
 bpy.ops.object.select_all(action='DESELECT')
 for o in objects:o.select_set(True)
 bpy.context.view_layer.objects.active=objects[0];bpy.ops.object.join();o=bpy.context.object;o.name=name;bpy.ops.object.transform_apply(location=True,rotation=True,scale=True);parts[name]=[o]
# One retained Blender document contains all four independently exported rigid parts.
for name,objects in parts.items():
 collection=bpy.data.collections.new(name);bpy.context.scene.collection.children.link(collection)
 for o in objects:
  for old in list(o.users_collection):old.objects.unlink(o)
  collection.objects.link(o)
bpy.ops.wm.save_as_mainfile(filepath=str(ROOT/'elevator_70s.blend'))
receipt={}
export_parts=set(filter(None,os.environ.get('PLACES_ELEVATOR_EXPORT_PARTS','').split(',')))
if export_parts-set(parts):raise ValueError('Unknown elevator export part')
for name,objects in parts.items():
 bpy.ops.object.select_all(action='DESELECT')
 for o in objects:o.select_set(True)
 bpy.context.view_layer.objects.active=objects[0]
 if not export_parts or name in export_parts:
  bpy.ops.export_scene.gltf(filepath=str(ROOT/(name+'.glb')),export_format='GLB',use_selection=True,export_yup=True,export_texcoords=True,export_normals=True,export_materials='EXPORT',export_image_format='AUTO',export_animations=False,export_cameras=False,export_lights=False,export_extras=False)
 for o in objects:o.data.calc_loop_triangles()
 receipt[name]={'objects':len(objects),'triangles':sum(len(o.data.loop_triangles) for o in objects),'source':'elevator_70s.blend'}
(ROOT/'authoring.json').write_text(json.dumps({'tools':{'Blender':bpy.app.version_string,'GIMP':'3.2.6'},'license':'MIT - original offline artwork','atlas':'4x4 fitted cells, 1024 master / 256 native; opaque clamped UVs','parts':receipt},indent=2)+'\n')
print('ELEVATOR_BLENDER_COMPLETE',json.dumps(receipt),flush=True)
