//! Object identities attached to shared scene bounds and actual model triangles.

use glam::Vec3;

use crate::editor::document::ObjectId;
use crate::engine::level::{LevelDef, LevelSurfaces, WallAxis, wall_solid_slices_profiled};
use crate::engine::loader::PropCatalog;
use crate::engine::props::PropAssets;
use crate::engine::spatial::Aabb;

#[path = "terrain_picking.rs"]
mod terrain;
use terrain::terrain_objects;

#[cfg(test)]
#[path = "../tests/terrain_picking.rs"]
mod tests;

pub(super) struct PickObject {
    pub id: ObjectId,
    pub bounds: Aabb,
    boxes: Vec<Aabb>,
    triangles: Vec<[Vec3; 3]>,
}

impl PickObject {
    const fn boxed(id: ObjectId, bounds: Aabb, boxes: Vec<Aabb>) -> Self {
        Self {
            id,
            bounds,
            boxes,
            triangles: Vec::new(),
        }
    }

    pub fn distance(&self, origin: Vec3, direction: Vec3) -> Option<f32> {
        let _bounds_hit = ray_box(origin, direction, self.bounds)?;
        self.boxes
            .iter()
            .filter_map(|bounds| ray_box(origin, direction, *bounds))
            .chain(
                self.triangles
                    .iter()
                    .filter_map(|triangle| ray_triangle(origin, direction, *triangle)),
            )
            .min_by(f32::total_cmp)
    }
}

pub(super) fn objects(
    level: &LevelDef,
    catalog: &PropCatalog,
    assets: &mut PropAssets,
    lighting: &crate::engine::lighting::LevelLighting,
) -> Vec<PickObject> {
    let mut result = terrain_objects(level);
    let surfaces = LevelSurfaces::new(level);
    for (index, wall) in level.walls.iter().enumerate() {
        let (x, z) = wall.min_corner();
        let slices = wall_solid_slices_profiled(
            wall,
            |offset| surfaces.clear_ceiling_height_along(wall, offset),
            &surfaces.wall_profile_breaks(wall),
        );
        let mut bounds = Aabb::EMPTY;
        let boxes: Vec<_> = slices
            .into_iter()
            .map(|slice| {
                let slice_bounds = match wall.axis() {
                    WallAxis::X => Aabb {
                        min: [x + slice.start, slice.bottom, z],
                        max: [x + slice.end, slice.top, z + wall.depth.abs()],
                    },
                    WallAxis::Z => Aabb {
                        min: [x, slice.bottom, z + slice.start],
                        max: [x + wall.width.abs(), slice.top, z + slice.end],
                    },
                };
                bounds = bounds.union(&slice_bounds);
                slice_bounds
            })
            .collect();
        result.push(PickObject::boxed(ObjectId::Wall(index), bounds, boxes));
    }
    for (index, prop) in level.props.iter().enumerate() {
        let definition = catalog.get(&prop.model);
        let base = surfaces.floor_y_at(prop.x, prop.z).unwrap_or(0.0);
        let matrix = prop.placement_matrix(base);
        let model = definition
            .model
            .as_deref()
            .and_then(|path| assets.resolve(path).ok());
        if let Some(asset) = model {
            let mut bounds = Aabb::EMPTY;
            let triangles = asset
                .model
                .indices
                .as_chunks::<3>()
                .0
                .iter()
                .filter_map(|[a, b, c]| {
                    let transform = |vertex_index| {
                        asset
                            .model
                            .vertices
                            .get(usize::from(vertex_index))
                            .map(|vertex| matrix.transform_point3(Vec3::from_array(vertex.pos)))
                    };
                    let triangle = [transform(*a)?, transform(*b)?, transform(*c)?];
                    for position in triangle {
                        bounds.expand(position.to_array());
                    }
                    Some(triangle)
                })
                .collect();
            result.push(PickObject {
                id: ObjectId::Prop(index),
                bounds,
                boxes: Vec::new(),
                triangles,
            });
        } else {
            let bounds = prop.placement_bounds(base, prop.size.unwrap_or(definition.size));
            result.push(PickObject::boxed(
                ObjectId::Prop(index),
                bounds,
                vec![bounds],
            ));
        }
    }
    gameplay_objects(level, catalog, assets, lighting, &mut result);
    result
}

