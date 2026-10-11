//! Production-coordinate controls for model-lighting investigation.

use super::*;

fn corridor_charts() -> Vec<(LightmapPatch, Chart)> {
    // Untouched Demo Full charts 1006, 1285 and 1287. Keep their f32 geometry
    // and endpoint grids: a rounded, origin-centred substitute hides the bug.
    [
        (
            PatchKind::Ceiling,
            [60.3, 1.6, -12.3],
            [1.400_001_5, 0.0, 0.0],
            [0.0, 0.0, 15.15],
            24,
            244,
        ),
        (
            PatchKind::Wall,
            [60.3, 1.6, -13.7],
            [0.0, 0.0, 16.55],
            [0.0, -2.5, 0.0],
            266,
            41,
        ),
        (
            PatchKind::Wall,
            [61.7, 1.6, 2.849_999_4],
            [0.0, 0.0, -15.15],
            [0.0, -2.5, 0.0],
            244,
            41,
        ),
    ]
    .into_iter()
    .map(|(kind, origin, u_axis, v_axis, width, height)| {
        (
            LightmapPatch {
                origin,
                u_axis,
                v_axis,
                diagonal_correction: [0.0; 3],
                triangle: false,
                room: None,
                kind,
            },
            Chart {
                page: 0,
                x: 0,
                y: 0,
                width,
                height,
            },
        )
    })
    .collect()
}

fn corridor_scene(sealed: bool) -> TransportScene {
    let mut triangles = Vec::new();
    for (patch, _) in corridor_charts() {
        let [a, b, c, d] = [
            patch.point_at(0.0, 0.0),
            patch.point_at(1.0, 0.0),
            patch.point_at(1.0, 1.0),
            patch.point_at(0.0, 1.0),
        ];
        triangles.extend(wall(a, b, c, d, [0.6; 3]));
    }
    if sealed {
        triangles.extend(wall(
            [60.3, -0.9, -6.0],
            [61.7, -0.9, -6.0],
            [61.7, 1.6, -6.0],
            [60.3, 1.6, -6.0],
            [0.6; 3],
        ));
    }
    TransportScene::new(triangles, vec![point([61.0, 1.59, -5.0], 0.1)])
        .expect("production-scale corridor")
}

#[test]
fn production_corner_witness_exposes_unsafe_support_ray() {
    let triangle = TransportTriangle::new(
        [60.3, 1.6, -13.7],
        [60.3, 1.6, 2.849_999_4],
        [60.3, -0.9, 2.849_999_4],
        [0.6; 3],
    )
    .expect("actual west wall triangle");
    let position = [60.3, 1.599_971_3, -12.237_655];
    let direction = [0.096_267_23, -0.001_371_292_4, 0.995_354_65];
    assert_eq!(
        position[0], triangle.p0[0],
        "witness is on the exact wall plane"
    );
    assert!(direction[0] > 0.0, "ray departs the wall into room air");
    // The supporting-plane intersection is exactly zero. The projected
    // barycentric sum leaves a positive cancellation residual in f64, which
    // the zero-dead-zone kernel correctly cannot distinguish from real t>0.
    let unsafe_hit = ray_triangle(position, direction, &triangle).expect("edge witness");
    assert!(
        unsafe_hit > 0.0 && unsafe_hit < 1.0e-12,
        "a departing zero-distance plane hit acquired cancellation residual {unsafe_hit}"
    );
    let safe = [60.300_056, position[1], position[2]];
    let safe_hit = ray_triangle(safe, direction, &triangle).expect("supporting plane behind ray");
    assert!(
        safe_hit < 0.0,
        "safe-origin supporting plane is behind the ray"
    );
}

