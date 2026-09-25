//! Stage 9 decals: the reference's local surface markings.
//!
//! The OpenGL reference draws decals as the last scene pass: static
//! `SurfaceKind::Decal` ranges through a second program
//! (`DECAL_FRAGMENT_SHADER_SRC`) that is the shared world vertex stage plus an
//! alpha-tested texture multiply, with `glPolygonOffset(-1.0, -4.0)` pulling
//! each marking towards the camera, depth writes on and no blending. This
//! module owns the wgpu half of that contract:
//!
//! * the level's decal ranges packed into 16-bit-indexable buffer pairs, one
//!   draw record per packed range (the neutral [`MeshPacker`] may split a range
//!   across chunks, exactly like the world upload);
//! * one GPU texture per decal sheet: the generated 256x256 atlas from
//!   [`generate_decal_atlas`], then one texture per external PNG sheet in
//!   [`decal_external_sheet_ids`] order, each with a CPU mip chain and the
//!   reference's REPEAT world-sheet policy, and one bind group per filtering
//!   mode so the player's setting chooses the sampler at encode time;
//! * the [`DecalPipeline`]: the reference's decal program, `LessEqual` depth
//!   with writes on, [`DECAL_DEPTH_BIAS`], no culling and no blending;
//! * [`WgpuDecals::encode`], one draw per range surviving the frame's frustum
//!   test, submitted after the translucent pass and never in the emissive body.
//!
//! Display space is the first-order constraint, exactly as it was for the world
//! stage: the reference uploads decal sheets as a plain non-sRGB `GL_RGBA`
//! texture and multiplies the raw authored bytes, and its scene target is
//! non-sRGB too. The wgpu sheets therefore upload as non-sRGB
//! [`DECAL_SHEET_FORMAT`], the fragment multiplies in display space and
//! `decals.wgsl` converts the product to linear exactly once for the sRGB
//! target. The CPU mip chain averages the raw 8-bit channels without a gamma
//! step, which is what the reference's `glGenerateMipmap` applied to the same
//! non-sRGB sheet.
//!
//! The neutral geometry already lifts a decal [`crate::render::common::DECAL_SURFACE_OFFSET_M`]
//! along its surface normal; that near-field separation and this pass's
//! depth-buffer bias are the two halves of one contract and neither replaces
//! the other.

use std::path::Path;
use std::rc::Rc;

use glam::Mat4;

use super::surface::DEPTH_FORMAT;
use super::texture::{TextureCache, TextureFiltering};
use super::world::{
    CAMERA_UNIFORM_SIZE, CameraUniform, WORLD_VERTEX_STRIDE, WorldFrame, WorldVertex,
    camera_uniform_changed, world_vertex_layout,
};
use crate::assets::AssetCatalog;
use crate::level::LevelDef;
use crate::materials::RawImage;
use crate::quality::{QualityProfile, TextureClass, fit_image};
use crate::render::common::decals::{
    DECAL_ATLAS_SIZE, DECAL_EXTERNAL_BASE, decal_external_sheet_ids, generate_decal_atlas,
};
use crate::render::common::mesh::{LevelMesh, MaterialIndex, SurfaceKind, Vertex};
use crate::render::common::{MeshChunk, MeshPacker};
use crate::spatial::Aabb;

/// The decal shader, from the file next to this module.
pub const DECAL_SHADER_SRC: &str = include_str!("decals.wgsl");

/// Name of the decal vertex entry point.
pub const DECAL_VERTEX_ENTRY: &str = "vs_main";
/// Name of the decal fragment entry point.
pub const DECAL_FRAGMENT_ENTRY: &str = "fs_main";
/// The fragment entry point for a raw (non-sRGB) scene or reflection target:
/// the decal product is display-space, so it is written directly, exactly like
/// the world shader's raw entry points.
pub const DECAL_FRAGMENT_ENTRY_RAW: &str = "fs_main_raw";

/// The format every decal sheet uploads as.
///
/// Raw display space, exactly like the reference's plain `GL_RGBA` decal sheets
/// and (since Stage 10) every other texture: the fragment multiplies the raw
/// authored bytes and writes display-space targets directly, converting only
/// for the sRGB surface.
pub const DECAL_SHEET_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// Maps the reference's `glPolygonOffset(factor, units)` onto wgpu's depth bias.
///
/// The two APIs name the terms differently but the semantics line up one to
/// one: GL's `factor` is the slope-scaled term and becomes wgpu's
/// `slope_scale`; GL's `units` is the constant term and becomes wgpu's
/// `constant`. Both bias terms land in the depth buffer's own resolution units
/// — GL's `units` in steps of the attached depth format, wgpu's `constant` in
/// steps of the target's depth format ([`DEPTH_FORMAT`], a 32-bit float here;
/// the reference preferred a 24-bit buffer for this very reason) — so the
/// reference's numbers are the calibrated mapping rather than a unit-for-unit
/// identity. `clamp` is zero because GL has no clamp term: any clamp would
/// weaken the pull this bias exists to apply.
#[must_use]
pub const fn depth_bias_state(factor: f32, units: i32) -> wgpu::DepthBiasState {
    wgpu::DepthBiasState {
        constant: units,
        slope_scale: factor,
        clamp: 0.0,
    }
}

/// The decal pass's depth bias: the reference's `DECAL_POLYGON_OFFSET`.
///
/// Both terms are negative, which pulls a decal *towards* the camera; the
/// slope-scaled half keeps the pull at grazing angles and long range, where the
/// constant term is finer than the depth buffer's resolution.
pub const DECAL_DEPTH_BIAS: wgpu::DepthBiasState = depth_bias_state(-1.0, -4);

