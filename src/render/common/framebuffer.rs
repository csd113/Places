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
use crate::quality::QualityLevel;

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
    let reference = f64::from(crate::render::common::view::UI_REFERENCE_WIDTH);
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
    if factor >= 1.0 {
        return drawable;
    }
    let width = scale_dimension(drawable.width, factor);
    let height = scale_dimension(drawable.height, factor);
    DrawableSize::new(width.max(1), height.max(1))
}

/// Scales one drawable dimension, rounding to nearest and never to zero.
fn scale_dimension(value: u32, factor: f64) -> u32 {
    let scaled = (f64::from(value) * factor).round();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // `value` is a `u32` and `factor` is in `(0, 1]`, so the product is in
    // `[0, u32::MAX]` and non-negative; the clamp only guards the fractional
    // rounding.
    let clamped = scaled.clamp(1.0, f64::from(u32::MAX)) as u32;
    clamped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn high_renders_at_the_drawable_resolution() {
        let drawable = DrawableSize::new(1920, 1080);
        assert_eq!(
            scene_target_size(QualityLevel::High, drawable),
            drawable,
            "High must never rescale the scene"
        );
    }

    #[test]
    fn medium_halves_a_large_drawable_and_keeps_the_aspect() {
        let drawable = DrawableSize::new(1920, 1080);
        let medium = scene_target_size(QualityLevel::Medium, drawable);
        assert_eq!(medium, DrawableSize::new(960, 540));
        let drawable_aspect = f64::from(drawable.width) / f64::from(drawable.height);
        let medium_aspect = f64::from(medium.width) / f64::from(medium.height);
        assert!(
            (drawable_aspect - medium_aspect).abs() < 1.0e-3,
            "the target must keep the drawable's aspect ratio"
        );
    }

    #[test]
    fn low_caps_the_scene_width_at_the_reference_and_keeps_the_aspect() {
        let drawable = DrawableSize::new(1920, 1080);
        let low = scene_target_size(QualityLevel::Low, drawable);
        assert_eq!(low.width, 480);
        assert_eq!(low.height, 270);
        let drawable_aspect = f64::from(drawable.width) / f64::from(drawable.height);
        let low_aspect = f64::from(low.width) / f64::from(low.height);
        assert!(
            (drawable_aspect - low_aspect).abs() < 1.0e-3,
            "the target must keep the drawable's aspect ratio"
        );
    }

    #[test]
    fn medium_is_between_low_and_the_drawable_on_a_large_drawable() {
        let drawable = DrawableSize::new(1920, 1080);
        let low = scene_target_size(QualityLevel::Low, drawable);
        let medium = scene_target_size(QualityLevel::Medium, drawable);
        assert!(low.width < medium.width);
        assert!(medium.width < drawable.width);
        assert!(medium.height < drawable.height);
    }

    #[test]
    fn medium_follows_low_when_low_is_already_above_half() {
        // A 800-pixel drawable leaves Low at factor 0.6, above Medium's floor:
        // the two levels must agree rather than let Medium upscale.
        let drawable = DrawableSize::new(800, 600);
        let low = scene_target_size(QualityLevel::Low, drawable);
        let medium = scene_target_size(QualityLevel::Medium, drawable);
        assert_eq!(low, DrawableSize::new(480, 360));
        assert_eq!(medium, low);
    }

    #[test]
    fn low_is_the_identity_on_the_reference_device() {
        let drawable = DrawableSize::new(480, 272);
        assert_eq!(scene_target_size(QualityLevel::Low, drawable), drawable);
        assert_eq!(scene_target_size(QualityLevel::Medium, drawable), drawable);
    }

    #[test]
    fn a_smaller_than_reference_drawable_is_never_upscaled() {
        let drawable = DrawableSize::new(320, 180);
        for level in QualityLevel::ALL {
            assert_eq!(
                scene_target_size(level, drawable),
                drawable,
                "{level:?} must not upscale a small drawable"
            );
        }
    }

    #[test]
    fn an_empty_drawable_stays_empty() {
        let empty = DrawableSize::new(0, 0);
        for level in QualityLevel::ALL {
            assert_eq!(scene_target_size(level, empty), empty);
        }
    }

    #[test]
    fn tiny_drawables_still_produce_a_drawable_target() {
        let drawable = DrawableSize::new(3, 2);
        let low = scene_target_size(QualityLevel::Low, drawable);
        assert!(low.width >= 1 && low.height >= 1);
    }
}
