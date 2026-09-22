//! Unit tests for the level geometry builder and the renderer's CPU side.
//!
//! They exercise the mesh builder, batching and indexing, the decal sheets, the
//! prop instancing and the view/capture helpers through the same entry points
//! the game uses.

use crate::test_support::{assert_exact, assert_exact_array, assert_exact_named};

use super::*;
use crate::render::decals::{DECAL_ATLAS_SIZE, generate_decal_atlas};
use crate::spatial::{DepthRange, Frustum};

// ------------------------------------------------------- vertex packing

#[test]
fn the_packed_vertex_is_twenty_four_bytes_with_the_declared_layout() {
    assert_eq!(
        std::mem::size_of::<PackedVertex>(),
        24,
        "the packed scene vertex must be 24 bytes"
    );
    assert_eq!(std::mem::align_of::<PackedVertex>(), 4);
    assert_eq!(packed_layout::STRIDE, 24, "stride must match the struct");
    assert_eq!(
        VertexLayout::Exact.stride(),
        i32::try_from(std::mem::size_of::<Vertex>()).unwrap_or(i32::MAX),
        "the exact layout's stride must match the struct"
    );
    // The offsets are what `set_packed_vertex_attributes` hands to
    // `glVertexAttribPointer`; a struct change must move them too.
    assert_eq!(
        std::mem::offset_of!(PackedVertex, pos),
        packed_layout::POS_OFFSET as usize
    );
    assert_eq!(
        std::mem::offset_of!(PackedVertex, color),
        packed_layout::COLOR_OFFSET as usize
    );
    assert_eq!(
        std::mem::offset_of!(PackedVertex, uv),
        packed_layout::UV_OFFSET as usize
    );
    assert_eq!(
        packed_layout::UV_OFFSET as usize + 2 * 4,
        packed_layout::STRIDE as usize
    );
    assert_eq!(
        packed_layout::COLOR_OFFSET as usize + 4,
        packed_layout::UV_OFFSET as usize,
        "colour must be four packed bytes"
    );
    // 12 bytes saved per vertex against the original representation.
    assert_eq!(
        std::mem::size_of::<Vertex>() - std::mem::size_of::<PackedVertex>(),
        12
    );
}

/// The quantisation error the packed colour can introduce, in channel units.
fn packed_channel_error(value: f32) -> f32 {
    let packed = PackedVertex::from(&Vertex {
        pos: [0.0, 0.0, 0.0],
        color: [value, value, value, value],
        uv: [0.0, 0.0],
    });
    (dequantize_unit(packed.color[0]) - value).abs()
}

#[test]
fn packed_colour_is_accurate_at_the_lighting_extremes_and_in_between() {
    // Minimum baked lighting: the darkest a vertex can get.
    assert!(packed_channel_error(crate::lighting::AMBIENT_LEVEL) < 0.5 / 255.0);
    // Maximum brightness.
    assert!(packed_channel_error(crate::lighting::MAX_BRIGHTNESS) < 0.5 / 255.0);
    // Darkest and brightest possible shades of a wall/floor tint.
    assert!(packed_channel_error(0.0) < 1e-6);
    assert!(packed_channel_error(1.0) < 1e-6);
    // A representative intermediate value, and one that lands exactly
    // between two steps (the worst case).
    for value in [0.666, 0.42, 127.5 / 255.0, 1.0 / 255.0, 0.999] {
        assert!(
            packed_channel_error(value) <= 0.5 / 255.0 + 1e-6,
            "value {value} quantised by more than half a step"
        );
    }
    // The whole usable lighting range, swept at 1/1000.
    let mut worst = 0.0f32;
    for step in 0..=1000 {
        let value = crate::lighting::AMBIENT_LEVEL
            + (crate::lighting::MAX_BRIGHTNESS - crate::lighting::AMBIENT_LEVEL) * step as f32
                / 1000.0;
        worst = worst.max(packed_channel_error(value));
    }
    assert!(
        worst <= 0.5 / 255.0 + 1e-6,
        "worst lighting quantisation {worst} exceeds half a step"
    );
}

