//! Unit tests for the lightmap plan, packer, cache and atlas.

// Test code: unwrap/expect, indexing, loose casts and permissive arithmetic are
// idiomatic in tests; the production lints stay enforced everywhere else.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::format_push_string,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

use super::{Chart, ChartAllocator, LightmapConfig, LightmapMode, LightmapPatch, PatchKind};
use crate::quality::QualityProfile;

/// A flat X/Z floor quad at `y`, from `(x0, z0)` to `(x1, z1)`, wound as a floor.
fn floor_quad(x0: f32, z0: f32, x1: f32, z1: f32, y: f32) -> [[f32; 3]; 4] {
    [[x0, y, z1], [x1, y, z1], [x1, y, z0], [x0, y, z0]]
}

fn patch(width: f32, height: f32) -> LightmapPatch {
    LightmapPatch::from_quad(
        PatchKind::Floor,
        floor_quad(0.0, 0.0, width, height, 0.0),
        None,
    )
    .expect("a positive rectangle is a valid patch")
}

#[test]
fn patch_local_round_trip() {
    let patch = patch(6.0, 3.0);
    for u in [0.0_f32, 0.25, 0.5, 0.75, 1.0] {
        for v in [0.0_f32, 0.5, 1.0] {
            let point = patch.point_at(u, v);
            let (back_u, back_v) = patch.local_of(point);
            assert!((back_u - u).abs() < 1.0e-5, "u {u} -> {back_u}");
            assert!((back_v - v).abs() < 1.0e-5, "v {v} -> {back_v}");
        }
    }
}

#[test]
fn patch_axes_follow_the_quad_winding() {
    let corners = [
        [1.0, 2.0, 3.0],
        [3.0, 2.0, 3.0],
        [3.0, 2.0, 5.0],
        [1.0, 2.0, 5.0],
    ];
    let patch = LightmapPatch::from_quad(PatchKind::Wall, corners, Some(4)).expect("valid");
    let (u, v) = patch.local_of(corners[0]);
    assert!((u).abs() < 1.0e-6 && (v).abs() < 1.0e-6, "p0 is (0,0)");
    let (u, v) = patch.local_of(corners[1]);
    assert!((u - 1.0).abs() < 1.0e-6 && v.abs() < 1.0e-6, "p1 is (1,0)");
    let (u, v) = patch.local_of(corners[3]);
    assert!(u.abs() < 1.0e-6 && (v - 1.0).abs() < 1.0e-6, "p3 is (0,1)");
    let (u, v) = patch.local_of(corners[2]);
    assert!(
        (u - 1.0).abs() < 1.0e-6 && (v - 1.0).abs() < 1.0e-6,
        "p2 is (1,1)"
    );
    assert_eq!(patch.kind, PatchKind::Wall);
    assert_eq!(patch.room, Some(4));
}

#[test]
fn patch_extents_are_axis_lengths() {
    let patch = LightmapPatch::from_quad(
        PatchKind::Floor,
        [
            [0.0, 0.0, 0.0],
            [5.0, 0.0, 0.0],
            [5.0, 0.0, 2.0],
            [0.0, 0.0, 2.0],
        ],
        None,
    )
    .expect("valid");
    let (u, v) = patch.extent_m();
    assert!((u - 5.0).abs() < 1.0e-5);
    assert!((v - 2.0).abs() < 1.0e-5);
}

#[test]
fn degenerate_quads_are_rejected() {
    let none = |corners| LightmapPatch::from_quad(PatchKind::Wall, corners, None);
    // Zero-length u axis.
    assert!(
        none([
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 1.0],
            [0.0, 0.0, 1.0]
        ])
        .is_none()
    );
    // Zero area.
    assert!(
        none([
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 0.0, 0.0]
        ])
        .is_none()
    );
    // Non-finite corner.
    assert!(
        none([
            [0.0, 0.0, 0.0],
            [f32::NAN, 0.0, 0.0],
            [1.0, 0.0, 1.0],
            [0.0, 0.0, 1.0]
        ])
        .is_none()
    );
    // Bow-tie: the fourth corner does not close the frame.
    assert!(
        none([
            [0.0, 0.0, 0.0],
            [2.0, 0.0, 0.0],
            [3.0, 0.0, 9.0],
            [0.0, 0.0, 2.0]
        ])
        .is_none()
    );
}

#[test]
fn chart_uvs_stay_inside_the_chart() {
    let config = LightmapConfig::for_profile(crate::quality::QualityProfile::Full);
    let chart = Chart {
        page: 0,
        x: 10,
        y: 20,
        width: 100,
        height: 50,
    };
    let edge = config.page_edge;
    for (u, v) in [(0.0_f32, 0.0_f32), (0.5, 0.5), (1.0, 1.0), (-1.0, 2.0)] {
        let uv = chart.uv_at(edge, u, v);
        let scale = f32::from(u16::try_from(edge).expect("edge"));
        let x = f32::from(uv[0]) / 65_535.0 * scale;
        let y = f32::from(uv[1]) / 65_535.0 * scale;
        assert!(
            (10.0 - 0.1..=110.0 + 0.1).contains(&x),
            "x {x} outside the chart"
        );
        assert!(
            (20.0 - 0.1..=70.0 + 0.1).contains(&y),
            "y {y} outside the chart"
        );
    }
}

#[test]
fn packing_is_deterministic_and_disjoint() {
    let config = LightmapConfig::for_profile(crate::quality::QualityProfile::Full);
    let patches: Vec<LightmapPatch> = (0..40)
        .map(|index| patch(1.0 + (index % 7) as f32, 0.5 + (index % 5) as f32))
        .collect();
    let pack = |patches: &[LightmapPatch]| {
        let mut allocator = ChartAllocator::new(config);
        let charts: Vec<Chart> = patches
            .iter()
            .filter_map(|patch| allocator.allocate(patch))
            .collect();
        (allocator, charts)
    };
    let (allocator, charts) = pack(&patches);
    let (again, again_charts) = pack(&patches);
    assert_eq!(charts, again_charts, "packing must be deterministic");
    assert_eq!(allocator.page_count(), again.page_count());
    assert!(!allocator.failed());

    for (chart, page_edge) in charts.iter().map(|chart| (chart, config.page_edge)) {
        assert!(chart.x + chart.width <= page_edge);
        assert!(chart.y + chart.height <= page_edge);
    }
    // Every outer rectangle (data + both gutters) is disjoint from every other.
    let padding = config.padding;
    for (index, a) in charts.iter().enumerate() {
        let a = (
            a.x.saturating_sub(padding),
            a.y.saturating_sub(padding),
            a.x + a.width + padding,
            a.y + a.height + padding,
        );
        for b in charts
            .iter()
            .skip(index + 1)
            .filter(|b| b.page == charts[index].page)
        {
            let b = (
                b.x.saturating_sub(padding),
                b.y.saturating_sub(padding),
                b.x + b.width + padding,
                b.y + b.height + padding,
            );
            let disjoint = a.2 <= b.0 || b.2 <= a.0 || a.3 <= b.1 || b.3 <= a.1;
            assert!(disjoint, "charts {index} and their gutters overlap");
        }
    }
}

#[test]
fn the_packer_uses_best_short_side_fit() {
    // The policy in one picture: a 32 x 32 page with a tall 8 x 16 chart first.
    // Every following 8 x 8 chart takes the free rectangle whose shorter
    // remainder is smallest (then the smaller longer remainder), so it fills
    // the tight bands beside and above the tall chart instead of stacking at
    // the bottom. The page ends exactly full, with no wasted texel.
    let config = LightmapConfig {
        texels_per_metre: 1.0,
        page_edge: 32,
        max_pages: 1,
        padding: 0,
        bytes_per_texel: 16,
    };
    let mut allocator = ChartAllocator::new(config);
    let tall = allocator.allocate(&patch(8.0, 16.0)).expect("tall chart");
    assert_eq!((tall.x, tall.y, tall.width, tall.height), (0, 0, 8, 16));
    let expected = [
        (0u32, 16u32),
        (0, 24),
        (8, 0),
        (16, 0),
        (24, 0),
        (8, 8),
        (16, 8),
        (24, 8),
        (8, 16),
        (8, 24),
        (16, 16),
        (24, 16),
        (16, 24),
        (24, 24),
    ];
    for (x, y) in expected {
        let chart = allocator.allocate(&patch(8.0, 8.0)).expect("short chart");
        assert_eq!((chart.x, chart.y), (x, y), "best-short-side-fit placement");
    }
    // The 32 x 32 page is now full (the tall chart plus fourteen 8 x 8 charts
    // cover all 1024 texels), and the one-page budget is spent.
    assert!(allocator.allocate(&patch(8.0, 8.0)).is_none());
    assert!(allocator.failed());
}

