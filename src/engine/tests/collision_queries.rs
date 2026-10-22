//! Protect ray candidate completeness and stable order when repeated samples
//! share a cell. A 64-box numerical fixture covers boundary, reverse, vertical,
//! outside-grid and fallback queries; no assets, baking or timing assertions.

use crate::engine::collision::{WallAabb, ray_aabb_entry};
use crate::engine::collision_index::CollisionIndex;
use glam::Vec3;

#[test]
fn ray_neighbourhood_reuse_preserves_exact_hits_and_candidate_order() {
    let walls: Vec<_> = (0..8_u16)
        .flat_map(|z| {
            (0..8_u16).map(move |x| {
                WallAabb::with_y(
                    f32::from(x).mul_add(4.0, -16.0),
                    0.0,
                    f32::from(z).mul_add(4.0, -16.0),
                    1.0,
                    3.0,
                    1.0,
                )
            })
        })
        .collect();
    let index = CollisionIndex::build(&walls);
    for (origin, direction, length) in [
        (Vec3::new(-15.5, 1.0, -15.5), Vec3::X, 0.1),
        (Vec3::new(-30.0, 1.0, -15.5), Vec3::X, 70.0),
        (Vec3::new(30.0, 1.0, 8.5), Vec3::NEG_X, 70.0),
        (Vec3::new(-16.0, 1.0, -16.0), Vec3::new(0.6, 0.0, 0.8), 70.0),
        (Vec3::new(16.0, 1.0, 16.0), Vec3::new(-0.6, 0.0, -0.8), 70.0),
        (Vec3::new(0.5, -10.0, 0.5), Vec3::Y, 20.0),
        (Vec3::new(-30.0, 1.0, -30.0), Vec3::X, 1.0e8),
    ] {
        let mut candidates = Vec::new();
        index.for_each_ray(origin, direction, length, &walls, |wall| {
            let id = walls.iter().position(|source| std::ptr::eq(source, wall));
            assert!(
                id.is_some(),
                "every candidate borrows the installed collision world"
            );
            if let Some(source_index) = id {
                candidates.push(source_index);
            }
        });
        assert!(
            candidates
                .iter()
                .zip(candidates.iter().skip(1))
                .all(|(previous, next)| previous < next),
            "candidates retain unique ascending source order"
        );
        for (id, wall) in walls.iter().enumerate() {
            let hit = ray_aabb_entry(
                origin,
                direction,
                [wall.min_x, wall.min_y, wall.min_z],
                [wall.max_x, wall.max_y, wall.max_z],
            );
            if hit.is_some_and(|distance| distance <= length) {
                assert!(
                    candidates.contains(&id),
                    "indexed ray must retain every linear hit: {origin:?}, {direction:?}, box {id}"
                );
            }
        }
    }
}
