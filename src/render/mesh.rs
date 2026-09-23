//! Vertex layout, surface keys, the level mesh and GPU packing.
//!
//! Everything about *how* geometry is represented and handed to the GPU: the
//! vertex layout and its quantised packing, the surface key that decides which
//! texture a batch binds, the cullable mesh, and the packer that turns it into
//! 16-bit-indexable buffers.

use super::LevelDef;

/// Authoring/build-time vertex: exact floats, easy to reason about and to audit.
///
/// This is what the level builder, the lighting audit and every test work with.
/// It is converted to [`PackedVertex`] exactly once, when a mesh is uploaded.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub color: [f32; 4],
    pub uv: [f32; 2],
    /// Lightmap atlas coordinates, one 16-bit fixed-point value per axis
    /// covering the chart's slice of its atlas page.
    ///
    /// A vertex that is not lightmapped carries [`LIGHTMAP_NONE`] in
    /// [`Vertex::lightmap_page`] and the shader ignores these coordinates, so
    /// the two lighting paths share one buffer.
    pub lightmap: [u16; 2],
    /// Lightmap atlas page this vertex samples, or [`LIGHTMAP_NONE`] for the
    /// historical vertex-lit path.
    pub lightmap_page: u8,
}

/// Lightmap page sentinel meaning "this vertex is not lightmapped".
///
/// The shader treats any page at or above this value as "sample no lightmap and
/// use the vertex colour's baked light instead", which is what makes the
/// fallback per-vertex rather than per-draw.
pub const LIGHTMAP_NONE: u8 = u8::MAX;

impl Vertex {
    /// A vertex with no lightmap coordinates: the historical vertex-lit vertex.
    ///
    /// Used as a struct-update base (`..Vertex::UNLIT`) so a call site only
    /// names the attributes it cares about.
    pub const UNLIT: Self = Self {
        pos: [0.0; 3],
        color: [1.0; 4],
        uv: [0.0; 2],
        lightmap: [0; 2],
        lightmap_page: LIGHTMAP_NONE,
    };

    /// A vertex-lit vertex with the given position, shade and tile UV.
    #[must_use]
    pub const fn new(pos: [f32; 3], color: [f32; 4], uv: [f32; 2]) -> Self {
        Self {
            pos,
            color,
            uv,
            lightmap: [0; 2],
            lightmap_page: LIGHTMAP_NONE,
        }
    }

    /// A lightmapped vertex: its colour carries the surface tint and its baked
    /// light comes from the atlas.
    #[must_use]
    pub const fn lightmapped(
        pos: [f32; 3],
        color: [f32; 4],
        uv: [f32; 2],
        lightmap: [u16; 2],
        page: u8,
    ) -> Self {
        Self {
            pos,
            color,
            uv,
            lightmap,
            lightmap_page: page,
        }
    }

    /// True when this vertex samples the lightmap atlas.
    #[must_use]
    pub const fn is_lightmapped(&self) -> bool {
        self.lightmap_page != LIGHTMAP_NONE
    }
}

/// GPU vertex layout: 32 bytes, with no loss of achievable output.
///
/// * `pos` stays `f32` — world position precision is not negotiable, since a
///   liminal level can be over 250 m across and a centimetre of drift would move
///   geometry through walls.
/// * `uv` stays `f32` — texturing is where quantisation would actually show, and
///   tiling surfaces carry world-space coordinates that reach ±130 on a level
///   the size of the largest regression fixture.
/// * `color` becomes normalised `RGBA8`. It is a *shade* folded into the vertex
///   by the lighting bake, and that bake is bounded: `lighting::AMBIENT_LEVEL`
///   is 0.10 and `MAX_BRIGHTNESS` is 1.0, so a vertex channel only ever spans
///   [0, 1] and the smallest step is 1/255 ≈ 0.9% of the range actually used.
///   Alpha is kept because prop models carry it from their glTF `COLOR_0`.
/// * `lightmap` becomes two normalised `u16` atlas coordinates (4 bytes) and
///   `lightmap_page` a plain byte: 16 bits per axis resolves one quarter of a
///   texel on a 1024-texel atlas page, so the fixed-point step is far below what
///   the sampling filter can see.
///
/// `glVertexAttribPointer` with `normalized = true` and `GL_UNSIGNED_BYTE` is
/// core OpenGL ES 2.0, so no extension or newer context is required.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PackedVertex {
    pub pos: [f32; 3],
    pub color: [u8; 4],
    pub uv: [f32; 2],
    /// Lightmap atlas coordinates, uploaded as normalized `GL_UNSIGNED_SHORT`.
    pub lightmap: [u16; 2],
    /// Lightmap atlas page, uploaded as a plain `GL_UNSIGNED_BYTE`.
    pub lightmap_page: u8,
    /// Alignment padding so one vertex stays 32 bytes: the attribute offsets
    /// stay 4-byte aligned, which is what the Mali-400 wants.
    pub pad: [u8; 3],
}