#[test]
fn overflow_is_reported_not_hidden() {
    let config = LightmapConfig {
        max_pages: 1,
        ..LightmapConfig::for_profile(crate::quality::QualityProfile::Full)
    };
    let mut allocator = ChartAllocator::new(config);
    let usable = config.usable_edge();
    // One chart fills an empty page exactly; a second cannot fit anywhere.
    let big = patch(
        usable as f32 / config.texels_per_metre,
        usable as f32 / config.texels_per_metre,
    );
    assert!(allocator.allocate(&big).is_some(), "first chart fits");
    assert!(allocator.allocate(&big).is_none(), "second chart overflows");
    assert!(allocator.failed());
    assert!(
        allocator.allocate(&patch(1.0, 1.0)).is_none(),
        "failure is sticky"
    );
}

#[test]
fn the_shipped_page_budget_is_used_when_genuinely_needed() {
    let config = LightmapConfig::for_profile(crate::quality::QualityProfile::Full);
    assert_eq!(
        config.max_pages,
        super::LIGHTMAP_ATLAS_MAX_PAGES,
        "the shipped budget is the shared page budget"
    );
    let mut allocator = ChartAllocator::new(config);
    let span = config.max_chart_span_m();
    // A chart at the span cap fills a page at this density, so each of the
    // first `max_pages` charts opens its own page.
    let big = patch(span, span);
    for page in 0..config.max_pages {
        assert!(
            allocator.allocate(&big).is_some(),
            "chart {page} needs page {page}"
        );
        assert_eq!(allocator.page_count(), page + 1);
    }
    assert!(!allocator.failed());
    // One more big chart exceeds the shipped budget.
    assert!(allocator.allocate(&big).is_none());
    assert!(allocator.failed());
    assert_eq!(allocator.page_count(), config.max_pages);
}

#[test]
fn a_two_page_allocator_still_reports_the_same_overflow() {
    // The budget is a config field, not a hard-coded count: an allocator built
    // with two pages still fills two and rejects the third with the same named
    // failure the plan reports.
    let config = LightmapConfig {
        max_pages: 2,
        ..LightmapConfig::for_profile(crate::quality::QualityProfile::Full)
    };
    let mut allocator = ChartAllocator::new(config);
    let big = patch(config.max_chart_span_m(), config.max_chart_span_m());
    assert!(allocator.allocate(&big).is_some());
    assert!(allocator.allocate(&big).is_some());
    assert_eq!(allocator.page_count(), 2);
    assert!(allocator.allocate(&big).is_none());
    assert!(allocator.failed());
}

#[test]
fn chart_texels_match_the_density_and_are_profile_specific() {
    let full = LightmapConfig::for_profile(crate::quality::QualityProfile::Full);
    let low = LightmapConfig::for_profile(crate::quality::QualityProfile::Low);
    let patch = patch(10.0, 2.0);
    // Stated in metres and the profile's own density, so a retune of the
    // density cannot silently invalidate the expectation.
    let texels = |config: &LightmapConfig, metres: f32| -> u32 {
        (metres * config.texels_per_metre).ceil() as u32
    };
    assert_eq!(
        full.chart_texels(&patch),
        (texels(&full, 10.0), texels(&full, 2.0))
    );
    assert_eq!(
        low.chart_texels(&patch),
        (texels(&low, 10.0), texels(&low, 2.0))
    );
    assert!(
        full.chart_texels(&patch).0 > low.chart_texels(&patch).0,
        "Full must resolve more texels than Low"
    );
    assert_eq!(full.max_chart_span_m(), low.max_chart_span_m());
}

use super::{
    LIGHTMAP_FORMAT_VERSION, LightmapAtlas, LightmapFailure, LightmapPlan, LightmapTexel,
    content_key, content_key_with_extra, page_png_bytes, read_page_texel,
};

#[test]
fn plan_stamps_the_six_vertices_with_the_chart_mapping() {
    let config = LightmapConfig::for_profile(crate::quality::QualityProfile::Full);
    let mut plan = LightmapPlan::new(config);
    let corners = floor_quad(0.0, 0.0, 4.0, 2.0, 0.0);
    let mut vertices = vec![crate::render::Vertex::UNLIT; 6];
    assert!(plan.stamp_emitted(&mut vertices, 0, PatchKind::Floor, corners, Some(7)));
    assert_eq!(plan.chart_count(), 1);
    assert!(!plan.failed());
    let (_, chart) = plan.charts()[0];
    // A 4x2 m quad at this profile's density.
    assert_eq!(chart.width, (4.0 * config.texels_per_metre).ceil() as u32);
    assert_eq!(chart.height, (2.0 * config.texels_per_metre).ceil() as u32);
    for (index, corner) in [0usize, 1, 2, 0, 2, 3].into_iter().enumerate() {
        let (u, v) = match corner {
            0 => (0.0, 0.0),
            1 => (1.0, 0.0),
            2 => (1.0, 1.0),
            _ => (0.0, 1.0),
        };
        let expected = chart.uv_at(config.page_edge, u, v);
        let vertex = vertices[index];
        assert_eq!(vertex.lightmap, expected);
        assert_eq!(usize::from(vertex.lightmap_page), usize::from(chart.page));
        assert!(vertex.is_lightmapped());
    }
}

#[test]
fn plan_skips_an_invisible_sliver_and_keeps_the_rest() {
    // A sub-millimetre sliver is invisible: leaving its six vertices vertex-lit
    // must not cost the level its whole lightmap, which is what treating it as a
    // build failure used to do (a baseboard cap trimmed at a corner joint can
    // leave one).
    let config = LightmapConfig::for_profile(crate::quality::QualityProfile::Full);
    let mut plan = LightmapPlan::new(config);
    let mut vertices = vec![crate::render::Vertex::UNLIT; 12];
    let real = floor_quad(0.0, 0.0, 4.0, 2.0, 0.0);
    let sliver = [
        [0.0, 0.0, 0.0],
        [0.0, 0.0, 0.000_000_5],
        [1.0, 0.0, 1.0],
        [1.0, 0.0, 1.0],
    ];
    assert!(plan.stamp_emitted(&mut vertices, 0, PatchKind::Floor, real, None));
    assert!(!plan.stamp_emitted(&mut vertices, 6, PatchKind::Wall, sliver, None));
    assert!(!plan.failed(), "a sliver is not a build failure");
    assert_eq!(plan.failure(), None);
    assert_eq!(plan.slivers_skipped(), 1);
    assert_eq!(plan.chart_count(), 1, "the real quad still charted");
    for vertex in &vertices[..6] {
        assert!(vertex.is_lightmapped());
    }
    for vertex in &vertices[6..] {
        assert!(!vertex.is_lightmapped(), "sliver stays vertex-lit");
    }
}

#[test]
fn plan_fails_over_on_a_visible_malformed_quad() {
    // A bow-tie is *visible*: leaving it unlit would paint a bright unlit patch
    // on the level, so the plan fails over to the exact vertex-lit mesh instead
    // of skipping it.
    let config = LightmapConfig::for_profile(crate::quality::QualityProfile::Full);
    let mut plan = LightmapPlan::new(config);
    let mut vertices = vec![crate::render::Vertex::UNLIT; 6];
    let bow_tie = [
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 1.0],
        [1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0],
    ];
    assert!(!plan.stamp_emitted(&mut vertices, 0, PatchKind::Wall, bow_tie, None));
    assert_eq!(plan.failure(), Some(LightmapFailure::DegenerateQuad));
    assert!(plan.failed());
    assert_eq!(plan.slivers_skipped(), 0);
    for vertex in &vertices {
        assert!(!vertex.is_lightmapped());
    }
}

#[test]
fn plan_reports_page_overflow() {
    let config = LightmapConfig {
        max_pages: 1,
        ..LightmapConfig::for_profile(crate::quality::QualityProfile::Full)
    };
    let mut plan = LightmapPlan::new(config);
    let span = config.max_chart_span_m();
    let big = floor_quad(0.0, 0.0, span, span, 0.0);
    let mut vertices = vec![crate::render::Vertex::UNLIT; 6];
    assert!(plan.stamp_emitted(&mut vertices, 0, PatchKind::Floor, big, None));
    assert!(!plan.stamp_emitted(&mut vertices, 0, PatchKind::Floor, big, None));
    assert_eq!(plan.failure(), Some(LightmapFailure::PageOverflow));
}

