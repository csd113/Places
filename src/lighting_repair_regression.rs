//! Focused regression fixtures for the lighting-repair job.
//!
//! These cases are the acceptance suite for the prepared-path repairs: water
//! and translucent panes transmit light, an opaque barrier at the same plane
//! still blocks, several fixtures contribute independently, an ordinary
//! adjacent corner carries light, and every solved value stays finite and
//! non-negative. The fixtures live under `tests/fixtures/levels/`; this module
//! owns the checks, not the layout.
//!
//! The two fixtures:
//!
//! ```text
//! water_transmission     one room, two identical 3x3 m basins either side of a
//!                        full-height divider. Basin A floors are at -1.2 m with
//!                        an authored water volume at surface_y = -0.2 and a
//!                        0.35 m walk-in step at -0.85. Basin B is the identical
//!                        geometry control: no water, and an opaque half-wall
//!                        slab whose 8 cm top cap sits at exactly -0.2 over the
//!                        same footprint. One fluorescent panel above each
//!                        basin, same brightness and colour.
//! lighting_repair_cases  five independent 4x4 cells under one level: a
//!                        single-light cell, a three-light cell, a lit cell
//!                        with a doorway into a dark neighbour, and a lit
//!                        corner cell. Every cell uses the same neutral
//!                        surfaces so measurements compare like for like.
//! ```
//!
//! Test code: unwrap/expect, indexing, loose casts and permissive arithmetic are
//! idiomatic in tests; the production lints stay enforced everywhere else.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::print_stdout,
    clippy::suboptimal_flops,
    clippy::unwrap_used
)]

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::level::LevelDef;
use crate::lighting::lightmap::{LevelLightmaps, LightmapMode, LightmapPatch, PatchKind};
use crate::render::{
    LevelBuild, LightmapBuildOptions, build_level_geometry_timed_with_lightmaps, logical_materials,
};

/// Room index of the single room in `tests/fixtures/levels/water_transmission.json`.
mod water_cell {
    pub const POOL: usize = 0;
}

/// Room indices in `tests/fixtures/levels/lighting_repair_cases.json`, in file order.
mod repair_cells {
    pub const SINGLE_LIGHT: usize = 0;
    pub const MULTI_LIGHT: usize = 1;
    pub const DOOR_LIT: usize = 2;
    pub const DOOR_DARK: usize = 3;
    pub const CORNER_LIT: usize = 4;
}

// ---------------------------------------------------------------- fixtures

/// Loads one engine regression fixture from `tests/fixtures/levels/`.
fn fixture_level(name: &str) -> LevelDef {
    let path = format!("tests/fixtures/levels/{name}.json");
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{path} must be readable: {error}"));
    LevelDef::from_json(&content).unwrap_or_else(|error| panic!("{path} must parse: {error}"))
}

/// A renderer-independent High/On lightmap build with the builtin prop
/// catalog, modelled on `src/render/tests.rs::lightmap_build`: the same
/// options, no cache, so every run bakes fresh and deterministically.
fn lightmap_build(level: &LevelDef) -> LevelBuild {
    let materials = logical_materials(level);
    let catalog = crate::loader::PropCatalog::builtin();
    let mut assets = crate::props::PropAssets::default();
    build_level_geometry_timed_with_lightmaps(
        level,
        &catalog,
        &mut assets,
        &materials,
        LightmapBuildOptions::for_level(crate::quality::QualityLevel::High, LightmapMode::On),
        None,
    )
}

/// The solved water-transmission fixture, built once per test process.
fn water_build() -> &'static LevelBuild {
    static BUILD: OnceLock<LevelBuild> = OnceLock::new();
    BUILD.get_or_init(|| {
        let build = lightmap_build(&fixture_level("water_transmission"));
        assert_eq!(
            build.lightmap_failure, None,
            "water_transmission must atlas cleanly"
        );
        build
    })
}

