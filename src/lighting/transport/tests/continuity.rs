//! Production allocator controls for physical footprints and real boundaries.

use super::{patch_normal, receiver_position, supported_point, surface_axes};
use crate::lighting::lightmap::{LightmapConfig, LightmapPatch, LightmapPlan, PatchKind};
use crate::lighting::transport::{
    SolveOptions, TransportReceiver, TransportScene, TransportTriangle,
};
use crate::render::Vertex;

fn quad(x0: f32, z0: f32, x1: f32, z1: f32, y: f32) -> [[f32; 3]; 4] {
    [[x0, y, z1], [x1, y, z1], [x1, y, z0], [x0, y, z0]]
}

fn triangles(corners: [[f32; 3]; 4], albedo: [f32; 3]) -> Result<Vec<TransportTriangle>, String> {
    [
        [corners[0], corners[1], corners[2]],
        [corners[0], corners[2], corners[3]],
    ]
    .into_iter()
    .map(|[a, b, c]| {
        TransportTriangle::new(a, b, c, albedo).ok_or_else(|| "valid control triangle".to_owned())
    })
    .collect()
}

fn sun() -> crate::lighting::directional::DirectionalLight {
    crate::lighting::directional::DirectionalLight {
        incoming: [0.0, 1.0, 0.0],
        color: [1.0; 3],
        intensity: 1.0,
        cast_shadows: true,
        angular_radius: 0.0,
    }
}

fn options(taps: u8) -> SolveOptions {
    SolveOptions {
        taps_per_axis: taps,
        bounces: 0,
        gather_samples: 1,
        workers: 1,
    }
}

#[test]
fn production_triangle_frames_share_the_same_direct_footprint() -> Result<(), String> {
    // The hero seat's actual unequal edge frames. Its old allocations were
    // 6×7 and 7×4, although the diagonal samples occupied identical positions.
    let [a, b, c, d] = [
        [0.0, 0.0, 0.0],
        [0.0, 0.0, 0.632],
        [0.462, 0.0, 0.632],
        [0.462, 0.0, 0.0],
    ];
    let mut plan = LightmapPlan::new(LightmapConfig::for_profile(
        crate::quality::QualityProfile::Full,
    ));
    for corners in [[a, b, c], [a, c, d]] {
        let mut vertices = corners.map(|pos| Vertex {
            pos,
            ..Vertex::UNLIT
        });
        assert!(
            plan.stamp_prop_triangle(&mut vertices, 8.0),
            "control triangle must receive an atlas chart"
        );
    }
    let left = plan.charts()[0].1;
    let right = plan.charts()[1].1;
    assert_ne!(
        left.width, right.width,
        "control must retain unequal triangle edge widths"
    );
    assert_ne!(
        left.height, right.height,
        "control must retain unequal triangle edge heights"
    );
    assert_eq!(
        left.height, right.width,
        "both charts must share the same diagonal sample count"
    );
    let mut geometry = triangles([a, b, c, d], [0.6; 3])?;
    geometry.extend(triangles(quad(-0.1, -0.1, 0.6, 0.12, 0.4), [0.5; 3])?);
    let scene = TransportScene::new(geometry, Vec::new())
        .ok_or_else(|| "control scene".to_owned())?
        .with_global_lights(vec![sun()])
        .with_chart_sample_density(plan.sample_densities().to_vec())
        .ok_or_else(|| "physical densities".to_owned())?;
    for taps in [2, 3] {
        let solved = scene
            .solve(plan.charts(), options(taps), None)
            .map_err(|failure| failure.name().to_owned())?;
        let mut partial = false;
        for row in 0..usize::try_from(left.height).map_err(|error| error.to_string())? {
            let ai =
                row.saturating_mul(usize::try_from(left.width).map_err(|error| error.to_string())?);
            let left_light = solved.charts[0].texels[ai].light_at([0.0, 1.0, 0.0])[0];
            let right_light = solved.charts[1].texels[row].light_at([0.0, 1.0, 0.0])[0];
            assert!(
                (left_light - right_light).abs() < 1.0e-6,
                "physical diagonal changed direct energy: {left_light} vs {right_light}"
            );
            partial |= left_light > 1.0e-5 && left_light < 1.0 - 1.0e-5;
        }
        assert!(
            partial,
            "control must exercise a partially covered footprint"
        );
    }
    Ok(())
}

