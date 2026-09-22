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
    DECAL_POLYGON_OFFSET, DrawableSize, FRAGMENT_SHADER_SRC, HasContext, LevelMesh, MaterialIndex,
    MaterialTable, MeshChunk, MeshPacker, PackedVertex, PropMeshBatch, SCENE_ATTRIB_COLOR,
    SCENE_ATTRIB_POS, SCENE_ATTRIB_UV, StaticBatch, SurfaceKey, SurfaceKind, UI_REFERENCE_HEIGHT,
    UI_REFERENCE_WIDTH, VERTEX_SHADER_SRC, Vertex, VertexLayout, decal_external_sheet_ids,
    generate_font_atlas, generate_white_texture, packed_layout, spatial_cell_grid,
    vertical_fov_for_aspect,
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
/// single spatial cell, sharing one texture, drawn as a contiguous vertex range
/// of the prop buffer.
#[derive(Clone, Copy, Debug)]
pub(super) struct PropDraw {
    texture: glow::Texture,
    /// Which prop buffer pair this range lives in (see `MeshPacker`).
    chunk: usize,
    /// Range in that chunk's index buffer.
    index_start: i32,
    index_count: i32,
    /// Distinct vertices the range reads, for the debug counters.
    vertex_count: i32,
    bounds: crate::spatial::Aabb,
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
    a_pos_loc: u32,
    a_color_loc: u32,
    a_uv_loc: u32,
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

            let u_mvp_loc = gl.get_uniform_location(program, "u_mvp");
            let u_texture_loc = gl.get_uniform_location(program, "u_texture");

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
                a_pos_loc,
                a_color_loc,
                a_uv_loc,
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
    /// GPU textures for prop models, keyed by catalogue model path so a level
    /// change never re-uploads a texture that is already resident.
    prop_textures: std::collections::HashMap<String, glow::Texture>,
    /// GPU textures for catalog/missing surface textures, keyed by logical
    /// texture key. Decoded images are already cached per session; this cache
    /// keeps their GPU copies across level changes, so a level switch never
    /// re-uploads a built-in texture.
    surface_textures: std::collections::HashMap<String, glow::Texture>,
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
    /// The untextured fixture/light sheet (white unless a pack supplies one).
    white_texture: glow::Texture,
    font_texture: glow::Texture,
    /// Decal rendering state (program, shared sheet, uniforms).
    decal: DecalPass,
    u_mvp_loc: Option<glow::UniformLocation>,
    u_texture_loc: Option<glow::UniformLocation>,
    a_pos_loc: u32,
    a_color_loc: u32,
    a_uv_loc: u32,
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
            a_pos_loc,
            a_color_loc,
            a_uv_loc,
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
            decal_image_cache: crate::materials::TextureCache::new(),
            decal_sheet_textures: std::collections::HashMap::new(),
            level_textures: Vec::new(),
            material_textures: Vec::new(),
            material_texture_slots: Vec::new(),
            white_texture,
            font_texture,
            decal,
            u_mvp_loc,
            u_texture_loc,
            a_pos_loc,
            a_color_loc,
            a_uv_loc,
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
            for (_, texture) in &self.level_textures {
                self.gl.bind_texture(glow::TEXTURE_2D, Some(*texture));
                set_repeat_filter(&self.gl, linear);
            }
            for texture in self.prop_textures.values() {
                self.gl.bind_texture(glow::TEXTURE_2D, Some(*texture));
                set_repeat_filter(&self.gl, linear);
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

    unsafe fn upload_texture(
        gl: &glow::Context,
        texture: glow::Texture,
        raw_image: &crate::loader::RawImage,
        repeat: bool,
        linear: bool,
    ) {
        unsafe {
            gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA.cast_signed(),
                i32::try_from(raw_image.width).unwrap_or(i32::MAX),
                i32::try_from(raw_image.height).unwrap_or(i32::MAX),
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(&raw_image.rgba)),
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
        }
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
        for batch in batches {
            let texture = match self.prop_textures.get(&batch.model) {
                Some(texture) => *texture,
                None => match unsafe { self.upload_prop_texture(&batch.texture) } {
                    Ok(texture) => {
                        self.prop_textures.insert(batch.model.clone(), texture);
                        texture
                    }
                    Err(error) => {
                        eprintln!(
                            "[props] cannot upload texture for {}: {error}; skipping that batch",
                            batch.model
                        );
                        continue;
                    }
                },
            };
            let placements = if indexed {
                packer.push(&batch.vertices, &batch.indices)
            } else {
                packer.push_unindexed(&batch.vertices, &batch.indices)
            };
            for packed in placements {
                draws.push(PropDraw {
                    texture,
                    chunk: packed.chunk,
                    index_start: packed.index_start,
                    index_count: packed.index_count,
                    vertex_count: packed.vertex_count,
                    bounds: batch.bounds,
                });
            }
        }
        (packer, draws)
    }

