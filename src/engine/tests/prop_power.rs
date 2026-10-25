//! Protects power-off dominance over animation and independent owner/material
//! identity. Two tiny CPU fixtures; no GPU, assets or baking, under a millisecond.
use super::*;

#[test]
fn powered_emission_cannot_restart_while_off() {
    let mut control = PropEmissionControl {
        material: 0,
        enabled: true,
        scale: 0.16,
        animation: Some(
            crate::engine::render::common::animation::EmissionAnimation::new(
                crate::engine::render::common::animation::AnimationEffect::Flicker,
            ),
        ),
        applied: 1.0,
    };
    for seconds in [0.0, 0.25, 3.0, 17.0] {
        control.enabled = true;
        let on = control.desired_scale(seconds);
        assert!(on > 0.0 && on <= 0.16);
        control.enabled = false;
        assert_eq!(control.desired_scale(seconds).to_bits(), 0.0_f32.to_bits());
        control.enabled = true;
        assert_eq!(control.desired_scale(seconds).to_bits(), on.to_bits());
    }
}

#[test]
fn power_identity_separates_luminous_owners_and_keeps_hardware_shared() {
    use crate::engine::materials::{MaterialAlpha, MaterialEmission, MaterialResponse};
    let mut part = crate::engine::render::PropSubmeshBatch {
        response: MaterialResponse::NONE,
        texture: Some(0),
        emission: MaterialEmission::NONE,
        alpha: MaterialAlpha::OPAQUE,
        first_index: 0,
        index_count: 3,
    };
    let identity = |primitive: &crate::engine::render::PropSubmeshBatch, owner| {
        PropMaterialIdentity::new(primitive, &[2], 1, owner)
    };
    assert_eq!(identity(&part, Some(1)), identity(&part, Some(2)));
    part.emission = MaterialEmission::new([1.0; 3], 0.65);
    assert_ne!(identity(&part, Some(1)), identity(&part, Some(2)));
    assert_ne!(identity(&part, Some(1)), identity(&part, None));
    assert_eq!(identity(&part, Some(1)).texture, 2);
}
