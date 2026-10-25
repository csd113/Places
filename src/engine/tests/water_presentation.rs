//! Essential local-event regression coverage: cap/lifetime/reset and real
//! surface crossings versus settled, submerged and teleported subjects. These
//! protect new state ownership absent from the buoyancy/controller suite.
//! Three tiny CPU-only fixtures; no assets, GPU, atlas or map bake (milliseconds).
use super::{Interactions, LIFETIME, MAX_RIPPLES, Ripple};
use crate::engine::level::LevelDef;
use crate::engine::spatial::Aabb;

fn level() -> Result<LevelDef, String> {
    LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"local_water","name":"Local water","spawn":{"x":0,"z":0},"rooms":[{"x":-2,"z":-2,"width":4,"depth":4,"floor_y":-1,"height":4}],"water":[{"x":-1,"z":-1,"width":2,"depth":2,"surface_y":0,"bottom_y":-1}]}"#).map_err(|error| error.to_string())
}

#[test]
fn ripple_budget_expires_and_level_replacement_releases_state() -> Result<(), String> {
    let definition = level()?;
    let mut state = Interactions::default();
    state.install(&definition);
    for index in 0_u16..2000 {
        state.push(Ripple {
            origin_time: [0.0, 0.0, 0.0, f32::from(index) * 0.001],
            shape: [0.03, 0.1, LIFETIME, 0.0],
        });
    }
    assert_eq!(state.ripples.len(), MAX_RIPPLES);
    assert_eq!(
        state.ripples.capacity(),
        MAX_RIPPLES,
        "a burst never grows the event allocation"
    );
    assert_eq!(state.emitted, 2000);
    assert_eq!(state.replaced, 1992);
    state.advance(10.0, 0.016, std::iter::empty());
    assert!(
        state.ripples.is_empty(),
        "every impulse has a hard retirement time"
    );
    state.player = Some(Aabb::EMPTY);
    state.install(&definition);
    assert!(state.ripples.is_empty() && state.tracks.is_empty() && state.player.is_none());
    assert_eq!(
        state.emitted, 0,
        "a new level owns a new interaction history"
    );
    Ok(())
}

#[test]
fn impacts_and_floating_motion_disturb_only_the_real_surface() -> Result<(), String> {
    let mut state = Interactions::default();
    state.install(&level()?);
    let bounds = |x: f32, y: f32| Aabb {
        min: [x - 0.1, y, -0.1],
        max: [x + 0.1, y + 0.3, 0.1],
    };
    assert!(
        state.disturb(bounds(0.0, 0.1), bounds(0.0, -0.1), 1.0, 0.016),
        "entry crosses the water plane"
    );
    assert!(
        state.disturb(bounds(0.0, -0.1), bounds(0.001, -0.101), 1.3, 0.016),
        "a low-amplitude floating duck leaves a restrained wake"
    );
    assert!(
        !state.disturb(bounds(0.0, -0.1), bounds(0.0, -0.1), 1.4, 0.016),
        "settled bodies are silent"
    );
    assert!(
        !state.disturb(bounds(0.0, -0.8), bounds(0.1, -0.8), 1.5, 0.016),
        "fully submerged motion has no surface wake"
    );
    assert!(
        !state.disturb(bounds(10.0, 0.1), bounds(0.0, -0.1), 1.6, 0.016),
        "a teleport is not a physical impact"
    );
    assert!(
        !state.disturb(bounds(1.5, 0.1), bounds(1.5, -0.1), 1.7, 0.016),
        "dry ground cannot emit water rings"
    );
    assert!(
        state
            .ripples
            .iter()
            .all(|ripple| (ripple.shape[3] - 1.0).abs() < f32::EPSILON),
        "events retain their authored water priority"
    );
    let mut rate = Interactions::default();
    rate.install(&level()?);
    assert!(rate.disturb(bounds(0.0, 0.001), bounds(0.0, -0.004), 2.0, 0.01));
    assert!(rate.disturb(bounds(0.0, 0.001), bounds(0.0, -0.009), 2.0, 0.02));
    let strength: Vec<_> = rate.ripples.iter().map(|ripple| ripple.shape[0]).collect();
    let (Some(first), Some(second)) = (strength.first(), strength.get(1)) else {
        return Err("rate events missing".into());
    };
    assert!(
        (first - second).abs() < 1.0e-6,
        "equivalent impact velocities have equal strength at different frame rates"
    );
    // Essential optimization boundary: GPU culling must contain the complete
    // live annulus; CPU preparation must retire future/expired events. Three
    // scalar samples add no assets, GPU, allocations or map preparation.
    let event = Ripple {
        origin_time: [0.0, 0.0, 0.0, 0.0],
        shape: [0.1, 0.08, LIFETIME, 1.0],
    };
    let age = 1.0_f32;
    let prepared = event.prepare(age);
    let front = age.mul_add(1.15, 0.08);
    let width = age.mul_add(0.1, 0.14);
    for radius in [width.mul_add(-3.0, front), front, width.mul_add(3.0, front)] {
        let squared = radius * radius;
        assert!(squared >= prepared.origin_bounds[2] && squared <= prepared.origin_bounds[3]);
    }
    assert!(prepared.profile[0] > 0.0 && prepared.profile[0] < event.shape[0]);
    assert_eq!(event.prepare(-0.1).profile, [0.0; 4]);
    assert_eq!(event.prepare(LIFETIME).profile, [0.0; 4]);
    assert_eq!(state.ripples.len(), 2);
    assert!(
        state
            .ripples
            .iter()
            .all(|ripple| ripple.origin_time[1].abs() < f32::EPSILON)
    );
    Ok(())
}

