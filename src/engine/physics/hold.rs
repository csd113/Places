//! Collision-aware carrying and safe release, with no teleport-derived velocity.

use glam::{Quat, Vec3};
use rapier3d::parry::bounding_volume::BoundingVolume;
use rapier3d::parry::query::{
    NonlinearRigidMotion, ShapeCastHit, ShapeCastOptions, cast_shapes, cast_shapes_nonlinear,
    contact, distance, intersection_test,
};
use rapier3d::parry::shape::{Ball, Cuboid, Shape};
use rapier3d::prelude::{
    Collider, ColliderHandle, Pose, QueryFilter, QueryFilterFlags, Ray, RigidBodyType, SharedShape,
    Vector,
};

use super::collision::{
    add, door_pose, point, pose, quaternion, query_filter, refresh_collider, scaled, subtract,
    vector,
};
use super::world::{ItemWorld, PlayerContext, Runtime};
use super::{FIXED_STEP, HOLD_DISTANCE, PICKUP_REACH};
use crate::engine::collision::DoorCollider;

const HOLD_SPEED: f32 = 4.0;
const SWEEP_MARGIN: f32 = 0.006;
const CAMERA_RADIUS: f32 = 0.10;
const OBSTRUCTED_SECONDS: f32 = 0.35;
const AUTO_DROP_MARGIN: f32 = 0.5;

#[derive(Clone, Copy)]
pub enum HeldPlayerSlide {
    Full,
    Planar,
}

impl HeldPlayerSlide {
    fn contact_normal(self, raw_normal: Vector) -> Vec3 {
        let normal = point(raw_normal);
        match self {
            Self::Full => normal,
            Self::Planar => Vec3::new(normal.x, 0.0, normal.z).normalize_or_zero(),
        }
    }
}

pub struct HeldPlayerContact {
    pub eye_after_slide: Vec3,
    /// Command fraction at the last position the slide accepted.
    pub(crate) slide_fraction: f32,
    pub normal: Vec3,
    pub slide_normal: Option<Vec3>,
}

pub(super) struct HeldItem {
    pub key: u64,
    relative_rotation: Quat,
    route: Option<CarryRoute>,
    control_delta: f32,
    block_dynamic: bool,
    angular_velocity: Vec3,
    obstructed_seconds: f32,
    solver_anchor: Option<CarryAnchor>,
    // Controller-only poses are visible to stance and interaction before the
    // item clock runs. The solver resumes from the original published pose.
    command_anchor: Option<CommandCarryAnchor>,
    // Opposed static-friction contacts retain their decision until retreat or
    // separation; control previews and clock changes do not release hand lag.
    jammed: Vec<ColliderHandle>,
}

struct CarryAnchor {
    pose: Pose,
    route: Option<CarryRoute>,
    obstructed_seconds: f32,
}

struct CommandCarryAnchor {
    pose: Pose,
    origin: PlayerContext,
}

impl HeldItem {
    pub(super) const fn checkpoint_rotation(&self) -> [f32; 4] {
        self.relative_rotation.to_array()
    }
    pub(super) fn from_checkpoint(key: u64, rotation: [f32; 4]) -> Self {
        Self {
            key,
            relative_rotation: Quat::from_array(rotation).normalize(),
            route: None,
            control_delta: FIXED_STEP,
            block_dynamic: false,
            angular_velocity: Vec3::ZERO,
            obstructed_seconds: 0.0,
            solver_anchor: None,
            command_anchor: None,
            jammed: Vec::new(),
        }
    }
}

#[derive(Clone, Copy)]
enum CarryRoute {
    PositiveOrbit,
    NegativeOrbit,
    Above,
}

fn view_rotation(yaw: f32) -> Quat {
    // Camera yaw increases toward +X; a model's positive Y rotation has the
    // opposite sign. The carried item remains upright as camera pitch changes.
    Quat::from_rotation_y(-yaw)
}

pub(super) fn player_item_hull(player: PlayerContext) -> (Pose, Cuboid) {
    let center = Vec3::new(
        player.feet.x,
        player.height.mul_add(0.5, player.feet.y),
        player.feet.z,
    );
    let half = Vector::new(player.half, player.height * 0.5, player.half);
    (pose(center, Quat::IDENTITY), Cuboid::new(half))
}

fn camera_clear(candidate: &Pose, shape: &SharedShape, eye: Vec3) -> bool {
    let camera = Ball::new(CAMERA_RADIUS);
    intersection_test(
        candidate,
        shape.as_ref(),
        &pose(eye, Quat::IDENTITY),
        &camera,
    )
    .is_ok_and(|overlap| !overlap.intersecting)
}

fn translated_pose(start: &Pose, movement: Vector, time: f32) -> Pose {
    Pose::from_parts(
        vector(add(point(start.translation), scaled(point(movement), time))),
        start.rotation,
    )
}

fn carry_contact_normal(
    start: &Pose,
    shape: &dyn Shape,
    movement: Vector,
    other: &Collider,
    hit: &ShapeCastHit,
    skin: f32,
) -> Vector {
    // Near the contact skin a support-map sweep can return an unstable
    // starting normal. Use the exact shape contact at its impact pose, and
    // test the resulting normal before allowing a separating movement.
    contact(
        &translated_pose(start, movement, hit.time_of_impact),
        shape,
        other.position(),
        other.shape(),
        skin + 0.001,
    )
    .ok()
    .flatten()
    .map_or_else(
        || vector(quaternion(start.rotation).mul_vec3(point(hit.normal1))),
        |contact| contact.normal1,
    )
}

fn clear_carry_translation(
    start: &Pose,
    shape: &dyn Shape,
    movement: Vector,
    other: &Collider,
    clearance: f32,
    limit: f32,
) -> f32 {
    if limit <= f32::EPSILON {
        return 0.0;
    }
    let mut clear = 0.0_f32;
    let mut blocked = limit;
    for _ in 0_u8..12 {
        let middle = clear.midpoint(blocked);
        if contact(
            &translated_pose(start, movement, middle),
            shape,
            other.position(),
            other.shape(),
            clearance,
        )
        .is_ok_and(|contact| contact.is_none())
        {
            clear = middle;
        } else {
            blocked = middle;
        }
    }
    clear
}

fn carry_waypoint(
    route: CarryRoute,
    center: Vec3,
    desired: Vec3,
    player: PlayerContext,
    radius: f32,
) -> Vec3 {
    let radial = Vec3::new(center.x - player.feet.x, 0.0, center.z - player.feet.z);
    let outward = radial
        .try_normalize()
        .unwrap_or_else(|| view_rotation(player.yaw).mul_vec3(Vec3::X));
    let clearance = player.half + radius + 0.12;
    match route {
        CarryRoute::Above => {
            let above = 2.0_f32.mul_add(SWEEP_MARGIN, player.feet.y + player.height + radius);
            if center.y < above - SWEEP_MARGIN {
                add(
                    center,
                    add(scaled(outward, 0.04), Vec3::new(0.0, above - center.y, 0.0)),
                )
            } else {
                Vec3::new(desired.x, center.y.max(above), desired.z)
            }
        }
        CarryRoute::PositiveOrbit | CarryRoute::NegativeOrbit => {
            let sign = if matches!(route, CarryRoute::PositiveOrbit) {
                1.0
            } else {
                -1.0
            };
            let tangent = scaled(Vec3::new(-outward.z, 0.0, outward.x), sign);
            if radial.length() >= 2.0 * clearance {
                // Approach a clear shoulder waypoint when catching up after
                // a prescribed teleport, instead of orbiting metres away.
                add(
                    Vec3::new(player.feet.x, desired.y, player.feet.z),
                    scaled(tangent, clearance * 1.5),
                )
            } else {
                let lateral = add(scaled(tangent, clearance * 1.5), scaled(outward, 0.04));
                add(
                    center,
                    Vec3::new(lateral.x, desired.y - center.y, lateral.z),
                )
            }
        }
    }
}

