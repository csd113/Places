//! Analytical controls for chart segmentation and receiver reconstruction.
// The numerical oracle uses bounded chart indices and float-to-index conversion.
#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
use super::*;

fn moon() -> crate::lighting::directional::DirectionalLight {
    crate::lighting::directional::DirectionalLight {
        incoming: [0.36, 0.8, 0.48],
        color: [0.85, 0.9, 1.0],
        intensity: 0.12,
        cast_shadows: true,
        angular_radius: 0.0,
    }
}

#[test]
fn coplanar_triangle_diagonal_does_not_select_a_different_bounce_field() {
    let scene =
        TransportScene::new(floor(0.0, 0.0, 2.0, 2.0, [0.6; 3]), Vec::new()).expect("floor");
    let receivers = scene
        .receivers(&[floor_patch(0.0, 0.0, 2.0, 2.0, 33, 33)])
        .expect("receivers");
    let values = receivers
        .iter()
        .map(|receiver| Accumulator {
            surface_light: [0.1 + 0.1 * receiver.position[0]; 3],
            ..Accumulator::default()
        })
        .collect::<Vec<_>>();
    let cache = RadianceCache::build(&receivers);
    for step in 1..20 {
        let at = step as f32 * 0.1;
        let point = [at, 0.0, 2.0 - at];
        let first = cache
            .sample_surface(&scene, point, 0, &receivers, &values)
            .surface_light[0];
        let second = cache
            .sample_surface(&scene, point, 1, &receivers, &values)
            .surface_light[0];
        assert!(
            (first - second).abs() < 1.0e-5,
            "quad diagonal changed the bounce field by {} at {point:?}",
            (first - second).abs()
        );
    }
}

#[test]
fn adaptive_coverage_reduces_diagonal_shadow_error_without_softening_interiors() {
    let mut triangles = floor(0.0, 0.0, 2.0, 2.0, [0.6; 3]);
    triangles.extend(wall(
        [0.0, 0.0, 0.0],
        [2.0, 0.0, 2.0],
        [2.0, 1.0, 2.0],
        [0.0, 1.0, 0.0],
        [0.6; 3],
    ));
    let n = std::f32::consts::FRAC_1_SQRT_2;
    let source = crate::lighting::directional::DirectionalLight {
        incoming: [-n, n, 0.0],
        color: [1.0; 3],
        intensity: 1.0,
        cast_shadows: true,
        angular_radius: 0.0,
    };
    let scene = TransportScene::new(triangles, Vec::new())
        .expect("caster")
        .with_global_lights(vec![source]);
    let charts = [floor_patch(0.0, 0.0, 2.0, 2.0, 17, 17)];
    let hard = scene.solve(&charts, options(0, 1), None).expect("hard");
    let medium = scene.solve(&charts, options(0, 2), None).expect("medium");
    let high = scene.solve(&charts, options(0, 3), None).expect("high");
    let mut errors = [0.0_f32; 3];
    let mut edge_texels = 0;
    for row in 1..16 {
        for column in 1..16 {
            let x = column as f32 / 8.0;
            let z = 2.0 - row as f32 / 8.0;
            // Independent 64×64 area-coverage oracle for 0 < x-z < 1.
            let mut visible = 0_u16;
            for sy in 0..64 {
                for sx in 0..64 {
                    let px = x + ((sx as f32 + 0.5) / 64.0 - 0.5) / 8.0;
                    let pz = z + ((sy as f32 + 0.5) / 64.0 - 0.5) / 8.0;
                    if px - pz <= 0.0 || px - pz >= 1.0 {
                        visible += 1;
                    }
                }
            }
            let expected = f32::from(visible) / 4096.0 * n;
            let index = row * 17 + column;
            for (error, result) in errors.iter_mut().zip([&hard, &medium, &high]) {
                let actual = result.charts[0].texels[index].light_at([0.0, 1.0, 0.0])[0];
                *error += (actual - expected).abs();
                if visible == 0 {
                    assert!(actual < 1.0e-6, "shadow interior was blurred");
                }
            }
            if visible > 0 && visible < 4096 {
                edge_texels += 1;
            }
        }
    }
    crate::logging::info(format_args!(
        "[coverage-control] edge_texels={edge_texels} absolute_error={errors:?} direct_rays={:?}",
        [hard.direct_rays, medium.direct_rays, high.direct_rays]
    ));
    assert!(edge_texels > 10);
    assert!(
        errors[1] < errors[0] * 0.6,
        "medium coverage error: {errors:?}"
    );
    assert!(
        errors[2] < errors[1],
        "high must improve coverage: {errors:?}"
    );
    assert!(medium.direct_rays > hard.direct_rays && high.direct_rays > medium.direct_rays);
}

