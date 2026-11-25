//! Refit the reviewed GLB's two landings and incline. The end details, cross
//! section, materials and rail widths are untouched; only the incline grows.

use glam::Vec3;

use super::{CLIP, DEFAULT_RISE, DEFAULT_RISE_END, Dimensions, PITCH_M, RISE_START};
use crate::engine::gltf::{
    AnimationInterpolation, AnimationPath, PropAnimation, PropAnimationChannel, PropModel,
    PropNode, PropSubmesh, PropVertex,
};

pub fn fit_model(source: &PropModel, dimensions: Dimensions) -> Result<PropModel, String> {
    let node = |name: &str| {
        source
            .nodes
            .iter()
            .position(|node| node.name == name)
            .and_then(|index| u16::try_from(index).ok())
            .ok_or_else(|| format!("Escalator rig is missing {name}"))
    };
    let fixed = node("Escalator_Fixed")?;
    let glass = node("Escalator_Glazing")?;
    let tread = node("Tread_02")?;
    let tread_origin = source
        .nodes
        .get(usize::from(tread))
        .ok_or("Missing tread")?
        .translation;
    let mut model = PropModel {
        textures: source.textures.clone(),
        materials: source.materials,
        ..PropModel::default()
    };
    let mut channels = Vec::with_capacity(usize::from(dimensions.count).saturating_add(1));
    for (index, &part) in source.submeshes.iter().enumerate() {
        let vertices = part_vertices(source, part)?;
        let Some((_, owner)) = vertices.first() else {
            continue;
        };
        if *owner == fixed || *owner == glass {
            let key = add_node(&mut model, Vec3::ZERO)?;
            append(
                &mut model,
                part,
                vertices
                    .iter()
                    .map(|&(vertex, _)| (fit_shell(vertex, dimensions), key)),
            )?;
        } else if *owner == tread {
            // Keep the original material/texture definition. Batches need not
            // carry twenty-four identical material ranges for repeated steps.
            let first = model.indices.len();
            for step in 0..=dimensions.count {
                let start = (f32::from(step) - 0.5) * dimensions.pitch;
                let rest = tread_position(dimensions, start);
                let key = add_node(&mut model, rest)?;
                channels.push(channel(dimensions, start, key));
                for &(mut vertex, _) in &vertices {
                    let [x, y, z] = vertex.pos;
                    let local_z = (z - tread_origin[2]) * (dimensions.pitch / PITCH_M);
                    vertex.pos = [x, y - tread_origin[1] + rest.y, local_z + rest.z];
                    push(&mut model, vertex, key)?;
                }
            }
            add_range(&mut model, part, first)?;
        } else if index < 2 {
            return Err("Escalator rig shell layout has changed".into());
        }
    }
    if model.submeshes.len() != 3 || channels.is_empty() {
        return Err("Escalator rig must have two shell pieces and the shared tread".into());
    }
    model.triangles = model.indices.len() / 3;
    model.animations.push(PropAnimation {
        name: CLIP.into(),
        duration: 1.0,
        channels,
        looped: true,
        reference_speed_mps: None,
        kind: None,
    });
    Ok(model)
}

fn part_vertices(source: &PropModel, part: PropSubmesh) -> Result<Vec<(PropVertex, u16)>, String> {
    let first = usize::try_from(part.first_index).map_err(|error| error.to_string())?;
    let count = usize::try_from(part.index_count).map_err(|error| error.to_string())?;
    source
        .indices
        .get(first..first.saturating_add(count))
        .ok_or("Invalid escalator model indices")?
        .iter()
        .map(|&vertex_index| {
            let index = usize::from(vertex_index);
            let vertex = *source
                .vertices
                .get(index)
                .ok_or("Invalid escalator model vertex")?;
            let [node, ..] = *source
                .joints
                .get(index)
                .ok_or("Escalator rig lost its rigid nodes")?;
            Ok((vertex, node))
        })
        .collect()
}

