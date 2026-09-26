//! Object interaction targeting and world-anchored label rendering.
//!
//! One typed dispatcher serves both interaction sources: a placed object's
//! `interaction` actions (fired by the Interact key while the player looks at
//! the object) and an area trigger's actions (fired when the player enters its
//! volume). This module owns the presentation-side half:
//!
//! * [`Interactable`] / [`Interactables`] — the placed props and entities that
//!   carry a map-authored interaction, resolved once at level load into a
//!   world-space anchor and an axis-aligned bound;
//! * [`nearest_target`] — the controller's "nearest eligible object the player
//!   is looking at" test, including reach, occlusion and the acting player's
//!   stance-aware eye;
//! * [`append_world_labels`] — floating display names and the aimed-at prompt,
//!   drawn with the existing 2D text/UI pipeline by projecting world anchors
//!   into the 480x272 reference space. This is deliberately not a second text
//!   renderer: it emits the same [`Vertex`] format the menus use.
//!
//! The action *semantics* live in [`crate::game::Game`]; this module never
//! mutates the run. Identity is per placed instance (see
//! [`crate::level::LevelDef::prop_instance_ids`]), so two copies of one model
//! resolve to two independent targets.

use glam::Vec3;

use crate::collision::{WallAabb, ray_aabb_entry};
use crate::game::Game;
use crate::level::{
    ActionDef, DEFAULT_INTERACTION_PROMPT, LevelDef, LevelSurfaces, PROP_FALLBACK_SIZE,
};
use crate::render::{DrawableSize, RenderCamera, UI_REFERENCE_HEIGHT, UI_REFERENCE_WIDTH, Vertex};
use crate::spatial::Aabb;

/// Interaction reach in metres when a placed object authors none.
///
/// Short by design: the player must stand at the object, not aim at it across
/// a room. The office desk is 1.6 m wide, so 2.5 m reaches its centre from
/// either face with margin.
pub const DEFAULT_INTERACTION_REACH_M: f32 = 2.5;

/// Hard cap on an authored interaction reach, in metres.
///
/// The loader rejects anything above this, so a map can never author an
/// interaction that reaches across a room or through the void.
pub const MAX_INTERACTION_REACH_M: f32 = 4.0;

/// How far above an object's bound top its floating label sits, in metres.
const LABEL_HEIGHT_MARGIN_M: f32 = 0.28;

/// Tolerance within which a wall counts as the target's own collision box, in
/// metres: solid props are part of the collision world and must not occlude
/// themselves.
const SELF_OCCLUSION_EPS_M: f32 = 1.0e-3;

/// Extra clearance before a wall counts as occluding a label, in metres.
///
/// A label anchored exactly on a surface must not flicker because the ray
/// grazes that surface.
const LABEL_OCCLUSION_EPS_M: f32 = 0.05;

/// Scale of floating world labels in the 480x272 reference space.
const LABEL_TEXT_SCALE: f32 = 1.0;

/// The eye direction for a yaw/pitch pair, matching the render camera.
///
/// Yaw 0 looks toward `-Z`, yaw 90 toward `+X`; positive pitch looks up.
#[must_use]
pub fn view_direction(yaw: f32, pitch: f32) -> Vec3 {
    let cos_pitch = pitch.cos();
    // Per-component `f32` trigonometry with bounded operands; the lint cannot
    // see that through the operators.
    #[allow(clippy::arithmetic_side_effects)]
    Vec3::new(yaw.sin() * cos_pitch, pitch.sin(), -yaw.cos() * cos_pitch)
}

