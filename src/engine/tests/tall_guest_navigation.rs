//! The live-solid broad phase must retain thin sweep contacts, an escape from
//! initial overlap and vertical clearance. The existing moving-item route does
//! not isolate those boundaries. This fixture needs four boxes and no map bake.

use super::LiveSolids;
use crate::engine::collision::{WallAabb, dynamic_disc_segment_blocked};
use crate::engine::nav::NavDoorState;
use glam::{Vec2, Vec3};

#[test]
fn guest_navigation_keeps_exact_live_solid_contacts() {
    let solids = [
        WallAabb::with_y(4.0, 0.0, 4.0, 0.125, 1.0, 0.25),
        WallAabb::with_y(8.0, 4.0, 4.0, 0.125, 1.0, 0.25),
        WallAabb::with_y(-128.0, 0.0, -128.0, 1.0, 1.0, 1.0),
        WallAabb::with_y(128.0, 0.0, 128.0, 1.0, 1.0, 1.0),
    ];
    let live = LiveSolids(&solids);
    let radius = 0.75;
    let height = 3.25;
    let start = Vec3::new(3.0, 0.0, 4.125);
    let overlapping = Vec3::new(3.625, 0.0, 4.125);
    let end = Vec3::new(5.0, 0.0, 4.125);
    for (point, expected) in [
        (start, false),
        (overlapping, true),
        (end, false),
        (Vec3::new(3.25, 0.0, 4.125), false),
        (Vec3::new(8.0, 0.0, 4.125), false),
        (Vec3::new(8.0, 1.0, 4.125), true),
    ] {
        assert_eq!(
            live.obstacle_blocked(point, radius, height),
            expected,
            "point query at {point}"
        );
    }
    for (from, to, expected) in [
        (start, end, true),
        (overlapping, start, false),
        (overlapping, end, true),
        (
            Vec3::new(7.0, 0.0, 4.125),
            Vec3::new(9.0, 0.0, 4.125),
            false,
        ),
        (Vec3::new(7.0, 0.0, 4.125), Vec3::new(9.0, 1.0, 4.125), true),
    ] {
        let linear = dynamic_disc_segment_blocked(
            Vec2::new(from.x, from.z),
            Vec2::new(to.x, to.z),
            radius,
            from.y.min(to.y),
            height + (to.y - from.y).abs(),
            &solids,
        );
        assert_eq!(linear, expected, "linear sweep from {from} to {to}");
        assert_eq!(
            live.obstacle_segment_blocked(from, to, radius, height),
            linear,
            "filtered sweep from {from} to {to}"
        );
    }
}
