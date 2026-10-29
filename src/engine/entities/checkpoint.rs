//! Stable semantic checkpoints. No engine handles, definitions or executable data.
use super::{EntityWorld, StateValue, WorldCommand};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntityState {
    id: String,
    transform: Option<([f32; 3], f32)>,
    visible: Option<bool>,
    solid: Option<bool>,
    interaction: Option<(bool, bool, String)>,
    state: Option<Vec<(String, StateValue)>>,
    light: Option<bool>,
    material: Option<String>,
    audio: Option<(bool, bool)>,
    effect: Option<bool>,
    water: Option<bool>,
    fade: Option<(bool, f32, bool)>,
    glow: Option<bool>,
    animation: Option<super::components::Animation>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    time: f32,
    entities: Vec<EntityState>,
    bindings: Vec<(String, bool, f32)>,
    doors: Vec<crate::engine::door::Checkpoint>,
    timers: Vec<super::timers::Checkpoint>,
    routes: Vec<crate::engine::entity::RouteState>,
    poses: Vec<(String, crate::engine::entity::PoseCue)>,
    circuits: Vec<(String, super::light_power::Power, bool)>,
    scripts: crate::engine::scripting::Checkpoint,
}

pub(crate) fn number(value: f32, limit: f32) -> Result<(), String> {
    if value.is_finite() && value.abs() <= limit {
        Ok(())
    } else {
        Err("Checkpoint contains an out-of-range number".into())
    }
}

pub(crate) fn state_value(scalar: &StateValue) -> Result<(), String> {
    match scalar {
        StateValue::Bool(_) => Ok(()),
        StateValue::Int(value) if value.unsigned_abs() <= 2_147_483_647 => Ok(()),
        StateValue::Int(_) => Err("Checkpoint integer exceeds supported range".into()),
        StateValue::Float(value) => number(*value, 1_000_000.0),
        StateValue::Text(value) if value.len() <= 256 && !value.chars().any(char::is_control) => {
            Ok(())
        }
        StateValue::Text(_) => Err("Checkpoint text exceeds supported range".into()),
    }
}

impl EntityWorld {
    #[must_use]
    pub fn checkpoint_events_settled(&self) -> bool {
        self.events.is_empty() && self.commands.is_empty()
    }

    fn checkpoint_ready(&self) -> Result<(), String> {
        if !self.events.is_empty()
            || !self.commands.is_empty()
            || !self.moves.is_empty()
            || !self.sequence_runs.is_empty()
        {
            return Err("Checkpoint needs a settled event, move and sequence boundary".into());
        }
        if self.store.len() != self.checkpoint_entity_count
            || !self.live_spawns.is_empty()
            || !self.components.ais.is_empty()
            || !self.animated_doors.is_empty()
            || !self.physics_actors.is_empty()
            || !self.components.lifetimes.is_empty()
            || !self.components.balloons.is_empty()
            || self
                .components
                .carries
                .iter()
                .any(|(_, carry)| carry.item.is_some() || carry.retired)
        {
            return Err("Checkpoint does not yet support AI, live spawns, lifetimes, balloons, articulated/clip doors or transferred items".into());
        }
        if self.weather_strength != 0.0 || self.weather_target != 0.0 || self.weather_cycle_enabled
        {
            return Err("Checkpoint needs default weather with its cycle stopped".into());
        }
        if self
            .components
            .animations
            .iter()
            .any(|(_, animation)| animation.playing && !animation.looped)
        {
            return Err("Checkpoint needs one-shot animations to finish".into());
        }
        Ok(())
    }

