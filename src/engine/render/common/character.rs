//! Animated characters: the engine's skinned-entity path.
//!
//! A character is a placeable prop whose model carries a glTF skin
//! ([`crate::engine::gltf::PropSkin`]). It renders through the ordinary level
//! placement transform, but its vertices are re-posed on the CPU every frame
//! instead of being baked into a static batch:
//!
//! ```text
//! static prop                          character (this module)
//! -----------                          -----------------------
//! baked into PropMeshBatch at load     CPU-skinned every frame
//! bind pose only                       pose follows LocomotionSnapshot
//! one draw per model and cell          one draw per character per primitive
//! part of the lightmap bake            spatial irradiance and direct lights
//! ```
//!
//! The bind pose remains in the neutral prop batch for resource budgets and
//! static fallback. A fully claimed model is excluded from immutable lighting
//! casters and its GPU batch is suppressed while the animated character draws.
//!
//! # Pose model
//!
//! [`CharacterAnimator`] keeps a blend weight per [`LocomotionState`]. The
//! weight of the current state approaches one and every other weight
//! approaches zero exponentially with [`BLEND_TIME_CONSTANT_S`], so a state
//! change eases in and the pose never resets on a frame boundary.
//!
//! * A rig **with** clips maps each state to a clip by a case-insensitive
//!   name match (`idle`, `walk`/`run`, `jump`/`air`/`fall`, `swim`) and
//!   samples LINEAR or STEP keys. A state change crossfades the previous
//!   clip's pose over the same time constant.
//! * A rig **without** clips (the shipped Spoonerman rig) uses the
//!   procedural locomotion driver: leg pairs, a tail chain and the body chain
//!   are classified by joint name and posed by the state weights. The driver
//!   is inert for a rig it cannot classify: unknown joints stay at rest.
//!
//! # Skinning math
//!
//! The parser bakes each skinned vertex to the bind pose with
//! `meshInverseGlobal * jointGlobal(rest) * inverseBind * p`. The animator
//! never re-derives that pose: it composes the current node hierarchy and
//! evaluates the equivalent delta
//!
//! ```text
//! D_j = meshCurrentInverse * jointCurrent * jointRestInverse * meshRest
//! p_posed = sum_j weight_j * D_j * p_bind
//! ```
//!
//! so a vertex is one weighted blend of four `mat4` transforms, with no
//! inverse-bind matrix and no raw position needed at runtime.

use std::collections::HashMap;
use std::sync::Arc;

use glam::{Mat4, Quat, Vec3};

use super::Vertex;
use super::mesh::LIGHTMAP_NONE;
use super::props::{prop_instance_matrix, transform_bounds};
use super::{DynamicLight, DynamicLightSet};
use crate::engine::entities::balloon::{BalloonPose, STRING_NODE_NAMES};
use crate::engine::entity::GlowCue;
use crate::engine::gltf::{AnimationInterpolation, PropAnimation, PropNode, PropSkin};
use crate::engine::level::LevelSurfaces;
use crate::engine::lighting::LevelLighting;
use crate::engine::props::{LoadedPropAsset, PropAssets};
use crate::engine::scene::{LocomotionSnapshot, LocomotionState};
use crate::engine::spatial::Aabb;

/// Explicit per-entity pose requests live on the gameplay side
/// ([`crate::engine::entity::PoseCue`]); a character can be addressed by its placed
/// instance id so routes and interactions drive it.
pub use crate::engine::entity::{EntityFrame, PoseCue};

/// Most characters one level may spawn.
///
/// The draw path is one draw per character per primitive and the pose is
/// evaluated on the CPU, so the cap bounds per-frame skinning work rather than
/// GPU memory: each character owns only its own vertex buffer, and its mesh,
/// textures and materials are shared. The historical cap of 8 could not place
/// the required Model Zoo demonstrations (three mannequin poses, three skeleton
/// poses, two rat routes, two Spooner-Man routes, and several wall switches).
///
/// Raised to 128 from 64 in the 2026 capacity pass. Every character path
/// structure is `Vec`-backed and keyed by model path, so doubling the budget
/// adds no new allocation class: one vertex buffer and one environment bind
/// group per character, sized by the model's own bounded vertex count. The
/// per-frame skinning cost stays bounded because a character model is clamped
/// by the model budgets ([`crate::engine::level::MAX_PROP_VERTICES`] at the engine
/// ceiling, the 3 000-triangle [`ENTITY_TRIANGLE_BUDGET`](crate::engine::level::ENTITY_TRIANGLE_BUDGET)
/// for shipped art) and by [`crate::engine::level::MAX_ANIMATION_CHANNELS`]; 128
/// characters with the shipped skeleton's 94 joints is still only tens of
/// thousands of joint evaluations per frame. A level that places more keeps
/// the extras in the static prop batch in their bind pose and reports the
/// budget, exactly as before.
pub const MAX_CHARACTERS: usize = 128;

struct PlacedCharacter {
    prop_index: usize,
    asset: Arc<LoadedPropAsset>,
    animator: CharacterAnimator,
}

struct CharacterClaimPlan {
    placements: Vec<PlacedCharacter>,
    models: Vec<String>,
}

/// Models whose every placed instance follows the character path. The same
/// plan drives native spawning and immutable caster eligibility; partial or
/// overflowing model groups keep their visible static fallback geometry.
#[must_use]
pub fn claimed_character_models(
    level: &crate::engine::level::LevelDef,
    catalog: &crate::engine::loader::PropCatalog,
    assets: &mut PropAssets,
) -> Vec<String> {
    character_claim_plan(level, catalog, assets).models
}

fn character_claim_plan(
    level: &crate::engine::level::LevelDef,
    catalog: &crate::engine::loader::PropCatalog,
    assets: &mut PropAssets,
) -> CharacterClaimPlan {
    let mut placements = Vec::new();
    let mut order = Vec::new();
    let mut counts: HashMap<String, (usize, usize)> = HashMap::new();
    let mut overflow_reported = false;
    for (prop_index, prop) in level.props.iter().enumerate() {
        // Water and rigid-body motion own their live transforms. Claiming the
        // authored pose here would leave a second animated copy behind.
        if prop.float.is_some() || prop.is_physics_movable() {
            continue;
        }
        let Some(model_path) = catalog.get(&prop.model).model else {
            continue;
        };
        let count = counts.entry(model_path.clone()).or_insert((0, 0));
        if count.0 == 0 {
            order.push(model_path.clone());
        }
        count.0 = count.0.saturating_add(1);
        if !placement_is_finite(prop) {
            continue;
        }
        if placements.len() >= MAX_CHARACTERS {
            if !overflow_reported {
                overflow_reported = true;
                crate::engine::logging::warn_once(
                    "character-budget",
                    format!(
                        "[characters] the level places more than {MAX_CHARACTERS} skinned \
                         characters; the rest stay in their static bind pose"
                    ),
                );
            }
            continue;
        }
        let asset = match assets.resolve(&model_path) {
            Ok(asset) => asset,
            Err(error) => {
                assets.report_failure(&model_path, &error);
                continue;
            }
        };
        if !asset.model.is_animatable() {
            continue;
        }
        let Some(animator) = CharacterAnimator::new(&asset.model) else {
            continue;
        };
        if prop
            .components
            .iter()
            .any(|component| matches!(component, crate::engine::level::ComponentDef::Balloon(_)))
            && animator.balloon_rig.is_none()
        {
            crate::engine::logging::warn_once(
                format!("balloon-rig:{model_path}"),
                format!(
                    "[balloons] {model_path} needs an identity-parent balloon_body and twelve named string nodes; keeping its static fallback"
                ),
            );
            continue;
        }
        placements.push(PlacedCharacter {
            prop_index,
            asset,
            animator,
        });
        if let Some(updated) = counts.get_mut(&model_path) {
            updated.1 = updated.1.saturating_add(1);
        }
    }
    let models = order
        .into_iter()
        .filter(|path| {
            counts
                .get(path)
                .is_some_and(|(placed, claimed)| claimed > &0 && claimed == placed)
        })
        .collect();
    CharacterClaimPlan { placements, models }
}

/// Time constant of the exponential state-weight approach, in seconds.
///
/// A weight covers ~63% of the distance to its target in this time and is
/// within 1% after ~4.6 time constants. The same constant crossfades clips.
pub const BLEND_TIME_CONSTANT_S: f32 = 0.18;

/// Wall-clock seconds a scrub cue takes to travel the full length of its clip.
///
/// A scrub cue eases the clip time toward its authored target at a constant
/// clip-time rate, so a reversal mid-travel retargets from the current pose
/// with no snap and no restart, and the authored easing in the clip's own keys
/// is preserved. The rate is normalised per clip, so a rigid prop's toggle
/// takes this long whatever the clip's authored duration.
pub const SCRUB_TRAVERSE_SECONDS: f32 = 0.35;

/// Walking cycle length: one full gait cycle per this many metres travelled.
pub const WALK_CYCLES_PER_METRE: f32 = 0.9;

/// Swimming gait frequency, in cycles per second, independent of speed.
pub const SWIM_HZ: f32 = 1.1;

/// Idle tail-sway frequency, in cycles per second.
pub const IDLE_SWAY_HZ: f32 = 0.35;

/// Idle breathing frequency, in cycles per second.
pub const BREATH_HZ: f32 = 0.22;

/// Absolute margin added to a character's culling bounds, in metres.
pub const CHARACTER_BOUNDS_MARGIN_M: f32 = 0.05;

/// The speed the shipped `walk` clip is authored for, in metres per second.
///
/// One gait cycle covers a fixed stride at this speed, so an entity route that
/// walks at this speed plays the clip at rate 1.0 with no foot sliding;
/// `tools/props/animate_spooner_man.py --report` prints the measured stride
/// and the matching speed. A clip that declares its own reference speed in
/// `asset.extras.places_entity_clips` overrides this fallback.
pub const WALK_REFERENCE_SPEED_MPS: f32 = 0.26;

/// Fallback reference speed of a `run` clip that declares none, in m/s.
pub const RUN_REFERENCE_SPEED_MPS: f32 = 1.2;

/// Requested speed at which `Walk` prefers the `run` clip, as a multiple of
/// the walk clip's reference speed.
pub const RUN_GAIT_MULTIPLIER: f32 = 1.5;

/// Relative margin added to a character's culling bounds.
///
/// A pose can extend past the bind-pose box (a raised tail, a tucked leg), so
/// the culling box is the bind-pose box grown by this fraction and then by
/// [`CHARACTER_BOUNDS_MARGIN_M`]. The margin is conservative, never exact.
pub const CHARACTER_BOUNDS_EXPANSION: f32 = 0.15;

/// Key prefix of one attached character light: `"glow:" + instance id`.
const GLOW_KEY_PREFIX: &str = "glow:";

/// Number of locomotion states the blend vector carries.
const STATE_COUNT: usize = 5;

const IDLE: usize = 0;
const WALKING: usize = 1;
const AIRBORNE: usize = 2;
const SWIMMING: usize = 3;
const SURFACE_SWIMMING: usize = 4;

const TAU: f32 = std::f32::consts::TAU;

/// The blend-weight slot a locomotion state addresses.
const fn state_index(state: LocomotionState) -> usize {
    match state {
        LocomotionState::Idle => IDLE,
        LocomotionState::Walking => WALKING,
        LocomotionState::Airborne => AIRBORNE,
        LocomotionState::Swimming => SWIMMING,
        LocomotionState::SurfaceSwimming => SURFACE_SWIMMING,
    }
}

/// What one [`CharacterScene::update`] pass did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CharacterUpdate {
    /// Characters whose pose changed and whose vertices therefore need an
    /// upload.
    pub moved: usize,
    /// Instance ids whose one-shot pose cue completed this frame, so the entity
    /// runtime can emit `animation_complete`. Empty in the common case.
    pub finished: Vec<String>,
}

/// One node's local pose: the animator's working TRS.
#[derive(Clone, Copy, Debug, PartialEq)]
struct LocalTrs {
    translation: Vec3,
    rotation: Quat,
    scale: Vec3,
}

impl LocalTrs {
    fn from_node(node: &PropNode) -> Self {
        Self {
            translation: Vec3::from(node.translation),
            rotation: Quat::from_xyzw(
                node.rotation[0],
                node.rotation[1],
                node.rotation[2],
                node.rotation[3],
            ),
            scale: Vec3::from(node.scale),
        }
    }

    /// Interpolates between two local poses; rotations take the short arc.
    fn blend(self, other: Self, t: f32) -> Self {
        Self {
            translation: self.translation.lerp(other.translation, t),
            rotation: self.rotation.slerp(other.rotation, t),
            scale: self.scale.lerp(other.scale, t),
        }
    }

    fn matrix(self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.translation)
    }
}

/// How one node participates in the procedural locomotion driver.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum JointKind {
    Other,
    Body,
    Spine,
    Chest,
    Neck,
    Head,
    LegUpper,
    LegLower,
    LegPaw,
    Tail,
}

/// One node's procedural pose data, precomputed at spawn.
#[derive(Clone, Copy, Debug)]
struct NodePose {
    kind: JointKind,
    /// Diagonal gait pair: front-left with rear-right.
    pair: u8,
    /// True for a front leg, a front-chain node or the head; false otherwise.
    front: bool,
    /// Tail segment number (`1` at the base) or leg index, for phase offsets.
    index: u8,
    /// Whether this node carries the walking bob (a body-chain node).
    body: bool,
    /// The character's lateral (right) axis expressed in the node's parent
    /// space; a rotation about it swings the limb forward and back.
    axis_lateral: Vec3,
    /// The character's up axis expressed in the node's parent space; a
    /// rotation about it swings the tail or body sideways.
    axis_up: Vec3,
    /// Parent-space direction of the character's up axis, for body bob.
    up_in_parent: Vec3,
}

/// The retained skeleton, in the form the per-frame pose evaluation needs.
struct Rig {
    nodes: Vec<PropNode>,
    /// Parents before children, so one forward pass composes every global.
    order: Vec<u16>,
    parent: Vec<Option<u16>>,
    /// Node global transform in the rest pose.
    rest_global: Vec<Mat4>,
    /// Per joint slot: the joint node's rest global inverse.
    joint_rest_inverse: Vec<Mat4>,
    /// Per joint slot: the joint's node index.
    joint_nodes: Vec<u16>,
    /// The node carrying the skinned mesh, when the asset names one.
    mesh_node: Option<u16>,
    /// Per-node procedural classification and axes.
    pose: Vec<NodePose>,
}

