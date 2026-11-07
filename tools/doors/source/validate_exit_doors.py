"""Deterministic offline GLB gates; no game/native acceptance is implied."""
from pathlib import Path
import json,struct,hashlib,math,io
import numpy as np
from PIL import Image
R=Path(__file__).resolve().parents[1]
COMP={5120:np.int8,5121:np.uint8,5122:np.int16,5123:np.uint16,5125:np.uint32,5126:np.float32}
WIDTH={'SCALAR':1,'VEC2':2,'VEC3':3,'VEC4':4,'MAT4':16}
def sha(b):return hashlib.sha256(b).hexdigest()
def matrix(node):
 t=np.array(node.get('translation',[0,0,0]),float);q=np.array(node.get('rotation',[0,0,0,1]),float)
 x,y,z,w=q;M=np.eye(4)
 M[:3,:3]=np.array([[1-2*y*y-2*z*z,2*x*y-2*z*w,2*x*z+2*y*w],[2*x*y+2*z*w,1-2*x*x-2*z*z,2*y*z-2*x*w],[2*x*z-2*y*w,2*y*z+2*x*w,1-2*x*x-2*y*y]])@np.diag(node.get('scale',[1,1,1]))
 M[:3,3]=t
 if 'matrix'in node:M=np.array(node['matrix']).reshape((4,4),order='F')
 return M
def slerp(a,b,t):
 d=float(np.dot(a,b))
 if d<0:b=-b;d=-d
 if d>.9995:r=a+(b-a)*t;return r/np.linalg.norm(r)
 theta=math.acos(max(-1,min(1,d)))
 return (math.sin((1-t)*theta)*a+math.sin(t*theta)*b)/math.sin(theta)
class GLB:
 def __init__(self,path):
  self.path=path;self.data=path.read_bytes();b=self.data
  assert b[:4]==b'glTF' and struct.unpack_from('<II',b,4)==(2,len(b))
  n,kind=struct.unpack_from('<I4s',b,12);assert kind==b'JSON';self.j=json.loads(b[20:20+n])
  off=20+n;m,k=struct.unpack_from('<I4s',b,off);assert k==b'BIN\x00';self.bin=b[off+8:off+8+m]
 def acc(self,i):
  a=self.j['accessors'][i];assert 'sparse'not in a
  v=self.j['bufferViews'][a['bufferView']];dtype=np.dtype(COMP[a['componentType']]).newbyteorder('<')
  w=WIDTH[a['type']];stride=v.get('byteStride',dtype.itemsize*w);off=v.get('byteOffset',0)+a.get('byteOffset',0)
  x=np.ndarray((a['count'],w),dtype=dtype,buffer=self.bin,offset=off,strides=(stride,dtype.itemsize)).copy()
  if a.get('normalized'):x=x.astype(float)/np.iinfo(dtype).max
  return x
 def poses(self,time):
  ns=[dict(n)for n in self.j['nodes']]
  for anim in self.j.get('animations',[]):
   for ch in anim['channels']:
    s=anim['samplers'][ch['sampler']];ts=self.acc(s['input']).ravel();vs=self.acc(s['output'])
    k=max(0,min(len(ts)-2,int(np.searchsorted(ts,time,side='right'))-1))
    f=0 if ts[-1]==ts[0] else max(0,min(1,(time-ts[k])/(ts[k+1]-ts[k])))
    if s.get('interpolation','LINEAR')=='STEP':f=0
    path=ch['target']['path'];a=vs[k].astype(float);b=vs[k+1].astype(float)
    value=slerp(a,b,f)if path=='rotation'else a+(b-a)*f
    ns[ch['target']['node']][path]=value
  result={}
  def visit(idx,parent):
   M=parent@matrix(ns[idx]);result[idx]=M
   for ch in ns[idx].get('children',[]):visit(ch,M)
  for i in self.j['scenes'][self.j.get('scene',0)]['nodes']:visit(i,np.eye(4))
  return result
 def points(self,poses,indices=None):
  pts=[]
  for i,n in enumerate(self.j['nodes']):
   if 'mesh'not in n or(indices is not None and i not in indices):continue
   for p in self.j['meshes'][n['mesh']]['primitives']:
    v=self.acc(p['attributes']['POSITION']);M=poses[i]
    pts.append(v@M[:3,:3].T+M[:3,3])
  return np.concatenate(pts)if pts else np.zeros((0,3))
