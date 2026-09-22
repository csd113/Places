//! The static level mesh emitter.
//!
//! Floors, ceilings, recess skirts, walls, fixtures and decals are written into
//! a scratch buffer per material run and split into spatial batches on the way
//! into the mesh, so the emitting code itself is free of grid awareness.

use super::*;

pub(super) fn build_level_geometry_mesh(
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
                // the cross quad takes the light of the face on its own side,
                // sampled in the room that face opens into. This carries
                // doorway light through the jamb instead of dropping to ambient
                // in the wall cavity, and it carries it from the correct room
                // when the wall divides two of them.
                //
                // A cross-section always sits where the wall's solid profile
                // changes, so its own position is frequently a room boundary (a
                // shared divider) or inside the perpendicular wall (a buried
                // wall end). The room is therefore resolved just *inside* the
                // solid side of the boundary, where the face is unambiguous,
                // and the sample is taken from there.
                let covers = |intervals: &[(f32, f32)]| {
                    intervals
                        .iter()
                        .any(|(low, high)| *low <= bottom + 1e-3 && *high >= top - 1e-3)
                };
                let left_covers = covers(&left);
                let right_covers = covers(&right);
                let inward = if right_covers { 1.0 } else { -1.0 };
                let inboard = at + inward * LIGHT_FACE_PROBE_M;
                let face_sample =
                    |side: f32, side_normal: f32, y: f32| -> crate::lighting::LightColor {
                        let (px, pz, nx, nz) = match axis {
                            WallAxis::X => (inboard, side, 0.0, side_normal),
                            WallAxis::Z => (side, inboard, side_normal, 0.0),
                        };
                        let room = lighting.face_room(px, pz, nx, nz);
                        let probe = side_normal.mul_add(LIGHT_FACE_PROBE_M, side);
                        match axis {
                            WallAxis::X => lighting.sample_face(room, inboard, y, probe),
                            WallAxis::Z => lighting.sample_face(room, probe, y, inboard),
                        }
                    };
                let (bottom_t0, bottom_t1, top_t0, top_t1) = (
                    face_sample(t0, -1.0, bottom),
                    face_sample(t1, 1.0, bottom),
                    face_sample(t0, -1.0, top),
                    face_sample(t1, 1.0, top),
                );
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
