//! A lift transition retains the rider in cabin space until private admission.
use super::{Game, Stance, TWO_PI};
use crate::engine::elevator::Departure;
use glam::{Vec2, Vec3};

pub(super) struct Arrival {
    pub(super) id: String,
    pub(super) frame: crate::engine::elevator::CabinFrame,
    local_feet: Vec3,
    relative_yaw: f32,
    pitch: f32,
    stance: Stance,
    eye_offset: f32,
    duck_fraction: f32,
    grounded: bool,
    vertical_velocity: f32,
}

pub(super) struct LocalTrip {
    pub source: String,
    pub target_id: String,
    pub target: Option<crate::engine::elevator::Checkpoint>,
    pub from: crate::engine::elevator::CabinFrame,
    pub to: crate::engine::elevator::CabinFrame,
    pub eye: Vec3,
    pub feet_y: f32,
    pub yaw: f32,
    pub floor_y: f32,
}

impl Game {
    pub(super) const fn take_local_elevator_trip(&mut self) -> Option<LocalTrip> {
        self.local_elevator_trip.take()
    }

    pub(super) fn rollback_local_elevator_trip(&mut self, trip: &LocalTrip) {
        self.player_position = trip.eye;
        self.feet_y = trip.feet_y;
        self.player_yaw = trip.yaw;
        self.player_floor_y = trip.floor_y;
        self.vertical_velocity = 0.0;
        self.horizontal_velocity = Vec2::ZERO;
        self.world
            .rollback_elevator_arrival(&trip.source, &trip.target_id, trip.target.as_ref());
        self.reseed_trigger_inside();
    }
    pub(super) fn elevator_departure_pose(&self, departure: &Departure) -> Result<Arrival, String> {
        let source = self
            .world
            .elevators()
            .iter()
            .find(|e| e.def.id == departure.source)
            .ok_or("Departure elevator is missing")?;
        let feet = Vec3::new(self.player_position.x, self.feet_y, self.player_position.z);
        if !self.player_motion_is_finite()
            || !source.contains(feet, 0.25)
            || self.player_shape.is_some()
            || self.form_model.is_some()
            || self.noclip.is_some()
        {
            return Err("Elevator departure needs a safe normal player inside the cabin".into());
        }
        let mut local_feet = source.def.local(feet);
        local_feet.y -= source.y;
        Ok(Arrival {
            id: departure.arrival.clone(),
            frame: crate::engine::elevator::CabinFrame::from_elevator(source),
            local_feet,
            // ElevatorDef::point rotates local -Z towards world -X, whereas
            // player yaw grows towards +X. These angles have opposite signs.
            relative_yaw: self.player_yaw + source.def.rotation_degrees.to_radians(),
            pitch: self.player_pitch,
            stance: self.stance,
            eye_offset: self.eye_offset_current,
            duck_fraction: self.duck_fraction,
            grounded: self.grounded,
            vertical_velocity: self.vertical_velocity,
        })
    }

    /// Only called on the private candidate. An obstructed exact destination
    /// rejects the load and reopens the resident source; it never recentres.
    pub(super) fn arrive_elevator_from(&mut self, arrival: &Arrival) -> Result<(), String> {
        let _default_pose = self.world.arrive_elevator(&arrival.id)?;
        let target = self
            .world
            .elevators()
            .iter()
            .find(|e| e.def.id == arrival.id)
            .ok_or("Arrival elevator is missing")?;
        let feet = target.def.point(
            arrival.local_feet.x,
            target.y + arrival.local_feet.y,
            arrival.local_feet.z,
        );
        let yaw =
            (arrival.relative_yaw - target.def.rotation_degrees.to_radians()).rem_euclid(TWO_PI);
        if !target.contains(feet, 0.25)
            || !self.body_fits_for(feet, arrival.stance.height(), super::PLAYER_RADIUS)
        {
            return Err(
                "Arrival cabin is obstructed at the rider's position; departure retained".into(),
            );
        }
        self.stance = arrival.stance;
        self.eye_offset_current = arrival.eye_offset;
        self.duck_fraction = arrival.duck_fraction;
        self.player_position.x = feet.x;
        self.player_position.z = feet.z;
        self.set_feet_y(feet.y);
        self.player_yaw = yaw;
        self.player_pitch = arrival.pitch;
        self.horizontal_velocity = Vec2::ZERO;
        self.vertical_velocity = arrival.vertical_velocity;
        self.grounded = arrival.grounded;
        self.player_floor_y = feet.y - arrival.local_feet.y;
        self.reseed_trigger_inside();
        Ok(())
    }
}
