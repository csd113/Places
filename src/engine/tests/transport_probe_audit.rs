//! Surface dump regression on eight receivers at a divider. Production's
//! recovery must match the prefilter estimate on a floor, while a prop has
//! no recovery even with the same positive target. Existing contact tests
//! cover geometry origins only. No map, files, GPU or bake.
use super::super::{
    EmitterShape, LightFalloff, PatchKind, SURFACE_OFFSET_M, TransportEmitter, TransportTriangle,
};
use super::*;

#[test]
fn prefilter_surface_estimate_matches_production_at_wall_contact() -> Result<(), String> {
    // Keep the contact within the rounding-scale inset, with the raw and
    // inset origins on opposite sides of the divider.
    let seam = SURFACE_OFFSET_M;
    let wall = [
        [seam, 0.0, -1.0],
        [seam, 2.0, -1.0],
        [seam, 2.0, 2.0],
        [seam, 0.0, 2.0],
    ];
    let triangles = vec![
        TransportTriangle::new(wall[0], wall[1], wall[2], [0.5; 3]).ok_or("wall")?,
        TransportTriangle::new(wall[0], wall[2], wall[3], [0.5; 3]).ok_or("wall")?,
    ];
    let emitter = |x, range| TransportEmitter {
        position: [x, 1.0, 0.25],
        shape: EmitterShape::Point,
        color: [1.0; 3],
        intensity: 1.0,
        range,
        falloff: LightFalloff::Smooth,
        height_factor: 1.0,
        directional: false,
        switchable: None,
    };
    let scene = TransportScene::new(triangles, vec![emitter(-0.5, 8.0), emitter(0.5, 2.0)])
        .ok_or("scene")?
        .with_receiver_target(vec![[0.25; 3]; 8]);
    let corners = [[0.0; 3], [0.0, 0.0, 1.0], [1.0, 0.0, 1.0], [1.0, 0.0, 0.0]];
    let chart = Chart {
        page: 0,
        x: 0,
        y: 0,
        width: 2,
        height: 2,
    };
    let charts = [
        (
            LightmapPatch::from_quad(PatchKind::Floor, corners, None).ok_or("floor")?,
            chart,
        ),
        (
            LightmapPatch::from_quad(PatchKind::Prop, corners, None).ok_or("prop")?,
            chart,
        ),
    ];
    let receivers = scene
        .receivers(&charts)
        .map_err(super::super::LightmapFailure::name)?;
    let edge = receivers.first().ok_or("floor wall contact")?;
    let raw_support = scene.baseline_support(edge.position);
    let inset_support = scene.baseline_support_from(edge.position, edge.ray_origin);
    assert!(
        inset_support > 0.0 && inset_support < raw_support,
        "the inset blocks the stronger source across the wall and leaves the exposed source"
    );

    let values = [Accumulator::default(); 8];
    let texels = receivers
        .iter()
        .zip(&values)
        .map(|(receiver, value)| compress_surface(value, receiver.normal))
        .collect();
    let production = scene
        .apply_chart_fill(&charts, &receivers, texels, 1, None)
        .map_err(super::super::LightmapFailure::name)?;
    let dumped: Vec<_> = surface_receivers(&scene, &charts, &receivers, &values).collect();
    assert_eq!(
        dumped.len(),
        receivers.len(),
        "all receivers remain in order"
    );
    for ((row, texel), receiver) in dumped.iter().zip(&production).zip(&receivers) {
        assert_eq!(
            row.physical, [0.0; 3],
            "the physical input remains unfilled"
        );
        assert_eq!(
            row.filled,
            texel.light_at(receiver.normal),
            "diagnostic fill uses production recovery on identical prefilter light"
        );
    }
    assert!(
        dumped.first().ok_or("floor dump")?.filled[0] > 0.0,
        "the architectural wall contact retains visible fill"
    );
    assert_eq!(
        dumped.get(4).ok_or("prop dump")?.filled,
        [0.0; 3],
        "the prop wall contact receives no authored fill"
    );
    Ok(())
}
