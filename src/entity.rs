//! Entity routes: map-authored movement and pose playback for placed
//! characters.
//!
//! A [`crate::level::EntityRouteDef`] names a placed prop instance and an
//! ordered list of steps. This module resolves those definitions against the
//! level once at load and advances each route's deterministic runtime state
//! against the same collision world the player uses, so an authored route
//! cannot walk a character through a wall, off a floor or through a step it
//! could not climb.
//!
//! ```text
//! level.routes[]                resolved once at load
//!       |
//!       v
//! EntityRoutes (definitions) -> RouteState (position/yaw/step/cue)
//!       |                              |
//!       |  advance(delta, walls/floor) |
//!       v                              v
//!   EntityFrame { instance_id, transform, PoseCue } per frame -> renderer
//! ```
//!
//! Identity is the placed-instance id, exactly like labels and interactions,
//! so two copies of one model run independent routes. A route never mutates
//! the collision world, and a blocked step stalls in place and reports once
//! instead of teleporting or tunnelling.
//!
//! [`PoseCue`] lives here (rather than in the renderer) so the gameplay-side
//! runtime and the renderer share one pose vocabulary without a dependency
//! cycle between `game` and `render`.

use glam::{Vec2, Vec3};

use crate::collision::{WallAabb, resolve_player_collision_for_body};
use crate::level::{LevelDef, LevelSurfaces, PROP_FALLBACK_SIZE, RouteStepDef, WalkableFloor};
use crate::logging;

/// One explicit pose request for an entity-driven character.
#[derive(Clone, Debug, PartialEq)]
pub enum PoseCue {
    /// Loop the rig's idle clip (or the first clip / procedural idle).
    Idle,
    /// Loop the walk or run clip, time-scaled so one gait cycle matches the
    /// route speed and the authored stride.
    Walk { speed_mps: f32 },
    /// Play a named clip; `once` holds the last pose and reports completion,
    /// `paused` freezes the current time.
    Clip {
        name: String,
        once: bool,
        paused: bool,
    },
    /// Ease a named clip's time toward a normalized position in `0..=1`
    /// instead of playing it. A lever, switch or drawer is authored as a clip
    /// whose first key is one rest position and whose last key is the other:
    /// re-targeting mid-travel reverses from the current pose with no snap and
    /// no restart at an endpoint.
    Scrub { name: String, target: f32 },
}

impl PoseCue {
    /// The clip name this cue plays or scrubs, when it names one.
    #[must_use]
    pub fn clip_name(&self) -> Option<&str> {
        match self {
            Self::Clip { name, .. } | Self::Scrub { name, .. } => Some(name),
            Self::Idle | Self::Walk { .. } => None,
        }
    }
}

/// Turning speed for route facing, in degrees per second.
pub const ENTITY_TURN_RATE_DEGREES_PER_SECOND: f32 = 240.0;

/// Distance at which a `move_to` step counts as arrived, in metres.
pub const ENTITY_ARRIVE_EPS_M: f32 = 0.02;

/// Facing tolerance at which a `face` step completes, in radians.
pub const ENTITY_FACE_EPS_RAD: f32 = 0.02;

/// Largest single simulation step, in seconds.
///
/// The route avoids burning an unbounded frame on many substeps after a hitch,
/// exactly as the player's vertical integrator does (the same 0.1 s clamp).
pub const ENTITY_MAX_STEP_S: f32 = 0.1;

/// Fixed movement substep, in seconds.
pub const ENTITY_SUBSTEP_S: f32 = 1.0 / 60.0;

/// Largest climb or drop one substep may follow without blocking, in metres.
pub const ENTITY_STEP_HEIGHT_M: f32 = 0.3;

/// Smallest movement-disc radius, in metres.
///
/// The route runtime and the level validator share this floor so the runtime
/// disc can never exceed the validated one: validation samples the path at
/// `max(footprint) * 0.5` clamped to this minimum, and the runtime uses
/// `min(footprint) * 0.5` clamped to the same minimum.
pub const ENTITY_MIN_RADIUS_M: f32 = 0.08;

