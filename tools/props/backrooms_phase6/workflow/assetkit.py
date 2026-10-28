import bpy, math, os, json
from mathutils import Vector
BASE=os.environ.get('PLACES_PHASE6_ROOT','/workspace/shared/places-phase6')
ROLES=['galvanized','charcoal','paint_cream','paint_sage','acoustic','waterstain','wireglass','diffuser','clockface','rubber','laminate','label','brushed','rust','tube','paper']
MATS={}
CURRENT=[]
PASS=1

def xyz(p): return (p[0],-p[2],p[1])
def init(pass_no=1):
 global CURRENT,PASS,MATS
 bpy.ops.object.select_all(action='SELECT');bpy.ops.object.delete(use_global=False)
 CURRENT=[];PASS=pass_no;MATS={}
 im=bpy.data.images.load(BASE+'/textures/liminal_utility_256.png',check_existing=True)
 for i,role in enumerate(ROLES):
  m=bpy.data.materials.new(role);m.use_nodes=True;n=m.node_tree.nodes;p=n.get('Principled BSDF');t=n.new('ShaderNodeTexImage');t.image=im;t.interpolation='Closest';m.node_tree.links.new(t.outputs['Color'],p.inputs['Base Color']);p.inputs['Roughness'].default_value=.82;p.inputs['Metallic'].default_value=0
  if role in ['galvanized','brushed']:p.inputs['Metallic'].default_value=.55;p.inputs['Roughness'].default_value=.48
  if role=='wireglass':p.inputs['Roughness'].default_value=.4
  MATS[role]=m

def uv(o,role):
 if o.type!='MESH':return
 if not o.data.uv_layers:o.data.uv_layers.new()
 cell=ROLES.index(role);cx=cell%4;cy=cell//4
 # Individual faces fill a small fitted source region, never repeating outside 0..1.
 for f in o.data.polygons:
  n=f.normal;axes=(0,2) if abs(n[1])>=max(abs(n[0]),abs(n[2])) else ((0,1) if abs(n[2])>=abs(n[0]) else (1,2))
  pts=[o.data.vertices[o.data.loops[li].vertex_index].co for li in f.loop_indices]
  lo=[min(p[ax]for p in pts)for ax in axes];hi=[max(p[ax]for p in pts)for ax in axes]
  for li,p in zip(f.loop_indices,pts):
   q=[(p[ax]-lo[j])/max(1e-9,hi[j]-lo[j])for j,ax in enumerate(axes)]
   o.data.uv_layers.active.data[li].uv=((cx+.055+q[0]*.89)/4,1-(cy+.945-q[1]*.89)/4)
 o.data.materials.clear();o.data.materials.append(MATS[role]);CURRENT.append(o)
 return o

def box(name,p,s,role='galvanized',bevel=0):
 bpy.ops.mesh.primitive_cube_add(size=1,location=xyz(p));o=bpy.context.object;o.name=name;o.dimensions=(s[0],s[2],s[1]);bpy.ops.object.transform_apply(location=False,rotation=False,scale=True)
 if bevel and PASS>=3:
  mod=o.modifiers.new('one_cut_manufactured_edge','BEVEL');mod.width=bevel;mod.segments=1;bpy.context.view_layer.objects.active=o;bpy.ops.object.modifier_apply(modifier=mod.name)
 return uv(o,role)

def cyl(name,p,r,depth,role='charcoal',axis='y',verts=12):
 bpy.ops.mesh.primitive_cylinder_add(vertices=verts,radius=r,depth=depth,end_fill_type='NGON',location=xyz(p));o=bpy.context.object;o.name=name
 if axis=='z':o.rotation_euler[0]=math.pi/2
 if axis=='x':o.rotation_euler[1]=math.pi/2
 bpy.ops.object.transform_apply(location=False,rotation=True,scale=True)
 return uv(o,role)

