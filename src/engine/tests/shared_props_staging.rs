//! Candidate storage regressions: identical recipes, spilled-chunk lookup,
//! tamper refusal and error/cancel cleanup. Synthetic bytes cross one 16 MiB
//! boundary (under 40 MiB live); no assets, map bake, renderer or network.

use std::cell::Cell;
use std::io::Write as _;

use super::{SegmentStorage, StagedSegments, factor_props_staged};
use crate::engine::package::{props, shared_props};

#[test]
fn staged_recipes_match_memory_and_cleanup_on_cancel() -> Result<(), String> {
    let destination = std::env::temp_dir().join(format!(
        "places-props-parity-{}.placesmap",
        std::process::id()
    ));
    let bytes = props::write_props(&[])?;
    for version in [2, shared_props::SHARED_PROPS_RECIPE_VERSION] {
        let memory = shared_props::factor_props_with_version(&[&bytes, &bytes], version)?;
        let staged = factor_props_staged(&[&bytes, &bytes], version, &destination, &|| Ok(()))?;
        let root = staged.storage.root.clone().ok_or("Missing stage root")?;
        assert_eq!(
            memory.recipes, staged.recipes,
            "candidate backend must preserve recipe bytes and input order"
        );
        assert_eq!(
            memory.segments.len(),
            staged.segments().len(),
            "physical segment count"
        );
        for (expected, segment) in memory.segments.iter().zip(staged.segments()) {
            assert_eq!(
                *expected,
                segment.read_verified()?,
                "physical segment identity must be unchanged"
            );
        }
        staged.cleanup()?;
        assert!(
            !root.exists(),
            "successful candidate ownership must retire its disk files"
        );
    }
    let calls = Cell::new(0_usize);
    let cancelled = factor_props_staged(&[&bytes], 2, &destination, &|| {
        calls.set(calls.get().saturating_add(1));
        if calls.get() >= 3 {
            Err("synthetic cancellation".into())
        } else {
            Ok(())
        }
    });
    assert!(cancelled.is_err(), "cancellation must stop factoring");
    assert!(
        !destination.with_extension("props-staging.partial").exists(),
        "failed candidate must clean up its owned stage"
    );
    Ok(())
}

#[test]
fn staged_spill_preserves_chunks_and_refuses_tampering() -> Result<(), String> {
    let destination = std::env::temp_dir().join(format!(
        "places-props-spill-{}.placesmap",
        std::process::id()
    ));
    let mut memory = SegmentStorage::Memory(Vec::new());
    let mut staged = SegmentStorage::Staged(StagedSegments::new(&destination)?);
    let mut locations = Vec::new();
    for seed in 0..257_u16 {
        let bytes: Vec<u8> = (0..shared_props::SHARED_PROPS_CHUNK_BYTES)
            .map(|n| u8::try_from(n.saturating_add(usize::from(seed)) % 251).unwrap_or(0))
            .collect();
        let expected = memory.append(&bytes, &|| Ok(()))?;
        let actual = staged.append(&bytes, &|| Ok(()))?;
        assert_eq!(
            (actual.segment, actual.offset, actual.length),
            (expected.segment, expected.offset, expected.length)
        );
        assert!(
            staged.matches(actual, &bytes)?,
            "active or spilled chunk must match"
        );
        locations.push(actual);
    }
    assert_eq!(
        memory.identities(&|| Ok(()))?,
        staged.identities(&|| Ok(()))?,
        "segment boundaries and identities must be identical"
    );
    for (seed, location) in locations.iter().enumerate() {
        let bytes: Vec<u8> = (0..shared_props::SHARED_PROPS_CHUNK_BYTES)
            .map(|n| u8::try_from(n.saturating_add(seed) % 251).unwrap_or(0))
            .collect();
        assert!(
            staged.matches(*location, &bytes)?,
            "lookup must recover earlier spilled segments"
        );
    }
    if let SegmentStorage::Staged(stage) = &staged {
        assert!(
            stage.active.capacity()
                <= usize::try_from(shared_props::MAX_SHARED_PROPS_SEGMENT_BYTES)
                    .map_err(|e| e.to_string())?,
            "active capacity must remain bounded"
        );
        let segment = stage.segments.first().ok_or("Missing spilled segment")?;
        std::fs::write(&segment.path, b"changed").map_err(|e| e.to_string())?;
        assert!(
            segment.read_verified().is_err(),
            "untrusted staged bytes must not reach publication"
        );
    }
    drop(staged);
    assert!(
        !destination.with_extension("props-staging.partial").exists(),
        "drop must retire spilled files after failure"
    );
    Ok(())
}

#[test]
fn occupied_stage_names_are_preserved() -> Result<(), String> {
    let destination = std::env::temp_dir().join(format!(
        "places-props-occupied-{}.placesmap",
        std::process::id()
    ));
    let obstacle = destination.with_extension("props-staging.partial");
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&obstacle)
        .map_err(|e| e.to_string())?;
    file.write_all(b"preserved restart artifact")
        .map_err(|e| e.to_string())?;
    drop(file);
    let stage = StagedSegments::new(&destination)?;
    assert_ne!(
        stage.root.as_ref(),
        Some(&obstacle),
        "restart must exclusively reserve another sibling"
    );
    drop(stage);
    assert_eq!(
        std::fs::read(&obstacle).map_err(|e| e.to_string())?,
        b"preserved restart artifact"
    );
    std::fs::remove_file(obstacle).map_err(|e| e.to_string())?;
    Ok(())
}