#[test]
fn a_thin_caster_between_receiver_centres_is_detected() -> Result<(), String> {
    let corners = quad(0.0, 0.0, 1.0, 1.0, 0.0);
    let mut plan = LightmapPlan::new(LightmapConfig {
        texels_per_metre: 4.0,
        ..LightmapConfig::for_profile(crate::quality::QualityProfile::Full)
    });
    assert!(
        plan.stamp_emitted(&mut [Vertex::UNLIT; 6], 0, PatchKind::Floor, corners, None),
        "control floor must receive an atlas chart"
    );
    let mut geometry = triangles(corners, [0.6; 3])?;
    // All receiver centres miss the 2.5 cm stripe. A chart-neighbour-only
    // detector sees no visibility change anywhere on this surface.
    geometry.extend(triangles(quad(0.085, 0.0, 0.11, 1.0, 0.4), [0.5; 3])?);
    let scene = TransportScene::new(geometry, Vec::new())
        .ok_or_else(|| "control scene".to_owned())?
        .with_global_lights(vec![sun()])
        .with_chart_sample_density(plan.sample_densities().to_vec())
        .ok_or_else(|| "physical densities".to_owned())?;
    let point = scene
        .solve(plan.charts(), options(1), None)
        .map_err(|failure| failure.name().to_owned())?;
    assert!(
        point.charts[0]
            .texels
            .iter()
            .all(|texel| (texel.light_at([0.0, 1.0, 0.0])[0] - 1.0).abs() < 1.0e-6),
        "every receiver centre must miss the thin shadow"
    );
    let covered = scene
        .solve(plan.charts(), options(3), None)
        .map_err(|failure| failure.name().to_owned())?;
    assert!(
        covered.charts[0].texels.iter().any(|texel| {
            let light = texel.light_at([0.0, 1.0, 0.0])[0];
            light > 1.0e-5 && light < 1.0 - 1.0e-5
        }),
        "world-space probes must discover the unsampled thin shadow"
    );
    assert!(
        covered.direct_rays > point.direct_rays,
        "coverage must report its additional traced rays"
    );
    Ok(())
}

fn receiver(scene: &TransportScene, point: [f32; 3]) -> Result<TransportReceiver, String> {
    let (albedo, surface) = scene
        .surface_sample(point, 1.0e-5)
        .ok_or_else(|| "surface at control point".to_owned())?;
    let normal = [0.0, 1.0, 0.0];
    let position = receiver_position(point, normal);
    Ok(TransportReceiver {
        position,
        ray_origin: position,
        normal,
        albedo,
        area: 1.0,
        surface: u32::try_from(surface).map_err(|error| error.to_string())?,
        attenuation: [1.0; 3],
    })
}

#[test]
fn physical_support_cannot_jump_a_real_gap_or_material_boundary() -> Result<(), String> {
    for (gap, other_albedo) in [(0.02, [0.6; 3]), (0.0, [0.9; 3])] {
        let mut geometry = triangles(quad(0.0, 0.0, 1.0, 1.0, 0.0), [0.6; 3])?;
        geometry.extend(triangles(
            quad(1.0 + gap, 0.0, 2.0, 1.0, 0.0),
            other_albedo,
        )?);
        let scene =
            TransportScene::new(geometry, Vec::new()).ok_or_else(|| "boundary scene".to_owned())?;
        let from = [0.9, 0.0, 0.5];
        let clipped = supported_point(
            &scene,
            &receiver(&scene, from)?,
            from,
            [1.1, 0.0, 0.5],
            &mut Vec::new(),
        );
        assert!(
            (clipped[0] - 1.0).abs() < 1.0e-4,
            "tap crossed a real boundary: {clipped:?}"
        );
    }
    Ok(())
}

#[test]
fn diagonal_footprints_remain_inside_real_boundaries_at_multiple_scales() -> Result<(), String> {
    for size in [0.01_f32, 1.0, 1000.0] {
        let scene = TransportScene::new(
            triangles(quad(0.0, 0.0, size, size, 0.0), [0.6; 3])?,
            Vec::new(),
        )
        .ok_or_else(|| "scaled surface".to_owned())?;
        for (from, to) in [
            ([0.9 * size, 0.0, 0.9 * size], [1.1 * size, 0.0, 1.1 * size]),
            (
                [0.1 * size, 0.0, 0.1 * size],
                [-0.1 * size, 0.0, -0.1 * size],
            ),
        ] {
            let clipped =
                supported_point(&scene, &receiver(&scene, from)?, from, to, &mut Vec::new());
            assert!(
                clipped[0] > 0.0 && clipped[0] < size && clipped[2] > 0.0 && clipped[2] < size,
                "diagonal footprint left its physical support at scale {size}: {clipped:?}"
            );
        }
    }
    Ok(())
}

