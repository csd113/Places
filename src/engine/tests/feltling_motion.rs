//! Bounded native return-route regression: a snapped grid centre may overlap
//! an actor even when the actual moving body starts clear. No asset or bake.
use super::{DoorCollider, Vec3, leaf_path_blocked};

#[test]
fn planner_can_separate_from_a_rounded_grid_overlap() {
    let leaf = DoorCollider::from_pose([-0.4, 0.0, 0.0], [1.0, 0.0], 0.8, 0.8, 0.8);
    let overlap = Vec3::new(0.7, 0.0, 0.5);
    let clear = Vec3::new(0.9, 0.0, 0.5);
    assert!(
        !leaf_path_blocked(&leaf, overlap, clear, 0.4, 0.8),
        "a snapped query start can exit its existing overlap"
    );
    assert!(
        leaf_path_blocked(&leaf, clear, overlap, 0.4, 0.8),
        "clear starts cannot enter an occupied body"
    );
    assert!(
        leaf_path_blocked(
            &leaf,
            Vec3::new(-1.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            0.4,
            0.8
        ),
        "planning still refuses passage through a body"
    );
}
