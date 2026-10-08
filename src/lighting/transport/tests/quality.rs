//! Analytical controls for chart segmentation and receiver reconstruction.
use super::*;
use std::sync::Arc;

use crate::materials::{AlphaMode, MaterialAlpha, RawImage};

fn alpha_plane(
    y: f32,
    extent: f32,
    alpha: MaterialAlpha,
    image: Option<&Arc<RawImage>>,
    address: TransportTextureAddress,
) -> (Vec<TransportTriangle>, Vec<Option<TransportAlphaSurface>>) {
    let mut triangles = floor(-extent, -extent, extent, extent, [0.7; 3]);
    let mut surfaces = Vec::new();
    for triangle in &mut triangles {
        triangle.p0[1] = y;
        triangle.p1[1] = y;
        triangle.p2[1] = y;
        surfaces.push(Some(TransportAlphaSurface {
            alpha,
            image: image.cloned(),
            uv: [triangle.p0, triangle.p1, triangle.p2].map(|point| {
                [
                    f32::midpoint(point[0] / extent, 1.0),
                    f32::midpoint(point[2] / extent, 1.0),
                ]
            }),
            vertex_alpha: [1.0; 3],
            address,
        }));
    }
    (triangles, surfaces)
}

fn mask() -> MaterialAlpha {
    MaterialAlpha {
        mode: AlphaMode::Cutout,
        opacity: 1.0,
        cutoff: 0.5,
    }
}

#[test]
fn cutout_transport_uses_numeric_texel_coverage_and_keeps_solid_pixels() {
    let image = Arc::new(RawImage::new(2, 1, vec![255, 0, 0, 0, 0, 255, 0, 255]));
    let (mut triangles, mut surfaces) = alpha_plane(
        1.0,
        1.0,
        mask(),
        Some(&image),
        TransportTextureAddress::Clamp,
    );
    let (behind, _) = alpha_plane(
        2.0,
        1.0,
        MaterialAlpha::OPAQUE,
        None,
        TransportTextureAddress::Clamp,
    );
    triangles.extend(behind);
    surfaces.extend([None, None]);
    let scene = TransportScene::new(triangles, Vec::new())
        .expect("scene")
        .with_surface_alpha(surfaces)
        .expect("coverage");
    let open = [-0.5, 0.0, 0.25];
    let solid = [0.5, 0.0, 0.25];
    assert!(
        !scene.occluded(open, [-0.5, 1.5, 0.25]),
        "a transparent MASK pixel leaves the segment open"
    );
    assert!(
        scene.occluded(solid, [0.5, 1.5, 0.25]),
        "a covered MASK pixel blocks the segment"
    );
    assert_eq!(
        scene
            .intersect(open, [0.0, 1.0, 0.0])
            .expect("background")
            .0
            .to_bits(),
        2.0_f32.to_bits(),
        "the transparent pixel reaches the opaque background"
    );
    assert_eq!(
        scene
            .intersect(solid, [0.0, 1.0, 0.0])
            .expect("card pixel")
            .0
            .to_bits(),
        1.0_f32.to_bits(),
        "the covered pixel is the nearest opaque hit"
    );
}

