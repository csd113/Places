//! Tests for the offline transport solver.
//!
//! These are the solver's independent oracles: small scenes whose light paths
//! are reasoned about analytically (a sealed wall, a doorway, a single point
//! lobe) rather than compared against the solver's own output. Test code:
//! indexing, panic-based assertions and permissive float comparison are
//! idiomatic here; production lints stay enforced elsewhere.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::cast_precision_loss,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::indexing_slicing,
    clippy::manual_assert_eq,
    clippy::many_single_char_names,
    clippy::panic,
    clippy::suboptimal_flops,
    clippy::too_many_lines,
    clippy::unwrap_used
)]

use super::*;
use crate::lighting::lightmap::{Chart, LightmapPatch, PatchKind};

/// A floor quad split into two triangles, wound so its normal points up
/// (the project convention for floors).
fn floor(x0: f32, z0: f32, x1: f32, z1: f32, albedo: [f32; 3]) -> Vec<TransportTriangle> {
    let y = 0.0;
    let a = [x0, y, z1];
    let b = [x1, y, z1];
    let c = [x1, y, z0];
    let d = [x0, y, z0];
    let mut out = Vec::new();
    for (p0, p1, p2) in [(a, b, c), (a, c, d)] {
        if let Some(triangle) = TransportTriangle::new(p0, p1, p2, albedo) {
            out.push(triangle);
        }
    }
    out
}

/// A vertical quad split into two triangles.
fn wall(
    a: [f32; 3],
    b: [f32; 3],
    c: [f32; 3],
    d: [f32; 3],
    albedo: [f32; 3],
) -> Vec<TransportTriangle> {
    let mut out = Vec::new();
    for (p0, p1, p2) in [(a, b, c), (a, c, d)] {
        if let Some(triangle) = TransportTriangle::new(p0, p1, p2, albedo) {
            out.push(triangle);
        }
    }
    out
}

/// A point emitter with the standard authored defaults.
fn point(position: [f32; 3], intensity: f32) -> TransportEmitter {
    TransportEmitter {
        position,
        shape: EmitterShape::Point,
        color: [1.0, 1.0, 1.0],
        intensity,
        range: 12.0,
        falloff: LightFalloff::Smooth,
        height_factor: 1.0,
        directional: false,
        switchable: None,
    }
}

/// A rectangular emitter with half-extents `half_x` and `half_z`.
fn rect(position: [f32; 3], half_x: f32, half_z: f32, intensity: f32) -> TransportEmitter {
    TransportEmitter {
        position,
        shape: EmitterShape::Rect {
            u: [half_x, 0.0, 0.0],
            v: [0.0, 0.0, half_z],
        },
        color: [1.0, 1.0, 1.0],
        intensity,
        range: 12.0,
        falloff: LightFalloff::Smooth,
        height_factor: 1.0,
        directional: false,
        switchable: None,
    }
}

/// A small solve budget.
fn options(bounces: u8, taps: u8) -> SolveOptions {
    SolveOptions {
        taps_per_axis: taps,
        bounces,
        gather_samples: 32,
        workers: 1,
    }
}

/// One floor patch in the mesh's own winding: `u` runs +X, `v` runs -Z, so
/// `cross(u, v)` is the upward normal.
fn floor_patch(
    x0: f32,
    z0: f32,
    x1: f32,
    z1: f32,
    width: u32,
    height: u32,
) -> (LightmapPatch, Chart) {
    (
        LightmapPatch {
            origin: [x0, 0.0, z1],
            u_axis: [x1 - x0, 0.0, 0.0],
            v_axis: [0.0, 0.0, z0 - z1],
            room: None,
            kind: PatchKind::Floor,
        },
        Chart {
            page: 0,
            x: 0,
            y: 0,
            width,
            height,
        },
    )
}

/// The reconstructed light of the first solved texel at a normal.
fn first_light(solution: &TransportSolution, normal: [f32; 3]) -> [f32; 3] {
    solution.charts[0].texels[0].light_at(normal)
}

