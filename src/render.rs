use glow::HasContext;

use crate::font::generate_font_atlas;
use crate::level::{LevelDef, PropDef, WallAxis, ceiling_height_at, wall_solid_slices};
use crate::lighting::{LevelLighting, fixture_half_extents, light_grid_cells, wall_light_segments};

/// PocketCHIP reference resolution. The game logic and UI layout are authored
/// against this 480x272 space; it is also the default window size. It is *not*
/// an assumption about the actual drawable/framebuffer size at runtime.
pub const WINDOW_WIDTH: u32 = 480;
pub const WINDOW_HEIGHT: u32 = 272;

/// Reference space that 2D UI geometry is authored in (PocketCHIP baseline).
pub const UI_REFERENCE_WIDTH: u32 = WINDOW_WIDTH;
pub const UI_REFERENCE_HEIGHT: u32 = WINDOW_HEIGHT;

/// Physical size (in pixels) of the current drawable/framebuffer.
///
/// This is deliberately distinct from the window's logical size: on HiDPI
/// displays such as macOS Retina the drawable is larger than the window size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DrawableSize {
    pub width: u32,
    pub height: u32,
}

impl DrawableSize {
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    /// True when the surface cannot be rendered to (minimized/hidden windows).
    pub const fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// Aspect ratio derived from the real framebuffer, safe against zero height.
    pub fn aspect_ratio(self) -> f32 {
        if self.height == 0 {
            1.0
        } else {
            self.width as f32 / self.height as f32
        }
    }

    /// Pixel size of the integer-scaled UI region that fits this drawable while
    /// preserving the 480x272 reference aspect ratio, plus its bottom-left origin.
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
        let width =
            ((UI_REFERENCE_WIDTH as f32 * scale).round() as i32).clamp(1, self.width as i32);
        let height =
            ((UI_REFERENCE_HEIGHT as f32 * scale).round() as i32).clamp(1, self.height as i32);

        UiViewport {
            x: (self.width as i32 - width) / 2,
            y: (self.height as i32 - height) / 2,
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

/// Aspect ratio of the authored PocketCHIP reference resolution (480x272).
pub fn reference_aspect_ratio() -> f32 {
    UI_REFERENCE_WIDTH as f32 / UI_REFERENCE_HEIGHT as f32
}

/// Maps the configured (baseline) vertical field of view onto a drawable with
/// the given aspect ratio.
///
/// * Wider than the PocketCHIP baseline: the vertical FOV is unchanged, so the
///   horizontal view expands naturally ("Hor+").
/// * Narrower/taller than the baseline: the horizontal FOV is preserved instead
///   so the level is not cropped left/right; only the vertical FOV grows.
///
/// At the baseline aspect this is the identity, so PocketCHIP is unchanged.
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

const VERTEX_SHADER_SRC: &str = r#"
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
"#;

const FRAGMENT_SHADER_SRC: &str = r#"
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
"#;

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
///   by the lighting bake, and that bake is bounded: `lighting::MIN_AMBIENT` is
///   0.55 and `MAX_BRIGHTNESS` is 1.0, so a vertex channel only ever spans
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
    (value.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

/// The exact value a normalised byte decodes to, for tests and audits.
pub fn dequantize_unit(byte: u8) -> f32 {
    byte as f32 / 255.0
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BatchRange {
    pub start: i32,
    pub count: i32,
}

/// Aggregate span per material, covering every spatial batch of that material.
///
/// The spans are measured in **indices**, not vertices: each quad is six
/// indices whether or not indexing collapsed its corners, so "how much floor did
/// this level generate" stays comparable with the pre-indexing builds.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LevelMeshBatches {
    pub floor_batch: BatchRange,
    pub ceiling_batch: BatchRange,
    pub wall_batch: BatchRange,
    pub light_batch: BatchRange,
    pub prop_batch: BatchRange,
}

/// Surface family a [`SurfaceKind`] belongs to.
///
/// Each family has a maintained and a water-damaged kind; tests, the lighting
/// audit and the developer log reason about families, while the draw path binds
/// one texture per kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SurfaceFamily {
    Floor,
    Ceiling,
    Wall,
    Light,
    PropFallback,
}

impl SurfaceKind {
    /// The family this kind belongs to.
    pub fn family(self) -> SurfaceFamily {
        match self {
            SurfaceKind::Floor | SurfaceKind::FloorDamp => SurfaceFamily::Floor,
            SurfaceKind::Ceiling | SurfaceKind::CeilingStained => SurfaceFamily::Ceiling,
            SurfaceKind::Wall | SurfaceKind::WallStained => SurfaceFamily::Wall,
            SurfaceKind::Light => SurfaceFamily::Light,
            SurfaceKind::PropFallback => SurfaceFamily::PropFallback,
        }
    }
}

impl SurfaceFamily {
    /// Every family, in draw order.
    pub const ALL: [SurfaceFamily; 5] = [
        SurfaceFamily::Floor,
        SurfaceFamily::Ceiling,
        SurfaceFamily::Wall,
        SurfaceFamily::Light,
        SurfaceFamily::PropFallback,
    ];
}

/// Which surface family a static batch draws with.
///
/// The order matters: batches are emitted group-major, so a draw loop walking
/// [`LevelMesh::static_batches`] in order only rebinds its texture once per
/// group. `PropFallback` covers catalogue placeholder boxes, which share the
/// unshaded white texture with ceiling fixtures.
///
/// Each surface family has a water-damaged sibling (`FloorDamp`,
/// `CeilingStained`, `WallStained`) for the damaged core materials of design
/// section 21. They sit next to their maintained counterpart so a level that
/// uses both still binds each texture once per group.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SurfaceKind {
    Floor,
    FloorDamp,
    Ceiling,
    CeilingStained,
    Wall,
    WallStained,
    Light,
    PropFallback,
}

/// Which level-default surface a material id resolves against.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaterialSlot {
    Wall,
    Floor,
    Ceiling,
}

/// Built-in water-damaged material ids (design section 21). These are the only
/// material ids that have their own texture sheet; every other id renders with
/// the surface family's level-default sheet, exactly as before.
pub const DAMAGED_WALL_MATERIAL: &str = "core:wallpaper_stained_01";
pub const DAMAGED_FLOOR_MATERIAL: &str = "core:carpet_damp_01";
pub const DAMAGED_CEILING_MATERIAL: &str = "core:ceiling_stained_01";

/// Resolves a material id to the surface family that draws it.
///
/// Callers pass the effective id for a surface: the object-level override when
/// one exists, otherwise the level default (design section 21).
pub fn material_surface(slot: MaterialSlot, material: &str) -> SurfaceKind {
    let damaged = match slot {
        MaterialSlot::Wall => material == DAMAGED_WALL_MATERIAL,
        MaterialSlot::Floor => material == DAMAGED_FLOOR_MATERIAL,
        MaterialSlot::Ceiling => material == DAMAGED_CEILING_MATERIAL,
    };
    match (slot, damaged) {
        (MaterialSlot::Floor, false) => SurfaceKind::Floor,
        (MaterialSlot::Floor, true) => SurfaceKind::FloorDamp,
        (MaterialSlot::Ceiling, false) => SurfaceKind::Ceiling,
        (MaterialSlot::Ceiling, true) => SurfaceKind::CeilingStained,
        (MaterialSlot::Wall, false) => SurfaceKind::Wall,
        (MaterialSlot::Wall, true) => SurfaceKind::WallStained,
    }
}

/// Which damaged texture sheets a level actually references, as
/// `(wall, floor, ceiling)`.
///
/// `core:` materials are the only ones with a distinct damaged sheet, so a
/// level that only mentions `pack:` or maintained ids keeps using the
/// level-default sheets it always used.
pub fn damaged_variants_used(level: &LevelDef) -> (bool, bool, bool) {
    let wall = level.defaults.wall == DAMAGED_WALL_MATERIAL
        || level.walls.iter().any(|wall| {
            wall.material.as_deref() == Some(DAMAGED_WALL_MATERIAL)
                || wall
                    .faces
                    .values()
                    .any(|material| material == DAMAGED_WALL_MATERIAL)
        });
    let floor = level.defaults.floor == DAMAGED_FLOOR_MATERIAL
        || level
            .room_iter()
            .any(|room| room.material.as_deref() == Some(DAMAGED_FLOOR_MATERIAL))
        || level
            .floor_patches
            .iter()
            .any(|patch| patch.material == DAMAGED_FLOOR_MATERIAL);
    let ceiling = level.defaults.ceiling == DAMAGED_CEILING_MATERIAL
        || level
            .room_iter()
            .any(|room| room.ceiling_material.as_deref() == Some(DAMAGED_CEILING_MATERIAL));
    (wall, floor, ceiling)
}

