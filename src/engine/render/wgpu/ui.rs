//! Renderer-owned UI: the 480x272 HUD pass.
//!
//! The HUD uses a 480x272 reference space, shared world vertex layout,
//! font atlas, centred aspect-fit content, straight-alpha blend
//! and no depth test.
//!
//! A dedicated pipeline keeps the HUD's neutral material state independent
//! of the scene passes. Its fragment stage is exactly
//! `tex_color * v_color` in raw display space (the presented target is raw; the
//! surface-facing fallback entry point converts once with `srgb_to_linear`),
//! one font texture uploaded once, and bounded image draws per submission. The UI vertex list is the
//! renderer-neutral [`Vertex`] the application builds in UI pixels; it is
//! converted to the shared [`WorldVertex`] here, so the HUD is quantised and
//! laid out exactly like a scene vertex.
//!
//! The pipeline, bind groups and font atlas are construction-time work. The
//! camera projection updates only when drawable dimensions change, and the vertex
//! buffer is recreated only when a submission outgrows it.

use glam::{Mat4, Vec3};

use super::gpu_timing::GpuTimer;
use super::texture::SamplerPolicy;
use super::world::{
    CAMERA_UNIFORM_SIZE, CameraUniform, WORLD_VERTEX_STRIDE, WorldVertex, clip_correction,
    world_vertex_layout,
};
use crate::engine::font::{font_atlas_dimensions, generate_font_atlas};
use crate::engine::render::common::mesh::Vertex;
use crate::engine::render::common::view::{DrawableSize, dimension_f32};

#[cfg(test)]
#[path = "../../tests/menu_edges.rs"]
mod tests;

/// Whether this UI pass is followed by a surface copy or completes the frame.
pub enum UiTiming<'a> {
    Continue(&'a mut GpuTimer),
    Finish(&'a mut GpuTimer),
}

impl UiTiming<'_> {
    fn pass_writes(&self) -> Option<wgpu::RenderPassTimestampWrites<'_>> {
        match self {
            Self::Continue(timer) | Self::Finish(timer) => timer.pass_writes("ui"),
        }
    }
}

/// The UI shader, from the file next to this module.
pub const UI_SHADER_SRC: &str = include_str!("ui.wgsl");

/// Name of the UI vertex entry point.
pub const UI_VERTEX_ENTRY: &str = "vs_main";
/// Name of the UI fragment entry point.
pub const UI_FRAGMENT_ENTRY: &str = "fs_main";
/// The fragment entry point for a raw (non-sRGB) presented target: display-space
/// values written directly so hardware alpha blending matches the reference's
/// default-framebuffer blend.
pub const UI_FRAGMENT_ENTRY_RAW: &str = "fs_main_raw";

/// The font atlas is raw display-space artwork, not an sRGB-encoded image.
///
/// The reference keeps its font sheet in its non-sRGB framebuffer's own value
/// space, so the shader multiplies the sampled texels directly and performs the
/// one conversion at the very end. Uploading the atlas as `Rgba8UnormSrgb`
/// would decode the glyph RGB before the multiply and land the result on a
/// different curve than the reference; it stays linear-format here.
const UI_FONT_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// The reference's UI blend state, exactly.
///
/// The OpenGL reference calls `glBlendFunc(SRC_ALPHA, ONE_MINUS_SRC_ALPHA)`
/// once, which applies the same factors and `FUNC_ADD` to both the colour and
/// the alpha channel; the alpha output is straight, never premultiplied.
/// wgpu's convenience `BlendState::ALPHA_BLENDING` uses
/// `ONE`/`ONE_MINUS_SRC_ALPHA` for the alpha channel instead, so the state is
/// written out explicitly rather than approximated.
const UI_BLEND: wgpu::BlendState = wgpu::BlendState {
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

/// The reference UI ortho expanded to the drawable edges, with the world
/// path's one clip-space correction. The aspect-fit scale/origin are preserved
/// without clipping extended backdrops at the inner viewport boundary.
///
/// The reference calls
/// `glam::camera::rh::proj::directx::orthographic(0.0, 480.0, 272.0, 0.0, -1.0, 1.0)`, so UI
/// (0, 0) maps to the aspect-fit region's top-left and (480, 272) to its
/// bottom-right: the reference space has a top-left origin and +y points down.
/// On a reference-aspect drawable these are NDC `(-1, +1)` and `(1, -1)`.
/// WebGPU's NDC is +y up and
/// its framebuffer Y flip is the viewport transform's job, exactly like
/// OpenGL's, so the same projection maps 1:1 — negating y here would mirror the
/// HUD.
///
/// The only OpenGL -> wgpu correction the world path applies is
/// [`clip_correction`], and it only remaps z (`z' = 0.5 z + 0.5`, `w' = w`);
/// x and y pass through untouched. [`glam::camera::rh::proj::directx::orthographic`] is glam's
/// WebGPU/Direct3D depth variant already, so the corrected z lands in `[0, 1]`
/// (not GL's `[-1, 1]`); the UI pass carries no depth attachment at all, so z
/// is never read. The correction is applied anyway so the HUD shares the one
/// documented coordinate conversion every wgpu pass uses.
#[must_use]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "glam matrix multiplication uses floating point components without integer overflow or a panic path."
)]
pub fn ui_view_projection(drawable: DrawableSize) -> Mat4 {
    // `glam` matrix products are per-element `f32` arithmetic with no overflow
    // or panic path.
    let [left, top, right, bottom] = drawable.ui_bounds();
    clip_correction()
        * glam::camera::rh::proj::directx::orthographic(left, right, bottom, top, -1.0, 1.0)
}

