//! Bounded binary codec for layered, baked navigation (`PNV2`, version 2).
//!
//! Each horizontal XZ column holds zero or more strictly ascending walking
//! surfaces. Per-class masks describe physical body clearance; eight explicit
//! directional links per node carry continuously probed traversal. Stacked
//! floors share XZ without sharing topology. Runtime never infers connectivity
//! from nearby heights or repairs a missing record.
//!
//! Little-endian layout, without implicit padding:
//! ```text
//! magic "PNV2", version u16
//! cell_m, origin_x, origin_z f32; cells_x, cells_z, node_count u32
//! column_offsets u32[ cells_x * cells_z + 1 ]
//! class_count u32; classes { radius, height, step_height, max_slope f32 }
//! portal_count u32; portals { door_id length u32 + UTF-8 bytes }
//! nodes { y f32, flags u8, headroom_cm u16, portal u16 }
//! per class: walkable byte length u32 + ceil(node_count / 8) bytes
//! per class: region count u32 + u16[node_count]
//! per class: outgoing u32[node_count][8], u32::MAX for no link
//! ```
//!
//! Counts and the encoded-byte budget are checked before allocating class
//! runs. Offsets, strict height ordering, regions, reciprocal adjacency and
//! body traversal envelopes are validated. PNV1 records require recompilation.

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
pub const NAVIGATION_RECORD_VERSION: u16 = 2;

/// Magic identifying a compiled navigation record.
pub const NAVIGATION_MAGIC: [u8; 4] = *b"PNV2";

/// Largest accepted door id length in one portal, in bytes.
pub(crate) const MAX_NAV_DOOR_ID_BYTES: u64 = 256;

/// Cell flag bit: a walking surface exists under this cell.
pub const CELL_SURFACE: u8 = 1 << 0;

/// Sentinel for "this cell is not a door-portal cell".
pub const NO_PORTAL: u16 = u16::MAX;

/// Sentinel for "this cell has no region in this class".
pub const NO_REGION: u16 = u16::MAX;

/// Sentinel for an absent directional traversal link.
pub const NO_NAV_LINK: u32 = u32::MAX;

/// Canonical order of the eight horizontal traversal directions.
pub const NAV_DIRECTIONS: [(i32, i32); 8] = [
    (-1, 0),
    (1, 0),
    (0, -1),
    (0, 1),
    (-1, -1),
    (1, -1),
    (-1, 1),
    (1, 1),
];

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
    /// Prefix offsets for the vertically ordered spans in each XZ column.
    /// Length is `cells_x * cells_z + 1`; the final offset is the node count.
    pub column_offsets: Vec<u32>,
    /// Baked outgoing links, indexed by class, node and [`NAV_DIRECTIONS`].
    /// Each link was continuously checked against the physical geometry.
    pub links: Vec<Vec<[u32; 8]>>,
    /// Baked agent classes, in canonical order.
    pub classes: Vec<NavClass>,
    /// Walking-surface heights, ordered by column and then ascending Y.
    ///
    /// Meaningful only where [`CELL_SURFACE`] is set in `cell_flags`.
    pub cell_y: Vec<f32>,
    /// Per-node flags. See [`CELL_SURFACE`].
    pub cell_flags: Vec<u8>,
    /// Centre headroom above each surface, in centimetres.
    pub cell_headroom_cm: Vec<u16>,
    /// Door portal index per node, or [`NO_PORTAL`].
    pub cell_portal: Vec<u16>,
    /// Door portals referenced by `cell_portal`.
    pub portals: Vec<NavPortal>,
    /// Per-class walkable bitmask, node order (bit `i % 8` of byte
    /// `i / 8` is cell `i`).
    pub walkable: Vec<Vec<u8>>,
    /// Per-class region label, node order, or [`NO_REGION`].
    pub region: Vec<Vec<u16>>,
}

