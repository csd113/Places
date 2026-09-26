//! Desktop display configuration: window modes, the windowed resolution
//! choices, and the work-area fitting rule.
//!
//! Everything here is pure except [`DisplayStatus`], which is a snapshot of
//! what the windowing backend reports. Keeping the policy pure means the
//! resolution list and the fit rule are unit-testable without creating a
//! window, while `main` owns the actual SDL calls.
//!
//! ## Window size versus drawable size
//!
//! The values here are **logical window dimensions** (what the player sees and
//! drags). On a Retina/high-DPI display the GPU drawable is larger — see
//! [`crate::render::DrawableSize`] — and the renderer always uses the drawable
//! for viewports, framebuffers and projection. The two concepts are never
//! mixed.

use crate::settings::{DEFAULT_WINDOW_HEIGHT, DEFAULT_WINDOW_WIDTH, Settings, WindowMode};

/// Windowed client sizes offered in Display, smallest first.
///
/// A deliberately short list of the common 16:9 desktop modes. It is filtered
/// against the active display's work area (see [`resolution_choices`]) rather
/// than being an exhaustive mode database.
pub const COMMON_WINDOW_SIZES: [(u32, u32); 5] = [
    (1280, 720),
    (1600, 900),
    (1920, 1080),
    (2560, 1440),
    (3840, 2160),
];

/// A snapshot of the display configuration the window is actually running on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DisplayStatus {
    /// Windowed or fullscreen, as the backend currently reports it (not just
    /// what `settings.json` asks for).
    pub mode: WindowMode,
    /// Actual logical window size.
    pub window_size: (u32, u32),
    /// The active display's usable work area, in logical pixels. `(0, 0)` when
    /// the backend could not report it.
    pub usable_bounds: (u32, u32),
    /// The active display's own resolution, in logical pixels.
    pub desktop_size: (u32, u32),
}

impl Default for DisplayStatus {
    fn default() -> Self {
        Self {
            mode: WindowMode::default(),
            window_size: (DEFAULT_WINDOW_WIDTH, DEFAULT_WINDOW_HEIGHT),
            usable_bounds: (0, 0),
            desktop_size: (0, 0),
        }
    }
}

impl DisplayStatus {
    /// The windowed sizes the Display screen offers on this display.
    ///
    /// The common 16:9 modes that fit the work area, plus the current size if
    /// it is not one of them (so the selector can always show and return to
    /// whatever the window actually is). Never empty.
    #[must_use]
    pub fn resolution_choices(self, current: (u32, u32)) -> Vec<(u32, u32)> {
        let mut choices: Vec<(u32, u32)> = COMMON_WINDOW_SIZES
            .into_iter()
            .filter(|size| fits(*size, self.usable_bounds))
            .collect();
        if !choices.contains(&current) {
            choices.push(current);
        }
        choices.sort_unstable();
        choices.dedup();
        choices
    }
}

/// True when `size` fits inside `bounds`.
///
/// A zero bounds axis means "unknown" and matches everything, so a backend that
/// cannot report a work area never hides the standard modes.
#[must_use]
pub const fn fits(size: (u32, u32), bounds: (u32, u32)) -> bool {
    (bounds.0 == 0 || size.0 <= bounds.0) && (bounds.1 == 0 || size.1 <= bounds.1)
}

/// The windowed size a left/right input selects from `choices`.
///
/// Returns `None` when `choices` is empty (which [`DisplayStatus::resolution_choices`]
/// never produces); a current size that is not in the list selects the nearest
/// entry instead of being stuck.
#[must_use]
pub fn step_resolution(
    choices: &[(u32, u32)],
    current: (u32, u32),
    direction: i32,
) -> Option<(u32, u32)> {
    if choices.is_empty() {
        return None;
    }
    let index = choices
        .iter()
        .position(|size| *size == current)
        .unwrap_or_else(|| nearest_resolution_index(choices, current));
    let next = if direction < 0 {
        index
            .checked_sub(1)
            .unwrap_or_else(|| choices.len().saturating_sub(1))
    } else {
        let next = index.saturating_add(1);
        if next < choices.len() { next } else { 0 }
    };
    choices.get(next).copied()
}

/// Index of the choice closest to `current` by width, then height.
fn nearest_resolution_index(choices: &[(u32, u32)], current: (u32, u32)) -> usize {
    let mut best = 0usize;
    let mut best_distance = u64::MAX;
    for (index, size) in choices.iter().enumerate() {
        let width = u64::from(size.0.abs_diff(current.0));
        let height = u64::from(size.1.abs_diff(current.1));
        let distance = width
            .saturating_mul(width)
            .saturating_add(height.saturating_mul(height));
        if distance < best_distance {
            best_distance = distance;
            best = index;
        }
    }
    best
}

/// Reduces `size` until it fits inside `bounds`, preserving its aspect ratio.
///
/// A window that would not fit its display is created at a smaller size rather
/// than partly off-screen. The result is never zero-sized; an unknown (`0`)
/// bounds axis or a zero-sized input leaves the size alone.
#[must_use]
pub fn fit_window_to_bounds(size: (u32, u32), bounds: (u32, u32)) -> (u32, u32) {
    let (width, height) = size;
    if width == 0 || height == 0 {
        return (DEFAULT_WINDOW_WIDTH, DEFAULT_WINDOW_HEIGHT);
    }
    let (max_width, max_height) = bounds;
    if max_width == 0 || max_height == 0 {
        return (width, height);
    }
    if width <= max_width && height <= max_height {
        return (width, height);
    }
    let scale =
        (f64::from(max_width) / f64::from(width)).min(f64::from(max_height) / f64::from(height));
    let fitted_width = scale_edge(width, scale).min(max_width);
    let fitted_height = scale_edge(height, scale).min(max_height);
    (fitted_width.max(1), fitted_height.max(1))
}

