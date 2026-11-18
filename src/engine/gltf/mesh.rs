//! Scene traversal and assembly of drawable model geometry.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use glam::{Mat4, Quat, Vec3};

use super::accessor::{
    COMPONENT_FLOAT, accessor_view, components, json_f32, json_u32, json_usize, numeric_array,
    read_indices, read_joints, read_vec, read_weights,
};
use super::animation::parse_animations;
use super::{
    GltfError, PropModel, PropMorphTarget, PropSkin, PropSubmesh, PropVertex, SourceVertex,
    model_slot,
};
use crate::engine::level::{
    MAX_PROP_MORPH_TARGETS, MAX_PROP_MORPH_TARGETS_PER_MODEL, MAX_PROP_PRIMITIVES,
    MAX_PROP_VERTICES,
};
use crate::engine::materials::RawImage;

mod material;
mod skinning;

use material::ResolvedMaterial;
use skinning::{Rig, append_skinned_vertices};

const MODE_TRIANGLES: u32 = 4;

/// Maximum node-hierarchy depth accepted; real prop models nest a handful of
/// transform groups at most, so anything deeper is treated as malformed.
const MAX_NODE_DEPTH: usize = 64;

/// Maximum node visits during one scene walk.
///
/// A DAG may legitimately reference one node from several parents, but the
/// total has to stay bounded so a pathological graph cannot expand into
/// unbounded work.
const MAX_NODE_VISITS: usize = 4_096;

pub(super) fn decode_model(
    json: &serde_json::Value,
    binary: &[u8],
    materials: usize,
    uv_max: f32,
    triangle_limit: usize,
) -> Result<PropModel, GltfError> {
    let mut doc = Doc::new(json, binary, materials, triangle_limit)?;
    for root in scene_roots(json)? {
        doc.traverse_node(root, Mat4::IDENTITY, &mut Vec::new())?;
    }
    doc.finish_morphs();
    let triangles = validate_mesh(&doc.vertices, &doc.indices, uv_max, triangle_limit)?;
    let animations = parse_animations(json, binary)?;
    // Joint and weight arrays are parallel to `vertices`. A rigged document
    // that binds nothing to a node has nothing for the character path to
    // claim, so its placeholder entries are dropped. A rigid animated model
    // binds every primitive to its owning node, so its arrays are kept.
    if doc.skin.is_none() && !doc.rigid_animation {
        doc.joints = Vec::new();
        doc.weights = Vec::new();
    }
    let nodes = doc.rig.as_ref().map_or_default(|rig| rig.nodes.clone());
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
        morph_targets: doc.morph_targets,
        morph_weights: doc.morph_weights,
        mesh_morph_ranges: doc.mesh_morph_ranges,
        animations,
        nodes,
    })
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
            let _new_entry = has_parent.insert(index);
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

/// The per-mesh context one primitive read needs.
struct PrimitiveRequest<'a> {
    transform: &'a Mat4,
    mesh_index: usize,
    node_index: usize,
    node_skin: Option<usize>,
    mesh_weights: &'a [f32],
    target_base: usize,
}

/// One GLB document mid-assembly, with the caches that keep resolution
/// single-shot: textures decode once per distinct image, materials resolve
/// once, and the traversal counters cap the scene graph.
struct Doc<'a> {
    json: &'a serde_json::Value,
    binary: &'a [u8],
    /// The hash-verified profile's bounded ceiling, or the ordinary default.
    triangle_limit: usize,
    /// Declared material count; a primitive without `material` reports it as
    /// its synthetic default-material slot.
    default_material_index: usize,
    material_cache: Vec<Option<ResolvedMaterial>>,
    vertices: Vec<PropVertex>,
    indices: Vec<u16>,
    submeshes: Vec<PropSubmesh>,
    textures: Vec<Arc<RawImage>>,
    /// glTF image index -> index in `textures`, in first-use order.
    texture_of_image: HashMap<usize, u16>,
    primitives_seen: usize,
    node_visits: usize,
    /// Retained node hierarchy, present only when the document declares a skin
    /// or animations; static models keep the historical flattening and drop
    /// it.
    rig: Option<Rig>,
    /// True for a document that declares animation clips but no skin: every
    /// unskinned primitive is bound to its owning node with weight one, so the
    /// character path poses the node hierarchy rigidly.
    rigid_animation: bool,
    /// The model's single resolved skin, set by the first skinned primitive.
    skin: Option<PropSkin>,
    /// Per-vertex joint slots and weights, parallel to `vertices`; empty for
    /// an unskinned model.
    joints: Vec<[u16; 4]>,
    weights: Vec<[f32; 4]>,
    /// Morph targets in first-visit order (assembled after the traversal).
    morph_targets: Vec<PropMorphTarget>,
    /// Default morph weights, parallel to `morph_targets`.
    morph_weights: Vec<f32>,
    /// Per mesh-visit morph range: node index, first target, target count.
    mesh_morph_ranges: Vec<(u16, u16, u16)>,
    /// Per-primitive transformed deltas awaiting assembly into
    /// `morph_targets`: vertex base, global target base, per-target default
    /// weights and the deltas.
    morph_chunks: Vec<(usize, usize, Vec<f32>, Vec<PropMorphTarget>)>,
    /// Assembled vertex blocks keyed by `(attributes, mesh, node, skin)`: two
    /// primitives of one mesh visit that share every attribute accessor (the
    /// Halloween material splits do exactly this) draw one vertex block with
    /// two index ranges and two materials, so the flattened model stays as
    /// small as the authored one. A primitive with morph targets never
    /// reuses a block: its deltas are assembled per primitive.
    vertex_blocks: Vec<(serde_json::Value, usize, usize, Option<usize>, usize, usize)>,
}

