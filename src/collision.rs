use glam::Vec2;

pub const PLAYER_RADIUS: f32 = 0.30;
pub const PLAYER_HEIGHT: f32 = 1.8;

/// Axis-aligned horizontal wall bounding box in 3D space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WallAabb {
    pub min_x: f32,
    pub max_x: f32,
    pub min_y: f32,
    pub max_y: f32,
    pub min_z: f32,
    pub max_z: f32,
}

impl WallAabb {
    #[must_use]
    pub fn new(x: f32, z: f32, width: f32, depth: f32) -> Self {
        Self::with_y(x, 0.0, z, width, 3.5, depth)
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
        }
    }

    /// Checks if this wall intersects the player vertically.
    #[must_use]
    pub fn intersects_player_y(&self) -> bool {
        self.max_y > 0.0 && self.min_y < PLAYER_HEIGHT
    }

    /// Checks if a 2D circle intersects this wall AABB.
    #[must_use]
    pub fn intersects_circle(&self, center: Vec2, radius: f32) -> bool {
        if !self.intersects_player_y() {
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
#[must_use]
pub fn resolve_player_collision(mut pos: Vec2, radius: f32, walls: &[WallAabb]) -> Vec2 {
    for _ in 0..4 {
        let mut collided = false;
        for wall in walls {
            if !wall.intersects_player_y() {
                continue;
            }
            let closest_x = pos.x.clamp(wall.min_x, wall.max_x);
            let closest_z = pos.y.clamp(wall.min_z, wall.max_z);
            let diff = pos - Vec2::new(closest_x, closest_z);
            let dist_sq = diff.length_squared();

            if dist_sq < radius * radius {
                collided = true;
                if dist_sq > 1e-6 {
                    let dist = dist_sq.sqrt();
                    let normal = diff / dist;
                    let penetration = radius - dist;
                    pos += normal * penetration;
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
mod tests {
    use super::*;
    use crate::level::LevelDef;
    use crate::test_support::assert_exact;

    /// Solid props block with their catalogue-sized box; non-solid props
    /// (rugs, plants, lamps, TVs, cardboard boxes) never affect collision.
    #[test]
    fn test_showcase_level_collision_matches_the_solid_flags() {
        let content = std::fs::read_to_string("assets/levels/prop_showcase.json")
            .expect("the prop showcase level ships with the game");
        let level = LevelDef::from_json(&content).expect("showcase level parses");
        let aabbs = level.collision_aabbs();

        let solid_props = level.props.iter().filter(|prop| prop.solid).count();
        let non_solid = level.props.iter().filter(|prop| !prop.solid).count();
        assert!(solid_props >= 12, "the showcase places most props as solid");
        assert!(non_solid >= 4, "rug/plant/lamp/tv stay passable");
        assert!(
            aabbs.len() > solid_props,
            "prop boxes are added alongside the walls"
        );

        // A sunk prop still blocks: the player cannot stand inside the crate
        // that is deliberately sunk into the floor.
        let sunk = level
            .props
            .iter()
            .find(|prop| prop.model == "core:crate" && prop.y < 0.0)
            .expect("showcase keeps one crate sunk into the floor");
        let resolved = resolve_player_collision(Vec2::new(sunk.x, sunk.z), PLAYER_RADIUS, &aabbs);
        assert!(
            (resolved - Vec2::new(sunk.x, sunk.z)).length() > 1e-3,
            "the sunk solid crate must push the player out"
        );

        // A non-solid prop never pushes the player out of its own centre. Only
        // props standing clear of walls are checked here: the rug sits under
        // the solid coffee table and the TV hugs the back wall on purpose.
        for model in ["core:lamp", "core:plant"] {
            let prop = level
                .props
                .iter()
                .find(|prop| prop.model == model)
                .unwrap_or_else(|| panic!("showcase places a {model}"));
            let resolved =
                resolve_player_collision(Vec2::new(prop.x, prop.z), PLAYER_RADIUS, &aabbs);
            assert!(
                (resolved - Vec2::new(prop.x, prop.z)).length() < 1e-3,
                "{model} must stay passable"
            );
        }
    }

    #[test]
    fn test_wall_aabb_creation() {
        let wall = WallAabb::new(2.0, -5.0, 4.0, 1.0);
        assert_exact(wall.min_x, 2.0);
        assert_exact(wall.max_x, 6.0);
        assert_exact(wall.min_z, -5.0);
        assert_exact(wall.max_z, -4.0);
    }

    #[test]
    fn test_collision_stops_player_at_wall() {
        // Wall from x: [-5, 5], z: [-10.4, -10.0]
        let wall = WallAabb::new(-5.0, -10.4, 10.0, 0.4);
        let walls = vec![wall];

        // Player moving straight into the wall from z = -9.6 towards -10.1
        let candidate = Vec2::new(0.0, -10.1);
        let resolved = resolve_player_collision(candidate, PLAYER_RADIUS, &walls);

        // Player should be pushed back to z = -10.0 + PLAYER_RADIUS (-9.7)
        assert!((resolved.y - (-9.70)).abs() < 1e-4);
        assert!((resolved.x - 0.0).abs() < 1e-4);
    }

    #[test]
    fn test_wall_sliding_allows_tangential_motion() {
        // Wall along X at z = -10.0
        let wall = WallAabb::new(-5.0, -10.4, 10.0, 0.4);
        let walls = vec![wall];

        // Player at z = -9.7 (touching wall), moves diagonally: dx = +0.5, dz = -0.2
        let candidate = Vec2::new(0.5, -9.9);
        let resolved = resolve_player_collision(candidate, PLAYER_RADIUS, &walls);

        // X movement is preserved (0.5), Z is constrained to -9.7
        assert!((resolved.x - 0.5).abs() < 1e-4);
        assert!((resolved.y - (-9.70)).abs() < 1e-4);
    }

    #[test]
    fn test_corner_collision_stops_both_axes() {
        // North wall at z = -10.0 and East wall at x = 5.0
        let north_wall = WallAabb::new(-5.0, -10.4, 10.0, 0.4);
        let east_wall = WallAabb::new(5.0, -10.4, 0.4, 10.0);
        let walls = vec![north_wall, east_wall];

        // Player moving from inside room towards corner (x: 4.8 -> 4.9, z: -9.8 -> -9.9)
        // Candidate at (4.9, -9.9) penetrates both walls
        let candidate = Vec2::new(4.9, -9.9);
        let resolved = resolve_player_collision(candidate, PLAYER_RADIUS, &walls);

        // Should be constrained on both axes: x <= 5.0 - radius (4.70), z >= -10.0 + radius (-9.70)
        assert!((resolved.x - (5.0 - PLAYER_RADIUS)).abs() < 1e-3);
        assert!((resolved.y - (-10.0 + PLAYER_RADIUS)).abs() < 1e-3);
    }

    #[test]
    fn test_variable_height_wall_collision() {
        // Raised wall segment from y: 2.0 to 3.5 (player can walk under)
        let raised_wall = WallAabb::with_y(0.0, 2.0, 0.0, 5.0, 1.5, 0.4);
        assert!(!raised_wall.intersects_player_y());
        let walls = vec![raised_wall];
        let candidate = Vec2::new(2.5, 0.2);
        let resolved = resolve_player_collision(candidate, PLAYER_RADIUS, &walls);
        assert_eq!(resolved, candidate);

        // Half-height wall from y: 0.0 to 1.0 (blocks player)
        let half_wall = WallAabb::with_y(0.0, 0.0, 0.0, 5.0, 1.0, 0.4);
        assert!(half_wall.intersects_player_y());
        let walls = vec![half_wall];
        let resolved = resolve_player_collision(candidate, PLAYER_RADIUS, &walls);
        assert_ne!(resolved, candidate);
    }

    /// One 10x10 m room with a single 10 x 0.4 m wall at z = 4.8..5.2 and the
    /// supplied `openings`/`props` JSON.
    fn level_with_wall(openings_json: &str, props_json: &str) -> LevelDef {
        let json = format!(
            r#"{{
                "format_version": 1,
                "id": "collision_test",
                "name": "Collision Test",
                "spawn": {{ "x": 5.0, "z": 5.0 }},
                "room": {{ "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.5 }},
                "walls": [{{
                    "x": 0.0, "z": 4.8, "width": 10.0, "depth": 0.4, "height": 3.5,
                    "openings": {openings_json}
                }}],
                "props": {props_json}
            }}"#
        );
        LevelDef::from_json(&json).expect("valid json")
    }

    #[test]
    fn test_doorway_wall_lets_the_player_pass_through() {
        let level = level_with_wall(
            r#"[{ "kind": "door", "offset": 4.0, "width": 2.0, "height": 2.1 }]"#,
            "[]",
        );
        let walls = level.collision_aabbs();

        // Walking straight through the doorway is unobstructed.
        let in_doorway = Vec2::new(5.0, 5.0);
        assert_eq!(
            resolve_player_collision(in_doorway, PLAYER_RADIUS, &walls),
            in_doorway
        );

        // The solid wall either side of the door still blocks.
        let into_wall = Vec2::new(1.0, 4.9);
        let resolved = resolve_player_collision(into_wall, PLAYER_RADIUS, &walls);
        assert_ne!(resolved, into_wall);
        assert!(resolved.y <= 4.8 - PLAYER_RADIUS + 1e-3);
    }

    #[test]
    fn test_doorway_header_never_blocks_the_player() {
        let level = level_with_wall(
            r#"[{ "kind": "door", "offset": 4.0, "width": 2.0, "height": 2.1 }]"#,
            "[]",
        );
        let walls = level.collision_aabbs();
        // The header slice starts at 2.1 m, above the 1.8 m player.
        let header = walls
            .iter()
            .find(|w| w.min_y > 2.0 && w.max_y > 3.0)
            .expect("door header slice");
        assert!(!header.intersects_player_y());
    }

    #[test]
    fn test_window_with_sill_blocks_the_player() {
        let level = level_with_wall(
            r#"[{ "kind": "window", "offset": 4.0, "width": 2.0, "height": 1.0, "sill": 1.0 }]"#,
            "[]",
        );
        let walls = level.collision_aabbs();
        // The sill wall spans y = 0..1.0, so the window is not walk-through.
        let sill = walls
            .iter()
            .find(|w| w.min_y == 0.0 && w.max_y <= 1.0 + 1e-3 && w.min_x >= 3.9 && w.max_x <= 6.1)
            .expect("window sill slice");
        assert!(sill.intersects_player_y());

        let in_window = Vec2::new(5.0, 5.0);
        let resolved = resolve_player_collision(in_window, PLAYER_RADIUS, &walls);
        assert_ne!(resolved, in_window);
    }

    #[test]
    fn test_solid_prop_blocks_the_player() {
        let solid_level = level_with_wall(
            "[]",
            r#"[{ "model": "core:crate", "x": 5.0, "z": 2.0, "size": [1.0, 1.0, 1.0], "solid": true }]"#,
        );
        let solid_aabbs = solid_level.collision_aabbs();
        let into_prop = Vec2::new(5.0, 2.0);
        assert_ne!(
            resolve_player_collision(into_prop, PLAYER_RADIUS, &solid_aabbs),
            into_prop
        );

        // A non-solid prop is ignored entirely by collision.
        let decorative_level = level_with_wall(
            "[]",
            r#"[{ "model": "core:plant", "x": 5.0, "z": 2.0, "size": [1.0, 1.0, 1.0] }]"#,
        );
        let decorative_aabbs = decorative_level.collision_aabbs();
        assert_eq!(decorative_aabbs.len(), solid_aabbs.len() - 1);
        assert_eq!(
            resolve_player_collision(into_prop, PLAYER_RADIUS, &decorative_aabbs),
            into_prop
        );
    }
}
