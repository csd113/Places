// Linear radiance policy shared by world sampling and the water composite.
fn reflection_surface_enabled(mode: u32, strength: vec3<f32>) -> bool {
    return mode != 0u && any(strength != vec3<f32>(0.0));
}

fn reflection_surface_weight(normal: vec3<f32>, view: vec3<f32>, roughness: f32,
                             water: bool, water_reflectance: f32) -> f32 {
    if (water) { return water_reflectance; }
    let facing = clamp(abs(dot(normal, view)), 0.0, 1.0);
    let fresnel = mix(0.08, 1.0, pow(1.0 - facing, 5.0));
    let gloss = clamp(1.0 - roughness, 0.0, 1.0);
    let polish = gloss * gloss;
    return mix(fresnel * 0.35, 1.0, polish);
}

fn planar_blur_radius(roughness: f32) -> f32 {
    return roughness * 0.035;
}

fn planar_uses_blur(radius: f32) -> bool {
    return radius > 0.002;
}

fn planar_blur_offsets(radius: f32) -> array<vec2<f32>, 4> {
    return array<vec2<f32>, 4>(vec2<f32>(radius, radius), vec2<f32>(-radius, radius),
                               vec2<f32>(radius, -radius), vec2<f32>(-radius, -radius));
}

fn planar_blur_average(sum: vec3<f32>) -> vec3<f32> {
    return sum * 0.25;
}

fn reflection_plane_weight(plane: vec4<f32>, position: vec3<f32>) -> f32 {
    let distance = abs(dot(plane.xyz, position) + plane.w);
    return 1.0 - smoothstep(0.0, 0.08, distance);
}

fn reflection_probe_mip(roughness: f32, max_mip: f32) -> f32 {
    return clamp(roughness * max_mip, 0.0, max_mip);
}

// A live capture has a single mip, unlike an offline packaged probe.
fn live_probe_uses_broad_tap(max_mip: f32, roughness: f32) -> bool {
    return max_mip < 0.5 && roughness > 0.15;
}

fn live_probe_broad_direction(reflected: vec3<f32>, normal: vec3<f32>) -> vec3<f32> {
    return normalize(mix(reflected, normal, vec3<f32>(0.5)));
}

fn live_probe_mix(sharp: vec3<f32>, broad: vec3<f32>, roughness: f32) -> vec3<f32> {
    return mix(sharp, broad, vec3<f32>(roughness));
}

fn water_reflection_strength(strength: vec3<f32>) -> vec3<f32> {
    return clamp(strength * 6.0, vec3<f32>(0.0), vec3<f32>(0.95));
}
