override display_target: bool = false;

// Shared material shader for static, movable and character meshes.
// Base colour sampling returns linear RGB. Vertex colour is a linear factor,
// carrying baked light only in the vertex-lit fallback. Irradiance, attached
// lights, sheen, reflections and emission accumulate without a display curve
// into RGBA16F scene/capture targets. Presentation owns exposure and encoding.
// Alpha is straight coverage. Numeric normal/mask samples remain linear data.
// Authored fog/storm display colours decode once before linear mixing.
// Missing model normals use the geometric triangle normal; authored normals
// use the inverse transpose and tangents use the linear model transform.
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
    // Independent UV response, linear pigment and albedo tints; 112 bytes, align 16.
    uv_response: f32,
    uv_mask_enabled: f32,
    uv_tint: vec4<f32>,
    albedo_tint: vec4<f32>,
};

// One authored regional fog volume, in world space. `min_falloff.xyz` is the
// box minimum corner and `.w` the horizontal soft edge inside it, in metres;
// `max_top.xyz` the maximum corner and `.w` the world Y the layer has faded to
// zero at; `color_density.rgb` the colour the layer mixes towards and `.a` its
// density per metre; `ground_pad.x` the world Y the full-density ground layer
// starts at. 64 bytes, matching `FogRegionUniform`.
struct FogRegion {
    min_falloff: vec4<f32>,
    max_top: vec4<f32>,
    color_density: vec4<f32>,
    ground_pad: vec4<f32>,
};

// The whole frame/level environment: the baked-light switch and scale, the
// global fog constants and the level's regional fog volumes, the prepared
// lightmap layers' addressing, and the flat-data the reflection terms need
// (the mirrored planar view-projection and the active mirror plane).
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
    // Pages in one lightmap layer group (offset 40). Each page is two array
    // layers: irradiance then direction.
    lightmap_page_count: u32,
    // Packed switchable state (offset 44): bits 0..=3 the group count, bits
    // 8..=11 the enabled mask, bit `g` set when group `g` is on; bits 16..=19
    // hold the resident probe chain's top mip level (0 when the probes have a
    // single level and the rough fallback below stands in).
    lightmap_switchable: u32,
    // The reference's `u_planar_matrix`: the mirrored view-projection.
    planar_matrix: mat4x4<f32>,
    // The reference's `u_planar_plane`: `xyz` the unit normal, `w` the offset.
    planar_plane: vec4<f32>,
    // The reference's `u_model`: the object transform; identity for the static
    // world and for props, the moving transform for a dynamic object.
    model: mat4x4<f32>,
    // Per-instance opacity multiplier (offset 192). `1.0` for the static world
    // and for every draw without a fade; a character instance installs its
    // clamped fade opacity, which multiplies fragment alpha. Scene and bloom
    // blending apply that straight coverage once.
    opacity: f32,
    // The three pad words that keep `fog_region_count` at offset 208, exactly
    // where `EnvironmentUniform::tail_padding` sits. Never read.
    _tail0: f32,
    _tail1: f32,
    _tail2: f32,
    // Live entries in `fog_regions` (offset 208). The quality preset decides
    // how many authored regions are uploaded; the fragment loop is bounded by
    // this word, so a preset change moves the count with no rebuild.
    fog_region_count: u32,
    // Explicit padding so the fixed array starts 16-byte aligned at offset
    // 224. Never read.
    _fog_region_pad0: u32,
    _fog_region_pad1: u32,
    _fog_region_pad2: u32,
    // The level's regional fog volumes, the live ones first in authoring
    // order. Unused slots are all zero, so a read past the count adds nothing.
    fog_regions: array<FogRegion, 16u>,
    entity_irradiance: vec4<f32>,
    entity_moment: vec4<f32>,
    storm: Storm,
    normal_model: array<vec4<f32>, 3>,
    entity_bounds_min: vec4<f32>,
    entity_bounds_extent: vec4<f32>,
    entity_anchors: array<EntityLightingAnchor, 8>,
    entity_direct: array<EntityDirectLight, 8>,
    contact_shadows: array<ContactShadow, 8>,
    contact_shadow_count: vec4<u32>,
    blacklights: array<EntityDirectLight, 4>,
    uv_receiver: BlacklightReceiver,
    caustics_receiver: CausticsReceiver,
};

