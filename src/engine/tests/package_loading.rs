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
        "schema": "places-map",
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

// Essential reset regression: one probe exercises the new presence/group framing,
// including zero selected sources, and rejects the historical signature collision.
// All records stay below 160 bytes; no assets, map baking or GPU work is involved.
#[test]
fn reset_probe_field_roundtrips_optional_planes_and_rejects_superseded_headers()
-> Result<(), String> {
    use crate::engine::lighting::lightmap::LightmapTexel;
    use crate::engine::lighting::probes::{
        ProbeDirectField, ProbeField, ProbeSample, ProbeSwitchableField,
    };

    let zero = LightmapTexel {
        irradiance: [0.0; 3],
        direction: [0.0; 3],
        axis: [0.5; 2],
    };
    let mut field = ProbeField {
        min: [0.0; 3],
        cell_m: 1.0,
        dims: [1; 3],
        probes: vec![ProbeSample {
            irradiance: [1.0; 3],
            direction: [0.0; 3],
            axis: [0.5; 2],
            room: 0,
        }],
        local_direct: None,
        switchable_indirect: Vec::new(),
    };
    for state in 0_u8..3 {
        if state == 1 {
            field.local_direct = Some(ProbeDirectField {
                light_indices: Vec::new(),
                probes: vec![zero],
            });
        } else if state == 2 {
            field.switchable_indirect.push(ProbeSwitchableField {
                light_index: 0,
                probes: vec![zero],
            });
        }
        let encoded = field.write()?;
        assert_eq!(ProbeField::read(&encoded)?, field);
        let mut old = encoded.clone();
        old.get_mut(..4)
            .ok_or("missing probe signature")?
            .copy_from_slice(b"PLPF");
        assert!(ProbeField::read(&old).is_err());
        let mut unsupported = encoded.clone();
        unsupported
            .get_mut(4..6)
            .ok_or("missing probe version")?
            .copy_from_slice(&2_u16.to_le_bytes());
        assert!(ProbeField::read(&unsupported).is_err());
        let mut truncated = encoded;
        let _removed = truncated.pop();
        assert!(ProbeField::read(&truncated).is_err());
    }
    Ok(())
}
