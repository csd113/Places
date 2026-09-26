//! Dynamic-range audit of the baked surface light (test-only).
//!
//! This is the measurement the lighting rebalance is calibrated against, kept
//! as a regression guard: it rebuilds a level with lightmaps on, walks every
//! real lightmap texel through `fill_chart` (the exact function the atlas uses)
//! and reports the distribution of what the world shader samples.
//!
//! For every texel it decomposes the baked value into the three terms the model
//! sums — the room baseline, the visible local fixture pool and the doorway
//! blend — and additionally computes the *unoccluded* pool (the same loop with
//! the visibility test skipped) so the report can say how much light static
//! occluders actually remove and how much of that removal survives the
//! `MAX_BRIGHTNESS` clamp. A shadow whose occlusion is real but whose final
//! value does not move is the failure mode this audit exists to catch.
//!
//! Run with:
//! `cargo test --release -- --nocapture baked_light_range_audit_report`

// Test code: printing, indexing and permissive float comparison are idiomatic
// here; the production lints stay enforced everywhere else in the crate.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::print_stdout,
    clippy::too_many_lines,
    clippy::unwrap_used
)]

use super::LevelLighting;
use crate::level::LevelDef;
use crate::lighting::color::LightColor;
use crate::lighting::lightmap::{LightmapMode, LightmapPatch, PatchKind, fill_chart};
use crate::lighting::tuning::LIGHTMAP_FACE_NORMAL_BIAS_M;
use crate::lighting::{
    AMBIENT_LEVEL, FILL_MAX, FILL_RANGE_MULTIPLIER, FILL_STRENGTH, LOCAL_LIGHT_MAX,
    LOCAL_LIGHT_STRENGTH, MAX_BRIGHTNESS, ambient_color, smooth_falloff,
};
use crate::quality::QualityProfile;
use crate::render::{LightmapBuildOptions, build_level_geometry_timed_with_lightmaps};

/// One measured lightmap texel and the model terms behind its value.
#[derive(Clone, Copy)]
struct Texel {
    kind: PatchKind,
    /// The value the atlas stores and the shader samples.
    value: LightColor,
    /// Room baseline at the texel's evaluated position.
    baseline: LightColor,
    /// Visible directional direct pool.
    pool: LightColor,
    /// Visible bounce fill.
    fill: LightColor,
    /// Direct pool with the visibility test skipped.
    pool_open: LightColor,
    /// Bounce fill with the visibility test skipped.
    fill_open: LightColor,
    /// Doorway-blend delta.
    blend: LightColor,
}

impl Texel {
    fn value_lum(&self) -> f32 {
        self.value.luminance()
    }

    /// The value this sample would have with no static occluder in the way.
    fn open_value(&self) -> LightColor {
        self.baseline
            .plus(self.pool_open)
            .plus(self.fill_open)
            .plus(self.blend)
            .clamped(AMBIENT_LEVEL, MAX_BRIGHTNESS)
    }

    /// The value the measured terms reproduce (must equal [`Self::value`]).
    fn potential(&self) -> LightColor {
        self.baseline
            .plus(self.pool)
            .plus(self.fill)
            .plus(self.blend)
            .clamped(AMBIENT_LEVEL, MAX_BRIGHTNESS)
    }

    /// Local lighting the static occluders remove from this sample, in
    /// luminance.
    fn occluded_lum(&self) -> f32 {
        (self.pool_open.luminance() + self.fill_open.luminance()
            - self.pool.luminance()
            - self.fill.luminance())
        .max(0.0)
    }

    /// Of that removal, how much changes the clamped final value.
    fn visible_occlusion_lum(&self) -> f32 {
        (self.open_value().luminance() - self.value_lum()).max(0.0)
    }
}

/// A tiny distribution summary.
#[derive(Clone, Copy, Default)]
struct Dist {
    mean: f32,
    p05: f32,
    p50: f32,
    p90: f32,
    max: f32,
}