#[test]
fn plan_stamps_a_folded_triangle_without_a_phantom_corner() {
    // A triangle emitted in the quad form repeats its last corner. The quad's
    // `(1, 1)` corner does not exist for it: `p2` lies on the v = 1 edge, so
    // stamping it with u = 1 would stretch the chart across a corner the
    // triangle never reaches.
    let config = LightmapConfig::for_profile(crate::quality::QualityProfile::Full);
    let mut plan = LightmapPlan::new(config);
    let mut vertices = vec![crate::render::Vertex::UNLIT; 6];
    let p0 = [0.0, 0.0, 0.0];
    let p1 = [4.0, 0.0, 0.0];
    let p2 = [0.0, 0.0, 3.0];
    assert!(plan.stamp_emitted(&mut vertices, 0, PatchKind::Floor, [p0, p1, p2, p2], None));
    assert!(!plan.failed(), "a folded triangle is a valid patch");
    assert_eq!(plan.chart_count(), 1);
    let (_, chart) = plan.charts()[0];
    let edge = config.page_edge;
    let expected = [
        chart.uv_at(edge, 0.0, 0.0),
        chart.uv_at(edge, 1.0, 0.0),
        chart.uv_at(edge, 0.0, 1.0),
        chart.uv_at(edge, 0.0, 0.0),
        chart.uv_at(edge, 0.0, 1.0),
        chart.uv_at(edge, 0.0, 1.0),
    ];
    for (index, want) in expected.into_iter().enumerate() {
        assert_eq!(vertices[index].lightmap, want, "vertex {index}");
    }
    assert_ne!(
        vertices[2].lightmap,
        chart.uv_at(edge, 1.0, 1.0),
        "the repeated corner must not become the quad's (1, 1)"
    );
    for vertex in &vertices {
        assert!(vertex.is_lightmapped());
        assert_eq!(usize::from(vertex.lightmap_page), usize::from(chart.page));
    }
}

/// Assembles two charts of distinct per-texel values and proves the whole write
/// contract: exact HDR texel values at the chart's data offset, row-major along
/// `v`, and a gutter dilated from the chart's own nearest border texel.
#[test]
fn assemble_writes_charts_at_their_offsets_and_dilates_their_own_gutters() {
    let config = LightmapConfig::for_profile(crate::quality::QualityProfile::Full);
    let mut allocator = ChartAllocator::new(config);
    let chart_a = allocator.allocate(&patch(2.0, 2.0)).expect("chart a");
    let chart_b = allocator.allocate(&patch(2.0, 2.0)).expect("chart b");
    let charts = [(patch(2.0, 2.0), chart_a), (patch(2.0, 2.0), chart_b)];
    // Distinct values per chart and per texel, so a write at the wrong offset,
    // in the wrong order, or a gutter that sampled the neighbour cannot pass.
    let values = |seed: f32, chart: &Chart| -> Vec<LightmapTexel> {
        let width = usize::try_from(chart.width).expect("chart width");
        (0..width * usize::try_from(chart.height).expect("chart height"))
            .map(|index| {
                let i = index % width;
                let j = index / width;
                LightmapTexel {
                    irradiance: [
                        (i as f32).mul_add(0.01, seed),
                        (j as f32).mul_add(0.02, seed),
                        seed,
                    ],
                    direction: [seed * 0.5, seed * 0.25, i as f32 * 0.001],
                    axis: [0.25, 0.75],
                }
            })
            .collect()
    };
    let runs = [values(0.25, &chart_a), values(0.75, &chart_b)];
    let atlas = LightmapAtlas::assemble(&config, allocator.page_count(), &charts, &runs)
        .expect("atlas assembles");
    assert_eq!(atlas.page_count(), 1, "both charts share the one page");
    let page = &atlas.pages()[0];
    assert!(page.is_consistent());
    assert_eq!(read_page_texel(page, page.width, 0), None, "x is a bound");
    assert_eq!(read_page_texel(page, 0, page.height), None, "y is a bound");

    let padding = config.padding;
    let mut written = vec![false; usize::try_from(page.width * page.height).expect("page texels")];
    for ((_, chart), data) in charts.iter().zip(&runs) {
        let width = usize::try_from(chart.width).expect("width");
        let height = usize::try_from(chart.height).expect("height");
        for j in 0..height {
            for i in 0..width {
                let value = read_page_texel(page, chart.x + i as u32, chart.y + j as u32)
                    .expect("a data texel is inside the page");
                assert_eq!(value, data[j * width + i], "chart texel ({i}, {j})");
                written[(chart.y as usize + j) * page.width as usize + chart.x as usize + i] = true;
            }
        }
        // Every outer-rectangle texel that is not data must be the chart's own
        // nearest border texel, never a sample across the reserved gutter.
        for y in chart.y - padding..chart.y + chart.height + padding {
            for x in chart.x - padding..chart.x + chart.width + padding {
                let source_x = x.clamp(chart.x, chart.x + chart.width - 1);
                let source_y = y.clamp(chart.y, chart.y + chart.height - 1);
                let expected =
                    data[(source_y - chart.y) as usize * width + (source_x - chart.x) as usize];
                let value = read_page_texel(page, x, y).expect("an outer texel is inside the page");
                assert_eq!(value, expected, "gutter texel ({x}, {y})");
                written[y as usize * page.width as usize + x as usize] = true;
            }
        }
    }
    // No texel outside both reserved outer rectangles was ever touched.
    for y in 0..page.height {
        for x in 0..page.width {
            if !written[y as usize * page.width as usize + x as usize] {
                assert_eq!(
                    read_page_texel(page, x, y),
                    Some(LightmapTexel::ZERO),
                    "unwritten texel ({x}, {y})"
                );
            }
        }
    }
}

#[test]
fn assemble_rejects_wrong_length_and_non_finite_texel_runs() {
    let config = LightmapConfig::for_profile(crate::quality::QualityProfile::Full);
    let mut allocator = ChartAllocator::new(config);
    let chart = allocator.allocate(&patch(1.0, 1.0)).expect("chart");
    let charts = [(patch(1.0, 1.0), chart)];
    let expected = (chart.width * chart.height) as usize;
    assert!(expected > 1, "the fixture chart has more than one texel");

    // `charts` and `texels` are parallel: one run per chart, in plan order.
    assert_eq!(
        LightmapAtlas::assemble(&config, allocator.page_count(), &charts, &[]),
        Err(LightmapFailure::FillSize)
    );
    // A short or a long run is a fill that does not describe the chart.
    assert_eq!(
        LightmapAtlas::assemble(
            &config,
            allocator.page_count(),
            &charts,
            &[vec![LightmapTexel::ZERO; expected - 1]]
        ),
        Err(LightmapFailure::FillSize)
    );
    assert_eq!(
        LightmapAtlas::assemble(
            &config,
            allocator.page_count(),
            &charts,
            &[vec![LightmapTexel::ZERO; expected + 1]]
        ),
        Err(LightmapFailure::FillSize)
    );
    // A non-finite channel anywhere in the run is a named failure, not a page.
    let mut nan = vec![LightmapTexel::ZERO; expected];
    nan[expected / 2].direction[1] = f32::NAN;
    assert_eq!(
        LightmapAtlas::assemble(&config, allocator.page_count(), &charts, &[nan]),
        Err(LightmapFailure::FillNonFinite)
    );
    let mut infinite = vec![LightmapTexel::ZERO; expected];
    infinite[0].irradiance[2] = f32::INFINITY;
    assert_eq!(
        LightmapAtlas::assemble(&config, allocator.page_count(), &charts, &[infinite]),
        Err(LightmapFailure::FillNonFinite)
    );
    // The positive control keeps the rejection list honest.
    assert!(
        LightmapAtlas::assemble(
            &config,
            allocator.page_count(),
            &charts,
            &[vec![LightmapTexel::ZERO; expected]]
        )
        .is_ok(),
        "an exact, finite run assembles"
    );
}

#[test]
fn layout_validation_rejects_overflow_and_out_of_page_charts() {
    let config = LightmapConfig::for_profile(crate::quality::QualityProfile::Full);
    let mut allocator = ChartAllocator::new(config);
    let chart = allocator.allocate(&patch(1.0, 1.0)).expect("chart");
    let charts = [(patch(1.0, 1.0), chart)];
    assert_eq!(
        LightmapAtlas::validate_layout(&config, allocator.page_count(), &charts),
        Ok(())
    );
    // More pages than the budget is an overflow, not a layout bug.
    assert_eq!(
        LightmapAtlas::validate_layout(&config, config.max_pages + 1, &charts),
        Err(LightmapFailure::PageOverflow)
    );
    // A chart that names a page the plan does not have.
    assert_eq!(
        LightmapAtlas::validate_layout(&config, 1, &[(charts[0].0, Chart { page: 1, ..chart })]),
        Err(LightmapFailure::Layout)
    );
    // A zero-sized data rectangle has no texels to write.
    assert_eq!(
        LightmapAtlas::validate_layout(&config, 1, &[(charts[0].0, Chart { width: 0, ..chart })]),
        Err(LightmapFailure::Layout)
    );
    // A data rectangle that runs past the page edge cannot be written.
    assert_eq!(
        LightmapAtlas::validate_layout(
            &config,
            1,
            &[(
                charts[0].0,
                Chart {
                    x: config.page_edge,
                    ..chart
                }
            )]
        ),
        Err(LightmapFailure::Layout)
    );
    // A config that cannot describe a page at all is rejected up front.
    let no_pages = LightmapConfig {
        page_edge: 0,
        ..config
    };
    assert_eq!(
        LightmapAtlas::validate_layout(&no_pages, 0, &[]),
        Err(LightmapFailure::InvalidConfig)
    );
}