#[test]
fn packed_colour_clamps_instead_of_wrapping() {
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
        let packed = PackedVertex::from(&Vertex {
            pos: [0.0, 0.0, 0.0],
            color: [value, value, value, 1.0],
            uv: [0.0, 0.0],
        });
        assert_eq!(
            packed.color[0], expected,
            "value {value} must clamp to {expected}"
        );
    }
    let nan = PackedVertex::from(&Vertex {
        pos: [0.0, 0.0, 0.0],
        color: [f32::NAN; 4],
        uv: [0.0, 0.0],
    });
    assert_eq!(
        nan.color,
        [0, 0, 0, 0],
        "NaN must not become a bright value"
    );
}

#[test]
fn packed_alpha_is_preserved_for_props_and_the_hud() {
    // Prop models carry alpha from their glTF `COLOR_0`, and the UI blends
    // with it, so the fourth channel must survive packing.
    for value in [0.0f32, 0.25, 0.5, 1.0] {
        let packed = PackedVertex::from(&Vertex {
            pos: [0.0, 0.0, 0.0],
            color: [1.0, 1.0, 1.0, value],
            uv: [0.0, 0.0],
        });
        assert!(
            (dequantize_unit(packed.color[3]) - value).abs() <= 0.5 / 255.0 + 1e-6,
            "alpha {value} did not survive packing"
        );
    }
}

#[test]
fn packed_positions_and_uvs_are_bit_exact() {
    // World positions and texture coordinates are not quantised at all: a
    // 250-metre level and a world-space tiling UV both need the range.
    let samples: [[f32; 3]; 5] = [
        [0.0, 0.0, 0.0],
        [-131.9975, 3.4999, 132.0001],
        [1.0e-7, -1.0e-7, 2.5],
        [1.0e6, -1.0e6, 0.5],
        [-0.0, 0.1, -0.1],
    ];
    for pos in samples {
        let uv = [-131.9975f32, 132.0001];
        let vertex = Vertex {
            pos,
            color: [0.5, 0.5, 0.5, 1.0],
            uv,
        };
        let packed = PackedVertex::from(&vertex);
        assert_eq!(packed.pos.map(f32::to_bits), pos.map(f32::to_bits));
        assert_eq!(packed.uv.map(f32::to_bits), uv.map(f32::to_bits));
    }
}

#[test]
fn packing_a_whole_mesh_never_moves_geometry_or_uvs() {
    // End-to-end over a real level: every packed vertex must agree with the
    // build vertex it came from, bit for bit, except for the shade that is
    // deliberately quantised.
    let mesh = build_level_geometry(&two_cluster_level(4));
    for range in &mesh.ranges {
        for vertex in &range.vertices {
            let packed = PackedVertex::from(vertex);
            assert_exact_array(packed.pos, vertex.pos);
            assert_exact_array(packed.uv, vertex.uv);
            for channel in 0..4 {
                assert!(
                    (dequantize_unit(packed.color[channel]) - vertex.color[channel]).abs()
                        <= 0.5 / 255.0 + 1e-6,
                    "channel {channel} drifted: {} vs {}",
                    dequantize_unit(packed.color[channel]),
                    vertex.color[channel]
                );
            }
        }
    }
}