impl<'a> Doc<'a> {
    fn new(
        json: &'a serde_json::Value,
        binary: &'a [u8],
        materials: usize,
        triangle_limit: usize,
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
        let rigid_animation = json.get("skins").is_none() && json.get("animations").is_some();
        Ok(Self {
            json,
            binary,
            triangle_limit,
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
            rigid_animation,
            skin: None,
            joints: Vec::new(),
            weights: Vec::new(),
            morph_targets: Vec::new(),
            morph_weights: Vec::new(),
            mesh_morph_ranges: Vec::new(),
            morph_chunks: Vec::new(),
            vertex_blocks: Vec::new(),
        })
    }

    /// Assembles per-primitive morph deltas into model-parallel target arrays.
    ///
    /// Each chunk's deltas are parallel to the primitive's own vertices; a
    /// target array spans every model vertex, with zeros outside the primitive
    /// that declared it. Targets are grouped per mesh visit, so a mesh used by
    /// two nodes keeps independent ranges for each node's weights channel.
    fn finish_morphs(&mut self) {
        let target_count = self
            .mesh_morph_ranges
            .iter()
            .map(|(_, _, count)| usize::from(*count))
            .sum();
        if target_count == 0 {
            return;
        }
        let vertex_count = self.vertices.len();
        let mut targets: Vec<PropMorphTarget> = (0..target_count)
            .map(|_| PropMorphTarget {
                position: vec![[0.0; 3]; vertex_count],
                normal: Vec::new(),
                tangent: Vec::new(),
            })
            .collect();
        let mut weights = vec![0.0f32; target_count];
        for (vertex_base, target_base, defaults, chunk) in &self.morph_chunks {
            for (index, deltas) in chunk.iter().enumerate() {
                let target_index = target_base.saturating_add(index);
                let Some(default) = weights.get_mut(target_index) else {
                    continue;
                };
                *default = defaults.get(index).copied().unwrap_or(0.0);
                let Some(slot) = targets.get_mut(target_index) else {
                    continue;
                };
                if !deltas.normal.is_empty() && slot.normal.is_empty() {
                    slot.normal = vec![[0.0; 3]; vertex_count];
                }
                if !deltas.tangent.is_empty() && slot.tangent.is_empty() {
                    slot.tangent = vec![[0.0; 4]; vertex_count];
                }
                for (vertex, delta) in deltas.position.iter().enumerate() {
                    if let Some(entry) = slot.position.get_mut(vertex_base.saturating_add(vertex)) {
                        *entry = *delta;
                    }
                }
                for (vertex, delta) in deltas.normal.iter().enumerate() {
                    if let Some(entry) = slot.normal.get_mut(vertex_base.saturating_add(vertex)) {
                        *entry = *delta;
                    }
                }
                for (vertex, delta) in deltas.tangent.iter().enumerate() {
                    if let Some(entry) = slot.tangent.get_mut(vertex_base.saturating_add(vertex)) {
                        *entry = *delta;
                    }
                }
            }
        }
        self.morph_targets = targets;
        self.morph_weights = weights;
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
        #[expect(
            clippy::arithmetic_side_effects,
            reason = "`glam` matrix multiplication is per-element `f32` arithmetic with no integer overflow or panic path; clippy cannot see that through the operator."
        )]
        let world = parent * node_transform(node, index)?;
        path.push(index);
        let result = self.visit_node_contents(node, index, &world, path);
        let _removed_value = path.pop();
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
            let child_nodes = children
                .as_array()
                .ok_or_else(|| GltfError::new(format!("node {index} children is not an array")))?;
            for child in child_nodes {
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
        let node = self
            .json
            .get("nodes")
            .and_then(|value| value.as_array())
            .and_then(|nodes| nodes.get(node_index))
            .ok_or_else(|| GltfError::new(format!("node {node_index} does not exist")))?;
        if let Some(node_weights) = node.get("weights") {
            if !node_weights
                .as_array()
                .is_some_and(|values| values.iter().all(serde_json::Value::is_f64))
            {
                return Err(GltfError::new(format!(
                    "node {node_index} has malformed `weights`"
                )));
            }
            return Err(GltfError::new(format!(
                "node {node_index} overrides morph weights per node; \
                 per-node weight overrides are not supported"
            )));
        }
        let mesh_weights: Vec<f32> = match mesh.get("weights") {
            Some(values) => values
                .as_array()
                .ok_or_else(|| GltfError::new("mesh `weights` must be an array"))?
                .iter()
                .map(|value| {
                    json_f32(value).ok_or_else(|| GltfError::new("mesh `weights` must be numbers"))
                })
                .collect::<Result<Vec<f32>, _>>()?,
            None => Vec::new(),
        };
        let target_base = self
            .mesh_morph_ranges
            .iter()
            .map(|(_, start, count)| usize::from(*start).saturating_add(usize::from(*count)))
            .max()
            .unwrap_or(0);
        let mut target_count: Option<usize> = None;
        for primitive in primitives {
            self.primitives_seen = self.primitives_seen.saturating_add(1);
            if self.primitives_seen > MAX_PROP_PRIMITIVES {
                return Err(GltfError::new(format!(
                    "prop model declares more than {MAX_PROP_PRIMITIVES} primitives; \
                     the engine ceiling is {MAX_PROP_PRIMITIVES}"
                )));
            }
            let request = PrimitiveRequest {
                transform,
                mesh_index,
                node_index,
                node_skin,
                mesh_weights: &mesh_weights,
                target_base,
            };
            let targets = self.read_primitive(primitive, &request)?;
            match target_count {
                None => target_count = Some(targets),
                Some(previous) if previous != targets => {
                    return Err(GltfError::new(format!(
                        "mesh {mesh_index} primitives declare different morph target \
                         counts ({previous} and {targets}); all primitives of a mesh \
                         must agree"
                    )));
                }
                Some(_) => {}
            }
        }
        let count = target_count.unwrap_or(0);
        let total_targets = target_base.saturating_add(count);
        if total_targets > MAX_PROP_MORPH_TARGETS_PER_MODEL {
            return Err(GltfError::new(format!(
                "prop model declares {total_targets} morph targets; \
                 the engine ceiling is {MAX_PROP_MORPH_TARGETS_PER_MODEL}"
            )));
        }
        self.mesh_morph_ranges.push((
            model_slot(node_index, "node index")?,
            model_slot(target_base, "morph target index")?,
            model_slot(count, "morph target count")?,
        ));
        Ok(())
    }

    /// Reads one primitive's vertices and triangle indices into the model.
    ///
    /// Returns the primitive's morph-target count.
    fn read_primitive(
        &mut self,
        primitive: &serde_json::Value,
        request: &PrimitiveRequest<'_>,
    ) -> Result<usize, GltfError> {
        validate_primitive_mode(primitive)?;
        validate_primitive_counts(
            self.json,
            primitive,
            self.triangle_limit
                .saturating_mul(3)
                .saturating_sub(self.indices.len()),
        )?;
        let attributes = read_attributes(
            self.json,
            self.binary,
            primitive,
            request.mesh_index,
            request.mesh_weights,
        )?;
        let target_count = attributes.targets.len();
        let vertex_count = attributes.positions.len();
        let material = match primitive.get("material") {
            Some(value) => json_usize(value).ok_or_else(|| {
                GltfError::new(format!(
                    "mesh {} has a primitive with a non-numeric material index",
                    request.mesh_index
                ))
            })?,
            None => self.default_material_index,
        };
        let resolved = self.resolve_material(material)?;
        let mut local_indices = primitive_indices(self.json, self.binary, primitive, vertex_count)?;
        if local_indices.is_empty() {
            return Ok(target_count);
        }
        if local_indices.len() % 3 != 0 {
            return Err(GltfError::new(
                "index count is not a multiple of three; props must be triangle lists",
            ));
        }
        // A reflected static node reverses the triangle cross product while
        // inverse-transpose normals retain the outward side. Keep winding,
        // culling and the visibility origin on that same outward side.
        if request.node_skin.is_none() && request.transform.determinant() < 0.0 {
            for triangle in local_indices.as_chunks_mut::<3>().0 {
                triangle.swap(1, 2);
            }
        }
        let index_count = u32::try_from(local_indices.len()).map_err(|error| {
            GltfError::new(format!(
                "primitive index count does not fit in 32 bits: {error}"
            ))
        })?;
        let reused = if target_count == 0 {
            self.reusable_vertex_block(primitive, request)
        } else {
            None
        };
        let (base, block_vertices) = if let Some(found) = reused {
            found
        } else {
            let vertex_total = self.vertices.len().saturating_add(vertex_count);
            if vertex_total > MAX_PROP_VERTICES {
                return Err(GltfError::new(format!(
                    "prop model assembles {vertex_total} vertices; \
                     the engine ceiling is {MAX_PROP_VERTICES}"
                )));
            }
            let base = self.vertices.len();
            self.assemble_primitive(primitive, request, &attributes, resolved, target_count)?;
            if target_count == 0 {
                self.vertex_blocks.push((
                    primitive.get("attributes").cloned().unwrap_or_default(),
                    request.mesh_index,
                    request.node_index,
                    request.node_skin,
                    base,
                    vertex_count,
                ));
            }
            (base, vertex_count)
        };
        let first_index = u32::try_from(self.indices.len()).map_err(|error| {
            GltfError::new(format!(
                "prop model index buffer does not fit in 32 bits: {error}"
            ))
        })?;
        append_indices(&mut self.indices, &local_indices, base, block_vertices)?;
        self.submeshes.push(PropSubmesh {
            double_sided: resolved.double_sided,
            response: resolved.response,
            material: u16::try_from(material).map_err(|error| {
                GltfError::new(format!("material index does not fit in 16 bits: {error}"))
            })?,
            texture: resolved.texture,
            emission: resolved.emission,
            alpha: resolved.alpha,
            first_index,
            index_count,
        });
        self.check_triangle_budget()?;
        Ok(target_count)
    }

    fn check_triangle_budget(&self) -> Result<(), GltfError> {
        let triangles = self.indices.len() / 3;
        if triangles > self.triangle_limit {
            return Err(GltfError::new(format!(
                "prop model assembles {triangles} triangles; \
                 the engine ceiling is {}",
                self.triangle_limit
            )));
        }
        Ok(())
    }

    /// The already-assembled vertex block a primitive may reuse.
    ///
    /// Two primitives of one mesh **visit** that declare identical attribute
    /// accessors (and the same node skin) describe the same vertex data: the
    /// Halloween pumpkin-head and ghost-face splits share one block across two
    /// index ranges and two materials. Reuse is scoped to the mesh visit — a
    /// mesh placed under a second node is re-assembled because its vertices
    /// bake that node's transform — and is skipped for morph-target
    /// primitives, whose deltas are assembled per primitive.
    fn reusable_vertex_block(
        &self,
        primitive: &serde_json::Value,
        request: &PrimitiveRequest<'_>,
    ) -> Option<(usize, usize)> {
        let attributes = primitive.get("attributes")?;
        self.vertex_blocks
            .iter()
            .find(|(key, mesh, node, skin, _, _)| {
                key == attributes
                    && *mesh == request.mesh_index
                    && *node == request.node_index
                    && *skin == request.node_skin
            })
            .map(|(_, _, _, _, base, count)| (*base, *count))
    }

    /// Appends one primitive's vertices and records its morph-target chunk.
    fn assemble_primitive(
        &mut self,
        primitive: &serde_json::Value,
        request: &PrimitiveRequest<'_>,
        attributes: &PrimitiveAttributes,
        resolved: ResolvedMaterial,
        target_count: usize,
    ) -> Result<(), GltfError> {
        let base = self.vertices.len();
        // Skin pairing: a glTF node declares one skin and then every primitive
        // of the mesh it references must supply JOINTS_0 and WEIGHTS_0; a
        // primitive with joint attributes but no skin is malformed.
        let skinned = if request.node_skin.is_some() {
            if attributes.joints.is_none() || attributes.weights.is_none() {
                return Err(GltfError::new(format!(
                    "mesh {} has a skinned primitive with no \
                     JOINTS_0/WEIGHTS_0 attributes",
                    request.mesh_index
                )));
            }
            true
        } else {
            if attributes.joints.is_some() || attributes.weights.is_some() {
                return Err(GltfError::new(format!(
                    "mesh {} has a primitive with JOINTS_0/WEIGHTS_0 \
                     but no skin",
                    request.mesh_index
                )));
            }
            false
        };
        let chunk = if skinned {
            let skin_index = request.node_skin.ok_or_else(|| {
                GltfError::new(format!("mesh {} references no skin", request.mesh_index))
            })?;
            let skin_matrices = self.resolve_primitive_skin(skin_index, request.node_index)?;
            if let Some(skin) = &mut self.skin {
                for (index, position) in attributes.positions.iter().enumerate() {
                    skin.source_vertices.push(SourceVertex {
                        position: components(position)?,
                        normal: attributes
                            .normals
                            .as_ref()
                            .and_then(|normals| normals.get(index))
                            .copied(),
                    });
                }
            }
            append_skinned_vertices(
                &mut self.vertices,
                &mut self.joints,
                &mut self.weights,
                attributes,
                &skin_matrices,
                resolved.color,
            )?
        } else {
            let chunk = append_vertices(
                &mut self.vertices,
                attributes,
                request.transform,
                resolved.color,
            )?;
            if self.rigid_animation {
                // A rigid animated model binds each primitive to the node that
                // carries it: joint slot = node index, weight one. The
                // character path's delta `nodeGlobal * nodeRestInverse` then
                // moves exactly that node's vertices.
                let node = u16::try_from(request.node_index).map_err(|error| {
                    GltfError::new(format!("node index does not fit in 16 bits: {error}"))
                })?;
                self.joints.resize(self.vertices.len(), [0; 4]);
                self.weights.resize(self.vertices.len(), [0.0; 4]);
                for index in base..self.vertices.len() {
                    if let Some(slot) = self.joints.get_mut(index) {
                        *slot = [node, 0, 0, 0];
                    }
                    if let Some(slot) = self.weights.get_mut(index) {
                        *slot = [1.0, 0.0, 0.0, 0.0];
                    }
                }
            } else if self.is_rigged() {
                // Keep the joint/weight arrays parallel to `vertices`; a
                // rigid primitive inside a skinned document stays at its bind
                // pose (zero weights).
                self.joints.resize(self.vertices.len(), [0; 4]);
                self.weights.resize(self.vertices.len(), [0.0; 4]);
            }
            chunk
        };
        if target_count > 0 {
            self.morph_chunks.push((
                base,
                request.target_base,
                attributes.morph_defaults.clone(),
                chunk,
            ));
        }
        let _ = primitive;
        Ok(())
    }
}

