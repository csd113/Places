//! Binary codec for the compiled static collision record.
//!
//! The offline compiler resolves a level's static collision once and stores
//! this record; the player rebuilds its collision samplers from the record
//! instead of deriving boxes and surfaces from geometry at load. Interactive
//! semantics (doors, interactables, triggers, routes, fixture switches) stay
//! in the validated semantic level record.
//!
//! Layout (little-endian, byte-aligned, no implicit padding):
//!
//! ```text
//! magic         4 bytes  "PLCL"
//! version       u16      [`COLLISION_RECORD_VERSION`]
//! walls         u32 count, then count x wall
//! floor         u32 room count, then rooms
//! ceiling       u32 room count, then rooms
//! water         u32 volume count, then volumes
//! ladders       u32 ladder count, then ladders
//! ```
//!
//! ```text
//! wall:
//!   min_x, min_y, min_z, max_x, max_y, max_z, step_up   f32 x 7
//! ```
//!
//! ```text
//! floor room:
//!   x0, x1, z0, z1, floor_y                             f32 x 5
//!   ramp_count    u32, then ramp_count x ramp
//!   stair_count   u32, then stair_count x stair
//!   region_count  u32, then region_count x region
//! ramp:
//!   x, z, width, depth, offset_y, rise                  f32 x 6
//!   floor_y                                             f32
//! stair:
//!   x, z, width, depth, offset_y, rise                  f32 x 6
//!   steps                                               u32
//!   floor_y                                             f32
//! region:
//!   x0, x1, z0, z1, y                                   f32 x 5
//! ```
//!
//! ```text
//! ceiling room:
//!   x0, x1, z0, z1                                      f32 x 4
//!   floor_y, height                                     f32 x 2
//!   profile                                             u8 (0 flat, 1 gable)
//!   gable only:
//!     ridge                                             u8 (0 = x, 1 = z)
//!     ridge_rise                                        f32
//! ```
//!
//! ```text
//! water volume:
//!   x0, x1, z0, z1, surface_y, bottom_y, opacity        f32 x 7
//!   shape           u8 (0 rect, 1 circle)
//!   radius          f32 (0 for rect, > 0 for circle)
//!   swimming                                            u8 boolean
//!   material        u8 present, then u32 length + UTF-8 bytes (<= 256)
//! ```
//!
//! ```text
//! ladder:
//!   x0, x1, z0, z1, bottom_y, top_y                     f32 x 6
//!   facing_x, facing_z                                  f32 x 2
//! ```
//!
//! Every count and the whole record are bounded before any allocation; a
//! malformed record is rejected with a named error, never truncated.

use crate::collision::WallAabb;
use crate::level::{Ladders, WalkableCeiling, WalkableFloor, WaterVolumes};

use super::binary::{Reader, Writer};
use super::{MAX_COLLISION_BOXES, MAX_COLLISION_BYTES};

/// Version of the compiled collision record layout.
///
/// Version 1 wrote a water volume as footprint, surface, bottom, opacity,
/// swimming and material. Version 2 adds the footprint shape byte and the
/// circle radius after `opacity`, because a rectangular-only record cannot
/// express a circular pool's membership or its drawn disc. A version-1 record
/// is refused by name: its `swimming` byte would be misread as the shape code,
/// so packages built by the previous compiler must be rebuilt.
pub const COLLISION_RECORD_VERSION: u16 = 2;

/// Magic identifying a compiled collision record.
pub const COLLISION_MAGIC: [u8; 4] = *b"PLCL";

/// Largest accepted count of water volumes in one record.
pub(crate) const MAX_COLLISION_WATER_VOLUMES: u64 = 1 << 16;

/// Largest accepted count of ladders in one record.
pub(crate) const MAX_COLLISION_LADDERS: u64 = 1 << 16;

/// Largest accepted water material id length, in bytes.
pub(crate) const MAX_COLLISION_WATER_MATERIAL_BYTES: u64 = 256;

/// The compiled static collision primitives of one world.
#[derive(Debug, Clone, PartialEq)]
pub struct CompiledCollision {
    /// Static wall and solid-prop boxes.
    pub walls: Vec<WallAabb>,
    /// Walkable floor model.
    pub floor: WalkableFloor,
    /// Walkable ceiling model.
    pub ceiling: WalkableCeiling,
    /// Water volumes.
    pub water: WaterVolumes,
    /// Ladders.
    pub ladders: Ladders,
}

