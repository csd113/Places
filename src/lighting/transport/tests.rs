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
use crate::level::LevelDef;
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
            diagonal_correction: [0.0; 3],
            triangle: false,
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
fn a_nonzero_sky_ambient_lights_an_open_receiver_and_zero_is_unchanged() {
    // No emitters at all: every contribution below is the environment term,
    // which must work in a level whose only light is its sky.
    let triangles = floor(-2.0, -2.0, 2.0, 2.0, [0.6; 3]);
    let dark = TransportScene::new(triangles.clone(), Vec::new())
        .expect("scene")
        .solve(
            &[floor_patch(-2.0, -2.0, 2.0, 2.0, 1, 1)],
            options(1, 1),
            None,
        )
        .expect("solve");
    let explicit_zero = TransportScene::new(triangles.clone(), Vec::new())
        .expect("scene")
        .with_sky([0.0; 3])
        .solve(
            &[floor_patch(-2.0, -2.0, 2.0, 2.0, 1, 1)],
            options(1, 1),
            None,
        )
        .expect("solve");
    let with_sky = TransportScene::new(triangles, Vec::new())
        .expect("scene")
        .with_sky([0.5, 0.5, 0.5])
        .solve(
            &[floor_patch(-2.0, -2.0, 2.0, 2.0, 1, 1)],
            options(1, 1),
            None,
        )
        .expect("solve");
    assert_eq!(
        first_light(&dark, [0.0, 1.0, 0.0]),
        [0.0; 3],
        "without a sky the solve has no environment term"
    );
    assert_eq!(
        first_light(&dark, [0.0, 1.0, 0.0]),
        first_light(&explicit_zero, [0.0, 1.0, 0.0]),
        "an explicit zero sky is the default solve, bit for bit"
    );
    let lit = first_light(&with_sky, [0.0, 1.0, 0.0]);
    assert!(
        lit.iter().all(|channel| *channel > 0.001),
        "the sky ambient must reach an upward receiver: {lit:?}"
    );
    // A downward-facing receiver sees almost none of the sky dome: the
    // hemisphere integral keeps its direction, unlike an isotropic ambient
    // lift. (The reconstruction is analytic, so a small residual is expected.)
    let down = first_light(&with_sky, [0.0, -1.0, 0.0]);
    assert!(
        down[0] < lit[0] * 0.4,
        "the sky must stay overwhelmingly upward-facing: up {lit:?}, down {down:?}"
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

/// A sealed wall blocks every direct path: the corner behind it solves to
/// exactly zero. The bounce path is no longer exactly zero under the frozen
/// linear moment reconstruction: a two-sided divider's receivers also carry the
/// light their front face "sees through itself" (the emitter test is a
/// visibility test, not a normal test), and the first-order reconstruction
/// returns `irradiance * (1 + dot(n, g))` for a hit whose normal faces away
/// instead of the old hard terminator's zero. The residual must stay a small
/// bounded fraction of a directly lit texel, so this regression still fails
/// loudly if the visibility solve itself breaks.
#[test]
fn a_sealed_room_blocks_every_direct_path_and_almost_the_whole_bounce_path() {
    let (scene, charts) = two_room_scene_sealed(0.8);
    let direct = scene.solve(&charts, options(0, 3), None).expect("solve");
    assert_eq!(
        first_light(&direct, [0.0, 1.0, 0.0]),
        [0.0; 3],
        "a sealed wall must block every direct path exactly"
    );
    let solution = scene.solve(&charts, options(2, 3), None).expect("solve");
    let light = first_light(&solution, [0.0, 1.0, 0.0]);
    let lit = solution.charts[1].texels[0].light_at([0.0, 1.0, 0.0])[0];
    assert!(
        lit > 0.5,
        "the lit side of the scene must solve bright: {lit}"
    );
    assert!(
        light[0] <= lit * 0.05,
        "a sealed wall must block almost the whole light path: {light:?} vs lit {lit}"
    );
    assert!(
        light[0] >= 0.0 && light[0].is_finite(),
        "the bounded bounce residual must stay finite and non-negative: {light:?}"
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

/// The moment reconstruction, pinned on a real one-light solve: the stored mean
/// is the field mean, the stored moment points at the light, and `light_at(n)`
/// follows the calibrated sharp cosine `2 * I * max(0, dot(n, omega))` — the
/// peak is `2 * I`, the half-angle is half the peak, a receiver facing away is
/// dark, and the sphere mean is exactly `I`, so a texel can never gain energy
/// from its moment.
#[test]
fn the_moment_reconstruction_is_exact_for_a_single_light() {
    let light = point([0.0, 3.0, 0.0], 2.0);
    let scene = TransportScene::new(Vec::new(), vec![light]).expect("scene");
    let patch = floor_patch(-0.1, -0.1, 0.1, 0.1, 1, 1);
    let solution = scene.solve(&[patch], options(0, 1), None).expect("solve");
    let texel = solution.charts[0].texels[0];
    let omega = [0.0, 1.0, 0.0];
    let up = texel.light_at(omega)[0];
    let down = texel.light_at([0.0, -1.0, 0.0])[0];
    let sideways = texel.light_at([1.0, 0.0, 0.0])[0];
    let half_angle = texel.light_at([0.0, 0.5, 0.866_025_4])[0];
    assert!(up > 0.1, "the receiver must see the light: {up}");
    assert!(
        down <= 1.0e-6,
        "a receiver facing away must be dark: {down}"
    );
    assert!(
        sideways <= 1.0e-6,
        "the equator of a single light carries no light: sideways={sideways}"
    );
    assert!(
        (half_angle - up * 0.5).abs() < up * 1.0e-3,
        "the cosine half (60 degrees off the light) is exactly half the peak: half={half_angle} up={up}"
    );
    // The stored isotropic term is the exact mean of the reconstructed field:
    // the sharp cosine integrates to `I` over the sphere, so a texel can never
    // gain energy from its moment.
    assert!(
        (texel.irradiance[0] - up * 0.5).abs() < up * 1.0e-3,
        "the isotropic term must be the field mean: I={} up={up}",
        texel.irradiance[0]
    );
    // One shared direction: the moment vector is parallel to the light and
    // `dot(g, omega) == sum_c irradiance_c` exactly.
    let k: f32 = texel.irradiance.iter().sum();
    let g_dot = dot(texel.direction, omega);
    assert!(
        (g_dot - k).abs() < up * 1.0e-3,
        "the moment must point at the light: g.omega={g_dot} k={k}"
    );
    assert!(
        texel.direction[1] > 0.0,
        "the moment vector is signed and must point up: {:?}",
        texel.direction
    );
    assert_eq!(texel.axis, [0.5, 0.5], "the axis is reserved");
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
    assert_eq!(
        SOLVER_REVISION, 10,
        "native quad and folded-triangle receiver mapping is solver revision 10"
    );
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
                    diagonal_correction: [0.0; 3],
                    triangle: false,
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

// ------------------------------------------------- water and translucency

/// A horizontal quad at `y` spanning `[-half, half]²`, wound with an upward
/// normal, optionally transmissive.
fn overhead_quad(
    y: f32,
    half: f32,
    albedo: [f32; 3],
    transmissive: bool,
) -> Vec<TransportTriangle> {
    let mut out = wall(
        [-half, y, half],
        [half, y, half],
        [half, y, -half],
        [-half, y, -half],
        albedo,
    );
    if transmissive {
        for triangle in &mut out {
            *triangle = triangle.with_transmissive(true);
        }
    }
    out
}

/// One water body over the test pool, with the solver's calibrated extinction.
fn test_water_body(surface_y: f32, bottom_y: f32) -> TransportWaterBody {
    TransportWaterBody {
        x0: -2.0,
        x1: 2.0,
        z0: -2.0,
        z1: 2.0,
        surface_y,
        bottom_y,
        extinction: crate::lighting::tuning::WATER_EXTINCTION_PER_M,
    }
}

/// A water surface transmits direct light, attenuates it by the vertical
/// submerged path per channel with red absorbed most, and an identical opaque
/// barrier at the same plane still blocks.
#[test]
fn a_water_surface_transmits_attenuates_and_tints_while_an_opaque_barrier_blocks() {
    let light = point([0.0, 2.0, 0.0], 3.0);
    let ground = floor(-2.0, -2.0, 2.0, 2.0, [0.6; 3]);
    let patch = floor_patch(-0.1, -0.1, 0.1, 0.1, 1, 1);
    let charts = [patch];

    let open = TransportScene::new(ground.clone(), vec![light]).expect("open scene");
    let water_scene = TransportScene::new(
        ground
            .iter()
            .copied()
            .chain(overhead_quad(1.0, 1.0, [1.0; 3], true))
            .collect(),
        vec![light],
    )
    .expect("water scene")
    .with_water(vec![test_water_body(1.0, -1.0)]);
    let opaque_scene = TransportScene::new(
        ground
            .iter()
            .copied()
            .chain(overhead_quad(1.0, 1.0, [1.0; 3], false))
            .collect(),
        vec![light],
    )
    .expect("opaque scene");

    let solve = |scene: &TransportScene| {
        scene
            .solve(&charts, options(0, 1), None)
            .expect("solve")
            .charts[0]
            .texels[0]
            .light_at([0.0, 1.0, 0.0])
    };
    let open_light = solve(&open);
    let water_light = solve(&water_scene);
    let opaque_light = solve(&opaque_scene);

    assert!(
        open_light[0] > 0.05,
        "the open scene must be lit: {open_light:?}"
    );
    assert!(
        water_light.iter().all(|value| *value > 0.0),
        "water must transmit direct light: {water_light:?}"
    );
    assert_eq!(
        opaque_light, [0.0; 3],
        "an opaque barrier at the waterline must block"
    );
    // The submerged receiver's value is the open value times one bounded
    // Beer-Lambert step over the vertical path below the surface.
    let depth = 1.0 - SURFACE_OFFSET_M;
    for channel in 0..3 {
        let sigma = crate::lighting::tuning::WATER_EXTINCTION_PER_M[channel];
        let expected = open_light[channel] * (-sigma * depth).exp();
        assert!(
            (water_light[channel] - expected).abs() <= expected.abs().max(1.0e-4) * 1.0e-3,
            "channel {channel}: water {water_light:?} vs expected {expected} (open {open_light:?})"
        );
    }
    assert!(
        water_light[0] < water_light[2],
        "red is absorbed most, so water tints blue: {water_light:?}"
    );
}

/// A transmissive triangle never answers `occluded` or `intersect`: a bounce
/// ray passes straight through and reaches the real surface behind, so water
/// contributes no bounce albedo.
#[test]
fn a_transmissive_triangle_is_skipped_by_visibility_and_by_the_nearest_hit() {
    let mut triangles = floor(-2.0, -2.0, 2.0, 2.0, [0.6; 3]);
    triangles.extend(overhead_quad(1.0, 1.0, [1.0; 3], true));
    let scene = TransportScene::new(triangles, Vec::new()).expect("scene");
    assert!(
        !scene.occluded([0.0, 0.0, 0.0], [0.0, 2.0, 0.0]),
        "the pane must not shadow the segment through it"
    );
    let (distance, index) = scene
        .intersect([0.0, 2.0, 0.0], [0.0, -1.0, 0.0])
        .expect("the floor behind the pane answers the ray");
    assert!(
        distance > 1.0,
        "the pane must not be the nearest hit: t={distance}"
    );
    let hit = scene.triangles[index];
    assert!(
        !hit.transmissive,
        "a transmissive pane was returned: {hit:?}"
    );
    assert!(
        (hit.normal[1] - 1.0).abs() < 1.0e-6,
        "the nearest opaque hit must be the floor: {hit:?}"
    );
}

// --------------------------------------------------- authored baseline fill

/// Two rooms of one level: a small lit room and a separate fixture-free room.
/// The lit room's baseline is above ambient, the empty room's exactly ambient.
fn fill_test_level() -> LevelDef {
    LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "transport_fill_test",
            "name": "Transport Fill Test",
            "spawn": { "x": 1.0, "z": 1.0 },
            "rooms": [
                { "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0, "height": 3.0 },
                { "x": 8.0, "z": 0.0, "width": 4.0, "depth": 4.0, "height": 3.0 }
            ],
            "ceiling_lights": [
                { "fixture": "core:fluorescent_panel_01", "x": 0.5, "z": 2.0, "brightness": 1.0 }
            ]
        }"#,
    )
    .expect("fill test level parses")
}

/// The whole floor of the lit room and of the fixture-free room, one chart
/// each, several texels per chart.
fn fill_test_charts() -> Vec<(LightmapPatch, Chart)> {
    vec![
        floor_patch(0.0, 0.0, 4.0, 4.0, 4, 4),
        floor_patch(8.0, 0.0, 12.0, 4.0, 4, 4),
    ]
}

/// The mean reconstructed light of one chart at a normal.
fn chart_mean_light(
    solution: &TransportSolution,
    chart_index: usize,
    normal: [f32; 3],
) -> [f32; 3] {
    let Some(chart) = solution.charts.get(chart_index) else {
        return [0.0; 3];
    };
    let mut sum = [0.0_f32; 3];
    for texel in &chart.texels {
        let light = texel.light_at(normal);
        for channel in 0..3 {
            sum[channel] += light[channel];
        }
    }
    let divisor = f32::from(u16::try_from(chart.texels.len().max(1)).unwrap_or(u16::MAX));
    [sum[0] / divisor, sum[1] / divisor, sum[2] / divisor]
}

/// The prepared base solve lifts each chart to the authored room fill: the
/// chart mean ends at or above its mean target even when the direct path is
/// blocked, and a fixture-free room's texels stay exactly zero. No global
/// ambient is introduced.
#[test]
fn the_chart_fill_restores_the_authored_baseline_and_keeps_dark_rooms_dark() {
    let level = fill_test_level();
    let lighting = LevelLighting::bake(&level);
    let charts = fill_test_charts();
    let target = receiver_targets(&lighting, &charts);
    assert_eq!(target.len(), 32, "4x4 texels per chart, two charts");
    let lit_target = target[0];
    let dark_target = target[16];
    assert!(
        lit_target.iter().all(|value| *value > 0.0),
        "the lit room needs a non-zero target: {lit_target:?}"
    );
    assert_eq!(
        dark_target, [0.0; 3],
        "a fixture-free room's baseline is exactly ambient, so its target is zero"
    );
    // The target is the authored baseline less the ambient floor, compared
    // straight against the baked model.
    let baseline = lighting.baseline_in_room(0, 2.0, 2.0).to_array();
    for channel in 0..3 {
        let expected = (baseline[channel] - AMBIENT_LEVEL).max(0.0);
        assert!(
            (lit_target[channel] - expected).abs() < 1.0e-6,
            "channel {channel}: target {} vs baseline-ambient {expected}",
            lit_target[channel]
        );
    }

    // Case 1: no geometry and no emitters. The fill itself is the only light:
    // the lit chart lands exactly on its target, the empty chart on zero.
    let empty = TransportScene::new(Vec::new(), Vec::new())
        .expect("scene")
        .with_receiver_target(target.clone());
    let solved = empty.solve(&charts, options(0, 1), None).expect("solve");
    let normal = [0.0, 1.0, 0.0];
    let lit_mean = chart_mean_light(&solved, 0, normal);
    let dark_mean = chart_mean_light(&solved, 1, normal);
    for channel in 0..3 {
        assert!(
            (lit_mean[channel] - lit_target[channel]).abs() < 1.0e-6,
            "channel {channel}: the fill must land on the target mean: {lit_mean:?} vs {lit_target:?}"
        );
    }
    assert_eq!(
        dark_mean, [0.0; 3],
        "no blanket ambient in a fixture-free room"
    );

    // Case 2: the real fixture, with a full-height wall across the lit room.
    // Recovery fill is visibility gated: the sealed side stays dark and the
    // visible side keeps its physical pool rather than restoring a room mean.
    let blocker = wall(
        [2.0, 0.0, 0.0],
        [2.0, 0.0, 4.0],
        [2.0, 3.0, 4.0],
        [2.0, 3.0, 0.0],
        [0.8; 3],
    );
    let emitter = TransportEmitter::from_baked(&lighting.lights()[0], None);
    let scene = TransportScene::new(blocker, vec![emitter])
        .expect("scene")
        .with_receiver_target(target);
    let solved = scene.solve(&charts, options(0, 1), None).expect("solve");
    let lit_mean = chart_mean_light(&solved, 0, normal);
    for channel in 0..3 {
        assert!(
            lit_mean[channel] > 0.0 && lit_mean[channel] < lit_target[channel],
            "channel {channel}: a sealed area must not be lifted to a room-wide target: \
             {lit_mean:?} vs {lit_target:?}"
        );
    }
    // The chart's own structure survives: the texel under the fixture beats
    // the blocked corner, which is exactly what a per-texel deficit lost.
    let chart = &solved.charts[0];
    let mut near = 0.0_f32;
    let mut far = 0.0_f32;
    for (receiver, texel) in chart.receivers.iter().zip(&chart.texels) {
        let value = texel.light_at(normal)[0];
        if receiver.position[0] < 1.0 {
            near = near.max(value);
        } else if receiver.position[0] > 3.0 {
            far = far.max(value);
        }
    }
    assert!(
        near > far + 1.0e-3,
        "the chart fill must preserve the pool contrast: near {near} far {far}"
    );
}

/// A chart the physical solve already lights above its target mean is
/// untouched by the chart fill: the fill only ever lifts, never adds.
#[test]
fn a_chart_brighter_than_its_target_is_untouched() {
    let light = point([0.0, 2.0, 0.0], 3.0);
    let patch = floor_patch(-0.1, -0.1, 0.1, 0.1, 1, 1);
    let charts = [patch];
    let plain =
        TransportScene::new(floor(-2.0, -2.0, 2.0, 2.0, [0.6; 3]), vec![light]).expect("scene");
    let reference = plain
        .solve(&charts, options(0, 1), None)
        .expect("solve")
        .charts[0]
        .texels[0];
    let target = [0.02; 3];
    assert!(
        reference.light_at([0.0, 1.0, 0.0])[0] > target[0],
        "the setup needs direct light above the target"
    );
    let filled = TransportScene::new(floor(-2.0, -2.0, 2.0, 2.0, [0.6; 3]), vec![light])
        .expect("scene")
        .with_receiver_target(vec![target]);
    let solved = filled
        .solve(&charts, options(0, 1), None)
        .expect("solve")
        .charts[0]
        .texels[0];
    assert_eq!(
        solved.irradiance, reference.irradiance,
        "a zero fill must not change the stored irradiance"
    );
    assert_eq!(
        solved.direction, reference.direction,
        "a zero fill must not change the moment"
    );
}

/// The authored target is attenuated by the same water factors as the direct
/// and bounce terms, so a fill-dominated submerged chart mean lands exactly on
/// the target times `exp(-sigma * depth)` and never on the undimmed target.
#[test]
fn a_water_target_is_attenuated_with_the_direct_light() {
    // No emitters: the chart fill is the only contribution, which pins the
    // target path on its own.
    let charts = [floor_patch(-0.1, -0.1, 0.1, 0.1, 1, 1)];
    let target = [0.4, 0.4, 0.4];
    let open = TransportScene::new(Vec::new(), Vec::new())
        .expect("scene")
        .with_receiver_target(vec![target]);
    let water = TransportScene::new(Vec::new(), Vec::new())
        .expect("scene")
        .with_water(vec![test_water_body(1.0, -1.0)])
        .with_receiver_target(vec![target]);
    let solve = |scene: &TransportScene| {
        scene
            .solve(&charts, options(0, 1), None)
            .expect("solve")
            .charts[0]
            .texels[0]
            .light_at([0.0, 1.0, 0.0])
    };
    let open_light = solve(&open);
    let water_light = solve(&water);
    assert!(
        (open_light[0] - target[0]).abs() < 1.0e-6,
        "the open fill lands on the target: {open_light:?}"
    );
    let depth = 1.0 - SURFACE_OFFSET_M;
    for channel in 0..3 {
        let sigma = crate::lighting::tuning::WATER_EXTINCTION_PER_M[channel];
        let expected = open_light[channel] * (-sigma * depth).exp();
        assert!(
            (water_light[channel] - expected).abs() < 1.0e-6,
            "channel {channel}: attenuated target {water_light:?} vs {expected}"
        );
    }
    assert!(
        water_light.iter().all(|value| *value < target[0]),
        "the submerged fill must be dimmer than the target: {water_light:?}"
    );
    assert!(
        water_light[0] < water_light[2],
        "the attenuated target keeps the water tint: {water_light:?}"
    );
}

/// The moving-object field gets the same authored fill as the static atlas,
/// with the same continuous response at each probe, so a probe in a lit room
/// never reads darker than its baseline and a fixture-free room stays dark.
#[test]
fn the_chart_fill_reaches_the_probe_field_with_the_same_target() {
    let level = fill_test_level();
    let lighting = LevelLighting::bake(&level);
    // Whole-room floor charts, so the probe lattice the receivers imply has
    // probes inside both rooms (and in the air between them).
    let charts = vec![
        floor_patch(0.0, 0.0, 4.0, 4.0, 4, 4),
        floor_patch(8.0, 0.0, 12.0, 4.0, 4, 4),
    ];
    let target = receiver_targets(&lighting, &charts);
    let lit_target = target[0];
    assert!(
        lit_target.iter().all(|value| *value > 0.0),
        "the lit room needs a non-zero target: {lit_target:?}"
    );
    let probes = probe_targets(&lighting, &charts);
    assert!(
        probes.iter().any(|probe| probe.target[0] > 0.0),
        "the lit room's lattice needs a non-zero target: {probes:?}"
    );
    let scene = TransportScene::new(Vec::new(), Vec::new())
        .expect("scene")
        .with_receiver_target(target)
        .with_probe_target(probes);
    let solved = scene
        .solve_with_probes(&charts, options(0, 1), None, true)
        .expect("solve");
    let mut field = solved.probes.expect("solved field");
    // The compiler labels rooms before packaging; a synthetic label by X is
    // enough for this scene (rooms sit at x = 0 and x = 8).
    field.assign_rooms(|position| Some(usize::from(position[0] >= 4.0)));
    let lit = field
        .sample([2.0, 0.01, 2.0], Some(0))
        .expect("the lit room's probe resolves");
    for channel in 0..3 {
        assert!(
            lit.irradiance[channel] >= lit_target[channel] - 1.0e-6,
            "channel {channel}: the field must carry the fill: {:?} vs {lit_target:?}",
            lit.irradiance
        );
    }
    let dark = field
        .sample([10.0, 0.01, 2.0], Some(1))
        .expect("the empty room's probe resolves");
    assert_eq!(
        dark.irradiance, [0.0; 3],
        "a fixture-free room's probes stay dark"
    );
}

/// A solver-level mirror of the render suite's tall-chamber acceptance: a
/// 17 m room, one 0.45 panel, one whole-floor chart. The continuous baseline
/// response must lift dark samples while the physical pool keeps its
/// real margin over the far corner after soft-clip.
#[test]
fn a_chart_fill_preserves_the_tall_chamber_pool_contrast() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "transport_tall_chamber",
            "name": "Transport Tall Chamber",
            "spawn": { "x": 6.0, "z": 6.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 12.0, "depth": 12.0, "height": 17.0 } ],
            "ceiling_lights": [
                { "fixture": "core:fluorescent_panel_01", "x": 6.0, "z": 6.0, "brightness": 0.45 }
            ]
        }"#,
    )
    .expect("tall-chamber level parses");
    let lighting = LevelLighting::bake(&level);
    let charts = vec![floor_patch(0.0, 0.0, 12.0, 12.0, 24, 24)];
    let target = receiver_targets(&lighting, &charts);
    let emitter = TransportEmitter::from_baked(&lighting.lights()[0], None);
    let scene = TransportScene::new(Vec::new(), vec![emitter])
        .expect("scene")
        .with_receiver_target(target);
    let solved = scene.solve(&charts, options(0, 2), None).expect("solve");
    let chart = &solved.charts[0];
    let normal = [0.0, 1.0, 0.0];
    let luma = |color: [f32; 3]| {
        0.2126_f32.mul_add(
            soft_clip_channel(color[0]),
            0.7152_f32.mul_add(
                soft_clip_channel(color[1]),
                0.0722 * soft_clip_channel(color[2]),
            ),
        )
    };
    let mut under = (0.0_f32, 0usize);
    let mut far = (0.0_f32, 0usize);
    for (receiver, texel) in chart.receivers.iter().zip(&chart.texels) {
        let value = luma(texel.light_at(normal));
        let point = receiver.position;
        if (5.0..=7.0).contains(&point[0]) && (5.5..=6.5).contains(&point[2]) {
            under = (under.0 + value, under.1 + 1);
        }
        if (0.0..=2.5).contains(&point[0]) && (0.0..=2.5).contains(&point[2]) {
            far = (far.0 + value, far.1 + 1);
        }
    }
    assert!(
        under.1 > 0 && far.1 > 0,
        "both comparison boxes must sample real texels"
    );
    let under = under.0 / under.1 as f32;
    let far = far.0 / far.1 as f32;
    assert!(
        under > far + 15.0 / 255.0,
        "the floor under the panel ({under:.3}) must clearly beat the far floor ({far:.3}): \
         a 17 m ceiling must still pool"
    );
}

