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

/// Developer-only trigger IDs, already bounded by the live trigger query.
pub fn append_trigger_labels(
    vertices: &mut Vec<Vertex>,
    labels: &[crate::engine::entities::developer::TriggerLabel],
    camera: &RenderCamera,
    drawable: DrawableSize,
) {
    const EDGE: f32 = 2.0;
    const SIDE_PADDING: f32 = 3.0;
    const PLATE_HEIGHT: f32 = 13.0;
    const GAP: f32 = 2.0;

    let viewport = drawable.ui_viewport();
    if viewport.width <= 0_i32 || viewport.height <= 0_i32 {
        return;
    }
    let (projection, _) = camera.view_projection(drawable);
    let screen_width = pixel_f32(UI_REFERENCE_WIDTH);
    let screen_height = pixel_f32(UI_REFERENCE_HEIGHT);
    let max_text_width = (-2.0_f32).mul_add(EDGE + SIDE_PADDING, screen_width);
    let mut occupied: Vec<(f32, f32, f32, f32)> =
        Vec::with_capacity(crate::engine::entities::developer::MAX_TRIGGER_LABELS);
    for label in labels
        .iter()
        .take(crate::engine::entities::developer::MAX_TRIGGER_LABELS)
    {
        // Show only addressable command IDs; this also bounds glyph work and
        // keeps the font's u16 width measurement exact for every shown label.
        if !crate::game::console::command::is_valid_command_id(&label.id) {
            continue;
        }
        let Some((x, y)) = project_to_reference(projection, drawable, label.position) else {
            continue;
        };
        let text = format!("trigger: {}", label.id);
        let natural_width = crate::game::ui::text_width(&text, LABEL_TEXT_SCALE);
        // Keep the exact ID, fitting unusually long names without truncation.
        let scale = LABEL_TEXT_SCALE * (max_text_width / natural_width).min(1.0);
        let width = crate::game::ui::text_width(&text, scale).min(max_text_width);
        let plate_width = 2.0_f32.mul_add(SIDE_PADDING, width);
        let left =
            (width.mul_add(-0.5, x) - SIDE_PADDING).clamp(EDGE, screen_width - EDGE - plate_width);
        let right = left + plate_width;
        let preferred_top = (y - 5.0).clamp(EDGE, screen_height - EDGE - PLATE_HEIGHT);
        let mut best: Option<(f32, f32)> = None;
        // Eighteen rows fit the viewport; at most sixteen labels compete.
        // Horizontal separation permits unrelated labels to share a row.
        for row in 0_u16..18 {
            let candidate_top = f32::from(row).mul_add(PLATE_HEIGHT + GAP, EDGE);
            if candidate_top + PLATE_HEIGHT > screen_height - EDGE {
                break;
            }
            let bottom = candidate_top + PLATE_HEIGHT;
            let overlaps = occupied.iter().any(|(x0, y0, x1, y1)| {
                left < *x1 + GAP
                    && right + GAP > *x0
                    && candidate_top < *y1 + GAP
                    && bottom + GAP > *y0
            });
            let distance = (candidate_top - preferred_top).abs();
            if !overlaps && best.is_none_or(|(nearest, _)| distance < nearest) {
                best = Some((distance, candidate_top));
            }
        }
        let Some((_, top)) = best else {
            continue;
        };
        let bottom = top + PLATE_HEIGHT;
        occupied.push((left, top, right, bottom));
        crate::game::ui::add_rect_rgba(
            vertices,
            left,
            top,
            right,
            bottom,
            [0.05, 0.05, 0.06, 0.62],
        );
        crate::game::ui::draw_text(
            vertices,
            &text,
            left + SIDE_PADDING,
            top + 2.0,
            scale,
            [0.96, 0.94, 0.78],
        );
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

/// Explicit authoring overlay, sharing the normal projection and font assets.
pub(crate) fn append_authoring_marker(
    vertices: &mut Vec<Vertex>,
    text: &str,
    position: glam::Vec3,
    camera: &RenderCamera,
    drawable: DrawableSize,
) {
    let (projection, _) = camera.view_projection(drawable);
    if let Some((x, y)) = project_to_reference(projection, drawable, position) {
        draw_world_label(vertices, text, x, y);
    }
}

/// Wireframe of the exact receiver band, with no world or texture mutation.
pub(crate) fn append_authoring_box(
    vertices: &mut Vec<Vertex>,
    min: [f32; 3],
    max: [f32; 3],
    accepted: bool,
    camera: &RenderCamera,
    drawable: DrawableSize,
) {
    let (projection, _) = camera.view_projection(drawable);
    let corners = std::array::from_fn::<_, 8, _>(|index| {
        glam::Vec3::new(
            if index & 1 == 0 { min[0] } else { max[0] },
            if index & 2 == 0 { min[1] } else { max[1] },
            if index & 4 == 0 { min[2] } else { max[2] },
        )
    });
    let uv = crate::engine::font::get_white_uv();
    let color = if accepted {
        [0.25, 1.0, 0.55, 0.8]
    } else {
        [0.5, 0.8, 1.0, 0.6]
    };
    for (index, corner) in corners.iter().enumerate() {
        for axis in [1, 2, 4] {
            let other = index ^ axis;
            if other <= index {
                continue;
            }
            let (Some((x0, y0)), Some((x1, y1))) = (
                project_to_reference(projection, drawable, *corner),
                corners
                    .get(other)
                    .and_then(|point| project_to_reference(projection, drawable, *point)),
            ) else {
                continue;
            };
            let from = glam::Vec2::new(x0, y0);
            let to = glam::Vec2::new(x1, y1);
            let Some(direction) = glam::Vec2::new(to.x - from.x, to.y - from.y).try_normalize()
            else {
                continue;
            };
            let normal = glam::Vec2::new(-direction.y * 0.35, direction.x * 0.35);
            for point in [
                glam::Vec2::new(from.x - normal.x, from.y - normal.y),
                glam::Vec2::new(to.x - normal.x, to.y - normal.y),
                glam::Vec2::new(to.x + normal.x, to.y + normal.y),
                glam::Vec2::new(from.x - normal.x, from.y - normal.y),
                glam::Vec2::new(to.x + normal.x, to.y + normal.y),
                glam::Vec2::new(from.x + normal.x, from.y + normal.y),
            ] {
                vertices.push(Vertex {
                    pos: [point.x, point.y, 0.0],
                    color,
                    uv: [uv[0], uv[1]],
                    ..Vertex::UNLIT
                });
            }
        }
    }
}
