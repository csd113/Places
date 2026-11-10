//! Gameplay policy over the reusable mixer. Cue selection never reaches SDL.

use glam::Vec3;

mod guest_delivery;

use super::FrameLoop;
use crate::engine::audio::{
    AudioBus, AudioSettings, AudioSystem, Listener, PlayRequest, SourceId, TimeDomain,
};
use crate::engine::physics::SurfaceMaterial;

use crate::game::audio::{Capture, HVAC_CUE, WorldAudio};

pub(super) struct Audio {
    pub system: AudioSystem,
    world: WorldAudio,
    world_reset: u64,
    guest_fence: crate::engine::tall_guest::audio_fence::Fence,
    guest_delivery: guest_delivery::Delivery,
    swimming: Option<bool>,
    last_water_sound: Option<std::time::Instant>,
    capture: Option<Capture>,
}

impl Audio {
    pub fn new(
        prepared: Result<crate::engine::audio::PreparedAudio, String>,
        sdl: &sdl3::Sdl,
    ) -> Self {
        Self {
            system: AudioSystem::from_prepared(prepared, sdl),
            world: WorldAudio::default(),
            world_reset: 0,
            guest_fence: crate::engine::tall_guest::audio_fence::Fence::default(),
            guest_delivery: guest_delivery::Delivery::new(),
            swimming: None,
            last_water_sound: None,
            capture: Capture::new(),
        }
    }

    pub fn clear_world(&mut self) {
        self.system.clear_world();
        self.guest_fence.clear();
        self.guest_delivery = guest_delivery::Delivery::new();
        self.world = WorldAudio::default();
        self.world_reset = 0;
        self.swimming = None;
        self.last_water_sound = None;
    }

    pub fn light_toggles(&mut self, toggles: &[(usize, bool)]) {
        self.world.light_toggles(toggles);
    }

    pub fn finish(&mut self) {
        self.capture_output();
        self.capture = None;
        self.system.unload();
    }

    fn capture_output(&mut self) {
        if let Some(capture) = &mut self.capture {
            capture.drain(&mut self.system);
        }
    }
}

