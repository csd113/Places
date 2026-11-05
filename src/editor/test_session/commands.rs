//! Native presentation adapter for the shared entity command protocol.
use super::Session;
use crate::engine::audio::{AudioBus, AudioSystem, PlayRequest, SourceId, TimeDomain};
use crate::engine::entities::WorldCommand;
use crate::engine::render::Renderer;

#[derive(Clone, Copy)]
enum Target {
    World(usize),
    Pickup(u64),
    Activate(u64),
    Drop,
}

impl Session {
    fn target(&self) -> Option<Target> {
        let eye = self.game.player_position;
        let direction = self.game.view_direction();
        let reach = crate::engine::interact::MAX_INTERACTION_REACH_M;
        if self.physics.world.held().is_some() {
            let blocker = self
                .physics
                .world
                .first_unheld_body_hit(eye, direction, reach);
            return Some(
                self.game
                    .carried_door_target(blocker.map(|(_, distance)| distance))
                    .map_or(Target::Drop, |(index, _)| Target::World(index)),
            );
        }
        let world = self.game.interaction_target().and_then(|index| {
            self.game
                .interactables()
                .items()
                .get(index)
                .and_then(|item| {
                    crate::engine::interact::target_distance(
                        eye,
                        direction,
                        item,
                        self.game.door_colliders(),
                    )
                })
                .map(|distance| (index, distance))
        });
        if let Some((key, distance)) = self.physics.world.first_body_hit(eye, direction, reach)
            && world.is_none_or(|(_, obstacle)| distance < obstacle)
        {
            if self.physics.world.nearest_pickup(eye, direction) == Some(key) {
                return Some(Target::Pickup(key));
            }
            if self.physics.world.nearest_activation(eye, direction) == Some(key) {
                return Some(Target::Activate(key));
            }
            return None;
        }
        world.map(|(index, _)| Target::World(index))
    }

    pub(super) fn target_snapshot(&self) -> serde_json::Value {
        match self.target() {
            Some(Target::World(index)) => {
                serde_json::json!({"kind":"world", "index":index, "id":self.game.interactables().items().get(index).map(|item| &item.id)})
            }
            Some(Target::Pickup(key)) => serde_json::json!({"kind":"pickup", "key":key}),
            Some(Target::Activate(key)) => serde_json::json!({"kind":"activate", "key":key}),
            Some(Target::Drop) => serde_json::json!({"kind":"drop"}),
            None => serde_json::Value::Null,
        }
    }

    pub(super) fn interact(&mut self, audio: &mut AudioSystem) -> Result<(), String> {
        self.interactions = self.interactions.saturating_add(1);
        match self.target() {
            Some(Target::Pickup(key)) => {
                let picked = self.physics.world.pickup(
                    key,
                    self.game.player_position,
                    self.game.view_direction(),
                    self.game.player_yaw,
                );
                self.status = if picked {
                    "Picked up item. E drops; left click throws."
                } else {
                    "Pickup blocked"
                }
                .into();
            }
            Some(Target::Activate(key)) => {
                self.status = if self.physics.world.activate(key) {
                    "Activated physics item"
                } else {
                    "Physics activation blocked"
                }
                .into();
            }
            Some(Target::Drop) => {
                self.status = if self.physics.world.drop() {
                    "Dropped item"
                } else {
                    "Drop blocked: move the item clear"
                }
                .into();
            }
            Some(Target::World(index)) => {
                if let Some(report) = self.game.dispatch_interaction_at(Some(index)) {
                    self.status = format!(
                        "Used object: {} action(s), {} door(s), {} light(s)",
                        report.actions_run, report.doors_acted, report.lights_toggled
                    );
                    if report.unsupported > 0 || report.missing_targets > 0 {
                        return Err(format!(
                            "Interaction has {} unsupported action(s), {} missing target(s)",
                            report.unsupported, report.missing_targets
                        ));
                    }
                    if report.lights_toggled > 0 {
                        let _played = audio.play(
                            "switch_click",
                            PlayRequest {
                                gain: 0.8,
                                time_domain: TimeDomain::Real,
                                ..PlayRequest::default()
                            },
                        );
                    }
                }
            }
            None => self.status = "No usable object in reach".into(),
        }
        Ok(())
    }

    pub(super) fn apply_commands(
        &mut self,
        renderer: &mut Renderer,
        audio: &mut AudioSystem,
    ) -> Result<(), String> {
        for command in self.game.entities_mut().take_commands() {
            self.apply_command(command, renderer, audio)?;
        }
        Ok(())
    }

