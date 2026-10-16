//! The static world: Places geometry as a wgpu mesh and pipeline, textured with
//! the base-colour path, the material system and the baked lighting.
//!
//! This module uploads the renderer-neutral [`LevelMesh`] the engine builds.
//! Its draw set is every Floor, Ceiling, Wall, fixture Light and placeholder
//! `PropFallback` range — ramps, stairs, half walls, columns, archways,
//! guardrails, thresholds and baseboards all arrive as one of the three
//! architectural families — packed into 16-bit-indexable vertex/index buffer
//! pairs; decals and resolved props are drawn by their own modules. One draw is
//! issued per packed range, with the same per-range frustum test the reference
//! applied.
//!
//! Each range's [`MaterialIndex`] resolves to one base texture:
//!
//! * the draw set includes the material-defined cut-out and translucent
//!   architectural ranges alongside the opaque ones;
//! * each draw carries the range's [`SurfaceShine`] override and its
//!   [`BatchPass`]; the neutral [`resolve_surface_material`] folds the material
//!   table, the override and the quality level into one
//!   [`ResolvedSurfaceMaterial`], which the [`WorldMaterials`] cache turns into
//!   one GPU uniform and bind group per distinct identity;
//! * normal maps are interned through the [`TextureCache`] under the
//!   `DataLinear` semantic, with the reference's white fallback and fetch gate;
//! * opaque, cut-out and translucent draws are submitted in the reference's
//!   order, translucent draws sorted back to front per frame;
//! * the WGSL evaluates material and light in linear HDR.
//!
//! Lighting is the reference's:
//!
//! * the vertex-lit build stores the material factor multiplied by the baked
//!   light in the vertex colour, so the uploaded colour carries every room
//!   baseline, fixture pool, opening blend and static-occluder shadow the bake
//!   produced; the lightmap-atlas build instead reconstructs the light from the
//!   prepared HDR layer pairs at the material normal and leaves
//!   the vertex colour at the unlit material factor;
//! * the camera uniform carries the world-space eye, the fragment stage
//!   computes the reference's view-dependent sheen, and `lit + sheen` is
//!   accumulated in linear HDR before the presentation stage. See `docs/RENDERER.md`.
//!
//! Props, dynamics, fixture emission, decals, the reflection captures and the
//! planar mirror extend the same upload and shader; see the world shader's own
//! fragment assembly.
//!
//! Coordinate convention (one place, documented, tested):
//!
//! ```text
//! Places camera  ->  RenderCamera::view_projection  (glam, RH, GL clip z -1..1)
//!                ->  * CLIP_CORRECTION              (z -> 0.5 z + 0.5)
//!                ->  camera uniform (mat4x4<f32>)   (column-major, WGSL)
//!                ->  vertex shader position only
//! ```
//!
//! No other sign flips, no vertex-stage Y negation, no reversed winding: the
//! world geometry is already wound so that its front face is the side its
//! normal points to (right-hand rule over `p0 -> p1 -> p2`), which is wgpu's
//! `FrontFace::Ccw` for a right-handed projection. Architectural ranges remain
//! two-sided, including opaque panes and void shells.
//! Opaque entity primitives honor glTF sidedness when their transform path is
//! safe; reflected placements reverse the front-face convention. The shader
//! flips normals for intentional two-sided back faces.

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec4};
use wgpu::util::DeviceExt;

use super::material::WorldMaterials;
use super::surface::DEPTH_FORMAT;
use super::texture::{
    CacheOutcome, GpuTexture, TextureCache, TextureFiltering, TextureKey, TextureSemantic,
    TextureUploadRequest,
};
use crate::engine::materials::{MaterialTable, ResolvedTexture, TextureOrigin};
use crate::engine::quality::QualityLevel;
use crate::engine::render::RenderCamera;
use crate::engine::render::common::materials::{
    BatchPass, MaterialRenderState, batch_pass_for, resolve_surface_material,
};
use crate::engine::render::common::mesh::{
    LevelMesh, LevelMeshRange, MaterialIndex, SurfaceKey, SurfaceKind, SurfaceShine, Vertex,
};
use crate::engine::render::common::view::DrawableSize;
use crate::engine::render::common::{DynamicLightSet, MAX_DYNAMIC_LIGHTS};
use crate::engine::render::common::{MeshChunk, MeshPacker};
use crate::engine::spatial::{Aabb, Frustum};

/// The world shader, from the file next to this module.
pub const WORLD_SHADER_SRC: &str = concat!(
    include_str!("storm.wgsl"),
    "\n",
    include_str!("attached_lights.wgsl"),
    "\n",
    include_str!("world.wgsl"),
    "\n",
    include_str!("caustics.wgsl")
);

/// Name of the world vertex entry point.
pub const WORLD_VERTEX_ENTRY: &str = "vs_main";
/// Name of the opaque and translucent world fragment entry point for the sRGB
/// surface (the direct path).
pub const WORLD_FRAGMENT_ENTRY: &str = "fs_main";
/// Name of the alpha-tested world fragment entry point for the sRGB surface.
pub const WORLD_CUTOUT_FRAGMENT_ENTRY: &str = "fs_cutout";
/// Name of the opaque and translucent entry point for a raw (non-sRGB) scene or
/// reflection target, which writes linear HDR values.
pub const WORLD_FRAGMENT_ENTRY_RAW: &str = "fs_main_raw";
/// Name of the alpha-tested entry point for a raw target.
pub const WORLD_CUTOUT_FRAGMENT_ENTRY_RAW: &str = "fs_cutout_raw";
/// Name of the emissive-pass opaque fragment entry point.
pub const WORLD_EMISSION_FRAGMENT_ENTRY: &str = "fs_emission";
/// Name of the emissive-pass alpha-tested fragment entry point.
pub const WORLD_EMISSION_CUTOUT_FRAGMENT_ENTRY: &str = "fs_emission_cutout";

/// Vertex attribute locations of [`WorldVertex`]. Kept as named constants so
/// the WGSL, the layout and the tests name the same numbers.
pub const WORLD_ATTRIB_POSITION: u32 = 0;
pub const WORLD_ATTRIB_NORMAL: u32 = 1;
pub const WORLD_ATTRIB_UV: u32 = 2;
pub const WORLD_ATTRIB_COLOR: u32 = 3;
pub const WORLD_ATTRIB_TANGENT: u32 = 4;
pub const WORLD_ATTRIB_HANDEDNESS: u32 = 5;
/// Lightmap atlas UV, the reference's `a_lightmap_uv` (two normalized u16).
pub const WORLD_ATTRIB_LIGHTMAP_UV: u32 = 6;
/// Lightmap page byte as a float, the reference's `a_lightmap_page`.
pub const WORLD_ATTRIB_LIGHTMAP_PAGE: u32 = 7;
pub const WORLD_ATTRIB_UV_VISIBILITY: u32 = 8;

/// One GPU world vertex: the material frame, the lit colour and the lightmap
/// coordinates.
///
/// The faithful carry of the renderer-neutral `Vertex` fields the material and
/// lighting stages consume: world position, geometric frame (normal, UV-u
/// tangent and bitangent sign), world-space tiling UV, the material/vertex-lit
/// colour, and the lightmap atlas address.
///
/// The colour is uploaded as `Float32x4`, preserving its linear RGB factor and
/// straight alpha. In the atlas build it is the material factor
/// (`tint × directional face shade`) with no baked light; in the vertex-lit
/// build it also carries HDR bake values without byte quantisation or clipping.
///
/// The lightmap pair matches the reference's *exact* attribute types in both of
/// its vertex layouts: `lightmap_uv` is the vertex's two 16-bit fixed-point
/// coordinates, uploaded as `Unorm16x2` so the hardware expands them to
/// `[0, 1]` exactly as `glVertexAttribPointer(..., GL_UNSIGNED_SHORT,
/// normalized=true)` does, and `lightmap_page` is the plain page byte carried
/// as a float (`0..=254` select the prepared page pair, `255` is
/// `LIGHTMAP_NONE`) exactly as the un-normalized `GL_UNSIGNED_BYTE` attribute
/// reached the reference shader.
///
/// `#[repr(C)]` + `Pod` make the 88-byte GPU stride explicit. Visibility is
/// appended here, leaving the prepared neutral vertex format unchanged;
/// the unit tests pin every offset and the vertex-buffer layout.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct WorldVertex {
    /// World-space position in metres.
    pub position: [f32; 3],
    /// Unit geometric surface normal (right-hand rule over the triangle).
    pub normal: [f32; 3],
    /// World-space tiling UV, one repeat per material tiling period.
    pub uv: [f32; 2],
    /// Linear RGBA factor, including HDR lighting in the vertex-lit fallback.
    pub color: [f32; 4],
    /// Lightmap atlas UV in 16-bit fixed point, or zero for an unlightmapped
    /// vertex (whose page byte is `LIGHTMAP_NONE`).
    pub lightmap_uv: [u16; 2],
    /// The lightmap page byte as a float; [`LIGHTMAP_NONE`] when unlightmapped.
    pub lightmap_page: f32,
    /// Unit UV-u tangent, orthogonalised against the normal.
    pub tangent: [f32; 3],
    /// Bitangent sign: `cross(normal, tangent) * handedness` is UV-v.
    pub handedness: f32,
    /// Four immutable static UV visibility fractions; moving objects use anchors.
    pub uv_visibility: [f32; 4],
}

impl From<&Vertex> for WorldVertex {
    fn from(vertex: &Vertex) -> Self {
        Self {
            position: vertex.pos,
            normal: vertex.normal,
            uv: vertex.uv,
            color: vertex.color,
            lightmap_uv: vertex.lightmap,
            lightmap_page: f32::from(vertex.lightmap_page),
            tangent: vertex.tangent,
            handedness: vertex.handedness,
            uv_visibility: [0.0; 4],
        }
    }
}

impl From<Vertex> for WorldVertex {
    fn from(vertex: Vertex) -> Self {
        Self::from(&vertex)
    }
}

const WORLD_VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 9] = wgpu::vertex_attr_array![
    WORLD_ATTRIB_POSITION => Float32x3,
    WORLD_ATTRIB_NORMAL => Float32x3,
    WORLD_ATTRIB_UV => Float32x2,
    WORLD_ATTRIB_COLOR => Float32x4,
    WORLD_ATTRIB_LIGHTMAP_UV => Unorm16x2,
    WORLD_ATTRIB_LIGHTMAP_PAGE => Float32,
    WORLD_ATTRIB_TANGENT => Float32x3,
    WORLD_ATTRIB_HANDEDNESS => Float32,
    WORLD_ATTRIB_UV_VISIBILITY => Float32x4,
];

/// Bytes between consecutive world vertices.
pub const WORLD_VERTEX_STRIDE: u64 = super::buffer_element_bytes::<WorldVertex>();

/// The explicit world vertex buffer layout.
///
/// One interleaved buffer: position, normal, UV, in that byte order and
/// 4-byte-granular offsets (12, 24, 32), which every native backend accepts.
#[must_use]
pub const fn world_vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout {
        array_stride: WORLD_VERTEX_STRIDE,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &WORLD_VERTEX_ATTRIBUTES,
    }
}

/// The one clip-space conversion from the Places/OpenGL convention to wgpu.
///
/// Places builds its view-projection with `glam::camera::rh::proj::opengl::perspective`, so a
/// view-space point maps to OpenGL clip space with `z_ndc` in `[-1, 1]`
/// (near `-1`, far `+1`). wgpu (like Vulkan, Metal and Direct3D 12) rasterises
/// `z` in `[0, 1]` (near `0`, far `1`), so this matrix maps
/// `z' = 0.5 z + 0.5`, `w' = w`, leaving `x` and `y` untouched:
///
/// * the depth range is converted without changing the near/far planes;
/// * the Y axis is *not* negated. WebGPU NDC has +Y up and its framebuffer
///   Y-flip is the viewport transform's job, exactly like OpenGL's; negating Y
///   here would mirror the world;
/// * winding is *not* reversed. WebGPU classifies a triangle's orientation in
///   framebuffer coordinates with the same handedness result OpenGL's
///   `glFrontFace(GL_CCW)` gives for a right-handed projection.
///
/// This is the only OpenGL -> wgpu correction in the world path.
#[must_use]
pub const fn clip_correction() -> Mat4 {
    Mat4::from_cols(
        Vec4::new(1.0, 0.0, 0.0, 0.0),
        Vec4::new(0.0, 1.0, 0.0, 0.0),
        Vec4::new(0.0, 0.0, 0.5, 0.0),
        Vec4::new(0.0, 0.0, 0.5, 1.0),
    )
}

/// One frame's world-space matrices, in the spaces the GPU path consumes.
#[derive(Clone, Copy, Debug)]
pub struct WorldFrame {
    /// View-projection in wgpu clip space (`z` in `[0, 1]`).
    pub view_projection: Mat4,
    /// Frustum extracted with the Places depth convention. The near/far planes
    /// are view-space distances, so the clip-space conversion does not affect
    /// what it culls.
    pub frustum: Frustum,
    /// Eye position in world metres, for the translucent pass's back-to-front
    /// sort (the reference sorts by squared distance from the body camera).
    pub eye: glam::Vec3,
}

/// Prepares the frame's matrices from the shared Places camera.
///
/// The camera and the aspect handling are renderer-neutral
/// ([`RenderCamera::view_projection`]); this function adds only the clip-space
/// correction.
#[must_use]
pub fn prepare_world_frame(camera: RenderCamera, render_size: DrawableSize) -> WorldFrame {
    let (view_projection, frustum) = camera.view_projection(render_size);
    // `glam` matrix products are per-element `f32` arithmetic with no overflow
    // or panic path.
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "`glam` matrix products are per-element `f32` arithmetic with no integer overflow or panic path."
    )]
    let corrected = clip_correction() * view_projection;
    WorldFrame {
        view_projection: corrected,
        frustum,
        eye: camera.position,
    }
}

