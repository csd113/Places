//! Automatic world audio shared by the player and disposable editor sessions.
use super::Game;
use crate::engine::audio::{AudioBus, AudioSystem, PlayRequest, SourceId, TimeDomain};
use crate::engine::door::DoorPhase;
use crate::engine::level::{LevelDef, LevelSurfaces};
use crate::engine::loader::PropCatalog;
use crate::engine::physics::{ItemWorld, SurfaceMaterial};
use glam::{Mat4, Vec3};

#[cfg(test)]
#[path = "tests/audio_environment.rs"]
mod tests;

pub const HVAC_CUE: &str = "ambience_hvac";

#[derive(Default)]
pub struct WorldAudio {
    doors: Vec<DoorState>,
    last_feet: Option<Vec3>,
    distance: f32,
    environment_ready: bool,
    fluorescents: Vec<(usize, Vec3, bool)>,
    pools: Vec<crate::engine::level::WaterVolumeDef>,
    ducts: Vec<DuctSource>,
    environment: [EnvironmentLoop; 4],
}

struct DoorState {
    angle: f32,
    phase: DoorPhase,
    moving: bool,
    cooldown: f32,
}

impl WorldAudio {
    pub const fn ready(&self) -> bool {
        self.environment_ready
    }

    pub const fn reset_movement(&mut self) {
        self.last_feet = None;
        self.distance = 0.0;
    }

    pub fn light_toggles(&mut self, toggles: &[(usize, bool)]) {
        for (index, _, enabled) in &mut self.fluorescents {
            if let Some((_, on)) = toggles.iter().find(|(changed, _)| changed == index) {
                *enabled = *on;
            }
        }
    }

    pub fn prepare(&mut self, level: &LevelDef, catalog: &PropCatalog, states: &[(usize, bool)]) {
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
        self.ducts = duct_sources(level, catalog);
        self.environment_ready = true;
    }

