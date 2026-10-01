//! The static level mesh emitter.
//!
//! Floors, ceilings, recess skirts, walls, fixtures and decals are written into
//! a scratch buffer per material run and split into spatial batches on the way
//! into the mesh, so the emitting code itself is free of grid awareness.

use std::cell::RefCell;

use super::{
    DECAL_EXTERNAL_BASE, LIGHT_FACE_PROBE_M, LevelDef, LevelLighting, LevelMesh, LevelSurfaces,
    LightmapEmit, LitSurface, MaterialIndex, MaterialLookup, MaterialSlot, MaterialTable, PropDef,
    SurfaceKey, SurfaceKind, Vertex, WALL_COINCIDENCE_EPS, WALL_FACE_EAST_MULT,
    WALL_FACE_NORTH_MULT, WALL_FACE_SOUTH_MULT, WALL_FACE_WEST_MULT, WallAxis, WallCoverage,
    WallUnit, add_decal_quad, add_flush_mount_fixture, add_panel_fixture, add_prop_box, add_quad,
    add_round_fixture, add_wall_cross_quad, add_wall_fixture, add_wall_length_face,
    cross_section_covered, decal_sheet_index, decal_uv_rect, decal_uv_rect_full, emit_floor_skirts,
    emit_lit_surface_grid, finish_indexed_mesh, floor_surfaces, flush_wall_run,
    interval_symmetric_difference, lit_corners, lit_surface_grid, room_is_tessellatable, shade,
    spatial_cell_grid, split_rect, stamp_lightmap_quad, subtract_rectangles, tiled_uv, wall_layout,
    wall_vertical_extent,
};
use crate::level::{RoomDef, WallDef, WallSlice};
use crate::lighting::lightmap::{LightmapPlan, PatchKind};
use crate::spatial::SpatialBuckets;

/// Directional shade multiplier applied to the top of a wall face.
pub const WALL_TOP_GRADIENT: f32 = 1.05;
/// Directional shade multiplier applied to the bottom of a wall face.
pub const WALL_BOTTOM_GRADIENT: f32 = 0.92;
/// Reveal faces are deliberately darker than the wall faces they interrupt, so
/// doorways and windows read clearly.
const WALL_JAMB_MULT: f32 = 0.78;
/// Header reveals sit slightly below the wall face brightness.
const WALL_HEAD_MULT: f32 = 0.92;

/// The immutable inputs every emitter stage of one level build shares.
pub struct EmitContext<'a, 's> {
    pub level: &'a LevelDef,
    pub surfaces: &'s LevelSurfaces<'a>,
    pub lighting: &'a LevelLighting,
    pub materials: &'a MaterialLookup<'a>,
    /// Every authored wall's solid volume, so a wall face an abutting wall
    /// already covers is never emitted underneath it.
    pub coverages: &'s [WallCoverage],
    /// The lightmap state: the plan when this build is lightmapped plus the
    /// chart-span cap, or `None` for the historical vertex-lit path, where
    /// every emitter must compute exactly the colours and vertices it always
    /// did.
    pub lightmap: Option<LightmapEmit<'s>>,
}

impl EmitContext<'_, '_> {
    /// True when this build bakes light into an atlas instead of vertex colours.
    pub const fn lightmapped(&self) -> bool {
        self.lightmap.is_some()
    }

    /// True when vertex colours carry the material factor alone (tint ×
    /// directional face shade) because the atlas carries the baked light.
    ///
    /// Every colour decision must use this; geometry shaping (chart-span caps,
    /// merge runs) must keep using [`Self::lightmapped`] so a build without a
    /// plan merges exactly like the historical vertex-lit path.
    pub const fn vertex_colors_are_material_only(&self) -> bool {
        self.lightmap.is_some()
    }
}

pub fn build_level_geometry_mesh(
    level: &LevelDef,
    catalog: &crate::loader::PropCatalog,
    fallback_props: &[&PropDef],
    lighting: &LevelLighting,
    materials: &MaterialTable,
) -> LevelMesh {
    build_level_geometry_mesh_with_lightmaps(
        level,
        catalog,
        fallback_props,
        lighting,
        materials,
        None,
    )
}

/// [`build_level_geometry_mesh`] with a lightmap plan the emitters stamp.
///
/// With `lightmaps: None` the result is the historical vertex-lit mesh, byte
/// for byte. With a plan every static floor, ceiling, wall and skirt quad is
/// stamped with a chart and its vertex colour is reduced to material tint and
/// directional face shade; the plan records one patch per quad for the fill
/// pass. Fixtures, prop placeholder boxes and decals stay vertex-lit either
/// way.
pub fn build_level_geometry_mesh_with_lightmaps(
    level: &LevelDef,
    catalog: &crate::loader::PropCatalog,
    fallback_props: &[&PropDef],
    lighting: &LevelLighting,
    materials: &MaterialTable,
    lightmaps: Option<&mut LightmapPlan>,
) -> LevelMesh {
    // Collect the merged room list once; geometry and ceiling lookups then
    // borrow it instead of cloning the room vector repeatedly.
    let rooms: Vec<&RoomDef> = level.room_iter().collect();
    // The shared vertical geometry model: every floor, ceiling and wall height
    // below comes from it, and collision and the walkable surface use the same
    // queries, so the mesh cannot drift from what the player stands on.
    let surfaces = LevelSurfaces::new(level);
    let lookup = MaterialLookup::new(materials);
    // The wall emission plan: coincident walls already resolved into single
    // units, plus every wall's solid volume for the cross-section coverage
    // test. Built once and shared by the wall emitter.
    let wall_layout = wall_layout(level, &surfaces, &lookup);
    // The plan is lent to the emit context as a `RefCell` so the emitters can
    // stamp quads through `&EmitContext` without an extra `&mut` parameter on
    // every function; `plan_cell` outlives `context` in this scope.
    let max_span_m = lightmaps
        .as_ref()
        .map_or(f32::INFINITY, |plan| plan.max_chart_span_m());
    let plan_cell: Option<RefCell<&mut LightmapPlan>> = lightmaps.map(RefCell::new);
    let lightmap = plan_cell.as_ref().map(|cell| LightmapEmit {
        plan: Some(cell),
        max_span_m,
    });
    let context = EmitContext {
        level,
        surfaces: &surfaces,
        lighting,
        materials: &lookup,
        coverages: &wall_layout.coverages,
        lightmap,
    };
    // Emitters still write whole quads into one scratch buffer; the bucket
    // builder splits each run by spatial cell on the way into the mesh. That
    // keeps the emitting code free of any grid awareness.
    let mut scratch: Vec<Vertex> = Vec::new();
    let mut buckets = SpatialBuckets::<SurfaceKey>::with_grid(spatial_cell_grid(level));

    // 1. Floors (and the water surfaces that share their family), 2. ceilings,
    //    3. walls, 4. fixtures, 5. prop fallbacks and 6. decals, in the order
    //    they are drawn.
    emit_floors(&context, &mut buckets, &mut scratch, &rooms);
    // One translucent surface quad per authored water volume, in the floor
    // family: the material's `blend` contract puts it in the sorted
    // translucent pass, and the volume's opacity rides in the vertex alpha.
    super::water::emit_water(&context, &mut buckets, &mut scratch);
    emit_ceilings(&context, &mut buckets, &mut scratch, &rooms);
    emit_walls(&context, &wall_layout.units, &mut buckets, &mut scratch);
    // Generic architectural pieces: ramps, staircases, half walls, columns,
    // archways, guardrails, thresholds and baseboards. They are static
    // surfaces like the walls above, so they join the same build and the same
    // lightmap atlas.
    super::architecture::emit_architecture(&context, &mut buckets, &mut scratch);
    emit_glass_panes(&context, &mut buckets, &mut scratch);
    emit_fixtures(&context, &mut buckets, &mut scratch);
    emit_prop_fallbacks(
        &context,
        catalog,
        fallback_props,
        &mut buckets,
        &mut scratch,
    );
    emit_decals(&context, catalog, &mut buckets, &mut scratch);

    finish_indexed_mesh(buckets)
}

