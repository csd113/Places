//! Import regressions use tiny discovery-only packages with empty prepared
//! records. They exercise filesystem publication without baking a map, loading
//! textures, decoding world variants or allocating native resources.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::engine::loader::LevelManager;
use crate::engine::package::archive::{PendingEntry, write_archive};
use crate::engine::package::{
    FORMAT_VERSION, Manifest, PackageEntry, Variant, VariantEntries, hash,
};

static FIXTURE_ID: AtomicU64 = AtomicU64::new(0);

struct ImportFixture {
    root: PathBuf,
}

impl ImportFixture {
    fn new() -> Result<Self, String> {
        for _ in 0_u8..16 {
            let root = std::env::temp_dir().join(format!(
                "places-level-import-{}-{}",
                std::process::id(),
                FIXTURE_ID.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&root) {
                Ok(()) => return Ok(Self { root }),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error.to_string()),
            }
        }
        Err("could not reserve an import fixture directory".to_owned())
    }

    fn path(&self, directory: &str) -> Result<PathBuf, String> {
        let path = self.root.join(directory);
        fs::create_dir_all(&path).map_err(|error| error.to_string())?;
        Ok(path)
    }

    fn manager(&self) -> Result<LevelManager, String> {
        Ok(LevelManager::with_paths(
            self.path("bundled")?,
            self.path("levels")?,
            self.path("import")?,
        ))
    }
}

impl Drop for ImportFixture {
    fn drop(&mut self) {
        // Best-effort cleanup must not hide the original regression failure.
        let _cleanup_result = fs::remove_dir_all(&self.root);
    }
}

fn write_fixture(path: &Path, id: &str) -> Result<Vec<u8>, String> {
    write_release_fixture(path, id, "1.0.0", id)
}

fn write_release_fixture(
    path: &Path,
    id: &str,
    version: &str,
    name: &str,
) -> Result<Vec<u8>, String> {
    write_target_fixture(
        path,
        id,
        version,
        name,
        crate::engine::package::release::PLACES_VERSION,
    )
}

fn write_target_fixture(
    path: &Path,
    id: &str,
    version: &str,
    name: &str,
    target: &str,
) -> Result<Vec<u8>, String> {
    let semantics = serde_json::to_vec(&serde_json::json!({
        "schema": "places-level", "format_version": 1_u32,
        "id": id,
        "name": name,
        "package_version": version,
        "spawn": {"x": 0_u32, "z": 0_u32}
    }))
    .map_err(|error| error.to_string())?;
    let mut pending = vec![PendingEntry {
        name: "semantics.json".to_owned(),
        bytes: semantics,
    }];
    for role in ["mesh", "props", "lighting", "collision", "navigation"] {
        pending.push(PendingEntry {
            name: format!("{role}.bin"),
            bytes: Vec::new(),
        });
    }
    let entries = pending
        .iter()
        .map(|entry| PackageEntry {
            name: entry.name.clone(),
            role: entry
                .name
                .strip_suffix(".bin")
                .unwrap_or("semantics")
                .to_owned(),
            bytes: u64::try_from(entry.bytes.len()).unwrap_or(u64::MAX),
            sha256: hash::sha256_hex(&entry.bytes),
        })
        .collect();
    let manifest = Manifest {
        schema: crate::engine::package::PACKAGE_SCHEMA.to_string(),
        package_format: FORMAT_VERSION,
        id: id.to_owned(),
        package_id: id.to_owned(),
        package_version: version.to_string().try_into()?,
        places_version: target.into(),
        name: name.to_owned(),
        author: String::new(),
        created_by: String::new(),
        compiler_fingerprint: String::new(),
        lighting_fingerprint: None,
        required_capabilities: Vec::new(),
        dependencies: Vec::new(),
        entries,
        variants: vec![Variant {
            lightmap_quality: "off".to_owned(),
            quality_profile: "low".to_owned(),
            lightmap_failure: None,
            entries: VariantEntries {
                static_light_off: None,
                mesh: "mesh.bin".to_owned(),
                props: "props.bin".to_owned(),
                lighting: "lighting.bin".to_owned(),
                collision: "collision.bin".to_owned(),
                navigation: "navigation.bin".to_owned(),
                lightmaps: None,
                lightmaps_meta: None,
                irradiance: None,
                probes: Vec::new(),
            },
        }],
    };
    pending.push(PendingEntry {
        name: "manifest.json".to_owned(),
        bytes: manifest.to_json()?,
    });
    write_archive(path, pending)?;
    fs::read(path).map_err(|error| error.to_string())
}

