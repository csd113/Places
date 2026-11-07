"""Reopen each .blend, prove byte-identical re-export, then render actual GLBs.
Blender previews are offline evidence, not native Places acceptance.
"""
import bpy,os,json,hashlib,sys
from pathlib import Path
R=Path(os.environ['EXIT_DOOR_ROOT']);sys.path.insert(0,str(R/'source'))
import build_exit_doors as author
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
records=[]
(R/'qa/reexport').mkdir(exist_ok=True)
for variant,W,n in [('single',1.0,1),('double',1.9,2)]:
 source=R/'source'/('hollow_metal_exit_door_'+variant+'.blend');runtime=R/'runtime'/('hollow_metal_exit_door_'+variant+'.glb')
 bpy.ops.wm.open_mainfile(filepath=str(source));scene=bpy.context.scene;scene.frame_set(1)
 assert scene.unit_settings.scale_length==1
 root=bpy.data.objects['exit_door_'+variant]
 assert root['asset_class']=='rigid_animated_door'
 bpy.ops.object.select_all(action='DESELECT')
 for o in [root]+list(root.children_recursive):o.select_set(True)
 bpy.context.view_layer.objects.active=root
 out=R/'qa/reexport'/runtime.name
 kwargs=dict(filepath=str(out),export_format='GLB',use_selection=True,export_yup=True,export_apply=False,
 export_animations=True,export_animation_mode='SCENE',export_anim_scene_split_object=False,
 export_nla_strips_merged_animation_name='exit_open',export_frame_range=True,export_force_sampling=True,
 export_optimize_animation_size=False,export_anim_slide_to_zero=True,export_extras=True,export_normals=True,export_tangents=False,
 export_materials='EXPORT',export_image_format='AUTO',export_texcoords=True,export_cameras=False,export_lights=False)
 allowed=bpy.ops.export_scene.gltf.get_rna_type().properties.keys()
 bpy.ops.export_scene.gltf(**{k:v for k,v in kwargs.items()if k in allowed})
 assert sha(out)==sha(runtime),('source re-export identity differs',variant,sha(out),sha(runtime))
 # Inspect and render the actual runtime export in a fresh Blender scene.
 bpy.ops.wm.read_factory_settings(use_empty=True)
 scene=bpy.context.scene;scene.render.fps=30
 bpy.ops.import_scene.gltf(filepath=str(runtime))
 names=[o.name for o in scene.objects]
 assert 'exit_door_'+variant in names and 'frame_stock'in names
 assert 'hinge_L'in names and ('hinge_R'in names)==(n==2)
 scene.world=bpy.data.worlds.new('QA_WORLD')
 cam=author.setup_preview(W,2.15)
 report=json.load(open(R/'metadata/asset_gate_report.json'))
 original=next(m for m in report['models']if m['variant']==variant)
 sample_bounds={}
 for frame,key in [(0,'bounds_closed'),(33,'bounds_open_90')]:
  scene.frame_set(frame);bpy.context.view_layer.update()
  pts=[author.eng(o.matrix_world@v.co)for o in scene.objects if o.type=='MESH' and o.name!='QA_STAGE_FLOOR'for v in o.data.vertices]
  mn=[min(p[i]for p in pts)for i in range(3)];mx=[max(p[i]for p in pts)for i in range(3)]
  assert max(abs(a-b)for a,b in zip(mn,original[key]['min']))<2e-5,(variant,key,mn,original[key])
  assert max(abs(a-b)for a,b in zip(mx,original[key]['max']))<2e-5
  sample_bounds[key]={'min':mn,'max':mx}
 scene.frame_set(33)
 author.render(variant,'corrected_open',(W*1.4,2.8,4.5),(0,1.05,-.2),3.05 if n==1 else 3.25,cam)
 if variant=='double':
  scene.frame_set(0)
  author.render(variant,'corrected_rear',(-W*.7,2.6,-4.5),(0,1.06,0),2.98,cam)
 records.append({'variant':variant,'blend_file':source.name,'blend_sha256':sha(source),
 'runtime_sha256':sha(runtime),'reexport_sha256':sha(out),'byte_identical_source_reexport':True,
 'imported_nodes':names,'imported_bounds':sample_bounds,'preview_source':'actual runtime GLB re-import',
 'renderer':'Blender 4.3.2 Cycles CPU, 4 threads, 64 samples, no denoising'})
 json.dump({'status':'PASS','models':records},open(R/'metadata/blender_reopen_import_gate.json','w'),indent=2)
 print('SOURCE_REEXPORT_AND_GLB_IMPORT_PASS',variant,flush=True)
print('ALL_OFFLINE_REOPEN_RENDER_GATES_PASS',flush=True)

