//! Explicit decoded-record oracle for lossless package/storage comparisons.
//! It never prepares a map, resolves assets, or runs in tests/automatic CI.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, Write as _};
use std::path::Path;

use places::engine::package::{
    self, Manifest, PackageReader, compact_lightmaps, hash, project, shared_props,
};

type Reader = PackageReader<File>;

fn opened(path: &Path) -> Result<Reader, String> {
    PackageReader::new(File::open(path).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())
}

fn digest(bytes: &[u8]) -> serde_json::Value {
    serde_json::json!({"bytes": bytes.len(), "sha256": hash::sha256_hex(bytes)})
}

fn member(path: &Path, id: Option<&str>) -> Result<serde_json::Value, String> {
    let mut reader = opened(path)?;
    reader.select_level(id)?;
    let manifest_bytes = reader.read_entry("manifest.json", package::MAX_MANIFEST_BYTES)?;
    let manifest = Manifest::from_json(&manifest_bytes, reader.names())?;
    let mut ordinary = BTreeMap::new();
    for entry in &manifest.entries {
        if matches!(
            entry.role.as_str(),
            "props" | "props-shared" | "lightmaps" | "compiler-inputs"
        ) {
            continue;
        }
        // Limits and hash declarations were validated above. The compiler's
        // full typed validator remains a separate required acceptance gate.
        let bytes = reader.read_entry(&entry.name, entry.bytes)?;
        if hash::sha256_hex(&bytes) != entry.sha256 {
            return Err("oracle record digest mismatch".into());
        }
        drop(ordinary.insert(entry.name.clone(), digest(&bytes)));
    }
    let mut variants = BTreeMap::new();
    for variant in &manifest.variants {
        let entries = &variant.entries;
        let props = shared_props::read_record(&mut reader, &entries.props)?;
        let mut values = BTreeMap::from([("props", digest(&props))]);
        if let Some(state) = &entries.static_light_off {
            let off = shared_props::read_record(&mut reader, &state.props)?;
            drop(values.insert("static_light_off_props", digest(&off)));
        }
        if let Some(atlas) = &entries.lightmaps {
            let bytes = reader.read_blob(atlas, package::MAX_LIGHTMAP_ATLAS_BYTES)?;
            let canonical = if bytes.starts_with(b"PLC1") {
                let decoded = compact_lightmaps::decode(&bytes)?;
                serde_json::json!({"edge":decoded.edge,"pages":decoded.page_count,
                    "groups":decoded.group_count,"planes":digest(&decoded.rgba16f_planes)})
            } else {
                digest(&bytes)
            };
            drop(values.insert("lightmaps", canonical));
        }
        drop(variants.insert(variant.lightmap_quality.clone(), values));
    }
    let inputs = if reader.contains("build-inputs.json") {
        serde_json::from_slice::<serde_json::Value>(
            &reader.read_entry("build-inputs.json", package::MAX_MANIFEST_BYTES)?,
        )
        .map_err(|error| error.to_string())?
    } else {
        serde_json::Value::Null
    };
    Ok(
        serde_json::json!({"id":manifest.id,"manifest":manifest,"ordinary_records":ordinary,"variants":variants,"provenance":inputs}),
    )
}

pub fn run(path: &Path) -> io::Result<()> {
    let mut reader = opened(path).map_err(io::Error::other)?;
    let index = project::read_index(&mut reader).map_err(io::Error::other)?;
    let mut sources = BTreeMap::new();
    let mut levels = BTreeMap::new();
    if let Some(project) = index {
        for (source, physical) in &project.sources {
            let bytes = reader
                .read_blob(physical, project::MAX_SOURCE_BYTES)
                .map_err(io::Error::other)?;
            drop(sources.insert(source.as_str().to_string(), digest(&bytes)));
        }
        for level in &project.levels {
            drop(levels.insert(
                level.id.clone(),
                member(path, Some(&level.id)).map_err(io::Error::other)?,
            ));
        }
        // Own the source keys before the project index is retired.
    } else {
        drop(levels.insert(
            "ordinary".to_string(),
            member(path, None).map_err(io::Error::other)?,
        ));
    }
    let result = serde_json::json!({"sources":sources,"levels":levels});
    writeln!(io::stdout().lock(), "{result}")?;
    Ok(())
}

