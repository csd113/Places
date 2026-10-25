override water_blend_pass: bool = false;

// Water presentation inputs are private to this pipeline's camera binding.
// Reflections/Low retain disabled snapshots and the ordinary blend fallback.
struct WaterRipple { origin_bounds: vec4<f32>, profile: vec4<f32> };
struct WaterPresentation {
    inverse_view_projection: mat4x4<f32>,
    controls: vec4<f32>,
    ripples: array<WaterRipple, 8>,
};
@group(0) @binding(1) var<uniform> water_presentation: WaterPresentation;
@group(0) @binding(2) var water_opaque_color: texture_2d<f32>;
@group(0) @binding(3) var water_opaque_depth: texture_depth_2d;

fn water_fresnel(normal: vec3<f32>, view: vec3<f32>) -> f32 {
    let rim = 1.0 - clamp(abs(dot(normal, view)), 0.0, 1.0);
    let squared = rim * rim;
    return 0.02037 + 0.97963 * squared * squared * rim;
}

fn water_local_slope(position: vec3<f32>, priority: u32) -> vec2<f32> {
    var slope = vec2<f32>(0.0);
    if (!water_blend_pass) { return slope; }
    for (var index = 0u; index < min(u32(water_presentation.controls.z), 8u); index += 1u) {
        let ripple = water_presentation.ripples[index];
        if (ripple.profile.w != f32(priority) || ripple.profile.x <= 0.0) { continue; }
        // CPU-prepared squared annulus bounds reject distant/interior pixels
        // before sqrt, trigonometry or exponentials. Priority uniquely selects
        // the authored water plane; it cannot spill onto another volume.
        let delta = position.xz - ripple.origin_bounds.xy;
        let squared = dot(delta, delta);
        if (squared < ripple.origin_bounds.z || squared > ripple.origin_bounds.w) { continue; }
        let radius = sqrt(squared);
        if (radius < 0.01) { continue; }
        let distance = radius - ripple.profile.y;
        let band = distance * ripple.profile.z;
        if (abs(band) > 3.0) { continue; }
        let envelope = exp(-band * band) * ripple.profile.x;
        slope += delta / radius * cos(distance * 19.0) * envelope;
    }
    return clamp(slope, vec2<f32>(-0.16), vec2<f32>(0.16));
}

fn water_unproject(pixel: vec2<i32>, depth: f32) -> vec3<f32> {
    let size = vec2<f32>(textureDimensions(water_opaque_depth));
    let uv = (vec2<f32>(pixel) + 0.5) / size;
    let point = water_presentation.inverse_view_projection * vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, depth, 1.0);
    return point.xyz / point.w;
}

// Reject the complete bilinear footprint when a foreground silhouette or a
// view-frame edge intrudes. The fallback is the original pixel, never clamped
// foreground imagery dragged across water. Opaque depth stays immutable.
fn water_safe_sample(pixel: vec2<i32>, offset: vec2<f32>, fragment_depth: f32, surface_y: f32, above: bool) -> vec3<f32> {
    let size = vec2<i32>(textureDimensions(water_opaque_depth));
    let coordinate = vec2<f32>(pixel) + offset;
    let low = vec2<i32>(floor(coordinate));
    if (any(low < vec2<i32>(1)) || any(low + vec2<i32>(2) >= size)) { return textureLoad(water_opaque_color, pixel, 0).rgb; }
    for (var corner = 0u; corner < 4u; corner += 1u) {
        let step = vec2<i32>(i32(corner & 1u), i32(corner >> 1u));
        let candidate = low + step;
        let depth = textureLoad(water_opaque_depth, candidate, 0);
        if (depth <= fragment_depth + 0.00002) { return textureLoad(water_opaque_color, pixel, 0).rgb; }
        if (above && depth < 0.99999 && water_unproject(candidate, depth).y > surface_y + 0.005) { return textureLoad(water_opaque_color, pixel, 0).rgb; }
    }
    // Reuse the renderer's immutable clamped-linear sampler. Guard every depth
    // texel first, then let fixed-function filtering do the same bilinear read.
    return textureSampleLevel(water_opaque_color, reflection_sampler,
        (coordinate + 0.5) / vec2<f32>(size), 0.0).rgb;
}

fn water_composite(in: VsOut, normal: vec3<f32>, view: vec3<f32>, scatter: vec3<f32>, reflection: vec3<f32>, alpha: f32, facing: f32) -> vec4<f32> {
    let selected = water_surface_node(in.world_position);
    if (selected == 0u || alpha <= 0.0) { return vec4<f32>(scatter + reflection, alpha); }
    let node = caustics.nodes[selected - 1u];
    let size = vec2<i32>(textureDimensions(water_opaque_depth));
    let pixel = clamp(vec2<i32>(in.clip.xy), vec2<i32>(0), size - vec2<i32>(1));
    let depth = textureLoad(water_opaque_depth, pixel, 0);
    let above = camera.position.y >= node.min.w;
    var thickness = min(distance(in.world_position, camera.position), 12.0);
    if (above) {
        thickness = min((node.min.w - node.min.y) / max(abs(view.y), 0.12), 12.0);
        if (depth < 0.99999 && depth > in.clip.z) {
            let hit = water_unproject(pixel, depth);
            thickness = min(distance(hit, in.world_position), thickness);
        }
    }
    let contact = smoothstep(0.0, 0.055, thickness);
    let reflection_mix = clamp(material.reflection_strength * 6.0, vec3<f32>(0.0), vec3<f32>(0.95)) * facing * contact;
    // Project the horizontal perturbation into this camera's screen frame;
    // turning the view must not rotate refraction in world-X/Z screen axes.
    let right = normalize(vec3<f32>(camera.view_projection[0].x, camera.view_projection[1].x, camera.view_projection[2].x));
    let up = normalize(vec3<f32>(camera.view_projection[0].y, camera.view_projection[1].y, camera.view_projection[2].y));
    let slope = vec3<f32>(normal.x, 0.0, normal.z);
    let displacement = vec2<f32>(dot(slope, right), -dot(slope, up)) * (6.0 + 21.0 * min(thickness, 1.0));
    let offset = clamp(displacement, vec2<f32>(-3.0), vec2<f32>(3.0)) * contact;
    let background = water_safe_sample(pixel, offset, in.clip.z, node.min.w, above);
    // Existing opacity controls colour density; authored scalar absorption
    // remains additive. RGB extinction preferentially removes warm light.
    let extinction = (vec3<f32>(0.32, 0.095, 0.055) * (0.35 + node.params.w) + vec3<f32>(node.params.z)) * thickness;
    let transmission = exp(-extinction);
    let meniscus = 1.0 - 0.035 * (1.0 - smoothstep(0.025, 0.12, thickness));
    let body = background * transmission + max(scatter, vec3<f32>(0.0)) * (vec3<f32>(1.0) - transmission) * 0.82;
    let original = textureLoad(water_opaque_color, pixel, 0).rgb;
    let composite = mix(original, body * (vec3<f32>(1.0) - reflection_mix) * meniscus + reflection * contact, contact);
    // Compensated straight-alpha keeps already sorted transparent subjects
    // behind water contributing through its remaining transmission. With an
    // opaque backdrop the blend exactly reconstructs the refracted composite.
    let coverage = clamp(1.0 - min(min(transmission.r, transmission.g), transmission.b) * (1.0 - facing), 0.05, 1.0) * contact;
    if (coverage < 0.0001) { return vec4<f32>(0.0); }
    return vec4<f32>((composite - original * (1.0 - coverage)) / coverage, coverage);
}