impl Dist {
    fn of(values: &mut [f32]) -> Self {
        if values.is_empty() {
            return Self::default();
        }
        values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let pick = |p: f32| {
            let last = values.len() - 1;
            let index = (p * last as f32).round() as usize;
            values[index.min(last)]
        };
        Self {
            mean: values.iter().sum::<f32>() / values.len() as f32,
            p05: pick(0.05),
            p50: pick(0.50),
            p90: pick(0.90),
            max: values[values.len() - 1],
        }
    }

    fn line(&self, label: &str) {
        println!(
            "[bake-range] {label:<22} mean={:>6.3} p05={:>6.3} p50={:>6.3} p90={:>6.3} max={:>6.3}",
            self.mean, self.p05, self.p50, self.p90, self.max
        );
    }
}

fn percent(values: &[f32], condition: impl Fn(f32) -> bool) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    let hits = values.iter().filter(|value| condition(**value)).count();
    hits as f32 * 100.0 / values.len() as f32
}

/// The shipped demo level.
fn demo() -> LevelDef {
    let path = "assets/levels/places_demo.json";
    let content = std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("{path} must be readable: {error}"));
    LevelDef::from_json(&content).unwrap_or_else(|error| panic!("{path} must parse: {error}"))
}

/// Mirrors [`LevelLighting::local_light`] with the static-visibility test
/// deliberately skipped, so the audit can measure what occluders remove.
///
/// Returns `(direct, fill)`: the same two screened terms the production path
/// composes, with `visibility` forced to one. The geometry and the screen
/// operator are duplicated here on purpose — this is the measurement's
/// independent reference, and a shared helper would make a production bug
/// invisible to it.
fn pool_without_occlusion(
    lighting: &LevelLighting,
    room: Option<usize>,
    x: f32,
    y: f32,
    z: f32,
) -> (LightColor, LightColor) {
    if !x.is_finite() || !y.is_finite() || !z.is_finite() {
        return (LightColor::BLACK, LightColor::BLACK);
    }
    let mut remaining_direct = [1.0_f32; 3];
    let mut remaining_fill = [1.0_f32; 3];
    for light in lighting.lights() {
        if !light.is_active() {
            continue;
        }
        let (half_w, half_d) = light.source.half_extents();
        let range = light.source.range;
        let fill_range = range * FILL_RANGE_MULTIPLIER.max(1.0);
        let reach_squared = fill_range * fill_range;
        if let Some(sample_room) = room
            && let Some(light_room) = light.room
            && light_room != sample_room
            && lighting.footprint_contains(light_room, x, z)
            && !lighting.rooms_share_air(light_room, sample_room, x, z)
        {
            continue;
        }
        let dx = ((x - light.x()).abs() - half_w).max(0.0);
        let dz = ((z - light.z()).abs() - half_d).max(0.0);
        let horizontal_squared = dx * dx + dz * dz;
        if !horizontal_squared.is_finite() || horizontal_squared >= reach_squared {
            continue;
        }
        let horizontal = horizontal_squared.sqrt();
        let vertical = light.y() - y;
        let distance_squared = vertical.mul_add(vertical, horizontal_squared);
        if !distance_squared.is_finite() {
            continue;
        }
        let distance = distance_squared.sqrt();
        let direct_shape = if light.directional {
            if vertical <= 0.0 || distance <= f32::EPSILON || horizontal >= range {
                0.0
            } else {
                let lateral = 1.0 - horizontal / range;
                let (lateral_squared, incidence) = (lateral * lateral, vertical / distance);
                lateral_squared * incidence
            }
        } else if distance < range {
            light.falloff().factor(distance / range)
        } else {
            0.0
        };
        let fill_shape = if distance < fill_range {
            smooth_falloff(distance / fill_range)
        } else {
            0.0
        };
        if direct_shape <= 0.0 && fill_shape <= 0.0 {
            continue;
        }
        let color = light.color();
        for (channel, &value) in [color.r, color.g, color.b].iter().enumerate() {
            let direct = LOCAL_LIGHT_STRENGTH
                * light.intensity()
                * light.height_factor
                * direct_shape
                * value;
            let direct = direct.min(LOCAL_LIGHT_MAX);
            if let Some(slot) = remaining_direct.get_mut(channel) {
                *slot *= 1.0 - direct / LOCAL_LIGHT_MAX;
            }
            let fill = FILL_STRENGTH * light.intensity() * light.height_factor * fill_shape * value;
            let fill = fill.min(FILL_MAX);
            if let Some(slot) = remaining_fill.get_mut(channel) {
                *slot *= 1.0 - fill / FILL_MAX;
            }
        }
    }
    let channel = |remaining: [f32; 3], cap: f32| LightColor {
        r: cap * (1.0 - remaining[0]),
        g: cap * (1.0 - remaining[1]),
        b: cap * (1.0 - remaining[2]),
    };
    (
        channel(remaining_direct, LOCAL_LIGHT_MAX),
        channel(remaining_fill, FILL_MAX),
    )
}

