//! Native diagnostic geometry from the actual resident irradiance field.

use glam::Vec3;
use std::ops::{Add, Mul, Sub};

use crate::lighting::LevelLighting;
use crate::lighting::probes::ProbeField;
use crate::render::{LevelMesh, LevelMeshBatches, LevelMeshRange, SurfaceKey, SurfaceKind, Vertex};
use crate::spatial::Aabb;

/// Bounded lattice markers and accepted interpolation links. No texture is generated.
pub(super) fn mesh(
    field: &ProbeField,
    lighting: &LevelLighting,
    anchor: Option<[f32; 3]>,
    visibility: Option<&crate::render::common::dynamic_visibility::DynamicVisibility>,
    receiver: Option<crate::render::DynamicId>,
) -> LevelMesh {
    let candidates = anchor.map_or_else(Vec::new, |position| {
        field.sample_diagnostics_with_rooms(position, None, |probe, label| {
            lighting.labelled_probe_visible_from(position, probe, label)
                && visibility
                    .is_none_or(|scene| scene.transmittance(position, probe, receiver) > 0.0)
        })
    });
    // 24 vertices per marker plus query links stay inside one u16 mesh range.
    let stride = field.probes.len().div_ceil(2048).max(1);
    let mut range = LevelMeshRange {
        key: SurfaceKey::bare(SurfaceKind::PropFallback),
        vertices: Vec::new(),
        indices: Vec::new(),
        bounds: Aabb::EMPTY,
    };
    for (id, sample) in field.probes.iter().enumerate().step_by(stride) {
        let Some(position) = field.probe_position(id) else {
            continue;
        };
        let weight = candidates
            .iter()
            .find(|candidate| candidate.id == id)
            .map(|candidate| candidate.weight);
        let color = if let Some(accepted_weight) = weight {
            #[expect(
                clippy::as_conversions,
                clippy::cast_possible_truncation,
                reason = "Normalized diagnostic weights are finite in 0..1; narrowing only changes marker brightness below display precision."
            )]
            let normalized = accepted_weight as f32;
            [0.1, normalized.mul_add(0.6, 0.4), 1.0]
        } else if !sample.is_valid() {
            [1.0, 0.12, 0.08]
        } else if let Some(query) = anchor {
            if Vec3::from_array(query).distance(Vec3::from_array(position)) >= field.cell_m * 2.0 {
                continue;
            }
            if lighting.labelled_probe_visible_from(query, position, sample.room)
                && visibility
                    .is_none_or(|scene| scene.transmittance(query, position, receiver) > 0.0)
            {
                [0.25; 3]
            } else {
                [1.0, 0.3, 0.1]
            }
        } else {
            [0.15, 0.95, 0.25]
        };
        octahedron(&mut range, Vec3::from_array(position), 0.06, color);
    }
    if let Some(query) = anchor {
        let origin = Vec3::from_array(query);
        octahedron(&mut range, origin, 0.09, [1.0, 0.85, 0.1]);
        for candidate in candidates {
            let target = Vec3::from_array(candidate.world_position);
            let segment = target.sub(origin);
            let side = segment.cross(Vec3::Y).normalize_or_zero().mul(0.012);
            triangle(
                &mut range,
                [origin.sub(side), origin.add(side), target],
                [0.05, 0.9, 1.0],
            );
            triangle(
                &mut range,
                [target, origin.add(side), origin.sub(side)],
                [0.05, 0.9, 1.0],
            );
        }
    }
    LevelMesh {
        vertex_count: range.vertices.len(),
        index_count: range.indices.len(),
        ranges: vec![range],
        batches: LevelMeshBatches::default(),
    }
}

fn octahedron(range: &mut LevelMeshRange, center: Vec3, radius: f32, color: [f32; 3]) {
    for up in [Vec3::Y, Vec3::NEG_Y] {
        for (a, b) in [
            (Vec3::X, Vec3::Z),
            (Vec3::Z, Vec3::NEG_X),
            (Vec3::NEG_X, Vec3::NEG_Z),
            (Vec3::NEG_Z, Vec3::X),
        ] {
            triangle(
                range,
                [
                    center.add(up.mul(radius)),
                    center.add(a.mul(radius)),
                    center.add(b.mul(radius)),
                ],
                color,
            );
        }
    }
}

fn triangle(range: &mut LevelMeshRange, points: [Vec3; 3], color: [f32; 3]) {
    let [red, green, blue] = color;
    let Ok(first) = u16::try_from(range.vertices.len()) else {
        return;
    };
    if first > u16::MAX.saturating_sub(3) {
        return;
    }
    for (offset, point) in points.into_iter().enumerate() {
        let Ok(vertex_offset) = u16::try_from(offset) else {
            return;
        };
        range.indices.push(first.saturating_add(vertex_offset));
        // Alpha 2 is a feature-only marker flag; the opaque diagnostic route
        // does not interpret it as authored coverage or light energy.
        range.vertices.push(Vertex::new(
            point.to_array(),
            [red, green, blue, 2.0],
            [0.0; 2],
        ));
        range.bounds.expand(point.to_array());
    }
}
