//! Dependency-free GLB (binary glTF 2.0) reader for prop models.
//!
//! The prop pipeline in `tools/props` emits one deliberately boring GLB
//! profile, and production prop files come from modelling tools that split a
//! model per material and nest transform groups. This reader accepts the
//! production-friendly subset Places genuinely needs (see `assets/README.md`
//! for the asset rules) without becoming a general glTF engine:
//!
//! * GLB container, glTF 2.0, the scene graph walked from `scene` (or scene
//!   0, or the parentless nodes when no scenes exist), with node TRS or
//!   matrix transforms composed down the hierarchy;
//! * any number of nodes, meshes, primitives and materials up to the engine
//!   ceilings in [`crate::level`], each primitive keeping its own material;
//! * `POSITION` (float32), `TEXCOORD_0` (float32 or normalised integer),
//!   `COLOR_0` (optional; float32 or normalised integer), 16/32-bit indices;
//! * `mode: 4` (triangles) only, no morph targets;
//! * one skin per model: `JOINTS_0` (8/16-bit) and `WEIGHTS_0` (float32 or
//!   normalised 8/16-bit) per vertex, a retained node hierarchy, the joint
//!   list and the inverse bind matrices. A skinned primitive's `vertices`
//!   positions are baked to the bind pose, so the static prop path draws the
//!   rest pose unchanged; the raw skin data stays on the model for the
//!   character path;
//! * `animations` (LINEAR and STEP samplers) retained as named clips with
//!   per-node translation/rotation/scale channels; CUBICSPLINE samplers and
//!   morph-target weight channels are rejected by name;
//! * up to [`crate::level::MAX_PROP_IMAGES`] PNG images embedded in
//!   bufferViews (self-contained, no external files or data URIs), each
//!   distinct image decoded once for the model;
//! * `pbrMetallicRoughness.baseColorFactor`, `emissiveFactor` and the
//!   `KHR_materials_emissive_strength` extension - the only glTF extension
//!   this reader understands.
//!
//! Everything else - morph targets, external or data-URI images, sparse
//! accessors, texture transforms and every other extension - produces a
//! descriptive [`GltfError`] so a malformed asset degrades into the loader's
//! placeholder box instead of panicking or looping.

use std::collections::{HashMap, HashSet};

use glam::{Mat4, Quat, Vec3};

use crate::level::{
    MAX_ANIMATION_CHANNELS, MAX_PROP_ANIMATIONS, MAX_PROP_IMAGES, MAX_PROP_JOINTS,
    MAX_PROP_MATERIALS, MAX_PROP_PRIMITIVES, MAX_PROP_TEXTURE_SIZE, MAX_PROP_TRIANGLES,
    MAX_PROP_VERTICES,
};
use crate::loader::RawImage;
use crate::materials::MaterialEmission;

const GLB_MAGIC: u32 = 0x4654_6C67;
const CHUNK_JSON: u32 = 0x4E4F_534A;
const CHUNK_BIN: u32 = 0x004E_4942;

const COMPONENT_FLOAT: u32 = 5126;
const COMPONENT_UBYTE: u32 = 5121;
const COMPONENT_USHORT: u32 = 5123;
const COMPONENT_UINT: u32 = 5125;

const MODE_TRIANGLES: u32 = 4;

/// The one glTF extension this reader understands.
const KHR_MATERIALS_EMISSIVE_STRENGTH: &str = "KHR_materials_emissive_strength";

/// Maximum node-hierarchy depth accepted; real prop models nest a handful of
/// transform groups at most, so anything deeper is treated as malformed.
const MAX_NODE_DEPTH: usize = 64;

/// Maximum node visits during one scene walk.
///
/// A DAG may legitimately reference one node from several parents, but the
/// total has to stay bounded so a pathological graph cannot expand into
/// unbounded work.
const MAX_NODE_VISITS: usize = 4_096;

/// `baseColorFactor` of a material that does not declare one: no multiply.
const DEFAULT_BASE_COLOR: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

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

/// One vertex of a loaded prop model: position, baked diffuse tint and UV.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PropVertex {
    pub pos: [f32; 3],
    pub color: [f32; 4],
    pub uv: [f32; 2],
}

/// One primitive's slice of the model: a material assignment and an index range.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PropSubmesh {
    /// Index into the document's material list (stable, even for skipped materials).
    pub material: u16,
    /// Index into [`PropModel::textures`], or `None` for a material with no texture.
    pub texture: Option<u16>,
    /// Emission of the primitive's material.
    pub emission: MaterialEmission,
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

/// The retained skeleton of a skinned model.
#[derive(Clone, Debug)]
pub struct PropSkin {
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
}

impl AnimationPath {
    /// Values per keyframe: 3 for translation/scale, 4 for rotation.
    #[must_use]
    pub const fn stride(self) -> usize {
        match self {
            Self::Translation | Self::Scale => 3,
            Self::Rotation => 4,
        }
    }
}

/// How an animation channel's keys interpolate between samples.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnimationInterpolation {
    Linear,
    Step,
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
    /// Keyframe values, `times.len() * path.stride()` floats.
    pub values: Vec<f32>,
}

impl PropAnimationChannel {
    /// This channel's last keyframe time, or zero for an empty channel.
    #[must_use]
    pub fn duration(&self) -> f32 {
        self.times.last().copied().unwrap_or(0.0)
    }
}

/// One named animation clip.
#[derive(Clone, Debug)]
pub struct PropAnimation {
    pub name: String,
    /// The clip's last keyframe time, in seconds.
    pub duration: f32,
    pub channels: Vec<PropAnimationChannel>,
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
    pub textures: Vec<RawImage>,
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
    /// The model's animation clips, in asset order. Empty when it declares
    /// none.
    pub animations: Vec<PropAnimation>,
}

impl PropModel {
    /// Axis-aligned model-space bounds, or `None` for an empty mesh.
    #[must_use]
    pub fn bounds(&self) -> Option<([f32; 3], [f32; 3])> {
        let first = self.vertices.first()?;
        let mut min = first.pos;
        let mut max = first.pos;
        for vertex in &self.vertices {
            for ((min, max), value) in min.iter_mut().zip(max.iter_mut()).zip(&vertex.pos) {
                *min = min.min(*value);
                *max = max.max(*value);
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
}

// ------------------------------------------------------------------- parsing

/// Parses a self-contained GLB prop asset.
///
/// # Errors
///
/// Returns a [`GltfError`] naming the first problem found: a container that is
/// not a self-contained glTF 2.0 GLB, a document feature the prop renderer
/// cannot draw (extensions other than `KHR_materials_emissive_strength`,
/// morph targets, CUBICSPLINE animation samplers), a malformed scene graph (a
/// cycle, dangling node or non-finite transform), a malformed skin (an
/// out-of-range joint slot, a non-finite or non-positive weight, an inverse
/// bind count that does not match the joint list), a mesh outside the prop
/// budgets, or vertex data that is non-finite or outside the documented UV
/// range.
pub fn parse_glb(bytes: &[u8]) -> Result<PropModel, GltfError> {
    let (json, binary) = parse_container(bytes)?;
    let materials = validate_document_root(&json)?;
    let mut doc = Doc::new(&json, &binary, materials)?;
    for root in scene_roots(&json)? {
        doc.traverse_node(root, Mat4::IDENTITY, &mut Vec::new())?;
    }
    let triangles = validate_mesh(&doc.vertices, &doc.indices)?;
    let animations = parse_animations(&json, &binary)?;
    // Joint and weight arrays are parallel to `vertices`. A rigged document
    // that never uses a skin has nothing for the character path to claim, so
    // its placeholder entries are dropped.
    if doc.skin.is_none() {
        doc.joints = Vec::new();
        doc.weights = Vec::new();
    }
    Ok(PropModel {
        vertices: doc.vertices,
        indices: doc.indices,
        textures: doc.textures,
        submeshes: doc.submeshes,
        triangles,
        materials,
        skin: doc.skin,
        joints: doc.joints,
        weights: doc.weights,
        animations,
    })
}

/// Rejects document-level features the prop renderer cannot draw.
///
/// Returns the number of declared materials on success.
fn validate_document_root(json: &serde_json::Value) -> Result<usize, GltfError> {
    validate_extensions(json)?;
    validate_morph_targets(json)?;

    let materials = list_len(json, "materials");
    if materials > MAX_PROP_MATERIALS {
        return Err(GltfError::new(format!(
            "prop model declares {materials} materials; the engine ceiling is {MAX_PROP_MATERIALS}"
        )));
    }
    let images = list_len(json, "images");
    if images > MAX_PROP_IMAGES {
        return Err(GltfError::new(format!(
            "prop model embeds {images} images; the engine ceiling is {MAX_PROP_IMAGES}"
        )));
    }
    let animations = list_len(json, "animations");
    if animations > MAX_PROP_ANIMATIONS {
        return Err(GltfError::new(format!(
            "prop model declares {animations} animations; the engine ceiling is {MAX_PROP_ANIMATIONS}"
        )));
    }
    Ok(materials)
}

/// Length of a top-level array field, or zero when it is absent.
fn list_len(json: &serde_json::Value, key: &str) -> usize {
    json.get(key)
        .and_then(|value| value.as_array())
        .map_or(0, std::vec::Vec::len)
}

/// Rejects every glTF extension except `KHR_materials_emissive_strength`.
fn validate_extensions(json: &serde_json::Value) -> Result<(), GltfError> {
    for key in ["extensionsUsed", "extensionsRequired"] {
        let Some(list) = json.get(key).and_then(|value| value.as_array()) else {
            continue;
        };
        for name in list.iter().filter_map(|entry| entry.as_str()) {
            if name != KHR_MATERIALS_EMISSIVE_STRENGTH {
                return Err(GltfError::new(format!(
                    "glTF extensions are not supported by the prop reader: {name} \
                     (only {KHR_MATERIALS_EMISSIVE_STRENGTH} is allowed)"
                )));
            }
        }
    }
    reject_unknown_extensions(json)
}

/// Walks the document and rejects any `extensions` object naming an extension
/// other than the emissive-strength one.
fn reject_unknown_extensions(value: &serde_json::Value) -> Result<(), GltfError> {
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                // `extras` is arbitrary application data, not glTF structure.
                if key == "extras" {
                    continue;
                }
                if key == "extensions" {
                    let Some(extensions) = child.as_object() else {
                        continue;
                    };
                    for name in extensions.keys() {
                        if name != KHR_MATERIALS_EMISSIVE_STRENGTH {
                            return Err(GltfError::new(format!(
                                "glTF extension {name} is not supported; \
                                 only {KHR_MATERIALS_EMISSIVE_STRENGTH} may be used"
                            )));
                        }
                    }
                    continue;
                }
                reject_unknown_extensions(child)?;
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                reject_unknown_extensions(item)?;
            }
        }
        serde_json::Value::Null
        | serde_json::Value::Bool(_)
        | serde_json::Value::Number(_)
        | serde_json::Value::String(_) => {}
    }
    Ok(())
}