    fn apply_command(
        &mut self,
        command: WorldCommand,
        renderer: &mut Renderer,
        audio: &mut AudioSystem,
    ) -> Result<(), String> {
        match command {
            WorldCommand::ShowMessage { title, lines } => {
                self.enqueue_message(title, &lines, false)?;
            }
            WorldCommand::EndRun { won, title, detail } => {
                let outcome = if won { "Completed." } else { "Run ended." };
                self.enqueue_message(title, &[outcome.into(), detail], true)?;
            }
            WorldCommand::SetWeatherAlternate { alternate } => { let _changed = renderer.set_weather_alternate(alternate); }
            WorldCommand::SetWeatherStrength { strength, transition_seconds } => { let _changed = renderer.set_weather_strength(strength, transition_seconds); }
            WorldCommand::SetWeatherCycle { enabled } => { let _changed = renderer.set_weather_cycle(enabled); }
            WorldCommand::Blood { emission } => renderer.emit_blood(emission),
            WorldCommand::ActivatePhysicsGroup { group } => { let _report = self.physics.world.activate_group(&group)?; }
            WorldCommand::ResetPhysicsGroup { group } => { let _report = self.physics.world.reset_group(&group)?; }
            WorldCommand::SetEffectEnabled { index, enabled } => {
                let _changed = renderer.set_effect_enabled(usize::try_from(index).map_err(|error| error.to_string())?, enabled);
            }
            WorldCommand::SetWaterEnabled { index, enabled } => self.game.set_water_enabled(usize::try_from(index).map_err(|error| error.to_string())?, enabled),
            WorldCommand::SpawnDynamic { entity, model, character_texture, position, yaw_degrees, scale } => {
                self.spawn_dynamic(entity, &model, character_texture.as_deref(), position.to_array(), yaw_degrees, scale, renderer)?;
            }
            WorldCommand::DespawnDynamic { entity, instance_id } => {
                let _removed_model = renderer.despawn_runtime_model(entity);
                let _removed_character = renderer.despawn_runtime_character(&instance_id);
            }
            WorldCommand::SetDynamicTransform { entity, position, yaw_degrees } => {
                let _changed = renderer.set_runtime_transform(entity, position.to_array(), yaw_degrees);
                if let Some(id) = self.game.entities().handle_for_dynamic_key(entity).and_then(|handle| self.game.entities().instance_id_of(handle)) {
                    let _character_changed = renderer.set_runtime_character_transform(id, position.to_array(), yaw_degrees);
                }
            }
            WorldCommand::SetDynamicEmission { entity, scale } => { let _changed = renderer.set_runtime_emission(entity, scale); }
            WorldCommand::PlaySound { entity, sound, gain, looped, time_domain } => {
                let spatial = self.game.entities().audio_source_spatial(entity);
                // Match player ownership: legacy HVAC actions retain their
                // entity identity; only placed duct stock supplies the hum.
                if sound == crate::game::audio::HVAC_CUE {
                    return Ok(());
                }
                let _played = audio.play(&sound, PlayRequest {
                    source: Some(SourceId(entity)), position: self.game.entities().audio_source_position(entity), spatial,
                    attenuation: self.game.entities().audio_source_attenuation(entity), gain, looping: Some(looped), time_domain,
                    bus: if looped || spatial == Some(false) { AudioBus::Ambient } else { AudioBus::Effects },
                    perception: crate::engine::audio::PerceptionOwner::Playback,
                });
            }
            WorldCommand::StopSound { entity } => audio.stop_source(SourceId(entity)),
            WorldCommand::ChangeLevel { level } => return Err(format!("Quick test reached transition to '{level}'. Cross-level travel requires a final project package.")),
            WorldCommand::SaveCheckpoint { name } => return Err(format!("Quick test requested checkpoint '{name}'. Persistent checkpoints require final package testing.")),
            WorldCommand::ResumeCheckpoint => return Err("Resuming saved checkpoints requires final package testing.".into()),
            WorldCommand::PlayerForm { model: _, scale: _ } => return Err("Player forms require final package testing.".into()),
            WorldCommand::Ragdoll { target: _, enabled: _, velocity: _ } => return Err("Ragdoll ownership requires final package testing.".into()),
            WorldCommand::MannequinPins { group: _, enabled: _, target: _ } => return Err("Mannequin pin games require final package testing.".into()),
            WorldCommand::AiPutdown { token: _, contact: _ }
            | WorldCommand::AiUse { token: _, point: _ }
            | WorldCommand::AiPickup { token: _, item: _ }
            | WorldCommand::HandoffItem { target: _ }
            | WorldCommand::ReleaseItem { target: _ }
            | WorldCommand::RetireHidden { target: _ } => return Err("Actor carry/use assemblies require final package testing. Player pickup/drop physics are available in quick testing.".into()),
        }
        Ok(())
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "The typed spawn command supplies its complete bounded transform and appearance to the shared renderer."
    )]
    fn spawn_dynamic(
        &self,
        key: u64,
        model: &str,
        texture: Option<&str>,
        position: [f32; 3],
        yaw: f32,
        scale: f32,
        renderer: &mut Renderer,
    ) -> Result<(), String> {
        let instance = self
            .game
            .entities()
            .handle_for_dynamic_key(key)
            .and_then(|handle| self.game.entities().instance_id_of(handle));
        if let Some(id) = instance
            && renderer
                .spawn_runtime_character(id, model, position, yaw, scale)
                .is_ok()
        {
            renderer.set_runtime_character_texture(id, texture)?;
            return Ok(());
        }
        if renderer.spawn_runtime_model(key, model, position, yaw, scale) {
            Ok(())
        } else {
            Err(format!(
                "Cannot render spawned model '{model}' in the test resource budget"
            ))
        }
    }
}