/// One sampled animation channel, pre-indexed for playback.
#[derive(Clone, Debug)]
struct Channel {
    path: crate::engine::gltf::AnimationPath,
    interpolation: AnimationInterpolation,
    times: Vec<f32>,
    values: Vec<f32>,
    values_per_key: usize,
}

impl Channel {
    /// Builds a playback channel from one parsed channel.
    fn from_channel(channel: &crate::engine::gltf::PropAnimationChannel) -> Self {
        Self {
            path: channel.path,
            interpolation: channel.interpolation,
            times: channel.times.clone(),
            values: channel.values.clone(),
            values_per_key: channel.values_per_key,
        }
    }

    /// Value tuples stored per keyframe (three for CUBICSPLINE).
    const fn tuples_per_key(&self) -> usize {
        match self.interpolation {
            AnimationInterpolation::CubicSpline => 3,
            AnimationInterpolation::Linear | AnimationInterpolation::Step => 1,
        }
    }

    /// The tuple index that holds the key's value (CUBICSPLINE stores the
    /// in-tangent first).
    const fn value_tuple(&self) -> usize {
        match self.interpolation {
            AnimationInterpolation::CubicSpline => 1,
            AnimationInterpolation::Linear | AnimationInterpolation::Step => 0,
        }
    }

    /// Samples one component of the channel at `time`.
    ///
    /// STEP holds the previous key, LINEAR interpolates, and CUBICSPLINE
    /// evaluates the spec's Hermite form with the key's out-tangent and the
    /// next key's in-tangent scaled by the key interval. Before the first and
    /// after the last timestamp the channel holds that key's value.
    // bounded keyframe arithmetic
    fn sample_component(&self, time: f32, component: usize) -> Option<f32> {
        let (index, blend) = self.key_at(time)?;
        let stride = self.values_per_key;
        let tuples = self.tuples_per_key();
        let value_tuple = self.value_tuple();
        let tuple_base = |key: usize, tuple: usize| {
            key.saturating_mul(stride)
                .saturating_mul(tuples)
                .saturating_add(tuple.saturating_mul(stride))
                .saturating_add(component)
        };
        match self.interpolation {
            AnimationInterpolation::Step => {
                self.values.get(tuple_base(index, value_tuple)).copied()
            }
            AnimationInterpolation::Linear => {
                let a = self.values.get(tuple_base(index, value_tuple)).copied()?;
                if blend <= 0.0 {
                    return Some(a);
                }
                let next = index.saturating_add(1);
                let b = self.values.get(tuple_base(next, value_tuple)).copied()?;
                Some((b - a).mul_add(blend, a))
            }
            AnimationInterpolation::CubicSpline => {
                let v0 = self.values.get(tuple_base(index, 1)).copied()?;
                let m0 = self
                    .values
                    .get(tuple_base(index, 2))
                    .copied()
                    .unwrap_or(0.0);
                if blend <= 0.0 {
                    return Some(v0);
                }
                let next = index.saturating_add(1);
                let v1 = self.values.get(tuple_base(next, 1)).copied()?;
                let m1 = self.values.get(tuple_base(next, 0)).copied().unwrap_or(0.0);
                let delta = self
                    .times
                    .get(next)
                    .zip(self.times.get(index))
                    .map_or(0.0, |(end, start)| (end - start).max(0.0));
                let t = blend;
                let t2 = t * t;
                let t3 = t2 * t;
                let h00 = (2.0f32).mul_add(t3, (-3.0f32).mul_add(t2, 1.0));
                let h10 = t3 + (-2.0f32).mul_add(t2, t);
                let h01 = (-2.0f32).mul_add(t3, 3.0 * t2);
                let h11 = t3 - t2;
                Some(h11.mul_add(
                    delta * m1,
                    h01.mul_add(v1, h10.mul_add(delta * m0, h00 * v0)),
                ))
            }
        }
    }

    /// Samples one rotation key at `time`.
    ///
    /// LINEAR rotations use spherical interpolation with the shortest arc (the
    /// glTF recommendation); STEP holds and CUBICSPLINE evaluates the Hermite
    /// form component-wise, then normalizes. A degenerate result keeps the
    /// caller's current rotation.
    fn sample_rotation(&self, time: f32) -> Option<Quat> {
        let (index, blend) = self.key_at(time)?;
        let read = |key: usize, tuple: usize| -> Quat {
            let base = key
                .saturating_mul(self.values_per_key)
                .saturating_mul(self.tuples_per_key())
                .saturating_add(tuple.saturating_mul(self.values_per_key));
            Quat::from_xyzw(
                self.values.get(base).copied().unwrap_or(0.0),
                self.values
                    .get(base.saturating_add(1))
                    .copied()
                    .unwrap_or(0.0),
                self.values
                    .get(base.saturating_add(2))
                    .copied()
                    .unwrap_or(0.0),
                self.values
                    .get(base.saturating_add(3))
                    .copied()
                    .unwrap_or(1.0),
            )
        };
        let finished = match self.interpolation {
            AnimationInterpolation::Step => read(index, self.value_tuple()),
            AnimationInterpolation::Linear => {
                let a = read(index, self.value_tuple());
                if blend <= 0.0 {
                    a
                } else {
                    let b = read(index.saturating_add(1), self.value_tuple());
                    a.slerp(b, blend)
                }
            }
            AnimationInterpolation::CubicSpline => Quat::from_xyzw(
                self.sample_component(time, 0)?,
                self.sample_component(time, 1)?,
                self.sample_component(time, 2)?,
                self.sample_component(time, 3)?,
            ),
        };
        if finished.length_squared() > f32::EPSILON {
            Some(finished.normalize())
        } else {
            None
        }
    }

    /// Samples one key at `time` and writes it into the node's local pose, or
    /// into the morph weight slice for a `weights` channel.
    fn apply(
        &self,
        time: f32,
        pose: &mut LocalTrs,
        morphs: &mut [f32],
        morph_range: Option<(usize, usize)>,
    ) {
        let read = |component: usize| self.sample_component(time, component).unwrap_or(0.0);
        match self.path {
            crate::engine::gltf::AnimationPath::Translation => {
                pose.translation = Vec3::new(read(0), read(1), read(2));
            }
            crate::engine::gltf::AnimationPath::Scale => {
                pose.scale = Vec3::new(read(0), read(1), read(2));
            }
            crate::engine::gltf::AnimationPath::Rotation => {
                if let Some(quat) = self.sample_rotation(time) {
                    pose.rotation = quat;
                }
            }
            crate::engine::gltf::AnimationPath::Weights => {
                let Some((start, count)) = morph_range else {
                    return;
                };
                for index in 0..count {
                    if let Some(slot) = morphs.get_mut(start.saturating_add(index)) {
                        *slot = read(index);
                    }
                }
            }
        }
    }

    /// The bracketing key index at `time` and the interpolation factor towards
    /// the next key (always 0 for STEP or the last key).
    fn key_at(&self, time: f32) -> Option<(usize, f32)> {
        if self.times.is_empty() {
            return None;
        }
        if time <= *self.times.first()? {
            return Some((0, 0.0));
        }
        let last = self.times.len().saturating_sub(1);
        if time >= *self.times.get(last)? {
            return Some((last, 0.0));
        }
        // `partition_point` returns the first index whose time is greater than
        // `time`; the key before it brackets the sample.
        let after = self.times.partition_point(|key| *key <= time);
        let index = after.saturating_sub(1).min(last);
        let blend = match self.interpolation {
            AnimationInterpolation::Step => 0.0,
            AnimationInterpolation::Linear | AnimationInterpolation::CubicSpline => {
                let start = *self.times.get(index)?;
                let end = *self.times.get(index.saturating_add(1))?;
                if end > start {
                    ((time - start) / (end - start)).clamp(0.0, 1.0)
                } else {
                    0.0
                }
            }
        };
        Some((index, blend))
    }
}

/// The per-model morph defaults and node ranges one clip sample needs.
struct MorphRig<'a> {
    defaults: &'a [f32],
    ranges: &'a [(u16, u16, u16)],
}

/// The pose and morph buffers one clip sample writes into.
struct SampleTargets<'a> {
    pose: &'a mut [LocalTrs],
    morphs: &'a mut [f32],
}

/// One clip's channels grouped by target node.
#[derive(Clone, Debug)]
struct Clip {
    duration: f32,
    /// `channels[node]` drives that node; most nodes have no channel.
    channels: Vec<Vec<Channel>>,
}

/// Clip playback state: which clip each state plays and the crossfade.
struct ClipPlayer {
    clips: Vec<Clip>,
    /// The clip each locomotion state plays; every entry is valid.
    state_clip: [usize; STATE_COUNT],
    active: usize,
    previous: Option<usize>,
    /// Seconds since the active clip was selected.
    elapsed: f32,
}

impl ClipPlayer {
    /// Builds playback from the model's clips and the case-insensitive name
    /// heuristics.
    fn new(animations: &[PropAnimation], node_count: usize) -> Option<Self> {
        if animations.is_empty() {
            return None;
        }
        let clips: Vec<Clip> = animations
            .iter()
            .map(|animation| Clip::new(animation, node_count))
            .collect();
        let mut state_clip: [Option<usize>; STATE_COUNT] = [None; STATE_COUNT];
        for (index, animation) in animations.iter().enumerate() {
            let name = animation.name.to_ascii_lowercase();
            let slot = if name.contains("idle") {
                Some(IDLE)
            } else if name.contains("walk") || name.contains("run") {
                Some(WALKING)
            } else if name.contains("jump") || name.contains("air") || name.contains("fall") {
                Some(AIRBORNE)
            } else if name.contains("swim") {
                Some(SWIMMING)
            } else {
                None
            };
            if let Some(morph_slot) = slot {
                if state_clip.get(morph_slot).is_some_and(Option::is_none)
                    && let Some(entry) = state_clip.get_mut(morph_slot)
                {
                    *entry = Some(index);
                }
                if morph_slot == SWIMMING
                    && state_clip
                        .get(SURFACE_SWIMMING)
                        .is_some_and(Option::is_none)
                    && let Some(entry) = state_clip.get_mut(SURFACE_SWIMMING)
                {
                    *entry = Some(index);
                }
            }
        }
        // A state with no named clip falls back to the idle clip, then to the
        // first clip, so a partial clip set still plays something.
        let fallback = state_clip.get(IDLE).copied().flatten().unwrap_or(0);
        let mut resolved = [0usize; STATE_COUNT];
        for (slot, entry) in resolved.iter_mut().enumerate() {
            *entry = state_clip.get(slot).copied().flatten().unwrap_or(fallback);
        }
        Some(Self {
            clips,
            state_clip: resolved,
            active: resolved.get(IDLE).copied().unwrap_or(0),
            previous: None,
            elapsed: BLEND_TIME_CONSTANT_S,
        })
    }

    /// Advances the crossfade and returns the blend factor from the previous
    /// clip's pose (0) to the active clip's pose (1).
    fn advance(&mut self, delta_seconds: f32, state: usize) -> f32 {
        let wanted = self.state_clip.get(state).copied().unwrap_or(self.active);
        if wanted != self.active {
            self.previous = Some(self.active);
            self.active = wanted;
            self.elapsed = 0.0;
        }
        if self.previous.is_some() {
            self.elapsed = (self.elapsed + delta_seconds).max(0.0);
        }
        let fade = 1.0 - (-self.elapsed / BLEND_TIME_CONSTANT_S).exp();
        if fade >= 0.999 {
            self.previous = None;
        }
        fade
    }
}

impl Clip {
    fn new(animation: &PropAnimation, node_count: usize) -> Self {
        let mut channels: Vec<Vec<Channel>> = (0..node_count).map(|_| Vec::new()).collect();
        for channel in &animation.channels {
            let Some(slot) = channels.get_mut(usize::from(channel.node)) else {
                continue;
            };
            slot.push(Channel::from_channel(channel));
        }
        Self {
            duration: animation.duration,
            channels,
        }
    }

    /// Samples the clip at `time` into per-node local poses that start from
    /// the rest pose, plus the clip's morph weights. `looping` wraps the time;
    /// a one-shot clamps it to the clip's end so the last pose is held.
    fn sample(
        &self,
        rig: &Rig,
        morph: &MorphRig<'_>,
        time: f32,
        looping: bool,
        out: &mut SampleTargets<'_>,
    ) {
        for (node, pose) in rig.nodes.iter().enumerate() {
            let Some(slot) = out.pose.get_mut(node) else {
                continue;
            };
            *slot = LocalTrs::from_node(pose);
        }
        for (slot, default) in out.morphs.iter_mut().zip(morph.defaults.iter()) {
            *slot = *default;
        }
        let sample_time = if time.is_finite() { time } else { 0.0 };
        let local_time = if self.duration > 0.0 {
            if looping {
                sample_time.rem_euclid(self.duration)
            } else {
                sample_time.clamp(0.0, self.duration)
            }
        } else {
            0.0
        };
        for (node, node_channels) in self.channels.iter().enumerate() {
            let Some(slot) = out.pose.get_mut(node) else {
                continue;
            };
            let morph_range = u16::try_from(node).ok().and_then(|node_index| {
                morph
                    .ranges
                    .iter()
                    .find(|(candidate, _, count)| *candidate == node_index && *count > 0)
                    .map(|(_, start, count)| (usize::from(*start), usize::from(*count)))
            });
            for channel in node_channels {
                channel.apply(local_time, slot, out.morphs, morph_range);
            }
        }
    }
}

impl Rig {
    /// Builds the retained rig from one model's skin.
    fn new(skin: &PropSkin) -> Option<Self> {
        Self::assemble(
            skin.nodes.clone(),
            skin.joints.clone(),
            skin.root,
            skin.mesh_node,
        )
    }

    /// Builds a rig for a rigid animated model (clips, no skin): every node is
    /// its own one-joint chain, so a vertex bound to node `i` follows that
    /// node's animated global transform.
    fn new_rigid(nodes: &[PropNode]) -> Option<Self> {
        if nodes.is_empty() {
            return None;
        }
        let joints: Vec<u16> = (0..nodes.len())
            .filter_map(|index| u16::try_from(index).ok())
            .collect();
        if joints.len() != nodes.len() {
            return None;
        }
        let root = nodes
            .iter()
            .position(|node| node.parent.is_none())
            .and_then(|index| u16::try_from(index).ok());
        Self::assemble(nodes.to_vec(), joints, root, None)
    }