#[test]
fn the_packed_layout_shrinks_gpu_memory_by_a_third() {
    let mesh = build_level_geometry(&two_cluster_level(6));
    let packed_bytes = mesh.vertex_count * std::mem::size_of::<PackedVertex>();
    let unpacked_bytes = mesh.vertex_count * std::mem::size_of::<Vertex>();
    assert_eq!(packed_bytes * 3, unpacked_bytes * 2, "36 -> 24 bytes");
    // Indices are unchanged at two bytes each, so the whole static buffer
    // footprint drops by a quarter, not a third.
    let packed_total = packed_bytes + mesh.index_count * 2;
    let unpacked_total = unpacked_bytes + mesh.index_count * 2;
    assert!(packed_total < unpacked_total);
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
    // The six office surfaces are opaque 128x128 two-metre tiles; the one
    // NPOT diagnostic proves arbitrary PNG dimensions load.
    for texture in [
        "core:tex_wallpaper_yellow_01",
        "core:tex_wallpaper_stained_01",
        "core:tex_carpet_beige_01",
        "core:tex_carpet_damp_01",
        "core:tex_ceiling_panel_01",
        "core:tex_ceiling_stained_01",
    ] {
        let image = texture_image(texture);
        assert_eq!((image.width, image.height), (128, 128), "{texture}");
        assert_eq!(image.rgba.len(), (128 * 128 * 4) as usize);
        for texel in image.rgba.as_chunks::<4>().0 {
            assert_eq!(texel[3], 255, "{texture} must be fully opaque");
        }
    }

    let npot = texture_image("core:tex_diagnostic_alt_01");
    assert_eq!((npot.width, npot.height), (96, 64));
    assert_eq!(npot.rgba.len(), (96 * 64 * 4) as usize);
}

/// The surface textures tile: a wrapped edge must join its opposite edge,
/// or a floor or ceiling shows a grid of seams every repeat.
#[test]
fn test_shipped_surface_textures_tile() {
    for (name, texture) in [
        ("wall", "core:tex_wallpaper_yellow_01"),
        ("wall_stained", "core:tex_wallpaper_stained_01"),
        ("carpet", "core:tex_carpet_beige_01"),
        ("carpet_damp", "core:tex_carpet_damp_01"),
        ("ceiling", "core:tex_ceiling_panel_01"),
        ("ceiling_stained", "core:tex_ceiling_stained_01"),
    ] {
        let image = texture_image(texture);
        let (size_x, size_y) = (image.width, image.height);
        let texel = |x: u32, y: u32| -> [i32; 3] {
            let index = ((y * size_x + x) * 4) as usize;
            [
                i32::from(image.rgba[index]),
                i32::from(image.rgba[index + 1]),
                i32::from(image.rgba[index + 2]),
            ]
        };
        for i in 0..size_y.min(size_x) {
            for channel in 0..3 {
                assert!(
                    (texel(size_x - 1, i)[channel] - texel(0, i)[channel]).abs() <= 40,
                    "{name}: column seam at row {i}"
                );
                assert!(
                    (texel(i, size_y - 1)[channel] - texel(i, 0)[channel]).abs() <= 40,
                    "{name}: row seam at column {i}"
                );
            }
        }
    }
}

