//! Doors: map-authored leaves with a shared state machine, collision and
//! action surface.
//!
//! A door is the one movable solid in the level. The map authors one
//! [`DoorDef`](crate::engine::level::DoorDef); this module owns the run-time half:
//!
//! * [`DoorRuntime`] — the leaf's phase (`closed`, `opening`, `open`,
//!   `closing`), its current angle, its obstruction behaviour and the
//!   [`DoorCollider`](crate::engine::collision::DoorCollider) derived from the pose;
//! * [`Doors`] — every leaf of one level plus the id→index map that action and
//!   interaction dispatch resolve against. Targets are resolved by name once
//!   and the map is only rebuilt when the level changes, so no per-frame string
//!   search exists anywhere.
//!
//! Both the drawn slab and the collider read [`DoorRuntime::angle`] in the same
//! frame, so what the player sees and what stops them can never disagree.
//!
//! The visual build lives in [`crate::engine::render`] and the map wiring in
//! [`crate::engine::level`]: this module deliberately knows nothing about meshes,
//! textures or materials.

use std::collections::HashMap;

use crate::engine::collision::DoorCollider;
use crate::engine::level::{DoorDef, DoorObstruction, DoorStartState, LevelDef, LevelSurfaces};

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
    /// Chosen open endpoint. Kept through an interrupted close/reopen so the
    /// leaf never changes swing halfway through the occupant's body.
    open_angle: f32,
    phase: DoorPhase,
    /// True while the last advance was blocked.
    pub obstructed: bool,
    /// True while the leaf refuses to open.
    locked: bool,
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
            open_angle: def.signed_swing(),
            phase,
            obstructed: false,
            locked: def.locked,
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

    /// True while the leaf refuses to open.
    #[must_use]
    pub const fn is_locked(&self) -> bool {
        self.locked
    }

    /// Locks or unlocks the leaf. Returns whether the state changed.
    pub const fn set_locked(&mut self, locked: bool) -> bool {
        if self.locked == locked {
            return false;
        }
        self.locked = locked;
        true
    }

    /// Requests the open end.
    ///
    /// A locked leaf refuses the request and stays where it is; the caller
    /// reports the refusal once instead of silently ignoring it.
    pub const fn request_open(&mut self) -> bool {
        if self.locked || matches!(self.phase, DoorPhase::Opening | DoorPhase::Open) {
            return false;
        }
        self.phase = DoorPhase::Opening;
        true
    }

    /// Opens away from a world-space occupant for a bidirectional leaf. Only
    /// a fully closed leaf chooses a new side; an interrupted swing retains it.
    pub fn request_open_from(&mut self, origin: Option<glam::Vec3>) -> bool {
        if self.locked || matches!(self.phase, DoorPhase::Opening | DoorPhase::Open) {
            return false;
        }
        if self.phase == DoorPhase::Closed {
            self.choose_open_side(origin);
        }
        self.request_open()
    }

    fn choose_open_side(&mut self, origin: Option<glam::Vec3>) {
        self.open_angle = self.def.signed_swing();
        let Some(occupant) = origin.filter(|point| point.is_finite() && self.def.bidirectional)
        else {
            return;
        };
        let (dx, dz) = self.def.closed_direction();
        let side = (occupant.x - self.def.x).mul_add(-dz, (occupant.z - self.def.z) * dx);
        if side.abs() > 1.0e-4 {
            // Positive yaw moves the latch toward the negative closed normal.
            self.open_angle = self.def.swing_degrees.abs().copysign(side);
        }
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
    ///
    /// A locked leaf refuses a toggle that would open it; closing an open
    /// locked leaf is allowed, so a lock can never wedge a leaf mid-opening.
    pub const fn toggle(&mut self) -> bool {
        if self.locked && matches!(self.phase, DoorPhase::Closed | DoorPhase::Closing) {
            return false;
        }
        self.phase = self.phase.toggled();
        true
    }

    /// Toggle using the occupant's side when starting a new opening.
    pub fn toggle_from(&mut self, origin: Option<glam::Vec3>) -> bool {
        if matches!(self.phase, DoorPhase::Closed | DoorPhase::Closing) {
            self.request_open_from(origin)
        } else {
            self.request_close()
        }
    }

    /// Forces the phase to an end state immediately (a reset).
    pub fn reset(&mut self) {
        let (angle, phase) = match self.def.initial_state {
            DoorStartState::Closed => (0.0, DoorPhase::Closed),
            DoorStartState::Open => (self.def.signed_swing(), DoorPhase::Open),
        };
        self.angle = angle;
        self.open_angle = self.def.signed_swing();
        self.phase = phase;
        self.obstructed = false;
        self.locked = self.def.locked;
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
    pub fn advance(
        &mut self,
        delta: f32,
        mut blocked: impl FnMut(&DoorCollider) -> bool,
    ) -> DoorStep {
        let mut step = DoorStep::default();
        if !delta.is_finite() || delta <= 0.0 || !self.phase.is_moving() {
            return step;
        }
        self.reverse_guard = (self.reverse_guard - delta).max(0.0);
        let target = match self.phase {
            DoorPhase::Opening => self.open_angle,
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
        let intended = f32::mul_add(direction, sprint.min(remaining.abs()), self.angle);
        // A fast leaf must not jump over an actor or a small physical item.
        // Limit latch travel to 2.5 cm per probe, with a fixed work bound. A
        // pathological speed consumes less motion instead of skipping probes.
        let angle_step = (0.025 / (self.def.width + self.def.thickness).max(0.025))
            .to_degrees()
            .min(2.0);
        for _ in 0_u16..256 {
            let increment = (intended - self.angle).abs().min(angle_step);
            let candidate = direction.mul_add(increment, self.angle);
            if blocked(&self.collider_at(candidate)) {
                self.obstructed = true;
                step.obstructed = true;
                if self.def.obstruction == DoorObstruction::Reverse && self.reverse_guard <= 0.0 {
                    self.phase = self.phase.toggled();
                    self.reverse_guard = REVERSE_GUARD_SECONDS;
                }
                return step;
            }
            self.angle = candidate;
            step.moved = true;
            self.obstructed = false;
            if (intended - candidate).abs() <= STEP_EPS_DEGREES {
                break;
            }
        }
        let reached = (target - self.angle).abs() <= STEP_EPS_DEGREES;
        if reached {
            self.angle = target;
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
    /// Bumped whenever a leaf's phase or lock changes, so navigation can
    /// invalidate a cached route without comparing every door.
    version: u64,
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
        Self {
            version: 0,
            runtimes,
            by_id,
        }
    }

    /// The door-state version, bumped whenever a phase or lock changes.
    #[must_use]
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// A compound mechanism changed clearance in the shared live-leaf set.
    pub const fn notify_clearance_changed(&mut self) {
        self.bump();
    }

    /// Bumps the version (any observable state change).
    const fn bump(&mut self) {
        self.version = self.version.wrapping_add(1);
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
        self.request_open_from(id, None)
    }

    /// Opens a named leaf away from an eligible occupant, when configured.
    pub fn request_open_from(&mut self, id: &str, origin: Option<glam::Vec3>) -> bool {
        let changed = self
            .index_of(id)
            .and_then(|index| self.runtimes.get_mut(index))
            .is_some_and(|door| door.request_open_from(origin));
        if changed {
            self.bump();
        }
        changed
    }

    /// Requests the closed end of the door called `id`.
    pub fn request_close(&mut self, id: &str) -> bool {
        let changed = self
            .index_of(id)
            .and_then(|index| self.runtimes.get_mut(index))
            .is_some_and(DoorRuntime::request_close);
        if changed {
            self.bump();
        }
        changed
    }

    /// Whether the door called `id` is locked, if it exists.
    #[must_use]
    pub fn is_locked(&self, id: &str) -> Option<bool> {
        self.index_of(id)
            .and_then(|index| self.runtimes.get(index))
            .map(DoorRuntime::is_locked)
    }

    /// Locks or unlocks the door called `id`. Returns whether the state
    /// changed; an unknown id returns false.
    pub fn set_locked(&mut self, id: &str, locked: bool) -> bool {
        let changed = self
            .index_of(id)
            .and_then(|index| self.runtimes.get_mut(index))
            .is_some_and(|door| door.set_locked(locked));
        if changed {
            self.bump();
        }
        changed
    }

    /// Flips the door called `id` between its ends. Returns whether a phase
    /// change was requested.
    pub fn toggle(&mut self, id: &str) -> bool {
        self.toggle_from(id, None)
    }

    /// Toggle a named leaf, selecting the occupant's side from closed.
    pub fn toggle_from(&mut self, id: &str, origin: Option<glam::Vec3>) -> bool {
        let changed = self
            .index_of(id)
            .and_then(|index| self.runtimes.get_mut(index))
            .is_some_and(|door| door.toggle_from(origin));
        if changed {
            self.bump();
        }
        changed
    }

    /// Resets every leaf to its authored start state (a `reset_to_start`).
    pub fn reset(&mut self) {
        for door in &mut self.runtimes {
            door.reset();
        }
        self.bump();
    }

    /// Advances every moving leaf, returning how many moved this step.
    pub fn advance(
        &mut self,
        delta: f32,
        mut blocked: impl FnMut(usize, &DoorCollider) -> bool,
    ) -> usize {
        let mut moved = 0usize;
        let mut ended = false;
        for (index, door) in self.runtimes.iter_mut().enumerate() {
            let before = door.phase();
            let step = door.advance(delta, |candidate| blocked(index, candidate));
            if step.moved {
                moved = moved.saturating_add(1);
            }
            if door.phase() != before {
                ended = true;
            }
        }
        if ended {
            self.bump();
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
