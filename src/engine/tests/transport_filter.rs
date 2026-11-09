//! Regression value: finite-support denoising must not shift a sample toward
//! a brighter interior or introduce a chart-boundary seam. Two tiny layouts,
//! twelve receivers at most, no assets, bake, GPU or full-map dependency.
use super::{Accumulator, Chart, LightmapPatch, TransportScene, filter_accumulators};
use crate::engine::lighting::lightmap::PatchKind;
use crate::engine::lighting::transport::TransportTriangle;

fn patch(min_x: f32, max_x: f32) -> Result<LightmapPatch, String> {
    LightmapPatch::from_quad(
        PatchKind::Ceiling,
        [
            [min_x, 0.0, 0.0],
            [max_x, 0.0, 0.0],
            [max_x, 0.0, 2.0],
            [min_x, 0.0, 2.0],
        ],
        None,
    )
    .ok_or_else(|| "finite surface".to_string())
}

#[test]
fn finite_filter_preserves_affine_energy_at_edges_and_across_chart_splits() -> Result<(), String> {
    let triangles = vec![
        TransportTriangle::new([0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [2.0, 0.0, 2.0], [0.5; 3])
            .ok_or("triangle")?,
        TransportTriangle::new([0.0, 0.0, 0.0], [2.0, 0.0, 2.0], [0.0, 0.0, 2.0], [0.5; 3])
            .ok_or("triangle")?,
    ];
    for split in [false, true] {
        let charts: Vec<_> = if split {
            vec![
                (
                    patch(0.0, 1.0)?,
                    Chart {
                        page: 0,
                        x: 0,
                        y: 0,
                        width: 2,
                        height: 3,
                    },
                ),
                (
                    patch(1.0, 2.0)?,
                    Chart {
                        page: 0,
                        x: 0,
                        y: 0,
                        width: 2,
                        height: 3,
                    },
                ),
            ]
        } else {
            vec![(
                patch(0.0, 2.0)?,
                Chart {
                    page: 0,
                    x: 0,
                    y: 0,
                    width: 3,
                    height: 3,
                },
            )]
        };
        let scene = TransportScene::new(triangles.clone(), Vec::new())
            .ok_or("scene")?
            .with_chart_sample_density(vec![1.0; charts.len()])
            .ok_or("density")?;
        let receivers = scene.receivers(&charts).map_err(|e| e.name().to_string())?;
        for slope in [0.0, 0.25] {
            let values: Vec<_> = receivers
                .iter()
                .map(|r| {
                    let energy = 1.0 + slope * r.position[0] + 2.0 * slope * r.position[2];
                    Accumulator {
                        irradiance: [energy; 3],
                        surface_light: [energy; 3],
                        ..Accumulator::default()
                    }
                })
                .collect();
            let direct = vec![Accumulator::default(); values.len()];
            let filtered =
                filter_accumulators(&scene, &charts, &receivers, &values, &direct, 1, None)
                    .map_err(|e| e.name().to_string())?;
            for ((receiver, expected), actual) in receivers.iter().zip(&values).zip(filtered) {
                let rgb = actual.light_at(receiver.normal);
                for (got, want) in rgb.into_iter().zip(expected.surface_light) {
                    assert!(
                        (got - want).abs() < 1.0e-5,
                        "split={split} slope={slope} point={:?}: {got} != {want}",
                        receiver.position
                    );
                }
            }
        }
    }
    Ok(())
}

// Essential companion: a no-op filter would pass the affine invariant but
// leave gather noise untouched. Nine receivers establish actual smoothing,
// while a separate sharp direct term must retain its original footprint.
#[test]
fn balanced_filter_smooths_indirect_noise_without_spreading_direct() -> Result<(), String> {
    let patch = patch(0.0, 2.0)?;
    let triangles = vec![
        TransportTriangle::new([0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [2.0, 0.0, 2.0], [0.5; 3])
            .ok_or("triangle")?,
        TransportTriangle::new([0.0, 0.0, 0.0], [2.0, 0.0, 2.0], [0.0, 0.0, 2.0], [0.5; 3])
            .ok_or("triangle")?,
    ];
    let charts = [(
        patch,
        Chart {
            page: 0,
            x: 0,
            y: 0,
            width: 3,
            height: 3,
        },
    )];
    let scene = TransportScene::new(triangles, Vec::new())
        .ok_or("scene")?
        .with_chart_sample_density(vec![1.0])
        .ok_or("density")?;
    let receivers = scene.receivers(&charts).map_err(|e| e.name().to_string())?;
    let light = |energy| Accumulator {
        irradiance: [energy; 3],
        surface_light: [energy; 3],
        ..Accumulator::default()
    };
    let mut indirect = vec![light(1.0); receivers.len()];
    *indirect.get_mut(4).ok_or("centre")? = light(1.1);
    let mut direct = vec![Accumulator::default(); receivers.len()];
    *direct.get_mut(4).ok_or("centre")? = light(2.0);
    let dark = vec![Accumulator::default(); receivers.len()];
    let filtered = filter_accumulators(&scene, &charts, &receivers, &indirect, &dark, 1, None)
        .map_err(|e| e.name().to_string())?;
    let combined = filter_accumulators(&scene, &charts, &receivers, &indirect, &direct, 1, None)
        .map_err(|e| e.name().to_string())?;
    let centre = filtered.get(4).ok_or("centre")?.light_at([0.0, -1.0, 0.0])[0];
    assert!(
        (1.0..1.05).contains(&centre),
        "indirect impulse must be smoothed: {centre}"
    );
    for (index, ((base, lit), receiver)) in
        filtered.iter().zip(&combined).zip(&receivers).enumerate()
    {
        let expected = if index == 4 { 2.0 } else { 0.0 };
        for (a, b) in base
            .light_at(receiver.normal)
            .into_iter()
            .zip(lit.light_at(receiver.normal))
        {
            assert!(
                (b - a - expected).abs() < 1.0e-5,
                "direct footprint at {index}: {b} - {a}"
            );
        }
    }
    Ok(())
}