/// Byte offset of each packed attribute, and the stride between vertices.
/// Bytes one vertex occupies in the exact (unpacked) layout, as the GL stride
/// API takes it.
///
/// Written out rather than derived from `size_of::<Vertex>()` so that
/// [`VertexLayout::stride`] can stay a `const fn`; the packed-layout test keeps
/// it equal to the struct's real size.
pub const EXACT_VERTEX_STRIDE: i32 = 44;

pub mod packed_layout {
    /// Offset of `a_pos`, in bytes.
    pub const POS_OFFSET: i32 = 0;
    /// Offset of `a_color`, in bytes.
    pub const COLOR_OFFSET: i32 = 12;
    /// Offset of `a_uv`, in bytes.
    pub const UV_OFFSET: i32 = 16;
    /// Offset of `a_lightmap_uv`, in bytes.
    pub const LIGHTMAP_OFFSET: i32 = 24;
    /// Offset of `a_lightmap_page`, in bytes.
    pub const LIGHTMAP_PAGE_OFFSET: i32 = 28;
    /// Bytes between consecutive vertices.
    pub const STRIDE: i32 = 32;
}

impl From<&Vertex> for PackedVertex {
    fn from(vertex: &Vertex) -> Self {
        Self {
            pos: vertex.pos,
            color: [
                quantize_unit(vertex.color[0]),
                quantize_unit(vertex.color[1]),
                quantize_unit(vertex.color[2]),
                quantize_unit(vertex.color[3]),
            ],
            uv: vertex.uv,
            lightmap: vertex.lightmap,
            lightmap_page: vertex.lightmap_page,
            pad: [0; 3],
        }
    }
}

impl From<Vertex> for PackedVertex {
    fn from(vertex: Vertex) -> Self {
        Self::from(&vertex)
    }
}

/// Maps a unit-interval float to a normalised byte, rounding to nearest.
///
/// The input is clamped rather than wrapped: a value outside [0, 1] (a malformed
/// level, an over-bright hand-authored shade) must stay at the closest legal
/// value instead of flipping to the opposite end of the range.
fn quantize_unit(value: f32) -> u8 {
    if value.is_nan() {
        // An undefined shade must not become a bright one.
        return 0;
    }
    // `clamp` handles the infinities by saturation, which is what "clamp" means.
    let clamped = value.clamp(0.0, 1.0);
    // `clamped` is in [0, 1], so `clamped * 255 + 0.5` is in [0.5, 255.5]: the
    // truncating cast only drops the fraction and can never leave the byte
    // range, and the value is non-negative.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let byte = clamped.mul_add(255.0, 0.5) as u8;
    byte
}

/// The exact value a normalised byte decodes to, for tests and audits.
#[must_use]
pub fn dequantize_unit(byte: u8) -> f32 {
    f32::from(byte) / 255.0
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BatchRange {
    pub start: i32,
    pub count: i32,
}

/// Aggregate span per material, covering every spatial batch of that material.
///
/// The spans are measured in **indices**, not vertices: each quad is six
/// indices whether or not indexing collapsed its corners, so "how much floor did
/// this level generate" stays comparable with the pre-indexing builds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LevelMeshBatches {
    pub floor_batch: BatchRange,
    pub ceiling_batch: BatchRange,
    pub wall_batch: BatchRange,
    pub light_batch: BatchRange,
    pub prop_batch: BatchRange,
    pub decal_batch: BatchRange,
}

