
// The reference's `light` term for one fragment, per channel.
//
// The OpenGL world fragment stage contains exactly one light expression:
//
//     float lightmap_on = u_lightmap_enabled * (1.0 - step(254.5, v_lightmap_page));
//     vec3 light = vec3(1.0);
//     if (lightmap_on > 0.5) {
//         light = texture2D(u_lightmap0, uv).rgb;
//     }
//     light *= u_light_scale;
//
// Places binds the prepared pages as one `texture_2d_array` whose layer pairs
// are irradiance/direction planes, so the same expression generalises: the
// vertex's page byte selects the base pair, the uniform's switchable count and
// mask add each enabled fixture's prepared pair, and the reconstructed HDR sum
// remains HDR until presentation. There is no light loop over fixtures, no
// light array and no attenuation curve in the fragment stage: every fixture's
// contribution is already solved into the layers. A vertex with no lightmap
// coordinates (`page >= 254.5`, the historical vertex-lit build) keeps the
// light the bake folded into its colour, and the factor stays the unit vector.
// The dynamic path's factor is additionally multiplied by the object's neutral
// probe (`u_light_scale`), which is why the environment uniform carries it.
fn surface_light(in: VsOut, normal: vec3<f32>) -> vec3<f32> {
    if (environment.entity_bounds_min.w > 0.5) {
        return spatial_entity_light(in, normal);
    }
    if (environment.entity_irradiance.w > 0.5) {
        let energy = environment.entity_irradiance.rgb;
        let moment = environment.entity_moment.xyz;
        let k = energy.r + energy.g + energy.b;
        let lobe = 2.0 * max(0.0, dot(moment, normal)) - length(moment);
        let directional = energy / max(k, 1.0e-6) * lobe;
        return max(vec3<f32>(0.0), energy + select(vec3<f32>(0.0), directional, k > 1.0e-6));
    }
    let lightmap_on = environment.lightmap_enabled * (1.0 - step(254.5, in.lightmap_page));
    if (lightmap_on <= 0.5) {
        return vec3<f32>(1.0) * environment.light_scale;
    }
    // The page byte is an integer carried as a float; +0.5 and truncate is the
    // exact page index the mesh stamped.
    let page = u32(in.lightmap_page + 0.5);
    let pages = environment.lightmap_page_count;
    var hdr = decode_lightmap(in.lightmap_uv, page * 2u, normal);
    let count = environment.lightmap_switchable & 0xFu;
    let mask = (environment.lightmap_switchable >> 8u) & 0xFu;
    for (var group = 0u; group < count; group = group + 1u) {
        if ((mask & (1u << group)) != 0u) {
            // The base group occupies the first `pages * 2u` layers; each
            // switchable contribution follows in group order, page-major.
            let layer = pages * 2u * (group + 1u) + page * 2u;
            hdr += decode_lightmap(in.lightmap_uv, layer, normal);
        }
    }
    return hdr * environment.light_scale;
}
