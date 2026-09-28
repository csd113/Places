//! Runtime navigation: baked-mesh loading, nearest-point queries, path
//! queries and the door links that make routes react to live world state.
//!
//! The split is deliberate and enforced by the package boundary:
//!
//! * [`bake`] derives walkable space from real geometry. It runs **only** in
//!   the offline compiler; no player path calls it.
//! * [`query`] (`NavMesh`) is the runtime half: it validates and loads a baked
//!   [`NavGrid`](crate::package::navigation::NavGrid), answers nearest-point
//!   and path queries against the class that matches the agent's body, and
//!   consults [`NavDoorState`] so a closed or locked door blocks a route
//!   without touching the mesh.
//!
//! A query never mutates the mesh; the caller owns its own [`query::NavScratch`]
//! so pathfinding reuses allocations across frames and agents.

pub mod bake;
pub mod query;

#[cfg(test)]
mod tests;

pub use bake::{
    CELL_SLOPED, DEFAULT_NAV_CELL_M, HEADROOM_UNBOUNDED_CM, NAV_BAKE_MARGIN_M, NAV_MAX_SLOPE,
    NAV_MOVE_SUBSTEP_M, NavBakeInput, NavBakeOptions, NavBakeReport, NavWalkProxy, bake,
    neighbour_rule, reference_class,
};
pub use query::{NavMesh, NavPoint, NavScratch, Path, PathQuery, PathResult};

use crate::door::{DoorPhase, Doors};
use crate::package::navigation::NavClass;

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
    pub fn reference() -> Self {
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
    /// True when the door is fully open and passable.
    fn door_open(&self, door_id: &str) -> bool;
    /// `Some(true)` when the door refuses to open.
    fn door_locked(&self, door_id: &str) -> Option<bool>;
    /// The live leaf collider, for re-checking cells a swinging leaf covers.
    fn door_leaf(&self, door_id: &str) -> Option<crate::collision::DoorCollider>;
}

impl NavDoorState for Doors {
    fn door_open(&self, door_id: &str) -> bool {
        self.index_of(door_id)
            .and_then(|index| self.get(index))
            .is_some_and(|door| door.phase() == DoorPhase::Open)
    }

    fn door_locked(&self, door_id: &str) -> Option<bool> {
        self.is_locked(door_id)
    }

    fn door_leaf(&self, door_id: &str) -> Option<crate::collision::DoorCollider> {
        self.index_of(door_id)
            .and_then(|index| self.get(index))
            .map(crate::door::DoorRuntime::collider)
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

    fn door_leaf(&self, _door_id: &str) -> Option<crate::collision::DoorCollider> {
        None
    }
}