/// The frame camera uniform: the corrected view-projection and the eye.
///
/// WGSL layout, pinned by tests:
///
/// ```text
/// offset  0  view_projection  mat4x4<f32>  64 bytes
/// offset 64  position         vec3<f32>    12 bytes
/// offset 76  diagnostic_selector f32 (WGSL `_padding`)
/// ------------------------------------------------ 80 bytes, align 16
/// ```
///
/// The eye position completes the reference's sheen term: it computes its view
/// vector from `u_camera_pos - v_world_pos`, and the world fragment stage needs
/// the same world-space camera point. `align(16)` makes the Rust layout
/// the WGSL uniform layout explicitly instead of relying on the default
/// alignment of an `[[f32; 4]; 4]` array.
#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct CameraUniform {
    /// Column-major view-projection, as WGSL's `mat4x4<f32>` expects.
    pub view_projection: [[f32; 4]; 4],
    /// World-space eye position, the reference's `u_camera_pos`.
    pub position: [f32; 3],
    /// Reserved alignment lane; the development shader consumes it as a
    /// selector. Production WGSL retains the `_padding` spelling and zero.
    pub diagnostic_selector: f32,
}

impl CameraUniform {
    /// The uniform value for one prepared frame: matrix and eye.
    #[must_use]
    pub const fn new(matrix: Mat4, eye: glam::Vec3) -> Self {
        Self {
            view_projection: matrix.to_cols_array_2d(),
            position: [eye.x, eye.y, eye.z],
            diagnostic_selector: 0.0,
        }
    }

    /// Uses the otherwise unread padding lane for the development selector.
    #[cfg(feature = "visual-diagnostics")]
    #[must_use]
    fn with_visual_diagnostic(mut self, mode: super::diagnostics::VisualDiagnosticMode) -> Self {
        self.diagnostic_selector = f32::from(mode.code());
        self
    }
}

/// Bytes one camera uniform occupies.
pub const CAMERA_UNIFORM_SIZE: u64 = super::buffer_element_bytes::<CameraUniform>();

/// True when the two uniforms carry the same bits for every field the shader
/// reads.
///
/// Bit comparison rather than `==` keeps the predicate exact (a one-ULP camera
/// change still updates the buffer). The development shader additionally reads
/// the selector in the padding lane; the production shader never reads it.
fn camera_uniform_same_bits(left: &CameraUniform, right: &CameraUniform) -> bool {
    let bits = |matrix: [[f32; 4]; 4]| matrix.map(|column| column.map(f32::to_bits));
    let camera_matches = bits(left.view_projection) == bits(right.view_projection)
        && left.position.map(f32::to_bits) == right.position.map(f32::to_bits);
    #[cfg(feature = "visual-diagnostics")]
    {
        camera_matches && left.diagnostic_selector.to_bits() == right.diagnostic_selector.to_bits()
    }
    #[cfg(not(feature = "visual-diagnostics"))]
    {
        camera_matches
    }
}

/// True when `next` must be written over `previous`.
///
/// The predicate compares the *value* the shader reads — the view-projection
/// and the eye — and the feature-only diagnostic selector. A still camera must
/// not touch the buffer, and any change to a consumed field must reach the shader
/// (the sheen is view-dependent, so an eye that
/// moved while the matrix rounding stayed equal still has to be uploaded).
#[must_use]
pub fn camera_uniform_changed(previous: Option<CameraUniform>, next: CameraUniform) -> bool {
    previous.is_none_or(|previous_uniform| !camera_uniform_same_bits(&previous_uniform, &next))
}

/// One regional fog volume, as the fragment shader reads it: 64 bytes.
///
/// WGSL layout, pinned by tests: `min_falloff` is the box minimum corner and
/// the horizontal soft edge in metres (`.w`); `max_top` the box maximum corner
/// and the world Y the layer fades to zero at (`.w`); `color_density` the
/// colour the layer mixes towards and its density per metre (`.a`);
/// `ground_pad.x` the world Y the full-density ground layer starts at. The
/// padding lanes are never read.
#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct FogRegionUniform {
    /// `xyz` the box minimum corner, `w` the horizontal falloff in metres.
    pub min_falloff: [f32; 4],
    /// `xyz` the box maximum corner, `w` the fade-to-zero world Y.
    pub max_top: [f32; 4],
    /// `rgb` the mix-to colour, `a` the density per metre.
    pub color_density: [f32; 4],
    /// `x` the ground layer base; `yzw` never read.
    pub ground_pad: [f32; 4],
}

impl FogRegionUniform {
    /// The all-zero value an unused slot carries.
    pub const ZERO: Self = Self {
        min_falloff: [0.0; 4],
        max_top: [0.0; 4],
        color_density: [0.0; 4],
        ground_pad: [0.0; 4],
    };

    /// Packs one resolved region.
    #[must_use]
    pub const fn new(region: &crate::engine::render::common::atmosphere::FogRegion) -> Self {
        Self {
            min_falloff: [
                region.min[0],
                region.min[1],
                region.min[2],
                region.falloff_m,
            ],
            max_top: [region.max[0], region.max[1], region.max[2], region.top_y],
            color_density: [
                region.color[0],
                region.color[1],
                region.color[2],
                region.density,
            ],
            ground_pad: [region.ground_y, 0.0, 0.0, 0.0],
        }
    }
}

/// The frame/level environment uniform: the baked-light switch and scale, the
/// lightmap selection, the fog constants and regional volumes, and the active
/// planar mirror's projection and plane.
///
/// WGSL layout, pinned by tests:
///
/// ```text
/// offset   0  light_scale        vec3<f32>    12 bytes
/// offset  12  lightmap_enabled   f32
/// offset  16  fog_color          vec3<f32>    12 bytes
/// offset  28  fog_density        f32
/// offset  32  fog_reference_y    f32
/// offset  36  fog_height_gain    f32
/// offset  40  lightmap_page_count     u32      4 bytes
/// offset  44  lightmap_switchable     u32      4 bytes
/// offset  48  planar_matrix      mat4x4<f32>  64 bytes
/// offset 112  planar_plane       vec4<f32>    16 bytes
/// offset 128  model              mat4x4<f32>  64 bytes
/// offset 192  opacity            f32
/// offset 196  tail_padding       vec3<f32>    12 bytes (never read)
/// offset 208  fog_region_count   u32          4 bytes
/// offset 212  fog_region_padding [u32; 3]     12 bytes (never read)
/// offset 224  fog_regions        [FogRegion; 16]  1024 bytes, 64 each
/// offset 1248 entity_irradiance   [f32; 4]     16 bytes
/// offset 1264 entity_moment       [f32; 4]     16 bytes
/// offset 1280 storm               Storm       1568 bytes
/// offset 2848 normal_model        [vec4; 3]       48 bytes
/// offset 2896 entity_bounds_min   vec4            16 bytes
/// offset 2912 entity_bounds_extent vec4           16 bytes
/// offset 2928 entity_anchors      [Anchor; 8]   1408 bytes
/// offset 4336 entity_direct       [Source; 8]    896 bytes
/// offset 5232 contact_shadows     [Shadow; 8]    384 bytes
/// offset 5616 contact_shadow_count vec4<u32>      16 bytes
/// offset 5632 blacklights          [Source; 4]    448 bytes
/// offset 6080 uv_receiver          Receiver       160 bytes
/// ------------------------------------------------------ 6240 bytes, align 16
/// ```
///
/// Every field is the reference's own frame uniform: `u_light_scale`,
/// `u_lightmap_enabled`, the four fog uniforms, `u_planar_matrix` and
/// `u_planar_plane`; `model` is the dynamic path's per-object matrix, the two
/// lightmap words are the prepared HDR atlas's addressing — the pages in one
/// layer group, the switchable groups' count and live on/off mask, and the
/// resident probe chain's top mip level (`0..=7`, bits 16..=19) — and
/// `opacity` is the per-instance fade multiplier the character path installs
/// (`1.0` for the static world and every prop, so static output is unchanged);
/// the regional fog block is the level's authored volumes, bounded by the
/// live count word. The struct is 6240 bytes on the wire and in Rust
/// (`ENVIRONMENT_UNIFORM_SIZE`). `#[repr(C, align(16))]` makes the Rust layout
/// the WGSL uniform layout explicitly; the unit tests pin it.
#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct EnvironmentUniform {
    /// The reference's `u_light_scale`; `1` for static geometry.
    pub light_scale: [f32; 3],
    /// The reference's `u_lightmap_enabled`.
    pub lightmap_enabled: f32,
    /// The reference's `u_fog_color`.
    pub fog_color: [f32; 3],
    /// The reference's `u_fog_density`.
    pub fog_density: f32,
    /// The reference's `u_fog_reference_y`.
    pub fog_reference_y: f32,
    /// The reference's `u_fog_height_gain`.
    pub fog_height_gain: f32,
    /// Pages per lightmap layer group: `LevelLightmaps::pages.len()`.
    pub lightmap_page_count: u32,
    /// Packed switchable state: bits 0..=3 the group count, bits 8..=11 the
    /// enabled mask (bit `g` set when group `g` is on), bits 16..=19 the
    /// resident probe chain's top mip level (`0` while probes have one level).
    pub lightmap_switchable: u32,
    /// The reference's `u_planar_matrix`: the mirrored view-projection, or the
    /// identity when no plane is active.
    pub planar_matrix: [[f32; 4]; 4],
    /// The reference's `u_planar_plane`: `xyz` the unit normal, `w` the offset.
    /// `[0, 0, 1, 0]` when no plane is active.
    pub planar_plane: [f32; 4],
    /// The reference's `u_model`: the object transform. The identity for the
    /// static world and for props (whose vertices are already in world space);
    /// the moving transform for a dynamic object.
    pub model: [[f32; 4]; 4],
    /// Per-instance opacity multiplier, `0..=1`. `1.0` for the static world and
    /// for every draw without a fade; a character instance installs its
    /// clamped fade opacity here, which the shader multiplies into the fragment
    /// alpha. Straight-alpha scene and bloom blending apply that coverage once.
    pub opacity: f32,
    /// Explicit tail padding so the 16-byte alignment is part of the struct
    /// (WGSL pads a uniform struct to its alignment the same way) and the
    /// `Pod` derive sees no implicit padding. Never read.
    pub tail_padding: [f32; 3],
    /// Number of valid entries in [`Self::fog_regions`], `0..=MAX_FOG_REGIONS`.
    /// The quality preset decides how many authored regions are uploaded; the
    /// shader loop is bounded by this word, so a preset change is a uniform
    /// write with no rebuild.
    pub fog_region_count: u32,
    /// Explicit padding so the fixed region array starts at its 16-byte
    /// alignment without implicit padding. Never read.
    pub fog_region_padding: [u32; 3],
    /// The level's regional fog volumes, the live ones first in authoring
    /// order. Every unused slot is [`FogRegionUniform::ZERO`], so a shader that
    /// read past `fog_region_count` would add no fog.
    pub fog_regions: [FogRegionUniform; crate::engine::level::MAX_FOG_REGIONS],
    /// Linear entity irradiance; w enables the probe material path.
    pub entity_irradiance: [f32; 4],
    /// Signed linear first moment; reserved w is zero.
    pub entity_moment: [f32; 4],
    pub storm: crate::engine::render::common::storm::StormUniform,
    /// Padded columns of the inverse-transpose model matrix.
    pub normal_model: [[f32; 4]; 3],
    /// Model-space interpolation bounds; minimum.w enables spatial lighting.
    pub entity_bounds_min: [f32; 4],
    pub entity_bounds_extent: [f32; 4],
    pub entity_anchors: [crate::engine::render::common::light_transport::EntityLightingAnchor; 8],
    pub entity_direct: [crate::engine::render::common::light_transport::EntityDirectLight; 8],
    pub contact_shadows: [crate::engine::render::common::dynamic_lights::ContactShadowUniform; 8],
    pub contact_shadow_count: [u32; 4],
    pub blacklights: [crate::engine::render::common::light_transport::EntityDirectLight; 4],
    pub uv_receiver: crate::engine::render::common::blacklight::BlacklightReceiver,
}

impl EnvironmentUniform {
    /// The static-world value: unit light scale, the atlas switch, the level's
    /// fog, no active mirror and the identity model.
    ///
    /// The lightmap selection starts empty; [`Self::with_lightmaps`] installs
    /// the resident pages and switchable groups.
    /// Adds local ultraviolet sources without changing any visible light field.
    #[must_use]
    pub fn with_blacklights(
        mut self,
        sources: &crate::engine::render::common::blacklight::BlacklightSet,
        lighting: Option<&crate::engine::lighting::LevelLighting>,
    ) -> Self {
        self.blacklights = sources.descriptors(lighting);
        self
    }

