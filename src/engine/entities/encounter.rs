//! Opt-in, bounded encounter ownership over the ordinary spawn/AI/health systems.
use super::components::StateValue;
use super::{EntityWorld, WorldContext};
use crate::engine::entity::PoseCue;
use crate::engine::{
    health::{HealthAction, HealthTarget},
    level::LevelDef,
};
use glam::Vec3;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EncounterDef {
    #[serde(default = "one_companion", skip_serializing_if = "is_one_companion")]
    pub companion_count: u8,
    pub control: String,
    pub companion_point: String,
}

pub const MAX_COMPANIONS: u8 = 5;
const fn one_companion() -> u8 {
    1
}
// Serde's skip predicate requires a reference to the serialized field.
#[allow(
    clippy::trivially_copy_pass_by_ref,
    reason = "Serde requires a reference predicate"
)]
const fn is_one_companion(count: &u8) -> bool {
    *count == 1
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SpawnZoneDef {
    pub cooldown_seconds: f32,
    pub count: u16,
    pub width: f32,
    pub depth: f32,
}
impl Default for SpawnZoneDef {
    fn default() -> Self {
        Self {
            cooldown_seconds: 1.0,
            count: 10,
            width: 2.0,
            depth: 2.0,
        }
    }
}

struct Zone {
    point: String,
    rules: SpawnZoneDef,
    elapsed: f32,
    spawned: u16,
}
struct Actor {
    id: String,
    dead: bool,
    cooldown: f32,
    fast: bool,
    target_robot: Option<String>,
    swing: Option<Swing>,
}
struct Swing {
    interval: f32,
    pending: bool,
    target: HealthTarget,
}
struct AttackIntent {
    goal: Vec3,
    speed: f32,
    reach: f32,
    interval: f32,
    target: HealthTarget,
    hp: u16,
}
pub(super) struct Encounter {
    definition: EncounterDef,
    zones: Vec<Zone>,
    zombies: Vec<Actor>,
    robots: Vec<Actor>,
    started: bool,
    round: u64,
    announcement_seconds: f32,
    failure: Option<String>,
}
impl Encounter {
    pub(super) fn from_level(level: &LevelDef) -> Option<Self> {
        Some(Self {
            definition: level.encounter.clone()?,
            zones: level
                .spawn_points
                .iter()
                .filter_map(|p| {
                    p.zone.map(|rules| Zone {
                        point: p.id.clone(),
                        rules,
                        elapsed: 0.0,
                        spawned: 0,
                    })
                })
                .collect(),
            zombies: Vec::with_capacity(20),
            robots: Vec::with_capacity(usize::from(MAX_COMPANIONS)),
            started: false,
            round: 0,
            announcement_seconds: 0.0,
            failure: None,
        })
    }
}

/// # Errors
/// Rejects unbounded spawns, invalid clocks and missing opt-in health.
pub fn validate(level: &LevelDef) -> Result<(), String> {
    let mut total = 0_u16;
    for point in &level.spawn_points {
        if let Some(zone) = point.zone {
            if !zone.cooldown_seconds.is_finite()
                || !(0.1..=3600.0).contains(&zone.cooldown_seconds)
                || zone.count == 0
                || zone.count > 20
                || ![zone.width, zone.depth]
                    .into_iter()
                    .all(|v| v.is_finite() && (0.5..=16.0).contains(&v))
            {
                return Err("Spawn zones require 1..20 actors, 0.1..3600 second cooldown and 0.5..16 metre sides".into());
            }
            total = total.saturating_add(zone.count);
        }
    }
    if let Some(def) = &level.encounter {
        if !(1..=MAX_COMPANIONS).contains(&def.companion_count) {
            return Err("Encounter companion_count must be between 1 and 5".into());
        }
        if !level.health.enabled || total == 0 || total > 20 {
            return Err(
                "An encounter requires explicitly enabled health and 1..20 total zone spawns"
                    .into(),
            );
        }
        if !level
            .spawn_points
            .iter()
            .any(|p| p.id == def.companion_point && p.zone.is_none())
        {
            return Err("Encounter companion_point must name an ordinary spawn point".into());
        }
        crate::engine::project::validate_id(&def.control)?;
        if level.saving {
            return Err("Encounter checkpoints are not supported; disable map saving".into());
        }
        let control = level
            .props
            .iter()
            .find(|p| p.id.as_deref() == Some(&def.control))
            .ok_or("Encounter control must name a placed prop")?;
        for key in ["start", "clear", "companion"] {
            if !control.components.iter().any(|c| matches!(c, crate::engine::level::ComponentDef::State { name, value: StateValue::Bool(false) } if name == key)) {
                return Err(format!("Encounter control requires false boolean state `{key}`"));
            }
        }
        for point in level
            .spawn_points
            .iter()
            .filter(|p| p.zone.is_some() || p.id == def.companion_point)
        {
            let template = level
                .spawn_templates
                .iter()
                .find(|t| t.id == point.template)
                .ok_or("Encounter point requires a spawn template")?;
            if template.lifetime_seconds.is_some() || template.shadow_only
                || !matches!(template.model.as_str(), "mannequin" | "core:mannequin_black")
                || !template.components.iter().any(|c| matches!(c, crate::engine::level::ComponentDef::Ai(_)))
                || !template.components.iter().any(|c| matches!(c, crate::engine::level::ComponentDef::Health { starting_hp, max_hp: _ } if *starting_hp > 0))
            { return Err("Encounter templates require a visible mannequin with AI, positive authored health and no lifetime".into()); }
            if point.group.as_ref().is_some_and(|id| {
                level
                    .spawn_groups
                    .iter()
                    .any(|g| &g.id == id && g.at_most_one_active)
            }) {
                return Err("Encounter points cannot use exclusive spawn groups".into());
            }
        }
    }
    Ok(())
}

impl EntityWorld {
    #[must_use]
    pub const fn has_encounter(&self) -> bool {
        self.encounter.is_some()
    }

    pub fn clear_encounter(&mut self) {
        if let Some(mut encounter) = self.encounter.take() {
            self.clear_zombies(&mut encounter);
            self.clear_companions(&mut encounter);
            self.encounter = Some(encounter);
        }
    }
    fn clear_companions(&mut self, encounter: &mut Encounter) {
        for robot in encounter.robots.drain(..) {
            self.remove_combat_actor(&robot.id);
        }
    }
    fn toggle_companions(&mut self, encounter: &mut Encounter) {
        if !encounter.robots.is_empty() {
            self.clear_companions(encounter);
            return;
        }
        let count = encounter.definition.companion_count;
        for index in 0..count {
            let id = if index == 0 {
                "encounter:robot".into()
            } else {
                format!("encounter:robot:{index}")
            };
            let offset = Vec3::new(
                f32::from(count.saturating_sub(1)).mul_add(-0.5, f32::from(index)) * 1.4,
                0.0,
                0.0,
            );
            match self.spawn_combat_actor(
                &encounter.definition.companion_point,
                &id,
                false,
                Some(offset),
            ) {
                Ok(robot) => encounter.robots.push(robot),
                Err(error) => {
                    self.clear_companions(encounter);
                    encounter.failure = Some(error);
                    break;
                }
            }
        }
    }
    fn remove_combat_actor(&mut self, id: &str) {
        if let Some(handle) = self.handle_of(id) {
            let _removed = self.despawn_handle(handle);
        }
    }
    fn clear_zombies(&mut self, encounter: &mut Encounter) {
        for actor in encounter.zombies.drain(..) {
            self.remove_combat_actor(&actor.id);
        }
        for zone in &mut encounter.zones {
            zone.elapsed = 0.0;
            zone.spawned = 0;
        }
        encounter.started = false;
        encounter.announcement_seconds = 0.0;
        encounter.failure = None;
    }
    fn encounter_signal(&mut self, control: &str, key: &str) -> bool {
        let Some(handle) = self.handle_of(control) else {
            return false;
        };
        let Some(state) = self.components.states.get_mut(handle) else {
            return false;
        };
        if state.get(key).and_then(StateValue::as_bool) != Some(true) {
            return false;
        }
        let _reset = state.set(key, StateValue::Bool(false));
        true
    }
    pub(super) fn tick_encounter(&mut self, ctx: &WorldContext<'_>) -> bool {
        if !self.health_settings.enabled {
            return false;
        }
        let Some(mut encounter) = self.encounter.take() else {
            return false;
        };
        encounter.announcement_seconds =
            (encounter.announcement_seconds - ctx.delta_seconds).max(0.0);
        if self.encounter_signal(&encounter.definition.control, "clear") {
            self.clear_zombies(&mut encounter);
        }
        if self.encounter_signal(&encounter.definition.control, "companion") {
            self.toggle_companions(&mut encounter);
        }
        if self.encounter_signal(&encounter.definition.control, "start") && !encounter.started {
            encounter.started = true;
            encounter.round = encounter.round.wrapping_add(1);
            encounter.announcement_seconds = 3.0;
        }
        if encounter.started {
            self.spawn_encounter_zombies(&mut encounter, ctx.delta_seconds);
        }
        self.advance_combat(&mut encounter, ctx);
        self.encounter = Some(encounter);
        true
    }
    fn spawn_combat_actor(
        &mut self,
        point: &str,
        id: &str,
        fast: bool,
        offset: Option<Vec3>,
    ) -> Result<Actor, String> {
        let mut authored = self
            .spawn_points
            .iter()
            .find(|p| p.id == point)
            .cloned()
            .ok_or("Missing encounter spawn point")?;
        if let Some(displacement) = offset {
            let rotated = glam::Quat::from_rotation_y(authored.yaw_degrees.to_radians())
                .mul_vec3(displacement);
            authored.x += rotated.x;
            authored.z += rotated.z;
        }
        let _handle = self
            .spawn_at_point(&authored, None, None, Some(id), 0)
            .map_err(|error| format!("Encounter spawn {id}: {error:?}"))?;
        Ok(Actor {
            id: id.into(),
            dead: false,
            cooldown: 0.0,
            fast,
            target_robot: None,
            swing: None,
        })
    }
    fn spawn_encounter_zombies(&mut self, encounter: &mut Encounter, delta: f32) {
        for zone in &mut encounter.zones {
            if zone.spawned >= zone.rules.count {
                continue;
            }
            zone.elapsed += delta;
            if zone.elapsed < zone.rules.cooldown_seconds {
                continue;
            }
            let index = encounter.zombies.len();
            let id = format!("encounter:zombie:{index}");
            let phase = u64::try_from(index)
                .unwrap_or(0)
                .wrapping_mul(7)
                .wrapping_add(encounter.round);
            // Cycle four inset positions; source points remain unchanged and clocks independent.
            let corner = zone.spawned % 4;
            let offset = Vec3::new(
                if corner % 2 == 0 { -0.25 } else { 0.25 } * zone.rules.width,
                0.0,
                if corner < 2 { -0.25 } else { 0.25 } * zone.rules.depth,
            );
            match self.spawn_combat_actor(&zone.point, &id, phase % 5 == 0, Some(offset)) {
                Ok(actor) => {
                    encounter.zombies.push(actor);
                    zone.spawned = zone.spawned.saturating_add(1);
                    zone.elapsed = (zone.elapsed - zone.rules.cooldown_seconds).max(0.0);
                }
                Err(error) => {
                    encounter.failure = Some(error);
                    encounter.started = false;
                    break;
                }
            }
        }
    }
    fn combat_position(&self, id: &str) -> Option<Vec3> {
        self.components
            .transforms
            .get(self.handle_of(id)?)
            .map(|t| t.position)
    }
    fn combat_alive(&mut self, actor: &mut Actor, delta: f32) -> bool {
        actor.cooldown = (actor.cooldown - delta).max(0.0);
        if actor.dead {
            return false;
        }
        if self
            .entity_health(&actor.id)
            .is_none_or(crate::engine::health::HealthState::is_dead)
        {
            actor.dead = true;
            let _requested = self.request_ragdoll(&actor.id, true, [0.0; 3]);
            return false;
        }
        true
    }
    fn advance_combat(&mut self, encounter: &mut Encounter, ctx: &WorldContext<'_>) {
        let robot_positions: Vec<_> = encounter
            .robots
            .iter_mut()
            .filter_map(|robot| {
                self.combat_alive(robot, ctx.delta_seconds)
                    .then(|| {
                        self.combat_position(&robot.id)
                            .map(|p| (robot.id.clone(), p))
                    })
                    .flatten()
            })
            .collect();
        for zombie in &mut encounter.zombies {
            if !self.combat_alive(zombie, ctx.delta_seconds) {
                continue;
            }
            let Some(position) = self.combat_position(&zombie.id) else {
                continue;
            };
            let nearest_robot = robot_positions
                .iter()
                .min_by(|(_, a), (_, b)| {
                    position
                        .distance_squared(*a)
                        .total_cmp(&position.distance_squared(*b))
                })
                .filter(|(_, p)| {
                    position.distance_squared(*p) < position.distance_squared(ctx.feet)
                });
            zombie.target_robot = nearest_robot.map(|(id, _)| id.clone());
            let goal = nearest_robot.map_or(ctx.feet, |(_, p)| *p);
            let target = zombie
                .target_robot
                .as_ref()
                .map_or(HealthTarget::Player, |id| HealthTarget::Entity {
                    id: id.clone(),
                });
            let speed = ctx.player_walk_speed * if zombie.fast { 1.1 } else { 0.9 };
            self.combat_step(
                zombie,
                position,
                &AttackIntent {
                    goal,
                    speed,
                    reach: 1.25,
                    interval: 1.0,
                    target,
                    hp: 1,
                },
                ctx,
            );
        }
        for robot in &mut encounter.robots {
            if !robot.dead {
                self.advance_companion(robot, &encounter.zombies, ctx);
            }
        }
    }
    fn advance_companion(&mut self, robot: &mut Actor, zombies: &[Actor], ctx: &WorldContext<'_>) {
        let Some(position) = self.combat_position(&robot.id) else {
            return;
        };
        let target = zombies
            .iter()
            .filter(|z| !z.dead)
            .filter_map(|z| self.combat_position(&z.id).map(|p| (z, p)))
            .min_by(|(a, pa), (b, pb)| {
                let pursues_robot =
                    |z: &Actor| z.target_robot.as_deref() == Some(robot.id.as_str());
                pursues_robot(b).cmp(&pursues_robot(a)).then_with(|| {
                    position
                        .distance_squared(*pa)
                        .total_cmp(&position.distance_squared(*pb))
                })
            });
        if let Some((zombie, goal)) = target {
            let rules = self.tool_settings;
            self.combat_step(
                robot,
                position,
                &AttackIntent {
                    goal,
                    speed: ctx.player_walk_speed,
                    reach: rules.reach_m,
                    interval: rules.swings_per_second.recip(),
                    target: HealthTarget::Entity {
                        id: zombie.id.clone(),
                    },
                    hp: rules.damage_hp,
                },
                ctx,
            );
        } else {
            robot.swing = None;
            let destination = if position.distance(ctx.feet) > 2.5 {
                ctx.feet
            } else {
                position
            };
            let _goal = self.ai.set_navigation_goal_with_speed(
                &robot.id,
                destination,
                Some(ctx.player_walk_speed),
            );
        }
    }
    fn combat_step(
        &mut self,
        actor: &mut Actor,
        position: Vec3,
        intent: &AttackIntent,
        ctx: &WorldContext<'_>,
    ) {
        let close = position.distance(intent.goal) <= intent.reach;
        let destination = if close || actor.swing.is_some() {
            position
        } else {
            intent.goal
        };
        let _goal = self.ai.set_navigation_goal_with_speed(
            &actor.id,
            destination,
            Some(intent.speed.clamp(0.05, 8.0)),
        );
        if actor.cooldown <= 0.0 {
            actor.swing = None;
            if close {
                actor.cooldown = intent.interval;
                actor.swing = Some(Swing {
                    interval: intent.interval,
                    pending: true,
                    target: intent.target.clone(),
                });
            }
        }
        let Some(swing) = actor.swing.as_mut() else {
            return;
        };
        if !swing.pending || actor.cooldown > swing.interval * 0.68 {
            return;
        }
        swing.pending = false;
        // A single contact per swing, rechecked against the original target at contact time.
        if !close || swing.target != intent.target {
            return;
        }
        let from = Vec3::new(position.x, position.y + 1.0, position.z);
        let to = Vec3::new(intent.goal.x, intent.goal.y + 1.0, intent.goal.z);
        if crate::engine::ai::perception::sight_clear_with_solids(
            ctx.index,
            ctx.walls,
            self.door_colliders(),
            &self.navigation_obstacles,
            from,
            to,
            false,
        ) {
            let _hit = self.dispatch_health(&intent.target, HealthAction::Damage { hp: intent.hp });
        }
    }
    pub(super) fn encounter_pose(&self, id: &str, moving: bool) -> Option<PoseCue> {
        let e = self.encounter.as_ref()?;
        if e.zombies.iter().any(|z| z.id == id && !z.dead) {
            return Some(PoseCue::Clip {
                name: if moving {
                    "run_arms_forward"
                } else {
                    "pose_arms_forward"
                }
                .into(),
                once: false,
                paused: false,
            });
        }
        let robot = e.robots.iter().find(|a| a.id == id && !a.dead)?;
        let Some(swing) = robot.swing.as_ref() else {
            return Some(PoseCue::Clip {
                name: if moving {
                    "crowbar_walk"
                } else {
                    "crowbar_idle"
                }
                .into(),
                once: false,
                paused: false,
            });
        };
        Some(PoseCue::Sample {
            name: "crowbar_swing".into(),
            progress: (1.0 - robot.cooldown / swing.interval).clamp(0.0, 1.0),
        })
    }
    #[must_use]
    pub fn companion_tool_id(&self, index: u8) -> Option<&str> {
        self.encounter
            .as_ref()?
            .robots
            .get(usize::from(index))
            .filter(|r| !r.dead)
            .map(|r| r.id.as_str())
    }
    #[must_use]
    pub fn encounter_subtitle(&self) -> Option<&str> {
        let e = self.encounter.as_ref()?;
        e.failure
            .as_deref()
            .or_else(|| (e.announcement_seconds > 0.0).then_some("zombies incoming"))
    }

    #[must_use]
    pub fn encounter_snapshot(&self) -> serde_json::Value {
        self.encounter.as_ref().map_or(serde_json::Value::Null,|e|serde_json::json!({
            "started":e.started,"round":e.round,"failure":e.failure,"entities":self.store.len(),"components":self.components.len(),"bindings":self.bindings.len(),"live_spawns":self.live_spawns.len(),"ai_agents":self.ai.agents().len(),"dynamic_keys":self.dynamic_keys.len(),"zombies":e.zombies.iter().map(|z|serde_json::json!({"id":z.id,"dead":z.dead,"fast":z.fast,"targets_robot":z.target_robot.is_some(),"target_robot":z.target_robot})).collect::<Vec<_>>(),
            "robot":e.robots.first().map(|r|serde_json::json!({"id":r.id,"dead":r.dead,"swing_remaining":r.cooldown})),
            "robots":e.robots.iter().map(|r|serde_json::json!({"id":r.id,"dead":r.dead,"swing_remaining":r.cooldown})).collect::<Vec<_>>(),
            "zones":e.zones.iter().map(|z|serde_json::json!({"id":z.point,"spawned":z.spawned,"elapsed":z.elapsed})).collect::<Vec<_>>() }))
    }
}

#[cfg(test)]
#[path = "../tests/encounter.rs"]
mod tests;
