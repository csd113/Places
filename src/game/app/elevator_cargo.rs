//! Travelled objects retain one semantic owner across package-level visits.
use super::{FrameLoop, item_runtime::Items};
use crate::engine::{
    elevator::{CabinFrame, Elevator},
    level::{LevelDef, LevelSurfaces, PropDef},
};
use glam::{Quat, Vec3};
use serde::{Deserialize, Serialize};

const MAX_TRAVELLERS: usize = 32;

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ledger {
    entries: Vec<Traveller>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Traveller {
    id: String,
    origin_level: String,
    origin_id: String,
    host: String,
    definition: PropDef,
    position: Vec3,
    orientation: Quat,
    state: crate::engine::entities::passengers::Record,
    physical: Option<crate::engine::physics::checkpoint::Item>,
}

impl Traveller {
    fn runtime_id(&self) -> &str {
        if self.host == self.origin_level {
            &self.origin_id
        } else {
            &self.id
        }
    }
}

fn portable_definition(prop: &PropDef) -> Result<(), String> {
    use crate::engine::level::ComponentDef;
    if prop.solid || !prop.bindings.is_empty() || prop.float.is_some() || prop.animated_door.is_some()
        || !prop.lights.is_empty() || prop.switch_group.is_some() || prop.light_switch.is_some()
        || prop.components.iter().any(|c| !matches!(c,
            ComponentDef::State {name:_,value:_} | ComponentDef::NavAgent {radius:_,height:_,speed_mps:_,step_height:_,max_slope:_}
            | ComponentDef::Ai(_) | ComponentDef::Animation {clip:_,speed:_,looped:_,playing:_}
            | ComponentDef::Interactable {prompt:_,reach:_,enabled:_,label:_}))
        || prop.components.iter().any(|c| matches!(c,ComponentDef::Ai(ai) if !ai.tasks.is_empty() || !ai.senses.is_empty() || ai.home_bounds.is_some()))
    { return Err("Elevator passengers currently support ordinary movable props and NPCs without map-bound wiring, tasks or effects".into()); }
    Ok(())
}

#[derive(Clone)]
pub struct Transfer {
    pub from: CabinFrame,
    pub arrival: String,
    selected: Vec<String>,
}

pub struct Visit {
    pub ledger: Ledger,
    pub transfer: Option<Transfer>,
}

fn body_bounds(game: &crate::game::Game, items: &Items, id: &str) -> Option<(Vec3, Vec3)> {
    if let Some((key, _, _)) = items.authored.iter().find(|(_, name, _)| name == id) {
        return items.world.body_bounds(*key);
    }
    let agent = game.entities().ai().agent(id)?;
    Some(agent_bounds(agent))
}

fn agent_bounds(agent: &crate::engine::ai::AiAgent) -> (Vec3, Vec3) {
    let p = agent.position;
    let r = agent.profile.radius;
    (
        Vec3::new(p.x - r, p.y, p.z - r),
        Vec3::new(p.x + r, p.y + agent.profile.height, p.z + r),
    )
}

fn refuse_unsupported_occupants(items: &Items, frame: &CabinFrame) -> Result<(), String> {
    for body in items.world.snapshots().iter().filter(|b| {
        b.motion.is_movable() && !items.authored.iter().any(|(key, _, _)| *key == b.key)
    }) {
        if let Some((min, max)) = items.world.body_bounds(body.key)
            && frame.contains_bounds(min, max)
        {
            return Err(
                "Session-created props are not yet portable; use an authored movable prop".into(),
            );
        }
    }
    for actor in items.world.ragdoll_snapshots() {
        if actor.world_positions.iter().any(|p| {
            let local = frame.local(Vec3::from_array(*p));
            local.x.abs() < 1.2
                && local.z > 0.0
                && local.z < 2.4
                && local.y >= 0.0
                && local.y < crate::engine::elevator::HEIGHT
        }) {
            return Err(format!(
                "Articulated actor '{}' cannot travel yet; restore its NPC controller first",
                actor.id
            ));
        }
    }
    Ok(())
}

impl Ledger {
    pub fn validate_arrivals(
        &self,
        game: &crate::game::Game,
        items: Option<&super::item_runtime::PreparedItems>,
        level: &str,
    ) -> Result<(), String> {
        for agent in game.entities().ai().agents() {
            check_npc_player(
                game,
                agent.position,
                agent.profile.radius,
                agent.profile.height,
            )?;
            if items.is_some_and(|prepared| {
                !prepared.world.passenger_space_clear(
                    agent.position,
                    agent.profile.height,
                    agent.profile.radius,
                )
            }) {
                return Err(format!(
                    "Arrival is obstructed for resident NPC '{}'",
                    agent.instance_id
                ));
            }
        }
        for record in self.entries.iter().filter(|r| r.host == level) {
            let Some(agent) = game.entities().ai().agent(record.runtime_id()) else {
                continue;
            };
            check_npc_space(
                game,
                agent.instance_id.as_str(),
                agent.position,
                agent.profile.radius,
                agent.profile.height,
            )?;
            if game
                .navigation
                .as_ref()
                .and_then(|nav| nav.compatible_class_index(&agent.profile.class()))
                .is_none()
            {
                return Err(format!(
                    "Destination has no safe baked navigation class for NPC '{}'",
                    agent.instance_id
                ));
            }
            if items.is_some_and(|i| {
                !i.world.passenger_space_clear(
                    agent.position,
                    agent.profile.height,
                    agent.profile.radius,
                )
            }) {
                return Err(format!(
                    "Arrival is obstructed for NPC '{}'",
                    agent.instance_id
                ));
            }
        }
        Ok(())
    }
    pub fn has_host(&self, level: &str) -> bool {
        self.entries.iter().any(|r| r.host == level)
    }
    pub(super) fn refresh(
        &mut self,
        level: &LevelDef,
        game: &crate::game::Game,
        items: &Items,
    ) -> Result<(), String> {
        for record in self.entries.iter_mut().filter(|r| r.host == level.id) {
            let live_id = if record.origin_level == level.id
                && game.entities().handle_of(&record.origin_id).is_some()
            {
                &record.origin_id
            } else {
                &record.id
            };
            let live_name = live_id.clone();
            Self::observe(record, &live_name, game, items)?;
        }
        Ok(())
    }

    fn observe(
        record: &mut Traveller,
        id: &str,
        game: &crate::game::Game,
        items: &Items,
    ) -> Result<(), String> {
        record.state = game.entities().passenger_record(id)?;
        let handle = game
            .entities()
            .handle_of(id)
            .ok_or("Travelled entity disappeared")?;
        let transform = game
            .entities()
            .components()
            .transforms
            .get(handle)
            .ok_or("Passenger transform disappeared")?;
        record.position = transform.position;
        record.orientation = Quat::from_rotation_y(transform.yaw_degrees.to_radians());
        if let Some(item) = items
            .authored
            .iter()
            .find(|(_, name, _)| name.as_str() == id)
        {
            let (pose, _) = items
                .world
                .body_pose_velocity(item.0)
                .ok_or("Passenger body disappeared")?;
            record.position = pose.position;
            record.orientation = pose.rotation;
            record.physical = items
                .world
                .transport_checkpoint(std::slice::from_ref(item))?
                .into_iter()
                .next()
                .map(|(_, state)| state);
        }
        record.state.set_pose(
            id,
            record.position,
            record
                .orientation
                .to_euler(glam::EulerRot::YXZ)
                .0
                .to_degrees(),
        );
        Ok(())
    }

    fn capture(
        &mut self,
        level: &LevelDef,
        game: &crate::game::Game,
        items: &Items,
        frame: &CabinFrame,
        arrival: &str,
    ) -> Result<Transfer, String> {
        refuse_unsupported_occupants(items, frame)?;
        self.refresh(level, game, items)?;
        let ids = level.prop_instance_ids();
        let mut selected = Vec::new();
        let mut aliases = Vec::new();
        for (prop, id) in level.props.iter().zip(ids) {
            let Some((min, max)) = body_bounds(game, items, &id) else {
                continue;
            };
            if !frame.contains_bounds(min, max) {
                let centre = frame.local(Vec3::new(
                    f32::midpoint(min.x, max.x),
                    f32::midpoint(min.y, max.y),
                    f32::midpoint(min.z, max.z),
                ));
                if centre.x.abs() < 1.2
                    && centre.z > 0.36
                    && centre.z < 2.4
                    && centre.y >= 0.0
                    && centre.y < crate::engine::elevator::HEIGHT
                {
                    return Err(format!(
                        "Passenger '{id}' does not fit completely inside the cabin"
                    ));
                }
                continue;
            }
            portable_definition(prop).map_err(|e| format!("Passenger '{id}': {e}"))?;
            let found_index = self.entries.iter().position(|r| {
                r.host == level.id
                    && (r.id == id || (r.origin_level == level.id && r.origin_id == id))
            });
            let index = if let Some(previous) = found_index {
                previous
            } else {
                if self.entries.len() >= MAX_TRAVELLERS {
                    return Err("Elevator traveller limit reached (32)".into());
                }
                let hash = crate::engine::package::hash::sha256_hex(
                    format!("{}\n{id}", level.id).as_bytes(),
                );
                let token = format!(
                    "travel_{}",
                    hash.get(..32).ok_or("Invalid passenger identity")?
                );
                let state = game.entities().passenger_record(&id)?;
                self.entries.push(Traveller {
                    id: token,
                    origin_level: level.id.clone(),
                    origin_id: id.clone(),
                    host: level.id.clone(),
                    definition: prop.clone(),
                    position: Vec3::ZERO,
                    orientation: Quat::IDENTITY,
                    state,
                    physical: None,
                });
                self.entries.len().saturating_sub(1)
            };
            let record = self
                .entries
                .get_mut(index)
                .ok_or("Passenger slot missing")?;
            Self::observe(record, &id, game, items)?;
            record.state.cabin_home(frame);
            aliases.push((id, record.id.clone()));
            selected.push(record.id.clone());
        }
        if let Some(held) = items.world.held()
            && !items
                .authored
                .iter()
                .any(|(key, id, _)| *key == held && aliases.iter().any(|(old, _)| old == id))
        {
            return Err("Bring the whole held prop inside the cabin before travelling".into());
        }
        for agent in game.entities().ai().agents() {
            let (min, max) = agent_bounds(agent);
            if frame.contains_bounds(min, max)
                && !aliases.iter().any(|(id, _)| *id == agent.instance_id)
            {
                return Err(format!(
                    "Runtime NPC '{}' is not yet portable; use an authored NPC",
                    agent.instance_id
                ));
            }
        }
        for record in self.entries.iter_mut().filter(|r| selected.contains(&r.id)) {
            record.state.remap_targets(&aliases);
        }
        Ok(Transfer {
            from: frame.clone(),
            arrival: arrival.into(),
            selected,
        })
    }

    pub fn restore_world(&self, game: &mut crate::game::Game, level: &str) -> Result<(), String> {
        for record in self.entries.iter().filter(|r| r.host == level) {
            game.entities_mut().restore_passenger(&record.state)?;
        }
        Ok(())
    }

    pub fn restore_items(
        &self,
        items: &mut super::item_runtime::PreparedItems,
        level: &str,
    ) -> Result<(), String> {
        let records: Vec<_> = self
            .entries
            .iter()
            .filter(|r| r.host == level)
            .filter_map(|r| {
                r.physical
                    .as_ref()
                    .map(|p| (r.runtime_id().into(), p.clone()))
            })
            .collect();
        let selected: Vec<_> = items
            .authored
            .iter()
            .filter(|(_, id, _)| records.iter().any(|(name, _)| name == id))
            .cloned()
            .collect();
        items.world.restore_checkpoint(&selected, &records)?;
        for (key, id, _) in &selected {
            if items
                .world
                .body_penetration_depth(*key)
                .is_none_or(|d| d > 0.004)
            {
                return Err(format!(
                    "Arrival is obstructed for passenger '{id}'; departure retained"
                ));
            }
        }
        Ok(())
    }
}

impl Visit {
    fn validate(&self, catalog: &crate::engine::loader::PropCatalog) -> Result<(), String> {
        if self.ledger.entries.len() > MAX_TRAVELLERS {
            return Err("Too many transported entities".into());
        }
        for record in &self.ledger.entries {
            portable_definition(&record.definition)?;
            if !catalog.contains(&record.definition.model) {
                return Err("Transported model is not in the installed catalogue".into());
            }
            if !record.position.is_finite()
                || record.position.abs().max_element() > 100_000.0
                || !record.orientation.is_finite()
                || (record.orientation.length_squared() - 1.0).abs() > 0.001
            {
                return Err("Invalid transported entity pose".into());
            }
        }
        Ok(())
    }
    /// Runs on the loading worker before physics, AI and character preparation.
    pub fn apply(
        mut self,
        level: &mut LevelDef,
        catalog: &crate::engine::loader::PropCatalog,
    ) -> Result<Ledger, String> {
        self.validate(catalog)?;
        if let Some(transfer) = &self.transfer {
            let def = level
                .elevators
                .iter()
                .find(|e| e.id == transfer.arrival)
                .ok_or("Arrival elevator is missing")?;
            let target = CabinFrame::from_elevator(&Elevator::new(def));
            let turn = transfer.from.turn_to(&target);
            for record in self
                .ledger
                .entries
                .iter_mut()
                .filter(|r| transfer.selected.contains(&r.id))
            {
                let crossing = record.host != level.id;
                record.host.clone_from(&level.id);
                record.position = crate::engine::elevator::map_cabin_point(
                    transfer.from.origin,
                    target.origin,
                    turn,
                    record.position,
                );
                record.orientation = turn.mul_quat(record.orientation).normalize();
                if crossing {
                    record.state.retain_targets(&transfer.selected);
                }
                let live_name = record.runtime_id().to_string();
                record
                    .state
                    .relocate(&live_name, transfer.from.origin, target.origin, turn);
                record.physical = record
                    .physical
                    .as_ref()
                    .map(|p| p.transported(transfer.from.origin, target.origin, turn));
            }
        }
        let aliases: Vec<_> = self
            .ledger
            .entries
            .iter()
            .filter(|r| r.host == level.id)
            .map(|r| (r.id.clone(), r.runtime_id().to_string()))
            .collect();
        for record in self
            .ledger
            .entries
            .iter_mut()
            .filter(|r| r.host == level.id)
        {
            let live_name = record.runtime_id().to_string();
            record.state.set_pose(
                &live_name,
                record.position,
                record
                    .orientation
                    .to_euler(glam::EulerRot::YXZ)
                    .0
                    .to_degrees(),
            );
            record.state.remap_targets(&aliases);
        }
        let ids = level.prop_instance_ids();
        level.props = std::mem::take(&mut level.props)
            .into_iter()
            .zip(ids)
            .filter_map(|(p, id)| {
                (!self
                    .ledger
                    .entries
                    .iter()
                    .any(|r| r.id == id || (r.origin_level == level.id && r.origin_id == id)))
                .then_some(p)
            })
            .collect();
        for record in self.ledger.entries.iter().filter(|r| r.host == level.id) {
            let mut prop = record.definition.clone();
            prop.id = Some(record.runtime_id().into());
            prop.x = record.position.x;
            prop.z = record.position.z;
            prop.y = record.position.y
                - LevelSurfaces::new(level)
                    .floor_y_at(prop.x, prop.z)
                    .unwrap_or(0.0);
            let (yaw, pitch, roll) = record.orientation.to_euler(glam::EulerRot::YXZ);
            prop.rotation_degrees = yaw.to_degrees();
            prop.pitch_degrees = pitch.to_degrees();
            prop.roll_degrees = roll.to_degrees();
            level.props.push(prop);
        }
        if level
            .props
            .iter()
            .filter(|p| p.is_physics_movable())
            .count()
            > crate::engine::level::MAX_LEVEL_PHYSICS_PROPS
        {
            return Err("Destination has too many physics props for elevator passengers".into());
        }
        Ok(self.ledger)
    }
}

impl FrameLoop<'_> {
    /// Linked cabins are the passenger transport path. Advanced moving shafts
    /// cannot yet carry independent physics/AI owners; refuse before movement
    /// rather than allowing the rider to silently leave them behind.
    pub(super) fn refuse_occupied_physical_shaft_trip(&mut self) {
        let blocked = self
            .game
            .entities()
            .elevators()
            .iter()
            .find(|elevator| {
                elevator.pending_physical_trip() && {
                    let frame = CabinFrame::from_elevator(elevator);
                    self.items.world.snapshots().iter().any(|body| {
                        body.motion.is_movable()
                            && self
                                .items
                                .world
                                .body_bounds(body.key)
                                .is_some_and(|(min, max)| frame.contains_bounds(min, max))
                    }) || self.game.entities().ai().agents().iter().any(|agent| {
                        let (min, max) = agent_bounds(agent);
                        frame.contains_bounds(min, max)
                    }) || self.items.world.ragdoll_snapshots().iter().any(|actor| {
                        actor.world_positions.iter().any(|p| {
                            let local = frame.local(Vec3::from_array(*p));
                            local.x.abs() < 1.2
                                && local.z > 0.36
                                && local.z < 2.4
                                && local.y >= 0.0
                                && local.y < crate::engine::elevator::HEIGHT
                        })
                    })
                }
            })
            .map(|elevator| elevator.def.id.clone());
        if let Some(id) = blocked {
            self.game
                .entities_mut()
                .finish_local_elevator_departure(&id, None);
            self.elevator_notice("Use linked elevator cabins to carry props and NPCs. Advanced moving shafts currently support the player alone.");
        }
    }

    pub(super) fn finish_local_elevator_cargo(&mut self) {
        let Some(trip) = self.game.take_local_elevator_trip() else {
            return;
        };
        if let Err(problem) = self.move_local_cargo(&trip) {
            self.game.rollback_local_elevator_trip(&trip);
            self.synchronize_physical_player();
            self.sync_mechanisms();
            self.elevator_notice(&problem);
        }
    }

    fn move_local_cargo(
        &mut self,
        trip: &crate::game::elevator_transport::LocalTrip,
    ) -> Result<(), String> {
        let loaded = self
            .current_level
            .as_ref()
            .ok_or("Local elevator has no level")?;
        let mut ledger = self.elevator_cargo.clone();
        let transfer = ledger.capture(
            &loaded.level,
            self.game,
            &self.items,
            &trip.from,
            &trip.target_id,
        )?;
        let turn = trip.from.turn_to(&trip.to);
        let mut states = Vec::new();
        let mut physical = Vec::new();
        let mut aliases = Vec::new();
        for record in ledger.entries.iter().filter(|r| r.host == loaded.level.id) {
            let live = if record.origin_level == loaded.level.id
                && self.game.entities().handle_of(&record.origin_id).is_some()
            {
                record.origin_id.clone()
            } else {
                record.id.clone()
            };
            aliases.push((record.id.clone(), live));
        }
        for record in ledger
            .entries
            .iter_mut()
            .filter(|r| transfer.selected.contains(&r.id))
        {
            record.position = crate::engine::elevator::map_cabin_point(
                trip.from.origin,
                trip.to.origin,
                turn,
                record.position,
            );
            record.orientation = turn.mul_quat(record.orientation).normalize();
            record
                .state
                .relocate(&record.id, trip.from.origin, trip.to.origin, turn);
            record.physical = record
                .physical
                .as_ref()
                .map(|p| p.transported(trip.from.origin, trip.to.origin, turn));
            let live = aliases
                .iter()
                .find(|(id, _)| *id == record.id)
                .map(|(_, id)| id.clone())
                .ok_or("Passenger alias missing")?;
            check_local_npc(self.game, &self.items, record, &live, &aliases, &transfer)?;
            let mut state = record.state.clone();
            state.set_pose(
                &live,
                record.position,
                record
                    .orientation
                    .to_euler(glam::EulerRot::YXZ)
                    .0
                    .to_degrees(),
            );
            state.remap_targets(&aliases);
            states.push(state);
            if let Some(item) = &record.physical {
                physical.push((live, item.clone()));
            }
        }
        // Validate semantic restoration before changing any resident owner.
        let mut witness = crate::engine::entities::EntityWorld::from_level(&loaded.level);
        for state in &states {
            witness.restore_passenger(state)?;
        }
        let selected: Vec<_> = self
            .items
            .authored
            .iter()
            .filter(|(_, id, _)| physical.iter().any(|(name, _)| name == id))
            .cloned()
            .collect();
        self.synchronize_physical_player();
        check_local_residents(
            self.game,
            &self.items,
            &selected,
            &physical,
            &aliases,
            &transfer,
        )?;
        self.items.world.restore_transport(&selected, &physical)?;
        for state in &states {
            self.game.entities_mut().restore_passenger(state)?;
        }
        self.elevator_cargo = ledger;
        self.update_item_triggers(true);
        self.trace_world("elevator_cargo_transferred");
        Ok(())
    }

    pub(super) fn cargo_visit(&self, source: &super::loading::Source) -> Result<Visit, String> {
        if let Some((_, snapshot)) = &self.checkpoints.resume {
            return Ok(Visit {
                ledger: snapshot.elevator_cargo.clone(),
                transfer: None,
            });
        }
        let mut ledger = self.elevator_cargo.clone();
        let mut transfer = None;
        if let Some(loaded) = self.current_level.as_ref() {
            let path = match source {
                super::loading::Source::Entry(e, _) => &e.path,
                super::loading::Source::Retained(l, _) => &l.entry.path,
            };
            if *path != loaded.entry.path {
                ledger = Ledger::default();
            } else if let Some(arrival) = &self.elevator_arrival {
                transfer = Some(ledger.capture(
                    &loaded.level,
                    self.game,
                    &self.items,
                    &arrival.frame,
                    &arrival.id,
                )?);
            } else {
                ledger.refresh(&loaded.level, self.game, &self.items)?;
            }
        }
        Ok(Visit { ledger, transfer })
    }
}

