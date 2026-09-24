//! The OpenGL renderer: GL state, buffers, draws and captures.
//!
//! The renderer owns the GPU side of one level: the static and prop vertex
//! buffers, the textures, the draw calls (static batches, prop batches, decals
//! and the UI pass) and the readback used by developer captures. It never bakes
//! lighting or builds geometry; it uploads and draws what those phases already
//! produced.

use super::animation::{AnimationEffect, EmissionAnimation};
use super::api::{
    LevelBuild, LightmapBuildOptions, build_level_geometry_timed_with_lightmaps,
    shipped_asset_catalog,
};
use super::decals::{DECAL_ATLAS_SIZE, generate_decal_atlas};
use super::view::dimension_f32;
use super::{
    BatchRange, DECAL_ALPHA_CUTOFF, DECAL_EXTERNAL_BASE, DECAL_FRAGMENT_SHADER_SRC,
    DECAL_POLYGON_OFFSET, DrawableSize, EMISSION_MASK_TEXTURE_UNIT, HasContext,
    LIGHTMAP_PAGE_SLOTS, LIGHTMAP_TEXTURE_UNIT, LIGHTMAP_TEXTURE_UNIT_1, LevelMesh, MaterialIndex,
    MaterialTable, MeshChunk, MeshPacker, NORMAL_MAP_TEXTURE_UNIT, PRESENT_FRAGMENT_SHADER_SRC,
    PRESENT_TEXTURE_UNIT, PRESENT_VERTEX_SHADER_SRC, PackedVertex, PropMeshBatch,
    REFLECTION_PLANAR_TEXTURE_UNIT, REFLECTION_PROBE_TEXTURE_UNIT, SCENE_ATTRIB_COLOR,
    SCENE_ATTRIB_COUNT, SCENE_ATTRIB_HANDEDNESS, SCENE_ATTRIB_LIGHTMAP_PAGE,
    SCENE_ATTRIB_LIGHTMAP_UV, SCENE_ATTRIB_NORMAL, SCENE_ATTRIB_POS, SCENE_ATTRIB_TANGENT,
    SCENE_ATTRIB_UV, SCENE_FAR_M, SCENE_NEAR_M, SCENE_TEXTURE_UNIT, StaticBatch, SurfaceKey,
    SurfaceKind, UI_REFERENCE_HEIGHT, UI_REFERENCE_WIDTH, VERTEX_SHADER_SRC, Vertex, VertexLayout,
    decal_external_sheet_ids, exact_layout, fragment_shader_source, generate_font_atlas,
    packed_layout, spatial_cell_grid, vertical_fov_for_aspect,
};
use crate::lighting::LevelLighting;
use crate::lighting::lightmap::{
    LevelLightmaps, LightmapCache, LightmapFailure, LightmapMode, LightmapPage,
};
use crate::materials::{
    AlphaMode, MaterialAlpha, MaterialEmission, MaterialReflection, MaterialResponse,
    ReflectionMode,
};
use crate::spatial::Frustum;

use super::atmosphere::FogState;
use super::framebuffer::{self, PRESENT_QUAD};

use super::dynamic::{DynamicMesh, DynamicScene, DynamicUpdate};

/// Whether the offscreen scene path should start enabled.
///
/// On by default. `LIMINAL_NO_OFFSCREEN=1` forces the historical
/// draw-into-the-default-framebuffer path, which is what an A/B benchmark (and
/// a bring-up on a driver with a broken framebuffer implementation) needs.
fn offscreen_requested_from_env() -> bool {
    std::env::var("LIMINAL_NO_OFFSCREEN").as_deref() != Ok("1")
}

/// Whether the post-processing bloom stage should start enabled.
///
/// On by default. `LIMINAL_NO_BLOOM=1` drops the emissive pass and the blur
/// while keeping the resolve pass, so a benchmark can measure the bloom stages
/// alone against the same build, profile and level.
fn bloom_requested_from_env() -> bool {
    std::env::var("LIMINAL_NO_BLOOM").as_deref() != Ok("1")
}

/// Whether selective reflections should start enabled.
///
/// On by default. `LIMINAL_NO_REFLECTIONS=1` reports every material as
/// reflection-free, so the planar pass, the probe bake and the reflection
/// texture binds all disappear from the frame.
fn reflections_requested_from_env() -> bool {
    std::env::var("LIMINAL_NO_REFLECTIONS").as_deref() != Ok("1")
}

/// Whether the scene should render offscreen at all, and at which size.
///
/// Pure so the decision can be tested without a GL context: `None` is the
/// historical direct path (the switch is off, a target has already failed, or
/// there is nothing to render into), `Some(size)` is the target to have.
/// [`framebuffer::scene_target_size`] owns the profile's size rule.
#[must_use]
pub(super) fn offscreen_plan(
    enabled: bool,
    failed: bool,
    quality: crate::quality::QualityProfile,
    drawable: DrawableSize,
) -> Option<DrawableSize> {
    if !enabled || failed || drawable.is_empty() {
        return None;
    }
    Some(framebuffer::scene_target_size(quality, drawable))
}

/// Applies min/mag filtering for a repeating, mipmapped texture.
///
/// Nearest filtering keeps mipmaps (`NEAREST_MIPMAP_NEAREST`) so distant
/// minification still anti-aliases instead of shimmering.
unsafe fn set_repeat_filter(gl: &glow::Context, linear: bool) {
    let (min_filter, mag_filter) = if linear {
        (glow::LINEAR_MIPMAP_LINEAR, glow::LINEAR)
    } else {
        (glow::NEAREST_MIPMAP_NEAREST, glow::NEAREST)
    };
    unsafe {
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MIN_FILTER,
            min_filter.cast_signed(),
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MAG_FILTER,
            mag_filter.cast_signed(),
        );
    }
}

/// Applies min/mag filtering to a lightmap page.
///
/// Lightmaps deliberately have no mip chain: the atlas is a collection of
/// unrelated charts, and any mip level below the top would average across chart
/// boundaries (and gutters), bleeding one surface's light into another. `LINEAR`
/// or `NEAREST`, matching the game's filtering setting, is the whole policy.
unsafe fn set_lightmap_filter(gl: &glow::Context, linear: bool) {
    let (min_filter, mag_filter) = if linear {
        (glow::LINEAR, glow::LINEAR)
    } else {
        (glow::NEAREST, glow::NEAREST)
    };
    unsafe {
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MIN_FILTER,
            min_filter.cast_signed(),
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MAG_FILTER,
            mag_filter.cast_signed(),
        );
    }
}

unsafe fn create_texture_2d(
    gl: &glow::Context,
    width: i32,
    height: i32,
    pixels: &[u8],
    repeat: bool,
    linear: bool,
) -> Result<glow::Texture, String> {
    unsafe {
        let texture = gl.create_texture()?;
        gl.bind_texture(glow::TEXTURE_2D, Some(texture));

        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA.cast_signed(),
            width,
            height,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(Some(pixels)),
        );

        let wrap_mode = if repeat {
            glow::REPEAT.cast_signed()
        } else {
            glow::CLAMP_TO_EDGE.cast_signed()
        };
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, wrap_mode);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, wrap_mode);

        if repeat {
            set_repeat_filter(gl, linear);
            gl.generate_mipmap(glow::TEXTURE_2D);
        } else {
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MIN_FILTER,
                glow::NEAREST.cast_signed(),
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MAG_FILTER,
                glow::NEAREST.cast_signed(),
            );
        }

        gl.bind_texture(glow::TEXTURE_2D, None);
        Ok(texture)
    }
}

/// Creates a one-texel black cubemap.
///
/// The world shader declares `u_probe_map` unconditionally, so the unit must
/// always hold a complete cube: sampling an unbound cube sampler is a GL error
/// even when the material's reflection weight is zero. This is that cube, and
/// it is what a frame draws with when no probe was baked.
unsafe fn create_black_cube(gl: &glow::Context) -> Result<glow::Texture, String> {
    unsafe {
        let texture = gl.create_texture()?;
        gl.bind_texture(glow::TEXTURE_CUBE_MAP, Some(texture));
        for face in [
            glow::TEXTURE_CUBE_MAP_POSITIVE_X,
            glow::TEXTURE_CUBE_MAP_NEGATIVE_X,
            glow::TEXTURE_CUBE_MAP_POSITIVE_Y,
            glow::TEXTURE_CUBE_MAP_NEGATIVE_Y,
            glow::TEXTURE_CUBE_MAP_POSITIVE_Z,
            glow::TEXTURE_CUBE_MAP_NEGATIVE_Z,
        ] {
            gl.tex_image_2d(
                face,
                0,
                glow::RGBA.cast_signed(),
                1,
                1,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(&[0, 0, 0, 255])),
            );
        }
        gl.tex_parameter_i32(
            glow::TEXTURE_CUBE_MAP,
            glow::TEXTURE_WRAP_S,
            glow::CLAMP_TO_EDGE.cast_signed(),
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_CUBE_MAP,
            glow::TEXTURE_WRAP_T,
            glow::CLAMP_TO_EDGE.cast_signed(),
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_CUBE_MAP,
            glow::TEXTURE_MIN_FILTER,
            glow::LINEAR.cast_signed(),
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_CUBE_MAP,
            glow::TEXTURE_MAG_FILTER,
            glow::LINEAR.cast_signed(),
        );
        gl.bind_texture(glow::TEXTURE_CUBE_MAP, None);
        Ok(texture)
    }
}

unsafe fn create_shader(
    gl: &glow::Context,
    shader_type: u32,
    source: &str,
) -> Result<glow::Shader, String> {
    unsafe {
        let shader = gl.create_shader(shader_type)?;
        gl.shader_source(shader, source);
        gl.compile_shader(shader);
        if !gl.get_shader_compile_status(shader) {
            let log = gl.get_shader_info_log(shader);
            gl.delete_shader(shader);
            return Err(format!("Shader compile error: {log}"));
        }
        Ok(shader)
    }
}

pub(super) unsafe fn create_program(
    gl: &glow::Context,
    vert_src: &str,
    frag_src: &str,
) -> Result<glow::Program, String> {
    unsafe {
        let vs = create_shader(gl, glow::VERTEX_SHADER, vert_src)?;
        let fs = create_shader(gl, glow::FRAGMENT_SHADER, frag_src)?;

        let program = gl.create_program()?;
        // Both programs share the scene attribute layout, so bind the indices
        // explicitly before linking: the decal pass switches programs mid-frame
        // and must not need to re-point the vertex attributes. Every slot the
        // scene programs read is bound here; `scene_attribute_locations`
        // verifies the driver honoured them.
        gl.bind_attrib_location(program, SCENE_ATTRIB_POS, "a_pos");
        gl.bind_attrib_location(program, SCENE_ATTRIB_COLOR, "a_color");
        gl.bind_attrib_location(program, SCENE_ATTRIB_UV, "a_uv");
        gl.bind_attrib_location(program, SCENE_ATTRIB_LIGHTMAP_UV, "a_lightmap_uv");
        gl.bind_attrib_location(program, SCENE_ATTRIB_LIGHTMAP_PAGE, "a_lightmap_page");
        gl.bind_attrib_location(program, SCENE_ATTRIB_NORMAL, "a_normal");
        gl.bind_attrib_location(program, SCENE_ATTRIB_TANGENT, "a_tangent");
        gl.bind_attrib_location(program, SCENE_ATTRIB_HANDEDNESS, "a_handedness");
        gl.attach_shader(program, vs);
        gl.attach_shader(program, fs);
        gl.link_program(program);

        if !gl.get_program_link_status(program) {
            let log = gl.get_program_info_log(program);
            gl.delete_program(program);
            gl.delete_shader(vs);
            gl.delete_shader(fs);
            return Err(format!("Program link error: {log}"));
        }

        gl.delete_shader(vs);
        gl.delete_shader(fs);
        Ok(program)
    }
}

/// One drawable batch of placed props: all instances of a single model inside a
/// single spatial cell, drawn as a contiguous vertex range of the prop buffer
/// with one material.
#[derive(Clone, Copy, Debug)]
pub(super) struct PropDraw {
    texture: glow::Texture,
    /// Emission this range draws with (a prop material's own emission, never a
    /// light source).
    emission: EmissionState,
    /// Which prop buffer pair this range lives in (see `MeshPacker`).
    chunk: usize,
    /// Range in that chunk's index buffer.
    index_start: i32,
    index_count: i32,
    /// Distinct vertices the range reads, for the debug counters.
    vertex_count: i32,
    bounds: crate::spatial::Aabb,
}

/// The emission term one surface batch draws with: a colour premultiplied by
/// intensity, an optional mask sheet, or the per-vertex colour.
///
/// Split out of [`SurfaceState`] because it predates it and because the two
/// paths that carry it (materials and fixture faces) are genuinely different
/// sources of the same three uniforms.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct EmissionState {
    /// Colour premultiplied by intensity, or the per-vertex colour when
    /// `vertex` is set.
    pub(super) color: [f32; 3],
    /// Mask sheet bound on the emissive texture unit. `None` disables the mask,
    /// which is the common case and skips a fragment-stage texture fetch.
    mask: Option<glow::Texture>,
    /// True when the vertex colour itself carries the emission (fixture faces).
    vertex: bool,
}

impl EmissionState {
    /// No emission: every material authored before emission existed.
    pub(super) const NONE: Self = Self {
        color: [0.0, 0.0, 0.0],
        mask: None,
        vertex: false,
    };

    /// A material's uniform emission with an optional mask sheet.
    const fn material(
        emission: crate::materials::MaterialEmission,
        mask: Option<glow::Texture>,
    ) -> Self {
        Self {
            color: emission.effective_color(),
            mask,
            vertex: false,
        }
    }

    /// Per-vertex emission: a fixture's luminous face, whose glow varies per
    /// placement while the batch stays shared.
    const fn vertex() -> Self {
        Self {
            color: [0.0, 0.0, 0.0],
            mask: None,
            vertex: true,
        }
    }

    /// Whether this surface emits anything at all.
    ///
    /// The bloom pass draws exactly the batches this answers `true` for, which
    /// is what keeps the glow on fixtures, signs and screens instead of on
    /// brightly lit wallpaper.
    pub(super) fn is_emissive(self) -> bool {
        self.vertex || self.color[0] > 0.0 || self.color[1] > 0.0 || self.color[2] > 0.0
    }
}

/// The mirror plane a frame's planar reflection was drawn through.
#[derive(Clone, Copy, Debug)]
struct ActiveReflectionPlane {
    /// Index into the level's reflection routing.
    index: usize,
    /// View-projection of the reflected camera, for projecting a world position
    /// onto the reflection image.
    matrix: glam::Mat4,
    /// Plane as `(normal, offset)`, for the shader's on-plane test.
    plane: [f32; 4],
}

/// The complete surface state one world draw runs with.
///
/// A batch's material is *not* vertex state: it reaches the GPU as uniforms.
/// This value is cached by the renderer, so a run of batches that share the
/// previous batch's material costs no uniform or texture-unit work at all, and
/// content that authors no emission, no normal map and no sheen is exactly as
/// cheap to draw as it was before those paths existed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct SurfaceState {
    /// Albedo sheet bound on [`SCENE_TEXTURE_UNIT`].
    pub(super) texture: glow::Texture,
    /// Normal-map sheet bound on [`NORMAL_MAP_TEXTURE_UNIT`], `None` to disable.
    pub(super) normal: Option<glow::Texture>,
    /// Emission colour, mask and source.
    pub(super) emission: EmissionState,
    /// Sheen colour, premultiplied by the authored strength.
    pub(super) specular: [f32; 3],
    /// `0.0` mirror-tight .. `1.0` fully matte.
    pub(super) roughness: f32,
    /// Multiplier applied to the sampled alpha.
    pub(super) opacity: f32,
    /// Alpha below which the alpha-tested program discards a texel.
    pub(super) alpha_cutoff: f32,
    /// Multiplier applied to the normal map's decoded `xy`.
    pub(super) normal_strength: f32,
    /// Whether the response term (normal map and sheen) is live for this draw.
    ///
    /// False for a material that authors none, for the HUD, and for the whole
    /// scene under the Low quality profile.
    pub(super) response: bool,
    /// Where this surface's reflection comes from, if it authors one.
    pub(super) reflection: MaterialReflection,
    /// The mirror plane this surface lies on, when its reflection is planar.
    ///
    /// Resolved at level load from the geometry itself: the same material can
    /// be used on two planes, so the plane is a property of the batch, not of
    /// the material.
    pub(super) reflection_plane: Option<u16>,
    /// How this surface's emission moves over time, if the level animates it.
    pub(super) emission_animation: Option<EmissionAnimation>,
}

impl SurfaceState {
    /// The state of a batch that binds only an albedo sheet and a vertex-lit
    /// colour: no normal map, no sheen, no emission and opaque. The HUD draws
    /// with it, and a material that authors no response or alpha resolves to
    /// the same terms.
    pub(super) const fn plain(texture: glow::Texture) -> Self {
        Self {
            texture,
            normal: None,
            emission: EmissionState::NONE,
            specular: [0.0; 3],
            roughness: crate::materials::DEFAULT_ROUGHNESS,
            normal_strength: crate::materials::DEFAULT_NORMAL_STRENGTH,
            opacity: 1.0,
            alpha_cutoff: crate::materials::DEFAULT_ALPHA_CUTOFF,
            response: false,
            reflection: MaterialReflection::NONE,
            reflection_plane: None,
            emission_animation: None,
        }
    }
}

/// Which fragment stage one draw uses.
///
/// The opaque world and the alpha-tested world are two *programs*, not one
/// branch: a `discard` in a program can disable early depth testing for every
/// draw that uses it, and the opaque world must keep it. The index is also the
/// slot of the program's uniform locations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ScenePass {
    /// The opaque/translucent world stage.
    World,
    /// The same stage with an alpha cut-out.
    Cutout,
}

impl ScenePass {
    /// Every pass, in slot order.
    pub(super) const ALL: [Self; 2] = [Self::World, Self::Cutout];

    /// This pass's slot in the uniform-location table.
    const fn index(self) -> usize {
        match self {
            Self::World => 0,
            Self::Cutout => 1,
        }
    }
}

/// Which draw pass one batch belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum BatchPass {
    /// Written in the opaque pass with depth writes on.
    Opaque,
    /// Written in the opaque pass through the alpha-tested program.
    Cutout,
    /// Written after everything opaque, sorted and blended, with depth writes
    /// off.
    Translucent,
}

impl BatchPass {
    /// The pass a material's alpha contract implies.
    #[must_use]
    pub(super) const fn of(alpha: MaterialAlpha) -> Self {
        match alpha.mode {
            AlphaMode::Opaque => Self::Opaque,
            AlphaMode::Cutout => Self::Cutout,
            AlphaMode::Blend => {
                if alpha.opacity > 0.0 {
                    Self::Translucent
                } else {
                    // Nothing to see through: an invisible surface must not be
                    // submitted at all, and the opaque pass is where a
                    // zero-opacity surface is cheapest to skip.
                    Self::Opaque
                }
            }
        }
    }

    /// The scene program this pass draws with.
    #[must_use]
    pub(super) const fn program(self) -> ScenePass {
        match self {
            Self::Cutout => ScenePass::Cutout,
            Self::Opaque | Self::Translucent => ScenePass::World,
        }
    }
}

/// Which pass one static surface key belongs to, given its material's alpha.
///
/// Fixtures, placeholder prop boxes and decals are opaque by construction: their
/// alpha never reaches the framebuffer. Only a floor, ceiling or wall that binds
/// a level material can be translucent, because only a level material authors an
/// alpha contract at all.
#[must_use]
pub(super) fn batch_pass_for(
    kind: SurfaceKind,
    has_material: bool,
    alpha: Option<MaterialAlpha>,
) -> BatchPass {
    match kind {
        SurfaceKind::Floor | SurfaceKind::Ceiling | SurfaceKind::Wall if has_material => {
            alpha.map_or(BatchPass::Opaque, BatchPass::of)
        }
        SurfaceKind::Floor
        | SurfaceKind::Ceiling
        | SurfaceKind::Wall
        | SurfaceKind::Light
        | SurfaceKind::PropFallback
        | SurfaceKind::Decal => BatchPass::Opaque,
    }
}

/// Collects every translucent static batch that should be drawn, and sorts them
/// back to front.
///
/// `pass_of` answers which pass a key belongs to (the renderer's material table
/// answers it in the frame loop, a test answers it directly) and `visible`
/// answers whether a batch survived culling. Sorting is by the squared distance
/// from the camera to each batch's centre, nearest last, so a nearer translucent
/// surface blends *over* a farther one; two surfaces at the same distance keep
/// their build order, which is deterministic.
pub(super) fn collect_translucent_draws(
    batches: &[StaticBatch],
    pass_of: impl Fn(SurfaceKey) -> BatchPass,
    camera: glam::Vec3,
    visible: impl Fn(&crate::spatial::Aabb) -> bool,
    out: &mut Vec<TranslucentDraw>,
) {
    out.clear();
    for (index, batch) in batches.iter().enumerate() {
        if batch.index_range.count <= 0 || batch.key.kind == SurfaceKind::Decal {
            continue;
        }
        if pass_of(batch.key) != BatchPass::Translucent {
            continue;
        }
        if !visible(&batch.bounds) {
            continue;
        }
        let centre = batch.bounds.centre();
        let delta = glam::Vec3::new(
            centre[0] - camera.x,
            centre[1] - camera.y,
            centre[2] - camera.z,
        );
        out.push(TranslucentDraw {
            source: TranslucentSource::Static(index),
            distance_sq: delta.length_squared(),
        });
    }
    // Farthest first: a nearer surface must blend over a farther one.
    out.sort_by(|left, right| {
        right
            .distance_sq
            .partial_cmp(&left.distance_sq)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
}

/// One item of the translucent pass, resolved to the batch it draws.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct TranslucentDraw {
    pub(super) source: TranslucentSource,
    /// Squared distance from the camera to the batch's centre, used to sort the
    /// pass back to front.
    pub(super) distance_sq: f32,
}

/// Where one translucent draw's geometry lives.
///
/// Only the static world contributes translucent geometry: a prop's glTF
/// `alphaMode` is not parsed, so every placed model draws opaque.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TranslucentSource {
    /// Index into [`Renderer::static_batches`].
    Static(usize),
}

/// Cost and shape of the last level build, split by stage so a hardware run can
/// tell an expensive geometry bake from an expensive prop instancing pass.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LevelBuildStats {
    /// Distinct vertices in the static level mesh (floors, ceilings, walls, fixtures).
    pub static_vertices: usize,
    /// Indices in the static level mesh.
    pub static_indices: usize,
    /// Distinct vertices in the prop mesh (every placed instance).
    pub prop_vertices: usize,
    /// Indices in the prop mesh.
    pub prop_indices: usize,
    /// Draw calls the level needs for real prop geometry.
    pub prop_draws: usize,
    /// Cullable static batches the level was partitioned into.
    pub static_batches: usize,
    /// Static GPU buffer pairs (a level past 65 536 vertices needs several).
    pub static_chunks: usize,
    /// Prop GPU buffer pairs.
    pub prop_chunks: usize,
    /// Bytes resident in vertex buffers.
    pub vbo_bytes: usize,
    /// Bytes resident in index buffers.
    pub index_bytes: usize,
    /// Wall-clock cost of the last level build (geometry + lighting bake), in ms.
    pub build_millis: f64,
    /// Time spent baking the static lighting, in ms.
    pub lighting_millis: f64,
    /// Time spent resolving, transforming and lit-shading every placed prop, in ms.
    pub props_millis: f64,
    /// Time spent emitting and spatially bucketing the static surfaces, in ms.
    pub surfaces_millis: f64,
    /// Summary of the baked static lighting.
    pub lighting: crate::lighting::LightingSummary,
    /// Lightmap atlas pages resident on the GPU.
    pub lightmap_pages: usize,
    /// Page texels uploaded, including gutters and unused page space.
    pub lightmap_texels: usize,
    /// Charts the lightmap bake filled.
    pub lightmap_charts: usize,
    /// Chart data texels the fill pass wrote, i.e. the light samples this level
    /// actually evaluated. Smaller than [`Self::lightmap_texels`], which counts
    /// whole pages including gutters and unused space.
    pub lightmap_chart_texels: usize,
    /// Time spent filling and packing the lightmap atlas, in ms.
    pub lightmap_millis: f64,
    /// True when a lightmap build failed and the level fell back to vertex
    /// lighting; the reason is on the level build, not in this counter.
    pub lightmap_fallback: bool,
}

