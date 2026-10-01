//! Probe-specific independent transport and placement oracles.
use super::*;

#[test]
fn parallel_receiver_work_is_bounded_and_interleaved() {
    let assignments = parallel_map(144, 12, None, |index| {
        let name = std::thread::current().name().map(str::to_owned);
        (index, name)
    })
    .expect("worker assignment");
    for (index, (actual_index, name)) in assignments.iter().enumerate() {
        assert_eq!(*actual_index, index);
        assert_eq!(
            name.as_deref(),
            Some(format!("transport-solve-{}", index % 12).as_str())
        );
    }
}

fn probe_charts() -> Vec<(LightmapPatch, Chart)> {
    let floor = floor_patch(0.0, 0.0, 6.0, 6.0, 8, 8);
    let ceiling = (
        LightmapPatch {
            origin: [0.0, 3.0, 0.0],
            u_axis: [6.0, 0.0, 0.0],
            v_axis: [0.0, 0.0, 6.0],
            room: Some(0),
            kind: PatchKind::Ceiling,
        },
        floor.1,
    );
    vec![floor, ceiling]
}

fn room_shell() -> Vec<TransportTriangle> {
    let mut triangles = floor(0.0, 0.0, 6.0, 6.0, [0.6; 3]);
    for corners in [
        [
            [0.0, 3.0, 0.0],
            [6.0, 3.0, 0.0],
            [6.0, 3.0, 6.0],
            [0.0, 3.0, 6.0],
        ],
        [
            [0.0, 0.0, 0.0],
            [6.0, 0.0, 0.0],
            [6.0, 3.0, 0.0],
            [0.0, 3.0, 0.0],
        ],
        [
            [6.0, 0.0, 6.0],
            [0.0, 0.0, 6.0],
            [0.0, 3.0, 6.0],
            [6.0, 3.0, 6.0],
        ],
        [
            [0.0, 0.0, 6.0],
            [0.0, 0.0, 0.0],
            [0.0, 3.0, 0.0],
            [0.0, 3.0, 6.0],
        ],
        [
            [6.0, 0.0, 0.0],
            [6.0, 0.0, 6.0],
            [6.0, 3.0, 6.0],
            [6.0, 3.0, 0.0],
        ],
    ] {
        triangles.extend(wall(
            corners[0], corners[1], corners[2], corners[3], [0.6; 3],
        ));
    }
    triangles
}

fn bake(scene: &TransportScene, bounces: u8, workers: usize) -> TransportSolve {
    scene
        .solve_with_probes(
            &probe_charts(),
            SolveOptions {
                workers,
                ..options(bounces, 3)
            },
            None,
            true,
        )
        .expect("probe bake")
}

#[test]
fn escaping_probe_rays_see_authored_linear_hdr_sky() {
    // No surfaces: all rays see the same environment, whose analytic mean is L.
    let sky = [2.0, 0.125, 0.001];
    let scene = TransportScene::new(Vec::new(), Vec::new())
        .expect("scene")
        .with_sky(sky);
    let field = bake(&scene, 1, 1).probes.expect("field");
    for probe in field.probes {
        for (actual, expected) in probe.irradiance.iter().zip(sky) {
            assert!(
                (*actual - expected).abs() <= 2.0e-6,
                "sky mean must remain linear HDR: {probe:?}"
            );
        }
        assert!(
            length(probe.direction) <= 1.0e-6,
            "uniform sky has no directional bias: {probe:?}"
        );
    }
}

#[test]
fn sky_is_injected_once_across_diffuse_orders() {
    let scene = TransportScene::new(floor(0.0, 0.0, 6.0, 6.0, [0.0; 3]), Vec::new())
        .expect("scene")
        .with_sky([0.5; 3]);
    let charts = vec![floor_patch(0.0, 0.0, 6.0, 6.0, 8, 8)];
    let first = scene
        .solve(&charts, options(1, 1), None)
        .expect("first order");
    let third = scene
        .solve(&charts, options(3, 1), None)
        .expect("third order");
    assert_eq!(
        first.charts, third.charts,
        "black surfaces cannot reflect another order of sky light"
    );
}

#[test]
fn enclosed_dark_room_and_wall_separation_preserve_darkness() {
    let mut triangles = room_shell();
    triangles.extend(wall(
        [3.0, 0.0, 0.0],
        [3.0, 0.0, 6.0],
        [3.0, 3.0, 6.0],
        [3.0, 3.0, 0.0],
        [0.6; 3],
    ));
    let dark = TransportScene::new(triangles.clone(), Vec::new())
        .expect("dark scene")
        .with_sky([3.0; 3]);
    let dark_field = bake(&dark, 2, 1).probes.expect("dark field");
    assert!(
        dark_field
            .probes
            .iter()
            .all(|probe| probe.irradiance == [0.0; 3]),
        "sealed room cannot receive sky"
    );
    let lit = TransportScene::new(triangles, vec![point([1.5, 2.5, 3.0], 2.0)]).expect("lit scene");
    let solved = bake(&lit, 2, 1);
    let field = solved.probes.expect("field");
    for (index, probe) in field.probes.iter().enumerate() {
        let (x, _, _) = lattice_from_index(index, field.dims_usize());
        let px = field.min[0] + (x as f32 + 0.5) * field.cell_m;
        if px > 3.0 {
            assert_eq!(
                probe.irradiance, [0.0; 3],
                "sealed divider at probe {index}"
            );
        } else {
            assert!(
                probe.irradiance[0] > 0.01,
                "lit room probe {index}: {probe:?}"
            );
        }
    }
}