/// Which surface family a static batch draws with.
///
/// The order matters: batches are emitted group-major, so a draw loop walking
/// [`LevelMesh::static_batches`] in order only rebinds its texture once per
/// group. `Light` and `PropFallback` share the unshaded light sheet — a light
/// batch with a sheet index binds its fixture's face, a bare one the white
/// sheet; `Decal` is its own pass and always drawn last.
///
/// There is deliberately no per-material variant here: *which* texture a wall,
/// floor or ceiling draws is the level's [`SurfaceKey::material`] index into
/// its resolved material table, not a compile-time family.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SurfaceKind {
    Floor,
    Ceiling,
    Wall,
    Light,
    PropFallback,
    /// Local surface decals. Drawn last, in their own pass with a depth bias,
    /// so a decal always resolves in front of the surface it lies on.
    Decal,
}

/// One surface family's slot: a material index or a sheet index.
///
/// `Floor`, `Ceiling` and `Wall` index the level's
/// [`crate::materials::MaterialTable`]; `Light` indexes the level's resolved
/// fixture sheets (one per [`crate::lighting::FixtureKind`]); `Decal` indexes
/// the level's decal sheets. [`MATERIAL_NONE`] is the sentinel for a family
/// that binds its own shared fallback sheet instead: the untextured white sheet
/// for a light's metal housing, a prop placeholder box, or the diagnostic decal
/// pattern.
pub type MaterialIndex = u16;

/// Sentinel material index for a key that does not bind a level material.
pub const MATERIAL_NONE: MaterialIndex = u16::MAX;

/// Which surface a material id resolves against.
///
/// The slot comes from the geometry being emitted (a wall face asks for the
/// wall slot), never from the material id itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaterialSlot {
    Wall,
    Floor,
    Ceiling,
}

impl MaterialSlot {
    /// The surface family this slot emits.
    #[must_use]
    pub const fn kind(self) -> SurfaceKind {
        match self {
            Self::Wall => SurfaceKind::Wall,
            Self::Floor => SurfaceKind::Floor,
            Self::Ceiling => SurfaceKind::Ceiling,
        }
    }
}

/// A batch group: one surface family plus the material index it binds.
///
/// Sorting is `(kind, material)`, so every cell of one material stays adjacent
/// in the drain order and a draw loop binds each texture once per group.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SurfaceKey {
    pub kind: SurfaceKind,
    pub material: MaterialIndex,
}

impl SurfaceKey {
    /// A key for a material-bearing surface family.
    #[must_use]
    pub const fn new(kind: SurfaceKind, material: MaterialIndex) -> Self {
        Self { kind, material }
    }

    /// A key for a family that does not bind a level material.
    #[must_use]
    pub const fn bare(kind: SurfaceKind) -> Self {
        Self {
            kind,
            material: MATERIAL_NONE,
        }
    }

    /// True when this key binds a level material.
    #[must_use]
    pub const fn has_material(self) -> bool {
        self.material != MATERIAL_NONE
    }
}

/// Every family, in draw order. `Decal` sorts last: decals are a separate pass
/// with their own depth bias, so the opaque world is already in the depth
/// buffer when they are submitted.
impl SurfaceKind {
    pub const ALL: [Self; 6] = [
        Self::Floor,
        Self::Ceiling,
        Self::Wall,
        Self::Light,
        Self::PropFallback,
        Self::Decal,
    ];
}

/// One cullable, single-draw range of static level geometry.
///
/// Geometry used to be one range per material for the entire level, which meant
/// a camera looking away from a prop field still paid its full vertex cost.
/// Splitting each material by spatial cell keeps the draw shape (one texture,
/// one buffer, one call per range) while letting the frustum drop whole cells.
///
/// `index_range` addresses the index buffer of GPU chunk `chunk`, so the GPU
/// reads the range through `glDrawElements` and shades only the distinct
/// vertices in it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StaticBatch {
    pub key: SurfaceKey,
    /// Which 16-bit-indexable GPU buffer pair this range lives in.
    pub chunk: usize,
    /// Range in that chunk's index buffer.
    pub index_range: BatchRange,
    /// Number of distinct vertices this batch indexes.
    pub vertex_count: i32,
    pub bounds: crate::spatial::Aabb,
}