    #[must_use]
    pub const fn new(
        light_scale: [f32; 3],
        lightmap_enabled: bool,
        fog: crate::engine::render::common::atmosphere::FogState,
    ) -> Self {
        Self {
            light_scale,
            lightmap_enabled: if lightmap_enabled { 1.0 } else { 0.0 },
            fog_color: fog.color,
            fog_density: fog.density,
            fog_reference_y: fog.reference_y,
            fog_height_gain: fog.height_gain,
            lightmap_page_count: 0,
            lightmap_switchable: 0,
            planar_matrix: Mat4::IDENTITY.to_cols_array_2d(),
            planar_plane: [0.0, 0.0, 1.0, 0.0],
            model: Mat4::IDENTITY.to_cols_array_2d(),
            normal_model: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
            ],
            opacity: 1.0,
            tail_padding: [0.0; 3],
            fog_region_count: 0,
            fog_region_padding: [0; 3],
            fog_regions: [FogRegionUniform::ZERO; crate::engine::level::MAX_FOG_REGIONS],
            entity_irradiance: [0.0; 4],
            entity_moment: [0.0; 4],
            entity_bounds_min: [0.0; 4],
            entity_bounds_extent: [0.0; 4],
            entity_anchors:
                [crate::engine::render::common::light_transport::EntityLightingAnchor::ZERO; 8],
            entity_direct: [crate::engine::render::common::light_transport::EntityDirectLight::ZERO;
                8],
            contact_shadows:
                [crate::engine::render::common::dynamic_lights::ContactShadowUniform::ZERO; 8],
            contact_shadow_count: [0; 4],
            blacklights: [crate::engine::render::common::light_transport::EntityDirectLight::ZERO;
                4],
            uv_receiver: crate::engine::render::common::blacklight::BlacklightReceiver::ZERO,
            storm: crate::engine::render::common::storm::StormUniform {
                color_density: [0.0; 4],
                count: [0; 4],
                shelters: [crate::engine::render::common::storm::StormShelter {
                    min: [0.0; 4],
                    max: [0.0; 4],
                    roof: [0.0; 4],
                }; crate::engine::weather::MAX_STORM_SHELTERS],
            },
        }
    }

    /// Installs the same environmental payload for rigid and animated models.
    #[must_use]
    pub fn with_entity_lighting(
        mut self,
        sample: Option<crate::engine::render::common::light_transport::EntityLighting>,
    ) -> Self {
        self.entity_irradiance = [0.0, 0.0, 0.0, -1.0];
        self.entity_moment = [0.0; 4];
        self.light_scale =
            sample.map_or([crate::engine::lighting::AMBIENT_LEVEL; 3], |s| s.display);
        if let Some(texel) = sample.and_then(|s| s.prepared) {
            self.light_scale = [1.0; 3];
            let [red, green, blue] = texel.irradiance;
            let [x, y, z] = texel.direction;
            self.entity_irradiance = [red, green, blue, 1.0];
            self.entity_moment = [x, y, z, 0.0];
        }
        self
    }

    /// Installs residual probes and selected practical lights together. A
    /// legacy/Low entity explicitly clears the previous spatial payload.
    #[must_use]
    pub const fn with_spatial_lighting(
        mut self,
        spatial: Option<&crate::engine::render::common::light_transport::EntitySpatialLighting>,
    ) -> Self {
        self.entity_bounds_min = [0.0; 4];
        self.entity_bounds_extent = [0.0; 4];
        self.entity_anchors =
            [crate::engine::render::common::light_transport::EntityLightingAnchor::ZERO; 8];
        self.entity_direct =
            [crate::engine::render::common::light_transport::EntityDirectLight::ZERO; 8];
        if let Some(value) = spatial {
            self.entity_bounds_min = value.bounds_min;
            self.entity_bounds_extent = value.bounds_extent;
            self.entity_anchors = value.anchors;
            self.entity_direct = value.direct;
        }
        self
    }

    /// Installs the frame's bounded opaque movable grounding subjects.
    #[must_use]
    pub fn with_contact_shadows(
        mut self,
        shadows: &[crate::engine::render::common::dynamic_lights::ContactShadowUniform],
    ) -> Self {
        self.contact_shadows =
            [crate::engine::render::common::dynamic_lights::ContactShadowUniform::ZERO; 8];
        let count = shadows.len().min(self.contact_shadows.len());
        self.contact_shadow_count = [u32::try_from(count).unwrap_or(0), 0, 0, 0];
        for (slot, shadow) in self.contact_shadows.iter_mut().zip(shadows) {
            *slot = *shadow;
        }
        self
    }

    /// The same environment with the level's regional fog installed.
    ///
    /// `regions` is the resolved authoring-order list and `cap` the quality
    /// preset's upload budget; only the first `min(len, cap)` entries reach the
    /// shader, the count word bounds the fragment loop, and every further slot
    /// is cleared. A level with no regions leaves the word at zero and the
    /// whole array zeroed, which is the historical environment bit for bit.
    #[must_use]
    pub fn with_fog_regions(
        mut self,
        regions: &[crate::engine::render::common::atmosphere::FogRegion],
        cap: usize,
    ) -> Self {
        self.fog_regions = [FogRegionUniform::ZERO; crate::engine::level::MAX_FOG_REGIONS];
        let count = regions
            .len()
            .min(cap)
            .min(crate::engine::level::MAX_FOG_REGIONS);
        self.fog_region_count = u32::try_from(count).unwrap_or(u32::MAX);
        for (slot, region) in self.fog_regions.iter_mut().zip(regions.iter().take(count)) {
            *slot = FogRegionUniform::new(region);
        }
        self
    }

    /// The same environment with the resident lightmap selection installed.
    ///
    /// `page_count` is the pages in every layer group (base and switchable);
    /// `switchable_count` and `switchable_mask` are packed into the one
    /// switchable word at the four bits the shader reads, so a count beyond the
    /// atlas's representable groups can never make the shader loop past the
    /// layers it holds.
    #[must_use]
    pub const fn with_lightmaps(
        mut self,
        page_count: u32,
        switchable_count: u32,
        switchable_mask: u32,
    ) -> Self {
        self.lightmap_page_count = page_count;
        self.lightmap_switchable =
            (switchable_count & 0x000F) | ((switchable_mask & 0x000F) << 8_i32);
        self
    }

    /// The same environment with the resident probe chain's top mip level
    /// installed (bits 16..=19 of the switchable word).
    ///
    /// Zero is the live-capture case: the probes carry only level 0 and the
    /// shader keeps its two-tap rough fallback. A packaged chain passes its
    /// highest level, so `roughness * max_mip` selects a prefiltered level.
    #[must_use]
    pub const fn with_probe_mips(mut self, max_mip: u32) -> Self {
        self.lightmap_switchable =
            (self.lightmap_switchable & 0x0000_FFFF) | ((max_mip & 0x000F) << 16_i32);
        self
    }

    /// The same environment with one frame's active planar mirror installed.
    #[must_use]
    pub const fn with_planar(mut self, matrix: Mat4, plane: [f32; 4]) -> Self {
        self.planar_matrix = matrix.to_cols_array_2d();
        self.planar_plane = plane;
        self
    }

    /// The same environment with an object transform installed (dynamic path).
    #[must_use]
    pub fn with_model(mut self, model: Mat4) -> Self {
        self.model = model.to_cols_array_2d();
        let inverse = crate::engine::render::common::normal::inverse_transpose(model)
            .unwrap_or(Mat4::IDENTITY);
        let columns = inverse.to_cols_array_2d();
        let [x, y, z, _translation] = columns;
        self.normal_model = [x, y, z];
        self
    }

    /// The same environment with a per-instance opacity multiplier installed
    /// (the character path's fade).
    ///
    /// The value is clamped to `0..=1`; a non-finite value is treated as fully
    /// opaque so a malformed fade can never blank a draw or poison the uniform.
    /// `1.0` — the static world's value — leaves every shader term unchanged.
    #[must_use]
    pub const fn with_opacity(mut self, opacity: f32) -> Self {
        self.opacity = if opacity.is_finite() {
            opacity.clamp(0.0, 1.0)
        } else {
            1.0
        };
        self
    }
}

/// Bytes one environment uniform occupies.
pub const ENVIRONMENT_UNIFORM_SIZE: u64 = super::buffer_element_bytes::<EnvironmentUniform>();

/// One GPU attached light: the WGSL `DynamicLight`, 32 bytes.
///
/// The neutral [`crate::engine::render::common::dynamic_lights::DynamicLight`] keeps a
/// key for replacement; the key never reaches the shader, so the GPU value
/// carries only the position, radius, colour and intensity.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct GpuDynamicLight {
    /// World-space position, in metres.
    pub position: [f32; 3],
    /// The falloff window's radius, in metres, strictly positive.
    pub radius: f32,
    /// Linear colour factor, each channel `0..=1`.
    pub color: [f32; 3],
    /// Additive intensity.
    pub intensity: f32,
}

/// The one shadowed attached spotlight, packed alongside the existing lights.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct GpuFlashlight {
    pub view_projection: [[f32; 4]; 4],
    pub position_range: [f32; 4],
    pub color_strength: [f32; 4],
    pub direction_inner: [f32; 4],
    pub outer_enabled: [f32; 4],
}

impl GpuFlashlight {
    #[must_use]
    pub fn new(candidate: Option<crate::engine::render::common::Flashlight>) -> Self {
        let Some(light) = candidate.filter(|value| value.is_well_formed()) else {
            return Self::zeroed();
        };
        let [red, green, blue] = light.color;
        Self {
            view_projection: super::flashlight::flashlight_frame(light)
                .view_projection
                .to_cols_array_2d(),
            position_range: [
                light.position.x,
                light.position.y,
                light.position.z,
                light.range,
            ],
            color_strength: [red, green, blue, light.intensity],
            direction_inner: [
                light.direction.x,
                light.direction.y,
                light.direction.z,
                light.inner_cos,
            ],
            outer_enabled: [light.outer_cos, 1.0, 0.0, 0.0],
        }
    }
}

/// The group-4 uniform: a live count, three padding words and the fixed
/// [`MAX_DYNAMIC_LIGHTS`]-entry light array.
///
/// WGSL layout, pinned by tests: `count: u32` then three pad words (16 bytes),
/// then `array<DynamicLight, 8>` at a 32-byte stride. Every unused slot is
/// zero, so a shader that read past `count` would add nothing.
#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct DynamicLightsUniform {
    /// Number of valid entries in `lights`, `0..=MAX_DYNAMIC_LIGHTS`.
    pub count: u32,
    /// Explicit padding so the array starts at offset 16 (the struct's align).
    pub padding: [u32; 3],
    /// The frame's attached lights, the live ones first in insertion order.
    pub lights: [GpuDynamicLight; MAX_DYNAMIC_LIGHTS],
    pub flashlight: GpuFlashlight,
}

impl DynamicLightsUniform {
    /// Packs one frame's light set; every unused slot stays zero.
    #[must_use]
    pub fn new(lights: &DynamicLightSet) -> Self {
        let mut uniform = Self::zeroed();
        let live = lights.lights();
        let count = live.len().min(MAX_DYNAMIC_LIGHTS);
        uniform.count = u32::try_from(count).unwrap_or(u32::MAX);
        for (slot, light) in uniform.lights.iter_mut().zip(live.iter()) {
            *slot = GpuDynamicLight {
                position: [light.position.x, light.position.y, light.position.z],
                radius: light.radius,
                color: light.color,
                intensity: light.intensity,
            };
        }
        uniform
    }
}

/// Bytes one dynamic-lights uniform occupies.
pub const DYNAMIC_LIGHTS_UNIFORM_SIZE: u64 = super::buffer_element_bytes::<DynamicLightsUniform>();

fn world_camera_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("places-wgpu-world-camera-layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            // The fragment stage reads `camera.position` for the sheen's
            // view vector, so the uniform is visible to both stages.
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: wgpu::BufferSize::new(CAMERA_UNIFORM_SIZE),
            },
            count: None,
        }],
    })
}

/// The group-4 bind group layout: the frame's attached-light uniform.
///
/// Created once per world pipeline rebuild and shared by every pipeline that
/// samples the world shader; the bind group below is rebuilt with it because a
/// buffer binding is tied to one device buffer.
#[must_use]
pub fn dynamic_lights_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("places-wgpu-world-lights-layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(DYNAMIC_LIGHTS_UNIFORM_SIZE),
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Depth,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                count: None,
            },
        ],
    })
}

/// The group-3 bind group layout: environment uniform, the lightmap page array
/// and its sampler, the probe cubemap, the planar mirror image and the
/// reflection sampler.
///
/// Created once per renderer and shared by every world pipeline rebuild, so a
/// changed surface format never invalidates an environment binding.
#[must_use]
pub fn environment_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    let texture =
        |binding: u32, dimension: wgpu::TextureViewDimension| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: dimension,
                multisampled: false,
            },
            count: None,
        };
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("places-wgpu-environment-layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                // The vertex stage reads `model`; the fragment stage reads the
                // light switch, scale, fog and planar mirror.
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(ENVIRONMENT_UNIFORM_SIZE),
                },
                count: None,
            },
            // One `texture_2d_array` of layer pairs: the vertex's page byte and
            // the uniform's group counts select the pair. Binding 2 is
            // intentionally absent (the second page texture used to live
            // there).
            texture(1, wgpu::TextureViewDimension::D2Array),
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            texture(4, wgpu::TextureViewDimension::Cube),
            texture(5, wgpu::TextureViewDimension::D2),
            wgpu::BindGroupLayoutEntry {
                binding: 7,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(64),
                },
                count: None,
            },
            texture(8, wgpu::TextureViewDimension::D2),
            wgpu::BindGroupLayoutEntry {
                binding: 9,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 6,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    })
}

/// Builds one group-3 bind group from the environment's textures and uniform.
///
/// Every input is a view the caller keeps alive: the lightmap page array, the
/// probe cubemap and the planar mirror image. Fallback views keep every binding
/// complete when a resource is absent; the shader's switches decide whether
/// they are read at all.
#[must_use]
#[expect(
    clippy::too_many_arguments,
    reason = "one group-3 binding assembler; every view is a distinct slot"
)] // one group-3 binding assembler; every view is a distinct slot
pub fn environment_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    uniform: &wgpu::Buffer,
    lightmap_pages: &wgpu::TextureView,
    lightmap_sampler: &wgpu::Sampler,
    probe: &wgpu::TextureView,
    planar: &wgpu::TextureView,
    reflection_sampler: &wgpu::Sampler,
    caustics: &super::caustics::Caustics,
    caustics_sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("places-wgpu-environment"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(lightmap_pages),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::Sampler(lightmap_sampler),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::TextureView(probe),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::TextureView(planar),
            },
            wgpu::BindGroupEntry {
                binding: 6,
                resource: wgpu::BindingResource::Sampler(reflection_sampler),
            },
            wgpu::BindGroupEntry {
                binding: 7,
                resource: caustics.buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 8,
                resource: wgpu::BindingResource::TextureView(caustics.texture.view()),
            },
            wgpu::BindGroupEntry {
                binding: 9,
                resource: wgpu::BindingResource::Sampler(caustics_sampler),
            },
        ],
    })
}

/// True when a renderer-neutral range belongs to the wgpu static world draw set.
///
/// The rule is the reference static world's own draw set: the architectural
/// families (floors, ceilings and walls, including every piece the neutral
/// builder emits under those kinds — ramps, stairs, half walls, columns,
/// archways, guardrails, thresholds, baseboards, reveals and window panes), the
/// fixture families (`Light`: luminous faces and housings) and the placeholder
/// boxes of props whose model could not be drawn (`PropFallback`).
/// `Decal` ranges are their own pass with their own depth bias and stay out.
///
/// Material-defined cut-out and translucent ranges are part of the set: their
/// pass is [`WorldDraw::pass`], and the pipelines differ, not the membership.
#[must_use]
pub const fn is_world_range(range: &LevelMeshRange) -> bool {
    matches!(
        range.key.kind,
        SurfaceKind::Floor
            | SurfaceKind::Ceiling
            | SurfaceKind::Wall
            | SurfaceKind::Light
            | SurfaceKind::PropFallback
    )
}

