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

fn flashlight_visibility(world_position: vec3<f32>, normal: vec3<f32>, direction: vec3<f32>) -> f32 {
    let light = dynamic_lights.flashlight;
    // A small geometric normal bias removes self acne without changing the
    // surrounding darkness. The comparison footprint is bounded in texels.
    let receiver = world_position + normal * (0.002 + 0.004 * (1.0 - max(dot(normal, direction), 0.0)));
    let clip = light.view_projection * vec4<f32>(receiver, 1.0);
    if (clip.w <= 0.0) { return 0.0; }
    let projected = clip.xyz / clip.w;
    let uv = projected.xy * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5);
    if (any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)) || projected.z < 0.0 || projected.z > 1.0) { return 0.0; }
    let texel = 1.0 / vec2<f32>(textureDimensions(flashlight_depth));
    var visible = 0.0;
    for (var y = 0; y < 2; y = y + 1) {
        for (var x = 0; x < 2; x = x + 1) {
            let offset = (vec2<f32>(f32(x), f32(y)) - vec2<f32>(0.5)) * texel;
            visible += textureSampleCompareLevel(flashlight_depth, flashlight_sampler, uv + offset, projected.z);
        }
    }
    return visible * 0.25;
}


// The attached lights' diffuse term at one fragment. Each light contributes a
// cosine lobe windowed by a quadratic range falloff:
//
//     d = length(light.position - world_position)
//     ndl = max(dot(normal, normalize(light.position - world_position)), 0)
//     window = clamp(1 - d / light.radius, 0, 1)
//     sum += color * intensity * ndl * window * window
//
// The term is exactly zero when no light is attached, so the baked path (and
// the shader's unit-light bypass) is unchanged at zero lights. Zero distance
// is guarded before the normalise; a zero-radius light is refused by the
// neutral set, so `light.radius` is always positive here.
fn dynamic_light_term(normal: vec3<f32>, world_position: vec3<f32>) -> vec3<f32> {
    var sum = vec3<f32>(0.0);
    let count = min(dynamic_lights.count, 8u);
    for (var slot = 0u; slot < count; slot = slot + 1u) {
        let light = dynamic_lights.lights[slot];
        let to_light = light.position - world_position;
        let distance = length(to_light);
        if (distance >= light.radius) {
            continue;
        }
        let direction = to_light / max(distance, 1.0e-6);
        let ndl = max(dot(normal, direction), 0.0);
        let window = clamp(1.0 - distance / light.radius, 0.0, 1.0);
        sum += light.color * light.intensity * ndl * window * window;
    }
    let beam = dynamic_lights.flashlight;
    if (beam.outer_enabled.y > 0.5) {
        let to_light = beam.position_range.xyz - world_position;
        let distance = length(to_light);
        if (distance > 1.0e-6 && distance < beam.position_range.w) {
            let direction = to_light / distance;
            let angular = dot(beam.direction_inner.xyz, -direction);
            let cone = smoothstep(beam.outer_enabled.x, beam.direction_inner.w, angular);
            if (cone > 0.0) {
                let relative = distance / beam.position_range.w;
                let window = max(1.0 - relative * relative * relative * relative, 0.0);
                let falloff = window * window / (1.0 + distance * distance);
                let ndl = max(dot(normal, direction), 0.0);
                let visibility = flashlight_visibility(world_position, normal, direction);
                sum += beam.color_strength.rgb * beam.color_strength.w * cone * falloff * ndl * visibility;
            }
        }
    }
    return sum;
}
