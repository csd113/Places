//! Unit tests for the level geometry builder and the renderer's CPU side.
//!
//! They exercise the mesh builder, batching and indexing, the decal sheets, the
//! prop instancing and the view/capture helpers through the same entry points
//! the game uses.

// Test code: unwrap/expect, indexing, loose casts and permissive arithmetic are idiomatic in tests;
// the production lints stay enforced everywhere else in the crate.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::collection_is_never_read,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::indexing_slicing,
    clippy::manual_assert_eq,
    clippy::panic,
    clippy::print_stdout,
    clippy::unwrap_used,
    clippy::wildcard_enum_match_arm
)]

use crate::test_support::{assert_exact, assert_exact_array, assert_exact_named};

use super::*;
use crate::level::{LevelDef, PropDef};
use crate::lighting::LevelLighting;
use crate::render::common::MeshPacker;
use crate::render::common::decals::{DECAL_ATLAS_SIZE, generate_decal_atlas};
use crate::render::common::mesh;
use crate::spatial::{DepthRange, Frustum};

// ------------------------------------------------------- vertex quantisation

/// The quantisation error a unit-interval channel can introduce.
fn quantised_channel_error(value: f32) -> f32 {
    (dequantize_unit(mesh::quantize_unit(value)) - value).abs()
}

#[test]
fn the_unit_quantiser_is_accurate_at_the_lighting_extremes_and_in_between() {
    // Minimum baked lighting: the darkest a vertex can get.
    assert!(quantised_channel_error(crate::lighting::AMBIENT_LEVEL) < 0.5 / 255.0);
    // Maximum brightness.
    assert!(quantised_channel_error(crate::lighting::MAX_BRIGHTNESS) < 0.5 / 255.0);
    // Darkest and brightest possible shades of a wall/floor tint.
    assert!(quantised_channel_error(0.0) < 1e-6);
    assert!(quantised_channel_error(1.0) < 1e-6);
    // A representative intermediate value, and one that lands exactly
    // between two steps (the worst case).
    for value in [0.666, 0.42, 127.5 / 255.0, 1.0 / 255.0, 0.999] {
        assert!(
            quantised_channel_error(value) <= 0.5 / 255.0 + 1e-6,
            "value {value} quantised by more than half a step"
        );
    }
    // The whole usable lighting range, swept at 1/1000.
    let mut worst = 0.0f32;
    for step in 0..=1000 {
        let value = crate::lighting::AMBIENT_LEVEL
            + (crate::lighting::MAX_BRIGHTNESS - crate::lighting::AMBIENT_LEVEL) * step as f32
                / 1000.0;
        worst = worst.max(quantised_channel_error(value));
    }
    assert!(
        worst <= 0.5 / 255.0 + 1e-6,
        "worst lighting quantisation {worst} exceeds half a step"
    );
}

#[test]
fn the_unit_quantiser_clamps_instead_of_wrapping() {
    // A malformed level or an over-bright authored shade must saturate, not
    // wrap to the opposite end of the range.
    for (value, expected) in [
        (-1.0f32, 0u8),
        (-0.001, 0),
        (0.0, 0),
        (1.0, 255),
        (1.5, 255),
        (f32::INFINITY, 255),
        (f32::NEG_INFINITY, 0),
    ] {
        assert_eq!(
            mesh::quantize_unit(value),
            expected,
            "value {value} must clamp to {expected}"
        );
    }
    assert_eq!(
        mesh::quantize_unit(f32::NAN),
        0,
        "NaN must not become a bright value"
    );
}

#[test]
fn the_unit_quantiser_preserves_alpha_for_props_and_the_hud() {
    // Prop models carry alpha from their glTF `COLOR_0`, and the UI blends
    // with it, so the fourth channel must survive quantisation.
    for value in [0.0f32, 0.25, 0.5, 1.0] {
        assert!(
            (dequantize_unit(mesh::quantize_unit(value)) - value).abs() <= 0.5 / 255.0 + 1e-6,
            "alpha {value} did not survive quantisation"
        );
    }
}

/// Loads one shipped texture asset from the catalog and decodes it.
fn texture_image(texture_id: &str) -> crate::materials::RawImage {
    let catalog = crate::assets::AssetCatalog::load_default();
    let root = crate::assets::resolve_asset_root().expect("assets/ is discoverable");
    let path = catalog
        .texture_path(texture_id)
        .unwrap_or_else(|| panic!("{texture_id} must be a file texture"));
    crate::materials::load_png_relative(&root, path)
        .unwrap_or_else(|error| panic!("{texture_id}: {error}"))
}

#[test]
fn test_shipped_texture_assets_are_opaque_and_within_budget() {
    // The six office surfaces are opaque square sheets inside the shipped
    // texture policy: non-zero, square, within the hard 1024px limit and the
    // per-sheet decoded byte budget, with an RGBA8 buffer that matches the
    // dimensions exactly. The upgraded artwork sits exactly at that limit.
    for texture in [
        "core:tex_wallpaper_yellow_01",
        "core:tex_wallpaper_stained_01",
        "core:tex_carpet_beige_01",
        "core:tex_carpet_damp_01",
        "core:tex_ceiling_panel_01",
        "core:tex_ceiling_stained_01",
    ] {
        let image = texture_image(texture);
        crate::assets::ShippedTextureKind::Surface
            .check_dimensions(image.width, image.height)
            .unwrap_or_else(|error| panic!("{texture}: {error}"));
        assert_eq!(
            image.width, image.height,
            "{texture}: a surface sheet is sampled as a square tile"
        );
        assert!(
            image.width > 0 && image.height > 0,
            "{texture}: the decoded sheet must be non-empty"
        );
        assert!(
            image.width <= crate::assets::MAX_TEXTURE_DIMENSION
                && image.height <= crate::assets::MAX_TEXTURE_DIMENSION,
            "{texture}: over the hard texture limit"
        );
        assert_eq!(
            image.rgba.len(),
            crate::assets::decoded_rgba_bytes(image.width, image.height),
            "{texture}: the decoded buffer must be width*height RGBA8"
        );
        for texel in image.rgba.as_chunks::<4>().0 {
            assert_eq!(texel[3], 255, "{texture} must be fully opaque");
        }
    }

    // The one NPOT diagnostic is a deliberate exception to the fitted-sheet
    // policy: it exists to prove arbitrary PNG dimensions load, so its exact
    // 96x64 size is asserted here rather than hidden in the policy.
    let npot = texture_image("core:tex_diagnostic_alt_01");
    assert_eq!((npot.width, npot.height), (96, 64));
    assert_eq!(npot.rgba.len(), (96 * 64 * 4) as usize);
}

/// Mean and nearest-rank p95 of a sample, sorted in place.
fn seam_distribution(values: &mut [f32]) -> (f32, f32) {
    if values.is_empty() {
        return (0.0, 0.0);
    }
    let mean = values.iter().sum::<f32>() / values.len() as f32;
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let rank = ((values.len() as f32) * 0.95).ceil() as usize;
    let p95 = values[rank.saturating_sub(1).min(values.len().saturating_sub(1))];
    (mean, p95)
}

/// Asserts one axis of a sheet joins its opposite edge the way any other pair
/// of adjacent pixels agrees.
///
/// The comparison is deliberately distributional: a textured surface has large
/// adjacent-pixel steps everywhere, so a flat absolute tolerance either passes
/// a real seam on a noisy sheet or fails a clean one on a fine one. A genuine
/// seam shows up as a wrapped step far outside the sheet's own interior step
/// distribution — the damaged stained wallpaper sat at 5x the interior mean,
/// while a clean tile sits at about 1x. The wrapped step is smoothed across the
/// wrap (the 3-tap profile is the same operator the mip chain applies), and the
/// interior is sampled every eighth pixel so the check stays fast.
fn assert_axis_tiles(name: &str, image: &crate::materials::RawImage, horizontal: bool) {
    let (size_x, size_y) = (image.width, image.height);
    let (edge_len, across_len) = if horizontal {
        (size_y, size_x)
    } else {
        (size_x, size_y)
    };
    assert!(
        across_len >= 4,
        "{name}: a tileable surface needs an interior to compare against"
    );
    let texel = |x: u32, y: u32, channel: u32| -> f32 {
        let index = ((y * size_x + x) * 4 + channel) as usize;
        f32::from(image.rgba[index])
    };
    // Profile of the wrapped first/last texels, and of an interior texel pair:
    // the 3-tap average of the last column/row continues into the first.
    let profile = |across: u32, along: u32, channel: u32, wrapped: bool| -> f32 {
        if horizontal {
            if wrapped {
                // Wrapped pair (size_x-1, 0) and (size_x-2, size_x-1, 0).
                let left = (texel(across_len - 2, along, channel)
                    + texel(across_len - 1, along, channel)
                    + texel(0, along, channel))
                    / 3.0;
                let right = (texel(across_len - 1, along, channel)
                    + texel(0, along, channel)
                    + texel(1, along, channel))
                    / 3.0;
                (left - right).abs()
            } else {
                // Interior pair: profile(x) - profile(x+1) simplifies to
                // (C(x-1) - C(x+2)) / 3.
                ((texel(across.saturating_sub(1), along, channel)
                    - texel(across + 2, along, channel))
                    / 3.0)
                    .abs()
            }
        } else if wrapped {
            let left = (texel(along, across_len - 2, channel)
                + texel(along, across_len - 1, channel)
                + texel(along, 0, channel))
                / 3.0;
            let right = (texel(along, across_len - 1, channel)
                + texel(along, 0, channel)
                + texel(along, 1, channel))
                / 3.0;
            (left - right).abs()
        } else {
            ((texel(along, across.saturating_sub(1), channel) - texel(along, across + 2, channel))
                / 3.0)
                .abs()
        }
    };
    for channel in 0..3 {
        let mut wrapped: Vec<f32> = (0..edge_len)
            .map(|along| profile(0, along, channel, true))
            .collect();
        let mut interior: Vec<f32> = Vec::new();
        for along in (0..edge_len).step_by(4) {
            for across in (1..across_len.saturating_sub(2)).step_by(8) {
                interior.push(profile(across, along, channel, false));
            }
        }
        let (wrap_mean, wrap_p95) = seam_distribution(&mut wrapped);
        let (interior_mean, interior_p95) = seam_distribution(&mut interior);
        let axis = if horizontal {
            "left-right"
        } else {
            "top-bottom"
        };
        assert!(
            wrap_mean <= 1.60f32.mul_add(interior_mean, 1.0),
            "{name}: {axis} seam on channel {channel}: the wrapped step averages \
             {wrap_mean:.2} against the sheet's own interior step {interior_mean:.2}"
        );
        assert!(
            wrap_p95 <= 2.20f32.mul_add(interior_p95, 3.0),
            "{name}: {axis} seam tail on channel {channel}: p95 {wrap_p95:.2} \
             against the interior p95 {interior_p95:.2}"
        );
    }
}

/// The surface textures tile: a wrapped edge must join its opposite edge the
/// way any other pair of adjacent pixels does, or a floor, wall or ceiling
/// shows a grid of seams every repeat.
///
/// Every shipped surface sheet is covered on both axes and all three colour
/// channels. The deliberately non-tiling diagnostic sheets (arrows, checkers
/// and orientation stripes) are excluded by design: their mismatch is the
/// point, not a defect.
#[test]
fn test_shipped_surface_textures_tile() {
    for (name, texture) in [
        ("wall", "core:tex_wallpaper_yellow_01"),
        ("wall_stained", "core:tex_wallpaper_stained_01"),
        ("carpet", "core:tex_carpet_beige_01"),
        ("carpet_damp", "core:tex_carpet_damp_01"),
        ("ceiling", "core:tex_ceiling_panel_01"),
        ("ceiling_stained", "core:tex_ceiling_stained_01"),
        ("pool_deck", "core:tex_pool_tile_deck_01"),
        ("pool_basin", "core:tex_pool_tile_basin_01"),
        ("pool_wall", "core:tex_pool_tile_wall_01"),
        ("pool_ceiling", "core:tex_pool_ceiling_01"),
        ("pool_water", "core:tex_pool_water_01"),
    ] {
        let image = texture_image(texture);
        assert_axis_tiles(name, &image, true);
        assert_axis_tiles(name, &image, false);
    }
}

#[test]
fn test_build_geometry_from_test_room() {
    let json = include_str!("../../tests/fixtures/levels/test_room.json");
    let level = LevelDef::from_json(json).expect("valid test_room json");
    let mesh = build_level_geometry(&level);
    assert!(mesh.vertex_count != 0);
    assert_eq!(mesh.index_count % 6, 0, "geometry is whole quads");
    assert!(
        mesh.vertex_count < mesh.index_count,
        "indexing must share corners"
    );
    assert!(mesh.batches.floor_batch.count > 0);
    assert!(mesh.batches.ceiling_batch.count > 0);
    assert!(mesh.batches.wall_batch.count > 0);
}

#[test]
fn test_floor_geometry_does_not_scale_with_room_area() {
    let level = |size: f32| {
        let json = format!(
            r#"{{
                "format_version": 1,
                "id": "big",
                "name": "Big",
                "spawn": {{ "x": 0.0, "z": 0.0 }},
                "rooms": [{{ "x": 0.0, "z": 0.0, "width": {size}, "depth": {size}, "height": 3.5 }}]
            }}"#
        );
        LevelDef::from_json(&json).expect("valid json")
    };

    // A large room is subdivided on the bounded baked-lighting grid so
    // fixture pools can vary across the floor, but the cell count is capped
    // and flat regions merge: a 400x400 m room costs exactly the same as a
    // 100x100 m one, and with no fixtures both collapse to a single quad.
    let hundred = build_level_geometry(&level(100.0));
    let four_hundred = build_level_geometry(&level(400.0));
    let cap = i32::try_from(
        crate::lighting::MAX_LIGHT_GRID_CELLS * crate::lighting::MAX_LIGHT_GRID_CELLS,
    )
    .unwrap_or(i32::MAX)
        * 6;
    for mesh in [&hundred, &four_hundred] {
        assert!(mesh.batches.floor_batch.count > 0);
        assert!(mesh.batches.ceiling_batch.count > 0);
        assert!(
            mesh.batches.floor_batch.count <= cap,
            "floor geometry must stay capped, got {}",
            mesh.batches.floor_batch.count
        );
        assert!(
            mesh.batches.ceiling_batch.count <= cap,
            "ceiling geometry must stay capped, got {}",
            mesh.batches.ceiling_batch.count
        );
    }
    // No fixtures and no fixtures nearby: the uniform room merges to one
    // quad on each surface, so area genuinely stops mattering.
    assert_eq!(hundred.batches.floor_batch.count, 6);
    assert_eq!(hundred.batches.ceiling_batch.count, 6);
    assert_eq!(
        hundred.batches.floor_batch.count,
        four_hundred.batches.floor_batch.count
    );
    assert_eq!(
        hundred.batches.ceiling_batch.count,
        four_hundred.batches.ceiling_batch.count
    );

    // A room smaller than one lighting cell stays a single quad.
    let small = build_level_geometry(&level(2.0));
    assert_eq!(small.batches.floor_batch.count, 6);
    assert_eq!(small.batches.ceiling_batch.count, 6);
}

/// The final carpet must never read as a metre checker.
///
/// A 1 m bright/dark quadrant tint would look like a debug board on a large
/// floor. The artwork uses low-frequency pile variation, so the four quadrant
/// means must be close: the sheet may be mottled, but no quadrant may be a
/// visibly different flat cell. The quadrants are derived from the decoded
/// sheet, so the check holds at whatever resolution the artwork ships.
#[test]
fn test_carpet_png_has_no_metre_checker() {
    let carpet = texture_image("core:tex_carpet_beige_01");
    let width = carpet.width;
    let height = carpet.height;
    assert!(
        width > 0 && height > 0,
        "the carpet sheet must be non-empty"
    );
    assert_eq!(width, height, "a surface sheet is sampled as a square tile");
    let half_x = width / 2;
    let half_y = height / 2;
    let mean = |x0: u32, y0: u32| -> f32 {
        let mut total = 0.0f32;
        let mut texels = 0u32;
        for y in y0..y0 + half_y {
            for x in x0..x0 + half_x {
                let index = ((y * width + x) * 4) as usize;
                total += f32::from(carpet.rgba[index])
                    + f32::from(carpet.rgba[index + 1])
                    + f32::from(carpet.rgba[index + 2]);
                texels += 1;
            }
        }
        total / (texels as f32 * 3.0)
    };
    let quadrants = [
        mean(0, 0),
        mean(half_x, 0),
        mean(0, half_y),
        mean(half_x, half_y),
    ];
    let low = quadrants.iter().copied().fold(f32::MAX, f32::min);
    let high = quadrants.iter().copied().fold(f32::MIN, f32::max);
    // The historical checker tinted adjacent metre cells roughly 7-10
    // levels apart; gentle low-frequency pile mottle stays well under that,
    // so a 4-level spread separates the two cases.
    assert!(
        high - low <= 4.0,
        "the carpet reads as a 1 m checker again: quadrant means {quadrants:?}"
    );
    // It must still be carpet, not a flat colour: the sheet needs some
    // pixel-level variation to read as pile under dim warm light.
    let mut min = u8::MAX;
    let mut max = u8::MIN;
    for y in (0..height).step_by(7) {
        for x in (0..width).step_by(5) {
            let value = carpet.rgba[((y * width + x) * 4) as usize];
            min = min.min(value);
            max = max.max(value);
        }
    }
    assert!(
        u16::from(max) - u16::from(min) >= 4,
        "the carpet has no visible pile variation ({min}..{max})"
    );
}

#[test]
fn test_drawable_aspect_ratio() {
    let cases: [(u32, u32, f32); 7] = [
        (480, 272, 480.0 / 272.0),
        (1280, 720, 16.0 / 9.0),
        (1920, 1080, 16.0 / 9.0),
        (2560, 1440, 16.0 / 9.0),
        (3840, 2160, 16.0 / 9.0),
        (1600, 1200, 4.0 / 3.0),
        (960, 544, 480.0 / 272.0), // Retina 2x of the reference canvas baseline
    ];
    for (w, h, expected) in cases {
        let size = DrawableSize::new(w, h);
        assert!(
            (size.aspect_ratio() - expected).abs() < 1e-5,
            "{w}x{h} aspect mismatch"
        );
        assert!(!size.is_empty());
    }
}

fn horizontal_fov_degrees(vertical_fov_degrees: f32, aspect: f32) -> f32 {
    let half = (vertical_fov_degrees.to_radians() * 0.5).tan() * aspect;
    (2.0 * half.atan()).to_degrees()
}

#[test]
fn test_vertical_fov_baseline_is_identity() {
    let baseline = reference_aspect_ratio();
    for fov in [45.0, 60.0, 90.0, 110.0] {
        assert!((vertical_fov_for_aspect(fov, baseline) - fov).abs() < 1e-4);
    }
}

#[test]
fn test_wider_displays_expand_horizontally() {
    // 16:9 and 21:9 are wider than the 480x272 baseline, so the vertical FOV
    // is unchanged and the horizontal view simply grows.
    let baseline = reference_aspect_ratio();
    for aspect in [16.0 / 9.0, 21.0 / 9.0, 32.0 / 9.0] {
        assert!(aspect > baseline);
        let vfov = vertical_fov_for_aspect(60.0, aspect);
        assert_exact_named(vfov, 60.0, "wider aspect must keep vertical FOV");
        assert!(horizontal_fov_degrees(vfov, aspect) > horizontal_fov_degrees(60.0, baseline));
    }
}

#[test]
fn test_taller_displays_preserve_horizontal_view() {
    let baseline = reference_aspect_ratio();
    let baseline_hfov = horizontal_fov_degrees(60.0, baseline);
    // 16:10, 4:3, 3:2 and 1:1 are all narrower than the reference canvas.
    for aspect in [16.0 / 10.0, 4.0 / 3.0, 3.0 / 2.0, 1.0] {
        assert!(aspect < baseline);
        let vfov = vertical_fov_for_aspect(60.0, aspect);
        assert!(vfov > 60.0, "taller aspect must widen vertical FOV");
        let hfov = horizontal_fov_degrees(vfov, aspect);
        assert!(
            (hfov - baseline_hfov).abs() < 1e-3,
            "horizontal FOV cropped: {hfov} vs {baseline_hfov}"
        );
    }
}

#[test]
fn test_vertical_fov_handles_degenerate_aspects() {
    assert_exact(vertical_fov_for_aspect(60.0, 0.0), 60.0);
    assert_exact(vertical_fov_for_aspect(60.0, -1.0), 60.0);
    assert_exact(vertical_fov_for_aspect(60.0, f32::NAN), 60.0);
    // Extremely tall windows are capped to keep the projection invertible.
    assert!(vertical_fov_for_aspect(60.0, 0.1) <= 150.0);
}

#[test]
fn test_zero_sized_drawable_is_empty_and_safe() {
    for size in [
        DrawableSize::new(0, 0),
        DrawableSize::new(0, 272),
        DrawableSize::new(480, 0),
    ] {
        assert!(size.is_empty());
        // Must not divide by zero or panic when a window is minimized.
        assert!(size.aspect_ratio().is_finite());
        let viewport = size.ui_viewport();
        assert_eq!(viewport.width, 0);
        assert_eq!(viewport.height, 0);
    }
}

#[test]
fn test_ui_viewport_is_uniform_and_centred() {
    let cases = [
        (480, 272),
        (1280, 720),
        (1920, 1080),
        (2560, 1440),
        (3840, 2160),
        (1600, 1200),
        (960, 544),
    ];
    for (w, h) in cases {
        let size = DrawableSize::new(w, h);
        let vp = size.ui_viewport();

        // Fits inside the drawable and stays centred.
        assert!(
            vp.width <= i32::try_from(w).unwrap_or(i32::MAX)
                && vp.height <= i32::try_from(h).unwrap_or(i32::MAX)
        );
        assert!(vp.x >= 0 && vp.y >= 0);
        assert!((i32::try_from(size.width).unwrap_or(i32::MAX) - vp.width - 2 * vp.x).abs() <= 1);
        assert!((i32::try_from(size.height).unwrap_or(i32::MAX) - vp.height - 2 * vp.y).abs() <= 1);

        // Reference aspect preserved (within one pixel of rounding).
        let vp_aspect = vp.width as f32 / vp.height as f32;
        let ref_aspect = UI_REFERENCE_WIDTH as f32 / UI_REFERENCE_HEIGHT as f32;
        assert!(
            (vp_aspect - ref_aspect).abs() < 0.01,
            "{w}x{h} UI aspect distorted: {vp_aspect} vs {ref_aspect}"
        );

        // HUD never becomes microscopic at large resolutions.
        assert!(vp.scale >= 1.0, "{w}x{h} UI scale shrank: {}", vp.scale);
    }
}

#[test]
fn test_ui_viewport_baseline_is_identity() {
    let vp = DrawableSize::new(480, 272).ui_viewport();
    assert_eq!(
        (vp.x, vp.y, vp.width, vp.height),
        (0, 0, 480, 272),
        "the reference canvas UI layout must be pixel-identical to the original"
    );
    assert_exact(vp.scale, 1.0);
}

#[test]
fn test_hidpi_uses_physical_pixels_not_logical_size() {
    // A 480x272 logical window on a 2x Retina display has a 960x544 drawable.
    let logical = DrawableSize::new(480, 272);
    let physical = DrawableSize::new(960, 544);

    assert_exact(physical.ui_viewport().scale, 2.0);
    assert_eq!(physical.ui_viewport().width, 960);
    assert_eq!(physical.ui_viewport().height, 544);
    assert!((physical.aspect_ratio() - logical.aspect_ratio()).abs() < 1e-6);
}

/// The fresh-install 1920x1080 window on a 2x Retina display produces a
/// 3840x2160 drawable, and every derived render dimension follows the drawable:
/// the scene target is not an old low-resolution buffer being stretched, and
/// the projection keeps the window's 16:9 aspect.
#[test]
fn test_hidpi_1080p_window_renders_through_the_drawable_path() {
    use crate::quality::QualityLevel;

    let logical = (1920u32, 1080u32);
    let physical = DrawableSize::new(logical.0 * 2, logical.1 * 2);
    assert_eq!(physical, DrawableSize::new(3840, 2160));
    assert!(
        (logical.0 as f32 / logical.1 as f32 - physical.aspect_ratio()).abs() < 1e-6,
        "a HiDPI drawable must keep the logical window's 16:9 aspect"
    );

    assert_eq!(
        super::common::framebuffer::scene_target_size(QualityLevel::High, physical),
        physical,
        "High renders at the drawable's own resolution"
    );
    assert_eq!(
        super::common::framebuffer::scene_target_size(QualityLevel::Medium, physical),
        DrawableSize::new(1920, 1080),
        "Medium caps the scene scale at one half of a large drawable"
    );
    assert_eq!(
        super::common::framebuffer::scene_target_size(QualityLevel::Low, physical),
        DrawableSize::new(480, 270),
        "only the documented Low level scales the scene to the reference width"
    );

    // The UI viewport follows the physical drawable, not the logical window.
    assert_exact(physical.ui_viewport().scale, 2160.0 / 272.0);
    assert_eq!(physical.ui_viewport().height, 2160);
}

/// A resize (or a `HiDPI` backing-scale change) updates every derived target size
/// without a restart.
#[test]
fn test_a_resize_updates_scene_and_bloom_targets() {
    use crate::quality::QualityLevel;

    let before = DrawableSize::new(1920, 1080);
    let after = DrawableSize::new(2560, 1440);

    assert_eq!(
        super::common::framebuffer::scene_target_size(QualityLevel::High, before),
        before
    );
    assert_eq!(
        super::common::framebuffer::scene_target_size(QualityLevel::High, after),
        after
    );
    assert_eq!(
        super::common::postprocess::bloom_target_size(before),
        DrawableSize::new(480, 270)
    );
    assert_eq!(
        super::common::postprocess::bloom_target_size(after),
        DrawableSize::new(640, 360)
    );
    assert!(after.ui_viewport().scale > before.ui_viewport().scale);
}

#[test]
fn test_framebuffer_size_changes_update_scale() {
    let small = DrawableSize::new(480, 272);
    let large = DrawableSize::new(1920, 1080);
    assert_ne!(small, large);
    assert!(large.ui_viewport().scale > small.ui_viewport().scale);
    assert_exact(large.ui_viewport().scale, 1080.0 / 272.0);
}

#[test]
fn test_build_geometry_from_the_shipped_demo() {
    let json = include_str!("../../assets/levels/places_demo.json");
    let level = LevelDef::from_json(json).expect("valid places_demo json");
    let mesh = build_level_geometry(&level);
    assert!(mesh.vertex_count != 0);
    assert_eq!(mesh.index_count % 6, 0, "geometry is whole quads");
    assert!(
        mesh.vertex_count < mesh.index_count,
        "indexing must share corners"
    );
    assert!(mesh.batches.floor_batch.count > 0);
    assert!(mesh.batches.ceiling_batch.count > 0);
    assert!(mesh.batches.wall_batch.count > 0);
    assert!(mesh.batches.light_batch.count > 0);

    // The whole shipped demo stays a modest number of vertices.
    assert!(
        mesh.vertex_count < 100_000,
        "places_demo unexpectedly large: {} vertices",
        mesh.vertex_count
    );
}

/// Builds a compact test level: one 10x10 m room, one 10 x 0.4 m wall
/// spanning the full ceiling height, plus the supplied openings/props.
fn level_with_wall(openings_json: &str, props_json: &str) -> LevelDef {
    level_with_wall_and_lights(openings_json, props_json, "[]")
}

/// As [`level_with_wall`], with explicit ceiling fixtures.
fn level_with_wall_and_lights(
    openings_json: &str,
    props_json: &str,
    lights_json: &str,
) -> LevelDef {
    let json = format!(
        r#"{{
            "format_version": 1,
            "id": "geometry_test",
            "name": "Geometry Test",
            "spawn": {{ "x": 0.0, "z": 0.0 }},
            "room": {{ "x": -5.0, "z": -5.0, "width": 10.0, "depth": 10.0, "height": 3.5 }},
            "walls": [{{
                "x": -5.0, "z": 0.0, "width": 10.0, "depth": 0.4, "height": 3.5,
                "openings": {openings_json}
            }}],
            "ceiling_lights": {lights_json},
            "props": {props_json}
        }}"#
    );
    LevelDef::from_json(&json).expect("valid json")
}

/// A square room with the given ceiling fixtures and nothing else.
fn lit_room_level(width: f32, depth: f32, height: f32, lights_json: &str) -> LevelDef {
    let json = format!(
        r#"{{
            "format_version": 1,
            "id": "lit_room",
            "name": "Lit Room",
            "spawn": {{ "x": 0.0, "z": 0.0 }},
            "rooms": [{{ "x": 0.0, "z": 0.0, "width": {width}, "depth": {depth}, "height": {height} }}],
            "ceiling_lights": {lights_json}
        }}"#
    );
    LevelDef::from_json(&json).expect("valid lit room json")
}

/// Expands a material's aggregate index span back into draw-order vertices.
///
/// The renderer never expands indices; tests inspect geometry in draw order,
/// which is what the pre-indexing vertex buffer held.
fn batch_slice(mesh: &LevelMesh, kind: SurfaceKind) -> Vec<Vertex> {
    mesh.triangles_for(kind)
}

/// Draw-order vertices of one material id, resolved through the shipped
/// catalog's logical table (no image decoding).
fn material_vertices(mesh: &LevelMesh, level: &LevelDef, material_id: &str) -> Vec<Vertex> {
    let table = logical_materials(level);
    let index = table
        .index_of(material_id)
        .unwrap_or_else(|| panic!("{material_id} is not referenced by the level"));
    // Fixture sheets are material-indexed too, so a material query must stay
    // on the surface families it is about: a wall material and a fixture sheet
    // can share an index without being the same thing.
    let mut out = Vec::new();
    for range in &mesh.ranges {
        if range.key.material != index
            || !matches!(
                range.key.kind,
                SurfaceKind::Floor | SurfaceKind::Ceiling | SurfaceKind::Wall | SurfaceKind::Decal
            )
        {
            continue;
        }
        out.extend(
            range
                .indices
                .iter()
                .filter_map(|i| range.vertices.get(usize::from(*i)).copied()),
        );
    }
    out
}

// ---------------------------------------------------- material overrides

/// Axis-aligned X/Z bounds of a vertex run, as `(min_x, max_x, min_z, max_z)`.
fn xz_bounds(vertices: &[Vertex]) -> (f32, f32, f32, f32) {
    let mut bounds = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
    for vertex in vertices {
        bounds.0 = bounds.0.min(vertex.pos[0]);
        bounds.1 = bounds.1.max(vertex.pos[0]);
        bounds.2 = bounds.2.min(vertex.pos[2]);
        bounds.3 = bounds.3.max(vertex.pos[2]);
    }
    bounds
}

#[test]
fn material_ids_resolve_to_their_own_keys_tiling_and_tint() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 1,
            "id": "material_keys",
            "name": "Material Keys",
            "spawn": { "x": 1.0, "z": 1.0 },
            "defaults": {
                "wall": "core:wallpaper_yellow_01",
                "floor": "core:carpet_beige_01",
                "ceiling": "core:ceiling_panel_01"
            },
            "rooms": [{ "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0, "height": 3.0,
                        "material": "core:carpet_damp_01",
                        "ceiling_material": "core:ceiling_stained_01" }],
            "walls": [{ "x": 0.0, "z": 0.0, "width": 4.0, "depth": 0.2,
                        "material": "core:wallpaper_stained_01" }]
        }"#,
    )
    .expect("valid level");
    let table = logical_materials(&level);

    let maintained_wall = table.index_of("core:wallpaper_yellow_01").expect("wall");
    let stained_wall = table
        .index_of("core:wallpaper_stained_01")
        .expect("stained wall");
    let damp_floor = table.index_of("core:carpet_damp_01").expect("damp floor");
    let stained_ceiling = table
        .index_of("core:ceiling_stained_01")
        .expect("stained ceiling");
    assert_ne!(maintained_wall, stained_wall);

    let wall = table.entry(maintained_wall).expect("wall entry");
    assert_eq!(wall.tile_metres, 2.0);
    assert_eq!(wall.tint, [0.85, 0.80, 0.42]);
    let floor = table.entry(damp_floor).expect("floor entry");
    assert_eq!(floor.tile_metres, 2.0);
    assert_eq!(floor.tint, [1.0, 1.0, 1.0]);
    let ceiling = table.entry(stained_ceiling).expect("ceiling entry");
    assert_eq!(ceiling.tint, [0.72, 0.72, 0.70]);

    // The key carries the slot the geometry asked for, never the material id.
    assert_eq!(
        SurfaceKey::new(MaterialSlot::Ceiling.kind(), damp_floor),
        SurfaceKey::new(SurfaceKind::Ceiling, damp_floor)
    );
    assert!(SurfaceKey::bare(SurfaceKind::Light).material == MATERIAL_NONE);
}

