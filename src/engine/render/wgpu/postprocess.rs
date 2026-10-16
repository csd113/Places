//! Post-processing: the offscreen scene target, the emissive pass, the two
//! bloom blurs, the resolve and the plain present copy.
//!
//! The world renders into a linear RGBA16F scene texture, which these passes turn
//! into a display image:
//!
//! ```text
//! scene ──▶ emissive-only pass (scene size, scene depth) ──▶ blur H, blur V ──▶ bloom
//!      └──▶ resolve (scene + bloom, exposure, tone, grade) or present copy ──▶ target
//! ```
//!
//! This module owns the targets and the four fullscreen passes; the renderer
//! owns the world body that draws into the scene and emissive targets and the
//! call that supplies the surface view. Nothing is created per frame: [`PostProcess::ensure`]
//! rebuilds the targets only when their size changes and the `encode_*` methods
//! only append passes to the renderer's encoder.
//!
//! Scene, emission and blur targets remain linear HDR. Resolve applies exposure
//! and the existing shoulder before sRGB encoding into the RGBA8 presented
//! image. The HUD blends over that display image; the final surface copy
//! preserves its bytes. Identity resolve still encodes linear scene colour.
//!
//! The resolve's five parameters are authored constants
//! ([`PostSettings`]). The engine can only produce the three quality levels
//! with bloom off or on, so one uniform buffer per combination is created once
//! and [`PostProcess::encode_resolve`] selects the matching one; no buffer is
//! ever written in the frame path, and the resolve/present pipelines only
//! change when the surface format does.

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use super::gpu_timing::GpuTimer;
use super::surface::{DEPTH_FORMAT, surface_format_is_srgb};
use crate::engine::logging;
use crate::engine::quality::QualityLevel;
use crate::engine::render::common::framebuffer::scene_target_size;
use crate::engine::render::common::postprocess::{PostSettings, bloom_target_size};
use crate::engine::render::common::view::{DrawableSize, dimension_f32};

#[cfg(feature = "visual-diagnostics")]
#[path = "postprocess_probe.rs"]
mod probe;

/// Explicit native developer control; the renderer gates the call on both
/// benchmark and presentation-probe switches before using its existing device.
///
/// # Errors
/// Returns a validation, readback or byte-comparison failure with its format.
#[cfg(feature = "visual-diagnostics")]
pub(super) fn run_present_probe(device: &wgpu::Device, queue: &wgpu::Queue) -> Result<(), String> {
    probe::run(device, queue)
}

/// The post shader, from the file next to this module.
pub const POST_SHADER_SRC: &str = include_str!("post.wgsl");

/// Name of the shared fullscreen vertex entry point.
pub const POST_VERTEX_ENTRY: &str = "vs_main";
/// The raw-target resolve: identical maths to the surface resolve, written
/// without the final encode because its target is the display-space presented
/// image.
pub const POST_RESOLVE_RAW_FRAGMENT_ENTRY: &str = "fs_resolve_raw";
/// Name of the plain presentation-copy fragment entry point.
pub const POST_PRESENT_FRAGMENT_ENTRY: &str = "fs_present";
/// The raw-target present copy: encoded display values copied with no transfer
/// function, for raw surfaces and screenshot targets.
pub const POST_PRESENT_RAW_FRAGMENT_ENTRY: &str = "fs_present_raw";
/// Name of the bloom blur fragment entry point.
pub const POST_BLUR_FRAGMENT_ENTRY: &str = "fs_blur";

/// Preserve display values on a raw surface; only an sRGB surface needs the
/// decode that cancels its hardware encode.
fn surface_present_fragment_entry(format: wgpu::TextureFormat) -> &'static str {
    if surface_format_is_srgb(format) {
        POST_PRESENT_FRAGMENT_ENTRY
    } else {
        POST_PRESENT_RAW_FRAGMENT_ENTRY
    }
}

/// Vertex attribute location of the quad's `(x, y)` pair.
pub const POST_ATTRIB_CORNER: u32 = 0;

/// Encoded display image; the UI blends in this space.
pub const PRESENTED_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// Linear HDR scene colour.
pub const SCENE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

/// Linear HDR emission, independent of environmental illumination.
pub const EMISSIVE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

/// Linear HDR blur targets, preserving emission energy above one.
pub const BLOOM_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

