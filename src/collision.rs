use glam::{Vec2, Vec3};

use crate::package::binary::{Reader, Writer};

pub const PLAYER_RADIUS: f32 = 0.30;
pub const PLAYER_HEIGHT: f32 = 1.8;

/// Crouched body height: exactly half the standing height.
///
/// The crouch stance shrinks the collision cylinder to this height and its eye
/// offset proportionally (see `game::CROUCH_EYE_HEIGHT`), so a crouched player
/// can pass under geometry a standing player cannot.
pub const CROUCH_HEIGHT: f32 = PLAYER_HEIGHT * 0.5;

/// How far a box's face must overlap the body before it counts as blocking.
///
/// A box whose underside is exactly at head height is the ceiling the player
/// was just clamped under, not a wall to push against; the tolerance keeps the
/// vertical clamp and the horizontal band from fighting at the contact plane.
pub const CONTACT_EPS: f32 = 1e-4;

/// Largest vertical discontinuity the player walks up or down without
/// stopping, in metres.
///
/// The controller has no falling physics: a rise or drop larger than this is
/// refused (the player simply cannot walk off a cliff or through a deep
/// recess wall), and anything smaller is stepped through instantly. Floor
/// regions whose height differs by more than this also emit a solid rim, so
/// the rendered transition face and collision agree.
pub const PLAYER_STEP_HEIGHT: f32 = 0.4;

/// Vertical tolerance within which two floor heights count as the same
/// surface, in metres.
pub const STEP_EPS: f32 = 1e-3;

/// Axis-aligned horizontal wall bounding box in 3D space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WallAabb {
    pub min_x: f32,
    pub max_x: f32,
    pub min_y: f32,
    pub max_y: f32,
    pub min_z: f32,
    pub max_z: f32,
    /// Extra headroom below the box's top that the player may walk under
    /// without colliding, in metres.
    ///
    /// Zero for every real wall and solid: a wall flush with the floor must
    /// block, and a wall taller than the walkable step must block. A floor
    /// region's *rim* sets it to [`PLAYER_STEP_HEIGHT`]: the rim exists to stop
    /// a step the controller could not otherwise take, so a player whose feet
    /// are already within one walkable step of the rim's top (on a ramp or a
    /// staircase arriving beside it) must pass rather than snag on it.
    pub step_up: f32,
}

impl WallAabb {
    #[must_use]
    pub fn new(x: f32, z: f32, width: f32, depth: f32) -> Self {
        Self::with_y(
            x,
            0.0,
            z,
            width,
            crate::level::DEFAULT_CEILING_HEIGHT_M,
            depth,
        )
    }

    /// The same box, treating its top as a walkable step instead of a wall.
    ///
    /// Used for floor-region rims: a rim blocks a cliff, never a step the
    /// player's own feet could take.
    #[must_use]
    pub const fn allowing_step(mut self) -> Self {
        self.step_up = PLAYER_STEP_HEIGHT;
        self
    }

    #[must_use]
    pub fn with_y(x: f32, y: f32, z: f32, width: f32, height: f32, depth: f32) -> Self {
        let (min_x, max_x) = if width >= 0.0 {
            (x, x + width)
        } else {
            (x + width, x)
        };
        let (min_y, max_y) = if height >= 0.0 {
            (y, y + height)
        } else {
            (y + height, y)
        };
        let (min_z, max_z) = if depth >= 0.0 {
            (z, z + depth)
        } else {
            (z + depth, z)
        };
        Self {
            min_x,
            max_x,
            min_y,
            max_y,
            min_z,
            max_z,
            step_up: 0.0,
        }
    }

    /// Checks if this box intersects the player's body, whose feet stand at
    /// `foot_y` (world Y of the walkable floor under the player).
    ///
    /// A wall flush with the floor (`max_y == foot_y`) is *not* solid: that is
    /// what makes a recessed region's rim one-way, blocking a player standing
    /// inside the depression while letting a player on the upper floor walk
    /// right up to the edge. A box starting at or above head height never
    /// blocks, so door headers stay passable. [`WallAabb::step_up`] raises the
    /// top by the walkable step for rims, so a rim never blocks a step the
    /// controller could take anyway.
    #[must_use]
    pub fn intersects_player_y(&self, foot_y: f32) -> bool {
        self.max_y > foot_y + self.step_up + STEP_EPS && self.min_y < foot_y + PLAYER_HEIGHT
    }

