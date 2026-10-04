//! Small immutable prepared-cache contracts; no shipped models or package
//! records, and no GPU work.
#![allow(
    clippy::expect_used,
    reason = "Fixture construction reports exact failures."
)] // Fixture construction reports exact failures.
#![allow(
    clippy::arithmetic_side_effects,
    reason = "Fixture sizes are tiny bounded values."
)] // Fixture sizes are tiny bounded values.
use super::*;

fn empty_records() -> Arc<PreparedRecords> {
    let level = crate::level::LevelDef::from_json(
        r#"{"format_version":2,"id":"cache_test","name":"Cache Test","spawn":{"x":0,"z":0}}"#,
    )
    .expect("tiny level");
    let catalog = crate::loader::PropCatalog::builtin();
    let materials = crate::materials::MaterialTable::logical(&level, catalog.assets(), None);
    let mut assets = PropAssets::with_root("unused-cache-fixture-root");
    let build = crate::render::build_level_geometry_timed_with_lightmaps(
        &level,
        &catalog,
        &mut assets,
        &materials,
        crate::render::LightmapBuildOptions::for_lightmaps(LightmapQuality::Off),
        None,
    );
    let collision = CollisionWorld::from_level(&level);
    let compiled = CompiledCollision {
        walls: collision.walls.clone(),
        floor: collision.floor.clone(),
        ceiling: collision.ceiling.clone(),
        water: collision.water.clone(),
        ladders: collision.ladders,
    };
    let navigation = crate::package::navigation::NavGrid::default();
    let retained_bytes =
        PreparedRecords::retained_bytes(&build, &compiled, &navigation, &ProbeCaptures::default());
    Arc::new(PreparedRecords {
        build: Arc::new(build),
        collision: compiled,
        navigation,
        probes: ProbeCaptures::default(),
        retained_bytes,
    })
}

fn key(number: u8) -> PackageKey {
    PackageKey {
        package_sha256: format!("{number:064x}"),
        quality: LightmapQuality::Off,
    }
}

#[test]
fn cache_hits_reuse_the_exact_records_and_lru_eviction_keeps_displayed_arcs_alive() {
    let mut cache = BuildCache::default();
    let first = empty_records();
    cache.insert(key(1), Arc::clone(&first));
    let hit = cache.get(&key(1)).expect("retained record is reusable");
    assert!(Arc::ptr_eq(&hit, &first), "the cache returns the exact Arc");
    cache.insert(key(2), empty_records());
    cache.insert(key(3), empty_records());
    assert!(cache.get(&key(1)).is_some(), "a hit refreshes recency");
    cache.insert(key(4), empty_records());
    assert_eq!(cache.entries.len(), 3, "entry bound holds");
    // The displayed world still owns its data after eviction.
    assert!(Arc::strong_count(&first) >= 1);
}

#[test]
fn byte_budget_evicts_old_records_and_oversized_entries_are_not_retained() {
    let mut cache = BuildCache::default();
    let small = empty_records();
    let small_bytes = small.retained_bytes;
    cache.insert_with_limits(key(1), Arc::clone(&small), 3, small_bytes.saturating_mul(2));
    cache.insert_with_limits(key(2), Arc::clone(&small), 3, small_bytes.saturating_mul(2));
    // The second insert pushes the first out: the two-record budget is full.
    assert!(cache.get(&key(1)).is_none(), "budget evicts the oldest");
    assert!(cache.get(&key(2)).is_some(), "the newest record survives");
    let mut bounded = BuildCache::default();
    bounded.insert_with_limits(key(3), small, 3, 0);
    assert!(
        bounded.get(&key(3)).is_none(),
        "an over-budget record is not retained"
    );
}

#[test]
fn weak_identities_never_retain_an_unreferenced_world() {
    // The worker's second lookup path holds only Weak records: an evicted or
    // replaced world must be released when its last owner drops, and the
    // identity deque must stay bounded across many distinct worlds.
    let mut identities = std::collections::VecDeque::new();
    let first = empty_records();
    remember_build(&mut identities, key(1), &first);
    assert_eq!(identities.len(), 1);
    drop(first);

    let live = empty_records();
    remember_build(&mut identities, key(2), &live);
    assert!(
        identities.iter().all(|(_, weak)| weak.strong_count() != 0),
        "a dead weak identity is pruned on the next insert"
    );
    assert!(
        identities.iter().any(|(stored, _)| stored == &key(2)),
        "the live world stays addressable"
    );

    for number in 3..20_u8 {
        remember_build(&mut identities, key(number), &live);
    }
    assert!(
        identities.len() <= 8,
        "the identity deque never exceeds its bound"
    );
    assert!(
        identities.iter().all(|(_, weak)| weak.strong_count() != 0),
        "only the live world is referenced"
    );
    drop(live);
}

