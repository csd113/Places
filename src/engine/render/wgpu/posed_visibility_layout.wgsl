
// The same immutable opaque world in all quality modes and material passes.
struct VisibilityNode {
    minimum: vec3<f32>, first: u32,
    maximum: vec3<f32>, count: u32,
    escape: u32, ready: u32, _pad0: u32, _pad1: u32,
};
struct VisibilityTriangle { a_solid: vec4<f32>, edge0: vec4<f32>, edge1: vec4<f32> };