#[test]
fn directional_source_has_constant_direction_and_no_distance_falloff() {
    let source = moon();
    let scene = TransportScene::new(Vec::new(), Vec::new())
        .expect("scene")
        .with_global_lights(vec![source]);
    let charts = [floor_patch(-200.0, -150.0, 200.0, 150.0, 17, 17)];
    let first = scene.solve(&charts, options(0, 3), None).expect("solve");
    let second = scene
        .solve(&charts, options(0, 3), None)
        .expect("repeat solve");
    assert_eq!(first, second);
    let expected = source
        .color
        .map(|value| value * source.intensity * source.incoming[1]);
    for texel in &first.charts[0].texels {
        for (actual, expected) in texel.light_at([0.0, 1.0, 0.0]).into_iter().zip(expected) {
            assert!((actual - expected).abs() < 1.0e-6);
        }
    }
}

#[test]
fn closed_corners_and_a_ceiling_block_directional_light_at_multiple_scales() {
    for size in [0.01_f32, 1.0, 1000.0] {
        let mut triangles = floor(0.0, 0.0, size, size, [0.6; 3]);
        for (a, b) in [
            ([0.0, 0.0, 0.0], [size, 0.0, 0.0]),
            ([size, 0.0, 0.0], [size, 0.0, size]),
            ([size, 0.0, size], [0.0, 0.0, size]),
            ([0.0, 0.0, size], [0.0, 0.0, 0.0]),
        ] {
            triangles.extend(wall(a, b, [b[0], size, b[2]], [a[0], size, a[2]], [0.6; 3]));
        }
        triangles.extend(wall(
            [0.0, size, 0.0],
            [0.0, size, size],
            [size, size, size],
            [size, size, 0.0],
            [0.6; 3],
        ));
        let scene = TransportScene::new(triangles, Vec::new())
            .expect("closed box")
            .with_global_lights(vec![moon()]);
        let result = scene
            .solve(
                &[floor_patch(0.0, 0.0, size, size, 17, 17)],
                options(0, 3),
                None,
            )
            .expect("solve");
        assert!(
            result.charts[0].texels.iter().all(|texel| texel
                .light_at([0.0, 1.0, 0.0])
                .iter()
                .all(|value| *value < 1.0e-7)),
            "closed box leaked at scale {size}"
        );
    }
}

#[test]
fn a_ceiling_shields_the_interior_from_pitched_roof_triangles() {
    let ceiling = wall(
        [-2.0, 2.0, -2.0],
        [-2.0, 2.0, 2.0],
        [2.0, 2.0, 2.0],
        [2.0, 2.0, -2.0],
        [0.7; 3],
    );
    let without_roof = TransportScene::new(ceiling.clone(), vec![point([0.0, 1.8, 0.0], 1.0)])
        .expect("ceiling scene");
    let mut roof = ceiling;
    roof.extend(wall(
        [-2.0, 2.1, -2.0],
        [2.0, 2.1, -2.0],
        [2.0, 3.0, 0.0],
        [-2.0, 3.0, 0.0],
        [0.4; 3],
    ));
    roof.extend(wall(
        [-2.0, 3.0, 0.0],
        [2.0, 3.0, 0.0],
        [2.0, 2.1, 2.0],
        [-2.0, 2.1, 2.0],
        [0.4; 3],
    ));
    let with_roof = TransportScene::new(roof, without_roof.emitters.clone()).expect("roof scene");
    let charts = [floor_patch(-1.8, -1.8, 1.8, 1.8, 33, 33)];
    assert_eq!(
        without_roof
            .solve(&charts, options(1, 3), None)
            .expect("without"),
        with_roof.solve(&charts, options(1, 3), None).expect("with")
    );
    for direction in [[0.0, 1.0, 0.0], [0.36, 0.8, 0.48]] {
        let (_, caster) = with_roof
            .intersect([0.0, 0.5, 0.0], direction)
            .expect("ceiling hit");
        assert!(
            caster < 2,
            "interior ray reached roof caster {caster} through ceiling"
        );
    }
}