#[test]
fn coplanar_support_crosses_t_junctions_but_keeps_a_hard_fold() -> Result<(), String> {
    let mut geometry = triangles(quad(0.0, 0.0, 1.0, 2.0, 0.0), [0.6; 3])?;
    geometry.extend(triangles(quad(1.0, 0.0, 2.0, 0.7, 0.0), [0.6; 3])?);
    geometry.extend(triangles(quad(1.0, 0.7, 2.0, 2.0, 0.0), [0.6; 3])?);
    let scene = TransportScene::new(geometry, Vec::new()).ok_or_else(|| "T junction".to_owned())?;
    for z in [0.4, 0.7, 1.1] {
        let from = [0.9, 0.0, z];
        let to = [1.1, 0.0, z];
        let supported =
            supported_point(&scene, &receiver(&scene, from)?, from, to, &mut Vec::new());
        assert!(
            length_difference(supported, to) < 1.0e-6,
            "coplanar subdivision lost support"
        );
    }
    let mut folded_geometry = triangles(quad(0.0, 0.0, 1.0, 1.0, 0.0), [0.6; 3])?;
    folded_geometry.extend(triangles(
        [
            [1.0, 0.0, 1.0],
            [2.0, 0.5, 1.0],
            [2.0, 0.5, 0.0],
            [1.0, 0.0, 0.0],
        ],
        [0.6; 3],
    )?);
    let folded_scene =
        TransportScene::new(folded_geometry, Vec::new()).ok_or_else(|| "fold".to_owned())?;
    let from = [0.9, 0.0, 0.5];
    let supported = supported_point(
        &folded_scene,
        &receiver(&folded_scene, from)?,
        from,
        [1.1, 0.0, 0.5],
        &mut Vec::new(),
    );
    assert!(
        (supported[0] - 1.0).abs() < 1.0e-4,
        "footprint crossed the folded normal"
    );
    Ok(())
}

fn length_difference(left: [f32; 3], right: [f32; 3]) -> f32 {
    left.iter()
        .zip(right)
        .map(|(a, b)| (*a - b).powi(2))
        .sum::<f32>()
        .sqrt()
}

#[test]
fn canonical_surface_axes_ignore_triangle_orientation() -> Result<(), String> {
    let a = LightmapPatch::from_quad(
        PatchKind::Prop,
        [
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 2.0],
            [1.0, 0.0, 2.0],
            [1.0, 0.0, 2.0],
        ],
        None,
    )
    .ok_or_else(|| "first triangle".to_owned())?;
    let b = LightmapPatch::from_quad(
        PatchKind::Prop,
        [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 2.0],
            [1.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
        ],
        None,
    )
    .ok_or_else(|| "second triangle".to_owned())?;
    let (au, av) = surface_axes(patch_normal(&a));
    let (bu, bv) = surface_axes(patch_normal(&b));
    assert!(
        length_difference(au, bu) < 1.0e-6 && length_difference(av, bv) < 1.0e-6,
        "same oriented plane must select the same physical axes"
    );
    Ok(())
}

fn bevel_plan([a, b, c, d]: [[f32; 3]; 4]) -> LightmapPlan {
    let mut plan = LightmapPlan::new(LightmapConfig::for_profile(
        crate::quality::QualityProfile::Full,
    ));
    for corners in [[a, b, c], [a, c, d]] {
        let mut vertices = corners.map(|pos| Vertex {
            pos,
            ..Vertex::UNLIT
        });
        assert!(
            plan.stamp_prop_triangle(&mut vertices, 8.0),
            "each bevel triangle must retain its production chart"
        );
    }
    plan
}