impl NavGrid {
    /// Retained heap storage, including spare capacity and per-class buffers.
    #[must_use]
    pub(crate) fn retained_heap_bytes(&self) -> usize {
        let mut bytes = self
            .column_offsets
            .capacity()
            .saturating_mul(4)
            .saturating_add(
                self.links
                    .capacity()
                    .saturating_mul(std::mem::size_of::<Vec<[u32; 8]>>()),
            )
            .saturating_add(self.links.iter().fold(0usize, |total, links| {
                total.saturating_add(links.capacity().saturating_mul(32))
            }))
            .saturating_add(
                self.classes
                    .capacity()
                    .saturating_mul(std::mem::size_of::<NavClass>())
                    .saturating_add(
                        self.cell_y
                            .capacity()
                            .saturating_mul(std::mem::size_of::<f32>()),
                    )
                    .saturating_add(self.cell_flags.capacity())
                    .saturating_add(
                        self.cell_headroom_cm
                            .capacity()
                            .saturating_mul(std::mem::size_of::<u16>()),
                    )
                    .saturating_add(
                        self.cell_portal
                            .capacity()
                            .saturating_mul(std::mem::size_of::<u16>()),
                    )
                    .saturating_add(
                        self.portals
                            .capacity()
                            .saturating_mul(std::mem::size_of::<NavPortal>()),
                    )
                    .saturating_add(
                        self.walkable
                            .capacity()
                            .saturating_mul(std::mem::size_of::<Vec<u8>>()),
                    )
                    .saturating_add(
                        self.region
                            .capacity()
                            .saturating_mul(std::mem::size_of::<Vec<u16>>()),
                    ),
            );
        for portal in &self.portals {
            bytes = bytes.saturating_add(portal.door.capacity());
        }
        for mask in &self.walkable {
            bytes = bytes.saturating_add(mask.capacity());
        }
        for labels in &self.region {
            bytes =
                bytes.saturating_add(labels.capacity().saturating_mul(std::mem::size_of::<u16>()));
        }
        bytes
    }

    /// Number of walking-surface nodes, including every floor in a column.
    #[must_use]
    pub fn cell_count(&self) -> usize {
        self.cell_y.len()
    }

    /// Number of horizontal XZ columns, independently of their floor count.
    #[must_use]
    pub fn column_count(&self) -> usize {
        usize::try_from(u64::from(self.cells_x).saturating_mul(u64::from(self.cells_z)))
            .unwrap_or(usize::MAX)
    }

    /// All vertically ordered nodes in a horizontal XZ cell.
    #[must_use]
    pub fn column_range(&self, cx: u32, cz: u32) -> std::ops::Range<usize> {
        if cx >= self.cells_x || cz >= self.cells_z {
            return 0..0;
        }
        let column = usize::try_from(
            u64::from(cz)
                .saturating_mul(u64::from(self.cells_x))
                .saturating_add(u64::from(cx)),
        )
        .unwrap_or(usize::MAX);
        let start = self
            .column_offsets
            .get(column)
            .copied()
            .and_then(|value| usize::try_from(value).ok())
            .unwrap_or(0);
        let end = self
            .column_offsets
            .get(column.saturating_add(1))
            .copied()
            .and_then(|value| usize::try_from(value).ok())
            .unwrap_or(start);
        start..end
    }

    /// Lowest node in a column. Use [`Self::column_range`] when height matters.
    #[must_use]
    pub fn index_of(&self, cx: u32, cz: u32) -> Option<usize> {
        self.column_range(cx, cz).next()
    }

    /// Horizontal grid coordinates of a walking-surface node.
    #[must_use]
    pub fn coordinates(&self, index: usize) -> Option<(u32, u32)> {
        if index >= self.cell_count() || self.cells_x == 0 {
            return None;
        }
        let node = u32::try_from(index).ok()?;
        let column = self
            .column_offsets
            .partition_point(|offset| *offset <= node)
            .checked_sub(1)?;
        let width = usize::try_from(self.cells_x).ok()?;
        Some((
            u32::try_from(column.checked_rem(width)?).ok()?,
            u32::try_from(column.checked_div(width)?).ok()?,
        ))
    }