/// Scales one window edge, flooring to a whole pixel and never to zero.
fn scale_edge(edge: u32, factor: f64) -> u32 {
    let scaled = (f64::from(edge) * factor).floor();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // `edge` is a `u32` and `factor` is in `(0, 1]`, so the product is in
    // `[0, u32::MAX]` and non-negative; the clamp only guards the fractional
    // rounding.
    let clamped = scaled.clamp(1.0, f64::from(u32::MAX)) as u32;
    clamped
}

/// The display string for one windowed size (`1920 x 1080`).
///
/// The bitmap font is ASCII, so a plain `x` is used rather than `×`.
#[must_use]
pub fn format_resolution(size: (u32, u32)) -> String {
    format!("{} x {}", size.0, size.1)
}

/// The player-facing value of the Resolution row for the current state.
///
/// In fullscreen the window follows the display, and the row says so with the
/// display's own resolution: the menu always describes the configuration
/// actually in force.
#[must_use]
pub fn resolution_label(settings: &Settings, status: DisplayStatus) -> String {
    match status.mode {
        WindowMode::Windowed => format_resolution(settings.window_size()),
        WindowMode::Fullscreen => {
            let desktop = status.desktop_size;
            if desktop.0 == 0 || desktop.1 == 0 {
                "Follows Display".to_string()
            } else {
                format!("{} (display)", format_resolution(desktop))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::float_cmp)]

    use super::*;

    #[test]
    fn the_standard_modes_fit_the_standard_desktop() {
        let choices = DisplayStatus {
            usable_bounds: (1920, 1040),
            ..DisplayStatus::default()
        }
        .resolution_choices((1920, 1080));
        assert!(choices.contains(&(1920, 1080)));
        assert!(choices.contains(&(1280, 720)));
        assert!(!choices.contains(&(3840, 2160)));
    }

    #[test]
    fn an_unknown_work_area_offers_every_common_mode() {
        let choices = DisplayStatus::default().resolution_choices((1920, 1080));
        for size in COMMON_WINDOW_SIZES {
            assert!(choices.contains(&size), "{size:?} missing");
        }
    }

    #[test]
    fn the_current_size_is_always_representable() {
        // A hand-resized window that is not a listed mode still appears, so the
        // selector can show and return to it.
        let choices = DisplayStatus {
            usable_bounds: (1920, 1040),
            ..DisplayStatus::default()
        }
        .resolution_choices((1366, 768));
        assert!(choices.contains(&(1366, 768)));
    }

    #[test]
    fn stepping_wraps_in_both_directions_and_starts_from_a_missing_size() {
        let choices = vec![(1280, 720), (1920, 1080), (2560, 1440)];
        assert_eq!(
            step_resolution(&choices, (1920, 1080), 1),
            Some((2560, 1440))
        );
        assert_eq!(
            step_resolution(&choices, (1920, 1080), -1),
            Some((1280, 720))
        );
        assert_eq!(
            step_resolution(&choices, (2560, 1440), 1),
            Some((1280, 720))
        );
        assert_eq!(
            step_resolution(&choices, (1280, 720), -1),
            Some((2560, 1440))
        );
        // A size that is not listed selects the nearest entry, then steps.
        assert_eq!(
            step_resolution(&choices, (1400, 800), 1),
            Some((1920, 1080))
        );
        assert_eq!(step_resolution(&[], (1920, 1080), 1), None);
    }

    #[test]
    fn fitting_keeps_the_aspect_and_never_leaves_the_work_area() {
        let fitted = fit_window_to_bounds((1920, 1080), (1512, 900));
        assert!(fitted.0 <= 1512 && fitted.1 <= 900);
        let source_aspect = 1920.0 / 1080.0;
        let fitted_aspect = f64::from(fitted.0) / f64::from(fitted.1);
        assert!(
            (source_aspect - fitted_aspect).abs() < 0.01,
            "{fitted:?} must keep 16:9"
        );
        // A size that already fits is untouched.
        assert_eq!(
            fit_window_to_bounds((1920, 1080), (2560, 1440)),
            (1920, 1080)
        );
        // Unknown bounds and zero sizes never produce a broken window.
        assert_eq!(fit_window_to_bounds((1920, 1080), (0, 0)), (1920, 1080));
        assert_eq!(
            fit_window_to_bounds((0, 0), (1920, 1080)),
            (DEFAULT_WINDOW_WIDTH, DEFAULT_WINDOW_HEIGHT)
        );
        let tiny = fit_window_to_bounds((1920, 1080), (320, 200));
        assert!(tiny.0 >= 1 && tiny.1 >= 1 && tiny.0 <= 320 && tiny.1 <= 200);
    }

    #[test]
    fn the_resolution_row_describes_the_mode_in_force() {
        let mut settings = Settings::default();
        assert_eq!(
            resolution_label(
                &settings,
                DisplayStatus {
                    mode: WindowMode::Windowed,
                    window_size: (480, 272),
                    ..DisplayStatus::default()
                }
            ),
            "480 x 272"
        );
        settings.set_window_size(2560, 1440);
        assert_eq!(
            resolution_label(
                &settings,
                DisplayStatus {
                    mode: WindowMode::Fullscreen,
                    desktop_size: (2560, 1440),
                    ..DisplayStatus::default()
                }
            ),
            "2560 x 1440 (display)"
        );
    }
}
