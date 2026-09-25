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
/// The renderer converts it to its GPU vertex layout exactly once, when a mesh
/// is uploaded.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub color: [f32; 4],
    pub uv: [f32; 2],
    /// Geometric normal of the surface this vertex belongs to, unit length.
    ///
    /// Computed once per range from the emitted triangles
    /// ([`compute_surface_frames`]), never authored: the mesh builder emits
    /// quads and the frame falls out of their winding.
    pub normal: [f32; 3],
    /// Tangent along the surface's `u` direction, unit length and orthogonal
    /// to [`Vertex::normal`].
    pub tangent: [f32; 3],
    /// Sign of the bitangent: `cross(normal, tangent) * handedness` is the
    /// surface's `v` direction. Needed so a mirrored UV layout flips the normal
    /// map instead of tilting it the wrong way.
    pub handedness: f32,
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
    /// names the attributes it cares about. The default frame points along +Z
    /// with +X as its tangent, which is only ever a placeholder: every range
    /// gets its real frame from [`compute_surface_frames`] before upload, and
    /// the UI (which never reads a normal) keeps this one.
    pub const UNLIT: Self = Self {
        pos: [0.0; 3],
        color: [1.0; 4],
        uv: [0.0; 2],
        normal: [0.0, 0.0, 1.0],
        tangent: [1.0, 0.0, 0.0],
        handedness: 1.0,
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
            ..Self::UNLIT
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
            ..Self::UNLIT
        }
    }

    /// True when this vertex samples the lightmap atlas.
    #[must_use]
    pub const fn is_lightmapped(&self) -> bool {
        self.lightmap_page != LIGHTMAP_NONE
    }
}

/// Maps a unit-interval float to a normalised byte, rounding to nearest.
///
/// The input is clamped rather than wrapped: a value outside [0, 1] (a malformed
/// level, an over-bright hand-authored shade) must stay at the closest legal
/// value instead of flipping to the opposite end of the range.
///
/// The wgpu world vertex declares `Unorm8x4`, so a surface sees exactly
/// `byte / 255`.
#[must_use]
pub fn quantize_unit(value: f32) -> u8 {
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

/// A per-surface shine override, quantised to whole percent.
///
/// A [`SurfaceKey`] is compared, ordered and hashed to batch geometry, and an
/// `f32` is none of those things. One percent steps are far below the visible
/// difference in a material's response, so the override is stored as this
/// small integer and expanded back to a shine (and its inverse, the shader's
/// roughness) at draw time. The material *default* shine is not part of the
/// key: it lives in the resolved material, and only an authored override needs
/// to separate one surface from another.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SurfaceShine(u8);

impl SurfaceShine {
    /// Fully matte (`shine = 0.0`).
    pub const MATTE: Self = Self(0);
    /// Extremely glossy (`shine = 1.0`). Not a mirror: mirrors are the planar
    /// reflection mode, a separate material behaviour.
    pub const GLOSS: Self = Self(100);

    /// Quantises an authored `0.0..=1.0` shine to whole percent.
    ///
    /// Out-of-range and non-finite input saturates: the loader already rejects
    /// it for levels, and a material-level value that reached here must not
    /// become a NaN.
    #[must_use]
    pub fn from_unit(shine: f32) -> Self {
        if !shine.is_finite() {
            return Self::MATTE;
        }
        // The clamp keeps the scaled value inside `0..=100`, so the cast
        // neither wraps, truncates a meaningful fraction nor loses a sign.
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss // non-negative by the clamp
        )]
        Self((shine.clamp(0.0, 1.0) * 100.0).round() as u8)
    }

    /// The author-facing shine, `0.0..=1.0`.
    #[must_use]
    pub fn unit(self) -> f32 {
        f32::from(self.0) / 100.0
    }

    /// The shader-facing roughness, `1.0 - shine`.
    #[must_use]
    pub fn roughness(self) -> f32 {
        1.0 - self.unit()
    }
}

/// A batch group: one surface family, the material index it binds, and any
/// per-surface shine override.
///
/// Sorting is `(kind, material, shine)`, so every cell of one material stays
/// adjacent in the drain order and a draw loop binds each texture once per
/// group. Two surfaces of the same material with different authored shine are
/// separate groups, which is exactly one material state change.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SurfaceKey {
    pub kind: SurfaceKind,
    pub material: MaterialIndex,
    /// Per-surface shine override; `None` keeps the material's default.
    pub shine: Option<SurfaceShine>,
}

impl SurfaceKey {
    /// A key for a material-bearing surface family.
    #[must_use]
    pub const fn new(kind: SurfaceKind, material: MaterialIndex) -> Self {
        Self {
            kind,
            material,
            shine: None,
        }
    }

    /// A key with an authored per-surface shine override.
    #[must_use]
    pub const fn with_shine(
        kind: SurfaceKind,
        material: MaterialIndex,
        shine: Option<SurfaceShine>,
    ) -> Self {
        Self {
            kind,
            material,
            shine,
        }
    }

    /// A key for a family that does not bind a level material.
    #[must_use]
    pub const fn bare(kind: SurfaceKind) -> Self {
        Self {
            kind,
            material: MATERIAL_NONE,
            shine: None,
        }
    }

