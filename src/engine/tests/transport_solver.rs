//! Essential regression for grouped-circuit geometric sharing and parallel
//! filtering: compare independent layer solves, then serial/parallel outputs
//! on 100 receivers, a hard divider and tinted water. Under 1 MiB, milliseconds;
//! no assets, full map, filesystem or GPU. Cancellation guards worker scratch
//! publication. Existing package tests do not exercise these solver branches.
use super::*;

fn fixture() -> Result<(TransportScene, Vec<(LightmapPatch, Chart)>), String> {
    let quads = [
        [
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 2.0],
            [2.0, 0.0, 2.0],
            [2.0, 0.0, 0.0],
        ],
        [
            [0.0, 2.0, 0.0],
            [2.0, 2.0, 0.0],
            [2.0, 2.0, 2.0],
            [0.0, 2.0, 2.0],
        ],
        [
            [0.0, 0.0, 2.0],
            [0.0, 2.0, 2.0],
            [2.0, 2.0, 2.0],
            [2.0, 0.0, 2.0],
        ],
        [
            [1.0, 0.0, 0.7],
            [1.0, 1.1, 0.7],
            [1.0, 1.1, 1.3],
            [1.0, 0.0, 1.3],
        ],
    ];
    let mut triangles = Vec::new();
    let mut charts = Vec::new();
    for corners in quads {
        let [a, b, c, d] = corners;
        triangles.push(TransportTriangle::new(a, b, c, [0.7, 0.5, 0.3]).ok_or("triangle")?);
        triangles.push(TransportTriangle::new(a, c, d, [0.7, 0.5, 0.3]).ok_or("triangle")?);
        charts.push((
            LightmapPatch::from_quad(PatchKind::Prop, corners, None).ok_or("patch")?,
            Chart {
                page: 0,
                x: 0,
                y: 0,
                width: 5,
                height: 5,
            },
        ));
    }
    let emitters = (0..4_u16)
        .map(|index| TransportEmitter {
            position: [0.3 + f32::from(index) * 0.4, 1.7, 0.8],
            shape: EmitterShape::Point,
            color: if index.is_multiple_of(2) {
                [0.8, 0.4, 0.2]
            } else {
                [0.2, 0.5, 0.9]
            },
            intensity: 1.0,
            range: 6.0,
            falloff: LightFalloff::Smooth,
            height_factor: 1.0,
            directional: false,
            switchable: (index > 0).then_some(usize::from(index)),
        })
        .collect();
    let scene = TransportScene::new(triangles, emitters)
        .ok_or("scene")?
        .with_switchable_groups(vec![(1, vec![1, 2]), (3, vec![3])])
        .with_water(vec![TransportWaterBody {
            x0: 0.0,
            x1: 0.9,
            z0: 0.0,
            z1: 2.0,
            surface_y: 0.5,
            bottom_y: 0.0,
            extinction: [0.4, 0.2, 0.1],
        }])
        .with_sky([0.01, 0.02, 0.03])
        .with_chart_sample_density(vec![2.0; charts.len()])
        .ok_or("density")?;
    Ok((scene, charts))
}

fn bits(charts: &[SolvedChart]) -> Vec<u32> {
    charts
        .iter()
        .flat_map(|chart| &chart.texels)
        .flat_map(|texel| {
            texel
                .irradiance
                .into_iter()
                .chain(texel.direction)
                .chain(texel.axis)
        })
        .map(f32::to_bits)
        .collect()
}

#[test]
fn grouped_circuits_and_parallel_filter_preserve_independent_layer_bits() -> Result<(), String> {
    let (scene, charts) = fixture()?;
    let options = SolveOptions {
        taps_per_axis: 3,
        bounces: 2,
        gather_samples: 8,
        workers: 1,
    };
    let solved = scene
        .solve_with_probes(&charts, options, None, true)
        .map_err(|e| e.name().to_string())?;
    for (layer, emitters, base) in [
        (None, vec![0], true),
        (Some(1), vec![1, 2], false),
        (Some(3), vec![3], false),
    ] {
        let (mut rays, mut bounces, mut cells) = (0, 0, 0);
        let mut probes = None;
        let reference = scene
            .solve_pass(
                &charts,
                &emitters,
                base,
                3,
                2,
                8,
                1,
                None,
                &mut rays,
                &mut bounces,
                &mut cells,
                Some(&mut probes),
            )
            .map_err(|e| e.name().to_string())?;
        let actual = match layer {
            None => &solved.solution.charts,
            Some(id) => {
                &solved
                    .solution
                    .switchable
                    .iter()
                    .find(|(key, _)| *key == id)
                    .ok_or("missing circuit")?
                    .1
            }
        };
        assert_eq!(
            bits(actual),
            bits(&reference),
            "independent layer {layer:?}"
        );
    }
    let parallel = scene
        .solve_with_probes(
            &charts,
            SolveOptions {
                workers: 4,
                ..options
            },
            None,
            true,
        )
        .map_err(|e| e.name().to_string())?;
    assert_eq!(
        bits(&parallel.solution.charts),
        bits(&solved.solution.charts)
    );
    for ((left_id, left), (right_id, right)) in parallel
        .solution
        .switchable
        .iter()
        .zip(&solved.solution.switchable)
    {
        assert_eq!(left_id, right_id);
        assert_eq!(bits(left), bits(right));
    }
    assert_eq!(parallel.probes, solved.probes);
    assert!(
        solved
            .solution
            .charts
            .iter()
            .flat_map(|chart| &chart.texels)
            .any(|texel| texel.irradiance.iter().any(|value| *value > 0.0))
    );
    Ok(())
}

#[test]
fn cancelled_parallel_solver_does_not_publish_partial_receiver_batches() -> Result<(), String> {
    let (scene, charts) = fixture()?;
    let cancel = AtomicBool::new(true);
    assert_eq!(
        scene.solve(
            &charts,
            SolveOptions {
                workers: 4,
                ..SolveOptions::default()
            },
            Some(&cancel)
        ),
        Err(LightmapFailure::FillSize)
    );
    Ok(())
}
