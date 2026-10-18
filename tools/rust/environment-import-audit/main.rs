//! Explicit developer import/diagnostic workflow. No rendering or light baking.
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use places::editor::document::Document;
use places::editor::safety::{self, SafetyReport};
use places::engine::package::hash::sha256_hex;
use serde_json::{Value, json};

fn write_json(path: &Path, value: &Value) -> Result<(), String> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    bytes.push(b'\n');
    fs::write(path, bytes).map_err(|error| error.to_string())
}

fn report_json(report: &SafetyReport) -> Value {
    json!({
        "limitations": report.limitations,
        "findings": report.findings.iter().map(|entry| json!({
            "check":entry.check,"category":entry.category.label(),
            "severity":entry.severity.label(),"title":entry.title,"risk":entry.risk,
            "position":entry.position,"bounds":{"min":entry.bounds.min,"max":entry.bounds.max},
            "objects":entry.objects.iter().map(|object|format!("{object:?}")).collect::<Vec<_>>(),
            "correction":entry.fix.as_ref().map(|fix|&fix.title),
            "placement_ignore":entry.placement_ignore.as_ref().map(|state|json!({"key":state.key,"active":state.active}))
        })).collect::<Vec<_>>()
    })
}

fn offline_report(document: &Document, path: &Path) -> Result<Value, String> {
    let mut prepared = document.level().clone();
    let catalog = places::engine::loader::PropCatalog::load_default();
    places::map_compiler::prepare_level(&mut prepared, catalog.assets(), None);
    let checked =
        places::map_compiler::geometry_check::check_level(&prepared, "Imported legacy map", true);
    let summary = json!({"errors":checked.error_count(),"warnings":checked.warning_count(),
        "suppressed_by_preserved_geometry_intent":checked.suppressed});
    write_json(
        path,
        &json!({"summary":summary,"findings":checked.findings,
        "scope":"Explicit production architecture checker; placeholder prop envelopes, no GLB triangle comparisons or light baking"}),
    )?;
    Ok(summary)
}

fn import_one(source: &Path, output: &Path) -> Result<Value, String> {
    let original = places::engine::project::read_bounded_file(
        source,
        places::engine::level::MAX_LEVEL_JSON_BYTES,
    )?;
    let original_raw: Value =
        serde_json::from_slice(&original).map_err(|error| error.to_string())?;
    let destination = output
        .join("imports")
        .join(source.file_stem().ok_or("Map source needs a filename")?);
    let mut document = Document::create(&destination, "import_entry", "Import audit")?;
    let id = document.import_level(source)?;
    let imported = destination.join("levels").join(format!("{id}.json"));
    if fs::read(&imported).map_err(|error| error.to_string())? != original {
        return Err("JSON import changed original source bytes".into());
    }
    let before = safety::analyze(document.level(), document.raw());
    write_json(
        &destination.join("diagnostics-before.json"),
        &report_json(&before),
    )?;
    let mut corrections = Vec::new();
    let mut current = before;
    while let Some(finding) = current.findings.iter().find(|entry| {
        entry.fix.is_some()
            && !entry
                .placement_ignore
                .as_ref()
                .is_some_and(|state| state.active)
    }) {
        let expected = document.raw().clone();
        let candidate = safety::preview_corrections(&expected, std::slice::from_ref(finding))
            .into_iter()
            .next()
            .ok_or("Correction disappeared")?;
        match candidate.and_then(|raw| document.apply_checked_edit(&expected, raw)) {
            Ok(()) => corrections.push(json!({"finding":finding.title,"result":"applied"})),
            Err(error) => {
                corrections
                    .push(json!({"finding":finding.title,"result":"refused","reason":error}));
                break;
            }
        }
        current = safety::analyze(document.level(), document.raw());
    }
    document.save_all()?;
    let mut reopened = Document::open(&destination)?;
    reopened.select_level(&id)?;
    if reopened.raw() != document.raw() {
        return Err("Save/reopen changed imported semantics".into());
    }
    let catalog = places::engine::loader::PropCatalog::load_default();
    document.validate_assets(catalog.assets())?;
    let offline = offline_report(&document, &destination.join("diagnostics-offline.json"))?;
    let after = safety::analyze(document.level(), document.raw());
    write_json(
        &destination.join("diagnostics-after.json"),
        &report_json(&after),
    )?;
    let baseline = output.join("migrated").join(format!("{id}.json"));
    let _copied = fs::copy(&imported, &baseline).map_err(|error| error.to_string())?;
    let receipt = json!({"source":source,"source_sha256":sha256_hex(&original),
        "project":destination,"imported_source":imported,"baseline":baseline,"level_id":id,
        "import_preserved_bytes":true,"save_reopen_equal":true,"asset_validation":true,
        "before_findings":before_count(&destination)?,"after_findings":after.findings.len(),
        "original_placement_intent":document.raw().get("editor_placement_intent"),
        "offline_geometry":offline,
        "geometry_intent_count":original_raw.get("geometry_intent").and_then(Value::as_array).map_or(0, Vec::len),
        "geometry_intent_preserved":original_raw.get("geometry_intent")==document.raw().get("geometry_intent"),
        "corrections":corrections,"manual_edits":[],
        "migrated_sha256":sha256_hex(&fs::read(&baseline).map_err(|error|error.to_string())?)
    });
    write_json(&destination.join("receipt.json"), &receipt)?;
    writeln!(
        std::io::stdout(),
        "{}",
        json!({"ready":baseline,"findings":after.findings.len()})
    )
    .map_err(|error| error.to_string())?;
    Ok(receipt)
}