/// The solved lighting-repair-cases fixture, built once per test process.
fn repair_build() -> &'static LevelBuild {
    static BUILD: OnceLock<LevelBuild> = OnceLock::new();
    BUILD.get_or_init(|| {
        let build = lightmap_build(&fixture_level("lighting_repair_cases"));
        assert_eq!(
            build.lightmap_failure, None,
            "lighting_repair_cases must atlas cleanly"
        );
        build
    })
}

/// The atlas of a fixture build, failing loudly when the build fell back to
/// vertex lighting.
fn atlas_of(build: &'static LevelBuild, name: &str) -> &'static LevelLightmaps {
    build
        .lightmaps
        .as_deref()
        .unwrap_or_else(|| panic!("{name} must produce a lightmap atlas"))
}

// ------------------------------------------------------- texel inspection

/// One solved atlas texel selected by its reconstructed world position.
#[derive(Clone, Copy)]
struct TexelSample {
    page: u16,
    x: u32,
    y: u32,
    /// Soft-clipped per-channel display light along the patch normal.
    rgb: [f32; 3],
}

impl TexelSample {
    fn luma(self) -> f32 {
        rec709(self.rgb)
    }
}

/// Rec. 709 luminance, the same metric the renderer's atlas diagnostics use.
fn rec709(rgb: [f32; 3]) -> f32 {
    // Parenthesised to match the renderer's own nesting exactly.
    0.2126_f32.mul_add(rgb[0], 0.7152_f32.mul_add(rgb[1], 0.0722 * rgb[2]))
}

/// One channel of the renderer's display conversion: pass-through below the
/// 0.8 knee, C1-continuous shoulder above it (`soft_clip` in
/// `crate::lighting::transport`). Copied locally so this module measures the
/// displayed value without depending on solver internals.
fn soft_clip_channel(value: f32) -> f32 {
    const KNEE: f32 = 0.8;
    if !value.is_finite() {
        return 0.0;
    }
    if value <= KNEE {
        return value.max(0.0);
    }
    let shoulder = 1.0 - KNEE;
    KNEE + shoulder * (1.0 - (-(value - KNEE) / shoulder).exp())
}

/// The patch's outward normal from its own winding (`u = p0 -> p1`,
/// `v = p0 -> p3`), the same reconstruction the shader and the solver use.
fn patch_normal(patch: &LightmapPatch) -> [f32; 3] {
    let cross = [
        patch.u_axis[1] * patch.v_axis[2] - patch.u_axis[2] * patch.v_axis[1],
        patch.u_axis[2] * patch.v_axis[0] - patch.u_axis[0] * patch.v_axis[2],
        patch.u_axis[0] * patch.v_axis[1] - patch.u_axis[1] * patch.v_axis[0],
    ];
    let length = (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt();
    if length.is_finite() && length > 1.0e-9 {
        [cross[0] / length, cross[1] / length, cross[2] / length]
    } else {
        [0.0, 1.0, 0.0]
    }
}

/// Every solved texel of `kind` whose reconstructed world position falls inside
/// the axis-aligned box `min..=max`, in chart order.
///
/// The selection uses the patch's own mapping ([`LightmapPatch::point_at`]), so
/// it is the geometry the baker evaluated, not an image-space guess.
fn atlas_samples_in_box(
    lightmaps: &LevelLightmaps,
    kind: PatchKind,
    min: [f32; 3],
    max: [f32; 3],
) -> Vec<TexelSample> {
    let mut out = Vec::new();
    for (patch, chart) in &lightmaps.charts {
        if patch.kind != kind {
            continue;
        }
        let Some(page) = lightmaps.pages.get(usize::from(chart.page)) else {
            continue;
        };
        let normal = patch_normal(patch);
        for row in 0..chart.height {
            for column in 0..chart.width {
                let u = (column as f32 + 0.5) / chart.width as f32;
                let v = (row as f32 + 0.5) / chart.height as f32;
                let point = patch.point_at(u, v);
                if point[0] < min[0]
                    || point[0] > max[0]
                    || point[1] < min[1]
                    || point[1] > max[1]
                    || point[2] < min[2]
                    || point[2] > max[2]
                {
                    continue;
                }
                let Some(texel) = page.texel(chart.x + column, chart.y + row) else {
                    continue;
                };
                let light = texel.light_at(normal);
                out.push(TexelSample {
                    page: chart.page,
                    x: chart.x + column,
                    y: chart.y + row,
                    rgb: [
                        soft_clip_channel(light[0]),
                        soft_clip_channel(light[1]),
                        soft_clip_channel(light[2]),
                    ],
                });
            }
        }
    }
    out
}

/// Distribution of one world box's selected texels, in display units.
struct BoxStats {
    count: usize,
    mean_rgb: [f32; 3],
    min_luma: f32,
    max_luma: f32,
}

/// Mean, min and max of the soft-clipped texels inside one world box.
fn box_stats(
    lightmaps: &LevelLightmaps,
    kind: PatchKind,
    min: [f32; 3],
    max: [f32; 3],
) -> BoxStats {
    let samples = atlas_samples_in_box(lightmaps, kind, min, max);
    let mut sum = [0.0_f64; 3];
    let mut min_luma = f32::INFINITY;
    let mut max_luma = f32::NEG_INFINITY;
    for sample in &samples {
        for (channel, value) in sample.rgb.iter().enumerate() {
            sum[channel] += f64::from(*value);
        }
        let luma = sample.luma();
        min_luma = min_luma.min(luma);
        max_luma = max_luma.max(luma);
    }
    let count = samples.len();
    if count == 0 {
        return BoxStats {
            count: 0,
            mean_rgb: [0.0; 3],
            min_luma: 0.0,
            max_luma: 0.0,
        };
    }
    let divisor = count as f64;
    BoxStats {
        count,
        mean_rgb: [
            (sum[0] / divisor) as f32,
            (sum[1] / divisor) as f32,
            (sum[2] / divisor) as f32,
        ],
        min_luma,
        max_luma,
    }
}

impl BoxStats {
    /// Mean Rec. 709 luminance, the scalar the channel thresholds use.
    fn luma(&self) -> f32 {
        rec709(self.mean_rgb)
    }
}

/// Prints one measured box and the calibrated vertex-lit model's value at its
/// centre, for the evidence log.
fn measure(
    label: &str,
    build: &LevelBuild,
    lightmaps: &LevelLightmaps,
    room: usize,
    kind: PatchKind,
    min: [f32; 3],
    max: [f32; 3],
) -> BoxStats {
    let stats = box_stats(lightmaps, kind, min, max);
    let centre = [
        f32::midpoint(min[0], max[0]),
        f32::midpoint(min[1], max[1]),
        f32::midpoint(min[2], max[2]),
    ];
    let vertex = build
        .lighting
        .sample_in_room(room, centre[0], centre[1], centre[2])
        .to_array();
    println!(
        "{label:28} count {:5} mean ({:.3},{:.3},{:.3}) luma {:.3} min {:.3} max {:.3} | vertex ({:.3},{:.3},{:.3})",
        stats.count,
        stats.mean_rgb[0],
        stats.mean_rgb[1],
        stats.mean_rgb[2],
        stats.luma(),
        stats.min_luma,
        stats.max_luma,
        vertex[0],
        vertex[1],
        vertex[2],
    );
    stats
}

/// The `(page, x, y, luma)` samples of `samples` that sit below
/// `floor_fraction` of the mean of all four in-box atlas neighbours.
///
/// A texel whose four neighbours are not all part of the selected set is
/// skipped: the sampled region's interior is what a smoothness check can speak
/// for.
fn isolated_dips(samples: &[TexelSample], floor_fraction: f32) -> Vec<(u32, u32, f32, f32)> {
    let mut by_coord: HashMap<(u16, u32, u32), f32> = HashMap::new();
    for sample in samples {
        by_coord.insert((sample.page, sample.x, sample.y), sample.luma());
    }
    let mut dips = Vec::new();
    for sample in samples {
        let luma = by_coord
            .get(&(sample.page, sample.x, sample.y))
            .copied()
            .unwrap_or(0.0);
        let neighbours = [
            (sample.x.wrapping_sub(1), sample.y),
            (sample.x.wrapping_add(1), sample.y),
            (sample.x, sample.y.wrapping_sub(1)),
            (sample.x, sample.y.wrapping_add(1)),
        ];
        let mut sum = 0.0_f32;
        let mut present = 0_u32;
        for (x, y) in neighbours {
            if let Some(value) = by_coord.get(&(sample.page, x, y)) {
                sum += *value;
                present += 1;
            }
        }
        if present == 4 {
            let neighbour_mean = sum / 4.0;
            if neighbour_mean > 0.0 && luma < neighbour_mean * floor_fraction {
                dips.push((sample.x, sample.y, luma, neighbour_mean));
            }
        }
    }
    dips
}

/// Asserts every stored/solved value of one atlas is finite and non-negative.
fn assert_atlas_clean(lightmaps: &LevelLightmaps, name: &str) -> usize {
    let mut page_texels = 0_usize;
    for (page_index, page) in lightmaps.pages.iter().enumerate() {
        for (index, texel) in page.texels.iter().enumerate() {
            page_texels += 1;
            assert!(
                texel.is_finite(),
                "{name}: page {page_index} texel {index} holds a non-finite value: {texel:?}"
            );
            for (channel, value) in texel.irradiance.iter().enumerate() {
                assert!(
                    value.is_finite() && *value >= 0.0,
                    "{name}: page {page_index} texel {index} irradiance channel {channel} \
                     must be finite and non-negative, got {value}"
                );
            }
            let light = texel.light_at([0.0, 1.0, 0.0]);
            for (channel, value) in light.iter().enumerate() {
                assert!(
                    value.is_finite() && *value >= 0.0,
                    "{name}: page {page_index} texel {index} reconstructed channel {channel} \
                     must be finite and non-negative, got {value}"
                );
            }
        }
    }
    for (patch, chart) in &lightmaps.charts {
        let normal = patch_normal(patch);
        let Some(page) = lightmaps.pages.get(usize::from(chart.page)) else {
            continue;
        };
        for row in 0..chart.height {
            for column in 0..chart.width {
                let Some(texel) = page.texel(chart.x + column, chart.y + row) else {
                    continue;
                };
                for (channel, value) in texel.light_at(normal).iter().enumerate() {
                    assert!(
                        value.is_finite() && *value >= 0.0,
                        "{name}: charted texel ({column},{row}) channel {channel} \
                         must be finite and non-negative, got {value}"
                    );
                }
            }
        }
    }
    assert!(page_texels > 0, "{name}: the atlas must hold real pages");
    page_texels
}

// ------------------------------------------------------------- test cases

// Thresholds measured on the repaired solver; see
// `target/agent-work/.../recon/c-water-fixtures.md` for the run that produced
// them. Measured: basin A floor 0.670, basin B (opaque control) 0.331, step
// top 0.721, A/B ratio 2.02x, transmitted excess A - B 0.339.
//
// The control is deliberately *not* near zero on the repaired solver: the
// opaque lid blocks the fixture pool, and what it reads is the authored room
// fill the fill repair installs (0.331, against 0.426 in the calibrated
// vertex-lit model at the same point). The acceptance signal is therefore the
// transmitted excess over that fill, not a zero bound. A 4x ratio would need a
// fixture-free control zone or a ~4000 m2 room to push the logarithmic room
// baseline under 0.05; the measured ratio is documented as the job allows.
const BASIN_LIT_MIN: f32 = 0.55;
const BASIN_CONTROL_MAX: f32 = 0.45;
/// Basin A must beat the opaque control by at least this factor.
const BASIN_CONTRAST_MIN: f32 = 1.75;
/// And by at least this much in absolute display units.
const BASIN_EXCESS_MIN: f32 = 0.20;

#[test]
fn water_transmits_light_into_the_basin_and_the_opaque_control_blocks() {
    let build = water_build();
    let lightmaps = atlas_of(build, "water_transmission");
    // Mirrored 1.2 x 1.8 m floor boxes, 0.7 m from each basin's fixture, both
    // clear of the walls and of the step's edge.
    let basin_a = measure(
        "basin_a_floor",
        build,
        lightmaps,
        water_cell::POOL,
        PatchKind::Floor,
        [2.4, -1.7, 1.6],
        [3.6, -0.7, 3.4],
    );
    let basin_b = measure(
        "basin_b_floor_opaque",
        build,
        lightmaps,
        water_cell::POOL,
        PatchKind::Floor,
        [4.4, -1.7, 1.6],
        [5.6, -0.7, 3.4],
    );
    // The shallow step top at -0.85 m (0.65 m of water above) against the deep
    // floor at -1.2 m (1.0 m of water above): a second depth for attenuation.
    let step_a = measure(
        "basin_a_step_top",
        build,
        lightmaps,
        water_cell::POOL,
        PatchKind::Floor,
        [1.1, -1.2, 1.6],
        [1.9, -0.5, 3.4],
    );

    assert!(
        basin_a.count > 50 && basin_b.count > 50 && step_a.count > 20,
        "the boxes must select real texels: {} {} {}",
        basin_a.count,
        basin_b.count,
        step_a.count
    );
    for (name, stats) in [
        ("basin_a", &basin_a),
        ("basin_b", &basin_b),
        ("step", &step_a),
    ] {
        assert!(
            stats.luma().is_finite() && stats.luma() >= 0.0,
            "{name} must stay finite and non-negative, measured {}",
            stats.luma()
        );
    }

    println!(
        "water_contrast basin_a {:.4} basin_b {:.4} ratio {:.2}x step_top {:.4}",
        basin_a.luma(),
        basin_b.luma(),
        if basin_b.luma() > 0.0 {
            basin_a.luma() / basin_b.luma()
        } else {
            f32::INFINITY
        },
        step_a.luma(),
    );

    assert!(
        basin_a.luma() > BASIN_LIT_MIN,
        "basin A must be clearly lit through the authored water surface: \
         measured {:.4}, need > {BASIN_LIT_MIN}",
        basin_a.luma()
    );
    assert!(
        basin_b.luma() < BASIN_CONTROL_MAX,
        "the opaque control must stay below the water-lit basin: \
         measured {:.4}, need < {BASIN_CONTROL_MAX}",
        basin_b.luma()
    );
    assert!(
        basin_a.luma() > basin_b.luma() * BASIN_CONTRAST_MIN,
        "water must transmit clearly more than the opaque control: \
         measured {:.4} vs {:.4} (need > {BASIN_CONTRAST_MIN}x)",
        basin_a.luma(),
        basin_b.luma()
    );
    assert!(
        basin_a.luma() > basin_b.luma() + BASIN_EXCESS_MIN,
        "the transmitted excess over the control must be real: \
         measured {:.4} - {:.4} = {:.4}, need > {BASIN_EXCESS_MIN}",
        basin_a.luma(),
        basin_b.luma(),
        basin_a.luma() - basin_b.luma()
    );
    // The shipped water extinction is red-heavy ([0.35, 0.12, 0.05] per metre);
    // light that crossed 1 m of water must come out measurably bluer than red.
    // Measured on this fixture: mean red 0.571, mean blue 0.689.
    assert!(
        basin_a.mean_rgb[0] < basin_a.mean_rgb[2],
        "transmitted light must be blue-shifted by the water tint: \
         mean red {:.4} >= mean blue {:.4}",
        basin_a.mean_rgb[0],
        basin_a.mean_rgb[2]
    );
    // The shallower step is the second depth sample. It sits 0.65 m under the
    // water (against 1.0 m at the deep floor) and is marginally closer to the
    // fixture, so a physically monotone attenuation curve must leave it no
    // dimmer than the deeper floor.
    assert!(
        step_a.luma() >= basin_a.luma(),
        "the shallower step must not be dimmer than the deep floor: \
         measured step {:.4} vs deep {:.4}",
        step_a.luma(),
        basin_a.luma()
    );
}

/// Thresholds measured on the repaired solver for the multi-light cell; the
/// single-light cell is the reference and only needs to be clearly lit.
const SINGLE_LIGHT_MIN: f32 = 0.05;

#[test]
fn several_light_fixtures_contribute_independently() {
    let build = repair_build();
    let lightmaps = atlas_of(build, "lighting_repair_cases");
    // The same 1 x 1 m floor box directly under each cell's central panel; the
    // multi-light cell adds one identical panel either side along X.
    let single = measure(
        "single_light_floor",
        build,
        lightmaps,
        repair_cells::SINGLE_LIGHT,
        PatchKind::Floor,
        [1.5, -0.5, 1.5],
        [2.5, 0.5, 2.5],
    );
    let multi = measure(
        "multi_light_floor",
        build,
        lightmaps,
        repair_cells::MULTI_LIGHT,
        PatchKind::Floor,
        [5.9, -0.5, 1.5],
        [6.9, 0.5, 2.5],
    );

    assert!(
        single.count > 20 && multi.count > 20,
        "the boxes must select real texels: {} {}",
        single.count,
        multi.count
    );
    for (name, stats) in [("single", &single), ("multi", &multi)] {
        assert!(
            stats.luma().is_finite() && stats.luma() >= 0.0,
            "{name} must stay finite and non-negative, measured {}",
            stats.luma()
        );
    }
    assert!(
        single.luma() > SINGLE_LIGHT_MIN,
        "the single-light reference must be clearly lit: \
         measured {:.4}, need > {SINGLE_LIGHT_MIN}",
        single.luma()
    );
    println!(
        "multi_light single {:.4} multi {:.4} ratio {:.2}x",
        single.luma(),
        multi.luma(),
        multi.luma() / single.luma().max(1.0e-6)
    );
    assert!(
        multi.luma() > single.luma(),
        "three contributing fixtures must beat one: measured multi {:.4} vs single {:.4}",
        multi.luma(),
        single.luma()
    );

    // Every selected sample of both boxes must be finite and non-negative, not
    // only their means.
    for (name, min, max) in [
        ("single", [1.5, -0.5, 1.5], [2.5, 0.5, 2.5]),
        ("multi", [5.9, -0.5, 1.5], [6.9, 0.5, 2.5]),
    ] {
        for sample in atlas_samples_in_box(lightmaps, PatchKind::Floor, min, max) {
            for (channel, value) in sample.rgb.iter().enumerate() {
                assert!(
                    value.is_finite() && *value >= 0.0,
                    "{name}: channel {channel} must be finite and non-negative, got {value}"
                );
            }
        }
    }
}

#[test]
fn an_adjacent_corner_is_lit_and_clean() {
    let build = repair_build();
    let lightmaps = atlas_of(build, "lighting_repair_cases");
    // The inside corner where the corner cell's west wall (face at x = 17.6)
    // meets the outer south wall (face at z = 4.0), sampled 0.4..2.6 m up.
    let min = [17.55, 0.4, 3.55];
    let max = [18.1, 2.6, 4.05];
    let stats = measure(
        "corner_walls",
        build,
        lightmaps,
        repair_cells::CORNER_LIT,
        PatchKind::Wall,
        min,
        max,
    );
    assert!(stats.count > 20, "the corner box must select wall texels");

    let samples = atlas_samples_in_box(lightmaps, PatchKind::Wall, min, max);
    for sample in &samples {
        assert!(
            sample.luma().is_finite(),
            "corner texel ({},{}) must be finite, got {}",
            sample.x,
            sample.y,
            sample.luma()
        );
        assert!(
            sample.luma() > 0.0,
            "corner texel ({},{}) must be above zero, got {}",
            sample.x,
            sample.y,
            sample.luma()
        );
    }

    // No isolated dip: a texel with all four neighbours in the box may not sit
    // below 20% of their mean.
    let dips = isolated_dips(&samples, 0.2);
    assert!(
        dips.is_empty(),
        "the lit corner must have no isolated dark texel: {dips:?}"
    );
    println!(
        "corner_walls count {} mean luma {:.4} min {:.4} max {:.4} dips {}",
        stats.count,
        stats.luma(),
        stats.min_luma,
        stats.max_luma,
        dips.len()
    );
}

/// The through-door receiver must beat its mirror behind the solid wall by at
/// least this factor (measured on the repaired solver).
const DOORWAY_CONTRAST_MIN: f32 = 2.0;

#[test]
fn a_doorway_transmits_and_the_wall_beside_it_does_not() {
    let build = repair_build();
    let lightmaps = atlas_of(build, "lighting_repair_cases");
    // The lit cell's panel sits at (10.8, 2.0); the doorway cuts z = 0.5..1.5.
    // Both receiver boxes sit 0.2 m behind the shared wall's face at x = 13.2,
    // mirrored about the fixture axis z = 2.0, so the only difference is the
    // aperture.
    let lit_cell = measure(
        "door_lit_floor",
        build,
        lightmaps,
        repair_cells::DOOR_LIT,
        PatchKind::Floor,
        [10.3, -0.5, 1.5],
        [11.3, 0.5, 2.5],
    );
    let through_door = measure(
        "door_receiver",
        build,
        lightmaps,
        repair_cells::DOOR_DARK,
        PatchKind::Floor,
        [13.4, -0.5, 0.6],
        [14.6, 0.5, 1.4],
    );
    let through_wall = measure(
        "wall_receiver",
        build,
        lightmaps,
        repair_cells::DOOR_DARK,
        PatchKind::Floor,
        [13.4, -0.5, 2.6],
        [14.6, 0.5, 3.4],
    );
    assert!(
        through_door.count > 20 && through_wall.count > 20 && lit_cell.count > 20,
        "the boxes must select real texels: {} {} {}",
        through_door.count,
        through_wall.count,
        lit_cell.count
    );
    println!(
        "doorway lit_cell {:.4} through_door {:.4} through_wall {:.4} door/wall ratio {:.2}x",
        lit_cell.luma(),
        through_door.luma(),
        through_wall.luma(),
        through_door.luma() / through_wall.luma().max(1.0e-6)
    );
    assert!(
        lit_cell.luma() > through_door.luma(),
        "the lit cell's own floor must beat the receiver behind the doorway: \
         measured {:.4} vs {:.4}",
        lit_cell.luma(),
        through_door.luma()
    );
    assert!(
        through_door.luma() > through_wall.luma() * DOORWAY_CONTRAST_MIN,
        "the doorway must transmit: measured through-door {:.4} vs \
         behind-wall {:.4} (need > {DOORWAY_CONTRAST_MIN}x)",
        through_door.luma(),
        through_wall.luma()
    );
}

#[test]
fn every_solved_value_is_finite_and_non_negative() {
    let water = assert_atlas_clean(
        atlas_of(water_build(), "water_transmission"),
        "water_transmission",
    );
    let repair = assert_atlas_clean(
        atlas_of(repair_build(), "lighting_repair_cases"),
        "lighting_repair_cases",
    );
    println!(
        "atlas_health water_transmission {water} page texels, lighting_repair_cases {repair} page texels, all finite and non-negative"
    );
}

/// Explicit offline evidence run; excluded from the fast regression suite.
#[test]
#[ignore = "stage audit of maintained packages; run with PLACES_VERBOSE=1"]
fn maintained_transport_stage_audit() {
    for source in [
        "assets/levels/places_demo.json",
        "tests/fixtures/levels/lighting_repair_cases.json",
        "tests/fixtures/levels/home_showcase.json",
    ] {
        println!("AUDIT_SOURCE {source}");
        let level =
            LevelDef::from_json(&std::fs::read_to_string(source).expect("source")).expect("level");
        let materials = logical_materials(&level);
        let catalog = crate::loader::PropCatalog::builtin();
        let mut assets = crate::props::PropAssets::default();
        let build = build_level_geometry_timed_with_lightmaps(
            &level,
            &catalog,
            &mut assets,
            &materials,
            LightmapBuildOptions::for_lightmaps(crate::quality::LightmapQuality::Full),
            None,
        );
        assert_eq!(build.lightmap_failure, None);
    }
}
