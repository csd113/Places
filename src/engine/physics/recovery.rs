//! Geometry-derived prop escape recovery. Collision prevention remains separate.

use std::collections::VecDeque;

use glam::{Vec2, Vec3};
use rapier3d::parry::query::contact;
use rapier3d::prelude::{Ray, RigidBody, RigidBodyType, Vector};
use serde::Serialize;

use crate::engine::collision::WallAabb;
use crate::engine::level::{LevelDef, LevelSurfaces, WalkableFloor, WaterShape, WaterVolumes};

use super::MAX_BODIES;
use super::collision::{add, point, pose, quaternion, query_filter, subtract};
use super::config::BodyMotion;
use super::world::{BodyEntry, ItemPose, ItemWorld, Runtime};

/// Match the player's ordinary unsupported-excursion grace interval.
const ESCAPE_GRACE_SECONDS: f32 = 1.0;
/// Recovery starts once the whole body is beyond a local vertical extent.
const ESCAPE_VERTICAL_MARGIN: f32 = 2.0;
const MAX_RECOVERY_EVENTS: usize = 32;
const MAX_DESTINATION_TESTS: usize = 30;
const SUPPORT_REACH: f32 = 100.0;
const PLACEMENT_GAP: f32 = 0.012;
const RECOVERY_OFFSETS: [Vec2; 9] = [
    Vec2::ZERO,
    Vec2::X,
    Vec2::NEG_X,
    Vec2::Y,
    Vec2::NEG_Y,
    Vec2::ONE,
    Vec2::NEG_ONE,
    Vec2::new(-1.0, 1.0),
    Vec2::new(1.0, -1.0),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EscapeReason {
    NonFiniteMotion,
    OutsideCoverage,
    BelowPlayableSpace,
    AbovePlayableSpace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryOutcome {
    Restored,
    /// No supported, clear destination existed. An explicit reset can retry.
    Suspended,
}

/// Bounded diagnostic evidence retained before invalid motion is cleared.
/// Non-finite fields are represented by `None`, so JSON remains valid.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct RecoveryEvent {
    pub key: u64,
    pub reason: EscapeReason,
    pub outcome: RecoveryOutcome,
    pub escaped_pose: Option<ItemPose>,
    pub escaped_linear_velocity: Option<[f32; 3]>,
    pub escaped_angular_velocity: Option<[f32; 3]>,
    /// The prescribed kinematic destination was invalid before solving.
    pub invalid_next_pose: bool,
    pub destination: Option<ItemPose>,
    pub released_held: bool,
    pub outside_seconds: f32,
    pub candidate_tests: usize,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct RecoveryTracker {
    last_safe_pose: Option<ItemPose>,
    area: Option<usize>,
    outside_seconds: f32,
    pub suspended: bool,
}

impl RecoveryTracker {
    pub(super) const fn new(_initial: ItemPose) -> Self {
        Self {
            last_safe_pose: None,
            area: None,
            outside_seconds: 0.0,
            suspended: false,
        }
    }

    pub(super) const fn reset(&mut self, initial: ItemPose) {
        *self = Self::new(initial);
    }
}

pub(super) struct RecoveryState {
    events: VecDeque<RecoveryEvent>,
    requests: Vec<RecoveryRequest>,
}

#[derive(Clone, Copy)]
struct RecoveryRequest {
    key: u64,
    reason: EscapeReason,
    escaped_pose: Option<ItemPose>,
    linear: Option<[f32; 3]>,
    angular: Option<[f32; 3]>,
    invalid_next_pose: bool,
    outside_seconds: f32,
}

impl Default for RecoveryState {
    fn default() -> Self {
        Self {
            events: VecDeque::with_capacity(MAX_RECOVERY_EVENTS),
            requests: Vec::with_capacity(MAX_BODIES),
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum Footprint {
    Rect { minimum: Vec2, maximum: Vec2 },
    Circle { center: Vec2, radius: f32 },
}

impl Footprint {
    fn overlaps(self, center: Vec3, radius: f32) -> bool {
        let horizontal = Vec2::new(center.x, center.z);
        match self {
            Self::Rect { minimum, maximum } => {
                horizontal.distance_squared(horizontal.clamp(minimum, maximum)) <= radius * radius
            }
            Self::Circle {
                center: area_center,
                radius: area_radius,
            } => {
                let combined = area_radius + radius;
                horizontal.distance_squared(area_center) <= combined * combined
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct RecoveryArea {
    footprint: Footprint,
    bottom: f32,
    top: Option<f32>,
}

impl RecoveryArea {
    fn contains(self, center: Vec3, radius: f32) -> bool {
        self.footprint.overlaps(center, radius)
            && center.y + radius >= self.bottom - ESCAPE_VERTICAL_MARGIN
            && self
                .top
                .is_none_or(|top| center.y - radius <= top + ESCAPE_VERTICAL_MARGIN)
    }
}

/// Local authored footprint union, with cached membership for steady bodies.
/// Open sky has no artificial ceiling; thrown items can return ballistically.
#[derive(Debug, Default)]
pub(super) struct RecoverySpace {
    areas: Vec<RecoveryArea>,
    floor: WalkableFloor,
}

const fn rectangular_area(
    bounds: (f32, f32, f32, f32),
    bottom: f32,
    top: Option<f32>,
) -> RecoveryArea {
    let (x0, x1, z0, z1) = bounds;
    RecoveryArea {
        footprint: Footprint::Rect {
            minimum: Vec2::new(x0, z0),
            maximum: Vec2::new(x1, z1),
        },
        bottom,
        top,
    }
}

impl RecoverySpace {
    pub(super) fn from_level(level: &LevelDef, walls: &[WallAabb]) -> Self {
        let surfaces = LevelSurfaces::new(level);
        let mut areas = Vec::new();
        for room in level.room_iter() {
            let mut bottom = room.floor_y;
            let mut top = room.ridge_y().unwrap_or_else(|| room.eave_y());
            for region in surfaces.regions_for_room(room) {
                let height = room.floor_y + region.offset();
                bottom = bottom.min(height);
                top = top.max(height);
            }
            for ramp in surfaces.ramps_for_room(room) {
                bottom = bottom.min(room.floor_y + ramp.low_offset());
                top = top.max(room.floor_y + ramp.high_offset());
            }
            for stair in surfaces.stairs_for_room(room) {
                bottom = bottom.min(room.floor_y + stair.offset_y + stair.rise.min(0.0));
                top = top.max(room.floor_y + stair.offset_y + stair.rise.max(0.0));
            }
            areas.push(rectangular_area(
                room.bounds(),
                bottom,
                (!room.ceiling.is_open()).then_some(top),
            ));
        }
        for ladder in &level.ladders {
            areas.push(rectangular_area(
                ladder.bounds(),
                ladder.bottom_y,
                Some(ladder.top_y),
            ));
        }
        for wall in walls {
            areas.push(rectangular_area(
                (wall.min_x, wall.max_x, wall.min_z, wall.max_z),
                wall.min_y,
                Some(wall.max_y),
            ));
        }
        for water in WaterVolumes::from_level(level).volumes() {
            let footprint = match water.shape {
                WaterShape::Rect => Footprint::Rect {
                    minimum: Vec2::new(water.x0, water.z0),
                    maximum: Vec2::new(water.x1, water.z1),
                },
                WaterShape::Circle => {
                    let (x, z) = water.center();
                    Footprint::Circle {
                        center: Vec2::new(x, z),
                        radius: water.radius,
                    }
                }
            };
            areas.push(RecoveryArea {
                footprint,
                bottom: water.bottom_y,
                top: Some(water.surface_y),
            });
        }
        Self {
            areas,
            floor: WalkableFloor::from_level(level),
        }
    }

    fn membership(&self, center: Vec3, radius: f32, cached: Option<usize>) -> Option<usize> {
        if let Some(index) = cached
            && self
                .areas
                .get(index)
                .is_some_and(|area| area.contains(center, radius))
        {
            return Some(index);
        }
        self.areas
            .iter()
            .position(|area| area.contains(center, radius))
    }

    fn reason(&self, center: Vec3, radius: f32) -> EscapeReason {
        let overlaps = self
            .areas
            .iter()
            .any(|area| area.footprint.overlaps(center, radius));
        if !overlaps {
            return EscapeReason::OutsideCoverage;
        }
        let above = self
            .areas
            .iter()
            .filter(|area| area.footprint.overlaps(center, radius))
            .all(|area| {
                area.top
                    .is_some_and(|top| center.y - radius > top + ESCAPE_VERTICAL_MARGIN)
            });
        if above {
            EscapeReason::AbovePlayableSpace
        } else {
            EscapeReason::BelowPlayableSpace
        }
    }
}

fn finite_pose(value: ItemPose) -> bool {
    value.position.is_finite()
        && value.position.abs().max_element() <= 100_000.0
        && value.rotation.is_finite()
        && (0.999..=1.001).contains(&value.rotation.length_squared())
}

fn finite_motion_vector(value: Vector) -> bool {
    value.is_finite() && value.length_squared().is_finite()
}

fn finite_next_pose(body: &RigidBody) -> bool {
    let next = body.next_position();
    let rotation = quaternion(next.rotation);
    point(next.translation).is_finite()
        && rotation.is_finite()
        && (0.999..=1.001).contains(&rotation.length_squared())
}

fn observed_pose(entry: &BodyEntry, body: &RigidBody) -> ItemPose {
    let rotation = quaternion(*body.rotation());
    ItemPose {
        position: subtract(point(body.translation()), rotation.mul_vec3(entry.center)),
        rotation,
    }
}

impl ItemWorld {
    /// Drains at most 32 escape records. Ordinary contacts never enqueue one.
    #[must_use]
    pub fn take_recoveries(&mut self) -> Vec<RecoveryEvent> {
        self.runtime.as_mut().map_or_else(Vec::new, |runtime| {
            runtime.recovery.events.drain(..).collect()
        })
    }
}

impl Runtime {
    /// Call once before solving with zero age, then after solving with the
    /// actual fixed interval. Invalid motion never enters Rapier's solver.
    pub(super) fn recover_escaped(&mut self, space: &RecoverySpace, interval: f32) -> usize {
        let mut requests = std::mem::take(&mut self.recovery.requests);
        requests.clear();
        for (&key, entry) in &mut self.entries {
            if !entry.motion.is_movable() || entry.dormant || entry.recovery.suspended {
                continue;
            }
            let Some(body) = self.world.bodies.get(entry.body) else {
                continue;
            };
            let actual = observed_pose(entry, body);
            let motion_finite = finite_pose(actual)
                && finite_next_pose(body)
                && point(body.translation()).is_finite()
                && finite_motion_vector(body.linvel())
                && finite_motion_vector(body.angvel())
                && finite_motion_vector(body.user_force())
                && finite_motion_vector(body.user_torque());
            if !motion_finite {
                requests.push(RecoveryRequest {
                    key,
                    reason: EscapeReason::NonFiniteMotion,
                    escaped_pose: finite_pose(actual).then_some(actual),
                    linear: point(body.linvel())
                        .is_finite()
                        .then_some(point(body.linvel()).to_array()),
                    angular: point(body.angvel())
                        .is_finite()
                        .then_some(point(body.angvel()).to_array()),
                    invalid_next_pose: !finite_next_pose(body),
                    outside_seconds: entry.recovery.outside_seconds,
                });
                continue;
            }
            if space.areas.is_empty() {
                // Match the player's finite empty-world diagnostic opt-out.
                entry.recovery.outside_seconds = 0.0;
                continue;
            }
            let center = point(body.translation());
            let member = space.membership(center, entry.radius, entry.recovery.area);
            if let Some(index) = member {
                entry.recovery.area = Some(index);
                entry.recovery.outside_seconds = 0.0;
                // No shape query here. A recovery destination is revalidated
                // against its complete collider and current actors only on escape.
                let bottom = entry.shape.compute_aabb(body.position()).mins.y;
                if space
                    .areas
                    .get(index)
                    .is_some_and(|area| bottom >= area.bottom - 0.002)
                {
                    entry.recovery.last_safe_pose = Some(actual);
                }
            } else {
                entry.recovery.area = None;
                entry.recovery.outside_seconds += interval;
                if entry.recovery.outside_seconds >= ESCAPE_GRACE_SECONDS {
                    requests.push(RecoveryRequest {
                        key,
                        reason: space.reason(center, entry.radius),
                        escaped_pose: Some(actual),
                        linear: Some(point(body.linvel()).to_array()),
                        angular: Some(point(body.angvel()).to_array()),
                        invalid_next_pose: false,
                        outside_seconds: entry.recovery.outside_seconds,
                    });
                }
            }
        }
        // Invalid poses must never reach the broad phase either. Capture their
        // evidence first, then temporarily disable and sanitize before queries.
        for request in &requests {
            if request.reason == EscapeReason::NonFiniteMotion {
                self.sanitize_invalid_motion(request.key);
            }
        }
        if !requests.is_empty() {
            self.refresh_queries();
        }
        let mut restored = 0_usize;
        for &request in &requests {
            if self.recover_one(request, space) {
                restored = restored.saturating_add(1);
            }
        }
        self.recovery.requests = requests;
        restored
    }

    fn sanitize_invalid_motion(&mut self, key: u64) {
        let Some(entry) = self.entries.get_mut(&key) else {
            return;
        };
        let safe = entry
            .recovery
            .last_safe_pose
            .filter(|value| finite_pose(*value))
            .unwrap_or(entry.authored_pose);
        let target = pose(
            add(safe.position, safe.rotation.mul_vec3(entry.center)),
            safe.rotation,
        );
        if let Some(body) = self.world.bodies.get_mut(entry.body) {
            body.set_enabled(false);
            // KinematicPositionBased ignores velocity setters. sleep clears
            // the actual velocity storage for every controlled body type.
            body.sleep();
            body.set_position(target, false);
            body.set_next_kinematic_position(target);
            body.set_linvel(Vector::ZERO, false);
            body.set_angvel(Vector::ZERO, false);
            body.reset_forces(false);
            body.reset_torques(false);
        }
        if let Some(collider) = self.world.colliders.get_mut(entry.collider) {
            collider.set_enabled(false);
        }
        entry.previous = safe;
        entry.current = safe;
    }

    fn recover_one(&mut self, request: RecoveryRequest, space: &RecoverySpace) -> bool {
        let key = request.key;
        let released_held = self.held.as_ref().is_some_and(|held| held.key == key);
        let (destination, candidate_tests) = self.recovery_destination(key, space);
        let restored = destination.is_some();
        self.apply_recovery(key, destination);
        if released_held {
            self.held = None;
        }
        self.clear_item_contacts(key);
        self.refresh_queries();
        if self.recovery.events.len() >= MAX_RECOVERY_EVENTS {
            let _oldest = self.recovery.events.pop_front();
        }
        self.recovery.events.push_back(RecoveryEvent {
            key,
            reason: request.reason,
            outcome: if restored {
                RecoveryOutcome::Restored
            } else {
                RecoveryOutcome::Suspended
            },
            escaped_pose: request.escaped_pose,
            escaped_linear_velocity: request.linear,
            escaped_angular_velocity: request.angular,
            invalid_next_pose: request.invalid_next_pose,
            destination,
            released_held,
            outside_seconds: request.outside_seconds,
            candidate_tests,
        });
        restored
    }

    fn recovery_destination(&self, key: u64, space: &RecoverySpace) -> (Option<ItemPose>, usize) {
        let Some(entry) = self.entries.get(&key) else {
            return (None, 0);
        };
        let player_anchor = self.last_player.map(|player| ItemPose {
            position: player.feet,
            rotation: entry.authored_pose.rotation,
        });
        let anchors = [
            Some(entry.authored_pose),
            entry.recovery.last_safe_pose,
            player_anchor,
        ];
        let mut tests = 0_usize;
        for candidate in anchors.into_iter().flatten() {
            tests = tests.saturating_add(1);
            if self.recovery_destination_clear(key, candidate, space) {
                return (Some(candidate), tests);
            }
        }
        // Nine offsets per anchor, with no unbounded nav/floor search. The
        // complete physical shape, not the GLB or a visual lift, determines Y.
        let stride = (entry.radius.mul_add(2.0, 0.35)).max(0.5);
        for anchor in anchors
            .into_iter()
            .flatten()
            .filter(|anchor| finite_pose(*anchor))
        {
            for offset in RECOVERY_OFFSETS {
                if tests >= MAX_DESTINATION_TESTS {
                    return (None, tests);
                }
                let x = offset.x.mul_add(stride, anchor.position.x);
                let z = offset.y.mul_add(stride, anchor.position.z);
                let floor_candidate = space
                    .floor
                    .height_below(x, z, anchor.position.y + 4.0)
                    .or_else(|| space.floor.height_at(x, z));
                let Some(floor_height) = floor_candidate else {
                    continue;
                };
                let at_zero = pose(anchor.rotation.mul_vec3(entry.center), anchor.rotation);
                let bottom = entry.shape.compute_aabb(&at_zero).mins.y;
                let candidate = ItemPose {
                    position: Vec3::new(x, floor_height - bottom + PLACEMENT_GAP, z),
                    rotation: anchor.rotation,
                };
                tests = tests.saturating_add(1);
                if self.recovery_destination_clear(key, candidate, space) {
                    return (Some(candidate), tests);
                }
            }
        }
        (None, tests)
    }

    fn recovery_destination_clear(
        &self,
        key: u64,
        candidate: ItemPose,
        space: &RecoverySpace,
    ) -> bool {
        if !finite_pose(candidate) {
            return false;
        }
        let Some(entry) = self.entries.get(&key) else {
            return false;
        };
        let center = add(
            candidate.position,
            candidate.rotation.mul_vec3(entry.center),
        );
        if !space.areas.is_empty() && space.membership(center, entry.radius, None).is_none() {
            return false;
        }
        let target = pose(center, candidate.rotation);
        let filter = query_filter().exclude_collider(entry.collider);
        for (_, collider) in self
            .world
            .intersect_shape(target, entry.shape.as_ref(), filter)
        {
            let tolerance = if collider.parent().is_none() {
                0.002
            } else {
                1.0e-5
            };
            let clear = contact(
                &target,
                entry.shape.as_ref(),
                collider.position(),
                collider.shape(),
                0.0,
            )
            .is_ok_and(|contact| contact.is_none_or(|point| point.dist >= -tolerance));
            if !clear {
                return false;
            }
        }
        let bounds = entry.shape.compute_aabb(&target);
        let ray = Ray::new(
            Vector::new(center.x, bounds.mins.y + PLACEMENT_GAP, center.z),
            -Vector::Y,
        );
        let static_support = |handle, collider: &rapier3d::prelude::Collider| {
            handle != entry.collider
                && collider.is_enabled()
                && collider.parent().is_none_or(|parent| {
                    self.world
                        .bodies
                        .get(parent)
                        .is_some_and(RigidBody::is_fixed)
                        && self.key_for_collider(handle).is_none_or(|item_key| {
                            self.entries
                                .get(&item_key)
                                .is_some_and(|item| item.motion == BodyMotion::Fixed)
                        })
                })
        };
        self.world
            .cast_ray_and_get_normal(
                &ray,
                SUPPORT_REACH,
                true,
                query_filter().predicate(&static_support),
            )
            .is_some_and(|(_, hit)| hit.normal.y > 0.3)
    }

    fn apply_recovery(&mut self, key: u64, destination: Option<ItemPose>) {
        let Some(entry) = self.entries.get_mut(&key) else {
            return;
        };
        let fallback = if finite_pose(entry.current) {
            entry.current
        } else {
            entry
                .recovery
                .last_safe_pose
                .filter(|value| finite_pose(*value))
                .unwrap_or(entry.authored_pose)
        };
        let target = destination.unwrap_or(fallback);
        let suspended = destination.is_none();
        entry.dormant = !suspended && entry.authored && target == entry.authored_pose;
        let motion = if entry.dormant {
            RigidBodyType::Fixed
        } else {
            match entry.motion {
                BodyMotion::Fixed => RigidBodyType::Fixed,
                BodyMotion::Dynamic => RigidBodyType::Dynamic,
                BodyMotion::Kinematic => RigidBodyType::KinematicPositionBased,
            }
        };
        let physical_pose = pose(
            add(target.position, target.rotation.mul_vec3(entry.center)),
            target.rotation,
        );
        if let Some(body) = self.world.bodies.get_mut(entry.body) {
            body.set_body_type(motion, true);
            body.set_position(physical_pose, true);
            body.set_next_kinematic_position(physical_pose);
            body.set_linvel(Vector::ZERO, true);
            body.set_angvel(Vector::ZERO, true);
            body.reset_forces(true);
            body.reset_torques(true);
            body.set_enabled(!suspended);
            // Clear actual kinematic velocity too; those bodies ignore the
            // public set_linvel/set_angvel methods. Valid active bodies wake below.
            body.sleep();
        }
        if let Some(collider) = self.world.colliders.get_mut(entry.collider) {
            collider.set_enabled(!suspended);
        }
        entry.previous = target;
        entry.current = target;
        entry.recovery.reset(target);
        entry.recovery.suspended = suspended;
        if !entry.dormant && !suspended {
            self.world.wake_up(entry.body, true);
        }
    }
}