#[test]
fn production_corridor_boundary_fill_matches_clear_air_support() {
    let charts = corridor_charts();
    let geometry_scene = corridor_scene(false);
    let receivers = geometry_scene
        .receivers(&charts)
        .expect("corridor receivers");
    let scene = geometry_scene.with_receiver_target(vec![[0.4; 3]; receivers.len()]);
    let filled = scene
        .apply_chart_fill(
            &charts,
            &receivers,
            vec![LightmapTexel::ZERO; receivers.len()],
            1,
            None,
        )
        .expect("boundary recovery fill");
    let mut first = 0;
    for (chart_index, (_, chart)) in charts.iter().enumerate() {
        let width = usize::try_from(chart.width).expect("width");
        let height = usize::try_from(chart.height).expect("height");
        let count = if chart_index == 0 { height } else { width };
        let mut previous: Option<f32> = None;
        for edge_index in 1..count.saturating_sub(1) {
            let index = first
                + if chart_index == 0 {
                    edge_index * width
                } else {
                    edge_index
                };
            let receiver = receivers[index];
            // Every source ray leaves both touching faces into air. The
            // independent oracle is the authored falloff with visibility=1.
            let emitter = &scene.emitters[0];
            let expected = 0.4
                * emitter
                    .falloff
                    .factor(length(sub(emitter.position, receiver.position)) / emitter.range);
            let actual = filled[index].light_at(receiver.normal)[0];
            assert!(
                (actual - expected).abs() < 1.0e-5,
                "chart {chart_index} edge {edge_index}: recovery {actual}, clear-air {expected}; position {:?}, safe origin {:?}",
                receiver.position,
                receiver.ray_origin,
            );
            if let Some(last) = previous {
                assert!(
                    (actual - last).abs() < 0.01,
                    "smooth source support developed a boundary spike: {last} -> {actual}"
                );
            }
            previous = Some(actual);
        }
        first += width * height;
    }
}

#[test]
fn production_corridor_boundary_fill_preserves_a_sealed_blocker() {
    let charts = corridor_charts();
    let geometry_scene = corridor_scene(true);
    let receivers = geometry_scene
        .receivers(&charts)
        .expect("sealed corridor receivers");
    let scene = geometry_scene.with_receiver_target(vec![[0.4; 3]; receivers.len()]);
    let filled = scene
        .apply_chart_fill(
            &charts,
            &receivers,
            vec![LightmapTexel::ZERO; receivers.len()],
            1,
            None,
        )
        .expect("sealed corridor fill");
    let width = usize::try_from(charts[0].1.width).expect("width");
    let height = usize::try_from(charts[0].1.height).expect("height");
    let mut blocked = 0_usize;
    let mut clear = 0_usize;
    for row in 1..height.saturating_sub(1) {
        let index = row * width;
        let receiver = receivers[index];
        let light = filled[index].light_at(receiver.normal)[0];
        if receiver.position[2] < -6.1 {
            assert_eq!(light, 0.0, "recovery fill crossed the sealed divider");
            blocked += 1;
        } else if receiver.position[2] > -5.9 {
            let emitter = &scene.emitters[0];
            let expected = 0.4
                * emitter
                    .falloff
                    .factor(length(sub(emitter.position, receiver.position)) / emitter.range);
            assert!(
                (light - expected).abs() < 1.0e-5,
                "clear side lost recovery fill: {light} versus {expected}"
            );
            clear += 1;
        }
    }
    assert!(
        blocked > 20 && clear > 20,
        "both sides must have many control samples"
    );
}

#[test]
fn visible_footprint_interior_is_detected_when_centre_and_corners_are_blocked() {
    // The actual fridge corner has identical black centre/corner signatures,
    // but its [1,3] 4x4 sample sees the ceiling lights. This independent
    // geometric oracle isolates that trigger failure with a small real opening.
    let [a, b, c] = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0]];
    let patch =
        LightmapPatch::from_quad(PatchKind::Prop, [a, b, c, c], None).expect("triangular receiver");
    let chart = Chart {
        page: 0,
        x: 0,
        y: 0,
        width: 9,
        height: 13,
    };
    let mut triangles = vec![TransportTriangle::new(a, b, c, [0.6; 3]).expect("receiver triangle")];
    // Four opaque rectangles leave a 15x18mm slot at the one interior
    // footprint sample. All four corner samples and the centre miss the slot.
    for [x0, y0, x1, y1] in [
        [-1.0, -1.0, 2.0, 0.006],
        [-1.0, 0.024, 2.0, 2.0],
        [-1.0, 0.006, 0.039, 0.024],
        [0.054, 0.006, 2.0, 0.024],
    ] {
        triangles.extend(wall(
            [x0, y0, 0.4],
            [x1, y0, 0.4],
            [x1, y1, 0.4],
            [x0, y1, 0.4],
            [0.6; 3],
        ));
    }
    let scene = TransportScene::new(triangles, Vec::new())
        .expect("real slot scene")
        .with_global_lights(vec![crate::lighting::directional::DirectionalLight {
            incoming: [0.0, 0.0, 1.0],
            color: [1.0; 3],
            intensity: 1.0,
            cast_shadows: true,
            angular_radius: 0.0,
        }])
        .with_chart_sample_density(vec![8.0])
        .expect("actual model pitch");
    let solved = scene
        .solve(&[(patch, chart)], options(0, 3), None)
        .expect("slot coverage solve");
    let light = solved.charts[0].texels[0].light_at([0.0, 0.0, 1.0])[0];
    assert!(
        (light - 1.0 / 16.0).abs() < 1.0e-6,
        "one of the 16 physical samples sees the real slot: {light} versus 1/16"
    );
}

