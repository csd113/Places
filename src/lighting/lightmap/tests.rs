//! Unit tests for the lightmap plan, packer and atlas.

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
        bytes_per_texel: 3,
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
fn four_pages_are_used_when_genuinely_needed() {
    let config = LightmapConfig::for_profile(crate::quality::QualityProfile::Full);
    assert_eq!(config.max_pages, 4, "the shipped budget is four pages");
    let mut allocator = ChartAllocator::new(config);
    let span = config.max_chart_span_m();
    // A chart at the span cap fills a page at this density, so each of the
    // first four charts opens its own page.
    let big = patch(span, span);
    for page in 0..4usize {
        assert!(
            allocator.allocate(&big).is_some(),
            "chart {page} needs page {page}"
        );
        assert_eq!(allocator.page_count(), page + 1);
    }
    assert!(!allocator.failed());
    // A fifth big chart exceeds the four-page budget.
    assert!(allocator.allocate(&big).is_none());
    assert!(allocator.failed());
    assert_eq!(allocator.page_count(), 4);
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

use super::{LIGHTMAP_FORMAT_VERSION, LightmapAtlas, LightmapFailure, LightmapPlan, content_key};

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
fn atlas_dilates_each_charts_border_into_its_own_gutter() {
    let config = LightmapConfig::for_profile(crate::quality::QualityProfile::Full);
    let mut allocator = ChartAllocator::new(config);
    let chart_a = allocator.allocate(&patch(2.0, 2.0)).expect("chart a");
    let chart_b = allocator.allocate(&patch(2.0, 2.0)).expect("chart b");
    let charts = [(patch(2.0, 2.0), chart_a), (patch(2.0, 2.0), chart_b)];
    let atlas = LightmapAtlas::bake(&config, allocator.page_count(), &charts, |_, chart| {
        if chart.x == chart_a.x && chart.y == chart_a.y {
            vec![[0.25, 0.25, 0.25]; (chart.width * chart.height) as usize]
        } else {
            vec![[0.75, 0.75, 0.75]; (chart.width * chart.height) as usize]
        }
    })
    .expect("atlas bakes");
    let page = &atlas.pages()[0];
    let padding = config.padding;
    for chart in [chart_a, chart_b] {
        let expected = if chart == chart_a { 64u8 } else { 191u8 };
        // A texel diagonally outside the chart's top-left corner must carry the
        // chart's own edge colour, not the neighbour's and not zero.
        let gutter_x = chart.x - padding;
        let gutter_y = chart.y - padding;
        let offset = ((gutter_y * page.width + gutter_x) * 3) as usize;
        let gutter = &page.rgb[offset..offset + 3];
        assert!(
            gutter.iter().all(|byte| *byte == expected),
            "chart gutter must dilate its own edge, got {gutter:?}"
        );
        // Never a sample across a neighbour: the chart's data rectangle is
        // untouched by the other chart's fill.
        let data_offset = ((chart.y * page.width + chart.x) * 3) as usize;
        let data = &page.rgb[data_offset..data_offset + 3];
        assert!(data.iter().all(|byte| *byte == expected));
    }
}

#[test]
fn atlas_rejects_a_fill_of_the_wrong_size_or_with_non_finite_values() {
    let config = LightmapConfig::for_profile(crate::quality::QualityProfile::Full);
    let mut allocator = ChartAllocator::new(config);
    let chart = allocator.allocate(&patch(1.0, 1.0)).expect("chart");
    let charts = [(patch(1.0, 1.0), chart)];
    assert_eq!(
        LightmapAtlas::bake(&config, allocator.page_count(), &charts, |_, _| Vec::new()),
        Err(LightmapFailure::FillSize)
    );
    assert_eq!(
        LightmapAtlas::bake(&config, allocator.page_count(), &charts, |_, chart| {
            vec![[f32::NAN, 0.0, 0.0]; (chart.width * chart.height) as usize]
        }),
        Err(LightmapFailure::FillNonFinite)
    );
}

#[test]
fn a_page_encodes_as_a_decodable_png() {
    let config = LightmapConfig::for_profile(crate::quality::QualityProfile::Low);
    let mut allocator = ChartAllocator::new(config);
    let chart = allocator.allocate(&patch(1.0, 1.0)).expect("chart");
    let charts = [(patch(1.0, 1.0), chart)];
    let atlas = LightmapAtlas::bake(&config, allocator.page_count(), &charts, |_, chart| {
        vec![[0.0, 1.0, 0.5]; (chart.width * chart.height) as usize]
    })
    .expect("atlas bakes");
    let page = &atlas.pages()[0];
    let bytes = super::page_png_bytes(page).expect("page encodes");
    let decoded = crate::materials::decode_png(&bytes).expect("page decodes");
    assert_eq!(decoded.width, page.width);
    assert_eq!(decoded.height, page.height);
    // The top-left texel of the page is chart data or its dilated gutter, both
    // the same colour here; alpha is always 255.
    assert_eq!(decoded.rgba.get(3), Some(&255));
}

#[test]
fn content_key_is_stable_and_changes_with_the_inputs() {
    let level = crate::level::LevelDef::from_json(
        r#"{
            "format_version": 1,
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
}

use super::{LevelLightmaps, LightmapCache, LightmapPage, LightmapStats};

#[test]
fn memory_cache_returns_the_same_allocation_and_clears() {
    let mut cache = LightmapCache::memory_only();
    let lightmaps = std::sync::Arc::new(LevelLightmaps {
        pages: Vec::new(),
        charts: Vec::new(),
        stats: LightmapStats::default(),
        cache_key: "key".to_string(),
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

#[test]
fn disk_cache_round_trips_a_page_set() {
    let root = std::path::PathBuf::from("target/agent-work/lightmap-cache-test");
    let _ = std::fs::remove_dir_all(&root);
    let chart = Chart {
        page: 0,
        x: 0,
        y: 0,
        width: 4,
        height: 4,
    };
    let lightmaps = LevelLightmaps {
        pages: vec![LightmapPage {
            width: 4,
            height: 4,
            rgb: vec![7; 4 * 4 * 3],
        }],
        charts: vec![(patch(1.0, 1.0), chart)],
        stats: LightmapStats::default(),
        cache_key: "v1-test-key".to_string(),
    };
    super::cache::disk_store(&root, &lightmaps.cache_key, &lightmaps);
    let loaded = super::cache::disk_load(&root, &lightmaps.cache_key).expect("disk round trip");
    assert_eq!(loaded.pages, lightmaps.pages);
    assert_eq!(loaded.charts, lightmaps.charts);
    assert_eq!(loaded.stats.charts, 1);
    assert_eq!(loaded.stats.texels, 16);
    assert!(loaded.stats.cache_hit);
    let _ = std::fs::remove_dir_all(&root);
}

/// The format version is bumped whenever the atlas layout, texel encoding or
/// key inputs change; version 6 is the four-page array format that must not
/// read a version-5 two-page atlas. Pinned so a future layout change has to
/// bump it deliberately.
#[test]
fn the_format_version_is_current_and_is_part_of_every_key_prefix() {
    // The value itself is pinned by the cache module's version notes; the
    // contract under test is that the key carries it.
    assert_eq!(LIGHTMAP_FORMAT_VERSION, 8);
    let level = crate::level::LevelDef::from_json(
        r#"{
            "format_version": 1,
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
    // An older-version directory is a different path: the new key can never
    // resolve to it even for byte-identical level content and configuration.
    assert!(!key.starts_with(&format!("v{}-", LIGHTMAP_FORMAT_VERSION - 1)));
}

/// A cached atlas whose recorded version is not the current one is rejected:
/// the directory name scheme already hides old entries, and this is the second
/// gate that keeps a stray copy (a rename, a restored backup) from being read
/// as the new format.
#[test]
fn disk_load_rejects_a_mismatched_format_version() {
    let root = std::path::PathBuf::from("target/agent-work/lightmap-version-test");
    let _ = std::fs::remove_dir_all(&root);
    let chart = Chart {
        page: 0,
        x: 0,
        y: 0,
        width: 4,
        height: 4,
    };
    let key = format!("v{LIGHTMAP_FORMAT_VERSION}-version-test");
    let lightmaps = LevelLightmaps {
        pages: vec![LightmapPage {
            width: 4,
            height: 4,
            rgb: vec![9; 4 * 4 * 3],
        }],
        charts: vec![(patch(1.0, 1.0), chart)],
        stats: LightmapStats::default(),
        cache_key: key.clone(),
    };
    super::cache::disk_store(&root, &key, &lightmaps);
    assert!(
        super::cache::disk_load(&root, &key).is_some(),
        "the current-version entry loads"
    );
    let meta_path = root.join(&key).join("meta.json");
    let meta = std::fs::read_to_string(&meta_path).expect("meta is written");
    let old = meta.replacen(
        &format!("\"version\":{LIGHTMAP_FORMAT_VERSION}"),
        &format!("\"version\":{}", LIGHTMAP_FORMAT_VERSION - 1),
        1,
    );
    assert_ne!(meta, old, "the meta must record the current version");
    std::fs::write(&meta_path, &old).expect("the old meta is written");
    assert!(
        super::cache::disk_load(&root, &key).is_none(),
        "a version-5 meta must be rejected as a miss"
    );
    // Restoring the current version makes the same bytes load again, so the
    // rejection is the version gate and not a malformed fixture.
    std::fs::write(&meta_path, &meta).expect("the current meta is restored");
    assert!(super::cache::disk_load(&root, &key).is_some());
    let _ = std::fs::remove_dir_all(&root);
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
/// need more than the historical two-page budget but fit the shipped four. It
/// pins the capacity contract on a clean checkout, where the drop-in Pit is not
/// present.
fn large_tower_level() -> crate::level::LevelDef {
    let mut rooms: Vec<String> = Vec::new();
    let mut lights: Vec<String> = Vec::new();
    for storey in 0..2 {
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
            "format_version": 1,
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
            profile,
            bake: profile.bake_config(),
        },
        None,
    )
}

/// The capacity regression: The Pit's 2,808 m² of floor plus the same ceiling
/// (25 rooms, 114 fixtures) exceeded the old two-page budget at Full and the
/// whole level fell back to vertex lighting, which removed the per-texel light
/// from every room. The shipped four-page budget must hold a level of that
/// size, and a two-page budget must still fail over by name rather than drop
/// pages silently.
///
/// The synthetic tower pins both halves of that contract on every checkout. The
/// drop-in `levels/level0_pit.json` is a per-user file that is not committed, so
/// when it is present its real build is checked too; when it is absent the test
/// still covers the capacity boundary.
#[test]
fn the_pit_bakes_into_the_four_page_budget_at_full() {
    let profile = QualityProfile::Full;
    let config = profile.lightmap_config();
    assert_eq!(
        config.max_pages, 4,
        "the shipped profile supports four pages"
    );
    assert_eq!(config.page_edge, 1024);

    let tower = large_tower_level();

    // The historical two-page budget must overflow: this is the regression the
    // capacity raise exists for.
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

    // The shipped budget must hold it.
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

    // Every architectural vertex samples a real layer; only invisible sub-texel
    // slivers may stay vertex-lit, and the highest page byte is the last page.
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
    assert!(architectural > 0, "the tower emits architectural geometry");
    assert!(
        lightmapped * 100 >= architectural * 99,
        "the atlas must cover the architecture: {lightmapped} of {architectural} vertices"
    );
    assert_eq!(
        highest_page + 1,
        lightmaps.pages.len(),
        "every resident layer must be referenced by a stamped chart"
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

        assert_wall_texels_match_the_vertex_path(&level, real_maps, profile);
    }
}

/// Every wall texel must take the light the vertex path gives the same point of
/// the same face. The historical loose containment resolved a boundary face to
/// whichever overlapping room the tie-break preferred (191 of The Pit's 634
/// wall charts baked at a neighbour's ambient while the vertex path lit them
/// from their own room), which left the shaft walls near-black. Also requires
/// that a non-trivial share of the level's wall texels is actually lit.
fn assert_wall_texels_match_the_vertex_path(
    level: &crate::level::LevelDef,
    lightmaps: &LevelLightmaps,
    profile: QualityProfile,
) {
    let lighting = crate::lighting::LevelLighting::bake_with(level, profile.bake_config());
    let mut wall_texels = 0usize;
    let mut lifted = 0usize;
    for (patch, chart) in &lightmaps.charts {
        if !matches!(patch.kind, PatchKind::Wall) {
            continue;
        }
        let bias = super::fill::face_normal_bias(patch);
        let filled = super::fill_chart(&lighting, patch, chart);
        let width = chart.width.max(1);
        let height = chart.height.max(1);
        for (index, value) in filled.iter().enumerate() {
            let i = index % width as usize;
            let j = index / width as usize;
            let u = if width <= 1 {
                0.5
            } else {
                i as f32 / (width - 1) as f32
            };
            let v = if height <= 1 {
                0.5
            } else {
                j as f32 / (height - 1) as f32
            };
            let point = patch.point_at(u, v);
            let point = [point[0] + bias[0], point[1] + bias[1], point[2] + bias[2]];
            let vertex = lighting.sample_face(patch.room, point[0], point[1], point[2]);
            let expected = [
                vertex.r.clamp(crate::lighting::AMBIENT_LEVEL, 1.0),
                vertex.g.clamp(crate::lighting::AMBIENT_LEVEL, 1.0),
                vertex.b.clamp(crate::lighting::AMBIENT_LEVEL, 1.0),
            ];
            for channel in 0..3 {
                assert!(
                    (value[channel] - expected[channel]).abs() < 1.0e-4,
                    "wall texel {index} channel {channel}: atlas {} vs vertex path {}",
                    value[channel],
                    expected[channel]
                );
            }
            wall_texels += 1;
            if expected[0] > crate::lighting::AMBIENT_LEVEL + 0.15 {
                lifted += 1;
            }
        }
    }
    assert!(wall_texels > 0, "the level must have wall charts");
    assert!(
        lifted * 100 >= wall_texels * 5,
        "the lit walls must be lit, not ambient: {lifted} of {wall_texels} texels"
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
/// did. With the four-page budget the added room must not change the lit
/// room's bake at all.
#[test]
fn an_unrelated_distant_room_does_not_darken_a_lit_room() {
    const LIT_ROOM: &str = r#"{
        "format_version": 1,
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
        "format_version": 1,
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
    assert!(enlarged_lightmaps.pages.len() <= 4);
    assert!(
        enlarged_lightmaps.pages.len() > QualityProfile::Full.lightmap_config().max_pages / 2,
        "the distant room must actually push past the old two-page half of the budget"
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
                "format_version": 1,
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