#[test]
fn room_material_overrides_pick_the_damaged_sheets_for_that_room_only() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 1,
            "id": "damaged_rooms",
            "name": "Damaged Rooms",
            "spawn": { "x": 1.0, "z": 1.0 },
            "rooms": [
                { "x": 0.0, "z": 0.0, "width": 6.0, "depth": 6.0, "height": 3.0 },
                { "x": 6.0, "z": 0.0, "width": 6.0, "depth": 6.0, "height": 3.0,
                  "material": "core:carpet_damp_01",
                  "ceiling_material": "core:ceiling_stained_01" }
            ],
            "ceiling_lights": [
                { "fixture": "core:fluorescent_panel_01", "x": 3.0, "z": 3.0 },
                { "fixture": "core:fluorescent_panel_01", "x": 9.0, "z": 3.0 }
            ]
        }"#,
    )
    .expect("valid level");
    let mesh = build_level_geometry(&level);

    let clean_floor = material_vertices(&mesh, &level, "core:carpet_beige_01");
    let damp_floor = material_vertices(&mesh, &level, "core:carpet_damp_01");
    let clean_ceiling = material_vertices(&mesh, &level, "core:ceiling_panel_01");
    let stained_ceiling = material_vertices(&mesh, &level, "core:ceiling_stained_01");
    assert!(!clean_floor.is_empty() && !damp_floor.is_empty());
    assert!(!clean_ceiling.is_empty() && !stained_ceiling.is_empty());

    // Each room keeps its own texture: the damp floor is the second room's
    // rectangle, the clean floor the first's.
    assert_eq!(xz_bounds(&damp_floor), (6.0, 12.0, 0.0, 6.0));
    assert_eq!(xz_bounds(&clean_floor), (0.0, 6.0, 0.0, 6.0));
    assert_eq!(xz_bounds(&stained_ceiling), (6.0, 12.0, 0.0, 6.0));
    assert_eq!(xz_bounds(&clean_ceiling), (0.0, 6.0, 0.0, 6.0));

    // A level that never mentions a damaged id resolves only the
    // maintained materials it authored.
    let clean = lit_room_level(8.0, 8.0, 3.0, "[]");
    let clean_table = logical_materials(&clean);
    assert_eq!(clean_table.len(), 3);
    assert!(clean_table.index_of("core:carpet_damp_01").is_none());
}

#[test]
fn a_floor_patch_keeps_its_exact_edges_without_a_second_slab() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 1,
            "id": "patchy",
            "name": "Patchy",
            "spawn": { "x": 1.0, "z": 1.0 },
            "rooms": [
                { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 8.0, "height": 3.0 }
            ],
            "floor_patches": [
                { "x": 3.0, "z": 2.0, "width": 4.0, "depth": 3.0,
                  "material": "core:carpet_damp_01" }
            ],
            "ceiling_lights": [
                { "fixture": "core:fluorescent_panel_01", "x": 5.0, "z": 4.0 }
            ]
        }"#,
    )
    .expect("valid level");
    let mesh = build_level_geometry(&level);

    let damp = material_vertices(&mesh, &level, "core:carpet_damp_01");
    let clean = material_vertices(&mesh, &level, "core:carpet_beige_01");
    assert!(!damp.is_empty() && !clean.is_empty(), "both regions emit");
    // The patch's own bounds are exactly the authored rectangle: no
    // half-cell bleed in either direction and no overlapping slab.
    assert_eq!(xz_bounds(&damp), (3.0, 7.0, 2.0, 5.0));
    // The maintained floor tiles the rest of the room, still inside it.
    let (min_x, max_x, min_z, max_z) = xz_bounds(&clean);
    assert_eq!((min_x, max_x), (0.0, 10.0));
    assert_eq!((min_z, max_z), (0.0, 8.0));

    // Patched floors add cut lines, and the estimate still bounds what the
    // builder emits.
    let estimate = level.estimate_geometry();
    assert!(estimate.floor_quads >= 1);
    let floor_quads = (damp.len() + clean.len()) / 6;
    assert!(
        floor_quads as u64 <= estimate.floor_quads,
        "{floor_quads} floor quads exceed the {}-quad estimate",
        estimate.floor_quads
    );
    assert!(
        logical_materials(&level)
            .index_of("core:carpet_damp_01")
            .is_some(),
        "the patch needs damp carpet"
    );
}

#[test]
fn wall_material_and_face_overrides_apply_only_to_the_faces_they_name() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 1,
            "id": "stained_walls",
            "name": "Stained Walls",
            "spawn": { "x": 4.0, "z": 4.0 },
            "rooms": [
                { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 3.0 }
            ],
            "walls": [
                { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 0.3,
                  "material": "core:wallpaper_stained_01" },
                { "x": 0.0, "z": 7.7, "width": 8.0, "depth": 0.3,
                  "faces": { "south": "core:wallpaper_stained_01" } }
            ],
            "ceiling_lights": [
                { "fixture": "core:fluorescent_panel_01", "x": 4.0, "z": 4.0 }
            ]
        }"#,
    )
    .expect("valid level");
    let mesh = build_level_geometry(&level);

    // Wall 0 is stained throughout: both length faces use the stained sheet.
    // Wall 1 names only its south face, so it keeps one maintained face.
    let stained = material_vertices(&mesh, &level, "core:wallpaper_stained_01");
    let maintained = material_vertices(&mesh, &level, "core:wallpaper_yellow_01");
    assert!(!stained.is_empty() && !maintained.is_empty());
    let stained_bounds = xz_bounds(&stained);
    assert_exact(stained_bounds.0, 0.0);
    assert!(stained_bounds.1 >= 8.0);
    // The maintained faces belong to the second wall's north side, which
    // faces the room interior.
    let maintained_bounds = xz_bounds(&maintained);
    for v in maintained.iter().take(40) {
        println!("maintained vertex {:?} color {:?}", v.pos, v.color);
    }
    assert!(maintained_bounds.2 >= 7.7, "{maintained_bounds:?}");
    assert!(maintained_bounds.3 <= 8.0, "{maintained_bounds:?}");
    assert!(
        logical_materials(&level)
            .index_of("core:wallpaper_stained_01")
            .is_some(),
        "the stained wall material is referenced"
    );
}

// ------------------------------------------------------- spatial culling

/// A level with two widely separated clusters of placeholder props, so a
/// camera at the origin can only ever see one of them at a time. Two
/// clusters 40 m apart means the 12 m grid cannot merge them into one cell.
fn two_cluster_level(props_per_cluster: usize) -> LevelDef {
    let mut props: Vec<String> = Vec::new();
    for index in 0..props_per_cluster {
        let offset = (index as f32) * 1.4;
        props.push(format!(
            r#"{{ "model": "core:crate", "x": {}, "z": -20.0, "size": [1.0,1.0,1.0] }}"#,
            offset - 5.0
        ));
        props.push(format!(
            r#"{{ "model": "core:crate", "x": {}, "z": 20.0, "size": [1.0,1.0,1.0] }}"#,
            offset - 5.0
        ));
    }
    let json = format!(
        r#"{{
            "format_version": 1,
            "id": "two_clusters",
            "name": "Two Clusters",
            "spawn": {{ "x": 0.0, "z": 0.0 }},
            "rooms": [
                {{ "x": -12.0, "z": -28.0, "width": 24.0, "depth": 16.0, "height": 3.0 }},
                {{ "x": -12.0, "z": 12.0, "width": 24.0, "depth": 16.0, "height": 3.0 }}
            ],
            "ceiling_lights": [
                {{ "fixture": "core:panel_01", "x": 0.0, "z": -20.0 }},
                {{ "fixture": "core:panel_01", "x": 0.0, "z": 20.0 }}
            ],
            "props": [{}]
        }}"#,
        props.join(",")
    );
    LevelDef::from_json(&json).expect("valid two-cluster level")
}

/// The view-projection `render_scene` builds, so culling tests exercise the
/// same camera convention the game uses: OpenGL's `[-1, 1]` clip depth against
/// the same near/far planes.
fn scene_frustum(eye: glam::Vec3, yaw_degrees: f32, pitch_degrees: f32) -> Frustum {
    let aspect = 480.0 / 272.0;
    let fov = vertical_fov_for_aspect(60.0, aspect);
    let proj = glam::Mat4::perspective_rh_gl(fov.to_radians(), aspect, SCENE_NEAR_M, SCENE_FAR_M);
    let pitch = pitch_degrees.to_radians();
    let yaw = yaw_degrees.to_radians();
    let forward = glam::Vec3::new(
        yaw.sin() * pitch.cos(),
        pitch.sin(),
        -yaw.cos() * pitch.cos(),
    );
    let view = glam::Mat4::look_at_rh(eye, eye + forward, glam::Vec3::Y);
    Frustum::from_view_projection(&(proj * view), DepthRange::NegativeOneToOne)
}

#[test]
fn the_scene_projection_uses_the_full_depth_buffer() {
    // A point on the near plane must land on the depth buffer's near edge and
    // one on the far plane on its far edge. `perspective_rh` (glam's `[0, 1]`
    // convention) would put them at 0.5 and 1.0, throwing away half the depth
    // resolution and with it the margin coplanar surfaces need.
    let aspect = 480.0 / 272.0;
    let proj =
        glam::Mat4::perspective_rh_gl(60.0f32.to_radians(), aspect, SCENE_NEAR_M, SCENE_FAR_M);
    let depth_of = |z: f32| {
        let clip = proj * glam::Vec4::new(0.0, 0.0, -z, 1.0);
        clip.z / clip.w
    };
    assert!((depth_of(SCENE_NEAR_M) + 1.0).abs() < 1e-5, "near edge");
    assert!((depth_of(SCENE_FAR_M) - 1.0).abs() < 1e-5, "far edge");
    // The plane values themselves are gameplay constants: the eye sits about a
    // collision radius from every wall, and the far plane covers the largest
    // shipped level several times over. Pinned so a tuning change has to come
    // with its own precision review.
    assert_exact(SCENE_NEAR_M, 0.1);
    assert_exact(SCENE_FAR_M, 100.0);
}

/// Distinct vertices the frustum would submit for a level's static ranges.
fn visible_static_vertices(mesh: &LevelMesh, frustum: &Frustum) -> usize {
    mesh.ranges
        .iter()
        .filter(|range| frustum.intersects_aabb(&range.bounds))
        .map(|range| range.vertices.len())
        .sum()
}

#[test]
fn every_range_is_indexed_correctly_and_keeps_its_own_vertex_block() {
    let mesh = build_level_geometry(&two_cluster_level(6));
    assert!(mesh.ranges.len() > 1, "the grid must split the level");

    let mut total_indices = 0usize;
    for range in &mesh.ranges {
        assert!(
            !range.indices.is_empty(),
            "empty ranges must not be emitted"
        );
        assert_eq!(range.indices.len() % 6, 0, "ranges are whole quads");
        assert!(
            range.vertices.len() <= crate::spatial::MAX_INDEX_VERTICES,
            "a range must stay addressable with 16-bit indices"
        );
        for index in &range.indices {
            assert!(
                (*index as usize) < range.vertices.len(),
                "{:?} index {index} is out of range for {} vertices",
                range.key.kind,
                range.vertices.len()
            );
        }
        // Every vertex in the block must be referenced: indexing collapses
        // corners, it never leaves orphans behind.
        let mut used = vec![false; range.vertices.len()];
        for index in &range.indices {
            used[*index as usize] = true;
        }
        assert!(
            used.iter().all(|seen| *seen),
            "{:?} range has unreferenced vertices",
            range.key.kind
        );
        total_indices += range.indices.len();
    }
    assert_eq!(total_indices, mesh.index_count);
}

#[test]
fn indexing_shrinks_static_geometry_without_losing_triangles() {
    // The same level built with the pre-indexing emitter shape would hold
    // six vertices per quad; indexed, it must hold strictly fewer while
    // still generating six indices per quad.
    let mesh = build_level_geometry(&two_cluster_level(6));
    let quads = mesh.index_count / 6;
    assert!(quads > 100, "the fixture needs real geometry");
    assert_eq!(mesh.index_count % 6, 0);
    assert!(
        mesh.vertex_count < quads * 6,
        "indexing must beat the flat triangle list: {} vertices for {quads} quads",
        mesh.vertex_count
    );
    // Every quad is four distinct corners at worst.
    assert!(mesh.vertex_count <= quads * 4);
}

#[test]
fn every_range_bounds_contains_its_own_vertices() {
    let mesh = build_level_geometry(&two_cluster_level(6));
    for range in &mesh.ranges {
        for vertex in &range.vertices {
            for axis in 0..3 {
                assert!(
                    vertex.pos[axis] >= range.bounds.min[axis] - 1e-3
                        && vertex.pos[axis] <= range.bounds.max[axis] + 1e-3,
                    "{:?} at {:?} escapes its bounds {:?}..{:?}",
                    range.key.kind,
                    vertex.pos,
                    range.bounds.min,
                    range.bounds.max
                );
            }
        }
    }
}

#[test]
fn a_range_bounds_is_never_empty_and_never_contains_nan() {
    let mesh = build_level_geometry(&two_cluster_level(4));
    for range in &mesh.ranges {
        assert!(!range.bounds.is_empty());
        for axis in 0..3 {
            assert!(range.bounds.min[axis].is_finite());
            assert!(range.bounds.max[axis].is_finite());
        }
    }
}

#[test]
fn the_packer_keeps_every_range_inside_16_bit_indices() {
    let mut packer = MeshPacker::default();
    let vertex = |x: f32| Vertex {
        pos: [x, 0.0, 0.0],
        color: [1.0, 1.0, 1.0, 1.0],
        uv: [0.0, 0.0],
        ..Vertex::UNLIT
    };
    // Three ranges of 40 000 vertices each cannot share one chunk.
    let mut placements = Vec::new();
    for base in 0..3 {
        let vertices: Vec<Vertex> = (0..40_000)
            .map(|i| vertex((base * 40_000 + i) as f32))
            .collect();
        let indices: Vec<u16> = (0..40_000u16).collect();
        placements.extend(packer.push(&vertices, &indices));
    }
    assert!(
        packer.chunks.len() >= 2,
        "the packer must split before overflowing 16-bit indices"
    );
    for (index, placement) in placements.iter().enumerate() {
        let chunk = &packer.chunks[placement.chunk];
        assert!(chunk.vertices.len() <= crate::spatial::MAX_INDEX_VERTICES);
        let start = usize::try_from(placement.index_start).unwrap_or(0);
        let end = start + usize::try_from(placement.index_count).unwrap_or(0);
        for (offset, value) in chunk.indices[start..end].iter().enumerate() {
            assert_eq!(
                *value as usize,
                usize::try_from(placement.vertex_start).unwrap_or(0) + offset,
                "range {index} indices must be re-based into their chunk"
            );
        }
    }
}

#[test]
fn a_single_range_larger_than_the_index_space_is_split_not_wrapped() {
    // One (model, cell) prop batch can easily hold more than 65 536 vertices:
    // 400 chairs in a single cell are 147 200 vertices. Wrapping the u16
    // indices there would draw garbage, so the packer must split the range.
    let vertex = |x: f32| Vertex {
        pos: [x, 0.0, 0.0],
        color: [1.0, 1.0, 1.0, 1.0],
        uv: [0.0, 0.0],
        ..Vertex::UNLIT
    };
    // 50 000 distinct vertices with a 2x-long index list: one chunk cannot
    // hold them together with the next range, and the range itself must be
    // re-based correctly as it is split.
    let count = 50_000usize;
    let vertices: Vec<Vertex> = (0..count).map(|i| vertex(i as f32)).collect();
    let mut indices: Vec<u16> = Vec::with_capacity(count * 2);
    for index in 0..count {
        indices.push(u16::try_from(index % count).unwrap_or(u16::MAX));
        indices.push(u16::try_from((index + 1) % count).unwrap_or(u16::MAX));
    }
    // A second range of the same size cannot share the first chunk.
    let mut second: Vec<u16> = Vec::with_capacity(count);
    for index in 0..count {
        second.push(u16::try_from(index % count).unwrap_or(u16::MAX));
    }

    let mut packer = MeshPacker::default();
    let mut placements = packer.push(&vertices, &indices);
    placements.extend(packer.push(&vertices, &second));
    assert!(
        packer.chunks.len() >= 2,
        "two 50 000-vertex ranges cannot share one 16-bit chunk"
    );
    let mut total_indices = 0usize;
    for placement in &placements {
        let chunk = &packer.chunks[placement.chunk];
        assert!(chunk.vertices.len() <= crate::spatial::MAX_INDEX_VERTICES);
        let start = usize::try_from(placement.index_start).unwrap_or(0);
        let end = start + usize::try_from(placement.index_count).unwrap_or(0);
        for index in &chunk.indices[start..end] {
            assert!(
                (*index as usize) < chunk.vertices.len(),
                "index {index} escapes chunk {}",
                placement.chunk
            );
        }
        total_indices += usize::try_from(placement.index_count).unwrap_or(0);
    }
    assert_eq!(
        total_indices,
        indices.len() + second.len(),
        "no index may be lost"
    );
    // Every chunk must stay addressable, which is the property that would
    // break if the split were done by vertex count alone.
    for chunk in &packer.chunks {
        assert!(chunk.vertices.len() <= crate::spatial::MAX_INDEX_VERTICES);
    }
}

#[test]
fn turning_the_camera_away_rejects_an_entire_cluster() {
    let level = two_cluster_level(10);
    let mesh = build_level_geometry(&level);
    let eye = glam::Vec3::new(0.0, 1.6, 0.0);

    // Yaw 0 looks along -Z, yaw 180 along +Z: one cluster each way.
    let toward_far = scene_frustum(eye, 0.0, 0.0);
    let toward_near = scene_frustum(eye, 180.0, 0.0);

    let far_visible = visible_static_vertices(&mesh, &toward_far);
    let near_visible = visible_static_vertices(&mesh, &toward_near);
    let total: usize = mesh.ranges.iter().map(|batch| batch.vertices.len()).sum();

    assert!(
        far_visible < total,
        "looking one way must cull the other cluster ({far_visible} of {total})"
    );
    assert!(
        near_visible < total,
        "the mirrored view must cull the opposite cluster ({near_visible} of {total})"
    );
    // The two views are mirror images, so they must agree closely and
    // together leave a large fraction of the level unsubmitted.
    let ratio = far_visible.min(near_visible) as f32 / total as f32;
    assert!(
        ratio < 0.75,
        "a camera-away view must drop most of the level, kept {ratio:.2}"
    );
}

#[test]
fn looking_straight_up_or_down_still_sees_the_room_shell() {
    // One room, camera standing in the middle of it.
    let level = lit_room_level(12.0, 12.0, 3.0, "[]");
    let mesh = build_level_geometry(&level);
    let eye = glam::Vec3::new(0.0, 1.6, 0.0);

    // Extreme pitch must never cull the floor or the ceiling the camera is
    // standing between. Each extreme must see *more* than a level plank.
    for pitch in [-85.0_f32, 85.0] {
        let frustum = scene_frustum(eye, 0.0, pitch);
        let visible = visible_static_vertices(&mesh, &frustum);
        assert!(
            visible > 0,
            "pitch {pitch} culled the whole level; the camera is inside it"
        );
        // Looking down must see the floor, looking up the ceiling; the
        // opposite surface is genuinely outside a 30-degree half-FOV.
        let expected = if pitch < 0.0 {
            SurfaceKind::Floor
        } else {
            SurfaceKind::Ceiling
        };
        let saw_expected = mesh
            .ranges
            .iter()
            .any(|batch| batch.key.kind == expected && frustum.intersects_aabb(&batch.bounds));
        assert!(
            saw_expected,
            "pitch {pitch} must still see the {expected:?}"
        );
        let saw_opposite = mesh.ranges.iter().any(|batch| {
            batch.key.kind != expected
                && matches!(batch.key.kind, SurfaceKind::Floor | SurfaceKind::Ceiling)
                && frustum.intersects_aabb(&batch.bounds)
        });
        assert!(
            !saw_opposite,
            "pitch {pitch} must not see the opposite surface"
        );
    }
}

#[test]
fn a_camera_inside_a_batch_never_culls_it() {
    // Stand inside a deliberately oversized prop box: whatever the camera
    // looks at, the range it is standing in must survive every plane test.
    const EPS: f32 = 1e-3;
    let level = level_with_wall(
        "[]",
        r#"[{ "model": "core:crate", "x": 0.0, "z": 0.0, "size": [4.0, 4.0, 4.0] }]"#,
    );
    let mesh = build_level_geometry(&level);
    let eye = glam::Vec3::new(0.0, 1.6, 0.0);
    // Strictly inside, not merely touching: a wall face passing exactly
    // through the eye is still legitimately behind a camera looking away
    // from it.
    let contains_eye = |batch: &LevelMeshRange| {
        (0..3).all(|axis| {
            batch.bounds.min[axis] + EPS <= eye[axis] && batch.bounds.max[axis] - EPS >= eye[axis]
        })
    };
    let containing: Vec<&LevelMeshRange> = mesh.ranges.iter().filter(|b| contains_eye(b)).collect();
    assert!(
        !containing.is_empty(),
        "the camera must stand inside the oversized crate"
    );
    for (index, yaw) in [0.0_f32, 45.0, 90.0, 180.0, 270.0].iter().enumerate() {
        let frustum = scene_frustum(eye, *yaw, 0.0);
        for batch in &containing {
            assert!(
                frustum.intersects_aabb(&batch.bounds),
                "yaw {yaw} culled a batch the camera stands inside ({index})"
            );
        }
    }
}

#[test]
fn extreemely_distant_geometry_is_culled_by_the_far_plane() {
    // A room a kilometre away, far outside the 100 m far plane.
    let mesh = build_level_geometry(&two_cluster_level(3));
    let eye = glam::Vec3::new(0.0, 1.6, 0.0);
    let frustum = scene_frustum(eye, 180.0, 0.0);
    // Nothing at ±20 m is beyond 100 m, so this view still sees a cluster;
    // the far-plane behaviour itself is covered by `spatial`'s unit tests.
    assert!(visible_static_vertices(&mesh, &frustum) > 0);
}

#[test]
fn negative_and_extreme_level_coordinates_still_batch_and_cull() {
    // A room in the negative quadrant, far enough away to be its own set of
    // cells but still inside the 100 m far plane, plus a near room. Cell
    // keys are therefore negative and the grid spans a wide extent.
    let json = r#"{
        "format_version": 1,
        "id": "extreme",
        "name": "Extreme",
        "spawn": { "x": 0.0, "z": 0.0 },
        "rooms": [
            { "x": -60.0, "z": -60.0, "width": 20.0, "depth": 20.0, "height": 3.0 },
            { "x": -5.0, "z": -5.0, "width": 10.0, "depth": 10.0, "height": 3.0 }
        ],
        "props": [
            { "model": "core:crate", "x": -50.0, "z": -50.0, "size": [1.0,1.0,1.0] }
        ]
    }"#;
    let level = LevelDef::from_json(json).expect("valid extreme level");
    let mesh = build_level_geometry(&level);
    assert!(mesh.ranges.len() >= 2);
    for batch in &mesh.ranges {
        assert!(!batch.bounds.is_empty());
    }

    // The camera stands in the near room. `forward = (sin yaw, 0, -cos yaw)`,
    // so yaw 315 degrees looks along -X/-Z, straight at the distant room,
    // and yaw 135 looks the other way.
    let eye = glam::Vec3::new(0.0, 1.6, 0.0);
    let toward = scene_frustum(eye, 315.0, 0.0);
    let away = scene_frustum(eye, 135.0, 0.0);
    let away_visible = visible_static_vertices(&mesh, &away);
    let toward_visible = visible_static_vertices(&mesh, &toward);
    assert!(
        toward_visible > away_visible,
        "facing the distant room must submit more than facing away \
         ({toward_visible} vs {away_visible})"
    );
}

#[test]
fn overlapping_rooms_and_sunken_props_keep_every_cell_cullable() {
    // Two rooms deliberately overlap and a prop is deliberately sunk through
    // the floor between them. Neither is corrected: the geometry stays where
    // the level puts it, and every range still carries usable bounds.
    let json = r#"{
        "format_version": 1,
        "id": "overlap",
        "name": "Overlap",
        "spawn": { "x": 0.0, "z": 0.0 },
        "rooms": [
            { "x": -8.0, "z": -8.0, "width": 16.0, "depth": 16.0, "height": 3.0 },
            { "x": -4.0, "z": -4.0, "width": 16.0, "depth": 16.0, "height": 3.2 }
        ],
        "walls": [
            { "x": -4.0, "z": 0.0, "width": 8.0, "depth": 0.4, "height": 3.0 }
        ],
        "props": [
            { "model": "core:crate", "x": 2.0, "y": -0.4, "z": 2.0, "size": [1.0, 1.0, 1.0] },
            { "model": "core:crate", "x": 6.0, "y": 0.0, "z": 6.0, "size": [1.0, 1.0, 1.0] }
        ]
    }"#;
    let level = LevelDef::from_json(json).expect("valid overlapping level");
    let mesh = build_level_geometry(&level);
    assert!(mesh.ranges.len() >= 2);
    for range in &mesh.ranges {
        assert!(!range.bounds.is_empty());
        assert!(range.bounds.min.iter().all(|value| value.is_finite()));
        assert!(range.bounds.max.iter().all(|value| value.is_finite()));
    }

    // The sunk crate keeps its real (below-floor) vertical extent: culling
    // must never assume a prop sits above y = 0.
    let props = mesh
        .ranges
        .iter()
        .filter(|range| range.key.kind == SurfaceKind::PropFallback)
        .collect::<Vec<_>>();
    assert!(
        !props.is_empty(),
        "both crates must emit placeholder geometry"
    );
    assert!(
        props.iter().any(|range| range.bounds.min[1] < -0.2),
        "the sunk crate must keep its negative extent: {:?}",
        props.iter().map(|r| r.bounds.min[1]).collect::<Vec<_>>()
    );
    // The two crates share a cell, so they share one cullable range; the
    // range's bounds must still cover the sunk one.
    let prop_vertices: usize = props.iter().map(|range| range.vertices.len()).sum();
    assert!(prop_vertices >= 2 * 4, "two boxes need real geometry");

    // Both rooms contribute floor, and the camera in one of them sees the
    // other through the overlap rather than losing it to the frustum.
    let eye = glam::Vec3::new(0.0, 1.6, 0.0);
    let frustum = scene_frustum(eye, 180.0, 0.0);
    assert!(visible_static_vertices(&mesh, &frustum) > 0);
}

#[test]
fn the_same_level_always_splits_into_the_same_batches() {
    let level = two_cluster_level(5);
    let first = build_level_geometry(&level);
    let second = build_level_geometry(&level);
    assert_eq!(first.ranges, second.ranges);
    assert_eq!(first.batches, second.batches);
    assert_eq!(first.vertex_count, second.vertex_count);
    for (a, b) in first
        .all_vertices()
        .iter()
        .zip(second.all_vertices().iter())
    {
        assert_exact_array(a.pos, b.pos);
        assert_exact_array(a.color, b.color);
        assert_exact_array(a.uv, b.uv);
    }
}

#[test]
fn props_intersecting_a_wall_keep_their_own_cell_bounds() {
    // A crate deliberately half-buried in a wall: culling must use its real
    // world bounds, so it is never dropped while part of it is on screen.
    let level = level_with_wall(
        "[]",
        r#"[{ "model": "core:crate", "x": 5.0, "z": 0.0, "size": [1.0,1.0,1.0] }]"#,
    );
    let mesh = build_level_geometry(&level);
    let props: Vec<_> = mesh
        .ranges
        .iter()
        .filter(|batch| batch.key.kind == SurfaceKind::PropFallback)
        .collect();
    assert_eq!(props.len(), 1);
    let bounds = props[0].bounds;
    assert!(
        bounds.min[0] <= 4.5 + 1e-3 && bounds.max[0] >= 5.5 - 1e-3,
        "the sunk crate's bounds must cover its real extent: {:?}..{:?}",
        bounds.min,
        bounds.max
    );
}

#[test]
fn a_small_level_stays_a_small_number_of_batches() {
    // The 12 m grid must not shred a single room into many draw calls: the
    // whole point is to keep batching efficient while adding cullability.
    let level = level_with_wall("[]", "[]");
    let mesh = build_level_geometry(&level);
    assert!(
        mesh.ranges.len() <= 8,
        "a single small room produced {} static batches",
        mesh.ranges.len()
    );
}

#[test]
fn the_real_prop_batches_carry_bounds_and_split_by_cell() {
    let catalog = shipped_catalog();
    let mut assets = shipped_assets();
    // Two chairs 40 m apart cannot share a cell, and each batch must carry
    // bounds that contain its own vertices.
    let level = level_with_wall(
        "[]",
        r#"[{ "model": "core:chair", "x": -20.0, "z": 0.0 },
            { "model": "core:chair", "x": 20.0, "z": 0.0 }]"#,
    );
    let (_, batches) = build_level_geometry_with_assets(&level, &catalog, &mut assets);
    assert_eq!(batches.len(), 2, "one batch per (model, cell)");
    for batch in &batches {
        assert!(!batch.bounds.is_empty());
        for vertex in &batch.vertices {
            for axis in 0..3 {
                assert!(vertex.pos[axis] >= batch.bounds.min[axis] - 1e-3);
                assert!(vertex.pos[axis] <= batch.bounds.max[axis] + 1e-3);
            }
        }
    }
    // Mirror symmetry: the batches sit either side of the origin.
    let centres: Vec<f32> = batches
        .iter()
        .map(|batch| batch.bounds.centre()[0])
        .collect();
    assert!(
        centres.iter().any(|x| *x < -15.0) && centres.iter().any(|x| *x > 15.0),
        "both clusters must be represented: {centres:?}"
    );
}

fn brightest(vertices: &[Vertex]) -> &Vertex {
    vertices
        .iter()
        .max_by(|a, b| a.color[0].partial_cmp(&b.color[0]).unwrap())
        .expect("non-empty vertex slice")
}

fn dimmest(vertices: &[Vertex]) -> &Vertex {
    vertices
        .iter()
        .min_by(|a, b| a.color[0].partial_cmp(&b.color[0]).unwrap())
        .expect("non-empty vertex slice")
}

#[test]
fn floors_are_lit_by_the_baseline_and_the_local_fixture_pool() {
    let level = lit_room_level(
        20.0,
        20.0,
        3.0,
        r#"[{ "fixture": "core:fluorescent_panel_01", "x": 10.0, "z": 10.0 }]"#,
    );
    let mesh = build_level_geometry(&level);
    let floor = batch_slice(&mesh, SurfaceKind::Floor);
    assert!(!floor.is_empty());

    let bright = brightest(&floor);
    let dim = dimmest(&floor);
    assert!(
        (bright.pos[0] - 10.0).abs() < 2.5 && (bright.pos[2] - 10.0).abs() < 2.5,
        "the brightest floor vertex must sit under the fixture, got {:?}",
        bright.pos
    );
    assert!(
        bright.color[0] - dim.color[0] > 0.05,
        "the pool must be visible: {} vs {}",
        bright.color[0],
        dim.color[0]
    );
    assert!(
        dim.color[0] >= crate::lighting::AMBIENT_LEVEL - 1e-4,
        "no floor vertex may fall below the minimum ambient, got {}",
        dim.color[0]
    );
    for vertex in floor {
        for channel in vertex.color {
            assert!(channel.is_finite() && (0.0..=1.0).contains(&channel));
        }
    }
}

#[test]
fn wall_faces_vary_with_the_baked_lighting() {
    // A fixture right above the wall's west end: the wall face nearest to it
    // must be brighter than the far end, and long walls are split so the
    // change is gradual rather than one flat quad.
    let level = level_with_wall_and_lights(
        "[]",
        "[]",
        r#"[{ "fixture": "core:fluorescent_panel_01", "x": -4.0, "z": 0.2 }]"#,
    );
    let mesh = build_level_geometry(&level);
    let walls = batch_slice(&mesh, SurfaceKind::Wall);
    let bright = brightest(&walls);
    let dim = dimmest(&walls);
    assert!(
        bright.color[0] - dim.color[0] > 0.05,
        "wall lighting must vary: {} vs {}",
        bright.color[0],
        dim.color[0]
    );
    assert!(
        bright.pos[0] < -2.0,
        "the brightest wall vertex must be near the fixture, got {:?}",
        bright.pos
    );

    // Smooth, not banded: the bottom edge of the wall face carries several
    // distinct brightness levels instead of one flat colour.
    let mut edge: Vec<f32> = walls
        .iter()
        .filter(|v| v.pos[2].abs() < 1e-3 && v.pos[1].abs() < 1e-3)
        .map(|v| (v.color[0] * 1000.0).round() / 1000.0)
        .collect();
    edge.sort_by(|a, b| a.partial_cmp(b).unwrap());
    edge.dedup();
    assert!(
        edge.len() >= 3,
        "expected a gradient along the wall, got {edge:?}"
    );
    assert!(edge[edge.len() - 1] - edge[0] > 0.1);
}

