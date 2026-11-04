//! Navigation reuse follows physical authoring inputs, independently of light
//! quality, project menus and final archive encoding. No unverified cache data
//! can enter a package; legacy packages simply take the normal bake path.
use super::{BuildRequest, navigation::NavBakeReport};
use crate::engine::level::LevelDef;
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
                let entry = manifest
                    .entries
                    .iter()
                    .find(|entry| entry.name == ENTRY)
                    .ok_or("No navigation input identity")?;
                let cached: Cached = serde_json::from_slice(&super::read_declared_entry(
                    &mut reader,
                    &manifest,
                    entry,
                )?)
                .map_err(|e| e.to_string())?;
                if cached.fingerprint != fingerprint {
                    return Err("Navigation inputs changed".into());
                }
                super::verify_declared_entries(&path, &manifest)?;
                let variant = manifest.variants.first().ok_or("No navigation variant")?;
                let bytes = reader.read_blob(
                    &variant.entries.navigation,
                    crate::engine::package::MAX_NAVIGATION_BYTES,
                )?;
                drop(crate::engine::package::navigation::read_navigation(&bytes)?);
                Ok::<_, String>((bytes, cached))
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
