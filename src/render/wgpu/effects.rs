//! Ambient effect billboards on wgpu: the GPU side of
//! [`EffectScene`](crate::render::common::effects::EffectScene).
//!
//! The neutral scene owns the emitters, resolves their materials and evaluates
//! every particle's pose as a pure function of the animation clock. This
//! module owns exactly three GPU things:
//!
//! * **one vertex/index buffer pair, sized once to the level budget** — the
//!   scene's worst case is fixed by the level schema
//!   ([`MAX_EFFECT_VERTICES_PER_LEVEL`] vertices, [`MAX_EFFECT_INDICES_PER_LEVEL`]
//!   indices), so a level load allocates once and a frame only writes the
//!   *used* vertex range with `Queue::write_buffer`;
//! * **one uploaded sheet per distinct effect material**, resolved through the
//!   shared [`TextureCache`] exactly like a prop or fixture sheet (the cache's
//!   layout is the pipeline's group 1, and the sheet's own clamp/repeat and
//!   filtering policy travels with it);
//! * **one pipeline per colour target format**, built from [`EffectsPipeline`]:
//!   straight-alpha blending (`SrcAlpha`/`OneMinusSrcAlpha`), `LessEqual` depth
//!   with **writes off**, and no culling (a billboard is visible from both
//!   sides).
//!
//! Nothing here is created per frame: [`WgpuEffects::sync`] rewrites the
//! vertex buffer's used range and the encode draws the scene's fixed,
//! material-contiguous index ranges.

use std::sync::Arc;

use super::surface::DEPTH_FORMAT;
use super::texture::{CacheOutcome, GpuTexture, TextureCache, TextureFiltering, TextureSemantic};
use super::world::{CAMERA_UNIFORM_SIZE, CameraUniform, camera_uniform_changed};
use crate::quality::QualityLevel;
use crate::render::common::effects::{
    EffectScene, EffectVertex, INDICES_PER_PARTICLE, MAX_EFFECT_INDICES_PER_LEVEL,
    MAX_EFFECT_PARTICLES_PER_LEVEL, MAX_EFFECT_VERTICES_PER_LEVEL, VERTS_PER_PARTICLE,
};

/// The effect shader source, committed beside this module.
pub const EFFECTS_SHADER_SRC: &str = include_str!("effects.wgsl");

/// The shader's vertex entry point.
pub const EFFECTS_VERTEX_ENTRY: &str = "vs_main";

/// The shader's fragment entry point.
pub const EFFECTS_FRAGMENT_ENTRY: &str = "fs_main";

/// Bytes between consecutive effect vertices.
pub const EFFECT_VERTEX_STRIDE: u64 = std::mem::size_of::<EffectVertex>() as u64;

/// Vertex attribute locations, matching `effects.wgsl`'s `VertexIn`.
const EFFECT_ATTRIB_POSITION: u32 = 0;
const EFFECT_ATTRIB_COLOR: u32 = 1;
const EFFECT_ATTRIB_UV: u32 = 2;

const EFFECT_VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 3] = wgpu::vertex_attr_array![
    EFFECT_ATTRIB_POSITION => Float32x3,
    EFFECT_ATTRIB_COLOR => Float32x4,
    EFFECT_ATTRIB_UV => Float32x2,
];

/// The explicit effect vertex buffer layout: position, colour/alpha, UV, in
/// that byte order and 4-byte-granular offsets (0, 12, 28), stride 36.
#[must_use]
pub const fn effect_vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout {
        array_stride: EFFECT_VERTEX_STRIDE,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &EFFECT_VERTEX_ATTRIBUTES,
    }
}