/// The vertex attribute lists one primitive reads, before assembly.
struct PrimitiveAttributes {
    positions: Vec<Vec<f32>>,
    normals: Option<Vec<[f32; 3]>>,
    uvs: Vec<Vec<f32>>,
    colors: Vec<Vec<f32>>,
    /// `JOINTS_0` joint slots, when the primitive declares them.
    joints: Option<Vec<[u16; 4]>>,
    /// `WEIGHTS_0` joint weights, when the primitive declares them.
    weights: Option<Vec<[f32; 4]>>,
    /// Morph-target deltas in mesh space, in `primitive.targets` order.
    targets: Vec<PropMorphTarget>,
    /// Default morph weights: `primitive.weights`, else the mesh's, else zero.
    morph_defaults: Vec<f32>,
}

/// Reads one primitive's `POSITION`, `TEXCOORD_0`, `COLOR_0` and optional
/// `NORMAL`/`JOINTS_0`/`WEIGHTS_0` attributes.
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
    mesh_weights: &[f32],
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
    let normals = read_normals(json, binary, attributes, positions.len())?;
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
        .is_some_and(|joint_values| joint_values.len() != positions.len())
        || weights
            .as_ref()
            .is_some_and(|weight_values| weight_values.len() != positions.len())
    {
        return Err(GltfError::new(
            "POSITION, JOINTS_0 and WEIGHTS_0 attribute counts differ",
        ));
    }
    let targets = read_morph_targets(json, binary, primitive, positions.len())?;
    let mut defaults: Vec<f32> = match primitive.get("weights") {
        Some(values) => values
            .as_array()
            .ok_or_else(|| GltfError::new("primitive `weights` must be an array"))?
            .iter()
            .map(|value| {
                json_f32(value)
                    .ok_or_else(|| GltfError::new("primitive `weights` must contain numbers"))
            })
            .collect::<Result<Vec<f32>, _>>()?,
        None => mesh_weights.to_vec(),
    };
    if defaults.is_empty() {
        defaults = vec![0.0; targets.len()];
    }
    if defaults.len() != targets.len() {
        return Err(GltfError::new(format!(
            "primitive declares {} morph weights but {} targets",
            defaults.len(),
            targets.len()
        )));
    }
    for value in &defaults {
        if !value.is_finite() {
            return Err(GltfError::new(
                "morph-target default weights must be finite",
            ));
        }
    }
    Ok(PrimitiveAttributes {
        positions,
        normals,
        uvs,
        colors,
        joints,
        weights,
        targets,
        morph_defaults: defaults,
    })
}

