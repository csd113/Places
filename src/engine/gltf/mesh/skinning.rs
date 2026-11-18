//! Retained node hierarchy and bind-pose skinning for model import.

use glam::{Mat4, Vec3};

use super::{
    Doc, PrimitiveAttributes, default_normal, node_transform, normalized_normal,
    primitive_normal_transform,
};
use crate::engine::gltf::accessor::{components, json_usize, read_vec};
use crate::engine::gltf::{GltfError, PropMorphTarget, PropNode, PropSkin, PropVertex, list_len};
use crate::engine::level::MAX_PROP_JOINTS;

/// One document's retained node hierarchy plus the lazily resolved skins.
///
/// Built only for documents that declare a skin or animations: a pure static
/// model keeps the historical flatten-and-bake path and never materialises a
/// node list.
pub(super) struct Rig {
    /// Every node, indexed by node index.
    pub(super) nodes: Vec<PropNode>,
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
    pub(super) fn build(json: &serde_json::Value) -> Result<Self, GltfError> {
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
                    children.push(u16::try_from(child_index).map_err(|error| {
                        GltfError::new(format!("node index does not fit in 16 bits: {error}"))
                    })?);
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
        let Some(child_indices) = children else {
            continue;
        };
        let parent_index = u16::try_from(parent).map_err(|error| {
            GltfError::new(format!("node index does not fit in 16 bits: {error}"))
        })?;
        for child in child_indices {
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
        joints.push(u16::try_from(node).map_err(|error| {
            GltfError::new(format!("joint node index does not fit in 16 bits: {error}"))
        })?);
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
            Some(u16::try_from(node).map_err(|error| {
                GltfError::new(format!("node index does not fit in 16 bits: {error}"))
            })?)
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
            source_vertices: Vec::new(),
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
    let Some(accessor) = skin_json.get("inverseBindMatrices").and_then(json_usize) else {
        // The spec makes `inverseBindMatrices` optional: without them every
        // joint's inverse bind is the identity.
        return Ok(vec![Mat4::IDENTITY; joint_count]);
    };
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

impl Doc<'_> {
    /// Resolves a primitive's skin and returns the bind-pose skinning
    /// matrices per joint slot (`meshInverseGlobal * jointGlobal * inverseBind`).
    ///
    /// The model retains one skin: a second primitive that references a
    /// different skin or a different mesh node is refused, because the model
    /// carries a single [`PropSkin`] and a single skinned mesh.
    pub(super) fn resolve_primitive_skin(
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
        rig.mesh_node = Some(u16::try_from(node_index).map_err(|error| {
            GltfError::new(format!("node index does not fit in 16 bits: {error}"))
        })?);
        let resolved = rig.resolve_skin(skin_index, json, binary)?;
        let matrices = resolved.skin_matrices();
        if self.skin.is_none() {
            self.skin = Some(resolved.skin.clone());
        }
        Ok(matrices)
    }
}

/// Appends one skinned primitive's vertices, baked to the bind pose.
///
/// Each vertex's position is the weight-blended
/// `meshInverseGlobal * jointGlobal(rest) * inverseBind * p`, which is the
/// rest pose the static prop path draws and the pose the character path starts
/// from. Joint slots are validated against the primitive's skin and weights
/// are validated (finite, non-negative, positive sum) and renormalised, so the
/// stored weights always sum to one.
#[expect(
    clippy::arithmetic_side_effects,
    reason = "f32 blend of finite, validated values"
)] // f32 blend of finite, validated values
pub(super) fn append_skinned_vertices(
    vertices: &mut Vec<PropVertex>,
    joints: &mut Vec<[u16; 4]>,
    weights: &mut Vec<[f32; 4]>,
    attributes: &PrimitiveAttributes,
    skin_matrices: &[Mat4],
    color: [f32; 4],
) -> Result<Vec<PropMorphTarget>, GltfError> {
    let (Some(raw_joints), Some(raw_weights)) =
        (attributes.joints.as_ref(), attributes.weights.as_ref())
    else {
        return Err(GltfError::new(
            "a skinned primitive is missing its JOINTS_0/WEIGHTS_0 attributes",
        ));
    };
    let normal_matrices = skin_matrices
        .iter()
        .map(|matrix| primitive_normal_transform(attributes, matrix))
        .collect::<Result<Vec<_>, _>>()?;
    let joint_total = u16::try_from(skin_matrices.len()).unwrap_or(u16::MAX);
    let mut normalized_weights: Vec<[f32; 4]> = Vec::with_capacity(raw_weights.len());
    for (index, (((position, uv), vertex_color), (joint_slots, weight_values))) in attributes
        .positions
        .iter()
        .zip(attributes.uvs.iter())
        .zip(attributes.colors.iter())
        .zip(raw_joints.iter().zip(raw_weights.iter()))
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
        normalized_weights.push(normalized);
        let [red, green, blue, alpha]: [f32; 4] = components(vertex_color)?;
        vertices.push(PropVertex {
            pos: [blended.x, blended.y, blended.z],
            normal: skinned_normal(
                attributes,
                index,
                raw_joints,
                &normalized_weights,
                &normal_matrices,
            )?,
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
    // Deltas skin exactly like positions (the skinning transform is linear),
    // so a runtime weight change can be added to a baked vertex directly.
    let transformed = attributes
        .targets
        .iter()
        .map(|target| {
            skin_morph_target(
                target,
                raw_joints,
                &normalized_weights,
                skin_matrices,
                &normal_matrices,
            )
        })
        .collect();
    Ok(transformed)
}

/// Skins one morph target's deltas through the rest skinning matrices.
fn skin_morph_target(
    target: &PropMorphTarget,
    raw_joints: &[[u16; 4]],
    normalized_weights: &[[f32; 4]],
    skin_matrices: &[Mat4],
    normal_matrices: &[Option<Mat4>],
) -> PropMorphTarget {
    let position = target
        .position
        .iter()
        .enumerate()
        .map(|(index, delta)| {
            skin_vector(
                Vec3::from(*delta),
                index,
                raw_joints,
                normalized_weights,
                skin_matrices,
            )
        })
        .collect();
    let normal = target
        .normal
        .iter()
        .enumerate()
        .map(|(index, delta)| {
            skin_normal_vector(
                Vec3::from(*delta),
                index,
                raw_joints,
                normalized_weights,
                normal_matrices,
            )
        })
        .collect();
    let tangent = target
        .tangent
        .iter()
        .enumerate()
        .map(|(index, delta)| {
            let vector = skin_vector(
                Vec3::new(delta[0], delta[1], delta[2]),
                index,
                raw_joints,
                normalized_weights,
                skin_matrices,
            );
            [vector[0], vector[1], vector[2], delta[3]]
        })
        .collect();
    PropMorphTarget {
        position,
        normal,
        tangent,
    }
}

/// Skins one delta vector with the vertex's joint weights.
fn skin_vector(
    delta: Vec3,
    index: usize,
    raw_joints: &[[u16; 4]],
    normalized_weights: &[[f32; 4]],
    skin_matrices: &[Mat4],
) -> [f32; 3] {
    let mut blended = Vec3::ZERO;
    if let Some(normalized) = normalized_weights.get(index) {
        for (slot, weight) in raw_joints
            .get(index)
            .into_iter()
            .flat_map(|slots| slots.iter())
            .zip(normalized.iter())
        {
            if *weight <= 0.0 {
                continue;
            }
            if let Some(matrix) = skin_matrices.get(usize::from(*slot)) {
                blended = matrix
                    .transform_vector3(delta)
                    .mul_add(Vec3::splat(*weight), blended);
            }
        }
    }
    [blended.x, blended.y, blended.z]
}

/// Resolves one default-morphed bind-pose normal and normalizes after weighted skinning.
fn skinned_normal(
    attributes: &PrimitiveAttributes,
    index: usize,
    raw_joints: &[[u16; 4]],
    normalized_weights: &[[f32; 4]],
    normal_matrices: &[Option<Mat4>],
) -> Result<Option<[f32; 3]>, GltfError> {
    default_normal(attributes, index)?
        .map(|normal| {
            normalized_normal(Vec3::from(skin_normal_vector(
                normal,
                index,
                raw_joints,
                normalized_weights,
                normal_matrices,
            )))
        })
        .transpose()
}

/// Skins a normal delta through weighted inverse-transpose matrices, without normalizing a delta.
fn skin_normal_vector(
    delta: Vec3,
    index: usize,
    raw_joints: &[[u16; 4]],
    normalized_weights: &[[f32; 4]],
    normal_matrices: &[Option<Mat4>],
) -> [f32; 3] {
    let mut blended = Vec3::ZERO;
    if let Some(normalized) = normalized_weights.get(index) {
        for (slot, weight) in raw_joints
            .get(index)
            .into_iter()
            .flat_map(|slots| slots.iter())
            .zip(normalized.iter())
        {
            if *weight > 0.0
                && let Some(Some(matrix)) = normal_matrices.get(usize::from(*slot))
            {
                blended = matrix
                    .transform_vector3(delta)
                    .mul_add(Vec3::splat(*weight), blended);
            }
        }
    }
    blended.to_array()
}