/// The reference's translucent blend state, written out explicitly.
///
/// Exactly the world translucent variant's state: `SrcAlpha`/`OneMinusSrcAlpha`
/// with `FUNC_ADD` for both the colour and the alpha channel. WebGPU's
/// convenience `ALPHA_BLENDING` uses `ONE`/`ONE_MINUS_SRC_ALPHA` for the alpha
/// channel, which is a different (if rarely visible) contract, so it is not
/// used.
const EFFECT_BLEND: wgpu::BlendState = wgpu::BlendState {
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

/// The camera bind group layout: one frame uniform at binding 0, visible to
/// the vertex stage (the fragment stage has no view-dependent term).
fn camera_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("places-wgpu-effects-camera-layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
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

/// Builds one effect pipeline for one colour target format.
fn build_effect_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    pipeline_layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("places-wgpu-effects"),
        layout: Some(pipeline_layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some(EFFECTS_VERTEX_ENTRY),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[Some(effect_vertex_layout())],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            // A billboard is a two-sided quad: the CPU orients it towards the
            // camera, but a cull would make a puff pop when the camera crosses
            // its plane.
            cull_mode: None,
            unclipped_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            // Steam is translucent: it tests against the opaque world's depth
            // but never writes depth, exactly like the world translucent pass.
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(EFFECTS_FRAGMENT_ENTRY),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(EFFECT_BLEND),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

/// The effect pass pipeline, its camera uniform and the group-1 texture layout
/// it expects.
///
/// One pipeline is built per colour target format (the main surface and the
/// offscreen scene target); the camera buffer and bind group are
/// format-independent and persist for the pipeline's lifetime, exactly like
/// [`super::world::WorldPipeline`] and [`super::decals::DecalPipeline`].
pub struct EffectsPipeline {
    pipeline: wgpu::RenderPipeline,
    camera_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    /// The last uploaded camera state, so a still camera writes nothing.
    uploaded: Option<CameraUniform>,
}

impl EffectsPipeline {
    /// Builds the effect pipeline for one colour target format.
    ///
    /// `texture_layout` is the shared [`TextureCache`] layout (group 1): the
    /// sheet bind groups the cache already built are bound unchanged, exactly
    /// like the world and decal passes.
    #[must_use]
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        texture_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("places-wgpu-effects-shader"),
            source: wgpu::ShaderSource::Wgsl(EFFECTS_SHADER_SRC.into()),
        });
        let camera_layout = camera_bind_group_layout(device);
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("places-wgpu-effects-pipeline-layout"),
            bind_group_layouts: &[Some(&camera_layout), Some(texture_layout)],
            immediate_size: 0,
        });
        let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("places-wgpu-effects-camera"),
            size: CAMERA_UNIFORM_SIZE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("places-wgpu-effects-camera"),
            layout: &camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });
        let pipeline = build_effect_pipeline(device, format, &pipeline_layout, &shader);
        Self {
            pipeline,
            camera_buffer,
            bind_group,
            uploaded: None,
        }
    }

    /// Uploads the frame's corrected view-projection and eye position when they
    /// changed.
    ///
    /// The same one-uniform, compare-before-write contract as
    /// [`super::world::WorldPipeline::upload_camera`]: a still camera never
    /// touches the buffer, and any change to either field reaches the shader.
    pub fn upload_camera(
        &mut self,
        queue: &wgpu::Queue,
        view_projection: glam::Mat4,
        eye: glam::Vec3,
    ) {
        let uniform = CameraUniform::new(view_projection, eye);
        if !camera_uniform_changed(self.uploaded, uniform) {
            return;
        }
        queue.write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&uniform));
        self.uploaded = Some(uniform);
    }
}

/// One material-contiguous draw range of the uploaded vertex/index buffers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EffectsGroupGpu {
    /// Index into [`WgpuEffects::textures`].
    texture: usize,
    /// First index into the shared index buffer.
    first_index: u32,
    /// Indices this group draws (a multiple of six).
    index_count: u32,
    /// Vertices this group draws.
    vertex_count: u32,
}