    /// Shared rig construction for a skin or a rigid node hierarchy.
    fn assemble(
        nodes: Vec<PropNode>,
        joints: Vec<u16>,
        root: Option<u16>,
        mesh_node: Option<u16>,
    ) -> Option<Self> {
        let node_count = nodes.len();
        if node_count == 0 {
            return None;
        }
        let parent: Vec<Option<u16>> = nodes.iter().map(|node| node.parent).collect();
        let order = topological_order(&parent)?;
        let mut rest_global: Vec<Mat4> = vec![Mat4::IDENTITY; node_count];
        for node in &order {
            let index = usize::from(*node);
            let local = nodes.get(index).map(PropNode::local_transform)?;
            let global = parent
                .get(index)
                .copied()
                .flatten()
                .map_or(local, |parent_index| {
                    rest_global
                        .get(usize::from(parent_index))
                        .copied()
                        .unwrap_or(Mat4::IDENTITY)
                        .mul_mat4(&local)
                });
            if let Some(slot) = rest_global.get_mut(index) {
                *slot = global;
            }
        }
        let mut joint_rest_inverse: Vec<Mat4> = Vec::with_capacity(joints.len());
        for joint in &joints {
            let global = rest_global.get(usize::from(*joint)).copied()?;
            let determinant = global.determinant();
            let inverse = if determinant.is_finite() && determinant.abs() > 1.0e-12 {
                global.inverse()
            } else {
                Mat4::IDENTITY
            };
            joint_rest_inverse.push(inverse);
        }
        let mut pose: Vec<NodePose> = Vec::with_capacity(node_count);
        for (index, node) in nodes.iter().enumerate() {
            let parent_rotation =
                parent
                    .get(index)
                    .copied()
                    .flatten()
                    .map_or(Quat::IDENTITY, |parent_index| {
                        rest_global
                            .get(usize::from(parent_index))
                            .copied()
                            .map_or(Quat::IDENTITY, rest_rotation)
                    });
            let kind = classify_joint(&node.name);
            let mut node_pose = NodePose {
                kind,
                pair: 0,
                front: matches!(
                    kind,
                    JointKind::LegUpper | JointKind::LegLower | JointKind::LegPaw
                ),
                index: 0,
                body: kind == JointKind::Body,
                axis_lateral: parent_rotation.inverse().mul_vec3(Vec3::X),
                axis_up: parent_rotation.inverse().mul_vec3(Vec3::Y),
                up_in_parent: parent_rotation.inverse().mul_vec3(Vec3::Y),
            };
            if let Some((pair, front, leg_index)) = leg_facts(&node.name) {
                node_pose.pair = pair;
                node_pose.front = front;
                node_pose.index = leg_index;
            } else if kind == JointKind::Tail {
                node_pose.index = tail_index(&node.name);
            }
            pose.push(node_pose);
        }
        // A rig with no node named like a body still gets a bob carrier: the
        // rig's root, whose parent is by definition the outermost node, so the
        // character's up axis is its parent-space Y.
        if !pose.iter().any(|node| node.body)
            && let Some(leg_root) = root
            && let Some(slot) = pose.get_mut(usize::from(leg_root))
        {
            slot.body = true;
            slot.up_in_parent = Vec3::Y;
        }
        Some(Self {
            nodes,
            order,
            parent,
            rest_global,
            joint_rest_inverse,
            joint_nodes: joints,
            mesh_node,
            pose,
        })
    }
}

/// Rotation part of a global matrix, for expressing world axes in parent
/// space.
fn rest_rotation(global: Mat4) -> Quat {
    let (_, rotation, _) = global.to_scale_rotation_translation();
    if rotation.is_finite() {
        rotation.normalize()
    } else {
        Quat::IDENTITY
    }
}

/// Parents-before-children order for the retained hierarchy.
fn topological_order(parent: &[Option<u16>]) -> Option<Vec<u16>> {
    let mut order: Vec<u16> = Vec::with_capacity(parent.len());
    let mut emitted = vec![false; parent.len()];
    // A node is ready when it has no parent or its parent is emitted.
    loop {
        let mut progressed = false;
        for (index, parent_index) in parent.iter().enumerate() {
            if emitted.get(index).copied().unwrap_or(true) {
                continue;
            }
            let ready = parent_index.as_ref().is_none_or(|parent_node| {
                emitted
                    .get(usize::from(*parent_node))
                    .copied()
                    .unwrap_or(false)
            });
            if ready {
                order.push(u16::try_from(index).ok()?);
                if let Some(slot) = emitted.get_mut(index) {
                    *slot = true;
                }
                progressed = true;
            }
        }
        if order.len() == parent.len() {
            return Some(order);
        }
        if !progressed {
            // A cycle: the parser rejects these, so this is defensive.
            return None;
        }
    }
}

/// Classifies a joint by its name, case-insensitively.
fn classify_joint(name: &str) -> JointKind {
    let lower = name.to_ascii_lowercase();
    if lower.starts_with("leg_") {
        if lower.ends_with("_upper") {
            return JointKind::LegUpper;
        }
        if lower.ends_with("_lower") {
            return JointKind::LegLower;
        }
        if lower.ends_with("_paw") {
            return JointKind::LegPaw;
        }
    }
    if lower.starts_with("tail_") {
        return JointKind::Tail;
    }
    match lower.as_str() {
        "root" | "pelvis" | "hips" | "hip" | "body" => JointKind::Body,
        "spine" => JointKind::Spine,
        "chest" => JointKind::Chest,
        "neck" => JointKind::Neck,
        "head" => JointKind::Head,
        _ => JointKind::Other,
    }
}

/// Diagonal pair, front/rear flag and joint index of a leg node, or `None`.
fn leg_facts(name: &str) -> Option<(u8, bool, u8)> {
    let lower = name.to_ascii_lowercase();
    let side = ["_upper", "_lower", "_paw"].iter().find_map(|suffix| {
        lower
            .strip_prefix("leg_")
            .and_then(|rest| rest.strip_suffix(suffix))
    })?;
    let mut characters = side.chars();
    let front = matches!(characters.next(), Some('f'));
    let left = matches!(characters.next(), Some('l'));
    let pair = u8::from(front != left);
    let index = u8::from(!front);
    Some((pair, front, index))
}

/// Numeric tail segment, `1` when the name has no number.
fn tail_index(name: &str) -> u8 {
    let lower = name.to_ascii_lowercase();
    lower
        .strip_prefix("tail_")
        .and_then(|rest| rest.parse::<u8>().ok())
        .unwrap_or(1)
}

/// One character: a placement, a shared model, its animator and its baked
/// per-vertex albedo.
pub struct Character {
    asset: Arc<LoadedPropAsset>,
    transform: Mat4,
    animator: CharacterAnimator,
    /// Model-space material albedo, parallel
    /// to the model's `vertices`.
    albedo: Vec<[f32; 4]>,
    entity_lighting: super::light_transport::EntityLighting,
    center_light_key: Option<super::dynamic::EntityCenterLightKey>,
    lighting_refresh_key: Option<super::dynamic::EntityLightingRefreshKey>,
    spatial_lighting: Option<Box<super::light_transport::EntitySpatialLighting>>,
    spatial_key: Option<super::light_transport::EntitySpatialKey>,
    lighting_local_centre: Vec3,
    lighting_local_bounds: Aabb,
    /// Applied latex tint; avoids repeating immutable albedo work each frame.
    balloon_color: Option<[f32; 3]>,
    world_bounds: Aabb,
    /// Placed-instance id, so a route or an interaction can address this
    /// character. `None` for a placement outside the prop id namespace.
    instance_id: Option<String>,
    /// Uniform placement scale, retained so a live pose change composes the
    /// same transform the static prop path did.
    scale: f32,
    /// Per-instance opacity multiplier applied by the environment uniform,
    /// clamped to `0..=1`; `1.0` for a character with no fade component.
    opacity: f32,
    /// Optional simulation-driven playback for runtime visual representations.
    /// Ordinary runtime actors still hold their pose without an entity frame.
    runtime_locomotion: Option<LocomotionSnapshot>,
    /// A player visual can advance with unscaled time for the next pose pass.
    runtime_frame_delta: Option<f32>,
    /// Attached illumination owned by a runtime visual, independent of frames.
    runtime_glow: Option<GlowCue>,
    /// Suspended runtime props retain resources without drawing or lighting.
    runtime_visible: bool,
}

impl Character {
    /// The shared decoded model.
    #[must_use]
    pub const fn asset(&self) -> &Arc<LoadedPropAsset> {
        &self.asset
    }

    /// The character's current opacity, in `0..=1`.
    ///
    /// The environment uniform multiplies the fragment alpha and the emissive
    /// term by this value, so a faded ghost neither covers what is behind it
    /// nor blooms.
    #[must_use]
    pub const fn opacity(&self) -> f32 {
        if self.runtime_visible {
            self.opacity
        } else {
            0.0
        }
    }

    /// Explicit visibility, independent of a model's material alpha behavior.
    #[must_use]
    pub const fn is_visible(&self) -> bool {
        self.runtime_visible
    }

    /// Sets the character's opacity, clamped to `0..=1`.
    ///
    /// A non-finite value is treated as fully opaque: a malformed fade must not
    /// make the mesh disappear or poison the uniform.
    pub const fn set_opacity(&mut self, opacity: f32) {
        self.opacity = if opacity.is_finite() {
            opacity.clamp(0.0, 1.0)
        } else {
            1.0
        };
    }

    /// The placement transform (translate x yaw x uniform scale), the same
    /// transform the static prop path uses.
    #[must_use]
    pub const fn transform(&self) -> Mat4 {
        self.transform
    }

    /// Transformed mesh bind bounds for the lightweight projected shadow.
    /// Culling padding is excluded; animation silhouette remains approximate.
    #[must_use]
    pub fn contact_bounds(&self) -> Aabb {
        if !self.runtime_visible {
            return Aabb::EMPTY;
        }
        if self.balloon_color.is_some() {
            return transform_bounds(&self.lighting_local_bounds, &self.transform);
        }
        self.asset.model.bounds().map_or(Aabb::EMPTY, |(min, max)| {
            transform_bounds(&Aabb { min, max }, &self.transform)
        })
    }

    /// The placed-instance id this character is keyed by, if any.
    #[must_use]
    pub fn instance_id(&self) -> Option<&str> {
        self.instance_id.as_deref()
    }

    /// The pose evaluator.
    #[must_use]
    pub const fn animator(&self) -> &CharacterAnimator {
        &self.animator
    }

    /// Moves the character to a live base position and yaw (radians).
    ///
    /// The yaw is radians with `0` facing world `+Z`, the same unit
    /// [`EntityFrame::transform`] carries; a caller that holds degrees
    /// (`spawn_runtime_character`, `SetDynamicTransform`) converts at its own
    /// boundary before calling this.
    ///
    /// Rebuilds the same placement matrix `prop_instance_matrix` produced and
    /// recomputes the conservative world bounds, so culling follows the
    /// character. The mesh vertices are model-space and are unaffected.
    pub fn set_pose(&mut self, position: Vec3, yaw: f32) {
        self.set_oriented_pose(position, Quat::from_rotation_y(yaw));
    }

    /// Composes the world pose independently of the local animated skeleton.
    fn set_oriented_pose(&mut self, position: Vec3, rotation: Quat) {
        self.transform =
            Mat4::from_scale_rotation_translation(Vec3::splat(self.scale), rotation, position);
        self.world_bounds = character_bounds(&self.asset, &self.transform);
    }

    /// Applies one bounded balloon pose; reusable rig/albedo buffers allocate nothing.
    fn set_balloon_pose(&mut self, pose: BalloonPose) -> bool {
        let Some(rig) = self.animator.balloon_rig else {
            return false;
        };
        let bounds = pose.body_bounds(rig.body_bounds);
        if self.lighting_local_bounds != bounds {
            self.lighting_local_bounds = bounds;
            self.spatial_key = None;
        }
        self.lighting_local_centre = pose.body_centre(rig.body_bounds);
        let envelope = bounds
            .expanded([
                pose.anchor.x - 0.04,
                pose.anchor.y - 0.04,
                pose.anchor.z - 0.04,
            ])
            .expanded([
                pose.anchor.x + 0.04,
                pose.anchor.y + 0.04,
                pose.anchor.z + 0.04,
            ]);
        self.world_bounds = transform_bounds(&envelope, &self.transform);
        let tint_changed = self.balloon_color != Some(pose.color);
        if tint_changed {
            for (index, (albedo, vertex)) in self
                .albedo
                .iter_mut()
                .zip(&self.asset.model.vertices)
                .enumerate()
            {
                if self
                    .asset
                    .model
                    .joints
                    .get(index)
                    .and_then(|joints| joints.first())
                    .is_some_and(|joint| {
                        self.animator
                            .rig
                            .joint_nodes
                            .get(usize::from(*joint))
                            .copied()
                            .map(usize::from)
                            == Some(rig.body)
                    })
                {
                    for ((channel, base), tint) in albedo
                        .iter_mut()
                        .take(3)
                        .zip(vertex.color.iter())
                        .zip(pose.color)
                    {
                        *channel = *base * tint;
                    }
                }
            }
            self.balloon_color = Some(pose.color);
        }
        let moved = self.animator.pose_balloon(pose);
        if tint_changed && !moved {
            self.animator.revision = self.animator.revision.wrapping_add(1);
        }
        moved || tint_changed
    }

    /// Model-space material albedo per vertex, independent of lighting.
    #[must_use]
    pub fn albedo(&self) -> &[[f32; 4]] {
        &self.albedo
    }

    /// Stable world-space lighting anchor: transformed bind-pose bounds centre.
    #[must_use]
    pub fn lighting_sample_position(&self) -> [f32; 3] {
        self.transform
            .transform_point3(self.lighting_local_centre)
            .to_array()
    }

    /// Current environmental light, refreshed independently of animation.
    #[must_use]
    pub const fn entity_lighting(&self) -> super::light_transport::EntityLighting {
        self.entity_lighting
    }

    /// Stable bind-pose spatial samples, independent of animation phase.
    #[must_use]
    pub fn spatial_lighting(&self) -> Option<&super::light_transport::EntitySpatialLighting> {
        self.spatial_lighting.as_deref()
    }

    /// Padded bind-pose envelope for scene queries and grounding.
    /// The GPU renderer measures the uploaded pose separately for frustum culling.
    #[must_use]
    pub const fn world_bounds(&self) -> Aabb {
        self.world_bounds
    }
}

