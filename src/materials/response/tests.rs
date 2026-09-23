//! Unit tests for the lightweight surface response contract.

use super::*;

/// True when every channel of two colours matches.
fn same_color(left: [f32; 3], right: [f32; 3]) -> bool {
    left.iter()
        .zip(right)
        .all(|(left, right)| (left - right).abs() < f32::EPSILON)
}

/// True when two scalars match.
fn same(left: f32, right: f32) -> bool {
    (left - right).abs() < f32::EPSILON
}

#[test]
fn default_response_is_inactive_and_matches_the_legacy_look() {
    let response = MaterialResponse::default();
    assert!(!response.is_active());
    assert!(!response.has_normal());
    assert!(!response.has_sheen());
    // A legacy material carries no sheen and no normal map, so the draw path
    // uploads the neutral state and adds nothing to the lit pixel.
    assert!(same_color(response.specular, [0.0; 3]));
    assert!(response.normal.is_none());
}

#[test]
fn a_normal_map_alone_activates_the_response_without_a_sheen() {
    let response = MaterialResponse::with_normal(Some(7), 0.5);
    assert!(response.is_active());
    assert!(response.has_normal());
    assert!(!response.has_sheen());
    assert_eq!(response.normal, Some(7));
    assert!(same(response.normal_strength, 0.5));
}

#[test]
fn a_sheen_alone_activates_the_response_without_a_normal_map() {
    let response = MaterialResponse::with_sheen(0.4, 0.2);
    assert!(response.is_active());
    assert!(!response.has_normal());
    assert!(response.has_sheen());
    assert!(same_color(response.specular, [0.4; 3]));
}

#[test]
fn sanitize_clamps_every_field_and_neutralises_nan() {
    let response = MaterialResponse {
        normal: None,
        normal_strength: f32::NAN,
        specular: [2.0, -1.0, f32::INFINITY],
        roughness: 4.0,
    }
    .sanitized();
    assert!(same(response.normal_strength, DEFAULT_NORMAL_STRENGTH));
    // A finite out-of-range channel clamps; a non-finite one becomes zero,
    // because an infinity must never reach a shader.
    assert!(same_color(response.specular, [MAX_SPECULAR, 0.0, 0.0]));
    assert!(same(response.roughness, DEFAULT_ROUGHNESS));
}

#[test]
fn alpha_mode_round_trips_through_its_names() {
    for mode in AlphaMode::ALL {
        assert_eq!(AlphaMode::parse(mode.name()), Some(mode));
        assert_eq!(AlphaMode::parse(&mode.name().to_uppercase()), Some(mode));
        assert_eq!(AlphaMode::parse(&format!("  {} ", mode.name())), Some(mode));
    }
    assert_eq!(AlphaMode::parse("translucent"), None);
    assert_eq!(AlphaMode::parse(""), None);
}

#[test]
fn alpha_classification_separates_the_three_passes() {
    assert!(!AlphaMode::Opaque.is_translucent());
    assert!(!AlphaMode::Opaque.is_cutout());
    assert!(AlphaMode::Cutout.is_cutout());
    assert!(!AlphaMode::Cutout.is_translucent());
    assert!(AlphaMode::Blend.is_translucent());
    assert!(!AlphaMode::Blend.is_cutout());
}

#[test]
fn opaque_alpha_is_the_legacy_state() {
    let alpha = MaterialAlpha::default();
    assert_eq!(alpha.mode, AlphaMode::Opaque);
    assert!(!alpha.is_translucent());
    assert!(!alpha.is_cutout());
    assert!(same(alpha.opacity, 1.0));
}

#[test]
fn a_zero_opacity_blend_material_is_not_drawn_translucent() {
    // Nothing to see: the translucent pass would submit invisible geometry.
    let alpha = MaterialAlpha::blend(0.0);
    assert!(!alpha.is_translucent());
    assert!(MaterialAlpha::blend(0.25).is_translucent());
}

#[test]
fn alpha_sanitize_clamps_and_neutralises_nan() {
    let alpha = MaterialAlpha {
        mode: AlphaMode::Cutout,
        opacity: 7.0,
        cutoff: f32::NAN,
    }
    .sanitized();
    assert_eq!(alpha.mode, AlphaMode::Cutout);
    assert!(same(alpha.opacity, 1.0));
    assert!(same(alpha.cutoff, DEFAULT_ALPHA_CUTOFF));
}
