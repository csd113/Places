//! Opt-in offline stage images and exact visibility-caster reports.
//! Diagnostic files never replace a package's physical lighting solution.

use std::cell::RefCell;
use std::io::{BufWriter, Write as _};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{
    Chart, LightmapPatch, LightmapTexel, TransportReceiver, TransportScene, length, scale,
};
use crate::lighting::lightmap::{LIGHTMAP_ATLAS_MAX_PAGES, LightmapPage, page_png_bytes};

/// Destination directory for compiler diagnostics. Use one directory per map
/// and quality variant, and `--force` to bypass a prepared-package cache hit.
pub const DUMP_ENV: &str = "PLACES_LIGHTING_DUMP_DIR";

thread_local! {
    // Compiler scopes are installed before preparation and consumed on its
    // calling thread after worker joins. No process environment is mutated.
    static VARIANT_SCOPE: RefCell<Option<VariantDirectory>> = const { RefCell::new(None) };
}

struct VariantDirectory {
    quality: String,
    lighting: bool,
    probes: bool,
}

/// Restores the previous diagnostic scope when one compiler variant ends.
pub(crate) struct DumpScope(Option<VariantDirectory>);

impl Drop for DumpScope {
    fn drop(&mut self) {
        VARIANT_SCOPE.with(|scope| *scope.borrow_mut() = self.0.take());
    }
}

pub(crate) fn scope_variant(
    quality: crate::quality::LightmapQuality,
    lighting: bool,
    probes: bool,
) -> DumpScope {
    DumpScope(VARIANT_SCOPE.with(|scope| {
        scope.borrow_mut().replace(VariantDirectory {
            quality: quality.name().to_string(),
            lighting,
            probes,
        })
    }))
}

/// Selects a quality-scoped compiler directory, or the historical unscoped
/// directory for callers outside a compiler variant.
pub(crate) fn directory(environment: &str) -> Option<PathBuf> {
    let root = PathBuf::from(std::env::var_os(environment)?);
    scoped_directory(&root, environment)
}

fn scoped_directory(root: &Path, environment: &str) -> Option<PathBuf> {
    VARIANT_SCOPE.with(|scope| {
        scope.borrow().as_ref().map_or_else(
            || Some(root.to_path_buf()),
            |variant| {
                let active = if environment == DUMP_ENV {
                    variant.lighting
                } else {
                    variant.probes
                };
                active.then(|| root.join(&variant.quality))
            },
        )
    })
}

/// Contiguous scene triangle IDs belonging to one architecture range or model
/// batch. Hit triangle coordinates distinguish instances within that batch.
#[derive(Debug, Serialize)]
pub struct CasterRange {
    pub first: usize,
    pub end: usize,
    pub owner: String,
}

#[derive(Deserialize)]
struct RequestedRay {
    origin: [f32; 3],
    direction: [f32; 3],
    #[serde(default)]
    max_distance: Option<f32>,
}

#[derive(Serialize)]
struct CasterHit<'a> {
    origin: [f32; 3],
    direction: [f32; 3],
    distance: Option<f32>,
    triangle: Option<usize>,
    owner: Option<&'a str>,
    corners: Option<[[f32; 3]; 3]>,
    geometric_normal: Option<[f32; 3]>,
    /// Straight alpha throughput along the requested segment; water depth is
    /// a separate receiver attenuation, rather than an alpha surface blocker.
    surface_transmittance: f32,
    /// Existing vertical water-depth extinction at a finite segment's endpoint.
    /// This is receiver attenuation, not absorption along the traced segment.
    receiver_water_attenuation: Option<[f32; 3]>,
}

/// Whether the opt-in compiler dump is requested.
#[must_use]
pub fn enabled() -> bool {
    directory(DUMP_ENV).is_some()
}