    /// The stance-aware blocking rule: true when a body whose feet stand at
    /// `foot_y` with height `body_height` overlaps this box.
    ///
    /// Identical to [`Self::intersects_player_y`] except for the body height, so
    /// a crouched player passes under a header a standing player hits. A box
    /// whose underside sits exactly at the head does not block: the vertical
    /// pass clamps the head to that plane, and re-reading it as a wall would
    /// push the player sideways out of the opening.
    #[must_use]
    pub fn blocks_body(&self, foot_y: f32, body_height: f32) -> bool {
        self.max_y > foot_y + self.step_up + STEP_EPS
            && self.min_y + CONTACT_EPS < foot_y + body_height
    }

    /// True when the horizontal disc of radius `radius` centred at `(x, z)`
    /// touches this box's footprint.
    #[must_use]
    pub fn overlaps_disc(&self, x: f32, z: f32, radius: f32) -> bool {
        let closest_x = x.clamp(self.min_x, self.max_x);
        let closest_z = z.clamp(self.min_z, self.max_z);
        let dx = x - closest_x;
        let dz = z - closest_z;
        dx.mul_add(dx, dz * dz) < radius * radius
    }

    /// True when the point `(x, z)` lies over this box's footprint.
    ///
    /// Standing on a box top requires the centre of mass over it, not merely
    /// the disc touching: a player falling past a 9 cm ledge lip must fall,
    /// and a player who has walked half off a prop must fall too.
    #[must_use]
    pub fn supports_center(&self, x: f32, z: f32) -> bool {
        x >= self.min_x && x <= self.max_x && z >= self.min_z && z <= self.max_z
    }

    /// Writes this box in the compiled collision record's component order:
    /// `min_x, min_y, min_z, max_x, max_y, max_z, step_up`.
    pub(crate) fn write_compiled(&self, writer: &mut Writer) {
        writer.f32(self.min_x);
        writer.f32(self.min_y);
        writer.f32(self.min_z);
        writer.f32(self.max_x);
        writer.f32(self.max_y);
        writer.f32(self.max_z);
        writer.f32(self.step_up);
    }

    /// Reads one box from a compiled collision record.
    ///
    /// # Errors
    ///
    /// Returns a named error when the record is truncated, a component is not
    /// finite, or a minimum bound exceeds its maximum.
    pub(crate) fn read_compiled(reader: &mut Reader<'_>) -> Result<Self, String> {
        let min_x = reader.f32()?;
        let min_y = reader.f32()?;
        let min_z = reader.f32()?;
        let max_x = reader.f32()?;
        let max_y = reader.f32()?;
        let max_z = reader.f32()?;
        let step_up = reader.f32()?;
        if ![min_x, min_y, min_z, max_x, max_y, max_z, step_up]
            .iter()
            .all(|value| value.is_finite())
        {
            return Err("wall box has a non-finite value".to_string());
        }
        if min_x > max_x || min_y > max_y || min_z > max_z {
            return Err("wall box bounds are inverted".to_string());
        }
        Ok(Self {
            min_x,
            max_x,
            min_y,
            max_y,
            min_z,
            max_z,
            step_up,
        })
    }
}

/// The highest walkable top under `(x, z)`: a box whose footprint contains the
/// centre and whose top is not more than one walkable step above `max_top`.
///
/// This is the extra support the vertical pass lands on, alongside the level's
/// walkable floor, and is what makes a solid prop or a wall top landable. The
/// [`PLAYER_STEP_HEIGHT`] allowance matches the floor's: a descending player
/// whose feet are within one step of a prop top lands *on* the top (a bounded
/// step-up of at most one step) instead of sinking past the side. The centre
/// containment keeps a thin ledge lip from catching a falling player.
#[must_use]
pub fn highest_support_top(x: f32, z: f32, max_top: f32, walls: &[WallAabb]) -> Option<f32> {
    let mut highest: Option<f32> = None;
    for wall in walls {
        if !wall.supports_center(x, z) || wall.max_y > max_top + PLAYER_STEP_HEIGHT + STEP_EPS {
            continue;
        }
        highest = Some(highest.map_or(wall.max_y, |top| top.max(wall.max_y)));
    }
    highest
}