/// One 16-bit-indexable decal buffer pair.
///
/// The same shape as the world's chunk, kept separate because the two passes
/// never share a buffer: decal geometry is packed, uploaded and released with
/// the decal pass's own lifetime.
struct DecalChunk {
    /// Interleaved [`WorldVertex`] data.
    vertex_buffer: wgpu::Buffer,
    /// `u16` triangle indices into `vertex_buffer`.
    index_buffer: wgpu::Buffer,
    /// Distinct vertices the chunk holds.
    vertex_count: u32,
    /// Indices the chunk holds in total.
    index_count: u32,
}

/// One drawable decal range inside a chunk.
///
/// Mirrors the neutral `StaticBatch` shape: a chunk plus an index range, the
/// range's cull bounds and its sheet. The sheet is the neutral key's material,
/// which for a decal is the sheet index
/// ([`crate::render::common::decal_sheet_index`]): the generated atlas or one
/// external PNG.
struct DecalDraw {
    /// Which [`DecalChunk`] the range lives in.
    chunk: usize,
    /// First index in the chunk's index buffer.
    index_start: u32,
    /// Indices the draw covers.
    index_count: u32,
    /// Distinct vertices the range indexes.
    vertex_count: u32,
    /// World-space bounds the CPU frustum test uses.
    bounds: Aabb,
    /// The decal sheet index this range samples.
    sheet: MaterialIndex,
}

/// What one upload produced, as plain counters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DecalGpuStats {
    /// Draw ranges the upload produced.
    pub draws: usize,
    /// GPU buffer pairs the upload produced.
    pub chunks: usize,
    /// Distinct vertices uploaded.
    pub vertices: usize,
    /// Indices uploaded.
    pub indices: usize,
    /// Bytes resident in decal vertex buffers.
    pub vertex_bytes: usize,
    /// Bytes resident in decal index buffers.
    pub index_bytes: usize,
    /// Decal sheets uploaded (the generated atlas included).
    pub sheets: usize,
    /// External PNG sheets among them.
    pub external_sheets: usize,
    /// Sheets uploaded as the magenta/black diagnostic.
    pub diagnostic_sheets: usize,
    /// Texel storage every sheet occupies, mip levels included.
    pub texture_bytes: usize,
}

/// What one frame's decal submission did, for the neutral counters.
///
/// The world's `WorldDrawTotals` without the material binds: a decal draw binds
/// a sheet, never a resolved material.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DecalDrawTotals {
    /// `draw_indexed` calls issued.
    pub draw_calls: usize,
    /// Ranges that survived the frustum test.
    pub visible_batches: usize,
    /// Distinct vertices those ranges index.
    pub visible_vertices: usize,
    /// Sheet bind-group changes applied (one per run of draws sharing a sheet).
    pub texture_binds: usize,
}

/// The resources and frame state one decal submission needs.
#[derive(Clone, Copy)]
pub struct DecalEncodeInputs<'a> {
    /// The player's filtering setting, selecting the sampler at bind time.
    pub filtering: TextureFiltering,
    /// The prepared frame (matrices and frustum).
    pub frame: &'a WorldFrame,
    /// Whether the CPU frustum test runs.
    pub cull: bool,
}

/// The decal pass pipeline: the reference's decal program, its own camera
/// uniform and the sheet group's layout.
///
/// Created once per colour target format. The camera buffer and bind group are
/// format-independent and persist for the pipeline's lifetime; the camera is
/// written only when the uploaded uniform changed, exactly like
/// [`super::world::WorldPipeline`]. The sheet bind group layout is owned here
/// and handed to [`WgpuDecals::upload`], so the uploaded sheets and the
/// pipeline always agree on group 1.
pub struct DecalPipeline {
    pipeline: wgpu::RenderPipeline,
    sheet_layout: wgpu::BindGroupLayout,
    camera_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    /// The last uploaded camera state, so a still camera writes nothing.
    uploaded: Option<CameraUniform>,
}

/// The camera bind group layout: one frame uniform at binding 0.
///
/// The layout itself is not kept: the pipeline layout and the bind group both
/// hold what they need for their lifetime.
fn camera_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("places-wgpu-decal-camera-layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            // Only the vertex stage transforms by the camera; the decal
            // fragment stage has no view-dependent term.
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: wgpu::BufferSize::new(CAMERA_UNIFORM_SIZE),
            },
            count: None,
        }],
    })
}

/// The sheet bind group layout: one texture + sampler pair at binding 0/1.
fn sheet_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("places-wgpu-decal-sheet-layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    })
}

/// Builds the one decal pipeline state.
///
/// The reference's decal program plus its raster state: `LessEqual` depth with
/// writes on, [`DECAL_DEPTH_BIAS`], no culling (`GL_CULL_FACE` is never
/// enabled), no blending and the shared world vertex layout.
fn build_decal_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    pipeline_layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("places-wgpu-decal"),
        layout: Some(pipeline_layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some(DECAL_VERTEX_ENTRY),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            // The shared 8-attribute world layout, exactly like the reference
            // reusing its world vertex stage.
            buffers: &[Some(world_vertex_layout())],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            // The reference never enables `GL_CULL_FACE`; a decal quad is
            // authored facing its surface and must draw from either side.
            cull_mode: None,
            unclipped_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            // The reference keeps depth writes on during the decal pass.
            depth_write_enabled: Some(true),
            // The reference's `glDepthFunc(GL_LEQUAL)`.
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: wgpu::StencilState::default(),
            bias: DECAL_DEPTH_BIAS,
        }),
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(if format.is_srgb() {
                DECAL_FRAGMENT_ENTRY
            } else {
                DECAL_FRAGMENT_ENTRY_RAW
            }),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                // Decals are opaque-with-discard: the reference never enables
                // blending for them.
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