/// One ceiling patch at `y = 3`, wound so `cross(u, v)` points down (the
/// room-facing normal of a ceiling).
fn ceiling_patch(
    x0: f32,
    z0: f32,
    x1: f32,
    z1: f32,
    width: u32,
    height: u32,
) -> (LightmapPatch, Chart) {
    (
        LightmapPatch {
            origin: [x1, 3.0, z1],
            u_axis: [x0 - x1, 0.0, 0.0],
            v_axis: [0.0, 0.0, z0 - z1],
            diagonal_correction: [0.0; 3],
            triangle: false,
            room: None,
            kind: PatchKind::Ceiling,
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

/// Every architectural family receives the same smooth floor, including the
/// ceiling, while the physical bounce gradient and directional moment survive.
#[test]
fn the_spatial_fill_includes_ceilings_and_preserves_their_physical_gradients() {
    // The shared predicate is the single scoping decision.
    assert!(chart_receives_fill(PatchKind::Floor));
    assert!(chart_receives_fill(PatchKind::Wall));
    assert!(chart_receives_fill(PatchKind::Skirt));
    assert!(chart_receives_fill(PatchKind::Ceiling));

    let level = fill_test_level();
    let lighting = LevelLighting::bake(&level);
    let wall_chart = (
        LightmapPatch {
            origin: [0.0, 0.0, 4.0],
            u_axis: [0.0, 0.0, -4.0],
            v_axis: [0.0, 3.0, 0.0],
            diagonal_correction: [0.0; 3],
            triangle: false,
            room: None,
            kind: PatchKind::Wall,
        },
        Chart {
            page: 0,
            x: 0,
            y: 0,
            width: 4,
            height: 4,
        },
    );
    let charts = vec![
        floor_patch(0.0, 0.0, 4.0, 4.0, 4, 4),
        ceiling_patch(0.0, 0.0, 4.0, 4.0, 4, 4),
        wall_chart,
    ];
    let target = receiver_targets(&lighting, &charts);
    let floor_target = target[0];
    let wall_target = target[32];
    let ground = floor(0.0, 0.0, 4.0, 4.0, [0.7; 3]);
    let emitter = TransportEmitter::from_baked(&lighting.lights()[0], None);
    let plain = TransportScene::new(ground.clone(), vec![emitter])
        .expect("scene")
        .solve(&charts, options(2, 1), None)
        .expect("plain solve");
    let emitter = TransportEmitter::from_baked(&lighting.lights()[0], None);
    let filled = TransportScene::new(ground, vec![emitter])
        .expect("scene")
        .with_receiver_target(target)
        .solve(&charts, options(2, 1), None)
        .expect("filled solve");

    // The ceiling is physically lit, and the fill must preserve its structure.
    assert!(
        plain.charts[1]
            .texels
            .iter()
            .any(|texel| texel.irradiance[0] > 1.0e-4),
        "the setup needs a physically lit ceiling: {:?}",
        plain.charts[1].texels[0]
    );
    let mut lowest = f32::INFINITY;
    let mut highest = 0.0_f32;
    let support_scene = TransportScene::new(
        Vec::new(),
        vec![TransportEmitter::from_baked(&lighting.lights()[0], None)],
    )
    .expect("support");
    for ((original, lifted), receiver) in plain.charts[1]
        .texels
        .iter()
        .zip(&filled.charts[1].texels)
        .zip(&filled.charts[1].receivers)
    {
        let support = support_scene.baseline_support(receiver.position);
        let before = original.light_at([0.0, -1.0, 0.0]);
        let after = lifted.light_at([0.0, -1.0, 0.0]);
        assert_eq!(
            original.direction, lifted.direction,
            "fill preserves the directional moment"
        );
        for channel in 0..3 {
            assert!(after[channel] >= before[channel]);
            assert!(after[channel] >= floor_target[channel] * support - 1.0e-5);
        }
        lowest = lowest.min(after[0]);
        highest = highest.max(after[0]);
    }
    assert!(
        highest > lowest + 1.0e-4,
        "ceiling bounce gradients survive the fill"
    );

    // The floors and the wall rise to the authored fill mean.
    let floor_mean = chart_mean_light(&filled, 0, [0.0, 1.0, 0.0]);
    for channel in 0..3 {
        assert!(
            floor_mean[channel] >= floor_target[channel] - 1.0e-5,
            "channel {channel}: floor mean {floor_mean:?} vs target {floor_target:?}"
        );
    }
    let wall_mean = chart_mean_light(&filled, 2, [1.0, 0.0, 0.0]);
    for channel in 0..3 {
        assert!(
            wall_mean[channel] >= wall_target[channel] - 1.0e-5,
            "channel {channel}: wall mean {wall_mean:?} vs target {wall_target:?}"
        );
    }
}

/// The continuous probe response preserves a visible difference between
/// physically bright and dark parts of the same room.
#[test]
fn the_probe_fill_preserves_a_rooms_internal_structure() {
    let level = fill_test_level();
    let lighting = LevelLighting::bake(&level);
    let charts = vec![floor_patch(0.0, 0.0, 4.0, 4.0, 4, 4)];
    let target = receiver_targets(&lighting, &charts);
    let probes = probe_targets(&lighting, &charts);
    assert!(
        probes.iter().all(|probe| probe.room == 0),
        "the one-chart lattice must sit in the lit room"
    );
    let emitter = TransportEmitter::from_baked(&lighting.lights()[0], None);
    let scene = TransportScene::new(floor(0.0, 0.0, 4.0, 4.0, [0.7; 3]), vec![emitter])
        .expect("scene")
        .with_receiver_target(target)
        .with_probe_target(probes);
    let solved = scene
        .solve_with_probes(&charts, options(0, 1), None, true)
        .expect("solve");
    let field = solved.probes.expect("solved field");
    let mut lowest = f32::INFINITY;
    let mut highest = f32::NEG_INFINITY;
    for probe in &field.probes {
        lowest = lowest.min(probe.irradiance[0]);
        highest = highest.max(probe.irradiance[0]);
    }
    assert!(
        lowest.is_finite() && highest.is_finite(),
        "the field must hold finite probes"
    );
    assert!(
        highest > lowest + 1.0e-3,
        "the room's internal probe structure must survive the continuous fill: \
         highest {highest} lowest {lowest}"
    );
}

/// An edge texel can sit on a perpendicular face's plane. Leaving that plane
/// must remain clear even for a very oblique ray at translated world scales.
#[test]
fn perpendicular_corner_and_long_grazing_rays_do_not_self_occlude() {
    for translation in [0.0, 100.0, 10_000.0] {
        let mut triangles = floor(translation, 0.0, translation + 4.0, 4.0, [0.5; 3]);
        triangles.extend(wall(
            [translation, 0.0, 4.0],
            [translation, 0.0, 0.0],
            [translation, 4.0, 0.0],
            [translation, 4.0, 4.0],
            [0.5; 3],
        ));
        let scene = TransportScene::new(triangles, Vec::new()).expect("corner");
        let origin = receiver_position([translation, 0.0, 2.0], [0.0, 1.0, 0.0]);
        for target in [
            [translation + 2.0, 2.0, 2.0],
            [translation + 0.01, 0.02, 10_000.0],
        ] {
            assert!(
                !scene.occluded(origin, target),
                "corner {origin:?} -> {target:?}"
            );
        }
    }
}

#[test]
fn shared_triangle_edges_and_t_junctions_are_watertight() {
    let mut triangles = floor(-2.0, -2.0, 0.0, 2.0, [0.5; 3]);
    triangles.extend(floor(0.0, -2.0, 2.0, 0.0, [0.5; 3]));
    triangles.extend(floor(0.0, 0.0, 2.0, 2.0, [0.5; 3]));
    let scene = TransportScene::new(triangles, Vec::new()).expect("T junction");
    for point in [[0.0, 0.0, 0.0], [-1.0, 0.0, 0.0], [1.0, 0.0, 1.0]] {
        assert!(scene.occluded(add(point, [0.0, 1.0, 0.0]), add(point, [0.0, -1.0, 0.0])));
        let origin = receiver_position(point, [0.0, 1.0, 0.0]);
        assert!(!scene.occluded(origin, add(point, [10_000.0, 0.001, 0.02])));
        assert!(!scene.occluded(point, add(point, [0.0, 1.0, 0.0])));
    }
}

/// The old millimetre dead zone dropped the first real hit at a stair joint.
/// Both direct visibility and diffuse-bounce intersection must see it.
#[test]
fn stair_joint_blocks_even_submillimetre_hits() {
    let mut triangles = floor(-1.0, -1.0, 0.0, 1.0, [0.5; 3]);
    triangles.extend(wall(
        [0.0, 0.0, -1.0],
        [0.0, 0.0, 1.0],
        [0.0, 0.2, 1.0],
        [0.0, 0.2, -1.0],
        [0.5; 3],
    ));
    let scene = TransportScene::new(triangles, Vec::new()).expect("stair joint");
    let origin = receiver_position([-0.0001, 0.0, 0.0], [0.0, 1.0, 0.0]);
    assert!(scene.occluded(origin, [0.5, 0.1, 0.0]));
    let hit = scene
        .intersect(origin, [1.0, 0.0, 0.0])
        .expect("near riser blocks");
    assert!(hit.0 > 0.0 && hit.0 < 0.001);
}

#[test]
fn oblique_starting_surface_has_no_positive_self_hit() {
    let triangle = TransportTriangle::new(
        [100.0, 31.0, -71.0],
        [130.0, 47.0, -62.0],
        [102.0, 44.0, -20.0],
        [0.5; 3],
    )
    .expect("sloped triangle");
    let scene = TransportScene::new(vec![triangle], Vec::new()).expect("scene");
    let point = scale(add(add(triangle.p0, triangle.p1), triangle.p2), 1.0 / 3.0);
    let origin = receiver_position(point, triangle.normal);
    let tangent = normalize_or(sub(triangle.p1, triangle.p0), [1.0, 0.0, 0.0]);
    let target = add(
        origin,
        add(scale(tangent, 10_000.0), scale(triangle.normal, 0.01)),
    );
    assert!(!scene.occluded(origin, target));
    assert!(origin.iter().all(|value| value.is_finite()));
}

mod fill;

#[test]
fn stair_shared_edge_distinguishes_entering_solid_from_departing() {
    // Lower tread opens upward and riser opens toward negative x.
    let mut triangles = floor(-1.0, -1.0, 0.0, 1.0, [0.5; 3]);
    triangles.extend(wall(
        [0.0, 0.0, -1.0],
        [0.0, 0.0, 1.0],
        [0.0, 0.2, 1.0],
        [0.0, 0.2, -1.0],
        [0.5; 3],
    ));
    let scene = TransportScene::new(triangles, Vec::new()).expect("stair edge");
    let origin = receiver_position([0.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
    assert!(
        !scene.occluded(origin, [-0.5, 0.1, 0.0]),
        "departing riser is clear"
    );
    assert!(
        scene.occluded(origin, [0.5, 0.1, 0.0]),
        "entering riser blocks at zero t"
    );
}

/// Fixed cancellation witness from a deterministic search (seed 5931).
/// The old f32/FMA Moller-Trumbore kernel reports t=2.154646m even though the
/// supporting plane intersects behind the origin (t=-0.2302094m in f64).
#[test]
fn grazing_ray_cancellation_does_not_invent_a_blocker_two_metres_away() {
    let triangle = TransportTriangle::new(
        [-84.185_47, 110.031_66, -99.317_75],
        [46.573_353, -38.304_01, -105.976_29],
        [-118.381_86, 35.967_926, -28.538_006],
        [0.5; 3],
    )
    .expect("triangle");
    let origin = [-62.056_02, 28.499_092, -65.925_44];
    let direction = [0.660_889_57, -0.749_728_2, -0.033_654_39];
    let distance =
        ray_triangle(origin, direction, &triangle).expect("supporting plane intersection");
    assert!(
        distance < 0.0,
        "intersection is behind the origin: {distance}"
    );
    let scene = TransportScene::new(vec![triangle], Vec::new()).expect("scene");
    assert!(scene.intersect(origin, direction).is_none());
    assert!(!scene.any_hit(origin, direction, 10_000.0));
}

#[test]
fn corner_ray_origin_stays_on_the_charts_owning_side_and_surface() {
    let mut triangles = floor(0.0, 0.0, 2.0, 2.0, [0.3; 3]);
    triangles.extend(wall(
        [0.0, 0.0, 2.0],
        [2.0, 0.0, 2.0],
        [2.0, 2.0, 2.0],
        [0.0, 2.0, 2.0],
        [0.8; 3],
    ));
    // The two-sided wall is wound away from this floor, as at a room divider.
    let scene = TransportScene::new(triangles, vec![point([1.0, 1.0, 1.0], 2.0)]).expect("corner");
    let chart = floor_patch(0.0, 0.0, 2.0, 2.0, 3, 3);
    let solution = scene.solve(&[chart], options(0, 1), None).expect("solve");
    for (receiver, texel) in solution.charts[0]
        .receivers
        .iter()
        .zip(&solution.charts[0].texels)
    {
        assert!(receiver.ray_origin[2] < 2.0);
        assert_eq!(
            receiver.albedo, [0.3; 3],
            "a floor edge must keep its own material"
        );
        assert!(
            texel.light_at([0.0, 1.0, 0.0])[0] > 0.1,
            "corner must see the room light"
        );
        assert!((receiver.ray_origin[2] - receiver.position[2]).abs() < 0.00001);
    }
}

#[test]
fn subdivided_coplanar_surfaces_keep_their_bounce_cache_light() {
    let scene = TransportScene::new(Vec::new(), Vec::new()).expect("scene");
    let receiver = |x, surface, normal| TransportReceiver {
        position: [x, 0.0, 0.1],
        ray_origin: [x, 0.00001, 0.1],
        normal,
        albedo: [0.5; 3],
        area: 0.01,
        surface,
        attenuation: [1.0; 3],
    };
    // Four triangle/chart representatives compete in the same cache cell.
    let mut receivers: Vec<_> = (0..4_u16)
        .map(|index| {
            receiver(
                0.1 + f32::from(index) * 0.1,
                u32::from(index),
                [0.0, 1.0, 0.0],
            )
        })
        .collect();
    receivers.push(receiver(0.25, 4, [0.0, -1.0, 0.0]));
    let mut values = vec![
        Accumulator {
            irradiance: [0.6; 3],
            ..Accumulator::default()
        };
        4
    ];
    values.push(Accumulator::default());
    let cache = RadianceCache::build(&receivers);
    for (surface, receiver) in receivers.iter().enumerate() {
        let actual = cache.sample_surface(&scene, receiver.position, surface, &receivers, &values);
        for (actual, expected) in actual.irradiance.iter().zip(values[surface].irradiance) {
            assert!(
                (actual - expected).abs() < 1.0e-6,
                "each triangle must retain its own light, including the dark opposite face: {actual} vs {expected}"
            );
        }
    }
}

#[test]
fn reordering_charts_does_not_change_the_indirect_field() {
    let (scene, charts) = two_room_scene(0.8);
    let mut reversed = charts.clone();
    reversed.reverse();
    let first = scene
        .solve(&charts, options(2, 3), None)
        .expect("first solve");
    let second = scene
        .solve(&reversed, options(2, 3), None)
        .expect("reordered solve");
    for (original, reordered) in first.charts.iter().zip(second.charts.iter().rev()) {
        for (left, right) in original.texels.iter().zip(&reordered.texels) {
            for (a, b) in left.irradiance.iter().zip(right.irradiance) {
                assert!(
                    (a - b).abs() < 1.0e-5,
                    "chart order changed indirect light: {a} vs {b}"
                );
            }
        }
    }
}

mod probes;
