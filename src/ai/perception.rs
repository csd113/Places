//! Perception: sight with range, field of view and collision occlusion, and
//! hearing from gameplay stimuli.
//!
//! Perception is deliberately decoupled from rendering and audio. A stimulus
//! is a typed record — position, radius, loudness, category and source — that
//! gameplay systems publish (movement, doors, switches, spawns). Hearing reads
//! those records; sight raycasts the real static collision boxes and live door
//! leaves. Both are evaluated on a staggered interval per agent by
//! [`crate::ai::AiWorld`], never as a global per-frame scan.

// The navigation/AI runtime is numeric kernel code: bounded `f32` geometry
// over validated finite records, lattice indices converted after their caps
// are enforced, and fixed-size arrays walked by index. Those are exactly the
// shapes the cast/float/index lints flag, so they are allowed here as a unit;
// no other module inherits them, and every allocation and collection access
// still goes through bounds-checked paths.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::float_cmp,
    clippy::imprecise_flops,
    clippy::missing_const_for_fn,
    clippy::needless_range_loop,
    clippy::similar_names,
    clippy::suboptimal_flops,
    clippy::too_many_arguments,
    clippy::too_many_lines
)]

use glam::Vec3;

use crate::collision::{DoorCollider, WallAabb, ray_aabb_entry};
use crate::collision_index::CollisionIndex;
use crate::door::Doors;
use crate::entities::id::EntityHandle;

use super::{AiAgent, AiTickContext};

/// How long a stimulus stays audible, in seconds.
pub const STIMULUS_TTL_S: f32 = 3.0;

/// Largest number of stimuli one world retains.
pub const MAX_STIMULI: usize = 64;

/// One gameplay stimulus an agent can hear.
#[derive(Debug, Clone, PartialEq)]
pub struct Stimulus {
    /// World position where it happened.
    pub position: Vec3,
    /// Audible radius, in metres.
    pub radius: f32,
    /// Relative loudness multiplier, `1.0` ordinary.
    pub loudness: f32,
    /// Category tag (`movement`, `door`, `interact`, `spawn`, ...).
    pub category: String,
    /// The entity that caused it, when known.
    pub source: Option<EntityHandle>,
    /// Age in seconds when it was published.
    pub age: f32,
}

impl Stimulus {
    /// True when the stimulus is still worth hearing.
    #[must_use]
    pub fn is_fresh(&self, extra_age: f32) -> bool {
        self.age + extra_age <= STIMULUS_TTL_S
    }

    /// The audible radius after aging and loudness.
    #[must_use]
    pub fn audible_radius(&self, extra_age: f32) -> f32 {
        let fade = (1.0 - (self.age + extra_age) / STIMULUS_TTL_S).clamp(0.0, 1.0);
        self.radius * self.loudness.max(0.0) * fade
    }
}

/// One perceived contact.
#[derive(Debug, Clone, PartialEq)]
pub struct Perceived {
    /// The source entity.
    pub handle: EntityHandle,
    /// Where it was perceived.
    pub position: Vec3,
    /// Its body radius, when it is an agent.
    pub radius: f32,
    /// Its body height, when it is an agent.
    pub height: f32,
    /// Its advertised role tag (`""` for an anonymous stimulus).
    pub role: String,
    /// True when only hearing found it.
    pub heard_only: bool,
    /// Seconds since the contact was established.
    pub age: f32,
}

/// Perceives the best contact for one agent: the nearest reacting sighting, or
/// the newest audible stimulus.
#[must_use]
pub fn perceive(agent: &AiAgent, ctx: &AiTickContext<'_>) -> Option<Perceived> {
    if let Some(sighted) = sight_target(agent, ctx) {
        return Some(sighted);
    }
    heard(agent, ctx)
}

/// The nearest sighted agent within range, field of view and line of sight.
fn sight_target(agent: &AiAgent, ctx: &AiTickContext<'_>) -> Option<Perceived> {
    if agent.def.sight_range <= 0.0 {
        return None;
    }
    let eye = agent.position + Vec3::new(0.0, (agent.profile.height * 0.5).max(0.05), 0.0);
    let facing = yaw_direction(agent.yaw_degrees);
    let half_fov = (agent.def.sight_fov_degrees * 0.5)
        .to_radians()
        .min(std::f32::consts::PI);
    let mut best: Option<(f32, Perceived)> = None;
    for target in ctx.targets {
        if target.handle == agent.handle || target.caught {
            continue;
        }
        let to_target = target.position - agent.position;
        let distance = to_target.length();
        if distance > agent.def.sight_range + target.radius {
            continue;
        }
        // Across floors: a body more than its own height away vertically is
        // never a sighting, even when the ray misses a thin slab.
        if (target.position.y - agent.position.y).abs() > agent.def.catch_height + target.height {
            continue;
        }
        // A body inside the personal bubble is visible regardless of facing.
        let bubble = agent.profile.radius + target.radius + 0.35;
        if distance > bubble && agent.def.sight_fov_degrees < 360.0 {
            let flat = Vec3::new(to_target.x, 0.0, to_target.z);
            let Some(flat) = flat.try_normalize() else {
                continue;
            };
            let angle = flat.dot(facing).clamp(-1.0, 1.0).acos();
            if angle > half_fov {
                continue;
            }
        }
        let gaze = target.position + Vec3::new(0.0, (target.height * 0.5).max(0.05), 0.0);
        if !sight_clear(ctx.index, ctx.walls, ctx.doors, eye, gaze) {
            continue;
        }
        let contact = Perceived {
            handle: target.handle,
            position: target.position,
            radius: target.radius,
            height: target.height,
            role: target.role.clone().unwrap_or_default(),
            heard_only: false,
            age: 0.0,
        };
        if best
            .as_ref()
            .is_none_or(|(best_distance, _)| distance < *best_distance)
        {
            best = Some((distance, contact));
        }
    }
    best.map(|(_, contact)| contact)
}