/// Geometry counters for the frame that was most recently submitted.
///
/// Filled in by [`Renderer::render_scene`] and read by the debug-only benchmark
/// harness, so the numbers describe the actual draw path rather than a
/// reconstruction of it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RenderStats {
    /// Vertices the level holds in total (static + props), visible or not.
    pub total_vertices: usize,
    /// Vertices inside the batches that were actually submitted this frame.
    pub visible_vertices: usize,
    /// Vertices belonging to batches the frustum rejected this frame.
    pub culled_vertices: usize,
    /// Render batches the level is split into.
    pub total_batches: usize,
    /// Batches that survived culling and were submitted.
    pub visible_batches: usize,
    /// `glDrawArrays`/`glDrawElements` calls issued for the scene.
    pub draw_calls: usize,
    /// Draw calls the dynamic-object path submitted this frame: one per object
    /// per material, never one per vertex.
    pub dynamic_draws: usize,
    /// Distinct vertices the dynamic objects hold this frame.
    pub dynamic_vertices: usize,
    /// Bytes resident in static vertex buffers (level + props).
    pub vbo_bytes: usize,
    /// Bytes resident in element (index) buffers.
    pub index_bytes: usize,
    /// `glBindTexture` calls the scene and presentation passes issued. One
    /// material change costs three (albedo, emission mask, normal map); a run
    /// of batches that share a material costs none.
    pub texture_binds: usize,
    /// Surface-state changes the scene passes applied: one per material change
    /// per pass, plus the pass switches that invalidate the cache.
    pub material_changes: usize,
    /// Scene submissions the frame spent on reflections: one for the active
    /// planar plane, zero when only probes are drawn or nothing reflects.
    pub reflection_passes: u32,
}

/// GPU state for the decal pass: a second program (the world shader plus an
/// alpha cut-out), the shared generated decal sheet, and the uniforms the pass
/// has to set when it starts.
pub(super) struct DecalPass {
    program: glow::Program,
    texture: glow::Texture,
    /// External (PNG-backed) decal sheets for the current level, in the order
    /// [`decal_external_sheet_ids`] reports them. The generated atlas keeps
    /// slot 0 and the external sheets take [`DECAL_EXTERNAL_BASE`] onwards.
    external: Vec<glow::Texture>,
    u_mvp_loc: Option<glow::UniformLocation>,
    u_texture_loc: Option<glow::UniformLocation>,
    u_alpha_cutoff_loc: Option<glow::UniformLocation>,
}

/// Which emission term one static surface batch draws with.
///
/// Split out from [`Renderer::static_emission`] so the routing — the part that
/// decides *whether* a surface emits — is testable without a GL context, and so
/// the rule lives in one place instead of in a match spread over the draw path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EmissionRouting {
    /// No emission term: plain lit surfaces.
    None,
    /// The batch's material decides (floors, ceilings, walls).
    Material,
    /// The vertex colour *is* the emission (a fixture's luminous face, whose
    /// glow is per placement while the batch is shared).
    Vertex,
}

/// The emission term a static surface kind uses.
///
/// `has_material` is [`SurfaceKey::has_material`]: a light batch without one is
/// the fixture's untextured housing, which is lit like any other surface.
pub(super) const fn emission_routing(kind: SurfaceKind, has_material: bool) -> EmissionRouting {
    match kind {
        SurfaceKind::Floor | SurfaceKind::Ceiling | SurfaceKind::Wall => EmissionRouting::Material,
        SurfaceKind::Light if has_material => EmissionRouting::Vertex,
        SurfaceKind::Light | SurfaceKind::PropFallback | SurfaceKind::Decal => {
            EmissionRouting::None
        }
    }
}

/// The per-material parameters the draw path needs, derived once per level from
/// the resolved material table.
///
/// One place derives all four vectors, so a new material property cannot be
/// resolved correctly and then simply not reach the draw path: the failure this
/// type exists to prevent. Every vector is parallel and indexed by *material
/// index* (what a batch's `SurfaceKey` carries), not by texture index.
#[derive(Default)]
pub(super) struct MaterialRenderState {
    /// Texture slot per material index (what `Renderer::material_textures` is
    /// indexed by). Two materials that share one texture map to one slot.
    pub(super) texture_slots: Vec<u16>,
    /// Emission per material index.
    pub(super) emissions: Vec<MaterialEmission>,
    /// Surface response per material index.
    pub(super) responses: Vec<MaterialResponse>,
    /// Alpha contract per material index.
    pub(super) alphas: Vec<MaterialAlpha>,
    /// Reflection contract per material index.
    pub(super) reflections: Vec<MaterialReflection>,
}

impl MaterialRenderState {
    /// Derives every per-material parameter from a resolved table.
    pub(super) fn from_table(table: &MaterialTable) -> Self {
        let mut state = Self::default();
        for entry in table.entries() {
            state.texture_slots.push(entry.texture_index);
            state.emissions.push(entry.emission);
            state.responses.push(entry.response);
            state.alphas.push(entry.alpha);
            state.reflections.push(entry.reflection);
        }
        state
    }
}

/// Every uniform location one scene program exposes.
///
/// Uniform state in OpenGL is *per program*, not per context: switching to the
/// alpha-tested program invalidates everything the opaque program was holding,
/// so each program owns its own location table and the renderer re-uploads the
/// per-frame state (lightmap switch, light scale, model transform, camera) after
/// every switch.
struct ProgramUniforms {
    mvp: Option<glow::UniformLocation>,
    model: Option<glow::UniformLocation>,
    texture: Option<glow::UniformLocation>,
    camera_pos: Option<glow::UniformLocation>,
    emission_color: Option<glow::UniformLocation>,
    emission_mask_enabled: Option<glow::UniformLocation>,
    emission_vertex: Option<glow::UniformLocation>,
    emission_scale: Option<glow::UniformLocation>,
    lightmap_enabled: Option<glow::UniformLocation>,
    light_scale: Option<glow::UniformLocation>,
    response_enabled: Option<glow::UniformLocation>,
    normal_enabled: Option<glow::UniformLocation>,
    normal_strength: Option<glow::UniformLocation>,
    specular: Option<glow::UniformLocation>,
    roughness: Option<glow::UniformLocation>,
    opacity: Option<glow::UniformLocation>,
    alpha_cutoff: Option<glow::UniformLocation>,
    fog_color: Option<glow::UniformLocation>,
    fog_density: Option<glow::UniformLocation>,
    fog_reference_y: Option<glow::UniformLocation>,
    fog_height_gain: Option<glow::UniformLocation>,
    reflect_mode: Option<glow::UniformLocation>,
    reflect_strength: Option<glow::UniformLocation>,
    planar_matrix: Option<glow::UniformLocation>,
    planar_plane: Option<glow::UniformLocation>,
    emission_only: Option<glow::UniformLocation>,
}

impl ProgramUniforms {
    /// Looks up every location of one linked program.
    unsafe fn new(gl: &glow::Context, program: glow::Program) -> Self {
        unsafe {
            Self {
                mvp: gl.get_uniform_location(program, "u_mvp"),
                model: gl.get_uniform_location(program, "u_model"),
                texture: gl.get_uniform_location(program, "u_texture"),
                camera_pos: gl.get_uniform_location(program, "u_camera_pos"),
                emission_color: gl.get_uniform_location(program, "u_emission_color"),
                emission_mask_enabled: gl.get_uniform_location(program, "u_emission_mask_enabled"),
                emission_vertex: gl.get_uniform_location(program, "u_emission_vertex"),
                emission_scale: gl.get_uniform_location(program, "u_emission_scale"),
                lightmap_enabled: gl.get_uniform_location(program, "u_lightmap_enabled"),
                light_scale: gl.get_uniform_location(program, "u_light_scale"),
                response_enabled: gl.get_uniform_location(program, "u_response_enabled"),
                normal_enabled: gl.get_uniform_location(program, "u_normal_enabled"),
                normal_strength: gl.get_uniform_location(program, "u_normal_strength"),
                specular: gl.get_uniform_location(program, "u_specular"),
                roughness: gl.get_uniform_location(program, "u_roughness"),
                opacity: gl.get_uniform_location(program, "u_opacity"),
                alpha_cutoff: gl.get_uniform_location(program, "u_alpha_cutoff"),
                fog_color: gl.get_uniform_location(program, "u_fog_color"),
                fog_density: gl.get_uniform_location(program, "u_fog_density"),
                fog_reference_y: gl.get_uniform_location(program, "u_fog_reference_y"),
                fog_height_gain: gl.get_uniform_location(program, "u_fog_height_gain"),
                reflect_mode: gl.get_uniform_location(program, "u_reflect_mode"),
                reflect_strength: gl.get_uniform_location(program, "u_reflect_strength"),
                planar_matrix: gl.get_uniform_location(program, "u_planar_matrix"),
                planar_plane: gl.get_uniform_location(program, "u_planar_plane"),
                emission_only: gl.get_uniform_location(program, "u_emission_only"),
            }
        }
    }

    /// Points the program's fixed sampler units at the units they read.
    ///
    /// The units never change for the lifetime of a program, so this runs once
    /// at startup. Every unit always has a texture bound before a draw (the
    /// white sheet stands in for a missing mask or normal map), so a shader that
    /// samples one anyway reads a defined value.
    unsafe fn bind_samplers(&self, gl: &glow::Context, program: glow::Program) {
        unsafe {
            gl.use_program(Some(program));
            if let Some(ref loc) = self.texture {
                gl.uniform_1_i32(Some(loc), SCENE_TEXTURE_UNIT);
            }
            if let Some(loc) = gl.get_uniform_location(program, "u_emission_mask") {
                gl.uniform_1_i32(Some(&loc), EMISSION_MASK_TEXTURE_UNIT);
            }
            if let Some(loc) = gl.get_uniform_location(program, "u_normal_map") {
                gl.uniform_1_i32(Some(&loc), NORMAL_MAP_TEXTURE_UNIT);
            }
            if let Some(loc) = gl.get_uniform_location(program, "u_lightmap0") {
                gl.uniform_1_i32(Some(&loc), LIGHTMAP_TEXTURE_UNIT);
            }
            if let Some(loc) = gl.get_uniform_location(program, "u_lightmap1") {
                gl.uniform_1_i32(Some(&loc), LIGHTMAP_TEXTURE_UNIT_1);
            }
            if let Some(loc) = gl.get_uniform_location(program, "u_probe_map") {
                gl.uniform_1_i32(Some(&loc), REFLECTION_PROBE_TEXTURE_UNIT);
            }
            if let Some(loc) = gl.get_uniform_location(program, "u_planar_map") {
                gl.uniform_1_i32(Some(&loc), REFLECTION_PLANAR_TEXTURE_UNIT);
            }
        }
    }
}

/// One scene attribute's `glVertexAttribPointer` description.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SceneAttributePointer {
    /// Component count (3 for a position, 2 for a UV, 4 for a colour).
    pub(super) components: i32,
    /// GL component type.
    pub(super) gl_type: u32,
    /// Whether the fixed function should normalize an integer type to `[0, 1]`.
    pub(super) normalized: bool,
    /// Byte offset inside the vertex.
    pub(super) offset: i32,
}

/// The pointer description of every scene attribute, for one vertex layout.
///
/// This is the single source of truth for *where* each shader attribute reads
/// from: [`Renderer::set_vertex_attributes`] walks it, and
/// `scene_attribute_table_matches_the_vertex_structs` pins every entry against
/// the offset constants and `offset_of!`. An attribute left out here is not
/// merely unread — it silently reads the generic default `(0, 0, 0, 1)`, which
/// is how a missing normal once collapsed every surface frame in the scene.
#[must_use]
pub(super) fn scene_attribute_table(
    layout: VertexLayout,
) -> [Option<SceneAttributePointer>; SCENE_ATTRIB_COUNT] {
    let packed = layout == VertexLayout::Packed;
    let frame = |packed_offset: i32, exact_offset: i32| SceneAttributePointer {
        components: 3,
        gl_type: if packed { glow::BYTE } else { glow::FLOAT },
        normalized: packed,
        offset: if packed { packed_offset } else { exact_offset },
    };
    [
        Some(SceneAttributePointer {
            components: 3,
            gl_type: glow::FLOAT,
            normalized: false,
            offset: if packed {
                packed_layout::POS_OFFSET
            } else {
                exact_layout::POS_OFFSET
            },
        }),
        Some(SceneAttributePointer {
            components: 4,
            gl_type: if packed {
                glow::UNSIGNED_BYTE
            } else {
                glow::FLOAT
            },
            normalized: packed,
            offset: if packed {
                packed_layout::COLOR_OFFSET
            } else {
                exact_layout::COLOR_OFFSET
            },
        }),
        Some(SceneAttributePointer {
            components: 2,
            gl_type: glow::FLOAT,
            normalized: false,
            offset: if packed {
                packed_layout::UV_OFFSET
            } else {
                exact_layout::UV_OFFSET
            },
        }),
        // The lightmap attributes keep their exact quantised type in *both*
        // layouts, so the packed and exact builds sample the atlas identically.
        Some(SceneAttributePointer {
            components: 2,
            gl_type: glow::UNSIGNED_SHORT,
            normalized: true,
            offset: if packed {
                packed_layout::LIGHTMAP_OFFSET
            } else {
                exact_layout::LIGHTMAP_OFFSET
            },
        }),
        Some(SceneAttributePointer {
            components: 1,
            gl_type: glow::UNSIGNED_BYTE,
            normalized: false,
            offset: if packed {
                packed_layout::LIGHTMAP_PAGE_OFFSET
            } else {
                exact_layout::LIGHTMAP_PAGE_OFFSET
            },
        }),
        Some(frame(
            packed_layout::NORMAL_OFFSET,
            exact_layout::NORMAL_OFFSET,
        )),
        Some(frame(
            packed_layout::TANGENT_OFFSET,
            exact_layout::TANGENT_OFFSET,
        )),
        Some(SceneAttributePointer {
            components: 1,
            gl_type: if packed { glow::BYTE } else { glow::FLOAT },
            normalized: packed,
            offset: if packed {
                packed_layout::HANDEDNESS_OFFSET
            } else {
                exact_layout::HANDEDNESS_OFFSET
            },
        }),
    ]
}

/// GPU state for the offscreen presentation pass: the program, its uniforms and
/// the four-vertex quad every frame draws.
struct PresentPass {
    program: glow::Program,
    vbo: glow::Buffer,
    u_mvp: Option<glow::UniformLocation>,
    u_scene: Option<glow::UniformLocation>,
}

/// The GL objects every renderer owns from startup: the three scene programs, the
/// UI vertex buffer, the untextured sheet, the generated font atlas and the
/// scene attribute locations.
struct StartupResources {
    programs: [glow::Program; ScenePass::ALL.len()],
    uniforms: [ProgramUniforms; ScenePass::ALL.len()],
    present: PresentPass,
    post: Option<super::postprocess::PostProcess>,
    /// One-texel black cube bound to the reflection-probe unit whenever no probe
    /// is resident, so the world shader always samples a complete cube.
    black_cube: glow::Texture,
    ui_vbo: glow::Buffer,
    white_texture: glow::Texture,
    font_texture: glow::Texture,
    decal: DecalPass,
    a_pos_loc: u32,
    a_color_loc: u32,
    a_uv_loc: u32,
    a_lightmap_uv_loc: u32,
    a_lightmap_page_loc: u32,
    a_normal_loc: u32,
    a_tangent_loc: u32,
    a_handedness_loc: u32,
}

impl StartupResources {
    /// Creates the renderer's startup GL objects and applies the base GL state.
    ///
    /// `white_sheet` is the decoded committed PNG for the shared untextured
    /// sheet; the caller loads it (see [`load_white_sheet`]) because reading an
    /// asset is not GL work.
    ///
    /// # Errors
    ///
    /// Returns a message when a shader program does not link or a texture,
    /// buffer or attribute lookup fails.
    unsafe fn create(
        gl: &glow::Context,
        white_sheet: &crate::loader::RawImage,
    ) -> Result<Self, String> {
        unsafe {
            gl.enable(glow::DEPTH_TEST);
            gl.depth_func(glow::LEQUAL);
            gl.clear_color(0.08, 0.08, 0.09, 1.0);

            let (programs, uniforms) = create_scene_programs(gl)?;
            let [
                a_pos_loc,
                a_color_loc,
                a_uv_loc,
                a_lightmap_uv_loc,
                a_lightmap_page_loc,
                a_normal_loc,
                a_tangent_loc,
                a_handedness_loc,
            ] = scene_attribute_locations(gl, programs)?;
            let present = create_present_pass(gl)?;

            // The committed untextured sheet, the generated font atlas and the
            // generated decal atlas are the only textures this renderer owns up
            // front. Every surface texture is uploaded by `set_level`, once per
            // distinct resolved texture.
            let white_texture = create_texture_2d(
                gl,
                i32::try_from(white_sheet.width).unwrap_or(i32::MAX),
                i32::try_from(white_sheet.height).unwrap_or(i32::MAX),
                &white_sheet.rgba,
                false,
                false,
            )?;
            let font_texture =
                create_texture_2d(gl, 128, 64, &generate_font_atlas(), false, false)?;

            // The decal pass: the same vertex stage with an alpha cut-out
            // fragment stage, and the one shared generated decal sheet. The
            // sheet uses repeat mip-mapping (as the world sheets do) so the
            // user's texture filtering applies; its UVs never leave the sheet,
            // so the wrap mode itself cannot show.
            let decal_program = create_program(gl, VERTEX_SHADER_SRC, DECAL_FRAGMENT_SHADER_SRC)?;
            let decal = DecalPass {
                program: decal_program,
                texture: create_texture_2d(
                    gl,
                    DECAL_ATLAS_SIZE,
                    DECAL_ATLAS_SIZE,
                    &generate_decal_atlas(),
                    true,
                    true,
                )?,
                external: Vec::new(),
                u_mvp_loc: gl.get_uniform_location(decal_program, "u_mvp"),
                u_texture_loc: gl.get_uniform_location(decal_program, "u_texture"),
                u_alpha_cutoff_loc: gl.get_uniform_location(decal_program, "u_alpha_cutoff"),
            };

            // Level geometry is uploaded by `rebuild_level_geometry` once the
            // renderer (and its prop asset cache) exists.
            let ui_vbo = gl.create_buffer()?;
            gl.bind_buffer(glow::ARRAY_BUFFER, None);

            // Post-processing is optional: a context that cannot link the
            // resolve and blur programs (or cannot allocate their targets at
            // resize) keeps the plain presentation copy instead. The diagnostic
            // is one line; nothing else changes.
            let post = create_post_process(gl);
            let black_cube = create_black_cube(gl)?;

            Ok(Self {
                programs,
                uniforms,
                present,
                post,
                black_cube,
                ui_vbo,
                white_texture,
                font_texture,
                decal,
                a_pos_loc,
                a_color_loc,
                a_uv_loc,
                a_lightmap_uv_loc,
                a_lightmap_page_loc,
                a_normal_loc,
                a_tangent_loc,
                a_handedness_loc,
            })
        }
    }
}

/// Catalog id of the renderer's shared untextured sheet.
///
/// The sheet is a real committed PNG (`assets/core/textures/white_01.png`)
/// rather than a pixel array generated in code: the asset policy requires a
/// permanent visible texture to be file-backed, and cataloguing it keeps it
/// under the same validators as every other shipped texture.
const WHITE_SHEET_ID: &str = "core:tex_white_01";

/// The same committed PNG, compiled in so a fresh install with no asset tree
/// can still bring the renderer up.
///
/// This is a reference to the real `assets/core/textures/white_01.png` bytes,
/// not imagery generated in code: the file-backed policy still holds, and the
/// catalog copy is preferred whenever one resolves. The embedded copy exists
/// only because the smoke-tested empty install (binary alone, no `assets/`)
/// must boot the embedded demo with every sampler holding a complete texture.
pub(super) const WHITE_SHEET_PNG: &[u8] = include_bytes!("../../assets/core/textures/white_01.png");

/// Loads the shared untextured white sheet.
///
/// The renderer binds this sheet to fixture housings, plain body geometry and
/// every texture slot that has nothing better to show, so it is loaded once at
/// startup like the font atlas. The catalog's file-backed PNG is the canonical
/// source; when no asset root or catalog entry resolves, the embedded copy of
/// that same PNG is decoded instead, so a compiled install with no asset tree
/// still boots. A malformed catalog sheet is still fatal: an install that
/// *does* ship assets must not silently substitute for a broken one.
///
/// # Errors
///
/// Returns a message when the white sheet PNG cannot be read or decoded.
pub(super) fn load_white_sheet() -> Result<crate::loader::RawImage, String> {
    if let Some(path) = shipped_asset_catalog().texture_path(WHITE_SHEET_ID)
        && let Some(root) = crate::assets::resolve_asset_root()
    {
        return crate::materials::load_png_relative(&root, path)
            .map_err(|error| format!("`{WHITE_SHEET_ID}`: {error}"));
    }
    crate::loader::decode_png(WHITE_SHEET_PNG)
        .map_err(|error| format!("`{WHITE_SHEET_ID}` (embedded): {error}"))
}

/// Creates the optional post-processing pipeline for the default profile.
///
/// A failure is reported once and yields `None`: the renderer then presents the
/// offscreen scene with the plain copy quad, exactly as it did before the
/// post-processing stage existed.
#[allow(clippy::print_stderr)] // the documented non-fatal fallback path
unsafe fn create_post_process(gl: &glow::Context) -> Option<super::postprocess::PostProcess> {
    match unsafe {
        super::postprocess::PostProcess::create(
            gl,
            super::postprocess::PostSettings::for_profile(crate::quality::QualityProfile::DEFAULT),
        )
    } {
        Ok(post) => Some(post),
        Err(error) => {
            crate::logging::warn_once(
                "postprocess-disabled",
                format!("[postprocess] disabled: {error}"),
            );
            None
        }
    }
}

/// Compiles the two world programs and looks up their uniform locations.
///
/// One shader body, two programs: the opaque world and the same world with an
/// alpha cut-out. Splitting them keeps `discard` out of the program every opaque
/// draw uses, where it would disable early depth testing for the whole frame.
///
/// # Errors
///
/// Returns a message when either program fails to compile or link.
unsafe fn create_scene_programs(
    gl: &glow::Context,
) -> Result<
    (
        [glow::Program; ScenePass::ALL.len()],
        [ProgramUniforms; ScenePass::ALL.len()],
    ),
    String,
> {
    unsafe {
        let world = create_program(gl, VERTEX_SHADER_SRC, &fragment_shader_source(false))?;
        let cutout = match create_program(gl, VERTEX_SHADER_SRC, &fragment_shader_source(true)) {
            Ok(program) => program,
            Err(error) => {
                gl.delete_program(world);
                return Err(error);
            }
        };
        let programs = [world, cutout];
        let uniforms = [
            ProgramUniforms::new(gl, world),
            ProgramUniforms::new(gl, cutout),
        ];
        for pass in ScenePass::ALL {
            if let (Some(uniforms), Some(program)) =
                (uniforms.get(pass.index()), programs.get(pass.index()))
            {
                uniforms.bind_samplers(gl, *program);
            }
        }
        Ok((programs, uniforms))
    }
}