/// Step 1: the baked-lighting grid over every room's floor.
///
/// The grid is sampled once per corner and greedily merged wherever the
/// lighting is effectively flat (unlit rooms and the far flanks of large rooms
/// therefore stay one or two quads). The cell count is bounded by
/// `lighting::MAX_LIGHT_GRID_CELLS`, and UVs keep mapping world space at the
/// material's tiling period, so the checkered carpet and any replacement tile
/// map identically.
///
/// A room that carries floor patches or floor regions is cut at their edges and
/// emitted one material/height surface at a time, so a damp patch has an exact
/// edge and a recess sits at its real elevation without a second overlapping
/// slab.
///
/// # Threshold ownership
///
/// There is no threshold surface: a doorway is a hole through the wall, and the
/// floor under it is the two rooms' own floors meeting at their shared
/// boundary. Each room emits its own floor up to its own footprint edge, so two
/// rooms whose boundary falls on a wall's centre plane jointly cover the whole
/// wall footprint — the west half from one room, the east half from the other —
/// with no overlap and no gap, which is the arrangement the shipped demo and
/// the fixture levels use. Two rooms that leave a gap *inside* the wall are
/// authored that way: the floor stops at each room's edge and the unwalkable
/// gap stays visible, exactly as drawn.
fn emit_floors(
    context: &EmitContext<'_, '_>,
    buckets: &mut SpatialBuckets<SurfaceKey>,
    scratch: &mut Vec<Vertex>,
    rooms: &[&RoomDef],
) {
    for (room_index, room) in rooms.iter().enumerate() {
        if !room_is_tessellatable(room) {
            continue;
        }
        let base_material = room
            .floor_ref()
            .unwrap_or_else(|| context.level.defaults.floor_ref());
        let base_key = context.materials.key(MaterialSlot::Floor, base_material);
        let room_patches = context.surfaces.patches_for_room(room);
        let grid = context.surfaces.floor_grid(room);
        let (floor_surfaces, labels) = floor_surfaces(
            &grid,
            context.surfaces,
            base_key,
            &room_patches,
            context.materials,
        );

        for (label, surface) in floor_surfaces.iter().enumerate() {
            let y = room.floor_y + surface.offset;
            let tint = context.materials.tint(surface.key);
            let colors = lit_surface_grid(
                context.lighting,
                room_index,
                &grid.xs,
                &grid.zs,
                |_, _| y,
                Some(tint),
                context.vertex_colors_are_material_only(),
            );
            let tile = context.materials.tile_metres(surface.key);
            scratch.clear();
            emit_lit_surface_grid(
                scratch,
                &grid.xs,
                &grid.zs,
                &colors,
                LitSurface {
                    y_at: |_, _| y,
                    ceiling: false,
                    region: Some((u32::try_from(label).unwrap_or(u32::MAX), &labels)),
                },
                |x, z| tiled_uv(x, z, tile),
                PatchKind::Floor,
                Some(room_index),
                context.lightmap,
            );
            buckets.add_quads(surface.key, scratch);
        }

        // The vertical faces of a recessed or raised region are real geometry,
        // not a hole into the void.
        emit_floor_skirts(
            buckets,
            room,
            &grid,
            context.level,
            context.lighting,
            context.materials,
            context.lightmap,
        );
    }
}

/// Clip hidden ceiling strips out of the lightmap chart domain. A room's
/// authored rectangle extends underneath perimeter walls; interpolating their
/// buried texels into the visible ceiling creates a false dark edge. Keep only
/// proven solid coverage (including real opening slices), never low walls.
/// Sloped roofs additionally require a terminal ceiling-bounded wall slice
/// whose actual roof owner is this room, matching the wall-face emitter.
fn exposed_ceiling_rectangles(
    context: &EmitContext<'_, '_>,
    room: &RoomDef,
) -> Vec<(f32, f32, f32, f32)> {
    let bounds = room.bounds();
    if !context.lightmapped() {
        return vec![bounds];
    }
    let mut covered = Vec::new();
    for coverage in context.coverages {
        for &solid in &coverage.solids {
            if let Some(rectangle) =
                covered_ceiling_rectangle(context, room, bounds, coverage.index, coverage, solid)
            {
                covered.push(rectangle);
            }
        }
    }
    subtract_rectangles(bounds, &covered)
}

/// The part of one wall coverage rectangle whose ceiling is at or below the
/// wall's own flat top — the strip the wall really hides.
///
/// `None` when the wall covers none of this room's ceiling. A flat-ceiling
/// room keeps the historical whole-rectangle rule; a sloped one keeps only the
/// sub-strip the flat wall top reaches, so the ceiling meets the wall instead
/// of stopping short above it and leaving the gable eave slot open.
fn covered_ceiling_rectangle(
    context: &EmitContext<'_, '_>,
    room: &RoomDef,
    bounds: (f32, f32, f32, f32),
    wall_index: usize,
    coverage: &WallCoverage,
    solid: (f32, f32, f32, f32),
) -> Option<(f32, f32, f32, f32)> {
    let (start, end, bottom, top) = solid;
    let (x0, x1, z0, z1) = match coverage.axis {
        WallAxis::X => (start, end, coverage.thickness.0, coverage.thickness.1),
        WallAxis::Z => (coverage.thickness.0, coverage.thickness.1, start, end),
    };
    let (x0, x1, z0, z1) = (
        x0.max(bounds.0),
        x1.min(bounds.1),
        z0.max(bounds.2),
        z1.min(bounds.3),
    );
    if x0 >= x1 || z0 >= z1 {
        return None;
    }
    let heights = [
        room.ceiling_y_at(x0, z0),
        room.ceiling_y_at(x1, z0),
        room.ceiling_y_at(x0, z1),
        room.ceiling_y_at(x1, z1),
    ];
    let low = heights.into_iter().fold(f32::INFINITY, f32::min);
    let high = heights.into_iter().fold(f32::NEG_INFINITY, f32::max);
    if bottom >= low - WALL_COINCIDENCE_EPS {
        return None;
    }
    if high - low <= WALL_COINCIDENCE_EPS && top >= high - WALL_COINCIDENCE_EPS {
        // A flat ceiling the wall reaches: the historical whole rectangle.
        return Some((x0, x1, z0, z1));
    }
    let wall = context.level.walls.get(wall_index)?;
    if wall.height.is_some() {
        return None;
    }
    let across = f32::midpoint(coverage.thickness.0, coverage.thickness.1);
    let (along0, along1) = match coverage.axis {
        WallAxis::X => (x0, x1),
        WallAxis::Z => (z0, z1),
    };
    let centre = |along| match coverage.axis {
        WallAxis::X => (along, across),
        WallAxis::Z => (across, along),
    };
    let (cx, cz) = centre(f32::midpoint(along0, along1));
    if !context
        .surfaces
        .room_at(cx, cz)
        .is_some_and(|owner| std::ptr::eq(owner, room))
    {
        return None;
    }
    if [along0, along1].into_iter().any(|along| {
        let (x, z) = centre(along);
        let ceiling_limit = wall.y + context.surfaces.clear_ceiling_height_at(x, z);
        top < ceiling_limit - WALL_COINCIDENCE_EPS
    }) {
        return None;
    }
    // The wall's flat top only reaches the ceiling on the side where the gable
    // has fallen to it: keep the ceiling strip across the rest of the wall's
    // thickness, or the sloped ceiling ends at the wall's inner face above its
    // top and leaves the classic gable eave slot open to the void.
    let (across0, across1) = coverage.thickness;
    let sample = |across: f32| match coverage.axis {
        WallAxis::X => room.ceiling_y_at(cx, across),
        WallAxis::Z => room.ceiling_y_at(across, cz),
    };
    let limit = top + WALL_COINCIDENCE_EPS;
    let (c0, c1) = (sample(across0), sample(across1));
    let (kept0, kept1) = if c0 <= limit && c1 <= limit {
        (across0, across1)
    } else if c0 > limit && c1 > limit {
        return None;
    } else {
        // The ceiling is linear across the thickness: solve for the crossing
        // and keep the covered sub-interval.
        let t = (limit - c0) / (c1 - c0);
        let crossing = (across1 - across0).mul_add(t, across0);
        if c0 <= limit {
            (across0, crossing)
        } else {
            (crossing, across1)
        }
    };
    let (cx0, cx1, cz0, cz1) = match coverage.axis {
        WallAxis::X => (x0, x1, kept0, kept1),
        WallAxis::Z => (kept0, kept1, z0, z1),
    };
    let (cx0, cx1, cz0, cz1) = (
        cx0.max(bounds.0),
        cx1.min(bounds.1),
        cz0.max(bounds.2),
        cz1.min(bounds.3),
    );
    if cx0 >= cx1 || cz0 >= cz1 {
        return None;
    }
    Some((cx0, cx1, cz0, cz1))
}

/// One ceiling lattice with a shared exposed-region label. Wall cuts remain
/// exact, but adjoining clear cells can merge across artificial subtraction
/// cuts instead of each doorway projecting a separate chart across the roof.
fn ceiling_surface_grid(
    context: &EmitContext<'_, '_>,
    room: &RoomDef,
) -> (Vec<f32>, Vec<f32>, Vec<u32>) {
    let (mut columns, mut rows) = context.surfaces.ceiling_grid(room);
    let exposed = exposed_ceiling_rectangles(context, room);
    for &(x0, x1, z0, z1) in &exposed {
        columns.extend([x0, x1]);
        rows.extend([z0, z1]);
    }
    columns.sort_by(f32::total_cmp);
    columns.dedup();
    rows.sort_by(f32::total_cmp);
    rows.dedup();
    let mut labels = Vec::new();
    for row in rows.windows(2) {
        let &[z0, z1] = row else { continue };
        let z = f32::midpoint(z0, z1);
        for column in columns.windows(2) {
            let &[x0, x1] = column else { continue };
            let x = f32::midpoint(x0, x1);
            labels.push(u32::from(exposed.iter().any(
                |&(left, right, front, back)| x >= left && x <= right && z >= front && z <= back,
            )));
        }
    }
    (columns, rows, labels)
}

