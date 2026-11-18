//! Dependency-free GLB (binary glTF 2.0) reader for prop models.
//!
//! The prop pipeline in `tools/props` emits one deliberately boring GLB
//! profile, and production prop files come from modelling tools that split a
//! model per material and nest transform groups. This reader accepts the
//! production-friendly subset Places genuinely needs (see `docs/assets/README.md`
//! for the asset rules) without becoming a general glTF engine:
//!
//! * GLB container, glTF 2.0, the scene graph walked from `scene` (or scene
//!   0, or the parentless nodes when no scenes exist), with node TRS or
//!   matrix transforms composed down the hierarchy;
//! * any number of nodes, meshes, primitives and materials up to the engine
//!   ceilings in [`crate::engine::level`], each primitive keeping its own material;
//! * `POSITION` (float32), `TEXCOORD_0` (float32 or normalised integer),
//!   `NORMAL` (optional float32, retained in model space),
//!   `COLOR_0` (optional; float32 or normalised integer), 16/32-bit indices;
//! * `mode: 4` (triangles) only;
//! * optional morph targets (`POSITION` required, `NORMAL`/`TANGENT`
//!   optional) with their default weights; the bind-pose bake includes the
//!   defaults and the runtime can animate the weights;
//! * one skin per model: `JOINTS_0` (8/16-bit) and `WEIGHTS_0` (float32 or
//!   normalised 8/16-bit) per vertex, a retained node hierarchy, the joint
//!   list and the inverse bind matrices. A skinned primitive's `vertices`
//!   positions are baked to the bind pose, so the static prop path draws the
//!   rest pose unchanged; the raw skin data stays on the model for the
//!   character path;
//! * `animations` retained as named clips with per-node translation/rotation/
//!   scale channels and morph-weight channels; STEP, LINEAR and CUBICSPLINE
//!   samplers are all sampled by the character path;
//! * up to [`crate::engine::level::MAX_PROP_IMAGES`] PNG images embedded in
//!   bufferViews (self-contained, no external files or data URIs), each
//!   distinct image decoded once for the model;
//! * `pbrMetallicRoughness.baseColorFactor`, `emissiveFactor` and the
//!   `KHR_materials_emissive_strength` extension - the only glTF extension
//!   this reader understands.
//!
//! Everything else - sparse accessors, external or data-URI images, per-node
//! morph-weight overrides, a second used skin, texture transforms and every
//! extension except `KHR_materials_emissive_strength` - produces a descriptive
//! [`GltfError`] so a malformed asset degrades into the loader's placeholder
//! box instead of panicking or looping. See the module tests for the exact
//! supported/unsupported feature table.

use std::sync::Arc;

use glam::{Mat4, Quat, Vec3};

use crate::engine::level::MAX_PROP_TRIANGLES;
use crate::engine::materials::{MaterialAlpha, MaterialEmission, RawImage};

mod accessor;
mod animation;
mod mesh;
mod parse;

#[cfg(test)]
#[path = "../tests/gltf_loading.rs"]
mod tests;

/// Conservative encoded model ceiling, checked before file reads and parsing.
/// Geometry, image and animation policies still apply inside this envelope.
pub(crate) const MAX_ENCODED_GLB_BYTES: usize = 64 * 1024 * 1024;

/// The one glTF extension this reader understands.
const KHR_MATERIALS_EMISSIVE_STRENGTH: &str = "KHR_materials_emissive_strength";

/// A parse failure with a message meant for a developer reading the log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GltfError(pub String);

impl std::fmt::Display for GltfError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for GltfError {}

impl GltfError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

/// Converts a validated model index/count to its compact record slot.
fn model_slot(value: usize, description: &str) -> Result<u16, GltfError> {
    u16::try_from(value)
        .map_err(|error| GltfError::new(format!("{description} does not fit in 16 bits: {error}")))
}

