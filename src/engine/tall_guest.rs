//! Opt-in, blind acoustic hunter. Sensing owns observations; motion and audio
//! consume their sampled evidence, never a live hidden target transform.
pub mod audio_fence;
mod capture;
pub(crate) use capture::Tick as CaptureTick;
mod checkpoint;
mod doorway;
mod foot_contact;
pub(crate) mod hearing;
mod lifecycle;
pub mod prey;
pub(crate) mod prop_track;
mod recovery;
mod terrain;

use std::collections::VecDeque;
use std::sync::Arc;

use glam::{Mat4, Quat, Vec3};
use serde::{Deserialize, Serialize};

use crate::engine::ai::perception::{Stimulus, sight_clear_with_solids};
use crate::engine::collision::{DoorCollider, WallAabb};
use crate::engine::entities::WorldContext;
use crate::engine::entity::{EntityFrame, PoseCue};
use crate::engine::level::{LevelDef, LevelSurfaces};
use crate::engine::nav::{NavBounds, NavScratch, Path, PathQuery};
use crate::engine::physics::{ImpactEvent, ImpactRelease};
use crate::engine::props::LoadedPropAsset;
use crate::engine::render::CharacterAnimator;

// Frozen clip reference rates, separate from the user's current movement tuning.
const WALK: f32 = 0.213_089_8;
const RUN_REFERENCE: f32 = 1.923_076_9;
const SQUEEZE_REFERENCE: f32 = 0.413_793_1;
const SQUEEZE: f32 = 1.2;
const POSTURE_RATE: f32 = 1.5;
const POSTURE_SECONDS: f32 = 1.2 / POSTURE_RATE;
const TURN_RATE: f32 = 60.0;
const BLEND: f32 = 0.18;
const MARGIN: f32 = 0.05;
const CLIPS: [(&str, f32); 11] = [
    ("capture_prey", 3.2),
    ("swallow_prey", 5.9),
    ("idle_listen", 3.0),
    ("walk_slow", 1.8),
    ("run_pursuit", 0.8),
    ("howl_echo", 3.2),
    ("inspect_prop", 4.8),
    ("door_enter", 1.2),
    ("door_squeeze", 1.8),
    ("door_exit", 1.2),
    ("lost_prey_agony", 4.0),
];

/// Standing navigation envelope: native frozen clip samples measured 0.97536m
/// radial reach and 3.57973m height before the existing 0.05m body margin.
///
/// Distributed aiming yields to nearby solids; compressed passages are handled
/// by their complete skinned trajectory, never this upright graph.
#[must_use]
pub const fn navigation_class() -> crate::engine::package::navigation::NavClass {
    crate::engine::package::navigation::NavClass {
        radius: 1.03,
        height: 3.64,
        step_height: 0.4,
        max_slope: 2.6667,
    }
}

/// Explicit root staging poses outside a finite passage. The full path is
/// checked against the final skinned body; these points never grant fit.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Passage {
    pub id: String,
    pub entry: [f32; 3],
    pub exit: [f32; 3],
    /// Crouch locomotion semantics always prohibit this link.
    #[serde(default)]
    pub crouch_only: bool,
    /// Player-only protected volume, required for a crouch-only link.
    #[serde(default)]
    pub protected: Option<Refuge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Refuge {
    pub min: [f32; 3],
    pub max: [f32; 3],
}
impl Refuge {
    fn bounds(&self) -> WallAabb {
        WallAabb::with_y(
            self.min[0],
            self.min[1],
            self.min[2],
            self.max[0] - self.min[0],
            self.max[1] - self.min[1],
            self.max[2] - self.min[2],
        )
    }
    fn valid(&self) -> bool {
        self.min
            .iter()
            .chain(self.max.iter())
            .all(|v| v.is_finite() && v.abs() <= 100_000.0)
            && Vec3::from(self.max).cmpgt(Vec3::from(self.min)).all()
    }
}

/// No vision, arbitrary speeds, scale override or additive head aim is exposed.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Definition {
    #[serde(default)]
    pub patrol: Vec<[f32; 3]>,
    /// Opt-in authored living acoustic prey; no hidden live steering.
    #[serde(default)]
    pub prey_targets: Vec<String>,
    #[serde(default)]
    pub passages: Vec<Passage>,
}

