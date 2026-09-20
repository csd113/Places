use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::collision::WallAabb;

fn default_ceiling_height() -> f32 {
    3.5
}

/// Rectangular room section defining floor and ceiling boundaries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomDef {
    #[serde(default)]
    pub x: f32,
    #[serde(default)]
    pub z: f32,
    pub width: f32,
    pub depth: f32,
    #[serde(default = "default_ceiling_height")]
    pub height: f32,
}

/// Player initial spawn position and orientation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpawnDef {
    pub x: f32,
    pub z: f32,
    #[serde(default)]
    pub yaw_degrees: f32,
}

/// Default material codes for room surfaces.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LevelDefaults {
    #[serde(default)]
    pub wall: String,
    #[serde(default)]
    pub floor: String,
    #[serde(default)]
    pub ceiling: String,
}

impl Default for LevelDefaults {
    fn default() -> Self {
        Self {
            wall: "core:wallpaper_yellow_01".into(),
            floor: "core:carpet_beige_01".into(),
            ceiling: "core:ceiling_panel_01".into(),
        }
    }
}

/// Rectangular wall footprint definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WallDef {
    pub x: f32,
    #[serde(default)]
    pub y: f32,
    pub z: f32,
    pub width: f32,
    pub depth: f32,
    #[serde(default)]
    pub height: Option<f32>,
    #[serde(default)]
    pub faces: HashMap<String, String>,
}

impl WallDef {
    pub fn resolved_height(&self, default_ceiling: f32) -> f32 {
        self.height.unwrap_or(default_ceiling)
    }

    pub fn to_aabb(&self) -> WallAabb {
        let h = self.resolved_height(3.5);
        WallAabb::with_y(self.x, self.y, self.z, self.width, h, self.depth)
    }

    pub fn to_aabb_with_ceiling(&self, default_ceiling: f32) -> WallAabb {
        let h = self.resolved_height(default_ceiling);
        WallAabb::with_y(self.x, self.y, self.z, self.width, h, self.depth)
    }
}

/// Rectangular floor material patch (e.g. damp carpet).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FloorPatchDef {
    pub x: f32,
    pub z: f32,
    pub width: f32,
    pub depth: f32,
    pub material: String,
}

/// Ceiling light fixture placement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CeilingLightDef {
    pub fixture: String,
    pub x: f32,
    pub z: f32,
    #[serde(default)]
    pub rotation_degrees: f32,
    #[serde(default)]
    pub brightness: Option<f32>,
}

/// Level schema corresponding to Sections 22 and 24 of the design document,
/// supporting both single rooms and multiple connected room sections.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LevelDef {
    pub format_version: u32,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub room: Option<RoomDef>,
    #[serde(default)]
    pub rooms: Vec<RoomDef>,
    pub spawn: SpawnDef,
    #[serde(default)]
    pub defaults: LevelDefaults,
    #[serde(default)]
    pub walls: Vec<WallDef>,
    #[serde(default)]
    pub floor_patches: Vec<FloorPatchDef>,
    #[serde(default)]
    pub ceiling_lights: Vec<CeilingLightDef>,
}

