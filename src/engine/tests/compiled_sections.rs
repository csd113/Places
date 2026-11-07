//! Small CPU-only emission fixtures protect compile-only section connection,
//! real door/window holes and the tall-skirt chart gradient restart. They do
//! not solve lighting, load images, bake shipped maps or create GPU resources.

use super::{
    LightmapEmit, MaterialLookup, SkirtFace, SurfaceKey, SurfaceKind, WallAxis, emit_skirt_face,
    finish_indexed_mesh, geometry, wall_layout,
};
use crate::engine::level::{LevelDef, LevelSurfaces};
use crate::engine::lighting::LevelLighting;
use crate::engine::lighting::lightmap::{LightmapConfig, LightmapPlan};
use crate::engine::loader::PropCatalog;
use crate::engine::materials::MaterialTable;
use crate::engine::spatial::SpatialBuckets;
use std::cell::RefCell;

fn level() -> Result<LevelDef, String> {
    LevelDef::from_json(r#"{
        "schema":"places-level","format_version":1,"id":"sections","name":"Sections",
        "spawn":{"x":1,"z":1},"rooms":[{"width":6,"depth":4,"height":3}],
        "walls":[
          {"x":0,"z":0,"width":2,"depth":0.2,"height":3,"openings":[{"kind":"door","offset":0.4,"width":1.2,"height":2.1}]},
          {"x":2,"z":0,"width":2,"depth":0.2,"height":3,"openings":[{"kind":"window","offset":0.5,"width":1,"height":1,"sill":1}]},
          {"x":4,"z":0,"width":2,"depth":0.2,"height":3}
        ]
    }"#).map_err(|error| error.to_string())
}

const fn config() -> LightmapConfig {
    LightmapConfig {
        texels_per_metre: 1.0,
        page_edge: 64,
        max_pages: 2,
        padding: 1,
        bytes_per_texel: 16,
    }
}

// Actual mesh samples protect holes, face winding, material phase and shade,
// not merely the optimizer's internal group count. Three sections cost <1 ms.
#[test]
fn matching_sections_connect_but_keep_openings_and_authoring() -> Result<(), String> {
    let level = level()?;
    let before = serde_json::to_string(&level).map_err(|error| error.to_string())?;
    let materials = MaterialTable::default();
    let surfaces = LevelSurfaces::new(&level);
    let lookup = MaterialLookup::new(&materials);
    assert_eq!(
        wall_layout(&level, &surfaces, &lookup, false).units.len(),
        3,
        "Static and editable vertex-lit previews retain their original interpolation"
    );
    assert_eq!(wall_layout(&level, &surfaces, &lookup, true).units.len(), 1);
    let mut plan = LightmapPlan::new(config());
    let mesh = geometry::build_level_geometry_mesh_with_lightmaps(
        &level,
        &PropCatalog::builtin(),
        &[],
        &LevelLighting::default(),
        &materials,
        Some(&mut plan),
    );
    let triangles = mesh.triangles_for(SurfaceKind::Wall);
    for (x, y, solid) in [
        (1.0_f32, 1.0_f32, false),
        (3.0, 1.5, false),
        (1.0, 2.5, true),
        (3.0, 0.5, true),
        (5.0, 1.5, true),
    ] {
        let hits: Vec<_> = triangles
            .as_chunks::<3>()
            .0
            .iter()
            .filter(|triangle| {
                triangle.iter().all(|v| (v.pos[2] - 0.2).abs() < 0.00001)
                    && covers_xy(*triangle, x, y)
            })
            .collect();
        assert_eq!(!hits.is_empty(), solid, "solid coverage at {x},{y}");
        assert!(
            hits.iter()
                .all(|triangle| triangle.iter().all(|v| v.normal[2] > 0.99)),
            "connected room-facing faces retain outward winding"
        );
    }
    let key = lookup.key(super::MaterialSlot::Wall, level.defaults.wall_ref());
    for vertex in triangles
        .iter()
        .filter(|v| v.normal[2] > 0.99 && (v.pos[2] - 0.2).abs() < 0.00001)
    {
        assert_eq!(
            vertex.uv,
            lookup.uv(key, vertex.pos[0], 3.0 - vertex.pos[1]),
            "opening fragments keep the parent world-space texture phase"
        );
        let gradient = (geometry::WALL_TOP_GRADIENT - geometry::WALL_BOTTOM_GRADIENT)
            .mul_add(vertex.pos[1] / 3.0, geometry::WALL_BOTTOM_GRADIENT);
        for (actual, tint) in vertex.color.iter().zip(lookup.tint(key)).take(3) {
            assert!(
                (*actual - (tint * super::WALL_FACE_SOUTH_MULT * gradient).min(1.0)).abs()
                    < 0.00001,
                "connected opening fragments retain the matching parent material gradient"
            );
        }
    }
    assert_eq!(
        serde_json::to_string(&level).map_err(|error| error.to_string())?,
        before,
        "geometry preparation must never rewrite editable authoring"
    );
    assert!(!plan.failed(), "small connected geometry remains chartable");
    Ok(())
}