impl FrameLoop<'_> {
    pub(super) const fn reset_movement_audio(&mut self) {
        self.audio.world.reset_movement();
    }

    pub(super) const fn scripted_audio(&mut self, change: &super::actions::AudioAction) {
        self.settings.set_spatial_audio(if change.headphones {
            crate::engine::audio::SpatialMode::Headphones
        } else {
            crate::engine::audio::SpatialMode::Speakers
        });
        let _master_changed = self.settings.set_master_volume(change.master);
        let _mute_changed = self.settings.set_muted(change.muted);
    }
    pub(super) fn update_audio_mix(&mut self) {
        // A published cinematic eye owns presentation, never AI/body sensing.
        // Ordinary /become modes retain the existing physical-player ears.
        let (position, forward) = self.listener_pose();
        self.audio.system.update(
            Listener {
                position,
                forward,
                up: self.frame_camera().up,
            },
            !self.simulation_active(),
            self.items.world.simulation_state().time_scale,
            AudioSettings {
                spatial_mode: self.settings.spatial_audio,
                master_volume: self.settings.master_volume,
                effects_volume: self.settings.effects_volume,
                ambient_volume: self.settings.ambient_volume,
                muted: self.settings.muted,
            },
        );
    }

    pub(super) fn reconcile_audio_reset(&mut self) -> bool {
        if self.audio.world_reset != self.game.world_reset_count() {
            self.audio.clear_world();
            self.audio.world_reset = self.game.world_reset_count();
            return true;
        }
        false
    }

    fn play_player_footstep(&mut self) {
        if let Some(step) = self.audio.world.footstep(self.game, &self.items.world) {
            let request = step.request();
            let played = self.audio.system.play(step.cue, request);
            if self.bench.enabled() {
                self.trace.record(
                    "player_footstep",
                    self.load_generation,
                    &serde_json::json!({
                        "cue":step.cue,"origin":step.origin.to_array(),"stance":step.movement,"serial":step.serial,
                        "radius":step.movement.radius(),"gain":request.gain,"played":played,
                        "sim_time":self.game.entities().sim_time(),
                    })
                    .to_string(),
                );
            }
        }
    }

    fn update_guest_audio(&mut self) {
        let audio = &mut self.audio.system;
        let entities = self.game.entities_mut();
        let sounds = self
            .audio
            .guest_delivery
            .apply(entities.take_guest_sounds());
        for (key, sound) in sounds {
            if !self
                .audio
                .guest_fence
                .admit(key, entities.guest_audio_epoch(&sound.owner), &sound)
            {
                continue;
            }
            let source = SourceId(key);
            if sound.stop {
                audio.stop_source_with_fade(source, 0.12);
            } else if sound.update && audio.source_playing(source, &sound.cue) {
                audio.set_source_position(source, sound.position);
                if let Some(direction) = sound.forward {
                    audio.set_source_direction(source, direction);
                }
            } else {
                let _played = audio.play(
                    &sound.cue,
                    PlayRequest {
                        source: sound.forward.map(|_| source),
                        position: Some(sound.position),
                        direction: sound.forward,
                        spatial: Some(true),
                        looping: Some(sound.looping),
                        gain: sound.gain,
                        cursor_seconds: sound.cursor,
                        replacement_fade: 0.12,
                        perception: crate::engine::audio::PerceptionOwner::Gameplay {
                            event: sound.generation,
                        },
                        ..PlayRequest::default()
                    },
                );
            }
        }
        // Authoritative silence also retires a voice whose cosmetic stop was lost.
        for key in self.audio.guest_fence.sources() {
            if entities.guest_voice_cue(key).is_none()
                && audio.source_playing(SourceId(key), "tg_pursuit_shriek_loop")
            {
                audio.stop_source_with_fade(SourceId(key), 0.12);
            }
        }
    }

    pub(super) fn update_game_audio(&mut self) {
        let active = self.gameplay_active();
        self.audio
            .world
            .update_elevator_ride(self.game, active, &mut self.audio.system);
        if active {
            let swimming = self.game.is_swimming();
            if self
                .audio
                .swimming
                .replace(swimming)
                .is_some_and(|old| old != swimming)
                && !self.game.noclip_enabled()
                && self
                    .audio
                    .last_water_sound
                    .is_none_or(|last| last.elapsed() >= std::time::Duration::from_millis(750))
            {
                // Shallow ramp contact can briefly toggle swimming. Bound the
                // audible retrigger rate without delaying or changing physics.
                self.audio.last_water_sound = Some(std::time::Instant::now());
                self.play_game_sound(
                    "water_transition",
                    None,
                    if swimming { 0.7 } else { 0.45 },
                    TimeDomain::Real,
                );
            }
            self.play_player_footstep();
            self.audio.world.update_doors(
                self.game.doors(),
                self.game.delta_seconds(),
                &mut self.audio.system,
                |cue, id, played| {
                    if self.bench.enabled() {
                        self.trace.record(
                            "audio_door",
                            self.load_generation,
                            &serde_json::json!({"cue":cue,"id":id,"played":played}).to_string(),
                        );
                    }
                },
            );
            for (position, cue) in self.game.entities_mut().take_elevator_sounds() {
                let _played = self.audio.system.play(
                    cue,
                    PlayRequest {
                        position: Some(position),
                        spatial: Some(true),
                        attenuation: Some([1.5, 12.0]),
                        gain: 0.7,
                        ..PlayRequest::default()
                    },
                );
            }
        } else {
            self.audio.world.reset_movement();
        }
        self.update_environment_audio();
        // Only live controls are visited, capped by the backend voice budget.
        // Routed entities publish their current transform through this seam.
        self.audio.system.update_source_positions(|source| {
            self.audio
                .world
                .source_position(source)
                .or_else(|| source_position(source.0, self.game.entities(), &self.items))
        });
        self.update_guest_audio();
        let audio = &mut self.audio.system;
        let entities = self.game.entities_mut();
        self.items
            .world
            .drain_impacts(|impact| play_prop_impact(audio, entities, impact));
        self.audio.system.update_acoustic_doors(self.game.doors());
        self.audio
            .system
            .update_acoustic_elevators(self.game.world().elevators());
        self.audio.system.drain_events(|event| {
            if active {
                self.game.entities_mut().observe_audio_start(event);
            }
        });
        self.audio.capture_output();
    }

    fn update_environment_audio(&mut self) {
        let (listener_position, _) = self.listener_pose();
        if !self.menu_world()
            && let Some(loaded) = self.current_level.as_ref()
        {
            if !self.audio.world.ready() {
                self.audio
                    .world
                    .prepare(&loaded.level, &loaded.catalog, &self.game.light_states());
                self.audio.system.set_acoustic_scene(
                    crate::engine::audio::acoustics::AcousticScene::build(&loaded.level),
                );
            }
            if self.simulation_active() {
                self.audio.world.update_environment(
                    listener_position,
                    self.visual_delta_seconds(),
                    &mut self.audio.system,
                );
            }
        }
    }

    pub(super) fn play_game_sound(
        &mut self,
        cue: &str,
        position: Option<Vec3>,
        gain: f32,
        time_domain: TimeDomain,
    ) {
        // HVAC is owned by the one nearby-duct environment voice. Legacy
        // generic requests must not reintroduce global or stacked hum.
        let played = cue != HVAC_CUE
            && self.audio.system.play(
                cue,
                PlayRequest {
                    position,
                    gain,
                    time_domain,
                    ..PlayRequest::default()
                },
            );
        if self.bench.enabled() {
            self.trace.record("audio_cue", self.load_generation, &serde_json::json!({"cue":cue,"position":position.map(|point| point.to_array()),"gain":gain,"played":played}).to_string());
        }
    }

    pub(super) fn play_authored_sound(
        &mut self,
        entity: u64,
        sound: &str,
        gain: f32,
        looping: bool,
        time_domain: TimeDomain,
    ) {
        let position = source_position(entity, self.game.entities(), &self.items);
        // An expired/disabled owner cannot turn a queued positional voice into
        // listener-centred audio after disable, despawn or a world transition.
        if position.is_none() {
            return;
        }
        let spatial = self.game.entities().audio_source_spatial(entity);
        // Existing room/vending emitters retain their authored identity and
        // actions, but HVAC playback is derived only from placed duct stock.
        let played = sound != HVAC_CUE
            && self.audio.system.play(
                sound,
                PlayRequest {
                    source: Some(SourceId(entity)),
                    attenuation: self.game.entities().audio_source_attenuation(entity),
                    position,
                    direction: None,
                    spatial,
                    gain,
                    looping: Some(looping),
                    time_domain,
                    bus: if looping || spatial == Some(false) {
                        AudioBus::Ambient
                    } else {
                        AudioBus::Effects
                    },
                    perception: crate::engine::audio::PerceptionOwner::Playback,
                    cursor_seconds: 0.0,
                    replacement_fade: 0.0,
                },
            );
        if self.bench.enabled() {
            self.trace.record("audio_authored", self.load_generation, &serde_json::json!({"source":entity,"cue":sound,"position":position.map(|point| point.to_array()),"looping":looping,"played":played}).to_string());
        }
    }

    pub(super) fn interaction_audio_feedback(
        &mut self,
        index: usize,
        report: &crate::engine::entities::DispatchReport,
    ) {
        if report.lights_toggled > 0 {
            let position = self.game.interactables().items().get(index).map(|item| {
                Vec3::from_array(std::array::from_fn(|axis| {
                    f32::midpoint(
                        *item.bounds.min.get(axis).unwrap_or(&0.0),
                        *item.bounds.max.get(axis).unwrap_or(&0.0),
                    )
                }))
            });
            self.play_game_sound("switch_click", position, 0.8, TimeDomain::Real);
        }
    }

    pub(super) fn play_item_sound(&mut self, key: u64) {
        let model = self
            .items
            .authored
            .iter()
            .find_map(|(body, _, model)| (*body == key).then_some(model.as_str()))
            .or_else(|| self.console_runtime.given_model(key));
        let Some(cue) = model.and_then(prop_cue) else {
            return;
        };
        self.play_game_sound(
            cue,
            self.items.world.body_center(key),
            0.7,
            TimeDomain::Real,
        );
    }
}