/// Step 2: the ceiling batch, using the same grid and lighting sample as the
/// floor, with the fixture panels themselves drawn brighter by the light batch.
///
/// Ceilings carry no patches, only the room's ceiling material and its ceiling
/// profile; a gable is emitted as two real slopes meeting at the ridge, never
/// as a hidden flat plane above a decorative prop.
fn emit_ceilings(
    context: &EmitContext<'_, '_>,
    buckets: &mut SpatialBuckets<SurfaceKey>,
    scratch: &mut Vec<Vertex>,
    rooms: &[&RoomDef],
) {
    for (room_index, room) in rooms.iter().enumerate() {
        if !room_is_tessellatable(room) {
            continue;
        }
        // An open-ceiling room emits no ceiling batch at all: the sky (when the
        // level declares one) is what shows above it, and the level's other
        // geometry is unaffected.
        if room.ceiling.is_open() {
            continue;
        }
        let ceiling_key = context.materials.key(
            MaterialSlot::Ceiling,
            room.ceiling_ref()
                .unwrap_or_else(|| context.level.defaults.ceiling_ref()),
        );
        let (xs, zs, labels) = ceiling_surface_grid(context, room);
        scratch.clear();
        let ceiling_at = |x: f32, z: f32| room.ceiling_y_at(x, z);
        let colors = lit_surface_grid(
            context.lighting,
            room_index,
            &xs,
            &zs,
            ceiling_at,
            Some(context.materials.tint(ceiling_key)),
            context.vertex_colors_are_material_only(),
        );
        let tile = context.materials.tile_metres(ceiling_key);
        emit_lit_surface_grid(
            scratch,
            &xs,
            &zs,
            &colors,
            LitSurface {
                y_at: ceiling_at,
                ceiling: true,
                region: Some((1, &labels)),
            },
            // The ceiling tiles in the room's own tile frame (world origin at
            // zero rotation unless the room authors one), so a room with its
            // own ceiling module keeps its phase and a decal snapping to the
            // same grid lands exactly on the tiles that are drawn.
            |x, z| {
                let (local_x, local_z) = room.ceiling_tile_local(x, z);
                tiled_uv(local_x, local_z, tile)
            },
            PatchKind::Ceiling,
            Some(room_index),
            context.lightmap,
        );
        buckets.add_quads(ceiling_key, scratch);
    }
}

/// Step 3: the walls batch, one resolved wall unit at a time.
///
/// Each length face draws with its own material: the `faces` override for its
/// direction, else the wall's own `material`, else the level default. Sills,
/// headers and reveal jambs follow the wall's material, each sampling its own
/// texture at the material's tiling.
///
/// Coincident collinear walls are first resolved into single emission units
/// (`wall_layout`), so a water-damaged wall segment authored as a duplicate
/// surface becomes a material run on the one physical wall instead of a second
/// coplanar mesh.
fn emit_walls(
    context: &EmitContext<'_, '_>,
    units: &[WallUnit<'_>],
    buckets: &mut SpatialBuckets<SurfaceKey>,
    scratch: &mut Vec<Vertex>,
) {
    for unit in units {
        emit_wall_unit(context, buckets, scratch, unit);
    }
}

/// The resolved constants of one wall unit, shared by its sub-emitters.
struct WallState<'a> {
    /// The wall unit being emitted (its material runs follow the wall).
    unit: &'a WallUnit<'a>,
    /// The authored wall indices this unit emits, excluded from the
    /// cross-section coverage test (a wall never covers its own faces).
    members: Vec<usize>,
    /// The unit's earliest authored member: its ordering key for cross-wall
    /// cap ownership. A unit with a lower key keeps the coplanar overlap; a
    /// later one's cap subtracts the earlier footprint.
    first_member: usize,
    /// The wall's body geometry.
    wall: &'a WallDef,
    /// The body key: the fallback for slices with no material run.
    wall_key: SurfaceKey,
    /// The axis the wall's length runs along.
    axis: WallAxis,
    /// World start of the length axis.
    origin_x: f32,
    origin_z: f32,
    /// World span across the wall's thickness.
    t0: f32,
    t1: f32,
    /// World Y of the wall's base.
    wall_base: f32,
    /// Parent wall's top before openings split its material gradient.
    wall_top: f32,
    /// Parent room used when a doorway fragment lies beyond the adjoining
    /// room's vertical volume (for example a tall room's corridor lintel).
    room: Option<usize>,
    /// The wall's solid Y profile, in length order.
    slices: Vec<WallSlice>,
}

/// Emits one wall unit: its length faces, its top/bottom caps and the
/// cross-section reveals at every solid-profile boundary.
fn emit_wall_unit(
    context: &EmitContext<'_, '_>,
    buckets: &mut SpatialBuckets<SurfaceKey>,
    scratch: &mut Vec<Vertex>,
    unit: &WallUnit<'_>,
) {
    let wall = unit.wall();
    scratch.clear();
    let wall_material = wall
        .material_ref()
        .unwrap_or_else(|| context.level.defaults.wall_ref());
    let wall_key = context.materials.key(MaterialSlot::Wall, wall_material);
    let x0 = wall.x.min(wall.x + wall.width);
    let x1 = wall.x.max(wall.x + wall.width);
    let z0 = wall.z.min(wall.z + wall.depth);
    let z1 = wall.z.max(wall.z + wall.depth);
    // The unit's own solid profile: a plain wall cuts its own openings, a
    // coalesced group carries the union its members resolved to.
    let (wall_base, wall_top) = wall_vertical_extent(wall, context.surfaces);
    // The axis the wall's length runs along and the world span across its
    // thickness. Local slice offsets start at the wall's min corner.
    let axis = wall.axis();
    let (origin_x, origin_z) = wall.length_origin();
    let (t0, t1) = match axis {
        WallAxis::X => (z0, z1),
        WallAxis::Z => (x0, x1),
    };
    let slices = unit.slices(context.surfaces);
    let state = WallState {
        unit,
        members: unit.members(),
        first_member: unit.first_member(),
        wall,
        wall_key,
        axis,
        origin_x,
        origin_z,
        t0,
        t1,
        wall_base,
        wall_top,
        room: wall_parent_room(context.level, wall, wall_base, wall_top),
        slices,
    };
    // Cursor into `scratch` for the current face's quads; see `flush_wall_run`.
    let mut cursor = 0usize;
    for slice in &state.slices {
        emit_wall_slice(context, buckets, scratch, &state, slice, &mut cursor);
    }
    emit_wall_cross_sections(context, buckets, scratch, &state, &mut cursor);
    flush_wall_run(buckets, scratch, &mut cursor, wall_key);
}

/// Selects the parent from the complete wall's contact area, before openings
/// and material runs split it. A small adjoining corridor must not steal a
/// tall lintel just because its doorway happens to cross the wall midpoint.
fn wall_parent_room(level: &LevelDef, wall: &WallDef, bottom: f32, top: f32) -> Option<usize> {
    let (wx0, wx1) = (
        wall.x.min(wall.x + wall.width),
        wall.x.max(wall.x + wall.width),
    );
    let (wz0, wz1) = (
        wall.z.min(wall.z + wall.depth),
        wall.z.max(wall.z + wall.depth),
    );
    let mut best = None;
    let mut best_area = 0.0;
    for (index, room) in level.room_iter().enumerate() {
        let (x0, x1, z0, z1) = room.bounds();
        if x0 > wx1 || x1 < wx0 || z0 > wz1 || z1 < wz0 {
            continue;
        }
        let along = match wall.axis() {
            WallAxis::X => wx1.min(x1) - wx0.max(x0),
            WallAxis::Z => wz1.min(z1) - wz0.max(z0),
        };
        let ceiling = room.ceiling_y_at(f32::midpoint(wx0, wx1), f32::midpoint(wz0, wz1));
        let height = (top.min(ceiling) - bottom.max(room.floor_y)).max(0.0);
        let area = along.max(0.0) * height;
        if area > best_area {
            best_area = area;
            best = Some(index);
        }
    }
    best
}

/// One of the two length faces of a wall: its position across the wall's
/// thickness, outward normal, shade multiplier and winding.
#[derive(Clone, Copy)]
struct WallFace {
    /// Face coordinate across the thickness.
    position: f32,
    /// Outward normal, `-1.0` or `1.0` along the thickness axis.
    normal: f32,
    /// Directional brightness multiplier for this face.
    mult: f32,
    /// Whether the winding runs against the length axis.
    reversed: bool,
    /// Whether `u` is negated so the face reads unmirrored from its own side.
    flip_u: bool,
    /// Direction name used by the wall's `faces` overrides.
    name: &'static str,
}

/// One length-face strip to emit: the face's span, material and direction shade.
#[derive(Clone, Copy)]
struct WallLengthFace {
    key: SurfaceKey,
    face: f32,
    normal: f32,
    l0: f32,
    l1: f32,
    bottom: f32,
    shade: f32,
    reversed: bool,
    flip_u: bool,
}