/// The pass one range draws in, from its key and the resolved material alphas.
#[must_use]
fn range_pass(range: &LevelMeshRange, materials: &MaterialRenderState) -> BatchPass {
    let alpha = materials
        .alphas
        .get(usize::try_from(range.key.material).unwrap_or(usize::MAX))
        .copied();
    batch_pass_for(range.key.kind, range.key.has_material(), alpha)
}

/// One 16-bit-indexable GPU buffer pair.
pub struct WorldChunk {
    /// Interleaved [`WorldVertex`] data.
    pub vertex_buffer: wgpu::Buffer,
    /// `u16` triangle indices into `vertex_buffer`.
    pub index_buffer: wgpu::Buffer,
    /// Distinct vertices the chunk holds.
    pub vertex_count: u32,
    /// Indices the chunk holds in total.
    pub index_count: u32,
}

/// One drawable range inside a chunk.
///
/// Groups the chunk plus its index range with the material-stage fields: the
/// range's [`MaterialIndex`], its per-surface shine override and the
/// [`BatchPass`] its alpha contract implies. Those are the same values the
/// neutral `SurfaceKey` already carries, not a complete material: the renderer
/// resolves them through the existing material table.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldDraw {
    /// Which [`WorldChunk`] the range lives in.
    pub chunk: usize,
    /// First index in the chunk's index buffer.
    pub index_start: u32,
    /// Indices the draw covers.
    pub index_count: u32,
    /// Distinct vertices the range indexes.
    pub vertex_count: u32,
    /// World-space bounds the CPU frustum test uses.
    pub bounds: Aabb,
    /// The surface family, kept for the load-time family breakdown.
    pub kind: SurfaceKind,
    /// The range's material key, resolved to the material at upload.
    ///
    /// [`crate::engine::render::MATERIAL_NONE`] for an architectural range without a
    /// level material; such a draw samples the shared fallback sheet with the
    /// plain material state.
    pub material: MaterialIndex,
    /// The per-surface shine override the range authoried, if any. Part of the
    /// material identity: two surfaces sharing a material with different shine
    /// resolve to different GPU materials.
    pub shine: Option<SurfaceShine>,
    /// Index of the renderer-neutral mesh range this draw came from.
    ///
    /// Keeps the range's reflection-plane identity available at material
    /// resolution, so one material used on two planes never collapses to a
    /// single GPU material state.
    pub range: usize,
    /// The material-defined pass this range draws in.
    pub pass: BatchPass,
}

/// What one upload produced, as plain counters.
///
/// The neutral mesh numbers are recorded alongside the uploaded ones so the
/// parity of the world draw set against the neutral preparation can be
/// asserted in tests and named in the one load-time log line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WorldGeometryStats {
    /// Distinct vertices in the renderer-neutral static mesh.
    pub mesh_vertices: usize,
    /// Indices in the renderer-neutral static mesh.
    pub mesh_indices: usize,
    /// Ranges in the renderer-neutral static mesh (every surface family).
    pub mesh_ranges: usize,
    /// Distinct vertices actually uploaded to the GPU (the world draw set).
    pub uploaded_vertices: usize,
    /// Indices actually uploaded to the GPU.
    pub uploaded_indices: usize,
    /// Draw ranges the upload produced.
    pub draws: usize,
    /// GPU buffer pairs the upload produced.
    pub chunks: usize,
    /// Bytes resident in world vertex buffers.
    pub vertex_bytes: usize,
    /// Bytes resident in world index buffers.
    pub index_bytes: usize,
}

/// Packs the world draw set of one renderer-neutral static mesh.
///
/// GPU-free so the draw set's coverage and counts are unit-testable: the
/// returned packer holds exactly the chunks the upload will turn into buffers,
/// and every [`WorldDraw`] addresses one placement of one selected range. The
/// neutral [`MeshPacker`] retains the reference chunking. Reactive architecture
/// alone gains a bounded visibility-sampling grid, interpolating existing
/// attributes without adding prepared charts; ordinary ranges remain unchanged.
/// An index is always 16-bit and a range can split across chunk buffers without
/// re-basing at draw time. An empty draw set produces no chunks or draws.
#[must_use]
pub fn pack_world_ranges(
    mesh: &LevelMesh,
    materials: &MaterialRenderState,
) -> (MeshPacker, Vec<WorldDraw>) {
    let mut packer = MeshPacker::default();
    let mut draws: Vec<WorldDraw> = Vec::new();
    for (range_index, range) in mesh.ranges.iter().enumerate() {
        if !is_world_range(range) {
            continue;
        }
        let pass = range_pass(range, materials);
        let reactive = matches!(
            range.key.kind,
            SurfaceKind::Floor | SurfaceKind::Ceiling | SurfaceKind::Wall
        ) && usize::try_from(range.key.material)
            .ok()
            .and_then(|slot| materials.responses.get(slot))
            .is_some_and(|response| response.fluorescence.strength > 0.0);
        if reactive {
            for &[first, next, end] in range.indices.as_chunks::<3>().0 {
                let (Some(a), Some(b), Some(c)) = (
                    range.vertices.get(usize::from(first)),
                    range.vertices.get(usize::from(next)),
                    range.vertices.get(usize::from(end)),
                ) else {
                    continue;
                };
                let (vertices, indices) = super::uv_receivers::subdivide_triangle([*a, *b, *c]);
                append_world_draws(
                    &mut draws,
                    packer.push(&vertices, &indices),
                    range,
                    range_index,
                    pass,
                );
            }
        } else {
            append_world_draws(
                &mut draws,
                packer.push(&range.vertices, &range.indices),
                range,
                range_index,
                pass,
            );
        }
    }
    (packer, draws)
}

/// Refinement keeps adjacent placements in one draw per original range/chunk.
fn append_world_draws(
    draws: &mut Vec<WorldDraw>,
    placements: Vec<crate::engine::render::common::mesh::PackedRange>,
    range: &LevelMeshRange,
    range_index: usize,
    pass: BatchPass,
) {
    for packed in placements {
        let index_start = u32::try_from(packed.index_start).unwrap_or(u32::MAX);
        let index_count = u32::try_from(packed.index_count).unwrap_or(u32::MAX);
        let vertex_count = u32::try_from(packed.vertex_count).unwrap_or(u32::MAX);
        if let Some(previous) = draws.last_mut()
            && previous.range == range_index
            && previous.chunk == packed.chunk
            && previous.index_start.saturating_add(previous.index_count) == index_start
        {
            previous.index_count = previous.index_count.saturating_add(index_count);
            previous.vertex_count = previous.vertex_count.saturating_add(vertex_count);
            continue;
        }
        draws.push(WorldDraw {
            chunk: packed.chunk,
            index_start,
            index_count,
            vertex_count,
            bounds: range.bounds,
            kind: range.key.kind,
            material: range.key.material,
            shine: range.key.shine,
            range: range_index,
            pass,
        });
    }
}

/// The static world mesh uploaded for the loaded level.
///
/// Persistent for the level's lifetime: one-time conversion at upload, never a
/// per-frame rebuild. Dropping it releases the GPU buffers.
pub struct WgpuWorldGeometry {
    chunks: Vec<WorldChunk>,
    draws: Vec<WorldDraw>,
    pass_indices: WorldPassIndices,
    stats: WorldGeometryStats,
}

/// Immutable classification shared by scene, emission and visibility draws.
/// Keep packed order within each class; only translucent submission sorts it.
#[derive(Default)]
struct WorldPassIndices {
    opaque: Vec<u32>,
    cutout: Vec<u32>,
    translucent: Vec<u32>,
}

impl WorldPassIndices {
    fn new(draws: &[WorldDraw]) -> Self {
        let mut indices = Self::default();
        for (index, draw) in draws.iter().enumerate() {
            let draw_index = u32::try_from(index).unwrap_or(u32::MAX);
            match draw.pass {
                BatchPass::Opaque => indices.opaque.push(draw_index),
                BatchPass::Cutout => indices.cutout.push(draw_index),
                BatchPass::Translucent => indices.translucent.push(draw_index),
            }
        }
        indices
    }

    fn for_pass(&self, pass: BatchPass) -> &[u32] {
        match pass {
            BatchPass::Opaque => &self.opaque,
            BatchPass::Cutout => &self.cutout,
            BatchPass::Translucent => &self.translucent,
        }
    }
}

impl WgpuWorldGeometry {
    /// Uploads the world draw set of one renderer-neutral static mesh.
    #[cfg(feature = "visual-diagnostics")]
    #[must_use]
    pub fn upload(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        mesh: &LevelMesh,
        materials: &MaterialRenderState,
    ) -> Self {
        Self::upload_with_blacklight(device, queue, mesh, materials, None)
    }

    #[must_use]
    pub fn upload_with_blacklight(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        mesh: &LevelMesh,
        materials: &MaterialRenderState,
        visibility: Option<crate::engine::render::common::blacklight::BlacklightVisibility<'_>>,
    ) -> Self {
        let (packer, draws) = pack_world_ranges(mesh, materials);
        crate::engine::perf::startup_mark("world geometry: pack");

        let mut chunks: Vec<WorldChunk> = Vec::with_capacity(packer.chunks.len());
        for chunk in &packer.chunks {
            chunks.push(upload_chunk(device, queue, chunk, visibility));
        }
        crate::engine::perf::startup_mark("world geometry: GPU upload");

        let uploaded_vertices: usize = chunks
            .iter()
            .map(|chunk| usize::try_from(chunk.vertex_count).unwrap_or(usize::MAX))
            .sum();
        let uploaded_indices: usize = chunks
            .iter()
            .map(|chunk| usize::try_from(chunk.index_count).unwrap_or(usize::MAX))
            .sum();
        let stats = WorldGeometryStats {
            mesh_vertices: mesh.vertex_count,
            mesh_indices: mesh.index_count,
            mesh_ranges: mesh.ranges.len(),
            uploaded_vertices,
            uploaded_indices,
            draws: draws.len(),
            chunks: chunks.len(),
            vertex_bytes: uploaded_vertices
                .saturating_mul(usize::try_from(WORLD_VERTEX_STRIDE).unwrap_or(usize::MAX)),
            index_bytes: uploaded_indices.saturating_mul(std::mem::size_of::<u16>()),
        };
        let pass_indices = WorldPassIndices::new(&draws);
        Self {
            chunks,
            draws,
            pass_indices,
            stats,
        }
    }

    /// The uploaded draw ranges, in draw order.
    #[must_use]
    pub fn draws(&self) -> &[WorldDraw] {
        &self.draws
    }

    /// The upload counters.
    #[must_use]
    pub const fn stats(&self) -> WorldGeometryStats {
        self.stats
    }

    /// Draw ranges per surface family, in [`SurfaceKind::ALL`] order.
    #[must_use]
    pub fn family_breakdown(&self) -> [usize; SurfaceKind::ALL.len()] {
        let mut breakdown = [0usize; SurfaceKind::ALL.len()];
        for draw in &self.draws {
            if let Some(slot) = breakdown.get_mut(draw.kind.index()) {
                *slot = slot.saturating_add(1);
            }
        }
        breakdown
    }
}

/// The base texture one world draw samples, or `None` for the fallback.
///
/// A thin view of the neutral [`resolve_surface_material`] result: the resolved
/// base-colour texture index, looked up in the existing neutral
/// [`MaterialTable`]. Fixtures, prop placeholders and decals are not part of the
/// world draw set; an empty material key (`MATERIAL_NONE`, which is
/// `u16::MAX`) resolves to the fallback.
#[must_use]
pub fn resolve_base_texture<'a>(
    draw: &WorldDraw,
    materials: &MaterialRenderState,
    table: &'a MaterialTable,
) -> Option<&'a ResolvedTexture> {
    let key = SurfaceKey::with_shine(draw.kind, draw.material, draw.shine);
    resolve_surface_material(key, materials, table, true)
        .texture
        .and_then(|slot| {
            table
                .textures()
                .get(usize::try_from(slot).unwrap_or(usize::MAX))
        })
}

/// What one level's world texture resolution did, as plain counters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WorldTextureStats {
    /// World draws the level holds.
    pub draws: usize,
    /// Draws that resolved a base texture.
    pub textured_draws: usize,
    /// Draws that sample the shared fallback sheet.
    pub fallback_draws: usize,
    /// Draws that sample a fixture family's luminous-face sheet.
    pub fixture_draws: usize,
    /// Distinct base textures the draw set uses.
    pub unique: usize,
    /// GPU uploads this level load performed.
    pub uploads: usize,
    /// Lookups the renderer's cache already held.
    pub cache_hits: usize,
    /// Base textures that are the diagnostic missing-texture pattern.
    pub missing: usize,
    /// Texel storage all sampled textures occupy, mip levels included.
    pub resident_bytes: usize,
    /// The longest level-0 edge among the sampled textures.
    pub max_edge: u32,
}

/// The per-draw GPU textures of one uploaded world.
///
/// Resolved once per level load, never per frame. Each draw maps to one entry;
/// every surface that shares a texture shares the entry (and therefore one GPU
/// upload and one bind group). The `Arc`s keep the GPU textures alive even when
/// the cache is cleared for a profile change, so a frame between the release
/// and the level rebuild can still draw.
pub struct WorldTextures {
    /// Distinct GPU textures the draw set uses, in first-use order.
    entries: Vec<Arc<GpuTexture>>,
    /// Entry index per [`WorldDraw`], in draw order.
    per_draw: Vec<usize>,
    stats: WorldTextureStats,
}

