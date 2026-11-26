
// The reference's reflection term, in linear light. A planar surface projects
// the reflected frame through the mirrored camera; a probe surface reads the
// static cubemap baked at load. Both are weighted by the authored strength (the
// material's `specular × strength`), a Fresnel term and the gloss.
//
// Reflection targets store unexposed linear HDR radiance; sampling never
// re-applies tone mapping, exposure or display conversion.
fn surface_reflection(in: VsOut, normal: vec3<f32>, view: vec3<f32>, water_reflectance: f32) -> vec3<f32> {
    if (!reflection_surface_enabled(material.reflection_mode, material.reflection_strength)) {
        return vec3<f32>(0.0);
    }
    let water = water_blend_pass && in.water_surface > 0.5;
    let ordinary = !water_blend_pass || in.water_surface <= 0.5;
    let weight = reflection_surface_weight(normal, view, material.roughness, !ordinary, water_reflectance);
    var sample_color = vec3<f32>(0.0);
    var combine = weight;
    if (material.reflection_mode > 1u) {
        let clip = environment.planar_matrix * vec4<f32>(in.world_position, 1.0);
        // The reference's projection, then the WebGPU texture-row convention:
        // the capture wrote NDC +Y into the target's first row, while GL's
        // framebuffer and texture rows both put NDC +Y last, so the vertical
        // axis of the sampled image is flipped relative to the reference.
        let gl_uv = clip.xy / max(clip.w, 1.0e-4) * 0.5 + 0.5;
        let uv = vec2<f32>(gl_uv.x, 1.0 - gl_uv.y);
        // A rough surface reads a small disc around the projected point; a
        // polished one reads the single texel the plane projects to.
        let blur = planar_blur_radius(material.roughness);
        if (planar_uses_blur(blur)) {
            let offsets = planar_blur_offsets(blur);
            sample_color += textureSample(planar_map, reflection_sampler, uv + offsets[0]).rgb;
            sample_color += textureSample(planar_map, reflection_sampler, uv + offsets[1]).rgb;
            sample_color += textureSample(planar_map, reflection_sampler, uv + offsets[2]).rgb;
            sample_color += textureSample(planar_map, reflection_sampler, uv + offsets[3]).rgb;
            sample_color = planar_blur_average(sample_color);
        } else {
            sample_color = textureSample(planar_map, reflection_sampler, uv).rgb;
        }
        // Outside the reflected frame there is no image to show.
        let inside = step(0.0, uv.x) * step(uv.x, 1.0) * step(0.0, uv.y) * step(uv.y, 1.0);
        // Only a fragment on (or very near) the mirror plane reflects.
        let on_plane = reflection_plane_weight(environment.planar_plane, in.world_position);
        combine = combine * inside * on_plane;
    } else {
        let reflected = reflect(-view, normal);
        // A packaged probe carries an offline-prefiltered mip chain, so the
        // material's roughness selects the level directly; the top level is
        // packed in the environment word. A live capture has one level
        // (`probe_max_mip == 0`), where the chain does not exist and the
        // historical two-tap mix stands in so quality does not regress.
        let probe_max_mip = f32((environment.lightmap_switchable >> 16u) & 0xFu);
        let probe_level = reflection_probe_mip(material.roughness, probe_max_mip);
        var sharp = textureSampleLevel(probe_map, reflection_sampler, reflected, probe_level).rgb;
        if (live_probe_uses_broad_tap(probe_max_mip, material.roughness)) {
            let broad = textureSample(
                probe_map,
                reflection_sampler,
                live_probe_broad_direction(reflected, normal),
            ).rgb;
            sharp = live_probe_mix(sharp, broad, material.roughness);
        }
        sample_color = sharp;
    }
    let strength = select(material.reflection_strength, water_reflection_strength(material.reflection_strength), water);
    return strength * combine * sample_color;
}

// Eye-to-surface extinction through authored fog boxes. Slab clipping keeps
// work local to intersecting rays; four fixed Gauss quadrature samples retain
// horizontal softness and ground-height fade without a volume marching pass.