/// Every character a level spawns, plus the runtime-spawned actors and the
/// model paths the static prop draw path must suppress.
#[derive(Default)]
pub struct CharacterScene {
    characters: Vec<Character>,
    /// Runtime-spawned actors, keyed by instance id. They live in their own
    /// list so a placed character keeps its stable index whatever runtime
    /// actors come and go: the GPU side keys its entries by
    /// `Placed(index)`/`Runtime(instance id)`, never by a shared list slot.
    runtime: Vec<Character>,
    /// Bumped on every successful runtime spawn and every despawn that removed
    /// something, and zero until the first runtime spawn. The renderer compares
    /// it against the generation its GPU side was built from, so a change is
    /// visible on the frame it happens.
    runtime_generation: u64,
    claimed_models: Vec<String>,
    /// Attached lights for this frame's glow cues, keyed by instance id.
    /// Rebuilt by [`Self::update`] after the characters advance, so every
    /// socket resolves against the pose that will draw.
    dynamic_lights: DynamicLightSet,
}

impl CharacterScene {
    /// An empty scene.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of live characters: every placed character plus every
    /// runtime-spawned actor.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.characters.len().saturating_add(self.runtime.len())
    }

    /// True when no character is live.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.characters.is_empty() && self.runtime.is_empty()
    }

    /// The live characters, in placement order.
    ///
    /// Runtime-spawned actors are not part of this list; they are addressed by
    /// their instance id ([`Self::runtime_character`]), so a placed character's
    /// index is stable whatever runtime actors come and go.
    #[must_use]
    pub fn characters(&self) -> &[Character] {
        &self.characters
    }

    /// The number of runtime-spawned characters currently live.
    #[must_use]
    pub const fn runtime_len(&self) -> usize {
        self.runtime.len()
    }

    /// Monotonic counter bumped on every runtime spawn/despawn.
    #[must_use]
    pub const fn runtime_generation(&self) -> u64 {
        self.runtime_generation
    }

    /// The live runtime character with this instance id, if any.
    #[must_use]
    pub fn runtime_character(&self, instance_id: &str) -> Option<&Character> {
        self.runtime
            .iter()
            .find(|character| character.instance_id.as_deref() == Some(instance_id))
    }

    /// The live runtime character with this instance id, if any (mutable).
    pub fn runtime_character_mut(&mut self, instance_id: &str) -> Option<&mut Character> {
        self.runtime
            .iter_mut()
            .find(|character| character.instance_id.as_deref() == Some(instance_id))
    }

    /// The live runtime-spawned characters, in spawn order.
    ///
    /// This is the enumeration the GPU side walks when it builds its keyed
    /// entries; placed characters keep [`Self::characters`] and their stable
    /// indices.
    #[must_use]
    pub(crate) fn runtime_characters(&self) -> &[Character] {
        &self.runtime
    }

    /// Every placement of this model path is a character, so the static prop
    /// draw path must not draw its bind-pose batch on top of the animation.
    ///
    /// A model with even one unclaimed placement (the character cap was
    /// reached) stays in the static path; suppressing its batch would also
    /// remove that placement's geometry.
    #[must_use]
    pub fn claimed_models(&self) -> &[String] {
        &self.claimed_models
    }

    /// Keeps live pose/cue playback across a graphics-only rebuild while
    /// retaining the newly prepared environmental lighting. Identity is per
    /// placement; a changed model keeps its new animator instead of applying
    /// incompatible rig state from the previous asset.
    pub fn inherit_playback_from(&mut self, previous: &mut Self) {
        // Runtime actors have no authored placement to re-create. Retain them
        // across a graphics rebuild and re-sample the newly installed field.
        self.runtime = std::mem::take(&mut previous.runtime);
        for character in &mut self.runtime {
            character.center_light_key = None;
            character.lighting_refresh_key = None;
        }
        self.runtime_generation = previous.runtime_generation;
        for character in &mut self.characters {
            let Some(id) = character.instance_id.as_deref() else {
                continue;
            };
            let Some(old) = previous.characters.iter_mut().find(|old| {
                old.instance_id.as_deref() == Some(id) && Arc::ptr_eq(&old.asset, &character.asset)
            }) else {
                continue;
            };
            std::mem::swap(&mut character.animator, &mut old.animator);
            character.transform = old.transform;
            character.world_bounds = old.world_bounds;
            character.opacity = old.opacity;
        }
    }

    /// Claims every placed prop whose resolved model carries a skin.
    ///
    /// Placement uses the same transform as the static path, so a character
    /// stands exactly where its prop entry says. Lighting uses the transformed
    /// bind-pose bounds centre and refreshes after live transforms advance.
    /// A model whose asset fails to resolve is left to the
    /// static path's placeholder handling.
    #[must_use]
    pub fn spawn_characters(
        level: &crate::engine::level::LevelDef,
        catalog: &crate::engine::loader::PropCatalog,
        assets: &mut PropAssets,
        lighting: &LevelLighting,
    ) -> Self {
        Self::spawn_characters_with_field(level, catalog, assets, lighting, None)
    }

    /// [`Self::spawn_characters`] lighting each character from the prepared
    /// irradiance field when one is available.
    pub fn spawn_characters_with_field(
        level: &crate::engine::level::LevelDef,
        catalog: &crate::engine::loader::PropCatalog,
        assets: &mut PropAssets,
        lighting: &LevelLighting,
        irradiance: Option<&crate::engine::lighting::probes::ProbeField>,
    ) -> Self {
        let surfaces = LevelSurfaces::new(level);
        let instance_ids = level.prop_instance_ids();
        let mut characters: Vec<Character> = Vec::new();
        let plan = character_claim_plan(level, catalog, assets);
        for placement in plan.placements {
            let PlacedCharacter {
                prop_index,
                asset,
                animator,
            } = placement;
            let Some(prop) = level.props.get(prop_index) else {
                continue;
            };
            let base_y = surfaces.floor_y_at(prop.x, prop.z).unwrap_or(0.0);
            let transform = prop_instance_matrix(prop, base_y);
            let albedo = asset.model.vertices.iter().map(|v| v.color).collect();
            let entity_lighting = sample_character_light(&asset, &transform, lighting, irradiance);
            let lighting_local_centre = model_lighting_centre(&asset);
            let world_bounds = character_bounds(&asset, &transform);
            let instance_id = instance_ids
                .get(prop_index)
                .map(|id| id.trim())
                .filter(|id| !id.is_empty())
                .map(str::to_string);
            let lighting_local_bounds = asset_bounds(&asset);
            characters.push(Character {
                asset,
                transform,
                animator,
                albedo,
                entity_lighting,
                center_light_key: None,
                lighting_refresh_key: None,
                spatial_lighting: None,
                spatial_key: None,
                lighting_local_centre,
                lighting_local_bounds,
                balloon_color: None,
                world_bounds,
                instance_id,
                scale: prop.scale,
                opacity: 1.0,
                runtime_locomotion: None,
                runtime_frame_delta: None,
                runtime_glow: None,
                runtime_visible: true,
            });
        }
        let claimed_models = plan.models;
        Self {
            characters,
            runtime: Vec::new(),
            runtime_generation: 0,
            claimed_models,
            dynamic_lights: DynamicLightSet::new(),
        }
    }

    /// Spawns one runtime character from a catalogue model id.
    ///
    /// Resolves the model through `catalog`/`assets` exactly like
    /// [`Self::spawn_characters_with_field`], accepting either a catalogue
    /// registry id or a raw model path, and requires an animatable model
    /// (`PropsModel::is_animatable` plus [`CharacterAnimator::new`]). The
    /// initial environment is sampled from `lighting`/`irradiance` at its
    /// bounds centre, exactly like a placed character's. Refuses when
    /// the model cannot resolve, is not animatable, the position or scale are
    /// not finite/positive, or the placed + runtime character budget
    /// ([`MAX_CHARACTERS`]) is full. A live runtime character with the same
    /// instance id is replaced in place, which is a change like any other.
    ///
    /// # Errors
    ///
    /// Returns the reason the spawn was refused. A refused spawn leaves the
    /// scene untouched.
    #[expect(
        clippy::too_many_arguments,
        reason = "one frozen spawn seam: model, placement and lighting inputs"
    )] // one frozen spawn seam: model, placement and lighting inputs
    pub fn spawn_runtime_character(
        &mut self,
        level: &crate::engine::level::LevelDef,
        catalog: &crate::engine::loader::PropCatalog,
        assets: &mut PropAssets,
        lighting: &LevelLighting,
        irradiance: Option<&crate::engine::lighting::probes::ProbeField>,
        instance_id: &str,
        model: &str,
        position: Vec3,
        yaw_degrees: f32,
        scale: f32,
    ) -> Result<(), String> {
        let trimmed_id = instance_id.trim();
        if trimmed_id.is_empty() {
            return Err("a runtime character needs a non-empty instance id".to_string());
        }
        if !position.is_finite() || !yaw_degrees.is_finite() || !scale.is_finite() || scale <= 0.0 {
            return Err(format!(
                "the runtime character `{trimmed_id}` has a non-finite position or yaw, \
                 or a non-positive scale"
            ));
        }
        let replaced = self
            .runtime
            .iter()
            .position(|character| character.instance_id.as_deref() == Some(trimmed_id));
        if replaced.is_none() && self.len() >= MAX_CHARACTERS {
            return Err(format!(
                "the character budget ({MAX_CHARACTERS}) is full; the runtime character \
                 `{trimmed_id}` was not spawned"
            ));
        }
        // The catalogue maps a registry id to its model path; a string the
        // catalogue does not know is tried as a model path directly, exactly
        // like the dynamic runtime spawn and the placed character path.
        let entry = catalog.get(model);
        let path = entry
            .model
            .filter(|path| !path.is_empty())
            .unwrap_or_else(|| model.to_string());
        let asset = match assets.resolve(&path) {
            Ok(asset) => asset,
            Err(error) => {
                assets.report_failure(&path, &error);
                return Err(format!(
                    "the runtime model `{path}` did not resolve: {error}"
                ));
            }
        };
        if !asset.model.is_animatable() {
            return Err(format!(
                "the runtime model `{path}` carries no skin or animation; it cannot be posed"
            ));
        }
        let Some(animator) = CharacterAnimator::new(&asset.model) else {
            return Err(format!("the runtime model `{path}` has no poseable rig"));
        };
        let base_y = LevelSurfaces::new(level)
            .floor_y_at(position.x, position.z)
            .unwrap_or(0.0);
        let transform = runtime_instance_matrix(position, base_y, yaw_degrees, scale);
        let albedo = asset.model.vertices.iter().map(|v| v.color).collect();
        let entity_lighting = sample_character_light(&asset, &transform, lighting, irradiance);
        let lighting_local_centre = model_lighting_centre(&asset);
        let world_bounds = character_bounds(&asset, &transform);
        let lighting_local_bounds = asset_bounds(&asset);
        let character = Character {
            asset,
            transform,
            animator,
            albedo,
            entity_lighting,
            center_light_key: None,
            lighting_refresh_key: None,
            spatial_lighting: None,
            spatial_key: None,
            lighting_local_centre,
            lighting_local_bounds,
            balloon_color: None,
            world_bounds,
            instance_id: Some(trimmed_id.to_string()),
            scale,
            opacity: 1.0,
            runtime_locomotion: None,
            runtime_frame_delta: None,
            runtime_glow: None,
            runtime_visible: true,
        };
        match replaced {
            Some(index) => {
                if let Some(slot) = self.runtime.get_mut(index) {
                    *slot = character;
                }
            }
            None => self.runtime.push(character),
        }
        self.runtime_generation = self.runtime_generation.wrapping_add(1);
        Ok(())
    }

    /// Removes a runtime character; true when one was removed.
    pub fn despawn_runtime_character(&mut self, instance_id: &str) -> bool {
        let Some(index) = self
            .runtime
            .iter()
            .position(|character| character.instance_id.as_deref() == Some(instance_id))
        else {
            return false;
        };
        drop(self.runtime.remove(index));
        let _removed_light = self
            .dynamic_lights
            .remove(&format!("{GLOW_KEY_PREFIX}{instance_id}"));
        self.runtime_generation = self.runtime_generation.wrapping_add(1);
        true
    }

    /// Sets a live runtime character's placement (position + yaw in degrees);
    /// true when one exists. The animator keeps playing.
    ///
    /// A non-finite position or yaw is refused without moving the character, so
    /// a malformed frame cannot poison its culling bounds.
    pub fn set_runtime_character_transform(
        &mut self,
        instance_id: &str,
        position: Vec3,
        yaw_degrees: f32,
    ) -> bool {
        if !position.is_finite() || !yaw_degrees.is_finite() {
            return false;
        }
        let Some(character) = self.runtime_character_mut(instance_id) else {
            return false;
        };
        character.set_pose(position, yaw_degrees.to_radians());
        true
    }

    /// Sets a runtime actor's complete world pose, retaining its local rig.
    /// Rotations are normalized after rejecting zero/non-finite inputs.
    pub fn set_runtime_character_pose(
        &mut self,
        instance_id: &str,
        position: Vec3,
        orientation: Quat,
    ) -> bool {
        if !position.is_finite()
            || !orientation.is_finite()
            || orientation.length_squared() <= f32::EPSILON
            || !orientation.length_squared().is_finite()
        {
            return false;
        }
        let Some(character) = self.runtime_character_mut(instance_id) else {
            return false;
        };
        character.set_oriented_pose(position, orientation.normalize());
        true
    }

    /// Supplies the retained movement state for a runtime actor's own rig.
    pub fn set_runtime_character_locomotion(
        &mut self,
        instance_id: &str,
        locomotion: LocomotionSnapshot,
    ) -> bool {
        let Some(character) = self.runtime_character_mut(instance_id) else {
            return false;
        };
        character.runtime_locomotion = Some(locomotion);
        true
    }

    /// Overrides time for the next animation pass, then returns to world time.
    pub fn set_runtime_character_frame_delta(
        &mut self,
        instance_id: &str,
        delta_seconds: f32,
    ) -> bool {
        if !delta_seconds.is_finite() || delta_seconds < 0.0 {
            return false;
        }
        let Some(character) = self.runtime_character_mut(instance_id) else {
            return false;
        };
        character.runtime_frame_delta = Some(delta_seconds);
        true
    }

    /// Retains a runtime visual's attached glow, reserving its bounded slot.
    /// Invalid cues or a full light budget leave the previous glow unchanged.
    pub fn set_runtime_character_glow(&mut self, instance_id: &str, glow: Option<GlowCue>) -> bool {
        let Some(character) = self.runtime_character(instance_id) else {
            return false;
        };
        let key = format!("{GLOW_KEY_PREFIX}{instance_id}");
        if let Some(cue) = glow.as_ref() {
            if !runtime_glow_is_valid(cue) {
                return false;
            }
            let Some(light) = attached_character_light(character, key.clone(), cue) else {
                return false;
            };
            if light.intensity > 0.0 {
                if !self.dynamic_lights.insert(light) {
                    return false;
                }
            } else {
                let _removed_light = self.dynamic_lights.remove(&key);
            }
        } else {
            let _removed_light = self.dynamic_lights.remove(&key);
        }
        let Some(live_character) = self.runtime_character_mut(instance_id) else {
            return false;
        };
        live_character.runtime_glow = glow;
        true
    }

    /// Hides a retained runtime actor and its attached light without respawning.
    pub fn set_runtime_character_visible(&mut self, instance_id: &str, visible: bool) -> bool {
        let Some(character) = self.runtime_character_mut(instance_id) else {
            return false;
        };
        character.runtime_visible = visible;
        if !visible {
            let _removed_light = self
                .dynamic_lights
                .remove(&format!("{GLOW_KEY_PREFIX}{instance_id}"));
        }
        true
    }

    /// Advances every character's pose by `delta_seconds`.
    ///
    /// A character addressed by an [`EntityFrame`] follows that frame: the
    /// route's live transform (when it moved) and its pose cue. A placed
    /// character with no frame keeps following the player's locomotion
    /// snapshot, so a level that authors no routes behaves exactly as before. A
    /// runtime character with no frame holds its current pose: it was spawned
    /// by gameplay and has no placed locomotion to inherit.
    pub fn update(
        &mut self,
        delta_seconds: f32,
        snapshot: LocomotionSnapshot,
        frames: &[EntityFrame],
    ) -> CharacterUpdate {
        let mut moved = 0usize;
        let mut finished: Vec<String> = Vec::new();
        for character in &mut self.characters {
            if Self::advance_character(character, delta_seconds, snapshot, frames, true) {
                moved = moved.saturating_add(1);
            }
            if character.animator.take_cue_finished()
                && let Some(instance_id) = character.instance_id.clone()
            {
                finished.push(instance_id);
            }
        }
        for character in &mut self.runtime {
            if Self::advance_character(character, delta_seconds, snapshot, frames, false) {
                moved = moved.saturating_add(1);
            }
            if character.animator.take_cue_finished()
                && let Some(instance_id) = character.instance_id.clone()
            {
                finished.push(instance_id);
            }
        }
        // Run after the poses advanced: an attached light's socket resolves
        // against the pose the frame will draw, not the previous one.
        self.update_dynamic_lights(frames);
        CharacterUpdate { moved, finished }
    }

    /// Refreshes all environmental samples after entity transforms advance.
    pub fn refresh_lighting(
        &mut self,
        lighting: &LevelLighting,
        irradiance: Option<&crate::engine::lighting::probes::ProbeField>,
    ) {
        self.refresh_lighting_with_scene(lighting, irradiance, None);
    }

    /// Use the same immutable triangle/alpha resource as rigid objects.
    pub fn refresh_lighting_with_scene(
        &mut self,
        lighting: &LevelLighting,
        irradiance: Option<&crate::engine::lighting::probes::ProbeField>,
        scene: Option<&crate::engine::lighting::transport::TransportScene>,
    ) {
        self.refresh_lighting_with_visibility(lighting, irradiance, scene, None);
    }

    /// Rigid movable occluders use their current transforms; the animated
    /// subject itself continues to use spatial bind-bounds lighting anchors.
    pub fn refresh_lighting_with_visibility(
        &mut self,
        lighting: &LevelLighting,
        irradiance: Option<&crate::engine::lighting::probes::ProbeField>,
        scene: Option<&crate::engine::lighting::transport::TransportScene>,
        visibility: Option<&super::dynamic_visibility::DynamicVisibility>,
    ) {
        self.refresh_lighting_with_revision(lighting, irradiance, scene, visibility, None);
    }

    /// Reuses center probes against a caller-owned lighting revision. Increment
    /// it whenever authored lighting or field contents change, including
    /// switches and in-place edits. Spatial samples still follow every movable
    /// visibility revision and every exact placement transform.
    pub fn refresh_lighting_with_visibility_and_revision(
        &mut self,
        lighting: &LevelLighting,
        irradiance: Option<&crate::engine::lighting::probes::ProbeField>,
        scene: Option<&crate::engine::lighting::transport::TransportScene>,
        visibility: Option<&super::dynamic_visibility::DynamicVisibility>,
        lighting_revision: u64,
    ) {
        self.refresh_lighting_with_revision(
            lighting,
            irradiance,
            scene,
            visibility,
            Some(lighting_revision),
        );
    }

    fn refresh_lighting_with_revision(
        &mut self,
        lighting: &LevelLighting,
        irradiance: Option<&crate::engine::lighting::probes::ProbeField>,
        scene: Option<&crate::engine::lighting::transport::TransportScene>,
        visibility: Option<&super::dynamic_visibility::DynamicVisibility>,
        lighting_revision: Option<u64>,
    ) {
        const PARALLEL_LIGHTING_MIN_RECEIVERS: usize = 8;
        let context = super::light_transport::EntityVisibility {
            dynamic: visibility,
            receiver: None,
        };
        let refresh_context = super::dynamic::EntityLightingRefreshContext::new(
            lighting,
            irradiance,
            scene,
            visibility,
            lighting_revision,
        );
        let dirty = |character: &Character| {
            refresh_context
                .key(
                    character.transform(),
                    character.lighting_local_bounds,
                    character.lighting_sample_position(),
                    None,
                )
                .is_none_or(|key| character.lighting_refresh_key != Some(key))
        };
        let refresh = |character: &mut Character| {
            if !dirty(character) {
                return;
            }
            let position = character.lighting_sample_position();
            let transform = character.transform();
            let refresh_key =
                refresh_context.key(transform, character.lighting_local_bounds, position, None);
            let center_key = lighting_revision.map(|revision| {
                super::dynamic::EntityCenterLightKey::new(revision, lighting, irradiance, position)
            });
            let center_changed = character.center_light_key != center_key;
            if center_key.is_none() || center_changed {
                character.entity_lighting =
                    super::light_transport::entity_lighting(lighting, irradiance, position);
                character.center_light_key = center_key;
            }
            if refresh_key.is_none_or(|key| key.spatial_changed(character.lighting_refresh_key)) {
                let spatial_key = super::light_transport::entity_spatial_key_with_visibility(
                    lighting, irradiance, transform, scene, context,
                );
                if character.spatial_key != spatial_key || refresh_key.is_some() {
                    character.spatial_lighting = (!character.lighting_local_bounds.is_empty())
                        .then(|| {
                            super::light_transport::entity_spatial_lighting_with_visibility(
                                lighting,
                                irradiance,
                                character.lighting_local_bounds,
                                transform,
                                scene,
                                context,
                            )
                        })
                        .flatten();
                    character.spatial_key = spatial_key;
                }
            }
            character.lighting_refresh_key = refresh_key;
        };
        // Each receiver owns its output; field/transport/visibility are shared
        // immutable snapshots. Join before the backend observes these samples.
        for receivers in [&mut self.characters, &mut self.runtime] {
            let dirty_count = receivers
                .iter()
                .filter(|character| dirty(character))
                .count();
            if dirty_count == 0 {
                continue;
            }
            let min_receivers = if dirty_count >= PARALLEL_LIGHTING_MIN_RECEIVERS {
                PARALLEL_LIGHTING_MIN_RECEIVERS
            } else {
                usize::MAX
            };
            crate::engine::runtime_work::for_each_mut(receivers, min_receivers, refresh);
        }
    }

    /// Rebuilds the attached-light set from this frame's handoff.
    ///
    /// One light per character whose frame carries a glow and whose effective
    /// intensity (`glow.intensity`, scaled by the clamped instance opacity when
    /// the cue asked for a fade) is positive and finite. The position is the
    /// world position of the glow's socket — `character.transform() *
    /// animator.node_global(socket)` in model space — or, when no socket is
    /// authored or the node does not resolve, the placement applied to the
    /// entity-local offset. Runtime visuals may retain a cue without a frame.
    /// An absent cue or zero effective intensity removes the key.
    fn update_dynamic_lights(&mut self, frames: &[EntityFrame]) {
        for character in self.characters.iter().chain(self.runtime.iter()) {
            let Some(instance_id) = character.instance_id() else {
                continue;
            };
            let glow = frames
                .iter()
                .find(|frame| frame.instance_id == instance_id)
                .and_then(|frame| frame.glow.as_ref())
                .or(character.runtime_glow.as_ref());
            // Ordinary balloons carry no glow. Do not allocate a light key
            // for a character that has never had an attached light.
            if glow.is_none()
                && !self
                    .dynamic_lights
                    .lights()
                    .iter()
                    .any(|light| light.key.strip_prefix(GLOW_KEY_PREFIX) == Some(instance_id))
            {
                continue;
            }
            let mut key =
                String::with_capacity(GLOW_KEY_PREFIX.len().saturating_add(instance_id.len()));
            key.push_str(GLOW_KEY_PREFIX);
            key.push_str(instance_id);
            let Some(glow_cue) = glow else {
                let _removed_value = self.dynamic_lights.remove(&key);
                continue;
            };
            let Some(light) = attached_character_light(character, key.clone(), glow_cue) else {
                let _removed_value_2 = self.dynamic_lights.remove(&key);
                continue;
            };
            if light.intensity <= 0.0 {
                let _removed_light = self.dynamic_lights.remove(&key);
                continue;
            }
            if !self.dynamic_lights.insert(light) {
                // The bounded set refused the value (a malformed cue or a full
                // budget): never keep a stale light for the refused key.
                let _removed_value_3 = self.dynamic_lights.remove(&key);
            }
        }
    }

    /// The attached lights this frame's glow cues produced.
    ///
    /// Rebuilt by the last [`Self::update`]; the backend uploads it verbatim.
    #[must_use]
    pub const fn dynamic_lights(&self) -> &DynamicLightSet {
        &self.dynamic_lights
    }

    /// Advances one character against this frame's handoff.
    ///
    /// `follow_locomotion` is true for a placed character: with no frame it
    /// follows the player's locomotion snapshot. A runtime character passes
    /// false and holds its current pose instead.
    fn advance_character(
        character: &mut Character,
        delta_seconds: f32,
        snapshot: LocomotionSnapshot,
        frames: &[EntityFrame],
        follow_locomotion: bool,
    ) -> bool {
        let animation_delta = character
            .runtime_frame_delta
            .take()
            .unwrap_or(delta_seconds);
        let frame = character
            .instance_id
            .as_deref()
            .and_then(|id| frames.iter().find(|frame| frame.instance_id == id));
        match frame {
            Some(entity_frame) => {
                // The frame's fade is applied every pass, before the pose: the
                // opacity is independent of whether the pose changed.
                character.set_opacity(entity_frame.opacity);
                if let Some((position, yaw)) = entity_frame.transform
                    && !transforms_agree(character.transform, position, character.scale, yaw)
                {
                    character.set_pose(position, yaw);
                }
                if let Some(balloon) = entity_frame.balloon {
                    return character.set_balloon_pose(balloon);
                }
                character
                    .animator
                    .update_cued(animation_delta, &entity_frame.cue)
            }
            None if character.runtime_locomotion.is_some() => {
                if character.animator.is_rigid() {
                    character
                        .animator
                        .update_cued(animation_delta, &PoseCue::Idle)
                } else {
                    character.animator.update(
                        animation_delta,
                        character.runtime_locomotion.unwrap_or_default(),
                    )
                }
            }
            // A rigid prop has no locomotion state to drive: until an action
            // cues it, it holds the bind pose it was spawned in.
            None if !follow_locomotion || character.animator.is_rigid() => false,
            None => character.animator.update(animation_delta, snapshot),
        }
    }
}

