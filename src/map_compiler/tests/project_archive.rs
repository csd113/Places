//! Essential project-cache regression: non-entry provenance must resolve through
//! the project record view, and a cache seed must verify its bytes. Two tiny
//! manifests and placeholder records require no assets, world bake or GPU.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::engine::package::{archive, hash, project};
use crate::engine::project::{ProjectManifest, ProjectPath};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);
struct Fixture(std::path::PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _cleanup = fs::remove_dir_all(&self.0);
    }
}

fn write_fixture(
    root: &Path,
    bad_chartless: bool,
) -> Result<(PathBuf, project::ProjectPackage), String> {
    let project_bytes = br#"{"schema":"places-project","project_format":1,"id":"cache","name":"Cache","entry_level":"entry","levels":[{"id":"entry","source":"levels/entry.json"},{"id":"second","source":"levels/second.json"}]}"#;
    let manifest = ProjectManifest::from_json(project_bytes)?;
    let mut blobs = BTreeMap::new();
    let source_name = hash::blob_name(project_bytes, ".record");
    drop(blobs.insert(source_name.clone(), project_bytes.to_vec()));
    let mut index = project::ProjectPackage {
        schema: "places-project-package".into(),
        package_format: project::PROJECT_FORMAT_VERSION,
        places_version: crate::engine::package::release::PLACES_VERSION.into(),
        project: manifest,
        levels: Vec::new(),
        sources: BTreeMap::from([(ProjectPath::new("places.project.json".into())?, source_name)]),
    };
    for id in ["entry", "second"] {
        let inputs = serde_json::to_vec(&super::super::BuildInputs {
            revision: 1,
            catalog_inputs: None,
            compiler_code_sha256: None,
            source_sha256: id.into(),
            catalog_sha256: "catalog".into(),
            tool_sha256: "tool".into(),
            capture_probes: true,
            compact: false,
        })
        .map_err(|e| e.to_string())?;
        let blob = hash::blob_name(&inputs, ".record");
        drop(blobs.insert(blob.clone(), inputs.clone()));
        let prepared = hash::blob_name(&[], ".fixture");
        let common = hash::blob_name(&[], ".record");
        drop(blobs.insert(common.clone(), Vec::new()));
        let mut inner = serde_json::json!({"schema":"places-map","package_format":crate::engine::package::FORMAT_VERSION,"package_id":index.project.id,"package_version":index.project.package_version,"places_version":index.places_version,"id":id,"name":id,"lighting_fingerprint":"stage","entries":[{"name":"build-inputs.json","role":"compiler-inputs","bytes":inputs.len(),"sha256":hash::sha256_hex(&inputs)},{"name":prepared,"role":"","bytes":0_u32,"sha256":hash::sha256_hex(&[])}],"variants":[{"lightmap_quality":"off","quality_profile":"low","entries":{"mesh":prepared,"props":prepared,"lighting":prepared,"collision":prepared,"navigation":prepared}}]});
        let mut alternate = BTreeMap::new();
        if bad_chartless && id == "second" {
            add_inconsistent_chartless(&mut inner, &mut alternate, &mut blobs, &prepared)?;
        }
        let inner_bytes = serde_json::to_vec(&inner).map_err(|e| e.to_string())?;
        let inner_blob = hash::blob_name(&inner_bytes, ".record");
        drop(blobs.insert(inner_blob.clone(), inner_bytes));
        index.levels.push(project::ProjectMap {
            id: id.into(),
            records: BTreeMap::from([
                ("manifest.json".into(), inner_blob),
                ("build-inputs.json".into(), blob),
                (prepared, common),
            ])
            .into_iter()
            .chain(alternate)
            .collect(),
        });
    }
    let package = root.join("project.placesmap");
    let mut entries: Vec<_> = blobs
        .into_iter()
        .map(|(name, bytes)| archive::PendingEntry { name, bytes })
        .collect();
    entries.push(archive::PendingEntry {
        name: project::INDEX.into(),
        bytes: serde_json::to_vec(&index).map_err(|e| e.to_string())?,
    });
    archive::write_archive(&package, entries)?;
    Ok((package, index))
}