/// The model terms at one evaluated surface position.
fn terms_at(
    lighting: &LevelLighting,
    room: Option<usize>,
    point: [f32; 3],
) -> (LightColor, LightColor, LightColor, LightColor) {
    // A patch outside every room samples the whole-position path, which
    // resolves a room by height; mirror that here.
    let room = room
        .filter(|room| lighting.rooms().get(*room).is_some())
        .or_else(|| lighting.room_index_at_height(point[0], point[1], point[2]));
    let Some(room) = room else {
        let terms = lighting.pool_terms_in_room(None, point[0], point[1], point[2]);
        let (direct, fill) = screen_terms(&terms);
        return (ambient_color(), direct, fill, LightColor::BLACK);
    };
    let baseline = lighting.baseline_in_room(room, point[0], point[2]);
    let blend = lighting.opening_blend(room, point[0], point[1], point[2]);
    let terms = lighting.pool_terms_in_room(Some(room), point[0], point[1], point[2]);
    let (direct, fill) = screen_terms(&terms);
    (baseline, direct, fill, blend)
}

/// Screens the measured terms into `(direct, fill)` with the production caps.
fn screen_terms(terms: &[super::PoolTerm]) -> (LightColor, LightColor) {
    let mut remaining_direct = [1.0_f32; 3];
    let mut remaining_fill = [1.0_f32; 3];
    for term in terms {
        let direct = LOCAL_LIGHT_STRENGTH
            * term.intensity
            * term.height_factor
            * term.direct_shape
            * term.visibility;
        let fill =
            FILL_STRENGTH * term.intensity * term.height_factor * term.fill_shape * term.visibility;
        for (channel, &value) in [term.color.r, term.color.g, term.color.b]
            .iter()
            .enumerate()
        {
            let contribution = (direct * value).min(LOCAL_LIGHT_MAX);
            if let Some(slot) = remaining_direct.get_mut(channel) {
                *slot *= 1.0 - contribution / LOCAL_LIGHT_MAX;
            }
            let contribution = (fill * value).min(FILL_MAX);
            if let Some(slot) = remaining_fill.get_mut(channel) {
                *slot *= 1.0 - contribution / FILL_MAX;
            }
        }
    }
    let channel = |remaining: [f32; 3], cap: f32| LightColor {
        r: cap * (1.0 - remaining[0]),
        g: cap * (1.0 - remaining[1]),
        b: cap * (1.0 - remaining[2]),
    };
    (
        channel(remaining_direct, LOCAL_LIGHT_MAX),
        channel(remaining_fill, FILL_MAX),
    )
}

