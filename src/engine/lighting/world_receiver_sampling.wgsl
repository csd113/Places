
// One lightmap page pair sampled at `uv`: the irradiance plane at `layer` and
// the direction-moment plane at `layer + 1u`, reconstructed at `normal`.
//
// The bake stores the transport solve as two linear HDR values per texel: the
// mean irradiance `I` and the vector sum `g` of the per-channel moment
// vectors, both already in world space. Both planes interpolate linearly, and
// the nonlinear step below is a function of the *scalar* `dot(g, n)` and the
// *scalar* `|g|` of the interpolated moment â€” never of encoded coordinates â€”
// so a bilinear blend stays smooth across texel boundaries:
//
//     k     = I.r + I.g + I.b
//     light = max(0, I + (I / max(k, 1e-6)) * (2 * max(0, dot(g, n)) - length(g)))
//
// A texel with no directional content (`g` zero) reconstructs to `I` alone; a
// single shared direction of any colour reconstructs its cosine lobe exactly
// (`2 * I * max(0, cos)`); a near-cancelling moment collapses smoothly to the
// mean; the result is bounded by `2 * I` and never draws negative light. The
// alpha channels of both planes are reserved and ignored.
fn decode_lightmap(uv: vec2<f32>, layer: u32, normal: vec3<f32>) -> vec3<f32> {
    let physical = lightmap_layer(layer);
    let irradiance = textureSample(lightmap_pages, lightmap_sampler, uv, physical).rgb;
    let direction = textureSample(lightmap_pages, lightmap_sampler, uv, physical + 1u).rgb;
    let k = irradiance.r + irradiance.g + irradiance.b;
    let lobe = 2.0 * max(0.0, dot(direction, normal)) - length(direction);
    let directional = irradiance / max(k, 1.0e-6) * lobe;
    return max(vec3<f32>(0.0), irradiance + select(vec3<f32>(0.0), directional, k > 1.0e-6));
}

fn reconstruct_irradiance(energy: vec3<f32>, moment: vec3<f32>, normal: vec3<f32>) -> vec3<f32> {
    let k = energy.r + energy.g + energy.b;
    let lobe = 2.0 * max(0.0, dot(moment, normal)) - length(moment);
    return max(vec3<f32>(0.0), energy + select(vec3<f32>(0.0), energy / max(k, 1.0e-6) * lobe, k > 1.0e-6));
}

struct EntityAnchorWeights {
    lower: vec4<f32>,
    upper: vec4<f32>,
};

fn entity_anchor_weights(local_position: vec3<f32>) -> EntityAnchorWeights {
    let coordinate = clamp((local_position - environment.entity_bounds_min.xyz) / max(environment.entity_bounds_extent.xyz, vec3<f32>(1.0e-6)), vec3<f32>(0.0), vec3<f32>(1.0));
    let inverse = vec3<f32>(1.0) - coordinate;
    // X-fastest corner order. Keep each original (x * y) * z product; two
    // vectors hold all eight weights without a dynamically indexed array.
    let xy = vec4<f32>(
        inverse.x * inverse.y,
        coordinate.x * inverse.y,
        inverse.x * coordinate.y,
        coordinate.x * coordinate.y,
    );
    var weights: EntityAnchorWeights;
    weights.lower = xy * inverse.z;
    weights.upper = xy * coordinate.z;
    return weights;
}

fn source_visibility_with_weights(weights: EntityAnchorWeights, source: u32) -> vec4<f32> {
    var visibility = vec4<f32>(0.0);
    // Read only the selected source's four taps from uniform storage. Each
    // constant corner retains the original 0..7 accumulation order without
    // copying an anchor's visibility array into fragment-local storage.
    visibility += weights.lower.x * environment.entity_anchors[0u].visibility[source];
    visibility += weights.lower.y * environment.entity_anchors[1u].visibility[source];
    visibility += weights.lower.z * environment.entity_anchors[2u].visibility[source];
    visibility += weights.lower.w * environment.entity_anchors[3u].visibility[source];
    visibility += weights.upper.x * environment.entity_anchors[4u].visibility[source];
    visibility += weights.upper.y * environment.entity_anchors[5u].visibility[source];
    visibility += weights.upper.z * environment.entity_anchors[6u].visibility[source];
    visibility += weights.upper.w * environment.entity_anchors[7u].visibility[source];
    return clamp(visibility, vec4<f32>(0.0), vec4<f32>(1.0));
}

fn source_visibility(local_position: vec3<f32>, source: u32) -> vec4<f32> {
    return source_visibility_with_weights(entity_anchor_weights(local_position), source);
}
