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
    // Repeating a large installed dependency must not multiply hashing work;
    // case aliases also collide on supported portable installations. No I/O.
    let mut duplicate_manifest = minimal_manifest()?;
    let dependency = crate::engine::package::PackageDependency {
        kind: crate::engine::package::DependencyKind::Model,
        path: "models/shared.glb".into(),
        bytes: 0,
        sha256: hash::sha256_hex(&[]),
    };
    let mut alias = dependency.clone();
    alias.path = alias.path.to_ascii_uppercase();
    duplicate_manifest.dependencies = vec![dependency, alias];
    let names = vec![
        duplicate_manifest
            .entries
            .first()
            .ok_or("Missing fixture entry")?
            .name
            .clone(),
        "manifest.json".into(),
    ];
    assert!(
        duplicate_manifest.validate(&names).is_err(),
        "duplicate installed reads must be rejected at manifest admission"
    );
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

// Essential archive admission regression: reject ambiguous directory framing,
// encrypted/unsafe names and dishonest compressed sizes before publication.
// Every case derives from a <1 KiB ZIP; the decoded fixture is only 8 KiB.
#[test]
fn archive_framing_and_decompression_limits_fail_closed() -> Result<(), String> {
    use crate::engine::package::PackageReader;
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .start_file(
            "payload",
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated),
        )
        .map_err(|e| e.to_string())?;
    writer.write_all(&[0; 8192]).map_err(|e| e.to_string())?;
    let original = writer.finish().map_err(|e| e.to_string())?.into_inner();
    let directory = original
        .windows(4)
        .position(|bytes| bytes == b"PK\x01\x02")
        .ok_or("Missing central directory")?;
    let end = original
        .len()
        .checked_sub(22)
        .ok_or("Missing archive end")?;
    for (offset, replacement) in [
        (end + 8, 2_u16.to_le_bytes().to_vec()),
        (end + 20, 1_u16.to_le_bytes().to_vec()),
        (directory + 8, 1_u16.to_le_bytes().to_vec()),
        (directory + 46, b"../file".to_vec()),
        (directory + 46, b"bad\0key".to_vec()),
        (directory + 38, (0o120_777_u32 << 16).to_le_bytes().to_vec()),
        (
            directory + 24,
            u32::try_from(crate::engine::package::MAX_TOTAL_BYTES.saturating_add(1))
                .map_err(|e| e.to_string())?
                .to_le_bytes()
                .to_vec(),
        ),
    ] {
        let mut corrupt = original.clone();
        corrupt
            .get_mut(offset..offset.saturating_add(replacement.len()))
            .ok_or("Mutation outside ZIP")?
            .copy_from_slice(&replacement);
        assert!(
            PackageReader::new(Cursor::new(corrupt)).is_err(),
            "archive mutation at {offset} must be rejected before a payload read"
        );
    }
    let mut dishonest = original;
    dishonest
        .get_mut(directory + 24..directory + 28)
        .ok_or("Missing size")?
        .copy_from_slice(&1_u32.to_le_bytes());
    let mut reader = PackageReader::new(Cursor::new(dishonest)).map_err(|e| e.to_string())?;
    assert!(
        reader.read_entry("payload", 1024).is_err(),
        "a lying decompressed size must not defeat the streaming hard cap"
    );
    Ok(())
}

// A semantic policy edit must reject missing prepared capability/variants
// without decoding world data. This is one empty level/manifest, no bake.
#[test]
fn required_full_lighting_is_admitted_only_with_prepared_supported_capability() -> Result<(), String>
{
    let mut level: crate::engine::level::LevelDef = serde_json::from_value(serde_json::json!({"schema":"places-level","format_version":1_u32,"id":"full_policy","name":"Full policy","spawn":{"x":0_u32,"z":0_u32}})).map_err(|e|e.to_string())?;
    let mut manifest = minimal_manifest()?;
    world::validate_lighting_requirement(&level, &manifest)?;
    level.requires_full_lighting = true;
    assert!(world::validate_lighting_requirement(&level, &manifest).is_err());
    manifest
        .required_capabilities
        .push("full-lighting-v1".into());
    assert!(world::validate_lighting_requirement(&level, &manifest).is_err());
    manifest
        .variants
        .first_mut()
        .ok_or("Missing variant")?
        .lightmap_quality = "full".into();
    assert!(
        world::validate_lighting_requirement(&level, &manifest).is_err(),
        "a named Full variant without the prepared atlas is insufficient"
    );
    let full = manifest.variants.first_mut().ok_or("Missing variant")?;
    full.entries.lightmaps = Some("fixture-atlas".into());
    full.entries.lightmaps_meta = Some("fixture-metadata".into());
    world::validate_lighting_requirement(&level, &manifest)?;
    manifest
        .variants
        .first_mut()
        .ok_or("Missing variant")?
        .lightmap_failure = Some("fill failed".into());
    assert!(
        world::validate_lighting_requirement(&level, &manifest).is_err(),
        "required Full lighting cannot silently admit a vertex fallback"
    );
    world::validate_item_receipts(&level, &manifest)?;
    level.volumes.push(
        serde_json::from_value(
            serde_json::json!({"id":"receipt","width":1_u32,"depth":1_u32,"item_receipt":"centre"}),
        )
        .map_err(|e| e.to_string())?,
    );
    assert!(world::validate_item_receipts(&level, &manifest).is_err());
    manifest
        .required_capabilities
        .push("item-receipts-v1".into());
    world::validate_item_receipts(&level, &manifest)?;
    level
        .volumes
        .first_mut()
        .ok_or("Missing receiver")?
        .item_receipt = crate::engine::level::ItemReceiptDef::SettledCentre;
    world::validate_item_receipts(&level, &manifest)?;
    Ok(())
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

// Older hosts otherwise ignore the new Boolean and expose the mesh. This
// tiny admission check covers placed and template-only sources without a bake.
#[test]
fn shadow_only_sources_require_an_explicit_host_capability() -> Result<(), String> {
    let mut manifest = minimal_manifest()?;
    let mut level = crate::engine::level::LevelDef::from_json(
        r#"{"schema":"places-level","format_version":1,"id":"shadow","name":"Shadow","spawn":{"x":0,"z":0},"props":[{"model":"mannequin","shadow_only":true}]}"#,
    ).map_err(|e|e.to_string())?;
    assert!(world::validate_shadow_only(&level, &manifest).is_err());
    manifest.required_capabilities.push("shadow-only-v1".into());
    world::validate_shadow_only(&level, &manifest)?;
    level.props.clear();
    level.spawn_templates.push(
        serde_json::from_value(serde_json::json!({
            "id":"hidden","model":"mannequin","shadow_only":true
        }))
        .map_err(|e| e.to_string())?,
    );
    manifest.required_capabilities.clear();
    assert!(world::validate_shadow_only(&level, &manifest).is_err());
    Ok(())
}