#[test]
fn blend_transport_multiplies_layers_and_counts_a_shared_diagonal_once() {
    let (mut triangles, mut surfaces) = alpha_plane(
        1.0,
        2.0,
        MaterialAlpha::blend(0.25),
        None,
        TransportTextureAddress::Clamp,
    );
    let (second, second_alpha) = alpha_plane(
        2.0,
        2.0,
        MaterialAlpha::blend(0.5),
        None,
        TransportTextureAddress::Clamp,
    );
    triangles.extend(second);
    surfaces.extend(second_alpha);
    let panes = TransportScene::new(triangles.clone(), Vec::new())
        .expect("panes")
        .with_surface_alpha(surfaces.clone())
        .expect("alpha");
    assert!(
        !panes.occluded([0.0; 3], [0.0, 3.0, 0.0]),
        "BLEND coverage never becomes a boolean cache barrier"
    );
    assert_eq!(
        panes.transmittance([0.0; 3], [0.0, 3.0, 0.0]).to_bits(),
        0.375_f32.to_bits(),
        "each physical pane attenuates once at its shared diagonal"
    );
    let (first_solid, escaped_throughput) = panes.intersect_transport([0.0; 3], [0.0, 1.0, 0.0]);
    assert!(
        first_solid.is_none(),
        "both panes transmit to the environment"
    );
    assert_eq!(
        escaped_throughput.to_bits(),
        0.375_f32.to_bits(),
        "infinite transport retains the same two-layer transmission"
    );
    let (solid, _) = alpha_plane(
        2.5,
        2.0,
        MaterialAlpha::OPAQUE,
        None,
        TransportTextureAddress::Clamp,
    );
    triangles.extend(solid);
    surfaces.extend([None, None]);
    let (beyond, beyond_alpha) = alpha_plane(
        2.75,
        2.0,
        MaterialAlpha::blend(1.0),
        None,
        TransportTextureAddress::Clamp,
    );
    triangles.extend(beyond);
    surfaces.extend(beyond_alpha);
    let blocked = TransportScene::new(triangles, Vec::new())
        .expect("solid")
        .with_surface_alpha(surfaces)
        .expect("alpha");
    assert_eq!(
        blocked.transmittance([0.0; 3], [0.0, 3.0, 0.0]).to_bits(),
        0.0_f32.to_bits(),
        "the opaque endpoint blocks finite direct transmission"
    );
    let (hit, throughput) = blocked.intersect_transport([0.0; 3], [0.0, 1.0, 0.0]);
    assert_eq!(
        hit.expect("solid").0.to_bits(),
        2.5_f32.to_bits(),
        "the opaque surface remains the nearest bounce hit"
    );
    assert_eq!(
        throughput.to_bits(),
        0.375_f32.to_bits(),
        "coverage beyond the first solid must not attenuate its bounce"
    );
}

#[test]
fn transport_alpha_matches_clamp_repeat_vertex_factors_and_cutoff_equality() {
    let image = Arc::new(RawImage::new(2, 1, vec![0, 0, 0, 0, 0, 0, 0, 255]));
    for (address, expected) in [
        (TransportTextureAddress::Clamp, true),
        (TransportTextureAddress::Repeat, false),
    ] {
        let (triangles, mut surfaces) = alpha_plane(1.0, 1.0, mask(), Some(&image), address);
        for surface in surfaces.iter_mut().flatten() {
            surface.uv = [[1.25, 0.5]; 3];
        }
        let scene = TransportScene::new(triangles, Vec::new())
            .expect("plane")
            .with_surface_alpha(surfaces)
            .expect("alpha");
        assert_eq!(
            scene.occluded([0.0; 3], [0.0, 2.0, 0.0]),
            expected,
            "transport coverage follows the authored {address:?} texture addressing"
        );
    }
    let triangle = TransportTriangle::new(
        [-1.0, 1.0, -1.0],
        [1.0, 1.0, -1.0],
        [-1.0, 1.0, 1.0],
        [0.7; 3],
    )
    .expect("triangle");
    let factor_surface = TransportAlphaSurface {
        alpha: MaterialAlpha::blend(0.5),
        image: None,
        uv: [[0.0; 2]; 3],
        vertex_alpha: [0.0, 1.0, 1.0],
        address: TransportTextureAddress::Clamp,
    };
    let factor_scene = TransportScene::new(vec![triangle], Vec::new())
        .expect("triangle")
        .with_surface_alpha(vec![Some(factor_surface)])
        .expect("alpha");
    assert_eq!(
        factor_scene
            .transmittance([-0.5, 0.0, -0.5], [-0.5, 2.0, -0.5])
            .to_bits(),
        0.75_f32.to_bits(),
        "interpolated vertex alpha multiplies authored material opacity"
    );
    let (triangles, mut surfaces) =
        alpha_plane(1.0, 1.0, mask(), None, TransportTextureAddress::Clamp);
    for surface in surfaces.iter_mut().flatten() {
        surface.vertex_alpha = [0.5; 3];
    }
    let cutoff_scene = TransportScene::new(triangles, Vec::new())
        .expect("plane")
        .with_surface_alpha(surfaces)
        .expect("alpha");
    assert!(
        cutoff_scene.occluded([0.0; 3], [0.0, 2.0, 0.0]),
        "alpha == cutoff is opaque"
    );
}

