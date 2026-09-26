use glam::{Vec2, Vec3};

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
}

/// The highest walkable top under `(x, z)`: a box whose footprint contains the
/// centre and whose top is not above `max_top`.
///
/// This is the extra support the vertical pass lands on, alongside the level's
/// walkable floor, and is what makes a solid prop or a wall top landable. The
/// centre containment keeps a thin ledge lip from catching a falling player.
#[must_use]
pub fn highest_support_top(x: f32, z: f32, max_top: f32, walls: &[WallAabb]) -> Option<f32> {
    let mut highest: Option<f32> = None;
    for wall in walls {
        if !wall.supports_center(x, z) || wall.max_y > max_top + STEP_EPS {
            continue;
        }
        highest = Some(highest.map_or(wall.max_y, |top| top.max(wall.max_y)));
    }
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
            let closest_x = pos.x.clamp(wall.min_x, wall.max_x);
            let closest_z = pos.y.clamp(wall.min_z, wall.max_z);
            // Component-wise subtraction rather than the glam operator: the
            // scalar operations cannot overflow and are exactly what the
            // operator would do.
            let diff = Vec2::new(pos.x - closest_x, pos.y - closest_z);
            let dist_sq = diff.length_squared();

            if dist_sq < radius * radius {
                collided = true;
                if dist_sq > 1e-6 {
                    let dist = dist_sq.sqrt();
                    let normal = Vec2::new(diff.x / dist, diff.y / dist);
                    let penetration = radius - dist;
                    pos.x = normal.x.mul_add(penetration, pos.x);
                    pos.y = normal.y.mul_add(penetration, pos.y);
                } else {
                    // Center is inside or exactly on the bounding box boundary.
                    let d_left = (pos.x - wall.min_x).abs();
                    let d_right = (wall.max_x - pos.x).abs();
                    let d_near = (pos.y - wall.min_z).abs();
                    let d_far = (wall.max_z - pos.y).abs();

                    let min_d = d_left.min(d_right).min(d_near).min(d_far);
                    if (min_d - d_left).abs() < 1e-5 {
                        pos.x = wall.min_x - radius;
                    } else if (min_d - d_right).abs() < 1e-5 {
                        pos.x = wall.max_x + radius;
                    } else if (min_d - d_near).abs() < 1e-5 {
                        pos.y = wall.min_z - radius;
                    } else {
                        pos.y = wall.max_z + radius;
                    }
                }
            }
        }
        if !collided {
            break;
        }
    }
    pos
}

#[cfg(test)]
mod tests;
