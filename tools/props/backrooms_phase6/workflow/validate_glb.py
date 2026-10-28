import json,struct,os,glob,math,hashlib,io,collections
from PIL import Image
BASE=os.environ.get('PLACES_PHASE6_ROOT','/workspace/shared/places-phase6')
SIZES={'SCALAR':1,'VEC2':2,'VEC3':3,'VEC4':4,'MAT4':16};FMT={5120:('b',1),5121:('B',1),5122:('h',2),5123:('H',2),5125:('I',4),5126:('f',4)}
def load(path):
 b=open(path,'rb').read();magic,version,length=struct.unpack_from('<III',b);assert magic==0x46546c67 and version==2 and length==len(b)
 n,t=struct.unpack_from('<II',b,12);assert t==0x4E4F534A;j=json.loads(b[20:20+n]);off=20+n;nb,tb=struct.unpack_from('<II',b,off);assert tb==0x004E4942;return j,b[off+8:off+8+nb],b

def accessor(j,b,i):
 a=j['accessors'][i];v=j['bufferViews'][a['bufferView']];n=SIZES[a['type']];f,w=FMT[a['componentType']];stride=v.get('byteStride',n*w);off=v.get('byteOffset',0)+a.get('byteOffset',0)
 return [struct.unpack_from('<'+f*n,b,off+k*stride)for k in range(a['count'])]

reports=[]
for path in sorted(glob.glob(BASE+'/models/*.glb')):
 j,b,whole=load(path);name=os.path.basename(path)[:-4];meta=json.load(open(BASE+'/metadata/pass3/'+name+'.json'));errs=[];pos=[];tris=0;uv=[];edges=collections.Counter();volume=0.;minarea=1.
 for me in j['meshes']:
  for p in me['primitives']:
   assert p.get('mode',4)==4,'triangles only';assert'TEXCOORD_0'in p['attributes'];pp=accessor(j,b,p['attributes']['POSITION']);uu=accessor(j,b,p['attributes']['TEXCOORD_0']);ii=[v[0]for v in accessor(j,b,p['indices'])];assert len(ii)%3==0
   pos+=pp;uv+=uu;tris+=len(ii)//3
   for k in range(0,len(ii),3):
    abc=[pp[ii[k+t]]for t in range(3)];keys=[tuple(round(v,6)for v in q)for q in abc]
    u=[abc[1][i]-abc[0][i]for i in range(3)];v=[abc[2][i]-abc[0][i]for i in range(3)];cr=(u[1]*v[2]-u[2]*v[1],u[2]*v[0]-u[0]*v[2],u[0]*v[1]-u[1]*v[0]);ar=sum(x*x for x in cr)**.5/2;minarea=min(minarea,ar)
    for q,r in zip(keys,keys[1:]+keys[:1]):edges[tuple(sorted((q,r)))]+=1
 for p in pos:
  if not all(math.isfinite(v)for v in p):errs.append('nonfinite position')
 if not all(-.00001<=v<=1.00001 for q in uv for v in q):errs.append('UV outside0..1')
 lo=[min(v[i]for v in pos)for i in range(3)];hi=[max(v[i]for v in pos)for i in range(3)];size=[round(hi[i]-lo[i],6)for i in range(3)]
 if abs(lo[1])>.00001:errs.append('baseY!=0')
 if abs(lo[0]+hi[0])>.0001 or abs(lo[2]+hi[2])>.0001:errs.append('XZ notcentred')
 if any(abs(v-w)>.0001 for v,w in zip(size,meta['size'])):errs.append('metadata size mismatch')
 images=[]
 for im in j.get('images',[]):
  if'uri'in im or im.get('mimeType')!='image/png':errs.append('image notembedded PNG');continue
  vv=j['bufferViews'][im['bufferView']];raw=b[vv.get('byteOffset',0):vv.get('byteOffset',0)+vv['byteLength']];pil=Image.open(io.BytesIO(raw));pil.load();images.append({'size':list(pil.size),'bytes':len(raw),'decoded_rgba_bytes':pil.width*pil.height*4,'sha256':hashlib.sha256(raw).hexdigest()})
  if max(pil.size)>1024:errs.append('oversize image')
  native=Image.open(BASE+'/textures/liminal_utility_256.png').convert('RGBA')
  if pil.convert('RGBA').tobytes()!=native.tobytes():errs.append('embedded image differs from standalone')
 if len(j.get('materials',[]))>16:errs.append('>16mats')
 prims=sum(len(m['primitives'])for m in j['meshes'])
 if prims>32:errs.append('>32prims')
 if tris>6000:errs.append('>6000triangles')
 if minarea<1e-12:errs.append('degenerate triangle')
 report={**meta,'file':'models/'+name+'.glb','sha256':hashlib.sha256(whole).hexdigest(),'byte_length':len(whole),'triangles_measured':tris,'primitives':prims,'materials_count':len(j.get('materials',[])),'images':images,'bounds_min':lo,'bounds_max':hi,'welded_edge_incidence':dict(collections.Counter(edges.values())),'errors':list(dict.fromkeys(errs))};reports.append(report)
out={'assets':reports,'new_asset_count':len(reports),'new_texture_decoded_rgba_bytes':sum(i['decoded_rgba_bytes']for r in reports for i in r['images']),'passed':all(not r['errors']for r in reports)};json.dump(out,open(BASE+'/qa/glb_validation.json','w'),indent=2)
print(json.dumps({'count':len(reports),'passed':out['passed'],'decoded_bytes':out['new_texture_decoded_rgba_bytes'],'maxtris':max((r['triangles_measured']for r in reports),default=0),'errors':[(r['name'],r['errors'])for r in reports if r['errors']]},indent=2))
