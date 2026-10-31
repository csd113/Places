// Reused resident/tiled prototype. Ambiguity flags are advisory; exact CPU
// certification, never this f32 heuristic, authorizes an accepted result.
struct Node { low:vec4<f32>,high:vec4<f32>,info:vec4<u32> }
struct Triangle { a:vec4<f32>,b:vec4<f32>,c:vec4<f32> }
struct Ray { origin:vec4<f32>,direction:vec4<f32> }
@group(0) @binding(0) var<storage,read> nodes:array<Node>;
@group(0) @binding(1) var<storage,read> triangles:array<Triangle>;
@group(0) @binding(2) var<storage,read> rays:array<Ray>;
@group(0) @binding(3) var<storage,read_write> results:array<vec2<u32>>;
fn box_hit(n:Node,o:vec3<f32>,d:vec3<f32>,limit:f32)->bool {
 var lo=0.0;var hi=limit;
 for(var axis=0u;axis<3u;axis++) {
  if(d[axis]==0.0) {if(o[axis]<n.low[axis] || o[axis]>n.high[axis]) {return false;}}
  else {
   let a=(n.low[axis]-o[axis])/d[axis];let b=(n.high[axis]-o[axis])/d[axis];
   // Conservative traversal padding only; never expand triangle edges.
   let pad=8.0*1.192092896e-7*max(1.0,max(abs(a),abs(b)));
   lo=max(lo,min(a,b)-pad);hi=min(hi,max(a,b)+pad);
   if(lo>hi) {return false;}
  }
 }
 return true;
}
fn triangle_hit(t:Triangle,o:vec3<f32>,d:vec3<f32>,limit:f32)->vec2<u32> {
 var z=2u;if(abs(d.x)>abs(d.y)){if(abs(d.x)>abs(d.z)){z=0u;}}else if(abs(d.y)>abs(d.z)){z=1u;}
 let x=(z+1u)%3u;let y=(x+1u)%3u;let depth=d[z];
 if(depth==0.0){return vec2<u32>(0u,0u);}
 let sx=d[x]/depth;let sy=d[y]/depth;
 let pa=t.a.xyz-o;let pb=t.b.xyz-o;let pc=t.c.xyz-o;
 let a=vec3<f32>(pa[x]-sx*pa[z],pa[y]-sy*pa[z],pa[z]);
 let b=vec3<f32>(pb[x]-sx*pb[z],pb[y]-sy*pb[z],pb[z]);
 let c=vec3<f32>(pc[x]-sx*pc[z],pc[y]-sy*pc[z],pc[z]);
 let ea=c.x*b.y-c.y*b.x;let eb=a.x*c.y-a.y*c.x;let ec=b.x*a.y-b.y*a.x;
 if((ea<0.0 || eb<0.0 || ec<0.0) && (ea>0.0 || eb>0.0 || ec>0.0)){return vec2<u32>(0u,0u);}
 let determinant=ea+eb+ec;if(determinant==0.0){return vec2<u32>(0u,0u);}
 let distance=(ea*(a.z/depth)+eb*(b.z/depth)+ec*(c.z/depth))/determinant;
 let edge_scale=max(abs(ea),max(abs(eb),abs(ec)));
 let near_edge=min(abs(ea),min(abs(eb),abs(ec)))<=32.0*1.192092896e-7*edge_scale;
 let endpoint_pad=32.0*1.192092896e-7*max(1.0,max(abs(distance),abs(limit)));
 let ambiguous=near_edge || abs(distance)<=endpoint_pad || abs(distance-limit)<=endpoint_pad;
 return vec2<u32>(select(0u,bitcast<u32>(t.a.w),distance>0.0 && distance<limit),select(0u,1u,ambiguous));
}
@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) id:vec3<u32>) {
 if(id.x>=bitcast<u32>(rays[0].direction.w)){return;}
 let r=rays[id.x];var flags=0u;var stack:array<u32,64>;var depth=1u;stack[0]=0u;
 while(depth>0u){depth--;let node=nodes[stack[depth]];
  if(!box_hit(node,r.origin.xyz,r.direction.xyz,r.origin.w)){continue;}
  if(node.info.y>0u){
   for(var i=node.info.x;i<node.info.x+node.info.y;i++){
    let hit=triangle_hit(triangles[i],r.origin.xyz,r.direction.xyz,r.origin.w); flags|=hit.y;
    if(hit.x!=0u){results[id.x]=vec2<u32>(hit.x,flags);return;}
   }
  }else{if(depth+1u>=64u){results[id.x]=vec2<u32>(0u,2u);return;}stack[depth]=node.info.x;stack[depth+1u]=node.info.z;depth+=2u;}
 }
 results[id.x]=vec2<u32>(0u,flags);
}