impl Definition {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.prey_targets.len() <= 16
            && self
                .prey_targets
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                == self.prey_targets.len()
            && self
                .prey_targets
                .iter()
                .all(|id| !id.is_empty() && id.len() <= 64 && !id.chars().any(char::is_control))
            && self.patrol.len() <= 32
            && self.passages.len() <= 16
            && self
                .patrol
                .iter()
                .flatten()
                .all(|x| x.is_finite() && x.abs() <= 100_000.0)
            && self.passages.iter().all(|p| {
                (!p.crouch_only || p.protected.as_ref().is_some_and(Refuge::valid))
                    && p.protected.as_ref().is_none_or(Refuge::valid)
                    && !p.id.is_empty()
                    && p.id.len() <= 64
                    && p.entry
                        .iter()
                        .chain(p.exit.iter())
                        .all(|x| x.is_finite() && x.abs() <= 100_000.0)
                    && Vec3::from(p.entry).distance(Vec3::from(p.exit)) > 0.1
                    && Vec3::from(p.entry).distance(Vec3::from(p.exit)) <= 12.0
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Intent {
    Patrol,
    Listen,
    InvestigateNoise,
    Inspect,
    Chase,
    Search,
    BlockedAtRefuge,
    EscapeFrustration,
    Disabled,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    position: Vec3,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    heading: Option<f32>,
    aim_point: Vec3,
    velocity: Vec3,
    sampled: f32,
    pulse: u64,
    quality: f32,
    confidence: f32,
    uncertainty: f32,
    track: u64,
    confirmed: bool,
}

impl Observation {
    fn correlates(&self, next: &Self, now: f32) -> bool {
        let grace = if self.confirmed && now - self.sampled < 1.5 {
            1.5
        } else {
            1.25
        };
        self.track == next.track
            && self.pulse < next.pulse
            && next.sampled > self.sampled
            && next.sampled - self.sampled <= grace
            && next.position.distance(self.position)
                <= 12.0_f32.mul_add(
                    next.sampled - self.sampled,
                    self.uncertainty + next.uncertainty,
                )
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Plant {
    position: Vec3,
    rotation: Quat,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Return {
    observation: Observation,
    origin: Vec3,
    hit: Vec3,
    deliver: f32,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Transit {
    entry: Vec3,
    exit: Vec3,
    yaw: f32,
    phase: u8,
    elapsed: f32,
    progress: f32,
}

const fn default_contact_readiness() -> [bool; 3] {
    [true; 3]
}

/// Semantic, deterministic state. Renderer clocks and sound-device state are
/// absent; a restored pending pulse retains its original sampled timestamp.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub id: String,
    pub position: Vec3,
    pub yaw: f32,
    pub intent: Intent,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    physical_pose: Option<Vec<crate::engine::entity::JointPose>>,
    life: lifecycle::Life,
    clock: f32,
    entered: f32,
    clip: String,
    clip_time: f32,
    previous: String,
    previous_time: f32,
    blend_time: f32,
    observation: Option<Observation>,
    pending: VecDeque<Return>,
    pulse: u64,
    next_pulse: f32,
    goal: Option<Vec3>,
    goal_time: f32,
    patrol_index: usize,
    episode: u64,
    frustrated: u64,
    frustration_until: f32,
    frustration_probe_until: f32,
    search_until: f32,
    search_deadline: f32,
    noise_deadline: f32,
    noise_progress: Vec3,
    transit: Option<Transit>,
    incidents: VecDeque<(u64, u64)>,
    source_cooldowns: VecDeque<(u64, f32)>,
    voice: Option<String>,
    voice_until: f32,
    voice_generation: u64,
    voice_started: f32,
    audio_sequence: u64,
    blocked_seconds: f32,
    plants: [Option<Plant>; 3],
    #[serde(default = "default_contact_readiness")]
    contact_ready: [bool; 3],
    #[serde(default)]
    contact_pending: [bool; 3],
    movement_serial: Option<u64>,
    speed: f32,
    motion_rate: f32,
    attacked_episode: u64,
    aim_weight: f32,
    base_speed: f32,
    pursuit_speed: f32,
    base_posture: f32,
    #[serde(default)]
    support_lowering: f32,
    prop_track: Option<prop_track::Track>,
    pending_loss: bool,
    noise_portal: bool,
    blocked_refuge: Option<String>,
    impacts: VecDeque<(ImpactEvent, f32)>,
    heard_impacts: VecDeque<hearing::Landing>,
}

/// Presentation command with the final posed acoustic axis, never a desired beam.
#[derive(Clone, Debug)]
pub struct Sound {
    pub owner: String,
    pub epoch: u64,
    pub sequence: u64,
    pub cursor: f64,
    pub update: bool,
    pub generation: u64,
    pub cue: String,
    pub position: Vec3,
    pub forward: Option<Vec3>,
    pub looping: bool,
    pub stop: bool,
    pub gain: f32,
}

#[derive(Clone)]
struct PassagePlan {
    id: String,
    entry: Vec3,
    exit: Vec3,
    yaw: f32,
    sampled: Record,
    next_sample: f32,
    total: f32,
}

#[derive(PartialEq, Eq)]
enum RefugeRoute {
    Unresolved,
    Detour,
}

pub struct Guest {
    pub state: Record,
    pub(crate) capture: Option<capture::Capture>,
    capture_failure: Option<(u64, Vec3)>,
    audio_epoch: u64,
    definition: Definition,
    stages: Vec<doorway::Stage>,
    asset: Arc<LoadedPropAsset>,
    animator: CharacterAnimator,
    vertices: Vec<Vec3>,
    body_bounds: (Vec3, Vec3),
    ceiling: crate::engine::level::WalkableCeiling,
    path: Path,
    scratch: NavScratch,
    next_plan: f32,
    path_cursor: usize,
    refuge_route: RefugeRoute,
    recovery: recovery::Mode,
    next_forward_probe: f32,
    next_activation_check: f32,
    next_clear_log: f32,
    retreat_poses: VecDeque<recovery::Pose>,
    sounds: Vec<Sound>,
    diagnostics: VecDeque<serde_json::Value>,
    passage_plan: Option<PassagePlan>,
    next_passage_check: f32,
    support_heights: [f32; 3],
    swing_heights: [f32; 3],
    support_normals: [Vec3; 3],
    authored_lifts: [f32; 3],
    passage_credit: f32,
    attack_request: bool,
    distributed_aim: bool,
    sole_vertices: [Vec<usize>; 3],
    candidate_events: VecDeque<serde_json::Value>,
    posing_candidate: bool,
}

/// # Errors
/// Refuses malformed capability data, conflicting owners and missing rig hooks.
pub fn load(level: &LevelDef) -> Result<Vec<Guest>, String> {
    let mut result = Vec::new();
    if !level.props.iter().any(|p| p.tall_guest.is_some()) {
        return Ok(result);
    }
    let audio_epoch = audio_fence::allocate_epoch()?;
    let catalog = crate::engine::loader::PropCatalog::load_default();
    let mut assets = crate::engine::props::PropAssets::load_default();
    let surfaces = LevelSurfaces::new(level);
    for (prop, id) in level.props.iter().zip(level.prop_instance_ids()) {
        let Some(definition) = prop.tall_guest.clone() else {
            continue;
        };
        if !definition.is_valid()
            || prop.scale.to_bits() != 1.0_f32.to_bits()
            || prop.solid
            || prop.physics.is_some()
            || prop.float.is_some()
            || prop.span.is_some()
            || prop.shadow_only
            || !prop.material_overrides.is_empty()
            || !prop.emission_overrides.is_empty()
            || prop.animated_door.is_some()
            || prop.pitch_degrees != 0.0
            || prop.roll_degrees != 0.0
            || level.routes.iter().any(|route| route.id == id)
            || prop.components.iter().any(|c| {
                matches!(
                    c,
                    crate::engine::level::ComponentDef::Ai(_)
                        | crate::engine::level::ComponentDef::Animation {
                            clip: _,
                            speed: _,
                            looped: _,
                            playing: _
                        }
                )
            })
        {
            return Err(format!(
                "{id}: Tall Guest requires one upright scale-one motion owner and valid bounded passages"
            ));
        }
        validate_prey_targets(level, &definition, &id)?;
        let path = catalog
            .get(&prop.model)
            .model
            .ok_or("Tall Guest model is missing")?;
        if !definition.prey_targets.is_empty()
            && (path != "entities/tall_guest/model/tall_guest.glb"
                || !crate::engine::props::reviewed_source_matches(&path, prey::HUNTER_SHA256))
        {
            return Err(format!(
                "{id}: whole-prey capture requires the certified Tall Guest export"
            ));
        }
        let asset = assets.resolve(&path)?;
        let animator =
            CharacterAnimator::new(&asset.model).ok_or("Tall Guest requires a skinned rig")?;
        for (name, seconds) in CLIPS {
            if animator
                .clip_duration(name)
                .is_none_or(|value| (value - seconds).abs() > 0.001)
            {
                return Err(format!("{id}: missing or mistimed clip {name}"));
            }
        }
        for name in [
            "acoustic_origin",
            "mouth_capture_socket",
            "head",
            "foot_front",
            "foot_left",
            "foot_right",
        ] {
            if animator.node_global(name).is_none() {
                return Err(format!("{id}: missing rig node {name}"));
            }
        }
        let position = Vec3::new(
            prop.x,
            surfaces.floor_y_at(prop.x, prop.z).unwrap_or(0.0) + prop.y,
            prop.z,
        );
        let mut guest = Guest::new(
            id,
            definition,
            position,
            prop.rotation_degrees.to_radians(),
            asset,
            animator,
            crate::engine::level::WalkableCeiling::from_level(level),
        );
        guest.audio_epoch = audio_epoch;
        guest.stages = doorway::resolve(&mut guest, level);
        result.push(guest);
    }
    if result.len() > 8 {
        return Err("At most eight Tall Guests per level".into());
    }
    Ok(result)
}

fn validate_prey_targets(
    level: &LevelDef,
    definition: &Definition,
    id: &str,
) -> Result<(), String> {
    if definition.prey_targets.iter().any(|target| {
        !level
            .props
            .iter()
            .zip(level.prop_instance_ids())
            .any(|(candidate, candidate_id)| {
                candidate_id == *target && candidate.feltling.is_some()
            })
    }) {
        return Err(format!("{id}: prey target requires a native Feltling"));
    }
    Ok(())
}

fn duration(name: &str) -> f32 {
    CLIPS
        .iter()
        .find(|(n, _)| *n == name)
        .map_or(1.0, |(_, t)| *t)
}
fn looped(name: &str) -> bool {
    matches!(
        name,
        "idle_listen" | "walk_slow" | "run_pursuit" | "door_squeeze"
    )
}

impl Guest {
    fn new(
        id: String,
        definition: Definition,
        position: Vec3,
        yaw: f32,
        asset: Arc<LoadedPropAsset>,
        animator: CharacterAnimator,
        ceiling: crate::engine::level::WalkableCeiling,
    ) -> Self {
        let mut guest = Self {
            capture: None,
            capture_failure: None,
            state: Record {
                id,
                position,
                yaw,
                intent: Intent::Patrol,
                physical_pose: None,
                life: lifecycle::Life::Active,
                clock: 0.0,
                entered: 0.0,
                clip: "idle_listen".into(),
                clip_time: 0.0,
                previous: "idle_listen".into(),
                previous_time: 0.0,
                blend_time: BLEND,
                observation: None,
                pending: VecDeque::new(),
                pulse: 0,
                next_pulse: 0.7,
                goal: None,
                goal_time: 0.0,
                patrol_index: 0,
                episode: 0,
                frustrated: 0,
                frustration_until: 0.0,
                frustration_probe_until: 0.0,
                search_until: 0.0,
                search_deadline: 0.0,
                noise_deadline: 0.0,
                noise_progress: position,
                transit: None,
                incidents: VecDeque::new(),
                source_cooldowns: VecDeque::new(),
                voice: None,
                voice_until: 0.0,
                voice_generation: 0,
                voice_started: 0.0,
                audio_sequence: 0,
                blocked_seconds: 0.0,
                plants: [None, None, None],
                contact_ready: default_contact_readiness(),
                contact_pending: [false; 3],
                movement_serial: None,
                speed: 0.0,
                motion_rate: 1.0,
                attacked_episode: 0,
                aim_weight: 0.0,
                base_speed: 4.125,
                pursuit_speed: 4.5,
                base_posture: 0.0,
                support_lowering: 0.0,
                prop_track: None,
                pending_loss: false,
                noise_portal: false,
                blocked_refuge: None,
                impacts: VecDeque::new(),
                heard_impacts: VecDeque::new(),
            },
            audio_epoch: 0,
            vertices: vec![Vec3::ZERO; asset.model.vertices.len()],
            body_bounds: (Vec3::ZERO, Vec3::ZERO),
            ceiling,
            stages: Vec::new(),
            definition,
            asset,
            animator,
            path: Path::new(),
            scratch: NavScratch::default(),
            next_plan: 0.0,
            path_cursor: 1,
            refuge_route: RefugeRoute::Unresolved,
            recovery: recovery::Mode::Forward,
            next_forward_probe: 0.0,
            next_activation_check: 0.0,
            next_clear_log: 0.0,
            retreat_poses: VecDeque::new(),
            sounds: Vec::new(),
            diagnostics: VecDeque::new(),
            passage_plan: None,
            next_passage_check: 0.0,
            support_heights: [position.y; 3],
            swing_heights: [position.y; 3],
            support_normals: [Vec3::Y; 3],
            authored_lifts: [0.0; 3],
            passage_credit: 0.0,
            attack_request: false,
            sole_vertices: [Vec::new(), Vec::new(), Vec::new()],
            candidate_events: VecDeque::new(),
            posing_candidate: false,
            distributed_aim: !(std::env::var_os("PLACES_BENCH").is_some()
                && std::env::var_os("PLACES_TALL_GUEST_DISABLE_DISTRIBUTED_AIM").is_some()),
        };
        guest.sole_vertices = guest.sole_samples();
        guest.sample();
        guest.fill_vertices();
        guest
    }

    fn log(&mut self, kind: &str, detail: &serde_json::Value) {
        let event = serde_json::json!({"type":kind,"actor":self.state.id,"time":self.state.clock,"episode":self.state.episode,"detail":detail});
        if self.posing_candidate {
            self.candidate_events.push_back(event);
        } else {
            self.store_event(event);
        }
    }

    fn store_event(&mut self, event: serde_json::Value) {
        if self.diagnostics.len() == 128 {
            let _old = self.diagnostics.pop_front();
        }
        self.diagnostics.push_back(event);
    }

    fn set_clip(&mut self, name: &str) {
        if self.state.clip == name {
            return;
        }
        self.state.previous.clone_from(&self.state.clip);
        self.state.previous_time = self.state.clip_time;
        self.state.clip = name.into();
        self.state.clip_time = 0.0;
        self.state.blend_time = 0.0;
    }

    fn set_intent(&mut self, next: Intent, reason: &str) {
        if self.state.intent == next {
            return;
        }
        let from = self.state.intent;
        self.state.intent = next;
        self.state.entered = self.state.clock;
        self.log("state_changed",&serde_json::json!({"from":from,"to":next,"reason":reason,"evidence_age":self.evidence_age()}));
    }

    fn evidence_age(&self) -> f32 {
        self.state
            .observation
            .as_ref()
            .map_or(0.0, |o| (self.state.clock - o.sampled).max(0.0))
    }
    fn fresh(&self) -> bool {
        self.state.observation.as_ref().is_some_and(|o| {
            o.confirmed
                && self.state.clock - o.sampled < 1.5
                && (-0.3_f32).mul_add(self.state.clock - o.sampled, o.confidence) >= 0.35
        })
    }

    fn sample_base(&mut self) {
        let a = self.state.clip_time / duration(&self.state.clip);
        let b = self.state.previous_time / duration(&self.state.previous);
        let linear_fade = (self.state.blend_time / BLEND).min(1.0);
        let fade =
            if self.state.clip.starts_with("door_") || self.state.previous.starts_with("door_") {
                smooth(linear_fade)
            } else {
                linear_fade
            };
        let _changed = self.animator.sample_blended_fraction(
            &self.state.clip,
            a,
            &self.state.previous,
            b,
            fade,
        );
        if self.state.base_posture > 0.0 {
            let _base = self
                .animator
                .blend_upper_clip("walk_slow", a, self.state.base_posture);
        }
    }

    fn sample(&mut self) {
        self.sample_base();
        let _aim = self.animator.blend_upper_inspection(self.state.aim_weight);
        self.door_compression();
        self.door_steps();
        self.capture_authored_lifts();
        self.support_posture();
        self.plant_feet();
    }

    fn sample_floor(&mut self, ctx: &WorldContext<'_>) {
        self.sample_base();
        self.sample_support(ctx);
        let _aim = self.animator.blend_upper_inspection(self.state.aim_weight);
        self.door_compression();
        self.door_steps();
        self.capture_authored_lifts();
        self.support_posture();
        self.plant_feet();
        self.seat_terrain_soles(ctx);
    }

    fn extra_lowering(&self) -> f32 {
        let phase = if self.state.clip == "door_enter" {
            self.state.clip_time
        } else if self.state.clip == "door_exit" {
            // The delivered exit already raises from the compressed pose.
            // Additional lowering here folds its low leading knee into ground.
            return 0.0;
        } else {
            return 0.0;
        };
        0.45 * smooth(phase / 0.25) * smooth((1.2 - phase) / 0.35)
    }

    fn door_compression(&mut self) {
        let amount = self.extra_lowering();
        if amount <= 0.0 {
            return;
        }
        if let Some(root) = self.animator.node_global("root") {
            let (scale, rotation, position) = root.to_scale_rotation_translation();
            let target = Vec3::new(position.x, position.y - amount, position.z);
            let _root = self.animator.set_node_global(
                "root",
                Mat4::from_scale_rotation_translation(scale, rotation, target),
            );
        }
        // All three feet retain their authored support/lift heights. A lifted
        // front foot also needs the leg solver: a supported-ankle-only fix
        // leaves its authored low knee below the floor during a deep bend.
        for suffix in ["front", "left", "right"] {
            if let Some(foot) = self.animator.node_global(&format!("foot_{suffix}")) {
                let (_, rotation, position) = foot.to_scale_rotation_translation();
                self.solve_leg(
                    suffix,
                    Vec3::new(position.x, position.y + amount, position.z),
                    rotation,
                );
            }
        }
    }

    fn door_steps(&mut self) {
        let entering = self.state.clip == "door_enter";
        if !entering && self.state.clip != "door_exit" {
            return;
        }
        for (suffix, start, sign) in [("left", 0.18, -1.0_f32), ("right", 0.66, 1.0_f32)] {
            let phase = ((self.state.clip_time - start) / 0.36_f32).clamp(0.0, 1.0);
            let amount = phase * phase * 2.0_f32.mul_add(-phase, 3.0);
            let x = if entering {
                (-0.350_656_f32).mul_add(amount, 0.65)
            } else {
                0.350_656_f32.mul_add(amount, 0.299_344)
            };
            let Some(foot) = self.animator.node_global(&format!("foot_{suffix}")) else {
                continue;
            };
            let (_, rotation, position) = foot.to_scale_rotation_translation();
            let blend = (self.state.blend_time / BLEND).clamp(0.0, 1.0);
            let height = 0.10_f32.mul_add((std::f32::consts::PI * phase).sin(), 0.055);
            let target = Vec3::new(
                sign.mul_add(x, -position.x).mul_add(blend, position.x),
                (height - position.y).mul_add(blend, position.y),
                position.z,
            );
            self.solve_leg(suffix, target, rotation);
        }
    }

    /// Holds supported feet in world space through a gait, turn and blend.
    /// Two-bone rotations retain authored segment lengths; no scale adjustment.
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Analytic two-bone IK uses finite vector and trigonometric operations, bounded by measured limb lengths."
    )]
    fn plant_feet(&mut self) {
        let world = self.transform();
        let inverse = world.inverse();
        for (index, suffix) in ["front", "left", "right"].into_iter().enumerate() {
            let name = format!("foot_{suffix}");
            let Some(foot) = self.animator.node_global(&name) else {
                continue;
            };
            let current = world.mul_mat4(&foot);
            let (_, rotation, position) = current.to_scale_rotation_translation();
            let Some(existing_plant) = self.state.plants.get_mut(index) else {
                continue;
            };
            let floor = self
                .support_heights
                .get(index)
                .copied()
                .unwrap_or(self.state.position.y);
            let lift = self.authored_lifts.get(index).copied().unwrap_or(0.0);
            if lift > 0.016 {
                *existing_plant = None;
                self.swing_terrain(index, suffix, position, rotation, lift);
                continue;
            }
            let supported_rotation = self.terrain_rotation(index, rotation);
            let Some(slot) = self.state.plants.get_mut(index) else {
                continue;
            };
            let plant = slot.get_or_insert_with(|| Plant {
                position: Vec3::new(position.x, floor + 0.055, position.z),
                rotation: supported_rotation,
            });
            let target = inverse.transform_point3(plant.position);
            let orientation = Quat::from_rotation_y(-self.state.yaw) * plant.rotation;
            self.solve_leg(suffix, target, orientation);
            self.seat_sole(index, suffix, target, orientation);
        }
    }

    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Analytic two-bone IK uses finite metre-space vectors; denominators and limb lengths are checked before division."
    )]
    fn solve_leg(&mut self, suffix: &str, target: Vec3, orientation: Quat) {
        let upper_name = format!("leg_{suffix}_upper");
        let lower_name = format!("leg_{suffix}_lower");
        let foot_name = format!("foot_{suffix}");
        let Some(upper) = self.animator.node_global(&upper_name) else {
            return;
        };
        let Some(lower) = self.animator.node_global(&lower_name) else {
            return;
        };
        let Some(foot) = self.animator.node_global(&foot_name) else {
            return;
        };
        let (upper_scale, upper_rotation, hip) = upper.to_scale_rotation_translation();
        let (lower_scale, lower_rotation, knee) = lower.to_scale_rotation_translation();
        let ankle = foot.transform_point3(Vec3::ZERO);
        let first = hip.distance(knee);
        let second = knee.distance(ankle);
        if first < 0.01 || second < 0.01 || hip.distance(target) < 0.01 {
            return;
        }
        let direction = (target - hip).normalize_or_zero();
        let distance = hip
            .distance(target)
            .clamp((first - second).abs() + 0.001, first + second - 0.001);
        let solved_target = hip + direction * distance;
        let along =
            distance.mul_add(distance, second.mul_add(-second, first * first)) / (2.0 * distance);
        let height = first.mul_add(first, -along * along).max(0.0).sqrt();
        let authored = ((knee - hip) - direction * (knee - hip).dot(direction)).normalize_or_zero();
        let compression =
            ((self.extra_lowering() + self.state.support_lowering) / 0.45).clamp(0.0, 1.0);
        let upward = (Vec3::Y - direction * direction.y).normalize_or_zero();
        let bend = authored.lerp(upward, compression).normalize_or_zero();
        let solved_knee = hip + direction * along + bend * height;
        let upper_delta = Quat::from_rotation_arc(
            (knee - hip).normalize_or_zero(),
            (solved_knee - hip).normalize_or_zero(),
        );
        let lower_delta = Quat::from_rotation_arc(
            (ankle - knee).normalize_or_zero(),
            (solved_target - solved_knee).normalize_or_zero(),
        );
        let _upper = self.animator.set_node_global(
            &upper_name,
            Mat4::from_scale_rotation_translation(upper_scale, upper_delta * upper_rotation, hip),
        );
        let _lower = self.animator.set_node_global(
            &lower_name,
            Mat4::from_scale_rotation_translation(
                lower_scale,
                lower_delta * lower_rotation,
                solved_knee,
            ),
        );
        let _foot = self.animator.set_node_global(
            &foot_name,
            Mat4::from_scale_rotation_translation(Vec3::ONE, orientation, solved_target),
        );
    }

    fn transform(&self) -> Mat4 {
        Mat4::from_rotation_translation(Quat::from_rotation_y(self.state.yaw), self.state.position)
    }

    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Finite bounded metre-space vector operations have no checked-vector alternative; integer counters use checked or saturating arithmetic."
    )]
    fn socket(&self, name: &str) -> Option<Mat4> {
        self.animator
            .node_global(name)
            .map(|node| self.transform() * node)
    }