fn blocking_player_item(runtime: &Runtime, handle: ColliderHandle, key: u64, frozen: bool) -> bool {
    runtime.entries.get(&key).is_some_and(|entry| {
        entry.collider == handle
            && (frozen
                || entry.dormant
                || runtime.held.as_ref().is_some_and(|held| held.key == key))
    })
}

impl ItemWorld {
    /// A tool contact applies the existing throw-strength mass curve at the
    /// contact point. It neither picks up the body nor changes inventory.
    pub fn strike_body(&mut self, key: u64, direction: Vec3, contact: Vec3) -> bool {
        let Some(aim) = direction.try_normalize().filter(|aim| aim.is_finite()) else {
            return false;
        };
        if !contact.is_finite() || self.held() == Some(key) {
            return false;
        }
        let Some(runtime) = self.runtime.as_mut() else {
            return false;
        };
        if !runtime.activate_key(key) {
            return false;
        }
        let Some(entry) = runtime.entries.get(&key) else {
            return false;
        };
        if entry.motion != super::BodyMotion::Dynamic {
            return false;
        }
        let Some(body) = runtime.world.bodies.get_mut(entry.body) else {
            return false;
        };
        let mass = body.mass();
        if !mass.is_finite() || mass <= 0.0 {
            return false;
        }
        let speed = 8.0 / mass.sqrt().max(1.0);
        body.apply_impulse_at_point(vector(scaled(aim, mass * speed)), vector(contact), true);
        true
    }

    /// Checks the prescribed player hull against live item bodies only.
    /// Static level geometry remains the owning controller's responsibility.
    #[must_use]
    pub fn player_pose_clear(&self, player: PlayerContext) -> bool {
        if !player.valid() {
            return false;
        }
        let Some(runtime) = self.runtime.as_ref() else {
            return true;
        };
        let (target, hull) = player_item_hull(player);
        let only_items = |handle, collider: &rapier3d::prelude::Collider| {
            collider.is_enabled() && runtime.key_for_collider(handle).is_some()
        };
        runtime
            .world
            .intersect_shape(
                target,
                &hull,
                query_filter().predicate(&only_items),
            )
            .next()
            .is_none()
    }

    /// Sweeps a camera sphere against physical items and installed scenery.
    /// Invalid vectors or a radius outside `0 < radius <= 0.5` are rejected.
    #[must_use]
    pub fn camera_sweep(&self, start: Vec3, destination: Vec3, radius: f32) -> Option<Vec3> {
        if !start.is_finite()
            || !destination.is_finite()
            || !radius.is_finite()
            || radius <= 0.0
            || radius > 0.5
        {
            return None;
        }
        let Some(runtime) = self.runtime.as_ref() else {
            return Some(destination);
        };
        let camera = Ball::new(radius);
        let start_pose = pose(start, Quat::IDENTITY);
        let filter = query_filter().exclude_rigid_body(runtime.player);
        if runtime
            .world
            .intersect_shape(start_pose, &camera, filter)
            .next()
            .is_some()
        {
            return Some(start);
        }
        let motion = subtract(destination, start);
        let options = ShapeCastOptions {
            max_time_of_impact: 1.0,
            target_distance: SWEEP_MARGIN,
            stop_at_penetration: false,
            ..ShapeCastOptions::default()
        };
        let travel = runtime
            .world
            .cast_shape(&start_pose, vector(motion), &camera, options, filter)
            .map_or(1.0, |(_, hit)| hit.time_of_impact.clamp(0.0, 1.0));
        Some(add(start, scaled(motion, travel)))
    }

    /// Sweeps all controller movement axes against held and dormant items.
    /// A stopped world also blocks on active items; live dynamics retain their
    /// ordinary solver-driven pushing behavior.
    #[must_use]
    pub fn constrain_player_eye(&self, player: PlayerContext) -> Vec3 {
        let Some(runtime) = self.runtime.as_ref() else {
            return player.eye;
        };
        let Some(previous) = runtime.last_player else {
            return player.eye;
        };
        self.sweep_item_contact(runtime, previous, player, None).0
    }

    pub(crate) fn constrain_player_eye_from(
        &self,
        previous: PlayerContext,
        player: PlayerContext,
    ) -> (Vec3, f32) {
        self.runtime.as_ref().map_or((player.eye, 1.0), |runtime| {
            let (eye, fraction, _) = self.sweep_item_contact(runtime, previous, player, None);
            (eye, fraction)
        })
    }

    /// Sweeps the accepted footprint straight up or down against blocking
    /// items. An exact cast keeps lateral contact skin from acting as a roof.
    pub(crate) fn vertical_player_item_travel(
        &self,
        player: PlayerContext,
        travel: f32,
        min_normal_y: f32,
    ) -> Option<f32> {
        let runtime = self.runtime.as_ref()?;
        if !player.active || !player.valid() || travel == 0.0 {
            return None;
        }
        let hull = Cuboid::new(Vector::new(player.half, player.height * 0.5, player.half));
        let center = Vec3::new(
            player.feet.x,
            player.height.mul_add(0.5, player.feet.y),
            player.feet.z,
        );
        let start = pose(center, Quat::IDENTITY);
        let delta = vector(Vec3::new(0.0, travel, 0.0));
        let end = translated_pose(&start, delta, 1.0);
        let bounds = hull.compute_aabb(&start).merged(&hull.compute_aabb(&end));
        let clock = self.simulation_state();
        let frozen = clock.paused || clock.time_scale <= 0.0;
        let blocking = |handle, _collider: &Collider| {
            runtime
                .key_for_collider(handle)
                .is_some_and(|key| blocking_player_item(runtime, handle, key, frozen))
        };
        let filter = query_filter()
            .exclude_rigid_body(runtime.player)
            .predicate(&blocking);
        let options = ShapeCastOptions {
            max_time_of_impact: 1.0,
            stop_at_penetration: false,
            ..ShapeCastOptions::default()
        };
        runtime
            .world
            .intersect_aabb_conservative(bounds, filter)
            .filter_map(|(_, obstacle)| {
                let hit = cast_shapes(
                    &start,
                    delta,
                    &hull,
                    obstacle.position(),
                    Vector::ZERO,
                    obstacle.shape(),
                    options,
                )
                .ok()
                .flatten()?;
                // Local normal1 points from the unrotated player toward the item.
                (hit.normal1.y * travel.signum() > min_normal_y)
                    .then_some(hit.time_of_impact.clamp(0.0, 1.0))
            })
            .min_by(f32::total_cmp)
            .map(|fraction| fraction * travel)
    }

    /// Uses the resolved start of this command and the carry pose swept by
    /// earlier commands in the same frame. The item solver owns the live pose.
    pub(crate) fn constrain_carried_player_eye(
        &self,
        previous: PlayerContext,
        player: PlayerContext,
        slide: HeldPlayerSlide,
    ) -> (Vec3, f32, Option<HeldPlayerContact>) {
        self.runtime
            .as_ref()
            .map_or((player.eye, 1.0, None), |runtime| {
                self.sweep_item_contact(runtime, previous, player, Some(slide))
            })
    }

