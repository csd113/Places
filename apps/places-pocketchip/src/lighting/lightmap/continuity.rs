//! Regression tests: a lightmap chart boundary must not become a lighting step.
//!
//! Every emitted quad owns its own lightmap chart, and chart texels *span*
//! their patch (see [`super::fill::fill_chart`]): the first and last texel sit
//! exactly on the patch's geometric edges, so two coplanar charts evaluate the
//! same world point on their shared edge and store it in the texel that edge
//! reconstructs. The atlas is sampled bilinearly, and a chart's gutter is a
//! copy of its own border texel.
//!
//! Spanning matters because a texel evaluated at its *centre* would
//! reconstruct, on the patch's geometric edge, the light half a texel *inside*
//! that patch. Two coplanar patches sharing an edge (two albedo materials on
//! one floor, two length runs of one wall, one surface split at the chart-span
//! cap) would then each reconstruct their own inward-shifted value, and the
//! two shifts point in opposite directions: a first-order step of
//! `grad * (tA + tB) / 2` at every chart boundary — a visible lighting seam
//! wherever the material changed, even though the lighting itself was
//! continuous there. These tests pin the spanning rule against that failure.
//!
//! These tests drive the real mesh emitter and the real atlas pages, reconstruct
//! a fragment exactly as the shader would (quantised [`Chart::uv_at`] then a
//! bilinear page sample), and pin that:
//!
//! * a material boundary on one coplanar floor does not step;
//! * the same holds across the other axis and for a same-material chart split;
//! * a real 90-degree corner keeps each face's own light (nothing is averaged
//!   across faces, so a corner cannot be smeared);
//! * the light on both sides of a seam is not saturated, so a test cannot pass
//!   merely because both sides clamp to the same value.

// Test code: unwrap/expect, indexing, printing and permissive float arithmetic
// are idiomatic here; the production lints stay enforced everywhere else.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::too_many_lines,
    clippy::unwrap_used
)]

use super::{Chart, LevelLightmaps, LightmapMode, LightmapPage, LightmapPatch, PatchKind};
use crate::level::LevelDef;
use crate::lighting::LevelLighting;
use crate::loader::PropCatalog;
use crate::props::PropAssets;
use crate::quality::QualityProfile;
use crate::render::{
    LevelBuild, LightmapBuildOptions, build_level_geometry_timed_with_lightmaps, logical_materials,
};

/// One 24 x 12 m room split into two 12 x 12 m material regions. The bright
/// fixture sits on the far side of the seam so the light has a real gradient
/// there, and neither side saturates.
const MATERIAL_REGIONS_X: &str = r#"{
    "format_version": 1,
    "id": "continuity_regions_x",
    "name": "Continuity Regions X",
    "spawn": { "x": 2.0, "z": 6.0 },
    "rooms": [{ "x": 0.0, "z": 0.0, "width": 24.0, "depth": 12.0, "height": 3.0 }],
    "floor_regions": [
        { "x": 0.0, "z": 0.0, "width": 12.0, "depth": 12.0, "material": "core:carpet_beige_01" },
        { "x": 12.0, "z": 0.0, "width": 12.0, "depth": 12.0, "material": "core:carpet_damp_01" }
    ],
    "ceiling_lights": [
        { "fixture": "core:fluorescent_panel_01", "x": 15.0, "z": 6.0, "intensity": 1.2 },
        { "fixture": "core:fluorescent_panel_01", "x": 2.0, "z": 6.0, "intensity": 0.2 }
    ]
}"#;

/// The same boundary rotated 90 degrees in plan, so the seam terminates the
/// charts' other (`v`) axis.
const MATERIAL_REGIONS_Z: &str = r#"{
    "format_version": 1,
    "id": "continuity_regions_z",
    "name": "Continuity Regions Z",
    "spawn": { "x": 12.0, "z": 2.0 },
    "rooms": [{ "x": 0.0, "z": 0.0, "width": 24.0, "depth": 12.0, "height": 3.0 }],
    "floor_regions": [
        { "x": 0.0, "z": 0.0, "width": 24.0, "depth": 6.0, "material": "core:carpet_beige_01" },
        { "x": 0.0, "z": 6.0, "width": 24.0, "depth": 6.0, "material": "core:carpet_damp_01" }
    ],
    "ceiling_lights": [
        { "fixture": "core:fluorescent_panel_01", "x": 12.0, "z": 9.0, "intensity": 1.2 },
        { "fixture": "core:fluorescent_panel_01", "x": 12.0, "z": 1.5, "intensity": 0.2 }
    ]
}"#;

