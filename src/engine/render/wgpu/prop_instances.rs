//! Shared source buffers and independent placement/lighting storage.

use crate::engine::render::common::prop_vertices::SharedVertices;
use crate::engine::spatial::Aabb;
use bytemuck::{Pod, Zeroable};
use std::sync::Arc;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Instance {
    model: [[f32; 4]; 4],
    normal: [[f32; 4]; 4],
    tint: [f32; 4],
    lighting: [u32; 4],
}

pub struct SharedChunk {
    pub binding: wgpu::BindGroup,
    instances: wgpu::Buffer,
    colors: wgpu::Buffer,
    visibility: wgpu::Buffer,
    pub bounds: Vec<Aabb>,
    pub vertices_per_instance: u32,
    pub next_vertex: u32,
    reactive: bool,
}

#[derive(Default)]
pub struct Plan {
    pub vertices: u64,
    pub instances: u64,
    pub reactive: bool,
    pub chunk: Option<usize>,
}

pub fn layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("places-prop-instance-layout"),
        entries: &std::array::from_fn::<_, 3, _>(|index| wgpu::BindGroupLayoutEntry {
            binding: u32::try_from(index).unwrap_or(u32::MAX),
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }),
    })
}

impl Plan {
    pub fn admitted(&self, device: &wgpu::Device) -> bool {
        if device.limits().max_bind_groups < 6 {
            return false;
        }
        let limit = device.limits().max_storage_buffer_binding_size;
        self.vertices
            .checked_mul(16)
            .is_some_and(|bytes| bytes > 0 && bytes <= limit)
            && self
                .instances
                .checked_mul(super::buffer_element_bytes::<Instance>())
                .is_some_and(|bytes| bytes > 0 && bytes <= limit)
    }

    pub fn upload(
        &self,
        device: &wgpu::Device,
        shared: &SharedVertices,
    ) -> super::props::PropChunk {
        let vertex_count = shared.asset.model.vertices.len();
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("places-canonical-prop-vertices"),
            size: u64::try_from(vertex_count)
                .unwrap_or(u64::MAX)
                .saturating_mul(super::world::WORLD_VERTEX_STRIDE)
                .max(4),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let index_buffer = super::world::staged_index_buffer(
            device,
            shared.asset.model.indices.len(),
            "places-canonical-prop-indices",
        );
        let storage = |label, size| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let instances = storage(
            "places-prop-instance-transforms",
            self.instances
                .saturating_mul(super::buffer_element_bytes::<Instance>()),
        );
        let colors = storage(
            "places-prop-independent-lighting",
            self.vertices.saturating_mul(16),
        );
        let visibility = storage(
            "places-prop-instance-uv-visibility",
            if self.reactive {
                self.vertices.saturating_mul(16)
            } else {
                16
            },
        );
        let binding = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("places-prop-instance-binding"),
            layout: &layout(device),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: instances.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: colors.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: visibility.as_entire_binding(),
                },
            ],
        });
        super::props::PropChunk {
            vertex_buffer,
            index_buffer,
            vertex_count: u32::try_from(vertex_count).unwrap_or(u32::MAX),
            shared: Some(SharedChunk {
                binding,
                instances,
                colors,
                visibility,
                bounds: Vec::new(),
                next_vertex: 0,
                vertices_per_instance: u32::try_from(vertex_count).unwrap_or(u32::MAX),
                reactive: self.reactive,
            }),
        }
    }
}

impl SharedChunk {
    pub fn begin_append(
        &mut self,
        queue: &wgpu::Queue,
        shared: &SharedVertices,
        vertices: u32,
    ) -> std::ops::Range<u32> {
        let first = u32::try_from(self.bounds.len()).unwrap_or(u32::MAX);
        let records: Vec<Instance> = shared
            .placements
            .iter()
            .enumerate()
            .map(|(slot, placement)| {
                self.bounds.push(placement.bounds);
                Instance {
                    model: placement.model.to_cols_array_2d(),
                    normal: placement.normal.to_cols_array_2d(),
                    tint: [placement.tint[0], placement.tint[1], placement.tint[2], 1.0],
                    lighting: [
                        self.next_vertex.saturating_add(
                            u32::try_from(slot)
                                .unwrap_or(u32::MAX)
                                .saturating_mul(self.vertices_per_instance),
                        ),
                        self.vertices_per_instance,
                        u32::from(self.reactive),
                        0,
                    ],
                }
            })
            .collect();
        queue.write_buffer(
            &self.instances,
            u64::from(first).saturating_mul(super::buffer_element_bytes::<Instance>()),
            bytemuck::cast_slice(&records),
        );
        self.next_vertex = self.next_vertex.saturating_add(vertices);
        first..u32::try_from(self.bounds.len()).unwrap_or(u32::MAX)
    }

    pub fn write_slice(
        &self,
        queue: &wgpu::Queue,
        offset: u64,
        colors: &[[f32; 4]],
        visibility: Option<&[[f32; 4]]>,
    ) {
        queue.write_buffer(&self.colors, offset, bytemuck::cast_slice(colors));
        if self.reactive
            && let Some(values) = visibility
        {
            queue.write_buffer(&self.visibility, offset, bytemuck::cast_slice(values));
        }
    }

    pub fn bytes(&self) -> u64 {
        self.instances
            .size()
            .saturating_add(self.colors.size())
            .saturating_add(self.visibility.size())
    }
}

pub fn identity(shared: &SharedVertices) -> *const crate::engine::props::LoadedPropAsset {
    Arc::as_ptr(&shared.asset)
}

/// Rigid repeats already share source buffers. Exact lighting/material equality
/// admits fewer submissions, without treating nearby probes as interchangeable.
pub struct DynamicInstances {
    pub binding: wgpu::BindGroup,
    buffer: wgpu::Buffer,
    uploaded: Vec<Instance>,
}

impl DynamicInstances {
    pub fn new(device: &wgpu::Device, count: usize) -> Self {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("places-rigid-instance-transforms"),
            size: u64::try_from(count)
                .unwrap_or(u64::MAX)
                .saturating_mul(super::buffer_element_bytes::<Instance>())
                .max(16),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let dummy = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("places-rigid-instance-unused-lighting"),
            size: 16,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        let binding = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("places-rigid-instance-binding"),
            layout: &layout(device),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: dummy.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: dummy.as_entire_binding(),
                },
            ],
        });
        Self {
            binding,
            buffer,
            uploaded: Vec::new(),
        }
    }

    pub fn update(
        &mut self,
        queue: &wgpu::Queue,
        uniforms: impl Iterator<Item = super::world::EnvironmentUniform>,
    ) {
        let records: Vec<Instance> = uniforms
            .map(|uniform| {
                let [x, y, z] = uniform.normal_model;
                Instance {
                    model: uniform.model,
                    normal: [x, y, z, [0.0, 0.0, 0.0, 1.0]],
                    tint: [1.0; 4],
                    lighting: [0, 0, 0, 1],
                }
            })
            .collect();
        if bytemuck::cast_slice::<Instance, u8>(&records)
            != bytemuck::cast_slice::<Instance, u8>(&self.uploaded)
        {
            queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&records));
            self.uploaded = records;
        }
    }
}