/// Encodes a compiled static collision record.
///
/// # Errors
///
/// Returns a named error when a section exceeds the format's count or byte
/// limits.
pub fn write_collision(collision: &CompiledCollision) -> Result<Vec<u8>, String> {
    if collision.walls.len() > MAX_COLLISION_BOXES {
        return Err(format!(
            "collision record has {} walls (limit {MAX_COLLISION_BOXES})",
            collision.walls.len()
        ));
    }
    let mut writer = Writer::with_capacity(collision.walls.len().saturating_mul(32));
    writer.bytes(&COLLISION_MAGIC);
    writer.u16(COLLISION_RECORD_VERSION);
    writer.u32(
        u32::try_from(collision.walls.len())
            .map_err(|_| "collision record has too many walls".to_string())?,
    );
    for wall in &collision.walls {
        wall.write_compiled(&mut writer);
    }
    collision.floor.write_compiled(&mut writer)?;
    collision.ceiling.write_compiled(&mut writer)?;
    collision.water.write_compiled(&mut writer)?;
    collision.ladders.write_compiled(&mut writer)?;
    let bytes = writer.into_bytes();
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_COLLISION_BYTES {
        return Err(format!(
            "collision record is {} bytes (limit {MAX_COLLISION_BYTES})",
            bytes.len()
        ));
    }
    Ok(bytes)
}

/// Decodes and validates a compiled static collision record.
///
/// # Errors
///
/// Returns a named error when the record exceeds the byte limit, has the
/// wrong magic or version, is truncated, declares an out-of-range count,
/// holds a non-finite or inverted value, carries an invalid material string,
/// or has trailing bytes.
pub fn read_collision(bytes: &[u8]) -> Result<CompiledCollision, String> {
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_COLLISION_BYTES {
        return Err(format!(
            "collision record is {} bytes (limit {MAX_COLLISION_BYTES})",
            bytes.len()
        ));
    }
    let mut reader = Reader::new(bytes);
    if reader.bytes(4)? != COLLISION_MAGIC {
        return Err("collision record has the wrong magic".to_string());
    }
    let version = reader.u16()?;
    if version != COLLISION_RECORD_VERSION {
        return Err(format!(
            "collision record version {version} is not supported (this build reads \
             {COLLISION_RECORD_VERSION})"
        ));
    }
    let wall_count = reader.count(collision_box_limit(), "collision wall count")?;
    let mut walls = Vec::with_capacity(wall_count.min(4096));
    for _ in 0..wall_count {
        walls.push(WallAabb::read_compiled(&mut reader)?);
    }
    let floor = WalkableFloor::read_compiled(&mut reader)?;
    let ceiling = WalkableCeiling::read_compiled(&mut reader)?;
    let water = WaterVolumes::read_compiled(&mut reader)?;
    let ladders = Ladders::read_compiled(&mut reader)?;
    if !reader.is_empty() {
        return Err(format!(
            "collision record has {} trailing bytes",
            reader.remaining()
        ));
    }
    Ok(CompiledCollision {
        walls,
        floor,
        ceiling,
        water,
        ladders,
    })
}

