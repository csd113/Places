//! Source-supported pool caustics. One shared, threaded volume tree keeps
//! lookup camera-independent without a per-fragment scan or a volume cap.
//! The only per-frame upload is its 16-byte clock header. Imagery is a PNG.

use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use super::texture::{GpuTexture, TextureCache, TextureKey, TextureSemantic, TextureWrap};
use crate::engine::level::{LevelDef, WaterVolume, WaterVolumes};
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
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Header {
    seconds: f32,
    count: u32,
    mean: f32,
    padding: u32,
}

fn leaf(volume: &WaterVolume, index: usize) -> Node {
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

/// Fail closed across disconnected rooms/partitions. The existing bake owns
/// receiver shadows; a volume-wide source gate must not borrow another room's
/// light. Interior wall intersections are decisive even between probe points.
fn isolated(level: &LevelDef, lighting: &LevelLighting, volume: &WaterVolume) -> bool {
    let (cx, cz) = volume.center();
    let reference = [cx, volume.surface_y - 0.01, cz];
    let Some(room) = lighting.room_index_at_height(reference[0], reference[1], reference[2]) else {
        return false;
    };
    for room_def in &level.rooms {
        if room_def.floor_y > volume.bottom_y + 0.01
            && room_def.floor_y < volume.surface_y - 0.01
            && room_def.x < volume.x1
            && room_def.x + room_def.width > volume.x0
            && room_def.z < volume.z1
            && room_def.z + room_def.depth > volume.z0
        {
            return false;
        }
    }
    for wall in &level.walls {
        let x = cx.clamp(wall.x, wall.x + wall.width);
        let z = cz.clamp(wall.z, wall.z + wall.depth);
        if volume.contains(x, z)
            && x > volume.x0 + 0.01
            && x < volume.x1 - 0.01
            && z > volume.z0 + 0.01
            && z < volume.z1 - 0.01
            && crate::engine::level::LevelSurfaces::new(level)
                .wall_solid_slices(wall)
                .iter()
                .any(|slice| slice.bottom < volume.surface_y && slice.top > volume.bottom_y + 0.01)
        {
            return false;
        }
    }
    for u in [0.05, 0.5, 0.95] {
        for v in [0.05, 0.5, 0.95] {
            let x = (volume.x1 - volume.x0).mul_add(u, volume.x0);
            let z = (volume.z1 - volume.z0).mul_add(v, volume.z0);
            if !volume.contains(x, z) {
                continue;
            }
            for y in [volume.bottom_y + 0.01, volume.surface_y - 0.01] {
                let point = [x, y, z];
                if lighting.room_index_at_height(x, y, z) != Some(room)
                    || !lighting.same_probe_region(room, reference, point)
                {
                    return false;
                }
            }
        }
    }
    true
}

fn sources(level: &LevelDef, lighting: &LevelLighting, volume: &WaterVolume) -> Vec<usize> {
    if !isolated(level, lighting, volume) {
        return Vec::new();
    }
    let (cx, cz) = volume.center();
    lighting
        .lights()
        .iter()
        .enumerate()
        .filter_map(|(index, light)| {
            if light.y() <= volume.surface_y || light.intensity() <= 0.0 {
                return None;
            }
            let x = light.x().clamp(volume.x0, volume.x1);
            let z = light.z().clamp(volume.z0, volume.z1);
            let point = if volume.contains(x, z) {
                [x, volume.surface_y - 0.01, z]
            } else {
                [cx, volume.surface_y - 0.01, cz]
            };
            let distance = if light.directional {
                let dx = ((light.x() - point[0]).abs() - light.half_w()).max(0.0);
                let dz = ((light.z() - point[2]).abs() - light.half_d()).max(0.0);
                dx.hypot(dz)
            } else {
                glam::Vec3::from_array(light.source.position)
                    .distance(glam::Vec3::from_array(point))
            };
            (distance < light.range()
                && lighting.runtime_light_visible(index, light.source.position, point))
            .then_some(index)
        })
        .collect()
}

/// Shared by static, movable, animated and reflection environment bindings.
pub struct Caustics {
    pub buffer: wgpu::Buffer,
    pub texture: Arc<GpuTexture>,
    nodes: Vec<Node>,
    sources: Vec<Vec<usize>>,
    mean: f32,
}

impl Caustics {
    pub fn empty(device: &wgpu::Device, cache: &TextureCache) -> Self {
        Self {
            buffer: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("places-caustics-empty"),
                contents: &[0; 64],
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            }),
            texture: cache.fallback(),
            nodes: Vec::new(),
            sources: Vec::new(),
            mean: 1.0,
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
        let mut source_lighting = lighting.clone();
        for index in 0..lighting.lights().len() {
            let _changed = source_lighting.set_light_enabled(index, true);
        }
        result.sources = volumes
            .volumes()
            .iter()
            .map(|volume| sources(level, &source_lighting, volume))
            .collect();
        let mut leaves: Vec<_> = volumes
            .volumes()
            .iter()
            .enumerate()
            .map(|(index, volume)| leaf(volume, index))
            .collect();
        tree(&mut leaves, &mut result.nodes);
        let byte_len = 16_u64.saturating_add(
            u64::try_from(result.nodes.len())
                .unwrap_or(0)
                .saturating_mul(48),
        );
        result.buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("places-caustics-volumes"),
            size: byte_len.max(64),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        result.refresh_sources(queue, lighting);
        result.update(queue, 0.0);
        crate::engine::logging::info(format!(
            "[caustics] {} volumes, {} tree nodes, {byte_len} buffer bytes, {} mask bytes, mean {:.6}",
            volumes.volumes().len(),
            result.nodes.len(),
            result.texture.meta().resident_bytes,
            result.mean
        ));
        crate::engine::logging::info(format!(
            "[caustics] {} source-supported water volumes",
            result.nodes.iter().filter(|node| node.meta[3] != 0).count()
        ));
        result
    }

    pub fn refresh_sources(&mut self, queue: &wgpu::Queue, lighting: &LevelLighting) {
        for node in &mut self.nodes {
            let index = usize::try_from(node.meta[2].saturating_sub(1)).unwrap_or(usize::MAX);
            node.meta[3] = u32::from(
                node.meta[2] > 0
                    && self.sources.get(index).is_some_and(|entries| {
                        entries.iter().any(|source_index| {
                            lighting
                                .lights()
                                .get(*source_index)
                                .is_some_and(crate::engine::lighting::BakedLight::is_active)
                        })
                    }),
            );
        }
        if !self.nodes.is_empty() {
            queue.write_buffer(&self.buffer, 16, bytemuck::cast_slice(&self.nodes));
        }
    }

    pub fn update(&self, queue: &wgpu::Queue, seconds: f32) {
        if self.nodes.is_empty() {
            return;
        }
        let header = Header {
            seconds,
            count: u32::try_from(self.nodes.len()).unwrap_or(0),
            mean: self.mean,
            padding: 0,
        };
        queue.write_buffer(&self.buffer, 0, bytemuck::bytes_of(&header));
    }
}
