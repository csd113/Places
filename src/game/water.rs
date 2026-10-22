//! Independent water locomotion inspired by the Source SDK 2013 singleplayer
//! behavior pinned in the stage 3 report. No SDK implementation is included.
//! Land acceleration, air control and stance ownership stay in `game`.
use super::input::Control;
use super::{
    CONTACT_EPS, Game, HorizontalMode, InputState, PLAYER_RADIUS, PLAYER_STEP_HEIGHT, STEP_EPS,
    Settings, WATER_EXIT_EYE_CLEARANCE_M, WATER_EXIT_STEP_M, WaterExit, WaterSample,
};
use crate::engine::physics::GRAVITY;
use glam::{Vec2, Vec3};

const WATER_FRICTION: f32 = 4.0;
const WATER_ACCELERATION: f32 = 10.0;
// Retain the existing 2.4 m/s swim cap with the new 3.75 m/s land default.
const WATER_SPEED_FACTOR: f32 = 0.64;
// Scale Source's 72-unit standing hull to Places' 1.8 m body.
const SOURCE_METRES: f32 = 1.8 / 72.0;
const IDLE_SINK_SPEED: f32 = 60.0 * SOURCE_METRES;
const JUMP_RISE_SPEED: f32 = 100.0 * SOURCE_METRES;
const EXIT_FORWARD_SPEED: f32 = 50.0 * SOURCE_METRES;

impl Game {
    /// Source-style feet/waist/eye classification using the live stance.
    #[must_use]
    pub fn water_depth_level(&self) -> u8 {
        let wet = |y| {
            self.water
                .sample(self.player_position.x, self.player_position.z, y)
                .is_some_and(|sample| sample.swimming && y >= sample.bottom_y)
        };
        if !wet(self.feet_y + SOURCE_METRES) {
            return 0;
        }
        if !wet(self.body_height().mul_add(0.5, self.feet_y)) {
            return 1;
        }
        if !wet(self.player_position.y) {
            return 2;
        }
        3
    }

    pub(super) fn deep_water_under(&self, sample: Option<WaterSample>) -> bool {
        sample.is_some_and(|water| {
            water.swimming
                && self.body_height().mul_add(0.5, self.feet_y) <= water.surface_y
                && self.body_height().mul_add(0.5, self.feet_y) >= water.bottom_y
        })
    }

    /// Five body-footprint samples ease currents across banks and split volumes.
    /// The same last-authored membership rule serves the object column sampler.
    pub(super) fn player_fluid_velocity(&self) -> Vec2 {
        let mut flow = Vec2::ZERO;
        for (dx, dz) in [
            (0.0, 0.0),
            (PLAYER_RADIUS, 0.0),
            (-PLAYER_RADIUS, 0.0),
            (0.0, PLAYER_RADIUS),
            (0.0, -PLAYER_RADIUS),
        ] {
            if let Some(water) = self
                .water
                .sample(
                    self.player_position.x + dx,
                    self.player_position.z + dz,
                    self.feet_y,
                )
                .filter(|water| water.swimming && self.feet_y >= water.bottom_y - CONTACT_EPS)
            {
                flow = Vec2::new(
                    water.flow[0].mul_add(0.2, flow.x),
                    water.flow[1].mul_add(0.2, flow.y),
                );
            }
        }
        flow
    }

    pub(super) fn player_immersion(&self) -> f32 {
        self.water
            .sample(self.player_position.x, self.player_position.z, self.feet_y)
            .filter(|water| water.swimming)
            .map_or(0.0, |water| {
                ((water.surface_y - self.feet_y) / self.body_height()).clamp(0.0, 1.0)
            })
    }

    /// Friction precedes bounded acceleration in the full 3D fluid frame.
    /// Looking up amplifies forward ascent; looking down and moving dives.
    pub(super) fn water_velocity(&mut self, input: &InputState, settings: &Settings, delta: f32) {
        let axis = |positive, negative| {
            f32::from(u8::from(input.is_held(positive)))
                - f32::from(u8::from(input.is_held(negative)))
        };
        let forward_axis = axis(Control::MoveForward, Control::MoveBackward);
        let side_axis = axis(Control::StrafeRight, Control::StrafeLeft);
        let (yaw_sin, yaw_cos) = self.player_yaw.sin_cos();
        let (pitch_sin, pitch_cos) = self.player_pitch.sin_cos();
        let forward = Vec3::new(yaw_sin * pitch_cos, pitch_sin, -yaw_cos * pitch_cos);
        let right = Vec3::new(yaw_cos, 0.0, yaw_sin);
        let mut wish = Vec3::new(
            forward.x.mul_add(forward_axis, right.x * side_axis) * settings.walk_speed,
            forward.y * forward_axis * settings.walk_speed,
            forward.z.mul_add(forward_axis, right.z * side_axis) * settings.walk_speed,
        );
        if input.is_held(Control::Jump) {
            wish.y += settings.walk_speed;
            self.vertical_velocity = JUMP_RISE_SPEED;
        } else if forward_axis == 0.0 && side_axis == 0.0 {
            wish.y -= IDLE_SINK_SPEED;
        } else {
            wish.y += (forward_axis * settings.walk_speed * pitch_sin * 2.0)
                .clamp(0.0, settings.walk_speed);
        }
        let wish_speed = wish.length().min(settings.walk_speed) * WATER_SPEED_FACTOR;
        let fluid = self.player_fluid_velocity();
        let mut relative = Vec3::new(
            self.horizontal_velocity.x - fluid.x,
            self.vertical_velocity,
            self.horizontal_velocity.y - fluid.y,
        );
        relative = Vec3::from_array(
            relative
                .to_array()
                .map(|component| component * WATER_FRICTION.mul_add(-delta, 1.0).max(0.0)),
        );
        // Directional acceleration preserves authority along a bank when a
        // perpendicular current is physically blocked. Straight-line response
        // retains the reference rates; transverse pressure cannot consume the
        // swimmer's whole acceleration budget.
        let direction = wish.normalize_or_zero();
        let add = (wish_speed - relative.dot(direction)).max(0.0);
        let acceleration = add.min(WATER_ACCELERATION * wish_speed * delta);
        relative = Vec3::new(
            direction.x.mul_add(acceleration, relative.x),
            direction.y.mul_add(acceleration, relative.y),
            direction.z.mul_add(acceleration, relative.z),
        );
        self.horizontal_velocity = Vec2::new(relative.x + fluid.x, relative.z + fluid.y);
        self.vertical_velocity = relative.y;
    }

