
// Only the canonical entry point reads these independent instance resources.
struct PropInstance {
    model: mat4x4<f32>, normal: mat4x4<f32>, tint: vec4<f32>, lighting: vec4<u32>,
};
@group(5) @binding(0) var<storage, read> prop_instances: array<PropInstance>;
@group(5) @binding(1) var<storage, read> prop_colors: array<vec4<f32>>;
@group(5) @binding(2) var<storage, read> prop_uv_visibility: array<vec4<f32>>;

override prop_kind: u32 = 0u;

@vertex
fn vs_prop_instance(vertex: WorldVertex, @builtin(vertex_index) vertex_index: u32,
    @builtin(instance_index) instance_index: u32) -> VsOut {
    let instance = prop_instances[instance_index];
    let world = instance.model * vec4<f32>(vertex.position, 1.0);
    if (prop_kind == 2u || (prop_kind == 0u && instance.lighting.w != 0u)) {
        var rigid = vertex;
        rigid.position = world.xyz;
        rigid.normal = (instance.normal * vec4<f32>(vertex.normal, 0.0)).xyz;
        rigid.tangent = (instance.model * vec4<f32>(vertex.tangent, 0.0)).xyz;
        let determinant = dot(cross(instance.model[0].xyz, instance.model[1].xyz), instance.model[2].xyz);
        rigid.handedness *= select(1.0, -1.0, determinant < 0.0);
        var output = world_vertex(rigid);
        output.local_position = vertex.position;
        return output;
    }
    var placed = vertex;
    placed.position = world.xyz;
    let normal_matrix = mat3x3<f32>(instance.normal[0].xyz, instance.normal[1].xyz, instance.normal[2].xyz);
    let normal = normal_matrix * vertex.normal;
    placed.normal = select(vec3<f32>(0.0), normalize(normal), dot(normal, normal) > 0.0);
    // This admitted historical indexed route has a world-axis tangent. Split
    // triangle tangent frames are deliberately kept on their established path.
    placed.tangent = vec3<f32>(1.0, 0.0, 0.0);
    let address = instance.lighting.x + vertex_index;
    placed.color = prop_colors[address];
    placed.caustics_albedo = vertex.caustics_albedo * instance.tint.xyz;
    if (instance.lighting.z != 0u) { placed.uv_visibility = prop_uv_visibility[address]; }
    return world_vertex(placed);
}