impl WallLengthFace {
    /// Resolves one length-face strip's material and orientation.
    const fn new(key: SurfaceKey, face: WallFace, l0: f32, l1: f32, bottom: f32) -> Self {
        Self {
            key,
            face: face.position,
            normal: face.normal,
            l0,
            l1,
            bottom,
            shade: face.mult,
            reversed: face.reversed,
            flip_u: face.flip_u,
        }
    }
}

/// Emits the length faces and the horizontal caps of one solid wall slice.
fn emit_wall_slice(
    context: &EmitContext<'_, '_>,
    buckets: &mut SpatialBuckets<SurfaceKey>,
    scratch: &mut Vec<Vertex>,
    state: &WallState<'_>,
    slice: &WallSlice,
    cursor: &mut usize,
) {
    let (l0, l1) = match state.axis {
        WallAxis::X => (state.origin_x + slice.start, state.origin_x + slice.end),
        WallAxis::Z => (state.origin_z + slice.start, state.origin_z + slice.end),
    };
    let slice_bottom = slice.bottom;
    let slice_top = slice.top;
    // A wall without an authored height is bounded by the ceiling: its visible
    // top is the slice's top clipped to the ceiling directly above, so a wall
    // running up a gable slope reaches the real ceiling instead of poking
    // through it. An authored height is a rigid wall and is drawn exactly as
    // written, which is what lets a raised wall span two rooms with different
    // ceiling heights.
    let ceiling_bounded = state.wall.height.is_none();
    let profile_top = |offset| {
        state.wall.y
            + context
                .surfaces
                .clear_ceiling_height_along(state.wall, offset)
    };
    let follows_ceiling = ceiling_bounded
        && slice_top >= profile_top(slice.start).max(profile_top(slice.end)) - WALL_COINCIDENCE_EPS;

    // Faces parallel to the length axis: north/south for X-axis walls,
    // west/east for Z-axis walls. Each face is a strip of quads so the baked
    // lighting varies along the wall. `flip_u` makes each face read unmirrored
    // from the side its normal points into.
    let faces: [WallFace; 2] = match state.axis {
        WallAxis::X => [
            WallFace {
                position: state.t0,
                normal: -1.0,
                mult: WALL_FACE_NORTH_MULT,
                reversed: false,
                flip_u: true,
                name: "north",
            },
            WallFace {
                position: state.t1,
                normal: 1.0,
                mult: WALL_FACE_SOUTH_MULT,
                reversed: true,
                flip_u: false,
                name: "south",
            },
        ],
        WallAxis::Z => [
            WallFace {
                position: state.t0,
                normal: -1.0,
                mult: WALL_FACE_WEST_MULT,
                reversed: true,
                flip_u: true,
                name: "west",
            },
            WallFace {
                position: state.t1,
                normal: 1.0,
                mult: WALL_FACE_EAST_MULT,
                reversed: false,
                flip_u: false,
                name: "east",
            },
        ],
    };
    for (face_index, face) in faces.into_iter().enumerate() {
        let visible_top = |at| {
            if !ceiling_bounded {
                return slice_top;
            }
            let ceiling = wall_face_ceiling(context, state, at, face.position);
            if follows_ceiling {
                ceiling
            } else {
                slice_top.min(ceiling)
            }
        };
        // A coalesced unit splits the face at its material runs; a plain wall
        // emits the whole slice under its authored key.
        let runs = state
            .unit
            .runs_between(slice.start, slice.end, slice.bottom, slice.top);
        if runs.is_empty() {
            let key = wall_face_key(context, state.wall, face.name);
            let strip = WallLengthFace::new(key, face, l0, l1, slice_bottom);
            emit_wall_length_face(context, buckets, scratch, state, cursor, strip, visible_top);
        } else {
            for run in runs {
                let (run_start, run_end) = match state.axis {
                    WallAxis::X => (state.origin_x + run.start, state.origin_x + run.end),
                    WallAxis::Z => (state.origin_z + run.start, state.origin_z + run.end),
                };
                let key = run.faces.get(face_index).copied().unwrap_or(state.wall_key);
                let strip = WallLengthFace::new(key, face, run_start, run_end, slice_bottom);
                emit_wall_length_face(context, buckets, scratch, state, cursor, strip, visible_top);
            }
        }
    }

    emit_wall_caps(context, buckets, scratch, state, slice, cursor);
}

/// Appends one wall length face and flushes it under its own material key.
fn emit_wall_length_face(
    context: &EmitContext<'_, '_>,
    buckets: &mut SpatialBuckets<SurfaceKey>,
    scratch: &mut Vec<Vertex>,
    state: &WallState<'_>,
    cursor: &mut usize,
    strip: WallLengthFace,
    top_at: impl Fn(f32) -> f32,
) {
    // A perpendicular partition can cover an entire length-face strip. Such
    // a strip has no visible room-facing surface and must not acquire its own
    // chart or room hint. Keep partial-height overlaps unchanged here so their
    // authored face gradient and sloped ceiling profile are preserved.
    let intervals = if state.wall.height.is_some() {
        let axis = match state.axis {
            WallAxis::X => WallAxis::Z,
            WallAxis::Z => WallAxis::X,
        };
        // Sample on the outward side. A solid behind the face does not hide
        // it; only one that reaches into the half-space seen by the viewer can.
        let probe = strip.normal.mul_add(WALL_COINCIDENCE_EPS * 2.0, strip.face);
        let top = top_at(strip.l0).max(top_at(strip.l1));
        let covered: Vec<_> = cross_section_covered(context.coverages, axis, probe, &state.members)
            .into_iter()
            .filter(|(_, _, bottom, ceiling)| {
                *bottom <= strip.bottom + WALL_COINCIDENCE_EPS
                    && *ceiling >= top - WALL_COINCIDENCE_EPS
            })
            .collect();
        subtract_rectangles((strip.l0, strip.l1, strip.bottom, top), &covered)
            .into_iter()
            .map(|(lo, hi, _, _)| (lo, hi))
            .collect()
    } else {
        vec![(strip.l0, strip.l1)]
    };
    for (start, end) in intervals {
        let mid = f32::midpoint(start, end);
        let y = f32::midpoint(strip.bottom, top_at(mid));
        let side = strip.normal.mul_add(LIGHT_FACE_PROBE_M, strip.face);
        let (x, z) = match state.axis {
            WallAxis::X => (mid, side),
            WallAxis::Z => (side, mid),
        };
        // A lintel above a low corridor belongs to its tall parent room, not
        // to the corridor selected by a two-dimensional doorway probe.
        let room = context
            .lighting
            .room_index_strict_at_height(x, y, z)
            .or(state.room);
        add_wall_length_face(
            scratch,
            state.axis,
            start,
            end,
            strip.face,
            strip.normal,
            strip.bottom,
            &top_at,
            |at, y| wall_material_shade(context, state, strip, at, y),
            strip.reversed,
            strip.flip_u,
            context.lighting,
            room,
            context.materials.tile_metres(strip.key),
            context.lightmap,
        );
    }
    flush_wall_run(buckets, scratch, cursor, strip.key);
}

/// The wall's owning ceiling profile evaluated on the actual face, rather
/// than the thickness midpoint. This also extrapolates the same roof to an
/// exterior face instead of falling back to an unrelated room's ceiling.
fn wall_face_ceiling(
    context: &EmitContext<'_, '_>,
    state: &WallState<'_>,
    at: f32,
    face: f32,
) -> f32 {
    let middle = f32::midpoint(state.t0, state.t1);
    let ((cx, cz), (x, z)) = match state.axis {
        WallAxis::X => ((at, middle), (at, face)),
        WallAxis::Z => ((middle, at), (face, at)),
    };
    context
        .surfaces
        .room_at(cx, cz)
        .or_else(|| {
            state
                .room
                .and_then(|index| context.level.room_iter().nth(index))
        })
        .map_or_else(
            || context.surfaces.ceiling_y_at(x, z),
            |room| room.ceiling_y_at(x, z),
        )
}

/// Evaluates the authored bottom/top shade in the parent wall's height frame.
/// Opening fragments must not restart that gradient at their own sill/header.
fn wall_material_shade(
    context: &EmitContext<'_, '_>,
    state: &WallState<'_>,
    strip: WallLengthFace,
    at: f32,
    y: f32,
) -> [f32; 3] {
    let top = if state.wall.height.is_some() {
        state.wall_top
    } else {
        wall_face_ceiling(context, state, at, strip.face)
    };
    let fraction =
        ((y - state.wall_base) / (top - state.wall_base).max(f32::EPSILON)).clamp(0.0, 1.0);
    let gradient =
        (WALL_TOP_GRADIENT - WALL_BOTTOM_GRADIENT).mul_add(fraction, WALL_BOTTOM_GRADIENT);
    scaled_wall_color(context.materials, strip.key, strip.shade, gradient)
}