/// Validates optional authored normals independently of the other vertex attributes.
fn read_normals(
    json: &serde_json::Value,
    binary: &[u8],
    attributes: &serde_json::Map<String, serde_json::Value>,
    vertex_count: usize,
) -> Result<Option<Vec<[f32; 3]>>, GltfError> {
    match attributes.get("NORMAL") {
        Some(value) => {
            let index = accessor_index(value, "NORMAL")?;
            if accessor_view(json, binary, index, 3)?.component_type != COMPONENT_FLOAT {
                return Err(GltfError::new("NORMAL must use float32 components"));
            }
            let rows = read_vec(json, binary, index, 3)?;
            if rows.len() != vertex_count {
                return Err(GltfError::new(
                    "POSITION and NORMAL attribute counts differ",
                ));
            }
            Ok(Some(
                rows.iter()
                    .map(|row| {
                        components::<3>(row)
                            .and_then(|normal| normalized_normal(Vec3::from(normal)))
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            ))
        }
        None => Ok(None),
    }
}

/// Reads one primitive's `targets`: POSITION is required per target, NORMAL
/// and TANGENT are optional. Every delta list matches the primitive's vertex
/// count and must be finite.
fn read_morph_targets(
    json: &serde_json::Value,
    binary: &[u8],
    primitive: &serde_json::Value,
    vertex_count: usize,
) -> Result<Vec<PropMorphTarget>, GltfError> {
    let Some(list) = primitive.get("targets").and_then(|value| value.as_array()) else {
        return Ok(Vec::new());
    };
    if list.len() > MAX_PROP_MORPH_TARGETS {
        return Err(GltfError::new(format!(
            "primitive declares {} morph targets; the engine ceiling is {MAX_PROP_MORPH_TARGETS}",
            list.len()
        )));
    }
    let mut targets = Vec::with_capacity(list.len());
    for target in list {
        let Some(object) = target.as_object() else {
            return Err(GltfError::new("a morph target must be an object"));
        };
        let Some(position_value) = object.get("POSITION") else {
            return Err(GltfError::new(
                "morph target has no POSITION deltas; POSITION is required",
            ));
        };
        let position = read_vec(
            json,
            binary,
            accessor_index(position_value, "morph POSITION")?,
            3,
        )?;
        if position.len() != vertex_count {
            return Err(GltfError::new(format!(
                "morph POSITION has {} deltas for {vertex_count} vertices",
                position.len()
            )));
        }
        let normal = match object.get("NORMAL") {
            Some(value) => {
                let rows = read_vec(json, binary, accessor_index(value, "morph NORMAL")?, 3)?;
                if rows.len() != vertex_count {
                    return Err(GltfError::new("morph NORMAL delta count mismatch"));
                }
                rows
            }
            None => Vec::new(),
        };
        let tangent = match object.get("TANGENT") {
            Some(value) => {
                let rows = read_vec(json, binary, accessor_index(value, "morph TANGENT")?, 3)?;
                if rows.len() != vertex_count {
                    return Err(GltfError::new("morph TANGENT delta count mismatch"));
                }
                rows
            }
            None => Vec::new(),
        };
        let convert3 = |rows: Vec<Vec<f32>>| -> Result<Vec<[f32; 3]>, GltfError> {
            rows.iter().map(|row| components::<3>(row)).collect()
        };
        let convert_tangent = |rows: Vec<Vec<f32>>| -> Result<Vec<[f32; 4]>, GltfError> {
            rows.iter()
                .map(|row| components::<3>(row).map(|[x, y, z]| [x, y, z, 0.0]))
                .collect()
        };
        let position_deltas = convert3(position)?;
        for delta in &position_deltas {
            if !delta.iter().all(|value| value.is_finite()) {
                return Err(GltfError::new("morph POSITION contains a non-finite delta"));
            }
        }
        let normal_deltas = convert3(normal)?;
        if normal_deltas
            .iter()
            .flatten()
            .any(|value| !value.is_finite())
        {
            return Err(GltfError::new("morph NORMAL contains a non-finite delta"));
        }
        targets.push(PropMorphTarget {
            position: position_deltas,
            normal: normal_deltas,
            tangent: convert_tangent(tangent)?,
        });
    }
    Ok(targets)
}

/// Applies existing mesh ceilings to accessor metadata before any component
/// vectors are allocated. Invalid or absent metadata is diagnosed by the
/// ordinary accessor reader; only a declared over-budget count is handled here.
pub(super) fn validate_primitive_counts(
    json: &serde_json::Value,
    primitive: &serde_json::Value,
    remaining_indices: usize,
) -> Result<(), GltfError> {
    let check = |value: &serde_json::Value, limit: usize, description: &str| {
        let declared_count = json_usize(value)
            .and_then(|index| json.get("accessors")?.as_array()?.get(index))
            .and_then(|accessor| accessor.get("count"))
            .and_then(json_usize);
        if let Some(count) = declared_count
            && count > limit
        {
            return Err(GltfError::new(format!(
                "{description} declares {count} elements; the remaining engine ceiling is {limit}"
            )));
        }
        Ok(())
    };
    if let Some(attributes) = primitive
        .get("attributes")
        .and_then(serde_json::Value::as_object)
    {
        for name in [
            "POSITION",
            "TEXCOORD_0",
            "COLOR_0",
            "NORMAL",
            "JOINTS_0",
            "WEIGHTS_0",
        ] {
            if let Some(value) = attributes.get(name) {
                check(value, MAX_PROP_VERTICES, name)?;
            }
        }
    }
    if let Some(targets) = primitive
        .get("targets")
        .and_then(serde_json::Value::as_array)
    {
        for target in targets {
            for name in ["POSITION", "NORMAL", "TANGENT"] {
                if let Some(value) = target.get(name) {
                    check(value, MAX_PROP_VERTICES, "morph attribute")?;
                }
            }
        }
    }
    if let Some(value) = primitive.get("indices") {
        check(value, remaining_indices, "triangle indices")?;
    }
    Ok(())
}

fn validate_primitive_mode(primitive: &serde_json::Value) -> Result<(), GltfError> {
    let mode = primitive
        .get("mode")
        .and_then(json_u32)
        .unwrap_or(MODE_TRIANGLES);
    if mode != MODE_TRIANGLES {
        return Err(GltfError::new(format!(
            "primitive mode {mode} is not TRIANGLES (4)"
        )));
    }
    Ok(())
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
                u32::try_from(value).map_err(|error| {
                    GltfError::new(format!("mesh index does not fit in 32 bits: {error}"))
                })
            })
            .collect::<Result<Vec<u32>, _>>(),
    }
}