/// [`highest_support_top`] through a spatial index.
#[must_use]
pub fn highest_support_top_indexed(
    index: &crate::collision_index::CollisionIndex,
    x: f32,
    z: f32,
    max_top: f32,
    walls: &[WallAabb],
) -> Option<f32> {
    let mut highest: Option<f32> = None;
    index.for_each_point(x, z, walls, |wall| {
        if !wall.supports_center(x, z) || wall.max_y > max_top + PLAYER_STEP_HEIGHT + STEP_EPS {
            return;
        }
        highest = Some(highest.map_or(wall.max_y, |top| top.max(wall.max_y)));
    });
    highest
}

/// The lowest box underside above `above_y` whose footprint touches the disc
/// `(x, z, radius)`.
///
/// This is head collision against frames, headers and prop undersides; the room
/// ceiling is added by the caller. A box that starts at or below the feet is
/// the surface being stood next to, not an overhead.
#[must_use]
pub fn lowest_underside(
    x: f32,
    z: f32,
    radius: f32,
    above_y: f32,
    walls: &[WallAabb],
) -> Option<f32> {
    let mut lowest: Option<f32> = None;
    for wall in walls {
        if !wall.overlaps_disc(x, z, radius) || wall.min_y <= above_y + STEP_EPS {
            continue;
        }
        lowest = Some(lowest.map_or(wall.min_y, |bottom| bottom.min(wall.min_y)));
    }
    lowest
}

/// [`lowest_underside`] through a spatial index.
#[must_use]
pub fn lowest_underside_indexed(
    index: &crate::collision_index::CollisionIndex,
    x: f32,
    z: f32,
    radius: f32,
    above_y: f32,
    walls: &[WallAabb],
) -> Option<f32> {
    let mut lowest: Option<f32> = None;
    index.for_each_disc(x, z, radius, walls, |wall| {
        if !wall.overlaps_disc(x, z, radius) || wall.min_y <= above_y + STEP_EPS {
            return;
        }
        lowest = Some(lowest.map_or(wall.min_y, |bottom| bottom.min(wall.min_y)));
    });
    lowest
}

/// A door leaf's collision box at its current opening angle.
///
/// The leaf is an oriented box: a rectangle in the XZ plane from the hinge
/// along the leaf direction, with a vertical span. The collider is rebuilt from
/// the runtime angle every time a door moves, so the physical slab and the
/// drawn slab always share one transform.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DoorCollider {
    /// World X of the hinge edge, at the leaf's bottom.
    pub hinge_x: f32,
    /// World Y of the leaf's bottom.
    pub hinge_y: f32,
    /// World Z of the hinge edge.
    pub hinge_z: f32,
    /// Unit direction from the hinge along the leaf, in the XZ plane.
    pub dir_x: f32,
    /// Unit direction from the hinge along the leaf, in the XZ plane.
    pub dir_z: f32,
    /// Leaf width from the hinge to the latch edge.
    pub width: f32,
    /// Leaf thickness.
    pub thickness: f32,
    /// Leaf height above [`Self::hinge_y`].
    pub height: f32,
}

impl DoorCollider {
    /// Builds the collider for a hinge pose and leaf direction.
    ///
    /// `hinge` is the leaf's bottom hinge edge, `direction` the unit `(x, z)`
    /// from the hinge along the leaf. A zero-length direction is not a pose; it
    /// falls back to `+X` so a malformed call cannot produce a NaN normal.
    #[must_use]
    pub fn from_pose(
        hinge: [f32; 3],
        direction: [f32; 2],
        width: f32,
        thickness: f32,
        height: f32,
    ) -> Self {
        let [dir_x, dir_z] = direction;
        let (dir_x, dir_z) = if dir_x.is_finite() && dir_z.is_finite() {
            let length = dir_x.hypot(dir_z);
            if length > 1e-6 {
                (dir_x / length, dir_z / length)
            } else {
                (1.0, 0.0)
            }
        } else {
            (1.0, 0.0)
        };
        Self {
            hinge_x: hinge[0],
            hinge_y: hinge[1],
            hinge_z: hinge[2],
            dir_x,
            dir_z,
            width,
            thickness,
            height,
        }
    }