    pub fn update_environment(&mut self, listener: Vec3, delta: f32, system: &mut AudioSystem) {
        // One voice per family, irrespective of fixture/pool/duct count. Nearest
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
        let duct = nearest(
            listener,
            self.ducts.iter().map(|duct| duct.nearest(listener)),
        );
        for ((voice, spec), candidate) in
            self.environment
                .iter_mut()
                .zip(ENVIRONMENT)
                .zip([fluorescent, pool, room, duct])
        {
            let target =
                candidate.filter(|point| point.distance_squared(listener) < spec.far * spec.far);
            let blend = 1.0 - (-delta.clamp(0.0, 0.1) / 0.35).exp();
            voice.gain =
                (if target.is_some() { 1.0 } else { 0.0 } - voice.gain).mul_add(blend, voice.gain);
            if let Some(position) = target {
                voice.position = Some(if spec.cue == HVAC_CUE {
                    // Never sweep the sound anchor through empty space
                    // when the nearest duct changes.
                    position
                } else {
                    voice
                        .position
                        .map_or(position, |old| old.lerp(position, blend))
                });
            }
            if !voice.active
                && let Some(position) = target
            {
                voice.active = system.play(
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
                system.set_source_gain(spec.source, voice.gain);
                if target.is_none() && voice.gain < 0.001 {
                    system.stop_source(spec.source);
                    *voice = EnvironmentLoop::default();
                }
            }
        }
    }

    pub fn footstep(&mut self, game: &mut Game, items: &ItemWorld) -> Option<(&'static str, f32)> {
        let feet = Vec3::new(
            game.player_position.x,
            game.feet_y(),
            game.player_position.z,
        );
        let last_position = self.last_feet.replace(feet);
        if game.noclip_enabled() || game.is_climbing() || game.is_swimming() || !game.grounded {
            self.distance = 0.0;
            return None;
        }
        let previous = last_position?;
        let travel = (feet.x - previous.x).hypot(feet.z - previous.z);
        // Teleports/recovery cannot create a burst of footsteps.
        if !travel.is_finite() || travel > 1.0 || (feet.y - previous.y).abs() > 0.6 {
            self.distance = 0.0;
            return None;
        }
        self.distance += travel;
        let stride = if game.is_crouched() { 0.8 } else { 1.35 };
        if self.distance < stride {
            return None;
        }
        self.distance %= stride;
        let surface = if game.water.sample(feet.x, feet.z, feet.y + 0.05).is_some() {
            SurfaceMaterial::Water
        } else {
            items
                .floor_material_at(feet)
                .map_or(SurfaceMaterial::Hard, SurfaceMaterial::from_material_id)
        };
        let crouched = game.is_crouched();
        game.entities_mut().note_stimulus(
            feet,
            if crouched { 1.5 } else { 6.0 },
            "movement",
            Some(crate::engine::ai::PLAYER_TARGET_HANDLE),
        );
        Some((
            footstep_cue(surface),
            if game.is_crouched() { 0.35 } else { 0.65 },
        ))
    }

    pub fn update_doors(
        &mut self,
        doors: &crate::engine::door::Doors,
        delta: f32,
        system: &mut AudioSystem,
        mut observe: impl FnMut(&str, &str, bool),
    ) {
        if self.doors.len() != doors.len() {
            self.doors = doors
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
        for (door, previous) in doors.iter().zip(&mut self.doors) {
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
                let played = system.play(
                    cue,
                    PlayRequest {
                        position: Some(Vec3::new(door.def.x, door.base_y() + 1.0, door.def.z)),
                        ..PlayRequest::default()
                    },
                );
                observe(cue, &door.def.id, played);
                previous.cooldown = 0.15;
            }
            previous.angle = door.angle();
            previous.phase = door.phase();
            previous.moving = moved;
        }
    }

    pub fn source_position(&self, source: SourceId) -> Option<Vec3> {
        ENVIRONMENT
            .iter()
            .position(|spec| spec.source == source)
            .and_then(|index| self.environment.get(index))
            .and_then(|voice| voice.position)
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
// Engine entity keys occupy the regular identity space. These four private
// application sources are resolved here instead of the authored-entity seam.
const ENVIRONMENT: [EnvironmentSpec; 4] = [
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
    EnvironmentSpec {
        cue: HVAC_CUE,
        source: SourceId(u64::MAX - 3),
        near: 0.75,
        far: 8.0,
    },
];

/// Immutable acoustic envelope using the same transform as the rendered stock.
/// The fitted box handles either horizontal direction and vertical up/down.
struct DuctSource {
    transform: Mat4,
    inverse: Mat4,
    minimum: Vec3,
    maximum: Vec3,
}

impl DuctSource {
    fn nearest(&self, listener: Vec3) -> Vec3 {
        let local = self.inverse.transform_point3(listener);
        self.transform
            .transform_point3(local.clamp(self.minimum, self.maximum))
    }
}

fn duct_sources(level: &LevelDef, catalog: &PropCatalog) -> Vec<DuctSource> {
    let surfaces = LevelSurfaces::new(level);
    level
        .props
        .iter()
        .filter_map(|prop| {
            let entry = catalog.assets().placeable(&prop.model)?;
            if prop.is_physics_movable() || !entry.tags.iter().any(|tag| tag == "hvac-duct") {
                return None;
            }
            let [width, height, depth] = entry.size?;
            let transform =
                prop.placement_matrix(surfaces.floor_y_at(prop.x, prop.z).unwrap_or(0.0));
            let inverse = transform.inverse();
            (transform.is_finite() && inverse.is_finite()).then_some(DuctSource {
                transform,
                inverse,
                minimum: Vec3::new(-width * 0.5, 0.0, -depth * 0.5),
                maximum: Vec3::new(width * 0.5, height, depth * 0.5),
            })
        })
        .collect()
}

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

const fn opening(phase: DoorPhase) -> bool {
    matches!(phase, DoorPhase::Opening | DoorPhase::Open)
}

/// Optional genuine post-mixer PCM evidence. Enabled only in explicit native
/// acceptance; ordinary play has no file, capture allocation or output work.
pub struct Capture {
    file: std::fs::File,
    buffer: Box<[[f32; 2]]>,
    frames: u32,
    bytes: Vec<u8>,
}

impl Capture {
    pub const fn frames(&self) -> u32 {
        self.frames
    }

    pub fn new() -> Option<Self> {
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

    pub fn drain(&mut self, system: &mut AudioSystem) {
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
