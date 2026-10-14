//! Object interaction targeting and world-anchored label rendering.
//!
//! One typed dispatcher serves both interaction sources: a placed object's
//! `interactable` component (fired by the Interact key while the player looks
//! at the object) and a trigger volume's occupancy edges (fired when the
//! player's feet enter or leave its volume). This module owns the
//! presentation-side half:
//!
//! * [`Interactable`] / [`Interactables`] — the aiming table derived from the
//!   entities that carry an `interactable` component (plus label-only targets),
//!   resolved into a world anchor, oriented aim box and conservative label bound;
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

use crate::collision::{DoorCollider, WallAabb, nearest_door_entry, ray_aabb_entry};
use crate::door::DoorPhase;
use crate::game::Game;
use crate::level::LevelDef;
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

/// Millimetre tolerance for interaction surfaces and foreground blockers.
pub const INTERACTION_OCCLUSION_EPS_M: f32 = 0.001;

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

    Vec3::new(yaw.sin() * cos_pitch, pitch.sin(), -yaw.cos() * cos_pitch)
}

/// One placed instance that can be aimed at and acted upon.
#[derive(Debug, Clone, PartialEq)]
pub struct Interactable {
    /// Stable per-instance id (authored or deterministic default).
    pub id: String,
    /// Name a `toggle_label` action shows.
    pub display_name: String,
    /// Prompt shown while this instance is the current target. Empty means the
    /// instance's own default: a door shows the phase-appropriate open/close
    /// prompt, anything else shows [`DEFAULT_INTERACTION_PROMPT`].
    pub prompt: String,
    /// Interaction reach in metres, already sanitised.
    pub reach: f32,
    /// World position the floating label anchors to (top centre, plus margin).
    pub anchor: Vec3,
    /// Conservative world-space bounds for label placement and broad queries.
    pub bounds: Aabb,
    /// Current prop yaw for its exact oriented aim box; doors use their live
    /// collider instead. AI and route updates republish this with the bounds.
    pub yaw_degrees: f32,
    /// The instance's collision size contract `[width, height, depth]`, scaled:
    /// the authored `size` or the standard prop box. A routed instance
    /// republishes its live bounds from this and its current yaw.
    pub size: [f32; 3],
    /// The instance's own collision box when it is a solid prop, so occlusion
    /// never treats the target as its own blocker. `None` for non-solid props.
    pub own_box: Option<WallAabb>,
    /// For a door target: the door's index in [`Doors`], so the aim ray never
    /// treats the target leaf as its own occluder.
    pub door_index: Option<usize>,
    /// False disables aiming: the instance keeps its place in the table (and
    /// its label state) but E cannot target it until an `enable` action runs.
    pub enabled: bool,
}

/// The live pose a door publishes into its interaction entry each frame.
///
/// The aim bound must follow the leaf as it swings: a door's closed box and its
/// open box share almost no volume, so a static bound would let the player aim
/// at a door that is no longer there (or miss one that is).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InteractableSync {
    /// World-space bounds of the current leaf.
    pub bounds: Aabb,
    /// World-space label anchor (top centre of the leaf).
    pub anchor: Vec3,
}

impl InteractableSync {
    /// The live pose of one door collider.
    #[must_use]
    pub fn from_door_collider(collider: &DoorCollider) -> Self {
        let (hx, hz) = (collider.hinge_x, collider.hinge_z);
        let (dx, dz) = (collider.dir_x, collider.dir_z);
        let half_thickness = collider.thickness * 0.5;
        let (nx, nz) = (-dz * half_thickness, dx * half_thickness);
        let end_x = (collider.width).mul_add(dx, hx);
        let end_z = (collider.width).mul_add(dz, hz);

        // bounded world coordinates
        let corners = [
            (hx + nx, hz + nz),
            (end_x + nx, end_z + nz),
            (end_x - nx, end_z - nz),
            (hx - nx, hz - nz),
        ];
        let mut min_x = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut min_z = f32::INFINITY;
        let mut max_z = f32::NEG_INFINITY;
        for (x, z) in corners {
            min_x = min_x.min(x);
            max_x = max_x.max(x);
            min_z = min_z.min(z);
            max_z = max_z.max(z);
        }
        let top = collider.hinge_y + collider.height;
        Self {
            bounds: Aabb {
                min: [min_x, collider.hinge_y, min_z],
                max: [max_x, top, max_z],
            },
            anchor: Vec3::new(
                f32::midpoint(min_x, max_x),
                top + LABEL_HEIGHT_MARGIN_M,
                f32::midpoint(min_z, max_z),
            ),
        }
    }
}