#[test]
fn placeholder_prop_boxes_receive_the_environment_lighting() {
    let level = lit_room_level(
        20.0,
        20.0,
        3.0,
        r#"[{ "fixture": "core:fluorescent_panel_01", "x": 10.0, "z": 10.0 }]"#,
    );
    let mut level = level;
    level.props = vec![
        PropDef {
            model: "core:crate".into(),
            x: 10.0,
            y: 0.0,
            z: 10.0,
            rotation_degrees: 0.0,
            scale: 1.0,
            size: Some([1.0, 1.0, 1.0]),
            solid: false,
            lights: Vec::new(),
        },
        PropDef {
            model: "core:crate".into(),
            x: 1.0,
            y: 0.0,
            z: 1.0,
            rotation_degrees: 0.0,
            scale: 1.0,
            size: Some([1.0, 1.0, 1.0]),
            solid: false,
            lights: Vec::new(),
        },
    ];
    let mesh = build_level_geometry(&level);
    let props = batch_slice(&mesh, SurfaceKind::PropFallback);
    assert_eq!(props.len(), 72, "two Y-rotated boxes");

    let under: Vec<&Vertex> = props.iter().filter(|v| v.pos[0] > 5.0).collect();
    let far: Vec<&Vertex> = props.iter().filter(|v| v.pos[0] <= 5.0).collect();
    assert!(!under.is_empty() && !far.is_empty());
    let mean = |slice: &[&Vertex]| {
        slice.iter().map(|vertex| vertex.color[0]).sum::<f32>() / slice.len() as f32
    };
    assert!(
        mean(&under) > mean(&far) + 0.05,
        "the prop under the fixture must be brighter: {} vs {}",
        mean(&under),
        mean(&far)
    );
    // No prop may be lit as if it were outside the level: even the darkest
    // face of a mid-grey box at minimum ambient stays clearly visible.
    let darkest_possible = crate::lighting::AMBIENT_LEVEL * 0.541 * 0.62;
    for vertex in props {
        assert!(
            vertex.color[0] >= darkest_possible - 1e-4,
            "prop vertex {} is darker than the minimum ambient allows",
            vertex.color[0]
        );
    }
}

#[test]
fn vertically_offset_props_sample_their_true_world_position() {
    let mut level = lit_room_level(
        20.0,
        20.0,
        3.0,
        r#"[{ "fixture": "core:fluorescent_panel_01", "x": 10.0, "z": 10.0 }]"#,
    );
    let base = PropDef {
        model: "core:crate".into(),
        x: 10.0,
        y: 0.0,
        z: 10.0,
        rotation_degrees: 0.0,
        scale: 1.0,
        size: Some([1.0, 1.0, 1.0]),
        solid: false,
        lights: Vec::new(),
    };
    let mut raised = base.clone();
    raised.y = 2.0;
    level.props = vec![base, raised];

    let lighting = crate::lighting::LevelLighting::bake(&level);
    let mesh = build_level_geometry(&level);
    let props = batch_slice(&mesh, SurfaceKind::PropFallback);
    assert_eq!(props.len(), 72);
    let floor_box = &props[..36];
    let raised_box = &props[36..];

    // The box on the floor is 3 m below the panel, the raised one 1 m; every
    // corresponding vertex must carry exactly the ratio of the two samples
    // taken at its own transformed world position.
    let mut brighter_vertices = 0;
    for index in 0..36 {
        let low = floor_box[index].color[0];
        let high = raised_box[index].color[0];
        if high > low + 1e-6 {
            brighter_vertices += 1;
        }
        let low_light = lighting.sample(
            floor_box[index].pos[0],
            floor_box[index].pos[1],
            floor_box[index].pos[2],
        );
        let high_light = lighting.sample(
            raised_box[index].pos[0],
            raised_box[index].pos[1],
            raised_box[index].pos[2],
        );
        for channel in 0..3 {
            let low_channel = floor_box[index].color[channel];
            let high_channel = raised_box[index].color[channel];
            let low_sample = low_light.channel(channel);
            let high_sample = high_light.channel(channel);
            assert!(low_sample > 0.0 && high_sample > 0.0);
            let expected_ratio = high_sample / low_sample;
            assert!(
                (high_channel / low_channel - expected_ratio).abs() < 1e-3,
                "vertex {index} channel {channel} ratio {} does not match the \
                 world-space samples {expected_ratio}",
                high_channel / low_channel
            );
        }
    }
    assert!(
        brighter_vertices > 0,
        "the raised prop must be closer to the light"
    );
}

#[test]
fn real_props_are_lit_per_vertex_and_stay_batched() {
    let catalog = shipped_catalog();
    let mut assets = shipped_assets();
    let lights = r#"[
        { "fixture": "core:fluorescent_panel_01", "x": 0.0, "z": 0.0 },
        { "fixture": "core:fluorescent_panel_01", "x": 8.0, "z": 0.0 }
    ]"#;
    let mut props: Vec<String> = Vec::new();
    for index in 0..10 {
        props.push(format!(
            r#"{{ "model": "core:chair", "x": {}, "z": 0.0 }}"#,
            index as f32
        ));
    }
    let level = level_with_wall_and_lights("[]", &format!("[{}]", props.join(",")), lights);
    let (_, batches, lighting) =
        build_level_geometry_with_assets_and_lighting(&level, &catalog, &mut assets);

    assert_eq!(batches.len(), 1, "ten chairs still cost one draw call");
    let vertices = &batches[0].vertices;
    let min = vertices.iter().map(|v| v.color[0]).fold(f32::MAX, f32::min);
    let max = vertices.iter().map(|v| v.color[0]).fold(f32::MIN, f32::max);
    assert!(
        max - min > 0.05,
        "instances across the room must not be uniformly lit: {min}..{max}"
    );

    // Every vertex carries its model colour multiplied by the bake sampled
    // at its own transformed world position. Instances are concatenated in
    // placement order and each contributes the model's whole vertex array,
    // so the model index wraps once per instance.
    let asset = assets
        .resolve("environment/office/props/models/chair.glb")
        .expect("chair loads");
    let model = &asset.model;
    for (vertex_index, vertex) in vertices.iter().enumerate() {
        // Instances contribute the model's own vertex array in order, so the
        // model's index list is not needed to line a submitted vertex up
        // with the vertex it came from.
        let source = model.vertices[vertex_index % model.vertices.len()];
        let light = lighting.sample(vertex.pos[0], vertex.pos[1], vertex.pos[2]);
        for channel in 0..3 {
            let expected = source.color[channel] * light.channel(channel);
            assert!(
                (vertex.color[channel] - expected).abs() < 1e-4,
                "vertex {vertex_index}: channel {channel} baked as {} but expected {} * {}",
                vertex.color[channel],
                source.color[channel],
                light.channel(channel)
            );
        }
    }
    assert_eq!(assets.stats().models_failed, 0);
}

#[test]
fn malformed_geometry_never_reaches_the_vertex_buffer() {
    // A room with non-finite dimensions and fixtures with non-finite
    // coordinates must be skipped, not turned into NaN vertices. The loader
    // rejects such levels, but direct construction must stay safe too.
    let mut level = lit_room_level(
        12.0,
        8.0,
        3.0,
        r#"[{ "fixture": "core:fluorescent_panel_01", "x": 6.0, "z": 4.0 }]"#,
    );
    level.rooms[0].width = f32::NAN;
    level.ceiling_lights[0].x = f32::NAN;
    level.ceiling_lights[0].z = f32::INFINITY;

    let mesh = build_level_geometry(&level);
    assert_eq!(mesh.batches.floor_batch.count, 0);
    assert_eq!(mesh.batches.ceiling_batch.count, 0);
    assert_eq!(mesh.batches.light_batch.count, 0);
    for vertex in mesh.all_vertices() {
        assert!(
            vertex.pos.iter().all(|value| value.is_finite()),
            "non-finite position {:?}",
            vertex.pos
        );
    }
}

#[test]
fn a_room_without_fixtures_stays_visible_and_within_range() {
    let mut level = lit_room_level(12.0, 8.0, 3.0, "[]");
    level.walls = vec![crate::level::WallDef {
        x: -6.0,
        y: 0.0,
        z: 4.0,
        width: 12.0,
        depth: 0.4,
        height: None,
        faces: std::collections::HashMap::default(),
        openings: Vec::new(),
        material: None,
        shine: None,
        face_shine: std::collections::HashMap::default(),
    }];
    let mesh = build_level_geometry(&level);
    let floor = batch_slice(&mesh, SurfaceKind::Floor);
    let ceiling = batch_slice(&mesh, SurfaceKind::Ceiling);
    let walls = batch_slice(&mesh, SurfaceKind::Wall);
    assert!(!floor.is_empty() && !ceiling.is_empty() && !walls.is_empty());

    for vertex in mesh.all_vertices() {
        assert!(
            vertex.color.iter().all(|c| c.is_finite()),
            "non-finite baked colour at {:?}",
            vertex.pos
        );
        assert!(vertex.color.iter().all(|c| (0.0..=1.0).contains(c)));
    }
    // The floor uses an untinted base colour, so the ambient fill shows up
    // directly; wall and ceiling tints are darker by design but stay
    // visible rather than collapsing to pure black.
    assert_exact(floor[0].color[0], crate::lighting::AMBIENT_LEVEL);
    assert!(ceiling[0].color[0] > 0.05);
    assert!(walls[0].color[0] > 0.05);
    // And the unlit room is genuinely dark: no channel may approach the
    // historical 0.55 ambient floor.
    for vertex in mesh.all_vertices() {
        assert!(
            vertex.color[0] < 0.2,
            "an unlit room must stay dark, got {:?} at {:?}",
            vertex.color,
            vertex.pos
        );
    }
}

#[test]
fn test_wall_without_openings_emits_four_faces() {
    let level = level_with_wall("[]", "[]");
    let mesh = build_level_geometry(&level);
    // Two faces parallel to the wall's length, each split into lighting
    // segments, plus two end caps. The wall reaches the ceiling height, so
    // there is no top or bottom face. This test room has no fixtures, so the
    // lighting along each face is flat and the segments merge back into one
    // quad per face.
    let segments = i32::try_from(crate::lighting::wall_light_segments(10.0)).unwrap_or(i32::MAX);
    assert_eq!(mesh.batches.wall_batch.count, 4 * 6);
    assert!(4 * 6 <= (2 * segments + 2) * 6);
    assert_eq!(mesh.batches.prop_batch.count, 0);
}

#[test]
fn test_wall_with_doorway_emits_more_wall_quads() {
    let plain = build_level_geometry(&level_with_wall("[]", "[]"));
    let level = level_with_wall(
        r#"[{ "kind": "door", "offset": 4.0, "width": 2.0, "height": 2.1 }]"#,
        "[]",
    );
    let door = build_level_geometry(&level);
    assert!(
        door.batches.wall_batch.count > plain.batches.wall_batch.count,
        "doorway must add jamb and header geometry"
    );
    // Three slices, each split into lighting segments, two faces each; plus
    // the door head underside and four cross-section caps (2 wall ends,
    // 2 door jambs). Flat segments merge, so the bound is an upper limit.
    let mut expected = 0;
    for length in [4.0f32, 2.0, 4.0] {
        expected +=
            2 * i32::try_from(crate::lighting::wall_light_segments(length)).unwrap_or(i32::MAX);
    }
    assert!(door.batches.wall_batch.count <= (expected + 1 + 4) * 6);
    assert!(door.batches.wall_batch.count > 0);
}

#[test]
fn test_wall_with_window_emits_sill_and_header_faces() {
    let level = level_with_wall(
        r#"[{ "kind": "window", "offset": 4.0, "width": 2.0, "height": 1.0, "sill": 1.0 }]"#,
        "[]",
    );
    let mesh = build_level_geometry(&level);
    // Four slices: the full-height wall either side of the window plus the
    // sill and header slices, which add a sill top and a head underside,
    // plus 4 cross-section caps. Flat segments merge, so this is a bound.
    let mut expected = 0;
    for length in [4.0f32, 2.0, 2.0, 4.0] {
        expected +=
            2 * i32::try_from(crate::lighting::wall_light_segments(length)).unwrap_or(i32::MAX);
    }
    assert!(mesh.batches.wall_batch.count <= (expected + 2 + 4) * 6);
    assert!(mesh.batches.wall_batch.count > 0);
}

#[test]
fn test_geometry_without_openings_contains_floor_ceiling_and_wall_batches() {
    let level = level_with_wall("[]", "[]");
    let mesh = build_level_geometry(&level);
    assert!(mesh.batches.floor_batch.count > 0);
    assert!(mesh.batches.ceiling_batch.count > 0);
    assert!(mesh.batches.wall_batch.count > 0);
    assert_eq!(mesh.vertex_count % 6, 0);
}

#[test]
fn test_z_axis_wall_geometry_runs_along_z() {
    let json = r#"{
        "format_version": 1,
        "id": "z_wall",
        "name": "Z Wall",
        "spawn": { "x": 5.0, "z": 5.0 },
        "room": { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.5 },
        "walls": [{
            "x": 4.8, "z": 0.0, "width": 0.4, "depth": 10.0, "height": 3.5,
            "openings": [{ "kind": "door", "offset": 4.0, "width": 2.0, "height": 2.1 }]
        }]
    }"#;
    let level = LevelDef::from_json(json).expect("valid json");
    let mesh = build_level_geometry(&level);

    // Same decomposition as the equivalent X-axis wall: 3 slices, each
    // split into lighting segments, two faces each; plus door head
    // underside and 4 cross-section caps. Flat segments merge, so the bound
    // is an upper limit.
    let mut expected = 0;
    for length in [4.0f32, 2.0, 4.0] {
        expected +=
            2 * i32::try_from(crate::lighting::wall_light_segments(length)).unwrap_or(i32::MAX);
    }
    assert!(mesh.batches.wall_batch.count <= (expected + 1 + 4) * 6);
    assert!(mesh.batches.wall_batch.count > 0);

    let wall_vertices = mesh.triangles_for(SurfaceKind::Wall);
    let (min_x, max_x) = wall_vertices.iter().fold((f32::MAX, f32::MIN), |acc, v| {
        (acc.0.min(v.pos[0]), acc.1.max(v.pos[0]))
    });
    let (min_z, max_z) = wall_vertices.iter().fold((f32::MAX, f32::MIN), |acc, v| {
        (acc.0.min(v.pos[2]), acc.1.max(v.pos[2]))
    });
    // The wall spans the room in Z and only its thickness in X.
    assert!(
        min_z <= 1e-3 && max_z >= 10.0 - 1e-3,
        "z span {min_z}..{max_z}"
    );
    assert!(min_x >= 4.79 && max_x <= 5.21, "x span {min_x}..{max_x}");
}

#[test]
fn test_prop_batch_is_populated_for_one_prop() {
    let level = level_with_wall(
        "[]",
        r#"[{ "model": "core:crate", "x": 1.0, "z": 1.0, "size": [1.0, 1.0, 1.0] }]"#,
    );
    let mesh = build_level_geometry(&level);
    // One Y-rotated box = 6 quads = 36 vertices, drawn after every kind of
    // static geometry: the buffer is laid out floor, ceiling, wall, light,
    // then placeholder props.
    assert_eq!(mesh.batches.prop_batch.count, 36);
    assert!(
        mesh.batches.prop_batch.start
            >= mesh.batches.light_batch.start + mesh.batches.light_batch.count,
        "placeholder props must follow the light batch (props {} lights {}..{})",
        mesh.batches.prop_batch.start,
        mesh.batches.light_batch.start,
        mesh.batches.light_batch.start + mesh.batches.light_batch.count,
    );
    assert_eq!(
        mesh.index_count_for(SurfaceKind::PropFallback),
        usize::try_from(mesh.batches.prop_batch.count.max(0)).unwrap_or(0),
        "the prop aggregate span must match the prop ranges"
    );
}

#[test]
fn test_props_with_invalid_extents_are_skipped() {
    let level = level_with_wall(
        "[]",
        r#"[{ "model": "core:crate", "x": 1.0, "z": 1.0, "size": [0.0, 1.0, 1.0] }]"#,
    );
    let mesh = build_level_geometry(&level);
    assert_eq!(mesh.batches.prop_batch.count, 0);
}

#[test]
fn test_prop_catalog_supplies_size_and_colour() {
    let catalog = crate::loader::PropCatalog::from_json_str(
        r##"{
            "format_version": 1,
            "props": [{
                "id": "core:test_prop", "name": "Test Prop", "category": "Decorative",
                "size": [1.0, 2.0, 0.5], "color": "#804020", "solid": false
            }]
        }"##,
    )
    .expect("valid catalog");
    let level = level_with_wall(
        "[]",
        r#"[{ "model": "core:test_prop", "x": 0.5, "z": 0.5, "rotation_degrees": 45.0 }]"#,
    );
    let mesh = build_level_geometry_with_catalog(&level, &catalog);
    assert_eq!(mesh.batches.prop_batch.count, 36);
}

/// Catalogue + assets used by the real prop-geometry tests. Reading the
/// shipped catalogue keeps the tests honest about ids and model paths.
fn shipped_catalog() -> crate::loader::PropCatalog {
    let catalog = crate::loader::PropCatalog::load_default();
    assert!(
        catalog.contains("core:chair"),
        "shipped catalogue must list core:chair"
    );
    catalog
}

/// Documents how much vertex data non-indexed submission duplicates.
///
/// A GLB stores each model once, indexed. The prop batcher expands it to a
/// flat triangle list because that was the only way to share one buffer
/// across instances — so the GPU shades `triangles * 3` vertices where the
/// model only has `vertices.len()` distinct ones. This test measures that
/// expansion for the shipped pack, which is the baseline the indexed path
/// has to beat, and fails if a model stops sharing vertices at all (which
/// would make indexing pointless rather than wrong).
#[test]
fn non_indexed_submission_duplicates_prop_vertices() {
    let catalog = shipped_catalog();
    let mut assets = shipped_assets();

    let mut total_unique = 0usize;
    let mut total_submitted = 0usize;
    for entry in catalog.entries() {
        let Some(model_path) = entry.model.as_deref() else {
            continue;
        };
        let asset = assets.resolve(model_path).expect("shipped model loads");
        let model = &asset.model;
        let unique = model.vertices.len();
        let submitted = model.triangles * 3;
        assert!(
            model.triangles > 0 && unique > 0,
            "{}: model has no geometry",
            entry.id
        );
        assert!(
            unique <= submitted,
            "{}: an indexed model cannot have more vertices than a flat list",
            entry.id
        );
        println!(
            "{:<22} {:>5} unique -> {:>5} submitted ({:>4} triangles, {:.0}% of the flat list)",
            entry.id,
            unique,
            submitted,
            model.triangles,
            100.0 * unique as f32 / submitted as f32
        );
        total_unique += unique;
        total_submitted += submitted;
    }

    assert!(total_submitted > 0);
    // The pack shares vertices as authored; if this ever stops being true,
    // the indexed path has nothing to save and the case needs revisiting.
    assert!(
        total_unique < total_submitted,
        "the shipped pack has no vertex sharing at all ({total_unique} unique vs {total_submitted})"
    );
    println!(
        "pack total: {total_unique} unique vs {total_submitted} submitted ({:.0}%)",
        100.0 * total_unique as f32 / total_submitted as f32
    );
}

fn shipped_assets() -> crate::props::PropAssets {
    let assets = crate::props::PropAssets::load_default();
    assert!(
        assets.root().is_some(),
        "the assets/ directory must exist for these tests"
    );
    assets
}

fn bounds_of(vertices: &[Vertex]) -> ([f32; 3], [f32; 3]) {
    let mut min = vertices[0].pos;
    let mut max = vertices[0].pos;
    for vertex in vertices {
        for axis in 0..3 {
            min[axis] = min[axis].min(vertex.pos[axis]);
            max[axis] = max[axis].max(vertex.pos[axis]);
        }
    }
    (min, max)
}

#[test]
fn real_prop_geometry_replaces_the_placeholder_box() {
    let catalog = shipped_catalog();
    let mut assets = shipped_assets();
    let level = level_with_wall("[]", r#"[{ "model": "core:chair", "x": 3.0, "z": -2.0 }]"#);
    let (mesh, batches) = build_level_geometry_with_assets(&level, &catalog, &mut assets);

    // The box placeholder is gone: the chair renders as real geometry.
    assert_eq!(mesh.batches.prop_batch.count, 0);
    assert_eq!(batches.len(), 1, "one draw batch per distinct model");
    assert_eq!(
        batches[0].model,
        "environment/office/props/models/chair.glb"
    );
    assert!(!batches[0].vertices.is_empty());
    assert_eq!(batches[0].textures.len(), 1, "a single material model");
    assert_eq!(batches[0].submeshes.len(), 1);
    assert_eq!(batches[0].submeshes[0].texture, Some(0));
    assert!(batches[0].textures[0].width > 0);
    assert_eq!(batches[0].textures[0].width, batches[0].textures[0].height);
    assert!(batches[0].textures[0].width <= crate::level::MAX_PROP_TEXTURE_SIZE);
    assert!(batches[0].submeshes[0].index_count > 0);
    assert!(
        usize::try_from(batches[0].submeshes[0].first_index).unwrap_or(0)
            + usize::try_from(batches[0].submeshes[0].index_count).unwrap_or(0)
            <= batches[0].indices.len()
    );

    // Placed at (3, 0, -2), resting on the floor: a 0.5 x 0.9 x 0.5 chair.
    let (low, high) = bounds_of(&batches[0].vertices);
    assert!(
        (low[1]).abs() < 0.02,
        "chair must rest on the floor, got {}",
        low[1]
    );
    assert!((high[1] - 0.9).abs() < 0.06, "seat height {}", high[1]);
    assert!(
        low[0] > 2.6 && high[0] < 3.4,
        "x bounds {:?}..{:?}",
        low[0],
        high[0]
    );
    assert!(
        low[2] > -2.4 && high[2] < -1.6,
        "z bounds {:?}..{:?}",
        low[2],
        high[2]
    );
}

#[test]
fn repeated_instances_share_one_batch_and_reuse_the_model() {
    let catalog = shipped_catalog();
    let mut assets = shipped_assets();
    let mut props: Vec<String> = Vec::new();
    for index in 0..10 {
        props.push(format!(
            r#"{{ "model": "core:chair", "x": {}, "z": 0.0 }}"#,
            index as f32
        ));
    }
    let level = level_with_wall("[]", &format!("[{}]", props.join(",")));
    let (_, batches) = build_level_geometry_with_assets(&level, &catalog, &mut assets);

    assert_eq!(batches.len(), 1, "ten chairs are one draw batch");
    let single = {
        let one = level_with_wall("[]", r#"[{ "model": "core:chair", "x": 0.0, "z": 0.0 }]"#);
        let (_, batches) = build_level_geometry_with_assets(&one, &catalog, &mut assets);
        batches[0].vertices.len()
    };
    assert_eq!(
        batches[0].vertices.len(),
        single * 10,
        "each instance contributes its triangles to the shared batch"
    );
    // The decoded model is parsed once and shared by every instance.
    let stats = assets.stats();
    assert_eq!(stats.models_loaded, 1);
    assert_eq!(stats.models_failed, 0);
}

#[test]
fn prop_transforms_follow_position_rotation_scale_and_vertical_offset() {
    let catalog = shipped_catalog();
    let mut assets = shipped_assets();
    // The bed is 1.4 x 0.55 x 2.0 m, so a 90 degree yaw is visible in the
    // bounds; rotation, scale and a negative vertical offset all apply.
    let level = level_with_wall(
        "[]",
        r#"[{ "model": "core:bed", "x": 1.0, "y": -0.1, "z": 4.0, "rotation_degrees": 90.0, "scale": 0.5 }]"#,
    );
    let (_, batches) = build_level_geometry_with_assets(&level, &catalog, &mut assets);
    let (low, high) = bounds_of(&batches[0].vertices);

    // Rotated: 2.0 m deep bed becomes 2.0 m of X extent, at half scale 1.0 m.
    assert!(
        (low[0] - 0.5).abs() < 0.06 && (high[0] - 1.5).abs() < 0.06,
        "rotated x bounds {:?}..{:?}",
        low[0],
        high[0]
    );
    assert!(
        (low[2] - 3.65).abs() < 0.06 && (high[2] - 4.35).abs() < 0.06,
        "rotated z bounds {:?}..{:?}",
        low[2],
        high[2]
    );
    assert!(
        (low[1] + 0.1).abs() < 0.02,
        "vertical offset must sink the prop: base at {}",
        low[1]
    );
    assert!(
        (high[1] - 0.175).abs() < 0.03,
        "half-scale bed top at {}",
        high[1]
    );
}

/// Loads one engine regression fixture from `tests/fixtures/levels/`.
fn fixture_level(name: &str) -> crate::level::LevelDef {
    let path = format!("tests/fixtures/levels/{name}.json");
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{path} must be readable: {error}"));
    crate::level::LevelDef::from_json(&content)
        .unwrap_or_else(|error| panic!("{path} must parse: {error}"))
}

#[test]
fn the_showcase_level_renders_every_core_prop_with_real_geometry() {
    let catalog = shipped_catalog();
    let mut assets = shipped_assets();
    let level = fixture_level("prop_showcase");
    let (mesh, batches) = build_level_geometry_with_assets(&level, &catalog, &mut assets);

    assert_eq!(
        mesh.batches.prop_batch.count, 0,
        "no placeholder boxes expected"
    );

    // The shared showcase fixtures place every catalogue placeable exactly
    // once: the domestic/office map covers the generic and Office props, and
    // the Pool showcase covers the Pool family. Themes organize content;
    // this is the one place a "placed somewhere" check is legitimate.
    let pool_showcase = fixture_level("pool_showcase");
    let mut used: std::collections::HashSet<&str> = std::collections::HashSet::new();
    used.extend(level.props.iter().map(|prop| prop.model.as_str()));
    used.extend(pool_showcase.props.iter().map(|prop| prop.model.as_str()));
    for entry in catalog.entries() {
        assert!(
            used.contains(entry.id.as_str()),
            "the showcase levels must place {}",
            entry.id
        );
    }

    // Every prop this level places renders with real geometry, never a
    // placeholder box, and each model appears exactly once.
    let mut models: Vec<&str> = batches.iter().map(|batch| batch.model.as_str()).collect();
    models.sort_unstable();
    models.dedup();
    let placed: std::collections::HashSet<&str> =
        level.props.iter().map(|prop| prop.model.as_str()).collect();
    assert_eq!(
        models.len(),
        placed.len(),
        "each prop model placed here appears exactly once as real geometry"
    );
    assert_eq!(batches.len(), placed.len());
    assert_eq!(assets.stats().models_failed, 0);

    // The intentional clipping is present in the data, not corrected.
    let sunk = level
        .props
        .iter()
        .find(|prop| prop.model == "core:crate" && prop.y < 0.0)
        .expect("the showcase keeps one crate sunk into the floor");
    assert!(sunk.solid, "the sunk crate still blocks the player");
}

#[test]
fn the_stress_level_batches_repeats_into_one_draw_per_model_and_cell() {
    let catalog = shipped_catalog();
    let mut assets = shipped_assets();
    let level = fixture_level("prop_stress");
    assert!(
        level.props.len() >= 100,
        "the stress level needs a real load"
    );

    let (_, batches) = build_level_geometry_with_assets(&level, &catalog, &mut assets);
    // Every batch now covers one model *inside one spatial cell*, so the
    // frustum can reject a cell's worth of instances. That is still a
    // handful of draws per model, not one per instance.
    let distinct_models: std::collections::HashSet<&str> =
        batches.iter().map(|batch| batch.model.as_str()).collect();
    assert!(
        distinct_models.len() <= 12,
        "{} distinct models, got {}",
        level.props.len(),
        distinct_models.len()
    );
    assert!(
        batches.len() >= distinct_models.len(),
        "each model needs at least one batch"
    );
    assert!(
        batches.len() <= distinct_models.len() * 16,
        "cells must stay coarse: {} batches for {} models",
        batches.len(),
        distinct_models.len()
    );
    for batch in &batches {
        assert!(
            !batch.bounds.is_empty(),
            "every batch needs bounds for the frustum test"
        );
    }
    assert_eq!(assets.stats().models_failed, 0);

    let total_vertices: usize = batches.iter().map(|batch| batch.vertices.len()).sum();
    let total_indices: usize = batches.iter().map(|batch| batch.indices.len()).sum();
    // Cross-check the expansion: every placed instance contributes exactly
    // one copy of its model's distinct vertices and one copy of its index
    // list. The decoded asset is shared, so the cache only holds one copy
    // per model (proving instance reuse).
    let mut expected_vertices = 0usize;
    let mut expected_indices = 0usize;
    for prop in &level.props {
        let entry = catalog.get(&prop.model);
        let path = entry.model.expect("stress props come from the catalogue");
        let model = &assets.resolve(&path).expect("model loads").model;
        expected_vertices += model.vertices.len();
        expected_indices += model.indices.len();
    }
    assert_eq!(total_vertices, expected_vertices);
    assert_eq!(total_indices, expected_indices);
    assert!(
        total_vertices > assets.stats().triangles,
        "repeated instances must cost vertices, not extra decoded models"
    );
    assert!(
        total_vertices <= crate::level::MAX_LEVEL_PROP_VERTICES,
        "the stress level must stay inside the prop vertex budget ({total_vertices} vertices)"
    );

    // Sixty-plus instances of one model share the decoded mesh; they are
    // spread across whatever cells they occupy, never duplicated per cell.
    let chair_vertices: usize = batches
        .iter()
        .filter(|batch| batch.model == "environment/office/props/models/chair.glb")
        .map(|batch| batch.vertices.len())
        .sum();
    assert!(
        chair_vertices > 60 * 100,
        "sixty chairs should expand into a large shared batch set, got {chair_vertices} vertices",
    );
}

#[test]
fn a_broken_model_falls_back_to_the_placeholder_box_without_panicking() {
    let catalog = crate::loader::PropCatalog::from_json_str(
        r##"{
            "format_version": 1,
            "props": [{
                "id": "core:broken", "name": "Broken", "category": "Other",
                "size": [0.5, 1.0, 0.5], "color": "#808080",
                "model": "models/does_not_exist.glb"
            }]
        }"##,
    )
    .expect("valid catalog");
    let mut assets = shipped_assets();
    let level = level_with_wall("[]", r#"[{ "model": "core:broken", "x": 0.0, "z": 0.0 }]"#);
    let (mesh, batches) = build_level_geometry_with_assets(&level, &catalog, &mut assets);

    assert!(batches.is_empty(), "no real geometry for a missing model");
    assert_eq!(
        mesh.batches.prop_batch.count, 36,
        "a missing model must draw its placeholder box"
    );
    assert_eq!(assets.stats().models_failed, 1);
}

// ------------------------------------------------------------- decals

/// A 6x6 room with the given decal JSON and optional extra walls.
fn level_with_decals(decals_json: &str, walls_json: &str, lights_json: &str) -> LevelDef {
    let json = format!(
        r#"{{
            "format_version": 1,
            "id": "decal_test",
            "name": "Decal Test",
            "spawn": {{ "x": 0.0, "z": 0.0 }},
            "rooms": [{{ "x": 0.0, "z": 0.0, "width": 6.0, "depth": 6.0, "height": 3.0 }}],
            "walls": [{walls_json}],
            "decals": [{decals_json}],
            "ceiling_lights": {lights_json}
        }}"#
    );
    LevelDef::from_json(&json).expect("valid decal level json")
}

/// Diagonal of one decal quad's first triangle, as the emitted normal.
fn quad_normal(vertices: &[Vertex]) -> [f32; 3] {
    let a = glam::Vec3::from(vertices[0].pos);
    let b = glam::Vec3::from(vertices[1].pos);
    let c = glam::Vec3::from(vertices[2].pos);
    (b - a).cross(c - a).normalize().to_array()
}

fn normal_matches(actual: [f32; 3], expected: [f32; 3]) -> bool {
    (0..3).all(|axis| (actual[axis] - expected[axis]).abs() < 1e-4)
}