    fn sweep_item_contact(
        &self,
        runtime: &Runtime,
        previous: PlayerContext,
        player: PlayerContext,
        slide_on_held: Option<HeldPlayerSlide>,
    ) -> (Vec3, f32, Option<HeldPlayerContact>) {
        if !player.active || !previous.active || !player.valid() || !previous.valid() {
            return (player.eye, 1.0, None);
        }
        let motion = subtract(player.feet, previous.feet);
        if motion.length_squared() == 0.0 {
            return (player.eye, 1.0, None);
        }
        let clock = self.simulation_state();
        let frozen = clock.paused || clock.time_scale <= 0.0;
        let held_key = runtime.held.as_ref().map(|held| held.key);
        let start = add(previous.feet, Vec3::new(0.0, player.height * 0.5, 0.0));
        let hull = SharedShape::cuboid(player.half, player.height * 0.5, player.half);
        let options = ShapeCastOptions {
            max_time_of_impact: 1.0,
            target_distance: SWEEP_MARGIN,
            // Allow separating motion when a become copy starts around its player.
            stop_at_penetration: false,
            ..ShapeCastOptions::default()
        };
        let sweep = |center, delta, sliding| {
            let blocking = |handle, collider: &Collider| {
                collider.is_enabled()
                    && runtime.key_for_collider(handle).map_or(sliding, |key| {
                        blocking_player_item(runtime, handle, key, frozen)
                    })
            };
            runtime.world.cast_shape(
                &pose(center, Quat::IDENTITY),
                vector(delta),
                hull.as_ref(),
                options,
                query_filter()
                    .exclude_rigid_body(runtime.player)
                    .predicate(&blocking),
            )
        };
        let Some((handle, hit)) = sweep(start, motion, false) else {
            return (player.eye, 1.0, None);
        };
        let travel = hit.time_of_impact.clamp(0.0, 1.0);
        let moved = scaled(motion, travel);
        let resolved_eye = add(subtract(player.eye, motion), moved);
        if let Some(slide_mode) = slide_on_held
            && held_key.is_some()
            && runtime.key_for_collider(handle) == held_key
        {
            let normal = slide_mode.contact_normal(hit.normal1);
            let remaining = scaled(motion, 1.0 - travel);
            let slide = subtract(remaining, scaled(normal, remaining.dot(normal).min(0.0)));
            // The slide can take a path the main controller hasn't swept.
            let slide_hit = (slide.length_squared() > 1.0e-10)
                .then(|| sweep(add(start, moved), slide, true))
                .flatten()
                .map(|(_, obstacle)| obstacle);
            let slide_travel =
                slide_hit.map_or(1.0, |obstacle| obstacle.time_of_impact.clamp(0.0, 1.0));
            (
                resolved_eye,
                travel,
                Some(HeldPlayerContact {
                    slide_fraction: (1.0 - travel).mul_add(slide_travel, travel),
                    eye_after_slide: add(resolved_eye, scaled(slide, slide_travel)),
                    normal,
                    slide_normal: slide_hit
                        .map(|obstacle| slide_mode.contact_normal(obstacle.normal1)),
                }),
            )
        } else {
            (resolved_eye, travel, None)
        }
    }

    /// Advances the checked carry query pose between movement commands. These
    /// poses are available to stance and release while world time stands still.
    pub(crate) fn preview_carried_translation(
        &mut self,
        origin: PlayerContext,
        player: PlayerContext,
        doors: &[DoorCollider],
    ) {
        if let Some(runtime) = self.runtime.as_mut() {
            runtime.preview_carried_translation(origin, player, doors);
        }
    }

    /// Publishes the last swept query pose to the interaction and model APIs.
    pub(crate) fn publish_carried_preview(&mut self) {
        if let Some(runtime) = self.runtime.as_mut()
            && runtime
                .held
                .as_ref()
                .is_some_and(|held| held.command_anchor.is_some())
        {
            runtime.update_poses();
        }
    }

    /// The nearest enabled item collider on the exact ray, including fixed or
    /// unpickable bodies. World geometry is resolved separately by the shared
    /// interaction target. Query reach must be finite and at most four metres.
    #[must_use]
    pub fn first_body_hit(
        &self,
        eye: Vec3,
        direction: Vec3,
        max_distance: f32,
    ) -> Option<(u64, f32)> {
        self.first_body_hit_excluding(eye, direction, max_distance, None)
    }

    /// Door interaction while carrying ignores only the owned body. Every other
    /// enabled physical collider still blocks the exact sight ray.
    #[must_use]
    pub fn first_unheld_body_hit(
        &self,
        eye: Vec3,
        direction: Vec3,
        max_distance: f32,
    ) -> Option<(u64, f32)> {
        self.first_body_hit_excluding(eye, direction, max_distance, self.held())
    }

    fn first_body_hit_excluding(
        &self,
        eye: Vec3,
        direction: Vec3,
        max_distance: f32,
        excluded: Option<u64>,
    ) -> Option<(u64, f32)> {
        if !eye.is_finite()
            || !direction.is_finite()
            || !max_distance.is_finite()
            || !(0.0..=4.0).contains(&max_distance)
        {
            return None;
        }
        let runtime = self.runtime.as_ref()?;
        let unit_direction = direction.try_normalize()?;
        let only_items = |handle, collider: &rapier3d::prelude::Collider| {
            collider.is_enabled()
                && runtime.key_for_collider(handle).is_some_and(|key| {
                    Some(key) != excluded
                        && runtime
                            .entries
                            .get(&key)
                            .is_some_and(|entry| entry.collider == handle)
                })
        };
        let filter = query_filter().predicate(&only_items);
        let ray = Ray::new(vector(eye), vector(unit_direction));
        let (handle, distance) = runtime.world.cast_ray(&ray, max_distance, true, filter)?;
        Some((runtime.key_for_collider(handle)?, distance))
    }

    /// The first physical ray hit, only when it is pickable and within reach.
    /// Static geometry and other bodies occlude items behind them.
    #[must_use]
    pub fn nearest_pickup(&self, eye: Vec3, direction: Vec3) -> Option<u64> {
        if !eye.is_finite() || !direction.is_finite() {
            return None;
        }
        let unit_direction = direction.try_normalize()?;
        let runtime = self.runtime.as_ref()?;
        if runtime.held.is_some() {
            return None;
        }
        let ray = Ray::new(vector(eye), vector(unit_direction));
        let filter = query_filter().exclude_collider(runtime.player_collider);
        let (hit, _distance) = runtime.world.cast_ray(&ray, PICKUP_REACH, true, filter)?;
        let key = runtime.key_for_collider(hit)?;
        let entry = runtime.entries.get(&key)?;
        let fits_reach = HOLD_DISTANCE.max(entry.radius + 0.45) + entry.radius <= PICKUP_REACH;
        (entry.pickup && entry.motion == super::BodyMotion::Dynamic && fits_reach).then_some(key)
    }

    /// An unobstructed dormant movable body that can be activated by Interact.
    /// Large character copies may be too wide to carry; they still expose the
    /// same bounded activation action without pretending to be pickup items.
    #[must_use]
    pub fn nearest_activation(&self, eye: Vec3, direction: Vec3) -> Option<u64> {
        if !eye.is_finite() || !direction.is_finite() {
            return None;
        }
        let runtime = self.runtime.as_ref()?;
        let unit = direction.try_normalize()?;
        let ray = Ray::new(vector(eye), vector(unit));
        let (hit, _distance) = runtime.world.cast_ray(
            &ray,
            PICKUP_REACH,
            true,
            query_filter().exclude_collider(runtime.player_collider),
        )?;
        let key = runtime.key_for_collider(hit)?;
        let entry = runtime.entries.get(&key)?;
        (entry.dormant && entry.motion.is_movable()).then_some(key)
    }

    /// A sustained collision refusal while the held body is away from its hand
    /// goal. Normal arrival and brief acquisition movement are excluded.
    #[must_use]
    pub fn held_movement_blocked(&self) -> bool {
        self.runtime
            .as_ref()
            .and_then(|runtime| runtime.held.as_ref())
            .is_some_and(|held| held.obstructed_seconds > OBSTRUCTED_SECONDS)
    }

    /// Eye-to-body-centre limit for sustained obstructed carrying, adjusted
    /// conservatively for the carried shape's bounding radius.
    #[must_use]
    pub fn held_auto_drop_distance(&self) -> Option<f32> {
        let runtime = self.runtime.as_ref()?;
        let held = runtime.held.as_ref()?;
        let entry = runtime.entries.get(&held.key)?;
        Some(PICKUP_REACH + AUTO_DROP_MARGIN + entry.radius)
    }

