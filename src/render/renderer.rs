//! The OpenGL renderer: GL state, buffers, draws and captures.
//!
//! The renderer owns the GPU side of one level: the static and prop vertex
//! buffers, the textures, the draw calls (static batches, prop batches, decals
//! and the UI pass) and the readback used by developer captures. It never bakes
//! lighting or builds geometry; it uploads and draws what those phases already
//! produced.

use super::api::{build_level_geometry_timed, shipped_asset_catalog};
use super::decals::{DECAL_ATLAS_SIZE, generate_decal_atlas};
use super::view::dimension_f32;
use super::{
    BatchRange, DECAL_ALPHA_CUTOFF, DECAL_EXTERNAL_BASE, DECAL_FRAGMENT_SHADER_SRC,
    DECAL_POLYGON_OFFSET, DrawableSize, EMISSION_MASK_TEXTURE_UNIT, FRAGMENT_SHADER_SRC,
    HasContext, LevelMesh, LIGHTMAP_PAGE_SLOTS, LIGHTMAP_TEXTURE_UNIT, LIGHTMAP_TEXTURE_UNIT_1,
    MaterialIndex, MaterialTable, MeshChunk, MeshPacker, PackedVertex, PropMeshBatch,
    SCENE_ATTRIB_COLOR, SCENE_ATTRIB_LIGHTMAP_PAGE, SCENE_ATTRIB_LIGHTMAP_UV, SCENE_ATTRIB_POS,
    SCENE_ATTRIB_UV, SCENE_FAR_M, SCENE_NEAR_M, SCENE_TEXTURE_UNIT, StaticBatch, SurfaceKey,
    SurfaceKind, UI_REFERENCE_HEIGHT, UI_REFERENCE_WIDTH, VERTEX_SHADER_SRC, Vertex, VertexLayout,
    decal_external_sheet_ids, generate_font_atlas, generate_white_texture, packed_layout,
    spatial_cell_grid, vertical_fov_for_aspect,
};
use crate::spatial::Frustum;

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

unsafe fn create_program(
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
        // and must not need to re-point the vertex attributes.
        gl.bind_attrib_location(program, SCENE_ATTRIB_POS, "a_pos");
        gl.bind_attrib_location(program, SCENE_ATTRIB_COLOR, "a_color");
        gl.bind_attrib_location(program, SCENE_ATTRIB_UV, "a_uv");
        gl.bind_attrib_location(program, SCENE_ATTRIB_LIGHTMAP_UV, "a_lightmap_uv");
        gl.bind_attrib_location(program, SCENE_ATTRIB_LIGHTMAP_PAGE, "a_lightmap_page");
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

/// The emission term the world program should currently be drawing with.
///
/// A batch's emission is material state, not vertex state, so it reaches the
/// GPU as uniforms. This value is cached by the renderer: a run of batches that
/// share the previous batch's emission costs no uniform or texture-unit work at
/// all, and non-emissive content stays exactly as cheap as it was before the
/// emission path existed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct EmissionState {
    /// Colour premultiplied by intensity, or the per-vertex colour when
    /// `vertex` is set.
    color: [f32; 3],
    /// Mask sheet bound on the emissive texture unit. `None` disables the mask,
    /// which is the common case and skips a fragment-stage texture fetch.
    mask: Option<glow::Texture>,
    /// True when the vertex colour itself carries the emission (fixture faces).
    vertex: bool,
}

