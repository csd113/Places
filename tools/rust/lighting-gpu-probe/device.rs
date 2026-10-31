//! Resident geometry/pipeline and bounded ray/result/readback tiles.
use places::engine::lighting::transport::diagnostics::gpu_oracle::Snapshot;
use serde_json::json;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use wgpu::util::DeviceExt as _;

type Candidates = Vec<(Option<u32>, bool)>;

pub struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    binding: wgpu::BindGroup,
    rays: wgpu::Buffer,
    results: wgpu::Buffer,
    staging: wgpu::Buffer,
    tile: usize,
    failed: Arc<AtomicBool>,
    metadata: serde_json::Value,
}

fn bytes(count: usize, stride: u64) -> Result<u64, String> {
    u64::try_from(count)
        .map_err(|e| e.to_string())?
        .checked_mul(stride)
        .ok_or_else(|| "buffer size overflow".into())
}
const fn words(v: [f32; 3], fourth: u32) -> [u32; 4] {
    let [x, y, z] = v;
    [x.to_bits(), y.to_bits(), z.to_bits(), fourth]
}
fn ms(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1_000.0
}

fn open_device() -> Result<(wgpu::Device, wgpu::Queue, wgpu::Adapter), String> {
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
    #[cfg(target_os = "windows")]
    {
        descriptor.backends = wgpu::Backends::DX12;
        descriptor.backend_options.dx12.shader_compiler = wgpu::Dx12Compiler::DynamicDxc {
            dxc_path: std::env::current_exe()
                .map_err(|e| e.to_string())?
                .with_file_name("dxcompiler.dll")
                .to_string_lossy()
                .into_owned(),
        };
    }
    #[cfg(target_os = "macos")]
    {
        descriptor.backends = wgpu::Backends::METAL;
    }
    #[cfg(target_os = "linux")]
    {
        descriptor.backends = wgpu::Backends::VULKAN;
    }
    let instance = wgpu::Instance::new(descriptor);
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: None,
        apply_limit_buckets: false,
    }))
    .map_err(|e| e.to_string())?;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("isolated correctness-gated ray experiment"),
        ..Default::default()
    }))
    .map_err(|e| e.to_string())?;

    Ok((device, queue, adapter))
}

fn pipeline_bindings(
    device: &wgpu::Device,
    buffers: [&wgpu::Buffer; 4],
) -> (wgpu::ComputePipeline, wgpu::BindGroup) {
    let [node, triangle, ray, result] = buffers;
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("reused tiled opaque prototype"),
        source: wgpu::ShaderSource::Wgsl(include_str!("rays.wgsl").into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("candidate identities only"),
        layout: None,
        module: &shader,
        entry_point: Some("main"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    });
    let binding = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("resident query bindings"),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: node.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: triangle.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: ray.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: result.as_entire_binding(),
            },
        ],
    });

    (pipeline, binding)
}

fn failure_state(device: &wgpu::Device) -> Arc<AtomicBool> {
    let failed = Arc::new(AtomicBool::new(false));
    let errors = Arc::clone(&failed);
    device.on_uncaptured_error(Arc::new(move |_error| {
        errors.store(true, Ordering::Release);
    }));
    let lost = Arc::clone(&failed);
    device.set_device_lost_callback(move |_reason, _message| {
        lost.store(true, Ordering::Release);
    });

    failed
}

