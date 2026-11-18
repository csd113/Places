//! Verifies the compiler's two Static receiver states on one small room.
//! The fixture needs no source file, asset decode, atlas solve or GPU work.

use crate::engine::level::{LevelDef, StaticSwitchMode};
use crate::engine::lighting::lightmap::LightmapCache;
use crate::engine::loader::PropCatalog;
use crate::engine::materials::MaterialTable;
use crate::engine::props::PropAssets;
use crate::engine::quality::LightmapQuality;
use crate::engine::render::{
    LevelMesh, LightmapBuildOptions, SurfaceKind, prepare_compiled_level_geometry,
};

#[test]
fn prepared_static_circuit_changes_receivers_without_an_atlas() -> Result<(), String> {
    let mut level = LevelDef::from_json(
        r#"{"schema":"places-level","format_version":1,"id":"circuit","name":"Circuit",
        "spawn":{"x":1,"z":1},"rooms":[{"x":0,"z":0,"width":4,"depth":4,"height":3}],
        "ceiling_lights":[{"x":2,"y":2.8,"z":2,"fixture":"core:fluorescent_panel_01",
        "id":"panel","brightness":1.2,"switchable":true,"switch_group":"one",
        "static_switch":"prepared"}]}"#,
    )
    .map_err(|error| error.to_string())?;
    let catalog = PropCatalog::builtin();
    let materials = MaterialTable::logical(&level, catalog.assets(), None);
    let options = LightmapBuildOptions::for_lightmaps(LightmapQuality::Off);
    let mut cache = LightmapCache::memory_only();
    let mut assets = PropAssets::default();
    let prepared = prepare_compiled_level_geometry(
        &level,
        &catalog,
        &mut assets,
        &materials,
        options,
        &mut cache,
    )?;
    let alternate = prepared
        .build
        .static_light_off
        .as_ref()
        .ok_or("Missing alternate")?;
    assert_eq!(
        alternate.fixtures,
        [0],
        "Preserve the authored fixture ordinal"
    );
    assert!(
        prepared.fill.is_none() && prepared.build.lightmaps.is_none(),
        "Static must prepare both states without allocating an atlas"
    );

    let floor_peak = |mesh: &LevelMesh| {
        mesh.ranges
            .iter()
            .filter(|range| range.key.kind == SurfaceKind::Floor)
            .flat_map(|range| &range.vertices)
            .map(|vertex| vertex.color[0])
            .fold(0.0_f32, f32::max)
    };
    assert!(
        floor_peak(&prepared.build.mesh) > floor_peak(&alternate.mesh) + 0.01,
        "Circuit-off must update the actual receiver mesh, not just the fixture face"
    );

    let fixture = level
        .ceiling_lights
        .first_mut()
        .ok_or("Missing authored fixture")?;
    fixture.static_switch = StaticSwitchMode::Disabled;
    let ordinary = prepare_compiled_level_geometry(
        &level,
        &catalog,
        &mut assets,
        &materials,
        options,
        &mut cache,
    )?;
    assert!(
        ordinary.build.static_light_off.is_none(),
        "Ordinary Static has only one receiver state"
    );
    assert_eq!(
        prepared.build.mesh.ranges, ordinary.build.mesh.ranges,
        "Opting into a circuit must preserve the enabled-state receivers"
    );
    Ok(())
}

// A deliberately invalid page budget must produce a named error instead of
// drawable Static geometry. This small fixture performs no fill or GPU work.
#[test]
fn failed_full_plan_returns_its_error_before_building_a_candidate() -> Result<(), String> {
    let level = LevelDef::from_json(
        r#"{"schema":"places-level","format_version":1,"id":"budget","name":"Budget","spawn":{"x":1,"z":1}}"#,
    )
    .map_err(|error| error.to_string())?;
    let catalog = PropCatalog::builtin();
    let materials = MaterialTable::logical(&level, catalog.assets(), None);
    let mut options = LightmapBuildOptions::for_lightmaps(LightmapQuality::Full);
    options.config.max_pages = 0;
    let mut cache = LightmapCache::memory_only();
    let mut assets = PropAssets::default();
    let error = prepare_compiled_level_geometry(
        &level,
        &catalog,
        &mut assets,
        &materials,
        options,
        &mut cache,
    )
    .err()
    .ok_or("Invalid Full preparation produced a candidate")?;
    assert!(error.contains("invalid config"), "{error}");
    Ok(())
}
