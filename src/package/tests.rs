//! Focused package-format tests: round trips, adversarial archives, record
//! bounds and the compiler's incremental behaviour.
//!
//! Normal suites use tiny synthetic levels and never bake a shipped map: the
//! real Demo and Model Zoo conversion is covered by `tests/test_package.py`
//! and `tools/verify.sh`.
// Test code: panic/expect, indexing and permissive arithmetic are idiomatic.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::wildcard_enum_match_arm
)]

use std::io::Write as _;

use super::archive::{PackageReader, PendingEntry, write_archive};
use super::binary::{Reader, Writer};
use super::ktx2;
use super::manifest::{Manifest, Variant, VariantEntries};
use super::{FORMAT_VERSION, MAX_ENTRIES};

fn tiny_level_json(id: &str) -> String {
    format!(
        r#"{{
            "format_version": 3, "id": "{id}", "name": "Package Test",
            "spawn": {{ "x": 1.0, "z": 1.0 }},
            "rooms": [ {{ "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0, "height": 3.0 }} ],
            "ceiling_lights": [ {{ "fixture": "core:ceiling_panel_01", "x": 2.0, "z": 2.0 }} ]
        }}"#
    )
}

fn tiny_level(id: &str) -> crate::level::LevelDef {
    crate::level::LevelDef::from_json(&tiny_level_json(id)).expect("tiny level parses")
}

fn scratch_dir(tag: &str) -> std::path::PathBuf {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "places-package-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&path).expect("scratch directory");
    path
}

fn build_level_build(
    level: &crate::level::LevelDef,
    quality: crate::quality::LightmapQuality,
) -> crate::render::LevelBuild {
    let catalog = crate::loader::PropCatalog::load_default();
    let materials = crate::render::logical_materials(level);
    let mut assets = crate::props::PropAssets::load_default();
    let prepared = crate::render::prepare_level_geometry_with_lightmaps(
        level,
        &catalog,
        &mut assets,
        &materials,
        crate::render::LightmapBuildOptions::for_lightmaps(quality),
        None,
    );
    let mut build = prepared.build;
    if let Some(fill) = prepared.fill {
        match crate::render::fill_lightmaps_cancellable(
            &fill,
            &std::sync::atomic::AtomicBool::new(false),
        ) {
            crate::render::LightmapFillOutcome::Filled(product) => {
                build.lightmaps = Some(std::sync::Arc::new(product.lightmaps));
            }
            outcome => panic!("tiny lightmap fill did not complete: {outcome:?}"),
        }
    }
    build
}

#[test]
fn binary_primitives_round_trip_and_reject_truncation() {
    let mut writer = Writer::new();
    writer.u8(0xAB);
    writer.u16(0x1234);
    writer.u32(0xDEAD_BEEF);
    writer.u64(u64::MAX);
    writer.i32(-7);
    writer.f32(1.5);
    writer.bool(true);
    writer.str("hello").expect("string fits");
    writer.blob(&[1, 2, 3]).expect("blob fits");
    writer.u16s(&[7, 8, 9]).expect("indices fit");
    let bytes = writer.into_bytes();
    let mut reader = Reader::new(&bytes);
    assert_eq!(reader.u8().expect("u8"), 0xAB);
    assert_eq!(reader.u16().expect("u16"), 0x1234);
    assert_eq!(reader.u32().expect("u32"), 0xDEAD_BEEF);
    assert_eq!(reader.u64().expect("u64"), u64::MAX);
    assert_eq!(reader.i32().expect("i32"), -7);
    assert_eq!(reader.f32().expect("f32").to_bits(), 1.5_f32.to_bits());
    assert!(reader.bool().expect("bool"));
    assert_eq!(reader.str(16).expect("string"), "hello");
    assert_eq!(reader.blob(16).expect("blob"), vec![1, 2, 3]);
    assert_eq!(reader.u16s(16).expect("indices"), vec![7, 8, 9]);
    assert!(reader.is_empty());
    // Truncation is an error, never a panic or a silent short read.
    for cut in 0..bytes.len() {
        let result = (|| -> Result<(), String> {
            let mut reader = Reader::new(&bytes[..cut]);
            reader.u8()?;
            reader.u16()?;
            reader.u32()?;
            reader.u64()?;
            reader.i32()?;
            reader.f32()?;
            reader.bool()?;
            reader.str(16)?;
            reader.blob(16)?;
            reader.u16s(16)?;
            Ok(())
        })();
        assert!(result.is_err(), "a record cut at {cut} must error");
    }
    // Length-prefixed reads are bounded before allocation.
    let mut reader = Reader::new(&bytes);
    assert!(reader.str(4).is_err(), "an oversized string is rejected");
}

#[test]
fn ktx2_round_trips_a_2d_array_and_a_cube() {
    let edge = 4_u32;
    let layers: Vec<Vec<u8>> = (0..2)
        .map(|layer| vec![layer as u8; (edge * edge * 4) as usize])
        .collect();
    let encoded = ktx2::write_rgba8_2d_array(edge, &layers).expect("2D array encodes");
    let decoded = ktx2::read_rgba8(&encoded).expect("2D array decodes");
    assert_eq!(decoded.edge, edge);
    assert_eq!(decoded.layers, 2);
    assert_eq!(decoded.faces, 1);
    let flattened: Vec<u8> = layers.iter().flatten().copied().collect();
    assert_eq!(decoded.levels.len(), 1, "one mip level");
    assert_eq!(decoded.levels[0], flattened, "level data is layer-major");

    let faces: Vec<Vec<u8>> = (0..6)
        .map(|face| vec![face as u8; (edge * edge * 4) as usize])
        .collect();
    let encoded = ktx2::write_rgba8_cube(edge, &faces).expect("cube encodes");
    let decoded = ktx2::read_rgba8(&encoded).expect("cube decodes");
    assert_eq!(decoded.faces, 6);
    assert_eq!(decoded.layers, 0);
    let flattened: Vec<u8> = faces.iter().flatten().copied().collect();
    assert_eq!(decoded.levels.len(), 1);
    assert_eq!(decoded.levels[0], flattened, "level data is face-major");
}

#[test]
fn ktx2_rejects_unsupported_subset_violations() {
    let edge = 2_u32;
    let layers = vec![vec![9_u8; (edge * edge * 4) as usize]];
    let encoded = ktx2::write_rgba8_2d_array(edge, &layers).expect("payload encodes");

    let mut wrong_magic = encoded.clone();
    wrong_magic[0] = 0;
    assert!(ktx2::read_rgba8(&wrong_magic).is_err());

    let mut wrong_format = encoded.clone();
    wrong_format[12..16].copy_from_slice(&0_u32.to_le_bytes());
    assert!(ktx2::read_rgba8(&wrong_format).is_err());

    let mut supercompressed = encoded.clone();
    supercompressed[44..48].copy_from_slice(&2_u32.to_le_bytes());
    assert!(ktx2::read_rgba8(&supercompressed).is_err());

    // A declared level length that disagrees with the dimensions is rejected
    // before any allocation.
    let mut bad_length = encoded.clone();
    bad_length[88..96].copy_from_slice(&1_u64.to_le_bytes());
    assert!(ktx2::read_rgba8(&bad_length).is_err());

    assert!(ktx2::read_rgba8(&encoded[..encoded.len() - 1]).is_err());
    let mut trailing = encoded;
    trailing.push(0);
    assert!(
        ktx2::read_rgba8(&trailing).is_err(),
        "trailing bytes are rejected"
    );
    assert!(ktx2::write_rgba8_2d_array(edge, &[]).is_err());
    assert!(ktx2::write_rgba8_2d_array(0, &layers).is_err());
    assert!(ktx2::write_rgba8_cube(edge, &layers).is_err());
    assert!(ktx2::write_rgba8_2d_array(edge, &[vec![0_u8; 3]]).is_err());
}

/// Builds one `edge` x `edge` RGBA16F image whose half-float channels follow a
/// deterministic pattern, so a round trip can be compared for exactness.
fn f16_image(edge: u32, seed: u16) -> Vec<u8> {
    let texels = edge * edge;
    let mut out = Vec::with_capacity((texels * 8) as usize);
    for index in 0..texels {
        let value = f32::from((index as u16).wrapping_add(seed));
        for channel in 0..4_u16 {
            let bits = ktx2::f32_to_f16_bits((value + f32::from(channel)) * 0.25);
            out.extend_from_slice(&bits.to_le_bytes());
        }
    }
    out
}

#[test]
fn f16_bit_conversion_handles_boundary_values() {
    // Signed zero, powers of two and the largest finite value.
    assert_eq!(ktx2::f32_to_f16_bits(0.0), 0x0000);
    assert_eq!(ktx2::f32_to_f16_bits(-0.0), 0x8000);
    assert_eq!(ktx2::f32_to_f16_bits(1.0), 0x3C00);
    assert_eq!(ktx2::f32_to_f16_bits(-1.0), 0xBC00);
    assert_eq!(ktx2::f32_to_f16_bits(0.5), 0x3800);
    assert_eq!(ktx2::f32_to_f16_bits(65_504.0), 0x7BFF);
    assert_eq!(ktx2::f32_to_f16_bits(-65_504.0), 0xFBFF);
    assert_eq!(
        ktx2::f32_to_f16_bits(65_520.0),
        0x7C00,
        "the halfway value above the largest finite value rounds up to infinity"
    );

    // Subnormal boundaries: the smallest subnormal, a value that rounds up to
    // it, and values that round (ties to even) down to signed zero.
    assert_eq!(ktx2::f32_to_f16_bits(5.960_464_5e-8), 0x0001);
    assert_eq!(ktx2::f32_to_f16_bits(-5.960_464_5e-8), 0x8001);
    assert_eq!(ktx2::f32_to_f16_bits(6.0e-8), 0x0001);
    assert_eq!(ktx2::f32_to_f16_bits(1.0e-8), 0x0000);
    assert_eq!(ktx2::f32_to_f16_bits(-1.0e-8), 0x8000);

    // Infinities and NaNs: a signaling NaN becomes quiet and keeps the high
    // mantissa bits as its payload.
    assert_eq!(ktx2::f32_to_f16_bits(f32::INFINITY), 0x7C00);
    assert_eq!(ktx2::f32_to_f16_bits(f32::NEG_INFINITY), 0xFC00);
    assert_eq!(ktx2::f32_to_f16_bits(f32::from_bits(0x7FC0_0000)), 0x7E00);
    assert_eq!(ktx2::f32_to_f16_bits(f32::from_bits(0xFFC0_0000)), 0xFE00);
    assert_eq!(
        ktx2::f32_to_f16_bits(f32::from_bits(0x7FBF_FFFF)),
        0x7FFF,
        "a signaling NaN payload is preserved and quieted"
    );

    // Decoding is exact for every interesting binary16 value.
    assert_eq!(ktx2::f16_bits_to_f32(0x0000).to_bits(), 0.0_f32.to_bits());
    assert_eq!(
        ktx2::f16_bits_to_f32(0x8000).to_bits(),
        (-0.0_f32).to_bits()
    );
    assert_eq!(ktx2::f16_bits_to_f32(0x3C00).to_bits(), 1.0_f32.to_bits());
    assert_eq!(
        ktx2::f16_bits_to_f32(0x7BFF).to_bits(),
        65_504.0_f32.to_bits()
    );
    assert_eq!(
        ktx2::f16_bits_to_f32(0x0001).to_bits(),
        5.960_464_5e-8_f32.to_bits()
    );
    assert_eq!(
        ktx2::f16_bits_to_f32(0x0400).to_bits(),
        6.103_515_6e-5_f32.to_bits()
    );
    assert!(ktx2::f16_bits_to_f32(0x7C00).is_infinite());
    assert!(ktx2::f16_bits_to_f32(0xFC00).is_sign_negative());
    assert!(ktx2::f16_bits_to_f32(0xFC00).is_infinite());
    assert!(ktx2::f16_bits_to_f32(0x7E00).is_nan());
    assert!(ktx2::f16_bits_to_f32(0x7FFF).is_nan());
}

#[test]
fn f16_round_trip_is_stable_across_a_value_sweep() {
    // Values around every boundary: zero, powers of two, the largest finite
    // value, subnormals, overflow and underflow.
    let sweep = [
        0.0_f32,
        -0.0,
        1.0,
        -1.0,
        0.5,
        0.25,
        16_384.0,
        65_504.0,
        65_520.0,
        3.402_823_5e38,
        6.103_515_6e-5,
        6.097_555_e-5,
        5.960_464_5e-8,
        1.0e-7,
        1.0e-8,
        0.333_333_34,
        -7.629_394_5e-6,
    ];
    for value in sweep {
        let bits = ktx2::f32_to_f16_bits(value);
        let decoded = ktx2::f16_bits_to_f32(bits);
        assert_eq!(
            ktx2::f32_to_f16_bits(decoded),
            bits,
            "f16 conversion is idempotent for {value}"
        );
    }

    // Exactly representable values survive both directions bit for bit.
    for (value, bits) in [
        (1.0_f32, 0x3C00_u16),
        (-2.0, 0xC000),
        (0.25, 0x3400),
        (1024.0, 0x6400),
        (65_504.0, 0x7BFF),
        (6.103_515_6e-5, 0x0400),
    ] {
        assert_eq!(ktx2::f32_to_f16_bits(value), bits, "{value} encodes");
        assert_eq!(
            ktx2::f16_bits_to_f32(bits).to_bits(),
            value.to_bits(),
            "{value} decodes"
        );
    }

    // Every non-NaN binary16 bit pattern survives f16 -> f32 -> f16.
    for bits in 0..=u16::MAX {
        let is_nan = bits & 0x7C00 == 0x7C00 && bits & 0x03FF != 0;
        if is_nan {
            assert!(ktx2::f16_bits_to_f32(bits).is_nan());
            continue;
        }
        assert_eq!(
            ktx2::f32_to_f16_bits(ktx2::f16_bits_to_f32(bits)),
            bits,
            "binary16 {bits:#06x} does not survive a decode/encode round trip"
        );
    }
}

#[test]
fn ktx2_round_trips_rgba16f_arrays() {
    let edge = 4_u32;
    let layers = vec![f16_image(edge, 1), f16_image(edge, 2)];
    let encoded = ktx2::write_rgba16f_2d_array(edge, &layers).expect("RGBA16F encodes");
    let decoded = ktx2::read_rgba16f(&encoded).expect("RGBA16F decodes");
    assert_eq!(decoded.edge, edge);
    assert_eq!(decoded.layers, 2);
    assert_eq!(decoded.faces, 1);
    assert_eq!(decoded.image_count(), 2);
    let flattened: Vec<u8> = layers.iter().flatten().copied().collect();
    assert_eq!(decoded.levels.len(), 1, "one mip level");
    assert_eq!(decoded.levels[0], flattened, "level data is layer-major");

    // A single layer is the common prefiltered shape.
    let single = vec![f16_image(edge, 7)];
    let encoded = ktx2::write_rgba16f_2d_array(edge, &single).expect("single layer encodes");
    let decoded = ktx2::read_rgba16f(&encoded).expect("single layer decodes");
    assert_eq!(decoded.layers, 1);
    assert_eq!(decoded.levels[0], single[0]);

    // The two readers never cross-accept formats.
    let rgba8 = {
        let layers = vec![vec![3_u8; (edge * edge * 4) as usize]];
        ktx2::write_rgba8_2d_array(edge, &layers).expect("RGBA8 encodes")
    };
    assert!(ktx2::read_rgba16f(&rgba8).is_err(), "RGBA8 is not RGBA16F");
    assert!(ktx2::read_rgba8(&encoded).is_err(), "RGBA16F is not RGBA8");

    // Header and descriptor tampering is rejected by name.
    let mut wrong_magic = encoded.clone();
    wrong_magic[0] = 0;
    assert!(ktx2::read_rgba16f(&wrong_magic).is_err());
    let mut wrong_type = encoded.clone();
    wrong_type[16..20].copy_from_slice(&4_u32.to_le_bytes());
    assert!(ktx2::read_rgba16f(&wrong_type).is_err());
    let mut wrong_dfd = encoded.clone();
    wrong_dfd[104 + 20] ^= 0xFF; // bytesPlane[0], the RGBA16F marker
    assert!(ktx2::read_rgba16f(&wrong_dfd).is_err());

    // A level whose declared length disagrees with the dimensions is rejected
    // before allocation: the single level index sits at bytes 80..104.
    let mut bad_length = encoded.clone();
    bad_length[88..96].copy_from_slice(&8_u64.to_le_bytes());
    bad_length[96..104].copy_from_slice(&8_u64.to_le_bytes());
    assert!(ktx2::read_rgba16f(&bad_length).is_err());
    assert!(ktx2::read_rgba16f(&encoded[..encoded.len() - 1]).is_err());
    let mut trailing = encoded;
    trailing.push(0);
    assert!(
        ktx2::read_rgba16f(&trailing).is_err(),
        "trailing bytes are rejected"
    );

    // Writer-side shape checks.
    assert!(ktx2::write_rgba16f_2d_array(edge, &[]).is_err());
    assert!(ktx2::write_rgba16f_2d_array(0, &single).is_err());
    assert!(ktx2::write_rgba16f_2d_array(edge, &[vec![0_u8; 8]]).is_err());
}

#[test]
fn ktx2_round_trips_an_rgba8_cube_with_mip_chain() {
    let edge = 8_u32;
    let levels: Vec<[Vec<u8>; 6]> = (0..3_u32)
        .map(|level| {
            let level_edge = edge >> level;
            std::array::from_fn(|face| {
                vec![(level * 6 + face as u32) as u8; (level_edge * level_edge * 4) as usize]
            })
        })
        .collect();
    let encoded = ktx2::write_rgba8_cube_with_mips(edge, &levels).expect("mip chain encodes");
    let decoded = ktx2::read_rgba8(&encoded).expect("mip chain decodes");
    assert_eq!(decoded.edge, edge);
    assert_eq!(decoded.faces, 6);
    assert_eq!(decoded.layers, 0);
    assert_eq!(decoded.levels.len(), 3, "every mip level survives");
    for (level, (original, decoded)) in levels.iter().zip(&decoded.levels).enumerate() {
        let expected: Vec<u8> = original.iter().flatten().copied().collect();
        assert_eq!(*decoded, expected, "level {level} is face-major");
    }

    // A level whose declared length does not match `edge >> level` is
    // rejected: the second level index entry starts at byte 104.
    let mut bad_length = encoded.clone();
    bad_length[112..120].copy_from_slice(&4_u64.to_le_bytes());
    bad_length[120..128].copy_from_slice(&4_u64.to_le_bytes());
    assert!(
        ktx2::read_rgba8(&bad_length).is_err(),
        "a mip level must hold (edge >> level)^2 texels"
    );
    assert!(ktx2::read_rgba8(&encoded[..encoded.len() - 1]).is_err());
    let mut trailing = encoded;
    trailing.push(0);
    assert!(ktx2::read_rgba8(&trailing).is_err());

    // The chain may not run past one texel, while a short chain is legal.
    let too_deep: Vec<[Vec<u8>; 6]> = (0..5_u32)
        .map(|level| {
            let level_edge = edge >> level;
            std::array::from_fn(|_| vec![0_u8; (level_edge * level_edge * 4) as usize])
        })
        .collect();
    assert!(ktx2::write_rgba8_cube_with_mips(edge, &too_deep).is_err());
    assert!(ktx2::write_rgba8_cube_with_mips(edge, &levels[..2]).is_ok());

    // A face whose byte length does not match its level is rejected.
    let mut wrong_face = levels.clone();
    wrong_face[1][2] = vec![0_u8; 4];
    assert!(ktx2::write_rgba8_cube_with_mips(edge, &wrong_face).is_err());

    // Shape violations are rejected before any allocation.
    assert!(ktx2::write_rgba8_cube_with_mips(edge, &[]).is_err());
    assert!(ktx2::write_rgba8_cube_with_mips(0, &levels).is_err());
    assert!(ktx2::write_rgba8_cube_with_mips(edge + 1, &levels).is_err());
}

#[test]
fn archives_round_trip_deterministically_and_reject_traversal() {
    let dir = scratch_dir("archive");
    let first = dir.join("first.placesmap");
    let second = dir.join("second.placesmap");
    let entries = || {
        vec![
            PendingEntry {
                name: "manifest.json".to_string(),
                bytes: b"{}".to_vec(),
            },
            PendingEntry {
                name: "blobs/aa.mesh".to_string(),
                bytes: vec![1, 2, 3, 4],
            },
        ]
    };
    write_archive(&first, entries()).expect("first archive");
    write_archive(&second, entries()).expect("second archive");
    assert_eq!(
        std::fs::read(&first).expect("read first"),
        std::fs::read(&second).expect("read second"),
        "the same content publishes byte-identical archives"
    );

    let file = std::fs::File::open(&first).expect("open first");
    let mut reader = PackageReader::new(file).expect("archive opens");
    assert_eq!(reader.len(), 2);
    assert!(reader.contains("manifest.json"));
    assert_eq!(
        reader.read_entry("blobs/aa.mesh", 16).expect("entry reads"),
        vec![1, 2, 3, 4]
    );
    assert!(
        reader.read_entry("blobs/aa.mesh", 3).is_err(),
        "cap enforced"
    );
    assert!(reader.read_entry("missing", 16).is_err());

    // Content-addressed reads verify the hash in the name.
    let mut blob_reader =
        PackageReader::new(std::fs::File::open(&first).expect("open")).expect("open");
    assert!(
        blob_reader.read_blob("blobs/aa.mesh", 16).is_err(),
        "bad blob name"
    );

    // Adversarial archives: traversal, absolute names, duplicates.
    for (tag, names) in [
        ("traversal", vec!["../escape", "manifest.json"]),
        ("absolute", vec!["/abs", "manifest.json"]),
        ("drive", vec!["C:evil", "manifest.json"]),
        ("backslash", vec!["a\\b", "manifest.json"]),
        ("normalized-duplicate", vec!["./a", "a", "manifest.json"]),
    ] {
        let path = dir.join(format!("{tag}.placesmap"));
        write_adversarial_zip(&path, &names);
        let file = std::fs::File::open(&path).expect("open adversarial");
        assert!(
            PackageReader::new(file).is_err(),
            "{tag} archive must be rejected"
        );
    }

    // Exact duplicate entries collapse inside the ZIP reader's name index, so
    // the raw central-directory scan is the only defence. The writer refuses
    // to create one, so the archive is assembled by hand (the scanner reads
    // the central directory before any local entry is touched).
    let duplicate = raw_central_directory(&["manifest.json", "manifest.json"]);
    assert!(
        PackageReader::new(std::io::Cursor::new(duplicate)).is_err(),
        "an exact duplicate entry must be rejected"
    );

    // Too many entries.
    let path = dir.join("many.placesmap");
    let names: Vec<String> = (0..=MAX_ENTRIES).map(|index| format!("e{index}")).collect();
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    write_adversarial_zip(&path, &refs);
    let file = std::fs::File::open(&path).expect("open many");
    assert!(PackageReader::new(file).is_err());

    let _ = std::fs::remove_dir_all(&dir);
}

/// Builds a bare ZIP end-of-central-directory plus central directory carrying
/// `names`, with no local entries. The package scanner validates exactly these
/// bytes before the ZIP reader builds its name-keyed index.
fn raw_central_directory(names: &[&str]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    for name in names {
        out.extend_from_slice(&0x0201_4b50_u32.to_le_bytes()); // signature
        out.extend_from_slice(&20_u16.to_le_bytes()); // version made by
        out.extend_from_slice(&20_u16.to_le_bytes()); // version needed
        out.extend_from_slice(&0_u16.to_le_bytes()); // flags
        out.extend_from_slice(&0_u16.to_le_bytes()); // method (stored)
        out.extend_from_slice(&0_u16.to_le_bytes()); // time
        out.extend_from_slice(&0_u16.to_le_bytes()); // date
        out.extend_from_slice(&0_u32.to_le_bytes()); // crc
        out.extend_from_slice(&1_u32.to_le_bytes()); // compressed size
        out.extend_from_slice(&1_u32.to_le_bytes()); // uncompressed size
        out.extend_from_slice(&u16::try_from(name.len()).expect("name fits").to_le_bytes());
        out.extend_from_slice(&0_u16.to_le_bytes()); // extra length
        out.extend_from_slice(&0_u16.to_le_bytes()); // comment length
        out.extend_from_slice(&0_u16.to_le_bytes()); // disk start
        out.extend_from_slice(&0_u16.to_le_bytes()); // internal attributes
        out.extend_from_slice(&0o100_644_u32.to_le_bytes()); // external attributes
        out.extend_from_slice(&0_u32.to_le_bytes()); // local header offset
        out.extend_from_slice(name.as_bytes());
    }
    let cd_size = u32::try_from(out.len()).expect("directory fits");
    out.extend_from_slice(&0x0605_4b50_u32.to_le_bytes()); // EOCD signature
    out.extend_from_slice(&0_u16.to_le_bytes()); // disk
    out.extend_from_slice(&0_u16.to_le_bytes()); // cd disk
    out.extend_from_slice(
        &u16::try_from(names.len())
            .expect("count fits")
            .to_le_bytes(),
    );
    out.extend_from_slice(
        &u16::try_from(names.len())
            .expect("count fits")
            .to_le_bytes(),
    );
    out.extend_from_slice(&cd_size.to_le_bytes());
    out.extend_from_slice(&0_u32.to_le_bytes()); // cd offset
    out.extend_from_slice(&0_u16.to_le_bytes()); // comment length
    out
}

fn write_adversarial_zip(path: &std::path::Path, names: &[&str]) {
    let file = std::fs::File::create(path).expect("create zip");
    let mut writer = zip::ZipWriter::new(file);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for name in names {
        writer.start_file(*name, options).expect("start entry");
        writer.write_all(b"x").expect("write entry");
    }
    writer.finish().expect("finish zip");
}

fn role_entry(name: &str, role: &str) -> super::PackageEntry {
    super::PackageEntry {
        name: name.to_string(),
        role: role.to_string(),
        bytes: 4,
        sha256: "a".repeat(64),
    }
}

#[test]
fn manifests_reject_wrong_versions_capabilities_and_shapes() {
    let mut entries = vec![
        "blobs/aa.mesh".to_string(),
        "blobs/aa.props".to_string(),
        "blobs/aa.lighting".to_string(),
        "blobs/aa.collision".to_string(),
        "blobs/aa.navigation".to_string(),
        "semantics.json".to_string(),
    ];
    entries.sort();
    let good = Manifest {
        package_format: FORMAT_VERSION,
        id: "test".to_string(),
        name: "Test".to_string(),
        author: String::new(),
        created_by: "test".to_string(),
        compiler_fingerprint: "0".repeat(64),
        lighting_fingerprint: Some("0".repeat(64)),
        required_capabilities: vec!["geometry".to_string()],
        dependencies: Vec::new(),
        entries: vec![
            role_entry("blobs/aa.mesh", "mesh"),
            role_entry("blobs/aa.props", "props"),
            role_entry("blobs/aa.lighting", "lighting"),
            role_entry("blobs/aa.collision", "collision"),
            role_entry("blobs/aa.navigation", "navigation"),
            super::PackageEntry {
                name: "semantics.json".to_string(),
                role: "semantics".to_string(),
                bytes: 2,
                sha256: "b".repeat(64),
            },
        ],
        variants: vec![Variant {
            lightmap_quality: "off".to_string(),
            quality_profile: "low".to_string(),
            lightmap_failure: None,
            entries: VariantEntries {
                irradiance: None,
                mesh: "blobs/aa.mesh".to_string(),
                props: "blobs/aa.props".to_string(),
                lighting: "blobs/aa.lighting".to_string(),
                collision: "blobs/aa.collision".to_string(),
                navigation: "blobs/aa.navigation".to_string(),
                lightmaps: None,
                lightmaps_meta: None,
                probes: Vec::new(),
            },
        }],
    };
    good.validate(&entries)
        .expect("a well-formed manifest validates");

    let mut wrong_version = good.clone();
    wrong_version.package_format = FORMAT_VERSION + 1;
    assert!(wrong_version.validate(&entries).is_err());

    let mut unknown = good.clone();
    unknown
        .required_capabilities
        .push("hdr-lighting".to_string());
    assert!(unknown.validate(&entries).is_err());

    let mut missing = good.clone();
    missing.entries.clear();
    assert!(missing.validate(&entries).is_err());

    let mut duplicate_variant = good.clone();
    duplicate_variant.variants.push(good.variants[0].clone());
    assert!(duplicate_variant.validate(&entries).is_err());

    let mut unsafe_name = good.clone();
    unsafe_name.entries[0].name = "../evil.mesh".to_string();
    assert!(unsafe_name.validate(&entries).is_err());

    let mut bad_hash = good.clone();
    bad_hash.entries[0].sha256 = "nope".to_string();
    assert!(bad_hash.validate(&entries).is_err());

    // Every archive entry must be declared.
    let mut extra = entries.clone();
    extra.push("extra.json".to_string());
    extra.sort();
    assert!(good.validate(&extra).is_err());

    // A variant without lightmaps needs a recorded failure.
    let mut missing_lightmaps = good;
    missing_lightmaps.variants[0].lightmap_quality = "full".to_string();
    assert!(missing_lightmaps.validate(&entries).is_err());
    missing_lightmaps.variants[0].lightmap_failure = Some("page overflow".to_string());
    missing_lightmaps
        .validate(&entries)
        .expect("a recorded failure explains a vertex-lit variant");
}

#[test]
fn mesh_records_round_trip_and_reject_malformed_input() {
    let level = tiny_level("mesh_round_trip");
    let build = build_level_build(&level, crate::quality::LightmapQuality::Off);
    let bytes = super::mesh::write_mesh(&build.mesh).expect("mesh encodes");
    let decoded = super::mesh::read_mesh(&bytes).expect("mesh decodes");
    assert_eq!(decoded.ranges.len(), build.mesh.ranges.len());
    assert_eq!(decoded.vertex_count, build.mesh.vertex_count);
    assert_eq!(decoded.index_count, build.mesh.index_count);
    assert_eq!(decoded.batches.floor_batch, build.mesh.batches.floor_batch);
    for (original, decoded) in build.mesh.ranges.iter().zip(&decoded.ranges) {
        assert_eq!(original.key, decoded.key);
        assert_eq!(original.vertices, decoded.vertices);
        assert_eq!(original.indices, decoded.indices);
        assert_eq!(original.bounds, decoded.bounds);
    }
    assert!(super::mesh::read_mesh(&bytes[..bytes.len() - 3]).is_err());
    assert!(super::mesh::read_mesh(&[0, 1, 2, 3]).is_err());
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(super::mesh::read_mesh(&trailing).is_err());
    let mut wrong_version = bytes;
    wrong_version[4..6].copy_from_slice(&2_u16.to_le_bytes());
    assert!(super::mesh::read_mesh(&wrong_version).is_err());
}

#[test]
fn prop_records_round_trip_with_textures_reattached_at_load() {
    // One catalogued prop is enough; the textures deliberately travel by model
    // reference, not by value.
    let level = crate::level::LevelDef::from_json(
        r#"{
            "format_version": 3, "id": "props_round_trip", "name": "Props",
            "spawn": { "x": 1.0, "z": 1.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0, "height": 3.0 } ],
            "ceiling_lights": [ { "fixture": "core:ceiling_panel_01", "x": 2.0, "z": 2.0 } ],
            "props": [ { "model": "core:exit_sign", "x": 2.0, "z": 3.0 } ]
        }"#,
    )
    .expect("prop fixture parses");
    let build = build_level_build(&level, crate::quality::LightmapQuality::Off);
    assert!(!build.batches.is_empty(), "the fixture places one prop");
    let bytes = super::props::write_props(&build.batches).expect("props encode");
    let decoded = super::props::read_props(&bytes).expect("props decode");
    assert_eq!(decoded.len(), build.batches.len());
    for (original, decoded) in build.batches.iter().zip(&decoded) {
        assert_eq!(original.model, decoded.model);
        assert_eq!(original.vertices, decoded.vertices);
        assert_eq!(original.indices, decoded.indices);
        assert!(
            decoded.textures.is_empty(),
            "pixels travel by model reference"
        );
    }
    assert!(super::props::read_props(&bytes[..4]).is_err());
}

