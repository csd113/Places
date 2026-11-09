//! Protect the ceiling-contact glow regression: a down-facing fixture must
//! project its emitting face independently of the receiving surface's cosine.
//! Analytical point/finite-panel cases distinguish this from receiver-only
//! lighting and guard the isotropic control. No assets, maps, GPU or bakes;
//! two empty scenes and fewer than 100 direct rays take milliseconds.
use super::*;

fn emitter(directional: bool) -> TransportEmitter {
    TransportEmitter {
        position: [0.0, 2.0, 0.0],
        shape: EmitterShape::Point,
        color: [1.0; 3],
        intensity: 1.0,
        range: 10.0,
        falloff: LightFalloff::Constant,
        height_factor: 1.0,
        directional,
        switchable: None,
    }
}

#[test]
fn ceiling_emission_has_no_horizon_or_backside_energy() -> Result<(), String> {
    let source = emitter(true);
    let scene = TransportScene::new(Vec::new(), vec![source]).ok_or("scene")?;
    let options = SolveOptions {
        taps_per_axis: 1,
        bounces: 0,
        gather_samples: 1,
        workers: 1,
    };
    for (position, normal, expected) in [
        ([0.0, 0.0, 0.0], [0.0, 1.0, 0.0], 0.6),
        ([0.0, -20.0, 0.0], [0.0, 1.0, 0.0], 0.6),
        ([0.0, 1.0, 1.0], [0.0, 1.0, 0.0], 0.3),
        ([0.0, 1.0, 1.0], [0.0, 0.0, -1.0], 0.3),
        ([0.0, 2.0, 1.0], [0.0, 0.0, -1.0], 0.0),
        ([0.0, 3.0, 1.0], [0.0, 0.0, -1.0], 0.0),
    ] {
        let light = scene
            .intensity_at(position, normal, options)
            .light_at(normal);
        assert!(
            light.iter().all(|value| (value - expected).abs() < 1.0e-6),
            "ceiling emission at {position:?}: {light:?}, expected {expected}"
        );
    }
    let horizon = [0.0, 2.0, 1.0];
    let (summary, _) = source.direct(&scene, horizon, 1);
    assert_eq!(
        summary, [0.0; 3],
        "summary queries must not restore horizon light"
    );
    assert!(
        (scene.baseline_support(horizon) - 1.0).abs() < 1.0e-6,
        "authored geometric room-fill support is independent of projected emission"
    );
    let isotropic = TransportScene::new(Vec::new(), vec![emitter(false)]).ok_or("control")?;
    let normal = [0.0, 0.0, -1.0];
    let control = isotropic
        .intensity_at(horizon, normal, options)
        .light_at(normal);
    assert!(
        control.iter().all(|value| (value - 0.6).abs() < 1.0e-6),
        "ordinary isotropic source changed: {control:?}"
    );
    Ok(())
}

#[test]
fn finite_ceiling_panel_projects_each_visible_tap() -> Result<(), String> {
    let source = TransportEmitter {
        shape: EmitterShape::Rect {
            u: [1.0, 0.0, 0.0],
            v: [0.0, 0.0, 1.0],
        },
        ..emitter(true)
    };
    let scene = TransportScene::new(Vec::new(), vec![source]).ok_or("scene")?;
    let options = SolveOptions {
        taps_per_axis: 2,
        bounces: 0,
        gather_samples: 1,
        workers: 1,
    };
    // Four equal taps at (+/-0.5, 2, +/-0.5): both cosines are
    // 2/sqrt(4.5), so E = 0.6 * 4/4.5, not the centre's 0.6.
    let normal = [0.0, 1.0, 0.0];
    let light = scene
        .intensity_at([0.0; 3], normal, options)
        .light_at(normal);
    let expected = 0.6 * 4.0 / 4.5;
    assert!(
        light.iter().all(|value| (value - expected).abs() < 1.0e-6),
        "per-tap projected panel energy: {light:?}, expected {expected}"
    );
    let (weight, _) = source.direct(&scene, [0.0; 3], 2);
    let expected_weight = 0.6 * 2.0 / 4.5_f32.sqrt();
    assert!(
        weight
            .iter()
            .all(|value| (value - expected_weight).abs() < 1.0e-6),
        "summary must retain the emitter cosine without a receiver cosine"
    );
    Ok(())
}