/// Apply the same local socket/offset contract to frame and retained cues.
fn attached_character_light(
    character: &Character,
    key: String,
    cue: &GlowCue,
) -> Option<DynamicLight> {
    let fade = if !character.runtime_visible {
        0.0
    } else if cue.fade_with_opacity {
        character.opacity
    } else {
        1.0
    };
    let position = cue
        .socket
        .as_deref()
        .and_then(|socket| character.animator.node_global(socket))
        .map_or_else(
            || character.transform.transform_point3(Vec3::from(cue.offset)),
            |node| {
                character
                    .transform
                    .mul_mat4(&node)
                    .transform_point3(Vec3::ZERO)
            },
        );
    let light = DynamicLight {
        key,
        position,
        color: cue.color,
        intensity: cue.intensity * fade,
        radius: cue.range,
    };
    light.is_well_formed().then_some(light)
}

fn runtime_glow_is_valid(cue: &GlowCue) -> bool {
    cue.color
        .iter()
        .all(|value| value.is_finite() && (0.0..=1.0).contains(value))
        && cue.intensity.is_finite()
        && (0.0..=crate::engine::level::MAX_GLOW_INTENSITY).contains(&cue.intensity)
        && cue.range.is_finite()
        && (crate::engine::level::MIN_GLOW_RANGE_M..=crate::engine::level::MAX_GLOW_RANGE_M)
            .contains(&cue.range)
        && cue.offset.iter().all(|value| {
            value.is_finite() && value.abs() <= crate::engine::level::MAX_GLOW_OFFSET_M
        })
        && cue
            .socket
            .as_ref()
            .is_none_or(|socket| !socket.trim().is_empty())
}

/// True when a placement matrix already matches a live pose within a
/// sub-millimetre, so a still route does not rebuild bounds every frame.
fn transforms_agree(transform: Mat4, position: Vec3, scale: f32, yaw: f32) -> bool {
    let expected = Mat4::from_scale_rotation_translation(
        Vec3::splat(scale),
        Quat::from_rotation_y(yaw),
        position,
    );
    let current: [f32; 16] = transform.to_cols_array();
    let expected_columns: [f32; 16] = expected.to_cols_array();
    current
        .iter()
        .zip(expected_columns.iter())
        .all(|(a, b)| (a - b).abs() <= 1.0e-4)
}

/// True when a placement's transform values can be composed safely.
fn placement_is_finite(prop: &crate::engine::level::PropDef) -> bool {
    prop.x.is_finite()
        && prop.y.is_finite()
        && prop.z.is_finite()
        && prop.rotation_degrees.is_finite()
        && prop.scale.is_finite()
        && prop.scale > 0.0
}

