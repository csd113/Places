//! Bounded presentation geometry. No lights, transport mutation or texture synthesis.
//! Marks are clipped in the struck triangle's barycentric frame, then evaluated
//! from its live skinned vertices or rigid transform. No floating decal quads.

use super::{
    character::CharacterScene,
    dynamic::{DynamicId, DynamicScene},
    effects::EffectVertex,
};
use crate::engine::{lighting::transport::TransportScene, spatial::Aabb};
use glam::Vec3;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

pub const MAX_MARKS: usize = 256;
pub const MAX_DROPS: usize = 96;
pub const MAX_REQUESTS: usize = 8;
pub const MAX_VERTICES: usize = 8192;
const MARK_SECONDS: f32 = 30.0;
const DROP_SECONDS: f32 = 2.0;

/// One finite, at most twelve-metre impact segment. Zero drops makes a mark only.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BloodEmission {
    pub origin: [f32; 3],
    pub direction: [f32; 3],
    pub droplets: u8,
}
impl BloodEmission {
    /// # Errors
    /// Rejects malformed positions, rays and emission counts before queueing work.
    pub fn validate(self) -> Result<(), String> {
        let origin = Vec3::from_array(self.origin);
        let direction = Vec3::from_array(self.direction);
        if !origin.is_finite()
            || origin.abs().max_element() > 100_000.0
            || !direction.is_finite()
            || !(0.001..=12.0).contains(&direction.length())
            || self.droplets > 12
        {
            return Err("blood requires a finite origin within 100000 m, a 0.001..12 m segment and 0..12 droplets".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
enum Attachment {
    World([Vec3; 3]),
    Character { birth: u64, indices: [u16; 3] },
    Dynamic { id: DynamicId, indices: [u16; 3] },
}
#[derive(Clone)]
struct Mark {
    attachment: Attachment,
    polygon: Vec<Vec3>,
    age: f32,
    side: f32,
}
struct Drop {
    position: Vec3,
    velocity: Vec3,
    age: f32,
}
struct Hit {
    attachment: Attachment,
    points: [Vec3; 3],
    point: Vec3,
    normal: Vec3,
    t: f32,
}

/// All active state belongs to one installed world; reset and unload clear it.
#[derive(Default)]
pub struct BloodScene {
    enabled: bool,
    marks: VecDeque<Mark>,
    drops: VecDeque<Drop>,
    pending: VecDeque<BloodEmission>,
    pub rejected: u64,
    pub impacts: u64,
}
impl BloodScene {
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.clear();
        }
    }
    pub fn clear(&mut self) {
        self.marks.clear();
        self.drops.clear();
        self.pending.clear();
        self.rejected = 0;
        self.impacts = 0;
    }
    pub fn emit(&mut self, request: BloodEmission) {
        if !self.enabled {
            return;
        }
        if request.validate().is_err() || self.pending.len() >= MAX_REQUESTS {
            self.rejected = self.rejected.saturating_add(1);
            return;
        }
        self.pending.push_back(request);
    }
    #[must_use]
    pub fn diagnostics(&self) -> serde_json::Value {
        serde_json::json!({"enabled":self.enabled,"marks":self.marks.len(),"droplets":self.drops.len(),
            "pending":self.pending.len(),"impacts":self.impacts,"rejected":self.rejected,
            "caps":{"marks":MAX_MARKS,"droplets":MAX_DROPS,"requests":MAX_REQUESTS,"vertices":MAX_VERTICES},
            "mark_seconds":MARK_SECONDS,"drop_seconds":DROP_SECONDS})
    }
    #[must_use]
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }
    #[must_use]
    pub fn has_geometry(&self) -> bool {
        !self.marks.is_empty() || !self.drops.is_empty()
    }
    #[must_use]
    pub fn attachment_diagnostics(
        &self,
        characters: &CharacterScene,
        dynamic: &DynamicScene,
    ) -> serde_json::Value {
        let samples:Vec<_>=self.marks.iter().take(24).map(|mark| {
            let resolved=resolve(&mark.attachment,characters,dynamic);
            let owner=match &mark.attachment {Attachment::World(_)=>"world",Attachment::Character{birth:_,indices:_}=>"character",Attachment::Dynamic{id:_,indices:_}=>"dynamic"};
            serde_json::json!({"owner":owner,"identity":format!("{:?}",mark.attachment),"age":mark.age,
                "world_anchor":resolved.map(|(points,_)|mix(points,Vec3::splat(1.0/3.0)).to_array()),
                "barycentric_polygon":mark.polygon.iter().map(Vec3::to_array).collect::<Vec<_>>(),
                "live_triangle":resolved.map(|(points,_)|points.map(|point|point.to_array()))})
        }).collect();
        serde_json::json!(samples)
    }
    /// Advances bounded swept particles against the current complete posed scene.
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Finite validated rays, bounded velocities and delta <= 0.05 keep this particle integration finite."
    )]
    pub fn update(
        &mut self,
        delta: f32,
        characters: &CharacterScene,
        dynamic: &DynamicScene,
        world: Option<&TransportScene>,
    ) {
        if !self.enabled {
            return;
        }
        let elapsed = if delta.is_finite() {
            delta.max(0.0)
        } else {
            0.0
        };
        for mark in &mut self.marks {
            mark.age += elapsed;
        }
        self.marks.retain(|mark| {
            mark.age < MARK_SECONDS && resolve(&mark.attachment, characters, dynamic).is_some()
        });
        if self.pending.is_empty() && self.drops.is_empty() {
            return;
        }
        let collision = CollisionFrame::new(characters, dynamic, world);
        while let Some(request) = self.pending.pop_front() {
            let origin = Vec3::from_array(request.origin);
            let direction = Vec3::from_array(request.direction);
            if let Some(hit) = collision.cast(origin, direction) {
                let source = hit.point + hit.normal * 0.012;
                let normal = hit.normal;
                self.add_mark(hit, 0.12);
                for index in 0..request.droplets {
                    let phase = f32::from(index) * 2.399_963_1;
                    let tangent = normal.any_orthonormal_vector();
                    let bitangent = normal.cross(tangent);
                    let velocity = normal * 1.7
                        + Vec3::Y * 0.9
                        + (tangent * phase.cos() + bitangent * phase.sin())
                            * f32::from(index).mul_add(0.09, 0.5);
                    if self.drops.len() >= MAX_DROPS {
                        let _old = self.drops.pop_front();
                    }
                    self.drops.push_back(Drop {
                        position: source,
                        velocity,
                        age: 0.0,
                    });
                }
            }
        }
        // A suspended/paused application cannot launch a long catch-up ray through rooms.
        if elapsed > 0.1 {
            self.drops.clear();
            return;
        }
        let dt = elapsed.min(0.05);
        let count = self.drops.len();
        for _ in 0..count {
            let Some(mut drop) = self.drops.pop_front() else {
                break;
            };
            drop.age += elapsed;
            if drop.age >= DROP_SECONDS {
                continue;
            }
            let displacement = drop.velocity * dt + Vec3::NEG_Y * (4.905 * dt * dt);
            if let Some(hit) = collision.cast(drop.position, displacement) {
                self.add_mark(hit, 0.035);
            } else {
                drop.position += displacement;
                drop.velocity += Vec3::NEG_Y * (9.81 * dt);
                self.drops.push_back(drop);
            }
        }
    }
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Finite validated struck triangle vertices; the cross product defines only its orientation."
    )]
    fn add_mark(&mut self, hit: Hit, radius: f32) {
        let polygon = clip_mark(hit.points, hit.point, hit.normal, radius);
        if polygon.len() < 3 {
            return;
        }
        if self.marks.len() >= MAX_MARKS {
            let _old = self.marks.pop_front();
        }
        let [a, b, c] = hit.points;
        let side = (b - a).cross(c - a).dot(hit.normal).signum();
        self.marks.push_back(Mark {
            attachment: hit.attachment,
            polygon,
            age: 0.0,
            side,
        });
        self.impacts = self.impacts.saturating_add(1);
    }
    /// Writes triangle lists into a pre-sized backend buffer. Ordinary depth testing
    /// supplies occlusion; marks never enter the light/shadow caster registries.
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Ages and fade denominators are bounded positive values; positions are validated live mesh vertices."
    )]
    pub fn vertices(
        &self,
        characters: &CharacterScene,
        dynamic: &DynamicScene,
        eye: Vec3,
        output: &mut Vec<EffectVertex>,
    ) {
        output.clear();
        for mark in &self.marks {
            let Some((points, opacity)) = resolve(&mark.attachment, characters, dynamic) else {
                continue;
            };
            let [a, b, c] = points;
            let normal = (b - a).cross(c - a).normalize_or_zero() * mark.side;
            let lift = normal * 0.0008;
            let alpha = ((MARK_SECONDS - mark.age) / 5.0).clamp(0.0, 1.0) * opacity;
            let Some(first) = mark.polygon.first() else {
                continue;
            };
            for edge in mark.polygon.get(1..).unwrap_or_default().windows(2) {
                let [bary_b, bary_c] = edge else {
                    continue;
                };
                for bary in [first, bary_b, bary_c] {
                    output.push(EffectVertex {
                        position: (mix(points, *bary) + lift).to_array(),
                        color: [0.19, 0.006, 0.009, alpha],
                        uv: [0.5; 2],
                    });
                }
            }
        }
        for drop in &self.drops {
            let toward = (eye - drop.position).normalize_or_zero();
            let right = toward.any_orthonormal_vector() * 0.009;
            let up = toward.cross(right);
            for point in [
                drop.position + up,
                drop.position - right - up,
                drop.position + right - up,
            ] {
                output.push(EffectVertex {
                    position: point.to_array(),
                    color: [0.24, 0.008, 0.012, 1.0],
                    uv: [0.5; 2],
                });
            }
        }
        output.truncate(MAX_VERTICES);
    }
}