/// Verifies that every scene attribute linked at its fixed slot.
///
/// The slots are bound before linking (see [`create_program`]), so the decal
/// pass can switch programs mid-frame without re-pointing the vertex attributes.
/// A driver that reorders them anyway is a hard startup error rather than a
/// silently mis-drawn frame.
///
/// # Errors
///
/// Returns a message when an attribute is missing or linked at another slot.
unsafe fn scene_attribute_locations(
    gl: &glow::Context,
    programs: [glow::Program; ScenePass::ALL.len()],
) -> Result<[u32; SCENE_ATTRIB_COUNT], String> {
    let Some(&program) = programs.get(ScenePass::World.index()) else {
        return Err("the world program is missing".to_string());
    };
    let mut locations = [0u32; SCENE_ATTRIB_COUNT];
    unsafe {
        for (slot, (name, expected)) in [
            ("a_pos", SCENE_ATTRIB_POS),
            ("a_color", SCENE_ATTRIB_COLOR),
            ("a_uv", SCENE_ATTRIB_UV),
            ("a_lightmap_uv", SCENE_ATTRIB_LIGHTMAP_UV),
            ("a_lightmap_page", SCENE_ATTRIB_LIGHTMAP_PAGE),
            ("a_normal", SCENE_ATTRIB_NORMAL),
            ("a_tangent", SCENE_ATTRIB_TANGENT),
            ("a_handedness", SCENE_ATTRIB_HANDEDNESS),
        ]
        .into_iter()
        .enumerate()
        {
            let location = gl
                .get_attrib_location(program, name)
                .ok_or_else(|| format!("Missing {name} attribute"))?;
            if location != expected {
                return Err(format!(
                    "attribute {name} linked at {location}, expected the fixed slot {expected}"
                ));
            }
            if let Some(target) = locations.get_mut(slot) {
                *target = location;
            }
        }
    }
    Ok(locations)
}

/// Creates the offscreen presentation program and its four-vertex quad.
///
/// # Errors
///
/// Returns a message when the program fails to compile or link, or a buffer
/// cannot be created.
unsafe fn create_present_pass(gl: &glow::Context) -> Result<PresentPass, String> {
    unsafe {
        let program = create_program(gl, PRESENT_VERTEX_SHADER_SRC, PRESENT_FRAGMENT_SHADER_SRC)?;
        let vbo = match gl.create_buffer() {
            Ok(buffer) => buffer,
            Err(error) => {
                gl.delete_program(program);
                return Err(error);
            }
        };
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
        // `f32` has no padding and the array is a plain buffer, so its raw
        // bytes are exactly what the attribute pointer reads back.
        let quad_bytes = std::slice::from_raw_parts(
            PRESENT_QUAD.as_ptr().cast::<u8>(),
            std::mem::size_of_val(&PRESENT_QUAD),
        );
        gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, quad_bytes, glow::STATIC_DRAW);
        gl.bind_buffer(glow::ARRAY_BUFFER, None);
        let present = PresentPass {
            program,
            vbo,
            u_mvp: gl.get_uniform_location(program, "u_mvp"),
            u_scene: gl.get_uniform_location(program, "u_scene"),
        };
        if let Some(loc) = present.u_scene.as_ref() {
            gl.use_program(Some(program));
            gl.uniform_1_i32(Some(loc), PRESENT_TEXTURE_UNIT);
        }
        Ok(present)
    }
}

/// Manages OpenGL ES 2.0-compatible accelerated rendering context, textures, and scene/UI drawing.
// The flags are independent GL/runtime state — culling, indexing, three
// lightmap switches and the filtering mode — not a state machine with one
// active variant: a level can be culled, indexed and lightmapped at once, and
// the benchmark toggles each one alone.
#[allow(clippy::struct_excessive_bools)]
pub struct Renderer {
    _gl_context: sdl2::video::GLContext,
    gl: glow::Context,
    /// The opaque world and alpha-tested world programs, in [`ScenePass`] order.
    programs: [glow::Program; ScenePass::ALL.len()],
    /// Uniform locations of each program, in the same order.
    program_uniforms: [ProgramUniforms; ScenePass::ALL.len()],
    /// Which program is currently bound, so a run of draws in one pass costs no
    /// program switch at all.
    current_pass: Option<ScenePass>,
    /// The surface state the current program was last set to.
    surface_state: Option<SurfaceState>,
    /// The per-frame state (lightmap switch, light scale, model transform,
    /// camera position) *this program* was last set to. Cleared on every program
    /// switch, because uniform state lives with the program.
    frame_state_valid: bool,
    /// View-projection matrix of the current scene frame, re-uploaded to
    /// whichever program starts drawing after a switch.
    scene_mvp: glam::Mat4,
    /// Camera position the sheen term measures the view direction from.
    camera_pos: glam::Vec3,
    /// Camera position currently uploaded to the bound program.
    uploaded_camera_pos: glam::Vec3,
    /// The light multiplier the bound program is currently drawing with, so the
    /// static/dynamic switch does not re-upload an unchanged uniform.
    light_scale: [f32; 3],
    /// Offscreen scene target, recreated whenever the drawable's size or the
    /// quality profile changes. `None` means "draw straight into the default
    /// framebuffer", which is what a failed target falls back to.
    scene_target: Option<framebuffer::SceneTarget>,
    /// Size the resident scene target was created for.
    scene_target_size: DrawableSize,
    /// Whether offscreen rendering is enabled at all (settings switch and the
    /// `LIMINAL_NO_OFFSCREEN=1` benchmark override).
    offscreen_enabled: bool,
    /// True once a target creation has failed, so the failure is reported once
    /// and every later frame takes the direct path.
    offscreen_failed: bool,
    /// The presentation pass: the offscreen scene copied onto the drawable.
    present: PresentPass,
    /// Scratch list of translucent draws, rebuilt every frame and sorted back to
    /// front. Kept across frames so the sort never allocates.
    translucent_scratch: Vec<TranslucentDraw>,
    /// Static-geometry buffer pairs: `(vbo, ibo)`, each addressable with
    /// 16-bit indices. A small level needs one; a large one needs several.
    level_buffers: Vec<(glow::Buffer, glow::Buffer)>,
    ui_vbo: glow::Buffer,
    /// Prop buffer pairs, split the same way as `level_buffers`.
    prop_buffers: Vec<(glow::Buffer, glow::Buffer)>,
    /// Cullable static ranges for the current level, one per (material, cell).
    static_batches: Vec<StaticBatch>,
    /// Spatial grid the current level was partitioned with, for the debug log.
    spatial_grid: crate::spatial::CellGrid,
    /// Whether frustum culling is applied. Only the benchmark harness turns it
    /// off, to measure what culling is worth on real hardware.
    culling_enabled: bool,
    /// Scratch buffer for packing UI vertices each frame (never grows per frame
    /// beyond the UI's own vertex count).
    ui_scratch: Vec<PackedVertex>,
    /// Vertex layout uploaded for the scene and the HUD.
    vertex_layout: VertexLayout,
    /// Whether geometry reaches the GPU as an indexed triangle list.
    indexing_enabled: bool,
    /// Vertex count of the last HUD upload, in whichever layout was used.
    ui_packed_len: usize,
    /// Catalog used to size and colour placed props. Loaded once at startup.
    prop_catalog: crate::loader::PropCatalog,
    /// Decoded prop models, shared between instances and cached across levels.
    prop_assets: crate::props::PropAssets,
    /// Per-model prop draw ranges for the current level.
    prop_draws: Vec<PropDraw>,
    /// CPU-side dynamic objects for the current level. Empty until the game
    /// spawns some (`set_dynamic_demo`); never part of the static bake, the
    /// static batches or the lightmap occlusion set. See [`super::dynamic`].
    dynamic: DynamicScene,
    /// GPU state for the dynamic scene's distinct meshes, keyed by model path:
    /// one small vertex/index pair uploaded once in model space.
    dynamic_meshes: std::collections::HashMap<String, DynamicMeshGpu>,
    /// [`DynamicScene::revision`] the GPU meshes were uploaded from. The draw
    /// path skips the scene while the two disagree, so a mesh is never drawn
    /// from stale buffers.
    dynamic_revision: u64,
    /// Baked static lighting, kept for the dynamic light probes. The static
    /// path needs it only while building, but a moving object samples it as it
    /// moves, so the renderer keeps the (small) bake alongside the level.
    dynamic_lighting: Option<LevelLighting>,
    /// Per-model prop GPU textures for the current level, indexed by model path
    /// and then by the model's own texture slot. Kept across level changes so a
    /// level switch never re-uploads a model that is already resident.
    prop_textures: std::collections::HashMap<String, Vec<glow::Texture>>,
    /// GPU textures for catalog/missing surface textures, keyed by logical
    /// texture key. Decoded images are already cached per session; this cache
    /// keeps their GPU copies across level changes, so a level switch never
    /// re-uploads a built-in texture.
    surface_textures: std::collections::HashMap<String, glow::Texture>,
    /// GPU textures for catalog fixture sheets, keyed by their PNG path and kept
    /// across level changes like `surface_textures` (fixture PNGs never tile, so
    /// they upload clamped, exactly like prop textures).
    fixture_sheet_textures: std::collections::HashMap<String, glow::Texture>,
    /// The current level's fixture sheets, one entry per
    /// [`crate::lighting::FixtureKind::ALL`] slot: the decoded visible face a
    /// light batch binds by its sheet index, or the untextured white sheet for a
    /// family this level does not place (or whose sheet did not resolve).
    fixture_sheets: Vec<glow::Texture>,
    /// Decoded decal-sheet PNGs, keyed by their catalog path. Decoded once per
    /// session exactly like surface textures.
    decal_image_cache: crate::materials::TextureCache,
    /// GPU textures for external decal sheets, keyed by their catalog path and
    /// kept across level changes like `surface_textures`.
    decal_sheet_textures: std::collections::HashMap<String, glow::Texture>,
    /// Per-level GPU textures (pack-supplied and diagnostic fallbacks), freed
    /// when the next level is uploaded. `(key, texture)` pairs so a texture
    /// shared by several materials is still deleted exactly once.
    level_textures: Vec<(String, glow::Texture)>,
    /// One GPU texture per entry of the loaded level's material table, indexed
    /// exactly like [`MaterialTable::textures`]. The draw loop binds by the
    /// material index a batch carries.
    material_textures: Vec<glow::Texture>,
    /// The per-material draw parameters, derived from the loaded level's
    /// resolved table in one place (see [`MaterialRenderState`]).
    materials: MaterialRenderState,
    /// Runtime quality profile: how large a texture may reach the GPU. Chosen
    /// once at startup from the settings (`full` or `low`).
    quality: crate::quality::QualityProfile,
    /// The untextured sheet every family without its own artwork binds: the
    /// light housing's flat vertex colour, a prop placeholder box, the UI.
    white_texture: glow::Texture,
    font_texture: glow::Texture,
    /// Decal rendering state (program, shared sheet, uniforms).
    decal: DecalPass,
    /// Lightmap atlas pages currently assigned to texture units 2 and 3. The
    /// shared white sheet stands in when a page is absent, so
    /// [`Self::bind_lightmap_units`] always has a complete texture for both.
    lightmap_pages: [glow::Texture; LIGHTMAP_PAGE_SLOTS],
    /// Whether [`Self::lightmap_pages`] holds a real baked atlas.
    lightmaps_resident: bool,
    /// Whether the world program is currently sampling the atlas. Follows
    /// `lightmaps_resident` for a normal level load; the settings switch and the
    /// benchmark harness can turn sampling off without dropping the atlas, which
    /// is what makes an A/B capture of the two lighting paths cheap.
    lightmaps_enabled: bool,
    /// Whether the *next* level build should bake lightmaps. Set before
    /// [`Self::set_level`]; a change takes effect on the next build, because a
    /// lightmapped mesh's vertex colours deliberately omit the baked light and
    /// cannot be re-interpreted as a vertex-lit level.
    lightmaps_requested: bool,
    /// GPU textures of the resident atlas pages, in page order, so they can be
    /// deleted on the next level load and re-filtered when the user changes the
    /// texture filtering setting.
    lightmap_textures: Vec<glow::Texture>,
    /// Bake cache for the current quality profile: an in-memory table plus the
    /// project-owned on-disk store under `target/level-cache/lightmaps/`.
    lightmap_cache: LightmapCache,
    a_pos_loc: u32,
    a_color_loc: u32,
    a_uv_loc: u32,
    a_lightmap_uv_loc: u32,
    a_lightmap_page_loc: u32,
    a_normal_loc: u32,
    a_tangent_loc: u32,
    a_handedness_loc: u32,
    /// Whether repeating 3D textures use linear (vs nearest) filtering. Wired
    /// to the user-facing `texture_filtering` setting.
    linear_filtering: bool,
    /// Physical framebuffer size currently being rendered to. Updated on resize
    /// and HiDPI/backing-scale changes via [`Renderer::set_drawable_size`].
    drawable_size: DrawableSize,
    /// Cost and shape of the most recently built level.
    level_stats: LevelBuildStats,
    /// Counters for the most recently submitted frame (see [`RenderStats`]).
    render_stats: RenderStats,
    /// Set when a level build produced reflection routing that still needs its
    /// probes baked; the bake runs once the level's textures are resident.
    pending_probe_bake: bool,
    /// Whether the bloom stage may run this session (`LIMINAL_NO_BLOOM`).
    bloom_enabled: bool,
    /// Set by the main pass when an emissive batch survived culling, so a view
    /// with nothing glowing skips the bloom stages entirely.
    emissive_visible: bool,
    /// Whether selective reflections may run this session
    /// (`LIMINAL_NO_REFLECTIONS`).
    reflections_enabled: bool,
    /// Emission animation per material index, resolved from the level.
    material_animations: Vec<Option<EmissionAnimation>>,
    /// Seconds since the level loaded, for the emission animations. Advanced by
    /// the simulation's own delta, never by wall-clock time, so a capture is
    /// reproducible.
    animation_seconds: f32,
    /// The mirror plane this frame's planar reflection was drawn through, if
    /// any. The matrix projects a world position into the reflected frame, so
    /// the same value both drew the image and samples it.
    active_plane: Option<ActiveReflectionPlane>,
    /// The mirror plane this frame's planar reflection is drawing, while the
    /// reflection image is being rendered. Its own geometry is skipped from
    /// that image (see [`super::reflections::is_mirror_range`]).
    capture_plane: Option<usize>,
    /// True while a pass is drawing a reflection: reflective materials must not
    /// read the image being written, and a probe bake must not read a cubemap
    /// that does not exist yet.
    reflection_capture: bool,
    /// Restrained post-processing: bloom, exposure/tone and a subtle grade. None
    /// when the context refused the extra programs, in which case the offscreen
    /// scene is presented by the plain copy quad.
    post: Option<super::postprocess::PostProcess>,
    /// Non-zero while a pass draws the world's emissive term alone into the
    /// bloom target.
    emission_only: f32,
    /// Atmospheric fog the world shader mixes in.
    fog: FogState,
    /// One-texel black cube bound to the reflection-probe unit when no probe is
    /// resident.
    black_cube: glow::Texture,
    /// Planar reflection planes the current level declares, and the state of the
    /// reflection pass for this frame.
    reflections: super::reflections::Reflections,
}

impl Renderer {
    /// Initializes an accelerated OpenGL context with `VSync`, textures and an
    /// empty level buffer.
    ///
    /// The caller uploads the first level with [`Renderer::set_level`] (or
    /// [`Renderer::rebuild_level_geometry`]); building here as well would bake
    /// and upload the same level twice before the first frame, wasting real
    /// startup time.
    /// # Errors
    ///
    /// Returns a message when the GL context is missing, the shader program does
    /// not link, or a texture, buffer or attribute lookup fails.
    pub fn new(window: &sdl2::video::Window, video: &sdl2::VideoSubsystem) -> Result<Self, String> {
        let gl_attr = video.gl_attr();
        gl_attr.set_double_buffer(true);
        gl_attr.set_depth_size(24);

        gl_attr.set_context_profile(sdl2::video::GLProfile::GLES);
        gl_attr.set_context_version(2, 0);

        let gl_context = if let Ok(ctx) = window.gl_create_context() {
            ctx
        } else {
            gl_attr.set_context_profile(sdl2::video::GLProfile::Compatibility);
            gl_attr.set_context_version(2, 1);
            window.gl_create_context()?
        };

        window.gl_make_current(&gl_context)?;

        // Swap interval is configured by the caller *after* this returns: the
        // request must be issued while a context is current, and the caller owns
        // the user's VSync setting. Forcing VSync on here silently overrode it.

        let gl = unsafe {
            glow::Context::from_loader_function(|proc_name| {
                video.gl_get_proc_address(proc_name).cast()
            })
        };

        let prop_catalog = crate::loader::PropCatalog::load_default();
        let white_sheet = load_white_sheet()?;
        let startup = unsafe { StartupResources::create(&gl, &white_sheet)? };
        let (initial_width, initial_height) = window.drawable_size();
        Ok(Self::from_startup(
            gl,
            gl_context,
            startup,
            prop_catalog,
            DrawableSize::new(initial_width, initial_height),
        ))
    }

    /// Assembles a renderer from the objects [`StartupResources::create`] built.
    ///
    /// Split out of [`Renderer::new`] so the context creation and the state it
    /// starts from are each readable on their own.
    fn from_startup(
        gl: glow::Context,
        gl_context: sdl2::video::GLContext,
        startup: StartupResources,
        prop_catalog: crate::loader::PropCatalog,
        drawable_size: DrawableSize,
    ) -> Self {
        Self {
            _gl_context: gl_context,
            gl,
            programs: startup.programs,
            program_uniforms: startup.uniforms,
            current_pass: None,
            surface_state: None,
            frame_state_valid: false,
            scene_mvp: glam::Mat4::IDENTITY,
            camera_pos: glam::Vec3::ZERO,
            uploaded_camera_pos: glam::Vec3::ZERO,
            light_scale: [1.0; 3],
            scene_target: None,
            scene_target_size: DrawableSize::new(0, 0),
            offscreen_enabled: offscreen_requested_from_env(),
            offscreen_failed: false,
            present: startup.present,
            translucent_scratch: Vec::new(),
            level_buffers: Vec::new(),
            ui_vbo: startup.ui_vbo,
            ui_scratch: Vec::new(),
            vertex_layout: VertexLayout::Packed,
            indexing_enabled: true,
            ui_packed_len: 0,
            prop_buffers: Vec::new(),
            static_batches: Vec::new(),
            spatial_grid: crate::spatial::CellGrid::default(),
            culling_enabled: true,
            prop_catalog,
            prop_assets: crate::props::PropAssets::load_default(),
            prop_draws: Vec::new(),
            dynamic: DynamicScene::new(),
            dynamic_meshes: std::collections::HashMap::new(),
            dynamic_revision: 0,
            dynamic_lighting: None,
            prop_textures: std::collections::HashMap::new(),
            surface_textures: std::collections::HashMap::new(),
            fixture_sheet_textures: std::collections::HashMap::new(),
            fixture_sheets: Vec::new(),
            decal_image_cache: crate::materials::TextureCache::new(),
            decal_sheet_textures: std::collections::HashMap::new(),
            level_textures: Vec::new(),
            materials: MaterialRenderState::default(),
            material_textures: Vec::new(),
            quality: crate::quality::QualityProfile::DEFAULT,
            bloom_enabled: bloom_requested_from_env(),
            emissive_visible: false,
            reflections_enabled: reflections_requested_from_env(),
            white_texture: startup.white_texture,
            font_texture: startup.font_texture,
            decal: startup.decal,
            lightmap_pages: [startup.white_texture; LIGHTMAP_PAGE_SLOTS],
            lightmaps_resident: false,
            lightmaps_enabled: false,
            lightmaps_requested: true,
            lightmap_textures: Vec::new(),
            lightmap_cache: LightmapCache::with_disk(),
            a_pos_loc: startup.a_pos_loc,
            a_color_loc: startup.a_color_loc,
            a_uv_loc: startup.a_uv_loc,
            a_lightmap_uv_loc: startup.a_lightmap_uv_loc,
            a_lightmap_page_loc: startup.a_lightmap_page_loc,
            a_normal_loc: startup.a_normal_loc,
            a_tangent_loc: startup.a_tangent_loc,
            a_handedness_loc: startup.a_handedness_loc,
            linear_filtering: true,
            drawable_size,
            level_stats: LevelBuildStats::default(),
            render_stats: RenderStats::default(),
            pending_probe_bake: false,
            material_animations: Vec::new(),
            animation_seconds: 0.0,
            active_plane: None,
            reflection_capture: false,
            capture_plane: None,
            post: startup.post,
            emission_only: 0.0,
            fog: FogState::SHIPPED,
            black_cube: startup.black_cube,
            reflections: super::reflections::Reflections::default(),
        }
    }

    /// Records the current physical framebuffer size.
    ///
    /// The offscreen scene target is sized from this value, so the next
    /// [`Self::render_scene`] rebuilds it when the drawable changed — a window
    /// resize, a monitor move or a `HiDPI` backing-scale change. Nothing is
    /// rebuilt here: the target is created lazily, in the frame that needs it.
    /// Returns `true` when the size changed.
    pub fn set_drawable_size(&mut self, size: DrawableSize) -> bool {
        if self.drawable_size == size {
            return false;
        }
        self.drawable_size = size;
        true
    }

    /// True when culling is on: the benchmark harness toggles it.
    #[must_use]
    pub const fn culling_enabled(&self) -> bool {
        self.culling_enabled
    }

    /// Selects the runtime quality profile.
    ///
    /// The profile decides how large a texture may reach the GPU: [`Full`] is
    /// the historical Places runtime size, [`Low`] downscales the same source
    /// assets further. It is applied when a level (or a texture) is uploaded,
    /// so changing it takes effect on the next level load rather than by
    /// rescaling anything already resident — and never per frame.
    ///
    /// The post-processing stack and the reflection budget follow the profile:
    /// Full blooms, grades and reflects through one mirror plane, Low keeps the
    /// resolve pass (tone and fog are part of the image, not an extra effect),
    /// drops bloom and grading, and reads only the static probes.
    ///
    /// [`Full`]: crate::quality::QualityProfile::Full
    /// [`Low`]: crate::quality::QualityProfile::Low
    pub fn set_quality(&mut self, quality: crate::quality::QualityProfile) {
        if self.quality == quality {
            return;
        }
        self.quality = quality;
        let settings = super::postprocess::PostSettings::for_profile(quality);
        if let Some(post) = self.post.as_mut() {
            unsafe { post.set_settings(&self.gl, settings) };
        }
        self.reflections.set_profile(quality);
        if !self.reflections_enabled {
            self.reflections.set_enabled(false);
        }
        // Whether a material reflects at all depends on the profile, and the
        // surface cache holds the previous answer.
        self.surface_state = None;
    }

    /// The active runtime quality profile.
    #[must_use]
    pub const fn quality(&self) -> crate::quality::QualityProfile {
        self.quality
    }