/// The prompt a manually interactable door shows while closed.
pub const DOOR_PROMPT_OPEN: &str = "Open";
/// The prompt a manually interactable door shows while open.
pub const DOOR_PROMPT_CLOSE: &str = "Close";

/// The phase-dependent prompt for one door.
#[must_use]
pub fn door_prompt(phase: DoorPhase, authored: Option<&str>) -> String {
    if let Some(prompt) = authored.map(str::trim).filter(|prompt| !prompt.is_empty()) {
        return prompt.to_string();
    }
    match phase {
        DoorPhase::Closed | DoorPhase::Closing => DOOR_PROMPT_OPEN.to_string(),
        DoorPhase::Open | DoorPhase::Opening => DOOR_PROMPT_CLOSE.to_string(),
    }
}

/// Every target instance named anywhere in the level, trimmed.
///
/// A placed instance that any action or condition names joins the aiming table
/// as a label-only/cue-only entry with `enabled: false` (never aimable), so an
/// explicit target always resolves at runtime exactly as validation promised.
#[must_use]
pub(crate) fn referenced_targets(level: &LevelDef) -> Vec<String> {
    let mut targets = Vec::new();
    let mut push = |target: &str| {
        if !target.trim().is_empty() {
            targets.push(target.trim().to_string());
        }
    };
    for bindings in level.all_bindings() {
        for binding in bindings {
            for condition in &binding.when {
                push(condition.target());
            }
            for action in &binding.actions {
                if let Some(target) = action.target() {
                    push(target);
                }
            }
        }
    }
    for sequence in &level.sequences {
        for step in &sequence.steps {
            for action in step.actions() {
                if let Some(target) = action.target() {
                    push(target);
                }
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

    /// Builds the table from resolved entries.
    ///
    /// The entity runtime resolves one entry per placed instance that carries
    /// an `interactable` component (aimable) or is named by any binding
    /// (label-only), plus every manually interactable door; this constructor
    /// owns the result so the table can never disagree with the components it
    /// was built from.
    #[must_use]
    pub const fn with_items(items: Vec<Interactable>) -> Self {
        Self { items }
    }

    /// Republishes a door target's live aim bound, anchor and phase prompt.
    ///
    /// `door_index` identifies the door and `phase` decides the default prompt;
    /// the bounds and anchor come from the leaf's current collider.
    pub fn sync_door(
        &mut self,
        door_index: usize,
        collider: &DoorCollider,
        phase: DoorPhase,
        authored_prompt: Option<&str>,
    ) -> bool {
        let sync = InteractableSync::from_door_collider(collider);
        let Some(item) = self
            .items
            .iter_mut()
            .find(|item| item.door_index == Some(door_index))
        else {
            return false;
        };
        item.bounds = sync.bounds;
        item.anchor = sync.anchor;
        item.prompt = door_prompt(phase, authored_prompt);
        true
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

    /// Publishes a prompt changed by an authored action, reusing its storage.
    pub fn set_prompt(&mut self, id: &str, prompt: &str) -> bool {
        let Some(item) = self.items.iter_mut().find(|item| item.id == id) else {
            return false;
        };
        prompt.clone_into(&mut item.prompt);
        true
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
        if !position.is_finite() || !yaw_degrees.is_finite() {
            return false;
        }
        let Some(item) = self.items.get_mut(index) else {
            return false;
        };
        let [size_x, size_y, size_z] = item.size;
        let (extent_x, extent_z) = rotated_half_extents(size_x * 0.5, size_z * 0.5, yaw_degrees);
        // Bounded world-space math: the size contract is already validated.

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
            item.yaw_degrees = yaw_degrees;
        }
        true
    }
}

/// The axis-aligned half-extents of a yaw-rotated rectangle, in `(x, z)`.
///
/// The label bound is conservative: rotating the model can only grow its
/// axis-aligned footprint, never shrink it.
#[must_use]
pub(crate) fn rotated_half_extents(
    half_width: f32,
    half_depth: f32,
    rotation_degrees: f32,
) -> (f32, f32) {
    let (sin, cos) = rotation_degrees.to_radians().sin_cos();
    let abs_sin = sin.abs();
    let abs_cos = cos.abs();
    (
        half_width.mul_add(abs_cos, half_depth * abs_sin),
        half_width.mul_add(abs_sin, half_depth * abs_cos),
    )
}

/// The nearest **aimable** interactable the ray from `origin` along
/// `direction` reaches first, respecting each instance's own reach and the
/// collision world as occluders.
///
/// `direction` need not be normalised. A target is eligible when the ray enters
/// its bounds within that instance's reach and no wall obstructs the ray before
/// that entry. Props use their oriented size box and doors use the current leaf,
/// never the conservative axis-aligned bound that only the label uses: an
/// open leaf's swept AABB covers a wide angle around the hinge, and a ray that
/// misses the leaf but crosses that bound must not steal the aim from whatever
/// is actually on the line. A solid prop's own collision box is part of
/// `walls`, so a wall exactly matching the candidate's own box is excluded from
/// occlusion. Items with no actions (label-only targets) are not aimable and
/// never returned.
#[must_use]
pub fn nearest_target(
    origin: Vec3,
    direction: Vec3,
    items: &[Interactable],
    walls: &[WallAabb],
    doors: &[DoorCollider],
) -> Option<usize> {
    if items.is_empty() || !origin.is_finite() {
        return None;
    }
    let unit_direction = direction.try_normalize()?;
    let mut best: Option<(usize, f32)> = None;
    for (index, item) in items.iter().enumerate() {
        if !item.enabled {
            continue;
        }
        let Some(entry) = target_entry(origin, unit_direction, item, doors) else {
            continue;
        };
        if occluded_before(
            origin,
            unit_direction,
            entry,
            OcclusionTarget {
                own_box: item.own_box.as_ref(),
                own_door: item.door_index,
                margin: INTERACTION_OCCLUSION_EPS_M,
            },
            walls,
            doors,
        ) {
            continue;
        }
        if best.is_none_or(|(_, best_entry)| entry < best_entry) {
            best = Some((index, entry));
        }
    }
    best.map(|(index, _)| index)
}

/// Surface distance to one enabled target on an exact view ray, within reach.
///
/// `direction` is normalized before testing the prop's oriented size box or a
/// door's current leaf. This reports target geometry only; callers combine it
/// with world occlusion or nearer physics contacts to choose a global target.
#[must_use]
pub fn target_distance(
    origin: Vec3,
    direction: Vec3,
    item: &Interactable,
    doors: &[DoorCollider],
) -> Option<f32> {
    if !origin.is_finite() || !item.enabled {
        return None;
    }
    target_entry(origin, direction.try_normalize()?, item, doors)
}

/// Exact ray entry into an oriented target, with an already normalized ray.
fn target_entry(
    origin: Vec3,
    direction: Vec3,
    item: &Interactable,
    doors: &[DoorCollider],
) -> Option<f32> {
    if let Some(door_index) = item.door_index {
        let door = doors.get(door_index)?;
        return door.ray_entry(origin, direction, item.reach);
    }
    let [width, height, depth] = item.size;
    let [min_x, base_y, min_z] = item.bounds.min;
    let [max_x, _, max_z] = item.bounds.max;
    if !item.yaw_degrees.is_finite()
        || !item.size.iter().all(|size| size.is_finite() && *size > 0.0)
        || item.bounds.is_empty()
        || !item
            .bounds
            .min
            .iter()
            .chain(&item.bounds.max)
            .all(|value| value.is_finite())
    {
        return None;
    }
    let relative_origin = Vec3::new(
        origin.x - f32::midpoint(min_x, max_x),
        origin.y - base_y,
        origin.z - f32::midpoint(min_z, max_z),
    );
    let inverse_yaw = glam::Quat::from_rotation_y(-item.yaw_degrees.to_radians());
    let entry = ray_aabb_entry(
        inverse_yaw.mul_vec3(relative_origin),
        inverse_yaw.mul_vec3(direction),
        [-width * 0.5, 0.0, -depth * 0.5],
        [width * 0.5, height, depth * 0.5],
    )?;
    (entry <= item.reach).then_some(entry)
}

/// [`nearest_target`] through the collision index and the door leaves.
///
/// Identical semantics: the index only narrows which boxes each ray examines,
/// and the target's own collision body (a solid prop's box or the door's own
/// leaf) never occludes its own entry.
#[must_use]
pub fn nearest_target_indexed(
    origin: Vec3,
    direction: Vec3,
    items: &[Interactable],
    index: &crate::collision_index::CollisionIndex,
    walls: &[WallAabb],
    doors: &[DoorCollider],
) -> Option<usize> {
    if items.is_empty() || !origin.is_finite() {
        return None;
    }
    let unit_direction = direction.try_normalize()?;
    let mut best: Option<(usize, f32)> = None;
    for (item_index, item) in items.iter().enumerate() {
        if !item.enabled {
            continue;
        }
        let Some(entry) = target_entry(origin, unit_direction, item, doors) else {
            continue;
        };
        if occluded_before_indexed(
            origin,
            unit_direction,
            entry,
            OcclusionTarget {
                own_box: item.own_box.as_ref(),
                own_door: item.door_index,
                margin: INTERACTION_OCCLUSION_EPS_M,
            },
            index,
            walls,
            doors,
        ) {
            continue;
        }
        if best.is_none_or(|(_, best_entry)| entry < best_entry) {
            best = Some((item_index, entry));
        }
    }
    best.map(|(nearest_index, _)| nearest_index)
}

/// Self-collision exclusions and the sight margin for a target or label query.
#[derive(Clone, Copy)]
struct OcclusionTarget<'a> {
    /// A solid prop's own box, if the target is one.
    own_box: Option<&'a WallAabb>,
    /// The target door's leaf index, if the target is a door.
    own_door: Option<usize>,
    /// Interaction uses one millimetre; projected labels retain five centimetres.
    margin: f32,
}

/// True when the collision world leaves the finite sight line from `origin` to
/// `point` clear, excluding the target's own collision body.
#[must_use]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "bounded world coordinates, as above"
)] // bounded world coordinates, as above
pub fn clear_line_of_sight_indexed(
    origin: Vec3,
    point: Vec3,
    own_box: Option<&WallAabb>,
    own_door: Option<usize>,
    index: &crate::collision_index::CollisionIndex,
    walls: &[WallAabb],
    doors: &[DoorCollider],
) -> bool {
    let delta = point - origin;
    let length = delta.length();
    if !length.is_finite() {
        return false;
    }
    if length <= f32::EPSILON {
        return true;
    }
    let direction = delta / length;
    !occluded_before_indexed(
        origin,
        direction,
        length,
        OcclusionTarget {
            own_box,
            own_door,
            margin: LABEL_OCCLUSION_EPS_M,
        },
        index,
        walls,
        doors,
    )
}

