//! Essential broad-phase regression: axis-aligned, rotated, grazing and inside
//! rays must match the original exhaustive exact leaf oracle. 96 tiny boxes,
//! 4,096 deterministic rays; no map/asset/GPU, under 1 MiB and milliseconds.
use super::{Blocker, Horizontal, OrientedBox, Plane};
use crate::engine::lighting::directional_index::{DirectionalIndex, DirectionalSolid};

#[test]
fn conservative_tree_matches_exhaustive_directional_occlusion() -> Result<(), String> {
    let props: Vec<_> = (0..96_u16)
        .map(|index| {
            let i = f32::from(index);
            OrientedBox::new(
                [
                    i.mul_add(0.3, -12.0),
                    i.rem_euclid(7.0),
                    (i * 1.7).rem_euclid(17.0),
                ],
                [0.15, 0.8, 0.4],
                i * 0.13,
            )
            .ok_or("invalid fixture")
        })
        .collect::<Result<_, _>>()?;
    let tree = DirectionalIndex::build(&props);
    let walls = (0..96_u16)
        .map(|index| {
            let x = f32::from(index) * 0.3;
            Blocker {
                min: [x, 0.0, 1.0],
                max: [x + 0.1, 2.0, 1.2],
            }
        })
        .collect::<Vec<_>>();
    let floors = (0..96_u16)
        .map(|index| {
            let x = f32::from(index) * 0.3;
            Horizontal::Floor(Plane {
                x0: x,
                x1: x + 0.3,
                z0: 0.0,
                z1: 20.0,
                y: 2.0,
            })
        })
        .chain([Horizontal::Slab(Blocker {
            min: [-4.0, 3.0, -4.0],
            max: [4.0, 3.2, 4.0],
        })])
        .collect::<Vec<_>>();
    let wall_tree = DirectionalIndex::build(&walls);
    let floor_tree = DirectionalIndex::build(&floors);
    for index in 0..4096_u16 {
        let i = f32::from(index);
        let origin = [
            (i * 0.17).rem_euclid(40.0) - 20.0,
            (i * 0.31).rem_euclid(12.0) - 2.0,
            (i * 0.29).rem_euclid(30.0) - 5.0,
        ];
        let direction = match index % 4 {
            0 => [0.0, 1.0, 0.0],
            1 => [1.0, 0.0, 0.0],
            2 => [0.0, 0.0, -1.0],
            _ => [i.sin(), i.cos(), 0.3],
        };
        assert_eq!(
            wall_tree.occludes(&walls, origin, direction),
            walls
                .iter()
                .any(|wall| wall.directional_hit(origin, direction)),
            "wall ray {index}"
        );
        assert_eq!(
            floor_tree.occludes(&floors, origin, direction),
            floors
                .iter()
                .any(|floor| floor.directional_hit(origin, direction)),
            "floor ray {index}"
        );
        assert_eq!(
            tree.occludes(&props, origin, direction),
            props
                .iter()
                .any(|prop| prop.directional_hit(origin, direction)),
            "ray {index}: {origin:?} {direction:?}"
        );
    }
    let large = [
        OrientedBox::new([-100_000.0, 0.0, 0.0], [100_000.0, 1.0, 1.0], 0.0)
            .ok_or("invalid large fixture")?,
    ];
    let origin = [0.003, 0.0, -5.0];
    let direction = [0.0, 0.0, 1.0];
    assert!(
        large
            .first()
            .ok_or("missing large box")?
            .directional_hit(origin, direction),
        "retain the original f32 subtraction at a large-box face"
    );
    assert!(
        DirectionalIndex::build(&large).occludes(&large, origin, direction),
        "broad phase must cover cancellation error using primitive extent, not just the rounded corner"
    );
    let empty: &[OrientedBox] = &[];
    assert!(!DirectionalIndex::build(empty).occludes(empty, [0.0; 3], [0.0, 1.0, 0.0]));
    Ok(())
}