impl EmissionState {
    /// No emission: every material authored before emission existed.
    const NONE: Self = Self {
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
    /// Bytes resident in static vertex buffers (level + props).
    pub vbo_bytes: usize,
    /// Bytes resident in element (index) buffers.
    pub index_bytes: usize,
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

/// The GL objects every renderer owns from startup: the two scene programs, the
/// UI vertex buffer, the untextured and font sheets, and the scene attribute and
/// uniform locations.
struct StartupResources {
    program: glow::Program,
    ui_vbo: glow::Buffer,
    white_texture: glow::Texture,
    font_texture: glow::Texture,
    decal: DecalPass,
    u_mvp_loc: Option<glow::UniformLocation>,
    u_texture_loc: Option<glow::UniformLocation>,
    u_emission_color_loc: Option<glow::UniformLocation>,
    u_emission_mask_enabled_loc: Option<glow::UniformLocation>,
    u_emission_vertex_loc: Option<glow::UniformLocation>,
    u_lightmap_enabled_loc: Option<glow::UniformLocation>,
    u_light_scale_loc: Option<glow::UniformLocation>,
    a_pos_loc: u32,
    a_color_loc: u32,
    a_uv_loc: u32,
    a_lightmap_uv_loc: u32,
    a_lightmap_page_loc: u32,
}

impl StartupResources {
    /// Creates the renderer's startup GL objects and applies the base GL state.
    ///
    /// # Errors
    ///
    /// Returns a message when a shader program does not link or a texture,
    /// buffer or attribute lookup fails.
    unsafe fn create(gl: &glow::Context) -> Result<Self, String> {
        unsafe {
            gl.enable(glow::DEPTH_TEST);
            gl.depth_func(glow::LEQUAL);
            gl.clear_color(0.08, 0.08, 0.09, 1.0);

            let program = create_program(gl, VERTEX_SHADER_SRC, FRAGMENT_SHADER_SRC)?;
            let a_pos_loc = gl
                .get_attrib_location(program, "a_pos")
                .ok_or_else(|| "Missing a_pos attribute".to_string())?;
            let a_color_loc = gl
                .get_attrib_location(program, "a_color")
                .ok_or_else(|| "Missing a_color attribute".to_string())?;
            let a_uv_loc = gl
                .get_attrib_location(program, "a_uv")
                .ok_or_else(|| "Missing a_uv attribute".to_string())?;
            let a_lightmap_uv_loc = gl
                .get_attrib_location(program, "a_lightmap_uv")
                .ok_or_else(|| "Missing a_lightmap_uv attribute".to_string())?;
            let a_lightmap_page_loc = gl
                .get_attrib_location(program, "a_lightmap_page")
                .ok_or_else(|| "Missing a_lightmap_page attribute".to_string())?;

            let u_mvp_loc = gl.get_uniform_location(program, "u_mvp");
            let u_texture_loc = gl.get_uniform_location(program, "u_texture");
            // Emission state. The sampler units are fixed once (albedo on 0,
            // mask on 1) and the mask unit always has a texture bound, so a
            // shader that samples it anyway reads a defined value.
            let u_emission_color_loc = gl.get_uniform_location(program, "u_emission_color");
            let u_emission_mask_enabled_loc =
                gl.get_uniform_location(program, "u_emission_mask_enabled");
            let u_emission_vertex_loc = gl.get_uniform_location(program, "u_emission_vertex");
            // Lightmap state: both atlas units are fixed once and default to the
            // white sheet with the global switch off, which is exactly the
            // vertex-lit fallback.
            let u_lightmap_enabled_loc = gl.get_uniform_location(program, "u_lightmap_enabled");
            let u_light_scale_loc = gl.get_uniform_location(program, "u_light_scale");
            if let Some(ref loc) = u_texture_loc {
                gl.uniform_1_i32(Some(loc), SCENE_TEXTURE_UNIT);
            }
            if let Some(loc) = gl.get_uniform_location(program, "u_emission_mask") {
                gl.uniform_1_i32(Some(&loc), EMISSION_MASK_TEXTURE_UNIT);
            }
            if let Some(loc) = gl.get_uniform_location(program, "u_lightmap0") {
                gl.uniform_1_i32(Some(&loc), LIGHTMAP_TEXTURE_UNIT);
            }
            if let Some(loc) = gl.get_uniform_location(program, "u_lightmap1") {
                gl.uniform_1_i32(Some(&loc), LIGHTMAP_TEXTURE_UNIT_1);
            }
            if let Some(ref loc) = u_lightmap_enabled_loc {
                gl.uniform_1_f32(Some(loc), 0.0);
            }
            if let Some(ref loc) = u_light_scale_loc {
                gl.uniform_3_f32(Some(loc), 1.0, 1.0, 1.0);
            }

            // The untextured fixture sheet and the UI/decal resources are the
            // only textures this renderer owns up front. Every surface texture
            // is uploaded by `set_level`, once per distinct resolved texture.
            let white_texture =
                create_texture_2d(gl, 2, 2, &generate_white_texture(), false, false)?;
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

            Ok(Self {
                program,
                ui_vbo,
                white_texture,
                font_texture,
                decal,
                u_mvp_loc,
                u_texture_loc,
                u_emission_color_loc,
                u_emission_mask_enabled_loc,
                u_emission_vertex_loc,
                u_lightmap_enabled_loc,
                u_light_scale_loc,
                a_pos_loc,
                a_color_loc,
                a_uv_loc,
                a_lightmap_uv_loc,
                a_lightmap_page_loc,
            })
        }
    }
}

/// Manages OpenGL ES 2.0-compatible accelerated rendering context, textures, and scene/UI drawing.
pub struct Renderer {
    _gl_context: sdl2::video::GLContext,
    gl: glow::Context,
    program: glow::Program,
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
    /// Maps a material index (what batches carry) to its texture index (what
    /// `material_textures` is indexed by). Two materials that share one texture
    /// map to the same slot, so the upload is shared.
    material_texture_slots: Vec<u16>,
    /// Emission per material index, parallel to `material_texture_slots`.
    material_emissions: Vec<crate::materials::MaterialEmission>,
    /// The emission the world program is currently set to draw with. See
    /// [`EmissionState`].
    emission_state: EmissionState,
    /// Runtime quality profile: how large a texture may reach the GPU. Chosen
    /// once at startup from the settings (`full` or `low`).
    quality: crate::quality::QualityProfile,
    /// The untextured sheet every family without its own artwork binds: the
    /// light housing's flat vertex colour, a prop placeholder box, the UI.
    white_texture: glow::Texture,
    font_texture: glow::Texture,
    /// Decal rendering state (program, shared sheet, uniforms).
    decal: DecalPass,
    u_mvp_loc: Option<glow::UniformLocation>,
    u_texture_loc: Option<glow::UniformLocation>,
    /// World-program locations of the emission uniforms (see [`EmissionState`]).
    u_emission_color_loc: Option<glow::UniformLocation>,
    u_emission_mask_enabled_loc: Option<glow::UniformLocation>,
    u_emission_vertex_loc: Option<glow::UniformLocation>,
    /// World-program switch that turns lightmap sampling on.
    ///
    /// Zero whenever no valid lightmap set is resident, which makes every vertex
    /// fall back to the light already baked into its colour.
    u_lightmap_enabled_loc: Option<glow::UniformLocation>,
    /// World-program multiplier the dynamic-object path sets to the baked light
    /// sampled at a moving object's current position. Static draws keep `1`.
    u_light_scale_loc: Option<glow::UniformLocation>,
    /// The light multiplier the world program is currently drawing with, so the
    /// static/dynamic switch does not re-upload an unchanged uniform.
    light_scale: [f32; 3],
    /// Lightmap atlas pages currently bound on texture units 2 and 3. The shared
    /// white sheet stands in when a page is absent, so the units are never
    /// unbound.
    lightmap_pages: [glow::Texture; LIGHTMAP_PAGE_SLOTS],
    /// Whether [`Self::lightmap_pages`] holds a real baked atlas.
    lightmaps_resident: bool,
    /// Whether the world program is currently sampling the atlas. Follows
    /// `lightmaps_resident` for a normal level load; the settings switch and the
    /// benchmark harness can turn sampling off without dropping the atlas, which
    /// is what makes an A/B capture of the two lighting paths cheap.
    lightmaps_enabled: bool,
    a_pos_loc: u32,
    a_color_loc: u32,
    a_uv_loc: u32,
    a_lightmap_uv_loc: u32,
    a_lightmap_page_loc: u32,
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
}

impl Renderer {
    /// Initializes an accelerated OpenGL context with `VSync`, textures and an
    /// empty level buffer.
    ///
    /// The caller uploads the first level with [`Renderer::set_level`] (or
    /// [`Renderer::rebuild_level_geometry`]); building here as well would bake
    /// and upload the same level twice before the first frame, which is real
    /// cost on the `PocketCHIP`.
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

        let StartupResources {
            program,
            ui_vbo,
            white_texture,
            font_texture,
            decal,
            u_mvp_loc,
            u_texture_loc,
            u_emission_color_loc,
            u_emission_mask_enabled_loc,
            u_emission_vertex_loc,
            u_lightmap_enabled_loc,
            u_light_scale_loc,
            a_pos_loc,
            a_color_loc,
            a_uv_loc,
            a_lightmap_uv_loc,
            a_lightmap_page_loc,
        } = unsafe { StartupResources::create(&gl)? };

        let (initial_width, initial_height) = window.drawable_size();

        let renderer = Self {
            _gl_context: gl_context,
            gl,
            program,
            level_buffers: Vec::new(),
            ui_vbo,
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
            prop_textures: std::collections::HashMap::new(),
            surface_textures: std::collections::HashMap::new(),
            fixture_sheet_textures: std::collections::HashMap::new(),
            fixture_sheets: Vec::new(),
            decal_image_cache: crate::materials::TextureCache::new(),
            decal_sheet_textures: std::collections::HashMap::new(),
            level_textures: Vec::new(),
            material_textures: Vec::new(),
            material_texture_slots: Vec::new(),
            material_emissions: Vec::new(),
            emission_state: EmissionState::NONE,
            quality: crate::quality::QualityProfile::DEFAULT,
            white_texture,
            font_texture,
            decal,
            lightmap_pages: [white_texture; LIGHTMAP_PAGE_SLOTS],
            lightmaps_resident: false,
            lightmaps_enabled: false,
            light_scale: [1.0; 3],
            u_mvp_loc,
            u_texture_loc,
            u_emission_color_loc,
            u_emission_mask_enabled_loc,
            u_emission_vertex_loc,
            u_lightmap_enabled_loc,
            u_light_scale_loc,
            a_pos_loc,
            a_color_loc,
            a_uv_loc,
            a_lightmap_uv_loc,
            a_lightmap_page_loc,
            linear_filtering: true,
            drawable_size: DrawableSize::new(initial_width, initial_height),
            level_stats: LevelBuildStats::default(),
            render_stats: RenderStats::default(),
        };
        Ok(renderer)
    }

    /// Records the current physical framebuffer size.
    ///
    /// This renderer draws directly into the default framebuffer, so no offscreen
    /// colour/depth attachments exist to recreate; the viewport and projection are
    /// derived from this size each frame. Returns `true` when the size changed,
    /// which is where any future size-dependent GPU resource would be rebuilt.
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
    /// [`Full`]: crate::quality::QualityProfile::Full
    /// [`Low`]: crate::quality::QualityProfile::Low
    pub const fn set_quality(&mut self, quality: crate::quality::QualityProfile) {
        self.quality = quality;
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
        let (mesh, batches, lighting, timings) =
            build_level_geometry_timed(level, &self.prop_catalog, &mut self.prop_assets, materials);
        self.spatial_grid = spatial_cell_grid(level);

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
            eprintln!("[level] cannot upload static geometry: {error}");
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
        };
        self.prop_draws = draws;
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
                    eprintln!(
                        "[props] cannot upload texture for {model}: {error}; skipping that model"
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
                    eprintln!("[decals] {error}; drawing the diagnostic sheet instead");
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
                    eprintln!("[decals] decal `{id}`: {error}; drawing the built-in sheet instead");
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
                    eprintln!(
                        "[materials] cannot upload texture `{}`: {error}; binding the missing pattern",
                        texture.key
                    );
                    material_textures.push(self.white_texture);
                }
            }
        }
        self.material_textures = material_textures;
        self.material_texture_slots = loaded
            .materials
            .entries()
            .iter()
            .map(|entry| entry.texture_index)
            .collect();
        self.material_emissions = loaded
            .materials
            .entries()
            .iter()
            .map(|entry| entry.emission)
            .collect();

        // The fixture sheets: one slot per family, so a light batch binds its
        // family's PNG by the sheet index it carries. Catalog sheets stay
        // resident across level changes; a pack's own sheet belongs to this
        // level and is freed with it. A family with no sheet binds the
        // untextured white sheet, which draws the flat authored fixture glow
        // exactly as every fixture did before the sheets existed.
        self.fixture_sheets = self.upload_fixture_sheets(&loaded.light_sheets, &mut level_textures);

        self.level_textures = level_textures;
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
                    eprintln!(
                        "[fixtures] cannot upload sheet `{}`: {error}; drawing the untextured sheet",
                        sheet.key
                    );
                }
            }
        }
        slots
    }

    /// Renders the 3D level combining yaw and pitch into the view matrix.
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

        let (mvp, frustum) =
            scene_view_projection(camera_pos, camera_yaw, camera_pitch, fov_degrees, drawable);
        let cull = self.culling_enabled;

        unsafe {
            // Render at the real drawable resolution; no fixed 480x272 target.
            self.gl.viewport(
                0,
                0,
                i32::try_from(drawable.width).unwrap_or(i32::MAX),
                i32::try_from(drawable.height).unwrap_or(i32::MAX),
            );
            self.gl
                .clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
            self.gl.use_program(Some(self.program));
        }

        if let Some(ref loc) = self.u_mvp_loc {
            unsafe {
                self.gl
                    .uniform_matrix_4_f32_slice(Some(loc), false, &mvp.to_cols_array());
            }
        }
        if let Some(ref loc) = self.u_texture_loc {
            unsafe { self.gl.uniform_1_i32(Some(loc), 0) };
        }
        // Lightmap atlas pages live on their own units and are bound once per
        // frame: the global switch is on only while a real atlas is resident, and
        // every vertex whose page is `LIGHTMAP_NONE` takes the vertex-lit path
        // regardless.
        unsafe {
            self.gl.active_texture(glow::TEXTURE2);
            self.gl
                .bind_texture(glow::TEXTURE_2D, Some(self.lightmap_pages[0]));
            self.gl.active_texture(glow::TEXTURE3);
            self.gl
                .bind_texture(glow::TEXTURE_2D, Some(self.lightmap_pages[1]));
        }
        if let Some(ref loc) = self.u_lightmap_enabled_loc {
            unsafe {
                self.gl.uniform_1_f32(
                    Some(loc),
                    if self.lightmaps_resident { 1.0 } else { 0.0 },
                );
            }
        }
        unsafe { self.gl.active_texture(glow::TEXTURE0) };

        // Static level geometry, then the batched props, then the decal pass:
        // one draw loop each, in the order their state depends on.
        let totals = self
            .draw_static_batches(&frustum, cull)
            .plus(self.draw_prop_batches(&frustum, cull))
            .plus(self.draw_decal_batches(&frustum, cull, &mvp));

        unsafe {
            self.gl.disable_vertex_attrib_array(self.a_pos_loc);
            self.gl.disable_vertex_attrib_array(self.a_color_loc);
            self.gl.disable_vertex_attrib_array(self.a_uv_loc);
            self.gl
                .disable_vertex_attrib_array(self.a_lightmap_uv_loc);
            self.gl
                .disable_vertex_attrib_array(self.a_lightmap_page_loc);
            self.gl.bind_texture(glow::TEXTURE_2D, None);
            self.gl.bind_buffer(glow::ARRAY_BUFFER, None);
            self.gl.use_program(None);
        }

        // Report what this frame actually submitted, straight from the draw
        // path rather than reconstructed from the level.
        let total_vertices = self
            .level_stats
            .static_vertices
            .saturating_add(self.level_stats.prop_vertices);
        self.render_stats = RenderStats {
            total_vertices,
            visible_vertices: totals.vertices,
            culled_vertices: total_vertices.saturating_sub(totals.vertices),
            total_batches: self
                .static_batches
                .len()
                .saturating_add(self.prop_draws.len()),
            visible_batches: totals.batches,
            draw_calls: totals.calls,
            vbo_bytes: self.level_stats.vbo_bytes,
            index_bytes: self.level_stats.index_bytes,
        };
    }

    /// Submits every opaque static batch the frustum keeps, binding each
    /// texture, emission state and buffer pair once per run.
    ///
    /// Decals are skipped here: they are submitted by their own pass, with the
    /// decal program and depth bias, which keeps the world program's early
    /// depth testing intact.
    fn draw_static_batches(&mut self, frustum: &Frustum, cull: bool) -> DrawTotals {
        let mut totals = DrawTotals::default();
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
            if cull && !frustum.intersects_aabb(&batch.bounds) {
                continue;
            }
            if bound_chunk != Some(batch.chunk) {
                if self.bind_chunk(&self.level_buffers, batch.chunk) {
                    bound_chunk = Some(batch.chunk);
                } else {
                    continue;
                }
            }
            if bound_key != Some(batch.key) {
                let texture = self.static_texture(batch.key);
                let emission = self.static_emission(batch.key);
                unsafe {
                    self.gl.bind_texture(glow::TEXTURE_2D, Some(texture));
                    self.set_emission(emission);
                }
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

    /// The emission one static surface key draws with.
    ///
    /// Floors, ceilings and walls take their material's emission from the
    /// material table. Fixture luminous faces carry theirs per vertex: their
    /// key's material slot is the family's sheet, not a level material, and the
    /// glow varies per placement while the batch is shared. Fixture housings,
    /// placeholder boxes and anything else emit nothing.
    fn static_emission(&self, key: SurfaceKey) -> EmissionState {
        match emission_routing(key.kind, key.has_material()) {
            EmissionRouting::None => EmissionState::NONE,
            EmissionRouting::Vertex => EmissionState::vertex(),
            EmissionRouting::Material => {
                let emission = self
                    .material_emissions
                    .get(usize::from(key.material))
                    .copied()
                    .unwrap_or_default();
                if !emission.is_emissive() {
                    return EmissionState::NONE;
                }
                let mask = emission
                    .mask
                    .and_then(|index| self.material_textures.get(usize::from(index)).copied());
                EmissionState::material(emission, mask)
            }
        }
    }

    /// Applies the emission the world program should draw with, skipping the
    /// work when the previous batch already set exactly this state.
    ///
    /// # Safety
    ///
    /// The world program must be current. The mask sampler is bound on its own
    /// texture unit and the active unit is restored to
    /// [`SCENE_TEXTURE_UNIT`] before returning.
    unsafe fn set_emission(&mut self, state: EmissionState) {
        if self.emission_state == state {
            return;
        }
        unsafe {
            if let Some(ref loc) = self.u_emission_color_loc {
                self.gl
                    .uniform_3_f32(Some(loc), state.color[0], state.color[1], state.color[2]);
            }
            if let Some(ref loc) = self.u_emission_mask_enabled_loc {
                self.gl
                    .uniform_1_f32(Some(loc), if state.mask.is_some() { 1.0 } else { 0.0 });
            }
            if let Some(ref loc) = self.u_emission_vertex_loc {
                self.gl
                    .uniform_1_f32(Some(loc), if state.vertex { 1.0 } else { 0.0 });
            }
            self.gl.active_texture(glow::TEXTURE1);
            self.gl.bind_texture(
                glow::TEXTURE_2D,
                Some(state.mask.unwrap_or(self.white_texture)),
            );
            self.gl.active_texture(glow::TEXTURE0);
        }
        self.emission_state = state;
    }

    /// The texture one static surface key binds: its resolved material sheet,
    /// the fixture sheet its light batch carries, the decal sheet, or the
    /// untextured white sheet.
    fn static_texture(&self, key: SurfaceKey) -> glow::Texture {
        match key.kind {
            SurfaceKind::Floor | SurfaceKind::Ceiling | SurfaceKind::Wall => {
                if key.has_material() {
                    let slot = self
                        .material_texture_slots
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
    fn draw_prop_batches(&mut self, frustum: &Frustum, cull: bool) -> DrawTotals {
        let mut totals = DrawTotals::default();
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
            unsafe { self.set_emission(draw.emission) };
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

    /// Submits the decal pass: local surface markings drawn after the opaque
    /// world and the props.
    ///
    /// Depth testing stays on and depth writes stay on, so a decal is still
    /// hidden by anything in front of it; the pass adds a fixed polygon offset
    /// that pulls each decal two depth steps towards the camera, which is what
    /// makes it win the coincident-depth test against the surface it lies on.
    /// The world program and offset state are restored before returning.
    fn draw_decal_batches(&self, frustum: &Frustum, cull: bool, mvp: &glam::Mat4) -> DrawTotals {
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
            // program (whose uniforms are per-program and still valid), so
            // nothing after the pass can inherit decal state.
            unsafe {
                self.gl.polygon_offset(0.0, 0.0);
                self.gl.disable(glow::POLYGON_OFFSET_FILL);
                self.gl.use_program(Some(self.program));
            }
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
    /// buffer.
    pub fn set_light_scale(&mut self, scale: [f32; 3]) {
        let scale = scale.map(|value| if value.is_finite() { value } else { 1.0 });
        if self.light_scale == scale {
            return;
        }
        self.light_scale = scale;
        if let Some(ref loc) = self.u_light_scale_loc {
            unsafe {
                self.gl
                    .uniform_3_f32(Some(loc), scale[0], scale[1], scale[2]);
            }
        }
    }

    /// Turns lightmap sampling on or off without dropping the resident atlas.
    ///
    /// Enabling has no effect when no valid atlas is resident: a level whose bake
    /// failed must not render black surfaces, so the switch can only ever
    /// *restore* the vertex-lit path.
    pub fn set_lightmaps_enabled(&mut self, enabled: bool) {
        self.lightmaps_enabled = enabled && self.lightmaps_resident;
        self.upload_lightmap_switch();
    }

    /// Records whether a valid baked lightmap set is resident and follows it
    /// with the sampling switch: no atlas means the vertex-lit path.
    pub fn set_lightmaps_resident(&mut self, resident: bool) {
        self.lightmaps_resident = resident;
        self.lightmaps_enabled = resident;
        self.upload_lightmap_switch();
    }

    /// Pushes [`Self::lightmaps_enabled`] to the world program.
    fn upload_lightmap_switch(&mut self) {
        let enabled = self.lightmaps_enabled;
        if let Some(ref loc) = self.u_lightmap_enabled_loc {
            unsafe {
                self.gl
                    .uniform_1_f32(Some(loc), if enabled { 1.0 } else { 0.0 });
            }
        }
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
        let (color_type, color_normalized, color_offset) = match self.vertex_layout {
            VertexLayout::Packed => (glow::UNSIGNED_BYTE, true, packed_layout::COLOR_OFFSET),
            VertexLayout::Exact => (glow::FLOAT, false, 12),
        };
        let uv_offset = match self.vertex_layout {
            VertexLayout::Packed => packed_layout::UV_OFFSET,
            VertexLayout::Exact => 28,
        };
        let lightmap_offset = match self.vertex_layout {
            VertexLayout::Packed => packed_layout::LIGHTMAP_OFFSET,
            VertexLayout::Exact => 36,
        };
        let lightmap_page_offset = match self.vertex_layout {
            VertexLayout::Packed => packed_layout::LIGHTMAP_PAGE_OFFSET,
            VertexLayout::Exact => 40,
        };
        unsafe {
            self.gl.enable_vertex_attrib_array(self.a_pos_loc);
            self.gl
                .vertex_attrib_pointer_f32(self.a_pos_loc, 3, glow::FLOAT, false, stride, 0);
            self.gl.enable_vertex_attrib_array(self.a_color_loc);
            self.gl.vertex_attrib_pointer_f32(
                self.a_color_loc,
                4,
                color_type,
                color_normalized,
                stride,
                color_offset,
            );
            self.gl.enable_vertex_attrib_array(self.a_uv_loc);
            self.gl.vertex_attrib_pointer_f32(
                self.a_uv_loc,
                2,
                glow::FLOAT,
                false,
                stride,
                uv_offset,
            );
            self.gl.enable_vertex_attrib_array(self.a_lightmap_uv_loc);
            self.gl.vertex_attrib_pointer_f32(
                self.a_lightmap_uv_loc,
                2,
                glow::UNSIGNED_SHORT,
                true,
                stride,
                lightmap_offset,
            );
            self.gl.enable_vertex_attrib_array(self.a_lightmap_page_loc);
            self.gl.vertex_attrib_pointer_f32(
                self.a_lightmap_page_loc,
                1,
                glow::UNSIGNED_BYTE,
                false,
                stride,
                lightmap_page_offset,
            );
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

            self.gl.use_program(Some(self.program));

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

            if let Some(ref loc) = self.u_mvp_loc {
                self.gl
                    .uniform_matrix_4_f32_slice(Some(loc), false, &ortho.to_cols_array());
            }

            if let Some(ref loc) = self.u_texture_loc {
                self.gl.uniform_1_i32(Some(loc), 0);
            }
            // The HUD is never emissive: replace whatever emission state the
            // world left behind, so a bright fixture the camera walked away
            // from cannot leak its glow into the text.
            self.set_emission(EmissionState::NONE);
            self.gl.active_texture(glow::TEXTURE0);
            self.gl
                .bind_texture(glow::TEXTURE_2D, Some(self.font_texture));

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
fn pack_static_batches(mesh: &LevelMesh, indexed: bool) -> (MeshPacker, Vec<StaticBatch>) {
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
