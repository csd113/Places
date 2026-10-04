//! Binary codec for the compiled navigation record.
//!
//! The offline compiler rasterises a level's authored walkable surfaces (rooms,
//! floor regions, ramps and stairs) against its real collision geometry and
//! stores the result here; the player only validates, uploads and queries it.
//! Nothing at runtime derives walkable space from geometry, and a package
//! without a navigation record is refused by the manifest.
//!
//! The mesh is a **uniform cell grid with per-class clearance**: one row-major
//! cell per `cell_m` metres over the level's floor footprint. A cell stores the
//! walking-surface height under it, the headroom above that surface, and which
//! door portal (if any) sweeps it. Each baked agent class has a bitmask of
//! cells whose body disc fits at that surface, plus a connected-region label
//! for nearest-point and escape reasoning. One grid therefore serves a small
//! rat and a tall player without rebaking; a class is baked only for an agent
//! profile the level actually authors (plus the reference humanoid).
//!
//! Layout (little-endian, byte-aligned, no implicit padding):
//!
//! ```text
//! magic           4 bytes  "PLNV"
//! version         u16      [`NAVIGATION_RECORD_VERSION`]
//! cell_m          f32
//! origin_x, origin_z   f32 x 2
//! cells_x, cells_z     u32 x 2
//! class_count     u32, then per class:
//!   radius, height, step_height, max_slope   f32 x 4
//! portal_count    u32, then per portal:
//!   door id (u32 length + UTF-8 bytes, <= 256)
//! cells_x * cells_z cells:
//!   y             f32
//!   flags         u8   (bit 0: a walking surface exists)
//!   headroom_cm   u16  (0 when no surface, else clipped at u16::MAX)
//!   portal        u16  (u16::MAX = not a portal cell)
//! per class:
//!   walkable      u32 byte length + ceil(cells/8) bytes (bit i = cell i)
//!   region        u32 count + u16 x cells (u16::MAX = no region)
//! ```
//!
//! Every count and the whole record are bounded before any allocation; a
//! malformed record is rejected with a named error, never truncated.

// Keep the binary codec and its validation in cohesive, readable routines.
// Numeric conversions and arithmetic exceptions are justified locally.
#![allow(
    clippy::missing_const_for_fn,
    clippy::too_many_lines,
    reason = "The codec uses cohesive routines for validating and encoding each complete record; const qualification is not needed for runtime binary parsing."
)]

use super::binary::{Reader, Writer};
use super::{MAX_NAV_CELLS, MAX_NAV_CLASSES, MAX_NAV_PORTALS, MAX_NAVIGATION_BYTES};

/// Version of the compiled navigation record layout.
pub const NAVIGATION_RECORD_VERSION: u16 = 1;

/// Magic identifying a compiled navigation record.
pub const NAVIGATION_MAGIC: [u8; 4] = *b"PLNV";

/// Largest accepted door id length in one portal, in bytes.
pub(crate) const MAX_NAV_DOOR_ID_BYTES: u64 = 256;

/// Cell flag bit: a walking surface exists under this cell.
pub const CELL_SURFACE: u8 = 1 << 0;

/// Sentinel for "this cell is not a door-portal cell".
pub const NO_PORTAL: u16 = u16::MAX;

/// Sentinel for "this cell has no region in this class".
pub const NO_REGION: u16 = u16::MAX;

/// One baked agent class: the body a cell's walkability was baked for.
///
/// `radius` and `height` are the moving body's disc radius and standing height;
/// `step_height` is the largest surface rise the class walks up without a
/// route, and `max_slope` the largest rise-per-run it walks. Runtime agents
/// select the class whose physical profile matches theirs exactly.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NavClass {
    /// Body disc radius in metres.
    pub radius: f32,
    /// Body height in metres.
    pub height: f32,
    /// Largest walkable surface rise, in metres.
    pub step_height: f32,
    /// Largest walkable rise per metre of run.
    pub max_slope: f32,
}

