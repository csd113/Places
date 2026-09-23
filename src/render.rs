use std::cell::RefCell;

use glow::HasContext;

use crate::font::generate_font_atlas;
use crate::level::{
    FloorPatchDef, LevelDef, LevelSurfaces, MaterialRef, PropDef, RoomDef, RoomFloorGrid, WallAxis,
    WallDef, WallSlice, wall_solid_slices_profiled,
};
use crate::lighting::lightmap::{LightmapPlan, PatchKind};
use crate::lighting::{LevelLighting, LightColor, wall_light_segments};
use crate::materials::{MaterialTable, ResolvedMaterial};

mod animation;
mod api;
mod atmosphere;
mod decals;
mod dynamic;
mod fixtures;
mod framebuffer;
mod geometry;
mod mesh;
mod postprocess;
mod props;
mod reflections;
mod renderer;
mod view;

pub use animation::{
    AnimationEffect, EmissionAnimation, MAX_ANIMATION_DEPTH, MAX_FLICKER_HZ, MAX_PULSE_HZ,
};
pub use api::{
    BuildTimings, LevelBuild, LightmapBuildOptions, build_level_geometry,
    build_level_geometry_timed, build_level_geometry_timed_with_lightmaps,
    build_level_geometry_with_assets, build_level_geometry_with_assets_and_lighting,
    build_level_geometry_with_assets_and_lighting_and_materials, build_level_geometry_with_catalog,
    build_level_geometry_with_catalog_and_materials, build_level_geometry_with_materials,
    logical_materials,
};
pub use decals::{
    DECAL_EXTERNAL_BASE, DECAL_MATERIALS, DECAL_TEST_MATERIAL, decal_external_sheet_ids,
    decal_material_slot, decal_sheet_index, decal_uv_rect, decal_uv_rect_full,
};
pub use dynamic::{
    DEMO_DRUM_ID, DEMO_MACHINE_ID, DEMO_SPIN_DEGREES_PER_SECOND, DynamicId, DynamicMesh,
    DynamicObject, DynamicScene, DynamicSubmesh, DynamicUpdate, MAX_DYNAMIC_MESHES,
    MAX_DYNAMIC_OBJECTS, PROBE_EPSILON_M,
};
use fixtures::{add_panel_fixture, add_round_fixture, add_wall_fixture};
use geometry::build_level_geometry_mesh;
pub use mesh::packed_layout;
pub use mesh::{
    BatchRange, EXACT_VERTEX_STRIDE, LIGHTMAP_NONE, LevelMesh, LevelMeshBatches, LevelMeshRange,
    MATERIAL_NONE, MaterialIndex, MaterialSlot, PackedVertex, StaticBatch, SurfaceKey, SurfaceKind,
    SurfaceShine, Vertex, VertexLayout, dequantize_normal, dequantize_unit, exact_layout,
    spatial_cell_grid,
};
use mesh::{MeshChunk, MeshPacker, finish_indexed_mesh};
pub use props::PropMeshBatch;
pub use renderer::{LevelBuildStats, RenderStats, Renderer};

/// Near and far plane of the scene projection, in metres.
///
/// The near plane is the usual first-person 10 cm: the player's collision keeps
/// the eye about a radius from every wall, so raising it further would buy
/// precision the game does not need while risking a clip when leaning into a
/// corner. The far plane covers the largest shipped level several times over.
///
/// The two multiply the *depth buffer's* precision: with a 24-bit fixed-point
/// buffer the eye-space resolution at distance `z` is
/// `z^2 * (far - near) / (far * near) * 2^-24`, i.e. about 0.6 µm at 1 m,
/// 60 µm at 10 m and 6 mm at the far plane. A decal's whole depth bias is two
/// of those steps, so it can never visibly lift a marking off its surface.
pub(crate) const SCENE_NEAR_M: f32 = 0.1;
pub(crate) const SCENE_FAR_M: f32 = 100.0;
pub use view::{
    DECAL_ALPHA_CUTOFF, DECAL_POLYGON_OFFSET, DECAL_SURFACE_OFFSET_M, DrawableSize,
    UI_REFERENCE_HEIGHT, UI_REFERENCE_WIDTH, UiViewport, WINDOW_HEIGHT, WINDOW_WIDTH,
    fragment_shader_source, reference_aspect_ratio, vertical_fov_for_aspect,
};
use view::{
    DECAL_FRAGMENT_SHADER_SRC, EMISSION_MASK_TEXTURE_UNIT, LIGHTMAP_PAGE_SLOTS,
    LIGHTMAP_TEXTURE_UNIT, LIGHTMAP_TEXTURE_UNIT_1, NORMAL_MAP_TEXTURE_UNIT,
    PRESENT_FRAGMENT_SHADER_SRC, PRESENT_TEXTURE_UNIT, PRESENT_VERTEX_SHADER_SRC,
    REFLECTION_PLANAR_TEXTURE_UNIT, REFLECTION_PROBE_TEXTURE_UNIT, SCENE_ATTRIB_COLOR,
    SCENE_ATTRIB_COUNT, SCENE_ATTRIB_HANDEDNESS, SCENE_ATTRIB_LIGHTMAP_PAGE,
    SCENE_ATTRIB_LIGHTMAP_UV, SCENE_ATTRIB_NORMAL, SCENE_ATTRIB_POS, SCENE_ATTRIB_TANGENT,
    SCENE_ATTRIB_UV, SCENE_TEXTURE_UNIT, VERTEX_SHADER_SRC,
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

    /// The surface key for one slot and material reference.
    ///
    /// The reference's optional shine override becomes the key's quantised
    /// [`SurfaceShine`], so two surfaces that share a material but not a shine
    /// value batch separately and draw with their own glossiness.
    fn key(&self, slot: MaterialSlot, material: MaterialRef<'_>) -> SurfaceKey {
        SurfaceKey::with_shine(
            slot.kind(),
            self.index(material.id),
            material.shine.map(SurfaceShine::from_unit),
        )
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
        ..Vertex::UNLIT
    });
    vertices.push(Vertex {
        pos: p1,
        color: col1,
        uv: uv1,
        ..Vertex::UNLIT
    });
    vertices.push(Vertex {
        pos: p2,
        color: col2,
        uv: uv2,
        ..Vertex::UNLIT
    });
    vertices.push(Vertex {
        pos: p0,
        color: col0,
        uv: uv0,
        ..Vertex::UNLIT
    });
    vertices.push(Vertex {
        pos: p2,
        color: col2,
        uv: uv2,
        ..Vertex::UNLIT
    });
    vertices.push(Vertex {
        pos: p3,
        color: col3,
        uv: uv3,
        ..Vertex::UNLIT
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

/// Converts a renderer count (segment, vertex or pixel index) to `f32`.
///
/// Every count in this module is bounded by the level geometry and GPU buffer
/// sizes, far below 2^24, where an `usize` to `f32` conversion is exact.
#[allow(clippy::cast_precision_loss)] // counts are < 2^24, where f32 is exact
const fn count_to_f32(value: usize) -> f32 {
    value as f32
}

/// Baked brightness sampled at each of four quad corners.
fn lit_corners(base: [f32; 3], points: [[f32; 3]; 4], lighting: &LevelLighting) -> [[f32; 3]; 4] {
    points.map(|point| shade(base, lighting.sample(point[0], point[1], point[2])))
}

/// A plan shared with the emitters through the emit context.
///
/// The builder owns the plan and lends it as a [`RefCell`] so a `&EmitContext`
/// can stamp quads without threading `&mut` through every emitter signature.
/// Every stamp borrows for the length of one quad via `try_borrow_mut` and
/// never holds a borrow across another stamp.
pub(crate) type SharedLightmapPlan<'p> = RefCell<&'p mut LightmapPlan>;

/// The lightmap state one emitter needs: the shared plan, when the build is
/// lightmapped, plus the longest chart span its merges may produce.
///
/// `Copy`, so an emitter can take it by value out of the emit context and hand
/// it to a nested helper without borrowing anything. `plan: None` is the
/// historical vertex-lit path, where the emitters must compute exactly the
/// colours and vertices they always did.
#[derive(Clone, Copy)]
pub(crate) struct LightmapEmit<'a> {
    /// The plan to stamp quads into; `None` means vertex-lit.
    pub(crate) plan: Option<&'a SharedLightmapPlan<'a>>,
    /// Longest merged quad, in metres, a lightmapped build may emit.
    pub(crate) max_span_m: f32,
}

impl LightmapEmit<'_> {
    /// True when this build bakes light into an atlas.
    pub(crate) const fn is_on(&self) -> bool {
        self.plan.is_some()
    }
}