    /// The baked neighbour in one canonical horizontal direction.
    #[must_use]
    pub fn link(&self, class: usize, node: usize, direction: usize) -> Option<usize> {
        let target = *self.links.get(class)?.get(node)?.get(direction)?;
        (target != NO_NAV_LINK)
            .then(|| usize::try_from(target).ok())
            .flatten()
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
        if cell_x >= self.cells_x || cell_z >= self.cells_z {
            return None;
        }
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
        u32::try_from(cell_count)
            .map_err(|error| format!("navigation node count is too large: {error}"))?,
    );
    for offset in &grid.column_offsets {
        writer.u32(*offset);
    }
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
    for class_links in &grid.links {
        for links in class_links {
            for target in links {
                writer.u32(*target);
            }
        }
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
        return Err("navigation record magic is unsupported; recompile this map with the current compiler (layered PNV2 navigation required)".to_string());
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
    let node_count = reader.count(
        u64::try_from(MAX_NAV_CELLS).unwrap_or(u64::MAX),
        "navigation node count",
    )?;
    let columns = usize::try_from(u64::from(cells_x).saturating_mul(u64::from(cells_z)))
        .map_err(|error| format!("navigation columns are too large: {error}"))?;
    if columns > MAX_NAV_CELLS {
        return Err("navigation column count exceeds the limit".to_string());
    }
    let mut column_offsets = Vec::with_capacity(columns.saturating_add(1).min(4096));
    for _ in 0..=columns {
        column_offsets.push(reader.u32()?);
    }
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
    let cell_count = node_count;
    validate_record_budget(columns, cell_count, class_count)?;
    let minimum_payload = cell_count.saturating_mul(9).saturating_add(
        class_count.saturating_mul(
            cell_count
                .saturating_mul(34)
                .saturating_add(cell_count.div_ceil(8)),
        ),
    );
    if minimum_payload > reader.remaining() {
        return Err("navigation node/link payload is truncated".to_string());
    }
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
        let labels = reader.u16s(u64::try_from(cell_count).unwrap_or(u64::MAX))?;
        if labels.len() != cell_count {
            return Err(format!(
                "navigation class region run is {} entries (expected {cell_count})",
                labels.len()
            ));
        }
        region.push(labels);
    }
    let mut links = Vec::with_capacity(class_count);
    for _ in 0..class_count {
        let mut class_links = Vec::with_capacity(cell_count.min(4096));
        for _ in 0..cell_count {
            let mut neighbours = [NO_NAV_LINK; 8];
            for target in &mut neighbours {
                *target = reader.u32()?;
            }
            class_links.push(neighbours);
        }
        links.push(class_links);
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
        column_offsets,
        links,
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
    let columns = grid.column_count();
    validate_record_budget(columns, cell_count, grid.classes.len())?;
    if grid.column_offsets.len() != columns.saturating_add(1)
        || grid.column_offsets.first().copied() != Some(0)
        || grid
            .column_offsets
            .last()
            .copied()
            .and_then(|value| usize::try_from(value).ok())
            != Some(cell_count)
        || grid
            .column_offsets
            .windows(2)
            .any(|pair| pair.first() > pair.get(1))
    {
        return Err("navigation column offsets are inconsistent".to_string());
    }
    for column in grid.column_offsets.windows(2) {
        let start = usize::try_from(column.first().copied().unwrap_or(0)).unwrap_or(usize::MAX);
        let end = usize::try_from(column.get(1).copied().unwrap_or(0)).unwrap_or(usize::MAX);
        let Some(heights) = grid.cell_y.get(start..end) else {
            return Err("navigation column addresses invalid nodes".to_string());
        };
        if heights.iter().any(|height| !height.is_finite())
            || heights.windows(2).any(|pair| pair.first() >= pair.get(1))
        {
            return Err(
                "navigation column heights are not finite and strictly ascending".to_string(),
            );
        }
    }
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
    if grid.walkable.len() != grid.classes.len()
        || grid.region.len() != grid.classes.len()
        || grid.links.len() != grid.classes.len()
    {
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
            if grid.is_walkable(class, index) != grid.region_of(class, index).is_some() {
                return Err("navigation walkable mask and region labels disagree".to_string());
            }
        }
    }
    for labels in &grid.region {
        if labels.len() != cell_count {
            return Err("navigation class region run has the wrong length".to_string());
        }
    }
    for (class, class_links) in grid.links.iter().enumerate() {
        if class_links.len() != cell_count {
            return Err("navigation class link run has the wrong length".to_string());
        }
        for (node, neighbours) in class_links.iter().enumerate() {
            let Some((cx, cz)) = grid.coordinates(node) else {
                return Err("navigation node has no column".to_string());
            };
            for (direction, target) in neighbours.iter().enumerate() {
                if *target == NO_NAV_LINK {
                    continue;
                }
                let next = usize::try_from(*target)
                    .map_err(|error| format!("navigation link exceeds address space: {error}"))?;
                if !grid.is_walkable(class, node) || !grid.is_walkable(class, next) {
                    return Err("navigation link references a non-walkable node".to_string());
                }
                let Some(&(dx, dz)) = NAV_DIRECTIONS.get(direction) else {
                    return Err("navigation link direction is invalid".to_string());
                };
                let expected = cx.checked_add_signed(dx).zip(cz.checked_add_signed(dz));
                if expected != grid.coordinates(next) {
                    return Err("navigation link crosses nonadjacent columns".to_string());
                }
                let distance = if dx != 0_i32 && dz != 0_i32 {
                    grid.cell_m * std::f32::consts::SQRT_2
                } else {
                    grid.cell_m
                };
                let Some(profile) = grid.classes.get(class) else {
                    return Err("navigation link has no class".to_string());
                };
                let rise = (grid.surface(node).unwrap_or(f32::INFINITY)
                    - grid.surface(next).unwrap_or(f32::NEG_INFINITY))
                .abs();
                if rise
                    > profile.max_slope.mul_add(distance, profile.step_height)
                        + crate::engine::collision::STEP_EPS
                {
                    return Err(
                        "navigation link exceeds its body's step/slope envelope".to_string()
                    );
                }
                if !class_links.get(next).is_some_and(|links| {
                    links.contains(&u32::try_from(node).unwrap_or(NO_NAV_LINK))
                }) {
                    return Err("navigation link has no reciprocal traversal".to_string());
                }
                if grid.region_of(class, node) != grid.region_of(class, next) {
                    return Err("navigation link crosses region labels".to_string());
                }
            }
        }
    }
    Ok(())
}

/// Checks the node, column and encoded-link budgets before allocating class runs.
///
/// # Errors
/// Returns a named error when any structural or encoded-byte cap is exceeded.
pub(crate) fn validate_record_budget(
    columns: usize,
    nodes: usize,
    classes: usize,
) -> Result<(), String> {
    if columns > MAX_NAV_CELLS || nodes > MAX_NAV_CELLS || classes > MAX_NAV_CLASSES {
        return Err("navigation column, node or class count exceeds the limit".to_string());
    }
    let estimate = columns
        .saturating_add(1)
        .saturating_mul(4)
        .saturating_add(nodes.saturating_mul(9))
        .saturating_add(
            classes.saturating_mul(nodes.saturating_mul(34).saturating_add(nodes.div_ceil(8))),
        )
        .saturating_add(MAX_NAV_PORTALS.saturating_mul(260))
        .saturating_add(classes.saturating_mul(24))
        .saturating_add(128);
    if u64::try_from(estimate).unwrap_or(u64::MAX) > MAX_NAVIGATION_BYTES {
        return Err(format!(
            "layered navigation needs at least {estimate} bytes (limit {MAX_NAVIGATION_BYTES}); raise the cell size or split the level"
        ));
    }
    Ok(())
}
