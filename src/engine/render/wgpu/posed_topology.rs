//! Translate skinned character meshes into local posed lighting geometry.
use crate::engine::lighting::posed_topology::Topology;
use crate::engine::materials::AlphaMode;
use crate::engine::render::common::character::Character;
use glam::Vec3;

pub(super) struct Geometry {
    pub topology: Topology,
    /// Exact local posed positions shared by topology and ambient preparation.
    pub points: Vec<Vec3>,
}

impl Geometry {
    pub fn new(character: &Character) -> Result<Self, String> {
        let model = &character.asset().model;
        let mut points = Vec::with_capacity(model.vertices.len());
        positions(character, &mut points)?;
        let primitives = model
            .submeshes
            .iter()
            .filter(|part| part.alpha.mode == AlphaMode::Opaque)
            .map(|part| {
                let start = usize::try_from(part.first_index).map_err(|e| e.to_string())?;
                let end = start
                    .checked_add(usize::try_from(part.index_count).map_err(|e| e.to_string())?)
                    .ok_or("Posed index range overflow")?;
                let indices = model
                    .indices
                    .get(start..end)
                    .ok_or("Posed primitive range is unavailable")?;
                Ok(indices.as_chunks::<3>().0)
            });
        let topology = Topology::new(&points, primitives)?;
        Ok(Self { topology, points })
    }

    pub fn refit(&mut self, character: &Character) -> Result<(), String> {
        // Keep this allocation and skin each vertex once. Ambient sampling
        // consumes the same positions after the topology has been refitted.
        positions(character, &mut self.points)?;
        self.topology.refit(&self.points)
    }
}

fn positions(character: &Character, output: &mut Vec<Vec3>) -> Result<(), String> {
    output.clear();
    let model = &character.asset().model;
    for (index, vertex) in model.vertices.iter().enumerate() {
        let point = Vec3::from_array(character.animator().skin_position(
            model.joints.get(index).copied().unwrap_or([0; 4]),
            model.weights.get(index).copied().unwrap_or([0.0; 4]),
            vertex.pos,
        ));
        if !point.is_finite() {
            return Err("Nonfinite posed visibility vertex".into());
        }
        output.push(point);
    }
    Ok(())
}