/// Stamps the six vertices of one just-emitted quad with a lightmap chart.
///
/// `first` is the index of the quad's first vertex and `corners` are its four
/// corners in the emitter's own winding (`u = p0 -> p1`, `v = p0 -> p3`). A
/// `None` lightmap is the historical vertex-lit path and does nothing at all.
pub(crate) fn stamp_lightmap_quad(
    lightmap: Option<LightmapEmit<'_>>,
    vertices: &mut [Vertex],
    first: usize,
    kind: PatchKind,
    corners: [[f32; 3]; 4],
    room: Option<usize>,
) {
    let Some(plan) = lightmap.and_then(|lightmap| lightmap.plan) else {
        return;
    };
    let Ok(mut plan) = plan.try_borrow_mut() else {
        return;
    };
    let _ = plan.stamp_emitted(vertices, first, kind, corners, room);
}

/// Splits `(lo, hi)` into sub-intervals of at most `max_span` length each.
///
/// Only used on the lightmapped path: with a non-finite span (the vertex-lit
/// fallback) the original interval is returned unchanged, so no emitter that
/// calls this can drift from its historical coordinates.
pub(crate) fn split_span(lo: f32, hi: f32, max_span: f32) -> Vec<(f32, f32)> {
    let span = hi - lo;
    if !max_span.is_finite() || !span.is_finite() || span <= max_span || span <= 1.0e-6 {
        return vec![(lo, hi)];
    }
    let pieces = (span / max_span).ceil().clamp(1.0, 64.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // `pieces` is clamped to [1, 64] before the cast.
    let count = pieces as u32;
    let step = span / f32::from(u16::try_from(count).unwrap_or(u16::MAX));
    (0..count)
        .map(|index| {
            let index_f = f32::from(u16::try_from(index).unwrap_or(u16::MAX));
            let start = index_f.mul_add(step, lo);
            let end = if index.saturating_add(1) >= count {
                hi
            } else {
                start + step
            };
            (start, end)
        })
        .collect()
}

/// Splits a `(a0, a1, b0, b1)` rectangle into tiles no larger than `max_span`
/// on either axis. The whole rectangle is returned unchanged when `max_span`
/// is not finite, which is the vertex-lit fallback.
pub(crate) fn split_rect(rect: (f32, f32, f32, f32), max_span: f32) -> Vec<(f32, f32, f32, f32)> {
    if !max_span.is_finite() {
        return vec![rect];
    }
    let (a0, a1, b0, b1) = rect;
    let mut out = Vec::new();
    for (sa0, sa1) in split_span(a0, a1, max_span) {
        for (sb0, sb1) in split_span(b0, b1, max_span) {
            out.push((sa0, sa1, sb0, sb1));
        }
    }
    out
}

/// One wall face parallel to the wall's length axis, ready to emit as a strip
/// of quads.
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
struct WallFaceStrip<'a, Y: Fn(f32) -> f32> {
    /// The wall's length axis.
    axis: WallAxis,
    /// World position of the face across the wall's thickness.
    face: f32,
    /// Outward normal, `-1.0` or `1.0` across the thickness.
    normal: f32,
    /// World start of the face along the length axis.
    l0: f32,
    /// World end of the face along the length axis.
    l1: f32,
    /// World Y of the face's bottom edge.
    bottom: f32,
    /// The face's top edge at a length offset.
    top_at: Y,
    /// Shaded colour of the face's bottom edge.
    bottom_shade: [f32; 3],
    /// Shaded colour of the face's top edge.
    top_shade: [f32; 3],
    /// Whether the winding runs against the length axis.
    reversed: bool,
    /// Whether `u` is negated so the face reads unmirrored from its own side.
    flip_u: bool,
    lighting: &'a LevelLighting,
    tile_metres: f32,
    /// `None` is the historical vertex-lit path.
    lightmap: Option<LightmapEmit<'a>>,
}

impl<Y: Fn(f32) -> f32> WallFaceStrip<'_, Y> {
    /// A world point on the face.
    const fn point(&self, at: f32, y: f32) -> [f32; 3] {
        match self.axis {
            WallAxis::X => [at, y, self.face],
            WallAxis::Z => [self.face, y, at],
        }
    }

    /// The vertex colour at one edge: the baked sample on the historical path,
    /// the plain shaded base when the atlas carries the light.
    fn color(&self, at: f32, y: f32, base: [f32; 3], face_room: Option<usize>) -> [f32; 3] {
        if self.lightmap.is_some_and(|lightmap| lightmap.is_on()) {
            // The atlas carries the baked light; the vertex colour is the
            // material tint and the face's directional shade only.
            return base;
        }
        let probe = match self.axis {
            WallAxis::X => [at, y, self.normal.mul_add(LIGHT_FACE_PROBE_M, self.face)],
            WallAxis::Z => [self.normal.mul_add(LIGHT_FACE_PROBE_M, self.face), y, at],
        };
        shade(
            base,
            self.lighting
                .sample_face(face_room, probe[0], probe[1], probe[2]),
        )
    }

    /// World-space tile UV of a point, at the face's own V reference.
    fn uv(&self, v_ref: f32, at: f32, y: f32) -> [f32; 2] {
        let u = if self.flip_u { -at } else { at };
        tiled_uv(u, v_ref - y, self.tile_metres)
    }

    /// Samples the lighting once per segment boundary.
    ///
    /// Adjacent segments share a corner, so each boundary is sampled exactly
    /// once (a 2x saving) and a merged strip keeps a single value at every
    /// surviving edge.
    fn boundaries(&self, face_room: Option<usize>) -> Vec<WallBoundary> {
        let segments = wall_light_segments((self.l1 - self.l0).abs());
        let boundary_count = segments as usize + 1;
        let mut boundaries: Vec<WallBoundary> = Vec::with_capacity(boundary_count);
        for boundary in 0..boundary_count {
            let at = self.l0
                + (self.l1 - self.l0) * count_to_f32(boundary) / count_to_f32(segments as usize);
            let top = (self.top_at)(at);
            boundaries.push((
                at,
                top,
                self.color(at, self.bottom, self.bottom_shade, face_room),
                self.color(at, top, self.top_shade, face_room),
            ));
        }
        boundaries
    }

    /// Emits the face's merged lighting runs as quads.
    fn emit(&self, vertices: &mut Vec<Vertex>) {
        // Probe inside the room this face looks into, so the wall is lit by its
        // own side of the wall even when the surface sits exactly on a room
        // boundary. The room itself is resolved from the middle of the face,
        // which is unambiguous, so a face that runs along a shared boundary is
        // lit by the room it opens into instead of by whichever room the
        // tie-break preferred.
        let mid = f32::midpoint(self.l0, self.l1);
        let (mid_x, mid_z, normal_x, normal_z) = match self.axis {
            WallAxis::X => (mid, self.face, 0.0, self.normal),
            WallAxis::Z => (self.face, mid, self.normal, 0.0),
        };
        let face_room = self.lighting.face_room(mid_x, mid_z, normal_x, normal_z);
        // The V reference keeps the image's top row at the face's top while
        // staying constant along the face, so tiling never breaks across a
        // gable slope or a merged lighting run.
        let uv_v_ref = (self.top_at)(self.l0);
        let segments = wall_light_segments((self.l1 - self.l0).abs());
        let boundaries = self.boundaries(face_room);
        let max_span_m = self
            .lightmap
            .map_or(f32::INFINITY, |lightmap| lightmap.max_span_m);
        for (start, end) in merge_light_runs(&boundaries, segments as usize, max_span_m) {
            let (
                Some(&(at_start, top_start, bottom_start, top_color_start)),
                Some(&(at_end, top_end, bottom_end, top_color_end)),
            ) = (boundaries.get(start), boundaries.get(end))
            else {
                continue;
            };
            // Walk the strip from `start` to `end`, or the other way round when
            // the wall is reversed, so the quad keeps one consistent winding.
            let (
                first_at,
                first_top,
                first_bottom,
                first_top_color,
                second_at,
                second_top,
                second_bottom,
                second_top_color,
            ) = if self.reversed {
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
            let corners = [
                self.point(first_at, first_top),
                self.point(second_at, second_top),
                self.point(second_at, self.bottom),
                self.point(first_at, self.bottom),
            ];
            let first = vertices.len();
            add_quad(
                vertices,
                corners[0],
                first_top_color,
                self.uv(uv_v_ref, first_at, first_top),
                corners[1],
                second_top_color,
                self.uv(uv_v_ref, second_at, second_top),
                corners[2],
                second_bottom,
                self.uv(uv_v_ref, second_at, self.bottom),
                corners[3],
                first_bottom,
                self.uv(uv_v_ref, first_at, self.bottom),
            );
            stamp_lightmap_quad(
                self.lightmap,
                vertices,
                first,
                PatchKind::Wall,
                corners,
                face_room,
            );
        }
    }
}

/// Emits one wall face parallel to the wall's length axis as a strip of quads.
#[allow(clippy::too_many_arguments)] // one emitter per face; see `WallFaceStrip`
fn add_wall_length_face<'a>(
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
    lighting: &'a LevelLighting,
    tile_metres: f32,
    lightmap: Option<LightmapEmit<'a>>,
) {
    let strip = WallFaceStrip {
        axis,
        face,
        normal,
        l0,
        l1,
        bottom,
        top_at,
        bottom_shade,
        top_shade,
        reversed,
        flip_u,
        lighting,
        tile_metres,
        lightmap,
    };
    strip.emit(vertices);
}

