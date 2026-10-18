//! Essential archive regressions: selected-level isolation, exact editable recovery,
//! and refusal/cancellation without publication. Two tiny JSON levels and one shared
//! placeholder prepared record protect these boundaries without baking or decoding a
//! world. Cost: a few KiB of ZIP/JSON and short-lived files; no GPU/assets required.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::engine::package::{
    archive::{self, PendingEntry},
    hash, project, world,
};
use crate::engine::project::{ProjectManifest, ProjectPath};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    root: PathBuf,
    index: project::ProjectPackage,
    blobs: BTreeMap<String, Vec<u8>>,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _cleanup = fs::remove_dir_all(&self.root);
    }
}

fn blob(blobs: &mut BTreeMap<String, Vec<u8>>, bytes: Vec<u8>) -> String {
    let name = hash::blob_name(&bytes, ".record");
    let _old = blobs.insert(name.clone(), bytes);
    name
}

fn fixture() -> Result<Fixture, String> {
    let root = std::env::temp_dir().join(format!(
        "places-project-package-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&root).map_err(|e| e.to_string())?;
    let manifest = serde_json::json!({"schema":"places-project","project_format":1_u32,"id":"roundtrip","name":"Round trip","entry_level":"second","levels":[{"id":"entry","source":"levels/entry.json","display_name":"Friendly inactive map"},{"id":"second","source":"levels/second.json"}]});
    let manifest_bytes = serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?;
    let authored = ProjectManifest::from_json(&manifest_bytes)?;
    let mut blobs = BTreeMap::new();
    let mut index = project::ProjectPackage {
        schema: "places-project-package".into(),
        package_format: 1,
        project: authored,
        levels: Vec::new(),
        sources: BTreeMap::new(),
    };
    let manifest_blob = blob(&mut blobs, manifest_bytes);
    let _old = index.sources.insert(
        ProjectPath::new("places.project.json".into())?,
        manifest_blob,
    );
    let prepared = b"placeholder; discovery must not decode geometry".to_vec();
    let prepared_name = hash::blob_name(&prepared, ".fixture");
    let shared_blob = blob(&mut blobs, prepared.clone());
    for id in ["entry", "second"] {
        let source = serde_json::json!({"schema":"places-level","format_version":1_u32,"id":id,"name":id,"spawn":{"x":1_u32,"z":1_u32},"rooms":[{"width":2_u32,"depth":2_u32,"height":2.5_f32,"editor_instance_id":"stable_room"}],"editor_placement_intent":{"version":1_u32,"ignores":[]},"future_setting":{"preserved":true}});
        let source_bytes = serde_json::to_vec_pretty(&source).map_err(|e| e.to_string())?;
        let semantics = hash::blob_name(&source_bytes, ".record");
        let _old_source = index.sources.insert(
            ProjectPath::new(format!("levels/{id}.json"))?,
            blob(&mut blobs, source_bytes.clone()),
        );
        let inner = serde_json::json!({"schema":"places-map","package_format":1_u32,"id":id,"name":id,"entries":[{"name":"semantics.json","role":"semantics","bytes":source_bytes.len(),"sha256":hash::sha256_hex(&source_bytes)},{"name":prepared_name,"role":"","bytes":prepared.len(),"sha256":hash::sha256_hex(&prepared)}],"variants":[{"lightmap_quality":"off","quality_profile":"low","entries":{"mesh":prepared_name,"props":prepared_name,"lighting":prepared_name,"collision":prepared_name,"navigation":prepared_name}}]});
        let inner_blob = blob(
            &mut blobs,
            serde_json::to_vec(&inner).map_err(|e| e.to_string())?,
        );
        index.levels.push(project::ProjectMap {
            id: id.into(),
            records: BTreeMap::from([
                ("manifest.json".into(), inner_blob),
                ("semantics.json".into(), semantics),
                (prepared_name.clone(), shared_blob.clone()),
            ]),
        });
    }
    Ok(Fixture { root, index, blobs })
}

impl Fixture {
    fn write(&self) -> Result<PathBuf, String> {
        let path = self.root.join("campaign.placesmap");
        let mut entries: Vec<_> = self
            .blobs
            .iter()
            .map(|(name, bytes)| PendingEntry {
                name: name.clone(),
                bytes: bytes.clone(),
            })
            .collect();
        entries.push(PendingEntry {
            name: project::INDEX.into(),
            bytes: serde_json::to_vec(&self.index).map_err(|e| e.to_string())?,
        });
        archive::write_archive(&path, entries)?;
        Ok(path)
    }
}

#[test]
fn project_recovers_exact_sources_and_preserves_shared_record_identity() -> Result<(), String> {
    let fixture = fixture()?;
    let package = fixture.write()?;
    project::validate(&package, &|| Ok(()))?;
    assert_eq!(
        world::open(&package)?.level.id,
        "second",
        "entry selection follows the explicit manifest rather than declaration order"
    );
    let restored = fixture.root.join("restored");
    project::recover(&package, &restored, &|| Ok(()))?;
    for (path, record) in &fixture.index.sources {
        assert_eq!(
            fs::read(restored.join(path.as_str())).map_err(|e| e.to_string())?,
            *fixture
                .blobs
                .get(record)
                .ok_or("Missing source fixture blob")?,
            "raw source and all editor/unknown data survive byte-exact recovery"
        );
    }
    let record = fixture
        .index
        .levels
        .first()
        .and_then(|level| {
            level
                .records
                .iter()
                .find(|(name, _)| name.ends_with(".fixture"))
        })
        .map(|(_, value)| value)
        .ok_or("Missing shared record")?;
    assert!(
        fixture
            .index
            .levels
            .iter()
            .all(|level| level.records.values().any(|value| value == record)),
        "all levels reference the one common prepared record"
    );
    assert!(
        project::recover(&package, &restored, &|| Ok(())).is_err(),
        "repeated recovery must refuse an existing project"
    );
    Ok(())
}

#[test]
fn selected_level_does_not_read_other_level_payloads_but_import_checks_them() -> Result<(), String>
{
    let mut fixture = fixture()?;
    let corrupt = fixture
        .index
        .levels
        .first()
        .and_then(|level| level.records.get("semantics.json"))
        .ok_or("Missing semantics")?
        .clone();
    fixture
        .blobs
        .get_mut(&corrupt)
        .ok_or("Missing corrupt fixture record")?
        .fill(b'X');
    let package = fixture.write()?;
    assert_eq!(
        project::inspect(&package)?
            .ok_or("Missing project index")?
            .project
            .level(Some("entry"))?
            .display_name,
        "Friendly inactive map",
        "menu metadata must survive corrupt inactive semantics"
    );
    assert_eq!(
        world::open_level(&package, "second")?.level.id,
        "second",
        "selected load must not decompress another level"
    );
    assert_eq!(
        world::inspect_level(&package, "entry")?.id,
        "entry",
        "cache identity reads only a manifest, even when selected semantic bytes are corrupt"
    );
    assert!(
        world::open_level(&package, "entry").is_err(),
        "loading the damaged level verifies its own record hash"
    );
    assert!(
        project::validate(&package, &|| Ok(())).is_err(),
        "full import validation rejects corrupt inactive records"
    );
    let restored = fixture.root.join("restored");
    assert!(
        project::recover(&package, &restored, &|| Ok(())).is_err() && !restored.exists(),
        "failed recovery publishes no folder"
    );
    Ok(())
}

// Essential recovery race regression: replacing the caller's input after the
// bounded copy must not switch the validated source snapshot. Reuses the tiny
// discovery fixture; no prepared-world decoding, assets or baking.
#[test]
fn recovery_validates_and_publishes_one_owned_archive_snapshot() -> Result<(), String> {
    let fixture = fixture()?;
    let package = fixture.write()?;
    let size = fs::metadata(&package).map_err(|e| e.to_string())?.len();
    let target = fixture.root.join("snapshot");
    let replaced = std::cell::Cell::new(false);
    project::recover(&package, &target, &|| {
        if !replaced.get()
            && fs::metadata(fixture.root.join("snapshot.partial/.recovery-package"))
                .is_ok_and(|metadata| metadata.len() == size)
        {
            let replacement = fixture.root.join("replacement");
            fs::write(&replacement, b"caller replaced this path").map_err(|e| e.to_string())?;
            fs::rename(replacement, &package).map_err(|e| e.to_string())?;
            replaced.set(true);
        }
        Ok(())
    })?;
    assert!(
        replaced.get(),
        "fixture must replace the caller's source during recovery"
    );
    assert!(target.join("places.project.json").is_file());
    assert!(
        !target.join(".recovery-package").exists(),
        "owned transport staging is not part of the recovered project"
    );
    Ok(())
}

// Special-file admission must finish without opening a blocking FIFO. One tiny
// owned filesystem entry; no child process, payload allocation or map bake.
#[cfg(unix)]
#[test]
fn package_paths_refuse_fifo_before_any_archive_read() -> Result<(), String> {
    use std::os::unix::ffi::OsStrExt as _;
    let fixture = fixture()?;
    let path = fixture.root.join("named-pipe.placesmap");
    let name = std::ffi::CString::new(path.as_os_str().as_bytes()).map_err(|e| e.to_string())?;
    // SAFETY: name is an owned NUL-terminated path; this creates an inert FIFO
    // inside the exclusively owned test directory and retains no pointer.
    if unsafe { libc::mkfifo(name.as_ptr(), 0o600) } != 0_i32 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    assert!(archive::open_file(&path).is_err());
    assert!(project::inspect(&path).is_err());
    assert!(
        crate::engine::package::dependencies::identity(
            &fixture.root,
            "named-pipe.placesmap",
            crate::engine::package::DependencyKind::Model
        )
        .is_err()
    );
    Ok(())
}

// Essential installed-asset boundary regression: declared hashes/sizes do not
// authorize escaping the asset root or unbounded installed-file reads. Two
// tiny files plus one confined symlink; no model/PNG decode or map bake.
#[test]
fn installed_dependency_identity_is_confined_streamed_and_size_bounded() -> Result<(), String> {
    use crate::engine::package::{DependencyKind, PackageDependency, dependencies};
    let fixture = fixture()?;
    let root = fixture.root.join("assets");
    fs::create_dir(&root).map_err(|e| e.to_string())?;
    let bytes = b"tiny installed model bytes";
    fs::write(root.join("model.glb"), bytes).map_err(|e| e.to_string())?;
    let mut dependency = PackageDependency {
        kind: DependencyKind::Model,
        path: "model.glb".into(),
        bytes: u64::try_from(bytes.len()).map_err(|e| e.to_string())?,
        sha256: hash::sha256_hex(bytes),
    };
    dependencies::validate(&root, &dependency)?;
    dependency.bytes = u64::try_from(crate::engine::gltf::MAX_ENCODED_GLB_BYTES)
        .map_err(|e| e.to_string())?
        .saturating_add(1);
    assert!(
        dependencies::validate(&root, &dependency).is_err(),
        "oversized claims are refused before hashing"
    );
    dependency.bytes = u64::try_from(bytes.len()).map_err(|e| e.to_string())?;
    fs::write(root.join("model.glb"), b"changed model bytes").map_err(|e| e.to_string())?;
    assert!(
        dependencies::validate(&root, &dependency).is_err(),
        "installed substitutions do not satisfy the recorded identity"
    );
    assert!(dependencies::identity(&root, "../outside.glb", DependencyKind::Model).is_err());
    #[cfg(unix)]
    {
        let outside = fixture.root.join("outside.glb");
        fs::write(&outside, bytes).map_err(|e| e.to_string())?;
        std::os::unix::fs::symlink(&outside, root.join("escape.glb")).map_err(|e| e.to_string())?;
        assert!(
            dependencies::identity(&root, "escape.glb", DependencyKind::Model).is_err(),
            "an asset symlink cannot read outside the selected installation"
        );
    }
    Ok(())
}

#[test]
fn project_index_and_cancellation_refuse_unsafe_or_partial_publication() -> Result<(), String> {
    let mut fixture = fixture()?;
    let package = fixture.write()?;
    let target = fixture.root.join("cancelled");
    assert!(
        project::recover(&package, &target, &|| Err("cancelled".into())).is_err()
            && !target.exists(),
        "cancellation must not publish source recovery"
    );
    fixture.index.package_format = 2;
    let unsupported = fixture.write()?;
    assert!(
        project::inspect(&unsupported).is_err(),
        "unsupported versions require a real conversion, never a guessed upgrade"
    );
    fixture.index.package_format = 1;
    let _unsafe_record = fixture
        .index
        .levels
        .first_mut()
        .ok_or("Missing level")?
        .records
        .insert("../outside".into(), hash::blob_name(b"unsafe", ".record"));
    let unsafe_path = fixture.write()?;
    assert!(
        project::inspect(&unsafe_path).is_err(),
        "virtual archive paths cannot traverse"
    );
    assert!(
        project::validate_thumbnail(b"not a PNG").is_err(),
        "invalid thumbnail bytes must never be accepted as image metadata"
    );
    Ok(())
}

// Essential menu regression: icons must follow selected-map/entry fallback and
// must be readable without any prepared/semantic world. Tiny ZIP placeholders
// plus two PNGs; the maximum supported 1280x720 PNG checks the separate UI bound
// that world-texture decoding would wrongly reject (about 4 MiB, no bake/GPU).
fn thumbnail_png(width: u32, height: u32, shade: u8) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|error| error.to_string())?;
    let count = usize::try_from(width.saturating_mul(height)).map_err(|error| error.to_string())?;
    let pixels = [shade, 20, 40, 160].repeat(count);
    writer
        .write_image_data(&pixels)
        .map_err(|error| error.to_string())?;
    writer.finish().map_err(|error| error.to_string())?;
    Ok(bytes)
}

