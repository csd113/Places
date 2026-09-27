//! Doors: map-authored leaves with a shared state machine, collision and
//! action surface.
//!
//! A door is the one movable solid in the level. The map authors one
//! [`DoorDef`](crate::level::DoorDef); this module owns the run-time half:
//!
//! * [`DoorRuntime`] — the leaf's phase (`closed`, `opening`, `open`,
//!   `closing`), its current angle, its obstruction behaviour and the
//!   [`DoorCollider`](crate::collision::DoorCollider) derived from the pose;
//! * [`Doors`] — every leaf of one level plus the id→index map that action and
//!   interaction dispatch resolve against. Targets are resolved by name once
//!   and the map is only rebuilt when the level changes, so no per-frame string
//!   search exists anywhere.
//!
//! Both the drawn slab and the collider read [`DoorRuntime::angle`] in the same
//! frame, so what the player sees and what stops them can never disagree.
//!
//! The visual build lives in [`crate::render`] and the map wiring in
//! [`crate::level`]: this module deliberately knows nothing about meshes,
//! textures or materials.

use std::collections::HashMap;

use crate::collision::DoorCollider;
use crate::level::{DoorDef, DoorObstruction, DoorStartState, LevelDef, LevelSurfaces};

/// The phase of a door leaf.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DoorPhase {
    /// At the closed end, at rest.
    #[default]
    Closed,
    /// Moving toward the open end.
    Opening,
    /// At the open end, at rest.
    Open,
    /// Moving toward the closed end.
    Closing,
}

impl DoorPhase {
    /// The phase a `toggle` moves this phase toward.
    #[must_use]
    pub const fn toggled(self) -> Self {
        match self {
            Self::Closed | Self::Closing => Self::Opening,
            Self::Open | Self::Opening => Self::Closing,
        }
    }

    /// Stable lowercase name, for logs and tests.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Closed => "closed",
            Self::Opening => "opening",
            Self::Open => "open",
            Self::Closing => "closing",
        }
    }

    /// True while the leaf is between its ends.
    #[must_use]
    pub const fn is_moving(self) -> bool {
        matches!(self, Self::Opening | Self::Closing)
    }
}

/// How one advance step changed a door.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DoorStep {
    /// The angle changed this step.
    pub moved: bool,
    /// The sweep was blocked by the player or by solid geometry.
    pub obstructed: bool,
    /// The leaf reached one of its ends this step.
    pub settled: bool,
}

/// One door's run-time state.
#[derive(Debug, Clone, PartialEq)]
pub struct DoorRuntime {
    /// The authored definition, cloned at load so the runtime owns its pose.
    pub def: DoorDef,
    /// World Y of the leaf's bottom (walkable floor under the hinge plus `y`),
    /// resolved once at load.
    base_y: f32,
    /// Current angle from the closed pose, in degrees; signed like the swing.
    angle: f32,
    phase: DoorPhase,
    /// True while the last advance was blocked.
    pub obstructed: bool,
    /// Seconds before a `reverse` obstruction may flip direction again, so a
    /// leaf oscillating against a body cannot chatter every frame.
    reverse_guard: f32,
}

impl DoorRuntime {
    /// Builds the run-time state for one authored door.
    #[must_use]
    pub fn new(def: &DoorDef, level: &LevelDef) -> Self {
        let base_y = LevelSurfaces::new(level)
            .floor_y_at(def.x, def.z)
            .unwrap_or(0.0)
            + def.y;
        let (angle, phase) = match def.initial_state {
            DoorStartState::Closed => (0.0, DoorPhase::Closed),
            DoorStartState::Open => (def.signed_swing(), DoorPhase::Open),
        };
        Self {
            def: def.clone(),
            base_y,
            angle,
            phase,
            obstructed: false,
            reverse_guard: 0.0,
        }
    }

    /// World Y of the leaf's bottom.
    #[must_use]
    pub const fn base_y(&self) -> f32 {
        self.base_y
    }

    /// Current angle from the closed pose, in degrees.
    #[must_use]
    pub const fn angle(&self) -> f32 {
        self.angle
    }

    /// The current phase.
    #[must_use]
    pub const fn phase(&self) -> DoorPhase {
        self.phase
    }

    /// True while the leaf is between its ends.
    #[must_use]
    pub const fn is_moving(&self) -> bool {
        self.phase.is_moving()
    }

    /// The leaf's collider at its current angle.
    #[must_use]
    pub fn collider(&self) -> DoorCollider {
        self.collider_at(self.angle)
    }