impl WorldTextures {
    /// Resolves every world draw to its base texture, uploading what the cache
    /// does not hold yet.
    ///
    /// Called from the level upload with the resolved material table the engine
    /// already owns. A draw without a resolvable base texture (an empty
    /// material key, a stale table, a placeholder box) samples the fallback
    /// sheet; a fixture draw samples its family's sheet from the already
    /// uploaded `fixture_sheets` list. Both are normal, deterministic outcomes,
    /// not errors.
    ///
    /// The distinct base textures are collected first, in first-use order, and
    /// their CPU preparation (fit + mip chain) runs in one parallel batch; the
    /// GPU uploads stay serial and on the caller's thread. The draw mapping,
    /// the entry order and every counter are identical to resolving one draw at
    /// a time.
    #[must_use]
    #[expect(
        clippy::too_many_arguments,
        reason = "one resolution pass over the level's draws and materials"
    )] // one resolution pass over the level's draws and materials
    pub fn resolve(
        cache: &mut TextureCache,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        draws: &[WorldDraw],
        materials: &MaterialRenderState,
        table: &MaterialTable,
        fixture_sheets: &[Arc<GpuTexture>],
        level: QualityLevel,
    ) -> Self {
        /// Which texture one draw samples during the mapping pass.
        enum DrawTexture {
            /// The fixture family's sheet slot, indexed by material.
            Fixture(usize),
            /// One entry of the batched base-texture request list.
            Base(usize),
            /// The shared fallback sheet.
            Fallback,
        }

        let mut stats = WorldTextureStats {
            draws: draws.len(),
            ..WorldTextureStats::default()
        };
        let mut requests: Vec<TextureUploadRequest<'_>> = Vec::new();
        let mut sources: Vec<DrawTexture> = Vec::with_capacity(draws.len());
        // The identity includes the lifetime, exactly like the cache's two maps:
        // the same key can legitimately be a catalog entry and a pack entry.
        let mut request_by_key: HashMap<(TextureKey, bool), usize> = HashMap::new();
        // Pass 1: classify every draw and collect the distinct base textures
        // (one request per semantic identity, in first-use order).
        for draw in draws {
            if draw.kind == SurfaceKind::Light
                && draw.material != crate::engine::render::common::mesh::MATERIAL_NONE
            {
                stats.fixture_draws = stats.fixture_draws.saturating_add(1);
                sources.push(DrawTexture::Fixture(
                    usize::try_from(draw.material).unwrap_or(usize::MAX),
                ));
                continue;
            }
            let Some(resolved) = resolve_base_texture(draw, materials, table) else {
                stats.fallback_draws = stats.fallback_draws.saturating_add(1);
                sources.push(DrawTexture::Fallback);
                continue;
            };
            stats.textured_draws = stats.textured_draws.saturating_add(1);
            if resolved.origin == TextureOrigin::Missing {
                stats.missing = stats.missing.saturating_add(1);
            }
            let key = TextureKey::new(resolved, TextureSemantic::BaseColorDisplay, level);
            let identity = (key.clone(), resolved.origin == TextureOrigin::Pack);
            let index = match request_by_key.entry(identity) {
                Entry::Occupied(entry) => *entry.get(),
                Entry::Vacant(entry) => {
                    let index = requests.len();
                    let _configured_insert = entry.insert(index);
                    requests.push(TextureUploadRequest {
                        key,
                        image: resolved.image.as_ref(),
                        origin: resolved.origin,
                    });
                    index
                }
            };
            sources.push(DrawTexture::Base(index));
        }
        crate::engine::perf::startup_mark("world textures: plan");

        // One batch: cache hits are served from the maps, the misses are
        // prepared in parallel and uploaded serially in first-use order.
        let resolved_textures = cache.get_or_upload_batch(device, queue, &requests);
        crate::engine::perf::startup_mark("world textures: batch");

        // Pass 2: map each draw to its entry. The counters come from the batch
        // results: every miss uploads exactly once (its first draw), so the
        // remaining textured draws are cache hits, exactly like the serial
        // loop's outcome accounting.
        stats.uploads = resolved_textures
            .iter()
            .filter(|entry| entry.0 == CacheOutcome::Uploaded)
            .count();
        stats.cache_hits = stats.textured_draws.saturating_sub(stats.uploads);
        let mut entries: Vec<Arc<GpuTexture>> = Vec::new();
        let mut per_draw: Vec<usize> = Vec::with_capacity(draws.len());
        for source in &sources {
            let texture = match source {
                DrawTexture::Fixture(index) => fixture_sheets
                    .get(*index)
                    .map_or_else(|| cache.fallback(), Arc::clone),
                DrawTexture::Base(index) => resolved_textures
                    .get(*index)
                    .map_or_else(|| cache.fallback(), |entry| Arc::clone(&entry.1)),
                DrawTexture::Fallback => cache.fallback(),
            };
            let slot = entry_slot(&entries, &texture).unwrap_or_else(|| {
                entries.push(texture);
                entries.len().saturating_sub(1)
            });
            per_draw.push(slot);
        }
        stats.unique = entries.iter().filter(|entry| !entry.is_fallback()).count();
        stats.resident_bytes = entries
            .iter()
            .map(|entry| usize::try_from(entry.meta().resident_bytes).unwrap_or(usize::MAX))
            .sum();
        stats.max_edge = entries
            .iter()
            .map(|entry| entry.meta().max_edge())
            .max()
            .unwrap_or(0);
        Self {
            entries,
            per_draw,
            stats,
        }
    }

    /// The entry index one [`WorldDraw`] samples.
    #[must_use]
    pub fn slot_for_draw(&self, draw_index: usize) -> Option<usize> {
        self.per_draw.get(draw_index).copied()
    }

    /// The GPU texture of one entry index.
    #[must_use]
    pub fn entry(&self, slot: usize) -> Option<&GpuTexture> {
        self.entries.get(slot).map(Arc::as_ref)
    }

    /// The resolution counters.
    #[must_use]
    pub const fn stats(&self) -> WorldTextureStats {
        self.stats
    }
}

/// The entry index of `texture` in `entries`, by shared allocation.
///
/// `Arc::ptr_eq` is exactly the right equality here: the cache hands out the
/// same allocation for the same semantic texture, so an entry is shared by
/// every draw that resolved the same identity.
fn entry_slot(entries: &[Arc<GpuTexture>], texture: &Arc<GpuTexture>) -> Option<usize> {
    entries.iter().position(|entry| Arc::ptr_eq(entry, texture))
}

/// Uploads 16-bit indices with wgpu's required four-byte copy padding. An odd
/// triangle count has an odd index count; padding storage must not add indices
/// to the draw. Mapped initialization avoids an unaligned queue write.
pub(super) fn upload_index_buffer(
    device: &wgpu::Device,
    indices: &[u16],
    label: &str,
) -> wgpu::Buffer {
    let contents = if indices.is_empty() {
        &[0_u16; 2]
    } else {
        indices
    };
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: bytemuck::cast_slice(contents),
        usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
    })
}

/// Uploads one packed chunk as a vertex/index buffer pair.
fn upload_chunk(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    chunk: &MeshChunk,
    visibility: Option<crate::engine::render::common::blacklight::BlacklightVisibility<'_>>,
) -> WorldChunk {
    let vertices: Vec<WorldVertex> = chunk
        .vertices
        .iter()
        .map(|vertex| {
            let mut gpu = WorldVertex::from(vertex);
            if let Some(context) = visibility {
                gpu.uv_visibility = context.vertex(vertex);
            }
            gpu
        })
        .collect();
    let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("places-wgpu-world-vertices"),
        size: u64::try_from(vertices.len())
            .unwrap_or(u64::MAX)
            .saturating_mul(WORLD_VERTEX_STRIDE)
            .max(4),
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    if !vertices.is_empty() {
        queue.write_buffer(&vertex_buffer, 0, bytemuck::cast_slice(&vertices));
    }

    let index_buffer = upload_index_buffer(device, &chunk.indices, "places-wgpu-world-indices");

    WorldChunk {
        vertex_buffer,
        index_buffer,
        vertex_count: u32::try_from(chunk.vertices.len()).unwrap_or(u32::MAX),
        index_count: u32::try_from(chunk.indices.len()).unwrap_or(u32::MAX),
    }
}

/// What one frame's world submission did, for the neutral [`RenderStats`]
/// and the benchmark report.
///
/// [`RenderStats`]: crate::engine::render::RenderStats
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WorldDrawTotals {
    /// `draw_indexed` calls issued.
    pub draw_calls: usize,
    /// Ranges that survived the frustum test.
    pub visible_batches: usize,
    /// Distinct vertices those ranges index.
    pub visible_vertices: usize,
    /// Index elements in the submitted triangle-list draws.
    pub visible_indices: usize,
    /// Texture bind-group changes applied (one per run of draws sharing a
    /// texture; zero for a world with no textures).
    pub texture_binds: usize,
    /// Material bind-group changes applied (one per run of draws sharing a
    /// resolved material state).
    pub material_binds: usize,
    /// Opaque draws submitted.
    pub opaque_draws: usize,
    /// Cut-out draws submitted.
    pub cutout_draws: usize,
    /// Translucent draws submitted.
    pub translucent_draws: usize,
    /// True when a draw whose material emits survived the cull; the bloom
    /// stages run only then, exactly like the reference's `emissive_visible`.
    pub emissive_visible: bool,
}

impl WorldDrawTotals {
    /// Count an object's indexed buffer only for an alpha class that submitted it.
    const fn record_object_vertices(&mut self, before_draws: usize, vertices: usize) {
        if self.draw_calls > before_draws {
            self.visible_vertices = self.visible_vertices.saturating_add(vertices);
        }
    }

    /// Folds another pass segment's counters in.
    ///
    /// The per-pass counters (`opaque_draws`, `cutout_draws`,
    /// `translucent_draws`) stay with the class that produced them; the
    /// splice-in paths accumulate only the totals.
    const fn absorb(&mut self, other: Self) {
        self.draw_calls = self.draw_calls.saturating_add(other.draw_calls);
        self.visible_batches = self.visible_batches.saturating_add(other.visible_batches);
        self.visible_vertices = self.visible_vertices.saturating_add(other.visible_vertices);
        self.visible_indices = self.visible_indices.saturating_add(other.visible_indices);
        self.texture_binds = self.texture_binds.saturating_add(other.texture_binds);
        self.material_binds = self.material_binds.saturating_add(other.material_binds);
        self.emissive_visible |= other.emissive_visible;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TransparentItem {
    World(u32),
    Prop(usize),
    Dynamic(usize),
    Character(usize),
}

fn sort_transparency(items: &mut [(TransparentItem, [f32; 3])], eye: glam::Vec3) {
    items.sort_by(|left, right| {
        let distance = |point| glam::Vec3::from_array(point).distance_squared(eye);
        distance(right.1).total_cmp(&distance(left.1))
    });
}

/// Entity raster policy; unknown transforms stay conservatively two-sided.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EntityCullMode {
    TwoSided,
    Regular,
    Reflected,
}

const fn entity_cull_mode(
    pass: BatchPass,
    double_sided: bool,
    reflected: Option<bool>,
) -> EntityCullMode {
    match (pass, double_sided, reflected) {
        (BatchPass::Opaque, false, Some(false)) => EntityCullMode::Regular,
        (BatchPass::Opaque, false, Some(true)) => EntityCullMode::Reflected,
        _ => EntityCullMode::TwoSided,
    }
}

/// Transform parity used by entity pipelines. Singular or invalid matrices
/// cannot establish a front side and retain the two-sided fallback.
pub(super) fn culling_reflected(model: Mat4) -> Option<bool> {
    let determinant = model.determinant();
    (model.is_finite() && determinant.is_finite() && determinant.abs() > 1.0e-8)
        .then_some(determinant < 0.0)
}

/// One world pipeline variant's distinguishing state.
///
/// The six material passes share the shader module, vertex layout, depth
/// format, winding and bind group layouts; only these fields differ.
#[derive(Clone, Copy)]
struct WorldPipelineVariant {
    label: &'static str,
    fragment_entry: &'static str,
    /// The same pass's entry point for a raw (non-sRGB) target.
    fragment_entry_raw: &'static str,
    blend: Option<wgpu::BlendState>,
    depth_write: bool,
    cull: Option<wgpu::Face>,
}

/// The reference's translucent blend state, exactly.
///
/// The OpenGL reference calls `glBlendFunc(SRC_ALPHA, ONE_MINUS_SRC_ALPHA)`
/// once, which applies the same factors and `FUNC_ADD` to both the colour and
/// the alpha channel. wgpu's convenience `BlendState::ALPHA_BLENDING` uses
/// `ONE`/`ONE_MINUS_SRC_ALPHA` for the alpha channel instead, so the state is
/// written out explicitly rather than approximated.
const TRANSLUCENT_BLEND: wgpu::BlendState = wgpu::BlendState {
    color: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::SrcAlpha,
        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
        operation: wgpu::BlendOperation::Add,
    },
    alpha: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::SrcAlpha,
        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
        operation: wgpu::BlendOperation::Add,
    },
};

/// Six material-pass pipeline variants. Opaque scene and emission variants
/// share the requested sidedness. Cutout and blend retain their two-sided
/// coverage contract; callers select culled opaque variants per entity primitive.
/// Architectural and static-prop ranges use the two-sided set.
///
/// The emissive variants mirror the reference's `u_emission_only` path: the
/// same material states, the emissive term alone, written with depth writes off
/// into the raw bloom source. The cut-out variant keeps the reference's
/// `discard`.
#[must_use]
const fn world_pipeline_variants(cull_opaque: bool) -> [WorldPipelineVariant; 6] {
    [
        WorldPipelineVariant {
            label: "places-wgpu-world-opaque",
            fragment_entry: WORLD_FRAGMENT_ENTRY,
            fragment_entry_raw: WORLD_FRAGMENT_ENTRY_RAW,
            blend: None,
            depth_write: true,
            cull: if cull_opaque {
                Some(wgpu::Face::Back)
            } else {
                None
            },
        },
        WorldPipelineVariant {
            label: "places-wgpu-world-cutout",
            fragment_entry: WORLD_CUTOUT_FRAGMENT_ENTRY,
            fragment_entry_raw: WORLD_CUTOUT_FRAGMENT_ENTRY_RAW,
            blend: None,
            depth_write: true,
            cull: None,
        },
        WorldPipelineVariant {
            label: "places-wgpu-world-translucent",
            fragment_entry: WORLD_FRAGMENT_ENTRY,
            fragment_entry_raw: WORLD_FRAGMENT_ENTRY_RAW,
            blend: Some(TRANSLUCENT_BLEND),
            depth_write: false,
            cull: None,
        },
        WorldPipelineVariant {
            label: "places-wgpu-world-emission",
            fragment_entry: WORLD_EMISSION_FRAGMENT_ENTRY,
            fragment_entry_raw: WORLD_EMISSION_FRAGMENT_ENTRY,
            blend: None,
            depth_write: false,
            cull: if cull_opaque {
                Some(wgpu::Face::Back)
            } else {
                None
            },
        },
        WorldPipelineVariant {
            label: "places-wgpu-world-emission-blend",
            fragment_entry: "fs_emission_blend",
            fragment_entry_raw: "fs_emission_blend",
            blend: Some(TRANSLUCENT_BLEND),
            depth_write: false,
            cull: None,
        },
        WorldPipelineVariant {
            label: "places-wgpu-world-emission-cutout",
            fragment_entry: WORLD_EMISSION_CUTOUT_FRAGMENT_ENTRY,
            fragment_entry_raw: WORLD_EMISSION_CUTOUT_FRAGMENT_ENTRY,
            blend: None,
            depth_write: false,
            cull: None,
        },
    ]
}