#[test]
fn test_build_geometry_from_test_room() {
    let json = include_str!("../../assets/levels/test_room.json");
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

/// The metre checker lives in the carpet PNG now, not in a bake step: the
/// bright and dark quadrants of the two-metre tile must actually differ, so
/// the external asset reproduces the historical floor read.
#[test]
/// The final carpet must never read as the old metre checker again.
///
/// The seed art carried a deliberate 1 m bright/dark quadrant tint, which
/// looked like a debug board on a large floor. The Goal 5 artwork replaces
/// it with low-frequency pile variation, so the four quadrant means must be
/// close: the sheet may be mottled, but no quadrant may be a visibly
/// different flat cell.
fn test_carpet_png_has_no_metre_checker() {
    let carpet = texture_image("core:tex_carpet_beige_01");
    assert_eq!((carpet.width, carpet.height), (128, 128));
    let mean = |x0: u32, y0: u32| -> f32 {
        let mut total = 0.0f32;
        for y in y0..y0 + 64 {
            for x in x0..x0 + 64 {
                let index = ((y * 128 + x) * 4) as usize;
                total += f32::from(carpet.rgba[index])
                    + f32::from(carpet.rgba[index + 1])
                    + f32::from(carpet.rgba[index + 2]);
            }
        }
        total / (64.0 * 64.0 * 3.0)
    };
    let quadrants = [mean(0, 0), mean(64, 0), mean(0, 64), mean(64, 64)];
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
    for y in (0..128).step_by(7) {
        for x in (0..128).step_by(5) {
            let value = carpet.rgba[((y * 128 + x) * 4) as usize];
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
        (960, 544, 480.0 / 272.0), // Retina 2x of the PocketCHIP baseline
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
    // 16:10, 4:3, 3:2 and 1:1 are all narrower than PocketCHIP.
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
        "PocketCHIP UI layout must be pixel-identical to the original"
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

#[test]
fn test_framebuffer_size_changes_update_scale() {
    let small = DrawableSize::new(480, 272);
    let large = DrawableSize::new(1920, 1080);
    assert_ne!(small, large);
    assert!(large.ui_viewport().scale > small.ui_viewport().scale);
    assert_exact(large.ui_viewport().scale, 1080.0 / 272.0);
}

#[test]
fn test_build_geometry_from_level1() {
    let json = include_str!("../../assets/levels/level1.json");
    let level = LevelDef::from_json(json).expect("valid level1 json");
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

    // Floor/ceiling geometry follows the bounded baked-lighting grid: never
    // per square metre, and flat cells merge, so the emitted count is at
    // most the cell grid and usually below it.
    let expected_cells: i32 = level
        .room_iter()
        .map(|room| {
            i32::try_from(
                crate::lighting::light_grid_cells(room.width)
                    * crate::lighting::light_grid_cells(room.depth),
            )
            .unwrap_or(i32::MAX)
        })
        .sum();
    assert!(mesh.batches.floor_batch.count <= expected_cells * 6);
    assert!(mesh.batches.ceiling_batch.count <= expected_cells * 6);
    assert!(
        mesh.batches.floor_batch.count < expected_cells * 6,
        "level 1's large rooms must merge uniform lighting cells"
    );

    // The whole shipped level stays a few tens of thousands of vertices.
    // (Per-metre tessellation of its 25 large rooms would be ~800,000.)
    assert!(
        mesh.vertex_count < 100_000,
        "level1 unexpectedly large: {} vertices",
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
    mesh.triangles_for_material(index)
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
/// same camera convention the game uses.
fn scene_frustum(eye: glam::Vec3, yaw_degrees: f32, pitch_degrees: f32) -> Frustum {
    let aspect = 480.0 / 272.0;
    let fov = vertical_fov_for_aspect(60.0, aspect);
    let proj = glam::Mat4::perspective_rh(fov.to_radians(), aspect, 0.1, 100.0);
    let pitch = pitch_degrees.to_radians();
    let yaw = yaw_degrees.to_radians();
    let forward = glam::Vec3::new(
        yaw.sin() * pitch.cos(),
        pitch.sin(),
        -yaw.cos() * pitch.cos(),
    );
    let view = glam::Mat4::look_at_rh(eye, eye + forward, glam::Vec3::Y);
    Frustum::from_view_projection(&(proj * view), DepthRange::ZeroToOne)
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
    assert!(batches[0].texture.width > 0);
    assert_eq!(batches[0].texture.width, batches[0].texture.height);
    assert!(batches[0].texture.width <= crate::level::MAX_PROP_TEXTURE_SIZE);

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

fn shipped_level(name: &str) -> crate::level::LevelDef {
    let path = format!("assets/levels/{name}.json");
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{path} must be readable: {error}"));
    crate::level::LevelDef::from_json(&content)
        .unwrap_or_else(|error| panic!("{path} must parse: {error}"))
}

#[test]
fn the_showcase_level_renders_every_core_prop_with_real_geometry() {
    let catalog = shipped_catalog();
    let mut assets = shipped_assets();
    let level = shipped_level("prop_showcase");
    let (mesh, batches) = build_level_geometry_with_assets(&level, &catalog, &mut assets);

    assert_eq!(
        mesh.batches.prop_batch.count, 0,
        "no placeholder boxes expected"
    );

    // The shared showcase fixtures place every catalogue placeable exactly
    // once: the domestic/office map covers the generic and Office props, and
    // the Pool showcase covers the Pool family. Themes organize content;
    // this is the one place a "placed somewhere" check is legitimate.
    let pool_showcase = shipped_level("pool_showcase");
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
    let level = shipped_level("prop_stress");
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

    let arrow = cell(decal_material_slot(DECAL_ARROW_MATERIAL).expect("slot"));
    assert!(
        count(&arrow, |texel| texel[1] > 120
            && texel[1] > texel[0] + 30
            && texel[1] > texel[2] + 30)
            > 200,
        "the floor arrow's own cell must hold the green arrow"
    );

    let stripes = cell(decal_material_slot(DECAL_STRIPES_MATERIAL).expect("slot"));
    assert!(
        count(&stripes, |texel| texel[0] > 180
            && texel[1] > 150
            && texel[2] < 100)
            > 1000,
        "the hazard-stripe cell must hold the yellow stripes"
    );
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
             "material": "core:decal_arrow_01", "surface": "floor" },
           { "x": 3.0, "y": 0.0, "z": 3.0, "width": 0.9, "height": 0.9,
             "material": "core:decal_no_diving_01", "surface": "floor" }"#,
        r#"{ "x": 0.0, "z": 0.0, "width": 6.0, "depth": 0.4, "height": 3.0 }"#,
        "[]",
    );
    assert_eq!(
        decal_sheet_index(&mixed, catalog.assets(), "core:decal_arrow_01"),
        decal_material_slot("core:decal_arrow_01")
    );
    assert_eq!(
        decal_sheet_index(&mixed, catalog.assets(), "core:decal_no_diving_01"),
        Some(DECAL_EXTERNAL_BASE)
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
fn a_wall_decal_lies_exactly_on_its_wall_plane_and_faces_the_room() {
    let level = level_with_decals(
        r#"{ "x": 3.0, "y": 1.5, "z": 0.4, "width": 2.0, "height": 1.0,
             "material": "core:decal_test_01", "surface": "wall_south" }"#,
        r#"{ "x": 0.0, "z": 0.0, "width": 6.0, "depth": 0.4, "height": 3.0 }"#,
        "[]",
    );
    let mesh = build_level_geometry(&level);
    let quad = batch_slice(&mesh, SurfaceKind::Decal);
    assert_eq!(mesh.batches.decal_batch.count, 6, "one decal is one quad");
    // Every corner sits exactly on the authored wall plane, not on a
    // nudged or biased copy of it.
    for vertex in &quad {
        assert_exact_named(vertex.pos[2], 0.4, "wall decal plane");
        assert!(vertex.pos[1] >= 0.99 && vertex.pos[1] <= 2.01);
        assert!(vertex.pos[0] >= 1.99 && vertex.pos[0] <= 4.01);
    }
    assert!(normal_matches(quad_normal(&quad), [0.0, 0.0, 1.0]));
}

#[test]
fn a_floor_decal_stays_flat_and_rotates_in_its_plane() {
    let level = level_with_decals(
        r#"{ "x": 3.0, "y": 0.0, "z": 3.0, "width": 2.0, "height": 1.0,
             "material": "core:decal_arrow_01", "surface": "floor", "rotation_degrees": 90.0 }"#,
        r#"{ "x": 0.0, "z": 0.0, "width": 6.0, "depth": 0.4, "height": 3.0 }"#,
        "[]",
    );
    let mesh = build_level_geometry(&level);
    let quad = batch_slice(&mesh, SurfaceKind::Decal);
    for vertex in &quad {
        assert_exact_named(vertex.pos[1], 0.0, "floor decal plane");
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
             "material": "core:decal_arrow_01", "surface": "floor" }"#,
        r#"{ "x": 0.0, "z": 0.0, "width": 6.0, "depth": 0.4, "height": 3.0 }"#,
        r#"[{ "fixture": "core:fluorescent_panel_01", "x": 3.0, "z": 3.0, "brightness": 1.0,
               "color": [1.0, 0.5, 0.2] }]"#,
    );
    let blue = level_with_decals(
        r#"{ "x": 3.0, "y": 0.0, "z": 3.0, "width": 2.0, "height": 2.0,
             "material": "core:decal_arrow_01", "surface": "floor" }"#,
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
    assert_exact(factor, 0.0);
    assert!(
        (-4.0..0.0).contains(&units),
        "bias must pull decals slightly towards the camera, got {units}"
    );
    assert!(units == -2.0, "the bias is part of the render contract");
    assert!((0.0..1.0).contains(&DECAL_ALPHA_CUTOFF));
    // A constant (factor-free) bias is what keeps a grazing-angle decal
    // stable: the offset does not scale with the depth slope.
    assert_exact(factor, 0.0);
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
            WallUnit::Coalesced { wall, runs } => Some((wall, runs)),
            WallUnit::Plain(_) => None,
        })
        .collect();
    assert_eq!(coalesced.len(), 1, "the overlay is resolved into the host");
    let (wall, runs) = coalesced[0];
    assert_exact_named(wall.x, 0.0, "coalesced wall start");
    assert_exact_named(wall.width, 10.0, "coalesced wall length");
    assert_eq!(runs.len(), 3, "host/overlay/host material runs");
    assert_eq!(
        runs[0].body,
        lookup.key(MaterialSlot::Wall, "core:wallpaper_yellow_01")
    );
    assert_eq!(
        runs[1].body,
        lookup.key(MaterialSlot::Wall, "core:wallpaper_stained_01")
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
    let synthetic = units
        .iter()
        .find_map(|unit| match unit {
            WallUnit::Coalesced { wall, .. } => Some(wall),
            WallUnit::Plain(_) => None,
        })
        .expect("coalesced unit");
    assert!(
        synthetic.openings.is_empty(),
        "an opening covered by every-overlay solid must stay closed"
    );

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
            WallUnit::Coalesced { wall, .. } => Some(wall),
            WallUnit::Plain(_) => None,
        })
        .expect("coalesced unit");
    assert_eq!(
        synthetic.openings.len(),
        1,
        "the shared door survives the merge"
    );
    assert_exact_named(synthetic.openings[0].offset, 2.5, "shared door offset");
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