impl DecalPipeline {
    /// Builds the decal pipeline for one colour target format.
    #[must_use]
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("places-wgpu-decal-shader"),
            source: wgpu::ShaderSource::Wgsl(DECAL_SHADER_SRC.into()),
        });
        let camera_layout = camera_bind_group_layout(device);
        let sheet_layout = sheet_bind_group_layout(device);
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("places-wgpu-decal-pipeline-layout"),
            bind_group_layouts: &[Some(&camera_layout), Some(&sheet_layout)],
            immediate_size: 0,
        });
        let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("places-wgpu-decal-camera"),
            size: CAMERA_UNIFORM_SIZE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("places-wgpu-decal-camera"),
            layout: &camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });
        let pipeline = build_decal_pipeline(device, format, &pipeline_layout, &shader);
        Self {
            pipeline,
            sheet_layout,
            camera_buffer,
            bind_group,
            uploaded: None,
        }
    }

    /// The group-1 layout every decal sheet bind group is created against.
    ///
    /// [`WgpuDecals::upload`] takes this so a sheet and the pipeline that reads
    /// it cannot disagree about the binding.
    #[must_use]
    pub const fn sheet_layout(&self) -> &wgpu::BindGroupLayout {
        &self.sheet_layout
    }

    /// Uploads the frame's corrected view-projection and eye position when they
    /// changed.
    ///
    /// The same one-uniform, compare-before-write contract as
    /// [`super::world::WorldPipeline::upload_camera`]: a still camera never
    /// touches the buffer, and any change to either the matrix or the eye
    /// reaches the shader.
    pub fn upload_camera(&mut self, queue: &wgpu::Queue, view_projection: Mat4, eye: glam::Vec3) {
        let uniform = CameraUniform::new(view_projection, eye);
        if !camera_uniform_changed(self.uploaded, uniform) {
            return;
        }
        queue.write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&uniform));
        self.uploaded = Some(uniform);
    }
}

/// One uploaded decal sheet: the texture, its view and one bind group per
/// filtering mode.
struct DecalSheet {
    /// Kept for ownership; the bind groups reference it.
    _texture: wgpu::Texture,
    /// Kept for ownership; the bind groups were built from it.
    _view: wgpu::TextureView,
    /// Texture + the repeating linear sampler.
    linear: wgpu::BindGroup,
    /// Texture + the repeating nearest sampler.
    nearest: wgpu::BindGroup,
    /// Texel storage, every mip level included.
    resident_bytes: usize,
}

impl DecalSheet {
    /// The bind group for one filtering mode.
    const fn bind_group(&self, filtering: TextureFiltering) -> &wgpu::BindGroup {
        match filtering {
            TextureFiltering::Linear => &self.linear,
            TextureFiltering::Nearest => &self.nearest,
        }
    }
}

/// The inputs the decal upload needs from the loaded level.
///
/// Grouped so the call site cannot mix one level's mesh with another's decal
/// ids, exactly like the world material upload's inputs.
#[derive(Clone, Copy)]
pub struct DecalUploadInputs<'a> {
    /// The renderer-neutral static mesh; its `Decal` ranges are packed.
    pub mesh: &'a LevelMesh,
    /// The loaded level, for its decal ids and first-use order.
    pub level: &'a LevelDef,
    /// The catalog that resolves an external decal id to its PNG.
    pub catalog: &'a AssetCatalog,
    /// The asset root the external PNGs are read from.
    pub asset_root: Option<&'a Path>,
}

/// The level's decal geometry and textures, created once per level upload.
///
/// One geometry upload, never a per-frame rebuild; dropping the value releases
/// every GPU buffer and sheet.
pub struct WgpuDecals {
    chunks: Vec<DecalChunk>,
    draws: Vec<DecalDraw>,
    sheets: Vec<DecalSheet>,
    /// How many external sheets follow the generated atlas.
    external_sheets: usize,
    stats: DecalGpuStats,
}

impl WgpuDecals {
    /// Packs the level's decal ranges and uploads every sheet they sample.
    ///
    /// Sheet 0 is the generated atlas ([`generate_decal_atlas`], always
    /// resident); each external id the level places follows in
    /// [`decal_external_sheet_ids`] order and is decoded through the same
    /// catalog lookup the texture pipeline uses. A sheet that cannot resolve
    /// draws the same magenta/black diagnostic the reference draws and reports
    /// the same one-line warning; the mapping keeps its place either way, so a
    /// later sheet's index never shifts.
    #[must_use]
    pub fn upload(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        sheet_layout: &wgpu::BindGroupLayout,
        textures: &TextureCache,
        inputs: DecalUploadInputs<'_>,
        profile: QualityProfile,
    ) -> Self {
        let (packer, draws) = pack_decal_ranges(inputs.mesh);
        let mut chunks: Vec<DecalChunk> = Vec::with_capacity(packer.chunks.len());
        for chunk in &packer.chunks {
            chunks.push(upload_chunk(device, queue, chunk));
        }
        let uploaded = upload_sheets(device, queue, sheet_layout, textures, inputs, profile);
        let vertices: usize = chunks
            .iter()
            .map(|chunk| usize::try_from(chunk.vertex_count).unwrap_or(usize::MAX))
            .sum();
        let indices: usize = chunks
            .iter()
            .map(|chunk| usize::try_from(chunk.index_count).unwrap_or(usize::MAX))
            .sum();
        let stats = DecalGpuStats {
            draws: draws.len(),
            chunks: chunks.len(),
            vertices,
            indices,
            vertex_bytes: vertices
                .saturating_mul(usize::try_from(WORLD_VERTEX_STRIDE).unwrap_or(usize::MAX)),
            index_bytes: indices.saturating_mul(std::mem::size_of::<u16>()),
            sheets: uploaded.sheets.len(),
            external_sheets: uploaded.external,
            diagnostic_sheets: uploaded.diagnostics,
            texture_bytes: uploaded
                .sheets
                .iter()
                .map(|sheet| sheet.resident_bytes)
                .sum(),
        };
        Self {
            chunks,
            draws,
            sheets: uploaded.sheets,
            external_sheets: uploaded.external,
            stats,
        }
    }

