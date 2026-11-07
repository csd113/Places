"""Blender-owned single/pair flush hollow-metal exit-door assets.
Metres, +Y up and +Z push side in exported GLB. Exact geometry/pivots are
retained in .blend files; this script is offline authoring, never game runtime.
"""
import bpy, math, json, os, hashlib
from pathlib import Path
from mathutils import Vector
R=Path(os.environ['EXIT_DOOR_ROOT']); PI=math.pi
def xyz(v): return (v[0],-v[2],v[1])
def eng(v): return (v[0],v[2],-v[1])
def vec(v): return Vector(xyz(v))
def empty(name,p=(0,0,0),parent=None):
 o=bpy.data.objects.new(name,None);bpy.context.collection.objects.link(o)
 o.location=xyz(p)
 if parent:o.parent=parent
 return o
def shade_uv(o,cell):
 # All source primitives are already scale-applied. UVs stay inside one
 # inset material cell and preserve physical face aspect ratio.
 me=o.data; uv=me.uv_layers.new(name='UVMap') if not me.uv_layers else me.uv_layers.active
 col=cell%4;row=cell//4; lo_u=col/4+3/256;hi_v=1-row/4-3/256;span=.25-6/256
 for p in me.polygons:
  n=p.normal; axis=max(range(3),key=lambda i:abs(n[i]))
  axes=[i for i in range(3)if i!=axis]
  # Native X/Z are horizontal/height on front. Native Y is reverse depth.
  coords=[me.vertices[me.loops[i].vertex_index].co for i in p.loop_indices]
  mins=[min(v[a]for v in coords)for a in axes];maxs=[max(v[a]for v in coords)for a in axes]
  lens=[maxs[i]-mins[i]for i in range(2)];den=max(max(lens),1e-8)
  for li,v in zip(p.loop_indices,coords):
   u=(v[axes[0]]-mins[0])/den;w=(v[axes[1]]-mins[1])/den
   uv.data[li].uv=(lo_u+span*u,hi_v-span+span*w)
 o.data.materials.append(MATS['paint' if cell in (0,1,5,12) else 'dark' if cell==3 else 'steel'])
 for p in me.polygons:p.use_smooth=False
 return o
def box(name,c,size,cell,bevel=0):
 bpy.ops.mesh.primitive_cube_add(size=1,location=xyz(c));o=bpy.context.object;o.name=name;o.data.name=name+'_mesh'
 o.dimensions=(size[0],size[2],size[1]);bpy.ops.object.transform_apply(location=False,rotation=False,scale=True)
 if bevel:
  m=o.modifiers.new('Manufactured single-cut edge','BEVEL');m.width=bevel;m.segments=1
  bpy.ops.object.modifier_apply(modifier=m.name)
 return shade_uv(o,cell)
def cylinder(name,c,r,length,cell,axis='y',n=8):
 bpy.ops.mesh.primitive_cylinder_add(vertices=n,radius=r,depth=length,end_fill_type='NGON',location=xyz(c))
 o=bpy.context.object;o.name=name
 if axis=='z':o.rotation_euler[0]=PI/2
 if axis=='x':o.rotation_euler[1]=PI/2
 bpy.ops.object.transform_apply(location=False,rotation=True,scale=True)
 return shade_uv(o,cell)
def parent_keep(objects,parent):
 for o in objects:
  matrix=o.matrix_world.copy();o.parent=parent;o.matrix_world=matrix
def join(objects,name,parent=None):
 bpy.ops.object.select_all(action='DESELECT')
 for o in objects:o.select_set(True)
 bpy.context.view_layer.objects.active=objects[0];bpy.ops.object.join()
 o=bpy.context.object;o.name=name;o.data.name=name+'_mesh'
 # Material duplicates are intentionally consolidated after join.
 old=list(o.data.materials);slots=[];mapping={}
 for i,m in enumerate(old):
  if m not in slots:slots.append(m)
  mapping[i]=slots.index(m)
 idx=[mapping[p.material_index] for p in o.data.polygons]
 o.data.materials.clear()
 for m in slots:o.data.materials.append(m)
 for p,i in zip(o.data.polygons,idx):p.material_index=i
 if parent:parent_keep([o],parent)
 return o