#[test]
fn a_page_encodes_as_a_tone_mapped_decodable_png() {
    let config = LightmapConfig::for_profile(crate::quality::QualityProfile::Low);
    let chart = Chart {
        page: 0,
        x: config.padding,
        y: config.padding,
        width: 2,
        height: 1,
    };
    let dim = LightmapTexel {
        irradiance: [0.25; 3],
        direction: [0.0; 3],

        axis: [0.5, 0.5],
    };
    let bright = LightmapTexel {
        irradiance: [1.0; 3],
        direction: [0.0; 3],

        axis: [0.5, 0.5],
    };
    let atlas = LightmapAtlas::assemble(
        &config,
        1,
        &[(patch(1.0, 1.0), chart)],
        &[vec![dim, bright]],
    )
    .expect("atlas assembles");
    let page = &atlas.pages()[0];
    let bytes = page_png_bytes(page).expect("page encodes");
    let decoded = crate::materials::decode_png(&bytes).expect("page decodes");
    assert_eq!(decoded.width, page.width);
    assert_eq!(decoded.height, page.height);
    // The PNG is a developer preview in display space: every channel passes
    // through the same soft knee the shader uses, not the raw HDR value.
    let byte = |value: f32| {
        let display = crate::lighting::transport::soft_clip_channel(value);
        (display.clamp(0.0, 1.0).mul_add(255.0, 0.5)) as u8
    };
    let pixel = |x: u32, y: u32| -> [u8; 4] {
        let offset = ((y * page.width + x) * 4) as usize;
        decoded.rgba[offset..offset + 4]
            .try_into()
            .expect("one RGBA pixel")
    };
    // The two data texels tone-map to different preview bytes.
    assert_eq!(
        pixel(chart.x, chart.y),
        [byte(0.25), byte(0.25), byte(0.25), 255]
    );
    assert_eq!(
        pixel(chart.x + 1, chart.y),
        [byte(1.0), byte(1.0), byte(1.0), 255]
    );
    assert_ne!(pixel(chart.x, chart.y), pixel(chart.x + 1, chart.y));
    // The gutter copies keep the border texel's bytes; untouched page texels
    // tone-map black with an opaque alpha.
    assert_eq!(
        pixel(chart.x - 1, chart.y),
        [byte(0.25), byte(0.25), byte(0.25), 255]
    );
    assert_eq!(
        pixel(chart.x + 2, chart.y),
        [byte(1.0), byte(1.0), byte(1.0), 255]
    );
    assert_eq!(pixel(page.width - 1, page.height - 1), [0, 0, 0, 255]);
}

/// One solved chart's receiver grid must be the full patch texel grid, row-major
/// along `v` with `u` the fast axis, and every position exactly the world point
/// `texel_axis` names (already offset off the surface).
fn assert_chart_receivers(
    solved: &crate::lighting::transport::SolvedChart,
    patch: &LightmapPatch,
    chart: &Chart,
    shared_shape: (usize, usize),
) {
    use crate::lighting::transport::{SURFACE_OFFSET_M, patch_normal, texel_axis};
    let width = usize::try_from(chart.width).expect("chart width");
    let height = usize::try_from(chart.height).expect("chart height");
    assert_eq!((width, height), shared_shape);
    assert_eq!(solved.receivers.len(), width * height);
    assert_eq!(solved.texels.len(), width * height);
    let normal = patch_normal(patch);
    for j in 0..height {
        for i in 0..width {
            // Row-major along `v`: `j` is the slow axis, `i` the fast one.
            let receiver = solved.receivers[j * width + i];
            assert_eq!(receiver.normal, normal);
            let point = patch.point_at(texel_axis(i, width), texel_axis(j, height));
            let expected = [
                normal[0].mul_add(SURFACE_OFFSET_M, point[0]),
                normal[1].mul_add(SURFACE_OFFSET_M, point[1]),
                normal[2].mul_add(SURFACE_OFFSET_M, point[2]),
            ];
            assert_eq!(receiver.position, expected, "receiver ({i}, {j})");
        }
    }
}

/// The transport receiver walk must span each chart's patch inclusively and
/// row-major along `v`, exactly like the historical fill: two charts that share
/// a geometric edge then evaluate the *same world points* along it, which is
/// what keeps a shared edge seamless before the atlas even exists.
#[test]
fn transport_receivers_span_each_patch_and_meet_on_a_shared_edge() {
    use crate::lighting::transport::{
        SURFACE_OFFSET_M, SolveOptions, TransportScene, patch_normal, texel_axis,
    };
    // Two coplanar 2 x 2 m floor patches sharing the x = 2 edge; one chart each.
    let patch_a = LightmapPatch {
        origin: [0.0, 0.0, 0.0],
        u_axis: [2.0, 0.0, 0.0],
        v_axis: [0.0, 0.0, 2.0],
        room: None,
        kind: PatchKind::Floor,
    };
    let patch_b = LightmapPatch {
        origin: [2.0, 0.0, 0.0],
        ..patch_a
    };
    let charts = [
        (
            patch_a,
            Chart {
                page: 0,
                x: 0,
                y: 0,
                width: 4,
                height: 3,
            },
        ),
        (
            patch_b,
            Chart {
                page: 0,
                x: 4,
                y: 0,
                width: 4,
                height: 3,
            },
        ),
    ];
    // The receivers come from the charts, so an empty static scene and no
    // emitters still exercise the receiver walk.
    let scene = TransportScene::new(Vec::new(), Vec::new()).expect("an empty scene is valid");
    let solution = scene
        .solve(
            &charts,
            SolveOptions {
                taps_per_axis: 1,
                bounces: 0,
                gather_samples: 1,
                workers: 1,
            },
            None,
        )
        .expect("the receiver walk solves");
    assert_eq!(solution.charts.len(), charts.len());
    let width = usize::try_from(charts[0].1.width).expect("width");
    let height = usize::try_from(charts[0].1.height).expect("height");
    assert_eq!(texel_axis(0, width), 0.0, "the first texel is the 0 edge");
    assert_eq!(
        texel_axis(width - 1, width),
        1.0,
        "the last texel is the 1 edge"
    );
    assert_eq!(
        texel_axis(0, 1),
        0.5,
        "a single-texel axis samples the middle"
    );
    for (index, (patch, chart)) in charts.iter().enumerate() {
        assert_chart_receivers(&solution.charts[index], patch, chart, (width, height));
    }
    // The four chart corners sit exactly on the patch's geometric corners.
    let a = &solution.charts[0].receivers;
    let b = &solution.charts[1].receivers;
    let shifted = |point: [f32; 3]| {
        let normal = patch_normal(&patch_a);
        [
            normal[0].mul_add(SURFACE_OFFSET_M, point[0]),
            normal[1].mul_add(SURFACE_OFFSET_M, point[1]),
            normal[2].mul_add(SURFACE_OFFSET_M, point[2]),
        ]
    };
    assert_eq!(a[0].position, shifted(patch_a.point_at(0.0, 0.0)));
    assert_eq!(
        a[(height - 1) * width].position,
        shifted(patch_a.point_at(0.0, 1.0))
    );
    assert_eq!(b[width - 1].position, shifted(patch_b.point_at(1.0, 0.0)));
    assert_eq!(
        b[width * height - 1].position,
        shifted(patch_b.point_at(1.0, 1.0))
    );
    // The shared edge evaluates at one world point on both sides.
    for j in 0..height {
        assert_eq!(
            a[j * width + (width - 1)].position,
            b[j * width].position,
            "row {j} of the shared x = 2 edge"
        );
    }
}