/// Writes triangle ownership and reports precisely which opaque triangle first
/// blocks each requested ray. `PLACES_LIGHTING_RAYS` names a JSON ray-list file.
///
/// # Errors
/// Reports invalid/oversized ray input or filesystem failures; the caller logs
/// diagnostic failures without changing the bake.
pub fn dump_scene(scene: &TransportScene, owners: &[CasterRange]) -> Result<(), String> {
    let Some(dump_path) = directory(DUMP_ENV) else {
        return Ok(());
    };
    let rays = requested_rays()?;
    let hits = rays
        .iter()
        .map(|ray| {
            let direction = scale(ray.direction, 1.0 / length(ray.direction));
            let hit = scene
                .intersect(ray.origin, direction)
                .filter(|(distance, _)| ray.max_distance.is_none_or(|limit| *distance < limit));
            let triangle = hit.and_then(|(_, index)| scene.triangles.get(index));
            CasterHit {
                origin: ray.origin,
                direction,
                distance: hit.map(|(distance, _)| distance),
                triangle: hit.map(|(_, index)| index),
                owner: hit.and_then(|(_, index)| {
                    owners
                        .iter()
                        .find(|range| range.first <= index && index < range.end)
                        .map(|range| range.owner.as_str())
                }),
                corners: triangle
                    .map(|hit_triangle| [hit_triangle.p0, hit_triangle.p1, hit_triangle.p2]),
                geometric_normal: triangle.map(|hit_triangle| hit_triangle.normal),
                surface_transmittance: ray.max_distance.map_or_else(
                    || {
                        let (blocker, transmission) =
                            scene.intersect_transport(ray.origin, direction);
                        if blocker.is_some() { 0.0 } else { transmission }
                    },
                    |distance| {
                        scene.transmittance(
                            ray.origin,
                            super::add(ray.origin, scale(direction, distance)),
                        )
                    },
                ),
                receiver_water_attenuation: ray.max_distance.map(|distance| {
                    scene.attenuation_at(super::add(ray.origin, scale(direction, distance)))
                }),
            }
        })
        .collect::<Vec<_>>();
    write_json(&dump_path, "caster-ranges.json", &owners)?;
    let mut alpha_classes = [0_usize; 3];
    for (triangle, material) in scene.triangles.iter().zip(&scene.surface_alpha) {
        if triangle.transmissive {
            continue;
        }
        let index = material
            .as_ref()
            .map_or(0, |surface| match surface.alpha.mode {
                crate::materials::AlphaMode::Opaque => 0,
                crate::materials::AlphaMode::Cutout => 1,
                crate::materials::AlphaMode::Blend => 2,
            });
        if let Some(count) = alpha_classes.get_mut(index) {
            *count = count.saturating_add(1);
        }
    }
    write_json(
        &dump_path,
        "alpha-summary.json",
        &serde_json::json!({
            "opaque_triangles": alpha_classes.first().copied().unwrap_or(0),
            "cutout_triangles": alpha_classes.get(1).copied().unwrap_or(0),
            "blend_triangles": alpha_classes.get(2).copied().unwrap_or(0),
            "water_surface_triangles": scene.triangles.iter().filter(|triangle| triangle.transmissive).count(),
            "meaning": "Opaque blocks; cutout tests level-zero bilinear numeric coverage against cutoff; blend multiplies straight neutral throughput by one minus coverage. Water transmits at its surface and keeps separate volume depth attenuation. No refraction or coloured transmission."
        }),
    )?;
    let emitters = scene
        .emitters
        .iter()
        .enumerate()
        .map(|(index, emitter)| {
            (
                index,
                emitter.position,
                emitter.color,
                emitter.intensity,
                emitter.range,
            )
        })
        .collect::<Vec<_>>();
    write_json(&dump_path, "emitters.json", &emitters)?;
    write_json(&dump_path, "caster-rays.json", &hits)
}