#[test]
fn probe_bakes_are_bit_identical_across_workers_and_repeated_builds() {
    let scene =
        TransportScene::new(room_shell(), vec![point([3.0, 2.5, 3.0], 2.0)]).expect("scene");
    let serial = bake(&scene, 2, 1).probes.expect("serial");
    assert_eq!(serial, bake(&scene, 2, 12).probes.expect("parallel"));
    assert_eq!(serial, bake(&scene, 2, 1).probes.expect("repeat"));
}

#[test]
fn neighboring_probes_share_a_smooth_diffuse_estimator() {
    // A uniformly emitting surrounding sphere is represented here by the
    // analytic all-sky case: identical neighbors must have identical moments.
    let scene = TransportScene::new(Vec::new(), Vec::new())
        .expect("scene")
        .with_sky([0.2, 0.3, 0.4]);
    let field = bake(&scene, 1, 12).probes.expect("field");
    for pair in field.probes.windows(2) {
        assert_eq!(pair[0], pair[1]);
    }
    let room = TransportScene::new(room_shell(), vec![point([3.0, 2.5, 3.0], 2.0)])
        .expect("uniformly lit room");
    let mut field = bake(&room, 2, 12).probes.expect("room field");
    field.assign_rooms(|_| Some(0));
    let dims = field.dims_usize();
    for (index, probe) in field.probes.iter().enumerate() {
        let (x, y, z) = lattice_from_index(index, dims);
        for neighbor in [
            (x + 1 < dims[0]).then_some(index + 1),
            (y + 1 < dims[1]).then_some(index + dims[0]),
            (z + 1 < dims[2]).then_some(index + dims[0] * dims[1]),
        ]
        .into_iter()
        .flatten()
        {
            let a = channel_luminance(probe.irradiance);
            let b = channel_luminance(field.probes[neighbor].irradiance);
            assert!(a.min(b) > 0.01);
            assert!(
                a.max(b) / a.min(b) < 10.0,
                "adjacent room probes {index}/{neighbor}: {a}/{b}"
            );
        }
    }
}

#[test]
fn a_capped_lattice_covers_low_air_spaces_and_uses_world_coordinates() {
    let positions = [[-200.0, 10.0, 50.0], [800.0, 13.0, 56.0]];
    let (min, cell, dims) = probe_lattice(&positions).expect("lattice");
    assert!(
        dims.iter()
            .all(|count| *count <= crate::lighting::probes::MAX_PROBE_CELLS)
    );
    assert_eq!(
        dims[1], 1,
        "dimensions must be recomputed after increasing the shared cell size"
    );
    let first_y = min[1] + 0.5 * cell;
    assert!(
        (first_y - 11.5).abs() < 1.0e-4,
        "one vertical probe is centered in air: {first_y}"
    );
    let center_x = min[0] + 0.5 * dims[0] as f32 * cell;
    assert!(
        (center_x - 300.0).abs() < 1.0e-4,
        "world origin offset must survive"
    );
}

#[test]
fn recovery_fill_is_visibility_gated_across_a_sealed_wall() {
    let mut triangles = room_shell();
    triangles.extend(wall(
        [3.0, 0.0, 0.0],
        [3.0, 0.0, 6.0],
        [3.0, 3.0, 6.0],
        [3.0, 3.0, 0.0],
        [0.6; 3],
    ));
    let charts = probe_charts();
    let seed =
        TransportScene::new(triangles.clone(), vec![point([1.5, 2.5, 3.0], 2.0)]).expect("scene");
    let receivers = seed.receivers(&charts).expect("receivers");
    let (min, cell, dims) =
        probe_lattice(receivers.iter().map(|receiver| &receiver.position)).expect("lattice");
    let targets = (0..dims.iter().product())
        .map(|index| {
            let (x, y, z) = lattice_from_index(index, dims);
            ProbeTarget {
                position: [
                    min[0] + (x as f32 + 0.5) * cell,
                    min[1] + (y as f32 + 0.5) * cell,
                    min[2] + (z as f32 + 0.5) * cell,
                ],
                target: [0.5; 3],
                room: 0,
            }
        })
        .collect();
    let scene = seed
        .with_probe_target(targets)
        .with_receiver_target(vec![[0.5; 3]; receivers.len()]);
    let solved = bake(&scene, 0, 1);
    let parallel = bake(&scene, 0, 12);
    assert_eq!(solved.solution.charts, parallel.solution.charts);
    assert_eq!(solved.probes, parallel.probes);
    let field = solved.probes.expect("field");
    for (index, probe) in field.probes.iter().enumerate() {
        let (x, _, _) = lattice_from_index(index, field.dims_usize());
        if min[0] + (x as f32 + 0.5) * cell > 3.0 {
            assert_eq!(probe.irradiance, [0.0; 3]);
        }
    }
    for chart in solved.solution.charts {
        for (receiver, texel) in chart.receivers.iter().zip(chart.texels) {
            if receiver.position[0] > 3.1 {
                assert_eq!(texel.irradiance, [0.0; 3]);
            }
        }
    }
}