fn before_count(destination: &Path) -> Result<usize, String> {
    let raw: Value = serde_json::from_slice(
        &fs::read(destination.join("diagnostics-before.json"))
            .map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    Ok(raw
        .get("findings")
        .and_then(Value::as_array)
        .map_or(0, Vec::len))
}

fn ignored_correction_probe(output: &Path, legacy: &Path) -> Result<Value, String> {
    let root = output.join("ignore-probe");
    let mut document = Document::create(&root, "probe_entry", "Placement ignore import proof")?;
    let id = document.import_level(legacy)?;
    let original_wall_count = document.level().walls.len();
    inject_reviewed_duplicates(&mut document)?;
    let report = safety::analyze(document.level(), document.raw());
    let reviewed = report
        .findings
        .iter()
        .find(|finding| finding.fix.is_some())
        .ok_or("Probe needs an actual exact-wall correction")?;
    let expected = document.raw().clone();
    let candidate = safety::preview_corrections(&expected, std::slice::from_ref(reviewed))
        .into_iter()
        .next()
        .ok_or("Actual correction preview")??;
    if document.raw() != &expected {
        return Err("Preview mutated source".into());
    }
    document.apply_checked_edit(&expected, candidate)?;
    if !document.undo() || document.raw() != &expected {
        return Err("Correction undo lost source".into());
    }
    document.ignore_safety_finding(reviewed)?;
    document.save_all()?;
    let protected_source = root.join("levels").join(format!("{id}.json"));
    let source_bytes = fs::read(&protected_source).map_err(|error| error.to_string())?;
    let mut imported = Document::create(
        &output.join("ignore-probe-import"),
        "probe_entry",
        "Import proof",
    )?;
    let _id = imported.import_level(&protected_source)?;
    let imported_report = safety::analyze(imported.level(), imported.raw());
    let active = imported_report
        .findings
        .iter()
        .filter(|entry| {
            entry
                .placement_ignore
                .as_ref()
                .is_some_and(|state| state.active)
        })
        .count();
    let candidates = safety::preview_corrections(imported.raw(), &imported_report.findings);
    let direct_refused = imported_report
        .findings
        .iter()
        .filter(|entry| {
            entry
                .placement_ignore
                .as_ref()
                .is_some_and(|state| state.active)
        })
        .filter_map(|entry| entry.fix.as_ref())
        .all(|fix| fix.preview(imported.raw()).is_err());
    if active != 1 || candidates.len() != 1 || !direct_refused {
        return Err("Imported intent did not isolate correction eligibility".into());
    }
    let imported_expected = imported.raw().clone();
    imported.apply_checked_edit(
        &imported_expected,
        candidates
            .into_iter()
            .next()
            .ok_or("Unprotected correction")??,
    )?;
    imported.save_all()?;
    let after = safety::analyze(imported.level(), imported.raw());
    if !safety::preview_corrections(imported.raw(), &after.findings).is_empty() {
        return Err("Autofix offered protected correction after independent fix".into());
    }
    let receipt = json!({"legacy_source":legacy,"imported_original_sha256":sha256_hex(&source_bytes),
        "active_ignores":active,"unprotected_candidates":1_u32,"individual_protected_fix_refused":direct_refused,
        "preview_did_not_mutate":true,"apply_undo_exact":true,
        "independent_fix_applied":true,"protected_walls_retained":imported.level().walls.len()==original_wall_count.saturating_add(1),
        "remaining_autofix_candidates":0_u32,"diagnostics":report_json(&after),
        "deliberately_injected_defects_in_isolated_legacy_copy":true});
    write_json(&output.join("ignore-probe-receipt.json"), &receipt)?;
    Ok(receipt)
}

fn inject_reviewed_duplicates(document: &mut Document) -> Result<(), String> {
    let expected = document.raw().clone();
    let simple: Vec<_> = expected
        .get("walls")
        .and_then(Value::as_array)
        .ok_or("Legacy walls")?
        .iter()
        .filter(|wall| {
            wall.as_object().is_some_and(|record| {
                record.keys().all(|key| {
                    matches!(
                        key.as_str(),
                        "x" | "y" | "z" | "width" | "depth" | "height" | "material" | "shine"
                    )
                })
            })
        })
        .cloned()
        .collect();
    if simple.len() < 2 {
        return Err("Probe requires two independent simple legacy walls".into());
    }
    let mut candidate = expected.clone();
    let walls = candidate
        .get_mut("walls")
        .and_then(Value::as_array_mut)
        .ok_or("Legacy walls")?;
    walls.push(simple.first().ok_or("First simple wall")?.clone());
    walls.push(simple.last().ok_or("Last simple wall")?.clone());
    document.apply_checked_edit(&expected, candidate)
}

fn review_only(arguments: &mut impl Iterator<Item = std::ffi::OsString>) -> Result<(), String> {
    let project = PathBuf::from(
        arguments
            .next()
            .ok_or("Review needs project, level ID and NEW output directory")?,
    );
    let id = arguments
        .next()
        .ok_or("Review needs level ID")?
        .into_string()
        .map_err(|invalid| format!("Level ID must be UTF-8 ({} encoded bytes)", invalid.len()))?;
    let output = PathBuf::from(
        arguments
            .next()
            .ok_or("Review needs NEW output directory")?,
    );
    let mut document = Document::open(&project)?;
    document.select_level(&id)?;
    let (source_path, source_before, _level) = document.project().read_level(Some(&id))?;
    fs::create_dir(&output).map_err(|error| error.to_string())?;
    let report = safety::analyze(document.level(), document.raw());
    write_json(&output.join("diagnostics.json"), &report_json(&report))?;
    let offline = offline_report(&document, &output.join("diagnostics-offline.json"))?;
    let source_after = places::engine::project::read_bounded_file(
        &source_path,
        places::engine::level::MAX_LEVEL_JSON_BYTES,
    )?;
    if source_before != source_after {
        return Err("Source changed during read-only review".into());
    }
    write_json(
        &output.join("receipt.json"),
        &json!({"project":project,"level":id,
        "read_only":true,"source_sha256":sha256_hex(&source_before),"source_unchanged":true,
        "findings":report.findings.len(),"offline":offline}),
    )
}

fn main() -> Result<(), String> {
    let mut arguments = std::env::args_os().skip(1);
    let first = arguments
        .next()
        .ok_or("Pass NEW output directory then old JSON maps, or --review PROJECT LEVEL OUT")?;
    if first == "--review" {
        return review_only(&mut arguments);
    }
    let output = PathBuf::from(first);
    fs::create_dir(&output).map_err(|error| error.to_string())?;
    for child in ["imports", "migrated"] {
        fs::create_dir(output.join(child)).map_err(|error| error.to_string())?;
    }
    let mut inventory = Vec::new();
    let mut first_source = None;
    for source in arguments {
        if first_source.is_none() {
            first_source = Some(PathBuf::from(&source));
        }
        inventory.push(import_one(Path::new(&source), &output)?);
        write_json(
            &output.join("import-inventory.json"),
            &json!({"environments":inventory}),
        )?;
    }
    let intent = ignored_correction_probe(
        &output,
        &first_source.ok_or("At least one old map is required")?,
    )?;
    write_json(
        &output.join("import-inventory.json"),
        &json!({"environments":inventory,"intent_probe":intent}),
    )
}
