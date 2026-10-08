
// Compiled only with visual-diagnostics. Values come from current bindings;
// magenta identifies an unavailable coefficient, never invented illumination.
fn visual_unavailable() -> vec3<f32> { return vec3<f32>(1.0, 0.0, 1.0); }
fn visual_hdr(value: vec3<f32>) -> vec3<f32> {
    let nonnegative = max(value, vec3<f32>(0.0));
    return nonnegative / (vec3<f32>(1.0) + nonnegative);
}
fn visual_atlas_available(in: VsOut) -> bool {
    return abs(environment.entity_irradiance.w) <= 0.5
        && environment.lightmap_enabled > 0.5 && in.lightmap_page < 254.5;
}
fn visual_atlas_mean(in: VsOut) -> vec3<f32> {
    let page = u32(in.lightmap_page + 0.5);
    let pages = environment.lightmap_page_count;
    var mean = textureSampleLevel(lightmap_pages, lightmap_sampler, in.lightmap_uv, page * 2u, 0.0).rgb;
    let count = environment.lightmap_switchable & 0xFu;
    let mask = (environment.lightmap_switchable >> 8u) & 0xFu;
    for (var group = 0u; group < count; group += 1u) {
        if ((mask & (1u << group)) != 0u) {
            let layer = pages * 2u * (group + 1u) + page * 2u;
            mean += textureSampleLevel(lightmap_pages, lightmap_sampler, in.lightmap_uv, layer, 0.0).rgb;
        }
    }
    return mean;
}
fn visual_reconstruct(energy: vec3<f32>, moment: vec3<f32>, normal: vec3<f32>) -> vec3<f32> {
    let k = energy.r + energy.g + energy.b;
    let lobe = 2.0 * max(0.0, dot(moment, normal)) - length(moment);
    let directional = energy / max(k, 1.0e-6) * lobe;
    return max(vec3<f32>(0.0), energy + select(vec3<f32>(0.0), directional, k > 1.0e-6));
}
fn visual_atlas_layer(in: VsOut, layer: u32, normal: vec3<f32>) -> vec3<f32> {
    let energy = textureSampleLevel(lightmap_pages, lightmap_sampler, in.lightmap_uv, layer, 0.0).rgb;
    let moment = textureSampleLevel(lightmap_pages, lightmap_sampler, in.lightmap_uv, layer + 1u, 0.0).rgb;
    return visual_reconstruct(energy, moment, normal);
}
fn visual_baked(in: VsOut, normal: vec3<f32>) -> vec3<f32> {
    if (environment.entity_irradiance.w > 0.5) {
        return visual_hdr(visual_reconstruct(environment.entity_irradiance.rgb, environment.entity_moment.xyz, normal));
    }
    if (environment.entity_irradiance.w < -0.5) {
        return visual_hdr(environment.light_scale);
    }
    if (!visual_atlas_available(in)) { return visual_unavailable(); }
    let page = u32(in.lightmap_page + 0.5);
    let pages = environment.lightmap_page_count;
    var hdr = visual_atlas_layer(in, page * 2u, normal);
    let count = environment.lightmap_switchable & 0xFu;
    let mask = (environment.lightmap_switchable >> 8u) & 0xFu;
    for (var group = 0u; group < count; group += 1u) {
        if ((mask & (1u << group)) != 0u) {
            let layer = pages * 2u * (group + 1u) + page * 2u;
            hdr += visual_atlas_layer(in, layer, normal);
        }
    }
    return visual_hdr(hdr * environment.light_scale);
}
fn visual_diagnostic_color(in: VsOut, base: vec3<f32>, normal: vec3<f32>, mode: u32) -> vec3<f32> {
    // Derivatives evaluated before varying availability branches.
    let grid_width = max(fwidth(in.lightmap_uv * 16.0), vec2<f32>(0.002));
    switch mode {
        case 1u: { return base; }
        case 2u: {
            if (dot(in.world_normal, in.world_normal) < 1.0e-12) { return visual_unavailable(); }
            return normalize(in.world_normal) * 0.5 + 0.5;
        }
        case 3u: { return normal * 0.5 + 0.5; }
        case 4u: { return visual_baked(in, normal); }
        case 5u: {
            if (!visual_atlas_available(in)) { return visual_unavailable(); }
            return visual_hdr(visual_atlas_mean(in));
        }
        case 6u: {
            if (!visual_atlas_available(in)) { return visual_unavailable(); }
            let cell = fract(in.lightmap_uv * 16.0);
            let edge = min(cell, vec2<f32>(1.0) - cell);
            let grid = 1.0 - min(smoothstep(0.0, grid_width.x, edge.x), smoothstep(0.0, grid_width.y, edge.y));
            let color = vec3<f32>(in.lightmap_uv, fract((in.lightmap_page + 1.0) * 0.61803399));
            return mix(color, vec3<f32>(1.0), grid);
        }
        case 7u: { return vec3<f32>(material.roughness); }
        case 8u: { return vec3<f32>(in.clip.z); }
        case 9u: { return vec3<f32>(clamp(length(camera.position - in.world_position) / 100.0, 0.0, 1.0)); }
        case 10u: {
            if (environment.entity_irradiance.w > 0.5) { return vec3<f32>(0.2, 1.0, 0.3); }
            if (environment.entity_irradiance.w < -0.5) { return vec3<f32>(1.0, 0.0, 0.2); }
            if (visual_atlas_available(in)) { return vec3<f32>(0.0, 0.7, 1.0); }
            return vec3<f32>(1.0, 0.6, 0.1);
        }
        default: { return visual_unavailable(); }
    }
}