pub(super) fn dump_components(
    scene: &TransportScene,
    charts: &[(LightmapPatch, Chart)],
    receivers: &[TransportReceiver],
    taps: u8,
) -> Result<(), String> {
    if !enabled() {
        return Ok(());
    }
    if let Some(dump_path) = directory(DUMP_ENV) {
        let pitches = (0..charts.len())
            .map(|index| scene.chart_sample_pitch(index))
            .collect::<Vec<_>>();
        write_json(&dump_path, "sample-pitches.json", &pitches)?;
    }
    dump_stage(
        "global-direct",
        charts,
        receivers,
        receivers.iter().map(|receiver| {
            let sample = scene.direct_sample(receiver, &[], true, taps);
            super::compress_surface(&sample.light, receiver.normal)
        }),
    )?;
    dump_stage(
        "geometric-normals",
        charts,
        receivers,
        charts.iter().flat_map(|(patch, chart)| {
            let normal = super::patch_normal(patch);
            let count = usize::try_from(chart.width.saturating_mul(chart.height)).unwrap_or(0);
            std::iter::repeat_n(
                LightmapTexel {
                    irradiance: normal.map(|value| value.mul_add(0.5, 0.5)),
                    ..LightmapTexel::ZERO
                },
                count,
            )
        }),
    )?;
    dump_stage(
        "shading-normals",
        charts,
        receivers,
        receivers.iter().map(|receiver| LightmapTexel {
            irradiance: receiver.normal.map(|value| value.mul_add(0.5, 0.5)),
            ..LightmapTexel::ZERO
        }),
    )?;
    if let Ok(index) = std::env::var("PLACES_LIGHTING_LOCAL") {
        let light_index = index
            .parse::<usize>()
            .map_err(|error| format!("local-light index: {error}"))?;
        let _emitter = scene
            .emitters
            .get(light_index)
            .ok_or_else(|| "local-light index outside scene".to_string())?;
        dump_stage(
            &format!("local-{light_index}"),
            charts,
            receivers,
            receivers.iter().map(|receiver| {
                let sample = scene.direct_sample(receiver, &[light_index], false, taps);
                super::compress_surface(&sample.light, receiver.normal)
            }),
        )?;
    }
    Ok(())
}

fn requested_rays() -> Result<Vec<RequestedRay>, String> {
    let Some(path) = std::env::var_os("PLACES_LIGHTING_RAYS") else {
        return Ok(Vec::new());
    };
    let metadata = std::fs::metadata(&path).map_err(|error| format!("lighting rays: {error}"))?;
    if !metadata.is_file() || metadata.len() > 1_048_576 {
        return Err("lighting ray input must be a file <= 1 MiB".to_string());
    }
    let file = std::fs::File::open(path).map_err(|error| format!("lighting rays: {error}"))?;
    let rays: Vec<RequestedRay> = serde_json::from_reader(std::io::BufReader::new(file))
        .map_err(|error| format!("lighting ray JSON: {error}"))?;
    if rays.len() > 4096
        || rays.iter().any(|ray| {
            !ray.origin
                .iter()
                .chain(&ray.direction)
                .all(|v| v.is_finite())
                || !(1.0e-10..=1.0e10).contains(&length(ray.direction))
                || ray
                    .max_distance
                    .is_some_and(|limit| !limit.is_finite() || !(0.0..=1.0e6).contains(&limit))
        })
    {
        return Err(
            "lighting rays must be <=4096 finite, nonzero vectors with finite nonnegative limits"
                .to_string(),
        );
    }
    Ok(rays)
}

#[derive(Serialize)]
struct ChartRecord {
    index: usize,
    offset: usize,
    texel_count: usize,
    page: u16,
    rectangle: [u32; 4],
    kind: &'static str,
    room: Option<usize>,
    origin: [f32; 3],
    u_axis: [f32; 3],
    v_axis: [f32; 3],
    diagonal_correction: [f32; 3],
    triangle: bool,
    geometric_normal: [f32; 3],
    texels_per_metre: [f32; 2],
}

fn chart_record(index: usize, offset: usize, patch: &LightmapPatch, chart: &Chart) -> ChartRecord {
    let (width, height) = patch.extent_m();
    ChartRecord {
        index,
        offset,
        texel_count: usize::try_from(chart.width.saturating_mul(chart.height)).unwrap_or(0),
        page: chart.page,
        rectangle: [chart.x, chart.y, chart.width, chart.height],
        kind: patch.kind.name(),
        room: patch.room,
        origin: patch.origin,
        u_axis: patch.u_axis,
        v_axis: patch.v_axis,
        diagonal_correction: patch.diagonal_correction,
        triangle: patch.is_triangular(),
        geometric_normal: super::patch_normal(patch),
        texels_per_metre: [
            f32::from(u16::try_from(chart.width.saturating_sub(1)).unwrap_or(u16::MAX))
                / width.max(1.0e-10),
            f32::from(u16::try_from(chart.height.saturating_sub(1)).unwrap_or(u16::MAX))
                / height.max(1.0e-10),
        ],
    }
}