def rod(name,a,b,r=.012,role='charcoal',verts=8):
 aa=Vector(xyz(a));bb=Vector(xyz(b));v=bb-aa
 bpy.ops.mesh.primitive_cylinder_add(vertices=verts,radius=r,depth=v.length,end_fill_type='NGON',location=(aa+bb)/2);o=bpy.context.object;o.name=name;o.rotation_euler=v.to_track_quat('Z','Y').to_euler();bpy.ops.object.transform_apply(location=False,rotation=False,scale=True);return uv(o,role)

def rotate(o,degrees,axis='y',pivot=None):
 # Game Y corresponds Blender Z. Other axes only for crooked ceiling stock.
 idx={'x':0,'y':2,'z':1}[axis];o.rotation_euler[idx]+=math.radians(degrees)*(-1 if axis=='z' else 1)
 return o

def mesh(name,verts,faces,role='galvanized'):
 d=bpy.data.meshes.new(name);d.from_pydata([xyz(p)for p in verts],[],faces);d.update();o=bpy.data.objects.new(name,d);bpy.context.collection.objects.link(o);return uv(o,role)

def gather():return list(CURRENT)

def finish(name,pass_no,notes=None):
 # Apply transforms, merge by material role to bound primitive count, triangulate.
 bpy.ops.object.select_all(action='DESELECT')
 for o in CURRENT:o.select_set(True)
 bpy.context.view_layer.objects.active=CURRENT[0];bpy.ops.object.join();o=bpy.context.object;o.name=name
 bpy.ops.object.transform_apply(location=True,rotation=True,scale=True)
 # Recenter X/Z and put occupied base at Y0, exactly matching ordinary Places origin.
 vv=[v.co for v in o.data.vertices];mins=[min(p[i]for p in vv)for i in range(3)];maxs=[max(p[i]for p in vv)for i in range(3)];shift=Vector((-(mins[0]+maxs[0])/2,-(mins[1]+maxs[1])/2,-mins[2]));
 for v in o.data.vertices:v.co+=shift
 bpy.ops.object.mode_set(mode='EDIT');bpy.ops.mesh.select_all(action='SELECT');bpy.ops.mesh.normals_make_consistent(inside=False);bpy.ops.object.mode_set(mode='OBJECT')
 tri=o.modifiers.new('triangulated_export','TRIANGULATE');bpy.ops.object.modifier_apply(modifier=tri.name)
 o.data.update();dims=[max(v.co[i]for v in o.data.vertices)-min(v.co[i]for v in o.data.vertices)for i in range(3)];size=[round(dims[0],6),round(dims[2],6),round(dims[1],6)]
 dest=BASE+('/models'if pass_no==3 else '/passes/pass'+str(pass_no));os.makedirs(dest,exist_ok=True)
 bpy.ops.file.pack_all();bpy.ops.wm.save_as_mainfile(filepath=BASE+'/sources/'+name+('_pass'+str(pass_no)if pass_no<3 else '')+'.blend')
 bpy.ops.export_scene.gltf(filepath=dest+'/'+name+'.glb',export_format='GLB',use_selection=True,export_yup=True,export_texcoords=True,export_normals=True,export_materials='EXPORT',export_image_format='AUTO',export_animations=False,export_cameras=False,export_lights=False,export_extras=False)
 out={'name':name,'id':'backrooms:'+name,'size':size,'triangles':len(o.data.polygons),'materials':list(dict.fromkeys(m.name.split('.')[0]for m in o.data.materials if m)),'origin_translation_game':[round(shift[0],6),round(shift[2],6),round(-shift[1],6)],'pass':pass_no,'notes':notes or []}
 os.makedirs(BASE+'/metadata/pass'+str(pass_no),exist_ok=True);json.dump(out,open(BASE+'/metadata/pass'+str(pass_no)+'/'+name+'.json','w'),indent=2);print('ASSET',json.dumps(out),flush=True)
 return o,out