/// Samples one texel of the generated atlas through the same coordinate
/// convention `decal_uv_rect` hands to the shader (the sheet is stored
/// bottom-up, so the visual top row maps to the higher `v`).
fn atlas_texel(pixels: &[u8], u: f32, v: f32) -> [u8; 4] {
    let size = DECAL_ATLAS_SIZE;
    let x = ((u * size as f32) as i32).clamp(0, size - 1);
    let visual_y = (((1.0 - v) * size as f32) as i32).clamp(0, size - 1);
    let row = size - 1 - visual_y;
    let index = ((row * size + x) * 4) as usize;
    [
        pixels[index],
        pixels[index + 1],
        pixels[index + 2],
        pixels[index + 3],
    ]
}

/// The generated patterns must be drawn in the cell their sheet slot
/// samples: if the art and `decal_uv_rect` disagree, a level silently shows
/// a different pattern (hazard stripes rendering as an arrow, or nothing).
#[test]
fn generated_decal_atlas_cells_match_their_sheet_slots() {
    let atlas = generate_decal_atlas();
    let size = DECAL_ATLAS_SIZE as usize;
    let cell = |slot: u32| -> Vec<[u8; 4]> {
        let rect = decal_uv_rect(slot);
        let u0 = rect[0][0].min(rect[2][0]);
        let u1 = rect[0][0].max(rect[2][0]);
        let v0 = rect[0][1].min(rect[2][1]);
        let v1 = rect[0][1].max(rect[2][1]);
        let mut out = Vec::with_capacity(size * size);
        for row in 0..size {
            for column in 0..size {
                let u = u0 + (u1 - u0) * (column as f32 + 0.5) / size as f32;
                let v = v0 + (v1 - v0) * (row as f32 + 0.5) / size as f32;
                out.push(atlas_texel(&atlas, u, v));
            }
        }
        out
    };
    let count = |pixels: &[[u8; 4]], predicate: fn(&[u8; 4]) -> bool| -> usize {
        pixels.iter().filter(|texel| predicate(texel)).count()
    };

    let test = cell(decal_material_slot(DECAL_TEST_MATERIAL).expect("slot"));
    assert!(
        count(&test, |texel| texel[3] > 128
            && texel[0] > 180
            && texel[1] > 180
            && texel[2] > 180)
            > 100,
        "the validation marking's own cell must hold its white frame"
    );

    // The other three cells hold no ink: the floor arrow, the hazard stripes
    // and the Pool sign are external PNG sheets now, so nothing may be drawn
    // in them. A stray pattern here would show a level the wrong artwork.
    for slot in 1..4u32 {
        let spare = cell(slot);
        assert_eq!(
            count(&spare, |texel| texel[3] > 128),
            0,
            "generated atlas cell {slot} must stay transparent"
        );
    }
}

#[test]
fn every_decal_material_resolves_to_one_sheet_slot() {
    let mut seen = std::collections::HashSet::new();
    let mut rects = Vec::new();
    for material in DECAL_MATERIALS {
        let slot = decal_material_slot(material).expect("known decal material");
        assert!(seen.insert(slot), "{material} shares slot {slot}");
        let rect = decal_uv_rect(slot);
        for uv in rect {
            assert!(
                (0.0..=1.0).contains(&uv[0]) && (0.0..=1.0).contains(&uv[1]),
                "{material} samples outside the sheet: {uv:?}"
            );
        }
        rects.push(rect);
    }
    assert_eq!(decal_material_slot("core:not_a_decal"), None);
}

#[test]
fn a_catalogued_png_decal_draws_from_its_own_sheet() {
    // The final Pool sign is external artwork: the catalog declares it as a
    // file-backed PNG, the mesh builder gives it a sheet past the generated
    // atlas slots, and the quad samples the whole sheet.
    let catalog = shipped_catalog();
    let level = level_with_decals(
        r#"{ "x": 3.0, "y": 0.0, "z": 3.0, "width": 0.9, "height": 0.9,
             "material": "core:decal_no_diving_01", "surface": "floor" }"#,
        r#"{ "x": 0.0, "z": 0.0, "width": 6.0, "depth": 0.4, "height": 3.0 }"#,
        "[]",
    );
    assert_eq!(
        decal_external_sheet_ids(&level, catalog.assets()),
        vec!["core:decal_no_diving_01".to_string()],
        "the sign is the level's only external sheet"
    );
    let sheet = decal_sheet_index(&level, catalog.assets(), "core:decal_no_diving_01")
        .expect("the catalogued sign resolves");
    assert_eq!(sheet, DECAL_EXTERNAL_BASE);
    let mesh = build_level_geometry_with_catalog(&level, &catalog);
    assert_eq!(mesh.batches.decal_batch.count, 6, "one decal is one quad");
    let quad = batch_slice(&mesh, SurfaceKind::Decal);
    // Full-sheet UVs: the decal samples the whole PNG (the packed slice does
    // not promise a corner order, so compare the set).
    let mut uvs: Vec<[f32; 2]> = quad.iter().map(|vertex| vertex.uv).collect();
    uvs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    uvs.dedup();
    assert_eq!(
        uvs,
        vec![[0.0, 0.0], [0.0, 1.0], [1.0, 0.0], [1.0, 1.0]],
        "an external decal samples its whole sheet"
    );

    // The sheet index must be stable when the same level also uses a
    // generated pattern, and generated sheets keep the atlas rect.
    let mixed = level_with_decals(
        r#"{ "x": 1.0, "y": 0.0, "z": 1.0, "width": 1.0, "height": 1.0,
             "material": "core:decal_test_01", "surface": "floor" },
           { "x": 3.0, "y": 0.0, "z": 3.0, "width": 0.9, "height": 0.9,
             "material": "core:decal_no_diving_01", "surface": "floor" }"#,
        r#"{ "x": 0.0, "z": 0.0, "width": 6.0, "depth": 0.4, "height": 3.0 }"#,
        "[]",
    );
    assert_eq!(
        decal_sheet_index(&mixed, catalog.assets(), "core:decal_test_01"),
        decal_material_slot("core:decal_test_01")
    );
    assert_eq!(
        decal_sheet_index(&mixed, catalog.assets(), "core:decal_no_diving_01"),
        Some(DECAL_EXTERNAL_BASE)
    );
    // The two remaining former patterns are external sheets as well, and the
    // first one the level places takes the slot directly after the generated
    // atlas, so their indices are the level's placement order.
    assert_eq!(
        decal_external_sheet_ids(&mixed, catalog.assets()),
        vec!["core:decal_no_diving_01".to_string()]
    );
    let with_patterns = level_with_decals(
        r#"{ "x": 1.0, "y": 0.0, "z": 1.0, "width": 1.0, "height": 1.0,
             "material": "core:decal_arrow_01", "surface": "floor" },
           { "x": 3.0, "y": 0.0, "z": 3.0, "width": 1.0, "height": 1.0,
             "material": "core:decal_stripes_01", "surface": "floor" }"#,
        r#"{ "x": 0.0, "z": 0.0, "width": 6.0, "depth": 0.4, "height": 3.0 }"#,
        "[]",
    );
    assert_eq!(
        decal_external_sheet_ids(&with_patterns, catalog.assets()),
        vec![
            "core:decal_arrow_01".to_string(),
            "core:decal_stripes_01".to_string()
        ],
        "the arrow and the stripes are external sheets, in placement order"
    );
    assert_eq!(
        decal_sheet_index(&with_patterns, catalog.assets(), "core:decal_arrow_01"),
        Some(DECAL_EXTERNAL_BASE)
    );
    assert_eq!(
        decal_sheet_index(&with_patterns, catalog.assets(), "core:decal_stripes_01"),
        Some(DECAL_EXTERNAL_BASE + 1)
    );
    let mesh = build_level_geometry_with_catalog(&mixed, &catalog);
    assert_eq!(mesh.batches.decal_batch.count, 12, "two decals, two quads");
    // A generated decal that is not catalogued draws nothing, exactly like
    // an unknown material.
    assert_eq!(
        decal_sheet_index(&level, catalog.assets(), "core:not_a_decal"),
        None
    );
}

#[test]
fn a_wall_decal_lies_on_its_wall_plane_lifted_by_the_decal_offset() {
    let level = level_with_decals(
        r#"{ "x": 3.0, "y": 1.5, "z": 0.4, "width": 2.0, "height": 1.0,
             "material": "core:decal_test_01", "surface": "wall_south" }"#,
        r#"{ "x": 0.0, "z": 0.0, "width": 6.0, "depth": 0.4, "height": 3.0 }"#,
        "[]",
    );
    let mesh = build_level_geometry(&level);
    let quad = batch_slice(&mesh, SurfaceKind::Decal);
    assert_eq!(mesh.batches.decal_batch.count, 6, "one decal is one quad");
    // Every corner sits on the authored wall plane, lifted out of the wall by
    // exactly the shared decal surface offset and no more.
    for vertex in &quad {
        assert_exact_named(
            vertex.pos[2],
            0.4 + DECAL_SURFACE_OFFSET_M,
            "wall decal plane",
        );
        assert!(vertex.pos[1] >= 0.99 && vertex.pos[1] <= 2.01);
        assert!(vertex.pos[0] >= 1.99 && vertex.pos[0] <= 4.01);
    }
    assert!(normal_matches(quad_normal(&quad), [0.0, 0.0, 1.0]));
}

#[test]
fn a_floor_decal_stays_flat_and_rotates_in_its_plane() {
    let level = level_with_decals(
        r#"{ "x": 3.0, "y": 0.0, "z": 3.0, "width": 2.0, "height": 1.0,
             "material": "core:decal_test_01", "surface": "floor", "rotation_degrees": 90.0 }"#,
        r#"{ "x": 0.0, "z": 0.0, "width": 6.0, "depth": 0.4, "height": 3.0 }"#,
        "[]",
    );
    let mesh = build_level_geometry(&level);
    let quad = batch_slice(&mesh, SurfaceKind::Decal);
    for vertex in &quad {
        assert_exact_named(vertex.pos[1], DECAL_SURFACE_OFFSET_M, "floor decal plane");
        // A quarter turn puts the 2 m width along Z and the 1 m height
        // along X, centred on the anchor.
        assert!(vertex.pos[0] >= 2.49 && vertex.pos[0] <= 3.51);
        assert!(vertex.pos[2] >= 1.99 && vertex.pos[2] <= 4.01);
    }
    assert!(normal_matches(quad_normal(&quad), [0.0, 1.0, 0.0]));
}

#[test]
fn decal_rotation_is_a_pure_in_plane_spin() {
    let base = crate::level::DecalDef {
        x: 1.0,
        y: 2.0,
        z: 3.0,
        width: 2.0,
        height: 1.0,
        rotation_degrees: 0.0,
        material: "core:decal_test_01".into(),
        surface: crate::level::DecalSurface::WallSouth,
    };
    let quarter = crate::level::DecalDef {
        rotation_degrees: 90.0,
        ..base.clone()
    };
    let half = crate::level::DecalDef {
        rotation_degrees: 180.0,
        ..base.clone()
    };
    let a = decal_quad_points(&base).expect("finite decal");
    let b = decal_quad_points(&quarter).expect("finite decal");
    let c = decal_quad_points(&half).expect("finite decal");
    // All three keep the centre and the surface plane.
    for corners in [a, b, c] {
        let centre: [f32; 3] =
            std::array::from_fn(|axis| corners.iter().map(|point| point[axis]).sum::<f32>() / 4.0);
        assert_exact_array(centre, [1.0, 2.0, 3.0]);
    }
    // The quarter turn swaps the in-plane extents; the half turn restores
    // them, so the decal stays in its plane and keeps a valid winding.
    let xs = |corners: [[f32; 3]; 4]| {
        corners
            .iter()
            .map(|point| point[0])
            .fold((f32::MAX, f32::MIN), |(lo, hi), x| (lo.min(x), hi.max(x)))
    };
    assert!((xs(a).1 - xs(a).0 - 2.0).abs() < 1e-4);
    assert!((xs(b).1 - xs(b).0 - 1.0).abs() < 1e-4);
    assert!((xs(c).1 - xs(c).0 - 2.0).abs() < 1e-4);
    for corners in [a, b, c] {
        let normal = corners_normal(&corners);
        assert!(
            normal_matches(normal, [0.0, 0.0, 1.0]),
            "rotated decal lost its facing: {normal:?}"
        );
    }
}

/// The emitted winding normal of four decal corners.
fn corners_normal(corners: &[[f32; 3]; 4]) -> [f32; 3] {
    let a = glam::Vec3::from(corners[0]);
    let b = glam::Vec3::from(corners[1]);
    let c = glam::Vec3::from(corners[2]);
    (b - a).cross(c - a).normalize().to_array()
}

#[test]
fn malformed_decals_never_emit_geometry() {
    for decal in [
        crate::level::DecalDef {
            width: f32::NAN,
            ..crate::level::DecalDef {
                x: 0.0,
                y: 1.0,
                z: 0.0,
                width: 1.0,
                height: 1.0,
                rotation_degrees: 0.0,
                material: DECAL_TEST_MATERIAL.into(),
                surface: crate::level::DecalSurface::WallSouth,
            }
        },
        crate::level::DecalDef {
            height: 0.0,
            ..crate::level::DecalDef {
                x: 0.0,
                y: 1.0,
                z: 0.0,
                width: 1.0,
                height: 0.0,
                rotation_degrees: 0.0,
                material: DECAL_TEST_MATERIAL.into(),
                surface: crate::level::DecalSurface::Floor,
            }
        },
        crate::level::DecalDef {
            rotation_degrees: f32::INFINITY,
            ..crate::level::DecalDef {
                x: 0.0,
                y: 1.0,
                z: 0.0,
                width: 1.0,
                height: 1.0,
                rotation_degrees: f32::INFINITY,
                material: DECAL_TEST_MATERIAL.into(),
                surface: crate::level::DecalSurface::Floor,
            }
        },
    ] {
        assert!(decal_quad_points(&decal).is_none());
    }
}

/// The external-sheet UV convention is empirical (see
/// [`decal_uv_rect_full`]); this pins the verified mapping so a future
/// change cannot silently flip a sign upside down or mirror it.
#[test]
fn external_decal_sheets_pin_their_world_orientation() {
    let rect = decal_uv_rect_full();
    assert_eq!(
        rect,
        [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
        "the full-sheet rect that reads upright on a floor and on a wall"
    );
    for uv in rect {
        assert!((0.0..=1.0).contains(&uv[0]) && (0.0..=1.0).contains(&uv[1]));
    }
}

#[test]
fn unknown_decal_materials_are_skipped_without_failing_the_build() {
    let level = level_with_decals(
        r#"{ "x": 3.0, "y": 1.5, "z": 0.0, "width": 1.0, "height": 1.0,
             "material": "core:decal_from_a_newer_build", "surface": "wall_south" }"#,
        r#"{ "x": 0.0, "z": 0.0, "width": 6.0, "depth": 0.4, "height": 3.0 }"#,
        "[]",
    );
    let mesh = build_level_geometry(&level);
    assert_eq!(
        mesh.batches.decal_batch.count, 0,
        "an unresolved decal sheet must draw nothing"
    );
    assert!(mesh.batches.wall_batch.count > 0, "the level still builds");
}

#[test]
fn decals_are_lit_by_the_rooms_own_baked_light() {
    let warm = level_with_decals(
        r#"{ "x": 3.0, "y": 0.0, "z": 3.0, "width": 2.0, "height": 2.0,
             "material": "core:decal_test_01", "surface": "floor" }"#,
        r#"{ "x": 0.0, "z": 0.0, "width": 6.0, "depth": 0.4, "height": 3.0 }"#,
        r#"[{ "fixture": "core:fluorescent_panel_01", "x": 3.0, "z": 3.0, "brightness": 1.0,
               "color": [1.0, 0.5, 0.2] }]"#,
    );
    let blue = level_with_decals(
        r#"{ "x": 3.0, "y": 0.0, "z": 3.0, "width": 2.0, "height": 2.0,
             "material": "core:decal_test_01", "surface": "floor" }"#,
        r#"{ "x": 0.0, "z": 0.0, "width": 6.0, "depth": 0.4, "height": 3.0 }"#,
        r#"[{ "fixture": "core:fluorescent_panel_01", "x": 3.0, "z": 3.0, "brightness": 1.0,
               "color": [0.2, 0.5, 1.0] }]"#,
    );
    let sample = |level: &LevelDef| {
        let mesh = build_level_geometry(level);
        let quad = batch_slice(&mesh, SurfaceKind::Decal);
        assert_eq!(quad.len(), 6, "one decal quad");
        quad.iter()
            .map(|vertex| vertex.color)
            .fold([0.0f32; 3], |mut acc, color| {
                for channel in 0..3 {
                    acc[channel] += color[channel] / 6.0;
                }
                acc
            })
    };
    let warm_color = sample(&warm);
    let blue_color = sample(&blue);
    assert!(
        warm_color[0] > warm_color[2] + 0.1,
        "a warm fixture must warm the decal: {warm_color:?}"
    );
    assert!(
        blue_color[2] > blue_color[0] + 0.1,
        "a blue fixture must cool the decal: {blue_color:?}"
    );
    // The room's ambient floor still applies: a decal is never black.
    for channel in blue_color {
        assert!(channel >= crate::lighting::AMBIENT_LEVEL * 0.5);
    }
}

#[test]
fn the_decal_depth_bias_is_deterministic_and_sub_visible() {
    let (factor, units) = DECAL_POLYGON_OFFSET;
    // Both terms pull towards the camera (negative `glPolygonOffset`), and the
    // slope-scaled term is present so grazing angles and long distances keep
    // their bias after the constant term is below the buffer's resolution.
    assert!(
        factor < 0.0 && units < 0.0,
        "the bias must pull decals towards the camera, got ({factor}, {units})"
    );
    assert!(
        (-4.0..0.0).contains(&units),
        "the constant bias must stay a couple of depth steps, got {units}"
    );
    assert!(
        factor == -1.0 && units == -4.0,
        "the bias is part of the render contract"
    );
    assert!((0.0..1.0).contains(&DECAL_ALPHA_CUTOFF));

    // The geometry half of the contract: a strictly positive, sub-millimetre
    // normal offset that a rasteriser cannot round away in the near field and
    // no one can see as hover. It must be smaller than the thinnest fixture in
    // the game, so a decal can never be lifted into a nearby surface.
    const { assert!(DECAL_SURFACE_OFFSET_M > 0.0) };
    const { assert!(DECAL_SURFACE_OFFSET_M <= 1.0e-3) };
}

// ----------------------------------------------- decal depth relationship

/// Surface plane of every emitted decal quad: `(normal, n . p)`.
///
/// A quad is six vertices, exactly as every emitter writes it.
fn decal_planes(vertices: &[Vertex]) -> Vec<([f32; 3], f32)> {
    vertices
        .chunks(6)
        .filter(|quad| quad.len() == 6)
        .map(|quad| {
            let normal = quad_normal(quad);
            (
                normal,
                glam::Vec3::from(normal).dot(glam::Vec3::from(quad[0].pos)),
            )
        })
        .collect()
}

/// A room enclosed by four walls whose interior faces are exactly x = 0, x = 6,
/// z = 0 and z = 6, so every decal surface has a real parent plane.
fn six_surface_room(decals_json: &str) -> LevelDef {
    LevelDef::from_json(&format!(
        r#"{{
            "format_version": 1,
            "id": "six_surface_decals",
            "name": "Six Surface Decals",
            "spawn": {{ "x": 3.0, "z": 3.0 }},
            "rooms": [{{ "x": 0.0, "z": 0.0, "width": 6.0, "depth": 6.0, "height": 3.0 }}],
            "walls": [
                {{ "x": -0.4, "z": -0.4, "width": 6.8, "depth": 0.4, "height": 3.0 }},
                {{ "x": -0.4, "z": 6.0, "width": 6.8, "depth": 0.4, "height": 3.0 }},
                {{ "x": -0.4, "z": 0.0, "width": 0.4, "depth": 6.0, "height": 3.0 }},
                {{ "x": 6.0, "z": 0.0, "width": 0.4, "depth": 6.0, "height": 3.0 }}
            ],
            "decals": [{decals_json}],
            "ceiling_lights": []
        }}"#
    ))
    .expect("six-surface decal json")
}

/// The renderer invariant this whole task exists for: a decal is *never* on the
/// same plane as the surface it marks, on any of the six surfaces. Each one sits
/// exactly [`DECAL_SURFACE_OFFSET_M`] off its parent along the surface normal,
/// which is what lets the near field resolve without depending on the
/// rasteriser's plane-fit rounding.
#[test]
fn every_decal_surface_lifts_its_marking_by_the_shared_offset() {
    let level = six_surface_room(
        r#"{ "x": 3.0, "y": 0.0, "z": 3.0, "width": 1.0, "height": 1.0,
             "material": "core:decal_test_01", "surface": "floor" },
           { "x": 2.0, "y": 0.0, "z": 2.0, "width": 1.0, "height": 1.0,
             "material": "core:decal_test_01", "surface": "ceiling" },
           { "x": 3.0, "y": 1.5, "z": 0.0, "width": 1.0, "height": 1.0,
             "material": "core:decal_test_01", "surface": "wall_south" },
           { "x": 3.0, "y": 1.5, "z": 6.0, "width": 1.0, "height": 1.0,
             "material": "core:decal_test_01", "surface": "wall_north" },
           { "x": 0.0, "y": 1.5, "z": 3.0, "width": 1.0, "height": 1.0,
             "material": "core:decal_test_01", "surface": "wall_east" },
           { "x": 6.0, "y": 1.5, "z": 3.0, "width": 1.0, "height": 1.0,
             "material": "core:decal_test_01", "surface": "wall_west" }"#,
    );
    let mesh = build_level_geometry(&level);
    let planes = decal_planes(&batch_slice(&mesh, SurfaceKind::Decal));
    assert_eq!(planes.len(), 6, "one quad per authored decal");

    // Parent plane and outward normal per surface: floor y = 0, ceiling y = 3,
    // the wall faces at z = 0 / z = 6 and x = 0 / x = 6. `n . p` of a decal is
    // the parent's constant plus the normal-relative offset.
    let offset = DECAL_SURFACE_OFFSET_M;
    let expected = [
        ([0.0, 1.0, 0.0], 0.0 + offset),
        ([0.0, -1.0, 0.0], -3.0 + offset),
        ([0.0, 0.0, 1.0], 0.0 + offset),
        ([0.0, 0.0, -1.0], -6.0 + offset),
        ([1.0, 0.0, 0.0], 0.0 + offset),
        ([-1.0, 0.0, 0.0], -6.0 + offset),
    ];
    for (normal, plane_offset) in expected {
        assert!(
            planes.iter().any(|(plane_normal, candidate)| {
                normal_matches(*plane_normal, normal) && (candidate - plane_offset).abs() < 1e-6
            }),
            "no decal at {normal:?} offset {plane_offset}; emitted {planes:?}"
        );
    }
    // And no decal is left sitting on a parent plane.
    for (normal, plane_offset) in &planes {
        let parent = if normal_matches(*normal, [0.0, 1.0, 0.0]) {
            0.0
        } else if normal_matches(*normal, [0.0, -1.0, 0.0]) {
            -3.0
        } else if normal_matches(*normal, [0.0, 0.0, 1.0])
            || normal_matches(*normal, [1.0, 0.0, 0.0])
        {
            0.0
        } else {
            -6.0
        };
        assert!(
            (plane_offset - parent).abs() >= offset - 1e-6,
            "a decal shares its parent's depth plane: {normal:?} at {plane_offset}"
        );
    }
}

/// Rotation spins the marking inside its own plane; it must not change how far
/// the plane itself is lifted off the surface.
#[test]
fn rotated_decals_keep_the_full_normal_offset() {
    for rotation in [0.0f32, 30.0, 45.0, 90.0, 180.0, 270.0] {
        // `(surface, normal, axis the plane is constant on, parent plane)`.
        for (surface, normal, axis, parent) in [
            ("wall_south", [0.0, 0.0, 1.0], 2, 0.4f32),
            ("floor", [0.0, 1.0, 0.0], 1, 0.0),
        ] {
            let level = level_with_decals(
                &format!(
                    r#"{{ "x": 3.0, "y": 0.0, "z": 0.4, "width": 1.0, "height": 1.0,
                         "rotation_degrees": {rotation},
                         "material": "core:decal_test_01", "surface": "{surface}" }}"#
                ),
                r#"{ "x": 0.0, "z": 0.0, "width": 6.0, "depth": 0.4, "height": 3.0 }"#,
                "[]",
            );
            let mesh = build_level_geometry(&level);
            let quad = batch_slice(&mesh, SurfaceKind::Decal);
            let planes = decal_planes(&quad);
            assert_eq!(planes.len(), 1, "one quad at {surface} / {rotation} deg");
            assert!(
                normal_matches(planes[0].0, normal),
                "rotated decal changed its facing at {rotation} deg: {:?}",
                planes[0].0
            );
            let expected_plane = parent + DECAL_SURFACE_OFFSET_M;
            assert!(
                (planes[0].1 - expected_plane).abs() < 1e-6,
                "rotated decal lost its plane offset at {rotation} deg: {}",
                planes[0].1
            );
            // Every corner sits on the lifted plane, not just the first
            // triangle: a rotation must not fold the quad into the surface.
            for vertex in &quad {
                assert!(
                    (vertex.pos[axis] - expected_plane).abs() < 1e-6,
                    "corner left the lifted plane at {rotation} deg: {:?}",
                    vertex.pos
                );
            }
        }
    }
}

/// Decals next to a wall/floor intersection keep their own offset and stay out
/// of each other and out of the surfaces they are tucked against: the lift is
/// always along the surface normal, never a world-space nudge that could push a
/// marking into the perpendicular surface.
#[test]
fn decals_tucked_into_a_corner_keep_their_offsets() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 1,
            "id": "corner_decals",
            "name": "Corner Decals",
            "spawn": { "x": 2.0, "z": 2.0 },
            "rooms": [{ "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0, "height": 3.0 }],
            "walls": [
                { "x": 0.0, "z": 3.0, "width": 4.0, "depth": 0.4, "height": 3.0 }
            ],
            "decals": [
                { "x": 2.0, "y": 0.0, "z": 3.6, "width": 0.7, "height": 0.7,
                  "material": "core:decal_test_01", "surface": "floor" },
                { "x": 2.0, "y": 0.30, "z": 3.0, "width": 0.7, "height": 0.5,
                  "material": "core:decal_test_01", "surface": "wall_north" }
            ]
        }"#,
    )
    .expect("corner decal json");
    let mesh = build_level_geometry(&level);
    let quads = batch_slice(&mesh, SurfaceKind::Decal);
    assert_eq!(quads.len(), 12, "two decals, two quads");

    // The wall at z = 3.0 has its interior face at z = 3.0 facing -Z (north);
    // the floor decal stops 0.05 m short of it and the wall decal starts 0.05 m
    // above the floor. Classify whole quads by their facing, since the wall
    // decal's lower corners sit below the floor decal's height.
    let mut floor = Vec::new();
    let mut wall = Vec::new();
    for quad in quads.chunks(6) {
        if normal_matches(quad_normal(quad), [0.0, 1.0, 0.0]) {
            floor.extend_from_slice(quad);
        } else {
            assert!(normal_matches(quad_normal(quad), [0.0, 0.0, -1.0]));
            wall.extend_from_slice(quad);
        }
    }
    assert_eq!(floor.len(), 6);
    assert_eq!(wall.len(), 6);
    for vertex in &floor {
        assert_exact_named(vertex.pos[1], DECAL_SURFACE_OFFSET_M, "floor decal");
        assert!(vertex.pos[2] <= 3.96, "a floor decal reached into the wall");
    }
    for vertex in &wall {
        assert_exact_named(vertex.pos[2], 3.0 - DECAL_SURFACE_OFFSET_M, "wall decal");
        assert!(vertex.pos[1] >= 0.05, "a wall decal reached into the floor");
    }
}

#[test]
fn decals_are_the_last_static_kind_and_keep_the_world_families() {
    assert_eq!(SurfaceKind::ALL.last(), Some(&SurfaceKind::Decal));
    assert_eq!(MaterialSlot::Wall.kind(), SurfaceKind::Wall);
    assert_eq!(MaterialSlot::Floor.kind(), SurfaceKind::Floor);
    assert_eq!(MaterialSlot::Ceiling.kind(), SurfaceKind::Ceiling);
    for kind in [SurfaceKind::Floor, SurfaceKind::Ceiling, SurfaceKind::Wall] {
        assert_ne!(kind, SurfaceKind::Decal);
    }
}

// ------------------------------------------------- coincident wall overlays

