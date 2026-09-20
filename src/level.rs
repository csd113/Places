use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Room boundary dimensions for a level.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomDef {
    pub width: f32,
    pub depth: f32,
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
    pub wall: String,
    pub floor: String,
    pub ceiling: String,
}

/// Rectangular wall footprint definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WallDef {
    pub x: f32,
    pub z: f32,
    pub width: f32,
    pub depth: f32,
    #[serde(default)]
    pub faces: HashMap<String, String>,
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

/// Level schema corresponding to Section 24 of the design document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LevelDef {
    pub format_version: u32,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub author: String,
    pub room: RoomDef,
    pub spawn: SpawnDef,
    pub defaults: LevelDefaults,
    #[serde(default)]
    pub walls: Vec<WallDef>,
    #[serde(default)]
    pub floor_patches: Vec<FloorPatchDef>,
    #[serde(default)]
    pub ceiling_lights: Vec<CeilingLightDef>,
}
