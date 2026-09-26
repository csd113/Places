//! Animated characters: the engine's skinned-entity path.
//!
//! A character is a placeable prop whose model carries a glTF skin
//! ([`crate::gltf::PropSkin`]). It renders through the ordinary level
//! placement transform, but its vertices are re-posed on the CPU every frame
//! instead of being baked into a static batch:
//!
//! ```text
//! static prop                          character (this module)
//! -----------                          -----------------------
//! baked into PropMeshBatch at load     CPU-skinned every frame
//! bind pose only                       pose follows LocomotionSnapshot
//! one draw per model and cell          one draw per character per primitive
//! part of the lightmap bake            baked light sampled once at spawn
//! ```
//!
//! The bind pose is *also* baked into the static prop batch, so a skinned
//! model still contributes real geometry, occludes light and passes the
//! shipped-asset tests exactly like any other prop; the renderer suppresses
//! only the GPU draw of a claimed model and draws the animated character
//! instead.
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
use std::rc::Rc;

use glam::{Mat4, Quat, Vec3};

use super::Vertex;
use super::mesh::LIGHTMAP_NONE;
use super::props::{prop_instance_matrix, transform_bounds};
use crate::game::{LocomotionSnapshot, LocomotionState};
use crate::gltf::{AnimationInterpolation, PropAnimation, PropNode, PropSkin};
use crate::level::LevelSurfaces;
use crate::lighting::LevelLighting;
use crate::props::{LoadedPropAsset, PropAssets};
use crate::spatial::Aabb;

/// Explicit per-entity pose requests live on the gameplay side
/// ([`crate::entity::PoseCue`]); a character can be addressed by its placed
/// instance id so routes and interactions drive it.
pub use crate::entity::{EntityFrame, PoseCue};

/// Most characters one level may spawn.
///
/// The draw path is one draw per character per primitive, so the cap keeps a
/// pathological level from turning a batching prop field into hundreds of
/// per-frame skins. Placements past the cap stay in the static prop path in
/// their bind pose.
pub const MAX_CHARACTERS: usize = 8;

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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CharacterUpdate {
    /// Characters whose pose changed and whose vertices therefore need an
    /// upload.
    pub moved: usize,
}

/// One node's local pose: the animator's working TRS.
#[derive(Clone, Copy, Debug)]
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
    path: crate::gltf::AnimationPath,
    interpolation: AnimationInterpolation,
    times: Vec<f32>,
    values: Vec<f32>,
    values_per_key: usize,
}