    /// Applies the user-facing texture filtering mode to the repeating 3D
    /// textures and to every cached prop texture. UI/atlas textures stay
    /// nearest-filtered to preserve crisp text.
    pub fn set_texture_filtering(&mut self, mode: &str) {
        let linear = mode != "nearest";
        if self.linear_filtering == linear {
            return;
        }
        self.linear_filtering = linear;
        unsafe {
            // The rebinds below unbind whichever unit is active at the end; pin
            // that to the scene unit so a lightmap unit is never left empty.
            self.gl.active_texture(glow::TEXTURE0);
            self.gl
                .bind_texture(glow::TEXTURE_2D, Some(self.decal.texture));
            set_repeat_filter(&self.gl, linear);
            for texture in self
                .decal
                .external
                .iter()
                .chain(self.decal_sheet_textures.values())
            {
                if *texture == self.decal.texture {
                    continue;
                }
                self.gl.bind_texture(glow::TEXTURE_2D, Some(*texture));
                set_repeat_filter(&self.gl, linear);
            }
            for texture in self.surface_textures.values() {
                self.gl.bind_texture(glow::TEXTURE_2D, Some(*texture));
                set_repeat_filter(&self.gl, linear);
            }
            for texture in self.fixture_sheet_textures.values() {
                self.gl.bind_texture(glow::TEXTURE_2D, Some(*texture));
                set_repeat_filter(&self.gl, linear);
            }
            for (_, texture) in &self.level_textures {
                self.gl.bind_texture(glow::TEXTURE_2D, Some(*texture));
                set_repeat_filter(&self.gl, linear);
            }
            for texture in self.prop_textures.values() {
                for texture in texture {
                    self.gl.bind_texture(glow::TEXTURE_2D, Some(*texture));
                    set_repeat_filter(&self.gl, linear);
                }
            }
            // Lightmap pages follow the same setting but keep mipmaps off.
            for texture in &self.lightmap_textures {
                self.gl.bind_texture(glow::TEXTURE_2D, Some(*texture));
                set_lightmap_filter(&self.gl, linear);
            }
            self.gl.bind_texture(glow::TEXTURE_2D, None);
        }
    }

    /// Number of draw calls the current level's props need (one per distinct
    /// model), exposed for the performance overlay and tests.
    pub const fn prop_draw_count(&self) -> usize {
        self.prop_draws.len()
    }

    /// Static-geometry and baked-lighting statistics for the current level.
    pub const fn level_stats(&self) -> LevelBuildStats {
        self.level_stats
    }

    /// Reads back the default framebuffer as a top-down RGBA image.
    ///
    /// Used by the `LIMINAL_CAPTURE` developer/hardware path: it is the only way
    /// to inspect real prop rendering on the `PocketCHIP` (no screenshots over
    /// SSH) and on desktops where the window cannot be captured. Call it after
    /// drawing and before swapping buffers.
    /// # Errors
    ///
    /// Returns a message when the drawable is empty or the GL readback returns a
    /// non-finite or truncated buffer.
    pub fn capture_default_framebuffer(&self) -> Result<crate::loader::RawImage, String> {
        let drawable = self.drawable_size;
        if drawable.is_empty() {
            return Err("drawable has zero size; nothing to capture".into());
        }
        let width = drawable.width as usize;
        let height = drawable.height as usize;
        let stride = width.saturating_mul(4);
        let mut pixels = vec![0u8; stride.saturating_mul(height)];
        unsafe {
            self.gl.read_pixels(
                0,
                0,
                i32::try_from(width).unwrap_or(i32::MAX),
                i32::try_from(height).unwrap_or(i32::MAX),
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(Some(&mut pixels)),
            );
        }
        // OpenGL returns bottom-up rows; flip into top-down image order.
        let mut flipped = vec![0u8; pixels.len()];
        for (row, target) in flipped.chunks_exact_mut(stride).enumerate() {
            let source_row = height.saturating_sub(1).saturating_sub(row);
            let source_start = source_row.saturating_mul(stride);
            let Some(source) = pixels.get(source_start..source_start.saturating_add(stride)) else {
                break;
            };
            target.copy_from_slice(source);
        }
        Ok(crate::loader::RawImage::new(
            drawable.width,
            drawable.height,
            flipped,
        ))
    }

    /// Cached prop asset statistics (models loaded/failed, triangles, texture bytes).
    pub fn prop_asset_stats(&self) -> crate::props::PropAssetStats {
        self.prop_assets.stats()
    }

    /// Builds one level (geometry, props, lighting and lightmaps) and uploads
    /// its atlas, falling back to an explicit vertex-lit rebuild when a page
    /// cannot upload.
    ///
    /// This is the load-time half of [`Self::rebuild_level_geometry`], split out
    /// so the lightmap state machine reads in one piece. The returned build's
    /// mesh matches whatever atlas ended up resident: a lightmapped mesh if the
    /// atlas uploaded, the historical vertex-lit mesh otherwise.
    // A failed page upload is a chatty one-line diagnostic and this renderer has
    // no logger (the game prints its own diagnostics directly), so the stderr
    // report is the intended behaviour and stays scoped to this method.
    #[allow(clippy::print_stderr)]
    fn build_level_for_load(
        &mut self,
        level: &crate::level::LevelDef,
        materials: &MaterialTable,
    ) -> LevelBuild {
        let options = LightmapBuildOptions::for_profile(
            self.quality,
            if self.lightmaps_requested {
                LightmapMode::On
            } else {
                LightmapMode::Off
            },
        );
        let mut build = build_level_geometry_timed_with_lightmaps(
            level,
            &self.prop_catalog,
            &mut self.prop_assets,
            materials,
            options,
            Some(&mut self.lightmap_cache),
        );

        // The atlas goes up before the geometry that samples it, and a page that
        // cannot upload forces one explicit rebuild with `LightmapMode::Off`:
        // a lightmapped mesh's vertex colours deliberately omit the baked light,
        // so it must never be drawn without a resident atlas.
        if options.mode == LightmapMode::On {
            let upload = if let Some(lightmaps) = build.lightmaps.as_deref() {
                self.upload_level_lightmaps(lightmaps)
            } else {
                self.clear_lightmap_pages();
                Ok(())
            };
            if let Err(error) = upload {
                crate::logging::warn_once(
                    format!("lightmap-upload:{}", level.id),
                    format!(
                        "[lightmaps] {error}; rebuilding '{}' with vertex lighting",
                        level.id
                    ),
                );
                self.clear_lightmap_pages();
                build = build_level_geometry_timed_with_lightmaps(
                    level,
                    &self.prop_catalog,
                    &mut self.prop_assets,
                    materials,
                    LightmapBuildOptions::for_profile(self.quality, LightmapMode::Off),
                    None,
                );
                build.lightmap_failure = Some(LightmapFailure::Upload);
            }
        } else {
            self.clear_lightmap_pages();
        }
        build
    }

    /// Builds and uploads the level's static geometry plus every placed prop.
    ///
    /// Called once per level load (never per frame): each distinct prop model is
    /// parsed once, every instance transform is baked into one shared vertex
    /// buffer, and each model's texture is uploaded once and then reused for the
    /// rest of the session.
    ///
    /// A failed texture or buffer upload is reported and skipped rather than
    /// fatal, so a broken asset degrades visibly instead of taking the level
    /// down.
    // A failed GPU upload is a chatty one-line diagnostic and this renderer has
    // no logger (the game prints its own diagnostics directly), so the stderr
    // report is the intended behaviour and stays scoped to this function.
    #[allow(clippy::print_stderr)]
    pub fn rebuild_level_geometry(
        &mut self,
        level: &crate::level::LevelDef,
        materials: &MaterialTable,
    ) {
        let started = std::time::Instant::now();
        let build = self.build_level_for_load(level, materials);

        let LevelBuild {
            mesh,
            batches,
            lighting,
            timings,
            lightmaps,
            lightmap_failure,
            lightmap_millis,
        } = build;
        let lightmap_stats = lightmaps.as_deref().map(|lightmaps| lightmaps.stats);
        self.spatial_grid = spatial_cell_grid(level);
        // Keep the bake for the dynamic-object light probes. The clone is once
        // per level load, never per frame, and `LevelLighting` is a few bytes
        // per room, fixture and blocker.
        self.dynamic_lighting = Some(lighting.clone());

        let index_ranges = self.indexing_enabled;
        let (static_packer, static_batches) = pack_static_batches(&mesh, index_ranges);
        self.static_batches = static_batches;
        // Props go through the same packer, one range per (model, cell).
        let (prop_packer, draws) = self.pack_prop_batches(&batches, index_ranges);

        if let Err(error) = upload_chunks(
            &self.gl,
            self.vertex_layout,
            &mut self.level_buffers,
            &static_packer.chunks,
        ) {
            crate::logging::warn_once(
                "level-upload",
                format!("[level] cannot upload static geometry: {error}"),
            );
        }
        if let Err(error) = upload_chunks(
            &self.gl,
            self.vertex_layout,
            &mut self.prop_buffers,
            &prop_packer.chunks,
        ) {
            eprintln!("[level] cannot upload prop geometry: {error}");
        }

        let static_vertices = mesh.vertex_count;
        let static_indices = mesh.index_count;
        let prop_vertices = prop_packer.vertex_total();
        let prop_indices = prop_packer.index_total();
        self.level_stats = LevelBuildStats {
            static_vertices,
            static_indices,
            prop_vertices,
            prop_indices,
            prop_draws: draws.len(),
            static_batches: self.static_batches.len(),
            static_chunks: self.level_buffers.len(),
            prop_chunks: self.prop_buffers.len(),
            vbo_bytes: static_vertices
                .saturating_add(prop_vertices)
                .saturating_mul(self.vertex_layout.vertex_bytes()),
            index_bytes: static_indices
                .saturating_add(prop_indices)
                .saturating_mul(std::mem::size_of::<u16>()),
            build_millis: started.elapsed().as_secs_f64() * 1000.0,
            lighting_millis: timings.lighting_millis,
            props_millis: timings.props_millis,
            surfaces_millis: timings.surfaces_millis,
            lighting: lighting.summary(),
            lightmap_pages: lightmap_stats.map_or(0, |stats| stats.pages),
            lightmap_texels: lightmap_stats.map_or(0, |stats| stats.page_texels),
            lightmap_charts: lightmap_stats.map_or(0, |stats| stats.charts),
            lightmap_chart_texels: lightmap_stats.map_or(0, |stats| stats.texels),
            lightmap_millis,
            lightmap_fallback: lightmap_failure.is_some(),
        };
        self.prop_draws = draws;
        self.resolve_scene_extras(level, materials, &mesh);
    }

    /// Resolves the per-level reflection routing and emission animations.
    ///
    /// Both are derived from the geometry and material table that were just
    /// built: the material table says *whether* a surface reflects or animates,
    /// and the batch's own vertices say which mirror plane it reflects on. The
    /// probe bake waits for [`Self::set_level`], which is where the surface
    /// textures and the render-side material table exist.
    fn resolve_scene_extras(
        &mut self,
        level: &crate::level::LevelDef,
        materials: &MaterialTable,
        mesh: &LevelMesh,
    ) {
        let reflections = materials
            .entries()
            .iter()
            .map(|entry| entry.reflection)
            .collect::<Vec<_>>();
        let routing = super::reflections::routing_from_mesh(mesh, &reflections, reflections.len());
        if !routing.is_empty() {
            Self::report_reflections(&routing);
        }
        unsafe { self.reflections.set_routing(&self.gl, routing) };
        self.reflections.set_profile(self.quality);
        if !self.reflections_enabled {
            self.reflections.set_enabled(false);
        }
        self.pending_probe_bake = true;

        // Emission animations, resolved from material ids to table indices. A
        // level that declares none leaves every entry `None`, which is the
        // identity multiplier and costs one uniform per draw.
        self.material_animations = vec![None; materials.entries().len()];
        self.animation_seconds = 0.0;
        for animation in &level.animated_emissions {
            let Some(index) = materials.index_of(animation.material.trim()) else {
                continue;
            };
            let effect = animation
                .effect
                .as_deref()
                .and_then(AnimationEffect::parse)
                .unwrap_or(AnimationEffect::Pulse);
            let resolved = EmissionAnimation {
                effect,
                hz: animation.hz.unwrap_or_else(|| effect.default_hz()),
                depth: animation.depth.unwrap_or_else(|| effect.default_depth()),
                phase: animation.phase.unwrap_or(0.0),
            }
            .sanitized();
            if let Some(slot) = self.material_animations.get_mut(usize::from(index)) {
                *slot = resolved.is_active().then_some(resolved);
            }
        }
    }

    /// Packs every prop batch and uploads one texture per distinct model.
    ///
    /// A model whose texture cannot be uploaded is reported and skipped.
    // The upload failure is a chatty one-line diagnostic and this renderer has
    // no logger (the game prints its own diagnostics directly), so the stderr
    // report is the intended behaviour and stays scoped to this method.
    #[allow(clippy::print_stderr)]
    fn pack_prop_batches(
        &mut self,
        batches: &[PropMeshBatch],
        indexed: bool,
    ) -> (MeshPacker, Vec<PropDraw>) {
        let mut packer = MeshPacker::default();
        let mut draws: Vec<PropDraw> = Vec::with_capacity(batches.len());

        // Upload each model's textures once, at the active quality profile, and
        // keep them resident across level changes. `gpu_textures` is indexed
        // exactly like `batch.textures`, so a submesh's `texture`/mask index
        // means the same thing on the CPU and on the GPU.
        let mut gpu_textures: std::collections::HashMap<String, Vec<glow::Texture>> =
            std::collections::HashMap::new();
        for batch in batches {
            if gpu_textures.contains_key(&batch.model) {
                continue;
            }
            if let Some(textures) = self.prop_textures.get(&batch.model) {
                gpu_textures.insert(batch.model.clone(), textures.clone());
                continue;
            }
            if let Some(textures) = self.upload_model_textures(&batch.model, &batch.textures) {
                gpu_textures.insert(batch.model.clone(), textures);
            }
        }

        for batch in batches {
            let Some(textures) = gpu_textures.get(&batch.model) else {
                continue;
            };
            // Each submesh is packed on its own so every returned placement is
            // exactly one material's index range: a chunk split never cuts a
            // draw across two materials, and instance order never affects what
            // a draw covers.
            for submesh in &batch.submeshes {
                let start = usize::try_from(submesh.first_index).unwrap_or(0);
                let count = usize::try_from(submesh.index_count).unwrap_or(0);
                let Some(indices) = batch.indices.get(start..start.saturating_add(count)) else {
                    continue;
                };
                let placements = if indexed {
                    packer.push(&batch.vertices, indices)
                } else {
                    packer.push_unindexed(&batch.vertices, indices)
                };
                let texture = submesh
                    .texture
                    .and_then(|slot| textures.get(usize::from(slot)).copied())
                    .unwrap_or(self.white_texture);
                let mask = submesh
                    .emission
                    .mask
                    .and_then(|slot| textures.get(usize::from(slot)).copied());
                for packed in placements {
                    draws.push(PropDraw {
                        texture,
                        emission: EmissionState::material(submesh.emission, mask),
                        chunk: packed.chunk,
                        index_start: packed.index_start,
                        index_count: packed.index_count,
                        vertex_count: packed.vertex_count,
                        bounds: batch.bounds,
                    });
                }
            }
        }
        (packer, draws)
    }

