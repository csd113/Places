//! Developer movement and validated rigid-prop placement. Ordinary walking
//! and void recovery retain their own paths.
use super::{EYE_HEIGHT, Game, PLAYER_HEIGHT, PLAYER_RADIUS, Vec2, Vec3};
use crate::input::{Control, InputState};
use crate::settings::Settings;

#[derive(Clone, Copy)]
pub(super) struct ReturnPose {
    eye: Vec3,
    yaw: f32,
    pitch: f32,
}

impl Game {
    #[must_use]
    pub const fn noclip_enabled(&self) -> bool {
        self.noclip.is_some()
    }

    /// Switch modes with validated standing-body recovery, preserving world
    /// entities. A malformed world without any safe destination refuses exit.
    /// # Errors
    /// Returns a clear error if no safe walking destination exists.
    pub fn set_noclip(&mut self, enabled: bool) -> Result<&'static str, String> {
        if enabled == self.noclip_enabled() {
            return Ok(if enabled { "Noclip ON." } else { "Noclip OFF." });
        }
        if enabled {
            let pose = ReturnPose {
                eye: Vec3::new(
                    self.player_position.x,
                    self.feet_y + EYE_HEIGHT,
                    self.player_position.z,
                ),
                yaw: self.player_yaw,
                pitch: self.player_pitch,
            };
            self.noclip = Some(pose);
            self.vertical_velocity = 0.0;
            self.horizontal_velocity = Vec2::ZERO;
            self.grounded = false;
            self.interact_pressed = false;
            return Ok("Noclip ON. Move/look normally; hold Jump up, Crouch down.");
        }
        let current = ReturnPose {
            eye: self.player_position,
            yaw: self.player_yaw,
            pitch: self.player_pitch,
        };
        let current_feet = Vec3::new(current.eye.x, self.feet_y, current.eye.z);
        let keep_current = self.safe_walking_feet(current_feet);
        let destination = if keep_current {
            Some(ReturnPose { eye: Vec3::new(current_feet.x, current_feet.y + EYE_HEIGHT, current_feet.z), ..current })
        } else {
            self.noclip.filter(|pose| self.safe_walking_feet(Vec3::new(pose.eye.x, pose.eye.y - EYE_HEIGHT, pose.eye.z)))
                .or_else(|| self.recovery_destination().map(|spawn| ReturnPose { eye: spawn.eye, yaw: spawn.yaw, pitch: 0.0 }))
        }.ok_or_else(|| "Cannot exit noclip: no clear return pose or validated spawn. Move into a clear room and retry.".to_string())?;
        self.clear_run_state(destination.eye, destination.yaw, true);
        self.player_pitch = destination.pitch;
        self.reseed_trigger_inside();
        Ok(if keep_current {
            "Noclip OFF. Walking restored here."
        } else {
            "Noclip OFF. Returned to a validated safe pose."
        })
    }

    fn safe_walking_feet(&self, feet: Vec3) -> bool {
        feet.is_finite()
            && self.body_fits_with_height(feet, PLAYER_HEIGHT)
            && self.playable_feet(feet)
    }

    pub(super) fn update_noclip(
        &mut self,
        input: &InputState,
        settings: &Settings,
        motion: (f32, f32),
    ) {
        let delta = super::clamp_sim_delta(self.sim_delta_seconds);
        self.update_look(input, settings, motion, delta);
        self.update_doors(delta);
        let forward = self.view_direction();
        let right = Vec3::new(self.player_yaw.cos(), 0.0, self.player_yaw.sin());
        let mut direction = Vec3::ZERO;
        for (control, vector) in [
            (Control::MoveForward, forward),
            (
                Control::MoveBackward,
                Vec3::new(-forward.x, -forward.y, -forward.z),
            ),
            (Control::StrafeRight, right),
            (Control::StrafeLeft, Vec3::new(-right.x, -right.y, -right.z)),
            (Control::Jump, Vec3::Y),
            (Control::Crouch, -Vec3::Y),
        ] {
            if input.is_held(control) {
                direction = Vec3::new(
                    direction.x + vector.x,
                    direction.y + vector.y,
                    direction.z + vector.z,
                );
            }
        }
        let unit = direction.normalize_or_zero();
        let distance = settings.walk_speed * delta;
        let next = Vec3::new(
            unit.x.mul_add(distance, self.player_position.x),
            unit.y.mul_add(distance, self.player_position.y),
            unit.z.mul_add(distance, self.player_position.z),
        );
        // Keep coordinates usable by the renderer and recovery queries.
        if next.is_finite() && next.abs().max_element() <= 100_000.0 {
            self.player_position = next;
            self.feet_y = next.y - self.eye_offset_current;
        }
        self.reseed_trigger_inside();
        let feet = Vec3::new(self.player_position.x, self.feet_y, self.player_position.z);
        self.update_world(feet);
    }

    /// Conservative flat-floor placement of a small rigid prop's actual model
    /// bounds. Uses existing collision/ceiling queries, including live doors.
    #[must_use]
    pub fn developer_prop_position(&self, min: Vec3, max: Vec3) -> Option<Vec3> {
        if self.noclip_enabled() || !self.player_motion_is_finite() {
            return None;
        }
        let extent = Vec3::new(max.x - min.x, max.y - min.y, max.z - min.z);
        let half = Vec3::new(extent.x * 0.5, extent.y * 0.5, extent.z * 0.5);
        let radius = Vec2::new(half.x, half.z).length();
        let distance = radius + PLAYER_RADIUS + 1.0;
        let centre = Vec3::new(
            self.player_yaw
                .sin()
                .mul_add(distance, self.player_position.x),
            self.player_position.y,
            (-self.player_yaw.cos()).mul_add(distance, self.player_position.z),
        );
        let y = self
            .floor
            .height_below(centre.x, centre.z, self.feet_y + 0.4)?;
        if (y - self.feet_y).abs() > 0.4 {
            return None;
        }
        for x in [centre.x - radius, centre.x + radius] {
            for z in [centre.z - radius, centre.z + radius] {
                if self
                    .floor
                    .height_below(x, z, y + 0.001)
                    .is_none_or(|height| (height - y).abs() > 0.001)
                {
                    return None;
                }
            }
        }
        let mut clear = true;
        self.ceiling.for_each_body_contact(
            Vec2::new(centre.x, centre.z),
            radius,
            y,
            extent.y,
            |_| clear = false,
        );
        self.collision_index
            .for_each_disc(centre.x, centre.z, radius, &self.walls, |wall| {
                if wall.max_y > y + 0.001
                    && wall.min_y < y + extent.y
                    && wall.overlaps_disc(centre.x, centre.z, radius)
                {
                    clear = false;
                }
            });
        if self.world.door_colliders().iter().any(|door| {
            door.overlaps_body_y(y, extent.y) && door.overlaps_disc(centre.x, centre.z, radius)
        }) {
            clear = false;
        }
        clear.then_some(Vec3::new(
            centre.x - min.x.midpoint(max.x),
            y - min.y,
            centre.z - min.z.midpoint(max.z),
        ))
    }
}