    /// True when the leaf's vertical span overlaps a body band.
    #[must_use]
    pub fn overlaps_body_y(&self, foot_y: f32, body_height: f32) -> bool {
        self.hinge_y + self.height > foot_y + STEP_EPS
            && self.hinge_y + CONTACT_EPS < foot_y + body_height
    }

    /// Local `(u, v)` of a world point: `u` along the leaf, `v` across it.
    #[must_use]
    fn local(&self, x: f32, z: f32) -> (f32, f32) {
        let dx = x - self.hinge_x;
        let dz = z - self.hinge_z;
        (
            dx.mul_add(self.dir_x, dz * self.dir_z),
            dx.mul_add(-self.dir_z, dz * self.dir_x),
        )
    }

    /// World `(x, z)` of a local `(u, v)` point.
    #[must_use]
    fn world(&self, u: f32, v: f32) -> (f32, f32) {
        (
            (-self.dir_z).mul_add(v, self.dir_x.mul_add(u, self.hinge_x)),
            self.dir_x.mul_add(v, self.dir_z.mul_add(u, self.hinge_z)),
        )
    }

    /// Depenetrates a body disc from the leaf, or returns `None` when clear.
    ///
    /// The disc is resolved in the leaf's local frame: the closest point on the
    /// box pushes the centre out along the contact normal, and a centre inside
    /// the box exits through the shallowest face. This is the same rule
    /// [`WallAabb`] uses, expressed for an oriented rectangle.
    #[must_use]
    pub fn depenetrate(&self, x: f32, z: f32, radius: f32) -> Option<(f32, f32)> {
        let half_thickness = self.thickness * 0.5;
        let (u, v) = self.local(x, z);
        let closest_u = u.clamp(0.0, self.width);
        let closest_v = v.clamp(-half_thickness, half_thickness);
        let du = u - closest_u;
        let dv = v - closest_v;
        let dist_sq = du.mul_add(du, dv * dv);
        if dist_sq >= radius * radius {
            return None;
        }
        if dist_sq > 1e-6 {
            let dist = dist_sq.sqrt();
            let normal_u = du / dist;
            let normal_v = dv / dist;
            let penetration = radius - dist;
            return Some(self.world(
                normal_u.mul_add(penetration, u),
                normal_v.mul_add(penetration, v),
            ));
        }
        // Centre inside the box: leave through the nearest face.
        let d_start = u.abs();
        let d_end = (self.width - u).abs();
        let d_side = (v + half_thickness).abs();
        let d_other = (half_thickness - v).abs();
        let min_d = d_start.min(d_end).min(d_side).min(d_other);
        if (min_d - d_start).abs() < 1e-5 {
            Some(self.world(-radius, v))
        } else if (min_d - d_end).abs() < 1e-5 {
            Some(self.world(self.width + radius, v))
        } else if (min_d - d_side).abs() < 1e-5 {
            Some(self.world(u, -half_thickness - radius))
        } else {
            Some(self.world(u, half_thickness + radius))
        }
    }

    /// A world point on one face of the leaf.
    ///
    /// `t` runs `0`..`1` from the hinge to the latch edge and `side` is `-1`
    /// for one face and `+1` for the other.
    #[must_use]
    pub fn point_at(&self, t: f32, side: f32) -> (f32, f32) {
        let u = self.width * t.clamp(0.0, 1.0);
        let v = self.thickness * 0.5 * side;
        self.world(u, v)
    }