/// What one upload or frame submitted, as plain counters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EffectGpuStats {
    /// Live emitters in the uploaded scene.
    pub emitters: usize,
    /// Particles the scene can draw.
    pub particles: usize,
    /// Distinct effect sheets uploaded this level load.
    pub texture_uploads: usize,
    /// Effect sheets the shared cache already held.
    pub texture_cache_hits: usize,
    /// Draw calls one frame submits (one per distinct material).
    pub draws: usize,
    /// Vertices one frame writes.
    pub vertices: usize,
}

/// The counters one encode contributes to the frame's totals.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EffectDrawTotals {
    /// Draw calls submitted.
    pub draws: usize,
    /// Vertices drawn.
    pub vertices: usize,
    /// Texture bindings issued.
    pub texture_binds: usize,
}

/// The level's effect billboards on the GPU.
///
/// Buffers are sized to the level schema's worst case once, at upload; the
/// scene itself is immutable, so the material groups and the index buffer stay
/// valid for the level's whole lifetime and every frame is one vertex-buffer
/// write plus the draws.
pub struct WgpuEffects {
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    /// CPU scratch, reserved to the level budget once. Never grows per frame.
    vertices: Vec<EffectVertex>,
    /// Material-contiguous ranges, aligned with the scene's draw groups.
    groups: Vec<EffectsGroupGpu>,
    /// One uploaded sheet per distinct effect material.
    textures: Vec<Arc<GpuTexture>>,
    /// Indices the last sync made drawable.
    index_count: u32,
    /// Vertices the last sync wrote.
    vertex_count: u32,
    stats: EffectGpuStats,
}

impl WgpuEffects {
    /// Uploads the scene's sheets and sizes its buffers to the level budget.
    ///
    /// The index pattern is the fixed per-particle quad pattern
    /// (`0 1 2 0 2 3` offset by four per particle); the vertex buffer is filled
    /// per frame by [`Self::sync`]. Both buffers are sized once to the schema's
    /// maximum, so no level can overrun them.
    #[must_use]
    pub fn upload(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        cache: &mut TextureCache,
        scene: &EffectScene,
        level: QualityLevel,
    ) -> Self {
        let mut stats = EffectGpuStats {
            emitters: scene.emitters().len(),
            particles: scene.particle_count(),
            ..EffectGpuStats::default()
        };
        let mut textures: Vec<Arc<GpuTexture>> = Vec::with_capacity(scene.textures().len());
        for material in scene.textures() {
            let (outcome, texture) = cache.get_or_upload(
                device,
                queue,
                &material.texture,
                TextureSemantic::BaseColorDisplay,
                level,
            );
            match outcome {
                CacheOutcome::Uploaded => {
                    stats.texture_uploads = stats.texture_uploads.saturating_add(1);
                }
                CacheOutcome::Reused => {
                    stats.texture_cache_hits = stats.texture_cache_hits.saturating_add(1);
                }
            }
            textures.push(texture);
        }

        let vertex_bytes = u64::try_from(MAX_EFFECT_VERTICES_PER_LEVEL)
            .unwrap_or(u64::MAX)
            .saturating_mul(EFFECT_VERTEX_STRIDE)
            .max(4);
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("places-wgpu-effects-vertices"),
            size: vertex_bytes,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let indices = effect_quad_indices(MAX_EFFECT_PARTICLES_PER_LEVEL);
        let index_bytes = u64::try_from(MAX_EFFECT_INDICES_PER_LEVEL)
            .unwrap_or(u64::MAX)
            .saturating_mul(std::mem::size_of::<u16>() as u64)
            .max(4);
        let index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("places-wgpu-effects-indices"),
            size: index_bytes,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        if !indices.is_empty() {
            queue.write_buffer(&index_buffer, 0, bytemuck::cast_slice(&indices));
        }

        let mut groups: Vec<EffectsGroupGpu> = Vec::with_capacity(scene.draw_groups().len());
        let mut first_index = 0_u32;
        for group in scene.draw_groups() {
            let index_count = u32::try_from(group.index_count()).unwrap_or(u32::MAX);
            let vertex_count = u32::try_from(group.vertex_count()).unwrap_or(u32::MAX);
            groups.push(EffectsGroupGpu {
                texture: group.texture,
                first_index,
                index_count,
                vertex_count,
            });
            first_index = first_index.saturating_add(index_count);
        }

        Self {
            vertex_buffer,
            index_buffer,
            vertices: Vec::with_capacity(MAX_EFFECT_VERTICES_PER_LEVEL),
            groups,
            textures,
            index_count: first_index,
            vertex_count: 0,
            stats,
        }
    }