/// One 70 m room, one material: the greedy merge stops at
/// [`super::MAX_CHART_SPAN_M`] and emits two coplanar floor quads that differ
/// only in which emit call produced them.
const SAME_MATERIAL_SPAN: &str = r#"{
    "format_version": 1,
    "id": "continuity_span",
    "name": "Continuity Span",
    "spawn": { "x": 2.0, "z": 6.0 },
    "rooms": [{ "x": 0.0, "z": 0.0, "width": 70.0, "depth": 12.0, "height": 3.0 }],
    "ceiling_lights": [
        { "fixture": "core:fluorescent_panel_01", "x": 10.0, "z": 6.0, "intensity": 0.3 },
        { "fixture": "core:fluorescent_panel_01", "x": 61.5, "z": 6.0, "intensity": 1.2 }
    ]
}"#;

/// A floor meeting a wall at a real 90-degree corner.
const RIGHT_ANGLE_CORNER: &str = r#"{
    "format_version": 1,
    "id": "continuity_corner",
    "name": "Continuity Corner",
    "spawn": { "x": 10.0, "z": 6.0 },
    "rooms": [{ "x": 0.0, "z": 0.0, "width": 20.0, "depth": 12.0, "height": 3.0 }],
    "walls": [{ "x": 0.0, "z": -0.3, "width": 20.0, "depth": 0.3, "height": 3.0 }],
    "ceiling_lights": [
        { "fixture": "core:fluorescent_panel_01", "x": 10.0, "z": 0.8, "intensity": 0.8 }
    ]
}"#;

fn parse(json: &str) -> LevelDef {
    LevelDef::from_json(json).expect("test level must parse")
}

fn build(level: &LevelDef, profile: QualityProfile) -> LevelBuild {
    let materials = logical_materials(level);
    let catalog = PropCatalog::builtin();
    let mut assets = PropAssets::default();
    build_level_geometry_timed_with_lightmaps(
        level,
        &catalog,
        &mut assets,
        &materials,
        LightmapBuildOptions::for_profile(profile, LightmapMode::On),
        None,
    )
}

fn lightmaps_of(build: &LevelBuild) -> &LevelLightmaps {
    build
        .lightmaps
        .as_deref()
        .expect("the build must have baked lightmaps")
}

// ------------------------------------------------------- atlas reconstruction

/// One RGB8 page texel as linear `0..=1`, transparent black outside the page
/// (the GL clamp the lightmap sampler uses).
fn page_texel(page: &LightmapPage, x: i64, y: i64) -> [f32; 3] {
    let width = i64::from(page.width);
    let height = i64::from(page.height);
    if x < 0 || y < 0 || x >= width || y >= height {
        return [0.0; 3];
    }
    let index = usize::try_from((y * width + x) * 3).expect("a page index fits usize");
    [
        f32::from(page.rgb[index]) / 255.0,
        f32::from(page.rgb[index + 1]) / 255.0,
        f32::from(page.rgb[index + 2]) / 255.0,
    ]
}

fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    std::array::from_fn(|channel| (b[channel] - a[channel]).mul_add(t, a[channel]))
}