/// The placement matrix of a runtime character.
///
/// `position` is a world position — the value an [`EntityFrame`] carries and
/// [`Character::set_pose`] consumes — and `base_y` is the walkable floor the
/// level places under it. The composition is the one
/// [`prop_instance_matrix`] uses for a placed prop (translate, rotate about Y,
/// scale uniformly), with the prop's floor offset already folded into
/// `position.y`: a runtime actor resolves to the same matrix a placed prop at
/// the same world point would, and never jumps when its first frame arrives.
// f32 placement arithmetic: finite inputs
fn runtime_instance_matrix(position: Vec3, base_y: f32, yaw_degrees: f32, scale: f32) -> Mat4 {
    let offset = position.y - base_y;
    let translation = Vec3::new(position.x, base_y + offset, position.z);
    Mat4::from_translation(translation)
        .mul_mat4(&Mat4::from_rotation_y(yaw_degrees.to_radians()))
        .mul_mat4(&Mat4::from_scale(Vec3::splat(scale)))
}

/// Uses the same transformed bind-pose bounds centre as dynamic props.
/// Animation does not move the anchor within the body, avoiding pose flicker.
fn sample_character_light(
    asset: &LoadedPropAsset,
    transform: &Mat4,
    lighting: &LevelLighting,
    irradiance: Option<&crate::engine::lighting::probes::ProbeField>,
) -> super::light_transport::EntityLighting {
    let local = model_lighting_centre(asset);
    super::light_transport::entity_lighting(
        lighting,
        irradiance,
        transform.transform_point3(local).to_array(),
    )
}

fn model_lighting_centre(asset: &LoadedPropAsset) -> Vec3 {
    asset.model.bounds().map_or(Vec3::ZERO, |(min, max)| {
        Vec3::new(
            f32::midpoint(min[0], max[0]),
            f32::midpoint(min[1], max[1]),
            f32::midpoint(min[2], max[2]),
        )
    })
}

fn asset_bounds(asset: &LoadedPropAsset) -> Aabb {
    asset
        .model
        .bounds()
        .map_or(Aabb::EMPTY, |(min, max)| Aabb { min, max })
}

/// Padded bind-pose envelope for a placed character's scene queries.
fn character_bounds(asset: &LoadedPropAsset, transform: &Mat4) -> Aabb {
    let Some((low, high)) = asset.model.bounds() else {
        return Aabb::EMPTY;
    };
    let mut local = Aabb {
        min: low,
        max: high,
    };
    for axis in 0..3 {
        let min = local.min.get(axis).copied().unwrap_or(0.0);
        let max = local.max.get(axis).copied().unwrap_or(0.0);
        let half = (max - min) * 0.5 * (1.0 + CHARACTER_BOUNDS_EXPANSION);
        let centre = f32::midpoint(min, max);
        if let Some(slot) = local.min.get_mut(axis) {
            *slot = centre - half - CHARACTER_BOUNDS_MARGIN_M;
        }
        if let Some(slot) = local.max.get_mut(axis) {
            *slot = centre + half + CHARACTER_BOUNDS_MARGIN_M;
        }
    }
    transform_bounds(&local, transform)
}

/// Frame-rate-independent playback and skinning for one skinned model.
pub struct CharacterAnimator {
    rig: Rig,
    /// True for a clips-only model posed through the node hierarchy (a rigid
    /// prop). A rigid animator ignores the locomotion snapshot: with no pose
    /// cue it holds its current pose.
    rigid: bool,
    /// Named fixed-size node mapping, resolved once for the shared balloon rig.
    balloon_rig: Option<BalloonRig>,
    /// Blend weight per locomotion state; the current state approaches one and
    /// the others approach zero.
    weights: [f32; STATE_COUNT],
    /// Gait phase in cycles: walking advances per metre travelled, swimming
    /// per second.
    phase: f32,
    /// Free-running seconds, for idle sway and breathing.
    clock: f32,
    clips: Option<ClipPlayer>,
    /// Clip name to index, lower-cased; empty when the rig has no clips.
    named: Vec<(String, usize)>,
    /// Explicit clip playback (entity routes) overriding the locomotion
    /// states while a cue is set.
    cue: Option<CueState>,
    /// The clip, time and one-shot flag the active cue crossfaded from.
    cue_previous: Option<(usize, f32, bool)>,
    /// True while the first cue crossfades from the pre-cue pose buffers.
    cue_from_pose: bool,
    /// Seconds since the active cue started its crossfade.
    cue_fade: f32,
    /// True once a one-shot cue reached its last key; consumed by the caller.
    cue_finished: bool,
    /// The speed the walk clip is authored for, in metres per second.
    ///
    /// Kept as the fallback for an asset whose clips carry no declared
    /// reference speed; a declared per-clip value (from
    /// `asset.extras.places_entity_clips`) always wins.
    walk_reference_speed: f32,
    /// Per declared clip, the stride's authored ground speed, parallel to the
    /// model's `animations`.
    clip_reference_speeds: Vec<Option<f32>>,
    /// Per declared clip, the authored kind (`asset.extras.places_entity_clips`
    /// `kind`, lower-cased), parallel to the model's `animations`.
    ///
    /// The locomotion resolver uses it when a model declares no clip literally
    /// named `walk`/`run`: the pumpkin's `hop_forward` is its walk kind and the
    /// ghost's `float_forward` its float kind.
    clip_kinds: Vec<Option<String>>,
    /// Current morph weights, parallel to the model's morph targets.
    morphs: Vec<f32>,
    /// Default morph weights, restored before each sample.
    morph_defaults: Vec<f32>,
    /// Per node morph ranges, for `weights` channels.
    morph_ranges: Vec<(u16, u16, u16)>,
    /// Current local pose per node.
    pose: Vec<LocalTrs>,
    /// Current global transform per node.
    node_globals: Vec<Mat4>,
    /// Current model-space skinning delta per joint slot.
    deltas: Vec<Mat4>,
    /// Crossfade scratch per node, only allocated for a rig with clips.
    clip_a: Vec<LocalTrs>,
    clip_b: Vec<LocalTrs>,
    /// Morph weight scratch for the crossfade.
    morph_a: Vec<f32>,
    morph_b: Vec<f32>,
    revision: u64,
}

#[derive(Clone, Copy)]
struct BalloonRig {
    body: usize,
    strings: [usize; 12],
    body_bounds: Aabb,
}

impl BalloonRig {
    fn new(model: &crate::engine::gltf::PropModel, rig: &Rig) -> Option<Self> {
        if model.skin.is_some() {
            return None;
        }
        let body = rig
            .nodes
            .iter()
            .position(|node| node.name == "balloon_body")?;
        let parent = rig.nodes.get(body)?.parent;
        let parent_global = parent
            .and_then(|index| rig.rest_global.get(usize::from(index)))
            .copied()
            .unwrap_or(Mat4::IDENTITY);
        if parent_global
            .to_cols_array()
            .iter()
            .zip(Mat4::IDENTITY.to_cols_array())
            .any(|(actual, expected)| (*actual - expected).abs() > 0.00001)
        {
            return None;
        }
        let mut strings = [0_usize; 12];
        for (slot, name) in strings.iter_mut().zip(STRING_NODE_NAMES) {
            let index = rig.nodes.iter().position(|node| node.name == name)?;
            if rig.nodes.get(index)?.parent != parent {
                return None;
            }
            *slot = index;
        }
        let inverse = rig.rest_global.get(body)?.inverse();
        let mut body_bounds = Aabb::EMPTY;
        for (index, vertex) in model.vertices.iter().enumerate() {
            if model
                .joints
                .get(index)
                .and_then(|joints| joints.first())
                .and_then(|joint| rig.joint_nodes.get(usize::from(*joint)))
                .is_some_and(|node| usize::from(*node) == body)
            {
                body_bounds.expand(
                    inverse
                        .transform_point3(Vec3::from_array(vertex.pos))
                        .to_array(),
                );
            }
        }
        (!body_bounds.is_empty()).then_some(Self {
            body,
            strings,
            body_bounds,
        })
    }
}

/// What one explicit cue resolved to.
struct CueState {
    clip: usize,
    time: f32,
    once: bool,
    paused: bool,
    finished: bool,
    /// Clip-time target of a scrub cue, in the clip's own seconds. While set,
    /// the cue eases `time` toward it instead of advancing.
    scrub: Option<f32>,
}

/// The clip-time a scrub cue eases toward, or `None` for a playing cue.
fn scrub_target_for(cue: &PoseCue, duration: f32) -> Option<f32> {
    match cue {
        PoseCue::Scrub { target, name: _ } => {
            let fraction = if target.is_finite() {
                target.clamp(0.0, 1.0)
            } else {
                0.0
            };
            Some(duration * fraction)
        }
        PoseCue::Idle
        | PoseCue::Walk { speed_mps: _ }
        | PoseCue::Clip {
            name: _,
            once: _,
            paused: _,
        } => None,
    }
}

impl CharacterAnimator {
    /// Builds an animator for a skinned or rigidly animated model, or `None`
    /// when the model has nothing the character path can pose.
    ///
    /// A model with clips but no skin is posed *rigidly*: every node is a
    /// one-joint chain (see [`Rig::new_rigid`]). A rigid animator is driven by
    /// explicit pose cues only — it never runs the locomotion or procedural
    /// driver, so an idle prop holds its bind pose instead of looping a
    /// non-locomotion clip.
    #[must_use]
    pub fn new(model: &crate::engine::gltf::PropModel) -> Option<Self> {
        let (rig, rigid) = match model.skin.as_ref() {
            Some(skin) => (Rig::new(skin)?, false),
            None => (Rig::new_rigid(&model.nodes)?, true),
        };
        let node_count = rig.nodes.len();
        let balloon_rig = BalloonRig::new(model, &rig);
        let clips = ClipPlayer::new(&model.animations, node_count);
        let mut pose: Vec<LocalTrs> = Vec::with_capacity(node_count);
        for node in &rig.nodes {
            pose.push(LocalTrs::from_node(node));
        }
        let deltas = vec![Mat4::IDENTITY; rig.joint_nodes.len()];
        let (clip_a, clip_b) = if clips.is_some() {
            (
                vec![
                    LocalTrs {
                        translation: Vec3::ZERO,
                        rotation: Quat::IDENTITY,
                        scale: Vec3::ONE,
                    };
                    node_count
                ],
                vec![
                    LocalTrs {
                        translation: Vec3::ZERO,
                        rotation: Quat::IDENTITY,
                        scale: Vec3::ONE,
                    };
                    node_count
                ],
            )
        } else {
            (Vec::new(), Vec::new())
        };
        let mut weights = [0.0f32; STATE_COUNT];
        if let Some(slot) = weights.get_mut(IDLE) {
            *slot = 1.0;
        }
        let named: Vec<(String, usize)> = model
            .animations
            .iter()
            .enumerate()
            .map(|(index, animation)| (animation.name.to_ascii_lowercase(), index))
            .collect();
        let clip_reference_speeds: Vec<Option<f32>> = model
            .animations
            .iter()
            .map(|animation| animation.reference_speed_mps)
            .collect();
        let clip_kinds: Vec<Option<String>> = model
            .animations
            .iter()
            .map(|animation| {
                animation
                    .kind
                    .as_ref()
                    .map(|kind| kind.trim().to_ascii_lowercase())
                    .filter(|kind| !kind.is_empty())
            })
            .collect();
        let morph_count = model.morph_targets.len();
        let morph_defaults = model.morph_weights.clone();
        let morph_ranges = model.mesh_morph_ranges.clone();
        // The rest pose is what an animator that has not updated yet holds, so
        // the node globals start there rather than at the identity: an
        // attached light must find a named socket's bind-pose transform even
        // before the first pose pass.
        let node_globals = rig.rest_global.clone();
        Some(Self {
            rig,
            rigid,
            balloon_rig,
            weights,
            phase: 0.0,
            clock: 0.0,
            clips,
            named,
            cue: None,
            cue_previous: None,
            cue_from_pose: false,
            cue_fade: 1.0,
            cue_finished: false,
            walk_reference_speed: WALK_REFERENCE_SPEED_MPS,
            clip_reference_speeds,
            clip_kinds,
            morphs: vec![0.0; morph_count],
            morph_defaults,
            morph_ranges,
            pose,
            node_globals,
            deltas,
            clip_a,
            clip_b,
            morph_a: vec![0.0; morph_count],
            morph_b: vec![0.0; morph_count],
            revision: 0,
        })
    }

    /// Advances the pose by `delta_seconds` and returns whether the pose
    /// changed this pass.
    ///
    /// The animator allocates nothing here: every buffer is sized at
    /// construction and the weights approach their targets exponentially, so
    /// the same total time produces the same pose at any frame rate.
    // f32 pose arithmetic: finite inputs
    pub fn update(&mut self, delta_seconds: f32, snapshot: LocomotionSnapshot) -> bool {
        let step = if delta_seconds.is_finite() {
            delta_seconds.max(0.0)
        } else {
            0.0
        };
        let speed = if snapshot.speed.is_finite() {
            snapshot.speed.max(0.0)
        } else {
            0.0
        };
        let state = state_index(snapshot.state);
        let alpha = 1.0 - (-step / BLEND_TIME_CONSTANT_S).exp();
        let mut moved = false;
        for (slot, weight) in self.weights.iter_mut().enumerate() {
            let target = if slot == state { 1.0 } else { 0.0 };
            let next = (target - *weight).mul_add(alpha, *weight);
            if (next - *weight).abs() > 1.0e-5 {
                moved = true;
            }
            // Snap the tail of the approach so a settled state reads exactly
            // 1 and 0 and the pose stops drifting in the last ulps.
            *weight = if target > 0.5 {
                if next > 0.9995 { 1.0 } else { next }
            } else if next < 0.0005 {
                0.0
            } else {
                next
            };
        }
        if step > 0.0 {
            self.clock = (self.clock + step).rem_euclid(1.0e6);
            moved = true;
        }
        match snapshot.state {
            LocomotionState::Walking => {
                if speed > 0.0 {
                    self.phase = (speed * step)
                        .mul_add(WALK_CYCLES_PER_METRE, self.phase)
                        .rem_euclid(1.0);
                    moved = true;
                }
            }
            LocomotionState::Swimming | LocomotionState::SurfaceSwimming => {
                self.phase = f32::mul_add(step, SWIM_HZ, self.phase).rem_euclid(1.0);
            }
            LocomotionState::Idle | LocomotionState::Airborne => {}
        }
        if !moved {
            return false;
        }
        if !self.sample_locomotion_clips(step, state) {
            return false;
        }
        self.compose_globals();
        self.compute_deltas();
        self.revision = self.revision.wrapping_add(1);
        true
    }