impl Channel {
    /// Builds a playback channel from one parsed channel.
    fn from_channel(channel: &crate::gltf::PropAnimationChannel) -> Self {
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
    #[allow(clippy::arithmetic_side_effects)] // bounded keyframe arithmetic
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
            crate::gltf::AnimationPath::Translation => {
                pose.translation = Vec3::new(read(0), read(1), read(2));
            }
            crate::gltf::AnimationPath::Scale => {
                pose.scale = Vec3::new(read(0), read(1), read(2));
            }
            crate::gltf::AnimationPath::Rotation => {
                if let Some(quat) = self.sample_rotation(time) {
                    pose.rotation = quat;
                }
            }
            crate::gltf::AnimationPath::Weights => {
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
            if let Some(slot) = slot {
                if state_clip.get(slot).is_some_and(Option::is_none)
                    && let Some(entry) = state_clip.get_mut(slot)
                {
                    *entry = Some(index);
                }
                if slot == SWIMMING
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
        let time = if time.is_finite() { time } else { 0.0 };
        let local_time = if self.duration > 0.0 {
            if looping {
                time.rem_euclid(self.duration)
            } else {
                time.clamp(0.0, self.duration)
            }
        } else {
            0.0
        };
        for (node, node_channels) in self.channels.iter().enumerate() {
            let Some(slot) = out.pose.get_mut(node) else {
                continue;
            };
            let morph_range = u16::try_from(node).ok().and_then(|node| {
                morph
                    .ranges
                    .iter()
                    .find(|(candidate, _, count)| *candidate == node && *count > 0)
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
                .map_or(local, |parent| {
                    rest_global
                        .get(usize::from(parent))
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
                    .map_or(Quat::IDENTITY, |parent| {
                        rest_global
                            .get(usize::from(parent))
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
            if let Some((pair, front, index)) = leg_facts(&node.name) {
                node_pose.pair = pair;
                node_pose.front = front;
                node_pose.index = index;
            } else if kind == JointKind::Tail {
                node_pose.index = tail_index(&node.name);
            }
            pose.push(node_pose);
        }
        // A rig with no node named like a body still gets a bob carrier: the
        // rig's root, whose parent is by definition the outermost node, so the
        // character's up axis is its parent-space Y.
        if !pose.iter().any(|node| node.body)
            && let Some(root) = root
            && let Some(slot) = pose.get_mut(usize::from(root))
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
        for (index, parent) in parent.iter().enumerate() {
            if emitted.get(index).copied().unwrap_or(true) {
                continue;
            }
            let ready = parent
                .as_ref()
                .is_none_or(|parent| emitted.get(usize::from(*parent)).copied().unwrap_or(false));
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
    asset: Rc<LoadedPropAsset>,
    transform: Mat4,
    animator: CharacterAnimator,
    /// Model-space albedo with the baked light sampled once at spawn, parallel
    /// to the model's `vertices`.
    albedo: Vec<[f32; 4]>,
    world_bounds: Aabb,
    /// Placed-instance id, so a route or an interaction can address this
    /// character. `None` for a placement outside the prop id namespace.
    instance_id: Option<String>,
    /// Uniform placement scale, retained so a live pose change composes the
    /// same transform the static prop path did.
    scale: f32,
}

impl Character {
    /// The shared decoded model.
    #[must_use]
    pub const fn asset(&self) -> &Rc<LoadedPropAsset> {
        &self.asset
    }

    /// The placement transform (translate x yaw x uniform scale), the same
    /// transform the static prop path uses.
    #[must_use]
    pub const fn transform(&self) -> Mat4 {
        self.transform
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

    /// Mutable access to the pose evaluator.
    pub const fn animator_mut(&mut self) -> &mut CharacterAnimator {
        &mut self.animator
    }

    /// Moves the character to a live base position and yaw (radians).
    ///
    /// Rebuilds the same placement matrix `prop_instance_matrix` produced and
    /// recomputes the conservative world bounds, so culling follows the
    /// character. The mesh vertices are model-space and are unaffected.
    pub fn set_pose(&mut self, position: Vec3, yaw: f32) {
        let rotation = Quat::from_rotation_y(yaw);
        self.transform =
            Mat4::from_scale_rotation_translation(Vec3::splat(self.scale), rotation, position);
        self.world_bounds = character_bounds(&self.asset, &self.transform);
    }

    /// Model-space albedo x baked light per vertex.
    #[must_use]
    pub fn albedo(&self) -> &[[f32; 4]] {
        &self.albedo
    }

    /// World-space culling bounds, conservative for every pose.
    #[must_use]
    pub const fn world_bounds(&self) -> Aabb {
        self.world_bounds
    }
}

/// Every character a level spawns, plus the model paths the static prop draw
/// path must suppress.
#[derive(Default)]
pub struct CharacterScene {
    characters: Vec<Character>,
    claimed_models: Vec<String>,
}

impl CharacterScene {
    /// An empty scene.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of live characters.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.characters.len()
    }

    /// True when no character is live.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.characters.is_empty()
    }

    /// The live characters, in placement order.
    #[must_use]
    pub fn characters(&self) -> &[Character] {
        &self.characters
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

    /// Claims every placed prop whose resolved model carries a skin.
    ///
    /// Placement uses the same transform as the static path, so a character
    /// stands exactly where its prop entry says. Baked lighting is sampled
    /// once per model vertex at the rest pose; like the dynamic-object probe,
    /// it does not follow the pose, which keeps the per-frame work a pure
    /// matrix blend. A model whose asset fails to resolve is left to the
    /// static path's placeholder handling.
    #[must_use]
    pub fn spawn_characters(
        level: &crate::level::LevelDef,
        catalog: &crate::loader::PropCatalog,
        assets: &mut PropAssets,
        lighting: &LevelLighting,
    ) -> Self {
        let surfaces = LevelSurfaces::new(level);
        let instance_ids = level.prop_instance_ids();
        let mut characters: Vec<Character> = Vec::new();
        // Insertion-ordered placement counts per model path, so
        // `claimed_models` is deterministic.
        let mut order: Vec<String> = Vec::new();
        let mut placements: HashMap<String, (usize, usize)> = HashMap::new();
        let mut overflow_reported = false;
        for (prop_index, prop) in level.props.iter().enumerate() {
            // A floating prop is drawn and moved by the dynamic float path;
            // even a skinned model must never ride both lanes.
            if prop.float.is_some() {
                continue;
            }
            let entry = catalog.get(&prop.model);
            let Some(model_path) = entry.model.clone() else {
                continue;
            };
            let counts = placements.entry(model_path.clone()).or_insert((0, 0));
            if counts.0 == 0 {
                order.push(model_path.clone());
            }
            counts.0 = counts.0.saturating_add(1);
            if !placement_is_finite(prop) {
                continue;
            }
            if characters.len() >= MAX_CHARACTERS {
                if !overflow_reported {
                    overflow_reported = true;
                    crate::logging::warn_once(
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
            let base_y = surfaces.floor_y_at(prop.x, prop.z).unwrap_or(0.0);
            let transform = prop_instance_matrix(prop, base_y);
            let albedo = sample_albedo(&asset, &transform, lighting);
            let world_bounds = character_bounds(&asset, &transform);
            let instance_id = instance_ids
                .get(prop_index)
                .map(|id| id.trim())
                .filter(|id| !id.is_empty())
                .map(str::to_string);
            characters.push(Character {
                asset,
                transform,
                animator,
                albedo,
                world_bounds,
                instance_id,
                scale: prop.scale,
            });
            if let Some(counts) = placements.get_mut(&model_path) {
                counts.1 = counts.1.saturating_add(1);
            }
        }
        let claimed_models = order
            .into_iter()
            .filter(|path| {
                placements
                    .get(path)
                    .is_some_and(|(placed, claimed)| claimed > &0 && claimed == placed)
            })
            .collect();
        Self {
            characters,
            claimed_models,
        }
    }

    /// Advances every character's pose by `delta_seconds`.
    ///
    /// A character addressed by an [`EntityFrame`] follows that frame: the
    /// route's live transform (when it moved) and its pose cue. Characters
    /// with no frame keep following the player's locomotion snapshot, so a
    /// level that authors no routes behaves exactly as before.
    pub fn update(
        &mut self,
        delta_seconds: f32,
        snapshot: LocomotionSnapshot,
        frames: &[EntityFrame],
    ) -> CharacterUpdate {
        let mut moved = 0usize;
        for character in &mut self.characters {
            let frame = character
                .instance_id
                .as_deref()
                .and_then(|id| frames.iter().find(|frame| frame.instance_id == id));
            let changed = match frame {
                Some(frame) => {
                    if let Some((position, yaw)) = frame.transform
                        && !transforms_agree(character.transform, position, character.scale, yaw)
                    {
                        character.set_pose(position, yaw);
                    }
                    character.animator.update_cued(delta_seconds, &frame.cue)
                }
                // A rigid prop has no locomotion state to drive: until an
                // action cues it, it holds the bind pose it was spawned in.
                None if character.animator.is_rigid() => false,
                None => character.animator.update(delta_seconds, snapshot),
            };
            if changed {
                moved = moved.saturating_add(1);
            }
        }
        CharacterUpdate { moved }
    }
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
    let expected: [f32; 16] = expected.to_cols_array();
    current
        .iter()
        .zip(expected.iter())
        .all(|(a, b)| (a - b).abs() <= 1.0e-4)
}

/// True when a placement's transform values can be composed safely.
fn placement_is_finite(prop: &crate::level::PropDef) -> bool {
    prop.x.is_finite()
        && prop.y.is_finite()
        && prop.z.is_finite()
        && prop.rotation_degrees.is_finite()
        && prop.scale.is_finite()
        && prop.scale > 0.0
}

/// Samples each bind-pose vertex's albedo times baked light in world space.
fn sample_albedo(
    asset: &LoadedPropAsset,
    transform: &Mat4,
    lighting: &LevelLighting,
) -> Vec<[f32; 4]> {
    asset
        .model
        .vertices
        .iter()
        .map(|vertex| {
            let position = transform.transform_point3(Vec3::from(vertex.pos));
            let light = lighting.sample(position.x, position.y, position.z);
            [
                vertex.color[0] * light.r,
                vertex.color[1] * light.g,
                vertex.color[2] * light.b,
                vertex.color[3],
            ]
        })
        .collect()
}

/// Conservative world-space culling bounds for a placed character.
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
        PoseCue::Scrub { target, .. } => {
            let fraction = if target.is_finite() {
                target.clamp(0.0, 1.0)
            } else {
                0.0
            };
            Some(duration * fraction)
        }
        PoseCue::Idle | PoseCue::Walk { .. } | PoseCue::Clip { .. } => None,
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
    pub fn new(model: &crate::gltf::PropModel) -> Option<Self> {
        let (rig, rigid) = match model.skin.as_ref() {
            Some(skin) => (Rig::new(skin)?, false),
            None => (Rig::new_rigid(&model.nodes)?, true),
        };
        let node_count = rig.nodes.len();
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
        let morph_count = model.morph_targets.len();
        let morph_defaults = model.morph_weights.clone();
        let morph_ranges = model.mesh_morph_ranges.clone();
        Some(Self {
            rig,
            rigid,
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
            morphs: vec![0.0; morph_count],
            morph_defaults,
            morph_ranges,
            pose,
            node_globals: vec![Mat4::IDENTITY; node_count],
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
    #[allow(clippy::arithmetic_side_effects)] // f32 pose arithmetic: finite inputs
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
                self.phase = (self.phase + step * SWIM_HZ).rem_euclid(1.0);
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
        let fade = match previous_index.and_then(|index| clips.clips.get(index)) {
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
        if fade < 0.999 {
            for (node, (previous, active)) in self
                .pose
                .iter_mut()
                .zip(self.clip_b.iter().zip(self.clip_a.iter()))
            {
                *node = previous.blend(*active, fade);
            }
            for (slot, (previous, active)) in self
                .morphs
                .iter_mut()
                .zip(self.morph_b.iter().zip(self.morph_a.iter()))
            {
                *slot = (*active - *previous).mul_add(fade, *previous);
            }
        } else {
            for (node, active) in self.pose.iter_mut().zip(self.clip_a.iter()) {
                *node = *active;
            }
            self.morphs.copy_from_slice(&self.morph_a);
        }
        true
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

    /// Skins one bind-pose vertex with the current joint deltas.
    ///
    /// The weights are renormalised defensively; a vertex with no weight (a
    /// rigid primitive inside a rigged document) keeps its bind position.
    #[allow(clippy::arithmetic_side_effects)] // f32 blend: finite inputs
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
        let name = name.to_ascii_lowercase();
        self.named
            .iter()
            .find(|(candidate, _)| candidate == &name)
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

    /// The current morph weight delta over the model's default, for one target.
    #[must_use]
    pub fn morph_weight_delta(&self, index: usize) -> f32 {
        self.morphs.get(index).copied().unwrap_or(0.0)
            - self.morph_defaults.get(index).copied().unwrap_or(0.0)
    }

    /// Advances an explicit pose cue and returns whether the pose changed.
    ///
    /// A cue change crossfades from the previous clip's current time over the
    /// same time constant the locomotion states use, so a transition starts
    /// from the current pose. One-shot clips hold their last key and report
    /// completion through [`Self::take_cue_finished`].
    #[allow(clippy::arithmetic_side_effects)] // bounded pose arithmetic
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
            crate::logging::warn_once(
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
            state.time += step * rate;
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

    /// Resolves a pose cue to `(clip, once, paused, time rate)`.
    ///
    /// * `Idle` uses the `idle` clip.
    /// * `Walk { speed }` picks `run` when the rig has one and the requested
    ///   speed is at least [`RUN_GAIT_MULTIPLIER`] times the walk clip's
    ///   reference speed; otherwise `walk`. Either clip plays at
    ///   `speed / its_own_reference_speed`, and a clip with no declared
    ///   reference falls back to the engine constants, so the shipped
    ///   Spoonerman keeps its 0.26 m/s walk.
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
                let walk_index = self.clip_index("walk").unwrap_or(0);
                let walk_reference = self
                    .clip_reference_speeds
                    .get(walk_index)
                    .copied()
                    .flatten()
                    .unwrap_or(self.walk_reference_speed);
                if let Some(run_index) = self.clip_index("run")
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
                (
                    walk_index,
                    false,
                    false,
                    (speed / walk_reference).clamp(0.05, 4.0),
                )
            }
            PoseCue::Clip { name, once, paused } => {
                (self.clip_index(name).unwrap_or(0), *once, *paused, 1.0)
            }
            // A scrub cue never advances on its own: `update_cued` eases its
            // time toward the authored fraction instead.
            PoseCue::Scrub { name, .. } => (self.clip_index(name).unwrap_or(0), true, false, 1.0),
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
                    .map(|clip| (clip, previous_time, was_once))
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
        if let Some((previous, previous_time, was_once)) = previous {
            previous.sample(
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
            for (node, (previous, active)) in self
                .pose
                .iter_mut()
                .zip(self.clip_b.iter().zip(self.clip_a.iter()))
            {
                *node = previous.blend(*active, fade);
            }
            for (slot, (previous, active)) in self
                .morphs
                .iter_mut()
                .zip(self.morph_b.iter().zip(self.morph_a.iter()))
            {
                *slot = (*active - *previous).mul_add(fade, *previous);
            }
        } else {
            for (node, active) in self.pose.iter_mut().zip(self.clip_a.iter()) {
                *node = *active;
            }
            self.morphs.copy_from_slice(&self.morph_a);
        }
        if fade >= 0.999 {
            self.cue_from_pose = false;
        }
        true
    }

    /// Writes the procedural locomotion pose from the state weights.
    #[allow(clippy::arithmetic_side_effects)] // f32 pose arithmetic: finite inputs
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
                    * (walk_bob * weights.get(WALKING).copied().unwrap_or(0.0)
                        + idle_bob * weights.get(IDLE).copied().unwrap_or(0.0));
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
        #[allow(clippy::arithmetic_side_effects)]
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
            if let Some(slot) = self.deltas.get_mut(slot) {
                *slot = delta;
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
#[allow(clippy::arithmetic_side_effects)] // f32 pose arithmetic: finite inputs
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
                (10.0 + swing * 3.0) * scale,
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
/// The colour is the pre-baked albedo (light already sampled), so the world
/// shader's vertex-lit path is exact and no lightmap channel is used.
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

#[cfg(test)]
mod tests {
    // Test code: unwrap/expect, indexing, loose casts and permissive
    // arithmetic are idiomatic in tests; the production lints stay enforced
    // everywhere else in the crate.
    #![allow(
        clippy::arithmetic_side_effects,
        clippy::expect_used,
        clippy::float_cmp,
        clippy::indexing_slicing,
        clippy::panic,
        clippy::unwrap_used
    )]

    use super::*;
    use crate::gltf::{AnimationPath, PropAnimationChannel, PropModel, PropNode, PropSkin};

    /// A synthetic three-node rig: a body, a front-left leg and a tail, with
    /// one vertex skinned to all three.
    fn rigged_model(animations: Vec<PropAnimation>) -> PropModel {
        let nodes = vec![
            PropNode {
                name: "root".to_string(),
                parent: None,
                children: vec![1, 2],
                translation: [0.0, 0.0, 0.0],
                rotation: [0.0, 0.0, 0.0, 1.0],
                scale: [1.0; 3],
            },
            PropNode {
                name: "leg_fl_upper".to_string(),
                parent: Some(0),
                children: vec![],
                translation: [0.0, -1.0, 0.0],
                rotation: [0.0, 0.0, 0.0, 1.0],
                scale: [1.0; 3],
            },
            PropNode {
                name: "tail_01".to_string(),
                parent: Some(0),
                children: vec![],
                translation: [0.0, 0.0, -0.5],
                rotation: [0.0, 0.0, 0.0, 1.0],
                scale: [1.0; 3],
            },
        ];
        PropModel {
            vertices: vec![
                crate::gltf::PropVertex {
                    pos: [0.0, 0.0, 0.0],
                    color: [1.0; 4],
                    uv: [0.0, 0.0],
                },
                crate::gltf::PropVertex {
                    pos: [1.0, 0.0, 0.0],
                    color: [1.0; 4],
                    uv: [1.0, 0.0],
                },
                crate::gltf::PropVertex {
                    pos: [0.0, 1.0, 0.0],
                    color: [1.0; 4],
                    uv: [0.0, 1.0],
                },
            ],
            indices: vec![0, 1, 2],
            textures: Vec::new(),
            submeshes: Vec::new(),
            triangles: 1,
            materials: 0,
            skin: Some(PropSkin {
                joints: vec![0, 1, 2],
                inverse_bind: vec![Mat4::IDENTITY; 3],
                nodes: nodes.clone(),
                root: Some(0),
                mesh_node: Some(0),
            }),
            nodes,
            joints: vec![[0, 1, 2, 0]; 3],
            weights: vec![
                [0.4, 0.3, 0.3, 0.0],
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 0.5, 0.5, 0.0],
            ],
            animations,
            morph_targets: Vec::new(),
            morph_weights: Vec::new(),
            mesh_morph_ranges: Vec::new(),
        }
    }

    fn snapshot(state: LocomotionState, speed: f32) -> LocomotionSnapshot {
        LocomotionSnapshot { state, speed }
    }

    #[test]
    fn state_weights_converge_without_resetting() {
        let model = rigged_model(Vec::new());
        let mut animator = CharacterAnimator::new(&model).expect("rig animator");
        assert_eq!(animator.state_weight(LocomotionState::Idle), 1.0);
        animator.update(1.0 / 60.0, snapshot(LocomotionState::Walking, 2.0));
        let mid = animator.state_weight(LocomotionState::Idle);
        assert!(
            mid < 1.0 && mid > 0.0,
            "the weight eases, it does not jump: {mid}"
        );
        // One second at 60 Hz: both weights are effectively settled.
        for _ in 0..60 {
            animator.update(1.0 / 60.0, snapshot(LocomotionState::Walking, 2.0));
        }
        assert!(animator.state_weight(LocomotionState::Walking) > 0.99);
        assert!(animator.state_weight(LocomotionState::Idle) < 0.01);
    }

    #[test]
    fn phase_advances_only_with_speed() {
        let model = rigged_model(Vec::new());
        let mut animator = CharacterAnimator::new(&model).expect("rig animator");
        for _ in 0..30 {
            animator.update(1.0 / 60.0, snapshot(LocomotionState::Walking, 0.0));
        }
        assert_eq!(animator.phase(), 0.0, "a stationary walker does not stride");
        animator.update(0.5, snapshot(LocomotionState::Walking, 2.0));
        let walked = animator.phase();
        assert!(walked > 0.0);
        // Half a second at 2 m/s is one metre, 0.9 of a cycle.
        assert!((walked - 0.9).abs() < 1e-4, "phase {walked}");
        // Swimming advances at the fixed frequency, with no speed input.
        animator.update(1.0, snapshot(LocomotionState::Swimming, 0.0));
        assert!((animator.phase() - (0.9 + SWIM_HZ).rem_euclid(1.0)).abs() < 1e-4);
    }

    #[test]
    fn the_pose_is_frame_rate_independent() {
        let model = rigged_model(Vec::new());
        let mut fast = CharacterAnimator::new(&model).expect("rig animator");
        let mut normal = CharacterAnimator::new(&model).expect("rig animator");
        let mut slow = CharacterAnimator::new(&model).expect("rig animator");
        // One second of walking at 30, 60 and 144 fps.
        for _ in 0..30 {
            fast.update(1.0 / 30.0, snapshot(LocomotionState::Walking, 1.4));
        }
        for _ in 0..60 {
            normal.update(1.0 / 60.0, snapshot(LocomotionState::Walking, 1.4));
        }
        for _ in 0..144 {
            slow.update(1.0 / 144.0, snapshot(LocomotionState::Walking, 1.4));
        }
        for slot in 0..fast.joint_count() {
            let a = fast.joint_delta(slot).expect("delta");
            let b = normal.joint_delta(slot).expect("delta");
            let c = slow.joint_delta(slot).expect("delta");
            for (x, y) in a.to_cols_array().iter().zip(b.to_cols_array().iter()) {
                assert!((x - y).abs() < 1e-3, "slot {slot}: {a:?} vs {b:?}");
            }
            for (x, y) in a.to_cols_array().iter().zip(c.to_cols_array().iter()) {
                assert!((x - y).abs() < 1e-3, "slot {slot}: {a:?} vs {c:?}");
            }
        }
    }

    #[test]
    fn switching_states_keeps_matrices_finite_and_rigid() {
        let model = rigged_model(Vec::new());
        let mut animator = CharacterAnimator::new(&model).expect("rig animator");
        for state in [
            LocomotionState::Walking,
            LocomotionState::Airborne,
            LocomotionState::Swimming,
            LocomotionState::SurfaceSwimming,
            LocomotionState::Idle,
        ] {
            for _ in 0..8 {
                animator.update(1.0 / 60.0, snapshot(state, 1.0));
                for slot in 0..animator.joint_count() {
                    let delta = animator.joint_delta(slot).expect("delta");
                    assert!(delta.to_cols_array().iter().all(|value| value.is_finite()));
                    // Rigid skinning deltas keep an orthonormal rotation; the
                    // synthetic rig has no scale animation.
                    let rotation =
                        Mat4::from_quat(delta.to_scale_rotation_translation().1.normalize());
                    let product = rotation.transpose().mul_mat4(&rotation);
                    for row in 0..3 {
                        for column in 0..3 {
                            let expected = if row == column { 1.0 } else { 0.0 };
                            assert!(
                                (product.col(column)[row] - expected).abs() < 1e-3,
                                "slot {slot} not orthonormal: {product:?}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn a_skinned_vertex_follows_its_joint_delta() {
        let mut model = rigged_model(Vec::new());
        // The leg swings forward by a known amount: its delta must move the
        // vertex it drives.
        model.skin.as_mut().expect("skin").nodes[1].translation = [0.0, -1.0, 0.0];
        let mut animator = CharacterAnimator::new(&model).expect("rig animator");
        let bind = [0.0, 0.0, 0.0];
        let before = animator.skin_position([1, 0, 0, 0], [1.0, 0.0, 0.0, 0.0], bind);
        assert_eq!(before, bind);
        // Drive the walking pose long enough for the leg to swing.
        for _ in 0..20 {
            animator.update(1.0 / 60.0, snapshot(LocomotionState::Walking, 2.0));
        }
        let walked = animator.skin_position([1, 0, 0, 0], [1.0, 0.0, 0.0, 0.0], bind);
        assert!(
            (walked[0] - before[0]).abs() + (walked[2] - before[2]).abs() > 1e-4,
            "the leg delta must move its vertex: {before:?} -> {walked:?}"
        );
        // A zero-weight vertex stays at its bind position.
        let unmoved = animator.skin_position([1, 0, 0, 0], [0.0; 4], [3.0, 4.0, 5.0]);
        assert_eq!(unmoved, [3.0, 4.0, 5.0]);
    }

    #[test]
    fn clip_sampling_interpolates_linearly_and_holds_step_keys() {
        let linear = PropAnimation {
            looped: true,
            reference_speed_mps: None,
            kind: None,
            name: "Walk".to_string(),
            duration: 1.0,
            channels: vec![PropAnimationChannel {
                node: 1,
                path: AnimationPath::Translation,
                interpolation: AnimationInterpolation::Linear,
                times: vec![0.0, 1.0],
                values: vec![0.0, 0.0, 0.0, 0.0, 10.0, 0.0],
                values_per_key: 3,
            }],
        };
        let step = PropAnimation {
            looped: true,
            reference_speed_mps: None,
            kind: None,
            name: "Idle".to_string(),
            duration: 1.0,
            channels: vec![PropAnimationChannel {
                node: 1,
                path: AnimationPath::Translation,
                interpolation: AnimationInterpolation::Step,
                times: vec![0.0, 1.0],
                values: vec![5.0, 0.0, 0.0, 0.0, 0.0, 0.0],
                values_per_key: 3,
            }],
        };
        let model = rigged_model(vec![linear, step]);
        let mut animator = CharacterAnimator::new(&model).expect("clip animator");
        // Sample the linear clip mid-way: the joint delta translates by 5.
        let mut player = animator.clips.take().expect("clips");
        player.active = 0;
        player.previous = None;
        player.elapsed = BLEND_TIME_CONSTANT_S;
        let mut out: Vec<LocalTrs> = animator.rig.nodes.iter().map(LocalTrs::from_node).collect();
        player.clips[0].sample(
            &animator.rig,
            &MorphRig {
                defaults: &[],
                ranges: &[],
            },
            0.5,
            true,
            &mut SampleTargets {
                pose: &mut out,
                morphs: &mut [],
            },
        );
        assert!((out[1].translation.y - 5.0).abs() < 1e-6, "{:?}", out[1]);
        // STEP holds the previous key until the next one arrives.
        player.clips[1].sample(
            &animator.rig,
            &MorphRig {
                defaults: &[],
                ranges: &[],
            },
            0.5,
            true,
            &mut SampleTargets {
                pose: &mut out,
                morphs: &mut [],
            },
        );
        assert!((out[1].translation.x - 5.0).abs() < 1e-6, "{:?}", out[1]);
        // The wrapper wraps time past the duration: 1.5 s is 0.5 s again.
        player.clips[0].sample(
            &animator.rig,
            &MorphRig {
                defaults: &[],
                ranges: &[],
            },
            1.5,
            true,
            &mut SampleTargets {
                pose: &mut out,
                morphs: &mut [],
            },
        );
        assert!((out[1].translation.y - 5.0).abs() < 1e-6, "{:?}", out[1]);
    }

    #[test]
    fn a_clipped_rig_maps_states_to_clips_by_name() {
        let idle = PropAnimation {
            looped: true,
            reference_speed_mps: None,
            kind: None,
            name: "CatIdle".to_string(),
            duration: 1.0,
            channels: Vec::new(),
        };
        let walk = PropAnimation {
            looped: true,
            reference_speed_mps: None,
            kind: None,
            name: "cat_walk_cycle".to_string(),
            duration: 1.0,
            channels: Vec::new(),
        };
        let player = ClipPlayer::new(&[idle, walk], 3).expect("clips");
        assert_eq!(player.state_clip[IDLE], 0);
        assert_eq!(player.state_clip[WALKING], 1);
        assert_eq!(
            player.state_clip[AIRBORNE], 0,
            "an unmapped state falls back to idle"
        );
        assert_eq!(player.state_clip[SWIMMING], 0);
    }

    #[test]
    fn a_state_change_crossfades_between_clips() {
        let constant = |name: &str, y: f32| PropAnimation {
            looped: true,
            reference_speed_mps: None,
            kind: None,
            name: name.to_string(),
            duration: 1.0,
            channels: vec![PropAnimationChannel {
                node: 1,
                path: AnimationPath::Translation,
                interpolation: AnimationInterpolation::Linear,
                times: vec![0.0, 1.0],
                values: vec![0.0, y, 0.0, 0.0, y, 0.0],
                values_per_key: 3,
            }],
        };
        // The leg's rest translation is (0, -1, 0); the idle clip holds it and
        // the walk clip moves it to (0, 9, 0), a 10 m skinning delta.
        let model = rigged_model(vec![constant("Idle", -1.0), constant("Walk", 9.0)]);
        let mut animator = CharacterAnimator::new(&model).expect("clip animator");
        for _ in 0..60 {
            animator.update(1.0 / 60.0, snapshot(LocomotionState::Idle, 0.0));
        }
        let idle_delta = animator
            .joint_delta(1)
            .expect("delta")
            .transform_point3(Vec3::ZERO);
        assert!(idle_delta.y.abs() < 1e-4, "{idle_delta:?}");
        // The first walk frame blends only a fraction in: the pose moves
        // towards the walk clip instead of jumping to it.
        animator.update(1.0 / 60.0, snapshot(LocomotionState::Walking, 1.4));
        let mid_delta = animator
            .joint_delta(1)
            .expect("delta")
            .transform_point3(Vec3::ZERO);
        assert!(
            mid_delta.y > 0.0 && mid_delta.y < 9.0,
            "a crossfade must interpolate: {mid_delta:?}"
        );
        for _ in 0..120 {
            animator.update(1.0 / 60.0, snapshot(LocomotionState::Walking, 1.4));
        }
        let final_delta = animator
            .joint_delta(1)
            .expect("delta")
            .transform_point3(Vec3::ZERO);
        assert!((final_delta.y - 10.0).abs() < 0.01, "{final_delta:?}");
    }

    #[test]
    fn updating_never_reallocates_the_pose_buffers() {
        let model = rigged_model(Vec::new());
        let mut animator = CharacterAnimator::new(&model).expect("rig animator");
        let before = animator.allocation_probe();
        for frame in 0..240 {
            let state = if frame % 3 == 0 {
                LocomotionState::Walking
            } else {
                LocomotionState::Idle
            };
            animator.update(1.0 / 60.0, snapshot(state, 1.0));
        }
        assert_eq!(animator.allocation_probe(), before);
    }

    #[test]
    fn an_unclassifiable_rig_stays_at_rest() {
        let mut model = rigged_model(Vec::new());
        if let Some(skin) = model.skin.as_mut() {
            for node in &mut skin.nodes {
                node.name = "unnamed".to_string();
            }
        }
        let mut animator = CharacterAnimator::new(&model).expect("rig animator");
        for _ in 0..120 {
            animator.update(1.0 / 60.0, snapshot(LocomotionState::Walking, 3.0));
        }
        for slot in 0..animator.joint_count() {
            let delta = animator.joint_delta(slot).expect("delta");
            for value in delta.to_cols_array() {
                assert!((value - if value == 1.0 { 1.0 } else { 0.0 }).abs() < 1e-6);
            }
        }
    }

    /// CUBICSPLINE evaluates the spec's Hermite form, including the tangent
    /// terms, and clamps outside the key range.
    #[test]
    fn cubic_spline_sampling_hits_endpoints_tangents_and_midpoints() {
        let channel = PropAnimationChannel {
            node: 1,
            path: AnimationPath::Translation,
            interpolation: AnimationInterpolation::CubicSpline,
            times: vec![0.0, 1.0],
            // Per key: in-tangent, value, out-tangent.
            // key 0: v0 = (0,0,0), out m0 = (2,0,0); key 1: v1 = (10,0,0)
            values: vec![
                0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 10.0, 0.0, 0.0, 0.0,
                0.0, 0.0,
            ],
            values_per_key: 3,
        };
        let sampled = Channel::from_channel(&channel);
        // Hermite: h00*0 + h10*1*2 + h01*10 + h11*1*0 = 5 + 0.25 at t = 0.5.
        let mid = sampled.sample_component(0.5, 0).expect("mid sample");
        assert!((mid - 5.25).abs() < 1e-4, "cubic midpoint: {mid}");
        assert!((sampled.sample_component(0.0, 0).expect("start") - 0.0).abs() < 1e-6);
        assert!((sampled.sample_component(1.0, 0).expect("end") - 10.0).abs() < 1e-6);
        // Timestamps outside the key range clamp to the nearest key.
        assert!((sampled.sample_component(-3.0, 0).expect("before") - 0.0).abs() < 1e-6);
        assert!((sampled.sample_component(9.0, 0).expect("after") - 10.0).abs() < 1e-6);
    }

    /// An explicit one-shot cue holds its last pose and reports completion
    /// exactly once; a paused cue freezes its time and pose.
    #[test]
    fn explicit_clips_hold_once_pause_and_resume() {
        let clip = PropAnimation {
            looped: true,
            reference_speed_mps: None,
            kind: None,
            name: "sit_down".to_string(),
            duration: 1.0,
            channels: vec![PropAnimationChannel {
                node: 1,
                path: AnimationPath::Translation,
                interpolation: AnimationInterpolation::Linear,
                times: vec![0.0, 1.0],
                values: vec![0.0, 0.0, 0.0, 0.0, 5.0, 0.0],
                values_per_key: 3,
            }],
        };
        let model = rigged_model(vec![clip]);
        let mut animator = CharacterAnimator::new(&model).expect("cue animator");
        let cue = PoseCue::Clip {
            name: "sit_down".to_string(),
            once: true,
            paused: false,
        };
        animator.update_cued(0.5, &cue);
        assert!(!animator.take_cue_finished(), "not finished mid-clip");
        animator.update_cued(0.6, &cue);
        assert!(animator.take_cue_finished(), "finished at the last key");
        assert!(!animator.take_cue_finished(), "completion is consumed once");
        // The held pose is the clip's final translation.
        let held = animator
            .joint_delta(1)
            .expect("delta")
            .transform_point3(Vec3::ZERO);
        // The delta is the animated translation (5) minus the rest (-1).
        assert!((held.y - 6.0).abs() < 1e-3, "held pose: {held:?}");
        // A paused cue keeps its time: once the crossfade has settled, further
        // steps leave the pose untouched.
        let paused = PoseCue::Clip {
            name: "sit_down".to_string(),
            once: false,
            paused: true,
        };
        for _ in 0..60 {
            animator.update_cued(1.0 / 30.0, &paused);
        }
        let first = animator
            .joint_delta(1)
            .expect("delta")
            .transform_point3(Vec3::ZERO);
        animator.update_cued(2.0, &paused);
        let second = animator
            .joint_delta(1)
            .expect("delta")
            .transform_point3(Vec3::ZERO);
        assert!(
            (first - second).length() < 1e-6,
            "a paused cue must not advance: {first:?} vs {second:?}"
        );
        assert!(animator.has_pose_cue());
    }

    /// A `weights` animation channel drives the model's morph target weights,
    /// and the default weight is restored before every sample.
    #[test]
    fn morph_weight_channels_animate_the_target_weights() {
        let mut model = rigged_model(vec![PropAnimation {
            looped: true,
            reference_speed_mps: None,
            kind: None,
            name: "wave".to_string(),
            duration: 1.0,
            channels: vec![PropAnimationChannel {
                node: 0,
                path: AnimationPath::Weights,
                interpolation: AnimationInterpolation::Linear,
                times: vec![0.0, 1.0],
                values: vec![0.0, 1.0],
                values_per_key: 1,
            }],
        }]);
        model.morph_targets = vec![crate::gltf::PropMorphTarget {
            position: vec![[0.0, 0.25, 0.0]; 3],
            normal: Vec::new(),
            tangent: Vec::new(),
        }];
        model.morph_weights = vec![0.0];
        model.mesh_morph_ranges = vec![(0, 0, 1)];
        let animator = CharacterAnimator::new(&model).expect("morph animator");
        let clips = animator.clips.as_ref().expect("clips");
        let mut out: Vec<LocalTrs> = animator.rig.nodes.iter().map(LocalTrs::from_node).collect();
        let mut morphs = vec![0.0f32];
        clips.clips[0].sample(
            &animator.rig,
            &MorphRig {
                defaults: &model.morph_weights,
                ranges: &model.mesh_morph_ranges,
            },
            0.5,
            true,
            &mut SampleTargets {
                pose: &mut out,
                morphs: &mut morphs,
            },
        );
        assert!(
            (morphs[0] - 0.5).abs() < 1e-5,
            "half-way weight: {morphs:?}"
        );
        clips.clips[0].sample(
            &animator.rig,
            &MorphRig {
                defaults: &model.morph_weights,
                ranges: &model.mesh_morph_ranges,
            },
            1.0,
            false,
            &mut SampleTargets {
                pose: &mut out,
                morphs: &mut morphs,
            },
        );
        assert!((morphs[0] - 1.0).abs() < 1e-5, "end weight: {morphs:?}");
        // A looping sample wraps: 1.25 s is 0.25 s again.
        clips.clips[0].sample(
            &animator.rig,
            &MorphRig {
                defaults: &model.morph_weights,
                ranges: &model.mesh_morph_ranges,
            },
            1.25,
            true,
            &mut SampleTargets {
                pose: &mut out,
                morphs: &mut morphs,
            },
        );
        assert!(
            (morphs[0] - 0.25).abs() < 1e-5,
            "wrapped weight: {morphs:?}"
        );
    }

    /// Switching from a held one-shot to a loop starts the crossfade from the
    /// held last pose, not from the loop's first frame.
    #[test]
    fn a_one_shot_holds_its_last_pose_while_the_next_cue_fades_in() {
        let sit = PropAnimation {
            looped: true,
            reference_speed_mps: None,
            kind: None,
            name: "sit_down".to_string(),
            duration: 1.0,
            channels: vec![PropAnimationChannel {
                node: 1,
                path: AnimationPath::Translation,
                interpolation: AnimationInterpolation::Linear,
                times: vec![0.0, 1.0],
                values: vec![0.0, -1.0, 0.0, 0.0, 4.0, 0.0],
                values_per_key: 3,
            }],
        };
        let idle = PropAnimation {
            looped: true,
            reference_speed_mps: None,
            kind: None,
            name: "idle".to_string(),
            duration: 2.0,
            channels: vec![PropAnimationChannel {
                node: 1,
                path: AnimationPath::Translation,
                interpolation: AnimationInterpolation::Linear,
                times: vec![0.0, 1.0],
                values: vec![0.0, -1.0, 0.0, 0.0, -1.0, 0.0],
                values_per_key: 3,
            }],
        };
        let model = rigged_model(vec![sit, idle]);
        let mut animator = CharacterAnimator::new(&model).expect("cue animator");
        let once = PoseCue::Clip {
            name: "sit_down".to_string(),
            once: true,
            paused: false,
        };
        animator.update_cued(2.0, &once);
        assert!(animator.take_cue_finished());
        let held = animator
            .joint_delta(1)
            .expect("delta")
            .transform_point3(Vec3::ZERO);
        assert!((held.y - 5.0).abs() < 1e-3, "held pose: {held:?}");
        // The first frame of the loop cue keeps the held pose (fade from the
        // current pose, no frame-0 snap).
        animator.update_cued(0.0, &PoseCue::Idle);
        let frozen = animator
            .joint_delta(1)
            .expect("delta")
            .transform_point3(Vec3::ZERO);
        assert!(
            (frozen - held).length() < 1e-4,
            "no frame-0 jump: {frozen:?} vs {held:?}"
        );
        // After the fade the idle pose is reached.
        for _ in 0..120 {
            animator.update_cued(1.0 / 60.0, &PoseCue::Idle);
        }
        let settled = animator
            .joint_delta(1)
            .expect("delta")
            .transform_point3(Vec3::ZERO);
        assert!(settled.length() < 1e-3, "settled idle pose: {settled:?}");
    }

    /// Completion never leaks across a cue change, and `restart_cue` replays a
    /// finished one-shot.
    #[test]
    fn cue_completion_does_not_leak_and_can_restart() {
        let clip = PropAnimation {
            looped: true,
            reference_speed_mps: None,
            kind: None,
            name: "sit_down".to_string(),
            duration: 1.0,
            channels: vec![PropAnimationChannel {
                node: 1,
                path: AnimationPath::Translation,
                interpolation: AnimationInterpolation::Linear,
                times: vec![0.0, 1.0],
                values: vec![0.0, -1.0, 0.0, 0.0, 4.0, 0.0],
                values_per_key: 3,
            }],
        };
        let idle = PropAnimation {
            looped: true,
            reference_speed_mps: None,
            kind: None,
            name: "idle".to_string(),
            duration: 2.0,
            channels: Vec::new(),
        };
        let model = rigged_model(vec![clip, idle]);
        let mut animator = CharacterAnimator::new(&model).expect("cue animator");
        let once = PoseCue::Clip {
            name: "sit_down".to_string(),
            once: true,
            paused: false,
        };
        animator.update_cued(2.0, &once);
        // Finish without consuming, then change cues: the stale flag is gone.
        animator.update_cued(0.0, &PoseCue::Idle);
        assert!(!animator.take_cue_finished(), "completion must not leak");
        // Replay the same one-shot through the explicit restart hook.
        animator.update_cued(0.0, &once);
        animator.restart_cue();
        animator.update_cued(0.5, &once);
        assert!(!animator.take_cue_finished(), "restarted clip is mid-way");
        animator.update_cued(1.0, &once);
        assert!(
            animator.take_cue_finished(),
            "restarted clip completes again"
        );
    }

    /// LINEAR rotation keys slerp along the shortest arc instead of
    /// component-wise lerping the long way round.
    #[test]
    fn linear_rotation_keys_take_the_shortest_arc() {
        let channel = PropAnimationChannel {
            node: 1,
            path: AnimationPath::Rotation,
            interpolation: AnimationInterpolation::Linear,
            times: vec![0.0, 1.0],
            // 270 degrees about +X: (sin 135, 0, 0, cos 135).
            values: vec![
                0.0,
                0.0,
                0.0,
                1.0,
                std::f32::consts::FRAC_1_SQRT_2,
                0.0,
                0.0,
                -std::f32::consts::FRAC_1_SQRT_2,
            ],
            values_per_key: 4,
        };
        let sampled = Channel::from_channel(&channel);
        let mid = sampled.sample_rotation(0.5).expect("mid rotation");
        // The shortest arc from 0 to 270 degrees is -90 degrees, so the
        // midpoint is -45 degrees about +X: x = sin(-22.5 degrees).
        assert!(
            (mid.x - (-0.382_683_43)).abs() < 1e-3 && mid.w > 0.92,
            "shortest-arc slerp: {mid:?}"
        );
    }

    /// One clip that keys a single node, for cue-resolution tests.
    fn named_clip(name: &str, reference_speed_mps: Option<f32>) -> PropAnimation {
        PropAnimation {
            name: name.to_string(),
            duration: 1.0,
            channels: vec![PropAnimationChannel {
                node: 1,
                path: AnimationPath::Rotation,
                interpolation: AnimationInterpolation::Linear,
                times: vec![0.0, 1.0],
                values: vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
                values_per_key: 4,
            }],
            looped: true,
            reference_speed_mps,
            kind: None,
        }
    }

    /// A walk cue follows the clips' own declared reference speeds: the run
    /// clip takes over above the gait threshold and each clip plays at
    /// `speed / reference` with no foot sliding.
    #[test]
    fn a_walk_cue_uses_declared_reference_speeds_and_prefers_run() {
        let model = rigged_model(vec![
            named_clip("walk", Some(0.35)),
            named_clip("run", Some(1.2)),
            named_clip("idle", None),
        ]);
        let animator = CharacterAnimator::new(&model).expect("rig animator");
        let walk = animator.clip_index("walk").expect("walk clip");
        let run = animator.clip_index("run").expect("run clip");

        let (clip, once, paused, rate) = animator.resolve_cue(&PoseCue::Walk { speed_mps: 0.35 });
        assert_eq!((clip, once, paused), (walk, false, false));
        assert!((rate - 1.0).abs() < 1e-5, "walk at its reference: {rate}");

        let (clip, _, _, rate) = animator.resolve_cue(&PoseCue::Walk { speed_mps: 0.6 });
        assert_eq!(
            clip, run,
            "above 1.5x the walk reference, the run gait plays"
        );
        assert!(
            (rate - 0.5).abs() < 1e-5,
            "run at half its reference: {rate}"
        );

        let (clip, _, _, rate) = animator.resolve_cue(&PoseCue::Walk { speed_mps: 1.2 });
        assert_eq!(clip, run);
        assert!((rate - 1.0).abs() < 1e-5, "run at its reference: {rate}");

        // A rig with no run clip keeps the walk clip at any speed.
        let walk_only = rigged_model(vec![named_clip("walk", Some(0.35))]);
        let animator = CharacterAnimator::new(&walk_only).expect("rig animator");
        let (clip, _, _, rate) = animator.resolve_cue(&PoseCue::Walk { speed_mps: 1.4 });
        assert_eq!(clip, 0);
        assert!((rate - 4.0).abs() < 1e-5, "the rate clamps at 4x: {rate}");

        // A clip with no declared reference keeps the historical constant.
        let legacy = rigged_model(vec![named_clip("walk", None)]);
        let animator = CharacterAnimator::new(&legacy).expect("rig animator");
        let (_, _, _, rate) = animator.resolve_cue(&PoseCue::Walk {
            speed_mps: WALK_REFERENCE_SPEED_MPS,
        });
        assert!((rate - 1.0).abs() < 1e-5, "legacy walk reference: {rate}");
    }
}
