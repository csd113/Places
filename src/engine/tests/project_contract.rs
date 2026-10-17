//! Three compact regressions protect the shared source contract, malformed
//! metadata and filesystem confinement/build selection. One tiny generated
//! scene is reused throughout; no shipped map, GPU, bake or imported asset is
//! needed. Cost: small JSON allocations and one short-lived temporary folder.

use std::path::PathBuf;

use crate::engine::assets::AssetCatalog;
use crate::engine::level::LevelDef;
use crate::engine::materials::MaterialTable;
use crate::engine::project::{
    BuiltinAssetKind, BuiltinAssetRef, ProjectFolder, ProjectManifest, ProjectPath,
};

fn scene() -> serde_json::Value {
    serde_json::json!({
        "schema":"places-level", "format_version":1, "id":"entry", "name":"Contract room",
        "spawn":{"x":1,"z":1},
        "defaults":{"wall":"core:stone", "floor":"core:stone", "ceiling":"core:stone"},
        "rooms":[{"width":2,"depth":2,"height":2.5}]
    })
}

fn manifest() -> serde_json::Value {
    serde_json::json!({
        "schema":"places-project", "project_format":1, "id":"contract", "name":"Contract",
        "entry_level":"entry", "levels":[{"id":"entry", "source":"levels/entry.json"}]
    })
}

fn set(object: &mut serde_json::Value, key: &str, value: serde_json::Value) -> Result<(), String> {
    drop(
        object
            .as_object_mut()
            .ok_or("fixture is not an object")?
            .insert(key.into(), value),
    );
    Ok(())
}

fn json_bytes(value: &serde_json::Value) -> Result<Vec<u8>, String> {
    serde_json::to_vec(value).map_err(|error| error.to_string())
}

#[test]
fn project_round_trip_uses_engine_material_rules() -> Result<(), String> {
    let mut metadata = manifest();
    set(&mut metadata, "thumbnail", "thumbnails/project.png".into())?;
    set(
        &mut metadata,
        "scripts",
        serde_json::json!(["scripts/project.luau"]),
    )?;
    let parsed = ProjectManifest::from_json(&json_bytes(&metadata)?)?;
    let encoded = serde_json::to_vec(&parsed).map_err(|error| error.to_string())?;
    assert_eq!(
        parsed,
        ProjectManifest::from_json(&encoded)?,
        "metadata survives editor serialization"
    );

    let catalog = AssetCatalog::from_json_str(
        r#"{"format_version":2,"assets":[
        {"id":"core:tex_stone","display_name":"Stone PNG","asset_class":"core","asset_type":"texture","source":"file","model":"shared/stone.png"},
        {"id":"core:stone","display_name":"Stone","asset_class":"core","asset_type":"material","source":"definition","texture":"core:tex_stone","tile_metres":2.0}
    ]}"#,
    )?;
    let mut level = LevelDef::from_json(&scene().to_string()).map_err(|error| error.to_string())?;
    crate::engine::loader::validate_level(&level)?;
    crate::map_compiler::prepare_level(&mut level, &catalog, None);
    crate::engine::project::validate_level_assets(&level, &catalog)?;
    let serialized = serde_json::to_string(&level).map_err(|error| error.to_string())?;
    let decoded = LevelDef::from_json(&serialized).map_err(|error| error.to_string())?;
    let materials = MaterialTable::logical(&decoded, &catalog, None);
    assert_eq!(
        materials.len(),
        1,
        "default/room references share one canonical material"
    );
    assert_eq!(
        materials.entries().first().map(|entry| entry.id.as_str()),
        Some("core:stone"),
        "compiler and renderer retain the authored catalog ID"
    );
    assert!(
        BuiltinAssetRef {
            kind: BuiltinAssetKind::Model,
            id: "core:stone".into()
        }
        .validate(&catalog, None)
        .is_err(),
        "material IDs must not resolve as model IDs"
    );
    Ok(())
}