    /// The collider the leaf would have at `angle`.
    #[must_use]
    fn collider_at(&self, angle: f32) -> DoorCollider {
        let direction = self.def.direction_at(angle);
        DoorCollider::from_pose(
            [self.def.x, self.base_y, self.def.z],
            direction.into(),
            self.def.width,
            self.def.thickness,
            self.def.height,
        )
    }

    /// Requests the open end.
    pub const fn request_open(&mut self) -> bool {
        if matches!(self.phase, DoorPhase::Opening | DoorPhase::Open) {
            return false;
        }
        self.phase = DoorPhase::Opening;
        true
    }

    /// Requests the closed end.
    pub const fn request_close(&mut self) -> bool {
        if matches!(self.phase, DoorPhase::Closing | DoorPhase::Closed) {
            return false;
        }
        self.phase = DoorPhase::Closing;
        true
    }

    /// Flips between the two ends, mid-travel included.
    pub const fn toggle(&mut self) -> bool {
        self.phase = self.phase.toggled();
        true
    }

    /// Forces the phase to an end state immediately (a reset).
    pub fn reset(&mut self) {
        let (angle, phase) = match self.def.initial_state {
            DoorStartState::Closed => (0.0, DoorPhase::Closed),
            DoorStartState::Open => (self.def.signed_swing(), DoorPhase::Open),
        };
        self.angle = angle;
        self.phase = phase;
        self.obstructed = false;
        self.reverse_guard = 0.0;
    }

    /// Advances the leaf by `delta`, using `blocked` to test each candidate
    /// pose against the world.
    ///
    /// `blocked` is called only while the leaf is moving and only for the pose
    /// it would take this step, so a resting door costs nothing. A blocked step
    /// either holds (`stop`) or flips direction once-per-obstruction
    /// (`reverse`); the collider is never advanced into the obstruction, so the
    /// player is never pushed or trapped by a closing leaf.
    pub fn advance(&mut self, delta: f32, mut blocked: impl FnMut(&DoorCollider) -> bool) -> DoorStep {
        let mut step = DoorStep::default();
        if !delta.is_finite() || delta <= 0.0 || !self.phase.is_moving() {
            return step;
        }
        self.reverse_guard = (self.reverse_guard - delta).max(0.0);
        let target = match self.phase {
            DoorPhase::Opening => self.def.signed_swing(),
            DoorPhase::Closing | DoorPhase::Closed | DoorPhase::Open => 0.0,
        };
        let speed = match self.phase {
            DoorPhase::Opening => self.def.open_speed_degrees,
            DoorPhase::Closing | DoorPhase::Closed | DoorPhase::Open => self.def.close_speed(),
        };
        let sprint = speed.max(0.0) * delta;
        if sprint <= 0.0 {
            return step;
        }
        let remaining = target - self.angle;
        let direction = if remaining >= 0.0 { 1.0 } else { -1.0 };
        let candidate = self.angle + direction * sprint.min(remaining.abs());
        if blocked(&self.collider_at(candidate)) {
            self.obstructed = true;
            step.obstructed = true;
            if self.def.obstruction == DoorObstruction::Reverse && self.reverse_guard <= 0.0 {
                self.phase = self.phase.toggled();
                self.reverse_guard = REVERSE_GUARD_SECONDS;
            }
            return step;
        }
        self.obstructed = false;
        step.moved = true;
        let reached = (target - candidate).abs() <= STEP_EPS_DEGREES;
        self.angle = if reached { target } else { candidate };
        if reached {
            self.phase = match self.phase {
                DoorPhase::Opening => DoorPhase::Open,
                DoorPhase::Closing | DoorPhase::Closed | DoorPhase::Open => DoorPhase::Closed,
            };
            step.settled = true;
        }
        step
    }
}

/// Angle tolerance within which a moving leaf is considered at its end, in
/// degrees.
const STEP_EPS_DEGREES: f32 = 1.0e-3;

/// Minimum seconds between two `reverse` flips of one leaf.
const REVERSE_GUARD_SECONDS: f32 = 0.4;

/// Every door of one level, plus the id→index map used by dispatch.
///
/// The map is built once at level load; action targets and interaction targets
/// resolve in `O(1)` and a missing id is a validation error, never a per-frame
/// search.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Doors {
    runtimes: Vec<DoorRuntime>,
    by_id: HashMap<String, usize>,
}

impl Doors {
    /// Builds the door set for a level.
    #[must_use]
    pub fn from_level(level: &LevelDef) -> Self {
        let runtimes: Vec<DoorRuntime> = level
            .doors
            .iter()
            .map(|def| DoorRuntime::new(def, level))
            .collect();
        let by_id = runtimes
            .iter()
            .enumerate()
            .map(|(index, door)| (door.def.id.clone(), index))
            .collect();
        Self { runtimes, by_id }
    }