/// Guards the native menu regression and partial-colour-copy safety. One
/// source box/occluder and eight projected corners; no GPU, assets or bake.
#[test]
fn water_visibility_and_copy_region_preserve_conservative_edges() -> Result<(), String> {
    let mut state = Interactions::default();
    state.install(&level()?);
    let clip = |matrix| crate::engine::render::wgpu::world::WorldFrame {
        view_projection: matrix,
        frustum: crate::engine::spatial::Frustum::from_view_projection(
            &matrix,
            crate::engine::spatial::DepthRange::NegativeOneToOne,
        ),
        eye: glam::Vec3::ZERO,
        occlusion: crate::engine::render::common::occlusion::OcclusionView::EMPTY,
    };
    assert!(state.visible(&clip(glam::Mat4::IDENTITY)));
    assert!(
        !state.visible(&clip(glam::Mat4::from_translation(glam::Vec3::new(
            10.0, 0.0, 0.0
        )))),
        "offscreen water cannot activate refraction in a dry menu view"
    );
    let eye = glam::Vec3::new(0.0, 1.0, 3.0);
    let view = glam::camera::rh::view::look_at_mat4(eye, glam::Vec3::ZERO, glam::Vec3::Y);
    let projection = glam::camera::rh::proj::opengl::perspective(1.0, 1.0, 0.05, 100.0);
    let matrix = projection.mul_mat4(&view);
    let occluders = crate::engine::render::common::occlusion::Occluders::new(
        [[[-4.0, -2.0, 2.0], [4.0, -2.0, 2.0], [0.0, 3.0, 2.0]]].into_iter(),
    );
    let mut hidden = clip(matrix);
    hidden.eye = eye;
    assert!(state.visible(&hidden), "the pool is inside this frustum");
    hidden.occlusion = occluders.view(eye, matrix);
    assert!(
        !state.visible(&hidden),
        "a fully occluded pool must not trigger a refraction copy/pass in an otherwise dry view"
    );

    let size = crate::engine::render::DrawableSize::new(1280, 720);
    let ([x, y], extent) = state.copy_region(matrix, size);
    assert!(
        x.saturating_add(extent.width) <= size.width
            && y.saturating_add(extent.height) <= size.height
    );
    assert!(
        extent.height < size.height,
        "small projected water avoids copying unrelated colour rows"
    );
    let (origin, full) = state.copy_region(glam::Mat4::ZERO, size);
    assert_eq!(origin, [0, 0]);
    assert_eq!(
        full, size,
        "near/invalid projections fall back to the full safe copy"
    );
    Ok(())
}