/// One boundary sample of a wall length face: length offset, top edge and the
/// shaded colours of the bottom and top edge there.
type WallBoundary = (f32, f32, [f32; 3], [f32; 3]);

/// Merges adjacent boundary samples whose colours and top edge are flat.
///
/// Returns the surviving `(start, end)` boundary index pairs, each covering one
/// emitted quad. Adjacent segments share a corner, so every boundary is sampled
/// once and a merged strip keeps a single value at each surviving edge.
///
/// `max_span_m` caps how long one merged quad may become: lightmapped builds
/// pass the chart budget so every emitted face fits one atlas chart, while the
/// vertex-lit fallback passes an infinite span and merges exactly as it always
/// did. The cap is checked on the run's own far boundary, including the face's
/// last boundary, so a run can never grow past it; a run always covers at least
/// one segment.
fn merge_light_runs(
    boundaries: &[WallBoundary],
    segments: usize,
    max_span_m: f32,
) -> Vec<(usize, usize)> {
    let matches_run = |reference: &WallBoundary, candidate: &WallBoundary| {
        (candidate.0 - reference.0).abs() <= max_span_m
            && reference
                .2
                .iter()
                .zip(&candidate.2)
                .all(|(reference, candidate)| (candidate - reference).abs() <= LIGHT_GRID_MERGE_EPS)
            && reference
                .3
                .iter()
                .zip(&candidate.3)
                .all(|(reference, candidate)| (candidate - reference).abs() <= LIGHT_GRID_MERGE_EPS)
            && (candidate.1 - reference.1).abs() <= HEIGHT_MERGE_EPS
    };
    let mut runs = Vec::new();
    let mut start = 0;
    while start < segments {
        let Some(reference) = boundaries.get(start) else {
            break;
        };
        // `end` is the run's far boundary. It starts one segment ahead (a run
        // always covers at least one segment) and grows while the boundary at
        // `end` still matches the reference, so the face's final boundary is
        // tested like every other one.
        let mut end = start.saturating_add(1);
        while end <= segments
            && boundaries
                .get(end)
                .is_some_and(|candidate| matches_run(reference, candidate))
        {
            end = end.saturating_add(1);
        }
        // `end` is the first boundary that does not belong to the run (or
        // `segments + 1` when the whole face does), so the run ends one short.
        let end = end.saturating_sub(1).max(start.saturating_add(1));
        if boundaries.get(end).is_none() {
            break;
        }
        runs.push((start, end));
        start = end;
    }
    runs
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

/// One material run of a coalesced wall group: a rectangle in the group's own
/// (length, height) space over which the visible material is constant.
///
/// A run is the intersection of the group's solid volume with one authored
/// member's solid volume, so a wall that is coincident with another only over
/// part of its height (a longer wall in the next room, an overlay that stops at
/// a skirting) still contributes exactly the material the authored surfaces
/// used to show, once. The faces are ordered like the two length faces the
/// emitter walks: the low-thickness face first (north on an X-axis wall, west
/// on a Z-axis wall), the high-thickness face second (south/east).
#[derive(Clone, Copy, Debug, PartialEq)]
struct WallMaterialRun {
    start: f32,
    end: f32,
    bottom: f32,
    top: f32,
    faces: [SurfaceKey; 2],
    /// Key used by sills, headers and reveals inside this run.
    body: SurfaceKey,
}

/// One wall the geometry builder emits.
///
/// A wall on its own is emitted exactly as authored. Several walls that
/// overlap in the same plane (same axis and thickness span, overlapping length
/// and overlapping height) are coincident geometry: a material overlay, a wall
/// continued into the next room, a second slab sharing a corner. They are
/// resolved into one synthetic wall whose solid volume is the *union* of the
/// group's solid patches and whose (length, height) cells carry the last
/// covering member's material, so every surface is emitted once and there is no
/// second coplanar mesh to fight for the same depth value.
pub(in crate::render) enum WallUnit<'a> {
    Plain {
        index: usize,
        wall: &'a WallDef,
    },
    Coalesced {
        /// Every authored wall index this unit resolves.
        members: Vec<usize>,
        /// The synthetic wall, boxed: it is materialised per group and much
        /// larger than a plain unit's borrowed reference, so the enum stays
        /// small by value.
        wall: Box<WallDef>,
        /// The group's solid profile in the unit wall's local length space.
        slices: Vec<WallSlice>,
        runs: Vec<WallMaterialRun>,
    },
}