    /// True when the level has no decal geometry to submit.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.draws.is_empty() || self.chunks.is_empty()
    }

    /// The upload counters.
    #[must_use]
    pub const fn stats(&self) -> DecalGpuStats {
        self.stats
    }

    /// Draws the decal pass into `pass`.
    ///
    /// The caller opens the pass on the same colour + depth targets the scene
    /// body used; decals run last, with the depth test on, exactly like the
    /// reference's `draw_decal_batches`. Within the pass the packed draw order
    /// is kept and vertex/index buffers and sheets are rebound only when a run
    /// of draws changes them.
    #[must_use]
    pub fn encode<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        pipeline: &'a DecalPipeline,
        inputs: DecalEncodeInputs<'a>,
    ) -> DecalDrawTotals {
        let mut totals = DecalDrawTotals::default();
        if self.is_empty() {
            return totals;
        }
        pass.set_pipeline(&pipeline.pipeline);
        pass.set_bind_group(0, &pipeline.bind_group, &[]);
        let mut bound_chunk: Option<usize> = None;
        let mut bound_sheet: Option<usize> = None;
        for draw in &self.draws {
            if draw.index_count == 0 {
                continue;
            }
            if inputs.cull && !inputs.frame.frustum.intersects_aabb(&draw.bounds) {
                continue;
            }
            let slot = sheet_slot_for(draw.sheet, self.external_sheets);
            if bound_sheet != Some(slot) {
                let Some(sheet) = self.sheets.get(slot).or_else(|| self.sheets.first()) else {
                    continue;
                };
                pass.set_bind_group(1, sheet.bind_group(inputs.filtering), &[]);
                totals.texture_binds = totals.texture_binds.saturating_add(1);
                bound_sheet = Some(slot);
            }
            if bound_chunk != Some(draw.chunk) {
                let Some(chunk) = self.chunks.get(draw.chunk) else {
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

/// Packs the static mesh's decal ranges, one draw per packed chunk split.
///
/// The rule is the neutral builder's own: a range is a decal when its key's
/// kind is [`SurfaceKind::Decal`], and its key's material is the sheet index.
/// GPU-free, so the draw set, its split behaviour and its counters are
/// unit-testable exactly like [`super::world::pack_world_ranges`]; an empty or
/// decal-less mesh is a valid level with nothing to draw.
#[must_use]
fn pack_decal_ranges(mesh: &LevelMesh) -> (MeshPacker, Vec<DecalDraw>) {
    let mut packer = MeshPacker::default();
    let mut draws: Vec<DecalDraw> = Vec::new();
    for range in &mesh.ranges {
        if range.key.kind != SurfaceKind::Decal {
            continue;
        }
        for packed in packer.push(&range.vertices, &range.indices) {
            draws.push(DecalDraw {
                chunk: packed.chunk,
                index_start: u32::try_from(packed.index_start).unwrap_or(u32::MAX),
                index_count: u32::try_from(packed.index_count).unwrap_or(u32::MAX),
                vertex_count: u32::try_from(packed.vertex_count).unwrap_or(u32::MAX),
                bounds: range.bounds,
                sheet: range.key.material,
            });
        }
    }
    (packer, draws)
}

/// GPU vertices for one chunk, through the shared world conversion.
///
/// Kept separate from the buffer upload so the position/UV/colour carry is
/// unit-testable without a device.
fn world_vertices(vertices: &[Vertex]) -> Vec<WorldVertex> {
    vertices.iter().map(WorldVertex::from).collect()
}

/// Uploads one packed chunk as a vertex/index buffer pair.
fn upload_chunk(device: &wgpu::Device, queue: &wgpu::Queue, chunk: &MeshChunk) -> DecalChunk {
    let vertices = world_vertices(&chunk.vertices);
    let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("places-wgpu-decal-vertices"),
        size: (vertices.len() as u64)
            .saturating_mul(WORLD_VERTEX_STRIDE)
            .max(4),
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    if !vertices.is_empty() {
        queue.write_buffer(&vertex_buffer, 0, bytemuck::cast_slice(&vertices));
    }

    let index_bytes = chunk
        .indices
        .len()
        .saturating_mul(std::mem::size_of::<u16>());
    let index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("places-wgpu-decal-indices"),
        size: (index_bytes as u64).max(4),
        usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    if !chunk.indices.is_empty() {
        queue.write_buffer(&index_buffer, 0, bytemuck::cast_slice(&chunk.indices));
    }

    DecalChunk {
        vertex_buffer,
        index_buffer,
        vertex_count: u32::try_from(chunk.vertices.len()).unwrap_or(u32::MAX),
        index_count: u32::try_from(chunk.indices.len()).unwrap_or(u32::MAX),
    }
}

/// What one sheet upload produced.
struct UploadedSheets {
    sheets: Vec<DecalSheet>,
    /// External sheets that follow the generated atlas.
    external: usize,
    /// Sheets that fell back to the diagnostic image.
    diagnostics: usize,
}

/// Uploads the generated atlas plus every external sheet the level places.
///
/// The whole lookup mirrors the reference's `load_decal_sheets`: the same
/// [`decal_external_sheet_ids`] order, the same `resolve_decal_sheet` catalog
/// decode, the same diagnostic fallback and warning for a broken sheet, and the
/// same `TextureClass::DecalSheet` fit before upload. Decoded images are cached
/// for the duration of one upload through the neutral decode cache.
fn upload_sheets(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    sheet_layout: &wgpu::BindGroupLayout,
    textures: &TextureCache,
    inputs: DecalUploadInputs<'_>,
    profile: QualityProfile,
) -> UploadedSheets {
    let mut sheets: Vec<DecalSheet> = Vec::new();
    // The generated atlas is a fixed 256x256 sheet and always resident, exactly
    // as the reference creates it at startup; it is under both profiles'
    // `DecalSheet` budget by contract.
    let atlas_size = u32::try_from(DECAL_ATLAS_SIZE).unwrap_or(0);
    let atlas = RawImage::new(atlas_size, atlas_size, generate_decal_atlas());
    sheets.push(upload_sheet(device, queue, sheet_layout, textures, &atlas));

    let ids = decal_external_sheet_ids(inputs.level, inputs.catalog);
    let mut decode_cache = crate::materials::TextureCache::new();
    let mut diagnostics = 0usize;
    for id in &ids {
        let image = match crate::materials::resolve_decal_sheet(
            inputs.catalog,
            inputs.asset_root,
            &mut decode_cache,
            id,
        ) {
            Ok(sheet) => sheet.image,
            Err(error) => {
                crate::logging::warn_once(
                    format!("decal-sheet:{id}"),
                    format!("[decals] {error}; drawing the diagnostic sheet instead"),
                );
                diagnostics = diagnostics.saturating_add(1);
                Rc::new(crate::materials::missing_texture())
            }
        };
        let fitted = fit_image(image.as_ref(), profile, TextureClass::DecalSheet);
        // The diagnostic image uploads through the same path as authored
        // artwork, exactly like the reference; only the counters tell them
        // apart.
        sheets.push(upload_sheet(
            device,
            queue,
            sheet_layout,
            textures,
            fitted.as_ref(),
        ));
    }
    UploadedSheets {
        sheets,
        external: ids.len(),
        diagnostics,
    }
}

/// Uploads one sheet with a CPU mip chain and both filtering bind groups.
///
/// The mip chain is the same deterministic 2x2 box filter `texture.rs` applies
/// to the Stage 6 sheets (the two helpers are private there and duplicated
/// here, pinned by the tests to the same arithmetic): average the raw 8-bit
/// channels without a gamma step, stopping at 1x1.
fn upload_sheet(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    sheet_layout: &wgpu::BindGroupLayout,
    textures: &TextureCache,
    image: &RawImage,
) -> DecalSheet {
    let width = image.width.max(1);
    let height = image.height.max(1);
    let mip_levels = mip_levels(width, height);
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("places-wgpu-decal-sheet"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: mip_levels,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DECAL_SHEET_FORMAT,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut resident_bytes = level_bytes(width, height);
    write_mip(queue, &texture, 0, image);
    let mut current = image.clone();
    for level in 1..mip_levels {
        current = halve(&current);
        resident_bytes = resident_bytes.saturating_add(level_bytes(current.width, current.height));
        write_mip(queue, &texture, level, &current);
    }
    let linear = create_sheet_bind_group(
        device,
        sheet_layout,
        &view,
        textures,
        TextureFiltering::Linear,
    );
    let nearest = create_sheet_bind_group(
        device,
        sheet_layout,
        &view,
        textures,
        TextureFiltering::Nearest,
    );
    DecalSheet {
        _texture: texture,
        _view: view,
        linear,
        nearest,
        resident_bytes,
    }
}

/// Creates the texture + sampler bind group for one filtering mode.
///
/// The sampler comes from the shared texture cache's repeating pair, so a decal
/// sheet is read with exactly the policy a world sheet is: REPEAT addressing
/// with linear or nearest mip filtering, per the player's setting.
fn create_sheet_bind_group(
    device: &wgpu::Device,
    sheet_layout: &wgpu::BindGroupLayout,
    view: &wgpu::TextureView,
    textures: &TextureCache,
    filtering: TextureFiltering,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("places-wgpu-decal-sheet"),
        layout: sheet_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(
                    textures.sampler(filtering.sampler_policy()),
                ),
            },
        ],
    })
}

/// The sheet slot one decal draw's material index addresses.
///
/// The reference's `decal_sheet_texture` mapping: an index below
/// [`DECAL_EXTERNAL_BASE`] (and any stale index the level no longer has a sheet
/// for, including the empty-material sentinel) addresses the generated atlas at
/// slot 0; otherwise the index addresses the external sheet at that offset.
fn sheet_slot_for(material: MaterialIndex, external: usize) -> usize {
    let index = u32::from(material);
    if index < DECAL_EXTERNAL_BASE {
        return 0;
    }
    let offset = usize::try_from(index.saturating_sub(DECAL_EXTERNAL_BASE)).unwrap_or(usize::MAX);
    if offset < external {
        offset.saturating_add(1)
    } else {
        0
    }
}

/// Mip levels a full chain of this image holds; the same rule as `texture.rs`.
const fn mip_levels(width: u32, height: u32) -> u32 {
    let longest = if width > height { width } else { height };
    if longest == 0 {
        return 1;
    }
    u32::BITS.saturating_sub(longest.leading_zeros())
}

/// Bytes one RGBA8 level of these dimensions occupies.
fn level_bytes(width: u32, height: u32) -> usize {
    usize::try_from(
        u64::from(width)
            .saturating_mul(u64::from(height))
            .saturating_mul(4),
    )
    .unwrap_or(usize::MAX)
}

/// Writes one mip level through the queue.
///
/// `Queue::write_texture` accepts tightly packed rows, so no 256-byte row
/// padding is applied. An image with a zero edge cannot describe a copy and is
/// skipped (no caller produces one; every sheet is a generated, decoded or
/// diagnostic image with both edges non-zero).
fn write_mip(queue: &wgpu::Queue, texture: &wgpu::Texture, level: u32, image: &RawImage) {
    if image.width == 0 || image.height == 0 {
        return;
    }
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: level,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &image.rgba,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(image.width.saturating_mul(4)),
            rows_per_image: Some(image.height),
        },
        wgpu::Extent3d {
            width: image.width,
            height: image.height,
            depth_or_array_layers: 1,
        },
    );
}