/// Vertices in [`PRESENT_QUAD`]: two triangles.
pub const QUAD_VERTEX_COUNT: u32 = 6;

/// The unit quad every post pass draws, as six `(x, y)` corners in `[0, 1]`.
///
/// The reference's `PRESENT_QUAD` positions with the same six pairs (its
/// `z` is always 0, and its `xy` doubles as the texture coordinate). The
/// shader maps these to clip space; see [`POST_SHADER_SRC`]'s `vs_main` for the
/// y orientation.
pub const PRESENT_QUAD: [[f32; 2]; 6] = [
    [0.0, 0.0],
    [1.0, 0.0],
    [1.0, 1.0],
    [0.0, 0.0],
    [1.0, 1.0],
    [0.0, 1.0],
];

/// What one [`PostProcess::ensure`] produced, for logging and counters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PostTargetStats {
    /// The scene/emissive target size in pixels.
    pub scene: DrawableSize,
    /// The blur target size in pixels (the scene size divided by four).
    pub bloom: DrawableSize,
    /// The presented/resolve target size in pixels (the drawable).
    pub presented: DrawableSize,
    /// True when anything was created or recreated by this call.
    pub created: bool,
}

impl Default for PostTargetStats {
    fn default() -> Self {
        Self {
            scene: DrawableSize::new(0, 0),
            bloom: DrawableSize::new(0, 0),
            presented: DrawableSize::new(0, 0),
            created: false,
        }
    }
}

/// One frame's resolve parameters, as the fragment uniform.
///
/// WGSL layout, pinned by the tests:
///
/// ```text
/// offset  0  bloom_strength    f32
/// offset  4  exposure          f32
/// offset  8  tone_knee         f32
/// offset 12  grade_saturation  f32
/// offset 16  grade_contrast    f32
/// offset 20  _padding          f32 x3
/// ------------------------------------------ 32 bytes, 16-byte aligned
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
struct PostParams {
    bloom_strength: f32,
    exposure: f32,
    tone_knee: f32,
    grade_saturation: f32,
    grade_contrast: f32,
    _padding: [f32; 3],
}

impl PostParams {
    const fn from_settings(settings: PostSettings) -> Self {
        Self {
            bloom_strength: settings.bloom_strength,
            exposure: settings.exposure,
            tone_knee: settings.tone_knee,
            grade_saturation: settings.grade_saturation,
            grade_contrast: settings.grade_contrast,
            _padding: [0.0; 3],
        }
    }
}

/// The blur step uniform: one texel of the *source* image, per axis.
///
/// `xy` is the step; `zw` is unused padding so the struct is one 16-byte
/// uniform slot. WGSL layout: `vec4<f32>` at offset 0, 16 bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
struct BlurParams {
    texel: [f32; 4],
}

impl BlurParams {
    /// The step for one blur pass, one texel of `source` along one axis.
    ///
    /// The reference passes `(x / source_width, y / source_height)` with `(x, y)`
    /// either `(1, 0)` or `(0, 1)`, so a full-resolution emissive image feeding
    /// a quarter-resolution buffer steps in source texels and downsamples by
    /// four as it blurs.
    #[must_use]
    fn step(source: DrawableSize, horizontal: bool) -> Self {
        let width = dimension_f32(source.width).max(1.0);
        let height = dimension_f32(source.height).max(1.0);
        let texel = if horizontal {
            [1.0 / width, 0.0, 1.0, 0.0]
        } else {
            [0.0, 1.0 / height, 0.0, 0.0]
        };
        Self { texel }
    }
}

/// Bytes one [`PostParams`] occupies.
const POST_PARAMS_SIZE: u64 = super::buffer_element_bytes::<PostParams>();
/// Bytes one [`BlurParams`] occupies.
const BLUR_PARAMS_SIZE: u64 = super::buffer_element_bytes::<BlurParams>();