    pub(super) fn auto_drop_obstructed(&mut self, player: PlayerContext) -> Option<u64> {
        let limit = self.held_auto_drop_distance()?;
        let runtime = self.runtime.as_ref()?;
        let held = runtime.held.as_ref()?;
        if held.obstructed_seconds <= OBSTRUCTED_SECONDS {
            return None;
        }
        let key = held.key;
        let entry = runtime.entries.get(&key)?;
        let body = runtime.world.bodies.get(entry.body)?;
        if point(body.position().translation).distance_squared(player.eye) <= limit * limit {
            return None;
        }
        // A moving obstacle may have occupied this pose since its last sweep.
        // Retry safely rather than forcing an overlapping release or teleport.
        (self.can_drop() && self.release_held_at_rest()).then_some(key)
    }

    #[must_use]
    pub fn held(&self) -> Option<u64> {
        self.runtime.as_ref()?.held.as_ref().map(|held| held.key)
    }

    /// Begins carrying an unobstructed ray target without changing its pose.
    /// Subsequent fixed steps sweep both translation and yaw rotation to the hand.
    pub fn pickup(&mut self, key: u64, eye: Vec3, direction: Vec3, yaw: f32) -> bool {
        if !yaw.is_finite() || self.nearest_pickup(eye, direction) != Some(key) {
            return false;
        }
        let Some(runtime) = self.runtime.as_mut() else {
            return false;
        };
        if !runtime.activate_key(key) {
            return false;
        }
        let Some(entry) = runtime.entries.get_mut(&key) else {
            return false;
        };
        let Some(body) = runtime.world.bodies.get_mut(entry.body) else {
            return false;
        };
        let relative_rotation = view_rotation(yaw)
            .inverse()
            .mul_quat(entry.authored_pose.rotation);
        body.set_body_type(RigidBodyType::KinematicPositionBased, true);
        body.set_linvel(Vector::ZERO, true);
        body.set_angvel(Vector::ZERO, true);
        body.set_next_kinematic_position(*body.position());
        entry.previous = entry.current;
        runtime.held = Some(HeldItem {
            key,
            relative_rotation,
            route: None,
            control_delta: FIXED_STEP,
            block_dynamic: false,
            angular_velocity: Vec3::ZERO,
            obstructed_seconds: 0.0,
            solver_anchor: None,
            command_anchor: None,
            jammed: Vec::new(),
        });
        true
    }

    /// Checks whether the held item's current pose permits an ordinary drop.
    /// Refreshes collision queries without releasing or moving the item.
    pub fn can_drop(&mut self) -> bool {
        let Some(runtime) = self.runtime.as_mut() else {
            return false;
        };
        runtime.refresh_queries();
        let Some(entry) = runtime
            .held
            .as_ref()
            .and_then(|held| runtime.entries.get(&held.key))
        else {
            return false;
        };
        let Some(body) = runtime.world.bodies.get(entry.body) else {
            return false;
        };
        let candidate = *body.position();
        if runtime
            .last_player
            .is_some_and(|player| !camera_clear(&candidate, &entry.shape, player.eye))
        {
            return false;
        }
        let filter = query_filter().exclude_collider(entry.collider);
        if runtime
            .world
            .intersect_shape(candidate, entry.shape.as_ref(), filter)
            .any(|(_, collider)| {
                // Resting shapes retain solver-scale roundoff at their support.
                // An intersection flag alone mistakes that contact for an
                // unsafe drop; actual penetration still refuses release.
                !contact(
                    &candidate,
                    entry.shape.as_ref(),
                    collider.position(),
                    collider.shape(),
                    0.0,
                )
                .is_ok_and(|geometry| {
                    geometry.is_none_or(|surface| {
                        surface.dist >= -crate::engine::collision::CONTACT_EPS
                    })
                })
            })
        {
            return false;
        }
        true
    }

    /// Releases from the latest checked carry pose. A newly overlapping wall,
    /// door, player or ceiling keeps the item held and returns false.
    /// Velocity comes only from bounded player translation, never hand teleports.
    pub fn drop(&mut self) -> bool {
        let velocity = self
            .runtime
            .as_ref()
            .map_or(Vec3::ZERO, |runtime| runtime.player_velocity);
        self.drop_with_velocity(velocity)
    }

    pub(crate) fn drop_with_velocity(&mut self, velocity: Vec3) -> bool {
        if !self.can_drop() {
            return false;
        }
        let Some(runtime) = self.runtime.as_mut() else {
            return false;
        };
        let Some(held) = runtime.held.as_ref() else {
            return false;
        };
        let key = held.key;
        let angular_velocity = held.angular_velocity;
        let Some(entry) = runtime.entries.get_mut(&key) else {
            return false;
        };
        let Some(body) = runtime.world.bodies.get(entry.body) else {
            return false;
        };
        let candidate = *body.position();
        let Some(released_body) = runtime.world.bodies.get_mut(entry.body) else {
            return false;
        };
        released_body.set_body_type(RigidBodyType::Dynamic, true);
        let inherited = if velocity.is_finite() {
            velocity.clamp_length_max(2.0)
        } else {
            Vec3::ZERO
        };
        released_body.set_linvel(vector(inherited), true);
        // Only actual continuously swept hand rotation is transferred. A
        // camera snap cannot create an unbounded synthetic release impulse.
        released_body.set_angvel(vector(angular_velocity.clamp_length_max(3.0)), true);
        released_body.set_next_kinematic_position(candidate);
        entry.previous = entry.current;
        entry.release = super::ImpactRelease::Dropped;
        entry.originating_release = super::ImpactRelease::Dropped;
        entry.release_serial = entry.release_serial.saturating_add(1);
        entry.release_source = key;
        entry.release_incident = entry.release_serial;
        runtime.restore_carry_contacts();
        runtime.held = None;
        true
    }

    /// Safe release for lifecycle commands. An empty hand already succeeds;
    /// an obstructed held pose returns false without changing ownership.
    pub fn release_held(&mut self) -> bool {
        self.held().is_none() || self.drop()
    }

    /// Detaches an item after an authoritative player reset without moving it
    /// toward the new player. Its last valid swept pose is retained and its
    /// motion is cleared; the world clock still determines when gravity resumes.
    /// Unlike an ordinary drop, this does not validate against the new player.
    pub fn release_held_for_reset(&mut self) -> bool {
        self.release_held_at_rest()
    }

    /// Retain the current pose without inheriting remote player motion. Callers
    /// must validate clearance or own an authoritative player-reset boundary.
    fn release_held_at_rest(&mut self) -> bool {
        let Some(runtime) = self.runtime.as_mut() else {
            return true;
        };
        let Some(key) = runtime.held.as_ref().map(|held| held.key) else {
            return true;
        };
        let Some(entry) = runtime.entries.get(&key) else {
            return false;
        };
        let Some(body) = runtime.world.bodies.get_mut(entry.body) else {
            return false;
        };
        let retained = *body.position();
        if !point(retained.translation).is_finite() || !quaternion(retained.rotation).is_finite() {
            return false;
        }
        body.set_body_type(RigidBodyType::Dynamic, true);
        body.set_linvel(Vector::ZERO, true);
        body.set_angvel(Vector::ZERO, true);
        body.reset_forces(true);
        body.reset_torques(true);
        body.set_next_kinematic_position(retained);
        runtime.restore_carry_contacts();
        runtime.held = None;
        runtime.clear_item_contacts(key);
        runtime.update_poses();
        if let Some(released_entry) = runtime.entries.get_mut(&key) {
            released_entry.previous = released_entry.current;
            released_entry.release = super::ImpactRelease::Dropped;
            released_entry.originating_release = super::ImpactRelease::Dropped;
            released_entry.release_serial = released_entry.release_serial.saturating_add(1);
            released_entry.release_source = key;
            released_entry.release_incident = released_entry.release_serial;
        }
        runtime.populate_counts(&mut self.stats);
        true
    }

    /// Safely releases and applies a forward physical impulse. Bodies up to
    /// one kilogram leave at 8 m/s; heavier bodies receive `8 / sqrt(mass)`.
    /// Invalid aim or an obstructed release keeps the held state intact.
    pub fn throw(&mut self, direction: Vec3) -> bool {
        let velocity = self
            .runtime
            .as_ref()
            .map_or(Vec3::ZERO, |runtime| runtime.player_velocity);
        self.throw_with_velocity(direction, velocity)
    }