#[test]
fn package_hash_and_quality_are_the_prepared_identity() {
    let full = PackageKey {
        package_sha256: "a".repeat(64),
        quality: LightmapQuality::Full,
    };
    let same = PackageKey {
        package_sha256: "a".repeat(64),
        quality: LightmapQuality::Full,
    };
    let other_quality = PackageKey {
        package_sha256: "a".repeat(64),
        quality: LightmapQuality::Medium,
    };
    let other_package = PackageKey {
        package_sha256: "b".repeat(64),
        quality: LightmapQuality::Full,
    };
    assert_eq!(full, same, "identical bytes and quality share records");
    assert_ne!(full, other_quality, "the quality selects separate records");
    assert_ne!(
        full, other_package,
        "a different package is different content"
    );
}

#[test]
fn cancelled_preparation_keeps_displayed_weak_identity_without_retaining_records() {
    let mut identities = std::collections::VecDeque::new();
    let records = empty_records();
    remember_build(&mut identities, key(1), &records);
    assert_eq!(identities.len(), 1, "the weak identity is remembered");
    let durable = Arc::clone(&records);
    drop(records);
    assert!(
        identities.iter().all(|(_, weak)| weak.strong_count() > 0),
        "the displayed world keeps the identity alive"
    );
    drop(durable);
    identities.retain(|(_, weak)| weak.strong_count() != 0);
    assert!(identities.is_empty(), "a dropped world leaves no identity");
}

/// A repeated Low -> Medium -> High quality cycle re-uses the exact prepared
/// records of each (package, quality) pair and never grows the cache past its
/// bounds: the same transition costs one decode, every later cycle costs a hit.
#[test]
fn repeated_quality_cycles_reuse_records_and_stay_bounded() {
    use std::collections::VecDeque;
    let package = "c".repeat(64);
    let key_of = |quality: LightmapQuality| PackageKey {
        package_sha256: package.clone(),
        quality,
    };
    let mut cache = BuildCache::default();
    let mut identities: VecDeque<(PackageKey, std::sync::Weak<PreparedRecords>)> = VecDeque::new();
    let mut first: Vec<(LightmapQuality, Arc<PreparedRecords>)> = Vec::new();
    let cycle = [
        LightmapQuality::Off,
        LightmapQuality::Medium,
        LightmapQuality::Full,
        LightmapQuality::Medium,
        LightmapQuality::Off,
        LightmapQuality::Full,
        LightmapQuality::Off,
    ];
    for quality in cycle {
        let key = key_of(quality);
        let records = reusable_records(&key, &mut cache, &mut identities).unwrap_or_else(|| {
            let records = empty_records();
            cache.insert(key.clone(), Arc::clone(&records));
            records
        });
        remember_build(&mut identities, key, &records);
        match first.iter().find(|(seen, _)| *seen == quality) {
            Some((_, original)) => assert!(
                Arc::ptr_eq(original, &records),
                "{quality:?} must reuse the exact records"
            ),
            None => first.push((quality, Arc::clone(&records))),
        }
        assert!(
            cache.entries.len() <= 3,
            "the cache bound holds across cycles: {}",
            cache.entries.len()
        );
        assert!(
            identities.len() <= 8,
            "the weak identity deque stays bounded: {}",
            identities.len()
        );
    }
    assert_eq!(first.len(), 3, "one retained set per lightmap quality");
    // The retained total is stable for the rest of the run: every later cycle
    // re-uses the records instead of accumulating new ones.
    let total: usize = cache.entries.iter().map(|(_, _, bytes)| *bytes).sum();
    assert!(total > 0);
    let original = first
        .iter()
        .find(|(quality, _)| *quality == LightmapQuality::Medium)
        .expect("medium was retained")
        .1
        .clone();
    let again = reusable_records(
        &key_of(LightmapQuality::Medium),
        &mut cache,
        &mut identities,
    )
    .expect("the medium records are retained");
    assert!(
        Arc::ptr_eq(&again, &original),
        "the cycle re-uses the exact retained records"
    );
    let after: usize = cache.entries.iter().map(|(_, _, bytes)| *bytes).sum();
    assert_eq!(after, total, "a hit does not grow the retained total");
}