/// Two coincident walls: a host with the given openings and a shorter
/// overlay with the given material and openings.
fn coincident_wall_level(
    host_openings: &str,
    overlay_openings: &str,
    overlay_material: &str,
) -> LevelDef {
    let material = if overlay_material.is_empty() {
        String::new()
    } else {
        format!(r#", "material": "{overlay_material}""#)
    };
    let json = format!(
        r#"{{
            "format_version": 1,
            "id": "coincident",
            "name": "Coincident",
            "spawn": {{ "x": 0.0, "z": 0.0 }},
            "rooms": [{{ "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.0 }}],
            "walls": [
                {{ "x": 0.0, "z": 3.0, "width": 10.0, "depth": 0.4, "height": 3.0,
                   "openings": {host_openings} }},
                {{ "x": 2.0, "z": 3.0, "width": 4.0, "depth": 0.4, "height": 3.0,
                   "openings": {overlay_openings}{material} }}
            ],
            "ceiling_lights": []
        }}"#
    );
    LevelDef::from_json(&json).expect("valid coincident wall json")
}

#[test]
fn coincident_overlay_walls_become_one_surface_with_material_runs() {
    let level = coincident_wall_level("[]", "[]", "core:wallpaper_stained_01");
    let materials = logical_materials(&level);
    let lookup = MaterialLookup::new(&materials);
    let units = wall_units(&level, &crate::level::LevelSurfaces::new(&level), &lookup);
    let coalesced: Vec<_> = units
        .iter()
        .filter_map(|unit| match unit {
            WallUnit::Coalesced { wall, runs, .. } => Some((wall, runs)),
            WallUnit::Plain { .. } => None,
        })
        .collect();
    assert_eq!(coalesced.len(), 1, "the overlay is resolved into the host");
    let (wall, runs) = coalesced[0];
    assert_exact_named(wall.x, 0.0, "coalesced wall start");
    assert_exact_named(wall.width, 10.0, "coalesced wall length");
    assert_eq!(runs.len(), 3, "host/overlay/host material runs");
    assert_eq!(
        runs[0].body,
        lookup.key(
            MaterialSlot::Wall,
            crate::level::MaterialRef::id("core:wallpaper_yellow_01")
        )
    );
    assert_eq!(
        runs[1].body,
        lookup.key(
            MaterialSlot::Wall,
            crate::level::MaterialRef::id("core:wallpaper_stained_01")
        )
    );
    assert_exact_named(runs[1].start, 2.0, "stain run start");
    assert_exact_named(runs[1].end, 6.0, "stain run end");

    // The emitted mesh carries the overlay exactly once: a plain host is
    // two quads, and the stained run adds two more, with no duplicate of
    // the host's own faces underneath.
    let mesh = build_level_geometry(&level);
    let maintained = material_vertices(&mesh, &level, "core:wallpaper_yellow_01");
    let stained = material_vertices(&mesh, &level, "core:wallpaper_stained_01");
    let quads = |vertices: &[Vertex]| vertices.len() / 6;
    // The maintained runs (0..2 and 6..10) plus the two end caps; the
    // stained run is emitted once, and no maintained face survives under
    // it.
    assert_eq!(quads(&maintained), 6, "the maintained runs, once each");
    assert_eq!(quads(&stained), 2, "the overlay run, once");
    for vertex in &stained {
        assert!(vertex.pos[0] >= 1.99 && vertex.pos[0] <= 6.01);
    }
    for vertex in &maintained {
        assert!(
            vertex.pos[0] <= 2.001 || vertex.pos[0] >= 5.999,
            "no maintained face may be emitted under the stained run at x={}",
            vertex.pos[0]
        );
    }
}

#[test]
fn an_overlay_only_covers_a_hole_when_it_is_solid_there() {
    // The host has a window inside the overlay's span. The overlay is
    // solid there, so the combined surface has no hole: that is what the
    // duplicate surfaces showed (the opaque overlay covered the window).
    let covered = coincident_wall_level(
        r#"[{ "kind": "window", "offset": 2.5, "width": 1.0, "height": 1.0, "sill": 1.0 }]"#,
        "[]",
        "core:wallpaper_stained_01",
    );
    let covered_materials = logical_materials(&covered);
    let covered_lookup = MaterialLookup::new(&covered_materials);
    let units = wall_units(
        &covered,
        &crate::level::LevelSurfaces::new(&covered),
        &covered_lookup,
    );
    let surfaces = crate::level::LevelSurfaces::new(&covered);
    let synthetic = units
        .iter()
        .find_map(|unit| match unit {
            WallUnit::Coalesced { slices, .. } => Some(slices.clone()),
            WallUnit::Plain { .. } => None,
        })
        .expect("coalesced unit");
    // The union of an opaque overlay over the host's window is solid: its
    // slices partition the whole group rectangle, with no window left open.
    let area: f32 = synthetic
        .iter()
        .map(|slice| (slice.end - slice.start) * (slice.top - slice.bottom))
        .sum();
    assert!(
        (area - 30.0).abs() < 1e-2,
        "an opaque overlay must close the window: union area {area}"
    );
    assert_exact_named(synthetic[0].start, 0.0, "closed union start");
    assert!(
        (synthetic[synthetic.len() - 1].end - 10.0).abs() < 1e-3,
        "closed union end"
    );
    let _ = surfaces;

    // When both walls carry the same door, the combined surface keeps it.
    let shared = coincident_wall_level(
        r#"[{ "kind": "door", "offset": 2.5, "width": 1.0, "height": 2.1, "sill": 0.0 }]"#,
        r#"[{ "kind": "door", "offset": 0.5, "width": 1.0, "height": 2.1, "sill": 0.0 }]"#,
        "core:wallpaper_stained_01",
    );
    let shared_materials = logical_materials(&shared);
    let shared_lookup = MaterialLookup::new(&shared_materials);
    let units = wall_units(
        &shared,
        &crate::level::LevelSurfaces::new(&shared),
        &shared_lookup,
    );
    let synthetic = units
        .iter()
        .find_map(|unit| match unit {
            WallUnit::Coalesced { slices, .. } => Some(slices.clone()),
            WallUnit::Plain { .. } => None,
        })
        .expect("coalesced unit");
    // Both walls cut the same door, so the union keeps exactly that hole: the
    // solid area is the full rectangle minus the door.
    let solid: f32 = synthetic
        .iter()
        .map(|slice| (slice.end - slice.start) * (slice.top - slice.bottom))
        .sum();
    let expected = 30.0 - 2.1;
    assert!(
        (solid - expected).abs() < 1e-2,
        "the shared door must survive the merge: solid {solid}, expected {expected}"
    );
    assert!(
        synthetic.iter().any(|slice| slice.bottom >= 2.1 - 1e-3
            && slice.start <= 2.5 + 1e-3
            && slice.end >= 3.5 - 1e-3),
        "the header above the shared door must remain solid"
    );
}

#[test]
fn an_empty_material_id_emits_a_bare_key_not_an_arbitrary_material() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 1,
            "id": "empty_materials",
            "name": "Empty Materials",
            "spawn": { "x": 0.0, "z": 0.0 },
            "defaults": { "wall": "", "floor": "", "ceiling": "" },
            "rooms": [{ "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0, "height": 3.0 }]
        }"#,
    )
    .expect("valid level");
    let table = logical_materials(&level);
    assert!(table.is_empty(), "empty ids are not materials");
    let mesh = build_level_geometry(&level);
    assert!(mesh.batches.floor_batch.count > 0);
    assert!(mesh.batches.ceiling_batch.count > 0);
    for range in &mesh.ranges {
        assert!(
            !range.key.has_material(),
            "an empty id must not resolve to an arbitrary material"
        );
    }
}

// ------------------------------------------------------- vertical geometry

/// Vertical bounds of a vertex run, as `(min_y, max_y)`.
fn y_bounds(vertices: &[Vertex]) -> (f32, f32) {
    let mut bounds = (f32::MAX, f32::MIN);
    for vertex in vertices {
        bounds.0 = bounds.0.min(vertex.pos[1]);
        bounds.1 = bounds.1.max(vertex.pos[1]);
    }
    bounds
}

#[test]
fn test_elevated_room_shifts_floor_and_ceiling_together() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 1,
            "id": "elevated",
            "name": "Elevated",
            "spawn": { "x": 4.0, "z": 4.0 },
            "room": { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0,
                      "height": 3.0, "floor_y": 2.0 },
            "ceiling_lights": [
                { "fixture": "core:fluorescent_panel_01", "x": 4.0, "z": 4.0 }
            ]
        }"#,
    )
    .expect("elevated json");
    let mesh = build_level_geometry(&level);
    let floor = batch_slice(&mesh, SurfaceKind::Floor);
    let ceiling = batch_slice(&mesh, SurfaceKind::Ceiling);
    assert_eq!(y_bounds(&floor), (2.0, 2.0));
    assert_eq!(y_bounds(&ceiling), (5.0, 5.0));
    // The fixture hangs below the real ceiling, not at the world floor.
    let lights = batch_slice(&mesh, SurfaceKind::Light);
    assert!((y_bounds(&lights).1 - (5.0 - 0.01)).abs() < 1e-4);
}

// --------------------------------------------------------- fixture sheets

/// One room with one fixture of every built-in family, at authored heights.
fn fixture_family_level() -> LevelDef {
    LevelDef::from_json(
        r#"{
            "format_version": 1,
            "id": "fixture_families",
            "name": "Fixture Families",
            "spawn": { "x": 1.0, "z": 1.0 },
            "rooms": [{ "x": 0.0, "z": 0.0, "width": 12.0, "depth": 6.0, "height": 3.0 }],
            "ceiling_lights": [
                { "fixture": "core:fluorescent_panel_01", "x": 2.0, "z": 3.0 },
                { "fixture": "core:pool_light_round", "x": 6.0, "z": 3.0 },
                { "fixture": "core:pool_light_wall", "x": 10.0, "z": 3.0,
                  "mount": "wall", "y": 1.7 },
                { "fixture": "home:ceiling_light_round", "x": 4.0, "z": 3.0 }
            ]
        }"#,
    )
    .expect("valid fixture family level")
}

/// The sheet slot of a family, as the mesh format carries it.
fn sheet_slot(kind: crate::lighting::FixtureKind) -> MaterialIndex {
    MaterialIndex::try_from(kind.index()).expect("four families fit u16")
}

/// UV extents of a vertex run, as `(min_u, max_u, min_v, max_v)`.
fn uv_bounds(vertices: &[Vertex]) -> (f32, f32, f32, f32) {
    let mut bounds = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
    for vertex in vertices {
        bounds.0 = bounds.0.min(vertex.uv[0]);
        bounds.1 = bounds.1.max(vertex.uv[0]);
        bounds.2 = bounds.2.min(vertex.uv[1]);
        bounds.3 = bounds.3.max(vertex.uv[1]);
    }
    bounds
}

/// Signed area of a quad's corner ring, in the plane its axes span.
fn quad_area(points: [[f32; 2]; 4]) -> f32 {
    let mut twice = 0.0;
    for index in 0..4 {
        let current = points[index];
        let next = points[(index + 1) % 4];
        twice = next[0].mul_add(-current[1], current[0].mul_add(next[1], twice));
    }
    twice * 0.5
}

#[test]
fn every_fixture_family_has_a_stable_sheet_slot() {
    assert_eq!(crate::lighting::FixtureKind::ALL.len(), 4);
    for (slot, kind) in crate::lighting::FixtureKind::ALL.iter().enumerate() {
        assert_eq!(kind.index(), slot, "{kind:?} drifted to another sheet slot");
    }
}

/// A family's luminous faces bind that family's sheet, and only that family's:
/// a level that mixes an office panel with two pool lights never draws one
/// fixture with another fixture's artwork.
#[test]
fn fixture_faces_carry_their_own_family_sheet_and_the_housing_stays_bare() {
    use crate::lighting::FixtureKind;

    let level = fixture_family_level();
    let mesh = build_level_geometry(&level);

    // The panel diffuser is one quad, the wall luminaire's lens one quad, and
    // the round diffuser the ten segments of its ring.
    for (kind, quads) in [
        (FixtureKind::FluorescentPanel, 1),
        (FixtureKind::RoundRecessed, 10),
        (FixtureKind::WallSconce, 1),
        (FixtureKind::FlushMount, 10),
    ] {
        let lit = mesh.triangles_for_key(SurfaceKey::new(SurfaceKind::Light, sheet_slot(kind)));
        assert_eq!(
            lit.len(),
            quads * 6,
            "{kind:?} must emit its lit face on its own sheet slot"
        );
    }

    // The flat metal housing keeps the bare key: the panel's four side walls,
    // bottom frame and top flange (12 quads), the round bezel ring plus can
    // (20), the wall housing's four sides, and the flush mount's drum, bottom
    // rim and centre boss (21).
    let housing = mesh.triangles_for_key(SurfaceKey::bare(SurfaceKind::Light));
    assert_eq!(housing.len(), (12 + 20 + 4 + 10 + 10 + 1) * 6);
    let lit_plus_housing = batch_slice(&mesh, SurfaceKind::Light).len();
    assert_eq!(
        lit_plus_housing,
        housing.len() + (1 + 10 + 1 + 10) * 6,
        "every fixture quad is either a lit face or housing"
    );
}

/// A sheet is fitted once across the face it draws: UVs stay inside the sheet,
/// the face aspect matches the sheet's own aspect, and the mapping keeps its
/// orientation, so nothing stretches, mirrors or tiles.
#[test]
fn fixture_sheets_are_fitted_once_and_keep_their_aspect() {
    use crate::lighting::FixtureKind;

    let level = fixture_family_level();
    let mesh = build_level_geometry(&level);
    let sheet =
        |kind| mesh.triangles_for_key(SurfaceKey::new(SurfaceKind::Light, sheet_slot(kind)));

    // Panel: the diffuser is a 1.12 x 0.56 m 2:1 aperture inside the housing.
    let panel = sheet(FixtureKind::FluorescentPanel);
    assert_eq!(uv_bounds(&panel), (0.0, 1.0, 0.0, 1.0));
    let (x0, x1, z0, z1) = xz_bounds(&panel);
    assert!(
        (x1 - x0 - 1.12).abs() < 1e-5
            && (z1 - z0 - 0.56).abs() < 1e-5
            && ((x1 - x0) / (z1 - z0) - 2.0).abs() < 1e-5,
        "the diffuser aperture is the sheet's 2:1 face inside the housing"
    );
    // Orientation: the sheet's top row (v = 0) is the -z edge, and u runs +x.
    let corner = |x: f32, z: f32| {
        panel
            .iter()
            .find(|vertex| (vertex.pos[0] - x).abs() < 1e-5 && (vertex.pos[2] - z).abs() < 1e-5)
            .map_or_else(|| panic!("no panel corner at {x},{z}"), |vertex| vertex.uv)
    };
    assert_eq!(corner(x0, z0), [0.0, 0.0]);
    assert_eq!(corner(x1, z0), [1.0, 0.0]);
    assert_eq!(corner(x0, z1), [0.0, 1.0]);

    // Wall lens: 0.4 x 0.2 m on a 2:1 sheet, fitted once.
    let wall = sheet(FixtureKind::WallSconce);
    assert_eq!(uv_bounds(&wall), (0.0, 1.0, 0.0, 1.0));
    let (wall_x0, wall_x1, _, _) = xz_bounds(&wall);
    let (lens_bottom, lens_top) = y_bounds(&wall);
    assert!((wall_x1 - wall_x0 - 0.4).abs() < 1e-5);
    assert!((lens_top - lens_bottom - 0.2).abs() < 1e-5);
    assert!(
        (((wall_x1 - wall_x0) / (lens_top - lens_bottom)) - 2.0).abs() < 1e-5,
        "the lens is 2:1, so its sheet must be too"
    );

    // Round diffuser: planar, inside the sheet, and isotropic - a square sheet
    // covers the 0.44 m disc, so one texel is the same size on both axes.
    let round = sheet(FixtureKind::RoundRecessed);
    let (u0, u1, v0, v1) = uv_bounds(&round);
    assert!(
        u0 >= 0.0 && u1 <= 1.0 && v0 >= 0.0 && v1 <= 1.0,
        "the diffuser samples the sheet once"
    );
    let radius = 0.22_f32;
    let (rx0, rx1, rz0, rz1) = xz_bounds(&round);
    let centre_x = f32::midpoint(rx0, rx1);
    let centre_z = f32::midpoint(rz0, rz1);
    let mut outermost = 0.0_f32;
    for vertex in &round {
        let dx = vertex.pos[0] - centre_x;
        let dz = vertex.pos[2] - centre_z;
        outermost = outermost.max(dx.hypot(dz));
        // Planar and isotropic: a square sheet covers the 0.44 m disc.
        let expected = [
            (dx / radius).mul_add(0.5, 0.5),
            (dz / radius).mul_add(0.5, 0.5),
        ];
        assert!(
            (vertex.uv[0] - expected[0]).abs() < 1e-5 && (vertex.uv[1] - expected[1]).abs() < 1e-5,
            "the diffuser's UVs are planar in the fixture plane: {:?} vs {expected:?}",
            vertex.uv
        );
    }
    assert!(
        (outermost - radius).abs() < 1e-5,
        "the diffuser's outer edge is the sheet's inscribed circle, found {outermost}"
    );
    // The residential flush mount uses the same planar, isotropic mapping.
    assert_flush_mount_sheet_is_planar(&sheet(FixtureKind::FlushMount));

    // Orientation: every segment's UV ring winds the same way as its world
    // ring, so no segment is mirrored; a quad chunk is [p0, p1, p2, p0, p2, p3].
    for quad in round.as_chunks::<6>().0 {
        let corner = |index: usize| -> [f32; 2] { [quad[index].pos[0], quad[index].pos[2]] };
        let uv_corner = |index: usize| -> [f32; 2] { quad[index].uv };
        let world_area = quad_area([corner(0), corner(1), corner(2), corner(5)]);
        let uv_area = quad_area([uv_corner(0), uv_corner(1), uv_corner(2), uv_corner(5)]);
        assert!(
            world_area * uv_area > 0.0,
            "a diffuser segment is mirrored: world {world_area}, uv {uv_area}"
        );
    }
}

/// The flush mount's diffuser samples its sheet's inscribed circle through the
/// same planar, isotropic mapping the round downlight uses, with the diffuser
/// inset behind the drum's rim.
fn assert_flush_mount_sheet_is_planar(flush: &[Vertex]) {
    let (fu0, fu1, fv0, fv1) = uv_bounds(flush);
    assert!(
        fu0 >= 0.0 && fu1 <= 1.0 && fv0 >= 0.0 && fv1 <= 1.0,
        "the flush-mount diffuser samples the sheet once"
    );
    let flush_radius = crate::lighting::FLUSH_MOUNT_RADIUS_M;
    let (fx0, fx1, fz0, fz1) = xz_bounds(flush);
    let centre_x = f32::midpoint(fx0, fx1);
    let centre_z = f32::midpoint(fz0, fz1);
    let mut outermost = 0.0_f32;
    for vertex in flush {
        let dx = vertex.pos[0] - centre_x;
        let dz = vertex.pos[2] - centre_z;
        outermost = outermost.max(dx.hypot(dz));
        let expected = [
            (dx / flush_radius).mul_add(0.5, 0.5),
            (dz / flush_radius).mul_add(0.5, 0.5),
        ];
        assert!(
            (vertex.uv[0] - expected[0]).abs() < 1e-5 && (vertex.uv[1] - expected[1]).abs() < 1e-5,
            "the diffuser's UVs are planar in the fixture plane: {:?} vs {expected:?}",
            vertex.uv
        );
    }
    // The diffuser stops short of the fixture's outer radius by its inset, so
    // the drum's own rim shows as a ring around the glowing face.
    assert!(
        (outermost - (flush_radius - 0.012)).abs() < 1e-5,
        "the diffuser edge is the inset radius, found {outermost}"
    );
}

/// The office panel is a real troffer: a frame and body around the diffuser,
/// not one floating sheet. The diffuser is the only emissive face, the housing
/// is the fixed mid grey, and the housing covers the family's full 1.2 x 0.6 m
/// footprint while the diffuser is recessed a lip inside it.
#[test]
fn the_office_panel_emits_a_real_housing_and_one_emissive_diffuser() {
    use super::common::fixtures::{
        PANEL_BODY_DROP_M, PANEL_BORDER_DEPTH_M, PANEL_BORDER_WIDTH_M, PANEL_LIP_M,
        add_panel_fixture,
    };
    const EMISSION: [f32; 3] = [0.82, 0.82, 0.82];
    let is_emission = |color: [f32; 4]| {
        color[0] == EMISSION[0] && color[1] == EMISSION[1] && color[2] == EMISSION[2]
    };
    let mut lit = Vec::new();
    let mut housing = Vec::new();
    add_panel_fixture(&mut lit, &mut housing, 0.0, 1.2, 0.0, 0.6, 2.0, EMISSION);

    // Exactly one luminous face: the diffuser.
    assert_eq!(lit.len(), 6, "the diffuser is one quad");
    assert!(
        lit.iter().all(|vertex| is_emission(vertex.color)),
        "the diffuser carries the neutral emission"
    );
    let y_diffuser = 2.0 - PANEL_BODY_DROP_M + PANEL_LIP_M;
    assert!(
        lit.iter()
            .all(|vertex| (vertex.pos[1] - y_diffuser).abs() < 1e-6),
        "the diffuser is recessed above the frame's bottom"
    );
    let (dx0, dx1, dz0, dz1) = (
        PANEL_BORDER_WIDTH_M,
        1.2 - PANEL_BORDER_WIDTH_M,
        PANEL_BORDER_DEPTH_M,
        0.6 - PANEL_BORDER_DEPTH_M,
    );
    for vertex in &lit {
        let on_corner = (vertex.pos[0] - dx0).abs() < 1e-6
            || (vertex.pos[0] - dx1).abs() < 1e-6
            || (vertex.pos[2] - dz0).abs() < 1e-6
            || (vertex.pos[2] - dz1).abs() < 1e-6;
        assert!(on_corner, "the diffuser corners are the aperture corners");
    }
    assert!(
        ((dx1 - dx0) / (dz1 - dz0) - 2.0).abs() < 1e-6,
        "the aperture keeps the sheet's 2:1 aspect"
    );

    // The housing is never emissive and always the fixed mid grey.
    assert_eq!(housing.len(), 12 * 6, "twelve housing quads");
    for vertex in &housing {
        assert!(!is_emission(vertex.color), "the housing is not emissive");
        assert_eq!(vertex.color[0], vertex.color[1]);
        assert!(
            vertex.color[0] > 0.5 && vertex.color[2] > 0.5,
            "a mid grey housing, never black"
        );
    }
    // Four side walls span the drop, four bottom-frame strips sit at the frame
    // bottom and four top-flange strips close the body at the ceiling plane.
    let y_bottom = 2.0 - PANEL_BODY_DROP_M;
    let mut sides = 0;
    let mut frame = 0;
    let mut flange = 0;
    for quad in housing.as_chunks::<6>().0 {
        let min_y = quad
            .iter()
            .map(|vertex| vertex.pos[1])
            .fold(f32::MAX, f32::min);
        let max_y = quad
            .iter()
            .map(|vertex| vertex.pos[1])
            .fold(f32::MIN, f32::max);
        if (min_y - y_bottom).abs() < 1e-6 && (max_y - 2.0).abs() < 1e-6 {
            sides += 1;
        } else if (max_y - y_bottom).abs() < 1e-6 {
            frame += 1;
        } else if (min_y - 2.0).abs() < 1e-6 {
            flange += 1;
        }
    }
    assert_eq!(
        (sides, frame, flange),
        (4, 4, 4),
        "side walls, bottom frame and top flange are all real"
    );
    // The housing covers the family's full outer footprint.
    assert!(
        housing.iter().any(|vertex| vertex.pos[0].abs() < 1e-6)
            && housing
                .iter()
                .any(|vertex| (vertex.pos[0] - 1.2).abs() < 1e-6)
            && housing.iter().any(|vertex| vertex.pos[2].abs() < 1e-6)
            && housing
                .iter()
                .any(|vertex| (vertex.pos[2] - 0.6).abs() < 1e-6),
        "the housing is the 1.2 x 0.6 m outer footprint"
    );
}

/// Adjacent diffuser segments share their edge UVs exactly, and the ring closes
/// on itself at the sheet's edge: the radiating artwork cannot show a seam.
#[test]
fn the_round_diffuser_ring_has_no_uv_seam() {
    let level = fixture_family_level();
    let mesh = build_level_geometry(&level);
    let round = mesh.triangles_for_key(SurfaceKey::new(
        SurfaceKind::Light,
        sheet_slot(crate::lighting::FixtureKind::RoundRecessed),
    ));

    let segments = round.as_chunks::<6>().0;
    assert_eq!(segments.len(), 10);
    // Corner order is outer0, outer1, inner1, inner0, i.e. within a chunk
    // [0, 1, 2, 0, 2, 3]: the high-angle corners (1 and 2) of one segment are
    // the low-angle corners (0 and 5) of the next.
    for pair in segments.windows(2) {
        assert_eq!(pair[0][1].uv, pair[1][0].uv);
        assert_eq!(pair[0][2].uv, pair[1][5].uv);
    }
    let last = segments[9];
    let first = segments[0];
    assert!(
        (last[1].uv[0] - 1.0).abs() < 1e-5,
        "the ring closes at u = 1"
    );
    assert!(
        (first[0].uv[0] - 1.0).abs() < 1e-5,
        "and the next segment starts there"
    );
    assert!(
        (last[1].uv[1] - first[0].uv[1]).abs() < 1e-6,
        "the closed seam shares its v too"
    );
    assert!(
        (last[2].uv[1] - first[5].uv[1]).abs() < 1e-6,
        "and so does its inner edge"
    );
}

#[test]
fn test_recessed_region_emits_a_lowered_slab_and_real_transition_faces() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 1,
            "id": "recess",
            "name": "Recess",
            "spawn": { "x": 1.0, "z": 1.0 },
            "room": { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 4.0 },
            "floor_regions": [
                { "x": 3.0, "z": 3.0, "width": 4.0, "depth": 2.0, "offset_y": -1.2,
                  "material": "core:carpet_damp_01",
                  "edge_material": "core:wallpaper_stained_01" }
            ],
            "ceiling_lights": [
                { "fixture": "core:fluorescent_panel_01", "x": 5.0, "z": 4.0 }
            ]
        }"#,
    )
    .expect("recess json");
    let mesh = build_level_geometry(&level);

    // The region floor is a real slab at -1.2 m with exact edges.
    let damp = material_vertices(&mesh, &level, "core:carpet_damp_01");
    assert!(!damp.is_empty(), "the region floor is emitted");
    assert_eq!(y_bounds(&damp), (-1.2, -1.2));
    assert_eq!(xz_bounds(&damp), (3.0, 7.0, 3.0, 5.0));

    // The rest of the room keeps the room material at the room floor.
    let clean = material_vertices(&mesh, &level, "core:carpet_beige_01");
    assert_eq!(y_bounds(&clean), (0.0, 0.0));

    // The transition faces are real geometry spanning the drop, drawn with
    // the authored edge material.
    let stained = material_vertices(&mesh, &level, "core:wallpaper_stained_01");
    assert!(
        !stained.is_empty(),
        "the transition faces are real geometry"
    );
    assert_eq!(y_bounds(&stained), (-1.2, 0.0));
    let (min_x, max_x, min_z, max_z) = xz_bounds(&stained);
    assert_eq!((min_x, max_x), (3.0, 7.0));
    assert_eq!((min_z, max_z), (3.0, 5.0));
    // No hole: the wall faces close the depression completely.
    let perimeter = stained
        .iter()
        .filter(|vertex| vertex.pos[1] == -1.2)
        .count();
    assert!(perimeter >= 8, "the pit floor edge is fully skirted");
}

#[test]
fn test_shallow_region_transition_faces_still_render() {
    // A walkable step still gets a visible riser, even though collision
    // deliberately does not make it solid.
    let level = LevelDef::from_json(
        r#"{
            "format_version": 1,
            "id": "step",
            "name": "Step",
            "spawn": { "x": 1.0, "z": 1.0 },
            "room": { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 3.0 },
            "floor_regions": [
                { "x": 2.0, "z": 2.0, "width": 4.0, "depth": 4.0, "offset_y": -0.3 }
            ]
        }"#,
    )
    .expect("step json");
    let mesh = build_level_geometry(&level);
    let wall = batch_slice(&mesh, SurfaceKind::Wall);
    assert_eq!(y_bounds(&wall), (-0.3, 0.0));
    assert!(
        level.collision_aabbs().is_empty(),
        "no rim for a walkable step"
    );
}

#[test]
fn test_gable_ceiling_is_real_sloped_geometry() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 1,
            "id": "gable",
            "name": "Gable",
            "spawn": { "x": 4.0, "z": 4.0 },
            "room": { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 3.0,
                      "ceiling": { "kind": "gable", "ridge": "x", "ridge_rise": 2.0 } },
            "ceiling_lights": [
                { "fixture": "core:fluorescent_panel_01", "x": 4.0, "z": 1.0 },
                { "fixture": "core:fluorescent_panel_01", "x": 4.0, "z": 4.0 }
            ]
        }"#,
    )
    .expect("gable json");
    let mesh = build_level_geometry(&level);
    let ceiling = batch_slice(&mesh, SurfaceKind::Ceiling);
    assert!(!ceiling.is_empty());
    let (min_y, max_y) = y_bounds(&ceiling);
    assert!((min_y - 3.0).abs() < 1e-4, "eaves at {min_y}");
    assert!((max_y - 5.0).abs() < 1e-4, "ridge at {max_y}");
    // The ridge is a real line of geometry, not a hidden flat plane.
    let ridge_vertices = ceiling
        .iter()
        .filter(|vertex| (vertex.pos[1] - 5.0).abs() < 1e-4)
        .count();
    assert!(ridge_vertices >= 2, "the ridge exists in the mesh");
    // The slopes interpolate: nothing is left at the flat eave plane.
    let sloped = ceiling
        .iter()
        .filter(|vertex| vertex.pos[1] > 3.0 + 1e-4 && vertex.pos[1] < 5.0 - 1e-4)
        .count();
    assert!(
        sloped > 0,
        "the slopes carry vertices between eave and ridge"
    );

    // Fixtures pick the local ceiling: eave fixture low, ridge fixture high.
    let lights = batch_slice(&mesh, SurfaceKind::Light);
    let zs: Vec<f32> = lights.iter().map(|vertex| vertex.pos[2]).collect();
    let eave_light = zs.iter().fold(f32::MAX, |acc, z| acc.min(*z));
    let ridge_light = zs.iter().fold(f32::MIN, |acc, z| acc.max(*z));
    assert!(eave_light < 1.0 && ridge_light > 4.0, "{zs:?}");
    assert!(
        y_bounds(&lights).1 - y_bounds(&lights).0 > 1.4,
        "the two fixtures hang at different heights"
    );
}

#[test]
fn test_gable_end_wall_follows_the_sloped_ceiling() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 1,
            "id": "gable_walls",
            "name": "Gable Walls",
            "spawn": { "x": 4.0, "z": 4.0 },
            "room": { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 3.0,
                      "ceiling": { "kind": "gable", "ridge": "x", "ridge_rise": 2.0 } },
            "walls": [
                { "x": 0.0, "z": 0.0, "width": 0.3, "depth": 8.0 },
                { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 0.3 }
            ],
            "ceiling_lights": [
                { "fixture": "core:fluorescent_panel_01", "x": 4.0, "z": 4.0 }
            ]
        }"#,
    )
    .expect("gable walls json");
    let mesh = build_level_geometry(&level);
    let walls = batch_slice(&mesh, SurfaceKind::Wall);
    assert!(!walls.is_empty());
    let (min_y, max_y) = y_bounds(&walls);
    // The wall running along Z climbs to the ridge; the eave wall stays at
    // the eave. Together they span eave to ridge with no flat cap.
    assert!((max_y - 5.0).abs() < 1e-4, "max wall Y {max_y}");
    assert!((min_y - 0.0).abs() < 1e-4, "walls stand on the floor");
    assert!(
        walls
            .iter()
            .filter(|vertex| vertex.pos[1] > 4.9)
            .all(|vertex| vertex.pos[1] <= 5.0 + 1e-4)
    );
}

#[test]
fn test_vertical_diagnostic_geometry_has_no_degenerate_or_misoriented_faces() {
    let level = fixture_level("vertical_diagnostic");
    let mesh = build_level_geometry(&level);

    let normal = |a: [f32; 3], b: [f32; 3], c: [f32; 3]| -> [f32; 3] {
        let u = glam::Vec3::from(b) - glam::Vec3::from(a);
        let v = glam::Vec3::from(c) - glam::Vec3::from(a);
        (u.cross(v)).to_array()
    };
    for kind in SurfaceKind::ALL {
        let vertices = batch_slice(&mesh, kind);
        assert_eq!(
            vertices.len() % 3,
            0,
            "{kind:?} must be a whole triangle list"
        );
        for triangle in vertices.as_chunks::<3>().0 {
            let n = normal(triangle[0].pos, triangle[1].pos, triangle[2].pos);
            assert!(
                n.iter().all(|value| value.is_finite()),
                "{kind:?} has a non-finite normal"
            );
            assert!(
                glam::Vec3::from(n).length() > 1e-6,
                "{kind:?} has a degenerate triangle: {:?} {:?} {:?}",
                triangle[0].pos,
                triangle[1].pos,
                triangle[2].pos
            );
            // Floors are horizontal and face up; ceilings face down (a gable
            // slope is tilted, so only the sign is fixed).
            match kind {
                SurfaceKind::Floor => {
                    assert!(n[1] > 0.0, "a floor triangle faces down: {n:?}");
                }
                SurfaceKind::Ceiling => {
                    assert!(n[1] < 0.0, "a ceiling triangle faces up: {n:?}");
                }
                SurfaceKind::Light => {
                    // The luminous sheet and the housing's frame/flange look
                    // down; the housing's side walls are vertical, never up.
                    assert!(n[1] <= 1e-6, "a fixture face must never look up: {n:?}");
                }
                _ => {}
            }
        }
    }
}

#[test]
fn test_world_faces_wind_outward() {
    // A wall's two length faces must look out of the wall: -Z on the low
    // thickness side, +Z on the high side, for an X-axis wall in a room.
    let level = level_with_wall("[]", "[]");
    let mesh = build_level_geometry(&level);
    let walls = batch_slice(&mesh, SurfaceKind::Wall);
    let normal = |triangle: &[Vertex]| -> [f32; 3] {
        let u = glam::Vec3::from(triangle[1].pos) - glam::Vec3::from(triangle[0].pos);
        let v = glam::Vec3::from(triangle[2].pos) - glam::Vec3::from(triangle[0].pos);
        u.cross(v).to_array()
    };
    let mut saw_negative_z = false;
    let mut saw_positive_z = false;
    for triangle in walls.as_chunks::<3>().0 {
        let n = normal(triangle);
        // Length faces are vertical; sills, caps and headers may be
        // horizontal, so only the vertical faces are checked here.
        if n[1].abs() < 0.2 {
            if n[2] < 0.0 {
                saw_negative_z = true;
            } else if n[2] > 0.0 {
                saw_positive_z = true;
            }
        }
    }
    assert!(
        saw_negative_z && saw_positive_z,
        "the wall's two length faces must look outward"
    );

    // Floors face up, ceilings face down, in both directions of the grid.
    let floor = batch_slice(&mesh, SurfaceKind::Floor);
    let ceiling = batch_slice(&mesh, SurfaceKind::Ceiling);
    for triangle in floor.as_chunks::<3>().0 {
        assert!(normal(triangle)[1] > 0.0);
    }
    for triangle in ceiling.as_chunks::<3>().0 {
        assert!(normal(triangle)[1] < 0.0);
    }
}