/// Explicit packing/RAM probe over an already validated standalone fixture.
/// The destination is exclusive and no source record is changed.
pub fn reencode_lightmaps(input: &Path, output: &Path) -> io::Result<()> {
    use package::lightmaps::{read_lightmaps_owned, write_lightmaps};
    let result = (|| -> Result<serde_json::Value, String> {
        let mut reader = opened(input)?;
        let manifest_bytes = reader.read_entry("manifest.json", package::MAX_MANIFEST_BYTES)?;
        let manifest = Manifest::from_json(&manifest_bytes, reader.names())?;
        let variant = manifest.variants.iter().find(|variant| variant.lightmap_quality == "full")
            .ok_or("packing probe requires Full")?;
        let meta = reader.read_blob(variant.entries.lightmaps_meta.as_deref().ok_or("missing metadata")?, package::MAX_LIGHTMAP_METADATA_BYTES)?;
        let atlas = reader.read_blob(variant.entries.lightmaps.as_deref().ok_or("missing atlas")?, package::MAX_LIGHTMAP_ATLAS_BYTES)?;
        let maps = read_lightmaps_owned(&meta, atlas)?;
        let materialized = || maps.pages.iter().chain(maps.switchable.iter().flat_map(|group| &group.pages))
            .filter(|page| page.texels.is_materialized()).count();
        let before = materialized();
        let started = std::time::Instant::now();
        let (_, bytes) = write_lightmaps(&maps)?;
        let seconds = started.elapsed().as_secs_f64();
        let mut destination = File::options().write(true).create_new(true).open(output).map_err(|error| error.to_string())?;
        destination.write_all(&bytes).map_err(|error| error.to_string())?;
        destination.sync_all().map_err(|error| error.to_string())?;
        Ok(serde_json::json!({"encode_seconds":seconds,"bytes":bytes.len(),"pages":maps.pages.len(),
            "groups":maps.switchable.len().saturating_add(1),"materialized_pages_before":before,
            "materialized_pages_after":materialized()}))
    })().map_err(io::Error::other)?;
    writeln!(io::stdout().lock(), "{result}")?;
    Ok(())
}

/// Cheap bounded member identity for Python versions without ZIP method 93.
pub fn manifest_identity(path: &Path, id: &str) -> io::Result<()> {
    let mut reader = opened(path).map_err(io::Error::other)?;
    reader.select_level(Some(id)).map_err(io::Error::other)?;
    let bytes = reader
        .read_entry("manifest.json", package::MAX_MANIFEST_BYTES)
        .map_err(io::Error::other)?;
    let manifest = Manifest::from_json(&bytes, reader.names()).map_err(io::Error::other)?;
    if manifest.id != id {
        return Err(io::Error::other("manifest member identity differs"));
    }
    writeln!(io::stdout().lock(), "{}", hash::sha256_hex(&bytes))?;
    Ok(())
}

/// Explicit current runtime decoder/oracle probe, separate from native timing.
/// Decoded planes are hashed in logical order without a second flattened copy.
pub fn lightmap_residency(path: &Path, id: &str) -> io::Result<()> {
    use package::lightmaps::read_lightmaps_owned;
    use sha2::{Digest as _, Sha256};
    let result = (|| -> Result<serde_json::Value, String> {
        let mut reader = opened(path)?;
        if !id.is_empty() { reader.select_level(Some(id))?; }
        let manifest = Manifest::from_json(&reader.read_entry("manifest.json", package::MAX_MANIFEST_BYTES)?, reader.names())?;
        let variant = manifest.variants.iter().find(|variant| variant.lightmap_quality == "full").ok_or("probe requires Full")?;
        let meta = reader.read_blob(variant.entries.lightmaps_meta.as_deref().ok_or("missing metadata")?, package::MAX_LIGHTMAP_METADATA_BYTES)?;
        let atlas = reader.read_blob(variant.entries.lightmaps.as_deref().ok_or("missing atlas")?, package::MAX_LIGHTMAP_ATLAS_BYTES)?;
        let encoded_bytes = atlas.capacity();
        // Separate oracle allocation is diagnostic only, never a runtime owner.
        let reference = compact_lightmaps::decode(&atlas)?;
        let reference_hash = hash::sha256_hex(&reference.rgba16f_planes);
        drop(reference);
        let started = std::time::Instant::now();
        let maps = read_lightmaps_owned(&meta, atlas)?;
        let decode_seconds = started.elapsed().as_secs_f64();
        let mut digest = Sha256::new();
        let mut logical_bytes = 0_usize;
        let mut materialized = 0_usize;
        for page in maps.pages.iter().chain(maps.switchable.iter().flat_map(|group| &group.pages)) {
            let planes = page.rgba16f_planes().ok_or("runtime pixels materialized")?;
            digest.update(planes);
            logical_bytes = logical_bytes.saturating_add(planes.len());
            materialized = materialized.saturating_add(usize::from(page.texels.is_materialized()));
        }
        let mut runtime_hash = String::with_capacity(64);
        for byte in digest.finalize() {
            use std::fmt::Write as _;
            write!(runtime_hash, "{byte:02x}").map_err(|error| error.to_string())?;
        }
        if runtime_hash != reference_hash { return Err("runtime half planes differ from contiguous oracle".into()); }
        Ok(serde_json::json!({"id":manifest.id,"encoded_bytes":encoded_bytes,"logical_plane_bytes":logical_bytes,"unique_plane_bytes":maps.retained_pixel_bytes(),"materialized_pages":materialized,"sha256":runtime_hash,"reference_sha256":reference_hash,"decode_seconds":decode_seconds,"pages":maps.pages.len(),"switchable_lights":maps.switchable.iter().map(|group|group.light_index).collect::<Vec<_>>()}))
    })().map_err(io::Error::other)?;
    writeln!(io::stdout().lock(), "{result}")?;
    Ok(())
}
