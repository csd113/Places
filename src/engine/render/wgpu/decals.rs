//! Decals: the reference's local surface markings.
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
//!   level so the player's setting chooses the sampler at encode time;
//! * the [`DecalPipeline`]: the reference's decal program, `LessEqual` depth
//!   with writes on, [`DECAL_DEPTH_BIAS`], no culling and no blending;
//! * [`WgpuDecals::encode`], one draw per range surviving the frame's frustum
//!   test, submitted after the translucent pass and never in the emissive body.
//!
//! Authored colour uploads as sRGB [`DECAL_SHEET_FORMAT`], so sampling decodes
//! it before linear shading. Quality reduction and the CPU mip chain both
//! average in linear light, weighting RGB by alpha coverage.
//!
//! The neutral geometry already lifts a decal [`crate::engine::render::common::DECAL_SURFACE_OFFSET_M`]
//! along its surface normal; that near-field separation and this pass's
//! depth-buffer bias are the two halves of one contract and neither replaces
//! the other.

use std::path::Path;
use std::sync::Arc;

use glam::Mat4;

use super::surface::DEPTH_FORMAT;
use super::texture::{TextureCache, TextureFiltering};
use super::world::{
    CameraUniform, WORLD_VERTEX_STRIDE, WorldFrame, WorldVertex, world_vertex_layout,
};
use crate::engine::assets::AssetCatalog;
use crate::engine::level::LevelDef;
use crate::engine::materials::RawImage;
use crate::engine::quality::{QualityLevel, TextureClass, fit_image};
use crate::engine::render::common::decals::{
    DECAL_ATLAS_SIZE, DECAL_EXTERNAL_BASE, decal_blend_sheets, decal_external_sheet_ids,
    decal_sheet_is_blend, generate_decal_atlas,
};
use crate::engine::render::common::mesh::{LevelMesh, MaterialIndex, SurfaceKind, Vertex};
use crate::engine::render::common::{MeshChunk, MeshPacker};
use crate::engine::spatial::Aabb;

/// The decal shader, from the file next to this module.
pub const DECAL_SHADER_SRC: &str = concat!(
    include_str!("storm.wgsl"),
    "\n",
    include_str!("attached_lights.wgsl"),
    "\n",
    include_str!("decals.wgsl"),
    "\n",
    include_str!("caustics.wgsl")
);

/// Name of the decal vertex entry point.
pub const DECAL_VERTEX_ENTRY: &str = "vs_main";
/// Name of the decal fragment entry point.
pub const DECAL_FRAGMENT_ENTRY: &str = "fs_main";
/// The fragment entry point for a raw (non-sRGB) scene or reflection target:
/// the decal product is display-space, so it is written directly, exactly like
/// the world shader's raw entry points.
pub const DECAL_FRAGMENT_ENTRY_RAW: &str = "fs_main_raw";
/// The blended (soft-edge) fragment entry point.
pub const DECAL_BLEND_ENTRY: &str = "fs_blend";
/// The blended fragment entry point for a raw (non-sRGB) target.
pub const DECAL_BLEND_ENTRY_RAW: &str = "fs_blend_raw";

