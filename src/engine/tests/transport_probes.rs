//! Audit regression: point-source shadow references isolate probe cache and
//! angular errors. Three tiny CPU solves (1690 receivers, eight probes each)
//! plus analytic sky/closed controls; no assets, filesystem, GPU or map bake.
use super::*;

fn fixture(
    albedo: f32,
    sealed: bool,
) -> Result<(TransportScene, Vec<(LightmapPatch, Chart)>), String> {
    let mut quads: [[[f32; 3]; 4]; 10] = [
        [[0., 0., 0.], [0., 0., 3.], [3., 0., 3.], [3., 0., 0.]],
        [[0., 3., 0.], [3., 3., 0.], [3., 3., 3.], [0., 3., 3.]],
        [[0., 0., 0.], [3., 0., 0.], [3., 3., 0.], [0., 3., 0.]],
        [[0., 0., 3.], [0., 3., 3.], [3., 3., 3.], [3., 0., 3.]],
        [[0., 0., 0.], [0., 3., 0.], [0., 3., 3.], [0., 0., 3.]],
        [[3., 0., 0.], [3., 0., 3.], [3., 3., 3.], [3., 3., 0.]],
        // Finite 0.1m opaque hanging slab; two sides, bottom and exposed end.
        [
            [1.55, 1.2, 0.],
            [1.55, 3., 0.],
            [1.55, 3., 2.2],
            [1.55, 1.2, 2.2],
        ],
        [
            [1.45, 1.2, 0.],
            [1.45, 1.2, 2.2],
            [1.45, 3., 2.2],
            [1.45, 3., 0.],
        ],
        [
            [1.45, 1.2, 0.],
            [1.55, 1.2, 0.],
            [1.55, 1.2, 2.2],
            [1.45, 1.2, 2.2],
        ],
        [
            [1.45, 1.2, 2.2],
            [1.55, 1.2, 2.2],
            [1.55, 3., 2.2],
            [1.45, 3., 2.2],
        ],
    ];

    if sealed {
        for point in quads.iter_mut().skip(6).flatten() {
            if point[1].to_bits() == 1.2_f32.to_bits() {
                point[1] = 0.0;
            }
            if point[2].to_bits() == 2.2_f32.to_bits() {
                point[2] = 3.0;
            }
        }
    }
    let mut triangles = Vec::new();
    let mut charts = Vec::new();
    for corners in quads {
        let [a, b, c, d] = corners;
        triangles.push(TransportTriangle::new(a, b, c, [albedo; 3]).ok_or("triangle")?);
        triangles.push(TransportTriangle::new(a, c, d, [albedo; 3]).ok_or("triangle")?);
        charts.push((
            LightmapPatch::from_quad(PatchKind::Wall, corners, None).ok_or("patch")?,
            Chart {
                page: 0,
                x: 0,
                y: 0,
                width: 13,
                height: 13,
            },
        ));
    }
    let emitter = TransportEmitter {
        position: [0.8, 2.7, 0.8],
        shape: EmitterShape::Point,
        color: [1.0; 3],
        intensity: 1.0,
        range: 5.0,
        falloff: LightFalloff::Smooth,
        height_factor: 1.0,
        directional: false,
        switchable: None,
    };
    Ok((
        TransportScene::new(triangles, vec![emitter]).ok_or("scene")?,
        charts,
    ))
}

fn field(albedo: f32, sealed: bool) -> Result<ProbeField, String> {
    let (scene, charts) = fixture(albedo, sealed)?;
    scene
        .solve_with_probes(
            &charts,
            SolveOptions {
                taps_per_axis: 3,
                bounces: 0,
                gather_samples: 1,
                workers: 1,
            },
            None,
            true,
        )
        .map_err(|e| e.name().to_string())?
        .probes
        .ok_or_else(|| "probes".to_string())
}

#[test]
fn probe_shadow_references_preserve_first_hit_and_quadrature() -> Result<(), String> {
    let actual = field(0.7, false)?;
    let direct = field(0.0, false)?;
    // Independent uniform-sphere 32768-ray exact-hit integration. Point source
    // removes area-tap ambiguity. Refinement8192->32768 is <0.43% at these points.
    let reference = [
        0.122_196_026,
        0.031_286_96,
        0.230_031_49,
        0.009_116_776,
        0.101_537_526,
        0.032_901_786,
        0.144_257_78,
        0.024_490_25,
    ];
    assert_eq!(actual.probes.len(), reference.len());
    for (index, expected) in reference.into_iter().enumerate() {
        let probe = actual.probes.get(index).ok_or("probe")?;
        let black = direct.probes.get(index).ok_or("black probe")?;
        let reflected = probe.irradiance[0] - black.irradiance[0];
        assert!(
            (reflected - expected).abs() < expected * 0.03,
            "probe{index} reflected={reflected} reference={expected}"
        );
    }
    Ok(())
}

#[test]
fn probe_isotropic_and_sealed_controls_keep_energy_and_zero() -> Result<(), String> {
    let closed = field(0.7, true)?;
    let (closed_scene, _) = fixture(0.7, true)?;
    let mut tested = false;
    for (index, probe) in closed.probes.iter().enumerate() {
        let point = closed.probe_position(index).ok_or("position")?;
        if point[0] > 2.0 && closed_scene.probe_is_clear(point) {
            assert_eq!(probe.irradiance, [0.0; 3], "sealed dark compartment");
            tested = true;
        }
    }
    assert!(tested, "at least one valid probe in sealed compartment");
    let scene = TransportScene::new(Vec::new(), Vec::new())
        .ok_or("sky scene")?
        .with_sky([0.08, 0.04, 0.02]);
    let cache = RadianceCache::build(&[], &[]);
    let lighting = ProbeLighting {
        receivers: &[],
        cache: &cache,
        indirect: None,
        emitters: &[],
        taps: 1,
        base: true,
    };
    let (probe, _, _, _) = bake_probe(&scene, &lighting, [0.0; 3], 0, false);
    for (actual, expected) in probe.irradiance.into_iter().zip([0.08, 0.04, 0.02]) {
        assert!((actual - expected).abs() < 1e-6, "constant sky energy");
    }
    assert!(
        probe.direction.iter().all(|v| v.abs() < 1e-6),
        "antipodal sky moment"
    );
    Ok(())
}
