struct PosedDepthTriangle { a_solid:vec4<f32>, edge0:vec4<f32>, edge1:vec4<f32> };
struct PosedDepthDraw { projection:mat4x4<f32>, source:vec4<f32>, range:vec4<u32> };
@group(0) @binding(0) var<storage,read> triangles:array<PosedDepthTriangle>;
@group(0) @binding(1) var<uniform> draw:PosedDepthDraw;
@vertex
fn main(@builtin(vertex_index) vertex:u32)->@builtin(position) vec4<f32>{
    let triangle=triangles[draw.range.x+vertex/3u];
    var p=triangle.a_solid.xyz;
    if(vertex%3u==1u){p+=triangle.edge0.xyz;}
    if(vertex%3u==2u){p+=triangle.edge1.xyz;}
    return draw.projection*vec4<f32>(p-draw.source.xyz,1.0);
}