/// The world-space nudge `fill_chart` applies to a patch before evaluation.
fn face_bias(patch: &LightmapPatch) -> [f32; 3] {
    if !matches!(patch.kind, PatchKind::Wall | PatchKind::Skirt) {
        return [0.0; 3];
    }
    let cross = |a: [f32; 3], b: [f32; 3]| {
        [
            a[1].mul_add(b[2], -(a[2] * b[1])),
            a[2].mul_add(b[0], -(a[0] * b[2])),
            a[0].mul_add(b[1], -(a[1] * b[0])),
        ]
    };
    let normal = cross(patch.u_axis, patch.v_axis);
    let length = normal[0]
        .mul_add(
            normal[0],
            normal[1].mul_add(normal[1], normal[2] * normal[2]),
        )
        .sqrt();
    if !length.is_finite() || length <= f32::EPSILON {
        return [0.0; 3];
    }
    normal.map(|value| value / length * LIGHTMAP_FACE_NORMAL_BIAS_M)
}

/// `fill_chart`'s texel axis: the texels span the patch inclusively.
fn axis(index: usize, count: u32) -> f32 {
    if count <= 1 {
        return 0.5;
    }
    index as f32 / (count - 1) as f32
}

/// Everything one audit run measures.
struct Measurement {
    texels: Vec<Texel>,
    rooms: Vec<(usize, f32, usize, f32)>,
    charts: usize,
    lights: usize,
    zones: usize,
}

/// Every texel of one baked level, with its model decomposition.
fn measure(level: &LevelDef) -> Measurement {
    let catalog = crate::loader::PropCatalog::load_default();
    let mut assets = crate::props::PropAssets::load_default();
    let materials = crate::render::logical_materials(level);
    let build = build_level_geometry_timed_with_lightmaps(
        level,
        &catalog,
        &mut assets,
        &materials,
        LightmapBuildOptions::for_profile(QualityProfile::Full, LightmapMode::On),
        None,
    );
    let lighting = &build.lighting;
    let lightmaps = build
        .lightmaps
        .as_deref()
        .expect("the demo must bake lightmaps");
    let mut texels = Vec::new();
    for (patch, chart) in &lightmaps.charts {
        let values = fill_chart(lighting, patch, chart);
        let bias = face_bias(patch);
        let mut index = 0usize;
        for j in 0..usize::try_from(chart.height).unwrap_or(0) {
            for i in 0..usize::try_from(chart.width).unwrap_or(0) {
                let u = axis(i, chart.width);
                let v = axis(j, chart.height);
                let raw = patch.point_at(u, v);
                let point = [raw[0] + bias[0], raw[1] + bias[1], raw[2] + bias[2]];
                // A texel buried in a wall is walked into its room exactly like
                // `sample_in_room` walks it, so the decomposition is measured at
                // the position the model actually evaluates.
                let (px, pz) = match patch.room {
                    Some(room) if lighting.wall_contains_point(point[0], point[2]) => {
                        lighting.clear_sample(room, point[0], point[2])
                    }
                    _ => (point[0], point[2]),
                };
                let evaluated = [px, point[1], pz];
                let (baseline, pool, fill, blend) = terms_at(lighting, patch.room, evaluated);
                let open = pool_without_occlusion(
                    lighting,
                    patch.room,
                    evaluated[0],
                    evaluated[1],
                    evaluated[2],
                );
                let value = values[index];
                texels.push(Texel {
                    kind: patch.kind,
                    value: LightColor::rgb(value[0], value[1], value[2]),
                    baseline,
                    pool,
                    fill,
                    pool_open: open.0,
                    fill_open: open.1,
                    blend,
                });
                index += 1;
            }
        }
    }
    let summary = lighting.summary();
    let rooms = lighting
        .rooms()
        .iter()
        .enumerate()
        .map(|(index, room)| {
            (
                index,
                room.area_m2,
                room.fixture_count,
                room.baseline.luminance(),
            )
        })
        .collect();
    Measurement {
        texels,
        rooms,
        charts: lightmaps.chart_count(),
        lights: summary.lights,
        zones: summary.zones,
    }
}

