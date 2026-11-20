
// This presentation-only gate retains collision, lighting, and package geometry.
// Testing the geometric normal also handles a sloped roof and its top backing.
override editor_ceiling_gate: bool = true;

fn editor_ceiling_hidden(in: VsOut) -> bool {
    return editor_ceiling_gate && (material.flags & 8u) != 0u && (in.world_normal.y >= 0.0 ||
        dot(in.world_normal, camera.position - in.world_position) <= 0.0);
}

fn shade(in: VsOut, front_facing: bool, cutout: bool) -> Shaded {
    // Derivatives stay outside volume-dependent flow control. Transform fluid
    // metres into this material's actual world UV frame, preserving its tiling.
    let uv_dx = dpdx(in.uv); let uv_dy = dpdy(in.uv);
    let pos_dx = dpdx(in.world_position.xz); let pos_dy = dpdy(in.world_position.xz);
    // Select the authored plane once; normals, advection and optics share it.
    var selected_water = 0u;
    if (water_blend_pass && in.water_surface > 0.5) { selected_water = water_surface_node_hinted(in.world_position, in.water_node_hint); }
    var uv = in.uv;
    if (water_blend_pass && in.water_surface > 0.5) {
        let selected = selected_water;
        if (selected > 0u) {
            let motion = caustics.nodes[selected - 1u].motion;
            let determinant = pos_dx.x * pos_dy.y - pos_dy.x * pos_dx.y;
            if (abs(determinant) > 1.0e-12) {
                let travel = motion.zw * caustics.seconds;
                let coefficients = vec2<f32>(travel.x * pos_dy.y - travel.y * pos_dy.x, pos_dx.x * travel.y - pos_dx.y * travel.x) / determinant;
                uv -= uv_dx * coefficients.x + uv_dy * coefficients.y;
            }
            let phase = caustics.seconds * motion.x;
            uv += vec2<f32>(sin(phase * 0.73), sin(phase * 0.91)) * motion.y * 0.025;
        }
    }
    let base = textureSampleGrad(base_texture, base_sampler, uv, uv_dx, uv_dy);
    let base_linear = base.rgb;
    // The reference's alpha: texture x vertex-colour alpha x material opacity,
    // then the per-instance opacity. The instance factor is exactly 1.0 for
    // the static world and for every prop, so their alpha is unchanged.
    let alpha = base.a * in.color.a * material.opacity * environment.opacity;
    if (cutout && alpha < material.alpha_cutoff) {
        discard;
    }
    let normals = surface_normals(in, front_facing, selected_water);
    posed_fragment_normal = self_shadow_bias_normal(in, normals.material);
    let normal = normals.material;
    // The baked light term plus the frame's attached lights. The dynamic term
    // is additive and never modifies `surface_light`, so every lightmap and
    // baked-occlusion result survives; with no attached lights it is exactly
    // zero. No baked energy is clipped before material evaluation.
    let beam = flashlight_term(normal, in.world_position);
    let receiver_normal = select(vec3<f32>(0.0), normals.geometric, front_facing);
    let selected = caustic_volume(in.world_position, receiver_normal);
    let modulation = caustic_modulation(in.world_position, selected, dpdx(in.world_position.xz * 0.45), dpdy(in.world_position.xz * 0.45), beam, caustic_receiver_light(in, selected));
    let baked = surface_light(in, normal) * movable_contact_visibility(in, normal);
    let attached = dynamic_light_term(normal, in.world_position);
    let view = normalize(camera.position - in.world_position);
    var incident = baked + attached + beam;
    if (water_blend_pass && in.water_surface > 0.5
        && environment.lightmap_enabled * (1.0 - step(254.5, in.lightmap_page)) <= 0.5) {
        // Vertex-lit water already contains incident energy in its colour.
        // The neutral fallback factor is not incident light for its sheen.
        incident = baked * in.color.rgb + attached + beam;
    }
    var water_reflectance = 0.0;
    if (water_blend_pass && in.water_surface > 0.5) { water_reflectance = water_fresnel(normal, view); }
    let sheen = surface_sheen(in, normal, view, incident, water_reflectance);
    let reflection = surface_reflection(in, normal, view, water_reflectance);
    let emission = surface_emission(in, base_linear);
    // Baked vertex colour already carries static illumination. Realtime
    // sources multiply the independent unlit material factor instead.
    let lit = lit_linear(base_linear * material.albedo_tint.rgb, in.color.rgb, baked, material.emission_vertex)
        + lit_linear(base_linear * material.albedo_tint.rgb, in.caustics_albedo, attached + beam, material.emission_vertex);
    let focused = lit_linear(base_linear * material.albedo_tint.rgb, in.caustics_albedo, modulation * movable_contact_visibility(in, normal), material.emission_vertex);
    let pigment = base_linear * material.uv_tint.rgb;
    let fluorescence = ultraviolet_response(in, normal, pigment);
    let color = fogged(max(lit + focused, vec3<f32>(0.0)) + sheen + reflection + emission + fluorescence, in.world_position);
    var out: Shaded;
    out.color = storm_fog(camera.position, in.world_position, color);
    out.alpha = alpha;
    if (water_blend_pass && in.water_surface > 0.5 && water_presentation.controls.y > 0.5) {
        let water = water_composite(in, normal, view, out.color - reflection, reflection, alpha, water_reflectance, selected_water);
        out.color = water.rgb;
        out.alpha = water.a;
    }
    if (dot(camera.clip_plane, vec4<f32>(in.world_position, 1.0)) < 0.0) { discard; }
    if (editor_ceiling_hidden(in)) { discard; }
    return out;
}