fn add_inconsistent_chartless(
    manifest: &mut serde_json::Value,
    records: &mut BTreeMap<String, String>,
    blobs: &mut BTreeMap<String, Vec<u8>>,
    other: &str,
) -> Result<(), String> {
    use crate::engine::render::{
        LevelMesh, LevelMeshBatches, LevelMeshRange, SurfaceKey, SurfaceKind, Vertex,
    };

    let vertices = vec![
        Vertex::lightmapped([0.0, 0.0, 0.0], [1.0; 4], [0.0; 2], [0; 2], 0),
        Vertex::new([0.0, 0.0, 1.0], [1.0; 4], [0.0; 2]),
        Vertex::new([1.0, 0.0, 0.0], [1.0; 4], [0.0; 2]),
    ];
    let mesh = LevelMesh {
        ranges: vec![LevelMeshRange {
            key: SurfaceKey::new(SurfaceKind::Floor, 0),
            vertices,
            indices: vec![0, 1, 2],
            bounds: crate::engine::spatial::Aabb {
                min: [0.0; 3],
                max: [1.0, 0.0, 1.0],
            },
        }],
        batches: LevelMeshBatches::default(),
        vertex_count: 3,
        index_count: 3,
    };
    let entries = manifest
        .get_mut("entries")
        .and_then(serde_json::Value::as_array_mut)
        .ok_or("Missing fixture entries")?;
    let mut insert = |bytes: Vec<u8>, suffix: &str, role: &str| {
        let physical = hash::blob_name(&bytes, ".record");
        let logical = hash::blob_name(&bytes, suffix);
        entries.push(serde_json::json!({
            "name":logical, "role":role, "bytes":bytes.len(), "sha256":hash::sha256_hex(&bytes)
        }));
        drop(records.insert(logical.clone(), physical.clone()));
        drop(blobs.insert(physical, bytes));
        logical
    };
    let mesh_record = insert(
        crate::engine::package::mesh::write_mesh(&mesh)?,
        ".mesh",
        "mesh",
    );
    let props = insert(
        crate::engine::package::props::write_props(&[])?,
        ".props",
        "props",
    );
    manifest.get_mut("variants").and_then(serde_json::Value::as_array_mut).ok_or("Missing fixture variants")?.push(serde_json::json!({
        "lightmap_quality":"full", "quality_profile":"full",
        "lightmap_failure":crate::engine::package::NO_LIGHTMAP_CHARTS,
        "entries":{"mesh":mesh_record,"props":props,"lighting":other,"collision":other,"navigation":other}
    }));
    Ok(())
}

