//! Wall profiles retain one physically supported roof across mesh and collision.

use super::{CeilingProfileDef, LevelDef, LevelSurfaces, WallAxis, wall_solid_slices_profiled};

fn boundary_level() -> Result<LevelDef, String> {
    LevelDef::from_json(
        r#"{
        "format_version": 3,
        "id": "wall_roof_contact",
        "name": "Wall Roof Contact",
        "spawn": { "x": 15.0, "z": 3.0 },
        "rooms": [
            { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 6.0, "height": 2.8 },
            { "x": 12.0, "z": 0.0, "width": 6.0, "depth": 6.0, "height": 2.8,
              "ceiling": { "kind": "gable", "ridge": "x", "ridge_rise": 1.2 } },
            { "x": 12.0, "z": -4.0, "width": 6.0, "depth": 4.0, "height": 5.0,
              "ceiling": { "kind": "open" } }
        ],
        "walls": [
            { "x": 11.8, "z": 0.0, "width": 0.2, "depth": 6.0 },
            { "x": 18.0, "z": 0.0, "width": 0.2, "depth": 6.0 },
            { "x": 11.8, "z": -0.2, "width": 6.4, "depth": 0.2 },
            { "x": 11.8, "z": 6.0, "width": 6.4, "depth": 0.2 }
        ]
    }"#,
    )
    .map_err(|error| error.to_string())
}

#[test]
fn boundary_gable_profiles_and_collision_reach_the_ridge() -> Result<(), String> {
    let level = boundary_level()?;
    let surfaces = LevelSurfaces::new(&level);
    for wall in level.walls.iter().filter(|wall| wall.axis() == WallAxis::Z) {
        assert_eq!(
            surfaces.wall_profile_breaks(wall),
            vec![3.0],
            "gable ridge must cut the wall profile"
        );
        for (offset, expected) in [(0.0, 2.8), (1.5, 3.4), (3.0, 4.0), (6.0, 2.8)] {
            assert!(
                (surfaces.clear_ceiling_height_along(wall, offset) - expected).abs() < 1.0e-5,
                "outside wall centre must follow its contacting roof"
            );
        }
        let slices = wall_solid_slices_profiled(
            wall,
            |offset| surfaces.clear_ceiling_height_along(wall, offset),
            &surfaces.wall_profile_breaks(wall),
        );
        assert_eq!(slices.len(), 2, "gable has two linear wall spans");
        assert!(
            slices.iter().all(|slice| (slice.top - 4.0).abs() < 1.0e-5),
            "collision slices conservatively reach the same ridge"
        );
    }
    for wall in level.walls.iter().filter(|wall| wall.axis() == WallAxis::X) {
        assert!(
            (surfaces.ceiling_y_along(wall, wall.length() * 0.5) - 2.8).abs() < 1.0e-5,
            "open apron cannot displace the contacting closed eave"
        );
    }
    Ok(())
}

#[test]
fn boundary_gable_mesh_keeps_both_rake_edges() -> Result<(), String> {
    let level = boundary_level()?;
    let mesh = crate::render::build_level_geometry(&level);
    for face in [12.0, 18.0] {
        let vertices = mesh
            .ranges
            .iter()
            .filter(|range| range.key.kind == crate::render::SurfaceKind::Wall)
            .flat_map(|range| &range.vertices)
            .filter(|vertex| (vertex.pos[0] - face).abs() < 1.0e-5 && vertex.pos[1] > 2.8 + 1.0e-5)
            .collect::<Vec<_>>();
        assert!(
            !vertices.is_empty(),
            "gable end must contain real geometry above the eave"
        );
        assert!(
            vertices
                .iter()
                .any(|vertex| (vertex.pos[1] - 4.0).abs() < 1.0e-5),
            "gable end must reach the ridge"
        );
        assert!(
            vertices.iter().all(|vertex| (vertex.pos[1]
                - (vertex.pos[2] - 3.0).abs().mul_add(-0.4, 4.0))
            .abs()
                < 1.0e-5),
            "every raised rake vertex must lie on the actual gable"
        );
    }
    Ok(())
}