#[test]
fn importing_installed_file_aliases_never_truncates_the_package() -> Result<(), String> {
    let fixture = ImportFixture::new()?;
    let installed = fixture.path("levels")?.join("map.placesmap");
    let original = write_fixture(&installed, "same_file")?;
    let linked = fixture.path("linked")?.join("map.placesmap");
    fs::hard_link(&installed, &linked).map_err(|error| error.to_string())?;
    let mut sources = vec![installed.clone()];
    sources.push(linked);
    #[cfg(unix)]
    {
        let symbolic = fixture.path("symbolic")?.join("map.placesmap");
        std::os::unix::fs::symlink(&installed, &symbolic).map_err(|error| error.to_string())?;
        sources.push(symbolic);
    }
    let mut manager = fixture.manager()?;
    for source in sources {
        let entry = manager.import_file(&source)?;
        assert_eq!(
            entry.id, "same_file",
            "alias import retains package metadata"
        );
        assert_eq!(
            fs::read(&installed).map_err(|error| error.to_string())?,
            original,
            "an alias import must preserve installed bytes"
        );
        assert_eq!(
            fs::read(&source).map_err(|error| error.to_string())?,
            original,
            "publication must leave the source readable and complete"
        );
    }
    Ok(())
}

#[test]
fn import_failure_preserves_destination_and_cleans_only_owned_temporary() -> Result<(), String> {
    let fixture = ImportFixture::new()?;
    let levels = fixture.path("levels")?;
    let installed = levels.join("map.placesmap");
    let original = write_fixture(&installed, "original")?;
    let source = fixture.path("source")?.join("map.placesmap");
    fs::write(&source, b"invalid package").map_err(|error| error.to_string())?;
    let reserved = levels.join("map.placesmap.partial");
    fs::write(&reserved, b"unrelated artifact").map_err(|error| error.to_string())?;
    let mut manager = fixture.manager()?;
    assert!(
        manager.import_file(&source).is_err(),
        "invalid staged bytes must be rejected"
    );
    assert_eq!(
        fs::read(&installed).map_err(|error| error.to_string())?,
        original,
        "validation failure must preserve the installed package"
    );
    assert_eq!(
        fs::read(&reserved).map_err(|error| error.to_string())?,
        b"unrelated artifact",
        "an existing temporary name belongs to another writer"
    );
    assert!(
        !levels.join("map.placesmap.partial-1").exists(),
        "a rejected staged package must be removed"
    );
    let _different = write_fixture(&source, "replacement")?;
    assert!(
        manager.import_file(&source).is_err(),
        "a filename collision must not overwrite another map id"
    );
    assert_eq!(
        fs::read(&installed).map_err(|error| error.to_string())?,
        original
    );
    let replacement = write_fixture(&source, "original")?;
    let entry = manager.import_file(&source)?;
    assert_eq!(entry.id, "original", "metadata must describe staged bytes");
    assert_eq!(
        fs::read(&entry.path).map_err(|error| error.to_string())?,
        replacement,
        "successful publication replaces the entire package"
    );
    let duplicate = fixture.path("source")?.join("different-name.placesmap");
    let _duplicate_bytes = write_fixture(&duplicate, "original")?;
    assert_eq!(
        manager.import_file(&duplicate)?.path,
        installed,
        "an identical renamed import resolves the installed stable ID without a second copy"
    );
    let collision = fixture.path("source")?.join("MAP.placesmap");
    let _case_bytes = write_fixture(&collision, "other")?;
    assert!(
        manager.import_file(&collision).is_err(),
        "case-folded filenames conflict on every platform"
    );
    fs::remove_file(&installed).map_err(|error| error.to_string())?;
    fs::create_dir(&installed).map_err(|error| error.to_string())?;
    assert!(
        manager.import_file(&source).is_err(),
        "a destination directory must reject publication"
    );
    assert!(
        installed.is_dir(),
        "failed publication preserves the destination"
    );
    assert!(
        !levels.join("map.placesmap.partial-1").exists(),
        "publication failure must also remove the owned temporary"
    );
    Ok(())
}

