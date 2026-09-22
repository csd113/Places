//! The level-geometry entry points.
//!
//! These are the functions the game and the audits call: each one builds the
//! static mesh (and, where asked, the prop batches) from a level, a catalog and
//! a material table, with the lighting baked exactly once per level load.

use super::props::{PropMeshBatch, resolve_prop_instances};
use super::{
    LevelDef, LevelLighting, LevelMesh, LevelSurfaces, MaterialTable, PropDef,
    build_level_geometry_mesh,
};

/// Builds the level mesh with real prop geometry where possible, plus one
/// batched draw per distinct prop model.
///
/// Props whose model is missing, malformed or simply absent from the catalogue
/// still emit their catalogue-sized placeholder box into
/// `LevelMesh::batches.prop_batch`, so a broken asset degrades visibly instead
/// of vanishing, and never crashes or loops (failures are cached by
/// [`crate::props::PropAssets`]).
pub fn build_level_geometry_with_assets(
    level: &LevelDef,
    catalog: &crate::loader::PropCatalog,
    assets: &mut crate::props::PropAssets,
) -> (LevelMesh, Vec<PropMeshBatch>) {
    let materials = logical_materials(level);
    let (mesh, batches, _lighting) = build_level_geometry_with_assets_and_lighting_and_materials(
        level, catalog, assets, &materials,
    );
    (mesh, batches)
}

/// [`build_level_geometry_with_assets`], also returning the baked lighting that
/// was folded into the vertex colours.
///
/// The lighting is baked exactly once here, at level load, and passed to both
/// the world geometry and the prop instancing so the whole level shares one
/// consistent set of room baselines, fixture pools and opening blends.
pub fn build_level_geometry_with_assets_and_lighting(
    level: &LevelDef,
    catalog: &crate::loader::PropCatalog,
    assets: &mut crate::props::PropAssets,
) -> (LevelMesh, Vec<PropMeshBatch>, LevelLighting) {
    let materials = logical_materials(level);
    build_level_geometry_with_assets_and_lighting_and_materials(level, catalog, assets, &materials)
}

/// [`build_level_geometry_with_assets_and_lighting`] with an explicitly
/// resolved material table.
///
/// The renderer uses this with the level's loaded table (including pack
/// materials and decoded images); tests and the lighting audit use the
/// catalog-only wrapper above.
pub fn build_level_geometry_with_assets_and_lighting_and_materials(
    level: &LevelDef,
    catalog: &crate::loader::PropCatalog,
    assets: &mut crate::props::PropAssets,
    materials: &MaterialTable,
) -> (LevelMesh, Vec<PropMeshBatch>, LevelLighting) {
    let (mesh, batches, lighting, _) =
        build_level_geometry_timed(level, catalog, assets, materials);
    (mesh, batches, lighting)
}

/// Stage-by-stage timings for one level build, in milliseconds.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BuildTimings {
    pub lighting_millis: f64,
    pub props_millis: f64,
    pub surfaces_millis: f64,
}

/// [`build_level_geometry_with_assets_and_lighting`], also reporting how the
/// build time splits between the lighting bake, prop instancing and static
/// surface emission.
///
/// Kept separate from the untimed entry point so the timing does not change what
/// the normal load path does.
pub fn build_level_geometry_timed(
    level: &LevelDef,
    catalog: &crate::loader::PropCatalog,
    assets: &mut crate::props::PropAssets,
    materials: &MaterialTable,
) -> (LevelMesh, Vec<PropMeshBatch>, LevelLighting, BuildTimings) {
    let started = std::time::Instant::now();
    let lighting = LevelLighting::bake(level);
    let lighting_millis = started.elapsed().as_secs_f64() * 1000.0;

    let surfaces = LevelSurfaces::new(level);
    let started = std::time::Instant::now();
    let (batches, fallbacks) = resolve_prop_instances(level, catalog, assets, &lighting, &surfaces);
    let props_millis = started.elapsed().as_secs_f64() * 1000.0;

    let started = std::time::Instant::now();
    let mesh = build_level_geometry_mesh(level, catalog, &fallbacks, &lighting, materials);
    let surfaces_millis = started.elapsed().as_secs_f64() * 1000.0;

    (
        mesh,
        batches,
        lighting,
        BuildTimings {
            lighting_millis,
            props_millis,
            surfaces_millis,
        },
    )
}

/// The shipped catalog, loaded once per process for geometry-only callers.
pub(super) fn shipped_asset_catalog() -> &'static crate::assets::AssetCatalog {
    static CATALOG: std::sync::OnceLock<crate::assets::AssetCatalog> = std::sync::OnceLock::new();
    CATALOG.get_or_init(crate::assets::AssetCatalog::load_default)
}

/// The logical material table for a level, resolved through the shipped
/// catalog with no image decoding.
///
/// Geometry only needs each material's index, tiling and tint, so the tests and
/// the lighting audit can build meshes without touching the filesystem.
#[must_use]
pub fn logical_materials(level: &LevelDef) -> MaterialTable {
    MaterialTable::logical(level, shipped_asset_catalog(), None)
}

/// Builds level geometry using only built-in prop fallbacks.
///
/// Callers that can resolve the prop catalog should prefer
/// [`build_level_geometry_with_catalog`].
#[must_use]
pub fn build_level_geometry(level: &LevelDef) -> LevelMesh {
    build_level_geometry_with_catalog(level, &crate::loader::PropCatalog::builtin())
}

/// Builds level geometry using the shipped catalog's logical materials.
#[must_use]
pub fn build_level_geometry_with_materials(
    level: &LevelDef,
    materials: &MaterialTable,
) -> LevelMesh {
    build_level_geometry_with_catalog_and_materials(
        level,
        &crate::loader::PropCatalog::builtin(),
        materials,
    )
}

/// Builds level geometry, drawing every prop as its catalogue placeholder box
/// (no GLB assets are read). Used by tests and by the asset-less fallback path.
#[must_use]
pub fn build_level_geometry_with_catalog(
    level: &LevelDef,
    catalog: &crate::loader::PropCatalog,
) -> LevelMesh {
    let materials = logical_materials(level);
    build_level_geometry_with_catalog_and_materials(level, catalog, &materials)
}

/// [`build_level_geometry_with_catalog`] with an explicitly resolved material
/// table.
#[must_use]
pub fn build_level_geometry_with_catalog_and_materials(
    level: &LevelDef,
    catalog: &crate::loader::PropCatalog,
    materials: &MaterialTable,
) -> LevelMesh {
    let lighting = LevelLighting::bake(level);
    let fallbacks: Vec<&PropDef> = level.props.iter().collect();
    build_level_geometry_mesh(level, catalog, &fallbacks, &lighting, materials)
}
