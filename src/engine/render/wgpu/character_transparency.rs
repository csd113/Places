//! View-dependent triangle order for animated BLEND primitives only.
//!
//! Storage is sized once from admitted model topology (at most 16,240 triangles
//! per reviewed model, 128 characters). Invisible/offscreen instances do no work.
//! A settled pose retains its CPU positions; an unchanged eye/placement/pose
//! retains the index upload. Sorting is in-place O(T log T), with no per-frame
//! allocations or GPU readback. Other character materials keep shared indices.

use glam::{Mat4, Vec3};

use crate::engine::gltf::PropModel;
use crate::engine::render::common::character::CharacterAnimator;

#[derive(Clone, Copy, Debug)]
pub(super) struct BlendTriangle {
    pub indices: [u16; 3],
    pub submesh: usize,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct OrderedTriangle {
    pub source: usize,
    pub distance: f32,
}

/// A range in one character's view-ordered index buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CharacterSpan {
    pub batch: Option<usize>,
    pub submesh: usize,
    pub first_index: u32,
    pub index_count: u32,
}

impl CharacterSpan {
    /// Only adjacent submissions can merge, preserving intervening families,
    /// instances and materials in both scene and emission passes.
    pub fn append(&mut self, next: Self) -> bool {
        if self.batch != next.batch
            || self.submesh != next.submesh
            || self.first_index.checked_add(self.index_count) != Some(next.first_index)
        {
            return false;
        }
        let Some(count) = self.index_count.checked_add(next.index_count) else {
            return false;
        };
        self.index_count = count;
        true
    }
}

pub(super) struct CharacterTransparency {
    pub buffer: wgpu::Buffer,
    positions: Vec<Vec3>,
    revision: Option<u64>,
    view: Option<(Vec3, Mat4)>,
    pub order: Vec<OrderedTriangle>,
    indices: Vec<u16>,
}

impl CharacterTransparency {
    pub fn new(device: &wgpu::Device, vertices: usize, triangles: usize) -> Self {
        Self {
            buffer: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("places-character-blend-indices"),
                size: u64::try_from(triangles.saturating_mul(3).saturating_mul(2))
                    .unwrap_or(u64::MAX)
                    .next_multiple_of(wgpu::COPY_BUFFER_ALIGNMENT),
                usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            positions: vec![Vec3::ZERO; vertices],
            revision: None,
            view: None,
            order: vec![OrderedTriangle::default(); triangles],
            // u16 index writes must also satisfy wgpu's four-byte alignment.
            indices: vec![0; triangles.saturating_mul(3).next_multiple_of(2)],
        }
    }

    #[cfg(feature = "visual-diagnostics")]
    pub const fn cpu_bytes(&self) -> usize {
        self.positions
            .capacity()
            .saturating_mul(std::mem::size_of::<Vec3>())
            .saturating_add(
                self.order
                    .capacity()
                    .saturating_mul(std::mem::size_of::<OrderedTriangle>()),
            )
            .saturating_add(
                self.indices
                    .capacity()
                    .saturating_mul(std::mem::size_of::<u16>()),
            )
    }

    pub fn prepare(
        &mut self,
        queue: &wgpu::Queue,
        triangles: &[BlendTriangle],
        model: &PropModel,
        animator: &CharacterAnimator,
        view: (Mat4, Vec3, bool),
    ) {
        let (transform, eye, separate_indices) = view;
        if self.revision != Some(animator.revision()) {
            for (index, (position, vertex)) in
                self.positions.iter_mut().zip(&model.vertices).enumerate()
            {
                *position = Vec3::from_array(animator.skin_position(
                    model.joints.get(index).copied().unwrap_or([0; 4]),
                    model.weights.get(index).copied().unwrap_or([0.0; 4]),
                    vertex.pos,
                ));
            }
            self.revision = Some(animator.revision());
            self.view = None;
        }
        if self.view == Some((eye, transform)) {
            return;
        }
        triangle_keys(triangles, &self.positions, transform, eye, &mut self.order);
        if !separate_indices {
            self.view = Some((eye, transform));
            return;
        }
        sort_triangles(&mut self.order);
        for (ordered, target) in self
            .order
            .iter()
            .zip(self.indices.as_chunks_mut::<3>().0.iter_mut())
        {
            if let Some(triangle) = triangles.get(ordered.source) {
                target.copy_from_slice(&triangle.indices);
            }
        }
        queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&self.indices));
        self.view = Some((eye, transform));
    }
}

/// Rebuild keys in source order so equal depths never inherit a previous view's
/// tie order. The same squared-distance convention is used by every family.
fn triangle_keys(
    triangles: &[BlendTriangle],
    positions: &[Vec3],
    transform: Mat4,
    eye: Vec3,
    order: &mut [OrderedTriangle],
) {
    for (source, (triangle, target)) in triangles.iter().zip(order.iter_mut()).enumerate() {
        // Dividing finite admitted positions by three has no integer overflow path.
        #[expect(
            clippy::arithmetic_side_effects,
            reason = "finite Vec3 averaging by a nonzero constant"
        )]
        let centre = triangle
            .indices
            .iter()
            .filter_map(|index| positions.get(usize::from(*index)))
            .copied()
            .sum::<Vec3>()
            / 3.0;
        *target = OrderedTriangle {
            source,
            distance: transform.transform_point3(centre).distance_squared(eye),
        };
    }
}

fn sort_triangles(order: &mut [OrderedTriangle]) {
    order.sort_unstable_by(|left, right| {
        right
            .distance
            .total_cmp(&left.distance)
            .then_with(|| left.source.cmp(&right.source))
    });
}

#[cfg(test)]
fn order_triangles(
    triangles: &[BlendTriangle],
    positions: &[Vec3],
    transform: Mat4,
    eye: Vec3,
    order: &mut [OrderedTriangle],
) {
    triangle_keys(triangles, positions, transform, eye, order);
    sort_triangles(order);
}

#[cfg(test)]
#[path = "../../tests/character_transparency.rs"]
mod tests;
