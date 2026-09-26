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
//! * `mode: 4` (triangles) only, no skins, no morph targets, no animation;
//! * up to [`crate::level::MAX_PROP_IMAGES`] PNG images embedded in
//!   bufferViews (self-contained, no external files or data URIs), each
//!   distinct image decoded once for the model;
//! * `pbrMetallicRoughness.baseColorFactor`, `emissiveFactor` and the
//!   `KHR_materials_emissive_strength` extension - the only glTF extension
//!   this reader understands.
//!
//! Everything else - skins, animations, morph targets, external or data-URI
//! images, sparse accessors, texture transforms and every other extension -
//! produces a descriptive [`GltfError`] so a malformed asset degrades into the
//! loader's placeholder box instead of panicking or looping.

use std::collections::{HashMap, HashSet};

use glam::{Mat4, Quat, Vec3};

use crate::level::{
    MAX_PROP_IMAGES, MAX_PROP_MATERIALS, MAX_PROP_PRIMITIVES, MAX_PROP_TEXTURE_SIZE,
    MAX_PROP_TRIANGLES, MAX_PROP_VERTICES,
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
    /// The material's glTF `doubleSided` declaration, defaulting to `false`
    /// (the glTF default). A double-sided primitive is visible from both sides
    /// and must therefore draw with back-face culling disabled; a single-sided
    /// one is wound so its front faces out of the solid.
    pub double_sided: bool,
    /// First index into [`PropModel::indices`].
    pub first_index: u32,
    /// Number of indices (a multiple of three).
    pub index_count: u32,
}

/// A decoded, ready-to-render prop model.
#[derive(Clone, Debug)]
pub struct PropModel {
    /// Model-space vertices with baked per-face shading and material colour in
    /// `color`.
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
}

// ------------------------------------------------------------------- parsing

/// Parses a self-contained GLB prop asset.
///
/// # Errors
///
/// Returns a [`GltfError`] naming the first problem found: a container that is
/// not a self-contained glTF 2.0 GLB, a document feature the prop renderer
/// cannot draw (extensions other than `KHR_materials_emissive_strength`,
/// skins, animations or morph targets), a malformed scene graph (a cycle,
/// dangling node or non-finite transform), a mesh outside the prop budgets,
/// or vertex data that is non-finite or outside the documented UV range.
pub fn parse_glb(bytes: &[u8]) -> Result<PropModel, GltfError> {
    let (json, binary) = parse_container(bytes)?;
    let materials = validate_document_root(&json)?;
    let mut doc = Doc::new(&json, &binary, materials)?;
    for root in scene_roots(&json)? {
        doc.traverse_node(root, Mat4::IDENTITY, &mut Vec::new())?;
    }
    let triangles = validate_mesh(&doc.vertices, &doc.indices)?;
    Ok(PropModel {
        vertices: doc.vertices,
        indices: doc.indices,
        textures: doc.textures,
        submeshes: doc.submeshes,
        triangles,
        materials,
    })
}

/// Rejects document-level features the prop renderer cannot draw.
///
/// Returns the number of declared materials on success.
fn validate_document_root(json: &serde_json::Value) -> Result<usize, GltfError> {
    validate_extensions(json)?;
    if json.get("skins").is_some() {
        return Err(GltfError::new("skinned meshes are not supported"));
    }
    if json.get("animations").is_some() {
        return Err(GltfError::new("animated prop assets are not supported"));
    }
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
    /// The material's `doubleSided` declaration; `false` is the glTF default.
    double_sided: bool,
}

impl Default for ResolvedMaterial {
    fn default() -> Self {
        Self {
            color: DEFAULT_BASE_COLOR,
            texture: None,
            emission: MaterialEmission::NONE,
            double_sided: false,
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
        })
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
            self.read_mesh(mesh_index, transform)?;
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
    fn read_mesh(&mut self, mesh_index: usize, transform: &Mat4) -> Result<(), GltfError> {
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
            self.read_primitive(primitive, transform, mesh_index)?;
        }
        Ok(())
    }

    /// Reads one primitive's vertices and triangle indices into the model.
    fn read_primitive(
        &mut self,
        primitive: &serde_json::Value,
        transform: &Mat4,
        mesh_index: usize,
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
        append_vertices(&mut self.vertices, &attributes, transform, resolved.color)?;
        let first_index = u32::try_from(self.indices.len())
            .map_err(|_| GltfError::new("prop model index buffer does not fit in 32 bits"))?;
        append_indices(&mut self.indices, &local_indices, base, vertex_count)?;
        self.submeshes.push(PropSubmesh {
            material: u16::try_from(material)
                .map_err(|_| GltfError::new("material index does not fit in 16 bits"))?,
            texture: resolved.texture,
            emission: resolved.emission,
            double_sided: resolved.double_sided,
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
        // A declared non-boolean is an authoring error, not a value to guess
        // at: the two sides of the model render differently, so a typo must
        // not silently become "single-sided".
        let double_sided = match material.get("doubleSided") {
            Some(value) => value.as_bool().ok_or_else(|| {
                GltfError::new(format!("material {index} doubleSided is not a boolean"))
            })?,
            None => false,
        };
        let resolved = ResolvedMaterial {
            color,
            texture,
            emission,
            double_sided,
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

/// The vertex attribute lists one primitive reads, before assembly.
struct PrimitiveAttributes {
    positions: Vec<Vec<f32>>,
    uvs: Vec<Vec<f32>>,
    colors: Vec<Vec<f32>>,
}

/// Reads one primitive's `POSITION`, `TEXCOORD_0` and `COLOR_0` attributes.
///
/// `COLOR_0` stays optional (default white); `TEXCOORD_0` is required because
/// every prop is UV mapped.
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
    if positions.len() != uvs.len() || positions.len() != colors.len() {
        return Err(GltfError::new(
            "POSITION, TEXCOORD_0 and COLOR_0 attribute counts differ",
        ));
    }
    Ok(PrimitiveAttributes {
        positions,
        uvs,
        colors,
    })
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
