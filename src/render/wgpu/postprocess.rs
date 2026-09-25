//! Post-processing: the offscreen scene target, the emissive pass, the two
//! bloom blurs, the resolve and the plain present copy.
//!
//! The reference's offscreen path (preserved at the `renderer-gles2-reference`
//! tag) renders the world into an RGBA8 scene texture and then turns that
//! texture into a display image:
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
//! Colour space: every target here is raw `Rgba8Unorm` holding the reference's
//! display-space values — scene, presented, emissive and both blur buffers —
//! exactly like the reference's RGBA8 attachments. The world shader writes
//! display values directly; only the final copy to the sRGB surface converts
//! (`srgb_to_linear` in the present entry point). The presented image is the
//! drawable, so the resolve and the HUD run at default-framebuffer resolution
//! exactly like the reference.
//!
//! The resolve's five parameters are authored constants
//! ([`PostSettings`]). The engine can only produce the three quality levels
//! with bloom off or on, so one uniform buffer per combination is created once
//! and [`PostProcess::encode_resolve`] selects the matching one; no buffer is
//! ever written in the frame path, and the resolve/present pipelines only
//! change when the surface format does.

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use super::surface::{DEPTH_FORMAT, surface_format_is_srgb};
use crate::logging;
use crate::quality::QualityLevel;
use crate::render::common::framebuffer::scene_target_size;
use crate::render::common::postprocess::{PostSettings, bloom_target_size};
use crate::render::common::view::{DrawableSize, dimension_f32};

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
/// The raw-target present copy: the scene copied straight into the presented
/// image with no transfer function.
pub const POST_PRESENT_RAW_FRAGMENT_ENTRY: &str = "fs_present_raw";
/// Name of the bloom blur fragment entry point.
pub const POST_BLUR_FRAGMENT_ENTRY: &str = "fs_blur";

/// Vertex attribute location of the quad's `(x, y)` pair.
pub const POST_ATTRIB_CORNER: u32 = 0;

/// The scene target's format, and therefore the world pipeline format the
/// scene pass must be built for.
///
/// Raw (non-sRGB), like the reference's RGBA8 scene target: the world shader
/// writes display-space values directly, so hardware alpha blending, filtering
/// and the resolve all operate in the space OpenGL did.
pub const SCENE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// The emissive target's format, and therefore the world emissive pipeline
/// format: raw `Rgba8Unorm`, so `fs_emission`'s display-space output is stored
/// with no transfer function, exactly like the reference's RGBA8 bloom source.
pub const EMISSIVE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// Both blur targets' format: raw `Rgba8Unorm`, the same display space.
pub const BLOOM_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

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
            [1.0 / width, 0.0, 0.0, 0.0]
        } else {
            [0.0, 1.0 / height, 0.0, 0.0]
        };
        Self { texel }
    }
}

/// Bytes one [`PostParams`] occupies.
const POST_PARAMS_SIZE: u64 = std::mem::size_of::<PostParams>() as u64;
/// Bytes one [`BlurParams`] occupies.
const BLUR_PARAMS_SIZE: u64 = std::mem::size_of::<BlurParams>() as u64;

/// The six parameter sets the engine can produce: the three quality levels
/// with bloom off and on.
///
/// This is the whole space of [`PostSettings`] (`PostSettings::for_level` plus
/// [`PostSettings::with_bloom`]), so one static uniform per entry covers every
/// frame without a buffer write. Level-major order keeps each level's two slots
/// adjacent.
const RESOLVE_VARIANTS: [PostSettings; 6] = [
    PostSettings::for_level(QualityLevel::Low).with_bloom(false),
    PostSettings::for_level(QualityLevel::Low).with_bloom(true),
    PostSettings::for_level(QualityLevel::Medium).with_bloom(false),
    PostSettings::for_level(QualityLevel::Medium).with_bloom(true),
    PostSettings::for_level(QualityLevel::High).with_bloom(false),
    PostSettings::for_level(QualityLevel::High).with_bloom(true),
];

/// Index of High's bloom-off slot in [`RESOLVE_VARIANTS`].
const HIGH_RESOLVE_SLOT: usize = 4;

