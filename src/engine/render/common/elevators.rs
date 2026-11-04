//! Authored passenger-lift meshes. Gameplay remains the only transform owner.
use crate::engine::elevator::{
    CABIN_DOOR_Z, DOOR_BUTTON_GLYPH, Elevator, LANDING_DOOR_Z, MODEL_IDS, Phase, Stop,
    door_button_height, door_leaf_x,
};
use crate::engine::gltf::{PropModel, PropSubmesh};
use crate::engine::loader::PropCatalog;
use crate::engine::props::PropAssets;
use glam::Vec3;

/// Analytic visibility of the manufactured opaque shell and paired door tracks.
/// It follows gameplay, including the eased leaf overlap, rather than the camera
/// or a lighting-quality setting. No static light is baked into a landing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightEnclosure {
    pub origin: Vec3,
    pub rotation_degrees: f32,
    pub half_gap: f32,
    pub landing_doors: bool,
}

impl LightEnclosure {
    #[must_use]
    pub fn is_well_formed(self) -> bool {
        self.origin.is_finite()
            && self.rotation_degrees.is_finite()
            && self.half_gap.is_finite()
            && (-crate::engine::elevator::DOOR_LEAF_SEAL
                ..=crate::engine::elevator::DOOR_LEAF_TRAVEL)
                .contains(&self.half_gap)
    }
}

#[derive(Clone, Copy)]
pub enum Part {
    Cabin(usize),
    Frame,
    Leaf,
}

/// Elevator stock lies in a wall/shaft, where a geometric-centre probe has no
/// room and falls back to invented ambient. Receive the real landing's light
/// from clear hall air instead. Keep a travelling car's departure receiver
/// until arrival; the car's own practical still follows its authoritative pose.
#[must_use]
pub fn landing_light_probe(e: &Elevator, part: Part, position: [f32; 3]) -> Vec3 {
    let local = e.def.local(Vec3::from_array(position));
    let car = matches!(part, Part::Cabin) || (matches!(part, Part::Leaf) && local.z > 0.25);
    let floor = if car {
        e.def.stops.get(e.at).map_or(e.y, |stop| stop.y)
    } else {
        local.y
    };
    e.def.point(
        0.0,
        crate::engine::elevator::DOOR_HEIGHT.mul_add(0.5, floor),
        -0.35,
    )
}

