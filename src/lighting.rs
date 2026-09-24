//! Static baked interior lighting.
//!
//! Places ships as a desktop game, but its lighting discipline grew up on a
//! `PocketCHIP` (Mali-400/Lima, OpenGL ES 2.0, 480x272): there is no dynamic
//! lighting anywhere in the render loop. Everything in this module tree runs
//! once per level load, producing one [`LightColor`] per sampled point that the
//! geometry builder bakes into ordinary vertex colours, one channel at a time:
//!
//! ```text
//! load level
//!     -> collect rooms                               (self::bake)
//!     -> resolve generic light sources: fixtures      (self::light, self::bake)
//!        and prop-attached lights, into one list
//!     -> room area, light density, height factor      (self::math)
//!     -> partition areas + baseline field            (self::bake)
//!     -> room baseline + local fixture pools         (self::bake)
//!     -> walls, floor interfaces, ceiling bodies     (self::visibility)
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
//! Every source is a [`LightSource`]: a shape (point, rectangle or line), an
//! authored colour, intensity, range, falloff curve and an `enabled` flag. A
//! visible fixture is geometry that owns one; a placed prop may own several.
//! Nothing about a fixture family or a material creates a light.
//!
//! 1. **Room baseline.** Every room sums the emitted colour of the light
//!    sources it owns (`colour x intensity x ceiling-height factor`), divides
//!    each channel by its floor area and feeds that through a logarithmic
//!    compression and a smoothly saturating curve. The compression is what
//!    keeps the game's deliberately sparse large rooms (a long corridor or a
//!    hall with a handful of widely spaced panels) broadly illuminated without
//!    also saturating small, densely lit rooms; see [`compressed_density`]. A
//!    large room with two panels is dim; a small room with many panels
//!    approaches full brightness. The curve maps onto
//!    `[AMBIENT_LEVEL, BASELINE_MAX]`, never the full `[AMBIENT_LEVEL,
//!    MAX_BRIGHTNESS]` range: the baseline is the fill a static occluder cannot
//!    remove, so leaving the highlight headroom to the visibility-tested pools
//!    is what keeps a shadowed surface visibly darker than a lit one.
//!    The baseline is spatially aware: see *Partitions* below.
//! 2. **Local pools.** Every light adds a broad pool of its own colour that
//!    reaches zero at its own `range` on its own falloff curve (the default is
//!    the historical 6 m smooth cushion, so nothing authored before the generic
//!    model existed changed). The pool is measured to the light's emitting
//!    shape rather than to a point, so a panel reads as a panel and a tube as a
//!    tube. A pool only reaches a surface the light can actually see: see
//!    [`visibility`]. Visibility is sampled on the emitter's own rectangle, so
//!    a partially blocked pool fades over a penumbra; the tap count is a
//!    quality-profile choice ([`ShadowSampling`]), and one tap per axis is the
//!    historical hard edge the vertex-lit fallback still bakes with.
//! 3. **Opening blending.** Room areas joined by walk-through openings (doors
//!    and passages that reach the floor) mix a bounded fraction of each other's
//!    baseline near the opening, so light appears to leak through doorways
//!    instead of stopping at the threshold. The mixture follows the aperture,
//!    so an opening joins two areas through the hole it cuts rather than
//!    through the wall around it. Windows and vents are deliberately excluded:
//!    in this engine they usually face the outside, and a raised opening does
//!    not read as a walk-through connection. Only the openings of single walls
//!    are considered; there is no recursive propagation and no global solver.
//! 4. **Ambient floor.** A room without fixtures stays barely visible: the
//!    ambient contribution is deliberately small (see [`AMBIENT_LEVEL`]) and
//!    must never stand in for real fixtures. Unlit rooms are dark by design.
//!
//! Partitions
//! ----------
//! A room footprint is not assumed to be one open space. When opaque internal
//! walls split it into disconnected areas, each area gets its own baseline:
//! fixture power is spread over the area the fixture can actually reach, so a
//! lit half of a partitioned room cannot lend its baseline to the dark half
//! through the wall. Connectivity is decided by the same wall-solid geometry
//! the pools use, probed just below the ceiling, which is what makes a
//! full-height wall and a door's header separate while a wall that stops short
//! of the ceiling does not. A room that stays one connected volume keeps its
//! historical uniform baseline bit for bit; a doorway in an internal partition
//! still blends the two areas through [`LevelLighting::opening_blend`].
//! See [`LevelLighting::baseline_in_room`] and [`LevelLighting::zone_count`].
//!
//! Storeys and vertical isolation
//! ------------------------------
//! Floors and ceilings are lighting boundaries, not just surfaces. Every room
//! floor contributes zero-thickness horizontal interfaces at the same stair-step
//! heights collision walks on, and every ceiling contributes a solid body above
//! its plane, so a fixture cannot light through a floor slab — while a raised
//! platform, a lowered basin and an intentional vertical opening stay open,
//! because an interface never occupies room air. A ceiling fixture may author a
//! world `y` to pick its storey (see [`LevelLighting::fixture_y_for`]), and
//! whole-position samples resolve their room by height as well as footprint
//! (see [`LevelLighting::room_index_at_height`]). See [`visibility`].
//!
//! Opaque geometry matters
//! -----------------------
//! An opaque wall is a lighting boundary. A fixture's local pool is tested
//! against the same solid wall slices the geometry and collision use, so a
//! fixture behind a wall does not light the room on the other side — not with
//! white light and not with colour — while a doorway, window or vent still
//! transmits light through the hole it cuts. See [`visibility`].
//!
//! Placed props are lighting boundaries too. A static prop's real model
//! triangles are merged into a handful of oriented boxes (yaw included) and
//! tested by the same pools, so a washing machine grounds its own contact
//! shadow and a couch darkens the corner behind it. Emission stays separate:
//! a prop only illuminates through the generic [`LightSource`]s its level
//! entry attaches to it, never through its material. See [`occlusion`].
//!
//! Determinism and ownership
//! -------------------------
//! Overlapping and intersecting rooms are legal level design in this game, so
//! light ownership must be defined rather than rejected: a point (and therefore
//! a fixture) belongs to the *smallest-area* room that contains it, with ties
//! resolved by the level's own room order (`rooms`, then the optional `room`).
//! When rooms share a footprint at different heights, the fixture's own mount
//! height picks its storey first — see [`LevelLighting::room_index_at_height`].
//! Each fixture therefore contributes to exactly one room area and is never
//! counted twice.
//!
//! Module layout
//! -------------
//! ```text
//! color.rs        the emitted-colour type and its sanitising
//! tuning.rs       every calibrated number, and the fixture family table
//! math.rs         the pure, bounded falloff and density maths
//! bake.rs         the bake itself and the queries it answers
//! visibility.rs   static opaque-geometry visibility for local pools and openings
//! occlusion.rs    prop occlusion boxes derived from the placed models' triangles
//! tests.rs        unit tests for the whole tree
//! ```

