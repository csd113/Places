//! Tiny assetless cache regression: presentation reuse, conservative real-edit
//! invalidation and cancellation without replacing the last complete owner.
//! One two-metre room/wall, under 1 MiB and milliseconds; no GPU/full-map solve.
use super::*;

#[test]
fn preview_reuse_obeys_source_material_input_and_cancel_dependencies() -> Result<(), String> {
    let mut level = LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"tiny","name":"tiny","spawn":{"x":0,"z":0},"rooms":[{"width":2,"depth":2,"height":3}],"walls":[{"x":0,"z":0,"width":2,"depth":0.2}]}"#).map_err(|error|error.to_string())?;
    let catalog = PropCatalog::builtin();
    let mut assets = PropAssets::with_root(std::env::temp_dir().join("places-no-preview-assets"));
    let materials =
        crate::engine::materials::MaterialTable::logical(&level, catalog.assets(), None);
    let mut reuse = reuse::Reuse::default();
    let (initial, initial_reuse, _) =
        reuse.build(&level, &catalog, &mut assets, &materials, false, "inputs")?;
    assert!(!initial_reuse, "first preparation must compute its data");
    level.name = "renamed".into();
    let (renamed, metadata_reuse, _) =
        reuse.build(&level, &catalog, &mut assets, &materials, false, "inputs")?;
    assert!(
        metadata_reuse && Arc::ptr_eq(&initial, &renamed),
        "metadata must share immutable prepared geometry"
    );
    level.walls.first_mut().ok_or("missing wall")?.x = 0.2_f32;
    let (moved, wall_reuse, props) =
        reuse.build(&level, &catalog, &mut assets, &materials, false, "inputs")?;
    assert!(
        !wall_reuse && props && !Arc::ptr_eq(&initial, &moved),
        "a wall edit must rebuild architecture while retaining unaffected Inspect props"
    );
    let (_, environment_reuse, _) =
        reuse.build(&level, &catalog, &mut assets, &materials, true, "inputs")?;
    assert!(
        !environment_reuse,
        "Environment must not reuse studio-lit output"
    );
    let (_, input_reuse, _) = reuse.build(
        &level,
        &catalog,
        &mut assets,
        &materials,
        true,
        "changed bytes",
    )?;
    assert!(
        !input_reuse,
        "model/input changes must invalidate prepared output"
    );
    let cancelled = Arc::new(std::sync::atomic::AtomicBool::new(true));
    {
        let _scope = crate::engine::resources::enter(Some(
            crate::engine::resources::DecodeBudget::new(cancelled),
        ));
        assert!(
            reuse
                .build(
                    &level,
                    &catalog,
                    &mut assets,
                    &materials,
                    true,
                    "unpublished"
                )
                .is_err(),
            "cancelled preparation must not commit cache data"
        );
    }
    let (_, retained_reuse, _) = reuse.build(
        &level,
        &catalog,
        &mut assets,
        &materials,
        true,
        "changed bytes",
    )?;
    assert!(
        retained_reuse,
        "cancellation must retain the prior complete cache owner"
    );
    display_owner_is_collectible(&mut reuse, &level, &catalog, &mut assets, &materials)?;
    Ok(())
}

fn display_owner_is_collectible(
    reuse: &mut reuse::Reuse,
    level: &LevelDef,
    catalog: &PropCatalog,
    assets: &mut PropAssets,
    materials: &crate::engine::materials::MaterialTable,
) -> Result<(), String> {
    // Exercise the oversized-scene ownership policy without allocating a large map.
    let displayed = reuse.display_owner_only().ok_or("missing optional owner")?;
    let (shared, live_reuse, _) =
        reuse.build(level, catalog, assets, materials, true, "changed bytes")?;
    assert!(
        live_reuse && Arc::ptr_eq(&displayed, &shared),
        "an oversized live display must still reuse its exact prepared build"
    );
    drop(shared);
    drop(displayed);
    let (_, expired_reuse, _) =
        reuse.build(level, catalog, assets, materials, true, "changed bytes")?;
    assert!(
        !expired_reuse,
        "a cancelled undisplayed large result must expire and recompute"
    );
    Ok(())
}