/// Which fullscreen pass one frame's resolve selection records.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ResolvePath {
    /// The settings are the identity: the plain scene copy, not a no-op
    /// resolve.
    PresentCopy,
    /// The resolve, with the uniform slot that carries these settings.
    Resolve(usize),
}

/// The uniform slot that carries exactly these settings, if any.
#[must_use]
#[allow(clippy::float_cmp)] // authored constants compared for identity, not measurements
fn settings_slot(settings: PostSettings) -> Option<usize> {
    RESOLVE_VARIANTS
        .iter()
        .position(|variant| *variant == settings)
}

/// The pass [`PostProcess::encode_resolve`] records for one frame.
///
/// The identity check is the neutral [`PostSettings::is_identity`], not a
/// re-derivation; a bloom frame whose emissive stage did not run uses the same
/// settings with the strength cleared, which is exactly the reference's
/// `(0.0, scene)` fallback.
#[must_use]
fn resolve_path(settings: PostSettings, bloom_enabled: bool) -> ResolvePath {
    if settings.is_identity() {
        return ResolvePath::PresentCopy;
    }
    let effective = if bloom_enabled && settings.blooms() {
        settings
    } else {
        settings.with_bloom(false)
    };
    ResolvePath::Resolve(settings_slot(effective).unwrap_or_else(|| {
        logging::warn_once(
            "wgpu-post-settings-unmatched",
            "[wgpu] post settings are outside the level-derived set; using High",
        );
        HIGH_RESOLVE_SLOT.saturating_add(usize::from(effective.blooms()))
    }))
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
/// two forms: the *raw* form writes the presented image again, and the
/// *surface* form writes the sRGB surface with the one display-to-linear
/// encode.
struct PostPipelines {
    resolve_raw: wgpu::RenderPipeline,
    present_raw: wgpu::RenderPipeline,
    present: wgpu::RenderPipeline,
    blur: wgpu::RenderPipeline,
}

impl PostPipelines {
    /// Builds every post pipeline for `surface_format`, which the resolve and
    /// present copy write to. The blur always writes [`BLOOM_FORMAT`].
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
                SCENE_FORMAT,
            ),
            present_raw: build_pass_pipeline(
                device,
                "places-wgpu-present-raw",
                &shader,
                &layouts.present,
                POST_PRESENT_RAW_FRAGMENT_ENTRY,
                SCENE_FORMAT,
            ),
            present: build_pass_pipeline(
                device,
                "places-wgpu-present",
                &shader,
                &layouts.present,
                POST_PRESENT_FRAGMENT_ENTRY,
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
        array_stride: std::mem::size_of::<[f32; 2]>() as u64,
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
                "[wgpu] the surface format is not sRGB; the resolve and present passes assume the hardware encodes their output",
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
            surface_format,
            targets: None,
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
                "[wgpu] the surface format is not sRGB; the resolve and present passes assume the hardware encodes their output",
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
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        }))
    }

    /// Records the two blur passes (emissive→A horizontally, A→B vertically).
    ///
    /// A no-op when the targets do not exist or `bloom_strength` is zero, so a
    /// caller may invoke it unconditionally for the frame's settings.
    pub fn encode_blur(&self, encoder: &mut wgpu::CommandEncoder, bloom_strength: f32) {
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
        );
        encode_fullscreen(
            encoder,
            "places-wgpu-bloom-v",
            &targets.blur_b_view,
            &self.pipelines.blur,
            &targets.blur_v_bind,
            &self.quad_buffer,
        );
    }

    /// Records the resolve, or the plain present copy, into `target`, which
    /// must be the raw presented image (see [`Self::presented_view`]).
    ///
    /// The choice is [`PostSettings::is_identity`], exactly like the
    /// reference's `post.settings().is_identity()` branch: the identity path
    /// copies the scene straight into the presented image, byte-identical.
    /// Otherwise the resolve adds the bloom image when `bloom_enabled` and the
    /// settings bloom, then applies exposure, the tone shoulder and the grade —
    /// all in the display space the reference used. The presented image is
    /// encoded once by [`Self::encode_present_to`].
    pub fn encode_resolve(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        settings: PostSettings,
        bloom_enabled: bool,
    ) {
        let Some(targets) = self.targets.as_ref() else {
            return;
        };
        match resolve_path(settings, bloom_enabled) {
            ResolvePath::PresentCopy => encode_fullscreen(
                encoder,
                "places-wgpu-present",
                target,
                &self.pipelines.present_raw,
                &targets.present_bind,
                &self.quad_buffer,
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

    /// Copies the presented image to an sRGB target (the surface or a capture
    /// texture), applying the single display-to-linear encode.
    pub fn encode_present_to(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
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
        );
    }

    /// The presented target's pixel size, if the targets exist.
    #[must_use]
    pub fn presented_size(&self) -> Option<DrawableSize> {
        self.targets.as_ref().map(|targets| targets.presented_size)
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
            SCENE_FORMAT,
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
    label: &str,
    view: &wgpu::TextureView,
    pipeline: &wgpu::RenderPipeline,
    bind: &wgpu::BindGroup,
    quad: &wgpu::Buffer,
) {
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some(label),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Load,
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
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
        usage: wgpu::BufferUsages::UNIFORM,
    })
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::float_cmp,
        clippy::indexing_slicing,
        clippy::unwrap_used,
        clippy::arithmetic_side_effects,
        // The CPU mirrors below keep the shader's plain expression order, so
        // their rounding matches the WGSL's, not a fused multiply-add's.
        clippy::suboptimal_flops
    )]

    use super::*;

    /// The shader source, so the expression checks read the shipped file.
    const WGSL: &str = POST_SHADER_SRC;

    /// The CPU mirror of the resolve's tone shoulder.
    fn tone_shoulder(color: f32, knee: f32) -> f32 {
        let above = (color - knee).max(0.0);
        let span = (1.0 - knee).max(1.0e-3);
        color.min(knee) + span * (above / (above + span))
    }

    /// The CPU mirror of the Rec.709 saturation and mid-grey contrast.
    fn grade(color: [f32; 3], saturation: f32, contrast: f32) -> [f32; 3] {
        let luma = 0.2126 * color[0] + 0.7152 * color[1] + 0.0722 * color[2];
        let mut out = [0.0; 3];
        for (slot, channel) in out.iter_mut().zip(color) {
            let mixed = luma + (channel - luma) * saturation;
            *slot = ((mixed - 0.5) * contrast + 0.5).clamp(0.0, 1.0);
        }
        out
    }

    #[test]
    fn the_scene_target_is_raw_display_space() {
        assert_eq!(SCENE_FORMAT, wgpu::TextureFormat::Rgba8Unorm);
        assert!(
            !SCENE_FORMAT.is_srgb(),
            "the world shader writes the reference's display values directly"
        );
    }

    #[test]
    fn the_bloom_chain_is_raw_display_space() {
        assert_eq!(EMISSIVE_FORMAT, wgpu::TextureFormat::Rgba8Unorm);
        assert_eq!(BLOOM_FORMAT, wgpu::TextureFormat::Rgba8Unorm);
        assert!(
            !EMISSIVE_FORMAT.is_srgb() && !BLOOM_FORMAT.is_srgb(),
            "the emissive and blur targets are raw like the reference's RGBA8 chain"
        );
        assert!(
            DEPTH_FORMAT.has_depth_aspect(),
            "the scene target's depth is the shared main depth format"
        );
    }

    #[test]
    fn target_sizes_follow_the_level_and_never_upscale() {
        let drawable = DrawableSize::new(1920, 1080);
        // High: the drawable itself; bloom: a quarter of it; presented: the
        // drawable (the reference's default framebuffer).
        assert_eq!(
            target_sizes(QualityLevel::High, drawable),
            (
                DrawableSize::new(1920, 1080),
                DrawableSize::new(480, 270),
                DrawableSize::new(1920, 1080)
            )
        );
        // Medium: half the drawable, aspect kept; bloom a quarter of the scene;
        // the presented image stays the drawable.
        assert_eq!(
            target_sizes(QualityLevel::Medium, drawable),
            (
                DrawableSize::new(960, 540),
                DrawableSize::new(240, 135),
                DrawableSize::new(1920, 1080)
            )
        );
        // Low: the scene is no wider than 480, aspect kept; bloom is a quarter
        // of it; the presented image stays the drawable, because the reference
        // resolves and draws the HUD at default-framebuffer resolution.
        assert_eq!(
            target_sizes(QualityLevel::Low, drawable),
            (
                DrawableSize::new(480, 270),
                DrawableSize::new(120, 67),
                DrawableSize::new(1920, 1080)
            )
        );
        // The reference device is its own drawable under Low, and Medium
        // follows it rather than upscaling.
        assert_eq!(
            target_sizes(QualityLevel::Low, DrawableSize::new(480, 272)),
            (
                DrawableSize::new(480, 272),
                DrawableSize::new(120, 68),
                DrawableSize::new(480, 272)
            )
        );
        assert_eq!(
            target_sizes(QualityLevel::Medium, DrawableSize::new(480, 272)),
            (
                DrawableSize::new(480, 272),
                DrawableSize::new(120, 68),
                DrawableSize::new(480, 272)
            )
        );
        // A drawable already below the reference width is never upscaled.
        assert_eq!(
            target_sizes(QualityLevel::Low, DrawableSize::new(320, 180)),
            (
                DrawableSize::new(320, 180),
                DrawableSize::new(80, 45),
                DrawableSize::new(320, 180)
            )
        );
        // Tiny targets clamp each axis to one texel.
        assert_eq!(
            target_sizes(QualityLevel::High, DrawableSize::new(2, 3)),
            (
                DrawableSize::new(2, 3),
                DrawableSize::new(1, 1),
                DrawableSize::new(2, 3)
            )
        );
        assert_eq!(
            target_sizes(QualityLevel::High, DrawableSize::new(0, 0)),
            (
                DrawableSize::new(0, 0),
                DrawableSize::new(0, 0),
                DrawableSize::new(0, 0)
            )
        );
    }

    #[test]
    fn the_default_stats_are_an_empty_uncreated_set() {
        let stats = PostTargetStats::default();
        assert_eq!(stats.scene, DrawableSize::new(0, 0));
        assert_eq!(stats.bloom, DrawableSize::new(0, 0));
        assert_eq!(stats.presented, DrawableSize::new(0, 0));
        assert!(!stats.created);
    }

    #[test]
    fn the_blur_steps_one_source_texel() {
        // Pass 1 reads the scene-sized emissive image, horizontally.
        let horizontal = BlurParams::step(DrawableSize::new(960, 544), true);
        assert_eq!(horizontal.texel, [1.0 / 960.0, 0.0, 0.0, 0.0]);
        // Pass 2 reads quarter-size buffer A, vertically.
        let vertical = BlurParams::step(DrawableSize::new(240, 136), false);
        assert_eq!(vertical.texel, [0.0, 1.0 / 136.0, 0.0, 0.0]);
        // A degenerate source still steps by a whole texel, never by zero.
        let degenerate = BlurParams::step(DrawableSize::new(0, 0), true);
        assert_eq!(degenerate.texel, [1.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn the_blur_kernel_is_the_reference_five_tap() {
        // The reference GLSL's weights, verbatim: 0.375 centre, 0.25 one source
        // texel out, 0.0625 two out; the two tap distances are one and two
        // source texels. They sum to exactly one, and the shipped shader is
        // checked to carry the same numbers.
        let centre = 0.375_f32;
        let near = 0.25_f32;
        let far = 0.0625_f32;
        let steps = [1.0_f32, 2.0_f32];
        let sum = centre + 2.0 * near + 2.0 * far;
        assert!((sum - 1.0).abs() < f32::EPSILON, "kernel sums to {sum}");
        for token in [
            format!("* {centre:?}"),
            format!("* {near:?}"),
            format!("* {far:?}"),
            format!("blur.texel.xy * {:?}", steps[0]),
            format!("blur.texel.xy * {:?}", steps[1]),
            "return vec4<f32>(sum, 1.0);".to_string(),
        ] {
            assert!(
                WGSL.contains(&token),
                "the blur WGSL must contain {token:?}"
            );
        }
    }

    #[test]
    fn the_resolve_wgsl_is_the_reference_expression() {
        for token in [
            "if (post.bloom_strength > 0.0) {",
            "color *= post.exposure;",
            "vec3<f32>(post.tone_knee)",
            "max(1.0 - post.tone_knee, 1.0e-3)",
            "vec3<f32>(0.2126, 0.7152, 0.0722)",
            "mix(vec3<f32>(luma), color, post.grade_saturation)",
            "clamp((color - 0.5) * post.grade_contrast + 0.5, vec3<f32>(0.0), vec3<f32>(1.0))",
            "return vec4<f32>(srgb_to_linear(color), 1.0);",
        ] {
            assert!(
                WGSL.contains(token),
                "the resolve WGSL must contain {token:?}"
            );
        }
    }

    #[test]
    fn the_present_wgsl_is_the_reference_copy() {
        // gl_FragColor = vec4(texture2D(u_scene, v_uv).rgb, 1.0);
        assert!(WGSL.contains("fn fs_present"));
        assert!(
            WGSL.contains("let color = textureSample(source_texture, source_sampler, in.uv).rgb;")
        );
        assert!(WGSL.contains("return vec4<f32>(srgb_to_linear(color), 1.0);"));
        // The scene target is raw display space and the surface is sRGB, so the
        // only conversion is the output encode; the sample is never decoded.
        let present_start = WGSL.find("fn fs_present").unwrap();
        let present_end = WGSL.find("fn fs_blur").unwrap();
        let present = &WGSL[present_start..present_end];
        assert!(present.contains("srgb_to_linear("));
        assert!(!present.contains("linear_to_srgb("));
    }

    #[test]
    fn the_present_quad_is_the_reference_data() {
        // The reference quad's six (x, y) pairs, copied from the OpenGL
        // implementation as plain test data: this module may not import the
        // backend (see `render::boundary_tests`).
        let reference: Vec<[f32; 2]> = vec![
            [0.0, 0.0],
            [1.0, 0.0],
            [1.0, 1.0],
            [0.0, 0.0],
            [1.0, 1.0],
            [0.0, 1.0],
        ];
        assert_eq!(PRESENT_QUAD.to_vec(), reference);
        assert_eq!(QUAD_VERTEX_COUNT, 6);
        // The shader's clip mapping: uv (0, 0) is the top-left of the target,
        // uv (1, 1) the bottom-right, and the quad spans clip space exactly.
        let map = |uv: [f32; 2]| [uv[0] * 2.0 - 1.0, 1.0 - uv[1] * 2.0];
        assert_eq!(map([0.0, 0.0]), [-1.0, 1.0]);
        assert_eq!(map([1.0, 1.0]), [1.0, -1.0]);
        for corner in PRESENT_QUAD {
            let clip = map(corner);
            assert!((-1.0..=1.0).contains(&clip[0]));
            assert!((-1.0..=1.0).contains(&clip[1]));
        }
        assert!(WGSL.contains("1.0 - corner.y * 2.0"));
    }

    #[test]
    fn identity_settings_take_the_present_copy() {
        let high_off = PostSettings::for_level(QualityLevel::High).with_bloom(false);
        let high_on = PostSettings::for_level(QualityLevel::High).with_bloom(true);
        let medium_off = PostSettings::for_level(QualityLevel::Medium).with_bloom(false);
        let medium_on = PostSettings::for_level(QualityLevel::Medium).with_bloom(true);
        let low_off = PostSettings::for_level(QualityLevel::Low).with_bloom(false);
        let low_on = PostSettings::for_level(QualityLevel::Low).with_bloom(true);
        // Low without bloom is the neutral type's identity, so it must take the
        // plain copy no matter what the caller believes about bloom.
        assert!(low_off.is_identity());
        assert_eq!(resolve_path(low_off, false), ResolvePath::PresentCopy);
        assert_eq!(resolve_path(low_off, true), ResolvePath::PresentCopy);
        // High is never an identity: it resolves, with the bloom term gated by
        // the caller's bloom state.
        assert_eq!(resolve_path(high_off, false), ResolvePath::Resolve(4));
        assert_eq!(resolve_path(high_on, true), ResolvePath::Resolve(5));
        assert_eq!(resolve_path(high_on, false), ResolvePath::Resolve(4));
        // Medium always resolves too: the tone shoulder alone needs the pass.
        assert_eq!(resolve_path(medium_off, false), ResolvePath::Resolve(2));
        assert_eq!(resolve_path(medium_on, true), ResolvePath::Resolve(3));
        // Low with bloom resolves; without a valid bloom image it falls back to
        // the strength-zero Low parameters, exactly the reference's `(0.0, scene)`.
        assert_eq!(resolve_path(low_on, true), ResolvePath::Resolve(1));
        assert_eq!(resolve_path(low_on, false), ResolvePath::Resolve(0));
        // The six slots really are the level/bloom combinations, in order.
        assert_eq!(settings_slot(low_off), Some(0));
        assert_eq!(settings_slot(low_on), Some(1));
        assert_eq!(settings_slot(medium_off), Some(2));
        assert_eq!(settings_slot(medium_on), Some(3));
        assert_eq!(settings_slot(high_off), Some(4));
        assert_eq!(settings_slot(high_on), Some(5));
        assert_eq!(
            HIGH_RESOLVE_SLOT,
            settings_slot(high_off).unwrap_or(usize::MAX),
            "the fallback slot must be High's bloom-off slot"
        );
    }

    #[test]
    fn resolve_maths_mirror_the_shader() {
        // High's knee 0.75: identity at and below it, a shoulder above.
        assert!((tone_shoulder(0.5, 0.75) - 0.5).abs() < 1.0e-6);
        assert!((tone_shoulder(0.75, 0.75) - 0.75).abs() < 1.0e-6);
        assert!((tone_shoulder(1.0, 0.75) - 0.875).abs() < 1.0e-6);
        assert!((tone_shoulder(2.0, 0.75) - 0.958_333_3).abs() < 1.0e-5);
        // Low's knee 1.0 with the span floor keeps the curve continuous.
        assert!((tone_shoulder(2.0, 1.0) - 1.000_999).abs() < 1.0e-5);

        // Unit grade is the identity.
        let plain = grade([0.8, 0.4, 0.2], 1.0, 1.0);
        for (actual, expected) in plain.iter().zip([0.8, 0.4, 0.2]) {
            assert!((actual - expected).abs() < 1.0e-6);
        }
        // High's grade: saturation away from the luma, then contrast away from
        // mid grey.
        let graded = grade([0.8, 0.4, 0.2], 1.03, 1.02);
        let expected = [0.816_08, 0.395_84, 0.185_72];
        for (actual, expected) in graded.iter().zip(expected) {
            assert!(
                (actual - expected).abs() < 1.0e-5,
                "graded {graded:?} must match {expected}"
            );
        }
        // The contrast clamp keeps an over-white channel at one.
        let clamped = grade([1.5, 1.5, 1.5], 1.0, 1.02);
        for channel in clamped {
            assert_eq!(channel, 1.0);
        }
    }

    #[test]
    fn the_uniform_layouts_are_aligned_slots() {
        assert_eq!(POST_PARAMS_SIZE, 32);
        assert_eq!(BLUR_PARAMS_SIZE, 16);
        assert_eq!(std::mem::align_of::<PostParams>(), 4);
        assert_eq!(std::mem::align_of::<BlurParams>(), 4);
        // The shader declares the same field order.
        for token in [
            "bloom_strength: f32,",
            "exposure: f32,",
            "tone_knee: f32,",
            "grade_saturation: f32,",
            "grade_contrast: f32,",
            "texel: vec4<f32>,",
        ] {
            assert!(WGSL.contains(token), "the WGSL must contain {token:?}");
        }
    }

    #[test]
    fn the_wgsl_declares_the_bindings_the_layouts_use() {
        for binding in 0..6 {
            let declaration = format!("@group(0) @binding({binding})");
            assert!(
                WGSL.contains(&declaration),
                "the WGSL must declare {declaration}"
            );
        }
        // The entry points the pipelines name really exist.
        for entry in [
            POST_VERTEX_ENTRY,
            POST_RESOLVE_RAW_FRAGMENT_ENTRY,
            POST_PRESENT_FRAGMENT_ENTRY,
            POST_BLUR_FRAGMENT_ENTRY,
        ] {
            assert!(WGSL.contains(entry), "the WGSL must define {entry}");
        }
    }
}