impl NavClass {
    /// True when this class bakes the same body as `other`.
    #[must_use]
    pub fn matches(&self, other: &Self) -> bool {
        (self.radius - other.radius).abs() <= 1.0e-4
            && (self.height - other.height).abs() <= 1.0e-4
            && (self.step_height - other.step_height).abs() <= 1.0e-4
            && (self.max_slope - other.max_slope).abs() <= 1.0e-4
    }

    /// Builds a class with finite, positive values; `None` when malformed.
    #[must_use]
    pub fn new(radius: f32, height: f32, step_height: f32, max_slope: f32) -> Option<Self> {
        let value = Self {
            radius,
            height,
            step_height,
            max_slope,
        };
        (value.is_valid()).then_some(value)
    }

    /// True when every field is finite and positive.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.radius.is_finite()
            && self.radius > 0.0
            && self.height.is_finite()
            && self.height > 0.0
            && self.step_height.is_finite()
            && self.step_height > 0.0
            && self.max_slope.is_finite()
            && self.max_slope > 0.0
    }
}

/// One door portal: the cells a door leaf sweeps belong to this record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavPortal {
    /// Authored door instance id.
    pub door: String,
}

/// The baked cell navigation mesh of one world.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct NavGrid {
    /// Cell edge length in metres.
    pub cell_m: f32,
    /// World X of the grid's low corner.
    pub origin_x: f32,
    /// World Z of the grid's low corner.
    pub origin_z: f32,
    /// Cells along X.
    pub cells_x: u32,
    /// Cells along Z.
    pub cells_z: u32,
    /// Baked agent classes, in canonical order.
    pub classes: Vec<NavClass>,
    /// Cell walking-surface height, row-major (`cells_z` rows of `cells_x`).
    ///
    /// Meaningful only where [`CELL_SURFACE`] is set in `cell_flags`.
    pub cell_y: Vec<f32>,
    /// Per-cell flags, row-major. See [`CELL_SURFACE`].
    pub cell_flags: Vec<u8>,
    /// Headroom above the surface, in centimetres, row-major.
    pub cell_headroom_cm: Vec<u16>,
    /// Door portal index per cell, row-major, or [`NO_PORTAL`].
    pub cell_portal: Vec<u16>,
    /// Door portals referenced by `cell_portal`.
    pub portals: Vec<NavPortal>,
    /// Per-class walkable bitmask, row-major bit order (bit `i % 8` of byte
    /// `i / 8` is cell `i`).
    pub walkable: Vec<Vec<u8>>,
    /// Per-class region label, row-major, or [`NO_REGION`].
    pub region: Vec<Vec<u16>>,
}