    /// Samples the locomotion state's clip pair into the pose buffers.
    ///
    /// Returns false when the rig cannot produce a clip pose; a rig with no
    /// clips falls back to the procedural driver and always succeeds.
    fn sample_locomotion_clips(&mut self, step: f32, state: usize) -> bool {
        let Some(clips) = self.clips.as_mut() else {
            self.apply_procedural_pose();
            return true;
        };

        let fade = clips.advance(step, state);
        let active_index = clips.active;
        let previous_index = clips.previous;
        let Some(active) = clips.clips.get(active_index) else {
            return false;
        };
        if self.rig.nodes.is_empty() {
            return false;
        }
        active.sample(
            &self.rig,
            &MorphRig {
                defaults: &self.morph_defaults,
                ranges: &self.morph_ranges,
            },
            self.clock,
            true,
            &mut SampleTargets {
                pose: &mut self.clip_a,
                morphs: &mut self.morph_a,
            },
        );
        let blend_weight = match previous_index.and_then(|index| clips.clips.get(index)) {
            Some(previous) => {
                previous.sample(
                    &self.rig,
                    &MorphRig {
                        defaults: &self.morph_defaults,
                        ranges: &self.morph_ranges,
                    },
                    self.clock,
                    true,
                    &mut SampleTargets {
                        pose: &mut self.clip_b,
                        morphs: &mut self.morph_b,
                    },
                );
                fade
            }
            None => 1.0,
        };
        if blend_weight < 0.999 {
            for (node, (previous, active_pose)) in self
                .pose
                .iter_mut()
                .zip(self.clip_b.iter().zip(self.clip_a.iter()))
            {
                *node = previous.blend(*active_pose, blend_weight);
            }
            for (slot, (previous, active_morph)) in self
                .morphs
                .iter_mut()
                .zip(self.morph_b.iter().zip(self.morph_a.iter()))
            {
                *slot = (*active_morph - *previous).mul_add(blend_weight, *previous);
            }
        } else {
            for (node, active_pose) in self.pose.iter_mut().zip(self.clip_a.iter()) {
                *node = *active_pose;
            }
            self.morphs.copy_from_slice(&self.morph_a);
        }
        true
    }

    /// Poses the latex node and twelve visible string segments from an analytic cue.
    /// Existing node/pose/delta buffers are overwritten; no per-frame allocation.
    fn pose_balloon(&mut self, sampled: BalloonPose) -> bool {
        let Some(rig) = self.balloon_rig else {
            return false;
        };
        let mut changed = false;
        let body_pose = LocalTrs {
            translation: sampled.knot,
            rotation: sampled.rotation,
            scale: Vec3::splat(sampled.scale),
        };
        if let Some(slot) = self.pose.get_mut(rig.body) {
            changed |= *slot != body_pose;
            *slot = body_pose;
        }
        let mut start = sampled.anchor;
        for (segment, node) in (1_u8..=12).zip(rig.strings) {
            let end = sampled.string_point(f32::from(segment) / 12.0);
            let vector = Vec3::new(end.x - start.x, end.y - start.y, end.z - start.z);
            let length = vector.length();
            let string_pose = LocalTrs {
                translation: start,
                rotation: Quat::from_rotation_arc(Vec3::Y, vector.normalize_or_zero()),
                scale: Vec3::new(1.0, length.max(0.0001), 1.0),
            };
            if let Some(slot) = self.pose.get_mut(node) {
                changed |= *slot != string_pose;
                *slot = string_pose;
            }
            start = end;
        }
        if changed {
            self.compose_globals();
            self.compute_deltas();
            self.revision = self.revision.wrapping_add(1);
        }
        changed
    }

    /// The pose revision; it changes exactly when [`Self::update`] changed the
    /// pose, which is what the backend uses to skip vertex uploads.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// The blend weight of one locomotion state, in `0..=1`.
    #[must_use]
    pub fn state_weight(&self, state: LocomotionState) -> f32 {
        self.weights.get(state_index(state)).copied().unwrap_or(0.0)
    }

    /// The current gait phase, in cycles (`0..1`).
    #[must_use]
    pub const fn phase(&self) -> f32 {
        self.phase
    }

    /// Number of joints the model's skin declares.
    #[must_use]
    pub const fn joint_count(&self) -> usize {
        self.rig.joint_nodes.len()
    }

    /// One joint slot's current skinning delta, for tests.
    #[must_use]
    pub fn joint_delta(&self, slot: usize) -> Option<Mat4> {
        self.deltas.get(slot).copied()
    }

    /// A named node's current model-space global transform, or `None`.
    ///
    /// The lookup is case-insensitive over the rig's node names. The global is
    /// the composed local hierarchy of the current pose — the same value
    /// [`Self::joint_delta`] derives from — so an attached light reads the pose
    /// that is on screen. Before the first pose pass the globals hold the
    /// model's rest (bind) hierarchy, never the identity.
    #[must_use]
    pub fn node_global(&self, name: &str) -> Option<Mat4> {
        let index = self
            .rig
            .nodes
            .iter()
            .position(|node| node.name.eq_ignore_ascii_case(name))?;
        self.node_globals.get(index).copied()
    }

    /// Skins one bind-pose vertex with the current joint deltas.
    ///
    /// The weights are renormalised defensively; a vertex with no weight (a
    /// rigid primitive inside a rigged document) keeps its bind position.
    #[expect(clippy::arithmetic_side_effects, reason = "f32 blend: finite inputs")] // f32 blend: finite inputs
    #[must_use]
    pub fn skin_position(
        &self,
        joints: [u16; 4],
        weights: [f32; 4],
        position: [f32; 3],
    ) -> [f32; 3] {
        let point = Vec3::from(position);
        let mut blended = Vec3::ZERO;
        let mut total = 0.0f32;
        for (slot, weight) in joints.iter().zip(weights.iter()) {
            if *weight <= 0.0 {
                continue;
            }
            let Some(delta) = self.deltas.get(usize::from(*slot)) else {
                continue;
            };
            blended += delta.transform_point3(point) * *weight;
            total += *weight;
        }
        if total > 0.0 {
            let result = blended / total;
            [result.x, result.y, result.z]
        } else {
            position
        }
    }

    /// Transform an authored normal with the same weighted pose as positions.
    /// The inverse transpose of the blended linear transform preserves its
    /// orthogonality under joint rotation and non-uniform scale.
    #[must_use]
    pub fn skin_normal(&self, joints: [u16; 4], weights: [f32; 4], normal: [f32; 3]) -> [f32; 3] {
        let mut blended = Mat4::ZERO;
        let mut total = 0.0_f32;
        for (slot, weight) in joints.iter().zip(weights.iter()) {
            if *weight > 0.0
                && let Some(delta) = self.deltas.get(usize::from(*slot))
            {
                // Matrix products are finite floating-point pose arithmetic.
                #[expect(
                    clippy::arithmetic_side_effects,
                    reason = "glam matrix arithmetic has no integer overflow or panic path"
                )]
                {
                    blended += *delta * *weight;
                }
                total += *weight;
            }
        }
        if total <= 0.0 {
            return normal;
        }
        let Some(inverse) = super::normal::inverse_transpose(blended) else {
            return [0.0; 3];
        };
        inverse
            .transform_vector3(Vec3::from_array(normal))
            .normalize_or_zero()
            .to_array()
    }

    /// Buffer capacities, for the no-allocation regression test.
    #[must_use]
    pub const fn allocation_probe(&self) -> [usize; 5] {
        [
            self.pose.capacity(),
            self.node_globals.capacity(),
            self.deltas.capacity(),
            self.clip_a.capacity(),
            self.clip_b.capacity(),
        ]
    }

    /// The clip index whose lower-cased name matches `name`, if any.
    #[must_use]
    pub fn clip_index(&self, name: &str) -> Option<usize> {
        let lowercase_name = name.to_ascii_lowercase();
        self.named
            .iter()
            .find(|(candidate, _)| candidate == &lowercase_name)
            .map(|(_, index)| *index)
    }

    /// True while an explicit pose cue overrides the locomotion states.
    #[must_use]
    pub const fn has_pose_cue(&self) -> bool {
        self.cue.is_some()
    }

    /// True for a clips-only rigid model (no skin). A rigid animator is posed
    /// only by explicit cues and otherwise holds its current pose.
    #[must_use]
    pub const fn is_rigid(&self) -> bool {
        self.rigid
    }

    /// Consumes the one-shot completion of the active cue.
    pub const fn take_cue_finished(&mut self) -> bool {
        let finished = self.cue_finished;
        self.cue_finished = false;
        finished
    }

    /// Advances an explicit pose cue and returns whether the pose changed.
    ///
    /// A cue change crossfades from the previous clip's current time over the
    /// same time constant the locomotion states use, so a transition starts
    /// from the current pose. One-shot clips hold their last key and report
    /// completion through [`Self::take_cue_finished`].
    // bounded pose arithmetic
    pub fn update_cued(&mut self, delta_seconds: f32, cue: &PoseCue) -> bool {
        let Some(clips) = self.clips.as_ref() else {
            return false;
        };
        if self.rig.nodes.is_empty() {
            return false;
        }
        // A typo'd clip name falls back to the first clip; say so once rather
        // than silently playing the wrong pose.
        if let Some(name) = cue.clip_name()
            && self.clip_index(name).is_none()
        {
            crate::engine::logging::warn_once(
                format!("entity-clip-missing:{name}"),
                format!("[characters] no clip named `{name}`; playing the first clip instead"),
            );
        }
        let step = if delta_seconds.is_finite() {
            delta_seconds.max(0.0)
        } else {
            0.0
        };
        let (clip, once, paused, rate) = self.resolve_cue(cue);
        let Some(active_clip) = clips.clips.get(clip) else {
            return false;
        };
        let duration = active_clip.duration;
        let scrub_target = scrub_target_for(cue, duration);
        let changed = self
            .cue
            .as_ref()
            .is_none_or(|state| state.clip != clip || state.once != once);
        if changed {
            // A crossfade always starts from what is currently on screen: the
            // previous cue's pose, or the pre-cue pose for the first cue.
            self.cue_previous = self
                .cue
                .as_ref()
                .map(|state| (state.clip, state.time, state.once));
            self.cue_from_pose = self.cue.is_none();
            self.cue = Some(CueState {
                clip,
                time: 0.0,
                once,
                paused,
                finished: false,
                scrub: scrub_target,
            });
            self.cue_fade = 0.0;
            self.cue_finished = false;
        } else if let Some(state) = self.cue.as_mut() {
            state.paused = paused;
            state.scrub = scrub_target;
        }
        let Some(state) = self.cue.as_mut() else {
            return false;
        };
        if let Some(target) = scrub_target {
            // Ease toward the target at a constant clip-time rate. A new
            // target mid-travel simply reverses the direction from wherever
            // the pose currently is: no restart at an endpoint, no snap.
            let speed = if duration > 0.0 {
                duration / SCRUB_TRAVERSE_SECONDS
            } else {
                0.0
            };
            let travel = step * speed;
            let next = if state.time < target {
                (state.time + travel).min(target)
            } else if state.time > target {
                (state.time - travel).max(target)
            } else {
                target
            };
            state.time = next;
            if (state.time - target).abs() <= f32::EPSILON {
                if !state.finished {
                    state.finished = true;
                    self.cue_finished = true;
                }
            } else {
                // A scrub in flight is not finished, so a lever retargeted
                // after an earlier arrival reports the new arrival too.
                state.finished = false;
            }
        } else if !state.paused && step > 0.0 {
            state.time = f32::mul_add(step, rate, state.time);
            if state.once && state.time >= duration {
                state.time = duration;
                if !state.finished {
                    state.finished = true;
                    self.cue_finished = true;
                }
            } else if !state.once && duration > 0.0 {
                // A looping cue's clock stays bounded over long sessions.
                state.time = state.time.rem_euclid(duration);
            }
        }
        if self.cue_previous.is_some() {
            self.cue_fade = (self.cue_fade + step).max(0.0);
        }
        if !self.sample_active_cue() {
            return false;
        }
        self.compose_globals();
        self.compute_deltas();
        self.revision = self.revision.wrapping_add(1);
        true
    }

    /// Restarts the active cue at time zero (a one-shot replay hook).
    pub const fn restart_cue(&mut self) {
        if let Some(state) = self.cue.as_mut() {
            state.time = 0.0;
            state.finished = false;
            self.cue_finished = false;
        }
    }

    /// The best locomotion-kind clip for a model with no literal `walk`/`run`.
    ///
    /// A model may declare its ground/air locomotion through the asset's
    /// `places_entity_clips` metadata instead of a conventional clip name: the
    /// carved pumpkin's `hop_forward` (kind `walk`, 0.5 m/s) and the sheet
    /// ghost's `float_forward` (kind `float`, 0.3 m/s). Kinds are ranked so a
    /// model that declares several picks the most walk-like one, and equal
    /// ranks resolve in model order, so the choice is deterministic.
    fn declared_locomotion_clip(&self) -> Option<usize> {
        const KIND_RANK: [&str; 7] = ["walk", "run", "hop", "float", "fly", "swim", "crawl"];
        let mut best: Option<(usize, usize)> = None;
        for (index, kind) in self.clip_kinds.iter().enumerate() {
            let Some(declared_kind) = kind.as_deref() else {
                continue;
            };
            let Some((rank, _)) = KIND_RANK
                .iter()
                .enumerate()
                .find(|(_, name)| **name == declared_kind)
            else {
                continue;
            };
            if best.is_none_or(|(best_rank, _)| rank < best_rank) {
                best = Some((rank, index));
            }
        }
        best.map(|(_, index)| index)
    }

    /// Resolves a pose cue to `(clip, once, paused, time rate)`.
    ///
    /// * `Idle` uses the `idle` clip.
    /// * `Walk { speed }` picks `run` when the rig has one and the requested
    ///   speed is at least [`RUN_GAIT_MULTIPLIER`] times the walk clip's
    ///   reference speed; otherwise `walk`. When the model has no clip
    ///   literally named `walk`, the first clip whose declared kind is a
    ///   locomotion kind (`walk`, `run`, `hop`, `float`, `fly`, `swim`,
    ///   `crawl`, in that rank order) drives the gait — that is how the
    ///   pumpkin hops and the ghost floats on ordinary `move_to` route steps.
    ///   Either clip plays at `speed / its_own_reference_speed`, and a clip
    ///   with no declared reference falls back to the engine constants, so the
    ///   shipped Spoonerman keeps its 0.26 m/s walk.
    /// * `Clip { name, .. }` looks the name up case-insensitively.
    fn resolve_cue(&self, cue: &PoseCue) -> (usize, bool, bool, f32) {
        match cue {
            PoseCue::Idle => (self.clip_index("idle").unwrap_or(0), false, false, 1.0),
            PoseCue::Walk { speed_mps } => {
                let speed = if speed_mps.is_finite() {
                    speed_mps.max(0.0)
                } else {
                    0.0
                };
                let reference_of = |index: usize| {
                    self.clip_reference_speeds
                        .get(index)
                        .copied()
                        .flatten()
                        .unwrap_or(self.walk_reference_speed)
                };
                let literal_walk = self.clip_index("walk");
                let literal_run = self.clip_index("run");
                if let Some(walk_index) = literal_walk {
                    let walk_reference = reference_of(walk_index);
                    if let Some(run_index) = literal_run
                        && speed >= walk_reference * RUN_GAIT_MULTIPLIER
                    {
                        let run_reference = self
                            .clip_reference_speeds
                            .get(run_index)
                            .copied()
                            .flatten()
                            .unwrap_or(RUN_REFERENCE_SPEED_MPS);
                        return (
                            run_index,
                            false,
                            false,
                            (speed / run_reference).clamp(0.05, 4.0),
                        );
                    }
                    return (
                        walk_index,
                        false,
                        false,
                        (speed / walk_reference).clamp(0.05, 4.0),
                    );
                }
                // No conventional name: the model's own declared locomotion
                // kind wins, so `hop_forward`/`float_forward` play at their
                // authored speed on an ordinary walk cue.
                if let Some(index) = self.declared_locomotion_clip() {
                    return (
                        index,
                        false,
                        false,
                        (speed / reference_of(index)).clamp(0.05, 4.0),
                    );
                }
                let fallback = literal_run.unwrap_or(0);
                let reference = self
                    .clip_reference_speeds
                    .get(fallback)
                    .copied()
                    .flatten()
                    .unwrap_or(self.walk_reference_speed);
                (fallback, false, false, (speed / reference).clamp(0.05, 4.0))
            }
            PoseCue::Clip { name, once, paused } => {
                (self.clip_index(name).unwrap_or(0), *once, *paused, 1.0)
            }
            // A scrub cue never advances on its own: `update_cued` eases its
            // time toward the authored fraction instead.
            PoseCue::Scrub { name, target: _ } => {
                (self.clip_index(name).unwrap_or(0), true, false, 1.0)
            }
        }
    }

    /// Samples the active cue (and its crossfade source) into the pose buffers.
    fn sample_active_cue(&mut self) -> bool {
        let Some(clips) = self.clips.as_ref() else {
            return false;
        };
        let Some(state) = self.cue.as_ref() else {
            return false;
        };
        let clip = state.clip;
        let time = state.time;
        let once = state.once;
        let fade = 1.0 - (-self.cue_fade / BLEND_TIME_CONSTANT_S).exp();
        if fade >= 0.999 {
            self.cue_previous = None;
        }
        let previous = self
            .cue_previous
            .and_then(|(index, previous_time, was_once)| {
                clips
                    .clips
                    .get(index)
                    .map(|source_clip| (source_clip, previous_time, was_once))
            });
        let Some(active) = clips.clips.get(clip) else {
            return false;
        };
        if self.cue_from_pose && fade < 0.999 {
            // Freeze the pre-cue pose as the crossfade source.
            self.clip_b.copy_from_slice(&self.pose);
            self.morph_b.copy_from_slice(&self.morphs);
        }
        active.sample(
            &self.rig,
            &MorphRig {
                defaults: &self.morph_defaults,
                ranges: &self.morph_ranges,
            },
            time,
            !once,
            &mut SampleTargets {
                pose: &mut self.clip_a,
                morphs: &mut self.morph_a,
            },
        );
        if let Some((source_clip, previous_time, was_once)) = previous {
            source_clip.sample(
                &self.rig,
                &MorphRig {
                    defaults: &self.morph_defaults,
                    ranges: &self.morph_ranges,
                },
                previous_time,
                // A held one-shot must keep its last key; a looping clip wraps.
                !was_once,
                &mut SampleTargets {
                    pose: &mut self.clip_b,
                    morphs: &mut self.morph_b,
                },
            );
            for (node, (previous_pose, active_pose)) in self
                .pose
                .iter_mut()
                .zip(self.clip_b.iter().zip(self.clip_a.iter()))
            {
                *node = previous_pose.blend(*active_pose, fade);
            }
            for (slot, (previous_morph, active_morph)) in self
                .morphs
                .iter_mut()
                .zip(self.morph_b.iter().zip(self.morph_a.iter()))
            {
                *slot = (*active_morph - *previous_morph).mul_add(fade, *previous_morph);
            }
        } else {
            for (node, active_pose) in self.pose.iter_mut().zip(self.clip_a.iter()) {
                *node = *active_pose;
            }
            self.morphs.copy_from_slice(&self.morph_a);
        }
        if fade >= 0.999 {
            self.cue_from_pose = false;
        }
        true
    }

    /// Writes the procedural locomotion pose from the state weights.
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "f32 pose arithmetic: finite inputs"
    )] // f32 pose arithmetic: finite inputs
    fn apply_procedural_pose(&mut self) {
        let weights = self.weights;
        let phase = self.phase;
        let clock = self.clock;
        for (node, node_pose) in self.rig.pose.iter().enumerate() {
            let Some(rest) = self.rig.nodes.get(node) else {
                continue;
            };
            let (lateral, up) = procedural_angles(node_pose, &weights, phase, clock);
            let rotation = Quat::from_axis_angle(node_pose.axis_up, up.to_radians())
                .mul_quat(Quat::from_axis_angle(
                    node_pose.axis_lateral,
                    lateral.to_radians(),
                ))
                .mul_quat(LocalTrs::from_node(rest).rotation);
            let mut translation = Vec3::from(rest.translation);
            if node_pose.body {
                let walk_bob = -((TAU * 2.0 * phase).sin().abs()) * 0.01;
                let idle_bob = (TAU * clock * BREATH_HZ).sin() * 0.0015;
                translation += node_pose.up_in_parent
                    * idle_bob.mul_add(
                        weights.get(IDLE).copied().unwrap_or(0.0),
                        walk_bob * weights.get(WALKING).copied().unwrap_or(0.0),
                    );
            }
            if let Some(slot) = self.pose.get_mut(node) {
                *slot = LocalTrs {
                    translation,
                    rotation,
                    scale: Vec3::from(rest.scale),
                };
            }
        }
    }

    /// Composes every node's local pose into a global transform, parents
    /// first.
    fn compose_globals(&mut self) {
        // `glam` matrix multiplication is per-element f32 arithmetic with no
        // overflow or panic path; clippy cannot see that through the operator.

        for node in &self.rig.order {
            let index = usize::from(*node);
            let local = self
                .pose
                .get(index)
                .copied()
                .map_or(Mat4::IDENTITY, LocalTrs::matrix);
            let global = match self.rig.parent.get(index).copied().flatten() {
                Some(parent) => self
                    .node_globals
                    .get(usize::from(parent))
                    .copied()
                    .unwrap_or(Mat4::IDENTITY)
                    .mul_mat4(&local),
                None => local,
            };
            if let Some(slot) = self.node_globals.get_mut(index) {
                *slot = global;
            }
        }
    }

    /// Computes each joint slot's model-space delta from the composed pose.
    fn compute_deltas(&mut self) {
        let (mesh_rest, mesh_current, invertible) = match self.rig.mesh_node {
            Some(node) => {
                let index = usize::from(node);
                let rest = self
                    .rig
                    .rest_global
                    .get(index)
                    .copied()
                    .unwrap_or(Mat4::IDENTITY);
                let current = self
                    .node_globals
                    .get(index)
                    .copied()
                    .unwrap_or(Mat4::IDENTITY);
                let determinant = current.determinant();
                (
                    rest,
                    current,
                    determinant.is_finite() && determinant.abs() > 1.0e-12,
                )
            }
            None => (Mat4::IDENTITY, Mat4::IDENTITY, true),
        };
        let mesh_current_inverse = mesh_current.inverse();
        for slot in 0..self.rig.joint_nodes.len() {
            let Some(node) = self.rig.joint_nodes.get(slot).copied() else {
                continue;
            };
            let current = self
                .node_globals
                .get(usize::from(node))
                .copied()
                .unwrap_or(Mat4::IDENTITY);
            let rest_inverse = self
                .rig
                .joint_rest_inverse
                .get(slot)
                .copied()
                .unwrap_or(Mat4::IDENTITY);
            let delta = if invertible {
                mesh_current_inverse
                    .mul_mat4(&current)
                    .mul_mat4(&rest_inverse)
                    .mul_mat4(&mesh_rest)
            } else {
                Mat4::IDENTITY
            };
            if let Some(delta_slot) = self.deltas.get_mut(slot) {
                *delta_slot = delta;
            }
        }
    }
}