#[test]
fn shared_model_edges_keep_the_receiver_faces_normals_and_albedo() {
    let face = TransportTriangle::new([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.4; 3])
        .expect("floor");
    let adjacent =
        TransportTriangle::new([0.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0], [0.9; 3])
            .expect("side");
    let patch =
        LightmapPatch::from_quad(PatchKind::Prop, [face.p0, face.p1, face.p2, face.p2], None)
            .expect("patch");
    let scene = TransportScene::new(vec![adjacent, face], Vec::new()).expect("scene");
    let receivers = scene
        .receivers(&[(
            patch,
            Chart {
                page: 0,
                x: 0,
                y: 0,
                width: 9,
                height: 9,
            },
        )])
        .expect("receivers");
    assert!(receivers.iter().all(|receiver| receiver.surface == 1
        && receiver.albedo == [0.4; 3]
        && receiver.normal == [0.0, 1.0, 0.0]));
}

fn sample_chart(chart: &SolvedChart, width: usize, height: usize, u: f32, v: f32) -> f32 {
    let x = u * (width - 1) as f32;
    let y = v * (height - 1) as f32;
    let ix = x.floor() as usize;
    let iy = y.floor() as usize;
    let mut value = LightmapTexel::ZERO;
    for (dx, dy, weight) in [
        (0, 0, (1.0 - x.fract()) * (1.0 - y.fract())),
        (1, 0, x.fract() * (1.0 - y.fract())),
        (0, 1, (1.0 - x.fract()) * y.fract()),
        (1, 1, x.fract() * y.fract()),
    ] {
        let texel = chart.texels[(iy + dy).min(height - 1) * width + (ix + dx).min(width - 1)];
        for channel in 0..3 {
            value.irradiance[channel] += texel.irradiance[channel] * weight;
            value.direction[channel] += texel.direction[channel] * weight;
        }
    }
    value.light_at([0.0, 1.0, 0.0])[0]
}

#[test]
fn direct_light_is_invariant_at_a_coplanar_chart_seam() {
    let scene = TransportScene::new(Vec::new(), vec![point([2.0, 0.5, 0.5], 1.0)])
        .expect("valid analytical scene");
    let whole = scene
        .solve(
            &[floor_patch(0.0, 0.0, 8.0, 1.0, 129, 17)],
            options(0, 1),
            None,
        )
        .expect("whole solve");
    let strips = (0..4)
        .map(|strip| {
            floor_patch(
                2.0 * strip as f32,
                0.0,
                2.0 * (strip + 1) as f32,
                1.0,
                33,
                17,
            )
        })
        .collect::<Vec<_>>();
    let split = scene
        .solve(&strips, options(0, 1), None)
        .expect("split solve");
    let mut max_error = 0.0_f32;
    for step in 0..=800 {
        let x = step as f32 * 0.01;
        let strip = (x / 2.0).floor().min(3.0) as usize;
        let a = sample_chart(&whole.charts[0], 129, 17, x / 8.0, 0.5);
        let b = sample_chart(
            &split.charts[strip],
            33,
            17,
            (x - strip as f32 * 2.0) / 2.0,
            0.5,
        );
        max_error = max_error.max((a - b).abs());
    }
    assert!(
        max_error < 1.0e-4,
        "chart segmentation altered direct light by {max_error}"
    );
}

#[test]
fn direct_shadow_coverage_does_not_depend_on_a_chart_boundary() {
    // A diagonal opaque sheet intersects the fixture-to-floor rays at x=2.
    let blocker = wall(
        [1.6, 0.0, -1.0],
        [2.4, 0.0, 2.0],
        [2.4, 0.3, 2.0],
        [1.6, 0.3, -1.0],
        [0.5; 3],
    );
    let scene =
        TransportScene::new(blocker, vec![point([0.0, 1.0, 0.5], 1.0)]).expect("shadow scene");
    let charts = [floor_patch(0.0, 0.0, 4.0, 1.0, 65, 17)];
    let solved = scene.solve(&charts, options(0, 1), None).expect("solve");
    let receivers = scene.receivers(&charts).expect("receivers");
    // A hard point source's visibility is binary at each point. Reconstruction
    // must not manufacture light on the shadowed side before footprint coverage
    // is deliberately evaluated by the bake's visibility sampling.
    for (receiver, texel) in receivers.iter().zip(&solved.charts[0].texels) {
        let (weight, direction) =
            scene.emitters[0].direct_from(&scene, receiver.position, receiver.ray_origin, 1);
        let expected = weight[0] * dot(direction, receiver.normal).max(0.0);
        assert!(
            (texel.light_at(receiver.normal)[0] - expected).abs() < 1.0e-5,
            "direct reconstruction smeared a shadow edge at {:?}",
            receiver.position
        );
    }
}

