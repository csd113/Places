override display_target: bool = false;
const POSED_DIRECT_ENABLED:bool = true;
const POSED_RESIDUAL_ENABLED:bool = true;
const POSED_FALLBACK_ENABLED:bool = true;

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
    // Explicit tail padding so the struct starts with 80 bytes, 16-byte aligned.
    _padding: f32,
    // World-space capture half-space; zero for ordinary/probe/UI views.
    clip_plane: vec4<f32>,
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
    // `specular Ã— authored strength`, the reference's `u_reflect_strength`.
    reflection_strength: vec3<f32>,
    // 0 none, 1 probe, 2 planar; the reference's `u_reflect_mode`.
    reflection_mode: u32,
    // `emissive Ã— intensity`, the reference's `u_emission_color`.
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
    // where `EnvironmentUniform::tail_padding` sits. _tail0 selects the posed accessibility base.
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
    receiver_plane: vec4<f32>,
};

const MATERIAL_FLAG_NORMAL_ENABLED: u32 = 1u;
const MATERIAL_FLAG_RESPONSE_ENABLED: u32 = 2u;

@group(4) @binding(7) var<storage,read> posed_ambient_values:array<vec4<f32>>;
struct PosedDepthSample { projection:mat4x4<f32>, inverse:mat4x4<f32>, parameters:vec4<u32> };
@group(4) @binding(8) var posed_depth:texture_depth_2d_array;
@group(4) @binding(9) var<storage,read> posed_depth_samples:array<PosedDepthSample>;


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
@group(3) @binding(2)
var<uniform> lightmap_pair_layers: array<vec4<u32>, 50>;

// Logical page/group records remain untouched; only immutable physical storage aliases.
fn lightmap_layer(logical_layer: u32) -> u32 {
    let pair = logical_layer / 2u;
    return lightmap_pair_layers[pair / 4u][pair % 4u];
}
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
    @location(12) @interpolate(flat) water_node_hint: u32,
    @location(15) self_ambient: vec4<f32>,
};

fn world_vertex(vertex: WorldVertex, vertex_index: u32) -> VsOut {
    let world = environment.model * vec4<f32>(vertex.position, 1.0);
    var out: VsOut;
    out.clip = camera.view_projection * world;
    out.self_ambient = vec4<f32>(0.0);
    if (environment._tail0 > 0.0) {
        let ambient_index = u32(environment._tail0 - 1.0) + vertex_index;
        if (ambient_index < arrayLength(&posed_ambient_values)) { out.self_ambient = posed_ambient_values[ambient_index]; }
    }
    out.uv = vertex.uv;
    out.color = vertex.color;
    out.uv_visibility = vertex.uv_visibility;
    out.caustics_albedo = vertex.caustics_albedo;
    out.water_surface = min(vertex.water_surface, 1.0);
    out.water_node_hint = u32(max(vertex.water_surface - 1.0, 0.0));
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

@vertex
fn vs_main(vertex: WorldVertex, @builtin(vertex_index) vertex_index: u32) -> VsOut { return world_vertex(vertex, vertex_index); }

// Decode authored fog/storm display colours before linear mixing.
fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let low = c / 12.92;
    let high = pow((c + 0.055) / 1.055, vec3<f32>(2.4));
    return select(high, low, c <= vec3<f32>(0.04045));
}