#[test]
fn a_sealed_wall_blocks_direct_light_and_an_opening_admits_it() {
    let receiver = [0.0, 0.02, 2.0];
    let light = point([0.0, 2.0, 0.0], 3.0);
    let sealed = wall(
        [-1.0, 0.0, 1.0],
        [1.0, 0.0, 1.0],
        [1.0, 3.0, 1.0],
        [-1.0, 3.0, 1.0],
        [1.0; 3],
    );
    let mut open = wall(
        [-1.0, 0.0, 1.0],
        [-0.5, 0.0, 1.0],
        [-0.5, 3.0, 1.0],
        [-1.0, 3.0, 1.0],
        [1.0; 3],
    );
    open.extend(wall(
        [0.5, 0.0, 1.0],
        [1.0, 0.0, 1.0],
        [1.0, 3.0, 1.0],
        [0.5, 3.0, 1.0],
        [1.0; 3],
    ));

    let sealed_scene = TransportScene::new(
        sealed
            .iter()
            .copied()
            .chain(floor(-2.0, -2.0, 2.0, 4.0, [0.6; 3]))
            .collect(),
        vec![light],
    )
    .expect("scene");
    let open_scene = TransportScene::new(
        open.iter()
            .copied()
            .chain(floor(-2.0, -2.0, 2.0, 4.0, [0.6; 3]))
            .collect(),
        vec![light],
    )
    .expect("scene");

    let (sealed_weight, _) = sealed_scene.emitters()[0].direct(&sealed_scene, receiver, 1);
    let (open_weight, _) = open_scene.emitters()[0].direct(&open_scene, receiver, 1);
    assert_eq!(
        sealed_weight, [0.0; 3],
        "the sealed wall must block the light"
    );
    assert!(
        open_weight[0] > 0.01,
        "the doorway must admit the light: {open_weight:?}"
    );
}

#[test]
fn full_occlusion_reads_zero_in_the_solved_atlas() {
    let light = point([0.0, 2.0, 0.0], 3.0);
    let mut triangles = wall(
        [-1.0, 0.0, 1.0],
        [1.0, 0.0, 1.0],
        [1.0, 3.0, 1.0],
        [-1.0, 3.0, 1.0],
        [1.0; 3],
    );
    triangles.extend(floor(-2.0, -2.0, 2.0, 4.0, [0.6; 3]));
    let scene = TransportScene::new(triangles, vec![light]).expect("scene");
    let patch = floor_patch(0.0, 1.5, 0.1, 1.6, 1, 1);
    let solution = scene.solve(&[patch], options(0, 1), None).expect("solve");
    assert_eq!(
        first_light(&solution, [0.0, 1.0, 0.0]),
        [0.0; 3],
        "a fully occluded texel must solve to darkness"
    );
}

#[test]
fn a_diffuse_bounce_reaches_a_surface_the_light_cannot_see() {
    let (scene, charts) = two_room_scene(0.8);
    // The target corner is behind the solid divider span; the support charts
    // (the lit floors and the back wall the doorway sees) become the VPLs.
    let direct = scene
        .solve(&charts, options(0, 3), None)
        .expect("direct solve");
    assert_eq!(
        first_light(&direct, [0.0, 1.0, 0.0]),
        [0.0; 3],
        "the target corner must be in full shadow for the bounce to be its only light"
    );

    let bounced = scene
        .solve(&charts, options(2, 3), None)
        .expect("bounce solve");
    let light = first_light(&bounced, [0.0, 1.0, 0.0]);
    assert!(
        light[0] > 1.0e-4,
        "a diffuse bounce must reach the shadowed corner: {light:?}"
    );
    assert!(
        bounced.bounce_rays > 0 && bounced.cache_cells > 0,
        "the bounce pass must trace rays against a populated cache: {bounced:?}"
    );
}

#[test]
fn a_sealed_room_stays_dark_for_direct_and_indirect_light() {
    let (scene, charts) = two_room_scene_sealed(0.8);
    let solution = scene.solve(&charts, options(2, 3), None).expect("solve");
    let light = first_light(&solution, [0.0, 1.0, 0.0]);
    assert!(
        light[0] <= 1.0e-6,
        "a sealed wall must block the whole light path: {light:?}"
    );
}