/// Validate the shipped diagnostic page envelope before allocating images.
fn chart_within_dump_bounds(chart: &Chart) -> bool {
    chart.width > 0
        && chart.height > 0
        && usize::from(chart.page) < LIGHTMAP_ATLAS_MAX_PAGES
        && chart
            .x
            .checked_add(chart.width)
            .is_some_and(|end| end <= 1024)
        && chart
            .y
            .checked_add(chart.height)
            .is_some_and(|end| end <= 1024)
}

/// Dumps an isolated stage as tone-mapped receiver lighting atlas PNGs and
/// linear RGB f32 little-endian data in chart/row/column order. Metadata maps
/// every sample back to its chart, world position and geometric normal.
pub(super) fn dump_stage(
    stage: &str,
    charts: &[(LightmapPatch, Chart)],
    receivers: &[TransportReceiver],
    values: impl Iterator<Item = LightmapTexel>,
) -> Result<(), String> {
    let Some(dump_path) = directory(DUMP_ENV) else {
        return Ok(());
    };
    let semantics = stage_semantics(stage)?;
    if charts
        .iter()
        .any(|(_, chart)| !chart_within_dump_bounds(chart))
    {
        return Err("lighting dump chart outside the validated atlas bounds".to_string());
    }
    let records = chart_records(charts)?;
    let expected = records
        .last()
        .map_or(0, |record| record.offset.saturating_add(record.texel_count));
    let linear = reconstruct_samples(receivers, values, expected)?;
    std::fs::create_dir_all(&dump_path)
        .map_err(|error| format!("lighting dump directory: {error}"))?;
    if !dump_path.join("charts.json").exists() {
        dump_receivers(&dump_path, receivers)?;
        write_json(&dump_path, "charts.json", &records)?;
    }
    let path = dump_path.join(format!("{stage}.rgb-f32le"));
    let mut file = BufWriter::new(create_file(&path)?);
    for value in &linear {
        for channel in value {
            file.write_all(&channel.to_le_bytes())
                .map_err(|error| format!("lighting dump: {error}"))?;
        }
    }
    file.flush()
        .map_err(|error| format!("lighting dump: {error}"))?;
    dump_page_images(&dump_path, stage, charts, &linear)?;
    write_json(
        &dump_path,
        &format!("{stage}.json"),
        &serde_json::json!({
            "format_version": 1_u32,
            "origin": "live-transport-solve",
            "stage": stage,
            "semantics": semantics,
            "channel": if stage.ends_with("normals") { "encoded-normal-rgb" } else { "receiver-reconstructed-linear-rgb" },
            "samples": expected,
            "order": "chart index, row, column; chart offsets are sample offsets",
            "data": format!("{stage}.rgb-f32le"),
            "encoding": "3 x IEEE754 f32 little-endian per sample",
            "normal": "actual transport receiver shading normal",
            "png_mapping": "transport soft_clip, then round clamped display channels to RGBA8; no additional gamma transform; chart interiors only, no dilated gutters",
            "coefficient_subtraction": "reconstructed RGB differences are image comparisons; nonlinear light_at is not additive",
            "switchable_lights": if stage.starts_with("local-") { "explicit selected emitter; may include a switchable source" } else { "physical stages describe the base solve, excluding switchable sources; normal stages have no light sources" }
        }),
    )
}

fn dump_page_images(
    dump_path: &Path,
    stage: &str,
    charts: &[(LightmapPatch, Chart)],
    linear: &[[f32; 3]],
) -> Result<(), String> {
    let pages = charts
        .iter()
        .map(|(_, chart)| chart.page)
        .max()
        .unwrap_or(0);
    let edge = charts
        .iter()
        .map(|(_, chart)| {
            chart
                .x
                .saturating_add(chart.width)
                .max(chart.y.saturating_add(chart.height))
        })
        .max()
        .unwrap_or(1)
        .next_power_of_two()
        .min(1024);
    for page_index in 0..=pages {
        let mut page =
            LightmapPage::empty(edge).map_err(|error| format!("lighting dump: {error:?}"))?;
        let mut offset = 0_usize;
        for (_, chart) in charts {
            for row in 0..chart.height {
                for column in 0..chart.width {
                    if chart.page == page_index
                        && let Some(rgb) = linear.get(offset)
                    {
                        page.set_texel(
                            chart.x.saturating_add(column),
                            chart.y.saturating_add(row),
                            LightmapTexel {
                                irradiance: *rgb,
                                direction: [0.0; 3],
                                ..LightmapTexel::ZERO
                            },
                        );
                    }
                    offset = offset.saturating_add(1);
                }
            }
        }
        write_bytes(
            dump_path,
            &format!("{stage}-page{page_index}.png"),
            &page_png_bytes(&page)?,
        )?;
    }
    Ok(())
}