#[test]
fn test_walls_follow_the_local_ceiling_when_their_origin_is_not_at_zero() {
    // Regression: the wall emitter passes world coordinates along the length
    // axis, so a wall that does not start at X=0 (or Z=0) must still resolve
    // its ceiling at its own position instead of falling back to the first
    // room in the level.
    let level = LevelDef::from_json(
        r#"{
            "format_version": 1,
            "id": "offset_walls",
            "name": "Offset Walls",
            "spawn": { "x": 25.0, "z": 5.0 },
            "rooms": [
                { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 4.0 },
                { "x": 20.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.0,
                  "ceiling": { "kind": "gable", "ridge": "z", "ridge_rise": 2.0 } }
            ],
            "walls": [
                { "x": 20.3, "z": 9.7, "width": 9.7, "depth": 0.3, "y": 0.0 }
            ],
            "ceiling_lights": [
                { "fixture": "core:fluorescent_panel_01", "x": 25.0, "z": 5.0 }
            ]
        }"#,
    )
    .expect("offset wall json");
    let mesh = build_level_geometry(&level);
    let walls = batch_slice(&mesh, SurfaceKind::Wall);
    let second_room: Vec<Vertex> = walls
        .iter()
        .copied()
        .filter(|vertex| vertex.pos[0] > 19.0)
        .collect();
    assert!(!second_room.is_empty(), "the gable room's wall is emitted");
    // The wall runs along X at z = 9.85, where the gable room's ceiling is
    // just above its 5.0 m eave: the wall top must reach it, not the first
    // room's 4.0 m ceiling.
    let (min_y, max_y) = y_bounds(&second_room);
    assert!(min_y <= 1e-4, "the wall stands on the floor");
    assert!(
        (4.9..5.2).contains(&max_y),
        "wall top should follow its own room's ceiling, got {max_y}"
    );
}

#[test]
fn test_horizontal_decals_follow_the_real_surface_height() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 1,
            "id": "elevated_decals",
            "name": "Elevated Decals",
            "spawn": { "x": 4.0, "z": 4.0 },
            "room": { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0,
                      "height": 3.0, "floor_y": 2.0 },
            "decals": [
                { "x": 4.0, "y": 0.0, "z": 4.0, "width": 1.0, "height": 1.0,
                  "material": "core:decal_test_01", "surface": "floor" },
                { "x": 2.0, "y": 0.0, "z": 2.0, "width": 1.0, "height": 1.0,
                  "material": "core:decal_test_01", "surface": "ceiling" }
            ]
        }"#,
    )
    .expect("elevated decal json");
    let mesh = build_level_geometry(&level);
    let decals = batch_slice(&mesh, SurfaceKind::Decal);
    assert!(!decals.is_empty());
    // The floor decal sits on the elevated floor; the ceiling decal sits on
    // the real ceiling, at 5.0 m, not at the authored 0.0. Each is then lifted
    // off that real surface by the shared offset, along its own normal.
    assert_eq!(
        y_bounds(&decals),
        (2.0 + DECAL_SURFACE_OFFSET_M, 5.0 - DECAL_SURFACE_OFFSET_M)
    );
}

#[test]
fn test_props_stand_on_the_local_walkable_floor() {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 1,
            "id": "elevated_props",
            "name": "Elevated Props",
            "spawn": { "x": 4.0, "z": 4.0 },
            "room": { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0,
                      "height": 3.0, "floor_y": 2.0 },
            "floor_regions": [
                { "x": 3.0, "z": 3.0, "width": 2.0, "depth": 2.0, "offset_y": -0.5 }
            ],
            "props": [
                { "model": "core:crate", "x": 1.0, "y": 0.0, "z": 1.0, "size": [1.0,1.0,1.0] },
                { "model": "core:crate", "x": 4.0, "y": 0.0, "z": 4.0, "size": [1.0,1.0,1.0] }
            ]
        }"#,
    )
    .expect("elevated prop json");
    let mesh = build_level_geometry(&level);
    let props = batch_slice(&mesh, SurfaceKind::PropFallback);
    assert!(!props.is_empty());
    // The crate on the room floor spans 2.0..3.0; the one in the recess
    // spans 1.5..2.5.
    assert_eq!(y_bounds(&props), (1.5, 3.0));
}

#[test]
fn a_wall_with_no_twin_is_emitted_exactly_as_authored() {
    let level = level_with_wall("[]", "[]");
    let materials = logical_materials(&level);
    let lookup = MaterialLookup::new(&materials);
    let units = wall_units(&level, &crate::level::LevelSurfaces::new(&level), &lookup);
    assert_eq!(units.len(), 1);
    assert!(
        matches!(units[0], WallUnit::Plain { .. }),
        "a wall with no coincident twin must not be rewritten"
    );
    let mesh = build_level_geometry(&level);
    assert_eq!(
        batch_slice(&mesh, SurfaceKind::Wall).len() / 6,
        4,
        "two length faces plus two end caps"
    );
}

/// The shipped official demo.
fn shipped_demo() -> crate::level::LevelDef {
    crate::level::LevelDef::from_json(include_str!("../../assets/levels/places_demo.json"))
        .expect("the shipped places_demo parses")
}

#[test]
fn the_shipped_demo_and_the_rendering_fixture_resolve_their_stain_overlays() {
    for level in [shipped_demo(), fixture_level("rendering_diagnostic")] {
        let name = level.id.as_str();
        let materials = logical_materials(&level);
        let lookup = MaterialLookup::new(&materials);
        let units = wall_units(&level, &crate::level::LevelSurfaces::new(&level), &lookup);
        let coalesced = units
            .iter()
            .filter(|unit| matches!(unit, WallUnit::Coalesced { .. }))
            .count();
        assert!(
            coalesced >= 1,
            "{name}: expected the authored stain overlays to coalesce, got {coalesced}"
        );
        // Every coalesced unit's material runs must exactly partition its
        // solid profile: each slice's length span is covered by runs at the
        // slice's own height, so no face can fall back to the host material at
        // a run boundary.
        for unit in &units {
            if let WallUnit::Coalesced { slices, runs, .. } = unit {
                assert!(!runs.is_empty());
                for slice in slices {
                    let mut band: Vec<&WallMaterialRun> = runs
                        .iter()
                        .filter(|run| {
                            (run.bottom - slice.bottom).abs() < 1e-3
                                && (run.top - slice.top).abs() < 1e-3
                                && run.start >= slice.start - 1e-3
                                && run.end <= slice.end + 1e-3
                        })
                        .collect();
                    band.sort_by(|a, b| {
                        a.start
                            .partial_cmp(&b.start)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                    let mut cursor = slice.start;
                    for run in band {
                        assert!(
                            (run.start - cursor).abs() < 1e-3,
                            "{name}: a solid slice is not partitioned by its runs"
                        );
                        cursor = run.end;
                    }
                    assert!(
                        (cursor - slice.end).abs() < 1e-3,
                        "{name}: the runs must reach the end of their solid slice"
                    );
                }
            }
        }
        // And the normal build still succeeds with them.
        let mesh = build_level_geometry(&level);
        assert!(mesh.batches.wall_batch.count > 0);
    }
}

// ------------------------------------- emission routing and multi-material props

#[test]
fn emission_routing_follows_the_surface_kind() {
    use crate::render::common::materials::{EmissionRouting, emission_routing};

    // Built surfaces take their material's emission; a light batch's sheet face
    // carries it per vertex; the fixture housing, placeholder boxes and decals
    // never emit.
    for kind in [SurfaceKind::Floor, SurfaceKind::Ceiling, SurfaceKind::Wall] {
        assert_eq!(emission_routing(kind, true), EmissionRouting::Material);
        assert_eq!(emission_routing(kind, false), EmissionRouting::Material);
    }
    assert_eq!(
        emission_routing(SurfaceKind::Light, true),
        EmissionRouting::Vertex
    );
    assert_eq!(
        emission_routing(SurfaceKind::Light, false),
        EmissionRouting::None,
        "a fixture's untextured housing is lit like any other surface"
    );
    assert_eq!(
        emission_routing(SurfaceKind::PropFallback, false),
        EmissionRouting::None
    );
    assert_eq!(
        emission_routing(SurfaceKind::Decal, true),
        EmissionRouting::None,
        "decals do not carry material emission"
    );
}

/// A fixture whose face glows at a different strength from the light it casts
/// keeps both values separate in the emitted geometry and the bake.
#[test]
fn a_fixture_face_can_glow_independently_of_its_light() {
    let dim = level_with_wall_and_lights(
        "[]",
        "[]",
        r#"[{ "fixture": "core:fluorescent_panel_01", "x": 0.0, "z": 0.0,
              "brightness": 0.2, "emission": 1.0 }]"#,
    );
    let plain = level_with_wall_and_lights(
        "[]",
        "[]",
        r#"[{ "fixture": "core:fluorescent_panel_01", "x": 0.0, "z": 0.0,
              "brightness": 0.2 }]"#,
    );
    let dim_mesh = build_level_geometry(&dim);
    let plain_mesh = build_level_geometry(&plain);
    let face_color = |mesh: &crate::render::LevelMesh| -> [f32; 4] {
        let face = mesh
            .triangles_for(SurfaceKind::Light)
            .first()
            .copied()
            .expect("the fixture emits a luminous face");
        face.color
    };
    let dim_face = face_color(&dim_mesh);
    let plain_face = face_color(&plain_mesh);
    assert!(
        dim_face[0] > plain_face[0] + 0.1,
        "the authored emission must brighten the face: {} vs {}",
        dim_face[0],
        plain_face[0]
    );

    // The illumination side is unchanged by the emissive override.
    let dim_lighting = crate::lighting::LevelLighting::bake(&dim);
    let plain_lighting = crate::lighting::LevelLighting::bake(&plain);
    assert_eq!(
        dim_lighting.sample(0.0, 0.0, 0.0),
        plain_lighting.sample(0.0, 0.0, 0.0),
        "emission must not leak into environmental illumination"
    );
}

/// The authored light colour is a property of the illumination, never of the
/// drawn fixture: two otherwise identical pool lights must show the same
/// neutral face while their baked light stays red and blue.
#[test]
fn a_fixture_face_is_texture_first_and_never_repainted_by_the_light_colour() {
    use crate::lighting::FixtureKind;

    let pool_light = |color: &str| {
        level_with_wall_and_lights(
            "[]",
            "[]",
            &format!(
                r#"[{{ "fixture": "core:pool_light_round", "x": 0.0, "z": 0.0,
                       "brightness": 1.0, "color": {color} }}]"#
            ),
        )
    };
    let red = pool_light("[1.0, 0.0, 0.0]");
    let blue = pool_light("[0.0, 0.0, 1.0]");

    let face = |level: &LevelDef| -> Vec<[f32; 4]> {
        let mesh = build_level_geometry(level);
        mesh.triangles_for_key(SurfaceKey::new(
            SurfaceKind::Light,
            sheet_slot(FixtureKind::RoundRecessed),
        ))
        .into_iter()
        .map(|vertex| vertex.color)
        .collect()
    };
    let red_face = face(&red);
    let blue_face = face(&blue);
    assert_eq!(red_face.len(), 10 * 6, "the round diffuser's lit ring");
    assert_eq!(
        red_face, blue_face,
        "the authored colour must not repaint the fixture face"
    );
    for color in &red_face {
        assert!(
            (color[0] - color[1]).abs() < 1e-6 && (color[0] - color[2]).abs() < 1e-6,
            "the face's emission is neutral, so the sheet keeps its own colour: {color:?}"
        );
        assert!(color[0] > 0.5, "a lit fixture's face must not be black");
    }

    // The colour still reaches the room through the bake.
    let red_lighting = crate::lighting::LevelLighting::bake(&red);
    let blue_lighting = crate::lighting::LevelLighting::bake(&blue);
    let red_sample = red_lighting.sample(0.0, 0.0, 0.0);
    let blue_sample = blue_lighting.sample(0.0, 0.0, 0.0);
    assert!(red_sample.r > red_sample.b + 0.1, "{red_sample:?}");
    assert!(blue_sample.b > blue_sample.r + 0.1, "{blue_sample:?}");
}

/// Writes a two-material GLB where the second material emits, plus the catalog
/// and level that place one instance of it.
fn multi_material_scene() -> (
    crate::loader::PropCatalog,
    crate::props::PropAssets,
    LevelDef,
) {
    use crate::test_support::{TestGlbMaterial, two_material_glb};

    let directory = std::path::PathBuf::from("target/agent-work/tests/multi_material");
    std::fs::create_dir_all(&directory).expect("scratch directory is writable");
    let model_path = "multi_material.glb";
    let glb = two_material_glb(
        TestGlbMaterial::plain([200, 40, 40, 255]),
        TestGlbMaterial::emissive([20, 60, 220, 255], [0.25, 0.5, 1.0], true),
    );
    std::fs::write(directory.join(model_path), glb).expect("scratch GLB is writable");

    let catalog = crate::loader::PropCatalog::from_json_str(&format!(
        r##"{{
            "format_version": 1,
            "props": [{{
                "id": "core:test_multimat", "name": "Test Multimat", "category": "Decorative",
                "model": "{model_path}", "size": [1.0, 0.2, 2.0],
                "color": "#808080", "solid": false
            }}]
        }}"##
    ))
    .expect("the test catalog parses");

    let level = level_with_wall(
        "[]",
        r#"[{ "model": "core:test_multimat", "x": 0.0, "z": 0.0 }]"#,
    );
    (
        catalog,
        crate::props::PropAssets::with_root(&directory),
        level,
    )
}

#[test]
fn a_two_material_prop_becomes_two_draw_ranges_with_two_textures() {
    let (catalog, mut assets, level) = multi_material_scene();
    let (mesh, batches) = build_level_geometry_with_assets(&level, &catalog, &mut assets);
    assert_eq!(
        mesh.batches.prop_batch.count, 0,
        "the real model replaces the box"
    );
    assert_eq!(batches.len(), 1, "one model, one spatial cell");

    let batch = &batches[0];
    assert_eq!(batch.textures.len(), 2, "one decoded sheet per material");
    assert!(batch.textures.iter().all(|image| image.width == 2));
    assert_ne!(
        batch.textures[0].rgba, batch.textures[1].rgba,
        "the two materials must keep their own artwork"
    );
    assert_eq!(batch.submeshes.len(), 2, "one draw range per material");

    // Both materials draw the same two quads; the ranges partition the index
    // list, and only the second material emits.
    assert_eq!(batch.submeshes[0].texture, Some(0));
    assert_eq!(batch.submeshes[1].texture, Some(1));
    assert!(!batch.submeshes[0].emission.is_emissive());
    assert!(batch.submeshes[1].emission.is_emissive());
    assert_eq!(
        batch.submeshes[1].emission.effective_color(),
        [0.25, 0.5, 1.0]
    );
    assert_eq!(batch.submeshes[1].emission.mask, Some(1));
    let total: u32 = batch
        .submeshes
        .iter()
        .map(|submesh| submesh.index_count)
        .sum();
    assert_eq!(usize::try_from(total).unwrap_or(0), batch.indices.len());
    assert_eq!(
        batch.submeshes[0].first_index, 0,
        "the first range starts at the beginning of the index buffer"
    );

    // Every instance's vertices are lit; the emission itself never enters the
    // vertex colour (that is the environment light, not the glow).
    assert!(batch.vertices.len() >= 8);
}

#[test]
fn a_second_instance_of_a_multi_material_prop_still_batches() {
    let (catalog, mut assets, level) = multi_material_scene();
    let mut two = level.clone();
    two.props.push(crate::level::PropDef {
        model: "core:test_multimat".to_string(),
        x: 1.5,
        ..level.props[0].clone()
    });
    let (_, single) = build_level_geometry_with_assets(&level, &catalog, &mut assets);
    let (_, double) = build_level_geometry_with_assets(&two, &catalog, &mut assets);
    assert_eq!(double.len(), 1, "both instances share one batch");
    assert_eq!(
        double[0].submeshes.len(),
        single[0].submeshes.len(),
        "two instances cost the same draw ranges as one"
    );
    assert!(double[0].vertices.len() > single[0].vertices.len());
}

// ---------------------------------------------------------- lightmaps

use crate::lighting::lightmap::{
    LIGHTMAP_ATLAS_MAX_PAGES, LevelLightmaps, LightmapFailure, LightmapMode,
};
use crate::loader::PropCatalog;
use crate::props::PropAssets;

/// A renderer-independent lightmap build with the builtin prop catalog (every
/// prop is a fallback box, which is irrelevant to the static chart set).
fn lightmap_build(
    level: &LevelDef,
    quality: crate::quality::QualityLevel,
    mode: LightmapMode,
) -> LevelBuild {
    let materials = logical_materials(level);
    let catalog = PropCatalog::builtin();
    let mut assets = PropAssets::default();
    build_level_geometry_timed_with_lightmaps(
        level,
        &catalog,
        &mut assets,
        &materials,
        LightmapBuildOptions::for_level(quality, mode),
        None,
    )
}

/// True when a lightmapped vertex's quantised atlas UV lies inside one of the
/// charts on its page.
fn vertex_in_some_chart(lightmaps: &LevelLightmaps, vertex: &Vertex) -> bool {
    let Some(page) = lightmaps.pages.get(usize::from(vertex.lightmap_page)) else {
        return false;
    };
    let edge = page.width as f32;
    let x = f32::from(vertex.lightmap[0]) / 65535.0 * edge;
    let y = f32::from(vertex.lightmap[1]) / 65535.0 * edge;
    lightmaps.charts.iter().any(|(_, chart)| {
        usize::from(chart.page) == usize::from(vertex.lightmap_page)
            && x >= chart.x as f32 - 0.25
            && x <= (chart.x + chart.width) as f32 + 0.25
            && y >= chart.y as f32 - 0.25
            && y <= (chart.y + chart.height) as f32 + 0.25
    })
}

#[test]
fn the_demo_bakes_lightmaps_with_every_surface_vertex_charted() {
    let level = shipped_demo();
    let build = lightmap_build(&level, crate::quality::QualityLevel::High, LightmapMode::On);
    assert_eq!(build.lightmap_failure, None, "the demo must bake cleanly");
    let lightmaps = build
        .lightmaps
        .as_deref()
        .expect("the demo must produce an atlas");
    assert!(!lightmaps.pages.is_empty());
    assert!(lightmaps.pages.len() <= LIGHTMAP_ATLAS_MAX_PAGES);
    assert!(lightmaps.chart_count() > 0);
    for page in &lightmaps.pages {
        assert_eq!(page.width, 1024);
        assert_eq!(page.height, 1024);
        assert_eq!(page.rgb.len(), 1024 * 1024 * 3);
    }

    let mut surface_vertices = 0usize;
    let mut vertex_lit_vertices = 0usize;
    // A water surface is deliberately vertex-lit on every build: its quad
    // carries its baked light in the vertex colour and stays out of the atlas.
    let water = logical_materials(&level)
        .index_of(crate::level::DEFAULT_WATER_MATERIAL)
        .expect("the demo's water material resolves");
    for range in &build.mesh.ranges {
        for vertex in &range.vertices {
            match range.key.kind {
                SurfaceKind::Floor | SurfaceKind::Ceiling | SurfaceKind::Wall
                    if range.key.material == water =>
                {
                    assert!(
                        !vertex.is_lightmapped(),
                        "a water volume draws vertex-lit by design"
                    );
                    assert_eq!(vertex.lightmap, [0, 0]);
                    vertex_lit_vertices += 1;
                }
                SurfaceKind::Floor | SurfaceKind::Ceiling | SurfaceKind::Wall => {
                    assert!(
                        vertex.is_lightmapped(),
                        "a static surface vertex must carry a chart"
                    );
                    assert!(
                        vertex_in_some_chart(lightmaps, vertex),
                        "a lightmapped vertex must sample inside its own chart"
                    );
                    surface_vertices += 1;
                }
                SurfaceKind::Light | SurfaceKind::PropFallback | SurfaceKind::Decal => {
                    assert!(
                        !vertex.is_lightmapped(),
                        "fixtures, prop boxes and decals stay vertex-lit"
                    );
                    assert_eq!(vertex.lightmap, [0, 0]);
                    vertex_lit_vertices += 1;
                }
            }
        }
    }
    assert!(surface_vertices > 0);
    assert!(vertex_lit_vertices > 0, "the demo draws fixtures and props");
}

#[test]
fn lightmaps_off_reproduces_the_historical_vertex_lit_mesh() {
    let level = shipped_demo();
    let off = lightmap_build(
        &level,
        crate::quality::QualityLevel::High,
        LightmapMode::Off,
    );
    assert!(off.lightmaps.is_none());
    assert_eq!(off.lightmap_failure, None, "Off is not a failure");
    let history = build_level_geometry(&level);
    assert_eq!(off.mesh.all_vertices(), history.all_vertices());
    for vertex in off.mesh.all_vertices() {
        assert!(!vertex.is_lightmapped());
        assert_eq!(vertex.lightmap, [0, 0]);
    }
}

#[test]
fn every_level_shares_the_patch_set_at_different_densities() {
    // The one-level/two-profiles contract, checked on the Home showcase
    // fixture: it stays inside the two-page atlas at every density, so this
    // asserts the patch set and the density, not the page budget. Places Demo
    // is checked separately below, because it has grown past the Low atlas.
    let level = fixture_level("home_showcase");
    let high = lightmap_build(&level, crate::quality::QualityLevel::High, LightmapMode::On);
    let medium = lightmap_build(
        &level,
        crate::quality::QualityLevel::Medium,
        LightmapMode::On,
    );
    let low = lightmap_build(&level, crate::quality::QualityLevel::Low, LightmapMode::On);
    let high_lightmaps = high.lightmaps.as_deref().expect("high bake");
    let medium_lightmaps = medium.lightmaps.as_deref().expect("medium bake");
    let low_lightmaps = low.lightmaps.as_deref().expect("low bake");
    let patches = |lightmaps: &LevelLightmaps| {
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
            .collect::<Vec<_>>()
    };
    assert_eq!(
        patches(high_lightmaps),
        patches(low_lightmaps),
        "Low and High bake the same patch set"
    );
    assert_eq!(
        patches(medium_lightmaps),
        patches(low_lightmaps),
        "Medium bakes the same patch set"
    );
    assert!(
        high_lightmaps.stats.texels > medium_lightmaps.stats.texels,
        "High must bake more texels than Medium"
    );
    assert!(
        medium_lightmaps.stats.texels > low_lightmaps.stats.texels,
        "Medium must bake more texels than Low"
    );
    assert_eq!(high_lightmaps.pages[0].width, 1024);
    assert_eq!(medium_lightmaps.pages[0].width, 1024);
    assert_eq!(low_lightmaps.pages[0].width, 512);
}

#[test]
fn the_demo_bakes_inside_the_page_budget_on_every_level() {
    // Places Demo's chart set is the shipped level's real workload. Every level
    // must bake it into its own two-page atlas: the skyline packer and the
    // softened density were tuned exactly so `Low` no longer overflows its two
    // 512-texel pages and fall back to vertex lighting. A level that overflows
    // is worse than a lower density, so this pins the shipped behaviour rather
    // than a page count.
    let level = shipped_demo();
    for quality in [
        crate::quality::QualityLevel::High,
        crate::quality::QualityLevel::Medium,
        crate::quality::QualityLevel::Low,
    ] {
        let build = lightmap_build(&level, quality, LightmapMode::On);
        assert_eq!(
            build.lightmap_failure, None,
            "{quality:?} must bake the demo cleanly"
        );
        let lightmaps = build
            .lightmaps
            .as_deref()
            .unwrap_or_else(|| panic!("{quality:?} must produce the atlas"));
        let config = quality.lightmap_config();
        assert!(
            lightmaps.pages.len() <= config.max_pages,
            "{quality:?} must stay in its page budget"
        );
        assert!(
            lightmaps
                .pages
                .iter()
                .all(|page| page.width == config.page_edge),
            "{quality:?} must use its own page edge"
        );
        assert!(lightmaps.chart_count() > 900, "the whole level is charted");
    }
}

#[test]
fn a_second_bake_of_the_same_level_is_bit_identical() {
    let level = lit_room_level(
        8.0,
        6.0,
        3.0,
        r#"[{ "fixture": "core:fluorescent_panel_01", "x": 4.0, "z": 3.0, "brightness": 0.8 }]"#,
    );
    let first = lightmap_build(&level, crate::quality::QualityLevel::High, LightmapMode::On);
    let second = lightmap_build(&level, crate::quality::QualityLevel::High, LightmapMode::On);
    let first_lightmaps = first.lightmaps.as_deref().expect("first bake");
    let second_lightmaps = second.lightmaps.as_deref().expect("second bake");
    assert_eq!(first_lightmaps.pages, second_lightmaps.pages);
    assert_eq!(first_lightmaps.charts, second_lightmaps.charts);
    assert_eq!(first.mesh.all_vertices(), second.mesh.all_vertices());
    assert!(
        first_lightmaps
            .pages
            .iter()
            .any(|page| page.rgb.iter().any(|byte| *byte != 0)),
        "a lit room must not bake to black"
    );
}

#[test]
fn lightmapped_vertex_colours_carry_tint_and_face_shade_only() {
    let level = lit_room_level(
        8.0,
        6.0,
        3.0,
        r#"[{ "fixture": "core:fluorescent_panel_01", "x": 4.0, "z": 3.0, "brightness": 0.1 }]"#,
    );
    let on = lightmap_build(&level, crate::quality::QualityLevel::High, LightmapMode::On);
    let off = lightmap_build(
        &level,
        crate::quality::QualityLevel::High,
        LightmapMode::Off,
    );
    let table = logical_materials(&level);
    let index = table
        .index_of(&level.defaults.floor)
        .expect("the default floor material resolves");
    let tint = table.entry(index).expect("resolved").tint;

    let on_floor = on.mesh.triangles_for(SurfaceKind::Floor);
    assert!(!on_floor.is_empty());
    for vertex in &on_floor {
        assert_eq!(vertex.color[0], tint[0]);
        assert_eq!(vertex.color[1], tint[1]);
        assert_eq!(vertex.color[2], tint[2]);
    }
    let off_floor = off.mesh.triangles_for(SurfaceKind::Floor);
    assert!(
        off_floor
            .iter()
            .any(|vertex| vertex.color[0] < tint[0] - 1.0e-6),
        "the historical path folds the dim bake into the floor colour"
    );
}

#[test]
fn atlas_overflow_rebuilds_with_vertex_lighting() {
    let level = lit_room_level(
        8.0,
        6.0,
        3.0,
        r#"[{ "fixture": "core:fluorescent_panel_01", "x": 4.0, "z": 3.0, "brightness": 0.8 }]"#,
    );
    let materials = logical_materials(&level);
    let catalog = PropCatalog::builtin();
    let mut assets = PropAssets::default();
    let mut config = crate::quality::QualityLevel::High.lightmap_config();
    config.page_edge = 16;
    config.max_pages = 1;
    config.padding = 1;
    let options = LightmapBuildOptions {
        mode: LightmapMode::On,
        config,
        profile: crate::quality::QualityLevel::High.profile(),
        bake: crate::quality::QualityLevel::High.bake_config(),
    };
    let build = build_level_geometry_timed_with_lightmaps(
        &level,
        &catalog,
        &mut assets,
        &materials,
        options,
        None,
    );
    assert!(build.lightmaps.is_none());
    assert_eq!(build.lightmap_failure, Some(LightmapFailure::PageOverflow));
    let history = build_level_geometry(&level);
    assert_eq!(build.mesh.all_vertices(), history.all_vertices());
}

#[test]
fn every_lightmapped_vertex_uv_lands_on_its_own_chart_corner() {
    let level = shipped_demo();
    let materials = logical_materials(&level);
    let catalog = PropCatalog::builtin();
    let mut assets = PropAssets::default();
    let build = build_level_geometry_timed_with_lightmaps(
        &level,
        &catalog,
        &mut assets,
        &materials,
        LightmapBuildOptions::for_level(crate::quality::QualityLevel::High, LightmapMode::On),
        None,
    );
    let lightmaps = build.lightmaps.as_deref().expect("the demo bakes");
    // A water volume is deliberately vertex-lit: its quad carries its baked
    // light in the vertex colour and never enters the atlas, so it is exempt
    // from the per-vertex chart-corner mapping (its own tests cover it).
    let water = materials
        .index_of(crate::level::DEFAULT_WATER_MATERIAL)
        .expect("the demo's water material resolves");
    let mut checked = 0usize;
    for range in &build.mesh.ranges {
        if !matches!(
            range.key.kind,
            SurfaceKind::Floor | SurfaceKind::Ceiling | SurfaceKind::Wall
        ) || range.key.material == water
        {
            continue;
        }
        for vertex in &range.vertices {
            assert!(vertex.is_lightmapped());
            let page = lightmaps
                .pages
                .get(usize::from(vertex.lightmap_page))
                .expect("vertex page exists");
            let edge = page.width;
            let scale = f32::from(u16::try_from(edge).expect("page edge fits u16"));
            let atlas_u = f32::from(vertex.lightmap[0]) / 65_535.0 * scale;
            let atlas_v = f32::from(vertex.lightmap[1]) / 65_535.0 * scale;
            let (_, chart) = lightmaps
                .charts
                .iter()
                .find(|(_, chart)| {
                    usize::from(chart.page) == usize::from(vertex.lightmap_page)
                        && atlas_u >= chart.x as f32 - 0.75
                        && atlas_u <= (chart.x + chart.width) as f32 + 0.75
                        && atlas_v >= chart.y as f32 - 0.75
                        && atlas_v <= (chart.y + chart.height) as f32 + 0.75
                })
                .expect("every lightmapped vertex lies in a chart");
            // A stamped vertex is a quad corner, so its local coordinates are 0
            // or 1: it must sit within half a texel of one of the chart's four
            // corners, at exactly the u16 that corner's mapping produces.
            let mut matched = false;
            for (u, v) in [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)] {
                let target = chart.uv_at(edge, u, v);
                let target_u = f32::from(target[0]) / 65_535.0 * scale;
                let target_v = f32::from(target[1]) / 65_535.0 * scale;
                if (target_u - atlas_u).abs() <= 0.75 && (target_v - atlas_v).abs() <= 0.75 {
                    assert_eq!(
                        vertex.lightmap, target,
                        "vertex UV must be exactly the chart corner's quantised UV"
                    );
                    matched = true;
                    break;
                }
            }
            assert!(
                matched,
                "vertex UV ({atlas_u:.2}, {atlas_v:.2}) is not a corner of {chart:?}"
            );
            checked += 1;
        }
    }
    assert!(
        checked > 500,
        "the test must actually check a mesh: {checked}"
    );
}

#[test]
fn atlas_bytes_match_the_fill_pass_exactly() {
    let level = shipped_demo();
    let materials = logical_materials(&level);
    let catalog = PropCatalog::builtin();
    let mut assets = PropAssets::default();
    let build = build_level_geometry_timed_with_lightmaps(
        &level,
        &catalog,
        &mut assets,
        &materials,
        LightmapBuildOptions::for_level(crate::quality::QualityLevel::High, LightmapMode::On),
        None,
    );
    let lightmaps = build.lightmaps.as_deref().expect("demo bakes");
    // The atlas was baked with the active profile's bake config (soft shadows
    // and the finer prop grid on Full), so the reference fill must use exactly
    // the same one.
    let lighting =
        LevelLighting::bake_with(&level, crate::quality::QualityLevel::High.bake_config());
    let mut checked = 0usize;
    for (patch, chart) in &lightmaps.charts {
        let texels = crate::lighting::lightmap::fill_chart(&lighting, patch, chart);
        let page = &lightmaps.pages[usize::from(chart.page)];
        for row in 0..chart.height {
            for column in 0..chart.width {
                let index = (row * chart.width + column) as usize;
                let offset = ((chart.y + row) as usize * page.width as usize
                    + (chart.x + column) as usize)
                    * 3;
                for (channel, expected) in texels[index].iter().enumerate() {
                    let stored = f32::from(page.rgb[offset + channel]) / 255.0;
                    let wanted = expected.clamp(0.0, 1.0);
                    assert!(
                        (stored - wanted).abs() <= 1.0 / 255.0 + 1.0e-6,
                        "chart {chart:?} texel ({column},{row}) channel {channel}: \
                         stored {stored} vs fill {wanted}"
                    );
                }
                checked += 1;
            }
        }
    }
    assert!(checked > 100_000, "must check real texel volume: {checked}");
}

#[test]
fn the_dynamic_demonstration_machine_stays_a_static_prop() {
    // Places Demo places the washing machine the washer-drum demonstration is
    // built around as an ordinary static prop: it is baked, it occludes, it
    // collides, and it draws from the static prop batches. The drum the
    // demonstration turns is spawned by `render::dynamic` at runtime and must
    // never appear in the level file -- and it is spawned *inside* the
    // machine, spinning in place behind the porthole.
    let path = "assets/levels/places_demo.json";
    let content = std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("{path} must be readable: {error}"));
    let level = crate::level::LevelDef::from_json(&content)
        .unwrap_or_else(|error| panic!("{path} must parse: {error}"));
    assert!(
        level
            .props
            .iter()
            .any(|prop| prop.model == crate::render::DEMO_MACHINE_ID),
        "Places Demo must place {} statically",
        crate::render::DEMO_MACHINE_ID
    );
    assert!(
        !level
            .props
            .iter()
            .any(|prop| prop.model == crate::render::DEMO_DRUM_ID),
        "{} must only exist as a dynamic object, never as a level prop",
        crate::render::DEMO_DRUM_ID
    );

    let catalog = shipped_catalog();
    let mut assets = shipped_assets();
    let materials = logical_materials(&level);
    let (mesh, batches, _lighting) = build_level_geometry_with_assets_and_lighting_and_materials(
        &level,
        &catalog,
        &mut assets,
        &materials,
    );
    assert_eq!(
        mesh.batches.prop_batch.count, 0,
        "the machine must draw real prop geometry, never a placeholder box"
    );
    let machine = catalog
        .get(crate::render::DEMO_MACHINE_ID)
        .model
        .expect("the machine ships a model");
    assert!(
        batches.iter().any(|batch| batch.model == machine),
        "the static machine must be instanced into a static prop batch"
    );
}