/// The spatial grid a level is partitioned with.
///
/// The grid is deliberately simple — a uniform per-axis lattice over the X/Z
/// plane, no hierarchy, no occlusion queries — and its resolution adapts to the
/// level's extent so the cell count, and therefore the number of draw batches,
/// stays bounded for any level a creator ships. `LIMINAL_CELL_METRES` overrides
/// it for the debug benchmark sweep; the shipping default is the adaptive grid.
#[must_use]
pub fn spatial_cell_grid(level: &LevelDef) -> crate::spatial::CellGrid {
    let override_size = std::env::var("LIMINAL_CELL_METRES")
        .ok()
        .and_then(|value| value.trim().parse::<f32>().ok())
        .filter(|value| value.is_finite() && *value >= 1.0);
    if let Some(size) = override_size {
        return crate::spatial::CellGrid::uniform(size);
    }
    let (extent_x, extent_z) = level_extent(level);
    crate::spatial::CellGrid::for_extent(extent_x, extent_z)
}

/// World-space X/Z extent of everything a level places.
///
/// Rooms, walls, fixtures and props all contribute, so a level whose geometry
/// reaches far outside its first room still gets a grid that covers it.
fn level_extent(level: &LevelDef) -> (f32, f32) {
    let mut min_x = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut min_z = f32::INFINITY;
    let mut max_z = f32::NEG_INFINITY;
    let mut reach = |x: f32, z: f32| {
        if !x.is_finite() || !z.is_finite() {
            return;
        }
        min_x = min_x.min(x);
        max_x = max_x.max(x);
        min_z = min_z.min(z);
        max_z = max_z.max(z);
    };
    for room in level.room_iter() {
        reach(room.x, room.z);
        reach(room.x + room.width, room.z + room.depth);
    }
    for wall in &level.walls {
        reach(wall.x, wall.z);
        reach(wall.x + wall.width, wall.z + wall.depth);
    }
    for light in &level.ceiling_lights {
        reach(light.x, light.z);
    }
    for prop in &level.props {
        // The catalogue size is not available here, but a placement sitting
        // outside every room is still rare enough that a metre of margin around
        // its origin covers it.
        reach(prop.x - 1.0, prop.z - 1.0);
        reach(prop.x + 1.0, prop.z + 1.0);
    }
    for decal in &level.decals {
        let margin = decal.width.abs().max(decal.height.abs()).mul_add(0.5, 0.0);
        reach(decal.x - margin, decal.z - margin);
        reach(decal.x + margin, decal.z + margin);
    }
    if !min_x.is_finite() || !min_z.is_finite() {
        return (0.0, 0.0);
    }
    ((max_x - min_x).max(0.0), (max_z - min_z).max(0.0))
}

/// One spatially bucketed, already-indexed range of static geometry.
///
/// Ranges are produced in the order they are drawn: floor, ceiling, wall, light,
/// then placeholder prop boxes, and inside a material by ascending cell key.
#[derive(Clone, Debug, PartialEq)]
pub struct LevelMeshRange {
    pub key: SurfaceKey,
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u16>,
    pub bounds: crate::spatial::Aabb,
}

/// Static level geometry, indexed and split into cullable ranges.
///
/// The old representation was one flat vertex buffer with one draw range per
/// material; this one keeps per-range vertex/index blocks so a range can be
/// packed into a 16-bit-indexable GPU buffer without re-basing anything at draw
/// time. `batches` still carries the per-material aggregate so tests, the
/// lighting audit and the developer log keep asking "how much wall did this
/// level generate" in the same units as before (indices, six per quad).
pub struct LevelMesh {
    pub ranges: Vec<LevelMeshRange>,
    pub batches: LevelMeshBatches,
    /// Total distinct vertices across every range.
    pub vertex_count: usize,
    /// Total indices across every range.
    pub index_count: usize,
}

impl LevelMesh {
    /// Every vertex of one surface family, in draw order.
    ///
    /// The renderer never does this — it hands indices straight to the GPU — but
    /// tests and the lighting audit inspect geometry in the order it is drawn,
    /// which is what the pre-indexing vertex buffer held.
    #[must_use]
    pub fn triangles_for(&self, kind: SurfaceKind) -> Vec<Vertex> {
        let mut out = Vec::new();
        for range in self.ranges.iter().filter(|range| range.key.kind == kind) {
            out.extend(
                range
                    .indices
                    .iter()
                    .filter_map(|index| range.vertices.get(*index as usize).copied()),
            );
        }
        out
    }