    pub(crate) fn throw_with_velocity(&mut self, direction: Vec3, velocity: Vec3) -> bool {
        if !direction.is_finite() {
            return false;
        }
        let Some(aim) = direction.try_normalize() else {
            return false;
        };
        let Some(key) = self.held() else {
            return false;
        };
        let mass = self.runtime.as_ref().and_then(|runtime| {
            let entry = runtime.entries.get(&key)?;
            runtime
                .world
                .bodies
                .get(entry.body)
                .map(rapier3d::prelude::RigidBody::mass)
        });
        let Some(mass_kg) = mass.filter(|mass_kg| mass_kg.is_finite() && *mass_kg > 0.0) else {
            return false;
        };
        if !self.drop_with_velocity(velocity) {
            return false;
        }
        let Some(runtime) = self.runtime.as_mut() else {
            return false;
        };
        let Some(entry) = runtime.entries.get(&key) else {
            return false;
        };
        let Some(body) = runtime.world.bodies.get_mut(entry.body) else {
            return false;
        };
        let speed = 8.0 / mass_kg.sqrt().max(1.0);
        body.apply_impulse(vector(scaled(aim, mass_kg * speed)), true);
        if let Some(released) = runtime.entries.get_mut(&key) {
            released.release = super::ImpactRelease::Thrown;
            released.originating_release = super::ImpactRelease::Thrown;
        }
        true
    }
}

impl Runtime {
    pub(super) fn restore_carry_contacts(&mut self) {
        self.contact_solver.restore_contact_hooks(&mut self.world);
    }

    pub(super) const fn accept_held_preview(&mut self) {
        if let Some(held) = self.held.as_mut() {
            held.solver_anchor = None;
            held.command_anchor = None;
        }
    }

    pub(super) fn preview_held(&mut self, player: PlayerContext, delta: f32) {
        if let Some(held) = self.held.as_mut()
            && held.solver_anchor.is_none()
        {
            held.solver_anchor = self
                .entries
                .get(&held.key)
                .and_then(|entry| self.world.bodies.get(entry.body))
                .map(|body| CarryAnchor {
                    pose: *body.position(),
                    route: held.route,
                    obstructed_seconds: held.obstructed_seconds,
                });
        }
        self.advance_held(player, delta, true);
    }

    fn preview_carried_translation(
        &mut self,
        origin: PlayerContext,
        player: PlayerContext,
        doors: &[DoorCollider],
    ) {
        if !origin.active || !player.active || !origin.valid() || !player.valid() {
            return;
        }
        let Some(held) = self.held.as_ref() else {
            return;
        };
        let motion = subtract(player.eye, origin.eye);
        // A routed or jammed body still needs the ordinary hand solver.
        if held.route.is_some() || !held.jammed.is_empty() || motion.length_squared() == 0.0 {
            return;
        }
        let Some(entry) = self.entries.get(&held.key) else {
            return;
        };
        let Some(body) = self.world.bodies.get(entry.body) else {
            return;
        };
        let start = *body.position();
        // Retain contact with an item behind the new aim after a camera turn.
        if subtract(point(start.translation), origin.eye).dot(origin.view) <= 0.0 {
            return;
        }
        let cast_motion = vector(motion);
        let options = ShapeCastOptions {
            max_time_of_impact: 1.0,
            target_distance: SWEEP_MARGIN,
            stop_at_penetration: false,
            ..ShapeCastOptions::default()
        };
        // Treat active items as solid for the preview. The solver owns
        // their pushback and still sees the original carry pose.
        let filter = query_filter()
            .exclude_collider(entry.collider)
            .exclude_rigid_body(self.player);
        let mut travel = self
            .world
            .cast_shape(&start, cast_motion, entry.shape.as_ref(), options, filter)
            .map_or(1.0, |(_, hit)| hit.time_of_impact.clamp(0.0, 1.0));
        for door in doors {
            // Gameplay has already advanced the leaves for this frame.
            let shape = Cuboid::new(Vector::new(
                door.width * 0.5,
                door.height * 0.5,
                door.thickness * 0.5,
            ));
            if let Ok(Some(hit)) = cast_shapes(
                &start,
                cast_motion,
                entry.shape.as_ref(),
                &door_pose(door),
                Vector::ZERO,
                &shape,
                options,
            ) {
                travel = travel.min(hit.time_of_impact.clamp(0.0, 1.0));
            }
        }
        let candidate = translated_pose(&start, cast_motion, travel);
        let hull = Cuboid::new(Vector::new(player.half, player.height * 0.5, player.half));
        let center = add(player.feet, Vec3::new(0.0, player.height * 0.5, 0.0));
        if travel <= f32::EPSILON
            || !camera_clear(&candidate, &entry.shape, player.eye)
            || !intersection_test(
                &candidate,
                entry.shape.as_ref(),
                &pose(center, Quat::IDENTITY),
                &hull,
            )
            .is_ok_and(|overlap| !overlap.intersecting)
        {
            return;
        }
        if let Some(pending) = self.held.as_mut()
            && let Some(body_to_preview) = self.world.bodies.get_mut(entry.body)
        {
            let _anchor = pending.command_anchor.get_or_insert(CommandCarryAnchor {
                pose: start,
                origin,
            });
            body_to_preview.set_position(candidate, false);
            body_to_preview.set_next_kinematic_position(candidate);
            self.world
                .bodies
                .propagate_modified_body_positions_to_colliders(&mut self.world.colliders);
            refresh_collider(&mut self.world, entry.collider);
        }
    }

    pub(super) fn restore_command_carry_pose(&mut self) -> Option<PlayerContext> {
        let held = self.held.as_mut()?;
        let anchor = held.command_anchor.take()?;
        let entry = self.entries.get(&held.key)?;
        let body = self.world.bodies.get_mut(entry.body)?;
        body.set_position(anchor.pose, false);
        body.set_next_kinematic_position(anchor.pose);
        let collider = entry.collider;
        self.world
            .bodies
            .propagate_modified_body_positions_to_colliders(&mut self.world.colliders);
        refresh_collider(&mut self.world, collider);
        self.update_poses();
        Some(anchor.origin)
    }

