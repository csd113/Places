//! Opt-in CPU operation probe for `editor-profile --responsiveness PROJECT`.
//! No window, GPU, map bake, save, or automatic test/CI invocation.

use std::{hint::black_box, path::Path, time::Instant};

use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};

use super::{
    browser::{Browser, ChoiceKind, PAGE_SIZE},
    document::{Document, ObjectId},
};
use crate::engine::assets::{AssetCatalog, AssetType};

fn verify_choices(catalog: &AssetCatalog) -> Result<usize, String> {
    let mut browser = Browser::new(catalog);
    let mut themes = vec!["All themes".to_owned()];
    let unique = catalog
        .entries()
        .iter()
        .map(|entry| {
            entry
                .theme
                .as_ref()
                .map_or("Shared", |theme| theme.as_str())
                .to_owned()
        })
        .collect::<std::collections::BTreeSet<_>>();
    themes.extend(unique);
    let mut cases = 0_usize;
    for (theme, label) in themes.iter().enumerate() {
        browser.theme = theme;
        for (kind, types) in [
            (
                ChoiceKind::Models,
                &[AssetType::PROP, AssetType::ENTITY][..],
            ),
            (ChoiceKind::Materials, &[AssetType::MATERIAL][..]),
            (ChoiceKind::Fixtures, &[AssetType::LIGHT][..]),
            (ChoiceKind::Entities, &[AssetType::ENTITY][..]),
        ] {
            for query in ["", "ChAiR", "_01", "zzzznomatch"] {
                browser.query = query.into();
                let expected: Vec<_> = catalog
                    .entries()
                    .into_iter()
                    .filter(|entry| types.contains(&entry.asset_type.as_str()))
                    .filter(|entry| {
                        theme == 0
                            || label
                                == entry
                                    .theme
                                    .as_ref()
                                    .map_or("Shared", |value| value.as_str())
                    })
                    .filter(|entry| {
                        format!("{} {}", entry.id, entry.display_name)
                            .to_lowercase()
                            .contains(&query.to_lowercase())
                    })
                    .map(|entry| entry.id.clone())
                    .collect();
                if *browser.choices(kind) != expected {
                    return Err(format!(
                        "Picker parity failed for theme {label}, query {query}"
                    ));
                }
                cases = cases.saturating_add(1);
            }
        }
    }
    Ok(cases)
}

fn measure(mut work: impl FnMut(), count: u16) -> f64 {
    let started = Instant::now();
    for _ in 0..count {
        work();
    }
    started.elapsed().as_secs_f64() * 1_000.0_f64 / f64::from(count)
}

/// Measures real catalog, thumbnail, safety, preview and document operations.
/// # Errors
/// Returns source/asset admission or preview preparation failures.
pub fn responsiveness(path: &Path) -> Result<Value, String> {
    let mut document = Document::open(path)?;
    let catalog = AssetCatalog::load_default();
    let index_started = Instant::now();
    let mut browser = Browser::new(&catalog);
    let index_ms = index_started.elapsed().as_secs_f64() * 1_000.0_f64;
    let mut choices = Vec::new();
    let mut identity = Sha256::new();
    for (kind, name) in [
        (ChoiceKind::Models, "models"),
        (ChoiceKind::Materials, "materials"),
        (ChoiceKind::Fixtures, "fixtures"),
        (ChoiceKind::Entities, "entities"),
    ] {
        for query in ["", "chair", "zzzznomatch"] {
            browser.query = query.into();
            let cold_ms = measure(
                || {
                    drop(black_box(browser.choices(kind)));
                },
                1,
            );
            let warm_ms = measure(
                || {
                    drop(black_box(browser.choices(kind)));
                },
                2_000,
            );
            let ids = browser.choices(kind);
            identity.update(serde_json::to_vec(&*ids).map_err(|error| error.to_string())?);
            choices.push(json!({"kind":name,"query":query,"count":ids.len(),"cold_ms":cold_ms,"warm_ms":warm_ms}));
        }
    }
    browser.query.clear();
    let ids = browser.choices(ChoiceKind::Models);
    let visible: Vec<_> = ids.iter().take(PAGE_SIZE).cloned().collect();
    let tiles_cold_ms = measure(|| browser.prepare_one(&visible), 8);
    let tiles_warm_ms = measure(|| browser.prepare_one(&visible), 2_000);
    let tiles = browser.cached_tiles();
    let scans_ms = measure(
        || {
            drop(black_box(super::safety::analyze(
                document.level(),
                document.raw(),
            )));
        },
        3,
    );
    let previews = super::viewport::preview::profile(document.level())?;
    let raw = document.raw().clone();
    let pose = document.transform(ObjectId::Prop(0))?;
    let started = Instant::now();
    for _ in 0_u16..10 {
        let mut next = pose;
        next.yaw_degrees += 5.0_f32;
        document.set_transform(ObjectId::Prop(0), next)?;
        if !document.undo() || !document.redo() || !document.undo() || document.raw() != &raw {
            return Err("Transform/history did not restore the exact source".into());
        }
    }
    let edit_history_ms = started.elapsed().as_secs_f64() * 100.0_f64;
    let parity_cases = verify_choices(&catalog)?;
    Ok(
        json!({"kind":"editor_cpu_operations_ms","project":path,"objects":document.objects().len(),"catalog_entries":catalog.len(),"index_ms":index_ms,"choices":choices,"choice_sha256":crate::engine::package::hash::digest_hex(&identity.finalize()),"choice_parity_cases":parity_cases,"thumbnail_one_cold_ms":tiles_cold_ms,"thumbnail_warm_ms":tiles_warm_ms,"thumbnail_tiles":tiles,"safety_scan_ms":scans_ms,"previews":previews,"transform_undo_redo_undo_ms":edit_history_ms,"history_exact":document.raw()==&raw}),
    )
}

/// Explicit preview source/edit parity and OS worker-CPU audit.
/// # Errors
/// Reports source admission or any byte-level prepared-output mismatch.
pub fn preview_audit(path: &Path, level: &str) -> Result<Value, String> {
    let mut document = Document::open(path)?;
    document.select_level(level)?;
    super::viewport::preview::audit::run(document.level())
}
