//! Current-pose practical-light depth layers, fitted per receiver and emitter
//! tap. Unsupported close-source projections retain exact fragment ray queries.
use crate::engine::render::common::light_transport::EntityDirectLight;
use crate::engine::spatial::Aabb;
use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt;

pub(super) struct Receiver {
    pub first: u32,
    pub count: u32,
    pub bounds: Aabb,
    pub sources: [EntityDirectLight; 8],
}
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Sample {
    projection: [f32; 16],
    inverse: [f32; 16],
    parameters: [u32; 4],
}
struct Request {
    projection: Mat4,
    source: [f32; 4],
    first: u32,
    count: u32,
}
struct Pipeline {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
}

pub(super) struct Depth {
    pub view: wgpu::TextureView,
    pub samples: wgpu::Buffer,
    texture: wgpu::Texture,
    layers: Vec<wgpu::TextureView>,
    uniform: wgpu::Buffer,
    binding: Option<wgpu::BindGroup>,
    pipeline: Option<Pipeline>,
    requests: Vec<Request>,
    resolution: u32,
    stride: u32,
    dirty: bool,
}
fn texture(device: &wgpu::Device, resolution: u32, layers: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("posed-practical-depth"),
        size: wgpu::Extent3d {
            width: resolution,
            height: resolution,
            depth_or_array_layers: layers,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}
fn array_view(texture: &wgpu::Texture) -> wgpu::TextureView {
    texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    })
}
impl Depth {
    pub fn new(device: &wgpu::Device) -> Self {
        let image = texture(device, 1, 1);
        let storage = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("posed-depth-samples"),
            contents: bytemuck::bytes_of(&Sample::zero()),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });
        Self {
            view: array_view(&image),
            texture: image,
            samples: storage,
            layers: Vec::new(),
            uniform: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("posed-depth-draw"),
                size: 256,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            binding: None,
            pipeline: None,
            requests: Vec::new(),
            resolution: 1024,
            stride: device.limits().min_uniform_buffer_offset_alignment.max(256),
            dirty: false,
        }
    }
    pub fn clear(&mut self) {
        self.requests.clear();
        self.dirty = false;
    }
    pub const fn pending(&self) -> bool {
        self.dirty
    }
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        triangles: &wgpu::Buffer,
        receivers: &[Receiver],
    ) -> Result<bool, String> {
        self.clear();
        let count = receivers
            .len()
            .checked_mul(32)
            .ok_or("Posed depth source count overflow")?;
        let mut samples = vec![Sample::zero(); count.max(1)];
        for (receiver_index, receiver) in receivers.iter().enumerate() {
            for (source_index, source) in receiver.sources.iter().enumerate() {
                if source.color_strength[3] <= 0.0 || !affects(source, &receiver.bounds) {
                    continue;
                }
                for (tap_index, tap) in source.taps.iter().enumerate() {
                    if tap[3] <= 0.0 {
                        continue;
                    }
                    let Some(projection) =
                        projection(Vec3::from_slice(&tap[..3]), &receiver.bounds)
                    else {
                        continue;
                    };
                    let index = receiver_index
                        .saturating_mul(32)
                        .saturating_add(source_index.saturating_mul(4))
                        .saturating_add(tap_index);
                    let sample = samples
                        .get_mut(index)
                        .ok_or("Posed depth sample is unavailable")?;
                    let layer = u32::try_from(self.requests.len()).map_err(|e| e.to_string())?;
                    if layer >= device.limits().max_texture_array_layers.min(64) {
                        continue;
                    }
                    *sample = Sample {
                        projection: projection.to_cols_array(),
                        inverse: projection.inverse().to_cols_array(),
                        parameters: [layer, 1, self.resolution, 0],
                    };
                    self.requests.push(Request {
                        projection,
                        source: *tap,
                        first: receiver.first,
                        count: receiver.count,
                    });
                }
            }
        }
        let changed = self.upload(device, queue, triangles, &samples)?;
        self.dirty = !self.requests.is_empty();
        if changed && std::env::var("PLACES_BENCH").as_deref() == Ok("1") {
            crate::engine::logging::info(format!(
                "[posed-depth] resolution={} layers={} active={} bytes={}",
                self.resolution,
                self.texture.depth_or_array_layers(),
                self.requests.len(),
                u64::from(self.resolution)
                    .saturating_mul(u64::from(self.resolution))
                    .saturating_mul(u64::from(self.texture.depth_or_array_layers()))
                    .saturating_mul(4)
            ));
        }
        Ok(changed)
    }
    fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        triangles: &wgpu::Buffer,
        samples: &[Sample],
    ) -> Result<bool, String> {
        let data = bytemuck::cast_slice(samples);
        let size = u64::try_from(data.len()).map_err(|e| e.to_string())?;
        if size > device.limits().max_storage_buffer_binding_size {
            return Err("Posed depth metadata exceeds storage binding size".into());
        }
        let mut changed = self.samples.size() < size;
        if changed {
            self.samples = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("posed-depth-samples"),
                size,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        queue.write_buffer(&self.samples, 0, data);
        let count = u32::try_from(self.requests.len())
            .map_err(|e| e.to_string())?
            .max(1);
        if self.texture.depth_or_array_layers() < count || self.texture.width() != self.resolution {
            self.texture = texture(device, self.resolution, count);
            self.view = array_view(&self.texture);
            self.layers = (0..count)
                .map(|layer| {
                    self.texture.create_view(&wgpu::TextureViewDescriptor {
                        dimension: Some(wgpu::TextureViewDimension::D2),
                        base_array_layer: layer,
                        array_layer_count: Some(1),
                        ..Default::default()
                    })
                })
                .collect();
            changed = true;
        }
        let uniform_size = u64::from(count).saturating_mul(u64::from(self.stride));
        if self.uniform.size() < uniform_size {
            self.uniform = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("posed-depth-draws"),
                size: uniform_size,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        let mut bytes = vec![0_u8; usize::try_from(uniform_size).map_err(|e| e.to_string())?];
        for (index, request) in self.requests.iter().enumerate() {
            let start =
                index.saturating_mul(usize::try_from(self.stride).map_err(|e| e.to_string())?);
            let target = bytes
                .get_mut(start..start.saturating_add(96))
                .ok_or("Posed depth draw range unavailable")?;
            let (matrix, remaining) = target.split_at_mut(64);
            let (source, range) = remaining.split_at_mut(16);
            source.copy_from_slice(bytemuck::cast_slice(&request.source));
            matrix.copy_from_slice(bytemuck::cast_slice(&request.projection.to_cols_array()));
            range.copy_from_slice(bytemuck::cast_slice(&[
                request.first,
                request.count,
                0_u32,
                0,
            ]));
        }
        queue.write_buffer(&self.uniform, 0, &bytes);
        let pipeline = self.pipeline.get_or_insert_with(|| Pipeline::new(device));
        self.binding = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("posed-depth-binding"),
            layout: &pipeline.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: triangles.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &self.uniform,
                        offset: 0,
                        size: wgpu::BufferSize::new(96),
                    }),
                },
            ],
        }));
        Ok(changed)
    }
    pub fn encode(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        timestamps: Option<&wgpu::RenderPassTimestampWrites<'_>>,
    ) {
        if !self.dirty {
            return;
        }
        if let (Some(pipeline), Some(binding)) = (&self.pipeline, &self.binding) {
            for (index, request) in self.requests.iter().enumerate() {
                let Some(view) = self.layers.get(index) else {
                    continue;
                };
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("posed-practical-depth"),
                    color_attachments: &[],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: timestamps.as_ref().and_then(|writes| {
                        let first = index == 0;
                        let last = index == self.requests.len().saturating_sub(1);
                        (first || last).then_some(wgpu::RenderPassTimestampWrites {
                            query_set: writes.query_set,
                            beginning_of_pass_write_index: if first {
                                writes.beginning_of_pass_write_index
                            } else {
                                None
                            },
                            end_of_pass_write_index: if last {
                                writes.end_of_pass_write_index
                            } else {
                                None
                            },
                        })
                    }),
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_pipeline(&pipeline.pipeline);
                pass.set_bind_group(
                    0,
                    binding,
                    &[u32::try_from(index)
                        .unwrap_or(u32::MAX)
                        .saturating_mul(self.stride)],
                );
                pass.draw(0..request.count.saturating_mul(3), 0..1);
            }
        }
        self.dirty = false;
    }
}
impl Sample {
    const fn zero() -> Self {
        Self {
            projection: [0.0; 16],
            inverse: [0.0; 16],
            parameters: [0; 4],
        }
    }
}
impl Pipeline {
    fn new(device: &wgpu::Device) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("posed-depth-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(48),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: wgpu::BufferSize::new(96),
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("posed-depth-pipeline-layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("posed-depth-shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("posed_depth.wgsl").into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("posed-depth-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            fragment: None,
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        Self { pipeline, layout }
    }
}
#[expect(
    clippy::arithmetic_side_effects,
    reason = "Finite geometric projection arithmetic; integer ranges are separately checked"
)]
fn projection(source: Vec3, bounds: &Aabb) -> Option<Mat4> {
    let min = Vec3::from_array(bounds.min);
    let max = Vec3::from_array(bounds.max);
    let centre = (min + max) * 0.5;
    let forward = (centre - source).try_normalize()?;
    let up = if forward.y.abs() > 0.98 {
        Vec3::Z
    } else {
        Vec3::Y
    };
    let view = glam::camera::rh::view::look_at_mat4(Vec3::ZERO, forward, up);
    let mut near = f32::INFINITY;
    let mut far = 0.0_f32;
    let mut tangent = 0.0_f32;
    for x in [min.x, max.x] {
        for y in [min.y, max.y] {
            for z in [min.z, max.z] {
                let p = view.transform_point3(Vec3::new(x, y, z) - source);
                let depth = -p.z;
                if depth <= 0.001 {
                    return None;
                }
                near = near.min(depth);
                far = far.max(depth);
                tangent = tangent.max(p.x.abs().max(p.y.abs()) / depth);
            }
        }
    }
    let angle = 2.0 * (tangent * 1.01).atan();
    if !(0.0001..3.10).contains(&angle) {
        return None;
    }
    let result =
        glam::camera::rh::proj::directx::perspective(angle, 1.0, near * 0.98, far * 1.02) * view;
    result.is_finite().then_some(result)
}
#[expect(
    clippy::arithmetic_side_effects,
    reason = "Finite geometric range comparison; no integer arithmetic"
)]
fn affects(source: &EntityDirectLight, bounds: &Aabb) -> bool {
    let point = Vec3::from_slice(&source.position_range[..3]);
    let closest = point.clamp(Vec3::from_array(bounds.min), Vec3::from_array(bounds.max));
    let delta = point - closest;
    let distance = if source.extent_falloff[3] > 0.5 {
        (delta.x.abs() - source.extent_falloff[0])
            .max(0.0)
            .hypot((delta.z.abs() - source.extent_falloff[1]).max(0.0))
    } else {
        delta.length()
    };
    distance < source.position_range[3]
}
