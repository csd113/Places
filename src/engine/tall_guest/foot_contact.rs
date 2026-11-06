//! Sole contact correction for the extra physical doorway leg flex. The
//! skinned surface, rather than an ankle socket alone, determines support.
use super::Guest;
use glam::{Quat, Vec3};

impl Guest {
    pub(super) fn sole_samples(&self) -> [Vec<usize>; 3] {
        ["front", "left", "right"].map(|suffix| {
            self.asset
                .model
                .vertices
                .iter()
                .enumerate()
                .filter_map(|(index, _)| {
                    let joints = self.asset.model.joints.get(index)?;
                    let weights = self.asset.model.weights.get(index)?;
                    let slot = weights
                        .iter()
                        .enumerate()
                        .max_by(|(_, a), (_, b)| a.total_cmp(b))?
                        .0;
                    let joint = *joints.get(slot)?;
                    let skin = self.asset.model.skin.as_ref()?;
                    let node = *skin.joints.get(usize::from(joint))?;
                    let name = &skin.nodes.get(usize::from(node))?.name;
                    (name == &format!("foot_{suffix}") || name == &format!("leg_{suffix}_lower"))
                        .then_some(index)
                })
                .collect()
        })
    }

    fn sole_minimum(&self, index: usize) -> Option<f32> {
        let world = self.transform();
        self.sole_vertices
            .get(index)?
            .iter()
            .filter_map(|slot| {
                let vertex = self.asset.model.vertices.get(*slot)?;
                let joints = self.asset.model.joints.get(*slot)?;
                let weights = self.asset.model.weights.get(*slot)?;
                Some(
                    world
                        .transform_point3(Vec3::from(
                            self.animator.skin_position(*joints, *weights, vertex.pos),
                        ))
                        .y,
                )
            })
            .min_by(f32::total_cmp)
    }

    pub(super) fn seat_sole(
        &mut self,
        index: usize,
        suffix: &str,
        mut target: Vec3,
        rotation: Quat,
    ) {
        if self.extra_lowering() <= 0.0 {
            return;
        }
        let floor = self
            .support_heights
            .get(index)
            .copied()
            .unwrap_or(self.state.position.y);
        for _ in 0_u8..4 {
            let Some(minimum) = self.sole_minimum(index) else {
                return;
            };
            let change = (floor + 0.001 - minimum).clamp(-0.04, 0.04);
            if change.abs() < 0.0001 {
                break;
            }
            target.y += change;
            self.solve_leg(suffix, target, rotation);
        }
        let requested = self.transform().transform_point3(target);
        if let Some(actual) = self
            .socket(&format!("foot_{suffix}"))
            .map(|matrix| matrix.transform_point3(Vec3::ZERO))
            && actual.distance(requested) <= 0.002
            && let Some(Some(plant)) = self.state.plants.get_mut(index)
        {
            plant.position.y = actual.y;
        }
    }
}
