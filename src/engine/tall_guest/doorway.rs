//! Derive directional bow staging from the native, deformed leading geometry.
use super::{BLEND, Guest, MARGIN, Passage};
use crate::engine::level::{LevelDef, LevelSurfaces};
use glam::{Mat4, Quat, Vec3};
use serde::Serialize;

#[derive(Clone, Serialize)]
pub(super) struct Stage {
    pub passage: Passage,
    plane: Vec3,
    direction: Vec3,
}
impl Stage {
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Finite world-space side-of-plane checks select only the current entry side."
    )]
    pub fn approach(&self, position: Vec3, goal: Vec3) -> Option<Vec3> {
        let entry = Vec3::from(self.passage.entry);
        let plane = if self.passage.crouch_only {
            entry
        } else {
            self.plane
        };
        ((position - plane).dot(self.direction) <= 0.05
            && (goal - plane).dot(self.direction) > 0.05
            && position.distance(entry) <= 8.0)
            .then_some(entry)
    }
}
struct Aperture {
    near: Vec3,
    far: Vec3,
    tangent: Vec3,
    center: Vec3,
    width: f32,
    top: f32,
}

pub(super) fn resolve(guest: &mut Guest, level: &LevelDef) -> Vec<Stage> {
    let mut result = Vec::new();
    for passage in guest.definition.passages.clone() {
        for reverse in [false, true] {
            let mut directed = passage.clone();
            if reverse {
                std::mem::swap(&mut directed.entry, &mut directed.exit);
            }
            result.push(derive(guest, level, directed));
        }
    }
    result
}

#[expect(
    clippy::arithmetic_side_effects,
    reason = "Finite metre-space staging retains the fixed model scale; all candidates still require the full live swept-pose check."
)]
fn derive(guest: &mut Guest, level: &LevelDef, mut passage: Passage) -> Stage {
    let entry = Vec3::from(passage.entry);
    let exit = Vec3::from(passage.exit);
    let direction = (exit - entry).normalize_or_zero();
    let Some(aperture) = aperture(level, entry, exit) else {
        return Stage {
            passage,
            plane: entry.midpoint(exit),
            direction,
        };
    };
    if !passage.crouch_only {
        let (leading, trailing) = envelope(guest, &aperture, entry, direction);
        let distance = (aperture.near - entry).dot(direction);
        let bow = entry + direction * (distance - leading - MARGIN - 0.025);
        let far = (aperture.far - entry).dot(direction);
        let clear = entry + direction * (far + trailing + MARGIN + 0.025);
        passage.entry = bow.to_array();
        if (exit - clear).dot(direction) < 0.0 {
            passage.exit = clear.to_array();
        }
    }
    Stage {
        passage,
        plane: aperture.near.midpoint(aperture.far),
        direction,
    }
}

#[expect(
    clippy::arithmetic_side_effects,
    reason = "Finite aperture projections are bounded by validated level geometry; near-parallel approaches retain authored staging."
)]
fn aperture(level: &LevelDef, entry: Vec3, exit: Vec3) -> Option<Aperture> {
    let delta = exit - entry;
    let surfaces = LevelSurfaces::new(level);
    let mut nearest: Option<(f32, Aperture)> = None;
    for wall in &level.walls {
        let along_x = wall.width >= wall.depth;
        let normal = if along_x { Vec3::Z } else { Vec3::X };
        let tangent = if along_x { Vec3::X } else { Vec3::Z };
        let direction = delta.normalize_or_zero();
        if direction.dot(normal).abs() < 0.995 {
            continue;
        }
        let center = Vec3::new(
            wall.width.mul_add(0.5, wall.x),
            0.0,
            wall.depth.mul_add(0.5, wall.z),
        );
        let depth = if along_x { wall.depth } else { wall.width };
        let mid = center.dot(normal);
        let fraction = (mid - entry.dot(normal)) / delta.dot(normal);
        if !(0.0..=1.0).contains(&fraction) {
            continue;
        }
        let crossing = entry + delta * fraction;
        for opening in &wall.openings {
            let length_origin = if along_x { wall.x } else { wall.z };
            let cut = opening.width.mul_add(0.5, length_origin + opening.offset);
            if (crossing.dot(tangent) - cut).abs() > opening.width * 0.5 {
                continue;
            }
            let floor = surfaces
                .floor_y_at(crossing.x, crossing.z)
                .unwrap_or(entry.y);
            let base = floor + wall.y + opening.sill;
            let portal = normal * mid + tangent * cut + Vec3::Y * base;
            let sign = direction.dot(normal).signum();
            let candidate = Aperture {
                near: portal - normal * (sign * depth * 0.5),
                far: portal + normal * (sign * depth * 0.5),
                tangent,
                center: portal,
                width: opening.width,
                top: base + opening.height,
            };
            if nearest
                .as_ref()
                .is_none_or(|(distance, _)| fraction < *distance)
            {
                nearest = Some((fraction, candidate));
            }
        }
    }
    nearest.map(|(_, aperture)| aperture)
}

#[expect(
    clippy::arithmetic_side_effects,
    reason = "Native posed vertices determine the lintel/jamb envelope; no engine limit or visual scale is altered."
)]
fn envelope(guest: &mut Guest, aperture: &Aperture, entry: Vec3, direction: Vec3) -> (f32, f32) {
    let saved = guest.state.clone();
    let heights = guest.support_heights;
    let yaw = direction.x.atan2(direction.z);
    let rotation = Mat4::from_quat(Quat::from_rotation_y(yaw));
    let side = (entry - aperture.center).dot(aperture.tangent);
    let mut leading = 0.0_f32;
    let mut trailing = 0.0_f32;
    guest.state.position = Vec3::ZERO;
    guest.state.yaw = 0.0;
    guest.state.aim_weight = 0.0;
    guest.state.base_posture = 0.0;
    guest.state.blend_time = BLEND;
    guest.support_heights = [0.0; 3];
    guest.state.plants = [None, None, None];
    for (clip, seconds) in [
        ("door_enter", 1.2),
        ("run_pursuit", 0.8),
        ("door_squeeze", 1.8),
    ] {
        guest.state.clip = clip.into();
        guest.state.plants = [None, None, None];
        for frame in 0_u16..=216 {
            let time = f32::from(frame) / 120.0;
            if time > seconds + 0.0001 {
                break;
            }
            guest.state.clip_time = time;
            guest.sample();
            guest.fill_vertices();
            for vertex in &guest.vertices {
                let point = rotation.transform_point3(*vertex);
                if clip == "door_squeeze" {
                    trailing = trailing.max(-point.dot(direction));
                } else if point.y + entry.y + MARGIN > aperture.top
                    || (point.dot(aperture.tangent) + side).abs() + MARGIN > aperture.width * 0.5
                {
                    leading = leading.max(point.dot(direction));
                }
            }
        }
    }
    guest.state = saved;
    guest.support_heights = heights;
    guest.sample();
    guest.fill_vertices();
    (leading, trailing)
}
