//! Player labels and interaction hints projected through the scene camera.

use glam::Vec3;

use crate::engine::interact::clear_line_of_sight_indexed;
use crate::engine::render::{
    DrawableSize, RenderCamera, UI_REFERENCE_HEIGHT, UI_REFERENCE_WIDTH, Vertex,
};
use crate::game::Game;

/// Scale of floating world labels in the 480x272 reference space.
const LABEL_TEXT_SCALE: f32 = 1.0;

/// Appends the floating display names of every toggled-on instance and the
/// interaction hand for the currently aimed-at instance to a UI vertex list.
///
/// The caller passes the same camera and drawable the frame is rendered with,
/// so labels track the scene; the vertices use the existing 2D text pipeline
/// and therefore share the menus' font atlas, blending and viewport. Nothing
/// here allocates per vertex beyond the caller's list.
pub fn append_world_labels(
    vertices: &mut Vec<Vertex>,
    game: &Game,
    camera: &RenderCamera,
    drawable: DrawableSize,
) {
    append_world_labels_with_hint(vertices, game, camera, drawable, true, "E");
}

/// Appends world labels with a shared, rebound-key interaction hint when the
/// frame's selected item does not own that hint.
pub fn append_world_labels_with_hint(
    vertices: &mut Vec<Vertex>,
    game: &Game,
    camera: &RenderCamera,
    drawable: DrawableSize,
    show_hint: bool,
    key: &str,
) {
    let viewport = drawable.ui_viewport();
    if viewport.width <= 0_i32 || viewport.height <= 0_i32 {
        return;
    }
    let (view_projection, _) = camera.view_projection(drawable);
    let items = game.interactables().items();
    for (index, item) in items.iter().enumerate() {
        if !game.is_label_visible(index) {
            continue;
        }
        if !clear_line_of_sight_indexed(
            camera.position,
            item.anchor,
            item.own_box.as_ref(),
            item.door_index,
            game.collision_index(),
            game.walls(),
            game.door_colliders(),
        ) {
            continue;
        }
        if let Some((x, y)) = project_to_reference(view_projection, drawable, item.anchor) {
            draw_world_label(vertices, &item.display_name, x, y);
        }
    }
    if show_hint
        && let Some(target) = game.interaction_target()
        && items.get(target).is_some()
    {
        crate::game::ui::append_interaction_hint(vertices, key);
    }
}

/// Projected reference-space position of a world point, or `None` when it is
/// behind the camera or outside the UI viewport.
#[must_use]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "glam projection uses floating point components; finite and viewport checks precede pixel layout."
)]
pub(crate) fn project_to_reference(
    view_projection: glam::Mat4,
    drawable: DrawableSize,
    world: Vec3,
) -> Option<(f32, f32)> {
    // Projection and layout maths: bounded `f32` world coordinates through a
    // finite MVP, with behind-camera and NaN checks before any pixel maths.
    // The operators are the formula; the lint's overflow concern does not
    // apply to these bounded quantities.
    let clip = view_projection * world.extend(1.0);
    if clip.w <= f32::EPSILON {
        return None;
    }
    let ndc_x = clip.x / clip.w;
    let ndc_y = clip.y / clip.w;
    if !ndc_x.is_finite() || !ndc_y.is_finite() {
        return None;
    }
    let viewport = drawable.ui_viewport();
    let viewport_width = pixel_f32(u32::try_from(viewport.width).unwrap_or(0));
    let viewport_height = pixel_f32(u32::try_from(viewport.height).unwrap_or(0));
    if viewport_width <= 0.0 || viewport_height <= 0.0 {
        return None;
    }
    let pixel_x = ndc_x.mul_add(0.5, 0.5) * pixel_f32(drawable.width);
    // NDC +y is up; the reference space has a top-left origin, so the pixel row
    // is measured from the top before the viewport offset is removed.
    let pixel_from_top = ndc_y.mul_add(-0.5, 0.5) * pixel_f32(drawable.height);
    let x = (pixel_x - pixel_f32(u32::try_from(viewport.x).unwrap_or(0))) / viewport_width
        * pixel_f32(UI_REFERENCE_WIDTH);
    let y = (pixel_from_top - pixel_f32(u32::try_from(viewport.y).unwrap_or(0))) / viewport_height
        * pixel_f32(UI_REFERENCE_HEIGHT);
    if !(0.0..=pixel_f32(UI_REFERENCE_WIDTH)).contains(&x)
        || !(0.0..=pixel_f32(UI_REFERENCE_HEIGHT)).contains(&y)
    {
        return None;
    }
    Some((x, y))
}

/// One drawable or viewport dimension as `f32`.
///
/// Every real window dimension fits a `u16`; a larger value clamps rather than
/// producing an inexact conversion. Mirrors the viewport maths' own helper so
/// this module needs no render-internal import.
#[must_use]
fn pixel_f32(value: u32) -> f32 {
    f32::from(u16::try_from(value).unwrap_or(u16::MAX))
}

/// Draws one floating label centred at `(x, y)` with a readable backing plate.
fn draw_world_label(vertices: &mut Vec<Vertex>, text: &str, x: f32, y: f32) {
    // Reference-space layout arithmetic: fixed, bounded pixel offsets.
    let width = crate::game::ui::text_width(text, LABEL_TEXT_SCALE);
    let left = width.mul_add(-0.5, x);
    let top = y - 3.0;
    crate::game::ui::add_rect_rgba(
        vertices,
        left - 3.0,
        top - 2.0,
        left + width + 3.0,
        top + 11.0,
        [0.05, 0.05, 0.06, 0.62],
    );
    crate::game::ui::draw_text(
        vertices,
        text,
        left,
        top,
        LABEL_TEXT_SCALE,
        [0.96, 0.94, 0.78],
    );
}