/// [`MAX_COLLISION_BOXES`] as a `u64` reader limit.
fn collision_box_limit() -> u64 {
    u64::try_from(MAX_COLLISION_BOXES).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    // Test code: unwrap/expect, indexing and permissive arithmetic are
    // idiomatic here; the production lints stay enforced everywhere else.
    #![allow(
        clippy::arithmetic_side_effects,
        clippy::expect_used,
        clippy::float_cmp,
        clippy::indexing_slicing,
        clippy::panic,
        clippy::unwrap_used
    )]

    use super::*;
    use crate::game::CollisionWorld;
    use crate::level::LevelDef;

    /// A small level exercising every static collision section: walls, flat
    /// and gable ceilings, a floor region, a ramp, a staircase, water with and
    /// without an authored material and a ladder.
    const TEST_LEVEL: &str = r#"{
        "format_version": 3,
        "id": "collision_record_test",
        "name": "Collision Record Test",
        "spawn": { "x": 1.0, "z": 1.0 },
        "rooms": [
            { "x": 0.0, "z": 0.0, "width": 6.0, "depth": 6.0, "height": 3.0 },
            { "x": 6.0, "z": 0.0, "width": 4.0, "depth": 4.0, "height": 3.0,
              "ceiling": { "kind": "gable", "ridge": "z", "ridge_rise": 1.2 } }
        ],
        "walls": [
            { "x": 0.0, "z": 0.0, "width": 6.0, "depth": 0.2, "height": 3.0 }
        ],
        "floor_regions": [
            { "x": 1.0, "z": 1.0, "width": 2.0, "depth": 2.0, "offset_y": -0.4 }
        ],
        "ramps": [
            { "x": 0.0, "z": 4.0, "width": 2.0, "depth": 2.0,
              "offset_y": 0.0, "rise": 0.8 }
        ],
        "stairs": [
            { "x": 4.0, "z": 4.0, "width": 2.0, "depth": 2.0,
              "offset_y": 0.0, "rise": 0.8, "steps": 4 }
        ],
        "water": [
            { "x": 2.0, "z": 2.0, "width": 2.0, "depth": 2.0,
              "surface_y": -0.2, "bottom_y": -1.4,
              "material": "core:water_pool_01", "opacity": 0.5,
              "swimming": true },
            { "x": 4.5, "z": 1.0, "width": 1.0, "depth": 1.0,
              "surface_y": 0.0, "swimming": false },
            { "shape": "circle", "x": 1.0, "z": 4.5, "radius": 0.75,
              "surface_y": -0.1, "bottom_y": -1.0 }
        ],
        "ladders": [
            { "x": 3.0, "z": 5.0, "width": 0.6, "depth": 0.6,
              "bottom_y": 0.0, "top_y": 2.0, "facing_degrees": 90.0 }
        ]
    }"#;

    fn test_level() -> LevelDef {
        LevelDef::from_json(TEST_LEVEL).expect("the collision test level parses")
    }

    fn compiled_collision(level: &LevelDef) -> CompiledCollision {
        let world = CollisionWorld::from_level(level);
        CompiledCollision {
            walls: world.walls,
            floor: world.floor,
            ceiling: world.ceiling,
            water: world.water,
            ladders: world.ladders,
        }
    }

    fn valid_bytes(level: &LevelDef) -> Vec<u8> {
        write_collision(&compiled_collision(level)).expect("the test collision encodes")
    }

    fn push_f32s(bytes: &mut Vec<u8>, values: &[f32]) {
        for value in values {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
    }

    #[test]
    fn round_trips_every_static_section() {
        let level = test_level();
        let world = CollisionWorld::from_level(&level);
        assert!(!world.walls.is_empty(), "the test level has walls");
        assert!(!world.floor.is_empty(), "the test level has floor rooms");
        assert!(
            !world.ceiling.is_empty(),
            "the test level has ceiling rooms"
        );
        assert_eq!(world.water.len(), 3, "three water volumes");
        assert_eq!(world.ladders.len(), 1, "one ladder");
        let compiled = compiled_collision(&level);

        let bytes = write_collision(&compiled).expect("the collision encodes");
        let decoded = read_collision(&bytes).expect("the collision decodes");
        assert_eq!(decoded, compiled);
    }

    /// A circular volume survives the record round trip with its shape,
    /// radius and circular membership intact.
    #[test]
    fn round_trips_a_circular_water_volume() {
        let compiled = compiled_collision(&test_level());
        let circle = compiled
            .water
            .volumes()
            .iter()
            .find(|volume| volume.shape == crate::level::WaterShape::Circle)
            .expect("the test level authors one circle");
        assert_eq!(circle.radius, 0.75);

        let bytes = write_collision(&compiled).expect("the collision encodes");
        let decoded = read_collision(&bytes).expect("the collision decodes");
        let decoded_circle = decoded
            .water
            .volumes()
            .iter()
            .find(|volume| volume.shape == crate::level::WaterShape::Circle)
            .expect("the decoded record keeps the circle");
        assert_eq!(decoded_circle, circle);
        // Membership stays circular: the bounding box corner is outside, the
        // centre is inside, and the rim itself is the dry wall line.
        assert!(decoded_circle.contains(1.75, 5.25), "centre");
        assert!(
            !decoded_circle.contains(1.0, 4.5),
            "a bounding box corner is not water"
        );
        assert!(
            !decoded_circle.contains(2.5, 5.25),
            "the rim is the wall line and is dry"
        );
    }

    #[test]
    fn from_compiled_matches_from_level() {
        let level = test_level();
        let statics = compiled_collision(&level);
        let rebuilt = CollisionWorld::from_compiled(&level, statics, None);
        let from_level = CollisionWorld::from_level(&level);
        // `CollisionWorld` does not derive `PartialEq`: its `EntityWorld`
        // owns live runtime state. Compare every static sampler plus the entity
        // component tables both construction paths resolve.
        assert_eq!(rebuilt.walls, from_level.walls);
        assert_eq!(rebuilt.floor, from_level.floor);
        assert_eq!(rebuilt.ceiling, from_level.ceiling);
        assert_eq!(rebuilt.water, from_level.water);
        assert_eq!(rebuilt.ladders, from_level.ladders);
        assert_eq!(
            rebuilt.world.components(),
            from_level.world.components(),
            "both paths must resolve the same authored components"
        );
    }

    #[test]
    fn rejects_a_truncated_record() {
        let bytes = valid_bytes(&test_level());
        assert!(read_collision(&bytes[..bytes.len() - 1]).is_err());
        assert!(read_collision(&bytes[..6]).is_err());
    }

    #[test]
    fn rejects_the_wrong_magic_and_version() {
        let mut bytes = valid_bytes(&test_level());
        bytes[0] = b'X';
        assert!(read_collision(&bytes).is_err());

        // A version-1 record is the previous layout, whose `swimming` byte
        // would be misread as a shape code; it must be refused by name.
        let mut bytes = valid_bytes(&test_level());
        bytes[4..6].copy_from_slice(&1_u16.to_le_bytes());
        let error = read_collision(&bytes).expect_err("a v1 record must be refused");
        assert!(
            error.contains("version 1 is not supported"),
            "the refusal names the version: {error}"
        );
    }

    #[test]
    fn rejects_inverted_wall_bounds() {
        let mut compiled = compiled_collision(&test_level());
        compiled.walls.push(WallAabb {
            min_x: 2.0,
            max_x: 1.0,
            min_y: 0.0,
            max_y: 1.0,
            min_z: 0.0,
            max_z: 1.0,
            step_up: 0.0,
        });
        let bytes = write_collision(&compiled).expect("the collision encodes");
        assert!(read_collision(&bytes).is_err());
    }

    #[test]
    fn rejects_non_finite_wall_values() {
        let mut compiled = compiled_collision(&test_level());
        compiled.walls.push(WallAabb {
            min_x: f32::NAN,
            max_x: 1.0,
            min_y: 0.0,
            max_y: 1.0,
            min_z: 0.0,
            max_z: 1.0,
            step_up: 0.0,
        });
        let bytes = write_collision(&compiled).expect("the collision encodes");
        assert!(read_collision(&bytes).is_err());
    }

    #[test]
    fn rejects_out_of_range_counts() {
        let too_many = u32::try_from(MAX_COLLISION_BOXES).expect("fits in u32") + 1;
        let mut header = Vec::new();
        header.extend_from_slice(&COLLISION_MAGIC);
        header.extend_from_slice(&COLLISION_RECORD_VERSION.to_le_bytes());

        let mut walls = header.clone();
        walls.extend_from_slice(&too_many.to_le_bytes());
        assert!(read_collision(&walls).is_err());

        let mut floor_rooms = header;
        floor_rooms.extend_from_slice(&0_u32.to_le_bytes());
        floor_rooms.extend_from_slice(&too_many.to_le_bytes());
        assert!(read_collision(&floor_rooms).is_err());
    }

    #[test]
    fn rejects_a_zero_sized_ramp() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&COLLISION_MAGIC);
        bytes.extend_from_slice(&COLLISION_RECORD_VERSION.to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes.extend_from_slice(&1_u32.to_le_bytes());
        push_f32s(&mut bytes, &[0.0, 1.0, 0.0, 1.0, 0.0]);
        bytes.extend_from_slice(&1_u32.to_le_bytes());
        push_f32s(&mut bytes, &[0.0, 0.0, 0.0, 1.0, 0.0, 0.5, 0.0]);
        assert!(read_collision(&bytes).is_err());
    }

    #[test]
    fn rejects_trailing_bytes() {
        let mut bytes = valid_bytes(&test_level());
        bytes.push(0);
        assert!(read_collision(&bytes).is_err());
    }
}