struct CausticsReceiver {
    bounds_min: vec4<f32>,
    bounds_extent: vec4<f32>,
    radiance: array<vec4<f32>, 8>,
};

struct BlacklightReceiver {
    bounds_min: vec4<f32>,
    bounds_extent: vec4<f32>,
    visibility: array<vec4<f32>, 8>,
};

struct EntityLightingAnchor {
    irradiance: vec4<f32>,
    moment: vec4<f32>,
    visibility: array<vec4<f32>, 8>,
    attenuation: vec4<f32>,
};

struct EntityDirectLight {
    position_range: vec4<f32>,
    color_strength: vec4<f32>,
    extent_falloff: vec4<f32>,
    taps: array<vec4<f32>, 4>,
};

struct ContactShadow {
    position_floor: vec4<f32>,
    half_height: vec4<f32>,
    incoming: vec4<f32>,
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
@group(2) @binding(5)
var uv_texture: texture_2d<f32>;
@group(2) @binding(6)
var uv_sampler: sampler;

// Group 3 is the frame/level environment: the baked-light switch and scale,
// fog, the prepared lightmap layer array, the reflection probe cubemap and the
// planar mirror image. The page array holds two layers per page per group and
// exactly the resident layer count (the fallback is a 1x1 pair), so the
// binding is complete even when no atlas or reflection is resident; the
// shader's switches decide what is read.
@group(3) @binding(0)
var<uniform> environment: Environment;
@group(3) @binding(1)
var lightmap_pages: texture_2d_array<f32>;
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
    // Float32 linear RGB factor and straight alpha. In the atlas build this is
    // the material factor (`tint x directional face shade`) with no baked light;
    // in the vertex-lit build it also carries HDR bake values without clipping.
    @location(3) color: vec4<f32>,
    // Lightmap atlas UV, in `[0, 1]`, from the vertex's 16-bit fixed point.
    @location(6) lightmap_uv: vec2<f32>,
    // Lightmap page byte as a float: 0..=254 selects the prepared page pair,
    // 255 is `LIGHTMAP_NONE` (the vertex keeps its vertex-lit colour).
    @location(7) lightmap_page: f32,
    // Unit UV-u tangent.
    @location(4) tangent: vec3<f32>,
    // Bitangent sign: cross(normal, tangent) * handedness is UV-v.
    @location(5) handedness: f32,
    @location(8) uv_visibility: vec4<f32>,
    @location(9) caustics_albedo: vec3<f32>,
    @location(10) water_surface: f32,
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
    @location(8) local_position: vec3<f32>,
    @location(9) uv_visibility: vec4<f32>,
    @location(10) caustics_albedo: vec3<f32>,
    @location(11) water_surface: f32,
};

@vertex
fn vs_main(vertex: WorldVertex) -> VsOut {
    let world = environment.model * vec4<f32>(vertex.position, 1.0);
    var out: VsOut;
    out.clip = camera.view_projection * world;
    out.uv = vertex.uv;
    out.color = vertex.color;
    out.uv_visibility = vertex.uv_visibility;
    out.caustics_albedo = vertex.caustics_albedo;
    out.water_surface = vertex.water_surface;
    // Normals use the inverse transpose; tangents use the model transform.
    // The material stage normalizes and re-orthogonalizes the frame.
    let normal_matrix = mat3x3<f32>(environment.normal_model[0].xyz, environment.normal_model[1].xyz, environment.normal_model[2].xyz);
    out.world_normal = normal_matrix * vertex.normal;
    out.world_tangent = (environment.model * vec4<f32>(vertex.tangent, 0.0)).xyz;
    let determinant = dot(cross(environment.model[0].xyz, environment.model[1].xyz), environment.model[2].xyz);
    out.handedness = vertex.handedness * select(1.0, -1.0, determinant < 0.0);
    out.world_position = world.xyz;
    out.local_position = vertex.position;
    out.lightmap_uv = vertex.lightmap_uv;
    out.lightmap_page = vertex.lightmap_page;
    return out;
}

// Decode authored fog/storm display colours before linear mixing.
fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let low = c / 12.92;
    let high = pow((c + 0.055) / 1.055, vec3<f32>(2.4));
    return select(high, low, c <= vec3<f32>(0.04045));
}

