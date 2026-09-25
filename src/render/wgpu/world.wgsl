// Stage 9 world shader: the complete static Places world fragment assembly —
// base texture, material response, baked light from the lightmap atlas (or the
// vertex-lit fallback), emission, reflections, fog — exactly where the OpenGL
// reference assembles them, in the same raw display space.
//
// Scope:
//
// * sample the draw's base-colour texture with the material's world tiling UV
//   (raw `Rgba8Unorm`, so the authored display bytes reach the assembly as
//   authored);
// * reproduce the OpenGL reference's display-space fragment assembly —
//   `tex_color.rgb * v_color.rgb * light * (1 - emission_vertex)`
//   `+ sheen + reflection + emission`, then fog — writing raw display-space
//   targets directly; only the surface-facing entry points (`fs_main`,
//   `fs_cutout`) convert once with `srgb_to_linear` for the sRGB surface;
// * take `light` from the lightmap atlas when one is resident (the reference's
//   default path), page-selected by the vertex's page byte, and from the
//   historical vertex-lit colour otherwise;
// * decode the material's normal map (when the material binds one) into the
//   world-space material normal the sheen and reflection terms use;
// * classify alpha: opaque, alpha-tested (`fs_cutout`) and straight-alpha
//   blending (the same `fs_main` body, blended by the pipeline);
// * classify emission: the emissive pass entry points (`fs_emission`,
//   `fs_emission_cutout`) write the emissive term alone into the raw bloom
//   target, exactly like the reference's `u_emission_only` return.
//
// The light the reference's world stage uses is *baked*, not realtime: the
// OpenGL renderer has no light selection, no light array and no shadow map.
// Its fragment stage computes `light` from the lightmap atlas when one is
// resident and `vec3(1.0)` otherwise. See `docs/WGPU_LIGHTING.md`.
//
// Coordinate convention: `camera.view_projection` is the Places camera matrix
// with the single OpenGL -> wgpu clip-space correction already applied on the
// CPU (`render::wgpu::world::clip_correction`), so this shader does no sign
// flips, no Y negation and no depth remapping. The column-major `mat4x4<f32>`
// memory layout is exactly what `CameraUniform` writes, and the material and
// environment uniform fields and offsets are exactly what `MaterialUniform`
// and `EnvironmentUniform` write.

struct Camera {
    // Clip-space view-projection; wgpu depth range (z in [0, 1]).
    view_projection: mat4x4<f32>,
    // World-space eye position, the reference's `u_camera_pos`. Offset 64.
    position: vec3<f32>,
    // Explicit tail padding so the struct is 80 bytes, 16-byte aligned.
    _padding: f32,
};

struct Material {
    // Sheen colour; the reference's `u_specular`.
    specular: vec3<f32>,
    // Shader-facing roughness (`1 - shine`), the reference's `u_roughness`.
    roughness: f32,
    // Normal-map `xy` scale.
    normal_strength: f32,
    // Cut-out threshold of the `fs_cutout` entry point.
    alpha_cutoff: f32,
    // Alpha multiplier.
    opacity: f32,
    // Bit 0: normal map bound and enabled. Bit 1: surface response enabled.
    // Bit 2: reflection eligible.
    flags: u32,
    // `specular × authored strength`, the reference's `u_reflect_strength`.
    reflection_strength: vec3<f32>,
    // 0 none, 1 probe, 2 planar; the reference's `u_reflect_mode`.
    reflection_mode: u32,
    // `emissive × intensity`, the reference's `u_emission_color`.
    emission_color: vec3<f32>,
    // Whether the emission mask texture is bound (the reference's
    // `u_emission_mask_enabled`).
    emission_mask_enabled: f32,
    // 1 when the vertex colour itself is the emission (fixture faces).
    emission_vertex: f32,
    // Animated emission multiplier, 1.0 without an animation.
    emission_scale: f32,
    // Explicit tail padding; the struct is 80 bytes, 16-byte aligned.
    _padding0: f32,
    _padding1: f32,
};

