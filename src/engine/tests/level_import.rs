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
    let semantics = serde_json::to_vec(&serde_json::json!({
        "format_version": 3_u32,
        "id": id,
        "name": id,
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
        package_format: FORMAT_VERSION,
        id: id.to_owned(),
        name: id.to_owned(),
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
    let replacement = write_fixture(&source, "replacement")?;
    let entry = manager.import_file(&source)?;
    assert_eq!(
        entry.id, "replacement",
        "metadata must describe staged bytes"
    );
    assert_eq!(
        fs::read(&entry.path).map_err(|error| error.to_string())?,
        replacement,
        "successful publication replaces the entire package"
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