    /// # Errors
    /// Refuses unsupported or unsettled state; the prior disk save remains valid.
    pub fn checkpoint(&self) -> Result<Snapshot, String> {
        self.checkpoint_ready()?;
        let entities =
            self.store
                .handles()
                .into_iter()
                .map(|handle| {
                    let components = &self.components;
                    EntityState {
                        id: self.id_of(handle).unwrap_or_default().to_string(),
                        transform: components.transforms.get(handle).map(|transform| {
                            (transform.position.to_array(), transform.yaw_degrees)
                        }),
                        visible: components
                            .renderables
                            .get(handle)
                            .map(|value| value.visible),
                        solid: components.colliders.get(handle).map(|value| value.solid),
                        interaction: components.interactables.get(handle).map(|value| {
                            (value.enabled, value.label_visible, value.prompt.clone())
                        }),
                        state: components
                            .states
                            .get(handle)
                            .map(|value| value.values.clone()),
                        light: components.lights.get(handle).map(|value| value.enabled),
                        material: components
                            .materials
                            .get(handle)
                            .map(|value| value.variant.clone()),
                        audio: components
                            .audio
                            .get(handle)
                            .map(|value| (value.enabled, value.playing)),
                        effect: components.steam.get(handle).map(|value| value.enabled),
                        water: components.water.get(handle).map(|value| value.enabled),
                        fade: components
                            .fades
                            .get(handle)
                            .map(|value| (value.enabled, value.live_opacity, value.fading_out)),
                        glow: components.glows.get(handle).map(|value| value.enabled),
                        animation: components.animations.get(handle).cloned(),
                    }
                })
                .collect();
        Ok(Snapshot {
            time: self.sim_time,
            entities,
            bindings: self
                .bindings
                .iter()
                .map(|binding| {
                    (
                        self.id_of(binding.owner).unwrap_or_default().into(),
                        binding.fired,
                        binding.cooldown_remaining,
                    )
                })
                .collect(),
            doors: self.doors.checkpoint(),
            timers: self.timers.checkpoint(),
            routes: self.route_states.clone(),
            poses: self.animation_overrides.clone(),
            circuits: self
                .circuit_power
                .iter()
                .map(|circuit| {
                    (
                        circuit
                            .members
                            .first()
                            .and_then(|handle| self.id_of(*handle))
                            .unwrap_or_default()
                            .into(),
                        circuit.power,
                        circuit.requested,
                    )
                })
                .collect(),
            scripts: self.scripts.checkpoint()?,
        })
    }

    /// Apply only to a private, newly resolved world before publication.
    /// # Errors
    /// Validates IDs, shape, declared state kinds and bounded data. Emits no events.
    pub fn restore_checkpoint(&mut self, snapshot: &Snapshot) -> Result<(), String> {
        number(snapshot.time, 10_000_000.0)?;
        let entity_count_matches = snapshot.entities.len() == self.store.len();
        let route_count_matches = snapshot.routes.len() == self.route_states.len();
        if snapshot.time < 0.0
            || !entity_count_matches
            || snapshot.bindings.len() != self.bindings.len()
            || !route_count_matches
            || snapshot.circuits.len() != self.circuit_power.len()
            || snapshot.poses.len() > snapshot.entities.len()
        {
            return Err("Checkpoint world shape does not match this level".into());
        }
        let mut seen = BTreeSet::new();
        for record in &snapshot.entities {
            if !seen.insert(&record.id) {
                return Err("Duplicate checkpoint entity".into());
            }
            self.restore_entity(record)?;
        }
        for (binding, (owner, fired, remaining)) in self.bindings.iter_mut().zip(&snapshot.bindings)
        {
            if self.names.id_of(binding.owner).map(super::EntityId::as_str) != Some(owner.as_str())
            {
                return Err("Checkpoint binding identity mismatch".into());
            }
            number(*remaining, binding.cooldown_seconds)?;
            if *remaining < 0.0 {
                return Err("Negative checkpoint cooldown".into());
            }
            binding.fired = *fired;
            binding.cooldown_remaining = *remaining;
        }
        self.cooling_bindings = self
            .bindings
            .iter()
            .enumerate()
            .filter_map(|(index, binding)| (binding.cooldown_remaining > 0.0).then_some(index))
            .collect();
        self.doors.restore_checkpoint(&snapshot.doors)?;
        self.timers.restore_checkpoint(&snapshot.timers)?;
        // Route state contains value data only; route identity is fixed by the exact content fingerprint.
        for (route, state) in self.routes.routes().iter().zip(&snapshot.routes) {
            validate_pose(&state.cue)?;
            for value in state.position.to_array() {
                number(value, 100_000.0)?;
            }
            number(state.yaw, 100_000.0)?;
            number(state.step_time, 10_000_000.0)?;
            if state.step > route.steps.len() || state.step_time < 0.0 {
                return Err("Invalid checkpoint route".into());
            }
        }
        self.route_states.clone_from(&snapshot.routes);
        let mut pose_ids = BTreeSet::new();
        for (id, cue) in &snapshot.poses {
            if self.handle_of(id).is_none() || !pose_ids.insert(id) {
                return Err("Invalid checkpoint pose identity".into());
            }
            validate_pose(cue)?;
        }
        self.animation_overrides.clone_from(&snapshot.poses);
        self.scripted_poses = snapshot.poses.iter().map(|(id, _)| id.clone()).collect();
        for (circuit, (id, power, requested)) in
            self.circuit_power.iter_mut().zip(&snapshot.circuits)
        {
            if circuit
                .members
                .first()
                .and_then(|handle| self.names.id_of(*handle))
                .map(super::EntityId::as_str)
                != Some(id.as_str())
            {
                return Err("Checkpoint circuit identity mismatch".into());
            }
            circuit.power = *power;
            circuit.requested = *requested;
        }
        self.scripts.restore_checkpoint(&snapshot.scripts)?;
        self.publish_checkpoint(snapshot.time);
        Ok(())
    }

