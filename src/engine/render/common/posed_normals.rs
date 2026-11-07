//! Authored deforming-cavity rigs require normals from their final triangles.
//! Rest-position/normal groups preserve hard edges and material primitive splits.
use super::character::CharacterAnimator;
use crate::engine::gltf::PropModel;
use glam::{DVec3, Vec3};
use std::collections::BTreeMap;

#[must_use]
pub fn required(model: &PropModel) -> bool {
    model.skin.as_ref().is_some_and(|skin| {
        skin.nodes
            .iter()
            .any(|node| node.name == "mouth_capture_socket")
    })
}

pub struct PosedNormals {
    groups: Vec<usize>,
    faces: Vec<[usize; 3]>,
    sums: Vec<DVec3>,
    positions: Vec<DVec3>,
    normals: Vec<[f32; 3]>,
}

impl PosedNormals {
    #[must_use]
    pub fn new(model: &PropModel) -> Option<Self> {
        if !required(model) {
            return None;
        }
        let source = &model.skin.as_ref()?.source_vertices;
        if source.len() != model.vertices.len() {
            return None;
        }
        let mut groups = vec![0; model.vertices.len()];
        let mut keys = BTreeMap::new();
        let mut faces = Vec::with_capacity(model.indices.len() / 3);
        for (primitive, submesh) in model.submeshes.iter().enumerate() {
            let start = usize::try_from(submesh.first_index).ok()?;
            let end = start.checked_add(usize::try_from(submesh.index_count).ok()?)?;
            for face in model.indices.get(start..end)?.as_chunks::<3>().0 {
                let slots = face.map(usize::from);
                for index in slots {
                    let vertex = source.get(index)?;
                    let [x, y, z] = vertex.position.map(|v| quantized_bits(v, 1.0e-6));
                    let [nx, ny, nz] = vertex
                        .normal
                        .unwrap_or([0.0; 3])
                        .map(|v| quantized_bits(v, 1.0e-5));
                    let next = keys.len();
                    let group = *keys
                        .entry((primitive, [x, y, z, nx, ny, nz]))
                        .or_insert(next);
                    *groups.get_mut(index)? = group;
                }
                faces.push(slots);
            }
        }
        Some(Self {
            groups,
            faces,
            sums: vec![DVec3::ZERO; keys.len()],
            positions: vec![DVec3::ZERO; model.vertices.len()],
            normals: vec![[0.0; 3]; model.vertices.len()],
        })
    }

    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Finite imported vertices and bounded skin matrices; angle-weighted reference arithmetic uses double precision."
    )]
    pub fn update(&mut self, model: &PropModel, animator: &CharacterAnimator) {
        for (index, (vertex, position)) in
            model.vertices.iter().zip(&mut self.positions).enumerate()
        {
            let posed = animator.skin_position(
                model.joints.get(index).copied().unwrap_or([0; 4]),
                model.weights.get(index).copied().unwrap_or([0.0; 4]),
                vertex.pos,
            );
            *position = Vec3::from_array(posed).as_dvec3();
        }
        self.sums.fill(DVec3::ZERO);
        for face in &self.faces {
            let [Some(first), Some(second), Some(third)] =
                face.map(|i| self.positions.get(i).copied())
            else {
                continue;
            };
            let points = [first, second, third];
            let normal = (second - first).cross(third - first).normalize_or_zero();
            for (index, corner) in face.iter().enumerate() {
                let Some(point) = points.get(index) else {
                    continue;
                };
                let Some(neighbor_one) = points.get((index + 1) % 3) else {
                    continue;
                };
                let Some(neighbor_two) = points.get((index + 2) % 3) else {
                    continue;
                };
                let edge_one = *neighbor_one - *point;
                let edge_two = *neighbor_two - *point;
                let angle = edge_one
                    .cross(edge_two)
                    .length()
                    .atan2(edge_one.dot(edge_two));
                if let Some(sum) = self.groups.get(*corner).and_then(|g| self.sums.get_mut(*g)) {
                    *sum += normal * angle;
                }
            }
        }
        for (index, out) in self.normals.iter_mut().enumerate() {
            let sum = self
                .groups
                .get(index)
                .and_then(|g| self.sums.get(*g))
                .copied()
                .unwrap_or(DVec3::ZERO);
            *out = sum.normalize_or_zero().as_vec3().to_array();
            if sum.length_squared() <= 1.0e-24_f64 {
                *out = model
                    .vertices
                    .get(index)
                    .and_then(|v| v.normal)
                    .unwrap_or([0.0, 1.0, 0.0]);
            }
        }
    }

    #[must_use]
    pub fn normal(&self, index: usize) -> Option<[f32; 3]> {
        self.normals.get(index).copied()
    }
}

fn quantized_bits(value: f32, step: f64) -> u64 {
    let rounded = (f64::from(value) / step).round_ties_even();
    let bits = rounded.to_bits();
    if bits << 1_u8 == 0_u64 { 0_u64 } else { bits }
}
