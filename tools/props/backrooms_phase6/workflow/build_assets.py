import os,sys,math,json,argparse
ROOT=os.environ.get('PLACES_PHASE6_ROOT','/workspace/shared/places-phase6')
sys.path.insert(0,ROOT+'/workflow')
import bpy
from mathutils import Vector
import hvac,doors_electrical,ceilings_fixtures
BUILDERS={**hvac.BUILDERS,**doors_electrical.BUILDERS,**ceilings_fixtures.BUILDERS}
p=argparse.ArgumentParser();p.add_argument('--pass',dest='pass_no',type=int,default=1);p.add_argument('--render',action='store_true');p.add_argument('--only',nargs='*');args=p.parse_args(sys.argv[sys.argv.index('--')+1:]if'--'in sys.argv else[])
names=args.only or list(BUILDERS)
for name in names:BUILDERS[name](args.pass_no)
if not args.render:sys.exit(0)
bpy.ops.object.select_all(action='SELECT');bpy.ops.object.delete(use_global=False)
rows=math.ceil(len(names)/5);cols=min(5,len(names));tile=2.5
for i,name in enumerate(names):
 path=ROOT+('/models'if args.pass_no==3 else'/passes/pass'+str(args.pass_no))+'/'+name+'.glb'
 bpy.ops.import_scene.gltf(filepath=path)
 objs=[o for o in bpy.context.selected_objects if o.type=='MESH'];verts=[o.matrix_world@v.co for o in objs for v in o.data.vertices];lo=Vector(tuple(min(p[j]for p in verts)for j in range(3)));hi=Vector(tuple(max(p[j]for p in verts)for j in range(3)));center=(lo+hi)/2;scale=1.6/max(hi-lo)
 x=(i%5-(cols-1)/2)*tile;y=(rows-1-i//5)*tile
 for o in objs:
  o.location-=center;o.location*=scale;o.scale*=scale
  if name.startswith(('suspended_ceiling','fluorescent')):
   # Rotate entire fixture for underside inspection; never applied to exported asset.
   from mathutils import Matrix
   rot=Matrix.Rotation(math.pi,4,'X');o.matrix_world=rot@o.matrix_world
  o.location+=Vector((x,y,.88))
 bpy.ops.object.text_add(location=(x,y-1.02,.05));t=bpy.context.object;t.name='QA label '+name;t.data.body=name.replace('institutional','').replace('suspended_ceiling_','ceiling_').rstrip('_');t.data.align_x='CENTER';t.data.size=.105;t.data.extrude=0
 bpy.ops.mesh.primitive_plane_add(size=2.33,location=(x,y,0));floor=bpy.context.object;floor.name='QA tile';m=bpy.data.materials.get('qa_tile')
 if not m:
  m=bpy.data.materials.new('qa_tile');m.diffuse_color=(.14,.165,.16,1);m.use_nodes=True;m.node_tree.nodes.get('Principled BSDF').inputs['Base Color'].default_value=(.14,.165,.16,1);m.node_tree.nodes.get('Principled BSDF').inputs['Roughness'].default_value=1
 floor.data.materials.append(m)
# Orthographic near-top shows labelled contact sheet plus three dimensional structure.
scene=bpy.context.scene;scene.render.engine='CYCLES';scene.cycles.samples=24;scene.cycles.use_denoising=False;scene.render.threads_mode='FIXED';scene.render.threads=10
scene.world.color=(.32,.32,.30);scene.world.use_nodes=True;scene.world.node_tree.nodes['Background'].inputs['Color'].default_value=(.40,.45,.43,1);scene.world.node_tree.nodes['Background'].inputs['Strength'].default_value=.7
for pos,power,size in [((0,-4,14),1900,10),((-9,8,10),1600,8)]:
 bpy.ops.object.light_add(type='AREA',location=pos);l=bpy.context.object;l.data.energy=power;l.data.shape='DISK';l.data.size=size;l.rotation_euler=(Vector((0,(rows-1)*tile/2,0))-l.location).to_track_quat('-Z','Y').to_euler()
bpy.ops.object.camera_add(location=(0,(rows-1)*tile/2-9,19));cam=bpy.context.object;target=Vector((0,(rows-1)*tile/2,.3));cam.rotation_euler=(target-cam.location).to_track_quat('-Z','Y').to_euler();cam.data.type='ORTHO';cam.data.ortho_scale=max(cols*tile,rows*tile*1.02);scene.camera=cam
scene.render.resolution_x=2048;scene.render.resolution_y=max(800,round(2048*rows/cols*.94));scene.render.resolution_percentage=100;scene.render.image_settings.file_format='PNG';scene.render.filepath=ROOT+'/qa/pass'+str(args.pass_no)+'_contact.png';scene.view_settings.view_transform='Standard';scene.view_settings.look='None';scene.view_settings.exposure=0;scene.view_settings.gamma=1
bpy.ops.wm.save_as_mainfile(filepath=ROOT+'/qa/pass'+str(args.pass_no)+'_contact.blend');bpy.ops.render.render(write_still=True)
print('CONTACT_RENDER_DONE',args.pass_no,flush=True)
