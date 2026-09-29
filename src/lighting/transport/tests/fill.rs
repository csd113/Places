//! Regressions for chart-independent baseline recovery and maintained emitters.
use super::*;

#[test]
fn baseline_response_is_continuous_and_preserves_half_the_gradient() {
    let target = 0.4;
    let mut previous = target;
    for step in 1..=1000 {
        let current = step as f32 * 0.002;
        let lifted = current + baseline_fill(current, target);
        assert!(lifted.is_finite());
        assert!(lifted >= target);
        assert!(lifted - previous >= 0.001 - 1.0e-6);
        previous = lifted;
    }
    assert_eq!(baseline_fill(0.0, 0.0), 0.0);
    assert_eq!(baseline_fill(2.0, target), 0.0);
}

#[test]
fn splitting_a_chart_cannot_change_its_baseline_correction() {
    let whole = [floor_patch(0.0, 0.0, 4.0, 1.0, 4, 1)];
    let split = [
        floor_patch(0.0, 0.0, 2.0, 1.0, 2, 1),
        floor_patch(2.0, 0.0, 4.0, 1.0, 2, 1),
    ];
    let scene = TransportScene::new(Vec::new(), vec![point([0.0, 2.0, 0.0], 1.0)])
        .expect("scene")
        .with_receiver_target(vec![[0.4; 3]; 4]);
    // Same world samples and physical field, different chart partitioning.
    let receivers: Vec<_> = (0..4)
        .map(|i| TransportReceiver {
            position: [i as f32, 0.0, 0.0],
            ray_origin: [i as f32, 0.0, 0.0],
            normal: [0.0, 1.0, 0.0],
            albedo: [0.7; 3],
            area: 1.0,
            surface: u32::MAX,
            attenuation: [1.0; 3],
        })
        .collect();
    let physical: Vec<_> = [0.0, 0.15, 0.6, 2.0]
        .into_iter()
        .map(|value| LightmapTexel {
            irradiance: [value; 3],
            ..LightmapTexel::ZERO
        })
        .collect();
    let mut unsplit = physical.clone();
    let mut partitioned = physical;
    scene.apply_chart_fill(&whole, &receivers, &mut unsplit);
    scene.apply_chart_fill(&split, &receivers, &mut partitioned);
    assert_eq!(unsplit, partitioned);
    for adjacent in partitioned.windows(2) {
        assert!(adjacent[1].irradiance[0] > adjacent[0].irradiance[0]);
    }
}

#[test]
fn coloured_fill_preserves_moments_and_has_finite_probes_and_texels() {
    let level = fill_test_level();
    let lighting = LevelLighting::bake(&level);
    let charts = fill_test_charts();
    let scene = TransportScene::new(
        Vec::new(),
        vec![TransportEmitter::from_baked(&lighting.lights()[0], None)],
    )
    .expect("scene")
    .with_receiver_target(receiver_targets(&lighting, &charts))
    .with_probe_target(probe_targets(&lighting, &charts));
    let solved = scene
        .solve_with_probes(&charts, options(1, 2), None, true)
        .expect("solve");
    for chart in &solved.solution.charts {
        for texel in &chart.texels {
            assert!(
                texel
                    .irradiance
                    .iter()
                    .chain(&texel.direction)
                    .all(|x| x.is_finite())
            );
        }
    }
    for probe in &solved.probes.expect("probe field").probes {
        assert!(probe.irradiance.iter().all(|x| x.is_finite()));
    }
    let mut texel = LightmapTexel {
        irradiance: [0.1, 0.2, 0.3],
        direction: [0.0, 0.6, 0.0],
        ..LightmapTexel::ZERO
    };
    let before = texel.light_at([0.0, 1.0, 0.0]);
    let target = [0.3, 0.25, 0.15];
    fill_texel(&mut texel, [0.0, 1.0, 0.0], target);
    assert_eq!(texel.direction, [0.0, 0.6, 0.0]);
    for channel in 0..3 {
        let expected = before[channel] + baseline_fill(before[channel], target[channel]);
        assert!((texel.light_at([0.0, 1.0, 0.0])[channel] - expected).abs() < 1.0e-6);
    }
}