fn covers_xy(triangle: &[super::Vertex], x: f32, y: f32) -> bool {
    let mut positive = false;
    let mut negative = false;
    for (a, b) in triangle.iter().zip(triangle.iter().cycle().skip(1)).take(3) {
        let cross =
            (b.pos[0] - a.pos[0]).mul_add(y - a.pos[1], -(b.pos[1] - a.pos[1]) * (x - a.pos[0]));
        positive |= cross > 0.00001;
        negative |= cross < -0.00001;
    }
    !(positive && negative)
}

// Small negative controls prevent geometry reduction from eating gaps,
// changing deliberately distinct shade frames, or swallowing material edges.
#[test]
fn section_connections_reject_gaps_frames_materials_and_overlay_bridges() -> Result<(), String> {
    let mut base = level()?;
    base.walls.truncate(2);
    let count = |level: &LevelDef| {
        let materials = MaterialTable::logical(level, PropCatalog::builtin().assets(), None);
        wall_layout(
            level,
            &LevelSurfaces::new(level),
            &MaterialLookup::new(&materials),
            true,
        )
        .units
        .len()
    };
    for case in 0_u8..4 {
        let mut level = base.clone();
        let wall = level.walls.get_mut(1).ok_or("second wall")?;
        match case {
            0 => wall.x += 0.0001,
            1 => {
                wall.y = 1.0;
                wall.height = Some(2.0);
            }
            2 => wall.height = None,
            _ => {
                wall.material = Some("core:pool_tile_wall_01".into());
            }
        }
        assert_eq!(count(&level), 2, "incompatible connection case {case}");
    }
    let mut layered = base.clone();
    let mut room = layered.rooms.first().cloned().ok_or("first room")?;
    room.width = 2.0;
    layered.rooms.push(room);
    assert_eq!(
        count(&layered),
        2,
        "different face-room memberships stay split"
    );
    let mut overlay = level()?;
    let mut wall = overlay.walls.first().cloned().ok_or("first wall")?;
    wall.height = Some(1.0);
    overlay.walls.push(wall);
    assert_eq!(
        count(&overlay),
        2,
        "the existing overlap group cannot pull a differently framed group into a touching chain"
    );
    Ok(())
}

// Joining A(index 0)+B(index 2) must not replace C(index 1)'s exposed
// cap material at B/C. One three-wall CPU mesh protects actual visible material
// precedence, beyond the conservative optimizer's group-count guard.
#[test]
fn section_connections_preserve_interleaved_cap_material_ownership() -> Result<(), String> {
    let mut level = level()?;
    level.walls.truncate(2);
    level.rooms.first_mut().ok_or("room")?.height = 4.0;
    let mut crossing = level.walls.first().cloned().ok_or("wall")?;
    crossing.x = 3.0;
    crossing.z = -0.2;
    crossing.width = 0.2;
    crossing.depth = 1.0;
    crossing.openings.clear();
    crossing.material = Some("core:pool_tile_wall_01".into());
    level.walls.insert(1, crossing);
    let materials = MaterialTable::logical(&level, PropCatalog::builtin().assets(), None);
    let lookup = MaterialLookup::new(&materials);
    let owner_key = lookup.key(
        super::MaterialSlot::Wall,
        level
            .walls
            .get(1)
            .ok_or("crossing wall")?
            .material_ref()
            .ok_or("crossing material")?,
    );
    let mut plan = LightmapPlan::new(config());
    let mesh = geometry::build_level_geometry_mesh_with_lightmaps(
        &level,
        &PropCatalog::builtin(),
        &[],
        &LevelLighting::default(),
        &materials,
        Some(&mut plan),
    );
    let mut owners = Vec::new();
    for range in &mesh.ranges {
        for triangle in range.indices.as_chunks::<3>().0 {
            let points: Vec<_> = triangle
                .iter()
                .filter_map(|index| range.vertices.get(usize::from(*index)))
                .map(|vertex| super::Vertex {
                    pos: [vertex.pos[0], vertex.pos[2], vertex.pos[1]],
                    ..*vertex
                })
                .collect();
            if points.len() == 3
                && points.iter().all(|v| (v.pos[2] - 3.0).abs() < 0.00001)
                && covers_xy(&points, 3.1, 0.1)
            {
                owners.push(range.key);
            }
        }
    }
    assert!(
        !owners.is_empty() && owners.iter().all(|key| *key == owner_key),
        "the crossing wall must retain its earlier-authored exposed cap material"
    );
    Ok(())
}