#[test]
fn malformed_or_unsupported_contracts_fail_before_use() -> Result<(), String> {
    for (key, value) in [
        ("schema", serde_json::json!("other")),
        ("project_format", serde_json::json!(2_u32)),
        ("entry_level", serde_json::json!("missing")),
        ("unexpected", serde_json::json!(true)),
        ("levels", serde_json::json!([])),
        (
            "levels",
            serde_json::json!([{"id":"entry", "source":"../outside.json"}]),
        ),
        (
            "levels",
            serde_json::json!([{"id":"entry", "source":"scripts/entry.json"}]),
        ),
        ("scripts", serde_json::json!(["scripts/entry.lua"])),
    ] {
        let mut invalid = manifest();
        set(&mut invalid, key, value)?;
        assert!(
            ProjectManifest::from_json(&json_bytes(&invalid)?).is_err(),
            "invalid project field {key} must fail"
        );
    }
    for path in [
        "/absolute.json",
        "a//b",
        "a/./b",
        "a/../b",
        "C:/map.json",
        "a\\b",
        "levels/CON.json",
        "levels/COM¹.json",
        "scripts/LPT².luau",
        "a/file. ",
    ] {
        assert!(
            ProjectPath::new(path.into()).is_err(),
            "unsafe path {path} must fail portably"
        );
    }
    let mut unsupported = scene();
    set(&mut unsupported, "format_version", 3_u32.into())?;
    let result = LevelDef::from_json(&unsupported.to_string())
        .map_err(|error| error.to_string())
        .and_then(|level| crate::engine::loader::validate_level(&level));
    assert!(
        result.is_err(),
        "old source versions must not silently load"
    );
    let mut oversized = ProjectManifest::from_json(&json_bytes(&manifest())?)?;
    let mut repeated = oversized
        .levels
        .first()
        .ok_or("missing fixture level")?
        .clone();
    repeated.description = "x".repeat(4096);
    for index in 0..65_u8 {
        repeated.id = format!("map_{index}");
        repeated.source = ProjectPath::new(format!("levels/map_{index}.json"))?;
        oversized.levels.push(repeated.clone());
    }
    assert!(
        oversized
            .validate()
            .is_err_and(|error| error.contains("256 KiB")),
        "directly authored metadata must serialize within its reader's byte limit"
    );
    Ok(())
}

struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _cleanup = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn project_selection_and_paths_stay_inside_the_folder() -> Result<(), String> {
    let root = std::env::temp_dir().join(format!("places-project-contract-{}", std::process::id()));
    std::fs::create_dir(&root).map_err(|error| error.to_string())?;
    let fixture = Fixture(root);
    for directory in ["levels", "output"] {
        std::fs::create_dir(fixture.0.join(directory)).map_err(|error| error.to_string())?;
    }
    let path = fixture.0.join("places.project.json");
    std::fs::write(&path, json_bytes(&manifest())?).map_err(|error| error.to_string())?;
    let source = fixture.0.join("levels/entry.json");
    std::fs::write(&source, json_bytes(&scene())?).map_err(|error| error.to_string())?;
    let project = ProjectFolder::open(&fixture.0)?;
    let (_, _, level) = project.read_level(None)?;
    assert_eq!(
        level.id, "entry",
        "default selection uses the declared entry level"
    );
    assert!(
        project.read_level(Some("missing")).is_err(),
        "missing selection must fail"
    );
    let template = crate::map_compiler::BuildRequest {
        source: path,
        project_level: None,
        out: fixture.0.join("output"),
        asset_root: PathBuf::new(),
        variants: vec![crate::engine::quality::LightmapQuality::Off],
        workers: 1,
        force: false,
        capture_probes: false,
    };
    let plan = crate::map_compiler::project::build_plan(&template)?;
    assert_eq!(plan.len(), 1, "planning only includes declared maps");
    assert_eq!(
        plan.first()
            .and_then(|request| request.project_level.as_deref()),
        Some("entry"),
        "planning retains stable project selection"
    );
    let mut mismatch = scene();
    set(&mut mismatch, "id", "different".into())?;
    std::fs::write(&source, json_bytes(&mismatch)?).map_err(|error| error.to_string())?;
    assert!(
        project.read_level(None).is_err(),
        "manifest and source IDs must agree"
    );
    assert!(
        crate::map_compiler::project::build_plan(&template).is_err(),
        "bad source aborts planning before publication"
    );
    #[cfg(unix)]
    {
        let escaped = fixture.0.join("levels/escape.json");
        std::os::unix::fs::symlink("/etc/hosts", &escaped).map_err(|error| error.to_string())?;
        assert!(
            project
                .resolve(&ProjectPath::new("levels/escape.json".into())?)
                .is_err(),
            "symlinks must not escape project root"
        );
    }
    Ok(())
}
