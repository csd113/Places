//! Static baked interior lighting.
//!
//! The game targets a `PocketCHIP` (Mali-400/Lima, OpenGL ES 2.0, 480x272), so
//! there is no dynamic lighting anywhere in the render loop. Everything in this
//! module tree runs once per level load, producing one [`LightColor`] per
//! sampled point that the geometry builder bakes into ordinary vertex colours,
//! one channel at a time:
//!
//! ```text
//! load level
//!     -> collect rooms + ceiling fixtures            (self::bake)
//!     -> room area, fixture density, height factor   (self::math)
//!     -> room baseline + local fixture pools         (self::bake)
//!     -> static wall visibility for every pool       (self::visibility)
//!     -> bounded doorway blending                    (self::bake)
//!     -> bake into world geometry + prop instances   (crate::render)
//!     -> upload the same static batches as before    (crate::render)
//! ```
//!
//! There is no per-frame light loop, no light texture, no extra draw call and
//! no shader change: the renderer still draws exactly the batches it drew
//! before, with darker or brighter vertex colours.
//!
//! Lighting model
//! --------------
//! Every value below is a three-channel [`LightColor`], accumulated per channel;
//! a fixture emits the colour it authors, not one global tint.
//!
//! 1. **Room baseline.** Every room sums the emitted colour of the ceiling
//!    fixtures it owns (`colour x intensity x ceiling-height factor`), divides
//!    each channel by its floor area and feeds that through a logarithmic
//!    compression and a smoothly saturating curve. The compression is what
//!    keeps the game's deliberately sparse large rooms (a long corridor or a
//!    hall with a handful of widely spaced panels) broadly illuminated without
//!    also saturating small, densely lit rooms; see [`compressed_density`]. A
//!    large room with two panels is dim; a small room with many panels
//!    approaches full brightness; no channel ever exceeds [`MAX_BRIGHTNESS`].
//! 2. **Local fixture pools.** Every fixture adds a broad pool of its own
//!    colour with a smooth falloff that reaches zero at
//!    [`LOCAL_LIGHT_RADIUS_M`]. The pool is measured to the fixture's
//!    rectangular panel rather than to a point, so it reads as a fluorescent
//!    panel instead of a spotlight. A pool only reaches a surface the fixture
//!    can actually see: see [`visibility`].
//! 3. **Opening blending.** Rooms joined by walk-through openings (doors and
//!    passages that reach the floor) mix a bounded fraction of each other's
//!    baseline near the opening, so light appears to leak through doorways
//!    instead of stopping at the threshold. The mixture follows the aperture,
//!    so an opening joins two rooms through the hole it cuts rather than
//!    through the wall around it. Windows and vents are deliberately excluded:
//!    in this engine they usually face the outside, and a raised opening does
//!    not read as a walk-through connection. Only the openings of single walls
//!    are considered; there is no recursive propagation and no global solver.
//! 4. **Ambient floor.** A room without fixtures stays barely visible: the
//!    ambient contribution is deliberately small (see [`AMBIENT_LEVEL`]) and
//!    must never stand in for real fixtures. Unlit rooms are dark by design.
//!
//! Opaque geometry matters
//! -----------------------
//! An opaque wall is a lighting boundary. A fixture's local pool is tested
//! against the same solid wall slices the geometry and collision use, so a
//! fixture behind a wall does not light the room on the other side — not with
//! white light and not with colour — while a doorway, window or vent still
//! transmits light through the hole it cuts. See [`visibility`].
//!
//! Determinism and ownership
//! -------------------------
//! Overlapping and intersecting rooms are legal level design in this game, so
//! light ownership must be defined rather than rejected: a point (and therefore
//! a fixture) belongs to the *smallest-area* room that contains it, with ties
//! resolved by the level's own room order (`rooms`, then the optional `room`).
//! Each fixture therefore contributes to exactly one room and is never counted
//! twice. See [`LevelLighting::room_index_at`].
//!
//! Module layout
//! -------------
//! ```text
//! color.rs        the emitted-colour type and its sanitising
//! tuning.rs       every calibrated number, and the fixture family table
//! math.rs         the pure, bounded falloff and density maths
//! bake.rs         the bake itself and the queries it answers
//! visibility.rs   static opaque-wall visibility for local pools and openings
//! tests.rs        unit tests for the whole tree
//! ```

mod bake;
mod color;
mod math;
mod tuning;
mod visibility;

#[cfg(test)]
mod tests;

pub use bake::{BakedLight, LevelLighting, LightingSummary, RoomLighting};
pub use color::{LightColor, MAX_LIGHT_COLOR};
pub use math::{
    ceiling_height_factor, compressed_density, effective_power, fixture_half_extents,
    fixture_half_extents_for, fixture_is_turned, light_grid_cells, room_baseline,
    sanitize_intensity, saturating_brightness, smooth_falloff, wall_light_segments,
};
pub use tuning::{
    AMBIENT_LEVEL, DEFAULT_LIGHT_COLOR, FIXTURE_DROP_M, FIXTURE_HALF_DEPTH_M, FIXTURE_HALF_WIDTH_M,
    FixtureKind, FixtureProfile, HEIGHT_FALLOFF, LIGHT_FIXTURE_IDS, LIGHT_GRID_CELL_M,
    LOCAL_LIGHT_MAX, LOCAL_LIGHT_RADIUS_M, LOCAL_LIGHT_STRENGTH, MAX_BRIGHTNESS,
    MAX_LIGHT_GRID_CELLS, MAX_LIGHT_INTENSITY, MAX_WALL_LIGHT_SEGMENTS, MIN_ROOM_AREA_M2,
    OPENING_BLEND_RADIUS_M, OPENING_BLEND_STRENGTH, OPENING_VERTICAL_FADE_M,
    REFERENCE_CEILING_HEIGHT_M, REFERENCE_LIGHT_AREA_M2, WALL_FACE_PROBE_M,
    WALL_LIGHT_DEFAULT_HEIGHT_M, ambient_color, fixture_profile, fixture_profile_for_kind,
};
pub use visibility::{QuerySite, Visibility};