#[test]
fn maintained_hallway_recessed_and_hanging_fixtures_cast_direct_pools() {
    let level = LevelDef::from_json(include_str!("../../../../assets/levels/places_demo.json"))
        .expect("demo");
    let lighting = LevelLighting::bake(&level);
    let empty = TransportScene::new(Vec::new(), Vec::new()).expect("scene");
    for (label, x, z) in [
        ("corridor panel", 41.5, 13.0),
        ("recessed can", 9.0, 9.0),
        ("hall flush mount", 61.0, -5.0),
        ("hanging ball", 56.0, 7.6),
    ] {
        let light = lighting
            .lights()
            .iter()
            .find(|light| {
                (light.source.position[0] - x).abs() < 0.01
                    && (light.source.position[2] - z).abs() < 0.01
            })
            .expect(label);
        assert!(light.is_active(), "{label} must be active");
        let emitter = TransportEmitter::from_baked(light, None);
        let under = [x, light.source.position[1] - 2.0, z];
        let (near, _) = emitter.direct(&empty, under, 2);
        let (far, _) = emitter.direct(&empty, [x + 5.5, under[1], z], 2);
        assert!(
            near[0] > 0.01 && near[0] > far[0] + 0.01,
            "{label}: near={near:?} far={far:?}"
        );
        if label == "corridor panel" {
            assert!(
                near[0] > 0.2,
                "the maintained corridor fixture needs useful direct illumination: {near:?}"
            );
        }
    }
    for prop in level.props.iter().filter(|prop| prop.model == "core:lamp") {
        assert!(
            prop.lights.is_empty(),
            "the catalog's non-emissive decorative lamps stay unlit"
        );
    }
}

#[test]
fn authored_support_is_continuous_and_excludes_switchable_lights() {
    let mut light = point([2.0, 3.0, 0.0], 1.0);
    light.directional = true;
    let scene = TransportScene::new(Vec::new(), vec![light]).expect("scene");
    let left = scene.baseline_support([3.9999, 0.0, 0.0]);
    let right = scene.baseline_support([4.0001, 0.0, 0.0]);
    assert!(
        (left - right).abs() < 0.0001,
        "the world-space field cannot step at x=4"
    );
    assert_eq!(scene.baseline_support([2.0, -17.0, 0.0]), 1.0);
    assert_eq!(scene.baseline_support([20.0, 0.0, 0.0]), 0.0);
    let charts = [floor_patch(0.0, 0.0, 4.0, 1.0, 4, 1)];
    light.switchable = Some(0);
    let switchable = TransportScene::new(Vec::new(), vec![light])
        .expect("scene")
        .with_receiver_target(vec![[0.4; 3]; 4]);
    assert_eq!(switchable.baseline_support([2.0, 0.0, 0.0]), 0.0);
    let solved = switchable
        .solve(&charts, options(0, 1), None)
        .expect("solve");
    assert!(
        solved.charts[0]
            .texels
            .iter()
            .all(|texel| texel.irradiance == [0.0; 3])
    );
}

#[test]
fn a_second_bounce_cannot_repeat_a_single_reflection_path() {
    // Only the floor reflects; the ceiling is black. There are no walls.
    // The floor cannot see itself, so K²D is exactly zero.
    let mut triangles = floor(0.0, 0.0, 4.0, 4.0, [0.8; 3]);
    triangles.extend(wall(
        [4.0, 3.0, 4.0],
        [0.0, 3.0, 4.0],
        [0.0, 3.0, 0.0],
        [4.0, 3.0, 0.0],
        [0.0; 3],
    ));
    let scene = TransportScene::new(triangles, vec![point([2.0, 2.0, 2.0], 1.0)]).expect("scene");
    let charts = vec![
        ceiling_patch(0.0, 0.0, 4.0, 4.0, 16, 16),
        floor_patch(0.0, 0.0, 4.0, 4.0, 16, 16),
    ];
    let once = scene
        .solve(&charts, options(1, 3), None)
        .expect("one bounce");
    let twice = scene
        .solve(&charts, options(2, 3), None)
        .expect("two bounces");
    let direct = scene.solve(&charts, options(0, 3), None).expect("direct");
    assert!(first_light(&once, [0.0, -1.0, 0.0])[0] > first_light(&direct, [0.0, -1.0, 0.0])[0]);
    for (a, b) in once.charts.iter().zip(&twice.charts) {
        assert_eq!(
            a.texels, b.texels,
            "a black receiver cannot emit a second bounce"
        );
    }
}