/// Validates and normalizes an authored or transformed surface normal.
fn normalized_normal(normal: Vec3) -> Result<[f32; 3], GltfError> {
    normal
        .is_finite()
        .then(|| normal.try_normalize())
        .flatten()
        .map(|unit_normal| unit_normal.to_array())
        .ok_or_else(|| GltfError::new("NORMAL must be finite and nonzero after transformation"))
}

/// Normal deltas and surface directions use the inverse transpose, never position scaling.
fn primitive_normal_transform(
    attributes: &PrimitiveAttributes,
    transform: &Mat4,
) -> Result<Option<Mat4>, GltfError> {
    if attributes.normals.is_none()
        && attributes
            .targets
            .iter()
            .all(|target| target.normal.is_empty())
    {
        return Ok(None);
    }
    let matrix = transform.inverse().transpose();
    if !matrix.is_finite() {
        return Err(GltfError::new(
            "NORMAL cannot be transformed through a singular node or skin matrix",
        ));
    }
    Ok(Some(matrix))
}

/// Applies default morph weights before transforming the authored normal into model space.
fn default_normal(
    attributes: &PrimitiveAttributes,
    index: usize,
) -> Result<Option<Vec3>, GltfError> {
    let Some(normal) = attributes
        .normals
        .as_ref()
        .and_then(|normals| normals.get(index))
    else {
        return Ok(None);
    };
    let mut morphed_normal = Vec3::from(*normal);
    for (weight, target) in attributes.morph_defaults.iter().zip(&attributes.targets) {
        if *weight != 0.0
            && let Some(delta) = target.normal.get(index)
        {
            morphed_normal = Vec3::from(*delta).mul_add(Vec3::splat(*weight), morphed_normal);
        }
    }
    // Keep the unnormalized morphed direction until after inverse-transpose or skin blending.
    let _normalized_normal_status = normalized_normal(morphed_normal)?;
    Ok(Some(morphed_normal))
}