def init():
 bpy.ops.object.select_all(action='SELECT');bpy.ops.object.delete(use_global=False)
 for d in list(bpy.data.materials):bpy.data.materials.remove(d)
 for collection in (bpy.data.meshes,bpy.data.actions,bpy.data.images):
  for d in list(collection):
   if d.users==0:collection.remove(d)
 scene=bpy.context.scene;scene.unit_settings.system='METRIC';scene.unit_settings.scale_length=1
 scene.render.fps=30;scene.frame_start=1;scene.frame_end=34
 global MATS
 MATS={}
 im=bpy.data.images.load(str(R/'textures/exit_door_materials.png'),check_existing=False);im.pack()
 for name,metal,rough in [('paint',.08,.66),('steel',.65,.38),('dark',.05,.8)]:
  m=bpy.data.materials.new('EXIT_'+name.upper());m.use_nodes=True;m.use_backface_culling=True
  bs=m.node_tree.nodes.get('Principled BSDF');bs.inputs['Metallic'].default_value=metal;bs.inputs['Roughness'].default_value=rough
  tx=m.node_tree.nodes.new('ShaderNodeTexImage');tx.image=im;tx.interpolation='Linear';tx.extension='EXTEND'
  m.node_tree.links.new(tx.outputs['Color'],bs.inputs['Base Color']);MATS[name]=m
def frame(W,H,root):
 parts=[]
 # Jamb and head stock are separated physical closed boxes. Their interfaces
 # touch edge-to-edge; there are no coplanar front cladding overlays.
 for sign in (-1,1):
  parts.append(box('Jamb_return',(sign*(W/2+.014),H/2,0),(.032,H,.16),1))
  for side in (-1,1):
   parts.append(box('Jamb_face',(sign*(W/2+.046),H/2,side*.089),(.06,H,.018),1))
  parts.append(box('Jamb_stop',(sign*(W/2-.001),H/2,.041),(.014,H,.018),1))
 parts.append(box('Header_return',(0,H+.014,0),(W+.124,.032,.16),1))
 for side in (-1,1):
  parts.append(box('Header_face',(0,H+.030,side*.089),(W+.152,.060,.018),1))
 parts.append(box('Header_stop',(0,H-.001,.041),(W,.014,.018),1))
 # Threshold is deliberately 8 mm, not a collider that fills the aperture.
 parts.append(box('Low_threshold',(0,.004,0),(W,.008,.15),7,.002))
 return join(parts,'frame_stock',root)
