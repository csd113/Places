//! Tiny malformed-input regressions; no shipped assets, map baking or GPU.

use super::mesh::validate_primitive_counts;
use super::parse::{CHUNK_BIN, CHUNK_JSON, GLB_MAGIC, parse_container};
use crate::engine::level::MAX_PROP_VERTICES;

fn glb(chunks: &[(u32, &[u8])]) -> Result<Vec<u8>, String> {
    let mut bytes = GLB_MAGIC.to_le_bytes().to_vec();
    bytes.extend_from_slice(&2_u32.to_le_bytes());
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    for (kind, payload) in chunks {
        let padded = payload.len().div_ceil(4).saturating_mul(4);
        bytes.extend_from_slice(
            &u32::try_from(padded)
                .map_err(|error| error.to_string())?
                .to_le_bytes(),
        );
        bytes.extend_from_slice(&kind.to_le_bytes());
        bytes.extend_from_slice(payload);
        bytes.resize(
            bytes
                .len()
                .saturating_add(padded.saturating_sub(payload.len())),
            b' ',
        );
    }
    let length = u32::try_from(bytes.len()).map_err(|error| error.to_string())?;
    bytes
        .get_mut(8..12)
        .ok_or("missing GLB header length")?
        .copy_from_slice(&length.to_le_bytes());
    Ok(bytes)
}