    /// Returns the image a texture class uploads at the active quality profile.
    ///
    /// Full keeps the image exactly as decoded (all shipped content is at or
    /// below the historical runtime size, so nothing is rescaled and no copy is
    /// made). Low box-filters it once, here, at upload time; the result is
    /// uploaded and dropped, so a texture is never rescaled per frame or twice
    /// for the same upload.
    /// Returns the image a texture class uploads at the active quality profile.
    ///
    /// See [`crate::quality::fit_image`] for the policy; this is the renderer's
    /// thin wrapper over it.
    fn fit_texture<'a>(
        &self,
        image: &'a crate::loader::RawImage,
        class: crate::quality::TextureClass,
    ) -> std::borrow::Cow<'a, crate::loader::RawImage> {
        crate::quality::fit_image(image, self.quality, class)
    }

    /// Uploads every texture one prop model uses, at the prop quality budget.
    ///
    /// Returns `None` after reporting a failure, and deletes any texture it had
    /// already created for that model, so a model is either fully resident or
    /// absent — a half-uploaded model would bind the white sheet to some of its
    /// primitives and its own artwork to others.
    // A failed upload is a chatty one-line diagnostic and this renderer has no
    // logger (the game prints its own diagnostics directly), so the stderr
    // report is the intended behaviour and stays scoped to this method.
    #[allow(clippy::print_stderr)]
    fn upload_model_textures(
        &mut self,
        model: &str,
        images: &[std::rc::Rc<crate::loader::RawImage>],
    ) -> Option<Vec<glow::Texture>> {
        let mut uploads: Vec<glow::Texture> = Vec::with_capacity(images.len());
        for image in images {
            match unsafe { self.upload_fitted_texture(image, crate::quality::TextureClass::Prop) } {
                Ok(texture) => uploads.push(texture),
                Err(error) => {
                    crate::logging::warn_once(
                        format!("prop-texture-upload:{model}"),
                        format!(
                            "[props] cannot upload texture for {model}: {error}; skipping that model"
                        ),
                    );
                    for texture in uploads {
                        unsafe { self.gl.delete_texture(texture) };
                    }
                    return None;
                }
            }
        }
        self.prop_textures
            .insert(model.to_string(), uploads.clone());
        Some(uploads)
    }

    /// Uploads one fitted (non-tiling) sheet: a prop's material texture or a
    /// fixture's visible face, at the active quality profile.
    ///
    /// Fitted artwork is sampled with `CLAMP_TO_EDGE` wrapping and mipmaps: its
    /// UVs never leave the sheet, so a repeat wrap would only bleed one edge of
    /// the artwork into the opposite edge. The mip chain and the game's
    /// filtering setting still apply, exactly as they do to a tiling surface.
    unsafe fn upload_fitted_texture(
        &self,
        image: &crate::loader::RawImage,
        class: crate::quality::TextureClass,
    ) -> Result<glow::Texture, String> {
        let source = self.fit_texture(image, class);
        unsafe {
            let texture = self.gl.create_texture()?;
            self.gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            self.gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA.cast_signed(),
                i32::try_from(source.width).unwrap_or(i32::MAX),
                i32::try_from(source.height).unwrap_or(i32::MAX),
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(&source.rgba)),
            );
            self.gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_S,
                glow::CLAMP_TO_EDGE.cast_signed(),
            );
            self.gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_T,
                glow::CLAMP_TO_EDGE.cast_signed(),
            );
            set_repeat_filter(&self.gl, self.linear_filtering);
            self.gl.generate_mipmap(glow::TEXTURE_2D);
            self.gl.bind_texture(glow::TEXTURE_2D, None);
            Ok(texture)
        }
    }

    /// Rebuilds the level's geometry and uploads one GPU texture per distinct
    /// resolved material texture, without recompiling anything.
    ///
    /// Catalog textures and the diagnostic fallback stay resident across level
    /// changes; pack textures belong to their level and are freed here when the
    /// next level replaces them. The decoded images are already cached per
    /// session by [`crate::materials::TextureCache`], so a level switch never
    /// re-reads or re-decodes a PNG.
    /// Resolves and uploads the current level's external decal sheets.
    ///
    /// The four built-in patterns come from the generated atlas; a decal asset
    /// with `source: "file"` and a `.png` model is ordinary external artwork,
    /// decoded once per session and uploaded once per level exactly like a
    /// surface texture, so a creator edits the PNG and restarts. A sheet that
    /// cannot be resolved draws the same magenta/black diagnostic the surface
    /// pipeline uses, so the mistake is visible in game instead of silent.
    // A broken decal sheet is a chatty one-line diagnostic and this renderer has
    // no logger (the game prints its own diagnostics directly), so the stderr
    // report is the intended behaviour and stays scoped to this function.
    #[allow(clippy::print_stderr)]
    fn load_decal_sheets(&mut self, level: &crate::level::LevelDef) {
        self.decal.external.clear();
        let root = crate::assets::resolve_asset_root();
        let catalog = shipped_asset_catalog();
        for id in decal_external_sheet_ids(level, catalog) {
            let resolved = crate::materials::resolve_decal_sheet(
                catalog,
                root.as_deref(),
                &mut self.decal_image_cache,
                &id,
            );
            let (key, image) = match resolved {
                Ok(sheet) => (sheet.key, sheet.image),
                Err(error) => {
                    crate::logging::warn_once(
                        format!("decal-sheet:{id}"),
                        format!("[decals] {error}; drawing the diagnostic sheet instead"),
                    );
                    (
                        crate::materials::MISSING_TEXTURE_KEY.to_string(),
                        std::rc::Rc::new(crate::materials::missing_texture()),
                    )
                }
            };
            if let Some(handle) = self.decal_sheet_textures.get(&key) {
                self.decal.external.push(*handle);
                continue;
            }
            let fitted = self.fit_texture(&image, crate::quality::TextureClass::DecalSheet);
            let uploaded = unsafe {
                create_texture_2d(
                    &self.gl,
                    i32::try_from(fitted.width).unwrap_or(i32::MAX),
                    i32::try_from(fitted.height).unwrap_or(i32::MAX),
                    &fitted.rgba,
                    true,
                    self.linear_filtering,
                )
            };
            match uploaded {
                Ok(texture) => {
                    self.decal_sheet_textures.insert(key, texture);
                    self.decal.external.push(texture);
                }
                Err(error) => {
                    crate::logging::warn_once(
                        format!("decal-upload:{id}"),
                        format!(
                            "[decals] decal `{id}`: {error}; drawing the built-in sheet instead"
                        ),
                    );
                    self.decal.external.push(self.decal.texture);
                }
            }
        }
    }

    // A failed texture upload is a chatty one-line diagnostic and this renderer
    // has no logger (the game prints its own diagnostics directly), so the
    // stderr report is the intended behaviour and stays scoped to this function.
    #[allow(clippy::print_stderr)]
    pub fn set_level(&mut self, loaded: &crate::loader::LoadedLevel) {
        self.rebuild_level_geometry(&loaded.level, &loaded.materials);
        self.load_decal_sheets(&loaded.level);
        let linear = self.linear_filtering;

        // Free the previous level's pack/missing uploads.
        unsafe {
            for (_, texture) in self.level_textures.drain(..) {
                if texture == self.white_texture || texture == self.font_texture {
                    continue;
                }
                self.gl.delete_texture(texture);
            }
        }

        let mut material_textures: Vec<glow::Texture> =
            Vec::with_capacity(loaded.materials.textures().len());
        let mut level_textures: Vec<(String, glow::Texture)> = Vec::new();
        for texture in loaded.materials.textures() {
            let persistent = !matches!(texture.origin, crate::materials::TextureOrigin::Pack);
            if persistent {
                if let Some(handle) = self.surface_textures.get(&texture.key) {
                    material_textures.push(*handle);
                    continue;
                }
            } else if let Some(handle) = level_textures
                .iter()
                .find(|(key, _)| *key == texture.key)
                .map(|(_, handle)| *handle)
            {
                material_textures.push(handle);
                continue;
            }

            let uploaded = unsafe {
                let source = self.fit_texture(&texture.image, texture.class);
                create_texture_2d(
                    &self.gl,
                    i32::try_from(source.width).unwrap_or(i32::MAX),
                    i32::try_from(source.height).unwrap_or(i32::MAX),
                    &source.rgba,
                    true,
                    linear,
                )
            };
            match uploaded {
                Ok(handle) => {
                    if persistent {
                        self.surface_textures.insert(texture.key.clone(), handle);
                    } else {
                        level_textures.push((texture.key.clone(), handle));
                    }
                    material_textures.push(handle);
                }
                Err(error) => {
                    crate::logging::warn_once(
                        format!("material-upload:{}", texture.key),
                        format!(
                            "[materials] cannot upload texture `{}`: {error}; binding the missing pattern",
                            texture.key
                        ),
                    );
                    material_textures.push(self.white_texture);
                }
            }
        }
        self.material_textures = material_textures;
        self.materials = MaterialRenderState::from_table(&loaded.materials);

        // The fixture sheets: one slot per family, so a light batch binds its
        // family's PNG by the sheet index it carries. Catalog sheets stay
        // resident across level changes; a pack's own sheet belongs to this
        // level and is freed with it. A family with no sheet binds the
        // untextured white sheet, which draws the flat authored fixture glow
        // exactly as every fixture did before the sheets existed.
        self.fixture_sheets = self.upload_fixture_sheets(&loaded.light_sheets, &mut level_textures);

        self.level_textures = level_textures;

        // Now that the geometry, the material table and the fixture sheets are
        // resident, one look around each probe point is enough to bake the
        // static reflection probes for the whole level.
        if std::mem::take(&mut self.pending_probe_bake) {
            self.bake_reflection_probes();
        }
    }

    /// Uploads the level's fixture sheets, one GPU texture per
    /// [`crate::lighting::FixtureKind`] slot.
    ///
    /// Every slot is filled: a family the level does not place (or whose sheet
    /// could not be uploaded) keeps the shared white sheet, so the draw path
    /// never has to guess whether a slot is set.
    // A failed upload is a chatty one-line diagnostic and this renderer has no
    // logger (the game prints its own diagnostics directly), so the stderr
    // report is the intended behaviour and stays scoped to this method.
    #[allow(clippy::print_stderr)]
    fn upload_fixture_sheets(
        &mut self,
        sheets: &[crate::loader::ResolvedFixtureSheet],
        level_textures: &mut Vec<(String, glow::Texture)>,
    ) -> Vec<glow::Texture> {
        let mut slots = vec![self.white_texture; crate::lighting::FixtureKind::ALL.len()];
        for sheet in sheets {
            let Some(slot) = slots.get_mut(sheet.kind.index()) else {
                continue;
            };
            let persistent = matches!(sheet.origin, crate::materials::TextureOrigin::Catalog);
            if persistent && let Some(handle) = self.fixture_sheet_textures.get(&sheet.key) {
                *slot = *handle;
                continue;
            }
            match unsafe {
                self.upload_fitted_texture(&sheet.image, crate::quality::TextureClass::FixtureFace)
            } {
                Ok(texture) => {
                    if persistent {
                        self.fixture_sheet_textures
                            .insert(sheet.key.clone(), texture);
                    } else {
                        level_textures.push((sheet.key.clone(), texture));
                    }
                    *slot = texture;
                }
                Err(error) => {
                    crate::logging::warn_once(
                        format!("fixture-upload:{}", sheet.key),
                        format!(
                            "[fixtures] cannot upload sheet `{}`: {error}; drawing the untextured sheet",
                            sheet.key
                        ),
                    );
                }
            }
        }
        slots
    }

    /// Renders the 3D level combining yaw and pitch into the view matrix.
    ///
    /// With offscreen rendering enabled (the default) the scene is drawn into a
    /// dedicated colour+depth target and then presented to the default
    /// framebuffer by one fullscreen quad; the UI is drawn afterwards, on the
    /// default framebuffer, at the drawable's own resolution. The target follows
    /// the drawable's size and aspect ratio (see
    /// [`framebuffer::scene_target_size`]), so nothing is stretched, and a target
    /// that cannot be created falls back to drawing straight into the default
    /// framebuffer — the historical path, one flag away.
    ///
    /// Takes `&mut self` because the pass records what it actually submitted
    /// (see [`RenderStats`]) for the debug-only benchmark harness.
    pub fn render_scene(
        &mut self,
        camera_pos: glam::Vec3,
        camera_yaw: f32,
        camera_pitch: f32,
        fov_degrees: f32,
    ) {
        let drawable = self.drawable_size;
        if drawable.is_empty() {
            return;
        }

        let target_size = offscreen_plan(
            self.offscreen_enabled,
            self.offscreen_failed,
            self.quality,
            drawable,
        );
        let offscreen = self.ensure_scene_target(target_size);
        let render_size = if offscreen {
            target_size.unwrap_or(drawable)
        } else {
            drawable
        };
        let cull = self.culling_enabled;
        let _ = offscreen;
        // The projection follows the *scene* aspect, which is the drawable's own
        // aspect scaled by a single factor, so the presented image is neither
        // stretched nor cropped.
        let (mvp, frustum) = scene_view_projection(
            camera_pos,
            camera_yaw,
            camera_pitch,
            fov_degrees,
            render_size,
        );
        self.camera_pos = camera_pos;
        self.scene_mvp = mvp;
        self.bind_scene_target(offscreen, render_size);

        // The lightmap units are bound by [`Self::draw_scene_body`], which every
        // scene submission goes through — including the level-load probe bake
        // that runs before the first frame.
        self.current_pass = None;
        self.frame_state_valid = false;
        // The counters the passes increment; the report below replaces the whole
        // structure, so this is the only place they are cleared.
        self.render_stats.texture_binds = 0;
        self.render_stats.material_changes = 0;

        // Reflections are recorded before the main pass, because the pass that
        // draws the scene needs to know what the reflective materials read from.
        // A planar plane is chosen by (in-frustum, then nearest); the emissive
        // image the bloom blurs is a separate small target drawn afterwards.
        self.render_stats.reflection_passes = 0;
        if offscreen {
            self.capture_reflections(&frustum, cull, &mvp, render_size);
            // The reflection pass drew into its own target: put the scene target
            // back before the main body, exactly as if it had never run.
            self.bind_scene_target(offscreen, render_size);
        } else {
            // The direct path never runs the reflection pass, so the declared
            // reflection samplers still need their complete fallbacks (the
            // nearest probe, or the black cube/white sheet when none exists).
            unsafe { self.bind_reflection_textures(None) };
        }

        // Static level geometry, then the batched props, then the dynamic
        // objects, each in the pass their material belongs to, then the
        // translucent pass sorted back to front, then the decal pass: one draw
        // loop each, in the order their state depends on. Dynamic objects are
        // opaque and belong *before* the decal pass so their fragments are
        // already in the depth buffer when a decal's alpha cut-out is tested
        // against them, and so the pass that changes program and state stays
        // last.
        let totals = self.draw_scene_body(&mvp, &frustum, cull);

        let bloom = if offscreen {
            self.capture_bloom(&frustum, cull, &mvp)
        } else {
            None
        };

        unsafe {
            self.disable_scene_attributes();
            self.gl.bind_texture(glow::TEXTURE_2D, None);
            self.gl.bind_buffer(glow::ARRAY_BUFFER, None);
            self.gl.use_program(None);
        }

        if offscreen {
            self.resolve_scene(bloom);
        }

        // Report what this frame actually submitted, straight from the draw
        // path rather than reconstructed from the level.
        let dynamic_draws = self.dynamic.draw_count();
        let dynamic_vertices = self.dynamic.vertex_count();
        let total_vertices = self
            .level_stats
            .static_vertices
            .saturating_add(self.level_stats.prop_vertices)
            .saturating_add(dynamic_vertices);
        self.render_stats = RenderStats {
            total_vertices,
            visible_vertices: totals.vertices,
            culled_vertices: total_vertices.saturating_sub(totals.vertices),
            total_batches: self
                .static_batches
                .len()
                .saturating_add(self.prop_draws.len())
                .saturating_add(dynamic_draws),
            visible_batches: totals.batches,
            draw_calls: totals.calls,
            dynamic_draws,
            dynamic_vertices,
            vbo_bytes: self.level_stats.vbo_bytes,
            index_bytes: self.level_stats.index_bytes,
            texture_binds: self.render_stats.texture_binds,
            material_changes: self.render_stats.material_changes,
            reflection_passes: self.render_stats.reflection_passes,
        };
    }

    /// Unbinds every attribute the scene programs read.
    unsafe fn disable_scene_attributes(&self) {
        unsafe {
            for location in [
                self.a_pos_loc,
                self.a_color_loc,
                self.a_uv_loc,
                self.a_lightmap_uv_loc,
                self.a_lightmap_page_loc,
                self.a_normal_loc,
                self.a_tangent_loc,
                self.a_handedness_loc,
            ] {
                self.gl.disable_vertex_attrib_array(location);
            }
        }
    }

    /// Creates, recreates or discards the offscreen scene target so it matches
    /// `size`, returning it when the scene should render offscreen.
    ///
    /// A target is recreated only when its size actually changes (a window
    /// resize, a `HiDPI` scale change or a quality-profile change), never per
    /// frame. A creation failure is reported once and disables the offscreen
    /// path for the session: the caller then draws straight into the default
    /// framebuffer, exactly as the renderer did before this path existed.
    // A GL target that cannot be allocated is the one case this path exists for;
    // there is no error channel above the renderer, so the diagnostic is a
    // one-line stderr report and the fallback is the documented behaviour.
    #[allow(clippy::print_stderr)]
    fn ensure_scene_target(&mut self, size: Option<DrawableSize>) -> bool {
        let Some(size) = size else {
            return false;
        };
        if self.scene_target.is_none() || self.scene_target_size != size {
            if let Some(existing) = self.scene_target.take() {
                unsafe { existing.destroy(&self.gl) };
            }
            self.scene_target_size = size;
            match unsafe { framebuffer::SceneTarget::create(&self.gl, size) } {
                Ok(target) => {
                    crate::logging::info(format!(
                        "[framebuffer] offscreen scene target {}x{} (RGBA8 colour, {}-bit depth)",
                        size.width,
                        size.height,
                        target.depth_bits()
                    ));
                    self.scene_target = Some(target);
                }
                Err(error) => {
                    crate::logging::warn_once(
                        "framebuffer-fallback",
                        format!(
                            "[framebuffer] {error}; drawing directly into the default framebuffer"
                        ),
                    );
                    self.offscreen_failed = true;
                    return false;
                }
            }
        }
        true
    }

    /// Presents the offscreen scene on the default framebuffer.
    ///
    /// One fullscreen quad, one texture, no blending and no depth test: the
    /// scene pass's own depth buffer stayed with the offscreen target, and the
    /// default framebuffer's depth is left untouched for the UI pass that
    /// follows. The target covers the whole drawable, so no clear is needed —
    /// but the framebuffer's depth is cleared anyway, so a later frame that
    /// takes the direct path starts from the same state.
    /// Points the GL draw target at the scene (or the window) and clears it.
    fn bind_scene_target(&self, offscreen: bool, render_size: DrawableSize) {
        unsafe {
            match self.scene_target.as_ref() {
                Some(target) if offscreen => target.bind(&self.gl),
                _ => self.gl.bind_framebuffer(glow::FRAMEBUFFER, None),
            }
            self.gl.viewport(
                0,
                0,
                i32::try_from(render_size.width).unwrap_or(i32::MAX),
                i32::try_from(render_size.height).unwrap_or(i32::MAX),
            );
            self.gl
                .clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
        }
    }

    /// Draws the whole 3D scene body with one view-projection.
    ///
    /// This is the one place the frame's draw order is written down; the main
    /// pass and every reflection pass run it with a different matrix. Dynamic
    /// objects are opaque and belong *before* the decal pass so their fragments
    /// are already in the depth buffer when a decal's alpha cut-out is tested
    /// against them, and so the pass that changes program and state stays last.
    fn draw_scene_body(&mut self, mvp: &glam::Mat4, frustum: &Frustum, cull: bool) -> DrawTotals {
        self.scene_mvp = *mvp;
        // The program and its uniforms belong to whichever body ran last.
        self.current_pass = None;
        self.frame_state_valid = false;
        self.emissive_visible = false;
        // Every world draw needs both lightmap units defined, including the
        // level-load probe bake that runs before the first frame.
        self.bind_lightmap_units();
        self.draw_static_pass(frustum, cull, BatchPass::Opaque, false)
            .plus(self.draw_prop_batches(frustum, cull, false))
            .plus(self.draw_dynamic_objects(frustum, cull, mvp, false))
            .plus(self.draw_static_pass(frustum, cull, BatchPass::Cutout, false))
            .plus(self.draw_translucent_pass(frustum, cull, mvp, false))
            .plus(self.draw_decal_batches(frustum, cull, mvp))
    }

    /// Draws only the world's emissive surfaces, for the bloom image.
    ///
    /// The emissive term is the one thing bloom is allowed to see; drawing it
    /// into its own small target is what keeps a brightly lit wall out of the
    /// glow no matter how bright its bake is, and the pass submits a handful of
    /// fixture, sign and screen batches at a quarter resolution.
    fn draw_emissive_body(
        &mut self,
        mvp: &glam::Mat4,
        frustum: &Frustum,
        cull: bool,
    ) -> DrawTotals {
        self.scene_mvp = *mvp;
        self.current_pass = None;
        self.frame_state_valid = false;
        self.emission_only = 1.0;
        // The emissive stage never samples the atlas (`u_emission_only` returns
        // before the lightmap term), but the units are bound anyway so every
        // body begins from the same defined state.
        self.bind_lightmap_units();
        let totals = self
            .draw_static_pass(frustum, cull, BatchPass::Opaque, true)
            .plus(self.draw_prop_batches(frustum, cull, true))
            .plus(self.draw_dynamic_objects(frustum, cull, mvp, true))
            .plus(self.draw_static_pass(frustum, cull, BatchPass::Cutout, true))
            .plus(self.draw_translucent_pass(frustum, cull, mvp, true));
        self.emission_only = 0.0;
        totals
    }

    /// Binds the reflection samplers for this frame.
    ///
    /// The probe unit always has a complete cube and the planar unit a complete
    /// 2D texture, because the world shader declares both samplers for every
    /// draw; the per-material weights decide whether anything is read.
    unsafe fn bind_reflection_textures(&mut self, planar: Option<glow::Texture>) {
        let probe = self
            .reflections
            .probe_texture([self.camera_pos.x, self.camera_pos.y, self.camera_pos.z])
            .unwrap_or(self.black_cube);
        unsafe {
            self.gl
                .active_texture(super::view::texture_unit(REFLECTION_PROBE_TEXTURE_UNIT));
            self.gl.bind_texture(glow::TEXTURE_CUBE_MAP, Some(probe));
            self.gl
                .active_texture(super::view::texture_unit(REFLECTION_PLANAR_TEXTURE_UNIT));
            self.gl
                .bind_texture(glow::TEXTURE_2D, Some(planar.unwrap_or(self.white_texture)));
            self.gl.active_texture(glow::TEXTURE0);
        }
        self.render_stats.texture_binds = self.render_stats.texture_binds.saturating_add(2);
    }

    /// Chooses and draws this frame's planar reflection, and binds the
    /// reflection samplers.
    ///
    /// At most one plane is reflected per frame: the nearest one whose
    /// reflective geometry survived the frustum test. That is the deliberate
    /// limit — a second plane would double the scene submission again for a
    /// mirror edge the player is far less likely to be looking at.
    ///
    /// The mirror plane's own surface is left out of the mirrored draw: it is
    /// the nearest thing to the mirrored camera, so drawing it would fill the
    /// image with the mirror's own colour instead of the room it faces. The
    /// plane is an aperture, so leaving it out is exactly what a real mirror
    /// shows.
    fn capture_reflections(
        &mut self,
        frustum: &Frustum,
        cull: bool,
        mvp: &glam::Mat4,
        render_size: DrawableSize,
    ) {
        self.active_plane = None;
        let plane_index = self.nearest_visible_reflection_plane(frustum);
        let mut planar_texture = None;
        if let Some(index) = plane_index
            && self.reflections.planar_enabled()
        {
            let Some(plane) = self.reflections.routing.planes.get(index).copied() else {
                unsafe { self.bind_reflection_textures(None) };
                return;
            };
            let ready = unsafe { self.reflections.ensure_planar(&self.gl, render_size) };
            if ready {
                let mirror = super::reflections::mirror_matrix(plane.normal, plane.offset);
                let [nx, ny, nz] = plane.normal;
                // `glam` matrix products are per-element `f32` arithmetic with
                // no overflow or panic path.
                #[allow(clippy::arithmetic_side_effects)]
                let mirrored_mvp = *mvp * mirror;
                let mirrored_camera = super::reflections::mirror_point(
                    plane.normal,
                    plane.offset,
                    [self.camera_pos.x, self.camera_pos.y, self.camera_pos.z],
                );
                let frustum_mirrored = Frustum::from_view_projection(
                    &mirrored_mvp,
                    crate::spatial::DepthRange::NegativeOneToOne,
                );
                let planar_color = self
                    .reflections
                    .planar()
                    .map(super::reflections::PlanarTarget::color);
                if self.reflections.planar().is_some() {
                    if let Some(planar) = self.reflections.planar() {
                        unsafe { planar.bind(&self.gl) };
                    }
                    unsafe {
                        self.gl.clear_color(0.08, 0.08, 0.09, 1.0);
                        self.gl
                            .clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
                    }
                    // The mirrored transform reverses every triangle's winding,
                    // and the world shader flips the shading normal on a back
                    // face. Telling GL which winding is front keeps the
                    // reflected image shaded exactly like the real one.
                    unsafe { self.gl.front_face(glow::CW) };
                    self.reflection_capture = true;
                    self.capture_plane = Some(index);
                    let saved_camera = self.camera_pos;
                    self.camera_pos =
                        glam::Vec3::new(mirrored_camera[0], mirrored_camera[1], mirrored_camera[2]);
                    let _ = self.draw_scene_body(&mirrored_mvp, &frustum_mirrored, cull);
                    self.camera_pos = saved_camera;
                    self.capture_plane = None;
                    self.reflection_capture = false;
                    unsafe { self.gl.front_face(glow::CCW) };
                    self.render_stats.reflection_passes =
                        self.render_stats.reflection_passes.saturating_add(1);
                    planar_texture = planar_color;
                    self.active_plane = Some(ActiveReflectionPlane {
                        index,
                        matrix: mirrored_mvp,
                        plane: [nx, ny, nz, plane.offset],
                    });
                }
            }
        }
        unsafe { self.bind_reflection_textures(planar_texture) };
    }

    /// Draws the emissive image and blurs it into the bloom texture.
    ///
    /// `None` when the profile does not bloom or no target could be allocated,
    /// in which case the resolve pass adds nothing.
    fn capture_bloom(
        &mut self,
        frustum: &Frustum,
        cull: bool,
        mvp: &glam::Mat4,
    ) -> Option<glow::Texture> {
        // `LIMINAL_NO_BLOOM=1` measures the same build without the stage, and a
        // view with no emissive surface on screen skips it without one: bloom
        // can only come from emission, so there is nothing to draw.
        if !self.bloom_enabled || !self.emissive_visible {
            return None;
        }
        // The bloom target shares the scene target's aspect ratio (both scale
        // by the same factor), so the scene's own view-projection and frustum
        // are the right ones. The emissive pass is a scatter of small bright
        // rectangles: at a quarter resolution the blur cannot tell.
        let render_size = self.scene_target_size;
        let scene_depth = self
            .scene_target
            .as_ref()
            .map(framebuffer::SceneTarget::depth_buffer);
        {
            let post = self.post.as_mut()?;
            if !unsafe { post.ensure_targets(&self.gl, render_size, scene_depth) } {
                return None;
            }
        }
        {
            let post = self.post.as_ref()?;
            unsafe { post.begin_emissive(&self.gl) }?;
        }
        let _ = self.draw_emissive_body(mvp, frustum, cull);
        let post = self.post.as_ref()?;
        unsafe { super::postprocess::PostProcess::end_emissive(&self.gl) };
        unsafe { post.finish_bloom(&self.gl) }
    }

    /// The nearest mirror plane whose reflective geometry is on screen.
    fn nearest_visible_reflection_plane(&self, frustum: &Frustum) -> Option<usize> {
        let mut best: Option<(f32, usize)> = None;
        for (index, plane) in self.reflections.routing.planes.iter().enumerate() {
            if !frustum.intersects_aabb(&plane.bounds) {
                continue;
            }
            let distance = plane
                .distance([self.camera_pos.x, self.camera_pos.y, self.camera_pos.z])
                .abs();
            if !distance.is_finite() {
                continue;
            }
            if best.is_none_or(|(nearest, _)| distance < nearest) {
                best = Some((distance, index));
            }
        }
        best.map(|(_, index)| index)
    }

    /// Turns the offscreen scene into the display image.
    ///
    /// The post-processing resolve when the pipeline exists, the plain copy quad
    /// when it does not: the copy shows the scene texture unchanged and is
    /// pixel-identical to the direct (no-offscreen) presentation.
    fn resolve_scene(&mut self, bloom: Option<glow::Texture>) {
        let drawable = self.drawable_size;
        let Some(scene) = self
            .scene_target
            .as_ref()
            .map(framebuffer::SceneTarget::color)
        else {
            return;
        };
        let post = self.post.as_ref();
        if post.is_some_and(|post| post.settings().is_identity()) {
            // Nothing to add, expose or grade: the plain copy quad is the same
            // image for less work. The targets are dropped so the memory is not
            // held for a stage that never runs.
            self.present_scene();
            return;
        }
        let (bloom_strength, bloom_texture) = match (post, bloom) {
            (Some(post), Some(bloom)) if post.blooms() => (post.settings().bloom_strength, bloom),
            // No bloom this frame: the sampler still needs a complete texture,
            // so it reads the scene and multiplies it by zero.
            _ => (0.0, scene),
        };
        match post {
            Some(post) => unsafe {
                post.resolve(&self.gl, scene, bloom_texture, bloom_strength, drawable);
                self.render_stats.texture_binds = self.render_stats.texture_binds.saturating_add(2);
                self.gl.clear(glow::DEPTH_BUFFER_BIT);
            },
            None => self.present_scene(),
        }
    }

    /// Bakes the static reflection probes for the current level.
    ///
    /// Six scene submissions per probe, once per level load. A probe is the
    /// cheap half of the reflection support: after this, sampling it costs one
    /// texture read per reflective fragment and nothing per frame.
    fn bake_reflection_probes(&mut self) {
        let wanted = self.reflections.wanted_probe_count();
        if wanted == 0 {
            return;
        }
        let started = std::time::Instant::now();
        // The bake is the first thing that ever draws the world program, and it
        // runs before any frame has bound the reflection units. Both must hold a
        // complete texture already — the probe cubemap does not exist yet, so the
        // shared fallbacks are exactly right — or the driver reports the empty
        // binding as an unloadable sampler on the very first face and substitutes
        // a zero texture.
        unsafe { self.bind_reflection_textures(None) };
        let points = self.reflections.routing.probe_points.clone();
        let profile = self.quality;
        for point in points.into_iter().take(wanted) {
            // A probe sits a little above the surface that asked for it: a
            // floor's centroid is on the floor, and a cubemap baked *in* a
            // surface sees nothing.
            let position = [point[0], point[1] + 1.2, point[2]];
            let probe = match unsafe {
                super::reflections::ProbeTarget::create(&self.gl, profile, position)
            } {
                Ok(probe) => probe,
                Err(error) => {
                    Self::report_reflection_failure(&error);
                    return;
                }
            };
            let side = probe.face_texels();
            let projection = glam::Mat4::perspective_rh_gl(
                std::f32::consts::FRAC_PI_2,
                1.0,
                SCENE_NEAR_M,
                SCENE_FAR_M,
            );
            let eye = glam::Vec3::new(position[0], position[1], position[2]);
            for (face, direction, up) in super::reflections::CUBE_FACES {
                unsafe { probe.bind_face(&self.gl, face) };
                unsafe {
                    self.gl.viewport(
                        0,
                        0,
                        i32::try_from(side).unwrap_or(i32::MAX),
                        i32::try_from(side).unwrap_or(i32::MAX),
                    );
                    self.gl.clear_color(0.08, 0.08, 0.09, 1.0);
                    self.gl
                        .clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
                }
                let target = glam::Vec3::new(direction[0], direction[1], direction[2]);
                let up = glam::Vec3::new(up[0], up[1], up[2]);
                #[allow(clippy::arithmetic_side_effects)] // glam float math
                let (view, mvp) = {
                    let view = glam::Mat4::look_at_rh(eye, eye + target, up);
                    (view, projection * view)
                };
                let _ = view;
                let frustum = Frustum::from_view_projection(
                    &mvp,
                    crate::spatial::DepthRange::NegativeOneToOne,
                );
                // The probe is an image of the room, not of the reflections in
                // it: suppressing them here is what stops a mirror from baking
                // its own uninitialised cubemap into the probe.
                self.reflection_capture = true;
                let saved_camera = self.camera_pos;
                self.camera_pos = eye;
                let _ = self.draw_scene_body(&mvp, &frustum, true);
                self.camera_pos = saved_camera;
                self.reflection_capture = false;
            }
            self.reflections.push_probe(probe);
        }
        self.scene_mvp = glam::Mat4::IDENTITY;
        let millis = started.elapsed().as_secs_f64() * 1000.0;
        crate::logging::info(format!(
            "[reflections] baked {} probe(s) at {} texels/face in {millis:.1} ms",
            self.reflections.probes().len(),
            self.reflections
                .probes()
                .first()
                .map_or(0, super::reflections::ProbeTarget::face_texels)
        ));
    }

    fn report_reflections(routing: &super::reflections::ReflectionRouting) {
        crate::logging::info(format!(
            "[reflections] {} mirror plane(s), {} probe point(s)",
            routing.planes.len(),
            routing.probe_points.len()
        ));
    }

    fn report_reflection_failure(error: &str) {
        crate::logging::warn_once(
            "reflection-probe-disabled",
            format!("[reflections] probe baking disabled: {error}"),
        );
    }

    fn present_scene(&mut self) {
        let drawable = self.drawable_size;
        let Some(target) = self.scene_target.as_ref() else {
            return;
        };
        let color = target.color();
        unsafe {
            self.gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            self.gl.viewport(
                0,
                0,
                i32::try_from(drawable.width).unwrap_or(i32::MAX),
                i32::try_from(drawable.height).unwrap_or(i32::MAX),
            );
            self.gl.disable(glow::DEPTH_TEST);
            self.gl.disable(glow::BLEND);
            self.gl.use_program(Some(self.present.program));
            if let Some(ref loc) = self.present.u_mvp {
                let matrix = framebuffer::present_matrix();
                self.gl
                    .uniform_matrix_4_f32_slice(Some(loc), false, &matrix.to_cols_array());
            }
            self.gl.active_texture(glow::TEXTURE0);
            self.gl.bind_texture(glow::TEXTURE_2D, Some(color));
            self.render_stats.texture_binds = self.render_stats.texture_binds.saturating_add(1);
            self.gl
                .bind_buffer(glow::ARRAY_BUFFER, Some(self.present.vbo));
            self.gl
                .vertex_attrib_pointer_f32(self.a_pos_loc, 3, glow::FLOAT, false, 12, 0);
            self.gl.enable_vertex_attrib_array(self.a_pos_loc);
            self.gl.draw_arrays(glow::TRIANGLES, 0, 6);
            self.gl.disable_vertex_attrib_array(self.a_pos_loc);
            self.gl.bind_buffer(glow::ARRAY_BUFFER, None);
            self.gl.bind_texture(glow::TEXTURE_2D, None);
            self.gl.use_program(None);
            self.gl.enable(glow::DEPTH_TEST);
            // The presented quad covers the drawable; only the depth buffer
            // needs clearing for whatever draws next (the HUD does not depth
            // test, but the next scene frame's direct path might).
            self.gl.clear(glow::DEPTH_BUFFER_BIT);
        }
    }

    /// The linked program of one scene pass, when it exists.
    fn program_for(&self, pass: ScenePass) -> Option<glow::Program> {
        self.programs.get(pass.index()).copied()
    }

    /// The uniform table of one scene pass, when it exists.
    fn uniforms_for(&self, pass: ScenePass) -> Option<&ProgramUniforms> {
        self.program_uniforms.get(pass.index())
    }

    /// Binds one scene program and (re)uploads the per-frame state it owns.
    ///
    /// Uniform state in OpenGL is per program, so a switch invalidates
    /// everything the previous program held: the lightmap switch, the light
    /// scale, the model transform and the camera position are all re-uploaded
    /// here. A run of passes that stay on the same program pays nothing.
    fn begin_pass(&mut self, pass: ScenePass) {
        if self.current_pass == Some(pass) && self.frame_state_valid {
            return;
        }
        if self.current_pass != Some(pass) {
            let Some(program) = self.program_for(pass) else {
                return;
            };
            unsafe { self.gl.use_program(Some(program)) };
            self.current_pass = Some(pass);
            // The new program has never been told about the current surface.
            self.surface_state = None;
        }
        self.upload_frame_state(pass);
    }

    /// Pushes the per-frame uniforms to the program of `pass`.
    fn upload_frame_state(&mut self, pass: ScenePass) {
        let Some(uniforms) = self.uniforms_for(pass) else {
            return;
        };
        let mvp = self.scene_mvp.to_cols_array();
        let camera = [self.camera_pos.x, self.camera_pos.y, self.camera_pos.z];
        let scale = self.light_scale;
        let lightmap_on = if self.lightmaps_enabled { 1.0 } else { 0.0 };
        unsafe {
            if let Some(ref loc) = uniforms.mvp {
                self.gl.uniform_matrix_4_f32_slice(Some(loc), false, &mvp);
            }
            if let Some(ref loc) = uniforms.model {
                self.gl.uniform_matrix_4_f32_slice(
                    Some(loc),
                    false,
                    &glam::Mat4::IDENTITY.to_cols_array(),
                );
            }
            if let Some(ref loc) = uniforms.camera_pos {
                self.gl
                    .uniform_3_f32(Some(loc), camera[0], camera[1], camera[2]);
            }
            if let Some(ref loc) = uniforms.light_scale {
                self.gl
                    .uniform_3_f32(Some(loc), scale[0], scale[1], scale[2]);
            }
            if let Some(ref loc) = uniforms.lightmap_enabled {
                self.gl.uniform_1_f32(Some(loc), lightmap_on);
            }
            let fog = self.fog;
            if let Some(ref loc) = uniforms.fog_color {
                self.gl
                    .uniform_3_f32(Some(loc), fog.color[0], fog.color[1], fog.color[2]);
            }
            if let Some(ref loc) = uniforms.fog_density {
                self.gl.uniform_1_f32(Some(loc), fog.density);
            }
            if let Some(ref loc) = uniforms.fog_reference_y {
                self.gl.uniform_1_f32(Some(loc), fog.reference_y);
            }
            if let Some(ref loc) = uniforms.fog_height_gain {
                self.gl.uniform_1_f32(Some(loc), fog.height_gain);
            }
            if let Some(ref loc) = uniforms.emission_only {
                self.gl.uniform_1_f32(Some(loc), self.emission_only);
            }
            // The planar reflection's own view-projection and plane, zeroed
            // when this frame has no active mirror: a material can only read
            // the reflection image when its plane was the one drawn, and the
            // matrix is what projects a world position into that image.
            let (planar_matrix, planar_plane) = self.active_plane.map_or_else(
                || (glam::Mat4::IDENTITY.to_cols_array(), [0.0, 0.0, 1.0, 0.0]),
                |active| (active.matrix.to_cols_array(), active.plane),
            );
            if let Some(ref loc) = uniforms.planar_matrix {
                self.gl
                    .uniform_matrix_4_f32_slice(Some(loc), false, &planar_matrix);
            }
            if let Some(ref loc) = uniforms.planar_plane {
                self.gl.uniform_4_f32(
                    Some(loc),
                    planar_plane[0],
                    planar_plane[1],
                    planar_plane[2],
                    planar_plane[3],
                );
            }
        }
        self.uploaded_camera_pos = self.camera_pos;
        self.frame_state_valid = true;
    }

    /// Re-points the model transform and light scale for one dynamic object.
    ///
    /// Everything else about the frame state stays as it is; only these two
    /// change per object, and only for the dynamic path.
    fn set_dynamic_frame_state(&mut self, model: glam::Mat4, mvp: glam::Mat4, scale: [f32; 3]) {
        let Some(pass) = self.current_pass else {
            return;
        };
        let Some(uniforms) = self.uniforms_for(pass) else {
            return;
        };
        let model_columns = model.to_cols_array();
        let mvp_columns = mvp.to_cols_array();
        unsafe {
            if let Some(ref loc) = uniforms.model {
                self.gl
                    .uniform_matrix_4_f32_slice(Some(loc), false, &model_columns);
            }
            if let Some(ref loc) = uniforms.mvp {
                self.gl
                    .uniform_matrix_4_f32_slice(Some(loc), false, &mvp_columns);
            }
            if let Some(ref loc) = uniforms.light_scale {
                self.gl
                    .uniform_3_f32(Some(loc), scale[0], scale[1], scale[2]);
            }
        }
    }

    /// Restores the frame's own MVP and light scale after the dynamic path.
    fn restore_frame_mvp(&mut self) {
        let Some(pass) = self.current_pass else {
            return;
        };
        let Some(uniforms) = self.uniforms_for(pass) else {
            return;
        };
        let mvp = self.scene_mvp.to_cols_array();
        unsafe {
            if let Some(ref loc) = uniforms.mvp {
                self.gl.uniform_matrix_4_f32_slice(Some(loc), false, &mvp);
            }
            if let Some(ref loc) = uniforms.model {
                self.gl.uniform_matrix_4_f32_slice(
                    Some(loc),
                    false,
                    &glam::Mat4::IDENTITY.to_cols_array(),
                );
            }
            if let Some(ref loc) = uniforms.light_scale {
                self.gl.uniform_3_f32(Some(loc), 1.0, 1.0, 1.0);
            }
        }
        self.light_scale = [1.0; 3];
    }

    /// Submits every static batch of one pass the frustum keeps, binding each
    /// material state and buffer pair once per run.
    ///
    /// Decals are skipped here: they are submitted by their own pass, with the
    /// decal program and depth bias, which keeps the world program's early
    /// depth testing intact. Translucent batches are collected by
    /// [`Self::draw_translucent_pass`], which sorts them.
    fn draw_static_pass(
        &mut self,
        frustum: &Frustum,
        cull: bool,
        pass: BatchPass,
        emissive_only: bool,
    ) -> DrawTotals {
        let mut totals = DrawTotals::default();
        let mut begun = false;
        let mut bound_key: Option<SurfaceKey> = None;
        let mut bound_chunk: Option<usize> = None;
        for index in 0..self.static_batches.len() {
            let Some(batch) = self.static_batches.get(index).copied() else {
                continue;
            };
            if batch.index_range.count <= 0 {
                continue;
            }
            if batch.key.kind == SurfaceKind::Decal {
                continue;
            }
            if self.is_capture_mirror(batch.key) {
                continue;
            }
            if self.static_batch_pass(batch.key) != pass {
                continue;
            }
            if cull && !frustum.intersects_aabb(&batch.bounds) {
                continue;
            }
            let state = self.static_surface_state(batch.key);
            if emissive_only && !state.emission.is_emissive() {
                continue;
            }
            if state.emission.is_emissive() {
                self.emissive_visible = true;
            }
            if !begun {
                self.begin_pass(pass.program());
                begun = true;
            }
            if bound_chunk != Some(batch.chunk) {
                if self.bind_chunk(&self.level_buffers, batch.chunk) {
                    bound_chunk = Some(batch.chunk);
                } else {
                    continue;
                }
            }
            if bound_key != Some(batch.key) {
                unsafe { self.apply_surface_state(state) };
                bound_key = Some(batch.key);
            }
            unsafe {
                self.gl.draw_elements(
                    glow::TRIANGLES,
                    batch.index_range.count,
                    glow::UNSIGNED_SHORT,
                    batch.index_range.start.saturating_mul(2),
                );
            }
            totals.add(batch.vertex_count);
        }
        totals
    }

    /// Which pass one static batch's material belongs to.
    fn static_batch_pass(&self, key: SurfaceKey) -> BatchPass {
        batch_pass_for(
            key.kind,
            key.has_material(),
            self.material_alpha(key.material),
        )
    }

    /// The alpha contract of one material index, or `None` for a key without a
    /// level material.
    fn material_alpha(&self, material: MaterialIndex) -> Option<MaterialAlpha> {
        self.materials.alphas.get(usize::from(material)).copied()
    }

    /// Uploads the reflection and emission-animation uniforms of one draw.
    ///
    /// Both are per-material extras an ordinary surface does not use: the
    /// reflection mode is zero unless the surface authors one *and* this frame
    /// drew its plane or a probe, and the emission scale is one unless the level
    /// animates the material. Kept out of
    /// [`Self::apply_surface_state`]'s already-long body so each term is
    /// readable on its own.
    ///
    /// # Safety
    ///
    /// A scene program must be current.
    unsafe fn upload_scene_extras(&self, uniforms: &ProgramUniforms, state: &SurfaceState) {
        let (reflect_mode, reflect_strength) = self.reflection_uniforms(state);
        unsafe {
            if let Some(ref loc) = uniforms.reflect_mode {
                self.gl.uniform_1_f32(Some(loc), reflect_mode);
            }
            if let Some(ref loc) = uniforms.reflect_strength {
                self.gl.uniform_3_f32(
                    Some(loc),
                    reflect_strength[0],
                    reflect_strength[1],
                    reflect_strength[2],
                );
            }
            if let Some(ref loc) = uniforms.emission_scale {
                let scale = state
                    .emission_animation
                    .map_or(1.0, |animation| animation.factor(self.animation_seconds));
                self.gl.uniform_1_f32(Some(loc), scale);
            }
        }
    }

    /// Whether the current level asked for any reflection at all.
    ///
    /// A level that marks nothing never binds a reflection source, never
    /// allocates a target and reports its materials as reflection-free, so the
    /// frame is pixel-identical to one drawn with reflection support absent.
    const fn reflections_supported(&self) -> bool {
        self.reflections_enabled && !self.reflections.routing.is_empty()
    }

    /// True when a batch is the mirror plane the reflection image is currently
    /// being drawn through, and must therefore be left out of that image.
    ///
    /// Only while `reflection_capture` is set: the ordinary frame draws every
    /// batch exactly as always.
    fn is_capture_mirror(&self, key: SurfaceKey) -> bool {
        self.reflection_capture
            && key.has_material()
            && super::reflections::is_mirror_range(
                &self.reflections.routing,
                self.capture_plane,
                usize::from(key.material),
            )
    }

    /// The reflection uniforms one draw runs with.
    ///
    /// Zero mode for every material that authors none, for the whole frame
    /// while a reflection is being drawn (a mirror must not sample the image
    /// being written), for a planar material whose plane is not the one this
    /// frame reflects, and for a probe material while no probe is baked. The
    /// strength folds the material's specular colour in, so a surface with no
    /// sheen adds nothing even if it asks for a reflection.
    fn reflection_uniforms(&self, state: &SurfaceState) -> (f32, [f32; 3]) {
        let zero = [0.0_f32; 3];
        if self.reflection_capture || !state.reflection.is_active() {
            return (0.0, zero);
        }
        let strength = [
            state.specular[0] * state.reflection.strength,
            state.specular[1] * state.reflection.strength,
            state.specular[2] * state.reflection.strength,
        ];
        match state.reflection.mode {
            ReflectionMode::Planar => match (self.active_plane, state.reflection_plane) {
                (Some(active), Some(plane)) if active.index == usize::from(plane) => {
                    (2.0, strength)
                }
                _ => (0.0, zero),
            },
            ReflectionMode::Probe => {
                if self.reflections.probes().is_empty() {
                    (0.0, zero)
                } else {
                    (1.0, strength)
                }
            }
            ReflectionMode::None => (0.0, zero),
        }
    }

    /// The complete surface state one static surface key draws with.
    ///
    /// Floors, ceilings and walls take their emission, response and alpha from
    /// the material table. Fixture luminous faces carry their emission per
    /// vertex: their key's material slot is the family's sheet, not a level
    /// material, and the glow varies per placement while the batch is shared.
    /// Fixture housings, placeholder boxes and anything else draw plain.
    fn static_surface_state(&self, key: SurfaceKey) -> SurfaceState {
        let texture = self.static_texture(key);
        match emission_routing(key.kind, key.has_material()) {
            EmissionRouting::None => SurfaceState::plain(texture),
            EmissionRouting::Vertex => SurfaceState {
                emission: EmissionState::vertex(),
                ..SurfaceState::plain(texture)
            },
            EmissionRouting::Material => {
                let material = key.material;
                let emission = self
                    .materials
                    .emissions
                    .get(usize::from(material))
                    .copied()
                    .unwrap_or_default();
                let emission = if emission.is_emissive() {
                    let mask = emission
                        .mask
                        .and_then(|index| self.material_textures.get(usize::from(index)).copied());
                    EmissionState::material(emission, mask)
                } else {
                    EmissionState::NONE
                };
                let response = self
                    .materials
                    .responses
                    .get(usize::from(material))
                    .copied()
                    .unwrap_or_default();
                // Low leaves the optional response out: the same material, the
                // same albedo, emission and alpha, one fragment term less.
                let live = response.is_active() && self.quality.draws_surface_response();
                let normal = if live && response.has_normal() {
                    response
                        .normal
                        .and_then(|index| self.material_textures.get(usize::from(index)).copied())
                } else {
                    None
                };
                let reflection = self
                    .materials
                    .reflections
                    .get(usize::from(material))
                    .copied()
                    .unwrap_or(MaterialReflection::NONE);
                // A level's per-surface override wins over the material's own
                // shine and reaches both the sheen and the reflection, because
                // both read the same roughness.
                let roughness = key.roughness(response.roughness);
                SurfaceState {
                    texture,
                    normal,
                    emission,
                    specular: if live { response.specular } else { [0.0; 3] },
                    roughness,
                    normal_strength: response.normal_strength,
                    opacity: self
                        .material_alpha(material)
                        .unwrap_or(MaterialAlpha::OPAQUE)
                        .opacity,
                    alpha_cutoff: self
                        .material_alpha(material)
                        .unwrap_or(MaterialAlpha::OPAQUE)
                        .cutoff,
                    response: live && (response.has_normal() || response.has_sheen()),
                    emission_animation: self
                        .material_animations
                        .get(usize::from(material))
                        .copied()
                        .flatten(),
                    reflection: if self.reflections_supported() {
                        reflection
                    } else {
                        MaterialReflection::NONE
                    },
                    reflection_plane: self
                        .reflections
                        .routing
                        .plane_of(usize::from(material))
                        .and_then(|plane| u16::try_from(plane).ok()),
                }
            }
        }
    }

    /// Applies one surface state to the current program, skipping the work when
    /// the previous draw already set exactly this state.
    ///
    /// The three samplers are re-bound only when the texture behind one actually
    /// changes: most material runs share the "no mask, no normal map" case, so a
    /// level of plain surfaces still issues one bind per material change, not
    /// three. The state cache is per program and is invalidated on every program
    /// switch and after the decal and UI passes, both of which bind their own
    /// textures behind this cache's back.
    ///
    /// # Safety
    ///
    /// A scene program must be current
    /// ([`Self::begin_pass`]); the active texture unit is restored to
    /// [`SCENE_TEXTURE_UNIT`] before returning.
    unsafe fn apply_surface_state(&mut self, state: SurfaceState) {
        if self.surface_state == Some(state) {
            return;
        }
        let Some(pass) = self.current_pass else {
            return;
        };
        let Some(uniforms) = self.uniforms_for(pass) else {
            return;
        };
        let white = self.white_texture;
        let emission = state.emission;
        let mask = emission.mask.unwrap_or(white);
        let normal = state.normal.unwrap_or(white);
        // With no cached state every unit is (re)bound: the renderer cannot
        // assume what the last pass left on them, and an unbound sampler is a
        // driver error even when the shader would not read it.
        let previous = self.surface_state;
        let (bind_albedo, bind_mask, bind_normal) =
            previous.map_or((true, true, true), |previous| {
                (
                    previous.texture != state.texture,
                    previous.emission.mask.unwrap_or(white) != mask,
                    previous.normal.unwrap_or(white) != normal,
                )
            });
        let mut binds = 0usize;
        let bind_if_changed = |gl: &glow::Context, changed: bool, wanted: glow::Texture| -> usize {
            if changed {
                unsafe { gl.bind_texture(glow::TEXTURE_2D, Some(wanted)) };
                return 1;
            }
            0
        };
        unsafe {
            self.gl.active_texture(glow::TEXTURE0);
            binds = binds.saturating_add(bind_if_changed(&self.gl, bind_albedo, state.texture));
            if let Some(ref loc) = uniforms.emission_color {
                self.gl.uniform_3_f32(
                    Some(loc),
                    emission.color[0],
                    emission.color[1],
                    emission.color[2],
                );
            }
            if let Some(ref loc) = uniforms.emission_mask_enabled {
                self.gl
                    .uniform_1_f32(Some(loc), if emission.mask.is_some() { 1.0 } else { 0.0 });
            }
            if let Some(ref loc) = uniforms.emission_vertex {
                self.gl
                    .uniform_1_f32(Some(loc), if emission.vertex { 1.0 } else { 0.0 });
            }
            self.gl.active_texture(glow::TEXTURE1);
            binds = binds.saturating_add(bind_if_changed(&self.gl, bind_mask, mask));
            self.gl.active_texture(glow::TEXTURE4);
            binds = binds.saturating_add(bind_if_changed(&self.gl, bind_normal, normal));
            if let Some(ref loc) = uniforms.normal_enabled {
                self.gl
                    .uniform_1_f32(Some(loc), if state.normal.is_some() { 1.0 } else { 0.0 });
            }
            if let Some(ref loc) = uniforms.normal_strength {
                self.gl.uniform_1_f32(Some(loc), state.normal_strength);
            }
            if let Some(ref loc) = uniforms.specular {
                self.gl.uniform_3_f32(
                    Some(loc),
                    state.specular[0],
                    state.specular[1],
                    state.specular[2],
                );
            }
            if let Some(ref loc) = uniforms.roughness {
                self.gl.uniform_1_f32(Some(loc), state.roughness);
            }
            if let Some(ref loc) = uniforms.opacity {
                self.gl.uniform_1_f32(Some(loc), state.opacity);
            }
            if let Some(ref loc) = uniforms.alpha_cutoff {
                self.gl.uniform_1_f32(Some(loc), state.alpha_cutoff);
            }
            if let Some(ref loc) = uniforms.response_enabled {
                self.gl
                    .uniform_1_f32(Some(loc), if state.response { 1.0 } else { 0.0 });
            }
            self.upload_scene_extras(uniforms, &state);
            self.gl.active_texture(glow::TEXTURE0);
        }
        self.render_stats.texture_binds = self.render_stats.texture_binds.saturating_add(binds);
        self.render_stats.material_changes = self.render_stats.material_changes.saturating_add(1);
        self.surface_state = Some(state);
    }

    /// The texture one static surface key binds: its resolved material sheet,
    /// the fixture sheet its light batch carries, the decal sheet, or the
    /// untextured white sheet.
    fn static_texture(&self, key: SurfaceKey) -> glow::Texture {
        match key.kind {
            SurfaceKind::Floor | SurfaceKind::Ceiling | SurfaceKind::Wall => {
                if key.has_material() {
                    let slot = self
                        .materials
                        .texture_slots
                        .get(usize::from(key.material))
                        .copied()
                        .unwrap_or(0);
                    self.material_textures
                        .get(usize::from(slot))
                        .copied()
                        .unwrap_or(self.white_texture)
                } else {
                    // No resolved material (an empty authored id): the unshaded
                    // sheet is the honest fallback.
                    self.white_texture
                }
            }
            SurfaceKind::Light => {
                // A light batch's key carries its fixture family's sheet slot;
                // the housing and any unknown family draw the white sheet.
                if key.has_material() {
                    self.fixture_sheets
                        .get(usize::from(key.material))
                        .copied()
                        .unwrap_or(self.white_texture)
                } else {
                    self.white_texture
                }
            }
            SurfaceKind::PropFallback => self.white_texture,
            SurfaceKind::Decal => self.decal.texture,
        }
    }

    /// Submits the batched real prop geometry: one buffer and one draw call per
    /// (model, primitive, spatial cell), with one texture and emission bind per
    /// change.
    fn draw_prop_batches(
        &mut self,
        frustum: &Frustum,
        cull: bool,
        emissive_only: bool,
    ) -> DrawTotals {
        let mut totals = DrawTotals::default();
        let mut begun = false;
        let mut bound_texture: Option<glow::Texture> = None;
        let mut bound_chunk: Option<usize> = None;
        for index in 0..self.prop_draws.len() {
            let Some(draw) = self.prop_draws.get(index).copied() else {
                continue;
            };
            if draw.index_count <= 0 {
                continue;
            }
            if cull && !frustum.intersects_aabb(&draw.bounds) {
                continue;
            }
            if emissive_only && !draw.emission.is_emissive() {
                continue;
            }
            if draw.emission.is_emissive() {
                self.emissive_visible = true;
            }
            if !begun {
                self.begin_pass(ScenePass::World);
                begun = true;
            }
            if bound_chunk != Some(draw.chunk) {
                if self.bind_chunk(&self.prop_buffers, draw.chunk) {
                    bound_chunk = Some(draw.chunk);
                } else {
                    continue;
                }
            }
            if bound_texture != Some(draw.texture) {
                unsafe { self.gl.bind_texture(glow::TEXTURE_2D, Some(draw.texture)) };
                bound_texture = Some(draw.texture);
            }
            let state = SurfaceState {
                emission: draw.emission,
                ..SurfaceState::plain(draw.texture)
            };
            unsafe { self.apply_surface_state(state) };
            unsafe {
                self.gl.draw_elements(
                    glow::TRIANGLES,
                    draw.index_count,
                    glow::UNSIGNED_SHORT,
                    draw.index_start.saturating_mul(2),
                );
            }
            totals.add(draw.vertex_count);
        }
        totals
    }

    /// Submits every translucent batch, back to front.
    ///
    /// The list is collected from the static batches that survived culling and
    /// sorted by the squared distance from the camera to each batch's centre.
    /// Depth *testing* stays on — a pane of glass is still hidden by the wall it
    /// sits in — but depth *writing* is off, so two overlapping translucent
    /// surfaces blend with each other instead of one erasing the other, and the
    /// pass runs after the opaque world so it blends against real geometry.
    /// Sorting is per spatial batch, which is the granularity the renderer
    /// already partitions the world at; the scratch list keeps its capacity
    /// between frames, so the sort never allocates in the frame loop.
    fn draw_translucent_pass(
        &mut self,
        frustum: &Frustum,
        cull: bool,
        _mvp: &glam::Mat4,
        emissive_only: bool,
    ) -> DrawTotals {
        let mut totals = DrawTotals::default();
        {
            let batches = &self.static_batches;
            let alphas = &self.materials.alphas;
            let pass_of = |key: SurfaceKey| {
                let alpha = alphas.get(usize::from(key.material)).copied();
                batch_pass_for(key.kind, key.has_material(), alpha)
            };
            let visible = |bounds: &crate::spatial::Aabb| !cull || frustum.intersects_aabb(bounds);
            let camera = self.camera_pos;
            // `collect_translucent_draws` sorts in place and reuses the vector's
            // capacity, so the frame loop allocates nothing here.
            let scratch = &mut self.translucent_scratch;
            collect_translucent_draws(batches, pass_of, camera, visible, scratch);
        }
        if self.translucent_scratch.is_empty() {
            return totals;
        }

        self.begin_pass(ScenePass::World);
        unsafe {
            self.gl.enable(glow::BLEND);
            self.gl
                .blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
            self.gl.depth_mask(false);
        }
        let mut bound_chunk: Option<usize> = None;
        let mut bound_key: Option<SurfaceKey> = None;
        for slot in 0..self.translucent_scratch.len() {
            let Some(item) = self.translucent_scratch.get(slot).copied() else {
                continue;
            };
            let TranslucentSource::Static(index) = item.source;
            let Some(batch) = self.static_batches.get(index).copied() else {
                continue;
            };
            if self.is_capture_mirror(batch.key) {
                continue;
            }
            if bound_chunk != Some(batch.chunk) {
                if self.bind_chunk(&self.level_buffers, batch.chunk) {
                    bound_chunk = Some(batch.chunk);
                } else {
                    continue;
                }
            }
            let state = self.static_surface_state(batch.key);
            if emissive_only && !state.emission.is_emissive() {
                continue;
            }
            if bound_key != Some(batch.key) {
                unsafe { self.apply_surface_state(state) };
                bound_key = Some(batch.key);
            }
            unsafe {
                self.gl.draw_elements(
                    glow::TRIANGLES,
                    batch.index_range.count,
                    glow::UNSIGNED_SHORT,
                    batch.index_range.start.saturating_mul(2),
                );
            }
            totals.add(batch.vertex_count);
        }
        unsafe {
            self.gl.depth_mask(true);
            self.gl.disable(glow::BLEND);
        }
        totals
    }

    /// Submits the decal pass: local surface markings drawn after the opaque
    /// world and the props.
    ///
    /// Depth testing stays on and depth writes stay on, so a decal is still
    /// hidden by anything in front of it; the pass adds a fixed polygon offset
    /// that pulls each decal two depth steps towards the camera, which is what
    /// makes it win the coincident-depth test against the surface it lies on.
    /// The world program and offset state are restored before returning.
    fn draw_decal_batches(
        &mut self,
        frustum: &Frustum,
        cull: bool,
        mvp: &glam::Mat4,
    ) -> DrawTotals {
        let mut totals = DrawTotals::default();
        let mut decal_active = false;
        let mut decal_chunk: Option<usize> = None;
        let mut bound_decal_texture: Option<glow::Texture> = None;
        for batch in &self.static_batches {
            if batch.key.kind != SurfaceKind::Decal || batch.index_range.count <= 0 {
                continue;
            }
            if cull && !frustum.intersects_aabb(&batch.bounds) {
                continue;
            }
            if !decal_active {
                unsafe {
                    self.gl.use_program(Some(self.decal.program));
                    if let Some(ref loc) = self.decal.u_mvp_loc {
                        self.gl
                            .uniform_matrix_4_f32_slice(Some(loc), false, &mvp.to_cols_array());
                    }
                    if let Some(ref loc) = self.decal.u_texture_loc {
                        self.gl.uniform_1_i32(Some(loc), 0);
                    }
                    if let Some(ref loc) = self.decal.u_alpha_cutoff_loc {
                        self.gl.uniform_1_f32(Some(loc), DECAL_ALPHA_CUTOFF);
                    }
                    let (factor, units) = DECAL_POLYGON_OFFSET;
                    self.gl.enable(glow::POLYGON_OFFSET_FILL);
                    self.gl.polygon_offset(factor, units);
                    self.gl
                        .bind_texture(glow::TEXTURE_2D, Some(self.decal.texture));
                }
                decal_active = true;
                // Both programs share attribute locations, but rebind from
                // scratch so the pass cannot depend on what the world loop left
                // bound.
                decal_chunk = None;
            }
            if decal_chunk != Some(batch.chunk) {
                if self.bind_chunk(&self.level_buffers, batch.chunk) {
                    decal_chunk = Some(batch.chunk);
                } else {
                    continue;
                }
            }
            let sheet = self.decal_sheet_texture(batch.key.material);
            if bound_decal_texture != Some(sheet) {
                unsafe { self.gl.bind_texture(glow::TEXTURE_2D, Some(sheet)) };
                self.render_stats.texture_binds = self.render_stats.texture_binds.saturating_add(1);
                bound_decal_texture = Some(sheet);
            }
            unsafe {
                self.gl.draw_elements(
                    glow::TRIANGLES,
                    batch.index_range.count,
                    glow::UNSIGNED_SHORT,
                    batch.index_range.start.saturating_mul(2),
                );
            }
            totals.add(batch.vertex_count);
        }
        if decal_active {
            // Restore the exact scene state: no polygon offset, and the world
            // program. The pass changed the bound program behind the renderer's
            // back, so the tracked state is invalidated and the next pass
            // re-binds and re-uploads.
            unsafe {
                self.gl.polygon_offset(0.0, 0.0);
                self.gl.disable(glow::POLYGON_OFFSET_FILL);
                if let Some(program) = self.program_for(ScenePass::World) {
                    self.gl.use_program(Some(program));
                }
            }
            self.current_pass = None;
            self.surface_state = None;
            self.frame_state_valid = false;
        }
        totals
    }

    /// The texture one decal material index binds.
    ///
    /// Indices below [`DECAL_EXTERNAL_BASE`] address the generated atlas;
    /// higher ones index the level's external sheets, falling back to the
    /// atlas when a sheet is missing.
    fn decal_sheet_texture(&self, material: MaterialIndex) -> glow::Texture {
        u32::from(material)
            .checked_sub(DECAL_EXTERNAL_BASE)
            .map_or(self.decal.texture, |offset| {
                self.decal
                    .external
                    .get(usize::try_from(offset).unwrap_or(usize::MAX))
                    .copied()
                    .unwrap_or(self.decal.texture)
            })
    }

    /// Selects the GPU vertex layout. Packed is the shipping default; the
    /// debug benchmark selects `Exact` to measure the packing win on the same
    /// build with everything else held constant.
    pub const fn set_vertex_layout(&mut self, layout: VertexLayout) {
        self.vertex_layout = layout;
    }

    /// Enables or disables indexed submission. Only the debug benchmark turns
    /// this off, to measure what indexing is worth on real hardware.
    pub const fn set_indexing(&mut self, enabled: bool) {
        self.indexing_enabled = enabled;
    }

    /// Enables or disables frustum culling.
    ///
    /// Culling is always on in normal play; the debug benchmark harness turns it
    /// off so the same build can measure what it is worth on real hardware.
    pub const fn set_culling(&mut self, enabled: bool) {
        self.culling_enabled = enabled;
    }

    /// Sets the world program's per-draw light multiplier.
    ///
    /// Static geometry always draws with `[1, 1, 1]`: its light is already in the
    /// lightmap atlas or in its vertex colour. The dynamic-object path sets this
    /// to the baked light sampled at the object's current position, which is how
    /// a moving object stays coherently lit without rebuilding its vertex
    /// buffer. The value is recorded here and uploaded by the next
    /// [`Self::begin_pass`], because uniform state belongs to the program.
    pub fn set_light_scale(&mut self, scale: [f32; 3]) {
        self.light_scale = scale.map(|value| if value.is_finite() { value } else { 1.0 });
    }

    /// Requests lightmaps for the *next* level build.
    ///
    /// This is the settings switch (`lightmaps: true|false`,
    /// `LIMINAL_NO_LIGHTMAPS=1`), applied by `set_level`/`rebuild_level_geometry`.
    /// It cannot re-interpret an already loaded lightmapped mesh: a lightmapped
    /// vertex's colour omits the baked light by design, so turning lightmaps off
    /// takes effect as an explicit rebuild with [`LightmapMode::Off`], which is
    /// the exact historical vertex-lit level.
    pub const fn set_lightmaps_requested(&mut self, requested: bool) {
        self.lightmaps_requested = requested;
        self.set_lightmaps_enabled(requested);
    }

    /// True when the next level build will bake lightmaps.
    #[must_use]
    pub const fn lightmaps_requested(&self) -> bool {
        self.lightmaps_requested
    }

    /// Turns lightmap sampling on or off without dropping the resident atlas.
    ///
    /// Enabling has no effect when no valid atlas is resident: a level whose bake
    /// failed must not render black surfaces, so the switch can only ever
    /// *restore* the vertex-lit path. Turning sampling off on a lightmapped level
    /// keeps that level's tint-only vertex colours, which is the benchmark A/B
    /// ("what does the atlas contribute"), not the vertex-lit fallback; use
    /// [`Self::set_lightmaps_requested`] plus a rebuild for that.
    pub const fn set_lightmaps_enabled(&mut self, enabled: bool) {
        self.lightmaps_enabled = enabled && self.lightmaps_resident;
        self.upload_lightmap_switch();
    }

    /// Records whether a valid baked lightmap set is resident and follows it
    /// with the sampling switch: no atlas means the vertex-lit path.
    pub const fn set_lightmaps_resident(&mut self, resident: bool) {
        self.lightmaps_resident = resident;
        self.lightmaps_enabled = resident;
        self.upload_lightmap_switch();
    }

    /// Marks the per-program lightmap switch stale so the next pass re-uploads
    /// it. The value itself lives in [`Self::lightmaps_enabled`].
    const fn upload_lightmap_switch(&mut self) {
        self.frame_state_valid = false;
    }

    /// True when a real baked lightmap atlas is resident and being sampled.
    #[must_use]
    pub const fn lightmaps_enabled(&self) -> bool {
        self.lightmaps_enabled
    }

    /// True when a valid baked lightmap atlas has been uploaded.
    #[must_use]
    pub const fn lightmaps_resident(&self) -> bool {
        self.lightmaps_resident
    }

    /// Replaces one lightmap atlas page on its texture unit.
    ///
    /// `slot` indexes the two units the world shader samples. `None` restores the
    /// white sheet, so an unused unit always has a defined sample.
    pub fn set_lightmap_page(&mut self, slot: usize, texture: Option<glow::Texture>) {
        let page = texture.unwrap_or(self.white_texture);
        if let Some(current) = self.lightmap_pages.get_mut(slot) {
            *current = page;
        }
    }

    /// Binds the current atlas pages on the two lightmap units, restoring
    /// [`SCENE_TEXTURE_UNIT`] as the active unit.
    ///
    /// Both units must hold a *complete* 2D texture for every world draw, atlas
    /// or not: the world fragment stage declares both samplers, and a unit whose
    /// binding is the default (empty) texture object is not a defined sample —
    /// the Apple GL driver reports it as unloadable and substitutes a zero
    /// texture. [`Self::draw_scene_body`] and [`Self::draw_emissive_body`] call
    /// this before every submission, not just once per frame, because the
    /// level-load reflection-probe bake draws the world program before
    /// `render_scene` has ever run.
    fn bind_lightmap_units(&mut self) {
        unsafe {
            self.gl
                .active_texture(super::view::texture_unit(LIGHTMAP_TEXTURE_UNIT));
            self.gl
                .bind_texture(glow::TEXTURE_2D, Some(self.lightmap_pages[0]));
            self.gl
                .active_texture(super::view::texture_unit(LIGHTMAP_TEXTURE_UNIT_1));
            self.gl
                .bind_texture(glow::TEXTURE_2D, Some(self.lightmap_pages[1]));
            self.gl
                .active_texture(super::view::texture_unit(SCENE_TEXTURE_UNIT));
        }
        self.render_stats.texture_binds = self.render_stats.texture_binds.saturating_add(2);
    }

    /// Number of atlas pages currently resident on the GPU.
    #[must_use]
    pub const fn lightmap_page_count(&self) -> usize {
        self.lightmap_textures.len()
    }

    /// Uploads a whole baked atlas: one RGB8 texture per page, clamped and
    /// filter-configured, then marked resident for [`Self::bind_lightmap_units`]
    /// to bind on units 2 and 3.
    ///
    /// On any failure every texture this call created is deleted and the units
    /// are restored to the white sheet, so the caller can fall back to vertex
    /// lighting without a half-bound atlas. Never called per frame.
    fn upload_level_lightmaps(&mut self, lightmaps: &LevelLightmaps) -> Result<(), String> {
        if lightmaps.pages.len() > LIGHTMAP_PAGE_SLOTS {
            return Err(format!(
                "{} atlas pages exceed the {LIGHTMAP_PAGE_SLOTS} bound units",
                lightmaps.pages.len()
            ));
        }
        self.clear_lightmap_pages();
        let mut uploaded: Vec<glow::Texture> = Vec::with_capacity(lightmaps.pages.len());
        for (slot, page) in lightmaps.pages.iter().enumerate() {
            match unsafe { self.upload_lightmap_page(page) } {
                Ok(texture) => {
                    self.set_lightmap_page(slot, Some(texture));
                    uploaded.push(texture);
                }
                Err(error) => {
                    for texture in uploaded {
                        unsafe { self.gl.delete_texture(texture) };
                    }
                    for slot in 0..LIGHTMAP_PAGE_SLOTS {
                        self.set_lightmap_page(slot, None);
                    }
                    self.set_lightmaps_resident(false);
                    return Err(error);
                }
            }
        }
        self.lightmap_textures = uploaded;
        self.set_lightmaps_resident(!self.lightmap_textures.is_empty());
        Ok(())
    }

    /// Deletes every resident atlas page and restores the white sheet.
    fn clear_lightmap_pages(&mut self) {
        for texture in self.lightmap_textures.drain(..) {
            unsafe { self.gl.delete_texture(texture) };
        }
        for slot in 0..LIGHTMAP_PAGE_SLOTS {
            self.set_lightmap_page(slot, None);
        }
        self.set_lightmaps_resident(false);
    }

    /// Uploads one atlas page as an RGB8 texture with clamped, mipmap-free
    /// sampling.
    ///
    /// Lightmap UVs are authored to touch a chart's dilated gutter, never a
    /// neighbouring chart, so `CLAMP_TO_EDGE` plus linear (or nearest) filtering
    /// with no mip chain is the whole sampling policy: a mip level would blend
    /// across chart boundaries far too early.
    unsafe fn upload_lightmap_page(&self, page: &LightmapPage) -> Result<glow::Texture, String> {
        unsafe {
            let texture = self.gl.create_texture()?;
            // Upload on the scene unit rather than whichever unit happens to be
            // active: the lightmap units hold the previous atlas until the
            // caller swaps the pages in, and this must not unbind them.
            self.gl.active_texture(glow::TEXTURE0);
            self.gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            self.gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGB8.cast_signed(),
                i32::try_from(page.width).unwrap_or(i32::MAX),
                i32::try_from(page.height).unwrap_or(i32::MAX),
                0,
                glow::RGB,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(&page.rgb)),
            );
            self.gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_S,
                glow::CLAMP_TO_EDGE.cast_signed(),
            );
            self.gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_T,
                glow::CLAMP_TO_EDGE.cast_signed(),
            );
            set_lightmap_filter(&self.gl, self.linear_filtering);
            self.gl.bind_texture(glow::TEXTURE_2D, None);
            Ok(texture)
        }
    }

    /// Spatial grid the current level was partitioned with, for developer logs.
    pub const fn spatial_grid(&self) -> crate::spatial::CellGrid {
        self.spatial_grid
    }

    /// Number of cullable static ranges the current level is split into.
    pub const fn static_batch_count(&self) -> usize {
        self.static_batches.len()
    }

    /// Static batch count per [`SurfaceKind`], in [`SurfaceKind::ALL`] order.
    ///
    /// Printed once per level load so a hardware run makes it obvious when a
    /// level has been shredded into more draw calls than the GPU can afford.
    pub fn static_batch_breakdown(&self) -> [usize; SurfaceKind::ALL.len()] {
        let mut counts = [0usize; SurfaceKind::ALL.len()];
        for batch in &self.static_batches {
            if let Some(count) = counts.get_mut(batch.key.kind as usize) {
                *count = count.saturating_add(1);
            }
        }
        counts
    }

    /// Static batch count per [`SurfaceKind`], in [`SurfaceKind::ALL`] order.
    ///
    /// The developer log reports families; how many distinct materials a level
    /// uses is a content choice, not a separate kind of surface.
    pub fn static_batch_family_breakdown(&self) -> [usize; SurfaceKind::ALL.len()] {
        let mut counts = [0usize; SurfaceKind::ALL.len()];
        for batch in &self.static_batches {
            if let Some(count) = counts.get_mut(batch.key.kind as usize) {
                *count = count.saturating_add(1);
            }
        }
        counts
    }

    /// Counters for the most recently submitted scene (see [`RenderStats`]).
    pub const fn render_stats(&self) -> RenderStats {
        self.render_stats
    }

    /// Points the scene attributes at the selected vertex layout.
    ///
    /// In the packed layout `normalized = true` lets the fixed-function pipeline
    /// expand `GL_UNSIGNED_BYTE` colour to `[0, 1]` floats, so the shader is the
    /// same `vec4` in both layouts. Both are core OpenGL ES 2.0.
    ///
    /// The lightmap attributes keep their exact quantised type in *both*
    /// layouts: `a_lightmap_uv` is always two normalized unsigned shorts and
    /// `a_lightmap_page` one plain unsigned byte, so the packed and exact builds
    /// sample the atlas identically and the debug benchmark's comparison stays
    /// honest.
    fn set_vertex_attributes(&self) {
        let stride = self.vertex_layout.stride();
        let locations = [
            self.a_pos_loc,
            self.a_color_loc,
            self.a_uv_loc,
            self.a_lightmap_uv_loc,
            self.a_lightmap_page_loc,
            self.a_normal_loc,
            self.a_tangent_loc,
            self.a_handedness_loc,
        ];
        for (slot, pointer) in scene_attribute_table(self.vertex_layout)
            .into_iter()
            .enumerate()
        {
            let Some(pointer) = pointer else {
                continue;
            };
            let Some(&location) = locations.get(slot) else {
                continue;
            };
            unsafe {
                self.gl.enable_vertex_attrib_array(location);
                self.gl.vertex_attrib_pointer_f32(
                    location,
                    pointer.components,
                    pointer.gl_type,
                    pointer.normalized,
                    stride,
                    pointer.offset,
                );
            }
        }
    }

    /// Binds one vertex/index buffer pair and points the vertex attributes at it.
    ///
    /// The attribute pointers are captured against whichever vertex buffer is
    /// bound when they are set, so every chunk needs them re-issued once per
    /// frame. Returns `false` for a chunk that does not exist, so a caller can
    /// skip rather than draw from a stale binding.
    fn bind_chunk(&self, buffers: &[(glow::Buffer, glow::Buffer)], chunk: usize) -> bool {
        let Some(&(vbo, ibo)) = buffers.get(chunk) else {
            return false;
        };
        unsafe {
            self.gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
            self.set_vertex_attributes();
            self.gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(ibo));
        }
        true
    }

    /// Drains the GL pipeline. Only used by the debug benchmark harness to
    /// separate renderer completion time from the presentation wait; it is a
    /// hard sync and must never be called in the normal frame loop.
    pub fn finish(&self) {
        unsafe { self.gl.finish() };
    }

    /// Renders a 2D UI overlay on top of the scene using an orthographic projection and the font atlas.
    ///
    /// UI geometry is authored in the 480x272 reference space; the projection
    /// below stays in that space while the viewport is scaled/centred to the
    /// drawable, so the HUD keeps its proportions at any resolution.
    ///
    /// Takes `&mut self` for the packed-vertex scratch buffer: UI geometry is
    /// produced once per frame as exact floats and converted here, so a single
    /// packed vertex layout (and one shader) serves both the scene and the HUD.
    pub fn render_ui(&mut self, ui_vertices: &[Vertex]) {
        let drawable = self.drawable_size;
        if ui_vertices.is_empty() || drawable.is_empty() {
            return;
        }

        let viewport = drawable.ui_viewport();

        unsafe {
            self.gl.disable(glow::DEPTH_TEST);
            self.gl.enable(glow::BLEND);
            self.gl
                .blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);

            if let Some(program) = self.program_for(ScenePass::World) {
                self.gl.use_program(Some(program));
            }
            // The HUD draws with the world program but with none of a surface's
            // extra terms: no lightmap (its vertices carry `LIGHTMAP_NONE`), no
            // emission, no sheen and no alpha. The tracked state is invalidated
            // *and then re-applied* rather than merely claimed, because the
            // uniforms still hold whatever the last world material left in them
            // — a translucent surface's `u_opacity`, an emissive fixture's
            // `u_emission_vertex`, a panel's `u_response_enabled` — and drawing
            // the HUD under those would blend the UI with the scene's material
            // state.
            self.current_pass = Some(ScenePass::World);
            self.surface_state = None;
            self.frame_state_valid = false;
            self.uploaded_camera_pos = self.camera_pos;

            self.gl
                .viewport(viewport.x, viewport.y, viewport.width, viewport.height);

            let ortho = glam::Mat4::orthographic_rh(
                0.0,
                dimension_f32(UI_REFERENCE_WIDTH),
                dimension_f32(UI_REFERENCE_HEIGHT),
                0.0,
                -1.0,
                1.0,
            );

            let ortho_columns = ortho.to_cols_array();
            let identity_columns = glam::Mat4::IDENTITY.to_cols_array();
            if let Some(uniforms) = self.uniforms_for(ScenePass::World) {
                if let Some(ref loc) = uniforms.mvp {
                    self.gl
                        .uniform_matrix_4_f32_slice(Some(loc), false, &ortho_columns);
                }
                if let Some(ref loc) = uniforms.model {
                    self.gl
                        .uniform_matrix_4_f32_slice(Some(loc), false, &identity_columns);
                }
                if let Some(ref loc) = uniforms.light_scale {
                    self.gl.uniform_3_f32(Some(loc), 1.0, 1.0, 1.0);
                }
                if let Some(ref loc) = uniforms.lightmap_enabled {
                    self.gl.uniform_1_f32(Some(loc), 0.0);
                }
                // The HUD is not in the world: no fog, no emissive-only pass and
                // no reflection image. These are frame uniforms rather than
                // material ones, so they are cleared here explicitly instead of
                // through `apply_surface_state`.
                if let Some(ref loc) = uniforms.fog_density {
                    self.gl.uniform_1_f32(Some(loc), FogState::NONE.density);
                }
                if let Some(ref loc) = uniforms.emission_only {
                    self.gl.uniform_1_f32(Some(loc), 0.0);
                }
                if let Some(ref loc) = uniforms.reflect_mode {
                    self.gl.uniform_1_f32(Some(loc), 0.0);
                }
            }
            self.light_scale = [1.0; 3];
            self.frame_state_valid = true;
            // The HUD closes `u_lightmap_enabled`, but the world program still
            // declares both atlas samplers: bind them complete so this draw can
            // never resolve an empty unit either.
            self.bind_lightmap_units();
            // All of the surface uniforms — emission, sheen, roughness, opacity
            // and the response gate — are set from the same plain state the
            // world's own batcher uses, so the UI can never inherit a material.
            self.apply_surface_state(SurfaceState::plain(self.font_texture));
            self.gl.active_texture(glow::TEXTURE0);

            self.gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.ui_vbo));
            // The HUD is a few thousand vertices rebuilt every frame anyway, so
            // converting it here keeps one vertex layout and one shader for both
            // the scene and the UI.
            let byte_slice = match self.vertex_layout {
                VertexLayout::Packed => {
                    self.ui_scratch.clear();
                    self.ui_scratch
                        .extend(ui_vertices.iter().map(PackedVertex::from));
                    self.ui_packed_len = self.ui_scratch.len();
                    std::slice::from_raw_parts(
                        self.ui_scratch.as_ptr().cast::<u8>(),
                        std::mem::size_of_val(self.ui_scratch.as_slice()),
                    )
                }
                VertexLayout::Exact => {
                    self.ui_packed_len = ui_vertices.len();
                    std::slice::from_raw_parts(
                        ui_vertices.as_ptr().cast::<u8>(),
                        std::mem::size_of_val(ui_vertices),
                    )
                }
            };
            self.gl
                .buffer_data_u8_slice(glow::ARRAY_BUFFER, byte_slice, glow::DYNAMIC_DRAW);

            self.set_vertex_attributes();

            self.gl.draw_arrays(
                glow::TRIANGLES,
                0,
                i32::try_from(self.ui_packed_len).unwrap_or(i32::MAX),
            );

            self.gl.disable_vertex_attrib_array(self.a_pos_loc);
            self.gl.disable_vertex_attrib_array(self.a_color_loc);
            self.gl.disable_vertex_attrib_array(self.a_uv_loc);
            self.gl.bind_texture(glow::TEXTURE_2D, None);
            self.gl.bind_buffer(glow::ARRAY_BUFFER, None);
            self.gl.use_program(None);

            self.gl.disable(glow::BLEND);
            self.gl.enable(glow::DEPTH_TEST);
        }
    }
}