// The whole frame/level environment: the baked-light switch and scale, the fog
// constants, and the flat-data the reflection terms need (the mirrored planar
// view-projection and the active mirror plane).
struct Environment {
    // The reference's `u_light_scale`; `1` for static geometry.
    light_scale: vec3<f32>,
    // The reference's `u_lightmap_enabled`.
    lightmap_enabled: f32,
    // The reference's `u_fog_color`.
    fog_color: vec3<f32>,
    // The reference's `u_fog_density`.
    fog_density: f32,
    // The reference's `u_fog_reference_y`.
    fog_reference_y: f32,
    // The reference's `u_fog_height_gain`.
    fog_height_gain: f32,
    // Explicit padding to the mat4 alignment.
    _padding: vec2<f32>,
    // The reference's `u_planar_matrix`: the mirrored view-projection.
    planar_matrix: mat4x4<f32>,
    // The reference's `u_planar_plane`: `xyz` the unit normal, `w` the offset.
    planar_plane: vec4<f32>,
    // The reference's `u_model`: the object transform; identity for the static
    // world and for props, the moving transform for a dynamic object.
    model: mat4x4<f32>,
};

const MATERIAL_FLAG_NORMAL_ENABLED: u32 = 1u;
const MATERIAL_FLAG_RESPONSE_ENABLED: u32 = 2u;

// Group 0 is the frame camera.
@group(0) @binding(0)
var<uniform> camera: Camera;

// Group 1 is the texture cache's one base texture + sampler pair, supplied per
// draw at bind time. The sampler is the repeating linear or repeating nearest
// policy (the player's filtering setting); the wrap mode tiles UVs beyond one
// repeat.
@group(1) @binding(0)
var base_texture: texture_2d<f32>;
@group(1) @binding(1)
var base_sampler: sampler;

// Group 2 is the resolved material: parameters, the normal map (or the shared
// white fallback with the fetch gated off) and its sampler, and the emission
// mask (or the same fallback with its fetch gated off).
@group(2) @binding(0)
var<uniform> material: Material;
@group(2) @binding(1)
var normal_texture: texture_2d<f32>;
@group(2) @binding(2)
var normal_sampler: sampler;
@group(2) @binding(3)
var emission_texture: texture_2d<f32>;
@group(2) @binding(4)
var emission_sampler: sampler;

// Group 3 is the frame/level environment: the baked-light switch and scale,
// fog, the lightmap atlas pages, the reflection probe cubemap and the planar
// mirror image. The fallback views keep every binding complete even when no
// atlas or reflection is resident; the shader's switches decide what is read.
@group(3) @binding(0)
var<uniform> environment: Environment;
@group(3) @binding(1)
var lightmap0: texture_2d<f32>;
@group(3) @binding(2)
var lightmap1: texture_2d<f32>;
@group(3) @binding(3)
var lightmap_sampler: sampler;
@group(3) @binding(4)
var probe_map: texture_cube<f32>;
@group(3) @binding(5)
var planar_map: texture_2d<f32>;
@group(3) @binding(6)
var reflection_sampler: sampler;

struct WorldVertex {
    // World-space position, pre-transformed by the CPU builder.
    @location(0) position: vec3<f32>,
    // Unit geometric normal.
    @location(1) normal: vec3<f32>,
    // World tiling UV, already scaled by the material's tiling period.
    @location(2) uv: vec2<f32>,
    // RGBA shade, quantised to bytes exactly like the reference upload. In the
    // atlas build this is the material factor (`tint x directional face shade`)
    // with no baked light; in the vertex-lit build it also carries the bake.
    @location(3) color: vec4<f32>,
    // Lightmap atlas UV, in `[0, 1]`, from the vertex's 16-bit fixed point.
    @location(6) lightmap_uv: vec2<f32>,
    // Lightmap page byte as a float: 0 or 1 selects a page, 255 is
    // `LIGHTMAP_NONE` (the vertex keeps its vertex-lit colour).
    @location(7) lightmap_page: f32,
    // Unit UV-u tangent.
    @location(4) tangent: vec3<f32>,
    // Bitangent sign: cross(normal, tangent) * handedness is UV-v.
    @location(5) handedness: f32,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) world_normal: vec3<f32>,
    @location(3) world_tangent: vec3<f32>,
    @location(4) handedness: f32,
    // World-space fragment position, the reference's `v_world_pos`.
    @location(5) world_position: vec3<f32>,
    @location(6) lightmap_uv: vec2<f32>,
    @location(7) lightmap_page: f32,
};

