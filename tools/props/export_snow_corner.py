"""Compose the fixed Blender timber export with the accepted rail interfaces.

Export the preserved .blend's selected mesh objects to a scratch GLB using
Blender's glTF exporter (Y up, normals/UV/materials/extras on, animations off).
Then run this file with that GLB, the installed rail as --template, and a new
scratch --out. Inspect/validate the result before adopting it. This offline
asset step preserves the template's snow, metal, PNG, slots, nodes and origin.
It always expands timber RGB vertex colours to RGBA with alpha 1 for Places.
It refuses changed outer bounds, unsupported colour formats, or missing nodes.
"""
import argparse
import copy
import hashlib
import json
from pathlib import Path
import struct


def read(path):
    data=path.read_bytes()
    magic,version,size=struct.unpack_from('<III',data)
    if (magic,version,size)!=(0x46546c67,2,len(data)):raise ValueError('Expected a complete GLB 2.0')
    length,kind=struct.unpack_from('<II',data,12)
    if kind!=0x4e4f534a:raise ValueError('Missing GLB JSON')
    return json.loads(data[20:20+length]),data[28+length:]


def compose(export,template,output):
    if output.exists():raise ValueError('Scratch output must be new')
    a,ab=read(template);b,bb=read(export);d=copy.deepcopy(a)
    nodes=[node for node in b['nodes'] if node.get('name')=='canonical_base' and 'mesh' in node]
    if len(nodes)!=1:raise ValueError('Expected exactly one canonical_base mesh node')
    candidates=[p for p in b['meshes'][nodes[0]['mesh']]['primitives'] if b['materials'][p['material']].get('name')=='railing_timber']
    if len(candidates)!=1:raise ValueError('Expected exactly one railing_timber primitive')
    timber=candidates[0]
    if set(timber['attributes'])!={'POSITION','NORMAL','TEXCOORD_0','COLOR_0'}:raise ValueError('Timber requires positions, flat normals, fitted UVs and colours')
    buffers=bytearray();views=[];accessors=[];cache={}
    def raw_accessor(doc,binary,index):
        ac=doc['accessors'][index];view=doc['bufferViews'][ac['bufferView']]
        count={'SCALAR':1,'VEC2':2,'VEC3':3,'VEC4':4}[ac['type']]
        size={5121:1,5123:2,5125:4,5126:4}[ac['componentType']]*count
        offset=view.get('byteOffset',0)+ac.get('byteOffset',0)
        return b''.join(binary[offset+i*view.get('byteStride',size):offset+i*view.get('byteStride',size)+size] for i in range(ac['count']))
    old=raw_accessor(a,ab,a['meshes'][0]['primitives'][0]['attributes']['POSITION'])
    new=raw_accessor(b,bb,timber['attributes']['POSITION'])
    def extrema(data):
        points=list(struct.iter_unpack('<fff',data))
        return [min(p[i] for p in points) for i in range(3)]+[max(p[i] for p in points) for i in range(3)]
    if any(abs(x-y)>1e-6 for x,y in zip(extrema(old),extrema(new))):raise ValueError('Timber bounds changed; review the catalog/interface before export')
    def view(src,index,rgba=False):
        key=(src,index,rgba)
        if key in cache:return cache[key]
        doc,binary=(a,ab) if src=='template' else (b,bb)
        v=copy.deepcopy(doc['bufferViews'][index]);offset=v.get('byteOffset',0);data=binary[offset:offset+v['byteLength']]
        if rgba:
            if v.get('byteStride',12)!=12:raise ValueError('Export non-interleaved RGB colours')
            data=b''.join(data[i:i+12]+struct.pack('<f',1) for i in range(0,len(data),12))
            v['byteLength']=len(data)
            if 'byteStride' in v:v['byteStride']=16
        buffers.extend(b'\0'*(-len(buffers)%4));v['byteOffset']=len(buffers);v['buffer']=0;buffers.extend(data)
        index=len(views);views.append(v);cache[key]=index;return index
    def acc(src,index,color=False):
        doc,binary=(a,ab) if src=='template' else (b,bb)
        value=copy.deepcopy(doc['accessors'][index]);rgba=color and value['type']=='VEC3'
        if color:
            if value['componentType']!=5126 or value['type'] not in ('VEC3','VEC4') or value.get('byteOffset',0):raise ValueError('Export tightly packed float RGB/RGBA colours')
            source_view=doc['bufferViews'][value['bufferView']]
            channels=3 if rgba else 4
            if source_view['byteLength']!=value['count']*channels*4:raise ValueError('Export a separate tightly packed timber colour view')
            if value['type']=='VEC4' and any(item[3]!=1 for item in struct.iter_unpack('<ffff',raw_accessor(doc,binary,index))):raise ValueError('Timber vertex alpha must stay 1')
        value['bufferView']=view(src,value['bufferView'],rgba)
        if rgba:
            value['type']='VEC4'
            for key in ('min','max'):
                if key in value:value[key]=[*value[key],1.0]
        result=len(accessors);accessors.append(value);return result
    for mi,mesh in enumerate(d['meshes']):
        for pi,primitive in enumerate(mesh['primitives']):
            src='export' if (mi,pi)==(0,0) else 'template'
            source=timber if src=='export' else a['meshes'][mi]['primitives'][pi]
            primitive['attributes']={key:acc(src,index,color=(src=='export' and key=='COLOR_0')) for key,index in source['attributes'].items()}
            primitive['indices']=acc(src,source['indices'])
    for image in d['images']:image['bufferView']=view('template',image['bufferView'])
    position=accessors[d['meshes'][0]['primitives'][0]['attributes']['POSITION']]
    original=a['accessors'][a['meshes'][0]['primitives'][0]['attributes']['POSITION']]
    position['min']=original['min'];position['max']=original['max']
    d['accessors']=accessors;d['bufferViews']=views;d['buffers']=[{'byteLength':len(buffers)}]
    js=json.dumps(d,separators=(',',':')).encode();js+=b' '*(-len(js)%4);buffers.extend(b'\0'*(-len(buffers)%4))
    data=struct.pack('<III',0x46546c67,2,28+len(js)+len(buffers))+struct.pack('<II',len(js),0x4e4f534a)+js+struct.pack('<II',len(buffers),0x004e4942)+buffers
    output.parent.mkdir(parents=True,exist_ok=True);output.write_bytes(data)
    return {'sha256':hashlib.sha256(data).hexdigest(),'bytes':len(data),'rgba_alpha':1}


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--blender-export',type=Path,required=True);p.add_argument('--template',type=Path,required=True);p.add_argument('--out',type=Path,required=True);args=p.parse_args()
    print(json.dumps(compose(args.blender_export,args.template,args.out),indent=2))
