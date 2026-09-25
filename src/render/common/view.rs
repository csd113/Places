//! Window, drawable, viewport and field-of-view maths.
//!
//! This is renderer-neutral: a `DrawableSize` is a windowing fact, the UI
//! viewport is aspect-fit policy, and the field of view for a non-reference
//! aspect is pure trigonometry. Nothing here depends on a GPU API, so a second
//! renderer backend reuses it unchanged.

/// The resolution 2D UI geometry is authored in.
///
/// This is a *reference space*, not a window size: the UI is scaled to the
/// drawable by [`DrawableSize::ui_viewport`], and the window the game opens at
/// is [`crate::settings::DEFAULT_WINDOW_WIDTH`] x
/// [`crate::settings::DEFAULT_WINDOW_HEIGHT`]. The Low quality profile also
/// caps its internal scene width at this reference, which is the one runtime
/// place it is used outside the UI.
pub const UI_REFERENCE_WIDTH: u32 = 480;
pub const UI_REFERENCE_HEIGHT: u32 = 272;

/// Physical size (in pixels) of the current drawable/framebuffer.
///
/// This is deliberately distinct from the window's logical size: on `HiDPI`
/// displays such as macOS Retina the drawable is larger than the window size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DrawableSize {
    pub width: u32,
    pub height: u32,
}

impl DrawableSize {
    #[must_use]
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    /// True when the surface cannot be rendered to (minimized/hidden windows).
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// Aspect ratio derived from the real framebuffer, safe against zero height.
    #[must_use]
    pub fn aspect_ratio(self) -> f32 {
        if self.height == 0 {
            1.0
        } else {
            dimension_f32(self.width) / dimension_f32(self.height)
        }
    }

    /// Pixel size of the integer-scaled UI region that fits this drawable while
    /// preserving the 480x272 reference aspect ratio, plus its bottom-left origin.
    #[must_use]
    pub fn ui_viewport(self) -> UiViewport {
        if self.is_empty() {
            return UiViewport {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
                scale: 1.0,
            };
        }

        let scale = (dimension_f32(self.width) / dimension_f32(UI_REFERENCE_WIDTH))
            .min(dimension_f32(self.height) / dimension_f32(UI_REFERENCE_HEIGHT))
            .max(0.0);
        let drawable_width = i32::try_from(self.width).unwrap_or(i32::MAX);
        let drawable_height = i32::try_from(self.height).unwrap_or(i32::MAX);
        let width =
            round_to_i32(dimension_f32(UI_REFERENCE_WIDTH) * scale).clamp(1, drawable_width);
        let height =
            round_to_i32(dimension_f32(UI_REFERENCE_HEIGHT) * scale).clamp(1, drawable_height);

        UiViewport {
            x: drawable_width.saturating_sub(width) / 2,
            y: drawable_height.saturating_sub(height) / 2,
            width,
            height,
            scale,
        }
    }
}

/// A drawable or reference dimension as `f32`.
///
/// Every dimension a windowing system reports fits a `u16` (65 535 px is far
/// past any real drawable), so the `u16` round-trip is exact; a value beyond
/// that bound is clamped rather than rounded, which keeps the viewport maths
/// inside a range it can represent.
pub fn dimension_f32(value: u32) -> f32 {
    f32::from(u16::try_from(value).unwrap_or(u16::MAX))
}

/// Rounds a viewport dimension to the nearest integer.
///
/// The value is `reference × scale`, where both dimensions come from
/// [`dimension_f32`] (at most 65 535) and the scale is their ratio, so the
/// result is a whole number in `0..=65 535`: the saturating `as` cast is exact
/// and the caller clamps it to the drawable anyway.
#[allow(clippy::cast_possible_truncation)]
const fn round_to_i32(value: f32) -> i32 {
    value.round() as i32
}

/// Placement of the 480x272 UI reference space inside the physical drawable.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiViewport {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub scale: f32,
}

/// Aspect ratio of the authored reference resolution (480x272).
#[must_use]
pub fn reference_aspect_ratio() -> f32 {
    dimension_f32(UI_REFERENCE_WIDTH) / dimension_f32(UI_REFERENCE_HEIGHT)
}

/// Maps the configured (baseline) vertical field of view onto a drawable with
/// the given aspect ratio.
///
/// * Wider than the authored baseline: the vertical FOV is unchanged, so the
///   horizontal view expands naturally ("Hor+").
/// * Narrower/taller than the baseline: the horizontal FOV is preserved instead
///   so the level is not cropped left/right; only the vertical FOV grows.
///
/// At the baseline aspect this is the identity, so the reference view is unchanged.
#[must_use]
pub fn vertical_fov_for_aspect(configured_vertical_fov_degrees: f32, aspect: f32) -> f32 {
    // Guards against a near-singular projection on very tall/portrait windows.
    const MAX_VERTICAL_FOV_DEGREES: f32 = 150.0;

    let reference = reference_aspect_ratio();
    if !aspect.is_finite() || aspect <= 0.0 || aspect >= reference {
        return configured_vertical_fov_degrees;
    }

    let half_vertical_tan = (configured_vertical_fov_degrees.to_radians() * 0.5).tan();
    let half_horizontal_tan = half_vertical_tan * reference;
    let adjusted = 2.0 * (half_horizontal_tan / aspect).atan();
    adjusted
        .to_degrees()
        .clamp(configured_vertical_fov_degrees, MAX_VERTICAL_FOV_DEGREES)
}

/// Largest number of static probes the renderer bakes for one level.
pub const MAX_REFLECTION_PROBES: usize = 2;

/// Divisor from the scene target's size to the emissive/bloom target's.
///
/// Bloom is a low-frequency glow: rendering the emissive term at a quarter of
/// the scene's edge (a sixteenth of its pixels) and letting bilinear filtering
/// stretch it back up is what makes it soft, and it is why the pass is cheap
/// enough to keep off the Low profile's critical path entirely.
pub const BLOOM_SCALE_DIVISOR: u32 = 4;

/// Divisor from the scene target's size to the planar reflection target's.
///
/// A planar reflection is a real second view of the level, so it is drawn once
/// per active plane per frame and at half resolution; the blur a rough surface
/// reads hides the difference, and the profile can switch it off completely.
pub const PLANAR_REFLECTION_SCALE_DIVISOR: u32 = 2;
