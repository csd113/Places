//! Bounded water presentation. Opaque snapshots never alias an attachment;
//! local impulses change normals only, leaving collision and mean height intact.
use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt;

use crate::engine::level::{LevelDef, WaterVolumes};
use crate::engine::render::DrawableSize;
use crate::engine::render::common::dynamic::{DynamicId, WaterMotion};
use crate::engine::spatial::Aabb;

pub const MAX_RIPPLES: usize = 8;
const LIFETIME: f32 = 2.8;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct Ripple {
    pub origin_time: [f32; 4],
    pub shape: [f32; 4],
}

/// Shader-ready annulus bounds and decay, prepared once for each event/frame.
/// Logical event time/height remain in `Ripple`; the fragment only needs its
/// authored volume priority, horizontal origin, annulus and normal amplitude.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct PreparedRipple {
    origin_bounds: [f32; 4],
    profile: [f32; 4],
}

impl Ripple {
    fn prepare(self, seconds: f32) -> PreparedRipple {
        let age = seconds - self.origin_time[3];
        if !(0.0..self.shape[2]).contains(&age) {
            return PreparedRipple::zeroed();
        }
        let front = age.mul_add(1.15, self.shape[1]);
        let width = age.mul_add(0.1, 0.14);
        let inner = (width.mul_add(-3.0, front) - 0.000_01).max(0.0);
        let outer = width.mul_add(3.0, front) + 0.000_01;
        let remaining = 1.0 - age / self.shape[2];
        PreparedRipple {
            origin_bounds: [
                self.origin_time[0],
                self.origin_time[2],
                inner * inner,
                outer * outer,
            ],
            profile: [
                self.shape[0] * remaining * remaining,
                front,
                width.recip(),
                self.shape[3],
            ],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Uniform {
    inverse: [[f32; 4]; 4],
    controls: [f32; 4],
    ripples: [PreparedRipple; MAX_RIPPLES],
}

#[derive(Default)]
struct Track {
    next: f32,
}

#[derive(Default)]
pub struct Interactions {
    volumes: WaterVolumes,
    bounds: Option<Aabb>,
    tracks: HashMap<DynamicId, Track>,
    ripples: Vec<Ripple>,
    player: Option<Aabb>,
    player_next: f32,
    player_seconds: f32,
    emitted: u64,
    replaced: u64,
}

impl Interactions {
    pub fn install(&mut self, level: &LevelDef) {
        let volumes = WaterVolumes::from_level(level);
        let bounds = (!volumes.is_empty()).then(|| {
            volumes
                .volumes()
                .iter()
                .fold(Aabb::EMPTY, |bounds, volume| {
                    bounds.union(&Aabb {
                        min: [volume.x0, volume.surface_y - 0.01, volume.z0],
                        max: [volume.x1, volume.surface_y + 0.01, volume.z1],
                    })
                })
        });
        *self = Self {
            volumes,
            bounds,
            ripples: Vec::with_capacity(MAX_RIPPLES),
            tracks: HashMap::with_capacity(super::super::common::dynamic::MAX_DYNAMIC_OBJECTS),
            ..Self::default()
        };
    }

    /// One conservative cached box test, not a per-frame water/scenery scan.
    pub fn visible(&self, frame: &super::world::WorldFrame) -> bool {
        self.bounds.is_some_and(|bounds| frame.visible(&bounds))
    }

    /// Copy only colour texels water can read. Resources remain full-size and
    /// stable, avoiding allocation churn as the camera moves. Five pixels cover
    /// the three-pixel distortion, bilinear footprint and projection rounding.
    pub fn copy_region(&self, matrix: Mat4, size: DrawableSize) -> ([u32; 2], DrawableSize) {
        let full = ([0, 0], size);
        let Some(bounds) = self.bounds else {
            return full;
        };
        let mut minimum = [1.0_f32; 2];
        let mut maximum = [0.0_f32; 2];
        for x in [bounds.min[0], bounds.max[0]] {
            for y in [bounds.min[1], bounds.max[1]] {
                for z in [bounds.min[2], bounds.max[2]] {
                    let clip = matrix.mul_vec4(glam::Vec4::new(x, y, z, 1.0));
                    if !clip.is_finite() || clip.w <= 0.0001 {
                        return full;
                    }
                    let uv = [
                        (clip.x / clip.w).mul_add(0.5, 0.5),
                        (clip.y / clip.w).mul_add(-0.5, 0.5),
                    ];
                    for ((low, high), value) in minimum.iter_mut().zip(&mut maximum).zip(uv) {
                        *low = low.min(value);
                        *high = high.max(value);
                    }
                }
            }
        }
        // GPU texture dimensions fit exact f32 integers. Conversion back is
        // bounded to the image before the external numeric conversion.
        let dimensions = glam::UVec2::new(size.width, size.height).as_vec2();
        let low = glam::Vec2::new(
            minimum[0].mul_add(dimensions.x, -5.0),
            minimum[1].mul_add(dimensions.y, -5.0),
        )
        .floor()
        .clamp(glam::Vec2::ZERO, dimensions)
        .as_uvec2();
        let high = glam::Vec2::new(
            maximum[0].mul_add(dimensions.x, 5.0),
            maximum[1].mul_add(dimensions.y, 5.0),
        )
        .ceil()
        .clamp(glam::Vec2::ZERO, dimensions)
        .as_uvec2();
        let extent = DrawableSize::new(high.x.saturating_sub(low.x), high.y.saturating_sub(low.y));
        if extent.is_empty() {
            full
        } else {
            ([low.x, low.y], extent)
        }
    }

    pub fn advance(
        &mut self,
        seconds: f32,
        delta: f32,
        motions: impl Iterator<Item = WaterMotion>,
    ) {
        self.ripples
            .retain(|ripple| seconds - ripple.origin_time[3] < LIFETIME);
        if self.volumes.is_empty() {
            return;
        }
        for motion in motions {
            let Some(current) = motion.current else {
                let _retired = self.tracks.remove(&motion.id);
                continue;
            };
            let next = self.tracks.get(&motion.id).map_or(0.0, |track| track.next);
            if seconds < next {
                continue;
            }
            if self.disturb(motion.previous, current, seconds, delta) {
                if self.tracks.len() >= super::super::common::dynamic::MAX_DYNAMIC_OBJECTS
                    && !self.tracks.contains_key(&motion.id)
                {
                    let oldest = self
                        .tracks
                        .iter()
                        .min_by(|(_, a), (_, b)| a.next.total_cmp(&b.next))
                        .map(|(id, _)| *id);
                    if let Some(id) = oldest {
                        let _retired = self.tracks.remove(&id);
                    }
                }
                self.tracks.entry(motion.id).or_default().next = seconds + 0.24;
            }
        }
    }

    pub fn player(&mut self, eye: Vec3, feet: f32, seconds: f32) {
        if self.volumes.is_empty() {
            return;
        }
        let current = Aabb {
            min: [eye.x - 0.2, feet, eye.z - 0.2],
            max: [eye.x + 0.2, eye.y + 0.1, eye.z + 0.2],
        };
        if let Some(previous) = self.player
            && seconds >= self.player_next
            && self.disturb(previous, current, seconds, seconds - self.player_seconds)
        {
            self.player_next = seconds + 0.26;
        }
        self.player = Some(current);
        self.player_seconds = seconds;
    }

    fn disturb(&mut self, previous: Aabb, current: Aabb, seconds: f32, delta_seconds: f32) -> bool {
        let centre =
            |bounds: Aabb| Vec3::from_array(bounds.min).midpoint(Vec3::from_array(bounds.max));
        let before = centre(previous);
        let after = centre(current);
        let delta = Vec3::new(after.x - before.x, after.y - before.y, after.z - before.z);
        // Teleports/loads are not impacts. Slow bobbing still emits a weak,
        // sparse wake; a truly settled body emits nothing.
        let travel = delta.length();
        if !(0.000_005..=3.0).contains(&travel) {
            return false;
        }
        let selected = self
            .volumes
            .volumes()
            .iter()
            .enumerate()
            .rev()
            .find(|(_, volume)| {
                volume.enabled
                    && volume.contains(after.x, after.z)
                    && current.min[1] <= volume.surface_y + 0.04
                    && current.max[1].max(previous.max[1]) >= volume.surface_y - 0.04
                    && previous.min[1].min(current.min[1]) <= volume.surface_y
            });
        let Some((volume_index, volume)) = selected else {
            return false;
        };
        let surface = volume.surface_y;
        let entry = previous.min[1] > surface && current.min[1] <= surface;
        let strength = if entry {
            ((delta.y.abs() / delta_seconds.clamp(0.001, 0.1)).mul_add(0.012, 0.025))
                .clamp(0.025, 0.1)
        } else {
            (travel / delta_seconds.clamp(0.001, 0.1) * 0.008).clamp(0.008, 0.035)
        };
        let radius = ((current.max[0] - current.min[0]).max(current.max[2] - current.min[2]) * 0.4)
            .clamp(0.08, 0.65);
        self.push(Ripple {
            origin_time: [after.x, surface, after.z, seconds],
            shape: [
                strength,
                radius,
                LIFETIME,
                f32::from(u16::try_from(volume_index.saturating_add(1)).unwrap_or(u16::MAX)),
            ],
        });
        true
    }

    fn push(&mut self, ripple: Ripple) {
        if self.ripples.len() == MAX_RIPPLES {
            // Preserve the strongest recent splashes while replacing the
            // oldest weak wake. All events have a hard lifetime regardless.
            let slot = self
                .ripples
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| {
                    (a.origin_time[3].mul_add(0.01, a.shape[0]))
                        .total_cmp(&(b.origin_time[3].mul_add(0.01, b.shape[0])))
                })
                .map(|(index, _)| index);
            if let Some(value) = slot.and_then(|index| self.ripples.get_mut(index)) {
                *value = ripple;
            }
            self.replaced = self.replaced.saturating_add(1);
        } else {
            self.ripples.push(ripple);
        }
        self.emitted = self.emitted.saturating_add(1);
    }

    pub fn uniform(&self, matrix: Mat4, seconds: f32, enabled: bool) -> Uniform {
        let mut result = Uniform {
            inverse: matrix.inverse().to_cols_array_2d(),
            controls: [
                seconds,
                f32::from(u8::from(enabled)),
                f32::from(u16::try_from(self.ripples.len()).unwrap_or(0)),
                0.0,
            ],
            ripples: [PreparedRipple::zeroed(); MAX_RIPPLES],
        };
        for (slot, event) in result.ripples.iter_mut().zip(&self.ripples) {
            *slot = event.prepare(seconds);
        }
        result
    }

    #[cfg(feature = "visual-diagnostics")]
    pub fn diagnostics(&self) -> serde_json::Value {
        serde_json::json!({"active":self.ripples.len(),"capacity":MAX_RIPPLES,"tracked":self.tracks.len(),"event_allocation":self.ripples.capacity(),"track_allocation":self.tracks.capacity(),"emitted":self.emitted,"replaced":self.replaced})
    }
}

/// One reusable opaque image/depth pair. Low, dry levels and unload release it.
pub struct Snapshot {
    pub color: wgpu::Texture,
    pub depth: wgpu::Texture,
    pub color_view: wgpu::TextureView,
    pub depth_view: wgpu::TextureView,
    pub size: DrawableSize,
}

impl Snapshot {
    pub fn new(device: &wgpu::Device, size: DrawableSize) -> Self {
        let make = |label, format| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: size.width.max(1),
                    height: size.height.max(1),
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
        };
        let color = make(
            "places-water-opaque-color",
            super::postprocess::SCENE_FORMAT,
        );
        let depth = make("places-water-opaque-depth", super::surface::DEPTH_FORMAT);
        let color_view = color.create_view(&wgpu::TextureViewDescriptor::default());
        let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());
        Self {
            color,
            depth,
            color_view,
            depth_view,
            size,
        }
    }
    #[cfg(feature = "visual-diagnostics")]
    pub fn bytes(&self) -> u64 {
        u64::from(self.size.width)
            .saturating_mul(u64::from(self.size.height))
            .saturating_mul(12)
    }
}

pub fn uniform_buffer(device: &wgpu::Device) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("places-water-controls"),
        contents: bytemuck::bytes_of(&Uniform::zeroed()),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    })
}

#[cfg(test)]
#[path = "../../tests/water_presentation.rs"]
mod tests;