#[test]
fn smooth_model_normals_drive_integration_without_moving_visibility_origins() {
    let normal = [0.6, 0.8, 0.0];
    let corners = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, -1.0]];
    let triangle = TransportTriangle::new(corners[0], corners[1], corners[2], [0.5; 3])
        .expect("triangle")
        .with_shading_normals([normal; 3]);
    let patch = LightmapPatch::from_quad(
        PatchKind::Prop,
        [corners[0], corners[1], corners[2], corners[2]],
        None,
    )
    .expect("patch");
    let chart = Chart {
        page: 0,
        x: 0,
        y: 0,
        width: 9,
        height: 9,
    };
    let emitter = point([3.0, 2.0, -0.5], 1.0);
    let scene = TransportScene::new(vec![triangle], vec![emitter]).expect("scene");
    let result = scene
        .solve(&[(patch, chart)], options(0, 1), None)
        .expect("solve");
    for (receiver, texel) in result.charts[0]
        .receivers
        .iter()
        .zip(&result.charts[0].texels)
    {
        assert!(length(sub(receiver.normal, normal)) < 1.0e-6);
        assert_eq!(receiver.position[1], SURFACE_OFFSET_M);
        let (weight, direction) =
            emitter.direct_from(&scene, receiver.position, receiver.ray_origin, 1);
        let expected = weight[0] * dot(direction, normal).max(0.0);
        assert!((texel.light_at(normal)[0] - expected).abs() < 1.0e-5);
    }
}

#[test]
fn medium_and_high_bounce_fields_remain_continuous_across_subdivisions() {
    let mut triangles = floor(0.0, 0.0, 8.0, 1.0, [0.6; 3]);
    let corners = [
        [0.0, 0.0, -1.0],
        [8.0, 0.0, -1.0],
        [8.0, 2.0, -1.0],
        [0.0, 2.0, -1.0],
    ];
    triangles.extend(wall(
        corners[0], corners[1], corners[2], corners[3], [0.6; 3],
    ));
    let wall_patch = LightmapPatch::from_quad(PatchKind::Wall, corners, None).expect("wall");
    let wall_chart = Chart {
        page: 0,
        x: 0,
        y: 0,
        width: 129,
        height: 33,
    };
    let scene = TransportScene::new(triangles, vec![point([2.0, 0.5, 0.5], 1.0)]).expect("scene");
    let whole = [
        floor_patch(0.0, 0.0, 8.0, 1.0, 129, 17),
        (wall_patch, wall_chart),
    ];
    let mut strips = (0..4)
        .map(|strip| {
            floor_patch(
                2.0 * strip as f32,
                0.0,
                2.0 * (strip + 1) as f32,
                1.0,
                33,
                17,
            )
        })
        .collect::<Vec<_>>();
    strips.push((wall_patch, wall_chart));
    for taps in [2, 3] {
        for bounces in [0, 2] {
            let a = scene
                .solve(&whole, options(bounces, taps), None)
                .expect("whole");
            let b = scene
                .solve(&strips, options(bounces, taps), None)
                .expect("strips");
            let mut max_error = 0.0_f32;
            for step in 1..800 {
                let x = step as f32 * 0.01;
                let strip = (x / 2.0).floor().min(3.0) as usize;
                let left = sample_chart(&a.charts[0], 129, 17, x / 8.0, 0.5);
                let right = sample_chart(
                    &b.charts[strip],
                    33,
                    17,
                    (x - strip as f32 * 2.0) / 2.0,
                    0.5,
                );
                max_error = max_error.max((left - right).abs());
            }
            assert!(
                max_error < 2.0e-4,
                "taps={taps} bounces={bounces} subdivision error={max_error}"
            );
        }
    }
}

fn vertical(point: [f32; 3]) -> [f32; 3] {
    [point[0], point[2], -point[1]]
}

fn vertical_patch(mut patch: LightmapPatch) -> LightmapPatch {
    patch.origin = vertical(patch.origin);
    patch.u_axis = vertical(patch.u_axis);
    patch.v_axis = vertical(patch.v_axis);
    patch.diagonal_correction = vertical(patch.diagonal_correction);
    patch
}

