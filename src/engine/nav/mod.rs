//! Runtime navigation: baked-mesh loading, nearest-point queries, path
//! queries and the door links that make routes react to live world state.
//!
//! The offline compiler derives walkable space from geometry. This module
//! owns the shared movement rules and the runtime query half:
//!
//! * [`query`] (`NavMesh`) is the runtime half: it validates and loads a baked
//!   [`NavGrid`](crate::engine::package::navigation::NavGrid), answers nearest-point
//!   and path queries against the class that matches the agent's body, and
//!   consults [`NavDoorState`] so a closed or locked door blocks a route
//!   without touching the mesh.
//!
//! A query never mutates the mesh; the caller owns its own [`query::NavScratch`]
//! so pathfinding reuses allocations across frames and agents.

pub mod query;

pub use query::{NavBounds, NavMesh, NavPoint, NavScratch, Path, PathQuery, PathResult};

use crate::engine::door::{DoorPhase, Doors};
use crate::engine::level::{MAX_RAMP_SLOPE, MAX_STAIR_RISER_M, MIN_STAIR_TREAD_M};
use crate::engine::package::navigation::NavClass;

/// The largest slope the bake treats as walkable, in rise per metre of run.
///
/// The loader already bounds ramps at [`MAX_RAMP_SLOPE`] and stairs at
/// `MAX_STAIR_RISER_M / MIN_STAIR_TREAD_M`; this is the larger of the two, so
/// the bake never refuses authored, walkable stair geometry.
pub const NAV_MAX_SLOPE: f32 = {
    let stair = MAX_STAIR_RISER_M / MIN_STAIR_TREAD_M;
    if stair > MAX_RAMP_SLOPE {
        stair
    } else {
        MAX_RAMP_SLOPE
    }
};

/// The movement substep the shared agent mover advances in metres.
///
/// The bake certifies links at this spacing and runtime motion uses no larger
/// step. Surface identity and [`neighbour_rule`] independently enforce each
/// body's discrete step/drop and continuous slope limits.
pub const NAV_MOVE_SUBSTEP_M: f32 = 0.05;

/// Cell flag bit: the surface under this cell is part of a continuous slope.
pub const CELL_SLOPED: u8 = 1 << 1;

/// The reference agent class: a standing human body.
///
/// Every map bakes this class so a caller never needs an authored `nav_agent`
/// to get a usable mesh.
#[must_use]
pub const fn reference_class() -> NavClass {
    NavClass {
        radius: crate::engine::collision::PLAYER_RADIUS,
        height: crate::engine::collision::PLAYER_HEIGHT,
        step_height: crate::engine::collision::PLAYER_STEP_HEIGHT,
        max_slope: NAV_MAX_SLOPE,
    }
}

/// The one step/slope neighbour rule, shared by the bake and the runtime.
///
/// Continuous slopes must respect `max_slope` even when fine samples rise less
/// than a full step. Other rises and drops are bounded symmetrically by
/// `step_height`, so a cliff never becomes a fine-sampled ramp.
#[must_use]
pub fn neighbour_rule(
    ya: f32,
    yb: f32,
    distance: f32,
    flags_a: u8,
    flags_b: u8,
    profile: &NavClass,
) -> bool {
    let rise = (yb - ya).abs();
    let sloped = flags_a & CELL_SLOPED != 0 && flags_b & CELL_SLOPED != 0;
    if sloped && rise > crate::engine::collision::STEP_EPS {
        return distance > f32::EPSILON && rise / distance <= profile.max_slope + 1.0e-4;
    }
    rise <= profile.step_height + crate::engine::collision::STEP_EPS
}
/// The physical body an agent navigates with.
///
/// This is the runtime mirror of a baked [`NavClass`]; `can_open_doors` is a
/// capability, not part of the baked class, so two agents with the same body
/// share one class while differing in what they can operate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NavAgentProfile {
    /// Body disc radius in metres.
    pub radius: f32,
    /// Body height in metres.
    pub height: f32,
    /// Largest surface rise walked without a route, in metres.
    pub step_height: f32,
    /// Largest walkable rise per metre of run.
    pub max_slope: f32,
    /// True when the agent may open an unlocked door blocking its route.
    pub can_open_doors: bool,
}