#[test]
fn bounce_strength_follows_the_source_albedo() {
    let (dull, dull_charts) = two_room_scene(0.1);
    let (bright, bright_charts) = two_room_scene(0.85);
    let dull_solution = dull
        .solve(&dull_charts, options(2, 3), None)
        .expect("solve");
    let bright_solution = bright
        .solve(&bright_charts, options(2, 3), None)
        .expect("solve");
    let dull_light = first_light(&dull_solution, [0.0, 1.0, 0.0])[0];
    let bright_light = first_light(&bright_solution, [0.0, 1.0, 0.0])[0];
    assert!(
        bright_light > dull_light * 1.5,
        "a brighter albedo must bounce more light: dull={dull_light}, bright={bright_light}"
    );
}

#[test]
fn an_extended_emitter_produces_a_partial_soft_visibility_transition() {
    // A rectangular panel on the lit side of a thin wall. Receivers behind the
    // wall's z edge see a fraction of the panel's taps: the penumbra.
    let emitter = rect([1.0, 2.0, 0.0], 0.5, 0.5, 4.0);
    let mut triangles = wall(
        [0.0, 0.0, -2.0],
        [0.0, 0.0, 2.0],
        [0.0, 3.0, 2.0],
        [0.0, 3.0, -2.0],
        [0.6; 3],
    );
    triangles.extend(floor(-4.0, -4.0, 4.0, 4.0, [0.6; 3]));
    let scene = TransportScene::new(triangles, vec![emitter]).expect("scene");
    let emitter = &scene.emitters()[0];

    let mut partial = 0usize;
    let mut fully_lit = 0usize;
    let mut fully_blocked = 0usize;
    for step in 0..=15 {
        let z = 1.6 + 0.1 * f32::from(u16::try_from(step).unwrap_or(0));
        let (weight, _) = emitter.direct(&scene, [-0.2, 0.02, z], 3);
        let centre_distance = ((1.2_f32 * 1.2) + (2.0_f32 * 2.0) + (z * z)).sqrt();
        let shape = LightFalloff::Smooth.factor(centre_distance / 12.0);
        let reference = crate::lighting::LOCAL_LIGHT_STRENGTH * 4.0 * shape;
        if weight[0] <= 1.0e-6 {
            fully_blocked += 1;
        } else if weight[0] >= reference * 0.99 {
            fully_lit += 1;
        } else {
            partial += 1;
        }
    }
    assert!(fully_lit > 0, "some samples must be fully exposed");
    assert!(fully_blocked > 0, "some samples must be fully shadowed");
    assert!(
        partial >= 3,
        "the panel edge must produce a penumbra: {partial} partial samples"
    );
}

#[test]
fn directional_lightmaps_respond_to_the_normal_without_double_energy() {
    let light = point([0.0, 3.0, 0.0], 2.0);
    let scene = TransportScene::new(Vec::new(), vec![light]).expect("scene");
    let patch = floor_patch(-0.1, -0.1, 0.1, 0.1, 1, 1);
    let solution = scene.solve(&[patch], options(0, 1), None).expect("solve");
    let texel = solution.charts[0].texels[0];
    let up = texel.light_at([0.0, 1.0, 0.0])[0];
    let down = texel.light_at([0.0, -1.0, 0.0])[0];
    let sideways = texel.light_at([1.0, 0.0, 0.0])[0];
    assert!(up > 0.1, "the receiver must see the light: {up}");
    assert!(
        down <= 1.0e-6,
        "a receiver facing away must be dark: {down}"
    );
    assert!(
        sideways <= 1.0e-6,
        "the equator lies on the dominant lobe's terminator: {sideways}"
    );
    // The stored isotropic term is the exact mean of the reconstructed field:
    // the directional factor integrates to zero over the sphere, so a texel
    // can never gain energy from its dominant direction.
    assert!(
        (texel.irradiance[0] - up * 0.5).abs() < up * 1.0e-3,
        "the isotropic term must be the field mean: A={} up={up}",
        texel.irradiance[0]
    );
    assert!(
        (texel.direction[0] - up * 0.5).abs() < up * 1.0e-3,
        "the dominant amplitude must carry the half-lobe: D={} up={up}",
        texel.direction[0]
    );
    // The stored axis must round-trip the incoming direction.
    let axis = crate::lighting::lightmap::oct_decode(texel.axis);
    assert!(
        axis[1] > 0.99,
        "the dominant axis must point at the light: {axis:?}"
    );
}

