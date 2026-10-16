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
    let level = crate::engine::level::LevelDef::from_json(
        r#"{"format_version":2,"id":"cache_test","name":"Cache Test","spawn":{"x":0,"z":0}}"#,
    )
    .expect("tiny level");
    let catalog = crate::engine::loader::PropCatalog::builtin();
    let materials =
        crate::engine::materials::MaterialTable::logical(&level, catalog.assets(), None);
    let mut assets = PropAssets::with_root("unused-cache-fixture-root");
    let build = crate::engine::render::build_level_geometry_timed_with_lightmaps(
        &level,
        &catalog,
        &mut assets,
        &materials,
        crate::engine::render::LightmapBuildOptions::for_lightmaps(LightmapQuality::Off),
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
    let navigation = crate::engine::package::navigation::NavGrid::default();
    Arc::new(PreparedRecords {
        build: Arc::new(build),
        collision: compiled,
        navigation,
        probes: ProbeCaptures::default(),
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
    let small_bytes = small.retained_bytes();
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

    // A one-texel diagnostic expansion changes an already inserted record's
    // resident bytes. The next lookup must charge those bytes and evict it.
    let mut diagnostic = empty_records();
    let half_planes = Arc::new(
        [0_u16, 0, 0, 0x3800, 0, 0, 0, 0x3800]
            .into_iter()
            .flat_map(u16::to_le_bytes)
            .collect(),
    );
    let page = crate::engine::lighting::lightmap::LightmapPage::from_packed_planes(
        1,
        1,
        half_planes,
        0..16,
    )
    .expect("tiny packed diagnostic page");
    Arc::get_mut(&mut Arc::get_mut(&mut diagnostic).expect("unique record").build)
        .expect("unique build")
        .lightmaps = Some(Arc::new(
        crate::engine::lighting::lightmap::LevelLightmaps {
            pages: vec![page],
            charts: Vec::new(),
            stats: crate::engine::lighting::lightmap::LightmapStats::default(),
            cache_key: "diagnostic".to_owned(),
            padding: 0,
            switchable: Vec::new(),
        },
    ));
    let diagnostic_key = key(4);
    let packed_charge = diagnostic
        .retained_bytes()
        .saturating_add(diagnostic_key.package_sha256.capacity());
    let mut diagnostic_cache = BuildCache::default();
    diagnostic_cache.insert_with_limits(
        diagnostic_key.clone(),
        Arc::clone(&diagnostic),
        3,
        packed_charge,
    );
    let diagnostic_page = diagnostic
        .build
        .lightmaps
        .as_ref()
        .expect("packed atlas")
        .pages
        .first()
        .expect("packed page");
    assert_eq!(
        diagnostic_page.texels.iter().count(),
        1,
        "a diagnostic slice materializes one texel"
    );
    assert!(
        diagnostic_cache.get(&diagnostic_key).is_none(),
        "diagnostic growth is charged at the next cache operation"
    );
    assert!(
        diagnostic_page.texel(0, 0).is_some(),
        "active ownership survives diagnostic eviction"
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

fn prepared_lease_fixture() -> PreparedWorld {
    let level = crate::engine::level::LevelDef::from_json(
        r#"{"format_version":2,"id":"cache_test","name":"Cache Test","spawn":{"x":0,"z":0}}"#,
    )
    .expect("tiny lease level");
    let catalog = Arc::new(crate::engine::loader::PropCatalog::builtin());
    let materials =
        crate::engine::materials::MaterialTable::logical(&level, catalog.assets(), None);
    let collision = CollisionWorld::from_level(&level);
    let entry = LevelEntry {
        id: level.id.clone(),
        name: level.name.clone(),
        author: String::new(),
        source_type: LevelSourceType::Installed,
        path: std::path::PathBuf::from("unused-lease-fixture.placesmap"),
    };
    let records = empty_records();
    PreparedWorld {
        preparation_millis: 0.0,
        cache_hit: false,
        loaded: LoadedLevel {
            catalog,
            level,
            materials,
            light_sheets: Vec::new(),
            entry,
        },
        build: Arc::clone(&records.build),
        collision,
        characters: CharacterScene::default(),
        assets: PropAssets::with_root("unused-lease-fixture-assets"),
        lightmaps: LightmapQuality::Off,
        probes: ProbeCaptures::default(),
        lease: PreparedLease { records },
    }
}

/// An empty prepared world exceeds a zero-byte LRU without a large allocation.
/// Its real preparation-to-pending handoff must keep records reusable, then
/// release them when the displayed lease ends even if a build Arc remains.
#[test]
fn prepared_handoff_lease_reuses_oversized_displayed_records() {
    let prepared = prepared_lease_fixture();
    let identity = key(1);
    let mut identities = std::collections::VecDeque::new();
    remember_build(&mut identities, identity.clone(), &prepared.lease.records);
    let mut cache = BuildCache::default();
    cache.insert_with_limits(identity.clone(), Arc::clone(&prepared.lease.records), 3, 0);
    assert!(
        cache.entries.is_empty(),
        "the record is too large for this LRU"
    );

    let PreparedWorld {
        loaded,
        build,
        collision,
        lease,
        preparation_millis: _,
        cache_hit: _,
        characters: _,
        assets: _,
        lightmaps: _,
        probes: _,
    } = prepared;
    let pending = crate::game::app::PendingCommit {
        id: 1,
        loaded,
        collision,
        intent: crate::game::app::LoadIntent::Graphics,
        lease,
    };
    let reused_pending = reusable_records(&identity, &mut cache, &mut identities)
        .expect("pending GPU upload keeps the records reusable");
    assert!(
        Arc::ptr_eq(&build, &reused_pending.build),
        "pending reuse keeps the exact prepared build"
    );
    drop(reused_pending);
    let crate::game::app::PendingCommit {
        lease: displayed,
        id: _,
        loaded: _,
        collision: _,
        intent: _,
    } = pending;
    let reused_displayed = reusable_records(&identity, &mut cache, &mut identities)
        .expect("displayed lease keeps oversized records reusable");
    assert!(
        Arc::ptr_eq(&build, &reused_displayed.build),
        "displayed reuse keeps the exact prepared build"
    );
    drop(reused_displayed);
    drop(displayed);
    assert!(
        reusable_records(&identity, &mut cache, &mut identities).is_none(),
        "retaining only the build cannot keep unused record metadata alive"
    );
    assert!(
        Arc::strong_count(&build) >= 1,
        "the retained build itself remains valid"
    );
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