// One lightmap page pair sampled at `uv`: the irradiance plane at `layer` and
// the direction-moment plane at `layer + 1u`, reconstructed at `normal`.
//
// The bake stores the transport solve as two linear HDR values per texel: the
// mean irradiance `I` and the vector sum `g` of the per-channel moment
// vectors, both already in world space. Both planes interpolate linearly, and
// the nonlinear step below is a function of the *scalar* `dot(g, n)` and the
// *scalar* `|g|` of the interpolated moment — never of encoded coordinates —
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
    let irradiance = textureSample(lightmap_pages, lightmap_sampler, uv, layer).rgb;
    let direction = textureSample(lightmap_pages, lightmap_sampler, uv, layer + 1u).rgb;
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

// The same source-wide authored distance calibration and finite tap pattern as
// TransportEmitter. Visibility is spatially interpolated at bounds anchors;
// receiver cosines and falloff use the actual transformed fragment position.
fn entity_direct_term_with_weights(in: VsOut, normal: vec3<f32>, weights: EntityAnchorWeights) -> vec3<f32> {
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
                cosine += sample.w * visibility[tap] * max(dot(normal, incoming / distance_to_tap), 0.0);
            }
        }
        result += color_strength.rgb * color_strength.w * falloff * cosine;
    }
    return result * attenuation;
}

fn entity_direct_term(in: VsOut, normal: vec3<f32>) -> vec3<f32> {
    return entity_direct_term_with_weights(in, normal, entity_anchor_weights(in.local_position));
}

fn spatial_entity_indirect(weights: EntityAnchorWeights, normal: vec3<f32>) -> vec3<f32> {
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
    return reconstruct_irradiance(energy, moment, normal);
}

fn spatial_entity_light(in: VsOut, normal: vec3<f32>) -> vec3<f32> {
    let weights = entity_anchor_weights(in.local_position);
    return spatial_entity_indirect(weights, normal) + entity_direct_term_with_weights(in, normal, weights);
}

fn movable_contact_visibility(in: VsOut, normal: vec3<f32>) -> f32 {
    // Entity bodies cannot receive their own projected contact footprint.
    if (environment.entity_irradiance.w != 0.0 || normal.y <= 0.5) { return 1.0; }
    var loss = 0.0;
    for (var index = 0u; index < min(environment.contact_shadow_count.x, 8u); index = index + 1u) {
        let shadow = environment.contact_shadows[index];
        if (abs(in.world_position.y - shadow.position_floor.w) > 0.03) { continue; }
        let height = max(shadow.position_floor.y - shadow.position_floor.w, 0.0);
        let projection = -shadow.incoming.xz * height / max(shadow.incoming.y, 0.35);
        let offset = in.world_position.xz - shadow.position_floor.xz - projection;
        let radius = max(shadow.half_height.xy, vec2<f32>(0.05)) + vec2<f32>(0.08 + height * 0.15);
        let elliptical = offset / radius;
        let footprint = exp(-2.0 * dot(elliptical, elliptical));
        loss += 0.22 * footprint * shadow.incoming.w;
    }
    return 1.0 - min(loss, 0.22);
}

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
fn surface_normals(in: VsOut, front_facing: bool) -> SurfaceNormals {
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
    if (in.water_surface > 0.5) {
        let selected = water_surface_node(in.world_position);
        if (selected > 0u) {
            let motion = caustics.nodes[selected - 1u].motion;
            let point = in.world_position.xz - motion.zw * caustics.seconds;
            let phase = caustics.seconds * motion.x;
            // Analytic wave slopes perturb the existing surface frame; imagery
            // continues to come exclusively from the authored PNG.
            let a = dot(point, vec2<f32>(2.8, 1.1)) - phase * 1.7;
            let b = dot(point, vec2<f32>(-1.3, 3.2)) - phase * 1.1;
            let slope = (vec2<f32>(2.8, 1.1) * cos(a) + vec2<f32>(-1.3, 3.2) * cos(b) * 0.45) * motion.y * 0.035;
            let facing = select(-1.0, 1.0, front_facing);
            normal = normalize(normal + vec3<f32>(-slope.x, 0.0, -slope.y) * facing);
        }
    }
    out.material = normal;
    out.geometric = geometric;
    return out;
}