/// Builds one world pipeline variant from the shared pipeline layout, shader
/// and target format.
fn build_world_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    format: wgpu::TextureFormat,
    front_face: wgpu::FrontFace,
    raw_target: bool,
    variant: WorldPipelineVariant,
) -> wgpu::RenderPipeline {
    let shadow = format == DEPTH_FORMAT;
    let fragment_entry = if shadow {
        if variant.blend.is_some() {
            "fs_shadow_blend"
        } else if variant.fragment_entry.contains("cutout") {
            "fs_shadow_cutout"
        } else {
            "fs_shadow"
        }
    } else if raw_target {
        variant.fragment_entry_raw
    } else {
        variant.fragment_entry
    };
    let color_targets = [Some(wgpu::ColorTargetState {
        format,
        blend: variant.blend,
        write_mask: wgpu::ColorWrites::ALL,
    })];
    let targets = if shadow { &[][..] } else { &color_targets[..] };
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(variant.label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some(WORLD_VERTEX_ENTRY),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[Some(world_vertex_layout())],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            // Places winding: the front side is the side the geometric normal
            // points to (right-hand rule over p0 -> p1 -> p2), and WebGPU's
            // `Ccw` is the right-handed convention. The planar capture reverses
            // it, exactly like the reference's `glFrontFace(GL_CW)`.
            front_face,
            cull_mode: variant.cull,
            unclipped_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(shadow || variant.depth_write),
            // The reference's `glDepthFunc(GL_LEQUAL)`.
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: wgpu::StencilState::default(),
            bias: if shadow {
                wgpu::DepthBiasState {
                    constant: 2,
                    slope_scale: 2.0,
                    clamp: 0.0,
                }
            } else {
                wgpu::DepthBiasState::default()
            },
        }),
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(fragment_entry),
            compilation_options: wgpu::PipelineCompilationOptions {
                constants: &super::surface::color_target_constants(format),
                ..Default::default()
            },
            targets,
        }),
        multiview_mask: None,
        cache: None,
    })
}

fn world_uniform_binding(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    label: &str,
    size: u64,
) -> (wgpu::Buffer, wgpu::BindGroup) {
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let binding = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(label),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: buffer.as_entire_binding(),
        }],
    });
    (buffer, binding)
}

/// Opaque scene/emission pipelines with matching ordinary and reflected facing.
fn build_entity_pipelines(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    format: wgpu::TextureFormat,
    front: wgpu::FrontFace,
    raw: bool,
) -> [wgpu::RenderPipeline; 4] {
    let [opaque, _, _, emission, _, _] = world_pipeline_variants(true);
    let reflected = match front {
        wgpu::FrontFace::Ccw => wgpu::FrontFace::Cw,
        wgpu::FrontFace::Cw => wgpu::FrontFace::Ccw,
    };
    let build =
        |face, variant| build_world_pipeline(device, layout, shader, format, face, raw, variant);
    if format == DEPTH_FORMAT {
        let regular = build(front, opaque);
        let mirrored = build(reflected, opaque);
        return [regular.clone(), regular, mirrored.clone(), mirrored];
    }
    [
        build(front, opaque),
        build(front, emission),
        build(reflected, opaque),
        build(reflected, emission),
    ]
}

/// The world material render pipelines and their camera + texture + material
/// bindings.
///
/// Created once per colour target format. The camera uniform buffer and bind
/// group are format-independent and persist for the renderer's lifetime; the
/// camera is written with `Queue::write_buffer` only when the matrix changed.
/// Groups 1 and 2 use the texture cache's and the renderer's shared layouts, so
/// per-texture and per-material bind groups created once at upload survive a
/// pipeline rebuild unchanged.
///
/// Three scene variants and three emissive variants:
///
/// * **opaque** — depth writes on, no blending, culling selected by the caller.
/// * **cut-out** — the same state with the `fs_cutout` fragment entry point,
///   which discards below the material's cut-off. Two-sided.
/// * **translucent** — straight-alpha blending, depth writes off, two-sided.
/// * **emission / emission-cutout** — the emissive term alone, depth writes off,
///   into the raw bloom source.
/// * **emission-blend** — straight coverage blending for transparent emission.
pub struct WorldPipeline {
    opaque_culled: wgpu::RenderPipeline,
    opaque_culled_reflected: wgpu::RenderPipeline,
    emission_culled: wgpu::RenderPipeline,
    emission_culled_reflected: wgpu::RenderPipeline,
    opaque: wgpu::RenderPipeline,
    cutout: wgpu::RenderPipeline,
    translucent: wgpu::RenderPipeline,
    emission: wgpu::RenderPipeline,
    emission_cutout: wgpu::RenderPipeline,
    emission_blend: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    camera_buffer: wgpu::Buffer,
    /// The group-4 attached-light uniform. Created zeroed and written by
    /// [`WorldPipeline::update_lights`]; the reflection-capture pipelines never
    /// write it, so their captures stay light-free.
    lights_buffer: wgpu::Buffer,
    /// The group-4 binding over `lights_buffer`.
    lights_bind_group: wgpu::BindGroup,
    lights_layout: wgpu::BindGroupLayout,
    lights_sampler: wgpu::Sampler,
    flashlight_shadow_bound: bool,
    /// The colour target format the pipelines were built for. A changed surface
    /// format (a recreated surface) rebuilds only this resource.
    format: wgpu::TextureFormat,
    /// The last uploaded camera state, so a still camera writes nothing.
    uploaded: Option<CameraUniform>,
    #[cfg(feature = "visual-diagnostics")]
    visual_diagnostic: super::diagnostics::VisualDiagnosticMode,
}

impl WorldPipeline {
    /// Builds the main-pass pipeline set for one colour target format.
    #[must_use]
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        texture_layout: &wgpu::BindGroupLayout,
        material_layout: &wgpu::BindGroupLayout,
        environment_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        Self::with_state(
            device,
            format,
            texture_layout,
            material_layout,
            environment_layout,
            wgpu::FrontFace::Ccw,
            // World/static ranges contain two-sided panes, curtains and void
            // shells. Entity primitives select their own culled pipeline.
            false,
            // The main pipeline writes the sRGB surface.
            false,
        )
    }

    /// Builds one world pipeline set with explicit raster state.
    ///
    /// The reflection capture sets use this: the probe set renders with no
    /// culling (the reference never enables it in a capture) and the planar set
    /// additionally reverses the front face, because mirroring the camera flips
    /// every triangle's winding and the reference's `glFrontFace(GL_CW)` during
    /// the planar capture is what keeps `gl_FrontFacing` (and therefore the
    /// shader's normal flip) meaning the same thing.
    #[must_use]
    #[expect(
        clippy::too_many_arguments,
        reason = "one pipeline set's full raster state, all explicit"
    )] // one pipeline set's full raster state, all explicit
    pub fn with_state(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        texture_layout: &wgpu::BindGroupLayout,
        material_layout: &wgpu::BindGroupLayout,
        environment_layout: &wgpu::BindGroupLayout,
        front_face: wgpu::FrontFace,
        cull_opaque: bool,
        raw_target: bool,
    ) -> Self {
        #[cfg(feature = "visual-diagnostics")]
        let shader_source = super::diagnostics::world_shader_source(WORLD_SHADER_SRC);
        #[cfg(not(feature = "visual-diagnostics"))]
        let shader_source = WORLD_SHADER_SRC.into();
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("places-wgpu-world-shader"),
            source: wgpu::ShaderSource::Wgsl(shader_source),
        });
        let bind_group_layout = world_camera_bind_group_layout(device);
        // Group 4 is the frame's attached-light array: created with the
        // pipeline set and shared by every variant, so binding it is one call
        // per pass.
        let lights_layout = dynamic_lights_bind_group_layout(device);
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("places-wgpu-world-pipeline-layout"),
            bind_group_layouts: &[
                Some(&bind_group_layout),
                Some(texture_layout),
                Some(material_layout),
                Some(environment_layout),
                Some(&lights_layout),
            ],
            immediate_size: 0,
        });
        let (camera_buffer, bind_group) = world_uniform_binding(
            device,
            &bind_group_layout,
            "places-wgpu-world-camera",
            CAMERA_UNIFORM_SIZE,
        );
        // Zero-initialized capture buffers see no attached lights.
        let (lights_buffer, lights_bind_group, lights_sampler) =
            create_dynamic_lights_binding(device, &lights_layout);
        let [
            opaque_variant,
            cutout_variant,
            translucent_variant,
            emission_variant,
            emission_blend_variant,
            emission_cutout_variant,
        ] = world_pipeline_variants(cull_opaque);
        let build = |variant| {
            build_world_pipeline(
                device,
                &pipeline_layout,
                &shader,
                format,
                front_face,
                raw_target,
                variant,
            )
        };
        let [
            opaque_culled,
            emission_culled,
            opaque_culled_reflected,
            emission_culled_reflected,
        ] = build_entity_pipelines(
            device,
            &pipeline_layout,
            &shader,
            format,
            front_face,
            raw_target,
        );
        let opaque = build(opaque_variant);
        let cutout = build(cutout_variant);
        let translucent = build(translucent_variant);
        let (emission, emission_cutout, emission_blend) = if format == DEPTH_FORMAT {
            (opaque.clone(), cutout.clone(), translucent.clone())
        } else {
            (
                build(emission_variant),
                build(emission_cutout_variant),
                build(emission_blend_variant),
            )
        };
        Self {
            opaque_culled,
            opaque_culled_reflected,
            emission_culled,
            emission_culled_reflected,
            opaque,
            cutout,
            translucent,
            emission,
            emission_cutout,
            emission_blend,
            bind_group,
            camera_buffer,
            lights_buffer,
            lights_bind_group,
            lights_layout,
            lights_sampler,
            flashlight_shadow_bound: false,
            format,
            uploaded: None,
            #[cfg(feature = "visual-diagnostics")]
            visual_diagnostic: super::diagnostics::VisualDiagnosticMode::Final,
        }
    }

    /// The visibility pass uses the ordinary world draw and alpha contracts.
    #[must_use]
    pub fn shadow(
        device: &wgpu::Device,
        texture_layout: &wgpu::BindGroupLayout,
        material_layout: &wgpu::BindGroupLayout,
        environment_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        Self::with_state(
            device,
            DEPTH_FORMAT,
            texture_layout,
            material_layout,
            environment_layout,
            wgpu::FrontFace::Ccw,
            false,
            true,
        )
    }

    /// Bind the shared shadow texture once per pipeline lifetime.
    pub fn bind_flashlight_shadow(&mut self, device: &wgpu::Device, view: &wgpu::TextureView) {
        if self.flashlight_shadow_bound {
            return;
        }
        self.lights_bind_group = dynamic_lights_binding(
            device,
            &self.lights_layout,
            &self.lights_buffer,
            view,
            &self.lights_sampler,
        );
        self.flashlight_shadow_bound = true;
    }

    pub fn update_flashlight_lights(
        &self,
        queue: &wgpu::Queue,
        lights: &DynamicLightSet,
        flashlight: Option<crate::engine::render::common::Flashlight>,
    ) {
        let mut uniform = DynamicLightsUniform::new(lights);
        uniform.flashlight = GpuFlashlight::new(flashlight);
        queue.write_buffer(&self.lights_buffer, 0, bytemuck::bytes_of(&uniform));
    }

    /// The colour target format these pipelines' fragment state was built for.
    #[must_use]
    pub const fn format(&self) -> wgpu::TextureFormat {
        self.format
    }

    /// The pipeline one material pass submits through.
    const fn pipeline_for(&self, pass: BatchPass) -> &wgpu::RenderPipeline {
        match pass {
            BatchPass::Opaque => &self.opaque,
            BatchPass::Cutout => &self.cutout,
            BatchPass::Translucent => &self.translucent,
        }
    }

    /// The emissive-pass pipeline matching one material pass.
    const fn emission_pipeline_for(&self, pass: BatchPass) -> &wgpu::RenderPipeline {
        match pass {
            BatchPass::Cutout => &self.emission_cutout,
            BatchPass::Opaque => &self.emission,
            BatchPass::Translucent => &self.emission_blend,
        }
    }

    /// Selects sidedness only for opaque entity materials with a safe transform.
    const fn entity_pipeline_for(
        &self,
        pass: BatchPass,
        emission: bool,
        double_sided: bool,
        reflected: Option<bool>,
    ) -> &wgpu::RenderPipeline {
        match entity_cull_mode(pass, double_sided, reflected) {
            EntityCullMode::Regular if emission => &self.emission_culled,
            EntityCullMode::Regular => &self.opaque_culled,
            EntityCullMode::Reflected if emission => &self.emission_culled_reflected,
            EntityCullMode::Reflected => &self.opaque_culled_reflected,
            EntityCullMode::TwoSided if emission => self.emission_pipeline_for(pass),
            EntityCullMode::TwoSided => self.pipeline_for(pass),
        }
    }

    /// Uploads the frame's corrected view-projection and eye position when they
    /// changed.
    ///
    /// Both are one uniform, so a moved camera updates the whole value; the
    /// comparison is by the packed uniform, which keeps a still camera's frame
    /// from touching the buffer at all. See [`camera_uniform_changed`], which
    /// the test below exercises directly.
    pub fn upload_camera(&mut self, queue: &wgpu::Queue, view_projection: Mat4, eye: glam::Vec3) {
        #[cfg(feature = "visual-diagnostics")]
        let uniform =
            CameraUniform::new(view_projection, eye).with_visual_diagnostic(self.visual_diagnostic);
        #[cfg(not(feature = "visual-diagnostics"))]
        let uniform = CameraUniform::new(view_projection, eye);
        if !camera_uniform_changed(self.uploaded, uniform) {
            return;
        }
        queue.write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&uniform));
        self.uploaded = Some(uniform);
    }

    /// Selects a development view for this pipeline's next camera upload.
    #[cfg(feature = "visual-diagnostics")]
    pub const fn set_visual_diagnostic(&mut self, mode: super::diagnostics::VisualDiagnosticMode) {
        self.visual_diagnostic = mode;
    }

    /// The exact selector last uploaded to this pipeline's camera binding.
    #[cfg(feature = "visual-diagnostics")]
    #[must_use]
    pub fn uploaded_visual_selector(&self) -> Option<f32> {
        self.uploaded.map(|uniform| uniform.diagnostic_selector)
    }

    /// Uploads one frame's attached lights.
    ///
    /// Always writes the whole uniform, an empty set included: a zero count is
    /// what clears the previous frame's lights, so no stale glow can linger in
    /// a frame that attaches none. Callers are the main scene, offscreen-scene
    /// and emissive pipelines; the reflection-capture pipelines deliberately
    /// keep their zeroed set, so a mirror or probe never bakes a glow in.
    pub fn update_lights(&self, queue: &wgpu::Queue, lights: &DynamicLightSet) {
        write_dynamic_lights(queue, &self.lights_buffer, lights);
    }
}