    #[must_use]
    pub fn acoustic_position(&self) -> Vec3 {
        self.head().0
    }

    fn head(&self) -> (Vec3, Vec3) {
        self.socket("acoustic_origin")
            .map_or((self.state.position, Vec3::Z), |m| {
                (
                    m.transform_point3(Vec3::ZERO),
                    m.transform_vector3(Vec3::Y).normalize_or_zero(),
                )
            })
    }

    const fn next_audio_sequence(&mut self) -> u64 {
        self.state.audio_sequence = self.state.audio_sequence.saturating_add(1);
        self.state.audio_sequence
    }

    #[must_use]
    pub const fn audio_epoch(&self) -> u64 {
        self.audio_epoch
    }

    #[must_use]
    pub fn voice_cue(&self) -> Option<&str> {
        self.enabled()
            .then_some(self.state.voice.as_deref())
            .flatten()
    }

    fn emit(&mut self, cue: &str, foot: Option<&str>, looping: bool) {
        if let Some(index) = ["foot_front", "foot_left", "foot_right"]
            .iter()
            .position(|name| Some(*name) == foot)
        {
            if let Some(ready) = self.state.contact_ready.get_mut(index) {
                *ready = false;
            }
            if let Some(pending) = self.state.contact_pending.get_mut(index) {
                *pending = false;
            }
        }
        let (head, forward) = self.head();
        let position = foot
            .and_then(|name| self.socket(name))
            .map_or(head, |m| m.transform_point3(Vec3::ZERO));
        let sequence = self.next_audio_sequence();
        self.sounds.push(Sound {
            epoch: self.audio_epoch,
            sequence,
            cursor: 0.0,
            update: false,
            owner: self.state.id.clone(),
            generation: self.state.voice_generation,
            cue: cue.into(),
            position,
            forward: foot.is_none().then_some(forward),
            looping,
            stop: false,
            gain: 1.0,
        });
        self.log("audio_command",&serde_json::json!({"cue":cue,"position":position,"direction":forward,"foot":foot,"looping":looping}));
    }

    fn voice(&mut self, desired: Option<&str>, until: f32) {
        if self.state.voice.as_deref() == desired {
            return;
        }
        if let Some(old) = self.state.voice.take() {
            let (position, forward) = self.head();
            let sequence = self.next_audio_sequence();
            self.sounds.push(Sound {
                epoch: self.audio_epoch,
                sequence,
                cursor: 0.0,
                update: false,
                owner: self.state.id.clone(),
                generation: self.state.voice_generation,
                cue: old,
                position,
                forward: Some(forward),
                looping: false,
                stop: true,
                gain: 0.0,
            });
        }
        self.state.voice_generation = self.state.voice_generation.saturating_add(1);
        if let Some(cue) = desired {
            self.state.voice = Some(cue.into());
            self.state.voice_until = until;
            self.state.voice_started = self.state.clock;
            self.emit(cue, None, cue == "tg_pursuit_shriek_loop");
        }
    }

