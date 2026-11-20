// Group 4 is the frame's attached lights: a small, fixed-capacity array of
// character glows the renderer resolves from the entity frames each frame.
// It is the one realtime light term in this shader, and it is deliberately
// bounded and additive on top of the baked `surface_light`; the spotlight
// additionally samples the current meshes' visibility target. The
// emissive pass never reads it, so bloom cannot double the pool.
struct DynamicLight {
    // World-space position, in metres.
    position: vec3<f32>,
    // The falloff window's radius, in metres, strictly positive.
    radius: f32,
    // Linear light colour, each channel `0..=1`.
    color: vec3<f32>,
    // Additive intensity.
    intensity: f32,
    enclosure_origin_gap: vec4<f32>,
    enclosure_right_enabled: vec4<f32>,
    enclosure_bounds: vec4<f32>,
    enclosure_portal: vec4<f32>,
    enclosure_doors: vec4<f32>,
};

struct Flashlight {
    view_projection: mat4x4<f32>,
    position_range: vec4<f32>,
    color_strength: vec4<f32>,
    direction_inner: vec4<f32>,
    outer_enabled: vec4<f32>,
};

struct DynamicLights {
    // Number of valid entries in `lights`, 0 when nothing is attached.
    count: u32,
    // Explicit scalar padding so the array starts at offset 16: three separate
    // `u32`s, because a `vec3<u32>` would itself align to 16.
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
    lights: array<DynamicLight, 8>,
    flashlight: Flashlight,
};

@group(4) @binding(0)
var<uniform> dynamic_lights: DynamicLights;
@group(4) @binding(1)
var flashlight_depth: texture_depth_2d;
@group(4) @binding(2)
var flashlight_sampler: sampler_comparison;

@group(4) @binding(3) var<storage, read> visibility_nodes: array<VisibilityNode>;
@group(4) @binding(4) var<storage, read> visibility_triangles: array<VisibilityTriangle>;
@group(4) @binding(5) var<storage, read> posed_visibility_nodes: array<VisibilityNode>;
@group(4) @binding(6) var<storage, read> posed_visibility_triangles: array<VisibilityTriangle>;