impl NavGrid {
    /// Number of cells in the grid.
    #[must_use]
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "The product of two u32 values fits in u64; conversion to the platform's collection size remains checked."
    )]
    pub fn cell_count(&self) -> usize {
        usize::try_from(u64::from(self.cells_x) * u64::from(self.cells_z)).unwrap_or(usize::MAX)
    }

    /// The row-major index of a cell, when it is inside the grid.
    #[must_use]
    pub fn index_of(&self, cx: u32, cz: u32) -> Option<usize> {
        if cx >= self.cells_x || cz >= self.cells_z {
            return None;
        }
        Some(self.index_unchecked(cx, cz))
    }

    /// The row-major index of an in-grid cell, without bounds checks.
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Even (u32::MAX * u32::MAX) + u32::MAX fits in u64; the subsequent usize conversion handles smaller platforms."
    )]
    fn index_unchecked(&self, cx: u32, cz: u32) -> usize {
        usize::try_from(u64::from(cz) * u64::from(self.cells_x) + u64::from(cx))
            .unwrap_or(usize::MAX)
    }

    /// The cell containing `(x, z)`, when the point is inside the grid.
    #[must_use]
    #[expect(
        clippy::cast_possible_truncation,
        clippy::as_conversions,
        reason = "Floored coordinates use native saturating conversion, then checked u32 conversion and grid bounds reject oversized positions; loaded grid metadata is validated finite with positive spacing."
    )]
    pub fn cell_at(&self, x: f32, z: f32) -> Option<(u32, u32)> {
        if !x.is_finite() || !z.is_finite() {
            return None;
        }
        let local_x = (x - self.origin_x) / self.cell_m;
        let local_z = (z - self.origin_z) / self.cell_m;
        if local_x < 0.0 || local_z < 0.0 {
            return None;
        }
        let cx = local_x.floor();
        let cz = local_z.floor();
        let cell_x = u32::try_from(cx as i64).ok()?;
        let cell_z = u32::try_from(cz as i64).ok()?;
        let _index_of_status = self.index_of(cell_x, cell_z)?;
        Some((cell_x, cell_z))
    }

    /// The cell centre of an in-grid cell.
    #[must_use]
    #[expect(
        clippy::cast_precision_loss,
        clippy::as_conversions,
        reason = "In-grid coordinates under the validated 2^21-cell cap convert exactly; arbitrary u32 coordinates supplied to this public helper retain their original finite rounded f32 view."
    )]
    pub fn cell_center(&self, cx: u32, cz: u32) -> (f32, f32) {
        let half = self.cell_m * 0.5;
        (
            (cx as f32).mul_add(self.cell_m, self.origin_x) + half,
            (cz as f32).mul_add(self.cell_m, self.origin_z) + half,
        )
    }

    /// True when a cell carries a walking surface.
    #[must_use]
    pub fn has_surface(&self, index: usize) -> bool {
        self.cell_flags
            .get(index)
            .is_some_and(|flags| flags & CELL_SURFACE != 0)
    }

    /// The cell's walking-surface height, when it has one.
    #[must_use]
    pub fn surface(&self, index: usize) -> Option<f32> {
        if !self.has_surface(index) {
            return None;
        }
        self.cell_y.get(index).copied().filter(|y| y.is_finite())
    }

    /// True when `class` can stand on this cell.
    #[must_use]
    pub fn is_walkable(&self, class: usize, index: usize) -> bool {
        let Some(byte) = self
            .walkable
            .get(class)
            .and_then(|mask| mask.get(index / 8))
        else {
            return false;
        };
        byte & (1 << (index % 8)) != 0
    }

    /// The cell's region label for `class`.
    #[must_use]
    pub fn region_of(&self, class: usize, index: usize) -> Option<u16> {
        let region = self
            .region
            .get(class)
            .and_then(|labels| labels.get(index))
            .copied()?;
        (region != NO_REGION).then_some(region)
    }

    /// The class index whose physical body matches `profile`, if any.
    #[must_use]
    pub fn class_index(&self, profile: &NavClass) -> Option<usize> {
        self.classes.iter().position(|class| class.matches(profile))
    }
}

/// Encodes a compiled navigation record.
///
/// # Errors
///
/// Returns a named error when a section exceeds the format's count or byte
/// limits, or the grid is internally inconsistent.
pub fn write_navigation(grid: &NavGrid) -> Result<Vec<u8>, String> {
    validate_navigation(grid)?;
    let cell_count = grid.cell_count();
    let mut writer = Writer::with_capacity(cell_count.saturating_mul(8).saturating_add(256));
    writer.bytes(&NAVIGATION_MAGIC);
    writer.u16(NAVIGATION_RECORD_VERSION);
    writer.f32(grid.cell_m);
    writer.f32(grid.origin_x);
    writer.f32(grid.origin_z);
    writer.u32(grid.cells_x);
    writer.u32(grid.cells_z);
    writer.u32(
        u32::try_from(grid.classes.len())
            .map_err(|error| format!("navigation record has too many classes: {error}"))?,
    );
    for class in &grid.classes {
        writer.f32(class.radius);
        writer.f32(class.height);
        writer.f32(class.step_height);
        writer.f32(class.max_slope);
    }
    writer.u32(
        u32::try_from(grid.portals.len())
            .map_err(|error| format!("navigation record has too many portals: {error}"))?,
    );
    for portal in &grid.portals {
        if u64::try_from(portal.door.len()).unwrap_or(u64::MAX) > MAX_NAV_DOOR_ID_BYTES {
            return Err(format!(
                "navigation portal door id is {} bytes (limit {MAX_NAV_DOOR_ID_BYTES})",
                portal.door.len()
            ));
        }
        writer.str(&portal.door)?;
    }
    for index in 0..cell_count {
        writer.f32(grid.cell_y.get(index).copied().unwrap_or(0.0));
        writer.u8(grid.cell_flags.get(index).copied().unwrap_or(0));
        writer.u16(grid.cell_headroom_cm.get(index).copied().unwrap_or(0));
        writer.u16(grid.cell_portal.get(index).copied().unwrap_or(NO_PORTAL));
    }
    for mask in &grid.walkable {
        writer.blob(mask)?;
    }
    for labels in &grid.region {
        writer.u16s(labels)?;
    }
    let bytes = writer.into_bytes();
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_NAVIGATION_BYTES {
        return Err(format!(
            "navigation record is {} bytes (limit {MAX_NAVIGATION_BYTES})",
            bytes.len()
        ));
    }
    Ok(bytes)
}