/// True when any box or door leaf except the target's own blocks the ray
/// before `entry`, examined through the index.
#[must_use]
// bounded world coordinates, as above
fn occluded_before_indexed(
    origin: Vec3,
    direction: Vec3,
    entry: f32,
    own: OcclusionTarget<'_>,
    index: &crate::collision_index::CollisionIndex,
    walls: &[WallAabb],
    doors: &[DoorCollider],
) -> bool {
    let limit = entry - own.margin;
    if nearest_door_entry(doors, origin, direction, limit)
        .is_some_and(|(door_index, _)| own.own_door != Some(door_index))
    {
        return true;
    }
    let mut occluded = false;
    index.for_each_ray(origin, direction, entry, walls, |wall| {
        if occluded {
            return;
        }
        if own.own_box.is_some_and(|own_box| same_box(wall, own_box)) {
            return;
        }
        let wall_min = [wall.min_x, wall.min_y, wall.min_z];
        let wall_max = [wall.max_x, wall.max_y, wall.max_z];
        if let Some(wall_entry) = ray_aabb_entry(origin, direction, wall_min, wall_max)
            && wall_entry < limit
        {
            occluded = true;
        }
    });
    occluded
}

/// True when any wall or door leaf that is not the target's own collision body
/// blocks the ray before `entry`.
#[must_use]
fn occluded_before(
    origin: Vec3,
    direction: Vec3,
    entry: f32,
    own: OcclusionTarget<'_>,
    walls: &[WallAabb],
    doors: &[DoorCollider],
) -> bool {
    // Float comparison against a fixed tolerance; no overflow path exists for
    // bounded world coordinates.

    let limit = entry - own.margin;
    if nearest_door_entry(doors, origin, direction, limit)
        .is_some_and(|(index, _)| own.own_door != Some(index))
    {
        return true;
    }
    for wall in walls {
        if own.own_box.is_some_and(|own_box| same_box(wall, own_box)) {
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
        crate::ui::append_interaction_hint(vertices, key);
    }
}

/// Projected reference-space position of a world point, or `None` when it is
/// behind the camera or outside the UI viewport.
#[must_use]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "glam projection uses floating point components; finite and viewport checks precede pixel layout."
)]
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

