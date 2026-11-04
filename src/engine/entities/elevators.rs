//! Lift ownership at the ordinary interaction/collision boundary.
use super::{
    DispatchReport, EntityWorld, WorldCommand, WorldContext, door_pose_hits_box,
    door_pose_hits_player,
};
use crate::engine::elevator::{Departure, Elevator, Phase};
use glam::Vec3;

impl EntityWorld {
    pub(super) fn resolve_elevators(&mut self, level: &crate::engine::level::LevelDef) {
        self.elevators = level.elevators.iter().map(Elevator::new).collect();
        self.elevator_departure = None;
        self.elevator_sounds.clear();
    }
    pub(super) fn reset_elevators(&mut self) {
        for elevator in &mut self.elevators {
            *elevator = Elevator::new(&elevator.def);
        }
        self.elevator_departure = None;
        self.elevator_sounds.clear();
    }

    #[must_use]
    pub fn elevators(&self) -> &[Elevator] {
        &self.elevators
    }
    pub const fn take_elevator_departure(&mut self) -> Option<Departure> {
        self.elevator_departure.take()
    }
    pub fn take_local_elevator_departure(&mut self) -> Option<crate::engine::elevator::Departure> {
        if self
            .elevator_departure
            .as_ref()
            .is_some_and(|departure| departure.level.is_none())
        {
            self.elevator_departure.take()
        } else {
            None
        }
    }
    pub fn finish_local_elevator_departure(&mut self, source: &str, arrival: Option<&str>) {
        if let Some(elevator) = self.elevators.iter_mut().find(|e| e.def.id == source) {
            if arrival.is_some() {
                elevator.finish_departure();
            } else {
                elevator.reopen();
            }
        }
        if let Some(id) = arrival
            && let Some(elevator) = self.elevators.iter_mut().find(|e| e.def.id == id)
        {
            elevator.arrive();
            self.elevator_sounds.push((
                elevator.def.point(0.0, elevator.y + 1.2, 1.2),
                "elevator_arrive",
            ));
        }
        self.rebuild_door_colliders();
        self.sync_elevator_controls();
    }
    pub fn take_elevator_sounds(&mut self) -> Vec<(Vec3, &'static str)> {
        std::mem::take(&mut self.elevator_sounds)
    }
    pub fn rollback_elevator_arrival(
        &mut self,
        source: &str,
        target_id: &str,
        target: Option<&crate::engine::elevator::Checkpoint>,
    ) {
        if let Some(record) = target
            && let Some(elevator) = self.elevators.iter_mut().find(|e| e.def.id == target_id)
        {
            let centre = elevator.def.point(0.0, elevator.y + 1.2, 1.2);
            self.elevator_sounds.retain(|(point, cue)| {
                *cue != "elevator_arrive" || point.distance_squared(centre) > 0.000_001
            });
            let _restored = elevator.restore(record);
        }
        self.finish_local_elevator_departure(source, None);
    }
    pub fn recover_elevators(&mut self) {
        for elevator in &mut self.elevators {
            if elevator.phase == Phase::Loading {
                elevator.reopen();
            }
        }
        self.elevator_departure = None;
        self.rebuild_door_colliders();
    }
    /// # Errors
    /// Arrival must name a lift carried by the destination package.
    pub fn arrive_elevator(&mut self, id: &str) -> Result<(Vec3, f32), String> {
        let elevator = self
            .elevators
            .iter_mut()
            .find(|e| e.def.id == id)
            .ok_or_else(|| format!("Arrival elevator '{id}' is missing"))?;
        elevator.arrive();
        self.elevator_sounds.push((
            elevator.def.point(0.0, elevator.y + 1.2, 1.2),
            "elevator_arrive",
        ));
        let result = (
            elevator.def.point(0.0, elevator.y, 1.2),
            elevator.def.rotation_degrees.to_radians(),
        );
        self.rebuild_door_colliders();
        self.sync_elevator_controls();
        Ok(result)
    }
    pub(super) fn dispatch_elevator(
        &mut self,
        id: &str,
        origin: Option<Vec3>,
    ) -> Option<DispatchReport> {
        let spec = id.strip_prefix("@elevator:")?;
        let mut parts = spec.split(':');
        let name = parts.next()?;
        let cabin = parts.next()? == "floor";
        let stop = parts.next()?.parse::<usize>().ok()?;
        let elevator = self.elevators.iter_mut().find(|e| e.def.id == name)?;
        let accepted = elevator.request(
            stop,
            cabin,
            origin
                .filter(|eye| eye.y > elevator.y + 0.4 && eye.y < elevator.y + 2.5)
                .map(|eye| Vec3::new(eye.x, elevator.y, eye.z)),
        );
        if accepted {
            self.elevator_sounds.push((
                elevator.def.point(0.94, elevator.y + 1.1, 0.0),
                "switch_click",
            ));
        }
        Some(DispatchReport {
            actions_run: usize::from(accepted),
            ..DispatchReport::default()
        })
    }
    pub(super) fn sync_elevator_controls(&mut self) {
        if self.elevators.is_empty() {
            return;
        }
        let mut items: Vec<_> = self
            .interactables
            .items()
            .iter()
            .filter(|i| !i.id.starts_with("@elevator:"))
            .cloned()
            .collect();
        for elevator in &self.elevators {
            items.extend(elevator.controls());
        }
        self.interactables = crate::engine::interact::Interactables::with_items(items);
    }
    pub fn update_elevators(&mut self, ctx: &WorldContext<'_>) -> f32 {
        if self.elevators.is_empty() {
            return 0.0;
        }
        let mut carry = 0.0;
        let mut changed = false;
        for elevator in &mut self.elevators {
            let before = (elevator.y, elevator.opening, elevator.at);
            let step = elevator.advance(ctx.delta_seconds, ctx.feet, |leaf| {
                door_pose_hits_player(leaf, ctx.feet, ctx.body_height, ctx.body_half)
                    || self
                        .navigation_obstacles
                        .iter()
                        .any(|wall| door_pose_hits_box(leaf, wall))
                    || self.ai.agents().iter().any(|agent| {
                        leaf.overlaps_body_y(agent.position.y, agent.profile.height)
                            && leaf.overlaps_disc(
                                agent.position.x,
                                agent.position.z,
                                agent.profile.radius,
                            )
                    })
            });
            changed |= (elevator.y - before.0).abs() > f32::EPSILON
                || (elevator.opening - before.1).abs() > f32::EPSILON
                || elevator.at != before.2;
            carry += step.carry_y;
            if let Some(sound) = step.sound {
                let source_y = if elevator.settled() {
                    elevator
                        .def
                        .stops
                        .iter()
                        .filter(|stop| stop.is_landing())
                        .min_by(|a, b| {
                            (a.y - ctx.feet.y)
                                .abs()
                                .total_cmp(&(b.y - ctx.feet.y).abs())
                        })
                        .map_or(elevator.y, |stop| stop.y)
                } else {
                    elevator.y
                };
                self.elevator_sounds
                    .push((elevator.def.point(0.0, source_y + 1.2, 0.3), sound));
            }
            if let Some(departure) = step.departure {
                if let Some(level) = &departure.level {
                    self.commands.push(WorldCommand::ChangeLevel {
                        level: level.clone(),
                    });
                }
                self.elevator_departure = Some(departure);
            }
        }
        if changed {
            self.rebuild_door_colliders();
            self.sync_elevator_controls();
        }
        carry
    }
}