/// GPU state for one distinct dynamic mesh: a small vertex/index pair plus its
/// per-primitive draw state.
///
/// Uploaded once in **model space** per distinct dynamic model, exactly like a
/// prop batch except that nothing is pre-transformed. Every object that uses
/// the mesh draws it with its own composed `u_mvp`, so nothing here is touched
/// when an object moves.
struct DynamicMeshGpu {
    vbo: glow::Buffer,
    ibo: glow::Buffer,
    submeshes: Vec<DynamicSubmeshGpu>,
}

/// One primitive's GPU draw state: the texture it binds and the emission it
/// adds, resolved once at upload.
struct DynamicSubmeshGpu {
    texture: glow::Texture,
    /// The model material's own emission, routed exactly like a static prop
    /// primitive's. A per-object override replaces it at draw time.
    emission: EmissionState,
    first_index: i32,
    index_count: i32,
    /// Distinct vertices this primitive reads, for the frame counters.
    vertex_count: i32,
}

/// The dynamic-object half of the renderer: spawn/update, one upload per
/// distinct model, and one draw per object.
///
/// This is deliberately a separate `impl` block so the static pipeline above
/// stays untouched: nothing here runs at level build time, and nothing in the
/// static path knows this block exists. A dynamic object is transformed by
/// `u_mvp` alone (the shared world program already multiplies `a_pos` by it),
/// lit by the `u_light_scale` probe, and shaded through the same texture,
/// emission and material routing the static props use.
impl Renderer {
    /// Number of draw calls the current dynamic scene needs (one per object per
    /// primitive), exposed for the developer log and tests.
    #[must_use]
    pub fn dynamic_draw_count(&self) -> usize {
        self.dynamic.draw_count()
    }

