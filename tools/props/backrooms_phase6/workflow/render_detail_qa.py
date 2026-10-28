import bpy,os,math,sys,json
from mathutils import Vector,Matrix
BASE=os.environ.get('PLACES_PHASE6_ROOT','/workspace/shared/places-phase6')
names=['hvac_elbow_horizontal','hvac_elbow_up','electrical_cabinet_open','utility_double_door_wired','fluorescent_damaged','fluorescent_hanging','folding_table_institutional','stackable_chair_institutional','wall_clock_institutional','janitor_cart_existing']
for name in names:
 bpy.ops.object.select_all(action='SELECT');bpy.ops.object.delete(use_global=False)
 path=BASE+'/audit/janitor_cart.glb'if name=='janitor_cart_existing'else BASE+'/models/'+name+'.glb'
 bpy.ops.import_scene.gltf(filepath=path);objs=[o for o in bpy.context.selected_objects if o.type=='MESH'];verts=[o.matrix_world@v.co for o in objs for v in o.data.vertices];lo=Vector(tuple(min(p[i]for p in verts)for i in range(3)));hi=Vector(tuple(max(p[i]for p in verts)for i in range(3)));center=(lo+hi)/2;span=max(hi-lo)
 for o in objs:o.location-=center
 if name.startswith('fluorescent'):
  for o in objs:o.matrix_world=Matrix.Rotation(math.pi,4,'X')@o.matrix_world
 bpy.ops.mesh.primitive_plane_add(size=200,location=(0,0,-span*.55));m=bpy.data.materials.new('neutral_qa_floor');m.diffuse_color=(.17,.19,.18,1);m.use_nodes=True;m.node_tree.nodes['Principled BSDF'].inputs['Base Color'].default_value=(.17,.19,.18,1);bpy.context.object.data.materials.append(m)
 scene=bpy.context.scene;scene.render.engine='CYCLES';scene.cycles.samples=24;scene.cycles.use_denoising=False;scene.render.threads_mode='FIXED';scene.render.threads=10;scene.world.use_nodes=True;scene.world.node_tree.nodes['Background'].inputs['Color'].default_value=(.40,.44,.42,1);scene.world.node_tree.nodes['Background'].inputs['Strength'].default_value=.65
 for pos,power,size in [((2.5,-4,5),400,4),((-3,1,3),270,3)]:
  bpy.ops.object.light_add(type='AREA',location=tuple(v*span for v in pos));l=bpy.context.object;l.data.energy=power*span*span;l.data.size=size*span;l.rotation_euler=(-l.location).to_track_quat('-Z','Y').to_euler()
 camera_pos=(2.5,-4,2.1)if name not in ['hvac_elbow_horizontal','hvac_elbow_up']else(3,-4,3)
 bpy.ops.object.camera_add(location=tuple(v*span for v in camera_pos));cam=bpy.context.object;cam.rotation_euler=(-cam.location).to_track_quat('-Z','Y').to_euler();cam.data.type='ORTHO';cam.data.ortho_scale=span*1.45;scene.camera=cam
 scene.render.resolution_x=640;scene.render.resolution_y=640;scene.render.resolution_percentage=100;scene.render.image_settings.file_format='PNG';scene.render.filepath=BASE+'/qa/detail_'+name+'.png';scene.view_settings.view_transform='Standard';scene.view_settings.look='None';bpy.ops.render.render(write_still=True);print('DETAIL_QA_DONE',name,flush=True)