/// True when a submission of `vertex_count` vertices into `drawable` can draw.
///
/// Pure and device-free so the zero-size rule is testable: the reference
/// returns immediately on an empty vertex list or an empty drawable
/// (`if ui_vertices.is_empty() || drawable.is_empty() { return; }`), and this
/// pass does the same instead of encoding an empty draw.
#[must_use]
pub const fn ui_is_drawable(drawable: DrawableSize, vertex_count: usize) -> bool {
    !drawable.is_empty() && vertex_count > 0
}

/// What one UI submission did, for the neutral `RenderStats` and the benchmark
/// report.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UiStats {
    /// Draw calls issued (font geometry plus visible PNGs).
    pub draw_calls: usize,
    /// Vertices the one draw covers.
    pub visible_vertices: usize,
}

/// The UI pipeline and its font atlas.
///
/// Built once per colour target format and kept: a changed surface format
/// (a recreated surface) rebuilds only this resource, exactly like
/// [`super::world::WorldPipeline`]. The camera uniform holds the UI ortho and
/// updates on resize; the font atlas is uploaded once and bound
/// through a dedicated group, so no frame touches a pipeline, a bind group or
/// a texture.
struct Picture {
    source: crate::engine::render::UiImage,
    binding: wgpu::BindGroup,
}

pub struct UiRenderer {
    pictures: Vec<Picture>,
    pipeline: wgpu::RenderPipeline,
    /// Persistent UI ortho binding; its buffer updates on drawable resize.
    camera_bind_group: wgpu::BindGroup,
    camera_buffer: wgpu::Buffer,
    camera_drawable: DrawableSize,
    /// The generated font atlas's binding; the texture and view are owned by
    /// the bind group.
    font_bind_group: wgpu::BindGroup,
    /// The colour target format the pipeline was built for.
    format: wgpu::TextureFormat,
    /// The reusable vertex buffer, allocated on the first non-empty
    /// submission and grown only when a later list outgrows it.
    vertex_buffer: Option<wgpu::Buffer>,
    /// Vertices `vertex_buffer` can hold.
    vertex_capacity: usize,
    /// The per-frame conversion scratch: the renderer-neutral list becomes the
    /// shared GPU vertex exactly once per submission, never per vertex per
    /// frame into a fresh allocation.
    scratch: Vec<WorldVertex>,
}