#[test]
fn closed_solid_and_surface_clearance_reject_embedded_probes() {
    // A room shell is wound into air. Reverse every face to create a solid
    // box with outward normals, as a correctly transformed prop mesh uses.
    let triangles = room_shell()
        .into_iter()
        .map(|triangle| {
            TransportTriangle::new(triangle.p0, triangle.p2, triangle.p1, triangle.albedo)
                .expect("outward triangle")
        })
        .collect();
    let scene = TransportScene::new(triangles, Vec::new()).expect("solid scene");
    assert!(!scene.probe_is_clear([3.0, 1.5, 3.0]), "inside closed prop");
    assert!(
        !scene.probe_is_clear([6.02, 1.5, 3.0]),
        "too close to opaque face"
    );
    assert!(scene.probe_is_clear([6.5, 1.5, 3.0]), "clear exterior air");
    let field = bake(&scene, 0, 1).probes.expect("field");
    assert!(
        field
            .probes
            .iter()
            .all(|probe| probe.irradiance == [0.0; 3])
    );
}

#[test]
fn invalid_transport_input_and_overflow_fail_before_probe_compression() {
    assert!(
        TransportTriangle::new([0.0; 3], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [f32::NAN; 3]).is_none()
    );
    let mut emitter = point([1.0, 2.0, 1.0], 1.0);
    emitter.position[1] = f32::INFINITY;
    assert!(TransportScene::new(Vec::new(), vec![emitter]).is_none());
    let sky = TransportScene::new(Vec::new(), Vec::new())
        .expect("scene")
        .with_sky([f32::INFINITY; 3]);
    assert_eq!(
        sky.solve_with_probes(&probe_charts(), options(1, 1), None, true),
        Err(LightmapFailure::FillNonFinite)
    );
    let mut huge = point([3.0, 2.0, 3.0], f32::MAX);
    huge.height_factor = f32::MAX;
    let product_overflow = TransportScene::new(Vec::new(), vec![huge]).expect("finite input");
    assert_eq!(
        product_overflow.solve_with_probes(&probe_charts(), options(0, 1), None, true),
        Err(LightmapFailure::FillNonFinite)
    );
    let emitters = vec![point([3.0, 2.0, 3.0], f32::MAX); 32];
    let overflowing =
        TransportScene::new(Vec::new(), emitters).expect("individually finite emitters");
    assert_eq!(
        overflowing.solve_with_probes(&probe_charts(), options(1, 1), None, true),
        Err(LightmapFailure::FillNonFinite)
    );
}

#[test]
fn switchable_atlas_layers_do_not_duplicate_the_base_sky() {
    let mut switchable = point([3.0, 2.0, 3.0], 0.0);
    switchable.switchable = Some(0);
    let scene = TransportScene::new(Vec::new(), vec![switchable])
        .expect("scene")
        .with_sky([0.5; 3]);
    let solve = bake(&scene, 2, 1);
    assert!(
        solve.solution.charts[0]
            .texels
            .iter()
            .any(|texel| texel.irradiance[0] > 0.0)
    );
    assert!(
        solve.solution.switchable[0]
            .1
            .iter()
            .flat_map(|chart| &chart.texels)
            .all(|texel| texel.irradiance == [0.0; 3]),
        "a zero-intensity switchable layer must not contain sky light"
    );
}

#[test]
fn neutral_probe_diffuse_illumination_is_comparable_to_nearby_static_surface() {
    let scene = TransportScene::new(
        floor(0.0, 0.0, 6.0, 6.0, [0.6; 3]),
        vec![point([3.0, 2.5, 3.0], 2.0)],
    )
    .expect("scene");
    let solve = bake(&scene, 0, 1);
    let field = solve.probes.expect("field");
    let probe = field.probe(1, 0, 1).expect("air probe");
    let near = solve.solution.charts[0]
        .receivers
        .iter()
        .zip(&solve.solution.charts[0].texels)
        .min_by(|(a, _), (b, _)| {
            let distance = |receiver: &TransportReceiver| {
                (receiver.position[0] - 2.25).powi(2) + (receiver.position[2] - 2.25).powi(2)
            };
            distance(a).total_cmp(&distance(b))
        })
        .expect("nearby floor receiver");
    let surface = near.1.light_at([0.0, 1.0, 0.0])[0];
    let entity = probe.texel().light_at([0.0, 1.0, 0.0])[0];
    assert!(surface > 0.1 && entity > 0.1);
    assert!(
        (0.5..=2.0).contains(&(entity / surface)),
        "neutral normal consistency: probe {entity}, static {surface}"
    );
}