pub(crate) fn write_json(
    directory: &Path,
    name: &str,
    value: &impl Serialize,
) -> Result<(), String> {
    std::fs::create_dir_all(directory)
        .map_err(|error| format!("lighting dump directory: {error}"))?;
    let path = directory.join(name);
    if path.exists() {
        return Err(format!("lighting dump refuses existing {}", path.display()));
    }
    let temporary = directory.join(format!(".{name}.{}.tmp", std::process::id()));
    let file = create_file(&temporary)?;
    let result = (|| {
        let mut writer = BufWriter::new(file);
        serde_json::to_writer(&mut writer, value)
            .map_err(|error| format!("lighting dump JSON: {error}"))?;
        writer
            .flush()
            .map_err(|error| format!("lighting dump flush: {error}"))?;
        // Linking a fully flushed temporary publishes atomically and fails if
        // another writer has already created the final name.
        std::fs::hard_link(&temporary, &path)
            .map_err(|error| format!("lighting dump publish: {error}"))
    })();
    let cleanup =
        std::fs::remove_file(&temporary).map_err(|error| format!("lighting dump cleanup: {error}"));
    result.and(cleanup)
}

pub(crate) fn create_file(path: &Path) -> Result<std::fs::File, String> {
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("diagnostic output {}: {error}", path.display()))
}

pub(crate) fn write_bytes(directory: &Path, name: &str, bytes: &[u8]) -> Result<(), String> {
    let mut file = create_file(&directory.join(name))?;
    file.write_all(bytes)
        .map_err(|error| format!("diagnostic output: {error}"))
}

/// Writes the same chart identity/mapping schema for live and saved data.
pub(crate) fn write_charts(
    directory: &Path,
    charts: &[(LightmapPatch, Chart)],
) -> Result<(), String> {
    write_json(directory, "charts.json", &chart_records(charts)?)
}

fn chart_records(charts: &[(LightmapPatch, Chart)]) -> Result<Vec<ChartRecord>, String> {
    let mut offset = 0_usize;
    charts
        .iter()
        .enumerate()
        .map(|(index, (patch, chart))| {
            let record = chart_record(index, offset, patch, chart);
            offset = offset
                .checked_add(record.texel_count)
                .filter(|count| {
                    *count
                        <= LIGHTMAP_ATLAS_MAX_PAGES
                            .saturating_mul(1024)
                            .saturating_mul(1024)
                })
                .ok_or_else(|| "lighting dump receiver count exceeds atlas budget".to_string())?;
            Ok(record)
        })
        .collect()
}