#[test]
fn a_real_gap_cannot_select_a_nearby_roof() -> Result<(), String> {
    let mut level = boundary_level()?;
    let authored_wall = level
        .walls
        .first_mut()
        .ok_or_else(|| "boundary wall".to_owned())?;
    authored_wall.x -= 0.02;
    let surfaces = LevelSurfaces::new(&level);
    let wall = level
        .walls
        .first()
        .ok_or_else(|| "boundary wall".to_owned())?;
    assert!(
        surfaces.wall_ceiling_room(wall, 3.0).is_none(),
        "a real gap has no supported roof"
    );
    assert!(
        surfaces.wall_profile_breaks(wall).is_empty(),
        "a disconnected roof cannot inject a ridge"
    );
    assert!(
        (surfaces.clear_ceiling_height_along(wall, 3.0) - 2.8).abs() < 1.0e-5,
        "disconnected wall retains its historical off-room fallback"
    );
    Ok(())
}

#[test]
fn a_containing_closed_room_keeps_overlap_precedence() -> Result<(), String> {
    let mut level = boundary_level()?;
    let room = level
        .rooms
        .first_mut()
        .ok_or_else(|| "first room".to_owned())?;
    room.x = 11.0;
    room.width = 8.0;
    room.height = 3.1;
    let surfaces = LevelSurfaces::new(&level);
    let wall = level
        .walls
        .first()
        .ok_or_else(|| "boundary wall".to_owned())?;
    assert!(
        (surfaces.clear_ceiling_height_along(wall, 3.0) - 3.1).abs() < 1.0e-5,
        "containing closed room keeps existing first-room precedence"
    );
    assert!(
        surfaces.wall_profile_breaks(wall).is_empty(),
        "later overlapping gable cannot steal ownership"
    );
    Ok(())
}

fn adjacent_roof_level(gap: bool) -> Result<LevelDef, String> {
    let mut level = boundary_level()?;
    level.walls.truncate(1);
    level
        .walls
        .first_mut()
        .ok_or_else(|| "side wall".to_owned())?
        .depth = 12.0;
    let mut next = level
        .rooms
        .get(1)
        .cloned()
        .ok_or_else(|| "first roof".to_owned())?;
    next.z = if gap { 7.0 } else { 6.0 };
    next.depth = if gap { 5.0 } else { 6.0 };
    next.height = 4.0;
    next.ceiling = CeilingProfileDef::Gable {
        ridge: WallAxis::X,
        ridge_rise: 0.6,
    };
    if gap {
        level
            .rooms
            .get_mut(1)
            .ok_or_else(|| "first roof".to_owned())?
            .depth = 5.0;
    }
    level.rooms.push(next);
    Ok(level)
}

#[test]
fn adjacent_roofs_resolve_each_sample_and_split_every_ridge_and_room_transition()
-> Result<(), String> {
    let level = adjacent_roof_level(false)?;
    let surfaces = LevelSurfaces::new(&level);
    let wall = level.walls.first().ok_or_else(|| "side wall".to_owned())?;
    for (offset, expected) in [(1.5, 3.4), (3.0, 4.0), (7.5, 4.3), (9.0, 4.6)] {
        assert!(
            (surfaces.clear_ceiling_height_along(wall, offset) - expected).abs() < 1.0e-5,
            "a sample cannot borrow the adjacent room's roof"
        );
    }
    assert_eq!(
        surfaces.wall_profile_breaks(wall),
        vec![3.0, 6.0, 9.0],
        "both ridges and the room transition must split the wall profile"
    );
    Ok(())
}

#[test]
fn adjacent_roof_mesh_join_retains_both_exact_endpoint_heights() -> Result<(), String> {
    let level = adjacent_roof_level(false)?;
    let mesh = crate::render::build_level_geometry(&level);
    let joining_vertices = mesh
        .ranges
        .iter()
        .filter(|range| range.key.kind == crate::render::SurfaceKind::Wall)
        .flat_map(|range| &range.vertices)
        .filter(|vertex| {
            (vertex.pos[0] - 12.0).abs() < 1.0e-5 && (vertex.pos[2] - 6.0).abs() < 1.0e-5
        })
        .map(|vertex| vertex.pos[1])
        .collect::<Vec<_>>();
    for expected in [2.8, 4.0] {
        assert!(
            joining_vertices
                .iter()
                .any(|height| (*height - expected).abs() < 1.0e-5),
            "each roof owns its exact shared wall endpoint height"
        );
    }
    Ok(())
}