#[test]
fn invalid_alpha_tables_and_nonpositive_chart_densities_are_rejected() {
    assert!(
        TransportScene::new(Vec::new(), Vec::new())
            .expect("empty")
            .with_surface_alpha(vec![None])
            .is_none(),
        "alpha sidecars must align with the transport triangle table"
    );
    let (triangles, mut surfaces) =
        alpha_plane(1.0, 1.0, mask(), None, TransportTextureAddress::Clamp);
    surfaces[0].as_mut().expect("surface").uv[0][0] = f32::NAN;
    assert!(
        TransportScene::new(triangles, Vec::new())
            .expect("plane")
            .with_surface_alpha(surfaces)
            .is_none(),
        "non-finite authored UV coordinates must be rejected"
    );
    for density in [
        0.0,
        -1.0,
        f32::NAN,
        f32::INFINITY,
        f32::MIN_POSITIVE / 100.0,
    ] {
        assert!(
            TransportScene::new(Vec::new(), Vec::new())
                .expect("empty")
                .with_chart_sample_density(vec![density])
                .is_none(),
            "invalid physical chart density {density:?} must be rejected"
        );
    }
}

#[test]
fn finite_source_integrates_per_tap_cosines_instead_of_normalizing_their_mean() {
    let emitter = rect([0.0, 1.0, 0.0], 1.0, 1.0, 1.0);
    let strength =
        crate::lighting::LOCAL_LIGHT_STRENGTH * emitter.falloff.factor(1.0 / emitter.range);
    let scene = TransportScene::new(Vec::new(), vec![emitter]).expect("source");
    let receiver = TransportReceiver {
        position: [0.0; 3],
        ray_origin: [0.0; 3],
        normal: [0.0, 1.0, 0.0],
        albedo: [0.01; 3],
        area: 1.0,
        surface: u32::MAX,
        attenuation: [1.0; 3],
    };
    let sampled = scene.direct_sample(&receiver, &[0], false, 2);
    // Four authored corners are each sqrt(3) metres away and have cosine 1/sqrt(3).
    let expected = strength / 3.0_f32.sqrt();
    assert_eq!(
        sampled.rays, 4,
        "the existing two-by-two emitter budget is retained"
    );
    for actual in sampled.light.surface_light {
        assert!(
            (actual - expected).abs() < 1.0e-6,
            "receiver albedo must be excluded"
        );
    }
    let texel = compress_surface(&sampled.light, receiver.normal);
    assert!(
        surface_energy_matches(texel, receiver.normal, [expected; 3]),
        "the compact field preserves the analytical receiver cosine integral"
    );
    let summarized = emitter.direct(&scene, receiver.position, 2);
    assert!(
        (summarized.0[0] - strength).abs() < 1.0e-6,
        "legacy query reports scalar source support"
    );
}

