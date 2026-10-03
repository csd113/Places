//! Opt-in offline stage images and exact visibility-caster reports.
//! Diagnostic files never replace a package's physical lighting solution.

use std::io::{BufWriter, Write as _};
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::{
    Chart, LightmapPatch, LightmapTexel, TransportReceiver, TransportScene, length, scale,
};
use crate::lighting::lightmap::{LightmapPage, write_page_png};

/// Destination directory for compiler diagnostics. Use one directory per map
/// and quality variant, and `--force` to bypass a prepared-package cache hit.
pub const DUMP_ENV: &str = "PLACES_LIGHTING_DUMP_DIR";

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
}

/// Whether the opt-in compiler dump is requested.
#[must_use]
pub fn enabled() -> bool {
    std::env::var_os(DUMP_ENV).is_some()
}

/// Writes triangle ownership and reports precisely which opaque triangle first
/// blocks each requested ray. `PLACES_LIGHTING_RAYS` names a JSON ray-list file.
///
/// # Errors
/// Reports invalid/oversized ray input or filesystem failures; the caller logs
/// diagnostic failures without changing the bake.
pub fn dump_scene(scene: &TransportScene, owners: &[CasterRange]) -> Result<(), String> {
    let Some(directory) = std::env::var_os(DUMP_ENV) else {
        return Ok(());
    };
    let directory = Path::new(&directory);
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
                corners: triangle.map(|triangle| [triangle.p0, triangle.p1, triangle.p2]),
                geometric_normal: triangle.map(|triangle| triangle.normal),
            }
        })
        .collect::<Vec<_>>();
    write_json(directory, "caster-ranges.json", &owners)?;
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
    write_json(directory, "emitters.json", &emitters)?;
    write_json(directory, "caster-rays.json", &hits)
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
    dump_stage(
        "global-direct",
        charts,
        receivers,
        receivers.iter().map(|receiver| {
            let mut value = super::Accumulator::default();
            scene.accumulate_global(
                &mut value,
                receiver.ray_origin,
                Some(receiver.normal),
                receiver.attenuation,
                taps,
            );
            super::compress_surface(&value, receiver.normal)
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
        let index = index
            .parse::<usize>()
            .map_err(|error| format!("local-light index: {error}"))?;
        let emitter = scene
            .emitters
            .get(index)
            .ok_or_else(|| "local-light index outside scene".to_string())?;
        dump_stage(
            &format!("local-{index}"),
            charts,
            receivers,
            receivers.iter().map(|receiver| {
                let (weight, direction) =
                    emitter.direct_from(scene, receiver.position, receiver.ray_origin, taps);
                let mut value = super::Accumulator::default();
                super::accumulate_surface_lobe(
                    &mut value,
                    super::attenuate(weight, receiver.attenuation),
                    direction,
                    receiver.normal,
                );
                super::compress_surface(&value, receiver.normal)
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

fn chart_record(index: usize, patch: &LightmapPatch, chart: &Chart) -> ChartRecord {
    let (width, height) = patch.extent_m();
    ChartRecord {
        index,
        page: chart.page,
        rectangle: [chart.x, chart.y, chart.width, chart.height],
        kind: patch.kind.name(),
        room: patch.room,
        origin: patch.origin,
        u_axis: patch.u_axis,
        v_axis: patch.v_axis,
        diagonal_correction: patch.diagonal_correction,
        triangle: patch.triangle,
        geometric_normal: super::patch_normal(patch),
        texels_per_metre: [
            f32::from(u16::try_from(chart.width.saturating_sub(1)).unwrap_or(u16::MAX))
                / width.max(1.0e-10),
            f32::from(u16::try_from(chart.height.saturating_sub(1)).unwrap_or(u16::MAX))
                / height.max(1.0e-10),
        ],
    }
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
    let Some(directory) = std::env::var_os(DUMP_ENV) else {
        return Ok(());
    };
    let directory = Path::new(&directory);
    if charts.iter().any(|(_, chart)| {
        chart.width == 0
            || chart.height == 0
            || chart.page >= 8
            || chart
                .x
                .checked_add(chart.width)
                .is_none_or(|end| end > 1024)
            || chart
                .y
                .checked_add(chart.height)
                .is_none_or(|end| end > 1024)
    }) {
        return Err("lighting dump chart outside the validated atlas bounds".to_string());
    }
    let records = charts
        .iter()
        .enumerate()
        .map(|(index, (patch, chart))| chart_record(index, patch, chart))
        .collect::<Vec<_>>();
    write_json(directory, "charts.json", &records)?;
    std::fs::create_dir_all(directory)
        .map_err(|error| format!("lighting dump directory: {error}"))?;
    let linear = receivers
        .iter()
        .zip(values)
        .map(|(receiver, value)| value.light_at(receiver.normal))
        .collect::<Vec<_>>();
    let path = directory.join(format!("{stage}.rgb-f32le"));
    let mut file = BufWriter::new(
        std::fs::File::create(&path).map_err(|error| format!("lighting dump: {error}"))?,
    );
    for value in &linear {
        for channel in value {
            file.write_all(&channel.to_le_bytes())
                .map_err(|error| format!("lighting dump: {error}"))?;
        }
    }
    file.flush()
        .map_err(|error| format!("lighting dump: {error}"))?;
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
        write_page_png(
            &page,
            &directory.join(format!("{stage}-page{page_index}.png")),
        )?;
    }
    Ok(())
}

fn write_json(directory: &Path, name: &str, value: &impl Serialize) -> Result<(), String> {
    std::fs::create_dir_all(directory)
        .map_err(|error| format!("lighting dump directory: {error}"))?;
    let path = directory.join(name);
    let temporary = path.with_extension("tmp");
    let file =
        std::fs::File::create(&temporary).map_err(|error| format!("lighting dump: {error}"))?;
    let mut writer = BufWriter::new(file);
    serde_json::to_writer(&mut writer, value)
        .map_err(|error| format!("lighting dump JSON: {error}"))?;
    writer
        .flush()
        .map_err(|error| format!("lighting dump flush: {error}"))?;
    std::fs::rename(&temporary, &path).map_err(|error| format!("lighting dump publish: {error}"))
}