/// `GL_LINEAR` reconstruction of one page at a normalised UV.
///
/// Atlas texel `i` is a unit cell centred at `i + 0.5`, so a UV maps to texel
/// coordinate `uv * edge - 0.5` in centre space.
fn sample_atlas(page: &LightmapPage, uv: [f32; 2]) -> [f32; 3] {
    let edge = f32::from(u16::try_from(page.width).unwrap_or(u16::MAX));
    let tx = uv[0].mul_add(edge, -0.5);
    let ty = uv[1].mul_add(edge, -0.5);
    let (fx, fy) = (tx.floor(), ty.floor());
    let (wx, wy) = (tx - fx, ty - fy);
    let (x0, y0) = (fx as i64, fy as i64);
    let top = lerp3(page_texel(page, x0, y0), page_texel(page, x0 + 1, y0), wx);
    let bottom = lerp3(
        page_texel(page, x0, y0 + 1),
        page_texel(page, x0 + 1, y0 + 1),
        wx,
    );
    lerp3(top, bottom, wy)
}

/// What the fragment shader reconstructs at local `(u, v)` of one chart: the
/// quantised [`Chart::uv_at`] UV, bilinearly filtered from the page.
fn reconstruct(page: &LightmapPage, chart: &Chart, u: f32, v: f32) -> [f32; 3] {
    let [qx, qy] = chart.uv_at(page.width, u, v);
    sample_atlas(page, [f32::from(qx) / 65_535.0, f32::from(qy) / 65_535.0])
}

// ------------------------------------------------------------ patch plumbing

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|axis| a[axis] - b[axis])
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[2].mul_add(b[2], a[1].mul_add(b[1], a[0] * b[0]))
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1].mul_add(b[2], -(a[2] * b[1])),
        a[2].mul_add(b[0], -(a[0] * b[2])),
        a[0].mul_add(b[1], -(a[1] * b[0])),
    ]
}

fn length(a: [f32; 3]) -> f32 {
    dot(a, a).sqrt()
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    length(sub(a, b))
}

fn max_abs(a: [f32; 3]) -> f32 {
    a.iter().fold(0.0_f32, |m, v| m.max(v.abs()))
}

fn patch_normal(patch: &LightmapPatch) -> [f32; 3] {
    let normal = cross(patch.u_axis, patch.v_axis);
    let len = length(normal);
    if len <= 1.0e-12 {
        [0.0; 3]
    } else {
        normal.map(|value| value / len)
    }
}

/// The four world-space edges of a patch: `u = 0`, `u = 1`, `v = 0`, `v = 1`.
fn patch_edges(patch: &LightmapPatch) -> [([f32; 3], [f32; 3]); 4] {
    [
        (patch.point_at(0.0, 0.0), patch.point_at(0.0, 1.0)),
        (patch.point_at(1.0, 0.0), patch.point_at(1.0, 1.0)),
        (patch.point_at(0.0, 0.0), patch.point_at(1.0, 0.0)),
        (patch.point_at(0.0, 1.0), patch.point_at(1.0, 1.0)),
    ]
}

/// True when two patches lie in one plane with the same facing.
fn patches_coplanar(a: &LightmapPatch, b: &LightmapPatch) -> bool {
    let normal = patch_normal(a);
    dot(normal, patch_normal(b)) >= 1.0 - 1.0e-4
        && dot(sub(b.origin, a.origin), normal).abs() <= 1.0e-4
}

/// Every pair of same-room, coplanar, same-kind charts that share a full edge.
fn coplanar_pairs(
    lightmaps: &LevelLightmaps,
    kind: PatchKind,
) -> Vec<(LightmapPatch, Chart, LightmapPatch, Chart)> {
    let stamped: Vec<(LightmapPatch, Chart)> = lightmaps
        .charts
        .iter()
        .copied()
        .filter(|(patch, _)| patch.kind == kind)
        .collect();
    let mut pairs = Vec::new();
    for (index, a) in stamped.iter().enumerate() {
        for b in stamped.iter().skip(index + 1) {
            if a.0.room != b.0.room || !patches_coplanar(&a.0, &b.0) {
                continue;
            }
            let shared = patch_edges(&a.0).iter().any(|(ap, aq)| {
                patch_edges(&b.0).iter().any(|(bp, bq)| {
                    let forward = distance(*ap, *bp) + distance(*aq, *bq);
                    let reverse = distance(*ap, *bq) + distance(*aq, *bp);
                    forward.min(reverse) <= 1.0e-4 && distance(*ap, *aq) > 1.0e-3
                })
            });
            if shared {
                pairs.push((a.0, a.1, b.0, b.1));
            }
        }
    }
    pairs
}

