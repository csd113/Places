//! Protects the menu backdrop's formerly clipped screen edge and resize cache
//! without a window, assets or baking. One small geometry fixture also verifies
//! unchanged content and pointer alignment through the real renderer projection.
use super::ui_view_projection;
use crate::engine::render::DrawableSize;
use crate::game::AppState;
use crate::game::display::{COMMON_WINDOW_SIZES, DisplayStatus};
use crate::game::settings::{Settings, WindowMode};
use crate::game::ui::{UiGeometryCache, UiState, build_ui_geometry, menu_layout};
use glam::Vec3;

fn close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 0.005, "{actual} != {expected}");
}

#[test]
fn menu_backdrop_reaches_drawable_edges_without_moving_content_or_hits() {
    let ui = UiState::new();
    let settings = Settings::default();
    let mut display = DisplayStatus::default();
    let reference = build_ui_geometry(
        AppState::MainMenu,
        &ui,
        &settings,
        &display,
        "test",
        DrawableSize::new(480, 272),
    );
    let mut cache = UiGeometryCache::new();
    for mode in [WindowMode::Windowed, WindowMode::Fullscreen] {
        display.mode = mode;
        for (width, height) in COMMON_WINDOW_SIZES.into_iter().chain([
            (480, 272),
            (3440, 1440),
            (5120, 1440),
            (720, 1280),
            (1201, 801),
            (1921, 1081),
            (2880, 1800),
        ]) {
            let drawable = DrawableSize::new(width, height);
            let viewport = drawable.ui_viewport();
            let projection = ui_view_projection(drawable);
            let width_f = f32::from(u16::try_from(width).unwrap_or(0));
            let height_f = f32::from(u16::try_from(height).unwrap_or(0));
            let pixel = |point: [f32; 3]| {
                let ndc = projection.project_point3(Vec3::from_array(point));
                [
                    ndc.x.midpoint(1.0) * width_f,
                    (1.0 - ndc.y) * 0.5 * height_f,
                ]
            };
            let vertices = cache.get(
                AppState::MainMenu,
                &ui,
                &settings,
                &display,
                "test",
                drawable,
            );
            let backdrop_count: usize = if viewport.x > 0 { 12 } else { 6 };
            assert_eq!(
                vertices.get(backdrop_count..),
                reference.get(6..),
                "title, selections, colors and other content stay identical"
            );
            let backdrop = vertices.iter().take(backdrop_count);
            let left = backdrop
                .clone()
                .map(|v| pixel(v.pos)[0])
                .fold(f32::INFINITY, f32::min);
            let top = backdrop
                .clone()
                .map(|v| pixel(v.pos)[1])
                .fold(f32::INFINITY, f32::min);
            let bottom = backdrop
                .map(|v| pixel(v.pos)[1])
                .fold(f32::NEG_INFINITY, f32::max);
            close(left, 0.0);
            close(top, 0.0);
            close(bottom, height_f);
            assert!(
                vertices
                    .iter()
                    .take(backdrop_count)
                    .filter(|v| (pixel(v.pos)[0] - left).abs() < 0.005)
                    .all(|v| (v.color[3] - 0.74).abs() < f32::EPSILON),
                "left edge retains its opacity"
            );
            let control = [32.0, 84.0];
            let drawn = pixel([control[0], control[1], 0.0]);
            for dpi in [1_u32, 2] {
                let window = (width / dpi, height / dpi);
                let logical = [
                    drawn[0] * f32::from(u16::try_from(window.0).unwrap_or(0)) / width_f,
                    drawn[1] * f32::from(u16::try_from(window.1).unwrap_or(0)) / height_f,
                ];
                let point = drawable
                    .window_to_ui(window, logical)
                    .unwrap_or([f32::NAN; 2]);
                close(point[0], control[0]);
                close(point[1], control[1]);
                assert_eq!(
                    menu_layout(AppState::MainMenu, &ui, &settings, &display)
                        .hit_test(point)
                        .map(|hit| hit.index),
                    Some(0),
                    "the rendered Explore row retains its hit target"
                );
            }
        }
    }
    assert!(
        ui_view_projection(DrawableSize::new(0, 0)).is_finite(),
        "minimized projection stays finite"
    );
}
