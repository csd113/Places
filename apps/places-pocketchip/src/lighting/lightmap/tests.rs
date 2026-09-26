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

use super::{Chart, LightmapConfig, LightmapMode, LightmapPatch, PatchKind, SkylineAllocator};

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
fn skyline_packing_is_deterministic_and_disjoint() {
    let config = LightmapConfig::for_profile(crate::quality::QualityProfile::Full);
    let patches: Vec<LightmapPatch> = (0..40)
        .map(|index| patch(1.0 + (index % 7) as f32, 0.5 + (index % 5) as f32))
        .collect();
    let pack = |patches: &[LightmapPatch]| {
        let mut allocator = SkylineAllocator::new(config);
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
fn the_packer_places_at_the_lowest_free_position() {
    // The bottom-left policy in one picture: a 32 x 32 page, an 8 x 16 tall
    // chart first, then 8 x 8 short ones that fill the free row beside it and
    // only then stack above it. A per-page shelf list would have had to start
    // every following row below the tall chart's full height.
    let config = LightmapConfig {
        texels_per_metre: 1.0,
        page_edge: 32,
        max_pages: 1,
        padding: 0,
        bytes_per_texel: 3,
    };
    let mut allocator = SkylineAllocator::new(config);
    let tall = allocator.allocate(&patch(8.0, 16.0)).expect("tall chart");
    assert_eq!((tall.x, tall.y, tall.width, tall.height), (0, 0, 8, 16));
    let expected = [
        (8u32, 0u32),
        (16, 0),
        (24, 0),
        (8, 8),
        (16, 8),
        (24, 8),
        (0, 16),
        (8, 16),
        (16, 16),
        (24, 16),
        (0, 24),
        (8, 24),
        (16, 24),
        (24, 24),
    ];
    for (x, y) in expected {
        let chart = allocator.allocate(&patch(8.0, 8.0)).expect("short chart");
        assert_eq!((chart.x, chart.y), (x, y), "bottom-left placement");
    }
    // The 32 x 32 page is now full, and the one-page budget is spent.
    assert!(allocator.allocate(&patch(8.0, 8.0)).is_none());
    assert!(allocator.failed());
}

#[test]
fn overflow_is_reported_not_hidden() {
    let config = LightmapConfig {
        max_pages: 1,
        ..LightmapConfig::for_profile(crate::quality::QualityProfile::Full)
    };
    let mut allocator = SkylineAllocator::new(config);
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
fn two_pages_are_used_when_genuinely_needed() {
    let config = LightmapConfig::for_profile(crate::quality::QualityProfile::Full);
    let mut allocator = SkylineAllocator::new(config);
    let span = config.max_chart_span_m();
    let big = patch(span, span);
    assert!(allocator.allocate(&big).is_some());
    assert!(
        allocator.allocate(&big).is_some(),
        "second big chart needs page 2"
    );
    assert_eq!(allocator.page_count(), 2);
    assert!(!allocator.failed());
    // A third big chart exceeds the two-page budget.
    assert!(allocator.allocate(&big).is_none());
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

use super::{LightmapAtlas, LightmapFailure, LightmapPlan, content_key};

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
    let mut allocator = SkylineAllocator::new(config);
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
    let mut allocator = SkylineAllocator::new(config);
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
    let mut allocator = SkylineAllocator::new(config);
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

#[test]
fn an_oversized_patch_is_clamped_to_a_page_not_dropped() {
    let config = LightmapConfig::for_profile(crate::quality::QualityProfile::Full);
    let mut allocator = SkylineAllocator::new(config);
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

/// The shipped level must keep real lightmaps under both profiles.
///
/// This is the density/packing regression guard: `Low` used to overflow its
/// two 512-texel pages and fall back to vertex lighting for the whole level,
/// and a density bump that overflows is worse than no bump at all. It also
/// pins that the bake uses a meaningful part of the budget rather than
/// "fitting" by accident at a trivial density.
#[test]
fn the_shipped_demo_fits_the_two_page_budget_at_both_profiles() {
    let level =
        crate::level::LevelDef::from_json(include_str!("../../../assets/levels/places_demo.json"))
            .expect("the shipped places_demo parses");
    for profile in crate::quality::QualityProfile::ALL {
        let config = profile.lightmap_config();
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
    }
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
        let mut probe = SkylineAllocator::new(LightmapConfig {
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
        if std::env::var("LIMINAL_DUMP_CHARTS").as_deref() == Ok("1") {
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