// The reference's view-dependent sheen, in linear light.
fn surface_sheen(in: VsOut, normal: vec3<f32>, view: vec3<f32>, light: vec3<f32>) -> vec3<f32> {
    if ((material.flags & MATERIAL_FLAG_RESPONSE_ENABLED) == 0u || all(material.specular == vec3<f32>(0.0))) {
        return vec3<f32>(0.0);
    }
    let facing = clamp(abs(dot(normal, view)), 0.0, 1.0);
    let gloss = 1.0 - material.roughness;
    let grazing = pow(1.0 - facing, mix(1.0, 16.0, gloss));
    let ahead = pow(facing, mix(1.0, 24.0, gloss)) * gloss;
    return material.specular * (grazing * 0.55 + ahead * 0.45) * light;
}

// The reference's reflection term, in linear light. A planar surface projects
// the reflected frame through the mirrored camera; a probe surface reads the
// static cubemap baked at load. Both are weighted by the authored strength (the
// material's `specular × strength`), a Fresnel term and the gloss.
//
// Reflection targets store unexposed linear HDR radiance; sampling never
// re-applies tone mapping, exposure or display conversion.
fn surface_reflection(in: VsOut, normal: vec3<f32>, view: vec3<f32>) -> vec3<f32> {
    if (material.reflection_mode == 0u || all(material.reflection_strength == vec3<f32>(0.0))) {
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
    return material.reflection_strength * combine * sample_color;
}

// Eye-to-surface extinction through authored fog boxes. Slab clipping keeps
// work local to intersecting rays; four fixed Gauss quadrature samples retain
// horizontal softness and ground-height fade without a volume marching pass.
fn fog_interval(eye: vec3<f32>, delta: vec3<f32>, lower: vec3<f32>, upper: vec3<f32>) -> vec2<f32> {
    var near = 0.0;
    var far = 1.0;
    for (var axis = 0u; axis < 3u; axis = axis + 1u) {
        if (abs(delta[axis]) <= 1.0e-6) {
            if (eye[axis] < lower[axis] || eye[axis] > upper[axis]) {
                return vec2<f32>(1.0, 0.0);
            }
        } else {
            let first = (lower[axis] - eye[axis]) / delta[axis];
            let last = (upper[axis] - eye[axis]) / delta[axis];
            near = max(near, min(first, last));
            far = min(far, max(first, last));
            if (far <= near) {
                return vec2<f32>(1.0, 0.0);
            }
        }
    }
    return vec2<f32>(near, far);
}

fn region_density(region: FogRegion, point: vec3<f32>) -> f32 {
    let lower = region.min_falloff.xyz;
    let upper = region.max_top.xyz;
    if (point.y < lower.y || point.y > upper.y) {
        return 0.0;
    }
    let horizontal = min(min(point.x - lower.x, upper.x - point.x), min(point.z - lower.z, upper.z - point.z));
    var edge = 0.0;
    if (horizontal > 0.0) {
        edge = clamp(horizontal / max(region.min_falloff.w, 1.0e-6), 0.0, 1.0);
    }
    let ground_y = region.ground_pad.x;
    let top_y = region.max_top.w;
    var vertical = 0.0;
    if (point.y <= ground_y) {
        vertical = 1.0;
    } else if (top_y > ground_y && point.y < top_y) {
        vertical = clamp((top_y - point.y) / (top_y - ground_y), 0.0, 1.0);
    }
    return region.color_density.a * edge * vertical;
}

fn atmosphere(world_position: vec3<f32>) -> vec4<f32> {
    if (environment.fog_density == 0.0 && environment.fog_region_count == 0u) {
        return vec4<f32>(0.0);
    }
    let delta = world_position - camera.position;
    let distance = length(delta);
    let below = max(0.0, environment.fog_reference_y - world_position.y);
    let global_density = environment.fog_density * (1.0 + environment.fog_height_gain * min(below, 12.0));
    let global_depth = global_density * distance;
    var layer_depth = 0.0;
    var layer_color = environment.fog_color;
    let quadrature = array<vec2<f32>, 4u>(
        vec2<f32>(0.06943184, 0.17392743), vec2<f32>(0.3300095, 0.32607257),
        vec2<f32>(0.66999054, 0.32607257), vec2<f32>(0.93056816, 0.17392743)
    );
    let region_count = min(environment.fog_region_count, 16u);
    for (var index = 0u; index < region_count; index = index + 1u) {
        let region = environment.fog_regions[index];
        if (region.color_density.a <= 0.0) {
            continue;
        }
        let interval = fog_interval(camera.position, delta, region.min_falloff.xyz, region.max_top.xyz);
        if (interval.y <= interval.x) {
            continue;
        }
        var average = 0.0;
        for (var sample = 0u; sample < 4u; sample = sample + 1u) {
            let point = camera.position + delta * mix(interval.x, interval.y, quadrature[sample].x);
            average = average + region_density(region, point) * quadrature[sample].y;
        }
        let depth = average * distance * (interval.y - interval.x);
        // Preserve the strongest-volume overlap policy and stable tie order.
        if (depth > layer_depth) {
            layer_depth = depth;
            layer_color = region.color_density.rgb;
        }
    }
    let depth = global_depth + layer_depth;
    let color = mix(srgb_to_linear(environment.fog_color), srgb_to_linear(layer_color), layer_depth / max(depth, 1.0e-6));
    let amount = 1.0 - exp(-depth * depth);
    return vec4<f32>(color, clamp(amount, 0.0, 1.0));
}

fn fogged(color: vec3<f32>, world_position: vec3<f32>) -> vec3<f32> {
    let fog = atmosphere(world_position);
    return mix(color, fog.rgb, fog.a);
}

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
fn caustic_receiver_light(in: VsOut, selected: u32) -> vec4<f32> {
    let minimum = environment.caustics_receiver.bounds_min;
    let extent = environment.caustics_receiver.bounds_extent;
    if (selected == 0u || minimum.w <= 0.0) { return vec4<f32>(0.0); }
    if (extent.w != f32(caustics.nodes[selected - 1u].links.z)) { return vec4<f32>(0.0); }
    let coordinate = clamp((in.local_position - minimum.xyz) / max(extent.xyz, vec3<f32>(1.0e-6)), vec3<f32>(0.0), vec3<f32>(1.0));
    var radiance = vec4<f32>(0.0);
    for (var corner = 0u; corner < 8u; corner += 1u) {
        let offset = vec3<u32>(corner & 1u, (corner >> 1u) & 1u, (corner >> 2u) & 1u);
        let weight = select(vec3<f32>(1.0) - coordinate, coordinate, offset != vec3<u32>(0u));
        radiance += environment.caustics_receiver.radiance[corner] * weight.x * weight.y * weight.z;
    }
    // Anchors may sit above submerged static supports. A buried fragment must
    // keep an active zero override, rather than borrow either raised light or
    // the global field that includes this object's own dynamic shadow.
    if (in.world_position.y < radiance.w - 0.02) { return vec4<f32>(0.0, 0.0, 0.0, 1.0); }
    return vec4<f32>(radiance.rgb, 1.0);
}

fn shade(in: VsOut, front_facing: bool, cutout: bool) -> Shaded {
    // Derivatives stay outside volume-dependent flow control. Transform fluid
    // metres into this material's actual world UV frame, preserving its tiling.
    let uv_dx = dpdx(in.uv); let uv_dy = dpdy(in.uv);
    let pos_dx = dpdx(in.world_position.xz); let pos_dy = dpdy(in.world_position.xz);
    var uv = in.uv;
    if (in.water_surface > 0.5) {
        let selected = water_surface_node(in.world_position);
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
    let normals = surface_normals(in, front_facing);
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
    let light = baked + attached + beam;
    let view = normalize(camera.position - in.world_position);
    let incident = baked + attached + beam;
    let sheen = surface_sheen(in, normal, view, incident);
    let reflection = surface_reflection(in, normal, view);
    let emission = surface_emission(in, base_linear);
    let lit = lit_linear(base_linear * material.albedo_tint.rgb, in.color.rgb, light, material.emission_vertex);
    let focused = lit_linear(base_linear * material.albedo_tint.rgb, in.caustics_albedo, modulation * movable_contact_visibility(in, normal), material.emission_vertex);
    let pigment = base_linear * material.uv_tint.rgb;
    let fluorescence = ultraviolet_response(in, normal, pigment);
    let color = fogged(max(lit + focused, vec3<f32>(0.0)) + sheen + reflection + emission + fluorescence, in.world_position);
    var out: Shaded;
    out.color = storm_fog(camera.position, in.world_position, color);
    out.alpha = alpha;
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
    return vec4<f32>(surface_emission(in, base.rgb) * transmission, alpha);
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