pub(super) fn create_dynamic_lights_binding(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
) -> (wgpu::Buffer, wgpu::BindGroup, wgpu::Sampler) {
    let lights_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("places-wgpu-world-lights"),
        size: DYNAMIC_LIGHTS_UNIFORM_SIZE,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let fallback = super::flashlight::depth_texture(device, 1);
    let fallback_view = fallback.create_view(&wgpu::TextureViewDescriptor::default());
    let lights_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("places-flashlight-shadow-sampler"),
        compare: Some(wgpu::CompareFunction::LessEqual),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let lights_bind_group = dynamic_lights_binding(
        device,
        layout,
        &lights_buffer,
        &fallback_view,
        &lights_sampler,
    );
    (lights_buffer, lights_bind_group, lights_sampler)
}

pub(super) fn dynamic_lights_binding(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    buffer: &wgpu::Buffer,
    shadow: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("places-wgpu-world-lights-binding"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(shadow),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

/// Writes one light set into a group-4 uniform buffer.
///
/// Split from [`WorldPipeline::update_lights`] so the byte-level contents are
/// testable with a bare device and a copy-back buffer.
fn write_dynamic_lights(queue: &wgpu::Queue, buffer: &wgpu::Buffer, lights: &DynamicLightSet) {
    let uniform = DynamicLightsUniform::new(lights);
    queue.write_buffer(buffer, 0, bytemuck::bytes_of(&uniform));
}

/// The resources and frame state one world submission needs.
///
/// Grouped so the encode entry point stays small and the caller cannot pass the
/// geometry, textures and materials of different levels by mistake.
#[derive(Clone, Copy)]
pub struct WorldEncodeInputs<'a> {
    /// The level's uploaded geometry.
    pub geometry: &'a WgpuWorldGeometry,
    /// The level's per-draw base textures.
    pub textures: &'a WorldTextures,
    /// The level's per-draw materials.
    pub materials: &'a WorldMaterials,
    /// The frame/level environment binding (lightmaps, fog, reflections).
    pub environment: &'a wgpu::BindGroup,
    /// The level's uploaded prop batches, drawn between the static opaque and
    /// cut-out passes exactly like the reference body order.
    pub props: Option<&'a super::props::WgpuProps>,
    /// The level's live dynamic objects, drawn after the props exactly like the
    /// reference body order.
    pub dynamic: Option<&'a super::dynamic::WgpuDynamic>,
    /// The level's live characters, drawn after the dynamics.
    pub characters: Option<&'a super::character::WgpuCharacters>,
    /// The plane the current capture reflects, if any. Static batches whose
    /// material belongs to that plane are left out of the capture, exactly like
    /// the reference's `is_capture_mirror`.
    pub capture_plane: Option<usize>,
    /// The player's filtering setting.
    pub filtering: TextureFiltering,
    /// The prepared frame (matrices, frustum and eye).
    pub frame: &'a WorldFrame,
    /// Whether the CPU frustum test runs.
    pub cull: bool,
}

impl WorldPipeline {
    /// Encodes one frame's world draws into `pass`.
    ///
    /// Mirrors the OpenGL scene body: the opaque pass first, then the cut-out
    /// pass, then the translucent pass back to front. Within a pass the draw
    /// ranges keep their packed upload order; vertex/index buffers, the base
    /// texture and the material bind group are rebound only when a run of draws
    /// changes them. No reordering for batching.
    pub fn encode<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        inputs: WorldEncodeInputs<'a>,
    ) -> WorldDrawTotals {
        let mut totals = WorldDrawTotals::default();
        for pass_kind in [BatchPass::Opaque, BatchPass::Cutout] {
            let class = self.encode_class(
                pass,
                inputs,
                &inputs.frame.frustum,
                inputs.geometry.pass_indices.for_pass(pass_kind),
                pass_kind,
                false,
            );
            match pass_kind {
                BatchPass::Opaque => {
                    totals.opaque_draws = totals.opaque_draws.saturating_add(class.visible_batches);
                }
                BatchPass::Cutout => {
                    totals.cutout_draws = totals.cutout_draws.saturating_add(class.visible_batches);
                }
                BatchPass::Translucent => {
                    totals.translucent_draws = totals
                        .translucent_draws
                        .saturating_add(class.visible_batches);
                }
            }
            totals.draw_calls = totals.draw_calls.saturating_add(class.draw_calls);
            totals.visible_batches = totals.visible_batches.saturating_add(class.visible_batches);
            totals.visible_vertices = totals
                .visible_vertices
                .saturating_add(class.visible_vertices);
            totals.visible_indices = totals.visible_indices.saturating_add(class.visible_indices);
            totals.texture_binds = totals.texture_binds.saturating_add(class.texture_binds);
            totals.material_binds = totals.material_binds.saturating_add(class.material_binds);
            totals.emissive_visible |= class.emissive_visible;
            // The reference body order: static opaque, then props, then the
            // dynamic objects, then the characters, then the remaining static
            // classes. Model draws join their matching opaque/cutout class.
            if pass_kind != BatchPass::Translucent {
                totals.absorb(self.encode_extra_class(pass, inputs, false, pass_kind));
            }
        }
        let transparent = self.encode_transparency(pass, inputs, false);
        totals.translucent_draws = transparent.visible_batches;
        totals.absorb(transparent);
        totals
    }

    /// The scene and emission passes use one order across all transparent families.
    /// Sorting remains per range/object; intersecting triangles need authored separation.
    fn encode_transparency<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        inputs: WorldEncodeInputs<'a>,
        emission_only: bool,
    ) -> WorldDrawTotals {
        let mut items = Self::transparent_items(inputs);
        sort_transparency(&mut items, inputs.frame.eye);
        let mut totals = WorldDrawTotals::default();
        for (item, _) in items {
            let submitted = match item {
                TransparentItem::World(index) => self.encode_class(
                    pass,
                    inputs,
                    &inputs.frame.frustum,
                    &[index],
                    BatchPass::Translucent,
                    emission_only,
                ),
                TransparentItem::Prop(index) => {
                    inputs.props.map_or_else(WorldDrawTotals::default, |props| {
                        self.encode_props(
                            pass,
                            inputs,
                            props,
                            emission_only,
                            BatchPass::Translucent,
                            Some(index),
                        )
                    })
                }
                TransparentItem::Dynamic(index) => {
                    inputs
                        .dynamic
                        .map_or_else(WorldDrawTotals::default, |dynamic| {
                            self.encode_dynamic(
                                pass,
                                inputs,
                                dynamic,
                                emission_only,
                                BatchPass::Translucent,
                                Some(index),
                            )
                        })
                }
                TransparentItem::Character(index) => {
                    self.bind_character_state(pass, emission_only, BatchPass::Translucent);
                    inputs
                        .characters
                        .map_or_else(WorldDrawTotals::default, |characters| {
                            self.encode_character_submeshes(
                                pass,
                                inputs,
                                characters,
                                index,
                                emission_only,
                                BatchPass::Translucent,
                            )
                        })
                }
            };
            totals.absorb(submitted);
        }
        totals
    }

    fn transparent_items(inputs: WorldEncodeInputs<'_>) -> Vec<(TransparentItem, [f32; 3])> {
        let mut items = Vec::new();
        // The shared stable sort already orders every family. Collect each in
        // packed/scene order so ties stay unchanged without sorting subsets.
        for index in inputs
            .geometry
            .pass_indices
            .for_pass(BatchPass::Translucent)
        {
            if let Some(draw) = usize::try_from(*index)
                .ok()
                .and_then(|slot| inputs.geometry.draws.get(slot))
                .filter(|draw| draw.index_count > 0)
            {
                items.push((TransparentItem::World(*index), draw.bounds.centre()));
            }
        }
        if let Some(props) = inputs.props {
            for (index, draw) in props
                .draws()
                .iter()
                .enumerate()
                .filter(|(_, draw)| draw.pass == BatchPass::Translucent && draw.index_count > 0)
            {
                items.push((TransparentItem::Prop(index), draw.bounds.centre()));
            }
        }
        if let Some(dynamic) = inputs.dynamic {
            for index in 0..dynamic.object_count() {
                if (0..dynamic.submesh_count(index)).any(|submesh| {
                    dynamic.submesh_pass(index, submesh) == Some(BatchPass::Translucent)
                }) && let Some(bounds) = dynamic.world_bounds(index)
                {
                    items.push((TransparentItem::Dynamic(index), bounds.centre()));
                }
            }
        }
        if let Some(characters) = inputs.characters {
            for index in (0..characters.character_count())
                .filter(|index| characters.has_submesh_pass(*index, BatchPass::Translucent))
            {
                if let Some(bounds) = characters.world_bounds(index) {
                    items.push((TransparentItem::Character(index), bounds.centre()));
                }
            }
        }
        items
    }

    /// Encodes the opaque extras spliced after the static opaque class, in the
    /// reference body order: props, then dynamic objects, then characters.
    fn encode_extra_class<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        inputs: WorldEncodeInputs<'a>,
        emission_only: bool,
        wanted: BatchPass,
    ) -> WorldDrawTotals {
        let mut totals = WorldDrawTotals::default();
        if let Some(props) = inputs.props {
            totals.absorb(self.encode_props(pass, inputs, props, emission_only, wanted, None));
        }
        if let Some(dynamic) = inputs.dynamic {
            totals.absorb(self.encode_dynamic(pass, inputs, dynamic, emission_only, wanted, None));
        }
        if let Some(characters) = inputs.characters {
            totals.absorb(self.encode_characters(pass, inputs, characters, emission_only, wanted));
        }
        totals
    }

    /// Encodes the dynamic submeshes that belong to one draw pass.
    ///
    /// One draw per object per primitive, culled by the object's world bounds
    /// (the reference culls the dynamic path the same way). Each object binds
    /// its own group-3 environment, which carries its model matrix and its
    /// per-frame baked-light probe; the material and texture come from the
    /// shared mesh and the object's own material slots. `emission_only` is the
    /// emissive pass; `wanted` selects the alpha class, so a blended dynamic
    /// primitive draws in the shared translucent order, with depth writes off.
    fn encode_dynamic<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        inputs: WorldEncodeInputs<'a>,
        dynamic: &'a super::dynamic::WgpuDynamic,
        emission_only: bool,
        wanted: BatchPass,
        selected: Option<usize>,
    ) -> WorldDrawTotals {
        let mut totals = WorldDrawTotals::default();
        if emission_only {
            pass.set_pipeline(self.emission_pipeline_for(wanted));
        } else {
            pass.set_pipeline(self.pipeline_for(wanted));
        }
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_bind_group(4, &self.lights_bind_group, &[]);
        let object_range = selected.map_or_else(
            || 0..dynamic.object_count(),
            |index| index..index.saturating_add(1).min(dynamic.object_count()),
        );
        for object in object_range {
            let Some(bounds) = dynamic.world_bounds(object) else {
                continue;
            };
            if inputs.cull && !inputs.frame.frustum.intersects_aabb(&bounds) {
                continue;
            }
            let Some(environment) = dynamic.environment(object) else {
                continue;
            };
            pass.set_bind_group(3, environment, &[]);
            let mut bound_texture: Option<usize> = None;
            let mut bound_material: Option<usize> = None;
            let mut bound_geometry: Option<(usize, usize)> = None;
            let before_draws = totals.draw_calls;
            for submesh in 0..dynamic.submesh_count(object) {
                if dynamic.submesh_pass(object, submesh) != Some(wanted) {
                    continue;
                }
                if emission_only
                    && wanted != BatchPass::Translucent
                    && !dynamic.submesh_emissive(object, submesh)
                {
                    continue;
                }
                pass.set_pipeline(self.entity_pipeline_for(
                    wanted,
                    emission_only,
                    dynamic.submesh_double_sided(object, submesh),
                    dynamic.culling_reflected(object),
                ));
                totals.emissive_visible |= dynamic.submesh_emissive(object, submesh);
                let Some(material_slot) = dynamic.material_slot(object, submesh) else {
                    continue;
                };
                if bound_material != Some(material_slot) {
                    let Some(material) = dynamic.material(material_slot) else {
                        continue;
                    };
                    pass.set_bind_group(2, material.bind_group(inputs.filtering), &[]);
                    totals.material_binds = totals.material_binds.saturating_add(1);
                    bound_material = Some(material_slot);
                }
                if bound_texture != Some(material_slot) {
                    let Some(texture) = dynamic.submesh_texture(object, submesh) else {
                        continue;
                    };
                    pass.set_bind_group(1, texture.bind_group(inputs.filtering), &[]);
                    totals.texture_binds = totals.texture_binds.saturating_add(1);
                    bound_texture = Some(material_slot);
                }
                let Some((vertex_buffer, index_buffer, first_index, index_count)) =
                    dynamic.geometry(object, submesh)
                else {
                    continue;
                };
                if bound_geometry != Some((object, submesh)) {
                    pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                    pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                    bound_geometry = Some((object, submesh));
                }
                let Some(end) = first_index.checked_add(index_count) else {
                    continue;
                };
                pass.draw_indexed(first_index..end, 0, 0..1);
                totals.visible_indices = totals
                    .visible_indices
                    .saturating_add(usize::try_from(index_count).unwrap_or(usize::MAX));
                totals.draw_calls = totals.draw_calls.saturating_add(1);
                totals.visible_batches = totals.visible_batches.saturating_add(1);
            }
            totals.record_object_vertices(before_draws, dynamic.object_vertex_count(object));
        }
        totals
    }

    /// Encodes the frame's character submeshes of one pass.
    ///
    /// One draw per character per primitive, culled by the character's
    /// world-space bounds. Each character binds its own group-3 environment,
    /// which carries the placement matrix and the instance opacity; its vertex
    /// buffer already holds the CPU-skinned model-space pose, so the shader
    /// path is exactly the prop path. `emission_only` is the emissive pass, and
    /// `wanted` filters the submesh's actual opaque/cutout alpha class; the translucent pass
    /// enters through [`Self::encode_transparency`].
    fn encode_characters<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        inputs: WorldEncodeInputs<'a>,
        characters: &'a super::character::WgpuCharacters,
        emission_only: bool,
        wanted: BatchPass,
    ) -> WorldDrawTotals {
        let mut totals = WorldDrawTotals::default();
        self.bind_character_state(pass, emission_only, wanted);
        for character in 0..characters.character_count() {
            totals.absorb(self.encode_character_submeshes(
                pass,
                inputs,
                characters,
                character,
                emission_only,
                wanted,
            ));
        }
        totals
    }

    /// Binds the character pass's pipeline and its frame-level groups.
    fn bind_character_state<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        emission_only: bool,
        wanted: BatchPass,
    ) {
        if emission_only {
            pass.set_pipeline(self.emission_pipeline_for(wanted));
        } else {
            pass.set_pipeline(self.pipeline_for(wanted));
        }
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_bind_group(4, &self.lights_bind_group, &[]);
    }

    /// Encodes one character's submeshes of one pass.
    ///
    /// Each primitive selects its material-sidedness pipeline; frame-level
    /// camera and light groups were bound by the caller.
    fn encode_character_submeshes<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        inputs: WorldEncodeInputs<'a>,
        characters: &'a super::character::WgpuCharacters,
        character: usize,
        emission_only: bool,
        wanted: BatchPass,
    ) -> WorldDrawTotals {
        let mut totals = WorldDrawTotals::default();
        let Some(bounds) = characters.world_bounds(character) else {
            return totals;
        };
        if inputs.cull && !inputs.frame.frustum.intersects_aabb(&bounds) {
            return totals;
        }
        let Some(environment) = characters.environment(character) else {
            return totals;
        };
        pass.set_bind_group(3, environment, &[]);
        let mut bound_texture: Option<usize> = None;
        let mut bound_material: Option<usize> = None;
        let mut bound_geometry: Option<usize> = None;
        for submesh in 0..characters.submesh_count(character) {
            if characters.submesh_pass(character, submesh) != Some(wanted) {
                continue;
            }
            if emission_only
                && wanted != BatchPass::Translucent
                && !characters.submesh_emissive(character, submesh)
            {
                continue;
            }
            pass.set_pipeline(self.entity_pipeline_for(
                wanted,
                emission_only,
                characters.submesh_double_sided(character, submesh),
                characters.culling_reflected(character),
            ));
            totals.emissive_visible |= characters.submesh_emissive(character, submesh);
            let Some(material_slot) = characters.material_slot(character, submesh) else {
                continue;
            };
            if bound_material != Some(material_slot) {
                let Some(material) = characters.material(material_slot) else {
                    continue;
                };
                pass.set_bind_group(2, material.bind_group(inputs.filtering), &[]);
                totals.material_binds = totals.material_binds.saturating_add(1);
                bound_material = Some(material_slot);
            }
            if bound_texture != Some(material_slot) {
                let Some(texture) = characters.submesh_texture(character, submesh) else {
                    continue;
                };
                pass.set_bind_group(1, texture.bind_group(inputs.filtering), &[]);
                totals.texture_binds = totals.texture_binds.saturating_add(1);
                bound_texture = Some(material_slot);
            }
            let Some((vertex_buffer, index_buffer, first_index, index_count)) =
                characters.geometry(character, submesh)
            else {
                continue;
            };
            if bound_geometry != Some(character) {
                pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                bound_geometry = Some(character);
            }
            let Some(end) = first_index.checked_add(index_count) else {
                continue;
            };
            pass.draw_indexed(first_index..end, 0, 0..1);
            totals.visible_indices = totals
                .visible_indices
                .saturating_add(usize::try_from(index_count).unwrap_or(usize::MAX));
            totals.draw_calls = totals.draw_calls.saturating_add(1);
            totals.visible_batches = totals.visible_batches.saturating_add(1);
        }
        totals.record_object_vertices(0, characters.character_vertex_count(character));
        totals
    }

    /// Draw emission without albedo, lightmaps or reflection colour. Fog and
    /// storm transmission attenuate it once. Transparent surfaces share scene
    /// order and coverage, including nonemitters that cover emission behind them.
    pub fn encode_emissive<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        inputs: WorldEncodeInputs<'a>,
    ) -> WorldDrawTotals {
        let mut totals = WorldDrawTotals::default();
        for pass_kind in [BatchPass::Opaque, BatchPass::Cutout] {
            let class = self.encode_class(
                pass,
                inputs,
                &inputs.frame.frustum,
                inputs.geometry.pass_indices.for_pass(pass_kind),
                pass_kind,
                true,
            );
            totals.draw_calls = totals.draw_calls.saturating_add(class.draw_calls);
            totals.visible_batches = totals.visible_batches.saturating_add(class.visible_batches);
            totals.visible_vertices = totals
                .visible_vertices
                .saturating_add(class.visible_vertices);
            totals.visible_indices = totals.visible_indices.saturating_add(class.visible_indices);
            totals.texture_binds = totals.texture_binds.saturating_add(class.texture_binds);
            totals.material_binds = totals.material_binds.saturating_add(class.material_binds);
            totals.emissive_visible |= class.emissive_visible;
            if pass_kind != BatchPass::Translucent {
                totals.absorb(self.encode_extra_class(pass, inputs, true, pass_kind));
            }
        }
        totals.absorb(self.encode_transparency(pass, inputs, true));
        totals
    }

    /// Encodes the frame's prop draws.
    ///
    /// One draw per submesh, culled by its conservative draw bounds, with the prop's own
    /// clamped sheet on group 1 and its plain `(sheet, emission, alpha)`
    /// material on group 2. `emission_only` is the emissive pass: only submeshes
    /// whose material emits are submitted. A submesh whose glTF material is
    /// alpha-masked switches to the cutout pipeline; everything else stays
    /// opaque.
    fn encode_props<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        inputs: WorldEncodeInputs<'a>,
        props: &'a super::props::WgpuProps,
        emission_only: bool,
        wanted: BatchPass,
        selected: Option<usize>,
    ) -> WorldDrawTotals {
        let mut totals = WorldDrawTotals::default();
        let mut bound_pipeline: Option<BatchPass> = None;
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_bind_group(3, inputs.environment, &[]);
        pass.set_bind_group(4, &self.lights_bind_group, &[]);
        let mut bound_chunk: Option<usize> = None;
        let mut bound_texture: Option<usize> = None;
        let mut bound_material: Option<usize> = None;
        let draws = props.draws();
        let draw_range = selected.map_or(0..draws.len(), |index| {
            index..index.saturating_add(1).min(draws.len())
        });
        for draw in draws
            .get(draw_range)
            .unwrap_or_default()
            .iter()
            .filter(|draw| draw.pass == wanted)
        {
            if draw.index_count == 0 {
                continue;
            }
            if emission_only && wanted != BatchPass::Translucent && !draw.emissive {
                continue;
            }
            if inputs.cull && !inputs.frame.frustum.intersects_aabb(&draw.bounds) {
                continue;
            }
            if bound_pipeline != Some(draw.pass) {
                if emission_only {
                    pass.set_pipeline(self.emission_pipeline_for(draw.pass));
                } else {
                    pass.set_pipeline(self.pipeline_for(draw.pass));
                }
                bound_pipeline = Some(draw.pass);
            }
            totals.emissive_visible |= draw.emissive;
            if bound_material != Some(draw.material) {
                let Some(material) = props.material(draw.material) else {
                    continue;
                };
                pass.set_bind_group(2, material.bind_group(inputs.filtering), &[]);
                totals.material_binds = totals.material_binds.saturating_add(1);
                bound_material = Some(draw.material);
            }
            if bound_texture != Some(draw.texture) {
                let Some(texture) = props.texture(draw.texture) else {
                    continue;
                };
                pass.set_bind_group(1, texture.bind_group(inputs.filtering), &[]);
                totals.texture_binds = totals.texture_binds.saturating_add(1);
                bound_texture = Some(draw.texture);
            }
            if bound_chunk != Some(draw.chunk) {
                let Some(chunk) = props.chunk(draw.chunk) else {
                    continue;
                };
                pass.set_vertex_buffer(0, chunk.vertex_buffer.slice(..));
                pass.set_index_buffer(chunk.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                bound_chunk = Some(draw.chunk);
            }
            let Some(end) = draw.index_start.checked_add(draw.index_count) else {
                continue;
            };
            pass.draw_indexed(draw.index_start..end, 0, 0..1);
            totals.visible_indices = totals
                .visible_indices
                .saturating_add(usize::try_from(draw.index_count).unwrap_or(usize::MAX));
            totals.draw_calls = totals.draw_calls.saturating_add(1);
            totals.visible_batches = totals.visible_batches.saturating_add(1);
            totals.visible_vertices = totals
                .visible_vertices
                .saturating_add(usize::try_from(draw.vertex_count).unwrap_or(usize::MAX));
        }
        totals
    }

    /// Encodes one pass's surviving draws.
    fn encode_class<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        inputs: WorldEncodeInputs<'a>,
        frustum: &Frustum,
        indices: &[u32],
        pass_kind: BatchPass,
        emission_only: bool,
    ) -> WorldDrawTotals {
        let geometry = inputs.geometry;
        let materials = inputs.materials;
        let textures = inputs.textures;
        let filtering = inputs.filtering;
        let mut totals = WorldDrawTotals::default();
        if emission_only {
            pass.set_pipeline(self.emission_pipeline_for(pass_kind));
        } else {
            pass.set_pipeline(self.pipeline_for(pass_kind));
        }
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_bind_group(3, inputs.environment, &[]);
        pass.set_bind_group(4, &self.lights_bind_group, &[]);
        let mut bound_chunk: Option<usize> = None;
        let mut bound_texture: Option<usize> = None;
        let mut bound_material: Option<usize> = None;
        for index in indices {
            let Ok(vertex_index) = usize::try_from(*index) else {
                continue;
            };
            let Some(draw) = geometry.draws.get(vertex_index) else {
                continue;
            };
            if draw.index_count == 0 {
                continue;
            }
            if inputs.cull && !frustum.intersects_aabb(&draw.bounds) {
                continue;
            }
            let Some(material_slot) = materials.slot_for_draw(vertex_index) else {
                continue;
            };
            // The mirror's own surface is left out of its reflection image,
            // exactly like the reference's `is_capture_mirror`: it is the
            // nearest thing to the mirrored camera and would otherwise fill the
            // image with the mirror's own colour.
            if let Some(plane) = inputs.capture_plane
                && materials.material_plane(material_slot) == Some(plane)
            {
                continue;
            }
            if emission_only
                && pass_kind != BatchPass::Translucent
                && !materials.is_emissive(material_slot)
            {
                continue;
            }
            if bound_material != Some(material_slot) {
                let Some(material) = materials.entry(material_slot) else {
                    continue;
                };
                pass.set_bind_group(2, material.bind_group(filtering), &[]);
                totals.material_binds = totals.material_binds.saturating_add(1);
                totals.emissive_visible |= material.is_emissive();
                bound_material = Some(material_slot);
            }
            let Some(slot) = textures.slot_for_draw(vertex_index) else {
                continue;
            };
            if bound_texture != Some(slot) {
                let Some(texture) = textures.entry(slot) else {
                    continue;
                };
                pass.set_bind_group(1, texture.bind_group(filtering), &[]);
                totals.texture_binds = totals.texture_binds.saturating_add(1);
                bound_texture = Some(slot);
            }
            if bound_chunk != Some(draw.chunk) {
                let Some(chunk) = geometry.chunks.get(draw.chunk) else {
                    continue;
                };
                pass.set_vertex_buffer(0, chunk.vertex_buffer.slice(..));
                pass.set_index_buffer(chunk.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                bound_chunk = Some(draw.chunk);
            }
            let Some(end) = draw.index_start.checked_add(draw.index_count) else {
                continue;
            };
            pass.draw_indexed(draw.index_start..end, 0, 0..1);
            totals.visible_indices = totals
                .visible_indices
                .saturating_add(usize::try_from(draw.index_count).unwrap_or(usize::MAX));
            totals.draw_calls = totals.draw_calls.saturating_add(1);
            totals.visible_batches = totals.visible_batches.saturating_add(1);
            totals.visible_vertices = totals
                .visible_vertices
                .saturating_add(usize::try_from(draw.vertex_count).unwrap_or(usize::MAX));
        }
        totals
    }
}