// The opaque and translucent fragment stage for the sRGB surface (the direct
// path): output linear RGB for hardware encoding.
@fragment
fn fs_main(in: VsOut, @builtin(front_facing) front_facing: bool) -> @location(0) vec4<f32> {
    let shaded = shade(in, front_facing, false);
    return vec4<f32>(shaded.color, shaded.alpha);
}

// The alpha-tested fragment stage for the sRGB surface.
@fragment
fn fs_cutout(in: VsOut, @builtin(front_facing) front_facing: bool) -> @location(0) vec4<f32> {
    let shaded = shade(in, front_facing, true);
    return vec4<f32>(shaded.color, shaded.alpha);
}

// The opaque and translucent fragment stage for a raw (non-sRGB) scene or
// reflection target: the reference's own framebuffer convention, written
// directly so blending, filtering and capture readbacks stay in linear light.
@fragment
fn fs_main_raw(in: VsOut, @builtin(front_facing) front_facing: bool) -> @location(0) vec4<f32> {
    let shaded = shade(in, front_facing, false);
    return vec4<f32>(target_color(shaded.color), shaded.alpha);
}

// The alpha-tested fragment stage for a raw target.
@fragment
fn fs_cutout_raw(in: VsOut, @builtin(front_facing) front_facing: bool) -> @location(0) vec4<f32> {
    let shaded = shade(in, front_facing, true);
    return vec4<f32>(target_color(shaded.color), shaded.alpha);
}

// The emissive pass stores linear HDR radiance without a display curve.
fn emissive_only(in: VsOut, cutout: bool) -> vec4<f32> {
    let base = textureSample(base_texture, base_sampler, in.uv);
    let alpha = base.a * in.color.a * material.opacity * environment.opacity;
    if (cutout && alpha < material.alpha_cutoff) {
        discard;
    }
    // The returned alpha includes instance opacity; straight-alpha blending
    // scales bloom once. The attached-light term is deliberately absent here.
    let transmission = (1.0 - atmosphere(in.world_position).a) * storm_transmission(camera.position, in.world_position);
    let emission = surface_emission(in, base.rgb) * transmission;
    if (dot(camera.clip_plane, vec4<f32>(in.world_position, 1.0)) < 0.0) { discard; }
    if (editor_ceiling_hidden(in)) { discard; }
    return vec4<f32>(emission, alpha);
}

@fragment
fn fs_emission(in: VsOut, @builtin(front_facing) front_facing: bool) -> @location(0) vec4<f32> {
    let emission = emissive_only(in, false);
    return vec4<f32>(emission.rgb, 1.0);
}

@fragment
fn fs_emission_cutout(in: VsOut, @builtin(front_facing) front_facing: bool) -> @location(0) vec4<f32> {
    let emission = emissive_only(in, true);
    return vec4<f32>(emission.rgb, 1.0);
}

// Straight-alpha bloom composition uses the same coverage as the scene. A
// non-emitting translucent pane attenuates emission already drawn behind it.
@fragment
fn fs_emission_blend(in: VsOut) -> @location(0) vec4<f32> {
    return emissive_only(in, false);
}

// Read weather directly from uniform storage. Passing the complete shelter
// array by value makes Metal copy it into thread-private memory per fragment.
fn storm_density() -> f32 { return environment.storm.color_density.a; }
fn storm_color() -> vec3<f32> { return srgb_to_linear(environment.storm.color_density.rgb); }
fn storm_count() -> u32 { return environment.storm.count.x; }
fn storm_shelter(index: u32) -> StormShelter { return environment.storm.shelters[index]; }

fn linear_to_srgb(color: vec3<f32>) -> vec3<f32> {
    return select(1.055 * pow(max(color, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.4)) - 0.055, color * 12.92, color <= vec3<f32>(0.0031308));
}

fn target_color(color: vec3<f32>) -> vec3<f32> {
    if (display_target) { return linear_to_srgb(color); }
    return color;
}


// Visibility-only entries use the same vertices, model matrices, texture UVs,
// material opacity and authored cutout threshold as the ordinary world pass.
@fragment
fn fs_shadow(in: VsOut) {
    if (environment.opacity < 0.5) { discard; }
}

@fragment
fn fs_shadow_cutout(in: VsOut) {
    let alpha = textureSample(base_texture, base_sampler, in.uv).a * in.color.a * material.opacity * environment.opacity;
    if (alpha < material.alpha_cutoff) { discard; }
}

@fragment
fn fs_shadow_blend(in: VsOut) {
    // Straight transparent glass and water pass the beam. A fully covering
    // blend primitive still blocks, matching its visible solid coverage.
    let alpha = textureSample(base_texture, base_sampler, in.uv).a * in.color.a * material.opacity * environment.opacity;
    if (alpha < 0.95) { discard; }
}


fn ordinary_fog_density() -> f32 { return environment.fog_density; }
fn ordinary_fog_region_count() -> u32 { return environment.fog_region_count; }
fn ordinary_fog_reference_y() -> f32 { return environment.fog_reference_y; }
fn ordinary_fog_height_gain() -> f32 { return environment.fog_height_gain; }
fn ordinary_fog_color() -> vec3<f32> { return environment.fog_color; }
fn ordinary_fog_region(index: u32) -> FogRegion { return environment.fog_regions[index]; }