#[test]
fn a_parallel_solve_is_identical_to_the_serial_solve() {
    let (scene, mut charts) = two_room_scene(0.7);
    charts.push(floor_patch(0.5, 4.5, 1.5, 5.5, 4, 4));
    charts.push(floor_patch(2.0, 6.0, 3.0, 7.0, 3, 5));
    let serial = scene.solve(&charts, options(2, 2), None).expect("serial");
    let parallel = scene
        .solve(
            &charts,
            SolveOptions {
                workers: 4,
                ..options(2, 2)
            },
            None,
        )
        .expect("parallel");
    assert_eq!(serial.charts.len(), parallel.charts.len());
    for (serial_chart, parallel_chart) in serial.charts.iter().zip(&parallel.charts) {
        assert_eq!(serial_chart.texels, parallel_chart.texels);
    }
    assert_eq!(serial.switchable, parallel.switchable);
}

#[test]
fn a_cancelled_solve_reports_a_fill_failure() {
    let (scene, charts) = two_room_scene(0.7);
    let cancel = AtomicBool::new(true);
    let outcome = scene.solve(&charts, options(2, 2), Some(&cancel));
    assert_eq!(outcome.err(), Some(LightmapFailure::FillSize));
}

#[test]
fn the_bvh_agrees_with_a_linear_scan() {
    let mut triangles = floor(-2.0, -2.0, 2.0, 2.0, [0.6; 3]);
    triangles.extend(wall(
        [-2.0, 0.0, 0.0],
        [-0.6, 0.0, 0.0],
        [-0.6, 3.0, 0.0],
        [-2.0, 3.0, 0.0],
        [0.6; 3],
    ));
    triangles.extend(wall(
        [0.6, 0.0, 0.0],
        [2.0, 0.0, 0.0],
        [2.0, 3.0, 0.0],
        [0.6, 3.0, 0.0],
        [0.6; 3],
    ));
    let scene = TransportScene::new(triangles, Vec::new()).expect("scene");
    for from_step in 0..7_u32 {
        for to_step in 0..7_u32 {
            let from = [
                -1.8 + f32::from(u16::try_from(from_step).unwrap_or(0)) * 0.6,
                0.4,
                -1.8 + f32::from(u16::try_from(to_step).unwrap_or(0)) * 0.6,
            ];
            let to = [
                1.8 - f32::from(u16::try_from(to_step).unwrap_or(0)) * 0.5,
                1.6,
                1.8 - f32::from(u16::try_from(from_step).unwrap_or(0)) * 0.5,
            ];
            let direction = sub(to, from);
            let distance = length(direction);
            let unit = scale(direction, 1.0 / distance);
            let max_t = distance - RAY_EPS_M;
            let accelerated = scene.any_hit(from, unit, max_t);
            let linear = scene.linear_any_hit(from, unit, max_t);
            assert_eq!(
                accelerated, linear,
                "ray {from:?} -> {to:?} disagreed with the linear scan"
            );
        }
    }
}

#[test]
fn the_emitter_shapes_sample_their_own_extents() {
    let point = EmitterShape::Point;
    assert_eq!(point.samples(3), vec![[0.0; 3]]);
    let rectangle = EmitterShape::Rect {
        u: [1.0, 0.0, 0.0],
        v: [0.0, 0.0, 2.0],
    };
    let samples = rectangle.samples(3);
    assert_eq!(samples.len(), 9);
    for sample in &samples {
        assert!(sample[0].abs() <= 1.0);
        assert!(sample[2].abs() <= 2.0);
    }
    let line = EmitterShape::Line {
        direction: [1.0, 0.0, 0.0],
        half_length: 2.0,
    };
    let samples = line.samples(3);
    assert_eq!(samples.len(), 3);
    assert_eq!(samples[0], [-2.0, 0.0, 0.0]);
    assert_eq!(samples[1], [0.0; 3]);
    assert_eq!(samples[2], [2.0, 0.0, 0.0]);
}

