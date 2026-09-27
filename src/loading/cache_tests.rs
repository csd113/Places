//! Small immutable prepared-cache contracts; no shipped models or level bakes.
#![allow(clippy::expect_used)] // Fixture construction reports exact failures.
use super::*;
use std::sync::atomic::AtomicU64;

struct Scratch(std::path::PathBuf);
impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "places-build-cache-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).expect("isolated cache fixture directory");
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn loaded_fixture(with_prop: bool) -> LoadedLevel {
    let catalog = Arc::new(
        crate::loader::PropCatalog::from_json_str(
            r#"{
        "format_version":2,"themes":[],"assets":[
          {"id":"core:used","asset_class":"environment","asset_type":"prop","source":"file",
           "model":"used.glb","size":[1,1,1],"solid":true}
        ]}"#,
        )
        .expect("tiny catalog"),
    );
    let props = if with_prop {
        r#", "props":[{"model":"core:used","x":0,"z":0}]"#
    } else {
        ""
    };
    let level = crate::level::LevelDef::from_json(&format!(
        r#"{{"format_version":2,"id":"cache_test","name":"Cache Test","spawn":{{"x":0,"z":0}}{props}}}"#
    )).expect("tiny level");
    LoadedLevel {
        materials: crate::materials::MaterialTable::logical(&level, catalog.assets(), None),
        entry: LevelEntry {
            id: level.id.clone(),
            name: level.name.clone(),
            author: String::new(),
            source_type: crate::loader::LevelSourceType::CustomJson,
            path: "fixture.json".into(),
        },
        level,
        catalog,
        light_sheets: Vec::new(),
    }
}
fn key(number: u8) -> BuildKey {
    BuildKey {
        definition: vec![number],
        materials: String::new(),
        catalog: String::new(),
        root: None,
        models: Vec::new(),
        quality: LightmapQuality::Off,
    }
}
fn empty_build() -> Arc<LevelBuild> {
    let loaded = loaded_fixture(false);
    let mut assets = PropAssets::with_root("unused-cache-fixture-root");
    Arc::new(crate::render::build_level_geometry_timed_with_lightmaps(
        &loaded.level,
        loaded.catalog.as_ref(),
        &mut assets,
        &loaded.materials,
        LightmapBuildOptions::for_lightmaps(LightmapQuality::Off),
        None,
    ))
}

#[test]
fn cancelled_preparation_keeps_displayed_weak_identity_without_retaining_geometry() {
    let mut identities = std::collections::VecDeque::new();
    let displayed = empty_build();
    let cancelled = empty_build();
    remember_build(&mut identities, key(1), &displayed);
    remember_build(&mut identities, key(2), &cancelled);
    assert_eq!(Arc::strong_count(&displayed), 1);
    drop(cancelled);
    let next = empty_build();
    remember_build(&mut identities, key(3), &next);
    assert_eq!(identities.len(), 2);
    assert!(identities.iter().any(|(identity, build)| {
        identity == &key(1)
            && build
                .upgrade()
                .is_some_and(|build| Arc::ptr_eq(&build, &displayed))
    }));
    drop(displayed);
    drop(next);
    remember_build(&mut identities, key(4), &empty_build());
    assert_eq!(identities.len(), 1);
    assert!(
        identities
            .front()
            .is_some_and(|(_, build)| build.upgrade().is_none())
    );
}

#[test]
fn cache_hits_reuse_the_exact_build_and_lru_eviction_keeps_displayed_arcs_alive() {
    let first = empty_build();
    let second = empty_build();
    let third = empty_build();
    let fourth = empty_build();
    let mut cache = BuildCache::default();
    cache.insert(key(1), Arc::clone(&first));
    cache.insert(key(2), Arc::clone(&second));
    cache.insert(key(3), Arc::clone(&third));
    let displayed = cache.get(&key(1)).expect("first hit");
    assert!(Arc::ptr_eq(&displayed, &first));
    cache.insert(key(4), fourth);
    assert!(
        cache.get(&key(2)).is_none(),
        "least recently used entry evicted"
    );
    assert!(Arc::ptr_eq(
        &cache.get(&key(1)).expect("touched entry retained"),
        &displayed
    ));
    assert!(Arc::ptr_eq(
        &cache.get(&key(3)).expect("third retained"),
        &third
    ));
    assert_eq!(
        Arc::strong_count(&second),
        1,
        "eviction releases only cache ownership"
    );
    drop(cache);
    assert!(
        Arc::ptr_eq(&displayed, &first),
        "displayed build survives all cache destruction"
    );
}

#[test]
fn byte_budget_evicts_old_builds_and_oversized_entries_are_not_retained() {
    let first = empty_build();
    let second = empty_build();
    // Keys here carry exactly one byte and no other allocated dependency data.
    let cost = first
        .retained_bytes()
        .saturating_add(key(1).definition.capacity());
    let mut cache = BuildCache::default();
    cache.insert_with_limits(key(1), Arc::clone(&first), 3, cost);
    cache.insert_with_limits(key(2), Arc::clone(&second), 3, cost);
    assert!(cache.get(&key(1)).is_none());
    assert!(Arc::ptr_eq(
        &cache.get(&key(2)).expect("newest fits"),
        &second
    ));
    cache.insert_with_limits(key(3), Arc::clone(&first), 3, cost.saturating_sub(1));
    assert!(
        cache.get(&key(3)).is_none(),
        "oversized item is usable but uncached"
    );
    assert!(
        cache.get(&key(2)).is_some(),
        "rejected item does not evict the valid cache"
    );
    cache.insert_with_limits(key(4), first, 0, cost);
    assert!(cache.get(&key(4)).is_none());
}

