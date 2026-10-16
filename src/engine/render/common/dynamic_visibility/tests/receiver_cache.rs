use super::tests::panel;
use super::*;
use crate::engine::materials::MaterialAlpha;

/// A two-triangle blocker and one UV source protect cache invalidation for
/// the actual receiver payload. It performs only bounded visibility rays;
/// no GPU, asset decoding or full-map preparation is needed.
#[test]
fn cached_uv_receivers_follow_sources_bounds_and_movable_visibility() -> Result<(), String> {
    let level = crate::engine::level::LevelDef::from_json(
        r#"{"format_version":3,"id":"uv_cache","name":"UV cache","spawn":{"x":0,"z":0},"rooms":[{"x":-4,"z":-4,"width":8,"depth":8}],"ceiling_lights":[{"fixture":"arcade:blacklight_tube","x":-2,"z":0,"y":1,"brightness":0,"ultraviolet":2}]}"#,
    )
    .map_err(|error| error.to_string())?;
    let lighting = crate::engine::lighting::LevelLighting::bake(&level);
    let mut sources = super::super::blacklight::BlacklightSet::from_level(&level, &lighting);
    let transport = TransportScene::new(Vec::new(), Vec::new()).ok_or("empty transport")?;
    let mut dynamic = DynamicScene::new();
    let id = dynamic
        .spawn(&panel(MaterialAlpha::OPAQUE), [0.0; 3], 0.0, 1.0, 0.0)
        .ok_or("UV blocker")?;
    let mut rays = DynamicVisibility::new();
    let _initial_sync = rays.sync(&dynamic)?;
    let bounds = Aabb {
        min: [2.0, 0.9, -0.1],
        max: [2.1, 1.1, 0.1],
    };
    let mut cache = BlacklightReceiverCache::default();
    let mut sample = |receiver_sources: &super::super::blacklight::BlacklightSet,
                      geometry: &DynamicVisibility,
                      local_bounds,
                      transform,
                      receiver| {
        cache.sample(
            Some(super::super::blacklight::BlacklightVisibility {
                sources: receiver_sources,
                scene: &transport,
                dynamic: Some(geometry),
            }),
            local_bounds,
            transform,
            receiver,
        )
    };
    let blocked = sample(&sources, &rays, bounds, Mat4::IDENTITY, None);
    assert_uv_visibility(blocked, 0.0);
    assert_eq!(
        sample(&sources, &rays, bounds, Mat4::IDENTITY, None),
        blocked,
        "stationary receivers retain the exact anchor payload"
    );
    assert!(dynamic.set_visible(id, false), "suspend the blocker");
    assert!(rays.sync(&dynamic)?, "suspension changes visibility");
    assert_uv_visibility(sample(&sources, &rays, bounds, Mat4::IDENTITY, None), 1.0);
    assert!(dynamic.set_visible(id, true), "restore the blocker");
    assert!(rays.sync(&dynamic)?, "restoration changes visibility");
    assert_uv_visibility(sample(&sources, &rays, bounds, Mat4::IDENTITY, None), 0.0);
    let moved = Mat4::from_translation(Vec3::new(-6.0, 0.0, 0.0));
    assert_uv_visibility(sample(&sources, &rays, bounds, moved, None), 1.0);
    assert_uv_visibility(
        sample(&sources, &rays, bounds, Mat4::IDENTITY, Some(id)),
        1.0,
    );
    assert_uv_visibility(sample(&sources, &rays, bounds, Mat4::IDENTITY, None), 0.0);
    let clear = Aabb {
        min: [-4.0, 0.9, -0.1],
        max: [-3.9, 1.1, 0.1],
    };
    assert_uv_visibility(sample(&sources, &rays, clear, Mat4::IDENTITY, None), 1.0);
    sources = super::super::blacklight::BlacklightSet::default();
    assert_uv_visibility(sample(&sources, &rays, clear, Mat4::IDENTITY, None), 0.0);
    Ok(())
}

fn assert_uv_visibility(receiver: super::super::blacklight::BlacklightReceiver, expected: f32) {
    assert!(
        receiver.visibility.iter().all(|anchor| {
            anchor
                .first()
                .is_some_and(|value| value.to_bits() == expected.to_bits())
        }),
        "all eight anchors must reflect the current visibility: {receiver:?}"
    );
}