/// Emits the horizontal caps one solid slice exposes: the top of a half-height
/// wall or window sill, and the underside of a raised wall or door header.
///
/// A cap is only the part of the slice's horizontal face that is actually
/// exposed. Three things can own the same plane instead:
///
/// * the unit's own solid volume: the step between two stacked cells of a
///   coalesced group is an interior face, not two caps back to back;
/// * a room floor: the adjoining rooms' floors meet at a doorway's shared
///   boundary and jointly cover the wall footprint, so a sill whose top lands
///   on that plane is buried under a real floor surface. Emitting it anyway
///   puts two coplanar faces at the same depth, which is the doorway threshold
///   flicker this function exists to prevent;
/// * an earlier-authored wall unit whose solid reaches the same plane: where
///   two perpendicular walls share a corner square at one top (or one bottom),
///   the earlier unit keeps the overlap and this one subtracts its footprint,
///   so the corner square carries exactly one cap instead of two coplanar ones
///   z-fighting. Ordering is by the unit's earliest authored member, so it is
///   deterministic and independent of emission or worker order.
///
/// All three are subtracted as rectangles, so a cap covered over only part of
/// its span keeps exactly the exposed remainder instead of disappearing whole
/// or surviving underneath the covering surface.
fn emit_wall_caps(
    context: &EmitContext<'_, '_>,
    buckets: &mut SpatialBuckets<SurfaceKey>,
    scratch: &mut Vec<Vertex>,
    state: &WallState<'_>,
    slice: &WallSlice,
    cursor: &mut usize,
) {
    let slice_mid = f32::midpoint(slice.start, slice.end);
    let ceiling_along = |offset: f32| context.surfaces.ceiling_y_along(state.wall, offset);
    let floor_along = |offset: f32| {
        let (x, z) = crate::level::wall_point(state.wall, offset);
        context
            .surfaces
            .floor_y_at(x, z)
            .or_else(|| context.surfaces.room_floor_y_at(x, z))
            .unwrap_or(0.0)
    };
    // The cap's own rectangle in (along the wall, across its thickness) space.
    let cap = (slice.start, slice.end, state.t0, state.t1);
    // Floor rectangles at a cap's world plane, in the same (along, across)
    // space. Each room's floor grid resolves heights exactly like the floor
    // mesh does, so coverage can never disagree with what is drawn.
    let floor_covered = |plane: f32| -> Vec<(f32, f32, f32, f32)> {
        let area = match state.axis {
            WallAxis::X => (
                state.origin_x + slice.start,
                state.origin_x + slice.end,
                state.t0,
                state.t1,
            ),
            WallAxis::Z => (
                state.t0,
                state.t1,
                state.origin_z + slice.start,
                state.origin_z + slice.end,
            ),
        };
        let (ax0, ax1, az0, az1) = area;
        floor_coverage_at(
            context.surfaces,
            plane,
            (ax0.min(ax1), ax0.max(ax1), az0.min(az1), az0.max(az1)),
        )
        .into_iter()
        // World rects become cap-local `(along, across)` rects: the cap's
        // along coordinate is measured from the wall's own length origin.
        .map(|(x0, x1, z0, z1)| match state.axis {
            WallAxis::X => (x0 - state.origin_x, x1 - state.origin_x, z0, z1),
            WallAxis::Z => (z0 - state.origin_z, z1 - state.origin_z, x0, x1),
        })
        .collect()
    };
    // Slices of the unit that sit directly on top of this one, or directly
    // under it: their span is solid volume, so the shared plane is interior
    // there.
    let self_covered = |plane: f32, above: bool| -> Vec<(f32, f32, f32, f32)> {
        state
            .slices
            .iter()
            .filter(|other| {
                let other_plane = if above { other.bottom } else { other.top };
                (other_plane - plane).abs() <= WALL_COINCIDENCE_EPS
            })
            .map(|other| (other.start, other.end, state.t0, state.t1))
            .collect()
    };
    // Footprints of every earlier-authored unit's solid slices that reach the
    // cap's plane, in the same (along, across) space as the cap. A coplanar
    // top (or bottom) is owned by whichever unit came first; this unit's cap
    // subtracts the earlier wall's length and thickness span so the shared
    // rectangle carries exactly one cap. Only the two planes' own reach
    // participates: a wall that merely rises past this plane does not cover
    // it.
    let earlier_wall_covered = |plane: f32, top: bool| -> Vec<(f32, f32, f32, f32)> {
        let mut covered = Vec::new();
        for coverage in context.coverages {
            // The unit's own solids are `self_covered`; a later unit's cap
            // subtracts this one instead.
            if coverage.owner >= state.first_member {
                continue;
            }
            for &(start, end, bottom, other_top) in &coverage.solids {
                let coplanar = if top {
                    (other_top - plane).abs() <= WALL_COINCIDENCE_EPS
                } else {
                    (bottom - plane).abs() <= WALL_COINCIDENCE_EPS
                };
                if !coplanar {
                    continue;
                }
                // World (x, z) footprint of the other slice: its length span
                // along its own axis, its full thickness across.
                let (x0, x1, z0, z1) = match coverage.axis {
                    WallAxis::X => (start, end, coverage.thickness.0, coverage.thickness.1),
                    WallAxis::Z => (coverage.thickness.0, coverage.thickness.1, start, end),
                };
                covered.push(match state.axis {
                    WallAxis::X => (x0 - state.origin_x, x1 - state.origin_x, z0, z1),
                    WallAxis::Z => (z0 - state.origin_z, z1 - state.origin_z, x0, x1),
                });
            }
        }
        covered
    };

    // A wall that reaches the ceiling over this span needs no top face, which
    // is what keeps gable-end walls from growing a flat cap above the slope.
    if slice.top < ceiling_along(slice_mid) - 1e-3 {
        let mut covered = self_covered(slice.top, true);
        covered.extend(floor_covered(slice.top));
        covered.extend(earlier_wall_covered(slice.top, true));
        for rect in subtract_rectangles(cap, &covered) {
            emit_wall_slice_cap(context, buckets, scratch, state, slice, cursor, true, rect);
        }
    }

    // The underside is visible wherever it is above the floor the player
    // actually stands on (raised walls, door and window headers).
    if slice.bottom > floor_along(slice_mid) + 1e-3 {
        let mut covered = self_covered(slice.bottom, false);
        covered.extend(floor_covered(slice.bottom));
        covered.extend(earlier_wall_covered(slice.bottom, false));
        for rect in subtract_rectangles(cap, &covered) {
            emit_wall_slice_cap(context, buckets, scratch, state, slice, cursor, false, rect);
        }
    }
}

/// Emits one exposed rectangle of a solid slice's top (`up = true`) or bottom
/// cap and flushes it.
///
/// `rect` is the rectangle in the cap's own `(along the wall, across its
/// thickness)` space, already reduced to the part no other surface owns. The
/// cap looks up or down, so the two directions use opposite winding; the
/// Z-axis world mapping is the transpose of the X-axis one, so its corners
/// run the other way round to keep the same facing.
#[allow(clippy::too_many_arguments)] // matches the other quad emitters in this module
fn emit_wall_slice_cap(
    context: &EmitContext<'_, '_>,
    buckets: &mut SpatialBuckets<SurfaceKey>,
    scratch: &mut Vec<Vertex>,
    state: &WallState<'_>,
    slice: &WallSlice,
    cursor: &mut usize,
    up: bool,
    rect: (f32, f32, f32, f32),
) {
    let y = if up { slice.top } else { slice.bottom };
    let key = state
        .unit
        .run_at(f32::midpoint(slice.start, slice.end), y)
        .map_or(state.wall_key, |run| run.body);
    let (mult, grad) = if up {
        (1.00, WALL_TOP_GRADIENT)
    } else {
        (0.85, WALL_BOTTOM_GRADIENT)
    };
    let tile = context.materials.tile_metres(key);
    // A lightmapped cap larger than one chart is tiled; the vertex-lit fallback
    // gets the original rectangle back unchanged (see `split_rect`).
    let max_span_m = context
        .lightmap
        .map_or(f32::INFINITY, |lightmap| lightmap.max_span_m);
    for (a0, a1, b0, b1) in split_rect(rect, max_span_m) {
        // `rect` is in the cap's own `(along the wall, across its thickness)`
        // space, so the along axis is a local offset from the wall's length
        // origin exactly as [`WallSlice`] is for the length faces. Translate it
        // before anything reads a world coordinate: an untranslated cap is
        // shifted by the wall's origin, which leaves a gap at one jamb and a
        // buried overhang at the other on every wall whose min corner is not
        // zero.
        let (a0, a1) = match state.axis {
            WallAxis::X => (state.origin_x + a0, state.origin_x + a1),
            WallAxis::Z => (state.origin_z + a0, state.origin_z + a1),
        };
        let points = match (state.axis, up) {
            (WallAxis::X, true) => [[a0, y, b1], [a1, y, b1], [a1, y, b0], [a0, y, b0]],
            (WallAxis::X, false) => [[a0, y, b0], [a1, y, b0], [a1, y, b1], [a0, y, b1]],
            (WallAxis::Z, true) => [[b0, y, a0], [b0, y, a1], [b1, y, a1], [b1, y, a0]],
            (WallAxis::Z, false) => [[b1, y, a0], [b1, y, a1], [b0, y, a1], [b0, y, a0]],
        };
        let base_color = scaled_wall_color(context.materials, key, mult, grad);
        let lightmapped = context.vertex_colors_are_material_only();
        let colors = if lightmapped {
            [base_color; 4]
        } else {
            lit_corners(base_color, points, context.lighting)
        };
        let room = if context.lightmapped() {
            let (center_x, center_z) = match state.axis {
                WallAxis::X => (f32::midpoint(a0, a1), f32::midpoint(b0, b1)),
                WallAxis::Z => (f32::midpoint(b0, b1), f32::midpoint(a0, a1)),
            };
            context.lighting.room_index_at_height(center_x, y, center_z)
        } else {
            None
        };
        let uv = |point: [f32; 3]| match state.axis {
            WallAxis::X => tiled_uv(point[0], point[2], tile),
            WallAxis::Z => tiled_uv(point[2], point[0], tile),
        };
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
        stamp_lightmap_quad(
            context.lightmap,
            scratch,
            first,
            PatchKind::Wall,
            points,
            room,
        );
        flush_wall_run(buckets, scratch, cursor, key);
    }
}

