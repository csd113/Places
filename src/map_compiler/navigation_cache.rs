//! Navigation reuse follows physical authoring inputs, independently of light
//! quality, project menus and final archive encoding. No unverified cache data
//! can enter a package; legacy packages simply take the normal bake path.
use super::{BuildRequest, navigation::NavBakeReport};
use crate::engine::level::LevelDef;
use crate::engine::package::{
    PackageReader,
    manifest::Manifest,
    navigation::{self, NavGrid},
};
use serde::{Deserialize, Serialize};

pub(super) const ENTRY: &str = "navigation-inputs.json";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cached {
    fingerprint: String,
    report: NavBakeReport,
    warnings: Vec<String>,
}

pub(super) fn fingerprint(level: &LevelDef) -> Result<String, String> {
    let mut value = serde_json::to_value(level).map_err(|e| e.to_string())?;
    let fields = value
        .as_object_mut()
        .ok_or("Navigation source must be an object")?;
    // Physical structures sampled by collision, floor/ceiling, door portals,
    // water and elevator landings, plus actor placement diagnostics. Retain
    // whole component/geometry definitions conservatively. Script edits also
    // invalidate this stage so future scripted physical policies remain safe.
    fields.retain(|name, _| {
        matches!(
            name.as_str(),
            "id" | "defaults"
                | "rooms"
                | "walls"
                | "floor_regions"
                | "water"
                | "ramps"
                | "stairs"
                | "half_walls"
                | "columns"
                | "void_walls"
                | "arc_walls"
                | "pillars"
                | "archways"
                | "guardrails"
                | "thresholds"
                | "props"
                | "doors"
                | "elevators"
                | "spawn_templates"
                | "spawn_points"
                | "scripts"
                | "script_sources"
        )
    });
    let bytes = crate::engine::canonical_json::canonical_json_bytes(&(
        "navigation-stage-v1",
        env!("PLACES_COMPILER_CODE_SHA256"),
        super::navigation::DEFAULT_NAV_CELL_M,
        value,
        crate::engine::package::hash::sha256_hex(
            &crate::engine::package::collision::write_collision(
                &crate::engine::package::collision::CompiledCollision::from_level(level),
            )?,
        ),
    ))?;
    Ok(crate::engine::package::hash::sha256_hex(&bytes))
}

pub(super) fn fits_provenance_record(bytes: usize) -> bool {
    u64::try_from(bytes).is_ok_and(|length| length <= crate::engine::package::MAX_MANIFEST_BYTES)
}

fn read_current<R: std::io::Read + std::io::Seek>(
    reader: &mut PackageReader<R>,
    manifest: &Manifest,
    fingerprint: &str,
) -> Result<(Vec<u8>, Cached), String> {
    let entry = manifest.entry(ENTRY).ok_or(
        "Navigation input identity unavailable (oversized diagnostics are omitted by builds)",
    )?;
    if entry.bytes > crate::engine::package::MAX_MANIFEST_BYTES || entry.role != "compiler-inputs" {
        return Err("Invalid navigation input identity record".into());
    }
    let bytes = super::read_declared_entry(reader, manifest, entry)?;
    let cached: Cached = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    if cached.fingerprint != fingerprint {
        return Err("Navigation inputs changed".into());
    }
    let first = manifest.variants.first().ok_or("No navigation variant")?;
    if manifest
        .variants
        .iter()
        .any(|variant| variant.entries.navigation != first.entries.navigation)
    {
        return Err("Quality variants disagree on the common navigation bake".into());
    }
    let grid_bytes = reader.read_blob(
        &first.entries.navigation,
        crate::engine::package::MAX_NAVIGATION_BYTES,
    )?;
    let grid = navigation::read_navigation(&grid_bytes)?;
    validate_report(&cached.report, &grid)?;
    Ok((grid_bytes, cached))
}

