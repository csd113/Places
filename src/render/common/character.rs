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
use crate::gltf::{
    AnimationInterpolation, PropAnimation, PropAnimationChannel, PropNode, PropSkin,
};
use crate::level::LevelSurfaces;
use crate::lighting::LevelLighting;
use crate::props::{LoadedPropAsset, PropAssets};
use crate::spatial::Aabb;

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

    /// Samples the clip at local time `time` into per-node local poses that
    /// start from the rest pose.
    fn sample(&self, rig: &Rig, time: f32, out: &mut [LocalTrs]) {
        for (node, pose) in rig.nodes.iter().enumerate() {
            let Some(slot) = out.get_mut(node) else {
                continue;
            };
            *slot = LocalTrs::from_node(pose);
        }
        let local_time = if self.duration > 0.0 {
            time.rem_euclid(self.duration)
        } else {
            0.0
        };
        for (node, node_channels) in self.channels.iter().enumerate() {
            let Some(slot) = out.get_mut(node) else {
                continue;
            };
            for channel in node_channels {
                channel.apply(local_time, slot);
            }
        }
    }
}

impl Channel {
    fn from_channel(channel: &PropAnimationChannel) -> Self {
        Self {
            path: channel.path,
            interpolation: channel.interpolation,
            times: channel.times.clone(),
            values: channel.values.clone(),
        }
    }

    /// Samples one key at `time` and writes it into the node's local pose.
    fn apply(&self, time: f32, pose: &mut LocalTrs) {
        let stride = self.path.stride();
        let Some((index, blend)) = self.key_at(time) else {
            return;
        };
        let Some(first) = self.values.get(index.saturating_mul(stride)..) else {
            return;
        };
        let second = if blend > 0.0 {
            let start = index.saturating_add(1).saturating_mul(stride);
            self.values.get(start..)
        } else {
            None
        };
        match self.path {
            crate::gltf::AnimationPath::Translation => {
                let a = vec3_at(first);
                let b = second.map_or(a, vec3_at);
                pose.translation = a.lerp(b, blend);
            }
            crate::gltf::AnimationPath::Scale => {
                let a = vec3_at(first);
                let b = second.map_or(a, vec3_at);
                pose.scale = a.lerp(b, blend);
            }
            crate::gltf::AnimationPath::Rotation => {
                let a = quat_at(first);
                let b = second.map_or(a, quat_at);
                pose.rotation = a.slerp(b, blend).normalize();
            }
        }
    }

