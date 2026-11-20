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
        let blur = material.roughness * 0.035;
        if (blur > 0.002) {
            sample_color += textureSample(planar_map, reflection_sampler, uv + vec2<f32>(blur, blur)).rgb;
            sample_color += textureSample(planar_map, reflection_sampler, uv + vec2<f32>(-blur, blur)).rgb;
            sample_color += textureSample(planar_map, reflection_sampler, uv + vec2<f32>(blur, -blur)).rgb;
            sample_color += textureSample(planar_map, reflection_sampler, uv + vec2<f32>(-blur, -blur)).rgb;
            sample_color *= 0.25;
        } else {
            sample_color = textureSample(planar_map, reflection_sampler, uv).rgb;
        }
        // Outside the reflected frame there is no image to show.
        let inside = step(0.0, uv.x) * step(uv.x, 1.0) * step(0.0, uv.y) * step(uv.y, 1.0);
        // Only a fragment on (or very near) the mirror plane reflects.
        let plane_distance = abs(dot(environment.planar_plane.xyz, in.world_position) + environment.planar_plane.w);
        let on_plane = 1.0 - smoothstep(0.0, 0.08, plane_distance);
        combine = combine * inside * on_plane;
    } else {
        let reflected = reflect(-view, normal);
        // A packaged probe carries an offline-prefiltered mip chain, so the
        // material's roughness selects the level directly; the top level is
        // packed in the environment word. A live capture has one level
        // (`probe_max_mip == 0`), where the chain does not exist and the
        // historical two-tap mix stands in so quality does not regress.
        let probe_max_mip = f32((environment.lightmap_switchable >> 16u) & 0xFu);
        let probe_level = clamp(material.roughness * probe_max_mip, 0.0, probe_max_mip);
        var sharp = textureSampleLevel(probe_map, reflection_sampler, reflected, probe_level).rgb;
        if (probe_max_mip < 0.5 && material.roughness > 0.15) {
            let broad = textureSample(
                probe_map,
                reflection_sampler,
                normalize(mix(reflected, normal, vec3<f32>(0.5))),
            ).rgb;
            sharp = mix(sharp, broad, vec3<f32>(material.roughness));
        }
        sample_color = sharp;
    }
    let strength = select(material.reflection_strength, clamp(material.reflection_strength * 6.0, vec3<f32>(0.0), vec3<f32>(0.95)), water_blend_pass && in.water_surface > 0.5);
    return strength * combine * sample_color;
}

// Eye-to-surface extinction through authored fog boxes. Slab clipping keeps
// work local to intersecting rays; four fixed Gauss quadrature samples retain
// horizontal softness and ground-height fade without a volume marching pass.