#[test]
fn the_tone_map_preserves_the_calibrated_range_and_compresses_highlights() {
    assert_eq!(soft_clip_channel(0.0), 0.0);
    assert!((soft_clip_channel(0.8) - 0.8).abs() < 1.0e-6);
    assert!((soft_clip_channel(0.4) - 0.4).abs() < 1.0e-6);
    let bright = soft_clip_channel(4.0);
    assert!(
        (0.8..=1.0).contains(&bright),
        "highlights compress: {bright}"
    );
    assert!(
        soft_clip_channel(1.0) > soft_clip_channel(0.9),
        "the tone map must be monotonic"
    );
    assert!(soft_clip_channel(f32::NAN) == 0.0);
    assert!(soft_clip_channel(f32::INFINITY) == 0.0);
}

#[test]
fn the_solver_fingerprint_is_stable_and_distinct_from_the_lighting_model() {
    let first = solver_fingerprint();
    assert_eq!(
        first,
        solver_fingerprint(),
        "the fingerprint must be stable"
    );
    assert_ne!(first, 0);
    assert_ne!(first, crate::lighting::model_fingerprint());
}

/// Two rooms joined by a doorway in a divider, with one light in room A.
///
/// Room A is `x in [0, 4], z in [0, 4]`, room B `x in [0, 4], z in [4, 8]`,
/// the divider at `z = 4` has a door `x in [1.5, 2.5], y in [0, 2.2]`, and the
/// light sits at `(1, 2.5, 1)`.
///
/// The returned chart list is the solved set: `charts[0]` is the far corner of
/// room B, out of the light's direct sight, and the rest are the support
/// surfaces the doorway actually lights (both floors and the back wall), which
/// is what makes them the solver's VPLs. A production solve always supplies the
/// whole mapped world this way.
fn two_room_scene(albedo: f32) -> (TransportScene, Vec<(LightmapPatch, Chart)>) {
    two_room_scene_with(albedo, true)
}

/// [`two_room_scene`] with the doorway sealed shut.
fn two_room_scene_sealed(albedo: f32) -> (TransportScene, Vec<(LightmapPatch, Chart)>) {
    two_room_scene_with(albedo, false)
}

fn two_room_scene_with(albedo: f32, open: bool) -> (TransportScene, Vec<(LightmapPatch, Chart)>) {
    let material = [albedo; 3];
    let mut triangles = floor(0.0, 0.0, 4.0, 4.0, material);
    triangles.extend(floor(0.0, 4.0, 4.0, 8.0, material));
    triangles.extend(wall(
        [0.0, 0.0, 8.0],
        [4.0, 0.0, 8.0],
        [4.0, 3.0, 8.0],
        [0.0, 3.0, 8.0],
        material,
    ));
    if open {
        triangles.extend(wall(
            [0.0, 0.0, 4.0],
            [1.0, 0.0, 4.0],
            [1.0, 3.0, 4.0],
            [0.0, 3.0, 4.0],
            material,
        ));
        triangles.extend(wall(
            [1.8, 0.0, 4.0],
            [4.0, 0.0, 4.0],
            [4.0, 3.0, 4.0],
            [1.8, 3.0, 4.0],
            material,
        ));
        triangles.extend(wall(
            [1.0, 2.2, 4.0],
            [1.8, 2.2, 4.0],
            [1.8, 3.0, 4.0],
            [1.0, 3.0, 4.0],
            material,
        ));
    } else {
        triangles.extend(wall(
            [0.0, 0.0, 4.0],
            [4.0, 0.0, 4.0],
            [4.0, 3.0, 4.0],
            [0.0, 3.0, 4.0],
            material,
        ));
    }
    let scene = TransportScene::new(triangles, vec![point([1.0, 2.5, 1.0], 4.0)]).expect("scene");
    // The receiver set is the whole mapped world in production; this scene's
    // support charts are the surfaces the doorway lights and the surfaces the
    // corner can see, so the bounce cache is populated where the corner's rays
    // land.
    let corner = floor_patch(3.6, 7.2, 4.0, 8.0, 3, 3);
    let support_floor_a = floor_patch(0.0, 0.0, 4.0, 4.0, 4, 4);
    let support_floor_b = floor_patch(1.0, 4.0, 3.0, 6.0, 4, 4);
    // A vertical chart whose `u` runs +X and `v` runs +Y has `+Z` as its
    // normal; swapping them gives `-Z`.
    let vertical =
        |origin: [f32; 3], width: f32, height: f32, width_texels: u32, height_texels: u32| {
            (
                LightmapPatch {
                    origin,
                    u_axis: [width, 0.0, 0.0],
                    v_axis: [0.0, height, 0.0],
                    room: None,
                    kind: PatchKind::Wall,
                },
                Chart {
                    page: 0,
                    x: 0,
                    y: 0,
                    width: width_texels,
                    height: height_texels,
                },
            )
        };
    let support_back_wall = vertical([1.8, 0.2, 8.0], 2.2, 2.6, 4, 4);
    let support_divider_left = vertical([0.0, 0.0, 4.0], 1.0, 3.0, 2, 3);
    let support_divider_right = vertical([1.8, 0.0, 4.0], 2.2, 3.0, 4, 3);
    (
        scene,
        vec![
            corner,
            support_floor_a,
            support_floor_b,
            support_back_wall,
            support_divider_left,
            support_divider_right,
        ],
    )
}

