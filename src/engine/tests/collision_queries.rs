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

// Value: native paced door playback found that the accepted 100-micron start
// band became a negative SAT entry, allowing an inward step and then freezing
// every later input. Nine door/body combinations protect incoming refusal and
// ordinary separating motion; a deep invalid start still cannot teleport out.
// One empty room, no assets or baking; tens of sweeps and negligible storage.
#[test]
fn shallow_hull_contact_blocks_incoming_motion_without_freezing_separation() -> Result<(), String> {
    use crate::engine::collision::{CONTACT_EPS, DoorCollider, HorizontalWorld, sweep_player_hull};
    use crate::engine::level::{LevelDef, WalkableCeiling, WalkableFloor};
    use glam::Vec2;
    let translated = |p: Vec2, d: Vec2, t: f32| Vec2::new(d.x.mul_add(t, p.x), d.y.mul_add(t, p.y));
    let level = LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"contact_band","name":"Contact band","spawn":{"x":0,"z":0},"rooms":[{"x":-4,"z":-4,"width":8,"depth":8,"height":4}]}"#).map_err(|error|error.to_string())?;
    let floor = WalkableFloor::from_level(&level);
    let ceiling = WalkableCeiling::from_level(&level);
    let index = CollisionIndex::build(&[]);
    for half in [0.07_f32, 0.4, 1.006] {
        for degrees in [0.0_f32, 43.35, 90.0] {
            let angle = degrees.to_radians();
            let direction = Vec2::new(angle.cos(), angle.sin());
            let normal = Vec2::new(-direction.y, direction.x);
            let hinge = translated(Vec2::ZERO, direction, -1.45);
            let doors = [DoorCollider::from_pose(
                [hinge.x, 0.0, hinge.y],
                direction.into(),
                2.9,
                0.075,
                3.0,
            )];
            let world = HorizontalWorld {
                index: &index,
                walls: &[],
                doors: &doors,
                ceiling: &ceiling,
                floor: &floor,
                debug: false,
            };
            let support = half * (normal.x.abs() + normal.y.abs());
            let from = translated(
                Vec2::ZERO,
                normal,
                CONTACT_EPS.mul_add(-0.5, support + 0.0375),
            );
            let incoming = sweep_player_hull(
                &world,
                (from, translated(from, normal, -0.05)),
                (0.0, 1.8, half),
                true,
            );
            assert!(
                incoming.distance(from) < 0.00001,
                "contact band must not admit an inward step: {half}, {degrees}, {incoming:?}"
            );
            let target = translated(from, normal, 0.05);
            let separating = sweep_player_hull(&world, (from, target), (0.0, 1.8, half), true);
            assert!(
                separating.distance(target) < 0.00001,
                "ordinary outward input must escape contact: {half}, {degrees}, {separating:?}"
            );
            let deep = translated(Vec2::ZERO, normal, 0.0375);
            assert_eq!(
                sweep_player_hull(&world, (deep, target), (0.0, 1.8, half), true),
                deep
            );
        }
    }
    Ok(())
}