/// One vertex of a loaded prop model: position, diffuse tint, UV and optional authored normal.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PropVertex {
    pub pos: [f32; 3],
    /// Unit normal in model space; absent when the asset supplies no NORMAL attribute.
    pub normal: Option<[f32; 3]>,
    pub color: [f32; 4],
    pub uv: [f32; 2],
}

/// One primitive's slice of the model: a material assignment and an index range.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PropSubmesh {
    /// glTF material sidedness. Omitted `doubleSided` defaults to false.
    /// Retained in memory for entity raster state; prepared prop records stay unchanged.
    pub double_sided: bool,
    /// Shared scalar material response for every mesh route.
    pub response: crate::engine::materials::MaterialResponse,
    /// Index into the document's material list (stable, even for skipped materials).
    pub material: u16,
    /// Index into [`PropModel::textures`], or `None` for a material with no texture.
    pub texture: Option<u16>,
    /// Emission of the primitive's material.
    pub emission: MaterialEmission,
    /// Alpha contract of the primitive's material.
    ///
    /// A glTF `alphaMode: "MASK"` material is [`AlphaMode::Cutout`](crate::engine::materials::AlphaMode::Cutout) with its
    /// `alphaCutoff` and draws through the alpha-tested cutout pass; a
    /// `"BLEND"` material is [`AlphaMode::Blend`](crate::engine::materials::AlphaMode::Blend) (its `baseColorFactor` alpha
    /// already lives in the vertex colours) and draws through the sorted
    /// depth-write-disabled translucent pass for static props, characters and dynamic objects. `"OPAQUE"` and an omitted mode stay
    /// [`AlphaMode::Opaque`](crate::engine::materials::AlphaMode::Opaque).
    pub alpha: MaterialAlpha,
    /// First index into [`PropModel::indices`].
    pub first_index: u32,
    /// Number of indices (a multiple of three).
    pub index_count: u32,
}

/// One node of a skinned or animated model's retained hierarchy.
///
/// Unskinned static models are still flattened into world-space vertices at
/// load time and carry no node list; a model that declares a skin or
/// animations keeps its hierarchy so the character path can pose it. `parent`
/// is the first node that lists this one as a child (glTF skins are trees in
/// practice; a DAG's extra parents are ignored by the retained view).
#[derive(Clone, Debug, PartialEq)]
pub struct PropNode {
    pub name: String,
    pub parent: Option<u16>,
    /// Child node indices, in declaration order.
    pub children: Vec<u16>,
    pub translation: [f32; 3],
    /// `xyzw` quaternion.
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
}

impl PropNode {
    /// The node's local transform from its rest TRS.
    #[must_use]
    pub fn local_transform(&self) -> Mat4 {
        let rotation = Quat::from_xyzw(
            self.rotation[0],
            self.rotation[1],
            self.rotation[2],
            self.rotation[3],
        );
        Mat4::from_scale_rotation_translation(
            Vec3::from(self.scale),
            rotation,
            Vec3::from(self.translation),
        )
    }
}

/// Unmodified source coordinates for topology/normal grouping. Rest skinning
/// arithmetic can move seam duplicates across a micron quantization boundary.
#[derive(Clone, Copy, Debug)]
pub struct SourceVertex {
    pub position: [f32; 3],
    pub normal: Option<[f32; 3]>,
}

/// The retained skeleton of a skinned model.
#[derive(Clone, Debug)]
pub struct PropSkin {
    pub source_vertices: Vec<SourceVertex>,
    /// Joint node indices, in skin order. A vertex's `JOINTS_0` values are
    /// slots into this list, not node indices.
    pub joints: Vec<u16>,
    /// Inverse bind matrix per joint slot, column-major, parallel to
    /// [`Self::joints`].
    pub inverse_bind: Vec<Mat4>,
    /// Every node of the document, indexed by node index.
    pub nodes: Vec<PropNode>,
    /// The skin's skeleton root node (`skeleton` when authored, else the
    /// topmost ancestor of the first joint).
    pub root: Option<u16>,
    /// The node that carries the skinned mesh. The renderer's skinning
    /// transform is `meshNodeInverseGlobal * jointGlobal * inverseBind`; the
    /// mesh node is the identity transform in most exports, but a general rig
    /// may animate it.
    pub mesh_node: Option<u16>,
}