    /// Number of live dynamic objects.
    #[must_use]
    pub const fn dynamic_object_count(&self) -> usize {
        self.dynamic.len()
    }

    /// The current dynamic scene, for the developer log and tests.
    #[must_use]
    pub const fn dynamic_scene(&self) -> &DynamicScene {
        &self.dynamic
    }

    /// Replaces the dynamic scene with the washer-drum demonstration for `level`.
    ///
    /// Called once per level load (never per frame). The scene is cleared, the
    /// demonstration spawns a turning drum in front of every placed washing
    /// machine, and the distinct meshes are uploaded once. Returns how many
    /// objects were spawned.
    pub fn set_dynamic_demo(&mut self, level: &crate::level::LevelDef) -> usize {
        self.dynamic.clear_all();
        let spawned =
            self.dynamic
                .spawn_washer_drum_demo(level, &self.prop_catalog, &mut self.prop_assets);
        self.sync_dynamic_meshes();
        spawned
    }

    /// Advances the dynamic objects by `delta_seconds`: spins, transform
    /// updates and (only for objects that moved appreciably) light probes.
    ///
    /// Called once per frame. It never uploads geometry, never rebuilds the
    /// level and never touches the lightmap baker; with an empty scene it is a
    /// length check.
    pub fn update_dynamic(&mut self, delta_seconds: f32) -> DynamicUpdate {
        // The emission animations share the simulation's clock. A non-finite or
        // negative delta is ignored rather than allowed to jump every animated
        // surface at once.
        if delta_seconds.is_finite() && delta_seconds > 0.0 {
            self.animation_seconds += delta_seconds;
        }
        self.dynamic
            .update(delta_seconds, self.dynamic_lighting.as_ref())
    }