/// Largest channel error between the stored texel and the measured terms.
fn decomposition_error(texels: &[Texel]) -> f32 {
    texels
        .iter()
        .map(|texel| {
            let a = texel.value;
            let b = texel.potential();
            (a.r - b.r)
                .abs()
                .max((a.g - b.g).abs())
                .max((a.b - b.b).abs())
        })
        .fold(0.0_f32, f32::max)
}

/// Fraction of texels whose stored value the measured terms do not reproduce
/// within 0.05. A small residue is expected on texels whose evaluated position
/// differs between `fill_chart` and this decomposition; a large one would mean
/// the report is not measuring the model it claims to.
fn decomposition_mismatch(texels: &[Texel]) -> f32 {
    if texels.is_empty() {
        return 0.0;
    }
    let hits = texels
        .iter()
        .filter(|texel| {
            let a = texel.value;
            let b = texel.potential();
            (a.r - b.r)
                .abs()
                .max((a.g - b.g).abs())
                .max((a.b - b.b).abs())
                > 0.05
        })
        .count();
    hits as f32 * 100.0 / texels.len() as f32
}

/// The counterfactual value with the baseline's span above ambient scaled by
/// `k`, pools and blends unchanged: the shape the rebalance is choosing.
fn scaled_value(texel: &Texel, pool: LightColor, fill: LightColor, k: f32) -> f32 {
    let channel = |base: f32, local: f32, fill: f32, blend: f32| {
        (base - AMBIENT_LEVEL)
            .mul_add(k, AMBIENT_LEVEL + local + fill + blend)
            .clamp(AMBIENT_LEVEL, MAX_BRIGHTNESS)
    };
    let baseline = texel.baseline;
    let blend = texel.blend;
    let red = channel(baseline.r, pool.r, fill.r, blend.r);
    let green = channel(baseline.g, pool.g, fill.g, blend.g);
    let blue = channel(baseline.b, pool.b, fill.b, blend.b);
    red.mul_add(0.2126, green.mul_add(0.7152, blue * 0.0722))
        .clamp(0.0, 1.0)
}