/// Bloom-off/on uniforms, updated only when authored presentation changes.
const RESOLVE_VARIANTS: [PostSettings; 2] = [
    PostSettings::for_level(QualityLevel::High).with_bloom(false),
    PostSettings::for_level(QualityLevel::High).with_bloom(true),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ResolvePath {
    PresentCopy,
    Resolve(usize),
}

fn resolve_path(settings: PostSettings, bloom_enabled: bool) -> ResolvePath {
    if settings.is_identity() {
        ResolvePath::PresentCopy
    } else {
        ResolvePath::Resolve(usize::from(bloom_enabled && settings.blooms()))
    }
}

/// The target sizes one `ensure` computes for a level and drawable.
///
/// The scene target follows the level ([`scene_target_size`]: the drawable
/// under High, half the drawable under Medium, no wider than the reference's
/// 480 pixels under Low); the presented image always follows the drawable. The
/// reference's default framebuffer is drawable-sized, so its resolve, grade and
/// the HUD all run at the drawable's resolution over a smaller scene. Sizing
/// the presented image with the scene instead would run the resolve and the HUD
/// at the scene's resolution and then upscale them, a difference the Low menu
/// showed clearly (measured mean 1.58 vs 0.26 at High); the presented image is
/// therefore always the drawable.
#[must_use]
fn target_sizes(
    level: QualityLevel,
    drawable: DrawableSize,
) -> (DrawableSize, DrawableSize, DrawableSize) {
    let scene = scene_target_size(level, drawable);
    (scene, bloom_target_size(scene), drawable)
}

/// The three bind group layouts the post pipelines and targets use.
struct PostLayouts {
    resolve: wgpu::BindGroupLayout,
    present: wgpu::BindGroupLayout,
    blur: wgpu::BindGroupLayout,
}

impl PostLayouts {
    fn new(device: &wgpu::Device) -> Self {
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
        let uniform = |binding: u32, size: u64| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: wgpu::BufferSize::new(size),
            },
            count: None,
        };
        Self {
            resolve: device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("places-wgpu-resolve-bind-layout"),
                entries: &[
                    texture(0),
                    sampler(1),
                    uniform(2, POST_PARAMS_SIZE),
                    texture(3),
                    sampler(4),
                ],
            }),
            present: device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("places-wgpu-present-bind-layout"),
                entries: &[texture(0), sampler(1)],
            }),
            blur: device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("places-wgpu-blur-bind-layout"),
                entries: &[texture(0), sampler(1), uniform(5, BLUR_PARAMS_SIZE)],
            }),
        }
    }
}

/// The post render pipelines.
///
/// The blur always writes [`BLOOM_FORMAT`]. The resolve pipeline is raw (it
/// writes the display-space presented image), while the present copy comes in
/// two forms: the *raw* form preserves encoded display values, while the sRGB
/// *surface* form decodes them before the hardware encode.
struct PostPipelines {
    resolve_raw: wgpu::RenderPipeline,
    present_raw: wgpu::RenderPipeline,
    linear_present_raw: wgpu::RenderPipeline,
    present: wgpu::RenderPipeline,
    blur: wgpu::RenderPipeline,
}

impl PostPipelines {
    /// Builds the final present copy for `surface_format`; intermediate
    /// presentation passes write [`PRESENTED_FORMAT`] and blur writes
    /// [`BLOOM_FORMAT`].
    fn new(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        layouts: &PostLayouts,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("places-wgpu-post-shader"),
            source: wgpu::ShaderSource::Wgsl(POST_SHADER_SRC.into()),
        });
        Self {
            resolve_raw: build_pass_pipeline(
                device,
                "places-wgpu-resolve-raw",
                &shader,
                &layouts.resolve,
                POST_RESOLVE_RAW_FRAGMENT_ENTRY,
                PRESENTED_FORMAT,
            ),
            present_raw: build_pass_pipeline(
                device,
                "places-wgpu-present-raw",
                &shader,
                &layouts.present,
                POST_PRESENT_RAW_FRAGMENT_ENTRY,
                PRESENTED_FORMAT,
            ),
            linear_present_raw: build_pass_pipeline(
                device,
                "places-wgpu-linear-present-raw",
                &shader,
                &layouts.present,
                "fs_linear_present_raw",
                PRESENTED_FORMAT,
            ),
            present: build_pass_pipeline(
                device,
                "places-wgpu-present",
                &shader,
                &layouts.present,
                surface_present_fragment_entry(surface_format),
                surface_format,
            ),
            blur: build_pass_pipeline(
                device,
                "places-wgpu-bloom-blur",
                &shader,
                &layouts.blur,
                POST_BLUR_FRAGMENT_ENTRY,
                BLOOM_FORMAT,
            ),
        }
    }
}

