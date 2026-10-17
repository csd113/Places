//! Source-resolved pool caustics. Compiled visibility prepares a bounded
//! receiver field once; live source changes only recompose its sparse bases.
//! Movable blockers refresh complete cell certificates on scene revisions.
//! One threaded volume tree serves every camera and receiver. Imagery is a PNG.

#[path = "caustics_field.rs"]
mod field;

pub use field::receiver::{CausticsReceiver, CausticsReceiverCache};

use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use super::texture::{GpuTexture, TextureCache, TextureKey, TextureSemantic, TextureWrap};
use crate::engine::level::{CausticsDef, LevelDef, WaterVolume, WaterVolumes};
use crate::engine::lighting::LevelLighting;
use crate::engine::quality::{QualityLevel, TextureClass};
use crate::engine::spatial::Aabb;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct Node {
    pub min: [f32; 4],
    pub max: [f32; 4],
    /// Escape index, greatest authored priority, leaf priority, source support.
    pub meta: [u32; 4],
    /// Receiver lattice dimensions XYZ, followed by its first sample ordinal.
    pub grid: [u32; 4],
    /// Linear water tint RGB and animation speed.
    pub tint_speed: [f32; 4],
    /// Focus strength, animation phase offset, followed by reserved zeros.
    pub params: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Header {
    seconds: f32,
    count: u32,
    mean: f32,
    padding: u32,
}

const _: [(); 96] = [(); std::mem::size_of::<Node>()];
const _: [(); 16] = [(); std::mem::size_of::<Header>()];

fn leaf(volume: &WaterVolume, index: usize, controls: CausticsDef, grid: [u32; 4]) -> Node {
    let priority = u32::try_from(index).unwrap_or(0).saturating_add(1);
    Node {
        min: [
            volume.x0,
            volume.bottom_y - 0.005,
            volume.z0,
            volume.surface_y,
        ],
        max: [
            volume.x1,
            volume.surface_y - 0.005,
            volume.z1,
            volume.radius,
        ],
        meta: [0, priority, priority, 0],
        grid,
        tint_speed: [
            controls.tint[0],
            controls.tint[1],
            controls.tint[2],
            controls.speed,
        ],
        params: [controls.strength, 0.0, 0.0, 0.0],
    }
}

fn tree(leaves: &mut [Node], nodes: &mut Vec<Node>) {
    if leaves.is_empty() {
        return;
    }
    let first = nodes.len();
    let mut bounds = Aabb::EMPTY;
    let mut priority = 0;
    for node in leaves.iter() {
        bounds.expand([node.min[0], node.min[1], node.min[2]]);
        bounds.expand([node.max[0], node.max[1], node.max[2]]);
        priority = priority.max(node.meta[1]);
    }
    nodes.push(Node {
        min: [bounds.min[0], bounds.min[1], bounds.min[2], 0.0],
        max: [bounds.max[0], bounds.max[1], bounds.max[2], 0.0],
        meta: [0, priority, 0, 0],
        grid: [0; 4],
        tint_speed: [0.0; 4],
        params: [0.0; 4],
    });
    if let [only] = leaves {
        if let Some(node) = nodes.get_mut(first) {
            *node = *only;
        }
    } else {
        let axis = if bounds.max[0] - bounds.min[0] >= bounds.max[2] - bounds.min[2] {
            0
        } else {
            2
        };
        leaves.sort_unstable_by(|a, b| {
            let centre = |n: &Node| {
                f32::midpoint(
                    n.min.get(axis).copied().unwrap_or(0.0),
                    n.max.get(axis).copied().unwrap_or(0.0),
                )
            };
            centre(a)
                .total_cmp(&centre(b))
                .then(a.meta[1].cmp(&b.meta[1]))
        });
        let middle = leaves.len() / 2;
        let (left, right) = leaves.split_at_mut(middle);
        let greatest =
            |entries: &[Node]| entries.iter().map(|node| node.meta[1]).max().unwrap_or(0);
        if greatest(right) > greatest(left) {
            tree(right, nodes);
            tree(left, nodes);
        } else {
            tree(left, nodes);
            tree(right, nodes);
        }
    }
    let escape = u32::try_from(nodes.len()).unwrap_or(u32::MAX);
    if let Some(node) = nodes.get_mut(first) {
        node.meta[0] = escape;
    }
}

/// Shared by static, movable, animated and reflection environment bindings.
pub struct Caustics {
    pub buffer: wgpu::Buffer,
    pub texture: Arc<GpuTexture>,
    pub sample_buffer: wgpu::Buffer,
    nodes: Vec<Node>,
    field: field::ReceiverField,
    mean: f32,
    last_count: Option<u32>,
    last_seconds: f32,
    last_flashlight: bool,
    source_uploads: u64,
    receiver_lighting: Option<Arc<LevelLighting>>,
    receiver_volumes: Vec<WaterVolume>,
}

impl Caustics {
    pub fn empty(device: &wgpu::Device, cache: &TextureCache) -> Self {
        Self {
            buffer: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("places-caustics-empty"),
                contents: &[0; 112],
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            }),
            texture: cache.fallback(),
            sample_buffer: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("places-caustics-field-empty"),
                contents: &[0; 16],
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            }),
            nodes: Vec::new(),
            field: field::ReceiverField::default(),
            mean: 1.0,
            last_count: None,
            last_seconds: 0.0,
            last_flashlight: false,
            source_uploads: 0,
            receiver_lighting: None,
            receiver_volumes: Vec::new(),
        }
    }

    pub fn upload(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        cache: &mut TextureCache,
        level: &LevelDef,
        lighting: &LevelLighting,
        quality: QualityLevel,
    ) -> Self {
        let mut result = Self::empty(device, cache);
        let volumes = WaterVolumes::from_level(level);
        let disabled = std::env::var("PLACES_BENCH").is_ok_and(|value| value == "1")
            && std::env::var("PLACES_BENCH_CAUSTICS").is_ok_and(|value| value == "off");
        if volumes.is_empty() || disabled {
            return result;
        }
        let decoded = crate::engine::assets::resolve_asset_root().and_then(|root| {
            crate::engine::materials::load_png_relative(
                &root,
                "shared/textures/effects/caustics_01.png",
            )
            .ok()
        });
        let Some(image) = decoded else {
            crate::engine::logging::warn_once(
                "caustics-mask",
                "[caustics] missing mask PNG; effect disabled",
            );
            return result;
        };
        let sum: f32 = image
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .map(|pixel| f32::from(pixel[0]) / 255.0)
            .sum();
        let pixels = f32::from(u16::try_from(image.width).unwrap_or(1))
            * f32::from(u16::try_from(image.height).unwrap_or(1));
        result.mean = (sum / pixels).max(0.001);
        let key = TextureKey {
            logical: "engine:caustics_01".into(),
            semantic: TextureSemantic::DataLinear,
            class: TextureClass::EmissionMask,
            level: quality,
            wrap: TextureWrap::Repeat,
        };
        let (_, texture) = cache.get_or_upload_with_key(
            device,
            queue,
            key,
            &image,
            crate::engine::materials::TextureOrigin::Catalog,
        );
        result.texture = texture;
        result.prepare_field(device, queue, level, lighting, &volumes);
        result.update(queue, 0.0, false);
        crate::engine::logging::info(format!(
            "[caustics] {} volumes, {} tree nodes, {} receiver samples, {} sparse factors, {} mask bytes, mean {:.6}",
            volumes.volumes().len(),
            result.nodes.len(),
            result.field.samples.len(),
            result.field.factor_count(),
            result.texture.meta().resident_bytes,
            result.mean
        ));
        result
    }

    fn prepare_field(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        level: &LevelDef,
        lighting: &LevelLighting,
        volumes: &WaterVolumes,
    ) {
        // Legacy prepared records predate the flag; explicit packaged authoring
        // still owns source semantics and stable source identities.
        let mut source_lighting = lighting.clone();
        source_lighting.resolve_caustics_eligibility(level);
        self.field = field::ReceiverField::build(level, &source_lighting, volumes.volumes());
        self.receiver_lighting = Some(Arc::new(source_lighting));
        self.receiver_volumes = volumes.volumes().to_vec();
        let mut leaves: Vec<_> = volumes
            .volumes()
            .iter()
            .enumerate()
            .map(|(index, volume)| {
                let controls = level
                    .water
                    .get(index)
                    .map_or_else(CausticsDef::default, |def| def.caustics);
                leaf(volume, index, controls, self.field.grid(index))
            })
            .collect();
        tree(&mut leaves, &mut self.nodes);
        let byte_len = 16_u64.saturating_add(
            u64::try_from(self.nodes.len())
                .unwrap_or(0)
                .saturating_mul(super::buffer_element_bytes::<Node>()),
        );
        self.buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("places-caustics-volumes"),
            size: byte_len.max(112),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.sample_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("places-caustics-receivers"),
            contents: bytemuck::cast_slice(&self.field.samples),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });
        self.upload_sources(queue);
    }

    /// Synchronizes ordinary fixture switches without repeating visibility work.
    pub fn refresh_sources(&mut self, queue: &wgpu::Queue, lighting: &LevelLighting) {
        if self.field.refresh_sources(lighting) {
            self.upload_sources(queue);
        }
    }

    fn upload_sources(&mut self, queue: &wgpu::Queue) {
        self.field.compose();
        for node in &mut self.nodes {
            let index = usize::try_from(node.meta[2].saturating_sub(1)).unwrap_or(usize::MAX);
            node.meta[3] = u32::from(node.meta[2] > 0 && self.field.volume_active(index));
        }
        if !self.nodes.is_empty() {
            queue.write_buffer(&self.buffer, 16, bytemuck::cast_slice(&self.nodes));
            queue.write_buffer(
                &self.sample_buffer,
                0,
                bytemuck::cast_slice(&self.field.samples),
            );
            self.source_uploads = self.source_uploads.saturating_add(1);
        }
    }

    /// Overrides the caustic energy of every resolved emitter with this stable id.
    /// Fixture intensity is its total brightness, shared across its apertures.
    /// Existing baked base lighting remains controlled by its ordinary layers.
    pub fn set_source(
        &mut self,
        id: &str,
        enabled: Option<bool>,
        intensity: Option<f32>,
        color: Option<[f32; 3]>,
        queue: &wgpu::Queue,
    ) -> bool {
        let Some(changed) = self.field.set_source(id, enabled, intensity, color) else {
            return false;
        };
        if changed {
            self.upload_sources(queue);
        }
        true
    }

    /// Updates presentation controls for one authored water without a rebuild.
    pub fn set_water(
        &mut self,
        index: usize,
        tint: Option<[f32; 3]>,
        speed: Option<f32>,
        strength: Option<f32>,
        queue: &wgpu::Queue,
    ) -> bool {
        let priority = u32::try_from(index).unwrap_or(u32::MAX).saturating_add(1);
        let Some(node) = self.nodes.iter_mut().find(|node| node.meta[2] == priority) else {
            return false;
        };
        let controls = CausticsDef {
            tint: tint.unwrap_or([node.tint_speed[0], node.tint_speed[1], node.tint_speed[2]]),
            speed: speed.unwrap_or(node.tint_speed[3]),
            strength: strength.unwrap_or(node.params[0]),
        };
        if !controls.is_valid() {
            return false;
        }
        let tint_speed = [
            controls.tint[0],
            controls.tint[1],
            controls.tint[2],
            controls.speed,
        ];
        if node.tint_speed.map(f32::to_bits) == tint_speed.map(f32::to_bits)
            && node.params[0].to_bits() == controls.strength.to_bits()
        {
            return true;
        }
        node.params[1] = self
            .last_seconds
            .mul_add(node.tint_speed[3] - controls.speed, node.params[1]);
        node.tint_speed = tint_speed;
        node.params[0] = controls.strength;
        queue.write_buffer(&self.buffer, 16, bytemuck::cast_slice(&self.nodes));
        true
    }

    /// Writes just the clock while an eligible source or flashlight is active.
    /// All-off transitions write one zero-count header, then remain upload-free.
    pub fn update(&mut self, queue: &wgpu::Queue, seconds: f32, flashlight_active: bool) {
        self.last_seconds = seconds;
        self.last_flashlight = flashlight_active;
        if self.nodes.is_empty() {
            return;
        }
        let source_active = self.field.has_active_sources();
        let active = self.nodes.iter().any(|node| {
            node.meta[2] > 0
                && node.params[0] > 0.0
                && node.tint_speed[..3].iter().any(|channel| *channel > 0.0)
                && (source_active || flashlight_active)
        });
        let count = if active {
            u32::try_from(self.nodes.len()).unwrap_or(0)
        } else {
            0
        };
        if count == 0 && self.last_count == Some(0) {
            return;
        }
        let header = Header {
            seconds,
            count,
            mean: self.mean,
            padding: 0,
        };
        queue.write_buffer(&self.buffer, 0, bytemuck::bytes_of(&header));
        self.last_count = Some(count);
    }

    /// Keeps live source and water controls across a quality-only reupload.
    pub fn inherit_runtime_state(&mut self, previous: &Self, queue: &wgpu::Queue) {
        self.field.inherit_state(&previous.field);
        for node in &mut self.nodes {
            if node.meta[2] == 0 {
                continue;
            }
            if let Some(old) = previous
                .nodes
                .iter()
                .find(|old| old.meta[2] == node.meta[2])
            {
                node.tint_speed = old.tint_speed;
                node.params = old.params;
            }
        }
        self.upload_sources(queue);
        self.update(queue, previous.last_seconds, previous.last_flashlight);
    }

    /// Refreshes only visibility against moving meshes after their transforms sync.
    pub fn refresh_dynamic_visibility(
        &mut self,
        queue: &wgpu::Queue,
        visibility: &crate::engine::render::common::dynamic_visibility::DynamicVisibility,
    ) {
        if self.has_active_sources() && self.field.refresh_dynamic_visibility(visibility) {
            self.upload_sources(queue);
        }
    }

    /// Active prepared sources still require movable visibility when currently
    /// shadowed, so moving a blocker away can restore their receiver energy.
    #[must_use]
    pub fn has_active_sources(&self) -> bool {
        self.field.has_active_sources()
            && self.nodes.iter().any(|node| {
                node.meta[2] > 0
                    && node.params[0] > 0.0
                    && node.tint_speed.iter().take(3).any(|channel| *channel > 0.0)
            })
    }

    /// Developer evidence for bounded memory, live controls and source support.
    #[must_use]
    pub fn diagnostics(&self) -> serde_json::Value {
        let sample_energy_rgb = self
            .field
            .samples
            .iter()
            .fold([0.0_f64; 3], |mut sum, sample| {
                for (total, channel) in sum.iter_mut().zip(sample.iter().take(3)) {
                    *total += f64::from(*channel);
                }
                sum
            });
        let waters: Vec<_> = self
            .nodes
            .iter()
            .filter(|node| node.meta[2] > 0)
            .map(|node| {
                serde_json::json!({
                    "index": node.meta[2].saturating_sub(1),
                    "active": node.meta[3] > 0,
                    "grid": node.grid,
                    "min": node.min,
                    "max": node.max,
                    "tint": &node.tint_speed[..3],
                    "speed": node.tint_speed[3],
                    "strength": node.params[0],
                    "phase_offset": node.params[1],
                    "phase": self.last_seconds.mul_add(node.tint_speed[3], node.params[1]),
                })
            })
            .collect();
        serde_json::json!({
            "receiver_samples": self.field.samples.len(),
            "receiver_limit": field::MAX_SAMPLES,
            "receiver_bytes": self.field.samples.len().saturating_mul(16),
            "sample_energy_rgb": sample_energy_rgb,
            "factor_entries": self.field.factor_count(),
            "factor_limit": field::MAX_FACTORS,
            "sources_dropped_by_budget": self.field.dropped_sources,
            "source_uploads": self.source_uploads,
            "dynamic_revision": self.field.dynamic_revision,
            "dynamic_visibility_queries": self.field.dynamic_queries,
            "dynamic_region_queries": self.field.dynamic_region_queries,
            "source_revision": self.field.source_revision,
            "tree_nodes": self.nodes.len(),
            "header_count": self.last_count.unwrap_or(0),
            "seconds": self.last_seconds,
            "mask_mean": self.mean,
            "sources": self.field.diagnostics(),
            "waters": waters,
        })
    }
}
