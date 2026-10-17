//! A submerged model's own floor must receive caustics without treating an
//! overhead body or the empty corner of a rotated bound as support. Three tiny
//! boxes exercise the real compiled-grid queries in microseconds, without
//! assets, transport, a map bake or a GPU.

use super::{Occluders, OrientedBox, SegmentGrid, Visibility};
use crate::engine::spatial::Aabb;

#[test]
fn submerged_static_support_raises_receiver_while_overhead_body_still_blocks() -> Result<(), String>
{
    let props = vec![
        OrientedBox::new([0.0, -1.5, 0.0], [2.0, 0.1, 2.0], 0.0).ok_or("invalid support box")?,
        OrientedBox::new([0.0, 0.5, 0.0], [0.25, 0.1, 0.25], 0.0).ok_or("invalid overhead box")?,
        OrientedBox::new(
            [5.0, -1.0, 0.0],
            [1.0, 0.1, 0.1],
            std::f32::consts::FRAC_PI_4,
        )
        .ok_or("invalid rotated box")?,
    ];
    let visibility = Visibility {
        occluders: Occluders {
            prop_grid: SegmentGrid::build(&props),
            props,
            ..Occluders::default()
        },
        ..Visibility::default()
    };
    let adopted = visibility.caustics_receiver_floor(0.0, 0.0, -2.0, 0.0);
    assert_eq!(
        adopted.to_bits(),
        (-1.4_f32).to_bits(),
        "only the submerged top raises the receiver floor"
    );
    assert_eq!(
        visibility
            .caustics_receiver_floor(0.0, 0.0, -1.0, 0.0)
            .to_bits(),
        (-1.0_f32).to_bits(),
        "receiver adoption never lowers an existing higher floor"
    );
    assert_eq!(
        visibility
            .caustics_receiver_floor(5.7, -0.7, -2.0, 0.0)
            .to_bits(),
        (-0.9_f32).to_bits(),
        "a rotated support uses its actual local footprint"
    );
    assert_eq!(
        visibility
            .caustics_receiver_floor(5.7, 0.7, -2.0, 0.0)
            .to_bits(),
        (-2.0_f32).to_bits(),
        "the empty corner of a rotated world bound is not a receiver floor"
    );
    let region = |x: f32| Aabb {
        min: [x - 0.01, adopted + 0.012, -0.01],
        max: [x + 0.01, adopted + 0.02, 0.01],
    };
    assert!(
        !visibility.direction_region_visible(region(0.0), [0.0, 1.0, 0.0]),
        "the above-water body retains its shadow after support adoption"
    );
    assert!(
        visibility.direction_region_visible(region(1.0), [0.0, 1.0, 0.0]),
        "an airside region above the model floor does not shadow itself"
    );
    Ok(())
}
