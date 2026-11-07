struct AmbientVertex { position_root: vec4<f32>, normal_scale: vec4<f32>, rotation: vec4<f32>, reach: vec4<f32> };
@group(0) @binding(0) var<storage,read> posed_visibility_nodes:array<VisibilityNode>;
@group(0) @binding(1) var<storage,read> posed_visibility_triangles:array<VisibilityTriangle>;
@group(0) @binding(2) var<storage,read> ambient_vertices:array<AmbientVertex>;
@group(0) @binding(3) var<storage,read_write> ambient_results:array<vec4<f32>>;
@group(0) @binding(4) var<uniform> ambient_count:vec4<u32>;

var<workgroup> visible_front:array<f32,64>;
var<workgroup> visible_back:array<f32,64>;
var<workgroup> total_front:array<f32,64>;
var<workgroup> total_back:array<f32,64>;
// One vertex per group, one deterministic sphere direction per lane. Every
// current-pose side is prepared, including reflection and shared-vertex uses.
@compute @workgroup_size(64)
fn main(@builtin(workgroup_id) group:vec3<u32>,@builtin(local_invocation_index) lane:u32){
    let index=group.x;
    if(index>=ambient_count.x){return;}
    let vertex=ambient_vertices[index];
    let normal=vertex.normal_scale.xyz;
    let direction=posed_rotate(vertex.rotation,POSED_DIRECTIONS[lane]);
    let cosine=dot(normal,direction);
    let front=max(cosine,0.0);let back=max(-cosine,0.0);
    var visible=false;
    if(abs(cosine)>0.0){
        visible=posed_character_visible(vertex.position_root.xyz,select(normal,-normal,cosine<0.0),vertex.position_root.xyz+direction*vertex.reach.x,vertex.position_root.w,vertex.normal_scale.w);
    }
    visible_front[lane]=select(0.0,front,visible);visible_back[lane]=select(0.0,back,visible);
    total_front[lane]=front;total_back[lane]=back;
    workgroupBarrier();
    for(var stride=32u;stride>0u;stride/=2u){
        if(lane<stride){
            visible_front[lane]+=visible_front[lane+stride];visible_back[lane]+=visible_back[lane+stride];
            total_front[lane]+=total_front[lane+stride];total_back[lane]+=total_back[lane+stride];
        }
        workgroupBarrier();
    }
    if(lane==0u){
        let valid=dot(normal,normal)>1.0e-12;
        ambient_results[index]=vec4<f32>(visible_front[0]/max(total_front[0],1.0e-6),visible_back[0]/max(total_back[0],1.0e-6),select(0.0,1.0,valid),select(0.0,1.0,valid));
    }
}