/// Tolerance within which a floor surface is treated as lying on a wall cap's
/// plane, in metres.
///
/// It is the same 1 mm the wall-coincidence resolution uses, so "the same
/// plane" means the same thing wherever two surfaces are deduplicated.
const FLOOR_CAP_EPS: f32 = 1e-3;

/// The rectangles of walkable floor at world Y `y`, clipped to the world
/// `(x0, x1, z0, z1)` area they are needed in.
///
/// The rectangles are the room floor grid cells the floor emitter draws:
/// heights resolve through [`LevelSurfaces::floor_grid`], which samples the
/// same per-cell offsets the mesh, the collision rims and the walkable surface
/// use, so a cap can never be emitted on top of a floor the renderer draws.
fn floor_coverage_at(
    surfaces: &LevelSurfaces<'_>,
    y: f32,
    area: (f32, f32, f32, f32),
) -> Vec<(f32, f32, f32, f32)> {
    let (ax0, ax1, az0, az1) = area;
    let mut rects: Vec<(f32, f32, f32, f32)> = Vec::new();
    for room in surfaces.rooms() {
        if !room_is_tessellatable(room) {
            continue;
        }
        let (rx0, rx1, rz0, rz1) = room.bounds();
        if rx1 <= ax0 || rx0 >= ax1 || rz1 <= az0 || rz0 >= az1 {
            continue;
        }
        // Without regions the room floor is its whole footprint; with them the
        // grid is cut at their edges and each cell resolves its own height.
        if surfaces.regions_for_room(room).is_empty() {
            if (room.floor_y - y).abs() <= FLOOR_CAP_EPS {
                rects.push((rx0.max(ax0), rx1.min(ax1), rz0.max(az0), rz1.min(az1)));
            }
            continue;
        }
        let grid = surfaces.floor_grid(room);
        for iz in 0..grid.cells_z() {
            for ix in 0..grid.cells_x() {
                if (room.floor_y + grid.offset_at(ix, iz) - y).abs() > FLOOR_CAP_EPS {
                    continue;
                }
                let (Some(&x0), Some(&x1)) = (grid.xs.get(ix), grid.xs.get(ix.saturating_add(1)))
                else {
                    continue;
                };
                let (Some(&z0), Some(&z1)) = (grid.zs.get(iz), grid.zs.get(iz.saturating_add(1)))
                else {
                    continue;
                };
                let rect = (x0.max(ax0), x1.min(ax1), z0.max(az0), z1.min(az1));
                if rect.0 < rect.1 && rect.2 < rect.3 {
                    rects.push(rect);
                }
            }
        }
    }
    rects
}

/// The two sides of one solid-profile boundary, as absolute Y intervals.
#[derive(Clone, Copy)]
struct WallBoundary<'a> {
    position: f32,
    left: &'a [(f32, f32)],
    right: &'a [(f32, f32)],
}

/// Emits the wall's two ends and the reveals where its solid Y profile changes.
///
/// The exposed range at a boundary is the symmetric difference between the
/// solid intervals on the left and right of it.
fn emit_wall_cross_sections(
    context: &EmitContext<'_, '_>,
    buckets: &mut SpatialBuckets<SurfaceKey>,
    scratch: &mut Vec<Vertex>,
    state: &WallState<'_>,
    cursor: &mut usize,
) {
    let mut boundaries: Vec<f32> =
        Vec::with_capacity(state.slices.len().saturating_mul(2).saturating_add(2));
    boundaries.push(0.0);
    boundaries.push(state.wall.length());
    for slice in &state.slices {
        boundaries.push(slice.start);
        boundaries.push(slice.end);
    }
    boundaries.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    boundaries.dedup_by(|a, b| (*a - *b).abs() <= 1e-3);
    let ceiling_along = |offset: f32| context.surfaces.ceiling_y_along(state.wall, offset);

    for position in boundaries {
        let left: Vec<(f32, f32)> = state
            .slices
            .iter()
            .filter(|s| (s.end - position).abs() <= 1e-3)
            .map(|s| (s.bottom, s.top))
            .collect();
        let right: Vec<(f32, f32)> = state
            .slices
            .iter()
            .filter(|s| (s.start - position).abs() <= 1e-3)
            .map(|s| (s.bottom, s.top))
            .collect();
        let boundary = WallBoundary {
            position,
            left: &left,
            right: &right,
        };
        for (bottom, top) in interval_symmetric_difference(&left, &right) {
            // A wall end under a gable stops at the ceiling, so its end cap
            // follows the triangle instead of rising to the ridge.
            let top = top.min(ceiling_along(position));
            emit_wall_cross_quad(
                context,
                buckets,
                scratch,
                state,
                cursor,
                boundary,
                (bottom, top),
            );
        }
    }
}

/// The room a cross-section edge's face opens into, via the same inboard probe
/// [`sample_cross_edge`] samples from.
fn cross_edge_room(
    context: &EmitContext<'_, '_>,
    state: &WallState<'_>,
    inboard: f32,
    side: f32,
    side_normal: f32,
) -> Option<usize> {
    let (px, pz, nx, nz) = match state.axis {
        WallAxis::X => (inboard, side, 0.0, side_normal),
        WallAxis::Z => (side, inboard, side_normal, 0.0),
    };
    context.lighting.face_room(px, pz, nx, nz)
}

/// Samples the baked light for one edge of a cross-section quad, from the room
/// that edge's face opens into.
fn sample_cross_edge(
    context: &EmitContext<'_, '_>,
    state: &WallState<'_>,
    inboard: f32,
    side: f32,
    side_normal: f32,
    y: f32,
) -> crate::lighting::LightColor {
    let room = cross_edge_room(context, state, inboard, side, side_normal);
    let probe = side_normal.mul_add(LIGHT_FACE_PROBE_M, side);
    match state.axis {
        WallAxis::X => context.lighting.sample_face(room, inboard, y, probe),
        WallAxis::Z => context.lighting.sample_face(room, probe, y, inboard),
    }
}

/// One cross-section top corner, following the same roof as the length face.
fn wall_cross_top(
    context: &EmitContext<'_, '_>,
    state: &WallState<'_>,
    at: f32,
    across: f32,
    top: f32,
) -> f32 {
    if state.wall.height.is_some() {
        return top;
    }
    let center_ceiling = wall_face_ceiling(context, state, at, f32::midpoint(state.t0, state.t1));
    let ceiling = wall_face_ceiling(context, state, at, across);
    if top >= center_ceiling - WALL_COINCIDENCE_EPS {
        ceiling
    } else {
        top.min(ceiling)
    }
}

