//! Opaque receiver topology is built once per asset and refitted to every pose.
//! Degenerate faces remain in the topology so later deformation can revive them.
use crate::engine::lighting::transport::runtime_visibility::{Node, Triangle};
use crate::engine::materials::AlphaMode;
use crate::engine::render::common::character::Character;
use glam::Vec3;

#[derive(Clone)]
struct Face {
    indices: [u16; 3],
    center: Vec3,
    minimum: Vec3,
    maximum: Vec3,
}

pub(super) struct Geometry {
    pub nodes: Vec<Node>,
    pub triangles: Vec<Triangle>,
    faces: Vec<Face>,
}

fn point(points: &[Vec3], index: u16) -> Result<Vec3, String> {
    points
        .get(usize::from(index))
        .copied()
        .ok_or_else(|| "Posed topology vertex is unavailable".into())
}

fn positions(character: &Character) -> Result<Vec<Vec3>, String> {
    let model = &character.asset().model;
    model
        .vertices
        .iter()
        .enumerate()
        .map(|(index, vertex)| {
            let p = Vec3::from_array(character.animator().skin_position(
                model.joints.get(index).copied().unwrap_or([0; 4]),
                model.weights.get(index).copied().unwrap_or([0.0; 4]),
                vertex.pos,
            ));
            if p.is_finite() {
                Ok(p)
            } else {
                Err("Nonfinite posed visibility vertex".into())
            }
        })
        .collect()
}

impl Geometry {
    pub fn new(character: &Character) -> Result<Self, String> {
        let model = &character.asset().model;
        let points = positions(character)?;
        let mut faces = Vec::new();
        for part in model
            .submeshes
            .iter()
            .filter(|part| part.alpha.mode == AlphaMode::Opaque)
        {
            let start = usize::try_from(part.first_index).map_err(|e| e.to_string())?;
            let end = start
                .checked_add(usize::try_from(part.index_count).map_err(|e| e.to_string())?)
                .ok_or("Posed index range overflow")?;
            let indices = model
                .indices
                .get(start..end)
                .ok_or("Posed primitive range is unavailable")?;
            for triangle in indices.as_chunks::<3>().0 {
                let [a, b, c] = *triangle;
                #[expect(
                    clippy::arithmetic_side_effects,
                    reason = "Finite vector centroid arithmetic has no integer overflow or panic path"
                )]
                let center = (point(&points, a)? + point(&points, b)? + point(&points, c)?) / 3.0;
                faces.push(Face {
                    indices: [a, b, c],
                    center,
                    minimum: point(&points, a)?
                        .min(point(&points, b)?)
                        .min(point(&points, c)?),
                    maximum: point(&points, a)?
                        .max(point(&points, b)?)
                        .max(point(&points, c)?),
                });
            }
        }
        let mut nodes = Vec::new();
        if !faces.is_empty() {
            build(&mut faces, 0, &mut nodes)?;
        }
        let mut result = Self {
            nodes,
            triangles: Vec::with_capacity(faces.len()),
            faces,
        };
        result.refit_points(&points)?;
        Ok(result)
    }

    pub fn refit(&mut self, character: &Character) -> Result<(), String> {
        self.refit_points(&positions(character)?)
    }

    fn refit_points(&mut self, points: &[Vec3]) -> Result<(), String> {
        self.triangles.clear();
        for face in &self.faces {
            let [ia, ib, ic] = face.indices;
            let a = point(points, ia)?;
            let b = point(points, ib)?;
            let c = point(points, ic)?;
            #[expect(
                clippy::arithmetic_side_effects,
                reason = "Finite vertex differences have no integer overflow or panic path"
            )]
            let (e0, e1) = (b - a, c - a);
            self.triangles.push(Triangle {
                a_solid: [a.x, a.y, a.z, 1.0],
                edge0: [e0.x, e0.y, e0.z, 0.0],
                edge1: [e1.x, e1.y, e1.z, 0.0],
            });
        }
        for index in (0..self.nodes.len()).rev() {
            let node = *self.nodes.get(index).ok_or("Missing refit node")?;
            let (min, max) = if node.count == 0 {
                let left = self
                    .nodes
                    .get(usize::try_from(node.first).map_err(|e| e.to_string())?)
                    .ok_or("Missing left refit child")?;
                let right = self
                    .nodes
                    .get(usize::try_from(left.escape).map_err(|e| e.to_string())?)
                    .ok_or("Missing right refit child")?;
                (
                    Vec3::from_array(left.minimum).min(Vec3::from_array(right.minimum)),
                    Vec3::from_array(left.maximum).max(Vec3::from_array(right.maximum)),
                )
            } else {
                let start = usize::try_from(node.first).map_err(|e| e.to_string())?;
                let end = start
                    .checked_add(usize::try_from(node.count).map_err(|e| e.to_string())?)
                    .ok_or("Refit leaf overflow")?;
                let mut min = Vec3::splat(f32::INFINITY);
                let mut max = Vec3::splat(f32::NEG_INFINITY);
                for face in self.faces.get(start..end).ok_or("Missing refit leaf")? {
                    for id in face.indices {
                        let p = point(points, id)?;
                        min = min.min(p);
                        max = max.max(p);
                    }
                }
                (min, max)
            };
            let target = self
                .nodes
                .get_mut(index)
                .ok_or("Missing refit destination")?;
            target.minimum = min.to_array();
            target.maximum = max.to_array();
        }
        Ok(())
    }
}