/// One placed instance that can be aimed at and acted upon, or named as a
/// `toggle_label` target.
#[derive(Debug, Clone, PartialEq)]
pub struct Interactable {
    /// Stable per-instance id (authored or deterministic default).
    pub id: String,
    /// Name a `toggle_label` action shows.
    pub display_name: String,
    /// Prompt shown while this instance is the current target.
    pub prompt: String,
    /// Interaction reach in metres, already sanitised.
    pub reach: f32,
    /// World position the floating label anchors to (top centre, plus margin).
    pub anchor: Vec3,
    /// Conservative world-space bounds for the view-ray test.
    pub bounds: Aabb,
    /// The instance's collision size contract `[width, height, depth]`, scaled:
    /// the authored `size` or the standard prop box. A routed instance
    /// republishes its live bounds from this and its current yaw.
    pub size: [f32; 3],
    /// The instance's own collision box when it is a solid prop, so occlusion
    /// never treats the target as its own blocker. `None` for non-solid props.
    pub own_box: Option<WallAabb>,
    /// Actions one press performs, in order. Empty for a label-only target that
    /// another instance toggles.
    pub actions: Vec<ActionDef>,
}

/// Every target instance named anywhere in the level, trimmed.
///
/// A `toggle_label`, `play_animation` or `toggle_animation` action may name a
/// placed prop that carries no interaction of its own; those props join the
/// aimable set as label-only/cue-only instances with empty actions (never
/// aimable), so an explicit target always resolves at runtime exactly as
/// validation promised.
#[must_use]
fn referenced_targets(level: &LevelDef) -> Vec<String> {
    let mut targets = Vec::new();
    let interactions = level
        .props
        .iter()
        .filter_map(|prop| prop.interaction.as_ref())
        .map(|interaction| interaction.actions.as_slice())
        .chain(
            level
                .area_triggers
                .iter()
                .map(|trigger| trigger.actions.as_slice()),
        );
    for actions in interactions {
        for action in actions {
            let target = match action {
                ActionDef::ToggleLabel {
                    target: Some(target),
                }
                | ActionDef::PlayAnimation {
                    target: Some(target),
                    ..
                }
                | ActionDef::ToggleAnimation {
                    target: Some(target),
                    ..
                } => Some(target),
                ActionDef::ToggleLabel { target: None }
                | ActionDef::PlayAnimation { target: None, .. }
                | ActionDef::ToggleAnimation { target: None, .. }
                | ActionDef::ResetToStart
                | ActionDef::PlayAudio { .. } => None,
            };
            if let Some(target) = target
                && !target.trim().is_empty()
            {
                targets.push(target.trim().to_string());
            }
        }
    }
    targets
}

/// The placed instances that carry a map-authored interaction, plus any placed
/// prop named as a `toggle_label` or `play_animation` target, resolved once per
/// level load.
///
/// An item with actions is **aimable** (E in reach runs them); an item with no
/// actions is a **label-only/cue-only target** that another instance's action
/// can toggle or pose. A level with neither is empty: the Interact key finds
/// nothing and no labels can be toggled.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Interactables {
    items: Vec<Interactable>,
}

impl Interactables {
    /// An empty set: nothing is aimable.
    #[must_use]
    pub const fn new() -> Self {
        Self { items: Vec::new() }
    }