#[test]
fn content_key_is_stable_and_changes_with_the_inputs() {
    let level = crate::level::LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "key",
            "name": "Key",
            "spawn": { "x": 0.0, "z": 0.0 },
            "rooms": [{ "x": 0.0, "z": 0.0, "width": 8.0, "depth": 6.0, "height": 3.0 }],
            "ceiling_lights": [{ "fixture": "core:fluorescent_panel_01", "x": 4.0, "z": 3.0 }],
            "props": [{ "model": "core:chair", "x": 1.0, "z": 1.0 }]
        }"#,
    )
    .expect("level parses");
    let config = LightmapConfig::for_profile(crate::quality::QualityProfile::Full);
    let key = content_key(&level, &config, crate::quality::QualityProfile::Full);
    assert_eq!(
        key,
        content_key(&level, &config, crate::quality::QualityProfile::Full)
    );

    let mut moved_light = level.clone();
    moved_light.ceiling_lights[0].x += 0.5;
    assert_ne!(
        key,
        content_key(&moved_light, &config, crate::quality::QualityProfile::Full)
    );

    let mut moved_prop = level.clone();
    moved_prop.props[0].y += 0.25;
    assert_ne!(
        key,
        content_key(&moved_prop, &config, crate::quality::QualityProfile::Full)
    );

    let low = LightmapConfig::for_profile(crate::quality::QualityProfile::Low);
    assert_ne!(
        key,
        content_key(&level, &low, crate::quality::QualityProfile::Low)
    );
    assert!(!key.is_empty());

    // Every config field the atlas layout depends on is part of the key, so a
    // retune can never resolve to pages an older configuration produced.
    for mutated in [
        LightmapConfig {
            texels_per_metre: config.texels_per_metre + 0.5,
            ..config
        },
        LightmapConfig {
            page_edge: config.page_edge * 2,
            ..config
        },
        LightmapConfig {
            max_pages: config.max_pages + 1,
            ..config
        },
        LightmapConfig {
            padding: config.padding + 1,
            ..config
        },
        LightmapConfig {
            bytes_per_texel: config.bytes_per_texel + 1,
            ..config
        },
    ] {
        assert_ne!(
            key,
            content_key(&level, &mutated, crate::quality::QualityProfile::Full),
            "a config change must mint a new key: {mutated:?}"
        );
    }

    // The renderer's key folds the solver and occluder fingerprints in as
    // extra bytes; identical extras keep the key, a changed byte does not.
    let extra = [7u8; 16];
    assert_eq!(
        content_key_with_extra(
            &level,
            &config,
            crate::quality::QualityProfile::Full,
            &extra
        ),
        content_key_with_extra(
            &level,
            &config,
            crate::quality::QualityProfile::Full,
            &extra
        )
    );
    let mut changed = extra;
    changed[15] ^= 1;
    assert_ne!(
        content_key_with_extra(
            &level,
            &config,
            crate::quality::QualityProfile::Full,
            &extra
        ),
        content_key_with_extra(
            &level,
            &config,
            crate::quality::QualityProfile::Full,
            &changed
        )
    );
}

/// The level definition's map fields are hashed through the canonical JSON
/// writer, so two level files that differ only in the authored key order of
/// their objects must mint the same key. A direct serialisation would depend on
/// `HashMap` iteration order and could silently evict a valid entry.
#[test]
fn content_key_is_canonical_over_reordered_json_fields() {
    let config = LightmapConfig::for_profile(crate::quality::QualityProfile::Full);
    let first = crate::level::LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "canonical_key",
            "name": "Canonical Key",
            "spawn": { "x": 0.0, "z": 0.0 },
            "rooms": [{ "width": 8.0, "x": 0.0, "depth": 6.0, "z": 0.0, "height": 3.0 }],
            "walls": [
                { "x": 3.0, "z": 0.0, "width": 0.2, "depth": 6.0, "height": 3.0,
                  "faces": { "west": "core:wall_brick", "east": "core:wall_panel" } }
            ],
            "ceiling_lights": [
                { "fixture": "core:fluorescent_panel_01", "x": 4.0, "z": 3.0 }
            ]
        }"#,
    )
    .expect("the first level parses");
    let second = crate::level::LevelDef::from_json(
        r#"{
            "id": "canonical_key",
            "format_version": 3,
            "name": "Canonical Key",
            "spawn": { "z": 0.0, "x": 0.0 },
            "rooms": [{ "z": 0.0, "height": 3.0, "depth": 6.0, "x": 0.0, "width": 8.0 }],
            "walls": [
                { "width": 0.2, "depth": 6.0, "height": 3.0, "z": 0.0, "x": 3.0,
                  "faces": { "east": "core:wall_panel", "west": "core:wall_brick" } }
            ],
            "ceiling_lights": [
                { "z": 3.0, "x": 4.0, "fixture": "core:fluorescent_panel_01" }
            ]
        }"#,
    )
    .expect("the second level parses");
    assert_eq!(
        crate::canonical_json::canonical_json_bytes(&first).expect("the first level serialises"),
        crate::canonical_json::canonical_json_bytes(&second).expect("the second level serialises"),
        "canonical JSON must not depend on the authored key order"
    );
    assert_eq!(
        content_key(&first, &config, crate::quality::QualityProfile::Full),
        content_key(&second, &config, crate::quality::QualityProfile::Full)
    );
}

use super::{LevelLightmaps, LightmapCache, LightmapPage, LightmapStats, SwitchableLightmaps};

#[test]
fn memory_cache_returns_the_same_allocation_and_clears() {
    let mut cache = LightmapCache::memory_only();
    let lightmaps = std::sync::Arc::new(LevelLightmaps {
        pages: Vec::new(),
        charts: Vec::new(),
        stats: LightmapStats::default(),
        cache_key: "key".to_string(),
        padding: 2,
        switchable: Vec::new(),
    });
    assert!(cache.get("key").is_none());
    cache.insert("key", std::sync::Arc::clone(&lightmaps));
    assert!(std::sync::Arc::ptr_eq(
        &cache.get("key").expect("hit"),
        &lightmaps
    ));
    assert!(cache.get("other").is_none());
    cache.clear_memory();
    assert!(cache.get("key").is_none());
}

/// The cache key is the atlas's identity: an entry whose recorded `cache_key` is
/// not the key it is stored under must never be served (a stale or renamed atlas
/// would otherwise light the level with another configuration's pages), and an
/// unsafe key must never enter the store at all.
#[test]
fn memory_cache_rejects_a_key_mismatch_and_unsafe_keys() {
    let mut cache = LightmapCache::memory_only();
    let atlas = std::sync::Arc::new(cache_fixture("right", 0.5));
    cache.insert("wrong", std::sync::Arc::clone(&atlas));
    assert!(cache.get("wrong").is_none(), "a mismatched key is a miss");
    assert!(cache.get("right").is_none(), "and nothing was stored");
    assert!(cache.is_empty());

    let long = "x".repeat(129);
    let unsafe_keys = ["", "../escape", "with space", "non_ascii_é", long.as_str()];
    for key in unsafe_keys {
        cache.insert(key, std::sync::Arc::new(cache_fixture(key, 0.25)));
        assert!(
            cache.get(key).is_none(),
            "unsafe key {key:?} must not store"
        );
    }
    assert!(cache.is_empty());

    cache.insert("right", std::sync::Arc::clone(&atlas));
    assert!(std::sync::Arc::ptr_eq(
        &cache.get("right").expect("the matching key stores"),
        &atlas
    ));
}

/// The uploaded array layers are the base page pairs followed by one pair per
/// switchable contribution; `irradiance_layer` must name exactly those even
/// layers, densely, so the shader's offset arithmetic and the package writer can
/// never disagree about which layer holds which plane.
#[test]
fn switchable_groups_follow_the_base_layers_in_a_dense_pair_layout() {
    let page = |value: f32| LightmapPage {
        width: 2,
        height: 2,
        texels: vec![
            LightmapTexel {
                irradiance: [value; 3],
                direction: [0.0; 3],

                axis: [0.5, 0.5],
            };
            4
        ],
    };
    for (pages, switchable) in [(1usize, 0usize), (1, 2), (3, 0), (3, 2), (4, 3)] {
        let lightmaps = LevelLightmaps {
            pages: (0..pages).map(|_| page(0.5)).collect(),
            charts: Vec::new(),
            stats: LightmapStats::default(),
            cache_key: "layout".to_string(),
            padding: 1,
            switchable: (0..switchable)
                .map(|light_index| SwitchableLightmaps {
                    light_index,
                    pages: (0..pages).map(|_| page(0.25)).collect(),
                })
                .collect(),
        };
        assert_eq!(
            lightmaps.layer_count(),
            pages * 2 * (switchable + 1),
            "{pages} page(s), {switchable} switchable group(s)"
        );
        let mut seen = vec![false; lightmaps.layer_count()];
        for group in 0..=switchable {
            for page_index in 0..pages {
                let switchable_index = (group > 0).then(|| group - 1);
                let layer = lightmaps.irradiance_layer(switchable_index, page_index);
                assert_eq!(layer, group * pages * 2 + page_index * 2);
                assert_eq!(layer % 2, 0, "irradiance is the even layer of the pair");
                assert!(layer < lightmaps.layer_count());
                assert!(!seen[layer], "layer {layer} is claimed twice");
                seen[layer] = true;
            }
        }
        assert!(
            seen.iter()
                .enumerate()
                .all(|(layer, claimed)| *claimed == (layer % 2 == 0)),
            "every even layer is exactly one resident irradiance plane, and the odd \
             layers stay the direction planes: {seen:?}"
        );
    }
}