#[test]
fn shared_bevel_filter_connections_ignore_chart_insets_and_keep_real_occlusion()
-> Result<(), String> {
    use crate::lighting::transport::{Accumulator, filter::filter_accumulators};

    // The hero sofa's small bevel meets a horizontal solid at its lower edge.
    // Chart-dependent insets put one corner origin above that solid and the
    // other below it, although their actual receiver positions are identical.
    let [a, b, c, d] = [
        [4.48, 0.398, 0.678],
        [4.48, 0.398, 0.842],
        [4.452, 0.37, 0.842],
        [4.452, 0.37, 0.678],
    ];
    let plan = bevel_plan([a, b, c, d]);
    let mut geometry = triangles([a, b, c, d], [0.6; 3])?;
    geometry.extend(triangles(quad(2.542, 0.692, 4.458, 1.508, 0.37), [0.5; 3])?);
    let scene = TransportScene::new(geometry.clone(), Vec::new())
        .ok_or_else(|| "bevel and adjoining solid".to_owned())?
        .with_chart_sample_density(plan.sample_densities().to_vec())
        .ok_or_else(|| "bevel sample density".to_owned())?;
    let receivers = scene
        .receivers(plan.charts())
        .map_err(|failure| failure.name().to_owned())?;
    let left_chart = plan.charts()[0].1;
    let right_chart = plan.charts()[1].1;
    let left_width = usize::try_from(left_chart.width).map_err(|error| error.to_string())?;
    let left_height = usize::try_from(left_chart.height).map_err(|error| error.to_string())?;
    let right_width = usize::try_from(right_chart.width).map_err(|error| error.to_string())?;
    let left_index = left_height.saturating_sub(1).saturating_mul(left_width);
    let right_index = left_height
        .saturating_mul(left_width)
        .saturating_add(right_width.saturating_sub(1));
    let left = receivers
        .get(left_index)
        .ok_or_else(|| "first shared bevel corner".to_owned())?;
    let right = receivers
        .get(right_index)
        .ok_or_else(|| "second shared bevel corner".to_owned())?;
    assert_eq!(
        left.position.map(f32::to_bits),
        right.position.map(f32::to_bits),
        "both charts must represent the identical physical corner"
    );
    let neighbour = receiver_position(
        [c[0], c[1], c[2] - 0.125],
        patch_normal(&plan.charts()[0].0),
    );
    assert!(
        !scene.occluded(left.ray_origin, neighbour)
            && scene.occluded(right.ray_origin, neighbour)
            && !scene.occluded(left.position, neighbour),
        "control must expose the chart-inset visibility disagreement at the real hard edge"
    );
    let indirect = receivers
        .iter()
        .map(|sample| Accumulator {
            surface_light: if sample.position[2] < 0.8 {
                [1.0; 3]
            } else {
                [0.0; 3]
            },
            ..Accumulator::default()
        })
        .collect::<Vec<_>>();
    let direct = vec![Accumulator::default(); receivers.len()];
    let filtered = filter_accumulators(&scene, plan.charts(), &receivers, &indirect, &direct)
        .map_err(|failure| failure.name().to_owned())?;
    let left_light = filtered[left_index].light_at(left.normal)[0];
    let right_light = filtered[right_index].light_at(right.normal)[0];
    assert!(
        left_light > 0.05 && (left_light - right_light).abs() < 1.0e-6,
        "shared physical filter connection changed energy: {left_light} vs {right_light}"
    );

    // A real cross-surface blocker must still prevent the supported stencil
    // connection; continuity cannot replace the existing geometric guard.
    geometry.extend(triangles(
        [
            [4.4, 0.3, 0.8],
            [4.5, 0.3, 0.8],
            [4.5, 0.45, 0.8],
            [4.4, 0.45, 0.8],
        ],
        [0.5; 3],
    )?);
    let blocked = TransportScene::new(geometry, Vec::new())
        .ok_or_else(|| "bevel with actual blocker".to_owned())?
        .with_chart_sample_density(plan.sample_densities().to_vec())
        .ok_or_else(|| "blocked bevel sample density".to_owned())?;
    let occluded = filter_accumulators(&blocked, plan.charts(), &receivers, &indirect, &direct)
        .map_err(|failure| failure.name().to_owned())?;
    for (index, normal) in [(left_index, left.normal), (right_index, right.normal)] {
        assert!(
            occluded[index].light_at(normal)[0].abs() < 1.0e-6,
            "a real blocker must keep the shared corner dark"
        );
    }
    Ok(())
}