    fn carry_filter(&self, collider: ColliderHandle) -> QueryFilter<'static> {
        let mut filter = query_filter().exclude_collider(collider);
        // Physical ticks push active items normally. Swept control previews
        // and stopped-world updates treat those bodies as immobile obstacles.
        if self.held.as_ref().is_some_and(|held| !held.block_dynamic) {
            filter.flags |= QueryFilterFlags::EXCLUDE_DYNAMIC;
        }
        filter
    }

    fn carry_delta(&self) -> f32 {
        self.held
            .as_ref()
            .map_or(FIXED_STEP, |held| held.control_delta)
    }

    fn restore_held_anchor(&mut self) {
        if let Some(held) = self.held.as_mut()
            && let Some(anchor) = held.solver_anchor.take()
            && let Some(entry) = self.entries.get(&held.key)
            && let Some(body) = self.world.bodies.get_mut(entry.body)
        {
            // Swept control previews are query/release poses. Restore the last
            // solved anchor so Rapier sees the entire fixed-interval motion.
            body.set_position(anchor.pose, false);
            body.set_next_kinematic_position(anchor.pose);
            held.route = anchor.route;
            held.obstructed_seconds = anchor.obstructed_seconds;
            self.update_poses();
            self.refresh_queries();
        }
    }

    pub(super) fn advance_held(&mut self, player: PlayerContext, delta: f32, immediate: bool) {
        if !immediate {
            self.restore_held_anchor();
        }
        if let Some(held) = self.held.as_mut() {
            held.control_delta = delta;
            held.block_dynamic = immediate;
            held.angular_velocity = Vec3::ZERO;
        }
        let Some(held) = self.held.as_ref() else {
            return;
        };
        let Some(entry) = self.entries.get(&held.key) else {
            return;
        };
        let Some(body) = self.world.bodies.get(entry.body) else {
            return;
        };
        let Some(direction) = player.view.try_normalize() else {
            return;
        };
        let start = *body.position();
        let desired_rotation = view_rotation(player.yaw).mul_quat(held.relative_rotation);
        let distance = HOLD_DISTANCE.max(entry.radius + 0.45);
        let right = Quat::from_rotation_y(-player.yaw).mul_vec3(Vec3::X);
        let ideal_center = add(player.eye, scaled(direction, distance));
        let desired_center = self.clipped_hand_center(entry, start, player, ideal_center);
        let overlaps_player = self
            .world
            .intersect_shape(
                start,
                entry.shape.as_ref(),
                query_filter().exclude_collider(entry.collider),
            )
            .any(|(handle, _)| handle == self.player_collider);
        if overlaps_player {
            let recovery =
                self.recover_carried_pose(entry, start, player, desired_center, right, direction);
            if let Some(recovered) = recovery {
                if let Some(held_body) = self.world.bodies.get_mut(entry.body) {
                    // This is geometric depenetration after prescribed player
                    // motion, not a hand velocity or a body passing a wall.
                    held_body.set_position(recovered, false);
                    held_body.set_next_kinematic_position(recovered);
                }
                self.update_poses();
                self.refresh_queries();
                return;
            }
        }
        let (routed_target, route) = self.routed_carried_pose(
            entry,
            start,
            desired_center,
            desired_rotation,
            player,
            held.route,
        );
        let (target, jammed) = if immediate {
            (routed_target, Vec::new())
        } else {
            self.swept_dynamic_carry(entry, start, routed_target, player.eye)
        };
        let body_handle = entry.body;
        if let Some(current_held) = self.held.as_mut() {
            if !immediate {
                current_held.jammed = jammed;
            }
            current_held.route = route;
            current_held.obstructed_seconds =
                if point(target.translation).distance_squared(point(start.translation)) < 1.0e-8
                    && point(start.translation).distance_squared(ideal_center) > 0.09
                {
                    (current_held.obstructed_seconds + delta).min(2.0)
                } else {
                    0.0
                };
            let raw_delta =
                quaternion(target.rotation).mul_quat(quaternion(start.rotation).inverse());
            let rotation_delta = if raw_delta.w < 0.0 {
                Quat::from_xyzw(-raw_delta.x, -raw_delta.y, -raw_delta.z, -raw_delta.w)
            } else {
                raw_delta
            };
            current_held.angular_velocity = scaled(rotation_delta.to_scaled_axis(), 1.0 / delta);
        }
        if let Some(held_body) = self.world.bodies.get_mut(body_handle) {
            if immediate {
                held_body.set_position(target, false);
                // Position-based kinematics ignore velocity setters. Clear
                // their stale solver velocity before a paused release/resume.
                held_body.sleep();
            }
            held_body.set_next_kinematic_position(target);
        }
        if immediate {
            self.update_poses();
            self.refresh_queries();
        }
    }

    fn routed_carried_pose(
        &self,
        entry: &super::world::BodyEntry,
        start: Pose,
        desired: Vec3,
        rotation: Quat,
        player: PlayerContext,
        previous_route: Option<CarryRoute>,
    ) -> (Pose, Option<CarryRoute>) {
        if !self.carry_path_crosses_player(entry, start, desired, player) {
            // The hand's ordinary pursuit limit must not leave it behind a jump
            // that is already clear. Only player motion toward the goal can add
            // speed, and the entire translation still passes the same sweep.
            let towards_hand = subtract(desired, point(start.translation)).normalize_or_zero();
            let speed = HOLD_SPEED.max(self.carry_follow_velocity.dot(towards_hand));
            return (
                self.swept_carried_pose(entry, start, desired, rotation, player.eye, speed)
                    .unwrap_or(start),
                None,
            );
        }
        let center = point(start.translation);
        let radial = Vec3::new(center.x - player.feet.x, 0.0, center.z - player.feet.z);
        let right = view_rotation(player.yaw).mul_vec3(Vec3::X);
        let tangent = Vec3::new(-radial.z, 0.0, radial.x);
        let positive_first = tangent.dot(right) >= 0.0;
        let (first, second) = if positive_first {
            (CarryRoute::PositiveOrbit, CarryRoute::NegativeOrbit)
        } else {
            (CarryRoute::NegativeOrbit, CarryRoute::PositiveOrbit)
        };
        // Keep an orbit's direction across frames so a half-turn cannot make
        // its preferred side flip as the object passes the player's shoulder.
        for route in [
            previous_route,
            Some(first),
            Some(second),
            Some(CarryRoute::Above),
        ]
        .into_iter()
        .flatten()
        {
            let waypoint = carry_waypoint(route, center, desired, player, entry.radius);
            if let Some(candidate) =
                self.swept_carried_pose(entry, start, waypoint, rotation, player.eye, HOLD_SPEED)
                && point(candidate.translation).distance_squared(center) > 1.0e-6
            {
                return (candidate, Some(route));
            }
        }
        (start, previous_route)
    }

    fn carry_path_crosses_player(
        &self,
        entry: &super::world::BodyEntry,
        start: Pose,
        desired: Vec3,
        player: PlayerContext,
    ) -> bool {
        let motion = subtract(desired, point(start.translation));
        let only_player = |handle, collider: &rapier3d::prelude::Collider| {
            handle == self.player_collider && collider.is_enabled()
        };
        let options = ShapeCastOptions {
            max_time_of_impact: 1.0,
            target_distance: SWEEP_MARGIN,
            stop_at_penetration: false,
            ..ShapeCastOptions::default()
        };
        let crosses_capsule = self
            .world
            .cast_shape(
                &start,
                vector(motion),
                entry.shape.as_ref(),
                options,
                query_filter().predicate(&only_player),
            )
            .is_some();
        let from_center = subtract(player.eye, point(start.translation));
        let closest_time = if motion.length_squared() > 1.0e-8 {
            (from_center.dot(motion) / motion.length_squared()).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let nearest = add(point(start.translation), scaled(motion, closest_time));
        let camera_clearance = entry.radius + CAMERA_RADIUS + SWEEP_MARGIN;
        // A slow-world solver anchor can begin beside the current camera
        // after swept previews have already followed the moving controller.
        // Moving away from that initial proximity is not a path crossing;
        // the destination still has to pass the exact camera/shape check.
        crosses_capsule
            || (closest_time > 0.0
                && nearest.distance_squared(player.eye) < camera_clearance.powi(2))
    }

    fn clipped_hand_center(
        &self,
        entry: &super::world::BodyEntry,
        start: Pose,
        player: PlayerContext,
        ideal: Vec3,
    ) -> Vec3 {
        let clipped = self.clip_world_center(entry, start, player.eye, ideal);
        if self.hand_center_clear(entry, start, clipped, player.eye) {
            return clipped;
        }
        let right = view_rotation(player.yaw).mul_vec3(Vec3::X);
        let span = player.half + entry.radius + 0.1;
        // World clipping can put the hand goal inside the player's capsule.
        // Search the nearest clear shoulder offset while retaining the clipped
        // forward distance, so the route has an attainable end near a wall.
        for side in [1.0_f32, -1.0_f32] {
            let mut previous_offset = 0.0_f32;
            for fraction in [0.25_f32, 0.5, 0.75, 1.0, 1.5, 2.0] {
                let offset = span * fraction;
                let candidate = self.clip_world_center(
                    entry,
                    start,
                    player.eye,
                    add(clipped, scaled(right, side * offset)),
                );
                if self.hand_center_clear(entry, start, candidate, player.eye) {
                    let mut minimum = previous_offset;
                    let mut maximum = offset;
                    let mut best = candidate;
                    for _ in 0_u32..5_u32 {
                        let midpoint = minimum.midpoint(maximum);
                        let refined = self.clip_world_center(
                            entry,
                            start,
                            player.eye,
                            add(clipped, scaled(right, side * midpoint)),
                        );
                        if self.hand_center_clear(entry, start, refined, player.eye) {
                            maximum = midpoint;
                            best = refined;
                        } else {
                            minimum = midpoint;
                        }
                    }
                    return best;
                }
                previous_offset = offset;
            }
        }
        clipped
    }

    fn hand_center_clear(
        &self,
        entry: &super::world::BodyEntry,
        start: Pose,
        center: Vec3,
        eye: Vec3,
    ) -> bool {
        let candidate = Pose::from_parts(vector(center), start.rotation);
        let player_clear = self
            .world
            .colliders
            .get(self.player_collider)
            .is_some_and(|collider| {
                !collider.is_enabled()
                    || distance(
                        &candidate,
                        entry.shape.as_ref(),
                        collider.position(),
                        collider.shape(),
                    )
                    .is_ok_and(|separation| separation.distance >= SWEEP_MARGIN * 2.0)
            });
        player_clear
            && camera_clear(&candidate, &entry.shape, eye)
            && self
                .world
                .intersect_shape(
                    candidate,
                    entry.shape.as_ref(),
                    self.carry_filter(entry.collider),
                )
                .next()
                .is_none()
    }

    fn clip_world_center(
        &self,
        entry: &super::world::BodyEntry,
        start: Pose,
        eye: Vec3,
        ideal: Vec3,
    ) -> Vec3 {
        let from_eye = Pose::from_parts(vector(eye), start.rotation);
        let movement = subtract(ideal, eye);
        let filter = self
            .carry_filter(entry.collider)
            .exclude_rigid_body(self.player);
        let options = ShapeCastOptions {
            max_time_of_impact: 1.0,
            target_distance: SWEEP_MARGIN,
            stop_at_penetration: false,
            ..ShapeCastOptions::default()
        };
        self.world
            .cast_shape(
                &from_eye,
                vector(movement),
                entry.shape.as_ref(),
                options,
                filter,
            )
            .map_or(ideal, |(_, hit)| {
                // A second skin of clearance gives tangential motion an outward
                // component, instead of repeatedly casting from exact contact.
                let clipped = add(eye, scaled(movement, hit.time_of_impact.clamp(0.0, 1.0)));
                add(clipped, scaled(point(hit.normal1), SWEEP_MARGIN))
            })
    }

    fn swept_carried_pose(
        &self,
        entry: &super::world::BodyEntry,
        start: Pose,
        desired_center: Vec3,
        desired_rotation: Quat,
        eye: Vec3,
        speed: f32,
    ) -> Option<Pose> {
        let motion_delta = subtract(desired_center, point(start.translation))
            .clamp_length_max(speed * self.carry_delta());
        let filter = self.carry_filter(entry.collider);
        let options = ShapeCastOptions {
            max_time_of_impact: 1.0,
            target_distance: SWEEP_MARGIN,
            stop_at_penetration: false,
            ..ShapeCastOptions::default()
        };
        let travel = self
            .world
            .cast_shape(
                &start,
                vector(motion_delta),
                entry.shape.as_ref(),
                options,
                filter,
            )
            .map_or(1.0, |(_, hit)| hit.time_of_impact.clamp(0.0, 1.0));
        let moved_center = add(point(start.translation), scaled(motion_delta, travel));
        let moved = pose(moved_center, entry.current.rotation);
        // Sweeping rotation, rather than checking only its endpoint, protects
        // a long box corner from crossing a thin wall during a fast camera turn.
        let raw_rotation_delta = desired_rotation.mul_quat(entry.current.rotation.inverse());
        let rotation_delta = if raw_rotation_delta.w < 0.0 {
            Quat::from_xyzw(
                -raw_rotation_delta.x,
                -raw_rotation_delta.y,
                -raw_rotation_delta.z,
                -raw_rotation_delta.w,
            )
        } else {
            raw_rotation_delta
        };
        let angular_delta = rotation_delta
            .to_scaled_axis()
            .clamp_length_max(8.0 * self.carry_delta());
        let rotation_motion =
            NonlinearRigidMotion::new(moved, Vector::ZERO, Vector::ZERO, vector(angular_delta));
        let rotational_travel = self
            .world
            .cast_shape_nonlinear(
                &rotation_motion,
                entry.shape.as_ref(),
                0.0,
                1.0,
                false,
                filter,
            )
            .map_or(1.0, |(_, hit)| (hit.time_of_impact - 0.001).clamp(0.0, 1.0));
        let candidate = rotation_motion.position_at_time(rotational_travel);
        // A query can begin within its contact tolerance. Refuse a true
        // overlapping destination instead of allowing a ghosted carry pose.
        let accepted = self.carry_pose_clear(entry, candidate, eye);
        accepted.then_some(candidate)
    }

    fn carry_pose_clear(
        &self,
        entry: &super::world::BodyEntry,
        candidate: Pose,
        eye: Vec3,
    ) -> bool {
        camera_clear(&candidate, &entry.shape, eye)
            && self
                .world
                .intersect_shape(
                    candidate,
                    entry.shape.as_ref(),
                    self.carry_filter(entry.collider),
                )
                .next()
                .is_none()
    }

    fn swept_dynamic_carry(
        &self,
        entry: &super::world::BodyEntry,
        start: Pose,
        target: Pose,
        eye: Vec3,
    ) -> (Pose, Vec<ColliderHandle>) {
        // Let free dynamics receive the full hand velocity. If the contact's
        // normal response is blocked by scenery, stop the hand at the actual
        // contact skins instead of forcing a prop into an immovable wedge.
        let mut dynamic_filter = query_filter().exclude_collider(entry.collider);
        dynamic_filter.flags |=
            QueryFilterFlags::EXCLUDE_FIXED | QueryFilterFlags::EXCLUDE_KINEMATIC;
        let movement = vector(subtract(
            point(target.translation),
            point(start.translation),
        ));
        let destination = Pose::from_parts(target.translation, start.rotation);
        let bounds = entry
            .shape
            .compute_aabb(&start)
            .merged(&entry.shape.compute_aabb(&destination))
            .loosened(0.004);
        let skin = self
            .world
            .colliders
            .get(entry.collider)
            .map_or(0.0, Collider::contact_skin);
        let mut travel = 1.0_f32;
        let mut jammed = Vec::new();
        for (handle, other) in self
            .world
            .intersect_aabb_conservative(bounds, dynamic_filter)
        {
            let options = ShapeCastOptions {
                max_time_of_impact: 1.0,
                target_distance: skin + other.contact_skin(),
                stop_at_penetration: true,
                ..ShapeCastOptions::default()
            };
            let Ok(Some(hit)) = cast_shapes(
                &start,
                movement,
                entry.shape.as_ref(),
                other.position(),
                Vector::ZERO,
                other.shape(),
                options,
            ) else {
                continue;
            };
            let normal = carry_contact_normal(
                &start,
                entry.shape.as_ref(),
                movement,
                other,
                &hit,
                skin + other.contact_skin(),
            );
            if movement.dot(normal) <= 0.0 {
                continue;
            }
            let contact_time = hit.time_of_impact.clamp(0.0, 1.0);
            let response = vector(scaled(
                point(normal),
                (1.0 - contact_time) * movement.dot(normal).max(0.0),
            ));
            let fraction = self.dynamic_response_fraction(entry.collider, handle, other, response);
            if fraction < 1.0 {
                jammed.push(handle);
            }
            let mut candidate_time = (1.0 - contact_time).mul_add(fraction, contact_time);
            if fraction < 1.0 {
                // Refine the sweep's approximate impact against actual shape
                // separation. Retain the same clearance used for immovable
                // carry obstacles; a jam may stop before a solver impulse.
                candidate_time = clear_carry_translation(
                    &start,
                    entry.shape.as_ref(),
                    movement,
                    other,
                    (skin + other.contact_skin()).max(SWEEP_MARGIN),
                    candidate_time,
                );
            }
            travel = travel.min(candidate_time);
        }
        let translated = translated_pose(&start, movement, travel);
        let rotation = quaternion(target.rotation).mul_quat(quaternion(start.rotation).inverse());
        let shortest = if rotation.w < 0.0 {
            Quat::from_xyzw(-rotation.x, -rotation.y, -rotation.z, -rotation.w)
        } else {
            rotation
        };
        let motion = NonlinearRigidMotion::new(
            translated,
            Vector::ZERO,
            Vector::ZERO,
            vector(shortest.to_scaled_axis()),
        );
        let angular_travel =
            self.jammed_rotation_fraction(entry, &motion, dynamic_filter, &mut jammed);
        let candidate = motion.position_at_time(angular_travel);
        // A clipped translation changes the pivot of the earlier static
        // rotation sweep. Revalidate that endpoint against scenery/player.
        if self.carry_pose_clear(entry, candidate, eye) {
            (candidate, jammed)
        } else {
            (start, jammed)
        }
    }

    fn jammed_rotation_fraction(
        &self,
        entry: &super::world::BodyEntry,
        motion: &NonlinearRigidMotion,
        filter: QueryFilter<'_>,
        jammed: &mut Vec<ColliderHandle>,
    ) -> f32 {
        if motion.angvel.length_squared() < 1.0e-12 {
            return 1.0;
        }
        let start = motion.position_at_time(0.0);
        let destination = motion.position_at_time(1.0);
        let bounds = Ball::new(entry.radius + 0.004).compute_aabb(&start);
        let skin = self
            .world
            .colliders
            .get(entry.collider)
            .map_or(0.0, Collider::contact_skin);
        let mut travel = 1.0_f32;
        for (handle, other) in self.world.intersect_aabb_conservative(bounds, filter) {
            let stationary = NonlinearRigidMotion::new(
                *other.position(),
                Vector::ZERO,
                Vector::ZERO,
                Vector::ZERO,
            );
            let Ok(Some(hit)) = cast_shapes_nonlinear(
                motion,
                entry.shape.as_ref(),
                &stationary,
                other.shape(),
                0.0,
                1.0,
                false,
            ) else {
                continue;
            };
            let impact = motion.position_at_time(hit.time_of_impact);
            let normal = vector(quaternion(impact.rotation).mul_vec3(point(hit.normal1)));
            let movement = vector(subtract(
                point(destination.transform_point(hit.witness1)),
                point(impact.transform_point(hit.witness1)),
            ));
            let response = vector(scaled(point(normal), movement.dot(normal).max(0.0)));
            if self.dynamic_response_fraction(entry.collider, handle, other, response) >= 1.0 {
                continue;
            }
            jammed.push(handle);
            // Nonlinear casts find core contact. Bisect just that interval to
            // retain the authored skins before a blocked rotational contact.
            let mut clear = 0.0_f32;
            let mut blocked = hit.time_of_impact.clamp(0.0, 1.0);
            for _ in 0_u8..12 {
                let middle = clear.midpoint(blocked);
                let candidate = motion.position_at_time(middle);
                if contact(
                    &candidate,
                    entry.shape.as_ref(),
                    other.position(),
                    other.shape(),
                    skin + other.contact_skin(),
                )
                .is_ok_and(|contact| contact.is_none())
                {
                    clear = middle;
                } else {
                    blocked = middle;
                }
            }
            travel = travel.min(clear);
        }
        travel
    }

    fn dynamic_response_fraction(
        &self,
        held: ColliderHandle,
        pushed: ColliderHandle,
        body: &Collider,
        movement: Vector,
    ) -> f32 {
        let Some(direction) = movement.try_normalize() else {
            return 1.0;
        };
        let Some(hand) = self.world.colliders.get(held) else {
            return 1.0;
        };
        let hand_friction =
            self.floor_contacts
                .friction_at(hand, body, point(body.position().translation));
        let destination = Pose::from_parts(
            vector(add(point(body.position().translation), point(movement))),
            body.position().rotation,
        );
        let bounds = body
            .compute_aabb()
            .merged(&body.shape().compute_aabb(&destination))
            .loosened(0.004);
        let mut filter = query_filter().exclude_collider(held);
        filter.flags |= QueryFilterFlags::EXCLUDE_DYNAMIC;
        let mut fraction = 1.0_f32;
        for (handle, obstacle) in self.world.intersect_aabb_conservative(bounds, filter) {
            if handle == pushed {
                continue;
            }
            let options = ShapeCastOptions {
                max_time_of_impact: 1.0,
                target_distance: body.contact_skin() + obstacle.contact_skin(),
                stop_at_penetration: true,
                ..ShapeCastOptions::default()
            };
            match cast_shapes(
                body.position(),
                movement,
                body.shape(),
                obstacle.position(),
                Vector::ZERO,
                obstacle.shape(),
                options,
            ) {
                Ok(Some(hit)) => {
                    let support_friction = self.floor_contacts.friction_at(
                        body,
                        obstacle,
                        point(obstacle.position().transform_point(hit.witness2)),
                    );
                    // A nearly tangent floor response is free to slide. Limit
                    // only potential self-locking wedges, where the authored
                    // contact friction cones can support opposing forces.
                    let normal = carry_contact_normal(
                        body.position(),
                        body.shape(),
                        movement,
                        obstacle,
                        &hit,
                        body.contact_skin() + obstacle.contact_skin(),
                    );
                    let opposition = normal.dot(direction);

                    let persistent = self
                        .held
                        .as_ref()
                        .is_some_and(|carried| carried.jammed.contains(&pushed));
                    // Static friction persists while both contacts remain
                    // opposed. A tiny solver-driven tip must not release all
                    // accumulated hand lag in a single catch-up tick.
                    if opposition > 0.0
                        && (persistent
                            || opposition >= (hand_friction.atan() + support_friction.atan()).cos())
                    {
                        fraction = fraction.min(hit.time_of_impact.clamp(0.0, 1.0));
                    }
                }
                Err(_) => return 0.0,
                _ => {}
            }
        }
        fraction
    }

    fn recover_carried_pose(
        &self,
        entry: &super::world::BodyEntry,
        start: Pose,
        player: PlayerContext,
        desired: Vec3,
        right: Vec3,
        direction: Vec3,
    ) -> Option<Pose> {
        let center = point(start.translation);
        let clearance = entry.radius + player.half + 0.12;
        let retreat = scaled(direction, -0.04);
        let alternatives = [
            desired,
            add(add(center, scaled(right, clearance)), retreat),
            add(add(center, scaled(right, -clearance)), retreat),
            add(center, Vec3::new(0.0, clearance, 0.0)),
            add(player.eye, scaled(right, clearance)),
            add(player.eye, scaled(right, -clearance)),
        ];
        // The prescribed player is already penetrating the item. Sweep the
        // escape through world geometry, then require a destination clear of
        // that same enabled player capsule and the camera. No collider is disabled.
        let sweep_filter = query_filter()
            .exclude_collider(entry.collider)
            .exclude_rigid_body(self.player);
        let final_filter = query_filter().exclude_collider(entry.collider);
        let options = ShapeCastOptions {
            max_time_of_impact: 1.0,
            target_distance: SWEEP_MARGIN,
            stop_at_penetration: false,
            ..ShapeCastOptions::default()
        };
        for destination in alternatives {
            let movement = subtract(destination, center);
            let travel = self
                .world
                .cast_shape(
                    &start,
                    vector(movement),
                    entry.shape.as_ref(),
                    options,
                    sweep_filter,
                )
                .map_or(1.0, |(_, hit)| hit.time_of_impact.clamp(0.0, 1.0));
            let candidate = Pose::from_parts(
                vector(add(center, scaled(movement, travel))),
                start.rotation,
            );
            if self
                .world
                .intersect_shape(candidate, entry.shape.as_ref(), final_filter)
                .next()
                .is_none()
                && camera_clear(&candidate, &entry.shape, player.eye)
            {
                return Some(candidate);
            }
        }
        None
    }
}