/// Decodes and validates a compiled navigation record.
///
/// # Errors
///
/// Returns a named error when the record exceeds the byte limit, has the wrong
/// magic or version, is truncated, declares an out-of-range count, holds a
/// non-finite or inconsistent value, or has trailing bytes.
pub fn read_navigation(bytes: &[u8]) -> Result<NavGrid, String> {
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_NAVIGATION_BYTES {
        return Err(format!(
            "navigation record is {} bytes (limit {MAX_NAVIGATION_BYTES})",
            bytes.len()
        ));
    }
    let mut reader = Reader::new(bytes);
    if reader.bytes(4)? != NAVIGATION_MAGIC {
        return Err("navigation record has the wrong magic".to_string());
    }
    let version = reader.u16()?;
    if version != NAVIGATION_RECORD_VERSION {
        return Err(format!(
            "navigation record version {version} is not supported (this build reads \
             {NAVIGATION_RECORD_VERSION})"
        ));
    }
    let cell_m = reader.f32()?;
    let origin_x = reader.f32()?;
    let origin_z = reader.f32()?;
    let cells_x = reader.u32()?;
    let cells_z = reader.u32()?;
    let class_count = reader.count(
        u64::try_from(MAX_NAV_CLASSES).unwrap_or(u64::MAX),
        "navigation class count",
    )?;
    let mut classes = Vec::with_capacity(class_count);
    for _ in 0..class_count {
        let class = NavClass {
            radius: reader.f32()?,
            height: reader.f32()?,
            step_height: reader.f32()?,
            max_slope: reader.f32()?,
        };
        if !class.is_valid() {
            return Err("navigation class has a non-finite or non-positive field".to_string());
        }
        classes.push(class);
    }
    let portal_count = reader.count(
        u64::try_from(MAX_NAV_PORTALS).unwrap_or(u64::MAX),
        "navigation portal count",
    )?;
    let mut portals = Vec::with_capacity(portal_count);
    for _ in 0..portal_count {
        portals.push(NavPortal {
            door: reader.str(MAX_NAV_DOOR_ID_BYTES)?,
        });
    }
    let cells = u64::from(cells_x)
        .checked_mul(u64::from(cells_z))
        .ok_or_else(|| "navigation grid dimensions overflow".to_string())?;
    if cells > u64::try_from(MAX_NAV_CELLS).unwrap_or(u64::MAX) {
        return Err(format!(
            "navigation grid declares {cells} cells (limit {MAX_NAV_CELLS})"
        ));
    }
    if cells > 0 && (class_count == 0 || !cell_m.is_finite() || cell_m <= 0.0) {
        return Err("navigation grid has no classes or a non-positive cell size".to_string());
    }
    if !origin_x.is_finite() || !origin_z.is_finite() {
        return Err("navigation grid origin is not finite".to_string());
    }
    let cell_count =
        usize::try_from(cells).map_err(|error| format!("navigation grid is too large: {error}"))?;
    let mut cell_y = Vec::with_capacity(cell_count.min(4096));
    let mut cell_flags = Vec::with_capacity(cell_count.min(4096));
    let mut cell_headroom_cm = Vec::with_capacity(cell_count.min(4096));
    let mut cell_portal = Vec::with_capacity(cell_count.min(4096));
    for _ in 0..cell_count {
        let y = reader.f32()?;
        let flags = reader.u8()?;
        let headroom = reader.u16()?;
        let portal = reader.u16()?;
        if flags & CELL_SURFACE != 0 && !y.is_finite() {
            return Err("navigation cell has a non-finite surface height".to_string());
        }
        if u32::from(portal) != u32::from(NO_PORTAL) && usize::from(portal) >= portal_count {
            return Err("navigation cell references an unknown portal".to_string());
        }
        cell_y.push(y);
        cell_flags.push(flags);
        cell_headroom_cm.push(headroom);
        cell_portal.push(portal);
    }
    let mask_bytes = cell_count.div_ceil(8);
    let mut walkable = Vec::with_capacity(class_count);
    for _ in 0..class_count {
        let mask = reader.blob(MAX_NAVIGATION_BYTES)?;
        if mask.len() != mask_bytes {
            return Err(format!(
                "navigation class mask is {} bytes (expected {mask_bytes})",
                mask.len()
            ));
        }
        walkable.push(mask);
    }
    let mut region = Vec::with_capacity(class_count);
    for _ in 0..class_count {
        let labels = reader.u16s(cells)?;
        if labels.len() != cell_count {
            return Err(format!(
                "navigation class region run is {} entries (expected {cell_count})",
                labels.len()
            ));
        }
        region.push(labels);
    }
    if !reader.is_empty() {
        return Err(format!(
            "navigation record has {} trailing bytes",
            reader.remaining()
        ));
    }
    let grid = NavGrid {
        cell_m,
        origin_x,
        origin_z,
        cells_x,
        cells_z,
        classes,
        cell_y,
        cell_flags,
        cell_headroom_cm,
        cell_portal,
        portals,
        walkable,
        region,
    };
    validate_navigation(&grid)?;
    Ok(grid)
}