impl Gpu {
    pub fn new(snapshot: &Snapshot, tile: usize) -> Result<Self, String> {
        let device_started = Instant::now();
        let (device, queue, adapter) = open_device()?;
        let info = adapter.get_info();
        let failed = failure_state(&device);
        let device_ms = ms(device_started);
        let upload_started = Instant::now();
        let nodes: Vec<[u32; 4]> = snapshot
            .nodes
            .iter()
            .flat_map(|n| {
                [
                    words(n.min, 0),
                    words(n.max, 0),
                    [n.first, n.count, n.right, 0],
                ]
            })
            .collect();
        let triangles: Vec<[u32; 4]> = snapshot
            .triangles
            .iter()
            .zip(&snapshot.canonical_ids)
            .flat_map(|([a, b, c], id)| {
                [words(*a, id.saturating_add(1)), words(*b, 0), words(*c, 0)]
            })
            .collect();
        let node_bytes = bytes(nodes.len(), 16)?;
        let triangle_bytes = bytes(triangles.len(), 16)?;
        let ray_bytes = bytes(tile, 32)?;
        let result_bytes = bytes(tile, 8)?;
        let limits = device.limits();
        if [node_bytes, triangle_bytes, ray_bytes, result_bytes]
            .iter()
            .any(|size| {
                *size > limits.max_buffer_size || *size > limits.max_storage_buffer_binding_size
            })
        {
            return Err("bounded geometry exceeds device limits".into());
        }
        let validation = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let storage = |label, data: &[[u32; 4]]| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents: bytemuck::cast_slice(data),
                usage: wgpu::BufferUsages::STORAGE,
            })
        };
        let node_buffer = storage("resident nodes", &nodes);
        let triangle_buffer = storage("resident canonical primitives", &triangles);
        let buffer = |label, size, usage| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage,
                mapped_at_creation: false,
            })
        };
        let rays = buffer(
            "bounded ray tile",
            ray_bytes,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        );
        let results = buffer(
            "bounded candidates",
            result_bytes,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        );
        let staging = buffer(
            "bounded readback",
            result_bytes,
            wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        );
        let (pipeline, binding) =
            pipeline_bindings(&device, [&node_buffer, &triangle_buffer, &rays, &results]);
        let _status = device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(Duration::from_secs(10)),
            })
            .map_err(|e| e.to_string())?;
        if let Some(error) = pollster::block_on(validation.pop()) {
            return Err(error.to_string());
        }
        if failed.load(Ordering::Acquire) {
            return Err("device failed during setup".into());
        }
        let metadata = json!({"adapter":format!("{info:?}"),"device_setup_ms":device_ms,"packing_upload_pipeline_ms":ms(upload_started),"resident_geometry_device_bytes":node_bytes.saturating_add(triangle_bytes),"tile_device_bytes":ray_bytes.saturating_add(result_bytes.saturating_mul(2)),"peak_packing_host_bytes":node_bytes.saturating_add(triangle_bytes),"shader_f64":adapter.features().contains(wgpu::Features::SHADER_F64)});
        Ok(Self {
            device,
            queue,
            pipeline,
            binding,
            rays,
            results,
            staging,
            tile,
            failed,
            metadata,
        })
    }
    pub fn metadata(&self) -> serde_json::Value {
        self.metadata.clone()
    }

    pub fn query(
        &self,
        snapshot: &Snapshot,
        fault: &str,
        cancel_after: Option<usize>,
    ) -> Result<(Candidates, serde_json::Value), String> {
        if fault == "destroy" {
            self.device.destroy();
            self.failed.store(true, Ordering::Release);
        }
        let mut values = Vec::with_capacity(snapshot.rays.len());
        let mut packing_upload_ms = 0.0_f64;
        let mut dispatch_submit_ms = 0.0_f64;
        let mut wait_ms = 0.0_f64;
        let mut readback_ms = 0.0_f64;
        for (index, tile) in snapshot.rays.chunks(self.tile).enumerate() {
            if cancel_after.is_some_and(|limit| index >= limit) {
                return Err("cancelled".into());
            }
            if self.failed.load(Ordering::Acquire) {
                return Err("device unavailable/lost".into());
            }
            let started = Instant::now();
            // Fill only a reusable bounded tile. The ray count is in the final word;
            // stale lanes of the final partial tile never execute.
            let count = u32::try_from(tile.len()).map_err(|e| e.to_string())?;
            let rays: Vec<[u32; 4]> = tile
                .iter()
                .flat_map(|(o, d, l)| [words(*o, l.to_bits()), words(*d, count)])
                .collect();
            self.queue
                .write_buffer(&self.rays, 0, bytemuck::cast_slice(&rays));
            packing_upload_ms += ms(started);
            if fault == "dispatch" {
                return Err("injected dispatch failure before submit".into());
            }
            let submit_started = Instant::now();
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
            {
                let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &self.binding, &[]);
                pass.dispatch_workgroups(count.div_ceil(64), 1, 1);
            }
            let size = bytes(tile.len(), 8)?;
            encoder.copy_buffer_to_buffer(&self.results, 0, &self.staging, 0, size);
            let submission = self.queue.submit([encoder.finish()]);
            dispatch_submit_ms += ms(submit_started);
            let wait_started = Instant::now();
            let (sender, receiver) = std::sync::mpsc::channel();
            self.staging
                .slice(..size)
                .map_async(wgpu::MapMode::Read, move |result| {
                    let _sent = sender.send(result);
                });
            let poll = self.device.poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: Some(Duration::from_secs(10)),
            });
            if let Err(error) = poll {
                self.staging.unmap();
                self.failed.store(true, Ordering::Release);
                return Err(error.to_string());
            }
            let mapped = receiver
                .recv_timeout(Duration::from_secs(1))
                .map_err(|e| e.to_string())?;
            mapped.map_err(|e| e.to_string())?;
            wait_ms += ms(wait_started);
            let read_started = Instant::now();
            let view = self
                .staging
                .slice(..size)
                .get_mapped_range()
                .map_err(|e| e.to_string())?;
            let raw: &[[u32; 2]] = bytemuck::cast_slice(&view);
            values.extend(
                raw.iter()
                    .map(|[id, flags]| (id.checked_sub(1), *flags != 0)),
            );
            drop(view);
            self.staging.unmap();
            readback_ms += ms(read_started);
            if fault == "readback" {
                return Err("injected readback rejection; discard entire partial batch".into());
            }
        }
        if self.failed.load(Ordering::Acquire) {
            return Err("device failed before certification".into());
        }
        if fault == "identity" {
            values.fill((Some(u32::MAX), false));
        }
        Ok((
            values,
            json!({"packing_upload_cpu_ms":packing_upload_ms,"dispatch_copy_submit_cpu_ms":dispatch_submit_ms,"synchronize_ms":wait_ms,"readback_collect_cpu_ms":readback_ms}),
        ))
    }
}