#[test]
fn lighting_records_round_trip_sample_for_sample() {
    let level = tiny_level("lighting_round_trip");
    let build = build_level_build(&level, crate::quality::LightmapQuality::Full);
    let bytes = super::lighting::write_lighting(&build.lighting).expect("lighting encodes");
    let decoded = super::lighting::read_lighting(&bytes).expect("lighting decodes");
    assert_eq!(
        decoded.summary(),
        build.lighting.summary(),
        "the summary is identical after a round trip"
    );
    for x in [0.5_f32, 1.5, 2.5, 3.5] {
        for z in [0.5_f32, 1.5, 2.5, 3.5] {
            for y in [0.0_f32, 1.5, 2.9] {
                let sampled = decoded.sample(x, y, z);
                let reference = build.lighting.sample(x, y, z);
                assert!(
                    sampled.r.to_bits() == reference.r.to_bits()
                        && sampled.g.to_bits() == reference.g.to_bits()
                        && sampled.b.to_bits() == reference.b.to_bits(),
                    "sampled light at ({x}, {y}, {z}) is unchanged"
                );
            }
        }
    }
    assert!(super::lighting::read_lighting(&bytes[..16]).is_err());
}

#[test]
fn lightmap_records_round_trip_charts_and_pages() {
    let level = tiny_level("lightmaps_round_trip");
    let build = build_level_build(&level, crate::quality::LightmapQuality::Full);
    let atlas = build
        .lightmaps
        .as_ref()
        .expect("the fixture builds lightmaps");
    let (meta, pages) = super::lightmaps::write_lightmaps(atlas).expect("atlas encodes");
    let decoded = super::lightmaps::read_lightmaps(&meta, &pages).expect("atlas decodes");
    assert_eq!(decoded.pages.len(), atlas.pages.len());
    assert_eq!(decoded.charts.len(), atlas.charts.len());
    assert_eq!(decoded.cache_key, atlas.cache_key);
    assert_eq!(decoded.padding, atlas.padding);
    for (original, decoded) in atlas.pages.iter().zip(&decoded.pages) {
        assert_eq!(original.width, decoded.width);
        assert_eq!(
            original.texels.len(),
            decoded.texels.len(),
            "page texel count is exact"
        );
        // The container stores half floats, so every channel is compared at
        // f16 precision: the test cannot ask for more than the format keeps.
        for (expected, actual) in original.texels.iter().zip(&decoded.texels) {
            for channel in 0..3 {
                let wanted = crate::package::ktx2::f16_bits_to_f32(
                    crate::package::ktx2::f32_to_f16_bits(expected.irradiance[channel]),
                );
                assert!(
                    (actual.irradiance[channel] - wanted).abs() <= 1.0e-3,
                    "irradiance channel {channel}: {} vs {wanted}",
                    actual.irradiance[channel]
                );
                let wanted = crate::package::ktx2::f16_bits_to_f32(
                    crate::package::ktx2::f32_to_f16_bits(expected.direction[channel]),
                );
                assert!(
                    (actual.direction[channel] - wanted).abs() <= 1.0e-3,
                    "direction channel {channel}: {} vs {wanted}",
                    actual.direction[channel]
                );
                let wanted = crate::package::ktx2::f16_bits_to_f32(
                    crate::package::ktx2::f32_to_f16_bits(expected.axis[channel.min(1)]),
                );
                assert!(
                    (actual.axis[channel.min(1)] - wanted).abs() <= 1.0e-3,
                    "axis channel {channel}: {} vs {wanted}",
                    actual.axis[channel.min(1)]
                );
            }
        }
    }
    assert_eq!(decoded.charts, atlas.charts);
    assert!(super::lightmaps::read_lightmaps(&meta[..meta.len() / 2], &pages).is_err());
}

