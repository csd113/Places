//! Protect trailing rows/columns in odd numeric mips and exact even reduction.
//! Tiny numerical arrays only: no assets, GPU or map compilation.
use super::{RawImage, halve_image};

#[test]
fn numeric_mips_include_odd_edges_and_weight_fractional_coverage() {
    let columns = RawImage::new(
        3,
        2,
        [0, 0, 255, 0, 0, 255]
            .into_iter()
            .flat_map(|red| [red, 0, 0, 255])
            .collect(),
    );
    assert_eq!(halve_image(&columns).rgba, [85, 0, 0, 255]);
    let rows = RawImage::new(
        2,
        3,
        [0, 0, 0, 0, 255, 255]
            .into_iter()
            .flat_map(|red| [red, 0, 0, 255])
            .collect(),
    );
    assert_eq!(halve_image(&rows).rgba, [85, 0, 0, 255]);
    let fractional = RawImage::new(
        5,
        1,
        [0, 0, 100, 200, 200]
            .into_iter()
            .flat_map(|red| [red, 0, 0, 255])
            .collect(),
    );
    let reduced = halve_image(&fractional);
    assert_eq!((reduced.width, reduced.height), (2, 1));
    assert_eq!(reduced.rgba, [20, 0, 0, 255, 180, 0, 0, 255]);
    assert_eq!(halve_image(&reduced).rgba, [100, 0, 0, 255]);
    let even = RawImage::new(
        2,
        2,
        [0, 1, 2, 3]
            .into_iter()
            .flat_map(|red| [red, 0, 0, 255])
            .collect(),
    );
    assert_eq!(halve_image(&even).rgba, [2, 0, 0, 255]);
}