impl UiRenderer {
    /// Builds the pipeline for one target colour format and uploads the font
    /// atlas.
    ///
    /// All of the GPU work here is construction-time: the shader module, the
    /// pipeline, the camera uniform (the UI ortho, updated on resize), the font
    /// texture and sampler, and the two bind groups. Nothing here runs per
    /// frame.
    #[must_use]
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        editor: bool,
    ) -> Self {
        let font_view = upload_font_atlas(device, queue, editor);

        let (camera_bind_group, camera_layout, camera_buffer) =
            create_camera_binding(device, queue);
        let (font_bind_group, font_layout) = create_font_binding(device, &font_view, editor);
        let pipeline = create_pipeline(device, format, &camera_layout, &font_layout);

        Self {
            pictures: Vec::new(),
            pipeline,
            camera_bind_group,
            camera_buffer,
            camera_drawable: DrawableSize::new(480, 272),
            font_bind_group,
            format,
            vertex_buffer: None,
            vertex_capacity: 0,
            scratch: Vec::new(),
        }
    }

    pub fn set_images(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        images: &[crate::engine::render::UiImage],
    ) {
        if images.len() == self.pictures.len()
            && images
                .iter()
                .zip(&self.pictures)
                .all(|(a, b)| std::sync::Arc::ptr_eq(&a.image, &b.source.image))
        {
            for (picture, image) in self.pictures.iter_mut().zip(images) {
                picture.source.bounds = image.bounds;
            }
            return;
        }
        self.pictures = images
            .iter()
            .take(24)
            .map(|source| {
                let view = upload_ui_image(device, queue, &source.image);
                let (binding, _layout) = create_font_binding(device, &view, true);
                Picture {
                    source: source.clone(),
                    binding,
                }
            })
            .collect();
    }

    /// Encodes and submits the UI pass into `target`.
    ///
    /// `target` is the surface texture view the scene pass presented into, so
    /// the pass only loads and blends over it: it clears nothing and draws its
    /// triangles only, exactly like the reference's HUD draw after the scene
    /// resolve. `drawable` selects the projection through the renderer-neutral
    /// [`DrawableSize::ui_bounds`], preserving the centred aspect-fit 480x272
    /// region; an empty drawable or an empty vertex list encodes nothing and
    /// reports no draws.
    ///
    /// The vertex buffer is created on the first non-empty submission and
    /// recreated only when a later list outgrows it. The camera uniform updates
    /// on resize; the pipeline, bind groups and font texture remain unchanged.
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        vertices: &[Vertex],
        drawable: DrawableSize,
        timing: Option<UiTiming<'_>>,
    ) -> UiStats {
        if !ui_is_drawable(drawable, vertices.len()) {
            return UiStats::default();
        }
        if self.camera_drawable != drawable {
            let uniform = CameraUniform::new(ui_view_projection(drawable), Vec3::ZERO);
            queue.write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&uniform));
            self.camera_drawable = drawable;
        }

        self.scratch.clear();
        self.scratch.extend(vertices.iter().map(WorldVertex::from));
        for picture in &self.pictures {
            self.scratch.extend(
                picture_vertices(picture.source.bounds)
                    .iter()
                    .map(WorldVertex::from),
            );
        }
        self.ensure_vertex_buffer(device);
        if let Some(buffer) = self.vertex_buffer.as_ref() {
            queue.write_buffer(buffer, 0, bytemuck::cast_slice(&self.scratch));
        }

        let count = u32::try_from(vertices.len()).unwrap_or(u32::MAX);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("places-wgpu-ui"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("places-wgpu-ui"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        // The scene is already there: the HUD blends over it.
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: timing.as_ref().and_then(UiTiming::pass_writes),
                occlusion_query_set: None,
                multiview_mask: None,
            });
            // Projection retains the inner UI scale/origin while the full
            // drawable viewport lets extended backdrops reach the screen edge.
            pass.set_viewport(
                0.0,
                0.0,
                dimension_f32(drawable.width),
                dimension_f32(drawable.height),
                0.0,
                1.0,
            );
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.camera_bind_group, &[]);
            pass.set_bind_group(1, &self.font_bind_group, &[]);
            if let Some(buffer) = self.vertex_buffer.as_ref() {
                pass.set_vertex_buffer(0, buffer.slice(..));
            }
            pass.draw(0..count, 0..1);
            for (index, picture) in self.pictures.iter().enumerate() {
                let first =
                    count.saturating_add(u32::try_from(index).unwrap_or(0).saturating_mul(6));
                pass.set_bind_group(1, &picture.binding, &[]);
                pass.draw(first..first.saturating_add(6), 0..1);
            }
        }
        let _submission = queue.submit([encoder.finish()]);
        if let Some(UiTiming::Finish(timer)) = timing {
            timer.finish_frame_submitted(queue);
        }

        UiStats {
            draw_calls: 1_usize.saturating_add(self.pictures.len()),
            visible_vertices: self.scratch.len(),
        }
    }

    /// Allocates the vertex buffer when the converted list outgrows it.
    ///
    /// Growth is to the next power of two, so a UI that oscillates around a
    /// size between frames stops reallocating; a list that fits the current
    /// buffer writes in place.
    fn ensure_vertex_buffer(&mut self, device: &wgpu::Device) {
        let required = self.scratch.len();
        if required == 0 || required <= self.vertex_capacity {
            return;
        }
        let capacity = required.checked_next_power_of_two().unwrap_or(required);
        let size = u64::try_from(capacity)
            .unwrap_or(u64::MAX)
            .saturating_mul(WORLD_VERTEX_STRIDE)
            .max(4);
        self.vertex_buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("places-wgpu-ui-vertices"),
            size,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }));
        self.vertex_capacity = capacity;
    }

    /// The colour target format the pipeline was built for.
    #[must_use]
    pub const fn format(&self) -> wgpu::TextureFormat {
        self.format
    }
}

/// Creates the font atlas texture, fills it from the committed PNG pixels and
/// returns its view. The bind group built from the view owns the texture.
///
/// The game loads its unchanged 256x128 HUD/hand sheet; the editor selects
/// the independent antialiased sheet. Both decode once at construction.
fn upload_font_atlas(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    editor: bool,
) -> wgpu::TextureView {
    let (font_dimensions, pixels) = if editor {
        let image = crate::engine::materials::BuiltinImage::EditorFont.decode();
        ((image.width, image.height), image.rgba)
    } else {
        (font_atlas_dimensions(), generate_font_atlas())
    };
    upload_ui_image(
        device,
        queue,
        &crate::engine::materials::RawImage::new(font_dimensions.0, font_dimensions.1, pixels),
    )
}

