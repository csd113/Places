//! The offscreen scene target's size policy.
//!
//! This is renderer-neutral: the offscreen target's *size* is a quality-profile
//! decision derived from the drawable, not a GPU fact. The OpenGL backend owns
//! the framebuffer itself (`opengl::framebuffer`).
//!
//! `Full` renders at the drawable's own resolution; `Low` renders no wider
//! than the authored 480-pixel reference width, never upscaling a smaller
//! drawable.

use super::view::DrawableSize;
use crate::quality::QualityProfile;

/// The size the offscreen scene target should render at for one profile.
///
/// * [`QualityProfile::Full`] renders at the drawable's own resolution: the
///   intended presentation, with no rescaling at all.
/// * [`QualityProfile::Low`] renders no wider than the authored reference
///   width, which is a large saving on a desktop window and exactly the
///   drawable's size on the reference device itself.
///
/// Both scale by a single factor, so the target's aspect ratio is the drawable's
/// and the presentation cannot distort the image. A drawable that is already
/// small is never *upscaled*: the factor is capped at one.
#[must_use]
pub fn scene_target_size(profile: QualityProfile, drawable: DrawableSize) -> DrawableSize {
    if drawable.is_empty() {
        return drawable;
    }
    let factor = match profile {
        QualityProfile::Full => 1.0,
        QualityProfile::Low => {
            let reference = f64::from(crate::render::common::view::UI_REFERENCE_WIDTH);
            let width = f64::from(drawable.width);
            (reference / width).min(1.0)
        }
    };
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
    fn full_renders_at_the_drawable_resolution() {
        let drawable = DrawableSize::new(1920, 1080);
        assert_eq!(
            scene_target_size(QualityProfile::Full, drawable),
            drawable,
            "Full must never rescale the scene"
        );
    }

    #[test]
    fn low_caps_the_scene_width_at_the_reference_and_keeps_the_aspect() {
        let drawable = DrawableSize::new(1920, 1080);
        let low = scene_target_size(QualityProfile::Low, drawable);
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
    fn low_is_the_identity_on_the_reference_device() {
        let drawable = DrawableSize::new(480, 272);
        assert_eq!(scene_target_size(QualityProfile::Low, drawable), drawable);
    }

    #[test]
    fn a_smaller_than_reference_drawable_is_never_upscaled() {
        let drawable = DrawableSize::new(320, 180);
        let low = scene_target_size(QualityProfile::Low, drawable);
        assert_eq!(low, drawable);
    }

    #[test]
    fn an_empty_drawable_stays_empty() {
        let empty = DrawableSize::new(0, 0);
        assert_eq!(scene_target_size(QualityProfile::Full, empty), empty);
        assert_eq!(scene_target_size(QualityProfile::Low, empty), empty);
    }

    #[test]
    fn tiny_drawables_still_produce_a_drawable_target() {
        let drawable = DrawableSize::new(3, 2);
        let low = scene_target_size(QualityProfile::Low, drawable);
        assert!(low.width >= 1 && low.height >= 1);
    }
}