/// Rejects morph-target data on any mesh, primitive or node.
fn validate_morph_targets(json: &serde_json::Value) -> Result<(), GltfError> {
    if let Some(meshes) = json.get("meshes").and_then(|value| value.as_array()) {
        for mesh in meshes {
            if mesh.get("weights").is_some() {
                return Err(morph_targets_error());
            }
            let Some(primitives) = mesh.get("primitives").and_then(|value| value.as_array()) else {
                continue;
            };
            for primitive in primitives {
                if primitive.get("targets").is_some() {
                    return Err(morph_targets_error());
                }
            }
        }
    }
    if let Some(nodes) = json.get("nodes").and_then(|value| value.as_array()) {
        for node in nodes {
            if node.get("weights").is_some() {
                return Err(morph_targets_error());
            }
        }
    }
    Ok(())
}

fn morph_targets_error() -> GltfError {
    GltfError::new("morph targets are not supported; prop models must be static")
}

/// Indices of the nodes a scene walk starts from.
///
/// Uses `scene` (default 0) when the document declares scenes, and otherwise
/// the nodes no other node lists as a child.
fn scene_roots(json: &serde_json::Value) -> Result<Vec<usize>, GltfError> {
    let nodes = json
        .get("nodes")
        .and_then(|value| value.as_array())
        .filter(|nodes| !nodes.is_empty())
        .ok_or_else(|| {
            GltfError::new("file has no nodes; a prop model needs a node with a mesh")
        })?;
    if let Some(scenes) = json
        .get("scenes")
        .and_then(|value| value.as_array())
        .filter(|scenes| !scenes.is_empty())
    {
        let scene_index = json.get("scene").and_then(json_usize).unwrap_or(0);
        let scene = scenes
            .get(scene_index)
            .ok_or_else(|| GltfError::new(format!("scene {scene_index} does not exist")))?;
        let roots = scene
            .get("nodes")
            .and_then(|value| value.as_array())
            .filter(|roots| !roots.is_empty())
            .ok_or_else(|| GltfError::new(format!("scene {scene_index} declares no nodes")))?;
        let mut out = Vec::with_capacity(roots.len());
        for root in roots {
            let index =
                json_usize(root).ok_or_else(|| GltfError::new("scene node is not a node index"))?;
            if index >= nodes.len() {
                return Err(GltfError::new(format!(
                    "scene {scene_index} references node {index}, which does not exist"
                )));
            }
            out.push(index);
        }
        return Ok(out);
    }

    let mut has_parent: HashSet<usize> = HashSet::new();
    for node in nodes {
        let Some(children) = node.get("children").and_then(|value| value.as_array()) else {
            continue;
        };
        for child in children {
            let index = json_usize(child)
                .ok_or_else(|| GltfError::new("node child is not a node index"))?;
            if index >= nodes.len() {
                return Err(GltfError::new(format!(
                    "node references child {index}, which does not exist"
                )));
            }
            has_parent.insert(index);
        }
    }
    let roots: Vec<usize> = (0..nodes.len())
        .filter(|index| !has_parent.contains(index))
        .collect();
    if roots.is_empty() {
        return Err(GltfError::new(
            "the node hierarchy has no root node; every node is a child",
        ));
    }
    Ok(roots)
}

/// One material resolved into the values a primitive needs.
#[derive(Clone, Copy)]
struct ResolvedMaterial {
    /// `baseColorFactor` multiplied into every vertex of the material.
    color: [f32; 4],
    /// Index into [`PropModel::textures`], or `None` for an untextured material.
    texture: Option<u16>,
    /// Material emission, already sanitised.
    emission: MaterialEmission,
}

impl Default for ResolvedMaterial {
    fn default() -> Self {
        Self {
            color: DEFAULT_BASE_COLOR,
            texture: None,
            emission: MaterialEmission::NONE,
        }
    }
}

/// One GLB document mid-assembly, with the caches that keep resolution
/// single-shot: textures decode once per distinct image, materials resolve
/// once, and the traversal counters cap the scene graph.
struct Doc<'a> {
    json: &'a serde_json::Value,
    binary: &'a [u8],
    /// Declared material count; a primitive without `material` reports it as
    /// its synthetic default-material slot.
    default_material_index: usize,
    material_cache: Vec<Option<ResolvedMaterial>>,
    vertices: Vec<PropVertex>,
    indices: Vec<u16>,
    submeshes: Vec<PropSubmesh>,
    textures: Vec<RawImage>,
    /// glTF image index -> index in `textures`, in first-use order.
    texture_of_image: HashMap<usize, u16>,
    primitives_seen: usize,
    node_visits: usize,
    /// Retained node hierarchy, present only when the document declares a skin
    /// or animations; static models keep the historical flattening and drop
    /// it.
    rig: Option<Rig>,
    /// The model's single resolved skin, set by the first skinned primitive.
    skin: Option<PropSkin>,
    /// Per-vertex joint slots and weights, parallel to `vertices`; empty for
    /// an unskinned model.
    joints: Vec<[u16; 4]>,
    weights: Vec<[f32; 4]>,
}

impl<'a> Doc<'a> {
    fn new(
        json: &'a serde_json::Value,
        binary: &'a [u8],
        materials: usize,
    ) -> Result<Self, GltfError> {
        let default_material_index = materials;
        let cache_len = default_material_index
            .checked_add(1)
            .ok_or_else(|| GltfError::new("material count overflows"))?;
        let mut material_cache = vec![None; cache_len];
        if let Some(slot) = material_cache.get_mut(default_material_index) {
            *slot = Some(ResolvedMaterial::default());
        }
        let rig = if json.get("skins").is_some() || json.get("animations").is_some() {
            Some(Rig::build(json)?)
        } else {
            None
        };
        Ok(Self {
            json,
            binary,
            default_material_index,
            material_cache,
            vertices: Vec::new(),
            indices: Vec::new(),
            submeshes: Vec::new(),
            textures: Vec::new(),
            texture_of_image: HashMap::new(),
            primitives_seen: 0,
            node_visits: 0,
            rig,
            skin: None,
            joints: Vec::new(),
            weights: Vec::new(),
        })
    }

    /// True when the document declares a skin or animations and therefore
    /// retains its node hierarchy.
    const fn is_rigged(&self) -> bool {
        self.rig.is_some()
    }

    /// Visits one node: composes its transform on top of `parent`, appends its
    /// mesh if it has one, and recurses into its children.
    fn traverse_node(
        &mut self,
        index: usize,
        parent: Mat4,
        path: &mut Vec<usize>,
    ) -> Result<(), GltfError> {
        let nodes = self
            .json
            .get("nodes")
            .and_then(|value| value.as_array())
            .ok_or_else(|| GltfError::new("file has no nodes"))?;
        let node = nodes
            .get(index)
            .ok_or_else(|| GltfError::new(format!("node {index} does not exist")))?;
        if path.contains(&index) {
            return Err(GltfError::new(format!(
                "the node hierarchy contains a cycle at node {index}; prop models must be acyclic"
            )));
        }
        if path.len() >= MAX_NODE_DEPTH {
            return Err(GltfError::new(format!(
                "the node hierarchy is deeper than {MAX_NODE_DEPTH} levels"
            )));
        }
        self.node_visits = self.node_visits.saturating_add(1);
        if self.node_visits > MAX_NODE_VISITS {
            return Err(GltfError::new(format!(
                "the node hierarchy expands past {MAX_NODE_VISITS} visits; \
                 check for repeated node references"
            )));
        }
        // `glam` matrix multiplication is per-element `f32` arithmetic with no
        // overflow or panic path; clippy cannot see that through the operator.
        #[allow(clippy::arithmetic_side_effects)]
        let world = parent * node_transform(node, index)?;
        path.push(index);
        let result = self.visit_node_contents(node, index, &world, path);
        path.pop();
        result
    }

    /// Appends a node's mesh and recurses into its children.
    fn visit_node_contents(
        &mut self,
        node: &serde_json::Value,
        index: usize,
        transform: &Mat4,
        path: &mut Vec<usize>,
    ) -> Result<(), GltfError> {
        if let Some(mesh) = node.get("mesh") {
            let mesh_index = json_usize(mesh).ok_or_else(|| {
                GltfError::new(format!("node {index} has a non-numeric mesh index"))
            })?;
            // glTF declares a skin on the *node*; every primitive of the mesh
            // it references is skinned with it.
            let node_skin = match node.get("skin") {
                Some(skin) => Some(json_usize(skin).ok_or_else(|| {
                    GltfError::new(format!("node {index} has a non-numeric skin index"))
                })?),
                None => None,
            };
            self.read_mesh(mesh_index, index, node_skin, transform)?;
        }
        if let Some(children) = node.get("children") {
            let children = children
                .as_array()
                .ok_or_else(|| GltfError::new(format!("node {index} children is not an array")))?;
            for child in children {
                let child_index = json_usize(child).ok_or_else(|| {
                    GltfError::new(format!("node {index} has a non-numeric child index"))
                })?;
                self.traverse_node(child_index, *transform, path)?;
            }
        }
        Ok(())
    }

    /// Appends every primitive of one mesh under `transform`.
    fn read_mesh(
        &mut self,
        mesh_index: usize,
        node_index: usize,
        node_skin: Option<usize>,
        transform: &Mat4,
    ) -> Result<(), GltfError> {
        let meshes = self
            .json
            .get("meshes")
            .and_then(|value| value.as_array())
            .ok_or_else(|| GltfError::new("file has no meshes"))?;
        let mesh = meshes.get(mesh_index).ok_or_else(|| {
            GltfError::new(format!(
                "node references mesh {mesh_index}, which does not exist"
            ))
        })?;
        let primitives = mesh
            .get("primitives")
            .and_then(|value| value.as_array())
            .ok_or_else(|| GltfError::new(format!("mesh {mesh_index} has no primitives")))?;
        for primitive in primitives {
            self.primitives_seen = self.primitives_seen.saturating_add(1);
            if self.primitives_seen > MAX_PROP_PRIMITIVES {
                return Err(GltfError::new(format!(
                    "prop model declares more than {MAX_PROP_PRIMITIVES} primitives; \
                     the engine ceiling is {MAX_PROP_PRIMITIVES}"
                )));
            }
            self.read_primitive(primitive, transform, mesh_index, node_index, node_skin)?;
        }
        Ok(())
    }

