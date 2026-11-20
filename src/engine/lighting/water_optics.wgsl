// Surface optics for the world shader. Authored motion and the current ripple
// buffer are supplied by the renderer's water and caustics bindings.

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

// Analytic wave slopes perturb the surface frame; the normal texture remains
// the authored PNG sampled by the material shader.
fn water_wave_slope(position: vec2<f32>, motion: vec4<f32>, seconds: f32) -> vec2<f32> {
    let point = position - motion.zw * seconds;
    let phase = seconds * motion.x;
    let a = dot(point, vec2<f32>(2.8, 1.1)) - phase * 1.7;
    let b = dot(point, vec2<f32>(-1.3, 3.2)) - phase * 1.1;
    return (vec2<f32>(2.8, 1.1) * cos(a) + vec2<f32>(-1.3, 3.2) * cos(b) * 0.45) * motion.y * 0.07;
}

struct WaterBody {
    radiance: vec3<f32>,
    transmission: vec3<f32>,
};

fn water_body(background: vec3<f32>, scatter: vec3<f32>, thickness: f32, opacity: f32, absorption: f32) -> WaterBody {
    // Existing opacity controls colour density; authored scalar absorption
    // remains additive. RGB extinction preferentially removes warm light.
    let extinction = (vec3<f32>(0.32, 0.095, 0.055) * (0.35 + opacity) + vec3<f32>(absorption)) * thickness;
    let transmission = exp(-extinction);
    let radiance = background * transmission + max(scatter, vec3<f32>(0.0)) * (vec3<f32>(1.0) - transmission) * 0.82;
    return WaterBody(radiance, transmission);
}
