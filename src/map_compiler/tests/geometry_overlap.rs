//! Three-triangle fixtures protect conservative candidates and the work bound.
//! No source map, asset load, preparation or bake is needed; cost is negligible.
use super::{SurfaceKey, SurfaceKind, Tri, coplanar_candidates};

fn triangle(points: [[f32; 3]; 3]) -> Tri {
    Tri {
        key: SurfaceKey::new(SurfaceKind::Wall, 0),
        points,
        normal: [0.0, 0.0, 1.0],
        centroid: points[0],
        area: 0.5,
    }
}

#[test]
fn spatial_candidates_cross_old_plane_bins_and_bound_every_active_pair() {
    let distant = triangle([[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]);
    let first = triangle([[1000.0, 0.0, 0.0], [1001.0, 0.0, 0.0], [1000.0, 1.0, 0.0]]);
    let second = triangle([
        [1000.0, 0.0, 0.0],
        [1001.0, 0.0, 0.000_01],
        [1000.0, 1.0, 0.0],
    ]);
    let (distant_pairs, distant_complete) = coplanar_candidates(&[distant, first, second], 1);
    assert!(
        distant_complete,
        "an unrelated remote face must not alter candidates"
    );
    assert_eq!(
        distant_pairs,
        [(1, 2)],
        "near planes survive the former quantization boundary"
    );

    let (partial, bounded_complete) = coplanar_candidates(&[first, first, first], 1);
    assert!(
        !bounded_complete,
        "a dense active sheet must stop at the work budget"
    );
    assert_eq!(
        partial.len(),
        1,
        "partial candidates cannot exceed the work limit"
    );

    let separated = triangle([[2000.0, 0.0, 0.0], [2001.0, 0.0, 0.0], [2000.0, 1.0, 0.0]]);
    let (separated_pairs, separated_complete) =
        coplanar_candidates(&[distant, first, separated], 0);
    assert!(
        separated_complete && separated_pairs.is_empty(),
        "disjoint sorted intervals stop without scanning a pair list"
    );
}