fn check_local_residents(
    game: &crate::game::Game,
    items: &Items,
    selected: &[(u64, String, String)],
    physical: &[(String, crate::engine::physics::checkpoint::Item)],
    aliases: &[(String, String)],
    transfer: &Transfer,
) -> Result<(), String> {
    if !items
        .world
        .player_pose_clear(super::item_runtime::player_context(game))
    {
        return Err("The arrival cabin is obstructed at the rider's position".into());
    }
    for agent in game.entities().ai().agents().iter().filter(|agent| {
        !aliases.iter().any(|(canonical, id)| {
            transfer.selected.contains(canonical) && *id == agent.instance_id
        })
    }) {
        check_npc_player(
            game,
            agent.position,
            agent.profile.radius,
            agent.profile.height,
        )?;
        if !items.world.transport_actor_clear(
            selected,
            physical,
            agent.position,
            agent.profile.height,
            agent.profile.radius,
        )? {
            return Err(format!(
                "An NPC is occupying the arrival space for transported props: '{}'",
                agent.instance_id
            ));
        }
    }
    Ok(())
}

fn check_local_npc(
    game: &crate::game::Game,
    items: &Items,
    record: &Traveller,
    live: &str,
    aliases: &[(String, String)],
    transfer: &Transfer,
) -> Result<(), String> {
    if let Some(agent) = game.entities().ai().agent(live) {
        if !items.world.passenger_space_clear(
            record.position,
            agent.profile.height,
            agent.profile.radius,
        ) {
            return Err(format!(
                "Elevator destination is obstructed for NPC '{live}'"
            ));
        }
        check_npc_player(
            game,
            record.position,
            agent.profile.radius,
            agent.profile.height,
        )?;
        if !game.body_fits_for(record.position, agent.profile.height, agent.profile.radius) {
            return Err(format!(
                "Elevator destination is obstructed for NPC '{live}'"
            ));
        }
        for other in game.entities().ai().agents().iter().filter(|a| {
            !aliases.iter().any(|(canonical, id)| {
                transfer.selected.contains(canonical) && *id == a.instance_id
            })
        }) {
            if record.position.y < other.position.y + other.profile.height
                && other.position.y < record.position.y + agent.profile.height
                && Vec3::new(
                    record.position.x - other.position.x,
                    0.0,
                    record.position.z - other.position.z,
                )
                .length()
                    < agent.profile.radius + other.profile.radius
            {
                return Err("An NPC is occupying the arrival cabin".into());
            }
        }
    }
    Ok(())
}