/// The newest audible stimulus, resolved to its source's role when it has one.
fn heard(agent: &AiAgent, ctx: &AiTickContext<'_>) -> Option<Perceived> {
    if agent.def.hearing_range <= 0.0 {
        return None;
    }
    let mut best: Option<(f32, Perceived)> = None;
    for stimulus in ctx.stimuli {
        if stimulus.source == Some(agent.handle) {
            continue;
        }
        let distance = stimulus.position.distance(agent.position);
        let radius = stimulus.audible_radius(0.0);
        if radius <= 0.0 || distance > agent.def.hearing_range.min(radius) {
            continue;
        }
        let source_target = stimulus
            .source
            .and_then(|handle| ctx.targets.iter().find(|target| target.handle == handle));
        let contact = Perceived {
            handle: stimulus.source.unwrap_or(agent.handle),
            position: stimulus.position,
            radius: source_target.map_or(0.0, |target| target.radius),
            height: source_target.map_or(0.0, |target| target.height),
            role: source_target
                .and_then(|target| target.role.clone())
                .unwrap_or_default(),
            heard_only: true,
            age: stimulus.age,
        };
        // Prefer the freshest stimulus, then the loudest proximity.
        let score = stimulus.age - radius / 100.0;
        if best
            .as_ref()
            .is_none_or(|(best_score, _)| score < *best_score)
        {
            best = Some((score, contact));
        }
    }
    best.map(|(_, contact)| contact)
}

/// True when nothing blocks the straight segment between two eye points.
///
/// Static collision boxes are tested through the spatial index and live door
/// leaves linearly; the first blocker short-circuits the segment.
#[must_use]
pub fn sight_clear(
    index: &CollisionIndex,
    walls: &[WallAabb],
    doors: &Doors,
    from: Vec3,
    to: Vec3,
) -> bool {
    if !from.is_finite() || !to.is_finite() {
        return false;
    }
    let delta = to - from;
    let distance = delta.length();
    if distance <= 1.0e-4 {
        return true;
    }
    let Some(direction) = delta.try_normalize() else {
        return true;
    };
    let mut nearest = distance;
    index.for_each_ray(from, direction, distance, walls, |wall| {
        if let Some(entry) = ray_aabb_entry(
            from,
            direction,
            [wall.min_x, wall.min_y, wall.min_z],
            [wall.max_x, wall.max_y, wall.max_z],
        ) && entry < nearest
        {
            nearest = entry;
        }
    });
    let colliders = doors.colliders();
    for collider in &colliders {
        if let Some(entry) = collider.ray_entry(from, direction, distance)
            && entry < nearest
        {
            nearest = entry;
        }
    }
    nearest >= distance - 1.0e-4
}

/// [`sight_clear`] against a caller-provided leaf list, for hot loops that
/// already hold the world's live colliders.
#[must_use]
pub fn sight_clear_with_leaves(
    index: &CollisionIndex,
    walls: &[WallAabb],
    leaves: &[DoorCollider],
    from: Vec3,
    to: Vec3,
) -> bool {
    if !from.is_finite() || !to.is_finite() {
        return false;
    }
    let delta = to - from;
    let distance = delta.length();
    if distance <= 1.0e-4 {
        return true;
    }
    let Some(direction) = delta.try_normalize() else {
        return true;
    };
    let mut nearest = distance;
    index.for_each_ray(from, direction, distance, walls, |wall| {
        if let Some(entry) = ray_aabb_entry(
            from,
            direction,
            [wall.min_x, wall.min_y, wall.min_z],
            [wall.max_x, wall.max_y, wall.max_z],
        ) && entry < nearest
        {
            nearest = entry;
        }
    });
    for collider in leaves {
        if let Some(entry) = collider.ray_entry(from, direction, distance)
            && entry < nearest
        {
            nearest = entry;
        }
    }
    nearest >= distance - 1.0e-4
}

/// The unit facing direction of a yaw in degrees.
#[must_use]
pub fn yaw_direction(yaw_degrees: f32) -> Vec3 {
    let yaw = yaw_degrees.to_radians();
    Vec3::new(yaw.sin(), 0.0, yaw.cos())
}