@vertex
fn vs_main(vertex: WorldVertex) -> VsOut {
    let world = environment.model * vec4<f32>(vertex.position, 1.0);
    var out: VsOut;
    out.clip = camera.view_projection * world;
    out.uv = vertex.uv;
    out.color = vertex.color;
    // The frame goes through the model's rotation. The model transform is a
    // rigid motion, so the vectors stay unit length.
    out.world_normal = (environment.model * vec4<f32>(vertex.normal, 0.0)).xyz;
    out.world_tangent = (environment.model * vec4<f32>(vertex.tangent, 0.0)).xyz;
    out.handedness = vertex.handedness;
    out.world_position = world.xyz;
    out.lightmap_uv = vertex.lightmap_uv;
    out.lightmap_page = vertex.lightmap_page;
    return out;
}

// The IEC 61966-2-1 transfer function. Every texture and offscreen target is
// raw display space — exactly like the reference's non-sRGB framebuffer — so
// the fragment is assembled where the reference assembled it and the sRGB
// surface is the *only* conversion point (`srgb_to_linear` in the
// surface-facing entry points). Stage 9 kept an sRGB texture decode plus a
// compensating `linear_to_srgb` here; Stage 10 measured that round trip as a
// broad +1 display level on minified surfaces (blending decoded values is a
// convexity bias) and returned textures to raw.
fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let low = c / 12.92;
    let high = pow((c + 0.055) / 1.055, vec3<f32>(2.4));
    return select(high, low, c <= vec3<f32>(0.04045));
}

// The reference's `light` term for one fragment, per channel.
//
// The OpenGL world fragment stage contains exactly one light expression:
//
//     float lightmap_on = u_lightmap_enabled * (1.0 - step(254.5, v_lightmap_page));
//     vec3 light = vec3(1.0);
//     if (lightmap_on > 0.5) {
//         light = mix(texture2D(u_lightmap0, uv).rgb,
//                     texture2D(u_lightmap1, uv).rgb,
//                     step(0.5, v_lightmap_page));
//     }
//     light *= u_light_scale;
//
// There is no light loop, no light array and no attenuation curve in the
// fragment stage: every fixture's contribution is already baked. A vertex with
// no lightmap coordinates (`page >= 254.5`, the historical vertex-lit build)
// keeps the light the bake folded into its colour, and the factor stays the
// unit vector. Stage 10 multiplies the dynamic path's factor by the neutral
// probe (`u_light_scale`), which is why the environment uniform carries it.
fn surface_light(in: VsOut) -> vec3<f32> {
    let lightmap_on = environment.lightmap_enabled * (1.0 - step(254.5, in.lightmap_page));
    var light = vec3<f32>(1.0);
    if (lightmap_on > 0.5) {
        let page0 = textureSample(lightmap0, lightmap_sampler, in.lightmap_uv).rgb;
        let page1 = textureSample(lightmap1, lightmap_sampler, in.lightmap_uv).rgb;
        light = mix(page0, page1, step(0.5, in.lightmap_page));
    }
    return light * environment.light_scale;
}

// The reference's lit term, in display space:
// `tex_color.rgb * v_color.rgb * light * (1.0 - u_emission_vertex)`.
fn lit_display(base_display: vec3<f32>, vertex_color: vec3<f32>, light: vec3<f32>, emission_vertex: f32) -> vec3<f32> {
    return base_display * vertex_color * light * (1.0 - emission_vertex);
}

