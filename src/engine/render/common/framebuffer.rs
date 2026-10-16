//! The offscreen scene target's size policy.
//!
//! This is renderer-neutral: the offscreen target's *size* is a quality-level
//! decision derived from the drawable, not a GPU fact. The renderer owns the
//! framebuffer itself.
//!
//! High renders at the drawable's own resolution; Medium caps the scene scale
//! at one half of the drawable; Low renders no wider than the authored
//! 480-pixel reference width. No level ever upscales a smaller drawable.

use super::view::DrawableSize;
use crate::engine::quality::QualityLevel;

/// The factor the offscreen scene target renders at for one level.
///
/// * [`QualityLevel::High`] is always 1: the intended presentation, with no
///   rescaling at all.
/// * [`QualityLevel::Low`] is no wider than the authored reference width,
///   which is a large saving on a desktop window and exactly the drawable's
///   size on the reference device itself.
/// * [`QualityLevel::Medium`] is Low's factor raised to at least one half, so
///   a huge desktop drawable still gets a genuinely intermediate target and a
///   small one simply follows Low.
///
/// Every factor is capped at one, so a drawable that is already small is never
/// *upscaled*.
#[must_use]
pub fn scene_target_factor(level: QualityLevel, drawable_width: u32) -> f64 {
    let width = f64::from(drawable_width);
    let reference = f64::from(crate::engine::render::common::view::UI_REFERENCE_WIDTH);
    // A zero width can only come from an empty drawable, which
    // `scene_target_size` answers before reaching here; the division still
    // yields a capped factor rather than an error.
    let low = (reference / width).min(1.0);
    match level {
        QualityLevel::Low => low,
        QualityLevel::Medium => low.max(0.5),
        QualityLevel::High => 1.0,
    }
}

/// The size the offscreen scene target should render at for one level.
///
/// The factor comes from [`scene_target_factor`], so the target's aspect ratio
/// is the drawable's and the presentation cannot distort the image.
#[must_use]
pub fn scene_target_size(level: QualityLevel, drawable: DrawableSize) -> DrawableSize {
    if drawable.is_empty() {
        return drawable;
    }
    let factor = scene_target_factor(level, drawable.width);
    if factor >= 1.0_f64 {
        return drawable;
    }
    let width = scale_dimension(drawable.width, factor);
    let height = scale_dimension(drawable.height, factor);
    DrawableSize::new(width.max(1), height.max(1))
}

/// Scales one drawable dimension, rounding to nearest and never to zero.
fn scale_dimension(value: u32, factor: f64) -> u32 {
    let scaled = (f64::from(value) * factor).round();
    #[expect(
        clippy::as_conversions,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "The rounded nonnegative dimension is clamped within 1..=u32::MAX before conversion."
    )]
    // `value` is a `u32` and `factor` is in `(0, 1]`, so the product is in
    // `[0, u32::MAX]` and non-negative; the clamp only guards the fractional
    // rounding.
    let clamped = scaled.clamp(1.0, f64::from(u32::MAX)) as u32;
    clamped
}