    /// Resolves every aimable prop and every prop referenced as a label/cue
    /// target.
    ///
    /// Bounds use the same size contract as collision
    /// ([`crate::level::PropDef::resolved_size`] against
    /// [`PROP_FALLBACK_SIZE`]): the authored `size`, scaled, else the standard
    /// prop box. A solid prop that should be aimable at its picture authors
    /// `size`, exactly as it does to block correctly. Malformed entries are
    /// skipped; a loaded level has already been validated.
    #[must_use]
    pub fn from_level(level: &LevelDef) -> Self {
        let surfaces = LevelSurfaces::new(level);
        let ids = level.prop_instance_ids();
        let referenced = referenced_targets(level);
        let mut items = Vec::new();
        for (index, prop) in level.props.iter().enumerate() {
            let interaction = prop.interaction.as_ref();
            let actions = interaction.map(|interaction| interaction.actions.as_slice());
            let aimable = actions.is_some_and(|actions| !actions.is_empty());
            let id = ids
                .get(index)
                .filter(|id| !id.trim().is_empty())
                .cloned()
                .unwrap_or_else(|| format!("prop_{index}"));
            let is_target = referenced.iter().any(|target| target == &id);
            if !aimable && !is_target {
                continue;
            }
            let size = prop.resolved_size(PROP_FALLBACK_SIZE);
            let [size_x, size_y, size_z] = size;
            if !size.iter().all(|value| value.is_finite() && *value > 0.0)
                || !prop.x.is_finite()
                || !prop.y.is_finite()
                || !prop.z.is_finite()
                || !prop.rotation_degrees.is_finite()
            {
                continue;
            }
            let display_name = prop
                .display_name
                .as_deref()
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .unwrap_or(prop.model.as_str())
                .to_string();
            let prompt = interaction
                .and_then(|interaction| interaction.prompt.as_deref())
                .map(str::trim)
                .filter(|prompt| !prompt.is_empty())
                .unwrap_or(DEFAULT_INTERACTION_PROMPT)
                .to_string();
            let reach = interaction
                .and_then(|interaction| interaction.reach)
                .filter(|reach| reach.is_finite() && *reach > 0.0)
                .map_or(DEFAULT_INTERACTION_REACH_M, |reach| {
                    reach.min(MAX_INTERACTION_REACH_M)
                });
            let base_y = surfaces.floor_y_at(prop.x, prop.z).unwrap_or(0.0);
            let (extent_x, extent_z) =
                rotated_half_extents(size_x * 0.5, size_z * 0.5, prop.rotation_degrees);
            // World-space bound math with bounded level data; the operators are
            // the formula, not unchecked indexing.
            #[allow(clippy::arithmetic_side_effects)]
            let (bounds, own_box, anchor) = {
                let base = base_y + prop.y;
                let own_box = prop.solid.then(|| {
                    WallAabb::with_y(
                        size_x.mul_add(-0.5, prop.x),
                        base,
                        size_z.mul_add(-0.5, prop.z),
                        size_x,
                        size_y,
                        size_z,
                    )
                });
                (
                    Aabb {
                        min: [prop.x - extent_x, base, prop.z - extent_z],
                        max: [prop.x + extent_x, base + size_y, prop.z + extent_z],
                    },
                    own_box,
                    Vec3::new(prop.x, base + size_y + LABEL_HEIGHT_MARGIN_M, prop.z),
                )
            };
            items.push(Interactable {
                id,
                display_name,
                prompt,
                reach,
                anchor,
                bounds,
                size,
                own_box,
                actions: actions.map_or_else(Vec::new, <[ActionDef]>::to_vec),
            });
        }
        Self { items }
    }

    /// True when no placed object declares an interaction.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Number of interactable instances.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.items.len()
    }

    /// Every interactable instance, in placed-prop order.
    #[must_use]
    pub fn items(&self) -> &[Interactable] {
        &self.items
    }

    /// The instance at `index`, if it exists.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<&Interactable> {
        self.items.get(index)
    }

    /// The index of the instance called `id`, if any.
    #[must_use]
    pub fn index_of(&self, id: &str) -> Option<usize> {
        self.items.iter().position(|item| item.id == id)
    }

    /// Republishes one routed instance's live anchor and bounds.
    ///
    /// `position` is the entity's live base (feet) position and `yaw_degrees`
    /// its current facing. The bounds are recomputed from the instance's own
    /// size contract at that yaw, so a route turn re-orients the aim box
    /// instead of leaving the spawn yaw's box in place; the label anchor stays
    /// the top centre (a rotation about the model origin does not move it).
    /// `own_box` is not moved: a routed entity is validated as `solid: false`,
    /// and a solid prop's box stays where the level authored it.
    pub fn set_live_pose(&mut self, index: usize, position: Vec3, yaw_degrees: f32) -> bool {
        let Some(item) = self.items.get_mut(index) else {
            return false;
        };
        let [size_x, size_y, size_z] = item.size;
        let (extent_x, extent_z) = rotated_half_extents(size_x * 0.5, size_z * 0.5, yaw_degrees);
        // Bounded world-space math: the size contract is already validated.
        #[allow(clippy::arithmetic_side_effects)]
        {
            item.bounds = Aabb {
                min: [position.x - extent_x, position.y, position.z - extent_z],
                max: [
                    position.x + extent_x,
                    position.y + size_y,
                    position.z + extent_z,
                ],
            };
            item.anchor = Vec3::new(
                position.x,
                position.y + size_y + LABEL_HEIGHT_MARGIN_M,
                position.z,
            );
        }
        true
    }
}