#[test]
fn adjacent_roof_collision_spans_cannot_borrow_a_neighbours_endpoint_height() -> Result<(), String>
{
    for next_height in [2.0, 4.0] {
        let mut level = adjacent_roof_level(false)?;
        level
            .rooms
            .last_mut()
            .ok_or_else(|| "adjacent roof".to_owned())?
            .height = next_height;
        let surfaces = LevelSurfaces::new(&level);
        let wall = level.walls.first().ok_or_else(|| "side wall".to_owned())?;
        let slices = wall_solid_slices_profiled(
            wall,
            |offset| surfaces.clear_ceiling_height_along(wall, offset),
            &surfaces.wall_profile_breaks(wall),
        );
        for slice in slices {
            let owner = surfaces
                .wall_ceiling_room(wall, f32::midpoint(slice.start, slice.end))
                .ok_or_else(|| "supported linear span".to_owned())?;
            let endpoint_height = |offset| {
                let (x, z) = super::wall_point(wall, offset);
                owner.ceiling_y_at(x, z) - owner.floor_y + wall.y
            };
            let maximum = endpoint_height(slice.start).max(endpoint_height(slice.end));
            assert!(
                (slice.top - maximum).abs() < 1.0e-5,
                "collision top must be the conservative maximum of its own roof span"
            );
        }
    }
    Ok(())
}

#[test]
fn an_internal_roof_gap_is_not_a_corner_extension() -> Result<(), String> {
    let level = adjacent_roof_level(true)?;
    let surfaces = LevelSurfaces::new(&level);
    let wall = level.walls.first().ok_or_else(|| "side wall".to_owned())?;
    for offset in [5.1, 6.0, 6.9] {
        assert!(
            surfaces.wall_ceiling_room(wall, offset).is_none(),
            "an interior roof gap must have no closed roof owner"
        );
    }
    assert_eq!(
        surfaces.wall_profile_breaks(wall),
        vec![2.5, 5.0, 7.0, 9.5],
        "real gap boundaries and both ridges must remain explicit"
    );
    Ok(())
}

#[test]
fn outer_roof_corner_extensions_are_bounded_by_wall_thickness() -> Result<(), String> {
    let mut level = boundary_level()?;
    level.walls.truncate(1);
    let authored_wall = level
        .walls
        .first_mut()
        .ok_or_else(|| "side wall".to_owned())?;
    authored_wall.z = -0.2;
    authored_wall.depth = 6.4;
    let surfaces = LevelSurfaces::new(&level);
    let wall = level.walls.first().ok_or_else(|| "side wall".to_owned())?;
    for offset in [0.0, 6.4] {
        assert!(
            surfaces.wall_ceiling_room(wall, offset).is_some(),
            "the actual wall-thickness corner must retain its roof profile"
        );
    }
    let mut extended = wall.clone();
    extended.z = -0.3;
    extended.depth = 6.6;
    for offset in [0.0, 6.6] {
        assert!(
            surfaces.wall_ceiling_room(&extended, offset).is_none(),
            "an outer extension beyond the wall's thickness cannot borrow the roof"
        );
    }
    Ok(())
}

fn upward_wall_area_at(mesh: &crate::render::LevelMesh, height: f32) -> f32 {
    mesh.triangles_for(crate::render::SurfaceKind::Wall)
        .as_chunks::<3>()
        .0
        .iter()
        .filter(|triangle| {
            triangle
                .iter()
                .all(|vertex| (vertex.pos[1] - height).abs() < 1.0e-5)
        })
        .map(|triangle| {
            let [first, second, third] = triangle;
            let a = first.pos;
            let b = second.pos;
            let c = third.pos;
            // Positive cross-product Y is the actual up-facing cap winding.
            (b[2] - a[2])
                .mul_add(c[0] - a[0], -(b[0] - a[0]) * (c[2] - a[2]))
                .max(0.0)
                * 0.5
        })
        .sum()
}