fn sample_wall(chart: &SolvedChart, width: usize, height: usize, u: f32, v: f32) -> f32 {
    let mut chart = chart.clone();
    for value in &mut chart.texels {
        value.direction = [value.direction[0], -value.direction[2], value.direction[1]];
    }
    sample_chart(&chart, width, height, u, v)
}

#[test]
fn vertical_and_horizontal_wall_strips_have_the_same_lighting_as_one_wall() {
    let triangles = floor(0.0, 0.0, 8.0, 3.0, [0.6; 3])
        .iter()
        .map(|face| {
            TransportTriangle::new(
                vertical(face.p0),
                vertical(face.p1),
                vertical(face.p2),
                face.albedo,
            )
            .expect("wall face")
        })
        .collect();
    let scene =
        TransportScene::new(triangles, vec![point([2.0, 1.5, -0.5], 1.0)]).expect("wall scene");
    let transform = |(patch, chart)| (vertical_patch(patch), chart);
    let whole = [transform(floor_patch(0.0, 0.0, 8.0, 3.0, 129, 49))];
    let vertical = (0..4)
        .map(|i| {
            transform(floor_patch(
                i as f32 * 2.0,
                0.0,
                (i + 1) as f32 * 2.0,
                3.0,
                33,
                49,
            ))
        })
        .collect::<Vec<_>>();
    let horizontal = (0..3)
        .map(|i| transform(floor_patch(0.0, i as f32, 8.0, (i + 1) as f32, 129, 17)))
        .collect::<Vec<_>>();
    for taps in [2, 3] {
        let a = scene.solve(&whole, options(2, taps), None).expect("whole");
        let b = scene
            .solve(&vertical, options(2, taps), None)
            .expect("vertical");
        let c = scene
            .solve(&horizontal, options(2, taps), None)
            .expect("horizontal");
        for (x, y) in [
            (2.0_f32, 1.5_f32),
            (4.0, 1.0),
            (6.0, 2.0),
            (3.9, 2.01),
            (4.01, 1.01),
        ] {
            let ix = (x / 2.0_f32).floor().min(3.0) as usize;
            let iy = y.floor().min(2.0) as usize;
            let whole = sample_wall(&a.charts[0], 129, 49, x / 8.0, 1.0 - y / 3.0);
            let vertical = sample_wall(
                &b.charts[ix],
                33,
                49,
                (x - ix as f32 * 2.0) / 2.0,
                1.0 - y / 3.0,
            );
            let horizontal = sample_wall(&c.charts[iy], 129, 17, x / 8.0, 1.0 - (y - iy as f32));
            assert!(
                (whole - vertical).abs() < 2.0e-4 && (whole - horizontal).abs() < 2.0e-4,
                "wall strips taps={taps}: {whole} {vertical} {horizontal}"
            );
        }
    }
}

fn opening_patch(rect: [f32; 4]) -> (LightmapPatch, Chart) {
    let [x, y, width, height] = rect;
    let (mut patch, chart) = floor_patch(
        x,
        y,
        x + width,
        y + height,
        (width * 16.0) as u32 + 1,
        (height * 16.0) as u32 + 1,
    );
    patch.kind = PatchKind::Wall;
    (vertical_patch(patch), chart)
}

fn sample_opening(solution: &TransportSolution, pieces: &[[f32; 4]], x: f32, y: f32) -> f32 {
    let (index, rect) = pieces
        .iter()
        .enumerate()
        .find(|(_, rect)| {
            x >= rect[0] && x <= rect[0] + rect[2] && y >= rect[1] && y <= rect[1] + rect[3]
        })
        .expect("point on wall");
    let chart = opening_patch(*rect).1;
    sample_wall(
        &solution.charts[index],
        chart.width as usize,
        chart.height as usize,
        (x - rect[0]) / rect[2],
        1.0 - (y - rect[1]) / rect[3],
    )
}