    /// Reads one primitive's vertices and triangle indices into the model.
    fn read_primitive(
        &mut self,
        primitive: &serde_json::Value,
        transform: &Mat4,
        mesh_index: usize,
        node_index: usize,
        node_skin: Option<usize>,
    ) -> Result<(), GltfError> {
        let mode = primitive
            .get("mode")
            .and_then(json_u32)
            .unwrap_or(MODE_TRIANGLES);
        if mode != MODE_TRIANGLES {
            return Err(GltfError::new(format!(
                "primitive mode {mode} is not TRIANGLES (4)"
            )));
        }
        let attributes = read_attributes(self.json, self.binary, primitive, mesh_index)?;
        let vertex_count = attributes.positions.len();
        let material = match primitive.get("material") {
            Some(value) => json_usize(value).ok_or_else(|| {
                GltfError::new(format!(
                    "mesh {mesh_index} has a primitive with a non-numeric material index"
                ))
            })?,
            None => self.default_material_index,
        };
        let resolved = self.resolve_material(material)?;

        let local_indices = primitive_indices(self.json, self.binary, primitive, vertex_count)?;
        if local_indices.is_empty() {
            return Ok(());
        }
        if local_indices.len() % 3 != 0 {
            return Err(GltfError::new(
                "index count is not a multiple of three; props must be triangle lists",
            ));
        }
        let index_count = u32::try_from(local_indices.len())
            .map_err(|_| GltfError::new("primitive index count does not fit in 32 bits"))?;
        let vertex_total = self.vertices.len().saturating_add(vertex_count);
        if vertex_total > MAX_PROP_VERTICES {
            return Err(GltfError::new(format!(
                "prop model assembles {vertex_total} vertices; \
                 the engine ceiling is {MAX_PROP_VERTICES}"
            )));
        }

        let base = self.vertices.len();
        // Skin pairing: a glTF node declares one skin and then every primitive
        // of the mesh it references must supply JOINTS_0 and WEIGHTS_0; a
        // primitive with joint attributes but no skin is malformed.
        let skinned = if node_skin.is_some() {
            if attributes.joints.is_none() || attributes.weights.is_none() {
                return Err(GltfError::new(format!(
                    "mesh {mesh_index} has a skinned primitive with no \
                     JOINTS_0/WEIGHTS_0 attributes"
                )));
            }
            true
        } else {
            if attributes.joints.is_some() || attributes.weights.is_some() {
                return Err(GltfError::new(format!(
                    "mesh {mesh_index} has a primitive with JOINTS_0/WEIGHTS_0 \
                     but no skin"
                )));
            }
            false
        };
        if skinned {
            let skin_index = node_skin
                .ok_or_else(|| GltfError::new(format!("mesh {mesh_index} references no skin")))?;
            let skin_matrices = self.resolve_primitive_skin(skin_index, node_index)?;
            append_skinned_vertices(
                &mut self.vertices,
                &mut self.joints,
                &mut self.weights,
                &attributes,
                &skin_matrices,
                resolved.color,
            )?;
        } else {
            append_vertices(&mut self.vertices, &attributes, transform, resolved.color)?;
            if self.is_rigged() {
                // Keep the joint/weight arrays parallel to `vertices`; a
                // rigid primitive inside a rigged document stays at its bind
                // pose (zero weights).
                self.joints.resize(self.vertices.len(), [0; 4]);
                self.weights.resize(self.vertices.len(), [0.0; 4]);
            }
        }
        let first_index = u32::try_from(self.indices.len())
            .map_err(|_| GltfError::new("prop model index buffer does not fit in 32 bits"))?;
        append_indices(&mut self.indices, &local_indices, base, vertex_count)?;
        self.submeshes.push(PropSubmesh {
            material: u16::try_from(material)
                .map_err(|_| GltfError::new("material index does not fit in 16 bits"))?,
            texture: resolved.texture,
            emission: resolved.emission,
            first_index,
            index_count,
        });
        let triangles = self.indices.len() / 3;
        if triangles > MAX_PROP_TRIANGLES {
            return Err(GltfError::new(format!(
                "prop model assembles {triangles} triangles; \
                 the engine ceiling is {MAX_PROP_TRIANGLES}"
            )));
        }
        Ok(())
    }

    /// Resolves a primitive's skin and returns the bind-pose skinning
    /// matrices per joint slot (`meshInverseGlobal * jointGlobal * inverseBind`).
    ///
    /// The model retains one skin: a second primitive that references a
    /// different skin or a different mesh node is refused, because the model
    /// carries a single [`PropSkin`] and a single skinned mesh.
    fn resolve_primitive_skin(
        &mut self,
        skin_index: usize,
        node_index: usize,
    ) -> Result<Vec<Mat4>, GltfError> {
        let json = self.json;
        let binary = self.binary;
        let rig = self.rig.as_mut().ok_or_else(|| {
            GltfError::new(format!(
                "primitive references skin {skin_index}, but the document declares no skins"
            ))
        })?;
        if let Some(used) = rig.used_skin
            && used != skin_index
        {
            return Err(GltfError::new(format!(
                "a prop model may use one skin; primitives reference skins {used} and {skin_index}"
            )));
        }
        if let Some(mesh_node) = rig.mesh_node
            && usize::from(mesh_node) != node_index
        {
            return Err(GltfError::new(format!(
                "a prop model may have one skinned mesh node; \
                 nodes {} and {node_index} both carry skinned primitives",
                usize::from(mesh_node)
            )));
        }
        rig.used_skin = Some(skin_index);
        rig.mesh_node = Some(
            u16::try_from(node_index)
                .map_err(|_| GltfError::new("node index does not fit in 16 bits"))?,
        );
        let resolved = rig.resolve_skin(skin_index, json, binary)?;
        let matrices = resolved.skin_matrices();
        if self.skin.is_none() {
            self.skin = Some(resolved.skin.clone());
        }
        Ok(matrices)
    }

    /// Resolves one declared material (or the synthetic default slot),
    /// decoding and caching anything it references.
    fn resolve_material(&mut self, index: usize) -> Result<ResolvedMaterial, GltfError> {
        if let Some(cached) = self.material_cache.get(index).copied().flatten() {
            return Ok(cached);
        }
        if index >= self.default_material_index {
            return Err(GltfError::new(format!(
                "primitive references material {index}, which does not exist"
            )));
        }
        let json = self.json;
        let material = json
            .get("materials")
            .and_then(|value| value.as_array())
            .and_then(|list| list.get(index))
            .ok_or_else(|| GltfError::new(format!("material {index} does not exist")))?;
        let pbr = material.get("pbrMetallicRoughness");
        let color = match pbr.and_then(|pbr| pbr.get("baseColorFactor")) {
            Some(value) => numeric_array::<4>(value, &format!("material {index} baseColorFactor"))?,
            None => DEFAULT_BASE_COLOR,
        };
        if color.iter().any(|channel| !channel.is_finite()) {
            return Err(GltfError::new(format!(
                "material {index} has a non-finite baseColorFactor"
            )));
        }
        let texture = match pbr.and_then(|pbr| pbr.get("baseColorTexture")) {
            Some(reference) => Some(self.resolve_texture_reference(
                reference,
                &format!("material {index} baseColorTexture"),
            )?),
            None => None,
        };
        let emission = self.resolve_emission(material, index)?;
        let resolved = ResolvedMaterial {
            color,
            texture,
            emission,
        };
        if let Some(slot) = self.material_cache.get_mut(index) {
            *slot = Some(resolved);
        }
        Ok(resolved)
    }

    /// Reads `emissiveFactor`, `KHR_materials_emissive_strength` and
    /// `emissiveTexture` into a sanitised [`MaterialEmission`].
    fn resolve_emission(
        &mut self,
        material: &serde_json::Value,
        index: usize,
    ) -> Result<MaterialEmission, GltfError> {
        let color = match material.get("emissiveFactor") {
            Some(value) => numeric_array::<3>(value, &format!("material {index} emissiveFactor"))?,
            None => [0.0; 3],
        };
        let extension = material
            .get("extensions")
            .and_then(|value| value.get(KHR_MATERIALS_EMISSIVE_STRENGTH));
        let intensity = match extension.and_then(|value| value.get("emissiveStrength")) {
            Some(value) => json_f32(value).ok_or_else(|| {
                GltfError::new(format!("material {index} emissiveStrength is not a number"))
            })?,
            None => 1.0,
        };
        let mask = match material.get("emissiveTexture") {
            Some(reference) => Some(self.resolve_texture_reference(
                reference,
                &format!("material {index} emissiveTexture"),
            )?),
            None => None,
        };
        Ok(MaterialEmission::new(color, intensity)
            .with_mask(mask)
            .sanitized())
    }

    /// Resolves a `baseColorTexture`/`emissiveTexture` reference to an index
    /// in [`PropModel::textures`].
    fn resolve_texture_reference(
        &mut self,
        reference: &serde_json::Value,
        label: &str,
    ) -> Result<u16, GltfError> {
        let index = reference
            .get("index")
            .and_then(json_usize)
            .ok_or_else(|| GltfError::new(format!("{label} has no texture index")))?;
        if let Some(tex_coord) = reference.get("texCoord") {
            let tex_coord = json_usize(tex_coord)
                .ok_or_else(|| GltfError::new(format!("{label} texCoord is not a number")))?;
            if tex_coord != 0 {
                return Err(GltfError::new(format!(
                    "{label} uses texCoord {tex_coord}; only TEXCOORD_0 is supported"
                )));
            }
        }
        let json = self.json;
        let texture = json
            .get("textures")
            .and_then(|value| value.as_array())
            .and_then(|list| list.get(index))
            .ok_or_else(|| {
                GltfError::new(format!(
                    "{label} references texture {index}, which does not exist"
                ))
            })?;
        let source = texture
            .get("source")
            .and_then(json_usize)
            .ok_or_else(|| GltfError::new(format!("texture {index} has no image source")))?;
        self.decode_image(source)
    }