fn picture_vertices([left, top, width, height]: [f32; 4]) -> [Vertex; 6] {
    [
        ([left, top, 0.0], [0.0, 0.0]),
        ([left + width, top, 0.0], [1.0, 0.0]),
        ([left + width, top + height, 0.0], [1.0, 1.0]),
        ([left, top, 0.0], [0.0, 0.0]),
        ([left + width, top + height, 0.0], [1.0, 1.0]),
        ([left, top + height, 0.0], [0.0, 1.0]),
    ]
    .map(|(pos, uv)| Vertex {
        pos,
        uv,
        color: [1.0; 4],
        ..Vertex::UNLIT
    })
}

fn upload_ui_image(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    image: &crate::engine::materials::RawImage,
) -> wgpu::TextureView {
    let font_dimensions = (image.width, image.height);
    let font_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("places-wgpu-ui-font"),
        size: wgpu::Extent3d {
            width: font_dimensions.0,
            height: font_dimensions.1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: UI_FONT_FORMAT,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let font_view = font_texture.create_view(&wgpu::TextureViewDescriptor::default());
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &font_texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &image.rgba,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(font_dimensions.0.saturating_mul(4)),
            rows_per_image: Some(font_dimensions.1),
        },
        wgpu::Extent3d {
            width: font_dimensions.0,
            height: font_dimensions.1,
            depth_or_array_layers: 1,
        },
    );
    font_view
}

/// Creates the camera layout, resizeable UI ortho uniform and their bind group.
fn create_camera_binding(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> (wgpu::BindGroup, wgpu::BindGroupLayout, wgpu::Buffer) {
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("places-wgpu-ui-camera-layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            // The UI fragment stage reads no camera state, so only the vertex
            // stage sees the uniform.
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: wgpu::BufferSize::new(CAMERA_UNIFORM_SIZE),
            },
            count: None,
        }],
    });
    let uniform = CameraUniform::new(ui_view_projection(DrawableSize::new(480, 272)), Vec3::ZERO);
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("places-wgpu-ui-camera"),
        size: CAMERA_UNIFORM_SIZE,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&buffer, 0, bytemuck::bytes_of(&uniform));
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("places-wgpu-ui-camera"),
        layout: &layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: buffer.as_entire_binding(),
        }],
    });
    (bind_group, layout, buffer)
}

/// Creates the font layout (texture + sampler) and binds the uploaded atlas.
///
/// The HUD keeps nearest sampling. Editor outline glyphs use clamped linear
/// coverage; neither sheet has mipmaps or any world-quality dependency.
fn create_font_binding(
    device: &wgpu::Device,
    view: &wgpu::TextureView,
    editor: bool,
) -> (wgpu::BindGroup, wgpu::BindGroupLayout) {
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("places-wgpu-ui-font-layout"),
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
    });
    let policy = if editor {
        SamplerPolicy::ClampLinear
    } else {
        SamplerPolicy::ClampNearest
    };
    let sampler = device.create_sampler(&policy.descriptor());
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("places-wgpu-ui-font"),
        layout: &layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    });
    (bind_group, layout)
}

/// Builds the one UI pipeline from the shared layouts and the UI shader.
///
/// The whole state is the reference's neutral HUD state: the world vertex
/// layout, no culling, no depth attachment, and the straight-alpha blend on the
/// one colour target.
fn create_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    camera_layout: &wgpu::BindGroupLayout,
    font_layout: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("places-wgpu-ui-shader"),
        source: wgpu::ShaderSource::Wgsl(UI_SHADER_SRC.into()),
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("places-wgpu-ui-pipeline-layout"),
        bind_group_layouts: &[Some(camera_layout), Some(font_layout)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("places-wgpu-ui"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some(UI_VERTEX_ENTRY),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            // The same vertex layout the world draws: the HUD is the world's
            // own `Vertex`/`WorldVertex` pair, quantised and laid out
            // identically.
            buffers: &[Some(world_vertex_layout())],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            // The reference never enables `GL_CULL_FACE` anywhere.
            cull_mode: None,
            unclipped_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        },
        // The reference disables the depth test for the HUD and the pass
        // carries no depth attachment, so no depth state exists to differ from
        // `glDisable(GL_DEPTH_TEST)`.
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some(if format.is_srgb() {
                UI_FRAGMENT_ENTRY
            } else {
                UI_FRAGMENT_ENTRY_RAW
            }),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(UI_BLEND),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}