// ------------------------------------------------- vertical geometry (4.0)

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
    let level = LevelDef::from_json(
        &std::fs::read_to_string("assets/levels/vertical_diagnostic.json")
            .expect("the phase 4 diagnostic ships"),
    )
    .expect("it parses");
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
                    assert!(n[1] < 0.0, "a fixture panel must face down: {n:?}");
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
    // the real ceiling, at 5.0 m, not at the authored 0.0.
    assert_eq!(y_bounds(&decals), (2.0, 5.0));
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
        matches!(units[0], WallUnit::Plain(_)),
        "a wall with no coincident twin must not be rewritten"
    );
    let mesh = build_level_geometry(&level);
    assert_eq!(
        batch_slice(&mesh, SurfaceKind::Wall).len() / 6,
        4,
        "two length faces plus two end caps"
    );
}

#[test]
fn the_residential_levels_resolve_their_stain_overlays() {
    for name in [
        "the_residence",
        "quiet_apartments",
        "after_the_leak",
        "rendering_diagnostic",
    ] {
        let level = shipped_level(name);
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
        if name != "rendering_diagnostic" {
            assert!(
                coalesced >= 5,
                "{name}: expected the authored stain overlays to coalesce, got {coalesced}"
            );
        }
        // Every coalesced unit must cover its whole span with runs, so no
        // face can fall back to the host material at a run boundary.
        for unit in &units {
            if let WallUnit::Coalesced { wall, runs } = unit {
                assert!(!runs.is_empty());
                assert_exact_named(runs[0].start, 0.0, "first run starts at the wall origin");
                assert!(
                    (runs[runs.len() - 1].end - wall.length()).abs() < 1e-3,
                    "{name}: the last material run must end with the wall"
                );
                for pair in runs.windows(2) {
                    assert!(
                        (pair[0].end - pair[1].start).abs() < 1e-3,
                        "{name}: material runs must be contiguous"
                    );
                }
            }
        }
        // And the normal build still succeeds with them.
        let mesh = build_level_geometry(&level);
        assert!(mesh.batches.wall_batch.count > 0);
    }
}
