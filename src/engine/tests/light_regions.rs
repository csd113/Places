//! Protects conservative visibility between receiver lattice rays. These pure
//! geometry regressions cover a thin blocker, sloping finite/directional bounds
//! and fail-closed invalid inputs. They allocate no assets, maps or GPU state;
//! each case executes at most fifteen finite or six directional axis tests.

use super::{Aabb, directional_light_region_overlaps, finite_light_region_overlaps};

#[test]
fn thin_blocker_between_receiver_columns_intersects_both_light_regions() {
    let receivers = Aabb {
        min: [0.0, -0.988, -0.2],
        max: [0.4, -0.988, 0.2],
    };
    let blocker = Aabb {
        min: [0.17, 0.5, -0.05],
        max: [0.19, 0.55, 0.05],
    };
    assert!(
        finite_light_region_overlaps(Aabb::from_point([0.18, 2.0, 0.0]), receivers, blocker),
        "finite-source support must include a thin blocker between lattice columns"
    );
    assert!(
        directional_light_region_overlaps([0.0, 1.0, 0.0], receivers, blocker),
        "infinite sunlight support must include a thin blocker between lattice columns"
    );
    let outside = Aabb {
        min: [0.5, 0.5, -0.05],
        max: [0.6, 0.55, 0.05],
    };
    assert!(
        !finite_light_region_overlaps(Aabb::from_point([0.18, 2.0, 0.0]), receivers, outside)
            && !directional_light_region_overlaps([0.0, 1.0, 0.0], receivers, outside),
        "a genuinely separated blocker must leave both light regions clear"
    );
}

#[test]
fn sloping_light_regions_reject_boxes_inside_only_their_coarse_world_bounds() {
    let receivers = Aabb {
        min: [-0.2, 0.0, -0.2],
        max: [0.2, 0.0, 0.2],
    };
    let emitter = Aabb {
        min: [3.8, 4.0, -0.2],
        max: [4.2, 4.0, 0.2],
    };
    let outside = Aabb {
        min: [0.0, 3.0, -0.1],
        max: [0.1, 3.1, 0.1],
    };
    assert!(
        !finite_light_region_overlaps(emitter, receivers, outside),
        "finite emitter hull must reject space outside its sloping faces"
    );
    assert!(
        !directional_light_region_overlaps([1.0, 1.0, 0.0], receivers, outside),
        "infinite prism must reject space outside its sloping faces"
    );
    let distant = Aabb {
        min: [99.9, 99.9, -0.1],
        max: [100.1, 100.1, 0.1],
    };
    assert!(
        directional_light_region_overlaps([1.0, 1.0, 0.0], receivers, distant),
        "directional visibility has no arbitrary far-distance cutoff"
    );
    assert!(
        !finite_light_region_overlaps(emitter, receivers, distant),
        "finite light support stops at its actual source extent"
    );
}

#[test]
fn light_regions_keep_contact_and_invalid_inputs_conservatively_blocked() {
    let point = Aabb::from_point([0.0; 3]);
    assert!(
        finite_light_region_overlaps(point, point, point)
            && directional_light_region_overlaps([0.0, 1.0, 0.0], point, point),
        "boundary contact remains an overlap for the caller to resolve"
    );
    let invalid = Aabb {
        min: [f32::NAN, 0.0, 0.0],
        max: [1.0; 3],
    };
    assert!(
        finite_light_region_overlaps(invalid, point, point)
            && finite_light_region_overlaps(point, Aabb::EMPTY, point)
            && directional_light_region_overlaps([0.0, 1.0, 0.0], point, invalid)
            && directional_light_region_overlaps([0.0; 3], point, point)
            && directional_light_region_overlaps([f32::INFINITY, 1.0, 0.0], point, point),
        "nonfinite, inverted and zero-direction queries fail closed"
    );
}