/// Select the manufactured assembly's actual adjacent receiver, not a tint.
pub fn set_landing_receiver(
    scene: &mut super::dynamic::DynamicScene,
    id: super::dynamic::DynamicId,
    elevator: &Elevator,
    part: Part,
    position: [f32; 3],
) -> bool {
    // Concealed leaf overlaps lie inside the sill/header and cannot be used
    // as light receivers for the entire exposed panel. This is a sampling
    // subset only; drawing, culling and occluding geometry retain full stock.
    let bounds = matches!(part, Part::Leaf).then_some(crate::engine::spatial::Aabb {
        min: [0.02, 0.03, crate::engine::elevator::DOOR_LEAF_FRONT + 0.001],
        max: [
            crate::engine::elevator::DOOR_LEAF_WIDTH - 0.02,
            crate::engine::elevator::DOOR_HEIGHT - 0.03,
            crate::engine::elevator::DOOR_LEAF_BACK - 0.001,
        ],
    });
    scene.set_lighting_receiver(
        id,
        Some(landing_light_probe(elevator, part, position).into()),
        bounds,
    )
}
pub struct Models {
    pub cabins: Vec<PropModel>,
    pub frame: PropModel,
    pub leaf: PropModel,
}
impl Models {
    #[must_use]
    pub fn model(&self, part: Part, _landing: usize) -> Option<&PropModel> {
        match part {
            Part::Cabin(index) => self.cabins.get(index),
            Part::Frame => Some(&self.frame),
            Part::Leaf => Some(&self.leaf),
        }
    }
}
/// Resolve through the normal bounded GLB cache; package identities cover all parts.
/// # Errors
/// Refuses missing/malformed assets or an inconsistent shared fitted atlas.
pub fn models(
    catalog: &PropCatalog,
    assets: &mut PropAssets,
    stops: &[Stop],
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
        cabins: (0..stops.len())
            .map(|current| attach_buttons(cabin.clone(), button, stops, current))
            .collect::<Result<Vec<_>, _>>()?,
        frame,
        leaf,
    })
}
fn attach_buttons(
    mut cabin: PropModel,
    button: &PropModel,
    stops: &[Stop],
    current: usize,
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
    let destinations = stops
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != current)
        .enumerate()
        .map(|(row, (index, stop))| {
            let y = f32::from(u16::try_from(row).unwrap_or(0)) * 0.11;
            (y, panel_symbol(&stop.label, index))
        });
    let door = (
        1.58 - door_button_height(stops.len().saturating_sub(1)),
        DOOR_BUTTON_GLYPH,
    );
    for (y, glyph) in destinations.chain(std::iter::once(door)) {
        let offset = u16::try_from(cabin.vertices.len())
            .map_err(|_conversion_error| "Elevator vertex budget exceeded")?;
        collect(button, offset)?;
        cabin
            .vertices
            .extend(button.vertices.iter().copied().map(|mut vertex| {
                vertex.pos[1] -= y;
                number_uv(&mut vertex.uv, glyph);
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
        enclosure: Some(LightEnclosure {
            origin: elevator.def.point(0.0, elevator.y, 0.0),
            rotation_degrees: elevator.def.rotation_degrees,
            half_gap: door_leaf_x(1.0, elevator.opening) - crate::engine::elevator::DOOR_LEAF_SEAL,
            landing_doors: elevator.phase != Phase::Moving,
        }),
    }
}
/// Every authored landing owns a fixed cabin practical as well as fixed geometry.
#[must_use]
pub fn cabin_lights(e: &Elevator) -> Vec<super::dynamic_lights::DynamicLight> {
    e.def
        .stops
        .iter()
        .enumerate()
        .filter(|(_, stop)| stop.is_landing())
        .map(|(index, stop)| {
            let mut light = cabin_light(e);
            light.key = format!("elevator:{}:{index}", e.def.id);
            light.position = e.def.point(0.0, stop.y + 2.30, 1.20);
            light
        })
        .collect()
}
#[must_use]
pub fn parts(e: &Elevator) -> Vec<(Part, [f32; 3])> {
    let mut parts = Vec::new();
    let leaves = |y, z, progress: f32| {
        [
            (
                Part::Leaf,
                e.def.point(door_leaf_x(-1.0, progress), y, z).into(),
            ),
            (
                Part::Leaf,
                e.def.point(door_leaf_x(1.0, progress), y, z).into(),
            ),
        ]
    };
    for (i, stop) in e
        .def
        .stops
        .iter()
        .enumerate()
        .filter(|(_, s)| s.is_landing())
    {
        parts.push((Part::Cabin(i), e.def.point(0.0, stop.y, 0.0).into()));
        parts.push((Part::Frame, e.def.point(0.0, stop.y, 0.0).into()));
        let open = if i == e.at && e.phase != Phase::Moving {
            e.opening
        } else {
            0.0
        };
        parts.extend(leaves(stop.y, CABIN_DOOR_Z, open));
        parts.extend(leaves(stop.y, LANDING_DOOR_Z, open));
    }
    parts
}

fn panel_symbol(label: &str, index: usize) -> u16 {
    if let Some(digit) = label.chars().find_map(|letter| letter.to_digit(10)) {
        return u16::try_from(digit).unwrap_or(0);
    }
    match label.chars().next().map(|c| c.to_ascii_uppercase()) {
        Some('G') => 10,
        Some('A') => 11,
        Some('B') => 12,
        Some('C') => 13,
        Some('D') => 14,
        _ => u16::try_from(index.saturating_add(1).min(9)).unwrap_or(1),
    }
}
fn number_uv(uv: &mut [f32; 2], glyph: u16) {
    let [u, v] = *uv;
    if (0.5..0.75).contains(&u) && (0.75..1.0).contains(&v) {
        let x = (u - 0.5) * 4.0;
        let y = (v - 0.75) * 4.0;
        *uv = [
            (f32::from(glyph % 4) + x).mul_add(0.0625, 0.5),
            (f32::from(glyph / 4) + y).mul_add(0.0625, 0.75),
        ];
    }
}
