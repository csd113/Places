//! Normal-direction transforms shared by GPU uploads, posing and diagnostics.

use glam::{Mat3, Mat4};

/// Returns an inverse transpose for normal directions, or `None` for a
/// singular/non-finite transform. Callers choose their existing fallback.
///
/// Ordinary transforms retain their exact previous inverse. A small determinant
/// alone does not mean singular: model units or a positive skin-weight total can
/// underflow it. Rescaling the linear part by one positive factor preserves
/// normal directions after normalization and keeps the fallback inverse
/// independent of those units. Translation does not affect a surface normal.
pub fn inverse_transpose(model: Mat4) -> Option<Mat4> {
    if !model.is_finite() {
        return None;
    }
    let determinant = model.determinant();
    if determinant.is_finite() && determinant.abs() > 1.0e-12 {
        let inverse = model.inverse().transpose();
        if inverse.is_finite() {
            return Some(inverse);
        }
    }
    let components = Mat3::from_mat4(model).to_cols_array();
    let magnitude = components
        .iter()
        .fold(0.0_f32, |largest, value| largest.max(value.abs()));
    if magnitude <= 0.0 {
        return None;
    }
    let linear = Mat3::from_cols_array(&components.map(|value| value / magnitude));
    if linear.determinant().abs() <= 0.0 {
        return None;
    }
    let inverse = linear.inverse().transpose();
    inverse.is_finite().then(|| Mat4::from_mat3(inverse))
}
