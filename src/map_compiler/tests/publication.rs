//! Data-loss regressions use only tiny editable files and ZIP entries. They
//! exercise the final guard after encoding without assets, a bake or a GPU.
use super::super::publication::Guard;
use crate::engine::package::archive::{self, PendingEntry};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

// The four implicit pipeline routes need every cabinet part recorded even
// in an open room where conduits do not render. Decode only the six tiny GLBs.
#[test]
fn electrical_panel_export_records_every_rig_dependency() -> Result<(), String> {
    use crate::engine::{electrical_panel as panel, level::LevelDef};

    let root = crate::engine::props::resolve_prop_root().ok_or("shipped asset root")?;
    let assets = super::super::AssetInputs::read(&root)?;
    let scene = LevelDef::from_json(
        r#"{"schema":"places-level","format_version":1,
        "id":"cabinet","name":"Cabinet","spawn":{"x":1,"z":1},
        "rooms":[{"width":5,"depth":5,"height":3,"ceiling":{"kind":"open"}}],
        "walls":[{"x":0,"z":0,"width":5,"depth":0.198,"height":3}],
        "props":[{"model":"maintenance:electrical_panel","x":2,"z":0.2,"y":1.3,
        "electrical_panel":{},"solid":true,"components":[{"component":"interactable"}]}]}"#,
    )
    .map_err(|error| error.to_string())?;
    crate::engine::project::validate_level_assets(&scene, assets.catalog().assets())?;
    let _scope = assets.scope();
    let mut warnings = Vec::new();
    let dependencies = super::super::collect_dependencies(&scene, &assets, &mut warnings)?;
    for path in panel::asset_paths() {
        assert!(
            dependencies.iter().any(|dep| dep.path == path),
            "compiled inputs retain exact {path}"
        );
    }
    let closure = assets.catalog_inputs(&scene)?;
    for (id, _) in panel::INTERNAL_MODELS {
        assert!(
            closure.entries.get(id).is_some_and(Option::is_some),
            "catalog identity covers hidden part {id}"
        );
    }
    Ok(())
}

struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _cleanup = fs::remove_dir_all(&self.0);
    }
}
fn fixture() -> Result<Fixture, String> {
    let root = Fixture(std::env::temp_dir().join(format!(
        "places-publication-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    )));
    fs::create_dir(&root.0).map_err(|e| e.to_string())?;
    Ok(root)
}
fn entries() -> Vec<PendingEntry> {
    vec![PendingEntry {
        name: "manifest.json".into(),
        bytes: b"candidate".to_vec(),
    }]
}

#[test]
fn build_guard_rejects_aliases_and_preserves_concurrent_work_and_cancellation() -> Result<(), String>
{
    let root = fixture()?;
    let source = root.0.join("editable.placesmap");
    let out = root.0.join("out.placesmap");
    fs::write(&source, b"original source").map_err(|e| e.to_string())?;
    assert!(Guard::new(&source, &source).is_err());
    assert!(Guard::new(&source, &root.0.join("./editable.placesmap")).is_err());
    fs::hard_link(&source, &out).map_err(|e| e.to_string())?;
    assert!(Guard::new(&source, &out).is_err());
    fs::remove_file(&out).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&source, &out).map_err(|e| e.to_string())?;
        assert!(Guard::new(&source, &out).is_err());
        fs::remove_file(&out).map_err(|e| e.to_string())?;
    }
    fs::write(&out, b"prior output").map_err(|e| e.to_string())?;
    let guard = Guard::new(&source, &out)?;
    let source_change = archive::write_archive_guarded(
        &out,
        entries(),
        || Ok(()),
        || {
            fs::write(&source, b"new source").map_err(|e| e.to_string())?;
            guard.check()
        },
    );
    assert!(source_change.is_err());
    assert_eq!(fs::read(&source).map_err(|e| e.to_string())?, b"new source");
    assert_eq!(fs::read(&out).map_err(|e| e.to_string())?, b"prior output");
    let destination_guard = Guard::new(&source, &out)?;
    let output_change = archive::write_archive_guarded(
        &out,
        entries(),
        || Ok(()),
        || {
            fs::write(&out, b"external work").map_err(|e| e.to_string())?;
            destination_guard.check()
        },
    );
    assert!(output_change.is_err());
    let cancellation = archive::write_archive_guarded(
        &out,
        entries(),
        || Ok(()),
        || Err("cancelled after encoding".into()),
    );
    assert!(cancellation.is_err());
    assert_eq!(fs::read(&out).map_err(|e| e.to_string())?, b"external work");
    // A previously absent destination may also appear while encoding.
    fs::remove_file(&out).map_err(|e| e.to_string())?;
    let absent_guard = Guard::new(&source, &out)?;
    fs::write(&out, b"new external file").map_err(|e| e.to_string())?;
    assert!(absent_guard.check().is_err());
    Ok(())
}

#[test]
fn project_inputs_are_protected_while_display_edits_keep_member_identity() -> Result<(), String> {
    let root = fixture()?;
    fs::create_dir(root.0.join("levels")).map_err(|e| e.to_string())?;
    let level = root.0.join("levels/test_room.json");
    fs::write(
        &level,
        include_bytes!("../../../tests/fixtures/levels/test_room.json"),
    )
    .map_err(|e| e.to_string())?;
    let manifest = root.0.join("places.project.json");
    let mut metadata = crate::engine::project::ProjectManifest::from_json(br#"{"schema":"places-project", "project_format":1, "id":"project", "name":"Original", "entry_level":"test_room", "levels":[{"id":"test_room", "source":"levels/test_room.json"}]}"#)?;
    fs::write(
        &manifest,
        serde_json::to_vec(&metadata).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    assert!(Guard::new(&root.0, &manifest).is_err());
    assert!(Guard::new(&manifest, &level).is_err());
    let out = root.0.join("out.placesmap");
    fs::hard_link(&level, &out).map_err(|e| e.to_string())?;
    assert!(Guard::new(&root.0, &out).is_err());
    fs::remove_file(&out).map_err(|e| e.to_string())?;
    let guard = Guard::new(&root.0, &out)?;
    let original = super::super::project::read_source(&root.0, None)?.identity;
    metadata.name = "Renamed".into();
    metadata.author = "New author".into();
    metadata.description = "New description".into();
    metadata
        .levels
        .first_mut()
        .ok_or("Missing level")?
        .display_name = "Menu title".into();
    fs::write(
        &manifest,
        serde_json::to_vec(&metadata).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    assert!(
        guard.check().is_err(),
        "display edits still protect the full source snapshot"
    );
    assert_eq!(
        original,
        super::super::project::read_source(&root.0, None)?.identity
    );
    metadata.package_version =
        crate::engine::package::release::ReleaseVersion::try_from("1.0.1".to_string())?;
    fs::write(
        &manifest,
        serde_json::to_vec(&metadata).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    assert_ne!(
        original,
        super::super::project::read_source(&root.0, None)?.identity
    );
    Ok(())
}

fn asset_fixture(root: &std::path::Path) -> Result<crate::engine::level::LevelDef, String> {
    fixture_catalog(root, "disc.png")?;
    for (path, bytes) in [
        ("wall.png", b"wall"),
        ("disc.png", b"disc"),
        ("alt.png", b"disc"),
        ("background.png", b"base"),
    ] {
        fs::write(root.join(path), bytes).map_err(|e| e.to_string())?;
    }
    crate::engine::level::LevelDef::from_json(
        r#"{
        "schema":"places-level","format_version":1,"id":"asset_guard","name":"Asset guard",
        "spawn":{"x":0,"z":0},"defaults":{"wall":"core:wall"},
        "global_illuminators":[{"id":"sun","kind":"sun","direction":[0,-1,0],"visual":{}}]
    }"#,
    )
    .map_err(|e| e.to_string())
}

fn fixture_catalog(root: &std::path::Path, disc: &str) -> Result<(), String> {
    let catalog = serde_json::json!({"format_version":2_u32,"assets":[
        {"id":"core:wall","asset_class":"environment","asset_type":"material","source":"definition","texture":"core:wall_image"},
        {"id":"core:wall_image","asset_class":"environment","asset_type":"texture","source":"file","model":"wall.png"},
        {"id":"core:celestial_sun","asset_class":"environment","asset_type":"texture","source":"file","model":disc},
        {"id":"core:tex_white_fallback_01","asset_class":"environment","asset_type":"texture","source":"file","model":"background.png"}
    ]});
    fs::write(root.join("catalog.json"), catalog.to_string()).map_err(|e| e.to_string())
}

fn spooled_project(root: &std::path::Path) -> Result<Vec<archive::SpoolEntry>, String> {
    let path = super::project_archive::project_for_publication(root)?;
    let mut reader = super::super::open_package(&path)?;
    reader
        .names()
        .to_vec()
        .into_iter()
        .enumerate()
        .map(|(index, name)| {
            let bytes = reader.read_entry(&name, crate::engine::package::MAX_MANIFEST_BYTES)?;
            let entry_path = root.join(format!("record-{index}"));
            fs::write(&entry_path, &bytes).map_err(|error| error.to_string())?;
            Ok(archive::SpoolEntry {
                compressed: None,
                name,
                path: entry_path,
                bytes: u64::try_from(bytes.len()).map_err(|error| error.to_string())?,
                sha256: crate::engine::package::hash::sha256_hex(&bytes),
            })
        })
        .collect()
}

#[test]
fn asset_drift_preserves_the_previous_archive_after_encoding() -> Result<(), String> {
    use super::super::{AssetInputs, collect_dependencies};
    let root = fixture()?;
    let level = asset_fixture(&root.0)?;
    let source = root.0.join("editable.json");
    let out = root.0.join("out.placesmap");
    fs::write(&source, b"source identity").map_err(|e| e.to_string())?;
    fs::write(&out, b"prior output").map_err(|e| e.to_string())?;
    let publication = Guard::new(&source, &out)?;

    // A late change must reject the actual guarded rename without a build.
    let assets = AssetInputs::read(&root.0)?;
    let dependencies = collect_dependencies(&level, &assets, &mut Vec::new())?;
    assert!(dependencies.iter().any(|input| input.path == "wall.png"));
    publication.check_assets(&assets)?;
    let wall = assets.root().join("wall.png");
    let result = archive::write_archive_guarded(
        &out,
        entries(),
        || Ok(()),
        || {
            fs::write(&wall, b"edit").map_err(|e| e.to_string())?;
            publication.check_assets(&assets)
        },
    );
    assert!(result.is_err_and(|error| error.contains("wall.png")));
    assert_eq!(fs::read(&out).map_err(|e| e.to_string())?, b"prior output");

    fs::write(&wall, b"wall").map_err(|e| e.to_string())?;
    let disc = assets.root().join("disc.png");
    let project_change = archive::write_project_archive_files_guarded(
        &out,
        spooled_project(&root.0)?,
        || Ok(()),
        || {
            fs::write(&disc, b"edit").map_err(|e| e.to_string())?;
            publication.check_assets(&assets)
        },
    );
    assert!(project_change.is_err_and(|error| error.contains("disc.png")));
    assert_eq!(fs::read(&out).map_err(|e| e.to_string())?, b"prior output");
    fs::write(&disc, b"disc").map_err(|e| e.to_string())?;

    // A consumer that sees different bytes and falls back invalidates the
    // build even when the file is restored before the final live check.
    let _scope = assets.scope();
    assert_eq!(
        crate::engine::assets::read_asset_bytes(&wall, 8).map_err(|e| e.to_string())?,
        b"wall",
    );
    fs::write(&wall, b"edit").map_err(|e| e.to_string())?;
    assert!(crate::engine::assets::read_asset_bytes(&wall, 8).is_err());
    fs::write(&wall, b"wall").map_err(|e| e.to_string())?;
    assert!(
        publication
            .check_assets(&assets)
            .is_err_and(|error| error.contains("wall.png"))
    );
    assert_eq!(fs::read(&out).map_err(|e| e.to_string())?, b"prior output");
    Ok(())
}

#[test]
fn door_model_captured_before_dependency_collection_stays_bound_to_validation() -> Result<(), String>
{
    use super::super::AssetInputs;
    use crate::engine::assets::read_asset_bytes;
    use crate::engine::package::DependencyKind;

    let root = fixture()?;
    drop(asset_fixture(&root.0)?);
    let assets = AssetInputs::read(&root.0)?;
    let name = "door.glb";
    let model = assets.root().join(name);
    fs::write(&model, b"original").map_err(|error| error.to_string())?;
    let _scope = assets.scope();
    let reads = crate::engine::assets::reads::current().ok_or("Missing asset scope")?;

    reads.capture_validation_model(name)?;
    assert_eq!(
        read_asset_bytes(&model, 8).map_err(|error| error.to_string())?,
        b"original",
    );
    assert!(assets.check().is_ok());
    fs::write(&model, b"switched").map_err(|error| error.to_string())?;
    assert!(
        assets
            .dependency(DependencyKind::Model, name, &mut Vec::new())
            .is_err_and(|error| error.contains("Conflicting asset identities"))
    );

    // A different member can be validated from the same project without
    // appearing in the selected member's package dependency list.
    let other_member_assets = AssetInputs::read(&root.0)?;
    let _other_member_scope = other_member_assets.scope();
    let other_member_reads =
        crate::engine::assets::reads::current().ok_or("Missing asset scope")?;
    other_member_reads.capture_validation_model(name)?;
    fs::write(&model, b"original").map_err(|error| error.to_string())?;
    assert!(
        other_member_assets
            .check()
            .is_err_and(|error| error.contains(name))
    );
    Ok(())
}

#[test]
fn captured_catalog_drives_lighting_and_probe_reuse_independently() -> Result<(), String> {
    use super::super::{
        AssetInputs, BuildInputs, LightmapQuality, collect_dependencies, fingerprint,
        fingerprint_with_build_inputs, lighting_stage_fingerprint, probe_inputs_match, probes,
    };
    let root = fixture()?;
    let level = asset_fixture(&root.0)?;
    let original = AssetInputs::read(&root.0)?;
    // Rebind the disc to equal bytes before computing either key. The old
    // operation still uses the catalog and attachment it already captured.
    fixture_catalog(&root.0, "alt.png")?;
    let changed = AssetInputs::read(&root.0)?;
    let variants = [LightmapQuality::Full];
    let keys = |assets: &AssetInputs, capture_probes| -> Result<_, String> {
        let dependencies = collect_dependencies(&level, assets, &mut Vec::new())?;
        let inputs = BuildInputs {
            revision: 2,
            source_sha256: "source".into(),
            catalog_sha256: assets.catalog_hash().into(),
            catalog_inputs: Some(assets.catalog_inputs(&level)?),
            compiler_code_sha256: Some("tool".into()),
            tool_sha256: "tool".into(),
            capture_probes,
            probe_sky_sha256: capture_probes
                .then(|| probes::sky_fingerprint(&level, assets.catalog().assets(), &dependencies))
                .transpose()?,
            compact: false,
        };
        let full = fingerprint_with_build_inputs(
            &fingerprint("source", &variants, &dependencies),
            &inputs,
        );
        let lighting =
            lighting_stage_fingerprint(&level, &variants, &dependencies, &inputs, assets)?;
        Ok((full, lighting, inputs))
    };
    assert_eq!(
        original
            .catalog()
            .assets()
            .texture_path("core:celestial_sun"),
        Some("disc.png")
    );
    assert_eq!(
        original
            .catalog_inputs(&level)?
            .entries
            .get("core:celestial_sun")
            .and_then(Option::as_ref)
            .and_then(|entry| entry.get("model"))
            .and_then(serde_json::Value::as_str),
        Some("disc.png")
    );
    let (original_full, original_lighting, before) = keys(&original, true)?;
    let (changed_full, changed_lighting, after) = keys(&changed, true)?;
    assert_ne!(original_full, changed_full);
    assert_eq!(original_lighting, changed_lighting);
    let captures = [crate::engine::package::manifest::ProbePayload {
        face_edge: 64,
        levels: 1,
        count: 1,
        cubemaps: vec!["probe.ktx2".into()],
        positions: "positions.json".into(),
    }];
    assert!(probe_inputs_match(&before, &before, &captures));
    assert!(!probe_inputs_match(&before, &after, &captures));
    assert!(probe_inputs_match(&before, &after, &[]));
    let (_, before_no_capture, disabled_before) = keys(&original, false)?;
    let (_, after_no_capture, disabled_after) = keys(&changed, false)?;
    assert_eq!(before_no_capture, after_no_capture);
    assert!(probe_inputs_match(&disabled_before, &disabled_after, &[]));
    assert!(!probe_inputs_match(&disabled_before, &after, &captures));
    assert!(
        original
            .check()
            .is_err_and(|error| error.contains("catalog.json"))
    );
    changed.check()?;
    fs::write(root.0.join("alt.png"), b"disc edit").map_err(|e| e.to_string())?;
    let replacement = AssetInputs::read(&root.0)?;
    let (_, replacement_lighting, replaced) = keys(&replacement, true)?;
    assert_eq!(changed_lighting, replacement_lighting);
    assert!(!probe_inputs_match(&after, &replaced, &captures));
    assert!(
        changed
            .check()
            .is_err_and(|error| error.contains("alt.png"))
    );

    let dependencies = collect_dependencies(&level, &changed, &mut Vec::new())?;
    let mut exposure_edit = level.clone();
    exposure_edit
        .global_illuminators
        .first_mut()
        .and_then(|light| light.visual.as_mut())
        .ok_or("fixture has no celestial disc")?
        .brightness = 2.0;
    assert_ne!(
        probes::sky_fingerprint(&level, changed.catalog().assets(), &dependencies)?,
        probes::sky_fingerprint(&exposure_edit, changed.catalog().assets(), &dependencies)?,
    );
    Ok(())
}

// Rebind lift stock and remove the selected sky background while all old
// bytes remain readable. The package closure and compiler must reject the
// stale bindings even though the file-only checks pass. Tiny temp records
// and in-memory level variants exercise this without decoding or baking.
#[test]
fn implicit_renderer_catalog_roots_follow_the_captured_bindings() -> Result<(), String> {
    use super::super::{AssetInputs, collect_dependencies, probes};
    use crate::engine::{
        assets::CELESTIAL_BACKGROUND_TEXTURE,
        elevator::{ElevatorDef, MODEL_IDS, Stop},
        package::{catalog_inputs::CatalogInputs, dependencies},
    };

    let root = fixture()?;
    let mut level = asset_fixture(&root.0)?;
    let mut unused = level.clone();
    unused.global_illuminators.clear();
    level.elevators.push(ElevatorDef {
        id: "test_lift".into(),
        x: 0.0,
        z: 0.0,
        rotation_degrees: 0.0,
        stops: vec![Stop {
            label: "G".into(),
            y: 0.0,
            level: None,
            arrival_elevator: None,
        }],
        initial_stop: 0,
        speed_mps: 1.5,
    });
    let catalog_path = root.0.join("catalog.json");
    let mut document = fixture_lift_catalog(&root.0)?;
    fs::write(&catalog_path, document.to_string()).map_err(|error| error.to_string())?;
    let before = AssetInputs::read(&root.0)?;
    let old_files = collect_dependencies(&level, &before, &mut Vec::new())?;
    let used_inputs = before.catalog_inputs(&level)?;
    for id in MODEL_IDS.into_iter().chain([CELESTIAL_BACKGROUND_TEXTURE]) {
        assert!(
            used_inputs.entries.get(id).is_some_and(Option::is_some),
            "{id}"
        );
    }
    let probe_key = |assets: &AssetInputs, files: &[crate::engine::package::PackageDependency]| {
        probes::sky_fingerprint(&level, assets.catalog().assets(), files)
    };

    let entries = fixture_asset_entries(&mut document)?;
    let cabin = MODEL_IDS.first().ok_or("elevator model IDs missing")?;
    let model = entries
        .iter_mut()
        .find(|value| value.get("id").and_then(serde_json::Value::as_str) == Some(*cabin))
        .and_then(|entry| entry.get_mut("model"))
        .ok_or("test cabin binding missing")?;
    *model = "replacement.glb".into();
    fs::write(root.0.join("replacement.glb"), b"lift").map_err(|error| error.to_string())?;
    fs::write(&catalog_path, document.to_string()).map_err(|error| error.to_string())?;
    let rebound = AssetInputs::read(&root.0)?;
    let rebound_files = collect_dependencies(&level, &rebound, &mut Vec::new())?;
    assert!(
        rebound_files
            .iter()
            .any(|file| file.path == "replacement.glb")
    );
    assert!(!rebound_files.iter().any(|file| file.path == "lift-0.glb"));
    assert_ne!(used_inputs, rebound.catalog_inputs(&level)?);
    assert_eq!(
        probe_key(&before, &old_files)?,
        probe_key(&rebound, &rebound_files)?
    );

    let retained = fixture_asset_entries(&mut document)?;
    retained.retain(|entry| {
        entry.get("id").and_then(serde_json::Value::as_str) != Some(CELESTIAL_BACKGROUND_TEXTURE)
    });
    let without_background = document.to_string();
    fs::write(&catalog_path, &without_background).map_err(|error| error.to_string())?;
    let after = AssetInputs::read(&root.0)?;
    let new_files = collect_dependencies(&level, &after, &mut Vec::new())?;
    assert!(!new_files.iter().any(|file| file.path == "background.png"));
    assert_ne!(
        probe_key(&rebound, &rebound_files)?,
        probe_key(&after, &new_files)?
    );
    let installed = CatalogInputs::for_level(&level, without_background.as_bytes())?;
    assert_eq!(installed, after.catalog_inputs(&level)?);
    assert_eq!(
        installed.entries.get(CELESTIAL_BACKGROUND_TEXTURE),
        Some(&None)
    );
    assert_ne!(rebound.catalog_inputs(&level)?, installed);
    assert_eq!(
        before.catalog_inputs(&unused)?,
        after.catalog_inputs(&unused)?
    );
    for dependency in old_files {
        dependencies::validate(before.root(), &dependency)?;
    }
    assert!(
        before
            .check()
            .is_err_and(|error| error.contains("catalog.json"))
    );
    after.check()?;
    Ok(())
}

fn fixture_asset_entries(
    catalog: &mut serde_json::Value,
) -> Result<&mut Vec<serde_json::Value>, String> {
    catalog
        .get_mut("assets")
        .and_then(serde_json::Value::as_array_mut)
        .ok_or_else(|| "test assets array missing".into())
}

fn fixture_lift_catalog(root: &std::path::Path) -> Result<serde_json::Value, String> {
    use crate::engine::elevator::MODEL_IDS;
    let mut document: serde_json::Value = serde_json::from_slice(
        &fs::read(root.join("catalog.json")).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    let entries = fixture_asset_entries(&mut document)?;
    for (index, id) in MODEL_IDS.into_iter().enumerate() {
        let path = format!("lift-{index}.glb");
        fs::write(root.join(&path), b"lift").map_err(|error| error.to_string())?;
        entries.push(serde_json::json!({"id":id,"asset_class":"core","asset_type":"mechanism","source":"file","model":path,"size":[1_u32,1_u32,1_u32]}));
    }
    Ok(document)
}