/// True when `point` lies on one of the patch's boundary edges.
fn on_patch_edge(patch: &LightmapPatch, point: [f32; 3]) -> bool {
    patch_edges(patch)
        .iter()
        .any(|(a, b)| distance_to_segment(point, *a, *b) <= 1.0e-3)
}

/// Distance from `point` to the segment `a`-`b`.
fn distance_to_segment(point: [f32; 3], a: [f32; 3], b: [f32; 3]) -> f32 {
    let ab = sub(b, a);
    let length_squared = dot(ab, ab);
    if length_squared <= 1.0e-12 {
        return distance(point, a);
    }
    let t = (dot(sub(point, a), ab) / length_squared).clamp(0.0, 1.0);
    let closest: [f32; 3] = std::array::from_fn(|axis| ab[axis].mul_add(t, a[axis]));
    distance(point, closest)
}

/// The baked light at a world point, as the fill pass evaluates it.
fn light_at(lighting: &LevelLighting, room: Option<usize>, point: [f32; 3]) -> [f32; 3] {
    let light = lighting.lightmap_texel(room, point[0], point[1], point[2]);
    [light.r, light.g, light.b]
}

/// One measured coplanar seam.
struct SeamMeasurement {
    /// Largest per-channel |recA - recB| over the sampled edge points.
    step: f32,
    /// Largest per-channel |trueA - trueB| at the same points: the real
    /// discontinuity the seam is allowed to have (zero for a coplanar seam).
    true_step: f32,
    /// Largest light value seen on either side, so a test can prove it is not
    /// measuring a pair of saturated samples.
    peak: f32,
}

/// Reconstructs both charts at five points along their shared edge and reports
/// the largest step, the true field's own change and the peak value.
fn measure_shared_edge(
    lighting: &LevelLighting,
    a: &LightmapPatch,
    chart_a: &Chart,
    page_a: &LightmapPage,
    b: &LightmapPatch,
    chart_b: &Chart,
    page_b: &LightmapPage,
) -> SeamMeasurement {
    let mut step = 0.0_f32;
    let mut true_step = 0.0_f32;
    let mut peak = 0.0_f32;
    for index in 0..=4 {
        // A point on the edge, a hair inside edge A: the shared line is found
        // from A's own frame and then projected into each patch's local frame.
        let t = f32::from(u8::try_from(index).unwrap_or(0)) / 4.0;
        let point = edge_sample_point(a, b, t);
        let (ua, va) = a.local_of(point);
        let (ub, vb) = b.local_of(point);
        let rec_a = reconstruct(page_a, chart_a, ua, va);
        let rec_b = reconstruct(page_b, chart_b, ub, vb);
        // Sanity: the sample really is on both patches' boundaries, so the
        // charts are being asked about the shared edge and not about interiors.
        assert!(
            on_patch_edge(a, a.point_at(ua, va)) && on_patch_edge(b, b.point_at(ub, vb)),
            "the sample must lie on both patches' edges"
        );
        let true_a = light_at(lighting, a.room, a.point_at(ua, va));
        let true_b = light_at(lighting, b.room, b.point_at(ub, vb));
        step = step.max(max_abs(sub(rec_a, rec_b)));
        true_step = true_step.max(max_abs(sub(true_a, true_b)));
        peak = peak
            .max(max_abs(rec_a))
            .max(max_abs(rec_b))
            .max(max_abs(true_a))
            .max(max_abs(true_b));
    }
    SeamMeasurement {
        step,
        true_step,
        peak,
    }
}