    /// Alias of [`LevelMesh::triangles_for`], kept for the lighting audit and
    /// the loader's surface queries.
    #[must_use]
    pub fn triangles_for_family(&self, kind: SurfaceKind) -> Vec<Vertex> {
        self.triangles_for(kind)
    }

    /// Every vertex of one material index, in draw order, regardless of which
    /// surface family it was emitted on.
    #[must_use]
    pub fn triangles_for_material(&self, material: MaterialIndex) -> Vec<Vertex> {
        let mut out = Vec::new();
        for range in self
            .ranges
            .iter()
            .filter(|range| range.key.material == material)
        {
            out.extend(
                range
                    .indices
                    .iter()
                    .filter_map(|index| range.vertices.get(*index as usize).copied()),
            );
        }
        out
    }

    /// Indices generated for one material index.
    #[must_use]
    pub fn index_count_for_material(&self, material: MaterialIndex) -> usize {
        self.ranges
            .iter()
            .filter(|range| range.key.material == material)
            .map(|range| range.indices.len())
            .sum()
    }

    /// Every vertex of one exact surface key, in draw order.
    #[must_use]
    pub fn triangles_for_key(&self, key: SurfaceKey) -> Vec<Vertex> {
        let mut out = Vec::new();
        for range in self.ranges.iter().filter(|range| range.key == key) {
            out.extend(
                range
                    .indices
                    .iter()
                    .filter_map(|index| range.vertices.get(*index as usize).copied()),
            );
        }
        out
    }

    /// Every vertex the mesh holds, in range order.
    ///
    /// Only for tests and the lighting audit, which check that no baked colour or
    /// position is out of range anywhere in the level.
    #[must_use]
    pub fn all_vertices(&self) -> Vec<Vertex> {
        let mut out = Vec::with_capacity(self.vertex_count);
        for range in &self.ranges {
            out.extend_from_slice(&range.vertices);
        }
        out
    }

    /// Indices generated for one surface family, in the units of
    /// [`LevelMeshBatches`].
    #[must_use]
    pub fn index_count_for_family(&self, family: SurfaceKind) -> usize {
        self.ranges
            .iter()
            .filter(|range| range.key.kind == family)
            .map(|range| range.indices.len())
            .sum()
    }

    /// Indices generated for one surface family, in the same units as
    /// [`LevelMeshBatches`].
    #[must_use]
    pub fn index_count_for(&self, kind: SurfaceKind) -> usize {
        self.index_count_for_family(kind)
    }

    /// Indices generated for one exact surface key.
    #[must_use]
    pub fn index_count_for_key(&self, key: SurfaceKey) -> usize {
        self.ranges
            .iter()
            .filter(|range| range.key == key)
            .map(|range| range.indices.len())
            .sum()
    }
}

