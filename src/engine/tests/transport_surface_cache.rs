//! Contact regression: a later bounce must sample the hit coordinate, not the
//! nearest coarse-cell centre. One 25-receiver plane with affine energy proves
//! endpoint/gradient preservation, with no assets, map bake or GPU work.
use crate::engine::lighting::{
    lightmap::{Chart, LightmapPatch, PatchKind},
    transport::{ReceiverSet, TransportScene, TransportTriangle},
};

#[test]
fn cached_surface_preserves_hit_coordinates_at_boundaries() -> Result<(), String> {
    let corners = [
        [0.0, 0.0, 0.0],
        [2.0, 0.0, 0.0],
        [2.0, 0.0, 2.0],
        [0.0, 0.0, 2.0],
    ];
    let [a, b, c, d] = corners;
    let scene = TransportScene::new(
        vec![
            TransportTriangle::new(a, b, c, [0.5; 3]).ok_or("triangle")?,
            TransportTriangle::new(a, c, d, [0.5; 3]).ok_or("triangle")?,
        ],
        Vec::new(),
    )
    .ok_or("scene")?;
    let charts = [(
        LightmapPatch::from_quad(PatchKind::Ceiling, corners, None).ok_or("patch")?,
        Chart {
            page: 0,
            x: 0,
            y: 0,
            width: 5,
            height: 5,
        },
    )];
    let prepared = ReceiverSet::new(&scene, &charts).map_err(|e| e.name().to_string())?;
    let values: Vec<_> = prepared
        .receivers
        .iter()
        .map(|receiver| [1.0 + receiver.position[0] + 2.0 * receiver.position[2]; 3])
        .collect();
    let cache = prepared.cache(&scene, &charts);
    for point in [
        [0.0, 0.0, 0.0],
        [2.0, 0.0, 2.0],
        [0.0, 0.0, 1.25],
        [0.125, 0.0, 1.125],
        [1.25, 0.0, 0.125],
    ] {
        let (_, surface) = scene
            .near_surface(point, 0.001, false, Some([0.0, -1.0, 0.0]))
            .ok_or("surface")?;
        let sampled = cache
            .surface_stencil(&scene, point, surface, &prepared.receivers, None)
            .sample(&values);
        let expected = 1.0 + point[0] + 2.0 * point[2];
        for actual in sampled {
            assert!(
                (actual - expected).abs() < 1.0e-5,
                "hit={point:?}: {actual} != {expected}"
            );
        }
    }
    Ok(())
}
