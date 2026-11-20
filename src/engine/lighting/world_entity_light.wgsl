
fn entity_direct_term_with_weights(in: VsOut, normal: vec3<f32>, weights: EntityAnchorWeights) -> vec3<f32> {
    let visibility_mask = entity_posed_visibility_mask(in, normal, weights);
    var result = vec3<f32>(0.0);
    var attenuation = vec3<f32>(0.0);
    attenuation += environment.entity_anchors[0u].attenuation.rgb * weights.lower.x;
    attenuation += environment.entity_anchors[1u].attenuation.rgb * weights.lower.y;
    attenuation += environment.entity_anchors[2u].attenuation.rgb * weights.lower.z;
    attenuation += environment.entity_anchors[3u].attenuation.rgb * weights.lower.w;
    attenuation += environment.entity_anchors[4u].attenuation.rgb * weights.upper.x;
    attenuation += environment.entity_anchors[5u].attenuation.rgb * weights.upper.y;
    attenuation += environment.entity_anchors[6u].attenuation.rgb * weights.upper.z;
    attenuation += environment.entity_anchors[7u].attenuation.rgb * weights.upper.w;
    for (var index = 0u; index < 8u; index = index + 1u) {
        // Keep the tap array in uniform storage rather than copying the whole
        // source into fragment-local storage for its dynamically indexed taps.
        let color_strength = environment.entity_direct[index].color_strength;
        if (color_strength.w <= 0.0) { continue; }
        let position_range = environment.entity_direct[index].position_range;
        let extent_falloff = environment.entity_direct[index].extent_falloff;
        let delta = position_range.xyz - in.world_position;
        let horizontal = max(abs(delta.xz) - extent_falloff.xy, vec2<f32>(0.0));
        let distance = select(length(delta), length(horizontal), extent_falloff.w > 0.5);
        let relative = distance / max(position_range.w, 1.0e-6);
        if (relative >= 1.0) { continue; }
        let u = 1.0 - max(relative, 0.0);
        var falloff = u * u * (1.0 + 2.0 * max(relative, 0.0));
        if (extent_falloff.z > 1.5) { falloff = 1.0; }
        else if (extent_falloff.z > 0.5) { falloff = u; }
        // All taps share the same trilinear receiver weights. Interpolate
        // once per in-range source, preserving each tap's independent value.
        let visibility = source_visibility_with_weights(weights, index);
        // An exactly occluded source contributes zero for every receiver
        // normal. Partial visibility keeps the original per-tap arithmetic.
        if (all(visibility == vec4<f32>(0.0))) { continue; }
        var cosine = 0.0;
        for (var tap = 0u; tap < 4u; tap = tap + 1u) {
            if (visibility[tap] == 0.0) { continue; }
            let sample = environment.entity_direct[index].taps[tap];
            if (sample.w <= 0.0) { continue; }
            let incoming = sample.xyz - in.world_position;
            let distance_to_tap = length(incoming);
            if (distance_to_tap > 1.0e-6) {
                let ndl = max(dot(normal, incoming / distance_to_tap), 0.0);
                // Match offline transport: the downward source face has a
                // projected emission cosine separate from receiver incidence.
                let emitted = select(1.0, max(incoming.y / distance_to_tap, 0.0), extent_falloff.w > 0.5);
                if (ndl > 0.0 && (visibility_mask & (1u << (index*4u+tap))) != 0u) {
                    cosine += sample.w * visibility[tap] * ndl * emitted;
                }
            }
        }
        result += color_strength.rgb * color_strength.w * falloff * cosine;
    }
    return result * attenuation;
}

fn entity_direct_term(in: VsOut, normal: vec3<f32>) -> vec3<f32> {
    return entity_direct_term_with_weights(in, normal, entity_anchor_weights(in.local_position));
}

