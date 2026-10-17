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

use crate::engine::collision::WallAabb;
use crate::engine::level::{Ladders, LevelDef, WalkableCeiling, WalkableFloor, WaterVolumes};

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

impl CompiledCollision {
    /// Retained heap storage of every collision sampler, including capacity.
    #[must_use]
    pub(crate) fn retained_heap_bytes(&self) -> usize {
        self.walls
            .capacity()
            .saturating_mul(std::mem::size_of::<WallAabb>())
            .saturating_add(self.floor.retained_heap_bytes())
            .saturating_add(self.ceiling.retained_heap_bytes())
            .saturating_add(self.water.retained_heap_bytes())
            .saturating_add(self.ladders.retained_heap_bytes())
    }

    /// Derives the shared static samplers without creating a gameplay runtime.
    #[must_use]
    pub fn from_level(level: &LevelDef) -> Self {
        Self {
            walls: level.collision_aabbs(),
            floor: WalkableFloor::from_level(level),
            ceiling: WalkableCeiling::from_level(level),
            water: WaterVolumes::from_level(level),
            ladders: Ladders::from_level(level),
        }
    }
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
            .map_err(|error| format!("collision record has too many walls: {error}"))?,
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