def bounds(v):return {'min':v.min(0).tolist(),'max':v.max(0).tolist(),'size':np.ptp(v,axis=0).tolist()}
def validate(path,variant):
 g=GLB(path);j=g.j
 assert len(j['scenes'])==1
 assert not j.get('skins')and not j.get('extensionsRequired')
 assert set(j.get('extensionsUsed',[]))<=set(['KHR_materials_emissive_strength'])
 assert len(j.get('materials',[]))<=16 and len(j.get('images',[]))<=16
 assert len(j.get('animations',[]))==1 and j['animations'][0]['name']=='exit_open'
 names=[n.get('name')for n in j['nodes']];assert len(names)==len(set(names))and None not in names
 root='exit_door_'+variant;assert root in names
 expected=['L']if variant=='single'else ['L','R']
 for side in expected:
  for part in ('core','panic_hardware','hinges','closer'):assert 'leaf_'+side+'_'+part in names
  assert 'hinge_'+side in names
 triangles=vertices=primitives=0;min_area=1;min_winding=1;umin=1;umax=0;mesh_records=[]
 for mi,mesh in enumerate(j['meshes']):
  tris=verts=0
  for p in mesh['primitives']:
   primitives+=1;assert p.get('mode',4)==4
   assert {'POSITION','NORMAL','TEXCOORD_0'}<=set(p['attributes'])
   assert not p.get('targets')
   v=g.acc(p['attributes']['POSITION']);n=g.acc(p['attributes']['NORMAL']);uv=g.acc(p['attributes']['TEXCOORD_0'])
   idx=g.acc(p['indices']).ravel();assert len(idx)%3==0 and idx.max()<len(v)and idx.max()<65536
   assert np.isfinite(v).all()and np.isfinite(n).all()and np.isfinite(uv).all()
   assert np.max(np.abs(np.linalg.norm(n,axis=1)-1))<1e-5
   assert uv.min()>=-.01 and uv.max()<=1.01
   umin=min(umin,float(uv.min()));umax=max(umax,float(uv.max()))
   a,b,c=v[idx.reshape(-1,3)].transpose((1,0,2));cross=np.cross(b-a,c-a);area=np.linalg.norm(cross,axis=1)
   assert area.min()>1e-10
   vn=n[idx.reshape(-1,3)].mean(1);alignment=np.sum(cross*vn,axis=1)/area
   assert alignment.min()>.90
   min_area=min(min_area,float(area.min()/2));min_winding=min(min_winding,float(alignment.min()))
   tris+=len(idx)//3;verts+=len(v)
  mesh_records.append({'name':mesh.get('name'), 'triangles':tris,'vertices':verts})
  triangles+=tris;vertices+=verts
 assert triangles<=1500 and triangles<=6000 and vertices<=65535 and primitives<=32
 image_records=[];standalone=(R/'textures/exit_door_materials.png').read_bytes()
 for im in j['images']:
  assert im['mimeType']=='image/png'and 'uri'not in im
  view=j['bufferViews'][im['bufferView']];data=g.bin[view.get('byteOffset',0):view.get('byteOffset',0)+view['byteLength']]
  image=Image.open(io.BytesIO(data));assert image.size==(256,256)
  assert image.convert('RGBA').getextrema()[3]==(255,255)
  assert data==standalone,'embedded PNG bytes differ from standalone'
  image_records.append({'sha256':sha(data),'bytes':len(data),'size':image.size,'decoded_rgba_bytes':256*256*4})
 for mat in j['materials']:
  assert mat.get('alphaMode','OPAQUE')=='OPAQUE'and mat.get('emissiveFactor',[0,0,0])==[0,0,0]
  assert mat.get('doubleSided',False)==False
  pm=mat['pbrMetallicRoughness'];assert 0<=pm['roughnessFactor']<=1 and 0<=pm['metallicFactor']<=1
 clip=j['animations'][0];durations=[];channels=[]
 for c in clip['channels']:
  s=clip['samplers'][c['sampler']];assert s.get('interpolation','LINEAR')in ('LINEAR','STEP')
  assert c['target']['path']in ('translation','rotation')
  ts=g.acc(s['input']).ravel();vals=g.acc(s['output'])
  assert np.all(np.diff(ts)>0)and np.isfinite(vals).all()and abs(ts[0])<1e-6
  durations.append(float(ts[-1]));channels.append({'node':names[c['target']['node']],'path':c['target']['path']})
 assert len(channels)<=4096 and max(durations)-min(durations)<1e-6 and abs(max(durations)-1.1)<1e-6
 B=[];open_points=None;closed_poses=None
 for f in np.linspace(0,1,281):
  poses=g.poses(float(f)*1.1)
  for i,M in poses.items():
   assert np.allclose(M[:3,:3].T@M[:3,:3],np.eye(3),atol=2e-5)
   assert np.allclose(M[:3,1],[0,1,0],atol=2e-5),'non-upright mechanism node'
   assert abs(np.linalg.det(M[:3,:3])-1)<2e-5
  pts=g.points(poses);B.append(bounds(pts))
  if f==0:closed_poses=poses
  if f==1:open_points=pts
 fixed_nodes=[i for i,n in enumerate(j['nodes'])if n['name']=='frame_stock'or 'frame_bracket'in n['name']]
 for i in fixed_nodes:assert np.allclose(g.poses(0)[i],g.poses(1.1)[i],atol=1e-7)
 closed_bounds=bounds(g.points(closed_poses))
 assert abs(closed_bounds['min'][1])<=.012
 assert max(abs((closed_bounds['min'][axis]+closed_bounds['max'][axis])/2)for axis in (0,2))<=.02
 linkage_error=0.0
 for f in np.linspace(0,1,281):
  poses=g.poses(float(f)*1.1)
  for side in expected:
   sign=1 if side=='L'else -1
   A=poses[names.index('closer_'+side+'_arm_a')];MB=poses[names.index('closer_'+side+'_arm_b')]
   hinge=poses[names.index('hinge_'+side)];rr=poses[names.index(root)]
   anchor=(rr@np.array([(-.494 if variant=='single'else sign*-.944)+sign*.187,2.07,.145,1]))[:3]
   start=(hinge@np.array([sign*.187,2.07,.108,1]))[:3]
   errors=[np.linalg.norm(A[:3,3]-start),np.linalg.norm((A@np.array([.185,0,0,1]))[:3]-MB[:3,3]),np.linalg.norm((MB@np.array([.205,0,0,1]))[:3]-anchor)]
   linkage_error=max(linkage_error,max(errors))
 assert linkage_error<.0003,('closer link gap',linkage_error)
 hinge_records=[]
 for side in expected:
  ni=names.index('hinge_'+side);M0=g.poses(0)[ni];M1=g.poses(1.1)[ni]
  assert np.allclose(M0[:3,3],M1[:3,3])
  angle=math.degrees(math.atan2(M1[0,2],M1[0,0]))
  assert abs(angle-(90 if side=='L'else -90))<1e-3
  hinge_records.append({'node':'hinge_'+side,'position':M0[:3,3].tolist(),'axis':[0,1,0],'open_degrees':angle})
 # Every local mesh is closed orientable stock. Count welded boundary edges
 # per disconnected component; touching assemblies are retained separately.
 # The geometric gate uses exact float-rounded edges so flat-normal seams
 # do not masquerade as holes.
 topology=[]
 for mesh in j['meshes']:
  edgecounts={};posidx={};signed_volume=0
  for p in mesh['primitives']:
   v=g.acc(p['attributes']['POSITION']);idx=g.acc(p['indices']).ravel().reshape(-1,3)
   ids=[]
   for vv in v:
    key=tuple(round(float(x),7)for x in vv)
    if key not in posidx:posidx[key]=len(posidx)
    ids.append(posidx[key])
   for a,b,c in idx:
    signed_volume+=float(np.dot(v[a],np.cross(v[b],v[c]))/6)
    for aa,bb in ((a,b),(b,c),(c,a)):
     key=tuple(sorted((ids[aa],ids[bb])));edgecounts[key]=edgecounts.get(key,0)+1
  boundary=sum(n==1 for n in edgecounts.values());nonmanifold=sum(n>2 for n in edgecounts.values())
  assert boundary==0,('open stock',mesh.get('name'),boundary)
  assert signed_volume>0,('inverted stock',mesh.get('name'),signed_volume)
  # Coincident contact edges at explicitly joined closed stock can have four
  # incident triangles; report rather than treating compound stock as one shell.
  topology.append({'mesh':mesh.get('name'),'boundary_edges':boundary,'compound_contact_edges':nonmanifold,'signed_volume_m3':signed_volume})
 record={'file':path.name,'sha256':sha(g.data),'bytes':len(g.data),'variant':variant,
 'triangles':triangles,'vertices':vertices,'primitives':primitives,'materials':[m['name']for m in j['materials']],
 'images':image_records,'joints':0,'nodes':names,'clip':{'name':'exit_open','duration_seconds':max(durations),'channels':channels},
 'bounds_closed':closed_bounds,'max_closer_endpoint_gap_m':float(linkage_error),'bounds_open_90':bounds(open_points),
 'bounds_sweep_281_samples':{'min':np.min([b['min']for b in B],0).tolist(),'max':np.max([b['max']for b in B],0).tolist()},
 'hinges':hinge_records,'min_triangle_area_m2':min_area,'min_normal_winding_alignment':min_winding,
 'uv_min_max':[umin,umax],'mesh_budgets':mesh_records,'topology':topology,
 'gates_passed':['container/profile','6000 engine ceiling unchanged','1500 art budget','primitive/material/image/vertex caps','embedded standalone opaque PNG parity','finite nondegenerate outward normals','fitted UVs','closed stock','stable node identities','floor contact and centred closed footprint','281 upright rigid clip samples','281 closer-link endpoint samples <=0.3 mm','fixed frame and hinge origins','exact 90-degree mirrored leaves'],
 'not_performed':['Places importer execution','native interaction/collision','editor placement','runtime lighting/floor grounding/self-shadow/reflections','Windows validation']}
 return record
if __name__=='__main__':
 reports=[]
 for variant in ('single','double'):reports.append(validate(R/'runtime'/('hollow_metal_exit_door_'+variant+'.glb'),variant))
 json.dump({'status':'PASS','baseline_commit':'624e4da8273200e7c5b843c106208a5afa7c1a68','models':reports},open(R/'metadata/asset_gate_report.json','w'),indent=2)
 print(json.dumps({'status':'PASS','models':[{'file':r['file'],'triangles':r['triangles'],'primitives':r['primitives'],'vertices':r['vertices'],'sha256':r['sha256']}for r in reports]},indent=2))
 
