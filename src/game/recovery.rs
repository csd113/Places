//! Geometry-derived escape detection and a single validated spawn destination.

use super::{
    EYE_HEIGHT, Game, JUMP_APEX_M, LevelDef, LevelSurfaces, PLAYER_HEIGHT, PLAYER_RADIUS,
    PLAYER_STEP_HEIGHT, Vec2, Vec3, WallAabb, WaterVolumes, clamp_sim_delta,
};
use crate::engine::level::WaterShape;

/// A full ordinary jump lasts about 0.9 s. Allow it to cross an open edge or
/// return from a brief unsupported excursion before restoring an escaped body.
const ESCAPE_GRACE_SECONDS: f32 = 1.0;

/// Local room/ladder volumes, preserving gaps and different storey elevations.
/// Water and real solid supports are checked through their live samplers.
#[derive(Debug, Default)]
pub(super) struct PlayableSpace {
    areas: Vec<WallAabb>,
}

impl PlayableSpace {
    pub(super) fn from_level(level: &LevelDef) -> Self {
        let surfaces = LevelSurfaces::new(level);
        let mut areas = Vec::with_capacity(level.rooms.len().saturating_add(level.ladders.len()));
        for room in level.room_iter() {
            let (x0, x1, z0, z1) = room.bounds();
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
                bottom = bottom.min(room.floor_y + stair.offset_y);
                top = top.max(room.floor_y + stair.offset_y + stair.rise);
            }
            areas.push(WallAabb::with_y(
                x0,
                bottom,
                z0,
                x1 - x0,
                top - bottom,
                z1 - z0,
            ));
        }
        for ladder in &level.ladders {
            let (x0, x1, z0, z1) = ladder.bounds();
            areas.push(WallAabb::with_y(
                x0,
                ladder.bottom_y,
                z0,
                x1 - x0,
                ladder.top_y - ladder.bottom_y,
                z1 - z0,
            ));
        }
        Self { areas }
    }

    fn contains(&self, feet: Vec3, water: &WaterVolumes, walls: &[WallAabb]) -> bool {
        let vertical = |bottom: f32, top: f32| {
            feet.y >= bottom - PLAYER_HEIGHT && feet.y <= top + JUMP_APEX_M + PLAYER_STEP_HEIGHT
        };
        self.areas.iter().chain(walls).any(|area| {
            vertical(area.min_y, area.max_y) && area.overlaps_disc(feet.x, feet.z, PLAYER_RADIUS)
        }) || water.volumes().iter().any(|volume| {
            let overlaps = match volume.shape {
                WaterShape::Rect => {
                    let nearest = Vec2::new(
                        feet.x.clamp(volume.x0, volume.x1),
                        feet.z.clamp(volume.z0, volume.z1),
                    );
                    Vec2::new(feet.x, feet.z).distance_squared(nearest)
                        < PLAYER_RADIUS * PLAYER_RADIUS
                }
                WaterShape::Circle => {
                    let (x, z) = volume.center();
                    let radius = volume.radius + PLAYER_RADIUS;
                    Vec2::new(feet.x, feet.z).distance_squared(Vec2::new(x, z)) < radius * radius
                }
            };
            overlaps && vertical(volume.bottom_y, volume.surface_y)
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct RecoveryDestination {
    pub(super) eye: Vec3,
    pub(super) yaw: f32,
}

impl Game {
    /// Whether the body has escaped every locally authored room, water,
    /// ladder or solid support volume. Normal edge jumps receive a grace
    /// interval before automatic recovery; empty diagnostic worlds opt out.
    #[must_use]
    pub fn player_is_out_of_map(&self) -> bool {
        if !self.player_motion_is_finite() {
            return true;
        }
        if self.playable_space.areas.is_empty() && self.water.is_empty() {
            return false;
        }
        !self.playable_feet(Vec3::new(
            self.player_position.x,
            self.feet_y,
            self.player_position.z,
        ))
    }

    pub(super) fn playable_feet(&self, feet: Vec3) -> bool {
        self.playable_space.contains(feet, &self.water, &self.walls)
    }

    pub(super) fn player_motion_is_finite(&self) -> bool {
        self.player_position.is_finite()
            && self.feet_y.is_finite()
            && self.vertical_velocity.is_finite()
            && self.horizontal_velocity.is_finite()
    }

    /// Cache the resolved, clear start instead of repeatedly trying a bad
    /// authored spawn. A closed door may invalidate it later; the destination
    /// selector rechecks clearance before restoring the player.
    pub(super) fn remember_recovery_spawn(&mut self) {
        let feet = Vec3::new(self.player_position.x, self.feet_y, self.player_position.z);
        let has_support = self.support_at(feet.x, feet.z, feet.y).is_some()
            || self.water.sample(feet.x, feet.z, feet.y).is_some();
        self.recovery_spawn = (has_support
            && !self.player_is_out_of_map()
            && self.body_fits_with_height(feet, PLAYER_HEIGHT))
        .then_some(RecoveryDestination {
            eye: Vec3::new(feet.x, feet.y + EYE_HEIGHT, feet.z),
            yaw: self.spawn_yaw,
        });
    }

    /// The single destination policy. Future authored respawn destinations
    /// can be selected here without changing detection or motion reset.
    pub(super) fn recovery_destination(&self) -> Option<RecoveryDestination> {
        let destination = self.recovery_spawn?;
        let feet = Vec3::new(
            destination.eye.x,
            destination.eye.y - EYE_HEIGHT,
            destination.eye.z,
        );
        let has_support = self.support_at(feet.x, feet.z, feet.y).is_some()
            || self.water.sample(feet.x, feet.z, feet.y).is_some();
        (has_support && self.body_fits_with_height(feet, PLAYER_HEIGHT)).then_some(destination)
    }

    pub(super) fn recover_out_of_map(&mut self) -> bool {
        if !self.player_is_out_of_map() {
            self.outside_map_seconds = 0.0;
            return false;
        }
        let finite = self.player_motion_is_finite();
        if finite {
            self.outside_map_seconds += clamp_sim_delta(self.sim_delta_seconds);
            if self.outside_map_seconds < ESCAPE_GRACE_SECONDS {
                return false;
            }
        }
        let Some(destination) = self.recovery_destination() else {
            crate::engine::logging::warn_once(
                "movement_no_recovery_spawn",
                "[movement] escaped map has no clear spawn destination; correct the level spawn",
            );
            return false;
        };
        if self.movement_debug {
            crate::engine::logging::warn(format_args!(
                "[movement] out_of_map_recovery initial={:?} destination={:?}",
                self.player_position, destination.eye
            ));
        }
        // Reset only the player: doors, switches and AI retain their state.
        // Reseeding volume membership prevents a cross-map trigger sweep.
        self.clear_run_state(destination.eye, destination.yaw, true);
        self.reseed_trigger_inside();
        self.reset_count = self.reset_count.saturating_add(1);
        true
    }
}