/// Depenetration distance above which a wall counts as blocking, in metres.
pub const ENTITY_BLOCK_EPS_M: f32 = 1.0e-3;

/// One resolved route: the authored steps plus the placement facts the
/// simulation needs (spawn, footprint, body height).
#[derive(Clone, Debug, PartialEq)]
pub struct EntityRoute {
    /// Stable per-instance id of the placed prop this route drives.
    pub instance_id: String,
    /// Restart at step 0 after the last step completes.
    pub looped: bool,
    /// The authored steps, in order.
    pub steps: Vec<RouteStepDef>,
    /// World position of the entity's base at spawn (floor-resolved).
    pub spawn_position: Vec3,
    /// Authored spawn yaw, in degrees.
    pub spawn_yaw_degrees: f32,
    /// Collision disc radius in metres (the narrower footprint axis).
    pub radius: f32,
    /// Collision body height in metres.
    pub body_height: f32,
}

/// Every route a level declares, resolved against its placed props.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EntityRoutes {
    routes: Vec<EntityRoute>,
}

impl EntityRoutes {
    /// An empty set: no entity moves.
    #[must_use]
    pub const fn new() -> Self {
        Self { routes: Vec::new() }
    }

    /// Resolves every authored route against the level's props.
    ///
    /// A route naming an unknown instance, carrying no steps or non-finite
    /// data is skipped; a loaded level has already been validated, so this is
    /// defensive exactly as the trigger and interactable resolvers are.
    #[must_use]
    pub fn from_level(level: &LevelDef) -> Self {
        let surfaces = LevelSurfaces::new(level);
        let ids = level.prop_instance_ids();
        let mut routes = Vec::new();
        for def in &level.routes {
            let id = def.id.trim();
            if id.is_empty() || def.steps.is_empty() {
                continue;
            }
            let Some(prop_index) = ids.iter().position(|candidate| candidate == id) else {
                continue;
            };
            let Some(prop) = level.props.get(prop_index) else {
                continue;
            };
            if !prop.x.is_finite()
                || !prop.y.is_finite()
                || !prop.z.is_finite()
                || !prop.rotation_degrees.is_finite()
            {
                continue;
            }
            let size = prop.resolved_size(PROP_FALLBACK_SIZE);
            if !size.iter().all(|value| value.is_finite() && *value > 0.0) {
                continue;
            }
            let base_y = surfaces.floor_y_at(prop.x, prop.z).unwrap_or(0.0) + prop.y;
            // The movement disc uses the narrower footprint axis: an elongated
            // body (a rat, a walking skeleton) must still fit doorways. Route
            // validation checks the whole authored path against the wider
            // extent clamped to the same minimum, so the runtime disc never
            // allows a wall overlap the map did not already clear.
            let radius = (size[0].min(size[2]) * 0.5).clamp(ENTITY_MIN_RADIUS_M, 0.5);
            routes.push(EntityRoute {
                instance_id: id.to_string(),
                looped: def.looped,
                steps: def.steps.clone(),
                spawn_position: Vec3::new(prop.x, base_y, prop.z),
                spawn_yaw_degrees: prop.rotation_degrees,
                radius,
                body_height: size[1].clamp(0.1, 2.0),
            });
        }
        Self { routes }
    }

    /// True when no route exists.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.routes.is_empty()
    }

    /// Number of routes.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.routes.len()
    }

    /// Every route, in authored order.
    #[must_use]
    pub fn routes(&self) -> &[EntityRoute] {
        &self.routes
    }

    /// The route driving `instance_id`, if any.
    #[must_use]
    pub fn get(&self, instance_id: &str) -> Option<&EntityRoute> {
        self.routes
            .iter()
            .find(|route| route.instance_id == instance_id)
    }

    /// Index of the route driving `instance_id`, if any.
    #[must_use]
    pub fn index_of(&self, instance_id: &str) -> Option<usize> {
        self.routes
            .iter()
            .position(|route| route.instance_id == instance_id)
    }
}