/// Which component of a node's transform an animation channel drives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnimationPath {
    Translation,
    Rotation,
    Scale,
    /// The model's morph-target weights, in target order.
    Weights,
}

impl AnimationPath {
    /// Values per keyframe for a transform path: 3 for translation/scale,
    /// 4 for rotation. Morph weights carry one value per morph target, so
    /// their width comes from the channel, not from the path.
    #[must_use]
    pub const fn stride(self) -> usize {
        match self {
            Self::Translation | Self::Scale => 3,
            Self::Rotation => 4,
            Self::Weights => 1,
        }
    }
}

/// How an animation channel's keys interpolate between samples.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnimationInterpolation {
    Linear,
    Step,
    /// Cubic Hermite between keys: each keyframe stores an in-tangent, the
    /// value and an out-tangent (glTF CUBICSPLINE).
    CubicSpline,
}

/// One sampled animation channel: a node, a transform path and its keys.
#[derive(Clone, Debug)]
pub struct PropAnimationChannel {
    /// Target node index.
    pub node: u16,
    pub path: AnimationPath,
    pub interpolation: AnimationInterpolation,
    /// Keyframe times, strictly increasing and finite.
    pub times: Vec<f32>,
    /// Keyframe values, `times.len() * values_per_key` floats. For
    /// CUBICSPLINE every key contributes three tuples (in-tangent, value,
    /// out-tangent), so this is `times.len() * values_per_key * 3`.
    pub values: Vec<f32>,
    /// Values per key for a transform path (translation/scale 3, rotation 4)
    /// or the morph-target count for [`AnimationPath::Weights`].
    pub values_per_key: usize,
}

impl PropAnimationChannel {
    /// This channel's last keyframe time, or zero for an empty channel.
    #[must_use]
    pub fn duration(&self) -> f32 {
        self.times.last().copied().unwrap_or(0.0)
    }

    /// Value tuples stored per keyframe: three for a CUBICSPLINE channel,
    /// one otherwise.
    #[must_use]
    pub const fn tuples_per_key(&self) -> usize {
        match self.interpolation {
            AnimationInterpolation::CubicSpline => 3,
            AnimationInterpolation::Linear | AnimationInterpolation::Step => 1,
        }
    }

    /// The keyframe's stored value tuple offset, in floats.
    #[must_use]
    pub const fn value_offset(&self, key: usize) -> usize {
        key.saturating_mul(self.values_per_key)
            .saturating_mul(self.tuples_per_key())
    }
}

/// One named animation clip.
#[derive(Clone, Debug)]
pub struct PropAnimation {
    pub name: String,
    /// The clip's last keyframe time, in seconds.
    pub duration: f32,
    pub channels: Vec<PropAnimationChannel>,
    /// Whether the asset authors this clip as a loop. Defaults to `true` (a
    /// looping clip); the runtime's explicit cue/route flags always win, so
    /// this is the documented default for a map that does not state one.
    pub looped: bool,
    /// Ground speed this clip's stride is authored for, in metres per second.
    ///
    /// A walk or run clip declares it in `asset.extras.places_entity_clips`,
    /// measured from the mesh at build time; the runtime plays the clip at
    /// `route_speed / reference_speed` so the feet do not slide.
    pub reference_speed_mps: Option<f32>,
    /// The asset's own label for the clip (`walk`, `run`, `idle`, `pose`, ...).
    pub kind: Option<String>,
}