/// Protects material-only reuse and global-light invalidation while proving
/// architecture COW cannot mutate the previously displayed scene. One tiny
/// untextured room; under 1 MiB, milliseconds, no shipped map/GPU/transport.
#[test]
fn environment_reuse_requires_exact_lighting_and_architecture_dependencies() -> Result<(), String> {
    let mut level = LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"environment-cache","name":"tiny","spawn":{"x":0,"z":0},"rooms":[{"width":2,"depth":2,"height":3}],"props":[{"model":"missing","x":1,"z":1,"occludes":false,"shadow_only":true}]}"#).map_err(|error|error.to_string())?;
    let catalog = PropCatalog::builtin();
    let mut assets = PropAssets::with_root(std::env::temp_dir().join("places-no-preview-assets"));
    let mut reuse = reuse::Reuse::default();
    let materials =
        crate::engine::materials::MaterialTable::logical(&level, catalog.assets(), None);
    let (initial, _, _) = reuse.build(&level, &catalog, &mut assets, &materials, true, "same")?;
    let original = crate::engine::package::mesh::write_mesh(&initial.mesh)?;
    // Keep the exact owner count and X/Z grid while moving hidden geometry.
    // Becoming visible must then emit a fresh fallback in the architecture.
    level.props.first_mut().ok_or("missing prop")?.y = 0.25_f32;
    let (hidden, _, _) = reuse.build(&level, &catalog, &mut assets, &materials, true, "same")?;
    assert!(
        Arc::ptr_eq(&initial.mesh, &hidden.mesh),
        "unchanged architecture must share its allocation"
    );
    let added = level.props.first().ok_or("missing prop")?.clone();
    level.props.push(added);
    let (appended, _, _) = reuse.build(&level, &catalog, &mut assets, &materials, true, "same")?;
    assert!(
        Arc::ptr_eq(&hidden.mesh, &appended.mesh),
        "trailing empty ownership must not invalidate equivalent sampling"
    );
    level.props.first_mut().ok_or("missing prop")?.shadow_only = false;
    let (fallback, _, _) = reuse.build(&level, &catalog, &mut assets, &materials, true, "same")?;
    assert!(
        !Arc::ptr_eq(&initial.mesh, &fallback.mesh),
        "new fallback geometry must invalidate architecture"
    );
    assert_eq!(
        reuse
            .metrics
            .get("equivalent_lighting")
            .and_then(serde_json::Value::as_bool),
        Some(true),
        "a nonoccluding fallback does not change lighting"
    );
    assert_eq!(
        crate::engine::package::mesh::write_mesh(&initial.mesh)?,
        original,
        "the displayed scene is immutable"
    );
    level.props.clear();
    level.global_illuminators.push(serde_json::from_value(serde_json::json!({"id":"sun","kind":"sun","direction":[0_i32,-1_i32,0_i32],"color":[1_i32,1_i32,1_i32],"intensity":0.5_f32})).map_err(|error|error.to_string())?);
    let (changed, _, _) = reuse.build(&level, &catalog, &mut assets, &materials, true, "same")?;
    assert_eq!(
        reuse
            .metrics
            .get("equivalent_lighting")
            .and_then(serde_json::Value::as_bool),
        Some(false),
        "a global source must reject all previous lit output"
    );
    assert!(!Arc::ptr_eq(&fallback.mesh, &changed.mesh));
    let weak = Arc::downgrade(&changed);
    drop(changed);
    level.id = "different-world".into();
    let _next = reuse.build(&level, &catalog, &mut assets, &materials, true, "same")?;
    assert!(
        weak.upgrade().is_none(),
        "inactive-world cache owners must be released"
    );
    Ok(())
}
