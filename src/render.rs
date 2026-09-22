use glow::HasContext;

use crate::font::generate_font_atlas;
use crate::level::{
    FloorPatchDef, LevelDef, LevelSurfaces, PropDef, RoomDef, RoomFloorGrid, WallAxis, WallDef,
    wall_solid_slices_profiled,
};
use crate::lighting::{LevelLighting, LightColor, wall_light_segments};
use crate::materials::{MaterialTable, ResolvedMaterial};

mod api;
mod decals;
mod fixtures;
mod geometry;
mod mesh;
mod props;
mod renderer;
mod view;

pub use api::{
    BuildTimings, build_level_geometry, build_level_geometry_timed,
    build_level_geometry_with_assets, build_level_geometry_with_assets_and_lighting,
    build_level_geometry_with_assets_and_lighting_and_materials, build_level_geometry_with_catalog,
    build_level_geometry_with_catalog_and_materials, build_level_geometry_with_materials,
    logical_materials,
};
pub use decals::{
    DECAL_EXTERNAL_BASE, DECAL_MATERIALS, DECAL_TEST_MATERIAL, decal_external_sheet_ids,
    decal_material_slot, decal_sheet_index, decal_uv_rect, decal_uv_rect_full,
};
use fixtures::{add_panel_fixture, add_round_fixture, add_wall_fixture};
use geometry::build_level_geometry_mesh;
pub use mesh::packed_layout;
pub use mesh::{
    BatchRange, LevelMesh, LevelMeshBatches, LevelMeshRange, MATERIAL_NONE, MaterialIndex,
    MaterialSlot, PackedVertex, StaticBatch, SurfaceKey, SurfaceKind, Vertex, VertexLayout,
    dequantize_unit, spatial_cell_grid,
};
use mesh::{MeshChunk, MeshPacker, finish_indexed_mesh};
pub use props::PropMeshBatch;
pub use renderer::{LevelBuildStats, RenderStats, Renderer};
pub use view::{
    DECAL_ALPHA_CUTOFF, DECAL_POLYGON_OFFSET, DrawableSize, UI_REFERENCE_HEIGHT,
    UI_REFERENCE_WIDTH, UiViewport, WINDOW_HEIGHT, WINDOW_WIDTH, reference_aspect_ratio,
    vertical_fov_for_aspect,
};
use view::{
    DECAL_FRAGMENT_SHADER_SRC, FRAGMENT_SHADER_SRC, SCENE_ATTRIB_COLOR, SCENE_ATTRIB_POS,
    SCENE_ATTRIB_UV, VERTEX_SHADER_SRC,
};

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
/// whichever room the boundary point happens to fall in. The bake resolves the
/// face's room with the same probe (`lighting::WALL_FACE_PROBE_M`).
const LIGHT_FACE_PROBE_M: f32 = crate::lighting::WALL_FACE_PROBE_M;

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
    // The room itself is resolved from the middle of the face, which is
    // unambiguous, so a face that runs along a shared boundary is lit by the
    // room it opens into instead of by whichever room the tie-break preferred.
    let mid = f32::midpoint(l0, l1);
    let (mid_x, mid_z, normal_x, normal_z) = match axis {
        WallAxis::X => (mid, face, 0.0, normal),
        WallAxis::Z => (face, mid, normal, 0.0),
    };
    let face_room = lighting.face_room(mid_x, mid_z, normal_x, normal_z);
    let color = |at: f32, y: f32, base: [f32; 3]| -> [f32; 3] {
        let probe = match axis {
            WallAxis::X => [at, y, normal.mul_add(LIGHT_FACE_PROBE_M, face)],
            WallAxis::Z => [normal.mul_add(LIGHT_FACE_PROBE_M, face), y, at],
        };
        shade(
            base,
            lighting.sample_face(face_room, probe[0], probe[1], probe[2]),
        )
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

/// Tolerance for treating two walls as occupying the same plane, in metres.
///
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

    let region_owner = |ix: usize, iz: usize| -> Option<&crate::level::FloorRegionDef> {
        let (x0, x1, z0, z1) = room.bounds();
        let x = f32::midpoint(grid.xs[ix], grid.xs[ix + 1]);
        let z = f32::midpoint(grid.zs[iz], grid.zs[iz + 1]);
        level.floor_regions.iter().rev().find(|region| {
            let (rx0, rx1, rz0, rz1) = region.bounds();
            rx1 > x0 && rx0 < x1 && rz1 > z0 && rz0 < z1 && region.contains(x, z)
        })
    };

    // The region owning a cell is the one whose material describes the faces
    // that cell's height difference creates.
    let edge_key = |region: Option<&crate::level::FloorRegionDef>| -> SurfaceKey {
        region
            .and_then(|region| region.edge_material.as_deref())
            .map_or(default_edge, |material| {
                materials.key(MaterialSlot::Wall, material)
            })
    };

    // A transition face belongs to the region that owns the height change, and
    // that region can be on either side of it. Only the lower-indexed cell of an
    // adjacent pair emits their shared face, so a recess on that cell's side
    // would otherwise be keyed by the cell outside the recess and fall back to
    // the room's wall material instead of the region's `edge_material`.
    let face_key = |inside: (usize, usize), outside: (usize, usize)| -> SurfaceKey {
        let (ix, iz) = inside;
        let (ox, oz) = outside;
        edge_key(region_owner(ix, iz).or_else(|| region_owner(ox, oz)))
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
                    face_key((ix + 1, iz), (ix, iz))
                } else {
                    edge_key(region_owner(ix, iz))
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
                    face_key((ix, iz + 1), (ix, iz))
                } else {
                    edge_key(region_owner(ix, iz))
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

#[cfg(test)]
mod tests;
