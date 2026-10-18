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
    let manifest = serde_json::json!({"schema":"places-project","project_format":1_u32,"id":"roundtrip","name":"Round trip","entry_level":"second","levels":[{"id":"entry","source":"levels/entry.json"},{"id":"second","source":"levels/second.json"}]});
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