#[test]
fn diffuse_hemisphere_preserves_the_cosine_integral() {
    // Equal solid-angle quadrature with mean cosine 1/2. For unit Lambertian
    // radiance, integral(2*cos(theta) dOmega / 2pi) = 1, not 1.5.
    let mut field = Accumulator::default();
    let tangent = 0.75_f32.sqrt();
    for direction in [
        [tangent, 0.5, 0.0],
        [-tangent, 0.5, 0.0],
        [0.0, 0.5, tangent],
        [0.0, 0.5, -tangent],
    ] {
        accumulate_surface_lobe(&mut field, [0.5; 3], direction, [0.0, 1.0, 0.0]);
    }
    let old = compress(&field).light_at([0.0, 1.0, 0.0]);
    assert!(
        (old[0] - 1.5).abs() < 1.0e-6,
        "the uncalibrated moment overcounts by 50%: {old:?}"
    );
    let corrected = compress_surface(&field, [0.0, 1.0, 0.0]).light_at([0.0, 1.0, 0.0]);
    for value in corrected {
        assert!((value - 1.0).abs() < 1.0e-6);
    }
}

#[test]
fn opposing_tangential_emitters_cannot_light_a_floor() {
    let scene = TransportScene::new(
        Vec::new(),
        vec![point([-2.0, 0.0, 0.0], 1.0), point([2.0, 0.0, 0.0], 1.0)],
    )
    .expect("scene");
    let light = scene.intensity_at([0.0; 3], [0.0, 1.0, 0.0], options(0, 1));
    assert_eq!(
        light.light_at([0.0, 1.0, 0.0]),
        [0.0; 3],
        "both incident cosines are zero"
    );
}

#[test]
fn package_round_trip_preserves_independent_surface_energy() {
    use crate::lighting::lightmap::{LevelLightmaps, LightmapPage, LightmapStats};
    let mut field = Accumulator::default();
    let tangent = 0.75_f32.sqrt();
    for direction in [
        [tangent, 0.5, 0.0],
        [-tangent, 0.5, 0.0],
        [0.0, 0.5, tangent],
        [0.0, 0.5, -tangent],
    ] {
        accumulate_surface_lobe(&mut field, [1.0, 0.5, 0.25], direction, [0.0, 1.0, 0.0]);
    }
    let normal = [0.0, 1.0, 0.0];
    // Four equal weights times cos(theta)=1/2, independent of compression.
    let expected = [2.0, 1.0, 0.5];
    assert!(!surface_energy_matches(compress(&field), normal, expected));
    let encoded = compress_surface(&field, normal);
    let atlas = LevelLightmaps {
        pages: vec![LightmapPage {
            width: 2,
            height: 2,
            texels: vec![encoded; 4],
        }],
        charts: Vec::new(),
        stats: LightmapStats::default(),
        cache_key: "cosine-integral".to_string(),
        padding: 0,
        switchable: Vec::new(),
    };
    let (meta, ktx) =
        crate::package::lightmaps::write_lightmaps(&atlas).expect("write HDR package record");
    let decoded =
        crate::package::lightmaps::read_lightmaps(&meta, &ktx).expect("runtime package decoder");
    let texel = decoded.pages[0].texels[0];
    for (actual, target) in texel.light_at(normal).iter().zip(expected) {
        assert!(
            (*actual - target).abs() < 0.002,
            "half-float runtime energy {actual} != {target}"
        );
    }
    assert!(
        texel.light_at(normal)[0] > 1.0,
        "package must retain HDR, not clamp it"
    );
}
