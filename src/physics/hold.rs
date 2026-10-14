//! Collision-aware carrying and safe release, with no teleport-derived velocity.

use glam::{Quat, Vec3};
use rapier3d::parry::query::{NonlinearRigidMotion, ShapeCastOptions, distance, intersection_test};
use rapier3d::parry::shape::Ball;
use rapier3d::prelude::{
    ColliderHandle, Pose, QueryFilter, QueryFilterFlags, Ray, RigidBodyType, SharedShape, Vector,
};

use crate::collision::PLAYER_RADIUS;

use super::collision::{add, point, pose, query_filter, scaled, subtract, vector};
use super::world::{ItemWorld, PlayerContext, Runtime};
use super::{FIXED_STEP, HOLD_DISTANCE, PICKUP_REACH};

const HOLD_SPEED: f32 = 4.0;
const SWEEP_MARGIN: f32 = 0.006;
const CAMERA_RADIUS: f32 = 0.10;

fn carry_filter(collider: ColliderHandle) -> QueryFilter<'static> {
    let mut filter = query_filter().exclude_collider(collider);
    // Controlled carrying pushes active rigid items through Rapier contacts.
    // Fixed scenery, dormant bodies, doors and the player remain sweep obstacles.
    filter.flags |= QueryFilterFlags::EXCLUDE_DYNAMIC;
    filter
}

pub(super) struct HeldItem {
    pub key: u64,
    relative_rotation: Quat,
    route: Option<CarryRoute>,
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
    let clearance = PLAYER_RADIUS + radius + 0.12;
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

impl ItemWorld {
    /// Sweeps ordinary planar controller movement against the actual held
    /// shape. Vertical controller output stays unchanged. Existing overlap
    /// from a developer teleport is left for geometric carry recovery.
    #[must_use]
    pub fn constrain_player_eye(&self, player: PlayerContext) -> Vec3 {
        let Some(runtime) = self.runtime.as_ref() else {
            return player.eye;
        };
        let Some(previous) = runtime.last_player else {
            return player.eye;
        };
        let Some(held) = runtime.held.as_ref() else {
            return player.eye;
        };
        let Some(entry) = runtime.entries.get(&held.key) else {
            return player.eye;
        };
        if !player.active
            || !previous.active
            || !player.eye.is_finite()
            || !player.feet.is_finite()
            || !player.height.is_finite()
            || player.height <= PLAYER_RADIUS * 2.0
        {
            return player.eye;
        }
        let center_y = player.height.mul_add(0.5, player.feet.y);
        let start = pose(
            Vec3::new(previous.feet.x, center_y, previous.feet.z),
            Quat::IDENTITY,
        );
        let motion = Vec3::new(
            player.feet.x - previous.feet.x,
            0.0,
            player.feet.z - previous.feet.z,
        );
        let capsule =
            SharedShape::capsule_y(player.height.mul_add(0.5, -PLAYER_RADIUS), PLAYER_RADIUS);
        let only_held = |handle, collider: &rapier3d::prelude::Collider| {
            handle == entry.collider && collider.is_enabled()
        };
        let filter = query_filter().predicate(&only_held);
        if runtime
            .world
            .intersect_shape(start, capsule.as_ref(), filter)
            .next()
            .is_some()
        {
            return player.eye;
        }
        let options = ShapeCastOptions {
            max_time_of_impact: 1.0,
            target_distance: SWEEP_MARGIN,
            stop_at_penetration: false,
            ..ShapeCastOptions::default()
        };
        let travel = runtime
            .world
            .cast_shape(&start, vector(motion), capsule.as_ref(), options, filter)
            .map_or(1.0, |(_, hit)| hit.time_of_impact.clamp(0.0, 1.0));
        Vec3::new(
            motion.x.mul_add(travel, previous.feet.x),
            player.eye.y,
            motion.z.mul_add(travel, previous.feet.z),
        )
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
                    runtime
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
        });
        true
    }

    /// Releases from the latest checked carry pose. A newly overlapping wall,
    /// door, player or ceiling keeps the item held and returns false.
    /// Velocity comes only from bounded player translation, never hand teleports.
    pub fn drop(&mut self) -> bool {
        let Some(runtime) = self.runtime.as_mut() else {
            return false;
        };
        let Some(held) = runtime.held.as_ref() else {
            return false;
        };
        let key = held.key;
        runtime.refresh_queries();
        let Some(entry) = runtime.entries.get_mut(&key) else {
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
            .next()
            .is_some()
        {
            return false;
        }
        let Some(released_body) = runtime.world.bodies.get_mut(entry.body) else {
            return false;
        };
        released_body.set_body_type(RigidBodyType::Dynamic, true);
        released_body.set_linvel(vector(runtime.player_velocity.clamp_length_max(2.0)), true);
        // Only actual continuously swept hand rotation is transferred. A
        // camera snap cannot create an unbounded synthetic release impulse.
        released_body.set_angvel(released_body.angvel().clamp_length_max(3.0), true);
        released_body.set_next_kinematic_position(candidate);
        entry.previous = entry.current;
        runtime.held = None;
        true
    }

    /// Safely releases and applies a forward physical impulse. Bodies up to
    /// one kilogram leave at 8 m/s; heavier bodies receive `8 / sqrt(mass)`.
    /// Invalid aim or an obstructed release keeps the held state intact.
    pub fn throw(&mut self, direction: Vec3) -> bool {
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
        if !self.drop() {
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
        true
    }
}

impl Runtime {
    pub(super) fn advance_held(&mut self, player: PlayerContext) {
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
                return;
            }
        }
        let (target, route) = self.routed_carried_pose(
            entry,
            start,
            desired_center,
            desired_rotation,
            player,
            held.route,
        );
        let body_handle = entry.body;
        if let Some(current_held) = self.held.as_mut() {
            current_held.route = route;
        }
        if let Some(held_body) = self.world.bodies.get_mut(body_handle) {
            held_body.set_next_kinematic_position(target);
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
            return (
                self.swept_carried_pose(entry, start, desired, rotation, player.eye)
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
                self.swept_carried_pose(entry, start, waypoint, rotation, player.eye)
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
        crosses_capsule || nearest.distance_squared(player.eye) < camera_clearance.powi(2)
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
        let span = PLAYER_RADIUS + entry.radius + 0.1;
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
                    carry_filter(entry.collider),
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
        let filter = carry_filter(entry.collider).exclude_rigid_body(self.player);
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
    ) -> Option<Pose> {
        let motion_delta = subtract(desired_center, point(start.translation))
            .clamp_length_max(HOLD_SPEED * FIXED_STEP);
        let filter = carry_filter(entry.collider);
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
            .clamp_length_max(8.0 * FIXED_STEP);
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
        let accepted = self
            .world
            .intersect_shape(candidate, entry.shape.as_ref(), filter)
            .next()
            .is_none()
            && camera_clear(&candidate, &entry.shape, eye);
        accepted.then_some(candidate)
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
        let clearance = entry.radius + PLAYER_RADIUS + 0.12;
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