/// Builds one fullscreen pipeline: the shared vertex stage, no depth, no
/// blending and no culling (the reference disables both for every post pass).
fn build_pass_pipeline(
    device: &wgpu::Device,
    label: &'static str,
    shader: &wgpu::ShaderModule,
    bind_layout: &wgpu::BindGroupLayout,
    fragment_entry: &'static str,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(label),
        bind_group_layouts: &[Some(bind_layout)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some(POST_VERTEX_ENTRY),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[Some(quad_vertex_layout())],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            unclipped_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(fragment_entry),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

/// Vertex attribute list of the fullscreen quad.
const POST_QUAD_ATTRIBUTES: [wgpu::VertexAttribute; 1] =
    wgpu::vertex_attr_array![POST_ATTRIB_CORNER => Float32x2];

/// The quad's vertex buffer layout: one `vec2<f32>` corner per vertex.
#[must_use]
const fn quad_vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout {
        array_stride: super::buffer_element_bytes::<[f32; 2]>(),
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &POST_QUAD_ATTRIBUTES,
    }
}

/// Every size-dependent post target and binding, rebuilt as one unit.
struct PostTargets {
    /// Kept for ownership; the view references it.
    _scene_texture: wgpu::Texture,
    scene_view: wgpu::TextureView,
    /// The scene depth, shared with the emissive pass. Kept for ownership.
    _depth_texture: wgpu::Texture,
    depth_view: wgpu::TextureView,
    /// The emissive image at the scene target's own size: the pass tests
    /// against the scene depth in window coordinates, so a smaller target
    /// would reject the wrong emitters.
    _emissive_texture: wgpu::Texture,
    emissive_view: wgpu::TextureView,
    /// Blur pass 1's destination, at the scene size divided by four.
    _blur_a_texture: wgpu::Texture,
    blur_a_view: wgpu::TextureView,
    /// Blur pass 2's destination and the resolve's bloom source.
    _blur_b_texture: wgpu::Texture,
    blur_b_view: wgpu::TextureView,
    /// The tone-mapped, display-space image the UI blends into: the backend's
    /// equivalent of the reference's default framebuffer, so the HUD blends in
    /// raw display space before the final sRGB encode.
    _presented_texture: wgpu::Texture,
    presented_view: wgpu::TextureView,
    /// The plain copy's binding for the presented image.
    presented_present_bind: wgpu::BindGroup,
    /// The scene target size these were created at.
    scene_size: DrawableSize,
    /// The presented target size these were created at (the drawable).
    presented_size: DrawableSize,
    /// Pass 1: the emissive image into buffer A, one **source** texel
    /// horizontally.
    blur_h_bind: wgpu::BindGroup,
    /// Pass 2: buffer A into buffer B, one **source** texel vertically.
    blur_v_bind: wgpu::BindGroup,
    /// The two blur step uniforms, kept alongside their bind groups.
    _horizontal_step: wgpu::Buffer,
    _vertical_step: wgpu::Buffer,
    /// One resolve binding per [`RESOLVE_VARIANTS`] entry, in the same order.
    resolve_binds: Vec<wgpu::BindGroup>,
    /// The plain copy: the scene and its nearest sampler.
    present_bind: wgpu::BindGroup,
}

/// The post-processing targets, pipelines and fullscreen quad.
///
/// Created once per renderer (`new`); the targets appear on the first
/// [`PostProcess::ensure`] and are rebuilt only when their size changes.
pub struct PostProcess {
    layouts: PostLayouts,
    pipelines: PostPipelines,
    /// Clamp-to-edge NEAREST: the scene target's own filter (sampled 1:1) and
    /// the emissive image's, both the reference's non-linear choice.
    nearest_sampler: wgpu::Sampler,
    /// Clamp-to-edge LINEAR: the blur targets' filter, and therefore the
    /// bloom image the resolve stretches back up.
    linear_sampler: wgpu::Sampler,
    quad_buffer: wgpu::Buffer,
    /// One uniform per [`RESOLVE_VARIANTS`] entry, in the same order.
    post_uniforms: Vec<wgpu::Buffer>,
    uploaded_settings: [PostSettings; 2],
    surface_format: wgpu::TextureFormat,
    targets: Option<PostTargets>,
}

impl PostProcess {
    /// Builds every pipeline and sampler for the surface format.
    ///
    /// No size-dependent target is created here: [`Self::ensure`] owns those.
    #[must_use]
    pub fn new(device: &wgpu::Device, surface_format: wgpu::TextureFormat) -> Self {
        if !surface_format_is_srgb(surface_format) {
            logging::warn_once(
                "wgpu-post-surface-format",
                "[wgpu] the surface format is not sRGB; presentation copies encoded display values and direct 8-bit drawing uses software encoding",
            );
        }
        let layouts = PostLayouts::new(device);
        let pipelines = PostPipelines::new(device, surface_format, &layouts);
        let quad_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("places-wgpu-post-quad"),
            contents: bytemuck::cast_slice(&PRESENT_QUAD),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let post_uniforms = RESOLVE_VARIANTS
            .iter()
            .map(|settings| {
                create_uniform_buffer(
                    device,
                    "places-wgpu-post-params",
                    &PostParams::from_settings(*settings),
                )
            })
            .collect();
        Self {
            layouts,
            pipelines,
            nearest_sampler: create_sampler(
                device,
                "places-wgpu-post-nearest",
                wgpu::FilterMode::Nearest,
            ),
            linear_sampler: create_sampler(
                device,
                "places-wgpu-post-linear",
                wgpu::FilterMode::Linear,
            ),
            quad_buffer,
            post_uniforms,
            uploaded_settings: RESOLVE_VARIANTS,
            surface_format,
            targets: None,
        }
    }

    /// A still level writes nothing; an authored change updates two 32-byte slots.
    pub fn upload_settings(&mut self, queue: &wgpu::Queue, settings: PostSettings) {
        let next = [settings.with_bloom(false), settings.with_bloom(true)];
        for ((buffer, previous), value) in self
            .post_uniforms
            .iter()
            .zip(&mut self.uploaded_settings)
            .zip(next)
        {
            if *previous != value {
                queue.write_buffer(
                    buffer,
                    0,
                    bytemuck::bytes_of(&PostParams::from_settings(value)),
                );
                *previous = value;
            }
        }
    }

    /// Rebuilds the resolve and present pipelines when the surface format
    /// changed, exactly like the world pipeline's format check.
    ///
    /// The targets and their bind groups are format-independent and survive.
    /// Returns true when anything was rebuilt.
    pub fn set_surface_format(
        &mut self,
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
    ) -> bool {
        if self.surface_format == surface_format {
            return false;
        }
        if !surface_format_is_srgb(surface_format) {
            logging::warn_once(
                "wgpu-post-surface-format",
                "[wgpu] the surface format is not sRGB; presentation copies encoded display values and direct 8-bit drawing uses software encoding",
            );
        }
        self.surface_format = surface_format;
        self.pipelines = PostPipelines::new(device, surface_format, &self.layouts);
        true
    }

    /// Ensures the scene, emissive, blur and presented targets for
    /// `(level, drawable)`.
    ///
    /// The scene size is [`scene_target_size`]: the drawable under High, half
    /// the drawable under Medium, no wider than the 480-pixel reference under
    /// Low, never upscaled. The emissive image shares that size (it shares the
    /// scene depth); the blur buffers are a quarter of it
    /// ([`bloom_target_size`]). The presented image is the drawable: the
    /// reference's default framebuffer, where the resolve and the HUD run at
    /// full resolution. An empty drawable drops the targets; a size change
    /// rebuilds the whole set once. `created` is true only when something was
    /// (re)created.
    pub fn ensure(
        &mut self,
        device: &wgpu::Device,
        level: QualityLevel,
        drawable: DrawableSize,
    ) -> PostTargetStats {
        let (scene, bloom, presented) = target_sizes(level, drawable);
        if scene.is_empty() || presented.is_empty() {
            self.targets = None;
            return PostTargetStats {
                scene,
                bloom,
                presented,
                created: false,
            };
        }
        if self.targets.as_ref().is_some_and(|targets| {
            targets.scene_size == scene && targets.presented_size == presented
        }) {
            return PostTargetStats {
                scene,
                bloom,
                presented,
                created: false,
            };
        }
        self.targets = Some(self.create_targets(device, scene, bloom, presented));
        PostTargetStats {
            scene,
            bloom,
            presented,
            created: true,
        }
    }

    /// The scene colour target, for the world body's main pass.
    #[must_use]
    pub fn scene_view(&self) -> Option<&wgpu::TextureView> {
        self.targets.as_ref().map(|targets| &targets.scene_view)
    }

    /// The scene depth target, for the world body's main pass and, through
    /// [`Self::emissive_begin`], the emissive pass.
    #[must_use]
    pub fn scene_depth_view(&self) -> Option<&wgpu::TextureView> {
        self.targets.as_ref().map(|targets| &targets.depth_view)
    }

    /// Begins the emissive pass on the emissive target, sharing the scene
    /// depth; the caller encodes the world's emissive body inside.
    ///
    /// The colour is cleared to the reference's opaque black and the depth is
    /// **loaded, never cleared**: it holds the scene the main pass just drew,
    /// which is what keeps an emitter hidden behind a wall out of the bloom.
    /// The world's emissive pipelines declare depth writes off, so the scene
    /// depth survives the pass unchanged.
    pub fn emissive_begin<'a>(
        &'a self,
        encoder: &'a mut wgpu::CommandEncoder,
        timer: Option<&GpuTimer>,
    ) -> Option<wgpu::RenderPass<'a>> {
        let targets = self.targets.as_ref()?;
        Some(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("places-wgpu-emissive"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &targets.emissive_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &targets.depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: timer.and_then(|value| value.pass_writes("emission")),
            occlusion_query_set: None,
            multiview_mask: None,
        }))
    }

    /// Records the two blur passes (emissive→A horizontally, A→B vertically).
    ///
    /// A no-op when the targets do not exist or `bloom_strength` is zero, so a
    /// caller may invoke it unconditionally for the frame's settings.
    pub fn encode_blur(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        bloom_strength: f32,
        timer: Option<&GpuTimer>,
    ) {
        if bloom_strength <= 0.0 {
            return;
        }
        let Some(targets) = self.targets.as_ref() else {
            return;
        };
        encode_fullscreen(
            encoder,
            "places-wgpu-bloom-h",
            &targets.blur_a_view,
            &self.pipelines.blur,
            &targets.blur_h_bind,
            &self.quad_buffer,
            timer,
        );
        encode_fullscreen(
            encoder,
            "places-wgpu-bloom-v",
            &targets.blur_b_view,
            &self.pipelines.blur,
            &targets.blur_v_bind,
            &self.quad_buffer,
            timer,
        );
    }

    /// Records the resolve, or the plain present copy, into `target`, which
    /// must be the raw presented image (see [`Self::presented_view`]).
    ///
    /// The choice is [`PostSettings::is_identity`], exactly like the
    /// reference's `post.settings().is_identity()` branch: the identity path
    /// encodes the linear scene into the presented image without grading.
    /// Otherwise the resolve adds the bloom image when `bloom_enabled` and the
    /// settings bloom, then applies exposure, the tone shoulder and the grade —
    /// in linear light, then encodes and applies the display grade. The final
    /// surface copy preserves those encoded values.
    pub fn encode_resolve(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        settings: PostSettings,
        bloom_enabled: bool,
        timer: Option<&GpuTimer>,
    ) {
        let Some(targets) = self.targets.as_ref() else {
            return;
        };
        match resolve_path(settings, bloom_enabled) {
            ResolvePath::PresentCopy => encode_fullscreen(
                encoder,
                "places-wgpu-present",
                target,
                &self.pipelines.linear_present_raw,
                &targets.present_bind,
                &self.quad_buffer,
                timer,
            ),
            ResolvePath::Resolve(slot) => {
                let Some(bind) = targets.resolve_binds.get(slot) else {
                    return;
                };
                encode_fullscreen(
                    encoder,
                    "places-wgpu-resolve",
                    target,
                    &self.pipelines.resolve_raw,
                    bind,
                    &self.quad_buffer,
                    timer,
                );
            }
        }
    }

    /// The presented image: resolve output plus the UI, in raw display space.
    ///
    /// This is the backend's default-framebuffer equivalent; both the UI pass
    /// and the one-shot capture read it.
    #[must_use]
    pub fn presented_view(&self) -> Option<&wgpu::TextureView> {
        self.targets.as_ref().map(|targets| &targets.presented_view)
    }

    /// Copies the presented image to the configured surface format. An sRGB
    /// target decodes before its hardware encode; a raw target preserves the
    /// encoded display values directly.
    pub fn encode_present_to(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        timer: Option<&GpuTimer>,
    ) {
        let Some(targets) = self.targets.as_ref() else {
            return;
        };
        encode_fullscreen(
            encoder,
            "places-wgpu-presented",
            target,
            &self.pipelines.present,
            &targets.presented_present_bind,
            &self.quad_buffer,
            timer,
        );
    }

    /// Copies the presented image to a **raw** target with no transfer
    /// function: the reference's own framebuffer convention, used by the
    /// screenshot readback so the PNG carries exactly the display-space bytes
    /// the reference read with `glReadPixels` — an sRGB capture texture would
    /// add a hardware conversion to every measured pixel.
    pub fn encode_present_raw_to(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
    ) {
        let Some(targets) = self.targets.as_ref() else {
            return;
        };
        encode_fullscreen(
            encoder,
            "places-wgpu-presented-raw",
            target,
            &self.pipelines.present_raw,
            &targets.presented_present_bind,
            &self.quad_buffer,
            None,
        );
    }

    /// The presented target's pixel size, if the targets exist.
    #[must_use]
    pub fn presented_size(&self) -> Option<DrawableSize> {
        self.targets.as_ref().map(|targets| targets.presented_size)
    }

    /// Actual resident scene dimensions for the opt-in capture receipt.
    #[cfg(feature = "visual-diagnostics")]
    #[must_use]
    pub fn diagnostic_scene_size(&self) -> Option<DrawableSize> {
        self.targets.as_ref().map(|targets| targets.scene_size)
    }

    /// Uncompressed attachment texels, excluding driver alignment/metadata.
    /// This remains useful on Metal, whose HAL byte counters are unavailable.
    #[cfg(feature = "visual-diagnostics")]
    pub fn diagnostic_attachment_bytes(&self) -> u64 {
        self.targets.as_ref().map_or(0, |targets| {
            let bytes = |size: DrawableSize, bytes_per_pixel: u64| {
                u64::from(size.width)
                    .saturating_mul(u64::from(size.height))
                    .saturating_mul(bytes_per_pixel)
            };
            // Scene/emission RGBA16F + Depth32F; two RGBA16F blur images;
            // one raw RGBA8 presented image. All have one mip and one layer.
            bytes(targets.scene_size, 20)
                .saturating_add(bytes(bloom_target_size(targets.scene_size), 16))
                .saturating_add(bytes(targets.presented_size, 4))
        })
    }

    /// True when the scene target exists at the size the last [`Self::ensure`]
    /// computed (the expected size), and therefore when the scene and
    /// emissive passes can run.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.targets
            .as_ref()
            .is_some_and(|targets| !targets.scene_size.is_empty())
    }

    /// Creates one blur pass's binding: source, sampler and step.
    fn blur_bind(
        &self,
        device: &wgpu::Device,
        label: &str,
        source: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
        step: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(label),
            layout: &self.layouts.blur,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(source),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: step.as_entire_binding(),
                },
            ],
        })
    }

    /// Creates one resolve binding: the scene, the blurred emissive image and
    /// one parameter set.
    fn resolve_bind(
        &self,
        device: &wgpu::Device,
        scene: &wgpu::TextureView,
        bloom: &wgpu::TextureView,
        params: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("places-wgpu-resolve"),
            layout: &self.layouts.resolve,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(scene),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.nearest_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: params.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(bloom),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&self.linear_sampler),
                },
            ],
        })
    }

    /// Creates the plain present copy's binding: the scene and its sampler.
    fn present_bind(&self, device: &wgpu::Device, scene: &wgpu::TextureView) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("places-wgpu-present"),
            layout: &self.layouts.present,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(scene),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.nearest_sampler),
                },
            ],
        })
    }

    /// Creates the whole size-dependent target set and its bindings.
    fn create_targets(
        &self,
        device: &wgpu::Device,
        scene_size: DrawableSize,
        bloom_size: DrawableSize,
        presented_size: DrawableSize,
    ) -> PostTargets {
        let attachment =
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;
        let (scene_texture, scene_view) = create_target_texture(
            device,
            "places-wgpu-scene",
            scene_size,
            SCENE_FORMAT,
            attachment,
        );
        let (depth_texture, depth_view) = create_target_texture(
            device,
            "places-wgpu-scene-depth",
            scene_size,
            DEPTH_FORMAT,
            wgpu::TextureUsages::RENDER_ATTACHMENT,
        );
        let (emissive_texture, emissive_view) = create_target_texture(
            device,
            "places-wgpu-emissive",
            scene_size,
            EMISSIVE_FORMAT,
            attachment,
        );
        let (scratch_texture, scratch_view) = create_target_texture(
            device,
            "places-wgpu-bloom-a",
            bloom_size,
            BLOOM_FORMAT,
            attachment,
        );
        let (bloom_texture, bloom_view) = create_target_texture(
            device,
            "places-wgpu-bloom-b",
            bloom_size,
            BLOOM_FORMAT,
            attachment,
        );
        // One source texel per step: horizontally out of the scene-sized
        // emissive image (which also downsamples by four), vertically out of
        // the scratch buffer.
        let horizontal_step = create_uniform_buffer(
            device,
            "places-wgpu-blur-h",
            &BlurParams::step(scene_size, true),
        );
        let vertical_step = create_uniform_buffer(
            device,
            "places-wgpu-blur-v",
            &BlurParams::step(bloom_size, false),
        );
        // The emissive image is NEAREST (the reference's non-linear colour
        // target); the scratch buffer is LINEAR (the reference's blur target).
        let horizontal_bind = self.blur_bind(
            device,
            "places-wgpu-bloom-h",
            &emissive_view,
            &self.nearest_sampler,
            &horizontal_step,
        );
        let vertical_bind = self.blur_bind(
            device,
            "places-wgpu-bloom-v",
            &scratch_view,
            &self.linear_sampler,
            &vertical_step,
        );
        let resolve_binds = self
            .post_uniforms
            .iter()
            .map(|params| self.resolve_bind(device, &scene_view, &bloom_view, params))
            .collect();
        let (presented_texture, presented_view) = create_target_texture(
            device,
            "places-wgpu-presented",
            presented_size,
            PRESENTED_FORMAT,
            attachment,
        );
        let present_bind = self.present_bind(device, &scene_view);
        let presented_present_bind = self.present_bind(device, &presented_view);
        PostTargets {
            _scene_texture: scene_texture,
            scene_view,
            _depth_texture: depth_texture,
            depth_view,
            _emissive_texture: emissive_texture,
            emissive_view,
            _blur_a_texture: scratch_texture,
            blur_a_view: scratch_view,
            _blur_b_texture: bloom_texture,
            blur_b_view: bloom_view,
            _presented_texture: presented_texture,
            presented_view,
            presented_present_bind,
            _horizontal_step: horizontal_step,
            _vertical_step: vertical_step,
            scene_size,
            presented_size,
            blur_h_bind: horizontal_bind,
            blur_v_bind: vertical_bind,
            resolve_binds,
            present_bind,
        }
    }
}