fn print_report(measurement: &Measurement, label: &str) {
    let texels = &measurement.texels;
    let count = texels.len();
    let mut value: Vec<f32> = texels.iter().map(Texel::value_lum).collect();
    let mut baseline: Vec<f32> = texels.iter().map(|t| t.baseline.luminance()).collect();
    let mut pool: Vec<f32> = texels
        .iter()
        .map(|t| t.pool.luminance() + t.fill.luminance())
        .collect();
    let mut open_pool: Vec<f32> = texels
        .iter()
        .map(|t| t.pool_open.luminance() + t.fill_open.luminance())
        .collect();
    let mut occluded: Vec<f32> = texels.iter().map(Texel::occluded_lum).collect();
    let mut visible: Vec<f32> = texels.iter().map(Texel::visible_occlusion_lum).collect();
    println!(
        "[bake-range] level={label} texels={count} charts={} lights={} zones={} decomposition_max_error={:.2e} mismatch>0.05={:.1}%",
        measurement.charts,
        measurement.lights,
        measurement.zones,
        decomposition_error(texels),
        decomposition_mismatch(texels)
    );
    Dist::of(&mut value).line("value");
    Dist::of(&mut baseline).line("baseline");
    Dist::of(&mut pool).line("pool+fill(visible)");
    Dist::of(&mut open_pool).line("pool+fill(open)");
    Dist::of(&mut occluded).line("occluded_removal");
    Dist::of(&mut visible).line("visible_occlusion");

    let stored: Vec<f32> = texels.iter().map(Texel::value_lum).collect();
    println!(
        "[bake-range] clamp: at_1.0={:.1}% at_0.95+={:.1}% at_0.9+={:.1}%",
        percent(&stored, |v| v >= MAX_BRIGHTNESS - 1e-3),
        percent(&stored, |v| v >= 0.95),
        percent(&stored, |v| v >= 0.90),
    );

    let shadowed: Vec<&Texel> = texels.iter().filter(|t| t.occluded_lum() >= 0.05).collect();
    let hidden = shadowed
        .iter()
        .filter(|t| t.visible_occlusion_lum() < 0.25 * t.occluded_lum())
        .count();
    let invisible = shadowed
        .iter()
        .filter(|t| t.visible_occlusion_lum() < 0.02)
        .count();
    let shadowed_n = shadowed.len();
    println!(
        "[bake-range] occlusion_vs_clamp: texels_with_removal>=0.05={shadowed_n} mostly_hidden(<25% shows)={:.1}% invisible(<0.02 shows)={:.1}%",
        if shadowed_n == 0 {
            0.0
        } else {
            hidden as f32 * 100.0 / shadowed_n as f32
        },
        if shadowed_n == 0 {
            0.0
        } else {
            invisible as f32 * 100.0 / shadowed_n as f32
        },
    );

    let fully: Vec<&Texel> = texels
        .iter()
        .filter(|t| {
            t.pool_open.luminance() + t.fill_open.luminance() >= 0.15
                && t.pool.luminance() + t.fill.luminance()
                    <= 0.1 * (t.pool_open.luminance() + t.fill_open.luminance())
        })
        .collect();
    if !fully.is_empty() {
        let open_mean = fully
            .iter()
            .map(|t| t.open_value().luminance())
            .sum::<f32>()
            / fully.len() as f32;
        let value_mean = fully.iter().map(|t| t.value_lum()).sum::<f32>() / fully.len() as f32;
        println!(
            "[bake-range] fully_shadowed: n={} open_value_mean={open_mean:.3} shadowed_value_mean={value_mean:.3} loss={:.3} ({:.0}% of open)",
            fully.len(),
            open_mean - value_mean,
            if open_mean > 0.0 {
                (open_mean - value_mean) * 100.0 / open_mean
            } else {
                0.0
            },
        );
    }

    for kind in [
        PatchKind::Floor,
        PatchKind::Ceiling,
        PatchKind::Wall,
        PatchKind::Skirt,
    ] {
        let subset: Vec<&Texel> = texels.iter().filter(|t| t.kind == kind).collect();
        if subset.is_empty() {
            continue;
        }
        let values: Vec<f32> = subset.iter().map(|t| t.value_lum()).collect();
        let mut dist = values.clone();
        let dist = Dist::of(&mut dist);
        println!(
            "[bake-range] kind={:<8} n={:<6} value_mean={:.3} p50={:.3} clamp={:.1}%",
            kind.name(),
            subset.len(),
            dist.mean,
            dist.p50,
            percent(&values, |v| v >= MAX_BRIGHTNESS - 1e-3),
        );
    }

    // The shape the rebalance is choosing: the measured baseline span
    // rescaled to a candidate ceiling, pools unchanged. `k` is the scale that
    // maps the measured baseline maximum onto the candidate, so the row reads
    // exactly as that BASELINE_MAX would bake on this level.
    let measured_max = texels
        .iter()
        .map(|t| t.baseline.luminance())
        .fold(0.0_f32, f32::max);
    let span = measured_max - AMBIENT_LEVEL;
    for target in [0.50_f32, 0.55, 0.60, 0.65, 0.70, 0.775, 0.85, 1.0] {
        let k = if span > 1e-6 {
            (target - AMBIENT_LEVEL) / span
        } else {
            1.0
        };
        let mut values: Vec<f32> = texels
            .iter()
            .map(|t| scaled_value(t, t.pool, t.fill, k))
            .collect();
        let dist = Dist::of(&mut values);
        let clamp = percent(&values, |v| v >= MAX_BRIGHTNESS - 1e-3);
        let hidden = if shadowed_n == 0 {
            0.0
        } else {
            shadowed
                .iter()
                .filter(|t| {
                    let open = scaled_value(t, t.pool_open, t.fill_open, k);
                    let value = scaled_value(t, t.pool, t.fill, k);
                    (open - value) < 0.25 * t.occluded_lum()
                })
                .count() as f32
                * 100.0
                / shadowed_n as f32
        };
        let shadow_loss = if fully.is_empty() {
            0.0
        } else {
            fully
                .iter()
                .map(|t| {
                    scaled_value(t, t.pool_open, t.fill_open, k)
                        - scaled_value(t, t.pool, t.fill, k)
                })
                .sum::<f32>()
                / fully.len() as f32
        };
        let open_mean = if fully.is_empty() {
            0.0
        } else {
            fully
                .iter()
                .map(|t| scaled_value(t, t.pool_open, t.fill_open, k))
                .sum::<f32>()
                / fully.len() as f32
        };
        println!(
            "[bake-range] BASELINE_MAX={target:.3}: k={k:.3} mean={:.3} p05={:.3} p50={:.3} p90={:.3} clamp={:.1}% hidden={:.1}% shadow_loss={:.3} ({:.0}% of open)",
            dist.mean,
            dist.p05,
            dist.p50,
            dist.p90,
            clamp,
            hidden,
            shadow_loss,
            if open_mean > 0.0 {
                shadow_loss * 100.0 / open_mean
            } else {
                0.0
            }
        );
    }
}