#[test]
fn authored_material_identity_crosses_texture_variation_but_keeps_real_boundaries() {
    for (identities, gap, other_albedo, transmissive, other_normal, crosses) in [
        ([1, 1, 1, 1], 0.0, [0.9; 3], false, [0.0, 1.0, 0.0], true),
        ([1, 1, 2, 2], 0.0, [0.6; 3], false, [0.0, 1.0, 0.0], false),
        ([1, 1, 1, 1], 0.02, [0.6; 3], false, [0.0, 1.0, 0.0], false),
        ([1, 1, 1, 1], 0.0, [0.6; 3], true, [0.0, 1.0, 0.0], false),
        ([1, 1, 1, 1], 0.0, [0.6; 3], false, [0.6, 0.8, 0.0], false),
        ([0, 0, 1, 1], 0.0, [0.6; 3], false, [0.0, 1.0, 0.0], false),
    ] {
        let mut geometry = floor(0.0, 0.0, 1.0, 1.0, [0.6; 3]);
        geometry.extend(
            floor(1.0 + gap, 0.0, 2.0, 1.0, other_albedo)
                .into_iter()
                .map(|triangle| {
                    triangle
                        .with_transmissive(transmissive)
                        .with_shading_normals([other_normal; 3])
                }),
        );
        let scene = TransportScene::new(geometry, Vec::new())
            .expect("material boundary scene")
            .with_surface_materials(identities.to_vec())
            .expect("aligned material identities");
        let point = [0.9, 0.0, 0.5];
        let (albedo, surface) = scene.surface_sample(point, 1.0e-5).expect("owning surface");
        let receiver = TransportReceiver {
            position: receiver_position(point, [0.0, 1.0, 0.0]),
            ray_origin: receiver_position(point, [0.0, 1.0, 0.0]),
            normal: [0.0, 1.0, 0.0],
            albedo,
            surface: u32::try_from(surface).expect("surface ID"),
            area: 1.0,
            attenuation: [1.0; 3],
        };
        let target = [1.1, 0.0, 0.5];
        let supported =
            coverage::supported_point(&scene, &receiver, point, target, &mut Vec::new());
        if crosses {
            assert!(
                length(sub(supported, target)) < 1.0e-6,
                "authored texture variation lost contiguous support: {supported:?}"
            );
        } else {
            assert!(
                (supported[0] - 1.0).abs() < 1.0e-4,
                "tap crossed a real material, gap, transmissive or normal boundary: {supported:?}"
            );
        }
    }
}

#[test]
fn diffuse_filter_uses_authored_material_identity_without_crossing_distinct_materials() {
    let charts = [
        floor_patch(0.0, 0.0, 1.0, 1.0, 3, 3),
        floor_patch(1.0, 0.0, 2.0, 1.0, 3, 3),
    ];
    for (identities, other_albedo, crosses) in [
        (vec![1, 1, 1, 1], [0.9; 3], true),
        (vec![1, 1, 2, 2], [0.6; 3], false),
        (Vec::new(), [0.9; 3], false),
    ] {
        let mut geometry = floor(0.0, 0.0, 1.0, 1.0, [0.6; 3]);
        geometry.extend(floor(1.0, 0.0, 2.0, 1.0, other_albedo));
        let geometry_scene =
            TransportScene::new(geometry, Vec::new()).expect("diffuse boundary scene");
        let scene = if identities.is_empty() {
            geometry_scene
        } else {
            geometry_scene
                .with_surface_materials(identities)
                .expect("aligned material identities")
        };
        let receivers = scene
            .receivers(&charts)
            .expect("diffuse boundary receivers");
        let values = (0_usize..18_usize)
            .map(|index| {
                let energy = if index < 9_usize { 0.0 } else { 1.0 };
                Accumulator {
                    irradiance: [energy; 3],
                    surface_light: [energy; 3],
                    ..Accumulator::default()
                }
            })
            .collect::<Vec<_>>();
        let filtered = filter::filter_accumulators(
            &scene,
            &charts,
            &receivers,
            &values,
            &vec![Accumulator::default(); 18],
        )
        .expect("diffuse material-aware filter");
        let edge_light = filtered[5].light_at([0.0, 1.0, 0.0])[0];
        if crosses {
            assert!(
                edge_light > 0.0,
                "incident diffuse light did not cross authored texture variation"
            );
        } else {
            assert_eq!(
                edge_light, 0.0,
                "diffuse light crossed a real or untagged material boundary"
            );
        }
    }
}