    /// Direct body echoes are sampled only inside a physically valid cone and
    /// clear two-way path. The delayed return stores the sample, not a handle
    /// that can be dereferenced to a later hidden player pose.
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Finite bounded metre-space vector operations have no checked-vector alternative; integer counters use checked or saturating arithmetic."
    )]
    fn sense(
        &mut self,
        ctx: &WorldContext<'_>,
        leaves: &[DoorCollider],
        solids: &[WallAabb],
        scene: &dyn prop_track::Scene,
    ) {
        let now = self.state.clock;
        while self.state.pending.front().is_some_and(|r| r.deliver <= now) {
            let Some(mut received) = self.state.pending.pop_front() else {
                break;
            };
            let (head, _) = self.head();
            if now - received.observation.sampled > 0.35
                || if received.observation.track == 1 {
                    !scene.clear(head, received.hit)
                } else {
                    !scene.prey_return_clear(received.observation.track, head, received.hit)
                }
                || (received.observation.track == 1
                    && !sight_clear_with_solids(
                        ctx.index,
                        ctx.walls,
                        leaves,
                        solids,
                        head,
                        received.hit,
                        false,
                    ))
            {
                continue;
            }
            let prior_observation = self.state.observation.as_ref();
            let pair = prior_observation.filter(|p| p.correlates(&received.observation, now));
            received.observation.confidence = pair.map_or_else(
                || received.observation.quality.min(0.5),
                |p| p.quality.midpoint(received.observation.quality),
            );
            received.observation.confirmed =
                pair.is_some() && received.observation.confidence >= 0.72;
            if let Some(previous) = prior_observation {
                let elapsed = received.observation.sampled - previous.sampled;
                if elapsed > 0.01 {
                    received.observation.velocity =
                        ((received.observation.position - previous.position) / elapsed)
                            .clamp_length_max(12.0);
                }
            }
            let confirmed = received.observation.confirmed;
            self.log("echo_return",&serde_json::json!({"pulse":received.observation.pulse,"sampled":received.observation.sampled,"position":received.observation.position,"confirmed":confirmed,"outbound":received.origin,"return_origin":head,"confidence":(-0.3_f32).mul_add(now-received.observation.sampled,received.observation.confidence),"uncertainty":0.9_f32.mul_add(now-received.observation.sampled,received.observation.uncertainty),"track":received.observation.track}));
            if confirmed && !self.fresh() {
                self.state.episode = self.state.episode.saturating_add(1);
                self.state.search_deadline = 0.0;
            }
            self.state.observation = Some(received.observation);
        }
        if now < self.state.next_pulse
            || matches!(self.state.intent, Intent::Disabled | Intent::Inspect)
        {
            return;
        }
        if self.state.intent == Intent::EscapeFrustration
            && now > self.state.frustration_probe_until
        {
            return;
        }
        let (origin, axis) = self.head();
        self.state.pulse = self.state.pulse.saturating_add(1);
        let interval = if self.fresh() {
            0.65
        } else if self
            .state
            .observation
            .as_ref()
            .is_some_and(|o| now - o.sampled < 1.25)
        {
            0.6
        } else {
            2.6
        };
        // A suspended reaction may retain an old deadline. Restart its phase
        // within the current tick instead of compressing two probes on resume.
        let scheduled = self
            .state
            .next_pulse
            .max(now - ctx.delta_seconds.clamp(0.0, 0.1));
        self.state.next_pulse = cadence(scheduled, now, interval);
        self.log("echo_pulse",&serde_json::json!({"pulse":self.state.pulse,"origin":origin,"axis":axis,"range":14_u8,"cone":110_u8}));
        self.ensure_echo_carrier();
        if self.state.transit.is_none() && !self.fresh() && self.state.goal.is_none() {
            self.set_clip("howl_echo");
        }
        self.issue_return(ctx, origin, axis, scene, scheduled);
    }

    fn ensure_echo_carrier(&mut self) {
        if self.state.voice.is_none() {
            let cue = if self.fresh() {
                "tg_pursuit_shriek_loop"
            } else if self.state.pulse.is_multiple_of(2) {
                "tg_echolocation_howl_02"
            } else {
                "tg_echolocation_howl_01"
            };
            let until = if cue == "tg_pursuit_shriek_loop" {
                f32::MAX
            } else {
                self.state.clock + 5.6
            };
            self.voice(Some(cue), until);
        }
    }

    fn stop_pursuit(&mut self) {
        if self.state.voice.as_deref() == Some("tg_pursuit_shriek_loop") {
            self.voice(None, 0.0);
        }
    }

    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Finite metre-space acoustic path arithmetic uses bounded distances and normalized axes."
    )]
    fn issue_return(
        &mut self,
        ctx: &WorldContext<'_>,
        origin: Vec3,
        axis: Vec3,
        scene: &dyn prop_track::Scene,
        scheduled: f32,
    ) {
        let now = self.state.clock;
        if let Some(echo) = scene.reflect_prey(&self.definition.prey_targets, origin, axis)
            && !self
                .definition
                .passages
                .iter()
                .filter_map(|p| p.protected.as_ref())
                .any(|refuge| segment_box(origin, echo.hit, refuge.bounds()))
        {
            let distance = origin.distance(echo.hit);
            let quality = scene.echo_quality(origin, echo.hit);
            if quality > 0.0 && self.state.pending.len() < 4 {
                self.state.pending.push_back(Return {
                    observation: Observation {
                        position: echo.position,
                        heading: Some(echo.heading),
                        aim_point: echo.hit,
                        velocity: Vec3::ZERO,
                        sampled: now,
                        pulse: self.state.pulse,
                        quality: (0.95 - distance / 100.0) * quality,
                        confidence: 0.0,
                        uncertainty: (1.0 - quality).mul_add(2.5, 0.15),
                        track: echo.key,
                        confirmed: false,
                    },
                    origin,
                    hit: echo.hit,
                    deliver: now + (distance * 2.0 / 343.0).max(0.05),
                });
                self.state.next_pulse = self.state.next_pulse.min(cadence(scheduled, now, 0.6));
                return;
            }
        }
        let samples = [
            ctx.eye,
            ctx.feet + Vec3::Y * (ctx.body_height * 0.55),
            ctx.feet + Vec3::Y * (ctx.body_height * 0.15),
        ];
        let Some(sample) = samples.into_iter().find(|sample| {
            let delta = *sample - origin;
            let distance = delta.length();
            distance > 0.01
                && distance <= 14.0
                && axis.dot(delta / distance) >= 55.0_f32.to_radians().cos()
                && !self
                    .definition
                    .passages
                    .iter()
                    .filter_map(|p| p.protected.as_ref())
                    .any(|r| segment_box(origin, *sample, r.bounds()))
                && scene.clear(origin, *sample)
        }) else {
            return;
        };
        let distance = sample.distance(origin);
        let quality = scene.echo_quality(origin, sample);
        if quality <= 0.0 {
            return;
        }
        let uncertainty = (1.0 - quality).mul_add(2.5, 0.15);
        let position = if quality >= 0.99 {
            ctx.feet
        } else {
            let phase = f32::from(
                u16::try_from(self.state.pulse.wrapping_mul(40_503) & 65_535).unwrap_or(0),
            ) * (std::f32::consts::TAU / 65_536.0);
            ctx.feet + Vec3::new(phase.sin(), 0.0, phase.cos()) * (uncertainty - 0.15)
        };
        if self.state.pending.len() < 4 {
            self.state.pending.push_back(Return {
                observation: Observation {
                    position,
                    heading: None,
                    aim_point: sample + (position - ctx.feet),
                    velocity: Vec3::ZERO,
                    sampled: now,
                    pulse: self.state.pulse,
                    quality: (0.95 - distance / 100.0) * quality,
                    confidence: 0.0,
                    uncertainty,
                    track: 1,
                    confirmed: false,
                },
                origin,
                hit: sample,
                deliver: now + (distance * 2.0 / 343.0).max(0.05),
            });
        }
        // Preserve the scheduled pulse phase instead of accumulating frame
        // quantization whenever a valid return accelerates the next probe.
        self.state.next_pulse = self.state.next_pulse.min(cadence(scheduled, now, 0.6));
    }

    /// Physics impacts share causal release IDs through settling. Playback
    /// admission and master volume never decide whether this evidence exists.
    pub fn hear_impact(&mut self, impact: ImpactEvent) {
        if !self.enabled() || checkpoint::impact(&impact).is_err() {
            return;
        }
        if std::env::var_os("PLACES_BENCH").is_some() {
            self.log("raw_physics_contact", &serde_json::json!(impact));
        }
        let now = self.state.clock;
        if self
            .state
            .incidents
            .iter()
            .any(|(key, generation)| *key == impact.causal_source && *generation >= impact.incident)
        {
            return;
        }
        if self
            .state
            .source_cooldowns
            .iter()
            .any(|(key, until)| *key == impact.causal_source && *until > now)
        {
            return;
        }
        // Path validation is performed at the next authoritative world tick.
        self.queued_impact(impact);
    }

    fn queued_impact(&mut self, impact: ImpactEvent) {
        if self.state.impacts.len() == 32 {
            let _old = self.state.impacts.pop_front();
        }
        self.state.impacts.push_back((impact, self.state.clock));
    }

    fn heard(&mut self, stimuli: &[Stimulus], scene: &dyn prop_track::Scene) {
        self.capture_landings(scene);
        if self.fresh() {
            return;
        }
        self.admit_landing();
        self.hear_transients(stimuli, scene);
    }

    fn hear_transients(&mut self, stimuli: &[Stimulus], scene: &dyn prop_track::Scene) {
        if !matches!(
            self.state.intent,
            Intent::Patrol | Intent::Listen | Intent::Search | Intent::EscapeFrustration
        ) {
            return;
        }
        let (head, _) = self.head();
        for event in stimuli.iter().rev().take(8) {
            if (self.state.intent == Intent::EscapeFrustration && event.player_movement.is_none())
                || event.category == "impact"
                || event.category.starts_with("tg_")
                || event.category == "creature"
                || !event.heard_at(self.state.position, 22.0)
                || !event.is_fresh(0.0)
                || (event.player_movement.is_some()
                    && event.movement_serial == self.state.movement_serial)
            {
                continue;
            }
            let request = hearing::Request {
                listener: self.state.position,
                origin: head,
                source: event.position,
                radius: event.audible_radius(0.0),
                loudness: event.loudness,
                seed: event.movement_serial.unwrap_or(self.state.pulse),
                target_key: None,
            };
            let Some(evidence) = scene.hear(&request) else {
                continue;
            };
            if event.player_movement.is_some() {
                self.state.movement_serial = event.movement_serial;
            }
            if self.state.intent == Intent::EscapeFrustration {
                self.state.frustration_probe_until = self.state.clock + 1.4;
                self.state.next_pulse = self.state.next_pulse.min(self.state.clock + 0.1);
                self.log("passive_heard", &serde_json::json!({"category":event.category,"age":event.age,"evidence":evidence,"reaction_probe":true}));
                break;
            }
            self.state.goal = Some(Vec3::new(
                evidence.goal.x,
                self.state.position.y,
                evidence.goal.z,
            ));
            self.state.goal_time = self.state.clock - event.age;
            self.set_intent(Intent::Listen, "validated passive evidence; no prey lock");
            self.state.next_pulse = self.state.next_pulse.min(self.state.clock + 0.1);
            self.log(
                "passive_heard",
                &serde_json::json!({"category":event.category,"age":event.age,"evidence":evidence}),
            );
            break;
        }
    }

    fn accept_landing(&mut self, impact: ImpactEvent, at: f32, evidence: &hearing::Heard) {
        let radius = if impact.release == ImpactRelease::Dropped {
            1.0
        } else {
            impact.strength * 8.0
        };
        // One newest accepted generation per causal body. Unattended late
        // bounces cannot become new incidents after unrelated throws fill a FIFO.
        self.state
            .incidents
            .retain(|(key, _)| *key != impact.causal_source);
        if self.state.incidents.len() == 128 {
            let _old = self.state.incidents.pop_front();
        }
        self.state
            .incidents
            .push_back((impact.causal_source, impact.incident));
        self.state
            .source_cooldowns
            .retain(|(_, until)| *until > self.state.clock);
        if self.state.source_cooldowns.len() == 128 {
            let _old = self.state.source_cooldowns.pop_front();
        }
        self.state
            .source_cooldowns
            .push_back((impact.causal_source, self.state.clock + 3.0));
        if impact.release == ImpactRelease::Thrown && impact.key == impact.causal_source {
            self.begin_prop_track(
                crate::engine::physics::ReleaseIdentity {
                    key: impact.key,
                    generation: impact.incident,
                    kind: ImpactRelease::Thrown,
                },
                evidence.estimated,
                at,
            );
        } else {
            self.state.prop_track = None;
        }
        self.state.goal = Some(Vec3::new(
            evidence.goal.x,
            self.state.position.y,
            evidence.goal.z,
        ));
        // A new accepted physical sound starts a separate investigation. Minor
        // passive Listen events cannot extend the previous prey search cap.
        self.state.search_deadline = 0.0;
        self.state.noise_deadline = self.state.clock + 8.0;
        self.state.noise_progress = self.state.position;
        self.state.noise_portal = !evidence.portals.is_empty();
        self.state.goal_time = at;
        self.next_plan = 0.0;
        self.path.clear();
        self.set_intent(
            if impact.release == ImpactRelease::Dropped
                || evidence.transmission < 0.99
                || evidence.quality < 0.5
            {
                Intent::Listen
            } else {
                Intent::InvestigateNoise
            },
            "audible physical landing",
        );
        self.log("stimulus_heard",&serde_json::json!({"source":impact.causal_source,"incident":impact.incident,"evidence":evidence,"release":impact.release,"radius":radius}));
    }

    fn search_end(&self) -> f32 {
        let end = self.state.clock + 6.0;
        if self.state.search_deadline > 0.0 {
            end.min(self.state.search_deadline)
        } else {
            end
        }
    }

    fn request_loss(&mut self) {
        if self.state.search_deadline <= 0.0 {
            self.state.search_deadline = self.state.clock + 10.0;
        }
        if self.state.episode > self.state.frustrated
            && self.state.clock >= self.state.frustration_until
        {
            self.state.frustrated = self.state.episode;
            self.state.frustration_until = self.state.clock + 6.0;
            self.state.frustration_probe_until = 0.0;
            self.set_intent(Intent::EscapeFrustration, "confirmed prey evidence expired");
            self.set_clip("lost_prey_agony");
            self.stop_pursuit();
        } else {
            self.set_intent(Intent::Search, "bounded last-observation search");
            self.state.search_until = self.search_end();
        }
    }

    fn choose_intent(&mut self) {
        if self.state.intent == Intent::Disabled {
            return;
        }
        if self.state.transit.is_some() {
            if self.fresh() {
                self.state.pending_loss = false;
                self.set_intent(Intent::Chase, "fresh body echoes during committed passage");
            } else if self.state.intent == Intent::Chase {
                self.state.pending_loss = true;
                self.set_intent(
                    Intent::Search,
                    "lock expired; physical passage retains safe posture",
                );
                self.stop_pursuit();
            }
            return;
        }
        if !self.fresh()
            && self.state.search_deadline > 0.0
            && self.state.clock >= self.state.search_deadline
        {
            self.set_intent(Intent::Patrol, "episode search cap");
            self.state.search_deadline = 0.0;
            self.state.goal = None;
            self.set_clip("idle_listen");
            self.stop_pursuit();
        }
        if self.state.intent == Intent::InvestigateNoise
            && self.state.noise_deadline > 0.0
            && self.state.clock >= self.state.noise_deadline
        {
            self.set_intent(Intent::Listen, "bounded noise approach expired");
            self.state.prop_track = None;
            self.state.goal = None;
            self.stop_pursuit();
        }
        if self.fresh() {
            let safe_stomp = self.state.intent != Intent::EscapeFrustration
                || !((self.state.clip_time > 0.32 && self.state.clip_time < 0.96)
                    || (self.state.clip_time > 1.12 && self.state.clip_time < 1.76));
            if safe_stomp
                && !(self.state.intent == Intent::Inspect
                    && self.state.clip_time > 1.15
                    && self.state.clip_time < 3.7)
            {
                self.set_intent(Intent::Chase, "two fresh correlated body echoes");
                if let Some(observed) = &self.state.observation {
                    self.state.goal = Some(observed.position);
                }
            }
            return;
        }
        if self.state.intent == Intent::Chase {
            self.request_loss();
        }
        if self.state.intent == Intent::EscapeFrustration && self.state.clip_time >= 4.0 {
            self.set_intent(
                Intent::Search,
                "safe agony animation completion; voice tail retained",
            );
            self.state.search_until = self.search_end();
        }
        if self.state.intent == Intent::Inspect && self.state.clip_time >= 4.8 {
            self.set_intent(Intent::Search, "inspection completed without fresh prey");
            self.state.search_until = self.search_end();
            self.state.goal = None;
        }
        if self.state.intent == Intent::Listen && self.state.clock - self.state.entered > 2.6 {
            self.set_intent(Intent::Search, "passive evidence did not confirm prey");
            self.state.search_until = self.search_end();
        }
        if self.state.intent == Intent::Search && self.state.clock >= self.state.search_until {
            self.set_intent(Intent::Patrol, "search deadline");
            self.state.goal = None;
        }
        if self.state.intent == Intent::BlockedAtRefuge
            && self.state.clock - self.state.entered >= 1.0
        {
            self.request_loss();
        }
        if self.state.intent == Intent::Patrol && !self.definition.patrol.is_empty() {
            let count = self.definition.patrol.len();
            if let Some(point) = self
                .definition
                .patrol
                .get(self.state.patrol_index.checked_rem(count).unwrap_or(0))
            {
                let destination = Vec3::from(*point);
                if self.state.position.distance(destination) < 0.1 {
                    self.state.patrol_index = self
                        .state
                        .patrol_index
                        .saturating_add(1)
                        .checked_rem(count)
                        .unwrap_or(0);
                } else {
                    self.state.goal = Some(destination);
                }
            }
        }
    }

    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Validated protected bounds are expanded by the fixed movement capsule margins."
    )]
    fn excluded_refuges(&self) -> Vec<NavBounds> {
        self.definition
            .passages
            .iter()
            .filter(|passage| passage.crouch_only)
            .filter_map(|passage| passage.protected.as_ref())
            .map(|refuge| NavBounds {
                min: Vec3::from(refuge.min) - Vec3::new(1.03, 3.64, 1.03),
                max: Vec3::from(refuge.max) + Vec3::new(1.03, 0.0, 1.03),
            })
            .collect()
    }

    fn staging_goal(&self, goal: Vec3) -> Vec3 {
        self.stages
            .iter()
            .filter(|stage| !stage.passage.crouch_only || self.refuge_route != RefugeRoute::Detour)
            .find_map(|stage| stage.approach(self.state.position, goal))
            .unwrap_or(goal)
    }

    fn waypoint(&mut self, ctx: &WorldContext<'_>, goal: Vec3) -> Option<Vec3> {
        let nav = ctx.nav?;
        let class = nav.compatible_class_index(&navigation_class())?;
        if self.state.clock >= self.next_plan {
            let staged = self
                .stages
                .iter()
                .filter(|stage| !stage.passage.crouch_only)
                .find_map(|stage| stage.approach(self.state.position, goal))
                .unwrap_or(goal);
            let excluded = self.excluded_refuges();
            let mut query = PathQuery {
                class,
                start: self.state.position,
                goal: staged,
                can_open_doors: false,
                max_expansions: 16_384,
                doors: &crate::engine::nav::NoDoors,
            };
            let result =
                nav.path_into_avoiding(&query, &mut self.scratch, &mut self.path, &excluded);
            self.refuge_route = if result.is_ok() && self.path.complete {
                RefugeRoute::Detour
            } else {
                RefugeRoute::Unresolved
            };
            if self.refuge_route != RefugeRoute::Detour {
                query.goal = self.staging_goal(goal);
                let _result =
                    nav.path_into_avoiding(&query, &mut self.scratch, &mut self.path, &excluded);
            }
            self.path_cursor = 1;
            self.next_plan = self.state.clock + 0.4;
        }
        let destination = self.staging_goal(goal);
        if self.state.position.distance(destination) < 0.35 {
            return Some(destination);
        }
        while let Some(point) = self.path.waypoints.get(self.path_cursor).copied() {
            if point.distance(self.state.position) > 0.08 {
                return Some(point);
            }
            self.path_cursor = self.path_cursor.saturating_add(1);
        }
        None
    }

    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Finite bounded metre-space vector operations have no checked-vector alternative; integer counters use checked or saturating arithmetic."
    )]
    fn maybe_passage(&mut self, goal: Vec3, sub: f32) -> bool {
        let candidate = self
            .stages
            .iter()
            .filter(|stage| !stage.passage.crouch_only || self.refuge_route != RefugeRoute::Detour)
            .find_map(|stage| {
                stage.approach(self.state.position, goal).map(|entry| {
                    (
                        entry,
                        Vec3::from(stage.passage.exit),
                        stage.passage.crouch_only,
                        stage.passage.id.clone(),
                    )
                })
            });
        let Some((mut entry, exit, refuge, id)) = candidate else {
            return false;
        };
        if refuge {
            if self.state.position.distance(entry) < 0.12 {
                if self.state.blocked_refuge.as_deref() != Some(&id) {
                    self.state.blocked_refuge = Some(id.clone());
                    self.log("traversal_changed", &serde_json::json!({"passage":id,"phase":"Blocked","class":"CrouchOnlyPassage"}));
                }
                // Only the authored link is unavailable. Exposed prey on this
                // side remains vulnerable to a fresh, legal physical contact.
                if !self.fresh() {
                    self.state.goal = None;
                    self.set_intent(Intent::BlockedAtRefuge, "explicit crouch-only refuge");
                    self.voice(None, 0.0);
                }
                return true;
            }
            return false;
        }
        if self.state.position.distance(entry) > 0.08 {
            return false;
        }
        entry = self.state.position;
        let direction = (exit - entry).normalize_or_zero();
        let yaw = direction.x.atan2(direction.z);
        if crate::engine::entity::angle_difference(self.state.yaw, yaw).abs() > 0.03 {
            self.state.yaw = crate::engine::entity::turn_toward(
                self.state.yaw,
                yaw,
                TURN_RATE.to_radians() * sub,
            );
            if crate::engine::entity::angle_difference(self.state.yaw, yaw).abs() >= 0.3 {
                self.set_clip("walk_slow");
                self.state.motion_rate = TURN_RATE / 20.0;
            } else {
                // Small portal alignment retains the braking pursuit stance.
                self.state.motion_rate = 0.0;
            }
            return true;
        }
        if self.state.aim_weight > 0.001
            || self.state.base_posture > 0.001
            || self.state.blend_time < BLEND
        {
            return true;
        }
        if self.state.clock < self.next_passage_check {
            return true;
        }
        if self.passage_plan.is_none() {
            self.passage_credit = 0.0;
            let mut sampled = self.state.clone();
            sampled.previous.clone_from(&sampled.clip);
            sampled.previous_time = sampled.clip_time;
            sampled.clip = "door_enter".into();
            sampled.clip_time = 0.0;
            sampled.blend_time = 0.0;
            sampled.pending_loss = false;
            sampled.transit = Some(Transit {
                entry,
                exit,
                yaw,
                phase: 0,
                elapsed: 0.0,
                progress: 0.0,
            });
            self.passage_plan = Some(PassagePlan {
                id,
                entry,
                exit,
                yaw,
                sampled,
                next_sample: 0.0,
                total: POSTURE_SECONDS.mul_add(2.0, entry.distance(exit) / SQUEEZE),
            });
        }
        true
    }

    /// Bounded work per simulation tick; never stalls a frame on a full mesh
    /// sweep. Dynamic occupancy is checked again on every committed substep.
    fn check_passage_plan(
        &mut self,
        ctx: &WorldContext<'_>,
        leaves: &[DoorCollider],
        solids: &[WallAabb],
    ) {
        let Some(mut plan) = self.passage_plan.take() else {
            return;
        };
        let live = self.state.clone();
        let physical = self.animator.joint_pose();
        self.state = plan.sampled;
        self.posing_candidate = true;
        self.candidate_events.clear();
        let mut legal = true;
        let mut failure = serde_json::Value::Null;
        self.passage_credit = ctx.delta_seconds.mul_add(2400.0, self.passage_credit);
        for _ in 0_u8..120 {
            if self.passage_credit < 1.0 {
                break;
            }
            self.passage_credit -= 1.0;
            self.state.yaw = plan.yaw;
            if !self.clear(ctx, leaves, solids) {
                let wall = ctx
                    .walls
                    .iter()
                    .chain(solids)
                    .find(|wall| self.intersects(**wall, MARGIN));
                let ground = self.vertices.iter().enumerate().find(|(_, point)| {
                    ctx.floor
                        .walk_height_near(point.x, point.z, self.state.position.y, 0.4)
                        .is_some_and(|height| point.y + 0.005 < height)
                }).map(|(index, point)| serde_json::json!({"point":point,
                    "bind": self.asset.model.vertices.get(index).map(|v|v.pos),
                    "joints": self.asset.model.joints.get(index), "weights":self.asset.model.weights.get(index)}));
                failure = serde_json::json!({"position":self.state.position,"clip":self.state.clip,"clip_time":self.state.clip_time,
                    "wall":wall.map(|w|[w.min_x,w.min_y,w.min_z,w.max_x,w.max_y,w.max_z]),"ground_point":ground});
                legal = false;
                break;
            }
            if self.state.transit.is_none() {
                break;
            }
            self.advance_motion(ctx, leaves, solids, 1.0 / 240.0);
            self.advance_pose_clocks(1.0 / 240.0);
            plan.next_sample += 1.0 / 240.0;
            if plan.next_sample > plan.total + 0.05 {
                legal = false;
                failure = serde_json::json!({"reason":"bounded passage time exhausted"});
                break;
            }
        }
        self.posing_candidate = false;
        self.candidate_events.clear();
        plan.sampled = self.state.clone();
        self.state = live;
        let _restored = self.restore_physical(&physical);
        if !legal {
            self.next_passage_check = self.state.clock + 0.5;
            self.log("sweep_rejected",&serde_json::json!({"passage":plan.id,"at":plan.next_sample,"class":"BlockedPassage","whole_trajectory":true,"failure":failure}));
        } else if plan.sampled.transit.is_some() {
            self.passage_plan = Some(plan);
        } else {
            self.retreat_poses.clear();
            self.retain_passage_pose(&self.state.clone(), physical);
            self.recovery = recovery::Mode::Forward;
            self.state.transit = Some(Transit {
                entry: plan.entry,
                exit: plan.exit,
                yaw: plan.yaw,
                phase: 0,
                elapsed: 0.0,
                progress: 0.0,
            });
            self.set_clip("door_enter");
            self.log(
                "traversal_changed",
                &serde_json::json!({"passage":plan.id,"phase":"Compress","world_scale":1_u8}),
            );
        }
    }

    fn traverse(&mut self, dt: f32) {
        let Some(mut transit) = self.state.transit.take() else {
            return;
        };
        transit.elapsed += dt;
        self.state.yaw = transit.yaw;
        match transit.phase {
            0 => {
                self.set_clip("door_enter");
                if transit.elapsed >= POSTURE_SECONDS {
                    transit.phase = 1;
                    transit.elapsed = 0.0;
                    self.set_clip("door_squeeze");
                }
            }
            1 => {
                self.set_clip("door_squeeze");
                let distance = transit.entry.distance(transit.exit);
                transit.progress = (transit.progress + dt * SQUEEZE / distance).min(1.0);
                self.state.position = transit.entry.lerp(transit.exit, transit.progress);
                if transit.progress >= 1.0 {
                    transit.phase = 2;
                    transit.elapsed = 0.0;
                    self.set_clip("door_exit");
                }
            }
            _ => {
                self.set_clip("door_exit");
                if transit.elapsed >= POSTURE_SECONDS {
                    self.log(
                        "traversal_changed",
                        &serde_json::json!({"phase":"Complete","position":self.state.position}),
                    );
                    if self.state.pending_loss && !self.fresh() {
                        self.state.pending_loss = false;
                        self.request_loss();
                    }
                    return;
                }
            }
        }
        self.state.transit = Some(transit);
    }

    /// One bounded simulation interval, including live doors, solids and player
    /// occupancy. A failed candidate retains the last actual pose; it cannot
    /// change scale, teleport, disable collision or freeze evidence age.
    pub fn tick(
        &mut self,
        ctx: &WorldContext<'_>,
        leaves: &[DoorCollider],
        solids: &[WallAabb],
        stimuli: &[Stimulus],
        prop_scene: &dyn prop_track::Scene,
    ) {
        let dt = ctx.delta_seconds.clamp(0.0, 0.1);
        if dt <= 0.0 {
            return;
        }
        let walk =
            if ctx.player_walk_speed.is_finite() && (1.0..=10.0).contains(&ctx.player_walk_speed) {
                ctx.player_walk_speed
            } else {
                3.75
            };
        self.state.base_speed = walk * 1.1;
        self.state.pursuit_speed = walk * 1.2;
        self.state.clock += dt;
        if !self.activate_if_clear(ctx, leaves, solids) {
            return;
        }
        if self.state.intent == Intent::InvestigateNoise
            && self.state.position.distance(self.state.noise_progress) > 0.2
        {
            self.state.noise_progress = self.state.position;
            self.state.noise_deadline = self.state.clock + 8.0;
        }
        if self.state.voice.is_some() && self.state.clock >= self.state.voice_until {
            self.voice(None, 0.0);
        }
        self.sense(ctx, leaves, solids, prop_scene);
        self.heard(stimuli, prop_scene);
        self.sense_prop(prop_scene, ctx);
        self.choose_intent();
        self.constrain_prediction(ctx, leaves, solids, prop_scene);
        if matches!(
            self.state.intent,
            Intent::EscapeFrustration | Intent::Disabled
        ) {
            self.passage_plan = None;
        }
        if self.passage_plan.is_some() {
            self.check_passage_plan(ctx, leaves, solids);
        }
        let mut left = dt;
        for _ in 0_u8..96 {
            if left <= 0.000_01 {
                break;
            }
            let sub = left.min((1.0 / 120.0) / self.state.motion_rate.abs().max(1.0));
            left -= sub;
            self.safe_step(ctx, leaves, solids, sub, prop_scene);
        }
        self.check_attack(ctx, leaves, solids, prop_scene);
        self.maintain_voice();
    }

    fn safe_step(
        &mut self,
        ctx: &WorldContext<'_>,
        leaves: &[DoorCollider],
        solids: &[WallAabb],
        sub: f32,
        scene: &dyn prop_track::Scene,
    ) {
        if self.retreat_step(ctx, leaves, solids) {
            return;
        }
        let saved = self.state.clone();
        let physical = self.animator.joint_pose();
        let saved_plan = self.passage_plan.clone();
        let saved_sound_count = self.sounds.len();
        self.candidate_events.clear();
        self.posing_candidate = true;
        let old_time = self.state.clip_time;
        let old_clip = self.state.clip.clone();
        self.advance_motion(ctx, leaves, solids, sub);
        let before = if old_clip == self.state.clip {
            old_time
        } else {
            -0.000_01
        };
        let stationary = self.stationary_step(&saved, sub);
        self.advance_pose_clocks(sub);
        if self.clear(ctx, leaves, solids) {
            self.posing_candidate = false;
            while let Some(event) = self.candidate_events.pop_front() {
                self.store_event(event);
            }
            self.accepted_contact_step(stationary, sub);
            if self.state.transit.is_some() {
                self.retain_passage_pose(&saved, physical);
            } else {
                self.retreat_poses.clear();
            }
            let after = if self.state.clip_time < before && looped(&self.state.clip) {
                self.state.clip_time + duration(&self.state.clip)
            } else {
                self.state.clip_time
            };
            self.update_markers(before, after, ctx, scene);
            return;
        }

        self.posing_candidate = false;
        self.candidate_events.clear();
        self.sounds.truncate(saved_sound_count);
        self.passage_plan = saved_plan;
        let now = self.state.clock;
        self.state = saved;
        self.state.clock = now;
        let _restored = self.restore_physical(&physical);
        self.state.speed = 0.0;
        if self.release_aim_clearance(ctx, leaves, solids, sub)
            || self.step_in_place(ctx, leaves, solids, sub, scene)
        {
            return;
        }
        self.state.blocked_seconds += sub;
        if self.state.blocked_seconds >= 2.0 {
            self.log("blocked",&serde_json::json!({"seconds":self.state.blocked_seconds,"transit":self.state.transit.is_some(),"recovery":"retained last safe pose; replan"}));
            self.recover_blockage();
            self.state.blocked_seconds = 0.0;
        }
    }

    fn release_aim_clearance(
        &mut self,
        ctx: &WorldContext<'_>,
        leaves: &[DoorCollider],
        solids: &[WallAabb],
        sub: f32,
    ) -> bool {
        if self.state.aim_weight <= 0.0 {
            return false;
        }
        let old = self.state.clone();
        let physical = self.animator.joint_pose();
        self.state.aim_weight = 1.6_f32.mul_add(-sub, self.state.aim_weight).max(0.0);
        if self.clear(ctx, leaves, solids) {
            return true;
        }
        self.state = old;
        let _restored = self.restore_physical(&physical);
        false
    }

    pub(super) fn retime_restored_posture(&mut self) {
        let Some(transit) = self.state.transit.as_mut() else {
            return;
        };
        if transit.phase == 1 || !matches!(self.state.clip.as_str(), "door_enter" | "door_exit") {
            return;
        }
        let expected = posture_clip_time(&self.state.clip, &self.state.previous, transit.elapsed);
        if (expected - self.state.clip_time).abs() <= 0.0001 {
            return;
        }
        if self.state.clip_time <= posture_clip_time(&self.state.clip, &self.state.previous, 0.0) {
            transit.elapsed = 0.0;
            return;
        }
        // Retain old saved poses and markers while mapping only the action clock
        // to their original phase. Evidence timestamps are unchanged.
        let mut lower = 0.0_f32;
        let mut upper = POSTURE_SECONDS;
        for _ in 0_u8..20 {
            let middle = lower.midpoint(upper);
            if posture_clip_time(&self.state.clip, &self.state.previous, middle)
                < self.state.clip_time
            {
                lower = middle;
            } else {
                upper = middle;
            }
        }
        transit.elapsed = lower.midpoint(upper);
    }

    fn advance_pose_clocks(&mut self, sub: f32) {
        self.state.support_lowering = (-1.2_f32)
            .mul_add(sub, self.state.support_lowering)
            .max(0.0);
        self.state.blend_time = (self.state.blend_time + sub).min(BLEND);
        if let Some(transit) = self.state.transit.as_ref()
            && transit.phase != 1
            && matches!(self.state.clip.as_str(), "door_enter" | "door_exit")
        {
            // Keep close staging and the 0.8s action. The authored curve already
            // eases the bend; another phase ease amplified its middle velocity.
            // Smooth crossfading and original contact phases are preserved.
            self.state.clip_time =
                posture_clip_time(&self.state.clip, &self.state.previous, transit.elapsed);
        } else {
            self.state.clip_time = sub.mul_add(self.state.motion_rate, self.state.clip_time);
        }
        if looped(&self.state.clip) {
            self.state.clip_time = self.state.clip_time.rem_euclid(duration(&self.state.clip));
        } else {
            self.state.clip_time = self.state.clip_time.min(duration(&self.state.clip));
        }
        self.resolve_aim(sub);
    }

    fn advance_motion(
        &mut self,
        ctx: &WorldContext<'_>,
        leaves: &[DoorCollider],
        solids: &[WallAabb],
        sub: f32,
    ) {
        self.state.motion_rate = 1.0;
        if self.passage_plan.is_some() {
            self.state.motion_rate = 0.0;
            self.state.speed = 0.0;
            return;
        }
        if self.state.transit.is_some() {
            self.state.speed = 0.0;
            self.traverse(sub);
            self.state.motion_rate = if self.state.transit.as_ref().is_some_and(|t| t.phase == 1) {
                SQUEEZE / SQUEEZE_REFERENCE
            } else {
                POSTURE_RATE
            };
        } else if matches!(
            self.state.intent,
            Intent::Chase
                | Intent::InvestigateNoise
                | Intent::Patrol
                | Intent::Search
                | Intent::Listen
        ) {
            if let Some(goal) = self.state.goal {
                if !self.maybe_passage(goal, sub) {
                    let distance = self.state.position.distance(goal);
                    if self.state.intent == Intent::InvestigateNoise
                        && distance <= self.inspection_standoff() + 0.02
                        && (self.prop_ready() || self.state.noise_portal)
                        && self.state.speed <= 0.02
                        && self.state.clock - self.state.goal_time >= 0.4
                    {
                        if self.state.noise_portal {
                            self.set_intent(
                                Intent::Listen,
                                "coarse portal region reached; exact source unproved",
                            );
                            self.set_clip("idle_listen");
                            self.stop_pursuit();
                        } else {
                            self.set_intent(Intent::Inspect, "measured lowered-head standoff");
                            self.set_clip("inspect_prop");
                            self.voice(None, 0.0);
                        }
                    } else if let Some(waypoint) = self.waypoint(ctx, goal) {
                        self.steer(ctx, leaves, solids, waypoint, distance, sub);
                    }
                }
            } else if self.state.clip != "howl_echo" || self.state.clip_time >= 3.2 {
                self.set_clip("idle_listen");
            }
        }
    }

    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Finite metre-space steering uses bounded vectors, normalized directions and positive simulation deltas."
    )]
    fn steer(
        &mut self,
        ctx: &WorldContext<'_>,
        leaves: &[DoorCollider],
        solids: &[WallAabb],
        waypoint: Vec3,
        distance: f32,
        sub: f32,
    ) {
        let fast = matches!(
            self.state.intent,
            Intent::Chase | Intent::InvestigateNoise | Intent::Patrol
        );
        let cap = if self.state.intent == Intent::Patrol {
            self.state.base_speed
        } else if fast {
            self.state.pursuit_speed
        } else {
            WALK
        };
        let reference = if fast { RUN_REFERENCE } else { WALK };
        let remaining = if self.state.intent == Intent::InvestigateNoise {
            (distance - self.inspection_standoff()).max(0.0)
        } else {
            self.state
                .position
                .distance(self.staging_goal(self.state.goal.unwrap_or(waypoint)))
        };
        if let Some(observation) = self
            .state
            .observation
            .as_ref()
            .filter(|observation| observation.track != 1)
            && remaining < 0.35
            && self.fresh()
        {
            let heading = observation.heading.unwrap_or(self.state.yaw);
            self.state.yaw = crate::engine::entity::turn_toward(
                self.state.yaw,
                heading,
                TURN_RATE.to_radians() * sub,
            );
            if let Some(goal) = self.state.goal {
                let acceleration = self.state.base_speed * 2.0;
                let desired = (2.0 * acceleration * remaining).sqrt().min(0.35);
                self.state.speed +=
                    (desired - self.state.speed).clamp(-acceleration * sub, acceleration * sub);
                let step = (goal - self.state.position).clamp_length_max(self.state.speed * sub);
                self.state.position += step;
                self.state.speed = step.length() / sub;
                self.state.motion_rate = (self.state.speed / RUN_REFERENCE).clamp(0.0, 7.0);
                self.set_clip(if self.state.speed > 0.02 {
                    "run_pursuit"
                } else {
                    "idle_listen"
                });
            }
            return;
        }
        let acceleration = self.state.base_speed * 2.0;
        let desired = cap.min((2.0 * acceleration * remaining).sqrt());
        self.state.speed +=
            (desired - self.state.speed).clamp(-acceleration * sub, acceleration * sub);
        let direction = (waypoint - self.state.position).normalize_or_zero();
        let yaw = direction.x.atan2(direction.z);
        self.state.yaw =
            crate::engine::entity::turn_toward(self.state.yaw, yaw, TURN_RATE.to_radians() * sub);
        if crate::engine::entity::angle_difference(self.state.yaw, yaw).abs() >= 0.3 {
            self.state.speed = 0.0;
            self.set_clip("walk_slow");
            self.state.motion_rate = TURN_RATE / 20.0;
        } else {
            let mut mover = crate::engine::ai::movement::AgentMove {
                position: self.state.position,
                yaw_degrees: self.state.yaw.to_degrees(),
                radius: 0.46,
                height: 1.6,
                step_height: 0.4,
                max_slope: 2.6667,
                speed_mps: self.state.speed,
            };
            if let crate::engine::ai::movement::MoveStep::Moved {
                position,
                yaw_degrees: _,
                speed_mps: _,
            } = mover.step_with_solids(
                waypoint,
                sub,
                ctx.walls,
                ctx.index,
                ctx.floor,
                &self.ceiling,
                leaves,
                solids,
            ) {
                let actual = self.state.position.distance(position) / sub;
                self.state.position = position;
                self.set_clip(if fast { "run_pursuit" } else { "walk_slow" });
                self.state.motion_rate = (actual / reference).clamp(0.0, 7.0);
            }
        }
    }

    /// Authored distributed fitting passed the independent native surface,
    /// turn and clearance gate. Direct head yaw/pitch offsets remain disabled.
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Bounded aim fitting compares normalized vectors to a previously measured body point, never live hidden target coordinates."
    )]
    fn resolve_aim(&mut self, sub: f32) {
        let clearance = self.state.transit.is_some()
            || self.passage_plan.is_some()
            || self.state.clip.starts_with("door_")
            || self.state.clip == "inspect_prop"
            || (self.state.blend_time < BLEND
                && (self.state.previous.starts_with("door_")
                    || self.state.previous == "inspect_prop"))
            || self.state.goal.is_some_and(|goal| {
                self.stages.iter().any(|stage| {
                    !stage.passage.crouch_only
                        && stage.approach(self.state.position, goal).is_some()
                })
            });
        let desired_base = if self.state.intent == Intent::Patrol
            && self.state.clip == "run_pursuit"
            && !clearance
        {
            1.0
        } else {
            0.0
        };
        self.state.base_posture +=
            (desired_base - self.state.base_posture).clamp(-4.0 * sub, 4.0 * sub);
        let target = if self.distributed_aim
            && self.fresh()
            && self.state.intent == Intent::Chase
            && !clearance
        {
            self.state
                .observation
                .as_ref()
                .map(|o| self.state.goal.unwrap_or(o.position) + (o.aim_point - o.position))
        } else {
            None
        };
        let mut desired = 0.0;
        let mut best = -1.0_f32;
        if let Some(point) = target {
            for step in 0_u8..=16 {
                let weight = f32::from(step) / 20.0;
                self.sample_base();
                let _aim = self.animator.blend_upper_inspection(weight);
                let (origin, axis) = self.head();
                let direction = (point - origin).normalize_or_zero();
                let score = axis.dot(direction);
                if score > best {
                    best = score;
                    desired = weight;
                }
            }
        }
        self.state.aim_weight += (desired - self.state.aim_weight).clamp(-1.6 * sub, 1.6 * sub);
    }

    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Prediction uses finite observed vectors for at most 0.6 seconds; the collision query rejects blocked extrapolation."
    )]
    fn constrain_prediction(
        &mut self,
        ctx: &WorldContext<'_>,
        leaves: &[DoorCollider],
        solids: &[WallAabb],
        scene: &dyn prop_track::Scene,
    ) {
        if self.state.intent != Intent::Chase || self.state.transit.is_some() {
            return;
        }
        let Some(observed) = self.state.observation.as_ref() else {
            return;
        };
        let age = self.state.clock - observed.sampled;
        let offset = if age <= 0.6 {
            observed.velocity * age.max(0.0)
        } else {
            Vec3::ZERO
        };
        let predicted = observed.aim_point + offset;
        if if observed.track == 1 {
            scene.clear(observed.aim_point, predicted)
        } else {
            scene.prey_return_clear(observed.track, observed.aim_point, predicted)
        } && (observed.track != 1
            || sight_clear_with_solids(
                ctx.index,
                ctx.walls,
                leaves,
                solids,
                observed.aim_point,
                predicted,
                false,
            ))
        {
            let standoff = if observed.track == 1 {
                Vec3::ZERO
            } else {
                Quat::from_rotation_y(observed.heading.unwrap_or(self.state.yaw)) * prey::SETUP
            };
            self.state.goal = Some(observed.position + offset - standoff);
        }
    }

    fn maintain_voice(&mut self) {
        if matches!(self.state.intent, Intent::Chase | Intent::InvestigateNoise)
            && self.state.transit.is_none()
        {
            self.voice(Some("tg_pursuit_shriek_loop"), f32::MAX);
        }
        if self.state.voice.is_some() {
            let (position, forward) = self.head();
            let sequence = self.next_audio_sequence();
            self.sounds.push(Sound {
                epoch: self.audio_epoch,
                sequence,
                cursor: f64::from(self.state.clock - self.state.voice_started),
                update: true,
                owner: self.state.id.clone(),
                generation: self.state.voice_generation,
                cue: self.state.voice.clone().unwrap_or_default(),
                position,
                forward: Some(forward),
                looping: self.state.voice.as_deref() == Some("tg_pursuit_shriek_loop"),
                stop: false,
                gain: 1.0,
            });
        }
    }

    fn gait_contacts(&mut self, name: &str, before: f32, after: f32) {
        let seconds = duration(name);
        for (index, foot) in ["front", "left", "right"].into_iter().enumerate() {
            let phase = match index {
                0 => 0.0,
                1 => 1.0 / 3.0,
                _ => 2.0 / 3.0,
            };
            let crossed =
                ((before / seconds) - phase).floor() < ((after / seconds) - phase).floor();
            if crossed
                && self
                    .state
                    .contact_ready
                    .get(index)
                    .copied()
                    .unwrap_or(false)
                && let Some(pending) = self.state.contact_pending.get_mut(index)
            {
                *pending = true;
            }
            if self
                .state
                .contact_pending
                .get(index)
                .copied()
                .unwrap_or(false)
                && self.state.plants.get(index).is_some_and(Option::is_some)
            {
                self.emit(
                    &format!(
                        "tg_foot_{}_{}",
                        if name == "run_pursuit" {
                            "fast"
                        } else {
                            "slow"
                        },
                        foot
                    ),
                    Some(&format!("foot_{foot}")),
                    false,
                );
            }
        }
    }

    fn update_markers(
        &mut self,
        before: f32,
        after: f32,
        ctx: &WorldContext<'_>,
        scene: &dyn prop_track::Scene,
    ) {
        let name = self.state.clip.clone();
        for (plant, ready) in self.state.plants.iter().zip(&mut self.state.contact_ready) {
            if plant.is_none() {
                *ready = true;
            }
        }
        if matches!(name.as_str(), "walk_slow" | "run_pursuit" | "door_squeeze") {
            self.gait_contacts(&name, before, after);
        } else {
            self.state.contact_pending = [false; 3];
        }
        if matches!(name.as_str(), "door_enter" | "door_exit") {
            for (at, foot, cue) in [
                (0.54, "foot_left", "tg_foot_slow_left"),
                (1.02, "foot_right", "tg_foot_slow_right"),
            ] {
                if before < at && after >= at {
                    self.emit(cue, Some(foot), false);
                }
            }
        }
        if name == "inspect_prop" {
            for (at, cue) in [
                (1.45, "tg_click_single_01"),
                (2.0, "tg_click_single_02"),
                (2.55, "tg_click_single_03"),
            ] {
                if before < at && after >= at {
                    self.emit(cue, None, false);
                    self.state.pulse = self.state.pulse.saturating_add(1);
                    let (origin, axis) = self.head();
                    self.log("echo_pulse",&serde_json::json!({"pulse":self.state.pulse,"origin":origin,"axis":axis,"inspection_marker":at,"range":14_u8,"cone":110_u8}));
                    self.issue_return(ctx, origin, axis, scene, self.state.clock);
                    self.request_prop_click();
                }
            }
        }
        if name == "lost_prey_agony" {
            for (at, foot) in [(0.96, "foot_front"), (1.76, "foot_left")] {
                if before < at && after >= at {
                    self.emit("tg_stomp_01", Some(foot), false);
                }
            }
            if before < 2.36 && after >= 2.36 {
                self.voice(Some("tg_lost_prey_voice"), self.state.clock + 3.8);
            }
        }
    }

    fn fill_vertices(&mut self) {
        let transform = self.transform();
        self.body_bounds = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
        for (i, vertex) in self.asset.model.vertices.iter().enumerate() {
            if let (Some(joints), Some(weights), Some(out)) = (
                self.asset.model.joints.get(i),
                self.asset.model.weights.get(i),
                self.vertices.get_mut(i),
            ) {
                *out = transform.transform_point3(Vec3::from(
                    self.animator.skin_position(*joints, *weights, vertex.pos),
                ));
                self.body_bounds.0 = self.body_bounds.0.min(*out);
                self.body_bounds.1 = self.body_bounds.1.max(*out);
            }
        }
    }

    /// Conservative posed triangle clusters shared with player and item
    /// collision. Each whole triangle belongs to one joint cluster, so no
    /// visible surface can fall between proxies. The hollow rim is solid.
    #[must_use]
    pub fn body_colliders(&self) -> Vec<DoorCollider> {
        if !self.enabled() {
            return Vec::new();
        }
        let count = self
            .asset
            .model
            .skin
            .as_ref()
            .map_or(0, |skin| skin.joints.len());
        let mut bounds = vec![(Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)); count];
        for face in self.asset.model.indices.as_chunks::<3>().0 {
            let Some(first) = face.first().copied().map(usize::from) else {
                continue;
            };
            let Some(joints) = self.asset.model.joints.get(first) else {
                continue;
            };
            let Some(weights) = self.asset.model.weights.get(first) else {
                continue;
            };
            let Some(slot) = weights
                .iter()
                .enumerate()
                .max_by(|(_, a), (_, b)| a.total_cmp(b))
                .and_then(|(i, _)| joints.get(i))
            else {
                continue;
            };
            let Some((lo, hi)) = bounds.get_mut(usize::from(*slot)) else {
                continue;
            };
            for index in face {
                if let Some(point) = self.vertices.get(usize::from(*index)) {
                    *lo = lo.min(*point);
                    *hi = hi.max(*point);
                }
            }
        }
        bounds
            .into_iter()
            .filter_map(|(lo, hi)| {
                if !lo.is_finite() || !hi.is_finite() {
                    return None;
                }
                Some(DoorCollider {
                    hinge_x: lo.x,
                    hinge_y: lo.y,
                    hinge_z: lo.z.midpoint(hi.z),
                    dir_x: 1.0,
                    dir_z: 0.0,
                    width: (hi.x - lo.x).max(0.001),
                    height: (hi.y - lo.y).max(0.001),
                    thickness: (hi.z - lo.z).max(0.001),
                })
            })
            .collect()
    }

    fn check_attack(
        &mut self,
        ctx: &WorldContext<'_>,
        leaves: &[DoorCollider],
        solids: &[WallAabb],
        scene: &dyn prop_track::Scene,
    ) {
        if self.state.attacked_episode == self.state.episode
            || self.state.intent != Intent::Chase
            || self
                .state
                .observation
                .as_ref()
                .is_some_and(|o| o.track != 1)
            || !self.fresh()
            || self.state.transit.is_some()
        {
            return;
        }
        let bounds = WallAabb::with_y(
            ctx.feet.x - ctx.body_half,
            ctx.feet.y,
            ctx.feet.z - ctx.body_half,
            ctx.body_half * 2.0,
            ctx.body_height,
            ctx.body_half * 2.0,
        );
        if !self.intersects(bounds, 0.15) {
            return;
        }
        let origin = self.head().0;
        if self
            .definition
            .passages
            .iter()
            .filter_map(|p| p.protected.as_ref())
            .any(|r| segment_box(origin, ctx.eye, r.bounds()))
            || !scene.clear(origin, ctx.eye)
            || !sight_clear_with_solids(
                ctx.index, ctx.walls, leaves, solids, origin, ctx.eye, false,
            )
            || ctx
                .floor
                .walk_height_near(
                    self.state.position.x,
                    self.state.position.z,
                    self.state.position.y,
                    0.03,
                )
                .is_none_or(|floor| (floor - self.state.position.y).abs() > 0.03)
        {
            self.log(
                "attack_rejected",
                &serde_json::json!({"reason":"protected, occluded or unsupported volume"}),
            );
            return;
        }
        self.state.attacked_episode = self.state.episode;
        self.attack_request = true;
        self.log("attack_requested",&serde_json::json!({"kind":"caught","target":"player","fresh_evidence_age":self.evidence_age(),"posed_volume":true}));
    }

    pub const fn take_attack_request(&mut self) -> bool {
        let requested = self.attack_request;
        self.attack_request = false;
        requested
    }

    /// Exact triangle/box narrow phase after deformed-body broad phase. The
    /// caller's temporal substeps include a conservative motion margin.
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Finite bounded metre-space vector operations have no checked-vector alternative; integer counters use checked or saturating arithmetic."
    )]
    fn intersects(&self, bounds: WallAabb, margin: f32) -> bool {
        let lo = Vec3::new(bounds.min_x, bounds.min_y, bounds.min_z) - Vec3::splat(margin);
        let hi = Vec3::new(bounds.max_x, bounds.max_y, bounds.max_z) + Vec3::splat(margin);
        if self.body_bounds.0.cmpgt(hi).any() || self.body_bounds.1.cmplt(lo).any() {
            return false;
        }
        self.asset
            .model
            .indices
            .as_chunks::<3>()
            .0
            .iter()
            .any(|face| {
                let points = face.map(|index| {
                    self.vertices
                        .get(usize::from(index))
                        .copied()
                        .unwrap_or(Vec3::ZERO)
                });
                triangle_box(points, lo, hi)
            })
    }

    #[must_use]
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Finite bounded metre-space vector operations have no checked-vector alternative; integer counters use checked or saturating arithmetic."
    )]
    pub fn door_intersects(&self, door: &DoorCollider) -> bool {
        if !self.enabled() {
            return false;
        }
        let origin = Vec3::new(door.hinge_x, door.hinge_y, door.hinge_z);
        let x = Vec3::new(door.dir_x, 0.0, door.dir_z);
        let z = Vec3::new(-door.dir_z, 0.0, door.dir_x);
        let center = (self.body_bounds.0 + self.body_bounds.1) * 0.5 - origin;
        let half = (self.body_bounds.1 - self.body_bounds.0) * 0.5;
        let projected = Vec3::new(center.dot(x), center.y, center.dot(z));
        // Projecting the complete posed AABB encloses every triangle on these
        // axes. Reject only a separated interval; nearby leaves keep exact SAT.
        let rounding = origin
            .abs()
            .max(self.body_bounds.0.abs())
            .max(self.body_bounds.1.abs())
            .max_element()
            .mul_add(16.0 * f32::EPSILON, 0.00001);
        let reach = Vec3::new(half.dot(x.abs()), half.y, half.dot(z.abs())) + Vec3::splat(rounding);
        let lo = Vec3::new(-MARGIN, -MARGIN, (-door.thickness).mul_add(0.5, -MARGIN));
        let hi = Vec3::new(
            door.width + MARGIN,
            door.height + MARGIN,
            door.thickness.mul_add(0.5, MARGIN),
        );
        if (projected - reach).cmpgt(hi).any() || (projected + reach).cmplt(lo).any() {
            return false;
        }
        self.asset
            .model
            .indices
            .as_chunks::<3>()
            .0
            .iter()
            .any(|face| {
                let p = face.map(|index| {
                    let v = self
                        .vertices
                        .get(usize::from(index))
                        .copied()
                        .unwrap_or(Vec3::ZERO)
                        - origin;
                    Vec3::new(v.dot(x), v.y, v.dot(z))
                });
                triangle_box(p, lo, hi)
            })
    }

    fn clear(
        &mut self,
        ctx: &WorldContext<'_>,
        leaves: &[DoorCollider],
        solids: &[WallAabb],
    ) -> bool {
        self.sample_floor(ctx);
        self.fill_vertices();
        self.pose_clear(ctx, leaves, solids)
    }

    fn pose_clear(
        &mut self,
        ctx: &WorldContext<'_>,
        leaves: &[DoorCollider],
        solids: &[WallAabb],
    ) -> bool {
        let feet = self.state.position.y;
        let ceiling_clear = self.vertices.iter().all(|point| {
            self.ceiling
                .ceiling_above_disc(point.x, point.z, 0.002, feet)
                .is_none_or(|height| point.y + 0.03 < height)
                && ctx
                    .floor
                    .lowest_room_underside(glam::Vec2::new(point.x, point.z), 0.002, feet)
                    .is_none_or(|height| point.y + 0.03 < height)
                && ctx
                    .floor
                    .height_below(point.x, point.z, feet + 0.45)
                    .is_none_or(|height| point.y + 0.005 >= height)
        });
        let accepted = ceiling_clear
            && self.contacts_fit()
            && !self
                .definition
                .passages
                .iter()
                .filter_map(|p| p.protected.as_ref())
                .any(|r| self.intersects(r.bounds(), MARGIN))
            && !ctx
                .walls
                .iter()
                .chain(solids)
                .any(|b| b.max_y > feet + 0.005 && self.intersects(*b, MARGIN))
            && !leaves.iter().any(|door| self.door_intersects(door))
            && !self.intersects(
                WallAabb::with_y(
                    ctx.feet.x - ctx.body_half,
                    ctx.feet.y,
                    ctx.feet.z - ctx.body_half,
                    ctx.body_half * 2.0,
                    ctx.body_height,
                    ctx.body_half * 2.0,
                ),
                0.01,
            );
        if !accepted {
            self.log_rejected_pose(ctx, leaves, solids);
        }
        accepted
    }

    #[must_use]
    pub fn frame(&self) -> EntityFrame {
        EntityFrame {
            rigid_transform: None,
            instance_id: self.state.id.clone(),
            transform: Some((self.state.position, self.state.yaw)),
            cue: Some(PoseCue::Articulated {
                name: self.state.clip.clone(),
                progress: self.state.clip_time / duration(&self.state.clip),
                joints: self.animator.joint_pose(),
            }),
            opacity: if self.enabled() { 1.0 } else { 0.0 },
            shadow_only: false,
            glow: None,
            balloon: None,
        }
    }

    #[must_use]
    pub fn diagnostics(&self) -> serde_json::Value {
        let feet:Vec<_>=["foot_front","foot_left","foot_right"].into_iter().filter_map(|name|self.socket(name).map(|matrix|serde_json::json!({"name":name,"position":matrix.transform_point3(Vec3::ZERO)}))).collect();
        let surface=(std::env::var_os("PLACES_BENCH").is_some() && std::env::var_os("PLACES_TALL_GUEST_DUMP_POSE").is_some()).then(||serde_json::json!({"vertices":self.vertices,"indices":self.asset.model.indices,"globals":self.animator.named_globals().into_iter().map(|(name,matrix)|(name,self.transform().mul_mat4(&matrix))).collect::<Vec<_>>()}));
        serde_json::json!({"state":self.state,"capture":self.capture.as_ref().map(capture::Capture::diagnostics),"head":self.head(),"feet":feet,"events":self.diagnostics,"direct_head_overlay":false,"distributed_aim":self.distributed_aim,"passages":self.stages,"native_surface":surface})
    }

    pub fn take_sounds(&mut self) -> Vec<Sound> {
        std::mem::take(&mut self.sounds)
    }
}