fn play_prop_impact(
    audio: &mut AudioSystem,
    entities: &mut crate::engine::entities::EntityWorld,
    impact: crate::engine::physics::ImpactEvent,
) {
    entities.note_guest_impact(impact);
    let dropped = impact.release == crate::engine::physics::ImpactRelease::Dropped;
    let radius = if dropped {
        1.0
    } else {
        impact.strength.clamp(0.0, 1.0) * 8.0
    };
    entities.note_stimulus(impact.position, radius, "impact", None);
    let _played = audio.play(
        if dropped {
            dropped_impact_cue(impact.surface)
        } else {
            impact_cue(impact.surface)
        },
        PlayRequest {
            perception: crate::engine::audio::PerceptionOwner::Gameplay {
                event: impact.incident,
            },
            position: Some(impact.position),
            spatial: Some(true),
            attenuation: dropped.then_some([0.01, 1.0]),
            gain: if dropped {
                0.35 * impact.strength
            } else {
                impact.strength
            },
            ..PlayRequest::default()
        },
    );
}

// A dropped item uses the same surface footstep sample and at most the
// crouch request gain, with a one-metre spatial falloff. It cannot become a
// louder impact merely because a heavy object fell farther or bounced later.
const fn dropped_impact_cue(surface: SurfaceMaterial) -> &'static str {
    match surface {
        SurfaceMaterial::Wood => "footstep_wood",
        SurfaceMaterial::Carpet => "footstep_carpet",
        SurfaceMaterial::Grass => "footstep_grass",
        SurfaceMaterial::Gravel => "footstep_gravel",
        SurfaceMaterial::Sand => "footstep_sand",
        SurfaceMaterial::Snow => "footstep_snow",
        SurfaceMaterial::Water => "footstep_water",
        SurfaceMaterial::Hard | SurfaceMaterial::Metal | SurfaceMaterial::Glass => "footstep_hard",
    }
}