#[expect(
    clippy::arithmetic_side_effects,
    reason = "Barycentric weights are finite clipped convex combinations of validated vertices."
)]
fn mix([a, b, c]: [Vec3; 3], bary: Vec3) -> Vec3 {
    a * bary.x + b * bary.y + c * bary.z
}
fn resolve(
    attachment: &Attachment,
    characters: &CharacterScene,
    dynamic: &DynamicScene,
) -> Option<([Vec3; 3], f32)> {
    match attachment {
        Attachment::World(points) => Some((*points, 1.0)),
        Attachment::Character { birth, indices } => {
            let character = characters
                .characters()
                .iter()
                .chain(characters.runtime_characters())
                .find(|c| c.blood_identity() == *birth && c.is_visible())?;
            let [a, b, c] = *indices;
            Some((
                [
                    character.posed_vertex(a)?,
                    character.posed_vertex(b)?,
                    character.posed_vertex(c)?,
                ],
                character.opacity(),
            ))
        }
        Attachment::Dynamic { id, indices } => {
            let object = dynamic.get(*id).filter(|object| object.visible())?;
            let transform = object.transform();
            let point = |index: u16| {
                object
                    .mesh()
                    .vertices
                    .get(usize::from(index))
                    .map(|v| transform.transform_point3(Vec3::from_array(v.pos)))
            };
            let [a, b, c] = *indices;
            Some(([point(a)?, point(b)?, point(c)?], 1.0))
        }
    }
}