// Protects real disk update admission, renamed stable-ID resolution, canonical
// repack identity and refusal preservation. Unique value: descriptor arithmetic
// alone cannot detect publication mistakes. A few KiB, no world bake or GPU.
#[test]
fn release_updates_are_numeric_atomic_idempotent_and_preserve_rejected_bytes() -> Result<(), String>
{
    use crate::engine::package::release;
    let fixture = ImportFixture::new()?;
    let source = fixture.path("source")?.join("first.placesmap");
    let original = write_release_fixture(&source, "releases", "1.9.0", "First")?;
    let mut manager = fixture.manager()?;
    let installed = manager.import_file(&source)?.path;
    let renamed = fixture.path("source")?.join("renamed.placesmap");
    let upgrade = write_release_fixture(&renamed, "releases", "1.10.0", "Next")?;
    assert_eq!(
        manager.import_file(&renamed)?.path,
        installed,
        "renamed upgrades keep one stable installed location"
    );
    assert_eq!(
        fs::read(&installed).map_err(|error| error.to_string())?,
        upgrade
    );
    assert!(!fixture.path("levels")?.join("renamed.placesmap").exists());

    let descriptor = release::inspect(&installed)?;
    let repacked = stored_repack(&upgrade, false)?;
    assert_ne!(repacked, upgrade, "fixture must change ZIP transport");
    fs::write(&renamed, repacked).map_err(|error| error.to_string())?;
    assert_eq!(
        release::inspect(&renamed)?,
        descriptor,
        "compression and archive order cannot create a new logical release"
    );
    let _entry = manager.import_file(&renamed)?;
    assert_eq!(
        fs::read(&installed).map_err(|error| error.to_string())?,
        upgrade,
        "equal content is a no-op retaining installed transport bytes"
    );
    check_corrupt_noop(&mut manager, &installed, &renamed, &upgrade)?;
    fs::write(&renamed, original).map_err(|error| error.to_string())?;
    assert!(
        manager.import_file(&renamed).is_err(),
        "downgrade must refuse"
    );
    let _conflict = write_release_fixture(&renamed, "releases", "1.10.0", "Conflicting")?;
    assert!(
        manager.import_file(&renamed).is_err(),
        "same release/different hash must refuse"
    );
    assert_eq!(
        fs::read(&installed).map_err(|error| error.to_string())?,
        upgrade,
        "all failed admission keeps installed bytes"
    );
    check_older_target_update(&mut manager, &installed, &renamed)?;
    fs::write(&installed, &upgrade).map_err(|error| error.to_string())?;
    let duplicate = fixture.path("levels")?.join("duplicate.placesmap");
    fs::write(&duplicate, &upgrade).map_err(|error| error.to_string())?;
    manager.refresh();
    assert!(
        manager.entries().is_empty(),
        "ambiguous installed releases cannot silently load"
    );
    assert!(
        manager.import_file(&renamed).is_err(),
        "duplicate owners cannot update"
    );
    fs::remove_file(duplicate).map_err(|error| error.to_string())?;
    let bundled = fixture.path("bundled")?.join("official.placesmap");
    let _bundled = write_release_fixture(&bundled, "releases", "1.0.0", "Bundled")?;
    manager.refresh();
    assert!(
        manager
            .entries()
            .iter()
            .all(|entry| entry.source_type == crate::engine::loader::LevelSourceType::Bundled)
    );
    assert!(
        manager.import_file(&renamed).is_err(),
        "bundled ownership cannot be shadowed"
    );
    assert_eq!(
        fs::read(installed).map_err(|error| error.to_string())?,
        upgrade
    );
    Ok(())
}

fn stored_repack(bytes: &[u8], corrupt: bool) -> Result<Vec<u8>, String> {
    use crate::engine::package::PackageReader;
    use std::io::Write as _;
    let mut reader =
        PackageReader::new(std::io::Cursor::new(bytes)).map_err(|error| error.to_string())?;
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for name in reader.names().to_vec().into_iter().rev() {
        writer
            .start_file(
                &name,
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored),
            )
            .map_err(|error| error.to_string())?;
        let mut data = reader.read_entry(&name, 65536)?;
        if corrupt && name == "mesh.bin" {
            data.push(1);
        }
        writer.write_all(&data).map_err(|error| error.to_string())?;
    }
    let result = writer
        .finish()
        .map_err(|error| error.to_string())?
        .into_inner();
    Ok(result)
}

fn check_corrupt_noop(
    manager: &mut LevelManager,
    installed: &Path,
    source: &Path,
    valid: &[u8],
) -> Result<(), String> {
    let corrupt = stored_repack(valid, true)?;
    fs::write(installed, &corrupt).map_err(|error| error.to_string())?;
    assert!(
        manager.import_file(source).is_err(),
        "an identical manifest cannot conceal corrupted installed payloads"
    );
    assert_eq!(
        fs::read(installed).map_err(|error| error.to_string())?,
        corrupt,
        "refusal preserves corrupt evidence"
    );
    fs::write(installed, valid).map_err(|error| error.to_string())?;
    Ok(())
}

fn check_older_target_update(
    manager: &mut LevelManager,
    installed: &Path,
    source: &Path,
) -> Result<(), String> {
    let _old_target = write_target_fixture(installed, "releases", "1.10.0", "Next", "0.13.0")?;
    manager.refresh();
    assert!(
        manager.entries().is_empty(),
        "an older game target cannot play"
    );
    let incoming = write_release_fixture(source, "releases", "1.11.0", "Current game")?;
    assert_eq!(
        manager.import_file(source)?.path,
        installed,
        "current-contract metadata still resolves incompatible installed targets for update"
    );
    assert_eq!(
        fs::read(installed).map_err(|error| error.to_string())?,
        incoming
    );
    Ok(())
}