impl WallUnit<'_> {
    const fn wall(&self) -> &WallDef {
        match self {
            Self::Plain { wall, .. } => wall,
            Self::Coalesced { wall, .. } => wall,
        }
    }

    /// Every authored wall index whose volume this unit emits.
    fn members(&self) -> Vec<usize> {
        match self {
            Self::Plain { index, .. } => vec![*index],
            Self::Coalesced { members, .. } => members.clone(),
        }
    }

    /// The unit's solid profile, in the unit wall's local length space.
    ///
    /// A plain wall cuts its own openings; a coalesced group carries the union
    /// already resolved by [`coalesce_wall_group`].
    fn slices(&self, surfaces: &LevelSurfaces<'_>) -> Vec<WallSlice> {
        match self {
            Self::Plain { wall, .. } => {
                let breaks = surfaces.wall_profile_breaks(wall);
                wall_solid_slices_profiled(
                    wall,
                    |offset| surfaces.clear_ceiling_height_along(wall, offset),
                    &breaks,
                )
            }
            Self::Coalesced { slices, .. } => slices.clone(),
        }
    }

    /// The material run covering a local length position at a world height, if
    /// this unit was coalesced. Plain walls keep their authored per-face
    /// materials.
    fn run_at(&self, position: f32, y: f32) -> Option<&WallMaterialRun> {
        match self {
            Self::Plain { .. } => None,
            Self::Coalesced { runs, .. } => runs.iter().find(|run| {
                position >= run.start - WALL_COINCIDENCE_EPS
                    && position <= run.end + WALL_COINCIDENCE_EPS
                    && y >= run.bottom - WALL_COINCIDENCE_EPS
                    && y <= run.top + WALL_COINCIDENCE_EPS
            }),
        }
    }

    /// Material runs intersecting a local length span and world height span,
    /// clipped to both.
    ///
    /// Empty for a plain wall, which draws its authored material across the
    /// whole face.
    fn runs_between(&self, start: f32, end: f32, bottom: f32, top: f32) -> Vec<WallMaterialRun> {
        match self {
            Self::Plain { .. } => Vec::new(),
            Self::Coalesced { runs, .. } => runs
                .iter()
                .filter_map(|run| {
                    let low = run.start.max(start);
                    let high = run.end.min(end);
                    let run_bottom = run.bottom.max(bottom);
                    let run_top = run.top.min(top);
                    (high - low > WALL_COINCIDENCE_EPS
                        && run_top - run_bottom > WALL_COINCIDENCE_EPS)
                        .then_some(WallMaterialRun {
                            start: low,
                            end: high,
                            bottom: run_bottom,
                            top: run_top,
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

/// The two length-face keys and the body key of one wall.
fn wall_material_keys(
    wall: &WallDef,
    default_wall: MaterialRef<'_>,
    materials: &MaterialLookup<'_>,
) -> ([SurfaceKey; 2], SurfaceKey) {
    let wall_ref = wall.material_ref().unwrap_or(default_wall);
    let axis = wall.axis();
    let (low_name, high_name) = match axis {
        WallAxis::X => ("north", "south"),
        WallAxis::Z => ("west", "east"),
    };
    let face = |name: &str| {
        materials.key(
            MaterialSlot::Wall,
            wall.face_ref(name).unwrap_or(default_wall),
        )
    };
    (
        [face(low_name), face(high_name)],
        materials.key(MaterialSlot::Wall, wall_ref),
    )
}

/// Root of a union-find set, with path compression.
///
/// Indices always come from the wall enumeration, so they stay in range; a
/// missing entry simply ends the walk instead of panicking.
fn find_root(parent: &mut [usize], index: usize) -> usize {
    let mut root = index;
    loop {
        let Some(&next) = parent.get(root) else {
            return root;
        };
        if next == root {
            break;
        }
        root = next;
    }
    let mut cursor = index;
    while let Some(&next) = parent.get(cursor) {
        if next == root {
            break;
        }
        if let Some(slot) = parent.get_mut(cursor) {
            *slot = root;
        }
        cursor = next;
    }
    root
}

/// Resolves coincident collinear walls into single emission units.
///
/// The authored levels paint part of a wall with water damage by
/// placing a second wall in exactly the same plane with a stained material.
/// That is a material overlay represented as duplicate geometry, and depending
/// on submission order the two identical surfaces fight for the same depth
/// value. Here such walls are grouped, the group's solid profile is unioned
/// (an opaque coincident face covers a hole in the other surface, which is
/// what the renderer already showed) and the group is emitted once with a
/// material run per span. Walls that merely overlap without sharing a plane
/// are untouched; collision keeps using the authored walls.
/// The coincidence-resolved emission units alone, for tests and audits that do
/// not need the cross-section coverage set.
#[cfg(test)]
fn wall_units<'a>(
    level: &'a LevelDef,
    surfaces: &LevelSurfaces<'_>,
    materials: &MaterialLookup<'_>,
) -> Vec<WallUnit<'a>> {
    wall_layout(level, surfaces, materials).units
}

/// One authored wall's solid patch, in world length coordinates, for the
/// group union.
#[derive(Clone, Copy, Debug)]
struct MemberSolid {
    index: usize,
    start: f32,
    end: f32,
    bottom: f32,
    top: f32,
}

/// The resolved wall emission plan for one level build.
pub(in crate::render) struct WallLayout<'a> {
    /// One emission unit per wall or coincident group, in authored order.
    pub units: Vec<WallUnit<'a>>,
    /// Every authored wall's solid volume, for the cross-section coverage test
    /// that keeps wall faces from being emitted underneath an abutting wall.
    pub coverages: Vec<WallCoverage>,
}

pub(in crate::render) fn wall_layout<'a>(
    level: &'a LevelDef,
    surfaces: &LevelSurfaces<'_>,
    materials: &MaterialLookup<'_>,
) -> WallLayout<'a> {
    let default_wall = level.defaults.wall_ref();
    let walls = &level.walls;
    let slabs: Vec<Option<WallSlab>> = walls.iter().map(|wall| wall_slab(wall, surfaces)).collect();
    let groups = wall_groups(&slabs);

    let mut units: Vec<WallUnit<'a>> = Vec::with_capacity(groups.len());
    for group in groups {
        if group.len() == 1 {
            if let Some(index) = group.first().copied()
                && let Some(wall) = walls.get(index)
            {
                units.push(WallUnit::Plain { index, wall });
            }
            continue;
        }
        if let Some(unit) =
            coalesce_wall_group(&group, &slabs, walls, surfaces, materials, default_wall)
        {
            units.push(unit);
        }
    }
    let coverages = walls
        .iter()
        .enumerate()
        .map(|(index, wall)| wall_coverage(index, wall, surfaces))
        .collect();
    WallLayout { units, coverages }
}

/// Groups coincident collinear wall slabs transitively (union-find over wall
/// indices), in first-appearance order so the emitted range order stays
/// deterministic and follows the authored wall order.
///
/// Two walls group when they share a plane and a thickness span and overlap in
/// *both* their length span and their height span. Requiring the spans to
/// overlap rather than to be equal is what lets a wall continued into the next
/// room (a different base or top) resolve into the same single surface as the
/// wall it continues from.
fn wall_groups(slabs: &[Option<WallSlab>]) -> Vec<Vec<usize>> {
    let mut parent: Vec<usize> = (0..slabs.len()).collect();
    for (i, slab) in slabs.iter().enumerate() {
        let Some(a) = *slab else { continue };
        for (j, other) in slabs.iter().enumerate().skip(i.saturating_add(1)) {
            let Some(b) = *other else { continue };
            if a.axis != b.axis
                || (a.thickness.0 - b.thickness.0).abs() > WALL_COINCIDENCE_EPS
                || (a.thickness.1 - b.thickness.1).abs() > WALL_COINCIDENCE_EPS
            {
                continue;
            }
            // The two footprints must overlap for the walls to share a surface.
            let (a_start, a_end) = a.length;
            let (b_start, b_end) = b.length;
            let shared_start = a_start.max(b_start);
            let shared_end = a_end.min(b_end);
            if shared_end - shared_start <= WALL_COINCIDENCE_EPS {
                continue;
            }
            let shared_base = a.base.max(b.base);
            let shared_top = a.top.min(b.top);
            if shared_top - shared_base <= WALL_COINCIDENCE_EPS {
                continue;
            }
            let (root_a, root_b) = (find_root(&mut parent, i), find_root(&mut parent, j));
            if root_a != root_b
                && let Some(slot) = parent.get_mut(root_b)
            {
                *slot = root_a;
            }
        }
    }

    let mut group_of: Vec<Option<usize>> = vec![None; slabs.len()];
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for (i, slab) in slabs.iter().enumerate() {
        if slab.is_none() {
            continue;
        }
        let root = find_root(&mut parent, i);
        let group_index = group_of.get(root).copied().flatten().unwrap_or_else(|| {
            let index = groups.len();
            if let Some(slot) = group_of.get_mut(root) {
                *slot = Some(index);
            }
            groups.push(Vec::new());
            index
        });
        if let Some(group) = groups.get_mut(group_index) {
            group.push(i);
        }
    }
    groups
}

/// Union length span of a group of coincident walls.
fn group_union_span(group: &[usize], slabs: &[Option<WallSlab>]) -> (f32, f32) {
    group
        .iter()
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), index| {
            let (start, end) = slabs
                .get(*index)
                .copied()
                .flatten()
                .map_or((0.0, 0.0), |slab| slab.length);
            (lo.min(start), hi.max(end))
        })
}

/// One authored wall's solid volume, prepared for the cross-section coverage
/// test.
#[derive(Debug)]
pub(in crate::render) struct WallCoverage {
    index: usize,
    axis: WallAxis,
    /// World span along the wall's length axis.
    length: (f32, f32),
    /// World span across the wall's thickness axis.
    thickness: (f32, f32),
    /// Solid rectangles as `(world length start, world length end, bottom Y,
    /// top Y)`.
    solids: Vec<(f32, f32, f32, f32)>,
}

/// Resolves one authored wall's solid volume and its solid patches.
fn wall_coverage(index: usize, wall: &WallDef, surfaces: &LevelSurfaces<'_>) -> WallCoverage {
    let axis = wall.axis();
    let (x0, x1) = (
        wall.x.min(wall.x + wall.width),
        wall.x.max(wall.x + wall.width),
    );
    let (z0, z1) = (
        wall.z.min(wall.z + wall.depth),
        wall.z.max(wall.z + wall.depth),
    );
    let (thickness, length) = match axis {
        WallAxis::X => ((z0, z1), (x0, x1)),
        WallAxis::Z => ((x0, x1), (z0, z1)),
    };
    let (origin_x, origin_z) = wall.length_origin();
    let origin = match axis {
        WallAxis::X => origin_x,
        WallAxis::Z => origin_z,
    };
    let breaks = surfaces.wall_profile_breaks(wall);
    let slices = wall_solid_slices_profiled(
        wall,
        |offset| surfaces.clear_ceiling_height_along(wall, offset),
        &breaks,
    );
    let mut solids: Vec<(f32, f32, f32, f32)> = slices
        .iter()
        .map(|slice| {
            (
                origin + slice.start,
                origin + slice.end,
                slice.bottom,
                slice.top,
            )
        })
        .collect();
    solids.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    WallCoverage {
        index,
        axis,
        length,
        thickness,
        solids,
    }
}

/// The rectangles of a cross-section plane that other walls' solid volumes
/// cover, as `(across low, across high, bottom Y, top Y)`.
///
/// A wall end (or a reveal) sits where the wall's solid profile changes, which
/// is very often exactly the plane of the wall it abuts. The abutting wall's
/// own surface already draws that plane, so the cross-section must not be
/// emitted there: two coplanar faces at the same depth are exactly the
/// z-fighting the coincidence resolution exists to remove. Every wall whose
/// volume contains the plane contributes its own footprint rectangle, and the
/// caller subtracts their union from the exposed face.
pub(in crate::render) fn cross_section_covered(
    coverages: &[WallCoverage],
    axis: WallAxis,
    at: f32,
    owners: &[usize],
) -> Vec<(f32, f32, f32, f32)> {
    let mut covered = Vec::new();
    for coverage in coverages {
        if owners.contains(&coverage.index) {
            continue;
        }
        // A parallel wall covers the plane when its length span reaches it; a
        // perpendicular wall covers it when its thickness span does.
        let on_plane = if coverage.axis == axis {
            at >= coverage.length.0 - WALL_COINCIDENCE_EPS
                && at <= coverage.length.1 + WALL_COINCIDENCE_EPS
        } else {
            at >= coverage.thickness.0 - WALL_COINCIDENCE_EPS
                && at <= coverage.thickness.1 + WALL_COINCIDENCE_EPS
        };
        if !on_plane {
            continue;
        }
        let across = if coverage.axis == axis {
            coverage.thickness
        } else {
            coverage.length
        };
        for (_, _, bottom, top) in &coverage.solids {
            covered.push((across.0, across.1, *bottom, *top));
        }
    }
    covered
}

