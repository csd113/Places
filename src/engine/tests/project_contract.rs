//! Three compact regressions protect the shared source contract, malformed
//! metadata and filesystem confinement/build selection. One tiny generated
//! scene is reused throughout; no shipped map, GPU, bake or imported asset is
//! needed. Cost: small JSON allocations and one short-lived temporary folder.

use std::path::PathBuf;

use crate::engine::assets::{AssetCatalog, AssetSource};
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
    for (key, value) in [
        ("schema", serde_json::json!("other")),
        ("transitions", serde_json::json!(["next", "next"])),
        ("rooms", serde_json::json!([{"width":0_u32,"depth":4_u32}])),
        ("props", serde_json::json!([{"model":""}])),
    ] {
        let mut invalid = scene();
        set(&mut invalid, key, value)?;
        let result = LevelDef::from_json(&invalid.to_string())
            .map_err(|error| error.to_string())
            .and_then(|level| crate::engine::loader::validate_level(&level));
        assert!(result.is_err(), "invalid level field {key} must fail");
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
        compact: false,
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

// This extends the no-bake source gate beyond check-project, which validates
// schema, scripts and ownership but does not resolve every built-in asset ID.
// Read the four shipping projects and stat file-backed catalog entries. Do not
// decode, reproduce or bake the assets.
#[test]
fn maintained_release_sources_resolve_builtin_assets_without_preparation() -> Result<(), String> {
    #[derive(serde::Deserialize)]
    struct ReleaseProject {
        source: String,
    }
    #[derive(serde::Deserialize)]
    struct Inventory {
        projects: Vec<ReleaseProject>,
    }

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let assets = root.join("assets");
    let asset_catalog = AssetCatalog::load_from_path(&assets.join("catalog.json"))
        .ok_or("shipped catalog or gameplay metadata did not load")?;
    for entry in asset_catalog.entries() {
        if entry.source == AssetSource::File {
            let relative = entry
                .model
                .as_deref()
                .ok_or_else(|| format!("file-backed catalog asset {} has no path", entry.id))?;
            assert!(
                assets.join(relative).is_file(),
                "file-backed catalog asset {} is missing: {relative}",
                entry.id
            );
        }
    }
    let inventory: Inventory = serde_json::from_slice(
        &std::fs::read(root.join("release/release-projects.json"))
            .map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    assert!(
        !inventory.projects.is_empty(),
        "the release inventory must declare gameplay projects"
    );
    for project in inventory
        .projects
        .iter()
        .map(|record| record.source.as_str())
        .chain(["projects/menu"])
    {
        let folder = ProjectFolder::open(&root.join(project))?;
        for member in &folder.manifest().levels {
            let (_, _, level) = folder.read_level(Some(&member.id))?;
            crate::engine::project::validate_level_assets(&level, &asset_catalog)
                .map_err(|error| format!("{project}/{}: {error}", member.id))?;
        }
    }
    Ok(())
}

fn weather_switch(action: &serde_json::Value) -> serde_json::Value {
    serde_json::json!([{
        "id":"snow_switch","model":"core:switch","x":1.0_f32,"z":1.0_f32,
        "components":[{"component":"interactable"}],
        "bindings":[{"on":"interact","actions":[action]}]
    }])
}

fn admit_weather(raw: &serde_json::Value) -> Result<(), String> {
    let level = LevelDef::from_json(&raw.to_string()).map_err(|error| error.to_string())?;
    crate::engine::loader::validate_level(&level)
}

// Keep the existing WeatherTests negative cases against the actual loader
// admission path. One tiny in-memory room, no assets, files, rendering or bake.
#[test]
fn authored_weather_checks_endpoints_and_interaction_admission() -> Result<(), String> {
    let mut baseline = scene();
    let base_weather = serde_json::json!({"kind":"snow"});
    let storm =
        serde_json::json!({"kind":"snow","storm_severity":1.0_f32,"wind":[8.0_f32,3.0_f32]});
    let cycle = serde_json::json!({
        "enabled":false, "min_strength":0.0_f32, "max_strength":0.45_f32,
        "period_seconds":120.0_f32, "transition_seconds":15.0_f32
    });
    set(&mut baseline, "weather", base_weather)?;
    set(&mut baseline, "weather_alternate", storm)?;
    set(&mut baseline, "weather_cycle", cycle)?;
    admit_weather(&baseline)?;
    for action in [
        serde_json::json!({"action":"toggle_weather"}),
        serde_json::json!({"action":"set_weather_strength","strength":0.35_f32,"transition_seconds":2.0_f32}),
        serde_json::json!({"action":"set_weather_cycle","enabled":true}),
        serde_json::json!({"action":"set_prompt","prompt":"Enable blizzard"}),
    ] {
        let mut authored = baseline.clone();
        set(&mut authored, "props", weather_switch(&action))?;
        admit_weather(&authored)?;
    }
    weather_endpoint_refusals(&baseline)?;
    weather_interaction_refusals(&baseline)?;
    let mut no_default = baseline.clone();
    set(&mut no_default, "weather", serde_json::Value::Null)?;
    assert!(
        admit_weather(&no_default)
            .is_err_and(|error| error.contains("requires authored default weather")),
        "an alternate without a default must fail"
    );
    let mut no_shelter_budget = baseline;
    set(
        &mut no_shelter_budget,
        "rooms",
        serde_json::json!(vec![
            serde_json::json!({
                "width":2.0_f32,"depth":2.0_f32,"height":2.5_f32
            });
            33
        ]),
    )?;
    assert!(
        admit_weather(&no_shelter_budget).is_err_and(|error| error.contains("at most 32")),
        "storm shelter count must be bounded"
    );
    Ok(())
}

fn weather_endpoint_refusals(baseline: &serde_json::Value) -> Result<(), String> {
    for (field, key, value, expected) in [
        ("weather", "count", 0_u32.into(), "weather count"),
        ("weather", "radius", 33_u32.into(), "weather radius"),
        (
            "weather",
            "size",
            serde_json::json!([0.1_f32, 0.01_f32]),
            "weather size",
        ),
        ("weather", "kind", "rain".into(), "rain"),
        (
            "weather_alternate",
            "visibility_m",
            1_u32.into(),
            "weather_alternate",
        ),
        (
            "weather_alternate",
            "wind",
            serde_json::json!([21.0_f32, 0.0_f32]),
            "weather_alternate",
        ),
        (
            "weather_alternate",
            "storm_severity",
            (-1_i32).into(),
            "weather_alternate",
        ),
        (
            "weather_alternate",
            "material",
            "core:steam_01".into(),
            "equal count and material",
        ),
        ("weather_cycle", "enabled", 1_u32.into(), "boolean"),
        (
            "weather_cycle",
            "period_seconds",
            0_u32.into(),
            "period_seconds",
        ),
        (
            "weather_cycle",
            "transition_seconds",
            61_u32.into(),
            "two transitions",
        ),
        (
            "weather_cycle",
            "min_strength",
            serde_json::json!(0.6_f32),
            "less than max_strength",
        ),
    ] {
        let mut invalid = baseline.clone();
        set(invalid.get_mut(field).ok_or(field)?, key, value)?;
        if key == "count" {
            set(&mut invalid, "weather_cycle", serde_json::Value::Null)?;
        }
        let outcome = admit_weather(&invalid);
        assert!(
            outcome
                .as_ref()
                .is_err_and(|error| error.contains(expected)),
            "{field}.{key} must fail native admission for {expected}: {outcome:?}"
        );
    }
    Ok(())
}

fn weather_interaction_refusals(baseline: &serde_json::Value) -> Result<(), String> {
    for (action, absent, expected) in [
        (
            serde_json::json!({"action":"toggle_weather"}),
            Some("weather_alternate"),
            "requires weather and weather_alternate",
        ),
        (
            serde_json::json!({"action":"set_weather_cycle","enabled":true}),
            Some("weather_cycle"),
            "requires weather_cycle",
        ),
        (
            serde_json::json!({"action":"set_weather_strength","strength":-0.1_f32}),
            None,
            "weather strength",
        ),
        (
            serde_json::json!({"action":"set_weather_strength","strength":0.5_f32,"transition_seconds":-1.0_f32}),
            None,
            "weather transition_seconds",
        ),
        (
            serde_json::json!({"action":"set_prompt","prompt":" "}),
            None,
            "prompt must not be blank",
        ),
    ] {
        let mut invalid = baseline.clone();
        if let Some(field) = absent {
            set(&mut invalid, field, serde_json::Value::Null)?;
            if field == "weather_alternate" {
                set(&mut invalid, "weather_cycle", serde_json::Value::Null)?;
            }
        }
        set(&mut invalid, "props", weather_switch(&action))?;
        let outcome = admit_weather(&invalid);
        assert!(
            outcome
                .as_ref()
                .is_err_and(|error| error.contains(expected)),
            "invalid weather interaction must fail for {expected}: {outcome:?}"
        );
    }
    Ok(())
}

// The generic geometry audit cannot see separate roof prop meshes. Eight
// saved homes share ten small meshes. Inspect the decoded eave/ridge vertices
// with the placement transform used for rendering, without baking the level.
#[test]
fn shipped_content_lantern_roof_meshes_cover_ridges_without_clipping_rooms() -> Result<(), String> {
    use crate::engine::gltf::parse_glb;
    use crate::engine::level::CeilingProfileDef;
    use std::collections::BTreeMap;
    use std::collections::btree_map::Entry;
    use std::path::Path;

    let level = LevelDef::from_json(include_str!(
        "../../../projects/environments/levels/lantern_hollow.json"
    ))
    .map_err(|error| error.to_string())?;
    let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
    let catalog = AssetCatalog::load_from_path(&assets.join("catalog.json"))
        .ok_or("shipped asset catalog did not load")?;
    let mut meshes = BTreeMap::new();
    for prop in &level.props {
        if !prop.model.ends_with("_roof_slope") && !prop.model.ends_with("_roof_ridge") {
            continue;
        }
        if let Entry::Vacant(entry) = meshes.entry(prop.model.as_str()) {
            let path = catalog
                .placeable(&prop.model)
                .and_then(|asset| asset.model.as_deref())
                .ok_or_else(|| format!("{} has no shipped mesh", prop.model))?;
            let bytes = std::fs::read(assets.join(path)).map_err(|error| error.to_string())?;
            let _mesh =
                entry.insert(parse_glb(&bytes).map_err(|error| format!("{path}: {error}"))?);
        }
    }
    let homes: Vec<_> = level
        .rooms
        .iter()
        .filter(|room| !matches!(room.ceiling, CeilingProfileDef::Open))
        .collect();
    assert_eq!(homes.len(), 8, "all eight shipped homes are covered");
    for room in homes {
        check_lantern_roof_joins(&level, room, &meshes)?;
    }
    Ok(())
}

fn check_lantern_roof_joins(
    level: &LevelDef,
    room: &crate::engine::level::RoomDef,
    meshes: &std::collections::BTreeMap<&str, crate::engine::gltf::PropModel>,
) -> Result<(), String> {
    use crate::engine::level::{LevelSurfaces, PropDef};
    use glam::Vec3;

    let mut walls: Vec<_> = level
        .walls
        .iter()
        .filter(|wall| {
            room.contains(wall.x, wall.z)
                && wall.width > wall.depth
                && wall.material.as_deref() == Some("outdoor:house_siding_01")
        })
        .collect();
    walls.sort_by(|a, b| a.z.total_cmp(&b.z));
    let &[back, front] = walls.as_slice() else {
        return Err(format!(
            "Lantern home at {},{} lacks both eave walls",
            room.x, room.z
        ));
    };
    let surfaces = LevelSurfaces::new(level);
    let placement = |prop: &PropDef| {
        surfaces
            .floor_y_at(prop.x, prop.z)
            .map(|base| prop.placement_matrix(base))
            .ok_or_else(|| format!("{:?} has no authored roof support", prop.id))
    };
    let (slopes, caps): (Vec<_>, Vec<_>) = level
        .props
        .iter()
        .filter(|prop| room.contains(prop.x, prop.z) && meshes.contains_key(prop.model.as_str()))
        .partition(|prop| prop.model.ends_with("_roof_slope"));
    assert_eq!(
        (slopes.len(), caps.len()),
        (6, 3),
        "three paired roof sections per home"
    );
    for slope in slopes {
        let id = slope.id.as_deref().unwrap_or(&slope.model);
        let pose = placement(slope)?;
        let joint_caps: Vec<_> = caps
            .iter()
            .filter(|other| slope.x.mul_add(1.0, -other.x).abs() < 0.0001)
            .collect();
        let [cap] = joint_caps.as_slice() else {
            return Err(format!("{id}: ridge cap must align with its roof section"));
        };
        let cap_pose = placement(cap)?;
        let cap_mesh = meshes.get(cap.model.as_str()).ok_or("missing ridge mesh")?;
        let slope_mesh = meshes
            .get(slope.model.as_str())
            .ok_or("missing slope mesh")?;
        let local_points = || {
            slope_mesh
                .vertices
                .iter()
                .map(|vertex| Vec3::from_array(vertex.pos))
        };
        let cap_bounds = vertex_interval(
            cap_mesh
                .vertices
                .iter()
                .map(|vertex| cap_pose.transform_point3(Vec3::from_array(vertex.pos))),
        )
        .ok_or("ridge cap has no vertices")?;
        let (eave_low, eave_high) = vertex_interval(
            local_points()
                .filter(|p| p.y <= 0.03501)
                .map(|p| pose.transform_point3(p)),
        )
        .ok_or("slope has no low soffit vertices")?;
        let (ridge_low, ridge_high) = vertex_interval(
            local_points()
                .filter(|p| p.z < -1.2999)
                .map(|p| pose.transform_point3(p)),
        )
        .ok_or("slope has no ridge-edge vertices")?;
        let ceiling_y = room.floor_y.mul_add(1.0, room.height);
        assert!(
            pose.transform_point3(Vec3::ZERO)
                .y
                .mul_add(1.0, -ceiling_y)
                .abs()
                < 0.0005,
            "{id}: fitted slope must start on the ceiling plane"
        );
        let clearance = if slope.z > cap.z {
            eave_low.z.mul_add(1.0, -front.z)
        } else {
            back.z.mul_add(1.0, back.depth).mul_add(1.0, -eave_high.z)
        };
        assert!(clearance >= 0.0099, "{id}: soffit extends into the room");
        let (cap_low, cap_high) = cap_bounds;
        assert!(
            cap_low.z < ridge_low.z
                && ridge_high.z < cap_high.z
                && ridge_low.y < cap_high.y
                && ridge_high.y > cap_low.y,
            "{id}: the ridge edge no longer intersects the cap"
        );
    }
    Ok(())
}

fn vertex_interval(
    mut points: impl Iterator<Item = glam::Vec3>,
) -> Option<(glam::Vec3, glam::Vec3)> {
    let first = points.next()?;
    Some(points.fold((first, first), |(low, high), point| {
        (low.min(point), high.max(point))
    }))
}