    /// True when the leaf's footprint touches a disc (`x`, `z`, `radius`).
    #[must_use]
    pub fn overlaps_disc(&self, x: f32, z: f32, radius: f32) -> bool {
        let half_thickness = self.thickness * 0.5;
        let (u, v) = self.local(x, z);
        let closest_u = u.clamp(0.0, self.width);
        let closest_v = v.clamp(-half_thickness, half_thickness);
        let du = u - closest_u;
        let dv = v - closest_v;
        du.mul_add(du, dv * dv) < radius * radius
    }

    /// True when a world point lies inside the leaf.
    #[must_use]
    pub fn contains_point(&self, point_x: f32, point_y: f32, point_z: f32) -> bool {
        if point_y < self.hinge_y || point_y > self.hinge_y + self.height {
            return false;
        }
        let half_thickness = self.thickness * 0.5;
        let (u, v) = self.local(point_x, point_z);
        u >= 0.0 && u <= self.width && v >= -half_thickness && v <= half_thickness
    }

    /// Entry distance of a ray into the leaf, or `None` when it misses.
    #[must_use]
    pub fn ray_entry(&self, origin: Vec3, direction: Vec3, max_distance: f32) -> Option<f32> {
        if !origin.is_finite() || !direction.is_finite() {
            return None;
        }
        let half_thickness = self.thickness * 0.5;
        // The ray in the leaf's local frame.
        let o = self.local(origin.x, origin.z);
        let d = (
            direction.x.mul_add(self.dir_x, direction.z * self.dir_z),
            direction.x.mul_add(-self.dir_z, direction.z * self.dir_x),
        );
        let mut t_enter = 0.0_f32;
        let mut t_exit = f32::INFINITY;
        for (o, d, lo, hi) in [
            (o.0, d.0, 0.0, self.width),
            (o.1, d.1, -half_thickness, half_thickness),
        ] {
            let (near, far) = slab_axis(o, d, lo, hi)?;
            t_enter = t_enter.max(near);
            t_exit = t_exit.min(far);
            if t_enter > t_exit {
                return None;
            }
        }
        let (near, far) = slab_axis(
            origin.y,
            direction.y,
            self.hinge_y,
            self.hinge_y + self.height,
        )?;
        t_enter = t_enter.max(near);
        t_exit = t_exit.min(far);
        if t_enter > t_exit {
            return None;
        }
        (t_enter <= max_distance).then_some(t_enter)
    }
}

/// The nearest door-leaf entry along a ray, if any is within `max_distance`.
#[must_use]
pub fn nearest_door_entry(
    doors: &[DoorCollider],
    origin: Vec3,
    direction: Vec3,
    max_distance: f32,
) -> Option<(usize, f32)> {
    let mut best: Option<(usize, f32)> = None;
    for (index, door) in doors.iter().enumerate() {
        let Some(entry) = door.ray_entry(origin, direction, max_distance) else {
            continue;
        };
        if best.is_none_or(|(_, best_entry)| entry < best_entry) {
            best = Some((index, entry));
        }
    }
    best
}

/// Resolves a body disc against walls **and** door leaves.
///
/// The wall pass keeps its indexed path; doors are few and linearly scanned.
/// Both are interleaved for the same four rounds so a corner where a wall and
/// a moving leaf meet cannot leave the body wedged between them.
#[must_use]
pub fn resolve_player_collision_with_doors(
    index: &crate::collision_index::CollisionIndex,
    pos: Vec2,
    radius: f32,
    foot_y: f32,
    body_height: f32,
    walls: &[WallAabb],
    doors: &[DoorCollider],
) -> Vec2 {
    let mut pos = pos;
    for _ in 0..4 {
        let mut collided = false;
        index.for_each_disc(pos.x, pos.y, radius, walls, |wall| {
            if !wall.blocks_body(foot_y, body_height) {
                return;
            }
            let Some(next) = depenetrate(pos, radius, wall) else {
                return;
            };
            pos = next;
            collided = true;
        });
        for door in doors {
            if !door.overlaps_body_y(foot_y, body_height) {
                continue;
            }
            if let Some((x, z)) = door.depenetrate(pos.x, pos.y, radius) {
                pos = Vec2::new(x, z);
                collided = true;
            }
        }
        if !collided {
            break;
        }
    }
    pos
}