/// Subtracts covered rectangles from an exposed `(across low, across high,
/// bottom, top)` rectangle, returning the disjoint remaining rectangles.
///
/// Both axes are cut at every covered edge, so the result is an exact
/// partition: a wall end half-covered by a thinner abutting wall keeps exactly
/// the half that is still exposed.
pub(in crate::render) fn subtract_rectangles(
    exposed: (f32, f32, f32, f32),
    covered: &[(f32, f32, f32, f32)],
) -> Vec<(f32, f32, f32, f32)> {
    let (a0, a1, b0, b1) = exposed;
    if a1 - a0 <= WALL_COINCIDENCE_EPS || b1 - b0 <= WALL_COINCIDENCE_EPS {
        return Vec::new();
    }
    let mut a_cuts = vec![a0, a1];
    let mut b_cuts = vec![b0, b1];
    let mut clipped: Vec<(f32, f32, f32, f32)> = Vec::new();
    for &(ca0, ca1, cb0, cb1) in covered {
        let low_a = ca0.max(a0);
        let high_a = ca1.min(a1);
        let low_b = cb0.max(b0);
        let high_b = cb1.min(b1);
        if high_a - low_a <= WALL_COINCIDENCE_EPS || high_b - low_b <= WALL_COINCIDENCE_EPS {
            continue;
        }
        a_cuts.push(low_a);
        a_cuts.push(high_a);
        b_cuts.push(low_b);
        b_cuts.push(high_b);
        clipped.push((low_a, high_a, low_b, high_b));
    }
    if clipped.is_empty() {
        return vec![exposed];
    }
    a_cuts.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
    a_cuts.dedup_by(|x, y| (*x - *y).abs() <= WALL_COINCIDENCE_EPS);
    b_cuts.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
    b_cuts.dedup_by(|x, y| (*x - *y).abs() <= WALL_COINCIDENCE_EPS);

    let mut remaining = Vec::new();
    for a_pair in a_cuts.windows(2) {
        let &[a_low, a_high] = a_pair else { continue };
        for b_pair in b_cuts.windows(2) {
            let &[b_low, b_high] = b_pair else { continue };
            if a_high - a_low <= WALL_COINCIDENCE_EPS || b_high - b_low <= WALL_COINCIDENCE_EPS {
                continue;
            }
            let a_mid = f32::midpoint(a_low, a_high);
            let b_mid = f32::midpoint(b_low, b_high);
            let is_covered = clipped.iter().any(|(ca0, ca1, cb0, cb1)| {
                a_mid >= *ca0 - WALL_COINCIDENCE_EPS
                    && a_mid <= *ca1 + WALL_COINCIDENCE_EPS
                    && b_mid >= *cb0 - WALL_COINCIDENCE_EPS
                    && b_mid <= *cb1 + WALL_COINCIDENCE_EPS
            });
            if !is_covered {
                remaining.push((a_low, a_high, b_low, b_high));
            }
        }
    }
    remaining
}

/// Coalesces one group of coincident walls into a single synthetic unit.
///
/// The unit's solid volume is the *union* of its members' solid slices (an
/// opaque member covers another's opening, exactly as the duplicate surfaces
/// used to show), and each `(length, height)` cell takes the material of the
/// last member whose solid volume covers it, in authored order: a damage
/// overlay authored after the wall it covers keeps winning, and two walls that
/// merely share a plane over part of their height each contribute their own
/// cell instead of two coplanar faces.
fn coalesce_wall_group<'a>(
    group: &[usize],
    slabs: &[Option<WallSlab>],
    walls: &'a [WallDef],
    surfaces: &LevelSurfaces<'_>,
    materials: &MaterialLookup<'_>,
    default_wall: MaterialRef<'_>,
) -> Option<WallUnit<'a>> {
    let (lo, hi) = group_union_span(group, slabs);
    let host_index = *group.first()?;
    let host = walls.get(host_index)?;

    let solids = group_member_solids(group, walls, surfaces);

    let (mut slices, mut runs) = group_solid_cells(&solids, lo, hi, walls, materials, default_wall);
    // Folding neighbouring cells is what keeps an ordinary group down to one
    // quad per face.
    frames_merge(&mut slices, &mut runs);

    // The synthetic wall spans the group's whole union, sharing the host's
    // thickness; its solid profile and materials travel separately. Collision
    // keeps using the authored walls, so this is a rendering-only resolution.
    let host_base = slabs
        .get(host_index)
        .copied()
        .flatten()
        .map_or(host.y, |slab| slab.base);
    let host_top = slabs
        .get(host_index)
        .copied()
        .flatten()
        .map_or_else(|| host.y + host.resolved_height(host_base), |slab| slab.top);
    let group_base = group
        .iter()
        .filter_map(|index| slabs.get(*index).copied().flatten())
        .fold(host_base, |low, slab| low.min(slab.base));
    let group_top = group
        .iter()
        .filter_map(|index| slabs.get(*index).copied().flatten())
        .fold(host_top, |high, slab| high.max(slab.top));
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
    wall.openings = Vec::new();
    wall.y = group_base;
    wall.height = Some(group_top - group_base);
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
    Some(WallUnit::Coalesced {
        members: group.to_vec(),
        wall: Box::new(wall),
        slices,
        runs,
    })
}

/// Every member's solid patches, in world length coordinates.
fn group_member_solids(
    group: &[usize],
    walls: &[WallDef],
    surfaces: &LevelSurfaces<'_>,
) -> Vec<MemberSolid> {
    let mut solids: Vec<MemberSolid> = Vec::new();
    for index in group {
        let Some(wall) = walls.get(*index) else {
            continue;
        };
        let (origin_x, origin_z) = wall.length_origin();
        let origin = match wall.axis() {
            WallAxis::X => origin_x,
            WallAxis::Z => origin_z,
        };
        let breaks = surfaces.wall_profile_breaks(wall);
        let slices = wall_solid_slices_profiled(
            wall,
            |offset| surfaces.clear_ceiling_height_along(wall, offset),
            &breaks,
        );
        for slice in slices {
            solids.push(MemberSolid {
                index: *index,
                start: origin + slice.start,
                end: origin + slice.end,
                bottom: slice.bottom,
                top: slice.top,
            });
        }
    }
    solids
}

/// Partitions a group's union into `(length, height)` cells, each carrying the
/// material of the last member whose solid volume covers it.
fn group_solid_cells(
    solids: &[MemberSolid],
    lo: f32,
    hi: f32,
    walls: &[WallDef],
    materials: &MaterialLookup<'_>,
    default_wall: MaterialRef<'_>,
) -> (Vec<WallSlice>, Vec<WallMaterialRun>) {
    let mut length_cuts: Vec<f32> = vec![lo, hi];
    for solid in solids {
        if solid.end <= lo + WALL_COINCIDENCE_EPS || solid.start >= hi - WALL_COINCIDENCE_EPS {
            continue;
        }
        length_cuts.push(solid.start.clamp(lo, hi));
        length_cuts.push(solid.end.clamp(lo, hi));
    }
    length_cuts.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    length_cuts.dedup_by(|a, b| (*a - *b).abs() <= WALL_COINCIDENCE_EPS);

    let default_face = materials.key(MaterialSlot::Wall, default_wall);
    let mut slices: Vec<WallSlice> = Vec::new();
    let mut runs: Vec<WallMaterialRun> = Vec::new();
    for pair in length_cuts.windows(2) {
        let &[start, end] = pair else { continue };
        if end - start <= WALL_COINCIDENCE_EPS {
            continue;
        }
        // Members whose patch spans this length segment completely.
        let covering: Vec<&MemberSolid> = solids
            .iter()
            .filter(|solid| {
                solid.start <= start + WALL_COINCIDENCE_EPS
                    && solid.end >= end - WALL_COINCIDENCE_EPS
            })
            .collect();
        if covering.is_empty() {
            continue;
        }
        let mut y_cuts: Vec<f32> = Vec::new();
        for solid in &covering {
            y_cuts.push(solid.bottom);
            y_cuts.push(solid.top);
        }
        y_cuts.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        y_cuts.dedup_by(|a, b| (*a - *b).abs() <= WALL_COINCIDENCE_EPS);
        for y_pair in y_cuts.windows(2) {
            let &[bottom, top] = y_pair else { continue };
            if top - bottom <= WALL_COINCIDENCE_EPS {
                continue;
            }
            let middle = f32::midpoint(bottom, top);
            // The last covering member in authored order owns the cell.
            let mut visible: Option<usize> = None;
            for solid in &covering {
                if solid.bottom <= middle && solid.top >= middle {
                    visible = Some(solid.index);
                }
            }
            let Some(index) = visible else { continue };
            let (faces, body) = walls
                .get(index)
                .map_or(([default_face, default_face], default_face), |wall| {
                    wall_material_keys(wall, default_wall, materials)
                });
            slices.push(WallSlice {
                start: start - lo,
                end: end - lo,
                bottom,
                top,
            });
            runs.push(WallMaterialRun {
                start: start - lo,
                end: end - lo,
                bottom,
                top,
                faces,
                body,
            });
        }
    }
    (slices, runs)
}