/// Emits one exposed interval of a solid-profile boundary cross-section.
///
/// A cross-section always sits where the wall's solid profile changes, so its
/// own position is frequently a room boundary or inside a perpendicular wall.
/// Light is therefore sampled just *inside* the solid side of the boundary,
/// where the face is unambiguous, and taken from the room it opens into.
///
/// The exposed interval is then reduced to the part no *other* wall's solid
/// volume covers. A wall end that abuts another wall's plane would otherwise
/// emit a second coplanar face at the same depth as that wall's own surface,
/// which is exactly the flicker the coincidence resolution removes inside a
/// group; the coverage test removes it between walls too.
fn emit_wall_cross_quad(
    context: &EmitContext<'_, '_>,
    buckets: &mut SpatialBuckets<SurfaceKey>,
    scratch: &mut Vec<Vertex>,
    state: &WallState<'_>,
    cursor: &mut usize,
    boundary: WallBoundary<'_>,
    span: (f32, f32),
) {
    let (bottom, top) = span;
    if top <= bottom + 1e-3 {
        return;
    }
    let at = match state.axis {
        WallAxis::X => state.origin_x + boundary.position,
        WallAxis::Z => state.origin_z + boundary.position,
    };
    let covered = cross_section_covered(context.coverages, state.axis, at, &state.members);
    let exposed = subtract_rectangles((state.t0, state.t1, bottom, top), &covered);
    if exposed.is_empty() {
        return;
    }
    let at_start = boundary.position <= 1e-3;
    let at_end = (boundary.position - state.wall.length()).abs() <= 1e-3;
    // A reveal is exposed to whichever side has no material over this Y range:
    // that is the side it faces. Wall ends follow the same rule (nothing is
    // solid outside the wall).
    let covers = |intervals: &[(f32, f32)]| {
        intervals
            .iter()
            .any(|(low, high)| *low <= bottom + 1e-3 && *high >= top - 1e-3)
    };
    let right_covers = covers(boundary.right);
    let inward = if right_covers { 1.0 } else { -1.0 };
    let inboard = at + inward * LIGHT_FACE_PROBE_M;
    let key = state
        .unit
        .run_at(boundary.position, f32::midpoint(bottom, top))
        .map_or(state.wall_key, |run| run.body);
    let tile = context.materials.tile_metres(key);
    let lightmapped = context.vertex_colors_are_material_only();
    let max_span_m = context
        .lightmap
        .map_or(f32::INFINITY, |lightmap| lightmap.max_span_m);
    for (across_low, across_high, rect_bottom, rect_top) in exposed {
        let (low_side, low_normal, high_side, high_normal) =
            nearest_cross_sides(state, across_low, across_high);
        // One room hint for every tile of this reveal: the room its lower edge
        // opens into (the higher side is the fallback when that is ambiguous).
        let room = if context.lightmapped() {
            cross_edge_room(context, state, inboard, low_side, low_normal)
                .or_else(|| cross_edge_room(context, state, inboard, high_side, high_normal))
        } else {
            None
        };
        // A lightmapped reveal larger than one chart is tiled; the vertex-lit
        // fallback gets the original rectangle back unchanged.
        for (across0, across1, bottom, top) in
            split_rect((across_low, across_high, rect_bottom, rect_top), max_span_m)
        {
            let top_low_y = wall_cross_top(context, state, at, across0, top);
            let top_high_y = wall_cross_top(context, state, at, across1, top);
            // Wall ends keep the directional face shading; internal reveals use
            // the darker jamb/head colours.
            let mult = if at_start {
                match state.axis {
                    WallAxis::X => WALL_FACE_WEST_MULT,
                    WallAxis::Z => WALL_FACE_NORTH_MULT,
                }
            } else if at_end {
                match state.axis {
                    WallAxis::X => WALL_FACE_EAST_MULT,
                    WallAxis::Z => WALL_FACE_SOUTH_MULT,
                }
            } else if bottom <= state.wall_base + 1e-3 {
                WALL_JAMB_MULT
            } else {
                WALL_HEAD_MULT
            };
            // Corner order: low thickness, high thickness, then the same at the
            // top (see add_wall_cross_quad).
            let bottom_shade =
                scaled_wall_color(context.materials, key, mult, WALL_BOTTOM_GRADIENT);
            let top_shade = scaled_wall_color(context.materials, key, mult, WALL_TOP_GRADIENT);
            let corners = if lightmapped {
                [bottom_shade, bottom_shade, top_shade, top_shade]
            } else {
                let (bottom_low, bottom_high, top_low, top_high) = (
                    sample_cross_edge(context, state, inboard, low_side, low_normal, bottom),
                    sample_cross_edge(context, state, inboard, high_side, high_normal, bottom),
                    sample_cross_edge(context, state, inboard, low_side, low_normal, top_low_y),
                    sample_cross_edge(context, state, inboard, high_side, high_normal, top_high_y),
                );
                [
                    shade(bottom_shade, bottom_low),
                    shade(bottom_shade, bottom_high),
                    shade(top_shade, top_high),
                    shade(top_shade, top_low),
                ]
            };
            *cursor = scratch.len();
            add_wall_cross_quad(
                scratch,
                state.axis,
                at,
                (across0, across1),
                bottom,
                [top_low_y, top_high_y],
                covers(boundary.left),
                corners,
                tile,
                context.lightmap,
                room,
            );
            flush_wall_run(buckets, scratch, cursor, key);
        }
    }
}

/// The wall faces a cross-section rectangle's two across edges sit on, as
/// `(side, outward normal)` pairs for the low and high edge.
///
/// A rectangle that still spans the whole wall uses both faces, exactly like an
/// unsplit cross-section. A rectangle left by subtracting an abutting wall's
/// footprint is anchored on one face, and its cut edge is sampled from that same
/// face, which is the surface it actually borders.
fn nearest_cross_sides(
    state: &WallState<'_>,
    across_low: f32,
    across_high: f32,
) -> (f32, f32, f32, f32) {
    let middle = f32::midpoint(state.t0, state.t1);
    let low = if across_low <= middle {
        (state.t0, -1.0)
    } else {
        (state.t1, 1.0)
    };
    let high = if across_high >= middle {
        (state.t1, 1.0)
    } else {
        (state.t0, -1.0)
    };
    (low.0, low.1, high.0, high.1)
}

/// Step 4: the light fixture batch.
///
/// A fixture's family comes from its catalog id (see
/// `lighting::fixture_profile`): the office panel hangs just below its room's
/// ceiling, a round downlight sits in the same plane, and a wall luminaire
/// mounts at its authored world height. The luminous face is texture-first:
/// its visible colour is the sheet's own RGB, and the vertex emission only
/// carries a neutral brightness from the fixture's **emissive** strength — by
/// default the same intensity the bake casts into the room, but separately
/// authorable (and independent of `enabled`) so a face can read bright while
/// its light stays dim or absent. The authored light colour never repaints the
/// face; it stays a property of the illumination the bake resolves.
///
/// A light batch carries the family's fixture-sheet slot
/// ([`crate::lighting::FixtureKind::index`]) so the renderer binds the PNG that
/// catalog declares as that fixture's visible face. The flat metal housing
/// (the round bezel and can, the wall housing, and the office panel's frame
/// and body) keeps the bare key: it draws its authored shade through the
/// shared white sheet.
fn emit_fixtures(
    context: &EmitContext<'_, '_>,
    buckets: &mut SpatialBuckets<SurfaceKey>,
    scratch: &mut Vec<Vertex>,
) {
    let mut housing: Vec<Vertex> = Vec::new();
    for (fixture_index, light) in context.level.ceiling_lights.iter().enumerate() {
        if !light.x.is_finite() || !light.z.is_finite() {
            continue;
        }
        scratch.clear();
        housing.clear();
        let profile = crate::lighting::fixture_profile(&light.fixture);
        let (half_w, half_d) =
            crate::lighting::fixture_half_extents_for(profile.kind, light.rotation_degrees);

        // The luminous face's emission is driven by the fixture's authored
        // emissive strength, which defaults to its light intensity but can be
        // authored independently: a fixture may read fully bright while casting
        // its dim light, or glow while casting nothing at all (`enabled: false`).
        // This value never becomes illumination — the bake reads `intensity()`.
        let emission = light.emission_intensity();
        // An explicitly zero-output fixture is off: its panel must not glow
        // while emitting no illumination.
        let output = if emission <= 0.0 {
            0.0
        } else {
            0.40f32
                .mul_add(emission.clamp(0.0, 2.0), 0.60)
                .clamp(0.0, 1.0)
        };
        // Neutral brightness, so the sheet is the fixture's visible colour: the
        // authored light colour (`light.emitted_color()`) reaches the room
        // through the bake and never tints the artwork.
        let face_emission = [output; 3];
        // The bake and the mesh share one resolver, so the drawn panel can
        // never sit at a different height from the light plane it casts: a
        // ceiling fixture without an authored `y` hangs below the lowest
        // ceiling point it covers (a gable fixture near the eave and one near
        // the ridge both clear the slope), and a fixture that authors a world
        // `y` — a wall sconce, or a ceiling fixture on a chosen storey of a
        // stacked building — mounts exactly there.
        let y = context.lighting.fixture_y_for(light);

        match profile.kind {
            crate::lighting::FixtureKind::FluorescentPanel => {
                let x0 = light.x - half_w;
                let x1 = light.x + half_w;
                let z0 = light.z - half_d;
                let z1 = light.z + half_d;
                add_panel_fixture(scratch, &mut housing, x0, x1, z0, z1, y, face_emission);
            }
            crate::lighting::FixtureKind::RoundRecessed => {
                add_round_fixture(
                    scratch,
                    &mut housing,
                    light.x,
                    light.z,
                    y,
                    profile.half_width,
                    face_emission,
                );
            }
            crate::lighting::FixtureKind::WallSconce => {
                add_wall_fixture(
                    scratch,
                    &mut housing,
                    light.x,
                    y,
                    light.z,
                    light.rotation_degrees,
                    face_emission,
                );
            }
            crate::lighting::FixtureKind::FlushMount => {
                add_flush_mount_fixture(
                    scratch,
                    &mut housing,
                    light.x,
                    light.z,
                    y,
                    profile.half_width,
                    face_emission,
                );
            }
        }
        // The luminous faces bind the family's sheet; the housing (empty only
        // for a family that generates none) binds the family's own bare key,
        // which draws the untextured white sheet.
        // A family sheet is shared by every fixture of the family; a
        // switchable fixture gets its own slot so its face can turn off
        // without touching another fixture's material.
        let sheet = crate::level::fixture_face_material_index(
            fixture_index,
            light.switchable,
            profile.kind,
        );
        buckets.add_quads(SurfaceKey::new(SurfaceKind::Light, sheet), scratch);
        buckets.add_quads(SurfaceKey::bare(SurfaceKind::Light), &housing);
    }
}

