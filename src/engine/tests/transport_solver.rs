//! Essential regression for grouped-circuit geometric sharing and parallel
//! filtering: compare independent layer solves, then serial/parallel outputs
//! on 100 receivers, a hard divider and tinted water. Under 1 MiB, milliseconds;
//! no assets, full map, filesystem or GPU. Four circuits exercise later pairs
//! and an odd tail; independent probe planes also protect against sky/fill
//! leaking into a later pair. Cancellation guards worker scratch
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
    let emitters = (0..6_u16)
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
        .with_switchable_groups(vec![
            (1, vec![1, 2]),
            (3, vec![3]),
            (4, vec![4]),
            (5, vec![5]),
        ])
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

fn assert_probe_plane(
    solved: &TransportSolve,
    layer: Option<usize>,
    probes: Option<ProbeField>,
) -> Result<(), String> {
    let expected = probes.ok_or("reference probes")?;
    let field = solved.probes.as_ref().ok_or("actual probes")?;
    if let Some(id) = layer {
        let plane = field
            .switchable_indirect
            .iter()
            .find(|plane| usize::try_from(plane.light_index) == Ok(id))
            .ok_or("missing probe plane")?;
        let texels: Vec<_> = expected
            .probes
            .into_iter()
            .map(ProbeSample::texel)
            .collect();
        assert_eq!(plane.probes, texels, "independent circuit probes {id}");
    } else {
        assert_eq!(field.probes, expected.probes, "independent base probes");
    }
    Ok(())
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
        (Some(4), vec![4], false),
        (Some(5), vec![5], false),
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
        assert_probe_plane(&solved, layer, probes)?;
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

// Essential contact regression: clipping creates triangular floor/ceiling charts.
// Their third edge must choose the owning air side without suppressing a real
// touching wall. A few CPU rays, no bake, files, assets or GPU.
#[test]
fn triangular_contact_receivers_choose_air_and_preserve_touching_wall_shadows() -> Result<(), String>
{
    let corners = [
        [4.0, 3.0, 4.0],
        [5.0, 3.0, 4.0],
        [4.0, 3.0, 5.0],
        [4.0, 3.0, 5.0],
    ];
    let patch = LightmapPatch::from_quad(PatchKind::Ceiling, corners, None).ok_or("patch")?;
    let wall = [
        [5.0, 0.0, 4.0],
        [4.0, 0.0, 5.0],
        [4.0, 3.0, 5.0],
        [5.0, 3.0, 4.0],
    ];
    let scene = TransportScene::new(
        vec![
            TransportTriangle::new(wall[0], wall[1], wall[2], [0.5; 3]).ok_or("wall")?,
            TransportTriangle::new(wall[0], wall[2], wall[3], [0.5; 3]).ok_or("wall")?,
        ],
        Vec::new(),
    )
    .ok_or("scene")?;
    for (u, v) in [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (0.5, 0.5), (1.0, 1.0)] {
        let origin = receiver_ray_origin(&patch, u, v, [0.0, -1.0, 0.0]);
        let local = patch.local_of(origin);
        assert!(
            local.0 > 0.0 && local.1 > 0.0 && local.0 + local.1 < 1.0,
            "all three boundaries choose this triangle's interior, including its reflected half"
        );
        assert!(
            length(sub(origin, patch.point_at(u, v))) < 0.0001,
            "the correction is rounding separation, not a visible lighting bias"
        );
    }
    let origin = receiver_ray_origin(&patch, 0.5, 0.5, [0.0, -1.0, 0.0]);
    assert!(
        !scene.occluded(origin, [4.3, 2.0, 4.3]),
        "the exposed side receives its light"
    );
    assert!(
        scene.occluded(origin, [4.8, 2.0, 4.8]),
        "a real touching wall still occludes"
    );
    let interior = receiver_ray_origin(&patch, 0.25, 0.25, [0.0, -1.0, 0.0]);
    assert_eq!(
        interior,
        receiver_position(patch.point_at(0.25, 0.25), [0.0, -1.0, 0.0]),
        "interior receivers retain their physical origin"
    );
    Ok(())
}

// One direct-only bounded solve proves local chart restriction keeps outside
// occlusion and light inputs, and metadata stays aligned. No filesystem/GPU.
#[test]
fn local_receiver_scope_keeps_full_scene_direct_shadow_bits() -> Result<(), String> {
    use crate::engine::lighting::lightmap::{LightmapConfig, LightmapPlan};
    let mut plan = LightmapPlan::new(LightmapConfig::for_profile(
        crate::engine::quality::QualityProfile::Full,
    ))
    .with_preview_scope([0.0, 0.0, 1.0, 1.0]);
    let mut vertices = [crate::engine::render::Vertex::UNLIT; 6];
    let far = [
        [10.0, 0.0, 10.0],
        [10.0, 0.0, 11.0],
        [11.0, 0.0, 11.0],
        [11.0, 0.0, 10.0],
    ];
    assert!(
        !plan.stamp_emitted(&mut vertices, 0, PatchKind::Floor, far, None),
        "outside receiver charts allocate no atlas space"
    );
    assert!(!plan.failed(), "scoping is not a plan failure");
    assert_eq!(
        plan.chart_count(),
        0,
        "outside geometry does not consume the local atlas"
    );
    assert!(
        plan.max_chart_span_m() <= 2.0,
        "preview architecture subdivides before receiver selection"
    );
    let (scene, charts) = fixture()?;
    let options = SolveOptions {
        taps_per_axis: 3,
        bounces: 0,
        gather_samples: 1,
        workers: 1,
    };
    let full = scene
        .solve(&charts, options, None)
        .map_err(|e| e.name().to_string())?;
    let triangles = scene.triangle_count();
    let emitters = scene.emitter_count();
    let selected = scene.restrict_preview_receivers(&charts, &[true, false, false, false]);
    assert_eq!(
        selected.triangle_count(),
        triangles,
        "outside occluders are retained"
    );
    assert_eq!(
        selected.emitter_count(),
        emitters,
        "outside light sources are retained"
    );
    let local = selected
        .solve(charts.get(..1).ok_or("chart")?, options, None)
        .map_err(|e| e.name().to_string())?;
    assert_eq!(
        bits(&local.charts),
        bits(full.charts.get(..1).ok_or("full chart")?),
        "local direct light and shadows match the same complete scene"
    );
    Ok(())
}