    /// The key index at `time` and the interpolation factor towards the next
    /// key (always 0 for a STEP channel or the last key).
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
            AnimationInterpolation::Linear => {
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

/// Reads a `vec3` value at the start of a channel's value slice.
fn vec3_at(values: &[f32]) -> Vec3 {
    Vec3::new(
        values.first().copied().unwrap_or(0.0),
        values.get(1).copied().unwrap_or(0.0),
        values.get(2).copied().unwrap_or(0.0),
    )
}

/// Reads a `vec4` value at the start of a channel's value slice.
fn quat_at(values: &[f32]) -> Quat {
    Quat::from_xyzw(
        values.first().copied().unwrap_or(0.0),
        values.get(1).copied().unwrap_or(0.0),
        values.get(2).copied().unwrap_or(0.0),
        values.get(3).copied().unwrap_or(1.0),
    )
}

impl Rig {
    /// Builds the retained rig from one model's skin.
    fn new(skin: &PropSkin) -> Option<Self> {
        let node_count = skin.nodes.len();
        if node_count == 0 {
            return None;
        }
        let parent: Vec<Option<u16>> = skin.nodes.iter().map(|node| node.parent).collect();
        let order = topological_order(&parent)?;
        let mut rest_global: Vec<Mat4> = vec![Mat4::IDENTITY; node_count];
        for node in &order {
            let index = usize::from(*node);
            let local = skin.nodes.get(index).map(PropNode::local_transform)?;
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
        let mut joint_rest_inverse: Vec<Mat4> = Vec::with_capacity(skin.joints.len());
        for joint in &skin.joints {
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
        for (index, node) in skin.nodes.iter().enumerate() {
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
        // skin's root, whose parent is by definition the outermost node, so the
        // character's up axis is its parent-space Y.
        if !pose.iter().any(|node| node.body)
            && let Some(root) = skin.root
            && let Some(slot) = pose.get_mut(usize::from(root))
        {
            slot.body = true;
            slot.up_in_parent = Vec3::Y;
        }
        let mesh_node = skin.mesh_node;
        Some(Self {
            nodes: skin.nodes.clone(),
            order,
            parent,
            rest_global,
            joint_rest_inverse,
            joint_nodes: skin.joints.clone(),
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

    /// The pose evaluator.
    #[must_use]
    pub const fn animator(&self) -> &CharacterAnimator {
        &self.animator
    }

    /// Mutable access to the pose evaluator.
    pub const fn animator_mut(&mut self) -> &mut CharacterAnimator {
        &mut self.animator
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
        let mut characters: Vec<Character> = Vec::new();
        // Insertion-ordered placement counts per model path, so
        // `claimed_models` is deterministic.
        let mut order: Vec<String> = Vec::new();
        let mut placements: HashMap<String, (usize, usize)> = HashMap::new();
        let mut overflow_reported = false;
        for prop in &level.props {
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
            if !asset.model.is_skinned() {
                continue;
            }
            let Some(animator) = CharacterAnimator::new(&asset.model) else {
                continue;
            };
            let base_y = surfaces.floor_y_at(prop.x, prop.z).unwrap_or(0.0);
            let transform = prop_instance_matrix(prop, base_y);
            let albedo = sample_albedo(&asset, &transform, lighting);
            let world_bounds = character_bounds(&asset, &transform);
            characters.push(Character {
                asset,
                transform,
                animator,
                albedo,
                world_bounds,
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
    pub fn update(&mut self, delta_seconds: f32, snapshot: LocomotionSnapshot) -> CharacterUpdate {
        let mut moved = 0usize;
        for character in &mut self.characters {
            if character.animator.update(delta_seconds, snapshot) {
                moved = moved.saturating_add(1);
            }
        }
        CharacterUpdate { moved }
    }
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
    /// Blend weight per locomotion state; the current state approaches one and
    /// the others approach zero.
    weights: [f32; STATE_COUNT],
    /// Gait phase in cycles: walking advances per metre travelled, swimming
    /// per second.
    phase: f32,
    /// Free-running seconds, for idle sway and breathing.
    clock: f32,
    clips: Option<ClipPlayer>,
    /// Current local pose per node.
    pose: Vec<LocalTrs>,
    /// Current global transform per node.
    node_globals: Vec<Mat4>,
    /// Current model-space skinning delta per joint slot.
    deltas: Vec<Mat4>,
    /// Crossfade scratch per node, only allocated for a rig with clips.
    clip_a: Vec<LocalTrs>,
    clip_b: Vec<LocalTrs>,
    revision: u64,
}

impl CharacterAnimator {
    /// Builds an animator for a skinned model, or `None` when it has no skin.
    #[must_use]
    pub fn new(model: &crate::gltf::PropModel) -> Option<Self> {
        let skin = model.skin.as_ref()?;
        let rig = Rig::new(skin)?;
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
        Some(Self {
            rig,
            weights,
            phase: 0.0,
            clock: 0.0,
            clips,
            pose,
            node_globals: vec![Mat4::IDENTITY; node_count],
            deltas,
            clip_a,
            clip_b,
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
        if let Some(clips) = self.clips.as_mut() {
            let fade = clips.advance(step, state);
            let active_index = clips.active;
            let previous_index = clips.previous;
            let Some(active) = clips.clips.get(active_index) else {
                return false;
            };
            if self.rig.nodes.is_empty() {
                return false;
            }
            active.sample(&self.rig, self.clock, &mut self.clip_a);
            let fade = match previous_index.and_then(|index| clips.clips.get(index)) {
                Some(previous) => {
                    previous.sample(&self.rig, self.clock, &mut self.clip_b);
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
            } else {
                for (node, active) in self.pose.iter_mut().zip(self.clip_a.iter()) {
                    *node = *active;
                }
            }
        } else {
            self.apply_procedural_pose();
        }
        self.compose_globals();
        self.compute_deltas();
        self.revision = self.revision.wrapping_add(1);
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
    use crate::gltf::{AnimationPath, PropModel, PropNode, PropSkin};

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
                nodes,
                root: Some(0),
                mesh_node: Some(0),
            }),
            joints: vec![[0, 1, 2, 0]; 3],
            weights: vec![
                [0.4, 0.3, 0.3, 0.0],
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 0.5, 0.5, 0.0],
            ],
            animations,
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
            name: "Walk".to_string(),
            duration: 1.0,
            channels: vec![PropAnimationChannel {
                node: 1,
                path: AnimationPath::Translation,
                interpolation: AnimationInterpolation::Linear,
                times: vec![0.0, 1.0],
                values: vec![0.0, 0.0, 0.0, 0.0, 10.0, 0.0],
            }],
        };
        let step = PropAnimation {
            name: "Idle".to_string(),
            duration: 1.0,
            channels: vec![PropAnimationChannel {
                node: 1,
                path: AnimationPath::Translation,
                interpolation: AnimationInterpolation::Step,
                times: vec![0.0, 1.0],
                values: vec![5.0, 0.0, 0.0, 0.0, 0.0, 0.0],
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
        player.clips[0].sample(&animator.rig, 0.5, &mut out);
        assert!((out[1].translation.y - 5.0).abs() < 1e-6, "{:?}", out[1]);
        // STEP holds the previous key until the next one arrives.
        player.clips[1].sample(&animator.rig, 0.5, &mut out);
        assert!((out[1].translation.x - 5.0).abs() < 1e-6, "{:?}", out[1]);
        // The wrapper wraps time past the duration: 1.5 s is 0.5 s again.
        player.clips[0].sample(&animator.rig, 1.5, &mut out);
        assert!((out[1].translation.y - 5.0).abs() < 1e-6, "{:?}", out[1]);
    }

    #[test]
    fn a_clipped_rig_maps_states_to_clips_by_name() {
        let idle = PropAnimation {
            name: "CatIdle".to_string(),
            duration: 1.0,
            channels: Vec::new(),
        };
        let walk = PropAnimation {
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
            name: name.to_string(),
            duration: 1.0,
            channels: vec![PropAnimationChannel {
                node: 1,
                path: AnimationPath::Translation,
                interpolation: AnimationInterpolation::Linear,
                times: vec![0.0, 1.0],
                values: vec![0.0, y, 0.0, 0.0, y, 0.0],
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
}
