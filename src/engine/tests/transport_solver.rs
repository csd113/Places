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

// Protects conservative filtering at normal-bin and approximate-plane edges,
// plus exact representative identity (including ties and blocked fallback).
// One tiny synthetic scene, no assets/files/GPU/bake; under 1 MiB, milliseconds.
// Existing solved-output tests cannot identify an incorrectly rejected sample.
#[test]
fn cache_normal_filter_preserves_boundary_and_blocked_selections() -> Result<(), String> {
    check_cache_normal_bins()?;
    let floor = TransportTriangle::new([0.0, 0.0, 0.0], [0.0, 0.0, 2.0], [2.0, 0.0, 0.0], [0.5; 3])
        .ok_or("floor")?;
    let mut near = floor;
    near.normal[0] = 8.0 * f32::EPSILON;
    near.p0[1] = SURFACE_OFFSET_M;
    let mut opposite = floor;
    opposite.normal = [0.0, -1.0, 0.0];
    opposite.p0[1] = -SURFACE_OFFSET_M;
    let divider = TransportTriangle::new(
        [0.5, -1.0, -1.0],
        [0.5, 3.0, -1.0],
        [0.5, -1.0, 3.0],
        [0.5; 3],
    )
    .ok_or("divider")?;
    let scene = TransportScene::new(vec![floor, near, opposite, divider], vec![]).ok_or("scene")?;
    assert!(
        same_lighting_plane(&floor, &near),
        "near-tolerance planes remain compatible"
    );
    let positions = [
        [0.75, 0.0, 0.25],
        [0.25, 0.0, 0.25],
        [0.25, 0.0, 0.25],
        [0.1, 0.0, 0.25],
    ];
    let surfaces = [0_u32, 0, 1, 2];
    let receivers: Vec<_> = positions
        .into_iter()
        .zip(surfaces)
        .map(|(position, surface)| TransportReceiver {
            position,
            ray_origin: receiver_position(position, floor.normal),
            normal: floor.normal,
            albedo: [0.5; 3],
            area: 0.1,
            surface,
            attenuation: [1.0; 3],
        })
        .collect();
    let mut cache = RadianceCache::build(&receivers, &scene.triangles);
    // One deliberate cell keeps the nearest right-hand sample behind the divider;
    // equal distances retain the original left fold and receiver order.
    cache.cells = vec![
        receivers
            .iter()
            .enumerate()
            .map(|(index, receiver)| -> Result<CacheRepresentative, String> {
                Ok(CacheRepresentative {
                    receiver: index,
                    distance: if index == 0 { 0.0 } else { 1.0 },
                    normal_mask: cache_normal_mask(
                        scene
                            .triangles
                            .get(usize::try_from(receiver.surface).map_err(|e| e.to_string())?)
                            .ok_or("surface")?
                            .normal,
                    ),
                })
            })
            .collect::<Result<Vec<_>, String>>()?,
    ];
    let blocked_target = SurfaceCacheTarget::new(&scene, 0, [0.1, 0.0, 0.25]).ok_or("target")?;
    let audit = GatherAudit::default();
    let selected = cache.representative_at_audited::<true>(
        &scene,
        0,
        blocked_target,
        &receivers,
        Some(&audit),
    );
    assert_eq!(
        selected,
        Some(1),
        "blocked nearest falls back to the first equal coplanar sample"
    );
    assert_eq!(
        audit.blocked_nearest.load(Ordering::Relaxed),
        1,
        "exercise blocked-nearest fallback"
    );
    let expected = cache_filter_selections(&cache, &scene, &receivers)?;
    for entry in cache.cells.iter_mut().flatten() {
        entry.normal_mask = u32::MAX;
    }
    let actual = cache_filter_selections(&cache, &scene, &receivers)?;
    assert_eq!(
        actual, expected,
        "filtering preserves every exact selected receiver or absent support"
    );
    cache.cells.get_mut(0).ok_or("cell")?.truncate(1);
    assert_eq!(
        cache.representative_at(&scene, 0, blocked_target, &receivers),
        None,
        "a disconnected nearest with no visible fallback has no support"
    );
    Ok(())
}

fn check_cache_normal_bins() -> Result<(), String> {
    #[cfg(target_pointer_width = "64")]
    assert_eq!(
        std::mem::size_of::<CacheRepresentative>(),
        16,
        "normal mask fills existing representative padding"
    );

    for edge in [-0.5_f32, 0.5] {
        for step in -20_i16..=20 {
            let value = edge + f32::from(step) * f32::EPSILON;
            for delta in -8_i16..=8 {
                for axis in 0..3 {
                    let mut left = [0.0, 1.0, 0.0];
                    let mut right = left;
                    *left.get_mut(axis).ok_or("axis")? = value;
                    *right.get_mut(axis).ok_or("axis")? = value + f32::from(delta) * f32::EPSILON;
                    let query = cache_normal_query(left);
                    assert_eq!(
                        cache_normal_mask(right) & query,
                        query,
                        "normal tolerance cannot cross a rejected bin"
                    );
                }
            }
        }
    }
    Ok(())
}