    /// Decodes one embedded PNG image, once per image index.
    fn decode_image(&mut self, image_index: usize) -> Result<u16, GltfError> {
        if let Some(existing) = self.texture_of_image.get(&image_index) {
            return Ok(*existing);
        }
        let json = self.json;
        let image = json
            .get("images")
            .and_then(|value| value.as_array())
            .and_then(|list| list.get(image_index))
            .ok_or_else(|| GltfError::new(format!("image {image_index} does not exist")))?;
        if image.get("uri").is_some() {
            return Err(GltfError::new(
                "external or data-URI images are not supported; embed the PNG in the GLB",
            ));
        }
        let mime = image
            .get("mimeType")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        if mime != "image/png" {
            return Err(GltfError::new(format!(
                "unsupported texture mime type '{mime}'; only image/png is supported"
            )));
        }
        let view_index = image
            .get("bufferView")
            .and_then(json_usize)
            .ok_or_else(|| GltfError::new("image has no bufferView"))?;
        let png = buffer_view(self.json, self.binary, view_index, "image")?;
        if let Some((width, height)) = png_dimensions(png)
            && (width > MAX_PROP_TEXTURE_SIZE || height > MAX_PROP_TEXTURE_SIZE)
        {
            return Err(GltfError::new(format!(
                "texture is {width}x{height}; \
                 the prop limit is {MAX_PROP_TEXTURE_SIZE}x{MAX_PROP_TEXTURE_SIZE}"
            )));
        }
        let decoded = crate::loader::decode_png(png).map_err(|error| {
            GltfError::new(format!("embedded texture is not a valid PNG: {error}"))
        })?;
        let index = u16::try_from(self.textures.len())
            .map_err(|_| GltfError::new("prop model has too many textures"))?;
        self.textures.push(decoded);
        self.texture_of_image.insert(image_index, index);
        Ok(index)
    }
}

/// One document's retained node hierarchy plus the lazily resolved skins.
///
/// Built only for documents that declare a skin or animations: a pure static
/// model keeps the historical flatten-and-bake path and never materialises a
/// node list.
struct Rig {
    /// Every node, indexed by node index.
    nodes: Vec<PropNode>,
    /// Rest global transform per node.
    globals: Vec<Mat4>,
    /// Resolved skins by skin index; a declared-but-unused skin stays `None`.
    skins: Vec<Option<ResolvedSkin>>,
    /// The one skin the model uses, set by the first skinned primitive.
    used_skin: Option<usize>,
    /// The one mesh node a skinned primitive hangs off.
    mesh_node: Option<u16>,
}

/// One skin resolved against the document's rest hierarchy.
struct ResolvedSkin {
    skin: PropSkin,
    /// Rest global transform per joint slot, parallel to
    /// [`PropSkin::joints`].
    joint_globals: Vec<Mat4>,
    /// Inverse of the skinned mesh node's rest global transform.
    mesh_global_inverse: Mat4,
}

impl ResolvedSkin {
    /// The single-shot bind matrices
    /// `meshInverseGlobal * jointGlobal(rest) * inverseBind` per joint slot.
    fn skin_matrices(&self) -> Vec<Mat4> {
        self.joint_globals
            .iter()
            .zip(self.skin.inverse_bind.iter())
            .map(|(joint, inverse_bind)| {
                self.mesh_global_inverse
                    .mul_mat4(joint)
                    .mul_mat4(inverse_bind)
            })
            .collect()
    }
}

impl Rig {
    /// Builds the retained hierarchy for one document.
    fn build(json: &serde_json::Value) -> Result<Self, GltfError> {
        let (nodes, locals) = build_nodes(json)?;
        let count = nodes.len();
        let parents: Vec<Option<usize>> = nodes
            .iter()
            .map(|node| node.parent.map(usize::from))
            .collect();
        let mut globals = vec![Mat4::IDENTITY; count];
        let mut state = vec![0u8; count];
        for index in 0..count {
            // Walk the ancestor chain, then compose it downwards. A node
            // already marked visiting is a cycle.
            let mut chain: Vec<usize> = Vec::new();
            let mut cursor = index;
            loop {
                match state.get(cursor).copied().unwrap_or(2) {
                    2 => break,
                    1 => {
                        return Err(GltfError::new(format!(
                            "the node hierarchy contains a cycle at node {cursor}; \
                             prop models must be acyclic"
                        )));
                    }
                    _ => {}
                }
                if let Some(slot) = state.get_mut(cursor) {
                    *slot = 1;
                }
                chain.push(cursor);
                match parents.get(cursor).copied().flatten() {
                    Some(parent) => cursor = parent,
                    None => break,
                }
            }
            for node_index in chain.into_iter().rev() {
                let local = locals.get(node_index).copied().unwrap_or(Mat4::IDENTITY);
                let global = parents
                    .get(node_index)
                    .copied()
                    .flatten()
                    .map_or(local, |parent| {
                        globals
                            .get(parent)
                            .copied()
                            .unwrap_or(Mat4::IDENTITY)
                            .mul_mat4(&local)
                    });
                if let Some(slot) = globals.get_mut(node_index) {
                    *slot = global;
                }
                if let Some(slot) = state.get_mut(node_index) {
                    *slot = 2;
                }
            }
        }
        let mut skins: Vec<Option<ResolvedSkin>> = Vec::with_capacity(list_len(json, "skins"));
        skins.resize_with(list_len(json, "skins"), || None);
        Ok(Self {
            nodes,
            globals,
            skins,
            used_skin: None,
            mesh_node: None,
        })
    }

    /// Resolves one skin on first use and returns it.
    fn resolve_skin(
        &mut self,
        index: usize,
        json: &serde_json::Value,
        binary: &[u8],
    ) -> Result<&ResolvedSkin, GltfError> {
        if self.skins.get(index).is_none() {
            return Err(GltfError::new(format!(
                "primitive references skin {index}, which does not exist"
            )));
        }
        if self.skins.get(index).is_some_and(Option::is_none) {
            let resolved = parse_skin(
                index,
                json,
                binary,
                &self.nodes,
                &self.globals,
                self.mesh_node,
            )?;
            if let Some(slot) = self.skins.get_mut(index) {
                *slot = Some(resolved);
            }
        }
        self.skins
            .get(index)
            .and_then(Option::as_ref)
            .ok_or_else(|| GltfError::new(format!("skin {index} could not be resolved")))
    }
}

/// Reads the document's node list into retained nodes and exact local
/// transforms.
fn build_nodes(json: &serde_json::Value) -> Result<(Vec<PropNode>, Vec<Mat4>), GltfError> {
    let nodes = json
        .get("nodes")
        .and_then(|value| value.as_array())
        .filter(|nodes| !nodes.is_empty())
        .ok_or_else(|| {
            GltfError::new("file has no nodes; a prop model needs a node with a mesh")
        })?;
    let count = nodes.len();
    let mut out: Vec<PropNode> = Vec::with_capacity(count);
    let mut locals: Vec<Mat4> = Vec::with_capacity(count);
    for (index, node) in nodes.iter().enumerate() {
        let local = node_transform(node, index)?;
        let (scale, rotation, translation) = local.to_scale_rotation_translation();
        if !translation.is_finite() || !rotation.is_finite() || !scale.is_finite() {
            return Err(GltfError::new(format!(
                "node {index} has a non-finite transform; \
                 check the authored matrix/TRS values"
            )));
        }
        let children = match node.get("children") {
            Some(value) => {
                let items = value.as_array().ok_or_else(|| {
                    GltfError::new(format!("node {index} children is not an array"))
                })?;
                let mut children = Vec::with_capacity(items.len());
                for child in items {
                    let child_index = json_usize(child).ok_or_else(|| {
                        GltfError::new(format!("node {index} has a non-numeric child index"))
                    })?;
                    if child_index >= count {
                        return Err(GltfError::new(format!(
                            "node references child {child_index}, which does not exist"
                        )));
                    }
                    children.push(
                        u16::try_from(child_index)
                            .map_err(|_| GltfError::new("node index does not fit in 16 bits"))?,
                    );
                }
                children
            }
            None => Vec::new(),
        };
        let name = node
            .get("name")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string();
        out.push(PropNode {
            name,
            parent: None,
            children,
            translation: translation.to_array(),
            rotation: rotation.to_array(),
            scale: scale.to_array(),
        });
        locals.push(local);
    }
    // A node's parent is the first node that lists it as a child.
    for parent in 0..count {
        let children = out.get(parent).map(|node| node.children.clone());
        let Some(children) = children else { continue };
        let parent_index = u16::try_from(parent)
            .map_err(|_| GltfError::new("node index does not fit in 16 bits"))?;
        for child in children {
            if let Some(slot) = out.get_mut(usize::from(child))
                && slot.parent.is_none()
            {
                slot.parent = Some(parent_index);
            }
        }
    }
    Ok((out, locals))
}

/// Resolves one skin against the retained hierarchy.
fn parse_skin(
    index: usize,
    json: &serde_json::Value,
    binary: &[u8],
    nodes: &[PropNode],
    globals: &[Mat4],
    mesh_node: Option<u16>,
) -> Result<ResolvedSkin, GltfError> {
    let skin_json = json
        .get("skins")
        .and_then(|value| value.as_array())
        .and_then(|list| list.get(index))
        .ok_or_else(|| GltfError::new(format!("skin {index} does not exist")))?;
    let joints_json = skin_json
        .get("joints")
        .and_then(|value| value.as_array())
        .ok_or_else(|| GltfError::new(format!("skin {index} has no joints list")))?;
    if joints_json.is_empty() {
        return Err(GltfError::new(format!(
            "skin {index} declares no joints; a skin needs at least one joint"
        )));
    }
    if joints_json.len() > MAX_PROP_JOINTS {
        return Err(GltfError::new(format!(
            "skin {index} declares {} joints; the engine ceiling is {MAX_PROP_JOINTS}",
            joints_json.len()
        )));
    }
    let mut joints: Vec<u16> = Vec::with_capacity(joints_json.len());
    for joint in joints_json {
        let node = json_usize(joint)
            .ok_or_else(|| GltfError::new(format!("skin {index} has a non-numeric joint")))?;
        if node >= nodes.len() {
            return Err(GltfError::new(format!(
                "skin {index} references joint node {node}, which does not exist"
            )));
        }
        joints.push(
            u16::try_from(node)
                .map_err(|_| GltfError::new("joint node index does not fit in 16 bits"))?,
        );
    }
    let inverse_bind = parse_inverse_bind(json, binary, skin_json, index, joints.len())?;
    let root = match skin_json.get("skeleton") {
        Some(value) => {
            let node = json_usize(value).ok_or_else(|| {
                GltfError::new(format!("skin {index} skeleton is not a node index"))
            })?;
            if node >= nodes.len() {
                return Err(GltfError::new(format!(
                    "skin {index} skeleton references node {node}, which does not exist"
                )));
            }
            Some(
                u16::try_from(node)
                    .map_err(|_| GltfError::new("node index does not fit in 16 bits"))?,
            )
        }
        None => joints
            .first()
            .copied()
            .and_then(|joint| topmost_ancestor(usize::from(joint), nodes)),
    };
    let mut joint_globals: Vec<Mat4> = Vec::with_capacity(joints.len());
    for joint in &joints {
        let global = globals
            .get(usize::from(*joint))
            .copied()
            .ok_or_else(|| GltfError::new(format!("skin {index} joint node is out of range")))?;
        joint_globals.push(global);
    }
    let mesh_global = match mesh_node {
        Some(node) => globals
            .get(usize::from(node))
            .copied()
            .ok_or_else(|| GltfError::new("the skinned mesh node is out of range"))?,
        None => Mat4::IDENTITY,
    };
    let determinant = mesh_global.determinant();
    if !determinant.is_finite() || determinant.abs() < 1.0e-12 {
        return Err(GltfError::new(
            "the skinned mesh node has a singular rest transform; \
             the bind pose cannot be inverted",
        ));
    }
    let mesh_global_inverse = mesh_global.inverse();
    Ok(ResolvedSkin {
        skin: PropSkin {
            joints,
            inverse_bind,
            nodes: nodes.to_vec(),
            root,
            mesh_node,
        },
        joint_globals,
        mesh_global_inverse,
    })
}