    /// Rewrites every billboard vertex for the scene's current clock and the
    /// camera, writing only the used range of the pre-sized vertex buffer.
    ///
    /// The caller runs this before encoding the frame; a still clock and a
    /// still camera still rewrite identical bytes, which is cheaper than the
    /// bookkeeping to prove otherwise for a bounded particle set.
    pub fn sync(&mut self, queue: &wgpu::Queue, scene: &EffectScene, camera_position: [f32; 3]) {
        let particles = scene.build_billboards(camera_position, &mut self.vertices);
        self.vertex_count = u32::try_from(self.vertices.len()).unwrap_or(u32::MAX);
        self.index_count = u32::try_from(particles)
            .unwrap_or(u32::MAX)
            .saturating_mul(u32::try_from(INDICES_PER_PARTICLE).unwrap_or(u32::MAX));
        if !self.vertices.is_empty() {
            queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&self.vertices));
        }
    }

    /// Encodes the effect pass into an open colour+depth pass.
    ///
    /// One draw per distinct material; the pass decides ordering (this appends
    /// after the world and its decals), and the caller has already synced the
    /// vertices for this frame's camera.
    pub fn encode<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        pipeline: &'a EffectsPipeline,
        filtering: TextureFiltering,
    ) -> EffectDrawTotals {
        let mut totals = EffectDrawTotals::default();
        if self.index_count() == 0 || self.vertex_count() == 0 || self.textures.is_empty() {
            return totals;
        }
        pass.set_pipeline(&pipeline.pipeline);
        pass.set_bind_group(0, &pipeline.bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        for group in &self.groups {
            let Some(texture) = self.textures.get(group.texture) else {
                continue;
            };
            pass.set_bind_group(1, texture.bind_group(filtering), &[]);
            pass.draw_indexed(
                group.first_index..group.first_index.saturating_add(group.index_count),
                0,
                0..1,
            );
            totals.draws = totals.draws.saturating_add(1);
            totals.texture_binds = totals.texture_binds.saturating_add(1);
            totals.vertices = totals
                .vertices
                .saturating_add(usize::try_from(group.vertex_count).unwrap_or(usize::MAX));
        }
        totals
    }

    /// The upload and frame counters.
    #[must_use]
    pub const fn stats(&self) -> EffectGpuStats {
        self.stats
    }

    /// The vertices the last [`Self::sync`] wrote.
    #[must_use]
    pub const fn vertex_count(&self) -> u32 {
        self.vertex_count
    }

    /// The indices the last [`Self::sync`] made drawable.
    #[must_use]
    pub const fn index_count(&self) -> u32 {
        self.index_count
    }
}

/// The fixed quad index pattern for `particles` billboards.
///
/// Particle `n` owns vertices `4n .. 4n + 3` and indices `0 1 2 0 2 3` offset
/// by `4n`, so one shared index buffer addresses every vertex the scene can
/// ever write. The caller draws only the scene's used prefix.
#[must_use]
fn effect_quad_indices(particles: usize) -> Vec<u16> {
    let mut indices: Vec<u16> = Vec::with_capacity(particles.saturating_mul(INDICES_PER_PARTICLE));
    for particle in 0..particles {
        let base = particle.saturating_mul(VERTS_PER_PARTICLE);
        let Some(base) = u16::try_from(base).ok() else {
            break;
        };
        for offset in [0_u16, 1, 2, 0, 2, 3] {
            let Some(index) = base.checked_add(offset) else {
                return indices;
            };
            indices.push(index);
        }
    }
    indices
}