/// A world point on the edge shared by `a` and `b`, at fraction `t` along it.
///
/// A's edge nearest `b` is located by matching corner pairs, exactly like the
/// coplanar-pair search, so the measurement does not depend on which axis of
/// the patch the seam terminates.
fn edge_sample_point(a: &LightmapPatch, b: &LightmapPatch, t: f32) -> [f32; 3] {
    for (ap, aq) in patch_edges(a) {
        for (bp, bq) in patch_edges(b) {
            let forward = distance(ap, bp) + distance(aq, bq);
            let reverse = distance(ap, bq) + distance(aq, bp);
            if forward.min(reverse) <= 1.0e-4 && distance(ap, aq) > 1.0e-3 {
                return std::array::from_fn(|axis| (aq[axis] - ap[axis]).mul_add(t, ap[axis]));
            }
        }
    }
    panic!("the patches must share an edge");
}

/// One material boundary on one coplanar floor must not step, at both profiles.
#[test]
fn a_material_boundary_on_one_floor_does_not_step() {
    for profile in [QualityProfile::Full, QualityProfile::Low] {
        let level = parse(MATERIAL_REGIONS_X);
        let build = build(&level, profile);
        assert_eq!(build.lightmap_failure, None, "{profile:?} must bake");
        let lightmaps = lightmaps_of(&build);
        let pages = lightmaps.pages.as_slice();
        let pairs = coplanar_pairs(lightmaps, PatchKind::Floor);
        assert_eq!(
            pairs.len(),
            1,
            "{profile:?}: the material boundary must be one coplanar pair"
        );
        let (a, chart_a, b, chart_b) = pairs[0];
        let page_a = pages.get(usize::from(chart_a.page)).expect("page A");
        let page_b = pages.get(usize::from(chart_b.page)).expect("page B");
        let measured =
            measure_shared_edge(&build.lighting, &a, &chart_a, page_a, &b, &chart_b, page_b);
        assert!(
            measured.true_step <= 1.0 / 255.0,
            "{profile:?}: the coplanar field must be continuous there (true step {:.4})",
            measured.true_step
        );
        assert!(
            measured.peak < 0.99,
            "{profile:?}: the seam must be measured away from saturation (peak {:.3})",
            measured.peak
        );
        assert!(
            measured.step <= 1.0 / 255.0 + 1.0e-6,
            "{profile:?}: a material boundary must not step in the lightmap (measured {:.4} = {:.2}/255)",
            measured.step,
            measured.step * 255.0
        );
    }
}

/// The same requirement with the seam terminating the charts' `v` axis.
#[test]
fn a_material_boundary_across_the_other_axis_does_not_step() {
    let level = parse(MATERIAL_REGIONS_Z);
    let build = build(&level, QualityProfile::Full);
    let lightmaps = lightmaps_of(&build);
    let pairs = coplanar_pairs(lightmaps, PatchKind::Floor);
    assert_eq!(pairs.len(), 1, "the Z split must give one coplanar pair");
    let (a, chart_a, b, chart_b) = pairs[0];
    let page_a = &lightmaps.pages[usize::from(chart_a.page)];
    let page_b = &lightmaps.pages[usize::from(chart_b.page)];
    let measured = measure_shared_edge(&build.lighting, &a, &chart_a, page_a, &b, &chart_b, page_b);
    assert!(
        measured.peak < 0.99,
        "the seam must be measured away from saturation (peak {:.3})",
        measured.peak
    );
    assert!(
        measured.step <= 1.0 / 255.0 + 1.0e-6,
        "a material boundary must not step in the lightmap (measured {:.4})",
        measured.step
    );
}