    /// True when this key binds a level material.
    #[must_use]
    pub const fn has_material(self) -> bool {
        self.material != MATERIAL_NONE
    }

    /// The shader-facing roughness this surface draws with.
    ///
    /// An authored per-surface `shine` override wins; otherwise the material's
    /// own resolved roughness applies. One place decides this, so the sheen and
    /// the reflection the shader derives from it can never disagree.
    #[must_use]
    pub fn roughness(self, material_roughness: f32) -> f32 {
        self.shine
            .map_or(material_roughness, SurfaceShine::roughness)
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

/// The spatial grid a level is partitioned with.
///
/// The grid is deliberately simple — a uniform per-axis lattice over the X/Z
/// plane, no hierarchy, no occlusion queries — and its resolution adapts to the
/// level's extent so the cell count, and therefore the number of draw batches,
/// stays bounded for any level a creator ships. `PLACES_CELL_METRES` overrides
/// it for the debug benchmark sweep; the shipping default is the adaptive grid.
#[must_use]
pub fn spatial_cell_grid(level: &LevelDef) -> crate::spatial::CellGrid {
    let override_size = std::env::var("PLACES_CELL_METRES")
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
///
/// Every range's per-vertex geometric frame is computed here, once, from the
/// range's own triangles (see [`compute_surface_frames`]): the emitters stay
/// free of normal arithmetic and no emitter can forget to set a normal.
pub fn finish_indexed_mesh(mut buckets: crate::spatial::SpatialBuckets<SurfaceKey>) -> LevelMesh {
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
        let mut vertices = range.vertices;
        compute_surface_frames(&mut vertices, &range.indices);
        let index_len = i32::try_from(range.indices.len()).unwrap_or(i32::MAX);
        let span_end = virtual_index.saturating_add(index_len);
        if let Some(slot) = spans.get_mut(key.kind as usize) {
            *slot = Some(match *slot {
                None => (virtual_index, span_end),
                Some((low, high)) => (low.min(virtual_index), high.max(span_end)),
            });
        }
        virtual_index = span_end;
        vertex_count = vertex_count.saturating_add(vertices.len());
        index_count = index_count.saturating_add(range.indices.len());
        ranges.push(LevelMeshRange {
            key,
            vertices,
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

/// Computes the per-vertex geometric frame of one indexed range.
///
/// The mesh builder emits *geometry* (positions, colours, UVs, lightmap
/// coordinates); the frame — normal, tangent and bitangent sign — is derived
/// here from the range's own triangles, so every surface family gets a correct
/// frame without a single emitter knowing about normals, and a future emitter
/// cannot forget one.
///
/// * **Normal** is the area-weighted average of the adjacent triangle normals.
///   The builder emits each quad with its own four vertices, so a planar quad
///   resolves to exactly its geometric normal, and a curved patch (a gable
///   slope, a prop box face) smooths within its own patch only.
/// * **Tangent** is the UV-space `u` derivative, Gram-Schmidt-orthogonalised
///   against the normal, which is what a normal map needs to be oriented with
///   the surface's own tiling.
/// * **Handedness** is the sign of the UV-space `v` derivative against
///   `cross(normal, tangent)`: `-1` for a mirrored UV layout, so a normal map
///   tilts the same way on both sides of a mirrored seam instead of inverting.
///
/// Degenerate triangles and degenerate UVs are skipped rather than propagated:
/// a vertex that ends up with no usable frame keeps a defined, unit-length one.
fn compute_surface_frames(vertices: &mut [Vertex], indices: &[u16]) {
    let count = vertices.len();
    let mut normals = vec![[0.0f32; 3]; count];
    let mut tangents = vec![[0.0f32; 3]; count];
    let mut bitangents = vec![[0.0f32; 3]; count];

    for triangle in indices.as_chunks::<3>().0 {
        let [a, b, c] = *triangle;
        let (Some(pa), Some(pb), Some(pc)) = (
            vertices.get(usize::from(a)).map(|vertex| vertex.pos),
            vertices.get(usize::from(b)).map(|vertex| vertex.pos),
            vertices.get(usize::from(c)).map(|vertex| vertex.pos),
        ) else {
            continue;
        };
        let edge1 = sub3(pb, pa);
        let edge2 = sub3(pc, pa);
        let face = cross3(edge1, edge2);
        let (Some(ua), Some(ub), Some(uc)) = (
            vertices.get(usize::from(a)).map(|vertex| vertex.uv),
            vertices.get(usize::from(b)).map(|vertex| vertex.uv),
            vertices.get(usize::from(c)).map(|vertex| vertex.uv),
        ) else {
            continue;
        };
        let duv1 = [ub[0] - ua[0], ub[1] - ua[1]];
        let duv2 = [uc[0] - ua[0], uc[1] - ua[1]];
        let determinant = duv2[0].mul_add(-duv1[1], duv1[0] * duv2[1]);
        // A UV-degenerate triangle (a zero-area UV, a seam point) contributes no
        // usable tangent, but its geometry still contributes a normal.
        let uv_ok = determinant.is_finite() && determinant.abs() > 1.0e-12;
        let scale = if uv_ok { 1.0 / determinant } else { 0.0 };
        let tangent = [
            edge2[0].mul_add(-duv1[1], edge1[0] * duv2[1]) * scale,
            edge2[1].mul_add(-duv1[1], edge1[1] * duv2[1]) * scale,
            edge2[2].mul_add(-duv1[1], edge1[2] * duv2[1]) * scale,
        ];
        let bitangent = [
            edge1[0].mul_add(-duv2[0], edge2[0] * duv1[0]) * scale,
            edge1[1].mul_add(-duv2[0], edge2[1] * duv1[0]) * scale,
            edge1[2].mul_add(-duv2[0], edge2[2] * duv1[0]) * scale,
        ];
        for index in [a, b, c] {
            let slot = usize::from(index);
            if let Some(accumulator) = normals.get_mut(slot) {
                add3_assign(accumulator, face);
            }
            if uv_ok {
                if let Some(accumulator) = tangents.get_mut(slot) {
                    add3_assign(accumulator, tangent);
                }
                if let Some(accumulator) = bitangents.get_mut(slot) {
                    add3_assign(accumulator, bitangent);
                }
            }
        }
    }

    for (index, vertex) in vertices.iter_mut().enumerate() {
        let normal = normals
            .get(index)
            .copied()
            .and_then(normalize3)
            .unwrap_or([0.0, 0.0, 1.0]);
        let raw_tangent = tangents.get(index).copied().unwrap_or_default();
        // Gram-Schmidt: the component along the normal is not part of the
        // surface's tangent plane.
        let projected = sub3(raw_tangent, scale3(normal, dot3(normal, raw_tangent)));
        let tangent = normalize3(projected)
            .or_else(|| normalize3(cross3([0.0, 1.0, 0.0], normal)))
            .or_else(|| normalize3(cross3([1.0, 0.0, 0.0], normal)))
            .unwrap_or([1.0, 0.0, 0.0]);
        let handedness = match bitangents.get(index).copied().and_then(normalize3) {
            Some(bitangent) if dot3(cross3(normal, tangent), bitangent) < 0.0 => -1.0,
            Some(_) | None => 1.0,
        };
        vertex.normal = normal;
        vertex.tangent = tangent;
        vertex.handedness = handedness;
    }
}

/// `a - b`.
fn sub3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// `a * scale`.
fn scale3(a: [f32; 3], scale: f32) -> [f32; 3] {
    [a[0] * scale, a[1] * scale, a[2] * scale]
}

/// `a + b`, in place.
fn add3_assign(accumulator: &mut [f32; 3], value: [f32; 3]) {
    for (slot, value) in accumulator.iter_mut().zip(value) {
        *slot += value;
    }
}

/// The dot product of two three-component vectors.
fn dot3(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0].mul_add(b[0], a[1].mul_add(b[1], a[2] * b[2]))
}

/// The cross product of two three-component vectors.
fn cross3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1].mul_add(b[2], -(a[2] * b[1])),
        a[2].mul_add(b[0], -(a[0] * b[2])),
        a[0].mul_add(b[1], -(a[1] * b[0])),
    ]
}

/// A unit vector in the direction of `value`, or `None` when it has no
/// direction (or is not finite).
fn normalize3(value: [f32; 3]) -> Option<[f32; 3]> {
    if !value.iter().all(|channel| channel.is_finite()) {
        return None;
    }
    let length = dot3(value, value).sqrt();
    if !length.is_finite() || length <= 1.0e-12 {
        return None;
    }
    Some(scale3(value, 1.0 / length))
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
/// The renderer-neutral mesh format is built on 16-bit indices (the historical
/// OpenGL ES 2.0 floor, with no base-vertex offset), so an index is always an
/// offset into the bound vertex buffer. A level whose props expand past
/// 65 536 vertices therefore needs several buffer pairs rather than one;
/// this helper fills them in order and re-bases each range's indices as it goes.
#[derive(Default)]
pub struct MeshPacker {
    pub chunks: Vec<MeshChunk>,
}

/// One vertex/index pair, small enough for 16-bit indices.
///
/// Vertices stay in the exact build representation here; the renderer chooses
/// the GPU layout at upload time.
#[derive(Default)]
pub struct MeshChunk {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u16>,
}

/// Where one packed range landed, in chunk-local coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PackedRange {
    pub chunk: usize,
    pub index_start: i32,
    pub index_count: i32,
    pub vertex_start: i32,
    pub vertex_count: i32,
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
    pub fn push(&mut self, vertices: &[Vertex], indices: &[u16]) -> Vec<PackedRange> {
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

    /// Total distinct vertices across every chunk.
    #[cfg(test)]
    pub fn vertex_total(&self) -> usize {
        self.chunks.iter().map(|chunk| chunk.vertices.len()).sum()
    }

    /// Total indices across every chunk.
    #[cfg(test)]
    pub fn index_total(&self) -> usize {
        self.chunks.iter().map(|chunk| chunk.indices.len()).sum()
    }
}
