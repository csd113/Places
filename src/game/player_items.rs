//! Fixed movement command integration with the physical item world.

use glam::{Vec2, Vec3};

use super::{Game, InputState, Settings};
use crate::engine::physics::{HeldPlayerContact, HeldPlayerSlide, ItemWorld, PlayerContext};

pub(super) enum PlayerContacts<'a> {
    External {
        constrain: &'a dyn Fn(&Game) -> Vec3,
        stance_clear: &'a dyn Fn(&Game) -> bool,
    },
    Items(&'a mut ItemWorld),
}

impl PlayerContacts<'_> {
    pub(super) fn stance_clear(&self, game: &Game) -> bool {
        match self {
            Self::External {
                stance_clear,
                constrain: _,
            } => stance_clear(game),
            Self::Items(world) => world.player_pose_clear(game.physical_player_context()),
        }
    }

    pub(super) const fn items(&self) -> Option<&ItemWorld> {
        match self {
            Self::Items(world) => Some(world),
            Self::External {
                constrain: _,
                stance_clear: _,
            } => None,
        }
    }

    pub(super) fn command_origin(&self, game: &Game) -> Option<PlayerContext> {
        self.items().map(|_| game.physical_player_context())
    }

    pub(super) fn resolve_command(&self, game: &mut Game, origin: Option<PlayerContext>) {
        if let (Self::Items(world), Some(command_start)) = (self, origin) {
            game.resolve_item_movement(
                world,
                command_start,
                game.physical_player_context(),
                HeldPlayerSlide::Full,
            );
        } else {
            self.apply_frame_constraint(game);
        }
    }

    pub(super) fn prepare_next_command(&mut self, game: &Game, origin: Option<PlayerContext>) {
        if let (Self::Items(world), Some(command_start)) = (self, origin)
            && world.held().is_some()
            && game.airborne_velocity().length_squared() > 0.0
        {
            world.preview_carried_translation(
                command_start,
                game.physical_player_context(),
                game.door_colliders(),
            );
        }
    }

    fn apply_frame_constraint(&self, game: &mut Game) {
        match self {
            Self::External {
                constrain,
                stance_clear: _,
            } => game.apply_movement_constraint(constrain(game)),
            Self::Items(_) => {}
        }
    }

    pub(super) fn finish(&mut self, game: &mut Game) {
        self.apply_frame_constraint(game);
        if let Self::Items(world) = self {
            world.publish_carried_preview();
        }
    }
}

impl Game {
    pub(crate) fn update_player_movement_with_items(
        &mut self,
        input: &mut InputState,
        settings: &Settings,
        items: &mut ItemWorld,
    ) {
        self.update_player_movement_checked(input, settings, PlayerContacts::Items(items));
    }

    pub(crate) fn physical_player_context(&self) -> PlayerContext {
        PlayerContext {
            eye: self.player_position,
            feet: Vec3::new(
                self.player_position.x,
                self.feet_y(),
                self.player_position.z,
            ),
            height: self.body_height(),
            half: self.body_half(),
            view: self.view_direction(),
            yaw: self.player_yaw,
            active: !self.noclip_enabled() && !self.player_is_dead(),
        }
    }

    pub(super) fn constrain_swim_ascent(
        &mut self,
        origin: PlayerContext,
        items: Option<&ItemWorld>,
    ) -> bool {
        let Some(world) = items.filter(|_| self.vertical_velocity > 0.0) else {
            return false;
        };
        let goal = self.physical_player_context();
        // The normal controller runs horizontal motion before vertical motion.
        // Keep that order for props so a clipped approach also chooses the
        // footprint used by the head clearance checks.
        let planar_origin = PlayerContext {
            eye: Vec3::new(origin.eye.x, goal.eye.y, origin.eye.z),
            feet: Vec3::new(origin.feet.x, goal.feet.y, origin.feet.z),
            ..goal
        };
        self.resolve_item_movement(world, planar_origin, goal, HeldPlayerSlide::Planar);
        true
    }

    fn resolve_item_movement(
        &mut self,
        world: &ItemWorld,
        origin: PlayerContext,
        goal: PlayerContext,
        slide: HeldPlayerSlide,
    ) {
        if world.held().is_some() {
            let (eye, contact) = world.constrain_carried_player_eye(origin, goal, slide);
            if let Some(held_contact) = contact {
                self.apply_held_item_contact(eye, &held_contact);
            } else {
                self.apply_movement_constraint(eye);
            }
        } else {
            self.apply_movement_constraint(world.constrain_player_eye_from(origin, goal));
        }
    }

    pub(super) fn rising_item_head_limit(
        &self,
        items: Option<&ItemWorld>,
        rise: f32,
    ) -> Option<f32> {
        let world = items?;
        let player = self.physical_player_context();
        let hit = world.vertical_player_item_travel(player, rise, super::CONTACT_EPS)?;
        Some(player.feet.y + player.height + hit)
    }

    pub(super) fn refresh_command_support(&mut self, items: Option<&ItemWorld>) {
        self.refresh_ground_support();
        if !self.grounded
            && self.vertical_velocity <= 0.0
            && let Some(world) = items
            // The movement sweep stands off by 6 mm. A short downward probe
            // can keep that support through the next command's jump check.
            && world
                .vertical_player_item_travel(self.physical_player_context(), -0.01, 0.7)
                .is_some()
        {
            self.grounded = true;
            self.vertical_velocity = 0.0;
            self.player_floor_y = self.feet_y;
        }
    }

    pub(crate) fn drop_carried_item(&self, world: &mut ItemWorld) -> bool {
        world.drop_with_velocity(self.last_command_velocity)
    }

    pub(crate) fn throw_carried_item(&self, world: &mut ItemWorld) -> bool {
        world.throw_with_velocity(self.view_direction(), self.last_command_velocity)
    }

    fn apply_held_item_contact(&mut self, blocked_eye: Vec3, contact: &HeldPlayerContact) {
        let accepts = |eye: Vec3| {
            self.body_fits_with_height(
                Vec3::new(eye.x, self.feet_y + (eye.y - self.player_position.y), eye.z),
                self.body_height(),
            )
        };
        let use_slide = accepts(contact.eye_after_slide);
        let eye = if use_slide {
            contact.eye_after_slide
        } else if accepts(blocked_eye) {
            blocked_eye
        } else {
            return;
        };
        let feet = self.feet_y + (eye.y - self.player_position.y);
        self.player_position = eye;
        self.feet_y = feet;
        self.apply_item_contact_normal(contact.normal);
        if use_slide && let Some(normal) = contact.slide_normal {
            self.apply_item_contact_normal(normal);
        }
    }

    fn apply_item_contact_normal(&mut self, normal: Vec3) {
        let side = Vec2::new(normal.x, normal.z);
        if normal.y.abs() > side.length() {
            // A real underside stops ascent. A top contact can support a fall.
            if self.vertical_velocity * normal.y < 0.0 {
                self.vertical_velocity = 0.0;
                self.vertical_accumulator = 0.0;
                self.grounded = normal.y > 0.0;
                if self.grounded {
                    self.player_floor_y = self.feet_y;
                } else {
                    self.ceiling_contact_this_frame = true;
                }
            }
        } else {
            let side_normal = side.normalize_or_zero();
            let into = self.horizontal_velocity.dot(side_normal).min(0.0);
            self.horizontal_velocity = Vec2::new(
                side_normal.x.mul_add(-into, self.horizontal_velocity.x),
                side_normal.y.mul_add(-into, self.horizontal_velocity.y),
            );
        }
    }
}