/// Two coplanar quads of the *same* material, split only at the chart-span cap,
/// must agree as well: geometry topology must not create a lighting step.
#[test]
fn a_same_material_chart_split_does_not_step() {
    let level = parse(SAME_MATERIAL_SPAN);
    let build = build(&level, QualityProfile::Full);
    assert_eq!(build.lightmap_failure, None, "the span level must bake");
    let lightmaps = lightmaps_of(&build);
    let pairs = coplanar_pairs(lightmaps, PatchKind::Floor);
    assert_eq!(pairs.len(), 1, "the span cap must leave one coplanar pair");
    let (a, chart_a, b, chart_b) = pairs[0];
    for patch in [&a, &b] {
        let span = length(patch.u_axis).max(length(patch.v_axis));
        assert!(
            span <= super::MAX_CHART_SPAN_M + 1.0e-3,
            "each quad must respect the chart span cap (span {span:.3} m)"
        );
    }
    let page_a = &lightmaps.pages[usize::from(chart_a.page)];
    let page_b = &lightmaps.pages[usize::from(chart_b.page)];
    let measured = measure_shared_edge(&build.lighting, &a, &chart_a, page_a, &b, &chart_b, page_b);
    assert!(
        measured.peak < 0.99,
        "the seam must be measured away from saturation (peak {:.3})",
        measured.peak
    );
    assert!(
        measured.step <= 1.0 / 255.0 + 1.0e-6,
        "a chart-span split must not step in the lightmap (measured {:.4})",
        measured.step
    );
}

/// A real 90-degree corner is a genuine lighting boundary: each face must keep
/// its own light, and nothing may be averaged across the corner.
#[test]
fn a_right_angle_corner_keeps_each_faces_own_light() {
    let level = parse(RIGHT_ANGLE_CORNER);
    let build = build(&level, QualityProfile::Full);
    let lightmaps = lightmaps_of(&build);
    let floors = coplanar_pairs(lightmaps, PatchKind::Floor);
    let walls = coplanar_pairs(lightmaps, PatchKind::Wall);
    assert!(
        floors.is_empty(),
        "a floor is not coplanar with a wall: {floors:?} floor pair(s)"
    );
    // Find the floor and wall charts whose boundaries meet at the wall's base.
    let corner = [10.0_f32, 0.0, 0.0];
    let mut floor_chart = None;
    let mut wall_chart = None;
    for (patch, chart) in &lightmaps.charts {
        if !on_patch_edge(patch, corner) {
            continue;
        }
        match patch.kind {
            PatchKind::Floor => floor_chart = Some((*patch, *chart)),
            PatchKind::Wall => wall_chart = Some((*patch, *chart)),
            PatchKind::Ceiling | PatchKind::Skirt => {}
        }
    }
    let (floor, floor_chart) = floor_chart.expect("a floor chart touches the corner");
    let (wall, wall_chart) = wall_chart.expect("a wall chart touches the corner");
    assert!(
        walls.is_empty(),
        "the floor and wall must not be treated as a coplanar seam"
    );
    // Each face's boundary texel holds that face's own light at the corner: no
    // cross-surface average, which would mix the wall's normal gradient into
    // the floor (and vice versa).
    let page_floor = &lightmaps.pages[usize::from(floor_chart.page)];
    let page_wall = &lightmaps.pages[usize::from(wall_chart.page)];
    let (uf, vf) = floor.local_of(corner);
    let (uw, vw) = wall.local_of(corner);
    let rec_floor = reconstruct(page_floor, &floor_chart, uf, vf);
    let rec_wall = reconstruct(page_wall, &wall_chart, uw, vw);
    let true_floor = light_at(&build.lighting, floor.room, floor.point_at(uf, vf));
    let true_wall = light_at(&build.lighting, wall.room, wall.point_at(uw, vw));
    for (name, reconstructed, truth) in [
        ("floor", rec_floor, true_floor),
        ("wall", rec_wall, true_wall),
    ] {
        assert!(
            max_abs(sub(reconstructed, truth)) <= 1.0 / 255.0 + 1.0e-6,
            "the {name} must hold its own light at the corner (got {reconstructed:?}, truth {truth:?})"
        );
    }
    // A cross-surface average would land between the two; if the two faces
    // genuinely differ, that average would be wrong on both.
    let average: [f32; 3] = std::array::from_fn(|c| f32::midpoint(true_floor[c], true_wall[c]));
    if max_abs(sub(true_floor, true_wall)) > 2.0 / 255.0 {
        assert!(
            max_abs(sub(rec_floor, average)) > 1.0 / 255.0
                || max_abs(sub(rec_wall, average)) > 1.0 / 255.0,
            "a corner must not be averaged: floor {true_floor:?}, wall {true_wall:?}"
        );
    }
}