/// The axis-aligned half-extents of a yaw-rotated rectangle, in `(x, z)`.
///
/// The interaction bound is conservative: rotating the model can only grow its
/// axis-aligned footprint, never shrink it.
#[must_use]
fn rotated_half_extents(half_width: f32, half_depth: f32, rotation_degrees: f32) -> (f32, f32) {
    let (sin, cos) = rotation_degrees.to_radians().sin_cos();
    let sin = sin.abs();
    let cos = cos.abs();
    (
        half_width.mul_add(cos, half_depth * sin),
        half_width.mul_add(sin, half_depth * cos),
    )
}

/// The nearest **aimable** interactable the ray from `origin` along
/// `direction` reaches first, respecting each instance's own reach and the
/// collision world as occluders.
///
/// `direction` need not be normalised. A target is eligible when the ray enters
/// its bounds within that instance's reach and no wall obstructs the ray before
/// that entry. A solid prop's own collision box is part of `walls`, so a wall
/// exactly matching the candidate's own box is excluded from occlusion. Items
/// with no actions (label-only targets) are not aimable and never returned.
#[must_use]
pub fn nearest_target(
    origin: Vec3,
    direction: Vec3,
    items: &[Interactable],
    walls: &[WallAabb],
) -> Option<usize> {
    if items.is_empty() || direction.length_squared() <= f32::EPSILON {
        return None;
    }
    let direction = direction.normalize();
    let mut best: Option<(usize, f32)> = None;
    for (index, item) in items.iter().enumerate() {
        if item.actions.is_empty() {
            continue;
        }
        let Some(entry) = ray_aabb_entry(origin, direction, item.bounds.min, item.bounds.max)
        else {
            continue;
        };
        if entry > item.reach {
            continue;
        }
        if occluded_before(origin, direction, entry, item.own_box.as_ref(), walls) {
            continue;
        }
        if best.is_none_or(|(_, best_entry)| entry < best_entry) {
            best = Some((index, entry));
        }
    }
    best.map(|(index, _)| index)
}

/// True when any wall that is not the target's own collision box blocks the ray
/// before `entry`.
#[must_use]
fn occluded_before(
    origin: Vec3,
    direction: Vec3,
    entry: f32,
    own_box: Option<&WallAabb>,
    walls: &[WallAabb],
) -> bool {
    // Float comparison against a fixed tolerance; no overflow path exists for
    // bounded world coordinates.
    #[allow(clippy::arithmetic_side_effects)]
    let limit = entry - LABEL_OCCLUSION_EPS_M;
    for wall in walls {
        if own_box.is_some_and(|own| same_box(wall, own)) {
            continue;
        }
        let wall_min = [wall.min_x, wall.min_y, wall.min_z];
        let wall_max = [wall.max_x, wall.max_y, wall.max_z];
        if let Some(wall_entry) = ray_aabb_entry(origin, direction, wall_min, wall_max)
            && wall_entry < limit
        {
            return true;
        }
    }
    false
}

/// True when two collision boxes are the same box within a millimetre.
///
/// This is deliberately an equality test, not an overlap test: only the
/// target's own box may be skipped as an occluder. A separate barrier that
/// merely intersects the target's (rotation-inflated) bounds still occludes.
#[must_use]
fn same_box(a: &WallAabb, b: &WallAabb) -> bool {
    let close = |lhs: f32, rhs: f32| (lhs - rhs).abs() <= SELF_OCCLUSION_EPS_M;
    close(a.min_x, b.min_x)
        && close(a.max_x, b.max_x)
        && close(a.min_y, b.min_y)
        && close(a.max_y, b.max_y)
        && close(a.min_z, b.min_z)
        && close(a.max_z, b.max_z)
}

