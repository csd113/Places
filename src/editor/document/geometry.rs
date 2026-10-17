//! Geometry tools edit existing engine records; no editor-only source schema.

use serde_json::{Map, Value};

use super::objects::{array_mut, object_mut, set_number, set_position};
use super::{Document, ObjectId, Transform};
use crate::engine::level::{CeilingProfileDef, RoomDef};

pub(super) fn set_transform(
    object: &mut Map<String, Value>,
    id: ObjectId,
    proposed: Transform,
) -> Result<(), String> {
    let [width, vertical, depth] = proposed.size;
    if matches!(id, ObjectId::Ramp(_)) && vertical.abs() <= 0.001_f32 {
        return Err("A ramp needs a signed, nonzero rise; use a region for a flat surface".into());
    }
    if matches!(id, ObjectId::FloorRegion(_)) && (vertical - 1.0_f32).abs() > 0.0001_f32 {
        return Err("A floor region has no thickness; change its offset Y instead".into());
    }
    let quarters = proposed.yaw_degrees / 90.0_f32;
    if (quarters - quarters.round()).abs() > 0.0001_f32 {
        return Err("Ramps and floor regions use 90 degree footprint rotations".into());
    }
    let turn = quarters.round().rem_euclid(4.0_f32);
    let odd = (turn - 1.0_f32).abs() < 0.0001_f32 || (turn - 3.0_f32).abs() < 0.0001_f32;
    if matches!(id, ObjectId::Ramp(_)) && odd && (width - depth).abs() <= 0.0001_f32 {
        return Err(
            "A square ramp's run is fixed to X; use unequal width/depth before a quarter turn"
                .into(),
        );
    }
    let [x, offset, z] = proposed.position;
    let (actual_width, actual_depth, actual_x, actual_z) = if odd {
        (
            depth,
            width,
            (width - depth).mul_add(0.5_f32, x),
            (depth - width).mul_add(0.5_f32, z),
        )
    } else {
        (width, depth, x, z)
    };
    // Positive yaw rotates +X toward -Z, matching model yaw. Reversing a
    // ramp exchanges its ends: both the signed rise and starting offset move.
    let reverse = (turn - 2.0_f32).abs() < 0.0001_f32
        || (width >= depth && (turn - 1.0_f32).abs() < 0.0001_f32)
        || (width < depth && (turn - 3.0_f32).abs() < 0.0001_f32);
    let ramp_reversed = matches!(id, ObjectId::Ramp(_)) && reverse;
    set_position(
        object,
        [
            actual_x,
            if ramp_reversed {
                offset + vertical
            } else {
                offset
            },
            actual_z,
        ],
        "offset_y",
    );
    set_number(object, "width", actual_width);
    set_number(object, "depth", actual_depth);
    if matches!(id, ObjectId::Ramp(_)) {
        set_number(
            object,
            "rise",
            if ramp_reversed { -vertical } else { vertical },
        );
    }
    Ok(())
}

impl Document {
    /// Creates an upper room at the selected room's eave in one undo step.
    /// The original ceiling remains; connect storeys with an external shaft
    /// or explicit splits of both room footprints. Global walking modifiers
    /// are refused because they would also modify the new storey.
    ///
    /// # Errors
    /// Rejects non-rooms, ambiguous global modifiers and invalid geometry.
    pub fn add_upper_floor(&mut self, id: ObjectId) -> Result<ObjectId, String> {
        let room = selected_room(self, id)?;
        if !room.ceiling.is_flat() && !room.ceiling.is_open() {
            return Err("An upper floor at this eave would intersect the gable roof; create a separate upper room above its ridge".into());
        }
        let footprint = room.bounds();
        if self
            .level()
            .floor_regions
            .iter()
            .any(|region| overlaps(footprint, region.bounds()))
            || self
                .level()
                .ramps
                .iter()
                .any(|ramp| overlaps(footprint, ramp.bounds()))
            || self
                .level()
                .stairs
                .iter()
                .any(|stairs| overlaps(footprint, stairs.bounds()))
        {
            return Err("This footprint has global regions, ramps or stairs that would affect both storeys; create a separate upper floor footprint".into());
        }
        let floor_y = room.eave_y();
        let upper_eave = floor_y + room.height;
        if self.level().rooms.iter().any(|other| {
            overlaps(footprint, other.bounds())
                && other.floor_y < upper_eave - 0.0001_f32
                && other.ridge_y().unwrap_or_else(|| other.eave_y()) > floor_y + 0.0001_f32
        }) {
            return Err("The proposed upper storey intersects an existing room; edit its footprint or elevation explicitly".into());
        }
        let mut edited = self.raw().clone();
        let mut copy = object_mut(&mut edited, id)?.clone();
        set_number(&mut copy, "floor_y", floor_y);
        let rooms = array_mut(&mut edited, "rooms")?;
        let new_id = ObjectId::Room(rooms.len());
        rooms.push(Value::Object(copy));
        self.accept_edit(edited)?;
        Ok(new_id)
    }