def make_leaf(W,H,hingex,sign,label,root):
 pivot=empty('hinge_'+label,(hingex,0,-.030),root)
 lw=W; bottom=.014;top=H-.008;center_y=(bottom+top)/2
 # Use global closed coordinates so physical hinge can be offset from the skin.
 def c(u,y,z):return (hingex+sign*u,y,z)
 panel=box('Flush_leaf',c(lw/2,center_y,0),(lw,top-bottom,.045),0,.0012)
 panel=join([panel],'leaf_'+label+'_core',pivot)
 hw=[]
 # Touchbar exit device silhouette: continuous horizontal rail, recessed case,
 # raised push face and compact vertical-rod centre case.
 y=1.015;lc=lw-.060;end=.065;barlen=lc-end-.055
 hw.append(box('Crossbar_back_rail',c((end+lc)/2,y,.042),(lc-end,.052,.039),11))
 hw.append(box('Crossbar_push_face',c(end+barlen/2,y,.073),(barlen,.047,.033),9,.003))
 hw.append(box('Crossbar_endcap',c(end-.008,y,.056),(.035,.061,.060),11))
 hw.append(box('Rod_centre_case',c(lc,y,.059),(.071,.185,.073),2,.004))
 for yy in (.045,H-.061):
  hw.append(box('Rod_latch_case',c(lc,yy,.044),(.047,.074,.041),10))
 for yy in (.59,1.56):
  hw.append(box('Rod_guide',c(lc,yy,.045),(.044,.031,.036),14))
 for a,b in ((.082,.922),(1.108,H-.098)):
  hw.append(cylinder('Surface_vertical_rod',c(lc,(a+b)/2,.055),.0075,b-a,2,n=8))
 # Two visible slotted fixing heads, not hidden screw threads.
 for u in (end+.045,lc-.11):
  hw.append(cylinder('Slotted_fixing',c(u,y,.092),.005,.003,6,axis='z',n=6))
 join(hw,'leaf_'+label+'_panic_hardware',pivot)
 hinges=[]
 for yy in (.235,1.08,H-.235):
  hinges.append(cylinder('Butt_hinge_barrel',(hingex,yy,-.030),.010,.080,13,n=8))
  hinges.append(box('Butt_hinge_leaf',c(.021,yy,-.025),(.040,.066,.005),13))
 join(hinges,'leaf_'+label+'_hinges',pivot)
 closer=box('Overhead_closer_body',c(.225,H-.155,.056),(.285,.074,.067),8,.004)
 # Spindle joins body to articulated linkage. Its centre travels with the leaf.
 spindle=cylinder('Closer_spindle',c(.187,H-.105,.078),.010,.050,13,n=8)
 join([closer,spindle],'leaf_'+label+'_closer',pivot)
 # Fixed bracket remains fixed. Arm links are root-level, rigid and sampled
 # analytically. No stretched geometry or animated nonuniform scales.
 fixed=box('Closer_frame_bracket',c(.187,H+.012,.119),(.053,.026,.065),10)
 stem=box('Closer_bracket_drop',c(.187,H-.030,.145),(.017,.090,.018),10)
 join([fixed,stem],'closer_'+label+'_frame_bracket',root)
 # A physical two-link arm in the horizontal plane.
 L1=.185;L2=.205; yy=H-.080
 arm1=box('closer_'+label+'_arm_a',(.5*L1,0,0),(L1,.012,.017),13)
 arm2=box('closer_'+label+'_arm_b',(.5*L2,0,0),(L2,.012,.015),13)
 for o in (arm1,arm2):
  # Move local mesh centre to start point and root at origin.
  for v in o.data.vertices:v.co+=o.location
  o.location=(0,0,0);o.parent=root
 # Preserve animation root metadata for future engine/native audits.
 pivot['hinge_axis']='+Y';pivot['swing_degrees']=sign*90
 pivot['nominal_leaf_width_m']=lw;pivot['leaf_bottom_m']=bottom
 moving=[]
 for fr in range(1,35):
  t=(fr-1)/33;angle=sign*PI/2*t
  pivot.rotation_euler[2]=angle;pivot.keyframe_insert(data_path='rotation_euler',frame=fr,group='exit_open')
  # local engine hinge coordinate of the closer spindle.
  lx=sign*.187;lz=.078+.030
  px=hingex+math.cos(angle)*lx+math.sin(angle)*lz
  pz=-.030-math.sin(angle)*lx+math.cos(angle)*lz
  qx=hingex+sign*.187;qz=.145
  dx=qx-px;dz=qz-pz;dist=math.hypot(dx,dz)
  assert abs(L1-L2)+1e-6<dist<L1+L2-1e-6
  along=(L1*L1-L2*L2+dist*dist)/(2*dist);height=math.sqrt(max(0,L1*L1-along*along))
  # Mirrored elbow solutions match reference's shallow forward fold.
  ex=px+along*dx/dist-sign*height*dz/dist
  ez=pz+along*dz/dist+sign*height*dx/dist
  for o,a,b in ((arm1,(px,pz),(ex,ez)),(arm2,(ex,ez),(qx,qz))):
   o.location=xyz((a[0],yy,a[1]));o.rotation_euler[2]=-math.atan2(b[1]-a[1],b[0]-a[0])
   o.keyframe_insert(data_path='location',frame=fr,group='exit_open')
   o.keyframe_insert(data_path='rotation_euler',frame=fr,group='exit_open')
 for o in (pivot,arm1,arm2):
  for fc in o.animation_data.action.fcurves:
   for k in fc.keyframe_points:k.interpolation='LINEAR'
 return pivot
def setup_preview(W,H):
 scene=bpy.context.scene;scene.render.engine='CYCLES';scene.cycles.device='CPU';scene.cycles.samples=64
 scene.cycles.use_denoising=False;scene.render.resolution_x=1000;scene.render.resolution_y=1100;scene.render.resolution_percentage=100
 scene.world.color=(.16,.16,.16);scene.view_settings.view_transform='AgX'
 # Stage is outside the export collection. It is a neutral review set only.
 bpy.ops.mesh.primitive_plane_add(size=200,location=(0,0,-.001))
 floor=bpy.context.object;floor.name='QA_STAGE_FLOOR'
 m=bpy.data.materials.new('QA neutral floor');m.diffuse_color=(.18,.195,.20,1);m.use_nodes=True
 m.node_tree.nodes['Principled BSDF'].inputs['Base Color'].default_value=(.18,.195,.20,1)
 m.node_tree.nodes['Principled BSDF'].inputs['Roughness'].default_value=.82;floor.data.materials.append(m)
 def light(name,p,power,size):
  d=bpy.data.lights.new(name,'AREA');d.energy=power;d.shape='DISK';d.size=size
  o=bpy.data.objects.new(name,d);bpy.context.collection.objects.link(o);o.location=xyz(p)
  o.rotation_euler=(vec((0,H/2,0))-o.location).to_track_quat('-Z','Y').to_euler()
 light('QA large softbox',(-3,4,4),650,4)
 light('QA fill',(3,2.6,1.5),380,3)
 light('QA rear rim',(-1,3,-3),500,2)
 d=bpy.data.cameras.new('QA_CAMERA');cam=bpy.data.objects.new('QA_CAMERA',d);bpy.context.collection.objects.link(cam);scene.camera=cam
 return cam