/// Reads and validates a skin's `inverseBindMatrices` accessor.
fn parse_inverse_bind(
    json: &serde_json::Value,
    binary: &[u8],
    skin_json: &serde_json::Value,
    index: usize,
    joint_count: usize,
) -> Result<Vec<Mat4>, GltfError> {
    let accessor = skin_json
        .get("inverseBindMatrices")
        .and_then(json_usize)
        .ok_or_else(|| {
            GltfError::new(format!(
                "skin {index} has no inverseBindMatrices; a skinned model needs bind matrices"
            ))
        })?;
    let raw_matrices = read_vec(json, binary, accessor, 16).map_err(|error| {
        GltfError::new(format!(
            "skin {index} inverseBindMatrices is invalid: {}",
            error.0
        ))
    })?;
    if raw_matrices.len() != joint_count {
        return Err(GltfError::new(format!(
            "skin {index} declares {joint_count} joints but {} inverse bind matrices",
            raw_matrices.len()
        )));
    }
    let mut inverse_bind: Vec<Mat4> = Vec::with_capacity(raw_matrices.len());
    for values in &raw_matrices {
        let columns: [f32; 16] = components(values)?;
        if columns.iter().any(|value| !value.is_finite()) {
            return Err(GltfError::new(format!(
                "skin {index} inverseBindMatrices contains a non-finite value"
            )));
        }
        inverse_bind.push(Mat4::from_cols_array(&columns));
    }
    Ok(inverse_bind)
}

/// The topmost ancestor of `index`, or `None` for an out-of-range node.
fn topmost_ancestor(index: usize, nodes: &[PropNode]) -> Option<u16> {
    let mut cursor = index;
    let mut hops = nodes.len();
    while hops > 0 {
        let node = nodes.get(cursor)?;
        match node.parent {
            Some(parent) => cursor = usize::from(parent),
            None => {
                return u16::try_from(cursor).ok();
            }
        }
        hops = hops.saturating_sub(1);
    }
    u16::try_from(index).ok()
}

/// Parses every animation clip in the document.
///
/// Only LINEAR and STEP samplers are accepted; CUBICSPLINE and morph-target
/// weight channels are refused by name. Every channel is validated up front
/// (strictly increasing finite times, finite values, matching counts) so the
/// sampler can index it without further checks.
fn parse_animations(
    json: &serde_json::Value,
    binary: &[u8],
) -> Result<Vec<PropAnimation>, GltfError> {
    let Some(list) = json
        .get("animations")
        .and_then(|value| value.as_array())
        .filter(|list| !list.is_empty())
    else {
        return Ok(Vec::new());
    };
    let node_count = json
        .get("nodes")
        .and_then(|value| value.as_array())
        .map_or(0, std::vec::Vec::len);
    let mut animations: Vec<PropAnimation> = Vec::with_capacity(list.len());
    let mut channel_total = 0usize;
    for (animation_index, animation) in list.iter().enumerate() {
        let name = animation
            .get("name")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string();
        let Some(channels_json) = animation.get("channels").and_then(|value| value.as_array())
        else {
            return Err(GltfError::new(format!(
                "animation {animation_index} declares no channels"
            )));
        };
        channel_total = channel_total.saturating_add(channels_json.len());
        if channel_total > MAX_ANIMATION_CHANNELS {
            return Err(GltfError::new(format!(
                "prop model declares more than {MAX_ANIMATION_CHANNELS} animation channels; \
                 the engine ceiling is {MAX_ANIMATION_CHANNELS}"
            )));
        }
        let samplers = animation
            .get("samplers")
            .and_then(|value| value.as_array())
            .ok_or_else(|| {
                GltfError::new(format!("animation {animation_index} has no samplers"))
            })?;
        let mut channels: Vec<PropAnimationChannel> = Vec::with_capacity(channels_json.len());
        for channel in channels_json {
            channels.push(parse_animation_channel(
                channel,
                samplers,
                animation_index,
                node_count,
                json,
                binary,
            )?);
        }
        let duration = channels
            .iter()
            .map(PropAnimationChannel::duration)
            .fold(0.0f32, f32::max);
        animations.push(PropAnimation {
            name,
            duration,
            channels,
        });
    }
    Ok(animations)
}

/// Parses and validates one animation channel.
fn parse_animation_channel(
    channel: &serde_json::Value,
    samplers: &[serde_json::Value],
    animation_index: usize,
    node_count: usize,
    json: &serde_json::Value,
    binary: &[u8],
) -> Result<PropAnimationChannel, GltfError> {
    let target = channel
        .get("target")
        .ok_or_else(|| GltfError::new("animation channel has no target"))?;
    let node = target
        .get("node")
        .and_then(json_usize)
        .ok_or_else(|| GltfError::new("animation channel target has no node"))?;
    if node >= node_count {
        return Err(GltfError::new(format!(
            "animation channel targets node {node}, which does not exist"
        )));
    }
    let path = match target.get("path").and_then(|value| value.as_str()) {
        Some("translation") => AnimationPath::Translation,
        Some("rotation") => AnimationPath::Rotation,
        Some("scale") => AnimationPath::Scale,
        Some("weights") => {
            return Err(GltfError::new(
                "morph-target animation channels are not supported",
            ));
        }
        Some(other) => {
            return Err(GltfError::new(format!(
                "animation path '{other}' is not supported"
            )));
        }
        None => return Err(GltfError::new("animation channel target has no path")),
    };
    let sampler_index = channel
        .get("sampler")
        .and_then(json_usize)
        .ok_or_else(|| GltfError::new("animation channel has no sampler index"))?;
    let sampler = samplers.get(sampler_index).ok_or_else(|| {
        GltfError::new(format!(
            "animation channel references sampler {sampler_index}, which does not exist"
        ))
    })?;
    let interpolation = match sampler
        .get("interpolation")
        .and_then(|value| value.as_str())
    {
        None | Some("LINEAR") => AnimationInterpolation::Linear,
        Some("STEP") => AnimationInterpolation::Step,
        Some("CUBICSPLINE") => {
            return Err(GltfError::new(
                "CUBICSPLINE animation samplers are not supported; \
                 use LINEAR or STEP",
            ));
        }
        Some(other) => {
            return Err(GltfError::new(format!(
                "animation interpolation '{other}' is not supported"
            )));
        }
    };
    let times = parse_animation_times(json, binary, sampler, animation_index)?;
    let stride = path.stride();
    let output_accessor = sampler
        .get("output")
        .and_then(json_usize)
        .ok_or_else(|| GltfError::new("animation sampler has no output accessor"))?;
    let output_view = accessor_view(json, binary, output_accessor, stride)?;
    if output_view.component_type != COMPONENT_FLOAT {
        return Err(GltfError::new("animation sampler values must be float32"));
    }
    let raw_values = read_vec(json, binary, output_accessor, stride)?;
    if raw_values.len() != times.len() {
        return Err(GltfError::new(format!(
            "animation sampler has {} keyframes but {} value tuples",
            times.len(),
            raw_values.len()
        )));
    }
    let mut values: Vec<f32> = Vec::with_capacity(raw_values.len().saturating_mul(stride));
    for raw in &raw_values {
        for value in raw {
            if !value.is_finite() {
                return Err(GltfError::new(
                    "animation sampler contains a non-finite value",
                ));
            }
            values.push(*value);
        }
    }
    Ok(PropAnimationChannel {
        node: u16::try_from(node)
            .map_err(|_| GltfError::new("animation node index does not fit in 16 bits"))?,
        path,
        interpolation,
        times,
        values,
    })
}

/// Reads and validates a sampler's keyframe times: float32, finite and
/// strictly increasing.
fn parse_animation_times(
    json: &serde_json::Value,
    binary: &[u8],
    sampler: &serde_json::Value,
    animation_index: usize,
) -> Result<Vec<f32>, GltfError> {
    let input_accessor = sampler
        .get("input")
        .and_then(json_usize)
        .ok_or_else(|| GltfError::new("animation sampler has no input accessor"))?;
    let input_view = accessor_view(json, binary, input_accessor, 1)?;
    if input_view.component_type != COMPONENT_FLOAT {
        return Err(GltfError::new(
            "animation sampler input times must be float32",
        ));
    }
    let raw_times = read_vec(json, binary, input_accessor, 1)?;
    if raw_times.is_empty() {
        return Err(GltfError::new(format!(
            "animation {animation_index} sampler has no keyframes"
        )));
    }
    let mut times: Vec<f32> = Vec::with_capacity(raw_times.len());
    for raw in &raw_times {
        let time = raw.first().copied().ok_or_else(|| {
            GltfError::new("animation keyframe time accessor declares no components")
        })?;
        if !time.is_finite() {
            return Err(GltfError::new(
                "animation sampler contains a non-finite keyframe time",
            ));
        }
        times.push(time);
    }
    for pair in times.windows(2) {
        let [before, after] = pair else { continue };
        if after <= before {
            return Err(GltfError::new(
                "animation keyframe times must be strictly increasing",
            ));
        }
    }
    Ok(times)
}

/// The vertex attribute lists one primitive reads, before assembly.
struct PrimitiveAttributes {
    positions: Vec<Vec<f32>>,
    uvs: Vec<Vec<f32>>,
    colors: Vec<Vec<f32>>,
    /// `JOINTS_0` joint slots, when the primitive declares them.
    joints: Option<Vec<[u16; 4]>>,
    /// `WEIGHTS_0` joint weights, when the primitive declares them.
    weights: Option<Vec<[f32; 4]>>,
}