impl NavAgentProfile {
    /// The reference profile: the player's own body, allowed to open doors.
    #[must_use]
    pub const fn reference() -> Self {
        let class = reference_class();
        Self {
            radius: class.radius,
            height: class.height,
            step_height: class.step_height,
            max_slope: class.max_slope,
            can_open_doors: true,
        }
    }

    /// The physical class to look up in a baked mesh.
    #[must_use]
    pub const fn class(&self) -> NavClass {
        NavClass {
            radius: self.radius,
            height: self.height,
            step_height: self.step_height,
            max_slope: self.max_slope,
        }
    }

    /// True when every field is finite and positive.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.class().is_valid()
    }
}

/// The live door state navigation queries against.
///
/// The trait exists so a path query can be driven by the real [`Doors`]
/// runtime, by a test double, or by a future overlay without the nav module
/// owning door policy.
pub trait NavDoorState {
    /// True when a live, non-baked solid overlaps the body at this surface.
    ///
    /// The default keeps static/editor queries allocation-free. Runtime callers
    /// provide a bounded snapshot of physical item bounds without rebuilding
    /// the baked topology.
    fn obstacle_blocked(&self, _position: glam::Vec3, _radius: f32, _height: f32) -> bool {
        false
    }

    /// True when a live solid intersects a walked segment's full body envelope.
    /// Runtime overlays override this with an exact horizontal sweep. The
    /// default preserves point-only providers with union-band endpoint/midpoint
    /// checks, including a short body lifting between disjoint body bands.
    fn obstacle_segment_blocked(
        &self,
        from: glam::Vec3,
        to: glam::Vec3,
        radius: f32,
        height: f32,
    ) -> bool {
        let feet = from.y.min(to.y);
        let sweep_height = height + (to.y - from.y).abs();
        [from, from.lerp(to, 0.5), to].into_iter().any(|point| {
            self.obstacle_blocked(
                glam::Vec3::new(point.x, feet, point.z),
                radius,
                sweep_height,
            )
        })
    }

    /// True when the door is fully open and passable.
    fn door_open(&self, door_id: &str) -> bool;
    /// True while a leaf is physically prevented from reaching its open end.
    fn door_obstructed(&self, _door_id: &str) -> bool {
        false
    }
    /// `Some(true)` when the door refuses to open.
    fn door_locked(&self, door_id: &str) -> Option<bool>;
    /// The live leaf collider, for re-checking cells a swinging leaf covers.
    fn door_leaf(&self, door_id: &str) -> Option<crate::engine::collision::DoorCollider>;
}

impl NavDoorState for Doors {
    fn door_obstructed(&self, door_id: &str) -> bool {
        self.index_of(door_id)
            .and_then(|index| self.get(index))
            .is_some_and(|door| door.obstructed && door.phase() != DoorPhase::Open)
    }

    fn door_open(&self, door_id: &str) -> bool {
        self.index_of(door_id)
            .and_then(|index| self.get(index))
            .is_some_and(|door| door.phase() == DoorPhase::Open)
    }

    fn door_locked(&self, door_id: &str) -> Option<bool> {
        self.is_locked(door_id)
    }

    fn door_leaf(&self, door_id: &str) -> Option<crate::engine::collision::DoorCollider> {
        self.index_of(door_id)
            .and_then(|index| self.get(index))
            .map(crate::engine::door::DoorRuntime::collider)
    }
}

/// No doors at all: used by tests and by a world with no authored doors.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoDoors;

impl NavDoorState for NoDoors {
    fn door_open(&self, _door_id: &str) -> bool {
        true
    }

    fn door_locked(&self, _door_id: &str) -> Option<bool> {
        Some(false)
    }

    fn door_leaf(&self, _door_id: &str) -> Option<crate::engine::collision::DoorCollider> {
        None
    }
}