    /// Every door, in authored order.
    pub fn iter(&self) -> std::slice::Iter<'_, DoorRuntime> {
        self.runtimes.iter()
    }

    /// Number of doors.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.runtimes.len()
    }

    /// True when the level has no doors.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.runtimes.is_empty()
    }

    /// One door by authored index.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<&DoorRuntime> {
        self.runtimes.get(index)
    }

    /// One door by authored index, mutably.
    #[must_use]
    pub fn get_mut(&mut self, index: usize) -> Option<&mut DoorRuntime> {
        self.runtimes.get_mut(index)
    }

    /// The index of the door called `id`, if any.
    #[must_use]
    pub fn index_of(&self, id: &str) -> Option<usize> {
        self.by_id.get(id.trim()).copied()
    }

    /// True while any leaf is moving.
    #[must_use]
    pub fn any_moving(&self) -> bool {
        self.runtimes.iter().any(DoorRuntime::is_moving)
    }

    /// Requests the open end of the door called `id`.
    ///
    /// Returns whether the request changed the phase; an unknown id returns
    /// `false` exactly like an already-open door, so callers that need the
    /// distinction use [`Self::index_of`].
    pub fn request_open(&mut self, id: &str) -> bool {
        self.index_of(id)
            .and_then(|index| self.runtimes.get_mut(index))
            .is_some_and(DoorRuntime::request_open)
    }

    /// Requests the closed end of the door called `id`.
    pub fn request_close(&mut self, id: &str) -> bool {
        self.index_of(id)
            .and_then(|index| self.runtimes.get_mut(index))
            .is_some_and(DoorRuntime::request_close)
    }

    /// Resets every leaf to its authored start state (a `reset_to_start`).
    pub fn reset(&mut self) {
        for door in &mut self.runtimes {
            door.reset();
        }
    }

    /// Advances every moving leaf, returning how many moved this step.
    pub fn advance(
        &mut self,
        delta: f32,
        mut blocked: impl FnMut(usize, &DoorCollider) -> bool,
    ) -> usize {
        let mut moved = 0usize;
        for (index, door) in self.runtimes.iter_mut().enumerate() {
            let step = door.advance(delta, |candidate| blocked(index, candidate));
            if step.moved {
                moved = moved.saturating_add(1);
            }
        }
        moved
    }
}

impl<'a> IntoIterator for &'a Doors {
    type Item = &'a DoorRuntime;
    type IntoIter = std::slice::Iter<'a, DoorRuntime>;

    fn into_iter(self) -> Self::IntoIter {
        self.runtimes.iter()
    }
}