/// The blended decal pipeline's blend state.
///
/// The same `glBlendFunc(SRC_ALPHA, ONE_MINUS_SRC_ALPHA)` + `FUNC_ADD` contract
/// the world translucent pass uses, written out explicitly so both channels get
/// the reference's factors.
const BLEND_DECAL_STATE: wgpu::BlendState = wgpu::BlendState {
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

/// The format every decal sheet uploads as.
///
/// Authored sRGB colour; sampling decodes RGB before linear shading.
pub const DECAL_SHEET_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

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
/// Holds the chunk plus the index range, the range's cull bounds and its sheet.
/// The sheet is the neutral key's material, which for a decal is the sheet index
/// ([`crate::engine::render::common::decal_sheet_index`]): the generated atlas or one
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
    /// True when the sheet is a soft-edged (blended) decal: drawn after the
    /// cutout decals, back to front, with depth writes off.
    blend: bool,
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
    /// Index elements in the submitted triangle-list draws.
    pub visible_indices: usize,
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
    /// The soft-edged variant: same program and vertex state, blending on and
    /// depth writes off, for decals whose catalog entry authors
    /// `alpha_mode: "blend"`.
    blend_pipeline: wgpu::RenderPipeline,
    sheet_layout: wgpu::BindGroupLayout,
    camera_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    empty_bind_group: wgpu::BindGroup,
    caustics_layout: wgpu::BindGroupLayout,
    caustics_bind_group: wgpu::BindGroup,
    lights_layout: wgpu::BindGroupLayout,
    lights_buffer: wgpu::Buffer,
    lights_bind_group: wgpu::BindGroup,
    lights_sampler: wgpu::Sampler,
    flashlight_shadow_bound: bool,
    /// The last uploaded camera state, so a still camera writes nothing.
    uploaded: Option<DecalCamera>,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
struct DecalCamera {
    camera: CameraUniform,
    storm: crate::engine::render::common::storm::StormUniform,
}
const DECAL_CAMERA_SIZE: u64 = 1648;

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
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: wgpu::BufferSize::new(DECAL_CAMERA_SIZE),
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
///
/// `blend` builds the soft-edge variant of the same program: the same bias and
/// vertex state, but `fs_blend`/`fs_blend_raw` with
/// [`BLEND_DECAL_STATE`] and depth writes off, so a feathered sheet fades
/// instead of testing at the hard 0.5 cut-out. It still tests depth
/// (`LessEqual`) against every opaque surface and against the cut-out decals,
/// so it can never draw through the floor it is printed on.
fn build_decal_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    pipeline_layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    blend: bool,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(if blend {
            "places-wgpu-decal-blend"
        } else {
            "places-wgpu-decal"
        }),
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
            // The reference keeps depth writes on during the cut-out decal
            // pass; a blended decal must not write depth, or every feathered
            // sheet would occlude whatever the scene draws after it.
            depth_write_enabled: Some(!blend),
            // The reference's `glDepthFunc(GL_LEQUAL)`.
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: wgpu::StencilState::default(),
            bias: DECAL_DEPTH_BIAS,
        }),
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(match (blend, format.is_srgb()) {
                (false, true) => DECAL_FRAGMENT_ENTRY,
                (false, false) => DECAL_FRAGMENT_ENTRY_RAW,
                (true, true) => DECAL_BLEND_ENTRY,
                (true, false) => DECAL_BLEND_ENTRY_RAW,
            }),
            compilation_options: wgpu::PipelineCompilationOptions {
                constants: &super::surface::color_target_constants(format),
                ..Default::default()
            },
            targets: &[Some(wgpu::ColorTargetState {
                format,
                // Cut-out decals are opaque-with-discard: the reference never
                // enables blending for them. The soft-edge variant blends.
                blend: if blend { Some(BLEND_DECAL_STATE) } else { None },
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

fn caustics_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    let storage = |binding, size| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: wgpu::BufferSize::new(size),
        },
        count: None,
    };
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("places-decal-caustics-layout"),
        entries: &[
            storage(7, 112),
            wgpu::BindGroupLayoutEntry {
                binding: 8,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 9,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            storage(10, 16),
        ],
    })
}

fn caustics_binding(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    caustics: &super::caustics::Caustics,
    cache: &TextureCache,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("places-decal-caustics"),
        layout,
        entries: &[
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
                resource: wgpu::BindingResource::Sampler(
                    cache.sampler(super::texture::SamplerPolicy::RepeatLow),
                ),
            },
            wgpu::BindGroupEntry {
                binding: 10,
                resource: caustics.sample_buffer.as_entire_binding(),
            },
        ],
    })
}