    /// Uploads one GPU mesh per distinct dynamic model.
    ///
    /// Runs when the dynamic scene's mesh list changes (level load, first
    /// spawn), never per frame. Textures go through the shared prop cache, so a
    /// model that is also placed statically uploads once for both paths.
    // A failed upload is a chatty one-line diagnostic and this renderer has no
    // logger (the game prints its own diagnostics directly), so the stderr
    // report is the intended behaviour and stays scoped to this method.
    #[allow(clippy::print_stderr)]
    fn sync_dynamic_meshes(&mut self) {
        // Drop the previous scene's GPU state first: a level switch may reuse a
        // model path with different geometry.
        for (_, mesh) in self.dynamic_meshes.drain() {
            unsafe {
                self.gl.delete_buffer(mesh.vbo);
                self.gl.delete_buffer(mesh.ibo);
            }
        }
        let meshes: Vec<std::rc::Rc<DynamicMesh>> = self.dynamic.meshes().to_vec();
        for mesh in &meshes {
            if !self.prop_textures.contains_key(&mesh.model_path)
                && self
                    .upload_model_textures(&mesh.model_path, &mesh.textures)
                    .is_none()
            {
                continue;
            }
            match self.upload_dynamic_mesh(mesh) {
                Ok(gpu) => {
                    self.dynamic_meshes.insert(mesh.model_path.clone(), gpu);
                }
                Err(error) => {
                    crate::logging::warn_once(
                        format!("dynamic-upload:{}", mesh.model_path),
                        format!(
                            "[dynamic] cannot upload mesh {}: {error}; skipping that object",
                            mesh.model_path
                        ),
                    );
                }
            }
        }
        self.dynamic_revision = self.dynamic.revision();
    }

    /// Uploads one model-space dynamic mesh into its own small buffer pair.
    ///
    /// # Errors
    ///
    /// Returns a message when a GL buffer cannot be created.
    fn upload_dynamic_mesh(&self, mesh: &DynamicMesh) -> Result<DynamicMeshGpu, String> {
        let textures = self
            .prop_textures
            .get(&mesh.model_path)
            .cloned()
            .unwrap_or_default();
        let mut submeshes: Vec<DynamicSubmeshGpu> = Vec::with_capacity(mesh.submeshes.len());
        for submesh in &mesh.submeshes {
            let texture = submesh
                .texture
                .and_then(|slot| textures.get(usize::from(slot)).copied())
                .unwrap_or(self.white_texture);
            let mask = submesh
                .emission
                .mask
                .and_then(|slot| textures.get(usize::from(slot)).copied());
            let emission = if submesh.emission.is_emissive() {
                EmissionState::material(submesh.emission, mask)
            } else {
                EmissionState::NONE
            };
            let start = usize::try_from(submesh.first_index).unwrap_or(0);
            let count = usize::try_from(submesh.index_count).unwrap_or(0);
            let end = start.saturating_add(count);
            let Some(indices) = mesh.indices.get(start..end) else {
                continue;
            };
            // Distinct vertices this primitive actually reads, for the frame
            // counters; the static path gets the same number out of its packer.
            let mut used = vec![false; mesh.vertices.len()];
            for index in indices {
                if let Some(flag) = used.get_mut(usize::from(*index)) {
                    *flag = true;
                }
            }
            let vertex_count = used.iter().filter(|flag| **flag).count();
            submeshes.push(DynamicSubmeshGpu {
                texture,
                emission,
                first_index: i32::try_from(start).unwrap_or(i32::MAX),
                index_count: i32::try_from(count).unwrap_or(i32::MAX),
                vertex_count: i32::try_from(vertex_count).unwrap_or(i32::MAX),
            });
        }
        let (vbo, ibo) = unsafe {
            let vbo = self.gl.create_buffer()?;
            let ibo = match self.gl.create_buffer() {
                Ok(ibo) => ibo,
                Err(error) => {
                    self.gl.delete_buffer(vbo);
                    return Err(error);
                }
            };
            (vbo, ibo)
        };
        unsafe {
            self.gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
            match self.vertex_layout {
                VertexLayout::Packed => {
                    let packed: Vec<PackedVertex> =
                        mesh.vertices.iter().map(PackedVertex::from).collect();
                    let bytes = std::slice::from_raw_parts(
                        packed.as_ptr().cast::<u8>(),
                        packed
                            .len()
                            .saturating_mul(std::mem::size_of::<PackedVertex>()),
                    );
                    self.gl
                        .buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes, glow::STATIC_DRAW);
                }
                VertexLayout::Exact => {
                    let bytes = std::slice::from_raw_parts(
                        mesh.vertices.as_ptr().cast::<u8>(),
                        mesh.vertices
                            .len()
                            .saturating_mul(std::mem::size_of::<Vertex>()),
                    );
                    self.gl
                        .buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes, glow::STATIC_DRAW);
                }
            }
            self.gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(ibo));
            let index_bytes = std::slice::from_raw_parts(
                mesh.indices.as_ptr().cast::<u8>(),
                mesh.indices
                    .len()
                    .saturating_mul(std::mem::size_of::<u16>()),
            );
            self.gl.buffer_data_u8_slice(
                glow::ELEMENT_ARRAY_BUFFER,
                index_bytes,
                glow::STATIC_DRAW,
            );
            self.gl.bind_buffer(glow::ARRAY_BUFFER, None);
            self.gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, None);
        }
        Ok(DynamicMeshGpu {
            vbo,
            ibo,
            submeshes,
        })
    }

    /// Subtracts the dynamic scene's objects from the frame: one `u_mvp`
    /// upload, one light-probe upload and one draw call per object per
    /// primitive, sharing the static path's program, attributes and textures.
    ///
    /// The pass restores the static state before returning: `u_mvp` goes back
    /// to the view-projection and `u_light_scale` back to `[1, 1, 1]`, so the
    /// next frame's static draws are exactly as they were before this path
    /// existed.
    fn draw_dynamic_objects(
        &mut self,
        frustum: &Frustum,
        cull: bool,
        view_projection: &glam::Mat4,
        emissive_only: bool,
    ) -> DrawTotals {
        let mut totals = DrawTotals::default();
        if self.dynamic.is_empty()
            || self.dynamic_revision != self.dynamic.revision()
            || self.dynamic_meshes.len() != self.dynamic.mesh_count()
        {
            return totals;
        }
        self.begin_pass(ScenePass::World);
        // Take the scene and its GPU state out for the loop so the draw helpers
        // (which borrow `self` mutably) can run while the objects are iterated.
        // Both takes are constant-time moves: no per-frame allocation, and both
        // are put back before returning.
        let dynamic = std::mem::take(&mut self.dynamic);
        let meshes = std::mem::take(&mut self.dynamic_meshes);
        let mut bound: Option<(glow::Buffer, glow::Buffer)> = None;
        for object in dynamic.objects() {
            if cull && !frustum.intersects_aabb(&object.world_bounds()) {
                continue;
            }
            let Some(gpu) = meshes.get(object.model_path()) else {
                continue;
            };
            if bound != Some((gpu.vbo, gpu.ibo)) {
                self.bind_dynamic_buffers(gpu.vbo, gpu.ibo);
                bound = Some((gpu.vbo, gpu.ibo));
            }
            let mvp = {
                // `glam` matrix multiplication is per-element `f32` arithmetic
                // with no overflow or panic path; clippy cannot see that through
                // the operator impl (the same note `scene_view_projection`
                // carries).
                #[allow(clippy::arithmetic_side_effects)]
                let composed = *view_projection * object.transform();
                composed
            };
            let light_scale = if object.probe_valid() {
                object.light_scale()
            } else {
                [1.0; 3]
            };
            // Only the model transform, the composed MVP and the probe scale
            // change per object: everything else about the frame state stays.
            self.set_dynamic_frame_state(object.transform(), mvp, light_scale);
            // The object-wide override, resolved once per object; `None` means
            // every primitive draws its own material emission.
            let override_emission = object.emission().filter(MaterialEmission::is_emissive);
            let override_mask = override_emission.and_then(|emission| {
                emission.mask.and_then(|slot| {
                    self.prop_textures
                        .get(object.model_path())
                        .and_then(|textures| textures.get(usize::from(slot)).copied())
                })
            });
            for submesh in &gpu.submeshes {
                let emission = override_emission.map_or(submesh.emission, |emission| {
                    EmissionState::material(emission, override_mask)
                });
                if emissive_only && !emission.is_emissive() {
                    continue;
                }
                let state = SurfaceState {
                    emission,
                    ..SurfaceState::plain(submesh.texture)
                };
                unsafe { self.apply_surface_state(state) };
                unsafe {
                    self.gl.draw_elements(
                        glow::TRIANGLES,
                        submesh.index_count,
                        glow::UNSIGNED_SHORT,
                        submesh.first_index.saturating_mul(2),
                    );
                }
                totals.add(submesh.vertex_count);
            }
        }
        if totals.calls > 0 {
            // Hand the world program back exactly as the static path left it:
            // the scene MVP, an identity model transform and no light override.
            self.restore_frame_mvp();
        }
        self.dynamic = dynamic;
        self.dynamic_meshes = meshes;
        totals
    }

    /// Binds one dynamic mesh's buffer pair and points the scene attributes at
    /// it (the attributes are captured against the bound buffer, exactly like
    /// [`Self::bind_chunk`]).
    fn bind_dynamic_buffers(&self, vbo: glow::Buffer, ibo: glow::Buffer) {
        unsafe {
            self.gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
            self.set_vertex_attributes();
            self.gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(ibo));
        }
    }
}

/// What one draw loop submitted, so the frame counters can be summed.
#[derive(Clone, Copy, Debug, Default)]
struct DrawTotals {
    vertices: usize,
    batches: usize,
    calls: usize,
}

impl DrawTotals {
    /// Records one submitted batch: its live vertices, one batch, one draw call.
    fn add(&mut self, vertex_count: i32) {
        self.vertices = self
            .vertices
            .saturating_add(usize::try_from(vertex_count.max(0)).unwrap_or(0));
        self.batches = self.batches.saturating_add(1);
        self.calls = self.calls.saturating_add(1);
    }

    /// The sum of two loops' totals.
    const fn plus(self, other: Self) -> Self {
        Self {
            vertices: self.vertices.saturating_add(other.vertices),
            batches: self.batches.saturating_add(other.batches),
            calls: self.calls.saturating_add(other.calls),
        }
    }
}

/// Uploads packed chunks into `buffers`, reusing the buffer pairs and dropping
/// any left over from a larger previous level.
///
/// # Errors
///
/// Returns a message when a GL buffer cannot be created.
fn upload_chunks(
    gl: &glow::Context,
    layout: VertexLayout,
    buffers: &mut Vec<(glow::Buffer, glow::Buffer)>,
    chunks: &[MeshChunk],
) -> Result<(), String> {
    unsafe {
        // Drop any buffers left over from a larger previous level.
        while buffers.len() > chunks.len() {
            if let Some((vbo, ibo)) = buffers.pop() {
                gl.delete_buffer(vbo);
                gl.delete_buffer(ibo);
            }
        }
        for (index, chunk) in chunks.iter().enumerate() {
            if index == buffers.len() {
                let vbo = gl.create_buffer()?;
                let ibo = gl.create_buffer()?;
                buffers.push((vbo, ibo));
            }
            let Some(&(vbo, ibo)) = buffers.get(index) else {
                continue;
            };
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
            match layout {
                VertexLayout::Packed => {
                    // The one and only place the exact build vertices
                    // become the packed GPU representation.
                    let packed: Vec<PackedVertex> =
                        chunk.vertices.iter().map(PackedVertex::from).collect();
                    let vertex_bytes = std::slice::from_raw_parts(
                        packed.as_ptr().cast::<u8>(),
                        packed
                            .len()
                            .saturating_mul(std::mem::size_of::<PackedVertex>()),
                    );
                    gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, vertex_bytes, glow::STATIC_DRAW);
                }
                VertexLayout::Exact => {
                    let vertex_bytes = std::slice::from_raw_parts(
                        chunk.vertices.as_ptr().cast::<u8>(),
                        chunk
                            .vertices
                            .len()
                            .saturating_mul(std::mem::size_of::<Vertex>()),
                    );
                    gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, vertex_bytes, glow::STATIC_DRAW);
                }
            }

            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(ibo));
            let index_bytes = std::slice::from_raw_parts(
                chunk.indices.as_ptr().cast::<u8>(),
                chunk
                    .indices
                    .len()
                    .saturating_mul(std::mem::size_of::<u16>()),
            );
            gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, index_bytes, glow::STATIC_DRAW);
        }
        gl.bind_buffer(glow::ARRAY_BUFFER, None);
        gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, None);
    }
    Ok(())
}

/// Packs every static range into 16-bit-indexable buffer pairs.
///
/// Each range keeps its own vertex block and its indices are re-based as it is
/// packed, so a draw never needs a base-vertex offset (which core OpenGL ES 2.0
/// does not have). With `indexed` false (`LIMINAL_BENCH_NOINDEX`) every range is
/// expanded into a flat triangle list first, so one build can measure indexed
/// submission against non-indexed submission with the same batching, culling
/// and vertex layout.
pub(super) fn pack_static_batches(
    mesh: &LevelMesh,
    indexed: bool,
) -> (MeshPacker, Vec<StaticBatch>) {
    let mut packer = MeshPacker::default();
    let mut batches: Vec<StaticBatch> = Vec::with_capacity(mesh.ranges.len());
    for range in &mesh.ranges {
        let placements = if indexed {
            packer.push(&range.vertices, &range.indices)
        } else {
            packer.push_unindexed(&range.vertices, &range.indices)
        };
        for packed in placements {
            batches.push(StaticBatch {
                key: range.key,
                chunk: packed.chunk,
                index_range: BatchRange {
                    start: packed.index_start,
                    count: packed.index_count,
                },
                vertex_count: packed.vertex_count,
                bounds: range.bounds,
            });
        }
    }
    (packer, batches)
}

/// Builds the frame's view-projection matrix and the frustum extracted from it.
///
/// The configured FOV is the `PocketCHIP` baseline; wider displays gain
/// horizontal view, taller displays keep the horizontal view instead of
/// cropping it. The frustum comes from the very matrix the GPU clips against,
/// so it can never disagree with what is on screen: pitch, a resized drawable
/// and an unusual aspect ratio are all included.
///
/// The projection uses the *OpenGL* clip-depth convention (`z_ndc` in
/// `[-1, 1]`), not glam's default `[0, 1]` one. OpenGL maps `[-1, 1]` onto the
/// depth buffer, so a `[0, 1]` matrix would only ever write the buffer's upper
/// half and halve the usable depth precision for no reason — precisely the
/// margin coplanar surfaces need. The frustum is extracted with the matching
/// depth convention so culling and clipping stay in lockstep.
fn scene_view_projection(
    camera_pos: glam::Vec3,
    camera_yaw: f32,
    camera_pitch: f32,
    fov_degrees: f32,
    drawable: DrawableSize,
) -> (glam::Mat4, Frustum) {
    let aspect = drawable.aspect_ratio();
    let effective_fov = vertical_fov_for_aspect(fov_degrees, aspect);
    let proj = glam::Mat4::perspective_rh_gl(
        effective_fov.to_radians(),
        aspect,
        SCENE_NEAR_M,
        SCENE_FAR_M,
    );

    // Correctly combine yaw and pitch in the camera forward vector.
    let cos_pitch = camera_pitch.cos();
    let forward = glam::Vec3::new(
        camera_yaw.sin() * cos_pitch,
        camera_pitch.sin(),
        -camera_yaw.cos() * cos_pitch,
    );
    // `glam`'s vector and matrix operators are per-component `f32` arithmetic
    // with no overflow or panic path; clippy cannot see that through the
    // operator impls, so the two operations below carry a documented allow.
    #[allow(clippy::arithmetic_side_effects)]
    let view = glam::Mat4::look_at_rh(camera_pos, camera_pos + forward, glam::Vec3::Y);
    #[allow(clippy::arithmetic_side_effects)]
    let mvp = proj * view;
    let frustum = Frustum::from_view_projection(&mvp, crate::spatial::DepthRange::NegativeOneToOne);
    (mvp, frustum)
}