/// One morph target's per-vertex deltas, parallel to [`PropModel::vertices`].
///
/// A vertex outside the mesh that declared the target keeps a zero delta, so
/// one target applies to the whole model's vertex list. Positions are already
/// in model space (mesh-local deltas passed through the node transform or the
/// rest skinning), so the runtime adds them to a baked vertex without another
/// transform. Normals and tangents are retained for completeness; the
/// vertex-lit character path bakes light into the albedo and does not consume
/// them.
#[derive(Clone, Debug, Default)]
pub struct PropMorphTarget {
    /// Per-vertex position deltas.
    pub position: Vec<[f32; 3]>,
    /// Per-vertex normal deltas, empty when the target stores none.
    pub normal: Vec<[f32; 3]>,
    /// Per-vertex tangent deltas (`xyz`, with zero `w` padding), empty when absent.
    pub tangent: Vec<[f32; 4]>,
}

/// A decoded, ready-to-render prop model.
#[derive(Clone, Debug, Default)]
pub struct PropModel {
    /// Model-space vertices with baked per-face shading and material colour in
    /// `color`. A skinned primitive's positions are the bind-pose skinned
    /// positions (`meshNodeInverseGlobal * jointGlobal(rest) * inverseBind *
    /// p`), so the static prop path draws the same rest pose the character
    /// path starts from.
    pub vertices: Vec<PropVertex>,
    /// Triangle indices into `vertices`.
    pub indices: Vec<u16>,
    /// Embedded textures, decoded to 8-bit RGBA, one per distinct image
    /// actually referenced by a used material, in first-use order.
    pub textures: Vec<Arc<RawImage>>,
    /// Draw ranges in primitive order, ascending through `indices`; primitives
    /// that draw nothing are omitted.
    pub submeshes: Vec<PropSubmesh>,
    /// Triangle count (a multiple of three indices), used for budget checks.
    pub triangles: usize,
    /// Number of materials declared by the asset. A primitive that declares no
    /// `material` uses the implicit glTF default material, reported in
    /// [`PropSubmesh::material`] as the synthetic slot at this index.
    pub materials: usize,
    /// The retained skeleton, when the model has a skinned primitive.
    pub skin: Option<PropSkin>,
    /// Per-vertex joint slots into [`PropSkin::joints`], parallel to
    /// `vertices`; empty when the model is unskinned. Vertices of an
    /// unskinned primitive inside a skinned model carry zero weights and stay
    /// at their bind position.
    pub joints: Vec<[u16; 4]>,
    /// Per-vertex joint weights, parallel to `vertices`; empty when the model
    /// is unskinned. Every vertex of a skinned primitive is renormalised to
    /// sum to one.
    pub weights: Vec<[f32; 4]>,
    /// Morph targets in first-visit order; every target's delta arrays are
    /// parallel to `vertices` (zero outside the primitive that declared it).
    pub morph_targets: Vec<PropMorphTarget>,
    /// Default morph weights, parallel to `morph_targets`: the primitive's
    /// `weights`, else the mesh's, else zero. The bind-pose bake already
    /// includes them, so the runtime applies `(current - default)`.
    pub morph_weights: Vec<f32>,
    /// Per mesh-visit morph range: `(node index, first target, target count)`.
    /// A `weights` animation channel targeting that node drives this range.
    pub mesh_morph_ranges: Vec<(u16, u16, u16)>,
    /// The model's animation clips, in asset order. Empty when it declares
    /// none.
    pub animations: Vec<PropAnimation>,
    /// The retained node hierarchy, present when the model declares a skin or
    /// animation clips; empty for a purely static model.
    ///
    /// A model that declares animation clips but no skin is a *rigid* animated
    /// prop: each primitive's vertices are bound one-to-one to their owning
    /// node (weight 1 against that node's joint slot), so the character path
    /// poses the node hierarchy without a skin. The wall switch is the shipped
    /// example.
    pub nodes: Vec<PropNode>,
}

