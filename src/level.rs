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
    pub z: f32,
    pub width: f32,
    pub depth: f32,
    #[serde(default = "default_ceiling_height")]
    pub height: f32,
    #[serde(default)]
    pub faces: HashMap<String, String>,
}

impl WallDef {
    pub fn to_aabb(&self) -> WallAabb {
        WallAabb::new(self.x, self.z, self.width, self.depth)
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

    /// Returns collision bounding boxes for all walls in the level.
    pub fn collision_aabbs(&self) -> Vec<WallAabb> {
        self.walls.iter().map(|w| w.to_aabb()).collect()
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
}