impl Doors {
    /// The current collider of every leaf, in authored order.
    #[must_use]
    pub fn colliders(&self) -> Vec<DoorCollider> {
        self.runtimes.iter().map(DoorRuntime::collider).collect()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::indexing_slicing, clippy::float_cmp)]

    use super::*;
    use crate::level::DoorSwing;

    fn level_with_doors(json: &str) -> LevelDef {
        LevelDef::from_json(json).expect("test level parses")
    }

    fn door_json(extra: &str) -> String {
        format!(
            r#"{{
                "format_version": 2,
                "id": "door_test",
                "name": "Door Test",
                "spawn": {{ "x": 2.0, "z": 5.0 }},
                "rooms": [ {{ "x": 0.0, "z": 0.0, "width": 6.0, "depth": 6.0, "height": 2.7 }} ],
                "doors": [ {{
                    "id": "test_door", "x": 1.0, "z": 3.0,
                    "width": 1.0, "height": 2.1, "thickness": 0.05,
                    "rotation_degrees": 0.0{extra}
                }} ]
            }}"#
        )
    }

    #[test]
    fn a_left_swing_opens_toward_negative_z() {
        let level = level_with_doors(&door_json(r#", "open_direction": "left""#));
        let mut doors = Doors::from_level(&level);
        assert_eq!(doors.len(), 1);
        assert_eq!(doors.index_of("test_door"), Some(0));
        assert_eq!(doors.get(0).expect("door").phase(), DoorPhase::Closed);
        doors.get_mut(0).expect("door").request_open();
        let moved = doors.advance(1.0, |_, _| false);
        assert_eq!(moved, 1);
        let angle = doors.get(0).expect("door").angle();
        assert!(angle > 0.0, "left swing is positive: {angle}");
        // 120 deg/s for one second overshoots 90 and settles exactly on it.
        assert!((angle - 90.0).abs() < 1e-3, "{angle}");
        assert_eq!(doors.get(0).expect("door").phase(), DoorPhase::Open);
    }

    #[test]
    fn a_right_swing_opens_negative() {
        let level = level_with_doors(&door_json(r#", "open_direction": "right""#));
        let mut doors = Doors::from_level(&level);
        doors.get_mut(0).expect("door").request_open();
        doors.advance(1.0, |_, _| false);
        assert!(doors.get(0).expect("door").angle() < 0.0);
        assert_eq!(doors.get(0).expect("door").phase(), DoorPhase::Open);
    }

    #[test]
    fn an_initially_open_door_starts_at_its_swing() {
        let level = level_with_doors(&door_json(r#", "open_direction": "left", "initial_state": "open""#));
        let doors = Doors::from_level(&level);
        assert_eq!(doors.get(0).expect("door").phase(), DoorPhase::Open);
        assert!((doors.get(0).expect("door").angle() - 90.0).abs() < 1e-3);
        assert!(doors.get(0).expect("door").collider().width > 0.0);
    }

    #[test]
    fn a_blocked_leaf_holds_and_resumes_when_clear() {
        let level = level_with_doors(&door_json(r#", "open_direction": "left", "obstruction": "stop""#));
        let mut doors = Doors::from_level(&level);
        doors.get_mut(0).expect("door").request_open();
        // Block every step: the angle must not advance.
        doors.advance(0.1, |_, _| true);
        assert_eq!(doors.get(0).expect("door").angle(), 0.0);
        assert!(doors.get(0).expect("door").obstructed);
        assert!(doors.get(0).expect("door").is_moving());
        // Cleared: the same request resumes.
        doors.advance(0.1, |_, _| false);
        assert!(doors.get(0).expect("door").angle() > 0.0);
        assert!(!doors.get(0).expect("door").obstructed);
    }

    #[test]
    fn a_reversing_leaf_flips_direction_once_per_obstruction() {
        let level = level_with_doors(&door_json(r#", "open_direction": "left", "obstruction": "reverse""#));
        let mut doors = Doors::from_level(&level);
        doors.get_mut(0).expect("door").request_open();
        doors.advance(0.25, |_, _| true);
        assert_eq!(doors.get(0).expect("door").phase(), DoorPhase::Closing);
        // The guard holds the flip during the next blocked step.
        doors.advance(0.1, |_, _| true);
        assert_eq!(doors.get(0).expect("door").phase(), DoorPhase::Closing);
        // Past the guard, a still-blocked close flips back to opening.
        doors.advance(0.5, |_, _| true);
        assert_eq!(doors.get(0).expect("door").phase(), DoorPhase::Opening);
    }

    #[test]
    fn a_toggle_from_mid_travel_reverses_without_snapping() {
        let level = level_with_doors(&door_json(r#", "open_direction": "left""#));
        let mut doors = Doors::from_level(&level);
        doors.get_mut(0).expect("door").request_open();
        doors.advance(0.25, |_, _| false);
        let mid = doors.get(0).expect("door").angle();
        assert!(mid > 0.0 && mid < 90.0);
        doors.get_mut(0).expect("door").toggle();
        assert_eq!(doors.get(0).expect("door").phase(), DoorPhase::Closing);
        doors.advance(0.1, |_, _| false);
        let closer = doors.get(0).expect("door").angle();
        assert!(closer < mid, "closing from {mid} to {closer}");
    }

    #[test]
    fn a_collider_follows_the_current_angle() {
        let level = level_with_doors(&door_json(r#", "open_direction": "left""#));
        let mut doors = Doors::from_level(&level);
        let closed = doors.get(0).expect("door").collider();
        assert!(closed.contains_point(1.5, 1.0, 3.0));
        doors.get_mut(0).expect("door").request_open();
        doors.advance(1.0, |_, _| false);
        let open = doors.get(0).expect("door").collider();
        // Open 90 degrees: the slab now runs along -Z from the hinge at (1, 3).
        assert!(open.contains_point(1.0, 1.0, 2.0), "{open:?}");
        assert!(!open.contains_point(1.5, 1.0, 3.0));
    }

    #[test]
    fn the_swing_direction_is_signed_by_the_authored_swing() {
        let level = level_with_doors(&door_json(r#", "open_direction": "left""#));
        let door = doors_first(&level);
        assert_eq!(door.def.open_direction, DoorSwing::Left);
        assert!(door.def.signed_swing() > 0.0);
    }

    fn doors_first(level: &LevelDef) -> DoorRuntime {
        DoorRuntime::new(&level.doors[0], level)
    }
}