/// The format version is bumped whenever the atlas layout, texel encoding or key
/// inputs change; version 12 is the offline HDR transport solve (an irradiance
/// term plus a directional moment per texel, and prepared switchable layer
/// groups). Pinned so a future layout change has to bump it deliberately, and
/// part of every key so a pre-12 atlas can never be reused.
#[test]
fn the_format_version_is_current_and_is_part_of_every_key_prefix() {
    // The value itself is pinned by the cache module's version notes; the
    // contract under test is that the key carries it.
    assert_eq!(LIGHTMAP_FORMAT_VERSION, 12);
    let level = crate::level::LevelDef::from_json(
        r#"{
            "format_version": 3,
            "id": "version_key",
            "name": "Version Key",
            "spawn": { "x": 0.0, "z": 0.0 },
            "rooms": [{ "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0, "height": 3.0 }]
        }"#,
    )
    .expect("the version-key level parses");
    let config = LightmapConfig::for_profile(crate::quality::QualityProfile::Full);
    let key = content_key(&level, &config, crate::quality::QualityProfile::Full);
    assert!(
        key.starts_with(&format!("v{LIGHTMAP_FORMAT_VERSION}-")),
        "{key}"
    );
    // An older-version key is a different string: the new key can never equal
    // one minted under an older format even for byte-identical level content
    // and configuration.
    assert!(!key.starts_with(&format!("v{}-", LIGHTMAP_FORMAT_VERSION - 1)));
}

#[test]
fn an_oversized_patch_is_clamped_to_a_page_not_dropped() {
    let config = LightmapConfig::for_profile(crate::quality::QualityProfile::Full);
    let mut allocator = ChartAllocator::new(config);
    // 400 m of floor is 6400 texels at this density: far past one page.
    let huge = patch(400.0, 2.0);
    let chart = allocator
        .allocate(&huge)
        .expect("a patch larger than a page must still be charted");
    assert_eq!(chart.width, config.usable_edge());
    assert!(!allocator.failed());
    assert!(chart.x + chart.width <= config.page_edge);
}

#[test]
fn chart_texels_are_clamped_to_the_usable_edge() {
    let config = LightmapConfig::for_profile(crate::quality::QualityProfile::Full);
    let huge = patch(400.0, 2.0);
    assert_eq!(
        config.chart_texels(&huge),
        (
            config.usable_edge(),
            (2.0 * config.texels_per_metre).ceil() as u32
        )
    );
}

/// The shipped level must keep real lightmaps under both profiles, and both
/// profiles must bake the *same* patch set.
///
/// This is the density/packing regression guard: `Low` used to overflow its
/// 512-texel pages and fall back to vertex lighting for the whole level, and a
/// density bump that overflows is worse than no bump at all. It also pins that
/// the bake uses a meaningful part of the pages it opens rather than "fitting"
/// by accident at a trivial density, and that a profile decides texel density
/// and page size, never where the geometry is cut.
#[test]
fn the_shipped_demo_fits_the_page_budget_at_both_profiles() {
    let level =
        crate::level::LevelDef::from_json(include_str!("../../../assets/levels/places_demo.json"))
            .expect("the shipped places_demo parses");
    let mut sets = Vec::new();
    for profile in crate::quality::QualityProfile::ALL {
        let config = profile.lightmap_config();
        let build = build_level(&level, profile);
        let lightmaps = build.lightmaps.as_deref().unwrap_or_else(|| {
            panic!(
                "{profile:?} must keep its lightmaps on the demo: {:?}",
                build.lightmap_failure
            )
        });
        assert!(
            lightmaps.chart_count() > 900,
            "the demo's chart set is complete"
        );
        assert!(
            lightmaps.pages.len() <= config.max_pages,
            "{profile:?} must fit its page budget"
        );
        let budget =
            u64::from(config.page_edge).pow(2) * u64::try_from(lightmaps.pages.len()).unwrap_or(0);
        assert!(
            lightmaps.stats.texels as u64 * 2 > budget,
            "{profile:?} must use more than half of its budget: {} of {budget}",
            lightmaps.stats.texels
        );
        sets.push(patch_set(lightmaps));
    }
    assert_eq!(
        sets[0], sets[1],
        "Low and Full must bake the same patch set on the demo"
    );
}

/// Builds one level through the same public entry point the audits use.
fn build_level(
    level: &crate::level::LevelDef,
    profile: QualityProfile,
) -> crate::render::LevelBuild {
    let materials = crate::render::logical_materials(level);
    let catalog = crate::loader::PropCatalog::builtin();
    let mut assets = crate::props::PropAssets::default();
    crate::render::build_level_geometry_timed_with_lightmaps(
        level,
        &catalog,
        &mut assets,
        &materials,
        crate::render::LightmapBuildOptions::for_profile(profile, LightmapMode::On),
        None,
    )
}

/// Identity of one chart's patch, for the "both profiles share one patch set"
/// equality check.
type PatchIdentity = (PatchKind, [f32; 3], [f32; 3], [f32; 3], Option<usize>);

/// The patch set of one atlas, for the "both profiles share one patch set"
/// equality check.
fn patch_set(lightmaps: &LevelLightmaps) -> Vec<PatchIdentity> {
    lightmaps
        .charts
        .iter()
        .map(|(patch, _)| {
            (
                patch.kind,
                patch.origin,
                patch.u_axis,
                patch.v_axis,
                patch.room,
            )
        })
        .collect()
}

/// A synthetic two-storey tower whose two 55 x 55 m floors plus their ceilings
/// need more than the historical two-page budget but fit the shipped eight. It
/// pins the capacity contract on a clean checkout, where the drop-in Pit is not
/// present.
fn large_tower_level() -> crate::level::LevelDef {
    large_tower_level_with_storeys(2)
}

/// The same 55 m x 55 m tower with an arbitrary storey count, so the page-budget
/// boundary can be pinned exactly (three storeys exceed four pages at Full and
/// fit the raised eight).
fn large_tower_level_with_storeys(storeys: u32) -> crate::level::LevelDef {
    let mut rooms: Vec<String> = Vec::new();
    let mut lights: Vec<String> = Vec::new();
    for storey in 0..storeys {
        let floor_y = -6.0 * storey as f32;
        rooms.push(format!(
            r#"{{"x": 0.0, "z": 0.0, "width": 55.0, "depth": 55.0, "height": 6.0, "floor_y": {floor_y}}}"#
        ));
        for ix in 0..3 {
            for iz in 0..3 {
                let x = 18.0f32.mul_add(ix as f32, 9.0);
                let z = 18.0f32.mul_add(iz as f32, 9.0);
                lights.push(format!(
                    r#"{{"fixture": "core:fluorescent_panel_01", "x": {x}, "z": {z}, "brightness": 0.5}}"#
                ));
            }
        }
    }
    let json = format!(
        r#"{{
            "format_version": 3,
            "id": "large_tower",
            "name": "Large Tower",
            "spawn": {{ "x": 9.0, "z": 9.0 }},
            "rooms": [{}],
            "ceiling_lights": [{}]
        }}"#,
        rooms.join(", "),
        lights.join(", ")
    );
    crate::level::LevelDef::from_json(&json).expect("the synthetic tower parses")
}

/// Builds one level with an explicit lightmap configuration.
fn build_with_config(
    level: &crate::level::LevelDef,
    config: LightmapConfig,
) -> crate::render::LevelBuild {
    let materials = crate::render::logical_materials(level);
    let catalog = crate::loader::PropCatalog::builtin();
    let mut assets = crate::props::PropAssets::default();
    let profile = QualityProfile::Full;
    crate::render::build_level_geometry_timed_with_lightmaps(
        level,
        &catalog,
        &mut assets,
        &materials,
        crate::render::LightmapBuildOptions {
            mode: LightmapMode::On,
            config,
            ..crate::render::LightmapBuildOptions::for_profile(profile, LightmapMode::On)
        },
        None,
    )
}

