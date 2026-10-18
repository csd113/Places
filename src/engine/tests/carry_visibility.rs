//! One wall and tiny mathematical bounds protect the disappearance invariant.
//! The torso is hidden while an arm or carried prop extends around the edge;
//! following to the same side must also refuse hiding. No mesh assets/GPU/bake.

use glam::Vec3;

use crate::engine::{carry_visibility::assembly_hidden, collision::WallAabb, spatial::Aabb};

#[test]
fn hidden_torso_cannot_retire_an_exposed_limb_or_carried_prop() {
    let eye = Vec3::new(0.0, 1.6, 0.0);
    let wall = WallAabb::with_y(-0.5, 0.0, -3.0, 1.0, 3.0, 0.2);
    let body = Aabb {
        min: [-0.3, 0.0, -5.2],
        max: [0.3, 1.8, -4.8],
    };
    assert!(
        assembly_hidden(eye, body, &[wall]),
        "one opaque face fully hides the model"
    );
    let arm = body.expanded([1.2, 1.1, -4.8]);
    assert!(
        !assembly_hidden(eye, arm, &[wall]),
        "the body centre is hidden but the arm remains visible"
    );
    let prop = body.expanded([-1.2, 1.0, -4.8]);
    assert!(
        !assembly_hidden(eye, prop, &[wall]),
        "the hidden model must not retire a visible carried prop"
    );
    assert!(
        !assembly_hidden(Vec3::new(0.0, 1.6, -4.0), body, &[wall]),
        "following the actor around the corner prevents retirement"
    );
    assert!(
        !assembly_hidden(eye, Aabb::EMPTY, &[wall]),
        "missing geometry is not evidence of being hidden"
    );
}
