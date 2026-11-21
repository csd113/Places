//! Essential invalidation regression: unused changes must not rebake a map,
//! while referenced definitions, recursive images and missing-to-present
//! lookups must invalidate it. Tiny in-memory JSON, no baking or filesystem.
use super::CatalogInputs;
use crate::engine::level::LevelDef;

#[test]
fn catalog_closure_tracks_only_referenced_transitive_and_missing_entries() -> Result<(), String> {
    let level = LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"closure","name":"Closure","spawn":{"x":0,"z":0},"defaults":{"wall":"core:wall","floor":"core:trim"}}"#).map_err(|e|e.to_string())?;
    let mut catalog = serde_json::json!({"format_version":2_u32,"assets":[{"id":"core:wall","asset_class":"environment","asset_type":"material","source":"definition","texture":"core:image"},{"id":"core:image","asset_class":"environment","asset_type":"texture","source":"file","model":"a.png"},{"id":"core:unused","asset_class":"environment","asset_type":"prop","source":"file","model":"unused.glb"}]});
    let key = |candidate: &serde_json::Value| {
        CatalogInputs::for_level(
            &level,
            &serde_json::to_vec(candidate).map_err(|e| e.to_string())?,
        )
    };
    let original = key(&catalog)?;
    *catalog
        .pointer_mut("/assets/2/model")
        .ok_or("missing unused")? = "another.glb".into();
    assert_eq!(
        original,
        key(&catalog)?,
        "unused catalog assets do not affect the closure"
    );
    *catalog
        .pointer_mut("/assets/1/model")
        .ok_or("missing image")? = "b.png".into();
    assert_ne!(
        original.identity()?,
        key(&catalog)?.identity()?,
        "recursive image mapping changes invalidate"
    );
    *catalog
        .pointer_mut("/assets/1/model")
        .ok_or("missing image")? = "a.png".into();
    catalog
        .get_mut("assets")
        .and_then(serde_json::Value::as_array_mut)
        .ok_or("missing array")?
        .push(serde_json::json!({"id":"core:trim","asset_class":"environment","asset_type":"material","source":"definition","texture":"core:image"}));
    assert_ne!(
        original,
        key(&catalog)?,
        "a formerly absent dependency becoming available invalidates"
    );
    Ok(())
}

// The parsed material changes tint while referencing the same PNG. This
// guards normalized lookups, legacy exact-catalog admission and duplicates
// with two catalog byte strings and no decode or bake.
#[test]
fn catalog_closure_follows_normalized_ids_and_checks_legacy_bytes() -> Result<(), String> {
    use crate::engine::package::hash::sha256_hex;

    let level = LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"closure","name":"Closure","spawn":{"x":0,"z":0},"defaults":{"wall":"core:wall"}}"#).map_err(|e|e.to_string())?;
    let mut catalog = serde_json::json!({"format_version":2_u32,"assets":[
        {"id":" core:wall ","asset_class":"environment","asset_type":"material","source":"definition","texture":" core:image ","normal_texture":" core:normal "},
        {"id":" core:image ","asset_class":"environment","asset_type":"texture","source":"file","model":"same.png"},
        {"id":"core:normal","asset_class":"environment","asset_type":"texture","source":"file","model":"normal.png"}
    ]});
    let original = catalog.to_string().into_bytes();
    let normalized = CatalogInputs::for_level(&level, &original)?;
    assert!(
        normalized
            .entries
            .get("core:wall")
            .is_some_and(Option::is_some)
    );
    assert!(
        normalized
            .entries
            .get("core:image")
            .is_some_and(Option::is_some)
    );
    assert!(
        normalized
            .entries
            .get("core:normal")
            .is_some_and(Option::is_some)
    );
    let legacy = CatalogInputs::collect(&level, &original, None)?;
    assert_ne!(normalized, legacy);
    assert!(legacy.matches_installed(&level, &original, &sha256_hex(&original))?);

    let wall = catalog.pointer_mut("/assets/0").ok_or("missing wall")?;
    let original_wall = wall.clone();
    let tint = serde_json::to_value([0.5_f32; 3]).map_err(|error| error.to_string())?;
    let definition = wall
        .as_object_mut()
        .ok_or("fixture wall entry is not an object")?;
    drop(definition.insert("tint".into(), tint));
    let changed = catalog.to_string().into_bytes();
    let fresh = CatalogInputs::for_level(&level, &changed)?;
    assert_ne!(normalized.identity()?, fresh.identity()?);
    assert_eq!(
        normalized.entries.get("core:image"),
        fresh.entries.get("core:image")
    );
    assert!(!legacy.matches_installed(&level, &changed, &sha256_hex(&original))?);
    assert!(!normalized.matches_installed(&level, &changed, &sha256_hex(&original))?);

    let mut duplicate = original_wall;
    *duplicate.get_mut("id").ok_or("missing id")? = "core:wall".into();
    catalog
        .get_mut("assets")
        .and_then(serde_json::Value::as_array_mut)
        .ok_or("missing assets")?
        .push(duplicate);
    assert!(
        CatalogInputs::for_level(&level, catalog.to_string().as_bytes())
            .is_err_and(|error| error.contains("duplicate asset id"))
    );
    Ok(())
}

// The new implicit root must make an omitted background distinguishable
// without stranding a legacy archive against its exact original catalog.
#[test]
fn legacy_implicit_root_omission_requires_the_original_catalog() -> Result<(), String> {
    use crate::engine::{assets::CELESTIAL_BACKGROUND_TEXTURE, package::hash::sha256_hex};

    let level = LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"closure","name":"Closure","spawn":{"x":0,"z":0},"global_illuminators":[{"id":"sun","kind":"sun","direction":[0,-1,0],"visual":{}}]}"#).map_err(|error| error.to_string())?;
    let original = serde_json::json!({"format_version":2_u32,"assets":[
        {"id":CELESTIAL_BACKGROUND_TEXTURE,"asset_class":"diagnostic","asset_type":"texture","source":"file","model":"background.png"}
    ]}).to_string();
    let legacy = CatalogInputs::collect(&level, original.as_bytes(), None)?;
    let original_hash = sha256_hex(original.as_bytes());
    assert!(!legacy.entries.contains_key(CELESTIAL_BACKGROUND_TEXTURE));
    assert!(legacy.matches_installed(&level, original.as_bytes(), &original_hash)?);
    let omitted = br#"{"format_version":2,"assets":[]}"#;
    assert_eq!(legacy, CatalogInputs::collect(&level, omitted, None)?);
    assert!(!legacy.matches_installed(&level, omitted, &original_hash)?);
    Ok(())
}