/// One route's deterministic runtime state.
#[derive(Clone, Debug, PartialEq)]
pub struct RouteState {
    /// Current base position (feet on the resolved walkable floor).
    pub position: Vec3,
    /// Current facing, in radians (`0` faces `+Z`).
    pub yaw: f32,
    /// Index of the active step; `steps.len()` once a one-shot route finished.
    pub step: usize,
    /// Seconds spent in the active step.
    pub step_time: f32,
    /// The pose the route's own steps last requested.
    pub cue: PoseCue,
    /// True while the active step cannot make progress (wall, void or a step
    /// taller than the entity can climb). The route stalls in place.
    pub blocked: bool,
    /// True once a non-looping route completed every step.
    pub finished: bool,
}

impl EntityRoute {
    /// Fresh state at the route's authored spawn.
    #[must_use]
    pub fn new_state(&self) -> RouteState {
        let mut state = RouteState {
            position: self.spawn_position,
            yaw: self.spawn_yaw_degrees.to_radians(),
            step: 0,
            step_time: 0.0,
            cue: PoseCue::Idle,
            blocked: false,
            finished: false,
        };
        self.enter_step(&mut state, 0);
        state
    }

    /// Advances the route by `delta_seconds` in fixed substeps.
    ///
    /// A non-finite or negative delta is ignored; a large delta is clamped to
    /// [`ENTITY_MAX_STEP_S`] so a frame hitch never skips the route across a
    /// wall or a floor edge.
    pub fn advance(&self, state: &mut RouteState, delta_seconds: f32, world: &RouteWorld<'_>) {
        let delta = if delta_seconds.is_finite() {
            delta_seconds.clamp(0.0, ENTITY_MAX_STEP_S)
        } else {
            0.0
        };
        if delta <= 0.0 {
            return;
        }
        // `delta <= 0.1 s` and the substep is `1/60 s`: at most six substeps.
        let substeps = f32::ceil(delta / ENTITY_SUBSTEP_S).max(1.0);
        let substep = delta / substeps;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let count = substeps as u32;
        for _ in 0..count {
            self.advance_substep(state, substep, world);
            if state.blocked || state.finished {
                break;
            }
        }
    }

    /// One fixed substep of the active step.
    #[allow(clippy::arithmetic_side_effects)] // bounded pose/movement arithmetic
    fn advance_substep(&self, state: &mut RouteState, delta: f32, world: &RouteWorld<'_>) {
        if state.finished {
            return;
        }
        let Some(step) = self.steps.get(state.step) else {
            self.finish(state);
            return;
        };
        state.step_time += delta;
        match step {
            RouteStepDef::MoveTo { x, z, speed } => {
                let here = Vec2::new(state.position.x, state.position.z);
                let delta_to_target = Vec2::new(*x, *z) - here;
                let distance = delta_to_target.length();
                if !distance.is_finite() {
                    self.block(state, "waypoint is not finite");
                    return;
                }
                if distance <= ENTITY_ARRIVE_EPS_M {
                    self.enter_step(state, state.step + 1);
                    return;
                }
                let direction = delta_to_target / distance;
                let target_yaw = direction.x.atan2(direction.y);
                state.yaw = turn_toward(
                    state.yaw,
                    target_yaw,
                    ENTITY_TURN_RATE_DEGREES_PER_SECOND.to_radians() * delta,
                );
                let travel = (speed * delta).min(distance);
                let candidate = here + direction * travel;
                let resolved = resolve_player_collision_for_body(
                    candidate,
                    self.radius,
                    state.position.y,
                    self.body_height,
                    world.walls,
                );
                if (resolved - candidate).length() > ENTITY_BLOCK_EPS_M {
                    self.block(state, "a wall blocks the path");
                    return;
                }
                let Some(floor_y) = world.floor.walk_height_at(resolved.x, resolved.y) else {
                    self.block(state, "the path leaves every walkable floor");
                    return;
                };
                if (floor_y - state.position.y).abs() > ENTITY_STEP_HEIGHT_M {
                    self.block(state, "the path steps further than the entity can climb");
                    return;
                }
                state.position = Vec3::new(resolved.x, floor_y, resolved.y);
                state.cue = PoseCue::Walk { speed_mps: *speed };
                state.blocked = false;
            }
            RouteStepDef::Face { yaw_degrees } => {
                let target = yaw_degrees.to_radians();
                state.yaw = turn_toward(
                    state.yaw,
                    target,
                    ENTITY_TURN_RATE_DEGREES_PER_SECOND.to_radians() * delta,
                );
                if angle_difference(state.yaw, target).abs() <= ENTITY_FACE_EPS_RAD {
                    state.yaw = target;
                    self.enter_step(state, state.step + 1);
                }
            }
            RouteStepDef::Wait { seconds } | RouteStepDef::Play { seconds, .. } => {
                state.blocked = false;
                if state.step_time >= *seconds {
                    self.enter_step(state, state.step + 1);
                }
            }
        }
    }