    fn publish_checkpoint(&mut self, time: f32) {
        self.sim_time = time;
        self.events.clear();
        self.commands.clear();
        self.rebuild_door_colliders();
        self.rebuild_interactables_from_tables();
        self.rebuild_entity_frames();
        // Publish existing effects and loops once; do not replay one-shot audio or startup handlers.
        let effects: Vec<_> = self
            .components
            .steam
            .iter()
            .filter_map(|(_, effect)| {
                effect.effect.map(|index| WorldCommand::SetEffectEnabled {
                    index,
                    enabled: effect.enabled,
                })
            })
            .collect();
        self.commands.extend(effects);
        let water: Vec<_> = self
            .components
            .water
            .iter()
            .filter_map(|(handle, water)| {
                self.water_volume_index(handle)
                    .map(|index| WorldCommand::SetWaterEnabled {
                        index,
                        enabled: water.enabled,
                    })
            })
            .collect();
        self.commands.extend(water);
        let audio: Vec<_> = self
            .components
            .audio
            .iter()
            .filter_map(|(handle, audio)| {
                (audio.playing && audio.enabled && audio.looped).then_some(handle)
            })
            .collect();
        for handle in audio {
            self.queue_audio_start(handle);
        }
    }

    fn restore_entity(&mut self, record: &EntityState) -> Result<(), String> {
        let handle = self
            .handle_of(&record.id)
            .ok_or("Unknown checkpoint entity")?;
        let components = &mut self.components;
        macro_rules! shape {
            ($field:ident,$table:ident) => {
                if record.$field.is_some() != components.$table.contains(handle) {
                    return Err(format!(
                        "Checkpoint {} component mismatch on {}",
                        stringify!($field),
                        record.id
                    ));
                }
            };
        }
        shape!(transform, transforms);
        shape!(visible, renderables);
        shape!(solid, colliders);
        shape!(interaction, interactables);
        shape!(light, lights);
        shape!(material, materials);
        shape!(audio, audio);
        shape!(effect, steam);
        shape!(water, water);
        shape!(fade, fades);
        shape!(glow, glows);
        shape!(animation, animations);
        if let Some((position, yaw)) = record.transform {
            for value in position {
                number(value, 100_000.0)?;
            }
            number(yaw, 100_000.0)?;
            let transform = components
                .transforms
                .get_mut(handle)
                .ok_or("Missing transform")?;
            transform.position = glam::Vec3::from_array(position);
            transform.yaw_degrees = yaw;
        }
        macro_rules! flag {
            ($field:ident,$table:ident,$property:ident) => {
                match (record.$field, components.$table.get_mut(handle)) {
                    (Some(value), Some(component)) => component.$property = value,
                    (None, None) => {}
                    _ => return Err("Checkpoint component shape mismatch".into()),
                }
            };
        }
        flag!(visible, renderables, visible);
        flag!(solid, colliders, solid);
        flag!(light, lights, enabled);
        flag!(effect, steam, enabled);
        flag!(water, water, enabled);
        flag!(glow, glows, enabled);
        if let Some(light) = components.lights.get_mut(handle) {
            light.dirty = true;
        }
        if let Some((enabled, label, prompt)) = &record.interaction {
            if prompt.len() > 96 || prompt.chars().any(char::is_control) {
                return Err("Invalid checkpoint prompt".into());
            }
            let value = components
                .interactables
                .get_mut(handle)
                .ok_or("Missing interaction")?;
            value.enabled = *enabled;
            value.label_visible = *label;
            value.prompt.clone_from(prompt);
        }
        self.restore_entity_values(handle, record)
    }

