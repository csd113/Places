//! Stage 9 renderer-owned UI: the 480x272 HUD pass.
//!
//! The port of the reference renderer's `Renderer::render_ui`: the same
//! 480x272 reference space, the
//! same world vertex layout, the same generated font atlas, the same centred
//! uniform-scale viewport, the same straight-alpha blend and no depth test.
//!
//! The reference draws the HUD with its World program reset to a neutral
//! material state; this module is that state made explicit instead of
//! inherited: one pipeline whose fragment stage is exactly
//! `tex_color * v_color` in raw display space (the presented target is raw; the
//! surface-facing fallback entry point converts once with `srgb_to_linear`),
//! one font texture uploaded once, and one draw call per submission. The UI vertex list is the
//! renderer-neutral [`Vertex`] the application builds in UI pixels; it is
//! converted to the shared [`WorldVertex`] here, so the HUD is quantised and
//! laid out exactly like a scene vertex.
//!
//! Nothing is regenerated per frame: the pipeline, the bind groups, the camera
//! uniform and the font atlas are construction-time work, and the vertex
//! buffer is recreated only when a submission outgrows it.

use glam::{Mat4, Vec3};

use super::texture::SamplerPolicy;
use super::world::{
    CAMERA_UNIFORM_SIZE, CameraUniform, WORLD_VERTEX_STRIDE, WorldVertex, clip_correction,
    world_vertex_layout,
};
use crate::font::{font_atlas_dimensions, generate_font_atlas};
use crate::render::common::mesh::Vertex;
use crate::render::common::view::{
    DrawableSize, UI_REFERENCE_HEIGHT, UI_REFERENCE_WIDTH, dimension_f32,
};

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

/// The UI projection: the reference's exact ortho, with the world path's one
/// clip-space correction.
///
/// The reference calls
/// `glam::Mat4::orthographic_rh(0.0, 480.0, 272.0, 0.0, -1.0, 1.0)`, so UI
/// (0, 0) maps to NDC `(-1, +1)` and UI (480, 272) to `(1, -1)`: the reference
/// space has a top-left origin and +y points down. WebGPU's NDC is +y up and
/// its framebuffer Y flip is the viewport transform's job, exactly like
/// OpenGL's, so the same projection maps 1:1 — negating y here would mirror the
/// HUD.
///
/// The only OpenGL -> wgpu correction the world path applies is
/// [`clip_correction`], and it only remaps z (`z' = 0.5 z + 0.5`, `w' = w`);
/// x and y pass through untouched. [`Mat4::orthographic_rh`] is glam's
/// WebGPU/Direct3D depth variant already, so the corrected z lands in `[0, 1]`
/// (not GL's `[-1, 1]`); the UI pass carries no depth attachment at all, so z
/// is never read. The correction is applied anyway so the HUD shares the one
/// documented coordinate conversion every wgpu pass uses.
#[must_use]
#[allow(clippy::arithmetic_side_effects)]
pub fn ui_view_projection() -> Mat4 {
    // `glam` matrix products are per-element `f32` arithmetic with no overflow
    // or panic path.
    clip_correction()
        * Mat4::orthographic_rh(
            0.0,
            dimension_f32(UI_REFERENCE_WIDTH),
            dimension_f32(UI_REFERENCE_HEIGHT),
            0.0,
            -1.0,
            1.0,
        )
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

/// One integer viewport field as the `f32` `RenderPass::set_viewport` takes.
///
/// The fields come from [`DrawableSize::ui_viewport`], whose values are
/// non-negative and clamped to the drawable, so the conversion is exact for
/// every real window; a value outside that range clamps to zero rather than
/// wrapping.
fn viewport_f32(value: i32) -> f32 {
    dimension_f32(u32::try_from(value).unwrap_or(0))
}

/// What one UI submission did, for the neutral `RenderStats` and the benchmark
/// report.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UiStats {
    /// `draw` calls issued (zero or one).
    pub draw_calls: usize,
    /// Vertices the one draw covers.
    pub visible_vertices: usize,
}