enum Owner {
    Character(u64),
    Dynamic(DynamicId),
}
struct Receiver<'a> {
    owner: Owner,
    bounds: Aabb,
    vertices: Vec<Vec3>,
    indices: Vec<&'a [u16]>,
}
struct CollisionFrame<'a> {
    receivers: Vec<Receiver<'a>>,
    world: Option<&'a TransportScene>,
}
impl<'a> CollisionFrame<'a> {
    fn new(
        characters: &'a CharacterScene,
        dynamic: &'a DynamicScene,
        world: Option<&'a TransportScene>,
    ) -> Self {
        let mut receivers = Vec::new();
        for character in characters
            .characters()
            .iter()
            .chain(characters.runtime_characters())
            .filter(|c| c.is_visible() && c.opacity() > 0.0)
        {
            let model = &character.asset().model;
            let vertices: Vec<Vec3> = (0..model.vertices.len())
                .filter_map(|i| {
                    u16::try_from(i)
                        .ok()
                        .and_then(|slot| character.posed_vertex(slot))
                })
                .collect();
            let mut bounds = Aabb::EMPTY;
            for point in &vertices {
                bounds.expand(point.to_array());
            }
            receivers.push(Receiver {
                owner: Owner::Character(character.blood_identity()),
                bounds,
                vertices,
                indices: model
                    .submeshes
                    .iter()
                    .filter(|submesh| {
                        submesh.alpha.mode == crate::engine::materials::AlphaMode::Opaque
                    })
                    .filter_map(|submesh| {
                        index_range(&model.indices, submesh.first_index, submesh.index_count)
                    })
                    .collect(),
            });
        }
        for object in dynamic.objects().iter().filter(|o| o.visible()) {
            let transform = object.transform();
            receivers.push(Receiver {
                owner: Owner::Dynamic(object.id()),
                bounds: object.world_bounds(),
                vertices: object
                    .mesh()
                    .vertices
                    .iter()
                    .map(|v| transform.transform_point3(Vec3::from_array(v.pos)))
                    .collect(),
                indices: object
                    .mesh()
                    .submeshes
                    .iter()
                    .filter(|submesh| {
                        submesh.alpha.mode == crate::engine::materials::AlphaMode::Opaque
                    })
                    .filter_map(|submesh| {
                        index_range(
                            &object.mesh().indices,
                            submesh.first_index,
                            submesh.index_count,
                        )
                    })
                    .collect(),
            });
        }
        Self { receivers, world }
    }
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Finite nonzero segment and normalized triangle normals bound hit reconstruction."
    )]
    fn cast(&self, origin: Vec3, direction: Vec3) -> Option<Hit> {
        if direction.length_squared() < 1.0e-12 {
            return None;
        }
        let mut best = self.world.and_then(|world| {
            let (t, index) = world.intersect(origin.to_array(), direction.to_array())?;
            if !(0.0001..=1.0).contains(&t) {
                return None;
            }
            let triangle = world.hit_triangle(index)?;
            let points = [triangle.p0, triangle.p1, triangle.p2].map(Vec3::from_array);
            let normal = Vec3::from_array(triangle.normal);
            Some(Hit {
                attachment: Attachment::World(points),
                points,
                point: origin + direction * t,
                normal: if normal.dot(direction) > 0.0 {
                    -normal
                } else {
                    normal
                },
                t,
            })
        });
        for receiver in &self.receivers {
            if !segment_bounds(origin, direction, receiver.bounds) {
                continue;
            }
            for triangle in receiver
                .indices
                .iter()
                .flat_map(|indices| indices.as_chunks::<3>().0)
            {
                let [a, b, c] = triangle;
                let point = |index: &u16| receiver.vertices.get(usize::from(*index)).copied();
                let (Some(a_pos), Some(b_pos), Some(c_pos)) = (point(a), point(b), point(c)) else {
                    continue;
                };
                let points = [a_pos, b_pos, c_pos];
                let Some(t) = triangle_hit(origin, direction, points) else {
                    continue;
                };
                if best.as_ref().is_some_and(|hit| hit.t <= t) {
                    continue;
                }
                let mut normal = (b_pos - a_pos).cross(c_pos - a_pos).normalize_or_zero();
                if normal.dot(direction) > 0.0 {
                    normal = -normal;
                }
                let indices = [*a, *b, *c];
                let attachment = match receiver.owner {
                    Owner::Character(birth) => Attachment::Character { birth, indices },
                    Owner::Dynamic(id) => Attachment::Dynamic { id, indices },
                };
                best = Some(Hit {
                    attachment,
                    points,
                    point: origin + direction * t,
                    normal,
                    t,
                });
            }
        }
        best
    }
}

