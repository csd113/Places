
// The reference's lit term, in linear light:
// `tex_color.rgb * v_color.rgb * light * (1.0 - u_emission_vertex)`.
fn lit_linear(base_linear: vec3<f32>, vertex_color: vec3<f32>, light: vec3<f32>, emission_vertex: f32) -> vec3<f32> {
    return base_linear * vertex_color * light * (1.0 - emission_vertex);
}

// Preserve direction before length/cross-product arithmetic. Screen-space
// derivatives shrink as the camera approaches or render resolution increases;
// authored normals change magnitude under the inverse-transpose model transform.
fn rescale_direction(direction: vec3<f32>) -> vec3<f32> {
    let scale = max(max(abs(direction.x), abs(direction.y)), abs(direction.z));
    return direction / select(1.0, scale, scale > 0.0);
}

fn unit_direction_or_zero(direction: vec3<f32>) -> vec3<f32> {
    let scaled = rescale_direction(direction);
    // Nonzero scaled vectors have a component of magnitude one. The zero
    // vector therefore stays finite without normalizing it or using an epsilon.
    return scaled * inverseSqrt(max(dot(scaled, scaled), 1.0));
}

struct SurfaceNormals {
    material: vec3<f32>,
    geometric: vec3<f32>,
};

// Prepare the normal frame once. Material shading orients the visible side
// and applies the normal map; caustic receiver selection keeps the original
// geometric orientation and its independent front-facing gate.
fn surface_normals(in: VsOut, front_facing: bool, selected_water: u32) -> SurfaceNormals {
    // Models may omit authored normals. Derive their diffuse normal from
    // actual posed world geometry, oriented toward the visible side.
    // Derivatives remain outside conditional control flow. Scale each vector
    // before crossing so a valid tiny/large surface cannot underflow/overflow.
    let position_dx = dpdx(in.world_position);
    let position_dy = dpdy(in.world_position);
    let authored_normal = unit_direction_or_zero(in.world_normal);
    let has_authored_normal = any(in.world_normal != vec3<f32>(0.0));
    var geometric = authored_normal;
    if (!has_authored_normal) {
        geometric = unit_direction_or_zero(cross(rescale_direction(position_dy), rescale_direction(position_dx)));
    }
    var normal = select(vec3<f32>(0.0, 1.0, 0.0), authored_normal, has_authored_normal);
    if (!front_facing) { normal = -normal; }
    if (!has_authored_normal) {
        if (any(geometric != vec3<f32>(0.0))) {
            normal = geometric;
            if (dot(normal, camera.position - in.world_position) < 0.0) { normal = -normal; }
        }
    }
    let normal_on = (material.flags & MATERIAL_FLAG_RESPONSE_ENABLED) != 0u
        && (material.flags & MATERIAL_FLAG_NORMAL_ENABLED) != 0u;
    if (normal_on) {
        let sampled = textureSample(normal_texture, normal_sampler, in.uv).xyz * 2.0 - 1.0;
        let scaled = vec3<f32>(
            sampled.x * material.normal_strength,
            sampled.y * material.normal_strength,
            sampled.z,
        );
        let tangent = normalize(in.world_tangent - normal * dot(normal, in.world_tangent));
        let bitangent = cross(normal, tangent) * in.handedness;
        normal = normalize(tangent * scaled.x + bitangent * scaled.y + normal * scaled.z);
    }
    var out: SurfaceNormals;
    if (water_blend_pass && in.water_surface > 0.5) {
        let selected = selected_water;
        if (selected > 0u) {
            let motion = caustics.nodes[selected - 1u].motion;
            let slope = water_wave_slope(in.world_position.xz, motion, caustics.seconds)
                + water_local_slope(in.world_position, caustics.nodes[selected - 1u].links.z);
            let facing = select(-1.0, 1.0, front_facing);
            normal = normalize(normal + vec3<f32>(-slope.x, 0.0, -slope.y) * facing);
        }
    }
    out.material = normal;
    out.geometric = geometric;
    return out;
}

// The reference's view-dependent sheen, in linear light.
fn surface_sheen(in: VsOut, normal: vec3<f32>, view: vec3<f32>, light: vec3<f32>, water_reflectance: f32) -> vec3<f32> {
    if ((material.flags & MATERIAL_FLAG_RESPONSE_ENABLED) == 0u || all(material.specular == vec3<f32>(0.0))) {
        return vec3<f32>(0.0);
    }
    if (water_blend_pass && in.water_surface > 0.5) { return material.specular * water_reflectance * light * 0.12; }
    let facing = clamp(abs(dot(normal, view)), 0.0, 1.0);
    let gloss = 1.0 - material.roughness;
    let grazing = pow(1.0 - facing, mix(1.0, 16.0, gloss));
    let ahead = pow(facing, mix(1.0, 24.0, gloss)) * gloss;
    return material.specular * (grazing * 0.55 + ahead * 0.45) * light;
}

// The reference's reflection term, in linear light. A planar surface projects
// the reflected frame through the mirrored camera; a probe surface reads the
// static cubemap baked at load. Both are weighted by the authored strength (the
// material's `specular Ã— strength`), a Fresnel term and the gloss.
//
// Reflection targets store unexposed linear HDR radiance; sampling never
// re-applies tone mapping, exposure or display conversion.
fn surface_reflection(in: VsOut, normal: vec3<f32>, view: vec3<f32>, water_reflectance: f32) -> vec3<f32> {
    if (material.reflection_mode == 0u || all(material.reflection_strength == vec3<f32>(0.0))) {
        return vec3<f32>(0.0);
    }
    var weight = water_reflectance;
    if ((!water_blend_pass || in.water_surface <= 0.5)) {
        let facing = clamp(abs(dot(normal, view)), 0.0, 1.0);
        let fresnel = mix(0.08, 1.0, pow(1.0 - facing, 5.0));
        let gloss = clamp(1.0 - material.roughness, 0.0, 1.0);
        let polish = gloss * gloss;
        weight = mix(fresnel * 0.35, 1.0, polish);
    }
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
// The emissive term, in linear light: `mix(u_emission_color, v_color.rgb,
// u_emission_vertex) * mask * tex_color.rgb * u_emission_scale`.
fn surface_emission(in: VsOut, base_linear: vec3<f32>) -> vec3<f32> {
    var mask = vec3<f32>(1.0);
    if (material.emission_mask_enabled > 0.5) {
        mask = textureSample(emission_texture, emission_sampler, in.uv).rgb;
    }
    let color = mix(material.emission_color, in.color.rgb, material.emission_vertex);
    return color * mask * base_linear * material.emission_scale;
}

struct Shaded {
    // Linear HDR radiance before exposure and display encoding.
    color: vec3<f32>,
    alpha: f32,
};

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