/// Records one fullscreen draw into `view`.
///
/// The attachment is loaded, not cleared: every pass covers its whole target,
/// and the resolve must not disturb the surface the UI is drawn into
/// afterwards.
fn encode_fullscreen(
    encoder: &mut wgpu::CommandEncoder,
    label: &'static str,
    view: &wgpu::TextureView,
    pipeline: &wgpu::RenderPipeline,
    bind: &wgpu::BindGroup,
    quad: &wgpu::Buffer,
    timer: Option<&GpuTimer>,
) {
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some(label),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                // This quad covers the full target with blending disabled.
                // Prior attachment pixels have no reader in this pass.
                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: timer.and_then(|value| value.pass_writes(label)),
        occlusion_query_set: None,
        multiview_mask: None,
    });
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, bind, &[]);
    pass.set_vertex_buffer(0, quad.slice(..));
    pass.draw(0..QUAD_VERTEX_COUNT, 0..1);
}

/// Creates one colour or depth target texture and its default view.
fn create_target_texture(
    device: &wgpu::Device,
    label: &str,
    size: DrawableSize,
    format: wgpu::TextureFormat,
    usage: wgpu::TextureUsages,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: size.width.max(1),
            height: size.height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

/// Creates a clamp-to-edge sampler with `filter` on both axes and no mips.
fn create_sampler(device: &wgpu::Device, label: &str, filter: wgpu::FilterMode) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some(label),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: filter,
        min_filter: filter,
        mipmap_filter: wgpu::MipmapFilterMode::Nearest,
        ..Default::default()
    })
}

/// Creates a uniform buffer holding one `Pod` value, filled at creation.
fn create_uniform_buffer<T: Pod>(device: &wgpu::Device, label: &str, value: &T) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: bytemuck::bytes_of(value),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    })
}