fn segment_bounds(origin: Vec3, direction: Vec3, bounds: Aabb) -> bool {
    let mut near = 0.0_f32;
    let mut far = 1.0_f32;
    for (o, d, min, max) in [
        (origin.x, direction.x, bounds.min[0], bounds.max[0]),
        (origin.y, direction.y, bounds.min[1], bounds.max[1]),
        (origin.z, direction.z, bounds.min[2], bounds.max[2]),
    ] {
        if d.abs() < 1.0e-8 {
            if o < min || o > max {
                return false;
            }
        } else {
            let a = (min - o) / d;
            let b = (max - o) / d;
            near = near.max(a.min(b));
            far = far.min(a.max(b));
            if near > far {
                return false;
            }
        }
    }
    true
}
#[expect(
    clippy::arithmetic_side_effects,
    reason = "Nondegenerate determinant is checked before division; finite bounded mesh coordinates cannot overflow f32 products."
)]
fn triangle_hit(origin: Vec3, direction: Vec3, [first, second, third]: [Vec3; 3]) -> Option<f32> {
    let base_edge = second - first;
    let tip_edge = third - first;
    let cross_ray = direction.cross(tip_edge);
    let determinant = base_edge.dot(cross_ray);
    if determinant.abs() < 1.0e-9 {
        return None;
    }
    let offset = origin - first;
    let first_weight = offset.dot(cross_ray) / determinant;
    if !(0.0..=1.0).contains(&first_weight) {
        return None;
    }
    let cross_offset = offset.cross(base_edge);
    let other_weight = direction.dot(cross_offset) / determinant;
    if other_weight < 0.0 || first_weight + other_weight > 1.0 {
        return None;
    }
    let fraction = tip_edge.dot(cross_offset) / determinant;
    (0.0001..=1.0).contains(&fraction).then_some(fraction)
}