impl DecalPipeline {
    /// Builds the decal pipeline for one colour target format.
    #[must_use]
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        caustics: &super::caustics::Caustics,
        cache: &TextureCache,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("places-wgpu-decal-shader"),
            source: wgpu::ShaderSource::Wgsl(DECAL_SHADER_SRC.into()),
        });
        let camera_layout = camera_bind_group_layout(device);
        let sheet_layout = sheet_bind_group_layout(device);
        let lights_layout = super::world::dynamic_lights_bind_group_layout(device);
        let empty_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("places-decal-unused-world-groups"),
            entries: &[],
        });
        let empty_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("places-decal-unused-world-groups"),
            layout: &empty_layout,
            entries: &[],
        });
        let caustics_layout = caustics_layout(device);
        let caustics_bind_group = caustics_binding(device, &caustics_layout, caustics, cache);
        let (lights_buffer, lights_bind_group, lights_sampler) =
            super::world::create_dynamic_lights_binding(device, &lights_layout);
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("places-wgpu-decal-pipeline-layout"),
            bind_group_layouts: &[
                Some(&camera_layout),
                Some(&sheet_layout),
                Some(&empty_layout),
                Some(&caustics_layout),
                Some(&lights_layout),
            ],
            immediate_size: 0,
        });
        let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("places-wgpu-decal-camera"),
            size: DECAL_CAMERA_SIZE,
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
        let pipeline = build_decal_pipeline(device, format, &pipeline_layout, &shader, false);
        let blend_pipeline = build_decal_pipeline(device, format, &pipeline_layout, &shader, true);
        Self {
            pipeline,
            blend_pipeline,
            sheet_layout,
            camera_buffer,
            bind_group,
            empty_bind_group,
            caustics_layout,
            caustics_bind_group,
            lights_layout,
            lights_buffer,
            lights_bind_group,
            lights_sampler,
            flashlight_shadow_bound: false,
            uploaded: None,
        }
    }

    pub fn bind_caustics(
        &mut self,
        device: &wgpu::Device,
        caustics: &super::caustics::Caustics,
        cache: &TextureCache,
    ) {
        self.caustics_bind_group = caustics_binding(device, &self.caustics_layout, caustics, cache);
    }

    pub fn bind_flashlight_shadow(&mut self, device: &wgpu::Device, view: &wgpu::TextureView) {
        if self.flashlight_shadow_bound {
            return;
        }
        self.lights_bind_group = super::world::dynamic_lights_binding(
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
        lights: &crate::engine::render::common::DynamicLightSet,
        flashlight: Option<crate::engine::render::common::Flashlight>,
    ) {
        let mut uniform = super::world::DynamicLightsUniform::new(lights);
        uniform.flashlight = super::world::GpuFlashlight::new(flashlight);
        queue.write_buffer(&self.lights_buffer, 0, bytemuck::bytes_of(&uniform));
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
        self.upload_camera_with_storm(
            queue,
            view_projection,
            eye,
            &crate::engine::render::common::storm::StormUniform::default(),
        );
    }

    pub fn upload_camera_with_storm(
        &mut self,
        queue: &wgpu::Queue,
        view_projection: Mat4,
        eye: glam::Vec3,
        storm: &crate::engine::render::common::storm::StormUniform,
    ) {
        let uniform = DecalCamera {
            camera: CameraUniform::new(view_projection, eye),
            storm: *storm,
        };
        if self.uploaded == Some(uniform) {
            return;
        }
        queue.write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&uniform));
        self.uploaded = Some(uniform);
    }
}

/// One uploaded decal sheet: the texture, its view and one bind group per
/// filtering level.
struct DecalSheet {
    /// Kept for ownership; the bind groups reference it.
    _texture: wgpu::Texture,
    /// Kept for ownership; the bind groups were built from it.
    _view: wgpu::TextureView,
    /// Texture + the repeating Low sampler.
    low: wgpu::BindGroup,
    /// Texture + the repeating Medium sampler.
    medium: wgpu::BindGroup,
    /// Texture + the repeating High sampler.
    high: wgpu::BindGroup,
    /// Texel storage, every mip level included.
    resident_bytes: usize,
}