// The world-space material normal: the geometric normal, flipped for a
// back-facing fragment exactly like the reference's `gl_FrontFacing`, then
// perturbed by the tangent-space normal map when the material binds one.
fn material_normal(in: VsOut, front_facing: bool) -> vec3<f32> {
    var normal = normalize(in.world_normal);
    if (!front_facing) {
        normal = -normal;
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
    return normal;
}

// The reference's view-dependent sheen, in display space.
fn surface_sheen(in: VsOut, normal: vec3<f32>, view: vec3<f32>, light: vec3<f32>) -> vec3<f32> {
    if ((material.flags & MATERIAL_FLAG_RESPONSE_ENABLED) == 0u) {
        return vec3<f32>(0.0);
    }
    let facing = clamp(abs(dot(normal, view)), 0.0, 1.0);
    let gloss = 1.0 - material.roughness;
    let grazing = pow(1.0 - facing, mix(1.0, 16.0, gloss));
    let ahead = pow(facing, mix(1.0, 24.0, gloss)) * gloss;
    return material.specular * (grazing * 0.55 + ahead * 0.45) * light;
}

// The reference's reflection term, in display space. A planar surface projects
// the reflected frame through the mirrored camera; a probe surface reads the
// static cubemap baked at load. Both are weighted by the authored strength (the
// material's `specular × strength`), a Fresnel term and the gloss.
//
// The reflection targets are raw (non-sRGB) textures holding the same display
// space this shader assembles, so a sample is used directly — exactly like the
// reference read its RGBA8 attachments and cubemap.
fn surface_reflection(in: VsOut, normal: vec3<f32>, view: vec3<f32>) -> vec3<f32> {
    if (material.reflection_mode == 0u) {
        return vec3<f32>(0.0);
    }
    let facing = clamp(abs(dot(normal, view)), 0.0, 1.0);
    let fresnel = mix(0.08, 1.0, pow(1.0 - facing, 5.0));
    let gloss = clamp(1.0 - material.roughness, 0.0, 1.0);
    let polish = gloss * gloss;
    let weight = mix(fresnel * 0.35, 1.0, polish);
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
        var sharp = textureSample(probe_map, reflection_sampler, reflected).rgb;
        // A rough surface averages a wider cone of the room than a single
        // reflected ray would: mixing the reading towards the surface's own
        // facing direction stands in for a blurred cubemap read without a mip
        // chain, a second render or a per-tap kernel.
        if (material.roughness > 0.15) {
            let broad = textureSample(
                probe_map,
                reflection_sampler,
                normalize(mix(reflected, normal, vec3<f32>(0.5))),
            ).rgb;
            sharp = mix(sharp, broad, vec3<f32>(material.roughness));
        }
        sample_color = sharp;
    }
    return material.reflection_strength * combine * sample_color;
}

// The reference's fog term, in display space, applied to the finished surface
// colour: emission is a surface property, not a hole punched through the air.
fn fogged(color: vec3<f32>, world_position: vec3<f32>) -> vec3<f32> {
    let distance = length(camera.position - world_position);
    let below = max(0.0, environment.fog_reference_y - world_position.y);
    let density = environment.fog_density * (1.0 + environment.fog_height_gain * min(below, 12.0));
    var fog_amount = density * distance;
    fog_amount = 1.0 - exp(-fog_amount * fog_amount);
    return mix(color, environment.fog_color, clamp(fog_amount, 0.0, 1.0));
}

// The emissive term, in display space: `mix(u_emission_color, v_color.rgb,
// u_emission_vertex) * mask * tex_color.rgb * u_emission_scale`.
fn surface_emission(in: VsOut, base_display: vec3<f32>) -> vec3<f32> {
    var mask = vec3<f32>(1.0);
    if (material.emission_mask_enabled > 0.5) {
        mask = textureSample(emission_texture, emission_sampler, in.uv).rgb;
    }
    let color = mix(material.emission_color, in.color.rgb, material.emission_vertex);
    return color * mask * base_display * material.emission_scale;
}

struct Shaded {
    // Display-space colour: the reference assembles the fragment in raw display
    // values, and so does this shader. The entry points that write the sRGB
    // surface convert once; the ones that write a raw scene/capture target
    // (the reference's own framebuffer convention) write it directly, which is
    // what makes hardware alpha blending happen in the same space the
    // reference blended in.
    color: vec3<f32>,
    alpha: f32,
};

