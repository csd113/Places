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