#[test]
fn rigid_boundary_wall_caps_keep_only_the_area_outside_real_roof_coverage() -> Result<(), String> {
    let boundary = LevelDef::from_json(
        r#"{
        "format_version": 3,
        "id": "rigid_wall_caps",
        "name": "Rigid Wall Caps",
        "spawn": { "x": 4.0, "z": 3.0 },
        "rooms": [
            { "x": 0.0, "z": 0.0, "width": 8.15, "depth": 6.0, "height": 2.8 },
            { "x": 8.15, "z": 0.0, "width": 2.35, "depth": 9.0, "height": 2.8 },
            { "x": -1.0, "z": -7.0, "width": 12.5, "depth": 7.0, "height": 8.0,
              "ceiling": { "kind": "open" } }
        ],
        "walls": [
            { "x": -0.3, "z": -0.3, "width": 11.1, "depth": 0.3, "height": 2.8,
              "openings": [{ "kind": "window", "offset": 2.1, "width": 3.2,
                "sill": 0.95, "height": 1.25, "solid": true }] }
        ]
    }"#,
    )
    .map_err(|error| error.to_string())?;
    for (covered_depth, expected_area) in [(0.0, 3.33), (0.15, 1.665), (0.3, 0.0)] {
        let mut level = boundary.clone();
        if covered_depth > 0.0 {
            let mut covering_roof = level
                .rooms
                .first()
                .cloned()
                .ok_or_else(|| "closed roof template".to_owned())?;
            covering_roof.x = -0.3;
            covering_roof.width = 11.1;
            covering_roof.z = -covered_depth;
            covering_roof.depth = covered_depth;
            level.rooms.push(covering_roof);
        }
        for quality in [
            crate::quality::LightmapQuality::Off,
            crate::quality::LightmapQuality::Medium,
            crate::quality::LightmapQuality::Full,
        ] {
            let materials = crate::render::logical_materials(&level);
            let prepared = crate::render::prepare_level_geometry_with_lightmaps(
                &level,
                &crate::loader::PropCatalog::builtin(),
                &mut crate::props::PropAssets::default(),
                &materials,
                crate::render::LightmapBuildOptions::for_lightmaps(quality),
                None,
            );
            assert!(
                prepared.build.lightmap_failure.is_none(),
                "the cap fixture must retain its requested chart preparation"
            );
            assert_eq!(
                prepared.fill.is_some(),
                quality != crate::quality::LightmapQuality::Off,
                "lightmapped cap geometry must be tested before filling the atlas"
            );
            let area = upward_wall_area_at(&prepared.build.mesh, 2.8);
            assert!(
                (area - expected_area).abs() < 1.0e-5,
                "{quality:?} cap area {area} must equal {expected_area} with roof depth {covered_depth}"
            );
        }
    }
    Ok(())
}

#[test]
fn descending_roof_spans_keep_each_exact_face_height_across_wall_thickness() -> Result<(), String> {
    let level = LevelDef::from_json(
        r#"{
        "format_version": 3,
        "id": "descending_roof_faces",
        "name": "Descending Roof Faces",
        "spawn": { "x": 15.0, "z": 3.0 },
        "rooms": [
            { "x": 12.0, "z": 0.0, "width": 6.0, "depth": 6.0, "height": 4.0,
              "floor_y": 0.7, "ceiling": { "kind": "gable", "ridge": "z", "ridge_rise": 1.2 } },
            { "x": 12.0, "z": 6.0, "width": 6.0, "depth": 6.0, "height": 2.8,
              "floor_y": 0.7, "ceiling": { "kind": "gable", "ridge": "z", "ridge_rise": 1.2 } }
        ],
        "walls": [
            { "x": 12.0, "z": 0.0, "width": 0.2, "depth": 12.0, "y": 0.7,
              "openings": [{ "kind": "window", "offset": 8.0, "width": 1.0,
                "sill": 0.9, "height": 1.0, "solid": false }] }
        ]
    }"#,
    )
    .map_err(|error| error.to_string())?;
    for quality in [
        crate::quality::LightmapQuality::Off,
        crate::quality::LightmapQuality::Medium,
        crate::quality::LightmapQuality::Full,
    ] {
        let materials = crate::render::logical_materials(&level);
        let prepared = crate::render::prepare_level_geometry_with_lightmaps(
            &level,
            &crate::loader::PropCatalog::builtin(),
            &mut crate::props::PropAssets::default(),
            &materials,
            crate::render::LightmapBuildOptions::for_lightmaps(quality),
            None,
        );
        assert!(
            prepared.build.lightmap_failure.is_none(),
            "descending roof control must preserve its requested geometry preparation"
        );
        assert_eq!(
            prepared.fill.is_some(),
            quality != crate::quality::LightmapQuality::Off,
            "each lightmapped geometry variant must retain a fill plan"
        );
        let wall_triangles = prepared
            .build
            .mesh
            .triangles_for(crate::render::SurfaceKind::Wall);
        for (face, high_roof, low_roof) in [(12.0, 4.7, 3.5), (12.2, 4.78, 3.58)] {
            // Requiring every triangle vertex on the same X plane selects
            // length faces rather than a reveal's incidental corner vertex.
            let face_points = wall_triangles
                .as_chunks::<3>()
                .0
                .iter()
                .filter(|triangle| {
                    triangle
                        .iter()
                        .all(|vertex| (vertex.pos[0] - face).abs() < 1.0e-5)
                })
                .flat_map(|triangle| triangle.iter().map(|vertex| vertex.pos))
                .collect::<Vec<_>>();
            for (along, height) in [
                (0.0, high_roof),
                (6.0, high_roof),
                (6.0, low_roof),
                (12.0, low_roof),
                (8.0, 1.6),
                (9.0, 1.6),
            ] {
                assert!(
                    face_points.iter().any(|point| {
                        (point[2] - along).abs() < 1.0e-5 && (point[1] - height).abs() < 1.0e-5
                    }),
                    "{quality:?} length face x={face} must retain exact endpoint z={along}, y={height}; points={face_points:?}"
                );
            }
            assert!(
                face_points
                    .iter()
                    .filter(|point| point[2] > 6.0 + 1.0e-5 && point[1] > 3.3)
                    .all(|point| (point[1] - low_roof).abs() < 1.0e-5),
                "{quality:?} descending span must follow its actual face roof throughout"
            );
        }
    }
    Ok(())
}