#[test]
fn probe_captures_are_required_when_the_geometry_routes_probes() {
    // A polished floor routes a probe; an empty capture set must be refused.
    let level = crate::level::LevelDef::from_json(
        r#"{
            "format_version": 3, "id": "probe_gate", "name": "Probe Gate",
            "spawn": { "x": 1.0, "z": 1.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0, "height": 3.0,
                         "material": "core:linoleum_polished_01" } ],
            "ceiling_lights": [ { "fixture": "core:ceiling_panel_01", "x": 2.0, "z": 2.0 } ]
        }"#,
    )
    .expect("probe fixture parses");
    let build = build_level_build(&level, crate::quality::LightmapQuality::Off);
    let catalog = crate::loader::PropCatalog::load_default();
    let materials = crate::render::logical_materials(&level);
    let state = crate::render::MaterialRenderState::from_table(&materials);
    let routing = crate::render::routing_from_mesh(
        &build.mesh,
        &state.reflections,
        materials.entries().len(),
    );
    assert!(
        !routing.probe_points.is_empty(),
        "the polished floor routes a probe"
    );
    let empty = crate::package::world::ProbeCaptures::default();
    assert!(
        crate::package::world::validate_probe_captures(&build.mesh, &materials, &empty).is_err()
    );
    let _ = catalog;

    // A level with no reflective routing accepts an empty capture set.
    let plain = tiny_level("no_probes");
    let plain_build = build_level_build(&plain, crate::quality::LightmapQuality::Off);
    let plain_materials = crate::render::logical_materials(&plain);
    crate::package::world::validate_probe_captures(&plain_build.mesh, &plain_materials, &empty)
        .expect("no routing means no captures are needed");
}

