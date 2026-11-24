//! Lift ownership at the ordinary interaction/collision boundary.
use super::{
    DispatchReport, EntityWorld, LevelTransition, WorldCommand, WorldContext, door_pose_hits_box,
    door_pose_hits_player,
};
use crate::engine::elevator::{Departure, Elevator, LocalDeparture, Phase};
use glam::Vec3;

impl EntityWorld {
    pub(super) fn resolve_elevators(&mut self, level: &crate::engine::level::LevelDef) {
        self.elevators = level.elevators.iter().map(Elevator::new).collect();
        self.local_elevator_departure = None;
        self.elevator_sounds.clear();
    }
    pub(super) fn reset_elevators(&mut self) {
        for elevator in &mut self.elevators {
            *elevator = Elevator::new(&elevator.def);
        }
        self.local_elevator_departure = None;
        self.elevator_sounds.clear();
    }

    #[must_use]
    pub fn elevators(&self) -> &[Elevator] {
        &self.elevators
    }
    pub const fn take_local_elevator_departure(&mut self) -> Option<LocalDeparture> {
        self.local_elevator_departure.take()
    }
    pub fn finish_local_elevator_departure(&mut self, source: &str, arrival: Option<&str>) {
        self.finish_local_elevator_departure_at(source, arrival, None);
    }
    pub fn finish_local_elevator_departure_at(
        &mut self,
        source: &str,
        arrival: Option<&str>,
        landing: Option<usize>,
    ) {
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
            if let Some(index) = landing {
                let _arrived = elevator.arrive_at(index);
            } else {
                elevator.arrive();
            }
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
        self.local_elevator_departure = None;
        self.commands.retain(|command| {
            !matches!(
                command,
                WorldCommand::ChangeLevel(LevelTransition::Elevator(_))
            )
        });
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
        let action = parts.next()?;
        let elevator = self.elevators.iter_mut().find(|e| e.def.id == name)?;
        let feet = origin
            .filter(|eye| eye.y > elevator.y + 0.4 && eye.y < elevator.y + 2.5)
            .map(|eye| Vec3::new(eye.x, elevator.y, eye.z));
        let accepted = match action {
            "door" if parts.next().is_none() => elevator.toggle_doors(feet),
            "floor" | "call" => {
                let stop = parts.next()?.parse::<usize>().ok()?;
                if parts.next().is_some() {
                    return None;
                }
                elevator.request(stop, action == "floor", feet)
            }
            _ => return None,
        };
        if accepted {
            let point = elevator
                .controls()
                .into_iter()
                .find(|button| button.id == id)?
                .anchor;
            self.elevator_sounds.push((point, "switch_click"));
            self.sync_elevator_controls();
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
    pub fn update_elevators(&mut self, ctx: &WorldContext<'_>) {
        if self.elevators.is_empty() {
            return;
        }
        let mut changed = false;
        for elevator in &mut self.elevators {
            let before = (elevator.y, elevator.opening, elevator.at, elevator.phase);
            let step = elevator.advance(ctx.delta_seconds, ctx.feet, |leaf| {
                door_pose_hits_player(leaf, ctx.feet, ctx.body_height, ctx.body_half)
                    || self
                        .item_obstacles
                        .all
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
                || elevator.at != before.2
                || elevator.phase != before.3;
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
            match step.departure {
                Some(Departure::Local(departure)) => {
                    self.local_elevator_departure = Some(departure);
                }
                Some(Departure::Level(departure)) => {
                    if !Self::queue_level_transition(
                        &mut self.commands,
                        LevelTransition::Elevator(departure),
                    ) {
                        elevator.reopen();
                        changed = true;
                    }
                }
                None => {}
            }
        }
        if changed {
            self.rebuild_door_colliders();
            self.sync_elevator_controls();
        }
    }
}