fn spatial_entity_indirect(in: VsOut, weights: EntityAnchorWeights, normal: vec3<f32>) -> vec3<f32> {
    let bias_normal = posed_fragment_normal;
    var energy = vec3<f32>(0.0);
    var moment = vec3<f32>(0.0);
    energy += environment.entity_anchors[0u].irradiance.rgb * weights.lower.x;
    moment += environment.entity_anchors[0u].moment.rgb * weights.lower.x;
    energy += environment.entity_anchors[1u].irradiance.rgb * weights.lower.y;
    moment += environment.entity_anchors[1u].moment.rgb * weights.lower.y;
    energy += environment.entity_anchors[2u].irradiance.rgb * weights.lower.z;
    moment += environment.entity_anchors[2u].moment.rgb * weights.lower.z;
    energy += environment.entity_anchors[3u].irradiance.rgb * weights.lower.w;
    moment += environment.entity_anchors[3u].moment.rgb * weights.lower.w;
    energy += environment.entity_anchors[4u].irradiance.rgb * weights.upper.x;
    moment += environment.entity_anchors[4u].moment.rgb * weights.upper.x;
    energy += environment.entity_anchors[5u].irradiance.rgb * weights.upper.y;
    moment += environment.entity_anchors[5u].moment.rgb * weights.upper.y;
    energy += environment.entity_anchors[6u].irradiance.rgb * weights.upper.z;
    moment += environment.entity_anchors[6u].moment.rgb * weights.upper.z;
    energy += environment.entity_anchors[7u].irradiance.rgb * weights.upper.w;
    moment += environment.entity_anchors[7u].moment.rgb * weights.upper.w;
    let reconstructed = reconstruct_irradiance(energy, moment, normal);
    let root = select(0.0,environment.entity_bounds_extent.w,POSED_RESIDUAL_ENABLED);
    let k = energy.r + energy.g + energy.b;
    if (root == 0.0 || k <= 1.0e-6) { return reconstructed; }
    let scale = max(length(environment.model[0u].xyz), 1.0e-6);
    let reach = max(length(environment.entity_bounds_extent.xyz) * scale * 1.01, 0.01 * scale);
    // The existing L0/L1 reconstruction is an isotropic remainder plus a
    // dominant cosine lobe. Occlude those separately, preserving its exact
    // unoccluded result and the existing world/probe visibility.
    let isotropic = energy * max(1.0 - length(moment) / k, 0.0);
    let dominant = energy * (2.0 * max(dot(moment, normal), 0.0) / k);
    var loss = vec3<f32>(0.0);
    if (max(max(isotropic.r, isotropic.g), isotropic.b) > 1.0e-6) {
        var ambient_visibility = select(in.self_ambient.x, in.self_ambient.y, dot(normal, in.world_normal) < 0.0);
        let ready = select(in.self_ambient.z, in.self_ambient.w, dot(normal, in.world_normal) < 0.0);
        if (ready < 0.999999 && POSED_FALLBACK_ENABLED) {
            let basis = mat3x3<f32>(normalize(environment.model[0u].xyz),normalize(environment.model[1u].xyz),normalize(environment.model[2u].xyz));
            ambient_visibility = posed_hemisphere_visibility_basis(in.world_position,normal,root,scale,reach,basis);
        }
        ambient_visibility = clamp(ambient_visibility, 0.0, 1.0);
        loss += isotropic * (1.0 - ambient_visibility);
    }
    if (max(max(dominant.r, dominant.g), dominant.b) > 1.0e-6) {
        if (!posed_character_visible(in.world_position, bias_normal, in.world_position + normalize(moment) * reach, root, scale)) {
            loss += dominant;
        }
    }
    return max(reconstructed - loss, vec3<f32>(0.0));
}

fn spatial_entity_light(in: VsOut, normal: vec3<f32>) -> vec3<f32> {
    let weights = entity_anchor_weights(in.local_position);
    return spatial_entity_indirect(in, weights, normal) + entity_direct_term_with_weights(in, normal, weights);
}
