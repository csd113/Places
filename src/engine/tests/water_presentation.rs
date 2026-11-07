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
        clip_plane: [0.0; 4],
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
    assert!(state.visible(&clip(matrix)));
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

/// Regression value: a union can cover a dry gap while every real surface is
/// outside the view. This also checks visible-only copy bounds and cached
/// camera-away/return transitions with two boxes, no assets/GPU/bake (<1 ms).
#[test]
fn disconnected_water_does_not_activate_the_gap_or_expand_a_visible_copy() {
    use crate::engine::render::common::occlusion::OcclusionView;
    use crate::engine::render::wgpu::world::WorldFrame;
    use crate::engine::spatial::{DepthRange, Frustum};
    let frame = |matrix| WorldFrame {
        view_projection: matrix,
        frustum: Frustum::from_view_projection(&matrix, DepthRange::NegativeOneToOne),
        eye: glam::Vec3::ZERO,
        occlusion: OcclusionView::EMPTY,
        clip_plane: [0.0; 4],
    };
    let left = Aabb {
        min: [-3.0, -0.01, -0.5],
        max: [-2.0, 0.01, 0.5],
    };
    let right = Aabb {
        min: [2.0, -0.01, -0.5],
        max: [3.0, 0.01, 0.5],
    };
    let mut state = Interactions {
        surfaces: vec![
            super::Surface::from_bounds(left),
            super::Surface::from_bounds(right),
        ],
        ..Interactions::default()
    };
    let gap = frame(glam::Mat4::IDENTITY);
    assert!(
        gap.visible(&left.union(&right)),
        "the old union produces a false positive"
    );
    assert!(
        !state.visible(&gap),
        "there is no real water in the dry gap"
    );
    let toward = frame(glam::Mat4::from_translation(glam::Vec3::new(
        -2.5, 0.0, 0.0,
    )));
    assert!(state.visible(&toward));
    assert_eq!(state.visible_bounds, Some(right));
    let size = crate::engine::render::DrawableSize::new(1280, 720);
    let region = state.copy_region(toward.view_projection, size);
    assert!(
        region.1.width < size.width,
        "invisible water must not expand the copy"
    );
    assert!(!state.visible(&gap));
    assert!(state.visible(&toward));
    assert_eq!(state.copy_region(toward.view_projection, size), region);
    // A dry triangle inside the gap must not activate optics merely because
    // it shares the uploaded chunk with two disconnected wet triangles.
    let vertex = |position, water_surface| super::super::world::WorldVertex {
        position,
        water_surface,
        ..bytemuck::Zeroable::zeroed()
    };
    let vertices = [
        vertex(left.min, 1.0),
        vertex(left.max, 1.0),
        vertex([-2.0, 0.0, -0.5], 1.0),
        vertex(right.min, 1.0),
        vertex(right.max, 1.0),
        vertex([3.0, 0.0, -0.5], 1.0),
        vertex([-0.5, 0.0, -0.5], 0.0),
        vertex([0.5, 0.0, -0.5], 0.0),
        vertex([0.0, 0.0, 0.5], 0.0),
    ];
    let prepared = super::surface_bounds(&vertices, &[0, 1, 2, 3, 4, 5, 6, 7, 8]);
    assert_eq!(prepared.len(), 2);
    state.install_surfaces(prepared);
    assert!(!state.visible(&gap));
    assert!(state.visible(&toward));
    // Intersecting the water/near plane must retain the safe full-copy fallback.
    let surface_eye = glam::Vec3::new(2.5, 0.0, 0.0);
    let projection = glam::camera::rh::proj::opengl::perspective(1.0, 1.0, 0.05, 100.0);
    for y in [0.0, -0.2] {
        let eye = surface_eye + glam::Vec3::Y * y;
        let view =
            glam::camera::rh::view::look_at_mat4(eye, eye + glam::Vec3::NEG_Z, glam::Vec3::Y);
        let matrix = projection.mul_mat4(&view);
        assert!(
            state.visible(&frame(matrix)),
            "near/underwater visibility remains conservative"
        );
        assert_eq!(state.copy_region(matrix, size), ([0, 0], size));
    }
}

/// One triangle/occluder guards the offscreen-point visibility regression.
/// No assets/GPU/bake; the clipped convex coverage proof costs microseconds.
#[test]
fn clipped_water_keeps_visible_patches_and_rejects_covered_screen_portion() {
    use crate::engine::render::common::occlusion::OcclusionView;
    let size = crate::engine::render::DrawableSize::new(1280, 720);
    let vertex = |position, water_surface| super::super::world::WorldVertex {
        position,
        water_surface,
        ..bytemuck::Zeroable::zeroed()
    };
    // Unique clipping regression: an offscreen vertex defeats the whole-box
    // proof, although every on-screen part is behind one solid triangle.
    let wet = [
        vertex([-5.0, -0.1, -1.0], 1.0),
        vertex([0.1, -0.1, -1.0], 1.0),
        vertex([0.0, 0.1, -1.0], 1.0),
    ];
    let clipped_surfaces = super::surface_bounds(&wet, &[0, 1, 2]);
    assert_eq!(clipped_surfaces.len(), 1);
    let camera =
        crate::engine::render::RenderCamera::new(glam::Vec3::new(0.0, 0.0, 3.0), 0.0, 0.0, 30.0);
    let mut clipped = crate::engine::render::wgpu::world::prepare_world_frame(camera, size);
    let solid = crate::engine::render::common::occlusion::Occluders::new(
        [[[-4.0, -4.0, 0.0], [4.0, -4.0, 0.0], [0.0, 4.0, 0.0]]].into_iter(),
    );
    clipped.occlusion = solid.view(clipped.eye, clipped.view_projection);
    for surface in clipped_surfaces {
        assert!(
            clipped.visible(&surface.bounds),
            "the original box extends beyond the blocker"
        );
        assert!(
            surface.visible_bounds(&clipped).is_none(),
            "the clipped polygon is fully hidden"
        );
        clipped.occlusion = OcclusionView::EMPTY;
        assert!(
            surface.visible_bounds(&clipped).is_some(),
            "an unobstructed water patch must remain visible"
        );
    }
}