    /// Uploads one prop model's diffuse texture with mipmaps and `CLAMP_TO_EDGE`
    /// wrapping (prop UVs never tile), matching the game's filtering setting.
    unsafe fn upload_prop_texture(
        &self,
        image: &crate::loader::RawImage,
    ) -> Result<glow::Texture, String> {
        unsafe {
            let texture = self.gl.create_texture()?;
            self.gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            self.gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA.cast_signed(),
                i32::try_from(image.width).unwrap_or(i32::MAX),
                i32::try_from(image.height).unwrap_or(i32::MAX),
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(&image.rgba)),
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
            let uploaded = unsafe {
                create_texture_2d(
                    &self.gl,
                    i32::try_from(image.width).unwrap_or(i32::MAX),
                    i32::try_from(image.height).unwrap_or(i32::MAX),
                    &image.rgba,
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
                create_texture_2d(
                    &self.gl,
                    i32::try_from(texture.image.width).unwrap_or(i32::MAX),
                    i32::try_from(texture.image.height).unwrap_or(i32::MAX),
                    &texture.image.rgba,
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
        self.level_textures = level_textures;

        // The light/fixture sheet: a pack may override it, otherwise the
        // untextured white sheet is restored so a previous pack's fixture never
        // leaks into the next level.
        unsafe {
            if let Some(fixture) = loaded.fixture.as_deref() {
                Self::upload_texture(&self.gl, self.white_texture, fixture, false, linear);
            } else {
                let white = generate_white_texture();
                Self::upload_texture(
                    &self.gl,
                    self.white_texture,
                    &crate::loader::RawImage::new(2, 2, white.to_vec()),
                    false,
                    linear,
                );
            }
        }
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
    /// texture and buffer pair once per run.
    ///
    /// Decals are skipped here: they are submitted by their own pass, with the
    /// decal program and depth bias, which keeps the world program's early
    /// depth testing intact.
    fn draw_static_batches(&self, frustum: &Frustum, cull: bool) -> DrawTotals {
        let mut totals = DrawTotals::default();
        let mut bound_key: Option<SurfaceKey> = None;
        let mut bound_chunk: Option<usize> = None;
        for batch in &self.static_batches {
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
                unsafe { self.gl.bind_texture(glow::TEXTURE_2D, Some(texture)) };
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

    /// The texture one static surface key binds: its resolved material sheet,
    /// the decal atlas, or the unshaded white sheet.
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
            SurfaceKind::Light | SurfaceKind::PropFallback => self.white_texture,
            SurfaceKind::Decal => self.decal.texture,
        }
    }

    /// Submits the batched real prop geometry: one buffer and one draw call per
    /// (model, spatial cell), with one texture bind per model.
    fn draw_prop_batches(&self, frustum: &Frustum, cull: bool) -> DrawTotals {
        let mut totals = DrawTotals::default();
        let mut bound_texture: Option<glow::Texture> = None;
        let mut bound_chunk: Option<usize> = None;
        for draw in &self.prop_draws {
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

    /// Points the three scene attributes at the selected vertex layout.
    ///
    /// In the packed layout `normalized = true` lets the fixed-function pipeline
    /// expand `GL_UNSIGNED_BYTE` colour to `[0, 1]` floats, so the shader is the
    /// same `vec4` in both layouts. Both are core OpenGL ES 2.0.
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
fn scene_view_projection(
    camera_pos: glam::Vec3,
    camera_yaw: f32,
    camera_pitch: f32,
    fov_degrees: f32,
    drawable: DrawableSize,
) -> (glam::Mat4, Frustum) {
    let aspect = drawable.aspect_ratio();
    let effective_fov = vertical_fov_for_aspect(fov_degrees, aspect);
    let proj = glam::Mat4::perspective_rh(effective_fov.to_radians(), aspect, 0.1, 100.0);

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
    let frustum = Frustum::from_view_projection(&mvp, crate::spatial::DepthRange::ZeroToOne);
    (mvp, frustum)
}