/// Folds adjacent cells of one coalesced group whose solid profile and
/// material are identical into one span, horizontally and vertically.
///
/// Merging is what keeps an ordinary group down to one quad per face; it also
/// removes the interior boundary those cells would otherwise emit a pair of
/// coincident caps along.
fn frames_merge(slices: &mut Vec<WallSlice>, runs: &mut Vec<WallMaterialRun>) {
    // A horizontal merge can expose a vertical one and vice versa, so the two
    // passes alternate until neither changes anything. Each pass only removes
    // entries, so the loop is bounded by the input cell count.
    for _ in 0..slices.len() {
        let before = slices.len();
        merge_cells(slices, runs, false);
        merge_cells(slices, runs, true);
        if slices.len() == before {
            break;
        }
    }
}

/// One ordered pass of [`frames_merge`]: `vertical` folds cells that share a
/// length span, otherwise cells that share a height span.
fn merge_cells(slices: &mut Vec<WallSlice>, runs: &mut Vec<WallMaterialRun>, vertical: bool) {
    let mut merged_slices: Vec<WallSlice> = Vec::with_capacity(slices.len());
    let mut merged_runs: Vec<WallMaterialRun> = Vec::with_capacity(runs.len());
    for (slice, run) in slices.drain(..).zip(runs.drain(..)) {
        let flat = |a: f32, b: f32| (a - b).abs() <= WALL_COINCIDENCE_EPS;
        let adjacent = merged_slices.last().is_some_and(|last| {
            if vertical {
                flat(last.start, slice.start)
                    && flat(last.end, slice.end)
                    && flat(last.top, slice.bottom)
            } else {
                flat(last.end, slice.start)
                    && flat(last.bottom, slice.bottom)
                    && flat(last.top, slice.top)
            }
        });
        let same_material = merged_runs
            .last()
            .is_some_and(|last| last.faces == run.faces && last.body == run.body);
        if adjacent && same_material {
            if let Some(last) = merged_slices.last_mut() {
                last.end = last.end.max(slice.end);
                last.top = last.top.max(slice.top);
                last.bottom = last.bottom.min(slice.bottom);
            }
            if let Some(last) = merged_runs.last_mut() {
                last.end = last.end.max(run.end);
                last.top = last.top.max(run.top);
                last.bottom = last.bottom.min(run.bottom);
            }
        } else {
            merged_slices.push(slice);
            merged_runs.push(run);
        }
    }
    *slices = merged_slices;
    *runs = merged_runs;
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

    let mut cuts: Vec<f32> =
        Vec::with_capacity(left.len().saturating_add(right.len()).saturating_mul(2));
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
    for pair in cuts.windows(2) {
        let [lower, upper] = pair else { continue };
        let (bottom, top) = (*lower, *upper);
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
    lightmap: Option<LightmapEmit<'_>>,
    room: Option<usize>,
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
    let first = vertices.len();
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
    stamp_lightmap_quad(
        lightmap,
        vertices,
        first,
        PatchKind::Wall,
        [order[0].0, order[1].0, order[2].0, order[3].0],
        room,
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
#[allow(clippy::arithmetic_side_effects)] // glam vector math is float-only and cannot overflow or panic
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
const fn decal_surface_tint(surface: crate::level::DecalSurface) -> [f32; 3] {
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
/// The quad is placed in two steps, and every decal a level authors goes
/// through both because they happen here and nowhere else:
///
/// 1. Its vertical placement follows the *actual* floor or ceiling under it, so
///    a decal in an elevated room or a recessed region stays on the surface
///    instead of being left behind at the authored world Y. Wall decals keep
///    their authored height, since a wall is not a horizontal surface.
/// 2. It is displaced [`DECAL_SURFACE_OFFSET_M`] along the surface normal — the
///    geometry half of the decal depth solution documented on
///    [`DECAL_POLYGON_OFFSET`] — so it can never occupy exactly the same depth
///    plane as its parent surface. The displacement is tiny and perpendicular
///    to the surface, so the marking still reads as printed/painted on it.
#[allow(clippy::arithmetic_side_effects)] // glam vector math is float-only and cannot overflow or panic
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
            crate::level::DecalSurface::WallNorth
            | crate::level::DecalSurface::WallSouth
            | crate::level::DecalSurface::WallWest
            | crate::level::DecalSurface::WallEast => point[1],
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
    // The single place a decal acquires its depth separation: offset along the
    // surface normal, after the horizontal snap above so the lift is measured
    // from the surface the decal actually lies on.
    for point in &mut points {
        let lifted = glam::Vec3::from(*point) + normal * DECAL_SURFACE_OFFSET_M;
        *point = lifted.to_array();
    }
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

/// True when a cell is still uncovered, so a growing rectangle can never
/// re-emit an earlier one. Out-of-range cells count as covered.
fn cell_is_covered(covered: &[bool], cells_x: usize, ix: usize, iz: usize) -> bool {
    covered
        .get(iz.saturating_mul(cells_x).saturating_add(ix))
        .copied()
        .unwrap_or(true)
}

/// True when every cell of the inclusive `ix0..=ix1` x `iz0..=iz1` rectangle is
/// still uncovered.
fn region_free(
    covered: &[bool],
    cells_x: usize,
    ix0: usize,
    ix1: usize,
    iz0: usize,
    iz1: usize,
) -> bool {
    covered
        .chunks(cells_x)
        .skip(iz0)
        .take(iz1.saturating_sub(iz0).saturating_add(1))
        .all(|row| {
            row.iter()
                .skip(ix0)
                .take(ix1.saturating_sub(ix0).saturating_add(1))
                .all(|cell| !cell)
        })
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
    if row_len == 0 {
        return true;
    }
    let width = ix1.saturating_sub(ix0).saturating_add(2);
    let height = iz1.saturating_sub(iz0).saturating_add(2);
    colors
        .chunks(row_len)
        .skip(iz0)
        .take(height)
        .flat_map(|row| row.iter().skip(ix0).take(width))
        .all(|color| {
            color
                .iter()
                .zip(&reference)
                .all(|(channel, reference)| (channel - reference).abs() <= LIGHT_GRID_MERGE_EPS)
        })
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
    let (Some(&x0), Some(&x1)) = (xs.get(ix0), xs.get(ix1.saturating_add(1))) else {
        return false;
    };
    let (Some(&z0), Some(&z1)) = (zs.get(iz0), zs.get(iz1.saturating_add(1))) else {
        return false;
    };
    let (span_x, span_z) = (x1 - x0, z1 - z0);
    if span_x <= 0.0 || span_z <= 0.0 {
        return false;
    }
    let (y00, y10, y01) = (y_at(x0, z0), y_at(x1, z0), y_at(x0, z1));
    let (Some(columns), Some(rows)) = (
        xs.get(ix0..=ix1.saturating_add(1)),
        zs.get(iz0..=iz1.saturating_add(1)),
    ) else {
        return false;
    };
    for z in rows {
        for x in columns {
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

/// The merge state of one floor/ceiling grid pass.
///
/// Holds the grid, its corner colours and the covered-cell lattice so the
/// greedy rectangle builder reads as one small method instead of a wall of
/// loop conditions inside the emitter.
struct GridMerger<'a, Y: Fn(f32, f32) -> f32> {
    xs: &'a [f32],
    zs: &'a [f32],
    colors: &'a [[f32; 3]],
    /// Cells already emitted, or outside the selected region.
    covered: Vec<bool>,
    cells_x: usize,
    cells_z: usize,
    row_len: usize,
    y_at: &'a Y,
    /// Longest merged quad, in metres; infinite on the vertex-lit path.
    max_span_m: f32,
}

impl<'a, Y: Fn(f32, f32) -> f32> GridMerger<'a, Y> {
    /// Creates the merge state, marking every cell outside `region` covered so
    /// a growing rectangle stops at the label boundary.
    fn new(
        xs: &'a [f32],
        zs: &'a [f32],
        colors: &'a [[f32; 3]],
        region: Option<(u32, &[u32])>,
        y_at: &'a Y,
        max_span_m: f32,
    ) -> Self {
        let cells_x = xs.len().saturating_sub(1);
        let cells_z = zs.len().saturating_sub(1);
        let mut covered = vec![false; cells_x.saturating_mul(cells_z)];
        if let Some((label, labels)) = region {
            for (index, cell) in covered.iter_mut().enumerate() {
                *cell = labels.get(index).copied() != Some(label);
            }
        }
        Self {
            xs,
            zs,
            colors,
            covered,
            cells_x,
            cells_z,
            row_len: xs.len(),
            y_at,
            max_span_m,
        }
    }

    /// Grows the largest rectangle starting at `(ix, iz)`, marks its cells
    /// covered and returns its inclusive `(ix, ix1, iz, iz1)` bounds.
    ///
    /// Growth stops where a corner's colour leaves
    /// [`LIGHT_GRID_MERGE_EPS`], where the surface stops being planar, where
    /// the region boundary is reached, or where the rectangle would exceed the
    /// lightmap chart span.
    fn grow(&mut self, ix: usize, iz: usize) -> Option<(usize, usize, usize, usize)> {
        if cell_is_covered(&self.covered, self.cells_x, ix, iz) {
            return None;
        }
        let reference = *self
            .colors
            .get(iz.saturating_mul(self.row_len).saturating_add(ix))?;
        let mut ix1 = ix;
        while ix1.saturating_add(1) < self.cells_x
            && region_free(
                &self.covered,
                self.cells_x,
                ix,
                ix1.saturating_add(1),
                iz,
                iz,
            )
            && grid_rect_is_uniform(
                self.colors,
                self.row_len,
                ix,
                ix1.saturating_add(1),
                iz,
                iz,
                reference,
            )
            && grid_rect_is_planar(
                self.xs,
                self.zs,
                self.y_at,
                ix,
                ix1.saturating_add(1),
                iz,
                iz,
            )
            && self
                .xs
                .get(ix1.saturating_add(2))
                .zip(self.xs.get(ix))
                .is_none_or(|(candidate, start)| candidate - start <= self.max_span_m)
        {
            ix1 = ix1.saturating_add(1);
        }
        let mut iz1 = iz;
        while iz1.saturating_add(1) < self.cells_z
            && region_free(
                &self.covered,
                self.cells_x,
                ix,
                ix1,
                iz,
                iz1.saturating_add(1),
            )
            && grid_rect_is_uniform(
                self.colors,
                self.row_len,
                ix,
                ix1,
                iz,
                iz1.saturating_add(1),
                reference,
            )
            && grid_rect_is_planar(
                self.xs,
                self.zs,
                self.y_at,
                ix,
                ix1,
                iz,
                iz1.saturating_add(1),
            )
            && self
                .zs
                .get(iz1.saturating_add(2))
                .zip(self.zs.get(iz))
                .is_none_or(|(candidate, start)| candidate - start <= self.max_span_m)
        {
            iz1 = iz1.saturating_add(1);
        }
        for z in iz..=iz1 {
            for x in ix..=ix1 {
                if let Some(cell) = self
                    .covered
                    .get_mut(z.saturating_mul(self.cells_x).saturating_add(x))
                {
                    *cell = true;
                }
            }
        }
        Some((ix, ix1, iz, iz1))
    }
}

/// Emits one lit floor or ceiling from a precomputed corner-colour grid.
///
/// Cells are greedily merged along X and then Z while every corner of the
/// candidate rectangle stays within [`LIGHT_GRID_MERGE_EPS`] and the surface
/// stays planar over it, so uniform regions cost one quad instead of up to
/// `MAX_LIGHT_GRID_CELLS`² of them. The surviving corners keep their exact
/// sampled colours and heights; UVs stay world-space, so merging is invisible
/// to texturing.
#[allow(clippy::too_many_arguments)] // mirrors the other quad emitters in this module
fn emit_lit_surface_grid(
    vertices: &mut Vec<Vertex>,
    xs: &[f32],
    zs: &[f32],
    colors: &[[f32; 3]],
    surface: LitSurface<'_, impl Fn(f32, f32) -> f32>,
    uv: impl Fn(f32, f32) -> [f32; 2],
    kind: PatchKind,
    room: Option<usize>,
    lightmap: Option<LightmapEmit<'_>>,
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
    let max_span_m = lightmap.map_or(f32::INFINITY, |lightmap| lightmap.max_span_m);
    let mut merger = GridMerger::new(xs, zs, colors, region, &y_at, max_span_m);
    let row_len = xs.len();

    for iz in 0..cells_z {
        for ix in 0..cells_x {
            let Some((ix1, iz1)) = merger.grow(ix, iz).map(|(_, ix1, _, iz1)| (ix1, iz1)) else {
                continue;
            };
            let (Some(&ax), Some(&bx)) = (xs.get(ix), xs.get(ix1.saturating_add(1))) else {
                continue;
            };
            let (Some(&az), Some(&bz)) = (zs.get(iz), zs.get(iz1.saturating_add(1))) else {
                continue;
            };
            let row = iz.saturating_mul(row_len);
            let next_row = iz1.saturating_add(1).saturating_mul(row_len);
            let column = ix1.saturating_add(1);
            let (Some(&c00), Some(&c10), Some(&c11), Some(&c01)) = (
                colors.get(row.saturating_add(ix)),
                colors.get(row.saturating_add(column)),
                colors.get(next_row.saturating_add(column)),
                colors.get(next_row.saturating_add(ix)),
            ) else {
                continue;
            };
            // Winding is the project convention: a face's front side is the
            // side its normal points to (right-hand rule over p0 -> p1 -> p2),
            // and every world face is wound to point *out* of the solid. So a
            // floor faces +Y and a ceiling faces -Y (a gable slope's normal tilts
            // but its vertical component keeps the same sign). Culling is off
            // today, but collision, decals and a future cull-enabled build all
            // read this convention.
            let (xz, corner_colors) = if ceiling {
                (
                    [[ax, az], [bx, az], [bx, bz], [ax, bz]],
                    [c00, c10, c11, c01],
                )
            } else {
                (
                    [[ax, bz], [bx, bz], [bx, az], [ax, az]],
                    [c01, c11, c10, c00],
                )
            };
            emit_grid_quad(
                vertices,
                xz,
                corner_colors,
                &y_at,
                &uv,
                kind,
                room,
                lightmap,
            );
        }
    }
}

/// Emits one merged grid rectangle: its `(x, z)` corners in winding order and
/// the four shaded corner colours, with world height and UVs derived here.
#[allow(clippy::too_many_arguments)] // mirrors the other quad emitters in this module
fn emit_grid_quad(
    vertices: &mut Vec<Vertex>,
    xz: [[f32; 2]; 4],
    corner_colors: [[f32; 3]; 4],
    y_at: &impl Fn(f32, f32) -> f32,
    uv: &impl Fn(f32, f32) -> [f32; 2],
    kind: PatchKind,
    room: Option<usize>,
    lightmap: Option<LightmapEmit<'_>>,
) {
    let points = xz.map(|[x, z]| [x, y_at(x, z), z]);
    let first = vertices.len();
    add_quad(
        vertices,
        points[0],
        corner_colors[0],
        uv(points[0][0], points[0][2]),
        points[1],
        corner_colors[1],
        uv(points[1][0], points[1][2]),
        points[2],
        corner_colors[2],
        uv(points[2][0], points[2][2]),
        points[3],
        corner_colors[3],
        uv(points[3][0], points[3][2]),
    );
    stamp_lightmap_quad(lightmap, vertices, first, kind, points, room);
}

/// Samples a `(cells_x + 1) x (cells_z + 1)` corner grid of baked colours.
///
/// The surface height is supplied per corner, so a recessed floor region or a
/// gable slope is lit by the baked illumination at its real world position.
///
/// With `lightmapped` set the baked light moves to the atlas and a corner's
/// colour is the material tint alone (white when there is none), so the
/// fragment stage's `texture x vertex colour x lightmap` still multiplies the
/// surface by exactly the tint and the light it always did.
fn lit_surface_grid(
    lighting: &LevelLighting,
    room_index: usize,
    xs: &[f32],
    zs: &[f32],
    y_at: impl Fn(f32, f32) -> f32,
    tint: Option<[f32; 3]>,
    lightmapped: bool,
) -> Vec<[f32; 3]> {
    let mut colors = Vec::with_capacity(xs.len().saturating_mul(zs.len()));
    if lightmapped {
        colors.resize(xs.len().saturating_mul(zs.len()), tint.unwrap_or([1.0; 3]));
        return colors;
    }
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
    let mut labels = vec![0u32; cells_x.saturating_mul(cells_z)];
    for iz in 0..cells_z {
        let (Some(&z0), Some(&z1)) = (grid.zs.get(iz), grid.zs.get(iz.saturating_add(1))) else {
            continue;
        };
        let z = f32::midpoint(z0, z1);
        for ix in 0..cells_x {
            let (Some(&x0), Some(&x1)) = (grid.xs.get(ix), grid.xs.get(ix.saturating_add(1)))
            else {
                continue;
            };
            let x = f32::midpoint(x0, x1);
            // Material precedence: the region's own material, then the latest
            // floor patch, then the room's floor material. Each carrier's own
            // shine override travels with it.
            let key = surfaces_at
                .region_at(x, z)
                .and_then(crate::level::FloorRegionDef::floor_ref)
                .map_or_else(
                    || {
                        patches
                            .iter()
                            .rev()
                            .find(|patch| patch_contains(patch, x, z))
                            .map_or(base_key, |patch| {
                                materials.key(MaterialSlot::Floor, patch.material_ref())
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
                        let index = surfaces.len();
                        surfaces.push(resolved);
                        u32::try_from(index).unwrap_or(u32::MAX)
                    },
                    |index| u32::try_from(index).unwrap_or(u32::MAX),
                );
            if let Some(cell) = labels.get_mut(iz.saturating_mul(cells_x).saturating_add(ix)) {
                *cell = label;
            }
        }
    }
    (surfaces, labels)
}

/// Corners of a vertical transition face, wound so the quad's normal points
/// toward the lower floor (out of the higher floor's volume).
const fn skirt_points(
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

/// One vertical transition face to emit: its side, position, length span,
/// vertical extent and material.
#[derive(Clone, Copy)]
struct SkirtFace {
    axis: WallAxis,
    positive: bool,
    at: f32,
    span: (f32, f32),
    low: f32,
    high: f32,
    key: SurfaceKey,
}

/// Corner shades of a transition face: the low corners take the darker shade
/// and the high corners the brighter one, in the same order the wall emitter
/// shades sills and headers.
fn skirt_corner_shades(tint: [f32; 3], mult: f32) -> [[f32; 3]; 4] {
    let shade_of = |grad: f32| {
        [
            (tint[0] * mult * grad).min(1.0),
            (tint[1] * mult * grad).min(1.0),
            (tint[2] * mult * grad).min(1.0),
        ]
    };
    let bottom_shade = shade_of(0.92);
    let top_shade = shade_of(1.05);
    [bottom_shade, bottom_shade, top_shade, top_shade]
}

/// Samples the baked illumination at each corner of a transition face.
fn skirt_corner_colors(
    base: [[f32; 3]; 4],
    points: [[f32; 3]; 4],
    lighting: &LevelLighting,
) -> [[f32; 3]; 4] {
    std::array::from_fn(|index| {
        // `index` is always < 4: both arrays have exactly four corners.
        let base = base.get(index).copied().unwrap_or_default();
        let point = points.get(index).copied().unwrap_or_default();
        shade(base, lighting.sample(point[0], point[1], point[2]))
    })
}

/// Emits one transition face as a lit quad carrying `key`'s material.
///
/// A lightmapped face whose span exceeds one chart is tiled into several quads,
/// each with its own chart; the vertex-lit fallback passes an infinite
/// `max_span_m` and emits exactly the historical single quad (positions and UVs
/// unchanged, bit for bit).
fn emit_skirt_face(
    buckets: &mut crate::spatial::SpatialBuckets<SurfaceKey>,
    scratch: &mut Vec<Vertex>,
    materials: &MaterialLookup<'_>,
    lighting: &LevelLighting,
    face: SkirtFace,
    lightmap: Option<LightmapEmit<'_>>,
) {
    let SkirtFace {
        axis,
        positive,
        at,
        span,
        low,
        high,
        key,
    } = face;
    if high - low <= HEIGHT_MERGE_EPS {
        return;
    }
    let mult = match (axis, positive) {
        (WallAxis::Z, false) => WALL_FACE_NORTH_MULT,
        (WallAxis::Z, true) => WALL_FACE_SOUTH_MULT,
        (WallAxis::X, false) => WALL_FACE_WEST_MULT,
        (WallAxis::X, true) => WALL_FACE_EAST_MULT,
    };
    let base = skirt_corner_shades(materials.tint(key), mult);
    let (hint_x, hint_z) = match axis {
        WallAxis::X => (at, f32::midpoint(span.0, span.1)),
        WallAxis::Z => (f32::midpoint(span.0, span.1), at),
    };
    let room = lighting.room_index_at_height(hint_x, f32::midpoint(low, high), hint_z);
    let lightmapped = lightmap.is_some_and(|lightmap| lightmap.is_on());
    let max_span_m = lightmap.map_or(f32::INFINITY, |lightmap| lightmap.max_span_m);

    for (span_start, span_end) in split_span(span.0, span.1, max_span_m) {
        for (tile_low, tile_high) in split_span(low, high, max_span_m) {
            if tile_high - tile_low <= HEIGHT_MERGE_EPS {
                continue;
            }
            let points = skirt_points(
                axis,
                positive,
                at,
                (span_start, span_end),
                tile_low,
                tile_high,
            );
            let colors = if lightmapped {
                base
            } else {
                skirt_corner_colors(base, points, lighting)
            };
            let uv = |point: [f32; 3]| match axis {
                WallAxis::X => materials.uv(key, point[2], high - point[1]),
                WallAxis::Z => materials.uv(key, point[0], high - point[1]),
            };
            scratch.clear();
            let first = scratch.len();
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
            stamp_lightmap_quad(lightmap, scratch, first, PatchKind::Skirt, points, room);
            buckets.add_quads(key, scratch);
        }
    }
}

/// The floor region owning cell `(ix, iz)`, if any: the latest authored region
/// whose bounds contain the cell centre.
fn region_at_cell<'a>(
    level: &'a LevelDef,
    room: &RoomDef,
    grid: &RoomFloorGrid,
    ix: usize,
    iz: usize,
) -> Option<&'a crate::level::FloorRegionDef> {
    let (x0, x1, z0, z1) = room.bounds();
    let (Some(&x_start), Some(&x_end)) = (grid.xs.get(ix), grid.xs.get(ix.saturating_add(1)))
    else {
        return None;
    };
    let (Some(&z_start), Some(&z_end)) = (grid.zs.get(iz), grid.zs.get(iz.saturating_add(1)))
    else {
        return None;
    };
    let x = f32::midpoint(x_start, x_end);
    let z = f32::midpoint(z_start, z_end);
    level.floor_regions.iter().rev().find(|region| {
        let (rx0, rx1, rz0, rz1) = region.bounds();
        rx1 > x0 && rx0 < x1 && rz1 > z0 && rz0 < z1 && region.contains(x, z)
    })
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
    room: &RoomDef,
    grid: &RoomFloorGrid,
    level: &LevelDef,
    lighting: &LevelLighting,
    materials: &MaterialLookup<'_>,
    lightmap: Option<LightmapEmit<'_>>,
) {
    let (cells_x, cells_z) = (grid.cells_x(), grid.cells_z());
    if cells_x == 0 || cells_z == 0 {
        return;
    }
    // A room has floor and ceiling materials but no wall material of its own, so
    // the documented fallback for a transition face is the level's wall material.
    // A region with its own `edge_material` overrides it.
    let default_edge = materials.key(MaterialSlot::Wall, level.defaults.wall_ref());

    // Skirts are few and short-lived: one local scratch keeps the emitter
    // signature within the module's argument budget.
    let mut scratch: Vec<Vertex> = Vec::new();
    let mut emit = |face: SkirtFace| {
        emit_skirt_face(buckets, &mut scratch, materials, lighting, face, lightmap);
    };

    let region_owner = |ix: usize, iz: usize| region_at_cell(level, room, grid, ix, iz);

    // The region owning a cell is the one whose material describes the faces
    // that cell's height difference creates.
    let edge_key = |region: Option<&crate::level::FloorRegionDef>| -> SurfaceKey {
        region
            .and_then(crate::level::FloorRegionDef::edge_ref)
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
            let next_x = ix.saturating_add(1);
            // Transition to the next cell along X, or to the room's own floor
            // plane when this is the room's last column.
            let right = if next_x < cells_x {
                Some(grid.y_at(room, next_x, iz))
            } else {
                Some(room.floor_y)
            };
            if let Some(other) = right
                && (other - y).abs() > HEIGHT_MERGE_EPS
            {
                let key = if next_x < cells_x {
                    face_key((next_x, iz), (ix, iz))
                } else {
                    edge_key(region_owner(ix, iz))
                };
                let Some(&at) = grid.xs.get(next_x) else {
                    continue;
                };
                let (Some(&span_start), Some(&span_end)) =
                    (grid.zs.get(iz), grid.zs.get(iz.saturating_add(1)))
                else {
                    continue;
                };
                emit(SkirtFace {
                    axis: WallAxis::X,
                    // The face belongs to the lower side's volume, so its
                    // normal points toward it: a recess wall faces into the
                    // recess, a raised platform's rim faces outward.
                    positive: y > other,
                    at,
                    span: (span_start, span_end),
                    low: y.min(other),
                    high: y.max(other),
                    key,
                });
            }

            let next_z = iz.saturating_add(1);
            let back = if next_z < cells_z {
                Some(grid.y_at(room, ix, next_z))
            } else {
                Some(room.floor_y)
            };
            if let Some(other) = back
                && (other - y).abs() > HEIGHT_MERGE_EPS
            {
                let key = if next_z < cells_z {
                    face_key((ix, next_z), (ix, iz))
                } else {
                    edge_key(region_owner(ix, iz))
                };
                let Some(&at) = grid.zs.get(next_z) else {
                    continue;
                };
                let (Some(&span_start), Some(&span_end)) =
                    (grid.xs.get(ix), grid.xs.get(ix.saturating_add(1)))
                else {
                    continue;
                };
                emit(SkirtFace {
                    axis: WallAxis::Z,
                    positive: y > other,
                    at,
                    span: (span_start, span_end),
                    low: y.min(other),
                    high: y.max(other),
                    key,
                });
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
    buckets.add_quads(key, scratch.get(*cursor..).unwrap_or_default());
    *cursor = scratch.len();
}

#[cfg(test)]
mod tests;