fn cache_filter_selections(
    cache: &RadianceCache,
    scene: &TransportScene,
    receivers: &[TransportReceiver],
) -> Result<Vec<Option<usize>>, String> {
    let mut result = Vec::new();
    for surface in 0..scene.triangles.len() {
        for point in [[0.1, 0.0, 0.25], [0.9, 0.0, 0.25], [0.5, 0.0, 0.25]] {
            let target = SurfaceCacheTarget::new(scene, surface, point).ok_or("target")?;
            result.push(cache.representative_at(scene, 0, target, receivers));
        }
    }
    Ok(result)
}

// Essential ceiling-streak regression: a black half-wall has an analytic
// cosine-weighted sky visibility. Shared azimuths made its error constant along
// radial strips (10.3 percentage points). 1,122 receivers, two triangles and
// three one-bounce solves stay below 1 MiB and need no map, assets or GPU.
fn diffuse_wall_end_charts() -> Result<Vec<(LightmapPatch, Chart)>, String> {
    let mut charts = Vec::new();
    for index in 0..17_u16 {
        let theta = -1.2 + f32::from(index) * 0.15;
        let (sin, cos) = theta.sin_cos();
        let origin = [-0.5 * cos, 3.0, 0.5 * sin];
        let u = [-3.0 * cos, 0.0, 3.0 * sin];
        let v = [-0.002 * sin, 0.0, -0.002 * cos];
        let patch = LightmapPatch::from_quad(
            PatchKind::Ceiling,
            [
                origin,
                add(origin, u),
                add(add(origin, u), v),
                add(origin, v),
            ],
            None,
        )
        .ok_or("ceiling strip")?;
        charts.push((
            patch,
            Chart {
                page: 0,
                x: 0,
                y: 0,
                width: 33,
                height: 2,
            },
        ));
    }
    Ok(charts)
}

#[test]
fn diffuse_wall_end_has_no_coherent_radial_bias_and_is_chart_order_stable() -> Result<(), String> {
    let mut charts = diffuse_wall_end_charts()?;
    let wall = [
        [0.0, 3.0, 0.0],
        [0.0, -10_000.0, 0.0],
        [0.0, -10_000.0, 10_000.0],
        [0.0, 3.0, 10_000.0],
    ];
    let blocked = TransportScene::new(
        vec![
            TransportTriangle::new(wall[0], wall[1], wall[2], [0.0; 3]).ok_or("wall")?,
            TransportTriangle::new(wall[0], wall[2], wall[3], [0.0; 3]).ok_or("wall")?,
        ],
        Vec::new(),
    )
    .ok_or("blocked scene")?
    .with_sky([1.0; 3]);
    let clear = TransportScene::new(Vec::new(), Vec::new())
        .ok_or("clear scene")?
        .with_sky([1.0; 3]);
    let options = SolveOptions {
        taps_per_axis: 1,
        bounces: 1,
        gather_samples: 64,
        workers: 1,
    };
    let solved = blocked
        .solve(&charts, options, None)
        .map_err(|e| e.name().to_string())?;
    let reference = clear
        .solve(&charts, options, None)
        .map_err(|e| e.name().to_string())?;
    let mut squared_error = 0.0;
    let mut receiver_count = 0_u16;
    for (strip, base) in solved.charts.iter().zip(&reference.charts) {
        let mut observed = 0.0;
        let mut expected = 0.0;
        for ((receiver, texel), unoccluded) in
            strip.receivers.iter().zip(&strip.texels).zip(&base.texels)
        {
            // Normalize the finite polar quadrature using the unobstructed
            // control; only the wall-end azimuthal visibility is under test.
            assert!(
                (unoccluded.light_at([0.0, -1.0, 0.0])[0] - 1.0).abs() < 1.0e-4,
                "importance sampling must preserve unit isotropic-sky irradiance"
            );
            let analytic = 0.75
                - (receiver.position[2] / receiver.position[0].abs()).atan()
                    / std::f32::consts::TAU;
            let fraction =
                texel.light_at([0.0, -1.0, 0.0])[0] / unoccluded.light_at([0.0, -1.0, 0.0])[0];
            observed += fraction;
            squared_error += (fraction - analytic).powi(2);
            receiver_count = receiver_count.saturating_add(1);
            expected += analytic;
        }
        let count =
            f32::from(u16::try_from(strip.texels.len()).map_err(|error| error.to_string())?);
        assert!(
            ((observed - expected) / count).abs() < 0.04,
            "finite gather must not retain a coherent radial visibility bias: observed={}, expected={}",
            observed / count,
            expected / count
        );
    }
    let rms = (squared_error / f32::from(receiver_count)).sqrt();
    assert!(
        rms < 0.015,
        "spatial gather error must not create mottling: RMS={rms}"
    );
    charts.reverse();
    let reordered = blocked
        .solve(&charts, options, None)
        .map_err(|e| e.name().to_string())?;
    for (actual, expected) in reordered.charts.iter().rev().zip(&solved.charts) {
        assert_eq!(
            actual.texels, expected.texels,
            "world-space phase must not depend on chart order"
        );
    }
    Ok(())
}