/// Reads one primitive's `POSITION`, `TEXCOORD_0`, `COLOR_0` and optional
/// `JOINTS_0`/`WEIGHTS_0` attributes.
///
/// `COLOR_0` stays optional (default white); `TEXCOORD_0` is required because
/// every prop is UV mapped. Joint attributes are read raw here; whether the
/// primitive is skinned, and whether the joint slots really address its skin,
/// is decided by the caller.
fn read_attributes(
    json: &serde_json::Value,
    binary: &[u8],
    primitive: &serde_json::Value,
    mesh_index: usize,
) -> Result<PrimitiveAttributes, GltfError> {
    let attributes = primitive
        .get("attributes")
        .and_then(|value| value.as_object())
        .ok_or_else(|| {
            GltfError::new(format!(
                "mesh {mesh_index} has a primitive with no attributes"
            ))
        })?;
    let positions = read_vec(json, binary, attribute(attributes, "POSITION")?, 3)?;
    let Some(uv_accessor) = attributes.get("TEXCOORD_0") else {
        return Err(GltfError::new(
            "primitive has no TEXCOORD_0; every prop vertex must be UV mapped",
        ));
    };
    let uvs = read_vec(json, binary, accessor_index(uv_accessor, "TEXCOORD_0")?, 2)?;
    let colors = match attributes.get("COLOR_0") {
        Some(value) => read_vec(json, binary, accessor_index(value, "COLOR_0")?, 4)?,
        None => vec![vec![1.0, 1.0, 1.0, 1.0]; positions.len()],
    };
    let joints = match attributes.get("JOINTS_0") {
        Some(value) => Some(read_joints(
            json,
            binary,
            accessor_index(value, "JOINTS_0")?,
        )?),
        None => None,
    };
    let weights = match attributes.get("WEIGHTS_0") {
        Some(value) => Some(read_weights(
            json,
            binary,
            accessor_index(value, "WEIGHTS_0")?,
        )?),
        None => None,
    };
    if positions.len() != uvs.len() || positions.len() != colors.len() {
        return Err(GltfError::new(
            "POSITION, TEXCOORD_0 and COLOR_0 attribute counts differ",
        ));
    }
    if joints
        .as_ref()
        .is_some_and(|joints| joints.len() != positions.len())
        || weights
            .as_ref()
            .is_some_and(|weights| weights.len() != positions.len())
    {
        return Err(GltfError::new(
            "POSITION, JOINTS_0 and WEIGHTS_0 attribute counts differ",
        ));
    }
    Ok(PrimitiveAttributes {
        positions,
        uvs,
        colors,
        joints,
        weights,
    })
}

/// Reads `JOINTS_0`: four unsigned byte or unsigned short joint slots per
/// vertex, never normalised.
fn read_joints(
    json: &serde_json::Value,
    binary: &[u8],
    index: usize,
) -> Result<Vec<[u16; 4]>, GltfError> {
    let view = accessor_view(json, binary, index, 4)?;
    if view.normalized {
        return Err(GltfError::new(
            "JOINTS_0 must not be normalised; joint indices are plain integers",
        ));
    }
    let component_size = view
        .element_size
        .checked_div(view.components)
        .ok_or_else(|| GltfError::new("JOINTS_0 accessor has no components"))?;
    let mut out = Vec::with_capacity(view.count);
    for element in 0..view.count {
        let base = element
            .checked_mul(view.stride)
            .ok_or_else(|| GltfError::new("JOINTS_0 element offset overflows"))?;
        let mut values = [0u16; 4];
        for (component, slot) in values.iter_mut().enumerate() {
            let offset = component
                .checked_mul(component_size)
                .and_then(|skip| base.checked_add(skip))
                .ok_or_else(|| GltfError::new("JOINTS_0 component offset overflows"))?;
            *slot = match view.component_type {
                COMPONENT_UBYTE => u16::from(
                    view.data
                        .get(offset)
                        .copied()
                        .ok_or_else(|| GltfError::new("JOINTS_0 data is truncated"))?,
                ),
                COMPONENT_USHORT => read_u16_le(view.data, offset)?,
                other => {
                    return Err(GltfError::new(format!(
                        "JOINTS_0 must use unsigned byte or unsigned short components, \
                         not componentType {other}"
                    )));
                }
            };
        }
        out.push(values);
    }
    Ok(out)
}

/// Reads `WEIGHTS_0`: four float32 or normalised integer joint weights per
/// vertex.
fn read_weights(
    json: &serde_json::Value,
    binary: &[u8],
    index: usize,
) -> Result<Vec<[f32; 4]>, GltfError> {
    let view = accessor_view(json, binary, index, 4)?;
    let integer = matches!(view.component_type, COMPONENT_UBYTE | COMPONENT_USHORT);
    if integer && !view.normalized {
        return Err(GltfError::new(
            "WEIGHTS_0 integer components must be normalised; \
             use float32 or a normalised integer accessor",
        ));
    }
    if !integer && view.component_type != COMPONENT_FLOAT {
        return Err(GltfError::new(format!(
            "WEIGHTS_0 must use float32 or normalised integer components, \
             not componentType {}",
            view.component_type
        )));
    }
    let component_size = view
        .element_size
        .checked_div(view.components)
        .ok_or_else(|| GltfError::new("WEIGHTS_0 accessor has no components"))?;
    let mut out = Vec::with_capacity(view.count);
    for element in 0..view.count {
        let base = element
            .checked_mul(view.stride)
            .ok_or_else(|| GltfError::new("WEIGHTS_0 element offset overflows"))?;
        let mut values = [0.0f32; 4];
        for (component, slot) in values.iter_mut().enumerate() {
            let offset = component
                .checked_mul(component_size)
                .and_then(|skip| base.checked_add(skip))
                .ok_or_else(|| GltfError::new("WEIGHTS_0 component offset overflows"))?;
            *slot = match view.component_type {
                COMPONENT_FLOAT => read_f32_le(view.data, offset)?,
                COMPONENT_UBYTE => {
                    f32::from(
                        view.data
                            .get(offset)
                            .copied()
                            .ok_or_else(|| GltfError::new("WEIGHTS_0 data is truncated"))?,
                    ) / 255.0
                }
                COMPONENT_USHORT => f32::from(read_u16_le(view.data, offset)?) / 65_535.0,
                other => {
                    return Err(GltfError::new(format!(
                        "WEIGHTS_0 componentType {other} is not supported"
                    )));
                }
            };
        }
        out.push(values);
    }
    Ok(out)
}

/// One primitive's index list; an unindexed primitive becomes `0..vertex_count`.
fn primitive_indices(
    json: &serde_json::Value,
    binary: &[u8],
    primitive: &serde_json::Value,
    vertex_count: usize,
) -> Result<Vec<u32>, GltfError> {
    match primitive.get("indices") {
        Some(value) => read_indices(json, binary, accessor_index(value, "indices")?),
        None => (0..vertex_count)
            .map(|value| {
                u32::try_from(value)
                    .map_err(|_| GltfError::new("mesh index does not fit in 32 bits"))
            })
            .collect::<Result<Vec<u32>, _>>(),
    }
}

/// Appends one primitive's transformed vertices, with the material colour
/// multiplied into every baked vertex colour.
fn append_vertices(
    vertices: &mut Vec<PropVertex>,
    attributes: &PrimitiveAttributes,
    transform: &Mat4,
    color: [f32; 4],
) -> Result<(), GltfError> {
    let positions = &attributes.positions;
    for ((position, uv), vertex_color) in positions
        .iter()
        .zip(attributes.uvs.iter())
        .zip(attributes.colors.iter())
    {
        let [x, y, z]: [f32; 3] = components(position)?;
        let point = transform.transform_point3(Vec3::new(x, y, z));
        let [red, green, blue, alpha]: [f32; 4] = components(vertex_color)?;
        vertices.push(PropVertex {
            pos: [point.x, point.y, point.z],
            color: [
                red * color[0],
                green * color[1],
                blue * color[2],
                alpha * color[3],
            ],
            uv: components(uv)?,
        });
    }
    Ok(())
}

/// Appends one skinned primitive's vertices, baked to the bind pose.
///
/// Each vertex's position is the weight-blended
/// `meshInverseGlobal * jointGlobal(rest) * inverseBind * p`, which is the
/// rest pose the static prop path draws and the pose the character path starts
/// from. Joint slots are validated against the primitive's skin and weights
/// are validated (finite, non-negative, positive sum) and renormalised, so the
/// stored weights always sum to one.
#[allow(clippy::arithmetic_side_effects)] // f32 blend of finite, validated values
fn append_skinned_vertices(
    vertices: &mut Vec<PropVertex>,
    joints: &mut Vec<[u16; 4]>,
    weights: &mut Vec<[f32; 4]>,
    attributes: &PrimitiveAttributes,
    skin_matrices: &[Mat4],
    color: [f32; 4],
) -> Result<(), GltfError> {
    let (Some(raw_joints), Some(raw_weights)) =
        (attributes.joints.as_ref(), attributes.weights.as_ref())
    else {
        return Err(GltfError::new(
            "a skinned primitive is missing its JOINTS_0/WEIGHTS_0 attributes",
        ));
    };
    let joint_total = u16::try_from(skin_matrices.len()).unwrap_or(u16::MAX);
    for (((position, uv), vertex_color), (joint_slots, weight_values)) in attributes
        .positions
        .iter()
        .zip(attributes.uvs.iter())
        .zip(attributes.colors.iter())
        .zip(raw_joints.iter().zip(raw_weights.iter()))
    {
        let [x, y, z]: [f32; 3] = components(position)?;
        let point = Vec3::new(x, y, z);
        let mut sum = 0.0f32;
        for (slot, weight) in joint_slots.iter().zip(weight_values.iter()) {
            if *slot >= joint_total {
                return Err(GltfError::new(format!(
                    "JOINTS_0 slot {slot} is outside the skin's {joint_total} joints"
                )));
            }
            if !weight.is_finite() || *weight < 0.0 {
                return Err(GltfError::new(
                    "WEIGHTS_0 contains a non-finite or negative weight; the asset is malformed",
                ));
            }
            sum += weight;
        }
        if !sum.is_finite() || sum <= 0.0 {
            return Err(GltfError::new(
                "WEIGHTS_0 sums to zero; every skinned vertex needs at least one weight",
            ));
        }
        let mut blended = Vec3::ZERO;
        let mut normalized = [0.0f32; 4];
        for ((slot, weight), stored) in joint_slots
            .iter()
            .zip(weight_values.iter())
            .zip(normalized.iter_mut())
        {
            let normalized_weight = weight / sum;
            *stored = normalized_weight;
            if let Some(matrix) = skin_matrices.get(usize::from(*slot)) {
                blended += matrix.transform_point3(point) * normalized_weight;
            }
        }
        let [red, green, blue, alpha]: [f32; 4] = components(vertex_color)?;
        vertices.push(PropVertex {
            pos: [blended.x, blended.y, blended.z],
            color: [
                red * color[0],
                green * color[1],
                blue * color[2],
                alpha * color[3],
            ],
            uv: components(uv)?,
        });
        joints.push(*joint_slots);
        weights.push(normalized);
    }
    Ok(())
}