#[test]
fn one_planar_material_on_two_floor_levels_keeps_both_routes() {
    // AUD-003 through the real emitter: the room's floor and a lowered floor
    // region share the wet-deck material at two elevations. Both ranges must
    // keep their own plane route instead of the later one overwriting the
    // earlier one.
    let level = crate::level::LevelDef::from_json(
        r#"{
            "format_version": 3, "id": "two_planes", "name": "Two Planes",
            "spawn": { "x": 1.0, "z": 1.0 },
            "rooms": [
                { "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0, "height": 3.0 },
                { "x": 12.0, "z": 0.0, "width": 4.0, "depth": 4.0, "height": 3.0 }
            ],
            "floor_regions": [
                { "x": 0.5, "z": 0.5, "width": 2.0, "depth": 2.0, "offset_y": -1.0,
                  "material": "core:pool_deck_wet_01" },
                { "x": 12.5, "z": 0.5, "width": 2.0, "depth": 2.0, "offset_y": -2.0,
                  "material": "core:pool_deck_wet_01" }
            ]
        }"#,
    )
    .expect("two-plane fixture parses");
    let build = build_level_build(&level, crate::quality::LightmapQuality::Off);
    let materials = crate::render::logical_materials(&level);
    let state = crate::render::MaterialRenderState::from_table(&materials);
    let routing = crate::render::routing_from_mesh(
        &build.mesh,
        &state.reflections,
        materials.entries().len(),
    );
    let wet = materials
        .index_of("core:pool_deck_wet_01")
        .expect("the wet deck material resolves");
    assert!(
        state.reflections[usize::from(wet)].is_planar(),
        "the fixture material must author a planar reflection"
    );
    let mut routed: Vec<usize> = build
        .mesh
        .ranges
        .iter()
        .enumerate()
        .filter(|(_, range)| range.key.material == wet)
        .filter_map(|(index, _)| routing.plane_of_range(index))
        .collect();
    routed.sort_unstable();
    routed.dedup();
    assert_eq!(
        routed,
        vec![0, 1],
        "the room floor and the lowered patch keep distinct planes"
    );
}

