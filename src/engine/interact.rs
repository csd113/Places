//! Object interaction targets, reach, and world-space occlusion queries.
//!
//! The aiming table resolves authored instances into live anchors, oriented
//! target boxes, and conservative label bounds. Callers own action dispatch and
//! presentation; this module only resolves which target a ray can reach.

use glam::Vec3;

use crate::engine::collision::{DoorCollider, WallAabb, ray_aabb_entry};
use crate::engine::door::DoorPhase;
use crate::engine::level::LevelDef;
use crate::engine::spatial::{Aabb, rotated_half_extents};

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
    /// Half-open range of live colliders owned by a compound mechanism.
    pub leaf_range: Option<(usize, usize)>,
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
    /// Bind a compound target to its actual live leaves and fixed frame stock.
    pub fn sync_leaves(&mut self, id: &str, start: usize, end: usize, doors: &[DoorCollider]) {
        let Some(item) = self.items.iter_mut().find(|item| item.id == id) else {
            return;
        };
        item.leaf_range = Some((start, end));
        let mut bounds = Aabb::EMPTY;
        for leaf in doors.get(start..end).unwrap_or(&[]) {
            let sync = InteractableSync::from_door_collider(leaf);
            bounds.expand(sync.bounds.min);
            bounds.expand(sync.bounds.max);
        }
        item.bounds = bounds;
    }
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
                own_leaves: item.leaf_range,
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
    if let Some((start, end)) = item.leaf_range {
        return doors
            .get(start..end)?
            .iter()
            .filter_map(|leaf| leaf.ray_entry(origin, direction, item.reach))
            .min_by(f32::total_cmp);
    }
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
    index: &crate::engine::collision_index::CollisionIndex,
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
                own_leaves: item.leaf_range,
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
    own_leaves: Option<(usize, usize)>,
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
    index: &crate::engine::collision_index::CollisionIndex,
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
            own_leaves: None,
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
    index: &crate::engine::collision_index::CollisionIndex,
    walls: &[WallAabb],
    doors: &[DoorCollider],
) -> bool {
    let limit = entry - own.margin;
    if doors.iter().enumerate().any(|(door_index, door)| {
        own.own_door != Some(door_index)
            && own
                .own_leaves
                .is_none_or(|(start, end)| door_index < start || door_index >= end)
            && door.ray_entry(origin, direction, limit).is_some()
    }) {
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
    if doors.iter().enumerate().any(|(index, door)| {
        own.own_door != Some(index)
            && own
                .own_leaves
                .is_none_or(|(start, end)| index < start || index >= end)
            && door.ray_entry(origin, direction, limit).is_some()
    }) {
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
