use glam::Vec2;

pub const PLAYER_RADIUS: f32 = 0.30;
pub const PLAYER_HEIGHT: f32 = 1.8;

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

    /// Checks if a 2D circle intersects this wall AABB at the player's foot Y.
    #[must_use]
    pub fn intersects_circle(&self, center: Vec2, radius: f32, foot_y: f32) -> bool {
        if !self.intersects_player_y(foot_y) {
            return false;
        }
        let closest_x = center.x.clamp(self.min_x, self.max_x);
        let closest_z = center.y.clamp(self.min_z, self.max_z);
        let diff_x = center.x - closest_x;
        let diff_z = center.y - closest_z;
        diff_z.mul_add(diff_z, diff_x * diff_x) < (radius * radius)
    }
}

/// Resolves collision between player horizontal position and wall bounding boxes.
/// Allows smooth sliding along walls and resolves corner collisions.
///
/// `foot_y` is the world Y of the floor the player is standing on; only boxes
/// overlapping the player's vertical span `[foot_y, foot_y + PLAYER_HEIGHT]`
/// are considered, which is what keeps collision on an elevated floor working
/// exactly like collision on the global floor.
#[must_use]
pub fn resolve_player_collision(
    mut pos: Vec2,
    radius: f32,
    foot_y: f32,
    walls: &[WallAabb],
) -> Vec2 {
    for _ in 0..4 {
        let mut collided = false;
        for wall in walls {
            if !wall.intersects_player_y(foot_y) {
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
