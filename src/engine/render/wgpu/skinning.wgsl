// Scalar output preserves the WorldVertex wire layout, including its
// packed lightmap field. Only position/normal change; every other byte survives.
const WORLD_VERTEX_WORDS: u32 = __WORLD_VERTEX_WORDS__u;

struct SkinVertex {
    position: vec4<f32>, normal: vec4<f32>, joints: vec4<u32>, weights: vec4<f32>,
}
@group(0) @binding(0) var<storage, read> bind_vertices: array<SkinVertex>;
@group(0) @binding(1) var<storage, read> joints: array<mat4x4<f32>>;
@group(0) @binding(2) var<storage, read_write> posed: array<f32>;

fn safe_normal(v: vec3<f32>) -> vec3<f32> {
    let scale = max(max(abs(v.x), abs(v.y)), abs(v.z));
    if scale <= 0.0 { return vec3<f32>(0.0); }
    return normalize(v / scale);
}

// Inverse transpose of the blended linear matrix, including determinant sign.
// Rescaling keeps tiny imported units and nonuniform scale numerically useful.
fn skin_normal(m: mat3x3<f32>, n: vec3<f32>) -> vec3<f32> {
    let a = max(abs(m[0]), max(abs(m[1]), abs(m[2])));
    let scale = max(max(a.x, a.y), a.z);
    if scale <= 0.0 { return vec3<f32>(0.0); }
    let x = m[0] / scale; let y = m[1] / scale; let z = m[2] / scale;
    let cx = cross(y, z); let cy = cross(z, x); let cz = cross(x, y);
    let det = dot(x, cx);
    if det == 0.0 { return vec3<f32>(0.0); }
    return safe_normal((mat3x3<f32>(cx, cy, cz) * n) * sign(det));
}

@compute @workgroup_size(64)
fn deform(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let index = invocation.x;
    if index >= arrayLength(&bind_vertices) { return; }
    let v = bind_vertices[index];
    var p = vec3<f32>(0.0); var m = mat3x3<f32>(); var total = 0.0;
    for (var slot = 0u; slot < 4u; slot += 1u) {
        let weight = v.weights[slot]; let joint = v.joints[slot];
        if weight > 0.0 && joint < arrayLength(&joints) {
            let delta = joints[joint];
            p += (delta * v.position).xyz * weight;
            m += mat3x3<f32>(delta[0].xyz, delta[1].xyz, delta[2].xyz) * weight;
            total += weight;
        }
    }
    var position = v.position.xyz; var normal = v.normal.xyz;
    if total > 0.0 { position = p / total; normal = skin_normal(m, normal); }
    let offset = index * WORLD_VERTEX_WORDS;
    posed[offset] = position.x; posed[offset + 1u] = position.y; posed[offset + 2u] = position.z;
    posed[offset + 3u] = normal.x; posed[offset + 4u] = normal.y; posed[offset + 5u] = normal.z;
}