    fn restore_state_bag(
        &mut self,
        handle: super::EntityHandle,
        record: &EntityState,
    ) -> Result<(), String> {
        let original = self.components.states.get(handle);
        let Some(values) = &record.state else {
            return if original.is_none() {
                Ok(())
            } else {
                Err("Missing checkpoint state bag".into())
            };
        };
        // Lights/enabling publish derived object_state keys. Headless volumes and
        // timers also accept script/sequence state without authored component lists.
        let headless =
            self.components.volumes.contains(handle) || self.timers.index_of(&record.id).is_some();
        let mut keys = BTreeSet::new();
        if values.len() > 128 {
            return Err("Checkpoint state key budget exceeded".into());
        }
        for (key, value) in values {
            let declared = original.and_then(|bag| bag.get(key));
            let derived =
                (key == "light" && self.components.lights.contains(handle)) || key == "enabled";
            if key.is_empty()
                || key.len() > 64
                || key.chars().any(char::is_control)
                || !keys.insert(key)
                || declared.map_or(
                    !(headless || derived && matches!(value, StateValue::Bool(_))),
                    |initial| initial.kind() != value.kind(),
                )
            {
                return Err("Checkpoint state kind mismatch".into());
            }
            state_value(value)?;
        }
        if original.is_some_and(|bag| bag.values.iter().any(|(key, _)| !keys.contains(key))) {
            return Err("Checkpoint state shape mismatch".into());
        }
        drop(self.components.states.insert(
            handle,
            super::components::ObjectState {
                values: values.clone(),
            },
        ));
        Ok(())
    }

    fn restore_entity_values(
        &mut self,
        handle: super::EntityHandle,
        record: &EntityState,
    ) -> Result<(), String> {
        self.restore_state_bag(handle, record)?;
        let components = &mut self.components;
        if let Some(variant) = &record.material {
            let target = components
                .materials
                .get_mut(handle)
                .ok_or("Missing material")?;
            if !target.variants.iter().any(|(name, _)| name == variant) {
                return Err("Unknown checkpoint material".into());
            }
            target.variant.clone_from(variant);
        }
        if let Some((enabled, playing)) = record.audio {
            let target = components.audio.get_mut(handle).ok_or("Missing audio")?;
            target.enabled = enabled;
            target.playing = playing;
        }
        if let Some((enabled, opacity, out)) = record.fade {
            let target = components.fades.get_mut(handle).ok_or("Missing fade")?;
            number(opacity, 1.0)?;
            if !(target.min_opacity..=target.max_opacity).contains(&opacity) {
                return Err("Invalid checkpoint opacity".into());
            }
            target.enabled = enabled;
            target.live_opacity = opacity;
            target.fading_out = out;
        }
        if let Some(animation) = &record.animation {
            if animation.clip.len() > 96 || animation.clip.chars().any(char::is_control) {
                return Err("Invalid checkpoint clip".into());
            }
            unit(animation.progress)?;
            number(animation.speed, 100.0)?;
            if animation.speed <= 0.0 {
                return Err("Invalid checkpoint animation rate".into());
            }
            let target = components
                .animations
                .get_mut(handle)
                .ok_or("Missing animation")?;
            target.clone_from(animation);
        }
        Ok(())
    }
}

fn validate_pose(cue: &crate::engine::entity::PoseCue) -> Result<(), String> {
    use crate::engine::entity::PoseCue;
    if let Some(name) = cue.clip_name()
        && (name.is_empty() || name.len() > 96 || name.chars().any(char::is_control))
    {
        return Err("Invalid checkpoint pose clip".into());
    }
    match cue {
        PoseCue::Idle
        | PoseCue::Clip {
            name: _,
            once: _,
            paused: _,
        } => Ok(()),
        PoseCue::Walk { speed_mps } => number(*speed_mps, 1000.0),
        PoseCue::Scrub { name: _, target } => unit(*target),
        PoseCue::Sample { name: _, progress } => unit(*progress),
    }
}

fn unit(value: f32) -> Result<(), String> {
    if value.is_finite() && (0.0..=1.0).contains(&value) {
        Ok(())
    } else {
        Err("Invalid checkpoint normalized progress".into())
    }
}
