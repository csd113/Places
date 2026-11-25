//! File-backed half-heart HUD. Large authored maxima use an exact numeric readout.
use crate::engine::health::{HealthSettings, HealthState};
use crate::engine::render::Vertex;

pub fn append_player_health(
    vertices: &mut Vec<Vertex>,
    health: &HealthState,
    settings: &HealthSettings,
) {
    if !settings.shows_player_hud() {
        return;
    }
    if health.max_hp() > 40 {
        super::draw_text(
            vertices,
            &format!("HP {} / {}", health.current_hp(), health.max_hp()),
            12.0,
            240.0,
            1.0,
            [0.96, 0.94, 0.88],
        );
        return;
    }
    // Each half is one HP. Rendering the halves separately keeps odd HP exact.
    let hearts = health.max_hp().div_ceil(2);
    for heart in 0..hearts {
        let x = f32::from(heart % 10).mul_add(18.0, 20.0);
        let y = f32::from(heart / 10).mul_add(-18.0, 246.0);
        for side in 0..2_u16 {
            let threshold = heart.saturating_mul(2).saturating_add(side);
            if threshold >= health.max_hp() {
                continue;
            }
            let filled = health.current_hp() > threshold;
            let color = if filled {
                [0.96, 0.16, 0.22, 0.95]
            } else {
                [0.18, 0.08, 0.1, 0.85]
            };
            half_heart(vertices, x, y, side == 1, color);
        }
    }
}

/// Lit-only status shared by native play and the editor's test viewport.
#[must_use]
pub fn flashlight_status(settings: &HealthSettings, lit: bool) -> Option<&'static str> {
    (settings.shows_player_hud() && lit).then_some("FLASHLIGHT ON")
}

pub fn append_flashlight_status(vertices: &mut Vec<Vertex>, settings: &HealthSettings, lit: bool) {
    if let Some(text) = flashlight_status(settings, lit) {
        // Reserve the lower margin below the subtitle panel (ends at y = 244).
        super::draw_text(vertices, text, 364.0, 252.0, 1.0, [0.96, 0.94, 0.88]);
    }
}

fn half_heart(vertices: &mut Vec<Vertex>, x: f32, y: f32, right: bool, color: [f32; 4]) {
    let [left, top, right_uv, bottom] = crate::engine::font::health_heart_uv();
    let middle = left.midpoint(right_uv);
    let (x0, x1, u0, u1) = if right {
        (x, x + 8.0, middle, right_uv)
    } else {
        (x - 8.0, x, left, middle)
    };
    super::add_ui_quad(
        vertices,
        x0,
        y - 8.0,
        x1,
        y + 8.0,
        color,
        [u0, top, u1, bottom],
    );
}
