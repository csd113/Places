//! Full measured impulse checks protect channel handedness and front/back and
//! elevation spectral cues; no subjective localization claim follows from them.
use super::*;
#[test]
fn measured_hrtf_has_mirrored_ears_and_distinct_elevation_and_rear_spectra() {
    let right = kernel(Vec3::X);
    let left = kernel(Vec3::NEG_X);
    let front = kernel(Vec3::Z);
    let back = kernel(Vec3::NEG_Z);
    let up = kernel(Vec3::Y);
    let energy = |samples: &Kernel| {
        samples
            .iter()
            .map(|f| f.left.mul_add(f.left, f.right * f.right))
            .sum::<f32>()
    };
    assert!(
        energy(&right) > 0.01 && energy(&front) > 0.01,
        "measured response must contain finite audible energy"
    );
    for (a, b) in right.iter().zip(&left) {
        assert!(
            (a.left - b.right).abs() < 1.0e-6 && (a.right - b.left).abs() < 1.0e-6,
            "mirroring must exchange the physical ears"
        );
    }
    let difference = |a: &Kernel, b: &Kernel| {
        a.iter()
            .zip(b)
            .map(|(x, y)| (x.left - y.left).abs() + (x.right - y.right).abs())
            .sum::<f32>()
    };
    assert!(
        difference(&front, &back) > 0.1 && difference(&front, &up) > 0.1,
        "headphone mode must preserve measured front/back and elevation filters beyond panning"
    );
    assert!(
        front
            .iter()
            .chain(&back)
            .chain(&up)
            .all(|f| f.left.is_finite() && f.right.is_finite()),
        "interpolation must remain finite at the poles"
    );
}