/// Appends one primitive's transformed vertices, with the material colour
/// multiplied into every baked vertex colour and the default morph weights
/// already applied. Returns the transformed morph-target deltas in model space.
fn append_vertices(
    vertices: &mut Vec<PropVertex>,
    attributes: &PrimitiveAttributes,
    transform: &Mat4,
    color: [f32; 4],
) -> Result<Vec<PropMorphTarget>, GltfError> {
    let normal_transform = primitive_normal_transform(attributes, transform)?;
    for (index, ((position, uv), vertex_color)) in attributes
        .positions
        .iter()
        .zip(attributes.uvs.iter())
        .zip(attributes.colors.iter())
        .enumerate()
    {
        let [x, y, z]: [f32; 3] = components(position)?;
        let mut point = Vec3::new(x, y, z);
        for (weight, target) in attributes.morph_defaults.iter().zip(&attributes.targets) {
            if *weight != 0.0
                && let Some(delta) = target.position.get(index)
            {
                point = Vec3::from(*delta).mul_add(Vec3::splat(*weight), point);
            }
        }
        let world_point = transform.transform_point3(point);
        let [red, green, blue, alpha]: [f32; 4] = components(vertex_color)?;
        vertices.push(PropVertex {
            pos: [world_point.x, world_point.y, world_point.z],
            normal: default_normal(attributes, index)?
                .map(|normal| {
                    normal_transform.map_or_else(
                        || Ok(normal.to_array()),
                        |matrix| normalized_normal(matrix.transform_vector3(normal)),
                    )
                })
                .transpose()?,
            color: [
                red * color[0],
                green * color[1],
                blue * color[2],
                alpha * color[3],
            ],
            uv: components(uv)?,
        });
    }
    Ok(transform_morphs_static(
        attributes,
        transform,
        normal_transform,
    ))
}