const fn impact_cue(surface: SurfaceMaterial) -> &'static str {
    match surface {
        SurfaceMaterial::Wood => "impact_wood",
        SurfaceMaterial::Metal => "impact_metal",
        SurfaceMaterial::Glass => "impact_glass",
        SurfaceMaterial::Carpet
        | SurfaceMaterial::Grass
        | SurfaceMaterial::Sand
        | SurfaceMaterial::Snow => "impact_soft",
        SurfaceMaterial::Hard | SurfaceMaterial::Gravel | SurfaceMaterial::Water => "impact_hard",
    }
}

fn prop_cue(model: &str) -> Option<&'static str> {
    if model.contains("rubber_duck") {
        Some("prop_duck")
    } else if model.contains("bell") {
        Some("prop_bell")
    } else if model.contains("party_cup") {
        Some("prop_plastic")
    } else if model.contains("cup") || model.contains("plate") || model.contains("mug") {
        Some("prop_ceramic")
    } else if model.contains("bottle") || model.contains("ball") {
        Some("prop_plastic")
    } else {
        None
    }
}

fn source_position(
    key: u64,
    world: &crate::engine::entities::EntityWorld,
    items: &super::item_runtime::Items,
) -> Option<Vec3> {
    let (instance, position) = world.audio_source_anchor(key)?;
    items
        .authored
        .iter()
        .find_map(|(body, id, _)| (id == instance).then_some(*body))
        .and_then(|body| items.world.body_center(body))
        .or(Some(position))
}