/// The lowest door-leaf underside above `above_y` whose footprint touches the
/// disc `(x, z, radius)`.
#[must_use]
pub fn lowest_door_underside(
    doors: &[DoorCollider],
    x: f32,
    z: f32,
    radius: f32,
    above_y: f32,
) -> Option<f32> {
    let mut lowest: Option<f32> = None;
    for door in doors {
        if door.hinge_y <= above_y + STEP_EPS || !door.overlaps_disc(x, z, radius) {
            continue;
        }
        lowest = Some(lowest.map_or(door.hinge_y, |bottom| bottom.min(door.hinge_y)));
    }
    lowest
}

/// One axis of the slab test: the ray's `[t_enter, t_exit]` span on `[lo, hi]`.
///
/// A direction component within [`f32::EPSILON`] of zero is treated as
/// parallel: the ray can only intersect when its origin already lies inside the
/// slab, which the caller expresses as an infinite span. Non-finite origins and
/// directions are rejected by the public entry points before this runs.
fn slab_axis(o: f32, d: f32, lo: f32, hi: f32) -> Option<(f32, f32)> {
    if d.abs() <= f32::EPSILON {
        return (o >= lo && o <= hi).then_some((f32::NEG_INFINITY, f32::INFINITY));
    }
    let inv = 1.0 / d;
    let a = (lo - o) * inv;
    let b = (hi - o) * inv;
    Some(if a <= b { (a, b) } else { (b, a) })
}

/// Entry distance of a ray into an axis-aligned box, or `None` when it misses.
///
/// The ray is `origin + t * direction` with `t >= 0`; the returned `t` is the
/// first intersection with `[min, max]`, and `0.0` when the origin is already
/// inside. Non-finite inputs miss. This is the shared primitive behind
/// interaction targeting, its occlusion test and the swept area-trigger test.
#[must_use]
pub fn ray_aabb_entry(origin: Vec3, direction: Vec3, min: [f32; 3], max: [f32; 3]) -> Option<f32> {
    if !origin.is_finite() || !direction.is_finite() {
        return None;
    }
    let [min_x, min_y, min_z] = min;
    let [max_x, max_y, max_z] = max;
    let mut t_enter = 0.0_f32;
    let mut t_exit = f32::INFINITY;
    for (o, d, lo, hi) in [
        (origin.x, direction.x, min_x, max_x),
        (origin.y, direction.y, min_y, max_y),
        (origin.z, direction.z, min_z, max_z),
    ] {
        let (near, far) = slab_axis(o, d, lo, hi)?;
        t_enter = t_enter.max(near);
        t_exit = t_exit.min(far);
        if t_enter > t_exit {
            return None;
        }
    }
    Some(t_enter)
}

/// True when the swept segment `from -> to` touches the axis-aligned box.
///
/// This is the crossing test for area triggers: a player falling fast can move
/// several metres in one frame, so testing only the endpoint against a thin
/// volume would skip a real crossing. A degenerate segment is a point test.
#[must_use]
pub fn segment_overlaps_aabb(from: Vec3, to: Vec3, min: [f32; 3], max: [f32; 3]) -> bool {
    // Vec3 subtraction is component-wise bounded float arithmetic; the lint
    // cannot see that through the operator impl.
    #[allow(clippy::arithmetic_side_effects)]
    let delta = to - from;
    if !delta.is_finite() {
        return false;
    }
    ray_aabb_entry(from, delta, min, max).is_some_and(|t| t <= 1.0)
}

/// Resolves horizontal collision for a body of the given height.
///
/// See [`resolve_player_collision`]; this is the stance-aware entry point the
/// controller uses so a crouched body passes under geometry a standing one
/// cannot.
#[must_use]
pub fn resolve_player_collision_for_body(
    pos: Vec2,
    radius: f32,
    foot_y: f32,
    body_height: f32,
    walls: &[WallAabb],
) -> Vec2 {
    resolve_with_band(pos, radius, walls, |wall| {
        wall.blocks_body(foot_y, body_height)
    })
}