/// The report over the shipped demo, plus the regression thresholds.
#[test]
fn baked_light_range_audit_report() {
    let level = demo();
    let measurement = measure(&level);
    print_report(&measurement, "places_demo");
    print!("[bake-range] rooms:");
    for (index, area, fixtures, baseline) in &measurement.rooms {
        print!(" r{index}(area={area:.0},fx={fixtures},base={baseline:.3})");
    }
    println!();

    // Regression thresholds. These are the calibrated shape of the rebalanced
    // model; a change that re-saturates the bake or re-hides the occlusion must
    // fail here. The numbers are the measured demo values with a margin:
    // clamped 0.0% (threshold 1%), occluded removal shown 100% (threshold 90%),
    // fully-shadowed loss 43% of the open value (threshold 35%).
    let texels = &measurement.texels;
    let values: Vec<f32> = texels.iter().map(Texel::value_lum).collect();
    let clamped = percent(&values, |v| v >= MAX_BRIGHTNESS - 1e-3);
    assert!(
        clamped < 1.0,
        "the clamped fraction must stay negligible, got {clamped:.1}%"
    );

    let count = texels.len() as f32;
    let removed = texels.iter().map(Texel::occluded_lum).sum::<f32>() / count;
    let shown = texels.iter().map(Texel::visible_occlusion_lum).sum::<f32>() / count;
    assert!(
        shown > removed * 0.9,
        "the clamp must not eat what occluders remove: {shown:.3} of {removed:.3} shows"
    );

    let fully: Vec<&Texel> = texels
        .iter()
        .filter(|t| {
            t.pool_open.luminance() + t.fill_open.luminance() >= 0.15
                && t.pool.luminance() + t.fill.luminance()
                    <= 0.1 * (t.pool_open.luminance() + t.fill_open.luminance())
        })
        .collect();
    assert!(fully.len() > 1000, "the demo must contain shadowed samples");
    let open_mean = fully
        .iter()
        .map(|t| t.open_value().luminance())
        .sum::<f32>()
        / fully.len() as f32;
    let shadow_mean = fully.iter().map(|t| t.value_lum()).sum::<f32>() / fully.len() as f32;
    assert!(
        open_mean - shadow_mean > open_mean * 0.35,
        "a fully shadowed sample must lose over a third of its light: {shadow_mean:.3} of {open_mean:.3}"
    );

    let mut sorted = values;
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let pick = |p: f32| sorted[(p * (sorted.len() - 1) as f32) as usize];
    assert!(
        pick(0.90) > pick(0.05) * 1.5,
        "lit surfaces must be clearly brighter than the dimmest open surface: p90 {:.3} vs p05 {:.3}",
        pick(0.90),
        pick(0.05)
    );
}