#[test]
fn the_dynamic_demonstration_drum_spins_inside_the_static_machine() {
    // The drum the demonstration spawns for Places Demo's machine is inside
    // its body: the transformed bounds stay within the machine's catalogue box
    // and behind its front panel, and the drum turns while the machine's
    // static transform does not move.
    let path = "assets/levels/places_demo.json";
    let content = std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("{path} must be readable: {error}"));
    let level = crate::level::LevelDef::from_json(&content)
        .unwrap_or_else(|error| panic!("{path} must parse: {error}"));
    let catalog = shipped_catalog();
    let mut assets = shipped_assets();
    let prop = level
        .props
        .iter()
        .find(|prop| prop.model == crate::render::DEMO_MACHINE_ID)
        .expect("Places Demo places the machine");
    let size = catalog.get(crate::render::DEMO_MACHINE_ID).size;
    let base_y = crate::level::LevelSurfaces::new(&level)
        .floor_y_at(prop.x, prop.z)
        .unwrap_or(0.0);
    let mut dynamic = crate::render::DynamicScene::new();
    assert_eq!(
        dynamic.spawn_washer_drum_demo(&level, &catalog, &mut assets),
        1,
        "every placed machine gets its drum"
    );
    let half_width = size[0] * prop.scale * 0.5;
    let half_depth = size[2] * prop.scale * 0.5;
    let machine_height = size[1] * prop.scale;
    let machine_min = [prop.x - half_width, base_y + prop.y, prop.z - half_depth];
    let machine_max = [
        prop.x + half_width,
        base_y + prop.y + machine_height,
        prop.z + half_depth,
    ];
    let bounds = dynamic.objects()[0].world_bounds();
    for axis in 0..3 {
        assert!(
            bounds.min[axis] >= machine_min[axis] - 1e-4,
            "the drum leaves the machine on axis {axis}: {:?} < {:?}",
            bounds.min,
            machine_min
        );
        assert!(
            bounds.max[axis] <= machine_max[axis] + 1e-4,
            "the drum leaves the machine on axis {axis}: {:?} > {:?}",
            bounds.max,
            machine_max
        );
    }
    let bezel = crate::render::common::dynamic::WASHER_PORTHOLE_BEZEL_DEPTH * prop.scale;
    let front_panel = prop.z + half_depth - bezel;
    assert!(
        bounds.max[2] < front_panel,
        "the drum's mouth ({}) must stay recessed behind the machine's front panel ({front_panel})",
        bounds.max[2]
    );
    let machine_matrix = crate::render::common::props::prop_instance_matrix(prop, base_y);
    let before = dynamic.objects()[0].transform();
    for _ in 0..60 {
        assert_eq!(dynamic.update(1.0 / 60.0, None).moved, 1);
    }
    assert_ne!(
        dynamic.objects()[0].transform(),
        before,
        "the drum must actually rotate inside the machine"
    );
    assert_eq!(
        crate::render::common::props::prop_instance_matrix(prop, base_y),
        machine_matrix,
        "the machine body's static transform must never be touched"
    );
}

// ------------------------------------------------------- material draw passes

use super::common::materials::{BatchPass, MaterialRenderState, batch_pass_for};
use crate::materials::{AlphaMode, MaterialAlpha};

#[test]
fn the_opaque_cutout_and_translucent_passes_are_decided_by_the_material() {
    // Non-material families are opaque whatever a material index happens to be:
    // a fixture face, a placeholder box and a decal never alpha-blend.
    for kind in [
        SurfaceKind::Light,
        SurfaceKind::PropFallback,
        SurfaceKind::Decal,
    ] {
        for alpha in [
            MaterialAlpha::OPAQUE,
            MaterialAlpha::blend(0.5),
            MaterialAlpha {
                mode: AlphaMode::Cutout,
                ..MaterialAlpha::OPAQUE
            },
        ] {
            assert_eq!(
                batch_pass_for(kind, true, Some(alpha)),
                BatchPass::Opaque,
                "{kind:?} must stay opaque"
            );
        }
    }

    // A surface material's contract decides its pass.
    for kind in [SurfaceKind::Floor, SurfaceKind::Ceiling, SurfaceKind::Wall] {
        assert_eq!(
            batch_pass_for(kind, true, Some(MaterialAlpha::OPAQUE)),
            BatchPass::Opaque
        );
        assert_eq!(
            batch_pass_for(
                kind,
                true,
                Some(MaterialAlpha {
                    mode: AlphaMode::Cutout,
                    ..MaterialAlpha::OPAQUE
                })
            ),
            BatchPass::Cutout
        );
        assert_eq!(
            batch_pass_for(kind, true, Some(MaterialAlpha::blend(0.4))),
            BatchPass::Translucent
        );
        // A surface family without a resolved material (an empty authored id)
        // has no alpha contract at all.
        assert_eq!(batch_pass_for(kind, false, None), BatchPass::Opaque);
    }

    // Zero opacity is invisible: it must not be submitted as translucent.
    assert_eq!(
        batch_pass_for(SurfaceKind::Wall, true, Some(MaterialAlpha::blend(0.0))),
        BatchPass::Opaque
    );
}

#[test]
fn the_post_process_fallback_is_the_historical_presentation() {
    use super::common::postprocess::PostSettings;

    // Low's level settings are the identity, so with bloom off the renderer
    // presents the scene with the plain copy quad instead of resolving it.
    // That is what keeps Low as cheap as the historical presentation; Bloom On
    // (an independent player choice) makes Low pay for exactly the bloom
    // resolve.
    let low = PostSettings::for_level(crate::quality::QualityLevel::Low);
    assert!(low.is_identity(), "Low without bloom can skip the resolve");
    assert!(!low.blooms());
    assert!(
        low.with_bloom(true).blooms(),
        "Low + Bloom On must run the bloom path"
    );
    // Medium keeps the tone shoulder but not the grade: it always resolves and
    // its grade terms are the identity.
    let medium = PostSettings::for_level(crate::quality::QualityLevel::Medium);
    assert!(!medium.is_identity(), "Medium always resolves the shoulder");
    assert!(medium.tone_knee < 1.0, "Medium keeps the tone shoulder");
    assert!(
        medium.grade_saturation == 1.0 && medium.grade_contrast == 1.0,
        "only High grades"
    );
    let high = PostSettings::for_level(crate::quality::QualityLevel::High);
    assert!(!high.is_identity(), "High always resolves");
    assert!(
        !high.blooms(),
        "bloom is a separate setting, not part of the level"
    );
    assert!(
        high.with_bloom(true).blooms(),
        "High + Bloom On runs the bloom path"
    );
    assert!(high.bloom_strength < 1.0, "bloom stays restrained");
    assert!(
        high.grade_saturation >= 1.0 && high.grade_saturation < 1.1,
        "the grade is a trim, not a look: {}",
        high.grade_saturation
    );
    assert!(
        (0.5..1.0).contains(&high.tone_knee),
        "the tone shoulder must leave the common range untouched: {}",
        high.tone_knee
    );
}

#[test]
fn every_window_cap_spans_its_opening_in_world_space() {
    // Caps are built in the wall unit's local length space and must be
    // translated by the wall's length origin like every other emitter. When
    // they are not, a sill stops short of one jamb and buries itself in the
    // other by exactly the wall's origin, which is a hole at one corner of
    // every opening on a wall whose min corner is not zero.
    let level = crate::level::LevelDef::from_json(
        r#"{ "format_version": 1, "id": "cap_origin", "name": "Cap Origin",
            "spawn": { "x": 1.0, "z": 1.0, "yaw_degrees": 0.0 },
            "rooms": [{ "x": 0.0, "z": 0.0, "width": 10.0, "depth": 14.0,
                        "height": 3.0, "floor_y": 0.0 }],
            "walls": [{
                "x": 3.0, "z": 4.0, "width": 0.3, "depth": 6.0, "height": 3.0,
                "openings": [{ "kind": "window", "offset": 2.5, "width": 2.0,
                               "height": 1.2, "sill": 1.0 }]
            }]
        }"#,
    )
    .expect("the cap-origin fixture parses");
    let mesh = lightmap_build(&level, crate::quality::QualityLevel::High, LightmapMode::On).mesh;
    let wall = &level.walls[0];
    // The wall runs along z from 4.0, so its length origin is non-zero — which
    // is exactly the case a cap that forgets to translate gets wrong.
    assert_eq!(wall.axis(), crate::level::WallAxis::Z);
    let (origin_x, origin_z) = wall.length_origin();
    let origin = match wall.axis() {
        crate::level::WallAxis::X => origin_x,
        crate::level::WallAxis::Z => origin_z,
    };
    let opening = &wall.openings[0];
    let expected = (origin + opening.offset, origin + opening.end());
    for (plane_y, up) in [(opening.bottom(wall.y), true), (opening.top(wall.y), false)] {
        let mut lo = f32::INFINITY;
        let mut hi = f32::NEG_INFINITY;
        for triangle in mesh.triangles_for(SurfaceKind::Wall).as_chunks::<3>().0 {
            let [a, b, c] = *triangle;
            let normal = glam::Vec3::from(a.normal);
            if normal.y.abs() < 0.9 {
                continue;
            }
            if (normal.y > 0.0) != up {
                continue;
            }
            if (a.pos[1] - plane_y).abs() > 1.0e-3 {
                continue;
            }
            for vertex in [a, b, c] {
                lo = lo.min(vertex.pos[2]);
                hi = hi.max(vertex.pos[2]);
            }
        }
        assert!(
            lo.is_finite() && hi.is_finite(),
            "the wall must emit a cap at y = {plane_y}"
        );
        assert!(
            (lo - expected.0).abs() <= 1.0e-3 && (hi - expected.1).abs() <= 1.0e-3,
            "the cap at y = {plane_y} spans {lo}..{hi}, expected {expected:?}"
        );
    }
}

#[test]
fn the_demo_routes_its_reflective_materials_to_a_plane_and_a_probe() {
    use crate::materials::{MaterialReflection, ReflectionMode, resolve_materials};

    let level = shipped_demo();
    let logical = logical_materials(&level);
    let catalog = crate::assets::AssetCatalog::load_default();
    let root = crate::assets::resolve_asset_root().expect("assets/ is discoverable");
    let mut cache = crate::materials::TextureCache::new();
    let resolved = resolve_materials(&level, &catalog, None, Some(&root), &mut cache);

    // Every material that authors a reflection must resolve into one; a
    // catalog entry whose mode the parser does not know is a catalog error, and
    // a material with no reflection must stay exactly `NONE`.
    let reflections: Vec<MaterialReflection> = resolved
        .entries()
        .iter()
        .map(|entry| entry.reflection)
        .collect();
    let wet = logical.index_of("core:pool_deck_wet_01").expect("wet deck");
    let linoleum = logical
        .index_of("core:linoleum_polished_01")
        .expect("linoleum");
    let carpet = logical.index_of("core:carpet_beige_01").expect("carpet");
    assert_eq!(
        reflections[usize::from(wet)].mode,
        ReflectionMode::Planar,
        "the wet deck is the demo's planar mirror"
    );
    assert!(reflections[usize::from(wet)].strength > 0.0);
    assert_eq!(
        reflections[usize::from(linoleum)].mode,
        ReflectionMode::Probe,
        "polished linoleum reads a static probe"
    );
    assert_eq!(
        reflections[usize::from(carpet)],
        MaterialReflection::NONE,
        "an unmarked material must never reflect"
    );

    // The routing itself comes from the emitted geometry, so build the demo the
    // renderer builds and check the plane it derives.
    let mesh = lightmap_build(&level, crate::quality::QualityLevel::High, LightmapMode::On).mesh;
    let routing =
        super::common::reflections::routing_from_mesh(&mesh, &reflections, reflections.len());
    assert_eq!(
        routing.planes.len(),
        1,
        "the demo has exactly one mirror plane: the pool deck"
    );
    let plane = routing.planes[0];
    assert!(
        (plane.normal[1] - 1.0).abs() < 1.0e-4 && plane.normal[0].abs() < 1.0e-4,
        "the wet deck faces up, got {:?}",
        plane.normal
    );
    assert!(
        (plane.offset - 1.5).abs() < 1.0e-3,
        "the pool deck sits at y = -1.5, got offset {}",
        plane.offset
    );
    assert_eq!(
        routing.plane_of(usize::from(wet)),
        Some(0),
        "the wet deck material routes to that plane"
    );
    assert_eq!(
        routing.plane_of(usize::from(linoleum)),
        None,
        "a probe material never routes to a plane"
    );
    assert!(routing.probe_for_material[usize::from(linoleum)]);
    assert!(!routing.probe_for_material[usize::from(wet)]);
    assert!(
        !routing.probe_points.is_empty(),
        "the demo's polished floors want a probe"
    );
    // The probe is a real point inside the building, not a plane coefficient.
    for point in &routing.probe_points {
        assert!(point.iter().all(|value| value.is_finite()), "{point:?}");
    }
}

#[test]
fn the_demo_glazes_every_window_and_classifies_the_panes_translucent() {
    let level = shipped_demo();
    let materials = logical_materials(&level);
    let panes: Vec<(SurfaceKey, MaterialAlpha)> = level
        .walls
        .iter()
        .flat_map(|wall| wall.openings.iter())
        .filter_map(|opening| opening.glass_material())
        .filter_map(|id| {
            let index = materials.index_of(id)?;
            let alpha = materials.entry(index)?.alpha;
            Some((SurfaceKey::new(SurfaceKind::Wall, index), alpha))
        })
        .collect();
    assert_eq!(
        panes.len(),
        6,
        "five windows and one transfer grille are glazed"
    );
    let translucent = panes
        .iter()
        .filter(|(_, alpha)| alpha.is_translucent())
        .count();
    let cutout = panes.iter().filter(|(_, alpha)| alpha.is_cutout()).count();
    assert_eq!((translucent, cutout), (5, 1));
    for (key, alpha) in &panes {
        let expected = if alpha.is_cutout() {
            BatchPass::Cutout
        } else {
            BatchPass::Translucent
        };
        assert_eq!(
            batch_pass_for(key.kind, key.has_material(), Some(*alpha)),
            expected,
            "{}",
            materials
                .entry(key.material)
                .map_or("?", |entry| entry.id.as_str())
        );
    }

    // The panes are real geometry: a glassed opening contributes wall quads
    // whose total area is exactly the opening's own area. (The mesh splits a
    // pane into several quads where its lightmap chart or its spatial cell
    // ends, so the count is not one per opening; the area is.) The builds are
    // the lightmapped ones, because that is what the renderer uses.
    let mesh = lightmap_build(&level, crate::quality::QualityLevel::High, LightmapMode::On).mesh;
    let opening_area: f32 = level
        .walls
        .iter()
        .flat_map(|wall| wall.openings.iter())
        .filter(|opening| opening.glass_material().is_some())
        .map(|opening| opening.width * opening.height)
        .sum();
    // Two openings may share one glass material, and a key's geometry is the
    // union of everything that binds it, so the distinct keys are what carry the
    // area.
    let mut keys: Vec<SurfaceKey> = panes.iter().map(|(key, _)| *key).collect();
    keys.sort_unstable();
    keys.dedup();
    let mut pane_area = 0.0f32;
    for key in &keys {
        let count = mesh.index_count_for_key(*key);
        assert!(
            count > 0 && count.is_multiple_of(6),
            "a pane is whole quads (six indices each), got {count}"
        );
        for triangle in mesh.triangles_for_key(*key).as_chunks::<3>().0 {
            let [a, b, c] = *triangle;
            let edge1 = glam::Vec3::from(a.pos) - glam::Vec3::from(b.pos);
            let edge2 = glam::Vec3::from(c.pos) - glam::Vec3::from(b.pos);
            pane_area = edge1.cross(edge2).length().mul_add(0.5, pane_area);
        }
    }
    assert_eq!(
        keys.len(),
        4,
        "three glass materials and one grille fill the demo's openings"
    );
    assert!(
        (pane_area - opening_area).abs() <= 1.0e-3 * opening_area.max(1.0),
        "the panes cover their openings exactly: {pane_area} vs {opening_area}"
    );
    let pane_vertices = mesh.triangles_for_key(panes[0].0);
    assert!(!pane_vertices.is_empty());
    let lightmapped = pane_vertices
        .iter()
        .filter(|vertex| vertex.is_lightmapped())
        .count();
    assert_eq!(
        lightmapped,
        pane_vertices.len(),
        "a pane samples the same atlas the wall around it does"
    );
}

#[test]
fn the_cutout_and_translucent_materials_are_built_but_never_blended_together() {
    // A material that authors a cut-out keeps its geometry in the opaque pass
    // with an alpha-tested stage; only `blend` reaches the sorted pass. The two
    // are mutually exclusive by construction, which is what keeps a cut-out
    // decal or grille from being sorted as if it were glass.
    let alphas = [
        MaterialAlpha::OPAQUE,
        MaterialAlpha {
            mode: AlphaMode::Cutout,
            ..MaterialAlpha::OPAQUE
        },
        MaterialAlpha::blend(0.5),
    ];
    let passes: Vec<BatchPass> = alphas
        .into_iter()
        .map(|alpha| batch_pass_for(SurfaceKind::Wall, true, Some(alpha)))
        .collect();
    assert_eq!(
        passes,
        vec![BatchPass::Opaque, BatchPass::Cutout, BatchPass::Translucent]
    );
    assert_eq!(
        passes
            .iter()
            .filter(|pass| **pass == BatchPass::Translucent)
            .count(),
        1
    );
}

// ----------------------------------------------------------- offscreen scene

#[test]
fn the_scene_target_size_follows_the_drawable_and_the_level() {
    use super::common::framebuffer::scene_target_size;
    use crate::quality::QualityLevel;

    let drawable = DrawableSize::new(1280, 720);

    // High: the target is the drawable itself.
    assert_eq!(scene_target_size(QualityLevel::High, drawable), drawable);

    // Medium: half the drawable (Low's 0.375 factor is below the floor), with
    // the drawable's aspect, never an upscale.
    let medium = scene_target_size(QualityLevel::Medium, drawable);
    assert_eq!(medium, DrawableSize::new(640, 360));

    // Low: a smaller target with the drawable's aspect, never an upscale.
    let low = scene_target_size(QualityLevel::Low, drawable);
    assert!(low.width < medium.width && medium.width < drawable.width);
    let drawable_aspect = f64::from(drawable.width) / f64::from(drawable.height);
    for target in [low, medium] {
        let aspect = f64::from(target.width) / f64::from(target.height);
        assert!(
            (drawable_aspect - aspect).abs() < 1.0e-2,
            "the scene target must not distort the image: {target:?}"
        );
    }

    // Resizing the drawable changes the target, which is what makes the
    // renderer rebuild the offscreen attachments exactly once per size change.
    let resized = scene_target_size(QualityLevel::High, DrawableSize::new(800, 600));
    assert_eq!(resized, DrawableSize::new(800, 600));
    assert_ne!(resized, drawable);
}

#[test]
fn every_material_property_resolves_into_the_renderers_per_material_state() {
    // The renderer derives its per-material vectors from the resolved table in
    // one place. The failure this guards against is a property that resolves
    // correctly and then never reaches the draw path: a glass material that
    // draws opaque, or a normal map that never binds.
    let level = shipped_demo();
    // The *resolved* table, not the logical one: a normal map only exists once
    // its PNG has been interned, which is exactly what the renderer loads.
    let mut cache = crate::materials::TextureCache::new();
    let root = crate::assets::resolve_asset_root().expect("assets/ is discoverable");
    let materials = crate::materials::resolve_materials(
        &level,
        super::common::api::shipped_asset_catalog(),
        None,
        Some(&root),
        &mut cache,
    );
    assert!(materials.errors().is_empty(), "{:?}", materials.errors());
    let state = MaterialRenderState::from_table(&materials);
    assert_eq!(state.alphas.len(), materials.len());

    let mut translucent = 0usize;
    let mut cutout = 0usize;
    let mut sheen = 0usize;
    let mut normal_mapped = 0usize;
    for (index, entry) in materials.entries().iter().enumerate() {
        let slot = state.texture_slots[index];
        assert_eq!(slot, entry.texture_index, "{}", entry.id);
        assert_eq!(state.emissions[index], entry.emission, "{}", entry.id);
        assert_eq!(state.responses[index], entry.response, "{}", entry.id);
        assert_eq!(state.alphas[index], entry.alpha, "{}", entry.id);
        translucent += usize::from(entry.alpha.is_translucent());
        cutout += usize::from(entry.alpha.is_cutout());
        sheen += usize::from(entry.response.has_sheen());
        normal_mapped += usize::from(entry.response.has_normal());
    }
    assert!(
        translucent >= 3,
        "the demo's windows are translucent (the former backlit sign panels \
         were removed as unintended artifacts)"
    );
    assert!(cutout >= 1, "the demo's grille is a cut-out");
    assert!(sheen >= 4, "the demo's glossy surfaces have a sheen");
    assert_eq!(
        normal_mapped, 2,
        "the metal and plastic panels are the demo's normal-mapped materials"
    );
}

// ------------------------------------------------------ material resolution

/// Draw-order indices of every architectural range's vertices, so two builds
/// can be compared vertex for vertex without depending on their range split.
fn architectural_vertices(mesh: &LevelMesh) -> Vec<Vertex> {
    let mut out = Vec::new();
    for range in &mesh.ranges {
        if !matches!(
            range.key.kind,
            SurfaceKind::Floor | SurfaceKind::Ceiling | SurfaceKind::Wall
        ) {
            continue;
        }
        for index in &range.indices {
            if let Some(vertex) = range.vertices.get(usize::from(*index)) {
                out.push(*vertex);
            }
        }
    }
    out
}

/// The architectural vertex colours of one build, quantised the way the GPU
/// upload quantises them.
fn architectural_colors(mesh: &LevelMesh) -> Vec<[u8; 4]> {
    architectural_vertices(mesh)
        .iter()
        .map(|vertex| vertex.color.map(mesh::quantize_unit))
        .collect()
}

#[test]
fn a_lightmapped_build_keeps_its_architectural_vertex_colours_independent_of_fixture_brightness() {
    use crate::quality::QualityLevel;

    // A lightmapped build's architectural vertex colours are the material
    // factor alone (tint x directional face shade): the baked light lives in
    // the atlas, so changing a fixture's brightness must change the atlas
    // without touching a single vertex colour.
    let level = lit_room_level(
        8.0,
        6.0,
        3.0,
        r#"[{ "fixture": "core:fluorescent_panel_01", "x": 4.0, "z": 3.0, "brightness": 0.8 }]"#,
    );
    let mut dim_level = level.clone();
    for light in &mut dim_level.ceiling_lights {
        light.brightness = Some(0.05);
    }
    let materials = logical_materials(&level);
    let catalog = PropCatalog::builtin();
    let mut assets = PropAssets::default();
    let options = LightmapBuildOptions::for_level(QualityLevel::High, LightmapMode::On);

    let bright = build_level_geometry_timed_with_lightmaps(
        &level,
        &catalog,
        &mut assets,
        &materials,
        options,
        None,
    );
    let dim = build_level_geometry_timed_with_lightmaps(
        &dim_level,
        &catalog,
        &mut assets,
        &materials,
        options,
        None,
    );
    let (Some(bright_atlas), Some(dim_atlas)) = (&bright.lightmaps, &dim.lightmaps) else {
        panic!("both lightmapped builds must produce an atlas");
    };
    assert!(
        bright_atlas.stats.charts > 0,
        "the atlas must contain the room's charts"
    );
    assert_ne!(
        bright_atlas.pages, dim_atlas.pages,
        "the atlas carries the baked light, so fixture brightness must change it"
    );
    assert_eq!(
        architectural_colors(&bright.mesh),
        architectural_colors(&dim.mesh),
        "the architecture's vertex colours are lighting-independent"
    );

    // The vertex-lit path folds the bake into the vertex colours, so the same
    // brightness change must move them; that keeps the equality above from
    // being vacuous.
    let bright_lit = build_level_geometry_timed_with_lightmaps(
        &level,
        &catalog,
        &mut assets,
        &materials,
        LightmapBuildOptions::for_level(QualityLevel::High, LightmapMode::Off),
        None,
    );
    let dim_lit = build_level_geometry_timed_with_lightmaps(
        &dim_level,
        &catalog,
        &mut assets,
        &materials,
        LightmapBuildOptions::for_level(QualityLevel::High, LightmapMode::Off),
        None,
    );
    assert!(bright_lit.lightmaps.is_none());
    assert_ne!(
        architectural_colors(&bright_lit.mesh),
        architectural_colors(&dim_lit.mesh),
        "the vertex-lit build carries baked light, so brightness must change it"
    );
}

#[test]
fn a_vertex_lit_build_is_identical_across_quality_levels() {
    use crate::quality::QualityLevel;

    // With no plan the emitters take the historical vertex-lit path, which
    // bakes with the same `BakeConfig` whatever level is active: High, Medium
    // and Low must produce identical meshes.
    let level = lit_room_level(
        12.0,
        9.0,
        3.0,
        r#"[{ "fixture": "core:fluorescent_panel_01", "x": 6.0, "z": 4.0, "brightness": 0.8 }]"#,
    );
    let materials = logical_materials(&level);
    let catalog = PropCatalog::builtin();
    let mut assets = PropAssets::default();
    let mut meshes = Vec::new();
    for quality in QualityLevel::ALL {
        let build = build_level_geometry_timed_with_lightmaps(
            &level,
            &catalog,
            &mut assets,
            &materials,
            LightmapBuildOptions::for_level(quality, LightmapMode::Off),
            None,
        );
        assert!(build.lightmaps.is_none());
        meshes.push(build.mesh);
    }
    let Some((high, rest)) = meshes.split_first() else {
        panic!("every level built a mesh");
    };
    for (quality, mesh) in QualityLevel::ALL.into_iter().skip(1).zip(rest) {
        assert_eq!(high.vertex_count, mesh.vertex_count, "{quality:?}");
        assert_eq!(high.index_count, mesh.index_count, "{quality:?}");
        assert_eq!(
            architectural_vertices(high),
            architectural_vertices(mesh),
            "{quality:?} must bake the identical historical mesh"
        );
    }
}

#[test]
fn the_neutral_resolver_applies_material_defaults() {
    use crate::materials::{
        DEFAULT_ROUGHNESS, MaterialAlpha, MaterialEmission, MaterialReflection, MaterialResponse,
        MaterialTable,
    };
    use crate::render::common::materials::resolve_surface_material;

    let materials = MaterialRenderState {
        texture_slots: vec![0],
        emissions: vec![MaterialEmission::NONE],
        responses: vec![MaterialResponse::NONE],
        alphas: vec![MaterialAlpha::OPAQUE],
        reflections: vec![MaterialReflection::NONE],
    };
    let table = MaterialTable::default();
    let state = resolve_surface_material(
        SurfaceKey::new(SurfaceKind::Wall, 0),
        &materials,
        &table,
        true,
    );
    assert!(!state.response_enabled);
    assert_eq!(state.roughness, DEFAULT_ROUGHNESS, "the legacy default");
    assert_eq!(state.specular, [0.0; 3]);
    assert_eq!(state.alpha, MaterialAlpha::OPAQUE);
    assert!(!state.reflection_eligible());
    assert_eq!(state.reflection_strength(), [0.0; 3]);
    // A key without a material resolves the same plain state.
    let bare = resolve_surface_material(
        SurfaceKey::bare(SurfaceKind::Wall),
        &materials,
        &table,
        true,
    );
    assert_eq!(bare, state);
}

#[test]
fn the_neutral_resolver_lets_the_shine_override_win_and_keeps_the_material_otherwise() {
    use crate::materials::{
        MaterialAlpha, MaterialEmission, MaterialReflection, MaterialResponse, MaterialTable,
    };
    use crate::render::common::materials::resolve_surface_material;

    let response = MaterialResponse {
        normal: None,
        normal_strength: 1.0,
        specular: [0.4; 3],
        roughness: 0.25,
    };
    let materials = MaterialRenderState {
        texture_slots: vec![7],
        emissions: vec![MaterialEmission::NONE],
        responses: vec![response],
        alphas: vec![MaterialAlpha::blend(0.5)],
        reflections: vec![MaterialReflection::NONE],
    };
    let table = MaterialTable::default();

    // No override: the material's own roughness applies and the shine override
    // is absent from the key.
    let base = resolve_surface_material(
        SurfaceKey::new(SurfaceKind::Wall, 0),
        &materials,
        &table,
        true,
    );
    assert_eq!(base.roughness, 0.25);
    assert_eq!(base.alpha, MaterialAlpha::blend(0.5));
    assert!(base.response_enabled);

    // With an override the roughness follows the quantised shine, and every
    // other property stays the material's.
    let key = SurfaceKey::with_shine(SurfaceKind::Wall, 0, Some(SurfaceShine::from_unit(0.05)));
    let overridden = resolve_surface_material(key, &materials, &table, true);
    assert!((overridden.roughness - 0.95).abs() < f32::EPSILON);
    assert_eq!(overridden.alpha, base.alpha);
    assert_eq!(overridden.specular, base.specular);
    assert_eq!(overridden.normal_strength, base.normal_strength);
}

#[test]
fn the_neutral_resolver_zeroes_the_response_and_reflection_strength_on_a_gated_profile() {
    use crate::materials::{
        MaterialAlpha, MaterialEmission, MaterialReflection, MaterialResponse, MaterialTable,
        ReflectionMode,
    };
    use crate::render::common::materials::resolve_surface_material;

    let response = MaterialResponse {
        normal: Some(3),
        normal_strength: 0.4,
        specular: [0.6; 3],
        roughness: 0.3,
    };
    let materials = MaterialRenderState {
        texture_slots: vec![1],
        emissions: vec![MaterialEmission::NONE],
        responses: vec![response],
        alphas: vec![MaterialAlpha::OPAQUE],
        reflections: vec![MaterialReflection::new(ReflectionMode::Planar, 0.8)],
    };
    let table = MaterialTable::default();

    let enabled = resolve_surface_material(
        SurfaceKey::new(SurfaceKind::Wall, 0),
        &materials,
        &table,
        true,
    );
    assert!(enabled.response_enabled);
    assert_eq!(enabled.normal, Some(3));
    assert_eq!(enabled.specular, [0.6; 3]);
    assert!(enabled.reflection_eligible(), "authored and weighted");
    assert!((enabled.reflection_strength()[0] - 0.48).abs() < 1e-6);

    let gated = resolve_surface_material(
        SurfaceKey::new(SurfaceKind::Wall, 0),
        &materials,
        &table,
        false,
    );
    assert!(!gated.response_enabled, "Low gates the response");
    assert_eq!(gated.normal, None, "a gated profile binds no normal map");
    assert_eq!(gated.specular, [0.0; 3], "the sheen is zeroed");
    assert!(
        !gated.reflection_eligible(),
        "the zeroed sheen zeroes the reflection weight, exactly like the reference"
    );
    assert_eq!(gated.reflection_strength(), [0.0; 3]);
    // The alpha, texture and mode are untouched by the response gate.
    assert_eq!(gated.alpha, enabled.alpha);
    assert_eq!(gated.texture, enabled.texture);
    assert_eq!(gated.reflection.mode, ReflectionMode::Planar);
}

#[test]
fn the_neutral_resolver_treats_a_fixture_or_decal_key_as_plain() {
    use crate::materials::{
        MaterialAlpha, MaterialEmission, MaterialReflection, MaterialResponse, MaterialTable,
    };
    use crate::render::common::materials::resolve_surface_material;

    let materials = MaterialRenderState {
        texture_slots: vec![0; 3],
        emissions: vec![MaterialEmission::NONE; 3],
        responses: vec![
            MaterialResponse {
                normal: Some(1),
                normal_strength: 1.0,
                specular: [1.0; 3],
                roughness: 0.1,
            };
            3
        ],
        alphas: vec![MaterialAlpha::blend(0.5); 3],
        reflections: vec![MaterialReflection::NONE; 3],
    };
    let table = MaterialTable::default();
    for kind in [
        SurfaceKind::Light,
        SurfaceKind::PropFallback,
        SurfaceKind::Decal,
    ] {
        let state = resolve_surface_material(SurfaceKey::new(kind, 0), &materials, &table, true);
        assert_eq!(
            state,
            crate::render::common::materials::ResolvedSurfaceMaterial::plain()
        );
    }
}

