//! Pose-owned scalar accessibility. Direct and residual-direction rays retain
//! fragment precision; front/back samples never share a sharp vertex seam.
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct Vertex {
    pub position_root: [f32; 4],
    pub normal_scale: [f32; 4],
    pub rotation: [f32; 4],
    pub reach: [f32; 4],
}

pub(super) struct Ambient {
    pub output: wgpu::Buffer,
    input: wgpu::Buffer,
    count_buffer: wgpu::Buffer,
    reset: bool,
    count: u32,
    pipeline: Option<wgpu::ComputePipeline>,
    binding: Option<wgpu::BindGroup>,
    dirty: bool,
}

impl Ambient {
    pub fn new(device: &wgpu::Device) -> Self {
        let buffer = |label, size| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage: wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::COPY_DST
                    | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            })
        };
        Self {
            output: buffer("posed-ambient-output", 16),
            input: buffer("posed-ambient-input", 64),
            count_buffer: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("posed-ambient-count"),
                contents: &[0_u8; 16],
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            }),
            reset: true,
            count: 0,
            pipeline: None,
            binding: None,
            dirty: false,
        }
    }

    fn pipeline(device: &wgpu::Device) -> wgpu::ComputePipeline {
        let queries = super::posed_visibility::query_source().replace(
            "const POSED_DEFER_PARALLEL_TEST:bool = false;",
            "const POSED_DEFER_PARALLEL_TEST:bool = true;",
        );
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("places-posed-ambient"),
            source: wgpu::ShaderSource::Wgsl(
                format!("{}{}", queries, include_str!("posed_ambient.wgsl")).into(),
            ),
        });
        device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("places-posed-ambient"),
            layout: None,
            module: &shader,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        })
    }

    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        nodes: &wgpu::Buffer,
        triangles: &wgpu::Buffer,
        vertices: &[Vertex],
        reset: bool,
    ) -> Result<bool, String> {
        self.dirty = false;
        self.reset |= reset || self.count != u32::try_from(vertices.len()).unwrap_or(u32::MAX);
        self.count = u32::try_from(vertices.len()).map_err(|error| error.to_string())?;
        if self.count == 0 {
            self.dirty = false;
            return Ok(false);
        }
        if self.count > device.limits().max_compute_workgroups_per_dimension {
            return Err(
                "Posed ambient dispatch exceeds the current device workgroup budget".into(),
            );
        }
        let input_size = u64::from(self.count).saturating_mul(64);
        let output_size = u64::from(self.count).saturating_mul(16);
        let limit = device.limits().max_storage_buffer_binding_size;
        if input_size > limit || output_size > limit {
            return Err("Posed ambient exceeds the existing storage budget".into());
        }
        let resize = |buffer: &mut wgpu::Buffer, size: u64| {
            let changed = buffer.size() < size;
            if changed {
                *buffer = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("posed-ambient-capacity"),
                    size: size.next_power_of_two().min(limit),
                    usage: wgpu::BufferUsages::STORAGE
                        | wgpu::BufferUsages::COPY_DST
                        | wgpu::BufferUsages::COPY_SRC,
                    mapped_at_creation: false,
                });
            }
            changed
        };
        let _input_changed = resize(&mut self.input, input_size);
        let changed = resize(&mut self.output, output_size);
        queue.write_buffer(&self.input, 0, bytemuck::cast_slice(vertices));
        queue.write_buffer(
            &self.count_buffer,
            0,
            bytemuck::cast_slice(&[self.count, 0_u32, 0, 0]),
        );
        let pipeline = self.pipeline.get_or_insert_with(|| Self::pipeline(device));
        self.binding = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("posed-ambient-binding"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: nodes.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: triangles.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.input.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: self.output.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: self.count_buffer.as_entire_binding(),
                },
            ],
        }));
        self.dirty = self.reset;
        Ok(changed)
    }

    pub const fn clear(&mut self) {
        self.count = 0;
        self.dirty = false;
        self.reset = true;
    }

    /// Explicit BENCH capture readback; normal rendering never synchronizes here.
    pub fn diagnostic_values(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<Vec<[f32; 4]>, String> {
        if self.count == 0 {
            return Ok(Vec::new());
        }
        let size = u64::from(self.count).saturating_mul(16);
        let staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("posed-ambient-readback"),
            size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        encoder.copy_buffer_to_buffer(&self.output, 0, &staging, 0, size);
        let submission = queue.submit([encoder.finish()]);
        let (sender, receiver) = std::sync::mpsc::channel();
        staging
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _sent = sender.send(result);
            });
        let _status = device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: Some(std::time::Duration::from_secs(5)),
            })
            .map_err(|error| error.to_string())?;
        receiver
            .recv_timeout(std::time::Duration::from_secs(1))
            .map_err(|error| error.to_string())?
            .map_err(|error| error.to_string())?;
        let view = staging
            .slice(..)
            .get_mapped_range()
            .map_err(|error| error.to_string())?;
        let values = bytemuck::cast_slice::<u8, [f32; 4]>(&view).to_vec();
        drop(view);
        staging.unmap();
        Ok(values)
    }

    #[cfg(feature = "visual-diagnostics")]
    pub const fn vertex_count(&self) -> u32 {
        self.count
    }

    #[cfg(feature = "visual-diagnostics")]
    pub fn device_bytes(&self) -> u64 {
        self.input
            .size()
            .saturating_add(self.output.size())
            .saturating_add(self.count_buffer.size())
    }

    pub const fn pending(&self) -> bool {
        self.dirty && self.count > 0
    }

    pub fn encode(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        queue: &wgpu::Queue,
        timestamp_writes: Option<wgpu::ComputePassTimestampWrites<'_>>,
    ) {
        if !self.dirty || self.count == 0 {
            return;
        }
        queue.write_buffer(
            &self.count_buffer,
            0,
            bytemuck::cast_slice(&[self.count, u32::from(self.reset), 0_u32, 0]),
        );
        if let (Some(binding), Some(pipeline)) = (&self.binding, &self.pipeline) {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("posed-ambient"),
                timestamp_writes,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, binding, &[]);
            pass.dispatch_workgroups(self.count, 1, 1);
        }
        self.dirty = false;
        self.reset = false;
    }
}