/// Separating axes for a triangle and axis-aligned solid, retaining the rim
/// and hollow lining geometry instead of treating the opening as empty space.
#[expect(
    clippy::arithmetic_side_effects,
    reason = "Fixed-size finite triangle vectors use ordinary subtraction, scaling and cross products; no integer indexing or sizing arithmetic occurs here."
)]
fn triangle_box(points: [Vec3; 3], lo: Vec3, hi: Vec3) -> bool {
    let center = (lo + hi) * 0.5;
    let half = (hi - lo) * 0.5;
    let [a, b, c] = points.map(|v| v - center);
    if a.min(b).min(c).cmpgt(half).any() || a.max(b).max(c).cmplt(-half).any() {
        return false;
    }
    let e0 = b - a;
    let e1 = c - b;
    let e2 = a - c;
    let axes = [
        Vec3::X,
        Vec3::Y,
        Vec3::Z,
        e0.cross(e1),
        e0.cross(Vec3::X),
        e0.cross(Vec3::Y),
        e0.cross(Vec3::Z),
        e1.cross(Vec3::X),
        e1.cross(Vec3::Y),
        e1.cross(Vec3::Z),
        e2.cross(Vec3::X),
        e2.cross(Vec3::Y),
        e2.cross(Vec3::Z),
    ];
    axes.into_iter().all(|axis| {
        if axis.length_squared() < 1.0e-12 {
            return true;
        }
        let values = [a.dot(axis), b.dot(axis), c.dot(axis)];
        let radius = half.dot(axis.abs());
        values.into_iter().fold(f32::INFINITY, f32::min) <= radius
            && values.into_iter().fold(f32::NEG_INFINITY, f32::max) >= -radius
    })
}