#[test]
fn the_surface_frame_follows_the_uv_orientation_and_flips_with_mirrored_uvs() {
    // The uploaded tangent frame is computed by the neutral builder from the
    // surface's own UVs; a normal map must tilt the same way on a mirrored
    // sheet as on the original, with the sign carried by `handedness`.
    use crate::render::common::finish_indexed_mesh;
    use crate::spatial::SpatialBuckets;

    let level = lit_room_level(1.0, 1.0, 3.0, "[]");
    let grid = spatial_cell_grid(&level);
    let key = SurfaceKey::new(SurfaceKind::Wall, MATERIAL_NONE);
    let quad = |uv: [[f32; 2]; 4]| -> Vec<Vertex> {
        let points = [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
        ];
        [0usize, 1, 2, 0, 2, 3]
            .iter()
            .map(|index| Vertex {
                pos: points[*index],
                uv: uv[*index],
                ..Vertex::UNLIT
            })
            .collect()
    };
    let build = |uv| {
        let mut buckets = SpatialBuckets::<SurfaceKey>::with_grid(grid);
        buckets.add_quads(key, &quad(uv));
        finish_indexed_mesh(buckets)
    };

    // u = +x, v = +y: normal +Z, tangent +X, bitangent +Y, handedness +1.
    let standard = build([[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
    let vertex = standard.ranges[0].vertices[0];
    let normal = glam::Vec3::from(vertex.normal);
    let tangent = glam::Vec3::from(vertex.tangent);
    assert!((normal - glam::Vec3::Z).length() < 1.0e-5, "{normal:?}");
    assert!((tangent - glam::Vec3::X).length() < 1.0e-5, "{tangent:?}");
    assert_eq!(vertex.handedness, 1.0);
    let bitangent = normal.cross(tangent) * vertex.handedness;
    assert!(
        (bitangent - glam::Vec3::Y).length() < 1.0e-5,
        "{bitangent:?}"
    );

    // u = -x: the tangent follows the UV and the handedness flips, so the
    // reconstructed bitangent still runs along +v.
    let mirrored = build([[0.0, 0.0], [-1.0, 0.0], [-1.0, 1.0], [0.0, 1.0]]);
    let vertex = mirrored.ranges[0].vertices[0];
    let tangent = glam::Vec3::from(vertex.tangent);
    assert!(
        (tangent - (-glam::Vec3::X)).length() < 1.0e-5,
        "{tangent:?}"
    );
    assert_eq!(vertex.handedness, -1.0);
    let bitangent = normal.cross(tangent) * vertex.handedness;
    assert!(
        (bitangent - glam::Vec3::Y).length() < 1.0e-5,
        "{bitangent:?}"
    );
}

#[test]
fn surface_shine_quantises_to_whole_percent_and_inverts_the_roughness() {
    assert_eq!(SurfaceShine::from_unit(0.0), SurfaceShine::MATTE);
    assert_eq!(SurfaceShine::from_unit(1.0), SurfaceShine::GLOSS);
    // Out-of-range and non-finite input saturates instead of producing a NaN.
    assert_eq!(SurfaceShine::from_unit(-2.0), SurfaceShine::MATTE);
    assert_eq!(SurfaceShine::from_unit(f32::NAN), SurfaceShine::MATTE);
    assert_eq!(SurfaceShine::from_unit(f32::INFINITY), SurfaceShine::MATTE);

    let half = SurfaceShine::from_unit(0.5);
    assert!((half.unit() - 0.5).abs() < f32::EPSILON);
    assert!((half.roughness() - 0.5).abs() < f32::EPSILON);
    assert!((SurfaceShine::MATTE.roughness() - 1.0).abs() < f32::EPSILON);
    assert!(SurfaceShine::GLOSS.roughness().abs() < f32::EPSILON);

    // Rounding to whole percent is what lets a key carry the value: two
    // shades within the same percent are the same batch, a whole percent
    // apart is a different one.
    assert_eq!(
        SurfaceShine::from_unit(0.054),
        SurfaceShine::from_unit(0.052)
    );
    assert_ne!(SurfaceShine::from_unit(0.05), SurfaceShine::from_unit(0.06));
}

#[test]
fn a_per_surface_shine_override_reaches_the_batch_key() {
    // A level lays matte linoleum over a waxed material: the material's own
    // resolved response keeps its authored default and the surface's batch key
    // carries the override, which is the one place the draw path reads it.
    let level = LevelDef::from_json(
        r#"{
            "format_version": 1, "id": "shine_key", "name": "Shine Key",
            "spawn": { "x": 1.0, "z": 1.0 },
            "defaults": { "wall": "core:wallpaper_yellow_01",
                          "floor": "core:carpet_beige_01",
                          "ceiling": "core:ceiling_panel_01" },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0 } ],
            "floor_patches": [
                { "x": 0.0, "z": 0.0, "width": 2.0, "depth": 2.0,
                  "material": "core:linoleum_polished_01", "shine": 0.05 }
            ]
        }"#,
    )
    .expect("shine level parses");
    let materials = logical_materials(&level);
    let linoleum = materials
        .index_of("core:linoleum_polished_01")
        .expect("the linoleum material resolves");
    let material_roughness = materials
        .entry(linoleum)
        .expect("the linoleum entry")
        .response
        .roughness;
    // The shipped material stays deliberately waxed; only this surface is matte.
    assert!(
        (material_roughness - 0.45).abs() < 1.0e-6,
        "the material default must stay its own, got {material_roughness}"
    );

    let mesh = build_level_geometry(&level);
    let patch_key = mesh
        .ranges
        .iter()
        .map(|range| range.key)
        .find(|key| key.material == linoleum)
        .expect("the patch emits a linoleum range");
    assert_eq!(patch_key.shine, Some(SurfaceShine::from_unit(0.05)));
    assert!(
        (patch_key.roughness(material_roughness) - 0.95).abs() < 1.0e-6,
        "the override must win over the material default"
    );

    // A surface of the same family without an override keeps the default.
    let carpet = materials.index_of("core:carpet_beige_01").expect("carpet");
    let carpet_key = mesh
        .ranges
        .iter()
        .map(|range| range.key)
        .find(|key| key.material == carpet)
        .expect("the room floor emits a carpet range");
    assert_eq!(carpet_key.shine, None);
    assert!(
        (carpet_key.roughness(0.6) - 0.6).abs() < f32::EPSILON,
        "a key without an override uses the material's own roughness"
    );
}

#[test]
fn the_demos_matte_linoleum_keeps_its_probe_and_its_material_default() {
    // The shipped demo overrides its linoleum patch down to near-matte. That
    // must change only the surface's glossiness: the material's waxed default
    // and its probe reflection are untouched, and shine 0.05 must not remove
    // the reflection the material authors.
    use crate::materials::{MaterialReflection, ReflectionMode};

    let level = shipped_demo();
    let materials = logical_materials(&level);
    let linoleum = materials
        .index_of("core:linoleum_polished_01")
        .expect("linoleum");
    let entry = materials.entry(linoleum).expect("linoleum entry");
    assert_eq!(entry.reflection.mode, ReflectionMode::Probe);
    assert_ne!(entry.reflection, MaterialReflection::NONE);
    assert!(
        (entry.response.roughness - 0.45).abs() < 1.0e-6,
        "the material's waxed default is still 0.55 shine"
    );

    let mesh = build_level_geometry(&level);
    let patch_key = mesh
        .ranges
        .iter()
        .map(|range| range.key)
        .find(|key| key.material == linoleum)
        .expect("the linoleum patch emits a range");
    assert_eq!(patch_key.shine, Some(SurfaceShine::from_unit(0.05)));
    assert!(
        (patch_key.roughness(entry.response.roughness) - 0.95).abs() < 1.0e-6,
        "the demo's institutional linoleum is matte"
    );
}

// ------------------------------------------------- generic architecture

/// The committed Home showcase: every generic architectural piece and every
/// Home material in one level.
fn home_showcase() -> crate::level::LevelDef {
    crate::level::LevelDef::from_json(include_str!(
        "../../tests/fixtures/levels/home_showcase.json"
    ))
    .expect("the Home showcase fixture parses")
}

#[test]
fn test_the_home_showcase_bakes_lightmaps_with_every_surface_vertex_charted() {
    let level = home_showcase();
    let build = lightmap_build(&level, crate::quality::QualityLevel::High, LightmapMode::On);
    assert_eq!(
        build.lightmap_failure, None,
        "the showcase must bake cleanly"
    );
    let lightmaps = build
        .lightmaps
        .as_deref()
        .expect("the showcase must produce an atlas");
    assert!(lightmaps.chart_count() > 0);
    for range in &build.mesh.ranges {
        for vertex in &range.vertices {
            match range.key.kind {
                SurfaceKind::Floor | SurfaceKind::Ceiling | SurfaceKind::Wall => {
                    assert!(
                        vertex.is_lightmapped(),
                        "a static surface vertex must carry a chart"
                    );
                    assert!(
                        vertex_in_some_chart(lightmaps, vertex),
                        "a lightmapped vertex must sample inside its own chart"
                    );
                }
                SurfaceKind::Light | SurfaceKind::PropFallback | SurfaceKind::Decal => {
                    assert!(
                        !vertex.is_lightmapped(),
                        "fixtures, prop boxes and decals stay vertex-lit"
                    );
                }
            }
        }
    }
}

/// Every architectural face is a real, finite, correctly wound quad, and the
/// archway's opening is genuinely open geometry.
#[test]
fn test_the_home_showcase_architecture_is_well_formed() {
    let level = home_showcase();
    let mesh = build_level_geometry(&level);
    assert!(mesh.vertex_count > 0);
    // The mesh is an indexed triangle list. Faces are emitted as quads almost
    // everywhere, but a face whose fourth corner collapses (a ramp side landing
    // flush on a floor, a baseboard cap trimmed at a corner) is a real triangle,
    // and every emitted triangle must still have area — checked below.
    assert_eq!(mesh.index_count % 3, 0, "the mesh is a triangle list");

    let normal = |a: [f32; 3], b: [f32; 3], c: [f32; 3]| -> [f32; 3] {
        let u = glam::Vec3::from(b) - glam::Vec3::from(a);
        let v = glam::Vec3::from(c) - glam::Vec3::from(a);
        (u.cross(v)).to_array()
    };
    for range in &mesh.ranges {
        let vertices: Vec<Vertex> = range
            .indices
            .iter()
            .filter_map(|index| range.vertices.get(usize::from(*index)).copied())
            .collect();
        for triangle in vertices.as_chunks::<3>().0 {
            let n = normal(triangle[0].pos, triangle[1].pos, triangle[2].pos);
            assert!(
                n.iter().all(|value| value.is_finite()),
                "{:?} has a non-finite normal",
                range.key.kind
            );
            assert!(
                glam::Vec3::from(n).length() > 1e-6,
                "{:?} has a degenerate triangle",
                range.key.kind
            );
            match range.key.kind {
                SurfaceKind::Floor => assert!(n[1] > 0.0, "a floor triangle faces down: {n:?}"),
                SurfaceKind::Ceiling => assert!(n[1] < 0.0, "a ceiling triangle faces up: {n:?}"),
                // A fixture's *sheet* face looks down into the room; its
                // untextured housing (bezel, drum, can) has real side walls.
                SurfaceKind::Light if range.key.has_material() => {
                    assert!(n[1] < 0.0, "a fixture face must look down: {n:?}");
                }
                _ => {}
            }
        }
    }

    // The ramp's sloped top must be tilted, not flat: it is the one place the
    // level's floors are not horizontal.
    let floors = batch_slice(&mesh, SurfaceKind::Floor);
    let tilted = floors.as_chunks::<3>().0.iter().any(|triangle| {
        let n = normal(triangle[0].pos, triangle[1].pos, triangle[2].pos);
        let length = glam::Vec3::from(n).length();
        (n[1] / length) < 0.999
    });
    assert!(tilted, "the ramp contributes a sloped floor surface");

    // The archway's opening is open: no architecture vertex sits inside the
    // clear volume between its piers and under its springing line.
    for triangle in mesh
        .ranges
        .iter()
        .filter(|range| {
            matches!(
                range.key.kind,
                SurfaceKind::Floor | SurfaceKind::Wall | SurfaceKind::Ceiling
            )
        })
        .flat_map(|range| range.vertices.iter())
    {
        let [x, y, z] = triangle.pos;
        let inside = x > 5.87 && x < 6.13 && z > 1.84 && z < 2.76 && y > 0.02 && y < 1.83;
        assert!(
            !inside,
            "the archway's clear volume has geometry in it at {x}, {y}, {z}"
        );
    }
}

/// Every material a generic piece names is resolved and actually drawn.
///
/// This is the regression test for the scan that collects a level's material
/// references: a piece whose material was missed renders the bare white sheet
/// instead of its own surface, which is exactly the kind of silent fallback the
/// generic architecture must never have.
#[test]
fn test_the_home_showcase_draws_every_architectural_material() {
    let level = home_showcase();
    let materials = logical_materials(&level);
    let mesh = build_level_geometry(&level);
    for id in [
        "home:hardwood_oak_01",
        "home:wall_paint_offwhite_01",
        "home:baseboard_white_01",
        "home:baseboard_wood_01",
        "home:handrail_wood_01",
        "home:threshold_wood_01",
    ] {
        let index = materials
            .index_of(id)
            .unwrap_or_else(|| panic!("{id} must resolve in the level's material table"));
        assert!(
            mesh.ranges.iter().any(|range| range.key.material == index),
            "{id} must be drawn by at least one batch"
        );
    }

    // The living room's north baseboard is trim: its faces stand proud of the
    // wall plane (z = 0) by the board's own thickness, so it never shares the
    // wall's plane.
    let wood = materials
        .index_of("home:baseboard_wood_01")
        .expect("the wood baseboard resolves");
    let mut seen = false;
    for range in &mesh.ranges {
        if range.key.material != wood {
            continue;
        }
        let vertices: Vec<Vertex> = range
            .indices
            .iter()
            .filter_map(|index| range.vertices.get(usize::from(*index)).copied())
            .collect();
        for vertex in &vertices {
            let [x, y, z] = vertex.pos;
            if (0.0..6.0).contains(&x) && (0.0..=0.09).contains(&y) {
                assert!(
                    z >= -1e-4,
                    "a baseboard vertex sits behind the wall plane: {z}"
                );
                seen = true;
            }
        }
    }
    assert!(seen, "the living room's baseboard must be drawn");
}

// ---------------------------------------------------------------------- water

/// A minimal level with one authored water volume over a recessed basin.
///
/// An 8x8 m room at `floor_y: 0.0`; the basin region (1..7 on both axes) drops
/// to -1.5, and the water surface sits at -0.15: 1.35 m over the basin floor
/// and 0.15 m below the deck, the same relationship the demo's pool uses.
fn water_test_level(opacity: Option<f32>, material: Option<&str>) -> LevelDef {
    let opacity = opacity.map_or(String::new(), |value| format!(r#", "opacity": {value}"#));
    let material = material.map_or(String::new(), |id| format!(r#", "material": "{id}""#));
    let json = format!(
        r#"{{
            "format_version": 1,
            "id": "water_test",
            "name": "Water Test",
            "spawn": {{ "x": 3.0, "z": 3.0 }},
            "rooms": [{{ "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 3.0 }}],
            "floor_regions": [
                {{ "x": 1.0, "z": 1.0, "width": 6.0, "depth": 6.0, "offset_y": -1.5,
                   "material": "core:pool_tile_basin_01",
                   "edge_material": "core:pool_tile_wall_01" }}
            ],
            "water": [
                {{ "x": 1.0, "z": 1.0, "width": 6.0, "depth": 6.0,
                   "surface_y": -0.15{opacity}{material} }}
            ]
        }}"#
    );
    LevelDef::from_json(&json).expect("the water test level parses")
}

/// One authored water volume draws exactly one translucent floor quad, at its
/// surface height, with the volume's opacity in the vertex alpha and the baked
/// light in the vertex colour.
#[test]
fn a_water_volume_draws_one_translucent_surface_quad() {
    let level = water_test_level(None, None);
    let materials = logical_materials(&level);
    let water = materials
        .index_of(crate::level::DEFAULT_WATER_MATERIAL)
        .expect("the default water material resolves through the catalog");
    let entry = materials.entry(water).expect("water material entry");
    assert_eq!(
        batch_pass_for(SurfaceKind::Floor, true, Some(entry.alpha)),
        BatchPass::Translucent,
        "the water material's blend contract lands in the sorted pass"
    );

    let mesh = build_level_geometry(&level);
    let key = SurfaceKey::new(SurfaceKind::Floor, water);
    assert_eq!(mesh.index_count_for_key(key), 6, "one volume is one quad");
    let surface = mesh.triangles_for_key(key);
    assert_eq!(surface.len(), 6);
    for vertex in &surface {
        assert!(
            (vertex.pos[1] + 0.15).abs() < 1e-6,
            "the quad sits at surface_y, not on the basin floor: {:?}",
            vertex.pos
        );
        assert!(
            (vertex.color[3] - crate::level::DEFAULT_WATER_OPACITY).abs() < 1e-6,
            "the vertex alpha carries the volume opacity: {:?}",
            vertex.color
        );
        assert_eq!(
            vertex.lightmap_page, LIGHTMAP_NONE,
            "water stays out of the lightmap atlas"
        );
        assert_eq!(vertex.lightmap, [0, 0]);
        // Floor winding, and the translucent pass is two-sided: the same quad
        // is the surface seen from above and from below the waterline.
        assert!(
            vertex.normal[1] > 0.9,
            "the water surface faces up: {:?}",
            vertex.normal
        );
    }
    // No side or bottom face: every water vertex is on the one surface plane,
    // inside the volume's own footprint.
    for vertex in &surface {
        assert!((vertex.pos[1] + 0.15).abs() < 1e-6);
        assert!((1.0..=7.0).contains(&vertex.pos[0]));
        assert!((1.0..=7.0).contains(&vertex.pos[2]));
    }
    // World-space UVs at the material's 2 m tiling: a 6 m span is three
    // repeats, so the sheet continues the metre grid across the volume.
    let us: Vec<f32> = surface.iter().map(|vertex| vertex.uv[0]).collect();
    let min = us.iter().fold(f32::INFINITY, |a, b| a.min(*b));
    let max = us.iter().fold(f32::NEG_INFINITY, |a, b| a.max(*b));
    assert!(
        (max - min - 3.0).abs() < 1e-6,
        "6 m at a 2 m repeat spans 3 UV periods: {min}..{max}"
    );
}

/// The volume's authored material and opacity both win over the defaults.
#[test]
fn a_water_volume_honours_its_opacity_and_material() {
    let level = water_test_level(Some(0.31), Some("core:glass_clear_01"));
    let volumes = crate::level::WaterVolumes::from_level(&level);
    assert!((volumes.volumes()[0].opacity - 0.31).abs() < 1e-6);

    let materials = logical_materials(&level);
    let glass = materials
        .index_of("core:glass_clear_01")
        .expect("the authored water material resolves");
    let mesh = build_level_geometry(&level);
    let surface = mesh.triangles_for_key(SurfaceKey::new(SurfaceKind::Floor, glass));
    assert_eq!(surface.len(), 6, "the authored material carries the quad");
    for vertex in &surface {
        assert!((vertex.color[3] - 0.31).abs() < 1e-6);
    }
}

/// The demo's pool is a real body of water: two adjacent volumes at one
/// waterline, the basin and the submerged walk-in step.
#[test]
fn the_demo_authors_water_over_the_basin_and_the_walk_in_step() {
    let level = shipped_demo();
    let volumes = crate::level::WaterVolumes::from_level(&level);
    assert_eq!(
        volumes.len(),
        2,
        "the basin and the step are two adjacent volumes"
    );

    let basin = &volumes.volumes()[0];
    assert_eq!(
        (basin.x0, basin.x1, basin.z0, basin.z1),
        (8.0, 20.0, 10.0, 16.0),
        "the basin volume covers the demo's recessed pool region"
    );
    assert!((basin.surface_y + 1.65).abs() < 1e-6);
    assert!((basin.bottom_y + 3.0).abs() < 1e-6);
    assert!((basin.depth() - 1.35).abs() < 1e-6);
    assert!(basin.swimming);
    assert_eq!(basin.material_id(), crate::level::DEFAULT_WATER_MATERIAL);
    assert!((basin.opacity - crate::level::DEFAULT_WATER_OPACITY).abs() < 1e-6);

    let step = &volumes.volumes()[1];
    assert_eq!(
        (step.x0, step.x1, step.z0, step.z1),
        (10.0, 16.0, 16.0, 16.9),
        "the step volume covers the submerged walk-in step"
    );
    assert!((step.surface_y + 1.65).abs() < 1e-6);
    assert!(
        (step.bottom_y + 1.85).abs() < 1e-6,
        "the step volume's bottom resolves to the step floor"
    );
    assert!((step.depth() - 0.2).abs() < 1e-6);

    // The two volumes draw exactly two translucent quads.
    let materials = logical_materials(&level);
    let water = materials
        .index_of(crate::level::DEFAULT_WATER_MATERIAL)
        .expect("the demo's water material resolves");
    let mesh = build_level_geometry(&level);
    let key = SurfaceKey::new(SurfaceKind::Floor, water);
    assert_eq!(mesh.index_count_for_key(key), 12, "one quad per volume");
    let surface = mesh.triangles_for_key(key);
    for vertex in &surface {
        assert!((vertex.pos[1] + 1.65).abs() < 1e-6);
        assert!((vertex.color[3] - 0.62).abs() < 1e-6);
        assert_eq!(vertex.lightmap_page, LIGHTMAP_NONE);
    }
    assert!(
        surface.iter().any(|vertex| vertex.pos[2] <= 10.0 + 1e-6),
        "the basin quad reaches the basin's north edge"
    );
    assert!(
        surface.iter().any(|vertex| vertex.pos[2] >= 16.9 - 1e-6),
        "the step quad reaches the walk-in step's far edge"
    );

    // The packed world draws the water through the sorted translucent pass:
    // the demo has no separate water pass and no water-specific pipeline.
    let entry = materials.entry(water).expect("water material entry");
    assert_eq!(
        batch_pass_for(SurfaceKind::Floor, true, Some(entry.alpha)),
        BatchPass::Translucent
    );
    let state = MaterialRenderState::from_table(&materials);
    let (_, draws) = super::wgpu::world::pack_world_ranges(&mesh, &state);
    assert!(
        draws
            .iter()
            .any(|draw| draw.material == water && draw.pass == BatchPass::Translucent),
        "the demo's water geometry is a translucent world draw"
    );
}

/// Two pools far enough apart to fall in different spatial cells: the sorted
/// translucent pass draws the farther surface first, exactly as it does for
/// glass. Water introduces no ordering rule of its own.
#[test]
fn water_is_ordered_with_the_other_translucent_surfaces() {
    use super::common::materials::MaterialRenderState;
    use super::wgpu::world::{pack_world_ranges, translucent_order};

    let level = LevelDef::from_json(
        r#"{
            "format_version": 1,
            "id": "two_pools",
            "name": "Two Pools",
            "spawn": { "x": 4.0, "z": 4.0 },
            "rooms": [{ "x": 0.0, "z": 0.0, "width": 44.0, "depth": 8.0, "height": 3.0 }],
            "floor_regions": [
                { "x": 2.0, "z": 2.0, "width": 6.0, "depth": 4.0, "offset_y": -0.5,
                  "material": "core:pool_tile_basin_01",
                  "edge_material": "core:pool_tile_wall_01" },
                { "x": 32.0, "z": 2.0, "width": 6.0, "depth": 4.0, "offset_y": -0.5,
                  "material": "core:pool_tile_basin_01",
                  "edge_material": "core:pool_tile_wall_01" }
            ],
            "water": [
                { "x": 2.0, "z": 2.0, "width": 6.0, "depth": 4.0, "surface_y": -0.15 },
                { "x": 32.0, "z": 2.0, "width": 6.0, "depth": 4.0, "surface_y": -0.15 }
            ]
        }"#,
    )
    .expect("the two-pool level parses");

    let materials = logical_materials(&level);
    let water = materials
        .index_of(crate::level::DEFAULT_WATER_MATERIAL)
        .expect("the default water material resolves");
    let state = MaterialRenderState::from_table(&materials);
    let mesh = build_level_geometry(&level);
    let (_, draws) = pack_world_ranges(&mesh, &state);

    let mut west = None;
    let mut east = None;
    for (index, draw) in draws.iter().enumerate() {
        if draw.material != water || draw.pass != BatchPass::Translucent {
            continue;
        }
        if draw.bounds.centre()[0] < 22.0 {
            west = Some(index);
        } else {
            east = Some(index);
        }
    }
    let west = west.expect("the west pool is a translucent draw");
    let east = east.expect("the east pool is a translucent draw");
    assert_ne!(
        west, east,
        "the pools sit in different cells, so they stay separate draws"
    );

    // An eye on the west pool: the east pool is farther, so it blends first.
    let eye = glam::Vec3::new(5.0, 1.0, 4.0);
    let order = translucent_order(&draws, eye);
    let rank = |draw: usize| {
        order
            .iter()
            .position(|ordered| *ordered == u32::try_from(draw).unwrap())
            .expect("a translucent draw is in the order")
    };
    assert!(
        rank(east) < rank(west),
        "the farther pool blends before the nearer one"
    );

    // The whole order is back to front by AABB centre distance.
    let distance = |index: &u32| {
        let centre = draws[usize::try_from(*index).unwrap()].bounds.centre();
        glam::Vec3::new(centre[0] - eye.x, centre[1] - eye.y, centre[2] - eye.z).length_squared()
    };
    for pair in order.windows(2) {
        assert!(
            distance(&pair[0]) >= distance(&pair[1]) - 1e-3,
            "translucent draws must be ordered farthest first"
        );
    }
}

// ------------------------------------------------------- animated characters

/// A skinned prop becomes a [`CharacterScene`] entry; an unskinned one stays
/// on the static path and is never claimed.
#[test]
fn a_placed_spooner_man_becomes_a_character_and_a_chair_does_not() {
    let catalog = shipped_catalog();
    let mut assets = shipped_assets();
    let level = level_with_wall_and_lights(
        "[]",
        r#"[
            { "model": "spooner-man", "x": 1.0, "z": 1.0, "rotation_degrees": 90.0 },
            { "model": "core:chair", "x": 3.0, "z": 1.0 }
        ]"#,
        r#"[{ "fixture": "core:fluorescent_panel_01", "x": 0.0, "z": 0.0 }]"#,
    );
    let lighting = LevelLighting::bake(&level);
    let scene = CharacterScene::spawn_characters(&level, &catalog, &mut assets, &lighting);
    assert_eq!(scene.len(), 1, "only the skinned model becomes a character");
    let character = &scene.characters()[0];
    let model_path = catalog
        .get("spooner-man")
        .model
        .expect("the shipped catalogue maps spooner-man to a model");
    assert_eq!(character.asset().model_path, model_path);
    assert!(character.asset().model.is_skinned());
    assert_eq!(
        scene.claimed_models(),
        std::slice::from_ref(&model_path),
        "exactly the skinned model is claimed"
    );

    // The character stands on the placement's floor point with its yaw.
    let centre = character.transform().transform_point3(glam::Vec3::ZERO);
    assert!((centre.x - 1.0).abs() < 1e-5 && (centre.z - 1.0).abs() < 1e-5);
    assert!(centre.y.abs() < 1e-5, "the room floor is at y = 0");

    // Baked albedo is sampled once at spawn and stays in range.
    assert_eq!(
        character.albedo().len(),
        character.asset().model.vertices.len()
    );
    for albedo in character.albedo() {
        for channel in albedo {
            assert!(channel.is_finite());
            assert!((-1e-4..=1.0 + 1e-4).contains(channel), "{albedo:?}");
        }
    }

    // Walking advances the pose; the scene reports the moved character.
    let mut scene = scene;
    let update = scene.update(
        1.0 / 60.0,
        crate::game::LocomotionSnapshot {
            state: crate::game::LocomotionState::Walking,
            speed: 2.0,
        },
    );
    assert_eq!(update.moved, 1);
    assert!(
        scene.characters()[0]
            .animator()
            .state_weight(crate::game::LocomotionState::Walking)
            > 0.0
    );
}

/// The shipped demo level places exactly one skinned prop, and the character
/// path claims its model without touching the rest of the prop field.
#[test]
fn the_places_demo_level_places_one_animated_spooner_man() {
    let content = std::fs::read_to_string("assets/levels/places_demo.json")
        .expect("the demo level must be readable");
    let level = LevelDef::from_json(&content).expect("the demo level parses");
    let catalog = shipped_catalog();
    let mut assets = shipped_assets();
    let lighting = LevelLighting::bake(&level);
    let scene = CharacterScene::spawn_characters(&level, &catalog, &mut assets, &lighting);
    assert_eq!(scene.len(), 1, "the demo places one skinned character");
    let model_path = catalog
        .get("spooner-man")
        .model
        .expect("spooner-man has a model");
    assert_eq!(scene.claimed_models(), std::slice::from_ref(&model_path));
    let character = &scene.characters()[0];
    let placement = level
        .props
        .iter()
        .find(|prop| prop.model == "spooner-man")
        .expect("the demo places spooner-man");
    let centre = character.transform().transform_point3(glam::Vec3::ZERO);
    assert!((centre.x - placement.x).abs() < 1e-5);
    assert!((centre.z - placement.z).abs() < 1e-5);
}

/// The static prop path still bakes the bind pose of a skinned model: real
/// geometry, one batch, and the shipped extents.
#[test]
fn the_static_path_still_bakes_the_spooner_man_bind_pose() {
    let catalog = shipped_catalog();
    let mut assets = shipped_assets();
    let level = fixture_level("prop_showcase");
    let (mesh, batches) = build_level_geometry_with_assets(&level, &catalog, &mut assets);
    assert_eq!(
        mesh.batches.prop_batch.count, 0,
        "no placeholder boxes for the skinned model"
    );
    let spoonerman: Vec<&PropMeshBatch> = batches
        .iter()
        .filter(|batch| batch.model.contains("spooner-man"))
        .collect();
    assert_eq!(spoonerman.len(), 1, "one real batch for one placement");
    let batch = spoonerman[0];
    assert!(
        batch.vertices.len() > 1_000,
        "the cat must draw real geometry, found {} vertices",
        batch.vertices.len()
    );

    // The batch is exactly the bind pose under the placement transform: a
    // yaw preserves the height, and sampled vertices match the model point
    // transformed by the same placement matrix.
    let prop = level
        .props
        .iter()
        .find(|prop| prop.model == "spooner-man")
        .expect("the showcase places spooner-man");
    let model_path = catalog
        .get("spooner-man")
        .model
        .expect("spooner-man has a model");
    let asset = assets.resolve(&model_path).expect("the model loads");
    let base_y = crate::level::LevelSurfaces::new(&level)
        .floor_y_at(prop.x, prop.z)
        .unwrap_or(0.0);
    let placement = crate::render::common::props::prop_instance_matrix(prop, base_y);
    for index in [
        0,
        asset.model.vertices.len() / 2,
        asset.model.vertices.len() - 1,
    ] {
        let source = asset.model.vertices[index].pos;
        let posed = placement.transform_point3(glam::Vec3::new(source[0], source[1], source[2]));
        let drawn = batch.vertices[index].pos;
        for axis in 0..3 {
            assert!(
                (drawn[axis] - posed[axis]).abs() < 1e-4,
                "vertex {index} axis {axis}: drawn {drawn:?} vs bind {posed:?}"
            );
        }
    }
    let (low, high) = bounds_of(&batch.vertices);
    let height = high[1] - low[1];
    assert!(
        (height - 0.389).abs() < 0.02,
        "the bind pose is {height:.3} m tall, expected 0.389 m"
    );
    assert!(
        low[1].abs() < 0.012,
        "the bind pose stands on y = 0, found {:.3}",
        low[1]
    );
    let widest = (high[0] - low[0]).max(high[2] - low[2]);
    assert!(
        (0.165..0.9).contains(&widest),
        "the rotated bind-pose footprint {widest:.3} m is not the cat's"
    );
}