/// Box-filters an image to half its size, exactly like `texture.rs`.
///
/// Deterministic and display-space: the raw 8-bit channels are averaged without
/// a gamma conversion, which is what the reference's `glGenerateMipmap` did to
/// its non-sRGB decal sheet. Odd dimensions follow the same clamped block
/// bounds `texture.rs` uses.
fn halve(source: &RawImage) -> RawImage {
    let width = (source.width / 2).max(1);
    let height = (source.height / 2).max(1);
    let Some(length) = buffer_len(width, height) else {
        return RawImage::new(1, 1, vec![255, 255, 255, 255]);
    };
    let mut rgba = vec![0u8; length];
    for out_y in 0..height {
        let y0 = out_y.saturating_mul(2);
        let y1 = y0.saturating_add(2).min(source.height.max(1));
        for out_x in 0..width {
            let x0 = out_x.saturating_mul(2);
            let x1 = x0.saturating_add(2).min(source.width.max(1));
            let mut sums = [0u32; 4];
            let mut count = 0u32;
            for y in y0..y1 {
                for x in x0..x1 {
                    let Some(texel) = texel(source, x, y) else {
                        continue;
                    };
                    for (sum, channel) in sums.iter_mut().zip(texel) {
                        *sum = sum.saturating_add(u32::from(channel));
                    }
                    count = count.saturating_add(1);
                }
            }
            if count == 0 {
                continue;
            }
            let Some(offset) = texel_offset(out_x, out_y, width) else {
                continue;
            };
            for (index, sum) in sums.iter().enumerate() {
                let rounded = sum
                    .saturating_add(count / 2)
                    .checked_div(count)
                    .unwrap_or(0)
                    .min(u32::from(u8::MAX));
                let value = u8::try_from(rounded).unwrap_or(u8::MAX);
                if let Some(slot) = rgba.get_mut(offset.saturating_add(index)) {
                    *slot = value;
                }
            }
        }
    }
    RawImage::new(width, height, rgba)
}