fn check_npc_player(
    game: &crate::game::Game,
    position: Vec3,
    radius: f32,
    height: f32,
) -> Result<(), String> {
    let player = game.player_position;
    let half = game.body_half();
    let near_x = position.x.clamp(player.x - half, player.x + half);
    let near_z = position.z.clamp(player.z - half, player.z + half);
    let dx = position.x - near_x;
    let dz = position.z - near_z;
    if position.y < game.feet_y() + game.body_height()
        && game.feet_y() < position.y + height
        && dz.mul_add(dz, dx * dx) < radius.mul_add(radius, -0.0001)
    {
        return Err("An NPC would overlap the rider at arrival".into());
    }
    Ok(())
}

fn check_npc_space(
    game: &crate::game::Game,
    id: &str,
    position: Vec3,
    radius: f32,
    height: f32,
) -> Result<(), String> {
    check_npc_player(game, position, radius, height)?;
    if !game.body_fits_for(position, height, radius) {
        return Err(format!("Arrival is obstructed for NPC '{id}'"));
    }
    for other in game
        .entities()
        .ai()
        .agents()
        .iter()
        .filter(|a| a.instance_id != id)
    {
        let dx = position.x - other.position.x;
        let dz = position.z - other.position.z;
        let distance = radius + other.profile.radius;
        if position.y < other.position.y + other.profile.height
            && other.position.y < position.y + height
            && dz.mul_add(dz, dx * dx) < distance.mul_add(distance, -0.0001)
        {
            return Err("NPC passengers overlap at arrival".into());
        }
    }
    Ok(())
}