/// True when nothing in the collision world blocks the sight line from
/// `origin` to `point`.
///
/// Used for floating labels: a name behind a wall is hidden rather than drawn
/// through it. The target's own collision box (`own_box`) is excluded exactly as
/// in [`nearest_target`].
#[must_use]
#[allow(clippy::arithmetic_side_effects)]
pub fn clear_line_of_sight(
    origin: Vec3,
    point: Vec3,
    own_box: Option<&WallAabb>,
    walls: &[WallAabb],
) -> bool {
    // A finite delta length with bounded world coordinates: the subtraction,
    // length and normalisation are the formula, not unchecked arithmetic.
    let delta = point - origin;
    let length = delta.length();
    if !length.is_finite() {
        return false;
    }
    if length <= f32::EPSILON {
        return true;
    }
    let direction = delta / length;
    !occluded_before(origin, direction, length, own_box, walls)
}

/// Appends the floating display names of every toggled-on instance and the
/// prompt of the currently aimed-at instance to an existing UI vertex list.
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
    let viewport = drawable.ui_viewport();
    if viewport.width <= 0 || viewport.height <= 0 {
        return;
    }
    let (view_projection, _) = camera.view_projection(drawable);
    let items = game.interactables().items();
    for (index, item) in items.iter().enumerate() {
        if !game.is_label_visible(index) {
            continue;
        }
        if !clear_line_of_sight(
            camera.position,
            item.anchor,
            item.own_box.as_ref(),
            game.walls(),
        ) {
            continue;
        }
        if let Some((x, y)) = project_to_reference(view_projection, drawable, item.anchor) {
            draw_world_label(vertices, &item.display_name, x, y);
        }
    }
    if let Some(target) = game.interaction_target()
        && let Some(item) = items.get(target)
    {
        draw_interaction_prompt(vertices, &item.prompt);
    }
}

