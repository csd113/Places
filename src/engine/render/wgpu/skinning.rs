//! Compute deformation publishes the ordinary world vertex buffer once per pose.
//! All render views reuse it; simulation, sockets and collision retain CPU poses.

use bytemuck::{Pod, Zeroable};

use crate::engine::gltf::PropModel;
use crate::engine::render::common::character::{CharacterAnimator, InfluenceBounds};
use crate::engine::spatial::Aabb;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SkinVertex {
    position: [f32; 4],
    normal: [f32; 4],
    joints: [u32; 4],
    weights: [f32; 4],
}

/// Immutable bind data shared by every instance of a model.
pub(super) struct SkinSource {
    buffer: wgpu::Buffer,
    count: u32,
}

impl SkinSource {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, model: &PropModel) -> Option<Self> {
        // Tiny settled mechanisms keep their cheap CPU path. Explicit benchmark
        // control permits same-build fallback comparisons without a user setting.
        let cpu_control = std::env::var("PLACES_BENCH").as_deref() == Ok("1")
            && std::env::var("PLACES_BENCH_SKINNING").as_deref() == Ok("cpu");
        let count = u32::try_from(model.vertices.len()).ok()?;
        let bytes = u64::from(count).checked_mul(64)?;
        let limits = device.limits();
        if cpu_control
            || count < 1_024
            || !model.morph_targets.is_empty()
            // UV anchors and translucent sorting retain exact CPU bounds.
            || model.submeshes.iter().any(|s| {
                s.response.fluorescence.strength > 0.0
                    || s.alpha.mode == crate::engine::materials::AlphaMode::Blend
            })
            || bytes > limits.max_storage_buffer_binding_size
            || u64::from(count).checked_mul(super::world::WORLD_VERTEX_STRIDE)?
                > limits.max_storage_buffer_binding_size
            || count.div_ceil(64) > limits.max_compute_workgroups_per_dimension
        {
            return None;
        }
        let vertices: Vec<SkinVertex> = model
            .vertices
            .iter()
            .enumerate()
            .map(|(index, vertex)| {
                let [x, y, z] = vertex.pos;
                let [nx, ny, nz] = vertex.normal.unwrap_or([0.0; 3]);
                SkinVertex {
                    position: [x, y, z, 1.0],
                    normal: [nx, ny, nz, 0.0],
                    joints: model
                        .joints
                        .get(index)
                        .copied()
                        .unwrap_or([0; 4])
                        .map(u32::from),
                    weights: model.weights.get(index).copied().unwrap_or([0.0; 4]),
                }
            })
            .collect();
        if vertices.iter().any(|v| {
            if v.weights.iter().any(|w| *w > 0.0 && *w < 1.0e-8) {
                return true;
            }
            v.position
                .iter()
                .chain(&v.normal)
                .chain(&v.weights)
                .any(|x| !x.is_finite() || x.abs() > 1.0e8)
        }) {
            return None;
        }
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("places-skin-bind-vertices"),
            size: bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&buffer, 0, bytemuck::cast_slice(&vertices));
        Some(Self { buffer, count })
    }
}

#[derive(Clone)]
pub(super) struct SkinPipeline {
    pipeline: wgpu::ComputePipeline,
}

impl SkinPipeline {
    pub fn new(device: &wgpu::Device) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("places-skin-deformation"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("skinning.wgsl")
                    .replace(
                        "__WORLD_VERTEX_WORDS__",
                        &super::world::WORLD_VERTEX_STRIDE
                            .saturating_div(4)
                            .to_string(),
                    )
                    .into(),
            ),
        });
        Self {
            pipeline: device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("places-skin-deformation"),
                layout: None,
                module: &module,
                entry_point: Some("deform"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                cache: None,
            }),
        }
    }

    pub fn encode(&self, pass: &mut wgpu::ComputePass<'_>, pose: &GpuPose) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &pose.binding, &[]);
        pass.dispatch_workgroups(pose.count.div_ceil(64), 1, 1);
    }
}