fn reconstruct_samples(
    receivers: &[TransportReceiver],
    values: impl Iterator<Item = LightmapTexel>,
    expected: usize,
) -> Result<Vec<[f32; 3]>, String> {
    if receivers.len() != expected {
        return Err("lighting dump receiver count does not match charts".to_string());
    }
    let mut source = values;
    let linear = receivers
        .iter()
        .map(|receiver| {
            source
                .next()
                .map(|value| value.light_at(receiver.normal))
                .ok_or_else(|| "lighting dump has fewer values than receivers".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    if source.next().is_some() {
        return Err("lighting dump has more values than receivers".to_string());
    }
    Ok(linear)
}

fn dump_receivers(directory: &Path, receivers: &[TransportReceiver]) -> Result<(), String> {
    let mut floating = BufWriter::new(create_file(&directory.join("receivers.f32le"))?);
    let mut surfaces = BufWriter::new(create_file(&directory.join("receiver-surfaces.u32le"))?);
    for receiver in receivers {
        for channel in receiver
            .position
            .iter()
            .chain(&receiver.ray_origin)
            .chain(&receiver.normal)
            .chain(&receiver.albedo)
            .chain(&receiver.attenuation)
            .chain(std::iter::once(&receiver.area))
        {
            floating
                .write_all(&channel.to_le_bytes())
                .map_err(|error| format!("receiver dump: {error}"))?;
        }
        surfaces
            .write_all(&receiver.surface.to_le_bytes())
            .map_err(|error| format!("receiver dump: {error}"))?;
    }
    floating
        .flush()
        .map_err(|error| format!("receiver dump: {error}"))?;
    surfaces
        .flush()
        .map_err(|error| format!("receiver dump: {error}"))?;
    write_json(
        directory,
        "receivers.json",
        &serde_json::json!({
            "format_version": 1_u32, "samples": receivers.len(),
            "data": "receivers.f32le", "stride_f32": 16_u32,
            "fields": ["position.xyz", "ray_origin.xyz", "normal.xyz", "albedo.rgb", "attenuation.rgb", "area_m2"],
            "surfaces": "receiver-surfaces.u32le", "missing_surface": u32::MAX,
            "order": "charts.json offset + row * chart width + column",
            "surface_identity": "transport triangle index; caster-ranges.json names scene owners; u32::MAX means no nearby surface",
            "position_contract": "surface-offset sample position; ray_origin additionally inset from chart boundary"
        }),
    )
}

fn stage_semantics(stage: &str) -> Result<&'static str, String> {
    match stage {
        "direct" => Ok(
            "integrated physical direct lighting, with receiver-footprint edge refinement; local nonswitchable and global sources",
        ),
        "bounced" => Ok(
            "physical direct plus all solved diffuse bounce orders before indirect filtering and authored fill",
        ),
        "indirect" => {
            Ok("all solved diffuse bounce orders only, before indirect filtering and authored fill")
        }
        "filtered" => Ok("denoised indirect plus unchanged physical direct, before authored fill"),
        "filled" => Ok(
            "filtered combined lighting plus visibility-supported authored recovery fill for eligible receiver classes",
        ),
        "global-direct" => Ok(
            "global physical direct recomputed at the receiver ray origin; point samples, not the integrated direct-pass decomposition",
        ),
        "geometric-normals" => {
            Ok("chart geometric normals encoded as 0.5 * normal + 0.5; no lighting")
        }
        "shading-normals" => {
            Ok("actual receiver shading normals encoded as 0.5 * normal + 0.5; no lighting")
        }
        local
            if local.strip_prefix("local-").is_some_and(|index| {
                !index.is_empty() && index.bytes().all(|byte| byte.is_ascii_digit())
            }) =>
        {
            Ok(
                "one local emitter recomputed at receiver point samples; not the integrated direct-pass decomposition",
            )
        }
        _ => Err("unknown or unsafe lighting diagnostic stage name".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_quality_scopes_are_distinct_and_restore() {
        let root = Path::new("diagnostics");
        let medium = scope_variant(crate::quality::LightmapQuality::Medium, true, false);
        assert_eq!(
            scoped_directory(root, DUMP_ENV),
            Some(root.join("medium")),
            "Medium has its own directory"
        );
        assert_eq!(
            scoped_directory(root, super::super::probe_audit::DUMP_ENV),
            None,
            "unavailable probe output stays disabled"
        );
        {
            let _full = scope_variant(crate::quality::LightmapQuality::Full, true, true);
            assert_eq!(
                scoped_directory(root, DUMP_ENV),
                Some(root.join("full")),
                "Full cannot overwrite Medium"
            );
        }
        assert_eq!(
            scoped_directory(root, DUMP_ENV),
            Some(root.join("medium")),
            "nested scope restores prior variant"
        );
        drop(medium);
        assert_eq!(
            scoped_directory(root, DUMP_ENV),
            Some(root.to_path_buf()),
            "unscoped callers retain compatibility"
        );
    }

    #[test]
    fn diagnostic_stage_names_and_semantics_are_explicit() {
        assert!(
            stage_semantics("indirect").is_ok_and(|text| text.contains("only")),
            "indirect is a real isolated stage"
        );
        assert!(
            stage_semantics("local-2").is_ok_and(|text| text.contains("point samples")),
            "local exports must disclose point sampling"
        );
        for invalid in ["../filled", "local-", "local-../x", "shadow", "fill"] {
            assert!(
                stage_semantics(invalid).is_err(),
                "unsupported or unsafe stage {invalid} must be rejected"
            );
        }
    }

    #[test]
    fn diagnostic_chart_offsets_cover_exact_sample_counts() -> Result<(), String> {
        let patch = LightmapPatch::from_quad(
            super::super::PatchKind::Floor,
            [
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [1.0, 0.0, 1.0],
                [0.0, 0.0, 1.0],
            ],
            Some(0),
        )
        .ok_or_else(|| "test patch rejected".to_string())?;
        let charts = [
            (
                patch,
                Chart {
                    page: 0,
                    x: 2,
                    y: 2,
                    width: 2,
                    height: 3,
                },
            ),
            (
                patch,
                Chart {
                    page: 0,
                    x: 8,
                    y: 2,
                    width: 1,
                    height: 1,
                },
            ),
        ];
        let records = chart_records(&charts)?;
        assert_eq!(
            records
                .iter()
                .map(|record| (record.offset, record.texel_count))
                .collect::<Vec<_>>(),
            [(0, 6), (6, 1)],
            "offsets use chart/row/column order"
        );
        Ok(())
    }

    #[test]
    fn diagnostic_bounds_accept_full_pages_eight_and_nine_and_reject_overflow() {
        let chart = Chart {
            page: 9,
            x: 2,
            y: 2,
            width: 2,
            height: 2,
        };
        assert!(chart_within_dump_bounds(&Chart { page: 8, ..chart }));
        assert!(chart_within_dump_bounds(&chart));
        assert!(!chart_within_dump_bounds(&Chart { page: 10, ..chart }));
        for broken in [
            Chart { width: 0, ..chart },
            Chart { x: 1023, ..chart },
            Chart {
                y: u32::MAX,
                ..chart
            },
        ] {
            assert!(
                !chart_within_dump_bounds(&broken),
                "invalid bounds must fail"
            );
        }
    }

    #[test]
    fn diagnostic_receiver_budget_accepts_ten_pages_but_not_one_extra_sample() -> Result<(), String>
    {
        let patch = LightmapPatch::from_quad(
            super::super::PatchKind::Floor,
            [
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [1.0, 0.0, 1.0],
                [0.0, 0.0, 1.0],
            ],
            Some(0),
        )
        .ok_or_else(|| "test patch rejected".to_string())?;
        let mut charts = (0_u16..10)
            .map(|page| {
                (
                    patch,
                    Chart {
                        page,
                        x: 0,
                        y: 0,
                        width: 1024,
                        height: 1024,
                    },
                )
            })
            .collect::<Vec<_>>();
        let records = chart_records(&charts)?;
        assert_eq!(records.len(), 10);
        assert_eq!(
            records.last().map(|record| record.offset),
            Some(9 * 1024 * 1024)
        );
        charts.push((
            patch,
            Chart {
                page: 0,
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            },
        ));
        assert!(
            chart_records(&charts).is_err(),
            "one extra sample exceeds the shared cap"
        );
        Ok(())
    }

    #[test]
    fn diagnostic_stage_rejects_truncated_and_excess_values() {
        let receiver = TransportReceiver {
            position: [0.0; 3],
            ray_origin: [0.0; 3],
            normal: [0.0, 1.0, 0.0],
            albedo: [1.0; 3],
            attenuation: [1.0; 3],
            area: 1.0,
            surface: u32::MAX,
        };
        assert!(
            reconstruct_samples(&[receiver], std::iter::empty(), 1).is_err(),
            "missing samples cannot silently zip away"
        );
        assert!(
            reconstruct_samples(&[receiver], std::iter::repeat_n(LightmapTexel::ZERO, 2), 1)
                .is_err(),
            "extra samples must be rejected"
        );
        assert!(
            reconstruct_samples(&[receiver], std::iter::once(LightmapTexel::ZERO), 2).is_err(),
            "chart and receiver counts must agree"
        );
        assert_eq!(
            reconstruct_samples(&[receiver], std::iter::once(LightmapTexel::ZERO), 1).ok(),
            Some(vec![[0.0; 3]]),
            "exact streams reconstruct normally"
        );
    }
}
