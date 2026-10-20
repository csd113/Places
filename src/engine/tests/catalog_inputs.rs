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
