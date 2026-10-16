use super::*;
use crate::test_support::{assert_exact, assert_exact_array};

#[test]
fn a_default_material_is_not_emissive() {
    let emission = MaterialEmission::default();
    assert_eq!(emission, MaterialEmission::NONE);
    assert!(!emission.is_emissive());
    assert_exact_array(emission.effective_color(), [0.0, 0.0, 0.0]);
}

#[test]
fn colour_and_intensity_multiply_into_the_uploaded_value() {
    let emission = MaterialEmission::new([0.25, 0.5, 1.0], 2.0);
    assert!(emission.is_emissive());
    assert_exact_array(emission.effective_color(), [0.5, 1.0, 2.0]);
}

#[test]
fn a_zero_intensity_or_black_colour_is_not_emissive() {
    assert!(!MaterialEmission::new([1.0, 1.0, 1.0], 0.0).is_emissive());
    assert!(!MaterialEmission::new([0.0, 0.0, 0.0], 4.0).is_emissive());
}

#[test]
fn sanitize_clamps_and_never_produces_nan() {
    let emission = MaterialEmission::new([f32::NAN, 2.0, -1.0], f32::INFINITY).sanitized();
    assert_exact_array(emission.color, [0.0, 1.0, 0.0]);
    assert_exact(emission.intensity, MAX_EMISSION_INTENSITY);

    let negative_infinity = MaterialEmission::new([0.5, 0.5, 0.5], f32::NEG_INFINITY).sanitized();
    assert_exact(negative_infinity.intensity, 0.0);
}

#[test]
fn a_mask_survives_sanitizing_and_is_optional() {
    let masked = MaterialEmission::new([1.0, 1.0, 1.0], 1.0).with_mask(Some(3));
    assert_eq!(masked.sanitized().mask, Some(3));
    assert_eq!(MaterialEmission::new([1.0, 1.0, 1.0], 1.0).mask, None);
}

#[test]
fn emission_is_clamped_to_the_documented_maximum() {
    let emission = MaterialEmission::new([1.0, 1.0, 1.0], 1_000.0).sanitized();
    assert_exact(emission.intensity, MAX_EMISSION_INTENSITY);
    assert_exact_array(emission.effective_color(), [8.0, 8.0, 8.0]);
}