#[test]
fn definition_lightmap_mode_and_root_change_the_prepared_identity() {
    let loaded = loaded_fixture(false);
    let mut assets = PropAssets::with_root("root-one");
    let base = BuildKey::for_request(
        &loaded,
        loaded.catalog.as_ref(),
        &mut assets,
        LightmapQuality::Off,
    )
    .expect("base key");
    let same = BuildKey::for_request(
        &loaded,
        loaded.catalog.as_ref(),
        &mut assets,
        LightmapQuality::Off,
    )
    .expect("same key");
    assert!(base == same);
    let full = BuildKey::for_request(
        &loaded,
        loaded.catalog.as_ref(),
        &mut assets,
        LightmapQuality::Full,
    )
    .expect("full key");
    assert!(base != full);
    let mut changed = loaded.clone();
    changed.level.spawn.x = 1.0;
    let edited = BuildKey::for_request(
        &changed,
        changed.catalog.as_ref(),
        &mut assets,
        LightmapQuality::Off,
    )
    .expect("edited key");
    assert!(base != edited);
    let moved = BuildKey::for_request(
        &loaded,
        loaded.catalog.as_ref(),
        &mut PropAssets::with_root("root-two"),
        LightmapQuality::Off,
    )
    .expect("different root key");
    assert!(base != moved);
}

fn assert_shared_model_inputs(original: &BuildKey, unrelated_edit: &BuildKey) {
    let original_bytes = original
        .models
        .first()
        .expect("used input")
        .1
        .as_ref()
        .expect("bytes");
    let unchanged_bytes = unrelated_edit
        .models
        .first()
        .expect("used input")
        .1
        .as_ref()
        .expect("bytes");
    assert!(
        Arc::ptr_eq(original_bytes, unchanged_bytes),
        "unchanged input keys share the retained snapshot instead of the temporary fresh read"
    );
}

#[test]
fn only_used_model_bytes_invalidate_and_missing_restored_inputs_change_identity() {
    let scratch = Scratch::new();
    let used = scratch.0.join("used.glb");
    let unrelated = scratch.0.join("unrelated.glb");
    std::fs::write(&used, b"original").expect("used input");
    std::fs::write(&unrelated, b"unrelated").expect("unrelated input");
    let timestamp = std::fs::metadata(&used)
        .expect("metadata")
        .modified()
        .expect("mtime");
    let loaded = loaded_fixture(true);
    let mut assets = PropAssets::with_root(&scratch.0);
    let original = BuildKey::for_request(
        &loaded,
        loaded.catalog.as_ref(),
        &mut assets,
        LightmapQuality::Off,
    )
    .expect("original key");
    std::fs::write(&unrelated, b"different").expect("unrelated edit");
    let unrelated_edit = BuildKey::for_request(
        &loaded,
        loaded.catalog.as_ref(),
        &mut assets,
        LightmapQuality::Off,
    )
    .expect("unrelated key");
    assert!(original == unrelated_edit);
    assert_shared_model_inputs(&original, &unrelated_edit);
    let mut cache = BuildCache::default();
    let retained = empty_build();
    let insertion_key = BuildKey::for_request(
        &loaded,
        loaded.catalog.as_ref(),
        &mut assets,
        LightmapQuality::Off,
    )
    .expect("insertion key");
    cache.insert(insertion_key, Arc::clone(&retained));
    assert!(Arc::ptr_eq(
        &cache
            .get(&unrelated_edit)
            .expect("unrelated edit still hits"),
        &retained
    ));
    std::fs::write(&used, b"modified").expect("same-length content edit");
    std::fs::File::options()
        .write(true)
        .open(&used)
        .expect("file")
        .set_times(std::fs::FileTimes::new().set_modified(timestamp))
        .expect("restore mtime");
    assert_eq!(
        std::fs::metadata(&used)
            .expect("metadata")
            .modified()
            .expect("mtime"),
        timestamp
    );
    let edited = BuildKey::for_request(
        &loaded,
        loaded.catalog.as_ref(),
        &mut assets,
        LightmapQuality::Off,
    )
    .expect("changed key");
    assert!(
        original != edited,
        "mtime and length cannot validate stale bytes"
    );
    assert!(
        cache.get(&edited).is_none(),
        "changed used bytes miss the prepared cache"
    );
    std::fs::remove_file(&used).expect("remove used input");
    let missing = BuildKey::for_request(
        &loaded,
        loaded.catalog.as_ref(),
        &mut assets,
        LightmapQuality::Off,
    )
    .expect("missing key");
    assert!(edited != missing);
    std::fs::write(&used, b"original").expect("restore used input");
    let restored = BuildKey::for_request(
        &loaded,
        loaded.catalog.as_ref(),
        &mut assets,
        LightmapQuality::Off,
    )
    .expect("restored key");
    assert!(missing != restored);
    assert!(
        original == restored,
        "identical dependencies regain original immutable identity"
    );
    assert!(Arc::ptr_eq(
        &cache.get(&restored).expect("exact restored bytes hit"),
        &retained
    ));
}