/// Byte length of an RGBA8 buffer, or `None` on overflow.
fn buffer_len(width: u32, height: u32) -> Option<usize> {
    usize::try_from(width)
        .ok()?
        .checked_mul(usize::try_from(height).ok()?)?
        .checked_mul(std::mem::size_of::<u32>())
}

/// Byte offset of texel `(x, y)` in a tightly packed RGBA8 buffer.
fn texel_offset(x: u32, y: u32, width: u32) -> Option<usize> {
    let row = usize::try_from(y)
        .ok()?
        .checked_mul(usize::try_from(width).ok()?)?
        .checked_mul(std::mem::size_of::<u32>())?;
    let column = usize::try_from(x)
        .ok()?
        .checked_mul(std::mem::size_of::<u32>())?;
    row.checked_add(column)
}

/// The four channels of one texel, or `None` outside the image.
fn texel(source: &RawImage, x: u32, y: u32) -> Option<[u8; 4]> {
    let offset = texel_offset(x, y, source.width)?;
    let end = offset.checked_add(std::mem::size_of::<u32>())?;
    let slice = source.rgba.get(offset..end)?;
    <[u8; 4]>::try_from(slice).ok()
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
        clippy::unwrap_used
    )]

    use super::*;
    use crate::render::DECAL_ALPHA_CUTOFF;
    use crate::render::common::decals::{decal_uv_rect, decal_uv_rect_full};
    use crate::render::common::dequantize_unit;
    use crate::render::common::mesh::{
        LIGHTMAP_NONE, LevelMeshBatches, LevelMeshRange, MATERIAL_NONE, SurfaceKey,
    };

    /// One decal quad as the neutral emitter writes it: `p0 p1 p2` and
    /// `p0 p2 p3`, with the sheet's UV rect in its four corners.
    fn decal_range(sheet: MaterialIndex, uv: [[f32; 2]; 4]) -> LevelMeshRange {
        let points = [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
        ];
        let mut vertices = Vec::with_capacity(6);
        for index in [0usize, 1, 2, 0, 2, 3] {
            vertices.push(Vertex::new(points[index], [0.5, 0.25, 1.0, 1.0], uv[index]));
        }
        LevelMeshRange {
            key: SurfaceKey::new(SurfaceKind::Decal, sheet),
            vertices,
            indices: vec![0, 1, 2, 3, 4, 5],
            bounds: Aabb {
                min: [-1.0, -1.0, -1.0],
                max: [2.0, 2.0, 2.0],
            },
        }
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

    /// One non-decal range, to prove the packer skips it.
    fn floor_range() -> LevelMeshRange {
        let mut range = decal_range(1, decal_uv_rect_full());
        range.key = SurfaceKey::new(SurfaceKind::Floor, 7);
        range
    }

    // ---------------------------------------------------------- depth bias

    #[test]
    fn the_depth_bias_maps_gl_polygon_offset_towards_the_camera() {
        let bias = DECAL_DEPTH_BIAS;
        // The reference's `glPolygonOffset(-1.0, -4.0)`: a slope-scaled term of
        // one and a constant term of four depth-buffer steps, both negative.
        assert_eq!(bias.constant, -4);
        assert_eq!(bias.slope_scale, -1.0);
        assert_eq!(bias.clamp, 0.0);
        assert!(
            bias.constant < 0 && bias.slope_scale < 0.0,
            "both terms must pull the decal towards the camera, got {bias:?}"
        );

        // The mapping swaps the APIs' names: GL `factor` (slope) -> wgpu
        // `slope_scale`, GL `units` (constant) -> wgpu `constant`.
        let mapped = depth_bias_state(-2.5, -9);
        assert_eq!(mapped.constant, -9);
        assert_eq!(mapped.slope_scale, -2.5);
        assert_eq!(mapped.clamp, 0.0);
    }

    // ------------------------------------------------------------ shader

    #[test]
    fn the_decal_fragment_tests_texture_alpha_and_converts_the_product_once() {
        assert!(DECAL_SHADER_SRC.contains("fn srgb_to_linear"));
        assert!(DECAL_SHADER_SRC.contains("fn linear_to_srgb"));
        // The reference tests `tex_color.a` alone, never the output alpha.
        assert!(DECAL_SHADER_SRC.contains("if (base.a < 0.5)"));
        assert!(!DECAL_SHADER_SRC.contains("if (alpha <"));
        assert!(DECAL_SHADER_SRC.contains("discard"));
        // The display-space product is converted exactly once, for the sRGB
        // target.
        assert!(DECAL_SHADER_SRC.contains("srgb_to_linear(base.rgb * in.color.rgb)"));
        assert!((0.0..1.0).contains(&DECAL_ALPHA_CUTOFF));
        // The shader's cut-off and the Rust constant cannot drift apart.
        assert!(DECAL_SHADER_SRC.contains(&format!("< {DECAL_ALPHA_CUTOFF}")));
    }

    #[test]
    fn the_decal_shader_declares_the_camera_and_the_sheet_bindings() {
        assert!(DECAL_SHADER_SRC.contains("@group(0) @binding(0)"));
        assert!(DECAL_SHADER_SRC.contains("var<uniform> camera: Camera"));
        assert!(DECAL_SHADER_SRC.contains("var decal_texture: texture_2d<f32>"));
        assert!(DECAL_SHADER_SRC.contains("var decal_sampler: sampler"));
        assert!(DECAL_SHADER_SRC.contains("fn vs_main"));
        assert!(DECAL_SHADER_SRC.contains("fn fs_main"));
    }

    #[test]
    fn decal_sheets_sample_display_space_bytes_like_the_reference() {
        // The reference's decal sheets are non-sRGB `GL_RGBA`; an sRGB texel
        // format would pre-decode the sheet and darken the multiply.
        assert_eq!(DECAL_SHEET_FORMAT, wgpu::TextureFormat::Rgba8Unorm);
        assert!(!DECAL_SHEET_FORMAT.is_srgb());
    }

    // -------------------------------------------------------------- sheets

    #[test]
    fn sheet_indices_address_the_atlas_then_each_external_sheet() {
        let external_base = MaterialIndex::try_from(DECAL_EXTERNAL_BASE).unwrap();
        assert_eq!(sheet_slot_for(0, 0), 0);
        assert_eq!(sheet_slot_for(0, 3), 0);
        assert_eq!(sheet_slot_for(external_base, 2), 1);
        assert_eq!(sheet_slot_for(external_base.saturating_add(1), 2), 2);
        // A stale index past the level's sheets falls back to the atlas,
        // exactly like the reference's `decal_sheet_texture`.
        assert_eq!(sheet_slot_for(99, 1), 0);
        assert_eq!(sheet_slot_for(MATERIAL_NONE, 0), 0);
    }

    /// The sheet-space UV rectangle a decal of `sheet` samples.
    ///
    /// The neutral builder already baked this rect into the quad's vertices
    /// (`emit_decals`): an index below [`DECAL_EXTERNAL_BASE`] is a generated
    /// atlas cell and samples its sub-rect via [`decal_uv_rect`]; the base and
    /// above are external sheets and sample the whole image via
    /// [`decal_uv_rect_full`] (both in-plane axes swapped, because the upload's
    /// row order runs opposite the decal frame's V axis). The wgpu pass only
    /// converts vertices, so the tests keep this mirror of the builder's
    /// mapping and pin the two together.
    fn decal_uv_rect_for_sheet(sheet: MaterialIndex) -> [[f32; 2]; 4] {
        if u32::from(sheet) < DECAL_EXTERNAL_BASE {
            decal_uv_rect(u32::from(sheet))
        } else {
            decal_uv_rect_full()
        }
    }

    #[test]
    fn a_range_samples_its_atlas_cell_or_the_whole_external_sheet() {
        // The neutral builder's own selection (`emit_decals`): the generated
        // atlas cell (index 0, below the one-entry external base) samples its
        // sub-rect, every index at or above the base samples the whole sheet.
        assert_eq!(DECAL_EXTERNAL_BASE, 1);
        assert_eq!(decal_uv_rect_for_sheet(0), decal_uv_rect(0));
        let external_base = MaterialIndex::try_from(DECAL_EXTERNAL_BASE).unwrap();
        assert_eq!(decal_uv_rect_for_sheet(external_base), decal_uv_rect_full());
        assert_eq!(decal_uv_rect_for_sheet(3), decal_uv_rect_full());
        assert_eq!(
            decal_uv_rect_for_sheet(external_base.saturating_add(7)),
            decal_uv_rect_full()
        );

        // And the quad's four corner UVs are exactly that rect, in the
        // emitter's winding order.
        let generated = decal_range(0, decal_uv_rect(0));
        let rect = decal_uv_rect_for_sheet(generated.key.material);
        for (index, vertex) in generated.vertices.iter().enumerate() {
            let corner = [0usize, 1, 2, 0, 2, 3][index];
            assert_eq!(vertex.uv, rect[corner]);
        }
        let external = decal_range(external_base, decal_uv_rect_full());
        let rect = decal_uv_rect_for_sheet(external.key.material);
        assert_eq!(rect, decal_uv_rect_full());
    }

    #[test]
    fn the_generated_atlas_fits_every_profile_budget() {
        let edge = u32::try_from(DECAL_ATLAS_SIZE).unwrap();
        for profile in QualityProfile::ALL {
            assert!(
                edge <= profile.budget(TextureClass::DecalSheet),
                "the {edge}px atlas must upload unchanged under {}",
                profile.name()
            );
        }
    }

    // -------------------------------------------------------------- upload

    #[test]
    fn an_empty_or_decal_less_mesh_packs_no_draws_and_no_chunks() {
        let (packer, draws) = pack_decal_ranges(&mesh(Vec::new()));
        assert!(packer.chunks.is_empty());
        assert!(draws.is_empty());
        assert_eq!(packer.vertex_total(), 0);

        let (packer, draws) = pack_decal_ranges(&mesh(vec![floor_range()]));
        assert!(packer.chunks.is_empty());
        assert!(draws.is_empty());
    }

    #[test]
    fn a_decal_range_packs_one_draw_with_its_sheet_and_bounds() {
        let range = decal_range(2, decal_uv_rect_full());
        let bounds = range.bounds;
        let expected_vertices = range.vertices.len();
        let expected_indices = range.indices.len();
        let (packer, draws) = pack_decal_ranges(&mesh(vec![range]));
        assert_eq!(packer.chunks.len(), 1);
        assert_eq!(draws.len(), 1);
        let draw = draws.first().unwrap();
        assert_eq!(draw.chunk, 0);
        assert_eq!(draw.index_start, 0);
        assert_eq!(draw.index_count, u32::try_from(expected_indices).unwrap());
        assert_eq!(draw.vertex_count, u32::try_from(expected_vertices).unwrap());
        assert_eq!(draw.sheet, 2);
        assert_eq!(draw.bounds, bounds);
        assert_eq!(packer.vertex_total(), expected_vertices);
        assert_eq!(packer.index_total(), expected_indices);
    }

    // ------------------------------------------------------------- vertices

    #[test]
    fn decal_vertices_reach_the_gpu_layout_field_for_field() {
        let vertex = Vertex::new([1.5, 2.25, -3.5], [0.5, 0.0, 1.0, 1.0], [0.25, 0.75]);
        let gpu = WorldVertex::from(&vertex);
        assert_eq!(gpu.position, [1.5, 2.25, -3.5]);
        assert_eq!(gpu.uv, [0.25, 0.75]);
        // The shared quantiser and the decal's constant vertex alpha.
        assert_eq!(gpu.color, [128, 0, 255, 255]);
        assert_eq!(gpu.lightmap_page, f32::from(LIGHTMAP_NONE));
        assert_eq!(dequantize_unit(gpu.color[0]), 128.0 / 255.0);

        let converted = world_vertices(&[vertex]);
        assert_eq!(converted, vec![gpu]);
        assert_eq!(
            usize::try_from(WORLD_VERTEX_STRIDE).unwrap(),
            std::mem::size_of::<WorldVertex>()
        );
        assert_eq!(world_vertex_layout().attributes.len(), 8);
    }

    // --------------------------------------------------------------- mips

    #[test]
    fn the_mip_chain_stops_at_one_by_one_and_halves_every_level() {
        assert_eq!(mip_levels(256, 256), 9);
        assert_eq!(mip_levels(1, 1), 1);
        assert_eq!(mip_levels(96, 64), 7);
        assert_eq!(mip_levels(3, 3), 2);

        let image = RawImage::new(2, 2, vec![0, 0, 0, 0, 2, 2, 2, 2, 4, 4, 4, 4, 6, 6, 6, 6]);
        let half = halve(&image);
        assert_eq!((half.width, half.height), (1, 1));
        assert_eq!(half.rgba, vec![3, 3, 3, 3]);

        // An odd source edge follows the same block rule `texture.rs` uses: a
        // 3x1 image halves to one texel averaging its first two columns.
        let odd = RawImage::new(3, 1, vec![0, 0, 0, 0, 4, 4, 4, 4, 8, 8, 8, 8]);
        let half = halve(&odd);
        assert_eq!((half.width, half.height), (1, 1));
        assert_eq!(half.rgba, vec![2, 2, 2, 2]);
    }
}
