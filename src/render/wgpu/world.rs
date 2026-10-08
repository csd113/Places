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
//! `FrontFace::Ccw` for a right-handed projection. No production pipeline
//! culls — the reference never enabled `GL_CULL_FACE` and its two-sided shading
//! flips the normal on back faces — so every world variant is two-sided and
//! every pipeline keeps the same winding.

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
use crate::materials::{MaterialTable, ResolvedTexture, TextureOrigin};
use crate::quality::QualityLevel;
use crate::render::RenderCamera;
use crate::render::common::materials::{
    BatchPass, MaterialRenderState, batch_pass_for, resolve_surface_material,
};
use crate::render::common::mesh::{
    LevelMesh, LevelMeshRange, MaterialIndex, SurfaceKey, SurfaceKind, SurfaceShine, Vertex,
};
use crate::render::common::view::DrawableSize;
use crate::render::common::{DynamicLightSet, MAX_DYNAMIC_LIGHTS};
use crate::render::common::{MeshChunk, MeshPacker};
use crate::spatial::{Aabb, Frustum};

/// The world shader, from the file next to this module.
pub const WORLD_SHADER_SRC: &str =
    concat!(include_str!("storm.wgsl"), "\n", include_str!("world.wgsl"));

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

/// One GPU world vertex: the material frame, the lit colour and the lightmap
/// coordinates.
///
/// The faithful carry of the renderer-neutral `Vertex` fields the material and
/// lighting stages consume: world position, geometric frame (normal, UV-u
/// tangent and bitangent sign), world-space tiling UV, the material/vertex-lit
/// colour, and the lightmap atlas address.
///
/// The colour is uploaded as `Unorm8x4` through [`quantize_unit`], the same
/// quantisation the reference's packed layout applied. In the atlas build it is the material factor
/// (`tint × directional face shade`) with no baked light; in the vertex-lit
/// build it also carries the bake.
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
/// `#[repr(C)]` + `Pod` make the 64-byte stride explicit (the lightmap
/// attributes and the model-space position/colour/handedness tail);
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
    /// Explicit tail padding so the struct has no implicit padding (bytemuck)
    /// and a 4-byte-granular 64-byte stride.
    pub padding: [u8; 4],
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
            padding: [0; 4],
        }
    }
}

impl From<Vertex> for WorldVertex {
    fn from(vertex: Vertex) -> Self {
        Self::from(&vertex)
    }
}

const WORLD_VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 8] = wgpu::vertex_attr_array![
    WORLD_ATTRIB_POSITION => Float32x3,
    WORLD_ATTRIB_NORMAL => Float32x3,
    WORLD_ATTRIB_UV => Float32x2,
    WORLD_ATTRIB_COLOR => Float32x4,
    WORLD_ATTRIB_LIGHTMAP_UV => Unorm16x2,
    WORLD_ATTRIB_LIGHTMAP_PAGE => Float32,
    WORLD_ATTRIB_TANGENT => Float32x3,
    WORLD_ATTRIB_HANDEDNESS => Float32,
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
/// Places builds its view-projection with `glam::Mat4::perspective_rh_gl`, so a
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
    pub const fn new(region: &crate::render::common::atmosphere::FogRegion) -> Self {
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
/// ------------------------------------------------------ 2848 bytes, align 16
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
/// live count word. The struct is 2848 bytes on the wire and in Rust
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
    /// alpha and the emissive term.
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
    pub fog_regions: [FogRegionUniform; crate::level::MAX_FOG_REGIONS],
    /// Linear entity irradiance; w enables the probe material path.
    pub entity_irradiance: [f32; 4],
    /// Signed linear first moment; reserved w is zero.
    pub entity_moment: [f32; 4],
    pub storm: crate::render::common::storm::StormUniform,
    /// Padded columns of the inverse-transpose model matrix.
    pub normal_model: [[f32; 4]; 3],
}

impl EnvironmentUniform {
    /// The static-world value: unit light scale, the atlas switch, the level's
    /// fog, no active mirror and the identity model.
    ///
    /// The lightmap selection starts empty; [`Self::with_lightmaps`] installs
    /// the resident pages and switchable groups.
    #[must_use]
    pub const fn new(
        light_scale: [f32; 3],
        lightmap_enabled: bool,
        fog: crate::render::common::atmosphere::FogState,
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
            fog_regions: [FogRegionUniform::ZERO; crate::level::MAX_FOG_REGIONS],
            entity_irradiance: [0.0; 4],
            entity_moment: [0.0; 4],
            storm: crate::render::common::storm::StormUniform {
                color_density: [0.0; 4],
                count: [0; 4],
                shelters: [crate::render::common::storm::StormShelter {
                    min: [0.0; 4],
                    max: [0.0; 4],
                    roof: [0.0; 4],
                }; crate::weather::MAX_STORM_SHELTERS],
            },
        }
    }

    /// Installs the same environmental payload for rigid and animated models.
    #[must_use]
    pub fn with_entity_lighting(
        mut self,
        sample: Option<crate::render::common::light_transport::EntityLighting>,
    ) -> Self {
        self.entity_irradiance = [0.0, 0.0, 0.0, -1.0];
        self.entity_moment = [0.0; 4];
        self.light_scale = sample.map_or([crate::lighting::AMBIENT_LEVEL; 3], |s| s.display);
        if let Some(texel) = sample.and_then(|s| s.prepared) {
            self.light_scale = [1.0; 3];
            let [red, green, blue] = texel.irradiance;
            let [x, y, z] = texel.direction;
            self.entity_irradiance = [red, green, blue, 1.0];
            self.entity_moment = [x, y, z, 0.0];
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
        regions: &[crate::render::common::atmosphere::FogRegion],
        cap: usize,
    ) -> Self {
        self.fog_regions = [FogRegionUniform::ZERO; crate::level::MAX_FOG_REGIONS];
        let count = regions.len().min(cap).min(crate::level::MAX_FOG_REGIONS);
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
        let inverse = if model.determinant().abs() > 1.0e-12 {
            model.inverse().transpose()
        } else {
            Mat4::IDENTITY
        };
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
/// The neutral [`crate::render::common::dynamic_lights::DynamicLight`] keeps a
/// key for replacement; the key never reaches the shader, so the GPU value
/// carries only the position, radius, colour and intensity.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct GpuDynamicLight {
    /// World-space position, in metres.
    pub position: [f32; 3],
    /// The falloff window's radius, in metres, strictly positive.
    pub radius: f32,
    /// Display-space colour, each channel `0..=1`.
    pub color: [f32; 3],
    /// Additive intensity.
    pub intensity: f32,
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

/// The group-4 bind group layout: the frame's attached-light uniform.
///
/// Created once per world pipeline rebuild and shared by every pipeline that
/// samples the world shader; the bind group below is rebuilt with it because a
/// buffer binding is tied to one device buffer.
#[must_use]
pub fn dynamic_lights_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("places-wgpu-world-lights-layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: wgpu::BufferSize::new(DYNAMIC_LIGHTS_UNIFORM_SIZE),
            },
            count: None,
        }],
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
    /// [`crate::render::MATERIAL_NONE`] for an architectural range without a
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
/// neutral [`MeshPacker`] chunks the ranges exactly as the reference upload did,
/// so an index is always 16-bit and a range can be split across chunk buffers
/// without re-basing at draw time. An empty draw set produces no chunks and no
/// draws, which is a valid world.
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
        for packed in packer.push(&range.vertices, &range.indices) {
            draws.push(WorldDraw {
                chunk: packed.chunk,
                index_start: u32::try_from(packed.index_start).unwrap_or(u32::MAX),
                index_count: u32::try_from(packed.index_count).unwrap_or(u32::MAX),
                vertex_count: u32::try_from(packed.vertex_count).unwrap_or(u32::MAX),
                bounds: range.bounds,
                kind: range.key.kind,
                material: range.key.material,
                shine: range.key.shine,
                range: range_index,
                pass,
            });
        }
    }
    (packer, draws)
}

/// The static world mesh uploaded for the loaded level.
///
/// Persistent for the level's lifetime: one-time conversion at upload, never a
/// per-frame rebuild. Dropping it releases the GPU buffers.
pub struct WgpuWorldGeometry {
    chunks: Vec<WorldChunk>,
    draws: Vec<WorldDraw>,
    stats: WorldGeometryStats,
}