#[test]
fn blend_attenuates_global_direct_and_escaping_sky_without_an_ambient_lift() {
    let (global_faces, global_alpha) = alpha_plane(
        1.0,
        1000.0,
        MaterialAlpha::blend(0.5),
        None,
        TransportTextureAddress::Clamp,
    );
    let moon = crate::lighting::directional::DirectionalLight {
        incoming: [0.0, 1.0, 0.0],
        color: [0.8, 0.9, 1.0],
        intensity: 2.0,
        cast_shadows: true,
        angular_radius: 0.0,
    };
    let scene = TransportScene::new(global_faces, Vec::new())
        .expect("pane")
        .with_surface_alpha(global_alpha)
        .expect("alpha")
        .with_global_lights(vec![moon]);
    let direct_rgb = scene
        .intensity_at([0.0; 3], [0.0, 1.0, 0.0], options(0, 3))
        .light_at([0.0, 1.0, 0.0]);
    for (channel, expected) in direct_rgb.into_iter().zip(moon.color) {
        assert!(
            (channel - expected).abs() < 1.0e-6,
            "half-opacity BLEND attenuates the authored global source exactly once"
        );
    }
    let charts = [floor_patch(-0.01, -0.01, 0.01, 0.01, 1, 1)];
    let open_sky = TransportScene::new(Vec::new(), Vec::new())
        .expect("clear")
        .with_sky([0.2; 3])
        .solve(&charts, options(1, 1), None)
        .expect("sky");
    let (sky_faces, sky_alpha) = alpha_plane(
        1.0,
        1000.0,
        MaterialAlpha::blend(0.5),
        None,
        TransportTextureAddress::Clamp,
    );
    let covered = TransportScene::new(sky_faces, Vec::new())
        .expect("pane")
        .with_surface_alpha(sky_alpha)
        .expect("alpha")
        .with_sky([0.2; 3])
        .solve(&charts, options(1, 1), None)
        .expect("covered sky");
    for (transmitted, before) in first_light(&covered, [0.0, 1.0, 0.0])
        .into_iter()
        .zip(first_light(&open_sky, [0.0, 1.0, 0.0]))
    {
        assert!(
            (transmitted - before * 0.5).abs() < 1.0e-6,
            "half-opacity BLEND attenuates escaping sky without adding ambient light"
        );
    }
}

#[test]
fn blend_attenuates_a_real_surface_bounce_without_changing_cache_connectivity() {
    let (ceiling, _) = alpha_plane(
        2.0,
        1000.0,
        MaterialAlpha::OPAQUE,
        None,
        TransportTextureAddress::Clamp,
    );
    let triangles = ceiling
        .iter()
        .map(|face| {
            TransportTriangle::new(face.p0, face.p2, face.p1, [0.8; 3]).expect("downward ceiling")
        })
        .collect::<Vec<_>>();
    let clear = TransportScene::new(triangles.clone(), Vec::new()).expect("ceiling");
    let mut receivers = vec![TransportReceiver {
        position: [0.0; 3],
        ray_origin: [0.0; 3],
        normal: [0.0, 1.0, 0.0],
        albedo: [0.1; 3],
        area: 1.0,
        surface: u32::MAX,
        attenuation: [1.0; 3],
    }];
    for (point, surface) in [
        ([0.0, 2.0, 0.0], 0),
        ([-1000.0, 2.0, -1000.0], 0),
        ([1000.0, 2.0, 1000.0], 1),
    ] {
        let position = receiver_position(point, [0.0, -1.0, 0.0]);
        receivers.push(TransportReceiver {
            position,
            ray_origin: position,
            normal: [0.0, -1.0, 0.0],
            albedo: [0.8; 3],
            area: 1.0,
            surface,
            attenuation: [1.0; 3],
        });
    }
    let current = receivers
        .iter()
        .map(|receiver| Accumulator {
            surface_light: if receiver.surface == u32::MAX {
                [0.0; 3]
            } else {
                [1.0, 0.5, 0.25]
            },
            ..Accumulator::default()
        })
        .collect::<Vec<_>>();
    let cache = RadianceCache::build(&receivers);
    let unattenuated = clear
        .bounce_pass(&receivers, &current, &cache, 32, 0, false, 1, None)
        .expect("bounce");
    let (pane, pane_alpha) = alpha_plane(
        1.0,
        1000.0,
        MaterialAlpha::blend(0.5),
        None,
        TransportTextureAddress::Clamp,
    );
    let mut covered_triangles = triangles;
    covered_triangles.extend(pane);
    let mut surfaces = vec![None, None];
    surfaces.extend(pane_alpha);
    let covered = TransportScene::new(covered_triangles, Vec::new())
        .expect("pane")
        .with_surface_alpha(surfaces)
        .expect("alpha");
    let attenuated = covered
        .bounce_pass(&receivers, &current, &cache, 32, 0, false, 1, None)
        .expect("attenuated bounce");
    assert!(
        unattenuated[0].surface_light[0] > 0.1,
        "the ceiling must contribute a real diffuse bounce"
    );
    for (actual, baseline) in attenuated[0]
        .surface_light
        .into_iter()
        .zip(unattenuated[0].surface_light)
    {
        assert!(
            (actual - baseline * 0.5).abs() < 1.0e-6,
            "half-opacity BLEND attenuates a real diffuse surface contribution"
        );
    }
}

