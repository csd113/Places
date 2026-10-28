//! Conservative bounded third-person camera sweep using live world collision.
use super::Game;
use glam::{Vec2, Vec3};

impl Game {
    /// First-person step presentation, constrained by the same live world as
    /// the third-person boom. Physics, triggers and stance retain physical feet.
    #[must_use]
    pub fn view_position(&self) -> Vec3 {
        if self.step_eye_lag.abs() <= f32::EPSILON {
            return self.player_position;
        }
        self.camera_sweep(
            self.player_position,
            Vec3::new(
                self.player_position.x,
                self.player_position.y + self.step_eye_lag,
                self.player_position.z,
            ),
            0.1,
        )
    }

    pub(super) fn advance_step_eye(&mut self, previous_feet: f32, previously_grounded: bool) {
        let rise = self.feet_y() - previous_feet;
        if !previously_grounded
            || !self.grounded
            || self.swimming
            || self.climbing.is_some()
            || self.player_shape.is_some()
        {
            self.step_eye_lag = 0.0;
            return;
        }
        // Constant catch-up speed settles fully on a landing or when stopped;
        // no asymptotic drift, velocity overshoot or delayed fall is possible.
        let lag = (self.step_eye_lag - rise).clamp(
            -2.0 * super::PLAYER_STEP_HEIGHT,
            2.0 * super::PLAYER_STEP_HEIGHT,
        );
        let catch_up = 3.0 * super::VERTICAL_SUBSTEP;
        self.step_eye_lag = lag.signum() * (lag.abs() - catch_up).max(0.0);
    }

    /// Resolves a camera sphere along the boom. Sampling every <=5 cm with a
    /// 10 cm minimum sphere radius conservatively catches thin wall crossings.
    #[must_use]
    pub fn camera_sweep(&self, start: Vec3, end: Vec3, radius: f32) -> Vec3 {
        if !start.is_finite() || !end.is_finite() || !radius.is_finite() {
            return start;
        }
        let bounded_radius = radius.clamp(0.01, 0.5);
        let direction = Vec3::new(end.x - start.x, end.y - start.y, end.z - start.z);
        let distance = direction.length().min(8.0);
        let unit = direction.normalize_or_zero();
        let mut clear = start;
        for index in 0_u16..=160 {
            let travel = (f32::from(index) * 0.05).min(distance);
            let candidate = Vec3::new(
                unit.x.mul_add(travel, start.x),
                unit.y.mul_add(travel, start.y),
                unit.z.mul_add(travel, start.z),
            );
            if !self.camera_sphere_clear(candidate, bounded_radius) {
                break;
            }
            clear = candidate;
            if travel >= distance {
                break;
            }
        }
        clear
    }

    fn camera_sphere_clear(&self, point: Vec3, radius: f32) -> bool {
        let mut clear = true;
        self.ceiling.for_each_body_contact(
            Vec2::new(point.x, point.z),
            radius,
            point.y - radius,
            radius * 2.0,
            |_| clear = false,
        );
        self.collision_index
            .for_each_disc(point.x, point.z, radius, &self.walls, |wall| {
                if wall.max_y > point.y - radius
                    && wall.min_y < point.y + radius
                    && wall.overlaps_disc(point.x, point.z, radius)
                {
                    clear = false;
                }
            });
        if self
            .floor
            .height_below(point.x, point.z, point.y + radius)
            .is_some_and(|floor| floor > point.y - radius)
        {
            clear = false;
        }
        clear
            && !self.door_colliders().iter().any(|door| {
                door.overlaps_body_y(point.y - radius, radius * 2.0)
                    && door.overlaps_disc(point.x, point.z, radius)
            })
    }
}