impl PropModel {
    /// The morph range one node's mesh owns, when it has morph targets.
    #[must_use]
    pub fn morph_range_for_node(&self, node: u16) -> Option<(usize, usize)> {
        self.mesh_morph_ranges
            .iter()
            .find(|(candidate, _, count)| *candidate == node && *count > 0)
            .map(|(_, start, count)| (usize::from(*start), usize::from(*count)))
    }
    /// Axis-aligned model-space bounds, or `None` for an empty mesh.
    #[must_use]
    pub fn bounds(&self) -> Option<([f32; 3], [f32; 3])> {
        let first = self.vertices.first()?;
        let mut min = first.pos;
        let mut max = first.pos;
        for vertex in &self.vertices {
            for ((axis_min, axis_max), value) in min.iter_mut().zip(max.iter_mut()).zip(&vertex.pos)
            {
                *axis_min = axis_min.min(*value);
                *axis_max = axis_max.max(*value);
            }
        }
        Some((min, max))
    }

    /// Number of distinct decoded textures the model carries.
    #[must_use]
    pub const fn texture_count(&self) -> usize {
        self.textures.len()
    }

    /// True when the model carries a skin and can be driven by the character
    /// path.
    #[must_use]
    pub const fn is_skinned(&self) -> bool {
        self.skin.is_some()
    }

    /// True when the model declares animation clips and a retained node
    /// hierarchy, so its nodes can be posed even without a skin.
    #[must_use]
    pub const fn is_animated(&self) -> bool {
        !self.animations.is_empty() && !self.nodes.is_empty()
    }

    /// True when the character path can pose this model: a skin, or a rigid
    /// node hierarchy driven by clips.
    #[must_use]
    pub const fn is_animatable(&self) -> bool {
        self.is_skinned() || self.is_animated()
    }
}

/// Parses a self-contained GLB prop asset.
///
/// # Errors
///
/// Returns a [`GltfError`] naming the first problem found: a container that is
/// not a self-contained glTF 2.0 GLB, a document feature the prop renderer
/// cannot draw (extensions other than `KHR_materials_emissive_strength`,
/// sparse accessors), a malformed scene graph (a
/// cycle, dangling node or non-finite transform), a malformed skin (an
/// out-of-range joint slot, a non-finite or non-positive weight, an inverse
/// bind count that does not match the joint list), a mesh outside the prop
/// budgets, or vertex data that is non-finite or outside the documented UV
/// range.
pub fn parse_glb(bytes: &[u8]) -> Result<PropModel, GltfError> {
    parse::parse_glb_with_limits(bytes, 1.01, MAX_PROP_TRIANGLES)
}

/// Hash-verified delivered cloths retain a small fitted-sheet overscan. The
/// normal import contract remains strict; all model samplers clamp to the edge.
pub(crate) fn parse_glb_with_clamped_overscan(bytes: &[u8]) -> Result<PropModel, GltfError> {
    parse::parse_glb_with_limits(bytes, 1.05, MAX_PROP_TRIANGLES)
}

/// Maximum reviewed static detail; ordinary imports retain the 6,000 limit.
pub(crate) const MAX_REVIEWED_PROP_TRIANGLES: usize = 16_240;

/// Used only after the asset cache verifies an explicitly reviewed export hash.
pub(crate) fn parse_glb_with_reviewed_triangle_budget(
    bytes: &[u8],
    triangle_limit: usize,
) -> Result<PropModel, GltfError> {
    if !(MAX_PROP_TRIANGLES..=MAX_REVIEWED_PROP_TRIANGLES).contains(&triangle_limit) {
        return Err(GltfError::new(
            "reviewed triangle limit is outside its bounded profile",
        ));
    }
    parse::parse_glb_with_limits(bytes, 1.01, triangle_limit)
}

/// Length of a top-level array field, or zero when it is absent.
fn list_len(json: &serde_json::Value, key: &str) -> usize {
    json.get(key)
        .and_then(|value| value.as_array())
        .map_or(0, std::vec::Vec::len)
}