#[test]
fn alpha_transport_keeps_serial_parallel_and_independent_light_layers_identical() {
    let (mut triangles, mut surfaces) = alpha_plane(
        1.0,
        4.0,
        MaterialAlpha::blend(0.5),
        None,
        TransportTextureAddress::Clamp,
    );
    triangles.extend(floor(-2.0, -2.0, 2.0, 2.0, [0.6; 3]));
    surfaces.extend([None, None]);
    let mut switchable = point([0.0, 2.0, 0.0], 1.0);
    switchable.switchable = Some(7);
    let scene = TransportScene::new(triangles, vec![point([0.0, 2.0, 0.0], 1.0), switchable])
        .expect("scene")
        .with_surface_alpha(surfaces)
        .expect("alpha");
    let charts = [floor_patch(-1.0, -1.0, 1.0, 1.0, 9, 9)];
    let serial = scene
        .solve_with_probes(&charts, options(1, 3), None, false)
        .expect("serial");
    let parallel = scene
        .solve_with_probes(
            &charts,
            SolveOptions {
                workers: 4,
                ..options(1, 3)
            },
            None,
            false,
        )
        .expect("parallel");
    assert_eq!(
        serial, parallel,
        "alpha transport is bit-identical across worker counts"
    );
    assert_eq!(
        serial.solution.charts[0].texels, serial.solution.switchable[0].1[0].texels,
        "identical emitters retain identical independent transport through the pane"
    );
}

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
    for step in 1_i32..20_i32 {
        let at = crate::test_support::exact_f32(step) * 0.1;
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
    let mut edge_texels = 0_i32;
    for row in 1..16 {
        for column in 1..16 {
            let x = crate::test_support::exact_f32(column) / 8.0;
            let z = 2.0 - crate::test_support::exact_f32(row) / 8.0;
            // Independent 64×64 area-coverage oracle for 0 < x-z < 1.
            let mut visible = 0_u16;
            for sy in 0_i32..64_i32 {
                for sx in 0_i32..64_i32 {
                    let px = x + ((crate::test_support::exact_f32(sx) + 0.5) / 64.0 - 0.5) / 8.0;
                    let pz = z + ((crate::test_support::exact_f32(sy) + 0.5) / 64.0 - 0.5) / 8.0;
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
                edge_texels += 1_i32;
            }
        }
    }
    crate::logging::info(format_args!(
        "[coverage-control] edge_texels={edge_texels} absolute_error={errors:?} direct_rays={:?}",
        [hard.direct_rays, medium.direct_rays, high.direct_rays]
    ));
    assert!(edge_texels > 10_i32);
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
        for (actual, expected_channel) in texel.light_at([0.0, 1.0, 0.0]).into_iter().zip(expected)
        {
            assert!((actual - expected_channel).abs() < 1.0e-6);
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

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::as_conversions,
    reason = "Callers sample fixture UVs in 0..=1 on small fixed chart dimensions; flooring to integer texels retains the independent bilinear reference calculation."
)]
fn sample_chart(chart: &SolvedChart, width: usize, height: usize, u: f32, v: f32) -> f32 {
    let x = u * crate::test_support::exact_f32(width - 1);
    let y = v * crate::test_support::exact_f32(height - 1);
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
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::as_conversions,
    reason = "The loop samples x in 0..=8 and caps its floored strip coordinate at 3, within the four-strip fixture."
)]
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
    let strips = (0_i32..4_i32)
        .map(|strip| {
            floor_patch(
                2.0 * crate::test_support::exact_f32(strip),
                0.0,
                2.0 * crate::test_support::exact_f32(strip + 1_i32),
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
    for step in 0_i32..=800_i32 {
        let x = crate::test_support::exact_f32(step) * 0.01;
        let strip = (x / 2.0).floor().min(3.0) as usize;
        let a = sample_chart(&whole.charts[0], 129, 17, x / 8.0, 0.5);
        let b = sample_chart(
            &split.charts[strip],
            33,
            17,
            (x - crate::test_support::exact_f32(strip) * 2.0) / 2.0,
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
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::as_conversions,
    reason = "The loop samples x strictly between 0 and 8 and caps its floored strip coordinate at 3, within the four-strip fixture."
)]
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
    let mut strips = (0_i32..4_i32)
        .map(|strip| {
            floor_patch(
                2.0 * crate::test_support::exact_f32(strip),
                0.0,
                2.0 * crate::test_support::exact_f32(strip + 1_i32),
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
            for step in 1_i32..800_i32 {
                let x = crate::test_support::exact_f32(step) * 0.01;
                let strip = (x / 2.0).floor().min(3.0) as usize;
                let left = sample_chart(&a.charts[0], 129, 17, x / 8.0, 0.5);
                let right = sample_chart(
                    &b.charts[strip],
                    33,
                    17,
                    (x - crate::test_support::exact_f32(strip) * 2.0) / 2.0,
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
    let mut display_chart = chart.clone();
    for value in &mut display_chart.texels {
        value.direction = [value.direction[0], -value.direction[2], value.direction[1]];
    }
    sample_chart(&display_chart, width, height, u, v)
}

#[test]
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::as_conversions,
    reason = "Fixed positive sample coordinates are floored and capped to the four vertical or three horizontal fixture strips."
)]
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
    let vertical = (0_i32..4_i32)
        .map(|i| {
            transform(floor_patch(
                crate::test_support::exact_f32(i) * 2.0,
                0.0,
                crate::test_support::exact_f32(i + 1_i32) * 2.0,
                3.0,
                33,
                49,
            ))
        })
        .collect::<Vec<_>>();
    let horizontal = (0_i32..3_i32)
        .map(|i| {
            transform(floor_patch(
                0.0,
                crate::test_support::exact_f32(i),
                8.0,
                crate::test_support::exact_f32(i + 1_i32),
                129,
                17,
            ))
        })
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
            let whole_light = sample_wall(&a.charts[0], 129, 49, x / 8.0, 1.0 - y / 3.0);
            let vertical_light = sample_wall(
                &b.charts[ix],
                33,
                49,
                (x - crate::test_support::exact_f32(ix) * 2.0) / 2.0,
                1.0 - y / 3.0,
            );
            let horizontal_light = sample_wall(
                &c.charts[iy],
                129,
                17,
                x / 8.0,
                1.0 - (y - crate::test_support::exact_f32(iy)),
            );
            assert!(
                (whole_light - vertical_light).abs() < 2.0e-4
                    && (whole_light - horizontal_light).abs() < 2.0e-4,
                "wall strips taps={taps}: {whole_light} {vertical_light} {horizontal_light}"
            );
        }
    }
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::as_conversions,
    reason = "The fixed positive wall-surround rectangles have dimensions at most 8 m; independent 16-texel-per-metre truncation preserves their small fixture chart counts."
)]
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
        usize::try_from(chart.width).expect("fixture integer fits usize"),
        usize::try_from(chart.height).expect("fixture integer fits usize"),
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
    let tiles = (0_i32..3_i32)
        .flat_map(|y| {
            (0_i32..8_i32).filter_map(move |x| {
                (!(y == 1_i32 && (3_i32..5_i32).contains(&x))).then_some([
                    crate::test_support::exact_f32(x),
                    crate::test_support::exact_f32(y),
                    1.0,
                    1.0,
                ])
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
    for (direct_texel, bounced_texel) in direct.charts[0]
        .texels
        .iter()
        .zip(&bounced.charts[0].texels)
    {
        let direct_light = direct_texel.light_at([0.6, 0.8, 0.0])[0];
        let bounced_light = bounced_texel.light_at([0.6, 0.8, 0.0])[0];
        assert!(
            (direct_light - bounced_light).abs() < 1.0e-6,
            "self bounce added {}",
            bounced_light - direct_light
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