#[cfg(test)]
mod tests {
    // Test code: unwrap/expect, indexing and permissive arithmetic are
    // idiomatic here; production lints stay enforced everywhere else.
    #![allow(
        clippy::expect_used,
        clippy::indexing_slicing,
        reason = "Regression fixtures assert exact reference results and fail on invalid setup; these exceptions are confined to tests"
    )]

    use super::*;
    use crate::collision::WallAabb;
    use crate::game::{CollisionWorld, Game, spawn_position};

    fn box_target(position: Vec3, size: [f32; 3], yaw_degrees: f32, reach: f32) -> Interactable {
        let mut table = Interactables::with_items(vec![Interactable {
            id: "box".into(),
            display_name: "box".into(),
            prompt: String::new(),
            reach,
            anchor: Vec3::ZERO,
            bounds: Aabb::EMPTY,
            yaw_degrees,
            size,
            own_box: None,
            door_index: None,
            enabled: true,
        }]);
        assert!(table.set_live_pose(0, position, yaw_degrees));
        table.get(0).expect("one synchronized target").clone()
    }

    /// Three descriptors cover nearest surface, off-axis rejection, distance
    /// normalization, reach and wall blocking without images or a map bake.
    #[test]
    fn exact_ray_targets_only_the_nearest_in_reach_surface() {
        let origin = Vec3::new(0.0, 0.5, 0.0);
        let direction = Vec3::NEG_Z;
        let near = box_target(Vec3::new(0.0, 0.0, -1.0), [0.1, 1.0, 0.1], 0.0, 2.5);
        let far = box_target(Vec3::new(0.0, 0.0, -2.0), [0.1, 1.0, 0.1], 0.0, 2.5);
        let off_axis = box_target(Vec3::new(0.25, 0.0, -0.4), [0.1, 1.0, 0.1], 0.0, 2.5);
        assert!(target_distance(origin, direction, &off_axis, &[]).is_none());
        assert_eq!(
            nearest_target(origin, direction, &[off_axis, far, near.clone()], &[], &[]),
            Some(2)
        );
        let distance = target_distance(origin, Vec3::new(0.0, 0.0, -10.0), &near, &[])
            .expect("normalized ray intersects the near surface");
        assert!((distance - 0.95).abs() < 1.0e-5);
        let mut outside_reach = near.clone();
        outside_reach.reach = 0.9;
        assert!(target_distance(origin, direction, &outside_reach, &[]).is_none());
        let wall = WallAabb::with_y(-0.2, 0.0, -0.7, 0.4, 1.0, 0.05);
        let collision_index = crate::collision_index::CollisionIndex::build(&[wall]);
        assert_eq!(
            nearest_target(origin, direction, std::slice::from_ref(&near), &[wall], &[]),
            None
        );
        assert_eq!(
            nearest_target_indexed(
                origin,
                direction,
                std::slice::from_ref(&near),
                &collision_index,
                &[wall],
                &[]
            ),
            None
        );
        assert!(target_distance(origin, Vec3::ZERO, &near, &[]).is_none());
        let close_wall = WallAabb::with_y(-0.2, 0.0, -0.945, 0.4, 1.0, 0.005);
        let close_index = crate::collision_index::CollisionIndex::build(&[close_wall]);
        assert_eq!(
            nearest_target(
                origin,
                direction,
                std::slice::from_ref(&near),
                &[close_wall],
                &[]
            ),
            None
        );
        assert_eq!(
            nearest_target_indexed(
                origin,
                direction,
                std::slice::from_ref(&near),
                &close_index,
                &[close_wall],
                &[]
            ),
            None
        );
        assert!(
            clear_line_of_sight_indexed(
                origin,
                Vec3::new(0.0, 0.5, -0.95),
                None,
                None,
                &close_index,
                &[close_wall],
                &[],
            ),
            "labels retain their five-centimetre surface margin"
        );
        assert!(target_distance(Vec3::splat(f32::NAN), direction, &near, &[]).is_none());
    }

    /// A thin diagonal target leaves empty corners in its AABB. This fixture
    /// catches off-axis selection and proves a live turn updates the exact box.
    #[test]
    fn thin_rotated_targets_use_their_live_oriented_box() {
        let item = box_target(Vec3::new(0.0, 0.0, -1.0), [2.0, 1.0, 0.05], 45.0, 4.0);
        let corner_origin = Vec3::new(0.6, 0.5, -0.4);
        assert!(ray_aabb_entry(corner_origin, Vec3::X, item.bounds.min, item.bounds.max).is_some());
        assert!(target_distance(corner_origin, Vec3::X, &item, &[]).is_none());
        let across_center = Vec3::new(0.0, 0.5, 0.0);
        assert!(target_distance(across_center, Vec3::NEG_Z, &item, &[]).is_some());
        let mut table = Interactables::with_items(vec![item]);
        assert!(table.set_live_pose(0, Vec3::new(0.0, 0.0, -1.0), 0.0));
        assert!(
            target_distance(
                corner_origin,
                Vec3::X,
                table.get(0).expect("live target"),
                &[]
            )
            .is_none()
        );
        assert!(table.set_live_pose(0, Vec3::new(0.0, 0.0, -0.4), 0.0));
        assert!(
            target_distance(
                corner_origin,
                Vec3::X,
                table.get(0).expect("moved target"),
                &[]
            )
            .is_some()
        );
        let mut level = test_level();
        level.spawn.x = 2.5;
        level.props[0].rotation_degrees = 45.0;
        level.props[0].size = Some([2.0, 1.8, 0.05]);
        let game = facing_game(&level);
        assert_eq!(
            nearest_target(
                game.player_position,
                game.view_direction(),
                game.interactables().items(),
                game.walls(),
                &[],
            ),
            Some(0),
            "the conservative solid collider must not occlude its rotated target"
        );
    }

    fn test_level() -> LevelDef {
        LevelDef::from_json(
            r#"{
                "format_version": 3,
                "id": "interact_render",
                "name": "Interact Render",
                "spawn": { "x": 2.0, "z": 5.0 },
                "rooms": [ { "x": 0.0, "z": 0.0, "width": 20.0, "depth": 20.0, "height": 4.0 } ],
                "props": [
                    { "id": "plant", "display_name": "Test Plant", "model": "core:plant",
                      "x": 4.6, "z": 5.0, "size": [0.6, 1.8, 0.6], "solid": true,
                      "components": [ { "component": "interactable",
                                        "prompt": "Toggle name" } ],
                      "bindings": [ { "on": "interact",
                                      "actions": [{ "action": "toggle_label" }] } ] }
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
        let (rotated_width, rotated_depth) = rotated_half_extents(0.4, 0.2, 90.0);
        assert!((rotated_width - 0.2).abs() < 1e-5 && (rotated_depth - 0.4).abs() < 1e-5);
        let (diagonal_width, diagonal_depth) = rotated_half_extents(0.4, 0.2, 45.0);
        let expected = (0.6_f32) * std::f32::consts::FRAC_1_SQRT_2;
        assert!(
            (diagonal_width - expected).abs() < 1e-5 && (diagonal_depth - expected).abs() < 1e-5
        );
    }

    /// Targeting respects reach and occlusion in both directions of the ray.
    #[test]
    fn nearest_target_respects_reach_and_occlusion() {
        let level = test_level();
        let game = facing_game(&level);
        let items = game.interactables().items();
        let origin = game.player_position;
        let direction = game.view_direction();
        assert_eq!(nearest_target(origin, direction, items, &[], &[]), Some(0));

        // Looking backwards misses.
        assert_eq!(
            nearest_target(origin, -direction, items, &[], &[]),
            None,
            "the target is only in front"
        );

        // A wall between the eye and the target occludes it.
        let wall = WallAabb::with_y(3.0, 1.0, 4.6, 0.2, 0.8, 0.8);
        assert_eq!(nearest_target(origin, direction, items, &[wall], &[]), None);

        // The target's own solid collision box (an exact match) never
        // self-occludes.
        let own = WallAabb::with_y(4.3, 0.0, 4.7, 0.6, 1.8, 0.6);
        assert_eq!(
            nearest_target(origin, direction, items, &[own], &[]),
            Some(0),
            "the target's own collision box must not self-occlude"
        );

        // A different barrier that merely intersects the target's bounds is a
        // real obstruction, not the target's own box.
        let barrier = WallAabb::with_y(4.2, 0.0, 4.7, 0.3, 1.2, 0.6);
        assert_eq!(
            nearest_target(origin, direction, items, &[barrier], &[]),
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
            nearest_target(crouched, low_direction, items, &[wall], &[]),
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

        let report = game
            .dispatch_interaction()
            .expect("the aimed plant fires its own bindings");
        assert_eq!(report.actions_run, 1);
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
        let mut blocked_walls = game.walls().to_vec();
        blocked_walls.push(WallAabb::with_y(3.0, 1.0, 4.6, 0.2, 0.8, 0.8));
        game.set_walls(blocked_walls);
        let mut occluded = Vec::new();
        append_world_labels(&mut occluded, &game, &camera, drawable);
        assert!(
            occluded.is_empty(),
            "an occluded label and a blocked target draw nothing"
        );
    }

    /// A door target's aim entry is measured against the oriented leaf, never
    /// the conservative axis-aligned bound its label uses: a ray that crosses
    /// the bound of a swung leaf but misses the leaf itself must reach the
    /// object behind it, while a ray on the leaf still resolves the door.
    #[test]
    fn door_targets_use_the_oriented_leaf_for_aim_entry() {
        // A leaf hinged at the origin and swung 45 degrees onto the +X/+Z
        // diagonal, 1.6 m wide and 5 cm thick. Its axis-aligned bound is the
        // diagonal slab's bounding square, which the 5 cm leaf does not fill:
        // the corner regions of the bound are clear of the leaf.
        let half_turn = std::f32::consts::FRAC_1_SQRT_2;
        let door = DoorCollider::from_pose([0.0, 0.0, 0.0], [half_turn, half_turn], 1.6, 0.05, 2.1);
        let sync = InteractableSync::from_door_collider(&door);
        let door_item = Interactable {
            id: "door".into(),
            display_name: "door".into(),
            prompt: String::new(),
            reach: 4.0,
            anchor: sync.anchor,
            bounds: sync.bounds,
            yaw_degrees: 0.0,
            size: [1.6, 2.1, 0.05],
            own_box: None,
            door_index: Some(0),
            enabled: true,
        };
        // A small aimable prop beyond the bound's far corner, off the leaf
        // line. The ray runs parallel to the leaf at a fixed 0.35 m offset, so
        // it crosses the bound and never touches the 5 cm slab.
        let prop_item = Interactable {
            id: "prop".into(),
            display_name: "prop".into(),
            prompt: String::new(),
            reach: 4.0,
            anchor: Vec3::new(1.4, 1.48, 0.9),
            bounds: crate::spatial::Aabb {
                min: [1.3, 0.0, 0.75],
                max: [1.5, 1.2, 1.05],
            },
            yaw_degrees: 0.0,
            size: [0.2, 1.2, 0.3],
            own_box: None,
            door_index: None,
            enabled: true,
        };
        let origin = Vec3::new(0.0, 1.0, -0.5);
        let direction = Vec3::new(1.0, 0.0, 1.0).normalize();
        let bound_entry = ray_aabb_entry(origin, direction, sync.bounds.min, sync.bounds.max)
            .expect("the precondition: the conservative bound is crossed");
        assert_eq!(
            door.ray_entry(origin, direction, 4.0),
            None,
            "the precondition: the ray runs parallel to the leaf, clear of it"
        );
        let prop_entry = ray_aabb_entry(
            origin,
            direction,
            prop_item.bounds.min,
            prop_item.bounds.max,
        )
        .expect("the aimable prop is on the ray");
        assert!(
            bound_entry < prop_entry,
            "the bound would win if it were the aim entry: {bound_entry} < {prop_entry}"
        );
        assert_eq!(
            nearest_target(
                origin,
                direction,
                &[door_item.clone(), prop_item.clone()],
                &[],
                &[door]
            ),
            Some(1),
            "a ray that misses the leaf but crosses its bound reaches the prop"
        );

        // A ray that does reach the leaf still resolves the door.
        let on_leaf = Vec3::new(1.0, 1.0, -0.5);
        let across_leaf = Vec3::new(0.0, 0.0, 1.0);
        assert!(
            door.ray_entry(on_leaf, across_leaf, 4.0).is_some(),
            "the precondition: the second ray crosses the leaf"
        );
        assert_eq!(
            nearest_target(on_leaf, across_leaf, &[door_item, prop_item], &[], &[door]),
            Some(0),
            "the leaf itself is still the target"
        );
    }
}