#[test]
fn rectangular_window_surrounds_keep_lighting_when_subdivided() {
    // Same physical 8x3 m wall and 2x1 m hole; only receiver segmentation varies.
    let surrounds = [
        [0.0, 0.0, 3.0, 3.0],
        [5.0, 0.0, 3.0, 3.0],
        [3.0, 0.0, 2.0, 1.0],
        [3.0, 2.0, 2.0, 1.0],
    ];
    let mut triangles = surrounds
        .iter()
        .flat_map(|rect| {
            let patch = opening_patch(*rect).0;
            wall(
                patch.point_at(0.0, 0.0),
                patch.point_at(1.0, 0.0),
                patch.point_at(1.0, 1.0),
                patch.point_at(0.0, 1.0),
                [0.6; 3],
            )
        })
        .collect::<Vec<_>>();
    triangles.extend(floor(0.0, -2.0, 8.0, 0.0, [0.7; 3]));
    let scene =
        TransportScene::new(triangles, vec![point([2.0, 1.5, -0.5], 1.0)]).expect("window wall");
    let tiles = (0..3)
        .flat_map(|y| {
            (0..8).filter_map(move |x| {
                (!(y == 1 && (3..5).contains(&x))).then_some([x as f32, y as f32, 1.0, 1.0])
            })
        })
        .collect::<Vec<_>>();
    let mut a_charts = surrounds.into_iter().map(opening_patch).collect::<Vec<_>>();
    let mut b_charts = tiles.iter().copied().map(opening_patch).collect::<Vec<_>>();
    let floor_chart = floor_patch(0.0, -2.0, 8.0, 0.0, 129, 33);
    a_charts.push(floor_chart);
    b_charts.push(floor_chart);
    for taps in [2, 3] {
        let a = scene
            .solve(&a_charts, options(2, taps), None)
            .expect("surrounds");
        let b = scene
            .solve(&b_charts, options(2, taps), None)
            .expect("tiles");
        for (x, y) in [
            (2.99, 0.5),
            (3.01, 0.5),
            (4.99, 2.5),
            (5.01, 2.5),
            (2.9, 1.01),
            (5.1, 1.99),
            (4.0, 0.99),
            (4.0, 2.01),
        ] {
            let left = sample_opening(&a, &surrounds, x, y);
            let right = sample_opening(&b, &tiles, x, y);
            assert!(
                (left - right).abs() < 2.0e-4,
                "window segmentation taps={taps} at {x},{y}: {left} {right}"
            );
        }
    }
}

#[test]
fn a_tilted_shading_normal_does_not_gather_light_from_its_own_backface() {
    let triangles = floor(0.0, 0.0, 2.0, 2.0, [0.6; 3])
        .into_iter()
        .map(|triangle| triangle.with_shading_normals([[0.6, 0.8, 0.0]; 3]))
        .collect();
    let scene = TransportScene::new(triangles, vec![point([1.0, 2.0, 1.0], 1.0)]).expect("plane");
    let (mut patch, chart) = floor_patch(0.0, 0.0, 2.0, 2.0, 17, 17);
    patch.kind = PatchKind::Prop;
    let charts = [(patch, chart)];
    let direct = scene.solve(&charts, options(0, 1), None).expect("direct");
    let bounced = scene.solve(&charts, options(1, 1), None).expect("bounce");
    for (direct, bounced) in direct.charts[0]
        .texels
        .iter()
        .zip(&bounced.charts[0].texels)
    {
        let direct = direct.light_at([0.6, 0.8, 0.0])[0];
        let bounced = bounced.light_at([0.6, 0.8, 0.0])[0];
        assert!(
            (direct - bounced).abs() < 1.0e-6,
            "self bounce added {}",
            bounced - direct
        );
    }
}

#[test]
fn an_opaque_backface_blocks_light_without_emitting_front_light() {
    let scene = TransportScene::new(
        floor(0.0, 0.0, 2.0, 2.0, [0.6; 3]),
        vec![point([1.0, 2.0, 1.0], 1.0)],
    )
    .expect("sheet");
    let receivers = scene
        .receivers(&[floor_patch(0.0, 0.0, 2.0, 2.0, 17, 17)])
        .expect("receivers");
    let values = receivers
        .iter()
        .map(|receiver| {
            let (weight, direction) =
                scene.emitters[0].direct_from(&scene, receiver.position, receiver.ray_origin, 1);
            let mut value = Accumulator::default();
            accumulate_surface_lobe(&mut value, weight, direction, receiver.normal);
            value
        })
        .collect::<Vec<_>>();
    let cache = RadianceCache::build(&receivers);
    let (probe, _, _) = bake_probe(
        &scene,
        &receivers,
        &values,
        &cache,
        [1.0, -1.0, 1.0],
        0,
        1,
        false,
    );
    assert!(
        probe.irradiance.iter().all(|value| *value < 1.0e-6),
        "front illumination crossed a sheet: {:?}",
        probe.irradiance
    );
}