/// Transforms an unskinned primitive's morph deltas into model space.
fn transform_morphs_static(
    attributes: &PrimitiveAttributes,
    transform: &Mat4,
    normal_transform: Option<Mat4>,
) -> Vec<PropMorphTarget> {
    attributes
        .targets
        .iter()
        .map(|target| PropMorphTarget {
            position: target
                .position
                .iter()
                .map(|delta| {
                    let point = transform.transform_vector3(Vec3::from(*delta));
                    [point.x, point.y, point.z]
                })
                .collect(),
            normal: target
                .normal
                .iter()
                .map(|delta| {
                    let vector = normal_transform
                        .unwrap_or(Mat4::IDENTITY)
                        .transform_vector3(Vec3::from(*delta));
                    [vector.x, vector.y, vector.z]
                })
                .collect(),
            tangent: target
                .tangent
                .iter()
                .map(|delta| {
                    let vector =
                        transform.transform_vector3(Vec3::new(delta[0], delta[1], delta[2]));
                    [vector[0], vector[1], vector[2], delta[3]]
                })
                .collect(),
        })
        .collect()
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
            .and_then(|local_index| base.checked_add(local_index))
            .ok_or_else(|| GltfError::new(format!("index {value} points outside the mesh")))?;
        if absolute >= limit {
            return Err(GltfError::new(format!(
                "index {value} points outside the primitive's vertices"
            )));
        }
        let index = u16::try_from(absolute).map_err(|error| {
            GltfError::new(format!(
                "prop model needs more than {MAX_PROP_VERTICES} vertices; \
                 lower the prop's detail: {error}"
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
    let transform = if let Some(matrix_json) = matrix {
        let values = numeric_array::<16>(matrix_json, &format!("node {index} matrix"))?;
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
        let quaternion = Quat::from_xyzw(rotation[0], rotation[1], rotation[2], rotation[3]);
        // glTF rotations must be unit quaternions; a malformed export can ship
        // a near-zero quaternion, which would collapse the node's geometry.
        if quaternion.length_squared() <= f32::EPSILON {
            return Err(GltfError::new(format!(
                "node {index} rotation is not a unit quaternion"
            )));
        }
        let unit_rotation = quaternion.normalize();
        Mat4::from_scale_rotation_translation(
            Vec3::from(scale),
            unit_rotation,
            Vec3::from(translation),
        )
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

/// Checks the assembled mesh against the prop budgets and data invariants.
///
/// Returns the triangle count on success, so the caller can store it without
/// recomputing it from the index list.
fn validate_mesh(
    vertices: &[PropVertex],
    indices: &[u16],
    uv_max: f32,
    triangle_limit: usize,
) -> Result<usize, GltfError> {
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
    if triangles > triangle_limit {
        return Err(GltfError::new(format!(
            "prop model has {triangles} triangles; the engine ceiling is {triangle_limit}"
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
            || vertex.uv[0] > uv_max
            || vertex.uv[1] < -0.01
            || vertex.uv[1] > uv_max
        {
            return Err(GltfError::new(format!(
                "UV {:.3},{:.3} lies outside 0..1; props use non-tiling UVs",
                vertex.uv[0], vertex.uv[1]
            )));
        }
    }
    Ok(triangles)
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