/// Clips a small asymmetric hexagon to the struck triangle without stretching a
/// quad across a silhouette, joint gap, unrelated surface or wall corner.
#[expect(
    clippy::arithmetic_side_effects,
    reason = "Six unit half-planes clip finite bounded points; intersection division only crosses opposite signed distances."
)]
fn clip_mark(points: [Vec3; 3], center: Vec3, normal: Vec3, radius: f32) -> Vec<Vec3> {
    let tangent = normal.any_orthonormal_vector();
    let bitangent = normal.cross(tangent);
    let mut polygon = vec![Vec3::X, Vec3::Y, Vec3::Z];
    for (index, extent) in [0.85_f32, 0.65, 0.95, 0.75, 0.9, 0.6]
        .into_iter()
        .enumerate()
    {
        let angle = f32::from(u16::try_from(index).unwrap_or(0)) * std::f32::consts::FRAC_PI_3;
        let axis = tangent * angle.cos() + bitangent * angle.sin();
        let distance = |bary: Vec3| radius.mul_add(-extent, (mix(points, bary) - center).dot(axis));
        let mut output = Vec::with_capacity(9);
        let Some(mut previous) = polygon.last().copied() else {
            break;
        };
        let mut before = distance(previous);
        for current in polygon {
            let after = distance(current);
            if (before <= 0.0) != (after <= 0.0) {
                output.push(previous.lerp(current, before / (before - after)));
            }
            if after <= 0.0 {
                output.push(current);
            }
            previous = current;
            before = after;
        }
        polygon = output;
    }
    polygon
}

fn index_range(indices: &[u16], first: u32, count: u32) -> Option<&[u16]> {
    let start = usize::try_from(first).ok()?;
    let end = start.checked_add(usize::try_from(count).ok()?)?;
    indices.get(start..end)
}

#[cfg(test)]
#[path = "../../tests/blood.rs"]
mod tests;