impl WgpuWorldGeometry {
    /// Uploads the world draw set of one renderer-neutral static mesh.
    #[must_use]
    pub fn upload(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        mesh: &LevelMesh,
        materials: &MaterialRenderState,
    ) -> Self {
        let (packer, draws) = pack_world_ranges(mesh, materials);
        crate::perf::startup_mark("world geometry: pack");

        let mut chunks: Vec<WorldChunk> = Vec::with_capacity(packer.chunks.len());
        for chunk in &packer.chunks {
            chunks.push(upload_chunk(device, queue, chunk));
        }
        crate::perf::startup_mark("world geometry: GPU upload");

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
        Self {
            chunks,
            draws,
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
                && draw.material != crate::render::common::mesh::MATERIAL_NONE
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
        crate::perf::startup_mark("world textures: plan");

        // One batch: cache hits are served from the maps, the misses are
        // prepared in parallel and uploaded serially in first-use order.
        let resolved_textures = cache.get_or_upload_batch(device, queue, &requests);
        crate::perf::startup_mark("world textures: batch");

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
fn upload_chunk(device: &wgpu::Device, queue: &wgpu::Queue, chunk: &MeshChunk) -> WorldChunk {
    let vertices: Vec<WorldVertex> = chunk.vertices.iter().map(WorldVertex::from).collect();
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
/// [`RenderStats`]: crate::render::RenderStats
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WorldDrawTotals {
    /// `draw_indexed` calls issued.
    pub draw_calls: usize,
    /// Ranges that survived the frustum test.
    pub visible_batches: usize,
    /// Distinct vertices those ranges index.
    pub visible_vertices: usize,
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
        self.texture_binds = self.texture_binds.saturating_add(other.texture_binds);
        self.material_binds = self.material_binds.saturating_add(other.material_binds);
        self.emissive_visible |= other.emissive_visible;
    }
}

/// The translucent draw indices, sorted back to front for one camera.
///
/// Mirrors the reference's translucent pass: one item per draw (never per
/// triangle), ordered by the squared distance from the camera to the draw's
/// AABB centre, farthest first so a nearer surface blends over a farther one.
/// The sort is stable, so equal distances keep the packed draw order — the
/// reference's own deterministic tie-break.
///
/// This is the one per-frame CPU ordering the world pass performs; the shipped
/// level has a handful of translucent ranges, so the cost is a few comparisons.
#[must_use]
pub fn translucent_order(draws: &[WorldDraw], eye: glam::Vec3) -> Vec<u32> {
    let mut order: Vec<u32> = draws
        .iter()
        .enumerate()
        .filter(|(_, draw)| draw.pass == BatchPass::Translucent && draw.index_count > 0)
        .map(|(index, _)| u32::try_from(index).unwrap_or(u32::MAX))
        .collect();
    let distance_sq = |index: &u32| -> f32 {
        let Some(draw) = usize::try_from(*index)
            .ok()
            .and_then(|draw_index| draws.get(draw_index))
        else {
            return 0.0;
        };
        let centre = draw.bounds.centre();
        // `glam`/plain float arithmetic with no overflow or panic path.

        let delta = glam::Vec3::new(centre[0] - eye.x, centre[1] - eye.y, centre[2] - eye.z);
        delta.length_squared()
    };
    order.sort_by(|left, right| {
        distance_sq(right)
            .partial_cmp(&distance_sq(left))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    order
}

/// The translucent characters' draw order: farthest first.
///
/// `entries` pairs each character's stable scene index with its world centre.
/// Ordering is by squared camera distance, farthest first (this frame's
/// nearer surface blends over a farther one), with the index as the
/// deterministic tie-break so two characters at the same distance always draw
/// in scene order. Pure, so the ordering contract is unit-testable without a
/// device.
#[must_use]
pub fn translucent_character_order(entries: &[(usize, glam::Vec3)], eye: glam::Vec3) -> Vec<usize> {
    let mut order: Vec<(usize, f32)> = entries
        .iter()
        .map(|(index, centre)| {
            // `glam` f32 arithmetic: a finite camera and finite bounds.
            #[expect(
                clippy::arithmetic_side_effects,
                reason = "`glam` f32 arithmetic: a finite camera and finite bounds."
            )]
            let distance_sq = (*centre - eye).length_squared();
            (*index, distance_sq)
        })
        .collect();
    order.sort_by(|left, right| {
        right
            .1
            .total_cmp(&left.1)
            .then_with(|| left.0.cmp(&right.0))
    });
    order.into_iter().map(|(index, _)| index).collect()
}

/// One world pipeline variant's distinguishing state.
///
/// The five material passes share the shader module, vertex layout, depth
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

/// The five material-pass pipeline variants.
///
/// Declarative and GPU-free so the states are unit-testable. `cull_opaque`
/// selects the main-pass opaque state (back-face culled) or the reference's
/// capture state (no culling anywhere: `GL_CULL_FACE` is never
/// enabled, and a mirror or a probe face must see the same two-sided room the
/// reference captured).
///
/// The material-defined cut-out and translucent passes are two-sided in every
/// set, exactly like the reference. Every variant keeps the same winding. A
/// single-sided opaque surface viewed from behind would be culled in the main
/// pass where the reference would shade it; no shipped content has one, and the
/// capture sets do not cull at all.
///
/// The emissive variants mirror the reference's `u_emission_only` path: the
/// same material states, the emissive term alone, written with depth writes off
/// into the raw bloom source. The cut-out variant keeps the reference's
/// `discard`.
#[must_use]
const fn world_pipeline_variants(cull_opaque: bool) -> [WorldPipelineVariant; 5] {
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
    let fragment_entry = if raw_target {
        variant.fragment_entry_raw
    } else {
        variant.fragment_entry
    };
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
            depth_write_enabled: Some(variant.depth_write),
            // The reference's `glDepthFunc(GL_LEQUAL)`.
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(fragment_entry),
            compilation_options: wgpu::PipelineCompilationOptions {
                constants: &super::surface::color_target_constants(format),
                ..Default::default()
            },
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: variant.blend,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
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
/// Three scene variants and two emissive variants:
///
/// * **opaque** — depth writes on, no blending, culling selected by the caller.
/// * **cut-out** — the same state with the `fs_cutout` fragment entry point,
///   which discards below the material's cut-off. Two-sided.
/// * **translucent** — straight-alpha blending, depth writes off, two-sided.
/// * **emission / emission-cutout** — the emissive term alone, depth writes off,
///   into the raw bloom source.
pub struct WorldPipeline {
    opaque: wgpu::RenderPipeline,
    cutout: wgpu::RenderPipeline,
    translucent: wgpu::RenderPipeline,
    emission: wgpu::RenderPipeline,
    emission_cutout: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    camera_buffer: wgpu::Buffer,
    /// The group-4 attached-light uniform. Created zeroed and written by
    /// [`WorldPipeline::update_lights`]; the reflection-capture pipelines never
    /// write it, so their captures stay light-free.
    lights_buffer: wgpu::Buffer,
    /// The group-4 binding over `lights_buffer`.
    lights_bind_group: wgpu::BindGroup,
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
            // The reference never enables `GL_CULL_FACE`, and the level holds
            // content that is single-sided and visible from its back (a draped
            // curtain/plane in the pool view), so the parity choice is the
            // reference's: shade both sides everywhere. `gl_FrontFacing` flips
            // the normal.
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
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
        });
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
        let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("places-wgpu-world-camera"),
            size: CAMERA_UNIFORM_SIZE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("places-wgpu-world-camera"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });
        // A wgpu buffer starts zeroed, so a capture that runs before the first
        // frame's update sees an empty light set.
        let lights_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("places-wgpu-world-lights"),
            size: DYNAMIC_LIGHTS_UNIFORM_SIZE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let lights_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("places-wgpu-world-lights"),
            layout: &lights_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: lights_buffer.as_entire_binding(),
            }],
        });
        let [
            opaque_variant,
            cutout_variant,
            translucent_variant,
            emission_variant,
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
        let opaque = build(opaque_variant);
        let cutout = build(cutout_variant);
        let translucent = build(translucent_variant);
        let emission = build(emission_variant);
        let emission_cutout = build(emission_cutout_variant);
        Self {
            opaque,
            cutout,
            translucent,
            emission,
            emission_cutout,
            bind_group,
            camera_buffer,
            lights_buffer,
            lights_bind_group,
            format,
            uploaded: None,
            #[cfg(feature = "visual-diagnostics")]
            visual_diagnostic: super::diagnostics::VisualDiagnosticMode::Final,
        }
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
            BatchPass::Opaque | BatchPass::Translucent => &self.emission,
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
        let geometry = inputs.geometry;
        let indices_of = |wanted: BatchPass| -> Vec<u32> {
            geometry
                .draws
                .iter()
                .enumerate()
                .filter(|(_, draw)| draw.pass == wanted)
                .map(|(index, _)| u32::try_from(index).unwrap_or(u32::MAX))
                .collect()
        };
        let opaque = indices_of(BatchPass::Opaque);
        let cutout = indices_of(BatchPass::Cutout);
        let translucent = translucent_order(&geometry.draws, inputs.frame.eye);
        let mut totals = WorldDrawTotals::default();
        for (indices, pass_kind) in [
            (opaque.as_slice(), BatchPass::Opaque),
            (cutout.as_slice(), BatchPass::Cutout),
            (translucent.as_slice(), BatchPass::Translucent),
        ] {
            let class = self.encode_class(
                pass,
                inputs,
                &inputs.frame.frustum,
                indices,
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
        if let Some(props) = inputs.props {
            totals.absorb(self.encode_props(pass, inputs, props, false, BatchPass::Translucent));
        }
        // Blended dynamic primitives (a moving glass panel) draw after the
        // sorted static translucent surfaces, with depth writes off and depth
        // testing against the opaque pass.
        if let Some(dynamic) = inputs.dynamic {
            totals.absorb(self.encode_dynamic(
                pass,
                inputs,
                dynamic,
                false,
                BatchPass::Translucent,
            ));
        }
        // Blended character submeshes (a fading ghost) follow the blended
        // dynamic primitives, sorted back to front so several ghosts composite
        // in the right order.
        if let Some(characters) = inputs.characters {
            totals.absorb(self.encode_translucent_characters(pass, inputs, characters, false));
        }
        totals
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
            totals.absorb(self.encode_props(pass, inputs, props, emission_only, wanted));
        }
        if let Some(dynamic) = inputs.dynamic {
            totals.absorb(self.encode_dynamic(pass, inputs, dynamic, emission_only, wanted));
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
    /// primitive draws in the translucent pass after the static translucent
    /// surfaces, with depth writes off.
    fn encode_dynamic<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        inputs: WorldEncodeInputs<'a>,
        dynamic: &'a super::dynamic::WgpuDynamic,
        emission_only: bool,
        wanted: BatchPass,
    ) -> WorldDrawTotals {
        let mut totals = WorldDrawTotals::default();
        if emission_only {
            pass.set_pipeline(self.emission_pipeline_for(wanted));
        } else {
            pass.set_pipeline(self.pipeline_for(wanted));
        }
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_bind_group(4, &self.lights_bind_group, &[]);
        for object in 0..dynamic.object_count() {
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
                if emission_only && !dynamic.submesh_emissive(object, submesh) {
                    continue;
                }
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
    /// enters through [`Self::encode_translucent_characters`].
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
            totals.absorb(Self::encode_character_submeshes(
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

    /// Encodes the frame's translucent character submeshes, farthest first.
    ///
    /// Drawn after the blended dynamic primitives and the static translucent
    /// class with depth writes off, depth-testing against the opaque pass. The
    /// order is the squared camera distance to the character's world centre,
    /// farthest first, with the entry index as the deterministic tie-break, so
    /// several ghosts composite consistently.
    fn encode_translucent_characters<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        inputs: WorldEncodeInputs<'a>,
        characters: &'a super::character::WgpuCharacters,
        emission_only: bool,
    ) -> WorldDrawTotals {
        let mut totals = WorldDrawTotals::default();
        let entries: Vec<(usize, glam::Vec3)> = (0..characters.character_count())
            .filter(|character| characters.has_submesh_pass(*character, BatchPass::Translucent))
            .filter_map(|character| {
                characters
                    .world_bounds(character)
                    .map(|bounds| (character, glam::Vec3::from_array(bounds.centre())))
            })
            .collect();
        if entries.is_empty() {
            return totals;
        }
        self.bind_character_state(pass, emission_only, BatchPass::Translucent);
        for character in translucent_character_order(&entries, inputs.frame.eye) {
            totals.absorb(Self::encode_character_submeshes(
                pass,
                inputs,
                characters,
                character,
                emission_only,
                BatchPass::Translucent,
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
    /// Associated rather than a method: the per-character body binds only
    /// frame-level state the caller already bound, so it needs no access to
    /// the pipeline set.
    fn encode_character_submeshes<'a>(
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
            if emission_only && !characters.submesh_emissive(character, submesh) {
                continue;
            }
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
            totals.draw_calls = totals.draw_calls.saturating_add(1);
            totals.visible_batches = totals.visible_batches.saturating_add(1);
        }
        totals.record_object_vertices(0, characters.character_vertex_count(character));
        totals
    }

    /// The reference's `draw_emissive_body`: the same body without decals,
    /// every stage drawing only the surfaces whose material emits, into the
    /// bloom source with depth writes off and no blending. The fragment entry
    /// points write the emissive term raw; fog, lightmaps, reflections and the
    /// albedo never run there.
    pub fn encode_emissive<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        inputs: WorldEncodeInputs<'a>,
    ) -> WorldDrawTotals {
        let geometry = inputs.geometry;
        let indices_of = |wanted: BatchPass| -> Vec<u32> {
            geometry
                .draws
                .iter()
                .enumerate()
                .filter(|(_, draw)| draw.pass == wanted)
                .map(|(index, _)| u32::try_from(index).unwrap_or(u32::MAX))
                .collect()
        };
        let opaque = indices_of(BatchPass::Opaque);
        let cutout = indices_of(BatchPass::Cutout);
        let translucent = translucent_order(&geometry.draws, inputs.frame.eye);
        let mut totals = WorldDrawTotals::default();
        for (indices, pass_kind) in [
            (opaque.as_slice(), BatchPass::Opaque),
            (cutout.as_slice(), BatchPass::Cutout),
            (translucent.as_slice(), BatchPass::Translucent),
        ] {
            let class = self.encode_class(
                pass,
                inputs,
                &inputs.frame.frustum,
                indices,
                pass_kind,
                true,
            );
            totals.draw_calls = totals.draw_calls.saturating_add(class.draw_calls);
            totals.visible_batches = totals.visible_batches.saturating_add(class.visible_batches);
            totals.visible_vertices = totals
                .visible_vertices
                .saturating_add(class.visible_vertices);
            totals.texture_binds = totals.texture_binds.saturating_add(class.texture_binds);
            totals.material_binds = totals.material_binds.saturating_add(class.material_binds);
            totals.emissive_visible |= class.emissive_visible;
            if pass_kind != BatchPass::Translucent {
                totals.absorb(self.encode_extra_class(pass, inputs, true, pass_kind));
            }
        }
        if let Some(props) = inputs.props {
            totals.absorb(self.encode_props(pass, inputs, props, true, BatchPass::Translucent));
        }
        // A blended dynamic primitive can still emit; its emissive draw joins
        // the bloom source after the static translucent emission.
        if let Some(dynamic) = inputs.dynamic {
            totals.absorb(self.encode_dynamic(pass, inputs, dynamic, true, BatchPass::Translucent));
        }
        // A translucent character's emissive submeshes join the bloom source
        // here too, scaled by the instance opacity the shader applies, so a
        // ghost's cyan core blooms through its fade instead of popping.
        if let Some(characters) = inputs.characters {
            totals.absorb(self.encode_translucent_characters(pass, inputs, characters, true));
        }
        totals
    }

    /// Encodes the frame's prop draws.
    ///
    /// One draw per submesh, culled by the batch bounds, with the prop's own
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
    ) -> WorldDrawTotals {
        let mut totals = WorldDrawTotals::default();
        let mut bound_pipeline: Option<BatchPass> = None;
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_bind_group(3, inputs.environment, &[]);
        pass.set_bind_group(4, &self.lights_bind_group, &[]);
        let mut bound_chunk: Option<usize> = None;
        let mut bound_texture: Option<usize> = None;
        let mut bound_material: Option<usize> = None;
        let mut translucent: Vec<_> = if wanted == BatchPass::Translucent {
            props
                .draws()
                .iter()
                .filter(|draw| draw.pass == wanted)
                .collect()
        } else {
            Vec::new()
        };
        translucent.sort_by(|left, right| {
            let distance = |draw: &super::props::PropDraw| {
                glam::Vec3::from_array(draw.bounds.centre()).distance_squared(inputs.frame.eye)
            };
            distance(right).total_cmp(&distance(left))
        });
        for draw in props
            .draws()
            .iter()
            .filter(|draw| wanted != BatchPass::Translucent && draw.pass == wanted)
            .chain(translucent)
        {
            if draw.index_count == 0 {
                continue;
            }
            if emission_only && !draw.emissive {
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
            if emission_only && !materials.is_emissive(material_slot) {
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
            totals.draw_calls = totals.draw_calls.saturating_add(1);
            totals.visible_batches = totals.visible_batches.saturating_add(1);
            totals.visible_vertices = totals
                .visible_vertices
                .saturating_add(usize::try_from(draw.vertex_count).unwrap_or(usize::MAX));
        }
        totals
    }
}

#[cfg(test)]
mod tests {
    // Test code: unwrap/indexing/float comparisons and panics are idiomatic here.
    #![allow(
        clippy::arithmetic_side_effects,
        clippy::expect_used,
        clippy::float_cmp,
        clippy::indexing_slicing,
        clippy::panic,
        clippy::print_stdout,
        clippy::suboptimal_flops,
        clippy::unwrap_used,
        reason = "Regression fixtures assert exact reference results and fail on invalid setup; these exceptions are confined to tests and developer measurement output"
    )]

    use super::*;
    use crate::test_support::{assert_exact, assert_exact_array};

    #[test]
    #[ignore = "requires a GPU adapter"]
    fn odd_triangle_indices_upload_without_changing_draw_counts() {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: None,
            ..Default::default()
        }))
        .expect("GPU adapter");
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("odd-index-upload-regression"),
            ..Default::default()
        }))
        .expect("GPU device");
        // Independent expected storage sizes: a triangle is six bytes and
        // needs eight for transfer; test_room's 363 indices need 728, not 726.
        for (count, expected_bytes) in [(0, 4), (3, 8), (6, 12), (363, 728)] {
            let chunk = MeshChunk {
                vertices: Vec::new(),
                indices: vec![0; count],
            };
            let uploaded = upload_chunk(&device, &queue, &chunk);
            assert_eq!(uploaded.index_buffer.size(), expected_bytes);
            assert_eq!(usize::try_from(uploaded.index_count).unwrap(), count);
        }
    }
    use crate::materials::{AlphaMode, MaterialAlpha};
    use crate::render::common::mesh::{
        LevelMesh, LevelMeshBatches, LevelMeshRange, MATERIAL_NONE, MaterialIndex, SurfaceKey,
    };
    use crate::render::common::view::vertical_fov_for_aspect;
    use crate::render::common::{SCENE_FAR_M, SCENE_NEAR_M, Vertex};

    /// A quad as the level builders emit one: `p0 p1 p2` and `p0 p2 p3`, with
    /// the frame left at the neutral default.
    fn quad(points: [[f32; 3]; 4]) -> (Vec<Vertex>, Vec<u16>) {
        let mut vertices = Vec::with_capacity(6);
        for index in [0usize, 1, 2, 0, 2, 3] {
            vertices.push(Vertex::new(points[index], [1.0; 4], [0.0; 2]));
        }
        (vertices, vec![0, 1, 2, 3, 4, 5])
    }

    /// One synthetic static range.
    fn range(kind: SurfaceKind, material: MaterialIndex) -> LevelMeshRange {
        let points = match kind {
            // The emitter winding: a floor's front side is +Y.
            SurfaceKind::Floor => [
                [0.0, 0.0, 2.0],
                [1.0, 0.0, 2.0],
                [1.0, 0.0, 0.0],
                [0.0, 0.0, 0.0],
            ],
            SurfaceKind::Ceiling
            | SurfaceKind::Wall
            | SurfaceKind::Light
            | SurfaceKind::PropFallback
            | SurfaceKind::Decal => [
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [1.0, 1.0, 0.0],
                [0.0, 1.0, 0.0],
            ],
        };
        let (vertices, indices) = quad(points);
        LevelMeshRange {
            key: SurfaceKey::new(kind, material),
            vertices,
            indices,
            bounds: Aabb {
                min: [-1.0, -1.0, -1.0],
                max: [2.0, 2.0, 3.0],
            },
        }
    }

    /// One synthetic static range with an authored shine override.
    fn range_with_shine(kind: SurfaceKind, material: MaterialIndex, shine: f32) -> LevelMeshRange {
        let mut range = range(kind, material);
        range.key.shine = Some(SurfaceShine::from_unit(shine));
        range
    }

    /// A mesh over the given ranges, with the aggregate counts filled in.
    fn mesh(ranges: Vec<LevelMeshRange>) -> LevelMesh {
        let vertex_count = ranges.iter().map(|range| range.vertices.len()).sum();
        let index_count = ranges.iter().map(|range| range.indices.len()).sum();
        LevelMesh {
            ranges,
            batches: LevelMeshBatches::default(),
            vertex_count,
            index_count,
        }
    }

    /// A camera looking level toward -Z from the origin.
    fn camera() -> RenderCamera {
        RenderCamera::new(glam::Vec3::ZERO, 0.0, 0.0, 60.0)
    }

    /// Project one world point to NDC through a prepared frame.
    fn ndc(frame: &WorldFrame, point: [f32; 3]) -> [f32; 3] {
        let clip = frame.view_projection * glam::Vec4::new(point[0], point[1], point[2], 1.0);
        [clip.x / clip.w, clip.y / clip.w, clip.z / clip.w]
    }

    /// The signed area of a projected triangle: positive is counter-clockwise
    /// in the Y-up NDC frame, which WebGPU's `FrontFace::Ccw` treats as front.
    fn signed_area(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> f32 {
        0.5 * ((b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1]))
    }

    #[test]
    fn normal_transform_is_perpendicular_after_rotation_and_nonuniform_scale() {
        let model = Mat4::from_rotation_y(0.7) * Mat4::from_scale(glam::Vec3::new(2.0, 0.5, 1.5));
        let environment = EnvironmentUniform::new(
            [1.0; 3],
            false,
            crate::render::common::atmosphere::FogState::SHIPPED,
        )
        .with_model(model);
        let [x, y, z] = environment.normal_model;
        let normal_matrix = glam::Mat3::from_cols(
            glam::Vec3::from_slice(&x[..3]),
            glam::Vec3::from_slice(&y[..3]),
            glam::Vec3::from_slice(&z[..3]),
        );
        let source_normal = glam::Vec3::new(1.0, 1.0, 0.0).normalize();
        let source_tangent = glam::Vec3::new(1.0, -1.0, 0.0).normalize();
        let actual = (normal_matrix * source_normal).normalize();
        let tangent = model.transform_vector3(source_tangent).normalize();
        assert!(actual.dot(tangent).abs() < 1.0e-6);
        assert!(
            model
                .transform_vector3(source_normal)
                .normalize()
                .dot(tangent)
                .abs()
                > 0.5
        );
        assert_eq!(std::mem::offset_of!(EnvironmentUniform, normal_model), 2848);
    }

    #[test]
    fn empty_model_alpha_classes_do_not_count_object_vertices() {
        let mut totals = WorldDrawTotals::default();
        totals.record_object_vertices(0, 432);
        assert_eq!(totals.visible_vertices, 0);
        totals.draw_calls = 1;
        totals.record_object_vertices(0, 432);
        totals.record_object_vertices(1, 432);
        assert_eq!(totals.visible_vertices, 432);
    }

    // ------------------------------------------------------------- layout

    #[test]
    fn the_world_vertex_preserves_linear_hdr_with_the_declared_attributes() {
        assert_eq!(std::mem::size_of::<WorldVertex>(), 76);
        assert_eq!(WORLD_VERTEX_STRIDE, 76);
        assert_eq!(std::mem::offset_of!(WorldVertex, position), 0);
        assert_eq!(std::mem::offset_of!(WorldVertex, normal), 12);
        assert_eq!(std::mem::offset_of!(WorldVertex, uv), 24);
        assert_eq!(std::mem::offset_of!(WorldVertex, color), 32);
        assert_eq!(std::mem::offset_of!(WorldVertex, lightmap_uv), 48);
        assert_eq!(std::mem::offset_of!(WorldVertex, lightmap_page), 52);
        assert_eq!(std::mem::offset_of!(WorldVertex, tangent), 56);
        assert_eq!(std::mem::offset_of!(WorldVertex, handedness), 68);
        assert_eq!(std::mem::offset_of!(WorldVertex, padding), 72);

        let layout = world_vertex_layout();
        assert_eq!(layout.array_stride, 76);
        assert_eq!(layout.step_mode, wgpu::VertexStepMode::Vertex);
        assert_eq!(layout.attributes.len(), 8);
        let attributes: [(u32, wgpu::VertexFormat, u64); 8] = [
            (WORLD_ATTRIB_POSITION, wgpu::VertexFormat::Float32x3, 0),
            (WORLD_ATTRIB_NORMAL, wgpu::VertexFormat::Float32x3, 12),
            (WORLD_ATTRIB_UV, wgpu::VertexFormat::Float32x2, 24),
            (WORLD_ATTRIB_COLOR, wgpu::VertexFormat::Float32x4, 32),
            (WORLD_ATTRIB_LIGHTMAP_UV, wgpu::VertexFormat::Unorm16x2, 48),
            (WORLD_ATTRIB_LIGHTMAP_PAGE, wgpu::VertexFormat::Float32, 52),
            (WORLD_ATTRIB_TANGENT, wgpu::VertexFormat::Float32x3, 56),
            (WORLD_ATTRIB_HANDEDNESS, wgpu::VertexFormat::Float32, 68),
        ];
        for (attribute, (location, format, offset)) in layout.attributes.iter().zip(attributes) {
            assert_eq!(attribute.shader_location, location);
            assert_eq!(attribute.format, format);
            assert_eq!(attribute.offset, offset);
        }
        // The reference's exact attribute types: two normalized unsigned shorts
        // for the atlas UV and one un-normalized byte (carried as a float) for
        // the page, in both of its vertex layouts.
        assert_eq!(WORLD_ATTRIB_LIGHTMAP_UV, 6);
        assert_eq!(WORLD_ATTRIB_LIGHTMAP_PAGE, 7);
    }

    #[test]
    fn a_renderer_neutral_vertex_converts_field_for_field() {
        let vertex = Vertex {
            pos: [1.5, -2.25, 3.0],
            color: [0.1, 0.2, 0.3, 1.0],
            uv: [4.0, 5.5],
            normal: [0.0, 1.0, 0.0],
            tangent: [1.0, 0.0, 0.0],
            handedness: -1.0,
            lightmap: [7, 9],
            lightmap_page: 3,
        };
        let world = WorldVertex::from(&vertex);
        assert_eq!(world.position, vertex.pos);
        assert_eq!(world.normal, vertex.normal);
        assert_eq!(world.uv, vertex.uv);
        assert_eq!(world.tangent, vertex.tangent);
        assert_eq!(world.handedness, vertex.handedness);
        // Linear factors and vertex illumination retain their full range.
        assert_eq!(world.color, vertex.color);
        // The lightmap address is carried exactly: the UV comes from the
        // vertex's 16-bit fixed point and the page byte becomes the float the
        // shader's `step(254.5, page)` test compares against.
        assert_eq!(world.lightmap_uv, [7, 9]);
        assert_eq!(world.lightmap_page, 3.0);
        assert_eq!(world.padding, [0; 4]);
    }

    #[test]
    fn the_camera_uniform_matches_the_wgsl_uniform_layout() {
        assert_eq!(std::mem::size_of::<CameraUniform>(), 80);
        assert_eq!(std::mem::align_of::<CameraUniform>(), 16);
        assert_eq!(CAMERA_UNIFORM_SIZE, 80);
        let uniform = CameraUniform::new(Mat4::IDENTITY, glam::Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(uniform.view_projection[0], [1.0, 0.0, 0.0, 0.0]);
        assert_eq!(uniform.view_projection[3], [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(uniform.position, [1.0, 2.0, 3.0]);
        // The eye is at the WGSL `vec3<f32>` offset (64) with explicit padding
        // behind it, so the struct's Rust layout is the shader's uniform layout.
        assert_eq!(std::mem::offset_of!(CameraUniform, position), 64);
        assert_eq!(std::mem::offset_of!(CameraUniform, diagnostic_selector), 76);
        // The shader declares exactly one camera binding: a matrix and the eye.
        assert!(WORLD_SHADER_SRC.contains("mat4x4<f32>"));
        assert!(WORLD_SHADER_SRC.contains("position: vec3<f32>"));
        assert!(WORLD_SHADER_SRC.contains("_padding: f32"));
        assert!(WORLD_SHADER_SRC.contains("@group(0) @binding(0)"));
    }

    #[test]
    fn a_still_camera_skips_the_uniform_write_but_an_eye_move_does_not() {
        let eye = glam::Vec3::new(1.0, 2.0, 3.0);
        let uniform = CameraUniform::new(Mat4::IDENTITY, eye);
        assert!(
            camera_uniform_changed(None, uniform),
            "the first frame always writes the camera"
        );
        assert!(
            !camera_uniform_changed(Some(uniform), uniform),
            "an identical packed uniform must not touch the buffer"
        );
        // The sheen is view-dependent, so an eye that moved must reach the
        // shader even when the view-projection is bit-identical.
        let moved_eye = CameraUniform::new(Mat4::IDENTITY, glam::Vec3::new(1.0, 2.0, 3.5));
        assert!(camera_uniform_changed(Some(uniform), moved_eye));
        // And a moved matrix must reach it with the eye unmoved.
        let moved_matrix =
            CameraUniform::new(Mat4::from_translation(glam::Vec3::new(0.25, 0.0, 0.0)), eye);
        assert!(camera_uniform_changed(Some(uniform), moved_matrix));
    }

    #[cfg(feature = "visual-diagnostics")]
    #[test]
    fn a_diagnostic_change_uploads_even_when_the_camera_is_still() {
        use super::super::diagnostics::VisualDiagnosticMode;
        let final_view = CameraUniform::new(Mat4::IDENTITY, glam::Vec3::ZERO);
        let normals = final_view.with_visual_diagnostic(VisualDiagnosticMode::WorldNormal);
        assert!(
            camera_uniform_changed(Some(final_view), normals),
            "live selector must reach a still camera"
        );
        assert!(
            !camera_uniform_changed(Some(normals), normals),
            "unchanged selector must skip writes"
        );
        assert!(
            camera_uniform_changed(Some(normals), final_view),
            "returning to final must reach the shader"
        );
        assert_eq!(
            final_view.diagnostic_selector.to_bits(),
            0.0_f32.to_bits(),
            "final retains production padding bits"
        );
    }

    // -------------------------------------------------------- coordinates

    #[test]
    fn the_clip_correction_maps_gl_depth_onto_the_wgpu_range() {
        let correction = clip_correction();
        let map = |z: f32| {
            let clip = correction * Vec4::new(0.0, 0.0, z, 1.0);
            clip.z / clip.w
        };
        assert!((map(-1.0) - 0.0).abs() < 1e-6, "near plane -> 0");
        assert!((map(0.0) - 0.5).abs() < 1e-6, "midpoint -> 0.5");
        assert!((map(1.0) - 1.0).abs() < 1e-6, "far plane -> 1");
        // The w component is untouched, so a clip vertex with w != 1 keeps its
        // perspective divide: gl z_ndc = -0.25 becomes wgpu z_ndc = 0.375.
        let clip = correction * Vec4::new(0.0, 0.0, -0.5, 2.0);
        assert_eq!(clip.w, 2.0);
        assert!((clip.z / clip.w - 0.375).abs() < 1e-6);
    }

    #[test]
    fn the_clip_correction_never_mirrors_x_or_y() {
        let correction = clip_correction();
        let clip = correction * Vec4::new(3.0, 4.0, 0.0, 1.0);
        assert_eq!(clip.x, 3.0);
        assert_eq!(clip.y, 4.0);
    }

    #[test]
    fn the_world_frame_uses_the_places_camera_and_full_depth_range() {
        let size = DrawableSize::new(1280, 720);
        assert_eq!(size.aspect_ratio(), 16.0 / 9.0);
        let frame = prepare_world_frame(camera(), size);

        // A point one near-plane ahead of the eye maps to depth 0; one at the
        // far plane maps to depth 1. Places keeps its 0.1/100 m planes.
        let near = ndc(&frame, [0.0, 0.0, -SCENE_NEAR_M]);
        let far = ndc(&frame, [0.0, 0.0, -SCENE_FAR_M]);
        assert!((near[2] - 0.0).abs() < 1e-5, "near depth {near:?}");
        assert!((far[2] - 1.0).abs() < 1e-5, "far depth {far:?}");

        // The vertical FOV is the shared aspect-aware one: a point exactly on
        // the top edge of the frustum at unit depth maps to y = 1.
        let fov = vertical_fov_for_aspect(60.0, size.aspect_ratio());
        let half = (fov.to_radians() * 0.5).tan();
        let edge = ndc(&frame, [0.0, half, -1.0]);
        assert!((edge[1] - 1.0).abs() < 1e-5, "top edge {edge:?}");
        // Wider than the reference aspect: the horizontal edge sits at 1 in x.
        let edge_x = ndc(&frame, [half * size.aspect_ratio(), 0.0, -1.0]);
        assert!((edge_x[0] - 1.0).abs() < 1e-5, "right edge {edge_x:?}");
    }

    #[test]
    fn a_floor_quad_is_front_facing_from_above_and_back_facing_from_below() {
        // The emitter winding (common/mod.rs: `emit_lit_surface_grid`): a floor
        // faces +Y. Looking down at it from above must be counter-clockwise in
        // NDC under `FrontFace::Ccw`; from below it must be clockwise.
        let floor = [[0.0, 0.0, -2.0], [1.0, 0.0, -2.0], [1.0, 0.0, -3.0]];
        let above = RenderCamera::new(glam::Vec3::new(0.5, 2.0, 2.0), 0.0, -0.5, 60.0);
        let frame = prepare_world_frame(above, DrawableSize::new(1280, 720));
        let [a, b, c] = floor.map(|point| ndc(&frame, point));
        assert!(
            a[2] > 0.0 && b[2] > 0.0 && c[2] > 0.0,
            "in front of the eye"
        );
        assert!(
            signed_area(a, b, c) > 0.0,
            "a floor's +Y front side must project counter-clockwise: {:?}",
            signed_area(a, b, c)
        );

        let below = RenderCamera::new(glam::Vec3::new(0.5, -2.0, 2.0), 0.0, 0.5, 60.0);
        let below_frame = prepare_world_frame(below, DrawableSize::new(1280, 720));
        let [below_a, below_b, below_c] = floor.map(|point| ndc(&below_frame, point));
        assert!(
            signed_area(below_a, below_b, below_c) < 0.0,
            "the floor's back side must project clockwise: {:?}",
            signed_area(below_a, below_b, below_c)
        );
    }

    #[test]
    fn a_wall_face_is_front_facing_from_the_room_it_looks_into() {
        // A wall's length face is wound so its front side is the room its
        // outward normal points into (geometry.rs `emit_wall_slice`).
        let face = [[0.0, 0.0, -2.0], [1.0, 0.0, -2.0], [1.0, 1.0, -2.0]];
        // Normal +Z: the face's front side faces the +Z side, where the camera is.
        let front = RenderCamera::new(glam::Vec3::new(0.5, 0.5, 0.0), 0.0, 0.0, 60.0);
        let frame = prepare_world_frame(front, DrawableSize::new(1280, 720));
        let [a, b, c] = face.map(|point| ndc(&frame, point));
        assert!(
            signed_area(a, b, c) > 0.0,
            "the face's outward side must project counter-clockwise"
        );

        // The camera behind the wall looks +Z at its back side: clockwise.
        let back = RenderCamera::new(
            glam::Vec3::new(0.5, 0.5, -4.0),
            std::f32::consts::PI,
            0.0,
            60.0,
        );
        let back_frame = prepare_world_frame(back, DrawableSize::new(1280, 720));
        let [back_a, back_b, back_c] = face.map(|point| ndc(&back_frame, point));
        assert!(
            back_a[2] > 0.0 && back_b[2] > 0.0 && back_c[2] > 0.0,
            "in front of the eye"
        );
        assert!(
            signed_area(back_a, back_b, back_c) < 0.0,
            "the reverse side must project clockwise"
        );
    }

    // ---------------------------------------------------------- draw set

    #[test]
    fn the_world_draw_set_keeps_every_static_family_but_decals() {
        for kind in [
            SurfaceKind::Floor,
            SurfaceKind::Ceiling,
            SurfaceKind::Wall,
            SurfaceKind::Light,
            SurfaceKind::PropFallback,
        ] {
            assert!(
                is_world_range(&range(kind, MATERIAL_NONE)),
                "{kind:?} belongs to the static world body"
            );
        }
        // Decals are their own pass with their own depth bias.
        assert!(!is_world_range(&range(SurfaceKind::Decal, MATERIAL_NONE)));
    }

    #[test]
    fn the_world_draw_set_includes_cutout_and_translucent_materials() {
        let mut materials = MaterialRenderState {
            alphas: vec![MaterialAlpha::OPAQUE; 3],
            ..MaterialRenderState::default()
        };
        materials.alphas[1] = MaterialAlpha {
            mode: AlphaMode::Blend,
            opacity: 0.5,
            cutoff: 0.5,
        };
        materials.alphas[2] = MaterialAlpha {
            mode: AlphaMode::Cutout,
            opacity: 1.0,
            cutoff: 0.5,
        };
        // Every architectural range is in the set; the material's alpha chooses
        // the pass, not membership.
        assert!(is_world_range(&range(SurfaceKind::Wall, 0)));
        assert!(is_world_range(&range(SurfaceKind::Wall, 1)));
        assert!(is_world_range(&range(SurfaceKind::Wall, 2)));
        assert!(is_world_range(&range(SurfaceKind::Wall, MATERIAL_NONE)));

        let mesh = mesh(vec![
            range(SurfaceKind::Wall, 0),
            range(SurfaceKind::Wall, 1),
            range(SurfaceKind::Wall, 2),
            range(SurfaceKind::Wall, MATERIAL_NONE),
        ]);
        let (_, draws) = pack_world_ranges(&mesh, &materials);
        let passes: Vec<BatchPass> = draws.iter().map(|draw| draw.pass).collect();
        assert_eq!(
            passes,
            vec![
                BatchPass::Opaque,
                BatchPass::Translucent,
                BatchPass::Cutout,
                // A bare key never binds a material, so it is opaque however the
                // table is populated.
                BatchPass::Opaque,
            ]
        );
    }

    #[test]
    fn a_zero_opacity_blend_material_stays_opaque() {
        // The reference classifies `blend` with `opacity == 0` as opaque and
        // draws it: this is the neutral rule the wgpu path must not "fix".
        let materials = MaterialRenderState {
            alphas: vec![MaterialAlpha::blend(0.0)],
            ..MaterialRenderState::default()
        };
        let mesh = mesh(vec![range(SurfaceKind::Wall, 0)]);
        let (_, draws) = pack_world_ranges(&mesh, &materials);
        assert_eq!(draws.len(), 1);
        assert_eq!(draws[0].pass, BatchPass::Opaque);
    }

    #[test]
    fn a_per_surface_shine_override_reaches_the_draw() {
        let materials = MaterialRenderState::default();
        let mesh = mesh(vec![
            range(SurfaceKind::Wall, 0),
            range_with_shine(SurfaceKind::Wall, 0, 0.05),
        ]);
        let (_, draws) = pack_world_ranges(&mesh, &materials);
        assert_eq!(draws.len(), 2);
        assert_eq!(draws[0].shine, None);
        assert_eq!(
            draws[1].shine,
            Some(SurfaceShine::from_unit(0.05)),
            "the override is part of the draw's material identity"
        );
    }

    #[test]
    fn packing_covers_every_selected_index_exactly_once() {
        let materials = MaterialRenderState::default();
        let mesh = mesh(vec![
            range(SurfaceKind::Floor, MATERIAL_NONE),
            range(SurfaceKind::Light, MATERIAL_NONE),
            range(SurfaceKind::Wall, MATERIAL_NONE),
            range(SurfaceKind::Decal, MATERIAL_NONE), // dropped
            range(SurfaceKind::Ceiling, MATERIAL_NONE),
        ]);
        let (packer, draws) = pack_world_ranges(&mesh, &materials);

        // Four selected ranges, one draw each, all in one chunk.
        assert_eq!(draws.len(), 4);
        assert_eq!(packer.chunks.len(), 1);
        let covered: usize = draws
            .iter()
            .map(|draw| usize::try_from(draw.index_count).expect("fixture integer fits usize"))
            .sum();
        assert_eq!(covered, 4 * 6, "every selected index is drawn");
        let chunk = &packer.chunks[0];
        for draw in &draws {
            let start = usize::try_from(draw.index_start).expect("fixture integer fits usize");
            let end =
                start + usize::try_from(draw.index_count).expect("fixture integer fits usize");
            let indices = &chunk.indices[start..end];
            assert_eq!(indices.len() % 3, 0);
            for index in indices {
                assert!(usize::from(*index) < chunk.vertices.len());
            }
        }
        // Every draw sits inside its chunk's index buffer.
        assert!(draws.iter().all(|draw| draw.chunk == 0));
        let last = draws.last().unwrap();
        assert_eq!(
            usize::try_from(last.index_start + last.index_count)
                .expect("fixture integer fits usize"),
            chunk.indices.len()
        );
        // The upload counts would be exactly the selected ranges' counts.
        let selected_vertices: usize = mesh
            .ranges
            .iter()
            .filter(|range| is_world_range(range))
            .map(|range| range.vertices.len())
            .sum();
        assert_eq!(packer.vertex_total(), selected_vertices);
        assert_eq!(packer.index_total(), covered);
    }

    #[test]
    fn an_empty_or_decal_only_world_packs_to_nothing() {
        let materials = MaterialRenderState::default();
        let empty = mesh(Vec::new());
        let (packer, draws) = pack_world_ranges(&empty, &materials);
        assert!(packer.chunks.is_empty());
        assert!(draws.is_empty(), "draws must be empty");

        // A level whose static mesh holds only decal ranges produces no world
        // resources at all: clear/present stays valid.
        let only_decals = mesh(vec![range(SurfaceKind::Decal, MATERIAL_NONE)]);
        let (decal_packer, decal_draws) = pack_world_ranges(&only_decals, &materials);
        assert!(decal_packer.chunks.is_empty());
        assert!(decal_draws.is_empty(), "decal_draws must be empty");
    }

    #[test]
    fn the_family_breakdown_counts_draws_per_surface_kind() {
        let materials = MaterialRenderState::default();
        let mesh = mesh(vec![
            range(SurfaceKind::Floor, MATERIAL_NONE),
            range(SurfaceKind::Wall, MATERIAL_NONE),
            range(SurfaceKind::Wall, MATERIAL_NONE),
            range(SurfaceKind::Ceiling, MATERIAL_NONE),
            range(SurfaceKind::Decal, MATERIAL_NONE),
            range(SurfaceKind::Light, MATERIAL_NONE),
            range(SurfaceKind::PropFallback, MATERIAL_NONE),
        ]);
        let (packer, draws) = pack_world_ranges(&mesh, &materials);
        // Mirror what the upload would build, without a device.
        assert_eq!(packer.chunks.len(), 1);
        let breakdown_for =
            |kind: SurfaceKind| draws.iter().filter(|draw| draw.kind == kind).count();
        assert_eq!(breakdown_for(SurfaceKind::Floor), 1);
        assert_eq!(breakdown_for(SurfaceKind::Wall), 2);
        assert_eq!(breakdown_for(SurfaceKind::Ceiling), 1);
        assert_eq!(breakdown_for(SurfaceKind::Decal), 0);
        assert_eq!(breakdown_for(SurfaceKind::Light), 1);
        assert_eq!(breakdown_for(SurfaceKind::PropFallback), 1);
    }

    #[test]
    fn a_drawable_size_change_only_changes_the_projection() {
        // A resize (or a HiDPI backing-scale change) must update the aspect and
        // FOV without any world-buffer work: `pack_world_ranges` is a function
        // of the neutral mesh and material table only — no drawable size — so
        // only `prepare_world_frame` consumes the frame's size.
        let camera = camera();
        let wide = prepare_world_frame(camera, DrawableSize::new(1280, 720));
        let tall = prepare_world_frame(camera, DrawableSize::new(640, 480));
        assert_ne!(
            wide.view_projection.to_cols_array(),
            tall.view_projection.to_cols_array(),
            "the projection must follow the drawable size"
        );
        // Both keep the full wgpu depth range.
        for frame in [&wide, &tall] {
            let near = ndc(frame, [0.0, 0.0, -SCENE_NEAR_M]);
            let far = ndc(frame, [0.0, 0.0, -SCENE_FAR_M]);
            assert!((near[2] - 0.0).abs() < 1e-5);
            assert!((far[2] - 1.0).abs() < 1e-5);
        }
        // A narrower drawable keeps the horizontal view (the configured FOV
        // widens vertically instead): the horizontal edge maps to 1 in x for
        // both sizes.
        for (frame, size) in [
            (&wide, DrawableSize::new(1280, 720)),
            (&tall, DrawableSize::new(640, 480)),
        ] {
            let fov = vertical_fov_for_aspect(60.0, size.aspect_ratio());
            let half = (fov.to_radians() * 0.5).tan();
            let edge = ndc(frame, [half * size.aspect_ratio(), 0.0, -1.0]);
            assert!(
                (edge[0] - 1.0).abs() < 1e-5,
                "horizontal edge at {size:?}: {edge:?}"
            );
        }
    }

    // ------------------------------------------------------------ culling

    #[test]
    fn the_draw_bounds_come_from_the_neutral_range() {
        let materials = MaterialRenderState::default();
        let mesh = mesh(vec![range(SurfaceKind::Floor, MATERIAL_NONE)]);
        let expected = mesh.ranges[0].bounds;
        let (_, draws) = pack_world_ranges(&mesh, &materials);
        assert_eq!(draws.len(), 1);
        assert_eq!(draws[0].bounds, expected);
    }

    // -------------------------------------------------- shipped-demo parity

    #[test]
    fn the_vertex_lit_build_matches_the_reference_mesh() {
        use crate::lighting::lightmap::LightmapMode;
        use crate::quality::QualityLevel;
        use crate::render::common::api::{
            LightmapBuildOptions, build_level_geometry_timed_with_lightmaps,
        };
        use crate::render::common::{build_level_geometry, logical_materials};

        let json = include_str!("../../../assets/levels/places_demo.json");
        let level = crate::level::LevelDef::from_json(json).expect("valid places_demo json");
        let materials = logical_materials(&level);
        let catalog = crate::loader::PropCatalog::builtin();

        // The shipped demo takes the vertex-lit path: the baked light rides in
        // the vertex colour, and a lightmapped build is opted into explicitly.
        let options = LightmapBuildOptions::for_level(QualityLevel::High, LightmapMode::Off);
        assert_eq!(options.mode, LightmapMode::Off);

        // Props cannot resolve (no asset root): the architecture, which is what
        // this test is about, is unaffected, and the bake's prop occluders
        // degrade to the catalogue placeholders the build and the reference
        // share.
        let mut assets = crate::props::PropAssets::with_root("/nonexistent-places-assets");
        let build = build_level_geometry_timed_with_lightmaps(
            &level,
            &catalog,
            &mut assets,
            &materials,
            options,
            None,
        );
        assert!(build.lightmaps.is_none(), "no atlas on the vertex-lit path");
        assert!(build.lightmap_failure.is_none());

        // Byte-for-byte the historical vertex-lit mesh the OpenGL reference
        // builds for `PLACES_NO_LIGHTMAPS=1`: geometry, frames and colours.
        let historical = build_level_geometry(&level);
        let compare_to_historical = |mesh: &crate::render::common::LevelMesh| {
            assert_eq!(mesh.vertex_count, historical.vertex_count);
            assert_eq!(mesh.index_count, historical.index_count);
            assert_eq!(mesh.ranges.len(), historical.ranges.len());
            for (built, reference) in mesh.ranges.iter().zip(historical.ranges.iter()) {
                assert_eq!(built.key, reference.key);
                assert_eq!(built.indices, reference.indices);
                assert_eq!(built.vertices, reference.vertices);
            }
        };
        compare_to_historical(&build.mesh);

        // Low must bake the *same* light: `LightmapMode::Off` always uses
        // `BakeConfig::HARD` whatever the level, so the two meshes are
        // byte-identical and Low only drops the surface response. (An atlas
        // build is where the level's tap count and prop cell matter.)
        let mut low_assets = crate::props::PropAssets::with_root("/nonexistent-places-assets");
        let low = build_level_geometry_timed_with_lightmaps(
            &level,
            &catalog,
            &mut low_assets,
            &materials,
            LightmapBuildOptions::for_level(QualityLevel::Low, LightmapMode::Off),
            None,
        );
        compare_to_historical(&low.mesh);
        assert_eq!(
            build.mesh.vertex_count, low.mesh.vertex_count,
            "the vertex-lit bake is level-independent"
        );
    }

    #[test]
    fn the_vertex_lit_bake_dims_the_material_factor_and_subdivides_the_grid() {
        use crate::lighting::lightmap::LightmapMode;
        use crate::quality::QualityLevel;
        use crate::render::common::api::{
            LightmapBuildOptions, build_level_geometry_timed_with_lightmaps,
        };
        use crate::render::common::logical_materials;

        let json = include_str!("../../../assets/levels/places_demo.json");
        let level = crate::level::LevelDef::from_json(json).expect("valid places_demo json");
        let materials = logical_materials(&level);
        let catalog = crate::loader::PropCatalog::builtin();
        let mut assets = crate::props::PropAssets::with_root("/nonexistent-places-assets");
        let build = build_level_geometry_timed_with_lightmaps(
            &level,
            &catalog,
            &mut assets,
            &materials,
            LightmapBuildOptions::for_level(QualityLevel::High, LightmapMode::Off),
            None,
        );

        // The light is really there. The vertex-lit build must (a) dim most
        // architectural vertices and (b) subdivide the lighting grid where the
        // light varies — that is how a vertex-lit bake produces a gradient at
        // all. The same level with every fixture at zero output bakes flat
        // light, so it merges the same architecture into fewer ranges; that is
        // the non-vacuous baseline for the subdivision below.
        let architectural = |mesh: &crate::render::common::LevelMesh| {
            mesh.ranges
                .iter()
                .filter(|range| {
                    matches!(
                        range.key.kind,
                        SurfaceKind::Floor | SurfaceKind::Ceiling | SurfaceKind::Wall
                    )
                })
                .count()
        };
        let mut unlit = level.clone();
        for light in &mut unlit.ceiling_lights {
            light.brightness = Some(0.0);
        }
        let mut direct_assets = crate::props::PropAssets::with_root("/nonexistent-places-assets");
        let flat = build_level_geometry_timed_with_lightmaps(
            &unlit,
            &catalog,
            &mut direct_assets,
            &materials,
            LightmapBuildOptions::for_level(QualityLevel::High, LightmapMode::Off),
            None,
        );
        assert!(
            architectural(&build.mesh) > architectural(&flat.mesh),
            "the vertex-lit build must subdivide the lighting grid: {} vs {}",
            architectural(&build.mesh),
            architectural(&flat.mesh)
        );
        let lit_vertices: Vec<Vertex> = build
            .mesh
            .triangles_for(SurfaceKind::Floor)
            .into_iter()
            .chain(build.mesh.triangles_for(SurfaceKind::Ceiling))
            .chain(build.mesh.triangles_for(SurfaceKind::Wall))
            .collect();
        assert!(
            lit_vertices.len() > 1_000,
            "the demo has a real architectural mesh: {} triangles' worth",
            lit_vertices.len()
        );
        // The bake's `shade` clamps every channel, and the demo is lit, so most
        // vertices sit meaningfully below the material factor alone. A build
        // that lost the light would have white-ish walls and no dimmed share
        // at all.
        let mut dimmed = 0usize;
        let mut strongly_dimmed = 0usize;
        for lit in &lit_vertices {
            for channel in 0..3 {
                assert!(
                    (0.0..=1.0).contains(&lit.color[channel]),
                    "a baked vertex colour is clamped: {:?}",
                    lit.color
                );
            }
            if lit.color[0] < 0.999 {
                dimmed = dimmed.saturating_add(1);
            }
            if lit.color[0] < 0.9 {
                strongly_dimmed = strongly_dimmed.saturating_add(1);
            }
        }
        assert!(
            dimmed.saturating_mul(2) > lit_vertices.len(),
            "most of the demo's architecture must carry baked light: {dimmed} of {}",
            lit_vertices.len()
        );
        assert!(
            strongly_dimmed > 100,
            "the demo must have measurably dimmed surfaces: {strongly_dimmed}"
        );
    }

    #[test]
    fn the_shipped_demo_uploads_every_static_world_index_exactly_once() {
        use crate::render::common::{build_level_geometry, logical_materials};

        let json = include_str!("../../../assets/levels/places_demo.json");
        let level = crate::level::LevelDef::from_json(json).expect("valid places_demo json");
        let mesh = build_level_geometry(&level);
        assert!(mesh.vertex_count > 0 && mesh.index_count > 0);

        // A bare table treats every range as opaque; the world draw set is
        // exactly the static world families: every floor, ceiling, wall, light
        // and placeholder-box index is drawn once, and only decals stay out.
        let materials = MaterialRenderState::default();
        let (packer, draws) = pack_world_ranges(&mesh, &materials);
        let static_world = mesh.index_count_for(SurfaceKind::Floor)
            + mesh.index_count_for(SurfaceKind::Ceiling)
            + mesh.index_count_for(SurfaceKind::Wall)
            + mesh.index_count_for(SurfaceKind::Light)
            + mesh.index_count_for(SurfaceKind::PropFallback);
        assert!(static_world > 0);
        assert_eq!(
            packer.index_total(),
            static_world,
            "every static world index must be uploaded"
        );
        assert!(!draws.is_empty(), "draws must contain entries");
        assert!(
            draws.iter().all(|draw| matches!(
                draw.kind,
                SurfaceKind::Floor
                    | SurfaceKind::Ceiling
                    | SurfaceKind::Wall
                    | SurfaceKind::Light
                    | SurfaceKind::PropFallback
            )),
            "only static world ranges may be drawn"
        );
        for draw in &draws {
            let chunk = &packer.chunks[draw.chunk];
            assert!(
                usize::try_from(draw.index_start + draw.index_count)
                    .expect("fixture integer fits usize")
                    <= chunk.indices.len(),
                "draw must stay inside its chunk"
            );
        }
        // Decals are the one family with its own pass and are not selected: the
        // geometry builder emits none without a resolved decal atlas, so the
        // demonstrated set here is the whole static mesh.
        for draw in &draws {
            assert_ne!(draw.kind, SurfaceKind::Decal);
        }

        // With the level's real material table the whole static set is still
        // drawn — the material-defined panes are now included — and the panes
        // land in the cut-out and translucent passes.
        let resolved = MaterialRenderState::from_table(&logical_materials(&level));
        let (with_materials, real) = pack_world_ranges(&mesh, &resolved);
        assert_eq!(
            with_materials.index_total(),
            static_world,
            "cut-out and translucent architecture belongs to the world draw set"
        );
        let translucent = real
            .iter()
            .filter(|draw| draw.pass == BatchPass::Translucent)
            .count();
        let cutout = real
            .iter()
            .filter(|draw| draw.pass == BatchPass::Cutout)
            .count();
        assert!(
            translucent >= 1,
            "the demo's glass panes are material-defined translucent surfaces"
        );
        assert!(
            cutout >= 1,
            "the demo's grille is a material-defined cut-out"
        );
        let opaque = real
            .iter()
            .filter(|draw| draw.pass == BatchPass::Opaque)
            .count();
        assert_eq!(
            translucent + cutout + opaque,
            real.len(),
            "every draw belongs to exactly one material pass"
        );
    }

    // ---------------------------------------------- base-colour texture identity

    /// One synthetic world draw of the given key, through the real packer.
    fn draw_of(kind: SurfaceKind, material: MaterialIndex) -> WorldDraw {
        let materials = MaterialRenderState::default();
        let (_, draws) = pack_world_ranges(&mesh(vec![range(kind, material)]), &materials);
        draws[0]
    }

    /// A draw of the given key without the packer's family selection, for the
    /// later-stage kinds the world pass deliberately excludes.
    fn synthetic_draw(kind: SurfaceKind, material: MaterialIndex) -> WorldDraw {
        WorldDraw {
            chunk: 0,
            index_start: 0,
            index_count: 6,
            vertex_count: 6,
            bounds: Aabb {
                min: [-1.0, -1.0, -1.0],
                max: [1.0, 1.0, 1.0],
            },
            kind,
            material,
            shine: None,
            range: 0,
            pass: BatchPass::Opaque,
        }
    }

    /// A synthetic translucent wall draw centred at `centre_z`.
    fn translucent_draw(centre_z: f32) -> WorldDraw {
        WorldDraw {
            chunk: 0,
            index_start: 0,
            index_count: 6,
            vertex_count: 6,
            bounds: Aabb {
                min: [-1.0, -1.0, centre_z - 1.0],
                max: [1.0, 1.0, centre_z + 1.0],
            },
            kind: SurfaceKind::Wall,
            material: 0,
            shine: None,
            range: 0,
            pass: BatchPass::Translucent,
        }
    }

    #[test]
    fn a_packed_draw_keeps_its_range_material_key() {
        let materials = MaterialRenderState::default();
        let mesh = mesh(vec![
            range(SurfaceKind::Floor, 3),
            range(SurfaceKind::Wall, 7),
            range(SurfaceKind::Ceiling, MATERIAL_NONE),
        ]);
        let (_, draws) = pack_world_ranges(&mesh, &materials);
        let keys: Vec<MaterialIndex> = draws.iter().map(|draw| draw.material).collect();
        assert_eq!(keys, vec![3, 7, MATERIAL_NONE]);
    }

    #[test]
    fn only_architectural_surfaces_with_a_real_material_resolve_a_base_texture() {
        let table = crate::materials::MaterialTable::default();
        let materials = MaterialRenderState::default();
        // An empty table resolves nothing, whatever the key says.
        for kind in [
            SurfaceKind::Floor,
            SurfaceKind::Ceiling,
            SurfaceKind::Wall,
            SurfaceKind::Light,
            SurfaceKind::PropFallback,
            SurfaceKind::Decal,
        ] {
            assert!(
                resolve_base_texture(&synthetic_draw(kind, 0), &materials, &table).is_none(),
                "{kind:?} must not resolve a base texture without a table"
            );
        }
    }

    #[test]
    fn an_empty_material_key_never_indexes_the_texture_table() {
        // MATERIAL_NONE is u16::MAX; the lookup must miss cleanly.
        let materials = MaterialRenderState {
            texture_slots: vec![0; 4],
            ..MaterialRenderState::default()
        };
        let table = crate::materials::MaterialTable::default();
        for kind in [SurfaceKind::Floor, SurfaceKind::Ceiling, SurfaceKind::Wall] {
            assert!(
                resolve_base_texture(&draw_of(kind, MATERIAL_NONE), &materials, &table).is_none()
            );
        }
    }

    #[test]
    fn a_shipped_material_resolves_through_its_texture_slot() {
        use crate::materials::resolve_materials;

        let json = include_str!("../../../assets/levels/places_demo.json");
        let level = crate::level::LevelDef::from_json(json).expect("valid places_demo json");
        let catalog = crate::assets::AssetCatalog::load_default();
        let root = crate::assets::resolve_asset_root().expect("assets/ is discoverable");
        let mut decode_cache = crate::materials::TextureCache::new();
        let table = resolve_materials(&level, &catalog, None, Some(&root), &mut decode_cache);
        let materials = MaterialRenderState::from_table(&table);

        let carpet = table.index_of("core:carpet_beige_01").expect("carpet");
        let carpet_texture =
            resolve_base_texture(&draw_of(SurfaceKind::Floor, carpet), &materials, &table)
                .expect("the carpet floor resolves");
        let logical = "core:tex_carpet_beige_01";
        let path = catalog.texture_path(logical).expect("carpet PNG path");
        let (expected_image, expected_key) = decode_cache
            .load_relative(&root, path, logical)
            .expect("carpet PNG decodes");
        assert_eq!(carpet_texture.key, expected_key);
        assert!(std::sync::Arc::ptr_eq(
            &carpet_texture.image,
            &expected_image
        ));

        // Two materials that share one sheet resolve to the same texture.
        let deck = table.index_of("core:pool_tile_deck_01").expect("deck");
        let wet = table.index_of("core:pool_deck_wet_01").expect("wet deck");
        assert_eq!(
            materials.texture_slots[usize::try_from(deck).unwrap_or(usize::MAX)],
            materials.texture_slots[usize::try_from(wet).unwrap_or(usize::MAX)],
            "the deck and the wet overlay share one texture slot"
        );
        let deck_texture =
            resolve_base_texture(&draw_of(SurfaceKind::Floor, deck), &materials, &table)
                .expect("the deck floor resolves");
        let wet_texture =
            resolve_base_texture(&draw_of(SurfaceKind::Floor, wet), &materials, &table)
                .expect("the wet deck floor resolves");
        assert_eq!(deck_texture.key, wet_texture.key);
    }

    #[test]
    fn the_shipped_demo_resolves_every_architectural_draw_to_a_base_texture() {
        use crate::materials::{TextureOrigin, resolve_materials};
        use crate::render::common::build_level_geometry;

        let json = include_str!("../../../assets/levels/places_demo.json");
        let level = crate::level::LevelDef::from_json(json).expect("valid places_demo json");
        let mesh = build_level_geometry(&level);
        let catalog = crate::assets::AssetCatalog::load_default();
        let root = crate::assets::resolve_asset_root().expect("assets/ is discoverable");
        let mut decode_cache = crate::materials::TextureCache::new();
        let table = resolve_materials(&level, &catalog, None, Some(&root), &mut decode_cache);
        let materials = MaterialRenderState::from_table(&table);

        let (_, draws) = pack_world_ranges(&mesh, &materials);
        assert!(
            draws.len() > 20,
            "the demo has a broad architectural draw set"
        );

        let mut distinct: Vec<&str> = Vec::new();
        let mut fixture_faces = 0usize;
        let mut fixture_housings = 0usize;
        for draw in &draws {
            // Fixture luminous faces sample their family's sheet through the
            // separate fixture-sheet path (`WorldTextures::resolve`), and the
            // housings of a material-less fixture sample the fallback sheet;
            // neither is a base-texture draw. The loader's fixture-sheet tests
            // cover that lookup.
            if draw.kind == SurfaceKind::Light {
                if draw.material == MATERIAL_NONE {
                    fixture_housings = fixture_housings.saturating_add(1);
                } else {
                    fixture_faces = fixture_faces.saturating_add(1);
                }
                continue;
            }
            // Only the architectural families are required to resolve a base
            // texture. A `PropFallback` placeholder box (a model that could
            // not be loaded) samples the fallback sheet by design, exactly
            // like a material-less architectural range; those are the class-B
            // working-tree states this test must not conflate with a material
            // resolution failure.
            if !matches!(
                draw.kind,
                SurfaceKind::Floor | SurfaceKind::Ceiling | SurfaceKind::Wall
            ) {
                continue;
            }
            let texture = resolve_base_texture(draw, &materials, &table).unwrap_or_else(|| {
                panic!(
                    "draw {:?} material {} must resolve a base texture",
                    draw.kind, draw.material
                )
            });
            assert_eq!(texture.origin, TextureOrigin::Catalog);
            if !distinct.contains(&texture.key.as_str()) {
                distinct.push(&texture.key);
            }
        }
        assert!(
            fixture_faces > 0,
            "the demo draws fixture luminous faces (a fixture-sheet draw)"
        );
        assert!(
            fixture_housings > 0,
            "the demo draws material-less fixture housings (the fallback sheet)"
        );
        assert!(
            distinct.len() >= 25,
            "the demo samples the shipped environments' base textures: {}",
            distinct.len()
        );
        assert!(
            distinct.iter().all(|key| !key.contains("normal")),
            "normal maps are not base textures: {distinct:?}"
        );
        assert!(
            distinct.len() < draws.len(),
            "surfaces share textures: {} distinct over {} draws",
            distinct.len(),
            draws.len()
        );
    }

    // -------------------------------------------------------------- shader

    #[test]
    fn the_world_shader_binds_a_base_texture_and_sampler_at_group_one() {
        assert!(WORLD_SHADER_SRC.contains("@group(1) @binding(0)"));
        assert!(WORLD_SHADER_SRC.contains("@group(1) @binding(1)"));
        assert!(WORLD_SHADER_SRC.contains("texture_2d<f32>"));
        assert!(WORLD_SHADER_SRC.contains("var base_sampler: sampler"));
        assert!(
            WORLD_SHADER_SRC.contains("textureSample(base_texture, base_sampler, in.uv)"),
            "the fragment stage must sample the base texture with the given UV"
        );
    }

    #[test]
    fn the_world_shader_binds_the_material_at_group_two() {
        assert!(WORLD_SHADER_SRC.contains("@group(2) @binding(0)"));
        assert!(WORLD_SHADER_SRC.contains("@group(2) @binding(1)"));
        assert!(WORLD_SHADER_SRC.contains("@group(2) @binding(2)"));
        assert!(WORLD_SHADER_SRC.contains("var<uniform> material: Material"));
        assert!(
            WORLD_SHADER_SRC.contains("textureSample(normal_texture, normal_sampler, in.uv)"),
            "the material normal must sample the normal map with the surface UV"
        );
    }

    #[test]
    fn the_world_shader_declares_the_material_uniform_layout() {
        // The WGSL struct order and names mirror `MaterialUniform`; the offsets
        // are pinned by `material.rs` and the field order here.
        let position = |needle: &str| WORLD_SHADER_SRC.find(needle);
        let fields = [
            "specular: vec3<f32>",
            "roughness: f32",
            "normal_strength: f32",
            "alpha_cutoff: f32",
            "opacity: f32",
            "flags: u32",
            "reflection_strength: vec3<f32>",
            "reflection_mode: u32",
        ];
        let mut last = None;
        for field in fields {
            let at = position(field).unwrap_or_else(|| panic!("missing field {field}"));
            if let Some(previous) = last {
                assert!(at > previous, "{field} must follow the previous field");
            }
            last = Some(at);
        }
    }

    #[test]
    fn the_world_shader_uses_shared_material_response_math() {
        // Every input and target is raw display space (the reference's own
        // framebuffer convention), so the only transfer function is the one
        // conversion at the sRGB surface.
        assert!(WORLD_SHADER_SRC.contains("fn target_color("));
        assert!(WORLD_SHADER_SRC.contains("fn srgb_to_linear("));
        // The fragment assembles the reference's
        // `lit + sheen + reflection + emission`, then fog, in display space and
        // converts once, after every display-space term.
        assert!(WORLD_SHADER_SRC.contains("fn lit_linear("));
        assert!(
            WORLD_SHADER_SRC.contains("return vec4<f32>(shaded.color, shaded.alpha);"),
            "the sRGB surface receives linear RGB"
        );
        assert!(
            WORLD_SHADER_SRC.contains("return vec4<f32>(shaded.color, shaded.alpha);"),
            "linear HDR targets retain the assembled radiance"
        );
        assert!(
            WORLD_SHADER_SRC.contains("lit + sheen + reflection + emission"),
            "the reference's additive order must survive"
        );
        // The normal decode is the reference's, verbatim in structure.
        for needle in [
            ".xyz * 2.0 - 1.0",
            "sampled.x * material.normal_strength",
            "sampled.y * material.normal_strength",
            "normalize(in.world_tangent - normal * dot(normal, in.world_tangent))",
            "cross(normal, tangent) * in.handedness",
            "tangent * scaled.x + bitangent * scaled.y + normal * scaled.z",
        ] {
            assert!(WORLD_SHADER_SRC.contains(needle), "missing {needle}");
        }
        // World lighting is baked: no fixture light array, no light selection
        // and no per-fixture attenuation loop. The one realtime addition is
        // the attached-light array (group 4), fixed at eight entries.
        assert!(
            !WORLD_SHADER_SRC.contains("u_lights")
                && !WORLD_SHADER_SRC.contains("light_count")
                && !WORLD_SHADER_SRC.contains("PointLight"),
            "world lighting is baked; no fixture light array may exist"
        );
        assert!(
            WORLD_SHADER_SRC.contains("lights: array<DynamicLight, 8>"),
            "the attached-light array is the one bounded realtime term"
        );
        // The reflection sampling is the reference's: one probe cubemap sample
        // and one planar projection, both under the material's reflect mode.
        // The probe read selects the packaged chain's roughness level, with
        // the two-tap fallback kept for a one-level live capture.
        assert!(WORLD_SHADER_SRC.contains("textureSampleLevel(probe_map, reflection_sampler"));
        assert!(WORLD_SHADER_SRC.contains("textureSample(planar_map, reflection_sampler"));
        assert!(WORLD_SHADER_SRC.contains("(environment.lightmap_switchable >> 16u) & 0xFu"));
        assert!(WORLD_SHADER_SRC.contains("clamp(material.roughness * probe_max_mip"));
        assert!(WORLD_SHADER_SRC.contains("if (probe_max_mip < 0.5 && material.roughness > 0.15)"));
        assert!(
            WORLD_SHADER_SRC
                .contains("environment.planar_matrix * vec4<f32>(in.world_position, 1.0)")
        );
        // And the fog term is the reference's squared-exponential, applied
        // after emission.
        assert!(WORLD_SHADER_SRC.contains("1.0 - exp(-fog_amount * fog_amount)"));
    }

    #[test]
    fn the_world_shader_has_the_two_fragment_entry_points() {
        assert!(WORLD_SHADER_SRC.contains("fn fs_main("));
        assert!(WORLD_SHADER_SRC.contains("fn fs_cutout("));
        assert!(
            WORLD_SHADER_SRC.contains("alpha < material.alpha_cutoff"),
            "the cut-out stage must discard below the material threshold"
        );
        assert!(
            WORLD_SHADER_SRC.contains("@builtin(front_facing)"),
            "a two-sided material pass must flip its normal for a back face"
        );
        assert_eq!(WORLD_FRAGMENT_ENTRY, "fs_main");
        assert_eq!(WORLD_CUTOUT_FRAGMENT_ENTRY, "fs_cutout");
        assert_eq!(WORLD_VERTEX_ENTRY, "vs_main");
    }

    #[test]
    fn the_five_pipelines_declare_the_reference_material_states() {
        let [opaque, cutout, translucent, emission, emission_cutout] =
            world_pipeline_variants(true);

        assert_eq!(opaque.label, "places-wgpu-world-opaque");
        assert_eq!(opaque.fragment_entry, WORLD_FRAGMENT_ENTRY);
        assert!(opaque.blend.is_none(), "the opaque pass does not blend");
        assert!(opaque.depth_write, "the opaque pass writes depth");
        assert_eq!(opaque.cull, Some(wgpu::Face::Back));

        assert_eq!(cutout.fragment_entry, WORLD_CUTOUT_FRAGMENT_ENTRY);
        assert!(cutout.blend.is_none(), "a cut-out does not blend");
        assert!(cutout.depth_write, "a cut-out keeps depth writes");
        assert_eq!(cutout.cull, None, "the reference never culls a pane");

        assert_eq!(translucent.fragment_entry, WORLD_FRAGMENT_ENTRY);
        assert!(!translucent.depth_write, "translucent writes no depth");
        assert_eq!(translucent.cull, None);
        let blend = translucent.blend.expect("translucent blends");
        for component in [blend.color, blend.alpha] {
            assert_eq!(component.src_factor, wgpu::BlendFactor::SrcAlpha);
            assert_eq!(component.dst_factor, wgpu::BlendFactor::OneMinusSrcAlpha);
            assert_eq!(component.operation, wgpu::BlendOperation::Add);
        }

        // The emissive pass writes no depth and never blends: it draws the
        // emissive term alone into the raw bloom source, exactly like the
        // reference's `u_emission_only` return with `depth_mask(false)`.
        assert_eq!(emission.fragment_entry, WORLD_EMISSION_FRAGMENT_ENTRY);
        assert_eq!(
            emission_cutout.fragment_entry,
            WORLD_EMISSION_CUTOUT_FRAGMENT_ENTRY
        );
        for variant in [emission, emission_cutout] {
            assert!(!variant.depth_write, "the emissive pass writes no depth");
            assert!(variant.blend.is_none(), "the emissive pass does not blend");
            assert_eq!(variant.cull, None, "the reference never culls");
        }
        assert_eq!(WORLD_EMISSION_FRAGMENT_ENTRY, "fs_emission");
        assert_eq!(WORLD_EMISSION_CUTOUT_FRAGMENT_ENTRY, "fs_emission_cutout");
    }

    // ----------------------------------------------------- translucent order

    #[test]
    fn translucent_draws_sort_farthest_first_and_skip_opaque_ranges() {
        let draws = vec![
            translucent_draw(-2.0),               // index 0, nearest
            synthetic_draw(SurfaceKind::Wall, 0), // index 1, opaque: never sorted
            translucent_draw(-10.0),              // index 2, farthest
            translucent_draw(-5.0),               // index 3, middle
        ];
        let order = translucent_order(&draws, glam::Vec3::ZERO);
        assert_eq!(order, vec![2, 3, 0]);
    }

    #[test]
    fn translucent_ties_keep_the_packed_order() {
        // Two panes at the same distance: the sort is stable, so the earlier
        // packed draw is submitted first.
        let draws = vec![
            translucent_draw(-5.0),
            translucent_draw(-5.0),
            translucent_draw(-5.0),
        ];
        let order = translucent_order(&draws, glam::Vec3::ZERO);
        assert_eq!(order, vec![0, 1, 2]);
    }

    #[test]
    fn translucent_order_ignores_empty_ranges() {
        let mut empty = translucent_draw(-5.0);
        empty.index_count = 0;
        let draws = vec![empty, translucent_draw(-1.0)];
        let order = translucent_order(&draws, glam::Vec3::ZERO);
        assert_eq!(order, vec![1]);
    }

    // ------------------------------------------------- material colour space

    #[test]
    fn the_world_shader_assembles_linear_light_and_only_encodes_raw_display_targets() {
        // The base texture is raw display space (no sRGB decode), so the
        // fragment is assembled in the reference's framebuffer space and the
        // sRGB surface converts exactly once, at the output.
        assert!(
            WORLD_SHADER_SRC.contains("let base_linear = base.rgb;"),
            "authored texels must be sampled as display values"
        );
        assert!(
            WORLD_SHADER_SRC.contains("fn target_color("),
            "no encode may exist inside the world assembly"
        );
        assert!(WORLD_SHADER_SRC.contains("return vec4<f32>(shaded.color, shaded.alpha);"));
        for needle in ["c / 12.92", "0.04045", "pow((c + 0.055) / 1.055"] {
            assert!(WORLD_SHADER_SRC.contains(needle), "missing {needle}");
        }
    }

    #[test]
    fn authored_midgrey_is_decoded_before_the_linear_factor() {
        let texture = crate::materials::color::decode_byte(128);
        assert!((texture - 0.215_860_5).abs() < 1.0e-6);
        let result = crate::materials::color::linear_to_srgb(texture * 0.5);
        assert!((result - 0.362_249_14).abs() < 1.0e-6);
        assert!(result > 128.0 / 255.0 * 0.5);
    }

    #[test]
    fn vertex_illumination_and_alpha_retain_their_numeric_range() {
        let vertex = Vertex::new([0.0; 3], [2.0, 4.0, 0.005, 0.25], [0.0; 2]);
        assert_eq!(WorldVertex::from(vertex).color, [2.0, 4.0, 0.005, 0.25]);
    }

    #[test]
    fn alpha_never_passes_through_the_colour_transfer_functions() {
        // The reference's alpha is a straight scalar product (the per-instance
        // opacity is one more scalar factor); RGB transfer functions must not
        // touch it.
        assert!(
            WORLD_SHADER_SRC.contains(
                "let alpha = base.a * in.color.a * material.opacity * environment.opacity;"
            )
        );
        assert!(
            !WORLD_SHADER_SRC.contains("srgb_to_linear(vec3<f32>(base.a"),
            "alpha must not be decoded as colour"
        );
        assert!(
            !WORLD_SHADER_SRC.contains("linear_to_srgb(vec3<f32>(alpha"),
            "alpha must not be encoded as colour"
        );
    }

    // ------------------------------------------------------------ lighting

    /// The CPU mirror of the reference's sheen term, for the tests below.
    ///
    /// `specular` and `light` are per-channel; the returned value is the
    /// display-space sheen the shader adds to the lit term.
    fn sheen(
        specular: [f32; 3],
        roughness: f32,
        normal: glam::Vec3,
        view: glam::Vec3,
        light: [f32; 3],
    ) -> [f32; 3] {
        let facing = normal.dot(view).abs().clamp(0.0, 1.0);
        let gloss = 1.0 - roughness;
        let grazing = (1.0 - facing).powf(1.0 + (16.0 - 1.0) * gloss);
        let ahead = facing.powf(1.0 + (24.0 - 1.0) * gloss) * gloss;
        let mut out = [0.0; 3];
        for channel in 0..3 {
            out[channel] = specular[channel] * (grazing * 0.55 + ahead * 0.45) * light[channel];
        }
        out
    }

    #[test]
    fn the_world_shader_declares_the_lightmap_light_seam() {
        // The reference's single light expression, reproduced: the atlas is
        // sampled only when the environment switch is on and the vertex's page
        // byte is not `LIGHTMAP_NONE`; otherwise the factor is exactly one and
        // the bake's light is already in the vertex colour.
        assert!(
            WORLD_SHADER_SRC
                .contains("fn surface_light(in: VsOut, normal: vec3<f32>) -> vec3<f32>"),
            "the light seam must take the prepared material normal"
        );
        for needle in [
            "environment.lightmap_enabled * (1.0 - step(254.5, in.lightmap_page))",
            "let page = u32(in.lightmap_page + 0.5);",
            "let pages = environment.lightmap_page_count;",
            "fn decode_lightmap(uv: vec2<f32>, layer: u32, normal: vec3<f32>) -> vec3<f32>",
            "let irradiance = textureSample(lightmap_pages, lightmap_sampler, uv, layer).rgb;",
            "let direction = textureSample(lightmap_pages, lightmap_sampler, uv, layer + 1u).rgb;",
            "let k = irradiance.r + irradiance.g + irradiance.b;",
            "let lobe = 2.0 * max(0.0, dot(direction, normal)) - length(direction);",
            "let directional = irradiance / max(k, 1.0e-6) * lobe;",
            "max(vec3<f32>(0.0), irradiance + select(vec3<f32>(0.0), directional, k > 1.0e-6))",
            "environment.lightmap_switchable & 0xFu",
            "(environment.lightmap_switchable >> 8u) & 0xFu",
            "pages * 2u * (group + 1u) + page * 2u",
            "return hdr * environment.light_scale;",
            "return vec3<f32>(1.0) * environment.light_scale;",
        ] {
            assert!(WORLD_SHADER_SRC.contains(needle), "missing {needle}");
        }
        // The octahedral decode and the interpolated-axis reconstruction are
        // gone: the stored moment planes are linear, and the only nonlinear
        // step runs on the scalar `dot(g, n)` and `|g|` of the interpolated
        // moment, never on encoded coordinates.
        assert!(!WORLD_SHADER_SRC.contains("decode_octahedral"));
        assert!(!WORLD_SHADER_SRC.contains("2.0 * max(0.0, dot(normal, axis)) - 1.0"));
        // The multiply happens in display space, where the reference did it,
        // and the sheen is scaled by the same factor, so a dark room darkens
        // the sheen.
        assert!(
            WORLD_SHADER_SRC
                .contains("base_linear * vertex_color * light * (1.0 - emission_vertex)")
        );
        assert!(
            WORLD_SHADER_SRC
                .contains("material.specular * (grazing * 0.55 + ahead * 0.45) * light")
        );
        // The sheen is gated by the response bit and reads the camera position
        // the reference called `u_camera_pos`.
        assert!(WORLD_SHADER_SRC.contains("MATERIAL_FLAG_RESPONSE_ENABLED) == 0u"));
        assert!(WORLD_SHADER_SRC.contains("normalize(camera.position - in.world_position)"));
        // No shadow resource: the reference has none.
        assert!(!WORLD_SHADER_SRC.contains("texture_depth_2d"));
        assert!(!WORLD_SHADER_SRC.contains("comparison"));
        assert!(!WORLD_SHADER_SRC.contains("@group(3) @binding(7)"));
        // The environment declares the pair array and its sampler at the
        // frozen bindings: the page array at 1, the sampler at 3, and no
        // second page binding.
        assert!(WORLD_SHADER_SRC.contains("var lightmap_pages: texture_2d_array<f32>"));
        assert!(WORLD_SHADER_SRC.contains("var lightmap_sampler: sampler"));
        assert!(!WORLD_SHADER_SRC.contains("var lightmap0"));
        assert!(!WORLD_SHADER_SRC.contains("var lightmap1"));
        assert!(!WORLD_SHADER_SRC.contains("@group(3) @binding(2)"));
    }

    /// Material evaluation must receive unclipped irradiance.
    #[test]
    fn light_is_not_display_mapped_before_albedo() {
        assert!(!WORLD_SHADER_SRC.contains("fn soft_clip("));
        assert!(WORLD_SHADER_SRC.contains("return hdr * environment.light_scale;"));
        let texture = crate::materials::color::decode_byte(128);
        let linear = texture * 4.0;
        assert!(linear > 0.8);
        assert!((linear - 0.863_442).abs() < 1.0e-5);
        assert!(crate::materials::color::linear_to_srgb(linear) > 0.93);
    }

    #[test]
    fn the_environment_uniform_matches_the_wgsl_layout() {
        assert_eq!(std::mem::size_of::<EnvironmentUniform>(), 2896);
        assert_eq!(std::mem::align_of::<EnvironmentUniform>(), 16);
        assert_eq!(ENVIRONMENT_UNIFORM_SIZE, 2896);
        assert_eq!(std::mem::offset_of!(EnvironmentUniform, light_scale), 0);
        assert_eq!(
            std::mem::offset_of!(EnvironmentUniform, lightmap_enabled),
            12
        );
        assert_eq!(std::mem::offset_of!(EnvironmentUniform, fog_color), 16);
        assert_eq!(std::mem::offset_of!(EnvironmentUniform, fog_density), 28);
        assert_eq!(
            std::mem::offset_of!(EnvironmentUniform, fog_reference_y),
            32
        );
        assert_eq!(
            std::mem::offset_of!(EnvironmentUniform, fog_height_gain),
            36
        );
        assert_eq!(
            std::mem::offset_of!(EnvironmentUniform, lightmap_page_count),
            40
        );
        assert_eq!(
            std::mem::offset_of!(EnvironmentUniform, lightmap_switchable),
            44
        );
        assert_eq!(std::mem::offset_of!(EnvironmentUniform, planar_matrix), 48);
        assert_eq!(std::mem::offset_of!(EnvironmentUniform, planar_plane), 112);
        assert_eq!(std::mem::offset_of!(EnvironmentUniform, model), 128);
        assert_eq!(std::mem::offset_of!(EnvironmentUniform, opacity), 192);
        // The two lightmap words occupy the bytes the padding used to, and the
        // per-instance opacity follows the model matrix: the matrix offsets are
        // unchanged from the 192-byte layout.
        assert_eq!(
            std::mem::offset_of!(EnvironmentUniform, planar_matrix),
            std::mem::offset_of!(EnvironmentUniform, lightmap_switchable) + 4
        );
        // The static default and the static builder keep 1.0, so the world's
        // alpha, emission and bypass path are bit-identical.
        let default = EnvironmentUniform::new(
            [1.0; 3],
            true,
            crate::render::common::atmosphere::FogState::SHIPPED,
        );
        assert_exact(default.opacity, 1.0);
        assert_exact(default.with_opacity(1.0).opacity, 1.0);
        assert_exact(default.with_opacity(0.25).opacity, 0.25);
        // The clamp is `0..=1`, and a non-finite fade is fully opaque.
        assert_exact(default.with_opacity(2.0).opacity, 1.0);
        assert_exact(default.with_opacity(-1.0).opacity, 0.0);
        assert_exact(default.with_opacity(f32::NAN).opacity, 1.0);
        assert_exact(default.with_opacity(f32::INFINITY).opacity, 1.0);
        // The WGSL struct declares the same field order and the same pack.
        assert!(WORLD_SHADER_SRC.contains("light_scale: vec3<f32>"));
        assert!(WORLD_SHADER_SRC.contains("lightmap_enabled: f32"));
        assert!(WORLD_SHADER_SRC.contains("lightmap_page_count: u32"));
        assert!(WORLD_SHADER_SRC.contains("lightmap_switchable: u32"));
        assert!(WORLD_SHADER_SRC.contains("bits 16..=19"));
        assert!(!WORLD_SHADER_SRC.contains("_padding: vec2<f32>"));
        assert!(WORLD_SHADER_SRC.contains("planar_matrix: mat4x4<f32>"));
        assert!(WORLD_SHADER_SRC.contains("planar_plane: vec4<f32>"));
        assert!(WORLD_SHADER_SRC.contains("model: mat4x4<f32>"));
        assert!(WORLD_SHADER_SRC.contains("opacity: f32"));
        // The instance opacity reaches both the fragment alpha and the
        // emissive term; the static world's 1.0 leaves both unchanged.
        assert!(
            WORLD_SHADER_SRC.contains("material.opacity * environment.opacity"),
            "shade() must fold the instance opacity into the fragment alpha"
        );
        assert!(
            WORLD_SHADER_SRC.contains("surface_emission(in, base.rgb) * environment.opacity"),
            "the emissive term must fade with the instance opacity"
        );
    }

    /// The lightmap and probe words pack into the same switchable word at the
    /// four bits each the shader reads.
    #[test]
    fn the_lightmap_and_probe_words_pack_into_the_switchable_word() {
        // The builder packs the four-bit count and mask.
        let environment = EnvironmentUniform::new(
            [1.0; 3],
            true,
            crate::render::common::atmosphere::FogState::SHIPPED,
        )
        .with_lightmaps(3, 2, 0b01);
        assert_eq!(environment.lightmap_page_count, 3);
        assert_eq!(environment.lightmap_switchable & 0xF, 2);
        assert_eq!((environment.lightmap_switchable >> 8_i32) & 0xF, 0b01);
        let clamped = EnvironmentUniform::new(
            [1.0; 3],
            true,
            crate::render::common::atmosphere::FogState::SHIPPED,
        )
        .with_lightmaps(3, 0xFFFF_FFFF, 0xFFFF_FFFF);
        assert_eq!(clamped.lightmap_switchable, 0x0000_0F0F);
        // The probe mip bits live in the same word without disturbing the
        // lightmap fields: bits 16..=19, clamped to four bits.
        let with_probes = EnvironmentUniform::new(
            [1.0; 3],
            true,
            crate::render::common::atmosphere::FogState::SHIPPED,
        )
        .with_lightmaps(3, 2, 0b01)
        .with_probe_mips(6);
        assert_eq!(with_probes.lightmap_switchable & 0xF, 2);
        assert_eq!((with_probes.lightmap_switchable >> 8_i32) & 0xF, 0b01);
        assert_eq!((with_probes.lightmap_switchable >> 16_i32) & 0xF, 6);
        let clamped_probes = with_probes.with_probe_mips(0xFFFF_FFFF);
        assert_eq!((clamped_probes.lightmap_switchable >> 16_i32) & 0xF, 0xF);
        assert_eq!(
            clamped_probes.lightmap_switchable & 0x0000_FFFF,
            0x0000_0102
        );
    }

    /// The regional fog uniform block: 64-byte entries at the tail of the
    /// environment uniform before the entity payload, with a count bounding the
    /// shader loop and the
    /// zeroed default for a level that authors no region.
    #[test]
    fn the_fog_region_uniform_matches_the_wgsl_layout() {
        // The block is appended after the historical 208 bytes: the count
        // word, three padding words (the array is 16-byte aligned) and then
        // the fixed 16-entry array, 64 bytes each.
        assert_eq!(
            std::mem::offset_of!(EnvironmentUniform, fog_region_count),
            208
        );
        assert_eq!(std::mem::offset_of!(EnvironmentUniform, fog_regions), 224);
        assert_eq!(std::mem::size_of::<FogRegionUniform>(), 64);
        assert_eq!(std::mem::align_of::<FogRegionUniform>(), 16);
        assert_eq!(std::mem::offset_of!(FogRegionUniform, min_falloff), 0);
        assert_eq!(std::mem::offset_of!(FogRegionUniform, max_top), 16);
        assert_eq!(std::mem::offset_of!(FogRegionUniform, color_density), 32);
        assert_eq!(std::mem::offset_of!(FogRegionUniform, ground_pad), 48);
        assert_eq!(
            std::mem::offset_of!(EnvironmentUniform, fog_regions)
                + 16 * std::mem::size_of::<FogRegionUniform>(),
            std::mem::offset_of!(EnvironmentUniform, entity_irradiance)
        );
        assert_eq!(
            std::mem::offset_of!(EnvironmentUniform, entity_irradiance),
            1248
        );
        assert_eq!(
            std::mem::offset_of!(EnvironmentUniform, entity_moment),
            1264
        );
        assert_eq!(std::mem::size_of::<EnvironmentUniform>(), 2896);
        // The static environment carries no regions and a zeroed array: the
        // historical uniform without a level authoring any.
        let default = EnvironmentUniform::new(
            [1.0; 3],
            true,
            crate::render::common::atmosphere::FogState::SHIPPED,
        );
        assert_eq!(default.fog_region_count, 0);
        assert_eq!(
            default.fog_regions,
            [FogRegionUniform::ZERO; crate::level::MAX_FOG_REGIONS]
        );
        // The WGSL struct declares the same field order and the same pack.
        assert!(WORLD_SHADER_SRC.contains("fog_region_count: u32"));
        assert!(WORLD_SHADER_SRC.contains("fog_regions: array<FogRegion, 16u>"));
        assert!(WORLD_SHADER_SRC.contains("min_falloff: vec4<f32>"));
        assert!(WORLD_SHADER_SRC.contains("max_top: vec4<f32>"));
        assert!(WORLD_SHADER_SRC.contains("color_density: vec4<f32>"));
        assert!(WORLD_SHADER_SRC.contains("ground_pad: vec4<f32>"));
    }

    /// The regional fog pack: the live entries first, the count word bounding
    /// the loop, unused slots zero, and the preset caps exactly 2/8/16.
    #[test]
    fn fog_regions_pack_in_authoring_order_bounded_by_the_preset_cap() {
        use crate::render::common::atmosphere::{FogRegion, LevelFog, fog_region_cap};
        let regions: Vec<FogRegion> = (0..crate::level::MAX_FOG_REGIONS)
            .map(|index| {
                let sample_index = f32::from(u8::try_from(index).unwrap_or(u8::MAX));
                FogRegion {
                    min: [sample_index, sample_index + 1.0, sample_index + 2.0],
                    max: [sample_index + 10.0, sample_index + 4.0, sample_index + 8.0],
                    density: 0.01 * (sample_index + 1.0),
                    color: [0.1 * sample_index, 0.2, 0.3],
                    falloff_m: 2.0,
                    ground_y: sample_index,
                    top_y: sample_index + 1.0,
                }
            })
            .collect();
        let level_fog = LevelFog {
            global: crate::render::common::atmosphere::FogState::SHIPPED,
            regions,
        };
        let low = EnvironmentUniform::new(
            [1.0; 3],
            true,
            crate::render::common::atmosphere::FogState::SHIPPED,
        )
        .with_fog_regions(
            &level_fog.regions,
            fog_region_cap(crate::quality::QualityLevel::Low),
        );
        assert_eq!(low.fog_region_count, 2);
        assert_eq!(low.fog_regions[0].color_density[3], 0.01);
        assert_eq!(low.fog_regions[1].color_density[3], 0.02);
        assert_eq!(low.fog_regions[2], FogRegionUniform::ZERO);

        let high = EnvironmentUniform::new(
            [1.0; 3],
            true,
            crate::render::common::atmosphere::FogState::SHIPPED,
        )
        .with_fog_regions(
            &level_fog.regions,
            fog_region_cap(crate::quality::QualityLevel::High),
        );
        assert_eq!(high.fog_region_count, 16);
        // The prefix the low preset uploaded is bit-identical in both packs:
        // a preset change only moves the count and the slots beyond it.
        for slot in 0..2 {
            assert_eq!(low.fog_regions[slot], high.fog_regions[slot]);
        }
        assert_eq!(high.fog_regions[15].min_falloff, [15.0, 16.0, 17.0, 2.0]);
        assert_eq!(high.fog_regions[15].max_top, [25.0, 19.0, 23.0, 16.0]);
        assert_eq!(high.fog_regions[15].ground_pad, [15.0, 0.0, 0.0, 0.0]);

        // A cap of zero (or a level with no regions) clears everything.
        let none = EnvironmentUniform::new(
            [1.0; 3],
            true,
            crate::render::common::atmosphere::FogState::SHIPPED,
        )
        .with_fog_regions(&level_fog.regions, 0);
        assert_eq!(none.fog_region_count, 0);
        assert_eq!(none.fog_regions, [FogRegionUniform::ZERO; 16]);
    }

    /// The world shader parses and validates as WGSL with no GPU adapter: the
    /// offline half of the pipeline check, including the uniform structs the
    /// environment layout mirrors.
    #[test]
    fn the_world_shader_is_valid_wgsl() {
        use wgpu::naga::valid::{Capabilities, ValidationFlags, Validator};
        let module = wgpu::naga::front::wgsl::parse_str(WORLD_SHADER_SRC)
            .expect("the world shader parses as WGSL");
        drop(
            Validator::new(ValidationFlags::all(), Capabilities::all())
                .validate(&module)
                .expect("the world shader validates"),
        );
        assert!(
            module
                .entry_points
                .iter()
                .any(|point| point.name == WORLD_FRAGMENT_ENTRY),
            "the world shader has its main fragment entry point"
        );
        assert!(
            module
                .entry_points
                .iter()
                .any(|point| point.name == WORLD_VERTEX_ENTRY),
            "the world shader has its vertex entry point"
        );
    }

    #[test]
    fn the_dynamic_lights_uniform_matches_the_wgsl_layout() {
        // One light is 32 bytes: vec3 + f32, twice.
        assert_eq!(std::mem::size_of::<GpuDynamicLight>(), 32);
        assert_eq!(std::mem::offset_of!(GpuDynamicLight, position), 0);
        assert_eq!(std::mem::offset_of!(GpuDynamicLight, radius), 12);
        assert_eq!(std::mem::offset_of!(GpuDynamicLight, color), 16);
        assert_eq!(std::mem::offset_of!(GpuDynamicLight, intensity), 28);
        // The uniform is the count word plus three pads, then the array.
        assert_eq!(std::mem::align_of::<DynamicLightsUniform>(), 16);
        assert_eq!(
            std::mem::size_of::<DynamicLightsUniform>(),
            16 + 32 * MAX_DYNAMIC_LIGHTS
        );
        assert_eq!(
            DYNAMIC_LIGHTS_UNIFORM_SIZE,
            u64::try_from(16 + 32 * MAX_DYNAMIC_LIGHTS).unwrap()
        );
        assert_eq!(std::mem::offset_of!(DynamicLightsUniform, count), 0);
        assert_eq!(std::mem::offset_of!(DynamicLightsUniform, padding), 4);
        assert_eq!(std::mem::offset_of!(DynamicLightsUniform, lights), 16);

        // An empty set packs a zero count and zeroed slots.
        let empty = DynamicLightsUniform::new(&DynamicLightSet::new());
        assert_eq!(empty.count, 0);
        assert!(
            empty
                .lights
                .iter()
                .all(|light| *light == GpuDynamicLight::zeroed())
        );

        // A live set packs in insertion order; every unused slot stays zero.
        let mut set = DynamicLightSet::new();
        assert!(
            set.insert(crate::render::common::dynamic_lights::DynamicLight {
                key: "glow:a".to_string(),
                position: glam::Vec3::new(1.0, 2.0, 3.0),
                color: [0.1, 0.2, 0.3],
                intensity: 2.0,
                radius: 4.0,
            })
        );
        let uniform = DynamicLightsUniform::new(&set);
        assert_eq!(uniform.count, 1);
        assert_eq!(
            uniform.lights[0],
            GpuDynamicLight {
                position: [1.0, 2.0, 3.0],
                radius: 4.0,
                color: [0.1, 0.2, 0.3],
                intensity: 2.0,
            }
        );
        assert_eq!(uniform.lights[1], GpuDynamicLight::zeroed());

        // The WGSL mirrors the struct and the term's falloff.
        for needle in [
            "struct DynamicLight",
            "position: vec3<f32>",
            "radius: f32",
            "color: vec3<f32>",
            "intensity: f32",
            "count: u32",
            "_pad0: u32",
            "lights: array<DynamicLight, 8>",
            "@group(4) @binding(0)",
            "fn dynamic_light_term(",
            "window * window",
        ] {
            assert!(WORLD_SHADER_SRC.contains(needle), "missing {needle}");
        }
        // The emissive pass must stay free of the dynamic term: bloom must not
        // double the pool.
        let emissive = WORLD_SHADER_SRC
            .split("fn emissive_only(")
            .nth(1)
            .unwrap_or("");
        assert!(
            !emissive.contains("dynamic_light_term"),
            "the emissive pass must not sum the attached lights"
        );
    }

    #[test]
    fn translucent_characters_sort_farthest_first_with_an_index_tie_break() {
        let entries = [
            (7, glam::Vec3::new(0.0, 0.0, -2.0)),  // 2 m from the eye
            (2, glam::Vec3::new(0.0, 0.0, -10.0)), // 10 m
            (5, glam::Vec3::new(0.0, 0.0, -5.0)),  // 5 m
        ];
        assert_eq!(
            translucent_character_order(&entries, glam::Vec3::ZERO),
            vec![2, 5, 7]
        );
        // Equal distances keep the scene index order whatever the input order.
        let tied = [
            (3, glam::Vec3::new(1.0, 0.0, 0.0)),
            (1, glam::Vec3::new(-1.0, 0.0, 0.0)),
            (2, glam::Vec3::new(0.0, 1.0, 0.0)),
        ];
        assert_eq!(
            translucent_character_order(&tied, glam::Vec3::ZERO),
            vec![1, 2, 3]
        );
        assert_eq!(
            translucent_character_order(&[], glam::Vec3::ZERO),
            [0_usize; 0]
        );
    }

    /// The group-4 write path against a real queue: the live set lands in the
    /// buffer, and an empty set overwrites it with a zero count so no stale
    /// frame lingers. Skips cleanly on a host with no adapter.
    #[test]
    fn update_lights_writes_the_uniform_contents_when_an_adapter_is_available() {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let Ok(adapter) =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: None,
                ..Default::default()
            }))
        else {
            println!("[test] no GPU adapter; skipping the update_lights buffer test");
            return;
        };
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("dynamic-lights-buffer-test"),
            required_limits: crate::render::wgpu::renderer::world_device_limits(),
            ..Default::default()
        }))
        .expect("GPU device");
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("dynamic-lights-test"),
            size: DYNAMIC_LIGHTS_UNIFORM_SIZE,
            usage: wgpu::BufferUsages::UNIFORM
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let readback = |label: &str| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: DYNAMIC_LIGHTS_UNIFORM_SIZE,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let read = |staging: &wgpu::Buffer| -> DynamicLightsUniform {
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("dynamic-lights-readback"),
            });
            encoder.copy_buffer_to_buffer(&buffer, 0, staging, 0, DYNAMIC_LIGHTS_UNIFORM_SIZE);
            let _submission = queue.submit([encoder.finish()]);
            let slice = staging.slice(..);
            let (sender, receiver) = std::sync::mpsc::channel();
            slice.map_async(wgpu::MapMode::Read, move |result| {
                sender
                    .send(result)
                    .expect("read-back receiver remains live until its callback");
            });
            let _poll_status = device
                .poll(wgpu::PollType::wait_indefinitely())
                .expect("GPU work completes before test read-back");
            receiver.recv().expect("map callback").expect("buffer maps");
            let data = slice.get_mapped_range().expect("mapped range");
            let uniform = *bytemuck::from_bytes::<DynamicLightsUniform>(&data);
            drop(data);
            staging.unmap();
            uniform
        };

        let mut set = DynamicLightSet::new();
        assert!(
            set.insert(crate::render::common::dynamic_lights::DynamicLight {
                key: "glow:a".to_string(),
                position: glam::Vec3::new(-1.5, 0.5, 2.0),
                color: [0.45, 0.95, 1.0],
                intensity: 0.6,
                radius: 3.5,
            })
        );
        write_dynamic_lights(&queue, &buffer, &set);
        let written = read(&readback("dynamic-lights-live"));
        assert_eq!(written.count, 1);
        assert_exact(written.lights[0].intensity, 0.6);
        assert_exact_array(written.lights[0].color, [0.45, 0.95, 1.0]);
        assert_eq!(written.lights[0].position, [-1.5, 0.5, 2.0]);

        // The clearing write: an empty set must reach the buffer too.
        write_dynamic_lights(&queue, &buffer, &DynamicLightSet::new());
        let cleared = read(&readback("dynamic-lights-cleared"));
        assert_eq!(cleared.count, 0);
        assert_eq!(cleared.lights[0], GpuDynamicLight::zeroed());
    }

    /// The world shader compiles against the five-group pipeline layout, and
    /// every variant is created: the group-4 addition and the new environment
    /// field must be valid WGSL and layout-compatible. Skips cleanly on a host
    /// with no adapter.
    #[test]
    fn the_world_shader_compiles_against_the_five_group_layout_when_an_adapter_is_available() {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let Ok(adapter) =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: None,
                ..Default::default()
            }))
        else {
            println!("[test] no GPU adapter; skipping the world shader compile test");
            return;
        };
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("world-shader-compile-test"),
            required_limits: crate::render::wgpu::renderer::world_device_limits(),
            ..Default::default()
        }))
        .expect("GPU device");
        let buffer_entry = |binding: u32, visibility: wgpu::ShaderStages| {
            wgpu::BindGroupLayoutEntry {
                binding,
                visibility,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    // Pipeline creation checks binding types, not sizes; the
                    // real layouts pin their own minimums.
                    min_binding_size: None,
                },
                count: None,
            }
        };
        let texture = |binding: u32| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let sampler = |binding: u32| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        };
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("world-shader-test-texture"),
            entries: &[texture(0), sampler(1)],
        });
        let material_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("world-shader-test-material"),
            entries: &[
                buffer_entry(0, wgpu::ShaderStages::FRAGMENT),
                texture(1),
                sampler(2),
                texture(3),
                sampler(4),
            ],
        });
        let environment_layout = environment_bind_group_layout(&device);
        let pipeline = WorldPipeline::with_state(
            &device,
            wgpu::TextureFormat::Rgba8Unorm,
            &texture_layout,
            &material_layout,
            &environment_layout,
            wgpu::FrontFace::Ccw,
            false,
            false,
        );
        assert_eq!(pipeline.format(), wgpu::TextureFormat::Rgba8Unorm);
        // The group-4 buffer is sized for the WGSL `DynamicLights` struct;
        // a mismatch would be caught by binding-time validation.
        assert_eq!(pipeline.lights_buffer.size(), DYNAMIC_LIGHTS_UNIFORM_SIZE);
        // The real upload path runs against the real pipeline's buffer.
        let mut lights = DynamicLightSet::new();
        assert!(
            lights.insert(crate::render::common::dynamic_lights::DynamicLight {
                key: "glow:pumpkin".to_string(),
                position: glam::Vec3::new(0.0, 1.0, 0.0),
                color: [1.0, 0.86, 0.6],
                intensity: 1.0,
                radius: 3.0,
            })
        );
        pipeline.update_lights(&queue, &lights);
        let _poll_status = device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("GPU work completes before test read-back");
    }

    /// The CPU mirror of the shader's `decode_lightmap` over the layer array:
    /// the irradiance plane at `layer` and the direction-moment plane after it,
    /// with `k = I.r + I.g + I.b` and
    /// `max(0, I + (I / max(k, 1e-6)) * (2 * max(0, dot(g, n)) - |g|))`; the
    /// planes' alpha channels are reserved and ignored. `layers` is the array's
    /// flat per-layer sample list, exactly as the texture holds it.
    fn decode_lightmap(layer: usize, normal: [f32; 3], layers: &[[f32; 4]]) -> [f32; 3] {
        let irradiance = layers.get(layer).copied().unwrap_or([0.0; 4]);
        let direction = layers
            .get(layer.saturating_add(1))
            .copied()
            .unwrap_or([0.0; 4]);
        let k = irradiance[0] + irradiance[1] + irradiance[2];
        let dot = normal[0] * direction[0] + normal[1] * direction[1] + normal[2] * direction[2];
        let length = (direction[0] * direction[0]
            + direction[1] * direction[1]
            + direction[2] * direction[2])
            .sqrt();
        let lobe = 2.0 * dot.max(0.0) - length;
        std::array::from_fn(|channel| {
            let directional = if k > 1.0e-6 {
                irradiance[channel] / k * lobe
            } else {
                0.0
            };
            (irradiance[channel] + directional).max(0.0)
        })
    }

    /// A CPU mirror of the shader's `surface_light`, for the layer/addressing,
    /// reconstruction and switchable-mask contract.
    ///
    /// `layers` is the array's flat per-layer sample list in upload order; an
    /// index outside it is not producible by a successful plan, so the mirror
    /// decodes a black plane (never an undefined sample).
    #[expect(
        clippy::too_many_arguments,
        reason = "one shader seam, every uniform explicit"
    )] // one shader seam, every uniform explicit
    fn surface_light_mirror(
        enabled: bool,
        page: f32,
        page_count: usize,
        switchable_count: u32,
        switchable_mask: u32,
        normal: [f32; 3],
        layers: &[[f32; 4]],
        scale: [f32; 3],
    ) -> [f32; 3] {
        let on =
            if enabled { 1.0_f64 } else { 0.0_f64 } * if page >= 254.5 { 0.0_f64 } else { 1.0_f64 };
        if on <= 0.5_f64 {
            return scale;
        }
        // Test mirror of the shader's `u32(in.lightmap_page + 0.5)`: the page
        // byte is finite and checked non-negative, and the test only feeds page
        // bytes in [0, 4), so the truncating cast is exact.
        let page_byte = page + 0.5;
        #[expect(
            clippy::as_conversions,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "This isolated shader mirror casts the finite nonnegative encoded page number; fixture indices reproduce the shader contract."
        )]
        let page_index = if page_byte.is_finite() && page_byte >= 0.0 {
            page_byte as usize
        } else {
            usize::MAX
        };
        let mut hdr = decode_lightmap(page_index * 2, normal, layers);
        let count = switchable_count & 0xF;
        let mask = switchable_mask & 0xF;
        for group in 0..count {
            if mask & (1u32 << group) != 0 {
                let layer = page_count
                    .saturating_mul(2)
                    .saturating_mul(usize::try_from(group).unwrap_or(0).saturating_add(1))
                    .saturating_add(page_index.saturating_mul(2));
                let contribution = decode_lightmap(layer, normal, layers);
                hdr = std::array::from_fn(|channel| hdr[channel] + contribution[channel]);
            }
        }
        std::array::from_fn(|channel| hdr[channel] * scale[channel])
    }

    #[test]
    fn the_light_seam_reconstructs_and_selects_groups_like_the_reference() {
        // The exact layer layout `LevelLightmaps::irradiance_layer` produces
        // for two pages and two switchable groups: each page is an irradiance
        // plane then a direction-moment plane, base pages first (irradiance at
        // 0 and 2), group 0's page planes at 4/6, group 1's at 8/10. The alpha
        // channels are reserved (`0.5`) and the reconstruction ignores them.
        let reserved = 0.5;
        let layers: Vec<[f32; 4]> = vec![
            // Base page 0: irradiance, zero moment.
            [0.1, 0.1, 0.1, reserved],
            [0.0, 0.0, 0.0, reserved],
            // Base page 1: irradiance and a +Y moment of 0.3.
            [0.2, 0.2, 0.2, reserved],
            [0.0, 0.3, 0.0, reserved],
            // Group 0, page 0 then page 1.
            [0.05, 0.05, 0.05, reserved],
            [0.0, 0.0, 0.0, reserved],
            [0.06, 0.06, 0.06, reserved],
            [0.0, 0.0, 0.0, reserved],
            // Group 1, page 0 then page 1.
            [0.02, 0.02, 0.02, reserved],
            [0.0, 0.0, 0.0, reserved],
            [0.03, 0.03, 0.03, reserved],
            [0.0, 0.0, 0.0, reserved],
        ];
        let unit = [1.0; 3];
        let up = [0.0, 1.0, 0.0];
        let down = [0.0, -1.0, 0.0];
        let close = |actual: [f32; 3], expected: [f32; 3]| {
            for channel in 0..3 {
                assert!(
                    (actual[channel] - expected[channel]).abs() < 1.0e-6,
                    "{actual:?} vs {expected:?}"
                );
            }
        };

        // Every stamped page byte addresses its own pair within the base group,
        // and the reconstruction reads the moment vector at the material
        // normal: base page 1 reconstructs
        // `max(0, 0.2 + (0.2 / 0.6) * 0.3) = 0.3` on every channel, because
        // `g = (0, 0.3, 0)` dots to `0.3` and each channel scales by its own
        // share of the mean irradiance.
        close(
            surface_light_mirror(true, 0.0, 2, 0, 0, up, &layers, unit),
            [0.1; 3],
        );
        close(
            surface_light_mirror(true, 1.0, 2, 0, 0, up, &layers, unit),
            [0.3; 3],
        );
        // A surface facing away from the moment loses the directional part
        // without a negative draw.
        close(
            surface_light_mirror(true, 1.0, 2, 0, 0, down, &layers, unit),
            [0.1; 3],
        );
        // The base group alone stays the reference: it holds every switchable
        // fixture's contribution out.
        close(
            surface_light_mirror(true, 0.0, 2, 2, 0b00, up, &layers, unit),
            [0.1; 3],
        );
        // Bit `g` of the mask adds exactly group `g` in page order.
        close(
            surface_light_mirror(true, 0.0, 2, 2, 0b01, up, &layers, unit),
            [0.15; 3],
        );
        close(
            surface_light_mirror(true, 0.0, 2, 2, 0b10, up, &layers, unit),
            [0.12; 3],
        );
        close(
            surface_light_mirror(true, 0.0, 2, 2, 0b11, up, &layers, unit),
            [0.17; 3],
        );
        // Page byte 1 inside group 0 addresses that group's page 1: 0.3 + 0.06.
        close(
            surface_light_mirror(true, 1.0, 2, 2, 0b01, up, &layers, unit),
            [0.36; 3],
        );
        // A count of one means group 1 does not exist: its mask bit is read but
        // the loop never reaches it.
        close(
            surface_light_mirror(true, 0.0, 2, 1, 0b10, up, &layers, unit),
            [0.1; 3],
        );
        // `LIGHTMAP_NONE` (and anything >= 254.5) keeps the vertex-lit unit
        // factor, which is what makes the historical path exact; the global
        // switch closes the atlas for every vertex.
        assert_eq!(
            surface_light_mirror(true, 255.0, 2, 2, 0b11, up, &layers, unit),
            unit
        );
        assert_eq!(
            surface_light_mirror(false, 0.0, 2, 2, 0b11, up, &layers, unit),
            unit
        );
        assert_eq!(
            surface_light_mirror(false, 255.0, 2, 2, 0b11, up, &layers, unit),
            unit
        );
        // The per-object scale multiplies whichever path was taken.
        close(
            surface_light_mirror(true, 0.0, 2, 0, 0, up, &layers, [0.5; 3]),
            [0.05; 3],
        );
        close(
            surface_light_mirror(true, 1.0, 2, 0, 0, up, &layers, [2.0; 3]),
            [0.6; 3],
        );
        close(
            surface_light_mirror(true, 0.0, 2, 0, 0, up, &layers, [2.0; 3]),
            [0.2; 3],
        );
        assert_eq!(
            surface_light_mirror(true, 255.0, 2, 0, 0, up, &layers, [2.0; 3]),
            [2.0; 3]
        );

        // Highlights retain HDR energy until presentation.
        let bright: Vec<[f32; 4]> = vec![[2.0, 2.0, 2.0, 0.5], [0.0, 0.0, 0.0, 0.5]];
        let expected = [2.0; 3];
        let actual = surface_light_mirror(true, 0.0, 1, 0, 0, up, &bright, unit);
        close(actual, expected);
        assert!(actual[0] == 2.0, "{actual:?}");

        // The shader source contains the exact expressions the mirror encodes.
        assert!(WORLD_SHADER_SRC.contains("step(254.5, in.lightmap_page)"));
        assert!(WORLD_SHADER_SRC.contains("let page = u32(in.lightmap_page + 0.5);"));
        assert!(WORLD_SHADER_SRC.contains("pages * 2u * (group + 1u) + page * 2u"));
        assert!(WORLD_SHADER_SRC.contains("return hdr * environment.light_scale;"));
    }

    /// The runtime-side regression for the octahedral-seam artifact: two
    /// adjacent texels whose *old* octahedral encodings straddled the stored
    /// `x = 0.5` seam while their physical directions were nearly equal. The
    /// hardware bilinearly interpolates whatever is stored; under the old
    /// representation that blended the folded axis coordinates, so decoding
    /// the blended pair swept through the fold and collapsed towards black in
    /// the middle of the span (the dark contour the artifact report
    /// describes). The new representation stores linear irradiance and a
    /// linear moment vector, and its only nonlinear step runs on the scalar
    /// of the interpolated moment, so reconstructing the interpolated pair
    /// sweeps a bounded interval.
    ///
    /// The simulation mirrors the WGSL exactly: the interpolated planes feed
    /// `decode_lightmap` in `world.wgsl`, whose body is
    /// `max(0, I + (I / max(k, 1e-6)) * (2 * max(0, dot(g, n)) - |g|))`.
    #[test]
    fn the_moment_reconstruction_interpolates_smoothly_across_the_old_octahedral_seam() {
        // The two stored encodings measured in the artifact evidence, both
        // decoding to a direction near `(0, -0.1, -0.99)`: the stored `x`
        // jumps from 0.928 to 0.048 across the seam at 0.5.
        let old_axis = [[0.928_f32, 0.048], [0.048, 0.048]];
        // The old shader's octahedral decode, kept here only as the measured
        // counter-example (neither it nor its formula exist in the shader any
        // more).
        let old_decode = |axis: [f32; 2]| -> [f32; 3] {
            let p = [
                axis[0].clamp(0.0, 1.0) * 2.0 - 1.0,
                axis[1].clamp(0.0, 1.0) * 2.0 - 1.0,
            ];
            let z = 1.0 - p[0].abs() - p[1].abs();
            if z < 0.0 {
                let sign_x = if p[0] >= 0.0 { 1.0 } else { -1.0 };
                let sign_y = if p[1] >= 0.0 { 1.0 } else { -1.0 };
                [(1.0 - p[1].abs()) * sign_x, (1.0 - p[0].abs()) * sign_y, z]
            } else {
                [p[0], p[1], z]
            }
        };
        let normalize = |v: [f32; 3]| {
            let length = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
            [v[0] / length, v[1] / length, v[2] / length]
        };
        // The physical directions the two texels encode, from the old decode.
        let omega_a = normalize(old_decode(old_axis[0]));
        let omega_b = normalize(old_decode(old_axis[1]));
        // A single shared direction stores `g = k * omega` (the interface's
        // moment identity), with `k = I.r + I.g + I.b = 1.5` on both texels;
        // the old representation additionally stored the per-channel amplitude
        // `A = I` (amplitude was not the artifact's carrier).
        let irradiance = [0.5_f32; 3];
        let k = 1.5_f32;
        let moment_a = omega_a.map(|channel| channel * k);
        let moment_b = omega_b.map(|channel| channel * k);
        // A wall facing the light: the moment points at -Z.
        let normal = [0.0_f32, 0.0, -1.0];

        let mut new_min = f32::MAX;
        let mut new_max = f32::MIN;
        let mut old_min = f32::MAX;
        let mut old_max = f32::MIN;
        for step in 0_i32..=64_i32 {
            // 65 exact sample positions
            let t = crate::test_support::exact_f32(step) / 64.0;
            // The blend the hardware performs: every stored value interpolates
            // linearly, so the sampled planes are the linear pair below.
            let planes = [
                [irradiance[0], irradiance[1], irradiance[2], 0.5],
                [
                    moment_a[0] + (moment_b[0] - moment_a[0]) * t,
                    moment_a[1] + (moment_b[1] - moment_a[1]) * t,
                    moment_a[2] + (moment_b[2] - moment_a[2]) * t,
                    0.5,
                ],
            ];
            let light = decode_lightmap(0, normal, &planes);
            for value in light {
                new_min = new_min.min(value);
                new_max = new_max.max(value);
            }
            let axis = [
                old_axis[0][0] + (old_axis[1][0] - old_axis[0][0]) * t,
                old_axis[0][1] + (old_axis[1][1] - old_axis[0][1]) * t,
            ];
            let decoded = old_decode(axis);
            let cosine = decoded[0] * normal[0] + decoded[1] * normal[1] + decoded[2] * normal[2];
            let lobe = 2.0 * cosine.max(0.0) - 1.0;
            for value in irradiance {
                let old_light = (value + value * lobe).max(0.0);
                old_min = old_min.min(old_light);
                old_max = old_max.max(old_light);
            }
        }
        // The new representation renders the span as a narrow band: a bilinear
        // blend of the stored values stays a bilinear blend of the light.
        assert!(
            new_max - new_min < 0.02,
            "moment interpolation must be smooth: {new_min}..{new_max}"
        );
        // The removed representation, measured on the same span: the blended
        // axis folds through the seam and the centre of the span collapses
        // towards black.
        assert!(
            old_max - old_min > 0.4,
            "the old representation must show the seam artifact: {old_min}..{old_max}"
        );
        assert!(
            old_min < 0.1,
            "the old contour must reach near-black: {old_min}"
        );
    }

    #[test]
    fn the_sheen_matches_the_reference_equation_and_gate() {
        // Head-on, a mirror-tight sheen (gloss 1) keeps the `ahead` lobe; a
        // matte one (gloss 0) nearly loses it, which is the reference's
        // documented anti-plastic behaviour.
        let normal = glam::Vec3::new(0.0, 0.0, 1.0);
        let view = glam::Vec3::new(0.0, 0.0, 1.0);
        let glossy = sheen([1.0; 3], 0.0, normal, view, [1.0; 3]);
        let matte = sheen([1.0; 3], 1.0, normal, view, [1.0; 3]);
        assert!(
            (glossy[0] - 0.45).abs() < 1.0e-6,
            "gloss 1: {:?}",
            glossy[0]
        );
        assert!((matte[0] - 0.0).abs() < 1.0e-6, "gloss 0: {:?}", matte[0]);
        assert!(glossy[1] > matte[1]);

        // Grazing: the broad lobe dominates and never depends on the gloss's
        // tight exponent only.
        let grazing_view = glam::Vec3::new(1.0, 0.0, 0.0);
        let grazing = sheen([0.5; 3], 0.5, normal, grazing_view, [1.0; 3]);
        assert!(grazing[0] > 0.0);

        // A zero light factor extinguishes the sheen exactly (the reference
        // multiplies by `light`), and a gated response returns exactly zero.
        let dark = sheen([1.0; 3], 0.5, normal, view, [0.0; 3]);
        assert_eq!(dark, [0.0; 3]);

        // The shader's source contains the exact exponents and weights.
        for needle in [
            "pow(1.0 - facing, mix(1.0, 16.0, gloss))",
            "pow(facing, mix(1.0, 24.0, gloss)) * gloss",
            "grazing * 0.55 + ahead * 0.45",
            "let gloss = 1.0 - material.roughness;",
        ] {
            assert!(WORLD_SHADER_SRC.contains(needle), "missing {needle}");
        }
    }

    #[test]
    fn the_sheen_uses_the_prepared_material_normal() {
        // The sheen must consume the same normal the normal-map decode
        // produced, exactly like the reference reads its `normal` variable in
        // both blocks. The shader calls `material_normal` inside
        // `surface_sheen`, and the CPU check below proves a perturbed normal
        // changes the lobe in the direction the formula says.
        assert!(
            WORLD_SHADER_SRC.contains("let normal = material_normal(in, front_facing);"),
            "the sheen must consume the prepared material normal"
        );

        let flat = glam::Vec3::new(0.0, 0.0, 1.0);
        let view = glam::Vec3::new(0.4, 0.2, 0.9).normalize();
        // A tangent-space sample whose `xy` leans towards the view: the decoded
        // normal then faces the view more than the geometric normal does, so a
        // glossy material's head-on lobe grows and its grazing lobe shrinks.
        let tilted = decode_normal(
            [0.634, 0.567, 0.977],
            1.0,
            flat,
            glam::Vec3::new(1.0, 0.0, 0.0),
            1.0,
        );
        let unit_tilted = tilted.normalize();
        assert!(
            unit_tilted.dot(view) > flat.dot(view),
            "the sample tilts to the view"
        );

        let glossy = 0.1;
        let flat_sheen = sheen([1.0; 3], glossy, flat, view, [1.0; 3]);
        let tilted_sheen = sheen([1.0; 3], glossy, unit_tilted, view, [1.0; 3]);
        assert!(
            tilted_sheen[0] > flat_sheen[0],
            "a glossy head-on surface must brighten when the map tilts it to the view: \
             {} vs {}",
            tilted_sheen[0],
            flat_sheen[0]
        );

        // A matte material barely changes: the tight lobe is scaled by gloss.
        let matte = 0.9;
        let flat_matte = sheen([1.0; 3], matte, flat, view, [1.0; 3]);
        let tilted_matte = sheen([1.0; 3], matte, unit_tilted, view, [1.0; 3]);
        assert!(
            (tilted_matte[0] - flat_matte[0]).abs() < (tilted_sheen[0] - flat_sheen[0]).abs(),
            "the normal map must matter less on a matte surface"
        );
    }

    #[test]
    fn the_sheen_matches_the_reference_formula_on_a_tilted_normal() {
        // An independently written copy of the reference GLSL, with a
        // non-axis-aligned normal/view pair, so the mirror helper is checked
        // rather than checking itself.
        let normal = glam::Vec3::new(0.3, 0.4, 0.86).normalize();
        let view = glam::Vec3::new(-0.2, 0.1, 0.97).normalize();
        let specular = [0.25, 0.5, 0.75];
        let roughness = 0.35_f32;
        let light = [1.0, 0.9, 0.8];

        let facing = normal.dot(view).abs().clamp(0.0, 1.0);
        let gloss = 1.0 - roughness;
        let grazing = (1.0 - facing).powf(1.0 + (16.0 - 1.0) * gloss);
        let ahead = facing.powf(1.0 + (24.0 - 1.0) * gloss) * gloss;
        let expected: Vec<f32> = (0..3)
            .map(|channel| specular[channel] * (grazing * 0.55 + ahead * 0.45) * light[channel])
            .collect();
        let actual = sheen(specular, roughness, normal, view, light);
        for channel in 0..3 {
            assert!(
                (actual[channel] - expected[channel]).abs() < 1.0e-6,
                "channel {channel}: mirror {} vs reference formula {}",
                actual[channel],
                expected[channel]
            );
        }
        // The grazing and head-on lobes really are different terms here.
        assert!(grazing > 0.0 && ahead > 0.0);
    }

    #[test]
    fn sheen_and_emission_accumulate_in_linear_energy() {
        let albedo = crate::materials::color::decode_byte(128);
        let energy = albedo * 0.5 + 0.2 + 2.0;
        assert!(energy > 2.3);
        assert!(WORLD_SHADER_SRC.contains("lit + sheen + reflection + emission"));
        assert!(!WORLD_SHADER_SRC.contains("srgb_to_linear(shaded.color)"));
    }

    #[test]
    fn bright_irradiance_cannot_trigger_an_unlit_bypass() {
        assert!(!WORLD_SHADER_SRC.contains("all(light >= vec3<f32>(1.0))"));
        assert!(!WORLD_SHADER_SRC.contains("color = base_linear;"));
    }

    // ------------------------------------------------------------------- fog

    /// The CPU mirror of `WW::fogged`, pinned to the shader's numeric response.
    // one formula, every uniform explicit
    fn fogged(
        color: [f32; 3],
        world_position: [f32; 3],
        eye: [f32; 3],
        density: f32,
        reference_y: f32,
        height_gain: f32,
        fog_color: [f32; 3],
    ) -> [f32; 3] {
        let delta = [
            eye[0] - world_position[0],
            eye[1] - world_position[1],
            eye[2] - world_position[2],
        ];
        let distance = (delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2]).sqrt();
        let below = (reference_y - world_position[1]).max(0.0);
        let effective_density = density * (1.0 + height_gain * below.min(12.0));
        let amount = effective_density * distance;
        let attenuation = 1.0 - (-amount * amount).exp();
        let clamped_attenuation = attenuation.clamp(0.0, 1.0);
        std::array::from_fn(|channel| {
            color[channel] + (fog_color[channel] - color[channel]) * clamped_attenuation
        })
    }

    #[test]
    fn the_fog_term_is_the_reference_formula() {
        // The shader tokens, so the CPU mirror below cannot drift from the WGSL.
        for needle in [
            "let distance = length(camera.position - world_position);",
            "let below = max(0.0, environment.fog_reference_y - world_position.y);",
            "let global_density = environment.fog_density * (1.0 + environment.fog_height_gain * min(below, 12.0));",
            "let density = global_density + layer_density;",
            "var fog_amount = density * distance;",
            "fog_amount = 1.0 - exp(-fog_amount * fog_amount);",
            "return mix(color, srgb_to_linear(layer_color), clamp(fog_amount, 0.0, 1.0));",
        ] {
            assert!(WORLD_SHADER_SRC.contains(needle), "missing {needle}");
        }
        let fog_color = [0.08, 0.08, 0.09];
        let eye = [0.0, 0.0, 0.0];
        // Zero distance leaves the colour untouched.
        let untouched = fogged([0.5; 3], [0.0, 0.0, 0.0], eye, 0.01, 0.0, 0.5, fog_color);
        for value in &untouched {
            assert!((value - 0.5).abs() < 1.0e-6);
        }
        // A long view tends to the fog colour.
        let far = fogged([0.5; 3], [0.0, 0.0, 100.0], eye, 0.05, 0.0, 0.0, fog_color);
        assert!((far[0] - fog_color[0]).abs() < 1.0e-3, "{far:?}");
        // Below the reference height the gain increases the fog at the same
        // distance (the second point sits at the same 10 m range)...
        let at_reference = fogged([0.5; 3], [0.0, 0.0, 10.0], eye, 0.01, 0.0, 0.5, fog_color);
        let below = fogged(
            [0.5; 3],
            [0.0, -5.0, 75.0_f32.sqrt()],
            eye,
            0.01,
            0.0,
            0.5,
            fog_color,
        );
        assert!(below[0] < at_reference[0], "{below:?} vs {at_reference:?}");
        // ...and the height term saturates at the reference's 12 m cap: two
        // points at the same range below the reference height, one at -20 m
        // and one at -12 m, produce the same fog.
        let deeper = fogged([0.5; 3], [0.0, -20.0, 0.0], eye, 0.01, 0.0, 0.5, fog_color);
        let capped = fogged([0.5; 3], [0.0, -12.0, 16.0], eye, 0.01, 0.0, 0.5, fog_color);
        for channel in 0..3 {
            assert!((deeper[channel] - capped[channel]).abs() < 1.0e-6);
        }
    }

    /// The regional block's tokens: per-fragment evaluation with the fragment's
    /// own world position, the edge and layer factors, the overlap rule and the
    /// global addition. The camera may only set the view distance.
    #[test]
    fn the_fog_layer_is_evaluated_per_fragment_from_the_world_position() {
        for needle in [
            "let region = environment.fog_regions[index];",
            "let horizontal = min(",
            "edge = clamp(horizontal / max(region.min_falloff.w, 1.0e-6), 0.0, 1.0);",
            "vertical = select(0.0, 1.0, world_position.y <= ground_y);",
            "vertical = clamp((top_y - world_position.y) / (top_y - ground_y), 0.0, 1.0);",
            "let contribution = region.color_density.a * edge * vertical;",
            "if (contribution > layer_density) {",
            "layer_color = region.color_density.rgb;",
            "let region_count = min(environment.fog_region_count, 16u);",
        ] {
            assert!(WORLD_SHADER_SRC.contains(needle), "missing {needle}");
        }
        // `fogged` reads the camera exactly once, for the distance. Regional
        // membership never consults the eye.
        let start = WORLD_SHADER_SRC.find("fn fogged").expect("fogged exists");
        let tail = WORLD_SHADER_SRC
            .get(start..)
            .expect("fogged starts on a UTF-8 boundary");
        let end = tail
            .find("\n}\n")
            .expect("fogged closes")
            .saturating_add(start);
        let body = WORLD_SHADER_SRC
            .get(start..end)
            .expect("fogged bounds are valid UTF-8 boundaries");
        assert_eq!(
            body.matches("camera.position").count(),
            1,
            "the only camera read in fogged() is the view distance"
        );
        assert!(body.contains("length(camera.position - world_position)"));
    }

    // ------------------------------------------------------ planar reflection

    #[test]
    fn the_planar_sampling_is_the_reference_projection_with_the_capture_flip() {
        /// The CPU mirror of the planar branch's projection.
        fn planar_uv(clip: [f32; 4]) -> [f32; 2] {
            let w = clip[3].max(1.0e-4);
            let gl = [clip[0] / w * 0.5 + 0.5, clip[1] / w * 0.5 + 0.5];
            [gl[0], 1.0 - gl[1]]
        }
        // The shader tokens: the reference's GL projection, the WebGPU row
        // flip, the frame rejection and the distance-to-plane gate.
        for needle in [
            "let clip = environment.planar_matrix * vec4<f32>(in.world_position, 1.0);",
            "let gl_uv = clip.xy / max(clip.w, 1.0e-4) * 0.5 + 0.5;",
            "let uv = vec2<f32>(gl_uv.x, 1.0 - gl_uv.y);",
            "let inside = step(0.0, uv.x) * step(uv.x, 1.0) * step(0.0, uv.y) * step(uv.y, 1.0);",
            "let plane_distance = abs(dot(environment.planar_plane.xyz, in.world_position) + environment.planar_plane.w);",
            "let on_plane = 1.0 - smoothstep(0.0, 0.08, plane_distance);",
            "combine = combine * inside * on_plane;",
        ] {
            assert!(WORLD_SHADER_SRC.contains(needle), "missing {needle}");
        }

        // NDC (0.5, 0.25) is the GL uv (0.75, 0.625); the capture writes NDC +Y
        // into the first row, so the port's v is flipped to 0.375. A missing
        // flip would sample the vertically mirrored reflection.
        let uv = planar_uv([0.5, 0.25, 0.0, 1.0]);
        assert!((uv[0] - 0.75).abs() < 1.0e-6);
        assert!((uv[1] - 0.375).abs() < 1.0e-6);
        assert!((planar_uv([-0.5, -0.25, 0.0, 1.0])[1] - 0.625).abs() < 1.0e-6);
        // A projection outside the captured frame reports no image: the
        // reference's `inside` product is zero for either axis outside [0, 1].
        let outside = planar_uv([1.2, 0.0, 0.0, 1.0]);
        let inside = f32::from(outside[0] >= 0.0)
            * f32::from(outside[0] <= 1.0)
            * f32::from(outside[1] >= 0.0)
            * f32::from(outside[1] <= 1.0);
        assert_eq!(inside, 0.0);
    }

    // ---------------------------------------------------- normal-map decode

    /// The CPU reference of `material_normal`'s decode, for the cases below.
    fn decode_normal(
        sample: [f32; 3],
        strength: f32,
        normal: glam::Vec3,
        tangent: glam::Vec3,
        handedness: f32,
    ) -> glam::Vec3 {
        let decoded = glam::Vec3::new(
            sample[0] * 2.0 - 1.0,
            sample[1] * 2.0 - 1.0,
            sample[2] * 2.0 - 1.0,
        );
        let scaled = glam::Vec3::new(decoded.x * strength, decoded.y * strength, decoded.z);
        let unit_normal = normal.normalize();
        let orthogonal_tangent = (tangent - unit_normal * unit_normal.dot(tangent)).normalize();
        let bitangent = unit_normal.cross(orthogonal_tangent) * handedness;
        (orthogonal_tangent * scaled.x + bitangent * scaled.y + unit_normal * scaled.z).normalize()
    }

    #[test]
    fn a_flat_normal_map_leaves_the_geometric_normal_unchanged() {
        for strength in [0.0, 0.5, 1.0, 2.0] {
            let decoded =
                decode_normal([0.5, 0.5, 1.0], strength, glam::Vec3::Z, glam::Vec3::X, 1.0);
            assert!((decoded - glam::Vec3::Z).length() < 1.0e-6, "{strength}");
        }
    }

    #[test]
    fn a_zero_strength_gates_the_normal_map_entirely() {
        // A proper flat map's z is 1, so with `xy` scaled to zero the decoded
        // vector is the geometric normal itself.
        let decoded = decode_normal(
            [0.9, 0.1, 1.0],
            0.0,
            glam::Vec3::new(0.0, 1.0, 0.0),
            glam::Vec3::X,
            1.0,
        );
        assert!((decoded - glam::Vec3::Y).length() < 1.0e-6);
    }

    #[test]
    fn a_known_tangent_space_sample_reconstructs_the_expected_world_normal() {
        // Frame: N = +Z, T = +X, handedness +1 -> bitangent +Y.
        let decoded = decode_normal([0.5, 0.8, 0.9], 1.0, glam::Vec3::Z, glam::Vec3::X, 1.0);
        // Decoded tangent space: (0, 0.6, 0.8) -> 0.6 Y + 0.8 Z.
        assert!((decoded - glam::Vec3::new(0.0, 0.6, 0.8)).length() < 1.0e-5);

        // The mirrored frame (T = -X, handedness -1) reconstructs the same
        // tangent-space direction, because the sign is in the frame, not green.
        let mirrored = decode_normal([0.5, 0.8, 0.9], 1.0, glam::Vec3::Z, -glam::Vec3::X, -1.0);
        assert!((mirrored - glam::Vec3::new(0.0, 0.6, 0.8)).length() < 1.0e-5);

        // A negative handedness on an unmirrored tangent flips the bitangent.
        let flipped = decode_normal([0.5, 0.8, 0.9], 1.0, glam::Vec3::Z, glam::Vec3::X, -1.0);
        assert!(flipped.y < 0.0, "{flipped:?}");
    }

    #[test]
    fn strength_scaling_happens_before_the_final_normalization() {
        // `xy` is scaled, `z` is not, and the result is renormalized: a
        // strength of 2 on a pure-X tangent sample must stay unit length.
        let decoded = decode_normal([0.75, 0.5, 0.5], 2.0, glam::Vec3::Z, glam::Vec3::X, 1.0);
        assert!((decoded.length() - 1.0).abs() < 1.0e-5);
        assert!(decoded.x > 0.99, "{decoded:?}");
    }
}
