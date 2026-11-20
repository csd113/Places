
// Local excitation of authored pigment. This does not feed emission or transport.
fn ultraviolet_response(in: VsOut, normal: vec3<f32>, base: vec3<f32>) -> vec3<f32> {
    if (material.uv_response <= 0.0) { return vec3<f32>(0.0); }
    var mask = vec3<f32>(1.0);
    if (material.uv_mask_enabled > 0.5) { mask = textureSample(uv_texture, uv_sampler, in.uv).rgb; }
    var visibility = in.uv_visibility;
    if (environment.uv_receiver.bounds_min.w > 0.5) {
        visibility = vec4<f32>(0.0);
        let coordinate = clamp((in.local_position - environment.uv_receiver.bounds_min.xyz) / max(environment.uv_receiver.bounds_extent.xyz, vec3<f32>(1.0e-6)), vec3<f32>(0.0), vec3<f32>(1.0));
        for (var corner = 0u; corner < 8u; corner += 1u) {
            let x = select(1.0 - coordinate.x, coordinate.x, (corner & 1u) != 0u);
            let y = select(1.0 - coordinate.y, coordinate.y, (corner & 2u) != 0u);
            let z = select(1.0 - coordinate.z, coordinate.z, (corner & 4u) != 0u);
            visibility += environment.uv_receiver.visibility[corner] * x * y * z;
        }
    }
    var excitation = 0.0;
    for (var slot = 0u; slot < 4u; slot += 1u) {
        let source = environment.blacklights[slot];
        if (source.color_strength.w <= 0.0 || visibility[slot] <= 0.0) { continue; }
        let delta = source.position_range.xyz - in.world_position;
        if (source.extent_falloff.w > 0.5 && delta.y <= 0.0) { continue; }
        let horizontal = max(abs(delta.xz) - source.extent_falloff.xy, vec2<f32>(0.0));
        let distance = length(vec3<f32>(horizontal.x, delta.y, horizontal.y));
        let range = source.position_range.w;
        if (range <= 0.0 || distance >= range) { continue; }
        let u = 1.0 - distance / range;
        var falloff = u * u * (3.0 - 2.0 * u);
        if (source.extent_falloff.z > 1.5) { falloff = 1.0; }
        else if (source.extent_falloff.z > 0.5) { falloff = u; }
        let incoming = unit_direction_or_zero(delta);
        excitation += source.color_strength.w * visibility[slot] * falloff * max(dot(normal, incoming), 0.0);
    }
    return base * mask * material.uv_response * clamp(excitation, 0.0, 1.0);
}

// The complete un-shadowed world fragment: lit + sheen + reflection + emission,
// then fog, all in linear light.