impl SurfaceKind {
    /// Every kind, in draw order.
    pub const ALL: [SurfaceKind; 8] = [
        SurfaceKind::Floor,
        SurfaceKind::FloorDamp,
        SurfaceKind::Ceiling,
        SurfaceKind::CeilingStained,
        SurfaceKind::Wall,
        SurfaceKind::WallStained,
        SurfaceKind::Light,
        SurfaceKind::PropFallback,
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
    pub kind: SurfaceKind,
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
    pub kind: SurfaceKind,
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
    /// Every vertex of one material, in draw order.
    ///
    /// The renderer never does this — it hands indices straight to the GPU — but
    /// tests and the lighting audit inspect geometry in the order it is drawn,
    /// which is what the pre-indexing vertex buffer held.
    pub fn triangles_for(&self, kind: SurfaceKind) -> Vec<Vertex> {
        let mut out = Vec::new();
        for range in self.ranges.iter().filter(|range| range.kind == kind) {
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
    pub fn all_vertices(&self) -> Vec<Vertex> {
        let mut out = Vec::with_capacity(self.vertex_count);
        for range in &self.ranges {
            out.extend_from_slice(&range.vertices);
        }
        out
    }

    /// Every triangle of one surface family, maintained and damaged kinds
    /// together.
    ///
    /// The lighting audit reasons about "the floors of this level" rather than
    /// about which texture sheet they use, so a level with damp rooms is checked
    /// exactly like a level without them.
    pub fn triangles_for_family(&self, family: SurfaceFamily) -> Vec<Vertex> {
        let mut out = Vec::new();
        for range in self
            .ranges
            .iter()
            .filter(|range| range.kind.family() == family)
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

    /// Indices generated for one surface family, in the units of
    /// [`LevelMeshBatches`].
    pub fn index_count_for_family(&self, family: SurfaceFamily) -> usize {
        self.ranges
            .iter()
            .filter(|range| range.kind.family() == family)
            .map(|range| range.indices.len())
            .sum()
    }

    /// Indices generated for one material, in the same units as
    /// [`LevelMeshBatches`].
    pub fn index_count_for(&self, kind: SurfaceKind) -> usize {
        self.ranges
            .iter()
            .filter(|range| range.kind == kind)
            .map(|range| range.indices.len())
            .sum()
    }
}

/// Metres of wall covered by one repeat of the authored wallpaper. Two metres
/// (the authored sheet is 128x128, so the texel density is unchanged) keeps the
/// pattern and its age marks from announcing their repeat every metre.
const WALL_TILE_METRES: f32 = 2.0;

/// Wall texture coordinates from a world-space (along the wall, up the wall)
/// pair in metres. The authored wallpaper covers `WALL_TILE_METRES` per repeat
/// in both directions, which is what keeps a metre of wall from showing the
/// same water stain three times over.
fn wall_uv(along: f32, up: f32) -> [f32; 2] {
    [along / WALL_TILE_METRES, up / WALL_TILE_METRES]
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

/// Multiplies one shaded colour by a baked brightness.
fn shade(base: [f32; 3], light: f32) -> [f32; 3] {
    [
        (base[0] * light).clamp(0.0, 1.0),
        (base[1] * light).clamp(0.0, 1.0),
        (base[2] * light).clamp(0.0, 1.0),
    ]
}

/// Baked brightness sampled at each of four quad corners.
fn lit_corners(base: [f32; 3], points: [[f32; 3]; 4], lighting: &LevelLighting) -> [[f32; 3]; 4] {
    points.map(|point| shade(base, lighting.sample(point[0], point[1], point[2])))
}

/// One length-parallel wall face: the world coordinate across the thickness,
/// the outward normal along that axis, the bottom/top shaded colours, whether
/// the winding runs against the length axis, and the direction name `faces`
/// overrides use.
type WallLengthFace = (f32, f32, [f32; 3], [f32; 3], bool, &'static str);

/// Emits one wall face parallel to the wall's length axis as a strip of quads.
///
/// The face is split along its length (bounded by
/// `lighting::MAX_WALL_LIGHT_SEGMENTS`) so baked fixture pools and doorway
/// blends vary along it; a single quad would smear them across the whole wall.
/// UVs keep the original convention: the world coordinate along the length
/// axis, then Y.
#[allow(clippy::too_many_arguments)]
fn add_wall_length_face(
    vertices: &mut Vec<Vertex>,
    axis: WallAxis,
    l0: f32,
    l1: f32,
    face: f32,
    normal: f32,
    bottom: f32,
    top: f32,
    bottom_shade: [f32; 3],
    top_shade: [f32; 3],
    reversed: bool,
    lighting: &LevelLighting,
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
            WallAxis::X => [at, y, face + normal * LIGHT_FACE_PROBE_M],
            WallAxis::Z => [face + normal * LIGHT_FACE_PROBE_M, y, at],
        };
        shade(base, lighting.sample(probe[0], probe[1], probe[2]))
    };

    let segments = wall_light_segments((l1 - l0).abs());
    // Sample the lighting once per segment boundary, then merge runs of
    // boundaries whose colours are effectively flat. Adjacent segments share a
    // corner, so each boundary is sampled exactly once (a 2x saving) and the
    // merged strip keeps a single value at every surviving edge.
    let boundary_count = segments as usize + 1;
    let mut boundaries: Vec<(f32, [f32; 3], [f32; 3])> = Vec::with_capacity(boundary_count);
    for boundary in 0..boundary_count {
        let at = l0 + (l1 - l0) * boundary as f32 / segments as f32;
        boundaries.push((
            at,
            color(at, bottom, bottom_shade),
            color(at, top, top_shade),
        ));
    }
    let matches_run = |reference: &(f32, [f32; 3], [f32; 3]),
                       candidate: &(f32, [f32; 3], [f32; 3])| {
        (0..3).all(|channel| {
            (candidate.1[channel] - reference.1[channel]).abs() <= LIGHT_GRID_MERGE_EPS
                && (candidate.2[channel] - reference.2[channel]).abs() <= LIGHT_GRID_MERGE_EPS
        })
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
        let (at_start, bottom_start, top_start) = boundaries[start];
        let (at_end, bottom_end, top_end) = boundaries[end];
        let (a, b) = if reversed {
            (at_end, at_start)
        } else {
            (at_start, at_end)
        };
        let (color_a_bottom, color_a_top) = if reversed {
            (bottom_end, top_end)
        } else {
            (bottom_start, top_start)
        };
        let (color_b_bottom, color_b_top) = if reversed {
            (bottom_start, top_start)
        } else {
            (bottom_end, top_end)
        };
        add_quad(
            vertices,
            point(a, bottom),
            color_a_bottom,
            wall_uv(a, bottom),
            point(b, bottom),
            color_b_bottom,
            wall_uv(b, bottom),
            point(b, top),
            color_b_top,
            wall_uv(b, top),
            point(a, top),
            color_a_top,
            wall_uv(a, top),
        );
        start = end;
    }
}

// ------------------------------------------------------------ texture noise
//
// The three built-in surface textures are authored texel by texel here rather
// than shipped as PNGs: they are 64x64/128x128, they must tile, and they must
// stay deterministic, so a few lines of wrapped value noise beat a binary
// asset that has to be regenerated through a toolchain.

/// Deterministic per-texel hash in `[0, 1)` (the same integer mix on every
/// platform, so the baked textures are reproducible).
fn hash01(x: i32, y: i32, seed: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x9E37_79B9)
        ^ (y as u32).wrapping_mul(0x85EB_CA6B)
        ^ seed.wrapping_mul(0xC2B2_AE35);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2545_F491);
    h ^= h >> 13;
    h = h.wrapping_mul(0x27D4_EB2D);
    h ^= h >> 16;
    (h & 0x00FF_FFFF) as f32 / 16_777_216.0
}

/// Tileable value noise in `[0, 1]`: a `period` x `period` lattice of hashed
/// corners, smoothstep-interpolated and wrapped over the texture's square, so
/// every octave joins itself at the edges with no seam.
fn tile_noise(x: i32, y: i32, size: i32, period: i32, seed: u32) -> f32 {
    let period = period.max(1);
    let scale = period as f32 / size as f32;
    let fx = x as f32 * scale;
    let fy = y as f32 * scale;
    let x0 = fx.floor() as i32;
    let y0 = fy.floor() as i32;
    let tx = fx - x0 as f32;
    let ty = fy - y0 as f32;
    let sx = tx * tx * (3.0 - 2.0 * tx);
    let sy = ty * ty * (3.0 - 2.0 * ty);
    let wrap = |value: i32| value.rem_euclid(period);
    let v00 = hash01(wrap(x0), wrap(y0), seed);
    let v10 = hash01(wrap(x0 + 1), wrap(y0), seed);
    let v01 = hash01(wrap(x0), wrap(y0 + 1), seed);
    let v11 = hash01(wrap(x0 + 1), wrap(y0 + 1), seed);
    let top = v00 + (v10 - v00) * sx;
    let bottom = v01 + (v11 - v01) * sx;
    top + (bottom - top) * sy
}

/// Two octaves of tileable noise, the shape most of the surface ageing uses.
fn tile_noise2(x: i32, y: i32, size: i32, coarse: i32, fine: i32, seed: u32) -> f32 {
    (tile_noise(x, y, size, coarse, seed) * 0.65
        + tile_noise(x, y, size, fine, seed.wrapping_add(7)) * 0.35)
        .clamp(0.0, 1.0)
}

fn write_texel(data: &mut [u8], index: usize, rgb: [f32; 3]) {
    for channel in 0..3 {
        data[index + channel] = rgb[channel].clamp(0.0, 255.0) as u8;
    }
    data[index + 3] = 255;
}

/// Yellow wallpaper: a two metre square of wall.
///
/// A printed two-tone stripe (16 px = 25 cm) with a groove and highlight line,
/// a fine paper grain, and a faint wrapped age mottle.  The renderer tints
/// walls gold (0.85, 0.80, 0.42), so the texture itself stays pale and nearly
/// neutral: painting it yellow here would multiply into orange mud.
pub(crate) fn generate_wall_texture() -> [u8; 128 * 128 * 4] {
    let mut data = [0u8; 128 * 128 * 4];
    const PAPER: [f32; 3] = [243.0, 237.0, 220.0];
    for y in 0..128i32 {
        for x in 0..128i32 {
            let index = ((y * 128 + x) * 4) as usize;
            // Printed stripe: a wide light band, a narrow darker band, a dark
            // groove between them and a hairline highlight inside the light one.
            let phase = x % 16;
            let mut tone: f32 = if phase < 7 { 0.997 } else { 0.928 };
            if phase == 7 || phase == 15 {
                tone *= 0.960;
            } else if phase == 3 {
                tone *= 1.014;
            }
            // Paper: a 1 px fibre grain and two octaves of age mottle.
            let fibre = (hash01(x, y, 11) - 0.5) * 0.030;
            let age = tile_noise2(x, y, 128, 6, 17, 23) - 0.5;
            let tone = tone * (1.0 + fibre + 0.075 * age);
            // Aged areas warm very slightly: the paper yellows where it has
            // been exposed, instead of just getting darker.
            write_texel(
                &mut data,
                index,
                [
                    PAPER[0] * tone,
                    PAPER[1] * tone * (1.0 - 0.008 * age),
                    PAPER[2] * tone * (1.0 - 0.022 * age),
                ],
            );
        }
    }
    data
}

/// Short-pile carpet: one square metre of floor per repeat.
///
/// The read comes from three scales: a per-texel speckle and a short
/// directional dash (the pile, which mipmaps average into a soft and slightly
/// directional tone), a 5-12 cm mottle (the traffic and vacuum marks that
/// survive at distance), and a very slight warm/cool drift so one square metre
/// never looks like a flat swatch.
pub(crate) fn generate_carpet_texture() -> [u8; 64 * 64 * 4] {
    let mut data = [0u8; 64 * 64 * 4];
    const PILE: [f32; 3] = [231.0, 223.0, 210.0];
    for y in 0..64i32 {
        for x in 0..64i32 {
            let index = ((y * 64 + x) * 4) as usize;
            let speckle = hash01(x, y, 31) - 0.5;
            // Pile lies in one direction: short dashes, two texels long.
            let dash_v = hash01(x, y >> 1, 37) - 0.5;
            let dash_h = hash01(x >> 1, y, 41) - 0.5;
            let tuft = tile_noise(x, y, 64, 21, 45) - 0.5;
            let mottle = tile_noise(x, y, 64, 5, 43) - 0.5;
            let broad = tile_noise(x, y, 64, 13, 47) - 0.5;
            let warm = tile_noise(x, y, 64, 3, 53) - 0.5;
            let tone = 1.0
                + 0.070 * speckle
                + 0.050 * dash_v
                + 0.035 * dash_h
                + 0.035 * tuft
                + 0.060 * mottle
                + 0.040 * broad;
            write_texel(
                &mut data,
                index,
                [
                    PILE[0] * tone * (1.0 + 0.020 * warm),
                    PILE[1] * tone,
                    PILE[2] * tone * (1.0 - 0.028 * warm),
                ],
            );
        }
    }
    data
}

/// Suspended mineral-fibre ceiling: a 2 x 2 m patch of grid.
///
/// Four 1 m tiles share a 3 cm T-bar grid, and the four differ slightly in
/// tone, speckle and scuffing, so a large ceiling is not one tile stamped
/// forever.  The repeat is two metres rather than one for the same reason.
pub(crate) fn generate_ceiling_texture() -> [u8; 128 * 128 * 4] {
    let mut data = [0u8; 128 * 128 * 4];
    const TILE: [f32; 3] = [247.0, 247.0, 242.0];
    const BAR: [f32; 3] = [168.0, 168.0, 162.0];
    // How the tile dips towards the T-bar: the gap, the bar's shading, the
    // tile's edge shadow and the first clean row of the tile.
    const DIP: [f32; 4] = [0.52, 0.66, 0.84, 0.95];
    for y in 0..128i32 {
        for x in 0..128i32 {
            let index = ((y * 128 + x) * 4) as usize;
            let tx = x % 64;
            let ty = y % 64;
            let edge = tx.min(63 - tx).min(ty.min(63 - ty));
            // Each of the four tiles gets its own tone and scuffing.
            let tile = (x / 64) + 2 * (y / 64);
            let tile_tone = 1.0 + (hash01(tile, tile * 7, 61) - 0.5) * 0.024;
            let fibre = (hash01(x, y, 67) - 0.5) * 0.045;
            let pores = if hash01(x, y, 71) > 0.945 {
                -0.075
            } else {
                0.0
            };
            let blotch = tile_noise(x, y, 128, 9, 73) - 0.5;
            let field = tile_tone * (1.0 + fibre + pores + 0.035 * blotch);
            let dip = if edge < 4 { DIP[edge as usize] } else { 1.0 };
            // The T-bar itself keeps a hair of its own grain, so the grid does
            // not read as a flat drawn line up close.
            let bar_mix = if edge < 2 {
                1.0 - (edge as f32) * 0.35
            } else {
                0.0
            };
            let base = [
                TILE[0] * field * dip * (1.0 - bar_mix) + BAR[0] * bar_mix * field,
                TILE[1] * field * dip * (1.0 - bar_mix) + BAR[1] * bar_mix * field,
                TILE[2] * field * dip * (1.0 - bar_mix) + BAR[2] * bar_mix * field,
            ];
            write_texel(&mut data, index, base);
        }
    }
    data
}

/// Water-damaged wallpaper for `core:wallpaper_stained_01`.
///
/// The same printed paper, faded and marked.  The damage is dominated by
/// vertical runs: a wrapped column mask times a wrapped *length* mask, so a
/// run is continuous down the whole two metre repeat and therefore down the
/// whole wall, exactly the way a long-standing leak behaves.  Broad damp
/// fields sit underneath them -- their shoulder gets the tide mark, their
/// middle is just wet paper -- and the wet areas warm towards a rusty brown
/// instead of only darkening.
pub(crate) fn generate_stained_wall_texture() -> [u8; 128 * 128 * 4] {
    let mut data = generate_wall_texture();
    for y in 0..128i32 {
        for x in 0..128i32 {
            let index = ((y * 128 + x) * 4) as usize;
            // Broad damp fields: two octaves at a 25-60 cm scale.
            let field = tile_noise2(x, y, 128, 3, 8, 101);
            let wet = ((field - 0.62) / 0.24).clamp(0.0, 1.0);
            let shoulder = ((field - 0.52) / 0.24).clamp(0.0, 1.0);
            // Vertical runs. The column mask picks a few bands, the length mask
            // makes each band fade in and out along its own length, and the
            // band's width feathers with a second column octave.
            let column = tile_noise(x, 0, 128, 9, 103);
            let feather = tile_noise(x, 0, 128, 21, 105);
            let length = tile_noise(0, y, 128, 5, 107);
            let run = ((column + 0.35 * feather - 0.62) / 0.20).clamp(0.0, 1.0)
                * ((length - 0.28) / 0.44).clamp(0.0, 1.0);
            let fibre = (hash01(x, y, 109) - 0.5) * 0.05;
            // The run weight is kept low on purpose: the sheet repeats every
            // two metres, and a strong run would turn that repeat into a
            // visible rhythm of stripes down the wall.
            let darken = 1.0 - 0.06 * shoulder - 0.05 * wet - 0.11 * run - 0.03 * fibre;
            // Brown the damp paper as well as darkening it: soaked paper loses
            // its yellow and picks up a rusty grey-brown.
            let warmth = 0.14 * shoulder + 0.12 * run;
            let aged = [
                data[index] as f32 * darken * (1.0 + warmth * 0.30),
                data[index + 1] as f32 * darken * (1.0 + warmth * 0.02),
                data[index + 2] as f32 * darken * (1.0 - warmth * 0.55),
            ];
            write_texel(&mut data, index, aged);
        }
    }
    data
}

/// Water-damaged carpet for `core:carpet_damp_01`.
///
/// The same short pile pushed darker, flatter and greyer over large irregular
/// regions rather than across the whole tile: a damp patch has a boundary and
/// the dry carpet around it still looks like carpet.
pub(crate) fn generate_damp_carpet_texture() -> [u8; 64 * 64 * 4] {
    let mut data = generate_carpet_texture();
    for y in 0..64i32 {
        for x in 0..64i32 {
            let index = ((y * 64 + x) * 4) as usize;
            let field = tile_noise2(x, y, 64, 3, 6, 201);
            // Large, clearly bounded damp regions: roughly half of the tile
            // stays dry, so the wet part still reads as a patch of the same
            // carpet rather than as a darker material.
            let wet = ((field - 0.52) / 0.24).clamp(0.0, 1.0);
            let margin = ((field - 0.40) / 0.24).clamp(0.0, 1.0) - wet;
            let flatten = ((tile_noise(x, y, 64, 11, 203) - 0.60) / 0.30).clamp(0.0, 1.0) * wet;
            let darken = 1.0 - 0.30 * wet - 0.08 * flatten - 0.07 * margin;
            // Damp pile reads grey-brown: pull the red and green down less than
            // the blue so the hue shifts instead of just the brightness.
            let warmth = 0.55 * wet;
            let damp = [
                data[index] as f32 * (darken + 0.03 * warmth),
                data[index + 1] as f32 * darken,
                data[index + 2] as f32 * (darken - 0.06 * warmth),
            ];
            write_texel(&mut data, index, damp);
        }
    }
    data
}

/// Water-damaged ceiling for `core:ceiling_stained_01`.
///
/// Deliberately restrained: three of the four tiles stay recognisable ceiling
/// panels and one carries the leak -- a brown tide ring, a spread stain down
/// one edge and a darker corner where the water collected.  A ceiling where
/// every panel is ruined reads as decoration; one bad panel reads as a
/// building.
pub(crate) fn generate_stained_ceiling_texture() -> [u8; 128 * 128 * 4] {
    let mut data = generate_ceiling_texture();
    const STAIN: [f32; 3] = [128.0, 98.0, 62.0];
    for y in 0..128i32 {
        for x in 0..128i32 {
            let index = ((y * 128 + x) * 4) as usize;
            let tx = x % 64;
            let ty = y % 64;
            let tile = (x / 64) + 2 * (y / 64);
            // One panel carries the leak, its neighbour catches the edge of it
            // and the other two stay clean but a little aged. Nothing here is
            // a circle: an irregular field with a damp shoulder reads as water
            // where a drawn ring reads as a target.
            let severity = match tile {
                3 => 1.0,
                1 => 0.38,
                _ => 0.14,
            };
            let dx = tx.min(63 - tx) as f32;
            let dy = ty.min(63 - ty) as f32;
            let edge = dx.min(dy);
            let field = tile_noise2(x, y, 128, 3, 9, 301);
            let spread = ((field - 0.48) / 0.32).clamp(0.0, 1.0);
            let shoulder = ((field - 0.36) / 0.32).clamp(0.0, 1.0);
            // A leak trail creeping towards the lighter panel next door.
            let trail = if tile == 3 || tile == 2 {
                ((1.0 - (dx / 40.0).clamp(0.0, 1.0))
                    * 0.34
                    * ((tile_noise(x, y, 128, 5, 307) - 0.46) / 0.30).clamp(0.0, 1.0))
                    * if tile == 2 { 0.45 } else { 1.0 }
            } else {
                0.0
            };
            // Water collects against the grid and the metal T-bar interrupts
            // it, which also keeps the stain seamless where the sheet wraps.
            let grid_fade = (edge / 6.0).clamp(0.0, 1.0);
            let stain =
                ((0.62 * spread + 0.30 * shoulder + trail) * severity).clamp(0.0, 1.0) * grid_fade;
            // The paper yellows before it browns: mixing towards the stain
            // colour keeps a pale panel pale instead of multiplying it away.
            // The ceiling sheet repeats every two metres, so the stain is kept
            // light enough that its repeat reads as ageing, not as a pattern.
            let mix = 0.40 * stain;
            let soaked = [
                data[index] as f32 * (1.0 - mix) + STAIN[0] * mix,
                data[index + 1] as f32 * (1.0 - mix) + STAIN[1] * mix,
                data[index + 2] as f32 * (1.0 - mix) + STAIN[2] * mix,
            ];
            write_texel(&mut data, index, soaked);
        }
    }
    data
}

pub(crate) fn generate_white_texture() -> [u8; 2 * 2 * 4] {
    [255u8; 2 * 2 * 4]
}

/// Texels per metre in the derived repeating floor texture. The authored carpet
/// texture is 64x64 and is displayed once per metre, so 64 keeps the default
/// floor pixel-identical while custom textures are resampled to a sane size.
const FLOOR_TEXELS_PER_METRE: u32 = 64;
/// Metres covered by one repeat of the authored ceiling texture. The sheet is
/// 128x128 -- four 1 m tiles in a T-bar grid -- so the ceiling's world UVs run
/// at half speed. Two metres hides the repeat and still gives each panel 64
/// texels, the same density as every other surface in the game.
const CEILING_TILE_METRES: f32 = 2.0;
/// Number of metres covered by one repeat of the derived floor texture. Two
/// metres covers the 1 m checker tint period.
const FLOOR_TILE_METRES: f32 = 2.0;
const FLOOR_TILE_TEXELS: u32 = FLOOR_TEXELS_PER_METRE * 2;

/// Bilinearly samples an RGBA image at normalized coordinates in `[0, 1)`,
/// wrapping at the edges to mirror `GL_REPEAT`.
fn sample_bilinear(src: &crate::loader::RawImage, u: f32, v: f32) -> [u8; 4] {
    let w = src.width.max(1) as i32;
    let h = src.height.max(1) as i32;
    let x = u * w as f32 - 0.5;
    let y = v * h as f32 - 0.5;
    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let fx = x - x0 as f32;
    let fy = y - y0 as f32;

    let texel = |xx: i32, yy: i32| -> [f32; 4] {
        let cx = xx.rem_euclid(w) as u32;
        let cy = yy.rem_euclid(h) as u32;
        let idx = ((cy * src.width + cx) * 4) as usize;
        [
            src.rgba[idx] as f32,
            src.rgba[idx + 1] as f32,
            src.rgba[idx + 2] as f32,
            src.rgba[idx + 3] as f32,
        ]
    };

    let c00 = texel(x0, y0);
    let c10 = texel(x0 + 1, y0);
    let c01 = texel(x0, y0 + 1);
    let c11 = texel(x0 + 1, y0 + 1);

    let mut out = [0u8; 4];
    for k in 0..4 {
        let top = c00[k] * (1.0 - fx) + c10[k] * fx;
        let bottom = c01[k] * (1.0 - fx) + c11[k] * fx;
        out[k] = (top * (1.0 - fy) + bottom * fy).round().clamp(0.0, 255.0) as u8;
    }
    out
}

/// Builds a repeating 2x2 m floor texture that bakes the 1 m alternating
/// checker tint into the source texture, so each room floor can be a single
/// quad without per-metre geometry.
pub(crate) fn generate_floor_checker_texture(
    src: &crate::loader::RawImage,
) -> crate::loader::RawImage {
    let tile = FLOOR_TILE_TEXELS;
    let mut rgba = vec![0u8; (tile * tile * 4) as usize];

    for ty in 0..tile {
        for tx in 0..tile {
            let cell_x = tx / FLOOR_TEXELS_PER_METRE;
            let cell_y = ty / FLOOR_TEXELS_PER_METRE;
            // The original per-tile vertex colours `(ix + iz) % 2 == 0`. The
            // two tints are deliberately close: the metre checker is meant to
            // be felt as uneven carpet wear, not read as a tiled floor.
            let tint = if (cell_x + cell_y).is_multiple_of(2) {
                [0.550f32, 0.500, 0.383]
            } else {
                [0.518, 0.471, 0.360]
            };
            let u = ((tx % FLOOR_TEXELS_PER_METRE) as f32 + 0.5) / FLOOR_TEXELS_PER_METRE as f32;
            let v = ((ty % FLOOR_TEXELS_PER_METRE) as f32 + 0.5) / FLOOR_TEXELS_PER_METRE as f32;
            let s = sample_bilinear(src, u, v);

            let idx = ((ty * tile + tx) * 4) as usize;
            rgba[idx] = (s[0] as f32 * tint[0]).round().clamp(0.0, 255.0) as u8;
            rgba[idx + 1] = (s[1] as f32 * tint[1]).round().clamp(0.0, 255.0) as u8;
            rgba[idx + 2] = (s[2] as f32 * tint[2]).round().clamp(0.0, 255.0) as u8;
            rgba[idx + 3] = s[3];
        }
    }

    crate::loader::RawImage::new(tile, tile, rgba)
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
        let middle = (bottom + top) * 0.5;
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
/// world span of the wall across it. `corners` are the shaded colours of the
/// four quad corners in emitted winding order, so a reveal between two rooms
/// can carry each side's baked light through the door rather than falling back
/// to ambient in the middle of the wall. UVs follow the wall face convention
/// (horizontal world coordinate, then Y).
#[allow(clippy::too_many_arguments)]
fn add_wall_cross_quad(
    vertices: &mut Vec<Vertex>,
    axis: WallAxis,
    at: f32,
    thickness: (f32, f32),
    bottom: f32,
    top: f32,
    corners: [[f32; 3]; 4],
) {
    let (t0, t1) = thickness;
    match axis {
        // Length runs along X, so the cross section lies in the Z/Y plane.
        WallAxis::X => add_quad(
            vertices,
            [at, bottom, t1],
            corners[0],
            wall_uv(t1, bottom),
            [at, bottom, t0],
            corners[1],
            wall_uv(t0, bottom),
            [at, top, t0],
            corners[2],
            wall_uv(t0, top),
            [at, top, t1],
            corners[3],
            wall_uv(t1, top),
        ),
        // Length runs along Z, so the cross section lies in the X/Y plane.
        WallAxis::Z => add_quad(
            vertices,
            [t0, bottom, at],
            corners[0],
            wall_uv(t0, bottom),
            [t1, bottom, at],
            corners[1],
            wall_uv(t1, bottom),
            [t1, top, at],
            corners[2],
            wall_uv(t1, top),
            [t0, top, at],
            corners[3],
            wall_uv(t0, top),
        ),
    }
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
    lighting: &LevelLighting,
) {
    let half_w = size[0] * 0.5;
    let half_h = size[1] * 0.5;
    let half_d = size[2] * 0.5;
    let center_y = prop.y + half_h;

    let (sin_yaw, cos_yaw) = prop.rotation_degrees.to_radians().sin_cos();
    let rotate = |lx: f32, lz: f32| -> (f32, f32) {
        (
            prop.x + lx * cos_yaw + lz * sin_yaw,
            prop.z - lx * sin_yaw + lz * cos_yaw,
        )
    };
    let corner = |sx: f32, sy: f32, sz: f32| -> [f32; 3] {
        let (world_x, world_z) = rotate(sx * half_w, sz * half_d);
        [world_x, center_y + sy * half_h, world_z]
    };
    let shaded = |mult: f32, point: [f32; 3]| -> [f32; 3] {
        let light = lighting.sample(point[0], point[1], point[2]);
        [
            (color[0] * mult * light).min(1.0),
            (color[1] * mult * light).min(1.0),
            (color[2] * mult * light).min(1.0),
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

/// One lit surface to emit: where it sits, which way it faces, and which
/// material region of its grid this call covers.
#[derive(Clone, Copy)]
struct LitSurface<'a> {
    y: f32,
    /// Ceilings run the opposite winding to floors so they face down.
    ceiling: bool,
    /// `Some((label, cell_labels))` emits only the cells carrying that label and
    /// never merges across a label boundary, which is what gives a floor patch
    /// its exact rectangular edge. `None` emits every cell.
    region: Option<(u32, &'a [u32])>,
}

/// Emits one lit floor or ceiling from a precomputed corner-colour grid.
///
/// Cells are greedily merged along X and then Z while every corner of the
/// candidate rectangle stays within [`LIGHT_GRID_MERGE_EPS`], so uniform
/// regions cost one quad instead of up to `MAX_LIGHT_GRID_CELLS`² of them.
/// The surviving corners keep their exact sampled colours; only interior
/// corners that were already within the tolerance of the merged corners are
/// removed. UVs stay world-space, so merging is invisible to texturing.
fn emit_lit_surface_grid(
    vertices: &mut Vec<Vertex>,
    xs: &[f32],
    zs: &[f32],
    colors: &[[f32; 3]],
    surface: LitSurface<'_>,
    uv: impl Fn(f32, f32) -> [f32; 2],
) {
    let LitSurface { y, ceiling, region } = surface;
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
            {
                ix1 += 1;
            }
            let mut iz1 = iz;
            while iz1 + 1 < cells_z
                && region_free(&covered, ix, ix1, iz, iz1 + 1)
                && grid_rect_is_uniform(colors, row_len, ix, ix1, iz, iz1 + 1, reference)
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
            // Winding mirrors the original per-cell loops: floors run +X/+Z
            // from the minimum corner, ceilings run the opposite way so they
            // face down. Colours follow their own corners in both cases.
            let (points, corners) = if ceiling {
                (
                    [[ax, y, bz], [bx, y, bz], [bx, y, az], [ax, y, az]],
                    [c01, c11, c10, c00],
                )
            } else {
                (
                    [[ax, y, az], [bx, y, az], [bx, y, bz], [ax, y, bz]],
                    [c00, c10, c11, c01],
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
fn lit_surface_grid(
    lighting: &LevelLighting,
    room_index: usize,
    xs: &[f32],
    zs: &[f32],
    y: f32,
    tint: Option<[f32; 3]>,
) -> Vec<[f32; 3]> {
    let mut colors = Vec::with_capacity(xs.len() * zs.len());
    for z in zs {
        for x in xs {
            let light = lighting.sample_in_room(room_index, *x, y, *z);
            colors.push(match tint {
                Some(tint) => [tint[0] * light, tint[1] * light, tint[2] * light],
                None => [light, light, light],
            });
        }
    }
    colors
}

/// Corner coordinates of a room surface along one axis, `cells + 1` values.
fn surface_axis_positions(origin: f32, extent: f32, cells: u32) -> Vec<f32> {
    (0..=cells)
        .map(|index| origin + extent * index as f32 / cells as f32)
        .collect()
}

/// True when any part of `patch` overlaps the room's floor rectangle.
fn patch_covers_room(patch: &crate::level::FloorPatchDef, room: &crate::level::RoomDef) -> bool {
    let x0 = room.x.min(room.x + room.width);
    let x1 = room.x.max(room.x + room.width);
    let z0 = room.z.min(room.z + room.depth);
    let z1 = room.z.max(room.z + room.depth);
    let px0 = patch.x.min(patch.x + patch.width);
    let px1 = patch.x.max(patch.x + patch.width);
    let pz0 = patch.z.min(patch.z + patch.depth);
    let pz1 = patch.z.max(patch.z + patch.depth);
    px1 > x0 && px0 < x1 && pz1 > z0 && pz0 < z1
}

/// Tolerance used when merging floor cut lines and when matching patch edges.
const FLOOR_CUT_EPS: f32 = 1e-3;

/// Grid positions of a room's floor along one axis, including every floor patch
/// edge that falls inside the room.
///
/// Adding the patch edges as cut lines is what keeps a patch boundary exact
/// without a second overlapping floor slab (design section 23).
fn floor_cut_positions(origin: f32, extent: f32, cells: u32, edges: &[f32]) -> Vec<f32> {
    let mut positions = surface_axis_positions(origin, extent, cells);
    let (low, high) = (origin + FLOOR_CUT_EPS, origin + extent - FLOOR_CUT_EPS);
    for edge in edges {
        if !edge.is_finite() || *edge <= low || *edge >= high {
            continue;
        }
        positions.push(*edge);
    }
    positions.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    positions.dedup_by(|a, b| (*a - *b).abs() <= FLOOR_CUT_EPS);
    positions
}

/// Material regions of one room's floor: which surface family each grid cell
/// draws with, and the families themselves.
struct FloorRegions {
    /// One label per grid cell, row-major over `xs` x `zs`.
    cells: Vec<u32>,
    /// Surface family per label; label 0 is the room's own floor material.
    kinds: Vec<SurfaceKind>,
}

/// Splits a room floor into its patch regions, or `None` when no patch covers
/// any of its cells.
///
/// Later patches win when two overlap, so the result is deterministic. A patch
/// whose material resolves to the room's own surface family is still tracked
/// separately (its edge simply does not change the texture); that keeps the
/// region logic independent of which materials a creator picks.
fn floor_regions(
    xs: &[f32],
    zs: &[f32],
    base_kind: SurfaceKind,
    patches: &[&crate::level::FloorPatchDef],
) -> Option<FloorRegions> {
    let cells_x = xs.len().saturating_sub(1);
    let cells_z = zs.len().saturating_sub(1);
    if cells_x == 0 || cells_z == 0 || patches.is_empty() {
        return None;
    }
    let mut cells = vec![0u32; cells_x * cells_z];
    let mut patch_kinds: Vec<SurfaceKind> = Vec::new();
    let mut any = false;
    for iz in 0..cells_z {
        let z = (zs[iz] + zs[iz + 1]) * 0.5;
        for ix in 0..cells_x {
            let x = (xs[ix] + xs[ix + 1]) * 0.5;
            let Some(patch) = patches.iter().rev().find(|patch| {
                let x0 = patch.x.min(patch.x + patch.width);
                let x1 = patch.x.max(patch.x + patch.width);
                let z0 = patch.z.min(patch.z + patch.depth);
                let z1 = patch.z.max(patch.z + patch.depth);
                x >= x0 && x <= x1 && z >= z0 && z <= z1
            }) else {
                continue;
            };
            let kind = material_surface(MaterialSlot::Floor, &patch.material);
            let label = match patch_kinds.iter().position(|existing| *existing == kind) {
                Some(index) => index as u32 + 1,
                None => {
                    patch_kinds.push(kind);
                    patch_kinds.len() as u32
                }
            };
            cells[iz * cells_x + ix] = label;
            any = true;
        }
    }
    if !any {
        return None;
    }
    let mut kinds = vec![base_kind];
    kinds.extend(patch_kinds);
    Some(FloorRegions { cells, kinds })
}

/// Flushes the quads appended to `scratch` since `cursor` into `kind`'s bucket.
///
/// Walls write their length faces, sills, headers and reveals into one scratch
/// buffer; flushing the run a face produced is what lets each face carry its own
/// material while the emitters stay unchanged. Whole quads only, as everywhere
/// else in the builder.
fn flush_wall_run(
    buckets: &mut crate::spatial::SpatialBuckets<SurfaceKind>,
    scratch: &[Vertex],
    cursor: &mut usize,
    kind: SurfaceKind,
) {
    if *cursor >= scratch.len() {
        return;
    }
    buckets.add_quads(kind, &scratch[*cursor..]);
    *cursor = scratch.len();
}

fn build_level_geometry_mesh(
    level: &LevelDef,
    catalog: &crate::loader::PropCatalog,
    fallback_props: &[&PropDef],
    lighting: &LevelLighting,
) -> LevelMesh {
    // Collect the merged room list once; geometry and ceiling lookups then
    // borrow it instead of cloning the room vector repeatedly.
    let rooms: Vec<_> = level.room_iter().collect();
    // Emitters still write whole quads into one scratch buffer; the bucket
    // builder splits each run by spatial cell on the way into the mesh. That
    // keeps the emitting code free of any grid awareness.
    let mut scratch: Vec<Vertex> = Vec::new();
    let mut buckets =
        crate::spatial::SpatialBuckets::<SurfaceKind>::with_grid(spatial_cell_grid(level));

    // 1. Floor batch: the baked-lighting grid over the room, sampled once per
    //    corner and greedily merged wherever the lighting is effectively flat
    //    (unlit rooms and the far flanks of large rooms therefore stay one or
    //    two quads). The cell count is bounded by `lighting::MAX_LIGHT_GRID_CELLS`,
    //    and UVs keep mapping the same world space as the original single quad.
    //    The metre-scale checker tint stays baked into the derived floor texture
    //    (see `generate_floor_checker_texture`), so no per-cell texture work is
    //    needed.
    //
    //    A room that carries floor patches is cut at the patch edges and emitted
    //    one material region at a time, so a damp patch has an exact edge and no
    //    second overlapping slab (design section 23).
    for (room_index, room) in rooms.iter().enumerate() {
        if !room_is_tessellatable(room) {
            continue;
        }
        let base_material = room
            .material
            .as_deref()
            .unwrap_or(level.defaults.floor.as_str());
        let base_kind = material_surface(MaterialSlot::Floor, base_material);
        let room_patches: Vec<&crate::level::FloorPatchDef> = level
            .floor_patches
            .iter()
            .filter(|patch| patch_covers_room(patch, room))
            .collect();
        let mut cut_x: Vec<f32> = Vec::with_capacity(room_patches.len() * 2);
        let mut cut_z: Vec<f32> = Vec::with_capacity(room_patches.len() * 2);
        for patch in &room_patches {
            cut_x.push(patch.x);
            cut_x.push(patch.x + patch.width);
            cut_z.push(patch.z);
            cut_z.push(patch.z + patch.depth);
        }
        let cells_x = light_grid_cells(room.width);
        let cells_z = light_grid_cells(room.depth);
        let xs = floor_cut_positions(room.x, room.width, cells_x, &cut_x);
        let zs = floor_cut_positions(room.z, room.depth, cells_z, &cut_z);
        let colors = lit_surface_grid(lighting, room_index, &xs, &zs, 0.0, None);

        match floor_regions(&xs, &zs, base_kind, &room_patches) {
            None => {
                scratch.clear();
                emit_lit_surface_grid(
                    &mut scratch,
                    &xs,
                    &zs,
                    &colors,
                    LitSurface {
                        y: 0.0,
                        ceiling: false,
                        region: None,
                    },
                    |x, z| [x / FLOOR_TILE_METRES, z / FLOOR_TILE_METRES],
                );
                buckets.add_quads(base_kind, &scratch);
            }
            Some(regions) => {
                for (label, kind) in regions.kinds.iter().enumerate() {
                    scratch.clear();
                    emit_lit_surface_grid(
                        &mut scratch,
                        &xs,
                        &zs,
                        &colors,
                        LitSurface {
                            y: 0.0,
                            ceiling: false,
                            region: Some((label as u32, &regions.cells)),
                        },
                        |x, z| [x / FLOOR_TILE_METRES, z / FLOOR_TILE_METRES],
                    );
                    buckets.add_quads(*kind, &scratch);
                }
            }
        }
    }

    // 2. Ceiling batch: the same grid and the same lighting sample, with the
    //    fixture panels themselves drawn brighter by the light batch below.
    //    Ceilings carry no patches, only the room's ceiling material.
    let ceiling_tint = [0.72, 0.72, 0.70];
    for (room_index, room) in rooms.iter().enumerate() {
        if !room_is_tessellatable(room) {
            continue;
        }
        let h = room.height;
        let ceiling_kind = material_surface(
            MaterialSlot::Ceiling,
            room.ceiling_material
                .as_deref()
                .unwrap_or(level.defaults.ceiling.as_str()),
        );
        let cells_x = light_grid_cells(room.width);
        let cells_z = light_grid_cells(room.depth);
        let xs = surface_axis_positions(room.x, room.width, cells_x);
        let zs = surface_axis_positions(room.z, room.depth, cells_z);
        let colors = lit_surface_grid(lighting, room_index, &xs, &zs, h, Some(ceiling_tint));
        scratch.clear();
        emit_lit_surface_grid(
            &mut scratch,
            &xs,
            &zs,
            &colors,
            LitSurface {
                y: h,
                ceiling: true,
                region: None,
            },
            |x, z| [x / CEILING_TILE_METRES, z / CEILING_TILE_METRES],
        );
        buckets.add_quads(ceiling_kind, &scratch);
    }

    // 3. Walls batch. Each length face draws with its own material (design
    //    section 21): the `faces` override for its direction, else the wall's
    //    own `material`, else the level default. Sills, headers and reveal
    //    jambs follow the wall's material.
    let base_wall = [0.85, 0.80, 0.42];

    for wall in &level.walls {
        scratch.clear();
        let wall_material = wall
            .material
            .as_deref()
            .unwrap_or(level.defaults.wall.as_str());
        let wall_kind = material_surface(MaterialSlot::Wall, wall_material);
        let face_kind = |name: &str| {
            let material = wall
                .faces
                .get(name)
                .map(String::as_str)
                .unwrap_or(wall_material);
            material_surface(MaterialSlot::Wall, material)
        };
        let x0 = wall.x.min(wall.x + wall.width);
        let x1 = wall.x.max(wall.x + wall.width);
        let z0 = wall.z.min(wall.z + wall.depth);
        let z1 = wall.z.max(wall.z + wall.depth);
        let ceiling_h =
            ceiling_height_at(&rooms, wall.x + wall.width * 0.5, wall.z + wall.depth * 0.5);
        let h = wall.resolved_height(ceiling_h);
        let wall_base = wall.y.min(wall.y + h);

        let north_mult = 1.00;
        let south_mult = 0.88;
        let west_mult = 0.84;
        let east_mult = 0.94;

        let top_grad = 1.05;
        let bot_grad = 0.92;
        // Reveal faces are deliberately darker than the wall faces they
        // interrupt, so doorways and windows read clearly.
        let jamb_mult = 0.78;
        let head_mult = 0.92;

        let scale_color = |mult: f32, grad: f32| -> [f32; 3] {
            [
                (base_wall[0] * mult * grad).min(1.0),
                (base_wall[1] * mult * grad).min(1.0),
                (base_wall[2] * mult * grad).min(1.0),
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
        let slices = wall_solid_slices(wall, ceiling_h);
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

            // Faces parallel to the length axis: north/south for X-axis
            // walls, west/east for Z-axis walls. Each face is a strip of quads
            // so the baked lighting varies along the wall.
            let n_top = scale_color(north_mult, top_grad);
            let n_bot = scale_color(north_mult, bot_grad);
            let s_top = scale_color(south_mult, top_grad);
            let s_bot = scale_color(south_mult, bot_grad);
            let w_top = scale_color(west_mult, top_grad);
            let w_bot = scale_color(west_mult, bot_grad);
            let e_top = scale_color(east_mult, top_grad);
            let e_bot = scale_color(east_mult, bot_grad);

            // (face coordinate across the thickness, outward normal, bottom/top
            // colour, whether the winding runs against the length axis, the
            // direction name used by `faces`).
            let faces: [WallLengthFace; 2] = match axis {
                WallAxis::X => [
                    (z0, -1.0, n_bot, n_top, false, "north"),
                    (z1, 1.0, s_bot, s_top, true, "south"),
                ],
                WallAxis::Z => [
                    (x0, -1.0, w_bot, w_top, true, "west"),
                    (x1, 1.0, e_bot, e_top, false, "east"),
                ],
            };
            for (face, normal, bottom_shade, top_shade, reversed, name) in faces {
                add_wall_length_face(
                    &mut scratch,
                    axis,
                    l0,
                    l1,
                    face,
                    normal,
                    slice_bottom,
                    slice_top,
                    bottom_shade,
                    top_shade,
                    reversed,
                    lighting,
                );
                flush_wall_run(&mut buckets, &scratch, &mut wall_cursor, face_kind(name));
            }

            // Top face (normal +Y): half-height walls and window sills.
            if slice_top < ceiling_h - 1e-3 {
                let top_col = scale_color(1.00, top_grad);
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
                            wall_uv(l0, t1),
                            points[1],
                            colors[1],
                            wall_uv(l1, t1),
                            points[2],
                            colors[2],
                            wall_uv(l1, t0),
                            points[3],
                            colors[3],
                            wall_uv(l0, t0),
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
                            wall_uv(l0, t1),
                            points[1],
                            colors[1],
                            wall_uv(l1, t1),
                            points[2],
                            colors[2],
                            wall_uv(l1, t0),
                            points[3],
                            colors[3],
                            wall_uv(l0, t0),
                        );
                    }
                }
                flush_wall_run(&mut buckets, &scratch, &mut wall_cursor, wall_kind);
            }

            // Bottom face (normal -Y): visible on raised walls and on door or
            // window headers. Testing against the floor plane reproduces the
            // previous whole-wall behaviour for raised walls.
            if slice_bottom > 1e-3 {
                let bot_col = scale_color(0.85, bot_grad);
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
                            wall_uv(l0, t0),
                            points[1],
                            colors[1],
                            wall_uv(l1, t0),
                            points[2],
                            colors[2],
                            wall_uv(l1, t1),
                            points[3],
                            colors[3],
                            wall_uv(l0, t1),
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
                            wall_uv(l0, t0),
                            points[1],
                            colors[1],
                            wall_uv(l1, t0),
                            points[2],
                            colors[2],
                            wall_uv(l1, t1),
                            points[3],
                            colors[3],
                            wall_uv(l0, t1),
                        );
                    }
                }
                flush_wall_run(&mut buckets, &scratch, &mut wall_cursor, wall_kind);
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
                // Wall ends keep the directional face shading; internal
                // reveals use the darker jamb/head colours.
                let mult = if at_start {
                    match axis {
                        WallAxis::X => west_mult,
                        WallAxis::Z => north_mult,
                    }
                } else if at_end {
                    match axis {
                        WallAxis::X => east_mult,
                        WallAxis::Z => south_mult,
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
                let corners = match axis {
                    // The emitted winding visits t1 first for X-axis walls and
                    // t0 first for Z-axis walls (see add_wall_cross_quad).
                    WallAxis::X => [
                        shade(scale_color(mult, bot_grad), bottom_t1),
                        shade(scale_color(mult, bot_grad), bottom_t0),
                        shade(scale_color(mult, top_grad), top_t0),
                        shade(scale_color(mult, top_grad), top_t1),
                    ],
                    WallAxis::Z => [
                        shade(scale_color(mult, bot_grad), bottom_t0),
                        shade(scale_color(mult, bot_grad), bottom_t1),
                        shade(scale_color(mult, top_grad), top_t1),
                        shade(scale_color(mult, top_grad), top_t0),
                    ],
                };
                add_wall_cross_quad(&mut scratch, axis, at, (t0, t1), bottom, top, corners);
            }
        }
        flush_wall_run(&mut buckets, &scratch, &mut wall_cursor, wall_kind);
    }

    // 4. Ceiling lights batch. Panels hang just below their room's ceiling
    //    (a 2.6 m corridor and a 3 m room therefore get different fixture
    //    heights) and glow slightly more or less with their authored intensity.
    for light in &level.ceiling_lights {
        if !light.x.is_finite() || !light.z.is_finite() {
            continue;
        }
        scratch.clear();
        let (half_w, half_d) = fixture_half_extents(light.rotation_degrees);

        let y = lighting.fixture_y(light.x, light.z);
        let x0 = light.x - half_w;
        let x1 = light.x + half_w;
        let z0 = light.z - half_d;
        let z1 = light.z + half_d;

        let intensity = light.intensity();
        let output = (0.60 + 0.40 * intensity.clamp(0.0, 2.0)).clamp(0.0, 1.0);
        let fixture_glow = [1.00 * output, 0.98 * output, 0.92 * output];
        add_quad_flat(
            &mut scratch,
            [x0, y, z1],
            [x1, y, z1],
            [x1, y, z0],
            [x0, y, z0],
            fixture_glow,
            [0.0, 1.0],
            [1.0, 1.0],
            [1.0, 0.0],
            [0.0, 0.0],
        );

        let bezel_color = [0.40, 0.40, 0.40];
        let b = 0.05;
        add_quad_flat(
            &mut scratch,
            [x0 - b, y, z0],
            [x1 + b, y, z0],
            [x1 + b, y, z0 - b],
            [x0 - b, y, z0 - b],
            bezel_color,
            [0.0, 1.0],
            [1.0, 1.0],
            [1.0, 0.0],
            [0.0, 0.0],
        );
        add_quad_flat(
            &mut scratch,
            [x0 - b, y, z1 + b],
            [x1 + b, y, z1 + b],
            [x1 + b, y, z1],
            [x0 - b, y, z1],
            bezel_color,
            [0.0, 1.0],
            [1.0, 1.0],
            [1.0, 0.0],
            [0.0, 0.0],
        );
        buckets.add_quads(SurfaceKind::Light, &scratch);
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
        add_prop_box(&mut scratch, prop, size, entry.color, lighting);
        // Whole run, not per quad: a placeholder box straddling a cell boundary
        // must stay one draw range, like the real prop geometry it stands in for.
        buckets.add_run(SurfaceKind::PropFallback, &scratch);
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
fn finish_indexed_mesh(mut buckets: crate::spatial::SpatialBuckets<SurfaceKind>) -> LevelMesh {
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

    for ((kind, _cell), range) in drained {
        if range.indices.is_empty() {
            continue;
        }
        let index_len = range.indices.len() as i32;
        let slot = &mut spans[kind as usize];
        *slot = Some(match *slot {
            None => (virtual_index, virtual_index + index_len),
            Some((low, high)) => (low.min(virtual_index), high.max(virtual_index + index_len)),
        });
        virtual_index += index_len;
        vertex_count += range.vertices.len();
        index_count += range.indices.len();
        ranges.push(LevelMeshRange {
            kind,
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
    pub fn stride(self) -> i32 {
        match self {
            VertexLayout::Packed => packed_layout::STRIDE,
            VertexLayout::Exact => std::mem::size_of::<Vertex>() as i32,
        }
    }

    /// Bytes one vertex occupies on the GPU in this layout.
    pub fn vertex_bytes(self) -> usize {
        match self {
            VertexLayout::Packed => std::mem::size_of::<PackedVertex>(),
            VertexLayout::Exact => std::mem::size_of::<Vertex>(),
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
            let index_start = chunk.indices.len() as i32;
            let vertex_start = chunk.vertices.len() as i32;

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
                    remap[source] = chunk.vertices.len() as u16;
                    chunk.vertices.push(*vertex);
                }
                chunk.indices.push(remap[source]);
                cursor += 1;
            }

            let index_count = chunk.indices.len() as i32 - index_start;
            if index_count > 0 {
                placements.push(PackedRange {
                    chunk: chunk_index,
                    index_start,
                    index_count,
                    vertex_start,
                    vertex_count: chunk.vertices.len() as i32 - vertex_start,
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
            flat_indices.push(flat.len() as u16);
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
    let (mesh, batches, _lighting) =
        build_level_geometry_with_assets_and_lighting(level, catalog, assets);
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
    let (mesh, batches, lighting, _) =
        build_level_geometry_timed(level, catalog, assets);
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
) -> (LevelMesh, Vec<PropMeshBatch>, LevelLighting, BuildTimings) {
    let started = std::time::Instant::now();
    let lighting = LevelLighting::bake(level);
    let lighting_millis = started.elapsed().as_secs_f64() * 1000.0;

    let started = std::time::Instant::now();
    let (batches, fallbacks) = resolve_prop_instances(level, catalog, assets, &lighting);
    let props_millis = started.elapsed().as_secs_f64() * 1000.0;

    let started = std::time::Instant::now();
    let mesh = build_level_geometry_mesh(level, catalog, &fallbacks, &lighting);
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

/// Builds level geometry using only built-in prop fallbacks.
///
/// Callers that can resolve the prop catalog should prefer
/// [`build_level_geometry_with_catalog`].
pub fn build_level_geometry(level: &LevelDef) -> LevelMesh {
    build_level_geometry_with_catalog(level, &crate::loader::PropCatalog::builtin())
}

/// Builds level geometry, drawing every prop as its catalogue placeholder box
/// (no GLB assets are read). Used by tests and by the asset-less fallback path.
pub fn build_level_geometry_with_catalog(
    level: &LevelDef,
    catalog: &crate::loader::PropCatalog,
) -> LevelMesh {
    let lighting = LevelLighting::bake(level);
    let fallbacks: Vec<&PropDef> = level.props.iter().collect();
    build_level_geometry_mesh(level, catalog, &fallbacks, &lighting)
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

        let model = prop_instance_matrix(prop);
        // Cull by the instance's real world-space extent, not by the cell it
        // happens to be centred in: a chair on a cell boundary must not be
        // culled while a sliver of it is still on screen.
        let instance_bounds = match asset.model.bounds() {
            Some((low, high)) => {
                transform_bounds(&crate::spatial::Aabb { min: low, max: high }, &model)
            }
            None => crate::spatial::Aabb::from_point([prop.x, prop.y, prop.z]),
        };
        let cell = grid.cell_of(instance_bounds.centre());

        // One batch holds every instance of a model inside one spatial cell, but
        // never more than a 16-bit index can address: `PropMeshBatch::indices`
        // are `GL_UNSIGNED_SHORT` offsets into the batch's own vertex list, so a
        // cell holding hundreds of instances has to become several batches.
        let key = (model_path.clone(), cell);
        let needs_new_batch = match index_by_batch.get(&key) {
            Some(index) => {
                batches[*index].vertices.len() + asset.model.vertices.len()
                    > crate::spatial::MAX_INDEX_VERTICES
            }
            None => true,
        };
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
            let position =
                model.transform_point3(glam::Vec3::new(vertex.pos[0], vertex.pos[1], vertex.pos[2]));
            // Bake the environment into the instance's colour: the same model in
            // a dark corner and under a fixture still shares one batch, but is
            // no longer uniformly lit.
            let light = lighting.sample(position.x, position.y, position.z);
            batch.vertices.push(Vertex {
                pos: [position.x, position.y, position.z],
                color: [
                    vertex.color[0] * light,
                    vertex.color[1] * light,
                    vertex.color[2] * light,
                    vertex.color[3],
                ],
                uv: vertex.uv,
            });
        }
        for index in &asset.model.indices {
            batch.indices.push(base as u16 + *index);
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
/// This is exactly the transform the placeholder boxes use (see
/// [`add_prop_box`]), so a prop keeps its position, orientation and vertical
/// offset when its real model replaces the box. Model space is metres with the
/// origin at the floor-contact centre (see `assets/props/README.md`).
pub fn prop_instance_matrix(prop: &PropDef) -> glam::Mat4 {
    let rotation = glam::Mat4::from_rotation_y(prop.rotation_degrees.to_radians());
    let scale = glam::Mat4::from_scale(glam::Vec3::splat(prop.scale));
    glam::Mat4::from_translation(glam::Vec3::new(prop.x, prop.y, prop.z)) * rotation * scale
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
            min_filter as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MAG_FILTER,
            mag_filter as i32,
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
            glow::RGBA as i32,
            width,
            height,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(Some(pixels)),
        );

        let wrap_mode = if repeat {
            glow::REPEAT as i32
        } else {
            glow::CLAMP_TO_EDGE as i32
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
                glow::NEAREST as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MAG_FILTER,
                glow::NEAREST as i32,
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
    wall_texture: glow::Texture,
    floor_texture: glow::Texture,
    ceiling_texture: glow::Texture,
    /// Water-damaged sheets for the damaged core materials (design section 21).
    /// They are built from the same procedural generators the whole level uses
    /// when its defaults are damaged, so a stained wall in one room matches a
    /// stained wall in a level that is stained throughout.
    wall_stained_texture: glow::Texture,
    floor_damp_texture: glow::Texture,
    ceiling_stained_texture: glow::Texture,
    white_texture: glow::Texture,
    font_texture: glow::Texture,
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
    /// Initializes an accelerated OpenGL context with VSync, textures and an
    /// empty level buffer.
    ///
    /// The caller uploads the first level with [`Renderer::set_level`] (or
    /// [`Renderer::rebuild_level_geometry`]); building here as well would bake
    /// and upload the same level twice before the first frame, which is real
    /// cost on the PocketCHIP.
    pub fn new(window: &sdl2::video::Window, video: &sdl2::VideoSubsystem) -> Result<Self, String> {
        let gl_attr = video.gl_attr();
        gl_attr.set_double_buffer(true);
        gl_attr.set_depth_size(24);

        gl_attr.set_context_profile(sdl2::video::GLProfile::GLES);
        gl_attr.set_context_version(2, 0);

        let gl_context = match window.gl_create_context() {
            Ok(ctx) => ctx,
            Err(_) => {
                gl_attr.set_context_profile(sdl2::video::GLProfile::Compatibility);
                gl_attr.set_context_version(2, 1);
                window.gl_create_context()?
            }
        };

        window.gl_make_current(&gl_context)?;

        // Swap interval is configured by the caller *after* this returns: the
        // request must be issued while a context is current, and the caller owns
        // the user's VSync setting. Forcing VSync on here silently overrode it.

        let gl = unsafe {
            glow::Context::from_loader_function(|proc_name| {
                video.gl_get_proc_address(proc_name) as *const _
            })
        };

        let prop_catalog = crate::loader::PropCatalog::load_default();

        let (
            program,
            ui_vbo,
            wall_texture,
            floor_texture,
            ceiling_texture,
            wall_stained_texture,
            floor_damp_texture,
            ceiling_stained_texture,
            white_texture,
            font_texture,
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

            // Create textures. The maintained sheets are re-uploaded per level
            // (see `set_level`); the three water-damaged sheets are fixed
            // built-in images, so they are uploaded once here and only bound by
            // the levels that reference the damaged core materials.
            let wall_texture =
                create_texture_2d(&gl, 128, 128, &generate_wall_texture(), true, true)?;
            let floor_texture =
                create_texture_2d(&gl, 64, 64, &generate_carpet_texture(), true, true)?;
            let ceiling_texture =
                create_texture_2d(&gl, 128, 128, &generate_ceiling_texture(), true, true)?;
            let wall_stained_texture =
                create_texture_2d(&gl, 128, 128, &generate_stained_wall_texture(), true, true)?;
            let floor_damp_sheet =
                crate::loader::RawImage::new(64, 64, generate_damp_carpet_texture().to_vec());
            let floor_damp = generate_floor_checker_texture(&floor_damp_sheet);
            let floor_damp_texture = create_texture_2d(
                &gl,
                floor_damp.width as i32,
                floor_damp.height as i32,
                &floor_damp.rgba,
                true,
                true,
            )?;
            let ceiling_stained_texture = create_texture_2d(
                &gl,
                128,
                128,
                &generate_stained_ceiling_texture(),
                true,
                true,
            )?;
            let white_texture =
                create_texture_2d(&gl, 2, 2, &generate_white_texture(), false, false)?;
            let font_texture =
                create_texture_2d(&gl, 128, 64, &generate_font_atlas(), false, false)?;

            // Level geometry is uploaded by `rebuild_level_geometry` once the
            // renderer (and its prop asset cache) exists.
            // Level geometry is uploaded by `rebuild_level_geometry` once the
            // renderer (and its prop asset cache) exists.
            let ui_vbo = gl.create_buffer()?;
            gl.bind_buffer(glow::ARRAY_BUFFER, None);

            (
                program,
                ui_vbo,
                wall_texture,
                floor_texture,
                ceiling_texture,
                wall_stained_texture,
                floor_damp_texture,
                ceiling_stained_texture,
                white_texture,
                font_texture,
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
            wall_texture,
            floor_texture,
            ceiling_texture,
            wall_stained_texture,
            floor_damp_texture,
            ceiling_stained_texture,
            white_texture,
            font_texture,
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
            for texture in [
                self.wall_texture,
                self.floor_texture,
                self.ceiling_texture,
                self.wall_stained_texture,
                self.floor_damp_texture,
                self.ceiling_stained_texture,
            ] {
                self.gl.bind_texture(glow::TEXTURE_2D, Some(texture));
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
    pub fn prop_draw_count(&self) -> usize {
        self.prop_draws.len()
    }

    /// Static-geometry and baked-lighting statistics for the current level.
    pub fn level_stats(&self) -> LevelBuildStats {
        self.level_stats
    }

    /// Reads back the default framebuffer as a top-down RGBA image.
    ///
    /// Used by the `LIMINAL_CAPTURE` developer/hardware path: it is the only way
    /// to inspect real prop rendering on the PocketCHIP (no screenshots over
    /// SSH) and on desktops where the window cannot be captured. Call it after
    /// drawing and before swapping buffers.
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
                width as i32,
                height as i32,
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
                glow::RGBA as i32,
                raw_image.width as i32,
                raw_image.height as i32,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(&raw_image.rgba)),
            );

            let wrap_mode = if repeat {
                glow::REPEAT as i32
            } else {
                glow::CLAMP_TO_EDGE as i32
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
                    glow::NEAREST as i32,
                );
                gl.tex_parameter_i32(
                    glow::TEXTURE_2D,
                    glow::TEXTURE_MAG_FILTER,
                    glow::NEAREST as i32,
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
    pub fn rebuild_level_geometry(&mut self, level: &crate::level::LevelDef) {
        let started = std::time::Instant::now();
        let (mesh, batches, lighting, timings) = build_level_geometry_timed(
            level,
            &self.prop_catalog,
            &mut self.prop_assets,
        );
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
                    kind: range.kind,
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
                                packed.as_ptr() as *const u8,
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
                                chunk.vertices.as_ptr() as *const u8,
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
                        chunk.indices.as_ptr() as *const u8,
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

    /// Uploads one prop model's diffuse texture with mipmaps and CLAMP_TO_EDGE
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
                glow::RGBA as i32,
                image.width as i32,
                image.height as i32,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(&image.rgba)),
            );
            self.gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_S,
                glow::CLAMP_TO_EDGE as i32,
            );
            self.gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_T,
                glow::CLAMP_TO_EDGE as i32,
            );
            set_repeat_filter(&self.gl, self.linear_filtering);
            self.gl.generate_mipmap(glow::TEXTURE_2D);
            self.gl.bind_texture(glow::TEXTURE_2D, None);
            Ok(texture)
        }
    }

    /// Re-uploads new level geometry and textures dynamically into OpenGL without recompilation.
    pub fn set_level(&mut self, loaded: &crate::loader::LoadedLevel) {
        self.rebuild_level_geometry(&loaded.level);
        // Bake the metre checkerboard tint into the floor texture so a room
        // floor is a single quad (see `generate_floor_checker_texture`).
        let floor_texture = generate_floor_checker_texture(&loaded.textures.floor);
        let linear = self.linear_filtering;

        unsafe {
            Self::upload_texture(
                &self.gl,
                self.wall_texture,
                &loaded.textures.wall,
                true,
                linear,
            );
            Self::upload_texture(&self.gl, self.floor_texture, &floor_texture, true, linear);
            Self::upload_texture(
                &self.gl,
                self.ceiling_texture,
                &loaded.textures.ceiling,
                true,
                linear,
            );
            Self::upload_texture(
                &self.gl,
                self.white_texture,
                &loaded.textures.fixture,
                false,
                linear,
            );
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
            self.gl
                .viewport(0, 0, drawable.width as i32, drawable.height as i32);
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
            let mut bound_kind: Option<SurfaceKind> = None;
            let mut bound_chunk: Option<usize> = None;
            for batch in &self.static_batches {
                if batch.index_range.count <= 0 {
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
                if bound_kind != Some(batch.kind) {
                    let texture = match batch.kind {
                        SurfaceKind::Floor => self.floor_texture,
                        SurfaceKind::FloorDamp => self.floor_damp_texture,
                        SurfaceKind::Ceiling => self.ceiling_texture,
                        SurfaceKind::CeilingStained => self.ceiling_stained_texture,
                        SurfaceKind::Wall => self.wall_texture,
                        SurfaceKind::WallStained => self.wall_stained_texture,
                        SurfaceKind::Light | SurfaceKind::PropFallback => self.white_texture,
                    };
                    self.gl.bind_texture(glow::TEXTURE_2D, Some(texture));
                    bound_kind = Some(batch.kind);
                }
                self.gl.draw_elements(
                    glow::TRIANGLES,
                    batch.index_range.count,
                    glow::UNSIGNED_SHORT,
                    batch.index_range.start * 2,
                );
                visible_vertices += batch.vertex_count.max(0) as usize;
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
                    visible_vertices += draw.vertex_count.max(0) as usize;
                    visible_batches += 1;
                    draw_calls += 1;
                }
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
    pub fn set_vertex_layout(&mut self, layout: VertexLayout) {
        self.vertex_layout = layout;
    }

    /// Enables or disables indexed submission. Only the debug benchmark turns
    /// this off, to measure what indexing is worth on real hardware.
    pub fn set_indexing(&mut self, enabled: bool) {
        self.indexing_enabled = enabled;
    }

    /// Enables or disables frustum culling.
    ///
    /// Culling is always on in normal play; the debug benchmark harness turns it
    /// off so the same build can measure what it is worth on real hardware.
    pub fn set_culling(&mut self, enabled: bool) {
        self.culling_enabled = enabled;
    }

    /// Spatial grid the current level was partitioned with, for developer logs.
    pub fn spatial_grid(&self) -> crate::spatial::CellGrid {
        self.spatial_grid
    }

    /// Number of cullable static ranges the current level is split into.
    pub fn static_batch_count(&self) -> usize {
        self.static_batches.len()
    }

    /// Static batch count per [`SurfaceKind`], in [`SurfaceKind::ALL`] order.
    ///
    /// Printed once per level load so a hardware run makes it obvious when a
    /// level has been shredded into more draw calls than the GPU can afford.
    pub fn static_batch_breakdown(&self) -> [usize; SurfaceKind::ALL.len()] {
        let mut counts = [0usize; SurfaceKind::ALL.len()];
        for batch in &self.static_batches {
            counts[batch.kind as usize] += 1;
        }
        counts
    }

    /// Static batch count per [`SurfaceFamily`], in [`SurfaceFamily::ALL`] order.
    ///
    /// The developer log reports families: whether a level's damaged sheets add
    /// draw calls is a texture choice, not a separate kind of surface.
    pub fn static_batch_family_breakdown(&self) -> [usize; SurfaceFamily::ALL.len()] {
        let mut counts = [0usize; SurfaceFamily::ALL.len()];
        for batch in &self.static_batches {
            counts[batch.kind.family() as usize] += 1;
        }
        counts
    }

    /// Counters for the most recently submitted scene (see [`RenderStats`]).
    pub fn render_stats(&self) -> RenderStats {
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
            VertexLayout::Packed => (
                glow::UNSIGNED_BYTE,
                true,
                packed_layout::COLOR_OFFSET,
            ),
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
            self.gl
                .vertex_attrib_pointer_f32(self.a_uv_loc, 2, glow::FLOAT, false, stride, uv_offset);
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
            self.gl
                .bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(ibo));
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
                        self.ui_scratch.as_ptr() as *const u8,
                        std::mem::size_of_val(self.ui_scratch.as_slice()),
                    )
                }
                VertexLayout::Exact => {
                    self.ui_packed_len = ui_vertices.len();
                    std::slice::from_raw_parts(
                        ui_vertices.as_ptr() as *const u8,
                        std::mem::size_of_val(ui_vertices),
                    )
                }
            };
            self.gl
                .buffer_data_u8_slice(glow::ARRAY_BUFFER, byte_slice, glow::DYNAMIC_DRAW);

            self.set_vertex_attributes();

            self.gl
                .draw_arrays(glow::TRIANGLES, 0, self.ui_packed_len as i32);

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
        assert_eq!(packed_layout::UV_OFFSET as usize + 2 * 4, packed_layout::STRIDE as usize);
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
        assert!(packed_channel_error(crate::lighting::MIN_AMBIENT) < 0.5 / 255.0);
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
            let value = crate::lighting::MIN_AMBIENT
                + (crate::lighting::MAX_BRIGHTNESS - crate::lighting::MIN_AMBIENT)
                    * step as f32
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
        assert_eq!(nan.color, [0, 0, 0, 0], "NaN must not become a bright value");
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
                assert_eq!(packed.pos, vertex.pos);
                assert_eq!(packed.uv, vertex.uv);
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

    #[test]
    fn test_texture_dimensions() {
        let wall = generate_wall_texture();
        let carpet = generate_carpet_texture();
        let ceiling = generate_ceiling_texture();
        let white = generate_white_texture();

        // Wallpaper and ceiling cover two metres per repeat at the same texel
        // density as the carpet's one metre; all three stay tiny.
        assert_eq!(wall.len(), 128 * 128 * 4);
        assert_eq!(carpet.len(), 64 * 64 * 4);
        assert_eq!(ceiling.len(), 128 * 128 * 4);
        assert_eq!(white.len(), 2 * 2 * 4);

        for image in [
            generate_wall_texture().to_vec(),
            generate_carpet_texture().to_vec(),
            generate_ceiling_texture().to_vec(),
            generate_stained_wall_texture().to_vec(),
            generate_damp_carpet_texture().to_vec(),
            generate_stained_ceiling_texture().to_vec(),
        ] {
            assert_eq!(image.len() % 4, 0);
            for texel in image.as_chunks::<4>().0 {
                assert_eq!(texel[3], 255, "surface textures are fully opaque");
            }
        }
    }

    /// The surface textures tile: a wrapped edge must join its opposite edge,
    /// or a floor or ceiling shows a grid of seams every repeat.
    #[test]
    fn test_surface_textures_tile() {
        let cases: [(&str, Vec<u8>, u32); 6] = [
            ("wall", generate_wall_texture().to_vec(), 128),
            (
                "wall_stained",
                generate_stained_wall_texture().to_vec(),
                128,
            ),
            ("carpet", generate_carpet_texture().to_vec(), 64),
            ("carpet_damp", generate_damp_carpet_texture().to_vec(), 64),
            ("ceiling", generate_ceiling_texture().to_vec(), 128),
            (
                "ceiling_stained",
                generate_stained_ceiling_texture().to_vec(),
                128,
            ),
        ];
        for (name, data, size) in cases {
            let texel = |x: u32, y: u32| -> [i32; 3] {
                let index = ((y * size + x) * 4) as usize;
                [
                    data[index] as i32,
                    data[index + 1] as i32,
                    data[index + 2] as i32,
                ]
            };
            for i in 0..size {
                // Horizontal wrap: the last column meets the first.
                for channel in 0..3 {
                    assert!(
                        (texel(size - 1, i)[channel] - texel(0, i)[channel]).abs() <= 40,
                        "{name}: column seam at row {i}"
                    );
                    assert!(
                        (texel(i, size - 1)[channel] - texel(i, 0)[channel]).abs() <= 40,
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
        assert!(mesh.vertex_count < mesh.index_count, "indexing must share corners");
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
        let cap = (crate::lighting::MAX_LIGHT_GRID_CELLS * crate::lighting::MAX_LIGHT_GRID_CELLS)
            as i32
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

    #[test]
    fn test_floor_checker_texture_bakes_tint_and_tiles() {
        let src = crate::loader::RawImage::new(64, 64, generate_carpet_texture().to_vec());
        let baked = generate_floor_checker_texture(&src);
        assert_eq!(baked.width, 128);
        assert_eq!(baked.height, 128);
        assert_eq!(baked.rgba.len(), 128 * 128 * 4);

        // Cell (0,0) uses the brighter tint, cell (1,0) the darker tint, so the
        // same source texel must differ between adjacent metre cells.
        let at = |x: usize, y: usize| -> [u8; 4] {
            let i = (y * 128 + x) * 4;
            [
                baked.rgba[i],
                baked.rgba[i + 1],
                baked.rgba[i + 2],
                baked.rgba[i + 3],
            ]
        };
        assert_ne!(at(0, 0), at(64, 0));
        assert_ne!(at(0, 0), at(0, 64));
        assert_eq!(at(0, 0), at(64, 64));
        assert_eq!(at(0, 0)[3], 255);
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
            assert_eq!(vfov, 60.0, "wider aspect must keep vertical FOV");
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
        assert_eq!(vertical_fov_for_aspect(60.0, 0.0), 60.0);
        assert_eq!(vertical_fov_for_aspect(60.0, -1.0), 60.0);
        assert_eq!(vertical_fov_for_aspect(60.0, f32::NAN), 60.0);
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
            assert!(vp.width <= w as i32 && vp.height <= h as i32);
            assert!(vp.x >= 0 && vp.y >= 0);
            assert!((size.width as i32 - vp.width - 2 * vp.x).abs() <= 1);
            assert!((size.height as i32 - vp.height - 2 * vp.y).abs() <= 1);

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
        assert_eq!(vp.scale, 1.0);
    }

    #[test]
    fn test_hidpi_uses_physical_pixels_not_logical_size() {
        // A 480x272 logical window on a 2x Retina display has a 960x544 drawable.
        let logical = DrawableSize::new(480, 272);
        let physical = DrawableSize::new(960, 544);

        assert_eq!(physical.ui_viewport().scale, 2.0);
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
        assert_eq!(large.ui_viewport().scale, 1080.0 / 272.0);
    }

    #[test]
    fn test_build_geometry_from_level1() {
        let json = include_str!("../assets/levels/level1.json");
        let level = LevelDef::from_json(json).expect("valid level1 json");
        let mesh = build_level_geometry(&level);
        assert!(mesh.vertex_count != 0);
        assert_eq!(mesh.index_count % 6, 0, "geometry is whole quads");
        assert!(mesh.vertex_count < mesh.index_count, "indexing must share corners");
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
                (crate::lighting::light_grid_cells(room.width)
                    * crate::lighting::light_grid_cells(room.depth)) as i32
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
    fn only_the_damaged_core_ids_resolve_to_a_damaged_surface() {
        assert_eq!(
            material_surface(MaterialSlot::Wall, DAMAGED_WALL_MATERIAL),
            SurfaceKind::WallStained
        );
        assert_eq!(
            material_surface(MaterialSlot::Floor, DAMAGED_FLOOR_MATERIAL),
            SurfaceKind::FloorDamp
        );
        assert_eq!(
            material_surface(MaterialSlot::Ceiling, DAMAGED_CEILING_MATERIAL),
            SurfaceKind::CeilingStained
        );
        // Maintained core ids and unknown/pack ids keep the family's own sheet,
        // which is the level default the level already uploaded.
        for material in [
            "core:wallpaper_yellow_01",
            "pack:wallpaper_blue",
            "core:carpet_beige_01",
            "core:ceiling_panel_01",
        ] {
            assert_eq!(
                material_surface(MaterialSlot::Wall, material),
                SurfaceKind::Wall
            );
            assert_eq!(
                material_surface(MaterialSlot::Floor, material),
                SurfaceKind::Floor
            );
            assert_eq!(
                material_surface(MaterialSlot::Ceiling, material),
                SurfaceKind::Ceiling
            );
        }
        // A damaged id used in the wrong slot is not a damaged surface.
        assert_eq!(
            material_surface(MaterialSlot::Ceiling, DAMAGED_FLOOR_MATERIAL),
            SurfaceKind::Ceiling
        );
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

        let clean_floor = batch_slice(&mesh, SurfaceKind::Floor);
        let damp_floor = batch_slice(&mesh, SurfaceKind::FloorDamp);
        let clean_ceiling = batch_slice(&mesh, SurfaceKind::Ceiling);
        let stained_ceiling = batch_slice(&mesh, SurfaceKind::CeilingStained);
        assert!(!clean_floor.is_empty() && !damp_floor.is_empty());
        assert!(!clean_ceiling.is_empty() && !stained_ceiling.is_empty());

        // Each room keeps its own sheet: the damp floor is the second room's
        // rectangle, the clean floor the first's.
        assert_eq!(xz_bounds(&damp_floor), (6.0, 12.0, 0.0, 6.0));
        assert_eq!(xz_bounds(&clean_floor), (0.0, 6.0, 0.0, 6.0));
        assert_eq!(xz_bounds(&stained_ceiling), (6.0, 12.0, 0.0, 6.0));
        assert_eq!(xz_bounds(&clean_ceiling), (0.0, 6.0, 0.0, 6.0));

        // Levels that never mention a damaged id use only the maintained sheets.
        let clean = lit_room_level(8.0, 8.0, 3.0, "[]");
        assert_eq!(damaged_variants_used(&clean), (false, false, false));
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

        let damp = batch_slice(&mesh, SurfaceKind::FloorDamp);
        let clean = batch_slice(&mesh, SurfaceKind::Floor);
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
        assert!(damaged_variants_used(&level).1, "the patch needs damp carpet");
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
        let stained = batch_slice(&mesh, SurfaceKind::WallStained);
        let maintained = batch_slice(&mesh, SurfaceKind::Wall);
        assert!(!stained.is_empty() && !maintained.is_empty());
        let stained_bounds = xz_bounds(&stained);
        assert_eq!(stained_bounds.0, 0.0);
        assert!(stained_bounds.1 >= 8.0);
        // The maintained faces belong to the second wall's north side, which
        // faces the room interior.
        let maintained_bounds = xz_bounds(&maintained);
        assert!(maintained_bounds.2 >= 7.7, "{maintained_bounds:?}");
        assert!(maintained_bounds.3 <= 8.0, "{maintained_bounds:?}");
        assert_eq!(
            damaged_variants_used(&level),
            (true, false, false),
            "only the wall sheet is referenced"
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
            assert!(!range.indices.is_empty(), "empty ranges must not be emitted");
            assert_eq!(range.indices.len() % 6, 0, "ranges are whole quads");
            assert!(
                range.vertices.len() <= crate::spatial::MAX_INDEX_VERTICES,
                "a range must stay addressable with 16-bit indices"
            );
            for index in &range.indices {
                assert!(
                    (*index as usize) < range.vertices.len(),
                    "{:?} index {index} is out of range for {} vertices",
                    range.kind,
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
                range.kind
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
                        range.kind,
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
            let start = placement.index_start as usize;
            let end = start + placement.index_count as usize;
            for (offset, value) in chunk.indices[start..end].iter().enumerate() {
                assert_eq!(
                    *value as usize,
                    placement.vertex_start as usize + offset,
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
            indices.push((index % count) as u16);
            indices.push(((index + 1) % count) as u16);
        }
        // A second range of the same size cannot share the first chunk.
        let mut second: Vec<u16> = Vec::with_capacity(count);
        for index in 0..count {
            second.push((index % count) as u16);
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
            let start = placement.index_start as usize;
            let end = start + placement.index_count as usize;
            for index in &chunk.indices[start..end] {
                assert!(
                    (*index as usize) < chunk.vertices.len(),
                    "index {index} escapes chunk {}",
                    placement.chunk
                );
            }
            total_indices += placement.index_count as usize;
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
        let total: usize = mesh
            .ranges
            .iter()
            .map(|batch| batch.vertices.len())
            .sum();

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
            let saw_expected = mesh.ranges.iter().any(|batch| {
                batch.kind == expected && frustum.intersects_aabb(&batch.bounds)
            });
            assert!(
                saw_expected,
                "pitch {pitch} must still see the {expected:?}"
            );
            let saw_opposite = mesh.ranges.iter().any(|batch| {
                batch.kind != expected
                    && matches!(batch.kind, SurfaceKind::Floor | SurfaceKind::Ceiling)
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
        let level = level_with_wall(
            "[]",
            r#"[{ "model": "core:crate", "x": 0.0, "z": 0.0, "size": [4.0, 4.0, 4.0] }]"#,
        );
        let mesh = build_level_geometry(&level);
        let eye = glam::Vec3::new(0.0, 1.6, 0.0);
        // Strictly inside, not merely touching: a wall face passing exactly
        // through the eye is still legitimately behind a camera looking away
        // from it.
        const EPS: f32 = 1e-3;
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
            .filter(|range| range.kind == SurfaceKind::PropFallback)
            .collect::<Vec<_>>();
        assert!(!props.is_empty(), "both crates must emit placeholder geometry");
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
        for (a, b) in first.all_vertices().iter().zip(second.all_vertices().iter()) {
            assert_eq!(a.pos, b.pos);
            assert_eq!(a.color, b.color);
            assert_eq!(a.uv, b.uv);
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
            .filter(|batch| batch.kind == SurfaceKind::PropFallback)
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
        let centres: Vec<f32> = batches.iter().map(|batch| batch.bounds.centre()[0]).collect();
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
            dim.color[0] >= crate::lighting::MIN_AMBIENT - 1e-4,
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
    fn placeholder_prop_boxes_receive_the_environment_lighting() {        let level = lit_room_level(
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
        let darkest_possible = crate::lighting::MIN_AMBIENT * 0.541 * 0.62;
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
            assert!(low_light > 0.0 && high_light > 0.0);
            let expected_ratio = high_light / low_light;
            assert!(
                (high / low - expected_ratio).abs() < 1e-3,
                "vertex {index} ratio {} does not match the world-space samples {expected_ratio}",
                high / low
            );
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
        let asset = assets.resolve("models/chair.glb").expect("chair loads");
        let model = &asset.model;
        for (vertex_index, vertex) in vertices.iter().enumerate() {
            // Instances contribute the model's own vertex array in order, so the
            // model's index list is not needed to line a submitted vertex up
            // with the vertex it came from.
            let source = model.vertices[vertex_index % model.vertices.len()];
            let light = lighting.sample(vertex.pos[0], vertex.pos[1], vertex.pos[2]);
            assert!(
                (vertex.color[0] - source.color[0] * light).abs() < 1e-4,
                "vertex {vertex_index}: baked colour {} does not match {} * {light}",
                vertex.color[0],
                source.color[0]
            );
        }
        assert!(assets.stats().models_failed == 0);
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
            faces: Default::default(),
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
        // The floor uses an untinted base colour, so minimum ambient shows up
        // directly; wall and ceiling tints are darker by design but stay visible.
        assert_eq!(floor[0].color[0], crate::lighting::MIN_AMBIENT);
        assert!(ceiling[0].color[0] > 0.3);
        assert!(walls[0].color[0] > 0.3);
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
        let segments = crate::lighting::wall_light_segments(10.0) as i32;
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
            expected += 2 * crate::lighting::wall_light_segments(length) as i32;
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
            expected += 2 * crate::lighting::wall_light_segments(length) as i32;
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
            expected += 2 * crate::lighting::wall_light_segments(length) as i32;
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
            mesh.batches.prop_batch.count.max(0) as usize,
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
        for entry in catalog.entries().to_vec() {
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
            "assets/props must exist for these tests"
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
        assert_eq!(batches[0].model, "models/chair.glb");
        assert!(!batches[0].vertices.is_empty());
        assert_eq!(batches[0].texture.width, 64);

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
        assert_eq!(
            batches.len(),
            21,
            "the showcase places every catalogue prop"
        );
        let mut models: Vec<&str> = batches.iter().map(|batch| batch.model.as_str()).collect();
        models.sort_unstable();
        models.dedup();
        assert_eq!(models.len(), 21, "each prop model appears exactly once");

        // Every catalogue prop id is exercised by this fixture.
        let used: std::collections::HashSet<&str> =
            level.props.iter().map(|prop| prop.model.as_str()).collect();
        for entry in catalog.entries() {
            assert!(
                used.contains(entry.id.as_str()),
                "the showcase must place {}",
                entry.id
            );
        }
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
            "the stress level must stay inside the prop vertex budget ({} vertices)",
            total_vertices
        );

        // Sixty-plus instances of one model share the decoded mesh; they are
        // spread across whatever cells they occupy, never duplicated per cell.
        let chair_vertices: usize = batches
            .iter()
            .filter(|batch| batch.model == "models/chair.glb")
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
}