impl LevelDef {
    pub fn from_json(json_str: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json_str)
    }

    /// Returns all room sections (merging optional `room` and `rooms`).
    pub fn all_rooms(&self) -> Vec<RoomDef> {
        let mut sections = self.rooms.clone();
        if let Some(ref r) = self.room {
            sections.push(r.clone());
        }
        sections
    }

    /// Returns the room ceiling height for the given (x, z) coordinates.
    pub fn ceiling_height_at(&self, x: f32, z: f32) -> f32 {
        let rooms = self.all_rooms();
        for room in &rooms {
            let x0 = room.x.min(room.x + room.width);
            let x1 = room.x.max(room.x + room.width);
            let z0 = room.z.min(room.z + room.depth);
            let z1 = room.z.max(room.z + room.depth);
            if x >= x0 - 0.01 && x <= x1 + 0.01 && z >= z0 - 0.01 && z <= z1 + 0.01 {
                return room.height;
            }
        }
        rooms.first().map(|r| r.height).unwrap_or(3.5)
    }

    /// Returns collision bounding boxes for all walls in the level.
    pub fn collision_aabbs(&self) -> Vec<WallAabb> {
        self.walls
            .iter()
            .map(|w| {
                let default_h = self.ceiling_height_at(w.x + w.width * 0.5, w.z + w.depth * 0.5);
                w.to_aabb_with_ceiling(default_h)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_single_room_level() {
        let json = r#"{
            "format_version": 1,
            "id": "test_room",
            "name": "Test Room",
            "spawn": { "x": 0.0, "z": 0.0 },
            "room": { "x": -6.0, "z": -12.0, "width": 12.0, "depth": 16.0, "height": 3.5 }
        }"#;
        let level = LevelDef::from_json(json).expect("valid json");
        let rooms = level.all_rooms();
        assert_eq!(rooms.len(), 1);
        assert_eq!(rooms[0].width, 12.0);
        assert_eq!(rooms[0].height, 3.5);
    }

    #[test]
    fn test_parse_multi_room_level_with_walls() {
        let json = r#"{
            "format_version": 1,
            "id": "multi_room",
            "name": "Connected Rooms",
            "spawn": { "x": 2.0, "z": 2.0, "yaw_degrees": 90.0 },
            "rooms": [
                { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0 },
                { "x": 10.0, "z": 2.0, "width": 8.0, "depth": 6.0 }
            ],
            "walls": [
                { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 0.35 }
            ]
        }"#;
        let level = LevelDef::from_json(json).expect("valid multi room json");
        assert_eq!(level.all_rooms().len(), 2);
        assert_eq!(level.walls.len(), 1);
        let aabbs = level.collision_aabbs();
        assert_eq!(aabbs.len(), 1);
        assert_eq!(aabbs[0].max_x, 10.0);
    }

    #[test]
    fn test_parse_variable_wall_properties() {
        let json = r#"{
            "format_version": 1,
            "id": "variable_walls",
            "name": "Variable Walls Test",
            "spawn": { "x": 0.0, "z": 0.0 },
            "room": { "x": 0.0, "z": 0.0, "width": 20.0, "depth": 20.0, "height": 4.0 },
            "walls": [
                { "x": 1.0, "z": 1.0, "width": 2.0, "depth": 0.2 },
                { "x": 5.0, "z": 5.0, "width": 3.0, "depth": 0.2, "y": 0.0, "height": 1.5 },
                { "x": 5.0, "z": 5.0, "width": 3.0, "depth": 0.2, "y": 2.5, "height": 1.5 }
            ]
        }"#;
        let level = LevelDef::from_json(json).expect("valid variable walls json");
        assert_eq!(level.walls.len(), 3);

        // Wall 0: y omitted (defaults to 0.0), height omitted (defaults to room height 4.0)
        assert_eq!(level.walls[0].y, 0.0);
        assert_eq!(level.walls[0].height, None);
        assert_eq!(level.walls[0].resolved_height(4.0), 4.0);

        // Wall 1: window sill (half-height)
        assert_eq!(level.walls[1].y, 0.0);
        assert_eq!(level.walls[1].height, Some(1.5));

        // Wall 2: window header (raised)
        assert_eq!(level.walls[2].y, 2.5);
        assert_eq!(level.walls[2].height, Some(1.5));

        let aabbs = level.collision_aabbs();
        assert_eq!(aabbs.len(), 3);
        assert_eq!(aabbs[0].min_y, 0.0);
        assert_eq!(aabbs[0].max_y, 4.0);
        assert_eq!(aabbs[1].min_y, 0.0);
        assert_eq!(aabbs[1].max_y, 1.5);
        assert_eq!(aabbs[2].min_y, 2.5);
        assert_eq!(aabbs[2].max_y, 4.0);
    }
}