/// Projected reference-space position of a world point, or `None` when it is
/// behind the camera or outside the UI viewport.
#[must_use]
#[allow(clippy::arithmetic_side_effects)]
fn project_to_reference(
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
    let pixel_from_top = (0.5 - ndc_y * 0.5) * pixel_f32(drawable.height);
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
#[allow(clippy::arithmetic_side_effects)]
fn draw_world_label(vertices: &mut Vec<Vertex>, text: &str, x: f32, y: f32) {
    // Reference-space layout arithmetic: fixed, bounded pixel offsets.
    let width = crate::ui::text_width(text, LABEL_TEXT_SCALE);
    let left = width.mul_add(-0.5, x);
    let top = y - 3.0;
    crate::ui::add_rect_rgba(
        vertices,
        left - 3.0,
        top - 2.0,
        left + width + 3.0,
        top + 11.0,
        [0.05, 0.05, 0.06, 0.62],
    );
    crate::ui::draw_text(
        vertices,
        text,
        left,
        top,
        LABEL_TEXT_SCALE,
        [0.96, 0.94, 0.78],
    );
}

/// Draws the Interact prompt under the crosshair when something is in reach.
#[allow(clippy::arithmetic_side_effects)]
fn draw_interaction_prompt(vertices: &mut Vec<Vertex>, prompt: &str) {
    // Reference-space layout arithmetic: fixed, bounded pixel offsets.
    let text = format!("[E] {prompt}");
    let width = crate::ui::text_width(&text, 1.0);
    let x = width.mul_add(-0.5, pixel_f32(UI_REFERENCE_WIDTH) * 0.5);
    let y = pixel_f32(UI_REFERENCE_HEIGHT).mul_add(0.5, 22.0);
    crate::ui::add_rect_rgba(
        vertices,
        x - 4.0,
        y - 3.0,
        x + width + 4.0,
        y + 11.0,
        [0.05, 0.05, 0.06, 0.55],
    );
    crate::ui::draw_text(vertices, &text, x, y, 1.0, [0.98, 0.96, 0.80]);
}

#[cfg(test)]
mod tests {
    // Test code: unwrap/expect, indexing and permissive arithmetic are
    // idiomatic here; production lints stay enforced everywhere else.
    #![allow(clippy::expect_used, clippy::indexing_slicing)]

    use super::*;
    use crate::collision::WallAabb;
    use crate::game::{CollisionWorld, Game, spawn_position};

    fn test_level() -> LevelDef {
        LevelDef::from_json(
            r#"{
                "format_version": 1,
                "id": "interact_render",
                "name": "Interact Render",
                "spawn": { "x": 2.0, "z": 5.0 },
                "room": { "x": 0.0, "z": 0.0, "width": 20.0, "depth": 20.0, "height": 4.0 },
                "props": [
                    { "id": "plant", "display_name": "Test Plant", "model": "core:plant",
                      "x": 4.6, "z": 5.0, "size": [0.6, 1.8, 0.6], "solid": true,
                      "interaction": { "prompt": "Toggle name",
                                       "actions": [{ "action": "toggle_label" }] } }
                ]
            }"#,
        )
        .expect("the render test level parses")
    }

    fn facing_game(level: &LevelDef) -> Game {
        let mut game = Game::new(
            spawn_position(level),
            level.spawn.yaw_degrees.to_radians(),
            CollisionWorld::from_level(level),
        );
        game.set_app_state(crate::game::AppState::Playing);
        let item = game
            .interactables()
            .get(0)
            .expect("one interactable")
            .clone();
        let dx = item.bounds.min[0] - game.player_position.x;
        let dz = f32::midpoint(item.bounds.min[2], item.bounds.max[2]) - game.player_position.z;
        let target_y = f32::midpoint(item.bounds.min[1], item.bounds.max[1]);
        let flat = dx.hypot(dz);
        game.player_yaw = dx.atan2(-dz);
        game.player_pitch = (target_y - game.player_position.y).atan2(flat);
        game
    }

    /// The eye direction matches the render camera's yaw/pitch convention.
    #[test]
    fn view_direction_matches_camera_conventions() {
        let forward = view_direction(0.0, 0.0);
        assert!((forward - Vec3::new(0.0, 0.0, -1.0)).length() < 1e-6);
        let right = view_direction(std::f32::consts::FRAC_PI_2, 0.0);
        assert!((right - Vec3::new(1.0, 0.0, 0.0)).length() < 1e-6);
        let up = view_direction(0.0, std::f32::consts::FRAC_PI_2);
        assert!((up - Vec3::new(0.0, 1.0, 0.0)).length() < 1e-6);
    }

    /// A yaw-rotated rectangle's axis-aligned bound only ever grows.
    #[test]
    fn rotated_half_extents_are_conservative() {
        let (x, z) = rotated_half_extents(0.4, 0.2, 0.0);
        assert!((x - 0.4).abs() < 1e-5 && (z - 0.2).abs() < 1e-5);
        let (x, z) = rotated_half_extents(0.4, 0.2, 90.0);
        assert!((x - 0.2).abs() < 1e-5 && (z - 0.4).abs() < 1e-5);
        let (x, z) = rotated_half_extents(0.4, 0.2, 45.0);
        let expected = (0.6_f32) * std::f32::consts::FRAC_1_SQRT_2;
        assert!((x - expected).abs() < 1e-5 && (z - expected).abs() < 1e-5);
    }

    /// Targeting respects reach and occlusion in both directions of the ray.
    #[test]
    fn nearest_target_respects_reach_and_occlusion() {
        let level = test_level();
        let game = facing_game(&level);
        let items = game.interactables().items();
        let origin = game.player_position;
        let direction = game.view_direction();
        assert_eq!(nearest_target(origin, direction, items, &[]), Some(0));

        // Looking backwards misses.
        assert_eq!(
            nearest_target(origin, -direction, items, &[]),
            None,
            "the target is only in front"
        );

        // A wall between the eye and the target occludes it.
        let wall = WallAabb::with_y(3.0, 1.0, 4.6, 0.2, 0.8, 0.8);
        assert_eq!(nearest_target(origin, direction, items, &[wall]), None);

        // The target's own solid collision box (an exact match) never
        // self-occludes.
        let own = WallAabb::with_y(4.3, 0.0, 4.7, 0.6, 1.8, 0.6);
        assert_eq!(
            nearest_target(origin, direction, items, &[own]),
            Some(0),
            "the target's own collision box must not self-occlude"
        );

        // A different barrier that merely intersects the target's bounds is a
        // real obstruction, not the target's own box.
        let barrier = WallAabb::with_y(4.2, 0.0, 4.7, 0.3, 1.2, 0.6);
        assert_eq!(
            nearest_target(origin, direction, items, &[barrier]),
            None,
            "a barrier overlapping the target's bounds still occludes"
        );

        // Crouching lowers the origin: the same ray then passes below the wall.
        let crouched = Vec3::new(origin.x, origin.y - 0.8, origin.z);
        let target_y = 0.9;
        let flat = (items[0].bounds.min[0] - crouched.x).abs();
        let pitch = (target_y - crouched.y).atan2(flat);
        let low_direction = view_direction(std::f32::consts::FRAC_PI_2, pitch);
        assert_eq!(
            nearest_target(crouched, low_direction, items, &[wall]),
            Some(0),
            "a crouched eye clears a high obstruction"
        );
    }

    /// The projection maps the view centre to the reference centre, rejects
    /// points behind the camera and clamps to the viewport.
    #[test]
    fn projection_maps_the_view_to_the_reference_space() {
        let drawable = DrawableSize::new(480, 272);
        let camera = RenderCamera::new(Vec3::ZERO, 0.0, 0.0, 90.0);
        let (view_projection, _) = camera.view_projection(drawable);

        let centre = project_to_reference(view_projection, drawable, Vec3::new(0.0, 0.0, -5.0))
            .expect("the view centre projects");
        assert!((centre.0 - 240.0).abs() < 1.0, "{centre:?}");
        assert!((centre.1 - 136.0).abs() < 1.0, "{centre:?}");

        assert!(
            project_to_reference(view_projection, drawable, Vec3::new(0.0, 0.0, 5.0)).is_none(),
            "a point behind the camera is not drawn"
        );
        assert!(
            project_to_reference(view_projection, drawable, Vec3::new(100.0, 0.0, -5.0)).is_none(),
            "a point outside the viewport is not drawn"
        );
    }

    /// Floating labels use the existing text pipeline: a toggled-on label
    /// emits vertices, an occluded one emits none, and the aimed-at prompt
    /// disappears with the target.
    #[test]
    fn labels_render_through_the_ui_text_pipeline_and_respect_occlusion() {
        let level = test_level();
        let mut game = facing_game(&level);
        let drawable = DrawableSize::new(480, 272);
        let camera = RenderCamera::new(
            game.player_position,
            game.player_yaw,
            game.player_pitch,
            70.0,
        );

        // Before the toggle: only the aimed-at prompt is emitted.
        let mut before = Vec::new();
        append_world_labels(&mut before, &game, &camera, drawable);
        assert!(!before.is_empty(), "the aimed-at prompt draws");

        let actions = game.interactables().get(0).expect("target").actions.clone();
        game.dispatch_actions(&actions, Some(0));
        assert!(game.is_label_visible(0));
        let mut with_label = Vec::new();
        append_world_labels(&mut with_label, &game, &camera, drawable);
        assert!(
            with_label.len() > before.len(),
            "the label adds vertices: {} vs {}",
            with_label.len(),
            before.len()
        );

        // A wall between the eye and the anchor hides both the label and the
        // now-unreachable prompt.
        game.walls
            .push(WallAabb::with_y(3.0, 1.0, 4.6, 0.2, 0.8, 0.8));
        let mut occluded = Vec::new();
        append_world_labels(&mut occluded, &game, &camera, drawable);
        assert!(
            occluded.is_empty(),
            "an occluded label and a blocked target draw nothing"
        );
    }
}