/// [`resolve_player_collision_for_body`] through a spatial index.
///
/// The index only decides *which* boxes are examined; the depenetration rule
/// itself is [`resolve_with_band`]'s, so an indexed resolve is identical to the
/// linear one for every position, radius and foot height.
#[must_use]
pub fn resolve_player_collision_for_body_indexed(
    index: &crate::collision_index::CollisionIndex,
    pos: Vec2,
    radius: f32,
    foot_y: f32,
    body_height: f32,
    walls: &[WallAabb],
) -> Vec2 {
    let mut pos = pos;
    for _ in 0..4 {
        let mut collided = false;
        index.for_each_disc(pos.x, pos.y, radius, walls, |wall| {
            if !wall.blocks_body(foot_y, body_height) {
                return;
            }
            let Some(next) = depenetrate(pos, radius, wall) else {
                return;
            };
            pos = next;
            collided = true;
        });
        if !collided {
            break;
        }
    }
    pos
}

/// Resolves collision between player horizontal position and wall bounding boxes.
/// Allows smooth sliding along walls and resolves corner collisions.
///
/// `foot_y` is the world Y of the floor the player is standing on; only boxes
/// overlapping the player's vertical span `[foot_y, foot_y + PLAYER_HEIGHT]`
/// are considered, which is what keeps collision on an elevated floor working
/// exactly like collision on the global floor.
#[must_use]
pub fn resolve_player_collision(pos: Vec2, radius: f32, foot_y: f32, walls: &[WallAabb]) -> Vec2 {
    resolve_player_collision_for_body(pos, radius, foot_y, PLAYER_HEIGHT, walls)
}

/// The shared depenetration loop behind both public resolvers.
fn resolve_with_band(
    mut pos: Vec2,
    radius: f32,
    walls: &[WallAabb],
    blocks: impl Fn(&WallAabb) -> bool,
) -> Vec2 {
    for _ in 0..4 {
        let mut collided = false;
        for wall in walls {
            if !blocks(wall) {
                continue;
            }
            if let Some(next) = depenetrate(pos, radius, wall) {
                pos = next;
                collided = true;
            }
        }
        if !collided {
            break;
        }
    }
    pos
}

/// One box's depenetration of a body disc, or `None` when it does not touch.
///
/// This is the single implementation of the contact rule the linear and
/// indexed resolvers both use, so a widened candidate set can never change the
/// resolved position.
fn depenetrate(pos: Vec2, radius: f32, wall: &WallAabb) -> Option<Vec2> {
    let closest_x = pos.x.clamp(wall.min_x, wall.max_x);
    let closest_z = pos.y.clamp(wall.min_z, wall.max_z);
    // Component-wise subtraction rather than the glam operator: the scalar
    // operations cannot overflow and are exactly what the operator would do.
    let diff = Vec2::new(pos.x - closest_x, pos.y - closest_z);
    let dist_sq = diff.length_squared();
    if dist_sq >= radius * radius {
        return None;
    }
    if dist_sq > 1e-6 {
        let dist = dist_sq.sqrt();
        let normal = Vec2::new(diff.x / dist, diff.y / dist);
        let penetration = radius - dist;
        return Some(Vec2::new(
            normal.x.mul_add(penetration, pos.x),
            normal.y.mul_add(penetration, pos.y),
        ));
    }
    // Centre is inside or exactly on the bounding box boundary: push out of
    // the nearest face.
    let d_left = (pos.x - wall.min_x).abs();
    let d_right = (wall.max_x - pos.x).abs();
    let d_near = (pos.y - wall.min_z).abs();
    let d_far = (wall.max_z - pos.y).abs();
    let min_d = d_left.min(d_right).min(d_near).min(d_far);
    if (min_d - d_left).abs() < 1e-5 {
        Some(Vec2::new(wall.min_x - radius, pos.y))
    } else if (min_d - d_right).abs() < 1e-5 {
        Some(Vec2::new(wall.max_x + radius, pos.y))
    } else if (min_d - d_near).abs() < 1e-5 {
        Some(Vec2::new(pos.x, wall.min_z - radius))
    } else {
        Some(Vec2::new(pos.x, wall.max_z + radius))
    }
}

#[cfg(test)]
mod tests;
