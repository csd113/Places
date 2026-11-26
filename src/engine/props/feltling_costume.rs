//! Cosmetic-only geometry on the canonical Feltling's existing joints.
use glam::{Mat4, Vec3};
use std::sync::{Arc, Weak};

use super::{LoadedPropAsset, PropAssets};
use crate::engine::gltf::{PropModel, SourceVertex};

pub const FELTLING_ID: &str = "feltling";
const MODEL: &str = "entities/feltling/model/feltling.glb";
pub const PARTS: [(&str, &str); 2] = [
    ("head", "entities/feltling/costumes/feltling_elf_hat.glb"),
    (
        "spine",
        "entities/feltling/costumes/feltling_elf_collar.glb",
    ),
];

#[derive(Debug, Clone, Copy, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FeltlingCostume {
    Elf,
}

impl PropAssets {
    pub(crate) fn fitted_feltling(
        &mut self,
        base: &Arc<LoadedPropAsset>,
        costume: Option<FeltlingCostume>,
    ) -> Option<Arc<LoadedPropAsset>> {
        let FeltlingCostume::Elf = costume?;
        match self.elf_feltling(base) {
            Ok(fitted) => Some(fitted),
            Err(error) => {
                crate::engine::logging::warn_once("feltling-elf-part", error);
                None
            }
        }
    }

    pub(crate) fn elf_feltling(
        &mut self,
        base: &Arc<LoadedPropAsset>,
    ) -> Result<Arc<LoadedPropAsset>, String> {
        if base.model_path != MODEL {
            return Err("Elf attire requires the canonical Feltling rig".into());
        }
        if let Some(cached) = self.elf_feltling.as_ref().and_then(Weak::upgrade) {
            return Ok(cached);
        }
        let mut model = base.model.clone();
        for (joint, path) in PARTS {
            let part = self.resolve(path)?;
            append_joint_mesh(&mut model, joint, &part.model)?;
        }
        let fitted = Arc::new(LoadedPropAsset {
            model_path: format!("{MODEL}#elf"),
            model,
        });
        self.elf_feltling = Some(Arc::downgrade(&fitted));
        Ok(fitted)
    }
}

fn add_offset(value: u16, offset: usize) -> Result<u16, String> {
    u16::try_from(usize::from(value).saturating_add(offset))
        .map_err(|error| format!("Feltling costume exceeds mesh index range: {error}"))
}

fn append_joint_mesh(base: &mut PropModel, name: &str, part: &PropModel) -> Result<(), String> {
    if part.is_animatable() || !base.morph_targets.is_empty() {
        return Err("Feltling attire needs a rigid overlay and its reviewed base rig".into());
    }
    let skin = base.skin.as_mut().ok_or("Feltling costume requires skin")?;
    let slot = skin
        .joints
        .iter()
        .position(|index| {
            skin.nodes
                .get(usize::from(*index))
                .is_some_and(|node| node.name == name)
        })
        .ok_or_else(|| format!("Feltling costume joint {name} is missing"))?;
    let joint = u16::try_from(slot)
        .map_err(|error| format!("Feltling costume joint index is too large: {error}"))?;
    let mut current = skin.joints.get(slot).copied();
    let mut rest = Mat4::IDENTITY;
    for _ in 0..skin.nodes.len() {
        let Some(index) = current else { break };
        let node = skin
            .nodes
            .get(usize::from(index))
            .ok_or("Invalid Feltling joint hierarchy")?;
        rest = node.local_transform().mul_mat4(&rest);
        current = node.parent;
    }
    if current.is_some() {
        return Err("Feltling costume joint hierarchy is cyclic".into());
    }
    let vertex_offset = base.vertices.len();
    let index_offset = u32::try_from(base.indices.len())
        .map_err(|error| format!("Too many Feltling indices: {error}"))?;
    let texture_offset = base.textures.len();
    let material_offset = base.materials;
    let indices = part
        .indices
        .iter()
        .map(|i| add_offset(*i, vertex_offset))
        .collect::<Result<Vec<_>, _>>()?;
    for source in &part.vertices {
        let mut vertex = *source;
        vertex.pos = rest
            .transform_point3(Vec3::from_array(vertex.pos))
            .to_array();
        vertex.normal = vertex.normal.map(|normal| {
            rest.transform_vector3(Vec3::from_array(normal))
                .normalize_or_zero()
                .to_array()
        });
        skin.source_vertices.push(SourceVertex {
            position: vertex.pos,
            normal: vertex.normal,
        });
        base.vertices.push(vertex);
        base.joints.push([joint, 0, 0, 0]);
        base.weights.push([1.0, 0.0, 0.0, 0.0]);
    }
    for source in &part.submeshes {
        let mut range = *source;
        range.first_index = range.first_index.saturating_add(index_offset);
        range.material = add_offset(range.material, material_offset)?;
        range.texture = range
            .texture
            .map(|i| add_offset(i, texture_offset))
            .transpose()?;
        base.submeshes.push(range);
    }
    base.textures.extend(part.textures.iter().cloned());
    base.indices.extend(indices);
    base.materials = base.materials.saturating_add(part.materials);
    base.triangles = base.triangles.saturating_add(part.triangles);
    Ok(())
}