/// Step 3b: the glass panes that fill openings.
///
/// An opening with a `glass` material is a *hole with a pane in it*: the wall
/// keeps its full cut (collision and the lighting bake are unchanged — glass
/// transmits light, and the bake already runs through the aperture), and one
/// quad is emitted in the wall's mid-plane so the opening reads as glazed
/// rather than empty.
///
/// The pane is deliberately the opening's own rectangle at the wall's centre
/// plane, with no frame and no thickness: it is a *surface*, so every material
/// feature applies to it — tint, dirt texture, roughness, sheen, alpha mode and
/// emission. A `blend` glass draws in the translucent pass like any other
/// translucent surface, which is what lets a level put real glass in a window
/// without the renderer knowing what a window is.
///
/// A pane is emitted once per authored opening. Two coincident walls that the
/// wall emitter coalesces into one would each emit their own pane; author a
/// wall once, as everywhere else in the format.
///
/// The pane is never lightmapped: like a fixture face or a prop, its four
/// corners sample the baked light directly and fold it into the vertex colour.
/// A pane spans at most one opening, so per-corner sampling is smooth across it.
fn emit_glass_panes(
    context: &EmitContext<'_, '_>,
    buckets: &mut SpatialBuckets<SurfaceKey>,
    scratch: &mut Vec<Vertex>,
) {
    for wall in &context.level.walls {
        if !wall.x.is_finite()
            || !wall.z.is_finite()
            || !wall.width.is_finite()
            || !wall.depth.is_finite()
            || !wall.y.is_finite()
            || !wall.height.unwrap_or(0.0).is_finite()
        {
            continue;
        }
        for opening in &wall.openings {
            let Some(material) = opening.glass_ref() else {
                continue;
            };
            if !opening.offset.is_finite()
                || !opening.width.is_finite()
                || !opening.height.is_finite()
                || !opening.sill.is_finite()
                || opening.width <= 0.0
                || opening.height <= 0.0
            {
                continue;
            }
            let key = context.materials.key(MaterialSlot::Wall, material);
            let tile = context.materials.tile_metres(key);
            let tint = context.materials.tint(key);
            let base = wall.y;
            let bottom = opening.bottom(base);
            let top = opening.top(base);
            let near = opening.offset;
            let far = opening.end();
            let (origin_x, origin_z) = wall.length_origin();
            let (t0, t1) = {
                let z0 = wall.z.min(wall.z + wall.depth);
                let z1 = wall.z.max(wall.z + wall.depth);
                let x0 = wall.x.min(wall.x + wall.width);
                let x1 = wall.x.max(wall.x + wall.width);
                match wall.axis() {
                    WallAxis::X => (z0, z1),
                    WallAxis::Z => (x0, x1),
                }
            };
            // The pane sits in the middle of the wall's thickness: one surface,
            // drawn from both sides (nothing in the world is back-face culled,
            // and the shader flips the normal towards the viewer).
            let across = f32::midpoint(t0, t1);
            // Corners in the wall's own winding: p0 -> p1 runs along the opening
            // (u), p0 -> p3 runs up it (v).
            let (p0, p1, p2, p3) = match wall.axis() {
                WallAxis::X => (
                    [origin_x + near, bottom, across],
                    [origin_x + far, bottom, across],
                    [origin_x + far, top, across],
                    [origin_x + near, top, across],
                ),
                WallAxis::Z => (
                    [across, bottom, origin_z + near],
                    [across, bottom, origin_z + far],
                    [across, top, origin_z + far],
                    [across, top, origin_z + near],
                ),
            };
            let (u_near, u_far) = (near, far);
            let uv0 = tiled_uv(u_near, opening.sill, tile);
            let uv1 = tiled_uv(u_far, opening.sill, tile);
            let uv2 = tiled_uv(u_far, opening.sill + opening.height, tile);
            let uv3 = tiled_uv(u_near, opening.sill + opening.height, tile);
            // The pane is a *wall* surface: a lightmapped build stamps it into
            // its own chart (so per-texel baked light crosses the pane, exactly
            // as it crosses the wall around it) and the vertex colour is the
            // material tint alone. The vertex-lit fallback bakes the light into
            // the corners as it does everywhere else.
            let lightmapped = context.vertex_colors_are_material_only();
            let colours = if lightmapped {
                [tint; 4]
            } else {
                lit_corners(tint, [p0, p1, p2, p3], context.lighting)
            };
            let room = if context.lightmapped() {
                let centre_x = 0.25 * (p0[0] + p1[0] + p2[0] + p3[0]);
                let centre_y = 0.25 * (p0[1] + p1[1] + p2[1] + p3[1]);
                let centre_z = 0.25 * (p0[2] + p1[2] + p2[2] + p3[2]);
                context
                    .lighting
                    .room_index_at_height(centre_x, centre_y, centre_z)
            } else {
                None
            };
            scratch.clear();
            let first = scratch.len();
            add_quad(
                scratch, p0, colours[0], uv0, p1, colours[1], uv1, p2, colours[2], uv2, p3,
                colours[3], uv3,
            );
            stamp_lightmap_quad(
                context.lightmap,
                scratch,
                first,
                PatchKind::Wall,
                [p0, p1, p2, p3],
                room,
            );
            buckets.add_quads(key, scratch);
        }
    }
}

/// Step 5: placeholder boxes for every prop whose real model is unavailable
/// (unknown catalogue entry, missing file, malformed GLB).
///
/// Real prop geometry is added by `build_level_geometry_with_assets`, which
/// batches instances per model and draws them with their own texture.
fn emit_prop_fallbacks(
    context: &EmitContext<'_, '_>,
    catalog: &crate::loader::PropCatalog,
    fallback_props: &[&PropDef],
    buckets: &mut SpatialBuckets<SurfaceKey>,
    scratch: &mut Vec<Vertex>,
) {
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
        let base_y = context.surfaces.floor_y_at(prop.x, prop.z).unwrap_or(0.0);
        add_prop_box(scratch, prop, size, entry.color, base_y, context.lighting);
        // Whole run, not per quad: a placeholder box straddling a cell boundary
        // must stay one draw range, like the real prop geometry it stands in for.
        buckets.add_run(SurfaceKey::bare(SurfaceKind::PropFallback), scratch);
    }
}

/// Step 6: local surface markings (signs, floor arrows, warning marks).
///
/// They are static geometry like everything else, bucketed per cell, but drawn
/// in their own pass so the depth bias is explicit. An unknown material is
/// skipped, which is how a level referencing a decal sheet from a newer build
/// still loads. The key's material index is the decal's sheet: a generated
/// atlas slot or an external PNG sheet.
fn emit_decals(
    context: &EmitContext<'_, '_>,
    catalog: &crate::loader::PropCatalog,
    buckets: &mut SpatialBuckets<SurfaceKey>,
    scratch: &mut Vec<Vertex>,
) {
    for decal in &context.level.decals {
        let Some(sheet) = decal_sheet_index(context.level, catalog.assets(), &decal.material)
        else {
            continue;
        };
        let uv = if sheet < DECAL_EXTERNAL_BASE {
            decal_uv_rect(sheet)
        } else {
            decal_uv_rect_full()
        };
        scratch.clear();
        add_decal_quad(
            scratch,
            decal,
            context.surfaces,
            context.lighting,
            uv,
            context.level.ceiling_decal_rotation(decal),
        );
        if !scratch.is_empty() {
            let Some(material) = MaterialIndex::try_from(sheet).ok() else {
                continue;
            };
            buckets.add_run(SurfaceKey::new(SurfaceKind::Decal, material), scratch);
        }
    }
}

/// The material key for one named wall face: the `faces` override for its
/// direction, else the wall's own material, else the level default.
fn wall_face_key(context: &EmitContext<'_, '_>, wall: &WallDef, name: &str) -> SurfaceKey {
    let material = wall
        .face_ref(name)
        .unwrap_or_else(|| context.level.defaults.wall_ref());
    context.materials.key(MaterialSlot::Wall, material)
}

/// A wall face's albedo: its material's tint, scaled by the directional face
/// multiplier and the bottom/top gradient. Nothing here knows a material id:
/// the tint comes from the resolved table.
fn scaled_wall_color(
    materials: &MaterialLookup<'_>,
    key: SurfaceKey,
    mult: f32,
    grad: f32,
) -> [f32; 3] {
    let tint = materials.tint(key);
    [
        (tint[0] * mult * grad).min(1.0),
        (tint[1] * mult * grad).min(1.0),
        (tint[2] * mult * grad).min(1.0),
    ]
}