/// The procedural swing angles one node contributes, in degrees.
///
/// Each state contributes an independent angle set and the state weights
/// blend them, so a transition is a smooth interpolation of the two gaits
/// rather than a switch. `lateral` swings about the character's lateral axis
/// (leg forward/back, tail up/down); `up` swings about its up axis (tail
/// sway).
// f32 pose arithmetic: finite inputs
fn procedural_angles(
    pose: &NodePose,
    weights: &[f32; STATE_COUNT],
    phase: f32,
    clock: f32,
) -> (f32, f32) {
    let mut lateral = 0.0f32;
    let mut up = 0.0f32;
    for (state, weight) in weights.iter().enumerate() {
        if *weight <= 0.0 {
            continue;
        }
        let (state_lateral, state_up) = match state {
            IDLE => idle_angles(pose, clock),
            WALKING => walk_angles(pose, phase),
            AIRBORNE => air_angles(pose),
            SWIMMING => swim_angles(pose, phase, 1.0),
            SURFACE_SWIMMING => swim_angles(pose, phase, 0.8),
            _ => (0.0, 0.0),
        };
        lateral += state_lateral * weight;
        up += state_up * weight;
    }
    (lateral, up)
}

/// Idle: a slow tail sway and a breathing body chain.
fn idle_angles(pose: &NodePose, clock: f32) -> (f32, f32) {
    match pose.kind {
        JointKind::Tail => {
            let index = f32::from(pose.index);
            let sway = (TAU * index.mul_add(0.08, clock * IDLE_SWAY_HZ)).sin();
            let curl = (TAU * index.mul_add(0.05, clock * IDLE_SWAY_HZ * 0.5)).sin();
            (curl * 2.5, sway * 7.0)
        }
        JointKind::Spine | JointKind::Chest | JointKind::Neck | JointKind::Head => {
            let offset = if pose.kind == JointKind::Spine {
                0.0
            } else if pose.kind == JointKind::Chest {
                0.12
            } else if pose.kind == JointKind::Neck {
                0.24
            } else {
                0.36
            };
            ((TAU * clock.mul_add(BREATH_HZ, offset)).sin() * 1.4, 0.0)
        }
        JointKind::LegUpper
        | JointKind::LegLower
        | JointKind::LegPaw
        | JointKind::Body
        | JointKind::Other => (0.0, 0.0),
    }
}

/// Walking: diagonal leg pairs swing, the tail counter-sways.
fn walk_angles(pose: &NodePose, phase: f32) -> (f32, f32) {
    let pair_phase = f32::from(pose.pair).mul_add(0.5, phase);
    let swing = (TAU * pair_phase).sin();
    match pose.kind {
        JointKind::LegUpper => (swing * if pose.front { 24.0 } else { 20.0 }, 0.0),
        JointKind::LegLower => {
            // The knee folds while the leg is behind, which is the stance
            // recovery half of the cycle.
            let fold = (1.0 - swing) * 0.5;
            (-fold * if pose.front { 12.0 } else { 15.0 }, 0.0)
        }
        JointKind::LegPaw => (swing * 6.0, 0.0),
        JointKind::Tail => {
            let index = f32::from(pose.index);
            (0.0, (TAU * index.mul_add(0.1, phase)).sin() * 5.0)
        }
        JointKind::Head | JointKind::Neck => ((TAU * (phase + 0.25)).sin() * 2.0, 0.0),
        JointKind::Spine | JointKind::Chest => (0.0, (TAU * phase).sin() * 2.0),
        JointKind::Body | JointKind::Other => (0.0, 0.0),
    }
}

/// Airborne: a fixed gathered pose, blended in over the state transition.
const fn air_angles(pose: &NodePose) -> (f32, f32) {
    match pose.kind {
        JointKind::LegUpper => (if pose.front { 24.0 } else { -26.0 }, 0.0),
        JointKind::LegLower => (if pose.front { -16.0 } else { 20.0 }, 0.0),
        JointKind::LegPaw => (if pose.front { 8.0 } else { -6.0 }, 0.0),
        JointKind::Tail => (14.0, 0.0),
        JointKind::Spine => (-4.0, 0.0),
        JointKind::Chest => (-3.0, 0.0),
        JointKind::Neck => (3.0, 0.0),
        JointKind::Head => (6.0, 0.0),
        JointKind::Body | JointKind::Other => (0.0, 0.0),
    }
}

/// Swimming and surface swimming: small, fast paddle strokes and a lifted
/// tail.
fn swim_angles(pose: &NodePose, phase: f32, scale: f32) -> (f32, f32) {
    let pair_phase = f32::from(pose.pair).mul_add(0.5, phase);
    let swing = (TAU * pair_phase).sin();
    let recovery = (TAU * (pair_phase + 0.25)).sin();
    match pose.kind {
        JointKind::LegUpper => (swing * 14.0 * scale, 0.0),
        JointKind::LegLower => (recovery * 12.0 * scale, 0.0),
        JointKind::LegPaw => (swing * 5.0 * scale, 0.0),
        JointKind::Tail => {
            let index = f32::from(pose.index);
            (
                swing.mul_add(3.0, 10.0) * scale,
                (TAU * index.mul_add(0.1, phase)).sin() * 8.0 * scale,
            )
        }
        JointKind::Spine | JointKind::Chest => ((3.0 - scale) * 1.5, 0.0),
        JointKind::Neck => (-3.0 * scale, 0.0),
        JointKind::Head => (-4.0 * scale, 0.0),
        JointKind::Body | JointKind::Other => (0.0, 0.0),
    }
}

/// The neutral vertex a posed character vertex uploads as.
///
/// Colour contains only material albedo. The environment uniform supplies
/// current lighting; no static lightmap channel is used.
#[must_use]
pub const fn character_vertex(albedo: [f32; 4], uv: [f32; 2], position: [f32; 3]) -> Vertex {
    Vertex {
        pos: position,
        color: albedo,
        uv,
        lightmap: [0; 2],
        lightmap_page: LIGHTMAP_NONE,
        ..Vertex::UNLIT
    }
}