fn gameplay_objects(
    level: &LevelDef,
    catalog: &PropCatalog,
    assets: &mut PropAssets,
    lighting: &crate::engine::lighting::LevelLighting,
    result: &mut Vec<PickObject>,
) {
    fixture_objects(level, catalog, assets, lighting, result);
    for (index, sound) in level.sound_emitters.iter().enumerate() {
        let bounds = Aabb {
            min: [sound.x - 0.16, sound.y - 0.16, sound.z - 0.16],
            max: [sound.x + 0.16, sound.y + 0.16, sound.z + 0.16],
        };
        result.push(PickObject::boxed(
            ObjectId::Audio(index),
            bounds,
            vec![bounds],
        ));
    }
    for (index, door) in level.doors.iter().enumerate() {
        let bounds = transformed_bounds(
            Aabb {
                min: [0.0, 0.0, -door.thickness * 0.5],
                max: [door.width, door.height, door.thickness * 0.5],
            },
            glam::Mat4::from_rotation_translation(
                glam::Quat::from_rotation_y(door.rotation_degrees.to_radians()),
                Vec3::new(door.x, door.base_y(level), door.z),
            ),
        );
        result.push(PickObject::boxed(
            ObjectId::Door(index),
            bounds,
            vec![bounds],
        ));
    }
    for (index, volume) in level.volumes.iter().enumerate() {
        let (bottom, top) = volume.resolved_y_bounds(level);
        let bounds = Aabb {
            min: [volume.x, bottom, volume.z],
            max: [volume.x + volume.width, top, volume.z + volume.depth],
        };
        result.push(PickObject::boxed(
            ObjectId::Trigger(index),
            bounds,
            vec![bounds],
        ));
    }
    for (index, point) in level.spawn_points.iter().enumerate() {
        let y = point.y.unwrap_or_else(|| {
            LevelSurfaces::new(level)
                .floor_y_at(point.x, point.z)
                .unwrap_or(0.0)
        });
        let bounds = Aabb {
            min: [point.x - 0.25, y, point.z - 0.25],
            max: [point.x + 0.25, y + 0.5, point.z + 0.25],
        };
        result.push(PickObject::boxed(
            ObjectId::SpawnPoint(index),
            bounds,
            vec![bounds],
        ));
    }
    for (index, fog) in level.fog_regions.iter().enumerate() {
        let bounds = Aabb {
            min: fog.min,
            max: fog.max,
        };
        result.push(PickObject::boxed(
            ObjectId::FogRegion(index),
            bounds,
            vec![bounds],
        ));
    }
    for (index, fog) in level.ground_fog.iter().enumerate() {
        let bounds = Aabb {
            min: [fog.min[0], fog.ground_y, fog.min[1]],
            max: [fog.max[0], fog.ground_y + fog.height, fog.max[1]],
        };
        result.push(PickObject::boxed(
            ObjectId::GroundFog(index),
            bounds,
            vec![bounds],
        ));
    }
}

fn fixture_objects(
    level: &LevelDef,
    catalog: &PropCatalog,
    assets: &mut PropAssets,
    lighting: &crate::engine::lighting::LevelLighting,
    result: &mut Vec<PickObject>,
) {
    for (index, fixture) in level.ceiling_lights.iter().enumerate() {
        let yaw = if fixture.mount == crate::engine::level::LightMount::Wall {
            fixture.rotation_degrees
        } else if crate::engine::lighting::fixture_is_turned(fixture.rotation_degrees) {
            -90.0
        } else {
            0.0
        };
        let matrix = glam::Mat4::from_rotation_translation(
            glam::Quat::from_rotation_y(yaw.to_radians()),
            Vec3::new(fixture.x, lighting.fixture_y_for(fixture), fixture.z),
        );
        let model = crate::engine::lighting::fixture_model_id(&fixture.fixture)
            .and_then(|id| catalog.get(id).model)
            .and_then(|path| assets.resolve(&path).ok());
        if let Some(asset) = model {
            let mut bounds = Aabb::EMPTY;
            let triangles = asset
                .model
                .indices
                .as_chunks::<3>()
                .0
                .iter()
                .filter_map(|indices| {
                    let mut triangle = [Vec3::ZERO; 3];
                    for (output, vertex_index) in triangle.iter_mut().zip(indices) {
                        *output = matrix.transform_point3(Vec3::from_array(
                            asset.model.vertices.get(usize::from(*vertex_index))?.pos,
                        ));
                        bounds.expand(output.to_array());
                    }
                    Some(triangle)
                })
                .collect();
            result.push(PickObject {
                id: ObjectId::Light(index),
                bounds,
                boxes: Vec::new(),
                triangles,
            });
        } else {
            let profile = crate::engine::lighting::fixture_profile(&fixture.fixture);
            let local = if fixture.mount == crate::engine::level::LightMount::Wall {
                Aabb {
                    min: [-profile.half_width, -0.1, 0.0],
                    max: [profile.half_width, 0.1, 0.11],
                }
            } else {
                Aabb {
                    min: [-profile.half_width, -0.1, -profile.half_depth],
                    max: [profile.half_width, 0.0, profile.half_depth],
                }
            };
            let bounds = transformed_bounds(local, matrix);
            result.push(PickObject::boxed(
                ObjectId::Light(index),
                bounds,
                vec![bounds],
            ));
        }
    }
}

fn transformed_bounds(local: Aabb, matrix: glam::Mat4) -> Aabb {
    let mut bounds = Aabb::EMPTY;
    for x in [local.min[0], local.max[0]] {
        for y in [local.min[1], local.max[1]] {
            for z in [local.min[2], local.max[2]] {
                bounds.expand(matrix.transform_point3(Vec3::new(x, y, z)).to_array());
            }
        }
    }
    bounds
}

fn ray_box(origin: Vec3, direction: Vec3, bounds: Aabb) -> Option<f32> {
    if bounds.is_empty() {
        return None;
    }
    crate::engine::collision::ray_aabb_entry(origin, direction, bounds.min, bounds.max)
}

#[expect(
    clippy::arithmetic_side_effects,
    reason = "Ray/triangle intersection uses finite floating point vector arithmetic without integer overflow."
)]
fn ray_triangle(origin: Vec3, direction: Vec3, [first, second, third]: [Vec3; 3]) -> Option<f32> {
    let edge_one = second - first;
    let edge_two = third - first;
    let perpendicular = direction.cross(edge_two);
    let determinant = edge_one.dot(perpendicular);
    if determinant.abs() < 0.000_001 {
        return None;
    }
    let inverse = determinant.recip();
    let offset = origin - first;
    let u = offset.dot(perpendicular) * inverse;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let cross = offset.cross(edge_one);
    let v = direction.dot(cross) * inverse;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let distance = edge_two.dot(cross) * inverse;
    (distance >= 0.0).then_some(distance)
}
