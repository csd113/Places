//! Dynamic door and lever. The cabinet itself uses the ordinary fixed prop path.

use super::dynamic::{DynamicId, DynamicScene, SpawnOrientation};
use crate::engine::{
    electrical_panel::{CONDUIT_PARTS, MOVING_PARTS, Panel, PartPose, conduit_height},
    level::{LevelDef, LevelSurfaces, PropDef, WalkableFloor},
    props::PropAssets,
};
use glam::{Mat4, Quat, Vec3};

#[cfg(test)]
#[path = "../../tests/electrical_panel_conduits.rs"]
mod tests;

pub(super) fn conduit_stock(
    level: &LevelDef,
    floor: &WalkableFloor,
    surfaces: &LevelSurfaces<'_>,
    cabinet: &PropDef,
) -> Vec<(&'static str, Mat4)> {
    let Some(height) = conduit_height(level, floor, surfaces, cabinet) else {
        return Vec::new();
    };
    let origin = cabinet
        .placement_matrix(surfaces.floor_y_at(cabinet.x, cabinet.z).unwrap_or(0.0))
        .mul_mat4(&Mat4::from_translation(Vec3::new(0.0, 0.36, 0.0)));
    let [riser, collars, clamp] = CONDUIT_PARTS;
    let mut parts = vec![(
        riser,
        origin.mul_mat4(&Mat4::from_scale(Vec3::new(1.0, height, 1.0))),
    )];
    if height >= 0.08 {
        parts.push((
            collars,
            origin.mul_mat4(&Mat4::from_translation(Vec3::new(0.0, 0.0145, 0.0))),
        ));
        parts.push((
            collars,
            origin.mul_mat4(&Mat4::from_rotation_translation(
                Quat::from_rotation_z(std::f32::consts::PI),
                Vec3::new(0.0, height - 0.0145, 0.0),
            )),
        ));
    }
    // Rigid clips never span either end fitting and never stretch with the pipes.
    for slot in 1..=16_u16 {
        let y = 1.15 * f32::from(slot);
        if y + 0.50 > height {
            break;
        }
        parts.push((
            clamp,
            origin.mul_mat4(&Mat4::from_translation(Vec3::new(0.0, y, 0.0))),
        ));
    }
    parts
}

#[derive(Default)]
pub(in crate::engine::render) struct Draws {
    objects: Vec<[Option<DynamicId>; 2]>,
}

impl Draws {
    pub(in crate::engine::render) fn clear(&mut self) {
        self.objects.clear();
    }

    pub(in crate::engine::render) fn sync(
        &mut self,
        scene: &mut DynamicScene,
        assets: &mut PropAssets,
        panels: &[Panel],
    ) -> bool {
        let initialized = self.objects.len() == panels.len();
        if !initialized {
            for object in self.objects.drain(..).flatten().flatten() {
                let _removed = scene.despawn(object);
            }
            let meshes = MOVING_PARTS.map(|path| {
                let asset = assets
                    .resolve(path)
                    .map_err(|error| {
                        crate::engine::logging::warn_once(format!("panel-part:{path}"), error);
                    })
                    .ok()?;
                let model = &asset.model;
                scene.register_model(
                    path,
                    model,
                    model.textures.clone(),
                    &model
                        .submeshes
                        .iter()
                        .map(|sub| sub.alpha)
                        .collect::<Vec<_>>(),
                )
            });
            for panel in panels {
                let [door, lever] = panel.parts();
                let [door_mesh, lever_mesh] = meshes;
                let mut spawn = |mesh: Option<usize>, pose: PartPose| {
                    scene.spawn_registered(
                        mesh?,
                        pose.origin.to_array(),
                        SpawnOrientation {
                            base_rotation: pose.rotation,
                            spin_axis: [0.0, 1.0, 0.0],
                        },
                        0.0,
                        1.0,
                    )
                };
                self.objects
                    .push([spawn(door_mesh, door), spawn(lever_mesh, lever)]);
            }
        }
        for (objects, panel) in self.objects.iter().zip(panels) {
            for (slot, pose) in objects.iter().zip(panel.parts()) {
                if let Some(id) = slot {
                    let _changed =
                        scene.set_pose(*id, pose.origin.to_array(), pose.rotation.to_array());
                }
            }
        }
        !initialized
    }
}