// One triangle in a two-member cache proves that correctly hashed records
// cannot claim chartless success while sampling an atlas. No bake or GPU.
#[test]
fn project_prepared_reuse_admits_static_and_rejects_chartless_atlas_references()
-> Result<(), String> {
    use crate::engine::quality::LightmapQuality;
    let root = Fixture(std::env::temp_dir().join(format!(
        "places-compiler-lighting-cache-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    )));
    fs::create_dir(&root.0).map_err(|error| error.to_string())?;
    let (package, _) = write_fixture(&root.0, true)?;
    let first = crate::engine::package::world::inspect_level(&package, "entry")?;
    super::super::verify_prepared_entries(&package, &first)?;
    let selected = crate::engine::package::world::inspect_level(&package, "second")?;
    super::super::verify_declared_entries(&package, &selected)?;
    let reason = super::super::verify_prepared_entries(&package, &selected)
        .err()
        .ok_or("Invalid chartless member was current")?;
    assert!(
        reason.contains("chartless lighting has geometry that references an atlas"),
        "{reason}"
    );

    let inputs = super::super::read_build_inputs(&package, &selected)?;
    let level = crate::engine::level::LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"second","name":"Second","spawn":{"x":1,"z":1}}"#)
        .map_err(|error| error.to_string())?;
    let (reused, _) = super::super::reuse_prepared_lighting(
        &package,
        "stage",
        &level,
        &[LightmapQuality::Off, LightmapQuality::Full],
        &inputs,
        &mut Vec::new(),
    )?;
    let variants: Vec<_> = reused
        .iter()
        .map(|variant| variant.lightmap_quality.as_str())
        .collect();
    assert_eq!(
        variants,
        ["off"],
        "a rejected Full record must not discard valid Static"
    );
    Ok(())
}

#[test]
fn project_provenance_and_cache_seed_select_and_verify_exact_records() -> Result<(), String> {
    let root = Fixture(std::env::temp_dir().join(format!(
        "places-compiler-project-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    )));
    fs::create_dir(&root.0).map_err(|e| e.to_string())?;
    let (package, index) = write_fixture(&root.0, false)?;
    let selected = crate::engine::package::world::inspect_level(&package, "second")?;
    let inputs = super::super::read_build_inputs(&package, &selected)?;
    assert_eq!(
        inputs.source_sha256, "second",
        "selected provenance must not use the entry level or raw archive root"
    );
    super::super::verify_declared_entries(&package, &selected)?;
    // Prepared reuse must select the same non-entry virtual member, too. This
    // protects metadata-only builds with no standalone cache; placeholder
    // records exercise selection/integrity only, with no solve or GPU.
    let selected_level = crate::engine::level::LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"second","name":"Second","spawn":{"x":1,"z":1},"rooms":[{"width":2,"depth":2,"height":3}]}"#).map_err(|error|error.to_string())?;
    let (variants, prepared) = super::super::reuse_prepared_lighting(
        &package,
        "stage",
        &selected_level,
        &[crate::engine::quality::LightmapQuality::Off],
        &inputs,
        &mut Vec::new(),
    )?;
    assert_eq!(variants.len(), 1);
    assert!(
        !prepared.is_empty(),
        "selected export records must seed prepared reuse without changing the export"
    );
    // A parsed provenance record must agree with the requested producer/mode.
    // Reuse this existing sub-KiB archive: no added bake, assets or large fixture.
    let mut changed_inputs = inputs.clone();
    changed_inputs.capture_probes = !inputs.capture_probes;
    assert!(!super::super::preparation_inputs_match(
        &inputs,
        &changed_inputs
    ));
    changed_inputs.capture_probes = inputs.capture_probes;
    changed_inputs.compiler_code_sha256 = Some("changed".into());
    assert!(!super::super::preparation_inputs_match(
        &inputs,
        &changed_inputs
    ));
    let staged = root.0.join("seed.placesmap");
    super::super::project_cache::seed(&package, "second", &staged)?;
    let seeded = super::super::inspect(&staged)?;
    assert_eq!(seeded.id, "second");
    assert_eq!(
        super::super::read_build_inputs(&staged, &seeded)?.source_sha256,
        "second"
    );
    // A hash-substituted record is refused before a seed can be published.
    let mut reader = super::super::open_package(&package)?;
    let mut corrupt = Vec::new();
    for name in reader.names().to_vec() {
        let mut bytes = reader.read_entry(&name, crate::engine::package::MAX_BINARY_BYTES)?;
        if index
            .levels
            .get(1)
            .and_then(|level| level.records.get("build-inputs.json"))
            == Some(&name)
        {
            bytes.fill(b'X');
        }
        corrupt.push(archive::PendingEntry { name, bytes });
    }
    archive::write_archive(&package, corrupt)?;
    let rejected = root.0.join("rejected.placesmap");
    assert!(
        super::super::project_cache::seed(&package, "second", &rejected).is_err()
            && !rejected.exists()
    );
    Ok(())
}

// Revision admission must follow actual stored recipes, including a reusable
// v1 recipe. Empty real PMP1 records yield tiny raw-only recipes: no assets,
// geometry build or segment fetch is needed to exercise capability emission.
#[test]
fn shared_props_capability_tracks_the_stored_recipe_revision() -> Result<(), String> {
    use crate::engine::package::{manifest::KNOWN_CAPABILITIES, props, shared_props};
    let original = props::write_props(&[])?;
    let factored = shared_props::factor_props(&[&original])?;
    let current = factored.recipes.first().ok_or("Missing current recipe")?;
    let mut previous = current.clone();
    previous
        .get_mut(4..6)
        .ok_or("Missing recipe version")?
        .copy_from_slice(&1_u16.to_le_bytes());
    let stored = |bytes: Vec<u8>| archive::PendingEntry {
        name: hash::blob_name(&bytes, ".props.recipe"),
        bytes,
    };
    assert!(
        !super::super::blobs_need_shared_props_v2(&[stored(previous)])?,
        "a stored v1 recipe retains its supported capability when reused"
    );
    assert!(
        super::super::blobs_need_shared_props_v2(&[stored(current.clone())])?,
        "a stored v2 recipe requires early v2 admission"
    );
    assert!(
        KNOWN_CAPABILITIES.contains(&"props-shared-streams")
            && KNOWN_CAPABILITIES.contains(&"props-shared-streams-v2"),
        "current hosts retain v1 support and explicitly admit v2"
    );
    let mut future = current.clone();
    future
        .get_mut(4..6)
        .ok_or("Missing recipe version")?
        .copy_from_slice(&u16::MAX.to_le_bytes());
    assert!(
        super::super::blobs_need_shared_props_v2(&[stored(future)]).is_err(),
        "an unsupported recipe cannot be silently labelled as v2"
    );
    Ok(())
}