/// Concatenates spatially bucketed geometry into one vertex buffer plus its
/// cullable ranges.
///
/// Buckets arrive group-major (all floors, then all ceilings, ...) and, inside a
/// group, cell-major with cell keys sorted, so the result is byte-for-byte
/// reproducible: the same level always produces the same buffer. Materials stay
/// contiguous, which is what keeps the per-material aggregate spans in
/// [`LevelMesh::batches`] meaningful.
pub(super) fn finish_indexed_mesh(
    mut buckets: crate::spatial::SpatialBuckets<SurfaceKey>,
) -> LevelMesh {
    let drained = buckets.drain_indexed();
    let mut ranges: Vec<LevelMeshRange> = Vec::with_capacity(drained.len());
    let mut batches = LevelMeshBatches::default();
    // Aggregates live in a virtual index space that walks the ranges in draw
    // order, so "how much wall" is one contiguous span even though each range
    // owns its own small index buffer.
    let mut spans: [Option<(i32, i32)>; SurfaceKind::ALL.len()] = [None; SurfaceKind::ALL.len()];
    let mut virtual_index = 0i32;
    let mut vertex_count = 0usize;
    let mut index_count = 0usize;

    for ((key, _cell), range) in drained {
        if range.indices.is_empty() {
            continue;
        }
        let index_len = i32::try_from(range.indices.len()).unwrap_or(i32::MAX);
        let span_end = virtual_index.saturating_add(index_len);
        if let Some(slot) = spans.get_mut(key.kind as usize) {
            *slot = Some(match *slot {
                None => (virtual_index, span_end),
                Some((low, high)) => (low.min(virtual_index), high.max(span_end)),
            });
        }
        virtual_index = span_end;
        vertex_count = vertex_count.saturating_add(range.vertices.len());
        index_count = index_count.saturating_add(range.indices.len());
        ranges.push(LevelMeshRange {
            key,
            vertices: range.vertices,
            indices: range.indices,
            bounds: range.bounds,
        });
    }
    drop(buckets);

    batches.floor_batch = span_for(&spans, SurfaceKind::Floor);
    batches.ceiling_batch = span_for(&spans, SurfaceKind::Ceiling);
    batches.wall_batch = span_for(&spans, SurfaceKind::Wall);
    batches.light_batch = span_for(&spans, SurfaceKind::Light);
    batches.prop_batch = span_for(&spans, SurfaceKind::PropFallback);
    batches.decal_batch = span_for(&spans, SurfaceKind::Decal);

    LevelMesh {
        ranges,
        batches,
        vertex_count,
        index_count,
    }
}

/// One surface family's aggregate index span, or an empty range when the
/// family emitted nothing.
fn span_for(spans: &[Option<(i32, i32)>], kind: SurfaceKind) -> BatchRange {
    match spans.get(kind as usize).copied().flatten() {
        Some((start, end)) => BatchRange {
            start,
            count: end.saturating_sub(start),
        },
        None => BatchRange::default(),
    }
}

/// Packs indexed ranges into GPU buffers that stay addressable with 16-bit
/// indices.
///
/// `GL_UNSIGNED_SHORT` is the only index type OpenGL ES 2.0 guarantees without
/// an extension, and core ES 2.0 has no `glDrawElementsBaseVertex`, so an index
/// is always an offset into the bound vertex buffer. A level whose props expand
/// past 65 536 vertices therefore needs several buffer pairs rather than one;
/// this helper fills them in order and re-bases each range's indices as it goes.
#[derive(Default)]
pub(super) struct MeshPacker {
    pub(super) chunks: Vec<MeshChunk>,
}

/// One vertex/index pair, small enough for 16-bit indices.
///
/// Vertices stay in the exact build representation here; the GPU layout is
/// chosen at upload time by [`VertexLayout`].
#[derive(Default)]
pub(super) struct MeshChunk {
    pub(super) vertices: Vec<Vertex>,
    pub(super) indices: Vec<u16>,
}

/// Which GPU vertex layout to upload with.
///
/// Both layouts draw identical geometry; `Packed` is the shipping default and
/// `Exact` exists only so the debug benchmark can measure what the 36 -> 24 byte
/// reduction is worth on the same build, with every other variable held fixed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VertexLayout {
    /// `PackedVertex`: 24 bytes per vertex, colour as normalised bytes.
    Packed,
    /// `Vertex`: 36 bytes per vertex, every attribute an `f32`.
    Exact,
}

impl VertexLayout {
    /// Bytes one vertex occupies in this layout.
    #[must_use]
    pub const fn stride(self) -> i32 {
        match self {
            Self::Packed => packed_layout::STRIDE,
            Self::Exact => EXACT_VERTEX_STRIDE,
        }
    }

    /// Bytes one vertex occupies on the GPU in this layout.
    #[must_use]
    pub const fn vertex_bytes(self) -> usize {
        match self {
            Self::Packed => std::mem::size_of::<PackedVertex>(),
            Self::Exact => std::mem::size_of::<Vertex>(),
        }
    }
}

/// Where one packed range landed, in chunk-local coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PackedRange {
    pub(super) chunk: usize,
    pub(super) index_start: i32,
    pub(super) index_count: i32,
    pub(super) vertex_start: i32,
    pub(super) vertex_count: i32,
}

