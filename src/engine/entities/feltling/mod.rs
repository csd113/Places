//! Opt-in benevolent guide. One controller owns its route, pose and finite voice.
mod capture;
mod motion;
pub mod policy;
mod pose;
mod terrain;

use crate::engine::entity::{EntityFrame, PoseCue};
use crate::engine::level::{LevelDef, LevelSurfaces, WalkableCeiling};
use crate::engine::nav::{NavScratch, Path};
use crate::engine::render::CharacterAnimator;
use glam::Vec3;
pub use policy::{Mode, Policy, Stage};
use serde::{Deserialize, Serialize};

const BLEND_SECONDS: f32 = 0.18;
const MODEL: &str = "entities/feltling/model/feltling.glb";
const CLIPS: [(&str, f32); 5] = [
    ("Walk", 1.066_666_7),
    ("Run", 0.666_666_7),
    ("Excited_Jumping", 1.8),
    ("Pointing", 2.0),
    ("Waving_Both_Arms", 2.4),
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Route {
    pub id: String,
    pub points: Vec<[f32; 3]>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Definition {
    #[serde(default)]
    pub routes: Vec<Route>,
    #[serde(default)]
    pub mode: Mode,
    #[serde(default)]
    pub look_at: Option<[f32; 3]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guide_recovery: Option<policy::Recovery>,
}

impl Definition {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        let mut ids = std::collections::BTreeSet::new();
        self.guide_recovery.is_none_or(policy::Recovery::is_valid)
            && self
                .look_at
                .is_none_or(|point| point.iter().all(|v| v.is_finite() && v.abs() <= 100_000.0))
            && self.routes.len() <= 8
            && (!matches!(self.mode, Mode::Guide | Mode::Flee) || !self.routes.is_empty())
            && self.routes.iter().all(|route| {
                !route.id.is_empty()
                    && route.id.len() <= 64
                    && !route.id.chars().any(char::is_control)
                    && ids.insert(&route.id)
                    && !route.points.is_empty()
                    && route.points.len() <= 32
                    && route
                        .points
                        .iter()
                        .flatten()
                        .all(|v| v.is_finite() && v.abs() <= 100_000.0)
            })
    }
}

/// Covers the delivered jump and arm gestures as well as ordinary locomotion.
/// Admission rechecks the corrected export against this conservative envelope.
#[must_use]
pub const fn navigation_class() -> crate::engine::package::navigation::NavClass {
    crate::engine::package::navigation::NavClass {
        radius: 0.40,
        height: 0.80,
        step_height: 0.18,
        max_slope: 2.6667,
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub id: String,
    pub position: Vec3,
    pub yaw: f32,
    pub policy: Policy,
    pub route: usize,
    pub waypoint: usize,
    pub enabled: bool,
    enable_pending: bool,
    pub clip: String,
    pub progress: f32,
    blend_from: Vec<crate::engine::entity::JointPose>,
    blend: f32,
    next_voice: f32,
    gesture: Option<(String, bool)>,
    gesture_time: f32,
    #[serde(default)]
    pose_restart: bool,
    #[serde(default)]
    terrain_offsets: [f32; 2],
}

pub struct Feltling {
    pub state: Record,
    pub(crate) asset: std::sync::Arc<crate::engine::props::LoadedPropAsset>,
    bind_pose: Vec<crate::engine::entity::JointPose>,
    settle_from: Vec<crate::engine::entity::JointPose>,
    settle_seconds: f32,
    pub(crate) attachment: Option<(Vec3, glam::Quat)>,
    pub(crate) reserved_by: Option<String>,
    pub(crate) release_pose: Option<(Vec3, f32)>,
    definition: Definition,
    animator: CharacterAnimator,
    sampler: CharacterAnimator,
    base_pose: Vec<crate::engine::entity::JointPose>,
    sole_shapes: [Vec<Vec3>; 2],
    ceiling: WalkableCeiling,
    scratch: NavScratch,
    path: Path,
    path_cursor: usize,
    next_plan: f32,
    voice: Option<&'static str>,
    status_changed: bool,
    speed: f32,
    run_reference: f32,
    animation_event: Option<String>,
    blocked_by: Option<&'static str>,
}

/// # Errors
/// Refuses missing native ownership, conflicting motion owners or unsupported
/// export contracts. An incompatible creature cannot silently become scenery.
pub fn load(level: &LevelDef) -> Result<Vec<Feltling>, String> {
    if !level.props.iter().any(|prop| prop.feltling.is_some()) {
        return Ok(Vec::new());
    }
    let catalog = crate::engine::loader::PropCatalog::load_default();
    let mut assets = crate::engine::props::PropAssets::load_default();
    let surfaces = LevelSurfaces::new(level);
    let floor = crate::engine::level::WalkableFloor::from_level(level);
    let mut result = Vec::new();
    for (prop, id) in level.props.iter().zip(level.prop_instance_ids()) {
        let Some(definition) = prop.feltling.clone() else {
            continue;
        };
        validate_placement(level, prop, &id, &definition)?;
        if catalog.get(&prop.model).model.as_deref() != Some(MODEL) {
            return Err(format!("{id}: Feltling requires its canonical model"));
        }
        let asset = assets.resolve(MODEL)?;
        let mut animator =
            CharacterAnimator::new(&asset.model).ok_or("Feltling requires a skinned rig")?;
        validate_rig(&asset.model, &mut animator, &id)?;
        let run_reference = asset
            .model
            .animations
            .iter()
            .find(|clip| clip.name == "Run")
            .and_then(|clip| clip.reference_speed_mps)
            .filter(|speed| speed.is_finite() && *speed > 0.0)
            .ok_or("Feltling run clip requires a finite positive reference speed")?;
        let _sampled = animator.sample_fraction("Waving_Both_Arms", 0.0);
        let position = Vec3::new(
            prop.x,
            floor
                .walk_height_near(
                    prop.x,
                    prop.z,
                    surfaces.floor_y_at(prop.x, prop.z).unwrap_or(0.0) + prop.y,
                    0.18,
                )
                .unwrap_or_else(|| surfaces.floor_y_at(prop.x, prop.z).unwrap_or(0.0) + prop.y),
            prop.z,
        );
        let seed = id
            .bytes()
            .fold(0_u16, |sum, value| sum.wrapping_add(u16::from(value)));
        let bind_pose = bind_pose(&asset.model)?;
        result.push(Feltling {
            asset: std::sync::Arc::clone(&asset),
            bind_pose,
            settle_from: Vec::new(),
            settle_seconds: 0.0,
            attachment: None,
            reserved_by: None,
            release_pose: None,
            state: Record {
                id,
                position,
                yaw: prop.rotation_degrees.to_radians(),
                policy: Policy {
                    recovery: definition.guide_recovery.unwrap_or_default(),
                    ..Policy::new(definition.mode)
                },
                route: 0,
                waypoint: 0,
                enabled: true,
                enable_pending: false,
                clip: "Waving_Both_Arms".into(),
                progress: 0.0,
                blend_from: animator.joint_pose(),
                blend: BLEND_SECONDS,
                next_voice: f32::from(seed % 17) * 0.19,
                gesture: None,
                gesture_time: 0.0,
                pose_restart: false,
                terrain_offsets: [0.0; 2],
            },
            definition,
            sole_shapes: terrain::sole_shapes(&asset.model, &animator)?,
            base_pose: animator.joint_pose(),
            // Keep raw clip sampling separate from the accepted blended pose;
            // an unchanged clip fraction must never reuse a blended cache.
            sampler: CharacterAnimator::new(&asset.model).ok_or("Missing Feltling clip sampler")?,
            animator,
            ceiling: WalkableCeiling::from_level(level),
            scratch: NavScratch::new(),
            path: Path::new(),
            path_cursor: 0,
            next_plan: 0.0,
            voice: None,
            status_changed: true,
            animation_event: None,
            blocked_by: None,
            speed: 0.0,
            run_reference,
        });
    }
    if result.len() > 16 {
        return Err("A level supports at most sixteen Feltlings".into());
    }
    Ok(result)
}

fn bind_pose(
    model: &crate::engine::gltf::PropModel,
) -> Result<Vec<crate::engine::entity::JointPose>, String> {
    Ok(model
        .skin
        .as_ref()
        .ok_or("Missing Feltling bind rig")?
        .nodes
        .iter()
        .map(|node| crate::engine::entity::JointPose {
            translation: Vec3::from_array(node.translation),
            rotation: glam::Quat::from_array(node.rotation),
            scale: Vec3::from_array(node.scale),
        })
        .collect())
}

fn validate_rig(
    model: &crate::engine::gltf::PropModel,
    animator: &mut CharacterAnimator,
    id: &str,
) -> Result<(), String> {
    for (name, duration) in CLIPS {
        if animator
            .clip_duration(name)
            .is_none_or(|value| (value - duration).abs() > 0.001)
        {
            return Err(format!(
                "{id}: missing or incompatible Feltling clip {name}"
            ));
        }
    }
    for name in [
        "root", "pelvis", "head", "thigh.L", "shin.L", "foot.L", "thigh.R", "shin.R", "foot.R",
    ] {
        if animator.node_global(name).is_none() {
            return Err(format!("{id}: missing Feltling node {name}"));
        }
    }
    validate_envelope(model, animator)?;
    Ok(())
}

fn validate_placement(
    level: &LevelDef,
    prop: &crate::engine::level::PropDef,
    id: &str,
    definition: &Definition,
) -> Result<(), String> {
    if !definition.is_valid()
        || prop.scale.to_bits() != 1.0_f32.to_bits()
        || prop.solid
        || prop.physics.is_some()
        || prop.float.is_some()
        || prop.span.is_some()
        || prop.tall_guest.is_some()
        || prop.animated_door.is_some()
        || prop.shadow_only
        || !prop.material_overrides.is_empty()
        || !prop.emission_overrides.is_empty()
        || prop.pitch_degrees.abs() > f32::EPSILON
        || prop.roll_degrees.abs() > f32::EPSILON
        || level.routes.iter().any(|route| route.id == id)
        || prop.components.iter().any(|component| {
            matches!(
                component,
                crate::engine::level::ComponentDef::Ai(_)
                    | crate::engine::level::ComponentDef::Animation {
                        clip: _,
                        speed: _,
                        looped: _,
                        playing: _
                    }
                    | crate::engine::level::ComponentDef::NavAgent {
                        radius: _,
                        height: _,
                        step_height: _,
                        max_slope: _,
                        speed_mps: _
                    }
            )
        })
    {
        return Err(format!(
            "{id}: Feltling requires one upright scale-one motion/pose owner and bounded authored routes"
        ));
    }
    Ok(())
}

impl Feltling {
    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.state.enabled
    }

    pub fn set_enabled(&mut self, enabled: bool) -> bool {
        let changed = if enabled {
            !self.state.enabled && !self.state.enable_pending
        } else {
            self.state.enabled || self.state.enable_pending
        };
        if !changed {
            return false;
        }
        if enabled && !self.state.enabled {
            self.state.enable_pending = true;
        }
        if !enabled {
            self.state.enabled = false;
            self.state.enable_pending = false;
        }
        self.voice = None;
        self.path.clear();
        self.next_plan = 0.0;
        changed
    }

    /// Script commands use ordinary declared state slots and existing actions.
    /// Returns false for malformed commands without changing the continuation.
    pub fn command(&mut self, key: &str, value: &str) -> bool {
        match key {
            "feltling_mode" => {
                let Some(mode) = Mode::parse(value) else {
                    return false;
                };
                if matches!(mode, Mode::Guide | Mode::Flee) && self.definition.routes.is_empty() {
                    return false;
                }
                self.state.policy.set_mode(mode);
                self.status_changed = true;
                self.state.pose_restart = true;
                self.state.gesture = None;
                true
            }
            "feltling_route" => {
                let Some(index) = self
                    .definition
                    .routes
                    .iter()
                    .position(|route| route.id == value)
                else {
                    return false;
                };
                if self.state.route != index {
                    self.state.route = index;
                    self.state.waypoint = 0;
                    self.state.policy = Policy {
                        recovery: self.definition.guide_recovery.unwrap_or_default(),
                        ..Policy::new(self.state.policy.mode)
                    };
                    self.status_changed = true;
                    self.state.pose_restart = true;
                    self.state.gesture = None;
                    self.path.clear();
                    self.next_plan = 0.0;
                }
                true
            }
            _ => false,
        }
    }

    pub fn perform(&mut self, name: &str, looped: bool) -> bool {
        if !CLIPS.iter().any(|(clip, _)| *clip == name) {
            return false;
        }
        self.state.gesture = Some((name.into(), looped));
        self.state.gesture_time = 0.0;
        self.state.pose_restart = true;
        if self.state.next_voice <= 0.0 {
            self.voice = Some(if name == "Excited_Jumping" {
                "feltling_cheer"
            } else {
                "feltling_babble"
            });
            self.state.next_voice = 3.6;
        }
        true
    }

    #[must_use]
    pub fn frame(&self) -> EntityFrame {
        EntityFrame {
            rigid_transform: self.attachment,
            instance_id: self.state.id.clone(),
            transform: Some((self.state.position, self.state.yaw)),
            cue: Some(PoseCue::Articulated {
                name: self.state.clip.clone(),
                progress: self.state.progress,
                joints: self.animator.joint_pose(),
            }),
            opacity: if self.enabled() { 1.0 } else { 0.0 },
            shadow_only: false,
            glow: None,
            balloon: None,
        }
    }

    #[must_use]
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Finite authored head socket transformed once into world metres."
    )]
    pub fn acoustic_position(&self) -> Vec3 {
        let local = self
            .animator
            .node_global("head")
            .map_or(Vec3::new(0.0, 0.5, 0.0), |matrix| {
                matrix.transform_point3(Vec3::ZERO)
            });
        let (position, rotation) = self.root_pose();
        position + rotation * local
    }

    pub const fn take_animation_event(&mut self) -> Option<String> {
        self.animation_event.take()
    }

    pub fn request_animation(&mut self, name: &str, looped: bool, toggle: bool) -> bool {
        if toggle
            && self
                .state
                .gesture
                .as_ref()
                .is_some_and(|(clip, _)| clip == name)
        {
            self.state.gesture = None;
            self.state.gesture_time = 0.0;
            true
        } else {
            self.perform(name, looped)
        }
    }

    pub const fn take_voice(&mut self) -> Option<&'static str> {
        self.voice.take()
    }
    pub fn take_status(&mut self) -> Option<&'static str> {
        std::mem::take(&mut self.status_changed).then_some(self.state.policy.stage.name())
    }

    #[must_use]
    pub fn checkpoint(&self) -> Record {
        self.state.clone()
    }

    /// # Errors
    /// Called on a private new world, before collision and render publication.
    pub fn restore(&mut self, record: &Record) -> Result<(), String> {
        let route_len = self
            .definition
            .routes
            .get(record.route)
            .map_or(0, |route| route.points.len());
        if record.id != self.state.id
            || (record.enabled && record.enable_pending)
            || !record.position.is_finite()
            || record.position.abs().max_element() > 100_000.0
            || !record.yaw.is_finite()
            || record.yaw.abs() > 100_000.0
            || !record.policy.is_valid()
            || (record.policy.mode == Mode::Guide && self.definition.routes.is_empty())
            || record.route >= self.definition.routes.len().max(1)
            || record.waypoint > route_len
            || !record.progress.is_finite()
            || !(0.0..=1.0).contains(&record.progress)
            || record.blend_from.len() != self.animator.joint_pose().len()
            || !record
                .blend_from
                .iter()
                .all(crate::engine::entity::JointPose::is_valid)
            || !record.blend.is_finite()
            || !(0.0..=BLEND_SECONDS).contains(&record.blend)
            || record
                .terrain_offsets
                .iter()
                .any(|offset| !offset.is_finite() || !(-0.18..=0.18).contains(offset))
            || !record.next_voice.is_finite()
            || !(0.0..=8.0).contains(&record.next_voice)
            || !record.gesture_time.is_finite()
            || !(0.0..=10_000_000.0).contains(&record.gesture_time)
            || !CLIPS.iter().any(|(name, _)| *name == record.clip)
            || record
                .gesture
                .as_ref()
                .is_some_and(|(name, _)| !CLIPS.iter().any(|(clip, _)| *clip == name))
        {
            return Err("Invalid Feltling checkpoint continuation".into());
        }
        self.state = record.clone();
        self.path.clear();
        self.next_plan = 0.0;
        self.voice = None;
        self.status_changed = true;
        self.animation_event = None;
        self.sample_pose();
        self.apply_terrain();
        Ok(())
    }

    #[must_use]
    pub fn diagnostics(&self) -> serde_json::Value {
        serde_json::json!({"id":self.state.id,"position":self.state.position,"yaw":self.state.yaw,
            "mode":self.state.policy.mode,"stage":self.state.policy.stage,"route":self.state.route,
            "waypoint":self.state.waypoint,"clip":self.state.clip,"progress":self.state.progress,
            "speed_mps":self.speed,"enabled":self.enabled(),"blocked_by":self.blocked_by,
            "capture_ready":self.capture_ready(),"attachment":self.attachment,"reserved_by":self.reserved_by,"settle_seconds":self.settle_seconds,
            "physical_vertices":(std::env::var_os("PLACES_PREY_DUMP").is_some()).then(||self.physical_vertices()),
            "next_path_point":self.path.waypoints.get(self.path_cursor)})
    }
}

/// Bounded actual imported-pose admission. The standing route/body envelope
/// must also contain the raised hands, nose and airborne jump, with margin.
fn validate_envelope(
    model: &crate::engine::gltf::PropModel,
    animator: &mut CharacterAnimator,
) -> Result<(), String> {
    for (clip, _) in CLIPS {
        for frame in 0_u8..=60 {
            let _sampled = animator.sample_fraction(clip, f32::from(frame) / 60.0);
            for ((vertex, joints), weights) in
                model.vertices.iter().zip(&model.joints).zip(&model.weights)
            {
                let [x, y, z] = animator.skin_position(*joints, *weights, vertex.pos);
                if !x.is_finite()
                    || !y.is_finite()
                    || !z.is_finite()
                    || x.hypot(z) > 0.39
                    || !(-0.005..=0.79).contains(&y)
                {
                    return Err(format!(
                        "Feltling clip {clip} leaves its reviewed navigation/gesture envelope"
                    ));
                }
            }
        }
    }
    Ok(())
}