// Essential cache-boundary regression: the cover really shadows the upper
// wall, while its lower cached representative remains lit. Four triangles,
// eight receivers, two64-ray one-bounce solves; no assets/GPU/map, under1MiB.
// The old cache supplies0.22false ceiling light; the uncovered control must
// retain illumination, so suppressing all reflected light cannot pass.
fn endcap_occlusion_scene(covered: bool) -> Result<TransportScene, String> {
    let wall = [
        [-1.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [1.0, 2.0, 0.0],
        [-1.0, 2.0, 0.0],
    ];
    let cover = [
        [-2.0, 1.5, 0.2],
        [2.0, 1.5, 0.2],
        [2.0, 1.5, 1.8],
        [-2.0, 1.5, 1.8],
    ];
    let mut triangles = Vec::new();
    for (quad, color) in
        std::iter::once((wall, [1.0; 3])).chain(covered.then_some((cover, [0.0; 3])))
    {
        triangles.push(TransportTriangle::new(quad[0], quad[1], quad[2], color).ok_or("wall")?);
        triangles.push(TransportTriangle::new(quad[0], quad[2], quad[3], color).ok_or("wall")?);
    }
    TransportScene::new(
        triangles,
        vec![TransportEmitter {
            position: [0.0, 1.0, 1.0],
            shape: EmitterShape::Point,
            color: [1.0; 3],
            intensity: 1.0,
            range: 5.0,
            falloff: LightFalloff::Smooth,
            height_factor: 1.0,
            directional: false,
            switchable: None,
        }],
    )
    .ok_or_else(|| "end-cap scene".into())
}

#[test]
fn first_reflection_preserves_secondary_shadow_instead_of_borrowing_cache_light()
-> Result<(), String> {
    let patches = [
        [
            [-0.005, 1.249, 0.0],
            [0.005, 1.249, 0.0],
            [0.005, 1.251, 0.0],
            [-0.005, 1.251, 0.0],
        ],
        [
            [0.0, 2.0, 0.0001],
            [0.001, 2.0, 0.0001],
            [0.001, 2.0, 0.0002],
            [0.0, 2.0, 0.0002],
        ],
    ];
    let charts = patches
        .into_iter()
        .map(|quad| {
            Ok((
                LightmapPatch::from_quad(PatchKind::Prop, quad, None).ok_or("patch")?,
                Chart {
                    page: 0,
                    x: 0,
                    y: 0,
                    width: 2,
                    height: 2,
                },
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let options = SolveOptions {
        taps_per_axis: 3,
        bounces: 1,
        gather_samples: 64,
        workers: 1,
    };
    for covered in [true, false] {
        let scene = endcap_occlusion_scene(covered)?;
        let upper = scene
            .intensity_at([0.0, 1.999, 0.000_001], [0.0, 0.0, 1.0], options)
            .light_at([0.0, 0.0, 1.0]);
        let lower = scene
            .intensity_at([0.0, 1.25, 0.000_001], [0.0, 0.0, 1.0], options)
            .light_at([0.0, 0.0, 1.0]);
        assert!(lower[0] > 0.5, "the lower cache sample really is lit");
        if covered {
            assert_eq!(upper, [0.0; 3], "the opaque cover really shadows the hit");
        } else {
            assert!(upper[0] > 0.3, "removing the cover exposes the source");
        }
        let primary = scene
            .intensity_at([0.0005, 2.0, 0.000_15], [0.0, -1.0, 0.0], options)
            .light_at([0.0, -1.0, 0.0])[0];
        let solved = scene
            .solve(&charts, options, None)
            .map_err(|error| error.name().to_string())?;
        let ceiling = solved.charts.get(1).ok_or("ceiling")?;
        for texel in &ceiling.texels {
            let light = texel.light_at([0.0, -1.0, 0.0])[0];
            if covered {
                assert!(
                    light < 1.0e-5,
                    "shadowed wall hit must not borrow the lower sample: {light}"
                );
            } else {
                assert!(
                    light > primary + 0.1,
                    "the unobstructed control retains reflected light beyond its primary direct term"
                );
            }
        }
    }
    Ok(())
}