fn fit_shell(mut vertex: PropVertex, dims: Dimensions) -> PropVertex {
    let [x, y, z] = vertex.pos;
    let stretch = (dims.rise - DEFAULT_RISE) / DEFAULT_RISE;
    let lift = ((z - RISE_START) * 0.5).clamp(0.0, DEFAULT_RISE) * stretch;
    vertex.pos = [x, y + lift, lift.mul_add(2.0, z)];
    if (RISE_START..DEFAULT_RISE_END).contains(&z)
        && let Some([nx, ny, nz]) = vertex.normal
    {
        vertex.normal = Some(
            Vec3::new(nx, ny, (ny * stretch).mul_add(-0.5, nz) / (1.0 + stretch))
                .normalize_or_zero()
                .to_array(),
        );
    }
    vertex
}

fn tread_position(dims: Dimensions, z: f32) -> Vec3 {
    let half = dims.pitch * 0.5;
    let centre = z.clamp(half, dims.length - half);
    // Retire steps below the integral plates, not across open space. At the
    // cycle seam the entire set advances by one pitch with the same pose.
    let foot = ((centre - 0.30) / 0.15).clamp(0.0, 1.0);
    let crest = ((centre - dims.rise_end) / 0.30).clamp(0.0, 1.0);
    let y = crest.mul_add(-0.125, (1.0 - foot).mul_add(-0.115, dims.height(centre)));
    Vec3::new(0.0, y, centre)
}

fn channel(dims: Dimensions, start: f32, node: u16) -> PropAnimationChannel {
    let half = dims.pitch * 0.5;
    let mut times = [
        0.0,
        half,
        0.30,
        0.45,
        RISE_START,
        dims.rise_end,
        dims.rise_end + 0.30,
        dims.length - half,
    ]
    .into_iter()
    .skip(1)
    .map(|z| (z - start) / dims.pitch)
    .filter(|&time| time > 0.0 && time < 1.0)
    .collect::<Vec<_>>();
    times.extend([0.0, 1.0]);
    times.sort_by(f32::total_cmp);
    times.dedup_by(|a, b| a.to_bits() == b.to_bits());
    let values = times
        .iter()
        .flat_map(|&t| tread_position(dims, t.mul_add(dims.pitch, start)).to_array())
        .collect();
    PropAnimationChannel {
        node,
        path: AnimationPath::Translation,
        interpolation: AnimationInterpolation::Linear,
        times,
        values,
        values_per_key: 3,
    }
}

fn add_node(model: &mut PropModel, at: Vec3) -> Result<u16, String> {
    let index = u16::try_from(model.nodes.len()).map_err(|error| error.to_string())?;
    model.nodes.push(PropNode {
        name: format!("Escalator segment {index}"),
        parent: None,
        children: Vec::new(),
        translation: at.to_array(),
        rotation: [0.0, 0.0, 0.0, 1.0],
        scale: [1.0; 3],
    });
    Ok(index)
}

fn push(model: &mut PropModel, vertex: PropVertex, node: u16) -> Result<(), String> {
    model
        .indices
        .push(u16::try_from(model.vertices.len()).map_err(|error| error.to_string())?);
    model.vertices.push(vertex);
    model.joints.push([node, 0, 0, 0]);
    model.weights.push([1.0, 0.0, 0.0, 0.0]);
    Ok(())
}

fn append(
    model: &mut PropModel,
    part: PropSubmesh,
    vertices: impl Iterator<Item = (PropVertex, u16)>,
) -> Result<(), String> {
    let first = model.indices.len();
    for (vertex, node) in vertices {
        push(model, vertex, node)?;
    }
    add_range(model, part, first)
}

fn add_range(model: &mut PropModel, part: PropSubmesh, first: usize) -> Result<(), String> {
    model.submeshes.push(PropSubmesh {
        first_index: u32::try_from(first).map_err(|error| error.to_string())?,
        index_count: u32::try_from(model.indices.len().saturating_sub(first))
            .map_err(|error| error.to_string())?,
        double_sided: part.double_sided,
        response: part.response,
        material: part.material,
        texture: part.texture,
        emission: part.emission,
        alpha: part.alpha,
    });
    Ok(())
}