impl MeshPacker {
    /// Appends one indexed range, splitting it across chunks as needed.
    ///
    /// A range can be larger than the 16-bit index space — a single prop batch
    /// holding hundreds of instances comfortably is — so this walks the index
    /// list, fills the current chunk until it can take no more vertices, and
    /// starts another. Every returned placement is independently drawable and
    /// re-based into its chunk, so no index ever exceeds
    /// [`crate::spatial::MAX_INDEX_VERTICES`].
    pub(super) fn push(&mut self, vertices: &[Vertex], indices: &[u16]) -> Vec<PackedRange> {
        let mut placements: Vec<PackedRange> = Vec::new();
        if indices.is_empty() {
            return placements;
        }
        let limit = crate::spatial::MAX_INDEX_VERTICES;
        // Source vertex -> index inside the current chunk, rebuilt whenever the
        // chunk changes. `u16::MAX` means "not in this chunk yet".
        let mut remap = vec![u16::MAX; vertices.len()];

        let mut cursor = 0usize;
        while cursor < indices.len() {
            let needs_chunk = self
                .chunks
                .last()
                .is_none_or(|chunk| chunk.vertices.len() >= limit);
            if needs_chunk {
                self.chunks.push(MeshChunk::default());
                remap.fill(u16::MAX);
            }
            let Some(chunk_index) = self.chunks.len().checked_sub(1) else {
                break;
            };
            let Some(chunk) = self.chunks.get_mut(chunk_index) else {
                break;
            };
            let index_start = i32::try_from(chunk.indices.len()).unwrap_or(i32::MAX);
            let vertex_start = i32::try_from(chunk.vertices.len()).unwrap_or(i32::MAX);

            while let Some(&index) = indices.get(cursor) {
                let source = usize::from(index);
                let Some(vertex) = vertices.get(source) else {
                    // Malformed index: skip it rather than fabricating geometry.
                    cursor = cursor.saturating_add(1);
                    continue;
                };
                let Some(remapped) = remap.get_mut(source) else {
                    cursor = cursor.saturating_add(1);
                    continue;
                };
                if *remapped == u16::MAX {
                    if chunk.vertices.len() >= limit {
                        break;
                    }
                    *remapped = u16::try_from(chunk.vertices.len()).unwrap_or(u16::MAX);
                    chunk.vertices.push(*vertex);
                }
                chunk.indices.push(*remapped);
                cursor = cursor.saturating_add(1);
            }

            let Some(chunk) = self.chunks.get(chunk_index) else {
                break;
            };
            let index_end = i32::try_from(chunk.indices.len()).unwrap_or(i32::MAX);
            let index_count = index_end.saturating_sub(index_start);
            if index_count > 0 {
                placements.push(PackedRange {
                    chunk: chunk_index,
                    index_start,
                    index_count,
                    vertex_start,
                    vertex_count: i32::try_from(chunk.vertices.len())
                        .unwrap_or(i32::MAX)
                        .saturating_sub(vertex_start),
                });
            }
        }
        placements
    }

    /// Appends a range, expanding it into a flat triangle list first.
    ///
    /// Only used by the debug benchmark's `LIMINAL_BENCH_NOINDEX` mode, which
    /// measures what indexed submission is worth while every other variable
    /// (spatial batching, culling, vertex layout, draw order) is held fixed.
    pub(super) fn push_unindexed(
        &mut self,
        vertices: &[Vertex],
        indices: &[u16],
    ) -> Vec<PackedRange> {
        let mut flat: Vec<Vertex> = Vec::with_capacity(indices.len());
        let mut flat_indices: Vec<u16> = Vec::with_capacity(indices.len());
        for index in indices {
            let Some(vertex) = vertices.get(*index as usize) else {
                continue;
            };
            if flat.len() >= crate::spatial::MAX_INDEX_VERTICES {
                break;
            }
            flat_indices.push(u16::try_from(flat.len()).unwrap_or(u16::MAX));
            flat.push(*vertex);
        }
        self.push(&flat, &flat_indices)
    }

    /// Total distinct vertices across every chunk.
    pub(super) fn vertex_total(&self) -> usize {
        self.chunks.iter().map(|chunk| chunk.vertices.len()).sum()
    }

    /// Total indices across every chunk.
    pub(super) fn index_total(&self) -> usize {
        self.chunks.iter().map(|chunk| chunk.indices.len()).sum()
    }
}