    /// Moves to a step, applying the step's initial pose, or finishes the
    /// route when the end is reached.
    fn enter_step(&self, state: &mut RouteState, index: usize) {
        if self.steps.is_empty() {
            self.finish(state);
            return;
        }
        if index >= self.steps.len() {
            if self.looped {
                state.step = 0;
            } else {
                self.finish(state);
                return;
            }
        } else {
            state.step = index;
        }
        state.step_time = 0.0;
        state.blocked = false;
        state.cue = match self.steps.get(state.step) {
            Some(RouteStepDef::Play { clip, looped, .. }) if !clip.trim().is_empty() => {
                PoseCue::Clip {
                    name: clip.trim().to_string(),
                    once: !looped,
                    paused: false,
                }
            }
            // A walk step starts walking immediately; the first substep then
            // keeps the same cue, so the transition never flashes an idle pose.
            Some(RouteStepDef::MoveTo { speed, .. }) => PoseCue::Walk { speed_mps: *speed },
            _ => PoseCue::Idle,
        };
    }

    /// Ends a non-looping route, holding its final clip pose.
    ///
    /// A finished route that ended on a `play` step keeps that clip (a seated
    /// pose stays seated); a route that ended walking or waiting settles into
    /// the idle cue rather than walking in place.
    fn finish(&self, state: &mut RouteState) {
        state.step = self.steps.len();
        state.step_time = 0.0;
        state.blocked = false;
        state.finished = true;
        if !matches!(state.cue, PoseCue::Clip { .. }) {
            state.cue = PoseCue::Idle;
        }
    }

    /// Marks the active step blocked and reports the reason once.
    fn block(&self, state: &mut RouteState, reason: &str) {
        state.blocked = true;
        // A stalled route stands still rather than walking in place.
        state.cue = PoseCue::Idle;
        logging::warn_once(
            format!("entity-route-blocked:{}", self.instance_id),
            format!(
                "[entities] route `{}` is blocked: {reason}; the entity stays put",
                self.instance_id
            ),
        );
    }
}

/// The collision world a route advances against.
pub struct RouteWorld<'a> {
    pub walls: &'a [WallAabb],
    pub floor: &'a WalkableFloor,
}

/// One entity's per-frame handoff to the renderer.
#[derive(Clone, Debug, PartialEq)]
pub struct EntityFrame {
    /// Placed-instance id the renderer's character is keyed by.
    pub instance_id: String,
    /// Live base position and yaw, when the route moved the entity.
    pub transform: Option<(Vec3, f32)>,
    /// The pose to play this frame (an interaction override wins over the
    /// route's own cue).
    pub cue: PoseCue,
}

/// Shortest signed angular difference `to - from`, in radians.
#[must_use]
pub fn angle_difference(from: f32, to: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    (to - from + PI).rem_euclid(TAU) - PI
}

