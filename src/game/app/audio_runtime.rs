//! Gameplay policy over the reusable mixer. Cue selection never reaches SDL.

use glam::Vec3;

use super::FrameLoop;
use crate::engine::audio::{
    AudioBus, AudioSettings, AudioSystem, Listener, PlayRequest, SourceId, TimeDomain,
};
use crate::engine::door::DoorPhase;
use crate::engine::physics::SurfaceMaterial;

pub(super) struct Audio {
    pub system: AudioSystem,
    doors: Vec<DoorState>,
    last_feet: Option<Vec3>,
    distance: f32,
    world_reset: u64,
    swimming: Option<bool>,
    last_water_sound: Option<std::time::Instant>,
    capture: Option<Capture>,
    environment_ready: bool,
    fluorescents: Vec<(usize, Vec3, bool)>,
    pools: Vec<crate::engine::level::WaterVolumeDef>,
    environment: [EnvironmentLoop; 3],
}

struct DoorState {
    angle: f32,
    phase: DoorPhase,
    moving: bool,
    cooldown: f32,
}

impl Audio {
    pub fn new(sdl: &sdl3::Sdl) -> Self {
        let root = crate::engine::assets::resolve_asset_root()
            .unwrap_or_else(|| std::path::PathBuf::from("assets"));
        Self {
            system: AudioSystem::new(&root, sdl),
            doors: Vec::new(),
            last_feet: None,
            distance: 0.0,
            world_reset: 0,
            swimming: None,
            last_water_sound: None,
            capture: Capture::new(),
            environment_ready: false,
            fluorescents: Vec::new(),
            pools: Vec::new(),
            environment: std::array::from_fn(|_| EnvironmentLoop::default()),
        }
    }

    pub fn clear_world(&mut self) {
        self.system.clear_world();
        self.environment_ready = false;
        self.fluorescents.clear();
        self.pools.clear();
        self.environment = std::array::from_fn(|_| EnvironmentLoop::default());
        self.doors.clear();
        self.last_feet = None;
        self.distance = 0.0;
        self.world_reset = 0;
        self.swimming = None;
        self.last_water_sound = None;
    }

    pub fn light_toggles(&mut self, toggles: &[(usize, bool)]) {
        for (index, _, enabled) in &mut self.fluorescents {
            if let Some((_, on)) = toggles.iter().find(|(changed, _)| changed == index) {
                *enabled = *on;
            }
        }
    }

    fn prepare_environment(
        &mut self,
        level: &crate::engine::level::LevelDef,
        states: &[(usize, bool)],
    ) {
        let surfaces = crate::engine::level::LevelSurfaces::new(level);
        self.fluorescents = level
            .ceiling_lights
            .iter()
            .enumerate()
            .filter(|(_, fixture)| {
                matches!(
                    crate::engine::lighting::fixture_profile(&fixture.fixture).kind,
                    crate::engine::lighting::FixtureKind::FluorescentPanel
                        | crate::engine::lighting::FixtureKind::BlacklightTube
                )
            })
            .map(|(index, fixture)| {
                (
                    index,
                    Vec3::new(
                        fixture.x,
                        fixture
                            .y
                            .unwrap_or_else(|| surfaces.ceiling_y_at(fixture.x, fixture.z)),
                        fixture.z,
                    ),
                    states
                        .iter()
                        .find(|(key, _)| *key == index)
                        .map_or(fixture.enabled, |(_, on)| *on),
                )
            })
            .collect();
        self.pools.clone_from(&level.water);
        self.system
            .set_acoustic_scene(crate::engine::audio::acoustics::AcousticScene::build(level));
        self.environment_ready = true;
    }

