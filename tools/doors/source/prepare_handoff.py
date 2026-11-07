"""Write measured collision/placement proposals and provenance, not project files."""
from pathlib import Path
import json,hashlib,sys
import numpy as np
from PIL import Image
R=Path(__file__).resolve().parents[1];sys.path.insert(0,str(R/'source'))
from validate_exit_doors import GLB,bounds
def write(name,data):json.dump(data,open(R/'metadata'/name,'w'),indent=2)
def digest(path):return hashlib.sha256(path.read_bytes()).hexdigest()
reports=json.load(open(R/'metadata/asset_gate_report.json'));proposals=[]
for variant,W,n in [('single',1.,1),('double',1.9,2)]:
 g=GLB(R/'runtime'/('hollow_metal_exit_door_'+variant+'.glb'));j=g.j
 names=[x['name']for x in j['nodes']];root='exit_door_'+variant;root_index=names.index(root)
 closed=g.poses(0);opened=g.poses(1.1);host_z=float(closed[root_index][2,3])
 boxes=[]
 def b(node,center,size):boxes.append({'node':node,'center':[round(float(v),9)for v in center],'size':[round(float(v),9)for v in size]})
 # Frame surrounds the opening, never one aperture-filling solid box.
 for side in (-1,1):b(root,[side*(W/2+.034),1.075,0],[.084,2.15,.196])
 b(root,[0,2.176,0],[W+.152,.068,.196]);b(root,[0,.004,0],[W,.008,.15])
 for ni,node in enumerate(j['nodes']):
  nm=node['name']
  if 'mesh'not in node or nm=='frame_stock':continue
  points=np.concatenate([g.acc(p['attributes']['POSITION'])for p in j['meshes'][node['mesh']]['primitives']])
  lo=points.min(0);hi=points.max(0)
  b(nm,(lo+hi)/2,hi-lo)
 assert 1<=len(boxes)<=32
 rep=next(v for v in reports['models']if v['variant']==variant)
 side_indices={}
 for side in (['L']if n==1 else ['L','R']):
  side_indices[side]=[i for i,node in enumerate(j['nodes'])if node['name'].startswith('leaf_'+side+'_')]
 left=g.points(opened,side_indices['L'])
 clear_min=max(-W/2+.008,float(left[:,0].max()))
 if n==2:
  right=g.points(opened,side_indices['R']);clear_max=min(W/2-.008,float(right[:,0].min()))
 else:clear_max=W/2-.008
 width=clear_max-clear_min
 assert width>=.8
 placement={'id':'review_hollow_metal_exit_'+variant,'model':'core:hollow_metal_exit_door_'+variant,
 'x':0.,'y':0.,'z':round(-host_z,9),'rotation_degrees':0.,'scale':1.,
 'size':[round(x,6)for x in rep['bounds_closed']['size']],'solid':False,
 'animated_door':{'clip':'exit_open','initial_open':False,'boxes':boxes},
 'components':[{'component':'interactable','reach':2.0},{'component':'animation','clip':'exit_open','looped':False,'playing':False}],
 'bindings':[{'on':'interact','actions':[{'action':'toggle_animation','clip':'exit_open'}]}]}
 proposals.append({'variant':variant,'logical_id_suggestion':placement['model'],
 'display_name_suggestion':'Flush hollow-metal exit door'+(' pair'if n==2 else '')+' with surface vertical-rod exit device',
 'runtime_file':'runtime/'+g.path.name,'catalog_size_m':placement['size'],
 'nominal_host_opening_m':[W,2.15],'host_wall_depth_m':.160,
 'host_wall_centre_plane_local_z_m':host_z,
 'proposed_placement_aligns_to_host_world_plane_z':0.,
 'closed_leaf_bottom_m':.014,'closed_leaf_top_m':2.142,'threshold_top_m':.008,
 'frame_clear_width_m':W-.016,'frame_clear_height_above_threshold_m':2.134,
 'full_open_conservative_body_path_width_m':width,
 'full_open_body_path_x_interval_m':[clear_min,clear_max],
 'player_hull_comparison_m':[.8,1.8],'player_width_margin_m':width-.8,
 'collision_box_count':len(boxes),'collision_profile':'Conservative rigid boxes, no mesh collision. Hardware group boxes deliberately include air around rods.',
 'placement_proposal_not_engine_validated':placement,
 'behavior':'Reversible 1.1 second open clip; starts closed. Pair opens both leaves together. Closing is another toggle; overhead closer appearance does not implement automatic closing.',
 'not_supported_by_this_delivery':['per-leaf independent pair controls','native slab DoorKind','native door lock/state API integration','opposite-hand single variant','certified building-egress behavior']})
write('integration_proposals.json',{'baseline_commit':reports['baseline_commit'],'status':'PRIVATE PROPOSALS, NOT INSTALLED OR ENGINE VALIDATED','models':proposals})
ref=R/'reference/image.png';im=Image.open(ref)
write('provenance.json',{'status':'prepared_privately_ready_for_later_integration','canonical_repository_modified':False,
 'requested_reference_library_id':'libfile_cfe2f74081d8819185ef90ab92d370af',
 'resolved_materialization_library_id':'libfile_fde3_f-cfe2f74081d8819185ef90ab92d370af_2e37728cdad08191acacf8a0401feb09',
 'reference':{'path':'reference/image.png','sha256':digest(ref),'bytes':ref.stat().st_size,'size':im.size,'pixels_inspected':True},
 'generic_terminology':'Flush hollow-metal exit door / pair with surface-mounted vertical-rod exit device (panic hardware).',
 'terminology_sources':[{'url':'https://steeldoor.org/glossary/','purpose':'Generic hollow-metal/flush door nomenclature.'},
 {'url':'https://kc.allegion.com/kb/article/surface-vertical-rod-exit-device/','purpose':'Generic visible surface-applied rods connected to top and bottom latches.'}],
 'identification_limits':'No brand, fire rating, metal gauge, tested security class, safety listing or certification is asserted.',
 'reference_interpretation':'Flat grey leaves, metal frame, top/bottom exposed rods, touchbars, overhead closers; reference shows pair. Single variant adapts that visible hardware arrangement.',
 'tools':{'modelling':'Blender 4.3.2','painting':'GIMP 3.0.4','preview':'Cycles CPU only; no GPU benchmarks','inspection':'Python deterministic GLB gates plus Blender re-import'},
 'originality':'Original geometry and native GIMP paint. No image-generation stand-ins, downloaded models or third-party textures.',
 'reference_rights':'Reference retained unchanged for comparison; this preparation makes no new rights claim over that image.',
 'engine_budget_changes':None,'new_runtime_formats':None,'new_runtime_extensions':None,
 'native_acceptance':False,'Mac_commands':False,'source_integration':False,
 'scope_end':'Pause after private preparation. Later integration and native/Windows validation remain queued. No map layout or level build.'})
identities=[]
for folder in ('runtime','source','textures'):
 for p in sorted((R/folder).glob('*')):
  if p.is_file()and p.suffix in ('.glb','.blend','.xcf','.png','.py'):
   identities.append({'path':p.relative_to(R).as_posix(),'bytes':p.stat().st_size,'sha256':digest(p)})
write('artifact_identities.json',{'files':identities,'unique_runtime_texture_bytes_decoded_rgba8':262144,
 'two_embedded_copies_before_engine_dedup_bytes_decoded_rgba8':524288,
 'pack_128MiB_budget_not_remeasured_here':True})
print(json.dumps([{'variant':p['variant'],'body_clear_width_m':p['full_open_conservative_body_path_width_m'],'boxes':p['collision_box_count']}for p in proposals],indent=2))