/// The UI pipeline and its font atlas.
///
/// Built once per colour target format and kept: a changed surface format
/// (a recreated surface) rebuilds only this resource, exactly like
/// [`super::world::WorldPipeline`]. The camera uniform holds the UI ortho and
/// is written once at construction; the font atlas is uploaded once and bound
/// through a dedicated group, so no frame touches a pipeline, a bind group or
/// a texture.
pub struct UiRenderer {
    pipeline: wgpu::RenderPipeline,
    /// The construction-time UI ortho binding; the buffer it binds is owned by
    /// the bind group.
    camera_bind_group: wgpu::BindGroup,
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
    /// pipeline, the camera uniform (the UI ortho, written once), the font
    /// texture and sampler, and the two bind groups. Nothing here runs per
    /// frame.
    #[must_use]
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let font_view = upload_font_atlas(device, queue);

        let (camera_bind_group, camera_layout) = create_camera_binding(device, queue);
        let (font_bind_group, font_layout) = create_font_binding(device, &font_view);
        let pipeline = create_pipeline(device, format, &camera_layout, &font_layout);

        Self {
            pipeline,
            camera_bind_group,
            font_bind_group,
            format,
            vertex_buffer: None,
            vertex_capacity: 0,
            scratch: Vec::new(),
        }
    }

    /// Encodes and submits the UI pass into `target`.
    ///
    /// `target` is the surface texture view the scene pass presented into, so
    /// the pass only loads and blends over it: it clears nothing and draws its
    /// triangles only, exactly like the reference's HUD draw after the scene
    /// resolve. `drawable` selects the viewport through the renderer-neutral
    /// [`DrawableSize::ui_viewport`] (the centred uniform-scale 480x272
    /// region); an empty drawable or an empty vertex list encodes nothing and
    /// reports no draws.
    ///
    /// The vertex buffer is created on the first non-empty submission and
    /// recreated only when a later list outgrows it; the pipeline, bind groups,
    /// camera uniform and font texture are never touched here.
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        vertices: &[Vertex],
        drawable: DrawableSize,
    ) -> UiStats {
        if !ui_is_drawable(drawable, vertices.len()) {
            return UiStats::default();
        }
        let viewport = drawable.ui_viewport();

        self.scratch.clear();
        self.scratch.extend(vertices.iter().map(WorldVertex::from));
        self.ensure_vertex_buffer(device);
        if let Some(buffer) = self.vertex_buffer.as_ref() {
            queue.write_buffer(buffer, 0, bytemuck::cast_slice(&self.scratch));
        }

        let count = u32::try_from(self.scratch.len()).unwrap_or(u32::MAX);
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
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            // The region is centred, so the bottom-left origin the neutral
            // viewport maths reports is numerically the top-left origin
            // WebGPU's `set_viewport` takes.
            pass.set_viewport(
                viewport_f32(viewport.x),
                viewport_f32(viewport.y),
                viewport_f32(viewport.width),
                viewport_f32(viewport.height),
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
        }
        queue.submit([encoder.finish()]);

        UiStats {
            draw_calls: 1,
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

/// Creates the font atlas texture, fills it from the generated pixels and
/// returns its view. The bind group built from the view owns the texture.
///
/// `generate_font_atlas` is the one font source the project has: a 128x64 RGBA8
/// sheet with the reserved white cell and the 8x8 glyphs. It is generated and
/// uploaded once, at construction.
fn upload_font_atlas(device: &wgpu::Device, queue: &wgpu::Queue) -> wgpu::TextureView {
    let font_dimensions = font_atlas_dimensions();
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
        &generate_font_atlas(),
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

/// Creates the camera layout, the uniform holding the UI ortho (written once
/// here) and their bind group.
fn create_camera_binding(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> (wgpu::BindGroup, wgpu::BindGroupLayout) {
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
    let uniform = CameraUniform::new(ui_view_projection(), Vec3::ZERO);
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
    (bind_group, layout)
}

/// Creates the font layout (texture + sampler) and binds the uploaded atlas.
///
/// The reference uploads the font atlas clamped and nearest, with no mip
/// chain; [`SamplerPolicy::ClampNearest`] is exactly that shared policy.
fn create_font_binding(
    device: &wgpu::Device,
    view: &wgpu::TextureView,
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
    let sampler = device.create_sampler(&SamplerPolicy::ClampNearest.descriptor());
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

#[cfg(test)]
mod tests {
    // Test code: unwrap/indexing/float comparisons and panics are idiomatic here.
    #![allow(
        clippy::arithmetic_side_effects,
        clippy::expect_used,
        clippy::float_cmp,
        clippy::indexing_slicing,
        clippy::panic,
        clippy::suboptimal_flops,
        clippy::unwrap_used
    )]

    use super::*;

    /// Project one UI reference point to NDC through the UI ortho.
    fn ndc(x: f32, y: f32) -> [f32; 3] {
        let clip = ui_view_projection() * glam::Vec4::new(x, y, 0.0, 1.0);
        [clip.x / clip.w, clip.y / clip.w, clip.z / clip.w]
    }

    #[test]
    fn the_ui_ortho_maps_the_reference_corners_top_left_first() {
        let top_left = ndc(0.0, 0.0);
        assert!((top_left[0] + 1.0).abs() < 1.0e-6, "{top_left:?}");
        assert!((top_left[1] - 1.0).abs() < 1.0e-6, "{top_left:?}");
        let bottom_right = ndc(
            dimension_f32(UI_REFERENCE_WIDTH),
            dimension_f32(UI_REFERENCE_HEIGHT),
        );
        assert!((bottom_right[0] - 1.0).abs() < 1.0e-6, "{bottom_right:?}");
        assert!((bottom_right[1] + 1.0).abs() < 1.0e-6, "{bottom_right:?}");
        // WebGPU NDC is +y up and its viewport transform flips into framebuffer
        // coordinates, so NDC +1 is the framebuffer top: UI (0, 0) is the
        // top-left of the HUD, and a larger UI y is lower on screen.
        assert!(top_left[1] > bottom_right[1]);
        // The clip correction only remaps z; nothing lands outside clip space.
        assert!(
            (top_left[2] - bottom_right[2]).abs() < 1.0e-6,
            "{top_left:?}"
        );
        assert!(top_left[2] >= 0.0 && top_left[2] <= 1.0, "{top_left:?}");
    }

    #[test]
    fn the_clip_correction_touches_z_alone() {
        let reference = Mat4::orthographic_rh(
            0.0,
            dimension_f32(UI_REFERENCE_WIDTH),
            dimension_f32(UI_REFERENCE_HEIGHT),
            0.0,
            -1.0,
            1.0,
        );
        let corrected = ui_view_projection();
        assert_eq!(corrected.x_axis, reference.x_axis);
        assert_eq!(corrected.y_axis, reference.y_axis);
        // The z translation lives in the w column; its x and y entries are the
        // top-left origin's translation and must survive.
        assert_eq!(corrected.w_axis.x, reference.w_axis.x);
        assert_eq!(corrected.w_axis.y, reference.w_axis.y);
        assert_ne!(corrected.z_axis, reference.z_axis);
    }

    #[test]
    fn the_ui_viewport_is_the_neutral_centred_region_as_f32() {
        // Equal drawable and reference: the whole window.
        let exact = DrawableSize::new(960, 544).ui_viewport();
        assert_eq!(
            (exact.x, exact.y, exact.width, exact.height),
            (0, 0, 960, 544)
        );
        assert_eq!(
            (
                viewport_f32(exact.x),
                viewport_f32(exact.y),
                viewport_f32(exact.width),
                viewport_f32(exact.height)
            ),
            (0.0, 0.0, 960.0, 544.0)
        );
        // 1280x720 is height-limited: the 272-tall reference fills the height
        // exactly, the 480-wide region rounds to 1271 and is centred with
        // 4-pixel margins.
        let wide = DrawableSize::new(1280, 720).ui_viewport();
        assert_eq!((wide.x, wide.y, wide.width, wide.height), (4, 0, 1271, 720));
        assert_eq!(
            (
                viewport_f32(wide.x),
                viewport_f32(wide.y),
                viewport_f32(wide.width),
                viewport_f32(wide.height)
            ),
            (4.0, 0.0, 1271.0, 720.0)
        );
        // 1000x1000 is width-limited: the region sits centred vertically. The
        // region is symmetric, so the bottom-left origin the GL-style maths
        // reports is the same number `set_viewport` takes from the top.
        let tall = DrawableSize::new(1000, 1000).ui_viewport();
        assert_eq!(
            (tall.x, tall.y, tall.width, tall.height),
            (0, 216, 1000, 567)
        );
        assert_eq!(
            (
                viewport_f32(tall.x),
                viewport_f32(tall.y),
                viewport_f32(tall.width),
                viewport_f32(tall.height)
            ),
            (0.0, 216.0, 1000.0, 567.0)
        );
    }

    #[test]
    fn a_zero_size_drawable_or_empty_list_draws_nothing() {
        assert!(!ui_is_drawable(DrawableSize::new(0, 0), 6));
        assert!(!ui_is_drawable(DrawableSize::new(0, 272), 6));
        assert!(!ui_is_drawable(DrawableSize::new(480, 0), 6));
        assert!(!ui_is_drawable(DrawableSize::new(480, 272), 0));
        assert!(ui_is_drawable(DrawableSize::new(480, 272), 6));
        assert_eq!(UiStats::default().draw_calls, 0);
        assert_eq!(UiStats::default().visible_vertices, 0);
    }

    #[test]
    fn the_blend_is_the_reference_blend_func_on_colour_and_alpha() {
        let component =
            |part: wgpu::BlendComponent| (part.src_factor, part.dst_factor, part.operation);
        let expected = (
            wgpu::BlendFactor::SrcAlpha,
            wgpu::BlendFactor::OneMinusSrcAlpha,
            wgpu::BlendOperation::Add,
        );
        assert_eq!(component(UI_BLEND.color), expected);
        assert_eq!(component(UI_BLEND.alpha), expected);
    }

    #[test]
    fn the_font_atlas_is_display_space_nearest_and_clamped() {
        assert_eq!(UI_FONT_FORMAT, wgpu::TextureFormat::Rgba8Unorm);
        assert!(!UI_FONT_FORMAT.is_srgb());
        let descriptor = SamplerPolicy::ClampNearest.descriptor();
        assert_eq!(descriptor.address_mode_u, wgpu::AddressMode::ClampToEdge);
        assert_eq!(descriptor.address_mode_v, wgpu::AddressMode::ClampToEdge);
        assert_eq!(descriptor.mag_filter, wgpu::FilterMode::Nearest);
        assert_eq!(descriptor.min_filter, wgpu::FilterMode::Nearest);
        // The atlas is the generated 128x64 sheet, and its pixels are exactly
        // one RGBA8 texel per texel of the declared size.
        let (width, height) = font_atlas_dimensions();
        assert_eq!((width, height), (128, 64));
        assert_eq!(
            generate_font_atlas().len(),
            usize::try_from(width.saturating_mul(height).saturating_mul(4)).unwrap()
        );
    }

    #[test]
    fn the_ui_shader_is_the_plain_ui_contract() {
        assert!(UI_SHADER_SRC.contains(UI_VERTEX_ENTRY));
        assert!(UI_SHADER_SRC.contains(UI_FRAGMENT_ENTRY));
        assert!(UI_SHADER_SRC.contains("@group(0) @binding(0)"));
        assert!(UI_SHADER_SRC.contains("@group(1) @binding(0)"));
        // The whole fragment: font x colour in raw display space, the IEC
        // decode for the sRGB target, and a straight alpha untouched by the
        // colour conversion.
        assert!(UI_SHADER_SRC.contains("let display = tex_color.rgb * in.color.rgb;"));
        assert!(UI_SHADER_SRC.contains("let alpha = tex_color.a * in.color.a;"));
        assert!(UI_SHADER_SRC.contains("srgb_to_linear(display)"));
        // The same IEC 61966-2-1 constants the world shader's conversion uses.
        assert!(UI_SHADER_SRC.contains("12.92"));
        assert!(UI_SHADER_SRC.contains("0.04045"));
        assert!(UI_SHADER_SRC.contains("1.055"));
        assert!(UI_SHADER_SRC.contains("0.055"));
        assert!(UI_SHADER_SRC.contains("2.4"));
        // None of the world program's other terms reach the HUD. Comments are
        // stripped first so the source checks the code, not the prose.
        let code: String = UI_SHADER_SRC
            .lines()
            .map(|line| line.split("//").next().unwrap_or(line))
            .collect::<Vec<_>>()
            .join("\n");
        for forbidden in [
            "fog",
            "lightmap",
            "emission",
            "reflect",
            "discard",
            "normal",
            "u_light_scale",
        ] {
            assert!(!code.contains(forbidden), "{forbidden} leaked into the HUD");
        }
    }

    #[test]
    fn the_camera_uniform_matches_the_ui_shader_layout() {
        assert_eq!(CAMERA_UNIFORM_SIZE, 80);
        assert!(UI_SHADER_SRC.contains("view_projection: mat4x4<f32>"));
        assert!(UI_SHADER_SRC.contains("position: vec3<f32>"));
        assert!(UI_SHADER_SRC.contains("_padding: f32"));
        // `WORLD_VERTEX_STRIDE` is the layout the UI pipeline binds, so the
        // camera and the vertex structs are the shared ones, not local copies.
        assert_eq!(WORLD_VERTEX_STRIDE, 64);
    }
}
