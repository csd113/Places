//! Authored passenger-lift meshes. Gameplay remains the only transform owner.
use crate::engine::elevator::{
    CABIN_DOOR_Z, DOOR_WIDTH, Elevator, LANDING_DOOR_Z, MODEL_IDS, Phase,
};
use crate::engine::gltf::{PropModel, PropSubmesh};
use crate::engine::loader::PropCatalog;
use crate::engine::props::PropAssets;

#[derive(Clone, Copy)]
pub enum Part {
    Cabin,
    Frame,
    Leaf,
}
pub struct Models {
    pub cabin: PropModel,
    pub frame: PropModel,
    pub leaf: PropModel,
}
impl Models {
    #[must_use]
    pub const fn model(&self, part: Part) -> &PropModel {
        match part {
            Part::Cabin => &self.cabin,
            Part::Frame => &self.frame,
            Part::Leaf => &self.leaf,
        }
    }
}
/// Resolve through the normal bounded GLB cache; package identities cover all parts.
/// # Errors
/// Refuses missing/malformed assets or an inconsistent shared fitted atlas.
pub fn models(
    catalog: &PropCatalog,
    assets: &mut PropAssets,
    stops: usize,
) -> Result<Models, String> {
    let mut resolved = Vec::new();
    for id in MODEL_IDS {
        let path = catalog
            .assets()
            .get(id)
            .and_then(|entry| entry.model.as_deref())
            .ok_or_else(|| format!("Elevator model {id} is absent from the catalog"))?;
        resolved.push(assets.resolve(path)?);
    }
    let cabin = resolved
        .first()
        .ok_or("Missing elevator cabin")?
        .model
        .clone();
    let frame = resolved
        .get(1)
        .ok_or("Missing elevator frame")?
        .model
        .clone();
    let leaf = resolved
        .get(2)
        .ok_or("Missing elevator leaf")?
        .model
        .clone();
    let button = &resolved.get(3).ok_or("Missing elevator button")?.model;
    Ok(Models {
        cabin: attach_buttons(cabin, button, stops)?,
        frame,
        leaf,
    })
}
fn attach_buttons(
    mut cabin: PropModel,
    button: &PropModel,
    stops: usize,
) -> Result<PropModel, String> {
    if cabin.textures.len() != 1
        || button.textures.len() != 1
        || cabin.textures.first().map(|t| &t.rgba) != button.textures.first().map(|t| &t.rgba)
    {
        return Err("Elevator components must share the fitted PNG atlas".into());
    }
    let mut groups: Vec<(PropSubmesh, Vec<u16>)> = Vec::new();
    let mut collect = |model: &PropModel, vertex_offset: u16| -> Result<(), String> {
        for sub in &model.submeshes {
            let start = usize::try_from(sub.first_index)
                .map_err(|_conversion_error| "Invalid elevator index")?;
            let count = usize::try_from(sub.index_count)
                .map_err(|_conversion_error| "Invalid elevator index count")?;
            let end = start.checked_add(count).ok_or("Elevator index overflow")?;
            let source = model
                .indices
                .get(start..end)
                .ok_or("Invalid elevator primitive range")?;
            let existing = groups.iter().position(|(other, _)| {
                other.texture == sub.texture
                    && other.emission == sub.emission
                    && other.alpha == sub.alpha
                    && other.response == sub.response
            });
            let target = existing.unwrap_or_else(|| {
                groups.push((*sub, Vec::new()));
                groups.len().saturating_sub(1)
            });
            let (_, indices) = groups
                .get_mut(target)
                .ok_or("Missing elevator primitive group")?;
            for index in source {
                indices.push(
                    index
                        .checked_add(vertex_offset)
                        .ok_or("Elevator vertex budget exceeded")?,
                );
            }
        }
        Ok(())
    };
    collect(&cabin, 0)?;
    for row in 0..stops.min(8) {
        let offset = u16::try_from(cabin.vertices.len())
            .map_err(|_conversion_error| "Elevator vertex budget exceeded")?;
        collect(button, offset)?;
        let y =
            f32::from(u16::try_from(row).map_err(|_conversion_error| "Elevator button overflow")?)
                * 0.11;
        cabin
            .vertices
            .extend(button.vertices.iter().copied().map(|mut vertex| {
                vertex.pos[1] -= y;
                vertex
            }));
    }
    cabin.indices.clear();
    cabin.submeshes.clear();
    for (mut sub, indices) in groups {
        sub.first_index = u32::try_from(cabin.indices.len())
            .map_err(|_conversion_error| "Elevator index budget exceeded")?;
        sub.index_count = u32::try_from(indices.len())
            .map_err(|_conversion_error| "Elevator index budget exceeded")?;
        cabin.indices.extend(indices);
        cabin.submeshes.push(sub);
    }
    cabin.triangles = cabin.indices.len() / 3;
    Ok(cabin)
}
/// The ceiling practical follows the car instead of being baked at a landing.
#[must_use]
pub fn cabin_light(elevator: &Elevator) -> super::dynamic_lights::DynamicLight {
    super::dynamic_lights::DynamicLight {
        key: format!("elevator:{}", elevator.def.id),
        position: elevator.def.point(0.0, elevator.y + 2.30, 1.20),
        color: [1.0, 0.69, 0.38],
        intensity: 2.0,
        radius: 3.2,
    }
}
#[must_use]
pub fn parts(e: &Elevator) -> Vec<(Part, [f32; 3])> {
    let mut parts = vec![(Part::Cabin, e.def.point(0.0, e.y, 0.0).into())];
    let half = DOOR_WIDTH * 0.5;
    let leaves = |y, z, progress: f32| {
        let open = crate::engine::elevator::slide_fraction(progress);
        [
            (
                Part::Leaf,
                e.def.point(half.mul_add(-open, -half), y, z).into(),
            ),
            (
                Part::Leaf,
                e.def.point(half.mul_add(open, 0.003), y, z).into(),
            ),
        ]
    };
    parts.extend(leaves(e.y, CABIN_DOOR_Z, e.opening));
    for (i, stop) in e
        .def
        .stops
        .iter()
        .enumerate()
        .filter(|(_, s)| s.level.is_none())
    {
        parts.push((Part::Frame, e.def.point(0.0, stop.y, 0.0).into()));
        let open = if i == e.at && e.phase != Phase::Moving {
            e.opening
        } else {
            0.0
        };
        parts.extend(leaves(stop.y, LANDING_DOOR_Z, open));
    }
    parts
}
