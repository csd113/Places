//! One small catalog protects picker dependencies: type/theme/query transitions,
//! stable variant ordering and replacement-catalog admission. It adds no file,
//! image, renderer or bake work; the unique regression is stale cached choices.

use std::rc::Rc;

use crate::{
    editor::browser::{Browser, ChoiceKind},
    engine::assets::AssetCatalog,
};

fn catalog(name: &str) -> Result<AssetCatalog, String> {
    let assets = serde_json::json!({"format_version":2_u32,"assets":[
        {"id":"core:chair_red","display_name":name,"asset_class":"environment","theme":"home","asset_type":"prop","source":"file","model":"shared/red.glb"},
        {"id":"core:chair_blue","display_name":"Blue Chair","asset_class":"environment","theme":"pool","asset_type":"prop","source":"file","model":"shared/blue.glb"},
        {"id":"core:actor","display_name":"Actor","asset_class":"entity","asset_type":"entity","source":"file","model":"shared/actor.glb"},
        {"id":"core:lamp","display_name":"Lamp","asset_class":"environment","theme":"home","asset_type":"light","source":"file","model":"shared/lamp.png"}
    ]});
    AssetCatalog::from_json_str(&assets.to_string())
}

#[test]
fn picker_reuses_only_identical_dependencies_and_retains_variant_order() -> Result<(), String> {
    let assets = catalog("RÉD Chair")?;
    let mut browser = Browser::new(&assets);
    let models = browser.choices(ChoiceKind::Models);
    assert_eq!(
        &*models,
        ["core:actor", "core:chair_blue", "core:chair_red"],
        "variants retain id ordering"
    );
    assert!(
        Rc::ptr_eq(&models, &browser.choices(ChoiceKind::Models)),
        "unchanged lists share the allocation"
    );
    browser.query = "réd".into();
    assert_eq!(
        &*browser.choices(ChoiceKind::Models),
        ["core:chair_red"],
        "Unicode case folding retains the existing search semantics"
    );
    browser.query = "chair".into();
    browser.theme = 2;
    assert_eq!(
        &*browser.choices(ChoiceKind::Models),
        ["core:chair_red"],
        "home theme excludes other variants"
    );
    browser.query.clear();
    assert_eq!(
        &*browser.choices(ChoiceKind::Fixtures),
        ["core:lamp"],
        "picker type is a dependency"
    );
    browser.theme = 0;
    assert_eq!(
        &*browser.choices(ChoiceKind::Entities),
        ["core:actor"],
        "theme reset and entity picker update immediately"
    );
    browser.query = "missing".into();
    assert!(
        browser.choices(ChoiceKind::Models).is_empty(),
        "no-match never retains old results"
    );
    browser.query.clear();
    assert_eq!(
        &*browser.choices(ChoiceKind::Models),
        &*models,
        "clearing restores all variants"
    );
    let replacement = catalog("Replacement")?;
    browser = Browser::new(&replacement);
    browser.query = "réd".into();
    assert!(
        browser.choices(ChoiceKind::Models).is_empty(),
        "reloaded catalog rebuilds search metadata"
    );
    browser.query = "replacement".into();
    assert_eq!(
        &*browser.choices(ChoiceKind::Models),
        ["core:chair_red"],
        "replacement display names are searchable"
    );
    Ok(())
}
