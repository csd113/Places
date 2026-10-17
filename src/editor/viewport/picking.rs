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
            let [width, height, depth] = prop.resolved_size(definition.size);
            let bounds = Aabb {
                min: [
                    width.mul_add(-0.5, prop.x),
                    base + prop.y,
                    depth.mul_add(-0.5, prop.z),
                ],
                max: [
                    width.mul_add(0.5, prop.x),
                    base + prop.y + height,
                    depth.mul_add(0.5, prop.z),
                ],
            };
            result.push(PickObject::boxed(
                ObjectId::Prop(index),
                bounds,
                vec![bounds],
            ));
        }
    }
    result
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