/// The capacity regression: The Pit's 2,808 m² of floor plus the same ceiling
/// (25 rooms, 114 fixtures) exceeded the historical two-page budget at Full and
/// the whole level fell back to vertex lighting, which removed the per-texel
/// light from every room. The shipped budget must hold a level of that size, a
/// two-page budget must still fail over by name rather than drop pages
/// silently, and the 2026 raise to eight pages must hold a three-storey tower
/// that four pages cannot.
///
/// The synthetic towers pin every half of that contract on each checkout. The
/// drop-in `levels/level0_pit.json` is a per-user file that is not committed, so
/// when it is present its real build is checked too; when it is absent the test
/// still covers the capacity boundary.
#[test]
fn the_pit_bakes_into_the_shipped_page_budget_at_full() {
    let profile = QualityProfile::Full;
    let config = profile.lightmap_config();
    assert_eq!(
        config.max_pages, 8,
        "the shipped profile supports the shared eight-page budget"
    );
    assert_eq!(config.page_edge, 1024);

    let tower = large_tower_level();

    // The historical two-page budget must overflow: this is the regression the
    // first capacity raise exists for.
    let mut two_page = config;
    two_page.max_pages = 2;
    let overflow = build_with_config(&tower, two_page);
    assert_eq!(
        overflow.lightmap_failure,
        Some(super::LightmapFailure::PageOverflow),
        "a level of this size must overflow the two-page budget by name"
    );
    assert!(overflow.lightmaps.is_none());
    assert!(
        overflow.mesh.vertex_count > 0,
        "the fallback mesh is complete"
    );

    // The shipped budget must hold the two-storey tower too.
    let build = build_with_config(&tower, config);
    assert_eq!(
        build.lightmap_failure, None,
        "the synthetic tower must bake cleanly at Full"
    );
    let lightmaps = build
        .lightmaps
        .as_deref()
        .unwrap_or_else(|| panic!("Full must produce an atlas for the tower"));
    assert!(!lightmaps.pages.is_empty());
    assert!(
        lightmaps.pages.len() <= config.max_pages,
        "{} pages over the {}-page budget",
        lightmaps.pages.len(),
        config.max_pages
    );
    assert!(lightmaps.chart_count() > 0, "Full must chart the level");
    assert!(lightmaps.stats.texels > 0, "Full must fill real texels");
    assert_architecture_is_lightmapped(&build, lightmaps);
}

/// The 2026 raise from four pages to eight: a third 55 m x 55 m storey must
/// overflow the old four-page budget by name (falling back to vertex lighting),
/// and the shipped eight-page budget must bake it with more than four pages
/// actually resident. Without this, the raise would be an unproven constant.
#[test]
fn the_three_storey_tower_needs_the_raised_page_budget() {
    let profile = QualityProfile::Full;
    let config = profile.lightmap_config();
    assert_eq!(config.max_pages, 8);
    let taller = large_tower_level_with_storeys(3);
    let mut four_page = config;
    four_page.max_pages = 4;
    let four = build_with_config(&taller, four_page);
    assert_eq!(
        four.lightmap_failure,
        Some(super::LightmapFailure::PageOverflow),
        "the three-storey tower must overflow a four-page budget"
    );
    assert!(four.lightmaps.is_none());
    let raised = build_with_config(&taller, config);
    assert_eq!(
        raised.lightmap_failure, None,
        "the eight-page budget must bake the three-storey tower"
    );
    let maps = raised
        .lightmaps
        .as_deref()
        .unwrap_or_else(|| panic!("the three-storey tower must produce an atlas"));
    assert!(
        maps.pages.len() > 4,
        "the fixture must actually need more than four pages: {} used",
        maps.pages.len()
    );

    // The real drop-in level, when the user has it installed.
    if let Ok(content) = std::fs::read_to_string("levels/level0_pit.json") {
        let level = crate::level::LevelDef::from_json(&content).expect("the drop-in Pit parses");
        let real = build_level(&level, profile);
        assert_eq!(
            real.lightmap_failure, None,
            "The Pit must bake cleanly at Full"
        );
        let real_maps = real
            .lightmaps
            .as_deref()
            .unwrap_or_else(|| panic!("Full must produce an atlas for The Pit"));
        assert!(
            real_maps.pages.len() <= config.max_pages,
            "The Pit: {} pages over the {}-page budget",
            real_maps.pages.len(),
            config.max_pages
        );

        assert_architecture_is_lightmapped(&real, real_maps);
    }
}

/// Every architectural vertex of a lightmapped build must sample a resident
/// page: only invisible sub-texel slivers may stay vertex-lit, and the highest
/// page byte must be the last resident page, so an unpopulated layer can never
/// ship. This replaces the historical wall-texel comparison against the
/// display-space fill: the prepared HDR pages are the transport solve's own
/// values (covered by `crate::lighting::transport::tests`), and the vertex-lit
/// model they were once compared with is the `off` fallback.
fn assert_architecture_is_lightmapped(
    build: &crate::render::LevelBuild,
    lightmaps: &LevelLightmaps,
) {
    let mut architectural = 0usize;
    let mut lightmapped = 0usize;
    let mut highest_page = 0usize;
    for range in &build.mesh.ranges {
        if !matches!(
            range.key.kind,
            crate::render::SurfaceKind::Floor
                | crate::render::SurfaceKind::Ceiling
                | crate::render::SurfaceKind::Wall
        ) {
            continue;
        }
        for vertex in &range.vertices {
            architectural += 1;
            if vertex.is_lightmapped() {
                lightmapped += 1;
                let page = usize::from(vertex.lightmap_page);
                assert!(
                    page < lightmaps.pages.len(),
                    "the vertex page byte must address a resident layer"
                );
                highest_page = highest_page.max(page);
            }
        }
    }
    assert!(architectural > 0, "the level emits architectural geometry");
    assert!(
        lightmapped * 100 >= architectural * 99,
        "the atlas must cover the architecture: {lightmapped} of {architectural} vertices"
    );
    assert_eq!(
        highest_page + 1,
        lightmaps.pages.len(),
        "every resident layer must be referenced by a stamped chart"
    );
}

/// Runs a level through the bake and samples a grid inside its first room.
fn samples_in_first_room(level: &crate::level::LevelDef) -> Vec<[f32; 3]> {
    let lighting =
        crate::lighting::LevelLighting::bake_with(level, QualityProfile::Full.bake_config());
    let mut out = Vec::new();
    for i in 0..=4 {
        for j in 0..=4 {
            let x = (i as f32).mul_add(2.0, 1.0);
            let z = (j as f32).mul_add(1.5, 1.0);
            let color = lighting.sample_in_room(0, x, 0.0, z);
            out.push([color.r, color.g, color.b]);
        }
    }
    out
}

/// The original two-page cap's failure mode: a level that adds an unrelated,
/// distant room blows the atlas budget and the WHOLE level loses its lightmap,
/// so a locally lit room's illumination changes even though nothing near it
/// did. With the shipped budget the added room must not change the lit room's
/// bake at all.
#[test]
fn an_unrelated_distant_room_does_not_darken_a_lit_room() {
    const LIT_ROOM: &str = r#"{
        "format_version": 3,
        "id": "capacity_lit",
        "name": "Capacity Lit",
        "spawn": { "x": 2.0, "z": 2.0 },
        "rooms": [
            { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 8.0, "height": 3.0 }
        ],
        "ceiling_lights": [
            { "fixture": "core:fluorescent_panel_01", "x": 5.0, "z": 4.0, "brightness": 0.9 }
        ]
    }"#;
    const LIT_ROOM_WITH_DISTANT_ROOM: &str = r#"{
        "format_version": 3,
        "id": "capacity_lit",
        "name": "Capacity Lit",
        "spawn": { "x": 2.0, "z": 2.0 },
        "rooms": [
            { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 8.0, "height": 3.0 },
            { "x": 200.0, "z": 200.0, "width": 70.0, "depth": 70.0, "height": 3.0 }
        ],
        "ceiling_lights": [
            { "fixture": "core:fluorescent_panel_01", "x": 5.0, "z": 4.0, "brightness": 0.9 }
        ]
    }"#;
    let lit = crate::level::LevelDef::from_json(LIT_ROOM).expect("the lit room parses");
    let enlarged = crate::level::LevelDef::from_json(LIT_ROOM_WITH_DISTANT_ROOM)
        .expect("the enlarged level parses");
    // The distant room alone is >2 pages of Full texels: under the old budget
    // this build overflowed and returned no atlas for the whole level.
    let enlarged_build = build_level(&enlarged, QualityProfile::Full);
    assert_eq!(
        enlarged_build.lightmap_failure, None,
        "the enlarged level must keep its atlas"
    );
    let enlarged_lightmaps = enlarged_build
        .lightmaps
        .as_deref()
        .expect("the enlarged level bakes an atlas");
    let config = QualityProfile::Full.lightmap_config();
    assert!(enlarged_lightmaps.pages.len() <= config.max_pages);
    assert!(
        enlarged_lightmaps.pages.len() > 2,
        "the distant room must actually push past the historical two-page budget: {} page(s)",
        enlarged_lightmaps.pages.len()
    );
    let before = samples_in_first_room(&lit);
    let after = samples_in_first_room(&enlarged);
    assert_eq!(before.len(), after.len());
    for (index, (a, b)) in before.iter().zip(after.iter()).enumerate() {
        for channel in 0..3 {
            assert!(
                (a[channel] - b[channel]).abs() <= 1.0e-6,
                "sample {index} channel {channel}: adding a distant room changed the lit room \
                 ({a:?} vs {b:?})"
            );
        }
    }
}