#[cfg(test)]
mod tests {
    // Test code: unwrap/expect, indexing and exact float comparison are
    // idiomatic in tests; the production lints stay enforced everywhere else.
    #![allow(
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::panic,
        clippy::unwrap_used
    )]

    use super::*;
    use crate::render::common::effects::MAX_EFFECT_INDICES_PER_LEVEL;

    #[test]
    fn the_vertex_layout_matches_the_shader_locations() {
        assert_eq!(std::mem::size_of::<EffectVertex>(), 36);
        assert_eq!(EFFECT_VERTEX_STRIDE, 36);
        assert_eq!(std::mem::offset_of!(EffectVertex, position), 0);
        assert_eq!(std::mem::offset_of!(EffectVertex, color), 12);
        assert_eq!(std::mem::offset_of!(EffectVertex, uv), 28);
        let layout = effect_vertex_layout();
        assert_eq!(layout.array_stride, 36);
        assert_eq!(layout.attributes.len(), 3);
        assert_eq!(layout.attributes[0].shader_location, 0);
        assert_eq!(layout.attributes[0].offset, 0);
        assert_eq!(layout.attributes[1].shader_location, 1);
        assert_eq!(layout.attributes[1].offset, 12);
        assert_eq!(layout.attributes[2].shader_location, 2);
        assert_eq!(layout.attributes[2].offset, 28);
    }

    #[test]
    fn the_shader_declares_the_camera_and_the_sheet_bindings() {
        assert!(EFFECTS_SHADER_SRC.contains("@group(0) @binding(0)"));
        assert!(EFFECTS_SHADER_SRC.contains("var<uniform> camera: Camera"));
        assert!(EFFECTS_SHADER_SRC.contains("@group(1) @binding(0)"));
        assert!(EFFECTS_SHADER_SRC.contains("var effect_texture: texture_2d<f32>"));
        assert!(EFFECTS_SHADER_SRC.contains("@group(1) @binding(1)"));
        assert!(EFFECTS_SHADER_SRC.contains("var effect_sampler: sampler"));
        assert!(
            EFFECTS_SHADER_SRC.contains("textureSample(effect_texture, effect_sampler, in.uv)")
        );
    }

    #[test]
    fn the_blend_state_is_the_world_translucent_state() {
        assert_eq!(EFFECT_BLEND.color.src_factor, wgpu::BlendFactor::SrcAlpha);
        assert_eq!(
            EFFECT_BLEND.color.dst_factor,
            wgpu::BlendFactor::OneMinusSrcAlpha
        );
        assert_eq!(EFFECT_BLEND.alpha.src_factor, wgpu::BlendFactor::SrcAlpha);
        assert_eq!(
            EFFECT_BLEND.alpha.dst_factor,
            wgpu::BlendFactor::OneMinusSrcAlpha
        );
    }

    #[test]
    fn the_index_pattern_addresses_every_quad_vertex() {
        let indices = effect_quad_indices(2);
        assert_eq!(indices, vec![0, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7]);
        let all = effect_quad_indices(MAX_EFFECT_PARTICLES_PER_LEVEL);
        assert_eq!(all.len(), MAX_EFFECT_INDICES_PER_LEVEL);
        assert_eq!(
            *all.last().expect("a non-empty pattern"),
            u16::try_from(MAX_EFFECT_VERTICES_PER_LEVEL - 1).expect("the budget fits u16")
        );
    }

    #[test]
    fn no_pattern_exceeds_the_u16_index_limit() {
        assert!(MAX_EFFECT_VERTICES_PER_LEVEL <= usize::from(u16::MAX) + 1);
    }
}