/// Turns `current` toward `target` by at most `max_step` radians.
#[must_use]
pub fn turn_toward(current: f32, target: f32, max_step: f32) -> f32 {
    let difference = angle_difference(current, target);
    if difference.abs() <= max_step {
        target
    } else {
        max_step.mul_add(difference.signum(), current)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::indexing_slicing)]

    use super::*;
    use crate::level::LevelDef;

    fn level_with_route(route: &str) -> LevelDef {
        LevelDef::from_json(&format!(
            r#"{{
                "format_version": 1,
                "id": "entity_route_test",
                "name": "Entity Route Test",
                "spawn": {{ "x": 1.0, "z": 1.0 }},
                "room": {{ "x": 0.0, "z": 0.0, "width": 20.0, "depth": 20.0, "height": 4.0 }},
                "props": [
                    {{ "id": "runner", "model": "core:crate", "x": 2.0, "z": 2.0,
                       "size": [0.4, 0.4, 0.4] }}
                ],
                "routes": [{route}]
            }}"#
        ))
        .expect("the route test level parses")
    }

    fn world_routes(level: &LevelDef) -> (EntityRoutes, Vec<WallAabb>, WalkableFloor) {
        (
            EntityRoutes::from_level(level),
            level.collision_aabbs(),
            WalkableFloor::from_level(level),
        )
    }

    #[test]
    fn a_move_route_walks_to_the_waypoint_at_the_authored_speed() {
        let level = level_with_route(
            r#"{ "id": "runner", "steps": [
                { "step": "move_to", "x": 6.0, "z": 2.0, "speed": 1.0 }
            ] }"#,
        );
        let (routes, walls, floor) = world_routes(&level);
        let route = routes.get("runner").expect("route resolves");
        let mut state = route.new_state();
        assert!((state.position.x - 2.0).abs() < 1e-6);
        let world = RouteWorld {
            walls: &walls,
            floor: &floor,
        };
        // Four metres at 1 m/s: four seconds of 1/60 s steps.
        for _ in 0..240 {
            route.advance(&mut state, 1.0 / 60.0, &world);
        }
        assert!(!state.blocked, "the flat room floor is walkable");
        assert!(state.finished, "the one-shot route completes");
        assert!(
            (state.position.x - 6.0).abs() < 0.05,
            "{:?}",
            state.position
        );
        assert!(
            (state.position.z - 2.0).abs() < 0.05,
            "{:?}",
            state.position
        );
        assert!((state.position.y - 0.0).abs() < 1e-6);
    }

    #[test]
    fn a_fast_move_cannot_tunnel_through_a_wall_or_leave_the_floor() {
        let level = level_with_route(
            r#"{ "id": "runner", "steps": [
                { "step": "move_to", "x": 30.0, "z": 2.0, "speed": 6.0 }
            ] }"#,
        );
        let (routes, walls, floor) = world_routes(&level);
        let route = routes.get("runner").expect("route resolves");
        let mut state = route.new_state();
        let world = RouteWorld {
            walls: &walls,
            floor: &floor,
        };
        // Ten seconds of 0.1 s frames: far past the room's 20 m edge.
        for _ in 0..100 {
            route.advance(&mut state, 0.1, &world);
        }
        assert!(
            state.blocked,
            "the route stops when it leaves the room floor"
        );
        assert!(
            state.position.x <= 20.01,
            "the route never crosses into the void: {:?}",
            state.position
        );
        assert!(
            floor
                .walk_height_at(state.position.x, state.position.z)
                .is_some(),
            "a blocked route stays on a walkable floor"
        );
    }

    #[test]
    fn a_play_step_sets_its_clip_and_a_looping_route_restarts() {
        let level = level_with_route(
            r#"{ "id": "runner", "loop": true, "steps": [
                { "step": "play", "clip": "sit_idle", "seconds": 0.5, "loop": true },
                { "step": "wait", "seconds": 0.25 }
            ] }"#,
        );
        let (routes, walls, floor) = world_routes(&level);
        let route = routes.get("runner").expect("route resolves");
        let mut state = route.new_state();
        let world = RouteWorld {
            walls: &walls,
            floor: &floor,
        };
        route.advance(&mut state, 1.0 / 60.0, &world);
        assert_eq!(
            state.cue,
            PoseCue::Clip {
                name: "sit_idle".into(),
                once: false,
                paused: false
            }
        );
        assert_eq!(state.step, 0);
        // 0.75 s is one full loop plus a frame: back at step 0, playing.
        for _ in 0..46 {
            route.advance(&mut state, 1.0 / 60.0, &world);
        }
        assert_eq!(state.step, 0, "a looping route restarts");
        assert_eq!(
            state.cue,
            PoseCue::Clip {
                name: "sit_idle".into(),
                once: false,
                paused: false
            }
        );
    }

    #[test]
    fn a_face_step_turns_to_the_authored_yaw() {
        let level = level_with_route(
            r#"{ "id": "runner", "steps": [
                { "step": "face", "yaw_degrees": 180.0 },
                { "step": "wait", "seconds": 0.1 }
            ] }"#,
        );
        let (routes, walls, floor) = world_routes(&level);
        let route = routes.get("runner").expect("route resolves");
        let mut state = route.new_state();
        let world = RouteWorld {
            walls: &walls,
            floor: &floor,
        };
        // 180 degrees at 240 deg/s is 0.75 s; give it a full second.
        for _ in 0..60 {
            route.advance(&mut state, 1.0 / 60.0, &world);
        }
        assert!((state.yaw - std::f32::consts::PI).abs() < 1e-3);
        assert!(!state.blocked);
    }

    #[test]
    fn a_one_shot_route_holds_a_final_play_clip_but_settles_after_a_walk() {
        let level = level_with_route(
            r#"{ "id": "runner", "steps": [
                { "step": "play", "clip": "pose_sit_chair", "seconds": 0.25 }
            ] }"#,
        );
        let (routes, walls, floor) = world_routes(&level);
        let route = routes.get("runner").expect("route resolves");
        let mut state = route.new_state();
        let world = RouteWorld {
            walls: &walls,
            floor: &floor,
        };
        for _ in 0..30 {
            route.advance(&mut state, 1.0 / 60.0, &world);
        }
        assert!(state.finished);
        assert_eq!(
            state.cue,
            PoseCue::Clip {
                name: "pose_sit_chair".into(),
                once: true,
                paused: false
            },
            "a one-shot route holds its final pose"
        );

        // A route that ends walking settles to idle instead of walking in
        // place forever.
        let level = level_with_route(
            r#"{ "id": "runner", "steps": [
                { "step": "move_to", "x": 3.0, "z": 2.0, "speed": 1.0 }
            ] }"#,
        );
        let (routes, walls, floor) = world_routes(&level);
        let route = routes.get("runner").expect("route resolves");
        let mut state = route.new_state();
        let world = RouteWorld {
            walls: &walls,
            floor: &floor,
        };
        for _ in 0..120 {
            route.advance(&mut state, 1.0 / 60.0, &world);
        }
        assert!(state.finished);
        assert_eq!(state.cue, PoseCue::Idle);
    }

    #[test]
    fn angle_helpers_wrap_the_short_way() {
        use std::f32::consts::PI;
        assert!((angle_difference(0.0, PI / 2.0) - PI / 2.0).abs() < 1e-6);
        assert!((angle_difference(PI - 0.1, -PI + 0.1) - 0.2).abs() < 1e-5);
        // Crossing the +/-PI seam takes the short arc, not the long one.
        let turned = turn_toward(PI - 0.05, -PI + 0.05, 0.1);
        assert!((turned - (-PI + 0.05)).abs() < 1e-4, "{turned}");
        let partial = turn_toward(PI - 0.05, -PI + 0.05, 0.03);
        assert!((partial - (PI - 0.02)).abs() < 1e-4, "{partial}");
    }
}