/// True when every structural invariant of the grid holds.
///
/// # Errors
///
/// Returns a named error for the first violated invariant.
pub fn validate_navigation(grid: &NavGrid) -> Result<(), String> {
    if !grid.cell_m.is_finite() || grid.cell_m <= 0.0 {
        return Err("navigation cell size must be finite and positive".to_string());
    }
    if !grid.origin_x.is_finite() || !grid.origin_z.is_finite() {
        return Err("navigation origin must be finite".to_string());
    }
    if grid.classes.is_empty() || grid.classes.len() > MAX_NAV_CLASSES {
        return Err(format!(
            "navigation record has {} classes (allowed 1..={MAX_NAV_CLASSES})",
            grid.classes.len()
        ));
    }
    for class in &grid.classes {
        if !class.is_valid() {
            return Err("navigation class has a non-finite or non-positive field".to_string());
        }
    }
    if grid.portals.len() > MAX_NAV_PORTALS {
        return Err(format!(
            "navigation record has {} portals (limit {MAX_NAV_PORTALS})",
            grid.portals.len()
        ));
    }
    for portal in &grid.portals {
        if portal.door.is_empty()
            || u64::try_from(portal.door.len()).unwrap_or(u64::MAX) > MAX_NAV_DOOR_ID_BYTES
        {
            return Err("navigation portal has an empty or oversized door id".to_string());
        }
    }
    let cell_count = grid.cell_count();
    if cell_count > MAX_NAV_CELLS {
        return Err(format!(
            "navigation grid declares {cell_count} cells (limit {MAX_NAV_CELLS})"
        ));
    }
    if grid.cell_y.len() != cell_count
        || grid.cell_flags.len() != cell_count
        || grid.cell_headroom_cm.len() != cell_count
        || grid.cell_portal.len() != cell_count
    {
        return Err("navigation cell runs disagree with the grid dimensions".to_string());
    }
    for (index, portal) in grid.cell_portal.iter().enumerate() {
        if *portal != NO_PORTAL && usize::from(*portal) >= grid.portals.len() {
            return Err("navigation cell references an unknown portal".to_string());
        }
        if grid
            .cell_flags
            .get(index)
            .is_some_and(|flags| flags & CELL_SURFACE == 0)
            && *portal != NO_PORTAL
        {
            return Err("navigation portal cell has no surface".to_string());
        }
    }
    if grid.walkable.len() != grid.classes.len() || grid.region.len() != grid.classes.len() {
        return Err("navigation class runs disagree with the class list".to_string());
    }
    let mask_bytes = cell_count.div_ceil(8);
    for (class, mask) in grid.walkable.iter().enumerate() {
        if mask.len() != mask_bytes {
            return Err("navigation class mask has the wrong length".to_string());
        }
        for index in 0..cell_count {
            if grid.is_walkable(class, index) && !grid.has_surface(index) {
                return Err("navigation walkable cell has no surface".to_string());
            }
        }
    }
    for labels in &grid.region {
        if labels.len() != cell_count {
            return Err("navigation class region run has the wrong length".to_string());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    // Test code: unwrap/expect, indexing and permissive arithmetic are
    // idiomatic here; the production lints stay enforced everywhere else.
    #![allow(
        clippy::arithmetic_side_effects,
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::panic,
        clippy::unwrap_used,
        reason = "Regression fixtures assert exact reference results and fail on invalid setup; these exceptions are confined to tests"
    )]

    use super::*;

    fn tiny_grid() -> NavGrid {
        let cells = 4usize;
        let mut walkable = vec![vec![0u8; 1], vec![0u8; 1]];
        walkable[0][0] = 0b0000_1111;
        walkable[1][0] = 0b0000_0011;
        NavGrid {
            cell_m: 0.5,
            origin_x: -1.0,
            origin_z: -1.0,
            cells_x: 4,
            cells_z: 1,
            classes: vec![
                NavClass::new(0.3, 1.8, 0.4, 2.0).unwrap(),
                NavClass::new(0.1, 0.2, 0.2, 1.5).unwrap(),
            ],
            cell_y: vec![0.0, 0.0, 0.4, 0.8],
            cell_flags: vec![CELL_SURFACE; cells],
            cell_headroom_cm: vec![250, 250, 250, 250],
            cell_portal: vec![NO_PORTAL, 0, NO_PORTAL, NO_PORTAL],
            portals: vec![NavPortal {
                door: "hall_door".to_string(),
            }],
            walkable,
            region: vec![vec![0, 0, 0, 0], vec![0, 0, NO_REGION, NO_REGION]],
        }
    }

    #[test]
    fn round_trips_every_section() {
        let grid = tiny_grid();
        let bytes = write_navigation(&grid).expect("the record encodes");
        let decoded = read_navigation(&bytes).expect("the record decodes");
        assert_eq!(decoded, grid);
    }

    #[test]
    fn rejects_the_wrong_magic_and_version() {
        let bytes = write_navigation(&tiny_grid()).expect("encodes");
        let mut bad = bytes.clone();
        bad[0] = b'X';
        assert!(read_navigation(&bad).is_err());
        let mut wrong_version = bytes;
        wrong_version[4] = 2;
        assert!(read_navigation(&wrong_version).is_err());
    }

    #[test]
    fn rejects_truncation_and_trailing_bytes() {
        let bytes = write_navigation(&tiny_grid()).expect("encodes");
        assert!(read_navigation(&bytes[..bytes.len() - 1]).is_err());
        let mut longer = bytes;
        longer.push(0);
        assert!(read_navigation(&longer).is_err());
    }

    #[test]
    fn rejects_a_cell_referencing_an_unknown_portal() {
        let mut grid = tiny_grid();
        grid.cell_portal[2] = 7;
        assert!(write_navigation(&grid).is_err());
    }

    #[test]
    fn rejects_a_walkable_cell_without_a_surface() {
        let mut grid = tiny_grid();
        grid.cell_flags[0] = 0;
        assert!(write_navigation(&grid).is_err());
    }

    #[test]
    fn rejects_a_mask_of_the_wrong_length() {
        let mut grid = tiny_grid();
        grid.walkable[0].push(0);
        assert!(write_navigation(&grid).is_err());
    }

    #[test]
    fn rejects_a_class_with_a_bad_profile() {
        let mut grid = tiny_grid();
        grid.classes[0].radius = 0.0;
        assert!(write_navigation(&grid).is_err());
    }
}