fn build(faces: &mut [Face], base: usize, nodes: &mut Vec<Node>) -> Result<(), String> {
    let index = nodes.len();
    nodes.push(Node {
        minimum: [0.0; 3],
        maximum: [0.0; 3],
        first: u32::try_from(base).map_err(|e| e.to_string())?,
        count: u32::try_from(faces.len()).map_err(|e| e.to_string())?,
        escape: 0,
        ready: 1,
        padding: [0; 2],
    });
    if faces.len() > 4 {
        let mid = partition(faces);
        let first = u32::try_from(nodes.len()).map_err(|e| e.to_string())?;
        let (left, right) = faces.split_at_mut(mid);
        build(left, base, nodes)?;
        build(
            right,
            base.checked_add(mid).ok_or("Topology range overflow")?,
            nodes,
        )?;
        let node = nodes.get_mut(index).ok_or("Missing topology node")?;
        node.first = first;
        node.count = 0;
    }
    let escape = u32::try_from(nodes.len()).map_err(|e| e.to_string())?;
    nodes
        .get_mut(index)
        .ok_or("Missing topology destination")?
        .escape = escape;
    Ok(())
}

// Surface-area partitioning is paid once per asset, then exact current-pose
// vertices refit the retained topology. A centroid median alone made close-view
// ray traversal substantially worse for long thin limbs and folded cavities.
#[derive(Clone, Copy)]
struct Bin {
    minimum: Vec3,
    maximum: Vec3,
    count: u32,
}
impl Bin {
    const EMPTY: Self = Self {
        minimum: Vec3::splat(f32::INFINITY),
        maximum: Vec3::splat(f32::NEG_INFINITY),
        count: 0,
    };
    fn join(self, other: Self) -> Self {
        Self {
            minimum: self.minimum.min(other.minimum),
            maximum: self.maximum.max(other.maximum),
            count: self.count.saturating_add(other.count),
        }
    }
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Finite nonnegative floating point surface-area cost; no integer overflow"
    )]
    fn cost(self) -> f64 {
        if self.count == 0 {
            return 0.0;
        }
        let d = (self.maximum - self.minimum)
            .as_dvec3()
            .max(glam::DVec3::ZERO);
        d.z.mul_add(d.x, d.y.mul_add(d.z, d.x * d.y)) * f64::from(self.count)
    }
}
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::as_conversions,
    reason = "Positive finite centroid extent and clamped 16-bin coordinates make conversion bounded to 0..15"
)]
fn bin_slot(value: f32, minimum: f32, extent: f32) -> usize {
    ((value - minimum) / extent * 16.0).clamp(0.0, 15.0) as usize
}
#[expect(
    clippy::arithmetic_side_effects,
    reason = "Finite centroid extents and nonnegative f64 partition costs"
)]
fn partition(faces: &mut [Face]) -> usize {
    let mut minimum = Vec3::splat(f32::INFINITY);
    let mut maximum = Vec3::splat(f32::NEG_INFINITY);
    for face in faces.iter() {
        minimum = minimum.min(face.center);
        maximum = maximum.max(face.center);
    }
    let extent = maximum - minimum;
    let mut best = None;
    let mut cost = f64::INFINITY;
    for axis in 0..3 {
        if extent[axis] <= 0.0 {
            continue;
        }
        let mut bins = [Bin::EMPTY; 16];
        for face in faces.iter() {
            let slot = bin_slot(face.center[axis], minimum[axis], extent[axis]);
            if let Some(bin) = bins.get_mut(slot) {
                *bin = bin.join(Bin {
                    minimum: face.minimum,
                    maximum: face.maximum,
                    count: 1,
                });
            }
        }
        let mut suffix = [Bin::EMPTY; 16];
        let mut accumulated_right = Bin::EMPTY;
        for (bin, target) in bins.iter().zip(suffix.iter_mut()).rev() {
            accumulated_right = accumulated_right.join(*bin);
            *target = accumulated_right;
        }
        let mut left = Bin::EMPTY;
        for (slot, (bin, right)) in bins.iter().zip(suffix.iter().skip(1)).enumerate() {
            left = left.join(*bin);
            if left.count == 0 || right.count == 0 {
                continue;
            }
            let candidate = left.cost() + right.cost();
            if candidate < cost {
                cost = candidate;
                best = Some((axis, slot));
            }
        }
    }
    if let Some((axis, cut)) = best {
        faces.sort_unstable_by(|a, b| a.center[axis].total_cmp(&b.center[axis]));
        let split = faces.partition_point(|face| {
            bin_slot(face.center[axis], minimum[axis], extent[axis]) <= cut
        });
        if split > 0 && split < faces.len() {
            return split;
        }
    }
    faces.len() / 2
}