    fn update_environment(&mut self, listener: Vec3, delta: f32) {
        // One voice per family, irrespective of fixture/pool count. Nearest
        // points and preallocated controls avoid per-frame vectors or restarts.
        let fluorescent = nearest(
            listener,
            self.fluorescents
                .iter()
                .filter_map(|(_, position, on)| on.then_some(*position)),
        );
        let pool = nearest(
            listener,
            self.pools.iter().map(|water| water_point(water, listener)),
        );
        let room = nearest(listener, fluorescent.into_iter().chain(pool));
        for ((voice, spec), candidate) in
            self.environment
                .iter_mut()
                .zip(ENVIRONMENT)
                .zip([fluorescent, pool, room])
        {
            let target =
                candidate.filter(|point| point.distance_squared(listener) < spec.far * spec.far);
            let blend = 1.0 - (-delta.clamp(0.0, 0.1) / 0.35).exp();
            voice.gain =
                (if target.is_some() { 1.0 } else { 0.0 } - voice.gain).mul_add(blend, voice.gain);
            if let Some(position) = target {
                voice.position = Some(
                    voice
                        .position
                        .map_or(position, |old| old.lerp(position, blend)),
                );
            }
            if !voice.active
                && let Some(position) = target
            {
                voice.active = self.system.play(
                    spec.cue,
                    PlayRequest {
                        source: Some(spec.source),
                        position: Some(position),
                        spatial: Some(true),
                        looping: Some(true),
                        gain: voice.gain,
                        attenuation: Some([spec.near, spec.far]),
                        time_domain: TimeDomain::Real,
                        bus: AudioBus::Ambient,
                    },
                );
            }
            if voice.active {
                self.system.set_source_gain(spec.source, voice.gain);
                if target.is_none() && voice.gain < 0.001 {
                    self.system.stop_source(spec.source);
                    *voice = EnvironmentLoop::default();
                }
            }
        }
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
        // The listener is the controlled player's ears in both camera modes.
        // /become changes the visual, never the interaction/listener identity.
        self.audio.system.update(
            Listener {
                position: self.game.player_position,
                forward: self.game.view_direction(),
                up: Vec3::Y,
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

    pub(super) fn update_game_audio(&mut self) {
        let active = self.gameplay_active();
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
            self.update_footsteps();
            self.update_door_sounds();
        } else {
            self.audio.last_feet = None;
            self.audio.distance = 0.0;
        }
        if !self.menu_world()
            && let Some(loaded) = self.current_level.as_ref()
        {
            if !self.audio.environment_ready {
                self.audio
                    .prepare_environment(&loaded.level, &self.game.light_states());
            }
            if self.simulation_active() {
                self.audio
                    .update_environment(self.game.player_position, self.visual_delta_seconds());
            }
        }
        // Only live controls are visited, capped by the backend voice budget.
        // Routed entities publish their current transform through this seam.
        self.audio.system.update_source_positions(|source| {
            ENVIRONMENT
                .iter()
                .position(|spec| spec.source == source)
                .and_then(|index| self.audio.environment.get(index))
                .map_or_else(
                    || source_position(source.0, self.game.entities(), &self.items),
                    |voice| voice.position,
                )
        });
        let audio = &mut self.audio.system;
        let entities = self.game.entities_mut();
        self.items.world.drain_impacts(|impact| {
            entities.note_stimulus(
                impact.position,
                impact.strength.clamp(0.0, 1.0) * 8.0,
                "impact",
                None,
            );
            let _played = audio.play(
                impact_cue(impact.surface),
                PlayRequest {
                    position: Some(impact.position),
                    gain: impact.strength,
                    ..PlayRequest::default()
                },
            );
        });
        self.audio.system.update_acoustic_doors(self.game.doors());
        self.audio.system.drain_events(|event| {
            if active {
                self.game.entities_mut().observe_audio_start(event);
            }
        });
        self.audio.capture_output();
    }

    fn update_footsteps(&mut self) {
        let feet = Vec3::new(
            self.game.player_position.x,
            self.game.feet_y(),
            self.game.player_position.z,
        );
        let last_position = self.audio.last_feet.replace(feet);
        if self.game.noclip_enabled()
            || self.game.is_climbing()
            || self.game.is_swimming()
            || !self.game.grounded
        {
            self.audio.distance = 0.0;
            return;
        }
        let Some(previous) = last_position else {
            return;
        };
        let travel = (feet.x - previous.x).hypot(feet.z - previous.z);
        // Teleports/recovery cannot create a burst of footsteps.
        if !travel.is_finite() || travel > 1.0 || (feet.y - previous.y).abs() > 0.6 {
            self.audio.distance = 0.0;
            return;
        }
        self.audio.distance += travel;
        let stride = if self.game.is_crouched() { 0.8 } else { 1.35 };
        if self.audio.distance < stride {
            return;
        }
        self.audio.distance %= stride;
        let surface = if self
            .game
            .water
            .sample(feet.x, feet.z, feet.y + 0.05)
            .is_some()
        {
            SurfaceMaterial::Water
        } else {
            self.items
                .world
                .floor_material_at(feet)
                .map_or(SurfaceMaterial::Hard, SurfaceMaterial::from_material_id)
        };
        let crouched = self.game.is_crouched();
        self.game.entities_mut().note_stimulus(
            feet,
            if crouched { 1.5 } else { 6.0 },
            "movement",
            Some(crate::engine::ai::PLAYER_TARGET_HANDLE),
        );
        self.play_game_sound(
            footstep_cue(surface),
            None,
            if self.game.is_crouched() { 0.35 } else { 0.65 },
            TimeDomain::Real,
        );
    }

    fn update_door_sounds(&mut self) {
        if self.audio.doors.len() != self.game.doors().len() {
            self.audio.doors = self
                .game
                .doors()
                .iter()
                .map(|door| DoorState {
                    angle: door.angle(),
                    phase: door.phase(),
                    moving: false,
                    cooldown: 0.0,
                })
                .collect();
            return;
        }
        let delta = self.game.delta_seconds();
        for (door, previous) in self.game.doors().iter().zip(&mut self.audio.doors) {
            previous.cooldown = (previous.cooldown - delta).max(0.0);
            let moved = (door.angle() - previous.angle).abs() > 1.0e-5;
            if moved
                && (!previous.moving || opening(previous.phase) != opening(door.phase()))
                && previous.cooldown <= 0.0
            {
                let cue = match door.phase() {
                    DoorPhase::Opening | DoorPhase::Open => "door_open",
                    DoorPhase::Closing | DoorPhase::Closed => "door_close",
                };
                let played = self.audio.system.play(
                    cue,
                    PlayRequest {
                        position: Some(Vec3::new(door.def.x, door.base_y() + 1.0, door.def.z)),
                        ..PlayRequest::default()
                    },
                );
                if self.bench.enabled() {
                    self.trace.record(
                        "audio_door",
                        self.load_generation,
                        &serde_json::json!({"cue":cue,"id":door.def.id,"played":played})
                            .to_string(),
                    );
                }
                previous.cooldown = 0.15;
            }
            previous.angle = door.angle();
            previous.phase = door.phase();
            previous.moving = moved;
        }
    }

    pub(super) fn play_game_sound(
        &mut self,
        cue: &str,
        position: Option<Vec3>,
        gain: f32,
        time_domain: TimeDomain,
    ) {
        let played = self.audio.system.play(
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
        let spatial = self.game.entities().audio_source_spatial(entity);
        let played = self.audio.system.play(
            sound,
            PlayRequest {
                source: Some(SourceId(entity)),
                attenuation: self.game.entities().audio_source_attenuation(entity),
                position,
                spatial,
                gain,
                looping: Some(looping),
                time_domain,
                bus: if looping || spatial == Some(false) {
                    AudioBus::Ambient
                } else {
                    AudioBus::Effects
                },
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

#[derive(Default)]
struct EnvironmentLoop {
    position: Option<Vec3>,
    gain: f32,
    active: bool,
}

struct EnvironmentSpec {
    cue: &'static str,
    source: SourceId,
    near: f32,
    far: f32,
}
// Engine entity keys occupy the regular identity space. These three private
// application sources are resolved here instead of the authored-entity seam.
const ENVIRONMENT: [EnvironmentSpec; 3] = [
    EnvironmentSpec {
        cue: "fluorescent_buzz",
        source: SourceId(u64::MAX),
        near: 0.5,
        far: 6.5,
    },
    EnvironmentSpec {
        cue: "pool_lapping",
        source: SourceId(u64::MAX - 1),
        near: 1.5,
        far: 11.0,
    },
    EnvironmentSpec {
        cue: "empty_room",
        source: SourceId(u64::MAX - 2),
        near: 4.0,
        far: 18.0,
    },
];

fn nearest(listener: Vec3, points: impl Iterator<Item = Vec3>) -> Option<Vec3> {
    points.min_by(|a, b| {
        a.distance_squared(listener)
            .total_cmp(&b.distance_squared(listener))
    })
}

fn water_point(water: &crate::engine::level::WaterVolumeDef, listener: Vec3) -> Vec3 {
    if let Some(radius) = water.radius {
        let centre = Vec3::new(water.x + radius, water.surface_y, water.z + radius);
        let offset = Vec3::new(listener.x - centre.x, 0.0, listener.z - centre.z);
        let bounded = offset.clamp_length_max(radius);
        return Vec3::new(centre.x + bounded.x, centre.y, centre.z + bounded.z);
    }
    Vec3::new(
        listener
            .x
            .clamp(water.x, water.x + water.width.unwrap_or(0.0)),
        water.surface_y,
        listener
            .z
            .clamp(water.z, water.z + water.depth.unwrap_or(0.0)),
    )
}

const fn footstep_cue(surface: SurfaceMaterial) -> &'static str {
    match surface {
        SurfaceMaterial::Carpet => "footstep_carpet",
        SurfaceMaterial::Wood => "footstep_wood",
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

/// Optional genuine post-mixer PCM evidence. Enabled only in explicit native
/// acceptance; ordinary play has no file, capture allocation or output work.
struct Capture {
    file: std::fs::File,
    buffer: Box<[[f32; 2]]>,
    frames: u32,
    bytes: Vec<u8>,
}

impl Capture {
    fn new() -> Option<Self> {
        use std::io::Write;
        if std::env::var("PLACES_AUDIO_CAPTURE").as_deref() != Ok("1")
            || std::env::var("PLACES_BENCH").as_deref() != Ok("1")
        {
            return None;
        }
        let path = crate::engine::paths::user()
            .ok()?
            .data
            .join("native-audio.wav");
        let mut file = std::fs::File::create(path).ok()?;
        file.write_all(&[0_u8; 44]).ok()?;
        Some(Self {
            file,
            buffer: vec![[0.0; 2]; 4096].into_boxed_slice(),
            frames: 0,
            bytes: Vec::with_capacity(32_768),
        })
    }

    fn drain(&mut self, system: &mut AudioSystem) {
        use std::io::Write;
        let count = system.drain_capture(&mut self.buffer);
        self.bytes.clear();
        for frame in self.buffer.iter().take(count) {
            if self.frames >= 2_880_000 {
                break;
            }
            for sample in frame {
                self.bytes.extend_from_slice(&sample.to_le_bytes());
            }
            self.frames = self.frames.saturating_add(1);
        }
        let _written = self.file.write_all(&self.bytes);
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        use std::io::{Seek, SeekFrom, Write};
        let data_bytes = self.frames.saturating_mul(8);
        let mut header = Vec::with_capacity(44);
        header.extend_from_slice(b"RIFF");
        header.extend_from_slice(&data_bytes.saturating_add(36).to_le_bytes());
        header.extend_from_slice(b"WAVEfmt ");
        header.extend_from_slice(&16_u32.to_le_bytes());
        header.extend_from_slice(&3_u16.to_le_bytes());
        header.extend_from_slice(&2_u16.to_le_bytes());
        header.extend_from_slice(&48_000_u32.to_le_bytes());
        header.extend_from_slice(&384_000_u32.to_le_bytes());
        header.extend_from_slice(&8_u16.to_le_bytes());
        header.extend_from_slice(&32_u16.to_le_bytes());
        header.extend_from_slice(b"data");
        header.extend_from_slice(&data_bytes.to_le_bytes());
        if self.file.seek(SeekFrom::Start(0)).is_ok() {
            let _written = self.file.write_all(&header);
        }
    }
}

const fn opening(phase: DoorPhase) -> bool {
    matches!(phase, DoorPhase::Opening | DoorPhase::Open)
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
