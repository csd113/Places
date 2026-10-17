//! Tiny package-boundary regressions: impossible collection headers must fail
//! before large reservations, and independently reopened variants must retain
//! the semantic manifest's identity. No maps are baked or assets resolved.

use std::io::{Cursor, Write as _};

use crate::engine::package::binary::Writer;
use crate::engine::package::manifest::Manifest;
use crate::engine::package::{MAX_MESH_RANGES, MAX_PROP_BATCHES, hash, mesh, props, world};
use crate::engine::props::PropAssets;
use crate::engine::quality::LightmapQuality;

#[test]
fn impossible_package_collection_counts_fail_before_reservation() -> Result<(), String> {
    let mut mesh_header = Writer::new();
    mesh_header.bytes(&mesh::MESH_MAGIC);
    mesh_header.u16(mesh::MESH_RECORD_VERSION);
    mesh_header.u32(u32::try_from(MAX_MESH_RANGES).map_err(|error| error.to_string())?);
    for _ in 0..12_usize {
        mesh_header.i32(0);
    }
    mesh_header.u32(0);
    mesh_header.u32(0);
    let mesh_error = mesh::read_mesh(mesh_header.as_slice())
        .err()
        .ok_or("impossible mesh ranges were accepted")?;
    assert!(
        mesh_error.contains("mesh range count") && mesh_error.contains("requiring at least"),
        "mesh collection must be rejected from its encoded size before reserving ranges: {mesh_error}"
    );

    let mut prop_header = Writer::new();
    prop_header.bytes(&props::PROPS_MAGIC);
    prop_header.u16(props::PROPS_RECORD_VERSION);
    prop_header.u32(u32::try_from(MAX_PROP_BATCHES).map_err(|error| error.to_string())?);
    let prop_error = props::read_props(prop_header.as_slice())
        .err()
        .ok_or("impossible prop batches were accepted")?;
    assert!(
        prop_error.contains("prop batch count") && prop_error.contains("requiring at least"),
        "prop collection must be rejected from its encoded size before reserving batches: {prop_error}"
    );
    Ok(())
}

#[test]
fn variant_load_rejects_a_replaced_manifest_before_decoding() -> Result<(), String> {
    let original = minimal_manifest()?;
    let mut replacement = original.clone();
    replacement.name = "Replacement snapshot".to_string();
    let bytes = fixture_archive(&replacement)?;
    let error = variant_error(&bytes, &original)?;
    assert!(
        error.contains("changed while loading"),
        "identity mismatch must precede the deliberately undecodable payload: {error}"
    );
    Ok(())
}

#[test]
fn manifest_declarations_must_match_the_archive_before_decoding() -> Result<(), String> {
    for (hash_disagrees, expected_error) in [
        (true, "content hash disagrees with the manifest"),
        (false, "bytes in the manifest but 0 in the archive"),
    ] {
        let mut manifest = minimal_manifest()?;
        let entry = manifest
            .entries
            .first_mut()
            .ok_or("missing fixture entry")?;
        if hash_disagrees {
            entry.sha256 = "1".repeat(64);
        } else {
            entry.bytes = 1;
        }
        let bytes = fixture_archive(&manifest)?;
        let error = variant_error(&bytes, &manifest)?;
        assert!(
            error.contains(expected_error),
            "inconsistent declaration must fail before its payload is decoded: {error}"
        );
    }
    Ok(())
}

fn minimal_manifest() -> Result<Manifest, String> {
    let blob = hash::blob_name(&[], ".mesh");
    serde_json::from_value(serde_json::json!({
        "package_format": 1,
        "id": "package_snapshot",
        "name": "Original snapshot",
        "entries": [{"name": blob, "role": "", "bytes": 0, "sha256": hash::sha256_hex(&[])}],
        "variants": [{
            "lightmap_quality": "off", "quality_profile": "low",
            "entries": {"mesh": blob, "props": blob, "lighting": blob,
                "collision": blob, "navigation": blob}
        }]
    }))
    .map_err(|error| error.to_string())
}

fn fixture_archive(manifest: &Manifest) -> Result<Vec<u8>, String> {
    let blob = &manifest
        .entries
        .first()
        .ok_or("missing fixture entry")?
        .name;
    let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    archive
        .start_file("manifest.json", options)
        .map_err(|error| error.to_string())?;
    archive
        .write_all(&manifest.to_json()?)
        .map_err(|error| error.to_string())?;
    archive
        .start_file(blob, options)
        .map_err(|error| error.to_string())?;
    Ok(archive
        .finish()
        .map_err(|error| error.to_string())?
        .into_inner())
}

fn variant_error(bytes: &[u8], manifest: &Manifest) -> Result<String, String> {
    world::load_variant_bytes(
        bytes,
        manifest,
        LightmapQuality::Off,
        &mut PropAssets::default(),
    )
    .err()
    .ok_or_else(|| "malformed package was accepted".to_string())
}