/// The prepared moving-object field excludes switchable fixtures in every
/// state: a switchable fixture's contribution exists only as its own atlas
/// layer pair. This pins the documented limitation (`docs/RENDERER.md` §7.1.3
/// and `docs/PACKAGE_FORMAT.md` §6.1); when per-state field layers land, this
/// test and that documentation must change together.
#[test]
fn switchable_fixtures_are_excluded_from_the_moving_object_field() {
    let material = [0.7; 3];
    let charts = vec![floor_patch(0.0, 0.0, 4.0, 4.0, 4, 4)];
    let plain = point([1.0, 2.5, 1.0], 4.0);
    let mut switchable = point([3.0, 2.5, 3.0], 4.0);
    switchable.switchable = Some(0);
    let with_switchable =
        TransportScene::new(floor(0.0, 0.0, 4.0, 4.0, material), vec![plain, switchable])
            .expect("scene");
    let plain_only = TransportScene::new(floor(0.0, 0.0, 4.0, 4.0, material), vec![plain])
        .expect("reference scene");
    let solved = with_switchable
        .solve_with_probes(&charts, options(0, 1), None, true)
        .expect("solve with a switchable fixture");
    let reference = plain_only
        .solve_with_probes(&charts, options(0, 1), None, true)
        .expect("reference solve");
    assert_eq!(
        solved.solution.switchable.len(),
        1,
        "the switchable fixture keeps its own atlas pass"
    );
    assert_eq!(
        solved.solution.charts, reference.solution.charts,
        "the base atlas must not contain the switchable fixture's light"
    );
    let mut field = solved.probes.expect("solved field");
    let mut reference_field = reference.probes.expect("reference field");
    // The compiler labels rooms before packaging; sampling needs valid rooms.
    field.assign_rooms(|_| Some(0));
    reference_field.assign_rooms(|_| Some(0));
    let mut sampled = 0;
    for position in [
        [1.0, 0.1, 1.0],
        [2.0, 0.1, 2.0],
        [3.0, 0.1, 3.0],
        [2.0, 1.0, 2.0],
    ] {
        let (Some(value), Some(reference_value)) = (
            field.sample(position, None),
            reference_field.sample(position, None),
        ) else {
            continue;
        };
        sampled += 1;
        assert!(
            reference_value.irradiance.iter().sum::<f32>() > 0.0,
            "the plain emitter must light the field at {position:?}"
        );
        for (actual, expected) in value.irradiance.iter().zip(&reference_value.irradiance) {
            assert!(
                (actual - expected).abs() < 1e-6,
                "the switchable fixture must not reach the field at {position:?}: \
                 {value:?} vs {reference_value:?}"
            );
        }
    }
    assert!(sampled > 0, "at least one probe position must resolve");
}