#[test]
fn compiler_builds_validates_reuses_and_keeps_failed_outputs() {
    let dir = scratch_dir("compiler");
    let source = dir.join("fixture.json");
    std::fs::write(&source, tiny_level_json("compiler_fixture")).expect("write source");
    let out = dir.join("fixture.placesmap");
    let request = crate::compiler::BuildRequest {
        source: source.clone(),
        out: out.clone(),
        asset_root: std::path::PathBuf::from("assets"),
        variants: vec![crate::quality::LightmapQuality::Off],
        workers: 1,
        force: false,
        capture_probes: false,
    };
    let first = crate::compiler::build(&request).expect("first build");
    assert!(first.rebuilt);
    let first_bytes = std::fs::read(&out).expect("package exists");
    let report = crate::compiler::validate(&out).expect("the package validates");
    assert_eq!(report.id, "compiler_fixture");
    assert_eq!(report.variants, vec!["off".to_string()]);

    // An unchanged source reuses the package.
    let second = crate::compiler::build(&request).expect("second build");
    assert!(!second.rebuilt, "an unchanged build is reused");
    assert_eq!(
        std::fs::read(&out).expect("package still exists"),
        first_bytes
    );

    // An edited source invalidates the fingerprint and rebuilds.
    std::fs::write(
        &source,
        tiny_level_json("compiler_fixture").replace("\"width\": 4.0", "\"width\": 5.0"),
    )
    .expect("edit source");
    let third = crate::compiler::build(&request).expect("third build");
    assert!(third.rebuilt, "an edited source rebuilds");
    assert_ne!(third.fingerprint, first.fingerprint);

    // A broken source never replaces the last valid package.
    let good_bytes = std::fs::read(&out).expect("valid package exists");
    std::fs::write(&source, "{ not json").expect("break source");
    assert!(crate::compiler::build(&request).is_err());
    assert_eq!(
        std::fs::read(&out).expect("package survives"),
        good_bytes,
        "a failed build leaves the previous package untouched"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn compiler_rebuilds_a_corrupted_package_and_keeps_inputs_read_only() {
    let dir = scratch_dir("compiler-integrity");
    let source = dir.join("fixture.json");
    std::fs::write(&source, tiny_level_json("compiler_integrity")).expect("write source");
    let source_before = std::fs::read(&source).expect("read source");
    let out = dir.join("fixture.placesmap");
    let request = crate::compiler::BuildRequest {
        source: source.clone(),
        out: out.clone(),
        asset_root: std::path::PathBuf::from("assets"),
        variants: vec![crate::quality::LightmapQuality::Off],
        workers: 1,
        force: false,
        capture_probes: false,
    };
    crate::compiler::build(&request).expect("initial build");
    let good = std::fs::read(&out).expect("package exists");

    // A stale interrupted artifact beside the output must not block a build and
    // must be replaced by the published archive.
    let partial = dir.join("fixture.placesmap.partial");
    std::fs::write(&partial, b"half-written").expect("stale partial");
    crate::compiler::build(&request).expect("rebuild over a stale partial");

    // Rewrite the package with one blob's bytes changed and the manifest left
    // alone: the fingerprint still matches, so only the integrity check can
    // catch it.
    let mut entries = Vec::new();
    {
        let mut reader = super::PackageReader::new(std::fs::File::open(&out).expect("open"))
            .expect("open package");
        let names = reader.names().to_vec();
        for entry in &names {
            let mut bytes = reader
                .read_entry(entry, super::MAX_ENTRY_BYTES)
                .expect("entry reads");
            let is_mesh = std::path::Path::new(entry)
                .extension()
                .is_some_and(|extension| extension == "mesh");
            if entry.starts_with("blobs/")
                && is_mesh
                && let Some(first) = bytes.first_mut()
            {
                *first ^= 0xFF;
            }
            entries.push(PendingEntry {
                name: entry.clone(),
                bytes,
            });
        }
    }
    write_archive(&out, entries).expect("corrupt package publishes");
    assert!(
        crate::compiler::validate(&out).is_err(),
        "the fixture is corrupt before the rebuild"
    );
    let report = crate::compiler::build(&request).expect("rebuild after corruption");
    assert!(report.rebuilt, "a corrupted package is rebuilt, not reused");
    crate::compiler::validate(&out).expect("the rebuilt package validates");
    assert!(
        std::fs::read(&out).expect("package exists") != good || report.rebuilt,
        "the published archive is either repaired or deliberately rebuilt"
    );
    // Inputs are read-only: the source bytes are untouched.
    assert_eq!(
        std::fs::read(&source).expect("read source"),
        source_before,
        "compilation never changes its source"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn archive_publication_is_atomic_and_deterministic_for_collections() {
    let dir = scratch_dir("collection");
    let one = dir.join("one.json");
    let two = dir.join("two.json");
    std::fs::write(&one, tiny_level_json("collection_one")).expect("write one");
    std::fs::write(&two, tiny_level_json("collection_two")).expect("write two");
    let results = crate::compiler::build_collection(
        &dir,
        &std::path::PathBuf::from("assets"),
        &[crate::quality::LightmapQuality::Off],
        1,
        false,
    )
    .expect("collection builds");
    assert_eq!(results.len(), 2, "one result per source");
    assert!(
        results.iter().all(Result::is_ok),
        "independent sources all build: {results:?}"
    );
    // The published archives are byte-identical across a rebuild from scratch.
    let first_one = std::fs::read(dir.join("one.placesmap")).expect("one package");
    let first_two = std::fs::read(dir.join("two.placesmap")).expect("two package");
    let results = crate::compiler::build_collection(
        &dir,
        &std::path::PathBuf::from("assets"),
        &[crate::quality::LightmapQuality::Off],
        1,
        true,
    )
    .expect("forced collection rebuild");
    assert!(results.iter().all(Result::is_ok));
    assert_eq!(
        std::fs::read(dir.join("one.placesmap")).expect("one package"),
        first_one,
        "rebuild output ordering is deterministic"
    );
    assert_eq!(
        std::fs::read(dir.join("two.placesmap")).expect("two package"),
        first_two
    );

    // A lightmapped variant is deterministic too: the packaged record carries
    // no wall-clock measurement.
    let lit = dir.join("lit.json");
    // The wall's `faces` map exercises the HashMap ordering that canonical
    // JSON must neutralise in both the semantic record and the cache key.
    std::fs::write(
        &lit,
        tiny_level_json("collection_lit").replace(
            "\"ceiling_lights\"",
            "\"walls\": [ { \"x\": 2.0, \"z\": 0.0, \"width\": 4.0, \"depth\": 0.2, \
             \"faces\": { \"south\": \"core:metal_brushed_01\", \"north\": \"core:wallpaper_yellow_01\" } } ], \
             \"ceiling_lights\"",
        ),
    )
    .expect("write lit");
    let request = crate::compiler::BuildRequest {
        source: lit,
        out: dir.join("lit.placesmap"),
        asset_root: std::path::PathBuf::from("assets"),
        variants: vec![crate::quality::LightmapQuality::Full],
        workers: 1,
        force: true,
        capture_probes: false,
    };
    crate::compiler::build(&request).expect("first lit build");
    let first_lit = std::fs::read(dir.join("lit.placesmap")).expect("lit package");
    crate::compiler::build(&request).expect("second lit build");
    assert_eq!(
        std::fs::read(dir.join("lit.placesmap")).expect("lit package"),
        first_lit,
        "a lightmapped package is byte-identical across builds"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