mod bake;
mod color;
mod light;
pub mod lightmap;
mod math;
mod occlusion;
mod tuning;
mod visibility;

#[cfg(test)]
mod tests;

pub use bake::{BakeConfig, BakedLight, LevelLighting, LightingSummary, RoomLighting};
pub use color::{LightColor, MAX_LIGHT_COLOR};
pub use light::{
    DEFAULT_LIGHT_RANGE_M, LINE_LIGHT_HALF_THICKNESS_M, LightFalloff, LightShape, LightSource,
    MAX_LIGHT_HALF_EXTENT_M, MAX_LIGHT_LENGTH_M, MAX_LIGHT_RANGE_M, MIN_LIGHT_RANGE_M,
};
pub use math::{
    ceiling_height_factor, compressed_density, effective_power, fixture_half_extents,
    fixture_half_extents_for, fixture_is_turned, light_grid_cells, room_baseline,
    sanitize_intensity, saturating_brightness, smooth_falloff, wall_light_segments,
};
pub use tuning::{
    AMBIENT_LEVEL, DEFAULT_LIGHT_COLOR, FIXTURE_DROP_M, FIXTURE_HALF_DEPTH_M, FIXTURE_HALF_WIDTH_M,
    FLUSH_MOUNT_RADIUS_M, FixtureKind, FixtureProfile, HEIGHT_FALLOFF, LIGHT_FIXTURE_IDS,
    LIGHT_GRID_CELL_M, LOCAL_LIGHT_MAX, LOCAL_LIGHT_RADIUS_M, LOCAL_LIGHT_STRENGTH, MAX_BRIGHTNESS,
    MAX_LIGHT_GRID_CELLS, MAX_LIGHT_INTENSITY, MAX_WALL_LIGHT_SEGMENTS, MIN_ROOM_AREA_M2,
    OPENING_BLEND_RADIUS_M, OPENING_BLEND_STRENGTH, OPENING_VERTICAL_FADE_M,
    REFERENCE_CEILING_HEIGHT_M, REFERENCE_LIGHT_AREA_M2, WALL_FACE_PROBE_M,
    WALL_LIGHT_DEFAULT_HEIGHT_M, ambient_color, fixture_profile, fixture_profile_for_kind,
};
pub use visibility::{QuerySite, ShadowSampling, Visibility};