#[expect(
    clippy::arithmetic_side_effects,
    reason = "Finite slab intersections use bounded normalized segment parameters; parallel axes are handled separately."
)]
pub(crate) fn segment_box(a: Vec3, b: Vec3, bounds: WallAabb) -> bool {
    let delta = b - a;
    let low = Vec3::new(bounds.min_x, bounds.min_y, bounds.min_z);
    let high = Vec3::new(bounds.max_x, bounds.max_y, bounds.max_z);
    let mut enter = 0.0_f32;
    let mut leave = 1.0_f32;
    for (origin, direction, min, max) in [
        (a.x, delta.x, low.x, high.x),
        (a.y, delta.y, low.y, high.y),
        (a.z, delta.z, low.z, high.z),
    ] {
        if direction.abs() < 0.000_001 {
            if origin < min || origin > max {
                return false;
            }
        } else {
            let x = (min - origin) / direction;
            let y = (max - origin) / direction;
            enter = enter.max(x.min(y));
            leave = leave.min(x.max(y));
            if enter > leave {
                return false;
            }
        }
    }
    true
}

fn smooth(value: f32) -> f32 {
    let x = value.clamp(0.0, 1.0);
    x * x * (-2.0_f32).mul_add(x, 3.0)
}

/// Carry the scheduled phase while skipping missed deadlines. One real frame
/// never emits a burst of catch-up pulses after a pause or long stall.
fn cadence(previous: f32, now: f32, interval: f32) -> f32 {
    let steps = ((now - previous) / interval).floor().max(0.0) + 1.0;
    steps.mul_add(interval, previous)
}

fn posture_clip_time(clip: &str, previous: &str, elapsed: f32) -> f32 {
    // This short measured clip phase matches the pursuing head height. Smooth
    // fading retains its live stance without inserting an upright walk pose.
    let start = if clip == "door_enter" && previous == "run_pursuit" {
        0.06
    } else {
        0.0
    };
    (duration(clip) - start).mul_add((elapsed / POSTURE_SECONDS).clamp(0.0, 1.0), start)
}