def render(name,view,p,target,scale,cam):
 scene=bpy.context.scene;cam.location=xyz(p);cam.rotation_euler=(vec(target)-cam.location).to_track_quat('-Z','Y').to_euler()
 cam.data.type='ORTHO';cam.data.ortho_scale=scale
 scene.render.filepath=str(R/'qa'/f'{name}_{view}.png');bpy.ops.render.render(write_still=True)
def build(variant,W,n):
 init();H=2.15;root=empty('exit_door_'+variant);root['asset_class']='rigid_animated_door'
 frame(W,H,root)
 if n==1:make_leaf(W-.012,H,-W/2+.006,1,'L',root)
 else:
  # Meeting gap 6 mm. Equal leaves have no central fixed mullion.
  lw=(W-.012-.006)/2
  make_leaf(lw,H,-W/2+.006,1,'L',root)
  make_leaf(lw,H,W/2-.006,-1,'R',root)
 scene=bpy.context.scene;scene.name='exit_open';scene.frame_set(1);bpy.context.view_layer.update()
 # Centre the closed occupied footprint once, without moving any hinge relative
 # to its leaf/frame or changing a mechanical transform. The host plane offset
 # is recorded by gates and must be respected during later placement.
 allpts=[o.matrix_world@v.co for o in root.children_recursive if o.type=='MESH' for v in o.data.vertices]
 cx=(min(v.x for v in allpts)+max(v.x for v in allpts))/2
 cy=(min(v.y for v in allpts)+max(v.y for v in allpts))/2
 root.location=(-cx,-cy,0);root['closed_footprint_centering_native_xy']=[-cx,-cy]
 bpy.context.view_layer.update()
 asset_objects=[root]+list(root.children_recursive)
 for o in asset_objects:o.select_set(True)
 bpy.context.view_layer.objects.active=root
 # One grouped scene animation, not an exporter action per arm or leaf.
 kwargs=dict(filepath=str(R/'runtime'/f'hollow_metal_exit_door_{variant}.glb'),
  export_format='GLB',use_selection=True,export_yup=True,export_apply=False,
  export_animations=True,export_animation_mode='SCENE',export_anim_scene_split_object=False,
  export_nla_strips_merged_animation_name='exit_open',export_frame_range=True,
  export_force_sampling=True,export_optimize_animation_size=False,export_anim_slide_to_zero=True,export_extras=True,
  export_normals=True,export_tangents=False,export_materials='EXPORT',
  export_image_format='AUTO',export_texcoords=True,
  export_cameras=False,export_lights=False)
 allowed=bpy.ops.export_scene.gltf.get_rna_type().properties.keys()
 bpy.ops.export_scene.gltf(**{k:v for k,v in kwargs.items() if k in allowed})
 # Save editable scene and inspectable CPU previews. Export never includes stage.
 cam=setup_preview(W,H)
 scene.frame_set(1)
 bpy.ops.wm.save_as_mainfile(filepath=str(R/'source'/f'hollow_metal_exit_door_{variant}.blend'))
 if not os.environ.get('EXIT_DOOR_SKIP_RENDERS'):
  render(variant,'closed_front',(W*.70,2.6,4.5),(0,1.06,0),2.75 if n==1 else 2.98,cam)
  render(variant,'closed_back',(-W*.7,2.6,-4.5),(0,1.06,0),2.75 if n==1 else 2.98,cam)
  scene.frame_set(34);render(variant,'open_90',(W*1.4,2.8,4.5),(0,1.05,-.2),3.05 if n==1 else 3.25,cam)
  scene.frame_set(1);render(variant,'hardware',(.05,1.22,3),((W/2-.11)if n==1 else 0,1.03,.04),.78,cam)
 return {'variant':variant,'opening':[W,H],'frame_depth':.196,'leaf_thickness':.045,
  'duration_seconds':1.1,'clip':'exit_open','leaf_count':n}
if __name__=='__main__':
 info=[build('single',1.0,1),build('double',1.9,2)]
 json.dump(info,open(R/'metadata/design_dimensions.json','w'),indent=2)
 print('EXIT_DOOR_BLENDER_AUTHORING_OK',flush=True)
