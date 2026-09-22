use glow::HasContext;

use crate::font::generate_font_atlas;
use crate::level::{
    FloorPatchDef, LevelDef, LevelSurfaces, PropDef, RoomDef, RoomFloorGrid, WallAxis, WallDef,
    wall_solid_slices_profiled,
};
use crate::lighting::{LevelLighting, LightColor, wall_light_segments};
use crate::materials::{MaterialTable, ResolvedMaterial};

/// Resolves level material ids into surface keys and render parameters.
///
/// The geometry builder only ever asks this for a slot (from the geometry it is
/// emitting) and a material id (from the level), so an id never decides which
/// surface family it draws on and the renderer contains no list of known
/// materials.
struct MaterialLookup<'a> {
    table: &'a MaterialTable,
}

impl<'a> MaterialLookup<'a> {
    const fn new(table: &'a MaterialTable) -> Self {
        Self { table }
    }

    /// The surface key for one slot and material id.
    fn key(&self, slot: MaterialSlot, material_id: &str) -> SurfaceKey {
        SurfaceKey::new(slot.kind(), self.index(material_id))
    }

    fn index(&self, material_id: &str) -> MaterialIndex {
        self.table.index_of(material_id).unwrap_or(MATERIAL_NONE)
    }

    fn entry(&self, key: SurfaceKey) -> Option<&'a ResolvedMaterial> {
        if key.has_material() {
            self.table.entry(key.material)
        } else {
            None
        }
    }

    /// World metres covered by one repeat of a key's texture.
    fn tile_metres(&self, key: SurfaceKey) -> f32 {
        self.entry(key)
            .map_or(crate::assets::DEFAULT_TILE_METRES, |entry| {
                entry.tile_metres
            })
    }

    /// The material's static tint; white for keys without a material.
    fn tint(&self, key: SurfaceKey) -> [f32; 3] {
        self.entry(key).map_or([1.0, 1.0, 1.0], |entry| entry.tint)
    }

    /// World-space UVs for a key at a `(a, b)` world pair.
    fn uv(&self, key: SurfaceKey, a: f32, b: f32) -> [f32; 2] {
        tiled_uv(a, b, self.tile_metres(key))
    }
}

/// `PocketCHIP` reference resolution.
///
/// The game logic and UI layout are authored against this 480x272 space; it is
/// also the default window size. It is *not* an assumption about the actual
/// drawable/framebuffer size at runtime.
pub const WINDOW_WIDTH: u32 = 480;
pub const WINDOW_HEIGHT: u32 = 272;

/// Reference space that 2D UI geometry is authored in (`PocketCHIP` baseline).
pub const UI_REFERENCE_WIDTH: u32 = WINDOW_WIDTH;
pub const UI_REFERENCE_HEIGHT: u32 = WINDOW_HEIGHT;

/// Physical size (in pixels) of the current drawable/framebuffer.
///
/// This is deliberately distinct from the window's logical size: on `HiDPI`
/// displays such as macOS Retina the drawable is larger than the window size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DrawableSize {
    pub width: u32,
    pub height: u32,
}

impl DrawableSize {
    #[must_use]
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    /// True when the surface cannot be rendered to (minimized/hidden windows).
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// Aspect ratio derived from the real framebuffer, safe against zero height.
    #[must_use]
    pub fn aspect_ratio(self) -> f32 {
        if self.height == 0 {
            1.0
        } else {
            self.width as f32 / self.height as f32
        }
    }

    /// Pixel size of the integer-scaled UI region that fits this drawable while
    /// preserving the 480x272 reference aspect ratio, plus its bottom-left origin.
    #[must_use]
    pub fn ui_viewport(self) -> UiViewport {
        if self.is_empty() {
            return UiViewport {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
                scale: 1.0,
            };
        }

        let scale = (self.width as f32 / UI_REFERENCE_WIDTH as f32)
            .min(self.height as f32 / UI_REFERENCE_HEIGHT as f32)
            .max(0.0);
        let drawable_width = i32::try_from(self.width).unwrap_or(i32::MAX);
        let drawable_height = i32::try_from(self.height).unwrap_or(i32::MAX);
        let width = ((UI_REFERENCE_WIDTH as f32 * scale).round() as i32).clamp(1, drawable_width);
        let height =
            ((UI_REFERENCE_HEIGHT as f32 * scale).round() as i32).clamp(1, drawable_height);

        UiViewport {
            x: (drawable_width - width) / 2,
            y: (drawable_height - height) / 2,
            width,
            height,
            scale,
        }
    }
}

/// Placement of the 480x272 UI reference space inside the physical drawable.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiViewport {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub scale: f32,
}

/// Aspect ratio of the authored `PocketCHIP` reference resolution (480x272).
#[must_use]
pub fn reference_aspect_ratio() -> f32 {
    UI_REFERENCE_WIDTH as f32 / UI_REFERENCE_HEIGHT as f32
}

/// Maps the configured (baseline) vertical field of view onto a drawable with
/// the given aspect ratio.
///
/// * Wider than the `PocketCHIP` baseline: the vertical FOV is unchanged, so the
///   horizontal view expands naturally ("Hor+").
/// * Narrower/taller than the baseline: the horizontal FOV is preserved instead
///   so the level is not cropped left/right; only the vertical FOV grows.
///
/// At the baseline aspect this is the identity, so `PocketCHIP` is unchanged.
#[must_use]
pub fn vertical_fov_for_aspect(configured_vertical_fov_degrees: f32, aspect: f32) -> f32 {
    // Guards against a near-singular projection on very tall/portrait windows.
    const MAX_VERTICAL_FOV_DEGREES: f32 = 150.0;

    let reference = reference_aspect_ratio();
    if !aspect.is_finite() || aspect <= 0.0 || aspect >= reference {
        return configured_vertical_fov_degrees;
    }

    let half_vertical_tan = (configured_vertical_fov_degrees.to_radians() * 0.5).tan();
    let half_horizontal_tan = half_vertical_tan * reference;
    let adjusted = 2.0 * (half_horizontal_tan / aspect).atan();
    adjusted
        .to_degrees()
        .clamp(configured_vertical_fov_degrees, MAX_VERTICAL_FOV_DEGREES)
}

const VERTEX_SHADER_SRC: &str = r"
#ifdef GL_ES
precision mediump float;
#endif
attribute vec3 a_pos;
attribute vec4 a_color;
attribute vec2 a_uv;
uniform mat4 u_mvp;
varying vec4 v_color;
varying vec2 v_uv;

void main() {
    v_color = a_color;
    v_uv = a_uv;
    gl_Position = u_mvp * vec4(a_pos, 1.0);
}
";

const FRAGMENT_SHADER_SRC: &str = r"
#ifdef GL_ES
precision mediump float;
#endif
uniform sampler2D u_texture;
varying vec4 v_color;
varying vec2 v_uv;

void main() {
    vec4 tex_color = texture2D(u_texture, v_uv);
    gl_FragColor = tex_color * v_color;
}
";

/// Fragment stage for the decal pass: the same lit, textured look as the world
/// shader, plus an alpha cut-out so a decal can have a silhouette instead of
/// being a floating rectangle.
///
/// Decals keep the world program's vertex stage, so the two programs share
/// attribute locations ([`create_program`] binds them explicitly). This is a
/// second program rather than a branch in the world shader because `discard`
/// can disable early depth testing for every draw that uses the program, and
/// the opaque world must keep it.
const DECAL_FRAGMENT_SHADER_SRC: &str = r"
#ifdef GL_ES
precision mediump float;
#endif
uniform sampler2D u_texture;
uniform float u_alpha_cutoff;
varying vec4 v_color;
varying vec2 v_uv;

void main() {
    vec4 tex_color = texture2D(u_texture, v_uv);
    if (tex_color.a < u_alpha_cutoff) {
        discard;
    }
    gl_FragColor = tex_color * v_color;
}
";

/// Depth bias the decal pass applies, as `glPolygonOffset(factor, units)`.
///
/// `units = -2` pulls a decal two depth-buffer resolution steps towards the
/// camera, which is enough to win against the surface it is printed on even
/// when the two quad tessellations disagree by a few ULPs, and is far too
/// small to be visible as physical separation: at a one-metre view distance it
/// is well under a micrometre. The `factor` is zero because a constant bias is
/// exactly what a coplanar decoration needs; a slope-dependent bias would push
/// decals further out at grazing angles for no benefit.
pub const DECAL_POLYGON_OFFSET: (f32, f32) = (0.0, -2.0);

/// Alpha below which the decal pass discards a decal texel.
pub const DECAL_ALPHA_CUTOFF: f32 = 0.5;

/// Attribute indices both scene programs bind before linking, so switching
/// between the world and decal programs never re-points vertex attributes.
const SCENE_ATTRIB_POS: u32 = 0;
const SCENE_ATTRIB_COLOR: u32 = 1;
const SCENE_ATTRIB_UV: u32 = 2;

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
}

/// GPU vertex layout: 24 bytes instead of 36, with no loss of achievable output.
///
/// * `pos` stays `f32` — world position precision is not negotiable, since a
///   liminal level can be over 250 m across and a centimetre of drift would move
///   geometry through walls.
/// * `uv` stays `f32` — texturing is where quantisation would actually show, and
///   tiling surfaces carry world-space coordinates that reach ±130 on the
///   largest shipped level.
/// * `color` becomes normalised `RGBA8`. It is a *shade* folded into the vertex
///   by the lighting bake, and that bake is bounded: `lighting::AMBIENT_LEVEL`
///   is 0.10 and `MAX_BRIGHTNESS` is 1.0, so a vertex channel only ever spans
///   [0, 1] and the smallest step is 1/255 ≈ 0.9% of the range actually used.
///   Alpha is kept because prop models carry it from their glTF `COLOR_0`.
///
/// `glVertexAttribPointer` with `normalized = true` and `GL_UNSIGNED_BYTE` is
/// core OpenGL ES 2.0, so no extension or newer context is required.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PackedVertex {
    pub pos: [f32; 3],
    pub color: [u8; 4],
    pub uv: [f32; 2],
}

/// Byte offset of each packed attribute, and the stride between vertices.
/// Bytes one vertex occupies in the exact (unpacked) layout, as the GL stride
/// API takes it.
///
/// Written out rather than derived from `size_of::<Vertex>()` so that
/// [`VertexLayout::stride`] can stay a `const fn`; the packed-layout test keeps
/// it equal to the struct's real size.
pub const EXACT_VERTEX_STRIDE: i32 = 36;

pub mod packed_layout {
    /// Offset of `a_pos`, in bytes.
    pub const POS_OFFSET: i32 = 0;
    /// Offset of `a_color`, in bytes.
    pub const COLOR_OFFSET: i32 = 12;
    /// Offset of `a_uv`, in bytes.
    pub const UV_OFFSET: i32 = 16;
    /// Bytes between consecutive vertices.
    pub const STRIDE: i32 = 24;
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
    value.clamp(0.0, 1.0).mul_add(255.0, 0.5) as u8
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
/// group. `Light` and `PropFallback` share the unshaded light sheet; `Decal` is
/// its own pass and always drawn last.
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

/// One surface's material index in a [`crate::materials::MaterialTable`].
///
/// [`MATERIAL_NONE`] is the sentinel for families that do not draw a level
/// material (light panels, prop placeholder boxes, decals), which bind their
/// own shared textures.
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

/// World-space UVs at a material's tiling: one texture repeat every
/// `tile_metres` of world surface, in both directions.
///
/// This is the one convention every surface shares: a floor/ceiling passes
/// `(x, z)`, a wall passes `(along the wall, up the wall)`. Keeping it in one
/// place is what makes a metre of wall show the same amount of wallpaper
/// whatever the sheet's pixel size.
#[must_use]
pub fn tiled_uv(a: f32, b: f32, tile_metres: f32) -> [f32; 2] {
    let tile = if tile_metres.is_finite() && tile_metres > 0.0 {
        tile_metres
    } else {
        crate::assets::DEFAULT_TILE_METRES
    };
    [a / tile, b / tile]
}

#[allow(clippy::too_many_arguments)]
fn add_quad(
    vertices: &mut Vec<Vertex>,
    p0: [f32; 3],
    c0: [f32; 3],
    uv0: [f32; 2],
    p1: [f32; 3],
    c1: [f32; 3],
    uv1: [f32; 2],
    p2: [f32; 3],
    c2: [f32; 3],
    uv2: [f32; 2],
    p3: [f32; 3],
    c3: [f32; 3],
    uv3: [f32; 2],
) {
    let col0 = [c0[0], c0[1], c0[2], 1.0];
    let col1 = [c1[0], c1[1], c1[2], 1.0];
    let col2 = [c2[0], c2[1], c2[2], 1.0];
    let col3 = [c3[0], c3[1], c3[2], 1.0];
    vertices.push(Vertex {
        pos: p0,
        color: col0,
        uv: uv0,
    });
    vertices.push(Vertex {
        pos: p1,
        color: col1,
        uv: uv1,
    });
    vertices.push(Vertex {
        pos: p2,
        color: col2,
        uv: uv2,
    });
    vertices.push(Vertex {
        pos: p0,
        color: col0,
        uv: uv0,
    });
    vertices.push(Vertex {
        pos: p2,
        color: col2,
        uv: uv2,
    });
    vertices.push(Vertex {
        pos: p3,
        color: col3,
        uv: uv3,
    });
}

#[allow(clippy::too_many_arguments)]
fn add_quad_flat(
    vertices: &mut Vec<Vertex>,
    p0: [f32; 3],
    p1: [f32; 3],
    p2: [f32; 3],
    p3: [f32; 3],
    color: [f32; 3],
    uv0: [f32; 2],
    uv1: [f32; 2],
    uv2: [f32; 2],
    uv3: [f32; 2],
) {
    add_quad(
        vertices, p0, color, uv0, p1, color, uv1, p2, color, uv2, p3, color, uv3,
    );
}

/// Distance a wall face is probed away from the wall when sampling baked
/// lighting, so the face is lit by the room it looks into rather than by
/// whichever room the boundary point happens to fall in.
const LIGHT_FACE_PROBE_M: f32 = 0.25;

/// Multiplies one shaded colour by the baked illumination colour, per channel.
fn shade(base: [f32; 3], light: LightColor) -> [f32; 3] {
    [
        (base[0] * light.r).clamp(0.0, 1.0),
        (base[1] * light.g).clamp(0.0, 1.0),
        (base[2] * light.b).clamp(0.0, 1.0),
    ]
}

/// Baked brightness sampled at each of four quad corners.
fn lit_corners(base: [f32; 3], points: [[f32; 3]; 4], lighting: &LevelLighting) -> [[f32; 3]; 4] {
    points.map(|point| shade(base, lighting.sample(point[0], point[1], point[2])))
}

/// Emits one wall face parallel to the wall's length axis as a strip of quads.
///
/// The face is split along its length (bounded by
/// `lighting::MAX_WALL_LIGHT_SEGMENTS`) so baked fixture pools and doorway
/// blends vary along it; a single quad would smear them across the whole wall.
/// `top_at` gives the face's top edge at a length offset, so a wall running up
/// a gable slope follows the real ceiling instead of stepping.
///
/// UVs are world-space at the material's `tile_metres` period, and are
/// oriented so the image reads the way it was authored from the side the face
/// looks into: the image's top row is at the face's top, and its left edge is
/// on the viewer's left (`flip_u` is set for the faces whose outward normal
/// makes the world length axis run the other way). A creator can therefore put
/// a sign, a border or a directional pattern in a wall PNG and see it upright
/// and unmirrored in game.
#[allow(clippy::too_many_arguments)]
fn add_wall_length_face(
    vertices: &mut Vec<Vertex>,
    axis: WallAxis,
    l0: f32,
    l1: f32,
    face: f32,
    normal: f32,
    bottom: f32,
    top_at: impl Fn(f32) -> f32,
    bottom_shade: [f32; 3],
    top_shade: [f32; 3],
    reversed: bool,
    flip_u: bool,
    lighting: &LevelLighting,
    tile_metres: f32,
) {
    let point = |at: f32, y: f32| -> [f32; 3] {
        match axis {
            WallAxis::X => [at, y, face],
            WallAxis::Z => [face, y, at],
        }
    };
    // Probe inside the room this face looks into, so the wall is lit by its own
    // side of the wall even when the surface sits exactly on a room boundary.
    let color = |at: f32, y: f32, base: [f32; 3]| -> [f32; 3] {
        let probe = match axis {
            WallAxis::X => [at, y, normal.mul_add(LIGHT_FACE_PROBE_M, face)],
            WallAxis::Z => [normal.mul_add(LIGHT_FACE_PROBE_M, face), y, at],
        };
        shade(base, lighting.sample(probe[0], probe[1], probe[2]))
    };

    // The V reference keeps the image's top row at the face's top while
    // staying constant along the face, so tiling never breaks across a gable
    // slope or a merged lighting run.
    let uv_v_ref = top_at(l0);
    let uv = |at: f32, y: f32| {
        let u = if flip_u { -at } else { at };
        tiled_uv(u, uv_v_ref - y, tile_metres)
    };

    let segments = wall_light_segments((l1 - l0).abs());
    // Sample the lighting once per segment boundary, then merge runs of
    // boundaries whose colours are effectively flat. Adjacent segments share a
    // corner, so each boundary is sampled exactly once (a 2x saving) and the
    // merged strip keeps a single value at every surviving edge.
    let boundary_count = segments as usize + 1;
    let mut boundaries: Vec<(f32, f32, [f32; 3], [f32; 3])> = Vec::with_capacity(boundary_count);
    for boundary in 0..boundary_count {
        let at = l0 + (l1 - l0) * boundary as f32 / segments as f32;
        let top = top_at(at);
        boundaries.push((
            at,
            top,
            color(at, bottom, bottom_shade),
            color(at, top, top_shade),
        ));
    }
    let matches_run = |reference: &(f32, f32, [f32; 3], [f32; 3]),
                       candidate: &(f32, f32, [f32; 3], [f32; 3])| {
        (0..3).all(|channel| {
            (candidate.2[channel] - reference.2[channel]).abs() <= LIGHT_GRID_MERGE_EPS
                && (candidate.3[channel] - reference.3[channel]).abs() <= LIGHT_GRID_MERGE_EPS
        }) && (candidate.1 - reference.1).abs() <= HEIGHT_MERGE_EPS
    };
    let mut start = 0;
    while start < segments as usize {
        let mut end = start + 1;
        while end < segments as usize
            && boundaries[start..=end]
                .iter()
                .all(|candidate| matches_run(&boundaries[start], candidate))
        {
            end += 1;
        }
        let (at_start, top_start, bottom_start, top_color_start) = boundaries[start];
        let (at_end, top_end, bottom_end, top_color_end) = boundaries[end];
        // Walk the strip from `start` to `end`, or the other way round when the
        // wall is reversed, so the quad keeps one consistent winding.
        let (
            first_at,
            first_top,
            first_bottom,
            first_top_color,
            second_at,
            second_top,
            second_bottom,
            second_top_color,
        ) = if reversed {
            (
                at_end,
                top_end,
                bottom_end,
                top_color_end,
                at_start,
                top_start,
                bottom_start,
                top_color_start,
            )
        } else {
            (
                at_start,
                top_start,
                bottom_start,
                top_color_start,
                at_end,
                top_end,
                bottom_end,
                top_color_end,
            )
        };
        // Wound so the face's front side is the room the `normal` direction
        // points into (the outward side of the wall).
        add_quad(
            vertices,
            point(first_at, first_top),
            first_top_color,
            uv(first_at, first_top),
            point(second_at, second_top),
            second_top_color,
            uv(second_at, second_top),
            point(second_at, bottom),
            second_bottom,
            uv(second_at, bottom),
            point(first_at, bottom),
            first_bottom,
            uv(first_at, bottom),
        );
        start = end;
    }
}

pub(crate) const fn generate_white_texture() -> [u8; 2 * 2 * 4] {
    [255u8; 2 * 2 * 4]
}

// ------------------------------------------------------------- decal sheets
//
// Decals are small local surface markings. They all share one generated RGBA
// sheet so the whole level draws them with a single texture bind, and they are
// authored without a plate: the background is alpha 0 and the decal pass
// discards it, which is what lets a future NO DIVING sign or floor arrow have
// a cut-out silhouette instead of a floating rectangle.

/// Generated decal sheet id for the internal validation marking.
pub const DECAL_TEST_MATERIAL: &str = "core:decal_test_01";
/// Generated decal sheet id for a floor-direction arrow.
pub const DECAL_ARROW_MATERIAL: &str = "core:decal_arrow_01";
/// Generated decal sheet id for hazard stripes.
pub const DECAL_STRIPES_MATERIAL: &str = "core:decal_stripes_01";

/// Edge length of the generated decal sheet.
const DECAL_ATLAS_SIZE: i32 = 256;
/// One decal pattern's cell size inside the sheet.
const DECAL_SLOT_SIZE: i32 = 128;
/// Transparent gutter between cells, so mip-mapping never bleeds one pattern
/// into its neighbour.
const DECAL_SLOT_GUTTER: i32 = 8;
/// Every generated decal sheet id the renderer can draw, in slot order.
///
/// The final Pool safety sign is no longer one of these: it is external PNG
/// artwork (`core:decal_no_diving_01` is a catalog `file` decal) drawn from its
/// own sheet. The atlas keeps one spare cell for a future generated pattern.
pub const DECAL_MATERIALS: [&str; 3] = [
    DECAL_TEST_MATERIAL,
    DECAL_ARROW_MATERIAL,
    DECAL_STRIPES_MATERIAL,
];

/// Resolves a decal material id to its slot in the generated sheet.
///
/// Unknown ids are not an error: a level may reference a decal sheet a future
/// build knows about, and simply drawing nothing is the graceful degradation
/// the loader wants for unsupported content.
#[must_use]
pub fn decal_material_slot(material: &str) -> Option<u32> {
    DECAL_MATERIALS
        .iter()
        .position(|id| *id == material)
        .map(|slot| slot as u32)
}

/// Sheet index of the first external (PNG-backed) decal sheet.
pub const DECAL_EXTERNAL_BASE: u32 = DECAL_MATERIALS.len() as u32;

/// True when the catalog declares `material` as a file-backed decal sheet.
///
/// Only these resolve to external PNG artwork; the generated patterns and
/// unknown ids are handled by [`decal_material_slot`].
fn catalog_decal_sheet<'a>(
    catalog: &'a crate::assets::AssetCatalog,
    material: &str,
) -> Option<&'a str> {
    let entry = catalog.get(material)?;
    if entry.asset_type.as_str() != crate::assets::AssetType::DECAL {
        return None;
    }
    if !matches!(entry.source, crate::assets::AssetSource::File) {
        return None;
    }
    entry
        .model
        .as_deref()
        .filter(|model| model.to_ascii_lowercase().ends_with(".png"))
}

/// External decal sheets a level places, in first-use order.
///
/// A decal asset that is not one of the generated patterns and is declared in
/// the catalog as a file-backed PNG resolves as external artwork, exactly like
/// a surface texture. Both the mesh builder and the GPU uploader derive the
/// mapping from the level and the catalog alone, so a decal's sheet index never
/// needs extra renderer state: `0..4` are the generated atlas patterns, then
/// one index per external sheet in the order the level first places it. The
/// mapping is stable and independent of whether a sheet's PNG could actually be
/// decoded; the renderer draws the diagnostic sheet for a broken file.
///
/// An id that is neither generated nor a catalogued file sheet is skipped, the
/// same graceful degradation unknown materials use.
#[must_use]
pub fn decal_external_sheet_ids(
    level: &LevelDef,
    catalog: &crate::assets::AssetCatalog,
) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    for decal in &level.decals {
        if decal_material_slot(&decal.material).is_some() {
            continue;
        }
        if catalog_decal_sheet(catalog, &decal.material).is_none() {
            continue;
        }
        if !ids.contains(&decal.material) {
            ids.push(decal.material.clone());
        }
    }
    ids
}

/// Sheet index a decal material draws from, or `None` when a level references
/// an unknown decal (no geometry is emitted for it, as before).
#[must_use]
pub fn decal_sheet_index(
    level: &LevelDef,
    catalog: &crate::assets::AssetCatalog,
    material: &str,
) -> Option<u32> {
    if let Some(slot) = decal_material_slot(material) {
        return Some(slot);
    }
    decal_external_sheet_ids(level, catalog)
        .iter()
        .position(|id| id == material)
        .map(|position| DECAL_EXTERNAL_BASE + position as u32)
}

/// UV rectangle of a whole external decal sheet.
///
/// An external sheet is uploaded as one image, so its decal quad samples the
/// full texture. The decal quad's corners arrive as
/// `[bottom-left, bottom-right, top-right, top-left]` of the decal's own
/// in-plane frame, and the uploaded image's row order runs opposite to that
/// frame's V axis, so both in-plane axes are swapped here. The same rect serves
/// floors, ceilings and walls: each family's frame is built from its own
/// out-of-plane axis, but the correction is the same. A marking then reads
/// upright and unmirrored in the world exactly as it does in an image viewer,
/// with the authored `rotation_degrees` applied as a real in-plane rotation.
///
/// Verified by scoring ink masks of the Pool showcase's external sign on the
/// deck and on a wall against the PNG under all four square symmetries (both
/// matched `identity`), and pinned by
/// `external_decal_sheets_pin_their_world_orientation`.
#[must_use]
pub const fn decal_uv_rect_full() -> [[f32; 2]; 4] {
    [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]]
}

/// Writes one texel into the decal sheet, in visual (top-down) coordinates.
///
/// The sheet is stored bottom-up so the generated text reads upright under the
/// game's `v` convention (v = 0 is the bottom of the image as displayed); every
/// other generated sheet is vertically symmetric, so this is the first texture
/// where the distinction is visible.
fn decal_atlas_put(pixels: &mut [u8], x: i32, y: i32, color: [u8; 4]) {
    if x < 0 || y < 0 || x >= DECAL_ATLAS_SIZE || y >= DECAL_ATLAS_SIZE {
        return;
    }
    let row = DECAL_ATLAS_SIZE - 1 - y;
    let index = ((row * DECAL_ATLAS_SIZE + x) * 4) as usize;
    pixels[index..index + 4].copy_from_slice(&color);
}

/// Plain rectangle fill in visual sheet coordinates.
fn decal_atlas_rect(pixels: &mut [u8], x0: i32, y0: i32, x1: i32, y1: i32, color: [u8; 4]) {
    for y in y0..=y1 {
        for x in x0..=x1 {
            decal_atlas_put(pixels, x, y, color);
        }
    }
}

/// Rectangle outline in visual sheet coordinates.
fn decal_atlas_frame(
    pixels: &mut [u8],
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    thickness: i32,
    color: [u8; 4],
) {
    decal_atlas_rect(pixels, x0, y0, x1, y0 + thickness - 1, color);
    decal_atlas_rect(pixels, x0, y1 - thickness + 1, x1, y1, color);
    decal_atlas_rect(pixels, x0, y0, x0 + thickness - 1, y1, color);
    decal_atlas_rect(pixels, x1 - thickness + 1, y0, x1, y1, color);
}

/// Stamps one line of the embedded 8x8 font into the sheet at `scale`.
///
/// The font table is already the project's own bitmap resource (the HUD uses
/// it), so diagnostic decal text stays project-created data with no new asset
/// pipeline.
fn decal_atlas_text(
    pixels: &mut [u8],
    origin_x: i32,
    origin_y: i32,
    text: &str,
    scale: i32,
    color: [u8; 4],
) {
    let mut cursor_x = origin_x;
    for character in text.bytes() {
        if character < crate::font::FONT_FIRST_CHAR {
            continue;
        }
        let glyph_index = usize::from(character - crate::font::FONT_FIRST_CHAR);
        if let Some(glyph) = crate::font::FONT_DATA.get(glyph_index) {
            for (row, bits) in glyph.iter().enumerate() {
                for column in 0..8i32 {
                    if bits & (0x80 >> column) == 0 {
                        continue;
                    }
                    for dy in 0..scale {
                        for dx in 0..scale {
                            decal_atlas_put(
                                pixels,
                                cursor_x + column * scale + dx,
                                origin_y + row as i32 * scale + dy,
                                color,
                            );
                        }
                    }
                }
            }
        }
        cursor_x += 8 * scale;
    }
}

/// Draws one line of text horizontally centred in a decal cell.
fn decal_atlas_text_centered(
    pixels: &mut [u8],
    slot: i32,
    text: &str,
    y: i32,
    scale: i32,
    color: [u8; 4],
) {
    let (col, row) = (slot % 2, slot / 2);
    let cell_x = col * DECAL_SLOT_SIZE;
    let cell_y = row * DECAL_SLOT_SIZE;
    let width = i32::try_from(text.len()).unwrap_or(0) * 8 * scale;
    decal_atlas_text(
        pixels,
        cell_x + (DECAL_SLOT_SIZE - width) / 2,
        cell_y + y,
        text,
        scale,
        color,
    );
}

/// Generates the shared decal sheet: a validation marking, a floor arrow and
/// hazard stripes, with one spare cell left transparent.
pub(crate) fn generate_decal_atlas() -> Vec<u8> {
    let mut pixels = vec![0u8; (DECAL_ATLAS_SIZE * DECAL_ATLAS_SIZE * 4) as usize];
    let white = [245, 245, 240, 255];
    let green = [64, 176, 96, 255];
    let yellow = [232, 196, 40, 255];

    // Slot 0: the validation marking, "DECAL TEST" in a frame on transparency.
    decal_atlas_frame(&mut pixels, 14, 14, 113, 113, 4, white);
    decal_atlas_text_centered(&mut pixels, 0, "DECAL", 40, 2, white);
    decal_atlas_text_centered(&mut pixels, 0, "TEST", 72, 2, white);

    // Slot 1: a floor arrow pointing up the decal's own vertical axis, so an
    // accidental 90/180 degree rotation is obvious on sight. It is drawn in
    // cell `(col, row) = (1, 0)`, exactly the cell `decal_uv_rect(1)` samples:
    // the drawn art and the sampled rect must agree, or a level silently shows
    // the wrong pattern.
    let cell_x = DECAL_SLOT_SIZE;
    let cell_y = 0;
    let arrow_x = cell_x + 64;
    let arrow_bottom = cell_y + 112;
    let arrow_stem_top = cell_y + 64;
    decal_atlas_rect(
        &mut pixels,
        arrow_x - 6,
        arrow_stem_top,
        arrow_x + 5,
        arrow_bottom,
        green,
    );
    for row in 0..=44 {
        let half = row * 3 / 4;
        decal_atlas_rect(
            &mut pixels,
            arrow_x - half,
            cell_y + 20 + row,
            arrow_x + half - 1,
            cell_y + 20 + row,
            green,
        );
    }

    // Slot 2: hazard stripes for grazing-angle tests, in cell `(0, 1)`.
    for y in 0..DECAL_SLOT_SIZE {
        for x in 0..DECAL_SLOT_SIZE {
            if (x + y).rem_euclid(32) < 16 {
                decal_atlas_put(&mut pixels, x, 128 + y, yellow);
            }
        }
    }

    pixels
}

/// Texture-coordinate rectangle of one decal slot, as
/// `[bottom-left, bottom-right, top-right, top-left]` matching the decal quad
/// winding (`add_decal_quad`).
#[must_use]
pub fn decal_uv_rect(slot: u32) -> [[f32; 2]; 4] {
    let cell = i32::try_from(slot).unwrap_or(0).clamp(0, 3);
    let (col, row) = (cell % 2, cell / 2);
    let inset = DECAL_SLOT_GUTTER;
    let x0 = (col * DECAL_SLOT_SIZE + inset) as f32;
    let x1 = (col * DECAL_SLOT_SIZE + DECAL_SLOT_SIZE - inset) as f32;
    let y0 = (row * DECAL_SLOT_SIZE + inset) as f32;
    let y1 = (row * DECAL_SLOT_SIZE + DECAL_SLOT_SIZE - inset) as f32;
    let size = DECAL_ATLAS_SIZE as f32;
    // The sheet is stored bottom-up, so the visual top row maps to the higher
    // texture coordinate.
    let u0 = x0 / size;
    let u1 = x1 / size;
    let v_top = (size - y0) / size;
    let v_bottom = (size - y1) / size;
    [[u0, v_bottom], [u1, v_bottom], [u1, v_top], [u0, v_top]]
}

/// Tolerance for treating two walls as occupying the same plane, in metres.
/// It is the same 1 mm tolerance the floor cut lines and wall cross-section
/// merging already use, so a wall that is "the same wall" to those steps is
/// also the same wall here.
const WALL_COINCIDENCE_EPS: f32 = 1e-3;

/// One material run of a coalesced wall group, in local length coordinates.
///
/// A run is a sub-span of the group's length over which every covering wall
/// agrees on the visible material. The faces are ordered like the two length
/// faces the emitter walks: the low-thickness face first (north on an X-axis
/// wall, west on a Z-axis wall), the high-thickness face second (south/east).
#[derive(Clone, Copy, Debug, PartialEq)]
struct WallMaterialRun {
    start: f32,
    end: f32,
    faces: [SurfaceKey; 2],
    /// Key used by sills, headers and reveals inside this run.
    body: SurfaceKey,
}

/// One wall the geometry builder emits.
///
/// A wall on its own is emitted exactly as authored. Several walls that occupy
/// the same plane (same axis, thickness span, base and height, overlapping
/// length) are a *material overlay* authored as duplicate geometry: they are
/// resolved into one synthetic wall carrying the group's combined solid
/// profile and per-run materials, so the surface is emitted once and there is
/// no second coplanar mesh to fight for the same depth value.
enum WallUnit<'a> {
    Plain(&'a WallDef),
    Coalesced {
        wall: WallDef,
        runs: Vec<WallMaterialRun>,
    },
}

impl WallUnit<'_> {
    fn wall(&self) -> &WallDef {
        match self {
            Self::Plain(wall) => wall,
            Self::Coalesced { wall, .. } => wall,
        }
    }

    /// The material run covering a local length position, if this unit was
    /// coalesced. Plain walls keep their authored per-face materials.
    fn run_at(&self, position: f32) -> Option<&WallMaterialRun> {
        match self {
            Self::Plain(_) => None,
            Self::Coalesced { runs, .. } => runs.iter().find(|run| {
                position >= run.start - WALL_COINCIDENCE_EPS
                    && position <= run.end + WALL_COINCIDENCE_EPS
            }),
        }
    }

    /// Material runs intersecting a local length span, clipped to it.
    ///
    /// Empty for a plain wall, which draws its authored material across the
    /// whole face.
    fn runs_between(&self, start: f32, end: f32) -> Vec<WallMaterialRun> {
        match self {
            Self::Plain(_) => Vec::new(),
            Self::Coalesced { runs, .. } => runs
                .iter()
                .filter_map(|run| {
                    let low = run.start.max(start);
                    let high = run.end.min(end);
                    (high - low > WALL_COINCIDENCE_EPS).then_some(WallMaterialRun {
                        start: low,
                        end: high,
                        ..*run
                    })
                })
                .collect(),
        }
    }
}

/// Wall facts the coincidence grouping compares.
#[derive(Clone, Copy)]
struct WallSlab {
    axis: WallAxis,
    /// Thickness span across the length axis (min, max).
    thickness: (f32, f32),
    /// World base and top of the wall.
    base: f32,
    top: f32,
    /// World span along the length axis (start, end).
    length: (f32, f32),
}

/// World Y span of a wall: its authored base and its top, which follows the
/// room ceiling profile when the wall does not author an explicit height.
///
/// The top is the maximum over the wall's length (a gable-end wall reaches the
/// ridge in the middle), so collision and coalescing both see the conservative
/// extent while the emitter clips each face against the exact local ceiling.
fn wall_vertical_extent(wall: &WallDef, surfaces: &LevelSurfaces<'_>) -> (f32, f32) {
    let length = wall.length();
    let breaks = surfaces.wall_profile_breaks(wall);
    let mut base = wall.y;
    let mut top = f32::NEG_INFINITY;
    let probes = std::iter::once(0.0)
        .chain(std::iter::once(length))
        .chain(breaks.iter().copied());
    for at in probes {
        let clear = wall
            .height
            .unwrap_or_else(|| surfaces.clear_ceiling_height_along(wall, at));
        let candidate = wall.y + clear;
        base = base.min(candidate);
        top = top.max(candidate);
    }
    (base, top)
}

fn wall_slab(wall: &WallDef, surfaces: &LevelSurfaces<'_>) -> Option<WallSlab> {
    let axis = wall.axis();
    let (x0, x1) = (
        wall.x.min(wall.x + wall.width),
        wall.x.max(wall.x + wall.width),
    );
    let (z0, z1) = (
        wall.z.min(wall.z + wall.depth),
        wall.z.max(wall.z + wall.depth),
    );
    let thickness = match axis {
        WallAxis::X => (z0, z1),
        WallAxis::Z => (x0, x1),
    };
    let length_span = match axis {
        WallAxis::X => (x0, x1),
        WallAxis::Z => (z0, z1),
    };
    let length = wall.length();
    if !length.is_finite() || length <= WALL_COINCIDENCE_EPS {
        return None;
    }
    let (base, top) = wall_vertical_extent(wall, surfaces);
    if !base.is_finite() || !top.is_finite() || top <= base + WALL_COINCIDENCE_EPS {
        return None;
    }
    Some(WallSlab {
        axis,
        thickness,
        base,
        top,
        length: length_span,
    })
}

/// Absolute Y intervals of a wall's openings that cover the world length span
/// `segment`, clamped the same way [`wall_solid_slices`] clamps them.
fn wall_opening_intervals(wall: &WallDef, ceiling_h: f32, segment: (f32, f32)) -> Vec<(f32, f32)> {
    let length = wall.length();
    let (origin_x, origin_z) = wall.length_origin();
    let origin = match wall.axis() {
        WallAxis::X => origin_x,
        WallAxis::Z => origin_z,
    };
    let resolved = wall.resolved_height(ceiling_h);
    let base = wall.y.min(wall.y + resolved);
    let ceiling = wall.y.max(wall.y + resolved);
    let mut intervals = Vec::new();
    for opening in &wall.openings {
        if !opening.offset.is_finite()
            || !opening.width.is_finite()
            || !opening.height.is_finite()
            || !opening.sill.is_finite()
            || opening.width <= 0.0
            || opening.height <= 0.0
        {
            continue;
        }
        let start = origin + opening.offset.clamp(0.0, length);
        let end = origin + opening.end().clamp(0.0, length);
        if end <= start + WALL_COINCIDENCE_EPS
            || start > segment.0 + WALL_COINCIDENCE_EPS
            || end < segment.1 - WALL_COINCIDENCE_EPS
        {
            continue;
        }
        let bottom = (base + opening.sill.max(0.0)).clamp(base, ceiling);
        let top = (base + opening.sill.max(0.0) + opening.height).clamp(base, ceiling);
        if top <= bottom + WALL_COINCIDENCE_EPS {
            continue;
        }
        intervals.push((bottom, top));
    }
    merge_intervals(intervals)
}

/// Intersection of two sorted, disjoint Y interval lists.
fn intersect_intervals(left: &[(f32, f32)], right: &[(f32, f32)]) -> Vec<(f32, f32)> {
    let mut out = Vec::new();
    for (a0, a1) in left {
        for (b0, b1) in right {
            let bottom = a0.max(*b0);
            let top = a1.min(*b1);
            if top > bottom + WALL_COINCIDENCE_EPS {
                out.push((bottom, top));
            }
        }
    }
    merge_intervals(out)
}

/// The two length-face keys and the body key of one wall.
fn wall_material_keys(
    wall: &WallDef,
    default_wall: &str,
    materials: &MaterialLookup<'_>,
) -> ([SurfaceKey; 2], SurfaceKey) {
    let wall_material = wall.material.as_deref().unwrap_or(default_wall);
    let axis = wall.axis();
    let (low_name, high_name) = match axis {
        WallAxis::X => ("north", "south"),
        WallAxis::Z => ("west", "east"),
    };
    let face = |name: &str| {
        let material = wall.faces.get(name).map_or(wall_material, String::as_str);
        materials.key(MaterialSlot::Wall, material)
    };
    (
        [face(low_name), face(high_name)],
        materials.key(MaterialSlot::Wall, wall_material),
    )
}

/// Resolves coincident collinear walls into single emission units.
///
/// The shipped residential levels paint part of a wall with water damage by
/// placing a second wall in exactly the same plane with a stained material.
/// That is a material overlay represented as duplicate geometry, and depending
/// on submission order the two identical surfaces fight for the same depth
/// value. Here such walls are grouped, the group's solid profile is unioned
/// (an opaque coincident face covers a hole in the other surface, which is
/// what the renderer already showed) and the group is emitted once with a
/// material run per span. Walls that merely overlap without sharing a plane
/// are untouched; collision keeps using the authored walls.
fn wall_units<'a>(
    level: &'a LevelDef,
    surfaces: &LevelSurfaces<'_>,
    materials: &MaterialLookup<'_>,
) -> Vec<WallUnit<'a>> {
    let default_wall = level.defaults.wall.as_str();
    let walls = &level.walls;
    let slabs: Vec<Option<WallSlab>> = walls.iter().map(|wall| wall_slab(wall, surfaces)).collect();

    // Transitive grouping of coincident slabs (union-find over wall indices).
    let mut parent: Vec<usize> = (0..walls.len()).collect();
    fn find(parent: &mut [usize], index: usize) -> usize {
        let mut root = index;
        while parent[root] != root {
            root = parent[root];
        }
        let mut cursor = index;
        while parent[cursor] != root {
            let next = parent[cursor];
            parent[cursor] = root;
            cursor = next;
        }
        root
    }
    for (i, slab) in slabs.iter().enumerate() {
        let Some(a) = *slab else { continue };
        for j in i + 1..walls.len() {
            let Some(b) = slabs[j] else { continue };
            if a.axis != b.axis
                || (a.thickness.0 - b.thickness.0).abs() > WALL_COINCIDENCE_EPS
                || (a.thickness.1 - b.thickness.1).abs() > WALL_COINCIDENCE_EPS
                || (a.base - b.base).abs() > WALL_COINCIDENCE_EPS
                || (a.top - b.top).abs() > WALL_COINCIDENCE_EPS
            {
                continue;
            }
            let (a_start, a_end) = slabs[i].map_or((0.0, 0.0), |slab| slab.length);
            let (b_start, b_end) = slabs[j].map_or((0.0, 0.0), |slab| slab.length);
            let (_, shared_end) = (a_start.max(b_start), a_end.min(b_end));
            if shared_end - a_start.max(b_start) <= WALL_COINCIDENCE_EPS {
                continue;
            }
            let (root_a, root_b) = (find(&mut parent, i), find(&mut parent, j));
            if root_a != root_b {
                parent[root_b] = root_a;
            }
        }
    }

    // Collect groups in first-appearance order so the emitted range order stays
    // deterministic and follows the authored wall order.
    let mut group_of: Vec<usize> = vec![usize::MAX; walls.len()];
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for (i, slab) in slabs.iter().enumerate() {
        if slab.is_none() {
            continue;
        }
        let root = find(&mut parent, i);
        if group_of[root] == usize::MAX {
            group_of[root] = groups.len();
            groups.push(Vec::new());
        }
        groups[group_of[root]].push(i);
    }

    let mut units: Vec<WallUnit<'a>> = Vec::with_capacity(groups.len());
    for group in groups {
        if group.len() == 1 {
            units.push(WallUnit::Plain(&walls[group[0]]));
            continue;
        }

        // Length boundaries of the group: every member's ends and every
        // opening edge, clipped to the group's union span.
        let (lo, hi) = group
            .iter()
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), index| {
                let (start, end) = slabs[*index].map_or((0.0, 0.0), |slab| slab.length);
                (lo.min(start), hi.max(end))
            });
        let mut boundaries: Vec<f32> = vec![lo, hi];
        for index in &group {
            let wall = &walls[*index];
            let (start, end) = slabs[*index].map_or((0.0, 0.0), |slab| slab.length);
            boundaries.push(start.clamp(lo, hi));
            boundaries.push(end.clamp(lo, hi));
            let (origin_x, origin_z) = wall.length_origin();
            let origin = match wall.axis() {
                WallAxis::X => origin_x,
                WallAxis::Z => origin_z,
            };
            for opening in &wall.openings {
                if !opening.offset.is_finite() || !opening.width.is_finite() {
                    continue;
                }
                boundaries.push((origin + opening.offset).clamp(lo, hi));
                boundaries.push((origin + opening.end()).clamp(lo, hi));
            }
        }
        boundaries.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        boundaries.dedup_by(|a, b| (*a - *b).abs() <= WALL_COINCIDENCE_EPS);

        let mut runs: Vec<WallMaterialRun> = Vec::new();
        let mut openings: Vec<crate::level::WallOpeningDef> = Vec::new();
        for pair in boundaries.windows(2) {
            let segment = (pair[0], pair[1]);
            if segment.1 - segment.0 <= WALL_COINCIDENCE_EPS {
                continue;
            }
            let covering: Vec<usize> = group
                .iter()
                .copied()
                .filter(|index| {
                    let (start, end) = slabs[*index].map_or((0.0, 0.0), |slab| slab.length);
                    start <= segment.0 + WALL_COINCIDENCE_EPS
                        && end >= segment.1 - WALL_COINCIDENCE_EPS
                })
                .collect();
            let Some(first) = covering.first().copied() else {
                continue;
            };

            // Solid profile of the group: a span is open only when every
            // covering wall has an opening there, because any opaque member
            // covers the others' holes.
            let base = slabs[first].map_or(0.0, |slab| slab.base);
            let mut holes: Option<Vec<(f32, f32)>> = None;
            for index in &covering {
                let member_h = surfaces.clear_ceiling_height_at(
                    walls[*index].width.mul_add(0.5, walls[*index].x),
                    walls[*index].depth.mul_add(0.5, walls[*index].z),
                );
                let member_holes: Vec<(f32, f32)> =
                    wall_opening_intervals(&walls[*index], member_h, segment)
                        .iter()
                        .map(|(bottom, top)| (bottom - base, top - base))
                        .collect();
                holes = Some(match holes {
                    None => member_holes,
                    Some(existing) => intersect_intervals(&existing, &member_holes),
                });
            }
            for (bottom, top) in holes.unwrap_or_default() {
                openings.push(crate::level::WallOpeningDef {
                    kind: "passage".into(),
                    offset: segment.0 - lo,
                    width: segment.1 - segment.0,
                    height: top - bottom,
                    sill: bottom,
                });
            }

            // Visible material: the last covering member wins, which is
            // exactly what the duplicate surfaces used to resolve to for the
            // shipped overlays, because a damage overlay is authored after the
            // wall it covers. Later levels keep that ordering rule explicit:
            // the latest authored material in the span is the one drawn.
            let mut faces = [
                materials.key(MaterialSlot::Wall, default_wall),
                materials.key(MaterialSlot::Wall, default_wall),
            ];
            let mut body = faces[0];
            for index in &covering {
                let (member_faces, member_body) =
                    wall_material_keys(&walls[*index], default_wall, materials);
                faces = member_faces;
                body = member_body;
            }
            runs.push(WallMaterialRun {
                start: segment.0 - lo,
                end: segment.1 - lo,
                faces,
                body,
            });
        }

        // The synthetic wall spans the group's whole union, sharing the first
        // member's thickness and vertical extent; it only carries the group's
        // combined openings and material runs. Collision keeps using the
        // authored walls, so this is a rendering-only resolution.
        let host = &walls[group[0]];
        let host_base = slabs[group[0]].map_or(host.y, |slab| slab.base);
        let host_top =
            slabs[group[0]].map_or(host.y + host.resolved_height(host_base), |slab| slab.top);
        let (t0, t1) = match host.axis() {
            WallAxis::X => (
                host.z.min(host.z + host.depth),
                host.z.max(host.z + host.depth),
            ),
            WallAxis::Z => (
                host.x.min(host.x + host.width),
                host.x.max(host.x + host.width),
            ),
        };
        let mut wall = host.clone();
        wall.openings = openings;
        wall.y = host_base;
        wall.height = Some(host_top - host_base);
        match host.axis() {
            WallAxis::X => {
                wall.x = lo;
                wall.width = hi - lo;
                wall.z = t0;
                wall.depth = t1 - t0;
            }
            WallAxis::Z => {
                wall.z = lo;
                wall.depth = hi - lo;
                wall.x = t0;
                wall.width = t1 - t0;
            }
        }
        units.push(WallUnit::Coalesced { wall, runs });
    }
    units
}

/// Merges overlapping/adjacent Y intervals into a sorted, disjoint list.
fn merge_intervals(mut intervals: Vec<(f32, f32)>) -> Vec<(f32, f32)> {
    intervals.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut merged: Vec<(f32, f32)> = Vec::with_capacity(intervals.len());
    for (bottom, top) in intervals {
        if let Some(last) = merged.last_mut()
            && bottom <= last.1 + 1e-3
        {
            last.1 = last.1.max(top);
            continue;
        }
        merged.push((bottom, top));
    }
    merged
}

/// Y ranges that are solid on exactly one of the two sides of a wall cross
/// section: the faces exposed by an opening or by the wall's end.
fn interval_symmetric_difference(left: &[(f32, f32)], right: &[(f32, f32)]) -> Vec<(f32, f32)> {
    let left = merge_intervals(left.to_vec());
    let right = merge_intervals(right.to_vec());

    let mut cuts: Vec<f32> = Vec::with_capacity((left.len() + right.len()) * 2);
    for (bottom, top) in left.iter().chain(right.iter()) {
        cuts.push(*bottom);
        cuts.push(*top);
    }
    cuts.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    cuts.dedup_by(|a, b| (*a - *b).abs() <= 1e-3);

    let covers = |intervals: &[(f32, f32)], y: f32| {
        intervals
            .iter()
            .any(|(bottom, top)| *bottom <= y && y <= *top)
    };

    let mut difference = Vec::new();
    for bounds in cuts.windows(2) {
        let (bottom, top) = (bounds[0], bounds[1]);
        if top <= bottom + 1e-3 {
            continue;
        }
        let middle = f32::midpoint(bottom, top);
        if covers(&left, middle) != covers(&right, middle) {
            difference.push((bottom, top));
        }
    }
    merge_intervals(difference)
}

/// Emits a vertical quad spanning a wall's thickness at a fixed offset along
/// the wall's length axis: a wall end cap or an opening reveal.
///
/// `at` is the world coordinate along the length axis and `thickness` the
/// world span of the wall across it. `facing_positive` selects which way along
/// the length axis the face looks: an end cap at the wall's start faces the
/// negative direction, one at its end the positive direction, and a reveal
/// faces into the opening it belongs to. `corners` are the shaded colours of
/// the four quad corners in `(t0, t1, t1, t0)` corner order, so a reveal
/// between two rooms can carry each side's baked light through the door rather
/// than falling back to ambient in the middle of the wall. UVs follow the wall
/// face convention (horizontal world coordinate, then Y).
#[allow(clippy::too_many_arguments)]
fn add_wall_cross_quad(
    vertices: &mut Vec<Vertex>,
    axis: WallAxis,
    at: f32,
    thickness: (f32, f32),
    bottom: f32,
    top: f32,
    facing_positive: bool,
    corners: [[f32; 3]; 4],
    tile_metres: f32,
) {
    let (t0, t1) = thickness;
    // The four corners are supplied in the order (low thickness, high
    // thickness) at the bottom, then the same two at the top, and each keeps
    // its own baked colour.
    let (a, b, c, d) = match axis {
        // Length runs along X, so the cross section lies in the Z/Y plane.
        WallAxis::X => (
            ([at, bottom, t0], corners[0]),
            ([at, bottom, t1], corners[1]),
            ([at, top, t1], corners[2]),
            ([at, top, t0], corners[3]),
        ),
        // Length runs along Z, so the cross section lies in the X/Y plane.
        WallAxis::Z => (
            ([t0, bottom, at], corners[0]),
            ([t1, bottom, at], corners[1]),
            ([t1, top, at], corners[2]),
            ([t0, top, at], corners[3]),
        ),
    };
    // Forward order faces the positive length direction, reversed the negative
    // one, so every cross-section face looks out of the solid it belongs to.
    let order = if facing_positive {
        [a, b, c, d]
    } else {
        [b, a, d, c]
    };
    let uv = |point: [f32; 3]| match axis {
        WallAxis::X => tiled_uv(point[2], top - point[1], tile_metres),
        WallAxis::Z => tiled_uv(point[0], top - point[1], tile_metres),
    };
    add_quad(
        vertices,
        order[0].0,
        order[0].1,
        uv(order[0].0),
        order[1].0,
        order[1].1,
        uv(order[1].0),
        order[2].0,
        order[2].1,
        uv(order[2].0),
        order[3].0,
        order[3].1,
        uv(order[3].0),
    );
}

/// Per-face shading multipliers for a prop box, in the prop's local space.
/// The top face is brightest and the bottom darkest, so unlit props still read
/// as solid boxes.
const PROP_FACE_SHADES: [f32; 6] = [1.00, 0.62, 0.90, 0.80, 0.74, 0.86];

/// Emits one Y-rotated box for a prop: six quads tinted with the catalog
/// colour and the baked lighting sampled at each corner, ready to be drawn with
/// the unshaded white texture.
fn add_prop_box(
    vertices: &mut Vec<Vertex>,
    prop: &PropDef,
    size: [f32; 3],
    color: [f32; 3],
    base_y: f32,
    lighting: &LevelLighting,
) {
    let half_w = size[0] * 0.5;
    let half_h = size[1] * 0.5;
    let half_d = size[2] * 0.5;
    let center_y = base_y + prop.y + half_h;

    let (sin_yaw, cos_yaw) = prop.rotation_degrees.to_radians().sin_cos();
    let rotate = |lx: f32, lz: f32| -> (f32, f32) {
        (
            lz.mul_add(sin_yaw, lx.mul_add(cos_yaw, prop.x)),
            lz.mul_add(cos_yaw, lx.mul_add(-sin_yaw, prop.z)),
        )
    };
    let corner = |sx: f32, sy: f32, sz: f32| -> [f32; 3] {
        let (world_x, world_z) = rotate(sx * half_w, sz * half_d);
        [world_x, sy.mul_add(half_h, center_y), world_z]
    };
    let shaded = |mult: f32, point: [f32; 3]| -> [f32; 3] {
        let light = lighting.sample(point[0], point[1], point[2]);
        [
            (color[0] * mult * light.r).min(1.0),
            (color[1] * mult * light.g).min(1.0),
            (color[2] * mult * light.b).min(1.0),
        ]
    };

    // Corner signs per face: top, bottom, south (+Z), north (-Z), west (-X), east (+X).
    let faces: [[(f32, f32, f32); 4]; 6] = [
        [
            (-1.0, 1.0, -1.0),
            (-1.0, 1.0, 1.0),
            (1.0, 1.0, 1.0),
            (1.0, 1.0, -1.0),
        ],
        [
            (-1.0, -1.0, -1.0),
            (1.0, -1.0, -1.0),
            (1.0, -1.0, 1.0),
            (-1.0, -1.0, 1.0),
        ],
        [
            (-1.0, -1.0, 1.0),
            (1.0, -1.0, 1.0),
            (1.0, 1.0, 1.0),
            (-1.0, 1.0, 1.0),
        ],
        [
            (1.0, -1.0, -1.0),
            (-1.0, -1.0, -1.0),
            (-1.0, 1.0, -1.0),
            (1.0, 1.0, -1.0),
        ],
        [
            (-1.0, -1.0, -1.0),
            (-1.0, -1.0, 1.0),
            (-1.0, 1.0, 1.0),
            (-1.0, 1.0, -1.0),
        ],
        [
            (1.0, -1.0, 1.0),
            (1.0, -1.0, -1.0),
            (1.0, 1.0, -1.0),
            (1.0, 1.0, 1.0),
        ],
    ];
    let uvs = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];

    for (face, shade_mult) in faces.iter().zip(PROP_FACE_SHADES) {
        let points = [
            corner(face[0].0, face[0].1, face[0].2),
            corner(face[1].0, face[1].1, face[1].2),
            corner(face[2].0, face[2].1, face[2].2),
            corner(face[3].0, face[3].1, face[3].2),
        ];
        let colors = points.map(|point| shaded(shade_mult, point));
        add_quad(
            vertices, points[0], colors[0], uvs[0], points[1], colors[1], uvs[1], points[2],
            colors[2], uvs[2], points[3], colors[3], uvs[3],
        );
    }
}

/// Wall face shading multipliers, shared by the wall builder and by decals so
/// a decal printed on a wall is shaded like the wall around it. Faces are the
/// ones the `WallDef::faces` names select: north/east are the low/high
/// thickness faces of an X/Z wall.
const WALL_FACE_NORTH_MULT: f32 = 1.00;
const WALL_FACE_SOUTH_MULT: f32 = 0.88;
const WALL_FACE_WEST_MULT: f32 = 0.84;
const WALL_FACE_EAST_MULT: f32 = 0.94;

/// Ceiling surfaces carry this tint so a ceiling panel is dimmer than the
/// fixture it hangs from. Decals on a ceiling use it too, for the same reason.
const CEILING_TINT: [f32; 3] = [0.72, 0.72, 0.70];

/// Distance a wall decal is probed away from its wall when sampling baked
/// lighting. Matching [`LIGHT_FACE_PROBE_M`] means a decal reads with exactly
/// the illumination of the wall face it is printed on.
const DECAL_WALL_LIGHT_PROBE_M: f32 = LIGHT_FACE_PROBE_M;
/// Distance a floor or ceiling decal is probed away from its plane. The floor
/// and ceiling grids sample on the plane itself, so this only needs to clear
/// the boundary the plane sits on, not a whole wall thickness.
const DECAL_HORIZONTAL_LIGHT_PROBE_M: f32 = 0.05;

/// The four world-space corners of a decal quad, in winding order.
///
/// The corners are `bottom-left, bottom-right, top-right, top-left` as seen
/// from the decal's normal side, so the triangle winding faces the normal and
/// the shared V axis points up the decal. Returns `None` for a decal whose
/// placement or size is not finite; the loader rejects those, but the builder
/// must never emit a NaN vertex.
#[must_use]
pub fn decal_quad_points(decal: &crate::level::DecalDef) -> Option<[[f32; 3]; 4]> {
    if !decal.x.is_finite()
        || !decal.y.is_finite()
        || !decal.z.is_finite()
        || !decal.width.is_finite()
        || !decal.height.is_finite()
        || !decal.rotation_degrees.is_finite()
        || decal.width <= 0.0
        || decal.height <= 0.0
    {
        return None;
    }

    let normal = glam::Vec3::from(decal.surface.normal());
    // `cross(up, normal)` gives the in-plane axis that reads left-to-right for
    // a viewer standing in front of a wall decal; horizontal surfaces have no
    // single such axis, so they start from world +X.
    let tangent = if decal.surface.is_horizontal() {
        glam::Vec3::X
    } else {
        glam::Vec3::Y.cross(normal).normalize_or_zero()
    };
    let bitangent = normal.cross(tangent);
    if tangent.length_squared() < 0.5 || bitangent.length_squared() < 0.5 {
        return None;
    }

    let (sin, cos) = decal.rotation_degrees.to_radians().sin_cos();
    let u_axis = tangent * cos + bitangent * sin;
    let v_axis = -tangent * sin + bitangent * cos;

    let center = glam::Vec3::new(decal.x, decal.y, decal.z);
    let [half_u, half_v] = decal.half_extents();
    let u = u_axis * half_u;
    let v = v_axis * half_v;
    let points = [
        center - u - v,
        center + u - v,
        center + u + v,
        center - u + v,
    ];
    points
        .iter()
        .all(|point| point.is_finite())
        .then(|| points.map(|point| point.to_array()))
}

/// Per-decal shading tint: the surface family's face shade, so a decal sits in
/// the same light as the surface it is printed on.
#[must_use]
fn decal_surface_tint(surface: crate::level::DecalSurface) -> [f32; 3] {
    use crate::level::DecalSurface;
    let mult = match surface {
        DecalSurface::Floor => 1.0,
        DecalSurface::Ceiling => return CEILING_TINT,
        DecalSurface::WallNorth => WALL_FACE_NORTH_MULT,
        DecalSurface::WallSouth => WALL_FACE_SOUTH_MULT,
        DecalSurface::WallWest => WALL_FACE_WEST_MULT,
        DecalSurface::WallEast => WALL_FACE_EAST_MULT,
    };
    [mult, mult, mult]
}

/// Emits one decal as a lit quad carrying the shared decal sheet.
///
/// The quad lies exactly on the authored surface — the depth relationship is
/// resolved in the decal pass by a fixed polygon offset, not by moving the
/// geometry — and its vertical placement follows the *actual* floor or ceiling
/// under it, so a decal in an elevated room or a recessed region stays on the
/// surface instead of being left behind at the authored world Y. Wall decals
/// keep their authored height, since a wall is not a horizontal surface.
fn add_decal_quad(
    vertices: &mut Vec<Vertex>,
    decal: &crate::level::DecalDef,
    surfaces: &LevelSurfaces<'_>,
    lighting: &LevelLighting,
    uv: [[f32; 2]; 4],
) {
    let Some(mut points) = decal_quad_points(decal) else {
        return;
    };
    let surface_y = |point: [f32; 3]| -> f32 {
        match decal.surface {
            crate::level::DecalSurface::Floor => {
                surfaces.floor_y_at(point[0], point[2]).unwrap_or(point[1])
            }
            crate::level::DecalSurface::Ceiling => surfaces.ceiling_y_at(point[0], point[2]),
            _ => point[1],
        }
    };
    if decal.surface.is_horizontal() {
        for point in &mut points {
            let y = surface_y(*point);
            if y.is_finite() {
                point[1] = y;
            }
        }
    }
    let normal = glam::Vec3::from(decal.surface.normal());
    let probe = if decal.surface.is_horizontal() {
        DECAL_HORIZONTAL_LIGHT_PROBE_M
    } else {
        DECAL_WALL_LIGHT_PROBE_M
    };
    let tint = decal_surface_tint(decal.surface);
    let colors = points.map(|point| {
        let sample = glam::Vec3::from(point) + normal * probe;
        shade(tint, lighting.sample(sample.x, sample.y, sample.z))
    });
    add_quad(
        vertices, points[0], colors[0], uv[0], points[1], colors[1], uv[1], points[2], colors[2],
        uv[2], points[3], colors[3], uv[3],
    );
}

/// True when a room can be tessellated without producing invalid geometry.
///
/// Malformed rooms (non-finite or non-positive dimensions) are skipped rather
/// than allowed to poison the vertex buffer with NaN positions; the loader
/// rejects them long before this point.
fn room_is_tessellatable(room: &crate::level::RoomDef) -> bool {
    room.x.is_finite()
        && room.z.is_finite()
        && room.height.is_finite()
        && room.width > 0.0
        && room.depth > 0.0
}

/// Colour difference below which adjacent baked-lighting cells may be merged
/// into a single quad.
///
/// 1/512 is under half of one 8-bit colour step (1/255), so a merged surface is
/// indistinguishable on screen from the per-cell surface it replaces, while
/// surfaces that carry no lighting gradient (unlit rooms, rooms far from every
/// fixture, the flanks of large rooms) collapse back to one quad per region.
const LIGHT_GRID_MERGE_EPS: f32 = 1.0 / 512.0;

/// True when every corner of the grid rectangle spanning cells
/// `ix0..=ix1` × `iz0..=iz1` is within [`LIGHT_GRID_MERGE_EPS`] of `reference`.
fn grid_rect_is_uniform(
    colors: &[[f32; 3]],
    row_len: usize,
    ix0: usize,
    ix1: usize,
    iz0: usize,
    iz1: usize,
    reference: [f32; 3],
) -> bool {
    for iz in iz0..=iz1 + 1 {
        for ix in ix0..=ix1 + 1 {
            let color = colors[iz * row_len + ix];
            for channel in 0..3 {
                if (color[channel] - reference[channel]).abs() > LIGHT_GRID_MERGE_EPS {
                    return false;
                }
            }
        }
    }
    true
}

/// Height tolerance within which a merged surface counts as planar, in metres.
const HEIGHT_MERGE_EPS: f32 = 1e-4;

/// True when the surface heights over a grid rectangle are coplanar, so a
/// merged quad cannot fold across a slope or a ridge.
///
/// The check compares every corner of the rectangle with the bilinear
/// interpolation of three of them, which is exact for the piecewise-linear
/// surfaces the builder emits (flat planes and gable slopes).
fn grid_rect_is_planar(
    xs: &[f32],
    zs: &[f32],
    y_at: &impl Fn(f32, f32) -> f32,
    ix0: usize,
    ix1: usize,
    iz0: usize,
    iz1: usize,
) -> bool {
    let (x0, x1) = (xs[ix0], xs[ix1 + 1]);
    let (z0, z1) = (zs[iz0], zs[iz1 + 1]);
    let (span_x, span_z) = (x1 - x0, z1 - z0);
    if span_x <= 0.0 || span_z <= 0.0 {
        return false;
    }
    let (y00, y10, y01) = (y_at(x0, z0), y_at(x1, z0), y_at(x0, z1));
    for z in &zs[iz0..=iz1 + 1] {
        for x in &xs[ix0..=ix1 + 1] {
            let expected = y00 + (y10 - y00) * (x - x0) / span_x + (y01 - y00) * (z - z0) / span_z;
            if (y_at(*x, *z) - expected).abs() > HEIGHT_MERGE_EPS {
                return false;
            }
        }
    }
    true
}

/// One lit surface to emit: where it sits, which way it faces, and which
/// material region of its grid this call covers.
struct LitSurface<'a, Y: Fn(f32, f32) -> f32> {
    /// World Y of the surface at a grid corner. A flat floor returns a
    /// constant; a ceiling built on a gable profile returns the profile height,
    /// so the mesh conforms to the same function lighting and collision use.
    y_at: Y,
    /// Ceilings run the opposite winding to floors so they face down.
    ceiling: bool,
    /// `Some((label, cell_labels))` emits only the cells carrying that label and
    /// never merges across a label boundary, which is what gives a floor patch
    /// or floor region its exact rectangular edge. `None` emits every cell.
    region: Option<(u32, &'a [u32])>,
}

/// Emits one lit floor or ceiling from a precomputed corner-colour grid.
///
/// Cells are greedily merged along X and then Z while every corner of the
/// candidate rectangle stays within [`LIGHT_GRID_MERGE_EPS`] and the surface
/// stays planar over it, so uniform regions cost one quad instead of up to
/// `MAX_LIGHT_GRID_CELLS`² of them. The surviving corners keep their exact
/// sampled colours and heights; UVs stay world-space, so merging is invisible
/// to texturing.
fn emit_lit_surface_grid(
    vertices: &mut Vec<Vertex>,
    xs: &[f32],
    zs: &[f32],
    colors: &[[f32; 3]],
    surface: LitSurface<'_, impl Fn(f32, f32) -> f32>,
    uv: impl Fn(f32, f32) -> [f32; 2],
) {
    let LitSurface {
        y_at,
        ceiling,
        region,
    } = surface;
    let cells_x = xs.len().saturating_sub(1);
    let cells_z = zs.len().saturating_sub(1);
    if cells_x == 0 || cells_z == 0 {
        return;
    }
    let row_len = xs.len();
    // Cells outside the selected region count as covered, so a growing
    // rectangle stops at the label boundary.
    let mut covered = vec![false; cells_x * cells_z];
    if let Some((label, labels)) = region {
        for (index, cell) in covered.iter_mut().enumerate() {
            *cell = labels.get(index).copied() != Some(label);
        }
    }
    // True when every cell of the `ix0..=ix1` x `iz0..=iz1` block is still
    // uncovered, so a growing rectangle can never re-emit an earlier one.
    let region_free = |covered: &[bool], ix0: usize, ix1: usize, iz0: usize, iz1: usize| {
        (iz0..=iz1).all(|z| (ix0..=ix1).all(|x| !covered[z * cells_x + x]))
    };
    for iz in 0..cells_z {
        for ix in 0..cells_x {
            if covered[iz * cells_x + ix] {
                continue;
            }
            let reference = colors[iz * row_len + ix];
            let mut ix1 = ix;
            while ix1 + 1 < cells_x
                && region_free(&covered, ix, ix1 + 1, iz, iz)
                && grid_rect_is_uniform(colors, row_len, ix, ix1 + 1, iz, iz, reference)
                && grid_rect_is_planar(xs, zs, &y_at, ix, ix1 + 1, iz, iz)
            {
                ix1 += 1;
            }
            let mut iz1 = iz;
            while iz1 + 1 < cells_z
                && region_free(&covered, ix, ix1, iz, iz1 + 1)
                && grid_rect_is_uniform(colors, row_len, ix, ix1, iz, iz1 + 1, reference)
                && grid_rect_is_planar(xs, zs, &y_at, ix, ix1, iz, iz1 + 1)
            {
                iz1 += 1;
            }
            for z in iz..=iz1 {
                for x in ix..=ix1 {
                    covered[z * cells_x + x] = true;
                }
            }

            let (ax, bx) = (xs[ix], xs[ix1 + 1]);
            let (az, bz) = (zs[iz], zs[iz1 + 1]);
            let c00 = colors[iz * row_len + ix];
            let c10 = colors[iz * row_len + ix1 + 1];
            let c11 = colors[(iz1 + 1) * row_len + ix1 + 1];
            let c01 = colors[(iz1 + 1) * row_len + ix];
            // Winding is the project convention: a face's front side is the
            // side its normal points to (right-hand rule over p0 -> p1 -> p2),
            // and every world face is wound to point *out* of the solid. So a
            // floor faces +Y and a ceiling faces -Y (a gable slope's normal tilts
            // but its vertical component keeps the same sign). Culling is off
            // today, but collision, decals and a future cull-enabled build all
            // read this convention.
            let (points, corners) = if ceiling {
                (
                    [
                        [ax, y_at(ax, az), az],
                        [bx, y_at(bx, az), az],
                        [bx, y_at(bx, bz), bz],
                        [ax, y_at(ax, bz), bz],
                    ],
                    [c00, c10, c11, c01],
                )
            } else {
                (
                    [
                        [ax, y_at(ax, bz), bz],
                        [bx, y_at(bx, bz), bz],
                        [bx, y_at(bx, az), az],
                        [ax, y_at(ax, az), az],
                    ],
                    [c01, c11, c10, c00],
                )
            };
            add_quad(
                vertices,
                points[0],
                corners[0],
                uv(points[0][0], points[0][2]),
                points[1],
                corners[1],
                uv(points[1][0], points[1][2]),
                points[2],
                corners[2],
                uv(points[2][0], points[2][2]),
                points[3],
                corners[3],
                uv(points[3][0], points[3][2]),
            );
        }
    }
}

/// Samples a `(cells_x + 1) x (cells_z + 1)` corner grid of baked colours.
///
/// The surface height is supplied per corner, so a recessed floor region or a
/// gable slope is lit by the baked illumination at its real world position.
fn lit_surface_grid(
    lighting: &LevelLighting,
    room_index: usize,
    xs: &[f32],
    zs: &[f32],
    y_at: impl Fn(f32, f32) -> f32,
    tint: Option<[f32; 3]>,
) -> Vec<[f32; 3]> {
    let mut colors = Vec::with_capacity(xs.len() * zs.len());
    for z in zs {
        for x in xs {
            let light = lighting.sample_in_room(room_index, *x, y_at(*x, *z), *z);
            colors.push(tint.map_or([light.r, light.g, light.b], |tint| {
                [tint[0] * light.r, tint[1] * light.g, tint[2] * light.b]
            }));
        }
    }
    colors
}

/// True when `(x, z)` lies inside a floor patch's rectangle.
fn patch_contains(patch: &FloorPatchDef, x: f32, z: f32) -> bool {
    let x0 = patch.x.min(patch.x + patch.width);
    let x1 = patch.x.max(patch.x + patch.width);
    let z0 = patch.z.min(patch.z + patch.depth);
    let z1 = patch.z.max(patch.z + patch.depth);
    x >= x0 && x <= x1 && z >= z0 && z <= z1
}

/// One resolved floor surface of a room: the vertical offset of its cells from
/// the room floor and the material key they draw with.
#[derive(Clone, Copy, PartialEq)]
struct FloorSurface {
    offset: f32,
    key: SurfaceKey,
}

/// Resolves every floor grid cell of a room to its surface (height offset and
/// material), returning the distinct surfaces and one label per cell.
///
/// Later floor patches win over earlier ones, and both sit on top of the room's
/// own floor material. The height comes from the shared grid the collision rims
/// and the walkable surface are built from, so a cell's label can never disagree
/// with the height the player stands at.
fn floor_surfaces(
    grid: &RoomFloorGrid,
    surfaces_at: &LevelSurfaces<'_>,
    base_key: SurfaceKey,
    patches: &[&FloorPatchDef],
    materials: &MaterialLookup<'_>,
) -> (Vec<FloorSurface>, Vec<u32>) {
    let (cells_x, cells_z) = (grid.cells_x(), grid.cells_z());
    let mut surfaces: Vec<FloorSurface> = Vec::new();
    let mut labels = vec![0u32; cells_x * cells_z];
    for iz in 0..cells_z {
        let z = f32::midpoint(grid.zs[iz], grid.zs[iz + 1]);
        for ix in 0..cells_x {
            let x = f32::midpoint(grid.xs[ix], grid.xs[ix + 1]);
            // Material precedence: the region's own material, then the latest
            // floor patch, then the room's floor material.
            let key = surfaces_at
                .region_at(x, z)
                .and_then(|region| region.material.as_deref())
                .map_or_else(
                    || {
                        patches
                            .iter()
                            .rev()
                            .find(|patch| patch_contains(patch, x, z))
                            .map_or(base_key, |patch| {
                                materials.key(MaterialSlot::Floor, &patch.material)
                            })
                    },
                    |material| materials.key(MaterialSlot::Floor, material),
                );
            let resolved = FloorSurface {
                offset: grid.offset_at(ix, iz),
                key,
            };
            let label = surfaces
                .iter()
                .position(|existing| *existing == resolved)
                .map_or_else(
                    || {
                        surfaces.push(resolved);
                        u32::try_from(surfaces.len() - 1).unwrap_or(u32::MAX)
                    },
                    |index| u32::try_from(index).unwrap_or(u32::MAX),
                );
            labels[iz * cells_x + ix] = label;
        }
    }
    (surfaces, labels)
}

/// Corners of a vertical transition face, wound so the quad's normal points
/// toward the lower floor (out of the higher floor's volume).
fn skirt_points(
    axis: WallAxis,
    positive: bool,
    at: f32,
    span: (f32, f32),
    low: f32,
    high: f32,
) -> [[f32; 3]; 4] {
    let (s0, s1) = span;
    match (axis, positive) {
        (WallAxis::X, false) => [[at, low, s0], [at, low, s1], [at, high, s1], [at, high, s0]],
        (WallAxis::X, true) => [[at, low, s1], [at, low, s0], [at, high, s0], [at, high, s1]],
        (WallAxis::Z, false) => [[s1, low, at], [s0, low, at], [s0, high, at], [s1, high, at]],
        (WallAxis::Z, true) => [[s0, low, at], [s1, low, at], [s1, high, at], [s0, high, at]],
    }
}

/// Emits the vertical transition faces around a room's floor regions.
///
/// Every grid edge whose two sides stand at different heights gets a real quad
/// spanning the difference, and a region cell at the room's boundary is closed
/// against the room's own floor plane, so a depression is never an open hole
/// into the void. The face carries the region's `edge_material` when authored,
/// otherwise the room's wall material, so transition surfaces always have a
/// deterministic texture.
fn emit_floor_skirts(
    buckets: &mut crate::spatial::SpatialBuckets<SurfaceKey>,
    scratch: &mut Vec<Vertex>,
    room: &RoomDef,
    grid: &RoomFloorGrid,
    level: &LevelDef,
    lighting: &LevelLighting,
    materials: &MaterialLookup<'_>,
) {
    let (cells_x, cells_z) = (grid.cells_x(), grid.cells_z());
    if cells_x == 0 || cells_z == 0 {
        return;
    }
    // A room has floor and ceiling materials but no wall material of its own, so
    // the documented fallback for a transition face is the level's wall material.
    // A region with its own `edge_material` overrides it.
    let default_edge = materials.key(MaterialSlot::Wall, level.defaults.wall.as_str());

    let mut emit = |axis: WallAxis,
                    positive: bool,
                    at: f32,
                    span: (f32, f32),
                    low: f32,
                    high: f32,
                    key: SurfaceKey| {
        if high - low <= HEIGHT_MERGE_EPS {
            return;
        }
        let points = skirt_points(axis, positive, at, span, low, high);
        let mult = match (axis, positive) {
            (WallAxis::Z, false) => WALL_FACE_NORTH_MULT,
            (WallAxis::Z, true) => WALL_FACE_SOUTH_MULT,
            (WallAxis::X, false) => WALL_FACE_WEST_MULT,
            (WallAxis::X, true) => WALL_FACE_EAST_MULT,
        };
        let tint = materials.tint(key);
        let shade_of = |grad: f32| {
            [
                (tint[0] * mult * grad).min(1.0),
                (tint[1] * mult * grad).min(1.0),
                (tint[2] * mult * grad).min(1.0),
            ]
        };
        let bottom_shade = shade_of(0.92);
        let top_shade = shade_of(1.05);
        // Low corners take the darker shade, high corners the brighter one, in
        // the same order the wall emitter shades sills and headers.
        let base: [[f32; 3]; 4] = [bottom_shade, bottom_shade, top_shade, top_shade];
        let colors: [[f32; 3]; 4] = std::array::from_fn(|index| {
            shade(
                base[index],
                lighting.sample(points[index][0], points[index][1], points[index][2]),
            )
        });
        let uv = |point: [f32; 3]| match axis {
            WallAxis::X => materials.uv(key, point[2], high - point[1]),
            WallAxis::Z => materials.uv(key, point[0], high - point[1]),
        };
        scratch.clear();
        add_quad(
            scratch,
            points[0],
            colors[0],
            uv(points[0]),
            points[1],
            colors[1],
            uv(points[1]),
            points[2],
            colors[2],
            uv(points[2]),
            points[3],
            colors[3],
            uv(points[3]),
        );
        buckets.add_quads(key, scratch);
    };

    let edge_key = |x: f32, z: f32| -> SurfaceKey {
        let (x0, x1, z0, z1) = room.bounds();
        level
            .floor_regions
            .iter()
            .rev()
            .find(|region| {
                let (rx0, rx1, rz0, rz1) = region.bounds();
                rx1 > x0 && rx0 < x1 && rz1 > z0 && rz0 < z1 && region.contains(x, z)
            })
            .and_then(|region| region.edge_material.as_deref())
            .map_or(default_edge, |material| {
                materials.key(MaterialSlot::Wall, material)
            })
    };

    // The region owning a cell is the one whose material describes the faces
    // that cell's height difference creates.
    let cell_key = |ix: usize, iz: usize| -> SurfaceKey {
        let x = f32::midpoint(grid.xs[ix], grid.xs[ix + 1]);
        let z = f32::midpoint(grid.zs[iz], grid.zs[iz + 1]);
        edge_key(x, z)
    };

    for iz in 0..cells_z {
        for ix in 0..cells_x {
            let y = grid.y_at(room, ix, iz);
            // Transition to the next cell along X, or to the room's own floor
            // plane when this is the room's last column.
            let right = if ix + 1 < cells_x {
                Some(grid.y_at(room, ix + 1, iz))
            } else {
                Some(room.floor_y)
            };
            if let Some(other) = right
                && (other - y).abs() > HEIGHT_MERGE_EPS
            {
                let key = if ix + 1 < cells_x {
                    cell_key(ix + 1, iz)
                } else {
                    cell_key(ix, iz)
                };
                emit(
                    WallAxis::X,
                    // The face belongs to the lower side's volume, so its
                    // normal points toward it: a recess wall faces into the
                    // recess, a raised platform's rim faces outward.
                    y > other,
                    grid.xs[ix + 1],
                    (grid.zs[iz], grid.zs[iz + 1]),
                    y.min(other),
                    y.max(other),
                    key,
                );
            }

            let back = if iz + 1 < cells_z {
                Some(grid.y_at(room, ix, iz + 1))
            } else {
                Some(room.floor_y)
            };
            if let Some(other) = back
                && (other - y).abs() > HEIGHT_MERGE_EPS
            {
                let key = if iz + 1 < cells_z {
                    cell_key(ix, iz + 1)
                } else {
                    cell_key(ix, iz)
                };
                emit(
                    WallAxis::Z,
                    y > other,
                    grid.zs[iz + 1],
                    (grid.xs[ix], grid.xs[ix + 1]),
                    y.min(other),
                    y.max(other),
                    key,
                );
            }
        }
    }
}

/// Flushes the quads appended to `scratch` since `cursor` into `key`'s bucket.
///
/// Walls write their length faces, sills, headers and reveals into one scratch
/// buffer; flushing the run a face produced is what lets each face carry its own
/// material while the emitters stay unchanged. Whole quads only, as everywhere
/// else in the builder.
fn flush_wall_run(
    buckets: &mut crate::spatial::SpatialBuckets<SurfaceKey>,
    scratch: &[Vertex],
    cursor: &mut usize,
    key: SurfaceKey,
) {
    if *cursor >= scratch.len() {
        return;
    }
    buckets.add_quads(key, &scratch[*cursor..]);
    *cursor = scratch.len();
}

/// Emits the office fluorescent panel: a luminous panel with two bezel strips,
/// all facing down into the room.
fn add_panel_fixture(
    scratch: &mut Vec<Vertex>,
    x0: f32,
    x1: f32,
    z0: f32,
    z1: f32,
    y: f32,
    glow: [f32; 3],
) {
    add_quad_flat(
        scratch,
        [x0, y, z0],
        [x1, y, z0],
        [x1, y, z1],
        [x0, y, z1],
        glow,
        [0.0, 0.0],
        [1.0, 0.0],
        [1.0, 1.0],
        [0.0, 1.0],
    );

    let bezel_color = [0.40, 0.40, 0.40];
    let b = 0.05;
    add_quad_flat(
        scratch,
        [x0 - b, y, z0 - b],
        [x1 + b, y, z0 - b],
        [x1 + b, y, z0],
        [x0 - b, y, z0],
        bezel_color,
        [0.0, 0.0],
        [1.0, 0.0],
        [1.0, 1.0],
        [0.0, 1.0],
    );
    add_quad_flat(
        scratch,
        [x0 - b, y, z1],
        [x1 + b, y, z1],
        [x1 + b, y, z1 + b],
        [x0 - b, y, z1 + b],
        bezel_color,
        [0.0, 0.0],
        [1.0, 0.0],
        [1.0, 1.0],
        [0.0, 1.0],
    );
}

/// One flat ring quad of a round fixture, facing down.
#[allow(clippy::too_many_arguments)]
fn add_ring_quad(
    scratch: &mut Vec<Vertex>,
    cx: f32,
    cz: f32,
    y: f32,
    r_in: f32,
    r_out: f32,
    cos0: f32,
    sin0: f32,
    cos1: f32,
    sin1: f32,
    color: [f32; 3],
) {
    let outer0 = [cx + r_out * cos0, y, cz + r_out * sin0];
    let outer1 = [cx + r_out * cos1, y, cz + r_out * sin1];
    let inner1 = [cx + r_in * cos1, y, cz + r_in * sin1];
    let inner0 = [cx + r_in * cos0, y, cz + r_in * sin0];
    add_quad_flat(
        scratch,
        outer0,
        outer1,
        inner1,
        inner0,
        color,
        [0.0, 0.0],
        [1.0, 0.0],
        [1.0, 1.0],
        [0.0, 1.0],
    );
}

/// One outward-facing side quad of a round fixture's shallow can.
#[allow(clippy::too_many_arguments)]
fn add_can_quad(
    scratch: &mut Vec<Vertex>,
    cx: f32,
    cz: f32,
    y_top: f32,
    y_bottom: f32,
    radius: f32,
    cos0: f32,
    sin0: f32,
    cos1: f32,
    sin1: f32,
    color: [f32; 3],
) {
    let top0 = [cx + radius * cos0, y_top, cz + radius * sin0];
    let top1 = [cx + radius * cos1, y_top, cz + radius * sin1];
    let bottom1 = [cx + radius * cos1, y_bottom, cz + radius * sin1];
    let bottom0 = [cx + radius * cos0, y_bottom, cz + radius * sin0];
    add_quad_flat(
        scratch,
        top0,
        top1,
        bottom1,
        bottom0,
        color,
        [0.0, 0.0],
        [1.0, 0.0],
        [1.0, 1.0],
        [0.0, 1.0],
    );
}

/// Emits a round recessed ceiling downlight: a shallow can, a flat bezel ring
/// and an emissive diffuser ring, all facing down into the room.
///
/// The diffuser is a ring rather than a filled disc so the fixture stays
/// quad-only; the small centre it leaves reads as the lamp recess behind a
/// nearly-closed diffuser.
fn add_round_fixture(
    scratch: &mut Vec<Vertex>,
    cx: f32,
    cz: f32,
    y: f32,
    radius: f32,
    glow: [f32; 3],
) {
    const SEGMENTS: usize = 10;
    const BEZEL_COLOR: [f32; 3] = [0.40, 0.40, 0.40];
    /// Depth of the visible can below the ceiling plane, in metres.
    const CAN_DEPTH: f32 = 0.03;
    let inner = radius * 0.12;
    let bezel_outer = radius + 0.03;
    for segment in 0..SEGMENTS {
        let a0 = segment as f32 / SEGMENTS as f32 * std::f32::consts::TAU;
        let a1 = (segment + 1) as f32 / SEGMENTS as f32 * std::f32::consts::TAU;
        let (sin0, cos0) = a0.sin_cos();
        let (sin1, cos1) = a1.sin_cos();
        add_ring_quad(
            scratch, cx, cz, y, inner, radius, cos0, sin0, cos1, sin1, glow,
        );
        add_ring_quad(
            scratch,
            cx,
            cz,
            y,
            radius,
            bezel_outer,
            cos0,
            sin0,
            cos1,
            sin1,
            BEZEL_COLOR,
        );
        add_can_quad(
            scratch,
            cx,
            cz,
            y,
            y - CAN_DEPTH,
            bezel_outer,
            cos0,
            sin0,
            cos1,
            sin1,
            BEZEL_COLOR,
        );
    }
}

/// Emits a wall-mounted luminaire at `(x, y, z)` facing `yaw_degrees`:
/// a shallow housing with one emissive outward face.
fn add_wall_fixture(
    scratch: &mut Vec<Vertex>,
    x: f32,
    y: f32,
    z: f32,
    yaw_degrees: f32,
    glow: [f32; 3],
) {
    const HALF_WIDTH: f32 = 0.20;
    const HALF_HEIGHT: f32 = 0.10;
    const DEPTH: f32 = 0.11;
    const BEZEL_COLOR: [f32; 3] = [0.40, 0.40, 0.40];
    let yaw = yaw_degrees.to_radians();
    let (sin, cos) = yaw.sin_cos();
    // `+Z` is the fixture's front, exactly like a prop at rotation 0.
    let forward = [sin, 0.0, cos];
    let right = [cos, 0.0, -sin];
    let point = |u: f32, v: f32, d: f32| {
        [
            x + right[0] * u + forward[0] * d,
            y + v,
            z + right[2] * u + forward[2] * d,
        ]
    };
    let uv = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    // Emissive front face.
    add_quad_flat(
        scratch,
        point(-HALF_WIDTH, -HALF_HEIGHT, DEPTH),
        point(HALF_WIDTH, -HALF_HEIGHT, DEPTH),
        point(HALF_WIDTH, HALF_HEIGHT, DEPTH),
        point(-HALF_WIDTH, HALF_HEIGHT, DEPTH),
        glow,
        uv[0],
        uv[1],
        uv[2],
        uv[3],
    );
    // Top face (up), bottom face (down) and the two ends.
    add_quad_flat(
        scratch,
        point(-HALF_WIDTH, HALF_HEIGHT, 0.0),
        point(-HALF_WIDTH, HALF_HEIGHT, DEPTH),
        point(HALF_WIDTH, HALF_HEIGHT, DEPTH),
        point(HALF_WIDTH, HALF_HEIGHT, 0.0),
        BEZEL_COLOR,
        uv[0],
        uv[1],
        uv[2],
        uv[3],
    );
    add_quad_flat(
        scratch,
        point(-HALF_WIDTH, -HALF_HEIGHT, 0.0),
        point(HALF_WIDTH, -HALF_HEIGHT, 0.0),
        point(HALF_WIDTH, -HALF_HEIGHT, DEPTH),
        point(-HALF_WIDTH, -HALF_HEIGHT, DEPTH),
        BEZEL_COLOR,
        uv[0],
        uv[1],
        uv[2],
        uv[3],
    );
    add_quad_flat(
        scratch,
        point(HALF_WIDTH, HALF_HEIGHT, 0.0),
        point(HALF_WIDTH, HALF_HEIGHT, DEPTH),
        point(HALF_WIDTH, -HALF_HEIGHT, DEPTH),
        point(HALF_WIDTH, -HALF_HEIGHT, 0.0),
        BEZEL_COLOR,
        uv[0],
        uv[1],
        uv[2],
        uv[3],
    );
    add_quad_flat(
        scratch,
        point(-HALF_WIDTH, -HALF_HEIGHT, 0.0),
        point(-HALF_WIDTH, -HALF_HEIGHT, DEPTH),
        point(-HALF_WIDTH, HALF_HEIGHT, DEPTH),
        point(-HALF_WIDTH, HALF_HEIGHT, 0.0),
        BEZEL_COLOR,
        uv[0],
        uv[1],
        uv[2],
        uv[3],
    );
}

fn build_level_geometry_mesh(
    level: &LevelDef,
    catalog: &crate::loader::PropCatalog,
    fallback_props: &[&PropDef],
    lighting: &LevelLighting,
    materials: &MaterialTable,
) -> LevelMesh {
    // Collect the merged room list once; geometry and ceiling lookups then
    // borrow it instead of cloning the room vector repeatedly.
    let rooms: Vec<_> = level.room_iter().collect();
    // The shared vertical geometry model: every floor, ceiling and wall height
    // below comes from it, and collision and the walkable surface use the same
    // queries, so the mesh cannot drift from what the player stands on.
    let surfaces = LevelSurfaces::new(level);
    // Emitters still write whole quads into one scratch buffer; the bucket
    // builder splits each run by spatial cell on the way into the mesh. That
    // keeps the emitting code free of any grid awareness.
    let materials = MaterialLookup::new(materials);
    let mut scratch: Vec<Vertex> = Vec::new();
    let mut buckets =
        crate::spatial::SpatialBuckets::<SurfaceKey>::with_grid(spatial_cell_grid(level));

    // 1. Floor batch: the baked-lighting grid over the room, sampled once per
    //    corner and greedily merged wherever the lighting is effectively flat
    //    (unlit rooms and the far flanks of large rooms therefore stay one or
    //    two quads). The cell count is bounded by `lighting::MAX_LIGHT_GRID_CELLS`,
    //    and UVs keep mapping world space at the material's tiling period, so
    //    the checkered seed carpet and a Goal 5 replacement tile identically.
    //
    //    A room that carries floor patches or floor regions is cut at their
    //    edges and emitted one material/height surface at a time, so a damp
    //    patch has an exact edge and a recess sits at its real elevation without
    //    a second overlapping slab (design section 23).
    for (room_index, room) in rooms.iter().enumerate() {
        if !room_is_tessellatable(room) {
            continue;
        }
        let base_material = room
            .material
            .as_deref()
            .unwrap_or(level.defaults.floor.as_str());
        let base_key = materials.key(MaterialSlot::Floor, base_material);
        let room_patches = surfaces.patches_for_room(room);
        let grid = surfaces.floor_grid(room);
        let (floor_surfaces, labels) =
            floor_surfaces(&grid, &surfaces, base_key, &room_patches, &materials);

        for (label, surface) in floor_surfaces.iter().enumerate() {
            let y = room.floor_y + surface.offset;
            let tint = materials.tint(surface.key);
            let colors = lit_surface_grid(
                lighting,
                room_index,
                &grid.xs,
                &grid.zs,
                |_, _| y,
                Some(tint),
            );
            let tile = materials.tile_metres(surface.key);
            scratch.clear();
            emit_lit_surface_grid(
                &mut scratch,
                &grid.xs,
                &grid.zs,
                &colors,
                LitSurface {
                    y_at: |_, _| y,
                    ceiling: false,
                    region: Some((u32::try_from(label).unwrap_or(u32::MAX), &labels)),
                },
                |x, z| tiled_uv(x, z, tile),
            );
            buckets.add_quads(surface.key, &scratch);
        }

        // The vertical faces of a recessed or raised region are real geometry,
        // not a hole into the void.
        emit_floor_skirts(
            &mut buckets,
            &mut scratch,
            room,
            &grid,
            level,
            lighting,
            &materials,
        );
    }

    // 2. Ceiling batch: the same grid and the same lighting sample, with the
    //    fixture panels themselves drawn brighter by the light batch below.
    //    Ceilings carry no patches, only the room's ceiling material and its
    //    ceiling profile; a gable is emitted as two real slopes meeting at the
    //    ridge, never as a hidden flat plane above a decorative prop.
    for (room_index, room) in rooms.iter().enumerate() {
        if !room_is_tessellatable(room) {
            continue;
        }
        let ceiling_key = materials.key(
            MaterialSlot::Ceiling,
            room.ceiling_material
                .as_deref()
                .unwrap_or(level.defaults.ceiling.as_str()),
        );
        let (xs, zs) = surfaces.ceiling_grid(room);
        let ceiling_at = |x: f32, z: f32| room.ceiling_y_at(x, z);
        let colors = lit_surface_grid(
            lighting,
            room_index,
            &xs,
            &zs,
            ceiling_at,
            Some(materials.tint(ceiling_key)),
        );
        let tile = materials.tile_metres(ceiling_key);
        scratch.clear();
        emit_lit_surface_grid(
            &mut scratch,
            &xs,
            &zs,
            &colors,
            LitSurface {
                y_at: ceiling_at,
                ceiling: true,
                region: None,
            },
            |x, z| tiled_uv(x, z, tile),
        );
        buckets.add_quads(ceiling_key, &scratch);
    }

    // 3. Walls batch. Each length face draws with its own material: the `faces`
    //    override for its direction, else the wall's own `material`, else the
    //    level default. Sills, headers and reveal jambs follow the wall's
    //    material, each sampling its own texture at the material's tiling.
    //
    //    Coincident collinear walls are first resolved into single emission
    //    units (`wall_units`), so a water-damaged wall segment authored as a
    //    duplicate surface becomes a material run on the one physical wall
    //    instead of a second coplanar mesh.
    let wall_units = wall_units(level, &surfaces, &materials);

    for unit in &wall_units {
        let wall = unit.wall();
        scratch.clear();
        let wall_material = wall
            .material
            .as_deref()
            .unwrap_or(level.defaults.wall.as_str());
        let wall_key = materials.key(MaterialSlot::Wall, wall_material);
        let face_key = |name: &str| {
            let material = wall.faces.get(name).map_or(wall_material, String::as_str);
            materials.key(MaterialSlot::Wall, material)
        };
        let x0 = wall.x.min(wall.x + wall.width);
        let x1 = wall.x.max(wall.x + wall.width);
        let z0 = wall.z.min(wall.z + wall.depth);
        let z1 = wall.z.max(wall.z + wall.depth);
        // A wall without an authored height follows the room's ceiling profile:
        // its top is the ceiling at each length position, so gable-end walls
        // reach the ridge and eave walls stay flat at the eave.
        let breaks = surfaces.wall_profile_breaks(wall);
        let (wall_base, _) = wall_vertical_extent(wall, &surfaces);
        let ceiling_along = |offset: f32| surfaces.ceiling_y_along(wall, offset);
        let floor_along = |offset: f32| {
            let (x, z) = crate::level::wall_point(wall, offset);
            surfaces
                .floor_y_at(x, z)
                .or_else(|| surfaces.room_floor_y_at(x, z))
                .unwrap_or(0.0)
        };

        let top_grad = 1.05;
        let bot_grad = 0.92;
        // Reveal faces are deliberately darker than the wall faces they
        // interrupt, so doorways and windows read clearly.
        let jamb_mult = 0.78;
        let head_mult = 0.92;

        // A wall face's albedo is its material's tint, scaled by the
        // directional face multiplier and the bottom/top gradient. Nothing
        // here knows a material id: the tint comes from the resolved table.
        let scale_color = |key: SurfaceKey, mult: f32, grad: f32| -> [f32; 3] {
            let tint = materials.tint(key);
            [
                (tint[0] * mult * grad).min(1.0),
                (tint[1] * mult * grad).min(1.0),
                (tint[2] * mult * grad).min(1.0),
            ]
        };

        // The axis the wall's length runs along and the world span across its
        // thickness. Local slice offsets start at the wall's min corner.
        let axis = wall.axis();
        let (origin_x, origin_z) = wall.length_origin();
        let (t0, t1) = match axis {
            WallAxis::X => (z0, z1),
            WallAxis::Z => (x0, x1),
        };
        let slices = wall_solid_slices_profiled(
            wall,
            |offset| surfaces.clear_ceiling_height_along(wall, offset),
            &breaks,
        );
        // Cursor into `scratch` for the current face's quads; see
        // `flush_wall_run`.
        let mut wall_cursor = 0usize;

        // Each solid slice emits the two wall faces parallel to its length
        // axis, plus a top/bottom face where the slice does not reach the
        // ceiling or the wall base (window sills, door headers).
        for slice in &slices {
            let (l0, l1) = match axis {
                WallAxis::X => (origin_x + slice.start, origin_x + slice.end),
                WallAxis::Z => (origin_z + slice.start, origin_z + slice.end),
            };
            let (slice_bottom, slice_top) = (slice.bottom, slice.top);
            let slice_mid = f32::midpoint(slice.start, slice.end);
            // A wall without an authored height is bounded by the ceiling: its
            // visible top is the slice's top clipped to the ceiling directly
            // above, so a wall running up a gable slope reaches the real ceiling
            // instead of poking through it. An authored height is a rigid wall
            // and is drawn exactly as written, which is what lets a raised wall
            // span two rooms with different ceiling heights.
            // The emitter passes world coordinates along the length axis, so
            // the ceiling is resolved at the matching world point.
            let ceiling_bounded = wall.height.is_none();
            let ceiling_at_world = |at: f32| match axis {
                WallAxis::X => surfaces.ceiling_y_at(at, f32::midpoint(z0, z1)),
                WallAxis::Z => surfaces.ceiling_y_at(f32::midpoint(x0, x1), at),
            };
            let visible_top = move |at: f32| {
                if ceiling_bounded {
                    slice_top.min(ceiling_at_world(at))
                } else {
                    slice_top
                }
            };

            // Faces parallel to the length axis: north/south for X-axis
            // walls, west/east for Z-axis walls. Each face is a strip of quads
            // so the baked lighting varies along the wall.
            //
            // (face coordinate across the thickness, outward normal, the face
            // multiplier, whether the winding runs against the length axis, the
            // direction name used by `faces`).
            // `flip_u` makes each face read unmirrored from the side its
            // normal points into: on an X-axis wall `+X` runs to the viewer's
            // left on the north side, and on a Z-axis wall `+Z` runs left on
            // the west side.
            let faces: [(f32, f32, f32, bool, bool, &'static str); 2] = match axis {
                WallAxis::X => [
                    (z0, -1.0, WALL_FACE_NORTH_MULT, false, true, "north"),
                    (z1, 1.0, WALL_FACE_SOUTH_MULT, true, false, "south"),
                ],
                WallAxis::Z => [
                    (x0, -1.0, WALL_FACE_WEST_MULT, true, true, "west"),
                    (x1, 1.0, WALL_FACE_EAST_MULT, false, false, "east"),
                ],
            };
            for (face, normal, face_mult, reversed, flip_u, name) in faces {
                let face_index = usize::from(name == "south" || name == "east");
                // A coalesced unit splits the face at its material runs; a
                // plain wall emits the whole slice under its authored key.
                let runs = unit.runs_between(slice.start, slice.end);
                if runs.is_empty() {
                    let key = face_key(name);
                    add_wall_length_face(
                        &mut scratch,
                        axis,
                        l0,
                        l1,
                        face,
                        normal,
                        slice_bottom,
                        visible_top,
                        scale_color(key, face_mult, bot_grad),
                        scale_color(key, face_mult, top_grad),
                        reversed,
                        flip_u,
                        lighting,
                        materials.tile_metres(key),
                    );
                    flush_wall_run(&mut buckets, &scratch, &mut wall_cursor, key);
                } else {
                    for run in runs {
                        let (run_start, run_end) = match axis {
                            WallAxis::X => (origin_x + run.start, origin_x + run.end),
                            WallAxis::Z => (origin_z + run.start, origin_z + run.end),
                        };
                        let key = run.faces[face_index];
                        add_wall_length_face(
                            &mut scratch,
                            axis,
                            run_start,
                            run_end,
                            face,
                            normal,
                            slice_bottom,
                            visible_top,
                            scale_color(key, face_mult, bot_grad),
                            scale_color(key, face_mult, top_grad),
                            reversed,
                            flip_u,
                            lighting,
                            materials.tile_metres(key),
                        );
                        flush_wall_run(&mut buckets, &scratch, &mut wall_cursor, key);
                    }
                }
            }

            // Top face (normal +Y): half-height walls and window sills. A wall
            // that reaches the ceiling over this span needs none, which is what
            // keeps gable-end walls from growing a flat cap above the slope.
            if slice_top < ceiling_along(slice_mid) - 1e-3 {
                let key = unit
                    .run_at(f32::midpoint(slice.start, slice.end))
                    .map_or(wall_key, |run| run.body);
                let top_col = scale_color(key, 1.00, top_grad);
                let tile = materials.tile_metres(key);
                match axis {
                    WallAxis::X => {
                        let points = [
                            [l0, slice_top, t1],
                            [l1, slice_top, t1],
                            [l1, slice_top, t0],
                            [l0, slice_top, t0],
                        ];
                        let colors = lit_corners(top_col, points, lighting);
                        add_quad(
                            &mut scratch,
                            points[0],
                            colors[0],
                            tiled_uv(l0, t1, tile),
                            points[1],
                            colors[1],
                            tiled_uv(l1, t1, tile),
                            points[2],
                            colors[2],
                            tiled_uv(l1, t0, tile),
                            points[3],
                            colors[3],
                            tiled_uv(l0, t0, tile),
                        );
                    }
                    WallAxis::Z => {
                        let points = [
                            [t1, slice_top, l0],
                            [t1, slice_top, l1],
                            [t0, slice_top, l1],
                            [t0, slice_top, l0],
                        ];
                        let colors = lit_corners(top_col, points, lighting);
                        add_quad(
                            &mut scratch,
                            points[0],
                            colors[0],
                            tiled_uv(l0, t1, tile),
                            points[1],
                            colors[1],
                            tiled_uv(l1, t1, tile),
                            points[2],
                            colors[2],
                            tiled_uv(l1, t0, tile),
                            points[3],
                            colors[3],
                            tiled_uv(l0, t0, tile),
                        );
                    }
                }
                flush_wall_run(&mut buckets, &scratch, &mut wall_cursor, key);
            }

            // Bottom face (normal -Y): visible on raised walls and on door or
            // window headers, wherever the wall's underside is above the floor
            // the player actually stands on.
            if slice_bottom > floor_along(slice_mid) + 1e-3 {
                let key = unit
                    .run_at(f32::midpoint(slice.start, slice.end))
                    .map_or(wall_key, |run| run.body);
                let bot_col = scale_color(key, 0.85, bot_grad);
                let tile = materials.tile_metres(key);
                match axis {
                    WallAxis::X => {
                        let points = [
                            [l0, slice_bottom, t0],
                            [l1, slice_bottom, t0],
                            [l1, slice_bottom, t1],
                            [l0, slice_bottom, t1],
                        ];
                        let colors = lit_corners(bot_col, points, lighting);
                        add_quad(
                            &mut scratch,
                            points[0],
                            colors[0],
                            tiled_uv(l0, t0, tile),
                            points[1],
                            colors[1],
                            tiled_uv(l1, t0, tile),
                            points[2],
                            colors[2],
                            tiled_uv(l1, t1, tile),
                            points[3],
                            colors[3],
                            tiled_uv(l0, t1, tile),
                        );
                    }
                    WallAxis::Z => {
                        let points = [
                            [t0, slice_bottom, l0],
                            [t0, slice_bottom, l1],
                            [t1, slice_bottom, l1],
                            [t1, slice_bottom, l0],
                        ];
                        let colors = lit_corners(bot_col, points, lighting);
                        add_quad(
                            &mut scratch,
                            points[0],
                            colors[0],
                            tiled_uv(l0, t0, tile),
                            points[1],
                            colors[1],
                            tiled_uv(l1, t0, tile),
                            points[2],
                            colors[2],
                            tiled_uv(l1, t1, tile),
                            points[3],
                            colors[3],
                            tiled_uv(l0, t1, tile),
                        );
                    }
                }
                flush_wall_run(&mut buckets, &scratch, &mut wall_cursor, key);
            }
        }

        // Cross-section faces: the wall's two ends (nothing is solid outside
        // the wall) and the reveals where the solid Y profile changes at a
        // slice boundary. The exposed range is the symmetric difference
        // between the solid intervals on the left and right of the boundary.
        let mut boundaries: Vec<f32> = Vec::with_capacity(slices.len() * 2 + 2);
        boundaries.push(0.0);
        boundaries.push(wall.length());
        for slice in &slices {
            boundaries.push(slice.start);
            boundaries.push(slice.end);
        }
        boundaries.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        boundaries.dedup_by(|a, b| (*a - *b).abs() <= 1e-3);

        for position in boundaries {
            let left: Vec<(f32, f32)> = slices
                .iter()
                .filter(|s| (s.end - position).abs() <= 1e-3)
                .map(|s| (s.bottom, s.top))
                .collect();
            let right: Vec<(f32, f32)> = slices
                .iter()
                .filter(|s| (s.start - position).abs() <= 1e-3)
                .map(|s| (s.bottom, s.top))
                .collect();

            let at_start = position <= 1e-3;
            let at_end = (position - wall.length()).abs() <= 1e-3;
            for (bottom, top) in interval_symmetric_difference(&left, &right) {
                // A wall end under a gable stops at the ceiling, so its end cap
                // follows the triangle instead of rising to the ridge.
                let top = top.min(ceiling_along(position));
                if top <= bottom + 1e-3 {
                    continue;
                }
                // Wall ends keep the directional face shading; internal
                // reveals use the darker jamb/head colours.
                let mult = if at_start {
                    match axis {
                        WallAxis::X => WALL_FACE_WEST_MULT,
                        WallAxis::Z => WALL_FACE_NORTH_MULT,
                    }
                } else if at_end {
                    match axis {
                        WallAxis::X => WALL_FACE_EAST_MULT,
                        WallAxis::Z => WALL_FACE_SOUTH_MULT,
                    }
                } else if bottom <= wall_base + 1e-3 {
                    jamb_mult
                } else {
                    head_mult
                };
                let at = match axis {
                    WallAxis::X => origin_x + position,
                    WallAxis::Z => origin_z + position,
                };
                // Light the reveal from both sides of the wall: each edge of
                // the cross quad sits on a wall face, inside whichever room
                // looks at that face. This carries doorway light through the
                // jamb instead of dropping to ambient in the wall cavity.
                let (bottom_t0, bottom_t1, top_t0, top_t1) = match axis {
                    WallAxis::X => (
                        lighting.sample(at, bottom, t0),
                        lighting.sample(at, bottom, t1),
                        lighting.sample(at, top, t0),
                        lighting.sample(at, top, t1),
                    ),
                    WallAxis::Z => (
                        lighting.sample(t0, bottom, at),
                        lighting.sample(t1, bottom, at),
                        lighting.sample(t0, top, at),
                        lighting.sample(t1, top, at),
                    ),
                };
                // Corner order: low thickness, high thickness, then the same at
                // the top (see add_wall_cross_quad).
                let key = unit.run_at(position).map_or(wall_key, |run| run.body);
                let corners = [
                    shade(scale_color(key, mult, bot_grad), bottom_t0),
                    shade(scale_color(key, mult, bot_grad), bottom_t1),
                    shade(scale_color(key, mult, top_grad), top_t1),
                    shade(scale_color(key, mult, top_grad), top_t0),
                ];
                // A reveal is exposed to whichever side has no material over
                // this Y range: that is the side it faces. Wall ends follow the
                // same rule (nothing is solid outside the wall).
                let left_covers = left
                    .iter()
                    .any(|(low, high)| *low <= bottom + 1e-3 && *high >= top - 1e-3);
                wall_cursor = scratch.len();
                add_wall_cross_quad(
                    &mut scratch,
                    axis,
                    at,
                    (t0, t1),
                    bottom,
                    top,
                    left_covers,
                    corners,
                    materials.tile_metres(key),
                );
                flush_wall_run(&mut buckets, &scratch, &mut wall_cursor, key);
            }
        }
        flush_wall_run(&mut buckets, &scratch, &mut wall_cursor, wall_key);
    }

    // 4. Light fixtures batch. A fixture's family comes from its catalog id
    //    (see `lighting::fixture_profile`): the office panel hangs just below
    //    its room's ceiling, a round downlight sits in the same plane, and a
    //    wall luminaire mounts at its authored world height. The fixture's
    //    visible glow is the same authored colour the bake emits into the room,
    //    scaled by the intensity response, so the two can never silently
    //    diverge.
    for light in &level.ceiling_lights {
        if !light.x.is_finite() || !light.z.is_finite() {
            continue;
        }
        scratch.clear();
        let profile = crate::lighting::fixture_profile(&light.fixture);
        let (half_w, half_d) =
            crate::lighting::fixture_half_extents_for(profile.kind, light.rotation_degrees);

        let intensity = light.intensity();
        // An explicitly zero-output fixture is off: its panel must not glow
        // with the authored colour while emitting no illumination.
        let output = if intensity <= 0.0 {
            0.0
        } else {
            0.40f32
                .mul_add(intensity.clamp(0.0, 2.0), 0.60)
                .clamp(0.0, 1.0)
        };
        let color = light.emitted_color();
        let fixture_glow = [color.r * output, color.g * output, color.b * output];

        match profile.kind {
            crate::lighting::FixtureKind::FluorescentPanel => {
                // The panel hangs below the lowest ceiling point it covers, so a
                // gable fixture near the eave and one near the ridge both clear
                // the slope.
                let y = lighting.fixture_panel_y(light.x, light.z, half_w, half_d);
                let x0 = light.x - half_w;
                let x1 = light.x + half_w;
                let z0 = light.z - half_d;
                let z1 = light.z + half_d;
                add_panel_fixture(&mut scratch, x0, x1, z0, z1, y, fixture_glow);
            }
            crate::lighting::FixtureKind::RoundRecessed => {
                let y = lighting.fixture_panel_y(light.x, light.z, half_w, half_d);
                add_round_fixture(
                    &mut scratch,
                    light.x,
                    light.z,
                    y,
                    profile.half_width,
                    fixture_glow,
                );
            }
            crate::lighting::FixtureKind::WallSconce => {
                let y = lighting.wall_fixture_y(light.x, light.z, light.y);
                add_wall_fixture(
                    &mut scratch,
                    light.x,
                    y,
                    light.z,
                    light.rotation_degrees,
                    fixture_glow,
                );
            }
        }
        buckets.add_quads(SurfaceKey::bare(SurfaceKind::Light), &scratch);
    }

    // 5. Props batch: placeholder boxes for every prop whose real model is
    //    unavailable (unknown catalogue entry, missing file, malformed GLB).
    //    Real prop geometry is added by `build_level_geometry_with_assets`,
    //    which batches instances per model and draws them with their own texture.
    for prop in fallback_props {
        let entry = catalog.get(&prop.model);
        let size = prop.resolved_size(entry.size);
        if !prop.x.is_finite()
            || !prop.y.is_finite()
            || !prop.z.is_finite()
            || !prop.rotation_degrees.is_finite()
            || !prop.scale.is_finite()
            || !size.iter().all(|v| v.is_finite() && *v > 0.0)
            || !entry.color.iter().all(|c| c.is_finite())
        {
            continue;
        }
        scratch.clear();
        let base_y = surfaces.floor_y_at(prop.x, prop.z).unwrap_or(0.0);
        add_prop_box(&mut scratch, prop, size, entry.color, base_y, lighting);
        // Whole run, not per quad: a placeholder box straddling a cell boundary
        // must stay one draw range, like the real prop geometry it stands in for.
        buckets.add_run(SurfaceKey::bare(SurfaceKind::PropFallback), &scratch);
    }

    // 6. Decals batch: local surface markings (signs, floor arrows, warning
    //    marks). They are static geometry like everything else, bucketed per
    //    cell, but drawn in their own pass so the depth bias is explicit. An
    //    unknown material is skipped, which is how a level referencing a decal
    //    sheet from a newer build still loads. The key's material index is the
    //    decal's sheet: a generated atlas slot or an external PNG sheet.
    for decal in &level.decals {
        let Some(sheet) = decal_sheet_index(level, catalog.assets(), &decal.material) else {
            continue;
        };
        let uv = if sheet < DECAL_EXTERNAL_BASE {
            decal_uv_rect(sheet)
        } else {
            decal_uv_rect_full()
        };
        scratch.clear();
        add_decal_quad(&mut scratch, decal, &surfaces, lighting, uv);
        if !scratch.is_empty() {
            buckets.add_run(
                SurfaceKey::new(SurfaceKind::Decal, sheet as MaterialIndex),
                &scratch,
            );
        }
    }

    finish_indexed_mesh(buckets)
}

/// Concatenates spatially bucketed geometry into one vertex buffer plus its
/// cullable ranges.
///
/// Buckets arrive group-major (all floors, then all ceilings, ...) and, inside a
/// group, cell-major with cell keys sorted, so the result is byte-for-byte
/// reproducible: the same level always produces the same buffer. Materials stay
/// contiguous, which is what keeps the per-material aggregate spans in
/// [`LevelMesh::batches`] meaningful.
fn finish_indexed_mesh(mut buckets: crate::spatial::SpatialBuckets<SurfaceKey>) -> LevelMesh {
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
        let slot = &mut spans[key.kind as usize];
        *slot = Some(match *slot {
            None => (virtual_index, virtual_index + index_len),
            Some((low, high)) => (low.min(virtual_index), high.max(virtual_index + index_len)),
        });
        virtual_index += index_len;
        vertex_count += range.vertices.len();
        index_count += range.indices.len();
        ranges.push(LevelMeshRange {
            key,
            vertices: range.vertices,
            indices: range.indices,
            bounds: range.bounds,
        });
    }
    drop(buckets);

    let span = |slot: Option<(i32, i32)>| match slot {
        Some((start, end)) => BatchRange {
            start,
            count: end - start,
        },
        None => BatchRange::default(),
    };
    batches.floor_batch = span(spans[SurfaceKind::Floor as usize]);
    batches.ceiling_batch = span(spans[SurfaceKind::Ceiling as usize]);
    batches.wall_batch = span(spans[SurfaceKind::Wall as usize]);
    batches.light_batch = span(spans[SurfaceKind::Light as usize]);
    batches.prop_batch = span(spans[SurfaceKind::PropFallback as usize]);
    batches.decal_batch = span(spans[SurfaceKind::Decal as usize]);

    LevelMesh {
        ranges,
        batches,
        vertex_count,
        index_count,
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
struct MeshPacker {
    chunks: Vec<MeshChunk>,
}

/// One vertex/index pair, small enough for 16-bit indices.
///
/// Vertices stay in the exact build representation here; the GPU layout is
/// chosen at upload time by [`VertexLayout`].
#[derive(Default)]
struct MeshChunk {
    vertices: Vec<Vertex>,
    indices: Vec<u16>,
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
struct PackedRange {
    chunk: usize,
    index_start: i32,
    index_count: i32,
    vertex_start: i32,
    vertex_count: i32,
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
    fn push(&mut self, vertices: &[Vertex], indices: &[u16]) -> Vec<PackedRange> {
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
            let chunk_index = self.chunks.len() - 1;
            let chunk = &mut self.chunks[chunk_index];
            let index_start = i32::try_from(chunk.indices.len()).unwrap_or(i32::MAX);
            let vertex_start = i32::try_from(chunk.vertices.len()).unwrap_or(i32::MAX);

            while cursor < indices.len() {
                let source = indices[cursor] as usize;
                let Some(vertex) = vertices.get(source) else {
                    // Malformed index: skip it rather than fabricating geometry.
                    cursor += 1;
                    continue;
                };
                if remap[source] == u16::MAX {
                    if chunk.vertices.len() >= limit {
                        break;
                    }
                    remap[source] = u16::try_from(chunk.vertices.len()).unwrap_or(u16::MAX);
                    chunk.vertices.push(*vertex);
                }
                chunk.indices.push(remap[source]);
                cursor += 1;
            }

            let index_count = i32::try_from(chunk.indices.len()).unwrap_or(i32::MAX) - index_start;
            if index_count > 0 {
                placements.push(PackedRange {
                    chunk: chunk_index,
                    index_start,
                    index_count,
                    vertex_start,
                    vertex_count: i32::try_from(chunk.vertices.len()).unwrap_or(i32::MAX)
                        - vertex_start,
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
    fn push_unindexed(&mut self, vertices: &[Vertex], indices: &[u16]) -> Vec<PackedRange> {
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
    fn vertex_total(&self) -> usize {
        self.chunks.iter().map(|chunk| chunk.vertices.len()).sum()
    }

    /// Total indices across every chunk.
    fn index_total(&self) -> usize {
        self.chunks.iter().map(|chunk| chunk.indices.len()).sum()
    }
}

/// Instanced prop geometry for one distinct prop model in a level.
///
/// Every placed instance of the same model is transformed on the CPU at level
/// load time and appended here, so the renderer binds one buffer and one
/// texture per model and issues one draw call for all of its instances. The
/// decoded model itself is parsed once and shared through
/// [`crate::props::PropAssets`].
#[derive(Clone, Debug)]
pub struct PropMeshBatch {
    /// Catalogue model path, e.g. `models/chair.glb`.
    pub model: String,
    /// Diffuse texture shared by every instance in this batch.
    pub texture: crate::loader::RawImage,
    /// Pre-transformed vertices, referenced by `indices`.
    ///
    /// The GLB already stores its mesh indexed, so an instance is a vertex
    /// offset and the model's own index list; nothing is expanded. That keeps
    /// the GPU shading ~30% fewer vertices per instance than the flat triangle
    /// list this used to build.
    pub vertices: Vec<Vertex>,
    /// `GL_UNSIGNED_SHORT` indices into `vertices`, offset per instance.
    pub indices: Vec<u16>,
    /// World-space bounds of every instance in this batch, used for frustum
    /// culling. One batch covers one model inside one spatial cell, so a prop
    /// field spread over a level becomes several cullable ranges of the same
    /// model instead of one range spanning the whole level.
    pub bounds: crate::spatial::Aabb,
}

/// Builds the level mesh with real prop geometry where possible, plus one
/// batched draw per distinct prop model.
///
/// Props whose model is missing, malformed or simply absent from the catalogue
/// still emit their catalogue-sized placeholder box into
/// `LevelMesh::batches.prop_batch`, so a broken asset degrades visibly instead
/// of vanishing, and never crashes or loops (failures are cached by
/// [`crate::props::PropAssets`]).
pub fn build_level_geometry_with_assets(
    level: &LevelDef,
    catalog: &crate::loader::PropCatalog,
    assets: &mut crate::props::PropAssets,
) -> (LevelMesh, Vec<PropMeshBatch>) {
    let materials = logical_materials(level);
    let (mesh, batches, _lighting) = build_level_geometry_with_assets_and_lighting_and_materials(
        level, catalog, assets, &materials,
    );
    (mesh, batches)
}

/// [`build_level_geometry_with_assets`], also returning the baked lighting that
/// was folded into the vertex colours.
///
/// The lighting is baked exactly once here, at level load, and passed to both
/// the world geometry and the prop instancing so the whole level shares one
/// consistent set of room baselines, fixture pools and opening blends.
pub fn build_level_geometry_with_assets_and_lighting(
    level: &LevelDef,
    catalog: &crate::loader::PropCatalog,
    assets: &mut crate::props::PropAssets,
) -> (LevelMesh, Vec<PropMeshBatch>, LevelLighting) {
    let materials = logical_materials(level);
    build_level_geometry_with_assets_and_lighting_and_materials(level, catalog, assets, &materials)
}

/// [`build_level_geometry_with_assets_and_lighting`] with an explicitly
/// resolved material table.
///
/// The renderer uses this with the level's loaded table (including pack
/// materials and decoded images); tests and the lighting audit use the
/// catalog-only wrapper above.
pub fn build_level_geometry_with_assets_and_lighting_and_materials(
    level: &LevelDef,
    catalog: &crate::loader::PropCatalog,
    assets: &mut crate::props::PropAssets,
    materials: &MaterialTable,
) -> (LevelMesh, Vec<PropMeshBatch>, LevelLighting) {
    let (mesh, batches, lighting, _) =
        build_level_geometry_timed(level, catalog, assets, materials);
    (mesh, batches, lighting)
}

/// Stage-by-stage timings for one level build, in milliseconds.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BuildTimings {
    pub lighting_millis: f64,
    pub props_millis: f64,
    pub surfaces_millis: f64,
}

/// [`build_level_geometry_with_assets_and_lighting`], also reporting how the
/// build time splits between the lighting bake, prop instancing and static
/// surface emission.
///
/// Kept separate from the untimed entry point so the timing does not change what
/// the normal load path does.
pub fn build_level_geometry_timed(
    level: &LevelDef,
    catalog: &crate::loader::PropCatalog,
    assets: &mut crate::props::PropAssets,
    materials: &MaterialTable,
) -> (LevelMesh, Vec<PropMeshBatch>, LevelLighting, BuildTimings) {
    let started = std::time::Instant::now();
    let lighting = LevelLighting::bake(level);
    let lighting_millis = started.elapsed().as_secs_f64() * 1000.0;

    let surfaces = LevelSurfaces::new(level);
    let started = std::time::Instant::now();
    let (batches, fallbacks) = resolve_prop_instances(level, catalog, assets, &lighting, &surfaces);
    let props_millis = started.elapsed().as_secs_f64() * 1000.0;

    let started = std::time::Instant::now();
    let mesh = build_level_geometry_mesh(level, catalog, &fallbacks, &lighting, materials);
    let surfaces_millis = started.elapsed().as_secs_f64() * 1000.0;

    (
        mesh,
        batches,
        lighting,
        BuildTimings {
            lighting_millis,
            props_millis,
            surfaces_millis,
        },
    )
}

/// The shipped catalog, loaded once per process for geometry-only callers.
fn shipped_asset_catalog() -> &'static crate::assets::AssetCatalog {
    static CATALOG: std::sync::OnceLock<crate::assets::AssetCatalog> = std::sync::OnceLock::new();
    CATALOG.get_or_init(crate::assets::AssetCatalog::load_default)
}

/// The logical material table for a level, resolved through the shipped
/// catalog with no image decoding.
///
/// Geometry only needs each material's index, tiling and tint, so the tests and
/// the lighting audit can build meshes without touching the filesystem.
#[must_use]
pub fn logical_materials(level: &LevelDef) -> MaterialTable {
    MaterialTable::logical(level, shipped_asset_catalog(), None)
}

/// Builds level geometry using only built-in prop fallbacks.
///
/// Callers that can resolve the prop catalog should prefer
/// [`build_level_geometry_with_catalog`].
#[must_use]
pub fn build_level_geometry(level: &LevelDef) -> LevelMesh {
    build_level_geometry_with_catalog(level, &crate::loader::PropCatalog::builtin())
}

/// Builds level geometry using the shipped catalog's logical materials.
#[must_use]
pub fn build_level_geometry_with_materials(
    level: &LevelDef,
    materials: &MaterialTable,
) -> LevelMesh {
    build_level_geometry_with_catalog_and_materials(
        level,
        &crate::loader::PropCatalog::builtin(),
        materials,
    )
}

/// Builds level geometry, drawing every prop as its catalogue placeholder box
/// (no GLB assets are read). Used by tests and by the asset-less fallback path.
#[must_use]
pub fn build_level_geometry_with_catalog(
    level: &LevelDef,
    catalog: &crate::loader::PropCatalog,
) -> LevelMesh {
    let materials = logical_materials(level);
    build_level_geometry_with_catalog_and_materials(level, catalog, &materials)
}

/// [`build_level_geometry_with_catalog`] with an explicitly resolved material
/// table.
#[must_use]
pub fn build_level_geometry_with_catalog_and_materials(
    level: &LevelDef,
    catalog: &crate::loader::PropCatalog,
    materials: &MaterialTable,
) -> LevelMesh {
    let lighting = LevelLighting::bake(level);
    let fallbacks: Vec<&PropDef> = level.props.iter().collect();
    build_level_geometry_mesh(level, catalog, &fallbacks, &lighting, materials)
}

/// Resolves every placed prop into either a batched real mesh or a fallback box,
/// sharing one decoded model (and one texture) per distinct model path.
///
/// Baked lighting is sampled per transformed vertex in world space, so a prop
/// standing on a crate or lying on a bed is lit at its real height and still
/// contributes to the same shared per-model batch (one draw call per model).
fn resolve_prop_instances<'a>(
    level: &'a LevelDef,
    catalog: &crate::loader::PropCatalog,
    assets: &mut crate::props::PropAssets,
    lighting: &LevelLighting,
    surfaces: &LevelSurfaces<'_>,
) -> (Vec<PropMeshBatch>, Vec<&'a PropDef>) {
    use std::collections::{HashMap, HashSet};

    let grid = spatial_cell_grid(level);
    let mut batches: Vec<PropMeshBatch> = Vec::new();
    // Keyed by (model, cell): one drawable range per model per spatial cell.
    let mut index_by_batch: HashMap<(String, crate::spatial::CellKey), usize> = HashMap::new();
    let mut models_seen: HashSet<String> = HashSet::new();
    let mut fallbacks: Vec<&'a PropDef> = Vec::new();
    let mut busy_vertices = 0usize;

    for prop in &level.props {
        let entry = catalog.get(&prop.model);
        let Some(model_path) = entry.model.clone() else {
            fallbacks.push(prop);
            continue;
        };
        if busy_vertices >= crate::level::MAX_LEVEL_PROP_VERTICES {
            fallbacks.push(prop);
            continue;
        }
        let asset = match assets.resolve(&model_path) {
            Ok(asset) => asset,
            Err(error) => {
                assets.report_failure(&model_path, &error);
                fallbacks.push(prop);
                continue;
            }
        };
        if !models_seen.contains(&model_path)
            && models_seen.len() >= crate::level::MAX_LEVEL_PROP_MODELS
        {
            fallbacks.push(prop);
            continue;
        }

        // A prop's authored `y` is an offset above the local walkable floor, so
        // a chair in an elevated room or a recessed region lands on the surface
        // it was placed against.
        let base_y = surfaces.floor_y_at(prop.x, prop.z).unwrap_or(0.0);
        let model = prop_instance_matrix(prop, base_y);
        // Cull by the instance's real world-space extent, not by the cell it
        // happens to be centred in: a chair on a cell boundary must not be
        // culled while a sliver of it is still on screen.
        let instance_bounds = match asset.model.bounds() {
            Some((low, high)) => transform_bounds(
                &crate::spatial::Aabb {
                    min: low,
                    max: high,
                },
                &model,
            ),
            None => crate::spatial::Aabb::from_point([prop.x, base_y + prop.y, prop.z]),
        };
        let cell = grid.cell_of(instance_bounds.centre());

        // One batch holds every instance of a model inside one spatial cell, but
        // never more than a 16-bit index can address: `PropMeshBatch::indices`
        // are `GL_UNSIGNED_SHORT` offsets into the batch's own vertex list, so a
        // cell holding hundreds of instances has to become several batches.
        let key = (model_path.clone(), cell);
        let needs_new_batch = index_by_batch.get(&key).is_none_or(|index| {
            batches[*index].vertices.len() + asset.model.vertices.len()
                > crate::spatial::MAX_INDEX_VERTICES
        });
        if needs_new_batch {
            models_seen.insert(model_path.clone());
            batches.push(PropMeshBatch {
                model: model_path.clone(),
                texture: asset.model.texture.clone(),
                vertices: Vec::with_capacity(asset.model.vertices.len()),
                indices: Vec::with_capacity(asset.model.indices.len()),
                bounds: crate::spatial::Aabb::EMPTY,
            });
            index_by_batch.insert(key, batches.len() - 1);
        }
        let batch_index = index_by_batch[&(model_path.clone(), cell)];
        let batch = &mut batches[batch_index];
        batch.bounds = batch.bounds.union(&instance_bounds);
        // One instance is the model's own index list shifted by the vertex
        // offset this instance was appended at. Nothing is expanded into a flat
        // triangle list, and each distinct model vertex is transformed and
        // lit exactly once per placement.
        let base = batch.vertices.len();
        for vertex in &asset.model.vertices {
            let position = model.transform_point3(glam::Vec3::new(
                vertex.pos[0],
                vertex.pos[1],
                vertex.pos[2],
            ));
            // Bake the environment into the instance's colour: the same model in
            // a dark corner and under a fixture still shares one batch, but is
            // no longer uniformly lit.
            let light = lighting.sample(position.x, position.y, position.z);
            batch.vertices.push(Vertex {
                pos: [position.x, position.y, position.z],
                color: [
                    vertex.color[0] * light.r,
                    vertex.color[1] * light.g,
                    vertex.color[2] * light.b,
                    vertex.color[3],
                ],
                uv: vertex.uv,
            });
        }
        for index in &asset.model.indices {
            batch
                .indices
                .push(u16::try_from(base).unwrap_or(u16::MAX) + *index);
        }
        busy_vertices += asset.model.vertices.len();
    }

    (batches, fallbacks)
}

/// World-space bounds of a local-space box placed by `transform`.
///
/// Only the eight corners are transformed: the result is the AABB of the
/// rotated box, which is conservative (never smaller than the real geometry),
/// which is exactly what a culling test needs.
fn transform_bounds(local: &crate::spatial::Aabb, transform: &glam::Mat4) -> crate::spatial::Aabb {
    let mut bounds = crate::spatial::Aabb::EMPTY;
    for x in [local.min[0], local.max[0]] {
        for y in [local.min[1], local.max[1]] {
            for z in [local.min[2], local.max[2]] {
                let point = transform.transform_point3(glam::Vec3::new(x, y, z));
                bounds.expand([point.x, point.y, point.z]);
            }
        }
    }
    bounds
}

/// Instance transform for a placed prop: translate, rotate about Y and scale.
///
/// `base_y` is the world Y of the walkable floor at the prop's `(x, z)`; the
/// authored `prop.y` is an offset above it. This is exactly the transform the
/// placeholder boxes use (see [`add_prop_box`]), so a prop keeps its position,
/// orientation and vertical offset when its real model replaces the box. Model
/// space is metres with the origin at the floor-contact centre (see
/// `assets/README.md`).
#[must_use]
pub fn prop_instance_matrix(prop: &PropDef, base_y: f32) -> glam::Mat4 {
    let rotation = glam::Mat4::from_rotation_y(prop.rotation_degrees.to_radians());
    let scale = glam::Mat4::from_scale(glam::Vec3::splat(prop.scale));
    glam::Mat4::from_translation(glam::Vec3::new(prop.x, base_y + prop.y, prop.z))
        * rotation
        * scale
}

/// Applies min/mag filtering for a repeating, mipmapped texture.
///
/// Nearest filtering keeps mipmaps (`NEAREST_MIPMAP_NEAREST`) so distant
/// minification still anti-aliases instead of shimmering.
unsafe fn set_repeat_filter(gl: &glow::Context, linear: bool) {
    let (min_filter, mag_filter) = if linear {
        (glow::LINEAR_MIPMAP_LINEAR, glow::LINEAR)
    } else {
        (glow::NEAREST_MIPMAP_NEAREST, glow::NEAREST)
    };
    unsafe {
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MIN_FILTER,
            min_filter.cast_signed(),
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MAG_FILTER,
            mag_filter.cast_signed(),
        );
    }
}

unsafe fn create_texture_2d(
    gl: &glow::Context,
    width: i32,
    height: i32,
    pixels: &[u8],
    repeat: bool,
    linear: bool,
) -> Result<glow::Texture, String> {
    unsafe {
        let texture = gl.create_texture()?;
        gl.bind_texture(glow::TEXTURE_2D, Some(texture));

        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA.cast_signed(),
            width,
            height,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(Some(pixels)),
        );

        let wrap_mode = if repeat {
            glow::REPEAT.cast_signed()
        } else {
            glow::CLAMP_TO_EDGE.cast_signed()
        };
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, wrap_mode);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, wrap_mode);

        if repeat {
            set_repeat_filter(gl, linear);
            gl.generate_mipmap(glow::TEXTURE_2D);
        } else {
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MIN_FILTER,
                glow::NEAREST.cast_signed(),
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MAG_FILTER,
                glow::NEAREST.cast_signed(),
            );
        }

        gl.bind_texture(glow::TEXTURE_2D, None);
        Ok(texture)
    }
}

unsafe fn create_shader(
    gl: &glow::Context,
    shader_type: u32,
    source: &str,
) -> Result<glow::Shader, String> {
    unsafe {
        let shader = gl.create_shader(shader_type)?;
        gl.shader_source(shader, source);
        gl.compile_shader(shader);
        if !gl.get_shader_compile_status(shader) {
            let log = gl.get_shader_info_log(shader);
            gl.delete_shader(shader);
            return Err(format!("Shader compile error: {log}"));
        }
        Ok(shader)
    }
}

unsafe fn create_program(
    gl: &glow::Context,
    vert_src: &str,
    frag_src: &str,
) -> Result<glow::Program, String> {
    unsafe {
        let vs = create_shader(gl, glow::VERTEX_SHADER, vert_src)?;
        let fs = create_shader(gl, glow::FRAGMENT_SHADER, frag_src)?;

        let program = gl.create_program()?;
        // Both programs share the scene attribute layout, so bind the indices
        // explicitly before linking: the decal pass switches programs mid-frame
        // and must not need to re-point the vertex attributes.
        gl.bind_attrib_location(program, SCENE_ATTRIB_POS, "a_pos");
        gl.bind_attrib_location(program, SCENE_ATTRIB_COLOR, "a_color");
        gl.bind_attrib_location(program, SCENE_ATTRIB_UV, "a_uv");
        gl.attach_shader(program, vs);
        gl.attach_shader(program, fs);
        gl.link_program(program);

        if !gl.get_program_link_status(program) {
            let log = gl.get_program_info_log(program);
            gl.delete_program(program);
            gl.delete_shader(vs);
            gl.delete_shader(fs);
            return Err(format!("Program link error: {log}"));
        }

        gl.delete_shader(vs);
        gl.delete_shader(fs);
        Ok(program)
    }
}

/// One drawable batch of placed props: all instances of a single model inside a
/// single spatial cell, sharing one texture, drawn as a contiguous vertex range
/// of the prop buffer.
#[derive(Clone, Copy, Debug)]
struct PropDraw {
    texture: glow::Texture,
    /// Which prop buffer pair this range lives in (see `MeshPacker`).
    chunk: usize,
    /// Range in that chunk's index buffer.
    index_start: i32,
    index_count: i32,
    /// Distinct vertices the range reads, for the debug counters.
    vertex_count: i32,
    bounds: crate::spatial::Aabb,
}

/// Cost and shape of the last level build, split by stage so a hardware run can
/// tell an expensive geometry bake from an expensive prop instancing pass.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LevelBuildStats {
    /// Distinct vertices in the static level mesh (floors, ceilings, walls, fixtures).
    pub static_vertices: usize,
    /// Indices in the static level mesh.
    pub static_indices: usize,
    /// Distinct vertices in the prop mesh (every placed instance).
    pub prop_vertices: usize,
    /// Indices in the prop mesh.
    pub prop_indices: usize,
    /// Draw calls the level needs for real prop geometry.
    pub prop_draws: usize,
    /// Cullable static batches the level was partitioned into.
    pub static_batches: usize,
    /// Static GPU buffer pairs (a level past 65 536 vertices needs several).
    pub static_chunks: usize,
    /// Prop GPU buffer pairs.
    pub prop_chunks: usize,
    /// Bytes resident in vertex buffers.
    pub vbo_bytes: usize,
    /// Bytes resident in index buffers.
    pub index_bytes: usize,
    /// Wall-clock cost of the last level build (geometry + lighting bake), in ms.
    pub build_millis: f64,
    /// Time spent baking the static lighting, in ms.
    pub lighting_millis: f64,
    /// Time spent resolving, transforming and lit-shading every placed prop, in ms.
    pub props_millis: f64,
    /// Time spent emitting and spatially bucketing the static surfaces, in ms.
    pub surfaces_millis: f64,
    /// Summary of the baked static lighting.
    pub lighting: crate::lighting::LightingSummary,
}

/// Geometry counters for the frame that was most recently submitted.
///
/// Filled in by [`Renderer::render_scene`] and read by the debug-only benchmark
/// harness, so the numbers describe the actual draw path rather than a
/// reconstruction of it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RenderStats {
    /// Vertices the level holds in total (static + props), visible or not.
    pub total_vertices: usize,
    /// Vertices inside the batches that were actually submitted this frame.
    pub visible_vertices: usize,
    /// Vertices belonging to batches the frustum rejected this frame.
    pub culled_vertices: usize,
    /// Render batches the level is split into.
    pub total_batches: usize,
    /// Batches that survived culling and were submitted.
    pub visible_batches: usize,
    /// `glDrawArrays`/`glDrawElements` calls issued for the scene.
    pub draw_calls: usize,
    /// Bytes resident in static vertex buffers (level + props).
    pub vbo_bytes: usize,
    /// Bytes resident in element (index) buffers.
    pub index_bytes: usize,
}

/// GPU state for the decal pass: a second program (the world shader plus an
/// alpha cut-out), the shared generated decal sheet, and the uniforms the pass
/// has to set when it starts.
struct DecalPass {
    program: glow::Program,
    texture: glow::Texture,
    /// External (PNG-backed) decal sheets for the current level, in the order
    /// [`decal_external_sheet_ids`] reports them. The generated atlas keeps
    /// slot 0 and the external sheets take [`DECAL_EXTERNAL_BASE`] onwards.
    external: Vec<glow::Texture>,
    u_mvp_loc: Option<glow::UniformLocation>,
    u_texture_loc: Option<glow::UniformLocation>,
    u_alpha_cutoff_loc: Option<glow::UniformLocation>,
}

/// Manages OpenGL ES 2.0-compatible accelerated rendering context, textures, and scene/UI drawing.
pub struct Renderer {
    _gl_context: sdl2::video::GLContext,
    gl: glow::Context,
    program: glow::Program,
    /// Static-geometry buffer pairs: `(vbo, ibo)`, each addressable with
    /// 16-bit indices. A small level needs one; a large one needs several.
    level_buffers: Vec<(glow::Buffer, glow::Buffer)>,
    ui_vbo: glow::Buffer,
    /// Prop buffer pairs, split the same way as `level_buffers`.
    prop_buffers: Vec<(glow::Buffer, glow::Buffer)>,
    /// Cullable static ranges for the current level, one per (material, cell).
    static_batches: Vec<StaticBatch>,
    /// Spatial grid the current level was partitioned with, for the debug log.
    spatial_grid: crate::spatial::CellGrid,
    /// Whether frustum culling is applied. Only the benchmark harness turns it
    /// off, to measure what culling is worth on real hardware.
    culling_enabled: bool,
    /// Scratch buffer for packing UI vertices each frame (never grows per frame
    /// beyond the UI's own vertex count).
    ui_scratch: Vec<PackedVertex>,
    /// Vertex layout uploaded for the scene and the HUD.
    vertex_layout: VertexLayout,
    /// Whether geometry reaches the GPU as an indexed triangle list.
    indexing_enabled: bool,
    /// Vertex count of the last HUD upload, in whichever layout was used.
    ui_packed_len: usize,
    /// Catalog used to size and colour placed props. Loaded once at startup.
    prop_catalog: crate::loader::PropCatalog,
    /// Decoded prop models, shared between instances and cached across levels.
    prop_assets: crate::props::PropAssets,
    /// Per-model prop draw ranges for the current level.
    prop_draws: Vec<PropDraw>,
    /// GPU textures for prop models, keyed by catalogue model path so a level
    /// change never re-uploads a texture that is already resident.
    prop_textures: std::collections::HashMap<String, glow::Texture>,
    /// GPU textures for catalog/missing surface textures, keyed by logical
    /// texture key. Decoded images are already cached per session; this cache
    /// keeps their GPU copies across level changes, so a level switch never
    /// re-uploads a built-in texture.
    surface_textures: std::collections::HashMap<String, glow::Texture>,
    /// Decoded decal-sheet PNGs, keyed by their catalog path. Decoded once per
    /// session exactly like surface textures.
    decal_image_cache: crate::materials::TextureCache,
    /// GPU textures for external decal sheets, keyed by their catalog path and
    /// kept across level changes like `surface_textures`.
    decal_sheet_textures: std::collections::HashMap<String, glow::Texture>,
    /// Per-level GPU textures (pack-supplied and diagnostic fallbacks), freed
    /// when the next level is uploaded. `(key, texture)` pairs so a texture
    /// shared by several materials is still deleted exactly once.
    level_textures: Vec<(String, glow::Texture)>,
    /// One GPU texture per entry of the loaded level's material table, indexed
    /// exactly like [`MaterialTable::textures`]. The draw loop binds by the
    /// material index a batch carries.
    material_textures: Vec<glow::Texture>,
    /// Maps a material index (what batches carry) to its texture index (what
    /// `material_textures` is indexed by). Two materials that share one texture
    /// map to the same slot, so the upload is shared.
    material_texture_slots: Vec<u16>,
    /// The untextured fixture/light sheet (white unless a pack supplies one).
    white_texture: glow::Texture,
    font_texture: glow::Texture,
    /// Decal rendering state (program, shared sheet, uniforms).
    decal: DecalPass,
    u_mvp_loc: Option<glow::UniformLocation>,
    u_texture_loc: Option<glow::UniformLocation>,
    a_pos_loc: u32,
    a_color_loc: u32,
    a_uv_loc: u32,
    /// Whether repeating 3D textures use linear (vs nearest) filtering. Wired
    /// to the user-facing `texture_filtering` setting.
    linear_filtering: bool,
    /// Physical framebuffer size currently being rendered to. Updated on resize
    /// and HiDPI/backing-scale changes via [`Renderer::set_drawable_size`].
    drawable_size: DrawableSize,
    /// Cost and shape of the most recently built level.
    level_stats: LevelBuildStats,
    /// Counters for the most recently submitted frame (see [`RenderStats`]).
    render_stats: RenderStats,
}

impl Renderer {
    /// Initializes an accelerated OpenGL context with `VSync`, textures and an
    /// empty level buffer.
    ///
    /// The caller uploads the first level with [`Renderer::set_level`] (or
    /// [`Renderer::rebuild_level_geometry`]); building here as well would bake
    /// and upload the same level twice before the first frame, which is real
    /// cost on the `PocketCHIP`.
    /// # Errors
    ///
    /// Returns a message when the GL context is missing, the shader program does
    /// not link, or a texture, buffer or attribute lookup fails.
    pub fn new(window: &sdl2::video::Window, video: &sdl2::VideoSubsystem) -> Result<Self, String> {
        let gl_attr = video.gl_attr();
        gl_attr.set_double_buffer(true);
        gl_attr.set_depth_size(24);

        gl_attr.set_context_profile(sdl2::video::GLProfile::GLES);
        gl_attr.set_context_version(2, 0);

        let gl_context = if let Ok(ctx) = window.gl_create_context() {
            ctx
        } else {
            gl_attr.set_context_profile(sdl2::video::GLProfile::Compatibility);
            gl_attr.set_context_version(2, 1);
            window.gl_create_context()?
        };

        window.gl_make_current(&gl_context)?;

        // Swap interval is configured by the caller *after* this returns: the
        // request must be issued while a context is current, and the caller owns
        // the user's VSync setting. Forcing VSync on here silently overrode it.

        let gl = unsafe {
            glow::Context::from_loader_function(|proc_name| {
                video.gl_get_proc_address(proc_name).cast()
            })
        };

        let prop_catalog = crate::loader::PropCatalog::load_default();

        let (
            program,
            ui_vbo,
            white_texture,
            font_texture,
            decal,
            u_mvp_loc,
            u_texture_loc,
            a_pos_loc,
            a_color_loc,
            a_uv_loc,
        ) = unsafe {
            gl.enable(glow::DEPTH_TEST);
            gl.depth_func(glow::LEQUAL);
            gl.clear_color(0.08, 0.08, 0.09, 1.0);

            let program = create_program(&gl, VERTEX_SHADER_SRC, FRAGMENT_SHADER_SRC)?;
            let a_pos_loc = gl
                .get_attrib_location(program, "a_pos")
                .ok_or_else(|| "Missing a_pos attribute".to_string())?;
            let a_color_loc = gl
                .get_attrib_location(program, "a_color")
                .ok_or_else(|| "Missing a_color attribute".to_string())?;
            let a_uv_loc = gl
                .get_attrib_location(program, "a_uv")
                .ok_or_else(|| "Missing a_uv attribute".to_string())?;

            let u_mvp_loc = gl.get_uniform_location(program, "u_mvp");
            let u_texture_loc = gl.get_uniform_location(program, "u_texture");

            // The untextured fixture sheet and the UI/decal resources are the
            // only textures this renderer owns up front. Every surface texture
            // is uploaded by `set_level`, once per distinct resolved texture.
            let white_texture =
                create_texture_2d(&gl, 2, 2, &generate_white_texture(), false, false)?;
            let font_texture =
                create_texture_2d(&gl, 128, 64, &generate_font_atlas(), false, false)?;

            // The decal pass: the same vertex stage with an alpha cut-out
            // fragment stage, and the one shared generated decal sheet. The
            // sheet uses repeat mip-mapping (as the world sheets do) so the
            // user's texture filtering applies; its UVs never leave the sheet,
            // so the wrap mode itself cannot show.
            let decal_program = create_program(&gl, VERTEX_SHADER_SRC, DECAL_FRAGMENT_SHADER_SRC)?;
            let decal = DecalPass {
                program: decal_program,
                texture: create_texture_2d(
                    &gl,
                    DECAL_ATLAS_SIZE,
                    DECAL_ATLAS_SIZE,
                    &generate_decal_atlas(),
                    true,
                    true,
                )?,
                external: Vec::new(),
                u_mvp_loc: gl.get_uniform_location(decal_program, "u_mvp"),
                u_texture_loc: gl.get_uniform_location(decal_program, "u_texture"),
                u_alpha_cutoff_loc: gl.get_uniform_location(decal_program, "u_alpha_cutoff"),
            };

            // Level geometry is uploaded by `rebuild_level_geometry` once the
            // renderer (and its prop asset cache) exists.
            let ui_vbo = gl.create_buffer()?;
            gl.bind_buffer(glow::ARRAY_BUFFER, None);

            (
                program,
                ui_vbo,
                white_texture,
                font_texture,
                decal,
                u_mvp_loc,
                u_texture_loc,
                a_pos_loc,
                a_color_loc,
                a_uv_loc,
            )
        };

        let (initial_width, initial_height) = window.drawable_size();

        let renderer = Self {
            _gl_context: gl_context,
            gl,
            program,
            level_buffers: Vec::new(),
            ui_vbo,
            ui_scratch: Vec::new(),
            vertex_layout: VertexLayout::Packed,
            indexing_enabled: true,
            ui_packed_len: 0,
            prop_buffers: Vec::new(),
            static_batches: Vec::new(),
            spatial_grid: crate::spatial::CellGrid::default(),
            culling_enabled: true,
            prop_catalog,
            prop_assets: crate::props::PropAssets::load_default(),
            prop_draws: Vec::new(),
            prop_textures: std::collections::HashMap::new(),
            surface_textures: std::collections::HashMap::new(),
            decal_image_cache: crate::materials::TextureCache::new(),
            decal_sheet_textures: std::collections::HashMap::new(),
            level_textures: Vec::new(),
            material_textures: Vec::new(),
            material_texture_slots: Vec::new(),
            white_texture,
            font_texture,
            decal,
            u_mvp_loc,
            u_texture_loc,
            a_pos_loc,
            a_color_loc,
            a_uv_loc,
            linear_filtering: true,
            drawable_size: DrawableSize::new(initial_width, initial_height),
            level_stats: LevelBuildStats::default(),
            render_stats: RenderStats::default(),
        };
        Ok(renderer)
    }

    /// Records the current physical framebuffer size.
    ///
    /// This renderer draws directly into the default framebuffer, so no offscreen
    /// colour/depth attachments exist to recreate; the viewport and projection are
    /// derived from this size each frame. Returns `true` when the size changed,
    /// which is where any future size-dependent GPU resource would be rebuilt.
    pub fn set_drawable_size(&mut self, size: DrawableSize) -> bool {
        if self.drawable_size == size {
            return false;
        }
        self.drawable_size = size;
        true
    }

    /// Applies the user-facing texture filtering mode to the repeating 3D
    /// textures and to every cached prop texture. UI/atlas textures stay
    /// nearest-filtered to preserve crisp text.
    pub fn set_texture_filtering(&mut self, mode: &str) {
        let linear = mode != "nearest";
        if self.linear_filtering == linear {
            return;
        }
        self.linear_filtering = linear;
        unsafe {
            self.gl
                .bind_texture(glow::TEXTURE_2D, Some(self.decal.texture));
            set_repeat_filter(&self.gl, linear);
            for texture in self
                .decal
                .external
                .iter()
                .chain(self.decal_sheet_textures.values())
            {
                if *texture == self.decal.texture {
                    continue;
                }
                self.gl.bind_texture(glow::TEXTURE_2D, Some(*texture));
                set_repeat_filter(&self.gl, linear);
            }
            for texture in self.surface_textures.values() {
                self.gl.bind_texture(glow::TEXTURE_2D, Some(*texture));
                set_repeat_filter(&self.gl, linear);
            }
            for (_, texture) in &self.level_textures {
                self.gl.bind_texture(glow::TEXTURE_2D, Some(*texture));
                set_repeat_filter(&self.gl, linear);
            }
            for texture in self.prop_textures.values() {
                self.gl.bind_texture(glow::TEXTURE_2D, Some(*texture));
                set_repeat_filter(&self.gl, linear);
            }
            self.gl.bind_texture(glow::TEXTURE_2D, None);
        }
    }

    /// Number of draw calls the current level's props need (one per distinct
    /// model), exposed for the performance overlay and tests.
    pub const fn prop_draw_count(&self) -> usize {
        self.prop_draws.len()
    }

    /// Static-geometry and baked-lighting statistics for the current level.
    pub const fn level_stats(&self) -> LevelBuildStats {
        self.level_stats
    }

    /// Reads back the default framebuffer as a top-down RGBA image.
    ///
    /// Used by the `LIMINAL_CAPTURE` developer/hardware path: it is the only way
    /// to inspect real prop rendering on the `PocketCHIP` (no screenshots over
    /// SSH) and on desktops where the window cannot be captured. Call it after
    /// drawing and before swapping buffers.
    /// # Errors
    ///
    /// Returns a message when the drawable is empty or the GL readback returns a
    /// non-finite or truncated buffer.
    pub fn capture_default_framebuffer(&self) -> Result<crate::loader::RawImage, String> {
        let drawable = self.drawable_size;
        if drawable.is_empty() {
            return Err("drawable has zero size; nothing to capture".into());
        }
        let width = drawable.width as usize;
        let height = drawable.height as usize;
        let mut pixels = vec![0u8; width * height * 4];
        unsafe {
            self.gl.read_pixels(
                0,
                0,
                i32::try_from(width).unwrap_or(i32::MAX),
                i32::try_from(height).unwrap_or(i32::MAX),
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(Some(&mut pixels)),
            );
        }
        // OpenGL returns bottom-up rows; flip into top-down image order.
        let stride = width * 4;
        let mut flipped = vec![0u8; pixels.len()];
        for row in 0..height {
            let source = (height - 1 - row) * stride;
            flipped[row * stride..(row + 1) * stride]
                .copy_from_slice(&pixels[source..source + stride]);
        }
        Ok(crate::loader::RawImage::new(
            drawable.width,
            drawable.height,
            flipped,
        ))
    }

    /// Cached prop asset statistics (models loaded/failed, triangles, texture bytes).
    pub fn prop_asset_stats(&self) -> crate::props::PropAssetStats {
        self.prop_assets.stats()
    }

    unsafe fn upload_texture(
        gl: &glow::Context,
        texture: glow::Texture,
        raw_image: &crate::loader::RawImage,
        repeat: bool,
        linear: bool,
    ) {
        unsafe {
            gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA.cast_signed(),
                i32::try_from(raw_image.width).unwrap_or(i32::MAX),
                i32::try_from(raw_image.height).unwrap_or(i32::MAX),
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(&raw_image.rgba)),
            );

            let wrap_mode = if repeat {
                glow::REPEAT.cast_signed()
            } else {
                glow::CLAMP_TO_EDGE.cast_signed()
            };
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, wrap_mode);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, wrap_mode);

            if repeat {
                set_repeat_filter(gl, linear);
                gl.generate_mipmap(glow::TEXTURE_2D);
            } else {
                gl.tex_parameter_i32(
                    glow::TEXTURE_2D,
                    glow::TEXTURE_MIN_FILTER,
                    glow::NEAREST.cast_signed(),
                );
                gl.tex_parameter_i32(
                    glow::TEXTURE_2D,
                    glow::TEXTURE_MAG_FILTER,
                    glow::NEAREST.cast_signed(),
                );
            }

            gl.bind_texture(glow::TEXTURE_2D, None);
        }
    }

    /// Builds and uploads the level's static geometry plus every placed prop.
    ///
    /// Called once per level load (never per frame): each distinct prop model is
    /// parsed once, every instance transform is baked into one shared vertex
    /// buffer, and each model's texture is uploaded once and then reused for the
    /// rest of the session.
    pub fn rebuild_level_geometry(
        &mut self,
        level: &crate::level::LevelDef,
        materials: &MaterialTable,
    ) {
        let started = std::time::Instant::now();
        let (mesh, batches, lighting, timings) =
            build_level_geometry_timed(level, &self.prop_catalog, &mut self.prop_assets, materials);
        self.spatial_grid = spatial_cell_grid(level);

        // Pack the static ranges into 16-bit-indexable buffer pairs. Each range
        // keeps its own vertex block and its indices are re-based as it is
        // packed, so a draw never needs a base-vertex offset (which core
        // OpenGL ES 2.0 does not have).
        // `LIMINAL_BENCH_NOINDEX` expands every range into a flat triangle list
        // before it is packed, so one build can measure indexed submission
        // against non-indexed submission with the same batching, culling and
        // vertex layout.
        let index_ranges = self.indexing_enabled;
        let mut static_packer = MeshPacker::default();
        let mut static_batches: Vec<StaticBatch> = Vec::with_capacity(mesh.ranges.len());
        for range in &mesh.ranges {
            let placements = if index_ranges {
                static_packer.push(&range.vertices, &range.indices)
            } else {
                static_packer.push_unindexed(&range.vertices, &range.indices)
            };
            for packed in placements {
                static_batches.push(StaticBatch {
                    key: range.key,
                    chunk: packed.chunk,
                    index_range: BatchRange {
                        start: packed.index_start,
                        count: packed.index_count,
                    },
                    vertex_count: packed.vertex_count,
                    bounds: range.bounds,
                });
            }
        }
        self.static_batches = static_batches;

        // Props go through the same packer, one range per (model, cell).
        let mut prop_packer = MeshPacker::default();
        let mut draws: Vec<PropDraw> = Vec::with_capacity(batches.len());
        for batch in &batches {
            let texture = match self.prop_textures.get(&batch.model) {
                Some(texture) => *texture,
                None => match unsafe { self.upload_prop_texture(&batch.texture) } {
                    Ok(texture) => {
                        self.prop_textures.insert(batch.model.clone(), texture);
                        texture
                    }
                    Err(error) => {
                        eprintln!(
                            "[props] cannot upload texture for {}: {error}; skipping that batch",
                            batch.model
                        );
                        continue;
                    }
                },
            };
            let placements = if index_ranges {
                prop_packer.push(&batch.vertices, &batch.indices)
            } else {
                prop_packer.push_unindexed(&batch.vertices, &batch.indices)
            };
            for packed in placements {
                draws.push(PropDraw {
                    texture,
                    chunk: packed.chunk,
                    index_start: packed.index_start,
                    index_count: packed.index_count,
                    vertex_count: packed.vertex_count,
                    bounds: batch.bounds,
                });
            }
        }

        let layout = self.vertex_layout;
        let upload_chunks = |gl: &glow::Context,
                             buffers: &mut Vec<(glow::Buffer, glow::Buffer)>,
                             chunks: &[MeshChunk]|
         -> Result<(), String> {
            unsafe {
                // Drop any buffers left over from a larger previous level.
                while buffers.len() > chunks.len() {
                    if let Some((vbo, ibo)) = buffers.pop() {
                        gl.delete_buffer(vbo);
                        gl.delete_buffer(ibo);
                    }
                }
                for (index, chunk) in chunks.iter().enumerate() {
                    if index == buffers.len() {
                        let vbo = gl.create_buffer()?;
                        let ibo = gl.create_buffer()?;
                        buffers.push((vbo, ibo));
                    }
                    let (vbo, ibo) = buffers[index];
                    gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
                    match layout {
                        VertexLayout::Packed => {
                            // The one and only place the exact build vertices
                            // become the packed GPU representation.
                            let packed: Vec<PackedVertex> =
                                chunk.vertices.iter().map(PackedVertex::from).collect();
                            let vertex_bytes = std::slice::from_raw_parts(
                                packed.as_ptr().cast::<u8>(),
                                packed.len() * std::mem::size_of::<PackedVertex>(),
                            );
                            gl.buffer_data_u8_slice(
                                glow::ARRAY_BUFFER,
                                vertex_bytes,
                                glow::STATIC_DRAW,
                            );
                        }
                        VertexLayout::Exact => {
                            let vertex_bytes = std::slice::from_raw_parts(
                                chunk.vertices.as_ptr().cast::<u8>(),
                                chunk.vertices.len() * std::mem::size_of::<Vertex>(),
                            );
                            gl.buffer_data_u8_slice(
                                glow::ARRAY_BUFFER,
                                vertex_bytes,
                                glow::STATIC_DRAW,
                            );
                        }
                    }

                    gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(ibo));
                    let index_bytes = std::slice::from_raw_parts(
                        chunk.indices.as_ptr().cast::<u8>(),
                        chunk.indices.len() * std::mem::size_of::<u16>(),
                    );
                    gl.buffer_data_u8_slice(
                        glow::ELEMENT_ARRAY_BUFFER,
                        index_bytes,
                        glow::STATIC_DRAW,
                    );
                }
                gl.bind_buffer(glow::ARRAY_BUFFER, None);
                gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, None);
            }
            Ok(())
        };

        if let Err(error) = upload_chunks(&self.gl, &mut self.level_buffers, &static_packer.chunks)
        {
            eprintln!("[level] cannot upload static geometry: {error}");
        }
        if let Err(error) = upload_chunks(&self.gl, &mut self.prop_buffers, &prop_packer.chunks) {
            eprintln!("[level] cannot upload prop geometry: {error}");
        }

        self.level_stats = LevelBuildStats {
            static_vertices: mesh.vertex_count,
            static_indices: mesh.index_count,
            prop_vertices: prop_packer.vertex_total(),
            prop_indices: prop_packer.index_total(),
            prop_draws: draws.len(),
            static_batches: self.static_batches.len(),
            static_chunks: self.level_buffers.len(),
            prop_chunks: self.prop_buffers.len(),
            vbo_bytes: (mesh.vertex_count + prop_packer.vertex_total())
                * self.vertex_layout.vertex_bytes(),
            index_bytes: (mesh.index_count + prop_packer.index_total())
                * std::mem::size_of::<u16>(),
            build_millis: started.elapsed().as_secs_f64() * 1000.0,
            lighting_millis: timings.lighting_millis,
            props_millis: timings.props_millis,
            surfaces_millis: timings.surfaces_millis,
            lighting: lighting.summary(),
        };
        self.prop_draws = draws;
    }

    /// Uploads one prop model's diffuse texture with mipmaps and `CLAMP_TO_EDGE`
    /// wrapping (prop UVs never tile), matching the game's filtering setting.
    unsafe fn upload_prop_texture(
        &self,
        image: &crate::loader::RawImage,
    ) -> Result<glow::Texture, String> {
        unsafe {
            let texture = self.gl.create_texture()?;
            self.gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            self.gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA.cast_signed(),
                i32::try_from(image.width).unwrap_or(i32::MAX),
                i32::try_from(image.height).unwrap_or(i32::MAX),
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(&image.rgba)),
            );
            self.gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_S,
                glow::CLAMP_TO_EDGE.cast_signed(),
            );
            self.gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_T,
                glow::CLAMP_TO_EDGE.cast_signed(),
            );
            set_repeat_filter(&self.gl, self.linear_filtering);
            self.gl.generate_mipmap(glow::TEXTURE_2D);
            self.gl.bind_texture(glow::TEXTURE_2D, None);
            Ok(texture)
        }
    }

    /// Rebuilds the level's geometry and uploads one GPU texture per distinct
    /// resolved material texture, without recompiling anything.
    ///
    /// Catalog textures and the diagnostic fallback stay resident across level
    /// changes; pack textures belong to their level and are freed here when the
    /// next level replaces them. The decoded images are already cached per
    /// session by [`crate::materials::TextureCache`], so a level switch never
    /// re-reads or re-decodes a PNG.
    /// Resolves and uploads the current level's external decal sheets.
    ///
    /// The four built-in patterns come from the generated atlas; a decal asset
    /// with `source: "file"` and a `.png` model is ordinary external artwork,
    /// decoded once per session and uploaded once per level exactly like a
    /// surface texture, so a creator edits the PNG and restarts. A sheet that
    /// cannot be resolved draws the same magenta/black diagnostic the surface
    /// pipeline uses, so the mistake is visible in game instead of silent.
    fn load_decal_sheets(&mut self, level: &crate::level::LevelDef) {
        self.decal.external.clear();
        let root = crate::assets::resolve_asset_root();
        let catalog = shipped_asset_catalog();
        for id in decal_external_sheet_ids(level, catalog) {
            let resolved = crate::materials::resolve_decal_sheet(
                catalog,
                root.as_deref(),
                &mut self.decal_image_cache,
                &id,
            );
            let (key, image) = match resolved {
                Ok(sheet) => (sheet.key, sheet.image),
                Err(error) => {
                    eprintln!("[decals] {error}; drawing the diagnostic sheet instead");
                    (
                        crate::materials::MISSING_TEXTURE_KEY.to_string(),
                        std::rc::Rc::new(crate::materials::missing_texture()),
                    )
                }
            };
            if let Some(handle) = self.decal_sheet_textures.get(&key) {
                self.decal.external.push(*handle);
                continue;
            }
            let uploaded = unsafe {
                create_texture_2d(
                    &self.gl,
                    i32::try_from(image.width).unwrap_or(i32::MAX),
                    i32::try_from(image.height).unwrap_or(i32::MAX),
                    &image.rgba,
                    true,
                    self.linear_filtering,
                )
            };
            match uploaded {
                Ok(texture) => {
                    self.decal_sheet_textures.insert(key, texture);
                    self.decal.external.push(texture);
                }
                Err(error) => {
                    eprintln!("[decals] decal `{id}`: {error}; drawing the built-in sheet instead");
                    self.decal.external.push(self.decal.texture);
                }
            }
        }
    }

    pub fn set_level(&mut self, loaded: &crate::loader::LoadedLevel) {
        self.rebuild_level_geometry(&loaded.level, &loaded.materials);
        self.load_decal_sheets(&loaded.level);
        let linear = self.linear_filtering;

        // Free the previous level's pack/missing uploads.
        unsafe {
            for (_, texture) in self.level_textures.drain(..) {
                if texture == self.white_texture || texture == self.font_texture {
                    continue;
                }
                self.gl.delete_texture(texture);
            }
        }

        let mut material_textures: Vec<glow::Texture> =
            Vec::with_capacity(loaded.materials.textures().len());
        let mut level_textures: Vec<(String, glow::Texture)> = Vec::new();
        for texture in loaded.materials.textures() {
            let persistent = !matches!(texture.origin, crate::materials::TextureOrigin::Pack);
            if persistent {
                if let Some(handle) = self.surface_textures.get(&texture.key) {
                    material_textures.push(*handle);
                    continue;
                }
            } else if let Some(handle) = level_textures
                .iter()
                .find(|(key, _)| *key == texture.key)
                .map(|(_, handle)| *handle)
            {
                material_textures.push(handle);
                continue;
            }

            let uploaded = unsafe {
                create_texture_2d(
                    &self.gl,
                    i32::try_from(texture.image.width).unwrap_or(i32::MAX),
                    i32::try_from(texture.image.height).unwrap_or(i32::MAX),
                    &texture.image.rgba,
                    true,
                    linear,
                )
            };
            match uploaded {
                Ok(handle) => {
                    if persistent {
                        self.surface_textures.insert(texture.key.clone(), handle);
                    } else {
                        level_textures.push((texture.key.clone(), handle));
                    }
                    material_textures.push(handle);
                }
                Err(error) => {
                    eprintln!(
                        "[materials] cannot upload texture `{}`: {error}; binding the missing pattern",
                        texture.key
                    );
                    material_textures.push(self.white_texture);
                }
            }
        }
        self.material_textures = material_textures;
        self.material_texture_slots = loaded
            .materials
            .entries()
            .iter()
            .map(|entry| entry.texture_index)
            .collect();
        self.level_textures = level_textures;

        // The light/fixture sheet: a pack may override it, otherwise the
        // untextured white sheet is restored so a previous pack's fixture never
        // leaks into the next level.
        unsafe {
            match loaded.fixture.as_deref() {
                Some(fixture) => {
                    Self::upload_texture(&self.gl, self.white_texture, fixture, false, linear)
                }
                None => {
                    let white = generate_white_texture();
                    Self::upload_texture(
                        &self.gl,
                        self.white_texture,
                        &crate::loader::RawImage::new(2, 2, white.to_vec()),
                        false,
                        linear,
                    );
                }
            }
        }
    }

    /// Renders the 3D level combining yaw and pitch into the view matrix.
    ///
    /// Takes `&mut self` because the pass records what it actually submitted
    /// (see [`RenderStats`]) for the debug-only benchmark harness.
    pub fn render_scene(
        &mut self,
        camera_pos: glam::Vec3,
        camera_yaw: f32,
        camera_pitch: f32,
        fov_degrees: f32,
    ) {
        let drawable = self.drawable_size;
        if drawable.is_empty() {
            return;
        }

        unsafe {
            // Render at the real drawable resolution; no fixed 480x272 target.
            self.gl.viewport(
                0,
                0,
                i32::try_from(drawable.width).unwrap_or(i32::MAX),
                i32::try_from(drawable.height).unwrap_or(i32::MAX),
            );
            self.gl
                .clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);

            self.gl.use_program(Some(self.program));

            // Derive the projection from the real framebuffer aspect ratio. The
            // configured FOV is the PocketCHIP baseline; wider displays gain
            // horizontal view, taller displays keep the horizontal view instead
            // of cropping it.
            let aspect = drawable.aspect_ratio();
            let effective_fov = vertical_fov_for_aspect(fov_degrees, aspect);
            let proj = glam::Mat4::perspective_rh(effective_fov.to_radians(), aspect, 0.1, 100.0);

            // Correctly combine yaw and pitch in the camera forward vector
            let cos_pitch = camera_pitch.cos();
            let forward = glam::Vec3::new(
                camera_yaw.sin() * cos_pitch,
                camera_pitch.sin(),
                -camera_yaw.cos() * cos_pitch,
            );
            let view = glam::Mat4::look_at_rh(camera_pos, camera_pos + forward, glam::Vec3::Y);
            let mvp = proj * view;

            // The frustum is extracted from the very matrix the GPU clips
            // against, so it can never disagree with what is on screen: pitch,
            // a resized drawable and an unusual aspect ratio are all included.
            let frustum = crate::spatial::Frustum::from_view_projection(
                &mvp,
                crate::spatial::DepthRange::ZeroToOne,
            );
            let cull = self.culling_enabled;

            if let Some(ref loc) = self.u_mvp_loc {
                self.gl
                    .uniform_matrix_4_f32_slice(Some(loc), false, &mvp.to_cols_array());
            }

            if let Some(ref loc) = self.u_texture_loc {
                self.gl.uniform_1_i32(Some(loc), 0);
            }
            self.gl.active_texture(glow::TEXTURE0);

            // Static level geometry: one range per (material, spatial cell).
            // The ranges are stored group-major and chunk-major, so walking them
            // in order binds each texture and each buffer pair as few times as
            // the partition allows while still dropping off-screen cells.
            let mut visible_vertices = 0usize;
            let mut visible_batches = 0usize;
            let mut draw_calls = 0usize;
            let mut bound_key: Option<SurfaceKey> = None;
            let mut bound_chunk: Option<usize> = None;
            for batch in &self.static_batches {
                if batch.index_range.count <= 0 {
                    continue;
                }
                // Decals are submitted by their own pass below, with the decal
                // program and depth bias. Skipping them here keeps the world
                // program's early depth testing intact.
                if batch.key.kind == SurfaceKind::Decal {
                    continue;
                }
                if cull && !frustum.intersects_aabb(&batch.bounds) {
                    continue;
                }
                if bound_chunk != Some(batch.chunk) {
                    if self.bind_chunk(&self.level_buffers, batch.chunk) {
                        bound_chunk = Some(batch.chunk);
                    } else {
                        continue;
                    }
                }
                if bound_key != Some(batch.key) {
                    let texture = match batch.key.kind {
                        SurfaceKind::Floor | SurfaceKind::Ceiling | SurfaceKind::Wall => {
                            if batch.key.has_material() {
                                let slot = self
                                    .material_texture_slots
                                    .get(batch.key.material as usize)
                                    .copied()
                                    .unwrap_or(0);
                                self.material_textures
                                    .get(slot as usize)
                                    .copied()
                                    .unwrap_or(self.white_texture)
                            } else {
                                // No resolved material (an empty authored id):
                                // the unshaded sheet is the honest fallback.
                                self.white_texture
                            }
                        }
                        SurfaceKind::Light | SurfaceKind::PropFallback => self.white_texture,
                        SurfaceKind::Decal => self.decal.texture,
                    };
                    self.gl.bind_texture(glow::TEXTURE_2D, Some(texture));
                    bound_key = Some(batch.key);
                }
                self.gl.draw_elements(
                    glow::TRIANGLES,
                    batch.index_range.count,
                    glow::UNSIGNED_SHORT,
                    batch.index_range.start * 2,
                );
                visible_vertices += usize::try_from(batch.vertex_count.max(0)).unwrap_or(0);
                visible_batches += 1;
                draw_calls += 1;
            }

            // 5. Draw the batched real prop geometry: one buffer and one draw
            //    call per (model, spatial cell), with one texture bind per model.
            if !self.prop_draws.is_empty() {
                let mut bound_texture: Option<glow::Texture> = None;
                let mut bound_chunk: Option<usize> = None;
                for draw in &self.prop_draws {
                    if draw.index_count <= 0 {
                        continue;
                    }
                    if cull && !frustum.intersects_aabb(&draw.bounds) {
                        continue;
                    }
                    if bound_chunk != Some(draw.chunk) {
                        if self.bind_chunk(&self.prop_buffers, draw.chunk) {
                            bound_chunk = Some(draw.chunk);
                        } else {
                            continue;
                        }
                    }
                    if bound_texture != Some(draw.texture) {
                        self.gl.bind_texture(glow::TEXTURE_2D, Some(draw.texture));
                        bound_texture = Some(draw.texture);
                    }
                    self.gl.draw_elements(
                        glow::TRIANGLES,
                        draw.index_count,
                        glow::UNSIGNED_SHORT,
                        draw.index_start * 2,
                    );
                    visible_vertices += usize::try_from(draw.vertex_count.max(0)).unwrap_or(0);
                    visible_batches += 1;
                    draw_calls += 1;
                }
            }

            // 6. Decal pass: local surface markings drawn after the opaque world
            //    and the props. Depth testing stays on and depth writes stay on,
            //    so a decal is still hidden by anything in front of it; the pass
            //    adds a fixed polygon offset that pulls each decal two depth
            //    steps towards the camera, which is what makes it win the
            //    coincident-depth test against the surface it lies on. The state
            //    is restored before the pass returns.
            let mut decal_active = false;
            let mut decal_chunk: Option<usize> = None;
            let mut bound_decal_texture: Option<glow::Texture> = None;
            for batch in &self.static_batches {
                if batch.key.kind != SurfaceKind::Decal || batch.index_range.count <= 0 {
                    continue;
                }
                if cull && !frustum.intersects_aabb(&batch.bounds) {
                    continue;
                }
                if !decal_active {
                    self.gl.use_program(Some(self.decal.program));
                    if let Some(ref loc) = self.decal.u_mvp_loc {
                        self.gl
                            .uniform_matrix_4_f32_slice(Some(loc), false, &mvp.to_cols_array());
                    }
                    if let Some(ref loc) = self.decal.u_texture_loc {
                        self.gl.uniform_1_i32(Some(loc), 0);
                    }
                    if let Some(ref loc) = self.decal.u_alpha_cutoff_loc {
                        self.gl.uniform_1_f32(Some(loc), DECAL_ALPHA_CUTOFF);
                    }
                    let (factor, units) = DECAL_POLYGON_OFFSET;
                    self.gl.enable(glow::POLYGON_OFFSET_FILL);
                    self.gl.polygon_offset(factor, units);
                    self.gl
                        .bind_texture(glow::TEXTURE_2D, Some(self.decal.texture));
                    decal_active = true;
                    // Both programs share attribute locations, but rebind from
                    // scratch so the pass cannot depend on what the world loop
                    // left bound.
                    decal_chunk = None;
                }
                if decal_chunk != Some(batch.chunk) {
                    if self.bind_chunk(&self.level_buffers, batch.chunk) {
                        decal_chunk = Some(batch.chunk);
                    } else {
                        continue;
                    }
                }
                let sheet = if batch.key.material < DECAL_EXTERNAL_BASE as MaterialIndex {
                    self.decal.texture
                } else {
                    self.decal
                        .external
                        .get((batch.key.material - DECAL_EXTERNAL_BASE as MaterialIndex) as usize)
                        .copied()
                        .unwrap_or(self.decal.texture)
                };
                if bound_decal_texture != Some(sheet) {
                    self.gl.bind_texture(glow::TEXTURE_2D, Some(sheet));
                    bound_decal_texture = Some(sheet);
                }
                self.gl.draw_elements(
                    glow::TRIANGLES,
                    batch.index_range.count,
                    glow::UNSIGNED_SHORT,
                    batch.index_range.start * 2,
                );
                visible_vertices += usize::try_from(batch.vertex_count.max(0)).unwrap_or(0);
                visible_batches += 1;
                draw_calls += 1;
            }
            if decal_active {
                // Restore the exact scene state: no polygon offset, and the
                // world program (whose uniforms are per-program and still
                // valid), so nothing after the pass can inherit decal state.
                self.gl.polygon_offset(0.0, 0.0);
                self.gl.disable(glow::POLYGON_OFFSET_FILL);
                self.gl.use_program(Some(self.program));
            }

            self.gl.disable_vertex_attrib_array(self.a_pos_loc);
            self.gl.disable_vertex_attrib_array(self.a_color_loc);
            self.gl.disable_vertex_attrib_array(self.a_uv_loc);
            self.gl.bind_texture(glow::TEXTURE_2D, None);
            self.gl.bind_buffer(glow::ARRAY_BUFFER, None);
            self.gl.use_program(None);

            // Report what this frame actually submitted, straight from the draw
            // path rather than reconstructed from the level.
            let total_vertices = self.level_stats.static_vertices + self.level_stats.prop_vertices;
            self.render_stats = RenderStats {
                total_vertices,
                visible_vertices,
                culled_vertices: total_vertices.saturating_sub(visible_vertices),
                total_batches: self.static_batches.len() + self.prop_draws.len(),
                visible_batches,
                draw_calls,
                vbo_bytes: self.level_stats.vbo_bytes,
                index_bytes: self.level_stats.index_bytes,
            };
        }
    }

    /// Selects the GPU vertex layout. Packed is the shipping default; the
    /// debug benchmark selects `Exact` to measure the packing win on the same
    /// build with everything else held constant.
    pub const fn set_vertex_layout(&mut self, layout: VertexLayout) {
        self.vertex_layout = layout;
    }

    /// Enables or disables indexed submission. Only the debug benchmark turns
    /// this off, to measure what indexing is worth on real hardware.
    pub const fn set_indexing(&mut self, enabled: bool) {
        self.indexing_enabled = enabled;
    }

    /// Enables or disables frustum culling.
    ///
    /// Culling is always on in normal play; the debug benchmark harness turns it
    /// off so the same build can measure what it is worth on real hardware.
    pub const fn set_culling(&mut self, enabled: bool) {
        self.culling_enabled = enabled;
    }

    /// Spatial grid the current level was partitioned with, for developer logs.
    pub const fn spatial_grid(&self) -> crate::spatial::CellGrid {
        self.spatial_grid
    }

    /// Number of cullable static ranges the current level is split into.
    pub const fn static_batch_count(&self) -> usize {
        self.static_batches.len()
    }

    /// Static batch count per [`SurfaceKind`], in [`SurfaceKind::ALL`] order.
    ///
    /// Printed once per level load so a hardware run makes it obvious when a
    /// level has been shredded into more draw calls than the GPU can afford.
    pub fn static_batch_breakdown(&self) -> [usize; SurfaceKind::ALL.len()] {
        let mut counts = [0usize; SurfaceKind::ALL.len()];
        for batch in &self.static_batches {
            counts[batch.key.kind as usize] += 1;
        }
        counts
    }

    /// Static batch count per [`SurfaceKind`], in [`SurfaceKind::ALL`] order.
    ///
    /// The developer log reports families; how many distinct materials a level
    /// uses is a content choice, not a separate kind of surface.
    pub fn static_batch_family_breakdown(&self) -> [usize; SurfaceKind::ALL.len()] {
        let mut counts = [0usize; SurfaceKind::ALL.len()];
        for batch in &self.static_batches {
            counts[batch.key.kind as usize] += 1;
        }
        counts
    }

    /// Counters for the most recently submitted scene (see [`RenderStats`]).
    pub const fn render_stats(&self) -> RenderStats {
        self.render_stats
    }

    /// Points the three scene attributes at the selected vertex layout.
    ///
    /// In the packed layout `normalized = true` lets the fixed-function pipeline
    /// expand `GL_UNSIGNED_BYTE` colour to `[0, 1]` floats, so the shader is the
    /// same `vec4` in both layouts. Both are core OpenGL ES 2.0.
    fn set_vertex_attributes(&self) {
        let stride = self.vertex_layout.stride();
        let (color_type, color_normalized, color_offset) = match self.vertex_layout {
            VertexLayout::Packed => (glow::UNSIGNED_BYTE, true, packed_layout::COLOR_OFFSET),
            VertexLayout::Exact => (glow::FLOAT, false, 12),
        };
        let uv_offset = match self.vertex_layout {
            VertexLayout::Packed => packed_layout::UV_OFFSET,
            VertexLayout::Exact => 28,
        };
        unsafe {
            self.gl.enable_vertex_attrib_array(self.a_pos_loc);
            self.gl
                .vertex_attrib_pointer_f32(self.a_pos_loc, 3, glow::FLOAT, false, stride, 0);
            self.gl.enable_vertex_attrib_array(self.a_color_loc);
            self.gl.vertex_attrib_pointer_f32(
                self.a_color_loc,
                4,
                color_type,
                color_normalized,
                stride,
                color_offset,
            );
            self.gl.enable_vertex_attrib_array(self.a_uv_loc);
            self.gl.vertex_attrib_pointer_f32(
                self.a_uv_loc,
                2,
                glow::FLOAT,
                false,
                stride,
                uv_offset,
            );
        }
    }

    /// Binds one vertex/index buffer pair and points the vertex attributes at it.
    ///
    /// The attribute pointers are captured against whichever vertex buffer is
    /// bound when they are set, so every chunk needs them re-issued once per
    /// frame. Returns `false` for a chunk that does not exist, so a caller can
    /// skip rather than draw from a stale binding.
    fn bind_chunk(&self, buffers: &[(glow::Buffer, glow::Buffer)], chunk: usize) -> bool {
        let Some(&(vbo, ibo)) = buffers.get(chunk) else {
            return false;
        };
        unsafe {
            self.gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
            self.set_vertex_attributes();
            self.gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(ibo));
        }
        true
    }

    /// Drains the GL pipeline. Only used by the debug benchmark harness to
    /// separate renderer completion time from the presentation wait; it is a
    /// hard sync and must never be called in the normal frame loop.
    pub fn finish(&self) {
        unsafe { self.gl.finish() };
    }

    /// Renders a 2D UI overlay on top of the scene using an orthographic projection and the font atlas.
    ///
    /// UI geometry is authored in the 480x272 reference space; the projection
    /// below stays in that space while the viewport is scaled/centred to the
    /// drawable, so the HUD keeps its proportions at any resolution.
    ///
    /// Takes `&mut self` for the packed-vertex scratch buffer: UI geometry is
    /// produced once per frame as exact floats and converted here, so a single
    /// packed vertex layout (and one shader) serves both the scene and the HUD.
    pub fn render_ui(&mut self, ui_vertices: &[Vertex]) {
        let drawable = self.drawable_size;
        if ui_vertices.is_empty() || drawable.is_empty() {
            return;
        }

        let viewport = drawable.ui_viewport();

        unsafe {
            self.gl.disable(glow::DEPTH_TEST);
            self.gl.enable(glow::BLEND);
            self.gl
                .blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);

            self.gl.use_program(Some(self.program));

            self.gl
                .viewport(viewport.x, viewport.y, viewport.width, viewport.height);

            let ortho = glam::Mat4::orthographic_rh(
                0.0,
                UI_REFERENCE_WIDTH as f32,
                UI_REFERENCE_HEIGHT as f32,
                0.0,
                -1.0,
                1.0,
            );

            if let Some(ref loc) = self.u_mvp_loc {
                self.gl
                    .uniform_matrix_4_f32_slice(Some(loc), false, &ortho.to_cols_array());
            }

            if let Some(ref loc) = self.u_texture_loc {
                self.gl.uniform_1_i32(Some(loc), 0);
            }
            self.gl.active_texture(glow::TEXTURE0);
            self.gl
                .bind_texture(glow::TEXTURE_2D, Some(self.font_texture));

            self.gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.ui_vbo));
            // The HUD is a few thousand vertices rebuilt every frame anyway, so
            // converting it here keeps one vertex layout and one shader for both
            // the scene and the UI.
            let byte_slice = match self.vertex_layout {
                VertexLayout::Packed => {
                    self.ui_scratch.clear();
                    self.ui_scratch
                        .extend(ui_vertices.iter().map(PackedVertex::from));
                    self.ui_packed_len = self.ui_scratch.len();
                    std::slice::from_raw_parts(
                        self.ui_scratch.as_ptr().cast::<u8>(),
                        std::mem::size_of_val(self.ui_scratch.as_slice()),
                    )
                }
                VertexLayout::Exact => {
                    self.ui_packed_len = ui_vertices.len();
                    std::slice::from_raw_parts(
                        ui_vertices.as_ptr().cast::<u8>(),
                        std::mem::size_of_val(ui_vertices),
                    )
                }
            };
            self.gl
                .buffer_data_u8_slice(glow::ARRAY_BUFFER, byte_slice, glow::DYNAMIC_DRAW);

            self.set_vertex_attributes();

            self.gl.draw_arrays(
                glow::TRIANGLES,
                0,
                i32::try_from(self.ui_packed_len).unwrap_or(i32::MAX),
            );

            self.gl.disable_vertex_attrib_array(self.a_pos_loc);
            self.gl.disable_vertex_attrib_array(self.a_color_loc);
            self.gl.disable_vertex_attrib_array(self.a_uv_loc);
            self.gl.bind_texture(glow::TEXTURE_2D, None);
            self.gl.bind_buffer(glow::ARRAY_BUFFER, None);
            self.gl.use_program(None);

            self.gl.disable(glow::BLEND);
            self.gl.enable(glow::DEPTH_TEST);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spatial::{DepthRange, Frustum};
    use crate::test_support::{assert_exact, assert_exact_array, assert_exact_named};

    // ------------------------------------------------------- vertex packing

    #[test]
    fn the_packed_vertex_is_twenty_four_bytes_with_the_declared_layout() {
        assert_eq!(
            std::mem::size_of::<PackedVertex>(),
            24,
            "the packed scene vertex must be 24 bytes"
        );
        assert_eq!(std::mem::align_of::<PackedVertex>(), 4);
        assert_eq!(packed_layout::STRIDE, 24, "stride must match the struct");
        assert_eq!(
            VertexLayout::Exact.stride(),
            i32::try_from(std::mem::size_of::<Vertex>()).unwrap_or(i32::MAX),
            "the exact layout's stride must match the struct"
        );
        // The offsets are what `set_packed_vertex_attributes` hands to
        // `glVertexAttribPointer`; a struct change must move them too.
        assert_eq!(
            std::mem::offset_of!(PackedVertex, pos),
            packed_layout::POS_OFFSET as usize
        );
        assert_eq!(
            std::mem::offset_of!(PackedVertex, color),
            packed_layout::COLOR_OFFSET as usize
        );
        assert_eq!(
            std::mem::offset_of!(PackedVertex, uv),
            packed_layout::UV_OFFSET as usize
        );
        assert_eq!(
            packed_layout::UV_OFFSET as usize + 2 * 4,
            packed_layout::STRIDE as usize
        );
        assert_eq!(
            packed_layout::COLOR_OFFSET as usize + 4,
            packed_layout::UV_OFFSET as usize,
            "colour must be four packed bytes"
        );
        // 12 bytes saved per vertex against the original representation.
        assert_eq!(
            std::mem::size_of::<Vertex>() - std::mem::size_of::<PackedVertex>(),
            12
        );
    }

    /// The quantisation error the packed colour can introduce, in channel units.
    fn packed_channel_error(value: f32) -> f32 {
        let packed = PackedVertex::from(&Vertex {
            pos: [0.0, 0.0, 0.0],
            color: [value, value, value, value],
            uv: [0.0, 0.0],
        });
        (dequantize_unit(packed.color[0]) - value).abs()
    }

    #[test]
    fn packed_colour_is_accurate_at_the_lighting_extremes_and_in_between() {
        // Minimum baked lighting: the darkest a vertex can get.
        assert!(packed_channel_error(crate::lighting::AMBIENT_LEVEL) < 0.5 / 255.0);
        // Maximum brightness.
        assert!(packed_channel_error(crate::lighting::MAX_BRIGHTNESS) < 0.5 / 255.0);
        // Darkest and brightest possible shades of a wall/floor tint.
        assert!(packed_channel_error(0.0) < 1e-6);
        assert!(packed_channel_error(1.0) < 1e-6);
        // A representative intermediate value, and one that lands exactly
        // between two steps (the worst case).
        for value in [0.666, 0.42, 127.5 / 255.0, 1.0 / 255.0, 0.999] {
            assert!(
                packed_channel_error(value) <= 0.5 / 255.0 + 1e-6,
                "value {value} quantised by more than half a step"
            );
        }
        // The whole usable lighting range, swept at 1/1000.
        let mut worst = 0.0f32;
        for step in 0..=1000 {
            let value = crate::lighting::AMBIENT_LEVEL
                + (crate::lighting::MAX_BRIGHTNESS - crate::lighting::AMBIENT_LEVEL) * step as f32
                    / 1000.0;
            worst = worst.max(packed_channel_error(value));
        }
        assert!(
            worst <= 0.5 / 255.0 + 1e-6,
            "worst lighting quantisation {worst} exceeds half a step"
        );
    }

    #[test]
    fn packed_colour_clamps_instead_of_wrapping() {
        // A malformed level or an over-bright authored shade must saturate, not
        // wrap to the opposite end of the range.
        for (value, expected) in [
            (-1.0f32, 0u8),
            (-0.001, 0),
            (0.0, 0),
            (1.0, 255),
            (1.5, 255),
            (f32::INFINITY, 255),
            (f32::NEG_INFINITY, 0),
        ] {
            let packed = PackedVertex::from(&Vertex {
                pos: [0.0, 0.0, 0.0],
                color: [value, value, value, 1.0],
                uv: [0.0, 0.0],
            });
            assert_eq!(
                packed.color[0], expected,
                "value {value} must clamp to {expected}"
            );
        }
        let nan = PackedVertex::from(&Vertex {
            pos: [0.0, 0.0, 0.0],
            color: [f32::NAN; 4],
            uv: [0.0, 0.0],
        });
        assert_eq!(
            nan.color,
            [0, 0, 0, 0],
            "NaN must not become a bright value"
        );
    }

    #[test]
    fn packed_alpha_is_preserved_for_props_and_the_hud() {
        // Prop models carry alpha from their glTF `COLOR_0`, and the UI blends
        // with it, so the fourth channel must survive packing.
        for value in [0.0f32, 0.25, 0.5, 1.0] {
            let packed = PackedVertex::from(&Vertex {
                pos: [0.0, 0.0, 0.0],
                color: [1.0, 1.0, 1.0, value],
                uv: [0.0, 0.0],
            });
            assert!(
                (dequantize_unit(packed.color[3]) - value).abs() <= 0.5 / 255.0 + 1e-6,
                "alpha {value} did not survive packing"
            );
        }
    }

    #[test]
    fn packed_positions_and_uvs_are_bit_exact() {
        // World positions and texture coordinates are not quantised at all: a
        // 250-metre level and a world-space tiling UV both need the range.
        let samples: [[f32; 3]; 5] = [
            [0.0, 0.0, 0.0],
            [-131.9975, 3.4999, 132.0001],
            [1.0e-7, -1.0e-7, 2.5],
            [1.0e6, -1.0e6, 0.5],
            [-0.0, 0.1, -0.1],
        ];
        for pos in samples {
            let uv = [-131.9975f32, 132.0001];
            let vertex = Vertex {
                pos,
                color: [0.5, 0.5, 0.5, 1.0],
                uv,
            };
            let packed = PackedVertex::from(&vertex);
            assert_eq!(packed.pos.map(f32::to_bits), pos.map(f32::to_bits));
            assert_eq!(packed.uv.map(f32::to_bits), uv.map(f32::to_bits));
        }
    }

    #[test]
    fn packing_a_whole_mesh_never_moves_geometry_or_uvs() {
        // End-to-end over a real level: every packed vertex must agree with the
        // build vertex it came from, bit for bit, except for the shade that is
        // deliberately quantised.
        let mesh = build_level_geometry(&two_cluster_level(4));
        for range in &mesh.ranges {
            for vertex in &range.vertices {
                let packed = PackedVertex::from(vertex);
                assert_exact_array(packed.pos, vertex.pos);
                assert_exact_array(packed.uv, vertex.uv);
                for channel in 0..4 {
                    assert!(
                        (dequantize_unit(packed.color[channel]) - vertex.color[channel]).abs()
                            <= 0.5 / 255.0 + 1e-6,
                        "channel {channel} drifted: {} vs {}",
                        dequantize_unit(packed.color[channel]),
                        vertex.color[channel]
                    );
                }
            }
        }
    }

    #[test]
    fn the_packed_layout_shrinks_gpu_memory_by_a_third() {
        let mesh = build_level_geometry(&two_cluster_level(6));
        let packed_bytes = mesh.vertex_count * std::mem::size_of::<PackedVertex>();
        let unpacked_bytes = mesh.vertex_count * std::mem::size_of::<Vertex>();
        assert_eq!(packed_bytes * 3, unpacked_bytes * 2, "36 -> 24 bytes");
        // Indices are unchanged at two bytes each, so the whole static buffer
        // footprint drops by a quarter, not a third.
        let packed_total = packed_bytes + mesh.index_count * 2;
        let unpacked_total = unpacked_bytes + mesh.index_count * 2;
        assert!(packed_total < unpacked_total);
    }

    /// Loads one shipped texture asset from the catalog and decodes it.
    fn texture_image(texture_id: &str) -> crate::materials::RawImage {
        let catalog = crate::assets::AssetCatalog::load_default();
        let root = crate::assets::resolve_asset_root().expect("assets/ is discoverable");
        let path = catalog
            .texture_path(texture_id)
            .unwrap_or_else(|| panic!("{texture_id} must be a file texture"));
        crate::materials::load_png_relative(&root, path)
            .unwrap_or_else(|error| panic!("{texture_id}: {error}"))
    }

    #[test]
    fn test_shipped_texture_assets_are_opaque_and_within_budget() {
        // The six office surfaces are opaque 128x128 two-metre tiles; the one
        // NPOT diagnostic proves arbitrary PNG dimensions load.
        for texture in [
            "core:tex_wallpaper_yellow_01",
            "core:tex_wallpaper_stained_01",
            "core:tex_carpet_beige_01",
            "core:tex_carpet_damp_01",
            "core:tex_ceiling_panel_01",
            "core:tex_ceiling_stained_01",
        ] {
            let image = texture_image(texture);
            assert_eq!((image.width, image.height), (128, 128), "{texture}");
            assert_eq!(image.rgba.len(), (128 * 128 * 4) as usize);
            for texel in image.rgba.as_chunks::<4>().0 {
                assert_eq!(texel[3], 255, "{texture} must be fully opaque");
            }
        }

        let npot = texture_image("core:tex_diagnostic_alt_01");
        assert_eq!((npot.width, npot.height), (96, 64));
        assert_eq!(npot.rgba.len(), (96 * 64 * 4) as usize);
    }

    /// The surface textures tile: a wrapped edge must join its opposite edge,
    /// or a floor or ceiling shows a grid of seams every repeat.
    #[test]
    fn test_shipped_surface_textures_tile() {
        for (name, texture) in [
            ("wall", "core:tex_wallpaper_yellow_01"),
            ("wall_stained", "core:tex_wallpaper_stained_01"),
            ("carpet", "core:tex_carpet_beige_01"),
            ("carpet_damp", "core:tex_carpet_damp_01"),
            ("ceiling", "core:tex_ceiling_panel_01"),
            ("ceiling_stained", "core:tex_ceiling_stained_01"),
        ] {
            let image = texture_image(texture);
            let (size_x, size_y) = (image.width, image.height);
            let texel = |x: u32, y: u32| -> [i32; 3] {
                let index = ((y * size_x + x) * 4) as usize;
                [
                    i32::from(image.rgba[index]),
                    i32::from(image.rgba[index + 1]),
                    i32::from(image.rgba[index + 2]),
                ]
            };
            for i in 0..size_y.min(size_x) {
                for channel in 0..3 {
                    assert!(
                        (texel(size_x - 1, i)[channel] - texel(0, i)[channel]).abs() <= 40,
                        "{name}: column seam at row {i}"
                    );
                    assert!(
                        (texel(i, size_y - 1)[channel] - texel(i, 0)[channel]).abs() <= 40,
                        "{name}: row seam at column {i}"
                    );
                }
            }
        }
    }

    #[test]
    fn test_build_geometry_from_test_room() {
        let json = include_str!("../assets/levels/test_room.json");
        let level = LevelDef::from_json(json).expect("valid test_room json");
        let mesh = build_level_geometry(&level);
        assert!(mesh.vertex_count != 0);
        assert_eq!(mesh.index_count % 6, 0, "geometry is whole quads");
        assert!(
            mesh.vertex_count < mesh.index_count,
            "indexing must share corners"
        );
        assert!(mesh.batches.floor_batch.count > 0);
        assert!(mesh.batches.ceiling_batch.count > 0);
        assert!(mesh.batches.wall_batch.count > 0);
    }

    #[test]
    fn test_floor_geometry_does_not_scale_with_room_area() {
        let level = |size: f32| {
            let json = format!(
                r#"{{
                    "format_version": 1,
                    "id": "big",
                    "name": "Big",
                    "spawn": {{ "x": 0.0, "z": 0.0 }},
                    "rooms": [{{ "x": 0.0, "z": 0.0, "width": {size}, "depth": {size}, "height": 3.5 }}]
                }}"#
            );
            LevelDef::from_json(&json).expect("valid json")
        };

        // A large room is subdivided on the bounded baked-lighting grid so
        // fixture pools can vary across the floor, but the cell count is capped
        // and flat regions merge: a 400x400 m room costs exactly the same as a
        // 100x100 m one, and with no fixtures both collapse to a single quad.
        let hundred = build_level_geometry(&level(100.0));
        let four_hundred = build_level_geometry(&level(400.0));
        let cap = i32::try_from(
            crate::lighting::MAX_LIGHT_GRID_CELLS * crate::lighting::MAX_LIGHT_GRID_CELLS,
        )
        .unwrap_or(i32::MAX)
            * 6;
        for mesh in [&hundred, &four_hundred] {
            assert!(mesh.batches.floor_batch.count > 0);
            assert!(mesh.batches.ceiling_batch.count > 0);
            assert!(
                mesh.batches.floor_batch.count <= cap,
                "floor geometry must stay capped, got {}",
                mesh.batches.floor_batch.count
            );
            assert!(
                mesh.batches.ceiling_batch.count <= cap,
                "ceiling geometry must stay capped, got {}",
                mesh.batches.ceiling_batch.count
            );
        }
        // No fixtures and no fixtures nearby: the uniform room merges to one
        // quad on each surface, so area genuinely stops mattering.
        assert_eq!(hundred.batches.floor_batch.count, 6);
        assert_eq!(hundred.batches.ceiling_batch.count, 6);
        assert_eq!(
            hundred.batches.floor_batch.count,
            four_hundred.batches.floor_batch.count
        );
        assert_eq!(
            hundred.batches.ceiling_batch.count,
            four_hundred.batches.ceiling_batch.count
        );

        // A room smaller than one lighting cell stays a single quad.
        let small = build_level_geometry(&level(2.0));
        assert_eq!(small.batches.floor_batch.count, 6);
        assert_eq!(small.batches.ceiling_batch.count, 6);
    }

    /// The metre checker lives in the carpet PNG now, not in a bake step: the
    /// bright and dark quadrants of the two-metre tile must actually differ, so
    /// the external asset reproduces the historical floor read.
    #[test]
    /// The final carpet must never read as the old metre checker again.
    ///
    /// The seed art carried a deliberate 1 m bright/dark quadrant tint, which
    /// looked like a debug board on a large floor. The Goal 5 artwork replaces
    /// it with low-frequency pile variation, so the four quadrant means must be
    /// close: the sheet may be mottled, but no quadrant may be a visibly
    /// different flat cell.
    fn test_carpet_png_has_no_metre_checker() {
        let carpet = texture_image("core:tex_carpet_beige_01");
        assert_eq!((carpet.width, carpet.height), (128, 128));
        let mean = |x0: u32, y0: u32| -> f32 {
            let mut total = 0.0f32;
            for y in y0..y0 + 64 {
                for x in x0..x0 + 64 {
                    let index = ((y * 128 + x) * 4) as usize;
                    total += f32::from(carpet.rgba[index])
                        + f32::from(carpet.rgba[index + 1])
                        + f32::from(carpet.rgba[index + 2]);
                }
            }
            total / (64.0 * 64.0 * 3.0)
        };
        let quadrants = [mean(0, 0), mean(64, 0), mean(0, 64), mean(64, 64)];
        let low = quadrants.iter().copied().fold(f32::MAX, f32::min);
        let high = quadrants.iter().copied().fold(f32::MIN, f32::max);
        // The historical checker tinted adjacent metre cells roughly 7-10
        // levels apart; gentle low-frequency pile mottle stays well under that,
        // so a 4-level spread separates the two cases.
        assert!(
            high - low <= 4.0,
            "the carpet reads as a 1 m checker again: quadrant means {quadrants:?}"
        );
        // It must still be carpet, not a flat colour: the sheet needs some
        // pixel-level variation to read as pile under dim warm light.
        let mut min = u8::MAX;
        let mut max = u8::MIN;
        for y in (0..128).step_by(7) {
            for x in (0..128).step_by(5) {
                let value = carpet.rgba[((y * 128 + x) * 4) as usize];
                min = min.min(value);
                max = max.max(value);
            }
        }
        assert!(
            u16::from(max) - u16::from(min) >= 4,
            "the carpet has no visible pile variation ({min}..{max})"
        );
    }

    #[test]
    fn test_drawable_aspect_ratio() {
        let cases: [(u32, u32, f32); 7] = [
            (480, 272, 480.0 / 272.0),
            (1280, 720, 16.0 / 9.0),
            (1920, 1080, 16.0 / 9.0),
            (2560, 1440, 16.0 / 9.0),
            (3840, 2160, 16.0 / 9.0),
            (1600, 1200, 4.0 / 3.0),
            (960, 544, 480.0 / 272.0), // Retina 2x of the PocketCHIP baseline
        ];
        for (w, h, expected) in cases {
            let size = DrawableSize::new(w, h);
            assert!(
                (size.aspect_ratio() - expected).abs() < 1e-5,
                "{w}x{h} aspect mismatch"
            );
            assert!(!size.is_empty());
        }
    }

    fn horizontal_fov_degrees(vertical_fov_degrees: f32, aspect: f32) -> f32 {
        let half = (vertical_fov_degrees.to_radians() * 0.5).tan() * aspect;
        (2.0 * half.atan()).to_degrees()
    }

    #[test]
    fn test_vertical_fov_baseline_is_identity() {
        let baseline = reference_aspect_ratio();
        for fov in [45.0, 60.0, 90.0, 110.0] {
            assert!((vertical_fov_for_aspect(fov, baseline) - fov).abs() < 1e-4);
        }
    }

    #[test]
    fn test_wider_displays_expand_horizontally() {
        // 16:9 and 21:9 are wider than the 480x272 baseline, so the vertical FOV
        // is unchanged and the horizontal view simply grows.
        let baseline = reference_aspect_ratio();
        for aspect in [16.0 / 9.0, 21.0 / 9.0, 32.0 / 9.0] {
            assert!(aspect > baseline);
            let vfov = vertical_fov_for_aspect(60.0, aspect);
            assert_exact_named(vfov, 60.0, "wider aspect must keep vertical FOV");
            assert!(horizontal_fov_degrees(vfov, aspect) > horizontal_fov_degrees(60.0, baseline));
        }
    }

    #[test]
    fn test_taller_displays_preserve_horizontal_view() {
        let baseline = reference_aspect_ratio();
        let baseline_hfov = horizontal_fov_degrees(60.0, baseline);
        // 16:10, 4:3, 3:2 and 1:1 are all narrower than PocketCHIP.
        for aspect in [16.0 / 10.0, 4.0 / 3.0, 3.0 / 2.0, 1.0] {
            assert!(aspect < baseline);
            let vfov = vertical_fov_for_aspect(60.0, aspect);
            assert!(vfov > 60.0, "taller aspect must widen vertical FOV");
            let hfov = horizontal_fov_degrees(vfov, aspect);
            assert!(
                (hfov - baseline_hfov).abs() < 1e-3,
                "horizontal FOV cropped: {hfov} vs {baseline_hfov}"
            );
        }
    }

    #[test]
    fn test_vertical_fov_handles_degenerate_aspects() {
        assert_exact(vertical_fov_for_aspect(60.0, 0.0), 60.0);
        assert_exact(vertical_fov_for_aspect(60.0, -1.0), 60.0);
        assert_exact(vertical_fov_for_aspect(60.0, f32::NAN), 60.0);
        // Extremely tall windows are capped to keep the projection invertible.
        assert!(vertical_fov_for_aspect(60.0, 0.1) <= 150.0);
    }

    #[test]
    fn test_zero_sized_drawable_is_empty_and_safe() {
        for size in [
            DrawableSize::new(0, 0),
            DrawableSize::new(0, 272),
            DrawableSize::new(480, 0),
        ] {
            assert!(size.is_empty());
            // Must not divide by zero or panic when a window is minimized.
            assert!(size.aspect_ratio().is_finite());
            let viewport = size.ui_viewport();
            assert_eq!(viewport.width, 0);
            assert_eq!(viewport.height, 0);
        }
    }

    #[test]
    fn test_ui_viewport_is_uniform_and_centred() {
        let cases = [
            (480, 272),
            (1280, 720),
            (1920, 1080),
            (2560, 1440),
            (3840, 2160),
            (1600, 1200),
            (960, 544),
        ];
        for (w, h) in cases {
            let size = DrawableSize::new(w, h);
            let vp = size.ui_viewport();

            // Fits inside the drawable and stays centred.
            assert!(
                vp.width <= i32::try_from(w).unwrap_or(i32::MAX)
                    && vp.height <= i32::try_from(h).unwrap_or(i32::MAX)
            );
            assert!(vp.x >= 0 && vp.y >= 0);
            assert!(
                (i32::try_from(size.width).unwrap_or(i32::MAX) - vp.width - 2 * vp.x).abs() <= 1
            );
            assert!(
                (i32::try_from(size.height).unwrap_or(i32::MAX) - vp.height - 2 * vp.y).abs() <= 1
            );

            // Reference aspect preserved (within one pixel of rounding).
            let vp_aspect = vp.width as f32 / vp.height as f32;
            let ref_aspect = UI_REFERENCE_WIDTH as f32 / UI_REFERENCE_HEIGHT as f32;
            assert!(
                (vp_aspect - ref_aspect).abs() < 0.01,
                "{w}x{h} UI aspect distorted: {vp_aspect} vs {ref_aspect}"
            );

            // HUD never becomes microscopic at large resolutions.
            assert!(vp.scale >= 1.0, "{w}x{h} UI scale shrank: {}", vp.scale);
        }
    }

    #[test]
    fn test_ui_viewport_baseline_is_identity() {
        let vp = DrawableSize::new(480, 272).ui_viewport();
        assert_eq!(
            (vp.x, vp.y, vp.width, vp.height),
            (0, 0, 480, 272),
            "PocketCHIP UI layout must be pixel-identical to the original"
        );
        assert_exact(vp.scale, 1.0);
    }

    #[test]
    fn test_hidpi_uses_physical_pixels_not_logical_size() {
        // A 480x272 logical window on a 2x Retina display has a 960x544 drawable.
        let logical = DrawableSize::new(480, 272);
        let physical = DrawableSize::new(960, 544);

        assert_exact(physical.ui_viewport().scale, 2.0);
        assert_eq!(physical.ui_viewport().width, 960);
        assert_eq!(physical.ui_viewport().height, 544);
        assert!((physical.aspect_ratio() - logical.aspect_ratio()).abs() < 1e-6);
    }

    #[test]
    fn test_framebuffer_size_changes_update_scale() {
        let small = DrawableSize::new(480, 272);
        let large = DrawableSize::new(1920, 1080);
        assert_ne!(small, large);
        assert!(large.ui_viewport().scale > small.ui_viewport().scale);
        assert_exact(large.ui_viewport().scale, 1080.0 / 272.0);
    }

    #[test]
    fn test_build_geometry_from_level1() {
        let json = include_str!("../assets/levels/level1.json");
        let level = LevelDef::from_json(json).expect("valid level1 json");
        let mesh = build_level_geometry(&level);
        assert!(mesh.vertex_count != 0);
        assert_eq!(mesh.index_count % 6, 0, "geometry is whole quads");
        assert!(
            mesh.vertex_count < mesh.index_count,
            "indexing must share corners"
        );
        assert!(mesh.batches.floor_batch.count > 0);
        assert!(mesh.batches.ceiling_batch.count > 0);
        assert!(mesh.batches.wall_batch.count > 0);
        assert!(mesh.batches.light_batch.count > 0);

        // Floor/ceiling geometry follows the bounded baked-lighting grid: never
        // per square metre, and flat cells merge, so the emitted count is at
        // most the cell grid and usually below it.
        let expected_cells: i32 = level
            .room_iter()
            .map(|room| {
                i32::try_from(
                    crate::lighting::light_grid_cells(room.width)
                        * crate::lighting::light_grid_cells(room.depth),
                )
                .unwrap_or(i32::MAX)
            })
            .sum();
        assert!(mesh.batches.floor_batch.count <= expected_cells * 6);
        assert!(mesh.batches.ceiling_batch.count <= expected_cells * 6);
        assert!(
            mesh.batches.floor_batch.count < expected_cells * 6,
            "level 1's large rooms must merge uniform lighting cells"
        );

        // The whole shipped level stays a few tens of thousands of vertices.
        // (Per-metre tessellation of its 25 large rooms would be ~800,000.)
        assert!(
            mesh.vertex_count < 100_000,
            "level1 unexpectedly large: {} vertices",
            mesh.vertex_count
        );
    }

    /// Builds a compact test level: one 10x10 m room, one 10 x 0.4 m wall
    /// spanning the full ceiling height, plus the supplied openings/props.
    fn level_with_wall(openings_json: &str, props_json: &str) -> LevelDef {
        level_with_wall_and_lights(openings_json, props_json, "[]")
    }

    /// As [`level_with_wall`], with explicit ceiling fixtures.
    fn level_with_wall_and_lights(
        openings_json: &str,
        props_json: &str,
        lights_json: &str,
    ) -> LevelDef {
        let json = format!(
            r#"{{
                "format_version": 1,
                "id": "geometry_test",
                "name": "Geometry Test",
                "spawn": {{ "x": 0.0, "z": 0.0 }},
                "room": {{ "x": -5.0, "z": -5.0, "width": 10.0, "depth": 10.0, "height": 3.5 }},
                "walls": [{{
                    "x": -5.0, "z": 0.0, "width": 10.0, "depth": 0.4, "height": 3.5,
                    "openings": {openings_json}
                }}],
                "ceiling_lights": {lights_json},
                "props": {props_json}
            }}"#
        );
        LevelDef::from_json(&json).expect("valid json")
    }

    /// A square room with the given ceiling fixtures and nothing else.
    fn lit_room_level(width: f32, depth: f32, height: f32, lights_json: &str) -> LevelDef {
        let json = format!(
            r#"{{
                "format_version": 1,
                "id": "lit_room",
                "name": "Lit Room",
                "spawn": {{ "x": 0.0, "z": 0.0 }},
                "rooms": [{{ "x": 0.0, "z": 0.0, "width": {width}, "depth": {depth}, "height": {height} }}],
                "ceiling_lights": {lights_json}
            }}"#
        );
        LevelDef::from_json(&json).expect("valid lit room json")
    }

    /// Expands a material's aggregate index span back into draw-order vertices.
    ///
    /// The renderer never expands indices; tests inspect geometry in draw order,
    /// which is what the pre-indexing vertex buffer held.
    fn batch_slice(mesh: &LevelMesh, kind: SurfaceKind) -> Vec<Vertex> {
        mesh.triangles_for(kind)
    }

    /// Draw-order vertices of one material id, resolved through the shipped
    /// catalog's logical table (no image decoding).
    fn material_vertices(mesh: &LevelMesh, level: &LevelDef, material_id: &str) -> Vec<Vertex> {
        let table = logical_materials(level);
        let index = table
            .index_of(material_id)
            .unwrap_or_else(|| panic!("{material_id} is not referenced by the level"));
        mesh.triangles_for_material(index)
    }

    // ---------------------------------------------------- material overrides

    /// Axis-aligned X/Z bounds of a vertex run, as `(min_x, max_x, min_z, max_z)`.
    fn xz_bounds(vertices: &[Vertex]) -> (f32, f32, f32, f32) {
        let mut bounds = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
        for vertex in vertices {
            bounds.0 = bounds.0.min(vertex.pos[0]);
            bounds.1 = bounds.1.max(vertex.pos[0]);
            bounds.2 = bounds.2.min(vertex.pos[2]);
            bounds.3 = bounds.3.max(vertex.pos[2]);
        }
        bounds
    }

    #[test]
    fn material_ids_resolve_to_their_own_keys_tiling_and_tint() {
        let level = LevelDef::from_json(
            r#"{
                "format_version": 1,
                "id": "material_keys",
                "name": "Material Keys",
                "spawn": { "x": 1.0, "z": 1.0 },
                "defaults": {
                    "wall": "core:wallpaper_yellow_01",
                    "floor": "core:carpet_beige_01",
                    "ceiling": "core:ceiling_panel_01"
                },
                "rooms": [{ "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0, "height": 3.0,
                            "material": "core:carpet_damp_01",
                            "ceiling_material": "core:ceiling_stained_01" }],
                "walls": [{ "x": 0.0, "z": 0.0, "width": 4.0, "depth": 0.2,
                            "material": "core:wallpaper_stained_01" }]
            }"#,
        )
        .expect("valid level");
        let table = logical_materials(&level);

        let maintained_wall = table.index_of("core:wallpaper_yellow_01").expect("wall");
        let stained_wall = table
            .index_of("core:wallpaper_stained_01")
            .expect("stained wall");
        let damp_floor = table.index_of("core:carpet_damp_01").expect("damp floor");
        let stained_ceiling = table
            .index_of("core:ceiling_stained_01")
            .expect("stained ceiling");
        assert_ne!(maintained_wall, stained_wall);

        let wall = table.entry(maintained_wall).expect("wall entry");
        assert_eq!(wall.tile_metres, 2.0);
        assert_eq!(wall.tint, [0.85, 0.80, 0.42]);
        let floor = table.entry(damp_floor).expect("floor entry");
        assert_eq!(floor.tile_metres, 2.0);
        assert_eq!(floor.tint, [1.0, 1.0, 1.0]);
        let ceiling = table.entry(stained_ceiling).expect("ceiling entry");
        assert_eq!(ceiling.tint, [0.72, 0.72, 0.70]);

        // The key carries the slot the geometry asked for, never the material id.
        assert_eq!(
            SurfaceKey::new(MaterialSlot::Ceiling.kind(), damp_floor),
            SurfaceKey::new(SurfaceKind::Ceiling, damp_floor)
        );
        assert!(SurfaceKey::bare(SurfaceKind::Light).material == MATERIAL_NONE);
    }

    #[test]
    fn room_material_overrides_pick_the_damaged_sheets_for_that_room_only() {
        let level = LevelDef::from_json(
            r#"{
                "format_version": 1,
                "id": "damaged_rooms",
                "name": "Damaged Rooms",
                "spawn": { "x": 1.0, "z": 1.0 },
                "rooms": [
                    { "x": 0.0, "z": 0.0, "width": 6.0, "depth": 6.0, "height": 3.0 },
                    { "x": 6.0, "z": 0.0, "width": 6.0, "depth": 6.0, "height": 3.0,
                      "material": "core:carpet_damp_01",
                      "ceiling_material": "core:ceiling_stained_01" }
                ],
                "ceiling_lights": [
                    { "fixture": "core:fluorescent_panel_01", "x": 3.0, "z": 3.0 },
                    { "fixture": "core:fluorescent_panel_01", "x": 9.0, "z": 3.0 }
                ]
            }"#,
        )
        .expect("valid level");
        let mesh = build_level_geometry(&level);

        let clean_floor = material_vertices(&mesh, &level, "core:carpet_beige_01");
        let damp_floor = material_vertices(&mesh, &level, "core:carpet_damp_01");
        let clean_ceiling = material_vertices(&mesh, &level, "core:ceiling_panel_01");
        let stained_ceiling = material_vertices(&mesh, &level, "core:ceiling_stained_01");
        assert!(!clean_floor.is_empty() && !damp_floor.is_empty());
        assert!(!clean_ceiling.is_empty() && !stained_ceiling.is_empty());

        // Each room keeps its own texture: the damp floor is the second room's
        // rectangle, the clean floor the first's.
        assert_eq!(xz_bounds(&damp_floor), (6.0, 12.0, 0.0, 6.0));
        assert_eq!(xz_bounds(&clean_floor), (0.0, 6.0, 0.0, 6.0));
        assert_eq!(xz_bounds(&stained_ceiling), (6.0, 12.0, 0.0, 6.0));
        assert_eq!(xz_bounds(&clean_ceiling), (0.0, 6.0, 0.0, 6.0));

        // A level that never mentions a damaged id resolves only the
        // maintained materials it authored.
        let clean = lit_room_level(8.0, 8.0, 3.0, "[]");
        let clean_table = logical_materials(&clean);
        assert_eq!(clean_table.len(), 3);
        assert!(clean_table.index_of("core:carpet_damp_01").is_none());
    }

    #[test]
    fn a_floor_patch_keeps_its_exact_edges_without_a_second_slab() {
        let level = LevelDef::from_json(
            r#"{
                "format_version": 1,
                "id": "patchy",
                "name": "Patchy",
                "spawn": { "x": 1.0, "z": 1.0 },
                "rooms": [
                    { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 8.0, "height": 3.0 }
                ],
                "floor_patches": [
                    { "x": 3.0, "z": 2.0, "width": 4.0, "depth": 3.0,
                      "material": "core:carpet_damp_01" }
                ],
                "ceiling_lights": [
                    { "fixture": "core:fluorescent_panel_01", "x": 5.0, "z": 4.0 }
                ]
            }"#,
        )
        .expect("valid level");
        let mesh = build_level_geometry(&level);

        let damp = material_vertices(&mesh, &level, "core:carpet_damp_01");
        let clean = material_vertices(&mesh, &level, "core:carpet_beige_01");
        assert!(!damp.is_empty() && !clean.is_empty(), "both regions emit");
        // The patch's own bounds are exactly the authored rectangle: no
        // half-cell bleed in either direction and no overlapping slab.
        assert_eq!(xz_bounds(&damp), (3.0, 7.0, 2.0, 5.0));
        // The maintained floor tiles the rest of the room, still inside it.
        let (min_x, max_x, min_z, max_z) = xz_bounds(&clean);
        assert_eq!((min_x, max_x), (0.0, 10.0));
        assert_eq!((min_z, max_z), (0.0, 8.0));

        // Patched floors add cut lines, and the estimate still bounds what the
        // builder emits.
        let estimate = level.estimate_geometry();
        assert!(estimate.floor_quads >= 1);
        let floor_quads = (damp.len() + clean.len()) / 6;
        assert!(
            floor_quads as u64 <= estimate.floor_quads,
            "{floor_quads} floor quads exceed the {}-quad estimate",
            estimate.floor_quads
        );
        assert!(
            logical_materials(&level)
                .index_of("core:carpet_damp_01")
                .is_some(),
            "the patch needs damp carpet"
        );
    }

    #[test]
    fn wall_material_and_face_overrides_apply_only_to_the_faces_they_name() {
        let level = LevelDef::from_json(
            r#"{
                "format_version": 1,
                "id": "stained_walls",
                "name": "Stained Walls",
                "spawn": { "x": 4.0, "z": 4.0 },
                "rooms": [
                    { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 3.0 }
                ],
                "walls": [
                    { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 0.3,
                      "material": "core:wallpaper_stained_01" },
                    { "x": 0.0, "z": 7.7, "width": 8.0, "depth": 0.3,
                      "faces": { "south": "core:wallpaper_stained_01" } }
                ],
                "ceiling_lights": [
                    { "fixture": "core:fluorescent_panel_01", "x": 4.0, "z": 4.0 }
                ]
            }"#,
        )
        .expect("valid level");
        let mesh = build_level_geometry(&level);

        // Wall 0 is stained throughout: both length faces use the stained sheet.
        // Wall 1 names only its south face, so it keeps one maintained face.
        let stained = material_vertices(&mesh, &level, "core:wallpaper_stained_01");
        let maintained = material_vertices(&mesh, &level, "core:wallpaper_yellow_01");
        assert!(!stained.is_empty() && !maintained.is_empty());
        let stained_bounds = xz_bounds(&stained);
        assert_exact(stained_bounds.0, 0.0);
        assert!(stained_bounds.1 >= 8.0);
        // The maintained faces belong to the second wall's north side, which
        // faces the room interior.
        let maintained_bounds = xz_bounds(&maintained);
        assert!(maintained_bounds.2 >= 7.7, "{maintained_bounds:?}");
        assert!(maintained_bounds.3 <= 8.0, "{maintained_bounds:?}");
        assert!(
            logical_materials(&level)
                .index_of("core:wallpaper_stained_01")
                .is_some(),
            "the stained wall material is referenced"
        );
    }

    // ------------------------------------------------------- spatial culling

    /// A level with two widely separated clusters of placeholder props, so a
    /// camera at the origin can only ever see one of them at a time. Two
    /// clusters 40 m apart means the 12 m grid cannot merge them into one cell.
    fn two_cluster_level(props_per_cluster: usize) -> LevelDef {
        let mut props: Vec<String> = Vec::new();
        for index in 0..props_per_cluster {
            let offset = (index as f32) * 1.4;
            props.push(format!(
                r#"{{ "model": "core:crate", "x": {}, "z": -20.0, "size": [1.0,1.0,1.0] }}"#,
                offset - 5.0
            ));
            props.push(format!(
                r#"{{ "model": "core:crate", "x": {}, "z": 20.0, "size": [1.0,1.0,1.0] }}"#,
                offset - 5.0
            ));
        }
        let json = format!(
            r#"{{
                "format_version": 1,
                "id": "two_clusters",
                "name": "Two Clusters",
                "spawn": {{ "x": 0.0, "z": 0.0 }},
                "rooms": [
                    {{ "x": -12.0, "z": -28.0, "width": 24.0, "depth": 16.0, "height": 3.0 }},
                    {{ "x": -12.0, "z": 12.0, "width": 24.0, "depth": 16.0, "height": 3.0 }}
                ],
                "ceiling_lights": [
                    {{ "fixture": "core:panel_01", "x": 0.0, "z": -20.0 }},
                    {{ "fixture": "core:panel_01", "x": 0.0, "z": 20.0 }}
                ],
                "props": [{}]
            }}"#,
            props.join(",")
        );
        LevelDef::from_json(&json).expect("valid two-cluster level")
    }

    /// The view-projection `render_scene` builds, so culling tests exercise the
    /// same camera convention the game uses.
    fn scene_frustum(eye: glam::Vec3, yaw_degrees: f32, pitch_degrees: f32) -> Frustum {
        let aspect = 480.0 / 272.0;
        let fov = vertical_fov_for_aspect(60.0, aspect);
        let proj = glam::Mat4::perspective_rh(fov.to_radians(), aspect, 0.1, 100.0);
        let pitch = pitch_degrees.to_radians();
        let yaw = yaw_degrees.to_radians();
        let forward = glam::Vec3::new(
            yaw.sin() * pitch.cos(),
            pitch.sin(),
            -yaw.cos() * pitch.cos(),
        );
        let view = glam::Mat4::look_at_rh(eye, eye + forward, glam::Vec3::Y);
        Frustum::from_view_projection(&(proj * view), DepthRange::ZeroToOne)
    }

    /// Distinct vertices the frustum would submit for a level's static ranges.
    fn visible_static_vertices(mesh: &LevelMesh, frustum: &Frustum) -> usize {
        mesh.ranges
            .iter()
            .filter(|range| frustum.intersects_aabb(&range.bounds))
            .map(|range| range.vertices.len())
            .sum()
    }

    #[test]
    fn every_range_is_indexed_correctly_and_keeps_its_own_vertex_block() {
        let mesh = build_level_geometry(&two_cluster_level(6));
        assert!(mesh.ranges.len() > 1, "the grid must split the level");

        let mut total_indices = 0usize;
        for range in &mesh.ranges {
            assert!(
                !range.indices.is_empty(),
                "empty ranges must not be emitted"
            );
            assert_eq!(range.indices.len() % 6, 0, "ranges are whole quads");
            assert!(
                range.vertices.len() <= crate::spatial::MAX_INDEX_VERTICES,
                "a range must stay addressable with 16-bit indices"
            );
            for index in &range.indices {
                assert!(
                    (*index as usize) < range.vertices.len(),
                    "{:?} index {index} is out of range for {} vertices",
                    range.key.kind,
                    range.vertices.len()
                );
            }
            // Every vertex in the block must be referenced: indexing collapses
            // corners, it never leaves orphans behind.
            let mut used = vec![false; range.vertices.len()];
            for index in &range.indices {
                used[*index as usize] = true;
            }
            assert!(
                used.iter().all(|seen| *seen),
                "{:?} range has unreferenced vertices",
                range.key.kind
            );
            total_indices += range.indices.len();
        }
        assert_eq!(total_indices, mesh.index_count);
    }

    #[test]
    fn indexing_shrinks_static_geometry_without_losing_triangles() {
        // The same level built with the pre-indexing emitter shape would hold
        // six vertices per quad; indexed, it must hold strictly fewer while
        // still generating six indices per quad.
        let mesh = build_level_geometry(&two_cluster_level(6));
        let quads = mesh.index_count / 6;
        assert!(quads > 100, "the fixture needs real geometry");
        assert_eq!(mesh.index_count % 6, 0);
        assert!(
            mesh.vertex_count < quads * 6,
            "indexing must beat the flat triangle list: {} vertices for {quads} quads",
            mesh.vertex_count
        );
        // Every quad is four distinct corners at worst.
        assert!(mesh.vertex_count <= quads * 4);
    }

    #[test]
    fn every_range_bounds_contains_its_own_vertices() {
        let mesh = build_level_geometry(&two_cluster_level(6));
        for range in &mesh.ranges {
            for vertex in &range.vertices {
                for axis in 0..3 {
                    assert!(
                        vertex.pos[axis] >= range.bounds.min[axis] - 1e-3
                            && vertex.pos[axis] <= range.bounds.max[axis] + 1e-3,
                        "{:?} at {:?} escapes its bounds {:?}..{:?}",
                        range.key.kind,
                        vertex.pos,
                        range.bounds.min,
                        range.bounds.max
                    );
                }
            }
        }
    }

    #[test]
    fn a_range_bounds_is_never_empty_and_never_contains_nan() {
        let mesh = build_level_geometry(&two_cluster_level(4));
        for range in &mesh.ranges {
            assert!(!range.bounds.is_empty());
            for axis in 0..3 {
                assert!(range.bounds.min[axis].is_finite());
                assert!(range.bounds.max[axis].is_finite());
            }
        }
    }

    #[test]
    fn the_packer_keeps_every_range_inside_16_bit_indices() {
        let mut packer = MeshPacker::default();
        let vertex = |x: f32| Vertex {
            pos: [x, 0.0, 0.0],
            color: [1.0, 1.0, 1.0, 1.0],
            uv: [0.0, 0.0],
        };
        // Three ranges of 40 000 vertices each cannot share one chunk.
        let mut placements = Vec::new();
        for base in 0..3 {
            let vertices: Vec<Vertex> = (0..40_000)
                .map(|i| vertex((base * 40_000 + i) as f32))
                .collect();
            let indices: Vec<u16> = (0..40_000u16).collect();
            placements.extend(packer.push(&vertices, &indices));
        }
        assert!(
            packer.chunks.len() >= 2,
            "the packer must split before overflowing 16-bit indices"
        );
        for (index, placement) in placements.iter().enumerate() {
            let chunk = &packer.chunks[placement.chunk];
            assert!(chunk.vertices.len() <= crate::spatial::MAX_INDEX_VERTICES);
            let start = usize::try_from(placement.index_start).unwrap_or(0);
            let end = start + usize::try_from(placement.index_count).unwrap_or(0);
            for (offset, value) in chunk.indices[start..end].iter().enumerate() {
                assert_eq!(
                    *value as usize,
                    usize::try_from(placement.vertex_start).unwrap_or(0) + offset,
                    "range {index} indices must be re-based into their chunk"
                );
            }
        }
    }

    #[test]
    fn a_single_range_larger_than_the_index_space_is_split_not_wrapped() {
        // One (model, cell) prop batch can easily hold more than 65 536 vertices:
        // 400 chairs in a single cell are 147 200 vertices. Wrapping the u16
        // indices there would draw garbage, so the packer must split the range.
        let vertex = |x: f32| Vertex {
            pos: [x, 0.0, 0.0],
            color: [1.0, 1.0, 1.0, 1.0],
            uv: [0.0, 0.0],
        };
        // 50 000 distinct vertices with a 2x-long index list: one chunk cannot
        // hold them together with the next range, and the range itself must be
        // re-based correctly as it is split.
        let count = 50_000usize;
        let vertices: Vec<Vertex> = (0..count).map(|i| vertex(i as f32)).collect();
        let mut indices: Vec<u16> = Vec::with_capacity(count * 2);
        for index in 0..count {
            indices.push(u16::try_from(index % count).unwrap_or(u16::MAX));
            indices.push(u16::try_from((index + 1) % count).unwrap_or(u16::MAX));
        }
        // A second range of the same size cannot share the first chunk.
        let mut second: Vec<u16> = Vec::with_capacity(count);
        for index in 0..count {
            second.push(u16::try_from(index % count).unwrap_or(u16::MAX));
        }

        let mut packer = MeshPacker::default();
        let mut placements = packer.push(&vertices, &indices);
        placements.extend(packer.push(&vertices, &second));
        assert!(
            packer.chunks.len() >= 2,
            "two 50 000-vertex ranges cannot share one 16-bit chunk"
        );
        let mut total_indices = 0usize;
        for placement in &placements {
            let chunk = &packer.chunks[placement.chunk];
            assert!(chunk.vertices.len() <= crate::spatial::MAX_INDEX_VERTICES);
            let start = usize::try_from(placement.index_start).unwrap_or(0);
            let end = start + usize::try_from(placement.index_count).unwrap_or(0);
            for index in &chunk.indices[start..end] {
                assert!(
                    (*index as usize) < chunk.vertices.len(),
                    "index {index} escapes chunk {}",
                    placement.chunk
                );
            }
            total_indices += usize::try_from(placement.index_count).unwrap_or(0);
        }
        assert_eq!(
            total_indices,
            indices.len() + second.len(),
            "no index may be lost"
        );
        // Every chunk must stay addressable, which is the property that would
        // break if the split were done by vertex count alone.
        for chunk in &packer.chunks {
            assert!(chunk.vertices.len() <= crate::spatial::MAX_INDEX_VERTICES);
        }
    }

    #[test]
    fn turning_the_camera_away_rejects_an_entire_cluster() {
        let level = two_cluster_level(10);
        let mesh = build_level_geometry(&level);
        let eye = glam::Vec3::new(0.0, 1.6, 0.0);

        // Yaw 0 looks along -Z, yaw 180 along +Z: one cluster each way.
        let toward_far = scene_frustum(eye, 0.0, 0.0);
        let toward_near = scene_frustum(eye, 180.0, 0.0);

        let far_visible = visible_static_vertices(&mesh, &toward_far);
        let near_visible = visible_static_vertices(&mesh, &toward_near);
        let total: usize = mesh.ranges.iter().map(|batch| batch.vertices.len()).sum();

        assert!(
            far_visible < total,
            "looking one way must cull the other cluster ({far_visible} of {total})"
        );
        assert!(
            near_visible < total,
            "the mirrored view must cull the opposite cluster ({near_visible} of {total})"
        );
        // The two views are mirror images, so they must agree closely and
        // together leave a large fraction of the level unsubmitted.
        let ratio = far_visible.min(near_visible) as f32 / total as f32;
        assert!(
            ratio < 0.75,
            "a camera-away view must drop most of the level, kept {ratio:.2}"
        );
    }

    #[test]
    fn looking_straight_up_or_down_still_sees_the_room_shell() {
        // One room, camera standing in the middle of it.
        let level = lit_room_level(12.0, 12.0, 3.0, "[]");
        let mesh = build_level_geometry(&level);
        let eye = glam::Vec3::new(0.0, 1.6, 0.0);

        // Extreme pitch must never cull the floor or the ceiling the camera is
        // standing between. Each extreme must see *more* than a level plank.
        for pitch in [-85.0_f32, 85.0] {
            let frustum = scene_frustum(eye, 0.0, pitch);
            let visible = visible_static_vertices(&mesh, &frustum);
            assert!(
                visible > 0,
                "pitch {pitch} culled the whole level; the camera is inside it"
            );
            // Looking down must see the floor, looking up the ceiling; the
            // opposite surface is genuinely outside a 30-degree half-FOV.
            let expected = if pitch < 0.0 {
                SurfaceKind::Floor
            } else {
                SurfaceKind::Ceiling
            };
            let saw_expected = mesh
                .ranges
                .iter()
                .any(|batch| batch.key.kind == expected && frustum.intersects_aabb(&batch.bounds));
            assert!(
                saw_expected,
                "pitch {pitch} must still see the {expected:?}"
            );
            let saw_opposite = mesh.ranges.iter().any(|batch| {
                batch.key.kind != expected
                    && matches!(batch.key.kind, SurfaceKind::Floor | SurfaceKind::Ceiling)
                    && frustum.intersects_aabb(&batch.bounds)
            });
            assert!(
                !saw_opposite,
                "pitch {pitch} must not see the opposite surface"
            );
        }
    }

    #[test]
    fn a_camera_inside_a_batch_never_culls_it() {
        // Stand inside a deliberately oversized prop box: whatever the camera
        // looks at, the range it is standing in must survive every plane test.
        const EPS: f32 = 1e-3;
        let level = level_with_wall(
            "[]",
            r#"[{ "model": "core:crate", "x": 0.0, "z": 0.0, "size": [4.0, 4.0, 4.0] }]"#,
        );
        let mesh = build_level_geometry(&level);
        let eye = glam::Vec3::new(0.0, 1.6, 0.0);
        // Strictly inside, not merely touching: a wall face passing exactly
        // through the eye is still legitimately behind a camera looking away
        // from it.
        let contains_eye = |batch: &LevelMeshRange| {
            (0..3).all(|axis| {
                batch.bounds.min[axis] + EPS <= eye[axis]
                    && batch.bounds.max[axis] - EPS >= eye[axis]
            })
        };
        let containing: Vec<&LevelMeshRange> =
            mesh.ranges.iter().filter(|b| contains_eye(b)).collect();
        assert!(
            !containing.is_empty(),
            "the camera must stand inside the oversized crate"
        );
        for (index, yaw) in [0.0_f32, 45.0, 90.0, 180.0, 270.0].iter().enumerate() {
            let frustum = scene_frustum(eye, *yaw, 0.0);
            for batch in &containing {
                assert!(
                    frustum.intersects_aabb(&batch.bounds),
                    "yaw {yaw} culled a batch the camera stands inside ({index})"
                );
            }
        }
    }

    #[test]
    fn extreemely_distant_geometry_is_culled_by_the_far_plane() {
        // A room a kilometre away, far outside the 100 m far plane.
        let mesh = build_level_geometry(&two_cluster_level(3));
        let eye = glam::Vec3::new(0.0, 1.6, 0.0);
        let frustum = scene_frustum(eye, 180.0, 0.0);
        // Nothing at ±20 m is beyond 100 m, so this view still sees a cluster;
        // the far-plane behaviour itself is covered by `spatial`'s unit tests.
        assert!(visible_static_vertices(&mesh, &frustum) > 0);
    }

    #[test]
    fn negative_and_extreme_level_coordinates_still_batch_and_cull() {
        // A room in the negative quadrant, far enough away to be its own set of
        // cells but still inside the 100 m far plane, plus a near room. Cell
        // keys are therefore negative and the grid spans a wide extent.
        let json = r#"{
            "format_version": 1,
            "id": "extreme",
            "name": "Extreme",
            "spawn": { "x": 0.0, "z": 0.0 },
            "rooms": [
                { "x": -60.0, "z": -60.0, "width": 20.0, "depth": 20.0, "height": 3.0 },
                { "x": -5.0, "z": -5.0, "width": 10.0, "depth": 10.0, "height": 3.0 }
            ],
            "props": [
                { "model": "core:crate", "x": -50.0, "z": -50.0, "size": [1.0,1.0,1.0] }
            ]
        }"#;
        let level = LevelDef::from_json(json).expect("valid extreme level");
        let mesh = build_level_geometry(&level);
        assert!(mesh.ranges.len() >= 2);
        for batch in &mesh.ranges {
            assert!(!batch.bounds.is_empty());
        }

        // The camera stands in the near room. `forward = (sin yaw, 0, -cos yaw)`,
        // so yaw 315 degrees looks along -X/-Z, straight at the distant room,
        // and yaw 135 looks the other way.
        let eye = glam::Vec3::new(0.0, 1.6, 0.0);
        let toward = scene_frustum(eye, 315.0, 0.0);
        let away = scene_frustum(eye, 135.0, 0.0);
        let away_visible = visible_static_vertices(&mesh, &away);
        let toward_visible = visible_static_vertices(&mesh, &toward);
        assert!(
            toward_visible > away_visible,
            "facing the distant room must submit more than facing away \
             ({toward_visible} vs {away_visible})"
        );
    }

    #[test]
    fn overlapping_rooms_and_sunken_props_keep_every_cell_cullable() {
        // Two rooms deliberately overlap and a prop is deliberately sunk through
        // the floor between them. Neither is corrected: the geometry stays where
        // the level puts it, and every range still carries usable bounds.
        let json = r#"{
            "format_version": 1,
            "id": "overlap",
            "name": "Overlap",
            "spawn": { "x": 0.0, "z": 0.0 },
            "rooms": [
                { "x": -8.0, "z": -8.0, "width": 16.0, "depth": 16.0, "height": 3.0 },
                { "x": -4.0, "z": -4.0, "width": 16.0, "depth": 16.0, "height": 3.2 }
            ],
            "walls": [
                { "x": -4.0, "z": 0.0, "width": 8.0, "depth": 0.4, "height": 3.0 }
            ],
            "props": [
                { "model": "core:crate", "x": 2.0, "y": -0.4, "z": 2.0, "size": [1.0, 1.0, 1.0] },
                { "model": "core:crate", "x": 6.0, "y": 0.0, "z": 6.0, "size": [1.0, 1.0, 1.0] }
            ]
        }"#;
        let level = LevelDef::from_json(json).expect("valid overlapping level");
        let mesh = build_level_geometry(&level);
        assert!(mesh.ranges.len() >= 2);
        for range in &mesh.ranges {
            assert!(!range.bounds.is_empty());
            assert!(range.bounds.min.iter().all(|value| value.is_finite()));
            assert!(range.bounds.max.iter().all(|value| value.is_finite()));
        }

        // The sunk crate keeps its real (below-floor) vertical extent: culling
        // must never assume a prop sits above y = 0.
        let props = mesh
            .ranges
            .iter()
            .filter(|range| range.key.kind == SurfaceKind::PropFallback)
            .collect::<Vec<_>>();
        assert!(
            !props.is_empty(),
            "both crates must emit placeholder geometry"
        );
        assert!(
            props.iter().any(|range| range.bounds.min[1] < -0.2),
            "the sunk crate must keep its negative extent: {:?}",
            props.iter().map(|r| r.bounds.min[1]).collect::<Vec<_>>()
        );
        // The two crates share a cell, so they share one cullable range; the
        // range's bounds must still cover the sunk one.
        let prop_vertices: usize = props.iter().map(|range| range.vertices.len()).sum();
        assert!(prop_vertices >= 2 * 4, "two boxes need real geometry");

        // Both rooms contribute floor, and the camera in one of them sees the
        // other through the overlap rather than losing it to the frustum.
        let eye = glam::Vec3::new(0.0, 1.6, 0.0);
        let frustum = scene_frustum(eye, 180.0, 0.0);
        assert!(visible_static_vertices(&mesh, &frustum) > 0);
    }

    #[test]
    fn the_same_level_always_splits_into_the_same_batches() {
        let level = two_cluster_level(5);
        let first = build_level_geometry(&level);
        let second = build_level_geometry(&level);
        assert_eq!(first.ranges, second.ranges);
        assert_eq!(first.batches, second.batches);
        assert_eq!(first.vertex_count, second.vertex_count);
        for (a, b) in first
            .all_vertices()
            .iter()
            .zip(second.all_vertices().iter())
        {
            assert_exact_array(a.pos, b.pos);
            assert_exact_array(a.color, b.color);
            assert_exact_array(a.uv, b.uv);
        }
    }

    #[test]
    fn props_intersecting_a_wall_keep_their_own_cell_bounds() {
        // A crate deliberately half-buried in a wall: culling must use its real
        // world bounds, so it is never dropped while part of it is on screen.
        let level = level_with_wall(
            "[]",
            r#"[{ "model": "core:crate", "x": 5.0, "z": 0.0, "size": [1.0,1.0,1.0] }]"#,
        );
        let mesh = build_level_geometry(&level);
        let props: Vec<_> = mesh
            .ranges
            .iter()
            .filter(|batch| batch.key.kind == SurfaceKind::PropFallback)
            .collect();
        assert_eq!(props.len(), 1);
        let bounds = props[0].bounds;
        assert!(
            bounds.min[0] <= 4.5 + 1e-3 && bounds.max[0] >= 5.5 - 1e-3,
            "the sunk crate's bounds must cover its real extent: {:?}..{:?}",
            bounds.min,
            bounds.max
        );
    }

    #[test]
    fn a_small_level_stays_a_small_number_of_batches() {
        // The 12 m grid must not shred a single room into many draw calls: the
        // whole point is to keep batching efficient while adding cullability.
        let level = level_with_wall("[]", "[]");
        let mesh = build_level_geometry(&level);
        assert!(
            mesh.ranges.len() <= 8,
            "a single small room produced {} static batches",
            mesh.ranges.len()
        );
    }

    #[test]
    fn the_real_prop_batches_carry_bounds_and_split_by_cell() {
        let catalog = shipped_catalog();
        let mut assets = shipped_assets();
        // Two chairs 40 m apart cannot share a cell, and each batch must carry
        // bounds that contain its own vertices.
        let level = level_with_wall(
            "[]",
            r#"[{ "model": "core:chair", "x": -20.0, "z": 0.0 },
                { "model": "core:chair", "x": 20.0, "z": 0.0 }]"#,
        );
        let (_, batches) = build_level_geometry_with_assets(&level, &catalog, &mut assets);
        assert_eq!(batches.len(), 2, "one batch per (model, cell)");
        for batch in &batches {
            assert!(!batch.bounds.is_empty());
            for vertex in &batch.vertices {
                for axis in 0..3 {
                    assert!(vertex.pos[axis] >= batch.bounds.min[axis] - 1e-3);
                    assert!(vertex.pos[axis] <= batch.bounds.max[axis] + 1e-3);
                }
            }
        }
        // Mirror symmetry: the batches sit either side of the origin.
        let centres: Vec<f32> = batches
            .iter()
            .map(|batch| batch.bounds.centre()[0])
            .collect();
        assert!(
            centres.iter().any(|x| *x < -15.0) && centres.iter().any(|x| *x > 15.0),
            "both clusters must be represented: {centres:?}"
        );
    }

    fn brightest(vertices: &[Vertex]) -> &Vertex {
        vertices
            .iter()
            .max_by(|a, b| a.color[0].partial_cmp(&b.color[0]).unwrap())
            .expect("non-empty vertex slice")
    }

    fn dimmest(vertices: &[Vertex]) -> &Vertex {
        vertices
            .iter()
            .min_by(|a, b| a.color[0].partial_cmp(&b.color[0]).unwrap())
            .expect("non-empty vertex slice")
    }

    #[test]
    fn floors_are_lit_by_the_baseline_and_the_local_fixture_pool() {
        let level = lit_room_level(
            20.0,
            20.0,
            3.0,
            r#"[{ "fixture": "core:fluorescent_panel_01", "x": 10.0, "z": 10.0 }]"#,
        );
        let mesh = build_level_geometry(&level);
        let floor = batch_slice(&mesh, SurfaceKind::Floor);
        assert!(!floor.is_empty());

        let bright = brightest(&floor);
        let dim = dimmest(&floor);
        assert!(
            (bright.pos[0] - 10.0).abs() < 2.5 && (bright.pos[2] - 10.0).abs() < 2.5,
            "the brightest floor vertex must sit under the fixture, got {:?}",
            bright.pos
        );
        assert!(
            bright.color[0] - dim.color[0] > 0.05,
            "the pool must be visible: {} vs {}",
            bright.color[0],
            dim.color[0]
        );
        assert!(
            dim.color[0] >= crate::lighting::AMBIENT_LEVEL - 1e-4,
            "no floor vertex may fall below the minimum ambient, got {}",
            dim.color[0]
        );
        for vertex in floor {
            for channel in vertex.color {
                assert!(channel.is_finite() && (0.0..=1.0).contains(&channel));
            }
        }
    }

    #[test]
    fn wall_faces_vary_with_the_baked_lighting() {
        // A fixture right above the wall's west end: the wall face nearest to it
        // must be brighter than the far end, and long walls are split so the
        // change is gradual rather than one flat quad.
        let level = level_with_wall_and_lights(
            "[]",
            "[]",
            r#"[{ "fixture": "core:fluorescent_panel_01", "x": -4.0, "z": 0.2 }]"#,
        );
        let mesh = build_level_geometry(&level);
        let walls = batch_slice(&mesh, SurfaceKind::Wall);
        let bright = brightest(&walls);
        let dim = dimmest(&walls);
        assert!(
            bright.color[0] - dim.color[0] > 0.05,
            "wall lighting must vary: {} vs {}",
            bright.color[0],
            dim.color[0]
        );
        assert!(
            bright.pos[0] < -2.0,
            "the brightest wall vertex must be near the fixture, got {:?}",
            bright.pos
        );

        // Smooth, not banded: the bottom edge of the wall face carries several
        // distinct brightness levels instead of one flat colour.
        let mut edge: Vec<f32> = walls
            .iter()
            .filter(|v| v.pos[2].abs() < 1e-3 && v.pos[1].abs() < 1e-3)
            .map(|v| (v.color[0] * 1000.0).round() / 1000.0)
            .collect();
        edge.sort_by(|a, b| a.partial_cmp(b).unwrap());
        edge.dedup();
        assert!(
            edge.len() >= 3,
            "expected a gradient along the wall, got {edge:?}"
        );
        assert!(edge[edge.len() - 1] - edge[0] > 0.1);
    }

    #[test]
    fn placeholder_prop_boxes_receive_the_environment_lighting() {
        let level = lit_room_level(
            20.0,
            20.0,
            3.0,
            r#"[{ "fixture": "core:fluorescent_panel_01", "x": 10.0, "z": 10.0 }]"#,
        );
        let mut level = level;
        level.props = vec![
            PropDef {
                model: "core:crate".into(),
                x: 10.0,
                y: 0.0,
                z: 10.0,
                rotation_degrees: 0.0,
                scale: 1.0,
                size: Some([1.0, 1.0, 1.0]),
                solid: false,
            },
            PropDef {
                model: "core:crate".into(),
                x: 1.0,
                y: 0.0,
                z: 1.0,
                rotation_degrees: 0.0,
                scale: 1.0,
                size: Some([1.0, 1.0, 1.0]),
                solid: false,
            },
        ];
        let mesh = build_level_geometry(&level);
        let props = batch_slice(&mesh, SurfaceKind::PropFallback);
        assert_eq!(props.len(), 72, "two Y-rotated boxes");

        let under: Vec<&Vertex> = props.iter().filter(|v| v.pos[0] > 5.0).collect();
        let far: Vec<&Vertex> = props.iter().filter(|v| v.pos[0] <= 5.0).collect();
        assert!(!under.is_empty() && !far.is_empty());
        let mean = |slice: &[&Vertex]| {
            slice.iter().map(|vertex| vertex.color[0]).sum::<f32>() / slice.len() as f32
        };
        assert!(
            mean(&under) > mean(&far) + 0.05,
            "the prop under the fixture must be brighter: {} vs {}",
            mean(&under),
            mean(&far)
        );
        // No prop may be lit as if it were outside the level: even the darkest
        // face of a mid-grey box at minimum ambient stays clearly visible.
        let darkest_possible = crate::lighting::AMBIENT_LEVEL * 0.541 * 0.62;
        for vertex in props {
            assert!(
                vertex.color[0] >= darkest_possible - 1e-4,
                "prop vertex {} is darker than the minimum ambient allows",
                vertex.color[0]
            );
        }
    }

    #[test]
    fn vertically_offset_props_sample_their_true_world_position() {
        let mut level = lit_room_level(
            20.0,
            20.0,
            3.0,
            r#"[{ "fixture": "core:fluorescent_panel_01", "x": 10.0, "z": 10.0 }]"#,
        );
        let base = PropDef {
            model: "core:crate".into(),
            x: 10.0,
            y: 0.0,
            z: 10.0,
            rotation_degrees: 0.0,
            scale: 1.0,
            size: Some([1.0, 1.0, 1.0]),
            solid: false,
        };
        let mut raised = base.clone();
        raised.y = 2.0;
        level.props = vec![base, raised];

        let lighting = crate::lighting::LevelLighting::bake(&level);
        let mesh = build_level_geometry(&level);
        let props = batch_slice(&mesh, SurfaceKind::PropFallback);
        assert_eq!(props.len(), 72);
        let floor_box = &props[..36];
        let raised_box = &props[36..];

        // The box on the floor is 3 m below the panel, the raised one 1 m; every
        // corresponding vertex must carry exactly the ratio of the two samples
        // taken at its own transformed world position.
        let mut brighter_vertices = 0;
        for index in 0..36 {
            let low = floor_box[index].color[0];
            let high = raised_box[index].color[0];
            if high > low + 1e-6 {
                brighter_vertices += 1;
            }
            let low_light = lighting.sample(
                floor_box[index].pos[0],
                floor_box[index].pos[1],
                floor_box[index].pos[2],
            );
            let high_light = lighting.sample(
                raised_box[index].pos[0],
                raised_box[index].pos[1],
                raised_box[index].pos[2],
            );
            for channel in 0..3 {
                let low_channel = floor_box[index].color[channel];
                let high_channel = raised_box[index].color[channel];
                let low_sample = low_light.channel(channel);
                let high_sample = high_light.channel(channel);
                assert!(low_sample > 0.0 && high_sample > 0.0);
                let expected_ratio = high_sample / low_sample;
                assert!(
                    (high_channel / low_channel - expected_ratio).abs() < 1e-3,
                    "vertex {index} channel {channel} ratio {} does not match the \
                     world-space samples {expected_ratio}",
                    high_channel / low_channel
                );
            }
        }
        assert!(
            brighter_vertices > 0,
            "the raised prop must be closer to the light"
        );
    }

    #[test]
    fn real_props_are_lit_per_vertex_and_stay_batched() {
        let catalog = shipped_catalog();
        let mut assets = shipped_assets();
        let lights = r#"[
            { "fixture": "core:fluorescent_panel_01", "x": 0.0, "z": 0.0 },
            { "fixture": "core:fluorescent_panel_01", "x": 8.0, "z": 0.0 }
        ]"#;
        let mut props: Vec<String> = Vec::new();
        for index in 0..10 {
            props.push(format!(
                r#"{{ "model": "core:chair", "x": {}, "z": 0.0 }}"#,
                index as f32
            ));
        }
        let level = level_with_wall_and_lights("[]", &format!("[{}]", props.join(",")), lights);
        let (_, batches, lighting) =
            build_level_geometry_with_assets_and_lighting(&level, &catalog, &mut assets);

        assert_eq!(batches.len(), 1, "ten chairs still cost one draw call");
        let vertices = &batches[0].vertices;
        let min = vertices.iter().map(|v| v.color[0]).fold(f32::MAX, f32::min);
        let max = vertices.iter().map(|v| v.color[0]).fold(f32::MIN, f32::max);
        assert!(
            max - min > 0.05,
            "instances across the room must not be uniformly lit: {min}..{max}"
        );

        // Every vertex carries its model colour multiplied by the bake sampled
        // at its own transformed world position. Instances are concatenated in
        // placement order and each contributes the model's whole vertex array,
        // so the model index wraps once per instance.
        let asset = assets
            .resolve("environment/office/props/models/chair.glb")
            .expect("chair loads");
        let model = &asset.model;
        for (vertex_index, vertex) in vertices.iter().enumerate() {
            // Instances contribute the model's own vertex array in order, so the
            // model's index list is not needed to line a submitted vertex up
            // with the vertex it came from.
            let source = model.vertices[vertex_index % model.vertices.len()];
            let light = lighting.sample(vertex.pos[0], vertex.pos[1], vertex.pos[2]);
            for channel in 0..3 {
                let expected = source.color[channel] * light.channel(channel);
                assert!(
                    (vertex.color[channel] - expected).abs() < 1e-4,
                    "vertex {vertex_index}: channel {channel} baked as {} but expected {} * {}",
                    vertex.color[channel],
                    source.color[channel],
                    light.channel(channel)
                );
            }
        }
        assert_eq!(assets.stats().models_failed, 0);
    }

    #[test]
    fn malformed_geometry_never_reaches_the_vertex_buffer() {
        // A room with non-finite dimensions and fixtures with non-finite
        // coordinates must be skipped, not turned into NaN vertices. The loader
        // rejects such levels, but direct construction must stay safe too.
        let mut level = lit_room_level(
            12.0,
            8.0,
            3.0,
            r#"[{ "fixture": "core:fluorescent_panel_01", "x": 6.0, "z": 4.0 }]"#,
        );
        level.rooms[0].width = f32::NAN;
        level.ceiling_lights[0].x = f32::NAN;
        level.ceiling_lights[0].z = f32::INFINITY;

        let mesh = build_level_geometry(&level);
        assert_eq!(mesh.batches.floor_batch.count, 0);
        assert_eq!(mesh.batches.ceiling_batch.count, 0);
        assert_eq!(mesh.batches.light_batch.count, 0);
        for vertex in mesh.all_vertices() {
            assert!(
                vertex.pos.iter().all(|value| value.is_finite()),
                "non-finite position {:?}",
                vertex.pos
            );
        }
    }

    #[test]
    fn a_room_without_fixtures_stays_visible_and_within_range() {
        let mut level = lit_room_level(12.0, 8.0, 3.0, "[]");
        level.walls = vec![crate::level::WallDef {
            x: -6.0,
            y: 0.0,
            z: 4.0,
            width: 12.0,
            depth: 0.4,
            height: None,
            faces: std::collections::HashMap::default(),
            openings: Vec::new(),
            material: None,
        }];
        let mesh = build_level_geometry(&level);
        let floor = batch_slice(&mesh, SurfaceKind::Floor);
        let ceiling = batch_slice(&mesh, SurfaceKind::Ceiling);
        let walls = batch_slice(&mesh, SurfaceKind::Wall);
        assert!(!floor.is_empty() && !ceiling.is_empty() && !walls.is_empty());

        for vertex in mesh.all_vertices() {
            assert!(
                vertex.color.iter().all(|c| c.is_finite()),
                "non-finite baked colour at {:?}",
                vertex.pos
            );
            assert!(vertex.color.iter().all(|c| (0.0..=1.0).contains(c)));
        }
        // The floor uses an untinted base colour, so the ambient fill shows up
        // directly; wall and ceiling tints are darker by design but stay
        // visible rather than collapsing to pure black.
        assert_exact(floor[0].color[0], crate::lighting::AMBIENT_LEVEL);
        assert!(ceiling[0].color[0] > 0.05);
        assert!(walls[0].color[0] > 0.05);
        // And the unlit room is genuinely dark: no channel may approach the
        // historical 0.55 ambient floor.
        for vertex in mesh.all_vertices() {
            assert!(
                vertex.color[0] < 0.2,
                "an unlit room must stay dark, got {:?} at {:?}",
                vertex.color,
                vertex.pos
            );
        }
    }

    #[test]
    fn test_wall_without_openings_emits_four_faces() {
        let level = level_with_wall("[]", "[]");
        let mesh = build_level_geometry(&level);
        // Two faces parallel to the wall's length, each split into lighting
        // segments, plus two end caps. The wall reaches the ceiling height, so
        // there is no top or bottom face. This test room has no fixtures, so the
        // lighting along each face is flat and the segments merge back into one
        // quad per face.
        let segments =
            i32::try_from(crate::lighting::wall_light_segments(10.0)).unwrap_or(i32::MAX);
        assert_eq!(mesh.batches.wall_batch.count, 4 * 6);
        assert!(4 * 6 <= (2 * segments + 2) * 6);
        assert_eq!(mesh.batches.prop_batch.count, 0);
    }

    #[test]
    fn test_wall_with_doorway_emits_more_wall_quads() {
        let plain = build_level_geometry(&level_with_wall("[]", "[]"));
        let level = level_with_wall(
            r#"[{ "kind": "door", "offset": 4.0, "width": 2.0, "height": 2.1 }]"#,
            "[]",
        );
        let door = build_level_geometry(&level);
        assert!(
            door.batches.wall_batch.count > plain.batches.wall_batch.count,
            "doorway must add jamb and header geometry"
        );
        // Three slices, each split into lighting segments, two faces each; plus
        // the door head underside and four cross-section caps (2 wall ends,
        // 2 door jambs). Flat segments merge, so the bound is an upper limit.
        let mut expected = 0;
        for length in [4.0f32, 2.0, 4.0] {
            expected +=
                2 * i32::try_from(crate::lighting::wall_light_segments(length)).unwrap_or(i32::MAX);
        }
        assert!(door.batches.wall_batch.count <= (expected + 1 + 4) * 6);
        assert!(door.batches.wall_batch.count > 0);
    }

    #[test]
    fn test_wall_with_window_emits_sill_and_header_faces() {
        let level = level_with_wall(
            r#"[{ "kind": "window", "offset": 4.0, "width": 2.0, "height": 1.0, "sill": 1.0 }]"#,
            "[]",
        );
        let mesh = build_level_geometry(&level);
        // Four slices: the full-height wall either side of the window plus the
        // sill and header slices, which add a sill top and a head underside,
        // plus 4 cross-section caps. Flat segments merge, so this is a bound.
        let mut expected = 0;
        for length in [4.0f32, 2.0, 2.0, 4.0] {
            expected +=
                2 * i32::try_from(crate::lighting::wall_light_segments(length)).unwrap_or(i32::MAX);
        }
        assert!(mesh.batches.wall_batch.count <= (expected + 2 + 4) * 6);
        assert!(mesh.batches.wall_batch.count > 0);
    }

    #[test]
    fn test_geometry_without_openings_contains_floor_ceiling_and_wall_batches() {
        let level = level_with_wall("[]", "[]");
        let mesh = build_level_geometry(&level);
        assert!(mesh.batches.floor_batch.count > 0);
        assert!(mesh.batches.ceiling_batch.count > 0);
        assert!(mesh.batches.wall_batch.count > 0);
        assert_eq!(mesh.vertex_count % 6, 0);
    }

    #[test]
    fn test_z_axis_wall_geometry_runs_along_z() {
        let json = r#"{
            "format_version": 1,
            "id": "z_wall",
            "name": "Z Wall",
            "spawn": { "x": 5.0, "z": 5.0 },
            "room": { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.5 },
            "walls": [{
                "x": 4.8, "z": 0.0, "width": 0.4, "depth": 10.0, "height": 3.5,
                "openings": [{ "kind": "door", "offset": 4.0, "width": 2.0, "height": 2.1 }]
            }]
        }"#;
        let level = LevelDef::from_json(json).expect("valid json");
        let mesh = build_level_geometry(&level);

        // Same decomposition as the equivalent X-axis wall: 3 slices, each
        // split into lighting segments, two faces each; plus door head
        // underside and 4 cross-section caps. Flat segments merge, so the bound
        // is an upper limit.
        let mut expected = 0;
        for length in [4.0f32, 2.0, 4.0] {
            expected +=
                2 * i32::try_from(crate::lighting::wall_light_segments(length)).unwrap_or(i32::MAX);
        }
        assert!(mesh.batches.wall_batch.count <= (expected + 1 + 4) * 6);
        assert!(mesh.batches.wall_batch.count > 0);

        let wall_vertices = mesh.triangles_for(SurfaceKind::Wall);
        let (min_x, max_x) = wall_vertices.iter().fold((f32::MAX, f32::MIN), |acc, v| {
            (acc.0.min(v.pos[0]), acc.1.max(v.pos[0]))
        });
        let (min_z, max_z) = wall_vertices.iter().fold((f32::MAX, f32::MIN), |acc, v| {
            (acc.0.min(v.pos[2]), acc.1.max(v.pos[2]))
        });
        // The wall spans the room in Z and only its thickness in X.
        assert!(
            min_z <= 1e-3 && max_z >= 10.0 - 1e-3,
            "z span {min_z}..{max_z}"
        );
        assert!(min_x >= 4.79 && max_x <= 5.21, "x span {min_x}..{max_x}");
    }

    #[test]
    fn test_prop_batch_is_populated_for_one_prop() {
        let level = level_with_wall(
            "[]",
            r#"[{ "model": "core:crate", "x": 1.0, "z": 1.0, "size": [1.0, 1.0, 1.0] }]"#,
        );
        let mesh = build_level_geometry(&level);
        // One Y-rotated box = 6 quads = 36 vertices, drawn after every kind of
        // static geometry: the buffer is laid out floor, ceiling, wall, light,
        // then placeholder props.
        assert_eq!(mesh.batches.prop_batch.count, 36);
        assert!(
            mesh.batches.prop_batch.start
                >= mesh.batches.light_batch.start + mesh.batches.light_batch.count,
            "placeholder props must follow the light batch (props {} lights {}..{})",
            mesh.batches.prop_batch.start,
            mesh.batches.light_batch.start,
            mesh.batches.light_batch.start + mesh.batches.light_batch.count,
        );
        assert_eq!(
            mesh.index_count_for(SurfaceKind::PropFallback),
            usize::try_from(mesh.batches.prop_batch.count.max(0)).unwrap_or(0),
            "the prop aggregate span must match the prop ranges"
        );
    }

    #[test]
    fn test_props_with_invalid_extents_are_skipped() {
        let level = level_with_wall(
            "[]",
            r#"[{ "model": "core:crate", "x": 1.0, "z": 1.0, "size": [0.0, 1.0, 1.0] }]"#,
        );
        let mesh = build_level_geometry(&level);
        assert_eq!(mesh.batches.prop_batch.count, 0);
    }

    #[test]
    fn test_prop_catalog_supplies_size_and_colour() {
        let catalog = crate::loader::PropCatalog::from_json_str(
            r##"{
                "format_version": 1,
                "props": [{
                    "id": "core:test_prop", "name": "Test Prop", "category": "Decorative",
                    "size": [1.0, 2.0, 0.5], "color": "#804020", "solid": false
                }]
            }"##,
        )
        .expect("valid catalog");
        let level = level_with_wall(
            "[]",
            r#"[{ "model": "core:test_prop", "x": 0.5, "z": 0.5, "rotation_degrees": 45.0 }]"#,
        );
        let mesh = build_level_geometry_with_catalog(&level, &catalog);
        assert_eq!(mesh.batches.prop_batch.count, 36);
    }

    /// Catalogue + assets used by the real prop-geometry tests. Reading the
    /// shipped catalogue keeps the tests honest about ids and model paths.
    fn shipped_catalog() -> crate::loader::PropCatalog {
        let catalog = crate::loader::PropCatalog::load_default();
        assert!(
            catalog.contains("core:chair"),
            "shipped catalogue must list core:chair"
        );
        catalog
    }

    /// Documents how much vertex data non-indexed submission duplicates.
    ///
    /// A GLB stores each model once, indexed. The prop batcher expands it to a
    /// flat triangle list because that was the only way to share one buffer
    /// across instances — so the GPU shades `triangles * 3` vertices where the
    /// model only has `vertices.len()` distinct ones. This test measures that
    /// expansion for the shipped pack, which is the baseline the indexed path
    /// has to beat, and fails if a model stops sharing vertices at all (which
    /// would make indexing pointless rather than wrong).
    #[test]
    fn non_indexed_submission_duplicates_prop_vertices() {
        let catalog = shipped_catalog();
        let mut assets = shipped_assets();

        let mut total_unique = 0usize;
        let mut total_submitted = 0usize;
        for entry in catalog.entries() {
            let Some(model_path) = entry.model.as_deref() else {
                continue;
            };
            let asset = assets.resolve(model_path).expect("shipped model loads");
            let model = &asset.model;
            let unique = model.vertices.len();
            let submitted = model.triangles * 3;
            assert!(
                model.triangles > 0 && unique > 0,
                "{}: model has no geometry",
                entry.id
            );
            assert!(
                unique <= submitted,
                "{}: an indexed model cannot have more vertices than a flat list",
                entry.id
            );
            println!(
                "{:<22} {:>5} unique -> {:>5} submitted ({:>4} triangles, {:.0}% of the flat list)",
                entry.id,
                unique,
                submitted,
                model.triangles,
                100.0 * unique as f32 / submitted as f32
            );
            total_unique += unique;
            total_submitted += submitted;
        }

        assert!(total_submitted > 0);
        // The pack shares vertices as authored; if this ever stops being true,
        // the indexed path has nothing to save and the case needs revisiting.
        assert!(
            total_unique < total_submitted,
            "the shipped pack has no vertex sharing at all ({total_unique} unique vs {total_submitted})"
        );
        println!(
            "pack total: {total_unique} unique vs {total_submitted} submitted ({:.0}%)",
            100.0 * total_unique as f32 / total_submitted as f32
        );
    }

    fn shipped_assets() -> crate::props::PropAssets {
        let assets = crate::props::PropAssets::load_default();
        assert!(
            assets.root().is_some(),
            "the assets/ directory must exist for these tests"
        );
        assets
    }

    fn bounds_of(vertices: &[Vertex]) -> ([f32; 3], [f32; 3]) {
        let mut min = vertices[0].pos;
        let mut max = vertices[0].pos;
        for vertex in vertices {
            for axis in 0..3 {
                min[axis] = min[axis].min(vertex.pos[axis]);
                max[axis] = max[axis].max(vertex.pos[axis]);
            }
        }
        (min, max)
    }

    #[test]
    fn real_prop_geometry_replaces_the_placeholder_box() {
        let catalog = shipped_catalog();
        let mut assets = shipped_assets();
        let level = level_with_wall("[]", r#"[{ "model": "core:chair", "x": 3.0, "z": -2.0 }]"#);
        let (mesh, batches) = build_level_geometry_with_assets(&level, &catalog, &mut assets);

        // The box placeholder is gone: the chair renders as real geometry.
        assert_eq!(mesh.batches.prop_batch.count, 0);
        assert_eq!(batches.len(), 1, "one draw batch per distinct model");
        assert_eq!(
            batches[0].model,
            "environment/office/props/models/chair.glb"
        );
        assert!(!batches[0].vertices.is_empty());
        assert!(batches[0].texture.width > 0);
        assert_eq!(batches[0].texture.width, batches[0].texture.height);
        assert!(batches[0].texture.width <= crate::level::MAX_PROP_TEXTURE_SIZE);

        // Placed at (3, 0, -2), resting on the floor: a 0.5 x 0.9 x 0.5 chair.
        let (low, high) = bounds_of(&batches[0].vertices);
        assert!(
            (low[1]).abs() < 0.02,
            "chair must rest on the floor, got {}",
            low[1]
        );
        assert!((high[1] - 0.9).abs() < 0.06, "seat height {}", high[1]);
        assert!(
            low[0] > 2.6 && high[0] < 3.4,
            "x bounds {:?}..{:?}",
            low[0],
            high[0]
        );
        assert!(
            low[2] > -2.4 && high[2] < -1.6,
            "z bounds {:?}..{:?}",
            low[2],
            high[2]
        );
    }

    #[test]
    fn repeated_instances_share_one_batch_and_reuse_the_model() {
        let catalog = shipped_catalog();
        let mut assets = shipped_assets();
        let mut props: Vec<String> = Vec::new();
        for index in 0..10 {
            props.push(format!(
                r#"{{ "model": "core:chair", "x": {}, "z": 0.0 }}"#,
                index as f32
            ));
        }
        let level = level_with_wall("[]", &format!("[{}]", props.join(",")));
        let (_, batches) = build_level_geometry_with_assets(&level, &catalog, &mut assets);

        assert_eq!(batches.len(), 1, "ten chairs are one draw batch");
        let single = {
            let one = level_with_wall("[]", r#"[{ "model": "core:chair", "x": 0.0, "z": 0.0 }]"#);
            let (_, batches) = build_level_geometry_with_assets(&one, &catalog, &mut assets);
            batches[0].vertices.len()
        };
        assert_eq!(
            batches[0].vertices.len(),
            single * 10,
            "each instance contributes its triangles to the shared batch"
        );
        // The decoded model is parsed once and shared by every instance.
        let stats = assets.stats();
        assert_eq!(stats.models_loaded, 1);
        assert_eq!(stats.models_failed, 0);
    }

    #[test]
    fn prop_transforms_follow_position_rotation_scale_and_vertical_offset() {
        let catalog = shipped_catalog();
        let mut assets = shipped_assets();
        // The bed is 1.4 x 0.55 x 2.0 m, so a 90 degree yaw is visible in the
        // bounds; rotation, scale and a negative vertical offset all apply.
        let level = level_with_wall(
            "[]",
            r#"[{ "model": "core:bed", "x": 1.0, "y": -0.1, "z": 4.0, "rotation_degrees": 90.0, "scale": 0.5 }]"#,
        );
        let (_, batches) = build_level_geometry_with_assets(&level, &catalog, &mut assets);
        let (low, high) = bounds_of(&batches[0].vertices);

        // Rotated: 2.0 m deep bed becomes 2.0 m of X extent, at half scale 1.0 m.
        assert!(
            (low[0] - 0.5).abs() < 0.06 && (high[0] - 1.5).abs() < 0.06,
            "rotated x bounds {:?}..{:?}",
            low[0],
            high[0]
        );
        assert!(
            (low[2] - 3.65).abs() < 0.06 && (high[2] - 4.35).abs() < 0.06,
            "rotated z bounds {:?}..{:?}",
            low[2],
            high[2]
        );
        assert!(
            (low[1] + 0.1).abs() < 0.02,
            "vertical offset must sink the prop: base at {}",
            low[1]
        );
        assert!(
            (high[1] - 0.175).abs() < 0.03,
            "half-scale bed top at {}",
            high[1]
        );
    }

    fn shipped_level(name: &str) -> crate::level::LevelDef {
        let path = format!("assets/levels/{name}.json");
        let content = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("{path} must be readable: {error}"));
        crate::level::LevelDef::from_json(&content)
            .unwrap_or_else(|error| panic!("{path} must parse: {error}"))
    }

    #[test]
    fn the_showcase_level_renders_every_core_prop_with_real_geometry() {
        let catalog = shipped_catalog();
        let mut assets = shipped_assets();
        let level = shipped_level("prop_showcase");
        let (mesh, batches) = build_level_geometry_with_assets(&level, &catalog, &mut assets);

        assert_eq!(
            mesh.batches.prop_batch.count, 0,
            "no placeholder boxes expected"
        );

        // The shared showcase fixtures place every catalogue placeable exactly
        // once: the domestic/office map covers the generic and Office props, and
        // the Pool showcase covers the Pool family. Themes organize content;
        // this is the one place a "placed somewhere" check is legitimate.
        let pool_showcase = shipped_level("pool_showcase");
        let mut used: std::collections::HashSet<&str> = std::collections::HashSet::new();
        used.extend(level.props.iter().map(|prop| prop.model.as_str()));
        used.extend(pool_showcase.props.iter().map(|prop| prop.model.as_str()));
        for entry in catalog.entries() {
            assert!(
                used.contains(entry.id.as_str()),
                "the showcase levels must place {}",
                entry.id
            );
        }

        // Every prop this level places renders with real geometry, never a
        // placeholder box, and each model appears exactly once.
        let mut models: Vec<&str> = batches.iter().map(|batch| batch.model.as_str()).collect();
        models.sort_unstable();
        models.dedup();
        let placed: std::collections::HashSet<&str> =
            level.props.iter().map(|prop| prop.model.as_str()).collect();
        assert_eq!(
            models.len(),
            placed.len(),
            "each prop model placed here appears exactly once as real geometry"
        );
        assert_eq!(batches.len(), placed.len());
        assert_eq!(assets.stats().models_failed, 0);

        // The intentional clipping is present in the data, not corrected.
        let sunk = level
            .props
            .iter()
            .find(|prop| prop.model == "core:crate" && prop.y < 0.0)
            .expect("the showcase keeps one crate sunk into the floor");
        assert!(sunk.solid, "the sunk crate still blocks the player");
    }

    #[test]
    fn the_stress_level_batches_repeats_into_one_draw_per_model_and_cell() {
        let catalog = shipped_catalog();
        let mut assets = shipped_assets();
        let level = shipped_level("prop_stress");
        assert!(
            level.props.len() >= 100,
            "the stress level needs a real load"
        );

        let (_, batches) = build_level_geometry_with_assets(&level, &catalog, &mut assets);
        // Every batch now covers one model *inside one spatial cell*, so the
        // frustum can reject a cell's worth of instances. That is still a
        // handful of draws per model, not one per instance.
        let distinct_models: std::collections::HashSet<&str> =
            batches.iter().map(|batch| batch.model.as_str()).collect();
        assert!(
            distinct_models.len() <= 12,
            "{} distinct models, got {}",
            level.props.len(),
            distinct_models.len()
        );
        assert!(
            batches.len() >= distinct_models.len(),
            "each model needs at least one batch"
        );
        assert!(
            batches.len() <= distinct_models.len() * 16,
            "cells must stay coarse: {} batches for {} models",
            batches.len(),
            distinct_models.len()
        );
        for batch in &batches {
            assert!(
                !batch.bounds.is_empty(),
                "every batch needs bounds for the frustum test"
            );
        }
        assert_eq!(assets.stats().models_failed, 0);

        let total_vertices: usize = batches.iter().map(|batch| batch.vertices.len()).sum();
        let total_indices: usize = batches.iter().map(|batch| batch.indices.len()).sum();
        // Cross-check the expansion: every placed instance contributes exactly
        // one copy of its model's distinct vertices and one copy of its index
        // list. The decoded asset is shared, so the cache only holds one copy
        // per model (proving instance reuse).
        let mut expected_vertices = 0usize;
        let mut expected_indices = 0usize;
        for prop in &level.props {
            let entry = catalog.get(&prop.model);
            let path = entry.model.expect("stress props come from the catalogue");
            let model = &assets.resolve(&path).expect("model loads").model;
            expected_vertices += model.vertices.len();
            expected_indices += model.indices.len();
        }
        assert_eq!(total_vertices, expected_vertices);
        assert_eq!(total_indices, expected_indices);
        assert!(
            total_vertices > assets.stats().triangles,
            "repeated instances must cost vertices, not extra decoded models"
        );
        assert!(
            total_vertices <= crate::level::MAX_LEVEL_PROP_VERTICES,
            "the stress level must stay inside the prop vertex budget ({total_vertices} vertices)"
        );

        // Sixty-plus instances of one model share the decoded mesh; they are
        // spread across whatever cells they occupy, never duplicated per cell.
        let chair_vertices: usize = batches
            .iter()
            .filter(|batch| batch.model == "environment/office/props/models/chair.glb")
            .map(|batch| batch.vertices.len())
            .sum();
        assert!(
            chair_vertices > 60 * 100,
            "sixty chairs should expand into a large shared batch set, got {chair_vertices} vertices",
        );
    }

    #[test]
    fn a_broken_model_falls_back_to_the_placeholder_box_without_panicking() {
        let catalog = crate::loader::PropCatalog::from_json_str(
            r##"{
                "format_version": 1,
                "props": [{
                    "id": "core:broken", "name": "Broken", "category": "Other",
                    "size": [0.5, 1.0, 0.5], "color": "#808080",
                    "model": "models/does_not_exist.glb"
                }]
            }"##,
        )
        .expect("valid catalog");
        let mut assets = shipped_assets();
        let level = level_with_wall("[]", r#"[{ "model": "core:broken", "x": 0.0, "z": 0.0 }]"#);
        let (mesh, batches) = build_level_geometry_with_assets(&level, &catalog, &mut assets);

        assert!(batches.is_empty(), "no real geometry for a missing model");
        assert_eq!(
            mesh.batches.prop_batch.count, 36,
            "a missing model must draw its placeholder box"
        );
        assert_eq!(assets.stats().models_failed, 1);
    }

    // ------------------------------------------------------------- decals

    /// A 6x6 room with the given decal JSON and optional extra walls.
    fn level_with_decals(decals_json: &str, walls_json: &str, lights_json: &str) -> LevelDef {
        let json = format!(
            r#"{{
                "format_version": 1,
                "id": "decal_test",
                "name": "Decal Test",
                "spawn": {{ "x": 0.0, "z": 0.0 }},
                "rooms": [{{ "x": 0.0, "z": 0.0, "width": 6.0, "depth": 6.0, "height": 3.0 }}],
                "walls": [{walls_json}],
                "decals": [{decals_json}],
                "ceiling_lights": {lights_json}
            }}"#
        );
        LevelDef::from_json(&json).expect("valid decal level json")
    }

    /// Diagonal of one decal quad's first triangle, as the emitted normal.
    fn quad_normal(vertices: &[Vertex]) -> [f32; 3] {
        let a = glam::Vec3::from(vertices[0].pos);
        let b = glam::Vec3::from(vertices[1].pos);
        let c = glam::Vec3::from(vertices[2].pos);
        (b - a).cross(c - a).normalize().to_array()
    }

    fn normal_matches(actual: [f32; 3], expected: [f32; 3]) -> bool {
        (0..3).all(|axis| (actual[axis] - expected[axis]).abs() < 1e-4)
    }

    /// Samples one texel of the generated atlas through the same coordinate
    /// convention `decal_uv_rect` hands to the shader (the sheet is stored
    /// bottom-up, so the visual top row maps to the higher `v`).
    fn atlas_texel(pixels: &[u8], u: f32, v: f32) -> [u8; 4] {
        let size = DECAL_ATLAS_SIZE;
        let x = ((u * size as f32) as i32).clamp(0, size - 1);
        let visual_y = (((1.0 - v) * size as f32) as i32).clamp(0, size - 1);
        let row = size - 1 - visual_y;
        let index = ((row * size + x) * 4) as usize;
        [
            pixels[index],
            pixels[index + 1],
            pixels[index + 2],
            pixels[index + 3],
        ]
    }

    /// The generated patterns must be drawn in the cell their sheet slot
    /// samples: if the art and `decal_uv_rect` disagree, a level silently shows
    /// a different pattern (hazard stripes rendering as an arrow, or nothing).
    #[test]
    fn generated_decal_atlas_cells_match_their_sheet_slots() {
        let atlas = generate_decal_atlas();
        let size = DECAL_ATLAS_SIZE as usize;
        let cell = |slot: u32| -> Vec<[u8; 4]> {
            let rect = decal_uv_rect(slot);
            let u0 = rect[0][0].min(rect[2][0]);
            let u1 = rect[0][0].max(rect[2][0]);
            let v0 = rect[0][1].min(rect[2][1]);
            let v1 = rect[0][1].max(rect[2][1]);
            let mut out = Vec::with_capacity(size * size);
            for row in 0..size {
                for column in 0..size {
                    let u = u0 + (u1 - u0) * (column as f32 + 0.5) / size as f32;
                    let v = v0 + (v1 - v0) * (row as f32 + 0.5) / size as f32;
                    out.push(atlas_texel(&atlas, u, v));
                }
            }
            out
        };
        let count = |pixels: &[[u8; 4]], predicate: fn(&[u8; 4]) -> bool| -> usize {
            pixels.iter().filter(|texel| predicate(texel)).count()
        };

        let test = cell(decal_material_slot(DECAL_TEST_MATERIAL).expect("slot"));
        assert!(
            count(&test, |texel| texel[3] > 128
                && texel[0] > 180
                && texel[1] > 180
                && texel[2] > 180)
                > 100,
            "the validation marking's own cell must hold its white frame"
        );

        let arrow = cell(decal_material_slot(DECAL_ARROW_MATERIAL).expect("slot"));
        assert!(
            count(&arrow, |texel| texel[1] > 120
                && texel[1] > texel[0] + 30
                && texel[1] > texel[2] + 30)
                > 200,
            "the floor arrow's own cell must hold the green arrow"
        );

        let stripes = cell(decal_material_slot(DECAL_STRIPES_MATERIAL).expect("slot"));
        assert!(
            count(&stripes, |texel| texel[0] > 180
                && texel[1] > 150
                && texel[2] < 100)
                > 1000,
            "the hazard-stripe cell must hold the yellow stripes"
        );
    }

    #[test]
    fn every_decal_material_resolves_to_one_sheet_slot() {
        let mut seen = std::collections::HashSet::new();
        let mut rects = Vec::new();
        for material in DECAL_MATERIALS {
            let slot = decal_material_slot(material).expect("known decal material");
            assert!(seen.insert(slot), "{material} shares slot {slot}");
            let rect = decal_uv_rect(slot);
            for uv in rect {
                assert!(
                    (0.0..=1.0).contains(&uv[0]) && (0.0..=1.0).contains(&uv[1]),
                    "{material} samples outside the sheet: {uv:?}"
                );
            }
            rects.push(rect);
        }
        assert_eq!(decal_material_slot("core:not_a_decal"), None);
    }

    #[test]
    fn a_catalogued_png_decal_draws_from_its_own_sheet() {
        // The final Pool sign is external artwork: the catalog declares it as a
        // file-backed PNG, the mesh builder gives it a sheet past the generated
        // atlas slots, and the quad samples the whole sheet.
        let catalog = shipped_catalog();
        let level = level_with_decals(
            r#"{ "x": 3.0, "y": 0.0, "z": 3.0, "width": 0.9, "height": 0.9,
                 "material": "core:decal_no_diving_01", "surface": "floor" }"#,
            r#"{ "x": 0.0, "z": 0.0, "width": 6.0, "depth": 0.4, "height": 3.0 }"#,
            "[]",
        );
        assert_eq!(
            decal_external_sheet_ids(&level, catalog.assets()),
            vec!["core:decal_no_diving_01".to_string()],
            "the sign is the level's only external sheet"
        );
        let sheet = decal_sheet_index(&level, catalog.assets(), "core:decal_no_diving_01")
            .expect("the catalogued sign resolves");
        assert_eq!(sheet, DECAL_EXTERNAL_BASE);
        let mesh = build_level_geometry_with_catalog(&level, &catalog);
        assert_eq!(mesh.batches.decal_batch.count, 6, "one decal is one quad");
        let quad = batch_slice(&mesh, SurfaceKind::Decal);
        // Full-sheet UVs: the decal samples the whole PNG (the packed slice does
        // not promise a corner order, so compare the set).
        let mut uvs: Vec<[f32; 2]> = quad.iter().map(|vertex| vertex.uv).collect();
        uvs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        uvs.dedup();
        assert_eq!(
            uvs,
            vec![[0.0, 0.0], [0.0, 1.0], [1.0, 0.0], [1.0, 1.0]],
            "an external decal samples its whole sheet"
        );

        // The sheet index must be stable when the same level also uses a
        // generated pattern, and generated sheets keep the atlas rect.
        let mixed = level_with_decals(
            r#"{ "x": 1.0, "y": 0.0, "z": 1.0, "width": 1.0, "height": 1.0,
                 "material": "core:decal_arrow_01", "surface": "floor" },
               { "x": 3.0, "y": 0.0, "z": 3.0, "width": 0.9, "height": 0.9,
                 "material": "core:decal_no_diving_01", "surface": "floor" }"#,
            r#"{ "x": 0.0, "z": 0.0, "width": 6.0, "depth": 0.4, "height": 3.0 }"#,
            "[]",
        );
        assert_eq!(
            decal_sheet_index(&mixed, catalog.assets(), "core:decal_arrow_01"),
            decal_material_slot("core:decal_arrow_01")
        );
        assert_eq!(
            decal_sheet_index(&mixed, catalog.assets(), "core:decal_no_diving_01"),
            Some(DECAL_EXTERNAL_BASE)
        );
        let mesh = build_level_geometry_with_catalog(&mixed, &catalog);
        assert_eq!(mesh.batches.decal_batch.count, 12, "two decals, two quads");
        // A generated decal that is not catalogued draws nothing, exactly like
        // an unknown material.
        assert_eq!(
            decal_sheet_index(&level, catalog.assets(), "core:not_a_decal"),
            None
        );
    }

    #[test]
    fn a_wall_decal_lies_exactly_on_its_wall_plane_and_faces_the_room() {
        let level = level_with_decals(
            r#"{ "x": 3.0, "y": 1.5, "z": 0.4, "width": 2.0, "height": 1.0,
                 "material": "core:decal_test_01", "surface": "wall_south" }"#,
            r#"{ "x": 0.0, "z": 0.0, "width": 6.0, "depth": 0.4, "height": 3.0 }"#,
            "[]",
        );
        let mesh = build_level_geometry(&level);
        let quad = batch_slice(&mesh, SurfaceKind::Decal);
        assert_eq!(mesh.batches.decal_batch.count, 6, "one decal is one quad");
        // Every corner sits exactly on the authored wall plane, not on a
        // nudged or biased copy of it.
        for vertex in &quad {
            assert_exact_named(vertex.pos[2], 0.4, "wall decal plane");
            assert!(vertex.pos[1] >= 0.99 && vertex.pos[1] <= 2.01);
            assert!(vertex.pos[0] >= 1.99 && vertex.pos[0] <= 4.01);
        }
        assert!(normal_matches(quad_normal(&quad), [0.0, 0.0, 1.0]));
    }

    #[test]
    fn a_floor_decal_stays_flat_and_rotates_in_its_plane() {
        let level = level_with_decals(
            r#"{ "x": 3.0, "y": 0.0, "z": 3.0, "width": 2.0, "height": 1.0,
                 "material": "core:decal_arrow_01", "surface": "floor", "rotation_degrees": 90.0 }"#,
            r#"{ "x": 0.0, "z": 0.0, "width": 6.0, "depth": 0.4, "height": 3.0 }"#,
            "[]",
        );
        let mesh = build_level_geometry(&level);
        let quad = batch_slice(&mesh, SurfaceKind::Decal);
        for vertex in &quad {
            assert_exact_named(vertex.pos[1], 0.0, "floor decal plane");
            // A quarter turn puts the 2 m width along Z and the 1 m height
            // along X, centred on the anchor.
            assert!(vertex.pos[0] >= 2.49 && vertex.pos[0] <= 3.51);
            assert!(vertex.pos[2] >= 1.99 && vertex.pos[2] <= 4.01);
        }
        assert!(normal_matches(quad_normal(&quad), [0.0, 1.0, 0.0]));
    }

    #[test]
    fn decal_rotation_is_a_pure_in_plane_spin() {
        let base = crate::level::DecalDef {
            x: 1.0,
            y: 2.0,
            z: 3.0,
            width: 2.0,
            height: 1.0,
            rotation_degrees: 0.0,
            material: "core:decal_test_01".into(),
            surface: crate::level::DecalSurface::WallSouth,
        };
        let quarter = crate::level::DecalDef {
            rotation_degrees: 90.0,
            ..base.clone()
        };
        let half = crate::level::DecalDef {
            rotation_degrees: 180.0,
            ..base.clone()
        };
        let a = decal_quad_points(&base).expect("finite decal");
        let b = decal_quad_points(&quarter).expect("finite decal");
        let c = decal_quad_points(&half).expect("finite decal");
        // All three keep the centre and the surface plane.
        for corners in [a, b, c] {
            let centre: [f32; 3] = std::array::from_fn(|axis| {
                corners.iter().map(|point| point[axis]).sum::<f32>() / 4.0
            });
            assert_exact_array(centre, [1.0, 2.0, 3.0]);
        }
        // The quarter turn swaps the in-plane extents; the half turn restores
        // them, so the decal stays in its plane and keeps a valid winding.
        let xs = |corners: [[f32; 3]; 4]| {
            corners
                .iter()
                .map(|point| point[0])
                .fold((f32::MAX, f32::MIN), |(lo, hi), x| (lo.min(x), hi.max(x)))
        };
        assert!((xs(a).1 - xs(a).0 - 2.0).abs() < 1e-4);
        assert!((xs(b).1 - xs(b).0 - 1.0).abs() < 1e-4);
        assert!((xs(c).1 - xs(c).0 - 2.0).abs() < 1e-4);
        for corners in [a, b, c] {
            let normal = corners_normal(&corners);
            assert!(
                normal_matches(normal, [0.0, 0.0, 1.0]),
                "rotated decal lost its facing: {normal:?}"
            );
        }
    }

    /// The emitted winding normal of four decal corners.
    fn corners_normal(corners: &[[f32; 3]; 4]) -> [f32; 3] {
        let a = glam::Vec3::from(corners[0]);
        let b = glam::Vec3::from(corners[1]);
        let c = glam::Vec3::from(corners[2]);
        (b - a).cross(c - a).normalize().to_array()
    }

    #[test]
    fn malformed_decals_never_emit_geometry() {
        for decal in [
            crate::level::DecalDef {
                width: f32::NAN,
                ..crate::level::DecalDef {
                    x: 0.0,
                    y: 1.0,
                    z: 0.0,
                    width: 1.0,
                    height: 1.0,
                    rotation_degrees: 0.0,
                    material: DECAL_TEST_MATERIAL.into(),
                    surface: crate::level::DecalSurface::WallSouth,
                }
            },
            crate::level::DecalDef {
                height: 0.0,
                ..crate::level::DecalDef {
                    x: 0.0,
                    y: 1.0,
                    z: 0.0,
                    width: 1.0,
                    height: 0.0,
                    rotation_degrees: 0.0,
                    material: DECAL_TEST_MATERIAL.into(),
                    surface: crate::level::DecalSurface::Floor,
                }
            },
            crate::level::DecalDef {
                rotation_degrees: f32::INFINITY,
                ..crate::level::DecalDef {
                    x: 0.0,
                    y: 1.0,
                    z: 0.0,
                    width: 1.0,
                    height: 1.0,
                    rotation_degrees: f32::INFINITY,
                    material: DECAL_TEST_MATERIAL.into(),
                    surface: crate::level::DecalSurface::Floor,
                }
            },
        ] {
            assert!(decal_quad_points(&decal).is_none());
        }
    }

    /// The external-sheet UV convention is empirical (see
    /// [`decal_uv_rect_full`]); this pins the verified mapping so a future
    /// change cannot silently flip a sign upside down or mirror it.
    #[test]
    fn external_decal_sheets_pin_their_world_orientation() {
        let rect = decal_uv_rect_full();
        assert_eq!(
            rect,
            [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
            "the full-sheet rect that reads upright on a floor and on a wall"
        );
        for uv in rect {
            assert!((0.0..=1.0).contains(&uv[0]) && (0.0..=1.0).contains(&uv[1]));
        }
    }

    #[test]
    fn unknown_decal_materials_are_skipped_without_failing_the_build() {
        let level = level_with_decals(
            r#"{ "x": 3.0, "y": 1.5, "z": 0.0, "width": 1.0, "height": 1.0,
                 "material": "core:decal_from_a_newer_build", "surface": "wall_south" }"#,
            r#"{ "x": 0.0, "z": 0.0, "width": 6.0, "depth": 0.4, "height": 3.0 }"#,
            "[]",
        );
        let mesh = build_level_geometry(&level);
        assert_eq!(
            mesh.batches.decal_batch.count, 0,
            "an unresolved decal sheet must draw nothing"
        );
        assert!(mesh.batches.wall_batch.count > 0, "the level still builds");
    }

    #[test]
    fn decals_are_lit_by_the_rooms_own_baked_light() {
        let warm = level_with_decals(
            r#"{ "x": 3.0, "y": 0.0, "z": 3.0, "width": 2.0, "height": 2.0,
                 "material": "core:decal_arrow_01", "surface": "floor" }"#,
            r#"{ "x": 0.0, "z": 0.0, "width": 6.0, "depth": 0.4, "height": 3.0 }"#,
            r#"[{ "fixture": "core:fluorescent_panel_01", "x": 3.0, "z": 3.0, "brightness": 1.0,
                   "color": [1.0, 0.5, 0.2] }]"#,
        );
        let blue = level_with_decals(
            r#"{ "x": 3.0, "y": 0.0, "z": 3.0, "width": 2.0, "height": 2.0,
                 "material": "core:decal_arrow_01", "surface": "floor" }"#,
            r#"{ "x": 0.0, "z": 0.0, "width": 6.0, "depth": 0.4, "height": 3.0 }"#,
            r#"[{ "fixture": "core:fluorescent_panel_01", "x": 3.0, "z": 3.0, "brightness": 1.0,
                   "color": [0.2, 0.5, 1.0] }]"#,
        );
        let sample = |level: &LevelDef| {
            let mesh = build_level_geometry(level);
            let quad = batch_slice(&mesh, SurfaceKind::Decal);
            assert_eq!(quad.len(), 6, "one decal quad");
            quad.iter()
                .map(|vertex| vertex.color)
                .fold([0.0f32; 3], |mut acc, color| {
                    for channel in 0..3 {
                        acc[channel] += color[channel] / 6.0;
                    }
                    acc
                })
        };
        let warm_color = sample(&warm);
        let blue_color = sample(&blue);
        assert!(
            warm_color[0] > warm_color[2] + 0.1,
            "a warm fixture must warm the decal: {warm_color:?}"
        );
        assert!(
            blue_color[2] > blue_color[0] + 0.1,
            "a blue fixture must cool the decal: {blue_color:?}"
        );
        // The room's ambient floor still applies: a decal is never black.
        for channel in blue_color {
            assert!(channel >= crate::lighting::AMBIENT_LEVEL * 0.5);
        }
    }

    #[test]
    fn the_decal_depth_bias_is_deterministic_and_sub_visible() {
        let (factor, units) = DECAL_POLYGON_OFFSET;
        assert_exact(factor, 0.0);
        assert!(
            (-4.0..0.0).contains(&units),
            "bias must pull decals slightly towards the camera, got {units}"
        );
        assert!(units == -2.0, "the bias is part of the render contract");
        assert!((0.0..1.0).contains(&DECAL_ALPHA_CUTOFF));
        // A constant (factor-free) bias is what keeps a grazing-angle decal
        // stable: the offset does not scale with the depth slope.
        assert_exact(factor, 0.0);
    }

    #[test]
    fn decals_are_the_last_static_kind_and_keep_the_world_families() {
        assert_eq!(SurfaceKind::ALL.last(), Some(&SurfaceKind::Decal));
        assert_eq!(MaterialSlot::Wall.kind(), SurfaceKind::Wall);
        assert_eq!(MaterialSlot::Floor.kind(), SurfaceKind::Floor);
        assert_eq!(MaterialSlot::Ceiling.kind(), SurfaceKind::Ceiling);
        for kind in [SurfaceKind::Floor, SurfaceKind::Ceiling, SurfaceKind::Wall] {
            assert_ne!(kind, SurfaceKind::Decal);
        }
    }

    // ------------------------------------------------- coincident wall overlays

    /// Two coincident walls: a host with the given openings and a shorter
    /// overlay with the given material and openings.
    fn coincident_wall_level(
        host_openings: &str,
        overlay_openings: &str,
        overlay_material: &str,
    ) -> LevelDef {
        let material = if overlay_material.is_empty() {
            String::new()
        } else {
            format!(r#", "material": "{overlay_material}""#)
        };
        let json = format!(
            r#"{{
                "format_version": 1,
                "id": "coincident",
                "name": "Coincident",
                "spawn": {{ "x": 0.0, "z": 0.0 }},
                "rooms": [{{ "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.0 }}],
                "walls": [
                    {{ "x": 0.0, "z": 3.0, "width": 10.0, "depth": 0.4, "height": 3.0,
                       "openings": {host_openings} }},
                    {{ "x": 2.0, "z": 3.0, "width": 4.0, "depth": 0.4, "height": 3.0,
                       "openings": {overlay_openings}{material} }}
                ],
                "ceiling_lights": []
            }}"#
        );
        LevelDef::from_json(&json).expect("valid coincident wall json")
    }

    #[test]
    fn coincident_overlay_walls_become_one_surface_with_material_runs() {
        let level = coincident_wall_level("[]", "[]", "core:wallpaper_stained_01");
        let materials = logical_materials(&level);
        let lookup = MaterialLookup::new(&materials);
        let units = wall_units(&level, &crate::level::LevelSurfaces::new(&level), &lookup);
        let coalesced: Vec<_> = units
            .iter()
            .filter_map(|unit| match unit {
                WallUnit::Coalesced { wall, runs } => Some((wall, runs)),
                WallUnit::Plain(_) => None,
            })
            .collect();
        assert_eq!(coalesced.len(), 1, "the overlay is resolved into the host");
        let (wall, runs) = coalesced[0];
        assert_exact_named(wall.x, 0.0, "coalesced wall start");
        assert_exact_named(wall.width, 10.0, "coalesced wall length");
        assert_eq!(runs.len(), 3, "host/overlay/host material runs");
        assert_eq!(
            runs[0].body,
            lookup.key(MaterialSlot::Wall, "core:wallpaper_yellow_01")
        );
        assert_eq!(
            runs[1].body,
            lookup.key(MaterialSlot::Wall, "core:wallpaper_stained_01")
        );
        assert_exact_named(runs[1].start, 2.0, "stain run start");
        assert_exact_named(runs[1].end, 6.0, "stain run end");

        // The emitted mesh carries the overlay exactly once: a plain host is
        // two quads, and the stained run adds two more, with no duplicate of
        // the host's own faces underneath.
        let mesh = build_level_geometry(&level);
        let maintained = material_vertices(&mesh, &level, "core:wallpaper_yellow_01");
        let stained = material_vertices(&mesh, &level, "core:wallpaper_stained_01");
        let quads = |vertices: &[Vertex]| vertices.len() / 6;
        // The maintained runs (0..2 and 6..10) plus the two end caps; the
        // stained run is emitted once, and no maintained face survives under
        // it.
        assert_eq!(quads(&maintained), 6, "the maintained runs, once each");
        assert_eq!(quads(&stained), 2, "the overlay run, once");
        for vertex in &stained {
            assert!(vertex.pos[0] >= 1.99 && vertex.pos[0] <= 6.01);
        }
        for vertex in &maintained {
            assert!(
                vertex.pos[0] <= 2.001 || vertex.pos[0] >= 5.999,
                "no maintained face may be emitted under the stained run at x={}",
                vertex.pos[0]
            );
        }
    }

    #[test]
    fn an_overlay_only_covers_a_hole_when_it_is_solid_there() {
        // The host has a window inside the overlay's span. The overlay is
        // solid there, so the combined surface has no hole: that is what the
        // duplicate surfaces showed (the opaque overlay covered the window).
        let covered = coincident_wall_level(
            r#"[{ "kind": "window", "offset": 2.5, "width": 1.0, "height": 1.0, "sill": 1.0 }]"#,
            "[]",
            "core:wallpaper_stained_01",
        );
        let covered_materials = logical_materials(&covered);
        let covered_lookup = MaterialLookup::new(&covered_materials);
        let units = wall_units(
            &covered,
            &crate::level::LevelSurfaces::new(&covered),
            &covered_lookup,
        );
        let synthetic = units
            .iter()
            .find_map(|unit| match unit {
                WallUnit::Coalesced { wall, .. } => Some(wall),
                WallUnit::Plain(_) => None,
            })
            .expect("coalesced unit");
        assert!(
            synthetic.openings.is_empty(),
            "an opening covered by every-overlay solid must stay closed"
        );

        // When both walls carry the same door, the combined surface keeps it.
        let shared = coincident_wall_level(
            r#"[{ "kind": "door", "offset": 2.5, "width": 1.0, "height": 2.1, "sill": 0.0 }]"#,
            r#"[{ "kind": "door", "offset": 0.5, "width": 1.0, "height": 2.1, "sill": 0.0 }]"#,
            "core:wallpaper_stained_01",
        );
        let shared_materials = logical_materials(&shared);
        let shared_lookup = MaterialLookup::new(&shared_materials);
        let units = wall_units(
            &shared,
            &crate::level::LevelSurfaces::new(&shared),
            &shared_lookup,
        );
        let synthetic = units
            .iter()
            .find_map(|unit| match unit {
                WallUnit::Coalesced { wall, .. } => Some(wall),
                WallUnit::Plain(_) => None,
            })
            .expect("coalesced unit");
        assert_eq!(
            synthetic.openings.len(),
            1,
            "the shared door survives the merge"
        );
        assert_exact_named(synthetic.openings[0].offset, 2.5, "shared door offset");
    }

    #[test]
    fn an_empty_material_id_emits_a_bare_key_not_an_arbitrary_material() {
        let level = LevelDef::from_json(
            r#"{
                "format_version": 1,
                "id": "empty_materials",
                "name": "Empty Materials",
                "spawn": { "x": 0.0, "z": 0.0 },
                "defaults": { "wall": "", "floor": "", "ceiling": "" },
                "rooms": [{ "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0, "height": 3.0 }]
            }"#,
        )
        .expect("valid level");
        let table = logical_materials(&level);
        assert!(table.is_empty(), "empty ids are not materials");
        let mesh = build_level_geometry(&level);
        assert!(mesh.batches.floor_batch.count > 0);
        assert!(mesh.batches.ceiling_batch.count > 0);
        for range in &mesh.ranges {
            assert!(
                !range.key.has_material(),
                "an empty id must not resolve to an arbitrary material"
            );
        }
    }

    // ------------------------------------------------- vertical geometry (4.0)

    /// Vertical bounds of a vertex run, as `(min_y, max_y)`.
    fn y_bounds(vertices: &[Vertex]) -> (f32, f32) {
        let mut bounds = (f32::MAX, f32::MIN);
        for vertex in vertices {
            bounds.0 = bounds.0.min(vertex.pos[1]);
            bounds.1 = bounds.1.max(vertex.pos[1]);
        }
        bounds
    }

    #[test]
    fn test_elevated_room_shifts_floor_and_ceiling_together() {
        let level = LevelDef::from_json(
            r#"{
                "format_version": 1,
                "id": "elevated",
                "name": "Elevated",
                "spawn": { "x": 4.0, "z": 4.0 },
                "room": { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0,
                          "height": 3.0, "floor_y": 2.0 },
                "ceiling_lights": [
                    { "fixture": "core:fluorescent_panel_01", "x": 4.0, "z": 4.0 }
                ]
            }"#,
        )
        .expect("elevated json");
        let mesh = build_level_geometry(&level);
        let floor = batch_slice(&mesh, SurfaceKind::Floor);
        let ceiling = batch_slice(&mesh, SurfaceKind::Ceiling);
        assert_eq!(y_bounds(&floor), (2.0, 2.0));
        assert_eq!(y_bounds(&ceiling), (5.0, 5.0));
        // The fixture hangs below the real ceiling, not at the world floor.
        let lights = batch_slice(&mesh, SurfaceKind::Light);
        assert!((y_bounds(&lights).1 - (5.0 - 0.01)).abs() < 1e-4);
    }

    #[test]
    fn test_recessed_region_emits_a_lowered_slab_and_real_transition_faces() {
        let level = LevelDef::from_json(
            r#"{
                "format_version": 1,
                "id": "recess",
                "name": "Recess",
                "spawn": { "x": 1.0, "z": 1.0 },
                "room": { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 4.0 },
                "floor_regions": [
                    { "x": 3.0, "z": 3.0, "width": 4.0, "depth": 2.0, "offset_y": -1.2,
                      "material": "core:carpet_damp_01",
                      "edge_material": "core:wallpaper_stained_01" }
                ],
                "ceiling_lights": [
                    { "fixture": "core:fluorescent_panel_01", "x": 5.0, "z": 4.0 }
                ]
            }"#,
        )
        .expect("recess json");
        let mesh = build_level_geometry(&level);

        // The region floor is a real slab at -1.2 m with exact edges.
        let damp = material_vertices(&mesh, &level, "core:carpet_damp_01");
        assert!(!damp.is_empty(), "the region floor is emitted");
        assert_eq!(y_bounds(&damp), (-1.2, -1.2));
        assert_eq!(xz_bounds(&damp), (3.0, 7.0, 3.0, 5.0));

        // The rest of the room keeps the room material at the room floor.
        let clean = material_vertices(&mesh, &level, "core:carpet_beige_01");
        assert_eq!(y_bounds(&clean), (0.0, 0.0));

        // The transition faces are real geometry spanning the drop, drawn with
        // the authored edge material.
        let stained = material_vertices(&mesh, &level, "core:wallpaper_stained_01");
        assert!(
            !stained.is_empty(),
            "the transition faces are real geometry"
        );
        assert_eq!(y_bounds(&stained), (-1.2, 0.0));
        let (min_x, max_x, min_z, max_z) = xz_bounds(&stained);
        assert_eq!((min_x, max_x), (3.0, 7.0));
        assert_eq!((min_z, max_z), (3.0, 5.0));
        // No hole: the wall faces close the depression completely.
        let perimeter = stained
            .iter()
            .filter(|vertex| vertex.pos[1] == -1.2)
            .count();
        assert!(perimeter >= 8, "the pit floor edge is fully skirted");
    }

    #[test]
    fn test_shallow_region_transition_faces_still_render() {
        // A walkable step still gets a visible riser, even though collision
        // deliberately does not make it solid.
        let level = LevelDef::from_json(
            r#"{
                "format_version": 1,
                "id": "step",
                "name": "Step",
                "spawn": { "x": 1.0, "z": 1.0 },
                "room": { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 3.0 },
                "floor_regions": [
                    { "x": 2.0, "z": 2.0, "width": 4.0, "depth": 4.0, "offset_y": -0.3 }
                ]
            }"#,
        )
        .expect("step json");
        let mesh = build_level_geometry(&level);
        let wall = batch_slice(&mesh, SurfaceKind::Wall);
        assert_eq!(y_bounds(&wall), (-0.3, 0.0));
        assert!(
            level.collision_aabbs().is_empty(),
            "no rim for a walkable step"
        );
    }

    #[test]
    fn test_gable_ceiling_is_real_sloped_geometry() {
        let level = LevelDef::from_json(
            r#"{
                "format_version": 1,
                "id": "gable",
                "name": "Gable",
                "spawn": { "x": 4.0, "z": 4.0 },
                "room": { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 3.0,
                          "ceiling": { "kind": "gable", "ridge": "x", "ridge_rise": 2.0 } },
                "ceiling_lights": [
                    { "fixture": "core:fluorescent_panel_01", "x": 4.0, "z": 1.0 },
                    { "fixture": "core:fluorescent_panel_01", "x": 4.0, "z": 4.0 }
                ]
            }"#,
        )
        .expect("gable json");
        let mesh = build_level_geometry(&level);
        let ceiling = batch_slice(&mesh, SurfaceKind::Ceiling);
        assert!(!ceiling.is_empty());
        let (min_y, max_y) = y_bounds(&ceiling);
        assert!((min_y - 3.0).abs() < 1e-4, "eaves at {min_y}");
        assert!((max_y - 5.0).abs() < 1e-4, "ridge at {max_y}");
        // The ridge is a real line of geometry, not a hidden flat plane.
        let ridge_vertices = ceiling
            .iter()
            .filter(|vertex| (vertex.pos[1] - 5.0).abs() < 1e-4)
            .count();
        assert!(ridge_vertices >= 2, "the ridge exists in the mesh");
        // The slopes interpolate: nothing is left at the flat eave plane.
        let sloped = ceiling
            .iter()
            .filter(|vertex| vertex.pos[1] > 3.0 + 1e-4 && vertex.pos[1] < 5.0 - 1e-4)
            .count();
        assert!(
            sloped > 0,
            "the slopes carry vertices between eave and ridge"
        );

        // Fixtures pick the local ceiling: eave fixture low, ridge fixture high.
        let lights = batch_slice(&mesh, SurfaceKind::Light);
        let zs: Vec<f32> = lights.iter().map(|vertex| vertex.pos[2]).collect();
        let eave_light = zs.iter().fold(f32::MAX, |acc, z| acc.min(*z));
        let ridge_light = zs.iter().fold(f32::MIN, |acc, z| acc.max(*z));
        assert!(eave_light < 1.0 && ridge_light > 4.0, "{zs:?}");
        assert!(
            y_bounds(&lights).1 - y_bounds(&lights).0 > 1.4,
            "the two fixtures hang at different heights"
        );
    }

    #[test]
    fn test_gable_end_wall_follows_the_sloped_ceiling() {
        let level = LevelDef::from_json(
            r#"{
                "format_version": 1,
                "id": "gable_walls",
                "name": "Gable Walls",
                "spawn": { "x": 4.0, "z": 4.0 },
                "room": { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 3.0,
                          "ceiling": { "kind": "gable", "ridge": "x", "ridge_rise": 2.0 } },
                "walls": [
                    { "x": 0.0, "z": 0.0, "width": 0.3, "depth": 8.0 },
                    { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 0.3 }
                ],
                "ceiling_lights": [
                    { "fixture": "core:fluorescent_panel_01", "x": 4.0, "z": 4.0 }
                ]
            }"#,
        )
        .expect("gable walls json");
        let mesh = build_level_geometry(&level);
        let walls = batch_slice(&mesh, SurfaceKind::Wall);
        assert!(!walls.is_empty());
        let (min_y, max_y) = y_bounds(&walls);
        // The wall running along Z climbs to the ridge; the eave wall stays at
        // the eave. Together they span eave to ridge with no flat cap.
        assert!((max_y - 5.0).abs() < 1e-4, "max wall Y {max_y}");
        assert!((min_y - 0.0).abs() < 1e-4, "walls stand on the floor");
        assert!(
            walls
                .iter()
                .filter(|vertex| vertex.pos[1] > 4.9)
                .all(|vertex| vertex.pos[1] <= 5.0 + 1e-4)
        );
    }

    #[test]
    fn test_vertical_diagnostic_geometry_has_no_degenerate_or_misoriented_faces() {
        let level = LevelDef::from_json(
            &std::fs::read_to_string("assets/levels/vertical_diagnostic.json")
                .expect("the phase 4 diagnostic ships"),
        )
        .expect("it parses");
        let mesh = build_level_geometry(&level);

        let normal = |a: [f32; 3], b: [f32; 3], c: [f32; 3]| -> [f32; 3] {
            let u = glam::Vec3::from(b) - glam::Vec3::from(a);
            let v = glam::Vec3::from(c) - glam::Vec3::from(a);
            (u.cross(v)).to_array()
        };
        for kind in SurfaceKind::ALL {
            let vertices = batch_slice(&mesh, kind);
            assert_eq!(
                vertices.len() % 3,
                0,
                "{kind:?} must be a whole triangle list"
            );
            for triangle in vertices.as_chunks::<3>().0 {
                let n = normal(triangle[0].pos, triangle[1].pos, triangle[2].pos);
                assert!(
                    n.iter().all(|value| value.is_finite()),
                    "{kind:?} has a non-finite normal"
                );
                assert!(
                    glam::Vec3::from(n).length() > 1e-6,
                    "{kind:?} has a degenerate triangle: {:?} {:?} {:?}",
                    triangle[0].pos,
                    triangle[1].pos,
                    triangle[2].pos
                );
                // Floors are horizontal and face up; ceilings face down (a gable
                // slope is tilted, so only the sign is fixed).
                match kind {
                    SurfaceKind::Floor => {
                        assert!(n[1] > 0.0, "a floor triangle faces down: {n:?}");
                    }
                    SurfaceKind::Ceiling => {
                        assert!(n[1] < 0.0, "a ceiling triangle faces up: {n:?}");
                    }
                    SurfaceKind::Light => {
                        assert!(n[1] < 0.0, "a fixture panel must face down: {n:?}");
                    }
                    _ => {}
                }
            }
        }
    }

    #[test]
    fn test_world_faces_wind_outward() {
        // A wall's two length faces must look out of the wall: -Z on the low
        // thickness side, +Z on the high side, for an X-axis wall in a room.
        let level = level_with_wall("[]", "[]");
        let mesh = build_level_geometry(&level);
        let walls = batch_slice(&mesh, SurfaceKind::Wall);
        let normal = |triangle: &[Vertex]| -> [f32; 3] {
            let u = glam::Vec3::from(triangle[1].pos) - glam::Vec3::from(triangle[0].pos);
            let v = glam::Vec3::from(triangle[2].pos) - glam::Vec3::from(triangle[0].pos);
            u.cross(v).to_array()
        };
        let mut saw_negative_z = false;
        let mut saw_positive_z = false;
        for triangle in walls.as_chunks::<3>().0 {
            let n = normal(triangle);
            // Length faces are vertical; sills, caps and headers may be
            // horizontal, so only the vertical faces are checked here.
            if n[1].abs() < 0.2 {
                if n[2] < 0.0 {
                    saw_negative_z = true;
                } else if n[2] > 0.0 {
                    saw_positive_z = true;
                }
            }
        }
        assert!(
            saw_negative_z && saw_positive_z,
            "the wall's two length faces must look outward"
        );

        // Floors face up, ceilings face down, in both directions of the grid.
        let floor = batch_slice(&mesh, SurfaceKind::Floor);
        let ceiling = batch_slice(&mesh, SurfaceKind::Ceiling);
        for triangle in floor.as_chunks::<3>().0 {
            assert!(normal(triangle)[1] > 0.0);
        }
        for triangle in ceiling.as_chunks::<3>().0 {
            assert!(normal(triangle)[1] < 0.0);
        }
    }

    #[test]
    fn test_walls_follow_the_local_ceiling_when_their_origin_is_not_at_zero() {
        // Regression: the wall emitter passes world coordinates along the length
        // axis, so a wall that does not start at X=0 (or Z=0) must still resolve
        // its ceiling at its own position instead of falling back to the first
        // room in the level.
        let level = LevelDef::from_json(
            r#"{
                "format_version": 1,
                "id": "offset_walls",
                "name": "Offset Walls",
                "spawn": { "x": 25.0, "z": 5.0 },
                "rooms": [
                    { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 4.0 },
                    { "x": 20.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.0,
                      "ceiling": { "kind": "gable", "ridge": "z", "ridge_rise": 2.0 } }
                ],
                "walls": [
                    { "x": 20.3, "z": 9.7, "width": 9.7, "depth": 0.3, "y": 0.0 }
                ],
                "ceiling_lights": [
                    { "fixture": "core:fluorescent_panel_01", "x": 25.0, "z": 5.0 }
                ]
            }"#,
        )
        .expect("offset wall json");
        let mesh = build_level_geometry(&level);
        let walls = batch_slice(&mesh, SurfaceKind::Wall);
        let second_room: Vec<Vertex> = walls
            .iter()
            .copied()
            .filter(|vertex| vertex.pos[0] > 19.0)
            .collect();
        assert!(!second_room.is_empty(), "the gable room's wall is emitted");
        // The wall runs along X at z = 9.85, where the gable room's ceiling is
        // just above its 5.0 m eave: the wall top must reach it, not the first
        // room's 4.0 m ceiling.
        let (min_y, max_y) = y_bounds(&second_room);
        assert!(min_y <= 1e-4, "the wall stands on the floor");
        assert!(
            (4.9..5.2).contains(&max_y),
            "wall top should follow its own room's ceiling, got {max_y}"
        );
    }

    #[test]
    fn test_horizontal_decals_follow_the_real_surface_height() {
        let level = LevelDef::from_json(
            r#"{
                "format_version": 1,
                "id": "elevated_decals",
                "name": "Elevated Decals",
                "spawn": { "x": 4.0, "z": 4.0 },
                "room": { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0,
                          "height": 3.0, "floor_y": 2.0 },
                "decals": [
                    { "x": 4.0, "y": 0.0, "z": 4.0, "width": 1.0, "height": 1.0,
                      "material": "core:decal_test_01", "surface": "floor" },
                    { "x": 2.0, "y": 0.0, "z": 2.0, "width": 1.0, "height": 1.0,
                      "material": "core:decal_test_01", "surface": "ceiling" }
                ]
            }"#,
        )
        .expect("elevated decal json");
        let mesh = build_level_geometry(&level);
        let decals = batch_slice(&mesh, SurfaceKind::Decal);
        assert!(!decals.is_empty());
        // The floor decal sits on the elevated floor; the ceiling decal sits on
        // the real ceiling, at 5.0 m, not at the authored 0.0.
        assert_eq!(y_bounds(&decals), (2.0, 5.0));
    }

    #[test]
    fn test_props_stand_on_the_local_walkable_floor() {
        let level = LevelDef::from_json(
            r#"{
                "format_version": 1,
                "id": "elevated_props",
                "name": "Elevated Props",
                "spawn": { "x": 4.0, "z": 4.0 },
                "room": { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0,
                          "height": 3.0, "floor_y": 2.0 },
                "floor_regions": [
                    { "x": 3.0, "z": 3.0, "width": 2.0, "depth": 2.0, "offset_y": -0.5 }
                ],
                "props": [
                    { "model": "core:crate", "x": 1.0, "y": 0.0, "z": 1.0, "size": [1.0,1.0,1.0] },
                    { "model": "core:crate", "x": 4.0, "y": 0.0, "z": 4.0, "size": [1.0,1.0,1.0] }
                ]
            }"#,
        )
        .expect("elevated prop json");
        let mesh = build_level_geometry(&level);
        let props = batch_slice(&mesh, SurfaceKind::PropFallback);
        assert!(!props.is_empty());
        // The crate on the room floor spans 2.0..3.0; the one in the recess
        // spans 1.5..2.5.
        assert_eq!(y_bounds(&props), (1.5, 3.0));
    }

    #[test]
    fn a_wall_with_no_twin_is_emitted_exactly_as_authored() {
        let level = level_with_wall("[]", "[]");
        let materials = logical_materials(&level);
        let lookup = MaterialLookup::new(&materials);
        let units = wall_units(&level, &crate::level::LevelSurfaces::new(&level), &lookup);
        assert_eq!(units.len(), 1);
        assert!(
            matches!(units[0], WallUnit::Plain(_)),
            "a wall with no coincident twin must not be rewritten"
        );
        let mesh = build_level_geometry(&level);
        assert_eq!(
            batch_slice(&mesh, SurfaceKind::Wall).len() / 6,
            4,
            "two length faces plus two end caps"
        );
    }

    #[test]
    fn the_residential_levels_resolve_their_stain_overlays() {
        for name in [
            "the_residence",
            "quiet_apartments",
            "after_the_leak",
            "rendering_diagnostic",
        ] {
            let level = shipped_level(name);
            let materials = logical_materials(&level);
            let lookup = MaterialLookup::new(&materials);
            let units = wall_units(&level, &crate::level::LevelSurfaces::new(&level), &lookup);
            let coalesced = units
                .iter()
                .filter(|unit| matches!(unit, WallUnit::Coalesced { .. }))
                .count();
            assert!(
                coalesced >= 1,
                "{name}: expected the authored stain overlays to coalesce, got {coalesced}"
            );
            if name != "rendering_diagnostic" {
                assert!(
                    coalesced >= 5,
                    "{name}: expected the authored stain overlays to coalesce, got {coalesced}"
                );
            }
            // Every coalesced unit must cover its whole span with runs, so no
            // face can fall back to the host material at a run boundary.
            for unit in &units {
                if let WallUnit::Coalesced { wall, runs } = unit {
                    assert!(!runs.is_empty());
                    assert_exact_named(runs[0].start, 0.0, "first run starts at the wall origin");
                    assert!(
                        (runs[runs.len() - 1].end - wall.length()).abs() < 1e-3,
                        "{name}: the last material run must end with the wall"
                    );
                    for pair in runs.windows(2) {
                        assert!(
                            (pair[0].end - pair[1].start).abs() < 1e-3,
                            "{name}: material runs must be contiguous"
                        );
                    }
                }
            }
            // And the normal build still succeeds with them.
            let mesh = build_level_geometry(&level);
            assert!(mesh.batches.wall_batch.count > 0);
        }
    }
}