// The old emitter restarted .92->1.05 at every vertical chart tile. One four
// metre face with a two-metre preview cap reproduces that discontinuity without
// an atlas fill. Endpoint UVs and all shared world-height shades must agree.
#[test]
fn skirt_chart_tiles_retain_the_parent_gradient_and_uv_frame() -> Result<(), String> {
    let materials = MaterialTable::default();
    let lookup = MaterialLookup::new(&materials);
    let level = level()?;
    let key = lookup.key(super::MaterialSlot::Wall, level.defaults.wall_ref());
    let mut plan = LightmapPlan::new(config());
    let cell = RefCell::new(&mut plan);
    let mut buckets = SpatialBuckets::<SurfaceKey>::new(16.0);
    emit_skirt_face(
        &mut buckets,
        &mut Vec::new(),
        &lookup,
        &LevelLighting::default(),
        SkirtFace {
            axis: WallAxis::X,
            positive: true,
            at: 0.0,
            span: (0.0, 1.0),
            low: 0.0,
            high: 4.0,
            key,
            room: None,
        },
        Some(LightmapEmit {
            plan: Some(&cell),
            max_span_m: 2.0,
        }),
    );
    let mesh = finish_indexed_mesh(buckets);
    let vertices = mesh.all_vertices();
    let middle: Vec<_> = vertices
        .iter()
        .filter(|v| (v.pos[1] - 2.0).abs() < 0.00001)
        .collect();
    assert!(
        middle.len() >= 4,
        "both charts must expose their common edge"
    );
    let first = middle.first().ok_or("common edge")?;
    for vertex in &middle {
        assert_eq!(
            vertex.color, first.color,
            "chart subdivision must not restart material shade"
        );
        assert!(
            (vertex.uv[1] - first.uv[1]).abs() < 0.00001,
            "texture V keeps its parent high reference"
        );
        assert!(
            vertex.normal[0] > 0.99,
            "normal remains outward across chart cuts"
        );
    }
    let low = vertices
        .iter()
        .find(|v| v.pos[1].abs() < 0.00001)
        .ok_or("low vertex")?;
    let high = vertices
        .iter()
        .find(|v| (v.pos[1] - 4.0).abs() < 0.00001)
        .ok_or("high vertex")?;
    for ((actual, bottom), top) in first.color.iter().zip(low.color).zip(high.color).take(3) {
        assert!(
            (*actual - f32::midpoint(bottom, top)).abs() < 0.00001,
            "shared shade must interpolate the original full face"
        );
    }
    Ok(())
}

// Coplanar grid edges must simplify without crossing a real gap, material,
// room, normal or height discontinuity. Eight tiny records, no bake or assets.
#[test]
fn skirt_runs_merge_only_contiguous_compatible_receivers() -> Result<(), String> {
    let level = level()?;
    let materials = MaterialTable::default();
    let lookup = MaterialLookup::new(&materials);
    let key = lookup.key(super::MaterialSlot::Wall, level.defaults.wall_ref());
    let base = SkirtFace {
        axis: WallAxis::X,
        positive: true,
        at: 0.0,
        span: (0.0, 1.0),
        low: -1.0,
        high: 0.0,
        key,
        room: Some(0),
    };
    let faces = vec![
        SkirtFace {
            span: (1.0, 2.0),
            ..base
        },
        base,
        SkirtFace {
            span: (2.001, 3.0),
            ..base
        },
        SkirtFace {
            span: (3.0, 4.0),
            room: Some(1),
            ..base
        },
        SkirtFace {
            span: (4.0, 5.0),
            positive: false,
            ..base
        },
        SkirtFace {
            span: (5.0, 6.0),
            high: 1.0,
            ..base
        },
        SkirtFace {
            span: (6.0, 7.0),
            key: SurfaceKey {
                kind: SurfaceKind::Ceiling,
                ..key
            },
            ..base
        },
    ];
    let merged = super::merge_skirt_faces(faces);
    assert_eq!(merged.len(), 6, "only one compatible pair coalesces");
    assert!(
        merged.iter().any(
            |face| face.span.0.abs() < f32::EPSILON && (face.span.1 - 2.0).abs() < f32::EPSILON
        )
    );
    assert!(
        !merged
            .iter()
            .any(|face| face.span.0 < 2.0 && face.span.1 > 2.0),
        "an actual gap cannot be filled"
    );
    Ok(())
}