/// Compares every persisted diagnostic count to the decoded grid. The worker
/// count is local to the original bake and is deliberately absent on disk.
pub(super) fn validate_report(report: &NavBakeReport, grid: &NavGrid) -> Result<(), String> {
    if report.cell_m.to_bits() != grid.cell_m.to_bits()
        || report.cells_x != grid.cells_x
        || report.cells_z != grid.cells_z
        || report.surface_cells != grid.cell_count()
        || report.portals != grid.portals.len()
        || report.walkable_cells.len() != grid.classes.len()
        || report.regions.len() != grid.classes.len()
    {
        return Err(
            "Navigation report disagrees with the decoded grid dimensions, portals or classes"
                .into(),
        );
    }
    // The decoder checks each walkable bit against its region label. Count
    // only real nodes, excluding mask-byte padding.
    for (class, labels) in grid.region.iter().enumerate() {
        let mut regions = std::collections::BTreeSet::new();
        let mut walkable = 0_usize;
        for chunk in labels.chunks(4096) {
            super::progress::check_cancelled()?;
            for label in chunk
                .iter()
                .copied()
                .filter(|label| *label != navigation::NO_REGION)
            {
                walkable = walkable.saturating_add(1);
                let _inserted = regions.insert(label);
            }
        }
        if report.walkable_cells.get(class).copied() != Some(walkable)
            || report
                .regions
                .get(class)
                .and_then(|count| usize::try_from(*count).ok())
                != Some(regions.len())
        {
            return Err(format!(
                "Navigation report disagrees with the decoded walkable or region count for class {class}"
            ));
        }
    }
    Ok(())
}

/// Requires current typed provenance. A legal omission takes the ordinary
/// build path, which can recompute the diagnostics and omit the record again.
#[cfg(unix)]
pub(super) fn sibling_warnings<R: std::io::Read + std::io::Seek>(
    reader: &mut crate::engine::package::PackageReader<R>,
    manifest: &crate::engine::package::manifest::Manifest,
    level: &LevelDef,
) -> Result<Vec<String>, String> {
    let (_, cached) = read_current(reader, manifest, &fingerprint(level)?)?;
    Ok(cached.warnings)
}

pub(super) fn prepare(
    request: &BuildRequest,
    level: &LevelDef,
    warnings: &mut Vec<String>,
    decisions: &mut Vec<super::CacheDecision>,
) -> Result<(Vec<u8>, NavBakeReport, Vec<u8>), String> {
    let fingerprint = fingerprint(level)?;
    if !request.force {
        let mut paths = vec![request.out.clone()];
        paths.extend(super::project::exported_cache(&request.source));
        for path in paths {
            let result = (|| {
                let mut reader = super::open_package(&path)?;
                crate::engine::package::project::select(&mut reader, Some(&level.id))?;
                let manifest = super::read_manifest(&mut reader)?;
                let current = read_current(&mut reader, &manifest, &fingerprint)?;
                super::verify_declared_entries(&path, &manifest)?;
                Ok::<_, String>(current)
            })();
            if let Ok((bytes, cached)) = result {
                warnings.extend(cached.warnings.iter().cloned());
                let provenance = serde_json::to_vec(&cached).map_err(|e| e.to_string())?;
                decisions.push(super::CacheDecision {
                    stage: "navigation".into(),
                    status: "hit".into(),
                    reasons: vec![
                        "Physical inputs, compiler identity and declared records verified".into(),
                    ],
                });
                return Ok((bytes, cached.report, provenance));
            }
        }
    }
    decisions.push(super::CacheDecision {
        stage: "navigation".into(),
        status: if request.force { "bypass" } else { "miss" }.into(),
        reasons: vec!["Navigation input identity unavailable or changed".into()],
    });
    let mut navigation_warnings = Vec::new();
    let (bytes, report) = super::bake_navigation(
        level,
        request.workers.min(super::MAX_WORKERS),
        &mut navigation_warnings,
    )?;
    let provenance = serde_json::to_vec(&Cached {
        fingerprint,
        report: report.clone(),
        warnings: navigation_warnings.clone(),
    })
    .map_err(|e| e.to_string())?;
    warnings.extend(navigation_warnings);
    Ok((bytes, report, provenance))
}