#[test]
fn glb_container_rejects_ambiguous_and_foreign_payloads() -> Result<(), String> {
    let json = br#"{"buffers":[{"byteLength":1}],"bufferViews":[{"buffer":0,"byteLength":1}]}"#;
    let valid = glb(&[(CHUNK_JSON, json), (CHUNK_BIN, &[7])])?;
    let (_, binary) = parse_container(&valid).map_err(|error| error.to_string())?;
    assert_eq!(
        binary,
        &[7],
        "the declared buffer excludes GLB alignment padding"
    );
    assert!(
        valid.as_ptr_range().contains(&binary.as_ptr()),
        "the binary chunk must borrow the input instead of copying it"
    );

    let mut trailing = valid.clone();
    trailing.push(0);
    let mut incomplete_header = trailing.clone();
    let incomplete_length =
        u32::try_from(incomplete_header.len()).map_err(|error| error.to_string())?;
    incomplete_header
        .get_mut(8..12)
        .ok_or("missing header")?
        .copy_from_slice(&incomplete_length.to_le_bytes());
    let cases = [
        ("trailing file bytes", trailing),
        ("truncated chunk header", incomplete_header),
        ("duplicate JSON", glb(&[(CHUNK_JSON, json), (CHUNK_JSON, json), (CHUNK_BIN, &[7])])?),
        ("duplicate BIN", glb(&[(CHUNK_JSON, json), (CHUNK_BIN, &[7]), (CHUNK_BIN, &[7])])?),
        ("BIN before JSON", glb(&[(CHUNK_BIN, &[7]), (CHUNK_JSON, json)])?),
        ("external buffer", glb(&[(CHUNK_JSON, br#"{"buffers":[{"byteLength":1,"uri":"elsewhere.bin"}]}"#), (CHUNK_BIN, &[7])])?),
        ("foreign buffer index", glb(&[(CHUNK_JSON, br#"{"buffers":[{"byteLength":1}],"bufferViews":[{"buffer":1,"byteLength":1}]}"#), (CHUNK_BIN, &[7])])?),
        ("buffer length mismatch", glb(&[(CHUNK_JSON, br#"{"buffers":[{"byteLength":5}]}"#), (CHUNK_BIN, &[7])])?),
    ];
    for (name, bytes) in cases {
        assert!(
            parse_container(&bytes).is_err(),
            "malformed GLB accepted: {name}"
        );
    }
    Ok(())
}

#[test]
fn primitive_counts_apply_existing_ceilings_before_reading_data() -> Result<(), String> {
    let json = serde_json::json!({"accessors":[
        {"count":MAX_PROP_VERTICES.saturating_add(1)}, {"count":4_usize}
    ]});
    let oversized = serde_json::json!({"attributes":{"POSITION":0_usize}});
    let vertex_error = validate_primitive_counts(&json, &oversized, 3)
        .err()
        .ok_or("oversized POSITION unexpectedly accepted")?;
    assert!(
        vertex_error.0.contains("POSITION"),
        "the vertex ceiling must precede any binary read"
    );
    let indices = serde_json::json!({"indices":1_usize});
    assert!(
        validate_primitive_counts(&json, &indices, 3).is_err(),
        "indices cannot exceed the remaining triangle budget"
    );
    assert!(
        validate_primitive_counts(&json, &indices, 4).is_ok(),
        "an accessor exactly at the remaining budget is accepted"
    );
    Ok(())
}

// Runtime parsing alone accepts a shifted base, an oversized authored mesh,
// or an unrelated embedded image. Check five installed Office props with the
// production GLB/PNG decoders, without invoking a generator or preparing a map.
#[test]
fn shipped_content_office_meshes_keep_fitted_bounds_and_texture_sheets() -> Result<(), String> {
    use crate::engine::assets::AssetCatalog;
    use crate::engine::materials::decode_png;
    use std::path::Path;

    type FittedSheet = (&'static str, u32, u32);

    let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
    let catalog = AssetCatalog::load_from_path(&assets.join("catalog.json"))
        .ok_or("shipped asset catalog did not load")?;
    let fixtures: [(&str, usize, &[FittedSheet]); 5] = [
        ("desk", 800, &[("desk.png", 256, 256)]),
        ("chair", 800, &[("chair.png", 256, 256)]),
        ("cabinet", 800, &[("cabinet.png", 256, 256)]),
        ("water_cooler", 800, &[("water_cooler.png", 256, 256)]),
        (
            "vending_machine",
            2869,
            &[
                ("drinks_materials.png", 256, 256),
                ("drinks_fascia.png", 256, 512),
            ],
        ),
    ];
    for (name, limit, sheets) in fixtures {
        let entry = catalog.placeable(&format!("core:{name}")).ok_or(name)?;
        let path = assets.join(entry.model.as_deref().ok_or(name)?);
        let bytes = std::fs::read(&path).map_err(|error| error.to_string())?;
        let model = super::parse_glb(&bytes).map_err(|error| format!("{name}: {error}"))?;
        check_office_mesh_shells(name, &model, name == "vending_machine")?;
        assert!(
            (1..=limit).contains(&model.triangles),
            "{name} must fit its reviewed triangle budget"
        );
        let (low, high) = model.bounds().ok_or(name)?;
        let [_, base_y, _] = low;
        assert!(
            base_y.abs() <= 1.0e-6,
            "{name} must sit on its placement plane"
        );
        for ((min, max), expected) in low.into_iter().zip(high).zip(entry.size.ok_or(name)?) {
            let extent = min.mul_add(-1.0, max);
            assert!(
                extent.mul_add(-1.0, expected).abs() <= expected.mul_add(0.06, 0.0).max(0.02),
                "{name}: rendered extent must fit the catalog placement size"
            );
        }
        assert!(
            model
                .vertices
                .iter()
                .flat_map(|vertex| vertex.uv)
                .all(|uv| (0.0..=1.0).contains(&uv)),
            "{name} must stay in its fitted UV frame"
        );
        assert_eq!(
            model.textures.len(),
            sheets.len(),
            "{name} must have all and only its fitted sheets"
        );
        let directory = path.parent().ok_or(name)?;
        for &(file, width, height) in sheets {
            let encoded = std::fs::read(directory.join(file)).map_err(|error| error.to_string())?;
            let sheet = decode_png(&encoded)?;
            assert_eq!(
                (sheet.width, sheet.height),
                (width, height),
                "{file} must keep its authored ratio"
            );
            assert!(
                sheet
                    .rgba
                    .iter()
                    .skip(3)
                    .step_by(4)
                    .all(|alpha| *alpha == 255),
                "{file} must be opaque"
            );
            assert!(
                model
                    .textures
                    .iter()
                    .any(|texture| texture.as_ref() == &sheet),
                "{name} must draw the visible {file} sheet, not stale embedded pixels"
            );
        }
    }
    Ok(())
}

// Four reused Office props should be closed, outward shells. The vending
// machine intentionally retains vent, cavity and label openings. Weld only
// coincident positions (not UV/normal seams) when checking these five small
// native meshes. A lost face or local reversal otherwise parses successfully.
fn check_office_mesh_shells(
    name: &str,
    model: &super::PropModel,
    allow_open_edges: bool,
) -> Result<(), String> {
    use glam::DVec3;
    use std::collections::BTreeMap;

    type Point = [u64; 3];
    type EdgeUses = Vec<(usize, bool)>;

    fn welded(point: DVec3) -> Point {
        point.to_array().map(|axis| {
            let rounded = axis.mul_add(1_000_000.0, 0.0).round();
            if rounded.abs() < 0.5_f64 {
                0
            } else {
                rounded.to_bits()
            }
        })
    }

    fn offset(point: DVec3, origin: DVec3) -> DVec3 {
        origin.mul_add(DVec3::splat(-1.0), point)
    }

    let mut faces = Vec::with_capacity(model.triangles);
    let mut edges: BTreeMap<(Point, Point), EdgeUses> = BTreeMap::new();
    let vertex = |index: u16| {
        model
            .vertices
            .get(usize::from(index))
            .map(|vertex| DVec3::from_array(vertex.pos.map(f64::from)))
            .ok_or_else(|| format!("{name}: face references a missing vertex"))
    };
    let (triangles, remainder) = model.indices.as_chunks::<3>();
    assert!(remainder.is_empty(), "{name}: incomplete triangle");
    for (face, &[index_a, index_b, index_c]) in triangles.iter().enumerate() {
        let [a, b, c] = [vertex(index_a)?, vertex(index_b)?, vertex(index_c)?];
        assert!(
            offset(b, a).cross(offset(c, a)).length_squared() >= 1.0e-20_f64,
            "{name}: face {face} has no area"
        );
        faces.push([a, b, c]);
        let [key_a, key_b, key_c] = [welded(a), welded(b), welded(c)];
        for (from, to) in [(key_a, key_b), (key_b, key_c), (key_c, key_a)] {
            let key = (from.min(to), from.max(to));
            edges.entry(key).or_default().push((face, from < to));
        }
    }
    let mut adjacent = vec![Vec::new(); faces.len()];
    let mut touches_open_edge = vec![false; faces.len()];
    for uses in edges.values() {
        if let [(first, forward), (second, backward)] = uses.as_slice() {
            assert_ne!(
                forward, backward,
                "{name}: shared edge reverses face winding"
            );
            adjacent.get_mut(*first).ok_or(name)?.push(*second);
            adjacent.get_mut(*second).ok_or(name)?.push(*first);
        } else {
            assert!(
                allow_open_edges,
                "{name}: shell edge has {} incident faces",
                uses.len()
            );
            for &(face, _) in uses {
                *touches_open_edge.get_mut(face).ok_or(name)? = true;
            }
        }
    }
    // Consistent edges alone cannot detect a whole component facing inward.
    // A closed shell's signed volume is independent of the chosen origin.
    let mut visited = vec![false; faces.len()];
    for (start, &[origin, _, _]) in faces.iter().enumerate() {
        if *visited.get(start).ok_or(name)? {
            continue;
        }
        let mut pending = vec![start];
        let mut closed = true;
        let mut volume = 0.0_f64;
        while let Some(face) = pending.pop() {
            if std::mem::replace(visited.get_mut(face).ok_or(name)?, true) {
                continue;
            }
            closed &= !*touches_open_edge.get(face).ok_or(name)?;
            let &[a, b, c] = faces.get(face).ok_or(name)?;
            volume = offset(a, origin)
                .dot(offset(b, origin).cross(offset(c, origin)))
                .mul_add(1.0, volume);
            pending.extend(adjacent.get(face).ok_or(name)?);
        }
        assert!(
            !closed || volume > 0.0_f64,
            "{name}: closed component faces inward"
        );
    }
    Ok(())
}
