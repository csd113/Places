//! Half-space capture rejection must never remove a touching/crossing receiver.
//! Five tiny boxes cover both plane signs, scale and numerical guards; no GPU/map.
use super::retained_by_clip_plane;
use crate::engine::spatial::Aabb;

#[test]
fn capture_plane_rejects_only_wholly_discarded_bounds() {
    let below = Aabb {
        min: [-1.0, -2.0, -1.0],
        max: [1.0, -1.0, 1.0],
    };
    let touching = Aabb {
        max: [1.0, 0.0, 1.0],
        ..below
    };
    let crossing = Aabb {
        max: [1.0, 1.0, 1.0],
        ..below
    };
    let plane = [0.0, 1.0, 0.0, 0.0];
    assert!(
        !retained_by_clip_plane(&below, plane),
        "wholly clipped geometry cannot contribute"
    );
    assert!(
        retained_by_clip_plane(&touching, plane),
        "coplanar geometry remains available to rasterization"
    );
    assert!(
        retained_by_clip_plane(&crossing, plane),
        "fragment clipping still owns intersecting boxes"
    );
    assert!(
        retained_by_clip_plane(&below, [0.0, -1.0, 0.0, 0.0]),
        "the reversed plane retains its opposite side"
    );
    assert!(
        !retained_by_clip_plane(&below, [0.0, 7.0, 0.0, 0.0]),
        "plane scaling preserves rejection"
    );
    assert!(
        retained_by_clip_plane(&below, [0.0; 4]),
        "ordinary views are unchanged"
    );
    assert!(
        retained_by_clip_plane(&below, [f32::NAN, 1.0, 0.0, 0.0]),
        "invalid planes cannot prove rejection"
    );
    let distant = Aabb {
        min: [9999.0, -1.0, -1.0],
        max: [10000.0, 1.0, 1.0],
    };
    assert!(
        retained_by_clip_plane(&distant, [1.0, 0.0, 0.0, -10000.0]),
        "large-coordinate contact keeps its rounding margin"
    );
}