    /// Cuts the central third of a room's floor and ceiling using the existing
    /// rectangular room representation. This is one atomic history step.
    ///
    /// # Errors
    /// Rejects the same unsupported/unsafe configurations as explicit cuts.
    pub fn cut_center_opening(&mut self, id: ObjectId) -> Result<Vec<ObjectId>, String> {
        let room = selected_room(self, id)?;
        let width = room.width / 3.0_f32;
        let depth = room.depth / 3.0_f32;
        self.cut_floor_opening(id, [room.x + width, room.z + depth, width, depth])
    }

    /// Replaces one flat/open-ceiling room with up to four disjoint strips
    /// around `[x, z, width, depth]`. The cut removes that room's floor AND
    /// ceiling; other storeys remain. Every retained strip clones the source
    /// record, preserving material, world-space tile frame and unknown data.
    ///
    /// # Errors
    /// Refuses gables, invalid/outside cuts, complete removal, intersecting
    /// ramps/stairs and other same-plane rooms that would fill the opening.
    pub fn cut_floor_opening(
        &mut self,
        id: ObjectId,
        rect: [f32; 4],
    ) -> Result<Vec<ObjectId>, String> {
        let room = selected_room(self, id)?;
        if matches!(
            room.ceiling,
            CeilingProfileDef::Gable {
                ridge: _,
                ridge_rise: _
            }
        ) {
            return Err(
                "Splitting a gable would change its roof profile; use flat or open room sections"
                    .into(),
            );
        }
        let [x, z, width, depth] = rect;
        let (x0, x1, z0, z1) = room.bounds();
        let cut_right = x + width;
        let cut_far = z + depth;
        if rect.iter().any(|value| !value.is_finite())
            || width <= 0.0_f32
            || depth <= 0.0_f32
            || !cut_right.is_finite()
            || !cut_far.is_finite()
            || x < x0
            || z < z0
            || cut_right > x1
            || cut_far > z1
        {
            return Err(
                "Opening must be a finite positive rectangle contained in the selected floor"
                    .into(),
            );
        }
        let cut = (x, cut_right, z, cut_far);
        require_clear_cut(self, id, cut, room.floor_y)?;
        let strips = [
            [x0, z0, x - x0, z1 - z0],
            [cut_right, z0, x1 - cut_right, z1 - z0],
            [x, z0, width, z - z0],
            [x, cut_far, width, z1 - cut_far],
        ];
        let mut edited = self.raw().clone();
        let original = object_mut(&mut edited, id)?.clone();
        let mut replacements = Vec::new();
        for [strip_x, strip_z, strip_width, strip_depth] in strips {
            if strip_width <= 0.0_f32 || strip_depth <= 0.0_f32 {
                continue;
            }
            let mut strip = original.clone();
            set_number(&mut strip, "x", strip_x);
            set_number(&mut strip, "z", strip_z);
            set_number(&mut strip, "width", strip_width);
            set_number(&mut strip, "depth", strip_depth);
            replacements.push(Value::Object(strip));
        }
        if replacements.is_empty() {
            return Err(
                "An opening must leave part of the floor; use Delete to remove a room".into(),
            );
        }
        let ObjectId::Room(index) = id else {
            return Err("Select a room floor".into());
        };
        let count = replacements.len();
        let rooms = array_mut(&mut edited, "rooms")?;
        drop(rooms.splice(index..index.saturating_add(1), replacements));
        self.accept_edit(edited)?;
        Ok((index..index.saturating_add(count))
            .map(ObjectId::Room)
            .collect())
    }

    /// Applies a previewed correction only to the exact scene it was derived
    /// from. Validation completes before either the document or history moves.
    ///
    /// # Errors
    /// Rejects stale previews or any invalid proposed scene without changes.
    pub fn apply_checked_edit(&mut self, expected: &Value, edited: Value) -> Result<(), String> {
        if expected != self.raw() {
            return Err(
                "The scene changed after this correction was previewed; analyze it again".into(),
            );
        }
        self.accept_edit(edited)
    }
}

fn selected_room(document: &Document, id: ObjectId) -> Result<&RoomDef, String> {
    let ObjectId::Room(index) = id else {
        return Err("Select a room floor".into());
    };
    document
        .level()
        .rooms
        .get(index)
        .ok_or_else(|| "The selected floor no longer exists".into())
}

fn require_clear_cut(
    document: &Document,
    id: ObjectId,
    cut: (f32, f32, f32, f32),
    floor_y: f32,
) -> Result<(), String> {
    for (index, room) in document.level().rooms.iter().enumerate() {
        if id != ObjectId::Room(index)
            && (room.floor_y - floor_y).abs() <= 0.0001_f32
            && overlaps(cut, room.bounds())
        {
            return Err("Another room on the same floor fills this opening; split that room explicitly first".into());
        }
    }
    if document
        .level()
        .ramps
        .iter()
        .any(|ramp| overlaps(cut, ramp.bounds()))
        || document
            .level()
            .stairs
            .iter()
            .any(|stairs| overlaps(cut, stairs.bounds()))
    {
        return Err(
            "Opening intersects a ramp or staircase; move it first so its support is preserved"
                .into(),
        );
    }
    Ok(())
}

fn overlaps(a: (f32, f32, f32, f32), b: (f32, f32, f32, f32)) -> bool {
    let (ax0, ax1, az0, az1) = a;
    let (bx0, bx1, bz0, bz1) = b;
    ax0 < bx1 && ax1 > bx0 && az0 < bz1 && az1 > bz0
}