/// Appends one primitive's indices, offset by the vertices already assembled.
fn append_indices(
    indices: &mut Vec<u16>,
    local_indices: &[u32],
    base: usize,
    vertex_count: usize,
) -> Result<(), GltfError> {
    let limit = base.saturating_add(vertex_count);
    for value in local_indices {
        let absolute = usize::try_from(*value)
            .ok()
            .and_then(|value| base.checked_add(value))
            .ok_or_else(|| GltfError::new(format!("index {value} points outside the mesh")))?;
        if absolute >= limit {
            return Err(GltfError::new(format!(
                "index {value} points outside the primitive's vertices"
            )));
        }
        let index = u16::try_from(absolute).map_err(|_| {
            GltfError::new(format!(
                "prop model needs more than {MAX_PROP_VERTICES} vertices; \
                 lower the prop's detail"
            ))
        })?;
        indices.push(index);
    }
    Ok(())
}

/// The local transform a node declares, from `matrix` or TRS.
///
/// glTF stores matrices column-major, which is also `glam`'s convention, and
/// TRS composes as `translation * rotation * scale`.
fn node_transform(node: &serde_json::Value, index: usize) -> Result<Mat4, GltfError> {
    let matrix = node.get("matrix");
    let has_trs = node.get("translation").is_some()
        || node.get("rotation").is_some()
        || node.get("scale").is_some();
    if matrix.is_some() && has_trs {
        return Err(GltfError::new(format!(
            "node {index} declares both matrix and TRS; use one or the other"
        )));
    }
    let transform = if let Some(matrix) = matrix {
        let values = numeric_array::<16>(matrix, &format!("node {index} matrix"))?;
        Mat4::from_cols_array(&values)
    } else {
        let translation = match node.get("translation") {
            Some(value) => numeric_array::<3>(value, &format!("node {index} translation"))?,
            None => [0.0; 3],
        };
        let rotation = match node.get("rotation") {
            Some(value) => numeric_array::<4>(value, &format!("node {index} rotation"))?,
            None => [0.0, 0.0, 0.0, 1.0],
        };
        let scale = match node.get("scale") {
            Some(value) => numeric_array::<3>(value, &format!("node {index} scale"))?,
            None => [1.0; 3],
        };
        let rotation = Quat::from_xyzw(rotation[0], rotation[1], rotation[2], rotation[3]);
        Mat4::from_scale_rotation_translation(Vec3::from(scale), rotation, Vec3::from(translation))
    };
    for value in transform.to_cols_array() {
        if !value.is_finite() {
            return Err(GltfError::new(format!(
                "node {index} has a non-finite transform; \
                 check the authored matrix/TRS values"
            )));
        }
    }
    Ok(transform)
}

/// Copies a JSON array of exactly `N` numbers into an `f32` array.
fn numeric_array<const N: usize>(
    value: &serde_json::Value,
    label: &str,
) -> Result<[f32; N], GltfError> {
    let items = value
        .as_array()
        .ok_or_else(|| GltfError::new(format!("{label} must be an array of {N} numbers")))?;
    if items.len() != N {
        return Err(GltfError::new(format!(
            "{label} must be an array of {N} numbers, found {}",
            items.len()
        )));
    }
    let mut out = [0.0f32; N];
    for (slot, item) in out.iter_mut().zip(items) {
        *slot = json_f32(item).ok_or_else(|| {
            GltfError::new(format!("{label} contains a value that is not a number"))
        })?;
    }
    Ok(out)
}

/// `f32` value of a JSON number, or `None` when it is absent or not a number.
///
/// glTF scalar values are authored as JSON numbers and consumed as `f32` by
/// the renderer, so the narrowing cast is the specified conversion; values
/// beyond `f32`'s range become infinite and are rejected by the finiteness
/// checks at the call sites.
fn json_f32(value: &serde_json::Value) -> Option<f32> {
    #[allow(clippy::cast_possible_truncation)] // glTF scalars are f32 by definition
    let value = value.as_f64()? as f32;
    Some(value)
}

/// Width and height read straight out of a PNG's `IHDR` header.
///
/// Checking the header first lets an oversized texture be rejected by name
/// before the decoder allocates anything for it.
fn png_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return None;
    }
    if bytes.get(12..16) != Some(b"IHDR".as_slice()) {
        return None;
    }
    let width = read_u32_be(bytes, 16).ok()?;
    let height = read_u32_be(bytes, 20).ok()?;
    Some((width, height))
}

/// The binary slice one bufferView addresses.
fn buffer_view<'a>(
    json: &serde_json::Value,
    binary: &'a [u8],
    index: usize,
    what: &str,
) -> Result<&'a [u8], GltfError> {
    let views = json
        .get("bufferViews")
        .and_then(|value| value.as_array())
        .ok_or_else(|| GltfError::new("file has no bufferViews"))?;
    let view = views
        .get(index)
        .ok_or_else(|| GltfError::new(format!("{what} bufferView {index} does not exist")))?;
    let offset = view.get("byteOffset").and_then(json_usize).unwrap_or(0);
    let length = view
        .get("byteLength")
        .and_then(json_usize)
        .ok_or_else(|| GltfError::new(format!("{what} bufferView has no byteLength")))?;
    let end = offset
        .checked_add(length)
        .ok_or_else(|| GltfError::new(format!("{what} bufferView length overflows")))?;
    if end > binary.len() {
        return Err(GltfError::new(format!(
            "{what} bufferView extends past the binary chunk; the GLB is truncated"
        )));
    }
    binary
        .get(offset..end)
        .ok_or_else(|| GltfError::new(format!("{what} bufferView is out of range")))
}

/// Copies the first `N` values out of a decoded component list.
///
/// [`read_vec`] always returns exactly the requested number of components, so
/// this only fails for a caller that asked for more components than the
/// accessor declares.
fn components<const N: usize>(values: &[f32]) -> Result<[f32; N], GltfError> {
    values
        .get(..N)
        .and_then(|slice| <[f32; N]>::try_from(slice).ok())
        .ok_or_else(|| {
            GltfError::new("accessor declares fewer components than the attribute needs")
        })
}

/// Checks the assembled mesh against the prop budgets and data invariants.
///
/// Returns the triangle count on success, so the caller can store it without
/// recomputing it from the index list.
fn validate_mesh(vertices: &[PropVertex], indices: &[u16]) -> Result<usize, GltfError> {
    if vertices.is_empty() || indices.is_empty() {
        return Err(GltfError::new(
            "prop model contains no triangles; a prop needs a triangle mesh",
        ));
    }
    if vertices.len() > MAX_PROP_VERTICES {
        return Err(GltfError::new(format!(
            "prop model has {} vertices; the engine ceiling is {MAX_PROP_VERTICES}",
            vertices.len()
        )));
    }
    let triangles = indices.len() / 3;
    if triangles > MAX_PROP_TRIANGLES {
        return Err(GltfError::new(format!(
            "prop model has {triangles} triangles; the engine ceiling is {MAX_PROP_TRIANGLES}"
        )));
    }
    for vertex in vertices {
        for value in vertex
            .pos
            .iter()
            .chain(vertex.uv.iter())
            .chain(vertex.color.iter())
        {
            if !value.is_finite() {
                return Err(GltfError::new(
                    "mesh contains a non-finite vertex value; the asset is malformed",
                ));
            }
        }
        if vertex.uv[0] < -0.01
            || vertex.uv[0] > 1.01
            || vertex.uv[1] < -0.01
            || vertex.uv[1] > 1.01
        {
            return Err(GltfError::new(format!(
                "UV {:.3},{:.3} lies outside 0..1; props use non-tiling UVs",
                vertex.uv[0], vertex.uv[1]
            )));
        }
    }
    Ok(triangles)
}

fn parse_container(bytes: &[u8]) -> Result<(serde_json::Value, Vec<u8>), GltfError> {
    if bytes.len() < 12 {
        return Err(GltfError::new("file is too small to be a GLB"));
    }
    let magic = read_u32_le(bytes, 0)?;
    let version = read_u32_le(bytes, 4)?;
    let declared_length = usize::try_from(read_u32_le(bytes, 8)?)
        .map_err(|_| GltfError::new("GLB declared length does not fit this target"))?;
    if magic != GLB_MAGIC {
        return Err(GltfError::new(
            "not a GLB file; prop models must be self-contained .glb assets",
        ));
    }
    if version != 2 {
        return Err(GltfError::new(format!(
            "unsupported glTF container version {version}; only glTF 2.0 is supported"
        )));
    }
    if declared_length > bytes.len() {
        return Err(GltfError::new("GLB header length exceeds the file size"));
    }

    let mut offset: usize = 12;
    let mut json: Option<serde_json::Value> = None;
    let mut binary: Vec<u8> = Vec::new();
    while offset
        .checked_add(8)
        .is_some_and(|header_end| header_end <= declared_length)
    {
        let length = usize::try_from(read_u32_le(bytes, offset)?)
            .map_err(|_| GltfError::new("GLB chunk length does not fit this target"))?;
        let kind_offset = offset
            .checked_add(4)
            .ok_or_else(|| GltfError::new("GLB chunk offset overflows"))?;
        let kind = read_u32_le(bytes, kind_offset)?;
        let start = offset
            .checked_add(8)
            .ok_or_else(|| GltfError::new("GLB chunk offset overflows"))?;
        let Some(end) = start.checked_add(length) else {
            return Err(GltfError::new("GLB chunk length overflows"));
        };
        if end > declared_length || end > bytes.len() {
            return Err(GltfError::new("GLB chunk is truncated"));
        }
        match kind {
            CHUNK_JSON => {
                let text = std::str::from_utf8(
                    bytes
                        .get(start..end)
                        .ok_or_else(|| GltfError::new("GLB chunk is truncated"))?,
                )
                .map_err(|_| GltfError::new("GLB JSON chunk is not valid UTF-8"))?;
                let value: serde_json::Value =
                    serde_json::from_str(text.trim_end_matches(['\0', ' ']))
                        .map_err(|error| GltfError::new(format!("Invalid glTF JSON: {error}")))?;
                json = Some(value);
            }
            CHUNK_BIN => {
                binary = bytes
                    .get(start..end)
                    .ok_or_else(|| GltfError::new("GLB chunk is truncated"))?
                    .to_vec();
            }
            _ => {}
        }
        offset = end;
    }

    let json = json.ok_or_else(|| GltfError::new("GLB has no JSON chunk"))?;
    Ok((json, binary))
}

/// Reads `N` bytes at `offset` as a fixed-size array.
///
/// Accessor bounds are validated before reading, but a truncated or malformed
/// asset must surface an error instead of a panic, so every read stays checked.
fn read_le_bytes<const N: usize>(data: &[u8], offset: usize) -> Result<[u8; N], GltfError> {
    let end = offset
        .checked_add(N)
        .ok_or_else(|| GltfError::new("accessor byte range overflows"))?;
    let slice = data
        .get(offset..end)
        .ok_or_else(|| GltfError::new("accessor data is truncated"))?;
    <[u8; N]>::try_from(slice).map_err(|_| GltfError::new("accessor data is truncated"))
}