/// Reordering equivalent lights must not change the bake: the light list is a
/// set, not a sequence, and an order-dependent sum would make a level's light
/// depend on authoring order.
#[test]
fn reordering_equivalent_lights_does_not_change_the_bake() {
    let light = |x: f32, z: f32, brightness: f32| {
        format!(
            r#"{{ "fixture": "core:fluorescent_panel_01", "x": {x}, "z": {z}, "brightness": {brightness} }}"#
        )
    };
    let level_with = |lights: &[String]| {
        crate::level::LevelDef::from_json(&format!(
            r#"{{
                "format_version": 3,
                "id": "light_order",
                "name": "Light Order",
                "spawn": {{ "x": 2.0, "z": 2.0 }},
                "rooms": [{{ "x": 0.0, "z": 0.0, "width": 16.0, "depth": 12.0, "height": 3.0 }}],
                "ceiling_lights": [{}]
            }}"#,
            lights.join(",")
        ))
        .expect("the reordered levels parse")
    };
    let lights: Vec<String> = vec![
        light(2.0, 2.0, 0.7),
        light(8.0, 2.0, 0.5),
        light(14.0, 2.0, 0.9),
        light(2.0, 10.0, 0.4),
        light(14.0, 10.0, 0.6),
    ];
    let mut reversed = lights.clone();
    reversed.reverse();
    let forward = level_with(&lights);
    let backward = level_with(&reversed);
    let a = crate::lighting::LevelLighting::bake_with(&forward, QualityProfile::Full.bake_config());
    let b =
        crate::lighting::LevelLighting::bake_with(&backward, QualityProfile::Full.bake_config());
    for i in 0..=8 {
        for j in 0..=6 {
            let x = (i as f32).mul_add(1.8, 0.5);
            let z = (j as f32).mul_add(1.8, 0.5);
            let first = a.sample_in_room(0, x, 0.0, z);
            let second = b.sample_in_room(0, x, 0.0, z);
            for (channel, (one, two)) in [first.r, first.g, first.b]
                .iter()
                .zip([second.r, second.g, second.b].iter())
                .enumerate()
            {
                assert!(
                    (one - two).abs() <= 1.0e-5,
                    "light order changed ({x}, {z}) channel {channel}: {one} vs {two}"
                );
            }
        }
    }
    // The atlas path is order-independent too: both orders bake an atlas.
    assert_eq!(
        build_level(&forward, QualityProfile::Full).lightmap_failure,
        None
    );
    assert_eq!(
        build_level(&backward, QualityProfile::Full).lightmap_failure,
        None
    );
}

/// Developer measurement, not an assertion.
///
/// Builds the shipped demo level with the real chart set and prints, per
/// profile: chart count, chart data texels, the outer rectangle area the charts
/// reserve (data plus both gutters), the pages the two-page build needs (or a
/// generous-budget probe when it does not fit), the resulting utilisation and
/// the bake time. Run with:
///
/// ```text
/// cargo test --release measure_demo_chart_statistics -- --ignored --nocapture
/// ```
///
/// Printing is the whole point of an `#[ignore]`d measurement, so the crate's
/// `print_stdout` lint is switched off for this one test.
#[test]
#[ignore = "developer measurement: prints places_demo's chart statistics"]
#[allow(clippy::print_stdout)]
fn measure_demo_chart_statistics() {
    let level =
        crate::level::LevelDef::from_json(include_str!("../../../assets/levels/places_demo.json"))
            .expect("the shipped places_demo parses");
    // The patch set is profile-independent (the chart-span cap is shared), so
    // one successful build at Full collects the whole demo's patches.
    let materials = crate::render::logical_materials(&level);
    let catalog = crate::loader::PropCatalog::builtin();
    let mut assets = crate::props::PropAssets::default();
    let build = crate::render::build_level_geometry_timed_with_lightmaps(
        &level,
        &catalog,
        &mut assets,
        &materials,
        crate::render::LightmapBuildOptions::for_profile(
            crate::quality::QualityProfile::Full,
            LightmapMode::On,
        ),
        None,
    );
    let Some(lightmaps) = build.lightmaps.as_deref() else {
        panic!("the demo must bake at Full: {:?}", build.lightmap_failure);
    };
    let patches: Vec<LightmapPatch> = lightmaps.charts.iter().map(|(patch, _)| *patch).collect();
    for profile in crate::quality::QualityProfile::ALL {
        let config = profile.lightmap_config();
        // Pack the same patch set with a generous budget, to separate "the
        // two-page budget is too small" from "the packer is too
        // wasteful".
        let mut probe = ChartAllocator::new(LightmapConfig {
            max_pages: 64,
            ..config
        });
        for patch in &patches {
            probe.allocate(patch);
        }
        let padding = u64::from(config.padding);
        let mut data = 0u64;
        let mut outer = 0u64;
        for patch in &patches {
            let (w, h) = config.chart_texels(patch);
            let (w, h) = (u64::from(w), u64::from(h));
            data += w * h;
            outer += (w + padding * 2) * (h + padding * 2);
        }
        let edge = u64::from(config.page_edge);
        let budget = edge * edge * u64::try_from(config.max_pages).unwrap_or(1);
        if std::env::var("PLACES_DUMP_CHARTS").as_deref() == Ok("1") {
            let mut dump = String::new();
            for patch in &patches {
                let (w, h) = config.chart_texels(patch);
                dump.push_str(&format!("{w} {h}\n"));
            }
            let path = format!("target/agent-work/chart-sizes-{}.txt", profile.name());
            std::fs::write(&path, dump).expect("chart size dump");
            println!("    wrote {path} (chart texels, emission order)");
        }
        println!(
            "{:?}: {} charts, {} data texels, {} outer texels; two-page budget {} texels \
             ({} pages of {}); data {:.1}% / outer {:.1}% of the budget; probe needs {} pages \
             (failed {}), target {:.1} texels/m, padding {}",
            profile,
            patches.len(),
            data,
            outer,
            budget,
            config.max_pages,
            config.page_edge,
            100.0 * data as f64 / budget as f64,
            100.0 * outer as f64 / budget as f64,
            probe.page_count(),
            probe.failed(),
            config.texels_per_metre,
            config.padding,
        );
    }
    // The real two-page build, per profile, for the bake time and page shape.
    for profile in crate::quality::QualityProfile::ALL {
        let materials = crate::render::logical_materials(&level);
        let catalog = crate::loader::PropCatalog::builtin();
        let mut assets = crate::props::PropAssets::default();
        let build = crate::render::build_level_geometry_timed_with_lightmaps(
            &level,
            &catalog,
            &mut assets,
            &materials,
            crate::render::LightmapBuildOptions::for_profile(profile, LightmapMode::On),
            None,
        );
        match build.lightmaps.as_deref() {
            Some(lightmaps) => println!(
                "{:?}: real build: {} page(s), {} charts, {} chart texels, {:.1} ms",
                profile,
                lightmaps.pages.len(),
                lightmaps.charts.len(),
                lightmaps.stats.texels,
                lightmaps.stats.bake_millis,
            ),
            None => println!(
                "{:?}: real build: FAILED ({:?})",
                profile,
                build.lightmap_failure.unwrap_or(LightmapFailure::Layout),
            ),
        }
    }
}

fn cache_fixture(key: &str, value: f32) -> LevelLightmaps {
    let texel = LightmapTexel {
        irradiance: [value; 3],
        direction: [0.0; 3],

        axis: [0.5, 0.5],
    };
    LevelLightmaps {
        pages: vec![LightmapPage {
            width: 4,
            height: 4,
            texels: vec![texel; 16],
        }],
        charts: vec![(
            patch(1.0, 1.0),
            Chart {
                page: 0,
                x: 0,
                y: 0,
                width: 4,
                height: 4,
            },
        )],
        stats: LightmapStats::default(),
        cache_key: key.to_string(),
        padding: 2,
        switchable: Vec::new(),
    }
}

#[test]
fn memory_cache_evicts_the_least_recently_used_entry_without_invalidating_owners() {
    let mut cache = LightmapCache::memory_only();
    let first = std::sync::Arc::new(cache_fixture("first", 1.0));
    cache.insert("first", std::sync::Arc::clone(&first));
    for key in ["second", "third", "fourth"] {
        cache.insert(key, std::sync::Arc::new(cache_fixture(key, 2.0)));
    }
    assert!(cache.get("first").is_some());
    cache.insert("fifth", std::sync::Arc::new(cache_fixture("fifth", 5.0)));
    assert_eq!(cache.len(), 4);
    assert!(cache.get("second").is_none());
    assert!(std::sync::Arc::ptr_eq(
        &first,
        &cache.get("first").expect("recent entry")
    ));
    for key in ["sixth", "seventh", "eighth", "ninth"] {
        cache.insert(key, std::sync::Arc::new(cache_fixture(key, 3.0)));
    }
    assert!(cache.get("first").is_none());
    assert_eq!(
        first.pages[0].texels,
        vec![
            LightmapTexel {
                irradiance: [1.0; 3],
                direction: [0.0; 3],

                axis: [0.5, 0.5],
            };
            16
        ],
        "the active owner survives eviction"
    );
}