    /// Physical feet/head contacts remain the shared controller's contacts.
    pub(super) fn advance_swimming(&mut self, delta: f32) {
        let start_feet = self.feet_y;
        self.set_feet_y(self.vertical_velocity.mul_add(delta, start_feet));
        self.grounded = false;
        if self.vertical_velocity <= 0.0
            && let Some(support) =
                self.support_at(self.player_position.x, self.player_position.z, start_feet)
            && self.feet_y <= support + CONTACT_EPS
        {
            self.set_feet_y(support);
            self.player_floor_y = support;
            self.vertical_velocity = 0.0;
            self.grounded = true;
        }
        if self.vertical_velocity > 0.0
            && let Some(limit) = self.head_limit(start_feet)
        {
            let max_feet = limit - self.body_height().max(self.eye_offset() + 0.1) - CONTACT_EPS;
            if self.feet_y > max_feet {
                self.set_feet_y(max_feet.max(start_feet));
                self.vertical_velocity = 0.0;
                self.ceiling_contact_this_frame = true;
            }
        }
    }

    /// A real near-surface ledge, forward intent and clear destination permit
    /// one bounded water jump. Backward entry and fast plunges never trigger it.
    pub(super) fn try_water_exit(&mut self, move_dir: Vec3) {
        if self.water_depth_level() != 2 || self.vertical_velocity < -180.0 * SOURCE_METRES {
            return;
        }
        let facing = Vec2::new(self.player_yaw.sin(), -self.player_yaw.cos());
        let intent = Vec2::new(move_dir.x, move_dir.z).normalize_or_zero();
        if intent.dot(facing) < 0.5 || self.horizontal_velocity.dot(facing) < -0.025 {
            return;
        }
        let Some(surface_y) = self
            .water
            .sample(self.player_position.x, self.player_position.z, self.feet_y)
            .map(|water| water.surface_y)
        else {
            return;
        };
        // Collision pressure can include a sideways current. Only the
        // player's forward intent may choose the water-jump ledge.
        self.pressed_exit_support = None;
        self.note_pressed_water_exit(
            Vec2::new(self.player_position.x, self.player_position.z),
            intent,
            surface_y,
        );
        // The arc covers the stance's waist depth plus the authored 0.5 m
        // exit rim and camera clearance: 1.5 m standing, 1.05 m crouched.
        let exit_rise = self
            .body_height()
            .mul_add(0.5, WATER_EXIT_STEP_M + WATER_EXIT_EYE_CLEARANCE_M);
        let Some(support_y) = self.pressed_exit_support.filter(|support| {
            *support > self.feet_y + PLAYER_STEP_HEIGHT + STEP_EPS
                && *support - self.feet_y <= exit_rise - CONTACT_EPS
                && self.water_support_reachable(*support, surface_y)
        }) else {
            return;
        };
        self.water_exit = Some(WaterExit {
            support_y,
            surface_y,
            direction: intent,
            remaining: 2.0,
        });
        self.vertical_velocity = (2.0 * GRAVITY * exit_rise).sqrt();
        self.grounded = false;
        self.swimming = false;
        self.jump_latched = true;
    }

    pub(super) fn advance_water_exit(&mut self, delta: f32) -> f32 {
        let Some(mut exit) = self.water_exit else {
            return 0.0;
        };
        let before = Vec2::new(self.player_position.x, self.player_position.z);
        self.move_horizontal(
            Vec3::new(exit.direction.x, 0.0, exit.direction.y),
            EXIT_FORWARD_SPEED,
            delta,
            HorizontalMode::Swim {
                surface_y: exit.surface_y,
            },
        );
        let _step = self.integrate_vertical_with_gravity(delta, GRAVITY);
        exit.remaining -= delta;
        if self.grounded
            || self.ceiling_contact_this_frame
            || exit.remaining <= 0.0
            || self.vertical_velocity <= 0.0
        {
            self.water_exit = None;
        } else {
            self.water_exit = Some(exit);
        }
        let after = Vec2::new(self.player_position.x, self.player_position.z);
        let interval = delta.max(f32::EPSILON);
        self.horizontal_velocity = Vec2::new(
            (after.x - before.x) / interval,
            (after.y - before.y) / interval,
        );
        before.distance(after)
    }

    /// Clearance and the authored mean plane bound every eligible rim.
    pub(super) fn water_support_reachable(&self, support: f32, surface_y: f32) -> bool {
        support <= surface_y + WATER_EXIT_STEP_M + STEP_EPS
            && self.head_clear_for(support, self.body_height())
    }
}