#[test]
fn exposed_wall_caps_keep_the_parent_above_neighbours_and_real_room_ownership_below()
-> Result<(), String> {
    let level = LevelDef::from_json(
        r#"{
        "format_version": 3,
        "id": "wall_cap_rooms",
        "name": "Wall Cap Rooms",
        "spawn": { "x": 4.0, "z": 4.0 },
        "rooms": [
            { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 8.0, "height": 4.5 },
            { "x": 4.0, "z": -8.0, "width": 2.0, "depth": 8.0, "height": 2.5 }
        ],
        "walls": [
            { "x": 0.0, "z": -0.3, "width": 10.0, "depth": 0.3, "height": 4.5,
              "openings": [{ "kind": "window", "offset": 4.3, "width": 1.4,
                "sill": 1.0, "height": 1.1, "solid": false }] }
        ]
    }"#,
    )
    .map_err(|error| error.to_string())?;
    for quality in [
        crate::quality::LightmapQuality::Medium,
        crate::quality::LightmapQuality::Full,
    ] {
        let materials = crate::render::logical_materials(&level);
        let prepared = crate::render::prepare_level_geometry_with_lightmaps(
            &level,
            &crate::loader::PropCatalog::builtin(),
            &mut crate::props::PropAssets::default(),
            &materials,
            crate::render::LightmapBuildOptions::for_lightmaps(quality),
            None,
        );
        assert!(
            prepared.build.lightmap_failure.is_none(),
            "cap ownership control must prepare without a lightmap failure"
        );
        let fill = prepared
            .fill
            .ok_or_else(|| "cap ownership control must retain its fill plan".to_owned())?;
        for (height, expected_room) in [(4.5, 0), (1.0, 1), (2.1, 1)] {
            let mut checked = 0_usize;
            for (patch, _) in &fill.charts {
                if patch.kind == crate::lighting::lightmap::PatchKind::Wall
                    && patch.u_axis[1].abs() < 1.0e-5
                    && patch.v_axis[1].abs() < 1.0e-5
                    && (patch.origin[1] - height).abs() < 1.0e-5
                {
                    assert_eq!(
                        patch.room,
                        Some(expected_room),
                        "{quality:?} horizontal cap at y={height} must retain its vertical room contract: {patch:?}"
                    );
                    checked += 1_usize;
                }
            }
            assert!(
                checked > 0_usize,
                "{quality:?} must contain real cap charts at y={height}"
            );
        }
    }
    Ok(())
}