/// Little-endian `u32` at `offset`, or an error when the data is truncated.
fn read_u32_le(data: &[u8], offset: usize) -> Result<u32, GltfError> {
    read_le_bytes::<4>(data, offset).map(u32::from_le_bytes)
}

/// Big-endian `u32` at `offset`, or an error when the data is truncated.
fn read_u32_be(data: &[u8], offset: usize) -> Result<u32, GltfError> {
    read_le_bytes::<4>(data, offset).map(u32::from_be_bytes)
}

/// Little-endian `u16` at `offset`, or an error when the data is truncated.
fn read_u16_le(data: &[u8], offset: usize) -> Result<u16, GltfError> {
    read_le_bytes::<2>(data, offset).map(u16::from_le_bytes)
}

/// Little-endian `f32` at `offset`, or an error when the data is truncated.
fn read_f32_le(data: &[u8], offset: usize) -> Result<f32, GltfError> {
    read_le_bytes::<4>(data, offset).map(f32::from_le_bytes)
}

struct AccessorView<'a> {
    data: &'a [u8],
    stride: usize,
    element_size: usize,
    count: usize,
    component_type: u32,
    normalized: bool,
    components: usize,
}

/// `usize` value of a non-negative JSON integer, or `None` when the field is
/// absent, is not a number, or does not fit in this target's pointer width.
fn json_usize(value: &serde_json::Value) -> Option<usize> {
    usize::try_from(value.as_u64()?).ok()
}

/// `u32` value of a non-negative JSON integer, or `None` when the field is
/// absent, is not a number, or does not fit in 32 bits.
fn json_u32(value: &serde_json::Value) -> Option<u32> {
    u32::try_from(value.as_u64()?).ok()
}

fn accessor_view<'a>(
    json: &serde_json::Value,
    binary: &'a [u8],
    index: usize,
    components: usize,
) -> Result<AccessorView<'a>, GltfError> {
    let accessors = json
        .get("accessors")
        .and_then(|value| value.as_array())
        .ok_or_else(|| GltfError::new("file has no accessors"))?;
    let accessor = accessors
        .get(index)
        .ok_or_else(|| GltfError::new(format!("accessor {index} does not exist")))?;
    if accessor.get("sparse").is_some() {
        return Err(GltfError::new(format!(
            "accessor {index} is sparse; sparse accessors are not supported"
        )));
    }

    let declared_components = accessor_components(accessor);
    if declared_components != components {
        return Err(GltfError::new(format!(
            "accessor {index} has {declared_components} components; expected {components}"
        )));
    }
    let component_type = accessor
        .get("componentType")
        .and_then(json_u32)
        .ok_or_else(|| GltfError::new(format!("accessor {index} has no componentType")))?;
    let component_size = component_size(component_type)?;
    let count = accessor
        .get("count")
        .and_then(json_usize)
        .ok_or_else(|| GltfError::new(format!("accessor {index} has no count")))?;
    let element_size = component_size
        .checked_mul(components)
        .ok_or_else(|| GltfError::new("accessor element size overflows"))?;
    let (data, stride) = accessor_data(json, binary, accessor, index, element_size, count)?;

    Ok(AccessorView {
        data,
        stride,
        element_size,
        count,
        component_type,
        normalized: accessor
            .get("normalized")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        components,
    })
}

/// Number of components an accessor's `type` string declares.
fn accessor_components(accessor: &serde_json::Value) -> usize {
    match accessor.get("type").and_then(|value| value.as_str()) {
        Some("VEC2") => 2,
        Some("VEC3") => 3,
        Some("VEC4") => 4,
        Some("SCALAR") => 1,
        Some("MAT4") => 16,
        // A missing or unknown type is reported by the caller's size check as
        // zero components, which no accessor request can match.
        _ => 0,
    }
}

/// Bytes per element of an accessor component type.
fn component_size(component_type: u32) -> Result<usize, GltfError> {
    match component_type {
        COMPONENT_FLOAT | COMPONENT_UINT => Ok(4),
        COMPONENT_UBYTE => Ok(1),
        COMPONENT_USHORT => Ok(2),
        other => Err(GltfError::new(format!(
            "unsupported accessor componentType {other}"
        ))),
    }
}

/// The binary slice an accessor addresses, plus its element stride in bytes.
fn accessor_data<'a>(
    json: &serde_json::Value,
    binary: &'a [u8],
    accessor: &serde_json::Value,
    index: usize,
    element_size: usize,
    count: usize,
) -> Result<(&'a [u8], usize), GltfError> {
    let view_index = accessor
        .get("bufferView")
        .and_then(json_usize)
        .ok_or_else(|| GltfError::new(format!("accessor {index} has no bufferView")))?;
    let views = json
        .get("bufferViews")
        .and_then(|value| value.as_array())
        .ok_or_else(|| GltfError::new("file has no bufferViews"))?;
    let view = views
        .get(view_index)
        .ok_or_else(|| GltfError::new(format!("bufferView {view_index} does not exist")))?;

    let view_offset = view.get("byteOffset").and_then(json_usize).unwrap_or(0);
    let view_length = view
        .get("byteLength")
        .and_then(json_usize)
        .ok_or_else(|| GltfError::new("bufferView has no byteLength"))?;
    let accessor_offset = accessor.get("byteOffset").and_then(json_usize).unwrap_or(0);
    let start = view_offset
        .checked_add(accessor_offset)
        .ok_or_else(|| GltfError::new("accessor byte offset overflows"))?;
    let end = start
        .checked_add(view_length)
        .ok_or_else(|| GltfError::new("bufferView length overflows"))?;
    if end > binary.len() {
        return Err(GltfError::new(
            "bufferView extends past the end of the binary chunk; the GLB is truncated",
        ));
    }

    let stride = view
        .get("byteStride")
        .and_then(json_usize)
        .unwrap_or(element_size);
    // A zero stride is only meaningful for a single element (the GLB spec
    // requires >= 4 when `byteStride` is authored at all). With more than one
    // element it would make the size check below pass for any `count`, so it is
    // rejected here rather than trusted.
    if stride == 0 && count > 1 {
        return Err(GltfError::new(format!(
            "accessor {index} declares a zero byteStride with {count} elements"
        )));
    }
    let required = if count == 0 {
        // No elements are read, so even an empty bufferView is acceptable.
        0
    } else {
        count
            .saturating_sub(1)
            .checked_mul(stride)
            .and_then(|size| size.checked_add(element_size))
            .ok_or_else(|| GltfError::new("accessor byte length overflows"))?
    };
    // `end == start + view_length`, so the view length is the budget the
    // accessor's elements must fit in.
    if required > view_length {
        return Err(GltfError::new(format!(
            "accessor {index} declares {count} elements but its bufferView is too small"
        )));
    }
    // Belt and braces against a count that the element size cannot physically
    // fit, so no reader can reserve or loop past the buffer it was given.
    let element_size = element_size.max(1);
    #[allow(clippy::arithmetic_side_effects)] // `element_size >= 1` is checked above
    let max_count = view_length / element_size;
    if count > max_count {
        return Err(GltfError::new(format!(
            "accessor {index} declares {count} elements but its bufferView holds at most {max_count}"
        )));
    }
    Ok((
        binary
            .get(start..end)
            .ok_or_else(|| GltfError::new("accessor data is truncated"))?,
        stride,
    ))
}

fn read_vec(
    json: &serde_json::Value,
    binary: &[u8],
    index: usize,
    components: usize,
) -> Result<Vec<Vec<f32>>, GltfError> {
    let view = accessor_view(json, binary, index, components)?;
    let component_size = view
        .element_size
        .checked_div(view.components)
        .ok_or_else(|| GltfError::new("accessor has no components"))?;
    let mut out = Vec::with_capacity(view.count);
    for element in 0..view.count {
        let base = element
            .checked_mul(view.stride)
            .ok_or_else(|| GltfError::new("accessor element offset overflows"))?;
        let mut values = Vec::with_capacity(view.components);
        for component in 0..view.components {
            let offset = component
                .checked_mul(component_size)
                .and_then(|skip| base.checked_add(skip))
                .ok_or_else(|| GltfError::new("accessor component offset overflows"))?;
            let value = match view.component_type {
                COMPONENT_FLOAT => read_f32_le(view.data, offset)?,
                COMPONENT_UBYTE => {
                    let raw = view
                        .data
                        .get(offset)
                        .copied()
                        .ok_or_else(|| GltfError::new("accessor data is truncated"))?;
                    if view.normalized {
                        f32::from(raw) / 255.0
                    } else {
                        f32::from(raw)
                    }
                }
                COMPONENT_USHORT => {
                    let raw = read_u16_le(view.data, offset)?;
                    if view.normalized {
                        f32::from(raw) / 65_535.0
                    } else {
                        f32::from(raw)
                    }
                }
                other => {
                    return Err(GltfError::new(format!(
                        "componentType {other} cannot be used for vertex attributes"
                    )));
                }
            };
            values.push(value);
        }
        out.push(values);
    }
    Ok(out)
}

fn read_indices(
    json: &serde_json::Value,
    binary: &[u8],
    index: usize,
) -> Result<Vec<u32>, GltfError> {
    let view = accessor_view(json, binary, index, 1)?;
    let component_size = view.element_size;
    let mut out = Vec::with_capacity(view.count);
    for element in 0..view.count {
        let offset = element
            .checked_mul(view.stride)
            .ok_or_else(|| GltfError::new("accessor element offset overflows"))?;
        out.push(match view.component_type {
            COMPONENT_UBYTE => u32::from(
                view.data
                    .get(offset)
                    .copied()
                    .ok_or_else(|| GltfError::new("accessor data is truncated"))?,
            ),
            COMPONENT_USHORT => u32::from(read_u16_le(view.data, offset)?),
            COMPONENT_UINT => read_u32_le(view.data, offset)?,
            other => {
                return Err(GltfError::new(format!(
                    "componentType {other} cannot be used for indices (size {component_size})"
                )));
            }
        });
    }
    Ok(out)
}

fn attribute(
    attributes: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<usize, GltfError> {
    attributes
        .get(key)
        .ok_or_else(|| GltfError::new(format!("primitive has no {key} attribute")))
        .and_then(|value| accessor_index(value, key))
}

fn accessor_index(value: &serde_json::Value, key: &str) -> Result<usize, GltfError> {
    json_usize(value).ok_or_else(|| GltfError::new(format!("{key} is not an accessor index")))
}

#[cfg(test)]
mod tests;