// The complete un-shadowed world fragment: lit + sheen + reflection + emission,
// then fog, all in display space.
fn shade(in: VsOut, front_facing: bool, cutout: bool) -> Shaded {
    let base = textureSample(base_texture, base_sampler, in.uv);
    let base_display = base.rgb;
    // The reference's alpha: texture x vertex-colour alpha x material opacity.
    let alpha = base.a * in.color.a * material.opacity;
    if (cutout && alpha < material.alpha_cutoff) {
        discard;
    }
    let light = surface_light(in);
    let normal = material_normal(in, front_facing);
    let view = normalize(camera.position - in.world_position);
    let sheen = surface_sheen(in, normal, view, light);
    let reflection = surface_reflection(in, normal, view);
    let emission = surface_emission(in, base_display);
    let lit = lit_display(base_display, in.color.rgb, light, material.emission_vertex);
    var color = fogged(lit + sheen + reflection + emission, in.world_position);
    // The Stage 6 unlit contract: an all-white vertex colour, the unit light
    // factor, no sheen, no reflection, no emission and no fog is exactly the
    // raw base-texture sample; the assignment spells that out so the bypass
    // cannot drift from the assembled value.
    if (all(in.color.rgb >= vec3<f32>(1.0))
        && all(light >= vec3<f32>(1.0))
        && all(sheen == vec3<f32>(0.0))
        && all(reflection == vec3<f32>(0.0))
        && all(emission == vec3<f32>(0.0))
        && environment.fog_density == 0.0) {
        color = base_display;
    }
    var out: Shaded;
    out.color = color;
    out.alpha = alpha;
    return out;
}

// The opaque and translucent fragment stage for the sRGB surface (the direct
// path and the UI's convention): assemble in display space, convert once.
@fragment
fn fs_main(in: VsOut, @builtin(front_facing) front_facing: bool) -> @location(0) vec4<f32> {
    let shaded = shade(in, front_facing, false);
    return vec4<f32>(srgb_to_linear(shaded.color), shaded.alpha);
}

// The alpha-tested fragment stage for the sRGB surface.
@fragment
fn fs_cutout(in: VsOut, @builtin(front_facing) front_facing: bool) -> @location(0) vec4<f32> {
    let shaded = shade(in, front_facing, true);
    return vec4<f32>(srgb_to_linear(shaded.color), shaded.alpha);
}

// The opaque and translucent fragment stage for a raw (non-sRGB) scene or
// reflection target: the reference's own framebuffer convention, written
// directly so blending, filtering and capture readbacks stay in display space.
@fragment
fn fs_main_raw(in: VsOut, @builtin(front_facing) front_facing: bool) -> @location(0) vec4<f32> {
    let shaded = shade(in, front_facing, false);
    return vec4<f32>(shaded.color, shaded.alpha);
}

// The alpha-tested fragment stage for a raw target.
@fragment
fn fs_cutout_raw(in: VsOut, @builtin(front_facing) front_facing: bool) -> @location(0) vec4<f32> {
    let shaded = shade(in, front_facing, true);
    return vec4<f32>(shaded.color, shaded.alpha);
}

// The emissive pass: the emissive term alone, in the raw display-space values
// the reference's `u_emission_only` return writes into its RGBA8 bloom source.
// The bloom targets are non-sRGB so no transfer function is applied here.
fn emissive_only(in: VsOut, cutout: bool) -> vec4<f32> {
    let base = textureSample(base_texture, base_sampler, in.uv);
    let alpha = base.a * in.color.a * material.opacity;
    if (cutout && alpha < material.alpha_cutoff) {
        discard;
    }
    return vec4<f32>(surface_emission(in, base.rgb), 1.0);
}

@fragment
fn fs_emission(in: VsOut, @builtin(front_facing) front_facing: bool) -> @location(0) vec4<f32> {
    return emissive_only(in, false);
}

@fragment
fn fs_emission_cutout(in: VsOut, @builtin(front_facing) front_facing: bool) -> @location(0) vec4<f32> {
    return emissive_only(in, true);
}