/// Per-instance matrices and conservative influence bounds. Pending survives a
/// skipped presentation, so the first visible frame always consumes the latest pose.
pub(super) struct GpuPose {
    binding: wgpu::BindGroup,
    matrices: wgpu::Buffer,
    values: Vec<[f32; 16]>,
    influences: InfluenceBounds,
    count: u32,
    pub pending: bool,
    pub active: bool,
}

impl GpuPose {
    pub fn new(
        device: &wgpu::Device,
        source: &SkinSource,
        output: &wgpu::Buffer,
        pipeline: &SkinPipeline,
        model: &PropModel,
        animator: &CharacterAnimator,
    ) -> Option<Self> {
        let size = u64::try_from(animator.joint_count())
            .ok()?
            .checked_mul(64)?;
        if size == 0 || size > device.limits().max_storage_buffer_binding_size {
            return None;
        }
        let matrices = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("places-skin-joint-palette"),
            size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let binding = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("places-skin-instance"),
            layout: &pipeline.pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: source.buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: matrices.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: output.as_entire_binding(),
                },
            ],
        });
        Some(Self {
            binding,
            matrices,
            values: vec![[0.0; 16]; animator.joint_count()],
            influences: InfluenceBounds::new(model, animator.joint_count()),
            count: source.count,
            pending: false,
            active: false,
        })
    }

    /// Extreme/non-affine matrices use the established defensive CPU arithmetic.
    pub fn prepare(&mut self, animator: &CharacterAnimator) -> Option<Aabb> {
        self.active = false;
        if self.values.len() != animator.joint_count() {
            return None;
        }
        for (slot, target) in self.values.iter_mut().enumerate() {
            let delta = animator.joint_delta(slot)?;
            let values = delta.to_cols_array();
            if !gpu_palette_matrix(delta) {
                self.pending = false;
                return None;
            }
            *target = values;
        }
        self.active = true;
        self.pending = true;
        Some(self.influences.posed(animator, glam::Mat4::IDENTITY))
    }

    pub fn same_pose(&self, other: &Self) -> bool {
        self.active && other.active && exact_palette(&self.values, &other.values)
    }

    pub fn upload(&self, queue: &wgpu::Queue) {
        if self.active && self.pending {
            queue.write_buffer(&self.matrices, 0, bytemuck::cast_slice(&self.values));
        }
    }
}

/// Revisions name changes within one animator, never equality across actors.
pub(super) fn exact_palette(a: &[[f32; 16]], b: &[[f32; 16]]) -> bool {
    bytemuck::cast_slice::<[f32; 16], u8>(a) == bytemuck::cast_slice::<[f32; 16], u8>(b)
}

/// GPU float implementations may flush subnormal values. Keep tiny units,
/// singular or severely ill-conditioned joint transforms on the CPU path.
/// Reflections and ordinary nonuniform scale remain supported.
pub(super) fn gpu_palette_matrix(delta: glam::Mat4) -> bool {
    if !delta.is_finite()
        || delta.to_cols_array().iter().any(|x| x.abs() > 1.0e8)
        || delta.x_axis.w.abs() > 0.0
        || delta.y_axis.w.abs() > 0.0
        || delta.z_axis.w.abs() > 0.0
        || (delta.w_axis.w - 1.0).abs() > 1.0e-5
    {
        return false;
    }
    let linear = glam::Mat3::from_mat4(delta);
    let magnitude = linear
        .to_cols_array()
        .iter()
        .fold(0.0_f32, |largest, x| largest.max(x.abs()));
    if magnitude < 1.0e-12 {
        return false;
    }
    let normalized = glam::Mat3::from_cols_array(&linear.to_cols_array().map(|x| x / magnitude));
    normalized.determinant().abs() >= 1.0e-8
}
