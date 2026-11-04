//! Two asset-free CPU regressions protect receiver-plane ownership and the
//! eight-slot selection regression. They cost milliseconds and avoid a map bake;
//! real shader clipping and cabin aperture behavior have native fixtures.
use glam::Vec3;

use super::{ContactShadowUniform, contact_relevance, contact_shadow, select_contact_shadows};
use crate::engine::level::{LevelDef, LevelSurfaces};
use crate::engine::lighting::transport::{TransportScene, TransportTriangle};
use crate::engine::spatial::{Aabb, DepthRange, Frustum};

#[test]
fn contact_receiver_plane_tracks_slope_and_own_storey() -> Result<(), String> {
    let level = LevelDef::from_json(
        r#"{"schema":"places-level","format_version":1,"id":"contact","name":"Contact",
        "spawn":{"x":0,"z":0},"rooms":[
          {"x":-2,"z":-2,"width":4,"depth":4,"floor_y":0,"height":3},
          {"x":-2,"z":-2,"width":4,"depth":4,"floor_y":4,"height":3}],
        "ramps":[{"x":-1,"z":-2,"width":2,"depth":4,"rise":1}]}"#,
    )
    .map_err(|error| error.to_string())?;
    let surfaces = LevelSurfaces::new(&level);
    for (bottom, floor) in [(0.5_f32, 0.5_f32), (4.5, 4.5)] {
        let shadow = contact_shadow(
            Aabb {
                min: [-0.2, bottom, -0.2],
                max: [0.2, bottom + 0.4, 0.2],
            },
            &surfaces,
            [0.0, 1.0, 0.0],
        )
        .ok_or("missing contact")?;
        assert!(
            (shadow.position_floor[3] - floor).abs() < 0.0001,
            "stacked floors must retain the subject's own support"
        );
        let plane = glam::Vec4::from_array(shadow.receiver_plane);
        assert!(
            plane.dot(glam::vec4(0.0, floor + 0.25, 1.0, 1.0)).abs() < 0.0001,
            "receiver plane must follow the slope beyond the subject centre"
        );
        assert!(
            plane.dot(glam::vec4(0.0, floor + 4.0, 0.0, 1.0)).abs() > 3.0,
            "another storey cannot receive this contact"
        );
    }
    Ok(())
}

#[test]
fn hidden_insertions_cannot_displace_nearby_contact_and_ties_are_stable() -> Result<(), String> {
    let triangle = TransportTriangle::new(
        [0.0, -1.0, -4.0],
        [0.0, 4.0, -4.0],
        [0.0, 4.0, 4.0],
        [0.5; 3],
    )
    .ok_or("wall triangle")?;
    let other = TransportTriangle::new(
        [0.0, -1.0, -4.0],
        [0.0, 4.0, 4.0],
        [0.0, -1.0, 4.0],
        [0.5; 3],
    )
    .ok_or("wall triangle")?;
    let scene = TransportScene::new(vec![triangle, other], Vec::new()).ok_or("scene")?;
    let frustum =
        Frustum::from_view_projection(&glam::Mat4::IDENTITY, DepthRange::NegativeOneToOne);
    let eye = Vec3::new(0.8, 0.8, 0.8);
    let make = |x: f32| ContactShadowUniform {
        position_floor: [x, 0.2, 0.0, 0.0],
        half_height: [0.1, 0.1, 0.0, 0.4],
        incoming: [0.0, 1.0, 0.0, 1.0],
        receiver_plane: [0.0, 1.0, 0.0, 0.0],
    };
    let local = make(0.6);
    let hidden = make(-0.6);
    assert!(
        contact_relevance(hidden, eye, &frustum, Some(&scene)).is_none(),
        "a floor behind a solid internal wall has no visible receiver relevance"
    );
    let distance =
        contact_relevance(local, eye, &frustum, Some(&scene)).ok_or("local relevance")?;
    let selected =
        select_contact_shadows(std::iter::repeat_n(hidden, 8).chain([local]).filter_map(
            |shadow| {
                contact_relevance(shadow, eye, &frustum, Some(&scene)).map(|score| (shadow, score))
            },
        ));
    assert_eq!(
        selected.1, 1,
        "hidden insertions cannot consume the eight slots"
    );
    assert_eq!(
        selected.0[0], local,
        "visible local shadow must remain selected"
    );
    let candidates: Vec<_> = (0_u8..12).map(|i| (make(f32::from(i)), distance)).collect();
    let forward = select_contact_shadows(candidates.iter().copied());
    let reversed = select_contact_shadows(candidates.iter().rev().copied());
    assert_eq!(forward, reversed, "ties must be independent of spawn order");
    assert_eq!(forward.1, 8, "retain the original fixed budget");
    Ok(())
}