#[test]
fn menu_icons_are_bounded_associated_and_independent_of_world_payloads() -> Result<(), String> {
    let mut fixture = fixture()?;
    let package_image = ProjectPath::new("thumbnails/package.png".into())?;
    let level_image = ProjectPath::new("thumbnails/entry.png".into())?;
    fixture.index.project.thumbnail = Some(package_image.clone());
    fixture
        .index
        .project
        .levels
        .first_mut()
        .ok_or("Missing level")?
        .thumbnail = Some(level_image.clone());
    drop(fixture.index.sources.insert(
        package_image,
        blob(&mut fixture.blobs, thumbnail_png(16, 9, 200)?),
    ));
    let image_blob = blob(&mut fixture.blobs, thumbnail_png(1280, 720, 80)?);
    drop(
        fixture
            .index
            .sources
            .insert(level_image, image_blob.clone()),
    );
    // Every semantic blob is corrupt: thumbnail browsing must not touch it.
    for level in &fixture.index.levels {
        let name = level
            .records
            .get("semantics.json")
            .ok_or("Missing semantics")?;
        fixture
            .blobs
            .get_mut(name)
            .ok_or("Missing semantic blob")?
            .fill(b'X');
    }
    let path = fixture.write()?;
    let image = project::menu_thumbnail(&path, "entry")?.ok_or("Missing per-level image")?;
    assert_eq!(
        (image.width, image.height),
        (1280, 720),
        "UI image maximum is accepted independently of world texture edges"
    );
    assert_eq!(image.rgba.first(), Some(&80), "selected level image wins");
    let fallback =
        project::menu_thumbnail(&path, "second")?.ok_or("Missing entry/package fallback")?;
    assert_eq!(
        fallback.rgba.first(),
        Some(&200),
        "package fallback belongs only to the entry map"
    );
    fixture
        .index
        .project
        .levels
        .first_mut()
        .ok_or("Missing level")?
        .thumbnail = None;
    assert!(
        project::menu_thumbnail(&fixture.write()?, "entry")?.is_none(),
        "non-entry map never inherits another map's package image"
    );
    fixture
        .index
        .project
        .levels
        .first_mut()
        .ok_or("Missing level")?
        .thumbnail = Some(ProjectPath::new("thumbnails/entry.png".into())?);
    fixture
        .blobs
        .get_mut(&image_blob)
        .ok_or("Missing PNG")?
        .fill(0);
    assert!(
        project::menu_thumbnail(&fixture.write()?, "entry").is_err(),
        "tampered image refuses decoding so caller can use fallback"
    );
    assert!(
        project::decode_thumbnail(&thumbnail_png(16, 16, 0)?).is_err(),
        "wrong aspect is rejected"
    );
    Ok(())
}