impl DecalSheet {
    /// The bind group for one filtering level.
    const fn bind_group(&self, filtering: TextureFiltering) -> &wgpu::BindGroup {
        match filtering {
            TextureFiltering::Low => &self.low,
            TextureFiltering::Medium => &self.medium,
            TextureFiltering::High => &self.high,
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
        level: QualityLevel,
    ) -> Self {
        let (packer, mut draws) = pack_decal_ranges(inputs.mesh);
        // A decal sheet whose catalog entry authors `alpha_mode: "blend"` is a
        // soft-edged sheet: its ranges move to the blended pipeline. The flag
        // is derived from the level and catalog alone, exactly like the sheet
        // mapping, so it cannot drift from what `upload_sheets` loaded.
        let blend_sheets = decal_blend_sheets(inputs.level, inputs.catalog);
        for draw in &mut draws {
            draw.blend = decal_sheet_is_blend(draw.sheet, &blend_sheets);
        }
        let mut chunks: Vec<DecalChunk> = Vec::with_capacity(packer.chunks.len());
        for chunk in &packer.chunks {
            chunks.push(upload_chunk(device, queue, chunk));
        }
        let uploaded = upload_sheets(device, queue, sheet_layout, textures, inputs, level);
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
    /// reference's `draw_decal_batches`. Cut-out decals keep the packed draw
    /// order; soft-edged (`alpha_mode: "blend"`) sheets draw after them through
    /// the blended pipeline, sorted back to front, so overlapping feather
    /// strips composite in a stable order. Vertex/index buffers and sheets are
    /// rebound only when a run of draws changes them.
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
        pass.set_bind_group(0, &pipeline.bind_group, &[]);
        pass.set_bind_group(2, &pipeline.empty_bind_group, &[]);
        pass.set_bind_group(3, &pipeline.caustics_bind_group, &[]);
        pass.set_bind_group(4, &pipeline.lights_bind_group, &[]);
        // Cut-out ranges first (they write depth), then the blended ranges back
        // to front. The sort only runs when the level actually authors a
        // soft-edged sheet, so an ordinary level keeps its exact packed order.
        let mut order: Vec<usize> = (0..self.draws.len()).collect();
        let has_blend = self.draws.iter().any(|draw| draw.blend);
        if has_blend {
            let eye = inputs.frame.eye;
            let distance_squared = |index: usize| -> f32 {
                self.draws.get(index).map_or(0.0, |draw| {
                    let centre = draw.bounds.centre();
                    let dx = centre[0] - eye.x;
                    let dy = centre[1] - eye.y;
                    let dz = centre[2] - eye.z;
                    dx.mul_add(dx, dy.mul_add(dy, dz * dz))
                })
            };
            order.sort_by(|a, b| {
                let blend_a = self.draws.get(*a).is_some_and(|draw| draw.blend);
                let blend_b = self.draws.get(*b).is_some_and(|draw| draw.blend);
                blend_a.cmp(&blend_b).then_with(|| {
                    if blend_a {
                        distance_squared(*b).total_cmp(&distance_squared(*a))
                    } else {
                        a.cmp(b)
                    }
                })
            });
        }
        let mut bound_pipeline: Option<bool> = None;
        let mut bound_chunk: Option<usize> = None;
        let mut bound_sheet: Option<usize> = None;
        for index in order {
            let Some(draw) = self.draws.get(index) else {
                continue;
            };
            if draw.index_count == 0 {
                continue;
            }
            if inputs.cull && !inputs.frame.frustum.intersects_aabb(&draw.bounds) {
                continue;
            }
            if bound_pipeline != Some(draw.blend) {
                pass.set_pipeline(if draw.blend {
                    &pipeline.blend_pipeline
                } else {
                    &pipeline.pipeline
                });
                bound_pipeline = Some(draw.blend);
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
                blend: false,
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

    let index_buffer =
        super::world::upload_index_buffer(device, &chunk.indices, "places-wgpu-decal-indices");

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
    level: QualityLevel,
) -> UploadedSheets {
    let mut sheets: Vec<DecalSheet> = Vec::new();
    // The generated atlas is a fixed 256x256 sheet and always resident, exactly
    // as the reference creates it at startup; it is under every level's
    // `DecalSheet` budget by contract.
    let atlas_size = u32::try_from(DECAL_ATLAS_SIZE).unwrap_or(0);
    let atlas = RawImage::new(atlas_size, atlas_size, generate_decal_atlas());
    sheets.push(upload_sheet(device, queue, sheet_layout, textures, &atlas));

    let ids = decal_external_sheet_ids(inputs.level, inputs.catalog);
    let mut decode_cache = crate::engine::materials::TextureCache::new();
    let mut diagnostics = 0usize;
    for id in &ids {
        let image = match crate::engine::materials::resolve_decal_sheet(
            inputs.catalog,
            inputs.asset_root,
            &mut decode_cache,
            id,
        ) {
            Ok(sheet) => sheet.image,
            Err(error) => {
                crate::engine::logging::warn_once(
                    format!("decal-sheet:{id}"),
                    format!("[decals] {error}; drawing the diagnostic sheet instead"),
                );
                diagnostics = diagnostics.saturating_add(1);
                Arc::new(crate::engine::materials::missing_texture())
            }
        };
        let fitted = fit_sheet(image.as_ref(), level);
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
/// The mip chain uses the same coverage-weighted linear-light colour filter as
/// the texture cache, stopping at 1x1.
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
    let low = create_sheet_bind_group(device, sheet_layout, &view, textures, TextureFiltering::Low);
    let medium = create_sheet_bind_group(
        device,
        sheet_layout,
        &view,
        textures,
        TextureFiltering::Medium,
    );
    let high = create_sheet_bind_group(
        device,
        sheet_layout,
        &view,
        textures,
        TextureFiltering::High,
    );
    DecalSheet {
        _texture: texture,
        _view: view,
        low,
        medium,
        high,
        resident_bytes,
    }
}

/// Creates the texture + sampler bind group for one filtering level.
///
/// The sampler comes from the shared texture cache's repeating presets, so a
/// decal sheet is read with exactly the policy a world sheet is: REPEAT
/// addressing with linear mag/min/mip and the level's anisotropy, per the
/// player's setting.
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
    let index = material;
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

/// Fits colour from the original artwork, preserving High's native pixels.
fn fit_sheet(source: &RawImage, level: QualityLevel) -> std::borrow::Cow<'_, RawImage> {
    let fit = fit_image(source, level, TextureClass::DecalSheet);
    if fit.width == source.width && fit.height == source.height {
        fit
    } else {
        std::borrow::Cow::Owned(crate::engine::materials::color::resize_color(
            source, fit.width, fit.height,
        ))
    }
}

/// Box-filters colour in linear light, retaining odd edges and alpha coverage.
fn halve(source: &RawImage) -> RawImage {
    crate::engine::materials::color::resize_color(
        source,
        (source.width / 2).max(1),
        (source.height / 2).max(1),
    )
}
